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
fn semantic_selection_refuses_ambiguous_declarations_before_queries_or_edits() {
    let cases = [
        ("def repeated(x):\n    return 1\ndef repeated(y):\n    return 2\n", "repeated", true),
        ("def repeated(x):\n    return 1\n@decorator\ndef repeated(y):\n    return 2\n", "repeated", true),
        ("class Group:\n    def repeated(self):\n        return 1\n    def repeated(self):\n        return 2\n", "repeated", true),
        ("class Group:\n    def repeated(self):\n        return 1\nclass Group:\n    def other(self):\n        return 2\n", "repeated", true),
        ("class Group:\n    value = 1\nclass Group:\n    value = 2\n", "Group", false),
    ];
    for (source, original_name, callable) in cases {
        for moved in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = if moved {
                "nested/subject.py"
            } else {
                "subject.py"
            };
            let name = if moved { "renamed" } else { original_name };
            let source = if moved {
                format!("# moved π\n\n{}", source.replace(original_name, name))
            } else {
                source.to_owned()
            };
            let file = dir.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, &source).unwrap();
            fs::write(
                dir.path().join("body.json"),
                r#"{"schema":"fr-semantic-body-1","body":[]}"#,
            )
            .unwrap();
            let found = report(dir.path(), &["project", "find", name]);
            let rows = found["rows"].as_array().unwrap();
            assert!(!rows.is_empty(), "{source}");
            for row in rows {
                let handle = row[0].as_str().unwrap();
                for flags in [
                    vec!["--body", "--origins"],
                    vec!["--body", "--pointers"],
                    vec!["--body", "--locators"],
                ] {
                    let mut args = vec!["project", "semantic", handle];
                    args.extend(flags);
                    let result = output(dir.path(), &args);
                    assert!(!result.status.success(), "{source}");
                    assert!(String::from_utf8_lossy(&result.stderr)
                        .contains("no exact semantic IR item"));
                }
                if callable {
                    let result = output(
                        dir.path(),
                        &[
                            "author",
                            "replace-body-semantic",
                            handle,
                            "--from",
                            "body.json",
                            "--write",
                        ],
                    );
                    assert!(!result.status.success(), "{source}");
                    assert!(String::from_utf8_lossy(&result.stderr)
                        .contains("no exact semantic function model"));
                    assert_eq!(fs::read_to_string(&file).unwrap(), source);
                }
            }
            let result = report(dir.path(), &["project", "semantic", path, "--body"]);
            assert_eq!(result["selection"]["kind"], "file");
        }
    }
}

#[test]
fn semantic_selection_keeps_distinct_qualified_methods_available() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "class First:\n    def repeated(self):\n        return 1\nclass Second:\n    def repeated(self):\n        return 2\n",
    )
    .unwrap();
    let found = report(dir.path(), &["project", "find", "repeated"]);
    let handles = found["rows"].as_array().unwrap();
    assert_eq!(handles.len(), 2);
    for (i, row) in handles.iter().enumerate() {
        let result = report(
            dir.path(),
            &["project", "semantic", row[0].as_str().unwrap(), "--body"],
        );
        assert_eq!(
            result["model"]["items"][0]["value"]["body"][0]["value"]["value"],
            (i + 1).to_string()
        );
    }
}

#[test]
fn normalized_bodies_never_invent_source_correspondence() {
    let dir = tempfile::tempdir().unwrap();
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
            "tools/compiler-profile.py",
            "--audit",
            "tests/agent-eval/results/2026-10-10-compiler-profile/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
