mod common;

use fun_refactor::project::object_merkle;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(root: &Path, arguments: &[&str], input: Option<&[u8]>) -> (bool, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(arguments);
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{arguments:?}: {error}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value)
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::create_dir_all(root.path().join(".fr")).unwrap();
    std::fs::create_dir_all(root.path().join("artifacts")).unwrap();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\n\
         pub fn caller() -> String { render(\"ok\") }\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join(".fr/checks.json"),
        serde_json::to_vec(&json!({
            "schema": 1,
            "checks": [{
                "name": "syntax",
                "argv": ["true"],
                "cwd": ".",
                "timeout_seconds": 10,
                "covers": ["selected source state"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

fn handle(root: &Path) -> String {
    let (success, report) = run(root, &["project", "find", "render", "--signature"], None);
    assert!(success, "{report}");
    report["rows"][0][0].as_str().unwrap().to_owned()
}

fn intent(target: &str, purpose: &str, needs: Value, packet_limit: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": "fr-agent-intent-1",
        "target": target,
        "purpose": purpose,
        "needs": needs,
        "token_limit": 4096,
        "call_limit": 192,
        "packet_limit": packet_limit
    }))
    .unwrap()
}

fn change_intent(target: &str, purpose: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": "fr-agent-intent-1",
        "target": target,
        "purpose": purpose,
        "needs": [{"name": "map", "section": "code_map", "pointer": "/target"}],
        "token_limit": 4096,
        "call_limit": 192,
        "packet_limit": 65_536,
        "action": {
            "task_change": {
                "schema": "fr-task-change-1",
                "requests": [],
                "targets": [{
                    "id": "render-body",
                    "handle": target,
                    "op": "replace-body",
                    "fragment": "{ value.to_uppercase() }\n"
                }],
                "postconditions": {
                    "files-changed": 1,
                    "edits": 1,
                    "changed-operations": 1,
                    "paths-changed": ["src/lib.rs"]
                },
                "checks": ["syntax"],
                "delivery": {
                    "check-original": true,
                    "compact-success": true,
                    "exercise-reversal": true,
                    "patch": "artifacts/change.patch",
                    "check-output-bytes": 256
                }
            },
            "diff_bytes": 4096,
            "report_bytes": 65_536
        }
    }))
    .unwrap()
}

#[test]
fn native_intent_compiles_selected_evidence_in_one_snapshot() {
    let root = fixture();
    let handle = handle(root.path());
    let input = intent(
        &handle,
        "trace",
        json!([
            {"name": "entry", "section": "code_map", "pointer": "/target"},
            {"name": "calls", "section": "call_traces", "pointer": ""},
            {"name": "flows", "section": "sources_and_sinks", "pointer": ""}
        ]),
        65_536,
    );
    let (success, report) = run(root.path(), &["intent", "--from", "-"], Some(&input));
    assert!(success, "{report}");
    assert_eq!(report["schema"], "fr-agent-context-1");
    assert_eq!(report["query"], "intent");
    assert_eq!(report["intent"]["purpose"], "trace");
    assert_eq!(report["target"]["handle"], handle);
    assert_eq!(report["calls"], 0);
    assert_eq!(report["execution"]["engine"], "native");
    assert_eq!(report["execution"]["project_snapshots"], 1);
    assert_eq!(report["execution"]["progressive_disclosure_calls"], 0);
    assert_eq!(report["selected"]["entry"]["name"], "render");
    assert_eq!(report["selected"]["calls"]["status"], "returned");
    assert_eq!(report["selected"]["flows"]["status"], "returned");
    for name in ["entry", "calls", "flows"] {
        assert_eq!(
            report["object_digests"][name],
            object_merkle(&report["selected"][name]).unwrap()
        );
    }
    assert!(report["context_basis"]
        .as_str()
        .unwrap()
        .starts_with("frcb1:"));
    assert!(report["view_basis"].as_str().unwrap().starts_with("frdv1:"));
    assert!(report["intent"]["basis"]
        .as_str()
        .unwrap()
        .starts_with("frai1:"));
    assert_eq!(
        report["serialized_bytes"].as_u64().unwrap() as usize,
        serde_json::to_vec(&report).unwrap().len()
    );
    assert!(report["serialized_bytes"].as_u64().unwrap() <= 65_536);
    assert!(!report.to_string().contains("value.to_owned"));
}

#[test]
fn native_intent_refuses_cross_purpose_absent_stale_and_oversized_requests() {
    let root = fixture();
    let handle = handle(root.path());
    let cases = [
        intent(
            &handle,
            "understand",
            json!([{"name": "impact", "section": "impact", "pointer": ""}]),
            65_536,
        ),
        intent(
            &handle,
            "understand",
            json!([{"name": "missing", "section": "code_map", "pointer": "/absent"}]),
            65_536,
        ),
        intent(
            &format!("{}0", handle),
            "understand",
            json!([{"name": "map", "section": "code_map", "pointer": ""}]),
            65_536,
        ),
        intent(
            &handle,
            "trace",
            json!([
                {"name": "map", "section": "code_map", "pointer": ""},
                {"name": "calls", "section": "call_traces", "pointer": ""},
                {"name": "flows", "section": "sources_and_sinks", "pointer": ""}
            ]),
            1_024,
        ),
    ];
    for input in cases {
        let (success, report) = run(root.path(), &["intent", "--from", "-"], Some(&input));
        assert!(!success, "{report}");
        assert!(report["error"]["message"].is_string(), "{report}");
    }
}

#[test]
fn native_intent_rejects_unknown_fields_and_noncanonical_pointers() {
    let root = fixture();
    let handle = handle(root.path());
    let unknown = json!({
        "schema": "fr-agent-intent-1", "target": handle, "purpose": "understand",
        "needs": [{"name": "map", "section": "code_map", "pointer": "", "extra": true}],
        "token_limit": 4096, "call_limit": 1, "packet_limit": 4096
    });
    let malformed = intent(
        &handle,
        "understand",
        json!([{"name": "map", "section": "code_map", "pointer": "/bad~2"}]),
        4096,
    );
    for input in [serde_json::to_vec(&unknown).unwrap(), malformed] {
        let (success, report) = run(root.path(), &["intent", "--from", "-"], Some(&input));
        assert!(!success, "{report}");
    }
}

#[test]
fn change_intent_previews_and_executes_one_bound_reviewed_lifecycle() {
    let root = fixture();
    let handle = handle(root.path());
    let input = change_intent(&handle, "change");
    let (success, preview) = run(root.path(), &["intent", "--from", "-"], Some(&input));
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    assert!(basis.starts_with("fraa1:"));
    assert_eq!(preview["action"]["schema"], "fr-agent-action-1");
    assert_eq!(preview["action"]["review"]["ready"], true);
    assert_eq!(preview["action"]["review"]["executed"], false);
    assert!(preview["action"]["review"]["author"]["diff"]
        .as_str()
        .unwrap()
        .contains("to_uppercase"));
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_owned"));
    assert!(!root.path().join(".fr-history").exists());

    let (success, applied) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&input),
    );
    assert!(success, "{applied}");
    assert_eq!(applied["schema"], "fr-agent-action-result-1");
    assert_eq!(applied["action_basis"], basis);
    assert_eq!(applied["executed"], true);
    assert_eq!(applied["passed"], true);
    assert_eq!(applied["action"]["executed"], true);
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_uppercase"));
    assert!(root.path().join("artifacts/change.patch").is_file());

    let transaction = applied["action"]["transaction"]
        .as_u64()
        .unwrap()
        .to_string();
    let (success, undone) = run(
        root.path(),
        &["history", "undo", &transaction, "--write"],
        None,
    );
    assert!(success, "{undone}");
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_owned"));
    let (success, redone) = run(
        root.path(),
        &["history", "redo", &transaction, "--write"],
        None,
    );
    assert!(success, "{redone}");
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_uppercase"));
}

#[test]
fn change_intent_refuses_wrong_purpose_target_basis_and_unreviewed_execution() {
    let root = fixture();
    let handle = handle(root.path());
    let mut cases = vec![change_intent(&handle, "trace")];
    let mut wrong_target: Value =
        serde_json::from_slice(&change_intent(&handle, "change")).unwrap();
    wrong_target["action"]["task_change"]["targets"][0]["handle"] = json!(format!("{handle}0"));
    cases.push(serde_json::to_vec(&wrong_target).unwrap());
    for input in cases {
        let (success, refused) = run(root.path(), &["intent", "--from", "-"], Some(&input));
        assert!(!success, "{refused}");
    }

    let input = change_intent(&handle, "change");
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", "fraa1:wrong"],
        Some(&input),
    );
    assert!(!success, "{refused}");
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_owned"));
    assert!(!root.path().join(".fr-history").exists());

    let plain = intent(
        &handle,
        "change",
        json!([{"name": "map", "section": "code_map", "pointer": ""}]),
        65_536,
    );
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", "fraa1:wrong"],
        Some(&plain),
    );
    assert!(!success, "{refused}");
}
