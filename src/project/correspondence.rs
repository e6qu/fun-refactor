//! Content identity and cross-revision candidates never renew action handles.
use super::{hash, page, Project};
use anyhow::{ensure, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Args)]
pub struct Options {
    #[arg(default_value = ".")]
    target: String,
    /// Previously retained identity report. Compare candidates without rebinding actions.
    #[arg(long)]
    from: Option<PathBuf>,
    #[arg(long, default_value_t = 40)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub handle: String,
    pub revision: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub object_digest: String,
}

impl Project<'_> {
    pub(super) fn identities(&self, options: &Options) -> Result<Value> {
        let selected = self.target(&options.target)?;
        let mut identities = Vec::new();
        for (id, node) in self.nodes.iter().enumerate() {
            if !self.within(id, selected) {
                continue;
            }
            let Some(symbol) = node.symbol.and_then(|id| self.index.symbol(id)) else {
                continue;
            };
            let (source, span) = self.source(id)?;
            identities.push(Identity {
                handle: self.handle(id),
                revision: self.revision.clone(),
                path: node.path.to_string_lossy().into_owned(),
                name: symbol.name.clone(),
                kind: symbol.kind.as_str().into(),
                object_digest: hash((
                    "fr-declaration-object-1",
                    symbol.language,
                    symbol.kind,
                    span.text(source),
                ))?,
            });
        }
        identities
            .sort_by(|a, b| (&a.path, &a.name, &a.handle).cmp(&(&b.path, &b.name, &b.handle)));
        let rows = if let Some(path) = &options.from {
            ensure!(
                std::fs::metadata(path)?.len() <= 1_048_576,
                "identity report exceeds 1 MiB"
            );
            let previous: Value = serde_json::from_slice(&std::fs::read(path)?)?;
            ensure!(
                previous["query"] == "identities",
                "expected identity report"
            );
            let old: Vec<Identity> = serde_json::from_value(previous["items"].clone())?;
            ensure!(old.len() <= 1000, "too many correspondence inputs");
            old.into_iter().map(|old| {
                let mut candidates: Vec<_> = identities.iter().filter(|new| new.object_digest == old.object_digest).collect();
                let basis = if candidates.is_empty() {
                    candidates = identities.iter().filter(|new| new.path == old.path && new.name == old.name && new.kind == old.kind).collect();
                    "same-path-name-kind"
                } else { "identical-declaration-content" };
                json!({"previous": old, "status": match candidates.len() { 0 => "missing", 1 => "matched", _ => "ambiguous" },
                    "candidates": candidates, "basis": basis, "action_rebound": false})
            }).collect::<Vec<_>>()
        } else {
            identities.iter().map(|item| json!(item)).collect()
        };
        let key = format!(
            "frpc1:{}",
            &hash((&self.revision, "identities", &rows))?[..32]
        );
        let (start, end, paging) =
            page(rows.len(), options.limit, options.cursor.as_deref(), &key)?;
        let mut report = self.envelope(if options.from.is_some() {
            "correspondence"
        } else {
            "identities"
        });
        report["items"] = json!(&rows[start..end]);
        report["page"] = paging;
        report["claim"] = json!(
            "syntactic correspondence candidates; no semantic equivalence or mutation authority"
        );
        report["graph_policy"] = json!("declaration objects exclude graph edges; shared or cyclic references use revision-bound handles");
        Ok(report)
    }
}
