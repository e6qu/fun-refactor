//! Code from an OpenAPI document.
//!
//! `fr openapi` derives a document from a service. This is the other direction: given
//! the document, write the service's skeleton. Both targets this tool translates
//! between are supported, a FastAPI application and a Next.js App Router tree.
//!
//! # The rule
//!
//! The rule that governs the derivation governs the writing: invent nothing. Paths,
//! methods, parameters and schemas come from the document. A handler body is in no
//! document, so every generated handler answers 501 out loud, in the target's own
//! idiom. A service that answers `[]` looks finished; one that answers 501 says where
//! the work is.
//!
//! Whatever the document leaves undetermined is reported beside the plan: a schema shape
//! this cannot spell, a parameter with no type, a response left `default`.

use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One generated file.
#[derive(Debug)]
pub struct ScaffoldFile {
    pub destination: PathBuf,
    /// What this file answers, as `(method, URL)`.
    pub endpoints: Vec<(String, String)>,
    pub output: String,
}

/// A document turned into a service skeleton.
#[derive(Debug)]
pub struct ScaffoldPlan {
    pub source: PathBuf,
    pub files: Vec<ScaffoldFile>,
    /// One line per thing the document left undetermined.
    pub notes: Vec<String>,
    /// The writes, unapplied, so a caller can show them before committing to them.
    pub edits: crate::edit::EditSet,
}

/// The target a scaffold can be written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    FastApi,
    NextJs,
}

/// One operation the document declares.
struct Operation {
    method: String,
    path: String,
    /// Path parameters in the order the URL writes them, with the schema type.
    path_params: Vec<(String, String)>,
    /// Query parameters, with the schema type.
    query_params: Vec<(String, String)>,
    /// The schema name the request body references, if it references one.
    body: Option<String>,
    summary: Option<String>,
}

/// A named schema, reduced to what both targets can spell.
struct Schema {
    name: String,
    /// Field name, target-agnostic type, required.
    fields: Vec<(String, FieldType, bool)>,
}

/// The types a schema field can carry across both targets.
enum FieldType {
    String,
    Number,
    Integer,
    Boolean,
    /// An array of a spellable type.
    List(Box<FieldType>),
    /// A reference to another named schema.
    Named(String),
    /// A shape this cannot spell, carried as the loosest type with a note.
    Unknown,
}

/// Is this file an OpenAPI document?
///
/// The `openapi` key is the document's own version declaration, so its presence is the
/// document saying what it is.
pub fn is_openapi_document(path: &Path) -> bool {
    read_document(path).is_ok_and(|d| d.get("openapi").is_some())
}

/// Read a document, from JSON or YAML by content rather than extension.
fn read_document(path: &Path) -> Result<Value> {
    let text = crate::vfs::read_to_string(path)?;
    // JSON is YAML, so the YAML parser reads both; parsing JSON first keeps its
    // errors precise for the common case.
    match serde_json::from_str(&text) {
        Ok(value) => Ok(value),
        Err(_) => Ok(serde_yaml::from_str(&text)?),
    }
}

/// Write the service a document describes.
pub fn plan_to(
    path: &Path,
    target: Target,
    out: Option<&Path>,
    force: bool,
) -> Result<ScaffoldPlan> {
    let document = read_document(path)?;
    if document.get("openapi").is_none() {
        bail!(
            "{} has no `openapi` key, so it is not an OpenAPI document.",
            path.display()
        );
    }

    let mut notes = Vec::new();
    let schemas = read_schemas(&document, &mut notes);
    let operations = read_operations(&document, &mut notes)?;
    if operations.is_empty() {
        bail!(
            "{} declares no operations under `paths`, so there is no service to write.",
            path.display()
        );
    }

    let root = match (out, target) {
        (Some(out), _) => out.to_path_buf(),
        // A Next.js route is only a route under `app/api`. The URL is where the file
        // sits, and this tool's own `fr openapi` reads the tree back by that rule.
        (None, Target::NextJs) => path
            .parent()
            .unwrap_or(Path::new("."))
            .join("app")
            .join("api"),
        (None, Target::FastApi) => path.parent().unwrap_or(Path::new(".")).to_path_buf(),
    };

    let files = match target {
        Target::FastApi => vec![write_fastapi(
            path,
            &operations,
            &schemas,
            &root,
            &mut notes,
        )],
        Target::NextJs => write_nextjs(path, &operations, &schemas, &root),
    };

    let mut edits = crate::edit::EditSet::new();
    for file in &files {
        if crate::vfs::exists(&file.destination) && !force {
            bail!(
                "{} already exists; scaffolding {} would overwrite it. --force \
                 overwrites, --out chooses another directory.",
                file.destination.display(),
                path.display()
            );
        }
        let language = match target {
            Target::FastApi => crate::lang::Language::Python,
            Target::NextJs => crate::lang::Language::TypeScript,
        };
        // The output has to parse as its own language. An unparseable result is a
        // defect here, not in the caller's document, and should say so.
        let parsed = crate::parse::Parsers::new().parse(language, &file.output)?;
        if parsed.has_errors() {
            bail!(
                "the {language} this produced does not parse. That is a defect in the \
                 scaffolder; nothing was written.\n\n{}",
                file.output
            );
        }
        edits.add(
            file.destination.clone(),
            crate::edit::Edit::new(
                crate::span::Span::new(0, 0),
                &file.output,
                format!("scaffold from {}", path.display()),
            ),
        );
        edits.declare_language(file.destination.clone(), language);
    }

    Ok(ScaffoldPlan {
        source: path.to_path_buf(),
        files,
        notes,
        edits,
    })
}

/// Every named schema under `components/schemas` this can spell.
fn read_schemas(document: &Value, notes: &mut Vec<String>) -> Vec<Schema> {
    let Some(schemas) = document
        .pointer("/components/schemas")
        .and_then(|s| s.as_object())
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (name, schema) in schemas {
        let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) else {
            notes.push(format!(
                "schema `{name}` has no properties to write, so nothing is written for it"
            ));
            continue;
        };
        let required: Vec<&str> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let fields = properties
            .iter()
            .map(|(field, spec)| {
                let ty = field_type(spec);
                if matches!(ty, FieldType::Unknown) {
                    notes.push(format!(
                        "{name}.{field}: a shape this cannot spell; written as the \
                         loosest type"
                    ));
                }
                (field.clone(), ty, required.contains(&field.as_str()))
            })
            .collect();
        found.push(Schema {
            name: name.clone(),
            fields,
        });
    }
    found
}

/// The field type a schema fragment declares.
fn field_type(spec: &Value) -> FieldType {
    if let Some(reference) = spec.get("$ref").and_then(|r| r.as_str()) {
        if let Some(name) = reference.strip_prefix("#/components/schemas/") {
            return FieldType::Named(name.to_string());
        }
        return FieldType::Unknown;
    }
    match spec.get("type").and_then(|t| t.as_str()) {
        Some("string") => FieldType::String,
        Some("number") => FieldType::Number,
        Some("integer") => FieldType::Integer,
        Some("boolean") => FieldType::Boolean,
        Some("array") => match spec.get("items") {
            Some(items) => FieldType::List(Box::new(field_type(items))),
            None => FieldType::Unknown,
        },
        _ => FieldType::Unknown,
    }
}

/// Every operation the document declares, in path order.
fn read_operations(document: &Value, notes: &mut Vec<String>) -> Result<Vec<Operation>> {
    const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];
    let Some(paths) = document.get("paths").and_then(|p| p.as_object()) else {
        return Ok(Vec::new());
    };
    let mut operations = Vec::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method, operation) in item {
            if !METHODS.contains(&method.as_str()) {
                continue;
            }
            let mut path_params = Vec::new();
            let mut query_params = Vec::new();
            for parameter in operation
                .get("parameters")
                .and_then(|p| p.as_array())
                .map(|p| p.as_slice())
                .unwrap_or_default()
            {
                let name = parameter
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string();
                let ty = parameter
                    .pointer("/schema/type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("string")
                    .to_string();
                match parameter.get("in").and_then(|i| i.as_str()) {
                    Some("path") => path_params.push((name, ty)),
                    Some("query") => query_params.push((name, ty)),
                    location => notes.push(format!(
                        "{method} {path}: parameter `{name}` is in `{}`, which this does \
                         not write",
                        location.unwrap_or("nowhere")
                    )),
                }
            }
            let body = operation
                .pointer("/requestBody/content/application~1json/schema/$ref")
                .and_then(|r| r.as_str())
                .and_then(|r| r.strip_prefix("#/components/schemas/"))
                .map(|s| s.to_string());
            if operation.get("requestBody").is_some() && body.is_none() {
                notes.push(format!(
                    "{method} {path}: the request body is not a reference to a named \
                     schema, so the handler takes it untyped"
                ));
            }
            operations.push(Operation {
                method: method.clone(),
                path: path.clone(),
                path_params,
                query_params,
                body,
                summary: operation
                    .get("summary")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }
    Ok(operations)
}

/// A schema's property name, kept as the document spells it.
///
/// The name is the wire contract: `petName` in the document is the key every request
/// carries, and re-casing it to `pet_name` would change the JSON. A name Python cannot
/// declare is reported and left out, which shrinks the model out loud instead of
/// serving a different one.
fn python_name(name: &str, owner: &str, notes: &mut Vec<String>) -> Option<String> {
    if is_identifier(name) {
        return Some(name.to_string());
    }
    notes.push(format!(
        "{owner}: `{name}` is not a name Python can declare, so it is left out; \
         the wire key would change if it were re-spelled"
    ));
    None
}

/// A schema's property name as a TypeScript key, quoted where it has to be.
///
/// TypeScript can spell any property name, so nothing is left out: `pet-name` becomes
/// `"pet-name"` and the wire contract holds.
fn ts_key(name: &str) -> String {
    if is_identifier(name) {
        name.to_string()
    } else {
        format!("{name:?}")
    }
}

/// Is this a name both targets can write bare?
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `pet_id`, whatever the document called it.
fn snake(name: &str) -> String {
    super::snake_always(name)
}

/// The URL with its parameters spelled the way the signature spells them.
///
/// FastAPI binds a path parameter by name: `{petId}` over `pet_id: int` never binds, and
/// the route answers 422 for every request. The URL a caller sees is unchanged, since a
/// placeholder's name is internal to the framework.
fn python_path(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push('{');
        out.push_str(&snake(&rest[open + 1..open + close]));
        out.push('}');
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A Python function name for one operation: `get_pets_pet_id`.
fn python_handler_name(operation: &Operation) -> String {
    let path = operation
        .path
        .trim_matches('/')
        .replace(['{', '}'], "")
        .replace(['/', '-', ':'], "_");
    let base = match path.is_empty() {
        true => "root".to_string(),
        false => path,
    };
    snake(&format!("{}_{}", operation.method, base))
}

// ------------------------------------------------------------------- FastAPI

fn python_field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::String => "str".into(),
        FieldType::Number => "float".into(),
        FieldType::Integer => "int".into(),
        FieldType::Boolean => "bool".into(),
        FieldType::List(inner) => format!("list[{}]", python_field_type(inner)),
        FieldType::Named(name) => name.clone(),
        // `Any` says out loud that the document did not say.
        FieldType::Unknown => "Any".into(),
    }
}

fn python_parameter_type(ty: &str) -> &'static str {
    match ty {
        "integer" => "int",
        "number" => "float",
        "boolean" => "bool",
        _ => "str",
    }
}

fn write_fastapi(
    source: &Path,
    operations: &[Operation],
    schemas: &[Schema],
    root: &Path,
    notes: &mut Vec<String>,
) -> ScaffoldFile {
    let mut out = String::new();
    out.push_str(&format!(
        "# Scaffolded from {} by fun-refactor.\n",
        source.display()
    ));
    out.push_str("# Paths, methods, parameters and models come from the document.\n");
    out.push_str("# The handler bodies are in no document: each raises 501 until written.\n\n");
    let uses_any = schemas
        .iter()
        .any(|s| s.fields.iter().any(|(_, ty, _)| loose(ty)));
    out.push_str("from fastapi import APIRouter, HTTPException\n");
    if !schemas.is_empty() {
        out.push_str("from pydantic import BaseModel\n");
    }
    if uses_any {
        out.push_str("from typing import Any\n");
    }
    out.push_str("\nrouter = APIRouter()\n");

    for schema in schemas {
        out.push_str(&format!("\n\nclass {}(BaseModel):\n", schema.name));
        if schema.fields.is_empty() {
            out.push_str("    pass\n");
        }
        for (field, ty, required) in &schema.fields {
            let spelled = python_field_type(ty);
            let Some(key) = python_name(field, &schema.name, notes) else {
                continue;
            };
            match required {
                true => out.push_str(&format!("    {key}: {spelled}\n")),
                false => out.push_str(&format!("    {key}: {spelled} | None = None\n")),
            }
        }
    }

    let mut endpoints = Vec::new();
    for operation in operations {
        endpoints.push((operation.method.to_uppercase(), operation.path.clone()));
        out.push_str(&format!(
            "\n\n@router.{}(\"{}\")\n",
            operation.method,
            python_path(&operation.path)
        ));
        let mut signature = Vec::new();
        for (name, ty) in &operation.path_params {
            signature.push(format!("{}: {}", snake(name), python_parameter_type(ty)));
        }
        if let Some(body) = &operation.body {
            signature.push(format!("body: {body}"));
        }
        for (name, ty) in &operation.query_params {
            let Some(key) = python_name(name, &operation.path, notes) else {
                continue;
            };
            signature.push(format!(
                "{key}: {} | None = None",
                python_parameter_type(ty)
            ));
        }
        out.push_str(&format!(
            "async def {}({}):\n",
            python_handler_name(operation),
            signature.join(", ")
        ));
        if let Some(summary) = &operation.summary {
            out.push_str(&format!("    \"\"\"{}\"\"\"\n", summary.replace('"', "'")));
        }
        out.push_str(&format!(
            "    raise HTTPException(status_code=501, detail=\"{} {} is not implemented\")\n",
            operation.method.to_uppercase(),
            operation.path
        ));
    }

    ScaffoldFile {
        destination: root.join("api.py"),
        endpoints,
        output: out,
    }
}

/// Does this type bottom out in `Unknown` anywhere?
fn loose(ty: &FieldType) -> bool {
    match ty {
        FieldType::Unknown => true,
        FieldType::List(inner) => loose(inner),
        _ => false,
    }
}

// ------------------------------------------------------------------- Next.js

fn typescript_field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::String => "string".into(),
        FieldType::Number | FieldType::Integer => "number".into(),
        FieldType::Boolean => "boolean".into(),
        FieldType::List(inner) => format!("{}[]", typescript_field_type(inner)),
        FieldType::Named(name) => name.clone(),
        FieldType::Unknown => "unknown".into(),
    }
}

/// `{petId}` and `{pet_id}` both become the directory `[petId]`.
fn nextjs_directory(path: &str) -> PathBuf {
    let mut directory = PathBuf::new();
    for segment in path.trim_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        match segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            Some(inner) => directory.push(format!("[{}]", super::write::camel(inner))),
            None => directory.push(segment),
        }
    }
    directory
}

fn write_nextjs(
    source: &Path,
    operations: &[Operation],
    schemas: &[Schema],
    root: &Path,
) -> Vec<ScaffoldFile> {
    // One file per URL, holding every method declared for it.
    let mut by_path: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for operation in operations {
        by_path
            .entry(operation.path.clone())
            .or_default()
            .push(operation);
    }

    let mut files = Vec::new();
    for (path, group) in by_path {
        let mut out = String::new();
        out.push_str(&format!(
            "// Scaffolded from {} by fun-refactor.\n",
            source.display()
        ));
        out.push_str("// Paths, methods, parameters and models come from the document.\n");
        out.push_str("// The handler bodies are in no document: each answers 501 until written.\n");

        // The schemas this file's operations name, declared in it: a route file
        // stands on its own.
        let named: Vec<&Schema> = schemas
            .iter()
            .filter(|schema| {
                group
                    .iter()
                    .any(|operation| operation.body.as_deref() == Some(schema.name.as_str()))
            })
            .collect();
        for schema in &named {
            out.push_str(&format!("\nexport interface {} {{\n", schema.name));
            for (field, ty, required) in &schema.fields {
                let marker = if *required { "" } else { "?" };
                out.push_str(&format!(
                    "    {}{marker}: {};\n",
                    ts_key(field),
                    typescript_field_type(ty)
                ));
            }
            out.push_str("}\n");
        }

        let parameters: &[(String, String)] = &group[0].path_params;
        if !parameters.is_empty() {
            out.push_str("\ninterface RouteParams {\n");
            for (name, _) in parameters {
                // Every Next.js path segment arrives as text, whatever the document
                // declared; the handler converts.
                out.push_str(&format!("    {}: string;\n", super::write::camel(name)));
            }
            out.push_str("}\n");
        }
        out.push_str("\ninterface RouteContext {\n");
        match parameters.is_empty() {
            true => out.push_str("    params: Record<string, string>;\n"),
            false => out.push_str("    params: RouteParams;\n"),
        }
        out.push_str("}\n");

        let mut endpoints = Vec::new();
        for operation in &group {
            endpoints.push((operation.method.to_uppercase(), path.clone()));
            out.push('\n');
            if let Some(summary) = &operation.summary {
                out.push_str(&format!("/** {} */\n", summary.replace("*/", "*\u{200b}/")));
            }
            out.push_str(&format!(
                "export async function {}(request: Request, context: RouteContext): \
                 Promise<Response> {{\n",
                operation.method.to_uppercase()
            ));
            out.push_str(&format!(
                "    return Response.json({{ error: \"{} {} is not implemented\" }}, \
                 {{ status: 501 }});\n}}\n",
                operation.method.to_uppercase(),
                path
            ));
        }

        files.push(ScaffoldFile {
            destination: root.join(nextjs_directory(&path)).join("route.ts"),
            endpoints,
            output: out,
        });
    }
    files
}
