//! Organize imports: drop the ones nothing names, sort the rest.
//!
//! Both halves are deliberately timid.
//!
//! *Removal* is driven by the index's own records: an import statement goes only when
//! no reference outside an import statement names anything it binds. A glob import
//! binds names nobody can enumerate, and a side-effect import binds nothing at all, so
//! neither is ever removed — they are reported instead.
//!
//! *Sorting* never regenerates import syntax. Each statement's original bytes are
//! reordered as-is, within one contiguous run of import lines. A blank line, a comment
//! or any other statement ends the run, because import grouping is a decision a
//! programmer made and this refactoring has no business overruling it.

use super::{Refusal, Warning, WarningKind};
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::Import;
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// An import reorganisation that has been worked out but not applied.
#[derive(Debug)]
pub struct ImportsPlan {
    pub file: PathBuf,
    pub language: Language,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Statements dropped because nothing in the file names what they bind.
    pub removed: Vec<RemovedImport>,
    /// Number of contiguous blocks whose statements changed order.
    pub sorted_blocks: usize,
}

/// One import statement the plan removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedImport {
    /// The module path as written.
    pub path: String,
    /// The local names the statement bound, none of which anything used.
    pub bindings: Vec<String>,
    pub span: Span,
    pub line: usize,
}

/// Work out how to organize the imports of one file.
///
/// Refuses for languages that have no import statement to organize, and for files with
/// syntax errors: a use hidden inside an unparsed region would make a removal look
/// safe when it is not.
///
/// Liveness is decided by name. That is exact for a value or type that must be spelled
/// where it is used, and wrong for anything a language brings into scope invisibly: a
/// Rust trait imported only so its methods resolve, a Python module imported for its
/// registration side effects under a name that is never mentioned again, a TypeScript
/// type used only in a JSDoc comment. Check the [`ImportsPlan::removed`] list before
/// committing.
pub fn plan(index: &Index, file: &Path) -> Result<ImportsPlan> {
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;

    if !organizable(info.language) {
        return Err(Refusal::Unsupported {
            operation: "organize imports".into(),
            language: info.language.name().into(),
        }
        .into());
    }

    if info.had_parse_errors {
        anyhow::bail!(
            "refusing to organize imports in {}: the file has syntax errors, so a use \
             hidden in the unparsed part could make a live import look unused",
            file.display()
        );
    }

    let source = std::fs::read_to_string(file)?;
    let line_index = LineIndex::new(&source);
    let mut warnings = Vec::new();

    let statements = statements(info.imports.iter(), &source, info.language);
    if statements.is_empty() {
        return Ok(ImportsPlan {
            file: file.to_path_buf(),
            language: info.language,
            edits: EditSet::new(),
            warnings,
            removed: Vec::new(),
            sorted_blocks: 0,
        });
    }

    // Every name used in the file, ignoring the import statements themselves — an
    // import naming `HashMap` is not a use of `HashMap`.
    let mut live: HashSet<&str> = HashSet::new();
    for reference_index in &info.references {
        let reference = &index.references[*reference_index];
        if statements.iter().any(|s| s.span.contains(reference.span)) {
            continue;
        }
        live.insert(reference.name.as_str());
    }

    let mut removed = Vec::new();
    let mut drop_statement = vec![false; statements.len()];
    for (i, statement) in statements.iter().enumerate() {
        let position = line_index.line_col(statement.span.start, &source);
        if statement.is_glob {
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: format!(
                    "'{}' is a glob import; what it binds cannot be enumerated, so it is \
                     never removed",
                    statement.path
                ),
            });
            continue;
        }
        if statement.bindings.is_empty() {
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: format!(
                    "'{}' binds no name; it is imported for its side effects and is never \
                     removed",
                    statement.path
                ),
            });
            continue;
        }
        if statement
            .bindings
            .iter()
            .any(|binding| live.contains(binding.as_str()))
        {
            continue;
        }
        drop_statement[i] = true;
        removed.push(RemovedImport {
            path: statement.path.clone(),
            bindings: statement.bindings.clone(),
            span: statement.span,
            line: position.line,
        });
    }

    let mut edits = EditSet::new();
    let mut sorted_blocks = 0;

    for block in blocks(&statements) {
        let members = &statements[block.clone()];
        if members.iter().any(|s| !s.line_exclusive) {
            let position = line_index.line_col(members[0].span.start, &source);
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: "an import here shares its line with other code; the block was left \
                         untouched rather than risk moving that code"
                    .into(),
            });
            continue;
        }

        let region = Span::new(
            members[0].lines.start,
            members[members.len() - 1].lines.end,
        );
        let kept: Vec<&Statement> = members
            .iter()
            .enumerate()
            .filter(|(i, _)| !drop_statement[block.start + i])
            .map(|(_, s)| s)
            .collect();

        let before = region.text(&source);
        let after = rebuild(&kept, &source, before.ends_with('\n'));
        if after == before {
            continue;
        }

        let reordered = kept
            .iter()
            .zip(sorted(&kept))
            .any(|(original, sorted)| original.span != sorted.span);
        if reordered {
            sorted_blocks += 1;
        }

        let dropped = members.len() - kept.len();
        let reason = match (dropped, reordered) {
            (0, _) => "sort import block".to_string(),
            (n, false) => format!("remove {n} unused import(s)"),
            (n, true) => format!("remove {n} unused import(s) and sort the block"),
        };
        edits.add(file.to_path_buf(), Edit::new(region, after, reason));
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    Ok(ImportsPlan {
        file: file.to_path_buf(),
        language: info.language,
        edits,
        warnings,
        removed,
        sorted_blocks,
    })
}

/// Does this language have import statements worth organizing?
///
/// CSS and SCSS are excluded on purpose even though they have `@import`: order there
/// is semantic — a later rule beats an earlier one and `@import` must precede all other
/// rules — so sorting would change what the stylesheet means. The markup and config
/// languages have no import construct at all, and Bash `source` is an executed
/// statement rather than a declaration.
fn organizable(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Python
            | Language::TypeScript
            | Language::Tsx
            | Language::Zig
    )
}

/// One import statement, which may correspond to several [`Import`] records: a query
/// reports `use m::{a, b}` once per name, all sharing the statement's span.
#[derive(Debug)]
struct Statement {
    /// Bytes of the statement itself.
    span: Span,
    /// The whole lines the statement occupies, newline included.
    lines: Span,
    path: String,
    /// Local names the statement introduces.
    bindings: Vec<String>,
    is_glob: bool,
    /// True when nothing but this statement (and whitespace) is on those lines.
    line_exclusive: bool,
}

/// Collapse import records into statements, in source order.
fn statements<'a>(
    imports: impl Iterator<Item = &'a Import>,
    source: &str,
    language: Language,
) -> Vec<Statement> {
    let mut grouped: BTreeMap<Span, Vec<&Import>> = BTreeMap::new();
    for import in imports {
        grouped.entry(import.span).or_default().push(import);
    }

    grouped
        .into_iter()
        .filter(|(span, _)| !span.is_empty() && span.end <= source.len())
        .map(|(span, records)| {
            let first = full_line_span(source, span.start);
            let last = full_line_span(source, span.end - 1);
            let lines = Span::new(first.start, last.end.max(first.end).max(span.end));
            let line_exclusive = source[lines.start..span.start].trim().is_empty()
                && source[span.end..lines.end].trim().is_empty();

            let mut bindings = Vec::new();
            for record in &records {
                bindings.extend(record.names.iter().map(|n| n.local.clone()));
                if let Some(alias) = &record.alias {
                    bindings.push(alias.clone());
                }
            }
            if bindings.is_empty() {
                bindings.extend(implicit_binding(&records[0].path, language));
            }
            // Go's `import _ "embed"` binds deliberately nothing.
            bindings.retain(|b| b != "_");
            bindings.sort();
            bindings.dedup();

            Statement {
                span,
                lines,
                path: records[0].path.clone(),
                bindings,
                is_glob: records.iter().any(|r| r.is_glob),
                line_exclusive,
            }
        })
        .collect()
}

/// The name a whole-module import binds without naming it.
///
/// `use std::fmt;`, `import "os"` and `import os` all bind their last path segment.
/// TypeScript and Zig have no such form: an import with no named binding there is a
/// side-effect import and binds nothing, so guessing a name from the path would invent
/// a binding that does not exist.
fn implicit_binding(path: &str, language: Language) -> Option<String> {
    if !matches!(language, Language::Rust | Language::Go | Language::Python) {
        return None;
    }
    path.rsplit(['/', ':', '.'])
        .find(|segment| !segment.is_empty())
        .filter(|segment| segment.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .map(|segment| segment.to_string())
}

/// Split statements into runs of directly consecutive import lines.
///
/// A blank line, a comment or any other statement between two imports leaves a gap in
/// the line coverage, which ends the run.
fn blocks(statements: &[Statement]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..statements.len() {
        if statements[i].lines.start != statements[i - 1].lines.end {
            out.push(start..i);
            start = i;
        }
    }
    if !statements.is_empty() {
        out.push(start..statements.len());
    }
    out
}

/// The statements of a block in path order, original order breaking ties.
fn sorted<'a>(statements: &[&'a Statement]) -> Vec<&'a Statement> {
    let mut out = statements.to_vec();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Rebuild a block from the original bytes of the statements that survive.
///
/// Each statement contributes its own line text verbatim, so indentation, spacing and
/// the exact spelling of the statement are carried across untouched. Nothing is
/// regenerated from the parsed import.
fn rebuild(kept: &[&Statement], source: &str, trailing_newline: bool) -> String {
    let parts: Vec<&str> = sorted(kept)
        .iter()
        .map(|s| {
            let text = s.lines.text(source);
            text.strip_suffix('\n').unwrap_or(text)
        })
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let mut out = parts.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_bindings_only_where_the_language_has_them() {
        assert_eq!(
            implicit_binding("std::fmt", Language::Rust),
            Some("fmt".into())
        );
        assert_eq!(
            implicit_binding("net/http", Language::Go),
            Some("http".into())
        );
        assert_eq!(implicit_binding("os.path", Language::Python), Some("path".into()));
        // A TS side-effect import binds nothing, so no name may be invented for it.
        assert_eq!(implicit_binding("./polyfills", Language::TypeScript), None);
        // A path segment that is not an identifier cannot be a binding.
        assert_eq!(implicit_binding("zed::*", Language::Rust), None);
    }

    #[test]
    fn organizable_languages_match_the_ones_with_import_declarations() {
        for language in [
            Language::Rust,
            Language::Go,
            Language::Python,
            Language::TypeScript,
            Language::Tsx,
            Language::Zig,
        ] {
            assert!(organizable(language), "{language} has imports to organize");
        }
        for language in [
            Language::Css,
            Language::Scss,
            Language::Html,
            Language::Xml,
            Language::Markdown,
            Language::Yaml,
            Language::Helm,
            Language::Hcl,
            Language::Bash,
        ] {
            assert!(!organizable(language), "{language} must refuse");
        }
    }
}
