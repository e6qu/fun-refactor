use super::Kind;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub(super) struct Entry {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_path: Option<String>,
    xy: String,
    index_status: Option<&'static str>,
    worktree_status: Option<&'static str>,
    untracked: bool,
    conflicted: bool,
    submodule: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    similarity: Option<u8>,
    #[serde(skip)]
    pub(super) fingerprint: String,
}

impl Entry {
    pub(super) fn matches(&self, kind: Kind) -> bool {
        match kind {
            Kind::All => true,
            Kind::Staged => !self.conflicted && self.index_status.is_some(),
            Kind::Unstaged => !self.conflicted && self.worktree_status.is_some(),
            Kind::Untracked => self.untracked,
            Kind::Conflicted => self.conflicted,
        }
    }
}

pub(super) struct Observation {
    pub(super) head: Option<String>,
    pub(super) branch: String,
    pub(super) entries: Vec<Entry>,
}

pub(super) fn records(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = bytes
        .strip_suffix(&[0])
        .context("Git output lacks a NUL terminator")?;
    let records = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if records.iter().any(|record| record.is_empty()) {
        bail!("Git output contains an empty record");
    }
    Ok(records)
}

fn path(value: &str) -> Result<String> {
    if value.is_empty()
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("invalid Git status path");
    }
    Ok(value.to_owned())
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn state(byte: u8) -> Result<Option<&'static str>> {
    Ok(match byte {
        b'.' => None,
        b'M' => Some("modified"),
        b'T' => Some("type-changed"),
        b'A' => Some("added"),
        b'D' => Some("deleted"),
        b'R' => Some("renamed"),
        b'C' => Some("copied"),
        b'U' => Some("unmerged"),
        _ => bail!("unknown Git status code"),
    })
}

pub(super) fn parse(bytes: &[u8]) -> Result<Observation> {
    let mut records = records(bytes)?.into_iter();
    let (mut head, mut branch) = (None, None);
    let mut entries = Vec::new();
    while let Some(raw) = records.next() {
        let line = std::str::from_utf8(raw).context("non-UTF-8 Git status output")?;
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            if head.is_some() || (value != "(initial)" && !oid(value)) {
                bail!("invalid Git status HEAD");
            }
            head = Some((value != "(initial)").then(|| value.to_owned()));
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            if branch.is_some() || value.is_empty() {
                bail!("invalid Git status branch");
            }
            branch = Some(value.to_owned());
            continue;
        }
        if line.starts_with("# ") {
            continue;
        }
        let mut digest = Sha256::new();
        digest.update(raw);
        digest.update([0]);
        let mut entry = Entry {
            path: String::new(),
            original_path: None,
            xy: "??".into(),
            index_status: None,
            worktree_status: None,
            untracked: false,
            conflicted: false,
            submodule: false,
            similarity: None,
            fingerprint: String::new(),
        };
        if let Some(value) = line.strip_prefix("? ") {
            entry.path = path(value)?;
            entry.untracked = true;
        } else {
            let (fields_count, modes, hashes) = match raw[0] {
                b'1' => (9, 3..6, 6..8),
                b'2' => (10, 3..6, 6..8),
                b'u' => (11, 3..7, 7..10),
                _ => bail!("unsupported Git status record"),
            };
            let fields = line.splitn(fields_count, ' ').collect::<Vec<_>>();
            if fields.len() != fields_count || fields[0].len() != 1 || fields[1].len() != 2 {
                bail!("invalid Git status fields");
            }
            if fields[modes].iter().any(|mode| {
                mode.len() != 6 || !mode.bytes().all(|byte| (b'0'..=b'7').contains(&byte))
            }) || fields[hashes].iter().any(|hash| !oid(hash))
            {
                bail!("invalid Git status mode or object identity");
            }
            let sub = fields[2].as_bytes();
            if sub != b"N..."
                && !(sub.len() == 4
                    && sub[0] == b'S'
                    && matches!(sub[1], b'.' | b'C')
                    && matches!(sub[2], b'.' | b'M')
                    && matches!(sub[3], b'.' | b'U'))
            {
                bail!("invalid Git submodule state");
            }
            entry.submodule = sub[0] == b'S';
            entry.xy = fields[1].into();
            let xy = fields[1].as_bytes();
            entry.conflicted = raw[0] == b'u';
            if entry.conflicted {
                if !matches!(fields[1], "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU") {
                    bail!("invalid Git conflict state");
                }
            } else if xy.contains(&b'U') || fields[1] == ".." {
                bail!("invalid Git tracked state");
            }
            if !entry.conflicted {
                entry.index_status = state(xy[0])?;
                entry.worktree_status = state(xy[1])?;
            }
            entry.path = path(fields[fields_count - 1])?;
            if raw[0] == b'2' {
                let score = fields[8];
                if !score.starts_with(['R', 'C']) || !xy.contains(&score.as_bytes()[0]) {
                    bail!("invalid Git similarity kind");
                }
                let score = score[1..]
                    .parse::<u8>()
                    .context("invalid Git similarity score")?;
                if score > 100 {
                    bail!("invalid Git similarity score");
                }
                entry.similarity = Some(score);
                let original = records
                    .next()
                    .context("Git rename lacks its original path")?;
                entry.original_path = Some(path(
                    std::str::from_utf8(original).context("non-UTF-8 Git rename path")?,
                )?);
                digest.update(original);
                digest.update([0]);
            } else if !entry.conflicted && xy.iter().any(|byte| matches!(byte, b'R' | b'C')) {
                bail!("Git rename lacks its original path");
            }
        }
        entry.fingerprint = format!("{:x}", digest.finalize());
        entries.push(entry);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    if entries.windows(2).any(|pair| pair[0].path == pair[1].path) {
        bail!("duplicate Git status path");
    }
    Ok(Observation {
        head: head.context("Git status omitted HEAD")?,
        branch: branch.context("Git status omitted branch")?,
        entries,
    })
}
