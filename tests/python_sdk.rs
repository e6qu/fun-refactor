use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn python() -> Command {
    let mut command = Command::new("python3");
    command.env("PYTHONPATH", root().join("sdk/python/src"));
    command
}

#[test]
fn python_sdk_unit_tests_pass_without_dependencies() {
    let output = python()
        .args(["-m", "unittest", "discover", "-s"])
        .arg(root().join("sdk/python/tests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn python_and_rust_publish_the_same_semantic_catalog() {
    let script = "import json; from fr_ir import *; print(json.dumps([TYPE_KINDS, STATEMENT_KINDS, EXPRESSION_KINDS, TEMPLATE_KINDS, [x.value for x in BinaryOp], [x.value for x in UnaryOp]]))";
    let output = python().args(["-c", script]).output().unwrap();
    assert!(output.status.success());
    let python: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let rust = [
        fun_refactor::project::semantic_ir::TYPE_KINDS,
        &fun_refactor::project::semantic_ir::STATEMENT_KINDS[..25],
        &fun_refactor::project::semantic_ir::EXPRESSION_KINDS[..28],
        fun_refactor::project::semantic_ir::TEMPLATE_KINDS,
        fun_refactor::project::semantic_ir::BINARY_OPERATORS,
        fun_refactor::project::semantic_ir::UNARY_OPERATORS,
    ];
    for (python, rust) in python.iter().zip(rust) {
        assert_eq!(python.iter().map(String::as_str).collect::<Vec<_>>(), rust);
    }
}

#[test]
fn every_python_constructor_deserializes_and_canonicalizes_in_rust() {
    let generated = python()
        .arg(root().join("sdk/python/examples/exhaustive.py"))
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("semantic.json");
    fs::write(&input, &generated.stdout).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(temp.path())
        .args(["author", "validate-semantic", "--from"])
        .arg(&input)
        .arg("--canonical")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(output.status.success(), "{report}");
    assert_eq!(report["valid"], true);
    assert_eq!(report["source_free"], true);
    assert_eq!(report["statements"], 65);
    assert_eq!(report["canonical"]["schema"], "fr-semantic-body-1");
}
