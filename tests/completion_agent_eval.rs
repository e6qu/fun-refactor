use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn script(root: &Path) -> Command {
    let mut command = Command::new("python3");
    command.arg(root.join("tools/completion-agent-eval.py"));
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

#[test]
fn prepared_agent_session_exposes_only_guide_follow_and_finish() {
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
    let session = sessions.join("fundamentals");
    let structured_prompt =
        std::fs::read_to_string(sessions.join("structured/prompt.txt")).unwrap();
    assert!(structured_prompt.contains("semantic-scalar"));
    assert!(structured_prompt.contains(r#"Request 1: {"tool":"guide","goal":{"#));
    assert!(structured_prompt.contains("template_lines"));
    assert!(!structured_prompt.contains(r#"\"purpose\":\"understand\""#));
    let guide = step(
        root,
        &session,
        json!({"tool":"guide","goal":{"schema":"fr-agent-goal-1","purpose":"understand",
            "selector":{"name":"calculate"},"operation":{"kind":"automatic"},
            "context":{"token_limit":4096,"packet_limit":65536}}}),
    );
    assert_eq!(guide["route"]["id"], "evidence");
    assert_eq!(guide["guide"], 0);
    let followed = step(
        root,
        &session,
        json!({"tool":"follow","guide":0,"action":0}),
    );
    assert_eq!(followed["response"]["schema"], "fr-agent-context-1");
    let finished = step(
        root,
        &session,
        json!({"tool":"finish","summary":"The guided evidence workflow completed."}),
    );
    assert_eq!(finished["finished"], true);
    assert!(!session.join("project/.fr-history").exists());

    let recipe = step(
        root,
        &session,
        json!({"tool":"guide","goal":{"schema":"fr-agent-goal-1","purpose":"change",
            "selector":{"name":"calculate"},"operation":{"kind":"recipe","verb":"rename"},
            "context":{"token_limit":4096,"packet_limit":65536}}}),
    );
    assert_eq!(recipe["route"]["id"], "recipe");
    let preview = step(
        root,
        &session,
        json!({"tool":"follow","guide":1,"action":0,
        "replace":{"<recipe-file>":"rename.recipe"},
        "file_lines":{"rename.recipe":[
            "schema 1",
            "recipe rename-calculate {",
            "  rename to \"compute\" where name=\"calculate\"",
            "  expect matched = 1",
            "  expect changed = 1 files",
            "  expect refusals = 0",
            "}"
        ]}}),
    );
    assert_eq!(preview["response"]["schema"], 1);

    let refused = script(root).arg("run").arg(&sessions).output().unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--confirm-agent-spend"));

    std::fs::write(
        session.join("codex-events.jsonl"),
        "{\"type\":\"turn.completed\",\"usage\":{}}\n",
    )
    .unwrap();
    std::fs::write(
        session.join("codex-stderr.txt"),
        "2026-09-17T00:00:00Z ERROR codex_core::tools::router: transient failure\n",
    )
    .unwrap();
    std::fs::write(session.join("codex-final.txt"), "finished\n").unwrap();
    std::fs::write(
        session.join("codex-run.json"),
        "{\"exit_code\":0,\"timed_out\":false}\n",
    )
    .unwrap();
    let structured = sessions.join("structured");
    let proof = step(
        root,
        &structured,
        json!({"tool":"guide","goal":{"schema":"fr-agent-goal-1","purpose":"prove",
            "selector":{"path":"specs/FrSpecs/SrcLibRsKeep.lean"},
            "operation":{"kind":"proof","obligation":"keepModel_identity"},
            "context":{"token_limit":4096,"packet_limit":65536}}}),
    );
    assert_eq!(proof["route"]["id"], "proof");
    step(
        root,
        &structured,
        json!({"tool":"follow","guide":0,"action":0}),
    );
    step(
        root,
        &structured,
        json!({"tool":"follow","guide":0,"action":1,
            "replace":{"<tactics-file>":"proof.lean"},"file_lines":{"proof.lean":["rfl"]}}),
    );
    let reused = step(
        root,
        &structured,
        json!({"tool":"follow","guide":0,"action":2,
            "replace":{"<tactics-file>":"proof.lean"}}),
    );
    assert_eq!(
        reused["arguments"].as_array().unwrap().last().unwrap(),
        "proof.lean"
    );
    assert_eq!(reused["response"]["schema"], 1);
    std::fs::write(
        structured.join("codex-events.jsonl"),
        "{\"type\":\"turn.completed\",\"usage\":{}}\n",
    )
    .unwrap();
    std::fs::write(structured.join("codex-stderr.txt"), "").unwrap();
    std::fs::write(structured.join("codex-final.txt"), "finished\n").unwrap();
    std::fs::write(
        structured.join("codex-run.json"),
        "{\"exit_code\":0,\"timed_out\":false}\n",
    )
    .unwrap();
    let output = script(root).arg("score").arg(&sessions).output().unwrap();
    assert!(output.status.success());
    let scored: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(scored["results"][0]["codex"]["infrastructure_errors"], 1);
    assert_eq!(scored["results"][0]["passed"], false);
}

#[test]
fn retained_failed_cohort_replays_only_as_diagnostic_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "2026-09-17-completion-diagnostic-1",
        "2026-09-17-completion-diagnostic-2",
        "2026-09-17-completion-diagnostic-3",
        "2026-09-17-completion-diagnostic-5",
        "2026-09-17-completion-diagnostic-6",
        "2026-09-17-completion-diagnostic-7",
    ] {
        let evidence = root.join("tests/agent-eval/results").join(name);
        let output = script(root).arg("replay").arg(evidence).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["verified"], true);
        assert_eq!(report["passed"], false);
        assert_eq!(report["acceptance_evidence"], false);
    }

    let evidence = root
        .join("tests/agent-eval/results")
        .join("2026-09-17-completion-diagnostic-4");
    let output = script(root).arg("replay").arg(evidence).output().unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["acceptance_evidence"], false);

    let evidence = root
        .join("tests/agent-eval/results")
        .join("2026-09-17-completion-acceptance");
    let output = script(root).arg("replay").arg(evidence).output().unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["acceptance_evidence"], true);
}
