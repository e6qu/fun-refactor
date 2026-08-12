//! A captured expression keeps its meaning where the template puts it.
//!
//! The third place in this tool where an expression is moved into a context it was not
//! written for. `inline` was fixed for it twice; `restructure` does the same thing by
//! design, the whole point is to move code shapes around, and did not.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::refactor::restructure;
use fun_refactor::scan::ScanOptions;

fn restructured(
    file: &str,
    source: &str,
    language: Language,
    pattern: &str,
    template: &str,
) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let plan = restructure::apply(&index, language, pattern, template).expect("a plan");
    match plan.edits.edits_for(&path) {
        Some(edits) => fun_refactor::edit::apply_to_string(source, edits).expect("applying"),
        None => source.to_string(),
    }
}

#[test]
fn a_captured_expression_keeps_its_grouping() {
    // `$X * 2` reads as "the captured expression, times two". Substituting the text of
    // `x + 1` gives `x + 1 * 2`, which is `x + 2`.
    for (file, source, language, expected) in [
        (
            "a.rs",
            "fn f(x: i32) -> i32 {\n    double(x + 1)\n}\n",
            Language::Rust,
            "(x + 1) * 2",
        ),
        (
            "a.py",
            "def f(x):\n    return double(x + 1)\n",
            Language::Python,
            "(x + 1) * 2",
        ),
        (
            "a.ts",
            "function f(x: number) {\n    return double(x + 1);\n}\n",
            Language::TypeScript,
            "(x + 1) * 2",
        ),
    ] {
        let after = restructured(file, source, language, "double($X)", "$X * 2");
        assert!(after.contains(expected), "{file}: {after}");
    }
}

#[test]
fn an_atomic_capture_gains_nothing() {
    // Bracketing everything would be safe and unreadable.
    let after = restructured(
        "a.rs",
        "fn f(y: i32) -> i32 {\n    double(y)\n}\n",
        Language::Rust,
        "double($X)",
        "$X * 2",
    );
    assert!(after.contains("y * 2"), "{after}");
    assert!(!after.contains("(y) * 2"), "{after}");
}

#[test]
fn a_capture_in_an_argument_gains_nothing() {
    // The template puts `$X` where a delimiter already holds it. This is the common
    // case and it must stay clean.
    let after = restructured(
        "a.rs",
        "fn f(x: i32) -> i32 {\n    old_api(x + 1)\n}\n",
        Language::Rust,
        "old_api($X)",
        "new_api($X, None)",
    );
    assert!(after.contains("new_api(x + 1, None)"), "{after}");
}

#[test]
fn a_replacement_that_binds_is_grouped_where_it_lands() {
    // The other half: the match sat where a call sat, and the template is an operator
    // expression. So whatever the call was an operand of now binds into it. `2 * double(y)` →
    // `2 * y / 2` is not `2 * (y / 2)` for integers.
    let after = restructured(
        "a.rs",
        "fn f(y: i32) -> i32 {\n    2 * double(y)\n}\n",
        Language::Rust,
        "double($X)",
        "$X / 2",
    );
    assert!(after.contains("2 * (y / 2)"), "{after}");
}

#[test]
fn a_language_that_does_not_group_with_brackets_gets_none() {
    // A CSS selector's parent is an `attribute_selector` or a `descendant_selector`, which read
    // as operator kinds by name and are nothing of the sort. Bracketing there is not a
    // grouping, it is a syntax error. The reparse guard caught it, which is how this was found
    // before it shipped.
    let after = restructured(
        "a.css",
        ".old-name {\n  color: red;\n}\n",
        Language::Css,
        ".old-name",
        ".new-name",
    );
    assert!(after.contains(".new-name {"), "{after}");
    assert!(!after.contains('('), "{after}");
}
