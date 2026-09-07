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
    python(&[
        "tools/agent-eval.py",
        "replay",
        "tests/agent-eval/results/2026-09-07",
    ]);
}
