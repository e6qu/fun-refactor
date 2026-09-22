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
    assert_eq!(
        report["target"]["location"]["name"]["span"],
        json!({"start": 7, "end": 13})
    );
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
    assert!(preview["action"]["review"].get("revision").is_none());
    assert!(preview["action"]["review"].get("coverage").is_none());
    assert_eq!(
        preview["action"]["review"]["intent_context_inherited"],
        json!(["revision", "coverage"])
    );
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

fn tagged(target: &str, purpose: &str, operation: Value) -> Value {
    json!({"schema":"fr-agent-intent-1", "target":target, "purpose":purpose,
        "needs":[{"name":"map", "section":"code_map", "pointer":"/target"}],
        "token_limit":4096,"call_limit":192,"packet_limit":65536,
        "action":{"schema":"fr-intent-action-2", "operation":operation,
                  "diff_bytes":65536,"report_bytes":65536}})
}
fn preview_value(root: &Path, input: &Value) -> (bool, Value) {
    run(
        root,
        &["intent", "--from", "-"],
        Some(&serde_json::to_vec(input).unwrap()),
    )
}
fn delivery() -> Value {
    json!({"check-original":true,"exercise-reversal":true,"compact-success":true,
        "patch":"artifacts/tagged.patch","check-output-bytes":256})
}

#[test]
fn tagged_task_resolves_multiple_targets_and_binds_secondary_evidence() {
    let root = fixture();
    let anchor = handle(root.path());
    let mut legacy: Value = serde_json::from_slice(&change_intent(&anchor, "change")).unwrap();
    let task = &mut legacy["action"]["task_change"];
    task["requests"] = json!([{"id":"caller","arguments":["find","caller","--signature"]}]);
    task["targets"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"caller-body",
        "handle":{"request":"caller","pointer":"/rows/0/0"},"op":"replace-body",
        "fragment":"{ render(\"changed\") }\n"}));
    task["postconditions"]["edits"] = json!(2);
    task["postconditions"]["changed-operations"] = json!(2);
    let input = tagged(
        &anchor,
        "change",
        json!({"kind":"task-change","task_change":task}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(preview["execution"]["project_snapshots"], 1);
    assert_eq!(
        preview["action"]["review"]["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let secondary = &preview["additional_evidence"][0];
    assert_eq!(secondary["target"]["name"], "caller");
    assert_eq!(
        secondary["object_digests"]["map"],
        object_merkle(&secondary["selected"]["map"]).unwrap()
    );
    assert!(!root.path().join(".fr-history").exists());
    let basis = preview["action"]["basis"].as_str().unwrap();
    let mut undersized = input.clone();
    undersized["packet_limit"] = json!(1024);
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&undersized).unwrap()),
    );
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert_eq!(result["schema"], "fr-agent-action-result-2");
    assert_eq!(result["passed"], true);
    assert_eq!(result["claims"]["implementation_correspondence"], false);
    assert!(root.path().join("artifacts/change.patch").exists());
    let source = std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
    assert!(source.contains("to_uppercase") && source.contains("changed"));
    assert_eq!(result["workflow"]["stages"].as_array().unwrap().len(), 8);
}

#[test]
fn tagged_actions_refuse_changed_secondary_target_checks_delivery_and_purpose() {
    let root = fixture();
    let anchor = handle(root.path());
    let legacy: Value = serde_json::from_slice(&change_intent(&anchor, "change")).unwrap();
    let input = tagged(
        &anchor,
        "change",
        json!({"kind":"task-change","task_change":legacy["action"]["task_change"]}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    for field in ["fragment", "checks", "delivery", "purpose"] {
        let mut changed = input.clone();
        match field {
            "fragment" => {
                changed["action"]["operation"]["task_change"]["targets"][0]["fragment"] =
                    json!("{ value.to_lowercase() }\n")
            }
            "checks" => changed["action"]["operation"]["task_change"]["checks"] = json!([]),
            "delivery" => {
                changed["action"]["operation"]["task_change"]["delivery"]["patch"] =
                    json!("artifacts/other.patch")
            }
            _ => changed["purpose"] = json!("prove"),
        }
        let (success, refused) = run(
            root.path(),
            &["intent", "--from", "-", "--write", "--basis", basis],
            Some(&serde_json::to_vec(&changed).unwrap()),
        );
        assert!(!success, "{field}: {refused}");
        assert!(!root.path().join(".fr-history").exists());
    }
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.into() }\n",
    )
    .unwrap();
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn tagged_author_batch_matches_the_standalone_planner_and_delivers() {
    let root = fixture();
    let anchor = handle(root.path());
    std::fs::write(
        root.path().join(".fr/body.rs"),
        "{ value.to_uppercase() }\n",
    )
    .unwrap();
    let batch = json!({"operations":[{"op":"replace-body","handle":anchor,
        "from":".fr/body.rs"}],
        "postconditions":{"files-changed":1,"edits":1,"changed-operations":1}});
    std::fs::write(
        root.path().join(".fr/batch.json"),
        serde_json::to_vec(&batch).unwrap(),
    )
    .unwrap();
    let (success, standalone) = run(
        root.path(),
        &[
            "author",
            "batch",
            "--from",
            ".fr/batch.json",
            "--diff-bytes",
            "65536",
        ],
        None,
    );
    assert!(success, "{standalone}");
    let input = tagged(
        &anchor,
        "change",
        json!({"kind":"author-batch","author_batch":batch,
        "checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(
        preview["action"]["review"]["plan"]["operations"],
        standalone["operations"]
    );
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, applied) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{applied}");
    assert!(root.path().join("artifacts/tagged.patch").exists());
}

#[test]
fn tagged_recipe_requires_anchored_selector_and_matches_standalone_plan() {
    let root = fixture();
    let recipe = "schema 1\nrecipe rename-render {\n rename to \"display\" where name=\"render\" in=\"src/lib.rs\"\n expect matched = 1\n expect refusals = 0\n}\n";
    std::fs::write(root.path().join("rename.fr"), recipe).unwrap();
    let anchor = handle(root.path());
    let input = tagged(
        &anchor,
        "change",
        json!({"kind":"recipe","recipe":recipe,
        "checks":["syntax"],"delivery":delivery()}),
    );
    let (success, standalone) = run(root.path(), &["recipe", "rename.fr"], None);
    assert!(success, "{standalone}");
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(preview["action"]["review"]["plan"], standalone);
    let mut broad = input.clone();
    broad["action"]["operation"]["recipe"] = json!(recipe.replace(" in=\"src/lib.rs\"", ""));
    let (success, refused) = preview_value(root.path(), &broad);
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("display(\"ok\")"));
}

#[test]
fn tagged_formal_plan_is_source_free_read_only_and_matches_standalone() {
    let root = fixture();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: bool) -> bool { value }\n",
    )
    .unwrap();
    let anchor = handle(root.path());
    let input = tagged(
        &anchor,
        "prove",
        json!({"kind":"formal-plan","properties":["identity"]}),
    );
    let (success, standalone) = run(
        root.path(),
        &[
            "spec",
            "plan",
            "src/lib.rs::render",
            "--property",
            "identity",
        ],
        None,
    );
    assert!(success, "{standalone}");
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(preview["action"]["review"]["plan"], standalone);
    assert_eq!(preview["action"]["review"]["writable"], false);
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn tagged_proof_workflow_checks_scaffold_and_agent_tactics_before_history() {
    let root = fixture();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: bool) -> bool { value }\n",
    )
    .unwrap();
    assert!(!root.path().join("specs").exists());
    let anchor = handle(root.path());
    let task_input = tagged(&anchor, "prove", json!({"kind":"property-task"}));
    let (success, task) = preview_value(root.path(), &task_input);
    assert!(success, "{task}");
    assert_eq!(
        task["action"]["review"]["plan"]["schema"],
        "fr-property-task-1"
    );
    let input = tagged(
        &anchor,
        "prove",
        json!({"kind":"formal-plan","properties":["identity"],
        "package":"specs","checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(
        preview["action"]["review"]["proof_validation"]["lake_build"],
        true
    );
    assert_eq!(
        preview["action"]["review"]["claims"]["model_theorem_checked"],
        false
    );
    assert!(!root.path().join("specs").exists());
    assert!(!root.path().join(".fr-history").exists());
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    let model = root.path().join("specs/FrSpecs/SrcLibRsRender.lean");
    let asset = root.path().join("specs/asset.bin");
    std::fs::write(&asset, [0, 255, 128, 10]).unwrap();
    let before = std::fs::read_to_string(&model).unwrap();
    let (success, goals) = run(root.path(), &["spec", "goals", "specs"], None);
    assert!(success, "{goals}");
    let name = goals["catalog"][0]["name"].as_str().unwrap();
    let (success, found) = run(root.path(), &["project", "find", name, "--signature"], None);
    assert!(success, "{found}");
    let proof_anchor = found["rows"][0][0].as_str().unwrap();
    let task_input = tagged(
        proof_anchor,
        "prove",
        json!({"kind":"proof-task","obligation":name}),
    );
    let (success, task) = preview_value(root.path(), &task_input);
    assert!(success, "{task}");
    assert_eq!(
        task["action"]["review"]["plan"]["schema"],
        "fr-proof-task-1"
    );
    let mut proof_delivery = delivery();
    proof_delivery["patch"] = json!("artifacts/proof.patch");
    let input = tagged(
        proof_anchor,
        "prove",
        json!({"kind":"proof-submission","obligation":name,
        "tactics":"rfl\n","checks":["syntax"],"delivery":proof_delivery}),
    );
    let mut wrong = input.clone();
    wrong["action"]["operation"]["tactics"] = json!("exact false\n");
    let (success, refused) = preview_value(root.path(), &wrong);
    assert!(!success, "{refused}");
    assert_eq!(std::fs::read_to_string(&model).unwrap(), before);
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(
        preview["action"]["review"]["proof_validation"]["strict_correspondence"],
        true
    );
    assert_eq!(
        preview["action"]["review"]["claims"]["model_theorem_checked"],
        true
    );
    assert_eq!(std::fs::read_to_string(&model).unwrap(), before);
    let basis = preview["action"]["basis"].as_str().unwrap();
    std::fs::write(&asset, [1, 255, 128, 10]).unwrap();
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert_eq!(std::fs::read_to_string(&model).unwrap(), before);
    std::fs::write(&asset, [0, 255, 128, 10]).unwrap();
    let lakefile = root.path().join("specs/lakefile.toml");
    let lake_original = std::fs::read_to_string(&lakefile).unwrap();
    std::fs::write(
        &lakefile,
        format!("{lake_original}\n# changed review configuration\n"),
    )
    .unwrap();
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert_eq!(std::fs::read_to_string(&model).unwrap(), before);
    std::fs::write(&lakefile, lake_original).unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert_eq!(result["claims"]["model_theorem_checked"], true);
    assert_eq!(result["claims"]["implementation_correspondence"], false);
    assert_eq!(std::fs::read(&asset).unwrap(), [0, 255, 128, 10]);
    assert!(!std::fs::read_to_string(model).unwrap().contains("sorry"));
    assert!(result["proof"]["receipt"].is_string());
}

#[test]
fn tagged_migration_matches_standalone_and_exports_creation_patch() {
    let root = fixture();
    let route = root.path().join("app/api/entries/route.ts");
    std::fs::create_dir_all(route.parent().unwrap()).unwrap();
    std::fs::write(
        &route,
        "export async function GET() { return Response.json([{ label: \"first\" }]); }\n",
    )
    .unwrap();
    let (success, features) = run(
        root.path(),
        &["project", "features", "--limit", "500"],
        None,
    );
    assert!(success, "{features}");
    let feature = features["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "feature")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let (success, selected) = run(
        root.path(),
        &["project", "find", "GET", "--signature"],
        None,
    );
    assert!(success, "{selected}");
    let anchor = selected["rows"][0][0].as_str().unwrap();
    let (success, standalone) = run(
        root.path(),
        &[
            "migrate",
            "feature",
            feature,
            "--to",
            "fastapi",
            "--out",
            "backend/entries.py",
            "--check",
            "syntax",
            "--diff-bytes",
            "65536",
        ],
        None,
    );
    assert!(success, "{standalone}");
    let input = tagged(
        anchor,
        "migrate",
        json!({"kind":"framework-migration","feature":feature,
        "to":"fastapi","out":"backend/entries.py","checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(
        preview["action"]["review"]["plan"]["migration"],
        standalone["migration"]
    );
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert!(root.path().join("backend/entries.py").exists());
    assert!(
        std::fs::read_to_string(root.path().join("artifacts/tagged.patch"))
            .unwrap()
            .contains("new file mode")
    );
}

#[test]
fn tagged_application_migration_executes_the_common_ir_planner() {
    let root = fixture();
    let source = "function signal(req: Request, res: Response) {\n  return res.status(200).json({id: req.params['id']});\n}\napp.get('/signals/:id', signal);\n";
    std::fs::write(root.path().join("api.ts"), source).unwrap();
    let goal = json!({"schema":"fr-agent-goal-1","purpose":"migrate",
        "selector":{"path":"api.ts"},
        "operation":{"kind":"framework-migration","to":"go-net-http"},
        "checks":["syntax"]});
    let (success, guide) = run(
        root.path(),
        &["guide", "--from", "-"],
        Some(&serde_json::to_vec(&goal).unwrap()),
    );
    assert!(success, "{guide}");
    assert_eq!(
        guide["intent_action"]["operation_kinds"],
        json!(["application-migration"])
    );
    let anchor = guide["target"]["handle"].as_str().unwrap();
    let mut input = tagged(
        anchor,
        "migrate",
        json!({"kind":"application-migration","to":"go-net-http","out":"generated",
            "checks":["syntax"],"delivery":delivery()}),
    );
    input["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(
        preview["action"]["review"]["plan"]["migration"]["source_kind"],
        "project-snapshot"
    );
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert!(root.path().join("generated/routes.go").is_file());
    assert_eq!(
        std::fs::read_to_string(root.path().join("api.ts")).unwrap(),
        source
    );
    assert!(
        std::fs::read_to_string(root.path().join("artifacts/tagged.patch"))
            .unwrap()
            .contains("generated/routes.go")
    );
}

#[test]
fn tagged_query_and_surface_action_preserve_the_selected_file() {
    let root = fixture();
    std::fs::write(root.path().join("style.css"), ".card { color: red; }\n").unwrap();
    let selection = serde_json::to_vec(&json!({"schema":"fr-agent-goal-1","purpose":"understand",
        "selector":{"path":"style.css"}}))
    .unwrap();
    let (success, selected) = run(root.path(), &["guide", "--from", "-"], Some(&selection));
    assert!(success, "{selected}");
    let anchor = selected["target"]["handle"].as_str().unwrap();
    let query = tagged(
        anchor,
        "understand",
        json!({"kind":"project-query",
        "requests":[{"id":"styles","arguments":["styles",anchor]}]}),
    );
    let (success, preview) = preview_value(root.path(), &query);
    assert!(success, "{preview}");
    assert_eq!(preview["action"]["review"]["writable"], false);
    let (success, styles) = run(root.path(), &["project", "styles", anchor], None);
    assert!(success, "{styles}");
    let edit = styles["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row["edit"]["id"].as_str().or_else(|| row["edit"].as_str()))
        .unwrap();
    let input = tagged(
        anchor,
        "change",
        json!({"kind":"surface-edit","edit":edit,"to":"panel",
        "checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("style.css")).unwrap(),
        ".panel { color: red; }\n"
    );
}

#[test]
fn legacy_write_checks_complete_packet_admission_before_history() {
    let root = fixture();
    let anchor = handle(root.path());
    let mut input: Value = serde_json::from_slice(&change_intent(&anchor, "change")).unwrap();
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    input["packet_limit"] = json!(1024);
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_owned"));
}

#[test]
fn direct_capability_intent_reuses_rename_planner_and_checks_unchanged_goal() {
    let root = fixture();
    let anchor = handle(root.path());
    let goal = json!({"schema":"fr-agent-goal-1","purpose":"change","target":anchor,
        "operation":{"kind":"capability","capability":"rename","parameters":{"new_name":"display"}},
        "checks":["syntax"]});
    let (success, guide) = run(
        root.path(),
        &["guide", "--from", "-"],
        Some(&serde_json::to_vec(&goal).unwrap()),
    );
    assert!(success, "{guide}");
    let mut input = tagged(
        &anchor,
        "change",
        json!({"kind":"capability","capability":"rename",
        "parameters":{"new_name":"display"},"checks":["syntax"],"delivery":delivery()}),
    );
    input["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert_eq!(preview["action"]["review"]["guide_basis"], guide["basis"]);
    let position = guide["target"]["position"].as_str().unwrap();
    let (success, standalone) = run(root.path(), &["rename", position, "display"], None);
    assert!(success, "{standalone}");
    assert_eq!(
        preview["action"]["review"]["diff"],
        standalone["changes"][0]["diff"]
    );
    let basis = preview["action"]["basis"].as_str().unwrap();
    let mut changed = input.clone();
    changed["action"]["operation"]["parameters"]["new_name"] = json!("other");
    let (success, refused) = preview_value(root.path(), &changed);
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(success, "{result}");
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("display(\"ok\")"));
}

#[test]
fn recipe_intents_reject_path_substrings_and_duplicate_declaration_names() {
    let root = fixture();
    let extra = root.path().join("copy/src/lib.rs");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(extra, "pub fn render() {}\n").unwrap();
    let (success, found) = run(
        root.path(),
        &["project", "find", "render", "--signature"],
        None,
    );
    assert!(success, "{found}");
    let anchor = found["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[3] == "src/lib.rs")
        .unwrap_or(&found["rows"][0])[0]
        .as_str()
        .unwrap();
    let input = tagged(
        anchor,
        "change",
        json!({"kind":"recipe",
        "recipe":"schema 1\nrecipe rename-render { rename to \"display\" where name=\"render\" in=\"src/lib.rs\" }",
        "checks":["syntax"],"delivery":delivery()}),
    );
    let (success, refused) = preview_value(root.path(), &input);
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn direct_read_capabilities_return_structure_without_history() {
    let root = fixture();
    let anchor = handle(root.path());
    for capability in [
        "symbols",
        "impact",
        "call-graph",
        "flow",
        "entry-points",
        "stitch",
        "duplicates",
        "dead-code",
        "declared-type",
    ] {
        let input = tagged(
            &anchor,
            "understand",
            json!({"kind":"capability", "capability":capability}),
        );
        let (success, preview) = preview_value(root.path(), &input);
        assert!(success, "{capability}: {preview}");
        assert_eq!(preview["action"]["review"]["writable"], false);
        let basis = preview["action"]["basis"].as_str().unwrap();
        let (success, refused) = run(
            root.path(),
            &["intent", "--from", "-", "--write", "--basis", basis],
            Some(&serde_json::to_vec(&input).unwrap()),
        );
        assert!(!success, "{capability}: {refused}");
    }
    std::fs::write(
        root.path().join("style.css"),
        ":root { --accent: red; --foreground: var(--accent); }\n",
    )
    .unwrap();
    let (success, found) = run(
        root.path(),
        &["project", "find", "--signature", "--", "--foreground"],
        None,
    );
    assert!(success, "{found}");
    let anchor = found["rows"][0][0].as_str().unwrap();
    let input = tagged(
        anchor,
        "trace",
        json!({"kind":"capability", "capability":"provenance"}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert!(preview["action"]["review"]["plan"]["planner"]["hops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hop| hop["kind"] == "var()"));
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn direct_call_and_extraction_ranges_remain_inside_the_retained_declaration() {
    let root = fixture();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "fn add(a: i32, b: i32) -> i32 { a + b }\nfn caller() -> i32 { add(1, 2) }\n",
    )
    .unwrap();
    let (success, found) = run(
        root.path(),
        &["project", "find", "caller", "--signature"],
        None,
    );
    assert!(success, "{found}");
    let anchor = found["rows"][0][0].as_str().unwrap();
    let source = std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
    let start = source.rfind("add(1, 2)").unwrap();
    let input = tagged(
        anchor,
        "change",
        json!({"kind":"capability", "capability":"inline-call",
        "range":{"start":start,"end":start+3}, "checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert!(preview["action"]["review"]["diff"]
        .as_str()
        .unwrap()
        .contains("1 + 2"));
    let goal = json!({"schema":"fr-agent-goal-1","purpose":"change","target":anchor,
        "operation":{"kind":"capability","capability":"inline-call",
            "range":{"start":start,"end":start+3}},"checks":["syntax"]});
    let (success, guide) = run(
        root.path(),
        &["guide", "--from", "-"],
        Some(&serde_json::to_vec(&goal).unwrap()),
    );
    assert!(success, "{guide}");
    assert_eq!(guide["actions"][0]["arguments"][1], "src/lib.rs:2:22");
    let mut guided = input.clone();
    guided["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
    let (success, preview) = preview_value(root.path(), &guided);
    assert!(success, "{preview}");
    let mut changed = input.clone();
    changed["action"]["operation"]["range"] = json!({"start":0,"end":2});
    let (success, refused) = preview_value(root.path(), &changed);
    assert!(!success, "{refused}");
    let mut extract = input;
    extract["action"]["operation"]["capability"] = json!("extract-variable");
    extract["action"]["operation"]["parameters"] = json!({"name":"answer"});
    extract["action"]["operation"]["range"] = json!({"start":start,"end":start+9});
    let goal = json!({"schema":"fr-agent-goal-1","purpose":"change","target":anchor,
        "operation":{"kind":"capability","capability":"extract-variable",
            "parameters":{"name":"answer", "range":"src/lib.rs:2:22-2:31"}},
        "constraints":{"allow_source":true},"checks":["syntax"]});
    let (success, guide) = run(
        root.path(),
        &["guide", "--from", "-"],
        Some(&serde_json::to_vec(&goal).unwrap()),
    );
    assert!(success, "{guide}");
    extract["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
    let (success, preview) = preview_value(root.path(), &extract);
    assert!(success, "{preview}");
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn automatic_prove_guide_keeps_its_inferred_formalization_route() {
    let root = fixture();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn render(value: u64) -> u64 { value }\n",
    )
    .unwrap();
    let anchor = handle(root.path());
    let goal = json!({"schema":"fr-agent-goal-1","purpose":"prove","target":anchor});
    let (success, guide) = run(
        root.path(),
        &["guide", "--from", "-"],
        Some(&serde_json::to_vec(&goal).unwrap()),
    );
    assert!(success, "{guide}");
    assert_eq!(guide["route"]["id"], "formalization");
    assert_eq!(
        guide["intent_action"]["operation_kinds"],
        json!(["property-task", "formal-plan"])
    );
    let mut input = tagged(&anchor, "prove", json!({"kind":"property-task"}));
    input["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    input["action"]["operation"] =
        json!({"kind":"project-query", "requests":[{"id":"show","arguments":["show",anchor]}]});
    let (success, refused) = preview_value(root.path(), &input);
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn tagged_review_byte_ceiling_refuses_before_source_history() {
    let root = fixture();
    let anchor = handle(root.path());
    let mut input = tagged(
        &anchor,
        "change",
        json!({"kind":"capability","capability":"rename",
        "parameters":{"new_name":"display"},"checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    input["action"]["report_bytes"] = json!(256);
    let (success, refused) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{refused}");
    assert!(!root.path().join(".fr-history").exists());
    assert!(std::fs::read_to_string(root.path().join("src/lib.rs"))
        .unwrap()
        .contains("render"));
}

#[test]
fn guided_flag_actions_preserve_both_explicit_boolean_choices() {
    for value in ["true", "false"] {
        let root = fixture();
        std::fs::write(
            root.path().join("src/lib.rs"),
            "const USE_NEW: bool = true;\nfn render() -> u64 { if USE_NEW { 7 } else { 9 } }\n",
        )
        .unwrap();
        let (success, found) = run(
            root.path(),
            &["project", "find", "USE_NEW", "--signature"],
            None,
        );
        assert!(success, "{found}");
        let anchor = found["rows"][0][0].as_str().unwrap();
        let goal = json!({"schema":"fr-agent-goal-1","purpose":"change","target":anchor,
            "operation":{"kind":"capability","capability":"remove-flag","parameters":{"value":value}},"checks":["syntax"]});
        let (success, guide) = run(
            root.path(),
            &["guide", "--from", "-"],
            Some(&serde_json::to_vec(&goal).unwrap()),
        );
        assert!(success, "{guide}");
        let mut input = tagged(
            anchor,
            "change",
            json!({"kind":"capability","capability":"remove-flag",
            "parameters":{"value":value},"checks":["syntax"],"delivery":delivery()}),
        );
        input["action"]["guide"] = json!({"goal":goal,"basis":guide["basis"]});
        let (success, preview) = preview_value(root.path(), &input);
        assert!(success, "{preview}");
        let basis = preview["action"]["basis"].as_str().unwrap();
        let (success, result) = run(
            root.path(),
            &["intent", "--from", "-", "--write", "--basis", basis],
            Some(&serde_json::to_vec(&input).unwrap()),
        );
        assert!(success, "{result}");
        let updated = std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
        assert!(!updated.contains("USE_NEW"));
        assert!(updated.contains(if value == "true" { "7" } else { "9" }));
        assert!(!updated.contains(if value == "true" { "9" } else { "7" }));
    }
}

#[test]
fn direct_value_traces_keep_literal_source_behind_explicit_reveal() {
    let root = fixture();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "fn render() -> &'static str { let hidden = \"must-stay-hidden\"; hidden }\n",
    )
    .unwrap();
    let (success, found) = run(
        root.path(),
        &["project", "find", "hidden", "--signature", "--locals"],
        None,
    );
    assert!(success, "{found}");
    let anchor = found["rows"][0][0].as_str().unwrap();
    let input = tagged(
        anchor,
        "trace",
        json!({"kind":"capability","capability":"flow"}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert!(!preview.to_string().contains("must-stay-hidden"));
    assert_eq!(
        preview["action"]["review"]["plan"]["planner"]["boundaries"][0]["kind"],
        "origin"
    );
    std::fs::write(
        root.path().join("style.css"),
        ":root { --hidden: must-stay-hidden; }\n",
    )
    .unwrap();
    let (success, found) = run(
        root.path(),
        &["project", "find", "--signature", "--", "--hidden"],
        None,
    );
    assert!(success, "{found}");
    let anchor = found["rows"][0][0].as_str().unwrap();
    let input = tagged(
        anchor,
        "trace",
        json!({"kind":"capability","capability":"provenance"}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    assert!(!preview.to_string().contains("must-stay-hidden"));
}

#[test]
fn tagged_failed_original_checks_preserve_normalized_evidence_and_recovery() {
    let root = fixture();
    std::fs::write(
        root.path().join(".fr/checks.json"),
        serde_json::to_vec(&json!({"schema":1,
        "checks":[{"name":"syntax","argv":["false"],"cwd":".","timeout_seconds":10,
        "covers":["original source gate"]}]}))
        .unwrap(),
    )
    .unwrap();
    let original = std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap();
    let anchor = handle(root.path());
    let input = tagged(
        &anchor,
        "change",
        json!({"kind":"capability","capability":"rename",
        "parameters":{"new_name":"display"},"checks":["syntax"],"delivery":delivery()}),
    );
    let (success, preview) = preview_value(root.path(), &input);
    assert!(success, "{preview}");
    let basis = preview["action"]["basis"].as_str().unwrap();
    let (success, result) = run(
        root.path(),
        &["intent", "--from", "-", "--write", "--basis", basis],
        Some(&serde_json::to_vec(&input).unwrap()),
    );
    assert!(!success, "{result}");
    assert_eq!(result["schema"], "fr-agent-action-result-2");
    assert_eq!(result["passed"], false);
    assert_eq!(result["workflow"]["transaction_status"], "planned");
    assert_eq!(result["workflow"]["stages"][0]["status"], "failed");
    assert_eq!(result["workflow"]["stages"][1]["status"], "pending");
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        original
    );
    assert!(!root.path().join("artifacts/tagged.patch").exists());
    assert!(result["transaction"].is_number());
}
