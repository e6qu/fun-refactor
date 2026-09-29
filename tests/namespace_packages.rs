use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn report(root: &Path) -> Value {
    let invoke = |args: &[&str]| {
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
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let found = invoke(&["project", "find", "entry"]);
    invoke(&[
        "project",
        "dataflow",
        found["rows"][0][0].as_str().unwrap(),
        "--summaries",
        "--imports",
        "--rules",
        root.join("rules.json").to_str().unwrap(),
        "--steps",
        "4096",
        "--bytes",
        "1048576",
    ])
}

#[test]
fn namespace_resolution_retains_directories_and_initializer_transitions() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("ns/deep")).unwrap();
    fs::write(
        root.path().join("ns/deep/helper.py"),
        "def identity(value):\n    return value\n",
    )
    .unwrap();
    fs::write(
        root.path().join("app.py"),
        "from ns.deep import helper\ndef entry():\n    return sink(helper.identity(source()))\n",
    )
    .unwrap();
    fs::write(
        root.path().join("rules.json"),
        r#"{"version":"ns-1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let before = report(root.path());
    assert_eq!(before["complete"], true, "{before}");
    assert_eq!(
        before["inputs"]["modules"]["namespaces"],
        serde_json::json!(["ns", "ns/deep"])
    );
    assert!(!before["witnesses"].as_array().unwrap().is_empty());
    let lookup = &before["inputs"]["modules"]["lookups"][0];
    assert_eq!(lookup["target"], "ns/deep");
    assert_eq!(lookup["candidates"]["ns"]["status"], "directory");
    assert_eq!(lookup["candidates"]["ns/__init__.py"]["status"], "missing");
    fs::write(
        root.path().join("ns/__init__.py"),
        "# regular package now\n",
    )
    .unwrap();
    let regular = report(root.path());
    assert_eq!(regular["complete"], true);
    assert_eq!(
        regular["inputs"]["modules"]["namespaces"],
        serde_json::json!(["ns/deep"])
    );
    assert_ne!(regular["input_digest"], before["input_digest"]);
    fs::write(
        root.path().join("ns/deep/helper.py"),
        "def identity(value):\n    return 0\n",
    )
    .unwrap();
    let repaired = report(root.path());
    assert_eq!(repaired["complete"], true);
    assert!(repaired["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn namespace_packages_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/namespace-packages-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-30-assignments-namespace-packages/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
