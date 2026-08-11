//! Cascading cleanup: remove a flag and everything that only existed to serve it.
//!
//! Deleting a feature flag is never one edit. The flag's uses become constants, the
//! conditionals around them collapse to whichever branch survives, and whatever only
//! that dead branch referenced becomes unused in turn. Uber's Piranha showed this
//! chain is what makes flag removal worth automating — the first edit is trivial and
//! the cascade is the work.
//!
//! Each round re-indexes the rewritten sources, so every decision is made against
//! what the code actually says now instead of a prediction of it. The cascade stops
//! when a round changes nothing.
//!
//! A cascade that cannot finish still runs: the substitution stays, and everything
//! left undone is named in [`CascadePlan::unfinished`]. Half a cleanup that says
//! which half it is beats refusing the whole operation.

use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::SymbolKind;
use crate::parse::{Parsed, Parsers};
#[cfg(feature = "cli")]
use crate::scan::{scan, ScanOptions};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, HashSet};
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
    /// Everything the cascade left behind, each named with the place it gave up.
    pub unfinished: Vec<String>,
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

/// Does the collapse step know this language's conditionals?
///
/// Substituting the flag needs nothing but the index, so it works wherever a symbol
/// resolves. Collapsing the conditional the substitution just made constant needs
/// the grammar's `if` shape, and that is what this list is.
pub fn supports_cascade(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Bash
            | Language::Hcl
            | Language::Java
    )
}

/// Remove `flag`, assuming it always had `value`, and clean up what follows.
///
/// Walks a workspace root, so it needs a filesystem. [`remove_flag_in`] is the same
/// refactoring over sources already in memory, which is what the browser calls and
/// what this delegates to once it has read them.
#[cfg(feature = "cli")]
pub fn remove_flag(root: &Path, flag: &str, value: bool) -> Result<CascadePlan> {
    let scanned = scan(root, &ScanOptions::default())?;

    let mut sources: BTreeMap<PathBuf, (Language, String)> = BTreeMap::new();
    for file in &scanned.files {
        let Ok(text) = crate::vfs::read_to_string(&file.path) else {
            continue;
        };
        sources.insert(file.path.clone(), (file.language, text));
    }
    remove_flag_in(sources, flag, value)
}

/// [`remove_flag`] over sources already held in memory.
///
/// The cascade never needed a filesystem — it rewrites in memory and re-indexes each
/// round — so the only thing the root was ever for was finding the files. Splitting
/// that off is what lets the browser build do this at all, and it is also why the
/// whole module stopped being dead code there: it had exactly one entry point, and
/// that entry point took a path.
pub fn remove_flag_in(
    sources: BTreeMap<PathBuf, (Language, String)>,
    flag: &str,
    value: bool,
) -> Result<CascadePlan> {
    let mut sources = sources;
    // The originals are kept to diff against at the end.
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

    // Which imports were unused before any of this started. Only an import the cascade
    // itself killed is the cascade's business; one that was already dead is a tidy-up of
    // the whole workspace, which is `fr imports` and not this. The same rule
    // `initially_used` applies to symbols.
    let already_dead_imports: HashSet<(PathBuf, Span)> = {
        let snapshot: Vec<(PathBuf, Language, String)> = sources
            .iter()
            .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
            .collect();
        let index = Index::build_from_sources(&snapshot)?;
        sources
            .iter()
            .flat_map(|(path, (_, text))| {
                dead_import_spans(&index, path, text)
                    .into_iter()
                    .map(|(span, _)| (path.clone(), span))
            })
            .collect()
    };

    let mut rounds = Vec::new();
    let mut removed_definition = false;
    let mut unfinished = Vec::new();
    // Read once, in the round that still has the declaration to read it from. Every
    // later round asks about uses of a flag whose definition has already gone.
    let mut flag_kind = SymbolKind::Constant;

    for round in 0..MAX_ROUNDS {
        let snapshot: Vec<(PathBuf, Language, String)> = sources
            .iter()
            .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
            .collect();
        let index = Index::build_from_sources(&snapshot)?;

        // Round 1 substitutes the flag; later rounds only tidy what that exposed.
        let changes = if !removed_definition {
            match substitute_flag(&index, &sources, flag, value)? {
                None if round == 0 => {
                    anyhow::bail!("no symbol named '{flag}' to remove; nothing was changed")
                }
                None => Vec::new(),
                Some((substituted, kind)) => {
                    flag_kind = kind;
                    removed_definition = true;
                    // An empty substitution means the flag is there and not one use of
                    // it could be rewritten. The loop ends and every reason is reported.
                    if !substituted.is_empty() {
                        rounds.push(RoundSummary {
                            description: format!("replaced uses of {flag} with {value}"),
                            files_touched: distinct_files(&substituted),
                        });
                    }
                    substituted
                }
            }
        } else {
            let simplified = simplify_constants(&sources, &originals)?;
            if !simplified.is_empty() {
                rounds.push(RoundSummary {
                    description: "collapsed conditionals whose test is now constant".into(),
                    files_touched: distinct_files(&simplified),
                });
                simplified
            } else {
                let orphans = remove_orphans(&index, &sources, flag, &initially_used)?;
                if !orphans.is_empty() {
                    rounds.push(RoundSummary {
                        description: "removed symbols nothing uses any more".into(),
                        files_touched: distinct_files(&orphans),
                    });
                    orphans
                } else {
                    let imports = remove_dead_imports(&index, &sources, &already_dead_imports)?;
                    if imports.is_empty() {
                        break;
                    }
                    rounds.push(RoundSummary {
                        description: "removed imports nothing uses any more".into(),
                        files_touched: distinct_files(&imports),
                    });
                    imports
                }
            }
        };

        if changes.is_empty() {
            break;
        }
        apply_in_memory(&mut sources, &changes)?;
    }

    unfinished.extend(remaining_uses(&sources, flag, flag_kind)?);
    unfinished.extend(unfinished_work(&sources, &originals)?);
    unfinished.extend(dangling_resource_uses(&sources, &originals)?);
    unfinished.sort();
    unfinished.dedup();

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

    // A cascade that changed nothing is a refusal, and reporting it as a plan of zero
    // edits would read as success. The reasons are already gathered, so they are what
    // the caller is told.
    if edits.is_empty() {
        anyhow::bail!(
            "nothing about '{flag}' could be removed; nothing was changed:\n  {}",
            unfinished.join("\n  ")
        );
    }

    Ok(CascadePlan {
        flag: flag.to_string(),
        value,
        edits,
        rounds,
        unfinished,
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
///
/// Uses it will not touch are left alone, and [`remaining_uses`] names them all against
/// the text the caller will actually be looking at.
///
/// The definition goes only when every use went with it. A use the substitution
/// declined still reads the flag, and deleting the declaration under it changes what the
/// program does: `USE_NEW=true` with `${USE_NEW:-no}` left behind starts reading `no`.
fn substitute_flag(
    index: &Index,
    sources: &BTreeMap<PathBuf, (Language, String)>,
    flag: &str,
    value: bool,
) -> Result<Option<(Vec<Change>, SymbolKind)>> {
    let definitions = index.find_symbols(flag, None);
    if definitions.is_empty() {
        return Ok(None);
    }
    if definitions.len() > 1 {
        anyhow::bail!(
            "'{flag}' is defined {} times; say which one with a position",
            definitions.len()
        );
    }
    let definition = definitions[0];

    let literal = literal_for(definition.language, value);
    let parsers = Parsers::new();
    let mut trees: BTreeMap<PathBuf, Parsed> = BTreeMap::new();
    let mut changes = Vec::new();

    if let Some((language, source)) = sources.get(&definition.file) {
        let parsed = parsers.parse(*language, source)?;
        if let Some(what) = not_a_flag(definition, &parsed, source) {
            anyhow::bail!(
                "'{flag}' is not a flag: it {what}. Removing a flag replaces every use of \
                 the name with `{literal}`, and that only reads correctly where the name \
                 held a boolean. Nothing was changed."
            );
        }
        trees.insert(definition.file.clone(), parsed);
    }

    // Nothing reads the name, so there is no flag to remove: no use to substitute, no
    // conditional to collapse, nothing to prune. What is left is one declaration nobody
    // reads, and deleting that is a different command with a different set of checks.
    // Answering it here removed a Next.js route handler called `DELETE`.
    if index.references_to(definition.id).is_empty() {
        anyhow::bail!(
            "'{flag}' is declared at {} and nothing reads it, so there is no flag to \
             remove. `fr delete` removes a declaration nothing uses. Nothing was changed.",
            definition.file.display()
        );
    }

    // A name means one thing everywhere it is written, so one use that names a type
    // settles what the name is for all of them. Zig makes the evidence necessary as well
    // as sufficient: a type is a value there, so `expectEqualSlices(Position, …)` puts a
    // type in argument position where nothing but the declaration elsewhere says so.
    if definition.language != Language::Hcl && definition.language != Language::Bash {
        for reference in index.references_to(definition.id) {
            let Some((language, source)) = sources.get(&reference.file) else {
                continue;
            };
            if !trees.contains_key(&reference.file) {
                trees.insert(reference.file.clone(), parsers.parse(*language, source)?);
            }
            let parsed = &trees[&reference.file];
            let names_a_type = parsed
                .root()
                .descendant_for_byte_range(reference.span.start, reference.span.end)
                .is_some_and(names_a_type);
            if names_a_type {
                let line = LineIndex::new(source)
                    .line_col(reference.span.start, source)
                    .line;
                anyhow::bail!(
                    "'{flag}' is not a flag: it names a type at {}:{line}. Removing a flag \
                     replaces every use of the name with `{literal}`, and a boolean is not \
                     a type. Nothing was changed.",
                    reference.file.display()
                );
            }
        }
    }

    let mut every_use_was_rewritten = true;
    for reference in index.references_to(definition.id) {
        let Some((language, source)) = sources.get(&reference.file) else {
            continue;
        };
        // A use we cannot place is a use we must not rewrite.
        if !reference.confidence.is_safe_to_rewrite() {
            every_use_was_rewritten = false;
            continue;
        }

        if !trees.contains_key(&reference.file) {
            trees.insert(reference.file.clone(), parsers.parse(*language, source)?);
        }
        let parsed = &trees[&reference.file];

        match use_site(*language, definition.kind, parsed, source, reference.span) {
            UseSite::Replace(span) => {
                changes.push((reference.file.clone(), span, literal.to_string()))
            }
            UseSite::Refuse(_) => every_use_was_rewritten = false,
        }
    }

    if every_use_was_rewritten {
        // The definition goes with whatever holds it and then with its whole line — the
        // same two steps `fr delete` takes, because the answer is the same question.
        // Taking the symbol's own span instead removed `NEW_UI = true` from
        // `const NEW_UI = true;` and left `const ;` behind: the edit guard caught it, so
        // `fr remove-flag` did not damage a TypeScript file, it simply never worked on
        // one.
        let definition_span = match sources.get(&definition.file) {
            Some((language, source)) => {
                let widened = match parsers.parse(*language, source) {
                    Ok(parsed) => {
                        crate::refactor::delete::widen_for_delete(&parsed, source, definition)
                    }
                    Err(_) => definition.full_span,
                };
                crate::refactor::delete::deletion_span(source, widened)
            }
            None => definition.full_span,
        };
        changes.push((definition.file.clone(), definition_span, String::new()));
    }
    Ok(Some((changes, definition.kind)))
}

/// What the declaration says the named symbol holds, where that rules a flag out.
///
/// Removing a flag replaces every use of a name with `true` or `false`. A Zig module
/// import and a Zig feature flag are both `const`, so the symbol's kind cannot tell the
/// two apart: asking to remove `DocumentScope` from a file that opens
/// `const DocumentScope = @import("DocumentScope.zig")` rewrote a type into
/// `*const true`, which is text no compiler accepts. The declaration says what the kind
/// cannot.
///
/// The question is whether the source rules a boolean out, and not whether the source
/// proves one. A flag read from a call — `const enabled = feature("new-ui")` — states
/// nothing about its type, and that is the case this command exists for.
///
/// The answer is prose, because it goes straight into the sentence the caller prints.
fn not_a_flag(symbol: &crate::model::Symbol, parsed: &Parsed, source: &str) -> Option<String> {
    // A `Field` is a struct member in Go and Rust and never a flag — but Java has no
    // top level below the type, so its constants are *all* fields: `public static final
    // boolean NEW_CHECKOUT` is the idiomatic feature flag and there is nowhere else to
    // put it. The kind alone cannot tell the two apart; the language can.
    let field_is_a_constant = symbol.language == Language::Java;
    let kind_can_hold_a_flag = matches!(
        symbol.kind,
        SymbolKind::Constant | SymbolKind::Variable | SymbolKind::Function
    ) || (field_is_a_constant && symbol.kind == SymbolKind::Field);
    if !kind_can_hold_a_flag {
        return Some(format!("is a {}", symbol.kind.as_str()));
    }

    let declaration = parsed
        .root()
        .descendant_for_byte_range(symbol.full_span.start, symbol.full_span.end)?;

    // Terraform states a variable's type in an argument and not in a grammar field, so
    // it is read on its own terms.
    if symbol.language == Language::Hcl {
        return hcl_not_a_flag(declaration, source);
    }

    // The exception made for Java fields is for `static final boolean`. An enum constant
    // is a different thing written in the same place: it names one member of a type, and
    // no boolean stands for it.
    if declaration.kind() == "enum_constant" {
        return Some("is an enum constant".into());
    }

    // The bound value first, because it is the stronger statement: `const NAME = "x"`
    // says what `NAME` holds without naming a type at all.
    if let Some(value) = crate::parse::declaration_value(declaration) {
        if let Some(what) = what_the_value_holds(value, source) {
            return Some(what);
        }
    }

    let stated = ["type", "return_type", "result"]
        .iter()
        .find_map(|field| declaration.child_by_field_name(field))
        .and_then(|node| crate::analysis::types::type_text(node, source))?;
    match is_a_boolean_type(&stated) {
        true => None,
        false => Some(format!("is declared `{stated}`")),
    }
}

/// What a bound expression holds, where that rules a boolean out.
///
/// A literal states its own kind in every grammar here, so the node's name is the
/// answer. Anything that is not a literal — a call, another name, an operator — states
/// nothing, and stating nothing is not grounds to refuse.
fn what_the_value_holds(value: Node<'_>, source: &str) -> Option<String> {
    let held = match value.kind() {
        "true" | "false" | "boolean" | "boolean_literal" | "bool_lit" => return None,
        "string"
        | "string_literal"
        | "interpreted_string_literal"
        | "raw_string_literal"
        | "template_string"
        | "char_literal"
        | "character_literal"
        | "concatenated_string" => "a string",
        "integer"
        | "float"
        | "number"
        | "int_literal"
        | "float_literal"
        | "integer_literal"
        | "decimal_integer_literal"
        | "hex_integer_literal"
        | "decimal_floating_point_literal" => "a number",
        "object"
        | "array"
        | "dictionary"
        | "list"
        | "set"
        | "tuple"
        | "composite_literal"
        | "array_creation_expression"
        | "object_creation_expression" => "a collection",
        "struct_declaration"
        | "enum_declaration"
        | "union_declaration"
        | "error_set_declaration"
        | "slice_type"
        | "pointer_type"
        | "array_type"
        | "generic_type"
        | "builtin_type"
        | "type_identifier" => "a type",
        _ if binds_a_module(value, source) => "a module",
        _ => return None,
    };
    Some(format!("holds {held}"))
}

/// Whether an expression opens another file.
///
/// Zig spells this as a builtin call and JavaScript as an ordinary one. Both bind a
/// module, and no boolean stands in for a module.
fn binds_a_module(value: Node<'_>, source: &str) -> bool {
    let callee = match value.kind() {
        "builtin_function" => value.named_child(0),
        "call_expression" => value.child_by_field_name("function"),
        _ => return false,
    };
    match callee {
        Some(callee) => matches!(
            Span::from(callee).text(source).trim(),
            "@import" | "require"
        ),
        None => false,
    }
}

/// How each language here spells the boolean type.
fn is_a_boolean_type(stated: &str) -> bool {
    matches!(stated.trim(), "bool" | "boolean" | "Bool" | "Boolean")
}

/// What a Terraform variable block says it holds.
///
/// Terraform writes the type as an argument, so there is no grammar field to read. A
/// variable that states no type at all is the ordinary case and says nothing either
/// way; `any` is Terraform's own word for unconstrained, and a boolean is one of the
/// things it allows.
fn hcl_not_a_flag(block: Node<'_>, source: &str) -> Option<String> {
    if let Some(stated) = hcl_block_argument(block, source, "type") {
        return match is_a_boolean_type(&stated) || stated == "any" {
            true => None,
            false => Some(format!("is declared `{stated}`")),
        };
    }
    // With no type written down, the default is the only statement of what it holds.
    let default = hcl_block_argument(block, source, "default")?;
    match default.as_str() {
        "true" | "false" => None,
        _ if default.starts_with('"') => Some("holds a string".into()),
        _ if default.parse::<f64>().is_ok() => Some("holds a number".into()),
        _ if default.starts_with('[') || default.starts_with('{') => {
            Some("holds a collection".into())
        }
        _ => None,
    }
}

/// One argument's value from inside a Terraform block, as written.
fn hcl_block_argument(block: Node<'_>, source: &str, name: &str) -> Option<String> {
    let mut walk = block.walk();
    let body = block
        .named_children(&mut walk)
        .find(|child| child.kind() == "body")?;
    let mut body_walk = body.walk();
    for attribute in body.named_children(&mut body_walk) {
        if attribute.kind() != "attribute" {
            continue;
        }
        let mut attribute_walk = attribute.walk();
        let mut children = attribute.named_children(&mut attribute_walk);
        let key = children.next()?;
        if Span::from(key).text(source).trim() != name {
            continue;
        }
        let value = children.next()?;
        return Some(Span::from(value).text(source).trim().to_string());
    }
    None
}

/// Occurrences of the flag's name that outlived the cascade.
///
/// Every use the substitution could rewrite is gone by now, so whatever still spells
/// the flag is something it declined — a use in a form no literal fits, or one whose
/// resolution was never strong enough to touch. Finding them in the finished text is
/// what makes the line numbers point at the file the caller will open.
///
/// The declaration is read from the finished text as well. Where it survived, it
/// survived because a use of it did, and reporting it as one more unrewritable use of
/// itself said something that is not true.
fn remaining_uses(
    sources: &BTreeMap<PathBuf, (Language, String)>,
    flag: &str,
    kind: SymbolKind,
) -> Result<Vec<String>> {
    let parsers = Parsers::new();
    let mut out = Vec::new();

    let snapshot: Vec<(PathBuf, Language, String)> = sources
        .iter()
        .map(|(p, (l, s))| (p.clone(), *l, s.clone()))
        .collect();
    let index = Index::build_from_sources(&snapshot)?;
    let declarations: BTreeSet<(PathBuf, Span)> = index
        .find_symbols(flag, None)
        .into_iter()
        .map(|symbol| (symbol.file.clone(), symbol.name_span))
        .collect();

    for (path, (language, source)) in sources {
        if !source.contains(flag) {
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        for node in named_nodes(&parsed) {
            if node.child_count() != 0 || !is_name_token(node.kind()) {
                continue;
            }
            let span = Span::from(node);
            if span.text(source) != flag {
                continue;
            }
            if declarations.contains(&(path.clone(), span)) {
                out.push(describe(
                    path,
                    source,
                    span,
                    "the declaration stayed, because a use of it did",
                ));
                continue;
            }
            let reason = match use_site(*language, kind, &parsed, source, span) {
                UseSite::Refuse(reason) => reason,
                UseSite::Replace(_) => {
                    "this use of the flag did not resolve to it firmly enough to rewrite".into()
                }
            };
            out.push(describe(path, source, span, &reason));
        }
    }
    Ok(out)
}

/// Node kinds that spell a name a flag could be referred to by.
fn is_name_token(kind: &str) -> bool {
    kind.ends_with("identifier") || kind == "variable_name"
}

/// A message about one place in one file, carrying its line number.
fn describe(path: &Path, source: &str, span: Span, message: &str) -> String {
    let line = LineIndex::new(source).line_col(span.start, source).line;
    format!("{}:{line}: {message}", path.display())
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

/// The bytes a use site's literal must replace.
enum UseSite {
    Replace(Span),
    /// The use cannot be rewritten without changing what the code means.
    Refuse(String),
}

/// Widen a reference's span to whatever the literal has to stand in for.
///
/// In most languages the identifier *is* the expression, so the reference's own span
/// is the answer. Shell and HCL both write a use as a name inside a larger piece of
/// syntax — `$FLAG`, `var.flag` — and replacing only the name would leave the sigil
/// or the namespace stranded in front of a boolean.
fn use_site(
    language: Language,
    definition: SymbolKind,
    parsed: &Parsed,
    source: &str,
    span: Span,
) -> UseSite {
    match language {
        Language::Bash => bash_use_site(parsed, source, span),
        Language::Hcl => hcl_use_site(parsed, source, span),
        _ => general_use_site(definition, parsed, source, span),
    }
}

/// Where a boolean literal can stand, in a language that writes a use as the name.
///
/// Most uses are the reference's own span, so most of this is the identity. Three are
/// not, and each of the three produced text no compiler accepts:
///
/// * A name is also how a type is written. Replacing a use in type position gave
///   `pub fn tokenToPosition(…) true` where the source said `… Position`.
/// * A flag held by a function is read by calling it, so the literal replaces the call
///   and not the callee. Replacing the callee gave `if true()`, which then never
///   collapsed, because `true()` is not a boolean literal.
/// * Reading a field of the flag reads into its value, and a boolean has no field.
fn general_use_site(definition: SymbolKind, parsed: &Parsed, source: &str, span: Span) -> UseSite {
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return UseSite::Replace(span);
    };
    let Some(parent) = node.parent() else {
        return UseSite::Replace(span);
    };

    if names_a_type(node) {
        return UseSite::Refuse(format!(
            "`{}` names a type here, and a boolean is not a type",
            span.text(source)
        ));
    }

    if let Some(call) = call_around(node) {
        // Only a function is read by calling it. A boolean that is called was already
        // broken before this arrived, and writing `true()` over it would hide that.
        return match definition == SymbolKind::Function {
            true => UseSite::Replace(Span::from(call)),
            false => UseSite::Refuse(format!(
                "`{}` calls the flag, and a boolean cannot be called",
                Span::from(call).text(source)
            )),
        };
    }

    // A flag held by a function and named without being called is the function itself
    // and not its result: `let f = is_on;` holds a function, and `let f = true;` does
    // not.
    if definition == SymbolKind::Function {
        return UseSite::Refuse(format!(
            "`{}` names the flag's function without calling it",
            span.text(source)
        ));
    }

    if reads_into_the_value(node, parent) {
        return UseSite::Refuse(format!(
            "`{}` reads through the flag, and a boolean has nothing to read",
            Span::from(parent).text(source)
        ));
    }

    UseSite::Replace(span)
}

/// Whether a use of a name is a use of it as a type.
///
/// A type is written as a name, so nothing about the name says which it is. What says it
/// is the field the grammar hangs it from: every language here holds a declared type
/// under `type`, and a declared result under `return_type` or `result`, wrapping it in a
/// node or two on the way — `*const P`, `P[]`, `: P`.
fn names_a_type(node: Node<'_>) -> bool {
    let mut current = node;
    for _ in 0..4 {
        let Some(parent) = current.parent() else {
            return false;
        };
        for field in ["type", "return_type", "result"] {
            if parent.child_by_field_name(field) == Some(current) {
                return true;
            }
        }
        // Zig writes a struct literal as the type followed by its fields and hangs the
        // type off no field at all.
        if parent.kind() == "struct_initializer" && parent.named_child(0) == Some(current) {
            return true;
        }
        // Keep climbing only while still inside something that spells a type.
        if !parent.kind().contains("type") {
            return false;
        }
        current = parent;
    }
    false
}

/// The call this reference is the target of, where it is one.
fn call_around(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if is_the_callee(node, parent) {
        return Some(parent);
    }
    // `config.flag()` — the reference names the member, and the member access is what
    // the call calls. The literal replaces the whole call, namespace included.
    if member_of(parent) == Some(node) {
        let grandparent = parent.parent()?;
        if is_the_callee(parent, grandparent) {
            return Some(grandparent);
        }
    }
    None
}

fn is_the_callee(node: Node<'_>, parent: Node<'_>) -> bool {
    let callee = match parent.kind() {
        "call_expression" | "call" => parent.child_by_field_name("function"),
        "method_invocation" => parent.child_by_field_name("name"),
        _ => return false,
    };
    callee == Some(node)
}

/// The member a name-access node reads, where the node is one.
fn member_of(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "field_expression" => node
            .child_by_field_name("member")
            .or_else(|| node.child_by_field_name("field")),
        "member_expression" => node.child_by_field_name("property"),
        "attribute" => node.child_by_field_name("attribute"),
        "field_access" => node.child_by_field_name("field"),
        "selector_expression" => node.child_by_field_name("field"),
        "scoped_identifier" | "scoped_type_identifier" => node.child_by_field_name("name"),
        _ => None,
    }
}

/// Whether the use reads a member of the flag.
fn reads_into_the_value(node: Node<'_>, parent: Node<'_>) -> bool {
    let base = match parent.kind() {
        "field_expression" => parent
            .child_by_field_name("object")
            .or_else(|| parent.child_by_field_name("value")),
        "member_expression" | "attribute" | "field_access" | "method_invocation" => {
            parent.child_by_field_name("object")
        }
        "selector_expression" => parent.child_by_field_name("operand"),
        "scoped_identifier" | "scoped_type_identifier" => parent.child_by_field_name("path"),
        _ => return false,
    };
    base == Some(node)
}

/// `$FLAG`, `${FLAG}` and `"$FLAG"` all stand for the value; anything more does not.
fn bash_use_site(parsed: &Parsed, source: &str, span: Span) -> UseSite {
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return UseSite::Replace(span);
    };
    let Some(parent) = node.parent() else {
        return UseSite::Replace(span);
    };

    let expansion = match parent.kind() {
        "simple_expansion" => parent,
        "expansion" => {
            // `${FLAG:-default}`, `${#FLAG}` and `${FLAG[0]}` mean more than the
            // value: taking the whole expansion would drop the rest of it, and
            // taking the name alone would leave `${true:-default}` behind.
            if parent.child_by_field_name("operator").is_some() || parent.named_child_count() != 1 {
                return UseSite::Refuse(format!(
                    "`{}` is not a plain expansion of the flag",
                    Span::from(parent).text(source)
                ));
            }
            parent
        }
        _ => {
            if inside_expansion(node) {
                return UseSite::Refuse(
                    "the flag is used inside a compound expansion, which a literal cannot replace"
                        .into(),
                );
            }
            // An arithmetic `(( FLAG ))` or an assignment target names the variable
            // directly, with no sigil to keep.
            return UseSite::Replace(span);
        }
    };

    // `"$FLAG"` on its own is the quoted value, and the quotes are exactly what stop
    // a shell test from reading as a literal, so the string goes with it.
    if let Some(string) = expansion.parent() {
        let quoted = format!("\"{}\"", Span::from(expansion).text(source));
        if string.kind() == "string" && Span::from(string).text(source) == quoted {
            return UseSite::Replace(Span::from(string));
        }
    }
    UseSite::Replace(Span::from(expansion))
}

/// Is this node wrapped in an expansion whose parent is not the expansion itself?
fn inside_expansion(node: Node<'_>) -> bool {
    let mut current = node.parent();
    for _ in 0..3 {
        let Some(parent) = current else { return false };
        if matches!(parent.kind(), "expansion" | "simple_expansion") {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// `var.flag` is one expression written as two segments; the literal replaces both.
fn hcl_use_site(parsed: &Parsed, source: &str, span: Span) -> UseSite {
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return UseSite::Replace(span);
    };
    let Some(get_attr) = node.parent().filter(|p| p.kind() == "get_attr") else {
        return UseSite::Replace(span);
    };
    let Some(expression) = get_attr.parent().filter(|p| p.kind() == "expression") else {
        return UseSite::Refuse("the flag is not used as a `var.NAME` traversal".into());
    };

    let mut cursor = expression.walk();
    let parts: Vec<Node> = expression.named_children(&mut cursor).collect();
    let namespace = parts
        .first()
        .copied()
        .filter(|p| p.kind() == "variable_expr");
    let Some(namespace) = namespace else {
        return UseSite::Refuse("the flag is not used as a `var.NAME` traversal".into());
    };
    // A longer traversal — `var.flag.attr`, `var.flag[0]` — reads *into* the value
    // instead of being it, and a boolean has nothing to read.
    if parts.len() != 2 || parts[1] != get_attr {
        return UseSite::Refuse(format!(
            "`{}` reads through the flag instead of using its value",
            Span::from(expression).text(source)
        ));
    }
    match Span::from(namespace).text(source) {
        "var" | "local" => UseSite::Replace(Span::from(expression)),
        other => UseSite::Refuse(format!("`{other}` is not a Terraform value namespace")),
    }
}

/// Collapse `if true { … } else { … }` to the branch that survives.
fn simplify_constants(
    sources: &BTreeMap<PathBuf, (Language, String)>,
    originals: &BTreeMap<PathBuf, (Language, String)>,
) -> Result<Vec<Change>> {
    let parsers = Parsers::new();
    let mut changes = Vec::new();

    let indexed = hcl_indexed_addresses(sources)?;

    for (path, (language, source)) in sources {
        if !supports_cascade(*language) {
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        let context = Context {
            original: originals.get(path).map(|(_, s)| s.as_str()).unwrap_or(""),
            indexed: &indexed,
        };
        let mut found = constant_conditionals(&parsed, source, *language, &context).changes;
        // One change per file per round: spans shift as soon as one is applied, and
        // the next round re-parses anyway.
        if let Some(change) = found.pop() {
            changes.push((path.clone(), change.0, change.1));
        }
    }
    Ok(changes)
}

/// What the collapse step needs to know beyond the file in front of it.
///
/// Terraform's unit of scope is the directory, so what one file may do to a resource
/// depends on how the files beside it address that resource.
struct Context<'a> {
    /// The file as it stood before the cascade started.
    original: &'a str,
    /// Resource addresses read with an index or a splat, module-wide.
    indexed: &'a BTreeSet<String>,
}

/// What one file's collapse pass found: the rewrites, and the places it gave up.
#[derive(Debug, Default)]
struct Collapse {
    changes: Vec<(Span, String)>,
    refusals: Vec<(Span, String)>,
}

/// Conditionals whose test is a boolean literal, with their replacement text.
fn constant_conditionals(
    parsed: &Parsed,
    source: &str,
    language: Language,
    context: &Context<'_>,
) -> Collapse {
    match language {
        Language::Zig => zig_conditionals(parsed, source),
        Language::Bash => bash_conditionals(parsed, source),
        Language::Hcl => hcl_constants(parsed, source, context),
        _ => generic_conditionals(parsed, source),
    }
}

/// Every named node, outermost first.
fn named_nodes<'a>(parsed: &'a Parsed) -> Vec<Node<'a>> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    let mut out = Vec::new();
    while let Some(node) = stack.pop() {
        stack.extend(node.named_children(&mut cursor));
        out.push(node);
    }
    out
}

/// The truth value a boolean literal spells, in any of the supported languages.
fn boolean_literal(text: &str) -> Option<bool> {
    // Java's `if` names its condition as the *parenthesised* expression, brackets and
    // all, so the literal arrives as `(true)`. Peeling a matched pair costs nothing in
    // the languages that do not, where a redundant `if ((true))` is still a constant.
    let mut text = text.trim();
    while let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        text = inner.trim();
    }
    match text {
        "true" | "True" => Some(true),
        "false" | "False" => Some(false),
        _ => None,
    }
}

/// Grammars that name an `if`'s three parts as fields: Rust, Go, TypeScript, Python.
fn generic_conditionals(parsed: &Parsed, source: &str) -> Collapse {
    let mut out = Collapse::default();

    for node in named_nodes(parsed) {
        if !node.kind().starts_with("if_") {
            continue;
        }
        let Some(condition) = node.child_by_field_name("condition") else {
            continue;
        };
        let Some(truth) = boolean_literal(Span::from(condition).text(source)) else {
            continue;
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
            Some(branch) => dedent_block(Span::from(branch), source, &indent),
            // A false `if` with no else leaves nothing behind.
            None => String::new(),
        };
        out.changes.push((span, replacement));
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

// --------------------------------------------------------------------------- zig

/// Zig `if`s, in both the statement and the expression spelling.
///
/// The statement calls its branches `body` and — one level down, inside the
/// `else_clause` — `alternative`, so the field names the other grammars share do not
/// find them. The expression names neither branch at all: they are positional,
/// separated by the `else` keyword.
fn zig_conditionals(parsed: &Parsed, source: &str) -> Collapse {
    let mut out = Collapse::default();

    for node in named_nodes(parsed) {
        if !matches!(node.kind(), "if_statement" | "if_expression") {
            continue;
        }
        let Some(condition) = node.child_by_field_name("condition") else {
            continue;
        };
        let Some(truth) = boolean_literal(Span::from(condition).text(source)) else {
            continue;
        };

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        if children.iter().any(|c| c.kind() == "payload") {
            out.refusals.push((
                Span::from(node),
                "this `if` captures its condition's payload, which a boolean has none of".into(),
            ));
            continue;
        }

        if node.kind() == "if_expression" {
            match zig_expression_branches(&children, truth) {
                Ok(kept) => out
                    .changes
                    .push((Span::from(node), Span::from(kept).text(source).to_string())),
                Err(reason) => out.refusals.push((Span::from(node), reason)),
            }
            continue;
        }

        let Some(body) = node.child_by_field_name("body") else {
            continue;
        };
        let alternative = children
            .iter()
            .find(|c| c.kind() == "else_clause")
            .and_then(|c| c.child_by_field_name("alternative"));

        let kept = if truth { Some(body) } else { alternative };
        let span = Span::from(node);
        let indent = crate::edit::line_indent(source, span.start);
        let terminated = span.text(source).trim_end().ends_with(';');
        let replacement = match kept {
            None => String::new(),
            Some(branch) => zig_branch_text(branch, source, &indent, terminated),
        };
        out.changes.push((span, replacement));
    }
    out
}

/// The branch a constant `if` expression keeps.
fn zig_expression_branches<'a>(children: &[Node<'a>], truth: bool) -> Result<Node<'a>, String> {
    let close = children
        .iter()
        .position(|c| c.kind() == ")")
        .ok_or_else(|| "this `if` expression has no parenthesised condition".to_string())?;
    let otherwise = children
        .iter()
        .position(|c| c.kind() == "else")
        .ok_or_else(|| {
            "this `if` expression has no `else`, so it has no value to keep".to_string()
        })?;

    let consequence: Vec<&Node> = children[close + 1..otherwise]
        .iter()
        .filter(|c| c.is_named())
        .collect();
    let alternative: Vec<&Node> = children[otherwise + 1..]
        .iter()
        .filter(|c| c.is_named())
        .collect();

    let kept = if truth { &consequence } else { &alternative };
    match kept.as_slice() {
        [only] => Ok(**only),
        _ => Err("this `if` expression's branches are not single expressions".into()),
    }
}

/// A Zig branch as it reads once the `if` around it is gone.
///
/// A braced branch loses its braces and a level of indentation. An unbraced one is an
/// expression where a statement now has to stand: the semicolon that ended the `if`
/// belonged to the statement, not to the branch, so it has to be carried over.
fn zig_branch_text(branch: Node<'_>, source: &str, indent: &str, terminated: bool) -> String {
    if let Some(block) = zig_block(branch) {
        return dedent_block(Span::from(block), source, indent);
    }
    let text = Span::from(branch).text(source).trim();
    if terminated && !text.ends_with(';') {
        format!("{text};")
    } else {
        text.to_string()
    }
}

/// The block a branch wraps, if it is braced and unlabelled.
///
/// A labelled block is kept whole: `blk: { … }` is a statement in its own right and
/// dropping the label would strand the `break :blk` inside it.
fn zig_block(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "block" {
        return Some(node);
    }
    if !matches!(node.kind(), "block_expression" | "labeled_statement") {
        return None;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    if children.iter().any(|c| c.kind() == "block_label") {
        return None;
    }
    children.into_iter().find(|c| c.kind() == "block")
}

// -------------------------------------------------------------------------- bash

/// Shell `if`s whose test provably succeeds or fails.
///
/// Shell has no boolean type: substituting the flag leaves a *string* where the test
/// used to read a variable, and only some of the ways a script can test a string are
/// decidable from the text alone. Everything else is reported and not guessed.
fn bash_conditionals(parsed: &Parsed, source: &str) -> Collapse {
    let mut out = Collapse::default();

    for node in named_nodes(parsed) {
        if node.kind() != "if_statement" {
            continue;
        }
        let Some(condition) = node.child_by_field_name("condition") else {
            continue;
        };
        let truth = match bash_truth(condition, source) {
            Ok(Some(truth)) => truth,
            Ok(None) => continue,
            Err(reason) => {
                out.refusals.push((Span::from(condition), reason));
                continue;
            }
        };

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let has_elif = children.iter().any(|c| c.kind() == "elif_clause");
        if has_elif && !truth {
            out.refusals.push((
                Span::from(node),
                "this `if` is false but has an `elif`, and promoting an `elif` to an `if` is \
                 not a rewrite this can do"
                    .into(),
            ));
            continue;
        }

        let Some(parts) = bash_if_parts(&children, node) else {
            out.refusals.push((
                Span::from(node),
                "this `if` has no `then`, so its branches cannot be found".into(),
            ));
            continue;
        };

        let kept = if truth { Some(parts.0) } else { parts.1 };
        let span = Span::from(node);
        let indent = crate::edit::line_indent(source, span.start);
        let replacement = match kept {
            None => String::new(),
            Some(branch) => dedent_to(branch.text(source), indent.as_str()),
        };
        out.changes.push((span, replacement));
    }
    out
}

/// A shell `if`'s two branches, delimited by its keywords.
///
/// The grammar gives `if_statement` a `condition` field and nothing else: `then`,
/// `else` and `fi` are bare tokens and the statements sit between them as ordinary
/// children.
fn bash_if_parts<'a>(children: &[Node<'a>], node: Node<'a>) -> Option<(Span, Option<Span>)> {
    let then_end = children
        .iter()
        .find(|c| c.kind() == "then")
        .map(|c| c.end_byte())?;
    let clause = children
        .iter()
        .find(|c| matches!(c.kind(), "else_clause" | "elif_clause"));
    let fi_start = children
        .iter()
        .find(|c| c.kind() == "fi")
        .map(|c| c.start_byte());

    let consequence_end = clause
        .map(|c| c.start_byte())
        .or(fi_start)
        .unwrap_or(node.end_byte());

    let alternative = children
        .iter()
        .find(|c| c.kind() == "else_clause")
        .and_then(|clause| {
            let mut cursor = clause.walk();
            let start = clause
                .children(&mut cursor)
                .find(|c| c.kind() == "else")
                .map(|c| c.end_byte())?;
            Some(Span::new(start, clause.end_byte()))
        });

    Some((Span::new(then_end, consequence_end), alternative))
}

/// Whether a shell condition provably succeeds.
///
/// `Ok(None)` means the test says nothing about the flag — an ordinary `[ -f path ]`
/// the cascade should walk past. `Err` means it does mention a literal the
/// substitution put there, but in a form whose outcome is not decidable.
fn bash_truth(condition: Node<'_>, source: &str) -> Result<Option<bool>, String> {
    let text = Span::from(condition).text(source).trim();
    if let Some(truth) = bash_evaluate(condition, source) {
        return Ok(Some(truth));
    }
    let mentions_literal = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| word == "true" || word == "false");
    if !mentions_literal {
        return Ok(None);
    }
    if bash_single_operand(condition, source) {
        return Err(format!(
            "`{text}` asks whether one operand is a non-empty string, which `false` also is; \
             collapsing it would change what the script does"
        ));
    }
    Err(format!("`{text}` is not a provably constant shell test"))
}

/// The outcome of a shell condition, when the text alone decides it.
fn bash_evaluate(condition: Node<'_>, source: &str) -> Option<bool> {
    match condition.kind() {
        "command" => {
            let name = condition.child_by_field_name("name")?;
            let mut cursor = condition.walk();
            let arguments: Vec<Node> = condition
                .children_by_field_name("argument", &mut cursor)
                .collect();
            match Span::from(name).text(source) {
                "true" if arguments.is_empty() => Some(true),
                "false" if arguments.is_empty() => Some(false),
                "test" => bash_test(&arguments, source, false),
                _ => None,
            }
        }
        "test_command" => {
            let mut cursor = condition.walk();
            let children: Vec<Node> = condition.children(&mut cursor).collect();
            // `[[ … ]]` matches its right operand as a pattern; `[ … ]` does not.
            let patterned = children.first().is_some_and(|c| c.kind() == "[[");
            let inner = children.iter().find(|c| c.is_named())?;
            bash_test_expression(*inner, source, patterned)
        }
        _ => None,
    }
}

/// Does this condition test exactly one operand?
fn bash_single_operand(condition: Node<'_>, source: &str) -> bool {
    match condition.kind() {
        "test_command" => {
            let mut cursor = condition.walk();
            let named: Vec<Node> = condition.named_children(&mut cursor).collect();
            named.len() == 1 && bash_literal(named[0], source).is_some()
        }
        "command" => {
            let Some(name) = condition.child_by_field_name("name") else {
                return false;
            };
            if Span::from(name).text(source) != "test" {
                return false;
            }
            let mut cursor = condition.walk();
            let arguments: Vec<Node> = condition
                .children_by_field_name("argument", &mut cursor)
                .collect();
            arguments.len() == 1 && bash_literal(arguments[0], source).is_some()
        }
        _ => false,
    }
}

/// `test a = b` and `test -n a`, as a list of argument nodes.
fn bash_test(arguments: &[Node<'_>], source: &str, patterned: bool) -> Option<bool> {
    match arguments {
        [left, operator, right] => bash_compare(
            &bash_literal(*left, source)?,
            Span::from(*operator).text(source),
            &bash_literal(*right, source)?,
            patterned,
        ),
        [operator, operand] => bash_unary(
            Span::from(*operator).text(source),
            &bash_literal(*operand, source)?,
        ),
        _ => None,
    }
}

/// The expression inside `[ … ]` or `[[ … ]]`.
fn bash_test_expression(node: Node<'_>, source: &str, patterned: bool) -> Option<bool> {
    match node.kind() {
        "binary_expression" => bash_compare(
            &bash_literal(node.child_by_field_name("left")?, source)?,
            Span::from(node.child_by_field_name("operator")?).text(source),
            &bash_literal(node.child_by_field_name("right")?, source)?,
            patterned,
        ),
        "unary_expression" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            let [operator, operand] = children.as_slice() else {
                return None;
            };
            bash_unary(
                Span::from(*operator).text(source),
                &bash_literal(*operand, source)?,
            )
        }
        _ => None,
    }
}

fn bash_compare(left: &str, operator: &str, right: &str, patterned: bool) -> Option<bool> {
    // Inside `[[ ]]` the right operand is a glob, and matching one is not string
    // equality, so a pattern is left for a human.
    if patterned && right.contains(['*', '?', '[']) {
        return None;
    }
    match operator {
        "=" | "==" => Some(left == right),
        "!=" => Some(left != right),
        _ => None,
    }
}

fn bash_unary(operator: &str, operand: &str) -> Option<bool> {
    match operator {
        "-n" => Some(!operand.is_empty()),
        "-z" => Some(operand.is_empty()),
        _ => None,
    }
}

/// The string a node denotes, when it denotes one without running anything.
fn bash_literal(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "word" | "number" => Some(Span::from(node).text(source).to_string()),
        "raw_string" => Some(Span::from(node).text(source).trim_matches('\'').to_string()),
        "string" => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.named_children(&mut cursor).collect();
            // An expansion or a command substitution inside the quotes is a value
            // only the shell knows.
            if children.iter().any(|c| c.kind() != "string_content") {
                return None;
            }
            Some(
                children
                    .iter()
                    .map(|c| Span::from(*c).text(source))
                    .collect::<String>(),
            )
        }
        _ => None,
    }
}

// --------------------------------------------------------------------------- hcl

/// Terraform's flag is a boolean `variable`, and its conditionals are expressions.
///
/// Three shapes follow from substituting it. A `cond ? a : b` collapses to a branch.
/// A `count` of 1 is the default and the argument goes. A `count` of 0, or a
/// `for_each` over nothing, means the block never exists at all — so the block goes,
/// and whatever addressed it is reported as dangling and not deleted in turn.
///
/// Each of the last two only fires where the cascade itself produced the value: a
/// `count = 0` that was already written by hand belongs to the author, not to this
/// refactoring.
fn hcl_constants(parsed: &Parsed, source: &str, context: &Context<'_>) -> Collapse {
    let mut out = Collapse::default();
    let before = hcl_arguments(context.original);

    let mut blocks = Vec::new();
    let mut attributes = Vec::new();
    let mut conditionals = Vec::new();

    for node in named_nodes(parsed) {
        match node.kind() {
            "conditional" => {
                let mut cursor = node.walk();
                let parts: Vec<Node> = node.named_children(&mut cursor).collect();
                let [condition, consequence, alternative] = parts.as_slice() else {
                    continue;
                };
                let Some(truth) = boolean_literal(Span::from(*condition).text(source)) else {
                    continue;
                };
                let kept = if truth { consequence } else { alternative };
                conditionals.push((
                    Span::from(node),
                    Span::from(*kept).text(source).trim().to_string(),
                ));
            }
            "attribute" => {
                let mut cursor = node.walk();
                let parts: Vec<Node> = node.named_children(&mut cursor).collect();
                let (Some(name), Some(value)) = (parts.first(), parts.get(1)) else {
                    continue;
                };
                let name = Span::from(*name).text(source);
                if !matches!(name, "count" | "for_each") {
                    continue;
                }
                let value = Span::from(*value).text(source).trim().to_string();
                let key = hcl_argument_key(node, source, name);
                if before.get(&key) == Some(&value) {
                    continue;
                }
                let Some(block) = enclosing_block(node) else {
                    continue;
                };
                match hcl_instance_count(name, &value) {
                    Some(0) => blocks.push((Span::from(block), String::new())),
                    Some(1) if name == "count" => {
                        match count_removable(block, node, source, context) {
                            Ok(()) => attributes.push((Span::from(node), String::new())),
                            Err(reason) => out.refusals.push((Span::from(node), reason)),
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Applied one per round, last first: the conditionals have to collapse before the
    // `count` they feed can be read.
    out.changes.extend(blocks);
    out.changes.extend(attributes);
    out.changes.extend(conditionals);
    out
}

/// May a `count` of 1 simply be deleted?
///
/// `count` is not only a number: it makes the block's address a list and puts a
/// `count.index` in scope. Deleting it where either of those is used turns a resource
/// that exists into a configuration that will not plan, so those cases keep the
/// argument and say why.
fn count_removable(
    block: Node<'_>,
    attribute: Node<'_>,
    source: &str,
    context: &Context<'_>,
) -> Result<(), String> {
    let body = Span::from(block).text(source);
    let argument = Span::from(attribute).text(source);
    if body.replace(argument, "").contains("count.index") {
        return Err(
            "`count.index` is used here, and it only exists while the resource has a `count`"
                .into(),
        );
    }

    let address = hcl_resource_address(block, source);
    if address
        .as_deref()
        .is_some_and(|a| context.indexed.contains(a))
    {
        return Err(format!(
            "`{}` is read with an index, which only a resource with a `count` has",
            address.unwrap_or_default()
        ));
    }
    Ok(())
}

/// How many instances a literal `count` or `for_each` produces, when it is literal.
fn hcl_instance_count(name: &str, value: &str) -> Option<usize> {
    match name {
        "count" => value.parse::<usize>().ok(),
        // An empty collection is the only `for_each` whose size the text alone gives.
        "for_each" if value == "{}" || value == "[]" => Some(0),
        _ => None,
    }
}

/// `count` and `for_each` as they were written before the cascade, by block address.
fn hcl_arguments(source: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if source.is_empty() {
        return out;
    }
    let Ok(parsed) = Parsers::new().parse(Language::Hcl, source) else {
        return out;
    };
    for node in named_nodes(&parsed) {
        if node.kind() != "attribute" {
            continue;
        }
        let mut cursor = node.walk();
        let parts: Vec<Node> = node.named_children(&mut cursor).collect();
        let (Some(name), Some(value)) = (parts.first(), parts.get(1)) else {
            continue;
        };
        let name = Span::from(*name).text(source);
        if !matches!(name, "count" | "for_each") {
            continue;
        }
        out.insert(
            hcl_argument_key(node, source, name),
            Span::from(*value).text(source).trim().to_string(),
        );
    }
    out
}

/// An address that names one argument of one block, stable across the cascade.
fn hcl_argument_key(node: Node<'_>, source: &str, name: &str) -> String {
    let mut path = Vec::new();
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "block" {
            path.push(hcl_block_address(parent, source));
        }
        current = parent.parent();
    }
    path.reverse();
    path.push(name.to_string());
    path.join("/")
}

/// A block's header — its type keyword and labels — as a dotted address.
fn hcl_block_address(block: Node<'_>, source: &str) -> String {
    let mut cursor = block.walk();
    let mut parts = Vec::new();
    for child in block.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => parts.push(Span::from(child).text(source).to_string()),
            "string_lit" => parts.push(hcl_label(child, source)),
            _ => break,
        }
    }
    parts.join(".")
}

/// The text inside a label's quotes.
fn hcl_label(string_lit: Node<'_>, source: &str) -> String {
    let mut cursor = string_lit.walk();
    let inner = string_lit
        .named_children(&mut cursor)
        .find(|c| c.kind() == "template_literal")
        .map(|c| Span::from(c).text(source).to_string());
    inner.unwrap_or_default()
}

/// The block a node sits directly inside.
fn enclosing_block<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "block" {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

/// The Terraform address a two-label block declares, as `type.name`.
fn hcl_resource_address(block: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = block.walk();
    let labels: Vec<String> = block
        .named_children(&mut cursor)
        .take_while(|c| matches!(c.kind(), "identifier" | "string_lit"))
        .filter(|c| c.kind() == "string_lit")
        .map(|c| hcl_label(c, source))
        .collect();
    (labels.len() == 2).then(|| labels.join("."))
}

/// Terraform addresses — `type.name` — declared by the two-label blocks in a file.
fn hcl_addresses(parsed: &Parsed, source: &str) -> BTreeSet<String> {
    named_nodes(parsed)
        .into_iter()
        .filter(|n| n.kind() == "block")
        .filter_map(|n| hcl_resource_address(n, source))
        .collect()
}

/// Resource addresses read with an index or a splat, across the whole workspace.
///
/// `aws_s3_bucket.logs[0]` and `aws_s3_bucket.logs[*].arn` both say the resource is
/// a list, which it is only while it has a `count`.
fn hcl_indexed_addresses(
    sources: &BTreeMap<PathBuf, (Language, String)>,
) -> Result<BTreeSet<String>> {
    let parsers = Parsers::new();
    let mut out = BTreeSet::new();

    for (language, source) in sources.values() {
        if *language != Language::Hcl {
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        for node in named_nodes(&parsed) {
            if node.kind() != "expression" {
                continue;
            }
            let mut cursor = node.walk();
            let parts: Vec<Node> = node.named_children(&mut cursor).collect();
            let [root, step, subscript, ..] = parts.as_slice() else {
                continue;
            };
            if root.kind() != "variable_expr"
                || step.kind() != "get_attr"
                || !matches!(subscript.kind(), "index" | "splat")
            {
                continue;
            }
            out.insert(format!(
                "{}.{}",
                Span::from(*root).text(source),
                Span::from(*step).text(source).trim_start_matches('.')
            ));
        }
    }
    Ok(out)
}

/// Addresses still written down after the resource they name has been deleted.
///
/// Terraform has no way to make these harmless: an expression naming a resource that
/// no longer exists is an error at plan time. Deleting them in turn would cascade a
/// flag removal into arbitrary configuration changes, so they are handed back instead.
fn dangling_resource_uses(
    sources: &BTreeMap<PathBuf, (Language, String)>,
    originals: &BTreeMap<PathBuf, (Language, String)>,
) -> Result<Vec<String>> {
    let parsers = Parsers::new();
    let mut before = BTreeSet::new();
    let mut after = BTreeSet::new();

    for (path, (language, source)) in originals {
        if *language != Language::Hcl {
            continue;
        }
        before.extend(hcl_addresses(&parsers.parse(*language, source)?, source));
        let current = sources.get(path).map(|(_, s)| s.as_str()).unwrap_or("");
        after.extend(hcl_addresses(&parsers.parse(*language, current)?, current));
    }

    let removed: BTreeSet<&String> = before.difference(&after).collect();
    if removed.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for (path, (language, source)) in sources {
        if *language != Language::Hcl {
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        for node in named_nodes(&parsed) {
            if node.kind() != "expression" {
                continue;
            }
            let mut cursor = node.walk();
            let parts: Vec<Node> = node.named_children(&mut cursor).collect();
            let (Some(root), Some(step)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            if root.kind() != "variable_expr" || step.kind() != "get_attr" {
                continue;
            }
            let address = format!(
                "{}.{}",
                Span::from(*root).text(source),
                Span::from(*step).text(source).trim_start_matches('.')
            );
            if removed.contains(&address) {
                out.push(describe(
                    path,
                    source,
                    Span::from(node),
                    &format!("{address} no longer exists; this reference is left dangling"),
                ));
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------- reporting

/// Everything the cascade substituted into but could not finish collapsing.
///
/// Run once, against the sources the cascade settled on, so a construct it walked
/// past twelve times is named once.
fn unfinished_work(
    sources: &BTreeMap<PathBuf, (Language, String)>,
    originals: &BTreeMap<PathBuf, (Language, String)>,
) -> Result<Vec<String>> {
    let parsers = Parsers::new();
    let indexed = hcl_indexed_addresses(sources)?;
    let mut out = Vec::new();

    for (path, (language, source)) in sources {
        let original = originals.get(path).map(|(_, s)| s.as_str()).unwrap_or("");
        if original == source {
            continue;
        }
        if !supports_cascade(*language) {
            out.push(format!(
                "{}: the flag was substituted, but {language} conditionals are not collapsed",
                path.display()
            ));
            continue;
        }
        let parsed = parsers.parse(*language, source)?;
        let context = Context {
            original,
            indexed: &indexed,
        };
        for (span, reason) in constant_conditionals(&parsed, source, *language, &context).refusals {
            out.push(describe(path, source, span, &reason));
        }
    }
    Ok(out)
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

/// Imports the cascade's own edits left with nothing naming them.
///
/// A dead branch is often the only place an import was used, and taking the branch away
/// leaves the statement behind: `go build` calls that an error outright, and Rust a
/// warning that this project's own CI turns into one. The output parses either way, which
/// is why sweeping for parse errors never saw it.
///
/// Which imports are dead is not asked here. `fr imports` already answers it, and carries
/// a body of knowledge about uses no query can see — a Rust trait used through its
/// methods, a JSX pragma in a comment — that a second answer would get wrong.
fn remove_dead_imports(
    index: &Index,
    sources: &BTreeMap<PathBuf, (Language, String)>,
    already_dead: &HashSet<(PathBuf, Span)>,
) -> Result<Vec<Change>> {
    for (path, (_, text)) in sources {
        for (span, replacement) in dead_import_spans(index, path, text) {
            if already_dead.contains(&(path.clone(), span)) {
                continue;
            }
            // One per round: the next round re-indexes and finds whatever this exposed.
            return Ok(vec![(path.clone(), span, replacement)]);
        }
    }
    Ok(Vec::new())
}

/// What `fr imports` would make of this file's import statements, region by region.
///
/// The replacements and not the removals: a statement may lose one of the names it binds
/// and keep the others, and treating that as a removal deleted the live ones with it.
fn dead_import_spans(index: &Index, path: &Path, source: &str) -> Vec<(Span, String)> {
    let Ok(plan) = crate::refactor::imports::plan_in(index, path, source) else {
        return Vec::new();
    };
    plan.replacements
}

/// The statements inside a block, moved out one indentation level.
fn dedent_block(block: Span, source: &str, indent: &str) -> String {
    let text = block.text(source);
    let trimmed = text.trim();
    let inner = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        &source[block.start + lead + 1..block.end - trail - 1]
    } else {
        text
    };
    dedent_to(inner, indent)
}

/// Re-indent a run of lines so its shallowest line sits at `indent`.
///
/// Taking the shallowest line as the baseline is what keeps nesting inside the
/// branch intact: every line moves by the same amount, so relative depth survives.
/// The first line is returned bare, because the replacement starts where the
/// construct it replaces did and that column is already occupied.
fn dedent_to(text: &str, indent: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    let body = lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{indent}{}", &line[common..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    body.trim_start().to_string()
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
                let last = crate::edit::full_line_span(
                    &updated,
                    span.end.saturating_sub(1).max(span.start),
                );
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
        previous
            .rsplit('\n')
            .next()
            .is_none_or(|l| l.trim().is_empty())
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

    fn collapse(language: Language, source: &str) -> Collapse {
        collapse_against(language, source, "")
    }

    fn collapse_against(language: Language, source: &str, original: &str) -> Collapse {
        let parsed = Parsers::new().parse(language, source).unwrap();
        let indexed = BTreeSet::new();
        let context = Context {
            original,
            indexed: &indexed,
        };
        constant_conditionals(&parsed, source, language, &context)
    }

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
        let found = collapse(Language::Rust, source).changes;
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("go();"), "got {:?}", found[0].1);
    }

    #[test]
    fn a_variable_condition_is_left_alone() {
        let source = "fn f() {\n    if ready {\n        go();\n    }\n}\n";
        assert!(collapse(Language::Rust, source).changes.is_empty());
    }

    #[test]
    fn a_false_conditional_without_an_else_collapses_to_nothing() {
        let source = "fn f() {\n    if false {\n        go();\n    }\n}\n";
        let found = collapse(Language::Rust, source).changes;
        assert_eq!(found.len(), 1);
        assert!(found[0].1.trim().is_empty(), "got {:?}", found[0].1);
    }

    #[test]
    fn a_false_conditional_keeps_the_else_branch() {
        let source =
            "fn f() {\n    if false {\n        go();\n    } else {\n        wait();\n    }\n}\n";
        let found = collapse(Language::Rust, source).changes;
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("wait();"), "got {:?}", found[0].1);
        assert!(!found[0].1.contains("go();"), "got {:?}", found[0].1);
    }

    #[test]
    fn dedenting_keeps_the_nesting_inside_a_branch() {
        let text = "\n        if x {\n            deep();\n        }\n    ";
        assert_eq!(dedent_to(text, ""), "if x {\n    deep();\n}");
    }

    #[test]
    fn a_zig_statement_conditional_uses_the_body_field() {
        let source = "pub fn f() void {\n    if (true) {\n        go();\n    }\n}\n";
        let found = collapse(Language::Zig, source).changes;
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].1, "go();");
    }

    #[test]
    fn a_zig_expression_conditional_collapses_to_a_branch() {
        let source = "pub fn f() u8 {\n    const x = if (false) 1 else 2;\n    return x;\n}\n";
        let found = collapse(Language::Zig, source).changes;
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].1, "2");
    }

    #[test]
    fn a_zig_unbraced_branch_keeps_its_semicolon() {
        let source = "pub fn f() void {\n    if (true) go();\n}\n";
        let found = collapse(Language::Zig, source).changes;
        assert_eq!(found[0].1, "go();");
    }

    #[test]
    fn a_zig_payload_capture_is_refused() {
        let source = "pub fn f() void {\n    if (true) |v| {\n        go(v);\n    }\n}\n";
        let refusals = collapse(Language::Zig, source).refusals;
        assert_eq!(refusals.len(), 1, "got {refusals:?}");
        assert!(refusals[0].1.contains("payload"), "got {refusals:?}");
    }

    #[test]
    fn a_bash_literal_command_is_provable() {
        let source = "if true; then\n  go\nelse\n  wait\nfi\n";
        let found = collapse(Language::Bash, source).changes;
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].1, "go");
    }

    #[test]
    fn a_bash_string_comparison_is_provable() {
        let source = "if [ true = true ]; then\n  go\nfi\n";
        assert_eq!(collapse(Language::Bash, source).changes[0].1, "go");
        let source = "if [ false = true ]; then\n  go\nelse\n  wait\nfi\n";
        assert_eq!(collapse(Language::Bash, source).changes[0].1, "wait");
    }

    #[test]
    fn a_bash_test_against_a_variable_is_refused() {
        let source = "if [ true = \"$OTHER\" ]; then\n  go\nfi\n";
        let refusals = collapse(Language::Bash, source).refusals;
        assert_eq!(refusals.len(), 1, "got {refusals:?}");
        assert!(refusals[0].1.contains("not a provably constant shell test"));
    }

    #[test]
    fn a_bash_condition_with_no_literal_is_not_this_refactorings_business() {
        let source = "if [ -f /etc/hosts ]; then\n  go\nfi\n";
        let result = collapse(Language::Bash, source);
        assert!(result.changes.is_empty());
        assert!(result.refusals.is_empty(), "got {:?}", result.refusals);
    }

    #[test]
    fn an_hcl_conditional_collapses_to_a_branch() {
        let source = "resource \"a\" \"b\" {\n  x = true ? \"yes\" : \"no\"\n}\n";
        let found = collapse(Language::Hcl, source).changes;
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].1, "\"yes\"");
    }

    #[test]
    fn a_count_the_author_wrote_is_left_alone() {
        // Only a `count` this cascade produced may be removed; one that was already
        // there is the author's, not ours.
        let source = "resource \"a\" \"b\" {\n  count = 1\n}\n";
        assert!(collapse_against(Language::Hcl, source, source)
            .changes
            .is_empty());
        assert_eq!(collapse(Language::Hcl, source).changes.len(), 1);
    }

    #[test]
    fn a_count_that_count_index_depends_on_is_refused() {
        let source = "resource \"a\" \"b\" {\n  count = 1\n  n = count.index\n}\n";
        let result = collapse(Language::Hcl, source);
        assert!(result.changes.is_empty(), "got {:?}", result.changes);
        assert!(
            result.refusals[0].1.contains("count.index"),
            "got {:?}",
            result.refusals
        );
    }

    #[test]
    fn an_else_if_branch_does_not_gain_a_stray_semicolon() {
        let source =
            "pub fn f(x: bool) void {\n    if (true) {\n        a();\n    } else if (x) {\n        b();\n    }\n}\n";
        let found = collapse(Language::Zig, source).changes;
        assert_eq!(found[0].1, "a();");

        let source =
            "pub fn f(x: bool) void {\n    if (false) {\n        a();\n    } else if (x) {\n        b();\n    }\n}\n";
        let found = collapse(Language::Zig, source).changes;
        assert!(!found[0].1.ends_with(';'), "got {:?}", found[0].1);
        assert!(found[0].1.starts_with("if (x)"), "got {:?}", found[0].1);
    }
}
