//! The lossless edit engine.
//!
//! Every refactoring produces [`Edit`]s: "replace these bytes with this text". Edits
//! are applied to the original source descending by offset, so earlier offsets stay
//! valid while later ones are rewritten. Nothing is pretty-printed, so formatting,
//! comments and trailing whitespace outside the edited ranges survive byte-for-byte.
//!
//! Before anything is written, the result is reparsed: an edit that introduces new
//! syntax errors is rejected and not saved.

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

/// A set of edits spanning any number of files, the unit a refactoring produces.
#[derive(Debug, Clone, Default)]
pub struct EditSet {
    files: BTreeMap<PathBuf, FileEdits>,
    /// What language a file is, where the producer knows better than the name.
    ///
    /// A translation writing `--out build/api.gen` knows the text is Python; the
    /// path says nothing. Detection from the name is the fallback, this is the
    /// authority.
    languages: BTreeMap<PathBuf, crate::lang::Language>,
}

impl EditSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, path: impl Into<PathBuf>, edit: Edit) {
        self.files.entry(path.into()).or_default().edits.push(edit);
    }

    /// Take every edit in `other` into this set.
    pub fn extend(&mut self, other: EditSet) {
        for (path, file) in other.files {
            for edit in file.edits {
                self.add(path.clone(), edit);
            }
        }
        self.languages.extend(other.languages);
    }

    /// Declare what language `path` holds, overriding detection by name.
    pub fn declare_language(&mut self, path: impl Into<PathBuf>, language: crate::lang::Language) {
        self.languages.insert(path.into(), language);
    }

    /// The declared language of `path`, where one was declared.
    pub fn language(&self, path: &Path) -> Option<crate::lang::Language> {
        self.languages.get(path).copied()
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
        // A refactoring may create a file, moving a symbol to a new module, or writing a Helm
        // `_helpers.tpl` that does not exist yet. So a missing destination is an empty one, not
        // a failure.
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

        let language = edit_set
            .language(path)
            .or_else(|| crate::lang::detect(path))
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
                     change. The file was left unchanged.{}",
                    path.display(),
                    rejection_evidence(&after, &updated)
                );
            }
            let before_errors = before.error_spans().len();
            let after_errors = after.error_spans().len();
            if after_errors > before_errors {
                bail!(
                    "edit rejected: {} would gain {} new syntax error(s). \
                     The file was left unchanged.{}",
                    path.display(),
                    after_errors - before_errors,
                    rejection_evidence(&after, &updated)
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

/// Where the rejected text stops parsing, with the lines around it.
///
/// A refusal naming no position sends whoever caused it back to guessing. The
/// rejected text never reaches disk, so this is the only moment to see it.
fn rejection_evidence(after: &crate::parse::Parsed, updated: &str) -> String {
    let Some(at) = after.error_spans().iter().map(|span| span.start).min() else {
        return String::new();
    };
    let index = crate::span::LineIndex::new(updated);
    let at = index.line_col(at, updated);
    let window: Vec<String> = updated
        .lines()
        .enumerate()
        .filter(|(i, _)| *i + 1 >= at.line.saturating_sub(3) && *i < at.line + 2)
        .map(|(i, text)| format!("  {:>4} {text}", i + 1))
        .collect();
    format!(
        "\n{}\n  That result stops parsing at line {}, column {}.",
        window.join("\n"),
        at.line,
        at.col
    )
}

/// Write planned outcomes to disk atomically across all files.
///
/// Every file is staged as a temporary file next to its target and only then renamed into
/// place. So a mid-run failure cannot leave a half-applied refactoring.
///
/// The plan captured each file's text when it was read, and the commit re-reads every
/// file and compares before writing anything. A file that changed in between fails the
/// whole set. Two `fr rename --write` runs racing on one workspace used to both report
/// success while the second silently overwrote the first's edits.
pub fn commit(outcomes: &[FileOutcome]) -> Result<usize> {
    // Without a filesystem there is nothing to stage against: a write goes into the same
    // in-memory workspace every read comes from. A partial write cannot survive a failure
    // because there is no second copy to be inconsistent with.
    //
    // Asked of the vfs and not of `cfg!(feature = "cli")`. Where the writes go is a fact about
    // the active backing. The feature only says which backings exist, and a build with both
    // would stage a temporary file beside a path that is in a browser's memory and has no
    // directory on disk.
    if crate::vfs::is_in_memory() {
        verify_basis_unchanged(outcomes)?;
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

    // A build with no filesystem support at all writes through the vfs, which is
    // what the branch above did.
    #[cfg(not(feature = "cli"))]
    {
        verify_basis_unchanged(outcomes)?;
        let mut written = 0;
        for outcome in outcomes.iter().filter(|o| o.changed()) {
            crate::vfs::write(&outcome.path, &outcome.updated)?;
            written += 1;
        }
        Ok(written)
    }
}

/// Refuse the whole commit when any file no longer holds the text the plan read.
///
/// The comparison is on the recorded text itself, which the plan already carries; a
/// separate hash would only re-derive it. A file the plan would create reads as empty,
/// matching how [`plan`] read it.
fn verify_basis_unchanged(outcomes: &[FileOutcome]) -> Result<()> {
    for outcome in outcomes.iter().filter(|o| o.changed()) {
        let current = match crate::vfs::read_to_string(&outcome.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(anyhow::Error::from(e))
                    .with_context(|| format!("re-reading {}", outcome.path.display()))
            }
        };
        if current != outcome.original {
            bail!(
                "{} changed since the plan was made; nothing was written. \
                 Re-run the command against the current text.",
                outcome.path.display()
            );
        }
    }
    Ok(())
}

/// Exclusive advisory locks over the commit window, one per directory written to.
///
/// Two processes committing into the same workspace at the same time could each pass
/// the re-read check and then overwrite each other. Any file both would write shares a
/// parent directory. So an OS lock per written directory serialises the whole
/// read-verify-write window. The locks are taken in sorted order, so two commits
/// cannot end up waiting on each other.
///
/// The lock files live in the system temporary directory, named by a hash of the
/// canonical directory path, never inside the workspace. A commit that added files
/// beside the sources it edits would show up in listings, backups and demo captures.
/// The canonical spelling makes two processes agree: `/var` and `/private/var` name
/// the same directory and must name the same lock.
///
/// The lock files themselves are left in place. Deleting one that another process is
/// blocked on would quietly hand out a second lock on a file nobody else can see.
/// Dropping the handles releases the locks, and the OS releases them if the process
/// dies, so there is no staleness to detect.
#[cfg(feature = "cli")]
struct CommitLocks {
    /// Held for the locks the open handles carry, never read.
    _held: Vec<std::fs::File>,
}

#[cfg(feature = "cli")]
impl CommitLocks {
    fn acquire(outcomes: &[FileOutcome]) -> Result<CommitLocks> {
        use sha2::{Digest, Sha256};

        let mut directories: Vec<PathBuf> = outcomes
            .iter()
            .filter(|o| o.changed())
            .map(|o| {
                o.path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            })
            .collect();
        directories.sort();
        directories.dedup();

        let lock_dir = std::env::temp_dir().join("fun-refactor-locks");
        std::fs::create_dir_all(&lock_dir)
            .with_context(|| format!("creating {}", lock_dir.display()))?;

        let mut held = Vec::new();
        for dir in directories {
            // The directory is created here as well. A plan may introduce a file in a
            // directory that does not exist yet, and its lock must name the real path.
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            let canonical = dir
                .canonicalize()
                .with_context(|| format!("resolving {}", dir.display()))?;
            let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
            let name: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
            let lock_path = lock_dir.join(format!("{name}.lock"));
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&lock_path)
                .with_context(|| format!("opening {}", lock_path.display()))?;
            file.lock()
                .with_context(|| format!("locking {}", lock_path.display()))?;
            held.push(file);
        }
        Ok(CommitLocks { _held: held })
    }
}

/// Stage each file beside its target and rename it into place, so a mid-run failure
/// cannot leave a half-applied refactoring.
#[cfg(feature = "cli")]
fn commit_via_staging(outcomes: &[FileOutcome]) -> Result<usize> {
    // The locks first, then the check, then the writes. Checking before locking would
    // let another commit slip its writes in between the two.
    let locks = CommitLocks::acquire(outcomes)?;
    verify_basis_unchanged(outcomes)?;

    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();

    let result = (|| -> Result<()> {
        for outcome in outcomes.iter().filter(|o| o.changed()) {
            let dir = outcome.path.parent().unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            let mut tmp = tempfile::NamedTempFile::new_in(dir)
                .with_context(|| format!("staging {}", outcome.path.display()))?;
            use std::io::Write;
            tmp.write_all(outcome.updated.as_bytes())
                .with_context(|| format!("writing staged {}", outcome.path.display()))?;
            tmp.flush()?;
            // A staged file is created with the private mode a temporary file
            // deserves, and renaming it over the target hands the target that
            // mode. So an executable script stopped being executable, and a
            // file the group could read became one only its owner can. What
            // the file was is what it stays.
            if let Ok(existing) = std::fs::metadata(&outcome.path) {
                use std::os::unix::fs::PermissionsExt;
                let mode = existing.permissions().mode();
                std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(mode))
                    .with_context(|| format!("keeping the mode of {}", outcome.path.display()))?;
            }
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
    drop(locks);
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
/// Read from the source and not assumed: generated code that arrives four spaces
/// deep in a two-space TypeScript file or a tab-indented Go file is a visible wart on
/// every line it touches. The shortest indentation any line carries is one level,
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

    /// The file a commit writes keeps the mode it had.
    ///
    /// A staged file is created private, the way a temporary file should be,
    /// and renaming it over the target handed the target that mode. An
    /// executable script stopped being executable, and a file the group could
    /// read became one only its owner can.
    #[cfg(feature = "cli")]
    #[test]
    fn committing_keeps_the_mode_the_file_had() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tool.py");
        crate::vfs::write(&path, "print(1)\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut set = EditSet::new();
        set.add(&path, Edit::new(Span::new(6, 7), "2", "change the number"));
        let outcomes = plan(&set, Validation::None).unwrap();
        commit(&outcomes).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "the file was executable and stays so.");
        assert_eq!(crate::vfs::read_to_string(&path).unwrap(), "print(2)\n");
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
    fn a_commit_refuses_when_the_file_changed_since_the_plan_was_made() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.rs");
        crate::vfs::write(&path, "fn one() {}\n").unwrap();

        // Two plans from the same basis, the shape of two `--write` runs racing on
        // one workspace. Both used to report success and the second overwrote the
        // first, so one rename vanished without a word.
        let mut first = EditSet::new();
        first.add(&path, Edit::new(Span::new(3, 6), "won", "rename"));
        let mut second = EditSet::new();
        second.add(&path, Edit::new(Span::new(3, 6), "two", "rename"));
        let plan_a = plan(&first, Validation::ReparseStrict).unwrap();
        let plan_b = plan(&second, Validation::ReparseStrict).unwrap();

        assert_eq!(commit(&plan_a).unwrap(), 1);
        let err = commit(&plan_b).unwrap_err().to_string();
        assert!(
            err.contains("changed since the plan was made"),
            "unexpected error: {err}"
        );
        assert_eq!(
            crate::vfs::read_to_string(&path).unwrap(),
            "fn won() {}\n",
            "the first edit must survive"
        );
    }

    #[test]
    fn a_stale_commit_writes_none_of_its_files() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.rs");
        let b = tmp.path().join("b.rs");
        crate::vfs::write(&a, "fn a() {}\n").unwrap();
        crate::vfs::write(&b, "fn b() {}\n").unwrap();

        let mut set = EditSet::new();
        set.add(&a, Edit::new(Span::new(3, 4), "x", "rename"));
        set.add(&b, Edit::new(Span::new(3, 4), "y", "rename"));
        let outcomes = plan(&set, Validation::ReparseStrict).unwrap();

        // Only one of the two files moved on, and the fresh one must not be written
        // either. Half a refactoring is the state the staging design exists to prevent.
        crate::vfs::write(&b, "fn moved() {}\n").unwrap();
        let err = commit(&outcomes).unwrap_err().to_string();
        assert!(
            err.contains("changed since the plan was made"),
            "unexpected error: {err}"
        );
        assert_eq!(crate::vfs::read_to_string(&a).unwrap(), "fn a() {}\n");
        assert_eq!(crate::vfs::read_to_string(&b).unwrap(), "fn moved() {}\n");
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
