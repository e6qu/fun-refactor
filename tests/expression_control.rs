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

fn analyze(root: &Path, expression: &str) -> (String, Value) {
    fs::write(
        root.join("app.py"),
        format!("def stop(value):\n    raise value\ndef entry(flag):\n    return {expression}\n"),
    )
    .unwrap();
    fs::write(
        root.join("rules.json"),
        r#"{"version":"expressions-1","sources":["source"],"sinks":["sink"]}"#,
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
fn selected_expressions_skip_effects_and_keep_normal_alternatives() {
    let root = tempfile::tempdir().unwrap();
    for (expression, normal, raises, witness) in [
        ("False and sink(source())", true, false, false),
        ("True or sink(source())", true, false, false),
        (
            "(sink(source()) if True else stop(source()))",
            true,
            false,
            true,
        ),
        ("flag and stop(source())", true, true, false),
        (
            "(stop(source()) if flag else sink(source()))",
            true,
            true,
            true,
        ),
        (
            "(stop(source()) if flag else stop(source()))",
            false,
            true,
            false,
        ),
        (
            "(source() < 0 < stop(source()) < sink(source()))",
            true,
            true,
            false,
        ),
        (
            "(source() < stop(source()) < sink(source()))",
            false,
            true,
            false,
        ),
    ] {
        let (_, report) = analyze(root.path(), expression);
        assert_eq!(
            report["complete"], true,
            "{expression}: {}",
            report["cutoffs"]
        );
        assert_eq!(
            report["completion"]["normal_return"], normal,
            "{expression}"
        );
        assert_eq!(report["completion"]["may_raise"], raises, "{expression}");
        assert_eq!(
            !report["witnesses"].as_array().unwrap().is_empty(),
            witness,
            "{expression}"
        );
        assert_eq!(report["semantics"], "python-scalar-summaries-4");
        assert_eq!(
            report["expression_control"],
            report["inputs"]["expression_control"]
        );
    }
}

#[test]
fn a_recomputed_digest_cannot_change_the_retained_expression_contract() {
    let root = tempfile::tempdir().unwrap();
    let (target, original) = analyze(root.path(), "flag and sink(source())");
    for field in ["semantics", "expression_control"] {
        let mut report = original.clone();
        report[field] = if field == "semantics" {
            json!("python-scalar-summaries-1")
        } else {
            json!({"schema":"fr-expression-control-1", "path_feasibility":true})
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        let digest = hex::encode(Sha256::digest(&bytes));
        let retained = tempfile::NamedTempFile::new().unwrap();
        let path = retained.path();
        fs::write(path, &bytes).unwrap();
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
                path.to_str().unwrap(),
                "--reuse-digest",
                &digest,
            ],
        );
        assert!(!output.status.success(), "forged {field} admitted");
        assert!(String::from_utf8_lossy(&output.stderr).contains("inconsistent"));
    }
}

#[test]
fn expression_control_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/expression-control-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-29-namespaces-expression-control/result.json",
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
