use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn script(root: &Path) -> Command {
    let mut command = Command::new("python3");
    command.arg(root.join("tools/matched-agent-context.py"));
    command
}

fn step(root: &Path, session: &Path, request: Value) -> Value {
    let mut child = script(root)
        .arg("step")
        .arg(session)
        .arg("--request-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(child.stdin.take().unwrap(), &request).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fake_codex(session: &Path, usage: Value) {
    std::fs::write(
        session.join("codex-events.jsonl"),
        format!("{}\n", json!({"type":"turn.completed","usage":usage})),
    )
    .unwrap();
    std::fs::write(session.join("codex-stderr.txt"), "").unwrap();
    std::fs::write(session.join("codex-final.txt"), "complete\n").unwrap();
    std::fs::write(
        session.join("codex-run.json"),
        "{\"exit_code\":0,\"timed_out\":false}\n",
    )
    .unwrap();
}

#[test]
fn matched_harness_requires_equal_outcomes_and_complete_arm_specific_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temporary = tempfile::tempdir().unwrap();
    let sessions = temporary.path().join("sessions");
    let output = script(root)
        .arg("prepare")
        .arg(&sessions)
        .arg("--fr")
        .arg(FR)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fr = sessions.join("fr");
    let files = sessions.join("files");
    let program = include_str!("fixtures/matched_agent_sdk.py")
        .lines()
        .collect::<Vec<_>>();
    let sdk = step(root, &fr, json!({"tool":"sdk","program_lines":program}));
    assert_eq!(sdk["exit_code"], 0);
    assert_eq!(sdk["answers"]["workflows"].as_array().unwrap().len(), 7);
    step(
        root,
        &fr,
        json!({"tool":"finish","summary":"all seven SDK previews completed"}),
    );

    let config: Value =
        serde_json::from_slice(&std::fs::read(files.join("session.json")).unwrap()).unwrap();
    for path in [
        "src/lib.rs",
        "app/api/signals/route.ts",
        config["proof_path"].as_str().unwrap(),
    ] {
        let report = step(root, &files, json!({"tool":"read","path":path,"lines":200}));
        assert_eq!(report["path"], path);
    }
    let submitted = step(
        root,
        &files,
        json!({"tool":"submit","answers":config["expected_answers"]}),
    );
    assert_eq!(submitted["accepted"], true);
    step(
        root,
        &files,
        json!({"tool":"finish","summary":"all seven direct-file outcomes completed"}),
    );
    fake_codex(
        &fr,
        json!({"input_tokens":100,"cached_input_tokens":50,"output_tokens":10,
               "reasoning_output_tokens":1}),
    );
    fake_codex(
        &files,
        json!({"input_tokens":200,"cached_input_tokens":100,"output_tokens":20,
               "reasoning_output_tokens":2}),
    );
    let output = script(root).arg("score").arg(&sessions).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["context_saving_observed"], true);
    assert_eq!(report["results"][0]["coverage"]["route_coverage"], true);
    assert_eq!(report["results"][0]["coverage"]["action_coverage"], true);
    assert_eq!(report["results"][1]["coverage"]["read_coverage"], true);

    let refused = script(root).arg("run").arg(&sessions).output().unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--confirm-agent-spend"));

    let retained = temporary.path().join("retained");
    let output = script(root)
        .arg("record")
        .arg(&sessions)
        .arg(&retained)
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = script(root).arg("replay").arg(&retained).output().unwrap();
    assert!(output.status.success());
    let replay: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(replay["verified"], true);
    assert_eq!(replay["acceptance_evidence"], true);
}

#[test]
fn direct_submission_refuses_incorrect_outcomes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temporary = tempfile::tempdir().unwrap();
    let sessions = temporary.path().join("sessions");
    let output = script(root)
        .arg("prepare")
        .arg(&sessions)
        .arg("--fr")
        .arg(FR)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = step(
        root,
        &sessions.join("files"),
        json!({"tool":"submit","answers":{"schema":"fr-matched-outcomes-1","workflows":[]}}),
    );
    assert_eq!(report["accepted"], false);
    let denied = step(
        root,
        &sessions.join("fr"),
        json!({"tool":"sdk","program_lines":["open('src/lib.rs').read()"]}),
    );
    assert_eq!(denied["exit_code"], 1);
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("forbidden Python primitive"));
    let denied_import = step(
        root,
        &sessions.join("fr"),
        json!({"tool":"sdk","program_lines":["from sys import modules"]}),
    );
    assert_eq!(denied_import["exit_code"], 1);
    assert!(denied_import["error"]
        .as_str()
        .unwrap()
        .contains("imports outside the bounded guide surface"));
}

#[test]
fn retained_matched_cohorts_preserve_diagnostic_and_acceptance_classification() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, passed, acceptance) in [
        ("2026-09-17-matched-context-diagnostic-1", false, false),
        ("2026-09-17-matched-context-diagnostic-2", false, false),
        ("2026-09-17-matched-context-acceptance", true, true),
    ] {
        let output = script(root)
            .arg("replay")
            .arg(root.join("tests/agent-eval/results").join(name))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["verified"], true);
        assert_eq!(report["passed"], passed);
        assert_eq!(report["acceptance_evidence"], acceptance);
    }
}
