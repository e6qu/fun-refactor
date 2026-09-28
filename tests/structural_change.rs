use std::process::Command;

#[test]
fn retained_structural_migration_preserves_compiled_results_and_exact_reversal() {
    let output = Command::new("python3")
        .args([
            "tools/structural-change-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-28-structural-change",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
