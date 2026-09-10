use super::{bounded_text, fast_routes, hash, next_routes, Project, RelationshipOptions};
use crate::lang::Language;
use crate::parse::Parsers;
use crate::transpile::routes;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct NextProject {
    path: PathBuf,
    evidence: Value,
}

pub(super) struct RouteDeclaration {
    pub endpoint: routes::Endpoint,
    pub framework: String,
    pub name_offset: Option<usize>,
    pub basis: &'static str,
    pub fast_decorator: Option<usize>,
    pub next_catch_all: Option<next_routes::CatchAll>,
    pub next_project: Option<Value>,
}

impl Project<'_> {
    fn next_project(&self, relative: &Path) -> Result<Option<NextProject>, &'static str> {
        if !matches!(
            relative.file_name().and_then(|n| n.to_str()),
            Some("route.ts" | "route.js")
        ) {
            return Ok(None);
        }
        let manifest = relative
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .map(|dir| dir.join("package.json"))
            .find(|path| self.manifests.snapshots.contains_key(&self.root.join(path)));
        let directory = manifest
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new(""));
        let path = relative
            .strip_prefix(directory)
            .map_err(|_| "The route exceeds its observed package boundary.")?;
        if !path.starts_with("app") && !path.starts_with("src/app") {
            return Ok(None);
        }
        if directory.as_os_str().is_empty() {
            return Ok(Some(NextProject {
                path: path.to_path_buf(),
                evidence: json!({"root": ".", "manifest": null, "basis": "project-root-layout"}),
            }));
        }
        let Some(document) = manifest
            .as_ref()
            .and_then(|path| self.manifests.documents.get(path))
        else {
            return Err("The nearest nested package manifest is unavailable or invalid; its App Router layout remains unknown.");
        };
        if !["dependencies", "devDependencies"].iter().any(|section| {
            document[*section]["next"]
                .as_str()
                .is_some_and(|version| !version.trim().is_empty())
        }) {
            return Err("The nearest nested package has no string-valued next dependency or devDependency; its App Router layout remains unknown.");
        }
        Ok(Some(NextProject {
            path: path.to_path_buf(),
            evidence: json!({"root": bounded_text(&directory.to_string_lossy(), 512),
            "manifest": manifest.map(|path| bounded_text(&path.to_string_lossy(), 512)), "basis": "observed-npm-next-dependency"}),
        }))
    }

    pub(super) fn routes(
        &self,
        options: &RelationshipOptions,
        contracts: bool,
        types: bool,
    ) -> Result<Value> {
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
        let mut route_dependencies = 0usize;
        let mut service_dependencies = 0usize;
        let mut service_gaps = 0usize;
        let mut next_gaps = 0usize;
        let mut fast_gaps = 0usize;
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
            let mut endpoints: Vec<_> = routes::endpoints_of(source, info.language)
                .into_iter()
                .flat_map(|(framework, endpoints)| {
                    endpoints.into_iter().map(move |endpoint| RouteDeclaration {
                        endpoint,
                        framework: framework.to_string(),
                        name_offset: None,
                        basis: "route-pattern-reader",
                        fast_decorator: None,
                        next_catch_all: None,
                        next_project: None,
                    })
                })
                .collect();
            if matches!(info.language, Language::TypeScript | Language::Tsx) {
                let project = self.next_project(relative);
                let (next, evidence) = match project {
                    Ok(Some(project)) => (
                        next_routes::read(&project.path, &parsed, source),
                        Some(project.evidence),
                    ),
                    Ok(None) => (next_routes::NextRoutes::default(), None),
                    Err(reason) => (
                        next_routes::NextRoutes {
                            entries: Vec::new(),
                            gaps: vec![(1, reason)],
                        },
                        None,
                    ),
                };
                next_gaps += next.gaps.len();
                for (line, reason) in next.gaps {
                    rows.push(json!({"kind": "analysis-gap", "path": path, "line": line,
                        "basis": "nextjs-app-reader", "reason": reason}));
                }
                endpoints.extend(next.entries.into_iter().map(|entry| RouteDeclaration {
                    endpoint: entry.endpoint,
                    framework: "nextjs-app".to_owned(),
                    name_offset: Some(entry.name_offset),
                    basis: entry.basis,
                    fast_decorator: None,
                    next_catch_all: entry.catch_all,
                    next_project: evidence.clone(),
                }));
            }
            if info.language == Language::Python {
                let fast = fast_routes::read(&parsed, source);
                fast_gaps += fast.gaps.len();
                endpoints.retain(|entry| !fast.claimed_lines.contains(&entry.endpoint.line));
                for (line, reason) in fast.gaps {
                    rows.push(json!({"kind": "analysis-gap", "path": path, "line": line,
                        "basis": "fastapi-reader", "reason": reason}));
                }
                endpoints.extend(fast.entries.into_iter().map(|entry| RouteDeclaration {
                    endpoint: entry.endpoint,
                    framework: "fastapi".to_owned(),
                    name_offset: Some(entry.name_offset),
                    basis: "fastapi-import-constructor-decorator",
                    fast_decorator: Some(entry.decorator_offset),
                    next_catch_all: None,
                    next_project: None,
                }));
            }
            if endpoints.is_empty() {
                empty += 1;
                continue;
            }
            let file_node = self
                .nodes
                .iter()
                .position(|node| node.kind == "file" && node.path == relative)
                .context("route declaration has no project file node")?;
            for (ordinal, declaration) in endpoints.iter().enumerate() {
                let RouteDeclaration {
                    endpoint,
                    framework,
                    name_offset,
                    basis,
                    ..
                } = declaration;
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
                            .filter(|symbol| {
                                name_offset.is_none_or(|offset| symbol.name_span.start == offset)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                handlers += candidates.len();
                let handler_basis = if name_offset.is_some() {
                    "declaration-span"
                } else {
                    "same-file-name"
                };
                let handler_confidence = if name_offset.is_some() {
                    Value::Null
                } else {
                    json!("name-only")
                };
                rows.push(json!({"kind": "route", "id": id, "path": path,
                    "file_handle": self.handle(file_node), "line": endpoint.line,
                    "method": endpoint.method, "url": bounded_text(&endpoint.url, 512),
                    "framework_candidate": framework, "basis": basis, "status": "candidate", "confidence": null,
                    "nextjs_project": declaration.next_project,
                    "handler": {"name": endpoint.handler.as_deref().map(|name| bounded_text(name, 160)),
                        "candidate_count": candidates.len(), "basis": handler_basis,
                        "status": if endpoint.handler.is_none() { "unnamed" } else if candidates.is_empty() { "unresolved" } else if candidates.len() == 1 { "candidate" } else { "ambiguous" }}}));
                if contracts {
                    let details =
                        self.contract_rows(&id, declaration, &candidates, &parsed, source, types)?;
                    contract_fields += details
                        .iter()
                        .filter(|r| r["kind"] == "route-contract-field")
                        .count();
                    contract_gaps += details
                        .iter()
                        .filter(|r| r["kind"] == "route-contract-gap")
                        .count();
                    route_dependencies += details
                        .iter()
                        .filter(|r| r["kind"] == "route-dependency")
                        .count();
                    service_dependencies += details
                        .iter()
                        .filter(|r| r["kind"] == "route-service-dependency")
                        .count();
                    service_gaps += details
                        .iter()
                        .filter(|r| r["kind"] == "route-service-gap")
                        .count();
                    rows.extend(details);
                }
                for candidate in candidates {
                    rows.push(json!({"kind": "route-handler", "route": id,
                        "handler": self.endpoint(candidate.id)?, "status": "candidate",
                        "basis": handler_basis, "confidence": handler_confidence}));
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
            "readers": ["express", "flask", "axum", "gin", "spring", "nextjs-app", "fastapi"],
            "fastapi_gaps": fast_gaps,
            "fastapi_limitations": "Top-level verb decorators on a direct FastAPI/APIRouter constructor assignment with an observed fastapi import. The reader joins valid plain literal constructor prefixes. No runtime import validation, shadowing analysis, factories, nested routers, include-router or mounted prefixes, or method-list decorators.",
            "nextjs_gaps": next_gaps,
            "nextjs_limitations": "Only route.ts and route.js under app or src/app at the project root or an observed nested npm Next.js package. Nearest observed manifests bound nested layouts. Runtime package identity, layout precedence, route validity, basePath, rewrites and implicit methods remain unchecked.",
            "certainty": "Declaration patterns and local handler-name candidates; the reader does not verify framework identity or runtime reachability.",
            "limitations": "No request/response schema expansion, middleware, mounted-router prefixes or cross-file handler resolution. Next.js covers function declarations and direct arrow/function-expression bindings with static, grouped, single dynamic and terminal catch-all segments. Other path forms, Pages Router, wrapped handlers and cross-file re-exports remain unsupported. Empty results do not prove absence of routes."});
        if contracts {
            analysis["type_references_requested"] = json!(types);
            if types {
                analysis["type_reference_limitations"] = json!("Supported declared returns and Axum request types only. Same-file names supply candidates, without import or lexical resolution. FastAPI marker types and response_model expressions remain outside reference inspection.");
                analysis["type_references"] = json!(rows
                    .iter()
                    .filter(|r| r["kind"] == "route-contract-type-reference")
                    .count());
                analysis["type_candidates"] = json!(rows
                    .iter()
                    .filter(|r| r["kind"] == "route-contract-type-candidate")
                    .count());
            }
            analysis["contract_fields"] = json!(contract_fields);
            analysis["contract_gaps"] = json!(contract_gaps);
            analysis["route_dependencies"] = json!(route_dependencies);
            analysis["service_dependencies"] = json!(service_dependencies);
            analysis["service_gaps"] = json!(service_gaps);
            analysis["contract_readers"] = json!([
                "literal-path-segments",
                "axum-extractor-types",
                "spring-parameter-annotations",
                "declared-return-types",
                "nextjs-catch-all-path",
                "fastapi-explicit-bindings",
                "fastapi-response-model"
            ]);
            analysis["limitations"] = json!("Partial signature evidence only. No type resolution, schema expansion, body analysis, runtime validation, response status or media-type inference. Names can match unrelated types or annotations. FastAPI covers explicit binding markers and response_model expressions; implicit parameter classification and binding aliases remain unknown. Next.js input bindings remain unknown; its route subset covers function declarations and direct arrow/function-expression bindings, including local exports and terminal catch-all paths. No middleware, mounted-router prefixes or cross-file handler resolution.");
        }
        let query = if contracts { "contracts" } else { "routes" };
        let mut result = self.relationship_page(query, selected, options, None, rows, analysis)?;
        result["scope"] = json!("Route declarations, local handler candidates and diagnostics in selected files. Route IDs join rows within this revision.");
        Ok(result)
    }
}
