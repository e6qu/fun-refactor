use super::{History, Record, Snapshot, SnapshotKind, Status};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fmt::Write;
use std::path::Path;

mod check;
pub use check::{check_patch_basis, PatchBasisCheck, PatchBasisFile};
mod git;
pub use git::{check_git_patch, GitPatchCheck};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub struct PatchExport {
    pub id: u64,
    pub status: Status,
    pub record_basis: String,
    pub source_revision: String,
    pub validation: String,
    pub reverse: bool,
    pub format: &'static str,
    pub mode_scope: &'static str,
    pub files: usize,
    pub patch: String,
}

pub fn export_patch(root: &Path, id: u64, reverse: bool) -> Result<PatchExport> {
    let history = History::read(root)?;
    let record = history.record(id)?;
    Ok(PatchExport {
        id,
        status: record.status,
        record_basis: record.basis.clone(),
        source_revision: record.source_revision.clone(),
        validation: record.validation.clone(),
        reverse,
        format: "git-text-diff",
        mode_scope: "regular-executable-or-symlink",
        files: record.changes.len(),
        patch: render(record, reverse)?,
    })
}

pub fn git_mode(mode: u32) -> u32 {
    if mode & 0o100 == 0 {
        0o100644
    } else {
        0o100755
    }
}

pub fn git_snapshot_mode(symlink: bool, mode: u32) -> u32 {
    if symlink {
        0o120000
    } else {
        git_mode(mode)
    }
}

fn snapshot_git_mode(snapshot: &Snapshot) -> u32 {
    git_snapshot_mode(snapshot.kind == SnapshotKind::Symlink, snapshot.mode)
}

pub fn git_mode_change_supported(before: u32, after: u32) -> bool {
    let changed = before ^ after;
    changed & !0o111 == 0 && (changed == 0 || git_mode(before) != git_mode(after))
}

pub fn owner_executable_mode(mode: u32, executable: bool) -> u32 {
    if executable {
        mode | 0o100
    } else {
        mode & !0o100
    }
}

pub fn matches_patch_basis(actual: &Option<Snapshot>, expected: &Option<Snapshot>) -> bool {
    actual.as_ref().map(|s| (&s.content, snapshot_git_mode(s)))
        == expected
            .as_ref()
            .map(|s| (&s.content, snapshot_git_mode(s)))
}

fn quote(path: &str) -> String {
    let mut quoted = String::from("\"");
    for byte in path.bytes() {
        match byte {
            b'"' | b'\\' => {
                quoted.push('\\');
                quoted.push(char::from(byte));
            }
            b' '..=b'~' => quoted.push(char::from(byte)),
            _ => write!(quoted, "\\{byte:03o}").unwrap(),
        }
    }
    quoted.push('"');
    quoted
}

fn render(record: &Record, reverse: bool) -> Result<String> {
    let mut changes = record.changes.iter().collect::<Vec<_>>();
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let mut patch = String::new();
    for change in changes {
        let name = change
            .path
            .components()
            .map(|part| part.as_os_str().to_str().context("non-UTF-8 patch path"))
            .collect::<Result<Vec<_>>>()?
            .join("/");
        let (before, after) = if reverse {
            (&change.after, &change.before)
        } else {
            (&change.before, &change.after)
        };
        for snapshot in [before, after].into_iter().flatten() {
            if snapshot.content.contains('\0') {
                bail!("cannot export binary snapshot as a text patch: {name:?}");
            }
        }
        if let (Some(before), Some(after)) = (before, after) {
            let both_regular =
                before.kind == SnapshotKind::Regular && after.kind == SnapshotKind::Regular;
            if both_regular && !git_mode_change_supported(before.mode, after.mode) {
                bail!("cannot represent recorded permission change in a Git patch: {name:?}");
            }
        }
        let old = quote(&format!("a/{name}"));
        let new = quote(&format!("b/{name}"));
        if let (Some(before), Some(after)) = (before, after) {
            if before.kind != after.kind {
                writeln!(patch, "diff --git {old} {new}")?;
                writeln!(patch, "deleted file mode {:06o}", snapshot_git_mode(before))?;
                patch.push_str(
                    &similar::TextDiff::from_lines(before.content.as_str(), "")
                        .unified_diff()
                        .header(&old, "/dev/null")
                        .to_string(),
                );
                writeln!(patch, "diff --git {old} {new}")?;
                writeln!(patch, "new file mode {:06o}", snapshot_git_mode(after))?;
                patch.push_str(
                    &similar::TextDiff::from_lines("", after.content.as_str())
                        .unified_diff()
                        .header("/dev/null", &new)
                        .to_string(),
                );
                continue;
            }
        }
        writeln!(patch, "diff --git {old} {new}")?;
        match (before, after) {
            (None, Some(after)) => {
                writeln!(patch, "new file mode {:06o}", snapshot_git_mode(after))?
            }
            (Some(before), None) => {
                writeln!(patch, "deleted file mode {:06o}", snapshot_git_mode(before))?
            }
            (Some(before), Some(after))
                if snapshot_git_mode(before) != snapshot_git_mode(after) =>
            {
                writeln!(patch, "old mode {:06o}", snapshot_git_mode(before))?;
                writeln!(patch, "new mode {:06o}", snapshot_git_mode(after))?;
            }
            _ => (),
        }
        patch.push_str(
            &similar::TextDiff::from_lines(
                before.as_ref().map_or("", |s| &s.content),
                after.as_ref().map_or("", |s| &s.content),
            )
            .unified_diff()
            .header(
                if before.is_some() { &old } else { "/dev/null" },
                if after.is_some() { &new } else { "/dev/null" },
            )
            .to_string(),
        );
    }
    Ok(patch)
}
