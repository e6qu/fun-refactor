mod boundaries;
mod component_facts;

use super::{bounded_text, hash, links, FeatureOptions, Project, RelationshipOptions};
use crate::analysis::stitch;
use crate::lang::Language;
use crate::parse::Parsers;
use crate::project::components;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const SOURCE_FACT_LIMIT: usize = 500;
const SCHEMA_EXPANSION_LIMIT: usize = 64;
const BUILD_SETTING_LIMIT: usize = 64;
const DEPENDENCY_FACT_LIMIT: usize = 256;
const MIDDLEWARE_FACT_LIMIT: usize = 64;
const LIFECYCLE_FACT_LIMIT: usize = 64;
const CONFIGURATION_FACT_LIMIT: usize = 128;
const CONFIGURATION_CONSUMER_LIMIT: usize = 256;
const EXECUTION_DEPENDENCY_FACT_LIMIT: usize = 256;
const COMPONENT_FACT_LIMIT: usize = 128;
const COMPONENT_DETAIL_FACT_LIMIT: usize = 512;
const COMPONENT_FILE_LIMIT: usize = 64;
const COMPONENT_FILE_GAP_LIMIT: usize = 128;

#[derive(Clone)]
struct Feature {
    id: String,
    url: Value,
    source: Value,
    routes: Vec<Value>,
    frontend_files: BTreeSet<PathBuf>,
    frontend_gaps: Vec<FrontendGap>,
    frontend_gaps_omitted: usize,
}

#[derive(Clone)]
struct FrontendGap {
    path: PathBuf,
    line: usize,
    reason: &'static str,
}

impl Feature {
    fn add_frontend_gap(&mut self, path: PathBuf, line: usize, reason: &'static str) {
        if self
            .frontend_gaps
            .iter()
            .any(|gap| gap.path == path && gap.line == line && gap.reason == reason)
        {
            return;
        }
        if self.frontend_gaps.len() < COMPONENT_FILE_GAP_LIMIT {
            self.frontend_gaps.push(FrontendGap { path, line, reason });
        } else {
            self.frontend_gaps_omitted += 1;
        }
    }
}

struct Application {
    id: String,
    framework: String,
    root: Value,
    source: Value,
    basis: &'static str,
    manifest: Option<PathBuf>,
    features: BTreeMap<String, Feature>,
}

struct ApplicationDiscovery {
    applications: BTreeMap<String, Application>,
    frontend_gaps: Vec<(PathBuf, &'static str)>,
}

#[derive(Default)]
struct PackageCounts {
    packages: usize,
    build_settings: usize,
    dependencies: usize,
    gaps: usize,
    build_settings_omitted: usize,
    dependencies_omitted: usize,
}

#[derive(Default)]
struct BoundaryCounts {
    middleware: usize,
    middleware_gaps: usize,
    middleware_omitted: usize,
    lifecycle_hooks: usize,
    lifecycle_gaps: usize,
    lifecycle_omitted: usize,
    execution_dependencies: usize,
    execution_dependencies_omitted: usize,
    authentication_candidates: usize,
    runtime_configurations: usize,
    configuration_consumers: usize,
    configuration_gaps: usize,
    configurations_omitted: usize,
    configuration_consumers_omitted: usize,
    service_dependencies: usize,
    service_gaps: usize,
    components: usize,
    component_details: usize,
    component_properties: usize,
    component_states: usize,
    component_effects: usize,
    component_hooks: usize,
    component_events: usize,
    component_styles: usize,
    render_edges: usize,
    component_gaps: usize,
    components_omitted: usize,
    component_details_omitted: usize,
}

fn key(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON values always serialize")
}

fn value_text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("text").and_then(Value::as_str))
}

fn source(row: &Value) -> Value {
    json!({
        "path": row.get("path").cloned().unwrap_or(Value::Null),
        "line": row.get("line").cloned().unwrap_or(Value::Null),
        "handle": row.get("file_handle").cloned().unwrap_or(Value::Null),
    })
}

fn next_page_url(path: &Path) -> Option<Result<String, &'static str>> {
    if !matches!(path.file_name()?.to_str()?, "page.tsx" | "page.jsx") {
        return None;
    }
    let relative = path
        .strip_prefix("app")
        .or_else(|_| path.strip_prefix("src/app"))
        .ok()?;
    let mut segments = Vec::new();
    for part in relative.parent()?.components() {
        let part = part.as_os_str().to_str()?;
        if part.starts_with('_') || part.starts_with('@') || part.starts_with("(.") {
            return Some(Err(
                "Private, parallel and intercepting page paths need further inspection.",
            ));
        }
        if let Some(group) = part
            .strip_prefix('(')
            .and_then(|part| part.strip_suffix(')'))
        {
            if group.is_empty() || group.contains(['(', ')']) {
                return Some(Err("The page route group exceeds the supported subset."));
            }
            continue;
        }
        if let Some(name) = part
            .strip_prefix("[[...")
            .and_then(|name| name.strip_suffix("]]"))
        {
            if !crate::project::contracts::simple_name(name) {
                return Some(Err("The page has a malformed catch-all parameter."));
            }
            segments.push(format!("{{...{name}?}}"));
        } else if let Some(name) = part
            .strip_prefix("[...")
            .and_then(|name| name.strip_suffix(']'))
        {
            if !crate::project::contracts::simple_name(name) {
                return Some(Err("The page has a malformed catch-all parameter."));
            }
            segments.push(format!("{{...{name}}}"));
        } else if let Some(name) = part
            .strip_prefix('[')
            .and_then(|name| name.strip_suffix(']'))
        {
            if !crate::project::contracts::simple_name(name) {
                return Some(Err("The page has a malformed dynamic parameter."));
            }
            segments.push(format!("{{{name}}}"));
        } else if !part
            .chars()
            .all(|character| character.is_alphanumeric() || "-._~".contains(character))
        {
            return Some(Err("The page path exceeds the literal segment subset."));
        } else {
            segments.push(part.to_owned());
        }
    }
    Some(Ok(format!("/{}", segments.join("/"))))
}

fn declaration_source(row: &Value) -> Value {
    let declaration = &row["declaration"];
    json!({
        "path": declaration.get("path").cloned().unwrap_or(Value::Null),
        "line": declaration.get("line").cloned().unwrap_or(Value::Null),
        "handle": declaration.get("handle").cloned().unwrap_or(Value::Null),
    })
}

fn evidence(basis: Value, validation: &[&str]) -> Value {
    json!({"basis": basis, "validation": validation})
}

struct FactEvidence {
    basis: Value,
    status: Value,
    confidence: Value,
    gaps: Value,
    validation: &'static [&'static str],
}

impl FactEvidence {
    fn new(basis: Value, status: Value, confidence: Value, gaps: Value) -> Self {
        Self {
            basis,
            status,
            confidence,
            gaps,
            validation: &["captured-source", "syntax-tree", "project-revision"],
        }
    }

    fn manifest(basis: Value, status: Value, confidence: Value, gaps: Value) -> Self {
        Self {
            basis,
            status,
            confidence,
            gaps,
            validation: &["captured-manifest", "project-revision"],
        }
    }
}

fn fact(
    kind: &str,
    id: String,
    parent: Option<&str>,
    source: Value,
    fact_evidence: FactEvidence,
    detail_name: &str,
    detail: Value,
) -> Value {
    let FactEvidence {
        basis,
        status,
        confidence,
        gaps,
        validation,
    } = fact_evidence;
    let mut row = json!({
        "kind": kind,
        "id": id,
        "parent": parent,
        "source": source,
        "status": status,
        "confidence": confidence,
        "evidence": evidence(basis, validation),
        "gaps": gaps,
    });
    row[detail_name] = detail;
    row
}

fn route_id(revision: &str, feature: &str, raw: &Value) -> Result<String> {
    Ok(format!(
        "frfr1:{}",
        &hash((revision, feature, &raw["id"]))?[..32]
    ))
}

fn child_id(prefix: &str, revision: &str, parent: &str, raw: &Value) -> Result<String> {
    Ok(format!(
        "{prefix}:{}",
        &hash((revision, parent, raw))?[..32]
    ))
}

fn route_children(items: &[Value]) -> BTreeMap<String, Vec<Value>> {
    let mut result: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for item in items {
        if matches!(
            item["kind"].as_str(),
            Some(
                "route-handler"
                    | "route-contract-field"
                    | "route-contract-gap"
                    | "route-dependency"
                    | "route-service-dependency"
                    | "route-service-gap"
            )
        ) {
            if let Some(route) = item["route"].as_str() {
                result
                    .entry(route.to_owned())
                    .or_default()
                    .push(item.clone());
            }
        }
    }
    result
}

fn field_references(items: &[Value]) -> BTreeMap<String, Vec<Value>> {
    let mut result: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for item in items {
        if item["kind"] == "route-contract-type-reference" {
            if let Some(field) = item["field"].as_str() {
                result
                    .entry(field.to_owned())
                    .or_default()
                    .push(item.clone());
            }
        }
    }
    result
}

fn reference_candidates(items: &[Value]) -> BTreeMap<String, Vec<Value>> {
    let mut result: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for item in items {
        if item["kind"] == "route-contract-type-candidate" {
            if let Some(reference) = item["reference"].as_str() {
                result
                    .entry(reference.to_owned())
                    .or_default()
                    .push(item.clone());
            }
        }
    }
    result
}

impl Project<'_> {
    fn expand_frontend_files(&self, application_root: &Value, feature: &mut Feature) -> Result<()> {
        let package_root = value_text(application_root)
            .filter(|root| *root != ".")
            .map(PathBuf::from)
            .unwrap_or_default();
        let pages = feature.frontend_files.iter().cloned().collect::<Vec<_>>();
        let mut page_directories: BTreeMap<PathBuf, usize> = BTreeMap::new();
        for page in &pages {
            *page_directories
                .entry(page.parent().unwrap_or(Path::new("")).to_path_buf())
                .or_default() += 1;
        }
        for (directory, count) in page_directories {
            if count > 1 {
                feature.add_frontend_gap(
                    directory,
                    1,
                    "Multiple page convention files map to the same feature path.",
                );
            }
        }
        for page in pages {
            let mut directory = page.parent();
            while let Some(current) = directory {
                let layouts = ["layout.tsx", "layout.jsx"]
                    .into_iter()
                    .map(|name| current.join(name))
                    .filter(|layout| self.sources.contains_key(&self.root.join(layout)))
                    .collect::<Vec<_>>();
                if layouts.len() > 1 {
                    feature.add_frontend_gap(
                        current.to_path_buf(),
                        1,
                        "Multiple layout convention files apply at one route level.",
                    );
                }
                for layout in layouts {
                    if feature.frontend_files.len() < COMPONENT_FILE_LIMIT {
                        feature.frontend_files.insert(layout);
                    } else if !feature.frontend_files.contains(&layout) {
                        feature.add_frontend_gap(
                            layout,
                            1,
                            "The component file limit omitted a layout file.",
                        );
                    }
                }
                if current.file_name().is_some_and(|name| name == "app") {
                    break;
                }
                directory = current.parent();
            }
        }
        let mut pending = feature.frontend_files.iter().cloned().collect::<Vec<_>>();
        let mut inspected = BTreeSet::new();
        while let Some(path) = pending.pop() {
            if !inspected.insert(path.clone()) {
                continue;
            }
            let source = &self.sources[&self.root.join(&path)];
            let parsed = Parsers::new().parse(Language::Tsx, source)?;
            if parsed.has_errors() {
                continue;
            }
            for import in components::imports(&parsed, source) {
                if !import.local.chars().next().is_some_and(char::is_uppercase)
                    || !import.source.starts_with('.')
                {
                    continue;
                }
                let candidates =
                    components::import_candidates(&path, &import.source, &self.sources, &self.root);
                if candidates.len() != 1 {
                    feature.add_frontend_gap(
                        path.clone(),
                        import.line,
                        if candidates.is_empty() {
                            "A relative component import has no captured TSX or JSX target."
                        } else {
                            "A relative component import has multiple captured TSX or JSX targets."
                        },
                    );
                    continue;
                }
                let target = candidates.into_iter().next().unwrap();
                if !package_root.as_os_str().is_empty() && !target.starts_with(&package_root) {
                    feature.add_frontend_gap(
                        path.clone(),
                        import.line,
                        "A relative component import crosses the captured package boundary.",
                    );
                    continue;
                }
                if feature.frontend_files.contains(&target) {
                    continue;
                }
                if feature.frontend_files.len() >= COMPONENT_FILE_LIMIT {
                    feature.add_frontend_gap(
                        path.clone(),
                        import.line,
                        "The component file limit omitted a relative import target.",
                    );
                    continue;
                }
                feature.frontend_files.insert(target.clone());
                pending.push(target);
            }
        }
        Ok(())
    }

    fn applications(&self, routes: &[Value], selected: usize) -> Result<ApplicationDiscovery> {
        let mut applications = BTreeMap::new();
        let mut frontend_gaps = Vec::new();
        for route in routes.iter().filter(|row| row["kind"] == "route") {
            let Some(framework) = route["framework_candidate"].as_str() else {
                continue;
            };
            let (root, basis, manifest) = match framework {
                "nextjs-app" => {
                    let root = route["nextjs_project"]
                        .get("root")
                        .cloned()
                        .unwrap_or_else(|| json!("."));
                    let manifest = route["nextjs_project"]
                        .get("manifest")
                        .and_then(value_text)
                        .map(PathBuf::from)
                        .or_else(|| {
                            let root = value_text(&root)?;
                            let path = if root == "." {
                                PathBuf::from("package.json")
                            } else {
                                Path::new(root).join("package.json")
                            };
                            self.manifests.documents.contains_key(&path).then_some(path)
                        });
                    (root, "nextjs-package-root", manifest)
                }
                "fastapi" => (route["path"].clone(), "fastapi-route-file", None),
                _ => continue,
            };
            let app_key = key(&json!([framework, root]));
            let app_id = format!("frfa1:{}", &hash((&self.revision, &app_key))?[..32]);
            let app = applications.entry(app_key).or_insert_with(|| Application {
                id: app_id,
                framework: framework.to_owned(),
                root: root.clone(),
                source: source(route),
                basis,
                manifest,
                features: BTreeMap::new(),
            });
            let feature_key = key(&route["url"]);
            let feature_id = format!(
                "frff1:{}",
                &hash((&self.revision, &app.id, &feature_key))?[..32]
            );
            app.features
                .entry(feature_key)
                .or_insert_with(|| Feature {
                    id: feature_id,
                    url: route["url"].clone(),
                    source: source(route),
                    routes: Vec::new(),
                    frontend_files: BTreeSet::new(),
                    frontend_gaps: Vec::new(),
                    frontend_gaps_omitted: 0,
                })
                .routes
                .push(route.clone());
        }
        for file in self.sources.keys() {
            if !self.scope_file(selected, file) {
                continue;
            }
            let relative = file.strip_prefix(&self.root)?;
            let manifest = relative
                .parent()
                .into_iter()
                .flat_map(Path::ancestors)
                .map(|directory| directory.join("package.json"))
                .find(|manifest| {
                    self.manifests
                        .snapshots
                        .contains_key(&self.root.join(manifest))
                });
            let directory = manifest
                .as_deref()
                .and_then(Path::parent)
                .unwrap_or(Path::new(""));
            let package_path = relative.strip_prefix(directory)?;
            let Some(url) = next_page_url(package_path) else {
                continue;
            };
            let url = match url {
                Ok(url) => url,
                Err(reason) => {
                    frontend_gaps.push((relative.to_path_buf(), reason));
                    continue;
                }
            };
            if let Some(document) = manifest
                .as_ref()
                .and_then(|manifest| self.manifests.documents.get(manifest))
            {
                let next = ["dependencies", "devDependencies"].iter().any(|section| {
                    document[*section]["next"]
                        .as_str()
                        .is_some_and(|version| !version.trim().is_empty())
                });
                if !next {
                    frontend_gaps.push((
                        relative.to_path_buf(),
                        "The nearest package has no captured string-valued Next.js dependency.",
                    ));
                    continue;
                }
            } else if manifest.is_some() {
                frontend_gaps.push((
                    relative.to_path_buf(),
                    "The nearest package manifest is unavailable or invalid.",
                ));
                continue;
            }
            let root = if directory.as_os_str().is_empty() {
                json!(".")
            } else {
                bounded_text(&directory.to_string_lossy(), 512)
            };
            let app_key = key(&json!(["nextjs-app", root]));
            let app_id = format!("frfa1:{}", &hash((&self.revision, &app_key))?[..32]);
            let app = applications.entry(app_key).or_insert_with(|| Application {
                id: app_id,
                framework: "nextjs-app".to_owned(),
                root: root.clone(),
                source: self.file_source(relative, 1),
                basis: if manifest.is_some() {
                    "nextjs-package-page"
                } else {
                    "project-root-page"
                },
                manifest: manifest.clone(),
                features: BTreeMap::new(),
            });
            let feature_key = key(&json!(url));
            let feature_id = format!(
                "frff1:{}",
                &hash((&self.revision, &app.id, &feature_key))?[..32]
            );
            app.features
                .entry(feature_key)
                .or_insert_with(|| Feature {
                    id: feature_id,
                    url: json!(url),
                    source: self.file_source(relative, 1),
                    routes: Vec::new(),
                    frontend_files: BTreeSet::new(),
                    frontend_gaps: Vec::new(),
                    frontend_gaps_omitted: 0,
                })
                .frontend_files
                .insert(relative.to_path_buf());
        }
        for application in applications.values_mut() {
            if application.framework != "nextjs-app" {
                continue;
            }
            let application_root = application.root.clone();
            for feature in application.features.values_mut() {
                self.expand_frontend_files(&application_root, feature)?;
            }
        }
        Ok(ApplicationDiscovery {
            applications,
            frontend_gaps,
        })
    }

    fn package_facts(
        &self,
        application: &Application,
        local_links: &[Value],
        rows: &mut Vec<Value>,
        counts: &mut PackageCounts,
    ) -> Result<()> {
        let Some(manifest) = &application.manifest else {
            counts.gaps += 1;
            let reason = if application.framework == "fastapi" {
                "Python package manifests are outside the current Cargo/npm project reader."
            } else {
                "No captured npm manifest establishes this application package."
            };
            let detail = json!({"reason": reason, "framework": application.framework});
            rows.push(fact(
                "package-gap",
                child_id("frfpg1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                application.source.clone(),
                FactEvidence::new(
                    json!("package-boundary-reader"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
            return Ok(());
        };
        let manifest_text = manifest.to_string_lossy();
        let Some(package) = self.manifests.packages.iter().find(|row| {
            row["manifest"]
                .as_str()
                .is_some_and(|path| path == manifest_text)
        }) else {
            counts.gaps += 1;
            let reason = "The captured manifest has no usable package declaration.";
            let detail = json!({"reason": reason, "manifest": bounded_text(&manifest_text, 512)});
            rows.push(fact(
                "package-gap",
                child_id("frfpg1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                json!({"path": bounded_text(&manifest_text, 512), "line": null, "handle": null}),
                FactEvidence::manifest(
                    json!("manifest-declaration"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
            return Ok(());
        };

        counts.packages += 1;
        let package_id = child_id("frfp1", &self.revision, &application.id, package)?;
        let package_source =
            json!({"path": bounded_text(&manifest_text, 512), "line": null, "handle": null});
        rows.push(fact(
            "package",
            package_id.clone(),
            Some(&application.id),
            package_source.clone(),
            FactEvidence::manifest(
                package["basis"].clone(),
                json!("declared"),
                Value::Null,
                json!(["Package-manager validation and source ownership remain unchecked."]),
            ),
            "package",
            json!({
                "manifest": package["manifest"],
                "root": package["root"],
                "ecosystem": package["ecosystem"],
                "name": package["name"],
                "version": package["version"],
            }),
        ));

        let scripts_value = self
            .manifests
            .documents
            .get(manifest)
            .and_then(|document| document.get("scripts"));
        if scripts_value.is_some_and(|scripts| !scripts.is_object()) {
            counts.gaps += 1;
            let reason = "The npm scripts declaration is not an object.";
            let detail = json!({"reason": reason});
            rows.push(fact(
                "package-gap",
                child_id("frfpg1", &self.revision, &package_id, &detail)?,
                Some(&package_id),
                package_source.clone(),
                FactEvidence::manifest(
                    json!("npm-script-declaration"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        if let Some(scripts) = scripts_value.and_then(Value::as_object) {
            let mut scripts: Vec<_> = scripts.iter().collect();
            scripts.sort_by_key(|(name, _)| *name);
            let omitted = scripts.len().saturating_sub(BUILD_SETTING_LIMIT);
            counts.build_settings_omitted += omitted;
            for (name, command) in scripts.into_iter().take(BUILD_SETTING_LIMIT) {
                if let Some(command) = command.as_str() {
                    counts.build_settings += 1;
                    let detail = json!({
                        "kind": "npm-script",
                        "name": bounded_text(name, 160),
                        "command": bounded_text(command, 512),
                    });
                    rows.push(fact(
                        "build-setting",
                        child_id("frfbs1", &self.revision, &package_id, &detail)?,
                        Some(&package_id),
                        package_source.clone(),
                        FactEvidence::manifest(
                            json!("npm-script-declaration"),
                            json!("declared"),
                            Value::Null,
                            json!(["No runner has executed or resolved this declared command."]),
                        ),
                        "build_setting",
                        detail,
                    ));
                } else {
                    counts.gaps += 1;
                    let reason = "An npm script has a non-string value.";
                    let detail = json!({"reason": reason, "name": bounded_text(name, 160)});
                    rows.push(fact(
                        "package-gap",
                        child_id("frfpg1", &self.revision, &package_id, &detail)?,
                        Some(&package_id),
                        package_source.clone(),
                        FactEvidence::manifest(
                            json!("npm-script-declaration"),
                            json!("gap"),
                            Value::Null,
                            json!([reason]),
                        ),
                        "gap",
                        detail,
                    ));
                }
            }
            if omitted > 0 {
                counts.gaps += 1;
                let reason =
                    "The build-setting limit omitted npm scripts; narrow the application scope.";
                let detail = json!({"reason": reason, "omitted": omitted});
                rows.push(fact(
                    "package-gap",
                    child_id("frfpg1", &self.revision, &package_id, &detail)?,
                    Some(&package_id),
                    package_source.clone(),
                    FactEvidence::manifest(
                        json!("build-setting-limit"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
        }

        let mut dependencies: Vec<_> = self
            .manifests
            .declarations
            .iter()
            .filter(|(path, row)| path == manifest && row["kind"] == "dependency")
            .map(|(_, row)| row)
            .collect();
        dependencies.sort_by_cached_key(|row| row.to_string());
        let omitted = dependencies.len().saturating_sub(DEPENDENCY_FACT_LIMIT);
        counts.dependencies_omitted += omitted;
        for dependency in dependencies.into_iter().take(DEPENDENCY_FACT_LIMIT) {
            counts.dependencies += 1;
            let link = local_links.iter().find(|link| {
                link["kind"] == "local-dependency"
                    && link["manifest"] == dependency["manifest"]
                    && link["name"] == dependency["name"]
                    && link["section"] == dependency["section"]
            });
            let boundary = match link.and_then(|row| row["status"].as_str()) {
                Some("linked") => "local-package",
                Some(_) => "unresolved-local",
                None => "external-or-unresolved",
            };
            let gaps = match boundary {
                "local-package" => {
                    json!(["Version compatibility, feature activation and runtime use remain unchecked."])
                }
                "unresolved-local" => json!(["The declared local package link did not resolve within the captured manifest snapshot."]),
                _ => json!(["Package-manager resolution and runtime use remain unchecked."]),
            };
            let detail = json!({
                "name": dependency["name"],
                "section": dependency["section"],
                "scope": dependency["scope"],
                "requirement": dependency["requirement"],
                "target_condition": dependency["target_condition"],
                "resolution": dependency["resolution"],
                "boundary": boundary,
                "target_manifest": link.map(|row| row["target_manifest"].clone()).unwrap_or(Value::Null),
                "link_status": link.map(|row| row["status"].clone()).unwrap_or(Value::Null),
                "link_reason": link.map(|row| row["reason"].clone()).unwrap_or(Value::Null),
            });
            rows.push(fact(
                "dependency",
                child_id("frfd1", &self.revision, &package_id, &detail)?,
                Some(&package_id),
                package_source.clone(),
                FactEvidence::manifest(
                    dependency["basis"].clone(),
                    dependency
                        .get("declaration_status")
                        .cloned()
                        .unwrap_or_else(|| json!("declared")),
                    Value::Null,
                    gaps,
                ),
                "dependency",
                detail,
            ));
        }
        if omitted > 0 {
            counts.gaps += 1;
            let reason =
                "The dependency fact limit omitted declarations; narrow the application scope.";
            let detail = json!({"reason": reason, "omitted": omitted});
            rows.push(fact(
                "package-gap",
                child_id("frfpg1", &self.revision, &package_id, &detail)?,
                Some(&package_id),
                package_source,
                FactEvidence::manifest(
                    json!("dependency-fact-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        Ok(())
    }

    fn schema_facts(
        &self,
        candidate: &Value,
        parent: &str,
        rows: &mut Vec<Value>,
        expanded: &mut usize,
        omitted: &mut usize,
    ) -> Result<bool> {
        let Some(handle) = candidate["target"]["handle"].as_str() else {
            return Ok(false);
        };
        if *expanded >= SCHEMA_EXPANSION_LIMIT {
            *omitted += 1;
            return Ok(false);
        }
        *expanded += 1;
        let selection = RelationshipOptions {
            target: handle.to_owned(),
            revision: None,
            limit: SOURCE_FACT_LIMIT,
            cursor: None,
        };
        let report = self.schemas(&selection)?;
        let schema_rows = report["items"].as_array().cloned().unwrap_or_default();
        if report["page"]["remaining"].as_u64().unwrap_or(0) > 0 {
            *omitted += 1;
        }
        let Some(schema) = schema_rows.iter().find(|row| row["kind"] == "schema") else {
            return Ok(false);
        };
        let schema_id = child_id("frfs1", &self.revision, parent, schema)?;
        let schema_source = declaration_source(schema);
        rows.push(fact(
            "schema",
            schema_id.clone(),
            Some(parent),
            schema_source.clone(),
            FactEvidence::new(
                schema["basis"].clone(),
                schema["status"].clone(),
                schema["confidence"].clone(),
                json!([]),
            ),
            "schema",
            json!({
                "declaration": schema["declaration"],
                "field_count": schema["field_count"],
                "completeness": schema["completeness"],
            }),
        ));
        let schema_handle = schema["declaration"]["handle"].clone();
        for row in schema_rows.iter().filter(|row| {
            matches!(row["kind"].as_str(), Some("schema-field" | "schema-gap"))
                && row["schema"] == schema_handle
        }) {
            let kind = if row["kind"] == "schema-field" {
                "schema-field"
            } else {
                "schema-gap"
            };
            let id = child_id("frfsf1", &self.revision, &schema_id, row)?;
            rows.push(fact(
                kind,
                id,
                Some(&schema_id),
                json!({
                    "path": schema_source["path"],
                    "line": row.get("line").cloned().unwrap_or(Value::Null),
                    "handle": schema_source["handle"],
                }),
                FactEvidence::new(
                    row.get("basis")
                        .cloned()
                        .unwrap_or_else(|| json!("schema-reader")),
                    row.get("status").cloned().unwrap_or_else(|| json!("gap")),
                    row.get("confidence").cloned().unwrap_or(Value::Null),
                    if kind == "schema-gap" {
                        json!([row["reason"]])
                    } else {
                        json!([])
                    },
                ),
                "field",
                if kind == "schema-field" {
                    json!({
                        "name": row["name"],
                        "declared_type": row["declared_type"],
                        "required": row["required"],
                        "optional_marker": row["optional_marker"],
                        "readonly_marker": row["readonly_marker"],
                    })
                } else {
                    json!({"reason": row["reason"]})
                },
            ));
        }
        Ok(true)
    }

    pub(super) fn features(&self, options: &FeatureOptions) -> Result<Value> {
        let selected = self.relationship_selection(&options.selection)?;
        let source_options = RelationshipOptions {
            target: options.selection.target.clone(),
            revision: options.selection.revision.clone(),
            limit: SOURCE_FACT_LIMIT,
            cursor: None,
        };
        let report = self.routes(&source_options, true, true)?;
        let configuration = stitch::analyze_snapshot(self.index, &self.sources)?;
        let items = report["items"].as_array().cloned().unwrap_or_default();
        let source_omitted = report["page"]["remaining"].as_u64().unwrap_or(0) as usize;
        let ApplicationDiscovery {
            applications,
            frontend_gaps,
        } = self.applications(&items, selected)?;
        let local_links = links::collect(&self.manifests, &self.root, None);
        let available: BTreeSet<_> = applications
            .values()
            .flat_map(|app| app.features.values().map(|feature| feature.id.clone()))
            .collect();
        if let Some(feature) = &options.feature {
            if !available.contains(feature) {
                bail!("feature ID is absent from this project revision and selected scope.");
            }
        }

        let children = route_children(&items);
        let references = field_references(&items);
        let candidates = reference_candidates(&items);
        let mut rows = Vec::new();
        let mut application_count = 0usize;
        let mut feature_count = 0usize;
        let mut route_count = 0usize;
        let mut handler_count = 0usize;
        let mut contract_count = 0usize;
        let mut schema_count = 0usize;
        let mut schema_expansions = 0usize;
        let mut schema_omitted = 0usize;
        let mut unsupported_framework_routes = 0usize;
        let mut package_counts = PackageCounts::default();
        let mut boundary_counts = BoundaryCounts::default();
        let mut selected_paths = BTreeSet::new();

        for application in applications.values() {
            let included: Vec<_> = application
                .features
                .values()
                .filter(|feature| options.feature.as_ref().is_none_or(|id| id == &feature.id))
                .collect();
            if included.is_empty() {
                continue;
            }
            application_count += 1;
            rows.push(fact(
                "application",
                application.id.clone(),
                None,
                application.source.clone(),
                FactEvidence::new(
                    json!(application.basis),
                    json!("candidate"),
                    Value::Null,
                    if application.framework == "nextjs-app" {
                        json!(["Runtime Next.js configuration and package resolution remain unchecked."])
                    } else {
                        json!(["Mounted routers, prefixes and FastAPI application factories remain unchecked."])
                    },
                ),
                "application",
                json!({
                    "framework": application.framework,
                    "root": application.root,
                    "feature_count": included.len(),
                }),
            ));
            self.package_facts(application, &local_links, &mut rows, &mut package_counts)?;
            self.middleware_facts(application, &mut rows, &mut boundary_counts)?;
            self.lifecycle_facts(application, &mut rows, &mut boundary_counts)?;
            self.global_dependency_facts(application, &mut rows, &mut boundary_counts)?;
            self.configuration_facts(application, &configuration, &mut rows, &mut boundary_counts)?;
            for feature in included {
                feature_count += 1;
                rows.push(fact(
                    "feature",
                    feature.id.clone(),
                    Some(&application.id),
                    feature.source.clone(),
                    FactEvidence::new(
                        json!("exact-route-path"),
                        json!("candidate"),
                        Value::Null,
                        json!([
                            "Business feature identity is approximated by an exact shared route path."
                        ]),
                    ),
                    "feature",
                    json!({
                        "route_path": feature.url,
                        "route_count": feature.routes.len(),
                        "component_file_count": feature.frontend_files.len(),
                    }),
                ));
                self.component_facts(application, feature, &mut rows, &mut boundary_counts)?;
                for route in &feature.routes {
                    route_count += 1;
                    if let Some(path) = route["path"].as_str() {
                        selected_paths.insert(path.to_owned());
                    }
                    let route_id = route_id(&self.revision, &feature.id, route)?;
                    rows.push(fact(
                        "route",
                        route_id.clone(),
                        Some(&feature.id),
                        source(route),
                        FactEvidence::new(
                            route["basis"].clone(),
                            route["status"].clone(),
                            route["confidence"].clone(),
                            json!([]),
                        ),
                        "route",
                        json!({
                            "source_id": route["id"],
                            "framework": route["framework_candidate"],
                            "method": route["method"],
                            "url": route["url"],
                        }),
                    ));
                    let Some(raw_id) = route["id"].as_str() else {
                        continue;
                    };
                    for child in children.get(raw_id).into_iter().flatten() {
                        match child["kind"].as_str() {
                            Some("route-handler") => {
                                handler_count += 1;
                                let id = child_id("frfh1", &self.revision, &route_id, child)?;
                                let handler = &child["handler"];
                                rows.push(fact(
                                    "handler",
                                    id,
                                    Some(&route_id),
                                    json!({"path": handler["path"], "line": handler["line"], "handle": handler["handle"]}),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        child["status"].clone(),
                                        child["confidence"].clone(),
                                        json!([]),
                                    ),
                                    "handler",
                                    handler.clone(),
                                ));
                            }
                            Some("route-contract-gap") => {
                                let id = child_id("frfcg1", &self.revision, &route_id, child)?;
                                rows.push(fact(
                                    "contract-gap",
                                    id,
                                    Some(&route_id),
                                    source(route),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        json!("gap"),
                                        Value::Null,
                                        json!([child["reason"]]),
                                    ),
                                    "contract",
                                    json!({"reason": child["reason"]}),
                                ));
                            }
                            Some("route-contract-field") => {
                                contract_count += 1;
                                let field_id = child_id("frfc1", &self.revision, &route_id, child)?;
                                rows.push(fact(
                                    "contract-field",
                                    field_id.clone(),
                                    Some(&route_id),
                                    source(route),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        child["status"].clone(),
                                        child["confidence"].clone(),
                                        json!([]),
                                    ),
                                    "contract",
                                    json!({
                                        "direction": child["direction"],
                                        "location": child["location"],
                                        "name": child["name"],
                                        "binding": child["binding"],
                                        "declared_type": child["declared_type"],
                                        "required": child["required"],
                                    }),
                                ));
                                let Some(raw_field) = child["id"].as_str() else {
                                    continue;
                                };
                                for reference in references.get(raw_field).into_iter().flatten() {
                                    let reference_id =
                                        child_id("frftr1", &self.revision, &field_id, reference)?;
                                    rows.push(fact(
                                        "schema-reference",
                                        reference_id.clone(),
                                        Some(&field_id),
                                        source(route),
                                        FactEvidence::new(
                                            reference["basis"].clone(),
                                            reference["status"].clone(),
                                            reference["confidence"].clone(),
                                            json!([]),
                                        ),
                                        "reference",
                                        json!({
                                            "name": reference["name"],
                                            "candidate_count": reference["candidate_count"],
                                        }),
                                    ));
                                    let Some(raw_reference) = reference["id"].as_str() else {
                                        continue;
                                    };
                                    for candidate in
                                        candidates.get(raw_reference).into_iter().flatten()
                                    {
                                        let candidate_id = child_id(
                                            "frftc1",
                                            &self.revision,
                                            &reference_id,
                                            candidate,
                                        )?;
                                        let target = &candidate["target"];
                                        rows.push(fact(
                                            "schema-candidate",
                                            candidate_id.clone(),
                                            Some(&reference_id),
                                            json!({"path": target["path"], "line": target["line"], "handle": target["handle"]}),
                                            FactEvidence::new(
                                                candidate["basis"].clone(),
                                                candidate["status"].clone(),
                                                candidate["confidence"].clone(),
                                                json!([]),
                                            ),
                                            "candidate",
                                            target.clone(),
                                        ));
                                        if self.schema_facts(
                                            candidate,
                                            &candidate_id,
                                            &mut rows,
                                            &mut schema_expansions,
                                            &mut schema_omitted,
                                        )? {
                                            schema_count += 1;
                                        }
                                    }
                                }
                            }
                            Some("route-dependency") => {
                                if boundary_counts.execution_dependencies
                                    >= EXECUTION_DEPENDENCY_FACT_LIMIT
                                {
                                    boundary_counts.execution_dependencies_omitted += 1;
                                    continue;
                                }
                                boundary_counts.execution_dependencies += 1;
                                if child["authentication_candidate"] == true {
                                    boundary_counts.authentication_candidates += 1;
                                }
                                let id = child_id("frfed1", &self.revision, &route_id, child)?;
                                rows.push(fact(
                                    "execution-dependency",
                                    id,
                                    Some(&route_id),
                                    json!({
                                        "path": route["path"],
                                        "line": child["line"],
                                        "handle": route["file_handle"],
                                    }),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        child["status"].clone(),
                                        child["confidence"].clone(),
                                        child["gaps"].clone(),
                                    ),
                                    "execution_dependency",
                                    json!({
                                        "binding": child["binding"],
                                        "provider": child["provider"],
                                        "marker": child["marker"],
                                        "scope": child["scope"],
                                        "authentication_candidate": child["authentication_candidate"],
                                    }),
                                ));
                            }
                            Some("route-service-dependency") => {
                                boundary_counts.service_dependencies += 1;
                                let id = child_id("frfsd1", &self.revision, &route_id, child)?;
                                rows.push(fact(
                                    "service-dependency",
                                    id,
                                    Some(&route_id),
                                    json!({
                                        "path": route["path"],
                                        "line": child["line"],
                                        "handle": route["file_handle"],
                                    }),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        child["status"].clone(),
                                        child["confidence"].clone(),
                                        child["gaps"].clone(),
                                    ),
                                    "service_dependency",
                                    json!({
                                        "transport": child["transport"],
                                        "method": child["method"],
                                        "target": child["target"],
                                        "target_kind": child["target_kind"],
                                        "query_or_fragment_omitted": child["query_or_fragment_omitted"],
                                        "credentials_omitted": child["credentials_omitted"],
                                    }),
                                ));
                            }
                            Some("route-service-gap") => {
                                boundary_counts.service_gaps += 1;
                                let id = child_id("frfsg1", &self.revision, &route_id, child)?;
                                rows.push(fact(
                                    "service-gap",
                                    id,
                                    Some(&route_id),
                                    json!({
                                        "path": route["path"],
                                        "line": child["line"],
                                        "handle": route["file_handle"],
                                    }),
                                    FactEvidence::new(
                                        child["basis"].clone(),
                                        json!("gap"),
                                        Value::Null,
                                        json!([child["reason"]]),
                                    ),
                                    "gap",
                                    json!({"reason": child["reason"]}),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let include_global = options.feature.is_none();
        if include_global {
            for (path, reason) in frontend_gaps {
                boundary_counts.component_gaps += 1;
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id(
                        "frfg1",
                        &self.revision,
                        "frontend-page",
                        &json!([path, reason]),
                    )?,
                    None,
                    self.file_source(&path, 1),
                    FactEvidence::new(
                        json!("nextjs-page-convention"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
        }
        for route in items.iter().filter(|row| row["kind"] == "route") {
            let framework = route["framework_candidate"].as_str().unwrap_or("unknown");
            if matches!(framework, "nextjs-app" | "fastapi") {
                continue;
            }
            unsupported_framework_routes += 1;
            if include_global {
                let reason =
                    format!("The route-centered feature hierarchy does not model {framework} yet.");
                let gap = json!({"framework": framework, "route": route["id"], "reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id("frfg1", &self.revision, "unsupported-framework", &gap)?,
                    None,
                    source(route),
                    FactEvidence::new(
                        json!("unsupported-framework-reader"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    gap,
                ));
            }
        }
        for gap in items
            .iter()
            .filter(|row| matches!(row["kind"].as_str(), Some("analysis-gap" | "coverage-gap")))
        {
            let path_matches = gap["path"]
                .as_str()
                .is_some_and(|path| selected_paths.contains(path));
            if include_global || path_matches {
                let id = child_id("frfg1", &self.revision, "root", gap)?;
                rows.push(fact(
                    "framework-gap",
                    id,
                    None,
                    json!({"path": gap["path"], "line": gap["line"], "handle": null}),
                    FactEvidence::new(
                        gap.get("basis")
                            .cloned()
                            .unwrap_or_else(|| json!("framework-coverage")),
                        json!("gap"),
                        Value::Null,
                        json!([gap["reason"]]),
                    ),
                    "gap",
                    json!({"reason": gap["reason"], "language": gap["language"], "files": gap["files"]}),
                ));
            }
        }
        if source_omitted > 0 {
            let gap = json!({"reason": "The bounded route and contract source page omitted facts; narrow TARGET before relying on completeness.", "omitted": source_omitted});
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, "source-limit", &gap)?,
                None,
                json!({"path": null, "line": null, "handle": null}),
                FactEvidence::new(
                    json!("source-fact-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([gap["reason"]]),
                ),
                "gap",
                gap,
            ));
        }
        if boundary_counts.execution_dependencies_omitted > 0 {
            let reason = "The execution dependency fact limit omitted dependencies; narrow TARGET before relying on completeness.";
            let gap = json!({"reason": reason, "omitted": boundary_counts.execution_dependencies_omitted});
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, "execution-dependency-limit", &gap)?,
                None,
                json!({"path": null, "line": null, "handle": null}),
                FactEvidence::new(
                    json!("execution-dependency-fact-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                gap,
            ));
        }
        if boundary_counts.components_omitted > 0 || boundary_counts.component_details_omitted > 0 {
            let reason = "The component fact limits omitted components or details; narrow TARGET before relying on completeness.";
            let gap = json!({
                "reason": reason,
                "components_omitted": boundary_counts.components_omitted,
                "component_details_omitted": boundary_counts.component_details_omitted,
            });
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, "component-limit", &gap)?,
                None,
                json!({"path": null, "line": null, "handle": null}),
                FactEvidence::new(
                    json!("component-fact-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                gap,
            ));
        }

        let mut analysis = json!({
            "applications": application_count,
            "features": feature_count,
            "routes": route_count,
            "handlers": handler_count,
            "contract_fields": contract_count,
            "schemas": schema_count,
            "packages": package_counts.packages,
            "build_settings": package_counts.build_settings,
            "dependencies": package_counts.dependencies,
            "package_gaps": package_counts.gaps,
            "build_setting_limit": BUILD_SETTING_LIMIT,
            "build_settings_omitted": package_counts.build_settings_omitted,
            "dependency_fact_limit": DEPENDENCY_FACT_LIMIT,
            "dependencies_omitted": package_counts.dependencies_omitted,
            "source_fact_limit": SOURCE_FACT_LIMIT,
            "source_facts_omitted": source_omitted,
            "schema_expansion_limit": SCHEMA_EXPANSION_LIMIT,
            "schema_expansions_omitted": schema_omitted,
            "unsupported_framework_routes": unsupported_framework_routes,
            "readers": ["nextjs-app", "fastapi"],
            "feature_selection": options.feature,
            "certainty": "Applications and features are candidates. Backend and component facts retain their source reader evidence.",
            "limitations": [
                "Feature identity groups exact backend or page paths within an inferred application boundary; business ownership remains unchecked.",
                "Middleware and FastAPI parameter dependencies preserve recognized syntax and order evidence; runtime behavior remains unchecked.",
                "Only FastAPI Security markers are authentication candidates; the purpose of Depends providers remains unknown.",
                "Lifecycle hooks preserve supported declarations; execution, resource effects and runtime selection remain unchecked.",
                "Runtime configuration preserves environment declaration and accessor evidence; values, precedence and deployment identity remain unchecked.",
                "Outbound HTTP calls preserve sanitized literal targets; receiver identity, request options, response use and runtime reachability remain unchecked.",
                "Next.js React function components preserve bounded props, state, effects, events, styles and render edges; runtime rendering remains unchecked.",
                "Schema expansion follows bounded same-file type-name candidates and preserves ambiguity."
            ],
        });
        analysis["middleware"] = json!(boundary_counts.middleware);
        analysis["middleware_gaps"] = json!(boundary_counts.middleware_gaps);
        analysis["middleware_fact_limit"] = json!(MIDDLEWARE_FACT_LIMIT);
        analysis["middleware_omitted"] = json!(boundary_counts.middleware_omitted);
        analysis["lifecycle_hooks"] = json!(boundary_counts.lifecycle_hooks);
        analysis["lifecycle_gaps"] = json!(boundary_counts.lifecycle_gaps);
        analysis["lifecycle_fact_limit"] = json!(LIFECYCLE_FACT_LIMIT);
        analysis["lifecycle_omitted"] = json!(boundary_counts.lifecycle_omitted);
        analysis["execution_dependencies"] = json!(boundary_counts.execution_dependencies);
        analysis["execution_dependency_fact_limit"] = json!(EXECUTION_DEPENDENCY_FACT_LIMIT);
        analysis["execution_dependencies_omitted"] =
            json!(boundary_counts.execution_dependencies_omitted);
        analysis["authentication_candidates"] = json!(boundary_counts.authentication_candidates);
        analysis["runtime_configurations"] = json!(boundary_counts.runtime_configurations);
        analysis["configuration_consumers"] = json!(boundary_counts.configuration_consumers);
        analysis["configuration_gaps"] = json!(boundary_counts.configuration_gaps);
        analysis["configuration_fact_limit"] = json!(CONFIGURATION_FACT_LIMIT);
        analysis["configuration_consumer_limit"] = json!(CONFIGURATION_CONSUMER_LIMIT);
        analysis["configurations_omitted"] = json!(boundary_counts.configurations_omitted);
        analysis["configuration_consumers_omitted"] =
            json!(boundary_counts.configuration_consumers_omitted);
        analysis["service_dependencies"] = json!(boundary_counts.service_dependencies);
        analysis["service_gaps"] = json!(boundary_counts.service_gaps);
        analysis["components"] = json!(boundary_counts.components);
        analysis["component_details"] = json!(boundary_counts.component_details);
        analysis["component_properties"] = json!(boundary_counts.component_properties);
        analysis["component_states"] = json!(boundary_counts.component_states);
        analysis["component_effects"] = json!(boundary_counts.component_effects);
        analysis["component_hooks"] = json!(boundary_counts.component_hooks);
        analysis["component_events"] = json!(boundary_counts.component_events);
        analysis["component_styles"] = json!(boundary_counts.component_styles);
        analysis["render_edges"] = json!(boundary_counts.render_edges);
        analysis["component_gaps"] = json!(boundary_counts.component_gaps);
        analysis["component_fact_limit"] = json!(COMPONENT_FACT_LIMIT);
        analysis["component_detail_fact_limit"] = json!(COMPONENT_DETAIL_FACT_LIMIT);
        analysis["component_file_limit"] = json!(COMPONENT_FILE_LIMIT);
        analysis["component_file_gap_limit"] = json!(COMPONENT_FILE_GAP_LIMIT);
        analysis["components_omitted"] = json!(boundary_counts.components_omitted);
        analysis["component_details_omitted"] = json!(boundary_counts.component_details_omitted);
        let mut result = self.relationship_page(
            "features",
            selected,
            &options.selection,
            None,
            rows,
            analysis,
        )?;
        result["scope"] = json!("Bounded route and page centered application feature candidates for Next.js App Router and FastAPI. Parent IDs form the hierarchy within this revision.");
        Ok(result)
    }
}
