use super::{
    check, lock, snapshot, store_record, sync_ancestors, target, workspace, Change, History,
};
use crate::edit::{CommitLocks, FileChange};
use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Record deletion of explicit regular text files.")]
    Delete {
        #[arg(required = true, help = "Workspace-relative file paths, at most 500.")]
        paths: Vec<PathBuf>,
        #[arg(long, help = "Apply the transaction after checking its snapshots.")]
        write: bool,
    },
    #[command(about = "Record an owner-execute permission change without changing content.")]
    Executable {
        #[arg(required = true, help = "Workspace-relative file paths, at most 500.")]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, help = "Set or clear only the owner-execute bit.")]
        set: Executable,
        #[arg(long, help = "Apply the transaction after checking its snapshots.")]
        write: bool,
    },
}

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Executable {
    On,
    Off,
}

fn plan(root: &Path, paths: &[PathBuf], executable: Option<Executable>) -> Result<Vec<Change>> {
    if paths.is_empty() || paths.len() > 500 {
        bail!("file operations require between 1 and 500 explicit paths");
    }
    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    for path in paths {
        path.to_str().context("file paths must use UTF-8")?;
        let absolute = target(root, path)?;
        if !seen.insert(absolute.clone()) {
            bail!("duplicate file target {}", path.display());
        }
        let before = snapshot(&absolute)?
            .with_context(|| format!("file does not exist: {}", path.display()))?;
        if before.content.contains('\0') {
            bail!(
                "file operations require text without NUL bytes: {}",
                path.display()
            );
        }
        let after = executable.map(|executable| {
            let mut after = before.clone();
            after.mode =
                super::owner_executable_mode(after.mode, matches!(executable, Executable::On));
            after
        });
        changes.push(Change {
            path: absolute.strip_prefix(root)?.to_path_buf(),
            before: Some(before),
            after,
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

fn persist(root: &Path, changes: Vec<Change>, apply: bool) -> Result<u64> {
    let _lock = lock(root)?;
    let mut history = History::read(root)?;
    history.ensure_ready()?;
    let paths = changes
        .iter()
        .map(|change| target(root, &change.path))
        .collect::<Result<Vec<_>>>()?;
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
        sync_ancestors(root, path.parent().unwrap())?;
    }
    check(root, &changes, false)?;
    store_record(&mut history, changes, apply, "file-snapshots")
}

pub fn execute(root: &Path, command: &Command, save_plan: bool) -> Result<Value> {
    let (paths, executable, write) = match command {
        Command::Delete { paths, write } => (paths, None, *write),
        Command::Executable { paths, set, write } => (paths, Some(*set), *write),
    };
    if write && save_plan {
        bail!("choose --save-plan or --write, not both");
    }
    let root = workspace(root)?;
    root.to_str().context("workspace root must use UTF-8")?;
    History::read(&root)?.ensure_ready()?;
    let planned = plan(&root, paths, executable)?;
    let entries = planned.iter().map(|change| json!({
        "path": change.path, "changed": change.before != change.after,
        "before_exists": change.before.is_some(), "after_exists": change.after.is_some(),
        "before_mode": change.before.as_ref().map(|s| s.mode), "after_mode": change.after.as_ref().map(|s| s.mode),
        "bytes": change.before.as_ref().map_or(0, |s| s.content.len())
    })).collect::<Vec<_>>();
    let changes = planned
        .into_iter()
        .filter(|c| c.before != c.after)
        .collect::<Vec<_>>();
    let changed = changes.len();
    let basis = super::basis(&changes)?;
    let transaction = if changed > 0 && (write || save_plan) {
        Some(persist(&root, changes, write)?)
    } else {
        None
    };
    Ok(json!({
        "schema": 1, "workspace_root": root, "operation": if executable.is_some() {"executable"} else {"delete"},
        "set": executable, "mode_scope": executable.map(|_| "owner-execute"), "validation": "file-snapshots",
        "basis": basis, "transaction": transaction, "applied": write && transaction.is_some(),
        "saved": save_plan && transaction.is_some(), "requested": paths.len(), "changed": changed, "entries": entries
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{act, Action, FAULT};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn setup(root: &Path, executable: bool, action: Action) {
        for (name, content) in [("first.txt", "before λ\n"), ("second.txt", "")] {
            fs::write(root.join(name), content).unwrap();
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o1640)).unwrap();
        }
        let paths = vec!["first.txt".into(), "second.txt".into()];
        let command = if executable {
            Command::Executable {
                paths,
                set: Executable::On,
                write: false,
            }
        } else {
            Command::Delete {
                paths,
                write: false,
            }
        };
        assert_eq!(execute(root, &command, true).unwrap()["transaction"], 1);
        if action != Action::Apply {
            act(root, Action::Apply, 1, true).unwrap();
        }
        if action == Action::Redo {
            act(root, Action::Undo, 1, true).unwrap();
        }
    }

    fn current(root: &Path) -> Vec<Option<crate::history::Snapshot>> {
        ["first.txt", "second.txt"]
            .iter()
            .map(|name| snapshot(&root.join(name)).unwrap())
            .collect()
    }

    #[test]
    fn handled_file_operation_failures_restore_every_write_boundary() {
        for executable in [false, true] {
            for action in [Action::Apply, Action::Undo, Action::Redo] {
                for index in 0..2 {
                    let dir = tempfile::tempdir().unwrap();
                    setup(dir.path(), executable, action);
                    let before = current(dir.path());
                    FAULT.with(|fault| fault.set(Some((index, false))));
                    assert!(act(dir.path(), action, 1, true).is_err());
                    assert_eq!(current(dir.path()), before);
                    assert!(History::read(dir.path()).unwrap().pending.is_none());
                }
            }
        }
    }

    #[test]
    fn interrupted_file_operations_recover_every_write_boundary() {
        if let Ok(root) = std::env::var("FR_FILE_CRASH_ROOT") {
            let index = std::env::var("FR_FILE_CRASH_INDEX")
                .unwrap()
                .parse()
                .unwrap();
            let action = match std::env::var("FR_FILE_CRASH_ACTION").unwrap().as_str() {
                "undo" => Action::Undo,
                "redo" => Action::Redo,
                _ => Action::Apply,
            };
            FAULT.with(|fault| fault.set(Some((index, true))));
            act(Path::new(&root), action, 1, true).unwrap();
            panic!("crash point did not run");
        }
        for executable in [false, true] {
            for action in [Action::Apply, Action::Undo, Action::Redo] {
                for index in 0..2 {
                    let dir = tempfile::tempdir().unwrap();
                    setup(dir.path(), executable, action);
                    let before = current(dir.path());
                    let output = std::process::Command::new(std::env::current_exe().unwrap())
                        .args(["--exact", "history::files::tests::interrupted_file_operations_recover_every_write_boundary", "--nocapture"])
                        .env("FR_FILE_CRASH_ROOT", dir.path()).env("FR_FILE_CRASH_INDEX", index.to_string())
                        .env("FR_FILE_CRASH_ACTION", format!("{action:?}").to_lowercase()).output().unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(77),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(History::read(dir.path()).unwrap().pending.is_some());
                    assert!(act(dir.path(), action, 1, true).is_err());
                    act(dir.path(), Action::Recover, 1, false).unwrap();
                    act(dir.path(), Action::Recover, 1, true).unwrap();
                    assert_eq!(current(dir.path()), before);
                    assert!(History::read(dir.path()).unwrap().pending.is_none());
                }
            }
        }
    }

    #[test]
    fn recording_rechecks_prepared_snapshots_before_saving() {
        for drift in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            fs::write(root.join("file.txt"), "before\n").unwrap();
            fs::set_permissions(root.join("file.txt"), fs::Permissions::from_mode(0o640)).unwrap();
            let changes = plan(&root, &["file.txt".into()], None).unwrap();
            match drift {
                0 => fs::write(root.join("file.txt"), "after\n").unwrap(),
                1 => fs::set_permissions(root.join("file.txt"), fs::Permissions::from_mode(0o600))
                    .unwrap(),
                _ => fs::remove_file(root.join("file.txt")).unwrap(),
            }
            let before = snapshot(&root.join("file.txt")).unwrap();
            assert!(persist(&root, changes, true).is_err());
            assert_eq!(snapshot(&root.join("file.txt")).unwrap(), before);
            assert!(History::read(&root).unwrap().records.is_empty());
        }
    }
}
