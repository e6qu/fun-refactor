#[test]
fn native_trials_review_and_apply_public_body_edits() {
    let output = std::process::Command::new("python3")
        .args(["-B", "tools/agent_eval/test_native_author.py", "RealAuthor"])
        .env("FR_NATIVE_AUTHOR_BINARY", env!("CARGO_BIN_EXE_fr"))
        .output()
        .expect("python3 runs the native author control");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
