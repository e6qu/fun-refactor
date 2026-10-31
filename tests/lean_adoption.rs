use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

#[test]
fn dependency_changes_expire_proof_tasks_and_rebuild_cached_imports() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("src")).unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn keep(value: bool) -> bool { value }\n",
    )
    .unwrap();
    assert!(run(workspace.path(), &["spec", "init", "specs", "--write"])
        .status
        .success());
    let lockfile = workspace.path().join("specs/lake-manifest.json");
    let initialized_lockfile = std::fs::read(&lockfile).unwrap();
    let plan =
        fun_refactor::spec::formal_plan(workspace.path(), "src/lib.rs::keep", &["identity".into()])
            .unwrap();
    std::fs::write(
        workspace.path().join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let scaffold = run(
        workspace.path(),
        &[
            "spec",
            "scaffold",
            "--from",
            "plan.json",
            "--package",
            "specs",
            "--write",
        ],
    );
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let model = workspace.path().join("specs/FrSpecs/SrcLibRsKeep.lean");
    let original = std::fs::read_to_string(&model).unwrap();
    std::fs::write(
        &model,
        format!(
            "import FrSpecs.Policy\n\n{}",
            original.replacen("  value\n", "  Policy.choose value\n", 1)
        ),
    )
    .unwrap();
    let dependency = workspace.path().join("specs/FrSpecs/Policy.lean");
    let policy =
        "namespace FrSpecs.Policy\ndef choose (value : Bool) : Bool := value\nend FrSpecs.Policy\n";
    std::fs::write(&dependency, policy).unwrap();
    let built = Command::new("lake")
        .arg("build")
        .current_dir(workspace.path().join("specs"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert_eq!(std::fs::read(&lockfile).unwrap(), initialized_lockfile);
    let target = "specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity";
    let first_task = fun_refactor::spec::proof_task(workspace.path(), target, 4096, true).unwrap();
    std::fs::write(workspace.path().join("proof.lean"), "rfl\n").unwrap();
    let first = run(
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
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["passed"], true);
    let before_show = std::fs::read(&model).unwrap();
    for tactics in [
        "refine ?_\nrfl\n",
        "have nonnegative (a b : Int) (ha : 0 ≤ a) (hb : 0 ≤ b) : 0 ≤ a * b := by\n  exact?\nrfl\n",
        include_str!("fixtures/lean-local-lemma-bound.tactics"),
        include_str!("fixtures/lean-anonymous-lemma.tactics"),
        include_str!("fixtures/lean-discard-proof.tactics"),
        "have h : (if True then keepModel x else !x) = x := by\n  simp only [↓reduceIte]\n  rfl\nexact h\n",
        "have h : keepModel x = x ∧ keepModel x = x := by\n  refine ⟨?_, ?_⟩\n  · rfl\n  · rfl\nexact h.1\n",
        "exact (show keepModel x = x by rfl)\n",
        "exact (show keepModel x = x from rfl)\n",
        "exact (by rfl : keepModel x = x)\n",
        "  have h : keepModel x = x := by rfl\n  exact h\n",
        "\n  \n    have h : keepModel x = x := by\n      rfl\n    exact h\n  \n\n",
        "let q := x\nhave : q = x := by rfl\ncalc\n  keepModel x = q := by rfl\n  _ = x := this\n",
        "calc keepModel x = x := rfl\n  _ = x := rfl\n",
        "exact calc keepModel x = x := rfl\n  _ = x := rfl\n",
        "have h : keepModel x = x := by\n  calc keepModel x = x := rfl\n    _ = x := rfl\nexact h\n",
    ] {
        let proof = workspace.path().join("proof.lean");
        std::fs::write(&proof, tactics).unwrap();
        let checked = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096)
            .unwrap_or_else(|error| panic!("{tactics:?}: {error:#}"));
        assert!(
            checked.passed,
            "{}",
            serde_json::to_string(&checked).unwrap()
        );
        assert_eq!(std::fs::read(&model).unwrap(), before_show);
    }
    let proof = workspace.path().join("proof.lean");
    std::fs::write(&proof, "simp only [keepModel, Bool.not_not]\n").unwrap();
    let warned = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(
        !warned.passed,
        "unused simp arguments must fail before saving"
    );
    assert!(warned.receipt.is_none());
    let warning = warned
        .diagnostics
        .iter()
        .find(|item| item.message.contains("simp argument is unused"))
        .unwrap();
    assert_eq!(warning.severity, "warning");
    let location = warning.draft_location.as_ref().unwrap();
    assert_eq!(location.line, 1);
    assert!(location
        .source_excerpt
        .as_ref()
        .unwrap()
        .contains("Bool.not_not"));
    assert!(fun_refactor::spec::prove(workspace.path(), target, &proof).is_err());
    assert_eq!(std::fs::read(&model).unwrap(), before_show);

    let with_other_debt = format!(
        "{}\nexample : True := by sorry\n",
        std::str::from_utf8(&before_show).unwrap()
    );
    std::fs::write(&model, &with_other_debt).unwrap();
    std::fs::write(&proof, "rfl\n").unwrap();
    let clean = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(clean.passed, "{}", serde_json::to_string(&clean).unwrap());
    assert!(clean.diagnostics.is_empty());
    assert_eq!(std::fs::read_to_string(&model).unwrap(), with_other_debt);
    std::fs::write(&model, &before_show).unwrap();

    std::fs::write(&proof, "refine ?_\n").unwrap();
    let unfinished =
        fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(!unfinished.passed);
    assert!(!unfinished.diagnostics.is_empty());
    assert_eq!(std::fs::read(&model).unwrap(), before_show);
    std::fs::write(
        &proof,
        "\n\n  have h : True := by trivial\n  unknown_step\n",
    )
    .unwrap();
    let unknown = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(!unknown.passed);
    let diagnostic = unknown
        .diagnostics
        .iter()
        .find(|item| item.message == "unknown tactic")
        .unwrap();
    let location = diagnostic.draft_location.as_ref().unwrap();
    assert_eq!(location.path, proof);
    assert_eq!((location.line, location.column), (4, 3));
    assert_eq!(location.source_excerpt.as_deref(), Some("  unknown_step"));
    assert!(diagnostic.checked_location.as_ref().unwrap().line > location.line);
    assert_eq!(std::fs::read(&model).unwrap(), before_show);
    std::fs::write(&proof, "exact (by rfl : )\n").unwrap();
    let syntax_error = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096)
        .unwrap_err()
        .to_string();
    let proof_line = std::str::from_utf8(&before_show)
        .unwrap()
        .lines()
        .position(|line| line.contains("fr:proof-begin"))
        .unwrap()
        + 2;
    assert!(
        syntax_error.contains(&format!("specs/FrSpecs/SrcLibRsKeep.lean:{proof_line}:")),
        "{syntax_error}"
    );
    assert!(syntax_error.ends_with("; no files changed."));
    assert!(syntax_error.contains(&format!("{}:1:", proof.display())));
    assert_eq!(std::fs::read(&model).unwrap(), before_show);
    let manually_edited = std::str::from_utf8(&before_show).unwrap().replace(
        "  -- fr:debt keepModel_identity\n  sorry\n",
        "  exact false\n",
    );
    assert_ne!(manually_edited.as_bytes(), before_show);
    std::fs::write(&model, &manually_edited).unwrap();
    assert!(fun_refactor::spec::formal_goals(
        workspace.path(),
        std::slice::from_ref(&model),
        None,
        8,
        4096,
        true,
    )
    .unwrap()
    .catalog
    .is_empty());
    let edited_task = fun_refactor::spec::proof_task(workspace.path(), target, 4096, true).unwrap();
    assert_eq!(edited_task.goal.target, target);
    std::fs::write(&proof, "exact false\n").unwrap();
    let rejected = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(!rejected.passed);
    assert!(rejected.receipt.is_none());
    assert!(!rejected.diagnostics.is_empty());
    assert_eq!(std::fs::read_to_string(&model).unwrap(), manually_edited);
    std::fs::write(&proof, "rfl\n").unwrap();
    let repaired = fun_refactor::spec::proof_check(workspace.path(), target, &proof, 4096).unwrap();
    assert!(
        repaired.passed,
        "{}",
        serde_json::to_string(&repaired).unwrap()
    );
    assert_eq!(std::fs::read_to_string(&model).unwrap(), manually_edited);
    let saved = run(
        workspace.path(),
        &["spec", "prove", target, "--from", "proof.lean", "--write"],
    );
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let saved_model = std::fs::read_to_string(&model).unwrap();
    assert!(saved_model.contains("  rfl\n"));
    assert!(!saved_model.contains("  exact false\n"));
    assert!(saved_model.contains("-- fr:proof-begin keepModel_identity"));
    assert!(saved_model.contains("-- fr:proof-end keepModel_identity"));
    std::fs::write(&model, &before_show).unwrap();
    std::fs::write(workspace.path().join("proof.lean"), "rfl\n").unwrap();
    std::fs::write(&dependency, policy.replace(":= value", ":= !value")).unwrap();
    let second_task = fun_refactor::spec::proof_task(workspace.path(), target, 4096, true).unwrap();
    assert_ne!(first_task.goal.id, second_task.goal.id);
    let false_claim = run(
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
        false_claim.status.success(),
        "{}",
        String::from_utf8_lossy(&false_claim.stderr)
    );
    let false_claim: serde_json::Value = serde_json::from_slice(&false_claim.stdout).unwrap();
    assert_eq!(false_claim["passed"], false);
    assert!(false_claim["receipt"].is_null());
    assert!(!false_claim["diagnostics"].as_array().unwrap().is_empty());
    std::fs::write(&dependency, format!("-- reviewed dependency\n{policy}")).unwrap();
    let renewed = run(
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
        renewed.status.success(),
        "{}",
        String::from_utf8_lossy(&renewed.stderr)
    );
    let renewed: serde_json::Value = serde_json::from_slice(&renewed.stdout).unwrap();
    assert_eq!(renewed["passed"], true);
    assert_ne!(first["receipt"], renewed["receipt"]);
    std::fs::remove_file(&dependency).unwrap();
    let missing = run(
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
        missing.status.success(),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    let missing: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(missing["passed"], false);
    assert!(missing["receipt"].is_null());
}

#[test]
fn guarded_page_count_keeps_early_returns_and_truncating_arithmetic() {
    use fun_refactor::formal_kernel::{evaluate, Value};
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("src")).unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(
        workspace.path().join("src/pages.rs"),
        include_str!("agent-eval/checked-translation/pages.rs"),
    )
    .unwrap();
    let plan =
        fun_refactor::spec::formal_plan(workspace.path(), "src/pages.rs::page_count", &[]).unwrap();
    let term = &plan.kernel.evaluation.as_ref().unwrap().term;
    for (items, size, expected) in [
        (-1, 5, -1),
        (11, 0, -1),
        (11, -5, -1),
        (0, 5, 0),
        (11, 5, 3),
        (10, 5, 2),
        (i64::MAX, 1, i64::MAX),
        (i64::MAX, 2, i64::MAX / 2 + 1),
        (i64::MAX, i64::MAX, 1),
        (1, i64::MAX, 1),
        (i64::MIN, 1, -1),
    ] {
        assert_eq!(
            evaluate(term, &[Value::Int(items), Value::Int(size)], 256),
            Ok(Value::Int(expected))
        );
    }
    assert!(plan.kernel.lean_definition.contains("Int.tdiv"));
    assert!(plan.kernel.lean_definition.contains("Int.tmod"));
    assert!(plan
        .assumptions
        .iter()
        .any(|item| item.contains("division by zero")));
    let init = run(workspace.path(), &["spec", "init", "specs", "--write"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    std::fs::write(
        workspace.path().join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let scaffold = run(
        workspace.path(),
        &[
            "spec",
            "scaffold",
            "--from",
            "plan.json",
            "--package",
            "specs",
            "--write",
        ],
    );
    assert!(
        scaffold.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let verify = run(workspace.path(), &["spec", "verify", "specs"]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn early_guard_bindings_stay_in_their_branch_and_fallthrough_refuses() {
    use fun_refactor::formal_kernel::{evaluate, Value};
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("guard.rs");
    std::fs::write(
        &path,
        "pub fn choose(n: i64) -> i64 { if n > 0 { let n = 3; return n; } n + 1 }\n",
    )
    .unwrap();
    let plan = fun_refactor::spec::formal_plan(workspace.path(), "guard.rs::choose", &[]).unwrap();
    let term = &plan.kernel.evaluation.as_ref().unwrap().term;
    assert_eq!(evaluate(term, &[Value::Int(17)], 256), Ok(Value::Int(3)));
    assert_eq!(evaluate(term, &[Value::Int(-1)], 256), Ok(Value::Int(0)));
    for source in [
        "pub fn choose(n: i64) -> i64 { if n > 0 { let m = n + 1; } n }",
        "pub fn choose(n: i64) -> i64 { if n > 0 { record(); return 1; } n }",
        "pub fn choose(n: i64) -> i64 { if n > 0 { let mut m = n; m = 3; return m; } n }",
    ] {
        std::fs::write(&path, source).unwrap();
        assert!(
            fun_refactor::spec::formal_plan(workspace.path(), "guard.rs::choose", &[]).is_err(),
            "{source}"
        );
    }
}

#[test]
fn signed_division_models_keep_all_signs_and_source_failures_visible() {
    use fun_refactor::formal_kernel::{evaluate, Failure, Value};
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("division.rs"), "pub fn quotient(a: i64, b: i64) -> i64 { a / b }\npub fn remainder(a: i64, b: i64) -> i64 { a % b }\n").unwrap();
    for (name, operation) in [
        ("quotient", i64::checked_div as fn(i64, i64) -> Option<i64>),
        ("remainder", i64::checked_rem),
    ] {
        let plan =
            fun_refactor::spec::formal_plan(workspace.path(), &format!("division.rs::{name}"), &[])
                .unwrap();
        let term = &plan.kernel.evaluation.as_ref().unwrap().term;
        for (a, b) in [(7, 3), (-7, 3), (7, -3), (-7, -3), (0, 3), (i64::MAX, 1)] {
            assert_eq!(
                evaluate(term, &[Value::Int(a), Value::Int(b)], 256),
                operation(a, b).map(Value::Int).ok_or(Failure::Overflow)
            );
        }
        assert_eq!(
            evaluate(term, &[Value::Int(7), Value::Int(0)], 256),
            Err(Failure::DivisionByZero)
        );
        assert_eq!(
            evaluate(term, &[Value::Int(i64::MIN), Value::Int(-1)], 256),
            Err(Failure::Overflow)
        );
    }
    std::fs::write(
        workspace.path().join("unsigned.rs"),
        "pub fn quotient(a: u64, b: u64) -> u64 { a / b }\n",
    )
    .unwrap();
    assert!(
        fun_refactor::spec::formal_plan(workspace.path(), "unsigned.rs::quotient", &[]).is_err()
    );
}

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
    for row in goals["catalog"].as_array().unwrap() {
        assert_eq!(
            row["target"],
            format!(
                "{}::{}",
                row["spec"].as_str().unwrap(),
                row["name"].as_str().unwrap()
            )
        );
    }
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
    assert_eq!(revealed["revealed"]["target"], target);
    let request = serde_json::json!({"schema":"fr-agent-goal-1","purpose":"prove",
        "selector":{"path":"specs/FrSpecs/SrcLibRsKeep.lean"},
        "operation":{"kind":"proof","obligation":obligation}});
    std::fs::write(
        workspace.path().join("goal.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let guided = run(
        workspace.path(),
        &["--json", "guide", "--from", "goal.json"],
    );
    assert!(
        guided.status.success(),
        "{}",
        String::from_utf8_lossy(&guided.stderr)
    );
    let guided: serde_json::Value = serde_json::from_slice(&guided.stdout).unwrap();
    assert_eq!(guided["state"], "ready", "{guided}");
    assert_eq!(guided["route"]["id"], "proof");
    let arguments = guided["actions"][0]["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    let followed = run(
        workspace.path(),
        &[&["--json"], arguments.as_slice()].concat(),
    );
    assert!(
        followed.status.success(),
        "{}",
        String::from_utf8_lossy(&followed.stderr)
    );
    let followed: serde_json::Value = serde_json::from_slice(&followed.stdout).unwrap();
    assert_eq!(followed["schema"], "fr-proof-task-1");
    assert_eq!(
        followed["object_digest"],
        guided["route"]["evidence"]["task_digest"]
    );
    let proof_task = run(workspace.path(), &["--json", "spec", "proof-task", &target]);
    assert!(
        proof_task.status.success(),
        "{}",
        String::from_utf8_lossy(&proof_task.stderr)
    );
    let proof_task: serde_json::Value = serde_json::from_slice(&proof_task.stdout).unwrap();
    assert_eq!(proof_task["schema"], "fr-proof-task-1");
    assert_eq!(proof_task["contract"]["author"], "agent");
    assert_eq!(
        proof_task["claim"]["parameters"],
        serde_json::json!(["(x : Bool)"])
    );
    assert_eq!(
        proof_task["claim"]["parts"][0]["assumptions"],
        serde_json::json!([])
    );
    assert_eq!(
        proof_task["claim"]["parts"][0]["conclusions"],
        serde_json::json!(["keepModel x = x"])
    );
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
