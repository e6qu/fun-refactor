//! Rename a symbol and every reference that provably points at it.

use super::{Refusal, Warning, WarningKind};
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{anchor_slug, Confidence, ReferenceKind, Symbol, SymbolId, SymbolKind};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// A rename that has been worked out but not applied.
#[derive(Debug)]
pub struct RenamePlan {
    pub old_name: String,
    pub new_name: String,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Number of reference sites rewritten, excluding the definition itself.
    pub reference_edits: usize,
}

/// The reference spans a rename of `symbol` would rewrite.
pub fn rewritable_spans(
    index: &Index,
    symbol_id: SymbolId,
) -> std::collections::HashSet<(std::path::PathBuf, crate::span::Span)> {
    plan(index, symbol_id, "fr_rewritable_probe_")
        .map(|plan| {
            plan.edits
                .iter()
                .flat_map(|(file, edits)| edits.iter().map(move |e| (file.clone(), e.span)))
                .collect()
        })
        .unwrap_or_default()
}

/// Work out how to rename `symbol` to `new_name`.
pub fn plan(index: &Index, symbol_id: SymbolId, new_name: &str) -> Result<RenamePlan> {
    if let Some(language) = index.symbol(symbol_id).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::Rename, language);
    }
    let symbol = index
        .symbol(symbol_id)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    validate_name(new_name, symbol.language, symbol.kind)?;

    if new_name == symbol.name {
        anyhow::bail!("'{new_name}' is already the name of this symbol.");
    }

    check_collision(index, symbol, new_name)?;
    check_capture(index, symbol, new_name)?;

    let mut edits = EditSet::new();
    let mut warnings = Vec::new();

    // Some entities have several definition sites, such as a CSS class declared by both `.btn`
    // and `.btn:hover`, and all of them change together.
    let mut group = index.definition_group(symbol_id);
    let hierarchy = crate::analysis::call_graph::Hierarchy::scanned(index);
    let family = hierarchy.method_group(index, symbol_id);
    let dispatched = !family.is_empty();
    // An instance attribute is one entity across its class chain the same way a
    // method is one across its dispatch family.
    let attribute_family = hierarchy.field_group(index, symbol_id);
    for member in family.into_iter().chain(attribute_family) {
        group.extend(index.definition_group(member));
    }
    group.sort();
    group.dedup();
    for id in &group {
        let Some(definition) = index.symbol(*id) else {
            continue;
        };
        edits.add(
            definition.file.clone(),
            Edit::new(
                definition.name_span,
                new_name,
                format!("rename definition of {}", definition.name),
            ),
        );
    }

    // Go's doc-comment convention heads the comment above a declaration with the declared name.
    for id in &group {
        if let Some(edit) = go_doc_head_edit(index, *id, new_name) {
            let (file, head) = edit;
            edits.add(file, head);
        }
    }

    // A heading's references are `#anchor` links, and an anchor is a slug of the heading
    // instead of the heading itself.
    let reference_text = match symbol.kind {
        SymbolKind::Heading => anchor_slug(new_name),
        _ => new_name.to_string(),
    };

    // References that resolved strongly enough to rewrite.
    let overload_peers = group
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.name == symbol.name)
        .count();
    let group_answers_alone = overload_peers > 1
        && index
            .find_symbols(&symbol.name, None)
            .iter()
            .all(|s| group.contains(&s.id));

    // The types that can reach this entity through a declared receiver: the group's own
    // containers and everything below them.
    let owners: std::collections::BTreeSet<String> = {
        let mut owners: std::collections::BTreeSet<String> = group
            .iter()
            .filter_map(|id| index.symbol(*id))
            .filter_map(|s| s.qualifier.clone())
            .collect();
        if let Some(family) = crate::analysis::call_graph::Family::of(symbol.language) {
            let subs: Vec<String> = owners
                .iter()
                .flat_map(|owner| hierarchy.subtypes_of(family, owner))
                .collect();
            owners.extend(subs);
        }
        owners
    };
    let declared_receiver_reaches = |reference: &crate::model::Reference| {
        matches!(reference.kind, ReferenceKind::Field | ReferenceKind::Call)
            && super::receiver_known_type(index, reference)
                .is_some_and(|declared| owners.contains(&declared))
    };

    let mut reference_edits = 0;
    let mut seen_spans = std::collections::HashSet::new();
    for id in &group {
        for reference in index.references_to(*id) {
            if !seen_spans.insert((reference.file.clone(), reference.span)) {
                continue;
            }
            if reference.confidence.is_safe_to_rewrite() || declared_receiver_reaches(reference) {
                // `Facts { count }` reads the local and writes the field in one identifier, so
                // rewriting that identifier renames both.
                let written = match shorthand_field(&reference.file, reference.span, &symbol.name) {
                    Some(old) if reference.kind == ReferenceKind::Field => {
                        format!("{reference_text}: {old}")
                    }
                    Some(old) => format!("{old}: {reference_text}"),
                    None => reference_text.clone(),
                };
                edits.add(
                    reference.file.clone(),
                    Edit::new(
                        reference.span,
                        written,
                        format!("rename reference to {}", symbol.name),
                    ),
                );
                reference_edits += 1;
            } else if group_answers_alone && reference.kind == ReferenceKind::Call {
                edits.add(
                    reference.file.clone(),
                    Edit::new(
                        reference.span,
                        reference_text.as_str(),
                        format!("rename overloaded call to {}", symbol.name),
                    ),
                );
                reference_edits += 1;
                warnings.push(locate_warning(
                    WarningKind::DispatchCandidate,
                    &reference.file,
                    reference.span.start,
                    format!(
                        "renamed with the overloads: only a declaration of '{}' answers \
                         this call, and every one of them renames here.",
                        symbol.name
                    ),
                ));
            } else {
                warnings.push(locate_warning(
                    WarningKind::WeaklyResolved,
                    &reference.file,
                    reference.span.start,
                    format!(
                        "left unchanged: {} (resolved only as '{}')",
                        why_it_was_left(index, reference),
                        reference.confidence.as_str()
                    ),
                ));
            }
        }
    }

    // Dispatch reaches call sites that never resolve: `s.area()` on a trait object names no
    // single implementation, so the family renames as a unit.
    if dispatched {
        let family_of = crate::analysis::call_graph::Family::of;
        // The types the family belongs to.
        for reference in index.unresolved_matching(symbol_id) {
            if reference.target.is_some() {
                continue;
            }
            let member_shaped =
                matches!(reference.kind, ReferenceKind::Field | ReferenceKind::Call)
                    && reference.confidence == Confidence::FieldBased;
            if !member_shaped || family_of(reference.language) != family_of(symbol.language) {
                continue;
            }
            if let Some(declared) = super::receiver_declared_type(index, reference) {
                if !owners.is_empty() && !owners.contains(&declared) {
                    continue;
                }
            }
            if !seen_spans.insert((reference.file.clone(), reference.span)) {
                continue;
            }
            edits.add(
                reference.file.clone(),
                Edit::new(
                    reference.span,
                    reference_text.as_str(),
                    format!("rename dispatch site of {}", symbol.name),
                ),
            );
            reference_edits += 1;
            warnings.push(locate_warning(
                WarningKind::DispatchCandidate,
                &reference.file,
                reference.span.start,
                format!(
                    "renamed with the family: dispatch from here can reach '{}'.",
                    symbol.name
                ),
            ));
        }
    }

    // A weak member reference whose receiver's declared type owns the renamed entity.
    for reference in index.unresolved_matching(symbol_id) {
        if reference.confidence.is_safe_to_rewrite() {
            continue;
        }
        if !matches!(reference.kind, ReferenceKind::Field | ReferenceKind::Call) {
            continue;
        }
        let Some(declared) = super::receiver_known_type(index, reference) else {
            continue;
        };
        if !owners.contains(&declared) {
            continue;
        }
        let strangers = index
            .find_symbols(&symbol.name, None)
            .iter()
            .any(|s| s.qualifier.as_deref() == Some(declared.as_str()) && !group.contains(&s.id));
        if strangers {
            continue;
        }
        if !seen_spans.insert((reference.file.clone(), reference.span)) {
            continue;
        }
        edits.add(
            reference.file.clone(),
            Edit::new(
                reference.span,
                reference_text.as_str(),
                format!("rename declared-receiver site of {}", symbol.name),
            ),
        );
        reference_edits += 1;
        warnings.push(locate_warning(
            WarningKind::DispatchCandidate,
            &reference.file,
            reference.span.start,
            format!(
                "renamed: its receiver is declared `{declared}`, and what `{declared}` \
                 answers here is being renamed.",
            ),
        ));
    }

    // Same-named references that resolved somewhere else, or nowhere.
    for reference in index.unresolved_matching(symbol_id) {
        let confidently_elsewhere =
            reference.target.is_some() && reference.confidence.is_safe_to_rewrite();
        if !confidently_elsewhere && !seen_spans.contains(&(reference.file.clone(), reference.span))
        {
            warnings.push(locate_warning(
                WarningKind::WeaklyResolved,
                &reference.file,
                reference.span.start,
                format!(
                    "occurrence of '{}' left unchanged: {} (resolved only as '{}')",
                    symbol.name,
                    why_it_was_left(index, reference),
                    reference.confidence.as_str()
                ),
            ));
        }
    }

    // Strings and comments defeat every analysis, so report every hit except the ones already
    // being rewritten.
    let edited: Vec<(PathBuf, Span)> = edits
        .iter()
        .flat_map(|(path, file_edits)| file_edits.iter().map(|e| (path.clone(), e.span)))
        .collect();
    warnings.extend(textual_sweep(index, &symbol.name, &edited)?);

    // Whatever did not reach the index may be hiding references.
    for (path, info) in index.files() {
        for gap in &info.gaps {
            warnings.push(Warning {
                kind: WarningKind::IncompleteFacts,
                file: path.clone(),
                line: 1,
                col: 1,
                detail: format!("{}; references in it may be missing", gap.cause()),
            });
        }
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    Ok(RenamePlan {
        old_name: symbol.name.clone(),
        new_name: new_name.to_string(),
        edits,
        warnings,
        reference_edits,
    })
}

/// Reject names that are not valid identifiers for the language.
fn validate_name(name: &str, language: Language, kind: SymbolKind) -> Result<(), Refusal> {
    if name.is_empty() {
        return Err(Refusal::InvalidName {
            name: name.into(),
            reason: "a name cannot be empty".into(),
        });
    }

    // Config and markup languages allow far more in a name: CSS classes take dashes and YAML
    // keys take almost anything.
    let strict = kind != SymbolKind::DataAttribute
        && matches!(
            language,
            Language::Rust
                | Language::Go
                | Language::Zig
                | Language::TypeScript
                | Language::Tsx
                | Language::Python
                | Language::Bash
        );

    if strict {
        let mut chars = name.chars();
        let first = chars.next().expect("checked non-empty above");
        if !(first.is_alphabetic() || first == '_') {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: "must start with a letter or underscore".into(),
            });
        }
        if let Some(bad) = chars.find(|c| !(c.is_alphanumeric() || *c == '_')) {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: format!("contains '{bad}', which is not allowed in an identifier"),
            });
        }
        if is_keyword(name, language) {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: format!("'{name}' is a {language} keyword"),
            });
        }
    } else if kind == SymbolKind::Heading {
        // A heading is prose, so the space in `## Getting Started` is ordinary.
        if name.trim().is_empty() || name.contains(['\n', '\r']) {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: "a heading must be one non-blank line".into(),
            });
        }
    } else if name.chars().any(|c| c.is_whitespace()) {
        return Err(Refusal::InvalidName {
            name: name.into(),
            reason: "must not contain whitespace".into(),
        });
    }

    Ok(())
}

/// Reserved words that cannot be used as identifiers.
fn is_keyword(name: &str, language: Language) -> bool {
    const SHARED: &[&str] = &["if", "else", "for", "while", "return", "break", "continue"];
    let specific: &[&str] = match language {
        Language::Rust => &[
            "fn", "let", "mut", "const", "struct", "enum", "impl", "trait", "match", "use", "pub",
            "mod", "self", "Self", "super", "crate", "move", "ref", "static", "type", "where",
            "unsafe", "async", "await", "dyn", "loop", "in", "as",
        ],
        Language::Go => &[
            "func",
            "var",
            "const",
            "type",
            "struct",
            "interface",
            "map",
            "chan",
            "go",
            "defer",
            "select",
            "switch",
            "case",
            "default",
            "package",
            "import",
            "range",
            "fallthrough",
        ],
        Language::Python => &[
            "def", "class", "lambda", "import", "from", "global", "nonlocal", "pass", "raise",
            "try", "except", "finally", "with", "yield", "assert", "del", "None", "True", "False",
            "and", "or", "not", "is", "in", "as", "async", "await", "elif",
        ],
        Language::TypeScript | Language::Tsx => &[
            "function",
            "var",
            "let",
            "const",
            "class",
            "interface",
            "type",
            "enum",
            "import",
            "export",
            "default",
            "extends",
            "implements",
            "new",
            "this",
            "super",
            "typeof",
            "instanceof",
            "void",
            "null",
            "undefined",
            "async",
            "await",
            "yield",
            "switch",
            "case",
            "try",
            "catch",
            "finally",
            "throw",
            "delete",
            "in",
            "of",
        ],
        Language::Zig => &[
            "fn",
            "var",
            "const",
            "pub",
            "struct",
            "enum",
            "union",
            "error",
            "try",
            "catch",
            "defer",
            "comptime",
            "inline",
            "test",
            "switch",
            "orelse",
            "unreachable",
            "and",
            "or",
        ],
        Language::Bash => &["function", "then", "fi", "do", "done", "case", "esac", "in"],
        _ => &[],
    };
    SHARED.contains(&name) || specific.contains(&name)
}

/// Refuse when the new name is already defined where the renamed symbol is visible.
fn check_collision(index: &Index, symbol: &Symbol, new_name: &str) -> Result<(), Refusal> {
    let existing = index.find_symbols(new_name, Some(&symbol.file));
    for other in existing {
        // A collision matters when the two could be visible at the same point.
        let same_scope = other.scope == symbol.scope && other.container == symbol.container;
        let either_top_level = other.container.is_none() || symbol.container.is_none();
        if same_scope || (either_top_level && other.kind == symbol.kind) {
            return Err(Refusal::NameCollision {
                existing: new_name.to_string(),
                file: other.file.clone(),
            });
        }
    }
    Ok(())
}

/// The field a shorthand at this span writes, when the span sits in one.
fn shorthand_field(file: &std::path::Path, span: crate::span::Span, name: &str) -> Option<String> {
    const SHORTHANDS: &[&str] = &[
        "shorthand_field_initializer",   // Rust
        "shorthand_property_identifier", // TypeScript, JavaScript
    ];
    let source = crate::vfs::read_to_string(file).ok()?;
    let language = crate::lang::detect(file)?;
    let parsed = crate::parse::Parsers::new().parse(language, &source).ok()?;
    let node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    let hit = SHORTHANDS.contains(&node.kind())
        || node
            .parent()
            .is_some_and(|p| SHORTHANDS.contains(&p.kind()));
    hit.then(|| name.to_string())
}

/// Refuse when the rename would move a use under a different declaration.
fn check_capture(index: &Index, symbol: &Symbol, new_name: &str) -> Result<(), Refusal> {
    let Some(info) = index.file(&symbol.file) else {
        return Ok(());
    };
    let parent_of = |id| {
        info.scopes
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.parent)
    };
    let contains = |outer, inner| {
        let mut at = Some(inner);
        while let Some(id) = at {
            if id == outer {
                return true;
            }
            at = parent_of(id);
        }
        false
    };
    let span_of = |id| info.scopes.iter().find(|s| s.id == id).map(|s| s.span);
    // Only a declaration a bare name resolves to can win a bare use.
    let binds_bare = |kind| {
        matches!(
            kind,
            SymbolKind::Variable
                | SymbolKind::Constant
                | SymbolKind::Parameter
                | SymbolKind::Function
        )
    };
    if !binds_bare(symbol.kind) {
        return Ok(());
    }
    let line_of = |offset| {
        crate::vfs::read_to_string(&symbol.file)
            .map(|source| LineIndex::new(&source).line_col(offset, &source).line)
            .unwrap_or(0)
    };

    for other in index.find_symbols(new_name, Some(&symbol.file)) {
        if other.file != symbol.file || other.scope == symbol.scope || !binds_bare(other.kind) {
            continue;
        }
        // A renamed use inside the inner declaration's scope falls to it.
        if contains(symbol.scope, other.scope) {
            if let Some(span) = span_of(other.scope) {
                for reference in index.references_to(symbol.id) {
                    if reference.file == symbol.file
                        && reference.receiver.is_none()
                        && reference.confidence.is_safe_to_rewrite()
                        && span.contains_offset(reference.span.start)
                    {
                        return Err(Refusal::ScopeCaptured {
                            name: new_name.to_string(),
                            file: symbol.file.clone(),
                            line: line_of(reference.span.start),
                            detail: format!(
                                "a declaration of `{new_name}` encloses this renamed use of \
                                 `{}` and would win it",
                                symbol.name
                            ),
                        });
                    }
                }
            }
        }
        // A use of the existing outer binding inside the renamed symbol's scope
        // falls to the renamed declaration.
        if contains(other.scope, symbol.scope) {
            if let Some(span) = span_of(symbol.scope) {
                for reference in index.references_to(other.id) {
                    if reference.file == symbol.file
                        && reference.receiver.is_none()
                        && span.contains_offset(reference.span.start)
                    {
                        return Err(Refusal::ScopeCaptured {
                            name: new_name.to_string(),
                            file: symbol.file.clone(),
                            line: line_of(reference.span.start),
                            detail: format!(
                                "the renamed `{}` would enclose this use of the existing \
                                 `{new_name}` and win it",
                                symbol.name
                            ),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// The edit renaming the head word of a Go definition's doc comment, if it is due.
fn go_doc_head_edit(index: &Index, id: SymbolId, new_name: &str) -> Option<(PathBuf, Edit)> {
    let symbol = index.symbol(id)?;
    if symbol.language != Language::Go {
        return None;
    }
    let source = crate::vfs::read_to_string(&symbol.file).ok()?;
    let definition_line_start = source[..symbol.full_span.start]
        .rfind('\n')
        .map(|at| at + 1)
        .unwrap_or(0);

    // Climb through the contiguous `//` block that touches the definition.
    let mut block_start = None;
    let mut at = definition_line_start;
    while at > 0 {
        let previous_start = source[..at - 1].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line = &source[previous_start..at - 1];
        if !line.trim_start().starts_with("//") {
            break;
        }
        block_start = Some(previous_start);
        at = previous_start;
    }
    let first_line_start = block_start?;
    let line_end = source[first_line_start..]
        .find('\n')
        .map(|i| first_line_start + i)
        .unwrap_or(source.len());
    let line = &source[first_line_start..line_end];

    let slashes = line.find("//")?;
    let after = &line[slashes + 2..];
    let word_offset = after.len() - after.trim_start().len();
    let word = &after[word_offset..];
    let head_len = word
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(word.len());
    if word[..head_len] != symbol.name {
        return None;
    }
    let start = first_line_start + slashes + 2 + word_offset;
    Some((
        symbol.file.clone(),
        Edit::new(
            Span::new(start, start + head_len),
            new_name,
            format!("rename doc comment head of {}", symbol.name),
        ),
    ))
}

/// Find the old name inside string literals and comments across the workspace.
fn textual_sweep(
    index: &Index,
    name: &str,
    already_edited: &[(PathBuf, Span)],
) -> Result<Vec<Warning>> {
    Ok(crate::mentions::of(index, name)?
        .into_iter()
        // An occurrence this rename already rewrites needs no warning.
        .filter(|m| {
            !already_edited
                .iter()
                .any(|(file, edited)| file == &m.file && edited.overlaps(m.span))
        })
        .map(|m| Warning {
            kind: WarningKind::TextualOccurrence,
            file: m.file,
            line: m.line,
            col: m.col,
            detail: format!(
                "'{name}' is written here as text, with nothing linking it to the \
                 declaration; left unchanged"
            ),
        })
        .collect())
}

fn why_it_was_left(index: &Index, reference: &crate::model::Reference) -> String {
    // A Terraform module argument names a variable of the configuration `source` points at.
    if let Some(call) = reference
        .receiver
        .as_deref()
        .filter(|_| reference.language == Language::Hcl)
        .and_then(|receiver| receiver.strip_prefix("module."))
    {
        return format!(
            "it is an argument of `module \"{call}\"`, whose source names no directory \
             in this workspace"
        );
    }
    if reference.receiver.is_some() && !reference.receiver_is_path {
        // A receiver whose declaration names its type gets the real reason: the named type
        // belongs to something other than the renamed symbol.
        match super::receiver_type(index, reference) {
            super::ReceiverType::Settled(declared) => {
                return format!(
                    "its receiver is declared `{declared}`, which is not what is being renamed"
                );
            }
            super::ReceiverType::Reassigned => {
                let receiver = reference.receiver.as_deref().unwrap_or("the receiver");
                return format!(
                    "`{receiver}` is assigned more than once here, so what it holds at \
                     this line is not settled"
                );
            }
            super::ReceiverType::Unwritten => {}
        }
        return "it is read from a value whose type is not known here, so it may name \
                something else of the same name"
            .to_string();
    }
    if reference.member_in_macro {
        return "it is written inside a macro, where the receiver is not recorded".to_string();
    }
    "it matched by name alone".to_string()
}

/// A warning placed at the line and column `offset` falls on in `file`.
fn locate_warning(kind: WarningKind, file: &PathBuf, offset: usize, detail: String) -> Warning {
    let (line, col) = match crate::vfs::read_to_string(file) {
        Ok(source) => {
            let pos = LineIndex::new(&source).line_col(offset, &source);
            (pos.line, pos.col)
        }
        Err(_) => (0, 0),
    };
    Warning {
        kind,
        file: file.clone(),
        line,
        col,
        detail,
    }
}

/// Group warnings by kind for reporting.
pub fn group_warnings(warnings: &[Warning]) -> HashMap<&'static str, Vec<&Warning>> {
    let mut grouped: HashMap<&'static str, Vec<&Warning>> = HashMap::new();
    for w in warnings {
        grouped.entry(w.kind.as_str()).or_default().push(w);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{ScanResult, SourceFile};

    use crate::testing::indexed as workspace;

    fn only_symbol(index: &Index, name: &str) -> SymbolId {
        let found = index.find_symbols(name, None);
        assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
        found[0].id
    }

    #[test]
    fn a_go_doc_comments_head_word_renames_with_its_definition() {
        // The gofmt convention heads the comment above a declaration with its name, so that one
        // occurrence is unambiguous and edits instead of warning.
        let (tmp, index) = workspace(&[(
            "a.go",
            "package p\n\n// Helper builds the Helper report.\n// Helper is safe.\n\
             func Helper() int {\n\treturn 1\n}\n\nfunc use() int {\n\treturn Helper()\n}\n",
        )]);
        let plan = plan(&index, only_symbol(&index, "Helper"), "Builder").unwrap();
        let path = tmp.path().join("a.go");
        let source = crate::vfs::read_to_string(&path).unwrap();
        let out = apply_to_string(&source, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert!(
            out.contains("// Builder builds the Helper report."),
            "the head word renames, the prose stays: {out}"
        );
        assert!(
            out.contains("// Helper is safe."),
            "later comment lines still only warn: {out}"
        );
        assert!(out.contains("func Builder() int"), "{out}");
    }

    #[test]
    fn a_comment_that_does_not_head_with_the_name_still_only_warns() {
        let (tmp, index) = workspace(&[(
            "a.go",
            "package p\n\n// Builds things with Helper.\nfunc Helper() int {\n\treturn 1\n}\n",
        )]);
        let plan = plan(&index, only_symbol(&index, "Helper"), "Builder").unwrap();
        let path = tmp.path().join("a.go");
        let source = crate::vfs::read_to_string(&path).unwrap();
        let out = apply_to_string(&source, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert!(
            out.contains("// Builds things with Helper."),
            "a mid-sentence mention is not the doc head: {out}"
        );
    }

    #[test]
    fn renames_definition_and_all_exact_references() {
        let src = "fn helper() {}\nfn main() { helper(); helper(); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();

        assert_eq!(plan.reference_edits, 2);
        let path = tmp.path().join("a.rs");
        let edits = plan.edits.edits_for(&path).unwrap();
        let out = apply_to_string(src, edits).unwrap();
        assert_eq!(out, "fn assist() {}\nfn main() { assist(); assist(); }\n");
    }

    #[test]
    fn rename_only_touches_the_identifier_not_the_line() {
        // Everything around the identifier must be preserved byte-for-byte.
        let src = "fn  helper ( ) { } // helper stays in this comment\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "x").unwrap();
        let path = tmp.path().join("a.rs");
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert_eq!(out, "fn  x ( ) { } // helper stays in this comment\n");
    }

    #[test]
    fn does_not_rename_same_name_in_another_file() {
        // Two independent `parse` functions: renaming one must not touch the other.
        let (tmp, index) = workspace(&[
            ("a.rs", "fn parse() {}\nfn a() { parse(); }\n"),
            ("b.rs", "fn parse() {}\nfn b() { parse(); }\n"),
        ]);
        let a_path = tmp.path().join("a.rs");
        let target = index
            .find_symbols("parse", Some(&a_path))
            .first()
            .unwrap()
            .id;

        let plan = plan(&index, target, "decode").unwrap();
        assert!(plan.edits.edits_for(&a_path).is_some());
        assert!(
            plan.edits.edits_for(&tmp.path().join("b.rs")).is_none(),
            "must not edit the other file's identically-named function"
        );
    }

    #[test]
    fn refuses_to_collide_with_an_existing_name() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\nfn beta() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "beta").unwrap_err();
        let refusal = err.downcast_ref::<Refusal>().expect("a refusal");
        assert!(
            matches!(refusal, Refusal::NameCollision { .. }),
            "{refusal}"
        );
    }

    #[test]
    fn refuses_invalid_identifiers() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let id = only_symbol(&index, "alpha");

        for bad in ["", "2fast", "has space", "has-dash"] {
            let err = plan(&index, id, bad).unwrap_err();
            assert!(
                err.downcast_ref::<Refusal>()
                    .is_some_and(|r| matches!(r, Refusal::InvalidName { .. })),
                "expected InvalidName for {bad:?}, got {err}"
            );
        }
    }

    #[test]
    fn refuses_language_keywords() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "impl").unwrap_err();
        assert!(err.to_string().contains("keyword"), "got: {err}");
    }

    #[test]
    fn refuses_a_no_op_rename() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "alpha").unwrap_err();
        assert!(err.to_string().contains("already the name"), "got: {err}");
    }

    #[test]
    fn reports_occurrences_in_strings_and_comments_without_editing_them() {
        let src =
            "// helper does things\nfn helper() {}\nfn main() { let s = \"call helper now\"; }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();

        let textual: Vec<_> = plan
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::TextualOccurrence)
            .collect();
        assert_eq!(textual.len(), 2, "got {textual:?}");

        // The strings and comments must survive untouched.
        let path = tmp.path().join("a.rs");
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert!(out.contains("// helper does things"));
        assert!(out.contains("\"call helper now\""));
        assert!(out.contains("fn assist()"));
    }

    #[test]
    fn textual_sweep_respects_word_boundaries() {
        // `helperful` contains `helper` but is a different word.
        let src = "fn helper() {}\n// helperful and helper_x are not matches, helper is\n";
        let (_tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();
        let textual: Vec<_> = plan
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::TextualOccurrence)
            .collect();
        assert_eq!(textual.len(), 1, "got {textual:?}");

        // Which one matters.
        let found = textual[0];
        let column = src
            .lines()
            .nth(found.line - 1)
            .map(|line| &line[found.col - 1..])
            .unwrap_or_default();
        assert!(
            column.starts_with("helper is"),
            "the textual sweep matched `{}` instead of the standalone word",
            column.split_whitespace().next().unwrap_or("")
        );
    }

    #[test]
    fn shadowed_variable_rename_stays_in_its_scope() {
        let src = "fn f() {\n    let x = 1;\n    {\n        let x = 2;\n        g(x);\n    }\n    h(x);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        // Target the INNER x by position.
        let inner_offset = src.rfind("let x").unwrap() + 4;
        let inner = index.definition_at(&path, inner_offset).unwrap();
        let plan = plan(&index, inner.id, "y").unwrap();
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();

        // g(x) used the inner binding and must change; h(x) used the outer one.
        assert!(out.contains("let y = 2;"), "got:\n{out}");
        assert!(out.contains("g(y)"), "inner use should be renamed:\n{out}");
        assert!(out.contains("h(x)"), "outer use must be untouched:\n{out}");
        assert!(
            out.contains("let x = 1;"),
            "outer binding untouched:\n{out}"
        );
    }

    #[test]
    fn rename_is_reversible() {
        // A→B→A must return the original bytes exactly.
        let src = "fn alpha() {}\nfn main() { alpha(); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let forward = plan(&index, only_symbol(&index, "alpha"), "renamed").unwrap();
        let once = apply_to_string(src, forward.edits.edits_for(&path).unwrap()).unwrap();
        crate::vfs::write(&path, &once).unwrap();

        let mut scanned = ScanResult::default();
        scanned.files.push(SourceFile {
            path: path.clone(),
            language: Language::Rust,
        });
        let index2 = Index::build_from_scan(&scanned).unwrap();
        let back = plan(&index2, only_symbol(&index2, "renamed"), "alpha").unwrap();
        let twice = apply_to_string(&once, back.edits.edits_for(&path).unwrap()).unwrap();

        assert_eq!(twice, src);
    }

    #[test]
    fn warns_about_files_that_failed_to_parse() {
        let (_tmp, index) =
            workspace(&[("a.rs", "fn alpha() {}\n"), ("broken.rs", "fn oops( {\n")]);
        let plan = plan(&index, only_symbol(&index, "alpha"), "beta").unwrap();
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.kind == WarningKind::IncompleteFacts),
            "a file with syntax errors may hide references and must be reported"
        );
    }

    #[test]
    fn config_languages_allow_names_identifiers_would_reject() {
        // A CSS class may contain dashes, which is not a valid Rust identifier.
        assert!(validate_name("my-class", Language::Css, SymbolKind::Selector).is_ok());
        assert!(validate_name("my-class", Language::Rust, SymbolKind::Function).is_err());
        // Whitespace is never acceptable.
        assert!(validate_name("two words", Language::Css, SymbolKind::Selector).is_err());
    }
}
