//! `hoist-function`: a nested function moved to module scope.

use fun_refactor::edit::{apply_to_string, plan, Validation};
use fun_refactor::index::Index;
use fun_refactor::refactor::rewrite::{self, Rewrite};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

fn workspace(name: &str, src: &str) -> (tempfile::TempDir, PathBuf, Index) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(name);
    std::fs::write(&path, src).expect("write");
    let scanned = scan(tmp.path(), &ScanOptions::default()).expect("scan");
    let index = Index::build_from_scan(&scanned).expect("index");
    (tmp, path, index)
}

fn hoisted(name: &str, src: &str, at: usize) -> String {
    let (_tmp, path, index) = workspace(name, src);
    let result =
        rewrite::apply(&index, &path, at, Rewrite::HoistFunction).expect("the hoist applies");
    plan(&result.edits, Validation::ReparseStrict).expect("the result reparses");
    let edits = result.edits.edits_for(&path).expect("edits for the file");
    apply_to_string(src, edits).expect("apply")
}

const NESTED: &str = "\
fn outer() -> bool {
    /// The doc travels with the function.
    fn helper(x: u8) -> bool {
        x > 1
    }
    helper(2)
}

fn other() {}
";

#[test]
fn a_nested_function_moves_to_module_scope() {
    let out = hoisted("a.rs", NESTED, NESTED.find("helper").unwrap());
    assert!(
        out.contains("fn outer() -> bool {\n    helper(2)\n}"),
        "the body keeps only the call: {out}"
    );
    assert!(
        out.contains("\nfn helper(x: u8) -> bool {\n    x > 1\n}"),
        "the function sits at module scope, re-indented: {out}"
    );
}

#[test]
fn the_doc_comment_travels_with_it() {
    let out = hoisted("a.rs", NESTED, NESTED.find("helper").unwrap());
    assert!(
        out.contains("/// The doc travels with the function.\nfn helper"),
        "{out}"
    );
    assert_eq!(
        out.matches("The doc travels").count(),
        1,
        "moved, not copied: {out}"
    );
}

#[test]
fn a_module_level_function_is_not_hoisted_further() {
    let (_tmp, path, index) = workspace("a.rs", NESTED);
    let refusal = rewrite::apply(
        &index,
        &path,
        NESTED.find("other").unwrap(),
        Rewrite::HoistFunction,
    )
    .expect_err("already at module scope");
    assert!(
        refusal.to_string().contains("already at module scope"),
        "{refusal}"
    );
}

#[test]
fn a_name_the_module_already_defines_is_refused() {
    let src = "\
fn outer() -> bool {
    fn other() -> bool {
        true
    }
    other()
}

fn other() {}
";
    let (_tmp, path, index) = workspace("a.rs", src);
    let refusal = rewrite::apply(
        &index,
        &path,
        src.find("fn other() -> bool").unwrap() + 4,
        Rewrite::HoistFunction,
    )
    .expect_err("two module-level `other`s would collide");
    assert!(refusal.to_string().contains("collide"), "{refusal}");
}

#[test]
fn a_language_where_inner_functions_capture_is_refused_with_the_reason() {
    // `def helper(): return limit` reads `limit` from the enclosing scope.
    let src = "def outer(limit):\n    def helper():\n        return limit\n    return helper()\n";
    let (_tmp, path, index) = workspace("a.py", src);
    let refusal = rewrite::apply(
        &index,
        &path,
        src.find("helper").unwrap(),
        Rewrite::HoistFunction,
    )
    .expect_err("capture makes the move unsafe");
    assert!(refusal.to_string().contains("capture"), "{refusal}");
}

#[test]
fn the_menu_offers_it_only_where_it_applies() {
    let (_tmp, path, index) = workspace("a.rs", NESTED);
    let inside = rewrite::available(&index, &path, NESTED.find("x > 1").unwrap()).expect("a menu");
    assert!(
        inside.contains(&Rewrite::HoistFunction),
        "inside the nested function: {inside:?}"
    );
    let outside = rewrite::available(&index, &path, NESTED.find("other").unwrap()).expect("a menu");
    assert!(
        !outside.contains(&Rewrite::HoistFunction),
        "at module scope: {outside:?}"
    );
}
