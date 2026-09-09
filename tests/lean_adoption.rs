use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

#[test]
fn initialized_package_is_a_checked_lake_target() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    let initialized = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "init", "--write"])
        .output()
        .expect("fr should initialize a Lean package");
    assert!(
        initialized.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&initialized.stdout),
        String::from_utf8_lossy(&initialized.stderr)
    );

    let verified = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "verify", "specs"])
        .output()
        .expect("fr should run the initialized package's checked target");
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(report["report"]["anchors"], serde_json::json!([]));
    assert_eq!(report["report"]["obligations"], 0);
    assert_eq!(report["packages"].as_array().unwrap().len(), 1);
    assert_eq!(report["packages"][0]["passed"], true, "{report}");
}
