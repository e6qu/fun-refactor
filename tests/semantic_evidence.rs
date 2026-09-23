use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn output(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(root: &Path, args: &[&str]) -> Value {
    let output = output(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn origin_pages_retain_distinct_calls_combined_and_absent_origins() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        include_str!("agent-eval/semantic-evidence/subject.py"),
    )
    .unwrap();
    let query = [
        "project",
        "semantic",
        "subject.py",
        "--declaration",
        "total",
        "--body",
        "--origins",
        "--origin-limit",
        "64",
    ];
    let value = report(dir.path(), &query);
    let rows = value["origins"]["items"].as_array().unwrap();
    let calls: Vec<_> = rows.iter().filter(|row| row["kind"] == "call").collect();
    assert_eq!(calls.len(), 2);
    let oracle = Command::new("python3")
        .arg("tests/agent-eval/semantic-evidence/oracle.py")
        .output()
        .unwrap();
    assert!(oracle.status.success());
    let expected: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    for (i, call) in calls.iter().enumerate() {
        let span = &call["origins"]["occurrence"]["location"]["span"];
        assert_eq!(
            serde_json::json!([span["start"], span["end"]]),
            expected["calls"][i]
        );
    }
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    assert!(rows
        .iter()
        .any(|row| row["origins"]["status"] == "multiple"));
    assert!(rows
        .iter()
        .any(|row| row["kind"] == "null" && row["origins"]["status"] == "absent"));
    assert_eq!(value["origins"]["complete"], true);
    assert_eq!(value["origins"]["mutation_authority"], false);
}

#[test]
fn normalized_and_shadowed_bodies_never_invent_source_correspondence() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def repeated(x):\n    return 1\ndef repeated(y):\n    return 2\n",
    )
    .unwrap();
    let found = report(dir.path(), &["project", "find", "repeated"]);
    let handles = found["rows"].as_array().unwrap();
    assert_eq!(handles.len(), 2);
    let result = report(
        dir.path(),
        &[
            "project",
            "semantic",
            handles[1][0].as_str().unwrap(),
            "--body",
            "--origins",
        ],
    );
    assert!(result["origins"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["origins"]["status"] == "absent"));
    fs::write(
        dir.path().join("subject.py"),
        "def total(x):\n    return x % 3\n",
    )
    .unwrap();
    let result = report(
        dir.path(),
        &[
            "project",
            "semantic",
            "subject.py",
            "--declaration",
            "total",
            "--body",
            "--origins",
        ],
    );
    assert!(result["origins"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["origins"]["status"] == "absent"));
}

#[test]
fn omission_and_unknown_pointers_cannot_claim_complete_origins() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def total(x):\n    return x + 1\n",
    )
    .unwrap();
    let result = report(
        dir.path(),
        &[
            "project",
            "semantic",
            "subject.py",
            "--declaration",
            "total",
            "--body",
            "--origins",
            "--nodes",
            "1",
        ],
    );
    assert_eq!(result["origins"]["complete"], false);
    assert_eq!(result["origins"]["items"], serde_json::json!([]));
    let result = output(
        dir.path(),
        &[
            "project",
            "semantic",
            "subject.py",
            "--body",
            "--origins",
            "--origin-pointer",
            "/invented",
        ],
    );
    assert!(!result.status.success());
    let result = output(
        dir.path(),
        &["project", "semantic", "subject.py", "--origins"],
    );
    assert!(!result.status.success());
}

#[test]
fn checked_evidence_requires_all_input_classes_in_native_and_lean() {
    let built = Command::new("lake")
        .args(["build", "fr-investigation-kernel", "--wfail"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let lean = Command::new("lake")
        .args(["exe", "fr-investigation-kernel", "check-scope"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(lean.status.success());
    let mut expected = Vec::new();
    for workspace in [false, true] {
        for configuration in [false, true] {
            for sources in [false, true] {
                for toolchain in [false, true] {
                    expected.push(
                        fun_refactor::project::investigation::check_scope_covered(
                            workspace,
                            configuration,
                            sources,
                            toolchain,
                        )
                        .to_string(),
                    );
                }
            }
        }
    }
    assert_eq!(
        String::from_utf8(lean.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn retained_semantic_evidence_is_source_bound_and_internally_verified() {
    let output = Command::new("python3")
        .args([
            "tools/semantic-evidence-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-23-semantic-evidence/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
