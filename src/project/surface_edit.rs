use super::{hash, Project};
use crate::span::Span;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    CssClass,
    ClassToken,
    MarkdownHeading,
    MermaidNode,
}

impl Kind {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::CssClass => "css-class",
            Self::ClassToken => "class-token",
            Self::MarkdownHeading => "markdown-heading",
            Self::MermaidNode => "mermaid-node",
        }
    }

    pub(super) fn operation(self) -> &'static str {
        match self {
            Self::CssClass => "rename-css-class",
            Self::ClassToken => "replace-class-token",
            Self::MarkdownHeading => "rename-markdown-heading",
            Self::MermaidNode => "rename-mermaid-node",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) file: PathBuf,
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) value: String,
    pub(super) spans: Vec<Span>,
    pub(super) scope: String,
}

fn candidate(
    project: &Project<'_>,
    kind: Kind,
    file: &Path,
    line: usize,
    value: String,
    spans: Vec<Span>,
    scope: String,
) -> Result<Candidate> {
    let path = file.strip_prefix(&project.root)?.to_path_buf();
    let id = format!(
        "frse1:{}",
        &hash((
            &project.revision,
            kind.id(),
            &path,
            line,
            &value,
            &spans,
            &scope
        ))?[..32]
    );
    Ok(Candidate {
        id,
        kind,
        file: file.to_path_buf(),
        path,
        line,
        value,
        spans,
        scope,
    })
}

fn lines(source: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut offset = 0usize;
    source
        .split_inclusive('\n')
        .enumerate()
        .map(move |(index, raw)| {
            let start = offset;
            offset += raw.len();
            let text = raw.strip_suffix('\n').unwrap_or(raw);
            let text = text.strip_suffix('\r').unwrap_or(text);
            (index + 1, start, text)
        })
}

fn css_candidates(project: &Project<'_>, file: &Path, source: &str) -> Result<Vec<Candidate>> {
    let mut result = Vec::new();
    for (line, line_start, text) in lines(source) {
        let Some((selector, _)) = text.split_once('{') else {
            continue;
        };
        let bytes = selector.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'.'
                || at > 0
                    && (bytes[at - 1].is_ascii_alphanumeric()
                        || matches!(bytes[at - 1], b'_' | b'-' | b'\\'))
            {
                at += 1;
                continue;
            }
            let start = at + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-'))
            {
                end += 1;
            }
            if end > start {
                let value = selector[start..end].to_owned();
                result.push(candidate(
                    project,
                    Kind::CssClass,
                    file,
                    line,
                    value,
                    vec![Span::new(line_start + start, line_start + end)],
                    line.to_string(),
                )?);
            }
            at = end.max(at + 1);
        }
    }
    Ok(result)
}

fn class_attribute_candidates(
    project: &Project<'_>,
    file: &Path,
    source: &str,
) -> Result<Vec<Candidate>> {
    let mut result = Vec::new();
    for (line, line_start, text) in lines(source) {
        let mut cursor = 0usize;
        while cursor < text.len() {
            let rest = &text[cursor..];
            let class = rest.find("class").map(|at| (cursor + at, 5));
            let class_name = rest.find("className").map(|at| (cursor + at, 9));
            let Some((at, width)) = [class_name, class]
                .into_iter()
                .flatten()
                .min_by_key(|(at, width)| (*at, usize::MAX - *width))
            else {
                break;
            };
            let prefix = &text[..at];
            let in_tag = prefix.rfind('<') > prefix.rfind('>');
            let continuation = prefix.trim().is_empty();
            if !in_tag && !continuation || at > 0 && text.as_bytes()[at - 1].is_ascii_alphanumeric()
            {
                cursor = at + width;
                continue;
            }
            let mut value = at + width;
            while text
                .as_bytes()
                .get(value)
                .is_some_and(u8::is_ascii_whitespace)
            {
                value += 1;
            }
            if text.as_bytes().get(value) != Some(&b'=') {
                cursor = at + width;
                continue;
            }
            value += 1;
            while text
                .as_bytes()
                .get(value)
                .is_some_and(u8::is_ascii_whitespace)
            {
                value += 1;
            }
            if text.as_bytes().get(value) == Some(&b'{') {
                value += 1;
                while text
                    .as_bytes()
                    .get(value)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    value += 1;
                }
            }
            let Some(quote) = text
                .as_bytes()
                .get(value)
                .copied()
                .filter(|quote| matches!(quote, b'\'' | b'"' | b'`'))
            else {
                cursor = at + width;
                continue;
            };
            let literal_start = value + 1;
            let Some(relative_end) = text.as_bytes()[literal_start..]
                .iter()
                .position(|byte| *byte == quote)
            else {
                cursor = at + width;
                continue;
            };
            let literal_end = literal_start + relative_end;
            let literal = &text[literal_start..literal_end];
            if quote != b'`' || !literal.contains("${") {
                let bytes = literal.as_bytes();
                let mut token = 0usize;
                while token < bytes.len() {
                    while token < bytes.len() && bytes[token].is_ascii_whitespace() {
                        token += 1;
                    }
                    let start = token;
                    while token < bytes.len() && !bytes[token].is_ascii_whitespace() {
                        token += 1;
                    }
                    if token > start {
                        let value = literal[start..token].to_owned();
                        result.push(candidate(
                            project,
                            Kind::ClassToken,
                            file,
                            line,
                            value,
                            vec![Span::new(
                                line_start + literal_start + start,
                                line_start + literal_start + token,
                            )],
                            format!("{line}:{at}"),
                        )?);
                    }
                }
            }
            cursor = literal_end + 1;
        }
    }
    Ok(result)
}

fn identifier(text: &str, base: usize) -> Option<(String, Span)> {
    let leading = text.len() - text.trim_start().len();
    let text = &text[leading..];
    let end = text
        .char_indices()
        .find(|(_, character)| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
        })
        .map_or(text.len(), |(at, _)| at);
    (end > 0).then(|| {
        (
            text[..end].to_owned(),
            Span::new(base + leading, base + leading + end),
        )
    })
}

fn mermaid_line_identifiers(text: &str, base: usize) -> Vec<(String, Span)> {
    for operator in ["-->>", "-.->", "-->", "==>", "->>", "---", "-x", "-)"] {
        if let Some(at) = text.find(operator) {
            let mut found = Vec::new();
            if let Some(left) = identifier(&text[..at], base) {
                found.push(left);
            }
            if let Some(right) =
                identifier(&text[at + operator.len()..], base + at + operator.len())
            {
                found.push(right);
            }
            return found;
        }
    }
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    for prefix in ["participant ", "actor "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return identifier(rest, base + leading + prefix.len())
                .into_iter()
                .collect();
        }
    }
    let Some(found) = identifier(text, base) else {
        return Vec::new();
    };
    let tail = &text[found.1.end - base..];
    if tail.trim_start().starts_with(['[', '(', '{', '>']) {
        vec![found]
    } else {
        Vec::new()
    }
}

fn markdown_candidates(project: &Project<'_>, file: &Path, source: &str) -> Result<Vec<Candidate>> {
    let source_lines = lines(source).collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut at = 0usize;
    while at < source_lines.len() {
        let (line, start, text) = source_lines[at];
        let trimmed = text.trim_start();
        let level = trimmed.chars().take_while(|value| *value == '#').count();
        if (1..=6).contains(&level)
            && trimmed
                .as_bytes()
                .get(level)
                .is_some_and(u8::is_ascii_whitespace)
        {
            let title = trimmed[level..].trim();
            if !title.is_empty() {
                let title_at = text.find(title).unwrap();
                result.push(candidate(
                    project,
                    Kind::MarkdownHeading,
                    file,
                    line,
                    title.to_owned(),
                    vec![Span::new(start + title_at, start + title_at + title.len())],
                    line.to_string(),
                )?);
            }
        }
        let fence = trimmed
            .chars()
            .next()
            .filter(|value| matches!(value, '`' | '~'));
        let width = fence.map_or(0, |marker| {
            trimmed.chars().take_while(|value| *value == marker).count()
        });
        let mermaid = width >= 3 && trimmed[width..].trim().eq_ignore_ascii_case("mermaid");
        if !mermaid {
            at += 1;
            continue;
        }
        let marker = fence.unwrap();
        let opening = line;
        at += 1;
        let mut occurrences: BTreeMap<String, Vec<(usize, Span)>> = BTreeMap::new();
        while at < source_lines.len() {
            let (body_line, body_start, body) = source_lines[at];
            let body_trimmed = body.trim_start();
            let closing = body_trimmed
                .chars()
                .take_while(|value| *value == marker)
                .count();
            if closing >= width && body_trimmed[closing..].trim().is_empty() {
                break;
            }
            for (name, span) in mermaid_line_identifiers(body, body_start) {
                occurrences.entry(name).or_default().push((body_line, span));
            }
            at += 1;
        }
        for (name, located) in occurrences {
            let first_line = located[0].0;
            let spans = located.into_iter().map(|(_, span)| span).collect();
            result.push(candidate(
                project,
                Kind::MermaidNode,
                file,
                first_line,
                name,
                spans,
                opening.to_string(),
            )?);
        }
        at += 1;
    }
    Ok(result)
}

pub(super) fn candidates(project: &Project<'_>) -> Result<Vec<Candidate>> {
    let mut result = Vec::new();
    for (file, info) in project.index.files() {
        let source = &project.sources[file];
        match info.language {
            crate::lang::Language::Css => result.extend(css_candidates(project, file, source)?),
            crate::lang::Language::Html | crate::lang::Language::Tsx => {
                result.extend(class_attribute_candidates(project, file, source)?)
            }
            crate::lang::Language::Markdown => {
                result.extend(markdown_candidates(project, file, source)?)
            }
            _ => {}
        }
    }
    result.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(result)
}

pub(super) fn capability(
    candidates: &[Candidate],
    kind: Kind,
    path: &Path,
    line: usize,
    value: &str,
) -> Option<Value> {
    let matches = candidates
        .iter()
        .filter(|candidate| {
            candidate.kind == kind
                && candidate.path == path
                && candidate.line == line
                && candidate.value == value
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| {
        let candidate = matches[0];
        json!({
            "schema": "fr-surface-edit-1",
            "id": candidate.id,
            "operation": candidate.kind.operation(),
            "occurrences": candidate.spans.len(),
            "preview": {"command": "fr", "arguments": ["author", "edit-surface", candidate.id, "--to", "<VALUE>"]},
        })
    })
}

pub fn surface_value_size_allowed(bytes: usize) -> bool {
    (1..=256).contains(&bytes)
}

fn simple_identifier(value: &str, dot: bool) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|value| value.is_ascii_alphabetic() || matches!(value, '_' | '-'))
        && characters.all(|value| {
            value.is_ascii_alphanumeric() || matches!(value, '_' | '-') || dot && value == '.'
        })
}

pub(super) fn value_valid(kind: Kind, to: &str) -> bool {
    surface_value_size_allowed(to.len())
        && match kind {
            Kind::CssClass => simple_identifier(to, false),
            Kind::MermaidNode => simple_identifier(to, true),
            Kind::ClassToken => {
                to.is_ascii()
                    && !to.bytes().any(|byte| {
                        byte.is_ascii_whitespace()
                            || matches!(byte, b'\'' | b'"' | b'`' | b'\\' | b'$' | b'{' | b'}')
                    })
            }
            Kind::MarkdownHeading => to.trim() == to && !to.chars().any(char::is_control),
        }
}

pub(super) fn collision_free(candidate: &Candidate, to: &str, all: &[Candidate]) -> bool {
    candidate.kind != Kind::MermaidNode
        || !all.iter().any(|other| {
            other.kind == Kind::MermaidNode
                && other.file == candidate.file
                && other.scope == candidate.scope
                && other.value == to
                && other.id != candidate.id
        })
}
