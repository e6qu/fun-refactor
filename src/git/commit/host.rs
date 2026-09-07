use super::{publish, Options};
use crate::git::{
    process,
    stage::{
        apply::{index_path, IndexLock},
        journal::{require_plain_entries, Journal},
    },
    status,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn args(values: &[&str]) -> Vec<std::ffi::OsString> {
    let mut args = vec![
        "--no-replace-objects".into(),
        "-c".into(),
        "core.splitIndex=false".into(),
        "-c".into(),
        "core.ignorestat=false".into(),
        "-c".into(),
        "user.useConfigOnly=true".into(),
        "-c".into(),
        "commit.gpgSign=false".into(),
        "-c".into(),
        "i18n.commitEncoding=UTF-8".into(),
    ];
    args.extend(values.iter().map(Into::into));
    args
}

fn checked(root: &Path, values: &[&str]) -> Result<Vec<u8>> {
    process::checked(root, &args(values))
}

fn line(bytes: &[u8]) -> Result<String> {
    Ok(std::str::from_utf8(bytes)?
        .strip_suffix('\n')
        .context("incomplete Git commit metadata")?
        .to_owned())
}

fn oid(bytes: &[u8]) -> Result<String> {
    let oid = line(bytes)?;
    ensure!(process::oid(&oid), "invalid Git commit object identity.");
    Ok(oid)
}

#[derive(Serialize, PartialEq, Eq)]
pub(super) struct Head {
    pub(super) branch: String,
    pub(super) parent: Option<String>,
}

pub(super) fn head(root: &Path) -> Result<Head> {
    let symbolic = process::run(
        root,
        &args(&["symbolic-ref", "--quiet", "--no-recurse", "HEAD"]),
        None,
    )?;
    ensure!(
        symbolic.status.success(),
        "reviewed commits require an attached local branch."
    );
    let branch = line(&symbolic.stdout)?;
    ensure!(
        branch.starts_with("refs/heads/") && branch.len() <= 1024,
        "reviewed commits require a bounded local branch name."
    );
    checked(root, &["check-ref-format", &branch])?;
    let chained = process::run(
        root,
        &args(&["symbolic-ref", "--quiet", "--no-recurse", &branch]),
        None,
    )?;
    ensure!(
        chained.status.code() == Some(1),
        "symbolic branch chains are unsupported for reviewed commits."
    );
    let current = process::run(
        root,
        &args(&["rev-parse", "--verify", "--quiet", &branch]),
        None,
    )?;
    let parent = if current.status.success() {
        let parent = oid(&current.stdout)?;
        ensure!(
            checked(root, &["cat-file", "-t", &parent])? == b"commit\n",
            "reviewed parent must be a commit."
        );
        Some(parent)
    } else {
        ensure!(
            current.status.code() == Some(1),
            "cannot inspect the reviewed branch: {}",
            process::diagnostic(&current.stderr)
        );
        None
    };
    Ok(Head { branch, parent })
}

pub(super) fn observed_branch(root: &Path, branch: &str) -> Option<String> {
    let output = process::run(
        root,
        &args(&["rev-parse", "--verify", "--quiet", branch]),
        None,
    )
    .ok()?;
    output
        .status
        .success()
        .then(|| oid(&output.stdout).ok())
        .flatten()
}

fn index_bytes(index: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(index) {
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_file(),
                "reviewed commits require a regular Git index."
            );
            Ok(Some(fs::read(index)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn idle(root: &Path) -> Result<()> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = line(&checked(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", marker],
        )?)?;
        match fs::symlink_metadata(path) {
            Ok(_) => anyhow::bail!(
                "finish the active Git operation before creating a reviewed commit: {marker}."
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn identity(root: &Path, name: &str) -> Result<String> {
    let raw = line(&checked(root, &["var", name]).context(
        "reading commit identity; configure repository-local user.name and user.email",
    )?)?;
    let identity = raw.rsplitn(3, ' ').nth(2).context("invalid Git identity")?;
    ensure!(
        identity.ends_with('>') && !identity.contains('\n') && identity.len() <= 1024,
        "unsupported Git identity; configure repository-local names and email addresses."
    );
    Ok(identity.to_owned())
}

struct Scratch {
    _directory: tempfile::TempDir,
    index: PathBuf,
    objects: PathBuf,
}

impl Scratch {
    fn run(
        &self,
        root: &Path,
        values: &[&str],
        input: Option<&[u8]>,
        temporary_objects: bool,
    ) -> Result<Vec<u8>> {
        let output = process::run_with_objects(
            root,
            &args(values),
            input,
            Some(&self.index),
            temporary_objects.then_some(self.objects.as_path()),
        )?;
        ensure!(
            output.status.success(),
            "preparing reviewed commit failed: {}",
            process::diagnostic(&output.stderr)
        );
        Ok(output.stdout)
    }

    fn new(root: &Path, rows: &[u8]) -> Result<Self> {
        let directory = tempfile::Builder::new().prefix("fr-commit-").tempdir()?;
        let scratch = Self {
            index: directory.path().join("index"),
            objects: directory.path().join("objects"),
            _directory: directory,
        };
        fs::create_dir(&scratch.objects)?;
        scratch.run(root, &["read-tree", "--empty"], None, true)?;
        scratch.run(
            root,
            &["update-index", "-z", "--index-info"],
            Some(rows),
            true,
        )?;
        Ok(scratch)
    }
}

fn inventory(root: &Path) -> Result<(Vec<u8>, BTreeSet<String>)> {
    let rows = checked(root, &["ls-files", "--stage", "-z"])?;
    let mut paths = BTreeSet::new();
    let mut objects = BTreeSet::new();
    for row in status::records(&rows)? {
        let (meta, path) = std::str::from_utf8(row)?
            .split_once('\t')
            .context("invalid commit index inventory")?;
        let fields = meta.split(' ').collect::<Vec<_>>();
        ensure!(
            fields.len() == 3 && fields[2] == "0",
            "unmerged index entries are unsupported for reviewed commits."
        );
        ensure!(
            matches!(fields[0], "100644" | "100755" | "120000") && process::oid(fields[1]),
            "reviewed commits require regular file or symlink blobs; submodules are unsupported."
        );
        ensure!(
            paths.insert(path.to_owned()),
            "duplicate reviewed index path."
        );
        objects.insert(fields[1]);
    }
    require_plain_entries(root, &paths)?;
    if !objects.is_empty() {
        let input = objects
            .iter()
            .map(|oid| format!("{oid}\n"))
            .collect::<String>();
        let output = process::run(
            root,
            &args(&["cat-file", "--batch-check=%(objectname) %(objecttype)"]),
            Some(input.as_bytes()),
        )?;
        let expected = objects
            .iter()
            .map(|oid| format!("{oid} blob\n"))
            .collect::<String>();
        ensure!(
            output.status.success() && output.stdout == expected.as_bytes(),
            "reviewed index contains missing or non-blob objects."
        );
    }
    Ok((rows, paths))
}

#[derive(Serialize)]
struct Basis<'a> {
    root: &'a Path,
    version: &'static str,
    index: &'a Path,
    index_digest: String,
    head: &'a Head,
    tree: &'a str,
    message: &'a str,
    author: &'a str,
    committer: &'a str,
}

fn verify_commit(
    root: &Path,
    commit: &str,
    tree: &str,
    head: &Head,
    author: &str,
    committer: &str,
    message: &str,
) -> Result<()> {
    let content = checked(root, &["cat-file", "commit", commit])?;
    let content = std::str::from_utf8(&content)?;
    let (headers, body) = content
        .split_once("\n\n")
        .context("invalid created commit")?;
    let mut lines = headers.lines();
    ensure!(
        lines.next() == Some(format!("tree {tree}").as_str()),
        "created commit has a different tree."
    );
    if let Some(parent) = &head.parent {
        ensure!(
            lines.next() == Some(format!("parent {parent}").as_str()),
            "created commit has a different parent."
        );
    }
    ensure!(
        lines
            .next()
            .is_some_and(|line| line.starts_with(&format!("author {author} ")))
            && lines
                .next()
                .is_some_and(|line| line.starts_with(&format!("committer {committer} ")))
            && lines.next().is_none()
            && body == message,
        "created commit differs from the reviewed identities or message."
    );
    Ok(())
}

pub(in crate::git) fn report(root: &Path, options: &Options) -> Result<Value> {
    ensure!(
        !options.message.trim().is_empty()
            && options.message.len() <= 16 * 1024
            && !options.message.contains('\0'),
        "commit messages require nonempty UTF-8 text without NUL, at most 16 KiB."
    );
    ensure!(
        (1..=500).contains(&options.limit),
        "commit path limit must be 1 through 500."
    );
    let message = if options.message.ends_with('\n') {
        options.message.clone()
    } else {
        format!("{}\n", options.message)
    };
    let requested = root.canonicalize()?;
    let root = process::repository_root(&requested)?;
    ensure!(
        requested.starts_with(&root),
        "Git resolved a working tree outside the requested directory."
    );
    let lock = options
        .write
        .then(|| IndexLock::acquire(&root))
        .transpose()?;
    let index = index_path(&root)?;
    let bytes = index_bytes(&index)?;
    let journal = Journal::read(&root, &index)?;
    journal.ready()?;
    idle(&root)?;
    let before = head(&root)?;
    let author = identity(&root, "GIT_AUTHOR_IDENT")?;
    let committer = identity(&root, "GIT_COMMITTER_IDENT")?;
    let (rows, paths) = inventory(&root)?;
    ensure!(
        before.parent.is_some() || !paths.is_empty(),
        "nothing staged for an initial commit."
    );
    let scratch = Scratch::new(&root, &rows)?;
    let tree = oid(&scratch.run(&root, &["write-tree", "--missing-ok"], None, true)?)?;
    if let Some(parent) = &before.parent {
        let old_tree = oid(&checked(
            &root,
            &["rev-parse", &format!("{parent}^{{tree}}")],
        )?)?;
        ensure!(
            old_tree != tree,
            "nothing staged; empty commits are unsupported."
        );
    }
    let changed = checked(
        &root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )?;
    let changed = status::records(&changed)?
        .iter()
        .map(|path| Ok(std::str::from_utf8(path)?.to_owned()))
        .collect::<Result<Vec<_>>>()?;
    let index_digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&bytes)?));
    let proposal = Basis {
        root: &root,
        version: env!("CARGO_PKG_VERSION"),
        index: &index,
        index_digest,
        head: &before,
        tree: &tree,
        message: &message,
        author: &author,
        committer: &committer,
    };
    let basis = format!(
        "frcommit1:{:x}",
        Sha256::digest(serde_json::to_vec(&proposal)?)
    );
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale commit basis; request a new preview."
        );
    }
    let check = || -> Result<()> {
        let observed = head(&root)?;
        ensure!(
            crate::git::commit_basis_matches(
                &before.branch,
                &observed.branch,
                &before.parent,
                &observed.parent
            ),
            "HEAD changed during reviewed commit; request a new preview."
        );
        ensure!(
            index_bytes(&index)? == bytes,
            "index changed during reviewed commit; request a new preview."
        );
        ensure!(
            identity(&root, "GIT_AUTHOR_IDENT")? == author
                && identity(&root, "GIT_COMMITTER_IDENT")? == committer,
            "commit identities changed; request a new preview."
        );
        idle(&root)?;
        journal.check()?;
        if let Some(lock) = &lock {
            lock.check(&root)?;
        }
        Ok(())
    };
    check()?;
    let mut result = json!({"schema":1,"operation":if options.write {"commit-apply"} else {"commit-preview"},"applied":false,
        "repository_root":root,"scope":"entire-index","basis":basis,"basis_verified":options.basis.is_some(),"head":before,
        "tree":tree,"message":message,"author":author,"committer":committer,"timestamps":"write-time","signing":"disabled","hooks":"disabled",
        "source_bodies":"omitted","index_paths":paths.len(),"changed_paths":changed.iter().take(options.limit).collect::<Vec<_>>(),
        "page":{"total":changed.len(),"returned":changed.len().min(options.limit),"omitted":changed.len().saturating_sub(options.limit)}});
    if options.write {
        ensure!(
            oid(&scratch.run(&root, &["write-tree"], None, false)?)? == tree,
            "materialized commit tree differs from preview."
        );
        let mut values = vec!["commit-tree", "--no-gpg-sign", &tree];
        if let Some(parent) = &before.parent {
            values.extend(["-p", parent]);
        }
        let output = process::run(&root, &args(&values), Some(message.as_bytes()))?;
        ensure!(
            output.status.success(),
            "creating reviewed commit failed: {}",
            process::diagnostic(&output.stderr)
        );
        let commit = oid(&output.stdout)?;
        verify_commit(
            &root, &commit, &tree, &before, &author, &committer, &message,
        )?;
        check()?;
        let outcome = publish::publish(&root, &commit, &before, check)?;
        result["applied"] = outcome["confirmed"].clone();
        result["commit"] = json!(commit);
        result["reference_update"] = outcome;
    }
    Ok(result)
}
