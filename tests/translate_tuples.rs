//! Several values travelling as one cross as a tuple.
//!
//! `return a, b` is Go's multiple return. Before the tuple existed in the IR,
//! the reader mapped it to nothing. Every translated two-value return came out
//! as a bare `return`: not a marked gap, a silent wrong answer. The tuple also
//! carries Python's `return a, b` and Rust's `(a, b)`. A writer with no spelling
//! for it says so instead of inventing one.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{:?}", target)).with_extension("txt");
    let plan = transpile::plan_to(&path, target, Some(&out), false).expect("a plan");
    plan.output
}

const FETCH_GO: &str = "package main\n\nimport \"fmt\"\n\n\
    func fetch(url string) (int, error) {\n\
    \tif url == \"\" {\n\t\treturn 0, fmt.Errorf(\"empty\")\n\t}\n\
    \treturn len(url) * 10, nil\n}\n";

#[test]
fn a_go_error_pair_becomes_typescripts_own_throw() {
    // `(int, error)` is Go's word for a function that can fail; TypeScript's
    // is a throw. The value side is the return type and the failure flies.
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "fetch.go", FETCH_GO, Language::TypeScript);
    assert!(
        out.contains("fetch(url: string): number {"),
        "the value side is the return type.\n{out}"
    );
    assert!(
        out.contains("throw new Error(`empty`);"),
        "the failure payload survives as the thrown message:\n{out}"
    );
    assert!(
        !out.contains("return;"),
        "no return loses its values silently.\n{out}"
    );
}

#[test]
fn a_go_error_pair_is_a_rust_result() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "fetch.go", FETCH_GO, Language::Rust);
    assert!(
        out.contains("-> Result<i64, String>"),
        "the pair is the Result it means.\n{out}"
    );
    assert!(
        out.contains("return Err(\"empty\".to_string());"),
        "the failure crosses as the Err it is:\n{out}"
    );
}

#[test]
fn a_go_error_pair_throws_in_java() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fetch.go");
    std::fs::write(&path, FETCH_GO).unwrap();
    let out_path = tmp.path().join("Fetch.java");
    let plan = transpile::plan_to(&path, Language::Java, Some(&out_path), false).unwrap();
    assert!(
        plan.output
            .contains("throw new RuntimeException(\"empty\");"),
        "the failure flies:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "nothing carries:\n{}",
        plan.output
    );
}

#[test]
fn a_python_bare_tuple_return_crosses() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "def split(pair: str) -> tuple[str, int]:\n    name, count = pair, 1\n    return name, count\n";
    let out = translated(tmp.path(), "pair.py", source, Language::TypeScript);
    assert!(
        out.contains("return [name, count];"),
        "both values arrive:\n{out}"
    );
    assert!(
        out.contains("[string, number]"),
        "and the annotation crosses as a tuple type:\n{out}"
    );
}
