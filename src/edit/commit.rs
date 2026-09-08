use super::{verify_basis_unchanged, CommitLocks, FileChange};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempPath};

#[derive(Debug, Serialize)]
pub struct RecoveryFailure {
    pub file: PathBuf,
    pub recovery_copy: Option<PathBuf>,
    pub reason: String,
}

#[derive(Debug)]
pub struct CommitFailure {
    cause: anyhow::Error,
    pub restored_files: Vec<PathBuf>,
    pub recovery_failures: Vec<RecoveryFailure>,
}

impl std::fmt::Display for CommitFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; restored {} earlier file(s)",
            self.cause,
            self.restored_files.len()
        )?;
        for failure in &self.recovery_failures {
            write!(
                f,
                "; recovery required for {}: {}",
                failure.file.display(),
                failure.reason
            )?;
            if let Some(copy) = &failure.recovery_copy {
                write!(f, "; original saved at {}", copy.display())?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for CommitFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

struct Staged<'a> {
    outcome: FileChange<'a>,
    replacement: TempPath,
    recovery: Option<TempPath>,
    original_metadata: Option<std::fs::Metadata>,
    replacement_metadata: std::fs::Metadata,
}

fn temporary(dir: &Path, prefix: &str, content: &str, mode: Option<u32>) -> Result<NamedTempFile> {
    let mut file = tempfile::Builder::new().prefix(prefix).tempfile_in(dir)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    if let Some(mode) = mode {
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(mode))?;
    }
    Ok(file)
}

fn stage<'a>(outcome: &FileChange<'a>) -> Result<Staged<'a>> {
    let path = outcome.path;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let original_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata),
        Ok(_) => bail!("{} is not a regular file", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("reading metadata for {}", path.display()))
        }
    };
    let mode = original_metadata
        .as_ref()
        .map(|metadata| metadata.permissions().mode());
    let replacement = temporary(dir, ".fr-stage-", outcome.updated, mode)?;
    let replacement_metadata = replacement.as_file().metadata()?;
    let replacement = replacement.into_temp_path();
    let recovery = mode
        .map(|mode| {
            temporary(dir, ".fr-recovery-", outcome.original, Some(mode))
                .map(NamedTempFile::into_temp_path)
        })
        .transpose()?;
    Ok(Staged {
        outcome: *outcome,
        replacement,
        recovery,
        original_metadata,
        replacement_metadata,
    })
}

fn same_file(current: &std::fs::Metadata, expected: &std::fs::Metadata) -> bool {
    current.is_file()
        && current.dev() == expected.dev()
        && current.ino() == expected.ino()
        && current.permissions().mode() == expected.permissions().mode()
}

fn verify_target(staged: &Staged<'_>) -> Result<()> {
    match (
        std::fs::symlink_metadata(staged.outcome.path),
        &staged.original_metadata,
    ) {
        (Ok(current), Some(original)) if same_file(&current, original) => (),
        (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => (),
        _ => bail!("{} changed during staging", staged.outcome.path.display()),
    }
    verify_basis_unchanged(std::slice::from_ref(&staged.outcome))
}

fn verify_our_write(staged: &Staged<'_>) -> Result<()> {
    let current = std::fs::symlink_metadata(staged.outcome.path)?;
    if !same_file(&current, &staged.replacement_metadata)
        || crate::vfs::read_to_string(staged.outcome.path)? != staged.outcome.updated
    {
        bail!("the target changed after this transaction wrote it; preserving the current file");
    }
    Ok(())
}

pub(super) fn commit(outcomes: &[FileChange<'_>]) -> Result<usize> {
    commit_with(outcomes, |from, to| std::fs::rename(from, to))
}

fn commit_with(
    outcomes: &[FileChange<'_>],
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<usize> {
    let _locks = CommitLocks::acquire(outcomes)?;
    verify_basis_unchanged(outcomes)?;
    let mut targets = std::collections::BTreeSet::new();
    for outcome in outcomes.iter().filter(|o| o.changed()) {
        let dir = outcome.path.parent().unwrap_or_else(|| Path::new("."));
        let target = dir.canonicalize()?.join(
            outcome
                .path
                .file_name()
                .context("a target needs a file name")?,
        );
        if !targets.insert(target) {
            bail!("duplicate commit target {}", outcome.path.display());
        }
    }
    let mut staged = outcomes
        .iter()
        .filter(|o| o.changed())
        .map(|outcome| {
            stage(outcome).with_context(|| format!("staging {}", outcome.path.display()))
        })
        .collect::<Result<Vec<_>>>()?;

    for applied in 0..staged.len() {
        let next = &staged[applied];
        let result = verify_target(next).and_then(|()| {
            rename(&next.replacement, next.outcome.path)
                .with_context(|| format!("committing {}", next.outcome.path.display()))
        });
        if let Err(cause) = result {
            let mut failure = CommitFailure {
                cause,
                restored_files: Vec::new(),
                recovery_failures: Vec::new(),
            };
            for previous in staged[..applied].iter_mut().rev() {
                let restored = verify_our_write(previous).and_then(|()| match &previous.recovery {
                    Some(copy) => rename(copy, previous.outcome.path).map_err(Into::into),
                    None => std::fs::remove_file(previous.outcome.path).map_err(Into::into),
                });
                match restored {
                    Ok(()) => failure
                        .restored_files
                        .push(previous.outcome.path.to_path_buf()),
                    Err(error) => {
                        let recovery_copy = previous.recovery.as_mut().map(|copy| {
                            copy.disable_cleanup(true);
                            copy.to_path_buf()
                        });
                        failure.recovery_failures.push(RecoveryFailure {
                            file: previous.outcome.path.to_path_buf(),
                            recovery_copy,
                            reason: format!("{error:#}"),
                        });
                    }
                }
            }
            return Err(failure.into());
        }
    }
    Ok(staged.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::FileOutcome;
    use crate::lang::Language;
    use std::fs;
    use std::io;

    fn commit_with(
        outcomes: &[FileOutcome],
        rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> Result<usize> {
        super::commit_with(
            &outcomes.iter().map(FileChange::from).collect::<Vec<_>>(),
            rename,
        )
    }

    fn commit(outcomes: &[FileOutcome]) -> Result<usize> {
        crate::edit::commit(outcomes)
    }

    fn fixtures(dir: &Path) -> Vec<FileOutcome> {
        [Some("before λ\n"), None, Some(""), Some("last\n")]
            .into_iter()
            .enumerate()
            .map(|(n, before)| {
                let path = dir.join(format!("{n}.txt"));
                if let Some(text) = before {
                    fs::write(&path, text).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
                }
                FileOutcome {
                    path,
                    original: before.unwrap_or("").to_string(),
                    updated: format!("after {n} 名\n"),
                    language: Language::Rust,
                }
            })
            .collect()
    }

    fn failure() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "injected rename failure")
    }

    fn assert_originals(outcomes: &[FileOutcome]) {
        for (n, outcome) in outcomes.iter().enumerate() {
            if n == 1 {
                assert!(!outcome.path.exists());
            } else {
                assert_eq!(fs::read_to_string(&outcome.path).unwrap(), outcome.original);
                assert_eq!(fs::metadata(&outcome.path).unwrap().mode() & 0o777, 0o751);
            }
        }
    }

    #[test]
    fn every_commit_failure_restores_bytes_existence_and_modes() {
        for fail_at in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let outcomes = fixtures(dir.path());
            let mut calls = 0;
            let error = commit_with(&outcomes, |from, to| {
                let fail = calls == fail_at;
                calls += 1;
                if fail {
                    Err(failure())
                } else {
                    fs::rename(from, to)
                }
            })
            .unwrap_err();
            let report = error.downcast_ref::<CommitFailure>().unwrap();
            assert_eq!(report.restored_files.len(), fail_at);
            assert!(report.recovery_failures.is_empty());
            assert_originals(&outcomes);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
        }
    }

    #[test]
    fn recovery_continues_after_a_restore_fails_and_retains_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let outcomes = fixtures(dir.path());
        let mut calls = 0;
        let error = commit_with(&outcomes, |from, to| {
            let fail = calls == 3 || calls == 4;
            calls += 1;
            if fail {
                Err(failure())
            } else {
                fs::rename(from, to)
            }
        })
        .unwrap_err();
        let report = error.downcast_ref::<CommitFailure>().unwrap();
        assert_eq!(
            report.restored_files,
            vec![outcomes[1].path.clone(), outcomes[0].path.clone()]
        );
        assert_eq!(report.recovery_failures.len(), 1);
        let failed = &report.recovery_failures[0];
        assert_eq!(failed.file, outcomes[2].path);
        let copy = failed.recovery_copy.as_ref().unwrap();
        assert_eq!(fs::read_to_string(copy).unwrap(), "");
        assert_eq!(fs::metadata(copy).unwrap().mode() & 0o777, 0o751);
        assert!(error.to_string().contains(copy.to_str().unwrap()));
        assert_eq!(
            fs::read_to_string(&outcomes[2].path).unwrap(),
            outcomes[2].updated
        );
        assert_eq!(
            fs::read_to_string(&outcomes[0].path).unwrap(),
            outcomes[0].original
        );
        assert!(!outcomes[1].path.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    }

    #[test]
    fn recovery_preserves_a_concurrent_edit_or_mode_change() {
        for change_mode in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let outcomes = fixtures(dir.path());
            let mut calls = 0;
            let error = commit_with(&outcomes, |from, to| {
                calls += 1;
                if calls == 2 {
                    if change_mode {
                        fs::set_permissions(&outcomes[0].path, fs::Permissions::from_mode(0o600))?;
                    } else {
                        fs::write(&outcomes[0].path, "another writer\n")?;
                    }
                    Err(failure())
                } else {
                    fs::rename(from, to)
                }
            })
            .unwrap_err();
            let report = error.downcast_ref::<CommitFailure>().unwrap();
            assert!(report.restored_files.is_empty());
            let recovery = &report.recovery_failures[0];
            assert_eq!(
                fs::read_to_string(recovery.recovery_copy.as_ref().unwrap()).unwrap(),
                outcomes[0].original
            );
            if change_mode {
                assert_eq!(
                    fs::metadata(&outcomes[0].path).unwrap().mode() & 0o777,
                    0o600
                );
            } else {
                assert_eq!(
                    fs::read_to_string(&outcomes[0].path).unwrap(),
                    "another writer\n"
                );
            }
        }
    }

    #[test]
    fn a_file_created_during_staging_is_not_overwritten_even_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let outcomes = fixtures(dir.path());
        let mut calls = 0;
        let error = commit_with(&outcomes, |from, to| {
            calls += 1;
            if calls == 1 {
                fs::write(&outcomes[1].path, "")?;
            }
            fs::rename(from, to)
        })
        .unwrap_err();
        let report = error.downcast_ref::<CommitFailure>().unwrap();
        assert_eq!(report.restored_files, vec![outcomes[0].path.clone()]);
        assert!(report.recovery_failures.is_empty());
        assert_eq!(fs::read_to_string(&outcomes[1].path).unwrap(), "");
        assert_eq!(
            fs::read_to_string(&outcomes[0].path).unwrap(),
            outcomes[0].original
        );
    }

    #[test]
    fn staging_failure_discards_temporary_files_and_preserves_targets() {
        let dir = tempfile::tempdir().unwrap();
        let outcomes = fixtures(dir.path());
        fs::remove_file(&outcomes[3].path).unwrap();
        let external = dir.path().join("external.txt");
        fs::write(&external, &outcomes[3].original).unwrap();
        std::os::unix::fs::symlink(&external, &outcomes[3].path).unwrap();
        let error = commit(&outcomes).unwrap_err();
        assert!(format!("{error:#}").contains("not a regular file"));
        assert_eq!(
            fs::read_to_string(&outcomes[0].path).unwrap(),
            outcomes[0].original
        );
        assert!(!outcomes[1].path.exists());
        assert!(fs::symlink_metadata(&outcomes[3].path)
            .unwrap()
            .is_symlink());
        assert_eq!(fs::read_to_string(external).unwrap(), outcomes[3].original);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    }

    #[test]
    fn duplicate_targets_through_directory_aliases_refuse_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let mut outcomes = fixtures(dir.path());
        let mut duplicate = outcomes[0].clone();
        duplicate.path = dir.path().join(".").join("0.txt");
        outcomes.push(duplicate);
        assert!(commit(&outcomes)
            .unwrap_err()
            .to_string()
            .contains("duplicate commit target"));
        assert_originals(&outcomes[..4]);
    }

    #[test]
    fn success_discards_recovery_material() {
        let dir = tempfile::tempdir().unwrap();
        let outcomes = fixtures(dir.path());
        assert_eq!(commit(&outcomes).unwrap(), 4);
        for outcome in &outcomes {
            assert_eq!(fs::read_to_string(&outcome.path).unwrap(), outcome.updated);
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    }

    #[test]
    fn large_commits_do_not_keep_every_temporary_file_open() {
        if std::env::var_os("FR_COMMIT_DESCRIPTOR_TEST").is_none() {
            let output = std::process::Command::new("sh")
                .args(["-c", "ulimit -n 128 && exec \"$@\"", "fr-commit-test"])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "edit::commit::tests::large_commits_do_not_keep_every_temporary_file_open",
                ])
                .env("FR_COMMIT_DESCRIPTOR_TEST", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let outcomes = (0..256)
            .map(|n| {
                let path = dir.path().join(format!("{n}.txt"));
                fs::write(&path, "old").unwrap();
                FileOutcome {
                    path,
                    original: "old".into(),
                    updated: "new".into(),
                    language: Language::Rust,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(commit(&outcomes).unwrap(), 256);
        for outcome in &outcomes {
            assert_eq!(fs::read_to_string(&outcome.path).unwrap(), "new");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 256);
    }
}
