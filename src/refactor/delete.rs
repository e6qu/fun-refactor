//! Safe delete: remove a definition only when nothing provably still uses it.
//!
//! The refusal is the feature. Deleting something that is still called is the exact
//! mistake this tool exists to prevent, so a reference that resolved well enough to
//! rewrite (`exact` or `import-qualified`) stops the delete and is reported with its
//! file, line and column. Weaker matches — a name that resolved elsewhere, a hit in a
//! string or comment — cannot be proven to be uses, so they are surfaced as warnings
//! instead of silently blocking or silently ignoring the delete.
//!
//! [`find_unused`] is the reporting half: candidates for deletion, found by combining
//! "nothing references it" with "nothing reachable from an entry point calls it".

use super::{Warning, WarningKind};
use crate::analysis::call_graph::CallGraph;
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::model::{Confidence, SymbolId};
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A delete that has been worked out but not applied.
#[derive(Debug)]
pub struct DeletePlan {
    pub symbol: SymbolId,
    pub name: String,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Number of definition sites removed. More than one only for kinds with no
    /// canonical definition, such as a CSS class declared by several rules.
    pub sites: usize,
}

/// Work out how to delete `symbol` and everything that defines it.
///
/// Fails — with every blocking reference listed as `file:line:col` — when any
/// reference to the symbol resolved strongly enough to be trusted. References inside
/// the definition being deleted (a recursive call, a method calling its own class)
/// do not block: they disappear with it.
pub fn plan(index: &Index, symbol: SymbolId) -> Result<DeletePlan> {
    let target = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // Every definition site of the entity, so a CSS class declared by both `.btn` and
    // `.btn:hover` goes away as a whole rather than half.
    let group = index.definition_group(symbol);
    // Some definitions cannot be removed on their own: a CSS selector leaves an
    // orphaned rule behind, so the span is widened to what actually has to go.
    let parsers = crate::parse::Parsers::new();
    let mut sites: Vec<(PathBuf, Span)> = Vec::new();
    for id in &group {
        let Some(definition) = index.symbol(*id) else {
            continue;
        };
        let span = match std::fs::read_to_string(&definition.file) {
            Ok(source) => match parsers.parse(definition.language, &source) {
                Ok(parsed) => widen_for_delete(&parsed, &source, definition),
                Err(_) => definition.full_span,
            },
            Err(_) => definition.full_span,
        };
        sites.push((definition.file.clone(), span));
    }

    let inside_a_site = |file: &Path, span: Span| {
        sites
            .iter()
            .any(|(site_file, site_span)| site_file == file && site_span.contains(span))
    };

    let mut sources = Sources::default();
    let mut blocking: Vec<(PathBuf, Span)> = Vec::new();
    let mut weak: Vec<(PathBuf, Span, Confidence)> = Vec::new();
    let mut seen: HashSet<(PathBuf, Span)> = HashSet::new();

    for id in &group {
        for reference in index.references_to(*id) {
            if !seen.insert((reference.file.clone(), reference.span)) {
                continue;
            }
            if inside_a_site(&reference.file, reference.span) {
                continue;
            }
            if reference.confidence.is_safe_to_rewrite() {
                blocking.push((reference.file.clone(), reference.span));
            } else {
                weak.push((reference.file.clone(), reference.span, reference.confidence));
            }
        }
    }

    if !blocking.is_empty() {
        blocking.sort();
        let mut message = format!(
            "refusing to delete '{}': {} reference(s) still resolve to it",
            target.name,
            blocking.len()
        );
        for (file, span) in &blocking {
            let (line, col) = sources.line_col(file, span.start);
            message.push_str(&format!("\n  {}:{line}:{col}", file.display()));
        }
        message.push_str("\nRemove or repoint these uses first; nothing was changed.");
        anyhow::bail!("{message}");
    }

    // Nothing proven uses it, so the definitions can go. Whole lines are removed when
    // the definition is alone on its lines, otherwise the leftover indentation and
    // newline would remain behind as a blank line.
    let mut deletions: HashMap<PathBuf, Vec<Span>> = HashMap::new();
    for (file, span) in &sites {
        let resolved = match sources.get(file) {
            Some(source) => deletion_span(source, *span),
            None => *span,
        };
        deletions.entry(file.clone()).or_default().push(resolved);
    }

    let mut edits = EditSet::new();
    let mut deleted: Vec<(PathBuf, Span)> = Vec::new();
    for (file, spans) in &mut deletions {
        // Two adjacent sites can claim the same blank line; one edit per merged run
        // keeps the edit set free of the overlaps the engine would reject.
        spans.sort_by_key(|s| (s.start, s.end));
        for span in merge_runs(spans) {
            edits.add(
                file.clone(),
                Edit::new(span, "", format!("delete {}", target.name)),
            );
            deleted.push((file.clone(), span));
        }
    }

    // Everything found but deliberately not acted on.
    let mut warnings = Vec::new();
    for (file, span, confidence) in weak {
        let (line, col) = sources.line_col(&file, span.start);
        warnings.push(Warning {
            kind: WarningKind::WeaklyResolved,
            file,
            line,
            col,
            detail: format!(
                "reference resolved only as '{}'; it may or may not be a use of '{}'",
                confidence.as_str(),
                target.name
            ),
        });
    }

    // Same-named occurrences that resolved nowhere at all.
    for reference in index.unresolved_matching(symbol) {
        if reference.target.is_some() || seen.contains(&(reference.file.clone(), reference.span)) {
            continue;
        }
        let (line, col) = sources.line_col(&reference.file, reference.span.start);
        warnings.push(Warning {
            kind: WarningKind::WeaklyResolved,
            file: reference.file.clone(),
            line,
            col,
            detail: format!("unresolved occurrence of '{}'; left in place", target.name),
        });
    }

    warnings.extend(textual_occurrences(index, &target.name, &deleted)?);

    for (path, info) in index.files() {
        if info.had_parse_errors {
            warnings.push(Warning {
                kind: WarningKind::ParseErrors,
                file: path.clone(),
                line: 1,
                col: 1,
                detail: "file has syntax errors; uses hidden in it would not have been seen".into(),
            });
        }
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    Ok(DeletePlan {
        symbol,
        name: target.name.clone(),
        edits,
        warnings,
        sites: sites.len(),
    })
}

/// Symbols nothing references and nothing reachable from `entrypoints` reaches.
///
/// This is what powers dead-CSS-selector, unused-Terraform-variable,
/// unused-`values.yaml`-key and unused-function reports: a symbol qualifies when no
/// resolved reference targets it (references from inside its own definition do not
/// count, so dead recursive code is still found) and the call graph cannot reach it
/// from any given entry point.
///
/// **The result is a candidate list, not a delete list.** Reachability follows
/// *resolved* call edges only, and a call the index could not resolve produces no edge
/// at all. So anything reached exclusively by dynamic dispatch — a trait object or
/// interface value, a function held in a map or struct field, reflection, a
/// string-keyed handler table, a name assembled at runtime — is live code that will
/// appear in this list. The same is true of a symbol used only from a file that failed
/// to parse. Mutually recursive dead code is the opposite error: the pair reference
/// each other, so neither is reported. Feed each candidate to [`plan`] before acting.
pub fn find_unused(index: &Index, entrypoints: &[SymbolId]) -> Vec<SymbolId> {
    let call_graph = CallGraph::build(index);
    let reachable = call_graph.reachable_from(entrypoints);

    // A reference from inside the symbol's own definition is not an outside use.
    let mut referenced: HashSet<SymbolId> = HashSet::new();
    for reference in &index.references {
        let Some(id) = reference.target else {
            continue;
        };
        let Some(symbol) = index.symbol(id) else {
            continue;
        };
        if symbol.file == reference.file && symbol.full_span.contains(reference.span) {
            continue;
        }
        referenced.insert(id);
    }

    let mut unused: Vec<SymbolId> = index
        .symbols
        .iter()
        .filter(|s| !reachable.contains(&s.id) && !referenced.contains(&s.id))
        .map(|s| s.id)
        .collect();
    unused.sort();
    unused
}

/// The bytes a delete should actually remove.
///
/// When the definition is the only thing on its lines, the whole lines go, indentation
/// and trailing newline included. A blank line immediately after is swallowed too, but
/// only when the definition was already preceded by a blank line or the start of the
/// file — otherwise that blank line is a separator belonging to the code that stays.
/// Widen a symbol's span to the construct that cannot survive without it.
///
/// A CSS class selector is its own symbol — that is what a rename rewrites — but the
/// rule it heads is meaningless once it is gone. If the selector is the only one on
/// the rule, the whole rule goes; if it is one of several, only that selector and its
/// comma do, leaving the rule to its remaining selectors.
fn widen_for_delete(
    parsed: &crate::parse::Parsed,
    source: &str,
    symbol: &crate::model::Symbol,
) -> Span {
    use crate::model::SymbolKind;
    if !matches!(symbol.kind, SymbolKind::Selector | SymbolKind::ElementId)
        || !matches!(symbol.language, crate::lang::Language::Css | crate::lang::Language::Scss)
    {
        return symbol.full_span;
    }

    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(symbol.full_span.start, symbol.full_span.end)
    else {
        return symbol.full_span;
    };

    // Climb to the selector as the rule sees it, then to the rule itself.
    let mut selector = node;
    while let Some(parent) = selector.parent() {
        if parent.kind() == "selectors" || parent.kind() == "rule_set" {
            break;
        }
        selector = parent;
    }
    let Some(list) = selector.parent() else {
        return symbol.full_span;
    };

    let siblings: Vec<tree_sitter::Node> = {
        let mut cursor = list.walk();
        list.named_children(&mut cursor)
            .filter(|c| !c.kind().contains("comment") && !c.kind().contains("block"))
            .collect()
    };

    if siblings.len() <= 1 {
        // The rule has nothing left to apply to.
        let rule = if list.kind() == "rule_set" {
            list
        } else {
            list.parent().unwrap_or(list)
        };
        return Span::from(rule);
    }

    // One of several: take this selector and the comma joining it to the next.
    let this = Span::from(selector);
    let mut end = this.end;
    let bytes = source.as_bytes();
    while end < bytes.len() && (bytes[end] == b',' || bytes[end].is_ascii_whitespace()) {
        let was_comma = bytes[end] == b',';
        end += 1;
        if was_comma {
            while end < bytes.len() && bytes[end] == b' ' {
                end += 1;
            }
            return Span::new(this.start, end);
        }
    }
    // Last in the list: take the preceding comma instead.
    let mut start = this.start;
    while start > 0 && (bytes[start - 1] == b' ' || bytes[start - 1] == b',') {
        start -= 1;
        if bytes[start] == b',' {
            break;
        }
    }
    Span::new(start, this.end)
}

fn deletion_span(source: &str, span: Span) -> Span {
    if span.is_empty() || span.end > source.len() {
        return span;
    }
    let first = full_line_span(source, span.start);
    let last = full_line_span(source, span.end - 1);
    let line_end = last.end.max(first.end).max(span.end);
    let alone = source[first.start..span.start].trim().is_empty()
        && source[span.end..line_end].trim().is_empty();
    if !alone {
        return span;
    }

    let mut end = line_end;
    let preceded_by_gap = first.start == 0 || {
        let previous = full_line_span(source, first.start - 1);
        previous.text(source).trim().is_empty()
    };
    if preceded_by_gap && end < source.len() {
        let next = full_line_span(source, end);
        if next.text(source).trim().is_empty() {
            end = next.end;
        }
    }
    Span::new(first.start, end)
}

/// Collapse overlapping or touching spans so each becomes one edit.
fn merge_runs(spans: &[Span]) -> Vec<Span> {
    let mut merged: Vec<Span> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => {
                last.end = last.end.max(span.end);
            }
            _ => merged.push(*span),
        }
    }
    merged
}

/// The name inside string literals and comments anywhere in the workspace.
///
/// Nothing resolves these, so they are reported for review. Occurrences inside the
/// bytes being deleted are not outstanding — they go away with the definition.
fn textual_occurrences(
    index: &Index,
    name: &str,
    deleted: &[(PathBuf, Span)],
) -> Result<Vec<Warning>> {
    let parsers = Parsers::new();
    let mut warnings = Vec::new();

    for (path, info) in index.files() {
        let Ok(source) = std::fs::read_to_string(path) else {
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
                if deleted
                    .iter()
                    .any(|(file, gone)| file == path && gone.overlaps(absolute))
                {
                    continue;
                }
                let pos = line_index.line_col(absolute.start, &source);
                warnings.push(Warning {
                    kind: WarningKind::TextualOccurrence,
                    file: path.clone(),
                    line: pos.line,
                    col: pos.col,
                    detail: format!(
                        "'{name}' appears in a string or comment; it is not deleted and may \
                         be a use nothing can resolve"
                    ),
                });
            }
        }
    }
    Ok(warnings)
}

/// Spans of string literals, comments and Helm template actions.
fn string_and_comment_spans(parsed: &Parsed) -> Vec<Span> {
    let mut spans: Vec<Span> = parsed.template_actions.clone();
    let mut cursor = parsed.root().walk();
    let mut recurse = true;

    loop {
        let node = cursor.node();
        let kind = node.kind();
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

/// Is the match at `offset` a whole word rather than part of a longer one?
fn is_word_boundary(haystack: &str, offset: usize, len: usize) -> bool {
    let before_ok = haystack[..offset]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let after_ok = haystack[offset + len..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    before_ok && after_ok
}

/// Reads each file at most once while one plan is being built.
#[derive(Default)]
struct Sources {
    cache: HashMap<PathBuf, Option<String>>,
}

impl Sources {
    fn get(&mut self, path: &Path) -> Option<&str> {
        self.cache
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok())
            .as_deref()
    }

    fn line_col(&mut self, path: &Path, offset: usize) -> (usize, usize) {
        match self.get(path) {
            Some(source) => {
                let pos = LineIndex::new(source).line_col(offset, source);
                (pos.line, pos.col)
            }
            None => (0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_line_deletion_takes_indentation_and_newline() {
        let source = "fn a() {}\nfn b() {}\n";
        assert_eq!(deletion_span(source, Span::new(0, 9)), Span::new(0, 10));
    }

    #[test]
    fn a_definition_sharing_its_line_loses_only_its_own_bytes() {
        let source = "fn a() {} fn b() {}\n";
        assert_eq!(deletion_span(source, Span::new(0, 9)), Span::new(0, 9));
    }

    #[test]
    fn a_leading_blank_line_is_not_left_behind() {
        // Deleting the first definition must not leave the file starting blank.
        let source = "fn a() {}\n\nfn b() {}\n";
        assert_eq!(deletion_span(source, Span::new(0, 9)), Span::new(0, 11));
    }

    #[test]
    fn a_separator_blank_line_belonging_to_the_survivor_is_kept() {
        // `b` is not preceded by a gap, so the blank line after it separates `a` from
        // `c` once `b` is gone and must stay.
        let source = "fn a() {}\nfn b() {}\n\nfn c() {}\n";
        assert_eq!(deletion_span(source, Span::new(10, 19)), Span::new(10, 20));
    }

    #[test]
    fn merge_runs_collapses_touching_spans() {
        let spans = [Span::new(0, 10), Span::new(10, 20), Span::new(30, 40)];
        assert_eq!(merge_runs(&spans), vec![Span::new(0, 20), Span::new(30, 40)]);
    }
}
