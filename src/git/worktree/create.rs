use super::{records, CreateOptions};
use crate::git::process;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

mod branch;
mod checkout;
mod ownership;
mod preparation;
pub(super) mod recovery;
pub(super) mod removal;

pub(super) fn args(values: &[&str]) -> Vec<OsString> {
    [
        "--no-replace-objects",
        "-c",
        "core.splitIndex=false",
        "-c",
        "core.sparseCheckout=false",
        "-c",
        "worktree.useRelativePaths=false",
        "-c",
        "branch.autoSetupMerge=false",
    ]
    .into_iter()
    .chain(values.iter().copied())
    .map(Into::into)
    .collect()
}

pub(super) fn checked(root: &Path, values: &[&str]) -> Result<Vec<u8>> {
    process::checked(root, &args(values))
}

pub(super) fn line(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes)?
        .strip_suffix('\n')
        .context("incomplete Git creation metadata.")
}

pub(super) fn directory(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_dir(),
        "worktree directory must be a real directory."
    );
    Ok((metadata.dev(), metadata.ino()))
}

pub(super) fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn common(root: &Path) -> Result<PathBuf> {
    let output = checked(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let path = Path::new(line(&output)?).canonicalize()?;
    path.to_str()
        .context("Git common directory must use UTF-8.")?;
    Ok(path)
}

pub(super) fn worktree_config(root: &Path) -> Result<bool> {
    let output = process::run(
        root,
        &args(&[
            "config",
            "--local",
            "--bool",
            "--get",
            "extensions.worktreeConfig",
        ]),
        None,
    )?;
    if output.status.code() == Some(1) {
        return Ok(false);
    }
    ensure!(
        output.status.success() && matches!(output.stdout.as_slice(), b"true\n" | b"false\n"),
        "cannot read extensions.worktreeConfig as a boolean."
    );
    Ok(output.stdout == b"true\n")
}

fn commit(root: &Path, revision: &str) -> Result<String> {
    let output = checked(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )?;
    let oid = line(&output)?;
    ensure!(process::oid(oid), "invalid worktree start commit.");
    Ok(oid.to_owned())
}

fn branch_present(root: &Path, branch: &str) -> Result<bool> {
    ensure!(
        !branch.is_empty() && !branch.starts_with('-') && branch.len() <= 1024,
        "invalid new worktree branch name."
    );
    let reference = format!("refs/heads/{branch}");
    checked(root, &["check-ref-format", &reference])?;
    ensure!(
        line(&checked(root, &["check-ref-format", "--branch", branch])?)? == branch,
        "new worktree branch must be a literal name."
    );
    let symbolic = process::run(
        root,
        &args(&["symbolic-ref", "--quiet", "--no-recurse", &reference]),
        None,
    )?;
    ensure!(
        symbolic.status.code() == Some(1),
        "new worktree branch is already symbolic or unreadable."
    );
    let output = process::run(
        root,
        &args(&["show-ref", "--verify", "--quiet", &reference]),
        None,
    )?;
    ensure!(
        matches!(output.status.code(), Some(0 | 1)),
        "worktree branch is unreadable."
    );
    Ok(output.status.success())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    version: String,
    root: PathBuf,
    common: PathBuf,
    common_identity: (u64, u64),
    #[serde(default)]
    worktree_config: bool,
    destination: PathBuf,
    parent_identity: (u64, u64),
    branch: String,
    #[serde(default)]
    existing_branch: bool,
    from: String,
    commit: String,
    tree: String,
    registrations: String,
    files: Vec<checkout::Entry>,
}

fn proposal(requested: &Path, options: &CreateOptions) -> Result<Proposal> {
    let requested = requested.canonicalize()?;
    let root = process::repository_root(&requested)?;
    ensure!(
        requested.starts_with(&root),
        "Git resolved a working tree outside the requested directory."
    );
    root.to_str()
        .context("Git repository root must use UTF-8.")?;
    let common = common(&root)?;
    let spelling = options
        .path
        .to_str()
        .context("worktree destination must use UTF-8.")?
        .trim_end_matches('/');
    ensure!(
        spelling != "." && !spelling.ends_with("/."),
        "worktree destination must name a new directory."
    );
    let input = root.join(&options.path);
    let name = input
        .file_name()
        .context("worktree destination must name a new directory.")?;
    ensure!(name != ".git", "invalid worktree destination name.");
    let parent = input
        .parent()
        .context("worktree destination needs a parent.")?
        .canonicalize()
        .context("worktree destination parent must already exist.")?;
    let destination = parent.join(name);
    destination
        .to_str()
        .context("worktree destination must use UTF-8.")?;
    ensure!(
        absent(&destination)?,
        "worktree destination already exists."
    );
    ensure!(
        !destination.starts_with(&common) && !common.starts_with(&destination),
        "worktree destination overlaps Git metadata."
    );
    let in_repository = process::run(&parent, &args(&["rev-parse", "--absolute-git-dir"]), None)?;
    ensure!(
        !in_repository.status.success(),
        "worktree destination parent must be outside Git repositories."
    );
    ensure!(
        in_repository.status.code() == Some(128)
            && in_repository
                .stderr
                .starts_with(b"fatal: not a git repository"),
        "cannot establish whether the destination parent is outside Git repositories."
    );
    let existing_branch = options.existing_branch.is_some();
    let selected_branch = options
        .existing_branch
        .as_ref()
        .or(options.branch.as_ref())
        .context("a new or existing branch is required.")?;
    let present = branch_present(&root, selected_branch)?;
    ensure!(
        crate::git::worktree_branch_selection_allowed(existing_branch, present, false),
        if existing_branch {
            "existing worktree branch does not exist."
        } else {
            "new worktree branch already exists."
        }
    );
    let worktree_config = worktree_config(&root)?;
    let registrations = checked(
        &root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?;
    let mut occupied = false;
    for entry in records::parse(&registrations, &root)? {
        let path = Path::new(&entry.path);
        ensure!(
            !destination.starts_with(path) && !path.starts_with(&destination),
            "worktree destination overlaps a registered worktree."
        );
        occupied |= entry.branch.as_deref() == Some(&format!("refs/heads/{selected_branch}"));
    }
    ensure!(
        crate::git::worktree_branch_selection_allowed(existing_branch, present, occupied),
        "branch belongs to a registered worktree."
    );
    let from = if existing_branch {
        format!("refs/heads/{selected_branch}")
    } else {
        options.from.clone().unwrap_or_else(|| "HEAD".to_owned())
    };
    let commit = commit(&root, &from)?;
    if existing_branch {
        branch::check(&root, selected_branch, &commit)?;
        let resolved = checked(
            &root,
            &[
                "rev-parse",
                "--symbolic-full-name",
                "--verify",
                "--end-of-options",
                selected_branch,
            ],
        )?;
        ensure!(
            line(&resolved)? == from,
            "existing worktree branch name is ambiguous."
        );
    }
    let output = checked(
        &root,
        &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )?;
    let tree = line(&output)?.to_owned();
    ensure!(process::oid(&tree), "invalid worktree tree identity.");
    let files = checkout::inventory(&root, &commit)?;
    Ok(Proposal {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        parent_identity: directory(&parent)?,
        common_identity: directory(&common)?,
        root,
        common,
        worktree_config,
        destination,
        branch: selected_branch.clone(),
        existing_branch,
        from,
        commit,
        tree,
        registrations: format!("{:x}", Sha256::digest(&registrations)),
        files,
    })
}

fn basis(proposal: &Proposal) -> Result<String> {
    Ok(format!(
        "frwtc1:{:x}",
        Sha256::digest(serde_json::to_vec(proposal)?)
    ))
}

pub(super) fn report(root: &Path, options: &CreateOptions) -> Result<Value> {
    ensure!(
        (1..=500).contains(&options.limit),
        "limit must be between 1 and 500."
    );
    let plan = proposal(root, options)?;
    let basis = basis(&plan)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale worktree creation basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1, "operation":if options.write {"worktree-create"} else {"worktree-create-preview"},
        "applied":false, "basis":basis, "basis_verified":options.basis.is_some(),
        "repository_root":plan.root, "common_directory":plan.common, "destination":plan.destination,
        "worktree_config":plan.worktree_config,
        "branch":format!("refs/heads/{}", plan.branch), "branch_action":if plan.existing_branch {"retain"}else{"create"}, "from":plan.from, "commit":plan.commit, "tree":plan.tree,
        "checkout":"raw-blobs", "hooks":"disabled", "content_filters":"bypassed", "lock_policy":"retain",
        "bytes":plan.files.iter().map(|entry| entry.size).sum::<usize>(),
        "files":plan.files.iter().take(options.limit).collect::<Vec<_>>(),
        "page":{"total":plan.files.len(), "returned":plan.files.len().min(options.limit),
            "omitted":plan.files.len().saturating_sub(options.limit)}});
    if options.write {
        let blobs = checkout::blobs(&plan)?;
        let _branch_lease = plan
            .existing_branch
            .then(|| branch::Lease::acquire(&plan.root, &plan.branch, &plan.commit))
            .transpose()?;
        ensure!(
            self::basis(&proposal(root, options)?)? == basis,
            "worktree creation basis changed before writing."
        );
        let outcome = checkout::apply(&plan, &blobs);
        match outcome {
            Ok(receipt) => {
                result["applied"] = json!(true);
                result["ownership_record"] = json!(receipt);
            }
            Err(error) => {
                result["applied"] = Value::Null;
                result["warning"] = json!("Creation is incomplete or unconfirmed. Inspect the destination, branch and worktree registrations before retrying.");
                let diagnostic = format!("{error:#}");
                result["diagnostic"] = json!(process::diagnostic(diagnostic.as_bytes()));
                result["diagnostic_truncated"] = json!(diagnostic.len() > 16 * 1024);
                result["observed_branch_commit"] =
                    json!(commit(&plan.root, &format!("refs/heads/{}", plan.branch)).ok());
                result["destination_present"] =
                    json!(absent(&plan.destination).ok().map(|absent| !absent));
                if let Ok(path) =
                    preparation::Prepared::record_path(&plan.common, &plan.destination)
                {
                    if !absent(&path).unwrap_or(true) {
                        result["preparation_record"] = json!(path);
                    }
                }
                result["recovery_hint"] = json!("Use fr git worktree recover PATH if creation recorded an ownership receipt or durable preparation.");
            }
        }
    }
    Ok(result)
}
