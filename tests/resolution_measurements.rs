#[test]
fn resolution_measurements_match_complete_baseline_outputs() {
    let output = std::process::Command::new("python3")
        .args([
            "tools/index-resolution-acceptance.py",
            "--verify",
            "tests/agent-eval/results/2026-09-27-index-resolution/result.json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("resolution measurement auditor");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
