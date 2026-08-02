//! Cascading cleanup: remove a flag and everything that only existed to serve it.
//!
//! Deleting a feature flag is never one edit. The flag's uses become constants, the
//! conditionals around them collapse to whichever branch survives, and whatever only
//! that dead branch referenced becomes unused in turn. Uber's Piranha showed this
//! chain is what makes flag removal worth automating — the first edit is trivial and
//! the cascade is the work.
//!
//! Each round re-indexes the rewritten sources, so every decision is made against
//! what the code actually says now rather than a prediction of it. The cascade stops
//! when a round changes nothing.

use super::Refusal;
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::SymbolKind;
use crate::parse::{Parsed, Parsers};
use crate::scan::{scan, ScanOptions};
use crate::span::Span;
use anyhow::Result;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Bound on rounds, so a rule that keeps finding work cannot spin forever.
const MAX_ROUNDS: usize = 12;

/// A cascade worked out but not applied.
#[derive(Debug)]
pub struct CascadePlan {
    pub flag: String,
    pub value: bool,
    pub edits: EditSet,
    /// What each round did, in order.
    pub rounds: Vec<RoundSummary>,
}

/// One pass of the cascade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundSummary {
    pub description: String,
    pub files_touched: usize,
}

impl CascadePlan {
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// Remove `flag`, assuming it always had `value`, and clean up what follows.
pub fn remove_flag(root: &Path, flag: &str, value: bool) -> Result<CascadePlan> {
    let scanned = scan(root, &ScanOptions::default())?;

    // The cascade rewrites in memory and re-indexes each round, so the originals are
    // kept to diff against at the end.
    let mut sources: BTreeMap<PathBuf, (Language, String)> = BTreeMap::new();
    for file in &scanned.files {
        let Ok(text) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        sources.insert(file.path.clone(), (file.language, text));
    }
    let originals = sources.clone();

    // Only symbols that had a use before any of this started can be *orphaned* by
    // the cascade. Anything already unreferenced was unused before we arrived and is
    // not this refactoring's business — removing it would turn a flag cleanup into an
    // unrelated purge of the workspace.
    let initially_used: HashSet<(String, PathBuf)> = {
        let snapshot: Vec<(PathBuf, Language, String)> = sources
            .iter()
            .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
            .collect();
        let index = Index::build_from_sources(&snapshot)?;
        index
            .symbols
            .iter()
            .filter(|s| !index.references_to(s.id).is_empty())
            .map(|s| (s.name.clone(), s.file.clone()))
            .collect()
    };

    let mut rounds = Vec::new();
    let mut removed_definition = false;

    for round in 0..MAX_ROUNDS {
        let snapshot: Vec<(PathBuf, Language, String)> = sources
            .iter()
            .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
            .collect();
        let index = Index::build_from_sources(&snapshot)?;

        // Round 1 substitutes the flag; later rounds only tidy what that exposed.
        let changes = if !removed_definition {
            let substituted = substitute_flag(&index, flag, value)?;
            if substituted.is_empty() {
                if round == 0 {
                    anyhow::bail!(
                        "no symbol named '{flag}' to remove; nothing was changed"
                    );
                }
                Vec::new()
            } else {
                removed_definition = true;
                rounds.push(RoundSummary {
                    description: format!("replaced uses of {flag} with {value}"),
                    files_touched: distinct_files(&substituted),
                });
                substituted
            }
        } else {
            let simplified = simplify_constants(&sources)?;
            if !simplified.is_empty() {
                rounds.push(RoundSummary {
                    description: "collapsed conditionals whose test is now constant".into(),
                    files_touched: distinct_files(&simplified),
                });
                simplified
            } else {
                let orphans = remove_orphans(&index, &sources, flag, &initially_used)?;
                if orphans.is_empty() {
                    break;
                }
                rounds.push(RoundSummary {
                    description: "removed symbols nothing uses any more".into(),
                    files_touched: distinct_files(&orphans),
                });
                orphans
            }
        };

        if changes.is_empty() {
            break;
        }
        apply_in_memory(&mut sources, &changes)?;
    }

    // The result is the difference between what was on disk and what the cascade
    // arrived at, expressed as one replacement per changed file.
    let mut edits = EditSet::new();
    for (path, (_, final_text)) in &sources {
        let Some((_, original)) = originals.get(path) else {
            continue;
        };
        if original != final_text {
            edits.add(
                path.clone(),
                Edit::new(
                    Span::new(0, original.len()),
                    final_text.clone(),
                    format!("cascade from removing {flag}"),
                ),
            );
        }
    }

    Ok(CascadePlan {
        flag: flag.to_string(),
        value,
        edits,
        rounds,
    })
}

/// A change to make to one file, as a byte range and its replacement.
type Change = (PathBuf, Span, String);

fn distinct_files(changes: &[Change]) -> usize {
    let mut paths: Vec<&PathBuf> = changes.iter().map(|(p, _, _)| p).collect();
    paths.sort();
    paths.dedup();
    paths.len()
}

/// Replace every use of the flag with a literal, and delete its definition.
fn substitute_flag(index: &Index, flag: &str, value: bool) -> Result<Vec<Change>> {
    let definitions = index.find_symbols(flag, None);
    if definitions.is_empty() {
        return Ok(Vec::new());
    }
    if definitions.len() > 1 {
        anyhow::bail!(
            "'{flag}' is defined {} times; say which one with a position",
            definitions.len()
        );
    }
    let definition = definitions[0];
    if !matches!(
        definition.kind,
        SymbolKind::Constant | SymbolKind::Variable | SymbolKind::Function
    ) {
        return Err(Refusal::Unsupported {
            operation: "remove flag".into(),
            language: format!("a {} is not a flag", definition.kind.as_str()),
        }
        .into());
    }

    let literal = literal_for(definition.language, value);
    let mut changes = Vec::new();

    for reference in index.references_to(definition.id) {
        if !reference.confidence.is_safe_to_rewrite() {
            // A use we cannot place is a use we must not rewrite.
            continue;
        }
        // A call to a flag-returning function is replaced along with its parentheses.
        changes.push((reference.file.clone(), reference.span, literal.to_string()));
    }

    // The definition goes with its whole line.
    changes.push((
        definition.file.clone(),
        definition.full_span,
        String::new(),
    ));
    Ok(changes)
}

/// How a language spells a boolean literal.
fn literal_for(language: Language, value: bool) -> &'static str {
    match (language, value) {
        (Language::Python, true) => "True",
        (Language::Python, false) => "False",
        (_, true) => "true",
        (_, false) => "false",
    }
}

/// Collapse `if true { … } else { … }` to the branch that survives.
fn simplify_constants(sources: &BTreeMap<PathBuf, (Language, String)>) -> Result<Vec<Change>> {
    let parsers = Parsers::new();
    let mut changes = Vec::new();

    for (path, (language, source)) in sources {
        if !matches!(
            language,
            Language::Rust
                | Language::Go
                | Language::TypeScript
                | Language::Tsx
                | Language::Python
        ) {
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        let mut found = constant_conditionals(&parsed, source, *language);
        // One change per file per round: spans shift as soon as one is applied, and
        // the next round re-parses anyway.
        if let Some(change) = found.pop() {
            changes.push((path.clone(), change.0, change.1));
        }
    }
    Ok(changes)
}

/// Conditionals whose test is a boolean literal, with their replacement text.
fn constant_conditionals(
    parsed: &Parsed,
    source: &str,
    language: Language,
) -> Vec<(Span, String)> {
    let mut out = Vec::new();
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];

    while let Some(node) = stack.pop() {
        stack.extend(node.named_children(&mut cursor));

        if !node.is_named() || !node.kind().starts_with("if_") {
            continue;
        }
        let Some(condition) = node.child_by_field_name("condition") else {
            continue;
        };
        let test = Span::from(condition).text(source).trim();
        let truth = match test {
            "true" | "True" => true,
            "false" | "False" => false,
            _ => continue,
        };

        let Some(consequence) = node.child_by_field_name("consequence") else {
            continue;
        };
        let alternative = node.child_by_field_name("alternative");

        let kept = if truth {
            Some(consequence)
        } else {
            alternative.map(else_body)
        };

        let span = Span::from(node);
        let indent = crate::edit::line_indent(source, span.start);
        let replacement = match kept {
            // The surviving branch loses a level of nesting.
            Some(branch) => dedent_block(Span::from(branch), source, &indent, language),
            // A false `if` with no else leaves nothing behind.
            None => String::new(),
        };
        out.push((span, replacement));
    }
    out
}

/// The block an else clause wraps.
fn else_body(alternative: Node<'_>) -> Node<'_> {
    let mut cursor = alternative.walk();
    let inner = alternative
        .named_children(&mut cursor)
        .find(|c| c.kind().contains("block"));
    inner.unwrap_or(alternative)
}

/// The statements inside a block, moved out one indentation level.
fn dedent_block(block: Span, source: &str, indent: &str, language: Language) -> String {
    let text = block.text(source);
    let trimmed = text.trim();
    let inner = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        &source[block.start + lead + 1..block.end - trail - 1]
    } else {
        text
    };

    let unit = "    ";
    let lines: Vec<String> = inner
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                let stripped = line
                    .strip_prefix(&format!("{indent}{unit}"))
                    .unwrap_or_else(|| line.trim_start_matches(' '));
                format!("{indent}{stripped}")
            }
        })
        .collect();

    let body = lines
        .iter()
        .rev()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    let _ = language;
    // The replacement starts where the `if` did, so its first line needs no indent.
    body.trim_start().to_string()
}

/// Delete symbols that nothing references any more.
///
/// Only functions and constants are considered, and only ones the cascade could
/// plausibly have orphaned — a symbol that was already unused before any of this
/// started is not this refactoring's business.
fn remove_orphans(
    index: &Index,
    sources: &BTreeMap<PathBuf, (Language, String)>,
    flag: &str,
    initially_used: &HashSet<(String, PathBuf)>,
) -> Result<Vec<Change>> {
    let mut changes = Vec::new();

    for symbol in &index.symbols {
        if symbol.name == flag || symbol.exported {
            continue;
        }
        // It has to have lost a use, not merely have none.
        if !initially_used.contains(&(symbol.name.clone(), symbol.file.clone())) {
            continue;
        }
        if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Constant) {
            continue;
        }
        // Entry points and anything still referenced stay.
        if !index.references_to(symbol.id).is_empty() {
            continue;
        }
        if symbol.name == "main" || symbol.name.starts_with("test") {
            continue;
        }
        if !sources.contains_key(&symbol.file) {
            continue;
        }
        changes.push((symbol.file.clone(), symbol.full_span, String::new()));
        // One per round: the next round re-indexes and finds whatever this exposed.
        break;
    }
    Ok(changes)
}

/// Apply changes to the in-memory sources, tidying the lines they empty.
fn apply_in_memory(
    sources: &mut BTreeMap<PathBuf, (Language, String)>,
    changes: &[Change],
) -> Result<()> {
    let mut by_file: BTreeMap<&PathBuf, Vec<&Change>> = BTreeMap::new();
    for change in changes {
        by_file.entry(&change.0).or_default().push(change);
    }

    for (path, mut file_changes) in by_file {
        let Some((_, source)) = sources.get_mut(path) else {
            continue;
        };
        file_changes.sort_by_key(|(_, span, _)| span.start);

        // Overlapping changes have no defined result; keep the first of each pair.
        let mut applied: Vec<&Change> = Vec::new();
        for change in file_changes {
            if applied
                .last()
                .is_some_and(|(_, previous, _)| previous.overlaps(change.1))
            {
                continue;
            }
            applied.push(change);
        }

        let mut updated = source.clone();
        for (_, span, replacement) in applied.iter().rev() {
            // Deleting a statement should take its line, not leave a blank one.
            let range = if replacement.is_empty() {
                // A definition usually spans several lines, so the deletion has to
                // cover all of them — taking only the first would leave the body
                // behind as a stray blank region.
                let first = crate::edit::full_line_span(&updated, span.start);
                let last =
                    crate::edit::full_line_span(&updated, span.end.saturating_sub(1).max(span.start));
                let whole = Span::new(first.start, last.end.max(span.end));
                if whole.text(&updated).trim() == span.text(&updated).trim() {
                    widen_to_blank_separator(&updated, whole)
                } else {
                    *span
                }
            } else {
                *span
            };
            updated.replace_range(range.start..range.end, replacement);
        }
        *source = updated;
    }
    Ok(())
}

/// Extend a whole-line deletion over the blank line that separated it from its
/// neighbour, so removing a definition does not leave a widening gap behind.
///
/// Only one blank line is taken, and only when the deleted text had a blank line
/// before it (or began the file) — otherwise the blank belonged to the code that
/// remains, as a separator it still needs.
fn widen_to_blank_separator(source: &str, line: Span) -> Span {
    let preceded_by_blank = line.start == 0 || {
        // Strip exactly the newline that ends the previous line, then look at that
        // line. Trimming every trailing newline would skip past the blank entirely
        // and inspect the code above it.
        let before = &source[..line.start];
        let previous = before.strip_suffix('\n').unwrap_or(before);
        previous.rsplit('\n').next().is_none_or(|l| l.trim().is_empty())
    };
    if !preceded_by_blank {
        return line;
    }

    let rest = &source[line.end..];
    let Some(next_end) = rest.find('\n').map(|i| line.end + i + 1) else {
        return line;
    };
    if source[line.end..next_end].trim().is_empty() {
        Span::new(line.start, next_end)
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deletion_takes_the_blank_line_that_separated_it() {
        let source = "a\n\nDELETED\n\nb\n";
        let line = crate::edit::full_line_span(source, source.find("DELETED").unwrap());
        let widened = widen_to_blank_separator(source, line);
        assert_eq!(widened.text(source), "DELETED\n\n");
    }

    #[test]
    fn a_deletion_leaves_a_separator_that_belongs_to_the_survivor() {
        // No blank line before it, so the blank after belongs to what remains.
        let source = "a\nDELETED\n\nb\n";
        let line = crate::edit::full_line_span(source, source.find("DELETED").unwrap());
        let widened = widen_to_blank_separator(source, line);
        assert_eq!(widened.text(source), "DELETED\n");
    }

    #[test]
    fn boolean_literals_use_each_language_spelling() {
        assert_eq!(literal_for(Language::Rust, true), "true");
        assert_eq!(literal_for(Language::Python, true), "True");
        assert_eq!(literal_for(Language::Python, false), "False");
        assert_eq!(literal_for(Language::Go, false), "false");
    }

    #[test]
    fn a_constant_conditional_is_recognised() {
        let source = "fn f() {\n    if true {\n        go();\n    }\n}\n";
        let parsed = Parsers::new().parse(Language::Rust, source).unwrap();
        let found = constant_conditionals(&parsed, source, Language::Rust);
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("go();"), "got {:?}", found[0].1);
    }

    #[test]
    fn a_variable_condition_is_left_alone() {
        let source = "fn f() {\n    if ready {\n        go();\n    }\n}\n";
        let parsed = Parsers::new().parse(Language::Rust, source).unwrap();
        assert!(constant_conditionals(&parsed, source, Language::Rust).is_empty());
    }

    #[test]
    fn a_false_conditional_without_an_else_collapses_to_nothing() {
        let source = "fn f() {\n    if false {\n        go();\n    }\n}\n";
        let parsed = Parsers::new().parse(Language::Rust, source).unwrap();
        let found = constant_conditionals(&parsed, source, Language::Rust);
        assert_eq!(found.len(), 1);
        assert!(found[0].1.trim().is_empty(), "got {:?}", found[0].1);
    }

    #[test]
    fn a_false_conditional_keeps_the_else_branch() {
        let source = "fn f() {\n    if false {\n        go();\n    } else {\n        wait();\n    }\n}\n";
        let parsed = Parsers::new().parse(Language::Rust, source).unwrap();
        let found = constant_conditionals(&parsed, source, Language::Rust);
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("wait();"), "got {:?}", found[0].1);
        assert!(!found[0].1.contains("go();"), "got {:?}", found[0].1);
    }
}
