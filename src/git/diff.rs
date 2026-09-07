use super::{process, status};
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

mod patch;
mod symbols;

#[derive(Args)]
pub struct Options {
    #[arg(help = "One literal file path relative to the repository root.")]
    path: PathBuf,
    #[arg(long, group = "diff_basis", help = "Compare HEAD with the index.")]
    staged: bool,
    #[arg(
        long,
        group = "diff_basis",
        help = "Compare one commit with the working tree."
    )]
    since: Option<String>,
    #[arg(long, help = "Page through declarations overlapping changed lines.")]
    symbols: bool,
    #[arg(long, default_value_t = 50, help = "Maximum rows, from 1 to 500.")]
    limit: usize,
    #[arg(long, help = "Continue the same observed diff.")]
    cursor: Option<String>,
}

fn checked(root: &Path, args: &[OsString]) -> Result<Vec<u8>> {
    let output = process::run(root, args, None)?;
    if !output.status.success() {
        bail!(
            "Git diff inspection failed: {}",
            process::diagnostic(&output.stderr)
        );
    }
    Ok(output.stdout)
}

pub(super) fn commit(root: &Path, revision: &str, optional: bool) -> Result<Option<String>> {
    let output = process::run(
        root,
        &[
            "rev-parse".into(),
            "--verify".into(),
            "--quiet".into(),
            "--end-of-options".into(),
            format!("{revision}^{{commit}}").into(),
        ],
        None,
    )?;
    if optional && output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!(
            "cannot resolve a Git commit: {}",
            process::diagnostic(&output.stderr)
        );
    }
    let value = std::str::from_utf8(&output.stdout)?
        .strip_suffix('\n')
        .context("invalid Git commit report")?;
    if !patch::oid(value) {
        bail!("invalid Git commit identity");
    }
    Ok(Some(value.to_owned()))
}

fn selection(root: &Path, path: &str, base: Option<&str>) -> Result<()> {
    let index = checked(
        root,
        &[
            "--literal-pathspecs".into(),
            "ls-files".into(),
            "--stage".into(),
            "-z".into(),
            "--".into(),
            path.into(),
        ],
    )?;
    let mut found = false;
    for row in status::records(&index)? {
        let row = std::str::from_utf8(row).context("non-UTF-8 Git index entry")?;
        let (metadata, actual) = row.split_once('\t').context("invalid Git index entry")?;
        if actual != path {
            bail!("Git diff requires one explicit file path; directories are unsupported.");
        }
        let fields = metadata.split(' ').collect::<Vec<_>>();
        if fields.len() != 3 || !patch::oid(fields[1]) {
            bail!("invalid Git index fields");
        }
        if fields[2] != "0" {
            bail!("unmerged paths require conflict inspection with fr git status.");
        }
        if !matches!(fields[0], "100644" | "100755") {
            bail!("Git diff supports regular files; symlinks and submodules are unsupported.");
        }
        found = true;
    }
    if let Some(base) = base {
        let tree = checked(
            root,
            &[
                "--literal-pathspecs".into(),
                "ls-tree".into(),
                "-z".into(),
                base.into(),
                "--".into(),
                path.into(),
            ],
        )?;
        for row in status::records(&tree)? {
            let row = std::str::from_utf8(row).context("non-UTF-8 Git tree entry")?;
            let (metadata, actual) = row.split_once('\t').context("invalid Git tree entry")?;
            if actual != path {
                bail!("Git diff requires one explicit file path");
            }
            let fields = metadata.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || !patch::oid(fields[2]) {
                bail!("invalid Git tree fields");
            }
            if fields[1] != "blob" || !matches!(fields[0], "100644" | "100755") {
                bail!("Git diff supports regular files; directories, symlinks and submodules are unsupported.");
            }
            found = true;
        }
    }
    if !found {
        bail!("path is absent from the selected index and commit; inspect fr git status for untracked or staged paths.");
    }
    let mut input = path.as_bytes().to_vec();
    input.push(0);
    process::require_no_filters(root, &input)
}

pub(super) fn report(root: &Path, options: &Options) -> Result<Value> {
    if !(1..=500).contains(&options.limit) {
        bail!("limit must be between 1 and 500");
    }
    if options.path.as_os_str().is_empty()
        || options
            .path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        bail!("Git diff requires a repository-relative file path without parent traversal.");
    }
    let path = options.path.components().collect::<PathBuf>();
    let path = path.to_str().context("Git diff paths must use UTF-8")?;
    let requested_root = root.canonicalize()?;
    let root = process::repository_root(&requested_root)?;
    root.to_str()
        .context("Git repository root must use UTF-8")?;
    if !requested_root.starts_with(&root) {
        bail!("Git resolved a working tree outside the requested directory.");
    }
    let base = if let Some(revision) = &options.since {
        commit(&root, revision, false)?
    } else if options.staged {
        commit(&root, "HEAD", true)?
    } else {
        None
    };
    selection(&root, path, base.as_deref())?;
    let scope = if options.staged {
        "head-to-index"
    } else if options.since.is_some() {
        "commit-to-worktree"
    } else {
        "index-to-worktree"
    };
    let mut args: Vec<OsString> = [
        "--literal-pathspecs",
        "-c",
        "diff.autoRefreshIndex=false",
        "-c",
        "diff.suppressBlankEmpty=false",
        "diff",
        "--raw",
        "--patch",
        "-z",
        "--no-abbrev",
        "--full-index",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-relative",
        "--diff-algorithm=myers",
        "--no-indent-heuristic",
        "--unified=3",
        "--inter-hunk-context=0",
        "--no-function-context",
        "--ignore-submodules=all",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--output-indicator-new=+",
        "--output-indicator-old=-",
        "--output-indicator-context= ",
    ]
    .map(Into::into)
    .to_vec();
    if options.staged {
        args.push("--cached".into());
    }
    if let Some(base) = &base {
        args.push(base.into());
    }
    args.extend(["--".into(), path.into()]);
    let output = process::run(&root, &args, None)?;
    if !output.status.success() {
        bail!("reading Git diff: {}", process::diagnostic(&output.stderr));
    }
    if options.staged && base.is_none() && commit(&root, "HEAD", true)?.is_some() {
        bail!("Git HEAD changed during unborn-branch inspection; retry the query");
    }
    let observed = patch::parse(&output.stdout, path)?;
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&(&root, path, scope, &base))?);
    digest.update([0]);
    digest.update(&output.stdout);
    let revision = format!("{:x}", digest.finalize());
    let symbol_view = if options.symbols {
        Some(symbols::collect(&root, path, options.staged, &observed)?)
    } else {
        None
    };
    let (total, structure, key) = if let Some(view) = &symbol_view {
        let identity = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &revision,
                env!("CARGO_PKG_VERSION"),
                &view.entries,
                &view.coverage,
            ))?)
        );
        let structure = json!({"revision": identity, "coverage": view.coverage,
            "scope": "changed-line-overlap", "hierarchy": "strict-span-containment", "locals": "omitted",
            "cross_side_matching": "none", "relationships": "not-collected", "text_bytes": 256});
        (
            view.entries.len(),
            Some(structure),
            format!("frs1:{identity}"),
        )
    } else {
        (observed.rows.len(), None, format!("frd1:{revision}"))
    };
    let start = if let Some(cursor) = &options.cursor {
        let (basis, offset) = cursor.rsplit_once(':').context("invalid Git diff cursor")?;
        if basis != key {
            bail!("stale cursor or different diff query; restart Git diff.");
        }
        offset.parse::<usize>().context("invalid Git diff offset")?
    } else {
        0
    };
    if start > total {
        bail!("Git diff cursor is beyond the result set");
    }
    let end = start + crate::project::page_length(total, start, options.limit);
    let entries = if let Some(view) = &symbol_view {
        serde_json::to_value(&view.entries[start..end])?
    } else {
        serde_json::to_value(&observed.rows[start..end])?
    };
    Ok(json!({
        "schema": 1, "repository_root": root, "path": path, "scope": scope, "base_commit": base,
        "configuration": "repository-only-without-content-filters", "renames": "disabled", "submodules": "unsupported",
        "diff_revision": revision, "changed": observed.change.is_some(), "change": observed.change, "binary": observed.binary,
        "counts": {"hunks": observed.hunks, "added": (!observed.binary).then_some(observed.added), "deleted": (!observed.binary).then_some(observed.deleted)},
        "limits": {"line_bytes": 1024, "heading_bytes": 256, "context_lines": 3},
        "page": {"total": total, "returned": end-start, "before": start, "remaining": total-end,
            "next": (end < total).then(|| format!("{key}:{end}"))},
        "entries": entries, "view": if options.symbols {"symbols"} else {"lines"}, "structure": structure, "diagnostics": process::diagnostic(&output.stderr), "diagnostics_truncated": output.stderr.len() > 16*1024
    }))
}
