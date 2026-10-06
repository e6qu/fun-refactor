use std::process::Command;

#[test]
fn roadmap_reports_and_evidence_classification_stay_honest() {
    for args in [
        vec!["tools/roadmap-status.py", "--check"],
        vec!["tests/agent-eval/test_roadmap_status.py"],
        vec!["tests/agent-eval/test_python_repository_acceptance.py"],
        vec![
            "tools/python-repository-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-10-06-explore-budgets-python-repositories",
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
