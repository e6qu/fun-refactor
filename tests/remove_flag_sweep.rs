//! `fr remove-flag`, driven in every language that claims it.
//!
//! The one writing command the sweeps never reached, because this repository has no
//! boolean flag to point it at. These fixtures are that corpus: one flag, one guarded
//! branch and one live statement per language. The tool's own reparse gate checks the
//! result parses; the assertions here check the cascade itself.

use std::path::Path;
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn run(root: &Path, args: &[&str]) -> (String, bool) {
    let cache = tempfile::tempdir().unwrap();
    let output = Command::new(FR)
        .arg("-C")
        .arg(root)
        .args(args)
        .env("FUN_REFACTOR_CACHE", cache.path())
        .output()
        .expect("fr should run");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text, output.status.success())
}

fn swept(file: &str, source: &str, gone: &[&str], kept: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(file), source).unwrap();
    let (text, ok) = run(tmp.path(), &["remove-flag", "SHINY", "--write"]);
    assert!(ok, "{file}: {text}");
    let after = std::fs::read_to_string(tmp.path().join(file)).unwrap();
    for needle in gone {
        assert!(
            !after.contains(needle),
            "{file}: `{needle}` should be gone:\n{after}"
        );
    }
    for needle in kept {
        assert!(
            after.contains(needle),
            "{file}: `{needle}` should survive:\n{after}"
        );
    }
}

#[test]
fn rust_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.rs",
        "pub const SHINY: bool = true;\n\n\
         pub fn greet() -> i64 {\n    if SHINY {\n        return 2;\n    }\n    return 1;\n}\n",
        &["SHINY"],
        &["return 2", "return 1"],
    );
}

#[test]
fn go_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.go",
        "package p\n\nconst SHINY = true\n\n\
         func Greet() int {\n\tif SHINY {\n\t\treturn 2\n\t}\n\treturn 1\n}\n",
        &["SHINY"],
        &["return 2", "return 1"],
    );
}

#[test]
fn python_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.py",
        "SHINY = True\n\n\ndef greet() -> int:\n    if SHINY:\n        return 2\n    return 1\n",
        &["SHINY"],
        &["return 2"],
    );
}

#[test]
fn typescript_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.ts",
        "export const SHINY = true;\n\n\
         export function greet(): number {\n    if (SHINY) {\n        return 2;\n    }\n    return 1;\n}\n",
        &["SHINY"],
        &["return 2", "return 1"],
    );
}

#[test]
fn zig_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.zig",
        "pub const SHINY = true;\n\n\
         pub fn greet() i64 {\n    if (SHINY) {\n        return 2;\n    }\n    return 1;\n}\n",
        &["SHINY"],
        &["return 2", "return 1"],
    );
}

#[test]
fn java_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "A.java",
        "public final class A {\n  static final boolean SHINY = true;\n\n  \
         static int greet() {\n    if (SHINY) {\n      return 2;\n    }\n    return 1;\n  }\n}\n",
        &["SHINY"],
        &["return 2", "return 1"],
    );
}

#[test]
fn bash_removes_the_flag_and_keeps_the_live_branch() {
    swept(
        "a.sh",
        "#!/bin/bash\nSHINY=true\n\nif [ \"$SHINY\" = true ]; then\n  echo two\nelse\n  echo one\nfi\n",
        &["SHINY"],
        &["echo two"],
    );
}
