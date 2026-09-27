//! Retained measurements must match the implementation and pinned workload.

#[test]
fn consumer_measurements_match_their_source_and_baseline() {
    let output = std::process::Command::new("python3")
        .args([
            "tools/index-consumers-acceptance.py",
            "--verify",
            "tests/agent-eval/results/2026-09-27-index-consumers/result.json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("measurement auditor");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
