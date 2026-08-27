//! A chain of `+` with a string in it builds a string, all the way along.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const CONCAT_JAVA: &str = r#"public class Concat {
    public static void main(String[] args) {
        System.out.println("x" + 1 + 2);

        System.out.println(1 + 2 + "x");
        System.out.println("a" + 1 + "b" + 2);
        int n = 7;

        System.out.println("n=" + n + "!");
    }
}
"#;

/// What the Java source prints.
const EXPECTED: &str = "x12\n3x\na1b2\nn=7!";

fn translated(dir: &Path, name: &str, source: &str, target: Language) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("the source file");
    let out = dir.join(format!("out_{target:?}.txt"));
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn every_number_in_the_chain_is_coerced() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Concat.java", CONCAT_JAVA, Language::Python);
    assert!(out.contains(r#"print(f"x{1}{2}")"#), "{out}");
    assert!(
        out.contains(r#"print(f"a{1}b{2}")"#),
        "a string later in the chain does not stop it.\n{out}"
    );
}

#[test]
fn a_number_on_the_left_still_takes_the_one_coercion_it_needs() {
    // `1 + 2 + "x"` adds first and concatenates second, which is what Java does.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Concat.java", CONCAT_JAVA, Language::Python);
    assert!(out.contains(r#"print(f"{1 + 2}x")"#), "{out}");
}

#[test]
fn a_declared_binding_in_the_chain_is_coerced_too() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Concat.java", CONCAT_JAVA, Language::Python);
    assert!(out.contains(r#"print(f"n={n}!")"#), "{out}");
}

#[test]
fn the_chain_prints_the_same_in_java_and_in_python() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();

    if Toolchain::Javac.is_available() {
        std::fs::write(dir.join("Concat.java"), CONCAT_JAVA).expect("the source");
        let built = Command::new("javac")
            .arg("Concat.java")
            .current_dir(dir)
            .output()
            .expect("running javac");
        assert!(built.status.success(), "{}", said(&built));
        let ran = Command::new("java")
            .arg("Concat")
            .current_dir(dir)
            .output()
            .expect("running java");
        assert!(ran.status.success(), "{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the Java source no longer prints what this gate expects."
        );
    } else {
        missing.push("javac".to_string());
    }

    if Toolchain::Python.is_available() {
        let python = translated(dir, "Concat.java", CONCAT_JAVA, Language::Python);
        std::fs::write(dir.join("concat.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("concat.py")
            .current_dir(dir)
            .output()
            .expect("running python3");
        assert!(ran.status.success(), "{python}\n{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the Python translation prints something else.\n{python}"
        );
    } else {
        missing.push("python3".to_string());
    }

    require_on_ci("a concatenation chain, run", &missing);
}
