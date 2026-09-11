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
#[ignore = "requires the pinned regex workspace dependencies; see docs/agent-workspace-evaluation.md"]
fn recorded_workspace_patches_pass_checks_oracles_and_exact_reversal() {
    for directory in [
        "tests/agent-eval/results/2026-09-08-regex",
        "tests/agent-eval/results/2026-09-08-coordinated",
        "tests/agent-eval/results/2026-09-09-structural-authoring",
        "tests/agent-eval/results/2026-09-11-context-v3",
        "tests/agent-eval/results/2026-09-11-workflow-v4",
    ] {
        python(&["tools/agent-eval.py", "replay", directory]);
    }
}

#[test]
fn smaller_history_reports_preserve_both_recorded_agent_edits() {
    python(&["tools/history-context.py", "--fr", env!("CARGO_BIN_EXE_fr")]);
}

#[test]
fn matched_check_policies_preserve_recorded_outcomes_and_live_diagnostics() {
    python(&[
        "tools/checks-policy-context.py",
        "--fr",
        env!("CARGO_BIN_EXE_fr"),
    ]);
}

#[test]
fn batch_and_individual_workflows_match_source_behavior_and_reversal() {
    python(&[
        "tools/author-batch-context.py",
        "--fr",
        env!("CARGO_BIN_EXE_fr"),
        "--repetitions",
        "1",
    ]);
}

#[test]
fn batched_project_queries_match_separate_compact_reports() {
    python(&[
        "tools/project-batch-context.py",
        "--fr",
        env!("CARGO_BIN_EXE_fr"),
        "--repetitions",
        "1",
    ]);
}
