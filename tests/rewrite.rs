//! Micro-rewrites, exercised on real source.
//!
//! These are the transformations rust-analyzer and gopls offer as code actions. The
//! interesting property is that each one must preserve meaning exactly while changing
//! shape, so the tests compare whole files rather than fragments.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::rewrite::{self, Rewrite};
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

fn rewritten(index: &Index, path: &PathBuf, offset: usize, r: Rewrite) -> String {
    let plan = rewrite::apply(index, path, offset, r)
        .unwrap_or_else(|e| panic!("{} failed: {e}", r.as_str()));
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn invert_if_swaps_branches_and_negates() {
    let src = "fn f() {\n    if ready {\n        go();\n    } else {\n        wait();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("if ready").unwrap(), Rewrite::InvertIf);
    assert_eq!(
        out,
        "fn f() {\n    if !ready {\n        wait();\n    } else {\n        go();\n    }\n}\n"
    );
}

#[test]
fn invert_if_flips_a_comparison_instead_of_adding_a_bang() {
    let src = "fn f() {\n    if a == b {\n        x();\n    } else {\n        y();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("if a").unwrap(), Rewrite::InvertIf);
    assert!(out.contains("if a != b {"), "got:\n{out}");
    assert!(!out.contains('!' ) || !out.contains("!(a"), "should not wrap: {out}");
}

#[test]
fn invert_if_preserves_comments_in_both_branches() {
    let src = "fn f() {\n    if ready {\n        // then\n        go();\n    } else {\n        // otherwise\n        wait();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("if ready").unwrap(), Rewrite::InvertIf);
    assert!(out.contains("// then"), "got:\n{out}");
    assert!(out.contains("// otherwise"), "got:\n{out}");
}

#[test]
fn invert_if_refuses_without_an_else() {
    let src = "fn f() {\n    if ready {\n        go();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = rewrite::apply(&index, &path, src.find("if ready").unwrap(), Rewrite::InvertIf)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no `else`"), "got: {err}");
}

#[test]
fn invert_if_works_in_python() {
    let src = "def f():\n    if ready:\n        go()\n    else:\n        wait()\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let out = rewritten(&index, &path, src.find("if ready").unwrap(), Rewrite::InvertIf);
    assert!(out.contains("if not ready:"), "got:\n{out}");
    assert!(
        out.find("wait()").unwrap() < out.find("go()").unwrap(),
        "branches should be swapped:\n{out}"
    );
}

#[test]
fn de_morgan_distributes_over_and() {
    let src = "fn f() {\n    let x = !(a && b);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("!(a").unwrap(), Rewrite::DeMorgan);
    assert!(out.contains("let x = !a || !b;"), "got:\n{out}");
}

#[test]
fn de_morgan_distributes_over_or_and_flips_comparisons() {
    let src = "fn f() {\n    let x = !(a == b || c < d);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("!(a").unwrap(), Rewrite::DeMorgan);
    assert!(out.contains("a != b && c >= d"), "got:\n{out}");
}

#[test]
fn de_morgan_refuses_a_plain_negation() {
    let src = "fn f() {\n    let x = !ready;\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = rewrite::apply(&index, &path, src.find("!ready").unwrap(), Rewrite::DeMorgan)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not an `and`/`or`"), "got: {err}");
}

#[test]
fn guard_clause_returns_early_instead_of_nesting() {
    let src = "fn f() {\n    setup();\n    if ready {\n        go();\n        finish();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("if ready").unwrap(), Rewrite::GuardClause);
    assert!(out.contains("if !ready {"), "got:\n{out}");
    assert!(out.contains("return;"), "got:\n{out}");
    assert!(out.contains("    go();"), "body should be dedented:\n{out}");
    assert!(!out.contains("        go();"), "body still nested:\n{out}");
}

#[test]
fn guard_clause_refuses_when_statements_follow() {
    // An early return would skip them, changing behaviour.
    let src = "fn f() {\n    if ready {\n        go();\n    }\n    cleanup();\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = rewrite::apply(&index, &path, src.find("if ready").unwrap(), Rewrite::GuardClause)
        .unwrap_err()
        .to_string();
    assert!(err.contains("skip"), "got: {err}");
}

#[test]
fn available_lists_what_applies_here() {
    let src = "fn f() {\n    if ready {\n        go();\n    } else {\n        wait();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let options = rewrite::available(&index, &path, src.find("if ready").unwrap()).unwrap();
    assert!(options.contains(&Rewrite::InvertIf), "got {options:?}");
    // With an else present, a guard clause is not on offer.
    assert!(!options.contains(&Rewrite::GuardClause), "got {options:?}");
}

#[test]
fn available_is_empty_where_nothing_applies() {
    let src = "fn f() {\n    let x = 1;\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let options = rewrite::available(&index, &path, src.find("let x").unwrap()).unwrap();
    assert!(options.is_empty(), "got {options:?}");
}

#[test]
fn every_rewrite_leaves_the_file_parsing() {
    let cases: &[(&str, &str, &str, Rewrite)] = &[
        (
            "a.rs",
            "fn f() {\n    if ready {\n        go();\n    } else {\n        wait();\n    }\n}\n",
            "if ready",
            Rewrite::InvertIf,
        ),
        (
            "b.rs",
            "fn f() {\n    let x = !(a && b);\n}\n",
            "!(a",
            Rewrite::DeMorgan,
        ),
        (
            "c.rs",
            "fn f() {\n    setup();\n    if ready {\n        go();\n    }\n}\n",
            "if ready",
            Rewrite::GuardClause,
        ),
    ];

    for (name, src, needle, r) in cases {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let plan = rewrite::apply(&index, &path, src.find(needle).unwrap(), *r).unwrap();
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .unwrap_or_else(|e| panic!("{} broke {name}: {e}", r.as_str()));
    }
}

#[test]
fn refuses_unsupported_languages() {
    let (tmp, index) = workspace(&[("style.css", ".a { color: red; }\n")]);
    let path = tmp.path().join("style.css");
    let err = rewrite::apply(&index, &path, 0, Rewrite::InvertIf).unwrap_err();
    assert!(
        err.downcast_ref::<fun_refactor::refactor::Refusal>()
            .is_some(),
        "got: {err}"
    );
}
