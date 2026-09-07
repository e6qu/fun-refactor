use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct Change {
    status: String,
    before_mode: Option<String>,
    after_mode: Option<String>,
    before_oid: Option<String>,
    after_oid: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct Excerpt {
    text: String,
    bytes: usize,
    truncated: bool,
}

fn excerpt(text: &str, limit: usize) -> Excerpt {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Excerpt {
        text: text[..end].to_owned(),
        bytes: text.len(),
        truncated: end < text.len(),
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Range {
    start: usize,
    count: usize,
}

fn range(value: &str, prefix: char) -> Result<Range> {
    let value = value
        .strip_prefix(prefix)
        .context("invalid Git hunk range")?;
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    if [start, count]
        .iter()
        .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!("invalid Git hunk coordinates");
    }
    let result = Range {
        start: start.parse()?,
        count: count.parse()?,
    };
    if result.start == 0 && result.count != 0 {
        bail!("Git hunk lines start at one");
    }
    result
        .start
        .checked_add(result.count)
        .context("Git hunk range overflow")?;
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum Row {
    Hunk {
        number: usize,
        old: Range,
        new: Range,
        heading: Excerpt,
    },
    Line {
        hunk: usize,
        change: &'static str,
        old_line: Option<usize>,
        new_line: Option<usize>,
        content: Excerpt,
    },
    NoNewline {
        hunk: usize,
        old_line: Option<usize>,
        new_line: Option<usize>,
    },
}

pub(super) struct Observation {
    pub change: Option<Change>,
    pub binary: bool,
    pub hunks: usize,
    pub added: usize,
    pub deleted: usize,
    pub rows: Vec<Row>,
    pub blobs: Option<(Option<String>, Option<String>)>,
}

pub(super) fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn mode(value: &str) -> Result<Option<String>> {
    match value {
        "000000" => Ok(None),
        "100644" | "100755" => Ok(Some(value.into())),
        _ => bail!("Git diff supports regular files; symlinks and submodules are unsupported."),
    }
}

pub(super) fn parse(bytes: &[u8], path: &str) -> Result<Observation> {
    let mut out = Observation {
        change: None,
        binary: false,
        hunks: 0,
        added: 0,
        deleted: 0,
        rows: Vec::new(),
        blobs: None,
    };
    if bytes.is_empty() {
        return Ok(out);
    }
    let mut fields = bytes.splitn(3, |b| *b == 0);
    let raw = std::str::from_utf8(fields.next().unwrap())?;
    if fields.next() != Some(path.as_bytes()) {
        bail!("Git diff returned a different path");
    }
    let patch = fields
        .next()
        .and_then(|p| p.strip_prefix(&[0]))
        .context("Git diff requires exactly one raw file record.")?;
    let fields = raw
        .strip_prefix(':')
        .context("invalid Git raw diff")?
        .split(' ')
        .collect::<Vec<_>>();
    if fields.len() != 5 || !oid(fields[2]) || !oid(fields[3]) || fields[2].len() != fields[3].len()
    {
        bail!("invalid Git raw diff fields");
    }
    if !matches!(fields[4], "A" | "D" | "M") {
        bail!("unsupported Git diff state; inspect fr git status.");
    }
    let before_mode = mode(fields[0])?;
    let after_mode = mode(fields[1])?;
    if match fields[4] {
        "A" => before_mode.is_some() || after_mode.is_none(),
        "D" => before_mode.is_none() || after_mode.is_some(),
        _ => before_mode.is_none() || after_mode.is_none(),
    } {
        bail!("Git diff state disagrees with its modes");
    }
    if patch.is_empty()
        && fields[4] == "M"
        && before_mode == after_mode
        && fields[3].bytes().all(|b| b == b'0')
    {
        return Ok(out);
    }
    out.change = Some(Change {
        status: fields[4].into(),
        before_mode,
        after_mode,
        before_oid: fields[2]
            .bytes()
            .any(|b| b != b'0')
            .then(|| fields[2].into()),
        after_oid: fields[3]
            .bytes()
            .any(|b| b != b'0')
            .then(|| fields[3].into()),
    });
    out.blobs = out
        .change
        .as_ref()
        .map(|c| (c.before_oid.clone(), c.after_oid.clone()));
    let mut index_seen = false;
    let text = std::str::from_utf8(patch).context("non-UTF-8 Git patch text")?;
    let text = text
        .strip_suffix('\n')
        .context("incomplete Git patch text")?;
    let mut lines = text.split('\n');
    if !lines.next().is_some_and(|s| s.starts_with("diff --git ")) {
        bail!("Git diff omitted its patch header");
    }
    let (mut old_next, mut new_next, mut old_left, mut new_left) = (0, 0, 0, 0);
    let mut previous = None;
    let mut file_headers = (false, false);
    for line in lines {
        if let Some(header) = line.strip_prefix("@@ ") {
            if old_left != 0 || new_left != 0 || out.binary || !file_headers.0 || !file_headers.1 {
                bail!("incomplete or inconsistent Git hunk");
            }
            let (ranges, heading) = header
                .split_once(" @@")
                .context("invalid Git hunk header")?;
            let (old, new) = ranges
                .split_once(' ')
                .context("invalid Git hunk coordinates")?;
            let old = range(old, '-')?;
            let new = range(new, '+')?;
            if old.count == 0 && new.count == 0 {
                bail!("empty Git hunk");
            }
            let heading = if heading.is_empty() {
                heading
            } else {
                heading
                    .strip_prefix(' ')
                    .context("invalid Git hunk heading")?
            };
            old_next = old.start;
            new_next = new.start;
            old_left = old.count;
            new_left = new.count;
            out.rows.push(Row::Hunk {
                number: out.hunks,
                old,
                new,
                heading: excerpt(heading, 256),
            });
            out.hunks += 1;
            previous = None;
            continue;
        }
        if out.hunks > 0 {
            if line == "\\ No newline at end of file" {
                let (old_line, new_line) = previous
                    .take()
                    .context("Git newline marker lacks a preceding line.")?;
                out.rows.push(Row::NoNewline {
                    hunk: out.hunks - 1,
                    old_line,
                    new_line,
                });
                continue;
            }
            let (change, old, new) = match line.as_bytes().first() {
                Some(b' ') => ("context", true, true),
                Some(b'+') => ("add", false, true),
                Some(b'-') => ("delete", true, false),
                _ => bail!("unsupported Git hunk line"),
            };
            if (old && old_left == 0) || (new && new_left == 0) {
                bail!("Git hunk exceeds its declared ranges");
            }
            let old_line = old.then_some(old_next);
            let new_line = new.then_some(new_next);
            if old {
                old_next += 1;
                old_left -= 1;
            }
            if new {
                new_next += 1;
                new_left -= 1;
            }
            out.added += usize::from(change == "add");
            out.deleted += usize::from(change == "delete");
            out.rows.push(Row::Line {
                hunk: out.hunks - 1,
                change,
                old_line,
                new_line,
                content: excerpt(&line[1..], 1024),
            });
            previous = Some((old_line, new_line));
        } else if line.starts_with("--- ") {
            if file_headers.0 {
                bail!("duplicate Git old-file header");
            }
            file_headers.0 = true;
        } else if line.starts_with("+++ ") {
            if file_headers.1 || !file_headers.0 {
                bail!("invalid Git new-file header");
            }
            file_headers.1 = true;
        } else if line.starts_with("Binary files ") && line.ends_with(" differ") {
            if out.binary || file_headers.0 || file_headers.1 {
                bail!("invalid Git binary marker");
            }
            out.binary = true;
        } else if let Some(index) = line.strip_prefix("index ") {
            let ids = index.split(' ').next().unwrap();
            let (old, new) = ids
                .split_once("..")
                .context("invalid Git patch object identities")?;
            if index_seen || !oid(old) || !oid(new) || old.len() != new.len() {
                bail!("invalid Git patch object identities");
            }
            index_seen = true;
            let ids = [old, new].map(|id| id.bytes().any(|b| b != b'0').then(|| id.to_owned()));
            let change = out.change.as_ref().unwrap();
            if change
                .before_oid
                .as_ref()
                .is_some_and(|id| Some(id) != ids[0].as_ref())
                || change
                    .after_oid
                    .as_ref()
                    .is_some_and(|id| Some(id) != ids[1].as_ref())
                || (change.before_mode.is_none() && ids[0].is_some())
                || (change.after_mode.is_none() && ids[1].is_some())
            {
                bail!("Git patch object identities disagree with its raw record.");
            }
            out.blobs = Some((ids[0].clone(), ids[1].clone()));
        } else if [
            "old mode ",
            "new mode ",
            "new file mode ",
            "deleted file mode ",
        ]
        .iter()
        .any(|prefix| {
            line.strip_prefix(prefix)
                .is_some_and(|m| matches!(m, "100644" | "100755"))
        }) {
        } else {
            bail!("unsupported Git patch header");
        }
    }
    if old_left != 0 || new_left != 0 || (file_headers.0 || file_headers.1) && out.hunks == 0 {
        bail!("incomplete Git hunk body");
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
