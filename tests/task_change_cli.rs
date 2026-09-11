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

fn fixture(check: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join(".fr")).unwrap();
    fs::create_dir(root.path().join("artifacts")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\n",
    )
    .unwrap();
    fs::write(
        root.path().join(".fr/replacement.fragment"),
        "{ value.to_uppercase() }\n",
    )
    .unwrap();
    fs::write(
        root.path().join(".fr/checks.json"),
        serde_json::to_vec(&json!({
            "schema": 1,
            "checks": [{
                "name": "syntax",
                "argv": [check],
                "cwd": ".",
                "timeout_seconds": 10,
                "covers": ["selected source state"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join(".fr/task-change.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "fr-task-change-1",
            "requests": [{
                "id": "target",
                "arguments": ["find", "render", "--signature", "--source", "--bytes", "2048"]
            }],
            "targets": [{
                "id": "render-body",
                "handle": {"request": "target", "pointer": "/rows/0/0"},
                "op": "replace-body",
                "from": ".fr/replacement.fragment"
            }],
            "postconditions": {
                "files-changed": 1,
                "edits": 1,
                "changed-operations": 1,
                "paths-changed": ["src/lib.rs"]
            },
            "checks": ["syntax"],
            "delivery": {
                "exercise-reversal": true,
                "patch": "artifacts/change.patch",
                "check-output-bytes": 256
            }
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

fn preview(root: &Path) -> Value {
    report(
        fr(root, &["task-change", "--from", ".fr/task-change.json"]),
        0,
    )
}

#[test]
fn reviewed_task_change_previews_then_executes_the_checked_lifecycle() {
    let root = fixture("true");
    let preview = preview(root.path());
    assert_eq!(preview["schema"], "fr-task-change-1");
    assert_eq!(preview["ready"], true);
    assert_eq!(preview["executed"], false);
    assert_eq!(preview["passed"], Value::Null);
    assert_eq!(preview["author"]["postconditions_held"], true);
    assert!(preview["author"]["diff"]
        .as_str()
        .unwrap()
        .contains("value.to_uppercase"));
    assert_eq!(preview["stages"].as_array().unwrap().len(), 7);
    assert!(!root.path().join(".fr-history").exists());
    assert!(!root.path().join("artifacts/change.patch").exists());

    let completed = report(
        fr(
            root.path(),
            &[
                "task-change",
                "--from",
                ".fr/task-change.json",
                "--write",
                "--basis",
                preview["task_change_basis"].as_str().unwrap(),
            ],
        ),
        0,
    );
    assert_eq!(completed["passed"], true);
    assert_eq!(completed["workflow"]["transaction_status"], "applied");
    assert!(completed["workflow"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|stage| stage["status"] == "passed"));
    assert!(fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("value.to_uppercase"));
    let patch = fs::read_to_string(root.path().join("artifacts/change.patch")).unwrap();
    assert!(patch.contains("+pub fn render"));

    let shown = report(fr(root.path(), &["history", "show", "1"]), 0);
    assert_eq!(
        shown["records"][0]["required_checks"]["checks"],
        json!(["syntax"])
    );
    assert_eq!(
        shown["records"][0]["check_evidence"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn reviewed_task_change_accepts_and_binds_a_source_free_semantic_body() {
    let root = fixture("true");
    let semantic = json!({
        "schema": "fr-semantic-body-1",
        "body": [{
            "kind": "return",
            "value": {
                "kind": "call",
                "value": {
                    "callee": {
                        "kind": "field",
                        "value": {"of":{"kind":"name","value":"value"},"name":"to_uppercase"}
                    },
                    "args": []
                }
            }
        }]
    });
    fs::write(
        root.path().join(".fr/replacement.fragment"),
        serde_json::to_vec_pretty(&semantic).unwrap(),
    )
    .unwrap();
    let manifest_path = root.path().join(".fr/task-change.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["targets"][0]["op"] = json!("replace-body-semantic");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let preview = preview(root.path());
    assert_eq!(preview["ready"], true);
    assert_eq!(
        preview["author"]["steps"][0]["operation"],
        "replace-body-semantic"
    );
    assert_eq!(
        preview["author"]["steps"][0]["semantic_input"]["source_free"],
        true
    );
    assert_eq!(
        preview["author"]["steps"][0]["semantic_render"]["fidelity"]["carried_verbatim"],
        0
    );
    let completed = report(
        fr(
            root.path(),
            &[
                "task-change",
                "--from",
                ".fr/task-change.json",
                "--write",
                "--basis",
                preview["task_change_basis"].as_str().unwrap(),
            ],
        ),
        0,
    );
    assert_eq!(completed["passed"], true);
    assert!(fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("value.to_uppercase"));
}

#[test]
fn changed_fragments_refuse_the_reviewed_write_before_history() {
    let root = fixture("true");
    let preview = preview(root.path());
    fs::write(
        root.path().join(".fr/replacement.fragment"),
        "{ value.repeat(2) }\n",
    )
    .unwrap();
    let failed = report(
        fr(
            root.path(),
            &[
                "task-change",
                "--from",
                ".fr/task-change.json",
                "--write",
                "--basis",
                preview["task_change_basis"].as_str().unwrap(),
            ],
        ),
        1,
    );
    assert!(failed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("task-change basis"));
    assert!(!root.path().join(".fr-history").exists());
    assert!(!root.path().join("artifacts/change.patch").exists());
}

#[test]
fn failed_checks_leave_structured_state_and_withhold_the_patch() {
    let root = fixture("false");
    let preview = preview(root.path());
    let failed = report(
        fr(
            root.path(),
            &[
                "task-change",
                "--from",
                ".fr/task-change.json",
                "--write",
                "--basis",
                preview["task_change_basis"].as_str().unwrap(),
            ],
        ),
        1,
    );
    assert_eq!(failed["passed"], false);
    assert_eq!(failed["workflow"]["transaction_status"], "applied");
    assert_eq!(failed["workflow"]["stages"][1]["status"], "failed");
    assert!(!root.path().join("artifacts/change.patch").exists());
}

#[test]
fn reviewed_source_manifest_checks_and_destination_drift_refuse_before_history() {
    for fault in ["source", "manifest", "checks", "destination"] {
        let root = fixture("true");
        let preview = preview(root.path());
        match fault {
            "source" => fs::write(
                root.path().join("src/lib.rs"),
                "pub fn render(value: &str) -> String { value.repeat(2) }\n",
            )
            .unwrap(),
            "manifest" => {
                let path = root.path().join(".fr/task-change.json");
                let mut manifest: Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                manifest["delivery"]["exercise-reversal"] = json!(false);
                fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            }
            "checks" => {
                let path = root.path().join(".fr/checks.json");
                let mut checks: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                checks["checks"][0]["covers"] = json!(["changed declaration"]);
                fs::write(path, serde_json::to_vec(&checks).unwrap()).unwrap();
            }
            "destination" => {
                fs::write(root.path().join("artifacts/change.patch"), "occupied\n").unwrap()
            }
            _ => unreachable!(),
        }
        let failed = report(
            fr(
                root.path(),
                &[
                    "task-change",
                    "--from",
                    ".fr/task-change.json",
                    "--write",
                    "--basis",
                    preview["task_change_basis"].as_str().unwrap(),
                ],
            ),
            1,
        );
        let message = failed["error"]["message"].as_str().unwrap();
        if fault == "destination" {
            assert!(message.contains("already exists"), "{failed}");
        } else {
            assert!(message.contains("task-change basis"), "{failed}");
        }
        assert!(!root.path().join(".fr-history").exists(), "{fault}");
    }
}

#[test]
fn incomplete_diffs_bad_fragments_and_invalid_fragment_choices_never_create_history() {
    for fault in ["diff", "syntax", "missing-fragment"] {
        let root = fixture("true");
        let mut args = vec!["task-change", "--from", ".fr/task-change.json"];
        match fault {
            "diff" => args.extend(["--diff-bytes", "1"]),
            "syntax" => fs::write(
                root.path().join(".fr/replacement.fragment"),
                "{ let invalid = ; }\n",
            )
            .unwrap(),
            "missing-fragment" => {
                let path = root.path().join(".fr/task-change.json");
                let mut manifest: Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                manifest["targets"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("from");
                fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let failed = report(fr(root.path(), &args), 1);
        let message = failed["error"]["message"].as_str().unwrap();
        match fault {
            "diff" => assert!(message.contains("complete untruncated diff")),
            "syntax" => assert!(message.contains("batch operation 1 failed")),
            "missing-fragment" => assert!(message.contains("invalid fragment choice")),
            _ => unreachable!(),
        }
        assert!(!root.path().join(".fr-history").exists(), "{fault}");
        assert!(!root.path().join("artifacts/change.patch").exists());
    }
}
