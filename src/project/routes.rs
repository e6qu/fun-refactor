use super::{bounded_text, hash, Project, RelationshipOptions};
use crate::lang::Language;
use crate::parse::Parsers;
use crate::transpile::routes;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

impl Project<'_> {
    pub(super) fn routes(&self, options: &RelationshipOptions, contracts: bool) -> Result<Value> {
        let selected = self.relationship_selection(options)?;
        ensure!(
            self.nodes[selected].symbol.is_none(),
            "Route inspection requires a file or directory. Inspect handler candidate handles with project show."
        );
        let mut rows = Vec::new();
        let mut unsupported = BTreeMap::new();
        let mut analyzed = 0usize;
        let mut empty = 0usize;
        let mut syntax_gaps = 0usize;
        let mut declarations = 0usize;
        let mut handlers = 0usize;
        let mut contract_fields = 0usize;
        let mut contract_gaps = 0usize;
        let parsers = Parsers::new();
        for (file, info) in self.index.files() {
            if !self.scope_file(selected, file) {
                continue;
            }
            if !matches!(
                info.language,
                Language::TypeScript
                    | Language::Tsx
                    | Language::Python
                    | Language::Rust
                    | Language::Go
                    | Language::Java
            ) {
                *unsupported.entry(info.language.name()).or_insert(0usize) += 1;
                continue;
            }
            let source = &self.sources[file];
            let relative = file.strip_prefix(&self.root)?;
            let path = bounded_text(&relative.to_string_lossy(), 512);
            let parsed = parsers.parse(info.language, source)?;
            if parsed.has_errors() {
                syntax_gaps += 1;
                rows.push(json!({"kind": "analysis-gap", "path": path,
                    "basis": "route-pattern-reader", "reason": "source has syntax errors; the reader skipped this file."}));
                continue;
            }
            analyzed += 1;
            let Some((framework, endpoints)) = routes::endpoints_of(source, info.language) else {
                empty += 1;
                continue;
            };
            let file_node = self
                .nodes
                .iter()
                .position(|node| node.kind == "file" && node.path == relative)
                .context("route declaration has no project file node")?;
            for (ordinal, endpoint) in endpoints.iter().enumerate() {
                declarations += 1;
                let id = format!(
                    "frpr1:{}",
                    &hash((
                        &self.revision,
                        relative,
                        ordinal,
                        &endpoint.method,
                        &endpoint.url
                    ))?[..32]
                );
                let candidates: Vec<_> = endpoint
                    .handler
                    .as_deref()
                    .map(|name| {
                        self.index
                            .find_symbols(name, None)
                            .into_iter()
                            .filter(|symbol| symbol.file == *file && symbol.kind.is_callable())
                            .collect()
                    })
                    .unwrap_or_default();
                handlers += candidates.len();
                rows.push(json!({"kind": "route", "id": id, "path": path,
                    "file_handle": self.handle(file_node), "line": endpoint.line,
                    "method": endpoint.method, "url": bounded_text(&endpoint.url, 512),
                    "framework_candidate": framework.to_string(), "basis": "route-pattern-reader", "status": "candidate", "confidence": null,
                    "handler": {"name": endpoint.handler.as_deref().map(|name| bounded_text(name, 160)),
                        "candidate_count": candidates.len(), "basis": "same-file-name",
                        "status": if endpoint.handler.is_none() { "unnamed" } else if candidates.is_empty() { "unresolved" } else if candidates.len() == 1 { "candidate" } else { "ambiguous" }}}));
                if contracts {
                    let details =
                        self.contract_rows(&id, endpoint, &candidates, &parsed, source)?;
                    contract_fields += details
                        .iter()
                        .filter(|r| r["kind"] == "route-contract-field")
                        .count();
                    contract_gaps += details
                        .iter()
                        .filter(|r| r["kind"] == "route-contract-gap")
                        .count();
                    rows.extend(details);
                }
                for candidate in candidates {
                    rows.push(json!({"kind": "route-handler", "route": id,
                        "handler": self.endpoint(candidate.id)?, "status": "candidate",
                        "basis": "same-file-name", "confidence": "name-only"}));
                }
            }
        }
        for (language, files) in &unsupported {
            rows.push(
                json!({"kind": "coverage-gap", "language": language, "files": files,
                "reason": "no route pattern reader for this language."}),
            );
        }
        let mut analysis = json!({"scope": "selected files", "analyzed_files": analyzed,
            "files_without_patterns": empty, "syntax_gaps": syntax_gaps,
            "unsupported_files": unsupported, "declarations": declarations, "handler_candidates": handlers,
            "readers": ["express", "flask", "axum", "gin", "spring"],
            "certainty": "Declaration patterns and local handler-name candidates; the reader does not verify framework identity or runtime reachability.",
            "limitations": "No Next.js or FastAPI-specific reader, request/response schemas, middleware, mounted-router prefixes or cross-file handler resolution. Empty results do not prove absence of routes."});
        if contracts {
            analysis["contract_fields"] = json!(contract_fields);
            analysis["contract_gaps"] = json!(contract_gaps);
            analysis["contract_readers"] = json!([
                "literal-path-segments",
                "axum-extractor-types",
                "spring-parameter-annotations",
                "declared-return-types"
            ]);
            analysis["limitations"] = json!("Partial signature evidence only. No type or import resolution, schema expansion, body analysis, runtime validation, response status or media-type inference. Names can match unrelated types or annotations. Other route-reader limits still apply: no Next.js or FastAPI-specific reader, middleware, mounted-router prefixes or cross-file handler resolution.");
        }
        let query = if contracts { "contracts" } else { "routes" };
        let mut result = self.relationship_page(query, selected, options, None, rows, analysis)?;
        result["scope"] = json!("Route declarations, local handler candidates and diagnostics in selected files. Route IDs join rows within this revision.");
        Ok(result)
    }
}
