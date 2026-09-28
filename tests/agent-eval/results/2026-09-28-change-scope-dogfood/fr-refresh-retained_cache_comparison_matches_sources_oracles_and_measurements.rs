{
    let output = Command::new("python3")
        .args([
            "tools/flow-cache-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-28-change-scope-flow-cache/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}