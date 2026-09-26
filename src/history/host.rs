use super::{
    check, directory, snapshot, sync_ancestors, target, validate_symlink_target, Change, History,
    SnapshotKind,
};
use anyhow::{ensure, Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    JournalCreate,
    JournalWrite,
    JournalSync,
    JournalPublish,
    JournalDirectorySync,
    CreateDirectory,
    SyncAncestor,
    StageCreate,
    StageWrite,
    StageMode,
    StageSync,
    StageUnlink,
    StageSymlink,
    StageDirectorySync,
    CheckSource,
    CheckStage,
    Publish,
    Remove,
    PublishDirectorySync,
    RecoveryCheck,
}

pub(super) fn step<T>(
    operation: Operation,
    path: &Path,
    action: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let execute = || -> Result<T> {
        #[cfg(test)]
        injection::visit(operation, path, false)?;
        let result = action()?;
        #[cfg(test)]
        injection::visit(operation, path, true)?;
        Ok(result)
    };
    execute().with_context(|| format!("{operation:?} {}", path.display()))
}

pub(super) fn sync_directory(operation: Operation, path: &Path) -> Result<()> {
    step(operation, path, || Ok(File::open(path)?.sync_all()?))
}

pub(super) fn save(history: &History) -> Result<()> {
    use Operation::*;
    let dir = directory(&history.root)?;
    let mut file = step(JournalCreate, &dir, || {
        Ok(tempfile::Builder::new()
            .prefix(".state-")
            .tempfile_in(&dir)?)
    })?;
    let temporary_path = file.path().to_path_buf();
    step(JournalWrite, &temporary_path, || {
        Ok(serde_json::to_writer(&mut file, history)?)
    })?;
    step(JournalSync, file.path(), || {
        Ok(file.as_file().sync_all()?)
    })?;
    let path = dir.join("state.json");
    step(JournalPublish, &path, || {
        file.persist(&path)?;
        Ok(())
    })?;
    sync_directory(JournalDirectorySync, &dir)
}

pub(super) fn install(root: &Path, changes: &[Change]) -> Result<()> {
    use Operation::*;
    let mut staged = Vec::new();
    for change in changes {
        let path = target(root, &change.path)?;
        let dir = path.parent().context("target has no parent")?;
        step(CreateDirectory, dir, || Ok(fs::create_dir_all(dir)?))?;
        sync_ancestors(root, dir)?;
        let replacement = if let Some(after) = &change.after {
            let mut temp = step(StageCreate, dir, || {
                Ok(tempfile::Builder::new()
                    .prefix(".fr-history-stage-")
                    .tempfile_in(dir)?)
            })?;
            if after.kind == SnapshotKind::Regular {
                let temp_path = temp.path().to_path_buf();
                step(StageWrite, &temp_path, || {
                    Ok(temp.write_all(after.content.as_bytes())?)
                })?;
                step(StageMode, &temp_path, || {
                    Ok(temp
                        .as_file()
                        .set_permissions(fs::Permissions::from_mode(after.mode))?)
                })?;
                step(StageSync, &temp_path, || Ok(temp.as_file().sync_all()?))?;
                Some(temp.into_temp_path())
            } else {
                validate_symlink_target(&after.content)?;
                let temp = temp.into_temp_path();
                step(StageUnlink, &temp, || Ok(fs::remove_file(&temp)?))?;
                step(StageSymlink, &temp, || Ok(symlink(&after.content, &temp)?))?;
                sync_directory(StageDirectorySync, dir)?;
                Some(temp)
            }
        } else {
            None
        };
        staged.push((path, replacement));
    }
    step(CheckSource, root, || check(root, changes, false))?;
    for (change, (path, replacement)) in changes.iter().zip(staged) {
        let current = step(CheckSource, &path, || snapshot(&path))?;
        let material = match &replacement {
            Some(temp) => step(CheckStage, temp, || snapshot(temp))?,
            None => None,
        };
        ensure!(
            crate::transaction_kernel::history_publication_allowed(
                current == change.before,
                material == change.after
            ),
            "{} conflicts with transaction snapshots or staged material; preserving current files",
            change.path.display()
        );
        match replacement {
            Some(temp) => step(Publish, &path, || Ok(fs::rename(&temp, &path)?))?,
            None => step(Remove, &path, || Ok(fs::remove_file(&path)?))?,
        }
        sync_directory(PublishDirectorySync, path.parent().unwrap())?;
        #[cfg(test)]
        super::fault()?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod injection {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Debug)]
    pub struct Event {
        pub operation: Operation,
        pub path: std::path::PathBuf,
        pub after: bool,
    }

    type Hook = Box<dyn FnMut(Event) -> Result<()>>;
    thread_local! { static HOOK: RefCell<Option<Hook>> = RefCell::new(None); }

    pub fn visit(operation: Operation, path: &Path, after: bool) -> Result<()> {
        HOOK.with_borrow_mut(|hook| match hook {
            Some(hook) => hook(Event {
                operation,
                path: path.to_path_buf(),
                after,
            }),
            None => Ok(()),
        })
    }

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            HOOK.with_borrow_mut(|hook| *hook = None);
        }
    }

    pub fn hook(hook: impl FnMut(Event) -> Result<()> + 'static) -> Guard {
        HOOK.with_borrow_mut(|slot| {
            assert!(slot.is_none());
            *slot = Some(Box::new(hook));
        });
        Guard
    }
}

#[cfg(test)]
mod tests;
