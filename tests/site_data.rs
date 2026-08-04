//! Generates the data behind `docs/catalog.html` and `docs/translate.html`.
//!
//! Every before, after and diff on those pages is produced here by running the real
//! `fr` binary over the sample files below, in a temporary directory, exactly as the
//! command printed beside it would. Nothing on either page is typed by hand, because a
//! hand-typed "after" is a claim about the tool rather than a demonstration of it, and
//! it goes stale the first time the tool improves.
//!
//! The generated files are committed, and this test fails when they no longer match
//! what the tool produces. Regenerate with:
//!
//! ```sh
//! UPDATE_SITE_DATA=1 cargo test --test site_data
//! ```
//!
//! # On the catalogue
//!
//! The refactoring *names* — Extract Function, Guard Clauses, Slide Statements — come
//! from Martin Fowler's and Kent Beck's catalogues, and each entry says which book and
//! which edition. The **code is not theirs**: every sample below is written for this
//! page, in Python, to exercise the move the catalogue describes. Their examples are
//! copyrighted and translating one into Python would be a derivative of it.

use std::path::{Path, PathBuf};
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

// ---------------------------------------------------------------- the samples

/// What an entry demonstrates.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    /// The command changes the file; the page shows before, after and the diff.
    Edit,
    /// The command finds work without doing it; the page shows the file and the report.
    Report,
    /// The command declines, and the reason is the point.
    Refused,
}

/// A sample workspace, the command to run against it, and where it came from.
struct Entry {
    kind: Kind,
    id: &'static str,
    /// The catalogue name.
    name: &'static str,
    /// Which book, and which edition, for each catalogue that has this move.
    sources: &'static [&'static str],
    /// What the move is for, in one sentence.
    intent: &'static str,
    /// What this particular sample shows, beyond the move itself.
    note: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The `fr` invocation, with `@from…to@` standing for a range this test computes
    /// from the source rather than a line and column somebody counted by hand.
    argv: &'static [&'static str],
    /// Which file the page shows before and after.
    subject: &'static str,
}

const BILLING: &str = r#"def print_invoice(invoice):
    outstanding = 0.0
    for order in invoice.orders:
        outstanding += order.amount

    print("***********************")
    print("**** Customer Owes ****")
    print("***********************")

    print(f"name: {invoice.customer}")
    print(f"amount: {outstanding}")
"#;

const PRICING: &str = r#"def price_of(order):
    return order.quantity * order.item_price - max(0, order.quantity - 500) * 0.05


def is_expensive(order):
    total = price_of(order)
    return total > 1000
"#;

const DELIVERY: &str = r#"def rating(driver):
    return 2 if more_than_five_late_deliveries(driver) else 1


def more_than_five_late_deliveries(driver):
    return driver.late_deliveries > 5
"#;

const GEOMETRY: &str = r#"import math


def circ(r):
    """The distance around a circle."""
    return 2 * math.pi * r


def band_width(inner, outer):
    return circ(outer) - circ(inner)
"#;

const PAYOUT: &str = r#"def payout(employee):
    if employee.is_separated:
        result = 0
    else:
        if employee.is_retired:
            result = 0
        else:
            result = employee.salary * employee.bonus_rate
    return result
"#;

const NOTIFY: &str = r#"def notify(subscriber, digest):
    if subscriber.is_active:
        if subscriber.wants_email:
            send_email(subscriber.address, digest)
"#;

const ALERTS: &str = r#"def should_alert(reading, limits):
    if not (reading.celsius < limits.high and reading.humidity < limits.wet):
        return True
    return False
"#;

const ACCOUNT: &str = r#"import datetime


def days_overdue(account):
    return (datetime.date.today() - account.due_on).days


def overdue_charge(account):
    return days_overdue(account) * account.daily_rate
"#;

const RATES: &str = r#"def daily_rate(account):
    return account.balance * 0.0004
"#;

const REPORTS: &str = r#"def summarise(rows):
    return len(rows)


def _legacy_histogram(rows):
    buckets = {}
    for row in rows:
        buckets[row.kind] = buckets.get(row.kind, 0) + 1
    return buckets
"#;

const TELEMETRY: &str = r#"def record(event):
    send_metric(event.name, event.value)


def flush(events):
    for event in events:
        send_metric(event.name, event.value)
"#;

const CHECKOUT: &str = r#"NEW_CHECKOUT = True


def total(basket):
    if NEW_CHECKOUT:
        return sum(line.price for line in basket.lines)
    else:
        running = 0
        for line in basket.lines:
            running = running + line.price
        return running
"#;

const SCOPES_PY: &str = r#"import math


def band(inner, outer):
    width = outer - inner
    return width * 2


def area(radius):
    width = radius * 2
    return math.pi * width * width
"#;

const CONNECT_PY: &str = r#"def send(host, port):
    return connect(host, port)


def main():
    return send("example.com", 443)
"#;

const DEFAULTS_PY: &str = r#"import math


def circ(r, units="m"):
    """The distance around a circle."""
    return f"{2 * math.pi * r}{units}"


def rim(r):
    return circ(r)
"#;

const REPEATED_PY: &str = r#"def total(order):
    base = order.quantity * order.item_price
    discount = order.quantity * order.item_price * 0.05
    return base - discount
"#;

const UNSORTED_PY: &str = r#"import os
import json
import sys


def load(path):
    with open(path) as handle:
        return json.load(handle)
"#;

const APP_CSS: &str = r#".nav-link {
  color: red;
}

.footer {
  color: blue;
}
"#;

const PAGE_HTML: &str = r#"<nav><a class="nav-link" href="/">Home</a></nav>
<footer class="footer">bye</footer>
"#;

const LIVE_PY: &str = r#"def helper():
    return 1


def entry():
    return helper()
"#;

const TDD_CYCLE: &str = r#"def convert_usd(bank, amount):
    rate = bank.rate("USD", "CHF")
    converted = amount * rate
    if converted < 0:
        raise ValueError("negative")
    return round(converted, 2)


def convert_gbp(exchange, sum_):
    ratio = exchange.rate("GBP", "CHF")
    result = sum_ * ratio
    if result < 0:
        raise ValueError("negative")
    return round(result, 2)
"#;

const ENTRIES: &[Entry] = &[
    Entry {
        kind: Kind::Edit,
        id: "extract-function",
        name: "Extract Function",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.1",
            "Tidy First? — Kent Beck (2023), “Extract Helper”",
        ],
        intent: "A fragment of code that can be grouped together gets its own name.",
        note: "The extracted statements read two variables defined above them, so the new \
               function takes both as parameters and the call passes them. Nothing here \
               was told what `invoice` is, or what an `outstanding` is; the parameters \
               come from which names the fragment uses and where they were bound.",
        files: &[("src/billing.py", BILLING)],
        argv: &[
            "extract",
            "@src/billing.py|    print(f\"name: {invoice.customer}\")|    print(f\"amount: {outstanding}\")@",
            "print_details",
            "--function",
        ],
        subject: "src/billing.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "extract-variable",
        name: "Extract Variable",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.3",
            "Tidy First? — Kent Beck (2023), “Explaining Variables”",
        ],
        intent: "A sub-expression that is hard to read gets a name that says what it is.",
        note: "The name is the whole point of the move: the expression is unchanged and \
               the code is no shorter.",
        files: &[("src/pricing.py", PRICING)],
        argv: &[
            "extract",
            "@src/pricing.py~order.quantity * order.item_price~@",
            "base_price",
        ],
        subject: "src/pricing.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "inline-variable",
        name: "Inline Variable",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.4",
            "Tidy First? — Kent Beck (2023), “One Pile”",
        ],
        intent: "A name that says no more than the expression it stands for is removed.",
        note: "The reverse of Extract Variable, and the reason both are in the catalogue: \
               which one is an improvement depends on whether the name earns its keep.",
        files: &[("src/pricing.py", PRICING)],
        argv: &["inline", "src/pricing.py:6:5"],
        subject: "src/pricing.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "inline-function",
        name: "Inline Function",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §6.2"],
        intent: "A function whose body is as clear as its name is replaced by its body.",
        note: "The call is replaced with the callee's body, with the arguments substituted \
               for the parameters.",
        files: &[("src/delivery.py", DELIVERY)],
        argv: &["inline", "src/delivery.py:2:17", "--call"],
        subject: "src/delivery.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "change-function-declaration",
        name: "Change Function Declaration",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.5",
            "Tidy First? — Kent Beck (2023), “Explicit Parameters”",
        ],
        intent: "A function's parameters change, and every call site changes with them.",
        note: "The call inside `band_width` is updated twice, because it is called twice. \
               A call the tool could not resolve would be reported rather than rewritten.",
        files: &[("src/geometry.py", GEOMETRY)],
        argv: &["signature", "circ", "add:1:units: str:\"m\""],
        subject: "src/geometry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "rename-function",
        name: "Rename Function",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.7 (Rename Variable), §6.5",
            "Implementation Patterns — Kent Beck (2007), “Intention-Revealing Name”",
        ],
        intent: "A name that does not say what the thing is becomes one that does.",
        note: "`circ` appears twice inside `band_width` and once in the docstring. The \
               docstring mention is reported, not rewritten — a name in prose is not a \
               reference.",
        files: &[("src/geometry.py", GEOMETRY)],
        argv: &["rename", "circ", "circumference"],
        subject: "src/geometry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "rename-variable",
        name: "Rename Variable",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.7",
            "Implementation Patterns — Kent Beck (2007), “Intention-Revealing Name”",
        ],
        intent: "A local whose name says nothing becomes one that says what it holds.",
        note: "There are two variables called `width` in this file and only one of them \
               is being renamed. Nothing here matches on the text `width`: the rename \
               follows the lexical scope, so the one in `area` is a different binding \
               and is left alone. A find-and-replace gets this wrong every time.",
        files: &[("src/geometry.py", SCOPES_PY)],
        argv: &["rename", "src/geometry.py:5:5", "span"],
        subject: "src/geometry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "rename-across-languages",
        name: "Rename, across a language boundary",
        sources: &["Not in either catalogue — the catalogues predate the problem"],
        intent: "A CSS class is renamed in the stylesheet and everywhere the markup names it.",
        note: "Two files, two grammars, one name. The stylesheet declares `.nav-link` and \
               the HTML reaches it through a `class` attribute — no import, no path, \
               nothing a compiler would check. The catalogues are about one language at \
               a time, and most of a web codebase is not.",
        files: &[("web/app.css", APP_CSS), ("web/page.html", PAGE_HTML)],
        argv: &["rename", "nav-link", "primary-link"],
        subject: "web/page.html",
    },
    Entry {
        kind: Kind::Edit,
        id: "remove-parameter",
        name: "Remove Parameter",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.5 (Change Function Declaration)",
            "The online refactoring catalogue — Martin Fowler",
        ],
        intent: "A parameter nobody needs goes, and every call site loses its argument.",
        note: "The declaration and the calls change together or not at all. A call the \
               tool could not resolve would be reported rather than left quietly \
               passing an argument to a parameter that no longer exists.",
        files: &[("src/geometry.py", DEFAULTS_PY)],
        argv: &["signature", "circ", "remove:1"],
        subject: "src/geometry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "reorder-parameters",
        name: "Reorder Parameters",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.5 (Change Function Declaration)",
            "The online refactoring catalogue — Martin Fowler",
        ],
        intent: "Two parameters swap, and so do the arguments at every call.",
        note: "The arguments move with the parameters. Getting one of the two halves \
               right is worse than doing nothing, which is why this is a refactoring \
               rather than two edits.",
        files: &[("src/net.py", CONNECT_PY)],
        argv: &["signature", "send", "move:0:1"],
        subject: "src/net.py",
    },
    Entry {
        kind: Kind::Refused,
        id: "reorder-parameters-refused",
        name: "…and the reorder that would not run",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §6.5"],
        intent: "The same move, where the language will not have it.",
        note: "Python requires every defaulted parameter to come last, so this would \
               produce `def circ(units=\"m\", r):` — which Python rejects outright. The \
               engine reparses every edit and would normally catch a broken result, but \
               tree-sitter parses this without complaint, so the refactoring has to \
               know the rule itself. It did not until this page was written.",
        files: &[("src/geometry.py", DEFAULTS_PY)],
        argv: &["signature", "circ", "move:0:1"],
        subject: "src/geometry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "extract-variable-everywhere",
        name: "Extract Variable, at every occurrence",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §6.3",
            "Tidy First? — Kent Beck (2023), “Explaining Variables”",
        ],
        intent: "One name for a repeated sub-expression, substituted everywhere it appears.",
        note: "Fowler's mechanics say to replace *all* occurrences, and the second one \
               here is inside a larger expression rather than alone on a line — which is \
               why this matches on the parse tree rather than on the text.",
        files: &[("src/pricing.py", REPEATED_PY)],
        argv: &[
            "extract",
            "@src/pricing.py~order.quantity * order.item_price~@",
            "gross",
            "--all",
        ],
        subject: "src/pricing.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "organize-imports",
        name: "Remove unused imports",
        sources: &["Not in either catalogue — but it is the tidying you do after the others"],
        intent: "Imports nothing uses are dropped and the rest are sorted.",
        note: "Read what it prints above the diff. Liveness is decided by name, so an \
               import kept for a trait, a registration side effect or a doc comment \
               would look unused — and it says so rather than letting you find out. \
               This is the step a recipe puts last, with `imports where changed`.",
        files: &[("src/loader.py", UNSORTED_PY)],
        argv: &["imports", "src/loader.py"],
        subject: "src/loader.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "move-function",
        name: "Move Function",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §8.1",
            "Tidy First? — Kent Beck (2023), “Cohesion Order”",
        ],
        intent: "A function moves to the module it belongs with, and its callers follow.",
        note: "The import appears in the file it left, because the function that stayed \
               behind still calls it.",
        files: &[("src/account.py", ACCOUNT), ("src/rates.py", RATES)],
        argv: &["move", "days_overdue", "src/rates.py"],
        subject: "src/account.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "remove-dead-code",
        name: "Remove Dead Code",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §8.9",
            "Tidy First? — Kent Beck (2023), “Dead Code”",
        ],
        intent: "Code nothing calls is deleted rather than maintained.",
        note: "`fr unused` finds it and `fr delete` removes it, and the two must agree: \
               delete refuses anything still referenced, which is what makes the list \
               worth acting on.",
        files: &[("src/reports.py", REPORTS)],
        argv: &["delete", "_legacy_histogram"],
        subject: "src/reports.py",
    },
    Entry {
        kind: Kind::Refused,
        id: "delete-refused",
        name: "…and the one it will not delete",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §8.9"],
        intent: "Deleting something that is still reached is not dead-code removal.",
        note: "The boundary that makes `fr unused` worth acting on: whatever the list \
               says, `delete` checks again and refuses anything still referenced. The \
               two halves have to agree, and running them against each other over a \
               nine-language workspace is how thirteen disagreements in fifty-nine were \
               found.",
        files: &[("src/live.py", LIVE_PY)],
        argv: &["delete", "helper"],
        subject: "src/live.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "guard-clauses",
        name: "Replace Nested Conditional with Guard Clauses",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §10.3",
            "Tidy First? — Kent Beck (2023), “Guard Clauses”",
        ],
        intent: "The special cases leave early, so the normal path is not indented.",
        note: "Beck's version is the one this does: a single guard at a time, taken off \
               the front. Run it again on what is left and the second guard comes off too.",
        files: &[("src/notify.py", NOTIFY)],
        argv: &["rewrite", "src/notify.py:2:5", "guard-clause"],
        subject: "src/notify.py",
    },
    Entry {
        kind: Kind::Refused,
        id: "guard-clauses-refused",
        name: "…and where it stops",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §10.3"],
        intent: "Fowler's own example is an `if`/`else` nest that assigns to a result.",
        note: "The tool will not do this one, and says why rather than guessing. Turning \
               an `else` into an early return means deciding what the function returns on \
               the path that used to fall through — a judgement about the code, not a \
               fact about its syntax. `invert-if` is the move it offers instead, and the \
               entry below is that same file.",
        files: &[("src/payout.py", PAYOUT)],
        argv: &["rewrite", "src/payout.py:2:5", "guard-clause"],
        subject: "src/payout.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "reverse-conditional",
        name: "Reverse Conditional",
        sources: &["The online refactoring catalogue — Martin Fowler"],
        intent: "A condition is negated and its branches swapped, when that reads better.",
        note: "Purely local: the tool does not need to resolve a single name to know this \
               is sound, which is why it is offered at a position rather than for a symbol.",
        files: &[("src/payout.py", PAYOUT)],
        argv: &["rewrite", "src/payout.py:2:5", "invert-if"],
        subject: "src/payout.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "de-morgan",
        name: "Push a negation through a conjunction (De Morgan)",
        sources: &[
            "Not in either catalogue — De Morgan's law, 1847",
            "Tidy First? — Kent Beck (2023), “Normalize Symmetries”, in spirit",
        ],
        intent: "A negated conjunction becomes a disjunction of negations, or the reverse.",
        note: "Named honestly: this is not Fowler's Consolidate Conditional Expression \
               (§10.2), which combines several conditionals that produce the same result. \
               It is a law of logic applied by the grammar rather than by eye. The two \
               forms mean the same thing and one of them is usually the one you meant.",
        files: &[("src/alerts.py", ALERTS)],
        argv: &["rewrite", "src/alerts.py:2:12", "de-morgan"],
        subject: "src/alerts.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "substitute-algorithm",
        name: "Substitute Algorithm",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §7.9"],
        intent: "Every occurrence of one shape of code becomes another shape.",
        note: "`$X` matches any node and substitutes back. This is the move to reach for \
               when an API changes under you and the change is mechanical.",
        files: &[("src/telemetry.py", TELEMETRY)],
        argv: &[
            "restructure",
            "send_metric($NAME, $VALUE)",
            "emit(name=$NAME, value=$VALUE)",
            "--lang",
            "python",
        ],
        subject: "src/telemetry.py",
    },
    Entry {
        kind: Kind::Edit,
        id: "remove-flag-argument",
        name: "Remove Flag Argument",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §11.3",
            "Feature Toggles — Martin Fowler & Pete Hodgson (2017)",
        ],
        intent: "A flag that is now always one value is removed, and the branch it chose with it.",
        note: "The dead branch goes with the flag. This is the one move on the page that \
               cascades: it keeps going until nothing else falls out.",
        files: &[("src/checkout.py", CHECKOUT)],
        argv: &["remove-flag", "NEW_CHECKOUT", "--value", "true"],
        subject: "src/checkout.py",
    },
    Entry {
        kind: Kind::Report,
        id: "tdd-refactor-step",
        name: "The refactor step of red–green–refactor",
        sources: &["Test-Driven Development by Example — Kent Beck (2002)"],
        intent: "Once the test passes, the duplication that made it pass is removed.",
        note: "The cycle ends with a refactoring, and the refactoring starts with seeing \
               the duplication. These two functions share not one identifier — `bank` and \
               `exchange`, `rate` and `ratio`, `converted` and `result` — and they are the \
               same code. Structure is compared, not text, which is the copy a textual \
               search never finds. What to do about it is yours; the moves above are the menu.",
        files: &[("src/exchange.py", TDD_CYCLE)],
        argv: &["duplicates", "--min-tokens", "40"],
        subject: "src/exchange.py",
    },
];

// ----------------------------------------------------------- the translations

/// One before-and-after of a whole file written as another language.
struct Translation {
    id: &'static str,
    title: &'static str,
    blurb: &'static str,
    files: &'static [(&'static str, &'static str)],
    subject: &'static str,
    target: &'static str,
    /// Where the sample came from, when it is not written for this page.
    provenance: Option<&'static str>,
    /// A directory under `tests/corpus/` to copy in whole, for a translation whose
    /// input is a *tree* rather than a file — a Next.js route's URL is its path.
    corpus: Option<&'static str>,
}

const TYPED_PYTHON: &str = r#"from dataclasses import dataclass
from typing import Optional


MAX_RETRY_COUNT = 3


@dataclass
class SensorReading:
    sensor_id: str
    celsius: float
    recorded_by: Optional[str]


def readings_above(readings: list[SensorReading], limit: float) -> list[SensorReading]:
    """Every reading warmer than the limit."""
    return [reading for reading in readings if reading.celsius > limit]


def label_for(reading: SensorReading, units: dict[str, str]) -> str:
    suffix = units[reading.sensor_id]
    return f"{reading.sensor_id}: {reading.celsius}{suffix}"


def load_reading(source: str) -> Optional[SensorReading]:
    try:
        raw = fetch(source)
        return parse_reading(raw)
    except ValueError as error:
        report(error)
        return None
"#;

const TYPED_TYPESCRIPT: &str = r#"export const MAX_RETRY_COUNT = 3;

export interface SensorReading {
  sensorId: string;
  celsius: number;
  recordedBy: string | null;
}

/** Every reading warmer than the limit. */
export function readingsAbove(readings: SensorReading[], limit: number): SensorReading[] {
  return readings.filter((reading) => reading.celsius > limit);
}

export function labelFor(reading: SensorReading, units: Record<string, string>): string {
  const suffix = units[reading.sensorId];
  return `${reading.sensorId}: ${reading.celsius}${suffix}`;
}

export async function loadReading(source: string): Promise<SensorReading | null> {
  try {
    const raw = await fetch(source);
    return parseReading(raw);
  } catch (error) {
    report(error);
    return null;
  }
}
"#;

const TYPED_GO: &str = r#"package store

// Reading is one sample from a sensor.
type Reading struct {
	SensorID string
	Celsius  float64
}

// Warmer counts the readings above a limit.
func Warmer(readings []Reading, limit float64) int {
	count := 0
	for _, reading := range readings {
		if reading.Celsius > limit {
			count = count + 1
		}
	}
	return count
}
"#;

const TYPED_RUST: &str = r#"/// One sample from a sensor.
pub struct Reading {
    pub sensor_id: String,
    pub celsius: f64,
}

/// Count the readings above a limit.
pub fn warmer(readings: Vec<Reading>, limit: f64) -> i64 {
    let mut count = 0;
    for reading in readings {
        if reading.celsius > limit {
            count = count + 1;
        }
    }
    return count;
}
"#;

const TRANSLATIONS: &[Translation] = &[
    Translation {
        id: "python-to-typescript",
        title: "Typed Python → TypeScript",
        blurb: "The signature is the contract. Every parameter, in order, with its type \
                and the return type — only the spelling changes, to the target's \
                convention.",
        files: &[("sensors.py", TYPED_PYTHON)],
        subject: "sensors.py",
        target: "typescript",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "typescript-to-python",
        title: "Typed TypeScript → Python",
        blurb: "The same crossing in the other direction, over the file the first one \
                produced by hand-writing what a TypeScript author would have written.",
        files: &[("sensors.ts", TYPED_TYPESCRIPT)],
        subject: "sensors.ts",
        target: "python",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "go-to-rust",
        title: "Go → Rust",
        blurb: "The other pair. A Go `struct` is a Rust `struct`, a `range` loop is a \
                `for … in`, and the exported/unexported convention becomes `pub`.",
        files: &[("store.go", TYPED_GO)],
        subject: "store.go",
        target: "rust",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "rust-to-go",
        title: "Rust → Go",
        blurb: "And back, where Go's capital letter is what `pub` means and the \
                signature carries unchanged.",
        files: &[("store.rs", TYPED_RUST)],
        subject: "store.rs",
        target: "go",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "nextjs-to-fastapi",
        title: "A Next.js API route → FastAPI",
        blurb: "Not a language translation — a *contract* one. The URL, the method and \
                the path parameter come from where the file sits on disk, which is the \
                one thing no content-only translation could recover.",
        files: &[],
        subject: "app/api/posts/[postId]/route.ts",
        target: "fastapi",
        provenance: Some(
            "shadcn-ui/taxonomy @ 298a8857c7128a0d121e7f699dfd729f23b3966d, MIT. \
             See tests/corpus/PROVENANCE.md.",
        ),
        corpus: Some("nextjs"),
    },
    Translation {
        id: "real-python-to-typescript",
        title: "Real code: a FastAPI backend → TypeScript",
        blurb: "Not a sample written to succeed. This is `backend/app/crud.py` from the \
                full-stack FastAPI template, unmodified, and the report says exactly \
                where the translation stops being one.",
        files: &[],
        subject: "crud.py",
        target: "typescript",
        provenance: Some(
            "fastapi/full-stack-fastapi-template @ 750d3d0bc6dfece4dec2d6ef8c3ff7e64f72545d, \
             MIT. See tests/corpus/PROVENANCE.md.",
        ),
        corpus: None,
    },
];

// --------------------------------------------------------------- the recipes

/// One recipe, the workspace it runs over, and what it is teaching.
struct Lesson {
    id: &'static str,
    title: &'static str,
    language: &'static str,
    teaches: &'static str,
    note: &'static str,
    files: &'static [(&'static str, &'static str)],
    recipe: &'static str,
    /// The file the page shows before and after.
    subject: &'static str,
}

const LESSONS: &[Lesson] = &[
    Lesson {
        id: "python-retire-a-flag",
        title: "Retire a feature flag, and what it was guarding",
        language: "python",
        teaches: "Steps run in order and each sees what the last one left.",
        note: "This is the whole reason a recipe is more than a shell loop. Nothing is \
               `unused` until the flag has gone, so the second step can only match \
               because the first one already ran and the index was rebuilt from its \
               result. Written as two commands, the second would find nothing.",
        files: &[("src/auth.py", AUTH_PY)],
        recipe: r#"schema 1

recipe retire-legacy-auth {
  description "The legacy auth path has been dark for a year."

  requires symbol "USE_LEGACY_AUTH"

  remove-flag "USE_LEGACY_AUTH" = false

  delete where kind=function
               name~"legacy_auth_*"
               unused

  expect changed > 0 files
  expect refusals = 0
}
"#,
        subject: "src/auth.py",
    },
    Lesson {
        id: "typescript-rename-and-tidy",
        title: "Rename across files, then tidy what that left",
        language: "typescript",
        teaches: "`where changed` selects the files this run has already touched.",
        note: "The second step does not name a file. It asks for the ones the first \
               step moved, which is a question only the run itself can answer — and the \
               reason `changed` is a predicate rather than a path you have to keep in \
               your head.",
        files: &[("src/parse.ts", PARSE_TS), ("src/main.ts", MAIN_TS)],
        recipe: r#"schema 1

recipe intention-revealing-name {
  description "`p` says nothing; `parseUri` says what it is."

  requires language typescript

  rename to "parseUri" where name="p" kind=function

  imports where changed

  expect refusals = 0
}
"#,
        subject: "src/main.ts",
    },
    Lesson {
        id: "java-rename-a-method",
        title: "Rename a method the whole package calls",
        language: "java",
        teaches: "A refusal is reported, and `on-refusal` decides what it costs.",
        note: "Java reaches a method through a receiver, and this tool does not track \
               the receiver's type — so the call in `Report.java` resolves only as \
               `field-based`, and the rename rewrites the declaration and *says* it \
               left that one alone. Read the `left` line: this is the tool telling the \
               truth about what it knows, and a recipe that swallowed it would report a \
               clean run over work still to do.",
        files: &[
            ("com/example/Account.java", ACCOUNT_JAVA),
            ("com/example/Report.java", REPORT_JAVA),
        ],
        recipe: r#"schema 1

recipe clearer-method-name {
  description "`overdueDays` reads as a noun; `daysOverdue` reads as what it returns."

  requires language java

  rename to "daysOverdue" where name="overdueDays" kind=method
                           on-refusal report
}
"#,
        subject: "com/example/Account.java",
    },
    Lesson {
        id: "go-guard-clauses",
        title: "Guard clauses, everywhere, but only ten of them",
        language: "go",
        teaches: "`limit` and the dry run, on the most dangerous step in the language.",
        note: "`rewrite` has no target in the usual sense: the selector chooses *files* \
               and the transformation applies everywhere in them that it applies. This \
               is the step that most needs a limit — `guard-clause` was once wrong at \
               1,258 of 1,498 sites in helm/helm — and `limit` takes the same sites \
               every run, so what you reviewed is what you get.",
        files: &[("pkg/services/notify.go", NOTIFY_GO)],
        recipe: r#"schema 1

recipe unnest-the-services {
  description "Turn wrapping ifs into early returns, a few at a time."

  rewrite guard-clause where lang=go in="pkg/services/"
                        limit 10

  expect refusals = 0
}
"#,
        subject: "pkg/services/notify.go",
    },
    Lesson {
        id: "rust-api-migration",
        title: "An API changed under you",
        language: "rust",
        teaches:
            "`restructure` rewrites a shape, and `expect no-new unused` checks what that orphaned.",
        note: "The pattern *is* the selector, which is why `restructure` takes no \
               `where` clause — a second way of choosing would contradict the first. \
               The expectation is the interesting half: replacing every call to a \
               helper leaves the helper with no callers, and a refactoring that orphans \
               a function has not finished.",
        files: &[("src/metrics.rs", METRICS_RS)],
        recipe: r#"schema 1

recipe emit-instead-of-record {
  description "record() became emit() with a named unit."

  requires language rust

  restructure rust 'record($NAME, $VALUE)' => 'emit($NAME, $VALUE, Unit::Count)'

  expect no-new unused
}
"#,
        subject: "src/metrics.rs",
    },
];

const AUTH_PY: &str = r#"USE_LEGACY_AUTH = False


def authenticate(user, token):
    if USE_LEGACY_AUTH:
        return legacy_auth_check(user, token)
    return modern_auth_check(user, token)


def legacy_auth_check(user, token):
    return token == user.legacy_token


def legacy_auth_header(request):
    return request.headers.get("X-Legacy-Auth")


def modern_auth_check(user, token):
    return user.verify(token)
"#;

const PARSE_TS: &str = r#"export function p(raw: string): string {
  return raw.trim().toLowerCase();
}
"#;

const MAIN_TS: &str = r#"import { p } from "./parse";
import { unused } from "./nowhere";

export function main(argument: string): string {
  return p(argument);
}
"#;

const ACCOUNT_JAVA: &str = r#"package com.example;

import java.util.List;

public class Account {
    private final String owner;

    public Account(String owner) {
        this.owner = owner;
    }

    public int overdueDays(List<Charge> charges) {
        int total = 0;
        for (Charge charge : charges) {
            total += charge.days();
        }
        return total;
    }
}
"#;

const REPORT_JAVA: &str = r#"package com.example;

import java.util.List;

public class Report {
    public String summarise(Account account, List<Charge> charges) {
        int days = account.overdueDays(charges);
        return "overdue " + days;
    }
}
"#;

const NOTIFY_GO: &str = r#"package services

func Notify(subscriber Subscriber, digest Digest) {
	if subscriber.Active {
		if subscriber.WantsEmail {
			send(subscriber.Address, digest)
		}
	}
}

func Archive(subscriber Subscriber, digest Digest) {
	if subscriber.Archiving {
		store(subscriber.Address, digest)
	}
}
"#;

const METRICS_RS: &str = r#"pub fn record(name: &str, value: i64) {
    println!("{name}={value}");
}

pub fn on_request(path: &str) {
    record("requests", 1);
}

pub fn on_error(kind: &str) {
    record("errors", 1);
}
"#;

// ------------------------------------------------------------------- the runner

/// Copy a vendored corpus tree into a temporary workspace.
fn corpus(subdirectory: &str) -> (tempfile::TempDir, PathBuf) {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(subdirectory);
    let tmp = tempfile::tempdir().unwrap();
    copy(&from, tmp.path());
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    tmp
}

/// Resolve a range written as the text it selects.
///
/// `@path|first line|last line@` for a run of whole statements, and `@path~snippet~@`
/// for a sub-expression. Written as the text rather than as four numbers, because four
/// numbers in a test are four numbers somebody counted, and they are wrong as soon as a
/// sample gains a line.
fn resolve(argument: &str, root: &Path) -> String {
    let Some(inner) = argument.strip_prefix('@').and_then(|a| a.strip_suffix('@')) else {
        return argument.to_string();
    };

    if let Some((path, rest)) = inner.split_once('~') {
        let snippet = rest.strip_suffix('~').expect("a closing ~");
        let source = std::fs::read_to_string(root.join(path)).expect("reading the sample");
        let at = source
            .find(snippet)
            .unwrap_or_else(|| panic!("no {snippet:?} in {path}"));
        let line = source[..at].matches('\n').count() + 1;
        let column = at - source[..at].rfind('\n').map(|n| n + 1).unwrap_or(0) + 1;
        return format!(
            "{path}:{line}:{column}-{line}:{}",
            column + snippet.chars().count()
        );
    }

    let mut parts = inner.split('|');
    let path = parts.next().expect("a path");
    let first = parts.next().expect("the first line's text");
    let last = parts.next().expect("the last line's text");

    let source = std::fs::read_to_string(root.join(path)).expect("reading the sample");
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|line| *line == first)
        .unwrap_or_else(|| panic!("no line {first:?} in {path}"));
    let end = lines
        .iter()
        .rposition(|line| *line == last)
        .unwrap_or_else(|| panic!("no line {last:?} in {path}"));
    let start_col = lines[start].len() - lines[start].trim_start().len() + 1;
    format!(
        "{path}:{}:{start_col}-{}:{}",
        start + 1,
        end + 1,
        lines[end].len() + 1
    )
}

/// Take the temporary directory's name back out of some text.
///
/// macOS hands out `/var/folders/...` and reports it back as `/private/var/...`, so
/// the longer spelling has to go first — replacing the short one first leaves the
/// `/private` behind and prints `/privatesrc/pricing.py`.
fn scrub(text: &str, root: &Path) -> String {
    let root_text = root.to_string_lossy().to_string();
    let private = format!("/private{root_text}");
    let mut text = text.to_string();
    for prefix in [private.as_str(), root_text.as_str()] {
        text = text.replace(&format!("{prefix}/"), "").replace(prefix, ".");
    }
    text.trim_end().to_string()
}

/// Run `fr` and return what it printed, with the temporary path taken back out.
fn run(root: &Path, argv: &[String]) -> String {
    let output = Command::new(FR)
        .arg("--root")
        .arg(root)
        .args(argv)
        .output()
        .expect("running fr");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    // The workspace is a temporary directory and its name is different every run; the
    // page has to show the path a reader would type.
    //
    // macOS hands out `/var/folders/...` and reports it back as `/private/var/...`, so
    // the longer spelling has to go first — replacing the short one first leaves the
    // `/private` behind and prints `/privatesrc/pricing.py`.
    let root_text = root.to_string_lossy().to_string();
    let private = format!("/private{root_text}");
    for prefix in [private.as_str(), root_text.as_str()] {
        text = text.replace(&format!("{prefix}/"), "").replace(prefix, ".");
    }
    text.trim_end().to_string()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

fn catalog_data() -> String {
    let mut out = String::from(
        "// Generated by `cargo test --test site_data`. Do not edit.\n\
         //\n\
         // Every `before`, `after` and `output` below is what the `fr` binary actually\n\
         // produced for the command beside it. Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test site_data\n\
         export const CATALOG = [\n",
    );
    for entry in ENTRIES {
        let tmp = workspace(entry.files);
        let root = tmp.path();
        let argv: Vec<String> = entry
            .argv
            .iter()
            .map(|a| resolve(a, root))
            .collect::<Vec<_>>();

        let before = std::fs::read_to_string(root.join(entry.subject)).unwrap();
        let output = run(root, &argv);
        let after = match entry.kind {
            Kind::Edit => {
                let mut applied = argv.clone();
                applied.push("--write".to_string());
                run(root, &applied);
                let after = std::fs::read_to_string(root.join(entry.subject)).unwrap();
                assert_ne!(
                    before, after,
                    "{} claims to be an edit and changed nothing. What it printed:\n{output}",
                    entry.id
                );
                after
            }
            // A report finds work without doing it, and a refusal declines with a
            // reason. Both are asserted, so the page cannot go on claiming a refusal
            // that no longer happens or a report that has become an edit.
            Kind::Report => {
                assert!(
                    !output.starts_with("Error:"),
                    "{} is a report and failed:\n{output}",
                    entry.id
                );
                String::new()
            }
            Kind::Refused => {
                assert!(
                    output.starts_with("Error:"),
                    "{} claims to be refused and was not:\n{output}",
                    entry.id
                );
                String::new()
            }
        };

        let command = format!(
            "fr {}",
            argv.iter()
                .map(|a| if a.contains(' ') || a.contains('"') {
                    format!("'{a}'")
                } else {
                    a.clone()
                })
                .collect::<Vec<_>>()
                .join(" ")
        );
        let sources: Vec<String> = entry.sources.iter().map(|s| json_string(s)).collect();
        out.push_str(&format!(
            "  {{\n    id: {},\n    kind: {},\n    name: {},\n    sources: [{}],\n    \
             intent: {},\n    note: {},\n    command: {},\n    file: {},\n    before: {},\n    \
             after: {},\n    output: {},\n  }},\n",
            json_string(entry.id),
            json_string(match entry.kind {
                Kind::Edit => "edit",
                Kind::Report => "report",
                Kind::Refused => "refused",
            }),
            json_string(entry.name),
            sources.join(", "),
            json_string(entry.intent),
            json_string(entry.note),
            json_string(&command),
            json_string(entry.subject),
            json_string(&before),
            json_string(&after),
            json_string(&output),
        ));
    }
    out.push_str("];\n");
    out
}

fn translate_data() -> String {
    let crud_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/fastapi/crud.py");
    let crud = std::fs::read_to_string(&crud_path).expect("the vendored corpus");

    let mut out = String::from(
        "// Generated by `cargo test --test site_data`. Do not edit.\n\
         //\n\
         // Every `after` is what `fr translate` actually produced. Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test site_data\n\
         export const TRANSLATIONS = [\n",
    );
    for case in TRANSLATIONS {
        // A Next.js route's URL is its position on disk, so a corpus case copies the
        // whole tree rather than one file.
        let tmp = match case.corpus {
            Some(directory) => corpus(directory).0,
            None => {
                let owned: Vec<(&str, &str)> = if case.files.is_empty() {
                    vec![(case.subject, crud.as_str())]
                } else {
                    case.files.to_vec()
                };
                workspace(&owned)
            }
        };
        let root = tmp.path();

        let before = std::fs::read_to_string(root.join(case.subject)).unwrap();
        let argv = vec![
            "translate".to_string(),
            case.subject.to_string(),
            case.target.to_string(),
        ];
        let report = run(root, &argv);
        let mut applied = argv.clone();
        applied.push("--write".to_string());
        run(root, &applied);

        // The translation writes beside the source under the target's extension.
        let beside = root.join(case.subject);
        let written = std::fs::read_dir(beside.parent().unwrap_or(root))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.file_name() != Path::new(case.subject).file_name() && p.is_file())
            .unwrap_or_else(|| panic!("{} produced no file", case.id));
        // The banner names the file it was translated from, and that path is a
        // temporary directory with a different name every run. The page has to show
        // the path a reader would type.
        let after = scrub(&std::fs::read_to_string(&written).unwrap(), root);

        out.push_str(&format!(
            "  {{\n    id: {},\n    title: {},\n    blurb: {},\n    provenance: {},\n    \
             command: {},\n    from: {},\n    to: {},\n    before: {},\n    after: {},\n    \
             report: {},\n  }},\n",
            json_string(case.id),
            json_string(case.title),
            json_string(case.blurb),
            match case.provenance {
                Some(text) => json_string(text),
                None => "null".to_string(),
            },
            json_string(&format!("fr translate {} {}", case.subject, case.target)),
            json_string(case.subject),
            json_string(written.file_name().unwrap().to_str().unwrap()),
            json_string(&before),
            json_string(&after),
            json_string(&report),
        ));
    }
    out.push_str("];\n");
    out
}

fn recipes_data() -> String {
    let mut out = String::from(
        "// Generated by `cargo test --test site_data`. Do not edit.\n\
         //\n\
         // Every `report`, `after` and `diff` below is what `fr recipe` actually did.\n\
         // Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test site_data\n\
         export const LESSONS = [\n",
    );
    for lesson in LESSONS {
        let tmp = workspace(lesson.files);
        let root = tmp.path();
        std::fs::write(root.join("tidy.recipe"), lesson.recipe).unwrap();

        let before = std::fs::read_to_string(root.join(lesson.subject)).unwrap();
        let argv = ["recipe".to_string(), "tidy.recipe".to_string()];
        let output = run(root, &argv);
        let mut applied = argv.to_vec();
        applied.push("--write".to_string());
        run(root, &applied);
        let after = std::fs::read_to_string(root.join(lesson.subject)).unwrap();

        out.push_str(&format!(
            "  {{\n    id: {},\n    title: {},\n    language: {},\n    teaches: {},\n    \
             note: {},\n    recipe: {},\n    file: {},\n    before: {},\n    after: {},\n    \
             output: {},\n  }},\n",
            json_string(lesson.id),
            json_string(lesson.title),
            json_string(lesson.language),
            json_string(lesson.teaches),
            json_string(lesson.note),
            json_string(lesson.recipe),
            json_string(lesson.subject),
            json_string(&before),
            json_string(&after),
            json_string(&output),
        ));
    }
    out.push_str("];\n");
    out
}

fn check(relative: &str, generated: String) {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    if std::env::var("UPDATE_SITE_DATA").is_ok() {
        std::fs::write(&path, &generated).expect("writing the generated data");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "{relative} is not what the tool produces any more. This is the page telling \
         you the truth about a change you made. Regenerate it:\n\n    \
         UPDATE_SITE_DATA=1 cargo test --test site_data\n"
    );
}

#[test]
fn the_catalogue_page_shows_what_the_tool_does() {
    check("docs/catalog-data.js", catalog_data());
}

#[test]
fn the_recipe_tutorial_shows_what_the_tool_does() {
    check("docs/recipes-data.js", recipes_data());
}

#[test]
fn the_translation_page_shows_what_the_tool_does() {
    check("docs/translate-data.js", translate_data());
}
