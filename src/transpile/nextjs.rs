//! Next.js API routes as FastAPI endpoints.

use super::ir::*;
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// The HTTP methods a Next.js App Router route file may export.
const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// A route file translated into a FastAPI module.
#[derive(Debug)]
pub struct RoutePlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    /// The URL path this file serves, in FastAPI's spelling.
    pub route: String,
    /// The methods this found, in declaration order.
    pub methods: Vec<String>,
    /// Every endpoint the file declares, as `(method, URL)`.
    pub endpoints: Vec<(String, String)>,
    pub output: String,
    pub fidelity: Fidelity,
    /// The write, unapplied, so a caller can show it before committing to it.
    pub edits: crate::edit::EditSet,
    /// The shapes this route declares, as an OpenAPI document would name them.
    pub models: Vec<Model>,
    /// Which schema each handler validates its body against.
    pub bodies: Vec<(String, String)>,
    /// The query parameters each handler reads.
    pub queries: Vec<(String, String)>,
    /// Every status code the handlers return, in the order met.
    pub statuses: Vec<String>,
}

/// A named shape a route declares, from an exported `interface` or a zod schema.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    /// Field name and its type, spelled as the IR sees it.
    pub fields: Vec<(String, Option<Type>)>,
}

/// One endpoint a file declares, however it declares it.
struct Endpoint {
    /// The HTTP method, in the decorator's spelling.
    method: String,
    /// The URL it answers.
    route: String,
    /// The name the Python function takes.
    name: String,
    handler: Function,
    kind: Kind,
}

/// How a caller reaches an endpoint, which decides where its arguments come from.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    /// An App Router handler: the URL is the file's position, and the handler reads its
    /// arguments off the request and the URL.
    Route,
    /// A `"use server"` function: the framework generates the call, and the arguments are
    /// the function's own parameters.
    ServerFunction,
}

/// Is this file a module of server functions?
pub fn is_server_module(path: &Path) -> bool {
    let Some(language) = crate::lang::detect(path) else {
        return false;
    };
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        return false;
    }
    crate::vfs::read_to_string(path)
        .map(|source| declares_use_server(&source))
        .unwrap_or(false)
}

/// Does this source open with the `"use server"` directive?
fn declares_use_server(source: &str) -> bool {
    source
        .lines()
        .find(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
        })
        .map(|line| line.trim().trim_end_matches(';').trim())
        .is_some_and(|line| line == "\"use server\"" || line == "'use server'")
}

/// `createPet` → `/create-pet`, the URL that reaches a server function.
fn action_route(name: &str) -> String {
    let mut url = String::from("/");
    for (index, part) in super::snake_always(name).split('_').enumerate() {
        if part.is_empty() {
            continue;
        }
        if index > 0 {
            url.push('-');
        }
        url.push_str(part);
    }
    url
}

/// The path segments that make up a route's URL, if this file is one.
fn route_segments(path: &Path) -> Option<Vec<String>> {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();

    let stem = path.file_stem()?.to_string_lossy().to_string();
    let extension = path.extension()?.to_string_lossy().to_string();
    if extension != "ts" && extension != "tsx" && extension != "js" && extension != "jsx" {
        return None;
    }

    // `pages/api/...`, the pair has to be adjacent, or `pages/foo/api` would count.
    let pages = parts
        .windows(2)
        .position(|w| w[0] == "pages" && w[1] == "api")
        .map(|at| at + 2);
    // `app/**/api/**/route.ts`, the last `api` above the file, since a route may
    // legitimately be at `app/api/api/route.ts`.
    let app = (stem == "route")
        .then(|| {
            parts
                .iter()
                .rposition(|part| part == "api")
                .map(|at| at + 1)
        })
        .flatten();

    let after = pages.or(app)?;
    let mut segments = Vec::new();
    for (index, part) in parts.iter().enumerate().skip(after) {
        let last = index + 1 == parts.len();
        let name = if last { stem.as_str() } else { part.as_str() };
        // `route.ts` names the file, not the URL; a Pages `index.ts` is its directory.
        if name.is_empty() || (last && (name == "route" || name == "index")) {
            continue;
        }
        segments.push(translate_segment(name));
    }
    Some(segments)
}

/// Is this a Next.js API route?
pub fn is_api_route(path: &Path) -> bool {
    route_segments(path).is_some()
}

/// The URL a route file serves, derived from where it sits.
pub fn route_for(path: &Path) -> String {
    let segments = route_segments(path).unwrap_or_default();
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

/// Does this file contain JSX.
fn has_jsx(source: &str, language: Language) -> Result<bool> {
    let parsers = Parsers::new();
    let parsed = parsers.parse(language, source)?;
    let mut stack = vec![parsed.root()];
    let mut cursor = parsed.root().walk();
    while let Some(node) = stack.pop() {
        if node.kind().starts_with("jsx_") {
            return Ok(true);
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Ok(false)
}

/// `[id]` → `{id}`, `[...path]` → `{path:path}`, anything else unchanged.
fn translate_segment(segment: &str) -> String {
    // The placeholder's name is internal: `/posts/{postId}` and `/posts/{post_id}` serve the
    // same URLs.
    let Some(inner) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return segment.to_string();
    };
    // `[[...slug]]` is optional-catch-all; the brackets nest.
    let inner = inner.trim_start_matches('[').trim_end_matches(']');
    match inner.strip_prefix("...") {
        // A catch-all matches slashes too, which FastAPI spells `:path`.
        Some(name) => format!("{{{}:path}}", super::snake_always(name)),
        None => format!("{{{}}}", super::snake_always(inner)),
    }
}

/// Translate a Next.js API route into a FastAPI module.
pub fn plan(path: &Path) -> Result<RoutePlan> {
    plan_to(path, None, false)
}

/// [`plan`], with the destination and the overwrite decision in the caller's hands.
pub fn plan_to(path: &Path, out: Option<&Path>, force: bool) -> Result<RoutePlan> {
    crate::capabilities::record(
        crate::capabilities::Capability::Openapi,
        crate::lang::detect(path).unwrap_or(crate::lang::Language::TypeScript),
    );
    let Some(language) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        bail!(
            "{} is {language}. A Next.js route is TypeScript.",
            path.display()
        );
    }

    let source = crate::vfs::read_to_string(path)?;

    if has_jsx(&source, language)? {
        bail!(
            "{} contains JSX, so it is a React component and not an API route. A \
             component renders a user interface and a FastAPI endpoint answers HTTP; \
             there is no translation between them, and a file that pretended there was \
             would be worse than none.",
            path.display()
        );
    }

    let server_module = declares_use_server(&source);
    if !is_api_route(path) && !server_module {
        bail!(
            "{} is neither a Next.js API route nor a module of server functions. A route \
             is `app/**/api/**/route.ts` or anything under `pages/api/`, and its URL comes \
             from where the file sits. A server module opens with `\"use server\"`, and its own name \
             reaches each of its exports.",
            path.display()
        );
    }

    let parsers = Parsers::new();
    let parsed = parsers.parse(language, &source)?;
    if parsed.has_errors() {
        bail!(
            "{} does not parse cleanly, so anything read out of it would be a guess",
            path.display()
        );
    }

    let module = super::read_module(language, &source, parsed.root())?;
    let route = route_for(path);
    let endpoints = match server_module {
        true => server_endpoints(&module),
        false => route_endpoints(&module, &route),
    };
    let Written {
        output,
        fidelity,
        methods,
        statuses,
    } = write(&module, &endpoints, path)?;

    // The declared shapes, from either place a Next.js route keeps them.
    let models: Vec<Model> = models_of(&module);

    let bodies = parsed_bodies(&module);
    let queries = read_queries(&module);

    if endpoints.is_empty() && server_module {
        bail!(
            "{} opens with `\"use server\"` and exports no async function. The framework \
             generates a call for each export, so a module without one declares nothing \
             to reach.",
            path.display()
        );
    }
    if methods.is_empty() {
        bail!(
            "{} exports no HTTP method. An App Router route exports `GET`, `POST` and \
             so on by name; a Pages Router one exports a default handler, which this \
             does not yet read.",
            path.display()
        );
    }

    let written = parsers.parse(Language::Python, &output)?;
    if written.has_errors() {
        bail!(
            "the FastAPI module this produced does not parse as Python. That is a defect \
             in the translator; this wrote nothing.\n\n{output}"
        );
    }

    let destination = match (out, server_module) {
        (Some(out), _) => out.to_path_buf(),
        // A server module's name is its own: nothing on disk says where its calls go.
        (None, true) => path.with_extension("py"),
        (None, false) => path.with_file_name(format!(
            "{}.py",
            route
                .trim_matches('/')
                .replace(['/', '{', '}', ':'], "_")
                .split('_')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("_")
                .then_or("index")
        )),
    };

    if crate::vfs::exists(&destination) && !force {
        bail!(
            "{} already exists; translating {} would overwrite it. --force \
             overwrites, --out chooses another path.",
            destination.display(),
            path.display()
        );
    }

    let mut edits = crate::edit::EditSet::new();
    edits.add(
        destination.clone(),
        crate::edit::Edit::new(
            crate::span::Span::new(0, 0),
            &output,
            format!("translate {} to FastAPI", path.display()),
        ),
    );
    edits.declare_language(destination.clone(), crate::lang::Language::Python);

    let route = match endpoints.first() {
        Some(first) => first.route.clone(),
        None => route,
    };
    Ok(RoutePlan {
        source: path.to_path_buf(),
        destination,
        edits,
        route,
        methods,
        endpoints: endpoints
            .iter()
            .map(|e| (e.method.to_uppercase(), e.route.clone()))
            .collect(),
        output,
        fidelity,
        models,
        bodies,
        queries,
        statuses,
    })
}

/// The endpoints an App Router file declares: its exports named after HTTP methods.
fn route_endpoints(module: &Module, route: &str) -> Vec<Endpoint> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) if METHODS.contains(&f.name.to_uppercase().as_str()) => {
                Some(Endpoint {
                    method: f.name.to_lowercase(),
                    route: route.to_string(),
                    name: f.name.to_lowercase(),
                    handler: f.clone(),
                    kind: Kind::Route,
                })
            }
            _ => None,
        })
        .collect()
}

/// The endpoints a `"use server"` module declares: its exported async functions.
fn server_endpoints(module: &Module) -> Vec<Endpoint> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) if f.exported && f.is_async => Some(Endpoint {
                method: "post".into(),
                route: action_route(&f.name),
                name: super::snake_always(&f.name),
                handler: f.clone(),
                kind: Kind::ServerFunction,
            }),
            _ => None,
        })
        .collect()
}

/// A tiny helper so an empty route name becomes a usable file name.
trait ThenOr {
    fn then_or(self, fallback: &str) -> String;
}

impl ThenOr for String {
    fn then_or(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

/// Turn a zod schema into a record, so a request body reaches FastAPI as a model.
pub fn models_in(path: &Path) -> Result<Vec<Model>> {
    let Some(language) = crate::lang::detect(path) else {
        return Ok(Vec::new());
    };
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        return Ok(Vec::new());
    }
    let source = crate::vfs::read_to_string(path)?;
    let parsed = Parsers::new().parse(language, &source)?;
    if parsed.has_errors() {
        return Ok(Vec::new());
    }
    let module = super::read_module(language, &source, parsed.root())?;
    Ok(models_of(&module))
}

/// The shapes a module declares, from either place a Next.js file keeps them.
fn models_of(module: &Module) -> Vec<Model> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(record) => Some(Model {
                name: record.name.clone(),
                fields: record
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect(),
            }),
            Item::Constant(c) => record_from_zod(&c.name, &c.value).map(|record| Model {
                name: record.name.clone(),
                fields: record
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect(),
            }),
            _ => None,
        })
        .collect()
}

/// Which query parameters each handler reads out of the URL.
fn read_queries(module: &Module) -> Vec<(String, String)> {
    fn in_expr(e: &Expr, found: &mut Vec<String>) {
        if let Expr::Call { callee, args } = e {
            if let Expr::Field { of, name } = callee.as_ref() {
                let reads = matches!(name.as_str(), "get" | "getAll" | "has");
                if reads && from_search_params(of) {
                    if let Some(Expr::Str(key)) = args.first() {
                        found.push(key.clone());
                    }
                }
            }
        }
        match e {
            Expr::Await(inner) | Expr::Unary { operand: inner, .. } => in_expr(inner, found),
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                in_expr(callee, found);
                for argument in args {
                    in_expr(argument, found);
                }
            }
            Expr::Binary { left, right, .. }
            | Expr::Coalesce {
                value: left,
                fallback: right,
            } => {
                in_expr(left, found);
                in_expr(right, found);
            }
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => {
                in_expr(condition, found);
                in_expr(then, found);
                in_expr(otherwise, found);
            }
            _ => {}
        }
    }

    fn in_stmts(stmts: &[Stmt], found: &mut Vec<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { value: Some(v), .. } | Stmt::Expr(v) | Stmt::Return(Some(v)) => {
                    in_expr(v, found)
                }
                Stmt::Assign { value, .. } => in_expr(value, found),
                Stmt::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    in_expr(condition, found);
                    in_stmts(then, found);
                    in_stmts(otherwise, found);
                }
                Stmt::While { body, .. } | Stmt::ForEach { body, .. } => in_stmts(body, found),
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    in_stmts(body, found);
                    for catch in catches {
                        in_stmts(&catch.body, found);
                    }
                    in_stmts(finally, found);
                }
                _ => {}
            }
        }
    }

    let mut out = Vec::new();
    for item in &module.items {
        let Item::Function(f) = item else { continue };
        if !METHODS.contains(&f.name.to_uppercase().as_str()) {
            continue;
        }
        let mut found = Vec::new();
        in_stmts(&f.body, &mut found);
        found.dedup();
        for key in found {
            out.push((f.name.clone(), key));
        }
    }
    out
}

/// Does this expression reach through `searchParams`?
fn from_search_params(e: &Expr) -> bool {
    match e {
        Expr::Name(name) => name == "searchParams",
        Expr::Field { of, name } => name == "searchParams" || from_search_params(of),
        Expr::Call { callee, .. } => from_search_params(callee),
        _ => false,
    }
}

/// Which schema each handler parses its body with.
fn parsed_bodies(module: &Module) -> Vec<(String, String)> {
    fn in_expr(e: &Expr, found: &mut Vec<String>) {
        if let Expr::Call { callee, .. } = e {
            if let Expr::Field { of, name } = callee.as_ref() {
                let validates = matches!(name.as_str(), "parse" | "parseAsync" | "safeParse");
                if let (true, Expr::Name(schema)) = (validates, of.as_ref()) {
                    found.push(schema.clone());
                }
            }
        }
        match e {
            Expr::Await(inner) | Expr::Unary { operand: inner, .. } => in_expr(inner, found),
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                in_expr(callee, found);
                for argument in args {
                    in_expr(argument, found);
                }
            }
            Expr::Binary { left, right, .. }
            | Expr::Coalesce {
                value: left,
                fallback: right,
            } => {
                in_expr(left, found);
                in_expr(right, found);
            }
            _ => {}
        }
    }

    fn in_stmts(stmts: &[Stmt], found: &mut Vec<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { value: Some(v), .. } | Stmt::Expr(v) | Stmt::Return(Some(v)) => {
                    in_expr(v, found)
                }
                Stmt::Assign { value, .. } => in_expr(value, found),
                Stmt::If {
                    then, otherwise, ..
                } => {
                    in_stmts(then, found);
                    in_stmts(otherwise, found);
                }
                Stmt::While { body, .. } | Stmt::ForEach { body, .. } => in_stmts(body, found),
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    in_stmts(body, found);
                    for catch in catches {
                        in_stmts(&catch.body, found);
                    }
                    in_stmts(finally, found);
                }
                _ => {}
            }
        }
    }

    let mut out = Vec::new();
    for item in &module.items {
        let Item::Function(f) = item else { continue };
        if !METHODS.contains(&f.name.to_uppercase().as_str()) {
            continue;
        }
        let mut found = Vec::new();
        in_stmts(&f.body, &mut found);
        if let Some(schema) = found.first() {
            out.push((f.name.clone(), model_name(schema)));
        }
    }
    out
}
/// The IR already holds the whole builder chain, so this is a walk and not a parse.
fn record_from_zod(name: &str, value: &Expr) -> Option<Record> {
    let fields = object_fields(value)?;
    Some(Record {
        extends: None,
        doc: vec![format!("Derived from the zod schema `{name}`.")],
        // `postPatchSchema` describes a `PostPatch`.
        name: model_name(name),
        fields,
        exported: true,
        methods: Vec::new(),
    })
}

/// `postPatchSchema` -> `PostPatch`; `userSchema` -> `User`.
fn model_name(name: &str) -> String {
    let base = name
        .strip_suffix("Schema")
        .or_else(|| name.strip_suffix("_schema"))
        .unwrap_or(name);
    let mut out = String::new();
    let mut upper = true;
    for c in base.chars() {
        if c == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The `{ a: …, b: … }` of a `z.object({ … })`, as fields.
fn object_fields(value: &Expr) -> Option<Vec<Field>> {
    let Expr::Call { callee, args } = value else {
        return None;
    };
    if !is_zod(callee, "object") {
        return None;
    }
    let Some(Expr::MapLit(entries)) = args.first() else {
        return None;
    };
    Some(
        entries
            .iter()
            .filter_map(|(key, spec)| {
                let name = match key {
                    Expr::Str(text) => text.clone(),
                    Expr::Name(text) => text.clone(),
                    _ => return None,
                };
                Some(Field {
                    doc: Vec::new(),
                    ty: Some(zod_type(spec)),
                    name,
                    default: None,
                    exported: true,
                })
            })
            .collect(),
    )
}

/// Is this `z.<method>`, under however many builders?
fn is_zod(callee: &Expr, method: &str) -> bool {
    matches!(callee, Expr::Field { of, name }
        if name == method && matches!(of.as_ref(), Expr::Name(z) if z == "z"))
}

/// The type a zod builder chain describes.
fn zod_type(spec: &Expr) -> Type {
    let mut modifiers: Vec<String> = Vec::new();
    let mut current = spec;
    while let Expr::Call { callee, args } = current {
        let Expr::Field { of, name } = callee.as_ref() else {
            break;
        };
        // The base of the chain: `z.something(...)`.
        if matches!(of.as_ref(), Expr::Name(z) if z == "z") {
            let mut ty = match name.as_str() {
                "string" => Type::String,
                "boolean" => Type::Bool,
                "number" => {
                    if modifiers.iter().any(|m| m == "int") {
                        Type::Int
                    } else {
                        Type::Float
                    }
                }
                "bigint" => Type::Int,
                "date" => Type::named("datetime"),
                "array" => Type::List(Box::new(
                    args.first().map(zod_type).unwrap_or(Type::named("Any")),
                )),
                "record" => Type::Map(Box::new(Type::String), Box::new(Type::named("Any"))),
                // `z.object` nested inside another is an anonymous shape; Python wants
                // it to be its own model and naming one would be inventing a name.
                "object" => Type::named("dict"),
                // `z.enum([...])`, `z.union([...])`, `z.any()`, and anything else this
                // does not know: written through by name and not guessed at.
                other => Type::named(other),
            };
            if modifiers.iter().any(|m| m == "optional" || m == "nullable") {
                ty = Type::Optional(Box::new(ty));
            }
            return ty;
        }
        modifiers.push(name.clone());
        current = of;
    }
    Type::named("Any")
}

/// A placeholder for the header line that depends on how the translation went.
const VERDICT: &str = "# fun-refactor: verdict\n\n";

/// Write the FastAPI module.
fn write(module: &Module, endpoints: &[Endpoint], source: &Path) -> Result<Written> {
    // What each handler reads out of the URL.
    let query_keys = read_queries(module);
    // The handlers are the exported functions named after HTTP methods; everything
    // else in the file is a helper and takes an ordinary function's shape.
    let handlers: Vec<&Endpoint> = endpoints.iter().collect();
    let mut rest = Module {
        doc: module.doc.clone(),
        name: module.name.clone(),
        items: Vec::new(),
        sweep_notes: Vec::new(),
    };
    for item in &module.items {
        match item {
            Item::Function(f) if endpoints.iter().any(|e| e.handler.name == f.name) => {}
            // The `"use server"` directive is what made this file a server module.
            Item::Statement(Stmt::Expr(Expr::Str(text))) if text == "use server" => {}
            // `import { NextResponse } from "next/server"` is the one import that leaves no
            // work for the reader, because this translation rewrites every use of it.
            Item::Import { text, .. } if text.contains("\"next/") || text.contains("'next/") => {}
            // A zod schema describes a shape.
            Item::Constant(c) => match record_from_zod(&c.name, &c.value) {
                Some(record) => rest.items.push(Item::Record(record)),
                None => rest.items.push(item.clone()),
            },
            other => rest.items.push(other.clone()),
        }
    }

    let methods: Vec<String> = handlers.iter().map(|h| h.method.to_uppercase()).collect();

    // Everything that is not a handler goes through the ordinary Python writer, which turns
    // interfaces into dataclasses.
    let (body, mut fidelity) = super::write_module(Language::Python, &rest)?;
    let body = body
        .replace("from dataclasses import dataclass", "")
        .replace("@dataclass", "");

    let mut out = String::new();
    out.push_str(&format!(
        "# Translated from a Next.js API route ({}) by fun-refactor.\n",
        source.display()
    ));
    // One line per URL: a route file serves one, and a server module serves one per
    // exported function, each named beside its method.
    let served: Vec<String> = handlers
        .iter()
        .map(|e| format!("{} {}", e.method.to_uppercase(), e.route))
        .collect();
    out.push_str(&format!(
        "# {} handler(s): {}\n",
        handlers.len(),
        served.join(", ")
    ));
    // The verdict is filled in at the end.
    out.push_str(VERDICT);
    out.push_str("from fastapi import APIRouter\n");
    if !rest
        .items
        .iter()
        .filter(|i| matches!(i, Item::Record(_)))
        .collect::<Vec<_>>()
        .is_empty()
    {
        out.push_str("from pydantic import BaseModel\n");
    }
    out.push('\n');
    out.push_str("router = APIRouter()\n\n");

    let with_models = promote_models(&body, &rest);
    if !with_models.trim().is_empty() {
        out.push_str(with_models.trim_start());
        out.push_str("\n\n");
    }

    // The imports the file will need.
    let mut takes_request = false;
    let mut responses = Responses::default();

    for endpoint in &handlers {
        let handler = &endpoint.handler;
        let method = &endpoint.method;
        let route = &endpoint.route;
        // The path parameters this endpoint's URL declares, which its handler receives.
        let parameters = path_parameters(route);
        out.push('\n');
        out.push_str(&format!("@router.{method}(\"{route}\")\n"));

        let mut signature: Vec<String> = match endpoint.kind {
            // A server function's arguments are its parameters, and the framework carries them.
            Kind::ServerFunction => handler
                .params
                .iter()
                .map(|p| match &p.ty {
                    Some(ty) => format!(
                        "{}: {}",
                        super::snake_always(&p.name),
                        super::write::python_type(ty)
                    ),
                    None => super::snake_always(&p.name),
                })
                .collect(),
            Kind::Route => parameters
                .iter()
                .map(|name| format!("{name}: str"))
                .collect(),
        };

        let declared_queries: Vec<String> = query_keys
            .iter()
            .filter(|_| endpoint.kind == Kind::Route)
            .filter(|(m, _)| m.eq_ignore_ascii_case(&handler.name))
            .map(|(_, key)| (key.clone(), super::snake_always(key)))
            .filter(|(key, name)| {
                // No declaration takes a key that is not a name (`page-size`, `filter[]`), and
                // inventing a spelling for it would answer a different URL.
                let spellable = !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !name.starts_with(|c: char| c.is_ascii_digit());
                let free =
                    !parameters.contains(name) && !handler.params.iter().any(|p| p.name == *name);
                if !spellable {
                    fidelity.notes.push(format!(
                        "`{}` reads the query key `{key}`, which is not a name Python can \
                         declare; that read stands as it was and the endpoint's \
                         contract does not mention it",
                        handler.name
                    ));
                }
                spellable && free
            })
            .map(|(_, name)| name)
            .collect();

        // A Next.js handler takes `(request, context)`.
        let request = handler
            .params
            .iter()
            .filter(|_| endpoint.kind == Kind::Route)
            .find(|p| is_the_request(p));
        if let Some(param) = request {
            signature.push(format!("{}: Request", param.name));
            takes_request = true;
        }
        // Last, because they carry a default and Python will not have one before a
        // parameter that does not.
        for name in &declared_queries {
            signature.push(format!("{name}: str | None = None"));
        }
        if signature.is_empty() {
            signature.push(String::new());
        }
        out.push_str(&format!(
            "async def {}({}):\n",
            endpoint.name,
            signature.join(", ").trim()
        ));

        // A Next.js handler receives `(request, context)`.
        let dropped: Vec<String> = match endpoint.kind {
            Kind::ServerFunction => Vec::new(),
            Kind::Route => handler
                .params
                .iter()
                .filter(|p| !is_the_request(p))
                .map(|p| p.name.clone())
                .collect(),
        };
        // `const id = context.params.id` is *redundant*.
        let mut body = Vec::new();
        for stmt in &handler.body {
            match supplied_by_fastapi(stmt, &dropped, &parameters) {
                Some(name) => fidelity.notes.push(format!(
                    "`{}` read `{name}` off the Next.js context; FastAPI supplies it as a \
                     path parameter, so that line went",
                    handler.name
                )),
                None => {
                    let supplied = supply_path_parameters(stmt.clone(), &dropped, &parameters);
                    let supplied = supply_query_parameters(supplied, &declared_queries);
                    match binds_itself(&supplied) {
                        Some(name) => fidelity.notes.push(format!(
                            "`{}` read `{name}` out of the query string; FastAPI supplies \
                             it as a parameter, so that line went",
                            handler.name
                        )),
                        None => body.push(as_fastapi(supplied, &mut responses)),
                    }
                }
            }
        }
        let uses_dropped: Vec<String> = dropped
            .iter()
            .filter(|name| body.iter().any(|s| reads(s, std::slice::from_ref(*name))))
            .cloned()
            .collect();

        // The body, through the ordinary writer, indented into the handler.
        let one = Module {
            doc: Vec::new(),
            name: None,
            sweep_notes: Vec::new(),
            items: vec![Item::Function(Function {
                doc: handler.doc.clone(),
                name: "body".into(),
                receiver: None,
                receiver_binding: None,
                params: Vec::new(),
                returns: None,
                body,
                exported: false,
                is_async: false,
                is_property: false,
                is_constructor: false,
                is_private: false,
            })],
        };
        let (written, inner) = super::write_module_in(Language::Python, &one, module)?;
        fidelity.carried_verbatim += inner.carried_verbatim;
        fidelity.notes.extend(inner.notes);

        let mut lines: Vec<String> = written
            .lines()
            .skip_while(|l| !l.starts_with("def body"))
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect();

        // The lines below are faithful to the source and name objects FastAPI does not have.
        if !uses_dropped.is_empty() {
            let warning = vec![
                format!(
                    "    # fun-refactor: this handler read {}. A Next.js object with no",
                    uses_dropped
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
                "    # FastAPI counterpart. The lines naming it will not run as written;"
                    .to_string(),
                "    # declare a request model or take `Request` instead.".to_string(),
            ];
            let after_doc = lines
                .iter()
                .position(|l| !l.trim_start().starts_with("\"\"\""))
                .unwrap_or(0)
                .max(usize::from(
                    lines
                        .first()
                        .is_some_and(|l| l.trim_start().starts_with("\"\"\"")),
                ));
            for (offset, line) in warning.into_iter().enumerate() {
                lines.insert(after_doc + offset, line);
            }
        }

        let wrote_any = !lines.is_empty();
        for line in lines {
            out.push_str(&format!("{line}\n"));
        }
        if !wrote_any {
            out.push_str("    raise NotImplementedError\n");
        }
        out.push('\n');
    }

    // `+=` rather than `=`: a route file may declare helpers beside its handlers, and the
    // ordinary writer already counted those.
    let mut imports = vec!["APIRouter"];
    if takes_request {
        imports.push("Request");
    }
    // `Response` comes from `fastapi` itself, unlike `JSONResponse`.
    if responses.plain_response {
        imports.push("Response");
    }
    out = out.replace(
        "from fastapi import APIRouter\n",
        &format!("from fastapi import {}\n", imports.join(", ")),
    );
    let mut extra = Vec::new();
    if responses.json_with_status {
        extra.push("JSONResponse");
    }
    if responses.redirect {
        extra.push("RedirectResponse");
    }
    // A model derived from `z.date()` is annotated `datetime`.
    if out.contains("datetime") && !out.contains("from datetime import") {
        out = out.replace(
            "\nrouter = APIRouter()",
            "from datetime import datetime\n\nrouter = APIRouter()",
        );
    }
    if !extra.is_empty() {
        out = out.replace(
            "\nrouter = APIRouter()",
            &format!(
                "from fastapi.responses import {}\n\nrouter = APIRouter()",
                extra.join(", ")
            ),
        );
    }

    out = out.replace(
        VERDICT,
        if fidelity.carried_verbatim == 0 {
            "# Routes, methods, path parameters, models and handler bodies all carried\n\
             # across. What remains foreign is your own dependencies. The database\n\
             # calls and helpers this file imported, which no translation could supply.\n\n"
        } else {
            "# The routes, methods, path parameters and models all crossed. Some of\n\
             # the handler bodies had no Python counterpart and are below as comments,\n\
             # marked, for you to finish. THIS FILE IS A DRAFT.\n\n"
        },
    );

    // PEP 8 wants two blank lines before a top-level definition.
    for keyword in ["class ", "@router."] {
        out = out.replace(&format!("\n\n{keyword}"), &format!("\n\n\n{keyword}"));
    }

    // The status codes are the half of the contract FastAPI will not read back.
    if !responses.statuses.is_empty() {
        fidelity.notes.push(format!(
            "returns status {}. FastAPI documents the status on the decorator, not the \
             one on a returned Response, so the generated OpenAPI will say 200 for these \
             endpoints unless you add `status_code=` to `@router.…` and `responses={{…}}` \
             for the rest",
            responses.statuses.join(", ")
        ));
    }

    fidelity.functions += handlers.len();
    fidelity.signatures_complete += handlers.len();

    // PEP 8 wants two blank lines between top-level definitions and no more.
    while out.contains("\n\n\n\n") {
        out = out.replace("\n\n\n\n", "\n\n\n");
    }

    Ok(Written {
        output: out,
        fidelity,
        methods,
        statuses: responses.statuses,
    })
}

/// What [`write`] produced: the Python text, and what it says about itself.
struct Written {
    output: String,
    fidelity: Fidelity,
    methods: Vec<String>,
    statuses: Vec<String>,
}

/// Which response helpers the translated handlers ended up needing.
#[derive(Default)]
struct Responses {
    json_with_status: bool,
    redirect: bool,
    plain_response: bool,
    /// Every status code a handler returns, in the order they were met.
    statuses: Vec<String>,
}

impl Responses {
    fn note_status(&mut self, status: &Expr) {
        let text = match status {
            Expr::Int(value) => value.clone(),
            Expr::Name(name) => name.clone(),
            _ => return,
        };
        if !self.statuses.contains(&text) {
            self.statuses.push(text);
        }
    }
}

/// Is this handler argument the request, instead of the route context?
fn is_the_request(param: &Param) -> bool {
    if let Some(Type::Named { name, .. }) = &param.ty {
        return name.ends_with("Request");
    }
    matches!(param.name.as_str(), "req" | "request")
}

/// Rewrite the Next.js response helpers as their FastAPI equivalents.
fn as_fastapi(stmt: Stmt, needs: &mut Responses) -> Stmt {
    fn rewrite(e: Expr, needs: &mut Responses) -> Expr {
        // `new Response(null, { status: 204 })` is the Web-standard spelling, and the App
        // Router examples use it.
        if let Expr::New { callee, args } = &e {
            if matches!(callee.as_ref(), Expr::Name(name) if name == "Response") {
                let status = args.get(1).and_then(status_in);
                if let Some(code) = &status {
                    needs.note_status(code);
                }
                needs.plain_response = true;
                let mut mapped = Vec::new();
                // The body is the first argument and `null` means there is none.
                match args.first() {
                    Some(Expr::Null) | None => {}
                    Some(body) => mapped.push(Expr::Keyword {
                        name: "content".into(),
                        value: Box::new(body.clone()),
                    }),
                }
                if let Some(code) = status {
                    mapped.push(Expr::Keyword {
                        name: "status_code".into(),
                        value: Box::new(code),
                    });
                }
                return Expr::Call {
                    callee: Box::new(Expr::Name("Response".into())),
                    args: mapped,
                };
            }
        }
        let Expr::Call { callee, mut args } = e else {
            return e;
        };
        let Expr::Field { of, name } = callee.as_ref() else {
            return Expr::Call { callee, args };
        };
        if !matches!(of.as_ref(), Expr::Name(object) if object == "NextResponse" || object == "Response")
        {
            return Expr::Call { callee, args };
        }
        match (name.as_str(), args.len()) {
            // The plain case: FastAPI serialises whatever the handler returns.
            ("json", 1) => args.remove(0),
            // With options, the status is the only one that carries; `JSONResponse` is
            // how FastAPI spells it.
            ("json", 2) => {
                let status = status_in(&args[1]);
                if let Some(code) = &status {
                    needs.note_status(code);
                }
                needs.json_with_status = true;
                Expr::Call {
                    callee: Box::new(Expr::Name("JSONResponse".into())),
                    args: vec![args.remove(0), status.unwrap_or(Expr::Int("200".into()))],
                }
            }
            ("redirect", _) if !args.is_empty() => {
                needs.redirect = true;
                Expr::Call {
                    callee: Box::new(Expr::Name("RedirectResponse".into())),
                    args: vec![args.remove(0)],
                }
            }
            _ => Expr::Call { callee, args },
        }
    }

    // This recurses into the nested bodies.
    let inside = |body: Vec<Stmt>, needs: &mut Responses| -> Vec<Stmt> {
        body.into_iter().map(|s| as_fastapi(s, needs)).collect()
    };

    match stmt {
        Stmt::Return(Some(e)) => Stmt::Return(Some(rewrite(e, needs))),
        Stmt::Expr(e) => Stmt::Expr(rewrite(e, needs)),
        Stmt::Let {
            name,
            ty,
            value: Some(e),
            mutable,
        } => Stmt::Let {
            name,
            ty,
            value: Some(rewrite(e, needs)),
            mutable,
        },
        Stmt::Assign { target, value } => Stmt::Assign {
            target,
            value: rewrite(value, needs),
        },
        Stmt::If {
            condition,
            then,
            otherwise,
        } => Stmt::If {
            condition,
            then: inside(then, needs),
            otherwise: inside(otherwise, needs),
        },
        Stmt::While { condition, body } => Stmt::While {
            condition,
            body: inside(body, needs),
        },
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => Stmt::ForEach {
            binding,
            iterable,
            body: inside(body, needs),
        },
        Stmt::Try {
            body,
            catches,
            finally,
            source,
            line,
        } => Stmt::Try {
            body: inside(body, needs),
            catches: catches
                .into_iter()
                .map(|mut clause| {
                    clause.body = clause
                        .body
                        .into_iter()
                        .map(|s| as_fastapi(s, needs))
                        .collect();
                    clause
                })
                .collect(),
            finally: inside(finally, needs),
            source,
            line,
        },
        Stmt::Throw(e) => Stmt::Throw(rewrite(e, needs)),
        other => other,
    }
}

/// The `status` out of a `{ status: 404 }` options object.
fn status_in(options: &Expr) -> Option<Expr> {
    let Expr::MapLit(entries) = options else {
        return None;
    };
    entries.iter().find_map(|(key, value)| {
        let named = match key {
            Expr::Str(text) => text.as_str(),
            Expr::Name(text) => text.as_str(),
            _ => return None,
        };
        (named == "status").then(|| value.clone())
    })
}

/// A binding of a name to itself, which is a statement that does nothing.
fn binds_itself(stmt: &Stmt) -> Option<String> {
    let Stmt::Let {
        name,
        value: Some(Expr::Name(read)),
        ..
    } = stmt
    else {
        return None;
    };
    (name == read).then(|| name.clone())
}

/// Read every `req.nextUrl.searchParams.get("species")` as the parameter FastAPI supplies.
fn supply_query_parameters(stmt: Stmt, declared: &[String]) -> Stmt {
    fn in_expr(e: Expr, declared: &[String]) -> Expr {
        if let Expr::Call { callee, args } = &e {
            if let Expr::Field { of, name } = callee.as_ref() {
                if name == "get" && from_search_params(of) {
                    if let Some(Expr::Str(key)) = args.first() {
                        let supplied = super::snake_always(key);
                        if declared.contains(&supplied) {
                            return Expr::Name(supplied);
                        }
                    }
                }
            }
        }
        match e {
            Expr::Field { of, name } => Expr::Field {
                of: Box::new(in_expr(*of, declared)),
                name,
            },
            Expr::Index { of, index } => Expr::Index {
                of: Box::new(in_expr(*of, declared)),
                index: Box::new(in_expr(*index, declared)),
            },
            Expr::Call { callee, args } => Expr::Call {
                callee: Box::new(in_expr(*callee, declared)),
                args: args.into_iter().map(|a| in_expr(a, declared)).collect(),
            },
            Expr::New { callee, args } => Expr::New {
                callee: Box::new(in_expr(*callee, declared)),
                args: args.into_iter().map(|a| in_expr(a, declared)).collect(),
            },
            Expr::Binary { op, left, right } => Expr::Binary {
                op,
                left: Box::new(in_expr(*left, declared)),
                right: Box::new(in_expr(*right, declared)),
            },
            Expr::Coalesce { value, fallback } => Expr::Coalesce {
                value: Box::new(in_expr(*value, declared)),
                fallback: Box::new(in_expr(*fallback, declared)),
            },
            Expr::Unary { op, operand } => Expr::Unary {
                op,
                operand: Box::new(in_expr(*operand, declared)),
            },
            Expr::Await(inner) => Expr::Await(Box::new(in_expr(*inner, declared))),
            Expr::Keyword { name, value } => Expr::Keyword {
                name,
                value: Box::new(in_expr(*value, declared)),
            },
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => Expr::Ternary {
                condition: Box::new(in_expr(*condition, declared)),
                then: Box::new(in_expr(*then, declared)),
                otherwise: Box::new(in_expr(*otherwise, declared)),
            },
            Expr::ListLit(items) => {
                Expr::ListLit(items.into_iter().map(|i| in_expr(i, declared)).collect())
            }
            Expr::MapLit(entries) => Expr::MapLit(
                entries
                    .into_iter()
                    .map(|(k, v)| (in_expr(k, declared), in_expr(v, declared)))
                    .collect(),
            ),
            other => other,
        }
    }

    fn in_stmts(stmts: Vec<Stmt>, declared: &[String]) -> Vec<Stmt> {
        stmts
            .into_iter()
            .map(|s| supply_query_parameters(s, declared))
            .collect()
    }

    match stmt {
        Stmt::Let {
            name,
            ty,
            value,
            mutable,
        } => Stmt::Let {
            name,
            ty,
            value: value.map(|v| in_expr(v, declared)),
            mutable,
        },
        Stmt::Assign { target, value } => Stmt::Assign {
            target: in_expr(target, declared),
            value: in_expr(value, declared),
        },
        Stmt::Return(value) => Stmt::Return(value.map(|v| in_expr(v, declared))),
        Stmt::Expr(e) => Stmt::Expr(in_expr(e, declared)),
        Stmt::Throw(e) => Stmt::Throw(in_expr(e, declared)),
        Stmt::If {
            condition,
            then,
            otherwise,
        } => Stmt::If {
            condition: in_expr(condition, declared),
            then: in_stmts(then, declared),
            otherwise: in_stmts(otherwise, declared),
        },
        Stmt::While { condition, body } => Stmt::While {
            condition: in_expr(condition, declared),
            body: in_stmts(body, declared),
        },
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => Stmt::ForEach {
            binding,
            iterable: in_expr(iterable, declared),
            body: in_stmts(body, declared),
        },
        Stmt::Try {
            body,
            catches,
            finally,
            source,
            line,
        } => Stmt::Try {
            body: in_stmts(body, declared),
            catches: catches
                .into_iter()
                .map(|c| crate::transpile::ir::Catch {
                    binding: c.binding,
                    ty: c.ty,
                    body: in_stmts(c.body, declared),
                })
                .collect(),
            finally: in_stmts(finally, declared),
            source,
            line,
        },
        other => other,
    }
}

/// Read every `context.params.petId` as the parameter FastAPI already supplies.
fn supply_path_parameters(stmt: Stmt, dropped: &[String], parameters: &[String]) -> Stmt {
    fn in_expr(e: Expr, dropped: &[String], parameters: &[String]) -> Expr {
        // `<context>.params.<field>`, and nothing else with that shape.
        if let Expr::Field { of, name: field } = &e {
            if let Expr::Field {
                of: object,
                name: params,
            } = of.as_ref()
            {
                if let Expr::Name(object) = object.as_ref() {
                    let supplied = super::snake_always(field);
                    if params == "params"
                        && dropped.contains(object)
                        && parameters.contains(&supplied)
                    {
                        return Expr::Name(supplied);
                    }
                }
            }
        }
        match e {
            Expr::Field { of, name } => Expr::Field {
                of: Box::new(in_expr(*of, dropped, parameters)),
                name,
            },
            Expr::Index { of, index } => Expr::Index {
                of: Box::new(in_expr(*of, dropped, parameters)),
                index: Box::new(in_expr(*index, dropped, parameters)),
            },
            Expr::Call { callee, args } => Expr::Call {
                callee: Box::new(in_expr(*callee, dropped, parameters)),
                args: args
                    .into_iter()
                    .map(|a| in_expr(a, dropped, parameters))
                    .collect(),
            },
            Expr::New { callee, args } => Expr::New {
                callee: Box::new(in_expr(*callee, dropped, parameters)),
                args: args
                    .into_iter()
                    .map(|a| in_expr(a, dropped, parameters))
                    .collect(),
            },
            Expr::Binary { op, left, right } => Expr::Binary {
                op,
                left: Box::new(in_expr(*left, dropped, parameters)),
                right: Box::new(in_expr(*right, dropped, parameters)),
            },
            Expr::Coalesce { value, fallback } => Expr::Coalesce {
                value: Box::new(in_expr(*value, dropped, parameters)),
                fallback: Box::new(in_expr(*fallback, dropped, parameters)),
            },
            Expr::Unary { op, operand } => Expr::Unary {
                op,
                operand: Box::new(in_expr(*operand, dropped, parameters)),
            },
            Expr::Await(inner) => Expr::Await(Box::new(in_expr(*inner, dropped, parameters))),
            Expr::Keyword { name, value } => Expr::Keyword {
                name,
                value: Box::new(in_expr(*value, dropped, parameters)),
            },
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => Expr::Ternary {
                condition: Box::new(in_expr(*condition, dropped, parameters)),
                then: Box::new(in_expr(*then, dropped, parameters)),
                otherwise: Box::new(in_expr(*otherwise, dropped, parameters)),
            },
            Expr::ListLit(items) => Expr::ListLit(
                items
                    .into_iter()
                    .map(|i| in_expr(i, dropped, parameters))
                    .collect(),
            ),
            Expr::MapLit(entries) => Expr::MapLit(
                entries
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            in_expr(k, dropped, parameters),
                            in_expr(v, dropped, parameters),
                        )
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    fn in_stmts(stmts: Vec<Stmt>, dropped: &[String], parameters: &[String]) -> Vec<Stmt> {
        stmts
            .into_iter()
            .map(|s| supply_path_parameters(s, dropped, parameters))
            .collect()
    }

    match stmt {
        Stmt::Let {
            name,
            ty,
            value,
            mutable,
        } => Stmt::Let {
            name,
            ty,
            value: value.map(|v| in_expr(v, dropped, parameters)),
            mutable,
        },
        Stmt::Assign { target, value } => Stmt::Assign {
            target: in_expr(target, dropped, parameters),
            value: in_expr(value, dropped, parameters),
        },
        Stmt::Return(value) => Stmt::Return(value.map(|v| in_expr(v, dropped, parameters))),
        Stmt::Expr(e) => Stmt::Expr(in_expr(e, dropped, parameters)),
        Stmt::Throw(e) => Stmt::Throw(in_expr(e, dropped, parameters)),
        Stmt::If {
            condition,
            then,
            otherwise,
        } => Stmt::If {
            condition: in_expr(condition, dropped, parameters),
            then: in_stmts(then, dropped, parameters),
            otherwise: in_stmts(otherwise, dropped, parameters),
        },
        Stmt::While { condition, body } => Stmt::While {
            condition: in_expr(condition, dropped, parameters),
            body: in_stmts(body, dropped, parameters),
        },
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => Stmt::ForEach {
            binding,
            iterable: in_expr(iterable, dropped, parameters),
            body: in_stmts(body, dropped, parameters),
        },
        Stmt::Try {
            body,
            catches,
            finally,
            source,
            line,
        } => Stmt::Try {
            body: in_stmts(body, dropped, parameters),
            catches: catches
                .into_iter()
                .map(|c| crate::transpile::ir::Catch {
                    binding: c.binding,
                    ty: c.ty,
                    body: in_stmts(c.body, dropped, parameters),
                })
                .collect(),
            finally: in_stmts(finally, dropped, parameters),
            source,
            line,
        },
        other => other,
    }
}

/// Is this statement just pulling a path parameter off the Next.js context?
fn supplied_by_fastapi(stmt: &Stmt, dropped: &[String], parameters: &[String]) -> Option<String> {
    let Stmt::Let {
        name,
        value: Some(Expr::Field { of, name: field }),
        ..
    } = stmt
    else {
        return None;
    };
    let Expr::Field {
        of: object,
        name: params,
    } = of.as_ref()
    else {
        return None;
    };
    let Expr::Name(object) = object.as_ref() else {
        return None;
    };
    (params == "params" && dropped.contains(object) && field == name && parameters.contains(name))
        .then(|| name.clone())
}

/// Does this statement read one of these names?
fn reads(stmt: &Stmt, names: &[String]) -> bool {
    fn in_expr(e: &Expr, names: &[String]) -> bool {
        match e {
            Expr::Name(n) => names.iter().any(|d| d == n),
            Expr::Field { of, .. } => in_expr(of, names),
            Expr::Index { of, index } => in_expr(of, names) || in_expr(index, names),
            Expr::Call { callee, args } => {
                in_expr(callee, names) || args.iter().any(|a| in_expr(a, names))
            }
            Expr::Binary { left, right, .. } => in_expr(left, names) || in_expr(right, names),
            Expr::Unary { operand, .. } => in_expr(operand, names),
            Expr::ListLit(items) => items.iter().any(|i| in_expr(i, names)),
            Expr::MapLit(entries) => entries
                .iter()
                .any(|(k, v)| in_expr(k, names) || in_expr(v, names)),
            Expr::Template(parts) => parts.iter().any(|p| match p {
                TemplatePart::Expr(e) => in_expr(e, names),
                TemplatePart::Text(_) => false,
            }),
            Expr::Comprehension {
                element,
                iterable,
                condition,
                ..
            } => {
                in_expr(element, names)
                    || in_expr(iterable, names)
                    || condition.as_ref().is_some_and(|c| in_expr(c, names))
            }
            _ => false,
        }
    }

    match stmt {
        Stmt::Return(Some(e)) | Stmt::Expr(e) => in_expr(e, names),
        Stmt::Let { value: Some(e), .. } => in_expr(e, names),
        Stmt::Assign { target, value } => in_expr(target, names) || in_expr(value, names),
        Stmt::If { condition, .. } | Stmt::While { condition, .. } => in_expr(condition, names),
        Stmt::ForEach { iterable, .. } => in_expr(iterable, names),
        Stmt::Throw(e) => in_expr(e, names),
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            body.iter().any(|s| reads(s, names))
                || catches
                    .iter()
                    .any(|c| c.body.iter().any(|s| reads(s, names)))
                || finally.iter().any(|s| reads(s, names))
        }
        _ => false,
    }
}

/// The `{name}` parameters a route declares, in order.
pub fn path_parameters(route: &str) -> Vec<String> {
    route
        .split('/')
        .filter_map(|segment| {
            let inner = segment.strip_prefix('{')?.strip_suffix('}')?;
            Some(inner.split(':').next().unwrap_or(inner).to_string())
        })
        .collect()
}

/// Turn the plain classes the Python writer produced into Pydantic models.
fn promote_models(body: &str, module: &Module) -> String {
    let records: Vec<&str> = module
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Record(r) => Some(r.name.as_str()),
            _ => None,
        })
        .collect();
    let mut out = body.to_string();
    for name in records {
        out = out.replace(
            &format!("class {name}:"),
            &format!("class {name}(BaseModel):"),
        );
    }
    out
}
