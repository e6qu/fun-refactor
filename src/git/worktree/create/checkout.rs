use super::{absent, args, checked, common, directory, line, Proposal};
use crate::git::{process, status};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub(super) path: String,
    pub(super) mode: String,
    oid: String,
    pub(super) size: usize,
}

pub(super) fn inventory(root: &Path, commit: &str) -> Result<Vec<Entry>> {
    let output = checked(
        root,
        &["ls-tree", "-r", "--full-tree", "-z", "--long", commit],
    )?;
    let mut entries = Vec::new();
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for record in status::records(&output)? {
        let record = std::str::from_utf8(record).context("worktree paths must use UTF-8.")?;
        let (metadata, path) = record
            .split_once('\t')
            .context("invalid worktree tree entry.")?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        ensure!(
            fields.len() == 4
                && matches!(fields[0], "100644" | "100755")
                && fields[1] == "blob"
                && process::oid(fields[2]),
            "raw worktree creation supports regular blobs only."
        );
        ensure!(
            !path.is_empty()
                && Path::new(path)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                && path
                    .split('/')
                    .all(|part| !part.eq_ignore_ascii_case(".git")),
            "unsafe raw worktree path."
        );
        ensure!(
            paths.insert(path.to_lowercase()),
            "worktree paths collide under case folding."
        );
        let size = fields[3]
            .parse::<usize>()
            .context("worktree blob is missing or has an invalid size.")?;
        total = total.checked_add(size).context("worktree size overflow.")?;
        ensure!(
            crate::git::worktree_budget_allows(entries.len() + 1, total, size),
            "worktree exceeds 20000 files, 256 MiB total or 32 MiB per blob."
        );
        entries.push(Entry {
            path: path.to_owned(),
            mode: fields[0].to_owned(),
            oid: fields[2].to_owned(),
            size,
        });
    }
    let mut spellings = BTreeMap::new();
    for entry in &entries {
        for path in Path::new(&entry.path).ancestors() {
            let spelling = path.to_str().context("worktree paths must use UTF-8.")?;
            if let Some(previous) = spellings.insert(spelling.to_lowercase(), spelling) {
                ensure!(
                    previous == spelling,
                    "worktree directory paths collide under case folding."
                );
            }
        }
        for parent in Path::new(&entry.path).ancestors().skip(1) {
            ensure!(
                !paths.contains(&parent.to_string_lossy().to_lowercase()),
                "worktree file and directory paths collide."
            );
        }
    }
    Ok(entries)
}

pub(super) fn blobs(plan: &Proposal) -> Result<Vec<Vec<u8>>> {
    let mut input = String::new();
    for entry in &plan.files {
        input.push_str(&entry.oid);
        input.push('\n');
    }
    let output = process::run(
        &plan.root,
        &args(&["cat-file", "--batch"]),
        Some(input.as_bytes()),
    )?;
    ensure!(
        output.status.success(),
        "reading worktree blobs: {}",
        process::diagnostic(&output.stderr)
    );
    let mut rest = output.stdout.as_slice();
    let mut blobs = Vec::new();
    for entry in &plan.files {
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .context("incomplete worktree blob header.")?;
        let expected = format!("{} blob {}", entry.oid, entry.size);
        ensure!(
            rest[..end] == *expected.as_bytes(),
            "worktree blob metadata changed."
        );
        rest = &rest[end + 1..];
        ensure!(
            rest.len() > entry.size && rest[entry.size] == b'\n',
            "incomplete worktree blob body."
        );
        blobs.push(rest[..entry.size].to_vec());
        rest = &rest[entry.size + 1..];
    }
    ensure!(rest.is_empty(), "unexpected trailing worktree blob data.");
    Ok(blobs)
}

pub(super) fn check_directory(plan: &Proposal, identity: (u64, u64)) -> Result<()> {
    ensure!(
        directory(plan.destination.parent().unwrap())? == plan.parent_identity
            && directory(&plan.destination)? == identity
            && directory(&plan.common)? == plan.common_identity,
        "worktree directory identity changed."
    );
    Ok(())
}

pub(super) fn check_registration(plan: &Proposal) -> Result<()> {
    ensure!(
        process::repository_root(&plan.destination)? == plan.destination
            && common(&plan.destination)? == plan.common,
        "new worktree resolves to another repository."
    );
    let head = checked(
        &plan.destination,
        &["symbolic-ref", "--quiet", "--no-recurse", "HEAD"],
    )?;
    ensure!(
        line(&head)? == format!("refs/heads/{}", plan.branch),
        "new worktree branch changed."
    );
    let commit = checked(&plan.destination, &["rev-parse", "--verify", "HEAD"])?;
    ensure!(
        line(&commit)? == plan.commit,
        "new worktree commit changed."
    );
    let registrations = checked(
        &plan.root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?;
    let entries = super::records::parse(&registrations, &plan.root)?;
    ensure!(
        entries
            .iter()
            .any(|entry| Path::new(&entry.path) == plan.destination && entry.locked),
        "new worktree registration is missing or unlocked."
    );
    if plan.existing_branch {
        super::branch::check(&plan.root, &plan.branch, &plan.commit)?;
        ensure!(
            entries
                .iter()
                .filter(
                    |entry| entry.branch.as_deref() == Some(&format!("refs/heads/{}", plan.branch))
                )
                .count()
                == 1,
            "existing branch acquired another registered worktree."
        );
    }
    Ok(())
}

pub(super) fn index_path(plan: &Proposal) -> Result<PathBuf> {
    let output = checked(
        &plan.destination,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let path = Path::new(line(&output)?);
    let parent = path
        .parent()
        .context("new worktree index lacks a directory.")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(plan.common.join("worktrees"))
            && path.file_name() == Some("index".as_ref()),
        "new worktree index is outside linked metadata."
    );
    directory(&parent)?;
    Ok(parent.join("index"))
}

pub(super) fn populate(
    plan: &Proposal,
    blobs: &[Vec<u8>],
    identity: (u64, u64),
    resume: bool,
) -> Result<()> {
    let mut directories = BTreeMap::from([(plan.destination.clone(), identity)]);
    let mut files = BTreeMap::new();
    for (entry, bytes) in plan.files.iter().zip(blobs) {
        check_directory(plan, identity)?;
        let mut parent = plan.destination.clone();
        for component in Path::new(&entry.path).parent().unwrap().components() {
            parent.push(component);
            if let Some(expected) = directories.get(&parent) {
                ensure!(
                    directory(&parent)? == *expected,
                    "checkout directory changed."
                );
            } else {
                if !resume || absent(&parent)? {
                    DirBuilder::new().mode(0o700).create(&parent)?;
                }
                directories.insert(parent.clone(), directory(&parent)?);
            }
        }
        let path = plan.destination.join(&entry.path);
        if resume && !absent(&path)? {
            let stat = fs::symlink_metadata(&path)?;
            ensure!(
                stat.file_type().is_file()
                    && (stat.mode() & 0o100 != 0) == (entry.mode == "100755")
                    && super::ownership::bytes(&path, entry.size as u64)? == *bytes,
                "existing recovery file differs from its committed bytes or mode."
            );
            files.insert(path, (stat.dev(), stat.ino()));
            continue;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("creating fresh worktree file {:?}", entry.path))?;
        file.write_all(bytes)?;
        file.set_permissions(Permissions::from_mode(if entry.mode == "100755" {
            0o755
        } else {
            0o644
        }))?;
        file.sync_all()?;
        let metadata = file.metadata()?;
        files.insert(path, (metadata.dev(), metadata.ino()));
    }
    for (path, expected) in directories.iter().rev() {
        ensure!(directory(path)? == *expected, "checkout directory changed.");
        File::open(path)?.sync_all()?;
    }
    for (entry, bytes) in plan.files.iter().zip(blobs) {
        let path = plan.destination.join(&entry.path);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.file_type().is_file()
                && files.get(&path) == Some(&(metadata.dev(), metadata.ino()))
                && (metadata.mode() & 0o100 != 0) == (entry.mode == "100755")
                && fs::read(&path)? == *bytes,
            "raw worktree file changed during creation."
        );
    }
    Ok(())
}

pub(super) fn apply(plan: &Proposal, blobs: &[Vec<u8>]) -> Result<PathBuf> {
    ensure!(
        directory(plan.destination.parent().unwrap())? == plan.parent_identity
            && directory(&plan.common)? == plan.common_identity,
        "worktree parent or repository changed before creation."
    );
    let preparation = super::preparation::Prepared::create(plan)?;
    DirBuilder::new().mode(0o700).create(&plan.destination)?;
    let identity = directory(&plan.destination)?;
    check_directory(plan, identity)?;
    let mut registration_args = vec![
        "worktree",
        "add",
        "--no-checkout",
        "--lock",
        "--reason",
        "fr: reviewed raw worktree",
    ];
    if plan.existing_branch {
        registration_args.extend([
            "--no-guess-remote",
            "--",
            plan.destination.to_str().unwrap(),
            &plan.branch,
        ]);
    } else {
        registration_args.extend([
            "--no-track",
            "-b",
            &plan.branch,
            "--",
            plan.destination.to_str().unwrap(),
            &plan.commit,
        ]);
    }
    let registration =
        checked(&plan.root, &registration_args).context("registering reviewed worktree");
    check_directory(plan, identity)?;
    check_registration(plan)?;
    let (receipt, lease) =
        super::ownership::Receipt::record(plan, identity, Some(preparation.id()))?;
    let receipt_bytes = super::ownership::bytes(&receipt.path(), 64 * 1024)?;
    let preparation_lease = super::ownership::Lease::acquire(preparation.lock_path())?;
    preparation.remove(preparation_lease)?;
    registration?;
    let index_lease = prepare_index(plan, None)?;
    populate(plan, blobs, identity, false)?;
    verify_index(plan)?;
    sync(plan)?;
    check_directory(plan, identity)?;
    check_registration(plan)?;
    index_lease.check()?;
    receipt.finish(&lease, &receipt_bytes)
}

pub(super) fn prepare_index(
    plan: &Proposal,
    expected: Option<&[u8]>,
) -> Result<super::ownership::Lease> {
    let index = index_path(plan)?;
    let lease = super::ownership::Lease::acquire(index.with_extension("lock"))?;
    if let Some(expected) = expected {
        ensure!(
            super::ownership::bytes(&index, 64 * 1024 * 1024)? == expected,
            "recovery index changed."
        );
        verify_index(plan)?;
        lease.check()?;
        return Ok(lease);
    }
    ensure!(absent(&index)?, "new worktree index already exists.");
    let temp = tempfile::tempdir_in(index.parent().unwrap())?;
    let prepared = temp.path().join("index");
    let output = process::run_with_index(
        &plan.destination,
        &args(&["read-tree", "--no-sparse-checkout", "--reset", &plan.commit]),
        None,
        Some(&prepared),
    )?;
    ensure!(
        output.status.success(),
        "preparing new worktree index: {}",
        process::diagnostic(&output.stderr)
    );
    let file = File::open(&prepared)?;
    ensure!(
        file.metadata()?.is_file(),
        "prepared index is not a regular file."
    );
    file.sync_all()?;
    lease.check()?;
    ensure!(
        absent(&index)?,
        "new worktree index appeared during preparation."
    );
    fs::hard_link(&prepared, &index)
        .context("installing fresh worktree index without replacement")?;
    File::open(index.parent().unwrap())?.sync_all()?;
    Ok(lease)
}

pub(super) fn verify_index(plan: &Proposal) -> Result<()> {
    let output = checked(&plan.destination, &["ls-files", "--stage", "-v", "-z"])?;
    let observed = status::records(&output)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let expected = plan
        .files
        .iter()
        .map(|entry| format!("H {} {} 0\t{}", entry.mode, entry.oid, entry.path))
        .collect::<Vec<_>>();
    ensure!(
        observed == expected.iter().map(|entry| entry.as_bytes()).collect(),
        "new worktree index differs from the reviewed tree."
    );
    let diff = |visibility| {
        checked(
            &plan.destination,
            &[
                "diff",
                "--cached",
                "--name-only",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                visibility,
                &plan.commit,
                "--",
            ],
        )
    };
    ensure!(
        diff("--ita-visible-in-index")? == diff("--ita-invisible-in-index")?,
        "intent-to-add entries are unsupported for worktree recovery."
    );
    Ok(())
}

pub(super) fn sync(plan: &Proposal) -> Result<()> {
    let index = index_path(plan)?;
    ensure!(
        fs::symlink_metadata(&index)?.file_type().is_file(),
        "new worktree index is not a regular file."
    );
    File::open(&index)?.sync_all()?;
    File::open(index.parent().unwrap())?.sync_all()?;
    File::open(plan.destination.parent().unwrap())?.sync_all()?;
    Ok(())
}
