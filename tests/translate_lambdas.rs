//! A one-expression lambda crosses between the four languages that have one.
//!
//! `lambda x: e`, `(x) => e`, `|x| e` and `x -> e` are the same nameless
//! function, and each used to carry as a runnable `null` or a comment, so a
//! `sorted(key=...)` or a `.map(...)` callback vanished from the program. Go
//! and Zig cannot write a closure without types the source never spelled, so
//! their writers carry it, visibly. A block body is a function that wants a
//! name and stays carried everywhere.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    let out = dir.join(format!("out_{target:?}")).with_extension("txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const DOUBLE_PY: &str = "def main() -> None:\n    double = lambda x: x * 2\n    \
    print(double(21))\n\n\nif __name__ == \"__main__\":\n    main()\n";

#[test]
fn a_python_lambda_is_a_typescript_arrow() {
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "double.py", DOUBLE_PY, Language::TypeScript);
    assert!(
        // The parameter carries the type the source gave it, which was none.
        // Strict TypeScript refuses an implicit `any` and accepts a written one.
        out.contains("(x: any) => x * 2"),
        "the callback is live code, no longer a runnable null.\n{out}"
    );
    assert!(
        !out.contains("not translated: lambda"),
        "nothing about it is a gap.\n{out}"
    );
}

#[test]
fn a_python_lambda_is_a_rust_closure_and_a_java_lambda() {
    let tmp = tempfile::tempdir().unwrap();
    let rust = translated(tmp.path(), "double.py", DOUBLE_PY, Language::Rust);
    assert!(rust.contains("|x| x * 2"), "Rust spells it bare.\n{rust}");
    let java = translated(tmp.path(), "double.py", DOUBLE_PY, Language::Java);
    assert!(
        java.contains("(x) -> x * 2"),
        "Java spells it with an arrow.\n{java}"
    );
}

#[test]
fn a_typescript_arrow_is_a_python_lambda() {
    let source = "export function main(): void {\n    const double = (x) => x * 2;\n    \
        console.log(double(21));\n}\n\nmain();\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "double.ts", source, Language::Python);
    assert!(
        out.contains("lambda x: x * 2"),
        "the arrow comes home as the lambda it is.\n{out}"
    );
}

#[test]
fn a_rust_closure_in_an_iterator_chain_crosses() {
    let source = "pub fn shout(names: Vec<String>) -> Vec<String> {\n    \
        let loud: Vec<String> = names.iter().map(|n| n.to_uppercase()).collect();\n    \
        return loud;\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "shout.rs", source, Language::Python);
    assert!(
        out.contains("lambda n: n.upper()"),
        "the closure crosses and its body speaks the target's library.\n{out}"
    );
}

#[test]
fn go_and_zig_carry_the_lambda_visibly() {
    let tmp = tempfile::tempdir().unwrap();
    let go = translated(tmp.path(), "double.py", DOUBLE_PY, Language::Go);
    assert!(
        go.contains("fun-refactor: not translated: (x) => x * 2"),
        "Go has no typeless closure, and the loss is inline where it happened.\n{go}"
    );
    let zig = translated(tmp.path(), "double.py", DOUBLE_PY, Language::Zig);
    assert!(
        zig.contains("fun-refactor: not translated: (x) => x * 2"),
        "Zig has none either.\n{zig}"
    );
}

#[test]
fn a_block_bodied_arrow_stays_carried() {
    let source = "export function run(xs: number[]): number[] {\n    \
        return xs.map((x) => { return x * 2; });\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "blocks.ts", source, Language::Python);
    assert!(
        out.contains("fun-refactor: not translated"),
        "a block body is a function that wants a name, and it carries whole.\n{out}"
    );
}
