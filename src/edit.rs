//! The lossless edit engine.
//!
//! Every refactoring produces [`Edit`]s: "replace these bytes with this text". Edits
//! are applied to the original source descending by offset, so earlier offsets stay
//! valid while later ones are rewritten. Nothing is pretty-printed, so formatting,
//! comments and trailing whitespace outside the edited ranges survive byte-for-byte.
//!
//! Before anything is written, the result is reparsed: an edit that introduces new
//! syntax errors is rejected rather than saved.

use crate::lang::Language;
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A single byte-range replacement within one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
    /// Short description of why this edit exists, surfaced in diffs and JSON output.
    pub reason: String,
}

impl Edit {
    pub fn new(span: Span, replacement: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
            reason: reason.into(),
        }
    }
}

/// All edits to be applied to a single file.
#[derive(Debug, Clone, Default)]
pub struct FileEdits {
    pub edits: Vec<Edit>,
}

/// A set of edits spanning any number of files — the unit a refactoring produces.
#[derive(Debug, Clone, Default)]
pub struct EditSet {
    files: BTreeMap<PathBuf, FileEdits>,
}

impl EditSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, path: impl Into<PathBuf>, edit: Edit) {
        self.files.entry(path.into()).or_default().edits.push(edit);
    }

    pub fn is_empty(&self) -> bool {
        self.files.values().all(|f| f.edits.is_empty())
    }

    pub fn file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|(_, f)| !f.edits.is_empty())
            .count()
    }

    pub fn edit_count(&self) -> usize {
        self.files.values().map(|f| f.edits.len()).sum()
    }

    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.keys()
    }

    pub fn edits_for(&self, path: &Path) -> Option<&[Edit]> {
        self.files.get(path).map(|f| f.edits.as_slice())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &[Edit])> {
        self.files.iter().map(|(p, f)| (p, f.edits.as_slice()))
    }
}

/// Apply `edits` to `source`, returning the rewritten text.
///
/// Fails if any two edits overlap: overlapping edits have no well-defined result, and
/// guessing one would silently corrupt the file.
pub fn apply_to_string(source: &str, edits: &[Edit]) -> Result<String> {
    if edits.is_empty() {
        return Ok(source.to_string());
    }

    let mut ordered: Vec<&Edit> = edits.iter().collect();
    // Sort ascending first to detect overlaps against immediate neighbours only.
    ordered.sort_by_key(|e| (e.span.start, e.span.end));

    for pair in ordered.windows(2) {
        if pair[0].span.overlaps(pair[1].span) {
            bail!(
                "conflicting edits: {} ({}) overlaps {} ({})",
                pair[0].span,
                pair[0].reason,
                pair[1].span,
                pair[1].reason
            );
        }
    }

    if let Some(last) = ordered.last() {
        if last.span.end > source.len() {
            bail!(
                "edit at {} extends past end of file ({} bytes)",
                last.span,
                source.len()
            );
        }
    }

    for edit in &ordered {
        if !source.is_char_boundary(edit.span.start) || !source.is_char_boundary(edit.span.end) {
            bail!(
                "edit at {} does not fall on character boundaries",
                edit.span
            );
        }
    }

    // Apply descending so each splice leaves all lower offsets untouched.
    let mut out = source.to_string();
    for edit in ordered.iter().rev() {
        out.replace_range(edit.span.start..edit.span.end, &edit.replacement);
    }
    Ok(out)
}

/// The outcome of applying edits to one file.
#[derive(Debug, Clone)]
pub struct FileOutcome {
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
    pub language: Language,
}

impl FileOutcome {
    pub fn changed(&self) -> bool {
        self.original != self.updated
    }

    /// A unified diff of this file's change.
    pub fn unified_diff(&self) -> String {
        let display = self.path.display().to_string();
        crate::edit::unified_diff(&self.original, &self.updated, &display)
    }
}

/// Validation performed on the rewritten text before it may be written to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    /// Reparse and reject the edit if it introduces syntax errors the original
    /// did not have. This is the default and should stay that way.
    ReparseStrict,
    /// Skip reparse validation. Only for callers that have already validated.
    None,
}

/// Compute the result of applying an [`EditSet`], validating each file.
///
/// Nothing is written; the caller decides whether to render a diff or commit.
pub fn plan(edit_set: &EditSet, validation: Validation) -> Result<Vec<FileOutcome>> {
    let parsers = Parsers::new();
    let mut outcomes = Vec::new();

    for (path, edits) in edit_set.iter() {
        if edits.is_empty() {
            continue;
        }
        // A refactoring may create a file — moving a symbol to a new module, or
        // writing a Helm `_helpers.tpl` that does not exist yet — so a missing
        // destination is an empty one, not a failure.
        let original = match crate::vfs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(anyhow::Error::from(e))
                    .with_context(|| format!("reading {}", path.display()))
            }
        };
        let updated = apply_to_string(&original, edits)
            .with_context(|| format!("applying edits to {}", path.display()))?;

        let language = crate::lang::detect(path)
            .with_context(|| format!("unsupported file type: {}", path.display()))?;

        if validation == Validation::ReparseStrict {
            let before = parsers.parse(language, &original)?;
            let after = parsers.parse(language, &updated)?;

            // Check the tree-wide error flag as well as the span count. Some
            // grammars flag a broken subtree without emitting an ERROR node, so
            // counting spans alone can miss a file we just broke.
            if after.has_errors() && !before.has_errors() {
                bail!(
                    "edit rejected: {} parses cleanly now but would not after the \
                     change. The file was left unchanged.",
                    path.display()
                );
            }
            let before_errors = before.error_spans().len();
            let after_errors = after.error_spans().len();
            if after_errors > before_errors {
                bail!(
                    "edit rejected: {} would gain {} new syntax error(s). \
                     The file was left unchanged.",
                    path.display(),
                    after_errors - before_errors
                );
            }
        }

        outcomes.push(FileOutcome {
            path: path.clone(),
            original,
            updated,
            language,
        });
    }

    Ok(outcomes)
}

/// Write planned outcomes to disk atomically across all files.
///
/// Every file is staged as a temporary file next to its target and only then renamed
/// into place, so a mid-run failure cannot leave a half-applied refactoring.
pub fn commit(outcomes: &[FileOutcome]) -> Result<usize> {
    // Without a filesystem there is nothing to stage against: a write goes into the
    // same in-memory workspace every read comes from, and a partial write cannot
    // survive a failure because there is no second copy to be inconsistent with.
    #[cfg(not(feature = "cli"))]
    {
        let mut written = 0;
        for outcome in outcomes.iter().filter(|o| o.changed()) {
            crate::vfs::write(&outcome.path, &outcome.updated)?;
            written += 1;
        }
        return Ok(written);
    }

    #[cfg(feature = "cli")]
    {
        commit_via_staging(outcomes)
    }
}

/// Stage each file beside its target and rename it into place, so a mid-run failure
/// cannot leave a half-applied refactoring.
#[cfg(feature = "cli")]
fn commit_via_staging(outcomes: &[FileOutcome]) -> Result<usize> {
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();

    let result = (|| -> Result<()> {
        for outcome in outcomes.iter().filter(|o| o.changed()) {
            let dir = outcome.path.parent().unwrap_or_else(|| Path::new("."));
            let mut tmp = tempfile::NamedTempFile::new_in(dir)
                .with_context(|| format!("staging {}", outcome.path.display()))?;
            use std::io::Write;
            tmp.write_all(outcome.updated.as_bytes())
                .with_context(|| format!("writing staged {}", outcome.path.display()))?;
            tmp.flush()?;
            let (_, tmp_path) = tmp.keep().context("retaining staged file")?;
            staged.push((tmp_path, outcome.path.clone()));
        }
        Ok(())
    })();

    if let Err(e) = result {
        for (tmp_path, _) in &staged {
            let _ = std::fs::remove_file(tmp_path);
        }
        return Err(e);
    }

    let count = staged.len();
    for (tmp_path, target) in staged {
        std::fs::rename(&tmp_path, &target)
            .with_context(|| format!("committing {}", target.display()))?;
    }
    Ok(count)
}

/// Render a unified diff between two texts.
pub fn unified_diff(before: &str, after: &str, path: &str) -> String {
    use similar::{ChangeTag, TextDiff};

    if before == after {
        return String::new();
    }

    let diff = TextDiff::from_lines(before, after);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");

    for group in diff.grouped_ops(3) {
        let (first, last) = (group.first(), group.last());
        if let (Some(first), Some(last)) = (first, last) {
            let old_start = first.old_range().start + 1;
            let old_len = last.old_range().end - first.old_range().start;
            let new_start = first.new_range().start + 1;
            let new_len = last.new_range().end - first.new_range().start;
            out.push_str(&format!(
                "@@ -{old_start},{old_len} +{new_start},{new_len} @@\n"
            ));
        }
        for op in group {
            for change in diff.iter_changes(&op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                out.push(sign);
                out.push_str(change.value());
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// Byte span of the whole line containing `offset`, including its trailing newline.
/// Used by refactorings that insert or remove whole statements.
pub fn full_line_span(source: &str, offset: usize) -> Span {
    let index = LineIndex::new(source);
    let pos = index.line_col(offset, source);
    let line = index
        .line_span(pos.line)
        .unwrap_or(Span::new(offset, offset));
    let end = if line.end < source.len() && source.as_bytes()[line.end] == b'\n' {
        line.end + 1
    } else {
        line.end
    };
    Span::new(line.start, end)
}

/// The indentation (leading horizontal whitespace) of the line containing `offset`.
pub fn line_indent(source: &str, offset: usize) -> String {
    let index = LineIndex::new(source);
    let pos = index.line_col(offset, source);
    let Some(line) = index.line_span(pos.line) else {
        return String::new();
    };
    line.text(source)
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// One level of indentation, as this file writes it.
///
/// Read from the source rather than assumed: generated code that arrives four spaces
/// deep in a two-space TypeScript file or a tab-indented Go file is a visible wart on
/// every line it touches. The shortest indentation any line carries is one level —
/// every real file has at least one line indented exactly once.
pub fn indent_unit(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect::<String>()
        })
        .filter(|lead| !lead.is_empty())
        .min_by_key(String::len)
        .unwrap_or_else(|| "    ".to_string())
}

/// Re-indent a multi-line block so its subsequent lines carry `indent`.
///
/// The first line is left alone: it is spliced at a position that already has the
/// caller's indentation.
pub fn reindent(text: &str, indent: &str) -> String {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = String::from(first);
    for line in lines {
        out.push('\n');
        if !line.trim().is_empty() {
            out.push_str(indent);
        }
        out.push_str(line);
    }
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_edits_descending_without_shifting_offsets() {
        let src = "let alpha = 1; let beta = 2;";
        let edits = vec![
            Edit::new(Span::new(4, 9), "first", "rename alpha"),
            Edit::new(Span::new(19, 23), "second", "rename beta"),
        ];
        let out = apply_to_string(src, &edits).unwrap();
        assert_eq!(out, "let first = 1; let second = 2;");
    }

    #[test]
    fn preserves_everything_outside_edited_ranges() {
        // Comments, odd spacing and trailing whitespace must survive untouched.
        let src = "fn  a() {} // keep   this\n\n\n/* and this */   \n";
        let edits = vec![Edit::new(Span::new(4, 5), "b", "rename")];
        let out = apply_to_string(src, &edits).unwrap();
        assert_eq!(out, "fn  b() {} // keep   this\n\n\n/* and this */   \n");
    }

    #[test]
    fn rejects_overlapping_edits() {
        let src = "abcdef";
        let edits = vec![
            Edit::new(Span::new(0, 3), "X", "one"),
            Edit::new(Span::new(2, 5), "Y", "two"),
        ];
        let err = apply_to_string(src, &edits).unwrap_err().to_string();
        assert!(err.contains("conflicting edits"), "unexpected error: {err}");
    }

    #[test]
    fn adjacent_edits_are_allowed() {
        let src = "abcdef";
        let edits = vec![
            Edit::new(Span::new(0, 3), "X", "one"),
            Edit::new(Span::new(3, 6), "Y", "two"),
        ];
        assert_eq!(apply_to_string(src, &edits).unwrap(), "XY");
    }

    #[test]
    fn rejects_edits_past_end_of_file() {
        let src = "short";
        let edits = vec![Edit::new(Span::new(3, 99), "x", "bad")];
        let err = apply_to_string(src, &edits).unwrap_err().to_string();
        assert!(err.contains("past end of file"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_edits_splitting_multibyte_characters() {
        let src = "héllo";
        // Byte 2 is inside the two-byte 'é'.
        let edits = vec![Edit::new(Span::new(1, 2), "x", "bad")];
        let err = apply_to_string(src, &edits).unwrap_err().to_string();
        assert!(
            err.contains("character boundaries"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn insertion_and_deletion_are_just_spans() {
        let src = "ac";
        let inserted = apply_to_string(src, &[Edit::new(Span::new(1, 1), "b", "insert")]).unwrap();
        assert_eq!(inserted, "abc");
        let deleted = apply_to_string("abc", &[Edit::new(Span::new(1, 2), "", "delete")]).unwrap();
        assert_eq!(deleted, "ac");
    }

    #[test]
    fn empty_edit_list_is_identity() {
        assert_eq!(apply_to_string("unchanged", &[]).unwrap(), "unchanged");
    }

    #[test]
    fn edit_set_tracks_files_and_counts() {
        let mut set = EditSet::new();
        assert!(set.is_empty());
        set.add("a.rs", Edit::new(Span::new(0, 1), "x", "r"));
        set.add("a.rs", Edit::new(Span::new(2, 3), "y", "r"));
        set.add("b.rs", Edit::new(Span::new(0, 1), "z", "r"));
        assert!(!set.is_empty());
        assert_eq!(set.file_count(), 2);
        assert_eq!(set.edit_count(), 3);
        assert_eq!(set.edits_for(Path::new("a.rs")).unwrap().len(), 2);
    }

    #[test]
    fn plan_rejects_edits_that_break_syntax() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("broken.rs");
        crate::vfs::write(&path, "fn main() {}\n").unwrap();

        let mut set = EditSet::new();
        // Delete the closing brace: the file would no longer parse.
        set.add(&path, Edit::new(Span::new(11, 12), "", "break it"));

        let err = plan(&set, Validation::ReparseStrict)
            .unwrap_err()
            .to_string();
        assert!(err.contains("edit rejected"), "unexpected error: {err}");
        // The file must be untouched.
        assert_eq!(crate::vfs::read_to_string(&path).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn plan_rejects_breakage_a_grammar_flags_without_an_error_node() {
        // Some grammars mark a subtree erroneous without emitting an ERROR node
        // (tree-sitter-zig does this for an empty container body). Comparing error
        // span counts alone would see no change and accept the edit, so the
        // tree-wide flag is checked too.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("thing.zig");
        crate::vfs::write(&path, "const S = struct { x: i32 };\n").unwrap();

        let before = Parsers::new()
            .parse(Language::Zig, "const S = struct { x: i32 };\n")
            .unwrap();
        assert!(!before.has_errors(), "the starting file must parse cleanly");

        let mut set = EditSet::new();
        // Emptying the body produces the flagged-but-node-less error case.
        set.add(&path, Edit::new(Span::new(17, 27), "{}", "empty the body"));

        let err = plan(&set, Validation::ReparseStrict)
            .unwrap_err()
            .to_string();
        assert!(err.contains("edit rejected"), "unexpected error: {err}");
        assert_eq!(
            crate::vfs::read_to_string(&path).unwrap(),
            "const S = struct { x: i32 };\n"
        );
    }

    #[test]
    fn error_spans_are_reported_even_without_an_error_node() {
        let parsed = Parsers::new()
            .parse(Language::Zig, "const Z = struct {};\n")
            .unwrap();
        if parsed.has_errors() {
            assert!(
                !parsed.error_spans().is_empty(),
                "a tree flagged as erroneous must report at least one span, \
                 or the edit engine cannot detect the breakage"
            );
        }
    }

    #[test]
    fn plan_allows_edits_to_already_broken_files() {
        // A file that already has errors should still be editable; we only reject
        // edits that make things worse.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("already.rs");
        crate::vfs::write(&path, "fn main( {}\n").unwrap();

        let mut set = EditSet::new();
        set.add(&path, Edit::new(Span::new(3, 7), " good", "tweak"));
        let outcomes = plan(&set, Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn a_refactoring_may_create_a_file_that_did_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let fresh = tmp.path().join("new_module.rs");

        let mut set = EditSet::new();
        set.add(
            &fresh,
            Edit::new(Span::new(0, 0), "fn moved() {}\n", "create"),
        );

        let outcomes = plan(&set, Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(commit(&outcomes).unwrap(), 1);
        assert_eq!(
            crate::vfs::read_to_string(&fresh).unwrap(),
            "fn moved() {}\n"
        );
    }

    #[test]
    fn commit_writes_all_files_and_reports_count() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.rs");
        let b = tmp.path().join("b.rs");
        crate::vfs::write(&a, "fn a() {}\n").unwrap();
        crate::vfs::write(&b, "fn b() {}\n").unwrap();

        let mut set = EditSet::new();
        set.add(&a, Edit::new(Span::new(3, 4), "x", "rename"));
        set.add(&b, Edit::new(Span::new(3, 4), "y", "rename"));

        let outcomes = plan(&set, Validation::ReparseStrict).unwrap();
        assert_eq!(commit(&outcomes).unwrap(), 2);
        assert_eq!(crate::vfs::read_to_string(&a).unwrap(), "fn x() {}\n");
        assert_eq!(crate::vfs::read_to_string(&b).unwrap(), "fn y() {}\n");
    }

    #[test]
    fn unified_diff_renders_changes() {
        let diff = unified_diff("a\nb\nc\n", "a\nB\nc\n", "f.rs");
        assert!(diff.contains("--- a/f.rs"));
        assert!(diff.contains("+++ b/f.rs"));
        assert!(diff.contains("-b"));
        assert!(diff.contains("+B"));
    }

    #[test]
    fn unified_diff_is_empty_when_unchanged() {
        assert_eq!(unified_diff("same\n", "same\n", "f.rs"), "");
    }

    #[test]
    fn line_helpers() {
        let src = "fn a() {\n    let x = 1;\n}\n";
        let offset = src.find("let").unwrap();
        assert_eq!(line_indent(src, offset), "    ");
        assert_eq!(full_line_span(src, offset).text(src), "    let x = 1;\n");
    }

    #[test]
    fn reindent_indents_continuation_lines_only() {
        let block = "if x {\n    y();\n}";
        assert_eq!(reindent(block, "  "), "if x {\n      y();\n  }");
    }
}
