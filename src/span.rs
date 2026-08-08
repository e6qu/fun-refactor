//! Byte-native source positions.
//!
//! Every position in fun-refactor is a byte offset into the original file. Line/column
//! are derived only for display. This is what makes edits lossless: an edit is
//! "replace bytes [start, end) with this text", touching nothing else in the file.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A half-open byte range `[start, end)` into a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start {start} must not exceed end {end}");
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Does this span fully contain `other`?
    pub fn contains(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Does this span contain the given byte offset?
    pub fn contains_offset(&self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Do the two spans share any byte? Adjacent (touching) spans do not overlap.
    pub fn overlaps(&self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Slice the source text this span refers to.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

impl From<tree_sitter::Node<'_>> for Span {
    fn from(node: tree_sitter::Node<'_>) -> Self {
        Span::new(node.start_byte(), node.end_byte())
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// A 1-based line/column position, derived from a byte offset for display only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LineCol {
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Maps byte offsets to line/column positions for one source file.
///
/// Built once per file; lookups are binary searches over line start offsets.
///
/// A trailing newline terminates the final line rather than starting a new empty
/// one, so `"a\nb\n"` has two lines — matching how editors and diffs count them.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the start of each line. Always starts with 0.
    line_starts: Vec<usize>,
    len: usize,
    ends_with_newline: bool,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        let ends_with_newline = source.ends_with('\n');
        if ends_with_newline {
            // The offset just past the final newline is the end of the last line,
            // not the start of another one.
            line_starts.pop();
        }
        Self {
            line_starts,
            len: source.len(),
            ends_with_newline,
        }
    }

    /// Byte offset one past the end of a 0-based line, excluding its newline.
    fn line_end(&self, line: usize) -> usize {
        match self.line_starts.get(line + 1) {
            Some(next) => next - 1,
            None if self.ends_with_newline => self.len - 1,
            None => self.len,
        }
    }

    /// Convert a byte offset to a 1-based line/column.
    ///
    /// Column counts characters, not bytes, so multi-byte characters advance the
    /// column by one — matching what editors display.
    pub fn line_col(&self, offset: usize, source: &str) -> LineCol {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        let line_start = self.line_starts[line];
        // Clamp to the line's end so a trailing newline does not report a column
        // past the last visible character.
        let mut col_end = offset.min(self.line_end(line));
        // And to a character boundary. Callers arrive here with offsets they computed —
        // `span.end - 1` to name the last covered byte, for one — and one of those lands
        // inside a multi-byte character whenever the region ends with one. Slicing there
        // panicked: `fr duplicates --language python` over `psf/black` died on a '𨉟'.
        while col_end > line_start && !source.is_char_boundary(col_end) {
            col_end -= 1;
        }
        let col = source[line_start..col_end].chars().count() + 1;
        LineCol {
            line: line + 1,
            col,
        }
    }

    /// 0-based line index containing `offset`.
    fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next - 1,
        }
    }

    /// Byte offset of a 1-based line/column position.
    ///
    /// Returns `None` if the line does not exist. A column beyond the end of the
    /// line clamps to the line's end, so callers can address end-of-line positions.
    pub fn offset(&self, pos: LineCol, source: &str) -> Option<usize> {
        if pos.line == 0 || pos.line > self.line_starts.len() {
            return None;
        }
        let line_start = self.line_starts[pos.line - 1];
        let line_end = self.line_end(pos.line - 1);
        let line_text = &source[line_start..line_end];
        let col_offset = line_text
            .char_indices()
            .nth(pos.col.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(line_text.len());
        Some(line_start + col_offset)
    }

    /// Byte span covering a whole 1-based line, excluding its newline.
    pub fn line_span(&self, line: usize) -> Option<Span> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        Some(Span::new(
            self.line_starts[line - 1],
            self.line_end(line - 1),
        ))
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// Parse `path:line:col-line:col` into a path and two positions.
///
/// Here rather than in the CLI because a recipe's `extract … at "…"` writes the same
/// spec, and two parsers for one syntax is two chances to disagree about it.
pub fn parse_range(spec: &str) -> anyhow::Result<(std::path::PathBuf, LineCol, LineCol)> {
    let shape = || anyhow::anyhow!("expected path:line:col-line:col, got '{spec}'");
    let (head, end_col) = spec.rsplit_once(':').ok_or_else(shape)?;
    let (head, end_line) = head.rsplit_once('-').ok_or_else(shape)?;
    let (path, start_col) = head.rsplit_once(':').ok_or_else(shape)?;
    let (path, start_line) = path.rsplit_once(':').ok_or_else(shape)?;

    Ok((
        std::path::PathBuf::from(path),
        LineCol {
            line: start_line.parse()?,
            col: start_col.parse()?,
        },
        LineCol {
            line: end_line.parse()?,
            col: end_col.parse()?,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_containment_and_overlap() {
        let outer = Span::new(0, 10);
        let inner = Span::new(2, 5);
        assert!(outer.contains(inner));
        assert!(!inner.contains(outer));
        assert!(outer.overlaps(inner));

        // Adjacent spans touch but do not overlap — this is what lets us apply
        // back-to-back edits without a conflict error.
        let a = Span::new(0, 5);
        let b = Span::new(5, 10);
        assert!(!a.overlaps(b));
        assert!(!b.overlaps(a));
    }

    #[test]
    fn line_col_roundtrip() {
        let source = "fn main() {\n    let x = 1;\n}\n";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 3);

        let offset = source.find("let").unwrap();
        let pos = index.line_col(offset, source);
        assert_eq!(pos, LineCol { line: 2, col: 5 });
        assert_eq!(index.offset(pos, source), Some(offset));
    }

    #[test]
    fn line_col_survives_an_offset_inside_a_character() {
        // Callers compute offsets: `span.end - 1` names the last byte a region covers,
        // and that is inside the character whenever the region ends with a multi-byte
        // one. Slicing there panicked — `fr duplicates --language python` over
        // `psf/black` died on a '𨉟', four bytes wide — and `full_line_span` reaches
        // here the same way, so `fr delete` and `fr imports` could do the same.
        let source = "x = \"𨉟\"\n";
        let index = LineIndex::new(source);
        let end = source.find('\n').expect("a newline");
        for offset in (end - 4)..=end {
            let pos = index.line_col(offset, source);
            assert_eq!(pos.line, 1, "offset {offset} is on line 1");
        }
    }

    #[test]
    fn line_col_counts_characters_not_bytes() {
        let source = "let名 = \"héllo\";\n";
        let index = LineIndex::new(source);
        let offset = source.find('=').unwrap();
        let pos = index.line_col(offset, source);
        // "let名 " is 5 characters, so '=' sits at column 6 despite the multi-byte char.
        assert_eq!(pos, LineCol { line: 1, col: 6 });
        assert_eq!(index.offset(pos, source), Some(offset));
    }

    #[test]
    fn offset_past_end_of_line_clamps() {
        let source = "ab\ncd\n";
        let index = LineIndex::new(source);
        let pos = LineCol { line: 1, col: 99 };
        assert_eq!(index.offset(pos, source), Some(2));
        assert_eq!(index.offset(LineCol { line: 9, col: 1 }, source), None);
    }

    #[test]
    fn line_span_excludes_newline() {
        let source = "alpha\nbeta\n";
        let index = LineIndex::new(source);
        // A trailing newline ends the last line; it does not begin a third one.
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_span(1).unwrap().text(source), "alpha");
        assert_eq!(index.line_span(2).unwrap().text(source), "beta");
        assert_eq!(index.line_span(3), None);
    }

    #[test]
    fn end_of_file_offset_maps_to_end_of_last_line() {
        let source = "alpha\nbeta\n";
        let index = LineIndex::new(source);
        // Offset at EOF sits just past "beta", i.e. line 2 column 5 — not on a
        // phantom line 3, and not past the newline.
        assert_eq!(
            index.line_col(source.len(), source),
            LineCol { line: 2, col: 5 }
        );
    }

    #[test]
    fn empty_source_has_one_line() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_span(1).unwrap(), Span::new(0, 0));
        assert_eq!(index.line_col(0, ""), LineCol { line: 1, col: 1 });
    }

    #[test]
    fn file_without_trailing_newline() {
        let source = "one\ntwo";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_span(2).unwrap().text(source), "two");
        let last = index.line_col(source.len(), source);
        assert_eq!(last, LineCol { line: 2, col: 4 });
    }
}
