use std::process::Command;

#[test]
fn guided_multi_body_review_requires_the_complete_target_set() {
    let root = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("python3")
        .arg(format!("{root}/tools/source-bodies-context.py"))
        .args(["--fr", env!("CARGO_BIN_EXE_fr")])
        .env("PYTHONPATH", format!("{root}/sdk/python/src"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["passed"], true);
    assert_eq!(result["wrong_target_sets_refused"], 2);
    assert_eq!(result["delivery_stages"], 8);
}
