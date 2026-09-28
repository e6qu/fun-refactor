use std::process::Command;

#[test]
fn retained_model_comparisons_check_truth_tables_proofs_and_receiver_reversal() {
    let output = Command::new("python3")
        .args([
            "tools/refinement-acceptance.py",
            "--fr",
            env!("CARGO_BIN_EXE_fr"),
            "--audit",
            "tests/agent-eval/results/2026-09-28-virtual-model-comparisons",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
