use super::{bounded_text, FeatureOptions, Project, RelationshipOptions};
use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::transpile::ir::{Item, Record, Type};
use crate::transpile::nextjs::Model;
use anyhow::{ensure, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

const FEATURE_FACT_LIMIT: usize = 500;

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Plan or apply one revision-bound framework feature migration.")]
    Feature(Options),
}

#[derive(Args)]
pub struct Options {
    #[arg(help = "Feature ID returned by `fr project features`.")]
    pub feature: String,
    #[arg(long, value_enum, help = "Destination framework.")]
    pub to: Target,
    #[arg(
        long,
        help = "Workspace-relative destination file for FastAPI or directory for Next.js."
    )]
    pub out: PathBuf,
    #[arg(
        long,
        help = "Mount generated FastAPI routes in an explicit PATH::APP_SYMBOL target."
    )]
    pub register_with: Option<String>,
    #[arg(
        long,
        help = "Remove the source route in the same transaction after registration checks."
    )]
    pub cutover: bool,
    #[arg(
        long,
        default_value_t = 4096,
        help = "Maximum UTF-8 diff bytes, from 0 through 65536."
    )]
    pub diff_bytes: usize,
    #[arg(long, help = "Record and apply the migration through source history.")]
    pub write: bool,
}

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    Fastapi,
    Nextjs,
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::Fastapi => "fastapi",
            Self::Nextjs => "nextjs",
        }
    }

    fn is_fastapi(self) -> bool {
        matches!(self, Self::Fastapi)
    }
}

pub struct Plan {
    pub edits: EditSet,
    pub source_removal: Option<SourceRemoval>,
    pub report: Value,
}

pub struct SourceRemoval {
    pub path: PathBuf,
    pub original: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct SchemaField {
    name: String,
    declared_type: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct SchemaShape {
    name: String,
    fields: Vec<SchemaField>,
}

impl Plan {
    pub fn set_diff(&mut self, diff: &str, budget: usize) {
        self.report["diff"] = bounded_text(diff, budget);
    }
}

fn destination(root: &Path, out: &Path) -> Result<PathBuf> {
    ensure!(
        !out.as_os_str().is_empty()
            && !out.is_absolute()
            && out
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "migration output must be one normalized workspace-relative path."
    );
    Ok(root.join(out))
}

struct FastapiRegistration {
    path: PathBuf,
    report: Value,
    note: String,
}

fn registration_selector(root: &Path, selector: &str) -> Result<(PathBuf, String)> {
    let (path, application) = selector
        .rsplit_once("::")
        .ok_or_else(|| anyhow::anyhow!("registration target must use PATH::APP_SYMBOL."))?;
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && path.extension().is_some_and(|extension| extension == "py")
            && super::contracts::simple_name(application),
        "registration target must name one normalized Python PATH::APP_SYMBOL."
    );
    Ok((root.join(path), application.to_owned()))
}

fn python_module_path(out: &Path) -> Result<String> {
    let module_path = out.with_extension("");
    let mut parts = module_path
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("FastAPI output path is not UTF-8."))?;
    if parts.last() == Some(&"__init__") {
        parts.pop();
    }
    ensure!(
        !parts.is_empty() && parts.iter().all(|part| super::contracts::simple_name(part)),
        "FastAPI output must have Python identifier path components for registration."
    );
    Ok(parts.join("."))
}

fn top_import_offset(root: tree_sitter::Node<'_>) -> usize {
    let mut offset = 0;
    for node in super::fast_routes::children(root) {
        let module_doc = offset == 0
            && node.kind() == "expression_statement"
            && node
                .named_child(0)
                .is_some_and(|child| child.kind() == "string");
        if module_doc
            || matches!(
                node.kind(),
                "import_statement" | "import_from_statement" | "future_import_statement"
            )
        {
            offset = node.end_byte();
        } else if node.kind() != "comment" {
            break;
        }
    }
    offset
}

fn application_assignment_end(
    root: tree_sitter::Node<'_>,
    source: &str,
    application: &str,
) -> Option<usize> {
    super::fast_routes::children(root)
        .into_iter()
        .filter(|node| node.kind() == "expression_statement")
        .filter_map(|statement| {
            let assignment = statement
                .named_children(&mut statement.walk())
                .find(|node| node.kind() == "assignment")?;
            let left = assignment.child_by_field_name("left")?;
            (left.kind() == "identifier" && super::fast_routes::text(left, source) == application)
                .then_some(statement.end_byte())
        })
        .next()
}

fn fresh_registration_alias(source: &str) -> String {
    let mut alias = "fr_migrated_router".to_owned();
    while source.contains(&alias) {
        alias.push('_');
    }
    alias
}

fn add_fastapi_registration(
    project: &Project<'_>,
    edits: &mut EditSet,
    selector: &str,
    out: &Path,
    source: &Path,
    endpoints: &[(String, String)],
) -> Result<FastapiRegistration> {
    let (path, application) = registration_selector(&project.root, selector)?;
    ensure!(
        path != source && path != project.root.join(out),
        "registration target must be separate from the source and generated route."
    );
    let text = project.sources.get(&path).ok_or_else(|| {
        anyhow::anyhow!("registration target is not a captured project source file.")
    })?;
    let parsed = crate::parse::Parsers::new().parse(Language::Python, text)?;
    ensure!(
        !parsed.has_errors(),
        "registration target does not parse cleanly as Python."
    );
    let application_binding = super::fast_routes::receivers(&parsed, text)
        .get(&application)
        .is_some_and(|router| !router);
    let routes = super::fast_routes::read(&parsed, text);
    let endpoint_conflict = routes.entries.iter().any(|route| {
        endpoints
            .iter()
            .any(|(method, url)| route.endpoint.method == *method && route.endpoint.url == *url)
    });
    ensure!(
        crate::project::framework_kernel::fastapi_registration_automatic(
            true,
            application_binding,
            endpoint_conflict,
        ),
        "registration target must contain one recognized FastAPI application binding and no direct endpoint conflict."
    );
    let assignment_end =
        application_assignment_end(parsed.root(), text, &application).ok_or_else(|| {
            anyhow::anyhow!("recognized FastAPI application assignment is unavailable.")
        })?;
    let import_offset = top_import_offset(parsed.root());
    let module = python_module_path(out)?;
    let alias = fresh_registration_alias(text);
    let import = if import_offset == 0 {
        format!("from {module} import router as {alias}\n")
    } else {
        format!("\nfrom {module} import router as {alias}")
    };
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(import_offset, import_offset),
            import,
            "Import the generated FastAPI router.",
        ),
    );
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(assignment_end, assignment_end),
            format!("\n{application}.include_router({alias})"),
            "Mount the generated FastAPI router.",
        ),
    );
    let relative = path.strip_prefix(&project.root).unwrap_or(&path);
    Ok(FastapiRegistration {
        path: relative.to_path_buf(),
        report: json!({
            "framework": "fastapi",
            "root": relative.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new(".")),
            "entrypoint": relative,
            "application": application,
            "basis": "explicit-recognized-fastapi-binding-and-direct-route-check",
        }),
        note: "FastAPI registration checks direct routes in the selected application file; included and mounted routers remain unchecked.".to_owned(),
    })
}

fn text_set(values: impl Iterator<Item = String>) -> Vec<String> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}

fn endpoint_set(items: &[Value], feature: &str) -> Vec<(String, String)> {
    items
        .iter()
        .filter_map(|row| {
            (row["kind"] == "route" && row["parent"] == feature).then(|| {
                Some((
                    row["route"]["method"].as_str()?.to_owned(),
                    row["route"]["url"].as_str()?.to_owned(),
                ))
            })?
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "unit".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Int | Type::Float => "number".to_owned(),
        Type::String => "string".to_owned(),
        Type::List(inner) => format!("list<{}>", canonical_type(inner)),
        Type::Set(inner) => format!("set<{}>", canonical_type(inner)),
        Type::Map(key, value) => {
            format!("map<{},{}>", canonical_type(key), canonical_type(value))
        }
        Type::Optional(inner) => format!("optional<{}>", canonical_type(inner)),
        Type::Tuple(parts) => format!(
            "tuple<{}>",
            parts
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Named { name, args } if args.is_empty() => format!("named<{name}>"),
        Type::Named { name, args } => format!(
            "named<{name};{}>",
            args.iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Fn { params, returns } => format!(
            "fn<{};{}>",
            params
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(","),
            canonical_type(returns)
        ),
    }
}

fn schema_shape<'a>(
    name: &str,
    fields: impl Iterator<Item = (&'a str, &'a Option<Type>)>,
) -> SchemaShape {
    let mut fields = fields
        .map(|(name, ty)| SchemaField {
            name: name.to_owned(),
            declared_type: ty.as_ref().map(canonical_type),
        })
        .collect::<Vec<_>>();
    fields.sort();
    SchemaShape {
        name: name.to_owned(),
        fields,
    }
}

fn model_shapes(models: &[Model]) -> Vec<SchemaShape> {
    let mut shapes = models
        .iter()
        .map(|model| {
            schema_shape(
                &model.name,
                model.fields.iter().map(|(name, ty)| (name.as_str(), ty)),
            )
        })
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.dedup();
    shapes
}

fn record_shapes(records: &[Record]) -> Vec<SchemaShape> {
    let mut shapes = records
        .iter()
        .map(|record| {
            schema_shape(
                &record.name,
                record
                    .fields
                    .iter()
                    .map(|field| (field.name.as_str(), &field.ty)),
            )
        })
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.dedup();
    shapes
}

fn generated_shapes(language: Language, outputs: &[String]) -> Result<Vec<SchemaShape>> {
    let parsers = crate::parse::Parsers::new();
    let mut records = Vec::new();
    for output in outputs {
        let parsed = parsers.parse(language, output)?;
        ensure!(
            !parsed.has_errors(),
            "generated migration output failed independent schema parsing."
        );
        let module = crate::transpile::read_module(language, output, parsed.root())?;
        records.extend(module.items.into_iter().filter_map(|item| match item {
            Item::Record(record) => Some(record),
            _ => None,
        }));
    }
    Ok(record_shapes(&records))
}

fn schema_agreement(expected: &[SchemaShape], generated: &[SchemaShape]) -> bool {
    let keys = |shapes: &[SchemaShape]| {
        shapes
            .iter()
            .map(|shape| serde_json::to_string(shape).expect("schema shape is JSON data"))
            .collect::<Vec<_>>()
    };
    crate::project::framework_kernel::migration_schema_agreement(&keys(expected), &keys(generated))
}

fn automatic_kind(kind: &str) -> bool {
    matches!(
        kind,
        "application"
            | "feature"
            | "route"
            | "handler"
            | "contract-field"
            | "schema-reference"
            | "schema-candidate"
            | "schema"
            | "schema-field"
    )
}

fn fact_disposition(row: &Value) -> &'static str {
    let kind = row["kind"].as_str().unwrap_or("unknown");
    match crate::project::framework_kernel::migration_disposition(
        row["status"] == "gap" || kind.ends_with("-gap"),
        automatic_kind(kind),
    ) {
        0 => "automatic",
        1 => "agent-decision",
        _ => "unsupported",
    }
}

fn nextjs_target_application(project: &Project<'_>, out: &Path) -> Option<Value> {
    let parent = out.parent()?;
    let mut roots = vec![parent];
    if parent.file_name().is_some_and(|name| name == "src") {
        roots.push(parent.parent().unwrap_or(Path::new("")));
    }
    for root in roots {
        let app_path = out.strip_prefix(root).ok()?;
        let app_router_path = app_path == Path::new("app") || app_path == Path::new("src/app");
        if !app_router_path {
            continue;
        }
        let manifest = root.join("package.json");
        let Some(document) = project.manifests.documents.get(&manifest) else {
            continue;
        };
        let declares_next = ["dependencies", "devDependencies"].iter().any(|section| {
            document[*section]["next"]
                .as_str()
                .is_some_and(|version| !version.trim().is_empty())
        });
        if !crate::project::framework_kernel::nextjs_registration_automatic(
            declares_next,
            app_router_path,
        ) {
            return None;
        }
        return Some(json!({
            "framework": "nextjs-app",
            "root": if root.as_os_str().is_empty() { "." } else { root.to_str()? },
            "manifest": manifest,
            "basis": "captured-nextjs-dependency-and-app-router-path",
        }));
    }
    None
}

fn source_has_external_references(project: &Project<'_>, source: &Path) -> bool {
    let Some((_, file)) = project
        .index
        .files()
        .find(|(path, _)| path.as_path() == source)
    else {
        return true;
    };
    file.symbols.iter().any(|symbol| {
        project
            .index
            .references_to(*symbol)
            .into_iter()
            .any(|reference| reference.file != source)
    })
}

impl Project<'_> {
    pub fn migrate_feature(&self, options: &Options) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65_536,
            "diff byte limit must be between 0 and 65536."
        );
        let feature_options = FeatureOptions {
            selection: RelationshipOptions {
                target: ".".to_owned(),
                revision: None,
                limit: FEATURE_FACT_LIMIT,
                cursor: None,
            },
            feature: Some(options.feature.clone()),
        };
        let feature_report = self.features(&feature_options)?;
        ensure!(
            feature_report["page"]["remaining"] == 0,
            "the selected feature exceeds the migration fact limit; narrow it before migrating."
        );
        let items = feature_report["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let feature = items
            .iter()
            .find(|row| row["kind"] == "feature" && row["id"] == options.feature)
            .ok_or_else(|| anyhow::anyhow!("selected feature fact is unavailable."))?;
        let application_id = feature["parent"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("selected feature has no application parent."))?;
        let application = items
            .iter()
            .find(|row| row["kind"] == "application" && row["id"] == application_id)
            .ok_or_else(|| anyhow::anyhow!("selected feature application is unavailable."))?;
        let source_framework = application["application"]["framework"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("selected application has no framework."))?;
        let source_fastapi = source_framework == "fastapi";
        ensure!(
            matches!(source_framework, "fastapi" | "nextjs-app")
                && crate::project::framework_kernel::framework_migration_supported(
                    source_fastapi,
                    options.to.is_fastapi(),
                ),
            "the bounded migration supports Next.js to FastAPI and FastAPI to Next.js only."
        );

        let semantic_endpoints = endpoint_set(&items, &options.feature);
        ensure!(
            !semantic_endpoints.is_empty(),
            "the selected feature has no route contract to migrate."
        );
        let source_paths = text_set(items.iter().filter_map(|row| {
            (row["kind"] == "route" && row["parent"] == options.feature)
                .then(|| row["source"]["path"].as_str().map(str::to_owned))?
        }));
        ensure!(
            source_paths.len() == 1,
            "the bounded migration requires every selected route to share one source file."
        );
        let source = self.root.join(&source_paths[0]);
        let out = destination(&self.root, &options.out)?;
        match options.to {
            Target::Fastapi => ensure!(
                options
                    .out
                    .extension()
                    .is_some_and(|extension| extension == "py"),
                "a FastAPI migration output must name a .py file."
            ),
            Target::Nextjs => ensure!(
                options.out.file_name().is_some_and(|name| name == "app"),
                "a Next.js migration output must name the destination app directory."
            ),
        }
        ensure!(
            options.register_with.is_none() || matches!(options.to, Target::Fastapi),
            "--register-with applies only to a FastAPI destination."
        );

        let (
            mut edits,
            destinations,
            translated_endpoints,
            source_shapes,
            generated_shapes,
            fidelity,
            translator_notes,
        ) = match options.to {
            Target::Fastapi => {
                let translated = crate::transpile::nextjs::plan_to(&source, Some(&out), false)?;
                let source_shapes = model_shapes(&translated.models);
                let generated_shapes =
                    generated_shapes(Language::Python, std::slice::from_ref(&translated.output))?;
                let fidelity = json!({
                    "functions": translated.fidelity.functions,
                    "records": translated.fidelity.records,
                    "carried_verbatim": translated.fidelity.carried_verbatim,
                    "signatures_complete": translated.fidelity.signatures_complete,
                });
                (
                    translated.edits,
                    vec![translated.destination],
                    translated.endpoints,
                    source_shapes,
                    generated_shapes,
                    fidelity,
                    translated.fidelity.notes,
                )
            }
            Target::Nextjs => {
                let translated = crate::transpile::fastapi::plan_to(&source, Some(&out), false)?;
                let endpoints = translated.endpoints;
                let source_shapes = record_shapes(&translated.models);
                let outputs = translated
                    .routes
                    .iter()
                    .map(|route| route.output.clone())
                    .collect::<Vec<_>>();
                let generated_shapes = generated_shapes(Language::TypeScript, &outputs)?;
                let destinations = translated
                    .routes
                    .iter()
                    .map(|route| route.destination.clone())
                    .collect();
                let fidelity = json!({
                    "functions": translated.fidelity.functions,
                    "records": translated.fidelity.records,
                    "carried_verbatim": translated.fidelity.carried_verbatim,
                    "signatures_complete": translated.fidelity.signatures_complete,
                });
                let mut notes = translated.notes;
                notes.extend(translated.fidelity.notes);
                (
                    translated.edits,
                    destinations,
                    endpoints,
                    source_shapes,
                    generated_shapes,
                    fidelity,
                    notes,
                )
            }
        };

        let translated_endpoints = translated_endpoints
            .into_iter()
            .map(|(method, url)| (method.to_uppercase(), url))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ensure!(
            translated_endpoints == semantic_endpoints,
            "the file translator and selected feature disagree about the route contract."
        );
        ensure!(
            schema_agreement(&source_shapes, &generated_shapes),
            "the generated destination disagrees with the translated declared schema shapes."
        );
        ensure!(
            destinations.iter().all(|path| path != &source),
            "migration must keep the source route while the destination coexists."
        );
        let registration = options
            .register_with
            .as_deref()
            .map(|selector| {
                add_fastapi_registration(
                    self,
                    &mut edits,
                    selector,
                    &options.out,
                    &source,
                    &semantic_endpoints,
                )
            })
            .transpose()?;
        let target_application = match options.to {
            Target::Nextjs => nextjs_target_application(self, &options.out),
            Target::Fastapi => registration
                .as_ref()
                .map(|registration| registration.report.clone()),
        };
        let registration_automatic = target_application.is_some();
        let external_references = source_has_external_references(self, &source);
        ensure!(
            !options.cutover
                || crate::project::framework_kernel::migration_cutover_automatic(
                    true,
                    registration_automatic,
                    external_references,
                ),
            "--cutover requires automatic destination registration and no resolved external source references."
        );
        let source_removal = options.cutover.then(|| SourceRemoval {
            path: source.clone(),
            original: self.sources[&source].clone(),
        });

        let facts = items
            .iter()
            .map(|row| {
                json!({
                    "id": row["id"],
                    "kind": row["kind"],
                    "disposition": fact_disposition(row),
                    "source": row["source"],
                })
            })
            .collect::<Vec<_>>();
        let unsupported = text_set(
            items
                .iter()
                .flat_map(|row| row["gaps"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .chain(translator_notes)
                .chain(
                    registration
                        .iter()
                        .map(|registration| registration.note.clone()),
                ),
        );
        let destinations_relative = destinations
            .iter()
            .map(|path| path.strip_prefix(&self.root).unwrap_or(path))
            .collect::<Vec<_>>();
        let migration_id = format!(
            "frfm1:{}",
            &super::hash((
                &self.revision,
                &options.feature,
                options.to.name(),
                &options.out,
                &options.register_with,
                options.cutover,
            ))?[..32]
        );
        let endpoints = semantic_endpoints
            .iter()
            .map(|(method, url)| json!({"method": method, "url": url}))
            .collect::<Vec<_>>();
        let mut automatic_steps = vec![json!({
            "order": 1,
            "action": "translate-selected-route-file",
            "validation": ["selected-feature-revision", "semantic-translation-endpoint-agreement", "declared-schema-translation-agreement", "reparse-strict"],
        })];
        let mut agent_decisions = Vec::new();
        if registration_automatic {
            automatic_steps.push(match options.to {
                Target::Nextjs => json!({
                    "order": 2,
                    "action": "register-nextjs-route-by-app-router-placement",
                    "validation": ["captured-nextjs-dependency", "app-or-src-app-destination"],
                }),
                Target::Fastapi => json!({
                    "order": 2,
                    "action": "register-fastapi-router-with-application",
                    "validation": ["explicit-application-target", "recognized-fastapi-binding", "no-direct-endpoint-conflict", "reparse-strict"],
                }),
            });
        } else {
            agent_decisions.push(json!({
                "order": 2,
                "action": "register-destination-with-application",
                "reason": "No captured target application proves automatic runtime registration.",
            }));
        }
        if options.cutover {
            automatic_steps.push(json!({
                "order": 3,
                "action": "remove-source-route-after-explicit-cutover",
                "validation": ["explicit-cutover", "automatic-destination-registration", "no-resolved-external-source-references", "source-snapshot"],
            }));
        } else {
            agent_decisions.push(json!({
                "order": 3,
                "action": "run-independent-checks-then-cut-over",
                "reason": "Project check results do not carry a durable source-snapshot receipt.",
            }));
        }
        let mut report = self.envelope("migration");
        report["migration"] = json!({
            "id": migration_id,
            "feature": options.feature,
            "source_framework": source_framework,
            "target_framework": options.to,
            "source_files": source_paths,
            "destination_files": destinations_relative,
            "connected_files": registration.iter().map(|registration| &registration.path).collect::<Vec<_>>(),
            "target_application": target_application,
            "coexistence": {
                "source_retained": !options.cutover,
                "destination_added": true,
                "destination_registration": if registration_automatic { "automatic" } else { "agent-decision" },
                "cutover_planned": options.cutover,
                "cutover_applied": false,
                "resolved_external_source_references": external_references,
            },
        });
        report["contract"] = json!({
            "endpoints": endpoints,
            "semantic_translation_agreement": true,
            "declared_schemas": {
                "status": if source_shapes.is_empty() { "not-observed" } else { "agreed" },
                "source": source_shapes,
                "generated": generated_shapes,
                "translation_agreement": true,
                "wire_schema_agreement": "unverified",
            },
        });
        report["steps"] = json!({
            "automatic": automatic_steps,
            "agent_decisions": agent_decisions,
            "unsupported": unsupported,
        });
        report["facts"] = json!(facts);
        report["translation"] = fidelity;
        report["scope"] = json!("One route-centered feature whose methods share one source file. Registered destinations may use an explicit source-removal cutover. Runtime checks and unresolved project connections remain reviewed evidence.");
        Ok(Plan {
            edits,
            source_removal,
            report,
        })
    }
}
