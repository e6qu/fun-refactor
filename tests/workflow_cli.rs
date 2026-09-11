use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn fr(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fixture() -> (tempfile::TempDir, Value, String) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("app.py"), "def before():\n    return 1\n").unwrap();
    fs::create_dir(root.path().join(".fr")).unwrap();
    fs::write(
        root.path().join(".fr/checks.json"),
        serde_json::to_vec(&json!({
            "schema": 1,
            "checks": [{
                "name": "syntax",
                "argv": ["python3", "-c", "from pathlib import Path; compile(open('app.py').read(), 'app.py', 'exec'); raise SystemExit(7 if Path('.fail-workflow').exists() else 0)"],
                "cwd": ".",
                "timeout_seconds": 10,
                "covers": ["Python syntax"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let checks = report(fr(root.path(), &["checks"]), 0);
    let mut plan = report(
        fr(root.path(), &["rename", "before", "after", "--save-plan"]),
        0,
    );
    let transaction = plan["transaction"].as_u64().unwrap().to_string();
    let shown = report(fr(root.path(), &["history", "show", &transaction]), 0);
    plan["transaction_context_basis"] = shown["records"][0]["context_basis"].clone();
    let basis = checks["basis"].as_str().unwrap().to_owned();
    (root, plan, basis)
}

fn write_manifest(
    root: &Path,
    plan: &Value,
    check_basis: &str,
    exercise_reversal: bool,
    patch: Option<&str>,
) {
    let mut manifest = manifest(plan, check_basis);
    manifest["exercise-reversal"] = json!(exercise_reversal);
    if let Some(output) = patch {
        manifest["patch"] = json!({"output": output});
    }
    write_manifest_value(root, &manifest);
}

fn manifest(plan: &Value, check_basis: &str) -> Value {
    json!({
        "schema": 1,
        "transaction": plan["transaction"],
        "transaction-context-basis": plan["transaction_context_basis"],
        "checks": {"basis": check_basis, "names": ["syntax"]},
        "check-output-bytes": 256
    })
}

fn write_manifest_value(root: &Path, manifest: &Value) {
    fs::write(
        root.join(".fr-workflow"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn reviewed_workflow_applies_checks_reverses_redoes_and_delivers_patch() {
    let (root, plan, check_basis) = fixture();
    write_manifest(root.path(), &plan, &check_basis, true, Some("change.patch"));

    let preview = report(fr(root.path(), &["workflow", "--from", ".fr-workflow"]), 0);
    assert_eq!(preview["ready"], true);
    assert_eq!(preview["executed"], false);
    assert_eq!(preview["passed"], Value::Null);
    assert_eq!(preview["transaction_status"], "planned");
    assert_eq!(preview["stages"].as_array().unwrap().len(), 7);
    assert!(!root.path().join("change.patch").exists());
    assert!(fs::read_to_string(root.path().join("app.py"))
        .unwrap()
        .contains("before"));

    let completed = report(
        fr(
            root.path(),
            &["workflow", "--from", ".fr-workflow", "--write"],
        ),
        0,
    );
    assert_eq!(completed["passed"], true);
    assert_eq!(completed["transaction_status"], "applied");
    assert!(completed["stages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|stage| stage["status"] == "passed"));
    assert_eq!(completed["stages"][1]["result"]["passed"], true);
    assert_eq!(
        completed["stages"][3]["result"]["recorded_evidence"],
        Value::Null
    );
    assert_eq!(
        completed["stages"][5]["result"]["recorded_evidence"]["created"],
        false
    );
    assert!(fs::read_to_string(root.path().join("app.py"))
        .unwrap()
        .contains("after"));
    let patch = fs::read_to_string(root.path().join("change.patch")).unwrap();
    assert!(patch.contains("+def after():"));
    assert!(!completed.to_string().contains(&patch));
}

#[test]
fn failed_checks_stop_before_reversal_and_patch_delivery() {
    let (root, plan, check_basis) = fixture();
    fs::write(root.path().join(".fail-workflow"), "fail\n").unwrap();
    write_manifest(root.path(), &plan, &check_basis, true, Some("failed.patch"));

    let failed = report(
        fr(
            root.path(),
            &["workflow", "--from", ".fr-workflow", "--write"],
        ),
        1,
    );
    assert_eq!(failed["passed"], false);
    assert_eq!(failed["transaction_status"], "applied");
    assert_eq!(failed["stages"][0]["status"], "passed");
    assert_eq!(failed["stages"][1]["status"], "failed");
    assert_eq!(failed["stages"][2]["status"], "pending");
    assert!(!root.path().join("failed.patch").exists());
    assert!(fs::read_to_string(root.path().join("app.py"))
        .unwrap()
        .contains("after"));
}

#[test]
fn invalid_manifests_and_artifact_destinations_refuse_before_mutation() {
    for (name, mutate, message) in [
        (
            "schema",
            (|value: &mut Value| value["schema"] = json!(2)) as fn(&mut Value),
            "requires schema 1",
        ),
        (
            "unknown",
            (|value: &mut Value| value["surprise"] = json!(true)) as fn(&mut Value),
            "unknown field",
        ),
        (
            "transaction-basis",
            (|value: &mut Value| value["transaction-context-basis"] = json!("frtb2:stale"))
                as fn(&mut Value),
            "transaction context basis",
        ),
        (
            "check-basis",
            (|value: &mut Value| value["checks"]["basis"] = json!("0".repeat(32)))
                as fn(&mut Value),
            "configuration basis",
        ),
        (
            "empty-checks",
            (|value: &mut Value| value["checks"]["names"] = json!([])) as fn(&mut Value),
            "at least one declared check",
        ),
        (
            "absolute-patch",
            (|value: &mut Value| value["patch"] = json!({"output": "/tmp/change.patch"}))
                as fn(&mut Value),
            "relative normal path",
        ),
        (
            "history-patch",
            (|value: &mut Value| value["patch"] = json!({"output": ".fr-history/change.patch"}))
                as fn(&mut Value),
            "cannot enter",
        ),
        (
            "source-patch",
            (|value: &mut Value| value["patch"] = json!({"output": "app.py"})) as fn(&mut Value),
            "already exists",
        ),
    ] {
        let (root, plan, basis) = fixture();
        let mut value = manifest(&plan, &basis);
        mutate(&mut value);
        write_manifest_value(root.path(), &value);
        let failed = report(fr(root.path(), &["workflow", "--from", ".fr-workflow"]), 1);
        assert!(
            failed["error"]["message"]
                .as_str()
                .unwrap()
                .contains(message),
            "{name}: {failed}"
        );
        assert!(fs::read_to_string(root.path().join("app.py"))
            .unwrap()
            .contains("before"));
        let history = report(fr(root.path(), &["history", "show", "1"]), 0);
        assert_eq!(history["records"][0]["status"], "planned", "{name}");
    }
}

#[test]
fn source_drift_refuses_and_preserves_the_drift_for_review() {
    let (root, plan, basis) = fixture();
    write_manifest(root.path(), &plan, &basis, false, Some("change.patch"));
    fs::write(
        root.path().join("app.py"),
        "def user_edit():\n    return 2\n",
    )
    .unwrap();
    let failed = report(fr(root.path(), &["workflow", "--from", ".fr-workflow"]), 1);
    assert!(failed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("source changed after planning"));
    assert_eq!(
        fs::read_to_string(root.path().join("app.py")).unwrap(),
        "def user_edit():\n    return 2\n"
    );
    assert!(!root.path().join("change.patch").exists());
}
