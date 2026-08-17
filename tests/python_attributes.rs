//! A Python instance attribute is a symbol, and it renames as one thing.
//!
//! `self.count = 0` is how a Python object gets its fields. The most common
//! rename target the language has answered "no symbol or resolved reference
//! at" that position. Each assignment site is a definition; the class, carried as
//! the qualifier, is the identity that groups them; the reads follow.

use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

const COUNTER_PY: &str = "class Counter:\n    def __init__(self) -> None:\n        \
    self.count = 0\n\n    def bump(self) -> int:\n        self.count = self.count + 1\n        \
    return self.count\n";

fn indexed(source: &str) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("model.py"), source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

#[test]
fn the_attribute_is_addressable_at_its_assignment() {
    let (_tmp, index) = indexed(COUNTER_PY);
    let fields: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.name == "count" && s.kind == fun_refactor::model::SymbolKind::Field)
        .collect();
    assert_eq!(
        fields.len(),
        2,
        "each assignment site declares the attribute. {fields:?}"
    );
}

#[test]
fn renaming_one_site_renames_the_attribute_everywhere() {
    let (tmp, index) = indexed(COUNTER_PY);
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == "count")
        .expect("the attribute")
        .id;
    let plan = rename::plan(&index, id, "total").unwrap();
    let path = tmp.path().join("model.py");
    let edits = plan.edits.edits_for(&path).expect("edits in the file");
    let out = fun_refactor::edit::apply_to_string(COUNTER_PY, edits).unwrap();
    assert!(
        !out.contains("count"),
        "no site keeps the old name, or the object answers two names at run time:\n{out}"
    );
    assert_eq!(
        out.matches("total").count(),
        4,
        "two assignments and two reads:\n{out}"
    );
}

#[test]
fn same_named_attributes_of_two_classes_stay_apart() {
    let source = "class A:\n    def __init__(self) -> None:\n        self.count = 0\n\n\n\
        class B:\n    def __init__(self) -> None:\n        self.count = 9\n";
    let (tmp, index) = indexed(source);
    let first = index
        .symbols
        .iter()
        .find(|s| s.name == "count" && s.qualifier.as_deref() == Some("A"))
        .expect("A's attribute")
        .id;
    let plan = rename::plan(&index, first, "total").unwrap();
    let path = tmp.path().join("model.py");
    let edits = plan.edits.edits_for(&path).expect("edits in the file");
    let out = fun_refactor::edit::apply_to_string(source, edits).unwrap();
    assert!(
        out.contains("self.total = 0") && out.contains("self.count = 9"),
        "the class is the identity:\n{out}"
    );
}
