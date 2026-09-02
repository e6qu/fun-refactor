//! Where a name appears in prose: comments, strings and template text.

use crate::index::Index;
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::path::PathBuf;

/// One appearance of a name inside literal data or a comment.
#[derive(Debug, Clone)]
pub struct Mention {
    pub file: PathBuf,
    pub span: Span,
    pub line: usize,
    pub col: usize,
}

/// Every appearance of `name` in the comments and literal data of the workspace.
pub fn of(index: &Index, name: &str) -> Result<Vec<Mention>> {
    let parsers = Parsers::new();
    let mut found = Vec::new();

    for (path, info) in index.files() {
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        if !source.contains(name) {
            continue;
        }
        let parsed = parsers.parse(info.language, &source)?;
        let line_index = LineIndex::new(&source);

        for span in string_and_comment_spans(&parsed) {
            let text = span.text(&source);
            for (offset, _) in text.match_indices(name) {
                if !is_word_boundary(text, offset, name.len()) {
                    continue;
                }
                let absolute = Span::new(span.start + offset, span.start + offset + name.len());
                let pos = line_index.line_col(absolute.start, &source);
                found.push(Mention {
                    file: path.clone(),
                    span: absolute,
                    line: pos.line,
                    col: pos.col,
                });
            }
        }
    }
    Ok(found)
}

/// The spans a grammar calls literal data or a comment, plus the spans masking replaced.
pub fn string_and_comment_spans(parsed: &Parsed) -> Vec<Span> {
    let mut spans: Vec<Span> = parsed.masked_spans.clone();
    let mut cursor = parsed.root().walk();
    let mut recurse = true;

    loop {
        let node = cursor.node();
        let kind = node.kind();
        // Grammars name these differently: string_literal, raw_string_literal, interpreted_string_literal,
        // char_literal, rune_literal, regex, line_comment, block_comment, comment.
        let prose = parsed.language == crate::lang::Language::Markdown
            && matches!(kind, "inline" | "code_fence_content");
        if prose
            || kind.contains("string")
            || kind.contains("comment")
            || kind.contains("char")
            || kind.contains("rune")
            || kind.contains("regex")
        {
            spans.push(Span::from(node));
            recurse = false;
        }
        if recurse && cursor.goto_first_child() {
            continue;
        }
        recurse = true;
        if cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                spans.sort();
                spans.dedup();
                return spans;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Is the match at `offset` a whole word?
pub fn is_word_boundary(haystack: &str, offset: usize, len: usize) -> bool {
    let before = haystack[..offset].chars().next_back();
    let after = haystack[offset + len..].chars().next();
    let part_of_a_word = |c: char| c.is_alphanumeric() || c == '_';
    !before.is_some_and(part_of_a_word) && !after.is_some_and(part_of_a_word)
}
