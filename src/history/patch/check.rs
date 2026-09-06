use super::{git_mode, render, History, Status};
use crate::history::{matches_snapshot, snapshot, target};
use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct PatchBasisFile {
    pub path: PathBuf,
    pub expected_exists: bool,
    pub actual_exists: bool,
    pub expected_mode: Option<u32>,
    pub actual_mode: Option<u32>,
    pub content_matches: bool,
    pub git_mode_matches: bool,
    pub matches_patch_basis: bool,
    pub matches_recorded_snapshot: bool,
}

#[derive(Debug, Serialize)]
pub struct PatchBasisCheck {
    pub id: u64,
    pub status: Status,
    pub record_basis: String,
    pub reverse: bool,
    pub receiving_root: PathBuf,
    pub scope: &'static str,
    pub checked_files: usize,
    pub matches_patch_basis: bool,
    pub matches_recorded_snapshots: bool,
    pub files: Vec<PatchBasisFile>,
}

pub fn check_patch_basis(
    root: &Path,
    id: u64,
    reverse: bool,
    against: Option<&Path>,
) -> Result<PatchBasisCheck> {
    let history = History::read(root)?;
    let record = history.record(id)?;
    render(record, reverse)?;
    let receiving_root = against.unwrap_or(&history.root).canonicalize()?;
    if !receiving_root.is_dir() {
        bail!("patch basis checks require a receiving directory");
    }
    let mut changes = record.changes.iter().collect::<Vec<_>>();
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let mut files = Vec::with_capacity(changes.len());
    for change in changes {
        let expected = if reverse {
            &change.after
        } else {
            &change.before
        };
        let actual = snapshot(&target(&receiving_root, &change.path)?)?;
        let content_matches =
            actual.as_ref().map(|s| &s.content) == expected.as_ref().map(|s| &s.content);
        let git_mode_matches = actual.as_ref().map(git_mode) == expected.as_ref().map(git_mode);
        files.push(PatchBasisFile {
            path: change.path.clone(),
            expected_exists: expected.is_some(),
            actual_exists: actual.is_some(),
            expected_mode: expected.as_ref().map(|s| s.mode),
            actual_mode: actual.as_ref().map(|s| s.mode),
            content_matches,
            git_mode_matches,
            matches_patch_basis: content_matches && git_mode_matches,
            matches_recorded_snapshot: matches_snapshot(&actual, expected, &None, false),
        });
    }
    Ok(PatchBasisCheck {
        id,
        status: record.status,
        record_basis: record.basis.clone(),
        reverse,
        receiving_root,
        scope: "affected-paths-content-and-git-mode",
        checked_files: files.len(),
        matches_patch_basis: files.iter().all(|file| file.matches_patch_basis),
        matches_recorded_snapshots: files.iter().all(|file| file.matches_recorded_snapshot),
        files,
    })
}
