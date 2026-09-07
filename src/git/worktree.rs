use super::process;
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(unix)]
mod create;
mod records;

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Page through registered worktree paths, HEADs and lock metadata.")]
    List(ListOptions),
    #[command(about = "Preview or create a raw worktree on a new or unused existing branch.")]
    Create(CreateOptions),
    #[command(about = "Preview or finish a recorded incomplete raw checkout.")]
    Recover(RecoverOptions),
    #[command(about = "Preview or remove an owned clean worktree while retaining its branch.")]
    Remove(RemoveOptions),
    #[command(about = "Inspect or resume a recorded incomplete worktree removal.")]
    ResumeRemoval(ResumeRemovalOptions),
    #[command(about = "Preview or compact a completed worktree removal archive.")]
    CompactRemoval(CompactRemovalOptions),
}

#[derive(Args)]
pub struct CompactRemovalOptions {
    #[arg(help = "Removal record.json path, relative to the repository root or absolute.")]
    record: std::path::PathBuf,
    #[arg(
        long,
        help = "Require the reviewed completed archive and absent worktree paths."
    )]
    basis: Option<String>,
    #[arg(
        long,
        requires = "basis",
        help = "Retain an audit summary and discard the full recovery record."
    )]
    write: bool,
}

#[derive(Args)]
pub struct ResumeRemovalOptions {
    #[arg(help = "Removal record.json path, relative to the repository root or absolute.")]
    record: std::path::PathBuf,
    #[arg(
        long,
        help = "Require the reviewed removal record and remaining paths."
    )]
    basis: Option<String>,
    #[arg(
        long,
        requires = "basis",
        help = "Remove remaining reviewed paths and confirm completion."
    )]
    write: bool,
    #[arg(
        long,
        default_value_t = 20,
        help = "Maximum inspection rows, from 1 to 500."
    )]
    limit: usize,
}

#[derive(Args)]
pub struct RemoveOptions {
    #[arg(help = "Owned linked worktree path, relative to the repository root or absolute.")]
    path: std::path::PathBuf,
    #[arg(long, help = "Require the reviewed checkout and private Git metadata.")]
    basis: Option<String>,
    #[arg(
        long,
        requires = "basis",
        help = "Archive metadata and remove reviewed files."
    )]
    write: bool,
    #[arg(long, default_value_t = 20, help = "Maximum file rows, from 1 to 500.")]
    limit: usize,
}

#[derive(Args)]
pub struct RecoverOptions {
    #[arg(help = "Recorded worktree path, relative to the invoking repository root or absolute.")]
    path: std::path::PathBuf,
    #[arg(
        long,
        help = "Require the reviewed ownership receipt and incomplete checkout."
    )]
    basis: Option<String>,
    #[arg(
        long,
        requires = "basis",
        help = "Create missing committed files and finish the receipt."
    )]
    write: bool,
    #[arg(
        long,
        default_value_t = 20,
        help = "Maximum missing paths, from 1 to 500."
    )]
    limit: usize,
}

#[derive(Args)]
pub struct CreateOptions {
    #[arg(help = "Fresh destination, relative to the repository root or absolute.")]
    path: std::path::PathBuf,
    #[arg(
        long,
        required_unless_present = "existing_branch",
        conflicts_with = "existing_branch",
        help = "New local branch name."
    )]
    branch: Option<String>,
    #[arg(
        long,
        conflicts_with = "from",
        help = "Unused existing local branch to attach without changing its tip."
    )]
    existing_branch: Option<String>,
    #[arg(long, help = "Start commit for a new branch; defaults to HEAD.")]
    from: Option<String>,
    #[arg(
        long,
        help = "Require the reviewed destination, branch, commit and registrations."
    )]
    basis: Option<String>,
    #[arg(long, requires = "basis", help = "Create the reviewed worktree.")]
    write: bool,
    #[arg(
        long,
        default_value_t = 20,
        help = "Maximum file metadata rows, from 1 to 500."
    )]
    limit: usize,
}

#[derive(Args)]
pub struct ListOptions {
    #[arg(long, default_value_t = 20, help = "Maximum rows, from 1 to 500.")]
    limit: usize,
    #[arg(long, help = "Continue the same worktree registration observation.")]
    cursor: Option<String>,
}

pub(super) fn report(root: &Path, command: &Command) -> Result<Value> {
    match command {
        Command::List(options) => list(root, options),
        #[cfg(unix)]
        Command::Create(options) => create::report(root, options),
        #[cfg(unix)]
        Command::Recover(options) => create::recovery::report(root, options),
        #[cfg(unix)]
        Command::Remove(options) => create::removal::report(root, options),
        #[cfg(unix)]
        Command::ResumeRemoval(options) => create::removal::resume::report(root, options),
        #[cfg(unix)]
        Command::CompactRemoval(options) => create::removal::compact::report(root, options),
        #[cfg(not(unix))]
        Command::CompactRemoval(_) => bail!("removal compaction requires Unix ownership checks."),
        #[cfg(not(unix))]
        Command::ResumeRemoval(_) => bail!("removal resumption requires Unix ownership checks."),
        #[cfg(not(unix))]
        Command::Remove(_) => bail!("worktree removal requires Unix ownership checks."),
        #[cfg(not(unix))]
        Command::Create(_) => bail!("worktree creation requires Unix directory ownership checks."),
        #[cfg(not(unix))]
        Command::Recover(_) => bail!("worktree recovery requires Unix ownership checks."),
    }
}

fn list(root: &Path, options: &ListOptions) -> Result<Value> {
    if !(1..=500).contains(&options.limit) {
        bail!("limit must be between 1 and 500");
    }
    let requested = root.canonicalize()?;
    let root = process::repository_root(&requested)?;
    root.to_str()
        .context("Git repository root must use UTF-8")?;
    if !requested.starts_with(&root) {
        bail!("Git resolved a working tree outside the requested directory.");
    }
    let common = process::checked(
        &root,
        &[
            "rev-parse".into(),
            "--path-format=absolute".into(),
            "--git-common-dir".into(),
        ],
    )?;
    let common = std::str::from_utf8(&common)?
        .strip_suffix('\n')
        .context("Git did not report its common directory")?;
    let common = Path::new(common).canonicalize()?;
    common
        .to_str()
        .context("Git common directory must use UTF-8")?;
    let output = process::run(
        &root,
        &[
            "worktree".into(),
            "list".into(),
            "--porcelain".into(),
            "-z".into(),
            "--expire=now".into(),
        ],
        None,
    )?;
    if !output.status.success() {
        bail!(
            "reading Git worktrees: {}",
            process::diagnostic(&output.stderr)
        );
    }
    let entries = records::parse(&output.stdout, &root)?;
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &root,
            &common,
            format!("{:x}", Sha256::digest(&output.stdout))
        ))?)
    );
    let key = format!("frwt1:{revision}");
    let start = if let Some(cursor) = &options.cursor {
        let (basis, offset) = cursor
            .rsplit_once(':')
            .context("invalid Git worktree cursor")?;
        if basis != key {
            bail!("stale cursor or different worktree query; restart worktree list.");
        }
        offset
            .parse::<usize>()
            .context("invalid Git worktree offset")?
    } else {
        0
    };
    if start > entries.len() {
        bail!("Git worktree cursor is beyond the result set");
    }
    let end = start + crate::project::page_length(entries.len(), start, options.limit);
    Ok(json!({
        "schema": 1, "repository_root": root, "common_directory": common,
        "scope": "registered-worktrees", "worktree_revision": revision,
        "configuration": "repository-only", "prunable_expiry": "now",
        "content_inspected": false, "atomic_snapshot": false,
        "counts": {
            "worktrees": entries.len(),
            "bare": entries.iter().filter(|row| row.bare).count(),
            "detached": entries.iter().filter(|row| row.detached).count(),
            "unborn": entries.iter().filter(|row| row.unborn).count(),
            "locked": entries.iter().filter(|row| row.locked).count(),
            "prunable": entries.iter().filter(|row| row.prunable).count()
        },
        "page": {"total": entries.len(), "returned": end - start, "before": start,
            "remaining": entries.len() - end,
            "next": (end < entries.len()).then(|| format!("{key}:{end}"))},
        "entries": entries[start..end],
        "diagnostics": process::diagnostic(&output.stderr),
        "diagnostics_truncated": output.stderr.len() > 16 * 1024
    }))
}
