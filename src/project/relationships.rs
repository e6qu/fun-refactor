use super::{bounded_text, check_limit, hash, page, CallDirection, Project, RelationshipOptions};
use crate::analysis::call_graph::{CallGraph, Family, Hierarchy};
use crate::capabilities::{support, Capability};
use crate::model::SymbolId;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

impl Project<'_> {
    fn relationship_selection(&self, options: &RelationshipOptions) -> Result<usize> {
        check_limit(options.limit)?;
        self.target(&self.explicit_handle(&options.target, options.revision.as_deref())?)
    }

    fn scope_file(&self, selected: usize, file: &Path) -> bool {
        let node = &self.nodes[selected];
        let Ok(relative) = file.strip_prefix(&self.root) else {
            return false;
        };
        if node.kind == "directory" {
            relative.starts_with(&node.path)
        } else {
            relative == node.path
        }
    }

    fn scope_site(&self, selected: usize, file: &Path, offset: usize) -> bool {
        self.scope_file(selected, file)
            && self.nodes[selected]
                .symbol
                .and_then(|id| self.index.symbol(id))
                .is_none_or(|symbol| {
                    symbol.full_span.start <= offset && offset < symbol.full_span.end
                })
    }

    fn scope_symbol(&self, selected: usize, symbol: SymbolId) -> bool {
        self.index.symbol(symbol).is_some_and(|symbol| {
            self.scope_file(selected, &symbol.file)
                && self.nodes[selected]
                    .symbol
                    .and_then(|id| self.index.symbol(id))
                    .is_none_or(|parent| parent.full_span.contains(symbol.full_span))
        })
    }

    fn endpoint(&self, symbol: SymbolId) -> Result<Value> {
        let node = *self
            .symbol_nodes
            .get(&symbol)
            .context("relationship target has no project node")?;
        let symbol = self
            .index
            .symbol(symbol)
            .context("relationship target has no indexed symbol")?;
        let source = self
            .sources
            .get(&symbol.file)
            .context("relationship target has no source snapshot")?;
        Ok(
            json!({"handle": self.handle(node), "name": bounded_text(&symbol.name, 160), "kind": symbol.kind,
            "qualifier": symbol.qualifier.as_deref().map(|q| bounded_text(q, 160)),
            "path": bounded_text(&symbol.file.strip_prefix(&self.root)?.to_string_lossy(), 512),
            "line": self.lines[&symbol.file].line_col(symbol.name_span.start, source).line}),
        )
    }

    fn call_site(&self, file: &Path, offset: usize) -> Result<Value> {
        let source = self
            .sources
            .get(file)
            .context("call site has no source snapshot")?;
        anyhow::ensure!(
            offset <= source.len() && source.is_char_boundary(offset),
            "call site is outside its source snapshot."
        );
        let position = self.lines[file].line_col(offset, source);
        Ok(
            json!({"path": bounded_text(&file.strip_prefix(&self.root)?.to_string_lossy(), 512),
            "offset": offset, "line": position.line, "column": position.col}),
        )
    }

    fn relation_scope(
        direction: CallDirection,
        incoming: bool,
        outgoing: bool,
    ) -> Option<&'static str> {
        let included = match direction {
            CallDirection::Both => incoming || outgoing,
            CallDirection::Incoming => incoming,
            CallDirection::Outgoing => outgoing,
        };
        included.then_some(if incoming && outgoing {
            "internal"
        } else if incoming {
            "incoming"
        } else {
            "outgoing"
        })
    }

    fn relationship_gaps(
        &self,
        gaps: &[(PathBuf, String)],
        rows: &mut Vec<Value>,
        calls: bool,
    ) -> Value {
        for (path, reason) in gaps {
            rows.push(json!({"kind": "analysis-gap", "path": bounded_text(&path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy(), 512),
                "reason": bounded_text(reason, 512), "basis": "hierarchy-analysis"}));
        }
        let mut unsupported = BTreeMap::new();
        let mut hierarchy_unsupported = BTreeMap::new();
        for (_, file) in self.index.files() {
            if Family::of(file.language).is_none() {
                *hierarchy_unsupported
                    .entry(file.language.name())
                    .or_insert(0usize) += 1;
            }
            let supported = if calls {
                support(Capability::CallGraph, file.language).is_yes()
            } else {
                Family::of(file.language).is_some()
            };
            if !supported {
                *unsupported.entry(file.language.name()).or_insert(0usize) += 1;
            }
        }
        for (language, files) in &unsupported {
            rows.push(
                json!({"kind": "coverage-gap", "language": language, "files": files,
                "reason": "analysis is unavailable for this language"}),
            );
        }
        json!({"hierarchy_gaps": gaps.len(), "unsupported_files": unsupported,
            "hierarchy_unsupported_files": hierarchy_unsupported, "scope": "indexed workspace"})
    }

    fn relationship_page(
        &self,
        query: &str,
        selected: usize,
        options: &RelationshipOptions,
        direction: Option<CallDirection>,
        mut rows: Vec<Value>,
        analysis: Value,
    ) -> Result<Value> {
        rows.sort_by_cached_key(Value::to_string);
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, query, selected, direction, &rows))?[..32]
        );
        let (start, end, page) = page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let mut result = self.envelope(query);
        result["root"] = json!(self.handle(selected));
        result["items"] = json!(&rows[start..end]);
        result["page"] = page;
        result["analysis"] = analysis;
        if let Some(direction) = direction {
            result["direction"] = json!(direction)
        }
        Ok(result)
    }

    pub(super) fn calls(
        &self,
        options: &RelationshipOptions,
        direction: CallDirection,
    ) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        let hierarchy = Hierarchy::scan(self.index);
        let graph = CallGraph::build_with(self.index, &hierarchy);
        let mut rows = Vec::new();
        for (caller, callee, edge) in graph.edges() {
            let Some(scope) = Self::relation_scope(
                direction,
                self.scope_symbol(selected, callee),
                self.scope_site(selected, &edge.file, edge.offset),
            ) else {
                continue;
            };
            rows.push(json!({"kind": "call", "scope_relation": scope,
                "caller": self.endpoint(caller)?, "callee": self.endpoint(callee)?, "site": self.call_site(&edge.file, edge.offset)?,
                "confidence": edge.confidence, "origin": edge.origin.as_str(), "dispatch_candidate": edge.origin.is_dispatch(),
                "status": if edge.origin.is_dispatch() { "dispatch-candidate" } else { "indexed-target" }}));
        }
        for call in &graph.file_scope {
            let Some(scope) = Self::relation_scope(
                direction,
                self.scope_symbol(selected, call.callee),
                self.scope_site(selected, &call.file, call.offset),
            ) else {
                continue;
            };
            rows.push(json!({"kind": "call", "scope_relation": scope, "caller": null, "callee": self.endpoint(call.callee)?,
                "site": self.call_site(&call.file, call.offset)?, "confidence": call.confidence, "origin": "resolved",
                "dispatch_candidate": false, "status": "indexed-target", "caller_scope": "file"}));
        }
        for call in &graph.unresolved {
            let Some(scope) = Self::relation_scope(
                direction,
                false,
                self.scope_site(selected, &call.file, call.offset),
            ) else {
                continue;
            };
            rows.push(json!({"kind": "call", "scope_relation": scope,
                "caller": call.caller.map(|s| self.endpoint(s)).transpose()?, "callee": null,
                "name": bounded_text(&call.callee_name, 160), "site": self.call_site(&call.file, call.offset)?,
                "confidence": call.confidence, "origin": "unresolved", "dispatch_candidate": false, "status": "unresolved"}));
        }
        let mut analysis = self.relationship_gaps(&graph.hierarchy_gaps, &mut rows, true);
        analysis["callable_nodes"] = json!(graph.node_count());
        analysis["edges"] = json!(graph.edge_count());
        analysis["unresolved_calls"] = json!(graph.unresolved.len());
        analysis["file_scope_calls"] = json!(graph.file_scope.len());
        let mut result =
            self.relationship_page("calls", selected, options, Some(direction), rows, analysis)?;
        result["scope"] = json!("Call sites and callee definitions within the selection; nested definitions included. Workspace diagnostics share the page.");
        Ok(result)
    }

    pub(super) fn implementations(&self, options: &RelationshipOptions) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        let hierarchy = Hierarchy::scan(self.index);
        let mut rows = Vec::new();
        for symbol in &self.index.symbols {
            if !self.scope_symbol(selected, symbol.id) {
                continue;
            }
            for implementation in hierarchy.implementations_of(self.index, symbol.id) {
                rows.push(json!({"kind": "implementation", "declaration": self.endpoint(symbol.id)?,
                    "implementation": self.endpoint(implementation)?, "status": "candidate", "basis": "hierarchy-analysis", "confidence": null}));
            }
        }
        let mut analysis = self.relationship_gaps(&hierarchy.gaps, &mut rows, false);
        analysis["certainty"] = json!("Hierarchy candidates; the analyzer provides no per-implementation confidence or runtime guarantee.");
        let mut result =
            self.relationship_page("implementations", selected, options, None, rows, analysis)?;
        result["scope"] = json!("Implementations of selected declarations, including nested declarations. Workspace diagnostics share the page.");
        Ok(result)
    }
}
