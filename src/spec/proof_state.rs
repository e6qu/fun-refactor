//! Read actual editor goals from the pinned Lean server, without saving a proof.
use super::{FormalGoalBudget, ProofDiagnosticLocation};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_CAPTURE: usize = 1024 * 1024;
const MAX_HEADER: usize = 8192;

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStateStatus {
    Goals,
    ClosedAtPosition,
    NoTacticAtPosition,
}

#[derive(Debug, Serialize)]
pub struct ProofState {
    pub operation: &'static str,
    pub target: String,
    pub goal_id: String,
    pub proof_digest: String,
    pub checker: &'static str,
    pub theorem: String,
    pub draft_location: ProofDiagnosticLocation,
    pub status: ProofStateStatus,
    pub goals: Vec<String>,
    pub acceptance: &'static str,
    pub scope: &'static str,
    pub check: Vec<String>,
    pub token_budget: FormalGoalBudget,
}

/// Coordinates are one-based Unicode scalar positions in the original tactics file.
pub fn proof_state(
    root: &Path,
    target: &str,
    proof_path: &Path,
    at: (usize, usize),
    token_limit: usize,
    timeout_seconds: u64,
) -> Result<ProofState> {
    if !(1024..=16384).contains(&token_limit) || !(1..=60).contains(&timeout_seconds) {
        bail!("proof state needs 1024..16384 response bytes and 1..60 shared checking seconds.");
    }
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let root = root.canonicalize()?;
    let prepared = super::prepare_proof(&root, target, proof_path)?;
    let package = super::checked_proof_package(&root, &prepared.spec)?;
    let checker_source = super::reviewed_kernel_context(&package, &prepared.updated)?;
    let position = checked_position(
        &prepared.proof_source,
        &checker_source,
        &prepared.obligation,
        at,
    )?;
    let task = super::proof_task(&root, target, 16384, true)?;
    let package_inputs =
        super::proof_package_inputs(&package, Some((&prepared.spec, &prepared.updated)), true)?;
    let dependency_target = format!("{}:deps", prepared.spec.strip_prefix(&package)?.display());
    let seconds = deadline.saturating_duration_since(Instant::now()).as_secs();
    if seconds == 0 {
        bail!("proof state deadline exhausted before dependency preparation.");
    }
    let mut build = Command::new("lake");
    build
        .args(["build", &dependency_target])
        .current_dir(&package);
    let dependencies = crate::checks::bounded_process(build, seconds, 4096, false)?;
    if dependencies["passed"] != true {
        bail!("proof state dependency preparation failed: {dependencies}");
    }
    let uri = file_uri(&prepared.spec)?;
    let mut command = Command::new("lake");
    command
        .args(["env", "lean", "-j", "1", "--server"])
        .current_dir(&package);
    let reply = query(
        command,
        &file_uri(&package)?,
        &uri,
        &checker_source,
        position,
        deadline,
    )?;
    if super::proof_package_inputs(&package, Some((&prepared.spec, &prepared.updated)), true)?
        != package_inputs
        || crate::vfs::read_to_string(&prepared.spec)? != prepared.original
        || super::proof_task(&root, target, 16384, true)?.goal.id != prepared.goal_id
        || crate::vfs::read_to_string(proof_path)? != prepared.proof_source
    {
        bail!("proof or package inputs changed during inspection; request a fresh proof state.");
    }
    let (status, goals) = decode_goals(reply)?;
    let mut report = ProofState {
        operation: "proof-state",
        target: target.into(),
        goal_id: prepared.goal_id,
        proof_digest: prepared.proof_digest,
        checker: super::LEAN_TOOLCHAIN,
        theorem: task.goal.theorem,
        draft_location: ProofDiagnosticLocation {
            path: proof_path.into(),
            line: at.0,
            column: at.1,
            source_excerpt: super::proof_source_excerpt(&prepared.proof_source, at.0, at.1),
        },
        status,
        goals,
        acceptance: "not-checked",
        scope: "Editor goals at this draft position. A closed branch does not accept the whole proof; run proof-check before saving.",
        check: vec![
            "spec".into(),
            "proof-check".into(),
            target.into(),
            "--from".into(),
            proof_path.display().to_string(),
            "--token-limit".into(),
            token_limit.to_string(),
        ],
        token_budget: FormalGoalBudget {
            limit: token_limit,
            used_upper_bound: token_limit,
            measurement: "serialized_utf8_bytes",
        },
    };
    let upper_bound = serde_json::to_vec_pretty(&report)?.len();
    if upper_bound > token_limit {
        bail!(
            "complete proof state needs {upper_bound} response bytes; the selected ceiling is {token_limit}. No goals or assumptions were silently removed."
        );
    }
    report.token_budget.used_upper_bound = upper_bound;
    Ok(report)
}

fn checked_position(
    proof: &str,
    checked: &str,
    obligation: &str,
    at: (usize, usize),
) -> Result<(usize, usize)> {
    let text = proof
        .lines()
        .nth(at.0.checked_sub(1).context("proof line starts at one")?)
        .context("proof cursor line is outside the tactics file")?;
    if at.1 == 0 || at.1 > text.chars().count() + 1 {
        bail!("proof cursor column must be one-based and within its line, including its end.");
    }
    let leading = &proof[..proof.len() - proof.trim_start_matches(['\r', '\n']).len()];
    let leading_lines = leading.bytes().filter(|byte| *byte == b'\n').count();
    let offset =
        at.0.checked_sub(leading_lines + 1)
            .context("proof cursor points to a removed outer blank line")?;
    if offset >= proof.trim_matches(['\r', '\n']).lines().count() {
        bail!("proof cursor points to a removed outer blank line.");
    }
    let (_, start) = super::proof_region_span(checked, obligation)?;
    let column = 2 + text
        .chars()
        .take(at.1 - 1)
        .map(char::len_utf16)
        .sum::<usize>();
    Ok((start - 1 + offset, column))
}

fn file_uri(path: &Path) -> Result<String> {
    let path = path
        .to_str()
        .context("Lean editor inspection needs a UTF-8 file path")?;
    #[cfg(windows)]
    let path = {
        let normalized = path.replace('\\', "/");
        let normalized = if let Some(unc) = normalized.strip_prefix("//?/UNC/") {
            format!("//{unc}")
        } else {
            normalized
                .strip_prefix("//?/")
                .unwrap_or(&normalized)
                .to_owned()
        };
        if normalized.starts_with("//") {
            normalized
        } else {
            format!("///{normalized}")
        }
    };
    #[cfg(windows)]
    let mut uri = String::from("file:");
    #[cfg(not(windows))]
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(uri, "%{byte:02X}")?;
        }
    }
    Ok(uri)
}

fn decode_goals(reply: Value) -> Result<(ProofStateStatus, Vec<String>)> {
    if reply.is_null() {
        return Ok((ProofStateStatus::NoTacticAtPosition, Vec::new()));
    }
    let rows = reply
        .get("goals")
        .and_then(Value::as_array)
        .context("Lean editor response has no goals array")?;
    let goals = rows
        .iter()
        .map(|row| {
            row.as_str()
                .map(String::from)
                .context("Lean editor goal must be text")
        })
        .collect::<Result<Vec<_>>>()?;
    if goals.iter().any(|goal| goal.is_empty()) {
        bail!("Lean editor returned an empty goal entry.");
    }
    Ok((
        if goals.is_empty() {
            ProofStateStatus::ClosedAtPosition
        } else {
            ProofStateStatus::Goals
        },
        goals,
    ))
}

fn read_message(reader: &mut impl Read, captured: &AtomicUsize) -> Result<Value> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() == MAX_HEADER {
            bail!("Lean editor header exceeds 8192 bytes.");
        }
        let mut byte = [0];
        reader
            .read_exact(&mut byte)
            .context("Lean editor closed its response stream")?;
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).context("Lean editor header is not UTF-8")?;
    let mut size = None;
    for line in header.trim_end().split("\r\n") {
        let (name, value) = line.split_once(':').context("invalid Lean editor header")?;
        if name.eq_ignore_ascii_case("Content-Length") {
            if size.is_some() {
                bail!("duplicate Lean editor content length.");
            }
            size = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Lean editor content length")?,
            );
        }
    }
    let size = size.context("Lean editor response has no content length")?;
    if size == 0
        || size > MAX_CAPTURE
        || captured
            .fetch_add(header.len() + size, Ordering::SeqCst)
            .saturating_add(header.len() + size)
            > MAX_CAPTURE
    {
        bail!("Lean editor output exceeds 1 MiB.");
    }
    let mut body = vec![0; size];
    reader
        .read_exact(&mut body)
        .context("incomplete Lean editor response")?;
    serde_json::from_slice(&body).context("Lean editor response is not JSON")
}

fn send(writer: &mut impl Write, message: Value) -> Result<()> {
    let body = serde_json::to_vec(&message)?;
    if body.len() > MAX_CAPTURE {
        bail!("Lean editor request exceeds 1 MiB.");
    }
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

fn response(
    reader: &mut impl Read,
    writer: &mut impl Write,
    captured: &AtomicUsize,
    identity: u8,
) -> Result<Value> {
    loop {
        let message = read_message(reader, captured)?;
        if message["jsonrpc"] != "2.0" {
            bail!("invalid Lean editor protocol version.");
        }
        if message.get("method").is_some() {
            if let Some(id) = message.get("id") {
                let method = message["method"].as_str().unwrap_or("");
                if !matches!(
                    method,
                    "client/registerCapability" | "workspace/inlayHint/refresh"
                ) {
                    bail!("unsupported Lean editor client request: {method}");
                }
                send(writer, json!({"jsonrpc":"2.0", "id":id, "result":null}))?;
            }
            continue;
        }
        if message["id"] != identity {
            bail!("unexpected Lean editor response identity.");
        }
        if let Some(error) = message.get("error") {
            bail!("Lean editor request failed: {error}");
        }
        return message
            .get("result")
            .cloned()
            .context("Lean editor reply has no result");
    }
}

struct OwnedProcess {
    child: Child,
    stopped: bool,
}

fn stop(process: &mut OwnedProcess) -> Result<()> {
    if process.stopped {
        return Ok(());
    }
    process.stopped = true;
    let child = &mut process.child;
    #[cfg(unix)]
    {
        let pid = i32::try_from(child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .context("invalid Lean editor process group")?;
        match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => (),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    }
    if child.try_wait()?.is_none() {
        if let Err(error) = child.kill() {
            if child.try_wait()?.is_none() {
                return Err(error.into());
            }
        }
    }
    child.wait().context("reaping Lean editor process")?;
    Ok(())
}

struct Session {
    child: Arc<Mutex<OwnedProcess>>,
    done: Option<mpsc::Sender<()>>,
    watcher: Option<JoinHandle<Result<()>>>,
    finished: bool,
}

impl Session {
    fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        if let Some(done) = self.done.take() {
            let _ = done.send(());
        }
        let watched = self
            .watcher
            .take()
            .map(|thread| {
                thread
                    .join()
                    .map_err(|_| anyhow::anyhow!("Lean editor resource monitor panicked"))?
            })
            .transpose();
        let stopped = {
            let mut child = self
                .child
                .lock()
                .map_err(|_| anyhow::anyhow!("Lean editor process lock failed"))?;
            stop(&mut child)
        };
        watched.map(|_| ())?;
        stopped
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn query(
    mut command: Command,
    root_uri: &str,
    uri: &str,
    source: &str,
    position: (usize, usize),
    deadline: Instant,
) -> Result<Value> {
    if Instant::now() >= deadline {
        bail!("proof state checking deadline exhausted.");
    }
    let errors = tempfile::tempfile()?;
    let mut reported_errors = errors.try_clone()?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(errors.try_clone()?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .context("starting the pinned Lean editor server")?;
    let mut input = child.stdin.take().context("Lean editor stdin is missing")?;
    let mut output = BufReader::new(
        child
            .stdout
            .take()
            .context("Lean editor stdout is missing")?,
    );
    let child = Arc::new(Mutex::new(OwnedProcess {
        child,
        stopped: false,
    }));
    let captured = Arc::new(AtomicUsize::new(0));
    let reason = Arc::new(AtomicU8::new(0));
    let (done, wait) = mpsc::channel();
    let watched_child = child.clone();
    let watched_capture = captured.clone();
    let watched_reason = reason.clone();
    let mut session = Session {
        child,
        done: Some(done),
        watcher: None,
        finished: false,
    };
    let watcher = std::thread::Builder::new()
        .name("lean-proof-state-limit".into())
        .spawn(move || -> Result<()> {
            loop {
                match wait.recv_timeout(Duration::from_millis(20)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                }
                let stderr_bytes = match errors.metadata() {
                    Ok(metadata) => metadata.len(),
                    Err(error) => {
                        watched_reason.store(3, Ordering::SeqCst);
                        let mut child = watched_child
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Lean editor process lock failed"))?;
                        stop(&mut child)?;
                        return Err(error.into());
                    }
                };
                let stopped = if Instant::now() >= deadline {
                    1
                } else if stderr_bytes.saturating_add(watched_capture.load(Ordering::SeqCst) as u64)
                    > MAX_CAPTURE as u64
                {
                    2
                } else {
                    0
                };
                if stopped != 0 {
                    watched_reason.store(stopped, Ordering::SeqCst);
                    let mut child = watched_child
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Lean editor process lock failed"))?;
                    return stop(&mut child);
                }
            }
        })
        .context("starting the Lean editor resource monitor")?;
    session.watcher = Some(watcher);
    let result = (|| {
        send(
            &mut input,
            json!({"jsonrpc":"2.0", "id":1, "method":"initialize",
            "params":{"processId":std::process::id(),"rootUri":root_uri,"capabilities":{}}}),
        )?;
        response(&mut output, &mut input, &captured, 1)?;
        send(
            &mut input,
            json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        )?;
        send(
            &mut input,
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
            "params":{"textDocument":{"uri":uri,"languageId":"lean4","version":1,"text":source}}}),
        )?;
        send(
            &mut input,
            json!({"jsonrpc":"2.0","id":2,"method":"$/lean/plainGoal",
            "params":{"textDocument":{"uri":uri},"position":{"line":position.0,"character":position.1}}}),
        )?;
        response(&mut output, &mut input, &captured, 2)
    })();
    session.finish()?;
    match reason.load(Ordering::SeqCst) {
        1 => bail!(
            "Lean editor exceeded the selected shared checking deadline; its processes were stopped."
        ),
        2 => bail!("Lean editor exceeded the 1 MiB output ceiling; its processes were stopped."),
        _ => {
            reported_errors.seek(SeekFrom::Start(0))?;
            let mut prefix = Vec::new();
            reported_errors.take(2048).read_to_end(&mut prefix)?;
            result.with_context(|| {
                format!(
                    "Lean editor stderr (first 2048 bytes): {}",
                    String::from_utf8_lossy(&prefix)
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_mapping_preserves_outer_lines_indentation_and_unicode() {
        let checked = "theorem demo : True := by\n-- fr:proof-begin demo\n  have 😀 := True\n  trivial\n-- fr:proof-end demo\n";
        assert_eq!(
            checked_position("\n\nhave 😀 := True\ntrivial\n", checked, "demo", (3, 7)).unwrap(),
            (2, 9)
        );
        assert!(checked_position("\ntrivial\n", checked, "demo", (1, 1)).is_err());
        assert!(checked_position("trivial\n", checked, "demo", (2, 1)).is_err());
        assert!(checked_position("trivial", checked, "demo", (1, 9)).is_err());
    }

    #[test]
    fn no_tactic_and_a_closed_branch_are_distinct_from_proof_acceptance() {
        assert!(matches!(
            decode_goals(Value::Null).unwrap().0,
            ProofStateStatus::NoTacticAtPosition
        ));
        assert!(matches!(
            decode_goals(json!({"goals":[]})).unwrap().0,
            ProofStateStatus::ClosedAtPosition
        ));
        assert!(decode_goals(json!({"goals":[false]})).is_err());
        assert!(decode_goals(json!({"goals":[""]})).is_err());
        assert_eq!(
            decode_goals(json!({"goals":["h : P\n⊢ Q"]})).unwrap().1,
            ["h : P\n⊢ Q"]
        );
    }

    #[test]
    fn framing_refuses_missing_duplicate_and_excessive_lengths() {
        for raw in [
            b"Content-Type: application/json\r\n\r\n".to_vec(),
            b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
            b"Content-Length: 1048577\r\n\r\n".to_vec(),
            vec![b'x'; MAX_HEADER + 1],
        ] {
            assert!(read_message(&mut raw.as_slice(), &AtomicUsize::new(0)).is_err());
        }
        let mut raw = b"Content-Length: 2\r\n\r\n{}".as_slice();
        assert_eq!(
            read_message(&mut raw, &AtomicUsize::new(0)).unwrap(),
            json!({})
        );
        #[cfg(not(windows))]
        assert_eq!(
            file_uri(Path::new("/tmp/a b/λ.lean")).unwrap(),
            "file:///tmp/a%20b/%CE%BB.lean"
        );
        #[cfg(windows)]
        for path in [r"C:\a b\λ.lean", r"\\?\C:\a b\λ.lean"] {
            assert_eq!(
                file_uri(Path::new(path)).unwrap(),
                "file:///C:/a%20b/%CE%BB.lean"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn stalled_editor_is_killed_with_its_process_group_at_deadline() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 20"]);
        let started = Instant::now();
        let error = query(
            command,
            "file:///tmp",
            "file:///tmp/Draft.lean",
            "",
            (0, 0),
            started + Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.to_string().contains("deadline"), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
