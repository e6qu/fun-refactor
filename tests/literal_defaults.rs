use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn analyze(root: &Path, parameters: &str, call: &str) -> (String, Value) {
    fs::write(
        root.join("app.py"),
        format!(
            "def choose({parameters}):\n    return left\ndef entry():\n    return sink({call})\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("rules.json"),
        r#"{"version":"calls-1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let found: Value =
        serde_json::from_slice(&run(root, &["project", "find", "entry"]).stdout).unwrap();
    let target = found["rows"][0][0].as_str().unwrap().to_owned();
    let output = run(
        root,
        &[
            "project",
            "dataflow",
            &target,
            "--summaries",
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--steps",
            "4096",
            "--bytes",
            "1048576",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (target, serde_json::from_slice(&output.stdout).unwrap())
}

#[test]
fn omitted_literals_and_explicit_values_have_distinct_origins() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call, witness) in [
        ("left=0", "choose()", false),
        ("left=0", "choose(source())", true),
        ("left=0, /, *, right=None", "choose(right=source())", false),
        ("left=-17", "choose()", false),
        ("left='café'", "choose()", false),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert_eq!(report["semantics"], "python-scalar-summaries-4");
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
        let first = &report["function_summaries"]["functions"]["choose"]["signature"][0];
        assert_eq!(first["name"], "left");
        assert_eq!(first["site"]["role"], "summary-parameter");
        assert_eq!(first["default"]["role"], "parameter-default");
        assert_eq!(first["default"]["revision"], report["revision"]);
    }
}

#[test]
fn definition_effects_and_required_slots_are_not_hidden_by_defaults() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call) in [
        ("left=source()", "choose(0)"),
        ("left=[]", "choose(0)"),
        ("left=0, *, right", "choose()"),
        ("left=0, /", "choose(left=source())"),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(report["complete"], false);
    }
}

#[test]
fn literal_defaults_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/literal-defaults-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-29-defaults-literal-defaults/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
