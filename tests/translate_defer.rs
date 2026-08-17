//! `defer` crosses, and a positional construction finds its record's fields.
//!
//! Go and Zig keep the word. Python, TypeScript and Java say the same thing
//! with `try`/`finally`: everything after the defer goes in the `try`, the
//! deferred body in the `finally`. Stacked defers nest, which keeps their
//! last-in, first-out order. Rust has no scope-exit hook short of inventing a
//! guard type, so it carries the body as a comment, rendered as Rust.

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

const READER_ZIG: &str = "fn readAll(path: []const u8) i64 {\n    const file = open(path);\n    \
                          defer file.close();\n    const n = parse(file);\n    return n;\n}\n";

#[test]
fn a_zig_defer_crosses_into_every_target() {
    let (_tmp, root) = workspace(&[("d.zig", READER_ZIG)]);
    let cases = [
        (Language::Go, "defer file.close()"),
        (Language::Python, "try:"),
        (Language::Python, "finally:"),
        (Language::Python, "file.close()"),
        (Language::TypeScript, "} finally {"),
        (Language::Java, "} finally {"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("d.zig"), to).expect("a draft");
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
fn the_statements_after_the_defer_run_inside_the_try() {
    let (_tmp, root) = workspace(&[("d.zig", READER_ZIG)]);
    let plan = transpile::plan(&root.join("d.zig"), Language::Python).expect("a draft");
    let try_at = plan.output.find("try:").expect("a try block");
    let parse_at = plan
        .output
        .find("parse(file)")
        .expect("the later statement");
    let finally_at = plan.output.find("finally:").expect("a finally block");
    assert!(
        try_at < parse_at && parse_at < finally_at,
        "what follows the defer belongs to the try:\n{}",
        plan.output
    );
}

#[test]
fn stacked_defers_nest_and_keep_their_order() {
    let source = "fn run() void {\n    defer first();\n    defer second();\n    work();\n}\n";
    let (_tmp, root) = workspace(&[("s.zig", source)]);
    let plan = transpile::plan(&root.join("s.zig"), Language::Python).expect("a draft");
    let outer = plan.output.find("first()").expect("the first defer");
    let inner = plan.output.find("second()").expect("the second defer");
    assert!(
        inner < outer,
        "the second defer runs first, so its finally sits inside:\n{}",
        plan.output
    );
}

#[test]
fn a_go_defer_reads_and_rust_carries_it_as_rendered_rust() {
    let source = "package main\n\nfunc readAll(path string) int {\n\tfile := open(path)\n\t\
                  defer file.Close()\n\treturn parse(file)\n}\n";
    let (_tmp, root) = workspace(&[("d.go", source)]);
    let plan = transpile::plan(&root.join("d.go"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("a defer runs this at scope exit:"),
        "{}",
        plan.output
    );
    assert!(
        plan.output.contains("// file.close();") || plan.output.contains("// file.Close();"),
        "the carried body is the body, rendered:\n{}",
        plan.output
    );
}

#[test]
fn a_positional_construction_names_the_declared_records_fields() {
    let source = "from dataclasses import dataclass\n\n\n@dataclass\nclass Point:\n    \
                  x: int\n    y: int\n\n\ndef origin() -> Point:\n    return Point(0, 0)\n";
    let (_tmp, root) = workspace(&[("pt.py", source)]);
    let cases = [
        (Language::Rust, "Point { x: 0, y: 0 }"),
        (Language::Go, "Point{X: 0, Y: 0}"),
        (Language::Zig, "Point{ .x = 0, .y = 0 }"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("pt.py"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried a construction whose shape it knows:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_construction_with_the_wrong_arity_carries() {
    // Two fields, one argument: mapping positions would invent a default.
    let source = "from dataclasses import dataclass\n\n\n@dataclass\nclass Point:\n    \
                  x: int\n    y: int\n\n\ndef partial() -> Point:\n    return Point(1)\n";
    let (_tmp, root) = workspace(&[("pp.py", source)]);
    let plan = transpile::plan(&root.join("pp.py"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("Point(1)"),
        "an arity mismatch stays a call for the reader to resolve:\n{}",
        plan.output
    );
}

#[test]
fn typescript_parameter_properties_become_fields() {
    let source = "class Point {\n    constructor(public x: number, public y: number) {}\n}\n\n\
                  export function origin(): Point {\n    return new Point(0, 0);\n}\n";
    let (_tmp, root) = workspace(&[("pt.ts", source)]);
    let plan = transpile::plan(&root.join("pt.ts"), Language::Go).expect("a draft");
    assert!(
        plan.output.contains("X float64"),
        "the constructor's parameter declares the field:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("Point{X: 0, Y: 0}"),
        "and the construction maps onto it:\n{}",
        plan.output
    );
}

#[test]
fn errdefer_cleans_up_only_on_the_failure_path() {
    // Zig's `errdefer` runs when the scope is left failing, and here failure is
    // an exception: the cleanup wraps the rest of the scope, runs on the way
    // out, and the exception keeps flying. Carried whole, 55 of these vanished
    // into comments while the code after them read the names they managed.
    let source = "pub fn build(allocator: anytype) !u32 {\n    \
        var list = try makeList(allocator);\n    errdefer list.deinit(allocator);\n    \
        try fill(&list);\n    return list.len;\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("build.zig");
    std::fs::write(&path, source).unwrap();

    let py = transpile::plan_to(&path, Language::Python, Some(&tmp.path().join("a")), false)
        .unwrap()
        .output;
    assert!(
        py.contains("except BaseException:") && py.contains("raise"),
        "Python cleans up and lets the failure keep flying:\n{py}"
    );

    let ts = transpile::plan_to(&path, Language::TypeScript, Some(&tmp.path().join("b")), false)
        .unwrap()
        .output;
    assert!(
        ts.contains("catch (fr_err)") && ts.contains("throw fr_err;"),
        "TypeScript rethrows after the cleanup:\n{ts}"
    );

    let go = transpile::plan_to(&path, Language::Go, Some(&tmp.path().join("c")), false)
        .unwrap()
        .output;
    assert!(
        go.contains("errdefer runs this when the scope fails"),
        "Go has no failure path a block can watch, and says so:\n{go}"
    );
}
