use sha2::{Digest, Sha256};
use std::process::Command;

fn collect(script: &str) {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new("python3")
        .args([
            "-B",
            script,
            "collect",
            "--binary",
            env!("CARGO_BIN_EXE_fr"),
            "--output",
        ])
        .arg(directory.path().join("result"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bounded_translations_match_independent_integer_arithmetic() {
    collect("tools/checked-translation.py");
}

#[test]
fn effect_locations_and_model_limits_match_independent_python_executions() {
    collect("tools/python-effects.py");
}

#[test]
fn plain_text_matches_python_and_unknown_receivers_stay_incomplete() {
    collect("tools/python-text-flow.py");
}

#[test]
fn saved_flow_cannot_turn_a_scalar_answer_into_an_exception_safety_claim() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("subject.py"),
        "def entry():\n    return sink(source() // 2)\n",
    )
    .unwrap();
    let rules = project.join("rules.json");
    std::fs::write(
        &rules,
        r#"{"version":"effects","sources":["source"],"sinks":["sink"],"propagators":[],"sanitizers":{}}"#,
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(&project)
            .args(args)
            .output()
            .unwrap()
    };
    let found = run(&["project", "find", "entry"]);
    assert!(found.status.success());
    let found: serde_json::Value = serde_json::from_slice(&found.stdout).unwrap();
    let arguments = [
        "project",
        "dataflow",
        found["rows"][0][0].as_str().unwrap(),
        "--summaries",
        "--rules",
        rules.to_str().unwrap(),
        "--steps",
        "4096",
        "--depth",
        "8",
        "--bytes",
        "262144",
    ];
    let analyzed = run(&arguments);
    assert!(analyzed.status.success());
    let mut report: serde_json::Value = serde_json::from_slice(&analyzed.stdout).unwrap();
    let saved = directory.path().join("answer.json");
    let encoded = serde_json::to_vec(&report).unwrap();
    let digest = hex::encode(Sha256::digest(&encoded));
    std::fs::write(&saved, encoded).unwrap();
    let mut reuse = arguments.to_vec();
    reuse.extend([
        "--reuse",
        saved.to_str().unwrap(),
        "--reuse-digest",
        &digest,
    ]);
    let current = run(&reuse);
    assert!(current.status.success(), "{current:?}");
    report["runtime_effects"]["exception_safety_established"] = serde_json::json!(true);
    let encoded = serde_json::to_vec(&report).unwrap();
    let forged_digest = hex::encode(Sha256::digest(&encoded));
    std::fs::write(&saved, encoded).unwrap();
    reuse.pop();
    reuse.push(&forged_digest);
    let forged = run(&reuse);
    assert!(!forged.status.success());
    let detail = format!(
        "{}{}",
        String::from_utf8_lossy(&forged.stdout),
        String::from_utf8_lossy(&forged.stderr)
    );
    assert!(detail.contains("runtime effect contract"), "{detail}");
}
