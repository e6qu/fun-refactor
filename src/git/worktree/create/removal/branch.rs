use super::super::{args, checked, line};
use crate::git::process;
use anyhow::{ensure, Context, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};

pub(super) struct Lease {
    child: Child,
    input: Option<ChildStdin>,
}

pub(super) fn check(root: &Path, branch: &str, commit: &str) -> Result<()> {
    let reference = format!("refs/heads/{branch}");
    let symbolic = process::run(
        root,
        &args(&["symbolic-ref", "--quiet", "--no-recurse", &reference]),
        None,
    )?;
    ensure!(
        symbolic.status.code() == Some(1),
        "removal requires a direct local branch."
    );
    ensure!(
        line(&checked(
            root,
            &["show-ref", "--verify", "--hash", &reference]
        )?)? == commit,
        "worktree branch changed during removal."
    );
    Ok(())
}

impl Lease {
    pub(super) fn acquire(root: &Path, branch: &str, commit: &str) -> Result<Self> {
        check(root, branch, commit)?;
        let errors = tempfile::tempfile()?;
        let mut child = process::command(
            root,
            &args(&["update-ref", "--no-deref", "--stdin"]),
            None,
            None,
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(errors))
        .spawn()?;
        let input = child.stdin.take();
        let output = child
            .stdout
            .take()
            .context("missing branch lease output.")?;
        let mut lease = Self { child, input };
        let input = lease
            .input
            .as_mut()
            .context("missing branch lease input.")?;
        write!(
            input,
            "start\nverify refs/heads/{branch} {commit}\nprepare\n"
        )?;
        input.flush()?;
        let mut output = BufReader::new(output);
        for expected in ["start: ok\n", "prepare: ok\n"] {
            let mut response = String::new();
            (&mut output).take(1024).read_line(&mut response)?;
            ensure!(
                response == expected,
                "Git refused the branch preservation lease."
            );
        }
        check(root, branch, commit)?;
        Ok(lease)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_verification_blocks_branch_writes_and_releases_without_changes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        checked(root, &["init", "-q", "-b", "main"]).unwrap();
        checked(
            root,
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "fixture",
            ],
        )
        .unwrap();
        let raw = checked(root, &["rev-parse", "HEAD"]).unwrap();
        let commit = line(&raw).unwrap();
        let before = checked(root, &["reflog", "show", "main"]).unwrap();
        let lease = Lease::acquire(root, "main", commit).unwrap();
        assert!(checked(root, &["update-ref", "-d", "refs/heads/main", commit]).is_err());
        check(root, "main", commit).unwrap();
        drop(lease);
        assert_eq!(checked(root, &["reflog", "show", "main"]).unwrap(), before);
        checked(root, &["update-ref", "-d", "refs/heads/main", commit]).unwrap();
    }
}
