//! A field named with no receiver still reaches the field.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const COUNTER_JAVA: &str = r#"public class Counter {
    static class Box {
        int total;
        int stepBy;

        Box(int stepBy) {
            this.total = 0;
            this.stepBy = stepBy;
        }

        void bump() {
            total = total + stepBy;
        }

        int peek() {
            int stepBy = 100;
            return total + stepBy;
        }
    }

    public static void main(String[] args) {
        Box b = new Box(3);
        b.bump();
        b.bump();
        System.out.println(b.peek());
    }
}
"#;

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
fn a_bare_field_is_written_through_this_in_typescript() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(
        tmp.path(),
        "Counter.java",
        COUNTER_JAVA,
        Language::TypeScript,
    );
    assert!(
        out.contains("this.total = this.total + this.stepBy;"),
        "every bare field reaches the receiver.\n{out}"
    );
}

#[test]
fn a_bare_field_takes_the_field_tables_spelling_in_python() {
    // The declaration says `step_by` and the body said `stepBy`.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Counter.java", COUNTER_JAVA, Language::Python);
    assert!(
        out.contains("self.total = self.total + self.step_by"),
        "the body spells the field as the declaration does.\n{out}"
    );
    assert!(
        !out.contains("stepBy"),
        "no Java spelling of the field survives into Python.\n{out}"
    );
}

#[test]
fn a_local_of_the_fields_name_keeps_the_local() {
    // `int stepBy = 100;` inside `peek` is the nearer declaration in Java and in every target.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(
        tmp.path(),
        "Counter.java",
        COUNTER_JAVA,
        Language::TypeScript,
    );
    assert!(
        out.contains("return this.total + stepBy;"),
        "the local wins over the field of the same name.\n{out}"
    );
}

#[test]
fn the_translated_class_compiles_and_prints_what_java_prints() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();
    let expected = "106";

    if Toolchain::Javac.is_available() {
        std::fs::write(dir.join("Counter.java"), COUNTER_JAVA).expect("the source");
        let built = Command::new("javac")
            .arg("Counter.java")
            .current_dir(dir)
            .output()
            .expect("running javac");
        assert!(built.status.success(), "{}", said(&built));
        let ran = Command::new("java")
            .arg("Counter")
            .current_dir(dir)
            .output()
            .expect("running java");
        assert!(ran.status.success(), "{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            expected,
            "the Java source no longer prints what this gate expects."
        );
    } else {
        missing.push("javac".to_string());
    }

    if Toolchain::Python.is_available() {
        let python = translated(dir, "Counter.java", COUNTER_JAVA, Language::Python);
        std::fs::write(dir.join("counter.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("counter.py")
            .current_dir(dir)
            .output()
            .expect("running python3");
        assert!(ran.status.success(), "{python}\n{}", said(&ran));
        assert_eq!(
            String::from_utf8_lossy(&ran.stdout).trim(),
            expected,
            "the Python translation prints something else.\n{python}"
        );
    } else {
        missing.push("python3".to_string());
    }

    if Toolchain::Tsc.is_available() {
        let ts = translated(dir, "Counter.java", COUNTER_JAVA, Language::TypeScript);
        std::fs::write(dir.join("counter.ts"), &ts).expect("the translation");
        let checked = Command::new("tsc")
            .current_dir(dir)
            .args(["--strict", "--target", "es2022", "--outDir", "js"])
            .arg("counter.ts")
            .output()
            .expect("running tsc");
        assert!(
            checked.status.success(),
            "the translated class does not satisfy tsc --strict:\n{ts}\n{}",
            said(&checked)
        );
        if Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            let ran = Command::new("node")
                .arg("js/counter.js")
                .current_dir(dir)
                .output()
                .expect("running node");
            assert!(ran.status.success(), "{ts}\n{}", said(&ran));
            assert_eq!(
                String::from_utf8_lossy(&ran.stdout).trim(),
                expected,
                "the TypeScript translation prints something else.\n{ts}"
            );
        } else {
            missing.push("node".to_string());
        }
    } else {
        missing.push("tsc".to_string());
    }

    require_on_ci("a bare field through the receiver, run", &missing);
}
