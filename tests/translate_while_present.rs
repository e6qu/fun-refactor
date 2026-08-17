//! Looping on an optional's payload crosses every boundary here.
//!
//! Rust spells it `while let Some(v) = e` and Zig `while (e) |v|`, re-evaluating
//! `e` each pass. The other four have no binding form in a loop header. Their
//! writers open an unconditional loop, take the value, and break when it is
//! empty, which is the same loop said longhand.

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

const DRAIN_RS: &str = "fn drain(mut it: Iter) -> i64 {\n    let mut total = 0;\n    \
                        while let Some(v) = it.next() {\n        total += v;\n    }\n    \
                        total\n}\n";

#[test]
fn a_while_let_crosses_into_every_target() {
    let (_tmp, root) = workspace(&[("w.rs", DRAIN_RS)]);
    let cases = [
        (Language::Zig, "while (it.next()) |v| {"),
        (Language::Python, "while True:"),
        (Language::Python, "v = it.next()"),
        (Language::Python, "if v is None:"),
        (Language::TypeScript, "const v = it.next();"),
        (Language::TypeScript, "if (v === null) {"),
        (Language::Go, "vPtr := it.next()"),
        (Language::Go, "if vPtr == nil {"),
        (Language::Java, "if (vMaybe.isEmpty()) {"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("w.rs"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried what it can say:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_zig_while_payload_becomes_while_let() {
    let source = "fn drain(it: Iter) i64 {\n    var total: i64 = 0;\n    \
                  while (it.next()) |v| {\n        total += v;\n    }\n    return total;\n}\n";
    let (_tmp, root) = workspace(&[("w.zig", source)]);
    let plan = transpile::plan(&root.join("w.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("while let Some(v) = it.next() {"),
        "{}",
        plan.output
    );
}

#[test]
fn a_while_with_a_continue_expression_carries() {
    // `while (c) |v| : (i += 1)` steps something the IR has no slot for.
    let source = "fn walk(it: Iter) void {\n    var i: usize = 0;\n    \
                  while (it.next()) |v| : (i += 1) {\n        use(v, i);\n    }\n}\n";
    let (_tmp, root) = workspace(&[("c.zig", source)]);
    let plan = transpile::plan(&root.join("c.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "a stepped while has no crossing and must say so.\n{}",
        plan.output
    );
}

#[test]
fn compound_assignment_desugars_from_every_reader() {
    // `total += item` is `total = total + item`, and half the readers dropped or
    // carried it. The Go and Java readers silently read it as plain `=`.
    let sources: &[(&str, &str)] = &[
        (
            "a.py",
            "def tally(items):\n    total = 0\n    for item in items:\n        \
             total += item\n    return total\n",
        ),
        (
            "a.zig",
            "fn tally(items: []const i64) i64 {\n    var total: i64 = 0;\n    \
             for (items) |item| {\n        total += item;\n    }\n    return total;\n}\n",
        ),
        (
            "a.ts",
            "export function tally(items: number[]): number {\n    let total = 0;\n    \
             for (const item of items) {\n        total += item;\n    }\n    return total;\n}\n",
        ),
        (
            "a.go",
            "package main\n\nfunc tally(items []int) int {\n\ttotal := 0\n\t\
             for _, item := range items {\n\t\ttotal += item\n\t}\n\treturn total\n}\n",
        ),
        (
            "Aug.java",
            "public final class Aug {\n    static int tally(int[] items) {\n        \
             int total = 0;\n        // Sum them.\n        for (int item : items) {\n            \
             total += item;\n        }\n        return total;\n    }\n}\n",
        ),
    ];
    for (name, source) in sources {
        let (_tmp, root) = workspace(&[(name, source)]);
        let plan = transpile::plan(&root.join(name), Language::Rust).expect("a draft");
        assert!(
            plan.output.contains("total = total + item;"),
            "{name} did not desugar `+=`.\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{name} carried what it can say:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_rust_compound_assignment_desugars() {
    let (_tmp, root) = workspace(&[("w.rs", DRAIN_RS)]);
    let plan = transpile::plan(&root.join("w.rs"), Language::Python).expect("a draft");
    assert!(plan.output.contains("total = total + v"), "{}", plan.output);
}

#[test]
fn an_unknown_compound_operator_carries() {
    // `>>=` has no BinaryOp; carrying beats quietly writing `=`.
    let source = "def halve(n):\n    n >>= 1\n    return n\n";
    let (_tmp, root) = workspace(&[("h.py", source)]);
    let plan = transpile::plan(&root.join("h.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains(transpile::MARKER),
        "an operator with no counterpart must carry.\n{}",
        plan.output
    );
}
