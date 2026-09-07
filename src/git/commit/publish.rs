use super::host::{args, observed_branch, Head};
use crate::git::process;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

struct Transaction {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    errors: File,
}

impl Transaction {
    fn start(root: &Path) -> Result<Self> {
        let errors = tempfile::tempfile()?;
        let mut child = process::command(
            root,
            &args(&[
                "update-ref",
                "--stdin",
                "--create-reflog",
                "-m",
                "fr: reviewed commit",
            ]),
            None,
            None,
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(errors.try_clone()?))
        .spawn()
        .context("starting reviewed reference update")?;
        let input = child.stdin.take();
        let output = BufReader::new(
            child
                .stdout
                .take()
                .context("missing Git transaction output")?,
        );
        Ok(Self {
            child,
            input,
            output,
            errors,
        })
    }

    fn send(&mut self, command: &str) -> Result<()> {
        let input = self.input.as_mut().context("closed Git transaction")?;
        input.write_all(command.as_bytes())?;
        input.flush()?;
        Ok(())
    }

    fn diagnostic(&mut self) -> String {
        let _ = self.errors.seek(SeekFrom::Start(0));
        let mut bytes = Vec::new();
        let _ = (&mut self.errors).take(16 * 1024).read_to_end(&mut bytes);
        process::diagnostic(&bytes)
    }

    fn acknowledge(&mut self, expected: &str) -> Result<()> {
        let mut line = String::new();
        (&mut self.output).take(1024).read_line(&mut line)?;
        ensure!(
            line == expected,
            "Git refused the prepared commit: {}",
            self.diagnostic()
        );
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
    }
}

pub(super) fn publish(
    root: &Path,
    commit: &str,
    before: &Head,
    check: impl Fn() -> Result<()>,
) -> Result<Value> {
    let mut transaction = Transaction::start(root)?;
    let missing = "0".repeat(commit.len());
    let old = before.parent.as_deref().unwrap_or(&missing);
    transaction.send(&format!("start\nupdate HEAD {commit} {old}\nprepare\n"))?;
    transaction.acknowledge("start: ok\n")?;
    transaction.acknowledge("prepare: ok\n")?;
    check()?;
    let sent = transaction.send("commit\n");
    transaction.input.take();
    let mut output = String::new();
    let read = (&mut transaction.output)
        .take(16 * 1024)
        .read_to_string(&mut output);
    let status = transaction.child.wait();
    let acknowledged = output.lines().any(|line| line == "commit: ok");
    let observed = if acknowledged {
        None
    } else {
        observed_branch(root, &before.branch)
    };
    let confirmed = acknowledged || observed.as_deref() == Some(commit);
    let successful = sent.is_ok()
        && read.is_ok()
        && status.as_ref().is_ok_and(|status| status.success())
        && acknowledged;
    let mut result = json!({"confirmed":confirmed.then_some(true),"branch":before.branch,"expected_parent":before.parent,
        "observed_commit":observed,"acknowledged":acknowledged});
    if !successful {
        result["warning"] = json!(if confirmed {
            "Commit publication is confirmed, but Git did not finish cleanly. Inspect the branch before retrying."
        } else {
            "Commit publication is unconfirmed. Inspect the branch and reflog before retrying."
        });
        result["diagnostic"] = json!(transaction.diagnostic());
    }
    Ok(result)
}
