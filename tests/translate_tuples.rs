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
fn a_go_multiple_return_keeps_both_values_in_typescript() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "fetch.go", FETCH_GO, Language::TypeScript);
    assert!(
        out.contains("): [number, error]"),
        "the result type crosses as a tuple type.\n{out}"
    );
    assert!(
        out.contains("return [0, fmt.Errorf(\"empty\")];"),
        "the failure payload survives:\n{out}"
    );
    assert!(
        !out.contains("return;"),
        "no return loses its values silently.\n{out}"
    );
}

#[test]
fn a_go_multiple_return_is_a_rust_tuple() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "fetch.go", FETCH_GO, Language::Rust);
    assert!(
        out.contains("-> (i64, error)"),
        "the result type is a tuple there too.\n{out}"
    );
    assert!(
        out.contains("return (0, fmt.Errorf(\"empty\"));"),
        "the values stay paired:\n{out}"
    );
}

#[test]
fn a_writer_with_no_tuple_says_so_instead_of_dropping_it() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fetch.go");
    std::fs::write(&path, FETCH_GO).unwrap();
    let out_path = tmp.path().join("Fetch.java");
    let plan = transpile::plan_to(&path, Language::Java, Some(&out_path), false).unwrap();
    assert!(
        plan.output.contains("fun-refactor: not translated: tuple"),
        "Java carries the tuple visibly:\n{}",
        plan.output
    );
    assert!(
        plan.fidelity.notes.iter().any(|n| n.contains("tuple")),
        "and the fidelity report names it: {:?}",
        plan.fidelity.notes
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
