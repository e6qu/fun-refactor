use super::{absent, basis, checkout, common, directory, ownership, worktree_config, Proposal};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    id: String,
    plan: Proposal,
}

pub(super) struct Prepared {
    path: PathBuf,
    id: String,
    identity: (u64, u64),
    mode: u32,
    bytes: Vec<u8>,
}

impl Prepared {
    pub(super) fn record_path(common: &Path, destination: &Path) -> Result<PathBuf> {
        let destination = destination
            .to_str()
            .context("prepared worktree destination must use UTF-8.")?;
        Ok(common.join(format!(
            "fr-worktree-creation-{:x}.json",
            Sha256::digest(destination.as_bytes())
        )))
    }

    pub(super) fn create(plan: &Proposal) -> Result<Self> {
        ensure!(
            directory(&plan.common)? == plan.common_identity,
            "repository changed before recording worktree preparation."
        );
        let path = Self::record_path(&plan.common, &plan.destination)?;
        ensure!(
            absent(&path)?,
            "a prepared creation already exists for this worktree destination."
        );
        let id = basis(plan)?;
        let bytes = serde_json::to_vec(&Record {
            schema: 1,
            id: id.clone(),
            plan: plan.clone(),
        })?;
        ensure!(
            bytes.len() <= 64 * 1024 * 1024,
            "worktree preparation exceeds 64 MiB."
        );
        let mut temporary = tempfile::NamedTempFile::new_in(&plan.common)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&path)?;
        File::open(&plan.common)?.sync_all()?;
        Self::read(&path, Some(&id))?.context("prepared creation disappeared after publication.")
    }

    fn read(path: &Path, expected_id: Option<&str>) -> Result<Option<Self>> {
        let before = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            before.file_type().is_file() && before.len() <= 64 * 1024 * 1024,
            "worktree preparation must be a bounded regular file."
        );
        let bytes = ownership::bytes(path, 64 * 1024 * 1024)?;
        let after = fs::symlink_metadata(path)?;
        ensure!(
            before.dev() == after.dev()
                && before.ino() == after.ino()
                && before.mode() == after.mode(),
            "worktree preparation changed during inspection."
        );
        let record: Record =
            serde_json::from_slice(&bytes).context("invalid prepared worktree creation.")?;
        ensure!(
            record.schema == 1
                && record.id == basis(&record.plan)?
                && expected_id.is_none_or(|expected| expected == record.id),
            "worktree preparation identity or schema differs."
        );
        Ok(Some(Self {
            path: path.to_owned(),
            id: record.id,
            identity: (after.dev(), after.ino()),
            mode: after.mode(),
            bytes,
        }))
    }

    pub(super) fn recover(root: &Path, destination: &Path) -> Result<(Self, Proposal)> {
        let shared = common(root)?;
        let path = Self::record_path(&shared, destination)?;
        let prepared = Self::read(&path, None)?
            .context("no ownership receipt or prepared creation exists for this worktree.")?;
        let record: Record = serde_json::from_slice(&prepared.bytes)?;
        let mut plan = record.plan;
        ensure!(
            plan.common == shared
                && plan.destination == destination
                && directory(&shared)? == plan.common_identity
                && directory(destination.parent().context("worktree needs a parent.")?)?
                    == plan.parent_identity
                && worktree_config(root)? == plan.worktree_config,
            "prepared creation belongs to another repository state."
        );
        ensure!(
            checkout::inventory(root, &plan.commit)? == plan.files,
            "prepared creation inventory changed."
        );
        let tree = super::checked(
            root,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{tree}}", plan.commit),
            ],
        )?;
        ensure!(
            super::line(&tree)? == plan.tree,
            "prepared creation tree changed."
        );
        plan.root = root.to_owned();
        let registrations = super::checked(
            root,
            &["worktree", "list", "--porcelain", "-z", "--expire=now"],
        )?;
        plan.registrations = ownership::digest(&registrations);
        checkout::check_registration(&plan)?;
        prepared.check()?;
        Ok((prepared, plan))
    }

    pub(super) fn load(
        common: &Path,
        destination: &Path,
        expected_id: &str,
    ) -> Result<Option<Self>> {
        Self::read(&Self::record_path(common, destination)?, Some(expected_id))
    }

    pub(super) fn id(&self) -> String {
        self.id.clone()
    }

    pub(super) fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn fingerprint(&self) -> ((u64, u64), u32, String) {
        (self.identity, self.mode, ownership::digest(&self.bytes))
    }

    pub(super) fn check(&self) -> Result<()> {
        let current = Self::read(&self.path, Some(&self.id))?
            .context("prepared worktree creation disappeared.")?;
        ensure!(
            current.identity == self.identity
                && current.mode == self.mode
                && current.bytes == self.bytes,
            "prepared worktree creation changed."
        );
        Ok(())
    }

    pub(super) fn remove(self, lease: ownership::Lease) -> Result<()> {
        lease.check()?;
        self.check()?;
        fs::remove_file(&self.path)?;
        drop(lease);
        File::open(self.path.parent().unwrap())?.sync_all()?;
        Ok(())
    }
}
