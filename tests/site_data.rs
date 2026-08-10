//! Generates the data behind `docs/catalog.html` and `docs/translate.html`.
//!
//! Every before, after and diff on those pages is produced here by running the real
//! `fr` binary over the sample files below, in a temporary directory, exactly as the
//! command printed beside it would. Nothing on either page is typed by hand, because a
//! hand-typed "after" is a claim about the tool and not a demonstration of it, and
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
    /// What is the same after the change as it was before.
    ///
    /// A refactoring is a change to the *text* of a program that leaves what the
    /// program does alone. Every entry has to be able to say what that means for it —
    /// and a move that redistributes behaviour across several files has to say where
    /// the behaviour went, not only where the edit landed. Naming it per entry is what
    /// stops the page from being a list of edits that happen to be reversible.
    invariant: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The `fr` invocation, with `@from…to@` standing for a range this test computes
    /// from the source instead of a line and column somebody counted by hand.
    argv: &'static [&'static str],
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
"#;

/// The consumer, in a file of its own.
///
/// A signature change is only half a refactoring until the calls change with it, and a
/// page that shows the declaration alone is showing the half that would break the
/// program. Every sample whose move ripples outward keeps its callers where a reader
/// can watch them move.
const GEOMETRY_USES: &str = r#"from geometry import circ


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
"#;

const CONNECT_USES_PY: &str = r#"from net import send


def main():
    return send("example.com", 443)


def retry():
    return send("backup.example.com", 8443)
"#;

// `precision` is passed at every call and read by nothing: the parameter nobody needs.
const DEFAULTS_PY: &str = r#"import math


def circ(r, precision, units="m"):
    """The distance around a circle."""
    return f"{2 * math.pi * r}{units}"
"#;

const DEFAULTS_USES_PY: &str = r#"from geometry import circ


def rim(r):
    return circ(r, 2)


def label(r):
    return "rim: " + circ(r, 4)
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
        invariant: "The statements run in the same order, on the same values, and produce the same result; they are reached through a call now instead of being written where they run.",
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
        invariant: "The expression is computed once instead of where it stood, and every place that used it reads the same value.",
        note: "The name is the whole point of the move: the expression is unchanged and \
               the code is no shorter.",
        files: &[("src/pricing.py", PRICING)],
        argv: &[
            "extract",
            "@src/pricing.py~order.quantity * order.item_price~@",
            "base_price",
        ],
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
        invariant: "The value is computed where it is used instead of once above; the parentheses keep the operators binding the way they did.",
        note: "The reverse of Extract Variable, and the reason both are in the catalogue: \
               which one is an improvement depends on whether the name earns its keep.",
        files: &[("src/pricing.py", PRICING)],
        argv: &["inline", "src/pricing.py:6:5"],
    },
    Entry {
        kind: Kind::Edit,
        id: "inline-function",
        name: "Inline Function",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §6.2"],
        intent: "A function whose body is as clear as its name is replaced by its body.",
        invariant: "The callee's body runs at the call site instead of inside a call; the same expression is evaluated on the same arguments.",
        note: "The call is replaced with the callee's body, with the arguments substituted \
               for the parameters.",
        files: &[("src/delivery.py", DELIVERY)],
        argv: &["inline", "src/delivery.py:2:17", "--call"],
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
        invariant: "Every call passes the same values it did, plus the new one. The declaration and the call sites move together: either both change or nothing does.",
        note: "The call inside `band_width` is updated twice, because it is called twice. \
               A call the tool could not resolve would be reported and not rewritten.",
        files: &[
            ("src/geometry.py", GEOMETRY),
            ("src/report.py", GEOMETRY_USES),
        ],
        argv: &["signature", "circ", "add:1:units: str:\"m\""],
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
        invariant: "Nothing but the name changes. Every call still reaches the same function, in both files.",
        note: "`circ` appears twice inside `band_width` and once in the docstring. The \
               docstring mention is reported, not rewritten — a name in prose is not a \
               reference.",
        files: &[
            ("src/geometry.py", GEOMETRY),
            ("src/report.py", GEOMETRY_USES),
        ],
        argv: &["rename", "circ", "circumference"],
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
        invariant: "One binding is renamed and the other `width` — a different binding in a different function — is not. Every read still reads what it read.",
        note: "There are two variables called `width` in this file and only one of them \
               is being renamed. Nothing here matches on the text `width`: the rename \
               follows the lexical scope, so the one in `area` is a different binding \
               and is left alone. A find-and-replace gets this wrong every time.",
        files: &[("src/geometry.py", SCOPES_PY)],
        argv: &["rename", "src/geometry.py:5:5", "span"],
    },
    Entry {
        kind: Kind::Edit,
        id: "rename-across-languages",
        name: "Rename, across a language boundary",
        sources: &["Not in either catalogue — the catalogues predate the problem"],
        intent: "A CSS class is renamed in the stylesheet and everywhere the markup names it.",
        invariant: "The stylesheet still styles the same element. The rule and the `class` attribute that reaches it are renamed together, across two grammars.",
        note: "Two files, two grammars, one name. The stylesheet declares `.nav-link` and \
               the HTML reaches it through a `class` attribute — no import, no path, \
               nothing a compiler would check. The catalogues are about one language at \
               a time, and most of a web codebase is not.",
        files: &[("web/app.css", APP_CSS), ("web/page.html", PAGE_HTML)],
        argv: &["rename", "nav-link", "primary-link"],
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
        invariant: "The parameter was unused, so no call was passing anything the body read. Every call keeps the arguments the body still uses.",
        note: "The declaration and the calls change together or not at all. A call the \
               tool could not resolve would be reported and not left quietly \
               passing an argument to a parameter that no longer exists.",
        files: &[
            ("src/geometry.py", DEFAULTS_PY),
            ("src/rim.py", DEFAULTS_USES_PY),
        ],
        argv: &["signature", "circ", "remove:1"],
    },
    Entry {
        kind: Kind::Refused,
        id: "remove-parameter-refused",
        name: "…and the parameter that could not go",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §6.5"],
        intent: "The same move, where taking the parameter away would change what runs.",
        invariant: "Nothing changes. `units` is read by the body, so removing it would \
                    leave a name nothing supplies — a change to what the program does, \
                    which is the definition of not a refactoring.",
        note: "The rule existed for shell functions, where a parameter is `$1` and the \
               body reading `$2` is obvious, and for nothing else. `def circ(r): return \
               f\"…{units}\"` was produced happily until this page asked for it.",
        files: &[
            ("src/geometry.py", DEFAULTS_PY),
            ("src/rim.py", DEFAULTS_USES_PY),
        ],
        argv: &["signature", "circ", "remove:2"],
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
        invariant: "Each argument still arrives at the parameter it was written for, in both files: the parameters and the arguments move in step.",
        note: "The arguments move with the parameters. Getting one of the two halves \
               right is worse than doing nothing, which is why this is a refactoring \
               instead of two edits.",
        files: &[
            ("src/net.py", CONNECT_PY),
            ("src/client.py", CONNECT_USES_PY),
        ],
        argv: &["signature", "send", "move:0:1"],
    },
    Entry {
        kind: Kind::Refused,
        id: "reorder-parameters-refused",
        name: "…and the reorder that would not run",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §6.5"],
        intent: "The same move, where the language will not have it.",
        invariant: "Nothing changes, because the only change available would not preserve anything — the file would stop parsing as Python.",
        note: "Python requires every defaulted parameter to come last, so this would \
               produce `def circ(units=\"m\", r):` — which Python rejects outright. The \
               engine reparses every edit and would normally catch a broken result, but \
               tree-sitter parses this without complaint, so the refactoring has to \
               know the rule itself. It did not until this page was written.",
        files: &[("src/geometry.py", DEFAULTS_PY)],
        argv: &["signature", "circ", "move:0:2"],
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
        invariant: "Both occurrences read one binding instead of computing the same expression twice; the expression was the same, so the value is.",
        note: "Fowler's mechanics say to replace *all* occurrences, and the second one \
               here is inside a larger expression and not alone on a line — which is \
               why this matches on the parse tree and not on the text.",
        files: &[("src/pricing.py", REPEATED_PY)],
        argv: &[
            "extract",
            "@src/pricing.py~order.quantity * order.item_price~@",
            "gross",
            "--all",
        ],
    },
    Entry {
        kind: Kind::Edit,
        id: "organize-imports",
        name: "Remove unused imports",
        sources: &["Not in either catalogue — but it is the tidying you do after the others"],
        intent: "Imports nothing uses are dropped and the rest are sorted.",
        invariant: "The same modules are imported. Only the order changes, and Python does not care about the order of independent imports.",
        note: "Read what it prints above the diff. Liveness is decided by name, so an \
               import kept for a trait, a registration side effect or a doc comment \
               would look unused — and it says so instead of letting you find out. \
               This is the step a recipe puts last, with `imports where changed`.",
        files: &[("src/loader.py", UNSORTED_PY)],
        argv: &["imports", "src/loader.py"],
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
        invariant: "The function is called through an import instead of from beside its caller. The same code runs on the same arguments; only where it is written moved.",
        note: "The import appears in the file it left, because the function that stayed \
               behind still calls it.",
        files: &[("src/account.py", ACCOUNT), ("src/rates.py", RATES)],
        argv: &["move", "days_overdue", "src/rates.py"],
    },
    Entry {
        kind: Kind::Edit,
        id: "remove-dead-code",
        name: "Remove Dead Code",
        sources: &[
            "Refactoring, 2nd ed. — Martin Fowler (2018), §8.9",
            "Tidy First? — Kent Beck (2023), “Dead Code”",
        ],
        intent: "Code nothing calls is deleted and not maintained.",
        invariant: "Nothing reaches the deleted function, so nothing that runs today stops running. That is what makes the deletion safe instead of a guess.",
        note: "`fr unused` finds it and `fr delete` removes it, and the two must agree: \
               delete refuses anything still referenced, which is what makes the list \
               worth acting on.",
        files: &[("src/reports.py", REPORTS)],
        argv: &["delete", "_legacy_histogram"],
    },
    Entry {
        kind: Kind::Refused,
        id: "delete-refused",
        name: "…and the one it will not delete",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §8.9"],
        intent: "Deleting something that is still reached is not dead-code removal.",
        invariant: "Nothing changes. The symbol is reachable, so removing it would change what the program does — which is the definition of not a refactoring.",
        note: "The boundary that makes `fr unused` worth acting on: whatever the list \
               says, `delete` checks again and refuses anything still referenced. The \
               two halves have to agree, and running them against each other over a \
               nine-language workspace is how thirteen disagreements in fifty-nine were \
               found.",
        files: &[("src/live.py", LIVE_PY)],
        argv: &["delete", "helper"],
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
        invariant: "The same conditions decide the same outcome; the nesting is inverted so the exceptional cases leave early.",
        note: "Beck's version is the one this does: a single guard at a time, taken off \
               the front. Run it again on what is left and the second guard comes off too.",
        files: &[("src/notify.py", NOTIFY)],
        argv: &["rewrite", "src/notify.py:2:5", "guard-clause"],
    },
    Entry {
        kind: Kind::Refused,
        id: "guard-clauses-refused",
        name: "…and where it stops",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §10.3"],
        intent: "Fowler's own example is an `if`/`else` nest that assigns to a result.",
        invariant: "Nothing changes. The tool could not prove the shape it needs, and a guess here would reorder the conditions.",
        note: "The tool will not do this one, and says why instead of guessing. Turning \
               an `else` into an early return means deciding what the function returns on \
               the path that used to fall through — a judgement about the code, not a \
               fact about its syntax. `invert-if` is the move it offers instead, and the \
               entry below is that same file.",
        files: &[("src/payout.py", PAYOUT)],
        argv: &["rewrite", "src/payout.py:2:5", "guard-clause"],
    },
    Entry {
        kind: Kind::Edit,
        id: "reverse-conditional",
        name: "Reverse Conditional",
        sources: &["The online refactoring catalogue — Martin Fowler"],
        intent: "A condition is negated and its branches swapped, when that reads better.",
        invariant: "The branches swap and the condition is negated with them, so each case still runs for the same inputs.",
        note: "Purely local: the tool does not need to resolve a single name to know this \
               is sound, which is why it is offered at a position and not for a symbol.",
        files: &[("src/payout.py", PAYOUT)],
        argv: &["rewrite", "src/payout.py:2:5", "invert-if"],
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
        invariant: "`not (a and b)` and `not a or not b` are the same predicate; the same readings still alert.",
        note: "Named honestly: this is not Fowler's Consolidate Conditional Expression \
               (§10.2), which combines several conditionals that produce the same result. \
               It is a law of logic applied by the grammar and not by eye. The two \
               forms mean the same thing and one of them is usually the one you meant.",
        files: &[("src/alerts.py", ALERTS)],
        argv: &["rewrite", "src/alerts.py:2:12", "de-morgan"],
    },
    Entry {
        kind: Kind::Edit,
        id: "substitute-algorithm",
        name: "Substitute Algorithm",
        sources: &["Refactoring, 2nd ed. — Martin Fowler (2018), §7.9"],
        intent: "Every occurrence of one shape of code becomes another shape.",
        invariant: "Both bodies send the same metric for the same event. One of them now does it by calling the other.",
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
        invariant: "The flag was constant, so only one branch could ever run. The branch that ran still runs; the one that could not is gone.",
        note: "The dead branch goes with the flag. This is the one move on the page that \
               cascades: it keeps going until nothing else falls out.",
        files: &[("src/checkout.py", CHECKOUT)],
        argv: &["remove-flag", "NEW_CHECKOUT", "--value", "true"],
    },
    Entry {
        kind: Kind::Report,
        id: "tdd-refactor-step",
        name: "The refactor step of red–green–refactor",
        sources: &["Test-Driven Development by Example — Kent Beck (2002)"],
        intent: "Once the test passes, the duplication that made it pass is removed.",
        invariant: "The test does not change, which is the whole point: it passed before the move and it passes after.",
        note: "The cycle ends with a refactoring, and the refactoring starts with seeing \
               the duplication. These two functions share not one identifier — `bank` and \
               `exchange`, `rate` and `ratio`, `converted` and `result` — and they are the \
               same code. Structure is compared, not text, which is the copy a textual \
               search never finds. What to do about it is yours; the moves above are the menu.",
        files: &[("src/exchange.py", TDD_CYCLE)],
        argv: &["duplicates", "--min-tokens", "40"],
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
    /// input is a *tree* and not a file — a Next.js route's URL is its path.
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

const TYPED_ZIG: &str = r#"//! Readings from a sensor.

const default_limit: f64 = 30.0;

/// One sample from a sensor.
pub const Reading = struct {
    sensor_id: []const u8,
    celsius: f64,
    valid: bool,

    /// Whether this reading is above a limit.
    pub fn warmerThan(self: Reading, limit: f64) bool {
        return self.celsius > limit;
    }
};

/// Count the readings above a limit.
pub fn warmer(readings: []const Reading, limit: f64) i64 {
    var count: i64 = 0;
    for (readings) |reading| {
        if (reading.celsius > limit) {
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
        id: "python-to-java",
        title: "Typed Python → Java",
        blurb: "The language with no top level below the type. A module has to become a \
                class, `sensors.py` has to become `Sensors.java` — the compiler enforces \
                that, it is not a convention — and a record that would have been public \
                becomes a package-private sibling, with a comment saying why.",
        files: &[("sensors.py", TYPED_PYTHON)],
        subject: "sensors.py",
        target: "java",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "zig-to-rust",
        title: "Zig → Rust",
        blurb: "Zig has no keyword for a method: `self` is the first parameter and a \
                struct is a value bound to a `const`. Rust puts the same shape in an \
                `impl` block, and `[]const u8` — Zig's string, which is a slice of \
                bytes that does not change — becomes `String`.",
        files: &[("sensors.zig", TYPED_ZIG)],
        subject: "sensors.zig",
        target: "rust",
        provenance: None,
        corpus: None,
    },
    Translation {
        id: "typescript-to-zig",
        title: "Typed TypeScript → Zig",
        blurb: "The other way, into the language with the least in common with the rest. \
                There is no `new`, no exception, no interpolated string and no block \
                comment — so a fragment with no counterpart goes on its own line above \
                the statement, because `//` in Zig would swallow the semicolon.",
        files: &[("sensors.ts", TYPED_TYPESCRIPT)],
        subject: "sensors.ts",
        target: "zig",
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
               reason `changed` is a predicate and not a path you have to keep in \
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
/// for a sub-expression. Written as the text and not as four numbers, because four
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

/// One command's standard output, on its own.
///
/// `run` glues stderr on the end, which is right for a report and wrong for a document:
/// `fr openapi` puts the notes on stderr precisely so that stdout stays parseable.
fn run_stdout(root: &Path, argv: &[String]) -> String {
    let output = Command::new(FR)
        .arg("--root")
        .arg(root)
        .args(argv)
        .output()
        .expect("running fr");
    scrub(&String::from_utf8_lossy(&output.stdout), root)
}

/// One command's standard error, on its own.
///
/// `fr openapi` puts the document on stdout and everything it could not settle on
/// stderr, so that the document stays a document. The page wants both halves and has to
/// keep them apart.
fn run_stderr(root: &Path, argv: &[String]) -> String {
    let output = Command::new(FR)
        .arg("--root")
        .arg(root)
        .args(argv)
        .output()
        .expect("running fr");
    let mut text = String::from_utf8_lossy(&output.stderr).to_string();
    let root_text = root.to_string_lossy().to_string();
    let private = format!("/private{root_text}");
    for prefix in [private.as_str(), root_text.as_str()] {
        text = text.replace(&format!("{prefix}/"), "").replace(prefix, ".");
    }
    text.trim_end().to_string()
}

/// Copy a directory tree, so a sample kept in the repository can be run against.
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

        // Every file in the sample, not one of them. A refactoring that changes a
        // signature has to change the callers too, and a page that shows only the
        // declaration is showing half of the move — the half that on its own would
        // break the program.
        let before: Vec<String> = entry
            .files
            .iter()
            .map(|(path, _)| std::fs::read_to_string(root.join(path)).unwrap())
            .collect();
        let output = run(root, &argv);
        let after = match entry.kind {
            Kind::Edit => {
                let mut applied = argv.clone();
                applied.push("--write".to_string());
                run(root, &applied);
                let after: Vec<String> = entry
                    .files
                    .iter()
                    .map(|(path, _)| std::fs::read_to_string(root.join(path)).unwrap())
                    .collect();
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
                before.clone()
            }
            Kind::Refused => {
                assert!(
                    output.starts_with("Error:"),
                    "{} claims to be refused and was not:\n{output}",
                    entry.id
                );
                before.clone()
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
        let files: Vec<String> = entry
            .files
            .iter()
            .zip(before.iter().zip(after.iter()))
            .map(|((path, _), (before, after))| {
                format!(
                    "{{ path: {}, before: {}, after: {} }}",
                    json_string(path),
                    json_string(before),
                    json_string(after)
                )
            })
            .collect();
        out.push_str(&format!(
            "  {{\n    id: {},\n    kind: {},\n    name: {},\n    sources: [{}],\n    \
             intent: {},\n    invariant: {},\n    note: {},\n    command: {},\n    \
             files: [\n      {},\n    ],\n    output: {},\n  }},\n",
            json_string(entry.id),
            json_string(match entry.kind {
                Kind::Edit => "edit",
                Kind::Report => "report",
                Kind::Refused => "refused",
            }),
            json_string(entry.name),
            sources.join(", "),
            json_string(entry.intent),
            json_string(entry.invariant),
            json_string(entry.note),
            json_string(&command),
            files.join(",\n      "),
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
        // whole tree instead of one file.
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

// -------------------------------------------------------------- the contract

/// One endpoint of the pet store, and what it looks like on both sides of the crossing.
struct Endpoint {
    /// The route file, relative to the sample tree.
    route: &'static str,
    /// What kind of endpoint this is, for the reader working down the page.
    shape: &'static str,
    /// What this one demonstrates that the others do not.
    note: &'static str,
}

/// Every shape a CRUD API has, in one tree.
///
/// Not a sampler: a router that answers all of these is a router that has met a
/// collection, a member, a sub-collection, a sub-member, a replacement, an action, an
/// aggregate and a catch-all — which is the whole surface most APIs ever have.
const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        route: "app/api/pets/route.ts",
        shape: "A collection: list and create",
        note: "`GET` reads `species` out of the query string and `POST` validates its \
               body against a zod schema declared in another module. Neither is a \
               declaration Next.js makes, and both are in the contract.",
    },
    Endpoint {
        route: "app/api/pets/[petId]/route.ts",
        shape: "A member: read, patch, delete",
        note: "`context.params.petId` becomes `pet_id` wherever it appears, because \
               FastAPI supplies the path parameter directly. The URL is unchanged; only \
               the way the handler reaches the value did.",
    },
    Endpoint {
        route: "app/api/pets/[petId]/photos/route.ts",
        shape: "A sub-collection under a member",
        note: "The parent's path parameter is still in scope: `/pets/{pet_id}/photos` \
               carries `pet_id` into a handler that also takes a body.",
    },
    Endpoint {
        route: "app/api/pets/[petId]/photos/[photoId]/route.ts",
        shape: "A sub-member: two path parameters",
        note: "Both parameters arrive, in the order the tree declares them, and both \
               are used in the body. Getting one of them right is not half a rewrite.",
    },
    Endpoint {
        route: "app/api/pets/[petId]/status/route.ts",
        shape: "A sub-resource replaced whole: PUT",
        note: "A nullable field in the schema — `note: z.string().nullable()` — becomes \
               `str | None`, and the contract leaves it out of `required`.",
    },
    Endpoint {
        route: "app/api/pets/search/route.ts",
        shape: "An action, which is not CRUD at all",
        note: "`/pets/search` is a *sibling* of `/pets/{pet_id}` in the tree, so a \
               router has to tell a literal segment from a parameter. Both are in the \
               contract, and the order they are declared in decides which wins.",
    },
    Endpoint {
        route: "app/api/stores/[storeId]/inventory/route.ts",
        shape: "An aggregate over a different resource",
        note: "A second root — `/stores/…` — with a path parameter of its own, to show \
               the tree is not one resource deep.",
    },
    Endpoint {
        route: "app/api/files/[...path]/route.ts",
        shape: "A catch-all segment",
        note: "`[...path]` matches across slashes; FastAPI spells that `{path:path}`. A \
               rewrite that emitted `{path}` would answer a strictly smaller set of \
               URLs than the one it replaced — silently, and only for the requests with \
               a slash in them.",
    },
];

/// The pet store, translated route by route, with the contract it declares.
fn contract_data() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/petstore");
    let tmp = tempfile::tempdir().expect("a temporary directory");
    // Named, not the temporary directory itself: `fr openapi` titles the document after
    // the workspace, and a title that is a different random string every run would make
    // the committed page churn for no reason.
    let workspace = tmp.path().join("petstore");
    copy(&root, &workspace);
    let workspace = workspace.as_path();

    let contract = run_stdout(workspace, &["openapi".into(), "--yaml".into()]);
    assert!(
        contract.contains("openapi: 3.1.0"),
        "the contract is a document:\n{contract}"
    );
    // The notes go to stderr, so what lands here is the document. Everything the
    // document could not settle is fetched separately, because it is the other half of
    // the story: a baseline that quietly invents an entry is worse than no baseline.
    let notes = run_stderr(workspace, &["openapi".into()]);

    let mut out = String::from(
        "// Generated by `cargo test --test site_data`. Do not edit.\n\
         //\n\
         // The contract, and every endpoint on both sides of the crossing, produced by\n\
         // running the real binary over `tests/petstore`. Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test site_data\n",
    );
    out.push_str(&format!(
        "export const CONTRACT = {};\n\nexport const CONTRACT_NOTES = {};\n\n\
         export const ENDPOINTS = [\n",
        json_string(&contract),
        json_string(&notes)
    ));

    for endpoint in ENDPOINTS {
        let before = std::fs::read_to_string(workspace.join(endpoint.route)).unwrap();
        let argv = vec![
            "translate".to_string(),
            endpoint.route.to_string(),
            "fastapi".to_string(),
        ];
        let report = run(workspace, &argv);
        assert!(
            !report.starts_with("Error:"),
            "{}: {report}",
            endpoint.route
        );
        let mut applied = argv.clone();
        applied.push("--write".to_string());
        run(workspace, &applied);
        // The destination is beside the route, named after the URL it serves.
        let directory = workspace
            .join(endpoint.route)
            .parent()
            .unwrap()
            .to_path_buf();
        // Read off disk and not out of the report, so it has to be scrubbed here:
        // the header names the file it was translated from, and that path is a
        // temporary directory whose name changes every run.
        let after = std::fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "py"))
            .map(|p| scrub(&std::fs::read_to_string(p).unwrap(), workspace))
            .unwrap_or_else(|| panic!("{} produced no Python", endpoint.route));

        out.push_str(&format!(
            "  {{\n    route: {},\n    shape: {},\n    note: {},\n    command: {},\n    \
             before: {},\n    after: {},\n    report: {},\n  }},\n",
            json_string(endpoint.route),
            json_string(endpoint.shape),
            json_string(endpoint.note),
            json_string(&format!("fr translate '{}' fastapi", endpoint.route)),
            json_string(&before),
            json_string(&after),
            json_string(&report),
        ));
    }
    out.push_str("];\n\n");

    // The crossing, checked. Every route file has been translated by now, so the same
    // command reads the *other* side — the decorators and the signatures a FastAPI
    // router declares — and the two documents are compared operation by operation.
    //
    // This is the check you can make without running the service, and it catches the
    // failure the whole exercise is about: an endpoint that did not survive, or a path
    // that quietly changed shape.
    for route in ENDPOINTS {
        std::fs::remove_file(workspace.join(route.route)).unwrap();
    }
    let crossed = run_stdout(workspace, &["openapi".into(), "--yaml".into()]);
    out.push_str(&format!(
        "export const CROSSED = {};\n",
        json_string(&crossed)
    ));

    let operations = |document: &str| -> Vec<String> {
        let value: serde_json::Value = serde_yaml::from_str(document).expect("a document");
        let mut found = Vec::new();
        if let Some(paths) = value.get("paths").and_then(|p| p.as_object()) {
            for (path, item) in paths {
                for (method, operation) in item.as_object().into_iter().flatten() {
                    let query: Vec<String> = operation
                        .get("parameters")
                        .and_then(|p| p.as_array())
                        .map(|all| {
                            all.iter()
                                .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("query"))
                                .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                                .map(|n| n.to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    found.push(match query.is_empty() {
                        true => format!("{} {path}", method.to_uppercase()),
                        false => format!("{} {path}?{}", method.to_uppercase(), query.join("&")),
                    });
                }
            }
        }
        found.sort();
        found
    };

    let before = operations(&contract);
    let after = operations(&crossed);
    // The addressing half — the URLs and the methods — has to be identical. That is the
    // part this tool takes responsibility for.
    let addressing = |ops: &[String]| -> Vec<String> {
        let mut out: Vec<String> = ops
            .iter()
            .map(|o| o.split('?').next().unwrap_or(o).to_string())
            .collect();
        out.sort();
        out.dedup();
        out
    };
    assert_eq!(
        addressing(&before),
        addressing(&after),
        "the URLs and methods must survive the crossing"
    );

    let lost: Vec<String> = before
        .iter()
        .filter(|o| !after.contains(o))
        .cloned()
        .collect();
    let gained: Vec<String> = after
        .iter()
        .filter(|o| !before.contains(o))
        .cloned()
        .collect();
    // Not only the addressing half. The whole contract survives this crossing —
    // including the query parameters, which neither framework declares — and asserting
    // it here is what stops the page from going on claiming so after it stops being
    // true. What the two documents *cannot* say is a separate matter, and is in the
    // notes beside them.
    assert!(
        lost.is_empty() && gained.is_empty(),
        "the contract must survive the crossing\n  lost:   {lost:?}\n  gained: {gained:?}"
    );
    out.push_str(&format!(
        "\nexport const SURVIVED = {{\n  before: [{}],\n  after: [{}],\n  \
         lost: [{}],\n  gained: [{}],\n}};\n",
        before
            .iter()
            .map(|o| json_string(o))
            .collect::<Vec<_>>()
            .join(", "),
        after
            .iter()
            .map(|o| json_string(o))
            .collect::<Vec<_>>()
            .join(", "),
        lost.iter()
            .map(|o| json_string(o))
            .collect::<Vec<_>>()
            .join(", "),
        gained
            .iter()
            .map(|o| json_string(o))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    out
}

#[test]
fn the_contract_page_shows_what_the_tool_does() {
    check("docs/contract-data.js", contract_data());
}
