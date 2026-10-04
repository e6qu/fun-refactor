use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: {error}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (success, report) = run(root, args);
    assert!(success, "{args:?}: {report}");
    report
}

fn follow(root: &Path, report: &Value, reason: &str) -> Value {
    let action = report["continuations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["reason"] == reason)
        .unwrap_or_else(|| panic!("missing {reason}: {report}"));
    let args = action["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| arg.as_str().unwrap())
        .collect::<Vec<_>>();
    ok(root, &args)
}

fn fixture(language: &str) -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    let comment = "κλμ".repeat(12);
    let (path, helper, header, line, footer) = match language {
        "python" => (
            "app.py",
            "def helper(value):\n    return value\n\n",
            "def investigate(value):\n",
            format!("    value = helper(value)  # {comment}\n"),
            "    return value",
        ),
        "rust" => (
            "app.rs",
            "fn helper(value: i32) -> i32 { value }\n\n",
            "fn investigate(mut value: i32) -> i32 {\n",
            format!("    value = helper(value); // {comment}\n"),
            "    value\n}",
        ),
        _ => unreachable!(),
    };
    let declaration = format!("{header}{}{footer}", line.repeat(60));
    fs::write(root.path().join(path), format!("{helper}{declaration}\n")).unwrap();
    (root, declaration)
}

fn initial(root: &Path, profile: &str) -> Value {
    let names = ok(root, &["project", "explore", "investigate"]);
    ok(
        root,
        &[
            "project",
            "explore",
            "investigate",
            "--mode",
            "behavior",
            "--target",
            names["rows"][0]["handle"].as_str().unwrap(),
            "--profile",
            profile,
        ],
    )
}

fn bytes(report: &Value) -> usize {
    serde_json::to_vec(report).unwrap().len() + 1
}

#[test]
fn source_pages_reconstruct_python_and_rust_without_repeating_relationships() {
    for language in ["python", "rust"] {
        for profile in ["compact", "expanded"] {
            let (root, source) = fixture(language);
            let first = initial(root.path(), profile);
            assert_eq!(first["view"], "both");
            let handle = first["declaration"]["node"]["handle"].as_str().unwrap();
            let mut page = first.clone();
            let mut collected = String::new();
            let mut count = 0;
            loop {
                assert_eq!(page["declaration"]["source"]["offset"], collected.len());
                collected.push_str(page["declaration"]["source"]["text"].as_str().unwrap());
                assert!(bytes(&page) <= page["profile"]["report_bytes"].as_u64().unwrap() as usize);
                if page["declaration"]["source"]["next_offset"].is_null() {
                    break;
                }
                page = follow(root.path(), &page, "more-source");
                count += 1;
                assert!(count < 10, "source continuation did not advance");
                assert_eq!(page["view"], "source");
                assert_eq!(page["revision"], first["revision"]);
                assert_eq!(page["coverage"], first["coverage"]);
                assert!(page.get("relationships").is_none());
                assert!(page["continuations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|action| { action["reason"] != "more-relationships" }));
                let offset = page["declaration"]["source"]["offset"].to_string();
                let combined = ok(
                    root.path(),
                    &[
                        "project",
                        "explore",
                        "investigate",
                        "--mode",
                        "behavior",
                        "--target",
                        handle,
                        "--profile",
                        profile,
                        "--offset",
                        &offset,
                    ],
                );
                assert_eq!(
                    page["declaration"]["source"],
                    combined["declaration"]["source"]
                );
                assert!(bytes(&page) < bytes(&combined));
            }
            assert!(count > 0);
            assert_eq!(collected, source, "{language}/{profile}");
        }
    }
}

#[test]
fn relationship_pages_preserve_every_row_without_repeating_source() {
    for language in ["python", "rust"] {
        for profile in ["compact", "expanded"] {
            let (root, _) = fixture(language);
            let first = initial(root.path(), profile);
            let handle = first["declaration"]["node"]["handle"].as_str().unwrap();
            let expected = ok(
                root.path(),
                &["project", "show", handle, "--relations", "--limit", "500"],
            );
            assert!(expected["relations"]["page"]["next"].is_null());
            let mut page = first.clone();
            let mut collected = Vec::new();
            let mut count = 0;
            loop {
                assert_eq!(page["relationships"]["page"]["before"], collected.len());
                collected.extend(
                    page["relationships"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned(),
                );
                assert!(bytes(&page) <= page["profile"]["report_bytes"].as_u64().unwrap() as usize);
                if page["relationships"]["page"]["next"].is_null() {
                    break;
                }
                let cursor = page["relationships"]["page"]["next"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                page = follow(root.path(), &page, "more-relationships");
                count += 1;
                assert!(count < 100, "relationship continuation did not advance");
                assert_eq!(page["view"], "relationships");
                assert_eq!(page["revision"], first["revision"]);
                assert_eq!(page["coverage"], first["coverage"]);
                assert!(page["declaration"].get("source").is_none());
                assert!(page["continuations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|action| { action["reason"] != "more-source" }));
                let combined = ok(
                    root.path(),
                    &[
                        "project",
                        "explore",
                        "investigate",
                        "--mode",
                        "behavior",
                        "--target",
                        handle,
                        "--profile",
                        profile,
                        "--relations-cursor",
                        &cursor,
                    ],
                );
                assert_eq!(page["relationships"], combined["relationships"]);
                assert!(bytes(&page) < bytes(&combined));
            }
            assert!(count > 0);
            assert_eq!(
                collected,
                *expected["relations"]["items"].as_array().unwrap()
            );
        }
    }
}

#[test]
fn expansion_keeps_the_selected_view_and_current_page_position() {
    let (root, _) = fixture("python");
    let first = initial(root.path(), "compact");
    for reason in ["more-source", "more-relationships"] {
        let page = follow(root.path(), &first, reason);
        let expanded = follow(root.path(), &page, "explicit-profile-expansion");
        assert_eq!(expanded["profile"]["name"], "expanded");
        assert_eq!(expanded["view"], page["view"]);
        assert_eq!(expanded["declaration"]["node"], page["declaration"]["node"]);
        if reason == "more-source" {
            assert_eq!(
                expanded["declaration"]["source"]["offset"],
                page["declaration"]["source"]["offset"]
            );
            assert!(expanded.get("relationships").is_none());
        } else {
            assert_eq!(
                expanded["relationships"]["page"]["before"],
                page["relationships"]["page"]["before"]
            );
            assert!(expanded["declaration"].get("source").is_none());
        }
    }
}

#[test]
fn focused_views_refuse_mixed_pagination_and_stale_handles() {
    let (root, _) = fixture("python");
    let first = initial(root.path(), "compact");
    let handle = first["declaration"]["node"]["handle"].as_str().unwrap();
    let cursor = first["relationships"]["page"]["next"].as_str().unwrap();
    for (view, flags) in [
        ("source", vec!["--relations-cursor", cursor]),
        ("relationships", vec!["--offset", "2048"]),
    ] {
        let mut args = vec![
            "project",
            "explore",
            "investigate",
            "--mode",
            "behavior",
            "--target",
            handle,
            "--view",
            view,
        ];
        args.extend(flags);
        let (success, report) = run(root.path(), &args);
        assert!(!success, "{report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unavailable"));
    }
    fs::write(
        root.path().join("app.py"),
        "def investigate(value):\n    return value + 1\n",
    )
    .unwrap();
    for view in ["both", "source", "relationships"] {
        let (success, report) = run(
            root.path(),
            &[
                "project",
                "explore",
                "investigate",
                "--mode",
                "behavior",
                "--target",
                handle,
                "--view",
                view,
            ],
        );
        assert!(!success, "{report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stale"));
    }
}

#[test]
fn discovery_links_preserve_short_scope_revision_and_containment() {
    let (root, _) = fixture("python");
    let names = ok(
        root.path(),
        &["project", "explore", "investigate", "--in", "app.py"],
    );
    let scope = names["root"].as_str().unwrap().rsplit(':').next().unwrap();
    let revision = names["revision"].as_str().unwrap();
    let scoped = ok(
        root.path(),
        &[
            "project",
            "explore",
            "investigate",
            "--in",
            scope,
            "--revision",
            revision,
        ],
    );
    let args = scoped["rows"][0]["next"]["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| arg.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(args.windows(2).any(|pair| pair == ["--in", scope]));
    assert!(args.windows(2).any(|pair| pair == ["--revision", revision]));
    let behavior = ok(root.path(), &args);
    for reason in ["more-source", "more-relationships"] {
        let page = follow(root.path(), &behavior, reason);
        assert_eq!(
            page["declaration"]["node"]["handle"],
            scoped["rows"][0]["handle"]
        );
    }
}

#[test]
fn focused_reads_keep_query_scope_and_utf8_validation() {
    let (root, source) = fixture("python");
    let first = initial(root.path(), "compact");
    let handle = first["declaration"]["node"]["handle"].as_str().unwrap();
    let names = ok(root.path(), &["project", "explore", "helper"]);
    let helper = names["rows"][0]["handle"].as_str().unwrap();
    for view in ["source", "relationships"] {
        let (success, report) = run(
            root.path(),
            &[
                "project",
                "explore",
                "investigate",
                "--mode",
                "behavior",
                "--target",
                handle,
                "--view",
                view,
                "--in",
                helper,
            ],
        );
        assert!(!success, "{report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("outside"));
        let (success, report) = run(
            root.path(),
            &["project", "explore", "investigate", "--view", view],
        );
        assert!(!success, "{report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("require --mode behavior"));
    }
    for offset in [source.find('κ').unwrap() + 1, source.len() + 1] {
        let offset = offset.to_string();
        let (success, report) = run(
            root.path(),
            &[
                "project",
                "explore",
                "investigate",
                "--mode",
                "behavior",
                "--target",
                handle,
                "--view",
                "source",
                "--offset",
                &offset,
            ],
        );
        assert!(!success, "{report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("UTF-8 boundary"));
    }
}

#[test]
fn bounded_batches_can_select_focused_views_from_prior_handles() {
    let (root, _) = fixture("python");
    let manifest = json!({"schema": "fr-project-batch-1", "requests": [
        {"id": "names", "arguments": ["explore", "investigate"]},
        {"id": "source", "arguments": ["explore", "investigate", "--mode", "behavior", "--view", "source", "--target", {"request": "names", "pointer": "/rows/0/handle"}]},
        {"id": "relationships", "arguments": ["explore", "investigate", "--mode", "behavior", "--view", "relationships", "--target", {"request": "names", "pointer": "/rows/0/handle"}]}
    ]});
    let path = root.path().join("batch.json");
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let batch = ok(
        root.path(),
        &[
            "project",
            "batch",
            "--from",
            path.to_str().unwrap(),
            "--profile",
            "compact",
        ],
    );
    let source = &batch["requests"][1]["report"];
    let relationships = &batch["requests"][2]["report"];
    assert_eq!(source["view"], "source");
    assert!(source.get("relationships").is_none());
    assert_eq!(relationships["view"], "relationships");
    assert!(relationships["declaration"].get("source").is_none());
    assert_eq!(
        source["declaration"]["node"],
        relationships["declaration"]["node"]
    );
}
