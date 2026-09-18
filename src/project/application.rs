use super::{FeatureOptions, Project, RelationshipOptions};
use crate::application_ir::{Adapter, ApplicationIr, ApplicationNode, FeatureKind, SCHEMA};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

mod normalize;

#[derive(Args)]
pub struct Options {
    #[arg(
        default_value = ".",
        help = "Workspace path or revision-bound project handle."
    )]
    pub target: String,
    #[arg(long)]
    pub revision: Option<String>,
    #[arg(
        long,
        help = "Select one revision-bound feature before constructing its hierarchy."
    )]
    pub feature: Option<String>,
}

fn hierarchy(rows: Vec<Value>) -> Result<Vec<ApplicationNode>> {
    let input_count = rows.len();
    let mut nodes = BTreeMap::new();
    let mut children: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for row in rows {
        let mut data = row
            .as_object()
            .context("application fact must be an object")?
            .clone();
        let id = data
            .remove("id")
            .and_then(|value| value.as_str().map(str::to_owned))
            .context("application fact omitted its identity")?;
        let parent = data
            .remove("parent")
            .and_then(|value| value.as_str().map(str::to_owned));
        let kind = data
            .remove("kind")
            .and_then(|value| value.as_str().map(str::to_owned))
            .context("application fact omitted its kind")?;
        let source = data.remove("source").unwrap_or(Value::Null);
        let boundary = if matches!(kind.as_str(), "route" | "handler" | "component") {
            Some("Source evidence retains its confidence. Executable response and rendering semantics require normalization.".into())
        } else {
            None
        };
        let node = ApplicationNode {
            id: id.clone(),
            kind,
            source,
            data: data.into_iter().collect(),
            children: Vec::new(),
            route: None,
            component: None,
            boundary,
        };
        ensure!(
            nodes.insert(id.clone(), node).is_none(),
            "application facts repeat an identity"
        );
        children.entry(parent).or_default().push(id);
    }
    for parent in children.keys().flatten() {
        ensure!(nodes.contains_key(parent), "application hierarchy contains an absent parent; narrow the scope or inspect the feature reader gap.");
    }
    fn take(
        id: &str,
        nodes: &mut BTreeMap<String, ApplicationNode>,
        children: &BTreeMap<Option<String>, Vec<String>>,
        depth: usize,
    ) -> Result<ApplicationNode> {
        ensure!(
            depth <= 64,
            "application hierarchy exceeds its depth bound."
        );
        let mut node = nodes
            .remove(id)
            .context("application hierarchy repeats a child or contains a cycle.")?;
        if let Some(ids) = children.get(&Some(id.to_owned())) {
            for id in ids {
                node.children.push(take(id, nodes, children, depth + 1)?);
            }
        }
        Ok(node)
    }
    let expected: BTreeSet<_> = nodes.keys().cloned().collect();
    let mut roots = Vec::new();
    for id in children.get(&None).into_iter().flatten() {
        roots.push(take(id, &mut nodes, &children, 0)?);
    }
    ensure!(
        nodes.is_empty(),
        "application hierarchy contains a parent cycle."
    );
    fn identities(node: &ApplicationNode, ids: &mut BTreeSet<String>) -> bool {
        ids.insert(node.id.clone()) && node.children.iter().all(|child| identities(child, ids))
    }
    let mut assigned = BTreeSet::new();
    let unique = roots.iter().all(|node| identities(node, &mut assigned));
    ensure!(
        crate::framework_kernel::application_dispositions_complete(
            input_count,
            assigned.len(),
            unique,
            assigned == expected
        ),
        "application hierarchy failed exact unique fact coverage."
    );
    Ok(roots)
}

pub fn adapter_contracts() -> Value {
    json!(Adapter::ALL
        .into_iter()
        .map(|source| {
            let reader_features = FeatureKind::ALL
                .into_iter()
                .filter(|feature| {
                    crate::framework_kernel::application_adapter_reads(
                        source.code(),
                        feature.code(),
                    )
                })
                .collect::<Vec<_>>();
            let targets = Adapter::ALL
                .into_iter()
                .map(|target| {
                    let features = FeatureKind::ALL
                        .into_iter()
                        .map(|feature| {
                            let source_support =
                                crate::framework_kernel::application_adapter_reads(
                                    source.code(),
                                    feature.code(),
                                );
                            let target_support =
                                crate::framework_kernel::application_adapter_writes(
                                    target.code(),
                                    feature.code(),
                                );
                            let admitted =
                                crate::framework_kernel::application_adapters_compatible(
                                    source.code(),
                                    target.code(),
                                    feature.code(),
                                );
                            let reason = if admitted {
                                Value::Null
                            } else if source == target {
                                json!("source-and-target-adapters-are-identical")
                            } else if !source_support {
                                json!(format!(
                                    "source-reader-does-not-model-{}",
                                    feature.name()
                                ))
                            } else if !target_support {
                                json!(format!(
                                    "target-writer-does-not-model-{}",
                                    feature.name()
                                ))
                            } else {
                                json!("adapter-compatibility-policy-refused")
                            };
                            json!({"feature": feature, "status": if admitted { "supported" } else { "unsupported" },
                                "reason": reason, "runtime_proved": false})
                        })
                        .collect::<Vec<_>>();
                    json!({"target": target, "features": features})
                })
                .collect::<Vec<_>>();
            let mut excluded = vec!["middleware", "authentication", "service calls", "dynamic rendering", "implicit HTTP methods", "runtime configuration"];
            if source != Adapter::Fastapi {
                excluded.insert(0, "source normalization of request bodies and query validation");
            }
            json!({"source": source, "reader_features": reader_features, "targets": targets,
                "excluded": excluded})
        })
        .collect::<Vec<_>>())
}

impl Project<'_> {
    pub(super) fn application_model(
        &self,
        target: &str,
        revision: Option<&str>,
        feature: Option<&str>,
    ) -> Result<ApplicationIr> {
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        let mut analysis = Value::Null;
        loop {
            let report = self.features(&FeatureOptions {
                selection: RelationshipOptions {
                    target: target.into(),
                    revision: revision.map(str::to_owned),
                    limit: 500,
                    cursor: cursor.clone(),
                },
                feature: feature.map(str::to_owned),
            })?;
            rows.extend(
                report["items"]
                    .as_array()
                    .context("feature report omitted its facts")?
                    .iter()
                    .cloned(),
            );
            ensure!(
                rows.len() <= 4096,
                "application IR exceeds 4096 facts; narrow TARGET or select a feature."
            );
            if analysis.is_null() {
                analysis = report["analysis"].clone();
            }
            cursor = report["page"]["next"].as_str().map(str::to_owned);
            if cursor.is_none() {
                ensure!(
                    report["page"]["remaining"].as_u64().unwrap_or(0) == 0,
                    "feature pagination omitted a continuation."
                );
                break;
            }
            ensure!(
                seen.insert(cursor.clone()),
                "feature pagination repeated a continuation."
            );
        }
        let mut applications = hierarchy(rows)?;
        normalize::routes(self, &mut applications)?;
        let ir = ApplicationIr {
            schema: SCHEMA.into(),
            revision: self.revision.clone(),
            applications,
            omissions: analysis,
            runtime_proved: false,
        };
        ir.validate().map_err(anyhow::Error::msg)?;
        Ok(ir)
    }

    pub(super) fn application(&self, options: &Options) -> Result<Value> {
        let ir = self.application_model(
            &options.target,
            options.revision.as_deref(),
            options.feature.as_deref(),
        )?;
        let model = serde_json::to_value(&ir)?;
        let mut report = self.envelope("application");
        report.as_object_mut().unwrap().extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"schema": "fr-application-report-1",
            "model": model, "object_digest": super::object_merkle(&model)?,
            "adapters": adapter_contracts(), "runtime_proved": false,
            "instructions": "Parent identities become children in the application IR. All source confidence, gaps and bounded reader omissions remain in data. Route and component boundaries require semantic normalization before conversion. Use project disclose --view application for bounded Merkle access."}))?);
        Ok(report)
    }
}
