use super::{apply::IndexLock, inventory, process, status, Blob, Entry};
use crate::history::{Action, Status};
use anyhow::{ensure, Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Show {
        id: u64,
    },
    Undo(Replay),
    Redo(Replay),
    Recover(Recovery),
    Compact(Compaction),
}

#[derive(Args)]
pub struct Replay {
    id: u64,
    #[command(flatten)]
    options: Recovery,
}

#[derive(Args)]
pub struct Recovery {
    #[arg(long)]
    basis: Option<String>,
    #[arg(long, requires = "basis")]
    write: bool,
}

#[derive(Args)]
pub struct Compaction {
    #[arg(
        long,
        default_value_t = 100,
        help = "Replayable records retained on each stack."
    )]
    keep: usize,
    #[arg(long)]
    basis: Option<String>,
    #[arg(long, requires = "basis")]
    write: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    blob: Blob,
    content: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    path: String,
    before: Option<Stored>,
    after: Option<Stored>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: u64,
    status: Status,
    digest: String,
    changes: Vec<Change>,
    #[serde(default, skip_serializing_if = "is_zero")]
    compacted_paths: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compaction_digest: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    id: u64,
    action: Action,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    root: PathBuf,
    index: PathBuf,
    records: Vec<Record>,
    applied: Vec<u64>,
    redo: Vec<u64>,
    pending: Option<Pending>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

pub(in crate::git) struct Journal {
    path: PathBuf,
    original: Option<Vec<u8>>,
    state: State,
}

fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn regular(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_file(),
                "staging journal must be a regular file."
            );
            Ok(Some(fs::read(path)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_dir(),
                "staging journal requires a real directory."
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl Journal {
    pub(in crate::git) fn read(root: &Path, index: &Path) -> Result<Self> {
        let dir = index
            .parent()
            .context("missing Git index directory")?
            .join("fr-stage");
        directory(&dir)?;
        let path = dir.join("state.json");
        let original = regular(&path)?;
        let state = match &original {
            Some(bytes) => serde_json::from_slice(bytes)
                .context("invalid staging journal; preserve it for recovery")?,
            None => State {
                schema: 1,
                root: root.to_owned(),
                index: index.to_owned(),
                records: vec![],
                applied: vec![],
                redo: vec![],
                pending: None,
            },
        };
        ensure!(
            state.schema == 1 && state.root == root && state.index == index,
            "staging journal belongs to a different root, index or schema."
        );
        let journal = Self {
            path,
            original,
            state,
        };
        journal.validate()?;
        Ok(journal)
    }

    fn validate(&self) -> Result<()> {
        let state = &self.state;
        for (offset, record) in state.records.iter().enumerate() {
            let detailed = !record.changes.is_empty();
            ensure!(
                record.id == offset as u64 + 1
                    && record.digest.len() == 64
                    && record.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && (!detailed || record.digest == digest(&(record.id, &record.changes))?),
                "invalid staging record identity or digest."
            );
            ensure!(
                (detailed
                    && record.compacted_paths == 0
                    && record.compaction_digest.is_none()
                    && record.changes.len() <= 32)
                    || (!detailed
                        && (1..=32).contains(&record.compacted_paths)
                        && record.compaction_digest.as_deref()
                            == Some(
                                digest(&(
                                    record.id,
                                    record.status,
                                    &record.digest,
                                    record.compacted_paths,
                                ))?
                                .as_str(),
                            )
                        && record.status != Status::Planned),
                "invalid staging record payload or compaction summary."
            );
            let mut previous: Option<&str> = None;
            for change in &record.changes {
                let path = Path::new(&change.path);
                ensure!(
                    !change.path.is_empty()
                        && !change.path.contains('\0')
                        && path
                            .components()
                            .all(|part| matches!(part, Component::Normal(_)))
                        && path.components().collect::<PathBuf>() == path
                        && previous.is_none_or(|previous| previous < change.path.as_str()),
                    "invalid staging record paths."
                );
                previous = Some(&change.path);
                ensure!(
                    change.before.as_ref().map(|s| &s.blob)
                        != change.after.as_ref().map(|s| &s.blob),
                    "unchanged staging journal entry."
                );
                for stored in [&change.before, &change.after].into_iter().flatten() {
                    ensure!(
                        matches!(stored.blob.mode.as_str(), "100644" | "100755")
                            && process::oid(&stored.blob.oid),
                        "invalid staging journal blob."
                    );
                }
            }
        }
        let mut active = BTreeSet::new();
        for (stack, status) in [
            (&state.applied, Status::Applied),
            (&state.redo, Status::Undone),
        ] {
            for id in stack {
                ensure!(
                    active.insert(*id)
                        && self.record(*id)?.status == status
                        && !self.record(*id)?.changes.is_empty(),
                    "invalid staging history stacks."
                );
            }
        }
        for record in &state.records {
            ensure!(
                record.changes.is_empty()
                    || !matches!(record.status, Status::Applied | Status::Undone)
                    || active.contains(&record.id),
                "staging record missing from stack."
            );
            ensure!(
                record.status != Status::Planned
                    || state.pending.as_ref().is_some_and(
                        |pending| pending.id == record.id && pending.action == Action::Apply
                    ),
                "orphaned staging plan."
            );
        }
        if let Some(pending) = &state.pending {
            self.check_action(pending.action, pending.id)?;
        }
        Ok(())
    }

    fn record(&self, id: u64) -> Result<&Record> {
        self.state
            .records
            .iter()
            .find(|record| record.id == id)
            .context("unknown staging transaction")
    }

    pub(in crate::git) fn ready(&self) -> Result<()> {
        ensure!(
            self.state.pending.is_none(),
            "staging transaction needs recovery; inspect `fr git stage-history recover`."
        );
        Ok(())
    }

    pub(in crate::git) fn check(&self) -> Result<()> {
        directory(
            self.path
                .parent()
                .context("missing staging journal directory")?,
        )?;
        ensure!(
            regular(&self.path)? == self.original,
            "staging journal changed during operation; retry inspection."
        );
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        self.check()?;
        let dir = self
            .path
            .parent()
            .context("missing staging journal directory")?;
        if !directory(dir)? {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().mode(0o700).create(dir)?;
            }
            #[cfg(not(unix))]
            fs::create_dir(dir)?;
            File::open(dir.parent().context("missing Git directory")?)?.sync_all()?;
        }
        let bytes = serde_json::to_vec(&self.state)?;
        let mut temporary = tempfile::NamedTempFile::new_in(dir)?;
        use std::io::Write;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        self.check()?;
        temporary.persist(&self.path)?;
        self.original = Some(bytes);
        File::open(dir)?.sync_all()?;
        Ok(())
    }

    pub(super) fn plan(&mut self, root: &Path, entries: &[Entry]) -> Result<u64> {
        self.ready()?;
        let paths = entries
            .iter()
            .filter(|entry| entry.action != "unchanged")
            .map(|entry| entry.path.clone())
            .collect();
        require_plain_entries(root, &paths)?;
        let mut changes = Vec::new();
        for entry in entries.iter().filter(|entry| entry.action != "unchanged") {
            let before = entry
                .before
                .as_ref()
                .map(|blob| -> Result<Stored> {
                    let content = process::checked(
                        root,
                        &["cat-file".into(), "blob".into(), blob.oid.clone().into()],
                    )?;
                    let hashed = process::run(
                        root,
                        &[
                            "hash-object".into(),
                            "--no-filters".into(),
                            "--stdin".into(),
                        ],
                        Some(&content),
                    )?;
                    ensure!(
                        hashed.status.success()
                            && hashed.stdout == format!("{}\n", blob.oid).as_bytes(),
                        "recorded staging blob does not match its identity."
                    );
                    Ok(Stored {
                        blob: blob.clone(),
                        content,
                    })
                })
                .transpose()?;
            let after = entry
                .after
                .as_ref()
                .map(|blob| -> Result<Stored> {
                    Ok(Stored {
                        blob: blob.clone(),
                        content: entry.source.clone().context("missing staging source")?,
                    })
                })
                .transpose()?;
            changes.push(Change {
                path: entry.path.clone(),
                before,
                after,
            });
        }
        let id = u64::try_from(self.state.records.len())?
            .checked_add(1)
            .context("staging identity overflow")?;
        self.state.records.push(Record {
            id,
            status: Status::Planned,
            digest: digest(&(id, &changes))?,
            changes,
            compacted_paths: 0,
            compaction_digest: None,
        });
        Ok(id)
    }

    fn check_action(&self, action: Action, id: u64) -> Result<()> {
        let record = self.record(id)?;
        let allowed = match action {
            Action::Apply => record.status == Status::Planned,
            Action::Undo => self.state.applied.last() == Some(&id),
            Action::Redo => self.state.redo.last() == Some(&id),
            Action::Recover => false,
        };
        ensure!(
            allowed,
            "staging undo and redo require the top transaction of their stack."
        );
        Ok(())
    }

    pub(super) fn begin(&mut self, action: Action, id: u64) -> Result<()> {
        if action == Action::Recover {
            ensure!(
                self.state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == id),
                "no matching pending staging transaction."
            );
        } else {
            self.ready()?;
            self.check_action(action, id)?;
            self.state.pending = Some(Pending { id, action });
        }
        self.save().context(
            "recording pending staging operation; inspect staging recovery before retrying.",
        )
    }

    pub(super) fn finish(&mut self, action: Action, id: u64) -> Value {
        let status = match action {
            Action::Apply => {
                for record in &mut self.state.records {
                    if self.state.redo.contains(&record.id) {
                        record.status = Status::Abandoned;
                    }
                }
                self.state.redo.clear();
                self.state.applied.push(id);
                Status::Applied
            }
            Action::Undo => {
                self.state.applied.pop();
                self.state.redo.push(id);
                Status::Undone
            }
            Action::Redo => {
                self.state.redo.pop();
                self.state.applied.push(id);
                Status::Applied
            }
            Action::Recover => {
                if self
                    .state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.action == Action::Apply)
                {
                    Status::Abandoned
                } else {
                    self.record(id).expect("validated pending record").status
                }
            }
        };
        self.state
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .expect("validated staging record")
            .status = status;
        self.state.pending = None;
        match self.save() {
            Ok(()) => json!({"id":id,"finalized":true}),
            Err(error) => {
                json!({"id":id,"finalized":false,"warning":format!("index operation completed; inspect staging history and recovery: {error:#}")})
            }
        }
    }

    fn summary(&self, record: &Record, detail: bool) -> Value {
        let compacted = record.changes.is_empty();
        let paths = if compacted {
            record.compacted_paths
        } else {
            record.changes.len()
        };
        let mut value = json!({"id":record.id,"status":record.status,"paths":paths,"digest":record.digest,"compacted":compacted});
        if detail && !compacted {
            value["entries"] = json!(record.changes.iter().map(|change| json!({"path":change.path,
                "before":change.before.as_ref().map(|s| &s.blob),"after":change.after.as_ref().map(|s| &s.blob)})).collect::<Vec<_>>());
        }
        value
    }
}

fn retained(stack: &[u64], keep: usize) -> BTreeSet<u64> {
    stack.iter().rev().take(keep).copied().collect()
}

fn compaction_candidates(state: &State, keep: usize) -> BTreeSet<u64> {
    let retained = retained(&state.applied, keep)
        .into_iter()
        .chain(retained(&state.redo, keep))
        .collect::<BTreeSet<_>>();
    state
        .records
        .iter()
        .filter(|record| {
            crate::git::staging_record_compactable(
                !record.changes.is_empty(),
                state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == record.id),
                retained.contains(&record.id),
            )
        })
        .map(|record| record.id)
        .collect()
}

fn compact_state(state: &mut State, candidates: &BTreeSet<u64>) -> Result<()> {
    state.applied.retain(|id| !candidates.contains(id));
    state.redo.retain(|id| !candidates.contains(id));
    for record in &mut state.records {
        if candidates.contains(&record.id) {
            record.compacted_paths = record.changes.len();
            record.changes.clear();
            record.compaction_digest = Some(digest(&(
                record.id,
                record.status,
                &record.digest,
                record.compacted_paths,
            ))?);
        }
    }
    Ok(())
}

fn compact(root: &Path, options: &Compaction) -> Result<Value> {
    ensure!(
        options.keep <= 10_000,
        "staging history keep count must be 0 through 10000."
    );
    let root = process::repository_root(&root.canonicalize()?)?;
    let _lock = options
        .write
        .then(|| IndexLock::acquire(&root))
        .transpose()?;
    let index = super::apply::index_path(&root)?;
    let mut journal = Journal::read(&root, &index)?;
    journal.ready()?;
    let candidates = compaction_candidates(&journal.state, options.keep);
    let paths = candidates
        .iter()
        .map(|id| journal.record(*id).map(|record| record.changes.len()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum::<usize>();
    let before_bytes = journal.original.as_ref().map_or(0, Vec::len);
    let mut compacted = journal.state.clone();
    compact_state(&mut compacted, &candidates)?;
    let after_bytes = serde_json::to_vec(&compacted)?.len();
    let basis = format!(
        "frstagecompact1:{}",
        digest(&(&journal.state, options.keep, &candidates))?
    );
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale staging compaction basis; request a new preview."
        );
    }
    journal.check()?;
    let mut result = json!({"schema":1,"repository_root":root,
        "operation":if options.write{"stage-history-compact"}else{"stage-history-compact-preview"},
        "basis":basis,"basis_verified":options.basis.is_some(),"applied":false,
        "keep_per_stack":options.keep,"records_compacted":candidates.len(),"paths_compacted":paths,
        "journal_bytes_before":before_bytes,"journal_bytes_after":after_bytes,
        "can_compact":!candidates.is_empty(),"undo_after":compacted.applied.last(),
        "redo_after":compacted.redo.last(),"source_bodies":"omitted"});
    if !options.write {
        return Ok(result);
    }
    ensure!(
        !candidates.is_empty(),
        "staging journal has no replay payloads eligible for compaction."
    );
    journal.state = compacted;
    match journal.save() {
        Ok(()) => result["applied"] = json!(true),
        Err(error) => {
            result["applied"] = Value::Null;
            result["can_compact"] = Value::Null;
            result["warning"] = json!("Staging compaction is incomplete or unconfirmed. Inspect staging history before retrying.");
            let message = format!("{error:#}");
            result["diagnostic"] = json!(crate::git::process::diagnostic(message.as_bytes()));
            result["diagnostic_truncated"] = json!(message.len() > 16 * 1024);
        }
    }
    Ok(result)
}

pub(in crate::git) fn require_plain_entries(root: &Path, paths: &BTreeSet<String>) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec![
        "--literal-pathspecs".into(),
        "ls-files".into(),
        "-v".into(),
        "-z".into(),
        "--".into(),
    ];
    args.extend(paths.iter().map(Into::into));
    let rows = process::checked(root, &args)?;
    ensure!(
        status::records(&rows)?
            .iter()
            .all(|row| row.starts_with(b"H ")),
        "selected staging history entries must not use assume-unchanged or skip-worktree flags."
    );
    let diff = |visibility: &str| -> Result<Vec<u8>> {
        let mut args = vec![
            "--literal-pathspecs".into(),
            "diff".into(),
            "--cached".into(),
            "--name-only".into(),
            "-z".into(),
            "--no-renames".into(),
            "--no-ext-diff".into(),
            "--no-textconv".into(),
            visibility.into(),
            "--".into(),
        ];
        args.extend(paths.iter().map(Into::into));
        process::checked(root, &args)
    };
    ensure!(
        diff("--ita-visible-in-index")? == diff("--ita-invisible-in-index")?,
        "selected intent-to-add entries are unsupported for staging history."
    );
    Ok(())
}

pub(in crate::git) fn report(root: &Path, command: &Command) -> Result<Value> {
    if let Command::Compact(options) = command {
        return compact(root, options);
    }
    let root = process::repository_root(&root.canonicalize()?)?;
    let (action, id, options) = match command {
        Command::Undo(replay) => (Action::Undo, Some(replay.id), Some(&replay.options)),
        Command::Redo(replay) => (Action::Redo, Some(replay.id), Some(&replay.options)),
        Command::Recover(options) => (Action::Recover, None, Some(options)),
        _ => (Action::Recover, None, None),
    };
    let lock = options
        .filter(|options| options.write)
        .map(|_| IndexLock::acquire(&root))
        .transpose()?;
    let index = super::apply::index_path(&root)?;
    let mut journal = Journal::read(&root, &index)?;
    let Some(options) = options else {
        let mut result = json!({"schema":1,"repository_root":root,"operation":"stage-history","source_bodies":"omitted","pending":journal.state.pending,
            "undo":journal.state.applied.last(),"redo":journal.state.redo.last(),"total":journal.state.records.len()});
        match command {
            Command::List { limit } => {
                ensure!(
                    (1..=500).contains(limit),
                    "staging history limit must be 1 through 500."
                );
                result["records"] = json!(journal
                    .state
                    .records
                    .iter()
                    .rev()
                    .take(*limit)
                    .map(|record| journal.summary(record, false))
                    .collect::<Vec<_>>());
            }
            Command::Show { id } => {
                result["record"] = journal.summary(journal.record(*id)?, true);
            }
            _ => unreachable!(),
        }
        journal.check()?;
        return Ok(result);
    };
    let id = if action == Action::Recover {
        journal
            .state
            .pending
            .as_ref()
            .context("no pending staging operation")?
            .id
    } else {
        journal.ready()?;
        let id = id.context("missing staging identity")?;
        journal.check_action(action, id)?;
        id
    };
    let pending_action = journal.state.pending.as_ref().map(|pending| pending.action);
    let record = journal.record(id)?;
    let reversed = action == Action::Undo
        || (action == Action::Recover && pending_action != Some(Action::Undo));
    let changes = &record.changes;
    let paths = changes.iter().map(|change| change.path.clone()).collect();
    require_plain_entries(&root, &paths)?;
    let current = inventory(&root, &paths, None)?;
    let mut entries = Vec::new();
    let mut matches_before = true;
    let mut matches_after = true;
    for change in changes {
        let (before, after) = if reversed {
            (&change.after, &change.before)
        } else {
            (&change.before, &change.after)
        };
        let observed = current.get(&change.path).cloned();
        matches_before &= observed.as_ref() == before.as_ref().map(|stored| &stored.blob);
        matches_after &= observed.as_ref() == after.as_ref().map(|stored| &stored.blob);
        let target = after.as_ref().map(|stored| stored.blob.clone());
        entries.push(Entry {
            path: change.path.clone(),
            action: match (&observed, &target) {
                (None, Some(_)) => "add",
                (Some(_), None) => "remove",
                _ if observed == target => "unchanged",
                _ => "update",
            },
            before: observed,
            after: target,
            working_bytes: None,
            source: after.as_ref().map(|stored| stored.content.clone()),
        });
    }
    ensure!(
        crate::git::staging_transition_allowed(
            matches_before,
            matches_after,
            action == Action::Recover
        ),
        "selected index differs from the recorded staging basis; preserve external changes."
    );
    let basis = format!(
        "frstagehistory1:{}",
        digest(&(&journal.state, action, id, &current))?
    );
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale staging history basis; request a new preview."
        );
    }
    ensure!(
        inventory(&root, &paths, None)? == current,
        "selected index changed during staging history inspection."
    );
    require_plain_entries(&root, &paths)?;
    journal.check()?;
    let outcome = if let Some(lock) = lock {
        Some(lock.replay(&root, &entries, &mut journal, action, id)?)
    } else {
        None
    };
    Ok(
        json!({"schema":1,"repository_root":root,"operation":"stage-history-transition","action":action,"id":id,
        "basis":basis,"basis_verified":options.basis.is_some(),"applied":options.write,"durability":outcome,"source_bodies":"omitted","entries":entries}),
    )
}
