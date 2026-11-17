use super::*;
use crate::span::LineIndex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

impl CallGraph {
    pub fn report(&self, index: &Index, root: &Path) -> Value {
        let relative = |path: &Path| path.strip_prefix(root).unwrap_or(path).to_path_buf();
        let sources: BTreeMap<_, _> = index.files().map(|(path, _)| {
            #[cfg(feature = "cli")]
            let source = index.captured_source(path).map(str::to_owned);
            #[cfg(not(feature = "cli"))]
            let source = crate::vfs::read_to_string(path).ok().filter(|text| {
                index.content_hash(path) == Some(crate::index::content_hash_of(text))
            });
            (path.clone(), source)
        }).collect();
        let source_digests: BTreeMap<_, _> = sources.iter().map(|(file, source)| {
            (relative(file), source.as_ref().map(|text| hex::encode(Sha256::digest(text.as_bytes()))))
        }).collect();
        let lines: BTreeMap<_, _> = sources.iter().filter_map(|(file, source)| {
            source.as_ref().map(|text| (file.as_path(), LineIndex::new(text)))
        }).collect();
        let mut reference_spans: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for (file, _) in index.files() {
            for reference in index.references_in(file) {
                reference_spans.entry((file.as_path(), reference.span.start))
                    .or_default().insert(reference.span);
            }
        }
        let site = |file: &Path, offset: usize| {
            let source = sources.get(file).and_then(Option::as_deref);
            let position = source.filter(|s| offset <= s.len() && s.is_char_boundary(offset))
                .and_then(|s| lines.get(file).map(|lines| lines.line_col(offset, s)));
            let spans = reference_spans.get(&(file, offset)).cloned().unwrap_or_default();
            json!({"file":relative(file), "offset":offset,
                "line":position.as_ref().map(|p| p.line), "column":position.as_ref().map(|p| p.col),
                "spans":spans, "source_available":source.is_some()})
        };
        let nodes: Vec<_> = self.nodes().into_iter().filter_map(|id| index.symbol(id))
            .map(|symbol| {
                let location = site(&symbol.file, symbol.name_span.start);
                json!({"id":symbol.id, "name":symbol.qualified_name(), "file":relative(&symbol.file),
                    "line":location["line"], "kind":symbol.kind,
                    "name_span":symbol.name_span, "definition_span":symbol.full_span})
            }).collect();
        let edges: Vec<_> = self.edges().into_iter().map(|(from, to, edge)| {
            json!({"from":from, "to":to, "confidence":edge.confidence.as_str(),
                "origin":edge.origin.as_str(), "dispatch_candidate":edge.origin.is_dispatch(),
                "site":site(&edge.file, edge.offset)})
        }).collect();
        let unresolved: Vec<_> = self.unresolved.iter().map(|call| {
            json!({"caller":call.caller, "name":call.callee_name,
                "confidence":call.confidence.as_str(), "site":site(&call.file, call.offset)})
        }).collect();
        let file_scope: Vec<_> = self.file_scope.iter().map(|call| {
            json!({"callee":call.callee, "name":call.callee_name,
                "confidence":call.confidence.as_str(), "site":site(&call.file, call.offset)})
        }).collect();
        let mut gaps: Vec<_> = self.hierarchy_gaps.iter().map(|(file, reason)| {
            json!({"file":relative(file), "reason":reason, "kind":"hierarchy"})
        }).collect();
        gaps.extend(index.skipped.iter().map(|(file, reason)| {
            json!({"file":relative(file), "reason":reason, "kind":"skipped-file"})
        }));
        for (file, info) in index.files() {
            for gap in &info.gaps {
                gaps.push(json!({"file":relative(file), "kind":"syntax", "reason":gap.cause()}));
            }
            if !crate::capabilities::support(crate::capabilities::Capability::CallGraph, info.language).is_yes() {
                gaps.push(json!({"file":relative(file), "kind":"unsupported-language", "language":info.language.name()}));
            }
            if sources.get(file).is_none_or(Option::is_none) {
                gaps.push(json!({"file":relative(file), "kind":"source-unavailable"}));
            }
        }
        gaps.sort_by_cached_key(Value::to_string);
        json!({"schema":"fr-call-graph-1", "functions":self.node_count(), "calls":self.edge_count(),
            "hierarchy_edges":self.hierarchy_edge_count(), "unresolved_calls":unresolved.len(),
            "file_scope_calls":file_scope.len(), "by_confidence":self.confidence_breakdown(),
            "by_origin":self.origin_breakdown(), "nodes":nodes, "edges":edges, "source_digests":source_digests,
            "unresolved":unresolved, "file_scope":file_scope,
            "coverage":{"scope":"indexed workspace", "gaps":gaps,
                "all_visible_calls_resolved":gaps.is_empty() && unresolved.is_empty(),
                "runtime_dispatch_established":false,
                "external_callers_known":false}})
    }
}
