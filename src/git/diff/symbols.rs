use super::{checked, patch};
use crate::extract::Extractor;
use crate::lang::Language;
use crate::parse::Parsers;
use crate::project::CallDirection;
use crate::span::{LineIndex, Span};

mod calls;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) struct View {
    pub entries: Vec<Value>,
    pub coverage: Value,
}

fn source(root: &Path, path: &str, oid: &str, working: bool) -> Result<String> {
    if !patch::oid(oid) {
        bail!("invalid symbol snapshot identity");
    }
    let text = if working {
        let mut selected = root.to_path_buf();
        for part in Path::new(path).components() {
            selected.push(part);
            if std::fs::symlink_metadata(&selected)?
                .file_type()
                .is_symlink()
            {
                bail!("symbol snapshot path traverses a symlink");
            }
        }
        if !std::fs::symlink_metadata(&selected)?.is_file() {
            bail!("symbol snapshot requires a regular working file");
        }
        crate::vfs::read_to_string(&selected).context("reading working symbol snapshot")?
    } else {
        String::from_utf8(checked(
            root,
            &["cat-file".into(), "blob".into(), oid.into()],
        )?)
        .context("non-UTF-8 symbol snapshot")?
    };
    let output = crate::git::process::run(
        root,
        &[
            "hash-object".into(),
            "--no-filters".into(),
            "--stdin".into(),
        ],
        Some(text.as_bytes()),
    )?;
    if !output.status.success() || output.stdout != format!("{oid}\n").as_bytes() {
        bail!("symbol snapshot differs from the Git diff. Retry, or inspect conversion attributes with fr git diff without --symbols.");
    }
    Ok(text)
}

fn bounded(text: &str) -> Value {
    let mut end = text.len().min(256);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    json!({"text": &text[..end], "bytes": text.len(), "truncated": end < text.len()})
}

fn side(
    path: &Path,
    side: &str,
    source: &str,
    changed: &BTreeSet<usize>,
    language: Language,
    calls: Option<CallDirection>,
) -> Result<(Vec<Value>, Value)> {
    let parsed = Parsers::new().parse(language, source)?;
    let facts = Extractor::new().extract(&parsed, path, source)?;
    let lines = LineIndex::new(source);
    if changed
        .iter()
        .any(|line| *line == 0 || *line > lines.line_count())
    {
        bail!("changed line is outside its symbol snapshot");
    }
    let mut symbols = facts
        .symbols
        .iter()
        .filter(|symbol| !symbol.kind.is_local())
        .collect::<Vec<_>>();
    symbols.sort_by_key(|symbol| {
        (
            symbol.full_span.start,
            std::cmp::Reverse(symbol.full_span.end),
            symbol.id,
        )
    });
    let mut stack: Vec<(Span, String)> = Vec::new();
    let mut entries = Vec::new();
    let mut mapped = BTreeSet::new();
    let mut selected = BTreeSet::new();
    for symbol in symbols {
        let span = symbol.full_span;
        if span.start >= span.end || source.get(span.start..span.end).is_none() {
            bail!("invalid declaration span in symbol snapshot");
        }
        let first = lines.line_col(span.start, source).line;
        let mut last_byte = span.end - 1;
        while !source.is_char_boundary(last_byte) {
            last_byte -= 1;
        }
        let last = lines.line_col(last_byte, source).line;
        let overlaps = changed
            .iter()
            .copied()
            .filter(|line| crate::git::line_in_range(first, last, *line))
            .collect::<Vec<_>>();
        while stack
            .last()
            .is_some_and(|(parent, _)| *parent == span || !parent.contains(span))
        {
            stack.pop();
        }
        let id = format!("{side}:{}", symbol.id.0);
        if !overlaps.is_empty() {
            selected.insert(symbol.id);
            mapped.extend(overlaps.iter().copied());
            entries.push(json!({"id": id, "side": side, "parent": stack.last().map(|(_, id)| id),
                "kind": symbol.kind.as_str(), "name": bounded(&symbol.name), "qualifier": symbol.qualifier.as_deref().map(bounded),
                "exported": symbol.exported, "span": span, "line": first, "end_line": last,
                "changed_lines": overlaps.len()}));
        }
        stack.push((span, id));
    }
    let gaps = facts
        .gaps
        .iter()
        .map(|gap| gap.as_str())
        .collect::<Vec<_>>();
    let mut coverage = json!({"status": if gaps.is_empty() {"parsed"} else {"partial"},
        "language": language, "gaps": gaps, "changed_lines": changed.len(),
        "mapped_lines": mapped.len(), "unmapped_lines": changed.len()-mapped.len(), "declarations": entries.len()});
    if let Some(direction) = calls {
        let result = calls::collect(path, side, source, language, &facts, &selected, direction)?;
        entries = result.0;
        coverage["calls"] = result.1;
    }
    Ok((entries, coverage))
}

pub(super) fn collect(
    root: &Path,
    path: &str,
    staged: bool,
    observed: &patch::Observation,
    calls: Option<CallDirection>,
) -> Result<View> {
    let extension = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let language = Language::ALL
        .iter()
        .copied()
        .find(|lang| lang.extensions().contains(&extension.as_str()));
    let mut changed = [BTreeSet::new(), BTreeSet::new()];
    for row in &observed.rows {
        if let patch::Row::Line {
            change,
            old_line,
            new_line,
            ..
        } = row
        {
            if *change == "delete" {
                changed[0].extend(old_line);
            }
            if *change == "add" {
                changed[1].extend(new_line);
            }
        }
    }
    let mut view = View {
        entries: Vec::new(),
        coverage: json!({"language_detection": "extension-only"}),
    };
    for (index, name) in ["before", "after"].iter().enumerate() {
        let oid = observed.blobs.as_ref().and_then(|ids| {
            if index == 0 {
                ids.0.as_deref()
            } else {
                ids.1.as_deref()
            }
        });
        let status = if observed.binary {
            Some("binary")
        } else if changed[index].is_empty() {
            Some("no-changed-lines")
        } else if language.is_none() {
            Some("unsupported-language")
        } else {
            None
        };
        let (entries, mut coverage) = if let Some(status) = status {
            (
                Vec::new(),
                json!({"status": status, "changed_lines": (!observed.binary).then_some(changed[index].len()), "mapped_lines": null, "unmapped_lines": null, "declarations": 0}),
            )
        } else {
            let oid = oid.context("Git diff lacks a blob identity required for symbol mapping")?;
            let text = source(root, path, oid, index == 1 && !staged)?;
            side(
                Path::new(path),
                name,
                &text,
                &changed[index],
                language.unwrap(),
                calls,
            )?
        };
        coverage["blob"] = json!(oid);
        view.coverage[name] = coverage;
        view.entries.extend(entries);
    }
    Ok(view)
}
