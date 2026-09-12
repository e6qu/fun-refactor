use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn retained_semantic_delta_agent_pair_is_complete_and_digest_bound() {
    let evidence = root().join("tests/agent-eval/results/2026-09-12-semantic-delta");
    let manifest: Value =
        serde_json::from_slice(&fs::read(evidence.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["model"], "gpt-5.6-luna");
    assert_eq!(manifest["reasoning_effort"], "low");
    for (name, expected) in manifest["files"].as_object().unwrap() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(fs::read(evidence.join(name)).unwrap())
            ),
            expected.as_str().unwrap()
        );
    }
    let delta: Value =
        serde_json::from_slice(&fs::read(evidence.join("semantic-delta-fr/result.json")).unwrap())
            .unwrap();
    let whole: Value = serde_json::from_slice(
        &fs::read(evidence.join("semantic-delta-files/result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(delta["route"], "semantic-delta");
    assert_eq!(whole["route"], "complete-body");
    for result in [delta, whole] {
        assert_eq!(result["passed"], true);
        assert_eq!(result["exact_semantic_body"], true);
        assert_eq!(result["behavior_passed"], true);
        assert_eq!(result["direct_source_reads"], 0);
    }
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
