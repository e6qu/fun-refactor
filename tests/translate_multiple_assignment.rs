//! `a, b = b, a` and `x, err := f()`, settled in one line.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use fun_refactor::transpile::MARKER;
use std::path::Path;
use std::process::Command;

const PAIR_GO: &str = r#"package main

import "fmt"

func Pair(n int) (int, int) {
	return n + 1, n * 2
}

func main() {
	a, b := Pair(3)
	a, b = b, a

	fmt.Println(a)
	fmt.Println(b)
}
"#;

/// What the Go source prints.
const EXPECTED: &str = "6\n4";

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
fn typescript_destructures_the_pair() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "pair.go", PAIR_GO, Language::TypeScript);
    assert!(out.contains("let [a, b] = pair(3);"), "{out}");
    assert!(out.contains("[a, b] = [b, a];"), "the swap.\n{out}");
}

#[test]
fn python_writes_the_line_it_already_has() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "pair.go", PAIR_GO, Language::Python);
    assert!(out.contains("a, b = pair(3)"), "{out}");
    assert!(out.contains("a, b = (b, a)"), "the swap.\n{out}");
    assert!(!out.contains(MARKER), "nothing carried.\n{out}");
}

#[test]
fn rust_binds_the_tuple_and_swaps_through_it() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "pair.go", PAIR_GO, Language::Rust);
    assert!(out.contains("let (mut a, mut b) = pair(3);"), "{out}");
    assert!(out.contains("(a, b) = (b, a);"), "the swap.\n{out}");
}

#[test]
fn a_python_swap_crosses_the_other_way() {
    let source = "def swap(a: int, b: int) -> int:\n    a, b = b, a\n    return a - b\n";
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "swap.py", source, Language::TypeScript);
    assert!(!out.contains(MARKER), "nothing carried.\n{out}");
    assert!(
        out.contains("[a, b] = [b, a];"),
        "the parameters are bound already, so this assigns.\n{out}"
    );
}

#[test]
fn java_binds_the_value_once_and_takes_its_parts() {
    // Java has no tuple statement.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "pair.go", PAIR_GO, Language::Java);
    assert!(
        out.contains("var frTup1 = ") && out.contains("var a = frTup1.get(0);"),
        "the names take the parts by position.\n{out}"
    );
}

#[test]
fn the_swap_prints_the_same_everywhere() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();

    if Toolchain::Go.is_available() {
        std::fs::write(dir.join("pair.go"), PAIR_GO).expect("the source");
        std::fs::write(dir.join("go.mod"), "module pair\n\ngo 1.21\n").expect("the module");
        let ran = Command::new("go")
            .args(["run", "pair.go"])
            .current_dir(dir)
            .output()
            .expect("running go");
        assert!(ran.status.success(), "{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the Go source no longer prints what this gate expects."
        );
    } else {
        missing.push("go".to_string());
    }

    if Toolchain::Python.is_available() {
        let python = translated(dir, "pair.go", PAIR_GO, Language::Python);
        std::fs::write(dir.join("pair.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("pair.py")
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

    if Toolchain::Cargo.is_available() {
        let rust = translated(dir, "pair.go", PAIR_GO, Language::Rust);
        std::fs::write(dir.join("pair.rs"), &rust).expect("the translation");
        let built = Command::new("rustc")
            .arg("-o")
            .arg(dir.join("pair"))
            .arg(dir.join("pair.rs"))
            .output()
            .expect("running rustc");
        assert!(built.status.success(), "{rust}\n{}", said(&built));
        let ran = Command::new(dir.join("pair"))
            .output()
            .expect("running the binary");
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the Rust translation prints something else.\n{rust}"
        );
    } else {
        missing.push("rustc".to_string());
    }

    require_on_ci("multiple assignment, run", &missing);
}
