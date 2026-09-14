use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const MAX_RECORDS: usize = 256;
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Applied,
    Undone,
    Abandoned,
}

impl Status {
    fn code(self) -> usize {
        match self {
            Self::Applied => 0,
            Self::Undone => 1,
            Self::Abandoned => 2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: u32,
    pub status: Status,
    pub basis: String,
    pub changes: Vec<Change>,
}

pub struct History {
    base: Vec<Change>,
    records: Vec<Record>,
    applied: Vec<u32>,
    redo: Vec<u32>,
    bytes: usize,
    next_id: u32,
}

impl Default for History {
    fn default() -> Self {
        Self {
            base: Vec::new(),
            records: Vec::new(),
            applied: Vec::new(),
            redo: Vec::new(),
            bytes: 0,
            next_id: 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub(crate) schema: String,
    pub(crate) base: Vec<Change>,
    pub(crate) records: Vec<Record>,
    pub(crate) applied: Vec<u32>,
    pub(crate) redo: Vec<u32>,
    pub(crate) next_id: u32,
}

#[derive(Serialize)]
pub struct Summary<'a> {
    pub schema: &'static str,
    pub records: Vec<RecordSummary<'a>>,
    pub applied: &'a [u32],
    pub redo: &'a [u32],
    pub retained_bytes: usize,
    pub compacted_files: usize,
    pub next_transaction: u32,
    pub record_limit: usize,
    pub byte_limit: usize,
}

#[derive(Debug, Serialize)]
pub struct Compaction {
    pub schema: &'static str,
    pub keep: usize,
    pub records_before: usize,
    pub records_after: usize,
    pub frozen_transactions: usize,
    pub discarded_transactions: usize,
    pub compacted_files: usize,
    pub retained_bytes: usize,
    pub undo: Option<u32>,
    pub redo: Option<u32>,
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
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            schema: "fr-memory-history-snapshot-2".to_string(),
            base: self.base.clone(),
            records: self.records.clone(),
            applied: self.applied.clone(),
            redo: self.redo.clone(),
            next_id: self.next_id,
        }
    }

    pub(crate) fn restore(
        snapshot: Snapshot,
        current_files: &BTreeMap<PathBuf, String>,
    ) -> Result<Self> {
        ensure!(
            snapshot.schema == "fr-memory-history-snapshot-2",
            "unsupported browser history snapshot"
        );
        ensure!(
            snapshot.records.len() <= MAX_RECORDS,
            "browser history exceeds its 256-record limit."
        );

        ensure_changes(&snapshot.base, "browser compacted history")?;
        let mut bytes = snapshot.base.iter().map(snapshot_bytes).sum::<usize>();
        let mut previous = 0;
        for record in &snapshot.records {
            ensure!(
                record.id > previous && record.id < snapshot.next_id,
                "browser transaction identities do not have increasing order."
            );
            previous = record.id;
            ensure!(
                !record.changes.is_empty() && record.changes.len() <= 1024,
                "browser transaction {} has an invalid file count.",
                record.id
            );
            ensure_changes(
                &record.changes,
                &format!("browser transaction {}", record.id),
            )?;
            let record_bytes = record.changes.iter().map(snapshot_bytes).sum::<usize>();
            ensure!(
                record_bytes <= MAX_RECORD_BYTES,
                "browser transaction {} exceeds its snapshot limit.",
                record.id
            );
            bytes = bytes
                .checked_add(record_bytes)
                .context("browser history size overflow")?;
            ensure!(
                record.basis == basis(&record.changes),
                "browser transaction {} has an invalid basis.",
                record.id
            );
        }
        ensure!(
            bytes <= MAX_HISTORY_BYTES,
            "browser history exceeds its 64 MiB snapshot limit."
        );
        ensure!(
            snapshot.next_id > 0,
            "browser history has an invalid next identity."
        );
        ensure!(
            snapshot.applied.windows(2).all(|ids| ids[0] < ids[1]),
            "browser applied stack does not have increasing order."
        );
        ensure!(
            snapshot.redo.windows(2).all(|ids| ids[0] > ids[1]),
            "browser redo stack does not have decreasing order."
        );

        let applied = snapshot.applied.iter().copied().collect::<BTreeSet<_>>();
        let redo = snapshot.redo.iter().copied().collect::<BTreeSet<_>>();
        ensure!(
            applied.len() == snapshot.applied.len()
                && redo.len() == snapshot.redo.len()
                && applied.is_disjoint(&redo),
            "browser history stacks contain duplicate transaction identities."
        );
        for record in &snapshot.records {
            let expected = match (applied.contains(&record.id), redo.contains(&record.id)) {
                (true, false) => Status::Applied,
                (false, true) => Status::Undone,
                (false, false) => Status::Abandoned,
                (true, true) => unreachable!(),
            };
            ensure!(
                record.status == expected,
                "browser transaction {} disagrees with its stack.",
                record.id
            );
        }
        let find = |id: u32| {
            snapshot
                .records
                .binary_search_by_key(&id, |record| record.id)
                .ok()
                .map(|at| &snapshot.records[at])
        };
        let replace = |files: &mut BTreeMap<PathBuf, String>,
                       changes: &[Change],
                       before: bool|
         -> Result<()> {
            for change in changes {
                let expected = if before {
                    &change.before
                } else {
                    &change.after
                };
                ensure!(
                    files.get(&change.path) == expected.as_ref(),
                    "browser transaction {} does not match the restored workspace.",
                    basis(changes)
                );
            }
            for change in changes {
                let value = if before {
                    &change.after
                } else {
                    &change.before
                };
                match value {
                    Some(text) => {
                        files.insert(change.path.clone(), text.clone());
                    }
                    None => {
                        files.remove(&change.path);
                    }
                }
            }
            Ok(())
        };

        let mut reconstructed_basis = current_files.clone();
        for id in snapshot.applied.iter().rev() {
            replace(
                &mut reconstructed_basis,
                &find(*id)
                    .context("browser applied stack names an unknown transaction")?
                    .changes,
                false,
            )?;
        }
        replace(&mut reconstructed_basis, &snapshot.base, false)?;
        let mut redo_state = current_files.clone();
        for id in snapshot.redo.iter().rev() {
            replace(
                &mut redo_state,
                &find(*id)
                    .context("browser redo stack names an unknown transaction")?
                    .changes,
                true,
            )?;
        }

        Ok(Self {
            base: snapshot.base,
            records: snapshot.records,
            applied: snapshot.applied,
            redo: snapshot.redo,
            bytes,
            next_id: snapshot.next_id,
        })
    }

    pub fn ensure_capacity(&self, changes: &[Change]) -> Result<()> {
        ensure!(
            !changes.is_empty(),
            "a transaction must change at least one file."
        );
        ensure!(
            changes.len() <= 1024,
            "a transaction exceeds 1024 changed files."
        );
        let mut paths = BTreeSet::new();
        ensure!(
            changes.iter().all(|change| {
                !change.path.as_os_str().is_empty()
                    && paths.insert(&change.path)
                    && change.before != change.after
            }),
            "a transaction has a duplicate or unchanged path."
        );
        let bytes = changes.iter().map(snapshot_bytes).sum::<usize>();
        ensure!(
            bytes <= MAX_RECORD_BYTES,
            "a transaction exceeds the 16 MiB snapshot limit."
        );
        ensure!(
            self.records.len() < MAX_RECORDS,
            "browser history reached its 256-record limit."
        );
        ensure!(
            self.next_id < u32::MAX,
            "browser history exhausted its transaction identities."
        );
        ensure!(
            self.bytes.saturating_add(bytes) <= MAX_HISTORY_BYTES,
            "browser history reached its 64 MiB snapshot limit."
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
        let id = self.next_id;
        self.next_id += 1;
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
            compacted_files: self.base.len(),
            next_transaction: self.next_id,
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
        let status = self.record(id)?.status;
        if !crate::transaction_kernel::memory_transition_allowed(
            status.code(),
            0,
            self.applied.last() == Some(&id),
        ) {
            bail!("transaction {id} cannot undo; browser undo follows applied stack order.");
        }
        self.transition(id, true)
    }

    pub fn redo(&mut self, id: u32) -> Result<Transition> {
        let status = self.record(id)?.status;
        if !crate::transaction_kernel::memory_transition_allowed(
            status.code(),
            1,
            self.redo.last() == Some(&id),
        ) {
            bail!("transaction {id} cannot redo; browser redo follows reversal stack order.");
        }
        self.transition(id, false)
    }

    fn transition(&mut self, id: u32, undo: bool) -> Result<Transition> {
        let record = self.record(id)?.clone();
        for change in &record.changes {
            let expected = if undo { &change.after } else { &change.before };
            ensure!(
                current(&change.path)? == *expected,
                "{} changed after transaction {id}; preserving the current workspace.",
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
        merge(&mut changes, &self.base);
        for id in &self.applied {
            merge(&mut changes, &self.record(*id)?.changes);
        }
        let changes = changes
            .into_values()
            .filter(|change| change.before != change.after)
            .collect::<Vec<_>>();
        render(&changes, false)
    }

    pub fn compact(&mut self, keep: usize) -> Result<Compaction> {
        ensure!(
            crate::transaction_kernel::memory_compaction_allowed(keep),
            "browser history keep count must be 0 through 256"
        );
        let records_before = self.records.len();
        let applied_drop = self.applied.len().saturating_sub(keep);
        let redo_drop = self.redo.len().saturating_sub(keep);
        let frozen = self.applied[..applied_drop].to_vec();

        let mut base = self
            .base
            .iter()
            .cloned()
            .map(|change| (change.path.clone(), change))
            .collect::<BTreeMap<_, _>>();
        for id in &frozen {
            merge(&mut base, &self.record(*id)?.changes);
        }
        self.base = base
            .into_values()
            .filter(|change| change.before != change.after)
            .collect();
        self.applied.drain(..applied_drop);
        self.redo.drain(..redo_drop);
        let retained = self
            .applied
            .iter()
            .chain(&self.redo)
            .copied()
            .collect::<BTreeSet<_>>();
        self.records.retain(|record| retained.contains(&record.id));
        self.bytes = self
            .base
            .iter()
            .map(snapshot_bytes)
            .chain(
                self.records
                    .iter()
                    .flat_map(|record| record.changes.iter().map(snapshot_bytes)),
            )
            .sum();

        Ok(Compaction {
            schema: "fr-memory-history-compaction-1",
            keep,
            records_before,
            records_after: self.records.len(),
            frozen_transactions: frozen.len(),
            discarded_transactions: records_before - self.records.len() - frozen.len(),
            compacted_files: self.base.len(),
            retained_bytes: self.bytes,
            undo: self.applied.last().copied(),
            redo: self.redo.last().copied(),
        })
    }
}

fn ensure_changes(changes: &[Change], label: &str) -> Result<()> {
    ensure!(
        changes.windows(2).all(|pair| pair[0].path < pair[1].path),
        "{label} has unordered or duplicate paths."
    );
    ensure!(
        changes
            .iter()
            .all(|change| !change.path.as_os_str().is_empty() && change.before != change.after),
        "{label} has an empty or unchanged path."
    );
    Ok(())
}

fn merge(changes: &mut BTreeMap<PathBuf, Change>, next: &[Change]) {
    for change in next {
        changes
            .entry(change.path.clone())
            .and_modify(|combined| combined.after = change.after.clone())
            .or_insert_with(|| change.clone());
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

        let third = vec![
            change("a.rs", Some("three"), Some("four")),
            change("b.rs", Some("stable"), Some("changed")),
        ];
        history.ensure_capacity(&third).unwrap();
        crate::vfs::write("a.rs", "four").unwrap();
        crate::vfs::write("b.rs", "changed").unwrap();
        history.push(third);
        crate::vfs::write("b.rs", "outside edit").unwrap();
        assert!(history
            .undo(3)
            .unwrap_err()
            .to_string()
            .contains("preserving"));
        assert_eq!(current(Path::new("a.rs")).unwrap().as_deref(), Some("four"));
        assert_eq!(
            current(Path::new("b.rs")).unwrap().as_deref(),
            Some("outside edit")
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
