//! A one-expression lambda crosses between the four languages that have one.

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
    // The chain is a comprehension written the way Rust writes one, so it arrives as the
    // comprehension the target spells rather than as a call to `map` with a lambda in it.
    let source = "pub fn shout(names: Vec<String>) -> Vec<String> {\n    \
        let loud: Vec<String> = names.iter().map(|n| n.to_uppercase()).collect();\n    \
        return loud;\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "shout.rs", source, Language::Python);
    assert!(
        out.contains("[n.upper() for n in names]"),
        "the chain crosses and its body speaks the target's library.\n{out}"
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
fn a_block_bodied_arrow_holding_one_return_crosses() {
    // A block whose only statement returns holds one expression, which is the shape a lambda
    // and a comprehension both take.
    let source = "export function run(xs: number[]): number[] {\n    \
        return xs.map((x) => { return x * 2; });\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "blocks.ts", source, Language::Python);
    assert!(
        out.contains("[x * 2 for x in xs]"),
        "the one expression the block returns is the comprehension's.\n{out}"
    );
}

#[test]
fn a_block_bodied_arrow_doing_more_stays_carried() {
    let source = "export function run(xs: number[]): number[] {\n    \
        return xs.map((x) => { const y = x * 2; return y + 1; });\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let out = translated(tmp.path(), "steps.ts", source, Language::Python);
    assert!(
        out.contains("fun-refactor: not translated"),
        "a body with steps in it is a function that wants a name.\n{out}"
    );
}
