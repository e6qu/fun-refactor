use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn semantic_delta_evaluation_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/semantic-delta-eval.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    let expected: Value = serde_json::from_slice(
        &fs::read(root().join("tests/agent-eval/semantic-delta.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}
