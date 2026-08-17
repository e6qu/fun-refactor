//! A test declared in the source crosses as the target's own test convention.
//!
//! Zig declares tests beside the code, named in prose. Rust writes `#[test]`,
//! Python a `test_…` function, Go a `TestX(t *testing.T)`; those three loops
//! close, and the readers bring each convention back. TypeScript and Java name
//! no runner in the language itself. Both write a plain function, and the
//! note says what is left to wire up.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

const MATH_ZIG: &str = "fn double(n: i64) i64 {\n    return n * 2;\n}\n\n\
                        test \"double doubles\" {\n    const four = double(2);\n    \
                        try check(four);\n}\n";

#[test]
fn a_zig_test_block_crosses_into_every_target() {
    let (_tmp, root) = workspace(&[("math.zig", MATH_ZIG)]);
    let cases = [
        (Language::Rust, "#[test]"),
        (Language::Rust, "fn double_doubles() {"),
        (Language::Rust, "check(four)?;"),
        (Language::Python, "def test_double_doubles():"),
        (Language::Go, "func TestDoubleDoubles(t *testing.T) {"),
        (Language::Go, "import \"testing\""),
        (
            Language::TypeScript,
            "export function testDoubleDoubles(): void {",
        ),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("math.zig"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
    }
}

#[test]
fn the_typescript_draft_says_what_is_not_wired() {
    let (_tmp, root) = workspace(&[("math.zig", MATH_ZIG)]);
    let plan = transpile::plan(&root.join("math.zig"), Language::TypeScript).expect("a draft");
    assert!(
        plan.fidelity.notes.iter().any(|n| n.contains("runner")),
        "a degraded test must say so: {:?}",
        plan.fidelity.notes
    );
}

#[test]
fn a_rust_test_function_reads_back_as_a_test() {
    let source = "#[test]\nfn doubles() {\n    let four = double(2);\n    check(four);\n}\n";
    let (_tmp, root) = workspace(&[("t.rs", source)]);
    let plan = transpile::plan(&root.join("t.rs"), Language::Zig).expect("a draft");
    assert!(
        plan.output.contains("test \"doubles\" {"),
        "{}",
        plan.output
    );
    assert!(
        !plan.output.contains("#[test]"),
        "the attribute is the mark, not a construct to carry:\n{}",
        plan.output
    );
}

#[test]
fn a_go_test_function_reads_back_as_a_test() {
    let source = "package main\n\nimport \"testing\"\n\nfunc TestDoubles(t *testing.T) {\n\t\
                  _ = t\n\tfour := double(2)\n\tcheck(four)\n}\n";
    let (_tmp, root) = workspace(&[("t.go", source)]);
    let plan = transpile::plan(&root.join("t.go"), Language::Zig).expect("a draft");
    assert!(
        plan.output.contains("test \"Doubles\" {"),
        "{}",
        plan.output
    );
    assert!(
        !plan.output.contains("_ = t"),
        "the runner handle is the convention's plumbing and must not cross:\n{}",
        plan.output
    );
}

#[test]
fn a_test_named_after_a_declaration_crosses_and_steps_aside() {
    // `test double {}` is Zig's doctest form: a test named by the declaration it
    // covers. It crosses as a test. Its name steps aside from the declaration it
    // would otherwise collide with in a target that puts both in one namespace.
    let source = "fn double(n: i64) i64 {\n    return n * 2;\n}\n\ntest double {}\n";
    let (_tmp, root) = workspace(&[("d.zig", source)]);
    let plan = transpile::plan(&root.join("d.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("#[test]") && plan.output.contains("fn test_double() {"),
        "the doctest crosses as a test under a name of its own:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "a test that crossed is not a loss:\n{}",
        plan.output
    );
}
