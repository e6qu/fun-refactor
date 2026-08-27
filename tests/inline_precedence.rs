//! Substituting a value into an expression, without changing what it means.

use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::refactor::inline;
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("writing the file");
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn binding(index: &Index, file: &str, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name && s.file.ends_with(file))
        .unwrap_or_else(|| panic!("no `{name}` in {file}"))
        .id
}

fn inlined(file: &str, source: &str, name: &str) -> String {
    let (_tmp, root) = workspace(&[(file, source)]);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let plan = inline::variable(&index, binding(&index, file, name)).expect("an inline");
    let path = root.join(file);
    let edits = plan.edits.edits_for(&path).expect("edits for the file");
    fun_refactor::edit::apply_to_string(source, edits).expect("applying")
}

#[test]
fn a_compound_value_keeps_its_grouping() {
    // Every language with an expression grammar, and every one of them was wrong.
    for (file, source, expected) in [
        (
            "a.rs",
            "pub fn f(a: i64) -> i64 {\n    let b = a + 1;\n    return b * 2;\n}\n",
            "return (a + 1) * 2;",
        ),
        (
            "a.py",
            "def f(a):\n    b = a + 1\n    return b * 2\n",
            "return (a + 1) * 2",
        ),
        (
            "a.ts",
            "export function f(a: number): number {\n  const b = a + 1;\n  return b * 2;\n}\n",
            "return (a + 1) * 2;",
        ),
        (
            "a.go",
            "package main\n\nfunc f(a int) int {\n\tb := a + 1\n\treturn b * 2\n}\n",
            "return (a + 1) * 2",
        ),
        (
            "a.zig",
            "pub fn f(a: i64) i64 {\n    const b = a + 1;\n    return b * 2;\n}\n",
            "return (a + 1) * 2;",
        ),
    ] {
        let after = inlined(file, source, "b");
        assert!(after.contains(expected), "{file}:\n{after}");
    }
}

#[test]
fn a_value_nothing_can_split_is_left_bare() {
    // A name, a literal, a call, a field, an index: no surrounding operator can get
    // inside one, so a parenthesis would be noise.
    let source = "pub fn f() -> i64 {\n    let b = g(1);\n    return b * 2;\n}\n";
    let after = inlined("a.rs", source, "b");
    assert!(after.contains("return g(1) * 2;"), "{after}");
}

#[test]
fn a_zig_binding_can_be_inlined_at_all() {
    // tree-sitter-zig names nothing on a `variable_declaration`, the `=` is an anonymous token
    // with the value after it, so asking for the `value` field refused every Zig binding there
    // has ever been, while the capability matrix said it worked.
    let source = "pub fn f(a: i64) i64 {\n    const b = g(a);\n    return b;\n}\n";
    let after = inlined("a.zig", source, "b");
    assert!(after.contains("return g(a);"), "{after}");
    assert!(!after.contains("const b"), "the binding goes:\n{after}");
}

#[test]
fn extracting_a_whole_statement_is_refused() {
    // Replacing it with the new name leaves a statement that only names the binding: `zzx;`,
    // which Zig rejects outright, Go rejects as an unused value, and the other three accept
    // while meaning nothing.
    let source = "pub fn f() void {\n    g(1);\n}\n";
    let (_tmp, root) = workspace(&[("a.zig", source)]);
    let index = Index::build(&root, &ScanOptions::default()).expect("an index");
    let path = root.join("a.zig");
    let at = source.find("g(1)").expect("the call");
    let span = fun_refactor::span::Span::new(at, at + 4);
    let refusal = fun_refactor::refactor::extract::variable(&index, &path, span, "zzx", false)
        .expect_err("extracting a whole statement");
    assert!(
        refusal.to_string().contains("whole of its statement"),
        "{refusal}"
    );
}

#[test]
fn a_config_value_is_not_an_expression() {
    // `(enabled)` is not the same scalar as `enabled`.
    let source = "root:\n  a: &shared enabled\n  b: *shared\n";
    let after = inlined("a.yaml", source, "shared");
    assert!(after.contains("b: enabled"), "{after}");
    assert!(!after.contains('('), "{after}");
}

#[test]
fn a_multi_line_binding_takes_its_whole_lines_along() {
    // The removal compared only the first line, so a wrapped binding was cut from its `let` to
    // its `;`.
    let source = "pub fn f(last: &str) -> String {\n    let bare = last\n        \
                  .trim()\n        .to_string();\n    bare\n}\n";
    let out = inlined("m.rs", source, "bare");
    assert!(
        !out.lines().any(|l| !l.is_empty() && l.trim().is_empty()),
        "no line of the result is only whitespace.\n{out:?}"
    );
    assert!(
        out.contains("last\n        .trim()\n        .to_string()"),
        "the value itself is intact.\n{out}"
    );
}
