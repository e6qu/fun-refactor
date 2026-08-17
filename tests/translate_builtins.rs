//! The everyday library calls cross as the target's own spelling.
//!
//! `print`, `len`, `str`, `.append`, `.upper`, `.lower`, `.strip` and
//! `sep.join(xs)` have exact counterparts in every language here. Written
//! through unchanged, each was a compile error in every direction.
//! `console.log` reached Python, `.push` reached Rust, `print` reached
//! TypeScript. The readers
//! rewrite their spellings into the canonical ones and the writers rewrite them
//! out, so one language pair costs two edits and not thirty.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const SHOUT_PY: &str = "def shout(names: list[str]) -> str:\n    loud = []\n    \
    for name in names:\n        loud.append(name.upper())\n    print(len(loud))\n    \
    return \", \".join(loud)\n";

#[test]
fn python_builtins_reach_typescript_as_typescript() {
    let out = translated(SHOUT_PY, "shout.py", Language::TypeScript);
    for expected in [
        "loud.push(name.toUpperCase())",
        "console.log(loud.length)",
        "loud.join(\", \")",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }
}

#[test]
fn python_builtins_reach_rust_as_rust() {
    let out = translated(SHOUT_PY, "shout.py", Language::Rust);
    for expected in [
        "loud.push(name.to_uppercase())",
        "println!(\"{}\", loud.len())",
        "loud.join(\", \")",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }
}

#[test]
fn typescript_spellings_reach_python_as_python() {
    let source = "export function shout(names: string[]): string {\n    let loud = [];\n    \
        for (const name of names) {\n        loud.push(name.toUpperCase());\n    }\n    \
        console.log(loud.length);\n    return loud.join(\", \");\n}\n";
    let out = translated(source, "shout.ts", Language::Python);
    for expected in [
        "loud.append(name.upper())",
        "print(len(loud))",
        "\", \".join(loud)",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }
}

#[test]
fn go_gains_the_imports_the_body_turned_out_to_need() {
    let out = translated(SHOUT_PY, "shout.py", Language::Go);
    assert!(
        out.contains("import \"fmt\"") && out.contains("import \"strings\""),
        "the packages the mapped calls need are imported.\n{out}"
    );
    assert!(
        out.contains("loud = append(loud, strings.ToUpper(name))"),
        "append assigns back, as Go's own append does:\n{out}"
    );
    assert!(
        out.contains("fmt.Println(len(loud))") && out.contains("strings.Join(loud, \", \")"),
        "print and join speak Go:\n{out}"
    );
}

#[test]
fn a_module_declaring_its_own_print_keeps_it() {
    let source = "def print(x: str) -> None:\n    pass\n\n\ndef run() -> None:\n    print(\"a\")\n";
    let out = translated(source, "own.py", Language::TypeScript);
    assert!(
        !out.contains("console.log"),
        "the module's own `print` is not the builtin:\n{out}"
    );
}

#[test]
fn java_spellings_cross_and_return() {
    let source = "public class Log {\n    static void greet(String name) {\n        \
        System.out.println(name.toUpperCase());\n    }\n}\n";
    let out = translated(source, "Log.java", Language::Python);
    assert!(
        out.contains("print(name.upper())"),
        "System.out.println and toUpperCase speak Python:\n{out}"
    );
}
