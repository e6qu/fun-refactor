use super::{
    occurrence::{Occurrence, SourceOrigins},
    semantic,
    semantic_origins::OriginOptions,
    Project,
};
use crate::model::SymbolKind;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) struct Linker<'a, 'p> {
    project: &'a Project<'p>,
    reports: BTreeMap<String, std::result::Result<Value, String>>,
}

impl<'a, 'p> Linker<'a, 'p> {
    pub fn new(project: &'a Project<'p>) -> Self {
        Self {
            project,
            reports: BTreeMap::new(),
        }
    }

    pub fn queries(&self) -> usize {
        self.reports.len()
    }

    pub fn explain(&mut self, point: &Occurrence) -> Result<Value> {
        let file = self.project.root.join(&point.path);
        let symbol = self
            .project
            .index
            .file(&file)
            .into_iter()
            .flat_map(|info| &info.symbols)
            .filter_map(|id| self.project.index.symbol(*id))
            .filter(|symbol| {
                matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method)
                    && symbol.full_span.contains(point.location.span)
            })
            .min_by_key(|symbol| (symbol.full_span.len(), symbol.id));
        let Some((symbol, target)) = symbol.and_then(|symbol| {
            self.project
                .symbol_nodes
                .get(&symbol.id)
                .map(|node| (symbol, self.project.handle(*node)))
        }) else {
            return Ok(
                json!({"occurrence":point,"mapping":{"status":"unavailable","complete":false,
                "reason":"No enclosing indexed function is available.","items":[]},"source":Value::Null}),
            );
        };
        let arguments = vec![
            "project".to_string(),
            "semantic".into(),
            target.clone(),
            "--body".into(),
            "--origins".into(),
            "--nodes".into(),
            "4096".into(),
            "--origin-limit".into(),
            "256".into(),
        ];
        let source = json!({"arguments":["project","show",target,"--source","--bytes","256",
            "--offset",(point.location.span.start-symbol.full_span.start).to_string()]});
        let mut mapping = json!({"status":"incomplete","complete":false,"items":[],
            "reason":"The per-page semantic query budget was exhausted.","follow":{"arguments":arguments}});
        if !self.reports.contains_key(&target) && self.reports.len() < 8 {
            let report = self
                .project
                .semantic(&semantic::Options {
                    provenance: OriginOptions {
                        origins: true,
                        origin_limit: 256,
                        ..Default::default()
                    },
                    target: target.clone(),
                    revision: None,
                    declaration: None,
                    body: true,
                    pointers: false,
                    locators: false,
                    locators_only: false,
                    locator_op: None,
                    locator_from: None,
                    intent_to: None,
                    nodes: 4096,
                    unsupported_source: false,
                    minimal: true,
                })
                .map_err(|error| error.to_string().chars().take(256).collect::<String>());
            self.reports.insert(target.clone(), report);
        }
        if let Some(stored) = self.reports.get(&target) {
            match stored {
                Err(reason) => {
                    mapping["status"] = json!("unavailable");
                    mapping["reason"] = json!(reason);
                }
                Ok(report) => {
                    let mut matches = Vec::new();
                    for row in report["origins"]["items"].as_array().into_iter().flatten() {
                        let origins: SourceOrigins =
                            serde_json::from_value(row["origins"].clone())?;
                        let (points, relation) = match origins {
                            SourceOrigins::Exact { occurrence } => (vec![occurrence], "exact"),
                            SourceOrigins::Multiple { occurrences } => {
                                (occurrences, "member-of-multiple")
                            }
                            SourceOrigins::Absent { .. } => (vec![], "absent"),
                        };
                        if points.iter().any(|origin| {
                            origin.revision == point.revision
                                && origin.path == point.path
                                && origin.location.span == point.location.span
                        }) {
                            let mut action = arguments.clone();
                            action.extend([
                                "--origin-pointer".into(),
                                row["pointer"].as_str().unwrap().into(),
                            ]);
                            matches.push(json!({"id":row["id"],"pointer":row["pointer"],"kind":row["kind"],
                                "body_pointer":row["body_pointer"],"body_basis":report["body_identity"]["basis"],
                                "semantic_basis":report["semantic_basis"],"rule":row["rule"],"relation":relation,
                                "follow":{"arguments":action}}));
                        }
                    }
                    let complete = report["origins"]["complete"] == true && matches.len() <= 4;
                    let omitted = matches.len().saturating_sub(4);
                    matches.truncate(4);
                    mapping = json!({"status":if !matches.is_empty() {"mapped"} else if complete {"absent"} else {"incomplete"},
                        "complete":complete,"items":matches,"omitted_matches":omitted,
                        "reason":if complete {"Exact-span origin lookup; missing origins can be synthesized or transformed nodes."} else {"The lookup inspected one bounded semantic origin page."},
                        "semantic_basis":report["semantic_basis"],"input_digest":report["origins"]["input_digest"],
                        "analyzer":report["origins"]["analyzer"],"origin_page":report["origins"]["page"],
                        "follow":{"arguments":arguments},"continuation":report["origins"]["continuation"],
                        "claim":"syntax origin relation; no source implementation correspondence proof"});
                }
            }
        }
        Ok(json!({"occurrence":point,"mapping":mapping,"source":source}))
    }
}
