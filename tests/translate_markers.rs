//! A marker is a draft's honesty, and a draft must still compile around it.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

#[test]
fn gos_inline_marker_asserts_the_type_a_bare_nil_has_not() {
    // A coalesce has no Go counterpart, so its stand-in must at least bind.
    let source = "export function pick(a: number | null, b: number): void {\n    \
                  const merged = a ?? b;\n    console.log(merged);\n}\n";
    let (_tmp, root) = common::tree(&[("pick.ts", source)]);
    let plan = transpile::plan(&root.join("pick.ts"), Language::Go).expect("a draft");
    assert!(
        plan.output.contains("merged := func() any {"),
        "the coalesce binds through a closure:\n{}",
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
    // A builtin nothing translates carries verbatim, braces included, and
    // those braces ride inside `todo!`'s format string.
    let source =
        "fn caught() usize {\n    const n = @cmpxchgWeak(usize, p, .{ .a = 1 }, v, o, o);\n    \
                  return n;\n}\n";
    let (_tmp, root) = common::tree(&[("caught.zig", source)]);
    let plan = transpile::plan(&root.join("caught.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("todo!(\"") && plan.output.contains("{{"),
        "a carried brace is doubled so the format string still parses.\n{}",
        plan.output
    );
}

#[test]
fn a_composed_error_set_keeps_its_spelling_as_text() {
    // `error{A} || B` composes error sets, which no target's error model can hold.
    let source = "const LoadError = error{Unsupported} || SomethingElse;\n\n\
                  pub fn answer() i64 {\n    return 7;\n}\n";
    let (_tmp, root) = common::tree(&[("sets.zig", source)]);
    let plan = transpile::plan(&root.join("sets.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("error{Unsupported} || SomethingElse"),
        "the set's own spelling is the value:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "nothing carries:\n{}",
        plan.output
    );
}
