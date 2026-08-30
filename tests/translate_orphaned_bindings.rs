//! A header that binds names stays under the branch it guards.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const LOOKUP_GO: &str = r#"package main

import "fmt"

func Best(n int) (int, bool) {
	if n == 0 {
		return 0, false
	}
	return n * 2, true
}

func main() {
	if m, ok := Best(4); ok {
		fmt.Println(m)
	}

	if m, ok := Best(0); ok {
		fmt.Println(m)
	} else {
		fmt.Println("none")
	}
}
"#;

const INDEXED_GO: &str = "package main\n\nfunc Weigh(xs []int) int {\n\ttotal := 0\n\n\t\
                          for i, x := range xs {\n\t\ttotal = total + i*x\n\t}\n\t\
                          return total\n}\n";

/// What the Go source prints.
const EXPECTED: &str = "8\nnone";

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
fn the_header_of_an_if_is_written_before_it() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "lookup.go", LOOKUP_GO, Language::Python);
    let bound = out
        .find("m, ok = best(4)")
        .unwrap_or_else(|| panic!("the header is missing\n{out}"));
    let tested = out
        .find("if ok:")
        .unwrap_or_else(|| panic!("the branch is missing\n{out}"));
    assert!(bound < tested, "the header comes first.\n{out}");
}

#[test]
fn a_second_header_of_the_same_names_settles_them_again() {
    // Each header was its own scope in Go.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "lookup.go", LOOKUP_GO, Language::TypeScript);
    assert!(out.contains("let [m, ok] = best(4);"), "{out}");
    assert!(out.contains("[m, ok] = best(0);"), "{out}");
    assert_eq!(
        out.matches("let [m, ok]").count(),
        1,
        "declared once.\n{out}"
    );
}

#[test]
fn the_index_of_a_range_loop_is_bound() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "weigh.go", INDEXED_GO, Language::Python);
    assert!(
        out.contains("for i, x in enumerate(xs):"),
        "the position takes its binding beside the value.\n{out}"
    );
}

#[test]
fn the_translation_prints_what_the_go_prints() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();

    if Toolchain::Go.is_available() {
        std::fs::write(dir.join("lookup.go"), LOOKUP_GO).expect("the source");
        std::fs::write(dir.join("go.mod"), "module lookup\n\ngo 1.21\n").expect("the module");
        let ran = Command::new("go")
            .args(["run", "lookup.go"])
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
        let python = translated(dir, "lookup.go", LOOKUP_GO, Language::Python);
        std::fs::write(dir.join("lookup.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("lookup.py")
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

    require_on_ci("a header that binds, run", &missing);
}
