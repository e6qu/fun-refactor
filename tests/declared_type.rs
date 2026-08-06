//! The type a symbol was declared with, in each language that writes one down.
//!
//! Nothing here is inferred, and that is the point rather than a limitation. `x = 5`
//! has no declared type; answering `int` would be a different claim from the one the
//! source made, and a tool that quietly fills the gap in cannot show the gap closing.

use fun_refactor::analysis::declared;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn describe(files: &[(&str, &str)], symbol: &str) -> declared::Declared {
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
    declared::of(&index, id).expect("a declared type")
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
    for files in [&[("a.py", PY)][..], &[("a.ts", TS)][..]] {
        let found = describe(files, "total");
        assert_eq!(found.declared, None, "{found:?}");
        assert_eq!(found.describe(), "no type written down");
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
    // source left untyped is marked rather than filled in.
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
        let found = declared::of(&index, id).expect("a declared type");
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
