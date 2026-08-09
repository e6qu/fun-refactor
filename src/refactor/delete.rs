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
        for (file, span) in &blocking {
            let at = sources.line_col(file, span.start);
            message.push_str(&format!("\n  {}:{at}", file.display()));
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

    Ok(DeletePlan {
        symbol,
        name: target.name.clone(),
        edits,
        warnings,
        sites: sites.len(),
    })
}

/// Why a symbol the resolved call graph would have called dead was kept off the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparedReason {
    /// Its name is spelled in a string literal somewhere in the workspace, which is
    /// the only trace reflection and a name-keyed handler table leave.
    NamedInAString,
    /// Dynamic dispatch reaches it: a call site names a method its type declares
    /// through a trait, an interface or a base class, and this is one of the
    /// implementations that call could pick.
    DynamicDispatch {
        from: SymbolId,
        basis: HierarchyBasis,
    },
    /// Its name begins with an underscore, which in Rust, TypeScript, Python and Zig
    /// is how an author writes "this is deliberately not used" — usually a parameter
    /// a signature requires and the body ignores.
    DeclaredUnused,
    /// Something uses this name, but more than one definition answers to it and
    /// nothing here says which — two types declaring the same member, or one package
    /// declaring the same function twice under opposite build tags. Every candidate
    /// stays live.
    AmbiguousMemberCall,
    /// It names where the file lives rather than something in it: Java's `package app;`,
    /// Go's `package main`. Nothing ever references one, so "unused" is true of all of
    /// them and says nothing.
    NamesTheFilesPlace,
    /// A JavaBean accessor whose *property* is named somewhere the method is not: a
    /// template writing `${owner.address}` reaches `getAddress`, and every Java template
    /// engine, JSON mapper and data binder works that way.
    ReachedByItsProperty,
    /// Something inside it is an entry point, so something outside the workspace reaches
    /// in: a JUnit test class, a Rust `mod tests`, a Python class of pytest cases.
    HoldsAnEntryPoint,
    /// The language gives it no address: an HCL block with no labels, such as
    /// `terraform {}` or `lifecycle {}`.
    NoAddressToReferenceIt,
}

/// [`find_unused`]'s answer with its reasoning attached.
#[derive(Debug, Default)]
pub struct UnusedReport {
    /// The candidates, in symbol order.
    pub unused: Vec<SymbolId>,
    /// Symbols reachability alone would have listed, and what saved each.
    pub spared: Vec<(SymbolId, SparedReason)>,
    /// Files whose dispatch edges could not be read, so an absent hierarchy is not
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
            SparedReason::HoldsAnEntryPoint => {
                "something inside it is an entry point, so whatever calls that reaches \
                 this to get there"
                    .to_string()
            }
            SparedReason::ReachedByItsProperty => {
                "its property is named elsewhere — a template or a mapper reaches a \
                 JavaBean accessor by the property, never by the method"
                    .to_string()
            }
            SparedReason::NamesTheFilesPlace => {
                "it names where the file lives rather than something in it; nothing \
                 references a package clause and removing one is a syntax error"
                    .to_string()
            }
            SparedReason::DynamicDispatch { from, basis } => {
                let caller = index
                    .symbol(*from)
                    .map(|s| s.qualified_name())
                    .unwrap_or_else(|| "<unknown>".into());
                format!(
                    "reached from {caller} by dynamic dispatch ({}); which implementation \
                     runs is a runtime fact",
                    basis.as_str()
                )
            }
        })
    }
}

/// Symbols nothing references and nothing reachable from `entrypoints` reaches.
///
/// Backs the dead-CSS-selector, unused-Terraform-variable, unused-`values.yaml`-key and
/// unused-function reports. A symbol qualifies when no resolved reference targets it —
/// references from inside its own definition do not count, so dead recursive code still
/// qualifies — and the call graph cannot reach it from any entry point.
///
/// Five corrections apply on top, because the raw answer errs in both directions:
///
/// * Off the list: a symbol whose name appears in a **string literal** anywhere in the
///   workspace. Reflection, a name-keyed handler table, a route string and a template
///   all reach code through a name no resolver follows, leaving only the string.
/// * On it: a **cycle** of symbols referencing only each other, when no member is
///   reachable from an entry point and nothing outside the cycle references any member.
///   Otherwise mutual recursion hides a whole dead component, since every member has an
///   incoming reference.
/// * Off: a method **dynamic dispatch can reach**. A call through a `dyn Trait`, an
///   interface value or a base-class reference names no single definition, but the
///   workspace says which types implement the abstraction and [`CallGraph`] puts an
///   edge on each. Those edges are unproven and marked so.
/// * Off: a **package clause**, which names where the file lives (see
///   [`names_where_the_file_lives`]).
/// * Off: a symbol **containing an entry point**, and a **JavaBean accessor** whose
///   property is named where the method is not.
///
/// The result is a candidate list, not a delete list. Still invisible: a function held
/// in a map or struct field and called through it, a name assembled at runtime, and any
/// use inside a file that failed to parse. [`find_unused_report`] says which correction
/// spared what. Feed each candidate to [`plan`] before acting.
pub fn find_unused(index: &Index, entrypoints: &Entrypoints) -> Vec<SymbolId> {
    find_unused_report(index, entrypoints).unused
}

/// The property a JavaBean accessor exposes: `getAddress` and `isActive` expose
/// `address` and `active`.
///
/// Java only, because there the convention is a specification rather than a habit:
/// template engines, JSON mappers and Spring's own data binding all reach a getter by
/// the property name and never write the method's. `spring-petclinic` called
/// `Owner::getAddress` dead while its template says `${owner.address}` and its tests say
/// `param("address", …)`. Both name the property; neither names the method.
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
///
/// `resource "aws_vpc" "this"` is addressable as `aws_vpc.this` and `output "id"` as an
/// output; `terraform {}`, `required_providers {}`, `lifecycle {}` and a `dynamic`
/// block's `content {}` are not addressable at all, so nothing can reference one and
/// "nothing uses this" is true of every one of them. terraform-aws-vpc reported 46, all
/// of them one of those four.
///
/// A labelled block takes its name from a string label; a block with no labels takes it
/// from the block-type keyword. So the quote before the name is the whole test, and it
/// reads the declaration rather than a list of block types that would drift as Terraform
/// adds them.
fn hcl_block_with_no_address(symbol: &crate::model::Symbol) -> bool {
    if symbol.language != crate::lang::Language::Hcl || symbol.kind != SymbolKind::Block {
        return false;
    }
    let Ok(source) = crate::vfs::read_to_string(&symbol.file) else {
        return false;
    };
    !source[..symbol.name_span.start.min(source.len())].ends_with('"')
}

/// Does this symbol name where the file lives, rather than something in it?
///
/// Java's `package app;` and Go's `package main` are file headers. Nothing references
/// them by name — Java classes in one package never write it, and nothing can import
/// `main` — so "nothing uses this" is true of every one of them and means nothing.
/// Removing one is a syntax error, not a refactoring. `spring-petclinic` reported all
/// forty-nine of its package declarations, one per file.
///
/// Rust's `mod helper;` is a different construct wearing the same symbol kind: it
/// declares a child module, and one nothing references is a real finding. So this asks
/// the language, not the kind.
fn names_where_the_file_lives(symbol: &crate::model::Symbol) -> bool {
    symbol.kind == SymbolKind::Module
        && matches!(
            symbol.language,
            crate::lang::Language::Java | crate::lang::Language::Go
        )
}

/// Did the author declare this unused by naming it so?
///
/// A leading underscore is the convention in Rust, TypeScript, Python and Zig for a
/// binding a signature forces on you and the body has no use for. Listing those as
/// dead code buries the real findings — a single real file turned up eight of them.
/// Go spells the same idea as a bare `_`, which binds nothing and never reaches the
/// index in the first place.
fn declared_unused(symbol: &crate::model::Symbol) -> bool {
    symbol.name.starts_with('_')
}

/// [`find_unused`], with the reason each spared symbol was spared.
pub fn find_unused_report(index: &Index, entrypoints: &Entrypoints) -> UnusedReport {
    let entrypoints = entrypoints.as_slice();
    let call_graph = CallGraph::build(index);

    // Two reachability answers, because a library has no `main`.
    //
    // Everything an exported symbol reaches is live: something outside this
    // workspace may call it, and no amount of scanning here can rule that out.
    // Without this, the entire tree beneath a public API reads as dead — in
    // helm/helm that was most of `pkg/action`, where `performInstall` is called by
    // `performInstallCtx`, called by the exported `RunWithContext`.
    //
    // The exported symbols themselves are judged on the narrow answer, so an export
    // nothing in the workspace uses is still reported — tagged as exported, since
    // whether that is dead code or the public API is not ours to decide.
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

    // Some kinds are declared in several places and are still one thing: a CSS class
    // written in a stylesheet and again in a theme, an element id. A reference picks
    // one of those sites, so counting uses per site reports the others as dead — and
    // `.nav-link`, used by three anchors in the markup, was reported dead twice while
    // `fr delete` refused to remove it and named those same three uses. Grouped once
    // here rather than per symbol, which would be quadratic on a large workspace.
    let mut siblings: HashMap<(&str, SymbolKind), Vec<SymbolId>> = HashMap::new();
    for symbol in &index.symbols {
        if symbol.kind.allows_multiple_definitions() {
            siblings
                .entry((symbol.name.as_str(), symbol.kind))
                .or_default()
                .push(symbol.id);
        }
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
        match siblings.get(&(symbol.name.as_str(), symbol.kind)) {
            Some(group) => referenced.extend(group.iter().copied()),
            None => {
                referenced.insert(id);
            }
        }
    }

    let named_in_a_string = names_in_string_literals(index);

    // A class whose methods are entry points is reached, whatever calls them. JUnit
    // constructs a test class to run the `@Test` methods inside it, and the class itself
    // is named nowhere — `spring-petclinic` reported eleven of them. The same holds for a
    // Rust `mod tests` and a Python class of pytest cases, so this asks the containment
    // chain rather than the language: if anything inside it is an entry point, something
    // outside the workspace reaches in.
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
    // Names the hierarchy analysis has already ruled on. Where it has, its answer
    // stands: it knows `Ledger.Area(scale)` cannot satisfy `Shape.Area()` because the
    // arities differ, and a name-only fallback would undo that.
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

    // What the answer would have been on resolved edges alone, so the difference the
    // hierarchy layer made can be named rather than merely applied. A workspace with
    // no dispatch edges pays nothing for this.
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
            if let Some(property) = bean_property(symbol) {
                if named_in_a_string.contains(&property) {
                    report
                        .spared
                        .push((symbol.id, SparedReason::ReachedByItsProperty));
                    continue;
                }
            }
            if !named_in_a_string.contains(&symbol.name) {
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
        if named_in_a_string.contains(&symbol.name) {
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
///
/// Reflection and handler tables leave a name in a string and nothing else. This takes
/// words whole and split on `-`, so CSS `class="btn-primary"` answers for `btn-primary`
/// and for `btn`. Files it cannot read or parse contribute nothing, which widens the
/// unused list rather than narrowing it; [`plan`] reports parse errors separately.
/// Names used where more than one definition could answer to them.
///
/// `cfg.recordRelease(r)` resolves to neither of helm's two `recordRelease` methods,
/// since choosing needs the type of `cfg`. Both stay live.
///
/// The fallback for names no declared hierarchy covers; where one does, the caller keeps
/// that more precise answer.
fn ambiguously_used_names(index: &Index) -> HashSet<String> {
    index
        .references
        .iter()
        .filter(|r| r.target.is_none() && r.confidence == Confidence::FieldBased)
        .map(|r| r.name.clone())
        .collect()
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
                names.insert(word.to_string());
                for part in word.split('-').filter(|p| !p.is_empty()) {
                    names.insert(part.to_string());
                }
            }
        }
    }
    names
}

/// Members of reference cycles that nothing outside the cycle can reach.
///
/// The per-symbol check asks "does anything reference this?", which mutual recursion
/// always answers yes to. The question a cycle needs is whether anything *outside* it
/// references any member; if not, and no member is reachable from an entry point, the
/// component is dead as a whole. Every symbol kind participates, not just callables: a
/// pair of CSS classes or Terraform locals can reference each other just as happily.
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

/// The bytes a delete should actually remove.
///
/// When the definition is alone on its lines, the whole lines go, indentation and
/// trailing newline included. A blank line immediately after goes too, but only when a
/// blank line or the start of the file already preceded the definition; otherwise that
/// blank line separates the code that stays.
/// Widen a symbol's span to the construct that cannot survive without it.
///
/// The index keeps the span a *rename* rewrites, which is rarely the span a delete can
/// remove. `export const defaultLimits = {…}` has the declarator as its span; removing
/// exactly that leaves `export const ;`, which the engine's reparse check rejects — so
/// `fr unused` named the constant and `fr delete` refused it. A CSS class has the same
/// shape.
///
/// One rule for both: climb while the symbol is the only child of its kind in its
/// parent, since a parent left with none has nothing left to be. Stop at the first
/// sibling of the same kind and take the symbol plus the separator joining them. Never
/// climb into the root.
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

    /// Siblings of the same kind, which is what decides whether the parent survives.
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
        // A body is not optional. "No sibling of the same kind" is true of a Java class
        // holding one field and four methods — the methods are a different kind — so the
        // climb went field → class_body → class_declaration and deleting one constant
        // took the whole class with it. Stop below anything its own parent names as its
        // body, which is the general form of "this container has to be here".
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
        let Ok(source) = crate::vfs::read_to_string(path) else {
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

/// Does this node kind hold a string literal?
fn is_string_kind(kind: &str) -> bool {
    // An attribute value is a string that the HTML grammar happens not to call one, and
    // it is where a template names the code behind it: `th:text="${owner.address}"`
    // reaches `Owner::getAddress`, `v-on:click="submit"` reaches `submit`. Reading only
    // nodes with "string" in their name meant the whole Thymeleaf, Vue and Angular way of
    // referring to code was invisible to the one rule meant to catch exactly that.
    kind.contains("string") || kind.contains("char_literal") || kind.contains("attribute_value")
}

/// Spans of string literals, comments and Helm template actions.
fn string_and_comment_spans(parsed: &Parsed) -> Vec<Span> {
    let mut spans: Vec<Span> = parsed.template_actions.clone();
    spans.extend(spans_of(parsed, |kind| {
        is_string_kind(kind) || kind.contains("comment")
    }));
    spans.sort();
    spans.dedup();
    spans
}

/// Spans of every node whose kind `wanted` accepts, without recursing into a match.
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
        assert_eq!(
            merge_runs(&spans),
            vec![Span::new(0, 20), Span::new(30, 40)]
        );
    }
}
