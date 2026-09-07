use std::process::Command;

fn python(args: &[&str]) {
    let output = Command::new("python3")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn acceptance_grading_requires_correct_behavior_and_ordered_evidence() {
    python(&["tools/agent_eval/test_harness.py"]);
}

#[test]
fn recorded_agent_patches_pass_upstream_tests_and_independent_oracles() {
    for directory in [
        "tests/agent-eval/results/2026-09-07",
        "tests/agent-eval/results/2026-09-07-context",
    ] {
        python(&["tools/agent-eval.py", "replay", directory]);
    }
}

#[test]
fn smaller_history_reports_preserve_both_recorded_agent_edits() {
    python(&["tools/history-context.py", "--fr", env!("CARGO_BIN_EXE_fr")]);
}
