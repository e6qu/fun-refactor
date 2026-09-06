use super::{render, History, Status};
use crate::history::{regular, target};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, Serialize)]
pub struct GitPatchCheck {
    pub id: u64,
    pub status: Status,
    pub record_basis: String,
    pub reverse: bool,
    pub receiving_root: PathBuf,
    pub repository_root: PathBuf,
    pub scope: &'static str,
    pub configuration: &'static str,
    pub checked_files: usize,
    pub applicable: bool,
    pub git_exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub diagnostics_truncated: bool,
}

fn run(root: &Path, args: &[OsString], input: Option<&[u8]>) -> Result<Output> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|name| name.starts_with("GIT_")) {
            command.env_remove(name);
        }
    }
    command
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .args([
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.attributesFile=/dev/null",
        ])
        .args(args);
    if let Some(input) = input {
        let mut file = tempfile::tempfile()?;
        file.write_all(input)?;
        file.seek(SeekFrom::Start(0))?;
        command.stdin(Stdio::from(file));
    } else {
        command.stdin(Stdio::null());
    }
    command
        .output()
        .context("running Git; install Git or use the snapshot basis check.")
}

fn diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]).into_owned()
}

pub fn check_git_patch(
    root: &Path,
    id: u64,
    reverse: bool,
    against: Option<&Path>,
    index: bool,
) -> Result<GitPatchCheck> {
    let history = History::read(root)?;
    let record = history.record(id)?;
    let patch = render(record, reverse)?;
    let receiving_root = against
        .unwrap_or(&history.root)
        .canonicalize()
        .context("resolving the receiving directory")?;
    if !receiving_root.is_dir() {
        bail!("Git patch checks require a receiving directory.");
    }
    for change in &record.changes {
        regular(&target(&receiving_root, &change.path)?)?;
    }
    let repository = run(
        &receiving_root,
        &["rev-parse".into(), "--show-toplevel".into()],
        None,
    )?;
    if !repository.status.success() {
        bail!(
            "receiving directory must be inside a Git working tree. {}",
            diagnostic(&repository.stderr)
        );
    }
    let repository_path = std::str::from_utf8(&repository.stdout)?
        .strip_suffix('\n')
        .context("Git did not report a working tree root")?;
    let repository_root = Path::new(repository_path).canonicalize()?;
    let prefix = receiving_root
        .strip_prefix(&repository_root)
        .context("Git resolved a working tree outside the receiving directory.")?;
    let reserved = run(
        &repository_root,
        &[
            "config".into(),
            "--null".into(),
            "--get-regexp".into(),
            "^filter\\.(unspecified|unset)\\.(clean|process)$".into(),
        ],
        None,
    )?;
    if reserved.status.success() {
        bail!("Git filter drivers named unset or unspecified are unsupported for patch checks.");
    }
    if reserved.status.code() != Some(1) {
        bail!(
            "checking Git filter configuration: {}",
            diagnostic(&reserved.stderr)
        );
    }
    let mut paths = Vec::new();
    for change in &record.changes {
        let path = prefix.join(&change.path);
        paths.extend_from_slice(
            path.to_str()
                .context("non-UTF-8 Git patch path")?
                .as_bytes(),
        );
        paths.push(0);
    }
    let attributes = run(
        &repository_root,
        &[
            "check-attr".into(),
            "-z".into(),
            "--stdin".into(),
            "filter".into(),
        ],
        Some(&paths),
    )?;
    if !attributes.status.success() {
        bail!(
            "checking Git content filters: {}",
            diagnostic(&attributes.stderr)
        );
    }
    let fields = attributes
        .stdout
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    if fields.len() != record.changes.len() * 3 + 1 || fields.last() != Some(&&b""[..]) {
        bail!("Git returned an incomplete attribute report.");
    }
    for (path, row) in paths.split(|byte| *byte == 0).zip(fields.chunks_exact(3)) {
        if row[0] != path || row[1] != b"filter" {
            bail!("Git returned an unexpected attribute path.");
        }
        if !matches!(row[2], b"unspecified" | b"unset") {
            bail!(
                "Git content filters are unsupported for patch checks: {:?}",
                String::from_utf8_lossy(path)
            );
        }
    }
    let mut args = vec!["apply".into(), "--check".into()];
    if index {
        args.push("--index".into());
    }
    if !prefix.as_os_str().is_empty() {
        let mut directory = OsString::from("--directory=");
        directory.push(prefix);
        args.push(directory);
    }
    args.push("-".into());
    let output = run(&repository_root, &args, Some(patch.as_bytes()))?;
    Ok(GitPatchCheck {
        id,
        status: record.status,
        record_basis: record.basis.clone(),
        reverse,
        receiving_root,
        repository_root,
        scope: if index {
            "index-and-worktree"
        } else {
            "worktree"
        },
        configuration: "repository-only-without-content-filters",
        checked_files: record.changes.len(),
        applicable: output.status.success(),
        git_exit_code: output.status.code(),
        stdout: diagnostic(&output.stdout),
        stderr: diagnostic(&output.stderr),
        diagnostics_truncated: output.stdout.len() > 16 * 1024 || output.stderr.len() > 16 * 1024,
    })
}
