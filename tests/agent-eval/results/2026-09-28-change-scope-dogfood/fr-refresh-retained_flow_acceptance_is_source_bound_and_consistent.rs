{
    let output = Command::new("python3")
        .args([
            "tools/flow-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-28-change-scope-flow-fixed-point/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}