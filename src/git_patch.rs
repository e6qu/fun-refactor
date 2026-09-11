use anyhow::{bail, Context, Result};
use std::fmt::Write;
use std::path::Path;

#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    pub content: &'a str,
    pub mode: u32,
    pub symlink: bool,
}

#[derive(Clone, Copy)]
pub struct Change<'a> {
    pub path: &'a Path,
    pub before: Option<Snapshot<'a>>,
    pub after: Option<Snapshot<'a>>,
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

fn mode(snapshot: Snapshot<'_>) -> u32 {
    if snapshot.symlink {
        0o120000
    } else if snapshot.mode & 0o100 == 0 {
        0o100644
    } else {
        0o100755
    }
}

fn mode_change_supported(before: Snapshot<'_>, after: Snapshot<'_>) -> bool {
    if before.symlink || after.symlink {
        return true;
    }
    let changed = before.mode ^ after.mode;
    changed & !0o111 == 0 && (changed == 0 || mode(before) != mode(after))
}

pub fn render(changes: &[Change<'_>], reverse: bool) -> Result<String> {
    let mut changes = changes.to_vec();
    changes.sort_by(|a, b| a.path.cmp(b.path));
    let mut patch = String::new();
    for change in changes {
        let name = change
            .path
            .components()
            .map(|part| part.as_os_str().to_str().context("non-UTF-8 patch path"))
            .collect::<Result<Vec<_>>>()?
            .join("/");
        let (before, after) = if reverse {
            (change.after, change.before)
        } else {
            (change.before, change.after)
        };
        for snapshot in [before, after].into_iter().flatten() {
            if snapshot.content.contains('\0') {
                bail!("cannot export binary snapshot as a text patch: {name:?}");
            }
        }
        if let (Some(before), Some(after)) = (before, after) {
            if !mode_change_supported(before, after) {
                bail!("cannot represent recorded permission change in a Git patch: {name:?}");
            }
        }

        let old = quote(&format!("a/{name}"));
        let new = quote(&format!("b/{name}"));
        if let (Some(before), Some(after)) = (before, after) {
            if before.symlink != after.symlink {
                writeln!(patch, "diff --git {old} {new}")?;
                writeln!(patch, "deleted file mode {:06o}", mode(before))?;
                patch.push_str(
                    &similar::TextDiff::from_lines(before.content, "")
                        .unified_diff()
                        .header(&old, "/dev/null")
                        .to_string(),
                );
                writeln!(patch, "diff --git {old} {new}")?;
                writeln!(patch, "new file mode {:06o}", mode(after))?;
                patch.push_str(
                    &similar::TextDiff::from_lines("", after.content)
                        .unified_diff()
                        .header("/dev/null", &new)
                        .to_string(),
                );
                continue;
            }
        }

        writeln!(patch, "diff --git {old} {new}")?;
        match (before, after) {
            (None, Some(after)) => writeln!(patch, "new file mode {:06o}", mode(after))?,
            (Some(before), None) => writeln!(patch, "deleted file mode {:06o}", mode(before))?,
            (Some(before), Some(after)) if mode(before) != mode(after) => {
                writeln!(patch, "old mode {:06o}", mode(before))?;
                writeln!(patch, "new mode {:06o}", mode(after))?;
            }
            _ => {}
        }
        patch.push_str(
            &similar::TextDiff::from_lines(
                before.map_or("", |snapshot| snapshot.content),
                after.map_or("", |snapshot| snapshot.content),
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
