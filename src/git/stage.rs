use super::{
    process,
    snapshot::{inventory, working_file, Blob},
    status,
};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

mod apply;
pub(super) mod journal;

#[derive(Args)]
pub struct Options {
    #[arg(
        required = true,
        value_name = "PATH",
        help = "Explicit repository-relative files; at most 32 arguments."
    )]
    paths: Vec<PathBuf>,
    #[arg(
        long,
        help = "Require the same selected index and working snapshots as this preview."
    )]
    basis: Option<String>,
    #[arg(
        long,
        requires = "basis",
        help = "Apply the reviewed raw entries to the Git index."
    )]
    write: bool,
}

#[derive(Serialize)]
struct Entry {
    path: String,
    action: &'static str,
    before: Option<Blob>,
    after: Option<Blob>,
    working_bytes: Option<usize>,
    #[serde(skip)]
    #[cfg_attr(not(unix), allow(dead_code))]
    source: Option<Vec<u8>>,
}

fn require_not_ignored(root: &Path, paths: &BTreeSet<String>) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let input = paths
        .iter()
        .flat_map(|path| path.bytes().chain([0]))
        .collect::<Vec<_>>();
    let output = process::run(
        root,
        &[
            "-c".into(),
            "core.excludesFile=/dev/null".into(),
            "check-ignore".into(),
            "--stdin".into(),
            "-z".into(),
        ],
        Some(&input),
    )?;
    ensure!(
        matches!(output.status.code(), Some(0 | 1)),
        "cannot check ignored staging paths: {}",
        process::diagnostic(&output.stderr)
    );
    let ignored = status::records(&output.stdout)?;
    ensure!(
        ignored.is_empty(),
        "ignored untracked paths are unsupported for staging previews."
    );
    Ok(())
}

pub(super) fn report(root: &Path, options: &Options) -> Result<Value> {
    ensure!(
        !options.paths.is_empty() && options.paths.len() <= 32,
        "staging previews require 1 through 32 explicit path arguments."
    );
    let mut paths = BTreeSet::new();
    for path in &options.paths {
        ensure!(
            !path.as_os_str().is_empty()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "staging previews require repository-relative file paths without parent traversal."
        );
        let path = path.components().collect::<PathBuf>();
        paths.insert(
            path.to_str()
                .context("staging paths must use UTF-8")?
                .to_owned(),
        );
    }
    let requested_root = root.canonicalize()?;
    let root = process::repository_root(&requested_root)?;
    root.to_str()
        .context("Git repository root must use UTF-8")?;
    ensure!(
        requested_root.starts_with(&root),
        "Git resolved a working tree outside the requested directory."
    );
    let lock = options
        .write
        .then(|| apply::IndexLock::acquire(&root))
        .transpose()?;
    let filter_paths = paths
        .iter()
        .flat_map(|path| path.bytes().chain([0]))
        .collect::<Vec<_>>();
    process::require_no_filters(&root, &filter_paths)?;
    let index = inventory(&root, &paths, None)?;
    let untracked = paths
        .iter()
        .filter(|path| !index.contains_key(*path))
        .cloned()
        .collect();
    require_not_ignored(&root, &untracked)?;
    let mut entries = Vec::new();
    for path in &paths {
        let before = index.get(path).cloned();
        let working = working_file(&root, path)?;
        ensure!(
            working
                .as_ref()
                .is_none_or(|(_, text)| !text.contains('\0')),
            "binary files are unsupported for staging previews."
        );
        let working_bytes = working.as_ref().map(|(_, text)| text.len());
        let (after, source) = match working {
            Some((blob, text)) => (Some(blob), options.write.then(|| text.into_bytes())),
            None => (None, None),
        };
        ensure!(
            before.is_some() || after.is_some(),
            "staging path is absent from the index and working tree: {path:?}."
        );
        let action = match (&before, &after) {
            (None, Some(_)) => "add",
            (Some(_), None) => "remove",
            _ if before == after => "unchanged",
            _ => "update",
        };
        entries.push(Entry {
            path: path.clone(),
            action,
            before,
            after,
            working_bytes,
            source,
        });
    }
    let basis = format!(
        "frstage1:{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &root,
            "raw-bytes-owner-executable",
            env!("CARGO_PKG_VERSION"),
            &entries,
        ))?)
    );
    if let Some(expected) = &options.basis {
        ensure!(
            expected == &basis,
            "stale staging basis or different query; request a new preview."
        );
    }
    ensure!(
        inventory(&root, &paths, None)? == index,
        "selected index entries changed during staging preview; retry the query."
    );
    for entry in &entries {
        let observed = working_file(&root, &entry.path)?.map(|(blob, _)| blob);
        ensure!(
            observed == entry.after,
            "selected working files changed during staging preview; retry the query."
        );
    }
    ensure!(
        inventory(&root, &paths, None)? == index,
        "selected index entries changed during staging preview; retry the query."
    );
    require_not_ignored(&root, &untracked)?;
    let durability = if let Some(lock) = lock {
        Some(lock.apply(&root, &entries, &filter_paths, &untracked)?)
    } else {
        None
    };
    let count = |action| {
        entries
            .iter()
            .filter(|entry| entry.action == action)
            .count()
    };
    Ok(
        json!({"schema":1,"repository_root":root,"operation":if options.write {"stage-apply"} else {"stage-preview"},
        "applied":options.write,"write_supported":cfg!(unix),"durability":durability,
        "basis":basis,"basis_verified":options.basis.is_some(),"staging_semantics":"raw-bytes-owner-executable",
        "configuration":"repository-only-without-content-filters","source_bodies":"omitted",
        "counts":{"paths":entries.len(),"add":count("add"),"update":count("update"),"remove":count("remove"),"unchanged":count("unchanged")},
        "entries":entries}),
    )
}
