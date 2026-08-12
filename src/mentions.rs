//! Where a name appears in prose: comments, strings and template text.
//!
//! Nothing resolves these. A comment that names a function is not a call, and no
//! grammar links the two. They still matter to a reader, so every command that answers
//! "where does this name appear" reports them, and no command edits them.
//!
//! `fr rename` and `fr delete` warn about them. `fr usages` lists them apart from the
//! references it resolved. The scan lived twice, once in each refactoring, and `fr
//! usages` had no copy at all: it answered "4 uses" for a name that appeared six times.

use crate::index::Index;
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::path::PathBuf;

/// One appearance of a name inside a comment or a string.
#[derive(Debug, Clone)]
pub struct Mention {
    pub file: PathBuf,
    pub span: Span,
    pub line: usize,
    pub col: usize,
}

/// Every appearance of `name` in the comments and strings of the workspace.
///
/// The match is on whole words, so `helper` does not match `helperful`.
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

/// The spans a grammar calls a string or a comment, plus the spans masking replaced.
pub fn string_and_comment_spans(parsed: &Parsed) -> Vec<Span> {
    let mut spans: Vec<Span> = parsed.masked_spans.clone();
    let mut cursor = parsed.root().walk();
    let mut recurse = true;

    loop {
        let node = cursor.node();
        let kind = node.kind();
        // Grammars name these differently: string_literal, raw_string_literal,
        // interpreted_string_literal, line_comment, block_comment, comment.
        if kind.contains("string") || kind.contains("comment") || kind.contains("char_literal") {
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
