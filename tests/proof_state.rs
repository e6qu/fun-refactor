use std::path::{Path, PathBuf};
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn run(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(FR)
        .arg("-C")
        .arg(root)
        .arg("--json")
        .args(args)
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected_code = if args.starts_with(&["proof", "check"]) && value["passed"] == false {
        1
    } else {
        0
    };
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}\n{}",
        value,
        String::from_utf8_lossy(&output.stderr)
    );
    value
}

fn setup(root: &Path) -> (PathBuf, String) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn keep(value: bool) -> bool { value }\n",
    )
    .unwrap();
    run(root, &["spec", "init", "specs", "--write"]);
    let plan =
        fun_refactor::spec::formal_plan(root, "src/lib.rs::keep", &["identity".into()]).unwrap();
    std::fs::write(root.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
    run(
        root,
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
    (
        root.join("specs/FrSpecs/SrcLibRsKeep.lean"),
        "specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity".into(),
    )
}

fn inspect(root: &Path, target: &str, draft: &str, line: usize) -> serde_json::Value {
    let text = std::fs::read_to_string(root.join(draft)).unwrap();
    let column = text.lines().nth(line - 1).unwrap().chars().count() + 1;
    run(
        root,
        &[
            "proof",
            "state",
            target,
            "--draft",
            draft,
            "--at",
            &format!("{line}:{column}"),
            "--bytes",
            "8192",
            "--timeout-seconds",
            "30",
        ],
    )
}

#[test]
fn live_cursor_states_follow_branches_and_full_check_remains_required() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project with spaces");
    let (module, target) = setup(&root);
    let original = std::fs::read(&module).unwrap();
    let draft = "have pair : keepModel x = x ∧ keepModel x = x := by\n  constructor\n  · rfl\n  · skip\nexact pair.1\n";
    std::fs::write(root.join("draft.tactics"), draft).unwrap();
    let task = run(&root, &["proof", "show", &target, "--bytes", "4096"]);
    assert_eq!(task["actions"]["inspect"][1], "state");

    let split = inspect(&root, &target, "draft.tactics", 2);
    assert_eq!(split["operation"], "proof-state");
    assert_eq!(split["status"], "goals");
    assert_eq!(split["goals"].as_array().unwrap().len(), 2);
    assert!(split["goals"][0]["context"]
        .as_str()
        .unwrap()
        .contains("x : Bool"));
    assert_eq!(split["goal_id"], task["goal"]["id"]);
    assert_eq!(split["draft_location"]["line"], 2);
    assert_eq!(split["acceptance"], "not-checked");
    assert!(
        serde_json::to_vec_pretty(&split).unwrap().len()
            <= split["token_budget"]["limit"].as_u64().unwrap() as usize
    );

    let closed_branch = inspect(&root, &target, "draft.tactics", 3);
    assert_eq!(closed_branch["status"], "closed-at-position");
    assert_eq!(closed_branch["acceptance"], "not-checked");
    let right_branch = inspect(&root, &target, "draft.tactics", 4);
    assert_eq!(right_branch["status"], "goals");
    assert_eq!(right_branch["goals"].as_array().unwrap().len(), 1);
    assert!(right_branch["goals"][0]["context"]
        .as_str()
        .unwrap()
        .contains("right"));
    let rejected = run(
        &root,
        &[
            "proof",
            "check",
            &target,
            "--draft",
            "draft.tactics",
            "--bytes",
            "4096",
            "--timeout-seconds",
            "30",
        ],
    );
    assert_eq!(rejected["passed"], false);
    assert!(rejected["receipt"].is_null());
    assert_eq!(rejected["actions"]["inspect"][1], "state");
    assert_eq!(std::fs::read(&module).unwrap(), original);

    std::fs::write(root.join("draft.tactics"), draft.replace("skip", "rfl")).unwrap();
    let closed = inspect(&root, &target, "draft.tactics", 5);
    assert_eq!(closed["status"], "closed-at-position");
    assert_eq!(closed["acceptance"], "not-checked");
    assert_ne!(closed["proof_digest"], split["proof_digest"]);
    let accepted = run(
        &root,
        &[
            "proof",
            "check",
            &target,
            "--draft",
            "draft.tactics",
            "--bytes",
            "4096",
            "--timeout-seconds",
            "30",
        ],
    );
    assert_eq!(accepted["passed"], true);
    assert!(accepted["receipt"].is_string());
    assert_eq!(std::fs::read(&module).unwrap(), original);

    let mut context = String::new();
    for index in 0..12 {
        context.push_str(&format!("have fact{index} : True := True.intro\n"));
    }
    context.push_str("skip\n");
    std::fs::write(root.join("context.tactics"), &context).unwrap();
    let state = inspect(&root, &target, "context.tactics", 13);
    let goal = state["goals"][0]["context"].as_str().unwrap();
    assert!(goal.contains("fact0") && goal.contains("fact11"), "{goal}");
    let budget = fun_refactor::spec::proof_state(
        &root,
        &target,
        &root.join("context.tactics"),
        (13, 5),
        1024,
        30,
    )
    .unwrap_err();
    assert!(
        budget.to_string().contains("complete proof state"),
        "{budget:#}"
    );
    assert!(fun_refactor::spec::proof_state(
        &root,
        &target,
        &root.join("context.tactics"),
        (13, 0),
        8192,
        30
    )
    .is_err());
    assert_eq!(std::fs::read(&module).unwrap(), original);

    let applied = run(
        &root,
        &[
            "proof",
            "save",
            &target,
            "--draft",
            "draft.tactics",
            "--bytes",
            "4096",
            "--timeout-seconds",
            "30",
        ],
    );
    assert_eq!(applied["applied"], true);
    let checked = run(
        &root,
        &["spec", "check", "specs", "--strict", "--max-debt", "0"],
    );
    assert_eq!(checked["obligations"], 0);
    assert!(checked["debts"].as_array().unwrap().is_empty());
}

#[test]
fn cursor_and_deadline_settings_are_explicit_cli_requirements() {
    let temporary = tempfile::tempdir().unwrap();
    for missing in ["--at", "--bytes", "--timeout-seconds"] {
        let mut arguments = vec![
            "proof",
            "state",
            "specs/Demo.lean::demo",
            "--draft",
            "draft",
            "--at",
            "1:1",
            "--bytes",
            "4096",
            "--timeout-seconds",
            "20",
        ];
        let index = arguments
            .iter()
            .position(|argument| *argument == missing)
            .unwrap();
        arguments.drain(index..index + 2);
        let output = Command::new(FR)
            .arg("-C")
            .arg(temporary.path())
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(missing));
    }
}
