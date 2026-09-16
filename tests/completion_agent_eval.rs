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

    let refused = script(root).arg("run").arg(&sessions).output().unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--confirm-agent-spend"));
}
