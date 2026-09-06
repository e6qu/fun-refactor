use super::{render, History, Status};
use crate::git::process::{diagnostic, repository_root, require_no_filters, run};
use crate::history::{regular, target};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

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
    let repository_root = repository_root(&receiving_root)?;
    let prefix = receiving_root
        .strip_prefix(&repository_root)
        .context("Git resolved a working tree outside the receiving directory.")?;
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
    require_no_filters(&repository_root, &paths)?;
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
