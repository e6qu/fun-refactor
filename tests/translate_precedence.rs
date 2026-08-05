//! Arithmetic that means the same thing on the other side.
//!
//! A translation preserves a signature; the point of preserving one is that the body
//! still computes what it computed. Every writer rendered a binary expression as
//! `left op right` and nothing else, so a group the source wrote was a group the
//! translation lost — in all six targets at once.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const TARGETS: &[Language] = &[
    Language::Python,
    Language::TypeScript,
    Language::Go,
    Language::Java,
    Language::Zig,
];

fn translated(source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("a.rs");
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

#[test]
fn a_group_the_source_wrote_survives() {
    for (body, expected) in [
        ("return (a + b) * c;", "(a + b) * c"),
        ("return a - (b - c);", "a - (b - c)"),
        ("return a / (b * c);", "a / (b * c)"),
    ] {
        let source = format!("pub fn f(a: i64, b: i64, c: i64) -> i64 {{\n    {body}\n}}\n");
        for target in TARGETS {
            let out = translated(&source, *target);
            assert!(
                out.contains(expected),
                "{target} lost the grouping in `{body}`:\n{out}"
            );
        }
    }
}

#[test]
fn a_group_that_was_never_needed_does_not_survive() {
    // Brackets are decided from precedence, not copied from the source, so the result
    // is right where the two languages disagree — and tidy where they agree.
    for (body, unwanted) in [
        ("return (a - b) - c;", "(a - b)"),
        ("return a * b + c;", "("),
        ("return (a * b) + c;", "("),
    ] {
        let source = format!("pub fn f(a: i64, b: i64, c: i64) -> i64 {{\n    {body}\n}}\n");
        for target in TARGETS {
            let out = translated(&source, *target);
            let line = out
                .lines()
                .find(|l| l.trim_start().starts_with("return "))
                .unwrap_or_else(|| panic!("no return in {target} output:\n{out}"));
            assert!(
                !line.contains(unwanted),
                "{target} bracketed `{body}` needlessly: {line}"
            );
        }
    }
}

#[test]
fn a_negation_keeps_what_it_negates() {
    // `!(a && b)` is not `!a && b`, which is the whole of De Morgan's law and the
    // reason it is a refactoring in its own right.
    let source = "pub fn f(a: bool, b: bool) -> bool {\n    return !(a && b);\n}\n";
    for target in TARGETS {
        let out = translated(source, *target);
        let expected = match target {
            Language::Python => "not (a and b)",
            Language::Zig => "!(a and b)",
            _ => "!(a && b)",
        };
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn the_tail_of_a_rust_function_is_its_return() {
    // The ordinary way to write a Rust function. Reading the tail as a plain statement
    // dropped the return in every target at once: Python returned `None`, Zig said
    // `_ = a + b;`, and Go, Java and TypeScript did not compile — each still declaring
    // the return type the signature carried across.
    let source = "pub fn f(a: i64, b: i64) -> i64 {\n    a + b\n}\n";
    for target in TARGETS {
        let out = translated(source, *target);
        assert!(out.contains("return a + b"), "{target}:\n{out}");
    }
}

#[test]
fn a_tail_after_other_statements_is_still_the_return() {
    let source = "pub fn f(a: i64) -> i64 {\n    let b = a + 1;\n    b * 2\n}\n";
    for target in TARGETS {
        let out = translated(source, *target);
        assert!(out.contains("return b * 2"), "{target}:\n{out}");
    }
}

#[test]
fn a_body_that_ends_in_a_statement_gains_no_return() {
    // A trailing `if` is already a statement. Turning it into a return would be a
    // different kind of wrong.
    let source = "pub fn f(a: i64) -> i64 {\n    if a > 0 {\n        return 1;\n    }\n    0\n}\n";
    let out = translated(source, Language::Python);
    assert!(out.contains("return 1"), "{out}");
    assert!(out.contains("return 0"), "{out}");
}
