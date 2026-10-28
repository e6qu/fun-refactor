use std::process::Command;

#[test]
fn retained_structural_migration_preserves_compiled_results_and_exact_reversal() {
    let output = Command::new("python3")
        .args([
            "tools/structural-change-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-10-28-m3-ci-shallow-structural-change",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
