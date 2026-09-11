//! Checked transaction history for a workspace held entirely in memory.

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const MAX_RECORDS: usize = 256;
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Applied,
    Undone,
    Abandoned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: u32,
    pub status: Status,
    pub basis: String,
    pub changes: Vec<Change>,
}

#[derive(Default)]
pub struct History {
    records: Vec<Record>,
    applied: Vec<u32>,
    redo: Vec<u32>,
    bytes: usize,
}

#[derive(Serialize)]
pub struct Summary<'a> {
    pub schema: &'static str,
    pub records: Vec<RecordSummary<'a>>,
    pub applied: &'a [u32],
    pub redo: &'a [u32],
    pub retained_bytes: usize,
    pub record_limit: usize,
    pub byte_limit: usize,
}

#[derive(Serialize)]
pub struct RecordSummary<'a> {
    pub id: u32,
    pub status: Status,
    pub basis: &'a str,
    pub files: usize,
    pub snapshot_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct Transition {
    pub schema: &'static str,
    pub transaction: u32,
    pub action: &'static str,
    pub status: Status,
    pub transaction_basis: String,
    pub files: Vec<ChangedPath>,
}

#[derive(Debug, Serialize)]
pub struct ChangedPath {
    pub path: String,
    pub before_exists: bool,
    pub after_exists: bool,
}

fn snapshot_bytes(change: &Change) -> usize {
    change.before.as_ref().map_or(0, String::len)
        + change.after.as_ref().map_or(0, String::len)
        + change.path.as_os_str().as_encoded_bytes().len()
}

fn basis(changes: &[Change]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"fr-memory-transaction-1");
    for change in changes {
        let path = change.path.as_os_str().as_encoded_bytes();
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path);
        for snapshot in [&change.before, &change.after] {
            digest.update([u8::from(snapshot.is_some())]);
            if let Some(content) = snapshot {
                digest.update((content.len() as u64).to_be_bytes());
                digest.update(content.as_bytes());
            }
        }
    }
    format!("frmb1:{:x}", digest.finalize())
}

fn current(path: &Path) -> Result<Option<String>> {
    match crate::vfs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write(path: &Path, snapshot: &Option<String>) -> Result<()> {
    match snapshot {
        Some(text) => crate::vfs::write(path, text)?,
        None => crate::vfs::remove(path)?,
    }
    Ok(())
}

impl History {
    pub fn ensure_capacity(&self, changes: &[Change]) -> Result<()> {
        ensure!(
            !changes.is_empty(),
            "a transaction must change at least one file"
        );
        ensure!(
            changes.len() <= 1024,
            "a transaction exceeds 1024 changed files"
        );
        let mut paths = BTreeSet::new();
        ensure!(
            changes.iter().all(|change| {
                !change.path.as_os_str().is_empty()
                    && paths.insert(&change.path)
                    && change.before != change.after
            }),
            "a transaction has a duplicate or unchanged path"
        );
        let bytes = changes.iter().map(snapshot_bytes).sum::<usize>();
        ensure!(
            bytes <= MAX_RECORD_BYTES,
            "a transaction exceeds the 16 MiB snapshot limit"
        );
        ensure!(
            self.records.len() < MAX_RECORDS,
            "browser history reached its 256-record limit"
        );
        ensure!(
            self.bytes.saturating_add(bytes) <= MAX_HISTORY_BYTES,
            "browser history reached its 64 MiB snapshot limit"
        );
        Ok(())
    }

    pub fn push(&mut self, mut changes: Vec<Change>) -> Record {
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        for id in self.redo.drain(..) {
            if let Some(record) = self.records.iter_mut().find(|record| record.id == id) {
                record.status = Status::Abandoned;
            }
        }
        let id = self.records.last().map_or(1, |record| record.id + 1);
        let record = Record {
            id,
            status: Status::Applied,
            basis: basis(&changes),
            changes,
        };
        self.bytes += record.changes.iter().map(snapshot_bytes).sum::<usize>();
        self.applied.push(id);
        self.records.push(record.clone());
        record
    }

    pub fn summary(&self) -> Summary<'_> {
        Summary {
            schema: "fr-memory-history-1",
            records: self
                .records
                .iter()
                .map(|record| RecordSummary {
                    id: record.id,
                    status: record.status,
                    basis: &record.basis,
                    files: record.changes.len(),
                    snapshot_bytes: record.changes.iter().map(snapshot_bytes).sum(),
                })
                .collect(),
            applied: &self.applied,
            redo: &self.redo,
            retained_bytes: self.bytes,
            record_limit: MAX_RECORDS,
            byte_limit: MAX_HISTORY_BYTES,
        }
    }

    pub fn record(&self, id: u32) -> Result<&Record> {
        self.records
            .iter()
            .find(|record| record.id == id)
            .context("unknown browser transaction identity")
    }

    pub fn undo(&mut self, id: u32) -> Result<Transition> {
        if self.applied.last() != Some(&id) {
            bail!("transaction {id} cannot undo; browser undo follows applied stack order");
        }
        self.transition(id, true)
    }

    pub fn redo(&mut self, id: u32) -> Result<Transition> {
        if self.redo.last() != Some(&id) {
            bail!("transaction {id} cannot redo; browser redo follows reversal stack order");
        }
        self.transition(id, false)
    }

    fn transition(&mut self, id: u32, undo: bool) -> Result<Transition> {
        let record = self.record(id)?.clone();
        for change in &record.changes {
            let expected = if undo { &change.after } else { &change.before };
            ensure!(
                current(&change.path)? == *expected,
                "{} changed after transaction {id}; preserving the current workspace",
                change.path.display()
            );
        }
        for change in &record.changes {
            write(
                &change.path,
                if undo { &change.before } else { &change.after },
            )?;
        }
        if undo {
            self.applied.pop();
            self.redo.push(id);
        } else {
            self.redo.pop();
            self.applied.push(id);
        }
        let status = if undo {
            Status::Undone
        } else {
            Status::Applied
        };
        self.records
            .iter_mut()
            .find(|entry| entry.id == id)
            .unwrap()
            .status = status;
        Ok(Transition {
            schema: "fr-memory-transition-1",
            transaction: id,
            action: if undo { "undo" } else { "redo" },
            status,
            transaction_basis: record.basis,
            files: record
                .changes
                .iter()
                .map(|change| ChangedPath {
                    path: change.path.display().to_string(),
                    before_exists: if undo {
                        change.after.is_some()
                    } else {
                        change.before.is_some()
                    },
                    after_exists: if undo {
                        change.before.is_some()
                    } else {
                        change.after.is_some()
                    },
                })
                .collect(),
        })
    }

    pub fn patch(&self, id: u32, reverse: bool) -> Result<String> {
        render(&self.record(id)?.changes, reverse)
    }

    pub fn current_patch(&self) -> Result<String> {
        let mut changes = BTreeMap::<PathBuf, Change>::new();
        for id in &self.applied {
            for change in &self.record(*id)?.changes {
                changes
                    .entry(change.path.clone())
                    .and_modify(|combined| combined.after = change.after.clone())
                    .or_insert_with(|| change.clone());
            }
        }
        let changes = changes
            .into_values()
            .filter(|change| change.before != change.after)
            .collect::<Vec<_>>();
        render(&changes, false)
    }
}

fn render(changes: &[Change], reverse: bool) -> Result<String> {
    crate::git_patch::render(
        &changes
            .iter()
            .map(|change| crate::git_patch::Change {
                path: &change.path,
                before: change
                    .before
                    .as_deref()
                    .map(|content| crate::git_patch::Snapshot {
                        content,
                        mode: 0o644,
                        symlink: false,
                    }),
                after: change
                    .after
                    .as_deref()
                    .map(|content| crate::git_patch::Snapshot {
                        content,
                        mode: 0o644,
                        symlink: false,
                    }),
            })
            .collect::<Vec<_>>(),
        reverse,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, before: Option<&str>, after: Option<&str>) -> Change {
        Change {
            path: PathBuf::from(path),
            before: before.map(str::to_string),
            after: after.map(str::to_string),
        }
    }

    fn activate(files: &[(&str, &str)]) {
        let handle = crate::vfs::new_handle(
            files
                .iter()
                .map(|(path, text)| (PathBuf::from(path), (*text).to_string())),
        );
        crate::vfs::activate(&handle);
    }

    #[test]
    fn undo_redo_are_checked_stack_transitions() {
        activate(&[("a.rs", "three"), ("b.rs", "stable")]);
        let mut history = History::default();
        let first = vec![change("a.rs", Some("one"), Some("two"))];
        let second = vec![change("a.rs", Some("two"), Some("three"))];
        history.ensure_capacity(&first).unwrap();
        history.push(first);
        history.ensure_capacity(&second).unwrap();
        history.push(second);

        assert!(history
            .undo(1)
            .unwrap_err()
            .to_string()
            .contains("stack order"));
        history.undo(2).unwrap();
        assert_eq!(current(Path::new("a.rs")).unwrap().as_deref(), Some("two"));
        history.redo(2).unwrap();
        assert_eq!(
            current(Path::new("a.rs")).unwrap().as_deref(),
            Some("three")
        );

        crate::vfs::write("a.rs", "outside edit").unwrap();
        assert!(history
            .undo(2)
            .unwrap_err()
            .to_string()
            .contains("preserving"));
        assert_eq!(
            current(Path::new("a.rs")).unwrap().as_deref(),
            Some("outside edit")
        );
        assert_eq!(
            current(Path::new("b.rs")).unwrap().as_deref(),
            Some("stable")
        );
    }

    #[test]
    fn created_files_are_removed_and_redo_is_abandoned_by_a_new_edit() {
        activate(&[("base.rs", "base")]);
        crate::vfs::write("generated.rs", "generated").unwrap();
        let mut history = History::default();
        let created = vec![change("generated.rs", None, Some("generated"))];
        history.ensure_capacity(&created).unwrap();
        history.push(created);

        history.undo(1).unwrap();
        assert_eq!(current(Path::new("generated.rs")).unwrap(), None);
        history.redo(1).unwrap();
        assert_eq!(
            current(Path::new("generated.rs")).unwrap().as_deref(),
            Some("generated")
        );
        history.undo(1).unwrap();

        crate::vfs::write("base.rs", "changed").unwrap();
        let replacement = vec![change("base.rs", Some("base"), Some("changed"))];
        history.ensure_capacity(&replacement).unwrap();
        history.push(replacement);
        assert!(history
            .redo(1)
            .unwrap_err()
            .to_string()
            .contains("stack order"));
        assert_eq!(history.record(1).unwrap().status, Status::Abandoned);
    }

    #[test]
    fn cumulative_patch_folds_applied_transactions_from_the_loaded_basis() {
        activate(&[("a.rs", "one\n")]);
        let mut history = History::default();
        for (before, after) in [("one\n", "two\n"), ("two\n", "three\n")] {
            let changes = vec![change("a.rs", Some(before), Some(after))];
            history.ensure_capacity(&changes).unwrap();
            crate::vfs::write("a.rs", after).unwrap();
            history.push(changes);
        }
        let patch = history.current_patch().unwrap();
        assert!(patch.contains("-one"), "{patch}");
        assert!(patch.contains("+three"), "{patch}");
        assert!(!patch.contains("two"), "{patch}");
        history.undo(2).unwrap();
        let patch = history.current_patch().unwrap();
        assert!(patch.contains("+two"), "{patch}");
    }
}
