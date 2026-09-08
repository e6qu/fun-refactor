use super::{bounded_text, hash, path_confidence, Project, RelationshipOptions};
use crate::analysis::{
    call_graph::{CallEdge, CallGraph, Hierarchy},
    entrypoints::{Catalog, EntryKind},
};
use crate::model::SymbolId;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

type Step = (usize, Option<(SymbolId, CallEdge)>);

fn witnesses(
    graph: &CallGraph,
    seeds: Vec<SymbolId>,
    depth: usize,
) -> (BTreeMap<SymbolId, Step>, usize) {
    let mut incoming: BTreeMap<SymbolId, Vec<(SymbolId, CallEdge)>> = BTreeMap::new();
    for (caller, callee, edge) in graph.edges() {
        incoming
            .entry(callee)
            .or_default()
            .push((caller, edge.clone()));
    }
    let mut reached: BTreeMap<_, _> = seeds.iter().map(|id| (*id, (0, None))).collect();
    let mut queue: VecDeque<_> = seeds.into();
    while let Some(callee) = queue.pop_front() {
        let distance = reached[&callee].0;
        if distance == depth {
            continue;
        }
        for (caller, edge) in incoming.get(&callee).into_iter().flatten() {
            if let std::collections::btree_map::Entry::Vacant(slot) = reached.entry(*caller) {
                slot.insert((distance + 1, Some((callee, edge.clone()))));
                queue.push_back(*caller);
            }
        }
    }
    let frontier = reached
        .iter()
        .filter(|(id, (distance, _))| {
            *distance == depth
                && incoming
                    .get(id)
                    .into_iter()
                    .flatten()
                    .any(|(caller, _)| !reached.contains_key(caller))
        })
        .count();
    (reached, frontier)
}

impl Project<'_> {
    pub(super) fn tests(&self, options: &RelationshipOptions, depth: usize) -> Result<Value> {
        ensure!(
            depth <= 16,
            "Test call-path depth must be from 0 through 16."
        );
        let selected = self.relationship_selection(options)?;
        let catalog = Catalog::builtin()?;
        let candidates = catalog.tests_in_snapshot(self.index, &self.sources);
        let hierarchy = Hierarchy::scan(self.index);
        let graph = CallGraph::build_with(self.index, &hierarchy);
        let seeds: Vec<_> = self
            .index
            .symbols
            .iter()
            .filter(|s| s.kind.is_callable() && self.scope_symbol(selected, s.id))
            .map(|s| s.id)
            .collect();
        let (reached, frontier) = witnesses(&graph, seeds, depth);
        let mut rows = Vec::new();
        let mut selected_tests = 0usize;
        for entry in &candidates.entries {
            let in_scope = self.scope_symbol(selected, entry.symbol);
            if !in_scope && !reached.contains_key(&entry.symbol) {
                continue;
            }
            selected_tests += 1;
            let mut path = Vec::new();
            let mut current = entry.symbol;
            if !in_scope {
                while let Some((_, Some((callee, edge)))) = reached.get(&current) {
                    path.push((current, *callee, edge));
                    current = *callee;
                }
            }
            let confidence = (!path.is_empty()).then(|| {
                path_confidence(
                    &path
                        .iter()
                        .map(|(_, _, edge)| edge.confidence)
                        .collect::<Vec<_>>(),
                )
            });
            let id = format!(
                "frpt1:{}",
                &hash((&self.revision, selected, depth, entry.symbol))?[..32]
            );
            rows.push(
                json!({"kind": "test-candidate", "id": id, "test": self.endpoint(entry.symbol)?,
                "rule": bounded_text(&entry.rule, 160), "catalog": "builtin", "status": "candidate",
                "basis": if in_scope { "catalog-in-scope" } else { "catalog-and-call-path" },
                "confidence": null, "path_confidence": confidence, "hops": path.len(),
                "target": if in_scope { None } else { Some(self.endpoint(current)?) }}),
            );
            for (position, (caller, callee, edge)) in path.iter().enumerate() {
                rows.push(json!({"kind": "test-path-edge", "test_candidate": id, "step": position + 1,
                    "caller": self.endpoint(*caller)?, "callee": self.endpoint(*callee)?,
                    "site": self.call_site(&edge.file, edge.offset)?, "confidence": edge.confidence,
                    "origin": edge.origin.as_str(), "dispatch_candidate": edge.origin.is_dispatch(), "status": "candidate"}));
            }
        }
        let mut analysis = self.relationship_gaps(&graph.hierarchy_gaps, &mut rows, true);
        for (file, reason) in &candidates.gaps {
            rows.push(json!({"kind": "analysis-gap", "path": bounded_text(&file.strip_prefix(&self.root)?.to_string_lossy(), 512),
                "reason": bounded_text(reason, 512), "basis": "test-catalog"}));
        }
        let mut missing_rules = BTreeMap::new();
        for (_, info) in self.index.files() {
            if !catalog.rules.iter().any(|r| {
                r.kind == EntryKind::Test && r.languages.iter().any(|l| l.covers(info.language))
            }) {
                *missing_rules.entry(info.language.name()).or_insert(0usize) += 1;
            }
        }
        for (language, files) in &missing_rules {
            rows.push(
                json!({"kind": "coverage-gap", "language": language, "files": files,
                "basis": "test-catalog", "reason": "no built-in test rules for this language."}),
            );
        }
        analysis["catalog_candidates"] = json!(candidates.entries.len());
        analysis["selected_candidates"] = json!(selected_tests);
        analysis["catalog_gaps"] = json!(candidates.gaps.len());
        analysis["files_without_test_rules"] = json!(missing_rules);
        analysis["max_depth"] = json!(depth);
        analysis["depth_frontier_nodes"] = json!(frontier);
        analysis["unresolved_calls"] = json!(graph.unresolved.len());
        analysis["file_scope_calls"] = json!(graph.file_scope.len());
        analysis["certainty"] = json!("Catalog candidates include fixtures, setup hooks and convention-matched helpers. Call paths do not establish test discovery, execution or runtime coverage.");
        analysis["limitations"] = json!("One deterministic shortest witness per candidate; alternatives may have different confidence. Unresolved and file-scope calls do not extend paths. Page limits bound output, not workspace analysis.");
        let mut result =
            self.relationship_page("tests", selected, options, None, rows, analysis)?;
        result["scope"] = json!("Tests in the selection and candidate callers of selected callable definitions, including nested definitions. Workspace diagnostics share the page.");
        Ok(result)
    }
}

#[cfg(test)]
mod checks {
    use super::*;
    use crate::model::Confidence;

    #[test]
    fn a_short_weak_witness_keeps_its_confidence_beside_a_longer_strong_route() {
        let (_dir, mut index) = crate::testing::workspace(&[("app.py",
            "def leaf():\n    pass\n\ndef bridge():\n    leaf()\n\ndef test_choice():\n    leaf()\n    bridge()\n")]);
        let leaf = index.find_symbols("leaf", None)[0].id;
        let test = index.find_symbols("test_choice", None)[0];
        let test_id = test.id;
        let span = test.full_span;
        let mut changed = 0;
        for reference in &mut index.references {
            if reference.target == Some(leaf) && span.contains(reference.span) {
                reference.confidence = Confidence::NameOnly;
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        let graph = CallGraph::build(&index);
        let (paths, _) = witnesses(&graph, vec![leaf], 2);
        let (hops, Some((target, edge))) = &paths[&test_id] else {
            panic!("missing path")
        };
        assert_eq!(*hops, 1);
        assert_eq!(*target, leaf);
        assert_eq!(edge.confidence, Confidence::NameOnly);
    }
}
