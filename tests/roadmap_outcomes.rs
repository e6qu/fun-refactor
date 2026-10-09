use std::process::Command;

#[test]
fn roadmap_reports_and_evidence_classification_stay_honest() {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("agent-eval/roadmap.json")).unwrap();
    let repository_evidence = catalog["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "repository-tasks")
        .unwrap()["path"]
        .as_str()
        .unwrap();
    for args in [
        vec!["tools/roadmap-status.py", "--check"],
        vec!["tests/agent-eval/test_roadmap_status.py"],
        vec!["tests/agent-eval/test_python_repository_acceptance.py"],
        vec![
            "tools/python-repository-acceptance.py",
            "--audit",
            repository_evidence,
        ],
    ] {
        let output = Command::new("python3").args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
