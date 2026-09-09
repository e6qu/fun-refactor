use super::super::super::{absent, checked, directory, ownership};
use super::super::{branch, configuration, FileState};
use super::record::{Loaded, Tree};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub(super) struct Row {
    pub(super) scope: &'static str,
    pub(super) path: String,
    pub(super) kind: &'static str,
    pub(super) state: &'static str,
}

impl Row {
    pub(super) fn blocked(&self) -> bool {
        !["missing", "remaining"].contains(&self.state)
    }
}

#[derive(Serialize)]
pub(super) struct Observation {
    pub(super) rows: Vec<Row>,
    pub(super) complete: bool,
    registrations: String,
}

impl Observation {
    pub(super) fn can_resume(&self) -> bool {
        !self.complete
            && self.rows.iter().all(|row| {
                let remaining = row.state == "remaining";
                !row.blocked()
                    && crate::git::worktree_removal_resume_allowed(
                        row.state != "missing",
                        remaining,
                        remaining,
                        remaining,
                    )
            })
    }
}

fn row(
    rows: &mut Vec<Row>,
    scope: &'static str,
    path: &str,
    kind: &'static str,
    state: &'static str,
) {
    rows.push(Row {
        scope,
        path: path.to_owned(),
        kind,
        state,
    });
}

fn tree(tree: &Tree, scope: &'static str, owned: bool, rows: &mut Vec<Row>) -> Result<()> {
    let mut pending = vec![String::new()];
    let mut seen = BTreeSet::new();
    while let Some(relative) = pending.pop() {
        let path = tree.root.join(&relative);
        if absent(&path)? {
            continue;
        }
        let expected = tree.directories.get(&relative);
        if directory(&path).ok().as_ref() != expected {
            row(rows, scope, &relative, "directory", "replaced");
            seen.insert(relative);
            continue;
        }
        seen.insert(relative.clone());
        row(rows, scope, &relative, "directory", "remaining");
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let relative = Path::new(&relative).join(entry.file_name());
            let Some(name) = relative.to_str() else {
                row(rows, scope, "<non-UTF-8 path>", "unknown", "unexpected");
                continue;
            };
            if owned
                && scope == "metadata"
                && ["index.lock", "HEAD.lock", "fr-creation.lock"].contains(&name)
            {
                continue;
            }
            if tree.directories.contains_key(name) {
                pending.push(name.to_owned());
                continue;
            }
            seen.insert(name.to_owned());
            if let Some(expected) = tree.files.get(name) {
                let current = FileState::read(&entry.path());
                let state = match current {
                    Ok(current)
                        if crate::git::worktree_removal_resume_allowed(
                            true,
                            current.identity == expected.identity,
                            current.bytes == expected.bytes,
                            current.mode == expected.mode,
                        ) =>
                    {
                        "remaining"
                    }
                    Ok(_) => "changed",
                    Err(_) => "unreadable-or-unsupported",
                };
                row(rows, scope, name, "file", state);
            } else {
                row(rows, scope, name, "unknown", "unexpected");
            }
        }
    }
    let uninspected = rows
        .iter()
        .filter(|row| row.scope == scope && row.kind == "directory" && row.blocked())
        .map(|row| row.path.clone())
        .collect::<Vec<_>>();
    for (name, kind) in tree
        .files
        .keys()
        .map(|name| (name, "file"))
        .chain(tree.directories.keys().map(|name| (name, "directory")))
    {
        if !seen.contains(name) {
            let state = if uninspected
                .iter()
                .any(|parent| Path::new(name).starts_with(parent))
            {
                "uninspected"
            } else {
                "missing"
            };
            row(rows, scope, name, kind, state);
        }
    }
    Ok(())
}

pub(super) fn observe(loaded: &Loaded, owned: bool) -> Result<Observation> {
    loaded.check()?;
    let mut rows = Vec::new();
    tree(&loaded.trees[0], "checkout", false, &mut rows)?;
    tree(&loaded.trees[1], "metadata", owned, &mut rows)?;
    if configuration(&loaded.root, loaded.plan.worktree_config).is_err() {
        row(
            &mut rows,
            "repository",
            "configuration",
            "guard",
            "unsupported",
        );
    }
    if branch::check(&loaded.root, &loaded.plan.branch, &loaded.plan.commit).is_err() {
        row(&mut rows, "repository", "branch", "guard", "changed");
    }
    if !owned && !absent(&loaded.path.with_file_name("resume.lock"))? {
        row(&mut rows, "archive", "resume.lock", "lock", "locked");
    }
    let marker = loaded.path.with_file_name("complete");
    let complete = if absent(&marker)? {
        false
    } else {
        if ownership::bytes(&marker, 256).ok().as_deref() != Some(super::COMPLETE) {
            row(&mut rows, "archive", "complete", "marker", "invalid");
        }
        true
    };
    let registrations = ownership::digest(&checked(
        &loaded.root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?);
    rows.sort_by(|a, b| (a.scope, &a.path, a.kind).cmp(&(b.scope, &b.path, b.kind)));
    loaded.check()?;
    Ok(Observation {
        rows,
        complete,
        registrations,
    })
}

pub(super) fn parents(tree: &Tree, relative: &str) -> Result<()> {
    for parent in Path::new(relative).ancestors().skip(1) {
        let name = parent.to_str().unwrap();
        ensure!(
            directory(&tree.root.join(parent))? == tree.directories[name],
            "removal parent directory changed."
        );
    }
    Ok(())
}

pub(super) fn directories(tree: &Tree, observation: &Observation, scope: &str) -> Vec<PathBuf> {
    let mut paths = observation
        .rows
        .iter()
        .filter(|row| row.scope == scope && row.kind == "directory" && row.state == "remaining")
        .map(|row| PathBuf::from(&row.path))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    paths.retain(|path| tree.directories.contains_key(path.to_str().unwrap()));
    paths
}
