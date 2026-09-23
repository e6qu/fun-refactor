use serde_json::Value;
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

fn analyze(root: &Path, name: &str, steps: &str, depth: &str) -> Value {
    let found = run(root, &["project", "find", name]);
    run(
        root,
        &[
            "project",
            "dataflow",
            found["rows"][0][0].as_str().unwrap(),
            "--summaries",
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--context",
            "html",
            "--steps",
            steps,
            "--depth",
            depth,
            "--bytes",
            "1048576",
        ],
    )
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        include_str!("agent-eval/recursive-flow/subject.py"),
    )
    .unwrap();
    fs::write(dir.path().join("rules.json"), r#"{"version":"recursive-1","sources":["source"],"sinks":["sink"],"sanitizers":{"clean":"html"}}"#).unwrap();
    dir
}

#[test]
fn direct_and_mutual_recursion_match_independent_runtime_and_source_oracles() {
    let dir = fixture();
    let output = Command::new("python3")
        .arg("tests/agent-eval/recursive-flow/oracle.py")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in [
        "positive",
        "mutual",
        "effects",
        "negative",
        "sanitized",
        "separate",
    ] {
        let report = analyze(dir.path(), name, "4096", "8");
        assert_eq!(report["complete"], true, "{name}: {report}");
        assert_eq!(report["function_summaries"]["converged"], true);
        let witnesses = report["witnesses"].as_array().unwrap();
        assert_eq!(
            !witnesses.is_empty(),
            oracle["runtime"][name][0].as_bool().unwrap(),
            "{name}"
        );
        for witness in witnesses {
            assert!(witness["trace"]["origin"]
                .as_str()
                .unwrap()
                .starts_with("recursive-1:source:"));
            for point in witness["trace"]["occurrences"].as_array().unwrap() {
                assert_eq!(point["revision"], report["revision"]);
                if matches!(
                    point["role"].as_str().unwrap(),
                    "source" | "sink" | "call-result" | "summary-call"
                ) {
                    let span = &point["location"]["span"];
                    assert!(oracle["call_spans"]
                        .as_array()
                        .unwrap()
                        .contains(&serde_json::json!([span["start"], span["end"]])));
                }
            }
        }
    }
}

#[test]
fn empty_returns_are_distinct_from_no_normal_return_and_raises_propagate() {
    let dir = fixture();
    for name in ["unreachable", "exceptional"] {
        let report = analyze(dir.path(), name, "4096", "8");
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(report["completion"]["normal_return"], false);
        assert!(report["witnesses"].as_array().unwrap().is_empty());
    }
    let raised = analyze(dir.path(), "exceptional", "4096", "8");
    assert_eq!(raised["completion"]["may_raise"], true);
    assert!(raised["exceptional_returns"]
        .as_object()
        .unwrap()
        .contains_key("parameter:error"));
    let erased = analyze(dir.path(), "negative", "4096", "8");
    assert_eq!(erased["completion"]["normal_return"], true);
    assert_eq!(
        erased["function_summaries"]["functions"]["erase"]["returns"],
        serde_json::json!({})
    );
}

#[test]
fn budgets_unknown_effects_and_ambiguous_targets_remain_incomplete() {
    let dir = fixture();
    for (name, steps, depth) in [
        ("positive", "1", "8"),
        ("positive", "4096", "1"),
        ("unknown", "4096", "8"),
        ("aliased", "4096", "8"),
    ] {
        assert_eq!(
            analyze(dir.path(), name, steps, depth)["complete"],
            false,
            "{name}"
        );
    }
    fs::write(dir.path().join("subject.py"), "def helper(value):\n    return value\ndef helper(value):\n    return 0\ndef entry():\n    sink(helper(source()))\n").unwrap();
    let report = analyze(dir.path(), "entry", "4096", "8");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .to_string()
        .contains("ambiguous-call:helper"));
}

#[test]
fn nested_calls_keep_evaluation_order_and_no_return_effects() {
    let dir = fixture();
    fs::write(dir.path().join("subject.py"), "def halt(value):\n    return halt(value)\ndef identity(value):\n    return value\ndef entry():\n    sink(identity(halt(source())))\n    sink(source())\n").unwrap();
    let report = analyze(dir.path(), "entry", "4096", "8");
    assert_eq!(report["complete"], true, "{report}");
    assert!(report["witnesses"].as_array().unwrap().is_empty());
    assert_eq!(report["completion"]["normal_return"], false);
}

#[test]
fn explicit_raise_and_normal_branches_retain_both_effects() {
    let dir = fixture();
    fs::write(dir.path().join("subject.py"), "def helper(value, flag):\n    if flag:\n        raise value\n    return value\ndef entry(flag):\n    sink(helper(source(), flag))\n").unwrap();
    let report = analyze(dir.path(), "entry", "4096", "8");
    assert_eq!(report["complete"], true, "{report}");
    assert_eq!(
        report["completion"],
        serde_json::json!({"normal_return":true,"may_raise":true})
    );
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(report["exceptional_returns"].as_object().unwrap().len(), 1);
}

#[test]
fn unsupported_short_circuit_and_raise_contracts_never_claim_absence() {
    let dir = fixture();
    for source in [
        "def helper():\n    return helper()\ndef entry(flag):\n    return flag or (helper() + 1)\n",
        "def entry():\n    raise\n",
        "def entry(error, cause):\n    raise error from cause\n",
        "def helper(value=1):\n    return value\ndef entry():\n    return helper()\n",
    ] {
        fs::write(dir.path().join("subject.py"), source).unwrap();
        assert_eq!(analyze(dir.path(), "entry", "4096", "8")["complete"], false);
    }
}

#[test]
fn the_function_budget_bounds_wide_call_graphs() {
    let dir = fixture();
    let mut source = String::new();
    for index in 0..65 {
        source.push_str(&format!("def helper{index}():\n    return 0\n"));
    }
    source.push_str("def entry(flag):\n");
    for index in 0..65 {
        source.push_str(&format!("    if flag:\n        helper{index}()\n"));
    }
    fs::write(dir.path().join("subject.py"), source).unwrap();
    let report = analyze(dir.path(), "entry", "4096", "8");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .to_string()
        .contains("summary-function-budget"));
    assert_eq!(
        report["function_summaries"]["functions"]
            .as_object()
            .unwrap()
            .len(),
        64
    );
}
