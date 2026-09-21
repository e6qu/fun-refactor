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
    let mut paths = vec![root().join("sdk/python/src")];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    command.env("PYTHONPATH", std::env::join_paths(paths).unwrap());
    command
}

fn assert_evidence_digests(evidence: &Path, manifest: &Value) {
    for (name, expected) in manifest["files"].as_object().unwrap() {
        let bytes = fs::read(evidence.join(name)).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(bytes)),
            expected.as_str().unwrap()
        );
    }
}

#[test]
fn python_sdk_tests_pass_with_the_declared_test_extra() {
    let output = python()
        .args(["-m", "pytest", "-q"])
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
fn python_package_keeps_explicit_module_boundaries() {
    let package = root().join("sdk/python/src/fr_ir");
    assert_eq!(fs::read(package.join("__init__.py")).unwrap(), b"");
    assert!(!package.join("__main__.py").exists());
    for module in [
        "ir.py",
        "runtime.py",
        "context.py",
        "intent.py",
        "intent_actions.py",
        "guide.py",
        "http_store.py",
    ] {
        let source = fs::read_to_string(package.join(module)).unwrap();
        assert!(!source.contains("__all__"), "{module} mutates __all__");
    }
}

#[test]
fn checked_agent_guide_context_comparison_is_reproducible() {
    let evidence = root().join("tests/agent-eval/agent-guide-context.json");
    let output = python()
        .arg(root().join("tools/agent-guide-context.py"))
        .arg("--audit")
        .arg(&evidence)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&fs::read(evidence).unwrap()).unwrap();
    assert_eq!(report["guided"]["process_calls"], 4);
    assert_eq!(report["manual"]["process_calls"], 3);
    assert!(report["equality"]
        .as_object()
        .unwrap()
        .values()
        .all(|value| value == true));
}

#[test]
fn python_guide_delivers_an_exact_scalar_goal_and_refuses_stale_guidance() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join(".fr")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    fs::write(
        workspace.path().join("app.rs"),
        "pub fn calculate(value: i64) -> i64 { value + 7 }\n",
    )
    .unwrap();
    fs::write(workspace.path().join(".fr/checks.json"),serde_json::to_vec(&serde_json::json!({"schema":1,"checks":[{"name":"syntax","argv":["true"],"cwd":".","timeout_seconds":10,"covers":["fixture state"]}]})).unwrap()).unwrap();
    let output=python().arg("-c").arg(r#"# => executable guided scalar fixture
import json, sys
from pathlib import Path
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient, FrRuntimeError
client=FrClient(sys.argv[1], executable=sys.argv[2])
delivery=TaskDelivery(patch='artifacts/change.patch',check_output_bytes=256)
goal=AgentGoal('change',selector=GoalSelector(name='calculate'),
    operation=GoalOperation('semantic-scalar', {'operation':'set-int','from':'7','to':'9'}),
    checks=('syntax',),delivery=delivery)
guide=client.guide(goal)
assert len(guide.actions()) == 1
review=client.review_guide(guide,guide.semantic_scalar_action())
assert '+ 9' in review.at('/diff')
result=client.execute_guide(review)
assert result.passed
assert Path(sys.argv[1], 'artifacts/change.patch').is_file()
assert '+ 9' in Path(sys.argv[1], 'app.rs').read_text()
transaction=result.at('/transaction')
client.call('history','undo',str(transaction),'--write')
assert '+ 7' in Path(sys.argv[1], 'app.rs').read_text()
client.call('history','redo',str(transaction),'--write')
assert '+ 9' in Path(sys.argv[1], 'app.rs').read_text()
try:
    client.execute_guide(review)
except FrRuntimeError:
    pass
else:
    raise AssertionError('stale guidance accepted')
print(json.dumps({'passed':result.passed,'stages':len(result.at('/workflow/stages')),'guide_bytes':guide.at('/serialized_bytes')}))
"#).arg(workspace.path()).arg(env!("CARGO_BIN_EXE_fr")).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["stages"], 8);
}

#[test]
fn python_guide_reviews_and_executes_application_migration() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join(".fr")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    let source = "function signal(req: Request, res: Response) {\n  return res.status(200).json({id: req.params['id']});\n}\napp.get('/signals/:id', signal);\n";
    fs::write(workspace.path().join("api.ts"), source).unwrap();
    fs::write(workspace.path().join(".fr/checks.json"),serde_json::to_vec(&serde_json::json!({"schema":1,"checks":[{"name":"syntax","argv":["true"],"cwd":".","timeout_seconds":10,"covers":["fixture state"]}]})).unwrap()).unwrap();
    let output = python()
        .arg("-c")
        .arg(
            r#"# => executable guided application migration fixture
import json, sys
from pathlib import Path
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.intent_actions import ApplicationMigrationOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient
client=FrClient(sys.argv[1], executable=sys.argv[2])
delivery=TaskDelivery(patch='artifacts/migration.patch',check_output_bytes=256)
goal=AgentGoal('migrate',selector=GoalSelector(path='api.ts'),
    operation=GoalOperation('framework-migration', {'to':'go-net-http'}),
    checks=('syntax',),delivery=delivery)
guide=client.guide(goal)
action=TaggedIntentAction(ApplicationMigrationOperation(
    to='go-net-http',out='generated',checks=('syntax',),delivery=delivery),
    diff_bytes=65536,report_bytes=65536)
review=client.review_guide(guide,action)
assert review.at('/kind') == 'application-migration'
assert 'generated/routes.go' in review.at('/diff')
result=client.execute_guide(review)
assert result.passed
assert Path(sys.argv[1], 'api.ts').read_text() == sys.argv[3]
assert Path(sys.argv[1], 'generated/routes.go').is_file()
print(json.dumps({'passed':result.passed,'stages':len(result.at('/workflow/stages'))}))
"#,
        )
        .arg(workspace.path())
        .arg(env!("CARGO_BIN_EXE_fr"))
        .arg(source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["stages"], 8);
}

#[test]
fn native_intent_context_evidence_is_source_bound_and_arithmetically_valid() {
    let output = python()
        .arg(root().join("tools/native-intent-context.py"))
        .arg("--audit")
        .arg(root().join("tests/agent-eval/native-intent-context.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["native"]["process_calls"], 1);
    assert_eq!(report["native"]["progressive_disclosure_calls"], 0);
    assert!(report["progressive"]["process_calls"].as_u64().unwrap() > 1);
    assert!(report["reduction"]["response_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn intent_action_context_evidence_is_source_bound_and_arithmetically_valid() {
    let output = python()
        .arg(root().join("tools/intent-action-context.py"))
        .arg("--audit")
        .arg(root().join("tests/agent-eval/intent-action-context.json"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["composed"]["process_calls"], 3);
    assert_eq!(report["intent_action"]["process_calls"], 2);
    assert_eq!(report["reduction"]["process_calls"], 1);
    assert!(report["equality"]
        .as_object()
        .unwrap()
        .values()
        .all(|value| value.as_bool() == Some(true)));
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
from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget

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
fn python_runtime_compiles_a_high_level_intent_into_bounded_evidence() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\n\
         pub fn caller() -> String { render(\"ok\") }\n",
    )
    .unwrap();
    let objects = tempfile::tempdir().unwrap();
    let script = r#"# => declarative native and progressive intent fixture
import json, sys
from fr_ir.context import DirectoryObjectStore, restore_stored_value
from fr_ir.intent import AgentIntent
from fr_ir.runtime import FrClient

client = FrClient(sys.argv[1], executable=sys.argv[2])
found = client.project('find', 'render', '--signature')
handle = found.at('/rows/0/0')
intent = AgentIntent(
    handle, 'trace', call_limit=192, packet_limit=65536,
)
store = DirectoryObjectStore(sys.argv[3])
prepared = client.prepare(intent, store=store)
compiled = client.compile(intent, store=store)
assert compiled.at('/selected') == prepared.at('/selected')
for name, digest in compiled.at('/object_digests').items():
    assert restore_stored_value(store, digest) == compiled.at('/selected/' + name)
print(json.dumps({
    'purpose': prepared.intent.purpose,
    'sections': sorted(prepared.at('/selected')),
    'calls': prepared.session.calls,
    'bytes': prepared.at('/serialized_bytes'),
    'cached_objects': len(prepared.session.cached_digests),
    'target': prepared.session.session_identity()[3],
    'native_calls': compiled.at('/calls'),
    'native_bytes': compiled.at('/serialized_bytes'),
    'native_engine': compiled.at('/execution/engine'),
    'native_stored': len(compiled.stored_digests),
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
    assert_eq!(report["purpose"], "trace");
    assert_eq!(
        report["sections"],
        serde_json::json!(["call_traces", "code_map", "sources_and_sinks"])
    );
    assert!(report["calls"].as_u64().unwrap() <= 193);
    assert!(report["bytes"].as_u64().unwrap() <= 65_536);
    assert!(report["cached_objects"].as_u64().unwrap() > 0);
    assert!(report["target"].as_str().unwrap().starts_with("frp1:"));
    assert_eq!(report["native_calls"], 0);
    assert!(report["native_bytes"].as_u64().unwrap() <= 65_536);
    assert_eq!(report["native_engine"], "native");
    assert_eq!(report["native_stored"], 3);
}

#[test]
fn python_runtime_executes_one_intent_bound_reviewed_change() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::create_dir_all(workspace.path().join(".fr")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\n",
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
    let script = r#"# => intent action SDK fixture
import json, sys
from fr_ir.intent import AgentIntent, IntentAction
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient

client = FrClient(sys.argv[1], executable=sys.argv[2])
handle = client.project('find', 'render', '--signature').at('/rows/0/0')
change = TaskChange(
    [],
    [TaskTarget('render-body', handle, 'replace-body',
                fragment='{ value.to_uppercase() }\n')],
    {'files-changed': 1, 'edits': 1, 'changed-operations': 1,
     'paths-changed': ['src/lib.rs']},
    ['syntax'],
    TaskDelivery(patch='artifacts/change.patch'),
)
compiled = client.compile(AgentIntent(
    handle, 'change', packet_limit=65536, action=IntentAction(change),
))
assert compiled.action_basis.startswith('fraa1:')
assert compiled.at('/action/review/ready') is True
result = client.execute_intent(compiled)
print(json.dumps({
    'passed': result.passed,
    'basis': result.at('/action_basis'),
    'status': result.at('/action/workflow/transaction_status'),
}))
"#;
    let output = python()
        .args(["-c", script])
        .arg(workspace.path())
        .arg(env!("CARGO_BIN_EXE_fr"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["status"], "applied");
    assert!(report["basis"].as_str().unwrap().starts_with("fraa1:"));
    assert!(fs::read_to_string(workspace.path().join("src/lib.rs"))
        .unwrap()
        .contains("to_uppercase"));
    assert!(workspace.path().join("artifacts/change.patch").is_file());
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
fn python_intent_purpose_sections_match_rust_exhaustively() {
    let script = r#"# => Python purpose-section kernel corpus
from fr_ir.intent import _intent_section_allowed
for purpose in range(7):
    for section in range(6):
        print(str(_intent_section_allowed(purpose, section)).lower())
"#;
    let output = python().args(["-c", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = String::from_utf8(output.stdout).unwrap();
    let mut observed = observed.lines();
    for purpose in 0..7 {
        for section in 0..6 {
            assert_eq!(
                observed.next().map(|line| line.parse::<bool>().unwrap()),
                Some(fun_refactor::project::agent_intent_section_allowed(
                    purpose, section
                ))
            );
        }
    }
    assert!(observed.next().is_none());
}

#[test]
fn python_intent_admission_matches_rust_at_every_boundary() {
    let script = r#"# => Python intent-admission kernel corpus
from fr_ir.intent import _intent_admitted
needs_samples = [0, 1, 32, 33]
section_samples = [0, 1, 8, 9]
call_samples = [0, 1, 512, 513]
packet_samples = [0, 1024, 65536, 65537]
for needs in needs_samples:
    for sections in section_samples:
        for calls in call_samples:
            for call_limit in call_samples:
                for packet_bytes in packet_samples:
                    for packet_limit in packet_samples:
                        for target in (False, True):
                            for session in (False, True):
                                for complete in (False, True):
                                    print(str(_intent_admitted(
                                        needs, sections, calls, call_limit,
                                        packet_bytes, packet_limit,
                                        target, session, complete)).lower())
"#;
    let output = python().args(["-c", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let needs_samples = [0, 1, 32, 33];
    let section_samples = [0, 1, 8, 9];
    let call_samples = [0, 1, 512, 513];
    let packet_samples = [0, 1_024, 65_536, 65_537];
    let mut expected = Vec::new();
    for needs in needs_samples {
        for sections in section_samples {
            for calls in call_samples {
                for call_limit in call_samples {
                    for packet_bytes in packet_samples {
                        for packet_limit in packet_samples {
                            for target_matches in [false, true] {
                                for session_matches in [false, true] {
                                    for complete in [false, true] {
                                        expected.push(
                                            fun_refactor::project::agent_intent_admitted(
                                                needs,
                                                sections,
                                                calls,
                                                call_limit,
                                                packet_bytes,
                                                packet_limit,
                                                target_matches,
                                                session_matches,
                                                complete,
                                            )
                                            .to_string(),
                                        );
                                    }
                                }
                            }
                        }
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
from fr_ir.ir import PropertyProposition as Prop, PropertyTask, PropertyTerm as Term
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
            "# => validate generated plan\nimport sys; from fr_ir.ir import FormalPlan; FormalPlan.from_json(open(sys.argv[1], encoding='utf-8').read())",
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
    let script = "# => catalog fixture\nimport json; from fr_ir.ir import BinaryOp, EXPRESSION_KINDS, INTENT_OPERATIONS, ROLE_NAMES, STATEMENT_KINDS, TEMPLATE_KINDS, TYPE_KINDS, UnaryOp; print(json.dumps([TYPE_KINDS, STATEMENT_KINDS, EXPRESSION_KINDS, TEMPLATE_KINDS, [x.value for x in BinaryOp], [x.value for x in UnaryOp], ROLE_NAMES, INTENT_OPERATIONS]))";
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
from fr_ir.ir import Change, Expr, SemanticBody, SemanticChange, Stmt
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
from fr_ir.ir import BinaryOp, Expr, Intent, LocatorStep, NodeCategory, Role, SemanticBody, SemanticIntent, Stmt
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
fn checked_agent_context_workspace_comparison_is_reproducible() {
    let temp = tempfile::tempdir().unwrap();
    let output_path = temp.path().join("report.json");
    let output = Command::new("python3")
        .arg(root().join("tools/agent-context-workspace.py"))
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
        &fs::read(root().join("tests/agent-eval/agent-context-workspace.json")).unwrap(),
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

#[test]
fn tagged_intent_sdk_verifies_multiple_evidence_roots_and_checked_delivery() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::create_dir_all(workspace.path().join(".fr")).unwrap();
    fs::create_dir_all(workspace.path().join("artifacts")).unwrap();
    fs::write(workspace.path().join("src/lib.rs"),
        "pub fn render(value: &str) -> String { value.to_owned() }\npub fn caller() -> String { render(\"ok\") }\n").unwrap();
    fs::write(
        workspace.path().join(".fr/checks.json"),
        serde_json::to_vec(&serde_json::json!({
        "schema":1,"checks":[{"name":"syntax","argv":["true"],"cwd":".",
        "timeout_seconds":10,"covers":["syntax"]}]}))
        .unwrap(),
    )
    .unwrap();
    let script = include_str!("fixtures/tagged_intent_sdk.py");
    let output = python()
        .args(["-c", script])
        .arg(workspace.path())
        .arg(env!("CARGO_BIN_EXE_fr"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["status"], "applied");
    assert!(workspace.path().join("artifacts/tagged.patch").exists());
}

#[test]
fn python_general_intent_review_policy_matches_rust_exhaustively() {
    let script = r#"# => Python general intent kernel corpus
from fr_ir.intent_actions import _action_purpose_allowed, _review_complete, _review_mode
for purpose in range(7):
    for operation in range(12):
        print(str(_action_purpose_allowed(purpose, operation)).lower())
        for bits in range(32):
            b = lambda i: bool(bits & (1 << i))
            print(_review_mode(purpose, operation, b(4), b(3), b(2), b(1), b(0)))
for targets in (0,1,2,32,33,2**64-1):
    for evidence in (0,1,31,32,2**64-1):
        for bits in range(128):
            b = lambda i: bool(bits & (1 << i))
            print(str(_review_complete(targets, evidence, b(6), b(5), b(4), b(3), b(2), b(1), b(0))).lower())
"#;
    let output = python().args(["-c", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for purpose in 0..7 {
        for operation in 0..12 {
            expected.push(
                fun_refactor::project::intent_action_purpose_allowed(purpose, operation)
                    .to_string(),
            );
            for bits in 0..32 {
                let b = |index| bits & (1 << index) != 0;
                expected.push(
                    fun_refactor::project::intent_review_mode(
                        purpose,
                        operation,
                        b(4),
                        b(3),
                        b(2),
                        b(1),
                        b(0),
                    )
                    .to_string(),
                );
            }
        }
    }
    for targets in [0, 1, 2, 32, 33, usize::MAX] {
        for evidence in [0, 1, 31, 32, usize::MAX] {
            for bits in 0..128 {
                let b = |index| bits & (1 << index) != 0;
                expected.push(
                    fun_refactor::project::intent_review_complete(
                        targets,
                        evidence,
                        b(6),
                        b(5),
                        b(4),
                        b(3),
                        b(2),
                        b(1),
                        b(0),
                    )
                    .to_string(),
                );
            }
        }
    }
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}
