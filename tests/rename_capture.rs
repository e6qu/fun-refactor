//! A rename must not move a use under a different declaration of its new name.

use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn symbol_at(index: &Index, source: &str, needle: &str) -> fun_refactor::model::SymbolId {
    let offset = source.find(needle).expect("the needle") + needle.len() - 1;
    index
        .symbols
        .iter()
        .find(|s| s.name_span.contains_offset(offset))
        .expect("a symbol at the needle")
        .id
}

const SHADOWED_RS: &str = "pub fn outer() -> i32 {\n    let value = 1;\n    \
    let result = {\n        let temp = 10;\n        temp + value\n    };\n    result\n}\n";

#[test]
fn renaming_into_an_inner_shadow_refuses() {
    let (_tmp, index) = workspace(&[("lib.rs", SHADOWED_RS)]);
    let id = symbol_at(&index, SHADOWED_RS, "let value");
    let err = rename::plan(&index, id, "temp").unwrap_err();
    assert!(
        err.to_string().contains("would win it"),
        "the refusal names the capture: {err}"
    );
}

#[test]
fn renaming_an_inner_binding_over_an_outer_use_refuses() {
    let (_tmp, index) = workspace(&[("lib.rs", SHADOWED_RS)]);
    let id = symbol_at(&index, SHADOWED_RS, "let temp");
    let err = rename::plan(&index, id, "value").unwrap_err();
    assert!(
        err.to_string().contains("the existing `value`"),
        "the reverse direction is the same hazard: {err}"
    );
}

#[test]
fn a_shadow_whose_scope_holds_no_renamed_use_is_no_capture() {
    // The inner block declares the new name and uses only that; the outer use sits outside it.
    let source = "pub fn outer() -> i32 {\n    let value = 1;\n    let early = value + 1;\n\n    \
        let result = {\n        let temp = 10;\n        temp * 2\n    };\n    early + result\n}\n";
    let (_tmp, index) = workspace(&[("lib.rs", source)]);
    let id = symbol_at(&index, source, "let value");
    let plan = rename::plan(&index, id, "temp").expect("no use sits under the shadow");
    assert!(!plan.edits.is_empty());
}

#[test]
fn a_python_function_scope_capture_refuses_too() {
    let source = "def outer():\n    value = 1\n    def inner():\n        temp = 10\n        \
        return temp + value\n    return inner()\n";
    let (_tmp, index) = workspace(&[("app.py", source)]);
    let id = symbol_at(&index, source, "value");
    let err = rename::plan(&index, id, "temp").unwrap_err();
    assert!(
        err.to_string().contains("would win it"),
        "the same class of bug in Python: {err}"
    );
}
