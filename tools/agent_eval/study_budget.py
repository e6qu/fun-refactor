"""A durable host-side reserve/dispatch/settle ledger shared by all agents."""
from __future__ import annotations

from contextlib import contextmanager
from decimal import Decimal, ROUND_CEILING
from pathlib import Path
import sqlite3

from .study import checked_plan, digest, number, require, text
from .provider_usage import normalize

SCALE = Decimal(1_000_000_000)


def units(value):
    if isinstance(value, Decimal):
        require(value.is_finite() and value >= 0, "invalid USD amount")
    else:
        number(value, "USD amount")
    result = int((Decimal(str(value)) * SCALE).to_integral_value(rounding=ROUND_CEILING))
    require(result <= 2**63 - 1, "USD amount exceeds ledger range")
    return result


class Budget:
    """Only a host that gates every request through this ledger can enforce its cap."""
    def __init__(self, path, frozen):
        checked_plan(frozen)
        self.path, self.frozen = Path(path), frozen
        self.cells = {cell["id"]: cell for cell in frozen["cells"]}
        self.models = {model["id"]: model for model in frozen["manifest"]["models"]}
        self.cap = units(frozen["manifest"]["spend_cap_usd"])
        self.attempt_cap = units(frozen["manifest"]["budgets"]["attempt_cap_usd"])
        self.token_cap = frozen["manifest"]["budgets"]["aggregate_tokens"]
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.transaction() as db:
            db.execute("CREATE TABLE IF NOT EXISTS binding (plan TEXT NOT NULL)")
            row = db.execute("SELECT plan FROM binding").fetchone()
            if row is None:
                db.execute("INSERT INTO binding VALUES (?)", (digest(frozen),))
            else:
                require(row[0] == digest(frozen), "budget ledger belongs to another frozen plan")
            db.execute("CREATE TABLE IF NOT EXISTS attempts (cell TEXT PRIMARY KEY, state TEXT NOT NULL)")
            db.execute("""CREATE TABLE IF NOT EXISTS calls (
                id TEXT PRIMARY KEY, cell TEXT NOT NULL, agent TEXT NOT NULL, state TEXT NOT NULL,
                reserved INTEGER NOT NULL, token_limit INTEGER NOT NULL, spent INTEGER,
                tokens INTEGER, response TEXT UNIQUE)""")
        self.path.chmod(0o600)

    @contextmanager
    def transaction(self):
        db = sqlite3.connect(self.path, timeout=5, isolation_level=None)
        try:
            db.execute("PRAGMA synchronous=FULL")
            db.execute("BEGIN IMMEDIATE")
            yield db
            db.execute("COMMIT")
        except BaseException:
            if db.in_transaction:
                db.execute("ROLLBACK")
            raise
        finally:
            db.close()

    def ready(self, db):
        require(db.execute("SELECT 1 FROM calls WHERE state IN ('unknown','overrun')").fetchone() is None,
                "unknown charge or budget overrun blocks further dispatch")

    def begin(self, cell):
        require(cell in self.cells, "unplanned cell")
        with self.transaction() as db:
            self.ready(db)
            require(db.execute("SELECT 1 FROM attempts WHERE cell=?", (cell,)).fetchone() is None,
                    "attempt already admitted; do not retry it silently")
            active = db.execute("SELECT count(*) FROM attempts WHERE state='open'").fetchone()[0]
            settled = db.execute("SELECT coalesce(sum(spent),0) FROM calls JOIN attempts USING(cell) WHERE attempts.state='closed'").fetchone()[0]
            require(settled + (active + 1) * self.attempt_cap <= self.cap, "study spend cap cannot admit another attempt")
            db.execute("INSERT INTO attempts VALUES (?, 'open')", (cell,))

    def reserve(self, cell, identity, agent, input_limit, output_limit):
        require(cell in self.cells, "unplanned cell")
        text(identity, "request id")
        text(agent, "agent id")
        number(input_limit, "input bound", integer=True)
        number(output_limit, "output bound", integer=True)
        rates = self.models[self.cells[cell]["model"]]["pricing"]["usd_per_million"]
        input_rate = max(Decimal(str(rates[key])) for key in ("uncached_input", "cache_read", "cache_write"))
        charge = (input_limit * input_rate + output_limit * Decimal(str(rates["output"]))) / Decimal(1_000_000)
        reserved = int((charge * SCALE).to_integral_value(rounding=ROUND_CEILING))
        require(reserved <= 2**63 - 1 and input_limit + output_limit <= self.token_cap, "request exceeds ledger bounds")
        with self.transaction() as db:
            self.ready(db)
            require(db.execute("SELECT state FROM attempts WHERE cell=?", (cell,)).fetchone() == ("open",), "attempt is not open")
            require(db.execute("SELECT 1 FROM calls WHERE id=?", (identity,)).fetchone() is None, "request id already reserved")
            held, tokens = db.execute("SELECT coalesce(sum(coalesce(spent,reserved)),0), coalesce(sum(coalesce(tokens,token_limit)),0) FROM calls WHERE cell=?", (cell,)).fetchone()
            require(held + reserved <= self.attempt_cap, "attempt spend cap exceeded")
            require(tokens + input_limit + output_limit <= self.token_cap, "aggregate token cap exceeded")
            db.execute("INSERT INTO calls VALUES (?,?,?,'reserved',?,?,NULL,NULL,NULL)",
                       (identity, cell, agent, reserved, input_limit + output_limit))
        return {"request": identity, "reserved_nano_usd": reserved, "token_limit": input_limit + output_limit}

    def dispatch(self, identity):
        with self.transaction() as db:
            self.ready(db)
            cursor = db.execute("UPDATE calls SET state='dispatched' WHERE id=? AND state='reserved'", (identity,))
            require(cursor.rowcount == 1, "request is not reserved or was already dispatched")

    def cancel(self, identity):
        with self.transaction() as db:
            cursor = db.execute("UPDATE calls SET state='cancelled', spent=0, tokens=0 WHERE id=? AND state='reserved'", (identity,))
            require(cursor.rowcount == 1, "only an undispatched request can be cancelled without accounting")

    def unknown(self, identity):
        with self.transaction() as db:
            cursor = db.execute("UPDATE calls SET state='unknown' WHERE id=? AND state='dispatched'", (identity,))
            require(cursor.rowcount == 1, "request was not dispatched")

    def settle(self, identity, usd, tokens, response):
        spent = units(usd)
        number(tokens, "settled tokens", integer=True)
        text(response, "provider response id")
        with self.transaction() as db:
            row = db.execute("SELECT state,reserved,token_limit FROM calls WHERE id=?", (identity,)).fetchone()
            require(row is not None and row[0] in {"dispatched", "unknown"}, "request cannot be settled twice or before dispatch")
            state = "overrun" if spent > row[1] or tokens > row[2] else "settled"
            # Preserve an actual overrun, then block subsequent requests. Never roll it back.
            db.execute("UPDATE calls SET state=?,spent=?,tokens=?,response=? WHERE id=?", (state, spent, tokens, response, identity))
        return state

    def finish(self, cell):
        with self.transaction() as db:
            require(db.execute("SELECT state FROM attempts WHERE cell=?", (cell,)).fetchone() == ("open",), "attempt is not open")
            unfinished = db.execute("SELECT 1 FROM calls WHERE cell=? AND state NOT IN ('settled','cancelled')", (cell,)).fetchone()
            require(unfinished is None, "attempt has unresolved or overrun requests")
            db.execute("UPDATE attempts SET state='closed' WHERE cell=?", (cell,))

    def settle_response(self, identity, format_name, response):
        with self.transaction() as db:
            row = db.execute("SELECT cell,state FROM calls WHERE id=?", (identity,)).fetchone()
            require(row is not None and row[1] in {"dispatched", "unknown"}, "request is not awaiting usage")
            db.execute("UPDATE calls SET state='unknown' WHERE id=?", (identity,))
        model = self.models[self.cells[row[0]]["model"]]
        provider = "anthropic" if format_name == "anthropic-message-1" else "openai"
        require(model["provider"] == provider, "response provider differs from reservation")
        observed = normalize(format_name, response, model["model"])
        if not observed["billable_complete"]:
            return "unknown"
        rates = model["pricing"]["usd_per_million"]
        usd = sum(Decimal(observed["tokens"][key]) * Decimal(str(rate))
                  for key, rate in rates.items()) / Decimal(1_000_000)
        return self.settle(identity, usd, observed["total_tokens"], observed["id"])

    def snapshot(self):
        with self.transaction() as db:
            return {"plan_sha256": digest(self.frozen), "cap_nano_usd": self.cap,
                    "attempts": [dict(zip(("cell", "state"), row)) for row in db.execute("SELECT * FROM attempts ORDER BY cell")],
                    "calls": [dict(zip(("id", "cell", "agent", "state", "reserved_nano_usd", "token_limit", "spent_nano_usd", "tokens", "response"), row))
                              for row in db.execute("SELECT * FROM calls ORDER BY id")]}
