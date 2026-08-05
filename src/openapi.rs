//! An OpenAPI document derived from a Next.js route tree.
//!
//! # Why this exists
//!
//! A rewrite from one framework to another has to preserve the **contract** — the URLs,
//! the methods, the path parameters and the shapes — and nothing else in this tool can
//! check that it did. `fr translate <route> fastapi` preserves what it can see and
//! reports the rest, which is not the same as a check: the failure it cannot catch is a
//! contract that quietly got *smaller*, and a smaller contract looks exactly like a
//! correct one.
//!
//! A check needs two documents to diff. FastAPI produces one from the finished service
//! (`/openapi.json`); this produces the other from the Next.js tree, **before** the
//! rewrite. Diff them and every difference is a defect until argued otherwise.
//!
//! # What it is honest about
//!
//! This is derived from what a Next.js route *declares*, and Next.js declares less than
//! FastAPI does. Specifically:
//!
//! - **Paths, methods and path parameters** are exact. They come from the tree, which
//!   is where a Next.js route's URL lives.
//! - **Schemas** are as good as the declaration: an exported `interface` or a zod
//!   schema. A body validated by hand appears nowhere and cannot.
//! - **Responses** are `default` only. Which status an endpoint returns is a fact about
//!   its code, not its declaration, and inventing `200` for everything would be writing
//!   fiction into the file you are about to diff against.
//!
//! Everything the document could not determine is in [`Baseline::notes`] rather than
//! guessed at, because a baseline that quietly invents an entry is worse than no
//! baseline: the diff comes out clean and the contract still shrank.

use crate::transpile::ir::Type;
use crate::transpile::nextjs::{self, Model};
use anyhow::Result;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// An OpenAPI document, and what could not be put in it.
pub struct Baseline {
    pub document: Value,
    /// One line per thing a reader must not take the document to have settled.
    pub notes: Vec<String>,
    /// The route files it was built from.
    pub routes: Vec<PathBuf>,
}

/// Build a baseline from every Next.js API route under `files`.
pub fn from_routes(title: &str, root: &Path, files: &[PathBuf]) -> Result<Baseline> {
    let mut paths: Map<String, Value> = Map::new();
    let mut schemas: Map<String, Value> = Map::new();
    let mut notes = Vec::new();
    let mut routes = Vec::new();

    // Every shape the tree declares, wherever it is declared.
    //
    // A real Next.js application keeps its zod schemas in a module the routes import —
    // `@/lib/schemas` here — and reading only the route file found none of them. The
    // contract came out with an empty `components` section, which says the endpoints
    // take no body at all: a smaller contract than the one it stands in for, and
    // exactly the failure this document exists to catch.
    let mut declared: std::collections::BTreeMap<String, Model> = std::collections::BTreeMap::new();
    for file in files {
        for model in nextjs::models_in(file).unwrap_or_default() {
            declared.entry(model.name.clone()).or_insert(model);
        }
    }

    for file in files.iter().filter(|f| nextjs::is_api_route(f)) {
        let plan = match nextjs::plan(file) {
            Ok(plan) => plan,
            Err(e) => {
                // A route this cannot read is a hole in the baseline, and the diff it
                // is for would silently pass over it.
                notes.push(format!("{}: not read — {e}", relative(root, file)));
                continue;
            }
        };
        routes.push(file.clone());

        for model in &plan.models {
            schemas.insert(model.name.clone(), schema_of(model, &mut notes));
        }
        for (_, name) in &plan.bodies {
            if let Some(model) = declared.get(name) {
                schemas.insert(model.name.clone(), schema_of(model, &mut notes));
            }
        }

        let entry = paths
            .entry(plan.route.clone())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("a path item is an object");

        let parameters: Vec<Value> = nextjs::path_parameters(&plan.route)
            .into_iter()
            .map(|name| {
                json!({
                    "name": name,
                    "in": "path",
                    "required": true,
                    // Every Next.js path segment arrives as text; a narrower type would
                    // be a guess about what the handler does with it.
                    "schema": { "type": "string" }
                })
            })
            .collect();

        for method in &plan.methods {
            // The path parameters, plus whatever this handler reads out of the query.
            // Next.js declares neither; the path ones come from the tree and the query
            // ones from the handler reaching into the URL.
            let mut all = parameters.clone();
            for (_, name) in plan.queries.iter().filter(|(m, _)| m == method) {
                all.push(json!({
                    "name": name,
                    "in": "query",
                    // Nothing says it is required: a handler that defaults it and a
                    // handler that rejects the request without it read the same way.
                    "required": false,
                    "schema": { "type": "string" }
                }));
            }
            let mut operation = json!({
                "operationId": format!("{}{}", method.to_lowercase(), operation_suffix(&plan.route)),
                "parameters": all,
                "responses": {
                    "default": { "description": "not declared by the source" }
                }
            });
            // The body the handler validates, linked to the operation that validates
            // it. A `components` section nothing refers to is not a contract.
            if let Some((_, schema)) = plan.bodies.iter().find(|(m, _)| m == method) {
                match declared.contains_key(schema) || plan.models.iter().any(|m| m.name == *schema)
                {
                    true => {
                        operation["requestBody"] = json!({
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{schema}") }
                                }
                            }
                        });
                    }
                    false => notes.push(format!(
                        "{}: {method} validates its body with `{schema}`, which is declared \
                         nowhere this document can see — the body is not in the contract",
                        relative(root, file)
                    )),
                }
            }
            entry.insert(method.to_lowercase(), operation);
        }

        if !plan.models.is_empty() {
            notes.push(format!(
                "{}: declares {} — which operation consumes it is not written down, so it \
                 is in `components` and referenced by nothing",
                relative(root, file),
                plan.models
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // A handler this could not read whole may be reaching into the URL in the part
        // it could not read. `Number(req.nextUrl.searchParams.get("limit") ?? "50")`
        // uses `??`, which the IR has no node for, so the statement is carried verbatim
        // — and `limit` never reaches this document. Saying so is the difference
        // between a contract with a gap and a contract that looks complete.
        if plan.fidelity.carried_verbatim > 0 {
            notes.push(format!(
                "{}: {} statement(s) could not be read; any query parameter read inside \
                 one of them is missing from this document",
                relative(root, file),
                plan.fidelity.carried_verbatim
            ));
        }

        let statuses: Vec<String> = plan
            .fidelity
            .notes
            .iter()
            .filter(|note| note.starts_with("returns status "))
            .cloned()
            .collect();
        for status in statuses {
            notes.push(format!("{}: {status}", relative(root, file)));
        }
    }

    let document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": title,
            "version": "0.0.0",
            "description":
                "Derived from a Next.js route tree by fun-refactor. Paths, methods and \
                 path parameters are exact; schemas are as good as what the source \
                 declared; responses are not declared by the source and are `default` \
                 here rather than invented. See the notes beside it.",
        },
        "paths": Value::Object(paths),
        "components": { "schemas": Value::Object(schemas) },
    });

    Ok(Baseline {
        document,
        notes,
        routes,
    })
}

fn relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}

/// A suffix that makes an operation id unique without inventing a name.
fn operation_suffix(route: &str) -> String {
    let mut out = String::new();
    for part in route.split('/') {
        let cleaned: String = part
            .trim_matches(|c| c == '{' || c == '}')
            .split(':')
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        let mut chars = cleaned.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

fn schema_of(model: &Model, notes: &mut Vec<String>) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, ty) in &model.fields {
        let Some(ty) = ty else {
            notes.push(format!(
                "{}.{name}: no declared type, so it is `{{}}` — anything at all",
                model.name
            ));
            properties.insert(name.clone(), json!({}));
            continue;
        };
        // Optional says the field may be absent, which OpenAPI spells by leaving it out
        // of `required` rather than by changing its type.
        let optional = matches!(ty, Type::Optional(_));
        if !optional {
            required.push(Value::String(name.clone()));
        }
        properties.insert(name.clone(), json_type(ty));
    }
    let mut schema = json!({ "type": "object", "properties": Value::Object(properties) });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}

/// The IR's type, as OpenAPI spells it.
fn json_type(ty: &Type) -> Value {
    match ty {
        Type::String => json!({ "type": "string" }),
        Type::Int => json!({ "type": "integer" }),
        Type::Float => json!({ "type": "number" }),
        Type::Bool => json!({ "type": "boolean" }),
        Type::Unit => json!({ "type": "null" }),
        Type::List(inner) => json!({ "type": "array", "items": json_type(inner) }),
        Type::Map(_, value) => {
            json!({ "type": "object", "additionalProperties": json_type(value) })
        }
        Type::Optional(inner) => json_type(inner),
        Type::Named { name, .. } => match name.as_str() {
            "datetime" => json!({ "type": "string", "format": "date-time" }),
            // A type this tool does not know is not a type OpenAPI can be told about.
            // `{}` is "anything", which is true, rather than a guess that is not.
            _ => json!({}),
        },
    }
}
