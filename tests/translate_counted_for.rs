//! Go's `for` in every spelling, and Java's counted one.
//!
//! `for` is the only loop keyword Go has, and it has four spellings. Only the
//! `range` one crossed. The other three became comments. That took their
//! bodies with them and left every name the header bound undeclared. Java's
//! counted `for` went the same way.
//!
//! Three targets write the whole header. Zig writes the step as a continue
//! expression. Rust and Python have neither. Both walk a range where the
//! header walks one, say the rest longhand, and carry the loop whole where
//! the longhand would move a `continue`.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use fun_refactor::transpile::MARKER;
use std::path::Path;
use std::process::Command;

const LOOPS_GO: &str = r#"package main

import "fmt"

func Sum(n int) int {
	total := 0

	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

func Down(n int) int {
	seen := 0
	left := n

	for left > 0 {
		seen += 1
		left -= 1
	}
	return seen
}

func Forever(limit int) int {
	i := 0

	for {
		i++
		if i >= limit {
			break
		}
	}
	return i
}

func Evens(n int) int {
	total := 0

	for i := 0; i < n; i++ {
		if i%2 == 1 {
			continue
		}
		total += i
	}
	return total
}

func main() {
	fmt.Println(Sum(5))
	fmt.Println(Down(4))
	fmt.Println(Forever(3))
	fmt.Println(Evens(7))
}
"#;

/// What the Go source prints, one line per call.
const EXPECTED: &str = "10\n4\n3\n12";

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
fn no_spelling_of_gos_for_is_carried_any_more() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for target in [
        Language::Python,
        Language::TypeScript,
        Language::Rust,
        Language::Java,
        Language::Zig,
    ] {
        let out = translated(tmp.path(), "loops.go", LOOPS_GO, target);
        assert!(
            !out.contains(MARKER),
            "{target} still carries a loop.\n{out}"
        );
    }
}

#[test]
fn typescript_writes_the_whole_header() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "loops.go", LOOPS_GO, Language::TypeScript);
    assert!(out.contains("for (let i = 0; i < n; i++) {"), "{out}");
    assert!(out.contains("for (;;) {"), "the bare loop.\n{out}");
    assert!(
        out.contains("while (left > 0) {"),
        "the one-clause loop.\n{out}"
    );
}

#[test]
fn python_walks_a_range_where_the_header_walks_one() {
    // The longhand, start above and step at the foot, would let the `continue`
    // in `Evens` skip the step and spin forever.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "loops.go", LOOPS_GO, Language::Python);
    assert_eq!(
        out.matches("for i in range(0, n):").count(),
        2,
        "both counted loops walk a range.\n{out}"
    );
    assert!(out.contains("while True:"), "the bare loop.\n{out}");
}

#[test]
fn zig_writes_the_step_as_a_continue_expression() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "loops.go", LOOPS_GO, Language::Zig);
    assert!(out.contains("while (i < n) : (i = i + 1) {"), "{out}");
}

#[test]
fn a_step_python_cannot_put_in_a_range_carries_with_its_continue() {
    // Doubling is not a range, so the longhand is all Python has, and the
    // longhand moves where the `continue` lands. The loop carries instead.
    let source = "package main\n\nfunc Double(n int) int {\n\tseen := 0\n\n\t\
                  for i := 1; i < n; i = i * 2 {\n\t\tif i == 4 {\n\t\t\tcontinue\n\t\t}\n\t\t\
                  seen += 1\n\t}\n\n\treturn seen\n}\n";
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "double.go", source, Language::Python);
    assert!(
        out.contains(MARKER) && out.contains("for i := 1; i < n; i = i * 2 {"),
        "the loop is in the output whole, under the marker.\n{out}"
    );
}

#[test]
fn a_javas_counted_for_crosses_too() {
    let source = "public class Counting {\n    static int sum(int n) {\n        \
                  int total = 0;\n\n        for (int i = 0; i < n; i++) {\n            \
                  total = total + i;\n        }\n        return total;\n    }\n}\n";
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Counting.java", source, Language::TypeScript);
    assert!(!out.contains(MARKER), "nothing carried.\n{out}");
    assert!(
        out.contains("for (let i: number = 0; i < n; i++) {"),
        "the declared type of the counter carries with it.\n{out}"
    );
}

#[test]
fn every_target_prints_what_the_go_prints() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();

    if Toolchain::Go.is_available() {
        std::fs::write(dir.join("loops.go"), LOOPS_GO).expect("the source");
        std::fs::write(dir.join("go.mod"), "module loops\n\ngo 1.21\n").expect("the module");
        let ran = Command::new("go")
            .args(["run", "loops.go"])
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
        let python = translated(dir, "loops.go", LOOPS_GO, Language::Python);
        std::fs::write(dir.join("loops.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("loops.py")
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
        let rust = translated(dir, "loops.go", LOOPS_GO, Language::Rust);
        std::fs::write(dir.join("loops.rs"), &rust).expect("the translation");
        let built = Command::new("rustc")
            .arg("-o")
            .arg(dir.join("loops"))
            .arg(dir.join("loops.rs"))
            .output()
            .expect("running rustc");
        assert!(built.status.success(), "{rust}\n{}", said(&built));
        let ran = Command::new(dir.join("loops"))
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

    if Toolchain::Javac.is_available() {
        let java = transpile::plan_to(
            &dir.join("loops.go"),
            Language::Java,
            Some(&dir.join("Loops.java")),
            false,
        )
        .expect("a plan")
        .output;
        std::fs::write(dir.join("Loops.java"), &java).expect("the translation");
        let built = Command::new("javac")
            .arg("Loops.java")
            .current_dir(dir)
            .output()
            .expect("running javac");
        assert!(built.status.success(), "{java}\n{}", said(&built));
        let ran = Command::new("java")
            .arg("Loops")
            .current_dir(dir)
            .output()
            .expect("running java");
        assert!(ran.status.success(), "{java}\n{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            EXPECTED,
            "the Java translation prints something else.\n{java}"
        );
    } else {
        missing.push("javac".to_string());
    }

    require_on_ci("Go's loops, run", &missing);
}
