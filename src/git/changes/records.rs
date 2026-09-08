use crate::git::{process, status};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};

#[derive(Serialize)]
pub(super) struct Entry {
    path: String,
    pub status: String,
    before_mode: Option<String>,
    after_mode: Option<String>,
    before_oid: Option<String>,
    after_oid: Option<String>,
    pub binary: bool,
    added_lines: Option<usize>,
    deleted_lines: Option<usize>,
    pub detail_candidate: bool,
    detail_gap: Option<&'static str>,
}

fn path(bytes: &[u8]) -> Result<String> {
    let path = std::str::from_utf8(bytes).context("non-UTF-8 Git changes path")?;
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        bail!("invalid Git changes path");
    }
    Ok(path.into())
}
fn mode(value: &str) -> Result<Option<String>> {
    match value {
        "000000" => Ok(None),
        "100644" | "100755" | "120000" | "160000" => Ok(Some(value.into())),
        _ => bail!("invalid Git changes mode"),
    }
}
fn count(value: &str) -> Result<usize> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        bail!("invalid Git line count");
    }
    value.parse().context("Git line count overflow")
}

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Entry>> {
    let mut records = status::records(bytes)?.into_iter().peekable();
    let mut entries = BTreeMap::new();
    while records.peek().is_some_and(|r| r.starts_with(b":")) {
        let raw = std::str::from_utf8(records.next().unwrap())
            .context("invalid Git raw change record")?;
        let fields = raw[1..].split(' ').collect::<Vec<_>>();
        if fields.len() != 5
            || !process::oid(fields[2])
            || !process::oid(fields[3])
            || fields[2].len() != fields[3].len()
        {
            bail!("invalid Git raw change fields");
        }
        if !matches!(fields[4], "A" | "D" | "M" | "T") {
            bail!("unsupported Git change state; inspect fr git status.");
        }
        let path = path(records.next().context("Git change lacks its path")?)?;
        let before_mode = mode(fields[0])?;
        let after_mode = mode(fields[1])?;
        if match fields[4] {
            "A" => before_mode.is_some() || after_mode.is_none(),
            "D" => before_mode.is_none() || after_mode.is_some(),
            _ => before_mode.is_none() || after_mode.is_none(),
        } {
            bail!("Git change state disagrees with its modes");
        }
        if matches!(fields[4], "M" | "T")
            && ((fields[0][..3] == fields[1][..3]) != (fields[4] == "M"))
        {
            bail!("Git change state disagrees with its file types");
        }
        let ids =
            [fields[2], fields[3]].map(|id| id.bytes().any(|b| b != b'0').then(|| id.to_owned()));
        if (before_mode.is_none() && ids[0].is_some()) || (after_mode.is_none() && ids[1].is_some())
        {
            bail!("Git change object identity disagrees with its mode");
        }
        let regular = [before_mode.as_deref(), after_mode.as_deref()]
            .iter()
            .all(|m| matches!(m, None | Some("100644" | "100755")));
        let entry = Entry {
            path: path.clone(),
            status: fields[4].into(),
            before_mode,
            after_mode,
            before_oid: ids[0].clone(),
            after_oid: ids[1].clone(),
            binary: false,
            added_lines: None,
            deleted_lines: None,
            detail_candidate: regular && fields[4] != "T",
            detail_gap: if regular {
                None
            } else {
                Some("non-regular-file")
            },
        };
        if entries.insert(path, (entry, false)).is_some() {
            bail!("duplicate Git raw change path");
        }
    }
    for record in records {
        let mut fields = record.splitn(3, |b| *b == b'\t');
        let added = std::str::from_utf8(fields.next().unwrap())?;
        let deleted =
            std::str::from_utf8(fields.next().context("Git change lacks deletion count")?)?;
        let path = path(
            fields
                .next()
                .context("Git change lacks its statistics path")?,
        )?;
        let (entry, seen) = entries
            .get_mut(&path)
            .context("Git statistics path has no raw change")?;
        if *seen {
            bail!("duplicate Git statistics path");
        }
        *seen = true;
        if added == "-" && deleted == "-" {
            entry.binary = true;
        } else {
            entry.added_lines = Some(count(added)?);
            entry.deleted_lines = Some(count(deleted)?);
        }
    }
    let mut result = Vec::new();
    for (entry, seen) in entries.into_values() {
        if !seen {
            if entry.status == "M"
                && entry.before_mode == entry.after_mode
                && entry.before_oid.is_some()
                && entry.after_oid.is_none()
            {
                continue;
            }
            bail!("Git change lacks its line statistics");
        }
        if entry.before_mode.as_deref() == Some("160000")
            || entry.after_mode.as_deref() == Some("160000")
        {
            continue;
        }
        result.push(entry);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
