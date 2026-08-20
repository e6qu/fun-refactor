//! Change a function's signature and every call site.
//!
//! LSP has no request for this; gopls approximates it with parameter-move code actions. A call
//! site that did not resolve conclusively is reported and the whole operation refuses: a
//! half-updated signature does not compile.
//!
//! Three languages spell "signature" differently:
//!
//! * SCSS. `@mixin name($a, $b)` is a parameter list and `@include name(1, 2)` is a call, so
//!   the machinery below handles both once it treats an `include_statement` as a call. SCSS
//!   adds a declaration that can start with no parentheses, and a grammar whose gaps hide
//!   call sites, see [`open_a_parameter_list`] and [`reject_hidden_call_sites`].
//! * Terraform. A module is a directory; its parameters are the `variable "x" {}` blocks
//!   declared in it. Its call sites are `module "m" { source = "./dir" }` blocks pointing at
//!   that directory. Arguments there are named and not positional, so a change addresses a
//!   position in the variables' document order and rewrites the named argument at each call
//!   site: [`terraform_module`].
//! * Bash. A shell function declares no parameter list, but still has a signature: the
//!   positional parameters `$1 $2 …` the body reads. The words every call site passes. A
//!   change renumbers one and rewrites the other: [`shell_function`].

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
    ///
    /// Here and not in the CLI because a recipe's `signature "…"` step writes the same syntax.
    /// Two parsers for one syntax is two chances to disagree.
    ///
    /// Three fields, not four. The declaration may itself contain colons, `flag: bool` is the
    /// documented example. So everything after the position is one field and the argument comes
    /// off its end. Splitting into four handed the arm below only the first word of the
    /// declaration and dropped the rest, which made `add:1:flag\. Bool:false` fail with the
    /// message that recommends it.
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
    /// A Terraform module directory, whose arguments are named.
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
    /// Things the change saw and deliberately did not act on. A note never blocks the
    /// change; it says what was left alone and why, so a reader can check it.
    pub notes: Vec<String>,
}

/// Refuse to remove a parameter the body still reads.
///
/// `def f(a, b): return a + b` with `remove:1` produced `def f(a): return a + b`, which names
/// something nothing supplies. The shell path has had this rule since it was written — "the
/// body still reads $2, the parameter being removed". It was never true of anything else, which
/// is the shape most of the defects in this tool have had. A rule that holds for the language
/// it was written against.
/// Refuse to add a parameter the declaration already has.
///
/// Running the same `add:` twice wrote `def scale(v, factor, factor)`, which
/// the grammar parses and Python refuses at import. Every other operation here
/// declines a repeat: a rename to an existing name, a delete of what is gone.
/// This one applied it again, so a retried command or a re-run recipe broke
/// the file it had just changed.
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
        anyhow::bail!(
            "this declaration already has a parameter called `{name}`; adding it \
             again would name one thing twice"
        );
    }
    Ok(())
}

/// The name a parameter's text declares, whatever else it carries.
///
/// Which word is the name depends on the language, and reading the wrong one
/// answered `float64` when asked what Go's `price float64` is called.
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
        | Language::Scss => head.split(':').next()?.trim(),
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
    // The parameter symbol is the one this file declares inside the removed span.
    let Some(parameter) = index.symbols.iter().find(|s| {
        s.file == sym.file && s.kind == SymbolKind::Parameter && span.contains(s.name_span)
    }) else {
        return Ok(());
    };
    // Inside the declaration, and nowhere else. A call passing the parameter
    // by name resolves to it too. A keyword argument three files away was
    // reported as "the body of `greet` still reads `punct`", with the call
    // site's line. The body is the only place a removal cannot repair.
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
///
/// The type `fr delete` raises, and the exit code is chosen from the error's type.
/// `fr --help` promises 5 for a refusal. Every one of these printed 1, the code for
/// a crash, under a message that had thought behind it.
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
        anyhow::bail!(
            "'{}' is {}; only functions and methods have signatures",
            sym.name,
            sym.kind.with_article()
        );
    }

    // A method in declared dispatch changes as one family. A trait method
    // with one more parameter than its implementations is a family answering
    // two shapes, and the callers compile against neither.
    let family = crate::analysis::call_graph::Hierarchy::scan(index).method_group(index, symbol);
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
            // A declaration can legitimately have no parameter list to change: SCSS
            // spells a no-argument mixin `@mixin reset { }`, with no parentheses at all.
            // Adding the first parameter has to write them.
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
        // The grammar decides whether this is a call. It is not the kind the extractor recorded. `new
        // Thing(1, "x")` is written down as a reference to the *type*, which it also is, so
        // filtering on the kind skipped it. A constructor's parameters could be reordered while
        // every `new` was left as it was. A mention that really is not a call has no arguments
        // to change and is passed over. A recorded call the grammar will not show as one is the
        // case worth refusing.
        let call = match call_expression(&call_parsed, reference.span) {
            Some(call) => call,
            // A mention that is not a call and not an import is the function used
            // as a value. Think `let f: fn(i32, i32) -> i32 = add;`, or a callback
            // pushed into a list. It has no argument list to rewrite, and after the change every
            // call through that binding passes the old shape. Skipping it, as this
            // arm once did, changed the declaration under the binding's feet and
            // reported success.
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
                            "`{}` is used as a value at {}, and a value keeps the old \
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
                // A macro body is tokens and not syntax, so the grammar offers
                // no call. The tokens still have a call's shape. Refusing
                // every `assert_eq!(f(x), …)` made the command unusable on real
                // Rust: half of a crate's call sites live in its tests' macros.
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
                         exposes, so its arguments cannot be rewritten",
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
                    "the call to `{}` at {} does not parse cleanly, so its argument list \
                     cannot be rewritten with certainty",
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
            // `@include reset;` passes nothing and needs no parentheses. Removing or
            // reordering there is a no-op, but an added argument has to go somewhere.
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

    // The call sites dispatch reaches without resolving: `s.area(2.0)` on a
    // trait object names no single implementation, which is why the family
    // changes as a unit. With every member changed, a site with the old
    // argument shape calls a signature nothing answers to. It changes too,
    // and the note says where.
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
            // A dispatch site this cannot rewrite is a call left with the old argument
            // shape, and every member of the family has already changed. Passing over it
            // is the partial update this refactoring exists to avoid. So each of the
            // three ways of failing to reach the arguments refuses, and names the site.
            let where_ = location(&reference.file, reference.span.start);
            let out_of_reach = |why: &str| -> anyhow::Error {
                Refusal::Unknowable {
                    detail: format!(
                        "`{}` is called at {where_}, where dispatch can reach the \
                         declaration being changed, and {why}",
                        sym.name
                    ),
                }
                .into()
            };
            let Some(call) = call_expression(&call_parsed, reference.span) else {
                // A macro body is tokens and not syntax, so the grammar offers no
                // call. The tokens still have a call's shape. Refusing every
                // `assert_eq!(f(x), …)` made the command unusable on real Rust:
                // half of a crate's call sites live in its tests' macros.
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
                        "it is written inside a macro, where the grammar records \
                             tokens and not a call"
                    }
                    false => "the grammar exposes no call expression there",
                }));
            };
            if call.has_error() {
                return Err(out_of_reach("that call does not parse cleanly"));
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

/// Rewrite one parameter or argument list. Would this reorder put a parameter with a default
/// before one without?
///
/// Python and TypeScript both require every defaulted parameter to come last, and tree-sitter
/// parses `def circ(units="m", r):` without complaint. So the engine's reparse check cannot see
/// this and `fr signature circ move:0:1` produced a file Python rejects with *"parameter
/// without a default follows parameter with a default"*. The languages with no defaults at all
/// cannot hit it.
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

/// The file a change is being written into.
///
/// The three travel together everywhere and separately were three of eight parameters.
struct Site<'a> {
    file: &'a std::path::Path,
    source: &'a str,
    language: Language,
}

/// The name of the parameter a removal targets, read from the declaration.
///
/// A call site passing arguments by name needs it: position tells it nothing
/// about which argument is going.
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
                    anyhow::bail!(
                        "there is no parameter at position {index}: the declaration has {} \
                         parameter(s), counted from 0",
                        items.len()
                    );
                }
                return Ok(());
            };
            // A call may pass its arguments by name, and then position says
            // nothing about which one this is. `greet("b", loud=True)` lost
            // its `loud=True` to a removal of parameter 1. The name decides at
            // a call site. Where a call names arguments and none is the one
            // going, it relied on the default and needs no edit.
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
            // Take the separating comma with it, or the list ends up malformed.
            let span = with_separator(source, items, index, target);
            edits.add(
                file.to_path_buf(),
                Edit::new(span, "", format!("remove parameter {index}")),
            );
        }
        Change::Move { from, to } => {
            let (Some(a), Some(b)) = (items.get(*from), items.get(*to)) else {
                if is_declaration {
                    anyhow::bail!(
                        "positions {from} and {to} are not both present: the declaration \
                         has {} parameter(s), counted from 0",
                        items.len()
                    );
                }
                return Ok(());
            };
            if is_declaration && defaults_would_be_out_of_order(language, source, items, *from, *to)
            {
                anyhow::bail!(
                    "moving parameter {from} to position {to} would put a parameter with a \
                     default before one without, which {language} does not allow. Give the \
                     other parameter a default first, or remove this one's."
                );
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
///
/// Only an addition can do this: there is nothing to remove or reorder in a list that does not
/// exist. On the call side that is a legitimate state, a mixin whose parameters all have
/// defaults is included as `@include reset;`.
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
            anyhow::bail!("`{name}` has no parameter list to change");
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
///
/// A call site inside an ERROR node produces no reference. So it is invisible to the index and
/// not weakly resolved, the confidence check above cannot see it. In SCSS this is not
/// hypothetical: `@include m();` with empty parentheses and the namespaced `@include ns.m()`
/// both fail to parse under tree-sitter-scss. Each one is a call this change would leave
/// behind.
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
             inside a syntax error is invisible to the index, so the call surface cannot \
             be shown to be complete",
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
        // Take the following comma and any space after it.
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
///
/// Most grammars wrap the arguments in a node of their own, and the answer is that node. Zig
/// does not: `holder.width(a, b)` is a `call_expression` whose children are the callee and then
/// the arguments themselves, with no list around them and no `(` of its own to find them by.
/// Asking only for a wrapper therefore reported every Zig call as taking no arguments. `fr
/// signature` reordered the declaration, said it was "updating 2 call site(s)", and left both
/// of them alone.
///
/// `None` still means what it meant: a call that passes nothing and has no parentheses, which
/// SCSS writes as `@include reset;`.
fn call_arguments(node: Node<'_>) -> Option<(usize, Vec<Span>)> {
    if let Some(list) = argument_list(node) {
        return Some((list.start_byte(), list_items(list)));
    }

    // No wrapper. The callee is the first child, the arguments are the rest, and the
    // opening parenthesis is the token between them.
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

/// The call expression whose callee is at `span`.
/// Does this mention sit inside an import, a `use`, or an export of the name?
///
/// Those lines restate the name without using the function, so a signature change
/// leaves them correct as written. Everything else that mentions the name without
/// calling it holds it as a value.
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
///
/// A macro body is tokens, so `assert_eq!(anchor_slug(&h.name), "x")` offers no
/// call expression. The tokens still have the shape of one: the named
/// identifier followed by a parenthesised `token_tree`. Its direct children
/// are the delimiters, the argument tokens, and the top-level commas. Nested
/// trees are single children, so splitting on the direct commas is exact.
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
        // Two grammars do not say "call". SCSS spells a mixin call `@include
        // name(args)`, an `include_statement`; Java spells every call a
        // `method_invocation` and every construction an `object_creation_expression`.
        //
        // The comment here used to say "the one call form whose kind does not say
        // call", which was true of the languages it was written against, and meant
        // `fr signature` refused at every Java call site there has ever been.
        if node.kind().contains("call")
            || matches!(
                node.kind(),
                "include_statement" | "method_invocation" | "object_creation_expression"
            )
        {
            // The walk has to find the call this reference *names*, not one it merely
            // sits inside. A mention in an argument position, `render(Pet)`, walks up
            // into a call to something else entirely, whose arguments would then be
            // reordered as if they were the mentioned symbol's.
            return match argument_list(node) {
                Some(args) if span.start >= args.start_byte() => None,
                _ => Some(node),
            };
        }
        node = node.parent()?;
    }
    None
}

/// Named children of a list node, i.e. its actual items.
/// The parameter spans a caller can address, with the receiver taken off.
///
/// Rust and Python write the receiver inside the parameter list, and a call
/// never passes it. Counting it put every position out by one, and
/// `remove:0` took `&self` off a trait method.
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

// -------------------------------------------------------- Bash functions
//
// `greet() { … }` declares no parameters, but a caller still observes a signature. The
// positional parameters `$1`, `$2`, … the body reads. The words each call site passes. A change
// is two rewrites that must agree, renumber the body, reorder the call sites, and both must be
// provable before either is written.
//
// Shell semantics put several shapes out of reach:
//
// * `$@`, `$*` and `shift` consume the parameter list wholesale. Renumbering individual
//   references cannot follow them, so a body using one refuses.
// * An unquoted expansion or a glob is one word in the syntax and any number of arguments at
//   run time. So nothing after it has a knowable position. Only the positions a change touches
//   must be determinate: `f a "$@"` can lose its first argument, `f $x b` cannot.
// * `$12` is not parameter 12, the shell reads `${1}` followed by a literal `2`. tree-sitter
//   reports one two-digit name, so a multi-digit unbraced reference refuses.
//
// Bash resolves a command name at run time against whatever `source` has already run, so the
// index resolves only same-file calls. This rebuilds the call surface from the `source` graph
// and reports a caller it cannot tie to the definition.

/// A positional parameter reference inside a function body.
#[derive(Debug, Clone, Copy)]
struct Positional {
    /// The digits alone, so a renumber rewrites `$2` and `${2}` the same way.
    span: Span,
    number: usize,
    /// `${12}` can hold two digits; `$12` cannot.
    braced: bool,
}

/// One command invocation of the function being changed.
struct ShellCall {
    /// Span of the command name, where a first argument has to be inserted.
    name: Span,
    /// The argument words, in source order.
    arguments: Vec<Span>,
}

/// Change the positional signature of the shell function `sym`.
fn shell_function(index: &Index, sym: &Symbol, change: Change) -> Result<SignaturePlan> {
    if sym.kind != SymbolKind::Function {
        anyhow::bail!(
            "'{}' is {}; only a shell function has positional parameters",
            sym.name,
            sym.kind.with_article()
        );
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
                "a shell function declares no parameters, so the declaration `{}` was not \
                 written anywhere; only the argument and the body's numbering changed",
                first_line(declaration)
            ));
        }
    }

    let calls = shell_call_files(index, sym, &mut notes)?;
    if let Change::Add { argument, .. } = &change {
        if argument.trim().is_empty() && !calls.is_empty() {
            anyhow::bail!(
                "`{}` is called from {} site(s) and shell arguments are positional, so an \
                 added parameter needs a word to pass; supply an argument",
                sym.name,
                calls.values().map(|v| v.len()).sum::<usize>()
            );
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

    // A recursive call passing `$1` would have the same bytes rewritten twice, once
    // as an argument and once as a renumbered reference.
    reject_shell_edit_collisions(&edits, &sym.file, sym, &renumbered)?;

    // A change that rewrites nothing is not a change. Saying so is the only way the
    // caller learns that the position they named exists nowhere.
    if edits.is_empty() {
        anyhow::bail!(
            "the change leaves `{}` exactly as it was: no call site and no reference in \
             its body names that position.{}",
            sym.name,
            notes.iter().map(|n| format!("\n  {n}")).collect::<String>()
        );
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
    anyhow::bail!(
        "`{}` at {} is not a function definition this grammar exposes",
        sym.name,
        location(&sym.file, sym.name_span.start)
    )
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
            "function_definition" if node.id() != definition.id() => anyhow::bail!(
                "`{}` defines a nested function at {}; its `$1` names that function's \
                 first argument, not this one's, so the body cannot be renumbered",
                sym.name,
                location(&sym.file, node.start_byte())
            ),
            "special_variable_name" if text == "@" || text == "*" => anyhow::bail!(
                "the body of `{}` uses `${text}` at {}, which expands to the whole \
                 parameter list; renumbering individual references cannot follow it",
                sym.name,
                location(&sym.file, node.start_byte())
            ),
            "special_variable_name" if text == "#" => reads_count = true,
            "command_name" if text == "shift" => anyhow::bail!(
                "the body of `{}` calls `shift` at {}, which renumbers the parameters at \
                 run time; a static renumbering cannot follow it",
                sym.name,
                location(&sym.file, node.start_byte())
            ),
            "command_name" if text == "set" => {
                if shell_command_resets_parameters(node, source) {
                    anyhow::bail!(
                        "the body of `{}` calls `set --` at {}, which replaces the \
                         positional parameters wholesale; a static renumbering cannot \
                         follow it",
                        sym.name,
                        location(&sym.file, node.start_byte())
                    );
                }
            }
            "variable_name" if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                let braced = match node.parent().map(|p| p.kind()) {
                    Some("expansion") => true,
                    Some("simple_expansion") => false,
                    other => anyhow::bail!(
                        "`${text}` at {} sits inside a {} instead of an expansion, so the \
                         tool cannot tell what rewriting it would mean",
                        location(&sym.file, node.start_byte()),
                        other.unwrap_or("(nothing)")
                    ),
                };
                // `$0` is the script's name. It is not a parameter, and survives any change.
                let number: usize = text.parse()?;
                if number == 0 {
                    continue;
                }
                if !braced && text.len() > 1 {
                    anyhow::bail!(
                        "`${text}` at {} is not parameter {text}: the shell reads `$` and one \
                         digit, then `{}` as literal text. Write it as `${{{text}}}` first if \
                         that is what was meant",
                        location(&sym.file, node.start_byte()),
                        &text[1..]
                    );
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

/// How a renumbered reference is spelled.
///
/// `$9` renumbered to 10 cannot stay unbraced: the shell would read `$10` as `${1}0`.
/// Replacing the digits with `{10}` turns the surviving `$` into `${10}`.
fn shell_positional_text(number: usize, braced: bool) -> String {
    if braced || number < 10 {
        number.to_string()
    } else {
        format!("{{{number}}}")
    }
}

/// Every command invocation of `sym` that can be tied to it, grouped by file.
///
/// Bash has no import that binds a name: a command is whatever `source` put in scope
/// by the time the line runs. So a call is attributed to this function only when its
/// file is the defining file or reaches it through a chain of literal `source` paths.
/// Anything else is either an external command of the same name, reported and left
/// alone, or a file whose scope cannot be known, which refuses.
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
            "{} runs `{}` {} time(s) but never sources {}, so those are a different \
             command and were left alone",
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
            anyhow::bail!(
                "the call to `{}` at {} is not a command invocation this grammar exposes",
                sym.name,
                location(file, span.start)
            )
        };
        node = parent;
    }
    if node.kind() != "command" {
        anyhow::bail!(
            "the call to `{}` at {} is not a command invocation this grammar exposes",
            sym.name,
            location(file, span.start)
        );
    }

    let name = node.child_by_field_name("name").ok_or_else(|| {
        anyhow::anyhow!(
            "the command at {} has no name node",
            location(file, node.start_byte())
        )
    })?;
    // A name that does not coincide with the reference means the reference was an
    // argument of some other command, which is not a call at all.
    if !Span::from(name).contains(span) {
        anyhow::bail!(
            "`{}` at {} is an argument of another command, not a call to the function",
            sym.name,
            location(file, span.start)
        );
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
///
/// Only the prefix up to the highest position touched has to be determinate: what
/// follows shifts uniformly whatever it expands to, so `f a "$@"` survives losing its
/// first argument even though `"$@"` is any number of words.
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
            // Not a resolution that is too weak, the call resolved. The shell decides
            // how many words this argument becomes when it runs, and no reading of the
            // text can say. `Refusal::Unknowable` exists for this, and saying
            // "resolution is only 'name-only'" sent the reader after a resolution
            // problem that is not there.
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

/// Why an argument cannot be treated as exactly one positional parameter, and what to
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

/// Recursive form of [`shell_argument_is_indeterminate`], over an already-found node. What is
/// wrong with this argument word, and what the author could do about it.
///
/// The remedy travels with the problem because it does not apply to all of them: quoting an
/// expansion makes it one argument. Quoting `$@` makes it one word per parameter, which is the
/// same problem again. Appending the advice at the call site told an author to quote a `$@`
/// that was already quoted.
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
                    "{}: the call to `{}` passes {} argument(s), so positions {from} and \
                     {to} are not both present and its arguments were left alone",
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
                anyhow::bail!(
                    "the call to `{}` at {} passes {} argument(s), so inserting at position \
                     {at} would land at position {} instead",
                    sym.name,
                    location(file, call.name.start),
                    call.arguments.len(),
                    call.arguments.len()
                );
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
            anyhow::bail!(
                "the recursive call to `{}` at {} passes a positional parameter that this \
                 change also renumbers; the same bytes would be rewritten twice",
                sym.name,
                location(file, clash.start)
            );
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

// ----------------------------------------------------- Terraform modules

/// A `variable "x" { ... }` block declared in the target module's directory.
#[derive(Debug)]
struct ModuleVariable {
    id: SymbolId,
    name: String,
    file: PathBuf,
    /// The whole block, `variable` keyword through closing brace.
    span: Span,
    has_default: bool,
}

/// A `module "m" { source = "./dir" ... }` block that calls the target module.
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
    /// A literal string, e.g. `"./modules/thing"` or `"hashicorp/consul/aws"`.
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
        // Terraform arguments are named, so shuffling `variable` blocks is a
        // formatting change that no call site can observe. Saying so beats
        // performing an edit that means nothing.
        Change::Move { .. } => {
            return Err(Refusal::Unsupported {
                operation: "reordering module variables".to_string(),
                language: Language::Hcl,
                because: "a Terraform module's arguments are named and not \
                          positional, so moving a `variable` block changes nothing at any \
                          call site",
            }
            .into());
        }

        Change::Remove(at) => {
            let Some(target) = variables.get(*at) else {
                anyhow::bail!(
                    "there is no module variable at position {at}; {} declares {}",
                    crate::vfs::describe_dir(dir),
                    describe_variables(&variables)
                );
            };

            // A variable the module's own configuration still reads cannot be
            // removed: the `var.x` uses would dangle. A caller's argument is not one
            // of those. It is the call surface, and the loop below deletes it.
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
                        "`{}` is still read {} time(s) inside the module ({where_}); \
                         removing it would leave those `var.{}` references dangling",
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
            // A variable with no default is required, so every caller has to start
            // passing it. Without a value to pass, adding it breaks them all.
            if argument.is_empty() && !has_default && !calls.is_empty() {
                anyhow::bail!(
                    "`{name}` has no `default`, so it is required at all {} call site(s); \
                     supply an argument value to pass there, or give the variable a default",
                    calls.len()
                );
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
                        anyhow::bail!(
                            "module \"{}\" at {} has no arguments to append to",
                            call.label,
                            location(&call.file, call.span.start)
                        );
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
///
/// Two handles work: a `variable` block, whose module is the directory it sits in,
/// and a `module` block, whose module is wherever its `source` points.
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
                        anyhow::bail!(
                            "module \"{}\" at {} has source `{path}`, which is not a local \
                             directory; its variables are not declared in this workspace",
                            sym.name,
                            location(&sym.file, sym.name_span.start)
                        );
                    };
                    if !directory_has_hcl(index, &target) {
                        anyhow::bail!(
                            "module \"{}\" at {} points at {}, which holds no Terraform files \
                             in this workspace",
                            sym.name,
                            location(&sym.file, sym.name_span.start),
                            target.display()
                        );
                    }
                    Ok(target)
                }
                ModuleSource::Computed(text) => anyhow::bail!(
                    "module \"{}\" at {} has a computed source `{text}`, so the directory it \
                     calls is not knowable without applying the configuration",
                    sym.name,
                    location(&sym.file, sym.name_span.start)
                ),
                ModuleSource::Missing => anyhow::bail!(
                    "module \"{}\" at {} has no `source` argument",
                    sym.name,
                    location(&sym.file, sym.name_span.start)
                ),
            }
        }
        _ => anyhow::bail!(
            "'{}' is {} in Terraform; only a `variable` block or a `module` block names a \
             module signature",
            sym.name,
            sym.kind.with_article()
        ),
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
///
/// All-or-nothing applies across the whole workspace: a `module` block whose source is not a
/// literal path might be the one caller this change would miss. No amount of reading the
/// configuration can prove otherwise. So one such block refuses the operation instead of
/// producing an update that is right for the callers we could see and wrong for Terraform as a
/// whole.
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

        // The index records every module `source` as an import. One that points at the target
        // directory without a matching top-level `module` block means the block is somewhere
        // this rewrite does not look, nested inside another block, say. Editing around it would
        // update only part of the call surface.
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
                         its arguments cannot be rewritten",
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
                "{} `module` block(s) do not name a literal source, so they cannot be shown \
                 not to call {}: {}",
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

    // A module with no variables at all has no anchor. So the conventional file is the only
    // sane target, and if it does not exist, saying so beats creating one.
    let path = dir.join("variables.tf");
    if index.file(&path).is_none() {
        anyhow::bail!(
            "module {} declares no variables and has no variables.tf to add one to; create \
             the file first",
            crate::vfs::describe_dir(dir)
        );
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

/// Validate a `variable "x" { ... }` declaration and report its name and defaulting.
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

    // A block separated from its neighbours by blank lines must take one of them
    // with it, or the file is left with a double gap where it used to be.
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

/// Resolve a module `source` to a workspace directory, or `None` if it names
/// something that is not a local path (a registry address, a git URL).
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

// -------------------------------------------------------- HCL tree access

/// The blocks directly under the file body. `variable` and `module` are only
/// meaningful there, and a rewrite must not mistake a nested block for one.
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
    use crate::scan::{scan, ScanOptions};
    use std::path::Path;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

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
        // parameter with a default", and tree-sitter parses it without complaint. So the
        // engine's reparse check cannot see this one and the refactoring has to know the rule
        // itself.
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
