use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn partial_retained_pages_never_claim_complete_correspondence() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("subject.py"),
        "def first():\n    return 1\n\ndef second():\n    return 2\n",
    )
    .unwrap();
    let capture = report(root.path(), &["project", "identities", "--limit", "1"]);
    assert_eq!(capture["complete"], false);
    let retained = tempfile::NamedTempFile::new().unwrap();
    fs::write(retained.path(), capture.to_string()).unwrap();
    let matched = report(
        root.path(),
        &[
            "project",
            "identities",
            "--from",
            retained.path().to_str().unwrap(),
        ],
    );
    assert_eq!(matched["input_complete"], false);
    assert_eq!(matched["complete"], false);
}

#[test]
fn retained_identity_digest_duplicate_and_revision_mismatches_refuse() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("subject.py"),
        "def target():\n    return 1\n",
    )
    .unwrap();
    let original = report(root.path(), &["project", "identities"]);
    let retained = tempfile::NamedTempFile::new().unwrap();
    let path = retained.path().to_str().unwrap();
    fs::write(path, original.to_string()).unwrap();
    assert!(!run(
        root.path(),
        &[
            "project",
            "identities",
            "--from",
            path,
            "--digest",
            &"0".repeat(64)
        ]
    )
    .status
    .success());
    for mutation in ["revision", "duplicate", "origin", "legacy"] {
        let mut value = original.clone();
        match mutation {
            "revision" => value["revision"] = json!("0".repeat(64)),
            "duplicate" => {
                value["items"] = json!([value["items"][0], value["items"][0]]);
                value["page"]["returned"] = json!(2);
                value["page"]["total"] = json!(2);
            }
            "origin" => {
                value["items"][0]["occurrence"]["id"] = json!(format!("fro1:{}", "0".repeat(64)))
            }
            _ => {
                value.as_object_mut().unwrap().remove("identity_contract");
            }
        }
        fs::write(path, value.to_string()).unwrap();
        assert!(
            !run(root.path(), &["project", "identities", "--from", path])
                .status
                .success(),
            "{mutation}"
        );
    }
}

#[test]
fn candidate_limits_and_scope_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let source = "def target():\n    return 1\n";
    fs::write(root.path().join("original.py"), source).unwrap();
    let original = report(root.path(), &["project", "identities"]);
    let retained = tempfile::NamedTempFile::new().unwrap();
    fs::write(retained.path(), original.to_string()).unwrap();
    for n in 0..5 {
        fs::write(root.path().join(format!("copy{n}.py")), source).unwrap();
    }
    let path = retained.path().to_str().unwrap();
    let clipped = report(
        root.path(),
        &["project", "identities", "--from", path, "--candidates", "2"],
    );
    assert_eq!(clipped["items"][0]["status"], "ambiguous");
    assert_eq!(clipped["items"][0]["candidate_count"], 6);
    assert_eq!(
        clipped["items"][0]["candidates"].as_array().unwrap().len(),
        2
    );
    assert_eq!(clipped["complete"], false);
    let selected = report(
        root.path(),
        &["project", "identities", "original.py", "--from", path],
    );
    assert_eq!(selected["items"][0]["status"], "matched");
    assert_eq!(selected["selection"], "original.py");
}

#[test]
fn identity_page_limits_refuse_invalid_budgets() {
    let root = tempfile::tempdir().unwrap();
    for (flag, value) in [
        ("--candidates", "0"),
        ("--candidates", "65"),
        ("--bytes", "4095"),
        ("--limit", "0"),
    ] {
        assert!(!run(root.path(), &["project", "identities", flag, value])
            .status
            .success());
    }
}

#[test]
fn correspondence_status_matches_lean_cases() {
    let built = Command::new("lake")
        .args(["build", "fr-correspondence-kernel", "--wfail"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new("lake")
        .args(["exe", "fr-correspondence-kernel"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected = (0..8)
        .flat_map(|count| {
            [false, true].into_iter().map(move |shared| {
                fun_refactor::project::correspondence::correspondence_status(count, shared)
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn retained_correspondence_acceptance_matches_its_inputs() {
    let output = Command::new("python3")
        .args([
            "tools/correspondence-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-24-resumable-correspondence/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
