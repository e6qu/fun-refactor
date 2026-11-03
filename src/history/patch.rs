use super::{Change, History, Snapshot, SnapshotKind, Status};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod check;
pub use check::{check_patch_basis, PatchBasisCheck, PatchBasisFile};
mod git;
pub use git::{check_git_patch, GitPatchCheck};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub struct PatchExport {
    pub transactions: Vec<PatchTransaction>,
    pub record_basis: String,
    pub reverse: bool,
    pub format: &'static str,
    pub mode_scope: &'static str,
    pub files: usize,
    pub patch: String,
}

#[derive(Debug, Serialize)]
pub struct PatchTransaction {
    pub id: u64,
    pub status: Status,
    pub record_basis: String,
    pub source_revision: String,
    pub validation: String,
}

struct Selection {
    transactions: Vec<PatchTransaction>,
    record_basis: String,
    changes: Vec<Change>,
}

fn select(history: &History, ids: &[u64]) -> Result<Selection> {
    ensure!(!ids.is_empty(), "select at least one history transaction");
    let mut seen = BTreeSet::new();
    let mut changes: BTreeMap<PathBuf, Change> = BTreeMap::new();
    let mut transactions = Vec::with_capacity(ids.len());
    for &id in ids {
        ensure!(seen.insert(id), "transaction {id} appears more than once");
        let record = history.record(id)?;
        record.ensure_replayable()?;
        for change in &record.changes {
            if let Some(previous) = changes.get_mut(&change.path) {
                ensure!(
                    previous.after == change.before,
                    "transaction {id} does not follow the selected file state for {}",
                    change.path.display()
                );
                previous.after = change.after.clone();
            } else {
                changes.insert(change.path.clone(), change.clone());
            }
        }
        transactions.push(PatchTransaction {
            id,
            status: record.status,
            record_basis: record.basis.clone(),
            source_revision: record.source_revision.clone(),
            validation: record.validation.clone(),
        });
    }
    let changes: Vec<_> = changes
        .into_values()
        .filter(|change| change.before != change.after)
        .collect();
    ensure!(
        !changes.is_empty(),
        "selected transactions make no net change"
    );
    Ok(Selection {
        record_basis: super::basis(&changes)?,
        changes,
        transactions,
    })
}

pub fn export_patch(root: &Path, ids: &[u64], reverse: bool) -> Result<PatchExport> {
    let history = History::read(root)?;
    let selection = select(&history, ids)?;
    Ok(PatchExport {
        patch: render(&selection.changes, reverse)?,
        transactions: selection.transactions,
        record_basis: selection.record_basis,
        reverse,
        format: "git-text-diff",
        mode_scope: "regular-executable-or-symlink",
        files: selection.changes.len(),
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

fn render(changes: &[Change], reverse: bool) -> Result<String> {
    let changes = changes
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
