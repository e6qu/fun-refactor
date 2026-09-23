use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn analyze(root: &Path, name: &str, steps: &str) -> Value {
    let found = run(root, &["project", "find", name]);
    let handle = found["rows"][0][0].as_str().unwrap();
    run(
        root,
        &[
            "project",
            "dataflow",
            handle,
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--steps",
            steps,
            "--bytes",
            "1048576",
        ],
    )
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        include_str!("agent-eval/flow-fixed-point/subject.py"),
    )
    .unwrap();
    fs::write(
        dir.path().join("rules.json"),
        r#"{"version":"fixture-1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    dir
}

#[test]
fn converges_past_one_iteration_and_exposes_exact_cyclic_graph() {
    let dir = fixture();
    let result = analyze(dir.path(), "delayed", "1024");
    assert_eq!(result["complete"], true, "{result}");
    let witnesses = result["witnesses"].as_array().unwrap();
    assert!(
        witnesses.iter().any(|w| w["trace"]["origin"]
            .as_str()
            .unwrap()
            .starts_with("fixture-1:source:")),
        "{result}"
    );
    let graph = &result["control_flow"]["delayed"];
    let nodes = graph["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["origins"]["status"], "absent");
    let source = fs::read_to_string(dir.path().join("subject.py")).unwrap();
    for node in nodes.iter().filter(|n| n["origins"]["status"] == "exact") {
        let occurrence = &node["origins"]["occurrence"];
        let span = &occurrence["location"]["span"];
        assert!(!source
            [span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize]
            .is_empty());
        assert_eq!(occurrence["revision"], result["revision"]);
    }
    let summary = result["summaries"].as_array().unwrap().last().unwrap();
    assert_eq!(summary["converged"], true);
    assert!(summary["block_visits"].as_u64().unwrap() > nodes.len() as u64);
    let output = Command::new("python3")
        .arg("tests/agent-eval/flow-fixed-point/oracle.py")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn overwrites_and_terminators_do_not_invent_sink_witnesses() {
    let dir = fixture();
    for name in ["overwritten", "stopped", "skipped", "raised"] {
        let result = analyze(dir.path(), name, "1024");
        assert_eq!(result["complete"], true, "{name}: {result}");
        assert!(
            result["witnesses"].as_array().unwrap().is_empty(),
            "{name}: {result}"
        );
    }
    let raised = analyze(dir.path(), "raised", "1024");
    assert!(raised["exceptional_returns"]
        .as_object()
        .unwrap()
        .contains_key("parameter:error"));
}

#[test]
fn exhausted_fixed_points_recursion_and_unbound_paths_stay_incomplete() {
    let dir = fixture();
    let bounded = analyze(dir.path(), "delayed", "20");
    assert_eq!(bounded["complete"], false);
    assert!(bounded["cutoffs"].to_string().contains("step-budget"));
    assert_eq!(analyze(dir.path(), "recursive", "1024")["complete"], false);
    fs::write(
        dir.path().join("subject.py"),
        "def maybe(flag):\n    if flag:\n        value = source()\n    sink(value)\n",
    )
    .unwrap();
    let result = analyze(dir.path(), "maybe", "1024");
    assert_eq!(result["complete"], false);
    assert!(result["cutoffs"].to_string().contains("bound:value"));
}

#[test]
fn elif_and_loop_else_edges_preserve_break_semantics() {
    let dir = fixture();
    fs::write(dir.path().join("subject.py"), "def choices(flag):\n    value = 0\n    if flag:\n        value = 1\n    elif flag:\n        value = source()\n    elif flag:\n        value = 2\n    else:\n        value = 3\n    sink(value)\ndef breaks(flag):\n    value = 0\n    while flag:\n        value = source()\n        break\n    else:\n        sink(value)\n").unwrap();
    let result = analyze(dir.path(), "choices", "1024");
    assert_eq!(result["complete"], true, "{result}");
    assert!(!result["witnesses"].as_array().unwrap().is_empty());
    let result = analyze(dir.path(), "breaks", "1024");
    assert_eq!(result["complete"], true, "{result}");
    assert!(
        result["witnesses"].as_array().unwrap().is_empty(),
        "{result}"
    );
}

#[test]
fn retained_analysis_checks_content_analyzer_and_current_source_before_reuse() {
    let dir = fixture();
    let record = analyze(dir.path(), "delayed", "1024");
    let file = tempfile::NamedTempFile::new().unwrap();
    let invoke = |record: &Value, digest: &str| {
        fs::write(file.path(), serde_json::to_vec(record).unwrap()).unwrap();
        let found = run(dir.path(), &["project", "find", "delayed"]);
        Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(dir.path())
            .args([
                "project",
                "dataflow",
                found["rows"][0][0].as_str().unwrap(),
                "--rules",
                dir.path().join("rules.json").to_str().unwrap(),
                "--steps",
                "1024",
                "--bytes",
                "1048576",
                "--reuse",
                file.path().to_str().unwrap(),
                "--reuse-digest",
                digest,
            ])
            .output()
            .unwrap()
    };
    let digest = |record: &Value| hex::encode(Sha256::digest(serde_json::to_vec(record).unwrap()));
    let original_digest = digest(&record);
    let output = invoke(&record, &original_digest);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["execution"]["kind"],
        "retained"
    );
    let mut tampered = record.clone();
    tampered["complete"] = Value::Bool(false);
    assert!(!invoke(&tampered, &original_digest).status.success());
    let mut old_analyzer = record.clone();
    old_analyzer["inputs"]["analyzer"] = Value::String("previous-analyzer".into());
    let output = invoke(&old_analyzer, &digest(&old_analyzer));
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["execution"]["reason"],
        "retained-result-not-reusable"
    );
    let path = dir.path().join("subject.py");
    let source = fs::read_to_string(&path).unwrap();
    fs::write(path, source.replace("first = source()", "first = 0")).unwrap();
    let output = invoke(&record, &original_digest);
    assert!(output.status.success());
    let refreshed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(refreshed["execution"]["kind"], "analyzed");
    assert!(refreshed["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn unsupported_exception_handlers_and_helper_throws_keep_explicit_boundaries() {
    let dir = fixture();
    fs::write(dir.path().join("subject.py"), "def throwing(error):\n    raise error\ndef caller(error):\n    throwing(error)\n    sink(source())\ndef handled(error):\n    try:\n        raise error\n    except:\n        sink(source())\n").unwrap();
    for name in ["caller", "handled"] {
        assert_eq!(analyze(dir.path(), name, "1024")["complete"], false);
    }
}

#[test]
fn later_local_assignment_prevents_an_external_rule_claim() {
    let dir = fixture();
    fs::write(
        dir.path().join("subject.py"),
        "def shadowed():\n    sink(source())\n    source = 1\n",
    )
    .unwrap();
    let report = analyze(dir.path(), "shadowed", "1024");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .to_string()
        .contains("ambiguous-call:source"));
    assert!(report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn retained_flow_acceptance_is_source_bound_and_consistent() {
    let output = Command::new("python3")
        .args([
            "tools/flow-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-23-flow-fixed-point/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn annotations_and_unreachable_scope_bindings_never_panic_or_claim_complete() {
    let dir = fixture();
    fs::write(dir.path().join("subject.py"), "def annotated():\n    value: int\n    return 1\ndef delayed_binding():\n    return source()\n    def source():\n        return 1\ndef return_annotation() -> dynamic():\n    return 1\n").unwrap();
    for name in ["annotated", "delayed_binding", "return_annotation"] {
        assert_eq!(analyze(dir.path(), name, "1024")["complete"], false);
    }
    fs::write(dir.path().join("subject.py"), "def walrus():\n    return source()\n    (source := 1)\ndef augmented():\n    return source()\n    source += 1\n").unwrap();
    for name in ["walrus", "augmented"] {
        assert_eq!(analyze(dir.path(), name, "1024")["complete"], false);
    }
}
