//! A field that starts at a value keeps it.

mod common;
use common::{require_on_ci, Toolchain};

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::Path;
use std::process::Command;

const POLICY_PY: &str = r#"from dataclasses import dataclass, field


@dataclass
class Policy:
    name: str
    retries: int = 3
    tags: list[str] = field(default_factory=list)


def main() -> None:
    p = Policy("a")

    print(p.retries)
    print(len(p.tags))


main()
"#;

const HOLDER_JAVA: &str = "public class Holder {\n    static class Box {\n        \
                           int seen = 4;\n\n        int peek() {\n            \
                           return seen;\n        }\n    }\n\n    \
                           public static void main(String[] args) {\n        \
                           System.out.println(new Box().peek());\n    }\n}\n";

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
fn typescript_writes_a_class_so_the_field_can_start_somewhere() {
    // An interface declares types and holds no initializer, so a record with a
    // starting value is a class here.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "policy.py", POLICY_PY, Language::TypeScript);
    assert!(out.contains("export class Policy {"), "{out}");
    assert!(out.contains("retries: number = 3;"), "{out}");
    assert!(
        out.contains("tags: string[] = [];"),
        "a default factory builds one per instance, which is what `[]` says.\n{out}"
    );
}

#[test]
fn a_record_with_no_defaults_is_still_an_interface() {
    let source = "from dataclasses import dataclass\n\n\n@dataclass\nclass Point:\n    \
                  x: int\n    y: int\n";
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "point.py", source, Language::TypeScript);
    assert!(out.contains("export interface Point {"), "{out}");
}

#[test]
fn a_java_field_keeps_the_value_it_was_declared_with() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "Holder.java", HOLDER_JAVA, Language::Python);
    assert!(out.contains("seen: int = 4"), "{out}");
}

#[test]
fn rust_keeps_the_value_in_a_default_impl() {
    // A struct field declares no value in Rust.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let out = translated(tmp.path(), "policy.py", POLICY_PY, Language::Rust);
    assert!(out.contains("impl Default for Policy {"), "{out}");
    assert!(out.contains("retries: 3,"), "{out}");
}

#[test]
fn the_defaults_hold_at_run_time() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path();
    let mut missing = Vec::new();
    let expected = "4";

    if Toolchain::Javac.is_available() {
        std::fs::write(dir.join("Holder.java"), HOLDER_JAVA).expect("the source");
        let built = Command::new("javac")
            .arg("Holder.java")
            .current_dir(dir)
            .output()
            .expect("running javac");
        assert!(built.status.success(), "{}", said(&built));
        let ran = Command::new("java")
            .arg("Holder")
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
        let python = translated(dir, "Holder.java", HOLDER_JAVA, Language::Python);
        std::fs::write(dir.join("holder.py"), &python).expect("the translation");
        let ran = Command::new("python3")
            .arg("holder.py")
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

    require_on_ci("a field's starting value, run", &missing);
}
