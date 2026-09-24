use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> Value {
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
    serde_json::from_slice(&output.stdout).unwrap()
}

fn detail(root: &Path) -> Value {
    let found = run(root, &["project", "find", "entry"]);
    let page = run(
        root,
        &[
            "project",
            "flow-facts",
            found["rows"][0][0].as_str().unwrap(),
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--steps",
            "4096",
            "--depth",
            "16",
            "--limit",
            "64",
            "--evidence-limit",
            "64",
            "--bytes",
            "1048576",
        ],
    );
    let fact = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["core"]["kind"] == "witness")
        .unwrap();
    let args: Vec<_> = fact["follow"]["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    run(root, &args)
}

#[test]
fn semantic_queries_stop_at_the_page_budget_and_preserve_follow_actions() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = "def helper0(value):\n    return value\n".to_string();
    for index in 1..11 {
        source.push_str(&format!(
            "def helper{index}(value):\n    return helper{}(value)\n",
            index - 1
        ));
    }
    source.push_str("def entry():\n    sink(helper10(source()))\n");
    fs::write(dir.path().join("subject.py"), source).unwrap();
    fs::write(
        dir.path().join("rules.json"),
        r#"{"version":"v1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let report = detail(dir.path());
    assert_eq!(report["analysis"]["complete"], true, "{report}");
    assert_eq!(report["semantic_queries"], 8);
    let points = report["evidence"].as_array().unwrap();
    assert!(points
        .iter()
        .any(|point| point["mapping"]["status"] == "mapped"));
    let gaps: Vec<_> = points
        .iter()
        .filter(|point| point["mapping"]["status"] == "incomplete")
        .collect();
    assert!(!gaps.is_empty(), "{report}");
    for point in gaps {
        assert_eq!(point["mapping"]["complete"], false);
        assert_eq!(point["mapping"]["follow"]["arguments"][1], "semantic");
        assert_eq!(point["source"]["arguments"][1], "show");
    }
}

#[test]
fn truncated_origin_pages_cannot_establish_absent_mappings() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = "def entry():\n".to_string();
    for index in 0..150 {
        source.push_str(&format!("    value{index} = {index}\n"));
    }
    source.push_str("    sink(source())\n");
    fs::write(dir.path().join("subject.py"), source).unwrap();
    fs::write(
        dir.path().join("rules.json"),
        r#"{"version":"v1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let report = detail(dir.path());
    assert_eq!(report["complete"], true);
    for point in report["evidence"].as_array().unwrap() {
        assert_eq!(point["mapping"]["status"], "incomplete", "{point}");
        assert!(
            point["mapping"]["origin_page"]["remaining"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(point["mapping"]["continuation"].is_object());
    }
}

#[test]
fn fact_coverage_matches_every_lean_model_case() {
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
        .args(["exe", "fr-investigation-kernel", "flow-fact-coverage"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(lean.status.success());
    let mut expected = Vec::new();
    for analysis in [false, true] {
        for from_start in [false, true] {
            for no_remaining in [false, true] {
                expected.push(
                    fun_refactor::project::fact_catalogue_complete(
                        analysis,
                        from_start,
                        no_remaining,
                    )
                    .to_string(),
                );
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
fn retained_fact_acceptance_matches_inputs_and_oracles() {
    let output = Command::new("python3")
        .args([
            "tools/flow-facts-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-24-flow-facts/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
