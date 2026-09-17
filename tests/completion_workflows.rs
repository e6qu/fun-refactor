use std::process::Command;

#[test]
fn retained_completion_workflows_cover_every_family_without_post_guide_discovery() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("python3")
        .arg(root.join("tools/completion-workflows.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--audit")
        .arg(root.join("tests/agent-eval/completion-workflows.json"))
        .output()
        .expect("completion workflow auditor should run");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["totals"]["workflow_families"], 7);
    assert_eq!(report["totals"]["manual_exploratory_calls"], 14);
    assert_eq!(report["totals"]["post_guide_exploratory_calls"], 0);
    assert!(report["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["source_state"]["unchanged"] == true));
}
