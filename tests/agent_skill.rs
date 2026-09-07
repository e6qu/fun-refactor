use std::process::Command;

#[test]
fn packaged_agent_skill_examples_complete_the_documented_workflows() {
    let output = Command::new("python3")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "tools/check-agent-skill.py",
            "--fr",
            env!("CARGO_BIN_EXE_fr"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["passed"], true);
    assert!(result["shell_examples"].as_u64().unwrap() > 20);
}
