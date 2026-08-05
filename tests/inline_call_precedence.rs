//! Inlining a call must not change what the call returned.
//!
//! The body binds its parameters at whatever precedence it was written with, and the
//! argument arrives as text. `n * 2` with `x + 1` for `n` is `x + 1 * 2`, which is a
//! different number in every one of these languages.

use fun_refactor::index::Index;
use fun_refactor::refactor::inline;
use fun_refactor::scan::ScanOptions;

fn inlined(file: &str, source: &str, line: usize, column: usize) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let offset = fun_refactor::span::LineIndex::new(source)
        .offset(fun_refactor::span::LineCol { line, col: column }, source)
        .expect("a position inside the file");
    let plan = inline::call(&index, &path, offset).expect("an inline plan");
    fun_refactor::edit::apply_to_string(source, plan.edits.edits_for(&path).expect("edits"))
        .expect("applying")
}

#[test]
fn an_argument_keeps_the_grouping_the_caller_wrote() {
    // Every language in the set, because the substitution is textual and shared. The
    // answer was `x + 1 * 2` in all six.
    for (file, source, line, column, expected) in [
        (
            "a.rs",
            "fn double(n: i32) -> i32 {\n    n * 2\n}\n\nfn use_it(x: i32) -> i32 {\n    \
             double(x + 1)\n}\n",
            6,
            5,
            "((x + 1) * 2)",
        ),
        (
            "a.go",
            "package main\n\nfunc double(n int) int {\n\treturn n * 2\n}\n\n\
             func useIt(x int) int {\n\treturn double(x + 1)\n}\n",
            8,
            9,
            "((x + 1) * 2)",
        ),
        (
            "a.zig",
            "fn double(n: i32) i32 {\n    return n * 2;\n}\n\npub fn useIt(x: i32) i32 {\n    \
             return double(x + 1);\n}\n",
            6,
            12,
            "((x + 1) * 2)",
        ),
        (
            "a.ts",
            "function double(n: number): number {\n    return n * 2;\n}\n\n\
             function useIt(x: number): number {\n    return double(x + 1);\n}\n",
            6,
            12,
            "((x + 1) * 2)",
        ),
        (
            "a.py",
            "def double(n):\n    return n * 2\n\ndef use_it(x):\n    return double(x + 1)\n",
            5,
            12,
            "((x + 1) * 2)",
        ),
        (
            "A.java",
            "public class A {\n    static int twice(int n) {\n        return n * 2;\n    }\n\n    \
             static int useIt(int x) {\n        return twice(x + 1);\n    }\n}\n",
            7,
            16,
            "((x + 1) * 2)",
        ),
    ] {
        let after = inlined(file, source, line, column);
        assert!(
            after.contains(expected),
            "{file} produced:\n{after}\nwanted it to contain `{expected}`"
        );
    }
}

#[test]
fn an_expansion_of_two_groups_is_still_grouped() {
    // `(p + 1) / (q - 1)` starts with a bracket and ends with one, and the check for
    // "already bracketed" read only those two characters. So the expansion went in
    // bare: `2 * (p + 1) / (q - 1)`, which for p = 1, q = 4 is 1 where the call
    // returned 0.
    let after = inlined(
        "d.rs",
        "fn scale(a: i32, b: i32) -> i32 {\n    a / b\n}\n\nfn u(p: i32, q: i32) -> i32 {\n    \
         2 * scale(p + 1, q - 1)\n}\n",
        6,
        9,
    );
    assert!(after.contains("2 * ((p + 1) / (q - 1))"), "{after}");
}

#[test]
fn an_atomic_argument_gains_no_brackets() {
    // Grouping everything would be safe and unreadable. A name, a literal, a call and
    // an already-bracketed expression need nothing.
    let after = inlined(
        "e.rs",
        "fn double(n: i32) -> i32 {\n    n * 2\n}\n\nfn u(x: i32) -> i32 {\n    \
         double(x)\n}\n",
        6,
        5,
    );
    assert!(after.contains("(x * 2)"), "{after}");
    assert!(!after.contains("((x) * 2)"), "{after}");
}

#[test]
fn a_unary_body_groups_its_argument() {
    let after = inlined(
        "f.rs",
        "fn neg(n: i32) -> i32 {\n    -n\n}\n\nfn u(p: i32, q: i32) -> i32 {\n    \
         neg(p - q)\n}\n",
        6,
        5,
    );
    assert!(after.contains("(-(p - q))"), "{after}");
}
