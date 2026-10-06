use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn query(root: &Path, args: &[&str]) -> (bool, Value, usize) {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: {error}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value, output.stdout.len())
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (success, value, size) = query(root, args);
    assert!(success, "{args:?}: {value}");
    if let Some(budget) = value["profile"]["report_bytes"].as_u64() {
        assert!(size <= budget as usize, "{size} > {budget}: {args:?}");
    }
    value
}

fn arguments(report: &Value, reason: &str) -> Vec<String> {
    report["continuations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["reason"] == reason)
        .unwrap_or_else(|| panic!("missing {reason}: {report}"))["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

fn follow(root: &Path, report: &Value, reason: &str) -> Value {
    let args = arguments(report, reason);
    ok(root, &args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn deep_file(root: &Path, extension: &str) -> String {
    let scope = format!(
        "{}/app.{extension}",
        vec![format!("nested_{}", "x".repeat(70)); 10].join("/")
    );
    fs::create_dir_all(root.join(&scope).parent().unwrap()).unwrap();
    scope
}

#[test]
fn long_scope_names_pages_return_every_match_within_the_delivered_budget() {
    for language in ["py", "rs"] {
        let root = tempfile::tempdir().unwrap();
        let scope = deep_file(root.path(), language);
        let expected = (0..35)
            .map(|i| format!("investigate_{i}_{}", "x".repeat(140)))
            .collect::<BTreeSet<_>>();
        let source = expected
            .iter()
            .map(|name| {
                if language == "py" {
                    format!("def {name}(value):\n    return value\n\n")
                } else {
                    format!("fn {name}(value: i32) -> i32 {{ value }}\n")
                }
            })
            .collect::<String>();
        fs::write(root.path().join(&scope), &source).unwrap();
        for profile in ["compact", "expanded"] {
            let mut report = ok(
                root.path(),
                &[
                    "project",
                    "explore",
                    "investigate",
                    "--contains",
                    "--in",
                    &scope,
                    "--profile",
                    profile,
                ],
            );
            let mut seen = BTreeSet::new();
            let mut pages = 0;
            let mut reduced = false;
            loop {
                assert_eq!(
                    report["page"]["before"].as_u64().unwrap() as usize,
                    seen.len()
                );
                let rows = report["rows"].as_array().unwrap();
                assert!(!rows.is_empty());
                if report["page"]["remaining"].as_u64().unwrap() > 0 {
                    reduced |=
                        rows.len() < report["profile"]["row_limit"].as_u64().unwrap() as usize;
                }
                for row in rows {
                    assert!(
                        seen.insert(row["name"].as_str().unwrap().to_owned()),
                        "repeated row"
                    );
                    assert!(row.get("location").is_some());
                }
                pages += 1;
                assert!(pages <= 35);
                if report["page"]["next"].is_null() {
                    break;
                }
                report = follow(root.path(), &report, "more-name-matches");
            }
            assert!(reduced, "fixture must exercise byte-driven page sizing");
            assert_eq!(seen, expected);
        }
    }
}

#[test]
fn name_expansion_preserves_position_and_stale_continuations_refuse() {
    let root = tempfile::tempdir().unwrap();
    let scope = deep_file(root.path(), "py");
    fs::write(
        root.path().join(&scope),
        (0..35)
            .map(|i| format!("def investigate_{i}():\n    pass\n"))
            .collect::<String>(),
    )
    .unwrap();
    let first = ok(
        root.path(),
        &[
            "project",
            "explore",
            "investigate",
            "--contains",
            "--in",
            &scope,
        ],
    );
    let second = follow(root.path(), &first, "more-name-matches");
    let expanded = follow(root.path(), &second, "explicit-profile-expansion");
    assert_eq!(expanded["profile"]["name"], "expanded");
    assert_eq!(expanded["page"]["before"], second["page"]["before"]);
    assert_eq!(expanded["rows"][0]["handle"], second["rows"][0]["handle"]);
    assert_eq!(expanded["rows"][0]["name"], second["rows"][0]["name"]);
    let args = arguments(&second, "more-name-matches");
    fs::write(root.path().join(&scope), "def changed():\n    pass\n").unwrap();
    let (success, failure, _) = query(
        root.path(),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    assert!(!success);
    assert!(failure["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale"));
}

#[test]
fn escaped_source_and_long_relationships_reconstruct_with_smaller_pages() {
    let root = tempfile::tempdir().unwrap();
    let scope = deep_file(root.path(), "rs");
    let source = format!(
        "fn investigate() {{\n    let _payload = \"{}\";\n{}\n}}",
        "\u{0001}🙂".repeat(900),
        "    helper();\n".repeat(40)
    );
    fs::write(
        root.path().join(&scope),
        format!("fn helper() {{}}\n{source}\n"),
    )
    .unwrap();
    let names = ok(
        root.path(),
        &["project", "explore", "investigate", "--in", &scope],
    );
    let handle = names["rows"][0]["handle"].as_str().unwrap();
    let full = ok(
        root.path(),
        &["project", "show", handle, "--relations", "--limit", "500"],
    );
    for profile in ["compact", "expanded"] {
        let first = ok(
            root.path(),
            &[
                "project",
                "explore",
                "investigate",
                "--in",
                &scope,
                "--mode",
                "behavior",
                "--target",
                handle,
                "--profile",
                profile,
            ],
        );
        assert!(
            first["declaration"]["source"]["returned_bytes"]
                .as_u64()
                .unwrap()
                < first["profile"]["source_bytes"].as_u64().unwrap()
                || first["relationships"]["items"].as_array().unwrap().len()
                    < first["profile"]["relationship_limit"].as_u64().unwrap() as usize
        );
        let mut page = first.clone();
        let mut joined = String::new();
        let mut count = 0;
        loop {
            let part = &page["declaration"]["source"];
            assert_eq!(part["offset"].as_u64().unwrap() as usize, joined.len());
            assert!(!part["text"].as_str().unwrap().is_empty());
            joined.push_str(part["text"].as_str().unwrap());
            if part["next_offset"].is_null() {
                break;
            }
            page = follow(root.path(), &page, "more-source");
            assert!(page.get("relationships").is_none());
            count += 1;
            assert!(count < 100);
        }
        assert_eq!(joined, source);
        let mut page = first;
        let mut relationships = Vec::new();
        let mut count = 0;
        loop {
            assert_eq!(
                page["relationships"]["page"]["before"].as_u64().unwrap() as usize,
                relationships.len()
            );
            relationships.extend(
                page["relationships"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
            if page["relationships"]["page"]["next"].is_null() {
                break;
            }
            page = follow(root.path(), &page, "more-relationships");
            assert!(page["declaration"].get("source").is_none());
            count += 1;
            assert!(count < 100);
        }
        assert_eq!(
            relationships,
            *full["relations"]["items"].as_array().unwrap()
        );
    }
}
