//! A rewrite that asks for no change has to make none.
//!
//! Running `fr restructure` with the same pattern and template over this repository
//! changed files eight ways out of eight. Three causes, none of them the shape the user
//! asked about: a metavariable bracketed because the template binds it, when the pattern
//! bound it the same way; a template written on one line pulling up a receiver the author
//! put on its own; and a trailing comma dropped from a call.
//!
//! None of the three broke a build. All three wrote to files nobody asked to change,
//! which is how a refactoring tool loses the benefit of the doubt.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::refactor::restructure;
use fun_refactor::scan::{scan, ScanOptions};

/// A file holding each shape that made the identity rewrite move something.
const AWKWARD: &str = "pub fn shapes(v: &[u8], s: &str) -> (usize, usize, Option<String>) {\n    \
     let a = v\n        .iter()\n        .len();\n    \
     let b = \"price * quantity\".len();\n    \
     let c = Some(\n        s.trim().to_string(),\n    );\n    \
     (a, b, c)\n}\n";

fn workspace() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), AWKWARD).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

#[test]
fn an_identity_rewrite_changes_nothing() {
    let (_tmp, index) = workspace();
    for pattern in [
        "$X.len()",
        "$X.iter()",
        "Some($X)",
        "$X.to_string()",
        "$X.trim()",
    ] {
        let plan = restructure::apply(&index, Language::Rust, pattern, pattern)
            .unwrap_or_else(|e| panic!("{pattern}: {e}"));
        assert_eq!(
            plan.edits.edit_count(),
            0,
            "{pattern} rewrote something when the template asks for what it matched"
        );
    }
}

#[test]
fn a_template_that_binds_tighter_still_brackets() {
    // The reason the bracketing exists: `double(a + 1)` becomes `a + 1 * 2` without it,
    // which is a different number.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    let source = "pub fn double(x: i32) -> i32 {\n    x * 2\n}\n\n\
                  pub fn run(a: i32) -> i32 {\n    double(a + 1)\n}\n";
    std::fs::write(tmp.path().join("src/lib.rs"), source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();

    let plan = restructure::apply(&index, Language::Rust, "double($X)", "$X * 2").expect("a plan");
    let file = tmp.path().join("src/lib.rs");
    let after = fun_refactor::edit::apply_to_string(source, plan.edits.edits_for(&file).unwrap())
        .expect("the edits apply");
    assert!(after.contains("(a + 1) * 2"), "{after}");
}

#[test]
fn a_rewrite_that_changes_the_shape_still_happens() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    let source = "pub fn run(v: &[u8]) -> bool {\n    v.len() == 0\n}\n";
    std::fs::write(tmp.path().join("src/lib.rs"), source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();

    let plan = restructure::apply(&index, Language::Rust, "$X.len() == 0", "$X.is_empty()")
        .expect("a plan");
    let file = tmp.path().join("src/lib.rs");
    let after = fun_refactor::edit::apply_to_string(source, plan.edits.edits_for(&file).unwrap())
        .expect("the edits apply");
    assert!(after.contains("v.is_empty()"), "{after}");
}

#[test]
fn a_string_holding_an_operator_is_one_thing() {
    // `"price * quantity"` read as an expression with a `*` in it, so an identity rewrite
    // bracketed it.
    let (_tmp, index) = workspace();
    let plan = restructure::apply(&index, Language::Rust, "$X.len()", "$X.len()").expect("a plan");
    assert_eq!(plan.edits.edit_count(), 0);
}
