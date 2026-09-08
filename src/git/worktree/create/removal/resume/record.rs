use super::super::super::{checked, checkout, common, directory, line, ownership, Proposal};
use super::super::{FileState, Snapshot};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    state: String,
    proposal: Proposal,
    receipt: ownership::Receipt,
    snapshot: Snapshot,
    metadata_bytes: BTreeMap<String, Vec<u8>>,
    gitfile_bytes: Vec<u8>,
}

pub(in crate::git::worktree::create::removal) struct Tree {
    pub(in crate::git::worktree::create::removal) root: PathBuf,
    pub(in crate::git::worktree::create::removal) files: BTreeMap<String, FileState>,
    pub(in crate::git::worktree::create::removal) directories: BTreeMap<String, (u64, u64)>,
}

pub(in crate::git::worktree::create::removal) struct Loaded {
    pub(in crate::git::worktree::create::removal) root: PathBuf,
    pub(in crate::git::worktree::create::removal) path: PathBuf,
    pub(in crate::git::worktree::create::removal) archive_identity: (u64, u64),
    pub(in crate::git::worktree::create::removal) raw: FileState,
    pub(in crate::git::worktree::create::removal) plan: Proposal,
    pub(in crate::git::worktree::create::removal) trees: [Tree; 2],
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
}

pub(in crate::git::worktree::create::removal) fn location(
    root: &Path,
    path: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let requested = root.canonicalize()?;
    let root = crate::git::process::repository_root(&requested)?;
    ensure!(
        requested.starts_with(&root),
        "Git resolved a working tree outside the requested directory."
    );
    let shared = common(&root)?;
    let input = root.join(path);
    let parent = input
        .parent()
        .context("removal record needs a directory.")?;
    directory(parent)?;
    let parent = parent.canonicalize()?;
    ensure!(
        input.file_name().is_some_and(|name| name == "record.json")
            && parent.parent() == Some(shared.as_path())
            && parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("fr-worktree-removal-")),
        "removal record must be inside its shared repository archive."
    );
    let path = parent.join("record.json");
    Ok((root, shared, path))
}

impl Loaded {
    pub(in crate::git::worktree::create::removal) fn read(
        root: &Path,
        path: &Path,
    ) -> Result<Self> {
        let (root, shared, path) = location(root, path)?;
        let parent = path.parent().unwrap().to_owned();
        let raw = FileState::read_limited(&path, 128 * 1024 * 1024)?;
        let mut record: Record =
            serde_json::from_slice(&raw.bytes).context("invalid removal archive.")?;
        let plan = &mut record.proposal;
        let receipt = &record.receipt;
        ensure!(
            record.schema == 1
                && record.state == "prepared"
                && receipt.schema == 1
                && receipt.complete,
            "unsupported removal archive state."
        );
        ensure!(
            plan.common == shared
                && receipt.common == shared
                && plan.common_identity == receipt.common_identity
                && directory(&shared)? == plan.common_identity
                && plan.destination == receipt.destination
                && plan.parent_identity == receipt.parent_identity
                && plan.branch == receipt.branch
                && plan.existing_branch == receipt.existing_branch,
            "removal archive ownership does not match its repository."
        );
        ensure!(
            absolute(&plan.destination)
                && absolute(&receipt.metadata)
                && receipt.metadata.parent() == Some(shared.join("worktrees").as_path())
                && !plan.destination.starts_with(&shared)
                && !shared.starts_with(&plan.destination)
                && !root.starts_with(&plan.destination),
            "unsafe removal archive destination."
        );
        ensure!(
            directory(
                plan.destination
                    .parent()
                    .context("removal destination needs a parent.")?
            )? == plan.parent_identity,
            "removal destination parent identity changed."
        );
        checked(
            &root,
            &["check-ref-format", &format!("refs/heads/{}", plan.branch)],
        )?;
        ensure!(
            crate::git::process::oid(&plan.commit) && crate::git::process::oid(&plan.tree),
            "invalid removal commit or tree."
        );
        ensure!(
            line(&checked(
                &root,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("{}^{{tree}}", plan.commit)
                ]
            )?)? == plan.tree,
            "removal tree differs from its retained commit."
        );
        ensure!(
            checkout::inventory(&root, &plan.commit)? == plan.files,
            "removal inventory differs from its retained commit."
        );
        plan.root = root.clone();
        let blobs = checkout::blobs(plan)?;
        let snapshot = &mut record.snapshot;
        ensure!(
            snapshot.checkout.missing.is_empty()
                && snapshot.checkout.files.len() == plan.files.len(),
            "invalid archived checkout inventory."
        );
        let mut files = BTreeMap::new();
        let mut expected_directories = BTreeSet::from([String::new()]);
        for (entry, bytes) in plan.files.iter().zip(blobs) {
            let (dev, ino, mode, digest) = snapshot
                .checkout
                .files
                .get(&entry.path)
                .context("missing archived file identity.")?;
            ensure!(
                *digest == ownership::digest(&bytes)
                    && mode & 0o170000 == 0o100000
                    && (*mode & 0o100 != 0) == (entry.mode == "100755"),
                "invalid archived file bytes or mode."
            );
            for parent in Path::new(&entry.path).ancestors().skip(1) {
                expected_directories.insert(parent.to_str().unwrap().to_owned());
            }
            files.insert(
                entry.path.clone(),
                FileState {
                    identity: (*dev, *ino),
                    mode: *mode,
                    digest: digest.clone(),
                    bytes,
                },
            );
        }
        ensure!(
            snapshot
                .checkout
                .directories
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                == expected_directories
                && snapshot.checkout.directories.get("") == Some(&receipt.destination_identity),
            "invalid archived checkout directories."
        );
        ensure!(
            snapshot.directories.get("") == Some(&receipt.metadata_identity)
                && snapshot
                    .directories
                    .keys()
                    .all(|name| ["", "logs", "refs"].contains(&name.as_str())),
            "invalid archived metadata directories."
        );
        ensure!(
            snapshot.metadata.len() == record.metadata_bytes.len(),
            "archived metadata inventory differs."
        );
        let mut total = 0usize;
        for (name, state) in &mut snapshot.metadata {
            ensure!(
                [
                    "HEAD",
                    "index",
                    "commondir",
                    "gitdir",
                    "locked",
                    "fr-creation.json",
                    "logs/HEAD",
                    "COMMIT_EDITMSG",
                    "ORIG_HEAD"
                ]
                .contains(&name.as_str()),
                "unsafe archived metadata path."
            );
            state.bytes = record
                .metadata_bytes
                .remove(name)
                .context("missing archived metadata bytes.")?;
            total += state.bytes.len();
            ensure!(
                total <= 16 * 1024 * 1024
                    && ownership::digest(&state.bytes) == state.digest
                    && state.mode & 0o170000 == 0o100000,
                "invalid archived metadata bytes or mode."
            );
            ensure!(
                snapshot
                    .directories
                    .contains_key(Path::new(name).parent().unwrap().to_str().unwrap()),
                "missing archived metadata parent."
            );
        }
        for required in [
            "HEAD",
            "index",
            "commondir",
            "gitdir",
            "locked",
            "fr-creation.json",
        ] {
            ensure!(
                snapshot.metadata.contains_key(required),
                "missing archived metadata file."
            );
        }
        let archived_receipt: ownership::Receipt =
            serde_json::from_slice(&snapshot.metadata["fr-creation.json"].bytes)?;
        ensure!(
            archived_receipt == *receipt
                && snapshot.metadata["HEAD"].bytes
                    == format!("ref: refs/heads/{}\n", plan.branch).as_bytes()
                && snapshot.checkout.index_digest.as_deref()
                    == Some(snapshot.metadata["index"].digest.as_str()),
            "inconsistent archived receipt, HEAD or index."
        );
        ensure!(
            snapshot.metadata["commondir"].bytes == b"../..\n"
                && snapshot.metadata["gitdir"].bytes
                    == format!("{}\n", plan.destination.join(".git").display()).as_bytes()
                && record.gitfile_bytes
                    == format!("gitdir: {}\n", receipt.metadata.display()).as_bytes(),
            "archived Git links do not match owned directories."
        );
        snapshot.gitfile.bytes = record.gitfile_bytes;
        ensure!(
            snapshot.gitfile.identity == receipt.gitfile_identity
                && snapshot.gitfile.digest == receipt.gitfile_digest
                && ownership::digest(&snapshot.gitfile.bytes) == snapshot.gitfile.digest
                && snapshot.gitfile.mode & 0o170000 == 0o100000,
            "invalid archived Git link."
        );
        let snapshot = record.snapshot;
        files.insert(".git".to_owned(), snapshot.gitfile);
        let trees = [
            Tree {
                root: plan.destination.clone(),
                files,
                directories: snapshot.checkout.directories,
            },
            Tree {
                root: receipt.metadata.clone(),
                files: snapshot.metadata,
                directories: snapshot.directories,
            },
        ];
        Ok(Self {
            root,
            path,
            archive_identity: directory(&parent)?,
            raw,
            plan: record.proposal,
            trees,
        })
    }

    pub(in crate::git::worktree::create::removal) fn check(&self) -> Result<()> {
        ensure!(
            directory(self.path.parent().unwrap())? == self.archive_identity
                && FileState::read_limited(&self.path, 128 * 1024 * 1024)? == self.raw,
            "removal archive changed during resumption."
        );
        ensure!(
            directory(&self.plan.common)? == self.plan.common_identity
                && directory(self.plan.destination.parent().unwrap())? == self.plan.parent_identity,
            "removal ownership parent changed."
        );
        let registration_parent = self.trees[1].root.parent().unwrap();
        if !super::super::super::absent(registration_parent)? {
            directory(registration_parent)?;
        }
        Ok(())
    }
}
