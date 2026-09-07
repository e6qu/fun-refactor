use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

mod changes;
mod diff;
pub(crate) mod process;
mod status;

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Page through Git status without source text or index writes.")]
    Status(StatusOptions),
    #[command(about = "Inspect bounded hunk and line details for one Git path.")]
    Diff(diff::Options),
    #[command(about = "Page through changed paths and line counts for one Git comparison.")]
    Changes(changes::Options),
}

#[derive(Args)]
pub struct StatusOptions {
    #[arg(long, default_value_t = 50, help = "Maximum rows, from 1 to 500.")]
    limit: usize,
    #[arg(long, help = "Continue a page from the same status observation.")]
    cursor: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "all",
        help = "Select a class of changed paths."
    )]
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    All,
    Staged,
    Unstaged,
    Untracked,
    Conflicted,
}

pub fn report(root: &Path, command: &Command) -> Result<Value> {
    match command {
        Command::Status(options) => status_report(root, options),
        Command::Diff(options) => diff::report(root, options),
        Command::Changes(options) => changes::report(root, options),
    }
}

fn status_report(root: &Path, options: &StatusOptions) -> Result<Value> {
    if !(1..=500).contains(&options.limit) {
        bail!("limit must be between 1 and 500");
    }
    let requested_root = root.canonicalize()?;
    let root = process::repository_root(&requested_root)?;
    if !requested_root.starts_with(&root) {
        bail!("Git resolved a working tree outside the requested directory.");
    }
    let tracked = process::run(&root, &["ls-files".into(), "-z".into()], None)?;
    if !tracked.status.success() {
        bail!(
            "reading tracked Git paths: {}",
            process::diagnostic(&tracked.stderr)
        );
    }
    let mut paths = Vec::new();
    for path in status::records(&tracked.stdout)?
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        paths.extend_from_slice(path);
        paths.push(0);
    }
    process::require_no_filters(&root, &paths)?;
    let args = [
        "-c",
        "core.excludesFile=/dev/null",
        "-c",
        "status.submoduleSummary=false",
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--no-ahead-behind",
        "--untracked-files=all",
        "--ignored=no",
        "--ignore-submodules=all",
        "--find-renames=50%",
    ];
    let output = process::run(&root, &args.map(Into::into), None)?;
    if !output.status.success() {
        bail!(
            "reading Git status: {}",
            process::diagnostic(&output.stderr)
        );
    }
    let observed = status::parse(&output.stdout)?;
    let fingerprints = observed
        .entries
        .iter()
        .map(|entry| &entry.fingerprint)
        .collect::<Vec<_>>();
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &root,
            &observed.branch,
            &observed.head,
            &fingerprints,
            options.kind
        ))?)
    );
    let key = format!("frg1:{revision}");
    let all = &observed.entries;
    let count = |kind| all.iter().filter(|entry| entry.matches(kind)).count();
    let entries = all
        .iter()
        .filter(|entry| entry.matches(options.kind))
        .collect::<Vec<_>>();
    let start = if let Some(cursor) = &options.cursor {
        let (basis, start) = cursor
            .rsplit_once(':')
            .context("invalid Git status cursor")?;
        if basis != key {
            bail!("stale cursor or different status query; restart Git status.");
        }
        start
            .parse::<usize>()
            .context("invalid Git status offset")?
    } else {
        0
    };
    if start > entries.len() {
        bail!("Git status cursor is beyond the result set");
    }
    let end = start + crate::project::page_length(entries.len(), start, options.limit);
    Ok(json!({
        "schema": 1, "repository_root": root, "scope": "repository", "kind": options.kind,
        "configuration": "repository-only-without-content-filters", "submodules": "ignored",
        "branch": observed.branch, "head": observed.head, "unborn": observed.head.is_none(),
        "status_revision": revision, "clean": all.is_empty(),
        "counts": {"paths": all.len(), "staged": count(Kind::Staged), "unstaged": count(Kind::Unstaged),
            "untracked": count(Kind::Untracked), "conflicted": count(Kind::Conflicted)},
        "page": {"total": entries.len(), "returned": end - start, "before": start, "remaining": entries.len() - end,
            "next": (end < entries.len()).then(|| format!("{key}:{end}"))},
        "entries": entries[start..end], "diagnostics": process::diagnostic(&output.stderr),
        "diagnostics_truncated": output.stderr.len() > 16 * 1024
    }))
}

pub fn line_in_range(start: usize, end: usize, line: usize) -> bool {
    start <= line && line <= end
}

pub fn call_in_selection(
    incoming: bool,
    outgoing: bool,
    include_incoming: bool,
    include_outgoing: bool,
) -> bool {
    (incoming && include_incoming) || (outgoing && include_outgoing)
}
