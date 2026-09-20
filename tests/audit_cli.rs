use serde_json::{json, Value};
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn audit(section: &str) -> Value {
    let output = Command::new(FR)
        .args(["--json", "audit", section])
        .output()
        .expect("fr audit should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("audit output is JSON")
}

#[test]
fn summary_is_small_and_progressively_reveals_every_detail_section() {
    let summary = audit("summary");
    assert_eq!(summary["schema"], "fr-completion-audit-1");
    assert!(serde_json::to_vec(&summary).unwrap().len() < 4096);
    assert_eq!(summary["report"]["counts"]["parser_languages"], 19);
    assert_eq!(summary["report"]["counts"]["capability_cells"], 456);
    assert_eq!(summary["report"]["counts"]["workflow_routes"], 11);
    assert_eq!(summary["report"]["counts"]["application_cells"], 100);

    let sections = summary["report"]["reveal"].as_array().unwrap();
    assert_eq!(sections.len(), 7);
    for disclosure in sections {
        let section = disclosure["section"].as_str().unwrap();
        let detail = audit(section);
        assert_eq!(detail["section"], section);
        assert!(detail["report"].is_object());
        assert_eq!(detail["object_root"].as_str().unwrap().len(), 64);
    }
}

#[test]
fn trust_sections_keep_support_tests_and_proofs_distinct() {
    let workflows = audit("workflows");
    assert_eq!(
        workflows["report"]["validation_state"],
        "Acceptance targets name executable tests. This report does not claim that an unrun test passed."
    );
    for route in workflows["report"]["routes"].as_array().unwrap() {
        assert!(!route["predicate"].as_str().unwrap().is_empty());
        assert!(route["acceptance_target"].as_str().unwrap().contains("::"));
    }
    assert_eq!(workflows["report"]["live_evaluation"]["sessions"], 2);
    assert_eq!(workflows["report"]["live_evaluation"]["families"], 7);
    assert_eq!(
        workflows["report"]["live_evaluation"]["model"],
        "gpt-5.6-luna"
    );
    assert_eq!(
        workflows["report"]["matched_evaluation"]["arms"],
        json!(["fr", "files"])
    );
    assert_eq!(workflows["report"]["matched_evaluation"]["families"], 7);

    let proofs = audit("proofs");
    let classes = proofs["report"]["evidence_classes"].as_array().unwrap();
    assert!(classes.iter().any(|row| row["name"] == "model-theorem"));
    assert!(classes
        .iter()
        .any(|row| row["name"] == "implementation-correspondence"));

    let boundaries = audit("boundaries");
    assert_eq!(
        boundaries["report"]["analysis"][0]["status"],
        "inherent-static-boundary"
    );
}

#[test]
fn framework_reader_boundaries_match_application_report() {
    let frameworks = audit("frameworks");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    let output = Command::new(FR)
        .args(["--json", "project", "application"])
        .current_dir(dir.path())
        .output()
        .expect("application report should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let application: Value = serde_json::from_slice(&output.stdout).unwrap();
    let boundary = "request validation outside each HTTP adapter's documented subset";
    assert!(frameworks["report"]["excluded"]
        .as_array()
        .unwrap()
        .contains(&json!(boundary)));

    for source in ["nextjs", "fastapi", "express", "go-net-http"] {
        let row = application["adapters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["source"] == source)
            .unwrap();
        assert!(
            row["reader_features"]
                .as_array()
                .unwrap()
                .contains(&json!("validated-json-route")),
            "{source}"
        );
        assert!(
            row["excluded"]
                .as_array()
                .unwrap()
                .contains(&json!(boundary)),
            "{source}"
        );
        let cell = frameworks["report"]["cells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|cell| {
                cell["source"] == source
                    && cell["target"] == "react"
                    && cell["feature"] == "validated-json-route"
            })
            .unwrap();
        assert_eq!(cell["source_reader"], true, "{source}");
    }

    let react = application["adapters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["source"] == "react")
        .unwrap();
    assert!(!react["excluded"]
        .as_array()
        .unwrap()
        .contains(&json!(boundary)));
}
