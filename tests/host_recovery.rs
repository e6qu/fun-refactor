#[test]
fn retained_host_recovery_evidence_matches_sources_and_oracles() {
    let output = std::process::Command::new("python3")
        .args([
            "tools/host-recovery-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-25-host-recovery/result.json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
