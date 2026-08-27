//! Error propagation crosses the language boundary.

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

const READER_RS: &str = "fn read_all(path: &str) -> Result<i64, ParseError> {\n    \
                         let n = parse(path)?;\n    Ok(n)\n}\n";

#[test]
fn a_rust_question_mark_becomes_zig_try() {
    let (_tmp, root) = workspace(&[("reader.rs", READER_RS)]);
    let plan = transpile::plan(&root.join("reader.rs"), Language::Zig).expect("a draft");
    assert!(
        plan.output.contains("const n = try parse(path);"),
        "{}",
        plan.output
    );
}

#[test]
fn a_zig_try_becomes_a_rust_question_mark() {
    let source = "fn readAll(path: []const u8) !i64 {\n    const n = try parse(path);\n    \
                  return n;\n}\n";
    let (_tmp, root) = workspace(&[("loader.zig", source)]);
    let plan = transpile::plan(&root.join("loader.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("let n = parse(path)?;"),
        "{}",
        plan.output
    );
}

#[test]
fn python_writes_the_expression_bare_and_says_so_once() {
    let (_tmp, root) = workspace(&[("reader.rs", READER_RS)]);
    let plan = transpile::plan(&root.join("reader.rs"), Language::Python).expect("a draft");
    assert!(plan.output.contains("n = parse(path)"), "{}", plan.output);
    assert!(
        !plan.output.contains(transpile::MARKER),
        "propagation is not a loss where errors propagate on their own.\n{}",
        plan.output
    );
    let said: Vec<&String> = plan
        .fidelity
        .notes
        .iter()
        .filter(|n| n.contains("propagates on its own"))
        .collect();
    assert_eq!(
        said.len(),
        1,
        "said once, not per site: {:?}",
        plan.fidelity.notes
    );
}

#[test]
fn go_writes_propagation_as_the_error_check_it_means() {
    let (_tmp, root) = workspace(&[("reader.rs", READER_RS)]);
    let plan = transpile::plan(&root.join("reader.rs"), Language::Go).expect("a draft");
    for expected in [
        "func readAll(path string) (int, error) {",
        "n, err := parse(path)",
        "if err != nil {",
        "return 0, err",
        "return n, nil",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
    assert!(
        !plan.output.contains(transpile::MARKER),
        "the early return is Go's own idiom, not a loss.\n{}",
        plan.output
    );
}

#[test]
fn zig_call_arguments_survive_translation() {
    // The grammar hangs arguments off the call with no argument-list node, and the
    // reader that looked for one read every call as nullary.
    let source = "fn twice(n: i64) i64 {\n    return n * 2;\n}\n\n\
                  pub fn run(x: i64, y: i64) i64 {\n    const a = twice(x);\n    \
                  const b = twice(y);\n    return a + b;\n}\n";
    let (_tmp, root) = workspace(&[("calls.zig", source)]);
    let plan = transpile::plan(&root.join("calls.zig"), Language::Python).expect("a draft");
    // `fn twice` is not `pub`, so Python marks it module-private, at its
    // declaration and at every call.
    assert!(plan.output.contains("a = _twice(x)"), "{}", plan.output);
    assert!(plan.output.contains("b = _twice(y)"), "{}", plan.output);
    assert!(plan.output.contains("def _twice("), "{}", plan.output);
}
