use std::process::Command;

#[test]
fn release_labels_preserve_evidence_but_sdk_changes_invalidate_it() {
    let output = Command::new("python3")
        .args(["tools/test-acceptance-release-inputs.py"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn retained_structural_migration_preserves_compiled_results_and_exact_reversal() {
    let output = Command::new("python3")
        .args([
            "tools/structural-change-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-11-08-native-decide-structural-change",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
