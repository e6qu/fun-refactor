//! A marker is a draft's honesty, and a draft must still compile around it.
//!
//! Three markers did not. Go's inline stand-in was a bare `nil`, which `:=` has
//! no type to infer for. Rust's `todo!` interpolated braces from the carried
//! source into its own format string. And a `todo!` in a `const` is not a draft
//! at all: constants evaluate at compile time, so the build stopped before
//! anything ran.

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

#[test]
fn gos_inline_marker_asserts_the_type_a_bare_nil_has_not() {
    // A coalesce has no Go counterpart, so its stand-in must at least bind.
    let source = "export function pick(a: number | null, b: number): void {\n    \
                  const merged = a ?? b;\n    console.log(merged);\n}\n";
    let (_tmp, root) = workspace(&[("pick.ts", source)]);
    let plan = transpile::plan(&root.join("pick.ts"), Language::Go).expect("a draft");
    assert!(
        plan.output.contains("merged := any(nil) /*"),
        "`x := nil` is not Go; the stand-in asserts a type.\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(":= nil"),
        "no bare nil binding remains:\n{}",
        plan.output
    );
}

#[test]
fn rusts_todo_marker_doubles_the_braces_the_source_carried() {
    // A Zig `catch` block carries verbatim, braces included, and those braces ride
    // inside `todo!`'s format string.
    let source = "fn caught() usize {\n    const n = parseLen(\"\") catch |err| {\n        \
                  return 0;\n    };\n    return n;\n}\n";
    let (_tmp, root) = workspace(&[("caught.zig", source)]);
    let plan = transpile::plan(&root.join("caught.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("todo!(\"") && plan.output.contains("{{"),
        "a carried brace is doubled so the format string still parses.\n{}",
        plan.output
    );
}

#[test]
fn a_constant_whose_value_cannot_translate_carries_whole_in_rust() {
    // `error{A} || B` is a composed error set with no counterpart; written as a
    // `const` with a todo body it would stop the build at compile-time evaluation.
    let source = "const LoadError = error{Unsupported} || SomethingElse;\n\n\
                  pub fn answer() i64 {\n    return 7;\n}\n";
    let (_tmp, root) = workspace(&[("sets.zig", source)]);
    let plan = transpile::plan(&root.join("sets.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER) && plan.output.contains("// const LoadError ="),
        "the constant carries whole as a comment, name and all.\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("const LOAD_ERROR"),
        "no const declaration with an unevaluable body remains.\n{}",
        plan.output
    );
}
