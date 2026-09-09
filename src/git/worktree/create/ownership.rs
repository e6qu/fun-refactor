use super::{checkout, directory, Proposal};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(super) struct Lease {
    path: PathBuf,
    file: File,
}

impl Lease {
    pub(super) fn acquire(path: PathBuf) -> Result<Self> {
        directory(path.parent().context("ownership lock needs a directory.")?)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .context("cannot acquire worktree lock; leave existing locks to their owner.")?;
        Ok(Self { path, file })
    }

    pub(super) fn check(&self) -> Result<()> {
        let current = fs::symlink_metadata(&self.path)?;
        let held = self.file.metadata()?;
        ensure!(
            current.file_type().is_file()
                && current.dev() == held.dev()
                && current.ino() == held.ino(),
            "worktree lock ownership changed."
        );
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if self.check().is_ok() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= limit,
        "unsupported ownership metadata file."
    );
    let file = File::open(path)?;
    let opened = file.metadata()?;
    ensure!(
        metadata.dev() == opened.dev() && metadata.ino() == opened.ino(),
        "ownership metadata changed while opening."
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "ownership metadata exceeds its size limit."
    );
    Ok(bytes)
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) schema: u32,
    pub(super) complete: bool,
    pub(super) common: PathBuf,
    pub(super) common_identity: (u64, u64),
    #[serde(default)]
    pub(super) worktree_config: bool,
    pub(super) destination: PathBuf,
    pub(super) parent_identity: (u64, u64),
    pub(super) destination_identity: (u64, u64),
    pub(super) metadata: PathBuf,
    pub(super) metadata_identity: (u64, u64),
    pub(super) gitfile_identity: (u64, u64),
    pub(super) gitfile_digest: String,
    pub(super) branch: String,
    #[serde(default)]
    pub(super) existing_branch: bool,
    pub(super) commit: String,
    pub(super) tree: String,
}

impl Receipt {
    pub(super) fn record(
        plan: &Proposal,
        destination_identity: (u64, u64),
    ) -> Result<(Self, Lease)> {
        let index = checkout::index_path(plan)?;
        let metadata = index.parent().unwrap().to_owned();
        let lease = Lease::acquire(metadata.join("fr-creation.lock"))?;
        let gitfile = plan.destination.join(".git");
        let gitfile_bytes = bytes(&gitfile, 64 * 1024)?;
        let stat = fs::symlink_metadata(&gitfile)?;
        let receipt = Self {
            schema: 1,
            complete: false,
            common: plan.common.clone(),
            common_identity: plan.common_identity,
            worktree_config: plan.worktree_config,
            destination: plan.destination.clone(),
            parent_identity: plan.parent_identity,
            destination_identity,
            metadata_identity: directory(&metadata)?,
            metadata,
            gitfile_identity: (stat.dev(), stat.ino()),
            gitfile_digest: digest(&gitfile_bytes),
            branch: plan.branch.clone(),
            existing_branch: plan.existing_branch,
            commit: plan.commit.clone(),
            tree: plan.tree.clone(),
        };
        lease.check()?;
        receipt.check()?;
        receipt.save(true)?;
        Ok((receipt, lease))
    }

    pub(super) fn path(&self) -> PathBuf {
        self.metadata.join("fr-creation.json")
    }

    pub(super) fn read(path: &Path) -> Result<(Self, Vec<u8>)> {
        let raw = bytes(path, 64 * 1024)
            .context("no readable ownership receipt; automatic adoption is unsupported.")?;
        let receipt: Self =
            serde_json::from_slice(&raw).context("invalid worktree ownership receipt.")?;
        ensure!(
            receipt.schema == 1 && receipt.path() == path,
            "ownership receipt location or schema differs."
        );
        receipt.check()?;
        Ok((receipt, raw))
    }

    pub(super) fn check(&self) -> Result<()> {
        ensure!(
            self.destination.is_absolute()
                && self.common.is_absolute()
                && self.metadata.is_absolute()
                && self.metadata.parent() == Some(self.common.join("worktrees").as_path())
                && !self.destination.starts_with(&self.common)
                && !self.common.starts_with(&self.destination),
            "invalid ownership receipt paths."
        );
        ensure!(
            directory(&self.common)? == self.common_identity
                && directory(&self.metadata)? == self.metadata_identity
                && directory(
                    self.destination
                        .parent()
                        .context("owned destination needs a parent.")?
                )? == self.parent_identity
                && directory(&self.destination)? == self.destination_identity,
            "worktree ownership directory identity changed."
        );
        let path = self.destination.join(".git");
        let raw = bytes(&path, 64 * 1024)?;
        let stat = fs::symlink_metadata(path)?;
        ensure!(
            (stat.dev(), stat.ino()) == self.gitfile_identity
                && digest(&raw) == self.gitfile_digest,
            "worktree Git link ownership changed."
        );
        ensure!(
            crate::git::process::oid(&self.commit) && crate::git::process::oid(&self.tree),
            "invalid ownership receipt object identity."
        );
        super::checked(
            &self.destination,
            &["check-ref-format", &format!("refs/heads/{}", self.branch)],
        )?;
        Ok(())
    }

    fn save(&self, fresh: bool) -> Result<()> {
        let mut temp = tempfile::NamedTempFile::new_in(&self.metadata)?;
        temp.write_all(&serde_json::to_vec(self)?)?;
        temp.as_file().sync_all()?;
        if fresh {
            temp.persist_noclobber(self.path())?;
        } else {
            temp.persist(self.path())?;
        }
        File::open(&self.metadata)?.sync_all()?;
        Ok(())
    }

    pub(super) fn finish(mut self, lease: &Lease, expected: &[u8]) -> Result<PathBuf> {
        lease.check()?;
        self.check()?;
        ensure!(
            bytes(&self.path(), 64 * 1024)? == expected,
            "ownership receipt changed before completion."
        );
        self.complete = true;
        self.save(false)?;
        Ok(self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_drop_preserves_a_replacement_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        let lease = Lease::acquire(path.clone()).unwrap();
        fs::rename(&path, temp.path().join("old-lock")).unwrap();
        fs::write(&path, b"foreign").unwrap();
        assert!(lease.check().is_err());
        drop(lease);
        assert_eq!(fs::read(path).unwrap(), b"foreign");
    }

    #[test]
    fn lease_refuses_existing_files_and_releases_its_own_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("lock");
        let lease = Lease::acquire(path.clone()).unwrap();
        assert!(Lease::acquire(path.clone()).is_err());
        drop(lease);
        let second = Lease::acquire(path).unwrap();
        second.check().unwrap();
    }
}
