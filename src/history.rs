//! Durable, checked workspace transactions for the native CLI.

use crate::edit::{CommitLocks, FileChange};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

mod patch;
pub use patch::{export_patch, PatchExport};

const DIRECTORY: &str = ".fr-history";

fn workspace(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    Ok(if root.is_file() {
        root.parent()
            .context("file root has no parent")?
            .to_path_buf()
    } else {
        root
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub content: String,
    pub mode: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<Snapshot>,
    pub after: Option<Snapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Planned,
    Applied,
    Undone,
    Abandoned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: u64,
    pub status: Status,
    pub basis: String,
    pub source_revision: String,
    pub validation: String,
    pub changes: Vec<Change>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Apply,
    Undo,
    Redo,
    Recover,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub id: u64,
    pub action: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub schema: u32,
    pub root: PathBuf,
    pub records: Vec<Record>,
    pub applied: Vec<u64>,
    pub redo: Vec<u64>,
    pub pending: Option<Pending>,
}

fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(true),
        Ok(_) => bail!("{} is not a regular file", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn snapshot(path: &Path) -> Result<Option<Snapshot>> {
    if !regular(path)? {
        return Ok(None);
    }
    Ok(Some(Snapshot {
        content: crate::vfs::read_to_string(path)
            .with_context(|| format!("reading snapshot for {}", path.display()))?,
        mode: fs::metadata(path)?.permissions().mode() & 0o7777,
    }))
}

fn directory(root: &Path) -> Result<PathBuf> {
    let dir = root.join(DIRECTORY);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => (),
        Ok(_) => bail!("{} must be a real directory", dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    Ok(dir)
}

fn target(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || relative
            .components()
            .any(|part| matches!(part.as_os_str().to_str(), Some(".fr-history" | ".git")))
    {
        bail!("invalid history target {}", relative.display());
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("history target traverses a symlink: {}", path.display())
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}

impl History {
    pub fn read(root: &Path) -> Result<Self> {
        let root = workspace(root)?;
        let path = directory(&root)?.join("state.json");
        if !regular(&path)? {
            return Ok(Self {
                schema: 1,
                root,
                records: Vec::new(),
                applied: Vec::new(),
                redo: Vec::new(),
                pending: None,
            });
        }
        let history: Self = serde_json::from_str(&crate::vfs::read_to_string(&path)?)
            .context("reading transaction history; preserve the journal for recovery")?;
        if history.schema != 1 || history.root != root {
            bail!("unsupported history version or different workspace root");
        }
        let mut ids = std::collections::BTreeSet::new();
        for record in &history.records {
            if record.id == 0 || !ids.insert(record.id) || record.basis != basis(&record.changes)? {
                bail!("invalid transaction identity or basis");
            }
            let mut paths = std::collections::BTreeSet::new();
            for change in &record.changes {
                target(&root, &change.path)?;
                if !paths.insert(&change.path) || change.before == change.after {
                    bail!("duplicate or unchanged history target");
                }
                for state in [&change.before, &change.after].into_iter().flatten() {
                    if state.mode > 0o7777 {
                        bail!("invalid recorded file mode");
                    }
                }
            }
        }
        let mut active = std::collections::BTreeSet::new();
        for (stack, status) in [
            (&history.applied, Status::Applied),
            (&history.redo, Status::Undone),
        ] {
            for id in stack {
                if !active.insert(id) || history.record(*id)?.status != status {
                    bail!("invalid transaction stack");
                }
            }
        }
        for record in &history.records {
            if matches!(record.status, Status::Applied | Status::Undone)
                && !active.contains(&record.id)
            {
                bail!("transaction is missing from its stack");
            }
        }
        if let Some(pending) = &history.pending {
            history.check_action(pending.action, pending.id)?;
        }
        Ok(history)
    }

    pub fn record(&self, id: u64) -> Result<&Record> {
        self.records
            .iter()
            .find(|r| r.id == id)
            .context("unknown transaction identity")
    }

    pub fn ensure_ready(&self) -> Result<()> {
        if let Some(pending) = &self.pending {
            bail!(
                "transaction {} needs recovery; run `fr history recover {} --write`",
                pending.id,
                pending.id
            );
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let dir = directory(&self.root)?;
        let mut file = tempfile::Builder::new()
            .prefix(".state-")
            .tempfile_in(&dir)?;
        serde_json::to_writer(&mut file, self)?;
        file.as_file().sync_all()?;
        file.persist(dir.join("state.json"))?;
        File::open(dir)?.sync_all()?;
        Ok(())
    }

    fn check_action(&self, action: Action, id: u64) -> Result<()> {
        let record = self.record(id)?;
        let valid = match action {
            Action::Apply => record.status == Status::Planned,
            Action::Undo => self.applied.last() == Some(&id),
            Action::Redo => self.redo.last() == Some(&id),
            Action::Recover => false,
        };
        if !valid {
            bail!(
                "transaction {id} cannot {action:?} from {:?}; undo and redo follow stack order",
                record.status
            );
        }
        Ok(())
    }

    fn finish(&mut self, action: Action, id: u64) {
        match action {
            Action::Apply => {
                for record in &mut self.records {
                    if self.redo.contains(&record.id) {
                        record.status = Status::Abandoned;
                    }
                }
                self.redo.clear();
                self.applied.push(id);
            }
            Action::Redo => {
                self.redo.pop();
                self.applied.push(id);
            }
            Action::Undo => {
                self.applied.pop();
                self.redo.push(id);
            }
            Action::Recover => unreachable!(),
        }
        self.records.iter_mut().find(|r| r.id == id).unwrap().status = if action == Action::Undo {
            Status::Undone
        } else {
            Status::Applied
        };
        self.pending = None;
    }
}

fn basis(changes: &[Change]) -> Result<String> {
    let source = changes
        .iter()
        .map(|c| (&c.path, &c.before))
        .collect::<Vec<_>>();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&source)?)
    ))
}

fn source_revision(root: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".fr-history" | ".git" | "target" | "node_modules" | ".lake")
            )
        })
        .build();
    let mut paths = Vec::new();
    for entry in walker {
        let entry = entry?;
        if entry.file_type().is_some_and(|t| t.is_file())
            && crate::lang::detect(entry.path()).is_some()
        {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    for file in paths {
        let path = file.strip_prefix(root)?.to_string_lossy();
        let content = crate::vfs::read_to_string(&file)
            .with_context(|| format!("hashing project source {}", file.display()))?;
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        digest.update((content.len() as u64).to_le_bytes());
        digest.update(content.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn lock(root: &Path) -> Result<File> {
    let dir = directory(root)?;
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    File::open(root)?.sync_all()?;
    let path = dir.join("lock");
    regular(&path)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    let ignore = dir.join(".gitignore");
    if !regular(&ignore)? {
        let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
        temp.write_all(b"*\n")?;
        temp.as_file().sync_all()?;
        temp.persist(ignore)?;
    }
    Ok(file)
}

fn sync_ancestors(root: &Path, directory: &Path) -> Result<()> {
    for parent in directory.ancestors() {
        File::open(parent)?.sync_all()?;
        if parent == root {
            break;
        }
    }
    Ok(())
}

pub fn record(
    root: &Path,
    changes: &[FileChange<'_>],
    apply: bool,
    validation: &str,
) -> Result<Option<u64>> {
    if !changes.iter().any(|c| c.original != c.updated) {
        return Ok(None);
    }
    let requested_root = std::path::absolute(root)?;
    let requested_root = if requested_root.is_file() {
        requested_root.parent().unwrap().to_path_buf()
    } else {
        requested_root
    };
    let root = workspace(root)?;
    let _lock = lock(&root)?;
    let mut history = History::read(&root)?;
    history.ensure_ready()?;
    let mut paths = Vec::new();
    for change in changes.iter().filter(|c| c.original != c.updated) {
        let absolute = std::path::absolute(change.path)?;
        let relative = absolute
            .strip_prefix(&root)
            .or_else(|_| absolute.strip_prefix(&requested_root))
            .context("history targets must be inside the workspace root; choose -C accordingly")?;
        paths.push(target(&root, relative)?);
    }
    let guards = paths
        .iter()
        .map(|path| FileChange {
            path,
            original: "",
            updated: "lock",
        })
        .collect::<Vec<_>>();
    let _locks = CommitLocks::acquire(&guards)?;
    for path in &paths {
        sync_ancestors(&root, path.parent().unwrap())?;
    }
    let mut stored = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (change, path) in changes
        .iter()
        .filter(|c| c.original != c.updated)
        .zip(paths)
    {
        if !seen.insert(path.clone()) {
            bail!("duplicate history target {}", path.display());
        }
        let before = snapshot(&path)?;
        if before.as_ref().map(|s| s.content.as_str()).unwrap_or("") != change.original {
            bail!("{} changed after planning", path.display());
        }
        let after = Some(Snapshot {
            content: change.updated.to_owned(),
            mode: before.as_ref().map_or(0o600, |s| s.mode),
        });
        stored.push(Change {
            path: path.strip_prefix(&root)?.to_path_buf(),
            before,
            after,
        });
    }
    let id = history
        .records
        .iter()
        .map(|r| r.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("transaction identities exhausted")?;
    history.records.push(Record {
        id,
        status: Status::Planned,
        basis: basis(&stored)?,
        source_revision: source_revision(&root)?,
        validation: validation.to_owned(),
        changes: stored,
    });
    history.save()?;
    if apply {
        transition(&mut history, Action::Apply, id)?;
    }
    Ok(Some(id))
}

pub fn act(root: &Path, action: Action, id: u64, write: bool) -> Result<serde_json::Value> {
    let root = workspace(root)?;
    let _lock = if write { Some(lock(&root)?) } else { None };
    let mut history = History::read(&root)?;
    let effective = if action == Action::Recover {
        let pending = history
            .pending
            .as_ref()
            .context("no interrupted transaction to recover")?;
        if pending.id != id {
            bail!("transaction {id} is not the pending transaction");
        }
        pending.action
    } else {
        history.ensure_ready()?;
        history.check_action(action, id)?;
        action
    };
    let changes = oriented(&history, effective, id)?;
    if action == Action::Apply && source_revision(&root)? != history.record(id)?.source_revision {
        bail!("project source changed after planning; create a fresh plan");
    }
    let paths = changes
        .iter()
        .map(|c| target(&root, &c.path))
        .collect::<Result<Vec<_>>>()?;
    let guards = paths
        .iter()
        .map(|path| FileChange {
            path,
            original: "",
            updated: "lock",
        })
        .collect::<Vec<_>>();
    let _locks = if write {
        Some(CommitLocks::acquire(&guards)?)
    } else {
        None
    };
    check(&root, &changes, action == Action::Recover)?;
    let changes = if action == Action::Recover {
        changes
            .into_iter()
            .map(|c| {
                Ok(Change {
                    before: snapshot(&target(&root, &c.path)?)?,
                    after: c.before,
                    path: c.path,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        changes
    };
    let report = serde_json::json!({ "transaction": id, "action": action, "applied": write,
        "changes": changes.iter().map(|c| {
            let (before, after) = (&c.before, &c.after);
            serde_json::json!({"path": c.path, "before_exists": before.is_some(), "after_exists": after.is_some(),
                "before_mode": before.as_ref().map(|s| s.mode), "after_mode": after.as_ref().map(|s| s.mode),
                "diff": crate::edit::unified_diff(before.as_ref().map_or("", |s| &s.content), after.as_ref().map_or("", |s| &s.content), &c.path.to_string_lossy())})
        }).collect::<Vec<_>>() });
    if write {
        if action == Action::Recover {
            recover(&mut history)?;
        } else {
            transition(&mut history, action, id)?;
        }
    }
    Ok(report)
}

fn oriented(history: &History, action: Action, id: u64) -> Result<Vec<Change>> {
    Ok(history
        .record(id)?
        .changes
        .iter()
        .map(|c| {
            if action == Action::Undo {
                Change {
                    path: c.path.clone(),
                    before: c.after.clone(),
                    after: c.before.clone(),
                }
            } else {
                c.clone()
            }
        })
        .collect())
}

fn check(root: &Path, changes: &[Change], recovery: bool) -> Result<()> {
    for change in changes {
        let current = snapshot(&target(root, &change.path)?)?;
        if !matches_snapshot(&current, &change.before, &change.after, recovery) {
            bail!(
                "{} conflicts with transaction snapshots; preserving current files",
                change.path.display()
            );
        }
    }
    Ok(())
}

pub fn matches_snapshot(
    current: &Option<Snapshot>,
    before: &Option<Snapshot>,
    after: &Option<Snapshot>,
    recovery: bool,
) -> bool {
    current == before || (recovery && current == after)
}

fn install(root: &Path, changes: &[Change]) -> Result<()> {
    let mut staged = Vec::new();
    for change in changes {
        let path = target(root, &change.path)?;
        let dir = path.parent().context("target has no parent")?;
        fs::create_dir_all(dir)?;
        sync_ancestors(root, dir)?;
        let replacement = if let Some(after) = &change.after {
            let mut temp = tempfile::Builder::new()
                .prefix(".fr-history-stage-")
                .tempfile_in(dir)?;
            temp.write_all(after.content.as_bytes())?;
            temp.as_file()
                .set_permissions(fs::Permissions::from_mode(after.mode))?;
            temp.as_file().sync_all()?;
            Some(temp.into_temp_path())
        } else {
            None
        };
        staged.push((path, replacement));
    }
    check(root, changes, false)?;
    for (change, (path, replacement)) in changes.iter().zip(staged) {
        check(root, std::slice::from_ref(change), false)?;
        match replacement {
            Some(temp) => fs::rename(&temp, &path)?,
            None => fs::remove_file(&path)?,
        }
        File::open(path.parent().unwrap())?.sync_all()?;
        #[cfg(test)]
        fault()?;
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAULT: std::cell::Cell<Option<(usize, bool)>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn fault() -> Result<()> {
    FAULT.with(|fault| match fault.take() {
        Some((0, true)) => std::process::exit(77),
        Some((0, false)) => Err(std::io::Error::other("injected storage failure").into()),
        Some((remaining, crash)) => {
            fault.set(Some((remaining - 1, crash)));
            Ok(())
        }
        None => Ok(()),
    })
}

fn recover(history: &mut History) -> Result<()> {
    let pending = history.pending.as_ref().context("no pending transaction")?;
    let changes = oriented(history, pending.action, pending.id)?;
    check(&history.root, &changes, true)?;
    let mut inverse = Vec::new();
    for change in changes.into_iter().rev() {
        if snapshot(&target(&history.root, &change.path)?)? == change.after {
            inverse.push(Change {
                path: change.path,
                before: change.after,
                after: change.before,
            });
        }
    }
    install(&history.root, &inverse)?;
    history.pending = None;
    history.save()
}

fn transition(history: &mut History, action: Action, id: u64) -> Result<()> {
    history.ensure_ready()?;
    history.check_action(action, id)?;
    if action == Action::Apply
        && source_revision(&history.root)? != history.record(id)?.source_revision
    {
        bail!("project source changed after planning; create a fresh plan");
    }
    let changes = oriented(history, action, id)?;
    check(&history.root, &changes, false)?;
    history.pending = Some(Pending { id, action });
    history.save()?;
    if let Err(error) = install(&history.root, &changes) {
        return match recover(history) {
            Ok(()) => Err(error.context(format!("transaction {id} failed; restored its starting state"))),
            Err(recovery) => Err(error.context(format!("transaction {id} needs recovery: {recovery:#}; run `fr history recover {id} --write`"))),
        };
    }
    history.finish(action, id);
    history.save().with_context(|| format!("transaction {id} changed source but could not confirm durable finalization. Inspect `fr history` and recover {id} if pending."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> Vec<(PathBuf, String, String)> {
        [Some("before λ\n"), None, Some("")]
            .into_iter()
            .enumerate()
            .map(|(n, original)| {
                let path = root.join(format!("{n}.txt"));
                if let Some(text) = original {
                    fs::write(&path, text).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
                }
                (
                    path,
                    original.unwrap_or("").to_owned(),
                    format!("after {n} 名\n"),
                )
            })
            .collect()
    }

    fn save(root: &Path, fixture: &[(PathBuf, String, String)], apply: bool) -> u64 {
        let changes = fixture
            .iter()
            .map(|(path, original, updated)| FileChange {
                path,
                original,
                updated,
            })
            .collect::<Vec<_>>();
        record(root, &changes, apply, "fixture").unwrap().unwrap()
    }

    fn assert_before(root: &Path) {
        assert_eq!(
            snapshot(&root.join("0.txt")).unwrap(),
            Some(Snapshot {
                content: "before λ\n".to_owned(),
                mode: 0o751
            })
        );
        assert_eq!(snapshot(&root.join("1.txt")).unwrap(), None);
        assert_eq!(
            snapshot(&root.join("2.txt")).unwrap(),
            Some(Snapshot {
                content: String::new(),
                mode: 0o751
            })
        );
    }

    #[test]
    fn apply_undo_redo_preserve_existence_content_and_modes() {
        let dir = tempfile::tempdir().unwrap();
        let files = fixture(dir.path());
        let id = save(dir.path(), &files, false);
        assert_before(dir.path());
        act(dir.path(), Action::Apply, id, false).unwrap();
        assert_before(dir.path());
        act(dir.path(), Action::Apply, id, true).unwrap();
        fs::write(dir.path().join("unrelated.txt"), "dirty").unwrap();
        act(dir.path(), Action::Undo, id, true).unwrap();
        assert_before(dir.path());
        act(dir.path(), Action::Redo, id, true).unwrap();
        for (path, _, after) in files {
            assert_eq!(fs::read_to_string(path).unwrap(), after);
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("unrelated.txt")).unwrap(),
            "dirty"
        );
        assert_eq!(History::read(dir.path()).unwrap().applied, [id]);
    }

    #[test]
    fn later_content_mode_and_existence_changes_refuse_without_partial_writes() {
        for kind in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let files = fixture(dir.path());
            let id = save(dir.path(), &files, true);
            let path = dir.path().join("2.txt");
            match kind {
                0 => fs::write(&path, "user edit").unwrap(),
                1 => fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap(),
                _ => fs::remove_file(&path).unwrap(),
            }
            let current = snapshot(&path).unwrap();
            assert!(act(dir.path(), Action::Undo, id, true).is_err());
            assert_eq!(snapshot(&path).unwrap(), current);
            assert_eq!(fs::read_to_string(&files[0].0).unwrap(), files[0].2);
            assert!(History::read(dir.path()).unwrap().pending.is_none());
        }
    }

    #[test]
    fn new_apply_abandons_redo_but_saving_a_plan_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let files = fixture(dir.path());
        let first = save(dir.path(), &files, true);
        act(dir.path(), Action::Undo, first, true).unwrap();
        let second = save(dir.path(), &files, false);
        assert_eq!(History::read(dir.path()).unwrap().redo, [first]);
        act(dir.path(), Action::Apply, second, true).unwrap();
        assert!(act(dir.path(), Action::Redo, first, true).is_err());
        let history = History::read(dir.path()).unwrap();
        assert_eq!(history.record(first).unwrap().status, Status::Abandoned);
        assert_eq!(history.applied, [second]);
    }

    #[test]
    fn every_handled_write_failure_restores_the_starting_state() {
        for index in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let files = fixture(dir.path());
            let id = save(dir.path(), &files, false);
            FAULT.with(|fault| fault.set(Some((index, false))));
            assert!(act(dir.path(), Action::Apply, id, true).is_err());
            assert_before(dir.path());
            let history = History::read(dir.path()).unwrap();
            assert!(history.pending.is_none());
            assert_eq!(history.record(id).unwrap().status, Status::Planned);
        }
    }

    #[test]
    fn interrupted_process_recovers_apply_undo_and_redo_at_every_write_boundary() {
        const CHILD: &str = "FR_HISTORY_CRASH_ROOT";
        if let Ok(root) = std::env::var(CHILD) {
            let root = Path::new(&root);
            let index = std::env::var("FR_HISTORY_CRASH_INDEX")
                .unwrap()
                .parse()
                .unwrap();
            let action = match std::env::var("FR_HISTORY_CRASH_ACTION").unwrap().as_str() {
                "undo" => Action::Undo,
                "redo" => Action::Redo,
                _ => Action::Apply,
            };
            FAULT.with(|fault| fault.set(Some((index, true))));
            act(root, action, 1, true).unwrap();
            panic!("crash point did not run");
        }
        for action in [Action::Apply, Action::Undo, Action::Redo] {
            for index in 0..3 {
                let dir = tempfile::tempdir().unwrap();
                let files = fixture(dir.path());
                let id = save(dir.path(), &files, action != Action::Apply);
                if action == Action::Redo {
                    act(dir.path(), Action::Undo, id, true).unwrap();
                }
                let before = files
                    .iter()
                    .map(|(path, _, _)| snapshot(path).unwrap())
                    .collect::<Vec<_>>();
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "history::tests::interrupted_process_recovers_apply_undo_and_redo_at_every_write_boundary", "--nocapture"])
                    .env(CHILD, dir.path()).env("FR_HISTORY_CRASH_INDEX", index.to_string())
                    .env("FR_HISTORY_CRASH_ACTION", format!("{action:?}").to_lowercase()).output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(77),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(History::read(dir.path()).unwrap().pending.is_some());
                assert!(act(dir.path(), Action::Apply, id, true).is_err());
                act(dir.path(), Action::Recover, id, false).unwrap();
                act(dir.path(), Action::Recover, id, true).unwrap();
                assert!(History::read(dir.path()).unwrap().pending.is_none());
                for ((path, _, _), expected) in files.iter().zip(before) {
                    assert_eq!(snapshot(path).unwrap(), expected);
                }
            }
        }
    }

    fn interrupted(root: &Path) -> u64 {
        let files = fixture(root);
        let id = save(root, &files, false);
        let mut history = History::read(root).unwrap();
        history.pending = Some(Pending {
            id,
            action: Action::Apply,
        });
        history.save().unwrap();
        install(&history.root, &history.record(id).unwrap().changes[..2]).unwrap();
        id
    }

    #[test]
    fn conflicting_recovery_preserves_all_files_and_can_retry_after_manual_repair() {
        let dir = tempfile::tempdir().unwrap();
        let id = interrupted(dir.path());
        let path = dir.path().join("1.txt");
        let expected = fs::read_to_string(&path).unwrap();
        fs::write(&path, "user change").unwrap();
        assert!(act(dir.path(), Action::Recover, id, true).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("0.txt")).unwrap(),
            "after 0 名\n"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "user change");
        fs::write(&path, expected).unwrap();
        act(dir.path(), Action::Recover, id, true).unwrap();
        assert_before(dir.path());
    }

    #[test]
    fn recovery_itself_can_fail_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let id = interrupted(dir.path());
        let preview = act(dir.path(), Action::Recover, id, false).unwrap();
        assert_eq!(preview["changes"][2]["diff"], "");
        assert_eq!(preview["changes"][1]["before_exists"], true);
        assert_eq!(preview["changes"][1]["after_exists"], false);
        FAULT.with(|fault| fault.set(Some((0, false))));
        assert!(act(dir.path(), Action::Recover, id, true).is_err());
        assert!(History::read(dir.path()).unwrap().pending.is_some());
        act(dir.path(), Action::Recover, id, true).unwrap();
        assert_before(dir.path());
    }

    #[test]
    fn saved_plan_refuses_changes_to_other_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let files = fixture(dir.path());
        fs::write(dir.path().join("other.rs"), "fn before() {}\n").unwrap();
        let id = save(dir.path(), &files, false);
        fs::write(dir.path().join("other.rs"), "fn after() {}\n").unwrap();
        assert!(act(dir.path(), Action::Apply, id, false).is_err());
        assert!(act(dir.path(), Action::Apply, id, true).is_err());
        assert_before(dir.path());
    }

    #[test]
    fn unsupported_or_corrupt_history_never_writes_source() {
        let dir = tempfile::tempdir().unwrap();
        let files = fixture(dir.path());
        let id = save(dir.path(), &files, false);
        let path = dir.path().join(DIRECTORY).join("state.json");
        let original = fs::read_to_string(&path).unwrap();
        for mutation in ["version", "escape", "stack", "basis"] {
            let mut state: serde_json::Value = serde_json::from_str(&original).unwrap();
            match mutation {
                "version" => state["schema"] = 200.into(),
                "escape" => state["records"][0]["changes"][0]["path"] = "../outside".into(),
                "stack" => state["applied"] = serde_json::json!([id]),
                _ => state["records"][0]["basis"] = "invalid".into(),
            }
            fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();
            assert!(act(dir.path(), Action::Apply, id, true).is_err());
            assert_before(dir.path());
        }
    }

    #[test]
    fn reads_and_previews_do_not_create_history() {
        let dir = tempfile::tempdir().unwrap();
        assert!(History::read(dir.path()).unwrap().records.is_empty());
        assert!(act(dir.path(), Action::Apply, 1, false).is_err());
        assert!(!dir.path().join(DIRECTORY).exists());
    }

    #[test]
    fn saved_absent_file_does_not_overwrite_a_later_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let files = fixture(dir.path());
        let id = save(dir.path(), &files, false);
        fs::write(dir.path().join("1.txt"), "").unwrap();
        assert!(act(dir.path(), Action::Apply, id, true).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("0.txt")).unwrap(),
            "before λ\n"
        );
        assert_eq!(fs::read_to_string(dir.path().join("1.txt")).unwrap(), "");
    }

    #[test]
    fn symlink_and_outside_targets_refuse_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("source.txt");
        fs::write(&external, "original").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("alias")).unwrap();
        for path in [external.clone(), dir.path().join("alias/source.txt")] {
            assert!(record(
                dir.path(),
                &[FileChange {
                    path: &path,
                    original: "original",
                    updated: "updated"
                }],
                true,
                "fixture"
            )
            .is_err());
            assert_eq!(fs::read_to_string(&external).unwrap(), "original");
            assert!(History::read(dir.path()).unwrap().records.is_empty());
        }
    }
}
