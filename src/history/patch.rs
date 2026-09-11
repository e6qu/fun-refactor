use super::{History, Record, Snapshot, SnapshotKind, Status};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

mod check;
pub use check::{check_patch_basis, PatchBasisCheck, PatchBasisFile};
mod git;
pub use git::{check_git_patch, GitPatchCheck};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub struct PatchExport {
    pub id: u64,
    pub status: Status,
    pub record_basis: String,
    pub source_revision: String,
    pub validation: String,
    pub reverse: bool,
    pub format: &'static str,
    pub mode_scope: &'static str,
    pub files: usize,
    pub patch: String,
}

pub fn export_patch(root: &Path, id: u64, reverse: bool) -> Result<PatchExport> {
    let history = History::read(root)?;
    let record = history.record(id)?;
    Ok(PatchExport {
        id,
        status: record.status,
        record_basis: record.basis.clone(),
        source_revision: record.source_revision.clone(),
        validation: record.validation.clone(),
        reverse,
        format: "git-text-diff",
        mode_scope: "regular-executable-or-symlink",
        files: record.changes.len(),
        patch: render(record, reverse)?,
    })
}

pub fn git_mode(mode: u32) -> u32 {
    if mode & 0o100 == 0 {
        0o100644
    } else {
        0o100755
    }
}

pub fn git_snapshot_mode(symlink: bool, mode: u32) -> u32 {
    if symlink {
        0o120000
    } else {
        git_mode(mode)
    }
}

fn snapshot_git_mode(snapshot: &Snapshot) -> u32 {
    git_snapshot_mode(snapshot.kind == SnapshotKind::Symlink, snapshot.mode)
}

pub fn git_mode_change_supported(before: u32, after: u32) -> bool {
    let changed = before ^ after;
    changed & !0o111 == 0 && (changed == 0 || git_mode(before) != git_mode(after))
}

pub fn owner_executable_mode(mode: u32, executable: bool) -> u32 {
    if executable {
        mode | 0o100
    } else {
        mode & !0o100
    }
}

pub fn matches_patch_basis(actual: &Option<Snapshot>, expected: &Option<Snapshot>) -> bool {
    actual.as_ref().map(|s| (&s.content, snapshot_git_mode(s)))
        == expected
            .as_ref()
            .map(|s| (&s.content, snapshot_git_mode(s)))
}

fn render(record: &Record, reverse: bool) -> Result<String> {
    let changes = record
        .changes
        .iter()
        .map(|change| crate::git_patch::Change {
            path: &change.path,
            before: change
                .before
                .as_ref()
                .map(|snapshot| crate::git_patch::Snapshot {
                    content: &snapshot.content,
                    mode: snapshot.mode,
                    symlink: snapshot.kind == SnapshotKind::Symlink,
                }),
            after: change
                .after
                .as_ref()
                .map(|snapshot| crate::git_patch::Snapshot {
                    content: &snapshot.content,
                    mode: snapshot.mode,
                    symlink: snapshot.kind == SnapshotKind::Symlink,
                }),
        })
        .collect::<Vec<_>>();
    crate::git_patch::render(&changes, reverse)
}
