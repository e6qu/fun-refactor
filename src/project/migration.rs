use super::{bounded_text, FeatureOptions, Project, RelationshipOptions};
use crate::edit::EditSet;
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
    pub report: Value,
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
        Type::Int => "int".to_owned(),
        Type::Float => "float".to_owned(),
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

        let (
            edits,
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
                .chain(translator_notes),
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
            ))?[..32]
        );
        let endpoints = semantic_endpoints
            .iter()
            .map(|(method, url)| json!({"method": method, "url": url}))
            .collect::<Vec<_>>();
        let mut report = self.envelope("migration");
        report["migration"] = json!({
            "id": migration_id,
            "feature": options.feature,
            "source_framework": source_framework,
            "target_framework": options.to,
            "source_files": source_paths,
            "destination_files": destinations_relative,
            "coexistence": {
                "source_retained": true,
                "destination_added": true,
                "cutover_applied": false,
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
            "automatic": [{
                "order": 1,
                "action": "translate-selected-route-file",
                "validation": ["selected-feature-revision", "semantic-translation-endpoint-agreement", "declared-schema-translation-agreement", "reparse-strict"],
            }],
            "agent_decisions": [
                {
                    "order": 2,
                    "action": "register-destination-with-application",
                    "reason": "Application composition and runtime registration remain project-specific.",
                },
                {
                    "order": 3,
                    "action": "cut-over-and-remove-source-route",
                    "reason": "The preview keeps both frameworks available until independent behavior checks pass.",
                }
            ],
            "unsupported": unsupported,
        });
        report["facts"] = json!(facts);
        report["translation"] = fidelity;
        report["scope"] = json!("One route-centered feature whose methods share one source file. Source removal and runtime registration require later reviewed steps.");
        Ok(Plan { edits, report })
    }
}
