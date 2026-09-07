use super::bounded;
use crate::analysis::call_graph::{CallGraph, Family, Hierarchy};
use crate::capabilities::{support, Capability};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{FileFacts, SymbolId};
use crate::parse::Parsers;
use crate::project::CallDirection;
use crate::span::LineIndex;
use anyhow::{anyhow, ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn collect(
    path: &Path,
    side: &str,
    source: &str,
    language: Language,
    facts: &FileFacts,
    selected: &BTreeSet<SymbolId>,
    direction: CallDirection,
) -> Result<(Vec<Value>, Value)> {
    if !support(Capability::CallGraph, language).is_yes() {
        return Ok((
            Vec::new(),
            json!({"status":"unsupported-language", "scope":"single-file-snapshot"}),
        ));
    }
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let memory = crate::vfs::new_handle([(path.to_path_buf(), source.to_owned())]);
            crate::vfs::activate(&memory);
            let index = Index::build_from_facts(&[(path.to_path_buf(), language, facts.clone())]);
            let hierarchy = Hierarchy::from_source(
                &Parsers::new(), path, language, &facts.imports, source,
            );
            let graph = CallGraph::build_with(&index, &hierarchy);
            let lines = LineIndex::new(source);
            let selected_spans = selected
                .iter()
                .filter_map(|id| index.symbol(*id))
                .map(|s| s.full_span)
                .collect::<Vec<_>>();
            let symbol_in_scope = |id| {
                index.symbol(id).is_some_and(|symbol| {
                    selected_spans.iter().any(|s| s.contains(symbol.full_span))
                })
            };
            let site_in_scope = |offset| selected_spans.iter().any(|s| s.contains_offset(offset));
            let endpoint = |id| -> Result<Value> {
                let symbol = index
                    .symbol(id)
                    .context("call endpoint lacks a snapshot declaration")?;
                ensure!(
                    symbol.file == path
                        && source.get(symbol.name_span.start..symbol.name_span.end).is_some(),
                    "call endpoint is outside its snapshot."
                );
                Ok(json!({"id":format!("{side}:{}",symbol.id.0),"name":bounded(&symbol.name),"kind":symbol.kind,
                    "qualifier":symbol.qualifier.as_deref().map(bounded),"line":lines.line_col(symbol.name_span.start,source).line,
                    "changed_declaration":selected.contains(&id),"in_selection":symbol_in_scope(id)}))
            };
            let site = |file: &Path, offset: usize| -> Result<Value> {
                ensure!(
                    file == path && offset < source.len() && source.is_char_boundary(offset),
                    "call site is outside its snapshot."
                );
                let position = lines.line_col(offset, source);
                Ok(json!({"offset":offset,"line":position.line,"column":position.col}))
            };
            let relation = |incoming, outgoing| {
                let include = crate::git::call_in_selection(
                    incoming,
                    outgoing,
                    !matches!(direction, CallDirection::Outgoing),
                    !matches!(direction, CallDirection::Incoming),
                );
                include.then_some(if incoming && outgoing {
                    "internal"
                } else if incoming {
                    "incoming"
                } else {
                    "outgoing"
                })
            };
            let mut rows = Vec::new();
            for (caller, callee, edge) in graph.edges() {
                let Some(relation) = relation(symbol_in_scope(callee), site_in_scope(edge.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":endpoint(caller)?,"callee":endpoint(callee)?,
                    "site":site(&edge.file,edge.offset)?,"confidence":edge.confidence,"origin":edge.origin.as_str(),
                    "dispatch_candidate":edge.origin.is_dispatch(),"status":if edge.origin.is_dispatch() {"dispatch-candidate"} else {"indexed-target"}}));
            }
            for call in &graph.file_scope {
                let Some(relation) = relation(symbol_in_scope(call.callee), site_in_scope(call.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":null,"caller_scope":"file","callee":endpoint(call.callee)?,
                    "site":site(&call.file,call.offset)?,"confidence":call.confidence,"origin":"resolved","dispatch_candidate":false,"status":"indexed-target"}));
            }
            for call in &graph.unresolved {
                let Some(relation) = relation(false, site_in_scope(call.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":call.caller.map(endpoint).transpose()?,"callee":null,
                    "name":bounded(&call.callee_name),"site":site(&call.file,call.offset)?,"confidence":call.confidence,"origin":"unresolved","dispatch_candidate":false,"status":"unresolved"}));
            }
            rows.sort_by_cached_key(Value::to_string);
            let gaps = graph.hierarchy_gaps.iter()
                .map(|(_, reason)| bounded(reason))
                .collect::<Vec<_>>();
            let analysis = json!({"status":if facts.gaps.is_empty() && gaps.is_empty() {"analyzed"} else {"partial"},
                "scope":"single-file-snapshot","cross_file":"not-collected","hierarchy_supported":Family::of(language).is_some(),"hierarchy_gaps":gaps,
                "callable_nodes":graph.node_count(),"edges":graph.edge_count(),"file_scope_calls":graph.file_scope.len(),"unresolved_calls":graph.unresolved.len(),
                "selected_rows":rows.len()});
            Ok((rows,analysis))
        });
        worker
            .join()
            .map_err(|_| anyhow!("snapshot call analysis failed"))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Extractor;

    #[test]
    fn snapshot_calls_preserve_the_callers_active_workspace() {
        let path = Path::new("/snapshot/code.py");
        let memory = crate::vfs::new_handle([(path.to_path_buf(), "caller workspace".to_owned())]);
        crate::vfs::activate(&memory);
        let source = "def leaf():\n    pass\ndef changed():\n    leaf()\n";
        let parsed = Parsers::new().parse(Language::Python, source).unwrap();
        let facts = Extractor::new().extract(&parsed, path, source).unwrap();
        let selected = facts
            .symbols
            .iter()
            .filter(|s| s.name == "changed")
            .map(|s| s.id)
            .collect();
        let (rows, _) = collect(
            path,
            "before",
            source,
            Language::Python,
            &facts,
            &selected,
            CallDirection::Both,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["callee"]["name"]["text"], "leaf");
        assert!(crate::vfs::is_in_memory());
        assert_eq!(
            crate::vfs::read_to_string(path).unwrap(),
            "caller workspace"
        );
        crate::vfs::use_filesystem();
    }
}
