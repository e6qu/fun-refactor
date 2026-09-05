use super::{bounded_text, hash, Project, RelationshipOptions};
use crate::analysis::stitch;
use crate::lang::Language;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

impl Project<'_> {
    fn configuration_location(&self, file: &Path, line: Option<usize>) -> Result<Value> {
        let relative = file.strip_prefix(&self.root)?;
        let node = self
            .nodes
            .iter()
            .position(|node| node.kind == "file" && node.path == relative)
            .context("configuration location has no project file node")?;
        Ok(
            json!({"path": bounded_text(&relative.to_string_lossy(), 512),
            "line": line, "file_handle": self.handle(node)}),
        )
    }

    pub(super) fn configuration(&self, options: &RelationshipOptions) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        ensure!(self.nodes[selected].symbol.is_none(),
            "Configuration requires a file or directory. Inspect locations through their file handles.");
        let facts = stitch::analyze_snapshot(self.index, &self.sources)?;
        let mut rows = Vec::new();
        let mut declared = BTreeSet::new();
        let mut selected_declarations = 0usize;
        let mut selected_consumers = 0usize;
        let mut unmatched = 0usize;
        for (ordinal, chain) in facts.chains.iter().enumerate() {
            declared.insert(&chain.env_var);
            let origin_selected = self.scope_file(selected, &chain.declared_in)
                || chain
                    .values_file
                    .as_ref()
                    .is_some_and(|file| self.scope_file(selected, file));
            let consumers: Vec<_> = chain
                .reads
                .iter()
                .filter(|read| origin_selected || self.scope_file(selected, &read.file))
                .collect();
            if !origin_selected && consumers.is_empty() {
                continue;
            }
            selected_declarations += 1;
            selected_consumers += consumers.len();
            let id = format!("frpcfg1:{}", &hash((&self.revision, ordinal))?[..32]);
            let values = chain
                .values_path
                .as_ref()
                .map(|parts| bounded_text(&parts.join("."), 512));
            rows.push(json!({"kind": "config-declaration", "id": id,
                "name": bounded_text(&chain.env_var, 160),
                "site": self.configuration_location(&chain.declared_in, Some(chain.declared_line))?,
                "status": "candidate", "basis": "manifest-pattern", "confidence": "name-only",
                "consumer_count": chain.reads.len(), "selected_consumers": consumers.len(),
                "consumer_status": if chain.reads.is_empty() { "no-observed-consumer" } else { "candidates" },
                "conditional_on": chain.conditional_on.as_deref().map(|s| bounded_text(s, 512)),
                "values_path": values, "values_path_components": chain.values_path.as_ref().map(Vec::len),
                "values_file_candidate": chain.values_file.as_ref().map(|file| self.configuration_location(file, None)).transpose()?,
                "values_file_basis": "nearest-ancestor-leaf-name"}));
            for read in consumers {
                rows.push(json!({"kind": "config-consumer", "declaration": id,
                    "name": bounded_text(&chain.env_var, 160), "site": self.configuration_location(&read.file, Some(read.line))?,
                    "language": read.language.name(), "confidence": read.confidence,
                    "status": "candidate", "basis": "environment-accessor-text"}));
            }
        }
        for read in &facts.reads {
            if declared.contains(&read.name) || !self.scope_file(selected, &read.read.file) {
                continue;
            }
            unmatched += 1;
            rows.push(json!({"kind": "config-consumer", "declaration": null,
                "name": bounded_text(&read.name, 160), "site": self.configuration_location(&read.read.file, Some(read.read.line))?,
                "language": read.read.language.name(), "confidence": read.read.confidence,
                "status": "no-observed-declaration", "basis": "environment-accessor-text"}));
        }
        for (file, reason) in &facts.gaps {
            rows.push(json!({"kind": "analysis-gap", "path": bounded_text(&file.strip_prefix(&self.root)?.to_string_lossy(), 512),
                "reason": bounded_text(reason, 512), "basis": "configuration-analysis"}));
        }
        let mut unsupported = BTreeMap::new();
        for (_, info) in self.index.files() {
            if !matches!(info.language, Language::Yaml | Language::Helm)
                && !stitch::reads_environment(info.language)
            {
                *unsupported.entry(info.language.name()).or_insert(0usize) += 1;
            }
        }
        for (language, files) in &unsupported {
            rows.push(
                json!({"kind": "coverage-gap", "language": language, "files": files,
                "reason": "no environment declaration or accessor reader for this language."}),
            );
        }
        let analysis = json!({"scope": "indexed workspace", "declarations": facts.chains.len(),
            "read_candidates": facts.reads.len(), "gaps": facts.gaps.len(), "unsupported_files": unsupported,
            "selected_declarations": selected_declarations, "selected_consumers": selected_consumers,
            "selected_reads_without_declarations": unmatched,
            "certainty": "Name matches are candidates; deployment identity, precedence and runtime use remain unchecked.",
            "limitations": "YAML/Helm manifest patterns and uppercase accessor text only. Comments, strings and overlapping accessors can produce extra candidates. Dynamic and lowercase names can remain unseen.",
            "values_lookup": "A nearest ancestor values file containing the leaf key is a candidate. Full paths, overlays and deployment inputs remain unchecked."});
        let mut result =
            self.relationship_page("configuration", selected, options, None, rows, analysis)?;
        result["scope"] = json!("Declarations or values-file candidates in selected files include all consumers. Selected code reads include their declarations. Workspace diagnostics share the page.");
        Ok(result)
    }
}
