use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn analyze(root: &Path, body: &str) -> (String, Value) {
    fs::write(root.join("subject.py"), format!("def entry():\n{body}\n")).unwrap();
    fs::write(
        root.join("rules.json"),
        r#"{"version":"assign-1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let found: Value =
        serde_json::from_slice(&run(root, &["project", "find", "entry"]).stdout).unwrap();
    let target = found["rows"][0][0].as_str().unwrap().to_owned();
    let result = run(
        root,
        &[
            "project",
            "dataflow",
            &target,
            "--summaries",
            "--rules",
            "rules.json",
            "--steps",
            "4096",
            "--bytes",
            "1048576",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    (target, serde_json::from_slice(&result.stdout).unwrap())
}

#[test]
fn parallel_assignments_preserve_reads_before_writes_and_duplicate_targets() {
    let root = tempfile::tempdir().unwrap();
    for (body, reaches) in [
        (
            "    # => swapped source\n    a, b = source(), 0\n    a, b = b, a\n    sink(b)",
            true,
        ),
        (
            "    # => swapped literal\n    a, b = source(), 0\n    a, b = b, a\n    sink(a)",
            false,
        ),
        (
            "    # => duplicate target\n    a, a = source(), 0\n    sink(a)",
            false,
        ),
        (
            "    # => chained value\n    a = b = source()\n    a = 0\n    sink(b)",
            true,
        ),
    ] {
        let (_, report) = analyze(root.path(), body);
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(report["semantics"], "python-scalar-summaries-5");
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), reaches);
        assert_eq!(
            report["assignment_control"],
            report["inputs"]["assignment_control"]
        );
    }
}

#[test]
fn recomputed_digest_does_not_admit_a_forged_assignment_contract() {
    let root = tempfile::tempdir().unwrap();
    let (target, mut report) = analyze(root.path(), "    a, b = source(), 0\n    sink(a)");
    report["assignment_control"]["binding"] = json!("write targets before reading values");
    let bytes = serde_json::to_vec(&report).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    let retained = tempfile::NamedTempFile::new().unwrap();
    fs::write(retained.path(), bytes).unwrap();
    let result = run(
        root.path(),
        &[
            "project",
            "dataflow",
            &target,
            "--summaries",
            "--rules",
            "rules.json",
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
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("inconsistent assignment control"));
}

#[test]
fn scalar_assignments_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/scalar-assignments-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-30-assignments-scalar-assignments/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
