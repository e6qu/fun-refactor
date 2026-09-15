use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn python() -> Command {
    let mut command = Command::new("python3");
    command.env("PYTHONPATH", root().join("sdk/python/src"));
    command
}

fn assert_evidence_digests(evidence: &Path, manifest: &Value) {
    for (name, expected) in manifest["files"].as_object().unwrap() {
        let bytes = fs::read(evidence.join(name)).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            expected.as_str().unwrap()
        );
    }
}

#[test]
fn python_sdk_unit_tests_pass_without_dependencies() {
    let output = python()
        .args(["-m", "unittest", "discover", "-s"])
        .arg(root().join("sdk/python/tests"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn python_runtime_discovers_discloses_reviews_and_executes_without_json_glue() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::create_dir_all(workspace.path().join(".fr")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\n\
         pub fn caller() -> String { render(\"ok\") }\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(".fr/checks.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "checks": [{
                "name": "syntax", "argv": ["true"], "cwd": ".",
                "timeout_seconds": 10, "covers": ["selected source state"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let objects = tempfile::tempdir().unwrap();
    let script = r#"# => complete structured agent runtime fixture
import json, sys
from fr_ir import DirectoryObjectStore, FrClient, TaskChange, TaskDelivery, TaskTarget

client = FrClient(sys.argv[1], executable=sys.argv[2])
found = client.project('find', 'render', '--signature')
handle = found.at('/rows/0/0')
session = client.context(
    handle, view='evidence', token_limit=4096,
    store=DirectoryObjectStore(sys.argv[3]),
)
code_map = session.materialize_section('code_map')
packet = session.packet(
    {'code_map': '/model/code_map'}, include_actions=False, max_bytes=4096,
)
change = TaskChange(
    [], [TaskTarget('render-body', handle, 'replace-body',
                    fragment='{ value.to_uppercase() }')],
    {'files-changed': 1, 'edits': 1, 'changed-operations': 1,
     'paths-changed': ['src/lib.rs']},
    ['syntax'], TaskDelivery(patch='artifacts/change.patch'),
)
review = client.review(change)
result = client.execute(review)
print(json.dumps({
    'code_map_fields': len(code_map),
    'code_map_mentions_target': 'render' in json.dumps(code_map),
    'context_schema': packet.schema,
    'context_calls': session.calls,
    'cached_objects': len(session.cached_digests),
    'basis': review.task_change_basis,
    'passed': result.passed,
    'transaction_status': result.at('/workflow/transaction_status'),
}))
"#;
    let output = python()
        .args(["-c", script])
        .arg(workspace.path())
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg(objects.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["code_map_fields"].as_u64().unwrap() >= 1);
    assert_eq!(report["code_map_mentions_target"], true);
    assert_eq!(report["context_schema"], "fr-agent-context-1");
    assert!(report["context_calls"].as_u64().unwrap() >= 3);
    assert!(report["cached_objects"].as_u64().unwrap() >= 1);
    assert_eq!(report["passed"], true);
    assert_eq!(report["transaction_status"], "applied");
    assert!(report["basis"].as_str().unwrap().starts_with("frtc1:"));
    assert!(fs::read_to_string(workspace.path().join("src/lib.rs"))
        .unwrap()
        .contains("value.to_uppercase"));
    assert!(
        fs::read_to_string(workspace.path().join("artifacts/change.patch"))
            .unwrap()
            .contains("+pub fn render")
    );
}

#[test]
fn python_runtime_session_policy_matches_rust_exhaustively() {
    let script = r#"# => Python agent-session kernel corpus
from fr_ir.runtime import _session_step
for state in range(3):
    for action in range(2):
        for preview in (False, True):
            for manifest in (False, True):
                for basis in (False, True):
                    print(_session_step(state, action, preview, manifest, basis))
"#;
    let output = python().args(["-c", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let mut expected = Vec::new();
    for state in 0..3 {
        for action in 0..2 {
            for preview in [false, true] {
                for manifest in [false, true] {
                    for basis in [false, true] {
                        expected.push(
                            fun_refactor::project::task_change::agent_session_step(
                                state, action, preview, manifest, basis,
                            )
                            .to_string(),
                        );
                    }
                }
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn python_context_admission_matches_rust_at_every_boundary() {
    let script = r#"# => Python context-admission kernel corpus
from fr_ir.context import _context_materialization_admitted, _object_store_admitted
samples = [0, 1, 63, 64, 65, 2**64 - 1]
for calls in samples:
    for limit in samples:
        for session in (False, True):
            for complete in (False, True):
                for digest in (False, True):
                    print(str(_context_materialization_admitted(
                        calls, limit, session, complete, digest)).lower())
objects = [0, 1, 65535, 65536, 65537, 2**64 - 1]
encoded = [0, 1, 67108863, 67108864, 67108865, 2**64 - 1]
for count in objects:
    for size in encoded:
        for digest in (False, True):
            for canonical in (False, True):
                for root in (False, True):
                    print(str(_object_store_admitted(
                        count, size, digest, canonical, root)).lower())
"#;
    let output = python().args(["-c", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let samples = [0, 1, 63, 64, 65, usize::MAX];
    let object_samples = [0, 1, 65_535, 65_536, 65_537, usize::MAX];
    let byte_samples = [0, 1, 67_108_863, 67_108_864, 67_108_865, usize::MAX];
    let mut expected = Vec::new();
    for calls in samples {
        for limit in samples {
            for session_matches in [false, true] {
                for complete in [false, true] {
                    for digest_matches in [false, true] {
                        expected.push(
                            fun_refactor::project::context_materialization_admitted(
                                calls,
                                limit,
                                session_matches,
                                complete,
                                digest_matches,
                            )
                            .to_string(),
                        );
                    }
                }
            }
        }
    }
    for objects in object_samples {
        for encoded_bytes in byte_samples {
            for digest_matches in [false, true] {
                for records_canonical in [false, true] {
                    for root_present in [false, true] {
                        expected.push(
                            fun_refactor::project::object_store_admitted(
                                objects,
                                encoded_bytes,
                                digest_matches,
                                records_canonical,
                                root_present,
                            )
                            .to_string(),
                        );
                    }
                }
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn python_authors_a_property_from_the_rust_task_shape() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn both(left: bool, right: bool) -> bool { left && right }\n",
    )
    .unwrap();
    let task = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(workspace.path())
        .args(["spec", "property-task", "src/lib.rs::both"])
        .output()
        .unwrap();
    assert!(task.status.success());
    let task_path = workspace.path().join("task.json");
    let property_path = workspace.path().join("property.json");
    fs::write(&task_path, task.stdout).unwrap();
    let script = r#"# => agent property SDK fixture
import json, sys
from fr_ir import PropertyProposition as Prop, PropertyTask, PropertyTerm as Term
task = PropertyTask.from_data(json.load(open(sys.argv[1], encoding='utf-8')))
x, y = Term.variable('x'), Term.variable('y')
property_ = task.property(
    'commutative',
    [{'name': 'x', 'lean_type': 'Bool'}, {'name': 'y', 'lean_type': 'Bool'}],
    Prop.equals(Term.model(x, y), Term.model(y, x)),
)
open(sys.argv[2], 'w', encoding='utf-8').write(property_.to_json(indent=2) + '\n')
"#;
    let authored = python()
        .args(["-c", script])
        .arg(&task_path)
        .arg(&property_path)
        .output()
        .unwrap();
    assert!(
        authored.status.success(),
        "{}",
        String::from_utf8_lossy(&authored.stderr)
    );
    let plan = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(workspace.path())
        .args(["spec", "plan", "src/lib.rs::both", "--property-from"])
        .arg(&property_path)
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan_path = workspace.path().join("plan.json");
    fs::write(&plan_path, &plan.stdout).unwrap();
    let validated = python()
        .args([
            "-c",
            "# => validate generated plan\nimport sys; from fr_ir import FormalPlan; FormalPlan.from_json(open(sys.argv[1], encoding='utf-8').read())",
        ])
        .arg(&plan_path)
        .output()
        .unwrap();
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let plan: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["properties"][0]["kind"], "agent");
    assert_eq!(plan["properties"][0]["agent_spec"]["name"], "commutative");
}

#[test]
fn python_and_rust_publish_the_same_semantic_catalog() {
    let script = "# => catalog fixture\nimport json; from fr_ir import *; print(json.dumps([TYPE_KINDS, STATEMENT_KINDS, EXPRESSION_KINDS, TEMPLATE_KINDS, [x.value for x in BinaryOp], [x.value for x in UnaryOp], ROLE_NAMES, INTENT_OPERATIONS]))";
    let output = python().args(["-c", script]).output().unwrap();
    assert!(output.status.success());
    let python: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let rust = [
        fun_refactor::project::semantic_ir::TYPE_KINDS,
        &fun_refactor::project::semantic_ir::STATEMENT_KINDS[..25],
        &fun_refactor::project::semantic_ir::EXPRESSION_KINDS[..28],
        fun_refactor::project::semantic_ir::TEMPLATE_KINDS,
        fun_refactor::project::semantic_ir::BINARY_OPERATORS,
        fun_refactor::project::semantic_ir::UNARY_OPERATORS,
        fun_refactor::project::semantic_intent::ROLE_NAMES,
        fun_refactor::project::semantic_intent::OPERATION_NAMES,
    ];
    for (python, rust) in python.iter().zip(rust) {
        assert_eq!(python.iter().map(String::as_str).collect::<Vec<_>>(), rust);
    }
}

#[test]
fn every_python_constructor_deserializes_and_canonicalizes_in_rust() {
    let generated = python()
        .arg(root().join("sdk/python/examples/exhaustive.py"))
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("semantic.json");
    fs::write(&input, &generated.stdout).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(temp.path())
        .args(["author", "validate-semantic", "--from"])
        .arg(&input)
        .arg("--canonical")
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(output.status.success(), "{report}");
    assert_eq!(report["valid"], true);
    assert_eq!(report["source_free"], true);
    assert_eq!(report["statements"], 65);
    assert_eq!(report["canonical"]["schema"], "fr-semantic-body-1");
}

#[test]
fn python_semantic_changes_apply_through_the_rust_engine() {
    let temp = tempfile::tempdir().unwrap();
    let body = temp.path().join("body.json");
    let change = temp.path().join("change.json");
    let script = r#"# => checked semantic change fixture
import sys
from fr_ir import Change, Expr, SemanticBody, SemanticChange, Stmt
body = SemanticBody([Stmt.Return(Expr.Name("left"))])
body.write(sys.argv[1])
SemanticChange(body, [
    Change.InsertStatement("/body", 0, Stmt.Comment("temporary")),
    Change.Replace("/body/1/value", Expr.Name("right")),
    Change.DeleteStatement("/body/0"),
]).write(sys.argv[2])
"#;
    let generated = python()
        .args(["-c", script])
        .arg(&body)
        .arg(&change)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(temp.path())
        .args([
            "author",
            "apply-semantic-change",
            "--body",
            "body.json",
            "--change",
            "change.json",
            "--canonical",
        ])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(output.status.success(), "{report}");
    assert_eq!(report["canonical"]["body"].as_array().unwrap().len(), 1);
    assert_eq!(report["canonical"]["body"][0]["value"]["value"], "right");
    let change: Value = serde_json::from_slice(&fs::read(change).unwrap()).unwrap();
    assert_eq!(report["input_basis"], change["base"]);
}

#[test]
fn python_semantic_intents_compile_and_apply_through_the_rust_engine() {
    let temp = tempfile::tempdir().unwrap();
    let body = temp.path().join("body.json");
    let intent = temp.path().join("intent.json");
    let script = r#"# => checked semantic intent fixture
import sys
from fr_ir import BinaryOp, Expr, Intent, LocatorStep, NodeCategory, Role, SemanticBody, SemanticIntent, Stmt
body = SemanticBody([Stmt.Return(Expr.Binary(BinaryOp.ADD, Expr.Name("value"), Expr.Int(1)))])
body.write(sys.argv[1])
SemanticIntent(body, [Intent.SetInt([
    LocatorStep(Role.STATEMENT, index=0, category=NodeCategory.STATEMENT, kind="return"),
    LocatorStep(Role.RESULT, category=NodeCategory.EXPRESSION, kind="binary"),
    LocatorStep(Role.RIGHT, category=NodeCategory.EXPRESSION, kind="int"),
], "1", "2")]).write(sys.argv[2])
"#;
    let generated = python()
        .args(["-c", script])
        .arg(&body)
        .arg(&intent)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(temp.path())
        .args([
            "author",
            "apply-semantic-intent",
            "--body",
            "body.json",
            "--intent",
            "intent.json",
            "--canonical",
            "--compiled",
        ])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(output.status.success(), "{report}");
    assert_eq!(report["refinement_checked"], true);
    assert_eq!(
        report["canonical"].pointer("/body/0/value/value/right/value"),
        Some(&Value::String("2".into()))
    );
    assert_eq!(
        report["compiled_change"]["operations"][0]["path"],
        "/body/0/value/value/right"
    );
    let intent: Value = serde_json::from_slice(&fs::read(intent).unwrap()).unwrap();
    assert_eq!(report["input_basis"], intent["base"]);
}

#[test]
fn checked_sdk_evaluation_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/agent-ir-sdk-eval.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    let expected: Value = serde_json::from_slice(
        &fs::read(root().join("tests/agent-eval/semantic-ir-sdk.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn checked_agent_runtime_context_comparison_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/agent-runtime-context.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    let expected: Value = serde_json::from_slice(
        &fs::read(root().join("tests/agent-eval/agent-runtime-context.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn semantic_edit_plan_evaluation_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/semantic-edit-plan-eval.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    let retained: Value = serde_json::from_slice(
        &fs::read(root().join("tests/agent-eval/semantic-edit-plan.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, retained);
    assert!(actual["equivalence"]
        .as_object()
        .unwrap()
        .values()
        .all(|value| value == true));
    assert_eq!(
        actual["routes"]["scalar-plan"]["commands_before_lifecycle"],
        2
    );
    assert!(
        actual["routes"]["scalar-plan"]["measured_context_bytes"]
            .as_u64()
            .unwrap()
            < actual["routes"]["explicit-intent"]["measured_context_bytes"]
                .as_u64()
                .unwrap()
    );
}

#[test]
fn checked_semantic_intent_evaluation_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/semantic-intent-eval.py"))
        .arg("--fr")
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg("--output")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    let expected: Value = serde_json::from_slice(
        &fs::read(root().join("tests/agent-eval/semantic-intent.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn retained_agent_pair_is_complete_and_digest_bound() {
    let evidence = root().join("tests/agent-eval/results/2026-09-11-semantic-ir-sdk");
    let manifest: Value =
        serde_json::from_slice(&fs::read(evidence.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["model"], "gpt-5.6-luna");
    assert_eq!(manifest["reasoning_effort"], "low");
    assert_evidence_digests(&evidence, &manifest);
    let sdk: Value =
        serde_json::from_slice(&fs::read(evidence.join("semantic-ir-sdk-fr/result.json")).unwrap())
            .unwrap();
    let direct: Value = serde_json::from_slice(
        &fs::read(evidence.join("semantic-ir-sdk-files/result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(sdk["route"], "python-sdk");
    assert_eq!(direct["route"], "direct-json");
    assert_eq!(sdk["passed"], true);
    assert_eq!(direct["passed"], true);
    assert_eq!(sdk["implementation_source_reads"], 0);
    assert_eq!(direct["implementation_source_reads"], 0);
}

#[test]
fn retained_semantic_intent_pair_is_complete_and_digest_bound() {
    let evidence = root().join("tests/agent-eval/results/2026-09-12-semantic-intent");
    let manifest: Value =
        serde_json::from_slice(&fs::read(evidence.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["model"], "gpt-5.6-luna");
    assert_eq!(manifest["reasoning_effort"], "low");
    assert_evidence_digests(&evidence, &manifest);

    let intent: Value =
        serde_json::from_slice(&fs::read(evidence.join("semantic-intent-fr/result.json")).unwrap())
            .unwrap();
    let body: Value = serde_json::from_slice(
        &fs::read(evidence.join("semantic-intent-files/result.json")).unwrap(),
    )
    .unwrap();
    for result in [&intent, &body] {
        assert_eq!(result["passed"], true);
        assert_eq!(result["exact_semantic_body"], true);
        assert_eq!(result["behavior_passed"], true);
        assert_eq!(result["direct_source_reads"], 0);
    }
    assert_eq!(intent["route"], "semantic-intent");
    assert_eq!(intent["filtered_locator_used"], true);
    assert_eq!(body["route"], "complete-body");
    assert!(intent["payload_bytes"].as_u64().unwrap() < body["payload_bytes"].as_u64().unwrap());
    assert!(intent["commands"].as_u64().unwrap() < body["commands"].as_u64().unwrap());
}

#[test]
fn retained_semantic_edit_plan_attempts_are_complete_and_digest_bound() {
    let results = root().join("tests/agent-eval/results");
    let diagnostic = results.join("2026-09-12-semantic-edit-plan-diagnostic-1");
    let accepted = results.join("2026-09-12-semantic-edit-plan");
    for evidence in [&diagnostic, &accepted] {
        let manifest: Value =
            serde_json::from_slice(&fs::read(evidence.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["model"], "gpt-5.6-luna");
        assert_eq!(manifest["reasoning_effort"], "low");
        assert_evidence_digests(evidence, &manifest);
    }

    let diagnostic_direct: Value = serde_json::from_slice(
        &fs::read(diagnostic.join("semantic-edit-plan-direct/result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(diagnostic_direct["passed"], false);
    assert_eq!(diagnostic_direct["exact_semantic_body"], true);
    assert_eq!(diagnostic_direct["separate_query_avoided"], false);

    for name in ["semantic-edit-plan-direct", "semantic-edit-plan-explicit"] {
        let result: Value =
            serde_json::from_slice(&fs::read(accepted.join(name).join("result.json")).unwrap())
                .unwrap();
        assert_eq!(result["passed"], true);
        assert_eq!(result["exact_semantic_body"], true);
        assert_eq!(result["behavior_passed"], true);
        assert_eq!(result["direct_source_reads"], 0);
    }
    let direct: Value = serde_json::from_slice(
        &fs::read(accepted.join("semantic-edit-plan-direct/result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(direct["route"], "direct-scalar-plan");
    assert_eq!(direct["separate_query_avoided"], true);
    assert_eq!(direct["payload_bytes"], 0);
}
