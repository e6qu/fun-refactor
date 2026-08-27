//! `/` between two integers truncates, and TypeScript's does not.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const HALVES_JAVA: &str = r#"public class Halves {
    static int half(int n) {
        return n / 2;
    }

    public static void main(String[] args) {
        System.out.println(half(7));
        System.out.println(half(-7));

        System.out.println(half(0));
        System.out.println(10 / 4.0);
    }
}
"#;

/// What the Java source prints.
const EXPECTED: &str = "3\n-3\n0\n2.5";

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
fn an_integer_division_truncates_toward_zero_in_typescript() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Halves.java", HALVES_JAVA, Language::TypeScript);
    assert!(out.contains("return Math.trunc(n / 2);"), "{out}");
    assert!(
        !out.contains("Math.floor"),
        "`Math.floor` rounds the other way on a negative quotient.\n{out}"
    );
}

#[test]
fn a_division_with_a_float_in_it_keeps_its_fraction() {
    // `10 / 4.0` is 2.5 in Java too.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Halves.java", HALVES_JAVA, Language::TypeScript);
    assert!(out.contains("console.log(10 / 4.0);"), "{out}");
}

#[test]
fn the_quotients_match_javac_on_both_signs() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();

    if Toolchain::Javac.is_available() {
        std::fs::write(dir.join("Halves.java"), HALVES_JAVA).expect("the source");
        let built = Command::new("javac")
            .arg("Halves.java")
            .current_dir(dir)
            .output()
            .expect("running javac");
        assert!(built.status.success(), "{}", said(&built));
        let ran = Command::new("java")
            .arg("Halves")
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

    let node = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if Toolchain::Tsc.is_available() && node {
        let ts = translated(dir, "Halves.java", HALVES_JAVA, Language::TypeScript);
        std::fs::write(dir.join("halves.ts"), &ts).expect("the translation");
        let built = Command::new("tsc")
            .current_dir(dir)
            .args(["--strict", "--target", "es2022", "--outDir", "js"])
            .arg("halves.ts")
            .output()
            .expect("running tsc");
        assert!(built.status.success(), "{ts}\n{}", said(&built));
        let ran = Command::new("node")
            .arg("js/halves.js")
            .current_dir(dir)
            .output()
            .expect("running node");
        assert!(ran.status.success(), "{ts}\n{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the TypeScript translation prints something else.\n{ts}"
        );
    } else {
        missing.push("tsc and node".to_string());
    }

    if Toolchain::Python.is_available() {
        let python = translated(dir, "Halves.java", HALVES_JAVA, Language::Python);
        std::fs::write(dir.join("halves.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("halves.py")
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

    require_on_ci("integer division, run", &missing);
}
