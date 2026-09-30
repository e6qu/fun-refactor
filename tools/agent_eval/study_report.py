"""Retain every planned cell and compare matched, independently graded attempts."""
from __future__ import annotations

from collections import Counter

from .study import ARMS, artifact, checked_plan, digest, load, number, require, text
from .study_usage import measurements, usage


def attempt(record, cell, frozen, root):
    manifest = frozen["manifest"]
    require(record["schema"] == "fr-agent-study-attempt-1", "unsupported attempt schema")
    require(record["plan_sha256"] == digest(frozen) and record["cell"] == cell["id"], "attempt belongs to another plan/cell")
    status = record["status"]
    require(status in {"completed", "failed", "blocked"}, "invalid attempt status")
    if status == "blocked":
        text(record["reason"], "blocked reason")
        require(record.get("agents", []) == [] and record.get("actual_usd") is None,
                "blocked means no execution; retain launched failures as failed attempts")
        return {"cell": cell, "status": "blocked", "reason": record["reason"], "outcome": "blocked",
                "estimated_usd": 0, "actual_usd": None}
    task = next(task for task in manifest["tasks"] if task["id"] == cell["task"])
    model = next(model for model in manifest["models"] if model["id"] == cell["model"])
    require(record["cache_state"] == manifest["cache_state"], "cache state differs from plan")
    require(record["repository_revision"] == task["revision"], "task revision differs")
    require(record["requirement_sha256"] == digest(task["requirement"]), "task requirement differs")
    require(record["fr"] == manifest["fr"], "fr binary or skill differs")
    wall = number(record["wall_seconds"], "wall seconds")
    maximum = manifest["max_children"] if cell["mode"] == "delegated" else 0
    counted = usage(record["agents"], model, root, maximum, wall)
    artifact(root, record["trace"])
    outcome = record["grade"]["outcome"]
    require(outcome in {"passed", "failed", "inconclusive"}, "invalid grader outcome")
    require(status == "completed" or outcome != "passed", "failed execution cannot be a success")
    require(record["grade"]["grader_sha256"] == task["grader_sha256"], "grader differs from plan")
    artifact(root, record["grade"]["evidence"])
    for key in ("regressions", "unsupported_claims", "human_interventions"):
        number(record["grade"][key], key, integer=True)
        require(outcome != "passed" or record["grade"][key] == 0, f"success has {key}")
    missing = measurements(record["measurements"])
    actual = record["actual_usd"]
    if actual is not None:
        number(actual, "actual cost")
        artifact(root, record["billing_evidence"])
    budgets = manifest["budgets"]
    limits = {"wall_seconds": wall, "aggregate_agent_seconds": counted["agent_seconds"],
              "aggregate_tokens": sum(counted["tokens"][key] for key in ("uncached_input", "cache_read", "cache_write", "output"))
              if counted["usage_complete"] else None,
              "rss_bytes": record["measurements"]["sampled_aggregate_rss_bytes"],
              "disk_bytes": record["measurements"]["disk_growth_bytes"],
              "attempt_cap_usd": actual if actual is not None else counted["estimated_usd"]}
    exceeded = [key for key, value in limits.items() if value is not None and value > budgets[key]]
    return {"cell": cell, "status": status, "outcome": outcome, **counted, "wall_seconds": wall,
            "actual_usd": actual, "grade": record["grade"], "measurements": record["measurements"],
            "missing_measurements": missing, "budget_exceeded": exceeded,
            "unmeasured_budgets": [key for key, value in limits.items() if value is None]}


def total(rows, key):
    values = [row.get(key) for row in rows]
    return sum(values) if all(value is not None for value in values) else None


def report(frozen, directory):
    checked_plan(frozen)
    require(directory.is_dir(), "attempt directory does not exist")
    expected = {cell["id"]: cell for cell in frozen["cells"]}
    paths = sorted(directory.glob("*.json"))
    require(all(path.stem in expected for path in paths), "unplanned attempt file; do not hide retries")
    rows, identities, invocations = [], set(), set()
    for cell in frozen["cells"]:
        path = directory / f'{cell["id"]}.json'
        if path.exists():
            # Artifact references are relative to this shared evidence directory.
            record = load(path)
            rows.append(attempt(record, cell, frozen, directory))
            for agent in record.get("agents", []):
                identity = (agent["provider"], agent["harness"], agent["id"])
                require(identity not in identities, "agent session reused across attempts")
                identities.add(identity)
                for invocation in agent["invocations"]:
                    identity = (agent["provider"], invocation["id"])
                    require(identity not in invocations, "provider invocation reused across attempts")
                    invocations.add(identity)
        else:
            rows.append({"cell": cell, "status": "pending", "outcome": "pending"})
    groups, paired = {}, {}
    for row in rows:
        cell = row["cell"]
        key = (cell["model"], cell["mode"], cell["arm"])
        groups.setdefault(key, []).append(row)
        paired.setdefault(cell["pair"], {})[cell["arm"]] = row
    summaries = []
    for (model, mode, arm), members in sorted(groups.items()):
        executed = [row for row in members if row["status"] in {"completed", "failed"}]
        successes = sum(row["outcome"] == "passed" for row in executed)
        estimated, actual = total(executed, "estimated_usd"), total(executed, "actual_usd")
        summaries.append({"model": model, "mode": mode, "arm": arm, "planned": len(members),
                          "outcomes": dict(Counter(row["outcome"] for row in members)), "executed": len(executed),
                          "estimated_usd": estimated, "actual_usd": actual,
                          "estimated_usd_per_success": estimated / successes if successes and estimated is not None else None,
                          "actual_usd_per_success": actual / successes if successes and actual is not None else None})
    comparisons = []
    for pair, arms in sorted(paired.items()):
        reasons = []
        for arm in ARMS:
            row = arms[arm]
            if row["outcome"] != "passed":
                reasons.append(f"{arm}: {row['outcome']}")
            if row.get("usage_complete") is not True:
                reasons.append(f"{arm}: incomplete usage")
            if row.get("budget_exceeded"):
                reasons.append(f"{arm}: budget exceeded")
            if row.get("unmeasured_budgets"):
                reasons.append(f"{arm}: unmeasured budgets")
        comparable = not reasons
        comparisons.append({"pair": pair, "model": arms["fr"]["cell"]["model"],
                            "mode": arms["fr"]["cell"]["mode"], "task": arms["fr"]["cell"]["task"],
                            "comparable_success": comparable, "excluded_reasons": reasons,
                            "estimated_fr_minus_files_usd": arms["fr"]["estimated_usd"] - arms["files"]["estimated_usd"] if comparable else None})
    executed = [row for row in rows if row["status"] in {"completed", "failed"}]
    spend = [row["actual_usd"] if row["actual_usd"] is not None else row["estimated_usd"] for row in executed]
    complete = all(value is not None for value in spend)
    known = sum(value for value in spend if value is not None)
    cap = frozen["manifest"]["spend_cap_usd"]
    audit_complete = all(row["status"] in {"completed", "failed"} and row.get("usage_complete")
                         and not row.get("missing_measurements") and not row.get("budget_exceeded")
                         and row["outcome"] != "inconclusive" for row in rows) and known <= cap
    return {"schema": "fr-agent-study-report-1", "plan_sha256": digest(frozen),
            "audit_complete": audit_complete,
            "planned": len(rows), "executed": len(executed),
            "outcomes": dict(Counter(row["outcome"] for row in rows)), "groups": summaries,
            "pairs": comparisons, "attempts": rows,
            "spend": {"known_usd": known, "complete": complete, "basis": "actual where supplied, otherwise estimated",
                      "cap_usd": cap, "cap_exceeded": known > cap,
                      "remaining_usd": max(0, cap - known) if complete else None},
            "scope": "Host-supplied usage and independent grades with pinned artifacts; no population efficiency claim."}
