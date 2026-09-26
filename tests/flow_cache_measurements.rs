use std::process::Command;

#[test]
fn release_versions_preserve_evidence_but_build_changes_invalidate_it() {
    let output = Command::new("python3")
        .arg("tools/test_flow_cache_basis.py")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn retained_cache_comparison_matches_sources_oracles_and_measurements() {
    let output = Command::new("python3")
        .args([
            "tools/flow-cache-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-25-flow-cache/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
