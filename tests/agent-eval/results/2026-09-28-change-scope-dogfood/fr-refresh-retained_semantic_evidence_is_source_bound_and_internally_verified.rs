{
    let output = Command::new("python3")
        .args([
            "tools/semantic-evidence-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-28-change-scope-semantic-evidence/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}