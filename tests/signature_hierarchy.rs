//! A signature change and a delete follow the dispatch family.
//!
//! A trait method with one more parameter than its implementations is a
//! family answering two shapes, and the callers compile against neither. So
//! `fr signature` changes the declaration, every implementation, and the
//! dispatch sites that resolve to no single one of them. `fr delete`
//! removes the family whole. The receiver is not an addressable parameter:
//! a call never passes it, and counting it put every position out by one.

use fun_refactor::index::Index;
use fun_refactor::refactor::{delete, signature};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn method_at(index: &Index, source: &str, needle: &str) -> fun_refactor::model::SymbolId {
    let offset = source.find(needle).expect("the needle") + needle.len() - 1;
    index
        .symbols
        .iter()
        .find(|s| s.name_span.contains_offset(offset))
        .expect("a symbol at the needle")
        .id
}

fn applied(root: &Path, file: &str, edits: &fun_refactor::edit::EditSet) -> String {
    let path = root.join(file);
    let before = std::fs::read_to_string(&path).unwrap();
    match edits.edits_for(&path) {
        Some(for_file) => fun_refactor::edit::apply_to_string(&before, for_file).unwrap(),
        None => before,
    }
}

const SHAPES_RS: &str = "pub trait Shape {\n    fn area(&self, scale: f64) -> f64;\n}\n\n\
    pub struct Circle {\n    pub radius: f64,\n}\n\n\
    impl Shape for Circle {\n    fn area(&self, scale: f64) -> f64 {\n        \
    self.radius * scale\n    }\n}\n\n\
    pub fn total(shapes: &[Box<dyn Shape>]) -> f64 {\n    \
    shapes.iter().map(|s| s.area(2.0)).sum()\n}\n";

#[test]
fn adding_a_parameter_changes_the_whole_family() {
    let (tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let id = method_at(&index, SHAPES_RS, "fn area");
    let plan = signature::change(
        &index,
        id,
        signature::Change::Add {
            at: 1,
            declaration: "precision: i64".into(),
            argument: "3".into(),
        },
    )
    .unwrap();
    let out = applied(tmp.path(), "shapes.rs", &plan.edits);
    assert_eq!(
        out.matches("precision: i64").count(),
        2,
        "declaration and implementation change together.\n{out}"
    );
    assert!(
        out.contains("s.area(2.0, 3)"),
        "the dispatch site takes the declared argument.\n{out}"
    );
    assert!(
        plan.notes.iter().any(|n| n.contains("family")),
        "the family members are named in the notes. {:?}",
        plan.notes
    );
}

#[test]
fn the_receiver_is_not_an_addressable_parameter() {
    let (_tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let id = method_at(&index, SHAPES_RS, "fn area");
    // Position 0 is `scale`, and the implementation's body still reads it, so
    // the refusal names `scale` and never touches `&self`.
    let err = signature::change(&index, id, signature::Change::Remove(0))
        .unwrap_err()
        .to_string();
    assert!(err.contains("scale"), "got: {err}");
    assert!(!err.contains("self"), "got: {err}");
}

#[test]
fn deleting_the_trait_method_deletes_the_implementation_too() {
    let (tmp, index) = workspace(&[("shapes.rs", SHAPES_RS)]);
    let id = method_at(&index, SHAPES_RS, "fn area");
    let plan = delete::plan(&index, id).unwrap();
    let out = applied(tmp.path(), "shapes.rs", &plan.edits);
    assert!(
        !out.contains("fn area"),
        "the family goes as a whole.\n{out}"
    );
    assert!(
        out.contains("impl Shape for Circle"),
        "the impl block itself stays.\n{out}"
    );
}

#[test]
fn two_unrelated_methods_sharing_a_name_stay_apart() {
    // Java's reachability tier fans a call out by name alone; a *change* must
    // not merge on that evidence.
    let source =
        "public class Widths {\n    public static int width(byte[] items, int n) {\n        \
        return items.length * n;\n    }\n}\n\n\
        class Holder {\n    byte[] items = new byte[4];\n\n    public int width(int n) {\n        \
        return Widths.width(items, n);\n    }\n}\n";
    let (tmp, index) = workspace(&[("Widths.java", source)]);
    let id = method_at(&index, source, "static int width");
    let plan = signature::change(&index, id, signature::Change::Move { from: 0, to: 1 }).unwrap();
    let out = applied(tmp.path(), "Widths.java", &plan.edits);
    assert!(
        out.contains("public int width(int n)"),
        "the one-parameter stranger is untouched.\n{out}"
    );
    assert!(
        out.contains("static int width(int n, byte[] items)"),
        "the addressed method changes alone.\n{out}"
    );
}

#[test]
fn a_function_held_as_a_value_refuses_the_change() {
    // `let f: fn(i32, i32) -> i32 = add;` has no argument list to rewrite, and a
    // changed `add` no longer matches the binding's type. This site was silently
    // skipped once, and the command reported one clean call site while the build
    // broke on the binding.
    let source = "fn add(a: i32, _unused: i32) -> i32 {\n    a\n}\n\n\
        pub fn run() -> i32 {\n    let f: fn(i32, i32) -> i32 = add;\n    \
        let direct = add(1, 2);\n    f(3, 4) + direct\n}\n";
    let (_tmp, index) = workspace(&[("held.rs", source)]);
    let id = method_at(&index, source, "fn add");
    let err = signature::change(&index, id, signature::Change::Remove(1)).unwrap_err();
    assert!(
        err.to_string().contains("used as a value"),
        "the refusal names the binding: {err}"
    );
}

#[test]
fn a_call_naming_its_arguments_loses_only_the_one_being_removed() {
    // Position says nothing about which argument is which once a call names
    // them. Removing parameter 1 took `loud=True` out of `greet("b",
    // loud=True)`, and the refusal for the same shape blamed "the body of
    // `greet`" while pointing at a call site.
    let source = "def greet(name: str, punct: str = \"!\", loud: bool = False) -> str:\n    \
        text = name\n    return text.upper() if loud else text\n\n\n\
        print(greet(\"a\"))\nprint(greet(\"b\", loud=True))\n\
        print(greet(\"c\", punct=\"-\", loud=True))\n";
    let (tmp, index) = workspace(&[("defs.py", source)]);
    let id = method_at(&index, source, "def greet");
    let plan = signature::change(&index, id, signature::Change::Remove(1)).expect("a plan");
    let out = applied(tmp.path(), "defs.py", &plan.edits);
    assert!(
        out.contains("def greet(name: str, loud: bool = False)"),
        "the declaration loses the parameter.\n{out}"
    );
    assert!(
        out.contains("print(greet(\"b\", loud=True))"),
        "a call that never passed it keeps what it did pass.\n{out}"
    );
    assert!(
        out.contains("print(greet(\"c\", loud=True))"),
        "a call that named it loses that argument alone.\n{out}"
    );
}

#[test]
fn a_body_that_reads_the_parameter_still_refuses() {
    let source = "def greet(name: str, punct: str = \"!\") -> str:\n    \
        return name + punct\n\n\nprint(greet(\"a\"))\n";
    let (tmp, index) = workspace(&[("defs.py", source)]);
    let _ = &tmp;
    let id = method_at(&index, source, "def greet");
    let err = signature::change(&index, id, signature::Change::Remove(1))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("still reads `punct`"),
        "the body is the one place a removal cannot repair: {err}"
    );
}
