//! The type a symbol was declared with, in each language that writes one down.
//!
//! Nothing here is inferred, and that is the point and not a limitation. `x = 5` has no
//! declared type; answering `int` would be a different claim from the one the source made. A
//! tool that quietly fills the gap in cannot show the gap closing.

use fun_refactor::analysis::types;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn describe(files: &[(&str, &str)], symbol: &str) -> types::Declared {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == symbol || s.name == symbol)
        .unwrap_or_else(|| panic!("no `{symbol}`"))
        .id;
    types::of(&index, id).expect("a declared type")
}

const PY: &str = "\
from dataclasses import dataclass

@dataclass(frozen=True)
class Money:
    minor_units: int

def capture(amount: Money, note) -> Money:
    return amount

total = 0
subtotal: Money = Money(0)
";

const TS: &str = "\
export interface Money {
  readonly minorUnits: number;
}

export function capture(amount: Money, note): Money {
  return amount;
}

const total = 0;
const subtotal: Money = { minorUnits: 0 };
";

#[test]
fn a_binding_with_no_annotation_says_so() {
    // `declared` stays `None` however much is worked out from elsewhere: the two are different
    // claims. The whole subject of annotating a codebase is the gap between them. What the
    // reader is shown names which one they are looking at.
    for (files, inferred) in [
        (&[("a.py", PY)][..], "int (from the literal)"),
        (&[("a.ts", TS)][..], "number (from the literal)"),
    ] {
        let found = describe(files, "total");
        assert_eq!(found.declared, None, "{found:?}");
        assert_eq!(found.describe(), inferred);
    }
}

#[test]
fn a_binding_with_an_annotation_reports_it() {
    for files in [&[("a.py", PY)][..], &[("a.ts", TS)][..]] {
        let found = describe(files, "subtotal");
        assert_eq!(found.declared.as_deref(), Some("Money"), "{found:?}");
    }
}

#[test]
fn a_callable_reports_the_signature_a_caller_has_to_satisfy() {
    // The return type alone is not what a caller needs to know, and a parameter the
    // source left untyped is marked and not filled in.
    for files in [&[("a.py", PY)][..], &[("a.ts", TS)][..]] {
        let found = describe(files, "capture");
        assert_eq!(
            found.declared.as_deref(),
            Some("(amount: Money, note: ?) -> Money"),
            "{found:?}"
        );
        assert_eq!(found.parameters.len(), 2);
        assert_eq!(found.parameters[0].1.as_deref(), Some("Money"));
        assert_eq!(found.parameters[1].1, None);
    }
}

#[test]
fn the_type_is_looked_up_in_its_own_language() {
    // A Python class called `Money` and a TypeScript interface called `Money` are two
    // types that share a spelling. The first version of this searched every symbol in
    // the workspace and pointed the TypeScript binding at the Python class, because
    // that one happened to be indexed first.
    let files = [("a.py", PY), ("a.ts", TS)];
    for (symbol_file, expected) in [("a.py", "a.py"), ("a.ts", "a.ts")] {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        for (name, content) in files {
            std::fs::write(tmp.path().join(name), content).expect("the file");
        }
        let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
        let id = index
            .symbols
            .iter()
            .find(|s| s.name == "subtotal" && s.file.ends_with(symbol_file))
            .expect("the binding")
            .id;
        let found = types::of(&index, id).expect("a declared type");
        let defined = found
            .defined_at
            .and_then(|id| index.symbol(id))
            .unwrap_or_else(|| panic!("{symbol_file}: no definition for its type"));
        assert!(
            defined.file.ends_with(expected),
            "{symbol_file} was sent to {}",
            defined.file.display()
        );
    }
}

#[test]
fn a_type_from_outside_the_tree_is_not_a_gap() {
    // `int` resolves nowhere and that is correct. Reporting it as unresolved would put
    // a warning on every annotation anybody writes.
    let found = describe(&[("a.py", PY)], "Money::minor_units");
    assert_eq!(found.declared.as_deref(), Some("int"));
    assert_eq!(found.defined_at, None);
}

// ---------------------------------------------------------------- inference

const PY_INFER: &str = "\
from dataclasses import dataclass

@dataclass(frozen=True)
class Money:
    minor_units: int

@dataclass(frozen=True)
class Payment:
    amount: Money

def price() -> Money:
    return Money(0)

total = 0
label = \"eur\"
paid = True
wallet = Money(500)
copy = wallet
charged = price()
bag = {\"amount\": 100}
nothing = some_library_call()

def take(p: Payment):
    taken = p.amount
    return taken
";

const TS_INFER: &str = "\
export class Money {
  constructor(readonly minorUnits: number) {}
}

export function price(): Money {
  return new Money(0);
}

const total = 0;
const label = \"eur\";
const paid = true;
const wallet = new Money(500);
const copy = wallet;
const charged = price();
const bag = { amount: 100 };
const nothing = someLibraryCall();
";

#[test]
fn a_literal_states_its_own_type() {
    for (files, cases) in [
        (
            &[("a.py", PY_INFER)][..],
            &[("total", "int"), ("label", "str"), ("paid", "bool")][..],
        ),
        (
            &[("a.ts", TS_INFER)][..],
            &[
                ("total", "number"),
                ("label", "string"),
                ("paid", "boolean"),
            ][..],
        ),
    ] {
        for (name, expected) in cases {
            let found = describe(files, name);
            assert_eq!(found.declared, None, "{name} should have no declared type");
            let inferred = found
                .inferred
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: nothing inferred"));
            assert_eq!(&inferred.ty, expected, "{name}");
            assert_eq!(inferred.basis, types::Basis::Literal, "{name}");
        }
    }
}

#[test]
fn constructing_a_class_gives_its_type() {
    for files in [&[("a.py", PY_INFER)][..], &[("a.ts", TS_INFER)][..]] {
        let found = describe(files, "wallet");
        let inferred = found.inferred.as_ref().expect("nothing inferred");
        assert_eq!(inferred.ty, "Money");
        assert_eq!(inferred.basis, types::Basis::Construction);
        assert!(inferred.from.is_some(), "no evidence recorded");
    }
}

#[test]
fn a_call_gives_what_the_callee_declared_it_returns() {
    for files in [&[("a.py", PY_INFER)][..], &[("a.ts", TS_INFER)][..]] {
        let found = describe(files, "charged");
        let inferred = found.inferred.as_ref().expect("nothing inferred");
        assert_eq!(inferred.ty, "Money");
        assert_eq!(inferred.basis, types::Basis::ReturnOfCall);
    }
}

#[test]
fn one_binding_carries_to_the_next() {
    for files in [&[("a.py", PY_INFER)][..], &[("a.ts", TS_INFER)][..]] {
        let found = describe(files, "copy");
        let inferred = found.inferred.as_ref().expect("nothing inferred");
        assert_eq!(inferred.ty, "Money");
        assert_eq!(inferred.basis, types::Basis::SameBinding);
    }
}

#[test]
fn a_field_gives_what_its_record_declared() {
    let found = describe(&[("a.py", PY_INFER)], "taken");
    let inferred = found.inferred.as_ref().expect("nothing inferred");
    assert_eq!(inferred.ty, "Money");
    assert_eq!(inferred.basis, types::Basis::FieldOfRecord);
}

#[test]
fn an_object_literal_is_not_answered_with_dict() {
    // `{"amount": 100}` is a dict and saying so is true and useless: a dictionary is where a
    // type should have been. A tool that answers `dict` has agreed with the code and not
    // described it.
    for (files, name) in [
        (&[("a.py", PY_INFER)][..], "bag"),
        (&[("a.ts", TS_INFER)][..], "bag"),
    ] {
        let found = describe(files, name);
        assert_eq!(found.inferred, None, "{name}: {found:?}");
        assert_eq!(found.describe(), "no type written down");
    }
}

#[test]
fn a_call_out_of_the_workspace_yields_nothing() {
    // The chain stops where the evidence does. `Any` would be a different claim.
    for files in [&[("a.py", PY_INFER)][..], &[("a.ts", TS_INFER)][..]] {
        let found = describe(files, "nothing");
        assert_eq!(found.inferred, None, "{found:?}");
    }
}

#[test]
fn a_declaration_wins_over_a_derivation() {
    // An annotation is a contract and an inference is a derivation. Where both could apply the
    // contract is the answer. A disagreement is a defect in the code instead of a choice for
    // this to make.
    let source = "\
class Money:
    pass

subtotal: int = Money()
";
    let found = describe(&[("a.py", source)], "subtotal");
    assert_eq!(found.declared.as_deref(), Some("int"));
    assert_eq!(found.inferred, None);
}

/// `--json` answers with names and places, like every other command.
///
/// It used to serialize the analysis struct directly, so it emitted `"symbol". 1` and
/// `"defined_at": 0`, `SymbolId`s, which are positions in one run's index and mean nothing to
/// whoever reads the output. `defined_at` read like a line number. The text rendering resolved
/// them all along; only the machine-readable half did not.
#[test]
fn the_json_names_what_it_points_at() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        tmp.path().join("a.py"),
        "class Money:\n    def __init__(self, cents):\n        self.cents = cents\n\n\n\
         def charge():\n    fee = Money(250)\n    return fee\n",
    )
    .expect("the file");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fr"))
        .args([
            "-C",
            tmp.path().to_str().expect("utf-8"),
            "type",
            "fee",
            "--json",
        ])
        .output()
        .expect("fr type");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON on stdout");
    assert_eq!(json["symbol"], "fee");
    let evidence = &json["inferred"]["evidence"];
    assert_eq!(evidence["symbol"], "Money", "{json:#}");
    assert_eq!(evidence["line"], 1, "{json:#}");
    assert!(
        evidence["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("a.py")),
        "{json:#}"
    );
    for pointer in ["/symbol", "/inferred/evidence/symbol"] {
        assert!(
            json.pointer(pointer).is_some_and(|v| v.is_string()),
            "{pointer} should name a symbol, not index one: {json:#}"
        );
    }
}

/// Go and Java fix a binding's type at the declaration, so a literal there settles it for
/// the whole scope. Rust and Zig are absent. `let x = 0;` takes its type from a later use,
/// and Zig's `0` is a `comptime_int` no parameter can be written with.
#[test]
fn a_go_or_java_literal_settles_what_the_binding_holds() {
    let go = "package main\n\nfunc run() {\n\ttotal := 0\n\tlabel := \"a\"\n\n\t\
              _ = total\n\t_ = label\n}\n";
    for (name, expected) in [("total", "int"), ("label", "string")] {
        let found = describe(&[("a.go", go)], name);
        let inferred = found.inferred.as_ref().expect("nothing inferred");
        assert_eq!(&inferred.ty, expected, "{name}");
    }

    let java = "public final class Calc {\n    static void run() {\n        var n = 2;\n\n        \
                var s = \"a\";\n    }\n}\n";
    for (name, expected) in [("n", "int"), ("s", "String")] {
        let found = describe(&[("Calc.java", java)], name);
        let inferred = found.inferred.as_ref().expect("nothing inferred");
        assert_eq!(&inferred.ty, expected, "{name}");
    }
}

/// Arithmetic over two operands of one type yields that type. A comparison yields a
/// boolean, so `a < b` derives nothing here.
#[test]
fn arithmetic_over_one_type_yields_that_type() {
    let go = "package main\n\nfunc run() {\n\ta := 6\n\tb := 3\n\n\tratio := a / b\n\t\
              smaller := a < b\n\n\t_ = ratio\n\t_ = smaller\n}\n";
    let ratio = describe(&[("a.go", go)], "ratio");
    let inferred = ratio.inferred.as_ref().expect("nothing inferred");
    assert_eq!(inferred.ty, "int");
    assert_eq!(inferred.basis, types::Basis::BothOperands);

    let smaller = describe(&[("a.go", go)], "smaller");
    assert_eq!(smaller.inferred, None, "a comparison is not its operands");
}

/// The walk out of a declaration looking for a written type stops at the block. A Zig
/// `const width = 3;` inside `fn run() void` reported `void`, the function's return type.
#[test]
fn a_binding_does_not_borrow_the_enclosing_function_type() {
    let zig = "fn run() void {\n    const width = 3;\n}\n";
    let found = describe(&[("a.zig", zig)], "width");
    assert_eq!(found.declared, None, "the source wrote no type for `width`");
    assert_eq!(found.inferred, None, "and nothing derives one");
}
