use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
fn required_parameters_bind_by_kind_and_name() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call, complete, witness) in [
        ("left, right", "choose(right=0, left=source())", true, true),
        ("left, right", "choose(left=0, right=source())", true, false),
        ("left, /, *, right", "choose(source(), right=0)", true, true),
        (
            "*, left, right",
            "choose(right=0, left=source())",
            true,
            true,
        ),
        (
            "left, /, right",
            "choose(left=source(), right=0)",
            false,
            false,
        ),
        ("left, *, right", "choose(source(), 0)", false, false),
        (
            "left, right",
            "choose(source(), left=0, right=0)",
            false,
            false,
        ),
        ("left, right", "choose(sink(source()))", false, true),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(
            report["complete"], complete,
            "{parameters}: {call}: {}",
            report["cutoffs"]
        );
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
        assert_eq!(report["semantics"], "python-scalar-summaries-4");
        assert_eq!(report["call_binding"], report["inputs"]["call_binding"]);
    }
}

#[test]
fn retained_call_contract_cannot_be_forged_with_a_new_digest() {
    let root = tempfile::tempdir().unwrap();
    let (target, original) = analyze(root.path(), "left", "choose(left=source())");
    for field in ["semantics", "call_binding"] {
        let mut report = original.clone();
        report[field] = if field == "semantics" {
            json!("python-scalar-summaries-2")
        } else {
            json!({"schema":"fr-call-binding-1", "implicit_exceptions":true})
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        let digest = hex::encode(Sha256::digest(&bytes));
        let retained = tempfile::NamedTempFile::new().unwrap();
        fs::write(retained.path(), &bytes).unwrap();
        let output = run(
            root.path(),
            &[
                "project",
                "dataflow",
                &target,
                "--summaries",
                "--rules",
                root.path().join("rules.json").to_str().unwrap(),
                "--steps",
                "4096",
                "--bytes",
                "1048576",
                "--reuse",
                retained.path().to_str().unwrap(),
                "--reuse-digest",
                &digest,
            ],
        );
        assert!(!output.status.success(), "forged {field} admitted");
        assert!(String::from_utf8_lossy(&output.stderr).contains("inconsistent"));
    }
}

#[test]
fn call_binding_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/call-binding-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-29-calls-call-binding/result.json",
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
