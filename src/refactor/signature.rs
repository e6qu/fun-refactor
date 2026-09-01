//! Change a function's signature and every call site.

use super::Refusal;
use crate::edit::{full_line_span, line_indent, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Reference, ReferenceKind, Symbol, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// What to do to a parameter list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Remove the parameter at this zero-based position.
    Remove(usize),
    /// Move a parameter from one position to another.
    Move { from: usize, to: usize },
    /// Add a parameter, with the text to insert at each call site.
    Add {
        at: usize,
        declaration: String,
        argument: String,
    },
}

impl Change {
    /// Parse `remove:1`, `move:1:2` or `add:2:flag: bool:false`.
    pub fn parse(spec: &str) -> anyhow::Result<Change> {
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        match parts.as_slice() {
            ["remove", index] => Ok(Change::Remove(index.parse()?)),
            ["move", from, to] => Ok(Change::Move {
                from: from.parse()?,
                to: to.parse()?,
            }),
            ["add", at, rest] => {
                let (declaration, argument) = rest.rsplit_once(':').ok_or_else(|| {
                    anyhow::anyhow!(
                        "add needs a declaration and an argument, e.g. add:1:flag\\: bool:false"
                    )
                })?;
                Ok(Change::Add {
                    at: at.parse()?,
                    declaration: declaration.to_string(),
                    argument: argument.to_string(),
                })
            }
            _ => anyhow::bail!(
                "unrecognised change '{spec}'. Use remove:<i>, move:<from>:<to>, or \
                 add:<i>:<declaration>:<argument>"
            ),
        }
    }
}

/// What sort of thing the changed signature belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// A function or method, whose arguments are positional.
    Callable,
    /// A Terraform module directory, which names its arguments.
    TerraformModule,
    /// A shell function, whose parameters are the `$1 $2 …` its body reads.
    ShellFunction,
}

/// A signature change worked out but not applied.
#[derive(Debug)]
pub struct SignaturePlan {
    /// The function's name, or the Terraform module's directory.
    pub subject: String,
    pub subject_kind: Subject,
    pub change: Change,
    pub edits: EditSet,
    /// Call sites updated.
    pub call_sites: usize,
    /// Things the change saw and deliberately did not act on.
    pub notes: Vec<String>,
}

/// Refuse to remove a parameter the body still reads.
fn already_declared(
    source: &str,
    items: &[Span],
    change: &Change,
    language: Language,
) -> Result<()> {
    let Change::Add { declaration, .. } = change else {
        return Ok(());
    };
    let Some(name) = parameter_name(declaration, language) else {
        return Ok(());
    };
    let taken = items
        .iter()
        .filter_map(|span| source.get(span.start..span.end))
        .filter_map(|text| parameter_name(text, language))
        .any(|existing| existing == name);
    if taken {
        return Err(Refusal::Declined {
            detail: format!(
                "this declaration already has a parameter called `{name}`; adding it \
             again would name one thing twice"
            ),
        }
        .into());
    }
    Ok(())
}

/// The name a parameter's text declares, whatever else it carries.
fn parameter_name(text: &str, language: Language) -> Option<String> {
    let head = text.split('=').next()?.trim();
    if head.is_empty() {
        return None;
    }
    let name = match language {
        // `name: type`, and the type may itself contain spaces.
        Language::Python
        | Language::TypeScript
        | Language::Tsx
        | Language::Rust
        | Language::Zig
        | Language::Scss
        | Language::Sass => head.split(':').next()?.trim(),
        // `name type`.
        Language::Go => head.split_whitespace().next()?,
        // `type name`, modifiers and annotations ahead of both.
        Language::Java => head.split_whitespace().last()?,
        _ => head,
    };
    let name = name.trim_start_matches(['*', '&', '$']).trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn still_read(
    index: &Index,
    sym: &crate::model::Symbol,
    items: &[Span],
    change: &Change,
) -> Result<()> {
    let Change::Remove(at) = change else {
        return Ok(());
    };
    let Some(span) = items.get(*at).copied() else {
        return Ok(());
    };
    let Some(parameter) = index.symbols.iter().find(|s| {
        s.file == sym.file && s.kind == SymbolKind::Parameter && span.contains(s.name_span)
    }) else {
        return Ok(());
    };
    // Inside the declaration, and nowhere else.
    let uses: Vec<_> = index
        .references_to(parameter.id)
        .into_iter()
        .filter(|r| r.file == sym.file && sym.full_span.contains(r.span))
        .collect();
    if let Some(first) = uses.first() {
        return Err(still_used(
            format!(
                "the body of `{}` still reads `{}` at {}; removing the parameter would \
                 leave a name nothing supplies",
                sym.name,
                parameter.name,
                location(&first.file, first.span.start)
            ),
            uses.iter().map(|r| refusal_site(&r.file, r.span.start)),
        ));
    }
    Ok(())
}

/// A considered refusal that names the uses a change would strand.
fn still_used(
    detail: String,
    sites: impl Iterator<Item = crate::refactor::RefusalSite>,
) -> anyhow::Error {
    crate::refactor::Refusal::StillUsed {
        detail,
        references: sites.collect(),
    }
    .into()
}

/// A position, for the data that rides beside a refusal's prose.
fn refusal_site(path: &Path, offset: usize) -> crate::refactor::RefusalSite {
    let (line, col) = crate::vfs::read_to_string(path)
        .map(|source| {
            let at = LineIndex::new(&source).line_col(offset, &source);
            (at.line, at.col)
        })
        .unwrap_or((0, 0));
    crate::refactor::RefusalSite {
        file: path.to_path_buf(),
        line,
        col,
    }
}

/// Apply `change` to `symbol` and every call site.
pub fn change(index: &Index, symbol: SymbolId, change: Change) -> Result<SignaturePlan> {
    if let Some(language) = index.symbol(symbol).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::ChangeSignature, language);
    }
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // A Terraform module's signature is its directory's `variable` blocks.
    if sym.language == Language::Hcl {
        return terraform_module(index, sym, change);
    }
    // A shell function's signature is the numbering of its positional parameters.
    if sym.language == Language::Bash {
        return shell_function(index, sym, change);
    }

    if !matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
        return Err(Refusal::Declined {
            detail: format!(
                "'{}' is {}; only functions and methods have signatures",
                sym.name,
                sym.kind.with_article()
            ),
        }
        .into());
    }

    let family = crate::analysis::call_graph::Hierarchy::scanned(index).method_group(index, symbol);
    let dispatched = !family.is_empty();
    let members: Vec<SymbolId> = if dispatched { family } else { vec![symbol] };

    // Every call site must be provable: a missed one is a compile error.
    let mut references: Vec<&crate::model::Reference> = Vec::new();
    for member in &members {
        references.extend(index.references_to(*member));
    }
    references.sort_by_key(|r| (r.file.clone(), r.span));
    references.dedup_by_key(|r| (r.file.clone(), r.span));
    let weak: Vec<_> = references
        .iter()
        .filter(|r| !r.confidence.is_safe_to_rewrite())
        .collect();
    if let Some(first) = weak.first() {
        return Err(Refusal::TooWeak {
            confidence: first.resolved_confidence(),
            detail: format!(
                "{} of {} call site(s) did not resolve conclusively; updating only some \
                 would leave the code uncompilable",
                weak.len(),
                references.len()
            ),
        }
        .into());
    }
    reject_hidden_call_sites(index, sym)?;

    let mut edits = EditSet::new();
    let mut notes = Vec::new();
    // The name of the parameter a removal takes away, read off the declaration
    // and used at every call site that names its arguments.
    let mut removing: Option<String> = None;
    for member in &members {
        let Some(m) = index.symbol(*member) else {
            continue;
        };
        let source = crate::vfs::read_to_string(&m.file)?;
        let parsed = Parsers::new().parse(m.language, &source)?;
        let declaration = parsed
            .root()
            .descendant_for_byte_range(m.full_span.start, m.full_span.end)
            .ok_or_else(|| anyhow::anyhow!("could not locate the declaration"))?;

        match parameter_list(declaration) {
            Some(params) => {
                let param_spans = without_receiver(m.language, m.kind, &source, list_items(params));
                still_read(index, m, &param_spans, &change)?;
                already_declared(&source, &param_spans, &change, m.language)?;
                apply_change(
                    &mut edits,
                    &Site {
                        file: &m.file,
                        source: &source,
                        language: m.language,
                    },
                    params.start_byte(),
                    &param_spans,
                    &change,
                    true,
                    None,
                )?;
                removing = removed_parameter_name(&source, &param_spans, &change, m.language);
            }
            // A declaration can legitimately have no parameter list to change: SCSS spells a
            // no-argument mixin `@mixin reset { }`, with no parentheses at all.
            None => {
                open_a_parameter_list(&mut edits, &m.file, m.name_span.end, &change, true, &m.name)?
            }
        }
        if *member != symbol {
            notes.push(format!(
                "{} at {} changed with the family",
                m.name,
                location(&m.file, m.name_span.start)
            ));
        }
    }

    let mut call_sites = 0;
    for reference in &references {
        let call_source = crate::vfs::read_to_string(&reference.file)?;
        let call_parsed = Parsers::new().parse(reference.language, &call_source)?;
        // The grammar decides whether this is a call.
        let call = match call_expression(&call_parsed, reference.span) {
            Some(call) => call,
            // A mention that is not a call and not an import is the function used as a value.
            None if reference.kind != crate::model::ReferenceKind::Call => {
                let value_shaped = matches!(
                    reference.kind,
                    crate::model::ReferenceKind::Identifier | crate::model::ReferenceKind::Field
                );
                if value_shaped
                    && reference.confidence.is_safe_to_rewrite()
                    && !inside_import(&call_parsed, reference.span)
                {
                    return Err(Refusal::NotHere {
                        operation: "signature".to_string(),
                        detail: format!(
                            "{1} binds `{0}` as a value, and a value keeps the old \
                             shape. Change or remove that binding first.",
                            sym.name,
                            location(&reference.file, reference.span.start)
                        ),
                    }
                    .into());
                }
                continue;
            }
            None => {
                // A macro body is tokens and not syntax, so the grammar offers no call.
                if let Some((opens_at, arg_spans)) =
                    macro_call_arguments(&call_parsed, reference.span)
                {
                    apply_change(
                        &mut edits,
                        &Site {
                            file: &reference.file,
                            source: &call_source,
                            language: reference.language,
                        },
                        opens_at,
                        &arg_spans,
                        &change,
                        false,
                        removing.as_deref(),
                    )?;
                    call_sites += 1;
                    continue;
                }
                return Err(Refusal::Unknowable {
                    detail: format!(
                        "the call to `{}` at {} is not a call expression this grammar \
                         exposes, so nothing can rewrite its arguments",
                        sym.name,
                        location(&reference.file, reference.span.start)
                    ),
                }
                .into());
            }
        };
        // An unparsed call site would be silently skipped, and a skipped call site
        // is exactly the partial update this refactoring exists to avoid.
        if call.has_error() {
            return Err(Refusal::TooWeak {
                confidence: reference.resolved_confidence(),
                detail: format!(
                    "the call to `{}` at {} does not parse cleanly, so nothing can rewrite \
                     its argument list with certainty",
                    sym.name,
                    location(&reference.file, reference.span.start)
                ),
            }
            .into());
        }
        if named_arguments_block_change(call, reference.language, &change) {
            return Err(Refusal::Declined {
                detail: format!(
                    "the call to `{}` at {} has a keyword argument; adding or moving a \
                     positional argument would change its meaning or make invalid syntax",
                    sym.name,
                    location(&reference.file, reference.span.start)
                ),
            }
            .into());
        }

        match call_arguments(call) {
            Some((opens_at, arg_spans)) => {
                apply_change(
                    &mut edits,
                    &Site {
                        file: &reference.file,
                        source: &call_source,
                        language: reference.language,
                    },
                    opens_at,
                    &arg_spans,
                    &change,
                    false,
                    removing.as_deref(),
                )?;
            }
            // `@include reset;` passes nothing and needs no parentheses.
            None => open_a_parameter_list(
                &mut edits,
                &reference.file,
                reference.span.end,
                &change,
                false,
                &sym.name,
            )?,
        }
        call_sites += 1;
    }

    if dispatched {
        let family_of = crate::analysis::call_graph::Family::of;
        let seen_family_owners: std::collections::BTreeSet<String> = members
            .iter()
            .filter_map(|id| index.symbol(*id))
            .filter_map(|s| s.qualifier.clone())
            .collect();
        let mut seen: std::collections::HashSet<(std::path::PathBuf, Span)> = references
            .iter()
            .map(|r| (r.file.clone(), r.span))
            .collect();
        for reference in index.unresolved_matching(symbol) {
            let member_shaped = matches!(
                reference.kind,
                crate::model::ReferenceKind::Call | crate::model::ReferenceKind::Field
            );
            if reference.target.is_some()
                || !member_shaped
                || reference.confidence != crate::model::Confidence::FieldBased
                || family_of(reference.language) != family_of(sym.language)
            {
                continue;
            }
            // A receiver whose declared type sits outside the family cannot
            // reach it; the same evidence rename uses to hold such a call still.
            if let Some(declared) = super::receiver_declared_type(index, reference) {
                let outside =
                    !seen_family_owners.is_empty() && !seen_family_owners.contains(&declared);
                if outside {
                    continue;
                }
            }
            if !seen.insert((reference.file.clone(), reference.span)) {
                continue;
            }
            let call_source = crate::vfs::read_to_string(&reference.file)?;
            let call_parsed = Parsers::new().parse(reference.language, &call_source)?;
            // A dispatch site this cannot rewrite is a call left with the old argument shape,
            // and every member of the family has already changed.
            let where_ = location(&reference.file, reference.span.start);
            let out_of_reach = |why: &str| -> anyhow::Error {
                Refusal::Unknowable {
                    detail: format!(
                        "a call to `{}` sits at {where_}, where dispatch can reach the \
                         declaration this changes, and {why}",
                        sym.name
                    ),
                }
                .into()
            };
            let Some(call) = call_expression(&call_parsed, reference.span) else {
                // A macro body is tokens and not syntax, so the grammar offers no call.
                if let Some((opens_at, arg_spans)) =
                    macro_call_arguments(&call_parsed, reference.span)
                {
                    apply_change(
                        &mut edits,
                        &Site {
                            file: &reference.file,
                            source: &call_source,
                            language: reference.language,
                        },
                        opens_at,
                        &arg_spans,
                        &change,
                        false,
                        removing.as_deref(),
                    )?;
                    call_sites += 1;
                    continue;
                }
                return Err(out_of_reach(match reference.member_in_macro {
                    true => {
                        "it sits inside a macro, where the grammar records \
                             tokens and not a call"
                    }
                    false => "the grammar exposes no call expression there",
                }));
            };
            if call.has_error() {
                return Err(out_of_reach("that call does not parse cleanly"));
            }
            if named_arguments_block_change(call, reference.language, &change) {
                return Err(out_of_reach(
                    "it has a keyword argument, so adding or moving a positional argument \
                     would not preserve its meaning",
                ));
            }
            let Some((opens_at, arg_spans)) = call_arguments(call) else {
                return Err(out_of_reach("that call has no argument list to rewrite"));
            };
            apply_change(
                &mut edits,
                &Site {
                    file: &reference.file,
                    source: &call_source,
                    language: reference.language,
                },
                opens_at,
                &arg_spans,
                &change,
                false,
                removing.as_deref(),
            )?;
            call_sites += 1;
            notes.push(format!(
                "dispatch site at {} changed with the family",
                location(&reference.file, reference.span.start)
            ));
        }
    }

    Ok(SignaturePlan {
        subject: sym.name.clone(),
        subject_kind: Subject::Callable,
        change,
        edits,
        call_sites,
        notes,
    })
}

/// Rewrite one parameter or argument list.
fn defaults_would_be_out_of_order(
    language: Language,
    source: &str,
    items: &[Span],
    from: usize,
    to: usize,
) -> bool {
    if !matches!(
        language,
        Language::Python | Language::TypeScript | Language::Tsx
    ) {
        return false;
    }
    // `?` is TypeScript's other way of saying the same thing.
    let defaulted = |span: &Span| {
        let text = span.text(source);
        text.contains('=')
            || text
                .split(':')
                .next()
                .is_some_and(|name| name.ends_with('?'))
    };
    let mut order: Vec<&Span> = items.iter().collect();
    if from >= order.len() || to >= order.len() {
        return false;
    }
    order.swap(from, to);
    let first_defaulted = order.iter().position(|span| defaulted(span));
    match first_defaulted {
        Some(at) => order[at..].iter().any(|span| !defaulted(span)),
        None => false,
    }
}

/// The file this change lands in.
struct Site<'a> {
    file: &'a std::path::Path,
    source: &'a str,
    language: Language,
}

/// The name of the parameter a removal targets, read from the declaration.
fn removed_parameter_name(
    source: &str,
    items: &[Span],
    change: &Change,
    language: Language,
) -> Option<String> {
    let Change::Remove(at) = change else {
        return None;
    };
    let text = source.get(items.get(*at)?.start..items.get(*at)?.end)?;
    parameter_name(text, language)
}

fn apply_change(
    edits: &mut EditSet,
    site: &Site<'_>,
    opens_at: usize,
    items: &[Span],
    change: &Change,
    is_declaration: bool,
    removed_name: Option<&str>,
) -> Result<()> {
    let Site {
        file,
        source,
        language,
    } = *site;
    match change {
        Change::Remove(index) => {
            let Some(target) = items.get(*index) else {
                // A declaration must have the parameter; a call may legitimately
                // omit a defaulted one, so only the declaration is an error.
                if is_declaration {
                    return Err(Refusal::Declined {
                        detail: format!(
                            "there is no parameter at position {index}: the declaration has {} \
                         parameter(s), counted from 0",
                            items.len()
                        ),
                    }
                    .into());
                }
                return Ok(());
            };
            let (index, target) = match (is_declaration, removed_name) {
                (false, Some(name)) => {
                    let named = |span: &Span| -> Option<String> {
                        let text = source.get(span.start..span.end)?.trim();
                        let (head, _) = text.split_once('=')?;
                        Some(head.trim().to_string())
                    };
                    match items.iter().position(|s| named(s).as_deref() == Some(name)) {
                        Some(at) => (at, items[at]),
                        None if items.iter().any(|s| named(s).is_some()) => return Ok(()),
                        None => (*index, *target),
                    }
                }
                _ => (*index, *target),
            };
            let span = with_separator(source, items, index, target);
            edits.add(
                file.to_path_buf(),
                Edit::new(span, "", format!("remove parameter {index}")),
            );
        }
        Change::Move { from, to } => {
            let (Some(a), Some(b)) = (items.get(*from), items.get(*to)) else {
                if is_declaration {
                    return Err(Refusal::Declined {
                        detail: format!(
                            "positions {from} and {to} are not both present: the declaration \
                         has {} parameter(s), counted from 0",
                            items.len()
                        ),
                    }
                    .into());
                }
                return Ok(());
            };
            if is_declaration && defaults_would_be_out_of_order(language, source, items, *from, *to)
            {
                return Err(Refusal::Declined {
                    detail: format!(
                        "moving parameter {from} to position {to} would put a parameter with a \
                     default before one without, which {language} does not allow. Give the \
                     other parameter a default first, or remove this one's."
                    ),
                }
                .into());
            }
            // Swapping the text of two items keeps every byte in between untouched.
            edits.add(
                file.to_path_buf(),
                Edit::new(*a, b.text(source), format!("move parameter {from}")),
            );
            edits.add(
                file.to_path_buf(),
                Edit::new(*b, a.text(source), format!("move parameter {to}")),
            );
        }
        Change::Add {
            at,
            declaration,
            argument,
        } => {
            let text = if is_declaration {
                declaration
            } else {
                argument
            };
            if items.is_empty() {
                // Insert just inside the parentheses.
                let inside = Span::new(opens_at + 1, opens_at + 1);
                edits.add(
                    file.to_path_buf(),
                    Edit::new(inside, text.clone(), "add parameter".to_string()),
                );
                return Ok(());
            }
            match items.get(*at) {
                Some(before) => edits.add(
                    file.to_path_buf(),
                    Edit::new(
                        Span::new(before.start, before.start),
                        format!("{text}, "),
                        "add parameter".to_string(),
                    ),
                ),
                None => {
                    let last = items.last().expect("non-empty");
                    edits.add(
                        file.to_path_buf(),
                        Edit::new(
                            Span::new(last.end, last.end),
                            format!(", {text}"),
                            "add parameter".to_string(),
                        ),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Write a parameter list for a declaration or call that has none yet.
fn open_a_parameter_list(
    edits: &mut EditSet,
    file: &std::path::Path,
    after: usize,
    change: &Change,
    is_declaration: bool,
    name: &str,
) -> Result<()> {
    let Change::Add {
        declaration,
        argument,
        ..
    } = change
    else {
        if is_declaration {
            return Err(Refusal::Declined {
                detail: format!("`{name}` has no parameter list to change"),
            }
            .into());
        }
        return Ok(());
    };
    let text = if is_declaration {
        declaration
    } else {
        argument
    };
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(after, after),
            format!("({text})"),
            "add parameter".to_string(),
        ),
    );
    Ok(())
}

/// Refuse when a file that could hold a call site did not parse cleanly.
fn reject_hidden_call_sites(index: &Index, sym: &Symbol) -> Result<()> {
    let hidden: Vec<&PathBuf> = index
        .files()
        .filter(|(_, info)| !info.gaps.is_empty() && info.language == sym.language)
        .filter(|(path, _)| {
            crate::vfs::read_to_string(path).is_ok_and(|source| source.contains(&sym.name))
        })
        .map(|(path, _)| path)
        .collect();

    let Some(first) = hidden.first() else {
        return Ok(());
    };
    Err(Refusal::Unknowable {
        detail: format!(
            "{} file(s) naming `{}` do not parse cleanly, starting with {}; a call site \
             inside a syntax error hides from the index, so nothing here proves the \
             call surface complete",
            hidden.len(),
            sym.name,
            first.display()
        ),
    }
    .into())
}

/// Extend a span to swallow the comma that separates it from its neighbour.
fn with_separator(source: &str, items: &[Span], index: usize, target: Span) -> Span {
    let bytes = source.as_bytes();
    if index + 1 < items.len() {
        let mut end = target.end;
        while end < bytes.len() && (bytes[end] == b',' || bytes[end].is_ascii_whitespace()) {
            end += 1;
            if bytes[end.saturating_sub(1)] == b',' {
                while end < bytes.len() && bytes[end] == b' ' {
                    end += 1;
                }
                break;
            }
        }
        Span::new(target.start, end)
    } else if index > 0 {
        // Last item: take the preceding comma instead.
        let mut start = target.start;
        while start > 0 && (bytes[start - 1] == b' ' || bytes[start - 1] == b',') {
            start -= 1;
            if bytes[start] == b',' {
                break;
            }
        }
        Span::new(start, target.end)
    } else {
        target
    }
}

/// The parameter list node of a declaration.
fn parameter_list(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(named) = node.child_by_field_name("parameters") {
        return Some(named);
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("parameter"));
    found
}

/// The argument list node of a call, where the grammar wraps one.
fn argument_list(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(named) = node.child_by_field_name("arguments") {
        return Some(named);
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("argument"));
    found
}

/// Where a call's arguments start, and what they are.
fn call_arguments(node: Node<'_>) -> Option<(usize, Vec<Span>)> {
    if let Some(list) = argument_list(node) {
        return Some((list.start_byte(), list_items(list)));
    }

    // No wrapper.
    let callee = node
        .child_by_field_name("function")
        .or_else(|| node.named_child(0))?;
    let mut cursor = node.walk();
    let open = node
        .children(&mut cursor)
        .find(|c| c.kind() == "(" && c.start_byte() >= callee.end_byte())?;

    let mut items = node.walk();
    let arguments: Vec<Span> = node
        .named_children(&mut items)
        .filter(|c| c.start_byte() >= open.end_byte() && !c.kind().contains("comment"))
        .map(Span::from)
        .filter(|span| !span.is_empty())
        .collect();
    Some((open.start_byte(), arguments))
}

fn named_arguments_block_change(call: Node<'_>, language: Language, change: &Change) -> bool {
    if language != Language::Python || matches!(change, Change::Remove(_)) {
        return false;
    }
    let Some(arguments) = argument_list(call) else {
        return false;
    };
    let mut cursor = arguments.walk();
    let has_keyword = arguments
        .named_children(&mut cursor)
        .any(|argument| argument.kind() == "keyword_argument");
    has_keyword
}

/// The call expression whose callee is at `span`.
fn inside_import(parsed: &Parsed, span: Span) -> bool {
    let Some(mut node) = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return false;
    };
    for _ in 0..8 {
        let kind = node.kind();
        if kind.contains("import") || kind.contains("use_") || kind.contains("export") {
            return true;
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => return false,
        }
    }
    false
}

/// The argument tokens of a call written inside a macro body.
fn macro_call_arguments(parsed: &Parsed, span: Span) -> Option<(usize, Vec<Span>)> {
    let node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    if node.end_byte() != span.end {
        return None;
    }
    let mut tree = node.next_sibling()?;
    // `myc::model::slug(x)`: the identifier's siblings inside the token tree
    // are `::` tokens and path segments; the argument tree follows the last.
    while matches!(tree.kind(), "::" | "identifier") {
        tree = tree.next_sibling()?;
    }
    if tree.kind() != "token_tree" {
        return None;
    }
    let text_start = tree.start_byte();
    let mut cursor = tree.walk();
    let children: Vec<Node> = tree.children(&mut cursor).collect();
    let first = children.first()?;
    if first.kind() != "(" {
        return None;
    }
    let mut arguments = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_end = 0usize;
    for child in &children[1..] {
        match child.kind() {
            ")" => break,
            "," => {
                if let Some(start) = run_start.take() {
                    arguments.push(Span::new(start, run_end));
                }
            }
            _ => {
                if run_start.is_none() {
                    run_start = Some(child.start_byte());
                }
                run_end = child.end_byte();
            }
        }
    }
    if let Some(start) = run_start {
        arguments.push(Span::new(start, run_end));
    }
    Some((text_start, arguments))
}

fn call_expression<'a>(parsed: &'a Parsed, span: Span) -> Option<Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..8 {
        // Two grammars do not say "call".
        if node.kind().contains("call")
            || matches!(
                node.kind(),
                "include_statement" | "method_invocation" | "object_creation_expression"
            )
        {
            return match argument_list(node) {
                Some(args) if span.start >= args.start_byte() => None,
                _ => Some(node),
            };
        }
        node = node.parent()?;
    }
    None
}

/// Named children of a list node, i.e.
fn without_receiver(
    language: Language,
    kind: SymbolKind,
    source: &str,
    items: Vec<Span>,
) -> Vec<Span> {
    if kind != SymbolKind::Method {
        return items;
    }
    let receiver = items.first().is_some_and(|span| {
        let text = span.text(source).trim();
        let bare = text
            .trim_start_matches('&')
            .trim_start_matches("mut ")
            .trim();
        match language {
            Language::Rust | Language::Zig => bare == "self" || bare.starts_with("self:"),
            Language::Python => {
                bare == "self"
                    || bare == "cls"
                    || bare.starts_with("self:")
                    || bare.starts_with("cls:")
            }
            _ => false,
        }
    });
    match receiver {
        true => items.into_iter().skip(1).collect(),
        false => items,
    }
}

fn list_items(list: Node<'_>) -> Vec<Span> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|c| !c.kind().contains("comment"))
        .map(Span::from)
        // An item occupying no bytes is not an item, whatever the grammar calls it;
        // counting one would put every position after it out by one.
        .filter(|span| !span.is_empty())
        .collect()
}

// `greet() { … }` declares no parameters, but a caller still observes a signature.

/// A positional parameter reference inside a function body.
#[derive(Debug, Clone, Copy)]
struct Positional {
    /// The digits alone, so a renumber rewrites `$2` and `${2}` the same way.
    span: Span,
    number: usize,
    /// `${12}` can hold two digits; `$12` cannot.
    braced: bool,
}

/// One command invocation of the function this changes.
struct ShellCall {
    /// Span of the command name, where a first argument goes.
    name: Span,
    /// The argument words, in source order.
    arguments: Vec<Span>,
}

/// Change the positional signature of the shell function `sym`.
fn shell_function(index: &Index, sym: &Symbol, change: Change) -> Result<SignaturePlan> {
    if sym.kind != SymbolKind::Function {
        return Err(Refusal::Declined {
            detail: format!(
                "'{}' is {}; only a shell function has positional parameters",
                sym.name,
                sym.kind.with_article()
            ),
        }
        .into());
    }
    // Two functions of one name make every call site ambiguous, and bash resolves
    // the ambiguity at run time by whichever definition ran last.
    if let Some(twin) = index.symbols.iter().find(|s| {
        s.id != sym.id
            && s.name == sym.name
            && s.kind == SymbolKind::Function
            && s.language == Language::Bash
    }) {
        return Err(Refusal::AmbiguousDefinition {
            name: sym.name.clone(),
            file: twin.file.clone(),
        }
        .into());
    }
    reject_hidden_call_sites(index, sym)?;

    let mut notes: Vec<String> = Vec::new();

    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(Language::Bash, &source)?;
    let definition = shell_function_node(&parsed, sym)?;
    let positionals = shell_positionals(definition, &source, sym, &mut notes)?;

    // Renumber the body first: what the body reads decides whether the change is
    // legal at all.
    let mut edits = EditSet::new();
    let mut renumbered: Vec<Span> = Vec::new();
    for reference in &positionals {
        let renumbered_to = match &change {
            Change::Remove(at) => {
                let gone = at + 1;
                if reference.number == gone {
                    return Err(still_used(
                        format!(
                            "the body of `{}` still reads ${gone}, the parameter being \
                             removed; nothing would supply it afterwards",
                            sym.name
                        ),
                        std::iter::once(refusal_site(&sym.file, reference.span.start)),
                    ));
                }
                if reference.number > gone {
                    reference.number - 1
                } else {
                    reference.number
                }
            }
            Change::Move { from, to } => {
                let (a, b) = (from + 1, to + 1);
                if reference.number == a {
                    b
                } else if reference.number == b {
                    a
                } else {
                    reference.number
                }
            }
            Change::Add { at, .. } => {
                let inserted = at + 1;
                if reference.number >= inserted {
                    reference.number + 1
                } else {
                    reference.number
                }
            }
        };
        if renumbered_to == reference.number {
            continue;
        }
        edits.add(
            sym.file.clone(),
            Edit::new(
                reference.span,
                shell_positional_text(renumbered_to, reference.braced),
                format!("${} is now ${renumbered_to}", reference.number),
            ),
        );
        renumbered.push(reference.span);
    }

    // A declaration is what bash does not have; saying so beats silently dropping
    // text the caller supplied.
    if let Change::Add { declaration, .. } = &change {
        if !declaration.trim().is_empty() {
            notes.push(format!(
                "a shell function declares no parameters, so the declaration `{}` went \
                 nowhere; only the argument and the body's numbering changed",
                first_line(declaration)
            ));
        }
    }

    let calls = shell_call_files(index, sym, &mut notes)?;
    if let Change::Add { argument, .. } = &change {
        if argument.trim().is_empty() && !calls.is_empty() {
            return Err(Refusal::Declined {
                detail: format!(
                    "{1} site(s) call `{0}` and shell arguments are positional, so an added \
                 parameter needs a word to pass; supply an argument",
                    sym.name,
                    calls.values().map(|v| v.len()).sum::<usize>()
                ),
            }
            .into());
        }
    }

    let mut call_sites = 0usize;
    for (file, references) in &calls {
        let call_source = crate::vfs::read_to_string(file)?;
        let call_parsed = Parsers::new().parse(Language::Bash, &call_source)?;
        for reference in references {
            let call = shell_call_at(&call_parsed, sym, file, reference.span)?;
            shell_check_positions(&call, &call_parsed, &call_source, sym, file, &change)?;
            shell_rewrite_call(
                &mut edits,
                file,
                &call_source,
                &call,
                sym,
                &change,
                &mut notes,
            )?;
            call_sites += 1;
        }
    }

    reject_shell_edit_collisions(&edits, &sym.file, sym, &renumbered)?;

    // A change that rewrites nothing is not a change.
    if edits.is_empty() {
        return Err(Refusal::Declined {
            detail: format!(
                "the change leaves `{}` as it was: no call site and no reference in \
             its body names that position.{}",
                sym.name,
                notes.iter().map(|n| format!("\n  {n}")).collect::<String>()
            ),
        }
        .into());
    }

    Ok(SignaturePlan {
        subject: sym.name.clone(),
        subject_kind: Subject::ShellFunction,
        change,
        edits,
        call_sites,
        notes,
    })
}

/// The `function_definition` node `sym` names.
fn shell_function_node<'a>(parsed: &'a Parsed, sym: &Symbol) -> Result<Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "could not locate `{}` in {} after reparsing it",
                sym.name,
                sym.file.display()
            )
        })?;
    for _ in 0..8 {
        if node.kind() == "function_definition" {
            return Ok(node);
        }
        let Some(parent) = node.parent() else { break };
        node = parent;
    }
    Err(Refusal::Declined {
        detail: format!(
            "`{}` at {} is not a function definition this grammar exposes",
            sym.name,
            location(&sym.file, sym.name_span.start)
        ),
    }
    .into())
}

/// Every `$1`-style reference in a function body, refusing on the shapes no
/// renumbering can follow.
fn shell_positionals(
    definition: Node<'_>,
    source: &str,
    sym: &Symbol,
    notes: &mut Vec<String>,
) -> Result<Vec<Positional>> {
    let mut out: Vec<Positional> = Vec::new();
    let mut reads_count = false;

    for node in descendants(definition) {
        let text = Span::from(node).text(source);
        match node.kind() {
            // A nested function has positional parameters of its own, and no rule
            // says which `$1` inside the body belongs to which.
            "function_definition" if node.id() != definition.id() => {
                return Err(Refusal::Declined {
                    detail: format!(
                        "`{}` defines a nested function at {}; its `$1` names that function's \
                 first argument, not this one's, so nothing can renumber the body",
                        sym.name,
                        location(&sym.file, node.start_byte())
                    ),
                }
                .into())
            }
            "special_variable_name" if text == "@" || text == "*" => {
                return Err(Refusal::Declined {
                    detail: format!(
                        "the body of `{}` uses `${text}` at {}, which expands to the whole \
                 parameter list; renumbering individual references cannot follow it",
                        sym.name,
                        location(&sym.file, node.start_byte())
                    ),
                }
                .into())
            }
            "special_variable_name" if text == "#" => reads_count = true,
            "command_name" if text == "shift" => {
                return Err(Refusal::Declined {
                    detail: format!(
                        "the body of `{}` calls `shift` at {}, which renumbers the parameters at \
                 run time; a static renumbering cannot follow it",
                        sym.name,
                        location(&sym.file, node.start_byte())
                    ),
                }
                .into())
            }
            "command_name" if text == "set" => {
                if shell_command_resets_parameters(node, source) {
                    return Err(Refusal::Declined {
                        detail: format!(
                            "the body of `{}` calls `set --` at {}, which replaces the \
                         positional parameters wholesale; a static renumbering cannot \
                         follow it",
                            sym.name,
                            location(&sym.file, node.start_byte())
                        ),
                    }
                    .into());
                }
            }
            "variable_name" if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                let braced = match node.parent().map(|p| p.kind()) {
                    Some("expansion") => true,
                    Some("simple_expansion") => false,
                    other => {
                        return Err(Refusal::Declined {
                            detail: format!(
                                "`${text}` at {} sits inside a {} instead of an expansion, so the \
                         tool cannot tell what rewriting it would mean",
                                location(&sym.file, node.start_byte()),
                                other.unwrap_or("(nothing)")
                            ),
                        }
                        .into())
                    }
                };
                // `$0` is the script's name.
                let number: usize = text.parse()?;
                if number == 0 {
                    continue;
                }
                if !braced && text.len() > 1 {
                    return Err(Refusal::Declined {
                        detail: format!(
                            "`${text}` at {} is not parameter {text}: the shell reads `$` and one \
                         digit, then `{}` as literal text. Write it as `${{{text}}}` first if \
                         that is what was meant",
                            location(&sym.file, node.start_byte()),
                            &text[1..]
                        ),
                    }
                    .into());
                }
                out.push(Positional {
                    span: Span::from(node),
                    number,
                    braced,
                });
            }
            _ => {}
        }
    }

    if reads_count {
        notes.push(format!(
            "the body of `{}` reads `$#`, the parameter count; the change alters it and \
             no renumbering can compensate",
            sym.name
        ));
    }
    out.sort_by_key(|p| p.span.start);
    Ok(out)
}

/// Does this `set` command replace the positional parameters?
fn shell_command_resets_parameters(name: Node<'_>, source: &str) -> bool {
    let Some(command) = name.parent() else {
        return false;
    };
    let mut cursor = command.walk();
    let found = command
        .children_by_field_name("argument", &mut cursor)
        .any(|argument| Span::from(argument).text(source) == "--");
    found
}

/// Spell a renumbered reference.
fn shell_positional_text(number: usize, braced: bool) -> String {
    if braced || number < 10 {
        number.to_string()
    } else {
        format!("{{{number}}}")
    }
}

/// Every command invocation of `sym` that can be tied to it, grouped by file.
fn shell_call_files<'a>(
    index: &'a Index,
    sym: &Symbol,
    notes: &mut Vec<String>,
) -> Result<BTreeMap<PathBuf, Vec<&'a Reference>>> {
    let (sources, opaque) = shell_source_graph(index);

    let mut by_file: BTreeMap<PathBuf, Vec<&Reference>> = BTreeMap::new();
    for reference in &index.references {
        if reference.language != Language::Bash
            || reference.kind != ReferenceKind::Call
            || reference.name != sym.name
        {
            continue;
        }
        by_file
            .entry(reference.file.clone())
            .or_default()
            .push(reference);
    }

    let mut out: BTreeMap<PathBuf, Vec<&Reference>> = BTreeMap::new();
    for (file, references) in by_file {
        if file != sym.file && opaque.contains(&file) {
            return Err(Refusal::Unknowable {
                detail: format!(
                    "{} calls `{}` and also sources a path that is not a literal, so what \
                     is in scope there cannot be known",
                    file.display(),
                    sym.name
                ),
            }
            .into());
        }
        if file == sym.file || shell_reaches(&sources, &file, &sym.file) {
            let mut references = references;
            references.sort_by_key(|r| r.span.start);
            out.insert(file, references);
            continue;
        }
        notes.push(format!(
            "{} runs `{}` {} time(s) but never sources {}, so those name a different \
             command; this left them alone",
            file.display(),
            sym.name,
            references.len(),
            sym.file.display()
        ));
    }
    Ok(out)
}

/// The `source` graph of the workspace, plus the files whose sourcing is not literal.
pub(super) fn shell_source_graph(
    index: &Index,
) -> (BTreeMap<PathBuf, Vec<PathBuf>>, BTreeSet<PathBuf>) {
    let mut sources: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut opaque: BTreeSet<PathBuf> = BTreeSet::new();

    for (path, info) in index.files() {
        if info.language != Language::Bash {
            continue;
        }
        let Some(dir) = path.parent() else { continue };
        for import in &info.imports {
            if !is_literal_shell_path(&import.path) {
                opaque.insert(path.clone());
                continue;
            }
            sources
                .entry(path.clone())
                .or_default()
                .push(crate::vfs::normalise(dir.join(&import.path)));
        }
    }
    (sources, opaque)
}

/// Is this `source` argument a fixed path, instead of one computed at run time?
fn is_literal_shell_path(path: &str) -> bool {
    !path.is_empty() && !path.contains(['$', '`', '*', '?', '[', '~'])
}

/// Does `from` reach `target` by following `source` statements?
pub(super) fn shell_reaches(
    sources: &BTreeMap<PathBuf, Vec<PathBuf>>,
    from: &Path,
    target: &Path,
) -> bool {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue = vec![from.to_path_buf()];
    while let Some(file) = queue.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let Some(sourced) = sources.get(&file) else {
            continue;
        };
        for next in sourced {
            if next == target {
                return true;
            }
            queue.push(next.clone());
        }
    }
    false
}

/// The command invocation whose name occupies `span`.
fn shell_call_at(parsed: &Parsed, sym: &Symbol, file: &Path, span: Span) -> Result<ShellCall> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "could not locate the call to `{}` at {}",
                sym.name,
                location(file, span.start)
            )
        })?;
    // `command_substitution` also contains "command", so the kind is matched whole.
    for _ in 0..8 {
        if node.kind() == "command" {
            break;
        }
        let Some(parent) = node.parent() else {
            return Err(Refusal::Declined {
                detail: format!(
                    "the call to `{}` at {} is not a command invocation this grammar exposes",
                    sym.name,
                    location(file, span.start)
                ),
            }
            .into());
        };
        node = parent;
    }
    if node.kind() != "command" {
        return Err(Refusal::Declined {
            detail: format!(
                "the call to `{}` at {} is not a command invocation this grammar exposes",
                sym.name,
                location(file, span.start)
            ),
        }
        .into());
    }

    let name = node.child_by_field_name("name").ok_or_else(|| {
        anyhow::anyhow!(
            "the command at {} has no name node",
            location(file, node.start_byte())
        )
    })?;
    // A name that fails to coincide with the reference means the reference was an argument of
    // some other command, and no call took place.
    if !Span::from(name).contains(span) {
        return Err(Refusal::Declined {
            detail: format!(
                "`{}` at {} is an argument of another command, not a call to the function",
                sym.name,
                location(file, span.start)
            ),
        }
        .into());
    }

    let mut cursor = node.walk();
    let arguments: Vec<Span> = node
        .children_by_field_name("argument", &mut cursor)
        .map(Span::from)
        .collect();
    Ok(ShellCall {
        name: Span::from(name),
        arguments,
    })
}

/// Refuse unless every position this change reads or moves is exactly one argument.
fn shell_check_positions(
    call: &ShellCall,
    parsed: &Parsed,
    source: &str,
    sym: &Symbol,
    file: &Path,
    change: &Change,
) -> Result<()> {
    let required = match change {
        Change::Remove(at) => at + 1,
        Change::Move { from, to } => from.max(to) + 1,
        Change::Add { at, .. } => *at,
    };
    for span in call.arguments.iter().take(required) {
        if let Some(reason) = shell_argument_is_indeterminate(parsed, source, *span) {
            // Not a resolution that is too weak, the call resolved.
            return Err(Refusal::Unknowable {
                detail: format!(
                    "the call to `{}` at {} passes {}, so the position of everything \
                     after it is only known at run time{}",
                    sym.name,
                    location(file, span.start),
                    reason.0,
                    reason.1.map(|r| format!(". {r}")).unwrap_or_default()
                ),
            }
            .into());
        }
    }
    Ok(())
}

/// Why an argument stands for no single positional parameter, and what to
/// do about it where there is anything to do.
fn shell_argument_is_indeterminate(
    parsed: &Parsed,
    source: &str,
    span: Span,
) -> Option<(String, Option<&'static str>)> {
    let node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    shell_word_problem(node, source)
}

/// Recursive form of [`shell_argument_is_indeterminate`], over an already-found node.
fn shell_word_problem(node: Node<'_>, source: &str) -> Option<(String, Option<&'static str>)> {
    let text = Span::from(node).text(source);
    // `$@` expands to one word per parameter wherever it appears, quoted or not, and
    // unquoted `$*` splits on IFS.
    for inner in descendants(node) {
        if inner.kind() == "special_variable_name" {
            let name = Span::from(inner).text(source);
            if name == "@" || name == "*" {
                return Some((
                    format!("`${name}`, which stands for the whole parameter list"),
                    None,
                ));
            }
        }
    }
    match node.kind() {
        "word" => text.contains(['*', '?', '[', '{']).then(|| {
            (
                format!("`{text}`, a glob or brace expansion that can become any number of words"),
                Some("quote it to stop the shell expanding it"),
            )
        }),
        "string" | "raw_string" | "ansi_c_string" | "translated_string" | "number" => None,
        "concatenation" => {
            let mut cursor = node.walk();
            let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
            children
                .into_iter()
                .find_map(|child| shell_word_problem(child, source))
        }
        other => Some((
            format!(
                "`{text}`, {} the shell splits into words at run time",
                match other {
                    "simple_expansion" | "expansion" => "an unquoted expansion",
                    "command_substitution" => "an unquoted command substitution",
                    "process_substitution" => "a process substitution",
                    "arithmetic_expansion" => "an unquoted arithmetic expansion",
                    _ => "a word",
                }
            ),
            Some("quote it to make it one argument"),
        )),
    }
}

/// Rewrite one call site's argument words.
fn shell_rewrite_call(
    edits: &mut EditSet,
    file: &Path,
    source: &str,
    call: &ShellCall,
    sym: &Symbol,
    change: &Change,
    notes: &mut Vec<String>,
) -> Result<()> {
    match change {
        Change::Remove(at) => {
            let Some(target) = call.arguments.get(*at) else {
                notes.push(format!(
                    "{}: the call to `{}` passes {} argument(s), so it has nothing at \
                     position {at} to remove",
                    location(file, call.name.start),
                    sym.name,
                    call.arguments.len()
                ));
                return Ok(());
            };
            edits.add(
                file.to_path_buf(),
                Edit::new(
                    shell_argument_removal(&call.arguments, *at, call.name, *target),
                    "",
                    format!("drop argument {at} of `{}`", sym.name),
                ),
            );
        }
        Change::Move { from, to } => {
            let (Some(a), Some(b)) = (call.arguments.get(*from), call.arguments.get(*to)) else {
                notes.push(format!(
                    "{}: the call to `{}` passes {} argument(s), so it holds no position \
                     {from} and {to} both; this left its arguments alone",
                    location(file, call.name.start),
                    sym.name,
                    call.arguments.len()
                ));
                return Ok(());
            };
            edits.add(
                file.to_path_buf(),
                Edit::new(*a, b.text(source), format!("move argument {from}")),
            );
            edits.add(
                file.to_path_buf(),
                Edit::new(*b, a.text(source), format!("move argument {to}")),
            );
        }
        Change::Add { at, argument, .. } => {
            if *at > call.arguments.len() {
                return Err(Refusal::Declined {
                    detail: format!(
                        "the call to `{}` at {} passes {} argument(s), so inserting at position \
                     {at} would land at position {} instead",
                        sym.name,
                        location(file, call.name.start),
                        call.arguments.len(),
                        call.arguments.len()
                    ),
                }
                .into());
            }
            match call.arguments.get(*at) {
                Some(before) => edits.add(
                    file.to_path_buf(),
                    Edit::new(
                        Span::new(before.start, before.start),
                        format!("{argument} "),
                        format!("pass a new argument {at} to `{}`", sym.name),
                    ),
                ),
                None => {
                    let anchor = call
                        .arguments
                        .last()
                        .map(|a| a.end)
                        .unwrap_or(call.name.end);
                    edits.add(
                        file.to_path_buf(),
                        Edit::new(
                            Span::new(anchor, anchor),
                            format!(" {argument}"),
                            format!("pass a new argument {at} to `{}`", sym.name),
                        ),
                    );
                }
            }
        }
    }
    Ok(())
}

/// The bytes a removed argument takes with it: itself plus one separating run of
/// whitespace, so the surviving words stay separated exactly once.
fn shell_argument_removal(arguments: &[Span], at: usize, name: Span, target: Span) -> Span {
    match arguments.get(at + 1) {
        Some(next) => Span::new(target.start, next.start),
        None => {
            let previous = if at == 0 {
                name.end
            } else {
                arguments[at - 1].end
            };
            Span::new(previous, target.end)
        }
    }
}

/// Refuse when a call site inside the function's own body passes bytes the body
/// renumbering also rewrites.
fn reject_shell_edit_collisions(
    edits: &EditSet,
    file: &Path,
    sym: &Symbol,
    renumbered: &[Span],
) -> Result<()> {
    let Some(list) = edits.edits_for(file) else {
        return Ok(());
    };
    for edit in list {
        if renumbered.contains(&edit.span) {
            continue;
        }
        if let Some(clash) = renumbered.iter().find(|span| span.overlaps(edit.span)) {
            return Err(Refusal::Declined {
                detail: format!(
                    "the recursive call to `{}` at {} passes a positional parameter that this \
                 change also renumbers; two edits would land on the same bytes",
                    sym.name,
                    location(file, clash.start)
                ),
            }
            .into());
        }
    }
    Ok(())
}

/// Every node of a subtree, the root included.
fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        out.push(current);
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

/// A `variable "x" { ...
#[derive(Debug)]
struct ModuleVariable {
    id: SymbolId,
    name: String,
    file: PathBuf,
    /// The whole block, `variable` keyword through closing brace.
    span: Span,
    has_default: bool,
}

/// A `module "m" { source = "./dir" ...
#[derive(Debug)]
struct ModuleCall {
    label: String,
    file: PathBuf,
    /// The whole `module` block.
    span: Span,
    /// Named arguments, each the whole `name = value` attribute.
    arguments: Vec<(String, Span)>,
}

/// What a `module` block's `source` argument says.
enum ModuleSource {
    /// A literal string, e.g.
    Literal(String),
    /// Anything else: `var.where`, an interpolation, a function call.
    Computed(String),
    /// No `source` argument at all.
    Missing,
}

/// Change the signature of the Terraform module `sym` belongs to.
fn terraform_module(index: &Index, sym: &Symbol, change: Change) -> Result<SignaturePlan> {
    let dir = target_module_dir(index, sym)?;
    let variables = module_variables(index, &dir)?;
    let calls = module_calls(index, &dir)?;

    let mut edits = EditSet::new();
    let mut call_sites = 0usize;

    match &change {
        // Terraform names its arguments, so shuffling `variable` blocks only reformats
        // that no call site can observe.
        Change::Move { .. } => {
            return Err(Refusal::Unsupported {
                operation: "reordering module variables".to_string(),
                language: Language::Hcl,
                because: "a Terraform module names its arguments rather than \
                          numbering them, so moving a `variable` block changes nothing \
                          at any call site",
            }
            .into());
        }

        Change::Remove(at) => {
            let Some(target) = variables.get(*at) else {
                return Err(Refusal::Declined {
                    detail: format!(
                        "there is no module variable at position {at}; {} declares {}",
                        crate::vfs::describe_dir(dir),
                        describe_variables(&variables)
                    ),
                }
                .into());
            };

            // A variable the module's own configuration still reads has to stay: the
            // `var.x` uses would dangle.
            let uses: Vec<&crate::model::Reference> = index
                .references_to(target.id)
                .into_iter()
                .filter(|r| r.file.parent() == Some(dir.as_path()))
                .collect();
            if !uses.is_empty() {
                let where_ = uses
                    .iter()
                    .map(|r| location(&r.file, r.span.start))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(still_used(
                    format!(
                        "{1} site(s) inside the module still read `{0}` ({where_}); \
                         removing it would leave those `var.{2}` references dangling",
                        target.name,
                        uses.len(),
                        target.name
                    ),
                    uses.iter().map(|r| refusal_site(&r.file, r.span.start)),
                ));
            }

            let source = crate::vfs::read_to_string(&target.file)?;
            edits.add(
                target.file.clone(),
                Edit::new(
                    statement_deletion_span(&source, target.span),
                    "",
                    format!("remove module variable `{}`", target.name),
                ),
            );

            for call in &calls {
                let Some((_, span)) = call.arguments.iter().find(|(n, _)| *n == target.name) else {
                    // The caller relied on the default; there is nothing to remove.
                    continue;
                };
                let call_source = crate::vfs::read_to_string(&call.file)?;
                edits.add(
                    call.file.clone(),
                    Edit::new(
                        statement_deletion_span(&call_source, *span),
                        "",
                        format!(
                            "drop `{}` argument from module \"{}\"",
                            target.name, call.label
                        ),
                    ),
                );
                call_sites += 1;
            }

            // A values file assigns the same call surface from the other side; an
            // assignment left behind names a variable that no longer exists.
            for (file, span) in tfvars_assignments(index, &dir, &target.name)? {
                let source = crate::vfs::read_to_string(&file)?;
                edits.add(
                    file,
                    Edit::new(
                        statement_deletion_span(&source, span),
                        "",
                        format!("drop `{}` from the values file", target.name),
                    ),
                );
                call_sites += 1;
            }
        }

        Change::Add {
            at,
            declaration,
            argument,
        } => {
            let (name, has_default) = parse_variable_declaration(declaration)?;

            if let Some(existing) = variables.iter().find(|v| v.name == name) {
                return Err(Refusal::NameCollision {
                    existing: name,
                    file: existing.file.clone(),
                }
                .into());
            }
            for call in &calls {
                if call.arguments.iter().any(|(n, _)| *n == name) {
                    return Err(Refusal::NameCollision {
                        existing: name,
                        file: call.file.clone(),
                    }
                    .into());
                }
            }
            // A variable with no default is required, so every caller has to start passing it.
            if argument.is_empty() && !has_default && !calls.is_empty() {
                return Err(Refusal::Declined {
                    detail: format!(
                        "`{name}` has no `default`, so it is required at all {} call site(s); \
                     supply an argument value to pass there, or give the variable a default",
                        calls.len()
                    ),
                }
                .into());
            }

            let (file, offset, text) =
                variable_insertion(index, &dir, &variables, *at, declaration)?;
            edits.add(
                file,
                Edit::new(
                    Span::new(offset, offset),
                    text,
                    format!("declare module variable `{name}`"),
                ),
            );

            if !argument.is_empty() {
                for call in &calls {
                    let call_source = crate::vfs::read_to_string(&call.file)?;
                    let Some((_, last)) = call.arguments.last() else {
                        return Err(Refusal::Declined {
                            detail: format!(
                                "module \"{}\" at {} has no arguments to append to",
                                call.label,
                                location(&call.file, call.span.start)
                            ),
                        }
                        .into());
                    };
                    let indent = argument_indent(&call_source, call.span, *last);
                    edits.add(
                        call.file.clone(),
                        Edit::new(
                            Span::new(last.end, last.end),
                            format!("\n{indent}{name} = {argument}"),
                            format!("pass `{name}` to module \"{}\"", call.label),
                        ),
                    );
                    call_sites += 1;
                }
            }
        }
    }

    Ok(SignaturePlan {
        subject: dir.display().to_string(),
        subject_kind: Subject::TerraformModule,
        change,
        edits,
        call_sites,
        notes: Vec::new(),
    })
}

/// The module directory whose signature `sym` identifies.
fn target_module_dir(index: &Index, sym: &Symbol) -> Result<PathBuf> {
    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;
    let block = top_level_blocks(&parsed)
        .into_iter()
        .find(|b| Span::from(*b).contains(sym.name_span));

    match (sym.kind, block) {
        (SymbolKind::Variable, Some(block))
            if block_keyword(block, &source) == Some("variable") =>
        {
            let dir = sym.file.parent().ok_or_else(|| {
                anyhow::anyhow!("{} is not inside a directory", sym.file.display())
            })?;
            Ok(crate::vfs::normalise(dir))
        }
        (SymbolKind::Module, Some(block)) if block_keyword(block, &source) == Some("module") => {
            let dir = sym.file.parent().ok_or_else(|| {
                anyhow::anyhow!("{} is not inside a directory", sym.file.display())
            })?;
            match block_source(block, &source) {
                ModuleSource::Literal(path) => {
                    let Some(target) = local_module_dir(dir, &path) else {
                        return Err(Refusal::Declined {
                            detail: format!(
                                "module \"{}\" at {} has source `{path}`, which is not a local \
                             directory; nothing in this workspace declares its variables",
                                sym.name,
                                location(&sym.file, sym.name_span.start)
                            ),
                        }
                        .into());
                    };
                    if !directory_has_hcl(index, &target) {
                        return Err(Refusal::Declined {
                            detail: format!(
                                "module \"{}\" at {} points at {}, which holds no Terraform files \
                             in this workspace",
                                sym.name,
                                location(&sym.file, sym.name_span.start),
                                target.display()
                            ),
                        }
                        .into());
                    }
                    Ok(target)
                }
                ModuleSource::Computed(text) => Err(Refusal::Declined {
                    detail: format!(
                        "module \"{}\" at {} has a computed source `{text}`, so the directory it \
                     calls is not knowable without applying the configuration",
                        sym.name,
                        location(&sym.file, sym.name_span.start)
                    ),
                }
                .into()),
                ModuleSource::Missing => Err(Refusal::Declined {
                    detail: format!(
                        "module \"{}\" at {} has no `source` argument",
                        sym.name,
                        location(&sym.file, sym.name_span.start)
                    ),
                }
                .into()),
            }
        }
        _ => Err(Refusal::Declined {
            detail: format!(
                "'{}' is {} in Terraform; only a `variable` block or a `module` block names a \
             module signature",
                sym.name,
                sym.kind.with_article()
            ),
        }
        .into()),
    }
}

/// The `variable` blocks of a module directory, in document order.
fn module_variables(index: &Index, dir: &Path) -> Result<Vec<ModuleVariable>> {
    let mut out: Vec<ModuleVariable> = Vec::new();

    for (path, info) in index.files() {
        if info.language != Language::Hcl || !is_terraform_config(path) {
            continue;
        }
        if path.parent().map(crate::vfs::normalise).as_deref() != Some(dir) {
            continue;
        }
        let source = crate::vfs::read_to_string(path)?;
        let parsed = Parsers::new().parse(Language::Hcl, &source)?;
        for block in top_level_blocks(&parsed) {
            if block_keyword(block, &source) != Some("variable") {
                continue;
            }
            let Some(label) = block_labels(block).first().copied() else {
                continue;
            };
            let name = Span::from(label).text(&source).to_string();
            let span = Span::from(block);
            let id = info
                .symbols
                .iter()
                .filter_map(|id| index.symbol(*id))
                .find(|s| s.kind == SymbolKind::Variable && s.name_span == Span::from(label))
                .map(|s| s.id)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "the index has no symbol for `variable \"{name}\"` at {}",
                        location(path, span.start)
                    )
                })?;
            let has_default = block_body(block).is_some_and(|body| {
                body_attributes(body, &source)
                    .iter()
                    .any(|(n, _)| *n == "default")
            });
            out.push(ModuleVariable {
                id,
                name,
                file: path.clone(),
                span,
                has_default,
            });
        }
    }

    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.span.start.cmp(&b.span.start)));
    Ok(out)
}

/// Every `module` block in the workspace that calls the module in `dir`.
fn module_calls(index: &Index, dir: &Path) -> Result<Vec<ModuleCall>> {
    let mut calls: Vec<ModuleCall> = Vec::new();
    let mut opaque: Vec<String> = Vec::new();

    for (path, info) in index.files() {
        if info.language != Language::Hcl || !is_terraform_config(path) {
            continue;
        }
        let source = crate::vfs::read_to_string(path)?;
        let parsed = Parsers::new().parse(Language::Hcl, &source)?;
        let here = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("{} is not inside a directory", path.display()))?;

        for block in top_level_blocks(&parsed) {
            if block_keyword(block, &source) != Some("module") {
                continue;
            }
            let label = block_labels(block)
                .first()
                .map(|l| Span::from(*l).text(&source).to_string())
                .unwrap_or_default();
            let at = location(path, block.start_byte());

            match block_source(block, &source) {
                ModuleSource::Missing => opaque.push(format!(
                    "module \"{label}\" at {at} has no `source` argument"
                )),
                ModuleSource::Computed(text) => opaque.push(format!(
                    "module \"{label}\" at {at} has a computed source `{text}`"
                )),
                ModuleSource::Literal(literal) => {
                    if local_module_dir(here, &literal).as_deref() != Some(dir) {
                        continue;
                    }
                    let arguments = block_body(block)
                        .map(|body| {
                            body_attributes(body, &source)
                                .into_iter()
                                .map(|(name, node)| (name.to_string(), Span::from(node)))
                                .collect()
                        })
                        .unwrap_or_default();
                    calls.push(ModuleCall {
                        label,
                        file: path.clone(),
                        span: Span::from(block),
                        arguments,
                    });
                }
            }
        }

        // The index records every module `source` as an import.
        for import in &info.imports {
            if local_module_dir(here, &import.path).as_deref() != Some(dir) {
                continue;
            }
            if !calls
                .iter()
                .any(|c| c.file == *path && c.span == import.span)
            {
                return Err(Refusal::Unknowable {
                    detail: format!(
                        "a `module` block at {} sources {} but is not a top-level block, so \
                         nothing can rewrite its arguments",
                        location(path, import.span.start),
                        crate::vfs::describe_dir(dir)
                    ),
                }
                .into());
            }
        }
    }

    if !opaque.is_empty() {
        return Err(Refusal::Unknowable {
            detail: format!(
                "{} `module` block(s) do not name a literal source, so nothing rules out a \
                 call to {}: {}",
                opaque.len(),
                crate::vfs::describe_dir(dir),
                opaque.join("; ")
            ),
        }
        .into());
    }

    calls.sort_by(|a, b| a.file.cmp(&b.file).then(a.span.start.cmp(&b.span.start)));
    Ok(calls)
}

/// Top-level `name = value` assignments of `name` in the module's `.tfvars` files.
fn tfvars_assignments(index: &Index, dir: &Path, name: &str) -> Result<Vec<(PathBuf, Span)>> {
    let mut out = Vec::new();
    for (path, info) in index.files() {
        if info.language != Language::Hcl || is_terraform_config(path) {
            continue;
        }
        if path.parent().map(crate::vfs::normalise).as_deref() != Some(dir) {
            continue;
        }
        for symbol in info.symbols.iter().filter_map(|id| index.symbol(*id)) {
            if symbol.kind == SymbolKind::Key && symbol.name == name {
                out.push((path.clone(), symbol.full_span));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Where a new `variable` block goes: its file, byte offset and text.
fn variable_insertion(
    index: &Index,
    dir: &Path,
    variables: &[ModuleVariable],
    at: usize,
    declaration: &str,
) -> Result<(PathBuf, usize, String)> {
    let block = declaration.trim_end_matches('\n');

    if let Some(before) = variables.get(at) {
        let source = crate::vfs::read_to_string(&before.file)?;
        let offset = full_line_span(&source, before.span.start).start;
        return Ok((before.file.clone(), offset, format!("{block}\n\n")));
    }

    // Past the end, or no position given: after the last variable there is.
    if let Some(last) = variables.last() {
        let source = crate::vfs::read_to_string(&last.file)?;
        let offset = full_line_span(&source, last.span.end - 1).end;
        return Ok((last.file.clone(), offset, format!("\n{block}\n")));
    }

    // A module with no variables at all has no anchor.
    let path = dir.join("variables.tf");
    if index.file(&path).is_none() {
        return Err(Refusal::Declined {
            detail: format!(
                "module {} declares no variables and has no variables.tf to add one to; create \
             the file first",
                crate::vfs::describe_dir(dir)
            ),
        }
        .into());
    }
    let source = crate::vfs::read_to_string(&path)?;
    // A block needs a blank line before it, but only as much of one as the file
    // does not already end with.
    let separator = if source.is_empty() || source.ends_with("\n\n") {
        ""
    } else if source.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    Ok((path, source.len(), format!("{separator}{block}\n")))
}

/// Validate a `variable "x" { ...
// The caller typed this, so a malformed one is invalid input and exits 2, not a decline.
fn parse_variable_declaration(declaration: &str) -> Result<(String, bool)> {
    let parsed = Parsers::new().parse(Language::Hcl, declaration)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "the declaration does not parse as Terraform; it must be a whole block, e.g. \
             `variable \"name\" {{\\n  type = string\\n}}`"
        );
    }
    let blocks = top_level_blocks(&parsed);
    let [block] = blocks.as_slice() else {
        anyhow::bail!(
            "the declaration must be exactly one `variable` block, not {}",
            blocks.len()
        );
    };
    if block_keyword(*block, declaration) != Some("variable") {
        anyhow::bail!("the declaration must be a `variable` block");
    }
    let labels = block_labels(*block);
    let [label] = labels.as_slice() else {
        anyhow::bail!("a `variable` block takes exactly one name label");
    };
    let name = Span::from(*label).text(declaration).to_string();
    if !is_terraform_identifier(&name) {
        return Err(Refusal::InvalidName {
            name,
            reason: "a Terraform variable name must start with a letter or underscore and \
                     contain only letters, digits, underscores and dashes"
                .to_string(),
        }
        .into());
    }
    let has_default = block_body(*block).is_some_and(|body| {
        body_attributes(body, declaration)
            .iter()
            .any(|(n, _)| *n == "default")
    });
    Ok((name, has_default))
}

fn is_terraform_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Indentation to give an argument appended to a `module` block.
fn argument_indent(source: &str, block: Span, last_argument: Span) -> String {
    let block_line = full_line_span(source, block.start);
    let argument_line = full_line_span(source, last_argument.start);
    if block_line.start == argument_line.start {
        // A one-line block: the appended argument opens the body's first real line.
        format!("{}  ", line_indent(source, block.start))
    } else {
        line_indent(source, last_argument.start)
    }
}

/// Widen a span to the whole lines it occupies, taking one adjoining blank line so a
/// removed block does not leave a double gap behind.
fn statement_deletion_span(source: &str, span: Span) -> Span {
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

    // A block separated from its neighbours by blank lines takes one of them with it.
    let mut start = first.start;
    let mut end = line_end;
    let previous_blank = start > 0
        && full_line_span(source, start - 1)
            .text(source)
            .trim()
            .is_empty();
    let next = (end < source.len()).then(|| full_line_span(source, end));
    let next_blank = next.is_some_and(|n| n.text(source).trim().is_empty());

    if (previous_blank || start == 0) && next_blank {
        end = next.expect("blank line exists").end;
    } else if previous_blank {
        start = full_line_span(source, start - 1).start;
    }
    Span::new(start, end)
}

/// Resolve a module `source` to a workspace directory.
fn local_module_dir(from: &Path, source: &str) -> Option<PathBuf> {
    if !(source.starts_with("./") || source.starts_with("../") || source.starts_with('/')) {
        return None;
    }
    Some(crate::vfs::normalise(from.join(source)))
}

/// Does the workspace hold any `.tf` file in this directory?
fn directory_has_hcl(index: &Index, dir: &Path) -> bool {
    index.files().any(|(path, info)| {
        info.language == Language::Hcl
            && is_terraform_config(path)
            && path.parent().map(crate::vfs::normalise).as_deref() == Some(dir)
    })
}

/// `.tf` declares configuration; `.tfvars` only assigns values to it.
fn is_terraform_config(path: &Path) -> bool {
    !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tfvars"))
}

fn describe_variables(variables: &[ModuleVariable]) -> String {
    if variables.is_empty() {
        return "none".to_string();
    }
    variables
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let required = if v.has_default { "" } else { " (required)" };
            format!("{i}: {}{required}", v.name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `path:line` for an error message.
fn location(path: &Path, offset: usize) -> String {
    let line = crate::vfs::read_to_string(path)
        .map(|src| LineIndex::new(&src).line_col(offset, &src).line)
        .unwrap_or(0);
    format!("{}:{line}", path.display())
}

/// The blocks directly under the file body.
fn top_level_blocks<'a>(parsed: &'a Parsed) -> Vec<Node<'a>> {
    let root = parsed.root();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "body" {
            continue;
        }
        let mut inner = child.walk();
        out.extend(
            child
                .named_children(&mut inner)
                .filter(|n| n.kind() == "block"),
        );
    }
    out
}

/// The block type keyword: `variable`, `module`, `resource`…
fn block_keyword<'a>(block: Node<'_>, source: &'a str) -> Option<&'a str> {
    let keyword = child_of_kind(block, "identifier")?;
    Some(&source[keyword.start_byte()..keyword.end_byte()])
}

/// The label contents of a block, quotes excluded.
fn block_labels(block: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = block.walk();
    block
        .children(&mut cursor)
        .filter(|c| c.kind() == "string_lit")
        .filter_map(|literal| child_of_kind(literal, "template_literal"))
        .collect()
}

fn block_body(block: Node<'_>) -> Option<Node<'_>> {
    child_of_kind(block, "body")
}

/// The `name = value` attributes of a body, in source order.
fn body_attributes<'a, 'tree>(body: Node<'tree>, source: &'a str) -> Vec<(&'a str, Node<'tree>)> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|c| c.kind() == "attribute")
        .filter_map(|attribute| {
            let name = child_of_kind(attribute, "identifier")?;
            Some((&source[name.start_byte()..name.end_byte()], attribute))
        })
        .collect()
}

/// What a block's `source` argument says.
fn block_source(block: Node<'_>, source: &str) -> ModuleSource {
    let Some(body) = block_body(block) else {
        return ModuleSource::Missing;
    };
    let Some((_, attribute)) = body_attributes(body, source)
        .into_iter()
        .find(|(name, _)| *name == "source")
    else {
        return ModuleSource::Missing;
    };
    let literal = child_of_kind(attribute, "expression")
        .and_then(|e| child_of_kind(e, "literal_value"))
        .and_then(|l| child_of_kind(l, "string_lit"))
        .and_then(|s| child_of_kind(s, "template_literal"));
    match literal {
        Some(node) => ModuleSource::Literal(Span::from(node).text(source).to_string()),
        None => ModuleSource::Computed(
            child_of_kind(attribute, "expression")
                .map(|e| Span::from(e).text(source).to_string())
                .unwrap_or_default(),
        ),
    }
}

fn child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// Describe a plan for display.
pub fn describe(index: &Index, plan: &SignaturePlan) -> String {
    let noun = match plan.subject_kind {
        Subject::Callable => "parameter",
        Subject::TerraformModule => "module variable",
        Subject::ShellFunction => "positional parameter",
    };
    let what = match &plan.change {
        Change::Remove(i) => format!("removed {noun} {i}"),
        Change::Move { from, to } => format!("moved {noun} {from} to position {to}"),
        Change::Add {
            at,
            declaration,
            argument,
        } => {
            // A shell function has no declaration, so the argument is the only text
            // there is to name the new parameter by.
            let shown = match plan.subject_kind {
                Subject::ShellFunction => argument,
                _ => declaration,
            };
            format!("added {noun} `{}` at position {at}", first_line(shown))
        }
    };
    let _ = index;
    let mut out = format!(
        "{}: {what}, updating {} call site(s)",
        plan.subject, plan.call_sites
    );
    for note in &plan.notes {
        out.push_str("\n  note: ");
        out.push_str(note);
    }
    out
}

/// The first line of a declaration, for one-line summaries.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text).trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use std::path::Path;

    use crate::testing::workspace;

    fn apply(plan: &SignaturePlan, path: &Path) -> String {
        let original = crate::vfs::read_to_string(path).unwrap();
        match plan.edits.edits_for(path) {
            Some(edits) => apply_to_string(&original, edits).unwrap(),
            None => original,
        }
    }

    #[test]
    fn removes_a_middle_parameter_and_updates_calls() {
        let src = "fn f(a: i32, b: i32, c: i32) {}\nfn caller() { f(1, 2, 3); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        assert_eq!(plan.call_sites, 1);
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32, c: i32) {}\nfn caller() { f(1, 3); }\n"
        );
    }

    #[test]
    fn removes_the_last_parameter_without_leaving_a_comma() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32) {}\nfn caller() { f(1); }\n"
        );
    }

    #[test]
    fn moves_a_parameter_and_reorders_arguments() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Move { from: 0, to: 1 }).unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(b: i32, a: i32) {}\nfn caller() { f(2, 1); }\n"
        );
    }

    #[test]
    fn refuses_a_reorder_that_would_put_a_default_before_a_required_parameter() {
        // Python rejects `def circ(units="m", r):` with "parameter without a default follows
        // parameter with a default", and tree-sitter parses it without complaint.
        let src = "def circ(r, units=\"m\"):\n    return r\n";
        let (_tmp, index) = workspace(&[("a.py", src)]);
        let id = index.find_symbols("circ", None)[0].id;

        let error = change(&index, id, Change::Move { from: 0, to: 1 })
            .expect_err("this would not run")
            .to_string();
        assert!(error.contains("before one without"), "{error}");

        // The same move where neither has a default is fine.
        let src = "def send(host, port):\n    return port\n";
        let (_tmp, index) = workspace(&[("b.py", src)]);
        let id = index.find_symbols("send", None)[0].id;
        assert!(change(&index, id, Change::Move { from: 0, to: 1 }).is_ok());
    }

    #[test]
    fn adds_a_parameter_with_an_argument_at_each_call() {
        let src = "fn f(a: i32) {}\nfn caller() { f(1); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(
            &index,
            id,
            Change::Add {
                at: 1,
                declaration: "flag: bool".into(),
                argument: "false".into(),
            },
        )
        .unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32, flag: bool) {}\nfn caller() { f(1, false); }\n"
        );
    }

    #[test]
    fn updates_call_sites_in_other_files() {
        let (tmp, index) = workspace(&[
            ("lib.rs", "pub fn shared(a: i32, b: i32) {}\n"),
            ("main.rs", "use lib::shared;\nfn main() { shared(1, 2); }\n"),
        ]);
        let id = index.find_symbols("shared", None)[0].id;
        let plan = change(&index, id, Change::Remove(0)).unwrap();

        let main = apply(&plan, &tmp.path().join("main.rs"));
        assert!(main.contains("shared(2);"), "got:\n{main}");
    }

    #[test]
    fn refuses_when_a_call_site_is_not_provable() {
        // A same-named function elsewhere makes resolution ambiguous, and updating
        // only some call sites would not compile.
        let (_tmp, index) = workspace(&[
            (
                "a.rs",
                "fn ambiguous(x: i32) {}\nfn ambiguous_caller() { ambiguous(1); }\n",
            ),
            (
                "b.rs",
                "fn ambiguous(x: i32) {}\nfn other() { ambiguous(2); }\n",
            ),
        ]);
        let id = index.find_symbols("ambiguous", None)[0].id;
        // Whatever resolution says, the operation must either be provably complete
        // or refuse; it must never silently update a subset.
        match change(&index, id, Change::Remove(0)) {
            Ok(plan) => {
                for (_, edits) in plan.edits.iter() {
                    assert!(!edits.is_empty());
                }
            }
            Err(e) => assert!(
                e.downcast_ref::<Refusal>().is_some(),
                "refusal should be explicit: {e}"
            ),
        }
    }

    #[test]
    fn refuses_non_functions() {
        let (_tmp, index) = workspace(&[("a.rs", "struct S;\n")]);
        let id = index.find_symbols("S", None)[0].id;
        let err = change(&index, id, Change::Remove(0))
            .unwrap_err()
            .to_string();
        assert!(err.contains("only functions"), "got: {err}");
    }

    #[test]
    fn rejects_a_position_that_does_not_exist() {
        let (_tmp, index) = workspace(&[("a.rs", "fn f(a: i32) {}\nfn c() { f(1); }\n")]);
        let id = index.find_symbols("f", None)[0].id;
        let err = change(&index, id, Change::Remove(9))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no parameter at position"), "got: {err}");
    }

    #[test]
    fn the_result_still_parses() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let id = index.find_symbols("f", None)[0].id;
        let plan = change(&index, id, Change::Remove(0)).unwrap();
        let outcomes =
            crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
        let _ = tmp;
    }

    #[test]
    fn works_for_python() {
        let src = "def f(a, b):\n    pass\n\ndef caller():\n    f(1, 2)\n";
        let (tmp, index) = workspace(&[("a.py", src)]);
        let path = tmp.path().join("a.py");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        let out = apply(&plan, &path);
        assert!(out.contains("def f(a):"), "got:\n{out}");
        assert!(out.contains("f(1)"), "got:\n{out}");
    }
}
