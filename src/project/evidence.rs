use super::*;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

impl Project<'_> {
    fn evidence_symbol(&self, symbol: crate::model::SymbolId) -> Value {
        let Some(item) = self.index.symbol(symbol) else {
            return json!({"status": "unknown"});
        };
        let handle = self
            .symbol_nodes
            .get(&symbol)
            .map(|node| self.handle(*node));
        let path = item
            .file
            .strip_prefix(&self.root)
            .unwrap_or(&item.file)
            .to_string_lossy();
        let position = self.sources.get(&item.file).map(|source| {
            let at = self.lines[&item.file].line_col(item.name_span.start, source);
            json!({"line": at.line, "col": at.col})
        });
        json!({
            "handle": handle,
            "name": bounded_text(&item.qualified_name(), 160),
            "kind": item.kind.as_str(),
            "path": bounded_text(&path, 256),
            "position": position,
            "language": item.language.name(),
            "exported": item.exported
        })
    }

    fn evidence_hierarchy(&self, selected: usize, depth: usize) -> Value {
        let row = |id: usize, level: usize| {
            let node = &self.nodes[id];
            json!({
                "handle": self.handle(id),
                "parent": node.parent.map(|parent| self.handle(parent)),
                "name": bounded_text(&node.name, 160),
                "kind": node.kind,
                "path": bounded_text(&node.path.to_string_lossy(), 256),
                "depth": level,
                "children": node.children.len()
            })
        };
        let mut ancestors = Vec::new();
        let mut current = Some(selected);
        while let Some(id) = current {
            ancestors.push(id);
            current = self.nodes[id].parent;
        }
        ancestors.reverse();
        let ancestors = ancestors
            .iter()
            .enumerate()
            .map(|(level, id)| row(*id, level))
            .collect::<Vec<_>>();

        let mut stack = self.nodes[selected]
            .children
            .iter()
            .rev()
            .map(|child| (*child, 1usize))
            .collect::<Vec<_>>();
        let mut descendants = Vec::new();
        let mut omitted = 0usize;
        while let Some((id, level)) = stack.pop() {
            if level > depth || descendants.len() >= 256 {
                omitted += 1;
                continue;
            }
            descendants.push(row(id, level));
            stack.extend(
                self.nodes[id]
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, level + 1)),
            );
        }
        json!({
            "target": row(selected, 0),
            "ancestors": ancestors,
            "descendants": descendants,
            "omitted": {"depth_or_row_bound": omitted, "row_limit": 256, "depth": depth}
        })
    }

    pub(super) fn evidence_trace_for_intent(
        &self,
        graph: &crate::analysis::call_graph::CallGraph,
        symbol: crate::model::SymbolId,
        callers: bool,
        depth: usize,
    ) -> Value {
        self.evidence_trace(
            graph,
            symbol,
            if callers {
                crate::analysis::call_graph::Direction2::Callers
            } else {
                crate::analysis::call_graph::Direction2::Callees
            },
            depth,
        )
    }

    fn evidence_trace(
        &self,
        graph: &crate::analysis::call_graph::CallGraph,
        symbol: crate::model::SymbolId,
        direction: crate::analysis::call_graph::Direction2,
        depth: usize,
    ) -> Value {
        let trace = graph.trace(symbol, direction, depth);
        let nodes = trace
            .nodes
            .iter()
            .take(256)
            .map(|node| {
                json!({
                    "symbol": self.evidence_symbol(node.symbol),
                    "depth": node.depth,
                    "via": node.caller.map(|(parent, confidence)| json!({
                        "parent": self.evidence_symbol(parent),
                        "confidence": confidence.as_str()
                    }))
                })
            })
            .collect::<Vec<_>>();
        json!({
            "direction": match direction {
                crate::analysis::call_graph::Direction2::Callers => "callers",
                crate::analysis::call_graph::Direction2::Callees => "callees",
            },
            "nodes": nodes,
            "cycles": trace.cycles.iter().take(128).map(|(from, to)| json!({
                "from": self.evidence_symbol(*from), "to": self.evidence_symbol(*to)
            })).collect::<Vec<_>>(),
            "unexplored": trace.unexplored.iter().take(128).map(|id| self.evidence_symbol(*id)).collect::<Vec<_>>(),
            "omitted": {
                "nodes": trace.nodes.len().saturating_sub(256),
                "cycles": trace.cycles.len().saturating_sub(128),
                "unexplored": trace.unexplored.len().saturating_sub(128)
            }
        })
    }

    pub(super) fn evidence_flow_steps(&self, result: &crate::analysis::flow::FlowResult) -> Value {
        let steps = result
            .steps
            .iter()
            .take(128)
            .map(|step| {
                let path = step.file.strip_prefix(&self.root).unwrap_or(&step.file);
                let position = self.sources.get(&step.file).map(|source| {
                    let at = self.lines[&step.file].line_col(step.span.start, source);
                    json!({"line": at.line, "col": at.col})
                });
                json!({
                    "symbol": step.symbol.map(|symbol| self.evidence_symbol(symbol)),
                    "path": bounded_text(&path.to_string_lossy(), 256),
                    "position": position,
                    "depth": step.depth,
                    "confidence": step.confidence.as_str()
                })
            })
            .collect::<Vec<_>>();
        let boundaries = result
            .stops
            .iter()
            .take(128)
            .map(|(depth, reason)| {
                let kind = match reason {
                    crate::analysis::flow::StopReason::Origin(_) => "origin",
                    crate::analysis::flow::StopReason::UnresolvedCall(_) => "unresolved-call",
                    crate::analysis::flow::StopReason::Unresolved(_) => "unresolved",
                    crate::analysis::flow::StopReason::TooWeak(_) => "confidence-boundary",
                    crate::analysis::flow::StopReason::DepthLimit => "depth-limit",
                    crate::analysis::flow::StopReason::CrossesFunctionBoundary(_) => {
                        "function-boundary"
                    }
                };
                json!({"depth": depth, "kind": kind})
            })
            .collect::<Vec<_>>();
        json!({
            "steps": steps,
            "boundaries": boundaries,
            "omitted": {
                "steps": result.steps.len().saturating_sub(128),
                "boundaries": result.stops.len().saturating_sub(128)
            }
        })
    }

    pub(super) fn evidence_model(&self, selected: usize, depth: usize) -> Result<Value> {
        ensure!(
            disclosure_view_admitted(1, depth),
            "evidence disclosure depth must be at most 8."
        );
        let Some(symbol_id) = self.nodes[selected].symbol else {
            let hierarchy = self.evidence_hierarchy(selected, depth);
            let candidates = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(id, node)| self.within(*id, selected) && node.symbol.is_some())
                .collect::<Vec<_>>();
            let next = candidates.iter().take(32).map(|(id, _)| json!({
                "target": self.handle(*id),
                "arguments": ["project", "disclose", self.handle(*id), "--view", "evidence"],
            })).collect::<Vec<_>>();
            let narrow = json!({"status":"requires-declaration",
                "reason":"select an exact declaration for calls, impact and value flow; scope rows are structural evidence.",
                "candidates":next,"omitted_candidates":candidates.len().saturating_sub(32)});
            return Ok(
                json!({"schema":"fr-project-evidence-1", "target":hierarchy["target"],
                "code_map":hierarchy,"call_traces":narrow,"impact":narrow,"sources_and_sinks":narrow}),
            );
        };
        let symbol = self
            .index
            .symbol(symbol_id)
            .context("selected evidence declaration is absent from the index")?;
        let graph = crate::analysis::call_graph::CallGraph::built(self.index);

        let calls = if symbol.kind.is_callable() {
            json!({
                "status": "returned",
                "callers": self.evidence_trace(&graph, symbol_id, crate::analysis::call_graph::Direction2::Callers, depth),
                "callees": self.evidence_trace(&graph, symbol_id, crate::analysis::call_graph::Direction2::Callees, depth),
                "hierarchy_gaps": graph.hierarchy_gaps.iter().take(64).map(|(path, _)| bounded_text(
                    &path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy(), 256
                )).collect::<Vec<_>>()
            })
        } else {
            json!({"status": "not-applicable", "reason": "selected declaration is not callable"})
        };

        let impact =
            crate::analysis::impact::analyse_with_graph(self.index, symbol_id, depth, &graph)?;
        let impact_total = impact.items.len();
        let impact_items = impact
            .items
            .iter()
            .take(256)
            .map(|item| json!({
                "path": bounded_text(&item.file.strip_prefix(&self.root).unwrap_or(&item.file).to_string_lossy(), 256),
                "position": {"line": item.line, "col": item.col},
                "language": item.language.name(),
                "kind": item.kind.as_str(),
                "confidence": item.confidence.as_str()
            }))
            .collect::<Vec<_>>();

        let mut seeds = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(id, node)| {
                *id == selected || (self.within(*id, selected) && node.symbol.is_some())
            })
            .filter_map(|(_, node)| node.symbol)
            .filter(|id| {
                self.index.symbol(*id).is_some_and(|item| {
                    matches!(
                        item.kind,
                        crate::model::SymbolKind::Variable
                            | crate::model::SymbolKind::Constant
                            | crate::model::SymbolKind::Parameter
                            | crate::model::SymbolKind::Field
                            | crate::model::SymbolKind::Key
                    )
                })
            })
            .collect::<Vec<_>>();
        seeds.sort();
        seeds.dedup();
        let seed_total = seeds.len();
        let mut flow_rows = Vec::new();
        if crate::analysis::flow::supports_flow(symbol.language) {
            for seed in seeds.iter().take(24) {
                let item = self.index.symbol(*seed).context("flow seed disappeared")?;
                let backward = crate::analysis::flow::backward(
                    self.index,
                    &item.file,
                    item.name_span.start,
                    depth,
                )?;
                let forward = crate::analysis::flow::forward(self.index, *seed, depth)?;
                flow_rows.push(json!({
                    "binding": self.evidence_symbol(*seed),
                    "sources": self.evidence_flow_steps(&backward),
                    "sinks": self.evidence_flow_steps(&forward)
                }));
            }
        }

        Ok(json!({
            "schema": "fr-project-evidence-1",
            "scope": "Source-free static evidence. Calls, impact and value flow preserve confidence and explicit bounds; sources and sinks are local value origins and uses, not a security-taint claim.",
            "target": self.evidence_symbol(symbol_id),
            "code_map": self.evidence_hierarchy(selected, depth),
            "call_traces": calls,
            "impact": {
                "items": impact_items,
                "callers_beyond_depth": impact.callers_beyond_the_depth_limit,
                "omitted": impact_total.saturating_sub(256)
            },
            "sources_and_sinks": {
                "status": if crate::analysis::flow::supports_flow(symbol.language) { "returned" } else { "not-applicable" },
                "flows": flow_rows,
                "omitted_bindings": seed_total.saturating_sub(24),
                "depth": depth
            }
        }))
    }
}
