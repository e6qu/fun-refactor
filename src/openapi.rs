//! An OpenAPI document derived from a Next.js route tree.
//!
//! # Why this exists
//!
//! A rewrite from one framework to another has to preserve the **contract**, the URLs, the
//! methods, the path parameters and the shapes. `fr translate <route> fastapi` preserves what
//! it can see and reports the rest, which is not a check. It cannot catch a contract that got
//! smaller, and a smaller contract looks like a correct one.
//!
//! A check needs two documents to diff. FastAPI emits one from the finished service
//! (`/openapi.json`); this emits the other from the Next.js tree, before the rewrite.
//!
//! # Precision
//!
//! Derived from what a Next.js route declares, which is less than FastAPI declares:
//!
//! - **Paths, methods and path parameters**: exact, read from the tree.
//! - **Schemas**: as good as the declaration, an exported `interface` or a zod schema. A body
//!   validated by hand appears nowhere.
//! - **Responses**: `default` only. Which status an endpoint returns is a fact about its code
//!   instead of its declaration.
//!
//! Anything undetermined goes in [`Baseline::notes`]. An invented entry would make the diff
//! come out clean while the contract shrank.

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
    // A real Next.js application keeps its zod schemas in a module the routes import,
    // `@/lib/schemas` here. Reading only the route file found none of them. The contract came
    // out with an empty `components` section. That says the endpoints take no body at all,
    // a smaller contract than the one it stands in for. This document exists to catch
    // that failure.
    let mut declared: std::collections::BTreeMap<String, Model> = std::collections::BTreeMap::new();
    for file in files {
        for model in nextjs::models_in(file).unwrap_or_default() {
            declared.entry(model.name.clone()).or_insert(model);
        }
    }

    for file in files
        .iter()
        .filter(|f| nextjs::is_api_route(f) || nextjs::is_server_module(f))
    {
        let plan = match nextjs::plan(file) {
            Ok(plan) => plan,
            Err(e) => {
                // A route this cannot read is a hole in the baseline, and the diff it
                // is for would silently pass over it.
                notes.push(format!("{} could not be read: {e}", relative(root, file)));
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

        // A server function takes its arguments in the framework's own wire encoding,
        // not a JSON body this document could describe. Writing one would be a guess
        // presented as a contract, so the gap is named instead.
        if nextjs::is_server_module(file) {
            notes.push(format!(
                "{}: server functions take their arguments in the framework's own \
                 encoding. Their payloads are not in this document",
                relative(root, file)
            ));
        }

        // One entry per endpoint. A route file's endpoints share its URL; a server
        // module's each have their own.
        for (method, route) in &plan.endpoints {
            let entry = paths
                .entry(route.clone())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("a path item is an object");

            let parameters: Vec<Value> = nextjs::path_parameters(route)
                .into_iter()
                .map(|name| {
                    json!({
                        "name": name,
                        "in": "path",
                        "required": true,
                        // Every Next.js path segment arrives as text; a narrower type
                        // would be a guess about what the handler does with it.
                        "schema": { "type": "string" }
                    })
                })
                .collect();

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
                "operationId": format!("{}{}", method.to_lowercase(), operation_suffix(route)),
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
                         nowhere this document can see. The body is not in the contract",
                        relative(root, file)
                    )),
                }
            }
            entry.insert(method.to_lowercase(), operation);
        }

        if !plan.models.is_empty() {
            notes.push(format!(
                "{}: declares {}. Which operation consumes it is not written down, so it \
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
        // uses `??`, which the IR has no node for, so the statement is carried verbatim,
        // and `limit` never reaches this document. Saying so is the difference
        // between a contract with a gap and a contract that looks complete.
        if plan.fidelity.carried_verbatim > 0 {
            notes.push(format!(
                "{}: {} statement(s) could not be read; any query parameter read inside \
                 one of them is missing from this document",
                relative(root, file),
                plan.fidelity.carried_verbatim
            ));
        }

        // In the source's own words. The translation's note about these statuses
        // advises adding `status_code=` to a `@router` decorator, which is advice
        // about the FastAPI file it writes. This document describes a Next.js tree,
        // where no such decorator exists, so the note points at the handler instead.
        if !plan.statuses.is_empty() {
            notes.push(format!(
                "{}: returns status {}. Next.js settles a status inside the handler, \
                 by `NextResponse.json(..., {{ status }})` or `new Response(..., \
                 {{ status }})`. The responses here stay `default` and do not carry it.",
                relative(root, file),
                plan.statuses.join(", ")
            ));
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
                 here and not invented. See the notes beside it.",
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
                "{}.{name}: no declared type, so it is `{{}}`, which means anything",
                model.name
            ));
            properties.insert(name.clone(), json!({}));
            continue;
        };
        // Optional says the field may be absent, which OpenAPI spells by leaving it out
        // of `required` and not by changing its type.
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
        // JSON Schema has no set. An array whose members are unique is the
        // nearest thing it says, and it says it.
        Type::Set(inner) => {
            json!({ "type": "array", "items": json_type(inner), "uniqueItems": true })
        }
        Type::Map(_, value) => {
            json!({ "type": "object", "additionalProperties": json_type(value) })
        }
        Type::Optional(inner) => json_type(inner),
        // JSON Schema describes data, and a function is not data. A route
        // taking or returning one has no body this can describe.
        Type::Fn { .. } => json!({}),
        Type::Tuple(parts) => json!({
            "type": "array",
            "prefixItems": parts.iter().map(json_type).collect::<Vec<_>>(),
        }),
        Type::Named { name, .. } => match name.as_str() {
            "datetime" => json!({ "type": "string", "format": "date-time" }),
            // A type this tool does not know is not a type OpenAPI can be told about.
            // `{}` is "anything", which is true, instead of a guess that is not.
            _ => json!({}),
        },
    }
}

/// The contract a FastAPI tree *declares*, read the same way FastAPI reads it.
///
/// The point of a baseline is to be diffed against the finished service, and doing that
/// properly means running the service. This is the check you can make without one. The
/// decorators and the signatures say what the router will answer. Comparing them with the
/// Next.js baseline catches the failure that matters. An endpoint may not survive the
/// crossing, or a path may quietly change shape.
///
/// It reads what is written. It is not what will happen. A route added at run time, a router mounted
/// under a prefix, a dependency that rejects the request: none of those are here. The document
/// says so instead of pretending otherwise.
pub fn from_fastapi(title: &str, root: &Path, files: &[PathBuf]) -> Result<Baseline> {
    const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

    let parsers = crate::parse::Parsers::new();
    let mut paths: Map<String, Value> = Map::new();
    let mut notes = Vec::new();
    let mut routes = Vec::new();

    for file in files {
        if crate::lang::detect(file) != Some(crate::lang::Language::Python) {
            continue;
        }
        let source = crate::vfs::read_to_string(file)?;
        let parsed = parsers.parse(crate::lang::Language::Python, &source)?;
        if parsed.has_errors() {
            notes.push(format!(
                "{}: does not parse cleanly, so what it declares is a guess",
                relative(root, file)
            ));
            continue;
        }

        let mut found_here = false;
        let mut stack = vec![parsed.root()];
        let mut cursor = parsed.root().walk();
        while let Some(node) = stack.pop() {
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
            if node.kind() != "decorated_definition" {
                continue;
            }
            let mut children = node.walk();
            let parts: Vec<_> = node.children(&mut children).collect();
            let Some(function) = parts.iter().find(|c| c.kind() == "function_definition") else {
                continue;
            };
            for decorator in parts.iter().filter(|c| c.kind() == "decorator") {
                let text = decorator.utf8_text(source.as_bytes()).unwrap_or("");
                let Some((verb, route)) = route_of(text, METHODS) else {
                    continue;
                };
                found_here = true;
                let parameters = signature_of(*function, &source, &route);
                let entry = paths
                    .entry(route.clone())
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .expect("a path item is an object");
                entry.insert(
                    verb.clone(),
                    json!({
                        "operationId": format!("{verb}{}", operation_suffix(&route)),
                        "parameters": parameters,
                        "responses": {
                            "default": { "description": "not declared by the source" }
                        }
                    }),
                );
            }
        }
        if found_here {
            routes.push(file.clone());
        }
    }

    let document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": title,
            "version": "0.0.0",
            "description":
                "Derived from a FastAPI router by fun-refactor, by reading the decorators \
                 and the signatures. What a router does at run time is not here: a \
                 prefix, a dependency, or a route added at run time.",
        },
        "paths": Value::Object(paths),
        "components": { "schemas": {} },
    });

    Ok(Baseline {
        document,
        notes,
        routes,
    })
}

/// `@router.get("/pets/{pet_id}")`, the method and the path it answers.
fn route_of(decorator: &str, methods: &[&str]) -> Option<(String, String)> {
    let after_dot = decorator.rsplit_once('.')?.1;
    let (verb, rest) = after_dot.split_once('(')?;
    let verb = verb.trim();
    if !methods.contains(&verb) {
        return None;
    }
    // The first string literal in the call is the path; anything after it is a keyword
    // argument, and `status_code=201` is not a URL.
    let opened = rest.find(['"', '\''])?;
    let quote = rest.as_bytes()[opened] as char;
    let rest = &rest[opened + 1..];
    let closed = rest.find(quote)?;
    Some((verb.to_string(), rest[..closed].to_string()))
}

/// The parameters a handler declares, sorted into path and query.
///
/// Which is which is decided by the path template, as FastAPI decides it. A parameter whose
/// name is a segment of the URL is a path parameter and everything else the caller supplies is
/// a query one. `Request` and `Response` are FastAPI's own and are not part of the contract.
fn signature_of(function: tree_sitter::Node<'_>, source: &str, route: &str) -> Vec<Value> {
    let in_path = nextjs::path_parameters(route);
    let Some(list) = function.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    let mut out = Vec::new();
    for parameter in list.named_children(&mut cursor) {
        let text = parameter.utf8_text(source.as_bytes()).unwrap_or("");
        let (name, annotation) = match text.split_once(':') {
            Some((name, annotation)) => (name.trim(), annotation.trim()),
            None => (text.trim(), ""),
        };
        let name = name.split('=').next().unwrap_or(name).trim();
        if name.is_empty() || name == "self" {
            continue;
        }
        if matches!(annotation, "Request" | "Response") {
            continue;
        }
        let where_it_comes_from = match in_path.iter().any(|p| p == name) {
            true => "path",
            // A parameter annotated with a model is the request body. It is not a query.
            false if annotation.starts_with(|c: char| c.is_uppercase()) => continue,
            false => "query",
        };
        // The source annotated the parameter, and FastAPI coerces the path
        // segment to what it says. Writing every one as a string contradicted
        // the document's own claim about its schemas. It also disagreed with
        // the document FastAPI generates for itself.
        let schema = match annotation.split('=').next().unwrap_or("").trim() {
            "int" => json!({ "type": "integer" }),
            "float" => json!({ "type": "number" }),
            "bool" => json!({ "type": "boolean" }),
            _ => json!({ "type": "string" }),
        };
        out.push(json!({
            "name": name,
            "in": where_it_comes_from,
            "required": where_it_comes_from == "path",
            "schema": schema
        }));
    }
    out
}
