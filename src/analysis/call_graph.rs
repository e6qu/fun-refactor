//! Call graph construction over the resolved index.
//!
//! The API shape (callers / callees / trace / DOT export) follows funveil's call
//! graph, but the resolution underneath is different in the way that matters: funveil
//! matched callee names as strings in a single flat namespace, so a `parse` in one
//! file and a `parse` in another became one node. Here every edge comes from a
//! resolved reference and carries the [`Confidence`] of that resolution, so callers
//! can distinguish a proven call from a plausible one.
//!
//! On top of that sits a second layer: **class hierarchy analysis** ([`Hierarchy`]).
//! A call through a `dyn Trait`, an interface value or a base-class reference names
//! no single definition, and resolution correctly refuses to invent one — but the
//! workspace does say which types implement the abstraction, and every one of their
//! implementations is a possible callee. Those edges are added with
//! [`Confidence::FieldBased`] and an [`EdgeOrigin::Hierarchy`] tag, so nothing
//! downstream can mistake a candidate for a proven call: they are dashed in DOT,
//! counted separately by [`CallGraph::origin_breakdown`], and a symbol kept off the
//! unused list by one of them can be told exactly why.

use crate::index::Index;
use crate::lang::Language;
use crate::model::{Confidence, ReferenceKind, Symbol, SymbolId, SymbolKind};
use crate::parse::Parsers;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// A call edge between two functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEdge {
    /// Byte offset of the call site.
    pub offset: usize,
    pub file: PathBuf,
    pub confidence: Confidence,
    /// Whether a resolved reference produced this edge or hierarchy analysis did.
    pub origin: EdgeOrigin,
}

/// Where a call edge came from.
///
/// Kept beside the [`Confidence`] rather than folded into it: an edge can be
/// unproven for two quite different reasons, and "the resolver was unsure" is not
/// the same claim as "this is one of the implementations dynamic dispatch could
/// pick".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeOrigin {
    /// A reference the index resolved to this definition.
    Resolved,
    /// Class hierarchy analysis: the call site could not name one definition, and
    /// this is one of the implementations the workspace's own declarations admit.
    Hierarchy(HierarchyBasis),
}

impl EdgeOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeOrigin::Resolved => "resolved",
            EdgeOrigin::Hierarchy(basis) => basis.as_str(),
        }
    }

    pub fn is_hierarchy(&self) -> bool {
        matches!(self, EdgeOrigin::Hierarchy(_))
    }
}

/// What licensed a hierarchy edge, strongest evidence first.
///
/// The ordering is the precedence used when two abstractions reach the same
/// implementation: a declared relationship beats a bare name match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HierarchyBasis {
    /// Rust: an `impl Trait for Type` block names the trait outright.
    ImplementedTrait,
    /// Go: the type's method set covers the interface's. Go has no `implements`
    /// keyword — covering the method set *is* implementing the interface — so
    /// name-and-arity matching is the language's own rule, not a guess.
    InterfaceMethodSet,
    /// A declared supertype: TypeScript `implements` / `extends`, Python
    /// `class C(Base)`.
    DeclaredSupertype,
    /// No declared relationship: the receiver's type is unknown and only the method
    /// name matched. This is the field-based heuristic (Feldthaus et al., ICSE'13),
    /// deliberately unsound and used for TypeScript only.
    MethodName,
}

impl HierarchyBasis {
    pub fn as_str(&self) -> &'static str {
        match self {
            HierarchyBasis::ImplementedTrait => "implemented-trait",
            HierarchyBasis::InterfaceMethodSet => "interface-method-set",
            HierarchyBasis::DeclaredSupertype => "declared-supertype",
            HierarchyBasis::MethodName => "method-name",
        }
    }
}

/// A directed graph of callables.
#[derive(Debug, Default)]
pub struct CallGraph {
    graph: DiGraph<SymbolId, CallEdge>,
    nodes: HashMap<SymbolId, NodeIndex>,
    /// Call sites whose callee could not be resolved, kept so they can be reported
    /// rather than silently dropped.
    pub unresolved: Vec<UnresolvedCall>,
    /// Files whose hierarchy could not be read or parsed, so the caller can see that
    /// the dispatch layer is incomplete for them rather than assume it is empty.
    pub hierarchy_gaps: Vec<(PathBuf, String)>,
}

/// A call site we could see but not resolve to a definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedCall {
    pub caller: Option<SymbolId>,
    pub callee_name: String,
    pub file: PathBuf,
    pub offset: usize,
    pub confidence: Confidence,
}

/// Which direction to walk the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction2 {
    /// Who calls this? (incoming edges)
    Callers,
    /// What does this call? (outgoing edges)
    Callees,
}

impl CallGraph {
    /// Build a call graph from a resolved index.
    ///
    /// This runs both layers: resolved references first, then the hierarchy
    /// fan-out for every method call site the first layer could not pin down. The
    /// second layer costs one parse per imperative file, which is what it takes to
    /// see an `impl Trait for T` or an `implements` clause — the index keeps a
    /// method's owning type but not the abstraction that type answers to.
    pub fn build(index: &Index) -> Self {
        let hierarchy = Hierarchy::scan(index);
        Self::build_with(index, &hierarchy)
    }

    /// Build against a hierarchy that has already been scanned.
    pub fn build_with(index: &Index, hierarchy: &Hierarchy) -> Self {
        let mut cg = CallGraph {
            hierarchy_gaps: hierarchy.gaps.clone(),
            ..CallGraph::default()
        };

        // Every callable becomes a node, so a function with no edges still appears.
        for symbol in &index.symbols {
            if symbol.kind.is_callable() {
                cg.node_for(symbol.id);
            }
        }

        // Call sites already accounted for, so the hierarchy layer neither duplicates
        // an edge nor reports the same site unresolved twice.
        let mut seen_sites: HashSet<(PathBuf, usize)> = HashSet::new();
        let mut edges: HashSet<(SymbolId, SymbolId, usize)> = HashSet::new();

        for reference in &index.references {
            if reference.kind != ReferenceKind::Call {
                continue;
            }
            let caller = enclosing_callable(index, &reference.file, reference.span.start);
            seen_sites.insert((reference.file.clone(), reference.span.start));

            match reference.target.and_then(|t| index.symbol(t)) {
                Some(callee) if callee.kind.is_callable() => {
                    let Some(caller_id) = caller else {
                        // A call outside any function (module top level, a static
                        // initialiser). Recorded as unresolved on the caller side.
                        cg.unresolved.push(UnresolvedCall {
                            caller: None,
                            callee_name: reference.name.clone(),
                            file: reference.file.clone(),
                            offset: reference.span.start,
                            confidence: reference.confidence,
                        });
                        continue;
                    };
                    edges.insert((caller_id, callee.id, reference.span.start));
                    let from = cg.node_for(caller_id);
                    let to = cg.node_for(callee.id);
                    cg.graph.add_edge(
                        from,
                        to,
                        CallEdge {
                            offset: reference.span.start,
                            file: reference.file.clone(),
                            confidence: reference.confidence,
                            origin: EdgeOrigin::Resolved,
                        },
                    );
                    // Resolving is not the same as arriving. `sink.Store(r)` where
                    // `sink` is a `Sink` resolves exactly — to the interface's own
                    // declaration, which has no body. Stopping there left every
                    // implementation of every interface unreached, and so reported as
                    // dead code: seven methods in a twenty-four-file workspace.
                    //
                    // The dispatch layer below only looks at sites that resolved to
                    // *nothing*, which is the other half of the same problem. This is
                    // the half where the answer was right and incomplete.
                    for implementation in hierarchy.implementations_of(index, callee.id) {
                        if !edges.insert((caller_id, implementation, reference.span.start)) {
                            continue;
                        }
                        let target = cg.node_for(implementation);
                        cg.graph.add_edge(
                            from,
                            target,
                            CallEdge {
                                offset: reference.span.start,
                                file: reference.file.clone(),
                                confidence: Confidence::FieldBased,
                                origin: EdgeOrigin::Hierarchy(basis_for(callee.language)),
                            },
                        );
                    }
                }
                _ => {
                    cg.unresolved.push(UnresolvedCall {
                        caller,
                        callee_name: reference.name.clone(),
                        file: reference.file.clone(),
                        offset: reference.span.start,
                        confidence: reference.confidence,
                    });
                }
            }
        }

        cg.add_dispatch_edges(index, hierarchy, &mut seen_sites, &mut edges);
        cg
    }

    /// The hierarchy layer: fan a method call site out to every implementation the
    /// workspace's declarations admit.
    ///
    /// Two things happen here. A method call whose query set files it as a field
    /// access rather than a call (Rust `x.m()`) still resolved, and becomes the
    /// ordinary resolved edge it always was. And a site that resolved to nothing, or
    /// to a candidate too weak to rewrite, gets one edge per plausible implementation
    /// — never replacing a proven answer, only filling in where there was none.
    fn add_dispatch_edges(
        &mut self,
        index: &Index,
        hierarchy: &Hierarchy,
        seen_sites: &mut HashSet<(PathBuf, usize)>,
        edges: &mut HashSet<(SymbolId, SymbolId, usize)>,
    ) {
        let methods = methods_by_owner(index);
        // Dispatch targets depend only on the family and the method name, and a
        // workspace calls the same name from many places.
        let mut targets_for: HashMap<(Family, String), Vec<(String, HierarchyBasis)>> =
            HashMap::new();

        for (file, sites) in &hierarchy.call_sites {
            for site in sites {
                let reference = index.reference_at(file, site.offset);

                // The hierarchy scan reads the file itself, so a source that has moved
                // on since the index was built would silently put every offset in the
                // wrong place. The reference standing at the call site has to be the
                // same name, or the two views disagree and this file gets no dispatch
                // edges — reported, not assumed away.
                let Some(reference) = reference.filter(|r| r.name == site.name) else {
                    let gap = (
                        file.clone(),
                        format!(
                            "call site at byte {} names '{}', but the indexed source has no \
                             such reference there; dispatch edges for this file were skipped",
                            site.offset, site.name
                        ),
                    );
                    if !self.hierarchy_gaps.iter().any(|(path, _)| path == file) {
                        self.hierarchy_gaps.push(gap);
                    }
                    continue;
                };
                let resolved = reference
                    .target
                    .and_then(|t| index.symbol(t))
                    .filter(|s| s.kind.is_callable());
                let caller = enclosing_callable(index, file, site.offset);

                // A site that resolved well enough to rewrite has one answer, and
                // fanning out from there would only inflate the graph.
                if resolved.is_some() && reference.confidence.is_safe_to_rewrite() {
                    self.add_resolved_site(
                        file, site, reference, resolved, caller, edges, seen_sites,
                    );
                    continue;
                }

                let targets = targets_for
                    .entry((site.family, site.name.clone()))
                    .or_insert_with(|| hierarchy.dispatch_targets(site.family, &site.name));

                let mut candidates = 0usize;
                for (owner, basis) in targets.iter() {
                    let key = (site.family, owner.clone(), site.name.clone());
                    let Some(implementations) = methods.get(&key) else {
                        continue;
                    };
                    for callee in implementations {
                        candidates += 1;
                        let Some(caller_id) = caller else { continue };
                        if !edges.insert((caller_id, *callee, site.offset)) {
                            continue;
                        }
                        let from = self.node_for(caller_id);
                        let to = self.node_for(*callee);
                        self.graph.add_edge(
                            from,
                            to,
                            CallEdge {
                                offset: site.offset,
                                file: file.clone(),
                                // Plausible, never proven: which implementation runs
                                // is a runtime fact.
                                confidence: Confidence::FieldBased,
                                origin: EdgeOrigin::Hierarchy(*basis),
                            },
                        );
                        seen_sites.insert((file.clone(), site.offset));
                    }
                }

                // Nothing in the hierarchy to point at: the index's own weak answer,
                // if it had one, is better than no edge and is kept with the
                // confidence it earned.
                if candidates == 0 {
                    self.add_resolved_site(
                        file, site, reference, resolved, caller, edges, seen_sites,
                    );
                }

                // Nothing to point at, or nowhere to point from: the site is still a
                // call, and stays visible as one.
                let unaccounted = (candidates == 0 && resolved.is_none()) || caller.is_none();
                if unaccounted && seen_sites.insert((file.clone(), site.offset)) {
                    self.unresolved.push(UnresolvedCall {
                        caller,
                        callee_name: site.name.clone(),
                        file: file.clone(),
                        offset: site.offset,
                        confidence: reference.confidence,
                    });
                }
            }
        }
    }

    /// Add the edge a resolved reference already earned.
    ///
    /// Only for sites whose reference the query set files as a field access rather
    /// than a call: Rust's `x.m()` is a `field_expression`, and the resolved-reference
    /// pass skips it, so a perfectly ordinary method call produced no edge at all.
    #[allow(clippy::too_many_arguments)]
    fn add_resolved_site(
        &mut self,
        file: &Path,
        site: &CallSite,
        reference: &crate::model::Reference,
        resolved: Option<&Symbol>,
        caller: Option<SymbolId>,
        edges: &mut HashSet<(SymbolId, SymbolId, usize)>,
        seen_sites: &mut HashSet<(PathBuf, usize)>,
    ) {
        let (Some(callee), Some(caller_id)) = (resolved, caller) else {
            return;
        };
        if reference.kind == ReferenceKind::Call {
            // Already an edge: the resolved-reference pass owns this site.
            return;
        }
        if !edges.insert((caller_id, callee.id, site.offset)) {
            return;
        }
        let from = self.node_for(caller_id);
        let to = self.node_for(callee.id);
        self.graph.add_edge(
            from,
            to,
            CallEdge {
                offset: site.offset,
                file: file.to_path_buf(),
                confidence: reference.confidence,
                origin: EdgeOrigin::Resolved,
            },
        );
        seen_sites.insert((file.to_path_buf(), site.offset));
    }

    fn node_for(&mut self, id: SymbolId) -> NodeIndex {
        if let Some(existing) = self.nodes.get(&id) {
            return *existing;
        }
        let idx = self.graph.add_node(id);
        self.nodes.insert(id, idx);
        idx
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn contains(&self, id: SymbolId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Functions that call `id`, with the edge that connects them.
    pub fn callers(&self, id: SymbolId) -> Vec<(SymbolId, &CallEdge)> {
        self.neighbours(id, Direction::Incoming)
    }

    /// Functions called by `id`.
    pub fn callees(&self, id: SymbolId) -> Vec<(SymbolId, &CallEdge)> {
        self.neighbours(id, Direction::Outgoing)
    }

    fn neighbours(&self, id: SymbolId, dir: Direction) -> Vec<(SymbolId, &CallEdge)> {
        let Some(node) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let mut out: Vec<(SymbolId, &CallEdge)> = self
            .graph
            .edges_directed(*node, dir)
            .map(|e| {
                let other = match dir {
                    Direction::Incoming => e.source(),
                    Direction::Outgoing => e.target(),
                };
                (self.graph[other], e.weight())
            })
            .collect();
        out.sort_by_key(|(id, edge)| (*id, edge.offset));
        out.dedup_by_key(|(id, _)| *id);
        out
    }

    /// Breadth-first walk from `start`, bounded by `max_depth`.
    ///
    /// Cycles terminate the walk on revisit, so recursive code cannot hang the tool.
    pub fn trace(&self, start: SymbolId, direction: Direction2, max_depth: usize) -> TraceResult {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        let mut queue = VecDeque::new();
        let mut cycles = Vec::new();
        let mut unexplored = Vec::new();

        queue.push_back((start, 0usize, None));
        visited.insert(start);

        while let Some((id, depth, via)) = queue.pop_front() {
            nodes.push(TraceNode {
                symbol: id,
                depth,
                caller: via,
            });
            let next = match direction {
                Direction2::Callers => self.callers(id),
                Direction2::Callees => self.callees(id),
            };
            if depth >= max_depth {
                // A node the walk stopped at that still had edges is coverage this
                // answer does not have. Recorded on the same footing as a cycle, and
                // for the same reason: a bound nobody is told about reads as a
                // complete answer.
                if !next.is_empty() {
                    unexplored.push(id);
                }
                continue;
            }
            for (other, edge) in next {
                if visited.insert(other) {
                    queue.push_back((other, depth + 1, Some((id, edge.confidence))));
                } else if other == start || nodes.iter().any(|n| n.symbol == other) {
                    cycles.push((id, other));
                }
            }
        }

        cycles.sort();
        cycles.dedup();
        unexplored.sort();
        unexplored.dedup();
        TraceResult {
            start,
            direction,
            nodes,
            cycles,
            unexplored,
        }
    }

    /// Callables with no incoming edges — potential entry points or dead code.
    pub fn roots(&self) -> Vec<SymbolId> {
        let mut roots: Vec<SymbolId> = self
            .graph
            .node_indices()
            .filter(|n| {
                self.graph
                    .edges_directed(*n, Direction::Incoming)
                    .next()
                    .is_none()
            })
            .map(|n| self.graph[n])
            .collect();
        roots.sort();
        roots
    }

    /// Everything reachable from any of `seeds`, following calls forwards.
    ///
    /// Hierarchy edges count: a method only dynamic dispatch reaches is reached.
    pub fn reachable_from(&self, seeds: &[SymbolId]) -> HashSet<SymbolId> {
        self.reachable_via(seeds, true)
    }

    /// Everything reachable from `seeds` following **resolved** edges only.
    ///
    /// The difference between this and [`CallGraph::reachable_from`] is exactly what
    /// hierarchy analysis contributed, which is what lets a report say why a symbol
    /// was spared rather than just dropping it.
    pub fn reachable_from_resolved(&self, seeds: &[SymbolId]) -> HashSet<SymbolId> {
        self.reachable_via(seeds, false)
    }

    fn reachable_via(&self, seeds: &[SymbolId], hierarchy: bool) -> HashSet<SymbolId> {
        let mut seen: HashSet<SymbolId> = HashSet::new();
        let mut queue: VecDeque<SymbolId> = VecDeque::new();
        for seed in seeds {
            if seen.insert(*seed) {
                queue.push_back(*seed);
            }
        }
        while let Some(id) = queue.pop_front() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            for edge in self.graph.edges_directed(*node, Direction::Outgoing) {
                if !hierarchy && edge.weight().origin.is_hierarchy() {
                    continue;
                }
                let callee = self.graph[edge.target()];
                if seen.insert(callee) {
                    queue.push_back(callee);
                }
            }
        }
        seen
    }

    /// Callers that reach `id` only because hierarchy analysis says they might,
    /// with the evidence for each.
    pub fn hierarchy_callers(&self, id: SymbolId) -> Vec<(SymbolId, HierarchyBasis)> {
        let Some(node) = self.nodes.get(&id) else {
            return Vec::new();
        };
        let mut out: Vec<(SymbolId, HierarchyBasis)> = self
            .graph
            .edges_directed(*node, Direction::Incoming)
            .filter_map(|e| match e.weight().origin {
                EdgeOrigin::Hierarchy(basis) => Some((self.graph[e.source()], basis)),
                EdgeOrigin::Resolved => None,
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// How many edges hierarchy analysis contributed, rather than resolution.
    pub fn hierarchy_edge_count(&self) -> usize {
        self.graph
            .edge_references()
            .filter(|e| e.weight().origin.is_hierarchy())
            .count()
    }

    /// Counts by what produced each edge: resolution, or one kind of hierarchy
    /// evidence. A graph must never quietly grow candidates.
    pub fn origin_breakdown(&self) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for edge in self.graph.edge_references() {
            *counts.entry(edge.weight().origin.as_str()).or_default() += 1;
        }
        counts
    }

    /// Render as Graphviz DOT.
    pub fn to_dot(&self, index: &Index) -> String {
        let mut out = String::from("digraph calls {\n  rankdir=LR;\n  node [shape=box];\n");
        for node in self.graph.node_indices() {
            let id = self.graph[node];
            if let Some(symbol) = index.symbol(id) {
                out.push_str(&format!(
                    "  n{} [label=\"{}\"];\n",
                    id.0,
                    escape_dot(&symbol.qualified_name())
                ));
            }
        }
        for edge in self.graph.edge_references() {
            let from = self.graph[edge.source()];
            let to = self.graph[edge.target()];
            // Unproven edges are dashed so a picture cannot overstate certainty, and
            // a dispatch candidate says what made it a candidate.
            let style = if edge.weight().confidence.is_safe_to_rewrite() {
                "solid"
            } else {
                "dashed"
            };
            let label = match edge.weight().origin {
                EdgeOrigin::Resolved => String::new(),
                EdgeOrigin::Hierarchy(basis) => format!(", label=\"{}\"", basis.as_str()),
            };
            out.push_str(&format!(
                "  n{} -> n{} [style={}{}];\n",
                from.0, to.0, style, label
            ));
        }
        out.push_str("}\n");
        out
    }

    /// Counts by resolution tier, so a caller can see how much of the graph is proven.
    pub fn confidence_breakdown(&self) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for edge in self.graph.edge_references() {
            *counts.entry(edge.weight().confidence.as_str()).or_default() += 1;
        }
        counts
    }
}

/// One step of a [`CallGraph::trace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceNode {
    pub symbol: SymbolId,
    pub depth: usize,
    /// The node we arrived from, and how well that edge resolved.
    pub caller: Option<(SymbolId, Confidence)>,
}

/// The result of walking the call graph.
#[derive(Debug, Clone)]
pub struct TraceResult {
    pub start: SymbolId,
    pub direction: Direction2,
    pub nodes: Vec<TraceNode>,
    /// Edges that closed a cycle; reported rather than silently pruned.
    pub cycles: Vec<(SymbolId, SymbolId)>,
    /// Nodes the depth limit stopped at that still had edges beyond them.
    ///
    /// The walk is bounded and the bound is a choice, so what it excluded is part of
    /// the answer. Without this a five-deep chain traced three levels reported "affects
    /// 4 site(s)" — a definite count of an incomplete search.
    pub unexplored: Vec<SymbolId>,
}

impl TraceResult {
    /// Render as an indented tree.
    pub fn format_tree(&self, index: &Index) -> String {
        let mut out = String::new();
        for node in &self.nodes {
            let name = index
                .symbol(node.symbol)
                .map(|s| s.qualified_name())
                .unwrap_or_else(|| "<unknown>".into());
            let marker = match node.caller {
                Some((_, confidence)) if !confidence.is_safe_to_rewrite() => {
                    format!(" [{}]", confidence.as_str())
                }
                _ => String::new(),
            };
            out.push_str(&format!("{}{}{}\n", "  ".repeat(node.depth), name, marker));
        }
        if !self.cycles.is_empty() {
            out.push_str(&format!("\n{} cycle(s) detected\n", self.cycles.len()));
        }
        if !self.unexplored.is_empty() {
            out.push_str(&format!(
                "\nthe depth limit stopped the walk at {} node(s) that had more beyond \
                 them; raise it to see further\n",
                self.unexplored.len()
            ));
        }
        out
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// The innermost callable whose body contains `offset`.
fn enclosing_callable(index: &Index, file: &Path, offset: usize) -> Option<SymbolId> {
    let info = index.file(file)?;
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind.is_callable() && s.full_span.contains_offset(offset))
        .min_by_key(|s| s.full_span.len())
        .map(|s| s.id)
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ------------------------------------------------------- class hierarchy analysis

/// Languages whose types share one hierarchy namespace.
///
/// A Go `Shape` and a TypeScript `Shape` are unrelated names that never dispatch to
/// each other, so the family is part of every key. TSX is TypeScript: a class in a
/// `.tsx` file implements an interface declared in a `.ts` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Family {
    Rust,
    Go,
    Ts,
    Java,
    Python,
}

impl Family {
    /// The family a language belongs to, or `None` when it has no type hierarchy to
    /// analyse. Zig dispatches through comptime duck typing and Bash has no methods
    /// at all; neither declares an implements-relationship anything could read.
    ///
    /// Java is not one of those, and used to fall into the same silent `_` as though
    /// it were — so the one language here that states its hierarchy in as many words
    /// was the one whose hierarchy went unread.
    fn of(language: Language) -> Option<Family> {
        match language {
            Language::Rust => Some(Family::Rust),
            Language::Go => Some(Family::Go),
            Language::TypeScript | Language::Tsx => Some(Family::Ts),
            Language::Java => Some(Family::Java),
            Language::Python => Some(Family::Python),
            Language::Zig
            | Language::Bash
            | Language::Html
            | Language::Css
            | Language::Scss
            | Language::Hcl
            | Language::Yaml
            | Language::Helm
            | Language::Xml
            | Language::Markdown => None,
        }
    }
}

/// A type name within one family.
type TypeKey = (Family, String);

/// A call written in method-call syntax: `receiver.name(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CallSite {
    /// Byte offset of the method name, which is also the reference's span start.
    offset: usize,
    name: String,
    family: Family,
}

/// The type hierarchy a workspace states outright.
///
/// This is the input class hierarchy analysis needs and the index does not keep: a
/// [`Symbol`] records the *type* that owns a method (`qualifier`), but not the trait,
/// interface or base class that type answers to. Recovering it costs one parse per
/// file, the same price [`crate::refactor::delete::find_unused`] already pays to read
/// string literals.
///
/// Nothing here is inferred. Every entry is a declaration someone wrote: an `impl
/// Trait for Type`, an `implements` clause, a `class C(Base)` line, or — in Go, where
/// no `implements` keyword exists — a method set that covers an interface's, which is
/// the whole of what implementing an interface means there.
#[derive(Debug, Default)]
pub struct Hierarchy {
    /// Methods an abstraction declares, name to arity: a Rust trait, a Go interface,
    /// a TypeScript interface or class, a Python class.
    declares: HashMap<TypeKey, BTreeMap<String, usize>>,
    /// The same methods, name to the *types* in their signature, where those could be
    /// read. Go decides implementation by signature and not by name and count, and
    /// matching on the count alone said a `Run() string` implements an interface that
    /// asks for `Run() error`.
    ///
    /// Separate from `declares` because it is a refinement and not a replacement: a
    /// signature nobody could read leaves the arity answer standing, so a method this
    /// cannot parse widens the answer instead of narrowing it to nothing.
    signatures: HashMap<TypeKey, BTreeMap<String, String>>,
    /// Which abstractions declare a given method name — the reverse of `declares`,
    /// so a call site asks about its own name instead of walking every type.
    declarers: HashMap<(Family, String), BTreeSet<String>>,
    /// Subtypes, keyed by the supertype they name: `impl T for X`, `implements`,
    /// `extends`, a Rust supertrait bound, a Python base class list. Held this way
    /// round because every question asked of it is "who implements this?".
    direct_subtypes: HashMap<TypeKey, BTreeSet<String>>,
    /// Concrete method sets, name to arity. Go only: it is the sole language here
    /// where implementing an interface is a structural fact rather than a declared
    /// one, so it is the only one that needs to compare method sets.
    method_sets: HashMap<TypeKey, BTreeMap<String, usize>>,
    /// Method-call syntax sites per file.
    call_sites: BTreeMap<PathBuf, Vec<CallSite>>,
    /// Files that could not be read or parsed. A gap costs edges, never invents
    /// them, but it is reported rather than passed off as an empty hierarchy.
    pub gaps: Vec<(PathBuf, String)>,
}

/// What licenses a hierarchy edge in this language.
///
/// The declaration was resolved, so the relationship is whatever the language uses to
/// express implementing: an `impl Trait for T`, a covered method set, an `implements`
/// clause. Never [`HierarchyBasis::MethodName`] — that tier is for a receiver whose
/// type is unknown, and here the callee's own declaration named the abstraction.
fn basis_for(language: Language) -> HierarchyBasis {
    match Family::of(language) {
        Some(Family::Rust) => HierarchyBasis::ImplementedTrait,
        Some(Family::Go) => HierarchyBasis::InterfaceMethodSet,
        _ => HierarchyBasis::DeclaredSupertype,
    }
}

/// Does this kind of symbol name a type that another type can implement or extend?
fn is_type_declaration(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Interface
            | SymbolKind::Trait
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::TypeAlias
    )
}

impl Hierarchy {
    /// Concrete methods that implement `symbol`, when it declares an abstraction.
    ///
    /// The same question the call graph asks at a dispatch site, answered for one
    /// declaration. Navigation and the graph share it so they cannot disagree about
    /// what implements what.
    pub fn implementations_of(&self, index: &Index, symbol: SymbolId) -> Vec<SymbolId> {
        let Some(declaration) = index.symbol(symbol) else {
            return Vec::new();
        };
        if is_type_declaration(declaration.kind) {
            return self.implementors_of_type(index, symbol);
        }
        if declaration.kind != SymbolKind::Method {
            return Vec::new();
        }
        let Some(family) = Family::of(declaration.language) else {
            return Vec::new();
        };

        // Only a method the abstraction itself declares has implementations; a method
        // on a concrete type already is one.
        let owner = declaration.qualifier.clone().unwrap_or_default();
        let declares_it = self
            .declarers
            .get(&(family, declaration.name.clone()))
            .is_some_and(|owners| owners.contains(&owner));
        if !declares_it {
            return Vec::new();
        }

        let mut found: Vec<SymbolId> = Vec::new();
        for (implementor, _) in self.dispatch_targets(family, &declaration.name) {
            for candidate in index.find_symbols(&declaration.name, None) {
                if candidate.id != symbol
                    && candidate.kind == SymbolKind::Method
                    && candidate.qualifier.as_deref() == Some(implementor.as_str())
                {
                    found.push(candidate.id);
                }
            }
        }
        found.sort();
        found.dedup();
        found
    }

    /// The concrete types that implement an abstraction.
    ///
    /// Pointing at the interface rather than at one of its methods is the question
    /// people actually ask — "what are the Sinks?" — and it used to answer nothing at
    /// all, on the grounds that only a method has implementations. The relationships
    /// were already known; nothing was reading them from this direction.
    fn implementors_of_type(&self, index: &Index, symbol: SymbolId) -> Vec<SymbolId> {
        let Some(declaration) = index.symbol(symbol) else {
            return Vec::new();
        };
        let Some(family) = Family::of(declaration.language) else {
            return Vec::new();
        };
        let name = &declaration.name;

        // Go says nothing about who implements what: a type implements an interface
        // by covering its method set, so the method set is the question. Everywhere
        // else the relationship is declared and recorded as a subtype edge.
        let mut names = self.subtypes(family, name);
        if family == Family::Go {
            if let Some(required) = self.declares.get(&(family, name.clone())) {
                // An interface with no methods is satisfied by every type in the
                // workspace, which is true and useless. Say nothing rather than
                // everything.
                if !required.is_empty() {
                    names.extend(self.go_implementors(&(family, name.clone())));
                }
            }
        }
        names.remove(name);

        let mut found: Vec<SymbolId> = Vec::new();
        for implementor in names {
            for candidate in index.find_symbols(&implementor, None) {
                if candidate.id != symbol
                    && is_type_declaration(candidate.kind)
                    && Family::of(candidate.language) == Some(family)
                {
                    found.push(candidate.id);
                }
            }
        }
        found.sort();
        found.dedup();
        found
    }

    /// Read every file in the index that belongs to a family with a hierarchy.
    pub fn scan(index: &Index) -> Self {
        let parsers = Parsers::new();
        let mut hierarchy = Hierarchy::default();

        for (path, info) in index.files() {
            let Some(family) = Family::of(info.language) else {
                continue;
            };
            let source = match crate::vfs::read_to_string(path) {
                Ok(source) => source,
                Err(error) => {
                    hierarchy.gaps.push((path.clone(), error.to_string()));
                    continue;
                }
            };
            let parsed = match parsers.parse(info.language, &source) {
                Ok(parsed) => parsed,
                Err(error) => {
                    hierarchy.gaps.push((path.clone(), error.to_string()));
                    continue;
                }
            };

            let mut sites: Vec<CallSite> = Vec::new();
            let mut visit = |node: Node| match family {
                Family::Rust => hierarchy.visit_rust(node, &source, &mut sites),
                Family::Go => hierarchy.visit_go(node, &source, &mut sites),
                Family::Ts => hierarchy.visit_ts(node, &source, &mut sites),
                Family::Java => hierarchy.visit_java(node, &source, &mut sites),
                Family::Python => hierarchy.visit_python(node, &source, &mut sites),
            };
            walk(parsed.root(), &mut visit);
            if !sites.is_empty() {
                hierarchy.call_sites.insert(path.clone(), sites);
            }
        }
        hierarchy
    }

    /// The types a call to `method` could dispatch to, with the evidence for each.
    ///
    /// Every abstraction that declares the name contributes its implementations. A
    /// type reached two ways keeps the stronger evidence.
    fn dispatch_targets(&self, family: Family, method: &str) -> Vec<(String, HierarchyBasis)> {
        let mut targets: BTreeMap<String, HierarchyBasis> = BTreeMap::new();
        let mut note = |name: String, basis: HierarchyBasis| {
            targets
                .entry(name)
                .and_modify(|existing| *existing = (*existing).min(basis))
                .or_insert(basis);
        };

        let declaring = match self.declarers.get(&(family, method.to_string())) {
            Some(declaring) => declaring,
            None => return Vec::new(),
        };
        for abstraction in declaring {
            if !self.declares.contains_key(&(family, abstraction.clone())) {
                continue;
            }
            match family {
                // The trait itself keeps its declaration (and any default body)
                // live; `impl Trait for T` supplies the rest.
                Family::Rust => {
                    note(abstraction.clone(), HierarchyBasis::ImplementedTrait);
                    for implementor in self.subtypes(family, abstraction) {
                        note(implementor, HierarchyBasis::ImplementedTrait);
                    }
                }
                Family::Go => {
                    note(abstraction.clone(), HierarchyBasis::InterfaceMethodSet);
                    for implementor in self.go_implementors(&(family, abstraction.clone())) {
                        note(implementor, HierarchyBasis::InterfaceMethodSet);
                    }
                }
                // The declaring class is reached by its own name alone — that is the
                // field-based heuristic and it is labelled as such. Its subclasses
                // are reached by a declared relationship, which is stronger.
                // Java sits here for the same reason rather than a stronger one: it
                // declares `implements` outright, but nothing in this tool infers the
                // static type of a receiver, so reaching the declaring type is still
                // the name-based step and is labelled as such.
                Family::Ts | Family::Java => {
                    note(abstraction.clone(), HierarchyBasis::MethodName);
                    for subclass in self.subtypes(family, abstraction) {
                        note(subclass, HierarchyBasis::DeclaredSupertype);
                    }
                }
                // Python gets no name-only tier: bucketing call sites by method name
                // over-links Python badly (PyCG, ICSE'21), so a class outside every
                // hierarchy contributes nothing.
                Family::Python => {
                    let subclasses = self.subtypes(family, abstraction);
                    if subclasses.is_empty() {
                        continue;
                    }
                    note(abstraction.clone(), HierarchyBasis::DeclaredSupertype);
                    for subclass in subclasses {
                        note(subclass, HierarchyBasis::DeclaredSupertype);
                    }
                }
            }
        }
        targets.into_iter().collect()
    }

    /// Types that name `abstraction` as a supertype, transitively.
    fn subtypes(&self, family: Family, abstraction: &str) -> BTreeSet<String> {
        let mut found: BTreeSet<String> = BTreeSet::new();
        let mut frontier: Vec<String> = vec![abstraction.to_string()];

        while let Some(current) = frontier.pop() {
            let Some(children) = self.direct_subtypes.get(&(family, current)) else {
                continue;
            };
            for child in children {
                // A cyclic `extends` is not legal in any of these languages, but a
                // workspace can still contain one; visiting each name once means it
                // cannot loop here.
                if child != abstraction && found.insert(child.clone()) {
                    frontier.push(child.clone());
                }
            }
        }
        found
    }

    /// Go types whose method set covers `required`.
    ///
    /// This is Go's rule verbatim, minus the types: a method's name and its number of
    /// parameters are what the syntax shows, and two same-named methods of the same
    /// arity but different signatures are indistinguishable here. That widens the
    /// candidate set; it never narrows it.
    fn go_implementors(&self, interface: &TypeKey) -> BTreeSet<String> {
        let Some(required) = self.declares.get(interface) else {
            return BTreeSet::new();
        };
        let wanted = self.signatures.get(interface);
        let mut found = BTreeSet::new();
        for ((family, name), methods) in &self.method_sets {
            if *family != Family::Go {
                continue;
            }
            let covers = required.iter().all(|(method, arity)| {
                if methods.get(method) != Some(arity) {
                    return false;
                }
                // Go decides this by signature. Where both sides are legible, they have
                // to agree; where either is not, the arity answer stands, because a
                // dropped edge here becomes a live method reported as dead code.
                match (
                    wanted.and_then(|w| w.get(method)),
                    self.signatures
                        .get(&(Family::Go, name.clone()))
                        .and_then(|s| s.get(method)),
                ) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                }
            });
            if covers {
                found.insert(name.clone());
            }
        }
        found
    }

    fn declare(&mut self, key: TypeKey, method: String, arity: usize) {
        self.declarers
            .entry((key.0, method.clone()))
            .or_default()
            .insert(key.1.clone());
        self.declares.entry(key).or_default().insert(method, arity);
    }

    fn add_supertype(&mut self, key: TypeKey, supertype: String) {
        self.direct_subtypes
            .entry((key.0, supertype))
            .or_default()
            .insert(key.1);
    }

    // ------------------------------------------------------------------ Rust
    fn visit_rust(&mut self, node: Node, source: &str, sites: &mut Vec<CallSite>) {
        match node.kind() {
            "trait_item" => {
                let Some(name) = field_text(node, "name", source) else {
                    return;
                };
                let key = (Family::Rust, name);
                if let Some(body) = node.child_by_field_name("body") {
                    for member in named_children(body) {
                        if !matches!(member.kind(), "function_item" | "function_signature_item") {
                            continue;
                        }
                        if let Some(method) = field_text(member, "name", source) {
                            self.declare(key.clone(), method, arity(member));
                        }
                    }
                }
                // `trait Circle: Shape` — implementing Circle implements Shape.
                if let Some(bounds) = node.child_by_field_name("bound") {
                    for supertrait in type_identifiers(bounds, source) {
                        self.add_supertype(key.clone(), supertrait);
                    }
                }
            }
            "impl_item" => {
                let (Some(trait_node), Some(type_node)) = (
                    node.child_by_field_name("trait"),
                    node.child_by_field_name("type"),
                ) else {
                    return;
                };
                let (Some(trait_name), Some(type_name)) = (
                    type_identifiers(trait_node, source).into_iter().next(),
                    type_identifiers(type_node, source).into_iter().next(),
                ) else {
                    return;
                };
                self.add_supertype((Family::Rust, type_name), trait_name);
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "field_expression" {
                        push_site(function, "field", source, Family::Rust, sites);
                    }
                }
            }
            _ => {}
        }
    }

    // -------------------------------------------------------------------- Go
    fn visit_go(&mut self, node: Node, source: &str, sites: &mut Vec<CallSite>) {
        match node.kind() {
            "type_spec" => {
                let (Some(name), Some(body)) = (
                    field_text(node, "name", source),
                    node.child_by_field_name("type"),
                ) else {
                    return;
                };
                if body.kind() != "interface_type" {
                    return;
                }
                let key = (Family::Go, name);
                for member in named_children(body) {
                    if member.kind() != "method_elem" {
                        continue;
                    }
                    if let Some(method) = field_text(member, "name", source) {
                        if let Some(signature) = go_signature(member, source) {
                            self.signatures
                                .entry(key.clone())
                                .or_default()
                                .insert(method.clone(), signature);
                        }
                        self.declare(key.clone(), method, arity(member));
                    }
                }
            }
            "method_declaration" => {
                let (Some(receiver), Some(method)) = (
                    node.child_by_field_name("receiver"),
                    field_text(node, "name", source),
                ) else {
                    return;
                };
                let Some(owner) = type_identifiers(receiver, source).into_iter().next() else {
                    return;
                };
                if let Some(signature) = go_signature(node, source) {
                    self.signatures
                        .entry((Family::Go, owner.clone()))
                        .or_default()
                        .insert(method.clone(), signature);
                }
                self.method_sets
                    .entry((Family::Go, owner))
                    .or_default()
                    .insert(method, arity(node));
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "selector_expression" {
                        push_site(function, "field", source, Family::Go, sites);
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ TypeScript
    fn visit_ts(&mut self, node: Node, source: &str, sites: &mut Vec<CallSite>) {
        match node.kind() {
            "class_declaration" | "abstract_class_declaration" | "interface_declaration" => {
                let Some(name) = field_text(node, "name", source) else {
                    return;
                };
                let key = (Family::Ts, name);

                for child in named_children(node) {
                    match child.kind() {
                        // `class C extends B implements I` and `interface I extends J`.
                        "class_heritage" | "extends_type_clause" => {
                            for supertype in heritage_names(child, source) {
                                self.add_supertype(key.clone(), supertype);
                            }
                        }
                        "class_body" | "interface_body" => {
                            for member in named_children(child) {
                                let is_method = matches!(
                                    member.kind(),
                                    "method_definition"
                                        | "method_signature"
                                        | "abstract_method_signature"
                                ) || is_arrow_field(member);
                                if !is_method {
                                    continue;
                                }
                                if let Some(method) = field_text(member, "name", source) {
                                    self.declare(key.clone(), method, arity(member));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "member_expression" {
                        push_site(function, "property", source, Family::Ts, sites);
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ Java
    fn visit_java(&mut self, node: Node, source: &str, sites: &mut Vec<CallSite>) {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => {
                let Some(name) = field_text(node, "name", source) else {
                    return;
                };
                let key = (Family::Java, name);

                for child in named_children(node) {
                    match child.kind() {
                        // `extends B`, `implements I, J`, `interface I extends J`.
                        "superclass" | "super_interfaces" | "extends_interfaces" => {
                            for supertype in java_supertypes(child, source) {
                                self.add_supertype(key.clone(), supertype);
                            }
                        }
                        "class_body" | "interface_body" | "enum_body" => {
                            // Direct members only: a nested class is visited in its
                            // own right, and its methods are not this type's. An
                            // enum wraps them in one more level.
                            for member in named_children(child) {
                                let members = if member.kind() == "enum_body_declarations" {
                                    named_children(member)
                                } else {
                                    vec![member]
                                };
                                for member in members {
                                    if member.kind() != "method_declaration" {
                                        continue;
                                    }
                                    if let Some(method) = field_text(member, "name", source) {
                                        self.declare(key.clone(), method, arity(member));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // `s.area()` is the call that may dispatch. A bare `area()` is a call on
            // `this`, which the index resolves without a hierarchy.
            "method_invocation" if node.child_by_field_name("object").is_some() => {
                push_site(node, "name", source, Family::Java, sites);
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- Python
    fn visit_python(&mut self, node: Node, source: &str, sites: &mut Vec<CallSite>) {
        match node.kind() {
            "class_definition" => {
                let Some(name) = field_text(node, "name", source) else {
                    return;
                };
                let key = (Family::Python, name);

                if let Some(bases) = node.child_by_field_name("superclasses") {
                    for base in named_children(bases) {
                        // `class C(Base)` and `class C(mod.Base)`; a keyword argument
                        // such as `metaclass=` names no base and is skipped.
                        let base_name = match base.kind() {
                            "identifier" => Some(text(base, source).to_string()),
                            "attribute" => field_text(base, "attribute", source),
                            _ => None,
                        };
                        if let Some(base_name) = base_name {
                            self.add_supertype(key.clone(), base_name);
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    for member in named_children(body) {
                        if member.kind() != "function_definition" {
                            continue;
                        }
                        if let Some(method) = field_text(member, "name", source) {
                            self.declare(key.clone(), method, arity(member));
                        }
                    }
                }
            }
            "call" => {
                if let Some(function) = node.child_by_field_name("function") {
                    if function.kind() == "attribute" {
                        push_site(function, "attribute", source, Family::Python, sites);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Every callable a type owns, keyed by family, owning type and method name.
///
/// This is the bridge from a hierarchy's type names back to the index's symbols.
fn methods_by_owner(index: &Index) -> HashMap<(Family, String, String), Vec<SymbolId>> {
    let mut methods: HashMap<(Family, String, String), Vec<SymbolId>> = HashMap::new();
    for symbol in &index.symbols {
        if !symbol.kind.is_callable() {
            continue;
        }
        let (Some(family), Some(owner)) = (Family::of(symbol.language), symbol.qualifier.as_ref())
        else {
            continue;
        };
        methods
            .entry((family, owner.clone(), symbol.name.clone()))
            .or_default()
            .push(symbol.id);
    }
    methods
}

/// Visit every node of a tree, iteratively — a deeply nested expression must not
/// depend on the stack depth of the analysis.
fn walk(root: Node, visit: &mut impl FnMut(Node)) {
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn field_text(node: Node, field: &str, source: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|child| text(child, source).to_string())
}

fn named_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Record the method name of a call written as `receiver.name(...)`.
fn push_site(accessor: Node, field: &str, source: &str, family: Family, sites: &mut Vec<CallSite>) {
    if let Some(name) = accessor.child_by_field_name(field) {
        sites.push(CallSite {
            offset: name.start_byte(),
            name: text(name, source).to_string(),
            family,
        });
    }
}

/// Every type name mentioned in a type position, outermost first.
///
/// `fmt::Display` names `Display` and a Go receiver `(a *A)` names `A`: the module
/// path and the binding are not type names, and only the grammar's `type_identifier`
/// nodes are. A generic argument *is* one — `Wrapper<T>` yields `Wrapper` then `T` —
/// so a caller after the type being named takes the first and no more.
fn type_identifiers(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut visit = |child: Node| {
        if child.kind() == "type_identifier" {
            names.push(text(child, source).to_string());
        }
    };
    walk(node, &mut visit);
    names
}

/// The types a Java `extends` or `implements` clause names.
///
/// One name per entry, and the outermost one: `implements Iterable<JsonElement>` names
/// `Iterable`. Taking every `type_identifier` under the clause instead makes the type
/// argument a supertype too — which put `JsonArray` under `JsonElement` for the wrong
/// reason and would put a `Comparator<Foo>` under `Foo` for no reason at all.
fn java_supertypes(clause: Node, source: &str) -> Vec<String> {
    let entries: Vec<Node> = named_children(clause)
        .into_iter()
        .flat_map(|child| {
            // `implements` and `interface … extends` wrap their names in a list;
            // `extends B` names one type directly.
            if child.kind() == "type_list" {
                named_children(child)
            } else {
                vec![child]
            }
        })
        .collect();
    entries
        .into_iter()
        .filter_map(|entry| type_identifiers(entry, source).into_iter().next())
        .collect()
}

/// The types in a Go signature, as `(A, B) -> C`.
///
/// Types only: a parameter's *name* is not part of whether one signature satisfies
/// another, and comparing `ctx context.Context` with `c context.Context` would refuse an
/// implementation Go accepts. Returns `None` where the shape cannot be read, which
/// leaves the arity answer standing rather than narrowing to nothing.
fn go_signature(node: Node<'_>, source: &str) -> Option<String> {
    /// A type with its package qualifier dropped and its whitespace squeezed out.
    ///
    /// `kube.ResourceList` and `ResourceList` are the same type written from outside and
    /// from inside the package, and comparing them as text refused
    /// `PrintingKubeClient` as an implementation of an interface it plainly satisfies.
    /// Two same-named types in different packages now match where they did not before,
    /// which is the direction to be wrong in: a spurious dispatch candidate is labelled
    /// as one, and a missing edge is a live method reported as dead.
    fn unqualified(written: &str) -> String {
        let mut out = String::with_capacity(written.len());
        let mut run = String::new();
        for character in written.chars() {
            if character.is_alphanumeric() || character == '_' || character == '.' {
                run.push(character);
                continue;
            }
            if !run.is_empty() {
                out.push_str(run.rsplit('.').next().unwrap_or(&run));
                run.clear();
            }
            if !character.is_whitespace() {
                out.push(character);
            }
        }
        if !run.is_empty() {
            out.push_str(run.rsplit('.').next().unwrap_or(&run));
        }
        out
    }

    fn types_in(list: Node<'_>, source: &str) -> String {
        let mut out = Vec::new();
        for parameter in named_children(list) {
            if parameter.kind().contains("comment") {
                continue;
            }
            let written = match parameter.child_by_field_name("type") {
                Some(ty) => text(ty, source),
                // A bare type with no name is the whole parameter.
                None => text(parameter, source),
            };
            out.push(unqualified(written));
        }
        out.join(",")
    }

    let parameters = node.child_by_field_name("parameters")?;
    let returns = match node.child_by_field_name("result") {
        Some(result) if result.kind().contains("parameter_list") => types_in(result, source),
        Some(result) => unqualified(text(result, source)),
        None => String::new(),
    };
    Some(format!("({}) -> {returns}", types_in(parameters, source)))
}

/// Names in a TypeScript heritage clause: `extends B implements I, J`.
fn heritage_names(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for clause in named_children(node) {
        // `class_heritage` wraps the clauses; `extends_type_clause` is one itself.
        let parts = match clause.kind() {
            "extends_clause" | "implements_clause" => named_children(clause),
            _ => vec![clause],
        };
        for part in parts {
            // A type argument list belongs to the supertype, not beside it.
            if matches!(
                part.kind(),
                "type_identifier" | "identifier" | "nested_type_identifier"
            ) {
                let name = text(part, source);
                names.push(name.rsplit('.').next().unwrap_or(name).to_string());
            }
        }
    }
    names
}

/// A class field holding an arrow function is a method in everything but spelling,
/// and the fact queries already index it as one.
fn is_arrow_field(node: Node) -> bool {
    node.kind() == "public_field_definition"
        && node.child_by_field_name("value").is_some_and(|value| {
            matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "generator_function"
            )
        })
}

/// How many parameters a declaration takes.
///
/// Only Go compares these — implementing an interface there is a structural fact —
/// but every family records them, because the number is what the syntax shows and
/// dropping it would leave Go's rule half-expressed.
fn arity(node: Node) -> usize {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return 0;
    };
    let mut count = 0;
    for parameter in named_children(parameters) {
        match parameter.kind() {
            // Go groups names: `(a, b int)` is one declaration of two parameters.
            "parameter_declaration" => {
                let names = named_children(parameter)
                    .iter()
                    .filter(|child| child.kind() == "identifier")
                    .count();
                count += names.max(1);
            }
            // A Rust receiver is not a parameter. Python's `self` is written as one,
            // and both sides of any comparison carry it, so it needs no special case.
            "self_parameter" | "comment" => {}
            _ => count += 1,
        }
    }
    count
}

/// Symbols that are callable, for reporting.
pub fn callables(index: &Index) -> Vec<&Symbol> {
    index
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{ScanResult, SourceFile};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            crate::vfs::write(&path, content).unwrap();
            scanned.files.push(SourceFile {
                language: crate::lang::detect(&path).unwrap(),
                path,
            });
        }
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn id_of(index: &Index, name: &str) -> SymbolId {
        let found = index.find_symbols(name, None);
        assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
        found[0].id
    }

    #[test]
    fn builds_edges_between_resolved_calls() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "fn leaf() {}\nfn middle() { leaf(); }\nfn top() { middle(); }\n",
        )]);
        let cg = CallGraph::build(&index);

        let top = id_of(&index, "top");
        let middle = id_of(&index, "middle");
        let leaf = id_of(&index, "leaf");

        assert_eq!(cg.callees(top).len(), 1);
        assert_eq!(cg.callees(top)[0].0, middle);
        assert_eq!(cg.callers(leaf).len(), 1);
        assert_eq!(cg.callers(leaf)[0].0, middle);
    }

    #[test]
    fn same_name_functions_in_two_files_are_separate_nodes() {
        // funveil's call graph merged these into one node; this must not.
        let (_tmp, index) = workspace(&[
            ("a.rs", "fn parse() {}\nfn a_top() { parse(); }\n"),
            ("b.rs", "fn parse() {}\nfn b_top() { parse(); }\n"),
        ]);
        let cg = CallGraph::build(&index);

        let parses = index.find_symbols("parse", None);
        assert_eq!(parses.len(), 2);

        // Each parse has exactly one caller, and it is the one in its own file.
        for parse in &parses {
            let callers = cg.callers(parse.id);
            assert_eq!(callers.len(), 1, "parse in {:?}", parse.file);
            let caller = index.symbol(callers[0].0).unwrap();
            assert_eq!(caller.file, parse.file);
        }
    }

    #[test]
    fn trace_walks_transitively_with_depth_bound() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "fn d() {}\nfn c() { d(); }\nfn b() { c(); }\nfn a() { b(); }\n",
        )]);
        let cg = CallGraph::build(&index);
        let a = id_of(&index, "a");

        let deep = cg.trace(a, Direction2::Callees, 10);
        assert_eq!(deep.len(), 4, "should reach a→b→c→d");

        let shallow = cg.trace(a, Direction2::Callees, 1);
        assert_eq!(shallow.len(), 2, "depth 1 reaches only a→b");
    }

    #[test]
    fn recursion_terminates_and_is_reported() {
        let (_tmp, index) = workspace(&[("a.rs", "fn loops() { loops(); }\n")]);
        let cg = CallGraph::build(&index);
        let loops = id_of(&index, "loops");

        let trace = cg.trace(loops, Direction2::Callees, 100);
        assert_eq!(trace.len(), 1, "a self-call must not revisit");
        assert!(!trace.cycles.is_empty(), "the cycle should be reported");
    }

    #[test]
    fn mutual_recursion_terminates() {
        let (_tmp, index) =
            workspace(&[("a.rs", "fn ping() { pong(); }\nfn pong() { ping(); }\n")]);
        let cg = CallGraph::build(&index);
        let trace = cg.trace(id_of(&index, "ping"), Direction2::Callees, 100);
        assert_eq!(trace.len(), 2);
        assert!(!trace.cycles.is_empty());
    }

    #[test]
    fn unresolved_calls_are_recorded_not_dropped() {
        let (_tmp, index) = workspace(&[("a.rs", "fn caller() { external_thing(); }\n")]);
        let cg = CallGraph::build(&index);
        assert!(
            cg.unresolved
                .iter()
                .any(|u| u.callee_name == "external_thing"),
            "an unresolvable call must still be visible: {:?}",
            cg.unresolved
        );
    }

    #[test]
    fn edges_carry_resolution_confidence() {
        let (_tmp, index) = workspace(&[("a.rs", "fn leaf() {}\nfn top() { leaf(); }\n")]);
        let cg = CallGraph::build(&index);
        let edges = cg.callees(id_of(&index, "top"));
        assert_eq!(edges[0].1.confidence, Confidence::Exact);
        assert_eq!(cg.confidence_breakdown().get("exact"), Some(&1));
    }

    #[test]
    fn reachability_from_a_seed() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "fn used() {}\nfn orphan() {}\nfn main() { used(); }\n",
        )]);
        let cg = CallGraph::build(&index);
        let reachable = cg.reachable_from(&[id_of(&index, "main")]);
        assert!(reachable.contains(&id_of(&index, "used")));
        assert!(
            !reachable.contains(&id_of(&index, "orphan")),
            "orphan is not reachable from main"
        );
    }

    #[test]
    fn dot_export_marks_unproven_edges() {
        let (_tmp, index) = workspace(&[("a.rs", "fn leaf() {}\nfn top() { leaf(); }\n")]);
        let cg = CallGraph::build(&index);
        let dot = cg.to_dot(&index);
        assert!(dot.starts_with("digraph calls {"));
        assert!(dot.contains("leaf"));
        assert!(dot.contains("style=solid"), "a proven edge should be solid");
    }

    #[test]
    fn a_file_the_hierarchy_pass_cannot_read_is_reported() {
        // The dispatch layer reads files itself. One it cannot read yields no edges,
        // which widens the unused list rather than narrowing it — but pretending the
        // file simply had no hierarchy would hide the difference.
        let (tmp, index) = workspace(&[(
            "a.rs",
            "trait T { fn m(&self); }\nstruct S;\nimpl T for S { fn m(&self) {} }\n",
        )]);
        std::fs::remove_file(tmp.path().join("a.rs")).unwrap();

        let cg = CallGraph::build(&index);
        assert_eq!(cg.hierarchy_gaps.len(), 1, "got {:?}", cg.hierarchy_gaps);
        assert!(cg.hierarchy_gaps[0].0.ends_with("a.rs"));
    }

    #[test]
    fn a_dispatch_candidate_is_never_a_proven_edge() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "trait T { fn m(&self); }\nstruct A;\nstruct B;\nimpl T for A { fn m(&self) {} }\n\
             impl T for B { fn m(&self) {} }\nfn go(t: &dyn T) { t.m(); }\n",
        )]);
        let cg = CallGraph::build(&index);
        assert_eq!(
            cg.hierarchy_edge_count(),
            3,
            "two impls and the declaration"
        );
        for (_, edge) in cg.callees(id_of(&index, "go")) {
            assert_eq!(edge.confidence, Confidence::FieldBased);
            assert!(edge.origin.is_hierarchy());
        }
        assert_eq!(
            cg.origin_breakdown().get("implemented-trait"),
            Some(&3),
            "{:?}",
            cg.origin_breakdown()
        );
    }

    #[test]
    fn methods_are_qualified_in_output() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "struct S;\nimpl S {\n    fn helper(&self) {}\n    fn run(&self) { self.helper(); }\n}\n",
        )]);
        let cg = CallGraph::build(&index);
        let dot = cg.to_dot(&index);
        assert!(dot.contains("S::helper"), "got:\n{dot}");
    }
}
