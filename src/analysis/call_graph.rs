//! Call graph construction over the resolved index.
//!
//! The API shape (callers / callees / trace / DOT export) follows funveil's call
//! graph, but the resolution underneath is different in the way that matters: funveil
//! matched callee names as strings in a single flat namespace, so a `parse` in one
//! file and a `parse` in another became one node. Here every edge comes from a
//! resolved reference and carries the [`Confidence`] of that resolution, so callers
//! can distinguish a proven call from a plausible one.

use crate::index::Index;
use crate::model::{Confidence, ReferenceKind, Symbol, SymbolId, SymbolKind};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// A call edge between two functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEdge {
    /// Byte offset of the call site.
    pub offset: usize,
    pub file: std::path::PathBuf,
    pub confidence: Confidence,
}

/// A directed graph of callables.
#[derive(Debug, Default)]
pub struct CallGraph {
    graph: DiGraph<SymbolId, CallEdge>,
    nodes: HashMap<SymbolId, NodeIndex>,
    /// Call sites whose callee could not be resolved, kept so they can be reported
    /// rather than silently dropped.
    pub unresolved: Vec<UnresolvedCall>,
}

/// A call site we could see but not resolve to a definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedCall {
    pub caller: Option<SymbolId>,
    pub callee_name: String,
    pub file: std::path::PathBuf,
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
    pub fn build(index: &Index) -> Self {
        let mut cg = CallGraph::default();

        // Every callable becomes a node, so a function with no edges still appears.
        for symbol in &index.symbols {
            if symbol.kind.is_callable() {
                cg.node_for(symbol.id);
            }
        }

        for reference in &index.references {
            if reference.kind != ReferenceKind::Call {
                continue;
            }
            let caller = enclosing_callable(index, &reference.file, reference.span.start);

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
                    let from = cg.node_for(caller_id);
                    let to = cg.node_for(callee.id);
                    cg.graph.add_edge(
                        from,
                        to,
                        CallEdge {
                            offset: reference.span.start,
                            file: reference.file.clone(),
                            confidence: reference.confidence,
                        },
                    );
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
        cg
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

        queue.push_back((start, 0usize, None));
        visited.insert(start);

        while let Some((id, depth, via)) = queue.pop_front() {
            nodes.push(TraceNode {
                symbol: id,
                depth,
                caller: via,
            });
            if depth >= max_depth {
                continue;
            }
            let next = match direction {
                Direction2::Callers => self.callers(id),
                Direction2::Callees => self.callees(id),
            };
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
        TraceResult {
            start,
            direction,
            nodes,
            cycles,
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
    pub fn reachable_from(&self, seeds: &[SymbolId]) -> HashSet<SymbolId> {
        let mut seen: HashSet<SymbolId> = HashSet::new();
        let mut queue: VecDeque<SymbolId> = VecDeque::new();
        for seed in seeds {
            if seen.insert(*seed) {
                queue.push_back(*seed);
            }
        }
        while let Some(id) = queue.pop_front() {
            for (callee, _) in self.callees(id) {
                if seen.insert(callee) {
                    queue.push_back(callee);
                }
            }
        }
        seen
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
            // Unproven edges are dashed so a picture cannot overstate certainty.
            let style = if edge.weight().confidence.is_safe_to_rewrite() {
                "solid"
            } else {
                "dashed"
            };
            out.push_str(&format!(
                "  n{} -> n{} [style={}];\n",
                from.0, to.0, style
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
            out.push_str(&format!(
                "{}{}{}\n",
                "  ".repeat(node.depth),
                name,
                marker
            ));
        }
        if !self.cycles.is_empty() {
            out.push_str(&format!("\n{} cycle(s) detected\n", self.cycles.len()));
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
fn enclosing_callable(index: &Index, file: &std::path::Path, offset: usize) -> Option<SymbolId> {
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
            std::fs::write(&path, content).unwrap();
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
        let (_tmp, index) = workspace(&[("a.rs", "fn ping() { pong(); }\nfn pong() { ping(); }\n")]);
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
            cg.unresolved.iter().any(|u| u.callee_name == "external_thing"),
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
