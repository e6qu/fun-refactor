use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

#[test]
fn initialized_package_is_a_checked_lake_target() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn allowed(ok: bool) -> bool { ok }\n",
    )
    .unwrap();
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

    let scaffolded = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "scaffold", "src/lib.rs::allowed", "--write"])
        .output()
        .expect("fr should scaffold a selected declaration");
    assert!(
        scaffolded.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffolded.stdout),
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let obligation = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "verify", "specs"])
        .output()
        .unwrap();
    assert!(!obligation.status.success());
    let report: serde_json::Value = serde_json::from_slice(&obligation.stdout).unwrap();
    assert_eq!(report["report"]["obligations"], 1, "{report}");
    assert_eq!(report["packages"][0]["passed"], false, "{report}");

    let model_path = workspace.path().join("specs/FrSpecs/SrcLibRsAllowed.lean");
    let scaffold = std::fs::read_to_string(&model_path).unwrap();
    let proved = scaffold.replace(
        "  by\n    -- fr:debt model-semantics\n    sorry\n-- fr:handwritten-end model-and-proofs",
        "  ok\n\naxiom external_policy : Bool\n\ntheorem accepts_true : allowedModel true = true := by rfl\n-- fr:handwritten-end model-and-proofs",
    );
    assert_ne!(proved, scaffold);
    std::fs::write(&model_path, &proved).unwrap();
    let proved_build = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "verify", "specs"])
        .output()
        .unwrap();
    assert!(
        proved_build.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&proved_build.stdout),
        String::from_utf8_lossy(&proved_build.stderr)
    );

    let handwritten_start = proved
        .find("-- fr:handwritten-begin model-and-proofs")
        .unwrap();
    let handwritten_end = proved
        .find("-- fr:handwritten-end model-and-proofs")
        .unwrap()
        + "-- fr:handwritten-end model-and-proofs".len();
    let handwritten = &proved[handwritten_start..handwritten_end];
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn allowed(ok: bool) -> bool { if ok { true } else { false } }\n",
    )
    .unwrap();
    let drift = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "check", "specs", "--strict"])
        .output()
        .unwrap();
    assert!(!drift.status.success());
    assert!(String::from_utf8_lossy(&drift.stdout).contains("stale"));
    let refreshed = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "scaffold", "src/lib.rs::allowed", "--write"])
        .output()
        .unwrap();
    assert!(
        refreshed.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&refreshed.stdout),
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let refreshed_model = std::fs::read_to_string(&model_path).unwrap();
    assert!(refreshed_model.contains(handwritten));
    let refreshed_build = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "verify", "specs"])
        .output()
        .unwrap();
    assert!(
        refreshed_build.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&refreshed_build.stdout),
        String::from_utf8_lossy(&refreshed_build.stderr)
    );
    let evidence = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "evidence", "specs"])
        .output()
        .unwrap();
    assert!(
        evidence.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&evidence.stdout),
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&evidence.stdout).unwrap();
    assert_eq!(evidence["properties"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["properties"][0]["status"], "checked_by_lean");
    assert_eq!(
        evidence["declared_assumptions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(evidence["declared_assumptions"][0]["kind"], "axiom");
    assert_eq!(
        evidence["correspondence"]["proved_implementation_model"],
        false
    );
    assert_eq!(
        evidence["remaining_obligations"].as_array().unwrap().len(),
        1
    );

    std::fs::write(
        &model_path,
        refreshed_model.replace("allowedModel true = true", "allowedModel true = false"),
    )
    .unwrap();
    let broken = Command::new(FR)
        .arg("-C")
        .arg(workspace.path())
        .args(["spec", "verify", "specs"])
        .output()
        .unwrap();
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stdout).contains("failed lake build"));
}
