use super::{bounded, context::Snapshot};
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
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) struct Focus<'a> {
    pub path: &'a Path,
    pub side: &'a str,
    pub source: &'a str,
    pub language: Language,
    pub facts: &'a FileFacts,
    pub selected: &'a BTreeSet<SymbolId>,
}

pub(super) fn collect(
    focus: Focus<'_>,
    direction: CallDirection,
    context: Option<&[Snapshot]>,
) -> Result<(Vec<Value>, Value)> {
    let Focus {
        path,
        side,
        source,
        language,
        facts,
        selected,
    } = focus;
    let analysis_scope = if context.is_some() {
        "selected-file-snapshot"
    } else {
        "single-file-snapshot"
    };
    if !support(Capability::CallGraph, language).is_yes() {
        return Ok((
            Vec::new(),
            json!({"status":"unsupported-language", "scope":analysis_scope}),
        ));
    }
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut files = vec![(path.to_path_buf(), language, facts.clone())];
            let mut sources = BTreeMap::from([(path, (source, LineIndex::new(source)))]);
            let parsers = Parsers::new();
            let mut hierarchy = Hierarchy::from_source(&parsers, path, language, &facts.imports, source);
            for snapshot in context.unwrap_or_default() {
                files.push((snapshot.path.clone(), snapshot.language, snapshot.facts.clone()));
                sources.insert(snapshot.path.as_path(), (snapshot.source.as_str(), LineIndex::new(&snapshot.source)));
                hierarchy.merge(Hierarchy::from_source(&parsers, &snapshot.path, snapshot.language, &snapshot.facts.imports, &snapshot.source));
            }
            hierarchy.gaps.sort();
            hierarchy.gaps.dedup();
            let memory = crate::vfs::new_handle(sources.iter().map(|(path, (source, _))| (path.to_path_buf(), (*source).to_owned())));
            crate::vfs::activate(&memory);
            let index = Index::build_from_facts(&files);
            let graph = CallGraph::build_with(&index, &hierarchy);
            let selected_spans = selected
                .iter()
                .filter_map(|id| index.symbol(*id))
                .map(|s| s.full_span)
                .collect::<Vec<_>>();
            let symbol_in_scope = |id| {
                index.symbol(id).is_some_and(|symbol| {
                    symbol.file == path && selected_spans.iter().any(|s| s.contains(symbol.full_span))
                })
            };
            let site_in_scope = |file: &Path, offset| file == path && selected_spans.iter().any(|s| s.contains_offset(offset));
            let endpoint = |id| -> Result<Value> {
                let symbol = index
                    .symbol(id)
                    .context("call endpoint lacks a snapshot declaration")?;
                let (source, lines) = sources.get(symbol.file.as_path()).context("call endpoint lacks a captured file")?;
                ensure!(
                    source.get(symbol.name_span.start..symbol.name_span.end).is_some(),
                    "call endpoint is outside its snapshot."
                );
                Ok(json!({"id":format!("{side}:{}",symbol.id.0),"path":symbol.file,"name":bounded(&symbol.name),"kind":symbol.kind,
                    "qualifier":symbol.qualifier.as_deref().map(bounded),"line":lines.line_col(symbol.name_span.start,source).line,
                    "changed_declaration":selected.contains(&id),"in_selection":symbol_in_scope(id)}))
            };
            let site = |file: &Path, offset: usize| -> Result<Value> {
                let (source, lines) = sources.get(file).context("call site lacks a captured file")?;
                ensure!(
                    offset < source.len() && source.is_char_boundary(offset),
                    "call site is outside its snapshot."
                );
                let position = lines.line_col(offset, source);
                Ok(json!({"path":file,"offset":offset,"line":position.line,"column":position.col}))
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
                let Some(relation) = relation(symbol_in_scope(callee), site_in_scope(&edge.file, edge.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":endpoint(caller)?,"callee":endpoint(callee)?,
                    "site":site(&edge.file,edge.offset)?,"confidence":edge.confidence,"origin":edge.origin.as_str(),
                    "dispatch_candidate":edge.origin.is_dispatch(),"status":if edge.origin.is_dispatch() {"dispatch-candidate"} else {"indexed-target"}}));
            }
            for call in &graph.file_scope {
                let Some(relation) = relation(symbol_in_scope(call.callee), site_in_scope(&call.file, call.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":null,"caller_scope":"file","callee":endpoint(call.callee)?,
                    "site":site(&call.file,call.offset)?,"confidence":call.confidence,"origin":"resolved","dispatch_candidate":false,"status":"indexed-target"}));
            }
            for call in &graph.unresolved {
                let Some(relation) = relation(false, site_in_scope(&call.file, call.offset)) else {
                    continue;
                };
                rows.push(json!({"kind":"call","side":side,"scope_relation":relation,"caller":call.caller.map(endpoint).transpose()?,"callee":null,
                    "name":bounded(&call.callee_name),"site":site(&call.file,call.offset)?,"confidence":call.confidence,"origin":"unresolved","dispatch_candidate":false,"status":"unresolved"}));
            }
            rows.sort_by_cached_key(Value::to_string);
            let gaps = graph.hierarchy_gaps.iter()
                .map(|(path, reason)| if context.is_some() {json!({"path":path,"reason":bounded(reason)})} else {bounded(reason)})
                .collect::<Vec<_>>();
            let analysis = json!({"status":if files.iter().all(|(_,_,facts)|facts.gaps.is_empty()) && gaps.is_empty() {"analyzed"} else {"partial"},
                "scope":analysis_scope,"cross_file":if context.is_some() {"explicit-files-only"} else {"not-collected"},"hierarchy_supported":Family::of(language).is_some(),"hierarchy_gaps":gaps,
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
        let caller_path = Path::new("/snapshot/caller.py");
        let caller_source = "from code import changed\nchanged()\n";
        let parsed = Parsers::new()
            .parse(Language::Python, caller_source)
            .unwrap();
        let caller_facts = Extractor::new()
            .extract(&parsed, caller_path, caller_source)
            .unwrap();
        let context = [Snapshot {
            path: caller_path.to_path_buf(),
            language: Language::Python,
            source: caller_source.to_owned(),
            facts: caller_facts,
        }];
        for (context, count) in [(None, 1), (Some(context.as_slice()), 2)] {
            let (rows, _) = collect(
                Focus {
                    path,
                    side: "before",
                    source,
                    language: Language::Python,
                    facts: &facts,
                    selected: &selected,
                },
                CallDirection::Both,
                context,
            )
            .unwrap();
            assert_eq!(rows.len(), count);
            assert!(rows
                .iter()
                .any(|row| row["callee"]["name"]["text"] == "leaf"));
            assert!(crate::vfs::is_in_memory());
            assert_eq!(
                crate::vfs::read_to_string(path).unwrap(),
                "caller workspace"
            );
            assert!(crate::vfs::read_to_string(caller_path).is_err());
        }
        crate::vfs::use_filesystem();
    }
}
