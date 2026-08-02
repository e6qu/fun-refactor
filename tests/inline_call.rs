//! Inline call: the strictest refactoring in the set.
//!
//! Every precondition here exists because breaking it changes behaviour while still
//! compiling — the worst possible failure for a refactoring tool. Each one is a
//! refusal that names what blocked it.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::inline;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn apply(plan: &inline::InlineCallPlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn inlines_a_thin_wrapper() {
    let src = "fn double(x: i32) -> i32 { x * 2 }\nfn main() { let y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("double").unwrap() + 1;
    let plan = inline::call(&index, &path, at).unwrap();
    assert_eq!(plan.function, "double");

    let out = apply(&plan, &path);
    assert!(out.contains("let y = (3 * 2);"), "got:\n{out}");
    // The definition is left alone; inlining one call is not deleting the function.
    assert!(out.contains("fn double(x: i32)"), "got:\n{out}");
}

#[test]
fn substitutes_each_argument_for_its_parameter() {
    let src = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() { let s = add(p, q); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("add").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let s = (p + q);"), "got:\n{out}");
}

#[test]
fn parenthesises_so_precedence_is_preserved() {
    // Without brackets `2 + one() * 3` would re-associate and change the answer.
    let src = "fn sum() -> i32 { 1 + 1 }\nfn main() { let n = 2 * sum(); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("sum").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("2 * (1 + 1)"), "got:\n{out}");
}

#[test]
fn a_bare_name_expansion_is_not_parenthesised() {
    let src = "fn get(v: i32) -> i32 { v }\nfn main() { let n = get(x); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("get").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let n = x;"), "got:\n{out}");
}

#[test]
fn refuses_a_multi_statement_body() {
    let src = "fn work(x: i32) -> i32 {\n    let t = x + 1;\n    t * 2\n}\nfn main() { let y = work(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("work").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("single-expression"), "got: {err}");
}

#[test]
fn refuses_to_duplicate_an_effectful_argument() {
    // `x` appears twice in the body, so passing a call would run it twice.
    let src = "fn square(x: i32) -> i32 { x * x }\nfn main() { let y = square(next()); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("square").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("more than once"), "got: {err}");
}

#[test]
fn a_repeated_parameter_with_a_plain_argument_is_fine() {
    let src = "fn square(x: i32) -> i32 { x * x }\nfn main() { let y = square(n); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("square").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let y = (n * n);"), "got:\n{out}");
}

#[test]
fn refuses_when_the_argument_count_disagrees() {
    let src = "fn two(a: i32, b: i32) -> i32 { a + b }\nfn main() { let y = two(1); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("two").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("parameter"), "got: {err}");
}

#[test]
fn refuses_a_recursive_call() {
    let src = "fn loops(n: i32) -> i32 { loops(n) }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("loops").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("not terminate"), "got: {err}");
}

#[test]
fn refuses_when_the_position_is_not_a_call() {
    let src = "fn f() -> i32 { 1 }\nfn main() { let y = 2; }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.find("let y").unwrap() + 4;
    assert!(inline::call(&index, &path, at).is_err());
}

#[test]
fn works_for_python() {
    let src = "def double(x):\n    return x * 2\n\ndef main():\n    y = double(3)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("y = (3 * 2)"), "got:\n{out}");
}

#[test]
fn the_result_still_parses() {
    let src = "fn double(x: i32) -> i32 { x * 2 }\nfn main() { let y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("double").unwrap() + 1;
    let plan = inline::call(&index, &path, at).unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("inlining must not break the file");
    assert_eq!(outcomes.len(), 1);
}

#[test]
fn works_for_go_despite_its_statement_list_wrapper() {
    // tree-sitter-go interposes a `statement_list` between a block and its
    // statements, which once made every Go function look multi-statement.
    let src = "package main\n\nfunc double(x int) int { return x * 2 }\n\nfunc main() { y := double(3); _ = y }\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("y := (3 * 2)"), "got:\n{out}");
}

#[test]
fn works_for_typescript() {
    let src = "function double(x: number) { return x * 2; }\nfunction main() { const y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("const y = (3 * 2);"), "got:\n{out}");
}

#[test]
fn works_for_zig() {
    let src = "fn double(x: i32) i32 { return x * 2; }\npub fn main() void { const y = double(3); _ = y; }\n";
    let (tmp, index) = workspace(&[("a.zig", src)]);
    let path = tmp.path().join("a.zig");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("const y = (3 * 2);"), "got:\n{out}");
}
