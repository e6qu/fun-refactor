use super::super::super::{checked, directory, ownership};
use super::super::{resume, FileState};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Fingerprint {
    identity: (u64, u64),
    mode: u32,
    digest: String,
    pub(super) bytes: usize,
}

impl Fingerprint {
    pub(super) fn of(file: &FileState) -> Self {
        Self {
            identity: file.identity,
            mode: file.mode,
            digest: file.digest.clone(),
            bytes: file.bytes.len(),
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Summary {
    schema: u32,
    operation: String,
    pub(super) common: PathBuf,
    common_identity: (u64, u64),
    archive: PathBuf,
    archive_identity: (u64, u64),
    pub(super) original_record: Fingerprint,
    completion: Option<Fingerprint>,
    pub(super) destination: PathBuf,
    pub(super) metadata: PathBuf,
    pub(super) branch: String,
    pub(super) commit: String,
    pub(super) tree: String,
    pub(super) checkout_files: usize,
    pub(super) metadata_files: usize,
    pub(super) metadata_bytes: usize,
}

impl Summary {
    pub(super) fn from_record(record: &resume::record::Loaded, marker: Option<&FileState>) -> Self {
        Self {
            schema: 1,
            operation: "worktree-removal-compaction".to_owned(),
            common: record.plan.common.clone(),
            common_identity: record.plan.common_identity,
            archive: record.path.parent().unwrap().to_owned(),
            archive_identity: record.archive_identity,
            original_record: Fingerprint::of(&record.raw),
            completion: marker.map(Fingerprint::of),
            destination: record.plan.destination.clone(),
            metadata: record.trees[1].root.clone(),
            branch: format!("refs/heads/{}", record.plan.branch),
            commit: record.plan.commit.clone(),
            tree: record.plan.tree.clone(),
            checkout_files: record.plan.files.len(),
            metadata_files: record.trees[1].files.len(),
            metadata_bytes: record.trees[1]
                .files
                .values()
                .map(|file| file.bytes.len())
                .sum(),
        }
    }

    pub(super) fn check_location(&self, common: &Path, path: &Path) -> Result<()> {
        ensure!(
            self.common == common
                && self.archive == path.parent().unwrap()
                && self.archive.parent() == Some(common)
                && directory(common)? == self.common_identity
                && directory(&self.archive)? == self.archive_identity,
            "compaction archive ownership changed."
        );
        Ok(())
    }

    pub(super) fn validate(
        &self,
        root: &Path,
        common: &Path,
        path: &Path,
        marker: Option<&FileState>,
    ) -> Result<()> {
        let regular = |file: &Fingerprint| {
            file.mode & 0o170000 == 0o100000
                && file.digest.len() == 64
                && file.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        let absolute = |path: &Path| {
            path.is_absolute()
                && path
                    .components()
                    .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
        };
        ensure!(
            self.schema == 1 && self.operation == "worktree-removal-compaction",
            "compaction summary ownership or schema differs."
        );
        self.check_location(common, path)?;
        ensure!(
            absolute(&self.destination)
                && absolute(&self.metadata)
                && !self.destination.starts_with(common)
                && !common.starts_with(&self.destination)
                && !root.starts_with(&self.destination)
                && self.metadata.parent() == Some(common.join("worktrees").as_path()),
            "unsafe compaction summary paths."
        );
        ensure!(
            regular(&self.original_record)
                && self.original_record.bytes <= 128 * 1024 * 1024
                && self.checkout_files <= 20000
                && self.metadata_files <= 9
                && self.metadata_bytes <= 16 * 1024 * 1024
                && crate::git::process::oid(&self.commit)
                && crate::git::process::oid(&self.tree)
                && self.branch.starts_with("refs/heads/"),
            "invalid compaction summary inventory."
        );
        checked(root, &["check-ref-format", &self.branch])?;
        ensure!(
            self.completion.as_ref().is_some_and(|file| regular(file)
                && file.digest == ownership::digest(resume::COMPLETE)
                && file.bytes == resume::COMPLETE.len())
                && self.completion == marker.map(Fingerprint::of),
            "compaction completion marker changed."
        );
        Ok(())
    }
}
