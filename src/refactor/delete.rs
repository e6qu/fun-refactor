//! Safe delete: remove a definition only when nothing provably still uses it.

use super::{Warning, WarningKind};
use crate::analysis::call_graph::{CallGraph, HierarchyBasis};
use crate::analysis::entrypoints::Entrypoints;
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::model::{Confidence, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::{LineCol, LineIndex, Span};
use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A delete that has been worked out but not applied.
#[derive(Debug)]
pub struct DeletePlan {
    pub symbol: SymbolId,
    pub name: String,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Number of definition sites removed.
    pub sites: usize,
}

/// Work out how to delete `symbol` and everything that defines it.
pub fn plan(index: &Index, symbol: SymbolId) -> Result<DeletePlan> {
    if let Some(language) = index.symbol(symbol).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::SafeDelete, language);
    }
    let target = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // Every definition site of the entity, so a CSS class declared by both `.btn` and
    // `.btn:hover` goes away as a whole and not half.
    let mut group = index.definition_group(symbol);
    for member in crate::analysis::call_graph::Hierarchy::scanned(index).method_group(index, symbol)
    {
        group.extend(index.definition_group(member));
    }
    group.sort();
    group.dedup();
    // Some definitions do not come away on their own.
    let parsers = crate::parse::Parsers::new();
    let mut sites: Vec<(PathBuf, Span)> = Vec::new();
    for id in &group {
        let Some(definition) = index.symbol(*id) else {
            continue;
        };
        let span = match crate::vfs::read_to_string(&definition.file) {
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
        let mut references = Vec::new();
        for (file, span) in &blocking {
            let at = sources.line_col(file, span.start);
            message.push_str(&format!("\n  {}:{at}", file.display()));
            references.push(crate::refactor::RefusalSite {
                file: file.clone(),
                line: at.line,
                col: at.col,
            });
        }
        message.push_str("\nRemove or repoint these uses first; nothing was changed.");
        // A typed refusal, so the caller's exit code can say "refused" instead of a bare
        // failure.
        return Err(crate::refactor::Refusal::StillUsed {
            detail: message,
            references,
        }
        .into());
    }

    // Nothing proven uses it, so the definitions can go.
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
        // Two adjacent sites can claim the same blank line.
        spans.sort_by_key(|s| (s.start, s.end));
        let merged = merge_runs(spans);
        for span in &merged {
            // Python's grammar has no empty suite: deleting the only statement of a body leaves
            // `def __init__(self):` over nothing, and the file stops parsing.
            let replacement = sources
                .get(file)
                .and_then(|source| python_pass_filler(file, source, *span, &merged))
                .unwrap_or_default();
            edits.add(
                file.clone(),
                Edit::new(*span, replacement, format!("delete {}", target.name)),
            );
            deleted.push((file.clone(), *span));
        }
    }

    // Found and not acted on.
    let mut warnings = Vec::new();
    for (file, span, confidence) in weak {
        let at = sources.line_col(&file, span.start);
        warnings.push(Warning {
            kind: WarningKind::WeaklyResolved,
            file,
            line: at.line,
            col: at.col,
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
        let at = sources.line_col(&reference.file, reference.span.start);
        warnings.push(Warning {
            kind: WarningKind::WeaklyResolved,
            file: reference.file.clone(),
            line: at.line,
            col: at.col,
            detail: format!("unresolved occurrence of '{}'; left in place", target.name),
        });
    }

    warnings.extend(textual_occurrences(index, &target.name, &deleted)?);

    for (path, info) in index.files() {
        for gap in &info.gaps {
            warnings.push(Warning {
                kind: WarningKind::IncompleteFacts,
                file: path.clone(),
                line: 1,
                col: 1,
                detail: format!(
                    "{}; uses hidden in it would not have been seen",
                    gap.cause()
                ),
            });
        }
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    // An import the deleted code was the only user of is part of what goes.
    let (orphaned_imports, kept_imports) = crate::refactor::imports::orphaned_by(index, &edits)?;
    for (file, orphaned) in orphaned_imports.iter() {
        for edit in orphaned {
            edits.add(file.clone(), edit.clone());
        }
    }
    warnings.extend(kept_imports);

    Ok(DeletePlan {
        symbol,
        name: target.name.clone(),
        edits,
        warnings,
        sites: sites.len(),
    })
}

/// Why the list spares a symbol the resolved call graph calls dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparedReason {
    /// A string literal somewhere in the workspace spells its name, which is
    /// the only trace reflection and a name-keyed handler table leave.
    NamedInAString,
    /// Dynamic dispatch reaches it: a call site names a method its type declares through a
    /// trait, an interface or a base class.
    DynamicDispatch {
        from: SymbolId,
        basis: HierarchyBasis,
    },
    /// Its name begins with an underscore.
    DeclaredUnused,
    /// Something uses this name, but more than one definition answers to it and nothing here
    /// says which one.
    AmbiguousMemberCall,
    /// It names where the file lives instead of something in it: Java's `package app;`, Go's
    /// `package main`.
    NamesTheFilesPlace,
    /// A JavaBean accessor whose *property* appears where the method does not.
    ReachedByItsProperty,
    /// Something inside it is an entry point, so something outside the workspace reaches in.
    HoldsAnEntryPoint,
    /// The language gives it no address: an HCL block with no labels, such as
    /// `terraform {}` or `lifecycle {}`.
    NoAddressToReferenceIt,
    /// It implements a trait this workspace does not declare, so the caller is the machinery
    /// behind that trait.
    ImplementsForeignTrait,
    /// It structures a document instead of naming code: a Markdown heading.
    StructuresProse,
}

/// [`find_unused`]'s answer with its reasoning attached.
#[derive(Debug, Default)]
pub struct UnusedReport {
    /// The candidates, in symbol order.
    pub unused: Vec<SymbolId>,
    /// Symbols reachability alone would have listed, and what saved each.
    pub spared: Vec<(SymbolId, SparedReason)>,
    /// Files whose dispatch edges failed to read, so an absent hierarchy does not
    /// mistaken for the absence of a hierarchy.
    pub hierarchy_gaps: Vec<(PathBuf, String)>,
}

impl UnusedReport {
    /// One line saying why `symbol` is not on the list, if it was on the raw one.
    pub fn explain(&self, index: &Index, symbol: SymbolId) -> Option<String> {
        let (_, reason) = self.spared.iter().find(|(id, _)| *id == symbol)?;
        Some(match reason {
            SparedReason::NamedInAString => {
                "name appears in a string literal; reflection or a handler table may reach it"
                    .to_string()
            }
            SparedReason::AmbiguousMemberCall => {
                "something uses this name and more than one definition answers to it; \
                 which one runs depends on a receiver type or a build tag this \
                 analysis does not track"
                    .to_string()
            }
            SparedReason::DeclaredUnused => {
                "its name begins with an underscore, which says the author meant it to \
                 go unused"
                    .to_string()
            }
            SparedReason::NoAddressToReferenceIt => {
                "the language gives this block no address, so nothing can reference it".to_string()
            }
            SparedReason::ImplementsForeignTrait => {
                "it implements a trait declared outside this workspace, whose machinery \
                 is the caller"
                    .to_string()
            }
            SparedReason::StructuresProse => {
                "it structures a document; a heading without a link into it is normal, \
                 not dead"
                    .to_string()
            }
            SparedReason::HoldsAnEntryPoint => {
                "something inside it is an entry point, so whatever calls that reaches \
                 this to get there"
                    .to_string()
            }
            SparedReason::ReachedByItsProperty => {
                "its property is named elsewhere. A template or a mapper reaches a \
                 JavaBean accessor by the property, never by the method"
                    .to_string()
            }
            SparedReason::NamesTheFilesPlace => {
                "it names where the file lives instead of something in it; nothing \
                 references a package clause and removing one is a syntax error"
                    .to_string()
            }
            SparedReason::DynamicDispatch { from, basis } => {
                let caller = index
                    .symbol(*from)
                    .map(|s| s.qualified_name())
                    .unwrap_or_else(|| "<unknown>".into());
                format!(
                    "{caller} reaches it by dynamic dispatch, through {}, so the \
                     program may call it while it runs",
                    basis.as_str()
                )
            }
        })
    }
}

/// Symbols nothing references and nothing reachable from `entrypoints` reaches.
pub fn find_unused(index: &Index, entrypoints: &Entrypoints) -> Vec<SymbolId> {
    find_unused_report(index, entrypoints).unused
}

/// The property a JavaBean accessor exposes: `getAddress` and `isActive` expose `address` and
/// `active`.
fn bean_property(symbol: &crate::model::Symbol) -> Option<String> {
    if symbol.language != crate::lang::Language::Java {
        return None;
    }
    if !matches!(symbol.kind, SymbolKind::Method) {
        return None;
    }
    let rest = symbol
        .name
        .strip_prefix("get")
        .or_else(|| symbol.name.strip_prefix("set"))
        .or_else(|| symbol.name.strip_prefix("is"))?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    // `getX` exposes `x`; `gettysburg` exposes nothing.
    first
        .is_uppercase()
        .then(|| first.to_lowercase().collect::<String>() + chars.as_str())
}

/// An HCL block Terraform gives no address to.
fn hcl_block_with_no_address(symbol: &crate::model::Symbol) -> bool {
    if symbol.language != crate::lang::Language::Hcl || symbol.kind != SymbolKind::Block {
        return false;
    }
    let Ok(source) = crate::vfs::read_to_string(&symbol.file) else {
        return false;
    };
    !source[..symbol.name_span.start.min(source.len())].ends_with('"')
}

/// Does this symbol name where the file lives, instead of something in it?
fn names_where_the_file_lives(symbol: &crate::model::Symbol) -> bool {
    symbol.kind == SymbolKind::Module
        && matches!(
            symbol.language,
            crate::lang::Language::Java | crate::lang::Language::Go
        )
}

/// Did the author declare this unused by naming it so?
fn declared_unused(symbol: &crate::model::Symbol) -> bool {
    symbol.name.starts_with('_')
}

/// [`find_unused`], with the reason each spared symbol was spared.
pub fn find_unused_report(index: &Index, entrypoints: &Entrypoints) -> UnusedReport {
    crate::capabilities::record_workspace(crate::capabilities::Capability::DeadCode, index);
    let entrypoints = entrypoints.as_slice();
    let call_graph = CallGraph::build(index);

    // Two reachability answers, because a library has no `main`.
    let exported_roots: Vec<SymbolId> = index
        .symbols
        .iter()
        .filter(|s| s.exported)
        .map(|s| s.id)
        .collect();
    let mut api_roots = entrypoints.to_vec();
    api_roots.extend(exported_roots);
    let reachable_from_entrypoints = call_graph.reachable_from(entrypoints);
    let reachable = call_graph.reachable_from(&api_roots);

    // Some kinds appear in several places and remain one thing.
    let mut siblings: HashMap<(&str, SymbolKind), Vec<SymbolId>> = HashMap::new();
    for symbol in &index.symbols {
        if symbol.kind.allows_multiple_definitions() {
            siblings
                .entry((symbol.name.as_str(), symbol.kind))
                .or_default()
                .push(symbol.id);
        }
    }

    // Two fields of one name on two variants, read by destructuring: the resolver picks the
    // nearer twin at field-based confidence, which is a guess.
    let mut namesakes: HashMap<(&str, SymbolKind), Vec<SymbolId>> = HashMap::new();
    for symbol in &index.symbols {
        namesakes
            .entry((symbol.name.as_str(), symbol.kind))
            .or_default()
            .push(symbol.id);
    }

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
        // The widening is for guesses: the resolver picking the nearer twin at field-based
        // confidence.
        let picked_by_receiver = reference.receiver.is_some()
            && crate::refactor::receiver_known_type(index, reference)
                .is_some_and(|ty| symbol.qualifier.as_deref() == Some(ty.as_str()));
        if reference.confidence >= Confidence::FieldBased && !picked_by_receiver {
            if let Some(group) = namesakes.get(&(symbol.name.as_str(), symbol.kind)) {
                referenced.extend(group.iter().copied());
                continue;
            }
        }
        // The index already knows which definition sites are one entity: two clap variants
        // declaring `include_unresolved`, a Python attribute assigned in two methods.
        referenced.extend(index.definition_group(id));
        match siblings.get(&(symbol.name.as_str(), symbol.kind)) {
            Some(group) => referenced.extend(group.iter().copied()),
            None => {
                referenced.insert(id);
            }
        }
    }

    let named_in_a_string = names_in_string_literals(index);

    // A class whose methods are entry points is reached, whatever calls them.
    let mut holds_an_entrypoint: HashSet<SymbolId> = HashSet::new();
    for entry in entrypoints {
        let mut at = index.symbol(*entry).and_then(|s| s.container);
        while let Some(id) = at {
            if !holds_an_entrypoint.insert(id) {
                break;
            }
            at = index.symbol(id).and_then(|s| s.container);
        }
    }
    // Names the hierarchy analysis has already ruled on.
    let decided_by_hierarchy: HashSet<&str> = index
        .symbols
        .iter()
        .filter(|s| !call_graph.hierarchy_callers(s.id).is_empty())
        .map(|s| s.name.as_str())
        .collect();
    let called_ambiguously: HashSet<String> = ambiguously_used_names(index)
        .into_iter()
        .filter(|name| !decided_by_hierarchy.contains(name.as_str()))
        .collect();
    let dead_cycles = dead_reference_cycles(index, &reachable);

    // What the answer would have been on resolved edges alone, so the difference the hierarchy
    // layer made carries a name and no edit.
    let (reachable_directly, dead_cycles_directly) = if call_graph.hierarchy_edge_count() == 0 {
        (reachable.clone(), dead_cycles.clone())
    } else {
        let direct = call_graph.reachable_from_resolved(entrypoints);
        let cycles = dead_reference_cycles(index, &direct);
        (direct, cycles)
    };

    let mut report = UnusedReport {
        hierarchy_gaps: call_graph.hierarchy_gaps.clone(),
        ..UnusedReport::default()
    };

    for symbol in &index.symbols {
        let reached = if symbol.exported {
            reachable_from_entrypoints.contains(&symbol.id)
        } else {
            reachable.contains(&symbol.id)
        };
        let orphaned = !reached && !referenced.contains(&symbol.id);
        if orphaned || dead_cycles.contains(&symbol.id) {
            if hcl_block_with_no_address(symbol) {
                report
                    .spared
                    .push((symbol.id, SparedReason::NoAddressToReferenceIt));
                continue;
            }
            if symbol.kind == SymbolKind::Heading {
                report
                    .spared
                    .push((symbol.id, SparedReason::StructuresProse));
                continue;
            }
            if names_where_the_file_lives(symbol) {
                report
                    .spared
                    .push((symbol.id, SparedReason::NamesTheFilesPlace));
                continue;
            }
            if declared_unused(symbol) {
                report
                    .spared
                    .push((symbol.id, SparedReason::DeclaredUnused));
                continue;
            }
            if called_ambiguously.contains(&symbol.name) {
                report
                    .spared
                    .push((symbol.id, SparedReason::AmbiguousMemberCall));
                continue;
            }
            if holds_an_entrypoint.contains(&symbol.id) {
                report
                    .spared
                    .push((symbol.id, SparedReason::HoldsAnEntryPoint));
                continue;
            }
            if implements_foreign_trait(index, symbol) {
                report
                    .spared
                    .push((symbol.id, SparedReason::ImplementsForeignTrait));
                continue;
            }
            if let Some(property) = bean_property(symbol) {
                if named_in_a_string.contains(&spelled_loosely(&property)) {
                    report
                        .spared
                        .push((symbol.id, SparedReason::ReachedByItsProperty));
                    continue;
                }
            }
            if !named_in_a_string.contains(&spelled_loosely(&symbol.name)) {
                report.unused.push(symbol.id);
                continue;
            }
        }

        // Only a symbol the plain reachability answer would have listed was spared.
        let orphaned_directly =
            !reachable_directly.contains(&symbol.id) && !referenced.contains(&symbol.id);
        if !(orphaned_directly || dead_cycles_directly.contains(&symbol.id)) {
            continue;
        }
        if !orphaned {
            if let Some((from, basis)) = call_graph.hierarchy_callers(symbol.id).first() {
                report.spared.push((
                    symbol.id,
                    SparedReason::DynamicDispatch {
                        from: *from,
                        basis: *basis,
                    },
                ));
                continue;
            }
        }
        if named_in_a_string.contains(&spelled_loosely(&symbol.name)) {
            report
                .spared
                .push((symbol.id, SparedReason::NamedInAString));
        }
    }

    report.unused.sort();
    report.spared.sort_by_key(|(id, _)| *id);
    report
}

/// Every identifier-shaped word inside a string literal anywhere in the workspace.
fn ambiguously_used_names(index: &Index) -> HashSet<String> {
    index
        .references
        .iter()
        .filter(|r| r.target.is_none() && r.confidence == Confidence::FieldBased)
        .map(|r| r.name.clone())
        .collect()
}

/// One spelling for a name however a wire format writes it.
fn spelled_loosely(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Whether this method's `impl` block names a trait the workspace does not declare.
fn implements_foreign_trait(index: &Index, symbol: &crate::model::Symbol) -> bool {
    if symbol.language != crate::lang::Language::Rust || symbol.kind != SymbolKind::Method {
        return false;
    }
    let Ok(source) = crate::vfs::read_to_string(&symbol.file) else {
        return false;
    };
    let Ok(parsed) = Parsers::new().parse(symbol.language, &source) else {
        return false;
    };
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(symbol.name_span.start, symbol.name_span.end)
    else {
        return false;
    };
    let mut at = node;
    while let Some(parent) = at.parent() {
        if parent.kind() == "impl_item" {
            let Some(named) = parent.child_by_field_name("trait") else {
                return false;
            };
            // `impl fmt::Display for X`: the trait's own name is the last segment.
            let written = Span::from(named).text(&source);
            let name = written
                .rsplit("::")
                .next()
                .unwrap_or(written)
                .split('<')
                .next()
                .unwrap_or(written)
                .trim();
            return !index
                .find_symbols(name, None)
                .iter()
                .any(|s| s.kind == SymbolKind::Trait);
        }
        at = parent;
    }
    false
}

fn names_in_string_literals(index: &Index) -> HashSet<String> {
    let parsers = Parsers::new();
    let mut names = HashSet::new();

    for (path, info) in index.files() {
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let Ok(parsed) = parsers.parse(info.language, &source) else {
            continue;
        };
        for span in spans_of(&parsed, is_string_kind) {
            for word in span
                .text(&source)
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '-'))
            {
                if word.is_empty() {
                    continue;
                }
                names.insert(spelled_loosely(word));
                for part in word.split('-').filter(|p| !p.is_empty()) {
                    names.insert(spelled_loosely(part));
                }
            }
        }
    }
    names
}

/// Members of reference cycles that nothing outside the cycle can reach.
fn dead_reference_cycles(index: &Index, reachable: &HashSet<SymbolId>) -> HashSet<SymbolId> {
    let mut graph: DiGraph<SymbolId, ()> = DiGraph::new();
    let mut nodes: HashMap<SymbolId, NodeIndex> = HashMap::new();
    // Who references each symbol: `None` stands for a reference outside every
    // definition, which is a use by top-level code and keeps the target alive.
    let mut incoming: HashMap<SymbolId, HashSet<Option<SymbolId>>> = HashMap::new();

    for reference in &index.references {
        let Some(target) = reference.target else {
            continue;
        };
        if index.symbol(target).is_none() {
            continue;
        }
        let owner = enclosing_symbol(index, &reference.file, reference.span);
        if owner == Some(target) {
            // A recursive call disappears with the definition it sits in.
            continue;
        }
        incoming.entry(target).or_default().insert(owner);
        if let Some(owner) = owner {
            let from = *nodes.entry(owner).or_insert_with(|| graph.add_node(owner));
            let to = *nodes
                .entry(target)
                .or_insert_with(|| graph.add_node(target));
            graph.add_edge(from, to, ());
        }
    }

    let mut dead = HashSet::new();
    for component in petgraph::algo::tarjan_scc(&graph) {
        if component.len() < 2 {
            continue;
        }
        let members: HashSet<SymbolId> = component.iter().map(|node| graph[*node]).collect();
        if members.iter().any(|id| reachable.contains(id)) {
            continue;
        }
        let held_from_outside = members.iter().any(|id| {
            incoming.get(id).is_some_and(|from| {
                from.iter()
                    .any(|owner| !owner.is_some_and(|o| members.contains(&o)))
            })
        });
        if held_from_outside {
            continue;
        }
        dead.extend(members);
    }
    dead
}

/// The innermost definition whose bytes enclose `span`, if any.
fn enclosing_symbol(index: &Index, file: &Path, span: Span) -> Option<SymbolId> {
    let info = index.file(file)?;
    let mut best: Option<(usize, SymbolId)> = None;
    for id in &info.symbols {
        let Some(symbol) = index.symbol(*id) else {
            continue;
        };
        if !symbol.full_span.contains(span) {
            continue;
        }
        let width = symbol.full_span.end - symbol.full_span.start;
        if best.is_none_or(|(widest, _)| width < widest) {
            best = Some((width, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// The bytes a delete should remove.
pub(crate) fn widen_for_delete(
    parsed: &crate::parse::Parsed,
    source: &str,
    symbol: &crate::model::Symbol,
) -> Span {
    let Some(start) = parsed
        .root()
        .descendant_for_byte_range(symbol.full_span.start, symbol.full_span.end)
    else {
        return symbol.full_span;
    };
    let root = parsed.root();

    /// Siblings of the same kind, which decides whether the parent survives.
    fn same_kind_siblings(node: tree_sitter::Node<'_>) -> usize {
        let Some(parent) = node.parent() else {
            return 0;
        };
        let mut cursor = parent.walk();
        parent
            .named_children(&mut cursor)
            .filter(|c| c.kind() == node.kind() && c.id() != node.id())
            .count()
    }

    let mut node = start;
    while let Some(parent) = node.parent() {
        if parent.id() == root.id() && same_kind_siblings(node) == 0 {
            // Everything above is the file itself; deleting that is not on offer.
            break;
        }
        if same_kind_siblings(node) > 0 {
            break;
        }
        if parent.id() == root.id() {
            break;
        }
        // A body is not optional.
        let parent_is_a_body = parent
            .parent()
            .and_then(|grandparent| grandparent.child_by_field_name("body"))
            .is_some_and(|body| body.id() == parent.id());
        if parent_is_a_body {
            break;
        }
        node = parent;
    }

    let span = Span::from(node);
    if same_kind_siblings(node) == 0 {
        return span;
    }

    // One of several: take this one and the separator joining it to the next.
    let bytes = source.as_bytes();
    let is_separator = |b: u8| b == b',' || b == b';';
    let mut end = span.end;
    while end < bytes.len() && (is_separator(bytes[end]) || bytes[end].is_ascii_whitespace()) {
        let separator = is_separator(bytes[end]);
        end += 1;
        if separator {
            while end < bytes.len() && bytes[end] == b' ' {
                end += 1;
            }
            return Span::new(span.start, end);
        }
    }
    // Last in the list: take the preceding separator instead, so what remains does
    // not end with one.
    let mut begin = span.start;
    while begin > 0 && (bytes[begin - 1] == b' ' || is_separator(bytes[begin - 1])) {
        begin -= 1;
        if is_separator(bytes[begin]) {
            break;
        }
    }
    Span::new(begin, span.end)
}

/// Whether the line starting at `at` sits inside a `/* ...
fn block_comment_continues(source: &str, at: usize) -> bool {
    let above = &source[..at];
    match (above.rfind("/*"), above.rfind("*/")) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(crate) fn deletion_span(source: &str, span: Span) -> Span {
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

    // What sits attached above the definition goes with it.
    let mut first = first;
    loop {
        if first.start == 0 {
            break;
        }
        let previous = full_line_span(source, first.start - 1);
        let line = previous.text(source).trim_start();
        let attached = line.starts_with("///")
            || line.starts_with("#[")
            || line.starts_with("*/")
            || line.starts_with("/**")
            || (line.starts_with('*') && block_comment_continues(source, previous.start));
        if !attached {
            break;
        }
        first = Span::new(previous.start, first.end);
    }

    let mut blanks_before = 0usize;
    let mut at = first.start;
    while at > 0 {
        let previous = full_line_span(source, at - 1);
        if !previous.text(source).trim().is_empty() {
            break;
        }
        blanks_before += 1;
        at = previous.start;
    }
    let allowance = match first.start == 0 {
        // The file must not open blank, so a deletion at its head takes the gap too.
        true => usize::MAX,
        false => blanks_before,
    };
    let mut end = line_end;
    let mut eaten = 0usize;
    while eaten < allowance && end < source.len() {
        let next = full_line_span(source, end);
        if !next.text(source).trim().is_empty() {
            break;
        }
        end = next.end;
        eaten += 1;
    }
    // A deletion that leaves nothing after it would leave the gap above it as a
    // blank tail; the tail goes with it.
    let mut start = first.start;
    if end >= source.len() {
        while start > 0 {
            let previous = full_line_span(source, start - 1);
            if !previous.text(source).trim().is_empty() {
                break;
            }
            start = previous.start;
        }
    }
    Span::new(start, end)
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
fn textual_occurrences(
    index: &Index,
    name: &str,
    deleted: &[(PathBuf, Span)],
) -> Result<Vec<Warning>> {
    Ok(crate::mentions::of(index, name)?
        .into_iter()
        .filter(|m| {
            !deleted
                .iter()
                .any(|(file, gone)| file == &m.file && gone.overlaps(m.span))
        })
        .map(|m| Warning {
            kind: WarningKind::TextualOccurrence,
            file: m.file,
            line: m.line,
            col: m.col,
            detail: format!(
                "'{name}' is written here as text, with nothing linking it to the \
                 declaration; it is not deleted and may \
                 be a use nothing can resolve"
            ),
        })
        .collect())
}

/// Does this node kind hold a string literal?
fn is_string_kind(kind: &str) -> bool {
    // An attribute value is a string that the HTML grammar happens not to call one, and a
    // template names its code there.
    kind.contains("string")
        || kind.contains("quote_scalar")
        || kind.contains("char_literal")
        || kind.contains("attribute_value")
}

/// Spans of string literals, comments and Helm template actions.
fn spans_of(parsed: &Parsed, wanted: impl Fn(&str) -> bool) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut cursor = parsed.root().walk();
    let mut recurse = true;

    loop {
        let node = cursor.node();
        if wanted(node.kind()) {
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

/// Reads each file at most once per plan.
#[derive(Default)]
struct Sources {
    cache: HashMap<PathBuf, Option<String>>,
}

impl Sources {
    fn get(&mut self, path: &Path) -> Option<&str> {
        self.cache
            .entry(path.to_path_buf())
            .or_insert_with(|| crate::vfs::read_to_string(path).ok())
            .as_deref()
    }

    fn line_col(&mut self, path: &Path, offset: usize) -> LineCol {
        match self.get(path) {
            Some(source) => LineIndex::new(source).line_col(offset, source),
            None => LineCol { line: 0, col: 0 },
        }
    }
}

/// `pass`, indented for the hole, when this deletion empties a Python suite.
fn python_pass_filler(
    file: &std::path::Path,
    source: &str,
    span: Span,
    all: &[Span],
) -> Option<String> {
    if crate::lang::detect(file) != Some(crate::lang::Language::Python) {
        return None;
    }
    let parsed = crate::parse::Parsers::new()
        .parse(crate::lang::Language::Python, source)
        .ok()?;
    // The whole-line span can swallow surrounding blank lines and overshoot the
    // block; the statement itself begins at the first dark byte.
    let probe = span.start
        + source[span.start..span.end]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(0);
    let mut node = parsed.root().descendant_for_byte_range(probe, probe + 1)?;
    while node.kind() != "block" {
        node = node.parent()?;
    }
    let mut cursor = node.walk();
    let emptied = node
        .named_children(&mut cursor)
        .all(|child| all.iter().any(|s| s.contains(Span::from(child))));
    if !emptied {
        return None;
    }
    let line_start = source[..span.start]
        .rfind('\n')
        .map(|at| at + 1)
        .unwrap_or(0);
    let indent: String = source[line_start..]
        .chars()
        .take_while(|c| c.is_whitespace() && *c != '\n')
        .collect();
    Some(format!("{indent}pass\n"))
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
        let source = "fn a() {}\nfn b() {}\n\nfn c() {}\n";
        assert_eq!(deletion_span(source, Span::new(10, 19)), Span::new(10, 20));
    }

    #[test]
    fn stacked_blank_lines_collapse_to_the_original_separation() {
        let source = "fn a() {}\n\n\nfn b() {}\n\n\nfn c() {}\n";
        let b = Span::new(12, 21);
        let deleted = deletion_span(source, b);
        let mut out = source.to_string();
        out.replace_range(deleted.start..deleted.end, "");
        assert_eq!(out, "fn a() {}\n\n\nfn c() {}\n");
    }

    #[test]
    fn a_deletion_reaching_the_end_takes_the_gap_above_it() {
        // Nothing follows, so the blank line above would become a blank tail.
        let source = "fn a() {}\n\nfn b() {}\n";
        let b = Span::new(11, 20);
        let deleted = deletion_span(source, b);
        let mut out = source.to_string();
        out.replace_range(deleted.start..deleted.end, "");
        assert_eq!(out, "fn a() {}\n");
    }

    #[test]
    fn merge_runs_collapses_touching_spans() {
        let spans = [Span::new(0, 10), Span::new(10, 20), Span::new(30, 40)];
        assert_eq!(
            merge_runs(&spans),
            vec![Span::new(0, 20), Span::new(30, 40)]
        );
    }
}
