use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn run(workspace: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(FR)
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn agent_formalization_plan_scaffold_disclose_prove_and_history_flow() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn keep(value: bool) -> bool { value }\n\
         pub fn invert(value: bool) -> bool { !value }\n\
         pub fn effect(value: bool) -> bool { println!(\"x\"); value }\n\
         pub unsafe fn danger(value: bool) -> bool { value }\n",
    )
    .unwrap();

    let init = run(workspace.path(), &["spec", "init", "--write"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let candidates = run(
        workspace.path(),
        &["--json", "spec", "candidates", "src/lib.rs"],
    );
    assert!(
        candidates.status.success(),
        "{}",
        String::from_utf8_lossy(&candidates.stderr)
    );
    let candidates: serde_json::Value = serde_json::from_slice(&candidates.stdout).unwrap();
    let rows = candidates["candidates"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().filter(|row| row["eligible"] == true).count(), 2);
    assert!(rows
        .iter()
        .any(|row| row["target"] == "src/lib.rs::effect" && row["eligible"] == false));
    assert!(rows.iter().any(|row| row["target"] == "src/lib.rs::danger"
        && row["eligible"] == false
        && row["reason"].as_str().unwrap().contains("unsafe")));

    let planned = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "plan",
            "src/lib.rs::keep",
            "--property",
            "identity",
            "--property",
            "idempotent",
        ],
    );
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(plan["schema"], "fr-formal-plan-1");
    assert_eq!(plan["kernel"]["semantic_ir"][0]["kind"], "return");
    assert!(!String::from_utf8_lossy(&planned.stdout).contains("pub fn keep"));
    let plan_path = workspace.path().join("formal-plan.json");
    std::fs::write(&plan_path, &planned.stdout).unwrap();

    let scaffold = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "scaffold",
            "--from",
            "formal-plan.json",
            "--write",
        ],
    );
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let model = workspace.path().join("specs/FrSpecs/SrcLibRsKeep.lean");
    let generated = std::fs::read_to_string(&model).unwrap();
    assert!(generated.contains("def keepModel (value : Bool) : Bool :=\n  value"));
    assert_eq!(generated.matches("-- fr:debt").count(), 2);

    let goals = run(workspace.path(), &["--json", "spec", "goals", "specs"]);
    assert!(
        goals.status.success(),
        "{}",
        String::from_utf8_lossy(&goals.stderr)
    );
    let goals: serde_json::Value = serde_json::from_slice(&goals.stdout).unwrap();
    assert_eq!(goals["schema"], "fr-formal-goals-1");
    assert_eq!(goals["catalog"].as_array().unwrap().len(), 2);
    assert!(goals["revealed"].is_null());
    assert!(goals["token_budget"]["used_upper_bound"].as_u64().unwrap() <= 4096);
    let goal = goals["catalog"][0]["id"].as_str().unwrap();
    let revealed = run(
        workspace.path(),
        &["--json", "spec", "goals", "specs", "--goal", goal],
    );
    assert!(
        revealed.status.success(),
        "{}",
        String::from_utf8_lossy(&revealed.stderr)
    );
    let revealed: serde_json::Value = serde_json::from_slice(&revealed.stdout).unwrap();
    assert_eq!(revealed["revealed"]["id"], goal);
    assert!(revealed["revealed"]["theorem"]
        .as_str()
        .unwrap()
        .starts_with("theorem "));

    std::fs::write(workspace.path().join("proof.lean"), "rfl\n").unwrap();
    let obligation = revealed["revealed"]["name"].as_str().unwrap();
    let target = format!("specs/FrSpecs/SrcLibRsKeep.lean::{obligation}");
    let proof_task = run(workspace.path(), &["--json", "spec", "proof-task", &target]);
    assert!(
        proof_task.status.success(),
        "{}",
        String::from_utf8_lossy(&proof_task.stderr)
    );
    let proof_task: serde_json::Value = serde_json::from_slice(&proof_task.stdout).unwrap();
    assert_eq!(proof_task["schema"], "fr-proof-task-1");
    assert_eq!(proof_task["contract"]["author"], "agent");
    assert!(proof_task["templates"][0]["lines"][0]
        .as_str()
        .unwrap()
        .contains("agent-written"));
    assert!(!proof_task.to_string().contains("rfl"));
    let source_path = workspace.path().join("src/lib.rs");
    let current_source = std::fs::read_to_string(&source_path).unwrap();
    std::fs::write(
        &source_path,
        current_source.replace("{ value }", "{ !value }"),
    )
    .unwrap();
    let stale_task = run(workspace.path(), &["spec", "proof-task", &target]);
    assert!(!stale_task.status.success());
    assert!(String::from_utf8_lossy(&stale_task.stderr).contains("stale source"));
    std::fs::write(&source_path, current_source).unwrap();

    std::fs::write(workspace.path().join("wrong-proof.lean"), "exact false\n").unwrap();
    let before_failed_attempt = std::fs::read_to_string(&model).unwrap();
    let failed = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "proof-check",
            &target,
            "--from",
            "wrong-proof.lean",
            "--token-limit",
            "2048",
        ],
    );
    assert!(
        failed.status.success(),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    let failed: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(failed["schema"], "fr-proof-attempt-1");
    assert_eq!(failed["passed"], false);
    assert!(failed["receipt"].is_null());
    assert!(!failed["diagnostics"].as_array().unwrap().is_empty());
    assert!(failed["token_budget"]["used_upper_bound"].as_u64().unwrap() <= 2048);
    assert_eq!(
        std::fs::read_to_string(&model).unwrap(),
        before_failed_attempt
    );
    let rejected_write = run(
        workspace.path(),
        &[
            "spec",
            "prove",
            &target,
            "--from",
            "wrong-proof.lean",
            "--write",
        ],
    );
    assert!(!rejected_write.status.success());
    assert!(
        String::from_utf8_lossy(&rejected_write.stderr).contains("proof failed Lean verification")
    );
    assert_eq!(
        std::fs::read_to_string(&model).unwrap(),
        before_failed_attempt
    );

    let checked = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "proof-check",
            &target,
            "--from",
            "proof.lean",
        ],
    );
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(checked["passed"], true);
    let checked_receipt = checked["receipt"].as_str().unwrap();
    let proved = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "prove",
            &target,
            "--from",
            "proof.lean",
            "--write",
        ],
    );
    assert!(
        proved.status.success(),
        "{}",
        String::from_utf8_lossy(&proved.stderr)
    );
    let proved_json: serde_json::Value = serde_json::from_slice(&proved.stdout).unwrap();
    assert_eq!(proved_json["receipt"], checked_receipt);
    let transaction = proved_json["transaction"].as_u64().unwrap().to_string();
    let after = std::fs::read_to_string(&model).unwrap();
    assert_eq!(after.matches("-- fr:debt").count(), 1);
    assert!(after.contains("  rfl\n"));
    assert!(run(
        workspace.path(),
        &["history", "undo", &transaction, "--write"]
    )
    .status
    .success());
    assert_eq!(std::fs::read_to_string(&model).unwrap(), generated);
    assert!(run(
        workspace.path(),
        &["history", "redo", &transaction, "--write"]
    )
    .status
    .success());
    assert_eq!(std::fs::read_to_string(&model).unwrap(), after);

    let remaining = run(workspace.path(), &["--json", "spec", "goals", "specs"]);
    let remaining: serde_json::Value = serde_json::from_slice(&remaining.stdout).unwrap();
    let obligation = remaining["catalog"][0]["name"].as_str().unwrap();
    let target = format!("specs/FrSpecs/SrcLibRsKeep.lean::{obligation}");
    assert!(run(
        workspace.path(),
        &["spec", "prove", &target, "--from", "proof.lean", "--write"]
    )
    .status
    .success());
    let verified = run(workspace.path(), &["spec", "verify", "specs"]);
    assert!(
        verified.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );

    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn keep(value: bool) -> bool { !value }\n",
    )
    .unwrap();
    let stale = run(
        workspace.path(),
        &["spec", "scaffold", "--from", "formal-plan.json", "--write"],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("does not match the current source"));
}

#[test]
fn agent_authors_a_multi_input_property_and_its_proof() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn both(left: bool, right: bool) -> bool { left && right }\n",
    )
    .unwrap();
    assert!(run(workspace.path(), &["spec", "init", "--write"])
        .status
        .success());

    let task = run(
        workspace.path(),
        &["--json", "spec", "property-task", "src/lib.rs::both"],
    );
    assert!(
        task.status.success(),
        "{}",
        String::from_utf8_lossy(&task.stderr)
    );
    let task: serde_json::Value = serde_json::from_slice(&task.stdout).unwrap();
    assert_eq!(task["schema"], "fr-property-task-1");
    assert_eq!(task["contract"]["author"], "agent");
    assert_eq!(task["contract"]["proof_author"], "agent");
    assert_eq!(task["kernel"]["inputs"].as_array().unwrap().len(), 2);
    assert!(!task.to_string().contains("pub fn both"));
    assert!(!task.to_string().contains("rfl"));

    let property = serde_json::json!({
        "schema": "fr-formal-property-1",
        "task_digest": task["object_digest"],
        "name": "commutative",
        "parameters": [
            {"name": "x", "lean_type": "Bool"},
            {"name": "y", "lean_type": "Bool"}
        ],
        "proposition": {
            "kind": "equals",
            "left": {
                "kind": "model",
                "arguments": [
                    {"kind": "variable", "name": "x"},
                    {"kind": "variable", "name": "y"}
                ]
            },
            "right": {
                "kind": "model",
                "arguments": [
                    {"kind": "variable", "name": "y"},
                    {"kind": "variable", "name": "x"}
                ]
            }
        }
    });
    std::fs::write(
        workspace.path().join("property.json"),
        serde_json::to_vec_pretty(&property).unwrap(),
    )
    .unwrap();
    let mut stale_property = property.clone();
    stale_property["task_digest"] = serde_json::Value::String("0".repeat(64));
    std::fs::write(
        workspace.path().join("stale-property.json"),
        serde_json::to_vec_pretty(&stale_property).unwrap(),
    )
    .unwrap();
    let stale = run(
        workspace.path(),
        &[
            "spec",
            "plan",
            "src/lib.rs::both",
            "--property-from",
            "stale-property.json",
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("task identity"));

    let mut bad_type = property.clone();
    bad_type["parameters"][0]["lean_type"] = serde_json::Value::String("String".into());
    let mut unknown_variable = property.clone();
    unknown_variable["proposition"]["left"]["arguments"][0]["name"] =
        serde_json::Value::String("missing".into());
    let mut wrong_arity = property.clone();
    wrong_arity["proposition"]["left"]["arguments"] = serde_json::json!([]);
    let mut unsafe_name = property.clone();
    unsafe_name["name"] = serde_json::Value::String("theorem".into());
    let mut too_deep = property.clone();
    let mut deep_proposition = property["proposition"].clone();
    for _ in 0..16 {
        deep_proposition = serde_json::json!({
            "kind": "not",
            "proposition": deep_proposition
        });
    }
    too_deep["proposition"] = deep_proposition;
    let mut too_many_nodes = property.clone();
    let mut wide_proposition = property["proposition"].clone();
    for _ in 0..4 {
        wide_proposition = serde_json::json!({
            "kind": "and",
            "propositions": [wide_proposition.clone(), wide_proposition]
        });
    }
    too_many_nodes["proposition"] = wide_proposition;
    for (index, (invalid, message)) in [
        (bad_type, "outside the disclosed kernel signature"),
        (unknown_variable, "is not a parameter"),
        (wrong_arity, "wrong arity"),
        (unsafe_name, "safe Lean identifier"),
        (too_deep, "16-level depth ceiling"),
        (too_many_nodes, "64-node ceiling"),
    ]
    .into_iter()
    .enumerate()
    {
        let file = format!("invalid-property-{index}.json");
        std::fs::write(
            workspace.path().join(&file),
            serde_json::to_vec_pretty(&invalid).unwrap(),
        )
        .unwrap();
        let rejected = run(
            workspace.path(),
            &["spec", "plan", "src/lib.rs::both", "--property-from", &file],
        );
        assert!(!rejected.status.success());
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }

    let planned = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "plan",
            "src/lib.rs::both",
            "--property-from",
            "property.json",
        ],
    );
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(plan["properties"][0]["kind"], "agent");
    assert_eq!(plan["properties"][0]["agent_spec"], property);
    assert_eq!(
        plan["properties"][0]["proposition"],
        "(x : Bool) (y : Bool) : (bothModel (x) (y)) = (bothModel (y) (x))"
    );
    std::fs::write(workspace.path().join("formal-plan.json"), &planned.stdout).unwrap();

    let scaffold = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "scaffold",
            "--from",
            "formal-plan.json",
            "--write",
        ],
    );
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let model = workspace.path().join("specs/FrSpecs/SrcLibRsBoth.lean");
    let unproved = std::fs::read_to_string(&model).unwrap();
    assert!(unproved.contains("theorem bothModel_commutative"));
    assert!(unproved.contains("-- fr:debt bothModel_commutative"));

    std::fs::write(
        workspace.path().join("proof.lean"),
        "cases x <;> cases y <;> rfl\n",
    )
    .unwrap();
    let target = "specs/FrSpecs/SrcLibRsBoth.lean::bothModel_commutative";
    let checked = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "proof-check",
            target,
            "--from",
            "proof.lean",
        ],
    );
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(checked["passed"], true);
    let proved = run(
        workspace.path(),
        &[
            "--json",
            "spec",
            "prove",
            target,
            "--from",
            "proof.lean",
            "--write",
        ],
    );
    assert!(
        proved.status.success(),
        "{}",
        String::from_utf8_lossy(&proved.stderr)
    );
    let proved: serde_json::Value = serde_json::from_slice(&proved.stdout).unwrap();
    let transaction = proved["transaction"].as_u64().unwrap().to_string();
    assert!(run(workspace.path(), &["spec", "verify", "specs"])
        .status
        .success());
    assert!(run(
        workspace.path(),
        &["history", "undo", &transaction, "--write"]
    )
    .status
    .success());
    assert_eq!(std::fs::read_to_string(&model).unwrap(), unproved);
    assert!(run(
        workspace.path(),
        &["history", "redo", &transaction, "--write"]
    )
    .status
    .success());

    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn both(left: bool, right: bool) -> bool { left || right }\n",
    )
    .unwrap();
    let stale = run(
        workspace.path(),
        &[
            "spec",
            "plan",
            "src/lib.rs::both",
            "--property-from",
            "property.json",
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("task identity"));
}

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
