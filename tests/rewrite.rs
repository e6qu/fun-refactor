//! Micro-rewrites, exercised on real source.
//!
//! These are the transformations rust-analyzer and gopls offer as code actions. The
//! interesting property is that each one must preserve meaning exactly while changing
//! shape, so the tests compare whole files rather than fragments.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::rewrite::{self, Rewrite};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

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

    let out = rewritten(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::InvertIf,
    );
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
    assert!(
        !out.contains('!') || !out.contains("!(a"),
        "should not wrap: {out}"
    );
}

#[test]
fn invert_if_preserves_comments_in_both_branches() {
    let src = "fn f() {\n    if ready {\n        // then\n        go();\n    } else {\n        // otherwise\n        wait();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::InvertIf,
    );
    assert!(out.contains("// then"), "got:\n{out}");
    assert!(out.contains("// otherwise"), "got:\n{out}");
}

#[test]
fn invert_if_refuses_without_an_else() {
    let src = "fn f() {\n    if ready {\n        go();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = rewrite::apply(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::InvertIf,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("no `else`"), "got: {err}");
}

#[test]
fn invert_if_works_in_python() {
    let src = "def f():\n    if ready:\n        go()\n    else:\n        wait()\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let out = rewritten(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::InvertIf,
    );
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

    let err = rewrite::apply(
        &index,
        &path,
        src.find("!ready").unwrap(),
        Rewrite::DeMorgan,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("not an `and`/`or`"), "got: {err}");
}

#[test]
fn guard_clause_returns_early_instead_of_nesting() {
    let src =
        "fn f() {\n    setup();\n    if ready {\n        go();\n        finish();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::GuardClause,
    );
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

    let err = rewrite::apply(
        &index,
        &path,
        src.find("if ready").unwrap(),
        Rewrite::GuardClause,
    )
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

#[test]
fn invert_if_works_in_bash() {
    // Bash exposes only a `condition` field; its branches are delimited by the
    // `then`, `else` and `fi` keywords with statements sitting bare in between.
    let src = "if [ -n \"$x\" ]; then\n  go\nelse\n  wait\nfi\n";
    let (tmp, index) = workspace(&[("a.sh", src)]);
    let path = tmp.path().join("a.sh");

    let out = rewritten(&index, &path, src.find("if [").unwrap(), Rewrite::InvertIf);
    assert!(out.contains("! [ -n \"$x\" ]"), "got:\n{out}");
    assert!(
        out.find("wait").unwrap() < out.find("go").unwrap(),
        "branches should be swapped:\n{out}"
    );
}

#[test]
fn guard_clause_works_in_bash() {
    let src = "main() {\n  setup\n  if [ -n \"$x\" ]; then\n    go\n  fi\n}\n";
    let (tmp, index) = workspace(&[("a.sh", src)]);
    let path = tmp.path().join("a.sh");

    let out = rewritten(
        &index,
        &path,
        src.find("if [").unwrap(),
        Rewrite::GuardClause,
    );
    assert!(out.contains("! [ -n \"$x\" ]"), "got:\n{out}");
    assert!(out.contains("return"), "got:\n{out}");
}

#[test]
fn bash_rewrites_still_parse() {
    let src = "if [ -n \"$x\" ]; then\n  go\nelse\n  wait\nfi\n";
    let (tmp, index) = workspace(&[("a.sh", src)]);
    let path = tmp.path().join("a.sh");
    let plan = rewrite::apply(&index, &path, src.find("if [").unwrap(), Rewrite::InvertIf).unwrap();
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
        .expect("an inverted shell conditional must still parse");
}

/// Every language `rewrite::supported` claims, with an if/else and a guardable if.
///
/// The gap these close: for a long time only Rust, Python and Bash were exercised,
/// and the matrix published the rest on the strength of the language list alone.
/// TypeScript and Zig were both broken — a negated condition lost the brackets those
/// grammars require — and nothing said so until the tool was run on real code.
const EVERY_LANGUAGE: &[(&str, &str, &str)] = &[
    (
        "a.rs",
        "fn f(a: bool) {\n    if a {\n        go();\n    } else {\n        wait();\n    }\n}\n",
        "fn g(a: bool) {\n    if a {\n        go();\n    }\n}\n",
    ),
    (
        "a.go",
        "package p\n\nfunc f(a bool) {\n\tif a {\n\t\tgo1()\n\t} else {\n\t\twait()\n\t}\n}\n",
        "package p\n\nfunc g(a bool) {\n\tif a {\n\t\tgo1()\n\t}\n}\n",
    ),
    (
        "a.zig",
        "fn f(a: bool) void {\n    if (a) {\n        go();\n    } else {\n        wait();\n    }\n}\n",
        "fn g(a: bool) void {\n    if (a) {\n        go();\n    }\n}\n",
    ),
    (
        "a.ts",
        "function f(a: boolean) {\n  if (a) {\n    go();\n  } else {\n    wait();\n  }\n}\n",
        "function g(a: boolean) {\n  if (a) {\n    go();\n  }\n}\n",
    ),
    (
        "a.tsx",
        "function f(a: boolean) {\n  if (a) {\n    go();\n  } else {\n    wait();\n  }\n}\n",
        "function g(a: boolean) {\n  if (a) {\n    go();\n  }\n}\n",
    ),
    (
        "a.py",
        "def f(a):\n    if a:\n        go()\n    else:\n        wait()\n",
        "def g(a):\n    if a:\n        go()\n",
    ),
    (
        "a.sh",
        "f() {\n  if [ -n \"$x\" ]; then\n    go\n  else\n    wait\n  fi\n}\n",
        "g() {\n  if [ -n \"$x\" ]; then\n    go\n  fi\n}\n",
    ),
];

fn assert_reparses(index: &Index, path: &Path, offset: usize, r: Rewrite, what: &str) {
    let plan = rewrite::apply(index, path, offset, r)
        .unwrap_or_else(|e| panic!("{} on {what}: {e}", r.as_str()));
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
        .unwrap_or_else(|e| {
            panic!(
                "{} on {what} produced code that will not parse: {e}",
                r.as_str()
            )
        });
}

#[test]
fn invert_if_produces_parseable_code_in_every_supported_language() {
    for (name, src, _) in EVERY_LANGUAGE {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let offset = src.find("if ").or_else(|| src.find("if(")).unwrap();
        assert_reparses(&index, &path, offset, Rewrite::InvertIf, name);
    }
}

#[test]
fn guard_clause_produces_parseable_code_in_every_supported_language() {
    for (name, _, src) in EVERY_LANGUAGE {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let offset = src.find("if ").or_else(|| src.find("if(")).unwrap();
        assert_reparses(&index, &path, offset, Rewrite::GuardClause, name);
    }
}

#[test]
fn a_negated_condition_keeps_the_brackets_its_grammar_requires() {
    // Zig and the C family fold the parentheses into the condition node. Negating
    // that node whole yields `if !(a)`, which is valid Rust and invalid everywhere
    // that needs the brackets.
    for (name, src, _) in EVERY_LANGUAGE {
        if !name.ends_with(".ts") && !name.ends_with(".tsx") && !name.ends_with(".zig") {
            continue;
        }
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let out = rewritten(&index, &path, src.find("if (").unwrap(), Rewrite::InvertIf);
        assert!(out.contains("if (!a)"), "{name} lost its brackets:\n{out}");
    }
}

#[test]
fn invert_if_refuses_an_else_if_chain() {
    // Swapping the branches would move the second block out from under a test that
    // only runs when the first is false.
    let cases = [
        (
            "a.rs",
            "fn f(a: bool, b: bool) {\n    if a {\n        go();\n    } else if b {\n        wait();\n    } else {\n        stop();\n    }\n}\n",
        ),
        (
            "a.ts",
            "function f(a: boolean, b: boolean) {\n  if (a) {\n    go();\n  } else if (b) {\n    wait();\n  } else {\n    stop();\n  }\n}\n",
        ),
        (
            "a.py",
            "def f(a, b):\n    if a:\n        go()\n    elif b:\n        wait()\n    else:\n        stop()\n",
        ),
    ];
    for (name, src) in cases {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let err = rewrite::apply(&index, &path, src.find("if ").unwrap(), Rewrite::InvertIf)
            .expect_err(&format!("{name}: an else-if chain must be refused"));
        assert!(
            err.to_string().contains("else if"),
            "{name}: the refusal should name the chain, got: {err}"
        );
    }
}

#[test]
fn de_morgan_keeps_the_grouping_the_result_needs() {
    // `!(a && b)` is one operand of the outer `&&`; `!a || !b` is two. Without new
    // brackets the outer operator rebinds and the meaning changes silently — this
    // one parses cleanly, so no reparse check would catch it.
    let src = "fn f(a: bool, b: bool, x: bool) -> bool {\n    x && !(a && b)\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("!(a").unwrap(), Rewrite::DeMorgan);
    assert!(
        out.contains("x && (!a || !b)"),
        "grouping was dropped:\n{out}"
    );
}

#[test]
fn de_morgan_leaves_a_standalone_negation_unbracketed() {
    let src = "fn f(a: bool, b: bool) {\n    if !(a && b) {\n        go();\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(&index, &path, src.find("!(a").unwrap(), Rewrite::DeMorgan);
    assert!(out.contains("if !a || !b {"), "needless brackets:\n{out}");
}

#[test]
fn guard_clause_indents_the_way_the_file_does() {
    // Two-space TypeScript and tab-indented Go both used to receive four spaces.
    let ts = "function f(a: boolean) {\n  if (a) {\n    go();\n  }\n}\n";
    let (tmp, index) = workspace(&[("a.ts", ts)]);
    let path = tmp.path().join("a.ts");
    let out = rewritten(
        &index,
        &path,
        ts.find("if (").unwrap(),
        Rewrite::GuardClause,
    );
    assert!(
        out.contains("\n    return;\n"),
        "expected a two-space unit:\n{out}"
    );

    let go = "package p\n\nfunc f(a bool) {\n\tif a {\n\t\tgo1()\n\t}\n}\n";
    let (tmp, index) = workspace(&[("a.go", go)]);
    let path = tmp.path().join("a.go");
    let out = rewritten(&index, &path, go.find("if ").unwrap(), Rewrite::GuardClause);
    assert!(out.contains("\n\t\treturn\n"), "expected tabs:\n{out}");
    assert!(
        !out.contains("return;"),
        "Go is not written with the semicolon:\n{out}"
    );
}

#[test]
fn available_only_offers_rewrites_that_survive_a_reparse() {
    for (name, src, _) in EVERY_LANGUAGE {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let offset = src.find("if ").or_else(|| src.find("if(")).unwrap();
        for r in rewrite::available(&index, &path, offset).unwrap() {
            assert_reparses(&index, &path, offset, r, name);
        }
    }
}

#[test]
fn guard_clause_refuses_when_the_if_is_not_last_in_a_go_block() {
    // Go puts a `statement_list` between a block and its statements. Mistaking that
    // wrapper for a statement made every block look like a block of one, so the
    // "is the `if` last?" check passed for an `if` with code after it and the guard
    // hoisted that code out from under the condition. The result parses, so no
    // reparse check would have caught it — only the meaning changes.
    let src = "package p\n\nfunc f(a bool) {\n\tif a {\n\t\tgo1()\n\t}\n\tafter()\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let err = rewrite::apply(
        &index,
        &path,
        src.find("if a").unwrap(),
        Rewrite::GuardClause,
    )
    .expect_err("an `if` with code after it must not become a guard");
    assert!(
        err.to_string().contains("not the last statement"),
        "got: {err}"
    );
}

#[test]
fn guard_clause_still_applies_to_a_trailing_go_if() {
    let src = "package p\n\nfunc f(a bool) {\n\tbefore()\n\tif a {\n\t\tgo1()\n\t}\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let out = rewritten(
        &index,
        &path,
        src.find("if a").unwrap(),
        Rewrite::GuardClause,
    );
    assert!(out.contains("if !a {\n\t\treturn\n\t}\n"), "got:\n{out}");
    assert!(
        out.contains("before()"),
        "the earlier statement was lost:\n{out}"
    );
}

#[test]
fn a_guard_at_the_end_of_a_loop_body_continues_rather_than_returns() {
    // ripgrep's `find_program` ends a `for` body with an `if`. Rewriting that to
    // `return` leaves the loop entirely — a different program — and leaves it with no
    // value in a function returning `Result<PathBuf>`.
    let src = "fn f(paths: Vec<i32>) {\n    for p in paths {\n        if p > 0 {\n            use_it(p);\n        }\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(
        &index,
        &path,
        src.find("if p > 0").unwrap(),
        Rewrite::GuardClause,
    );
    assert!(
        out.contains("continue;"),
        "a loop exits with continue:\n{out}"
    );
    assert!(!out.contains("return"), "not with return:\n{out}");
}

#[test]
fn a_guard_is_refused_where_the_function_owes_a_value() {
    // What to return early is a decision only the author can make, and `return;` in a
    // function returning `Result<PathBuf>` does not compile.
    let src = "fn f(x: i32) -> i32 {\n    if x > 0 {\n        use_it(x);\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = rewrite::apply(
        &index,
        &path,
        src.find("if x > 0").unwrap(),
        Rewrite::GuardClause,
    )
    .expect_err("a value-returning function cannot take a bare early return");
    assert!(err.to_string().contains("returns a value"), "got: {err}");
}

#[test]
fn a_guard_still_applies_where_the_function_returns_nothing() {
    let src = "fn f(x: i32) {\n    if x > 0 {\n        use_it(x);\n    }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let out = rewritten(
        &index,
        &path,
        src.find("if x > 0").unwrap(),
        Rewrite::GuardClause,
    );
    assert!(out.contains("return;"), "got:\n{out}");
}
