//! The local rewrites, and the meaning they must not change.
//!
//! `invert-if` swaps the branches and negates the condition. Negating *half* of a
//! condition and swapping the branches anyway is a different program — one that
//! compiles, passes the parse check, and answers differently.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::rewrite::{self, Rewrite};
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

fn workspace(name: &str, source: &str) -> (tempfile::TempDir, PathBuf, Index) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(name);
    std::fs::write(&path, source).expect("writing the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    (tmp, path, index)
}

fn applied(name: &str, source: &str, at: usize, which: Rewrite) -> String {
    let (_tmp, path, index) = workspace(name, source);
    let plan =
        rewrite::apply(&index, &path, at, which).unwrap_or_else(|e| panic!("{which:?}: {e}"));
    apply_to_string(source, plan.edits.edits_for(&path).expect("edits")).expect("applying")
}

#[test]
fn inverting_a_compound_condition_negates_all_of_it() {
    // `a == 1 and b == 2` negated by flipping the first `==` is `a != 1 and b == 2`,
    // which is a different program: the negation of an `and` is an `or` of the
    // negations, and flipping one operand cannot say that.
    for (name, source, expected) in [
        (
            "a.py",
            "def f(a, b):\n    if a == 1 and b == 2:\n        return 1\n    else:\n        return 2\n",
            "if not (a == 1 and b == 2):",
        ),
        (
            "a.zig",
            "pub fn f(a: i64, b: i64) i64 {\n    if (a == 1 and b == 2) {\n        return 1;\n    } else {\n        return 2;\n    }\n}\n",
            "if (!(a == 1 and b == 2)) {",
        ),
        (
            "a.rs",
            "pub fn f(a: i64, b: i64) -> i64 {\n    if a == 1 && b == 2 {\n        return 1;\n    } else {\n        return 2;\n    }\n}\n",
            "if !(a == 1 && b == 2) {",
        ),
    ] {
        let at = source.find("if ").or_else(|| source.find("if (")).expect("the if");
        let after = applied(name, source, at, Rewrite::InvertIf);
        assert!(after.contains(expected), "{name}:\n{after}");
    }
}

#[test]
fn a_lone_comparison_still_flips() {
    // The simplification is the point of the rewrite wherever it is sound.
    let source = "def f(a):\n    if a == 1:\n        return 1\n    else:\n        return 2\n";
    let at = source.find("if ").expect("the if");
    let after = applied("b.py", source, at, Rewrite::InvertIf);
    assert!(after.contains("if a != 1:"), "{after}");
}

#[test]
fn a_comparison_inside_a_call_is_not_the_conditions_own() {
    // `g(a == 1) == 2` has two `==` in it, and only one of them is the comparison the
    // condition makes.
    let source =
        "def f(a):\n    if g(a == 1) == 2:\n        return 1\n    else:\n        return 2\n";
    let at = source.find("if ").expect("the if");
    let after = applied("c.py", source, at, Rewrite::InvertIf);
    assert!(after.contains("if g(a == 1) != 2:"), "{after}");
}

#[test]
fn an_if_that_binds_what_it_tested_is_refused() {
    // Zig writes `if (maybe) |value| { … }`: the condition is an optional and the
    // payload binds what was inside it. `if (!maybe) |value|` is not a program.
    let source = "pub fn f(s: S, uri: U) void {\n    if (s.get(uri)) |old| {\n        \
                  old.deinit();\n    } else {\n        return;\n    }\n}\n";
    let (_tmp, path, index) = workspace("d.zig", source);
    let at = source.find("if (").expect("the if");
    for which in [Rewrite::InvertIf, Rewrite::GuardClause] {
        let refusal =
            rewrite::apply(&index, &path, at, which).expect_err("an `if` that binds a payload");
        assert!(
            refusal.to_string().contains("binds what it tested"),
            "{refusal}"
        );
    }
    assert!(
        rewrite::available(&index, &path, at)
            .expect("a listing")
            .is_empty(),
        "and nothing is offered there either"
    );
}

#[test]
fn zig_spells_its_boolean_operators_as_words() {
    // Falling into the C arm made `a and b` invisible to every rule that looks for an
    // operator — and `!(a and b)` is an `error_union_type` in that grammar, because
    // `!T` is an error union where a type is expected and a negation where a value is.
    let source = "pub fn f(a: bool, b: bool) i64 {\n    if (!(a and b)) {\n        \
                  return 1;\n    }\n    return 2;\n}\n";
    let at = source.find("!(").expect("the negation");
    let after = applied("e.zig", source, at, Rewrite::DeMorgan);
    assert!(after.contains("if (!a or !b) {"), "{after}");
}
