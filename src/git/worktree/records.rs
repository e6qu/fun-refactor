use crate::git::process;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[cfg(test)]
mod tests;

#[derive(Default, Debug, Serialize)]
pub(super) struct Entry {
    path: String,
    main: bool,
    current: bool,
    head: Option<String>,
    branch: Option<String>,
    pub(super) bare: bool,
    pub(super) detached: bool,
    pub(super) unborn: bool,
    pub(super) locked: bool,
    lock_reason: Option<String>,
    lock_reason_truncated: bool,
    pub(super) prunable: bool,
    prune_reason: Option<String>,
    prune_reason_truncated: bool,
}

fn reason(value: Option<&str>) -> (Option<String>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (Some(value[..end].to_owned()), end < value.len())
}

pub(super) fn parse(bytes: &[u8], root: &Path) -> Result<Vec<Entry>> {
    let bytes = bytes
        .strip_suffix(&[0])
        .context("Git worktree output lacks a NUL terminator.")?;
    let mut entries = Vec::new();
    let mut row: Option<Entry> = None;
    let mut labels = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for field in bytes.split(|byte| *byte == 0) {
        let field = std::str::from_utf8(field).context("non-UTF-8 Git worktree output")?;
        if field.is_empty() {
            let mut entry = row.take().context("empty Git worktree record")?;
            if entry.bare {
                if entry.head.is_some() || entry.branch.is_some() || entry.detached {
                    bail!("invalid bare Git worktree record");
                }
            } else {
                let head = entry
                    .head
                    .as_deref()
                    .context("Git worktree record lacks HEAD")?;
                if entry.detached == entry.branch.is_some() {
                    bail!("Git worktree record must have a branch or detached HEAD.");
                }
                if head.bytes().all(|byte| byte == b'0') {
                    if entry.detached {
                        bail!("detached Git worktree lacks a commit");
                    }
                    entry.unborn = true;
                    entry.head = None;
                }
            }
            entries.push(entry);
            labels.clear();
            continue;
        }
        let (label, value) = field
            .split_once(' ')
            .map_or((field, None), |(k, v)| (k, Some(v)));
        if !labels.insert(label.to_owned()) {
            bail!("duplicate Git worktree attribute: {label}.");
        }
        if label == "worktree" {
            if row.is_some() {
                bail!("Git worktree record lacks a separator");
            }
            let path = value.context("Git worktree record lacks a path")?;
            if !Path::new(path).is_absolute() || !paths.insert(path.to_owned()) {
                bail!("invalid or duplicate Git worktree path");
            }
            row = Some(Entry {
                path: path.to_owned(),
                main: entries.is_empty(),
                current: Path::new(path) == root,
                ..Entry::default()
            });
            continue;
        }
        let entry = row
            .as_mut()
            .context("Git worktree record must start with its path.")?;
        match (label, value) {
            ("HEAD", Some(value)) if process::oid(value) => entry.head = Some(value.to_owned()),
            ("branch", Some(value)) if value.starts_with("refs/heads/") && value.len() > 11 => {
                entry.branch = Some(value.to_owned());
            }
            ("bare", None) => entry.bare = true,
            ("detached", None) => entry.detached = true,
            ("locked", value) => {
                entry.locked = true;
                (entry.lock_reason, entry.lock_reason_truncated) = reason(value);
            }
            ("prunable", value) => {
                entry.prunable = true;
                (entry.prune_reason, entry.prune_reason_truncated) = reason(value);
            }
            _ => bail!("unsupported or invalid Git worktree attribute: {label}."),
        }
    }
    if row.is_some() || entries.is_empty() {
        bail!("incomplete Git worktree output");
    }
    if entries.iter().filter(|entry| entry.current).count() != 1 {
        bail!("Git worktree output does not identify the current working tree.");
    }
    Ok(entries)
}
