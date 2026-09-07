use super::{diff, process, status};
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

mod records;

#[derive(Args)]
pub struct Options {
    #[arg(long, group = "changes_basis", help = "Compare HEAD with the index.")]
    staged: bool,
    #[arg(
        long,
        group = "changes_basis",
        help = "Compare one commit with the working tree."
    )]
    since: Option<String>,
    #[arg(long, default_value_t = 50, help = "Maximum paths, from 1 to 500.")]
    limit: usize,
    #[arg(long, help = "Continue the same observed change metadata.")]
    cursor: Option<String>,
}

fn checked(root: &Path, args: &[OsString]) -> Result<Vec<u8>> {
    let out = process::run(root, args, None)?;
    if !out.status.success() {
        bail!(
            "Git change inspection failed: {}",
            process::diagnostic(&out.stderr)
        );
    }
    Ok(out.stdout)
}

fn preflight(root: &Path, base: Option<&str>) -> Result<()> {
    let unmerged = checked(root, &["ls-files".into(), "--unmerged".into(), "-z".into()])?;
    if !unmerged.is_empty() {
        bail!("unmerged paths require conflict inspection with fr git status.");
    }
    let index = checked(root, &["ls-files".into(), "-z".into()])?;
    let tree = if let Some(base) = base {
        checked(
            root,
            &[
                "ls-tree".into(),
                "-r".into(),
                "--name-only".into(),
                "-z".into(),
                base.into(),
            ],
        )?
    } else {
        Vec::new()
    };
    let paths = status::records(&index)?
        .into_iter()
        .chain(status::records(&tree)?)
        .collect::<BTreeSet<_>>();
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path);
        input.push(0);
    }
    process::require_no_filters(root, &input)
}

pub(super) fn report(root: &Path, options: &Options) -> Result<Value> {
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
    let base = if let Some(revision) = &options.since {
        diff::commit(&root, revision, false)?
    } else if options.staged {
        diff::commit(&root, "HEAD", true)?
    } else {
        None
    };
    preflight(&root, base.as_deref())?;
    let scope = if options.staged {
        "head-to-index"
    } else if options.since.is_some() {
        "commit-to-worktree"
    } else {
        "index-to-worktree"
    };
    let mut args: Vec<OsString> = [
        "-c",
        "diff.autoRefreshIndex=false",
        "diff",
        "--raw",
        "--numstat",
        "-z",
        "--no-abbrev",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-relative",
        "--diff-algorithm=myers",
        "--no-indent-heuristic",
        "--ignore-submodules=all",
    ]
    .map(Into::into)
    .to_vec();
    if options.staged {
        args.push("--cached".into());
    }
    if let Some(base) = &base {
        args.push(base.into());
    }
    args.push("--".into());
    let output = process::run(&root, &args, None)?;
    if !output.status.success() {
        bail!(
            "reading Git changes: {}",
            process::diagnostic(&output.stderr)
        );
    }
    if options.staged && base.is_none() && diff::commit(&root, "HEAD", true)?.is_some() {
        bail!("Git HEAD changed during unborn-branch inspection; retry the query.");
    }
    let entries = records::parse(&output.stdout)?;
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(&root, scope, &base, &entries))?)
    );
    let key = format!("frc1:{revision}");
    let start = if let Some(cursor) = &options.cursor {
        let (basis, offset) = cursor
            .rsplit_once(':')
            .context("invalid Git changes cursor")?;
        if basis != key {
            bail!("stale cursor or different changes query; restart Git changes.");
        }
        offset
            .parse::<usize>()
            .context("invalid Git changes offset")?
    } else {
        0
    };
    if start > entries.len() {
        bail!("Git changes cursor is beyond the result set");
    }
    let end = start + crate::project::page_length(entries.len(), start, options.limit);
    let count = |state| entries.iter().filter(|e| e.status == state).count();
    Ok(
        json!({"schema":1, "repository_root":root, "scope":scope, "base_commit":base,
        "configuration":"repository-only-without-content-filters", "renames":"disabled", "submodules":"ignored", "untracked":"excluded",
        "changes_revision":revision, "identity":"change-metadata", "clean":entries.is_empty(),
        "counts":{"paths":entries.len(), "added":count("A"), "deleted":count("D"), "modified":count("M"), "type_changed":count("T"),
            "binary":entries.iter().filter(|e| e.binary).count(), "detail_candidates":entries.iter().filter(|e| e.detail_candidate).count()},
        "page":{"total":entries.len(), "returned":end-start, "before":start, "remaining":entries.len()-end,
            "next":(end<entries.len()).then(||format!("{key}:{end}"))},
        "entries":entries[start..end], "diagnostics":process::diagnostic(&output.stderr), "diagnostics_truncated":output.stderr.len()>16*1024}),
    )
}
