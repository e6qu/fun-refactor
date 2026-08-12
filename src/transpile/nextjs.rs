//! Next.js API routes as FastAPI endpoints.
//!
//! # What corresponds
//!
//! A React component renders UI in a browser and a FastAPI endpoint answers HTTP, so this
//! declines a `.tsx` file full of JSX and gives the reason.
//!
//! A Next.js API route and a FastAPI endpoint correspond exactly:
//!
//! | Next.js | FastAPI | | --- | --- | | `app/api/users/route.ts` exporting `GET` |
//! `@router.get("/users")` | | `app/api/users/[id]/route.ts` | `@router.get("/users/{id}")` | |
//! `app/api/files/[...path]/route.ts` | `@router.get("/files/{path:path}")` | | `export async
//! function POST` | `@router.post(...)` on an `async def` | | an exported `interface` | a
//! Pydantic `BaseModel` |
//!
//! # The path comes from the file
//!
//! A Next.js route's URL is its position on disk: `app/api/users/[id]/route.ts` is
//! `/users/{id}`. Nothing inside the file says so. This translation reads the path as well as
//! the text, so it lives here and not in the general reader.
//!
//! # What it still cannot do
//!
//! The handler bodies are TypeScript, translated by the ordinary reader with the ordinary
//! limits. `NextResponse.json(...)`, `await request.json()` and database calls have no Python
//! counterpart and land in the output as comments. The result is a FastAPI skeleton, routes,
//! methods, path parameters, models, with the logic beside it to port by hand.

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
    /// The methods found, in the order they were declared.
    pub methods: Vec<String>,
    pub output: String,
    pub fidelity: Fidelity,
    /// The write, unapplied, so a caller can show it before committing to it.
    pub edits: crate::edit::EditSet,
    /// The shapes this route declares, as an OpenAPI document would name them.
    pub models: Vec<Model>,
    /// Which schema each handler validates its body against.
    ///
    /// `(method, schema name)`, from `petCreateSchema.parse(json)` inside `POST`. The
    /// contract needs the *link*, not only the shape: a document with a `components`
    /// section nothing refers to says the endpoints take no body at all, which is a
    /// smaller contract than the one it stands in for.
    pub bodies: Vec<(String, String)>,
    /// The query parameters each handler reads.
    ///
    /// `(method, name)`, from `req.nextUrl.searchParams.get("limit")`. Next.js declares nothing
    /// about them. They are read out of the URL by hand. So a contract built without them says
    /// the endpoint takes no query at all. A caller passing `?limit=10` is outside a contract
    /// that claims to describe it.
    pub queries: Vec<(String, String)>,
}

/// A named shape a route declares, from an exported `interface` or a zod schema.
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    /// Field name and its type, spelled as the IR sees it.
    pub fields: Vec<(String, Option<Type>)>,
}

/// The path segments that make up a route's URL, if this file is one.
///
/// Both routers, matched on path *components*: an `api` directory somewhere above an App Router
/// `route.ts`, or a `pages/api` pair for the Pages Router. Component-wise and not by substring,
/// because a substring rule needs a leading slash to avoid matching `capi/`. Requiring one
/// silently rejects every relative path, which a caller who passes `pages/api/users.ts`
/// hands over.
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
///
/// Both routers: `app/**/route.ts` under an `api` segment, and anything under
/// `pages/api/`.
pub fn is_api_route(path: &Path) -> bool {
    route_segments(path).is_some()
}

/// The URL a route file serves, derived from where it sits.
///
/// `app/api/users/[id]/route.ts` → `/users/{id}`
/// `app/api/files/[...path]/route.ts` → `/files/{path:path}`
/// `pages/api/users.ts` → `/users`
pub fn route_for(path: &Path) -> String {
    let segments = route_segments(path).unwrap_or_default();
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

/// Does this file contain JSX. That is, is it a component instead of a route?
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
    // The placeholder's name is internal, `/posts/{postId}` and `/posts/{post_id}` serve
    // exactly the same URLs. So it takes the target's convention like every other name FastAPI
    // will see.
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

    if !is_api_route(path) {
        bail!(
            "{} is not a Next.js API route. Those are `app/**/api/**/route.ts` or \
             anything under `pages/api/`. The URL comes from where the file sits, so this \
             command needs the path and not only the contents.",
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
    let (output, fidelity, methods) = write(&module, &route, path)?;

    // The declared shapes, from either place a Next.js route keeps them.
    let models: Vec<Model> = models_of(&module);

    let bodies = parsed_bodies(&module);
    let queries = read_queries(&module);

    if methods.is_empty() {
        bail!(
            "{} exports no HTTP method. An App Router route exports `GET`, `POST` and \
             so on by name; a Pages Router one exports a default handler, which this \
             does not yet read.",
            path.display()
        );
    }

    // The output has to be a Python file Python accepts. `transpile::plan` checks its
    // own output for the same reason: an unparseable result is a defect here, not in
    // the caller's file, and should say so.
    let written = parsers.parse(Language::Python, &output)?;
    if written.has_errors() {
        bail!(
            "the FastAPI module this produced does not parse as Python. That is a defect \
             in the translator; nothing was written.\n\n{output}"
        );
    }

    let destination = path.with_file_name(format!(
        "{}.py",
        route
            .trim_matches('/')
            .replace(['/', '{', '}', ':'], "_")
            .split('_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_")
            .then_or("index")
    ));

    if crate::vfs::exists(&destination) {
        bail!(
            "{} already exists; translating {} would overwrite it",
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

    Ok(RoutePlan {
        source: path.to_path_buf(),
        destination,
        edits,
        route,
        methods,
        output,
        fidelity,
        models,
        bodies,
        queries,
    })
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
///
/// Most Next.js applications declare their shapes with zod and not with an
/// `interface`, and a zod schema is a *runtime value*. Nothing that reads type
/// declarations will find it. Left alone it arrives as an ordinary constant and the
/// translated service publishes an OpenAPI document with no request body in it at all:
/// the endpoint works and the contract it advertises is smaller than the one it
/// replaced. That is the failure `API_CONTRACTS.md` is about, and this is the half of
/// it that can be fixed by reading harder.
///
/// Every shape a TypeScript module declares, wherever the module sits.
///
/// A real Next.js application keeps its zod schemas in a module the routes import, so
/// reading only the route file found none of them. This reads any `.ts` file, which is
/// what lets the contract carry a body declared in `lib/schemas.ts`.
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
///
/// `req.nextUrl.searchParams.get("limit")`, and the two other spellings of the same
/// thing. Next.js has no declaration for these, the handler reaches into the URL, so
/// a reader that only looks at declarations finds nothing and the contract comes out
/// claiming the endpoint takes no query.
fn read_queries(module: &Module) -> Vec<(String, String)> {
    /// Does this expression reach through `searchParams`?
    fn from_search_params(e: &Expr) -> bool {
        match e {
            Expr::Name(name) => name == "searchParams",
            Expr::Field { of, name } => name == "searchParams" || from_search_params(of),
            Expr::Call { callee, .. } => from_search_params(callee),
            _ => false,
        }
    }

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

/// Which schema each handler parses its body with.
///
/// A Next.js handler validates by hand — `const body = petCreateSchema.parse(json)`,
/// so the link between an operation and its request body is a *call* and not a
/// declaration. Reading it is what lets the contract say `requestBody` instead of
/// listing a shape under `components` that nothing points at.
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
                    exported: true,
                })
            })
            .collect(),
    )
}

/// Is this `z.<method>`, however many builders are wrapped around it?
fn is_zod(callee: &Expr, method: &str) -> bool {
    matches!(callee, Expr::Field { of, name }
        if name == method && matches!(of.as_ref(), Expr::Name(z) if z == "z"))
}

/// The type a zod builder chain describes.
///
/// A chain is left-nested, `z.string().min(3).optional()` is `optional(max(min(string)))`,
/// so this walks to the base call and collects the modifiers on the way past. The
/// constraints (`.min`, `.max`, `.email`) are *not* carried: Pydantic spells them with
/// `Field(...)` and inventing one from a zod call is a guess about validation, which is
/// the part of a contract it is least safe to guess at.
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
fn write(module: &Module, route: &str, source: &Path) -> Result<(String, Fidelity, Vec<String>)> {
    // What each handler reads out of the URL. Read here as well as in `plan`, because the
    // contract and the code have to agree about it. A parameter the document mentions and the
    // router does not declare is the same failure as the reverse.
    let query_keys = read_queries(module);
    // The handlers are the exported functions named after HTTP methods; everything
    // else in the file is a helper and is written as an ordinary function.
    let mut handlers = Vec::new();
    let mut rest = Module {
        doc: module.doc.clone(),
        name: module.name.clone(),
        items: Vec::new(),
    };
    for item in &module.items {
        match item {
            Item::Function(f) if METHODS.contains(&f.name.to_uppercase().as_str()) => {
                handlers.push(f.clone());
            }
            // `import { NextResponse } from "next/server"` is the one import that does
            // *not* become work for the reader: its uses are what this
            // translated away. Listing it under "the equivalent here is yours to add"
            // would point at a job already done.
            Item::Import { text, .. } if text.contains("\"next/") || text.contains("'next/") => {}
            // A zod schema is a shape. It is not a value. It is where most Next.js applications
            // keep the one thing FastAPI most wants: the request body.
            Item::Constant(c) => match record_from_zod(&c.name, &c.value) {
                Some(record) => rest.items.push(Item::Record(record)),
                None => rest.items.push(item.clone()),
            },
            other => rest.items.push(other.clone()),
        }
    }

    let methods: Vec<String> = handlers.iter().map(|h| h.name.to_uppercase()).collect();

    // Everything that is not a handler goes through the ordinary Python writer, which turns
    // interfaces into dataclasses. Those are then promoted to Pydantic models, because a
    // request body in FastAPI is a `BaseModel`.
    let (body, mut fidelity) = super::write_module(Language::Python, &rest)?;
    let body = body
        .replace("from dataclasses import dataclass", "")
        .replace("@dataclass", "");

    let mut out = String::new();
    out.push_str(&format!(
        "# Translated from a Next.js API route ({}) by fun-refactor.\n",
        source.display()
    ));
    out.push_str(&format!(
        "# Route: {route}, {} handler(s): {}\n",
        handlers.len(),
        methods.join(", ")
    ));
    // Filled in at the end: whether this is a skeleton is not knowable until the handlers are
    // written. A banner that says SKELETON over a file with nothing carried is the kind of
    // hedge that stops anyone reading the banner at all.
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

    // The models and helpers, with records turned into Pydantic models.
    let with_models = promote_models(&body, &rest);
    if !with_models.trim().is_empty() {
        out.push_str(with_models.trim_start());
        out.push_str("\n\n");
    }

    // The path parameters the route declares, which every handler receives.
    let parameters = path_parameters(route);

    // Which imports the file will need. Collected while writing the handlers, because that is
    // when it becomes known. Emitted into the header afterwards, the alternative is importing
    // `Request` and `JSONResponse` unconditionally. An unused import in generated code is a
    // thing the reader has to decide about.
    let mut takes_request = false;
    let mut responses = Responses::default();

    for handler in &handlers {
        let method = handler.name.to_lowercase();
        out.push('\n');
        out.push_str(&format!("@router.{method}(\"{route}\")\n"));

        let mut signature: Vec<String> = parameters
            .iter()
            .map(|name| format!("{name}: str"))
            .collect();

        // The query parameters this handler reads out of the URL, declared.
        //
        // Next.js has no declaration for them, so a translation that left the handler
        // reaching into the request produced a router that says it takes no query, and
        // a caller sending `?species=cat` is then outside a contract that claims to
        // describe it. `searchParams.get()` returns `string | null`, which is what
        // `str | None = None` says, so the endpoint answers exactly the URLs it did.
        let declared_queries: Vec<String> = query_keys
            .iter()
            .filter(|(m, _)| m.eq_ignore_ascii_case(&handler.name))
            .map(|(_, key)| (key.clone(), super::snake_always(key)))
            .filter(|(key, name)| {
                // A key that is not a name, `page-size`, `filter[]`, cannot be
                // declared, and inventing a spelling for it would answer a different
                // URL. Those stay where they are and are reported.
                let spellable = !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !name.starts_with(|c: char| c.is_ascii_digit());
                let free =
                    !parameters.contains(name) && !handler.params.iter().any(|p| p.name == *name);
                if !spellable {
                    fidelity.notes.push(format!(
                        "`{}` reads the query key `{key}`, which is not a name Python can \
                         declare; that read is left as it was and the endpoint's contract \
                         does not mention it",
                        handler.name
                    ));
                }
                spellable && free
            })
            .map(|(_, name)| name)
            .collect();

        // A Next.js handler takes `(request, context)`. The two arguments fare differently, and
        // treating them alike was wrong: `NextRequest` **is** Starlette's `Request`, same
        // headers, same `await .json()`. So it is kept under its own name and typed, which
        // makes every line that reads it correct and not commented out. `context` genuinely has
        // no counterpart, because FastAPI passes path parameters directly.
        let request = handler.params.iter().find(|p| is_the_request(p));
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
            method,
            signature.join(", ").trim()
        ));

        // A Next.js handler receives `(request, context)`; FastAPI passes path parameters
        // directly and has neither object. A statement that *reads* one of them cannot be
        // translated, `const id = context.params.id` became `id = context.params.id`, referring
        // to something that does not exist. So it is carried with the rest.
        let dropped: Vec<String> = handler
            .params
            .iter()
            .filter(|p| !is_the_request(p))
            .map(|p| p.name.clone())
            .collect();
        // `const id = context.params.id` is not untranslatable. It is *redundant*. Pulling a
        // path parameter off the context object is exactly the work FastAPI does for you, so
        // the line is dropped and not carried, and the report says why. It is the single most
        // common statement in a Next.js route. Carrying it would leave every translated handler
        // opening with a line that names an object Python does not have.
        let mut body = Vec::new();
        for stmt in &handler.body {
            match supplied_by_fastapi(stmt, &dropped, &parameters) {
                Some(name) => fidelity.notes.push(format!(
                    "`{}` read `{name}` off the Next.js context; FastAPI supplies it as a \
                     path parameter, so that line is not needed and was dropped",
                    handler.name
                )),
                None => {
                    let supplied = supply_path_parameters(stmt.clone(), &dropped, &parameters);
                    let supplied = supply_query_parameters(supplied, &declared_queries);
                    // `const species = req.…searchParams.get("species")` becomes `species =
                    // species` once the parameter supplies it. A binding of a name to itself is
                    // a statement that does nothing.
                    match binds_itself(&supplied) {
                        Some(name) => fidelity.notes.push(format!(
                            "`{}` read `{name}` out of the query string; FastAPI supplies \
                             it as a parameter, so that line is not needed and was dropped",
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
                is_constructor: false,
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

        // The lines below are faithful to the source and refer to objects FastAPI does
        // not have. Saying so once beats replacing each of them with a placeholder
        // that shows less than the translation does. It goes *after* the docstring:
        // a string that is not the first statement is not a docstring.
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

    // `+=`, not `=`: a route file may declare helpers beside its handlers, and the ordinary
    // writer already counted those. A handler's own signature is complete by construction — its
    // parameters are the route's. Every one of them is typed, so it counts as one, which is
    // what stops the report reading `0/2 complete` for a translation that got every signature
    // right. What the handlers turned out to need. Written last and patched in, because it is
    // only knowable after they are written. Importing `Request` and `JSONResponse`
    // unconditionally leaves unused imports in generated code for the reader to puzzle over.
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
    // A model derived from `z.date()` is annotated `datetime`. A name the output uses is a name
    // the output has to import, generated code that references something undefined is worse
    // than generated code that admits a gap.
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
            "# The routes, methods, path parameters and models are translated. Some of\n\
             # the handler bodies had no Python counterpart and are below as comments,\n\
             # marked, for you to finish. THIS FILE IS A DRAFT.\n\n"
        },
    );

    // PEP 8 wants two blank lines before a top-level definition. The pieces above are
    // written independently, so the seams are only right once, here.
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

    // PEP 8 wants two blank lines between top-level definitions and no more. The
    // pieces above are written independently and each ends with its own spacing, so
    // the seams accumulate; normalising once here beats making every piece guess what
    // follows it.
    while out.contains("\n\n\n\n") {
        out = out.replace("\n\n\n\n", "\n\n\n");
    }

    Ok((out, fidelity, methods))
}

/// Which response helpers the translated handlers ended up needing.
#[derive(Default)]
struct Responses {
    json_with_status: bool,
    redirect: bool,
    plain_response: bool,
    /// Every status code a handler returns, in the order they were met.
    ///
    /// Collected because FastAPI does not read them. Its OpenAPI document takes the
    /// status from the *decorator*, `@router.delete("…", status_code=204)`, and a
    /// status on a returned `Response` changes what the endpoint does without changing
    /// what it says it does. A rewrite that preserves the behaviour and quietly
    /// shrinks the published contract is the one failure this whole exercise is for.
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
///
/// By type where there is one, `Request`, `NextRequest`, and by name otherwise,
/// since `(req, res)` and `(request, context)` are the two spellings Next.js uses.
fn is_the_request(param: &Param) -> bool {
    if let Some(Type::Named { name, .. }) = &param.ty {
        return name.ends_with("Request");
    }
    matches!(param.name.as_str(), "req" | "request")
}

/// Rewrite the Next.js response helpers as their FastAPI equivalents.
///
/// These are not approximations. Returning a value from a FastAPI handler *is*
/// `NextResponse.json`, the framework serialises it, so `return NextResponse.json(x)`
/// is `return x` and nothing is lost. Where a status or a redirect is involved FastAPI
/// has a named class for it, which is the idiom and not a workaround.
fn as_fastapi(stmt: Stmt, needs: &mut Responses) -> Stmt {
    fn rewrite(e: Expr, needs: &mut Responses) -> Expr {
        // `new Response(null, { status: 204 })` is the Web-standard spelling and is what the
        // App Router examples use. FastAPI's own `Response` takes the status as a keyword. A
        // positional second argument would be the body.
        if let Expr::New { callee, args } = &e {
            if matches!(callee.as_ref(), Expr::Name(name) if name == "Response") {
                let status = args.get(1).and_then(status_in);
                if let Some(code) = &status {
                    needs.note_status(code);
                }
                needs.plain_response = true;
                let mut mapped = Vec::new();
                // The body is the first argument and `null` means there is none.
                // Dropping it would lose the payload silently, which is the one
                // outcome worse than not translating the call at all.
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

    // Recursive over the nested bodies: a `return NextResponse.json(...)` inside an `if` is the
    // commonest error branch in a Next.js route. Rewriting only the top level left exactly
    // those untranslated.
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
///
/// The same move as the path parameters, one level out. The value arrives by a different route
/// and every use of it has to arrive with it. Leaving the read in place would be worse than not
/// declaring the parameter at all, `req.nextUrl` is a Next.js object and Starlette's `Request`
/// does not have it, so the line would not run.
fn supply_query_parameters(stmt: Stmt, declared: &[String]) -> Stmt {
    fn reaches_search_params(e: &Expr) -> bool {
        match e {
            Expr::Name(name) => name == "searchParams",
            Expr::Field { of, name } => name == "searchParams" || reaches_search_params(of),
            Expr::Call { callee, .. } => reaches_search_params(callee),
            _ => false,
        }
    }

    fn in_expr(e: Expr, declared: &[String]) -> Expr {
        if let Expr::Call { callee, args } = &e {
            if let Expr::Field { of, name } = callee.as_ref() {
                if name == "get" && reaches_search_params(of) {
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
///
/// Dropping the *statement* `const id = context.params.id` was only half of it. A handler that
/// writes `context.params.petId` inline, inside a `where` clause, inside a call, kept naming an
/// object Python does not have. The path parameter arrives by a different route in FastAPI and
/// every use of it has to arrive with it, which makes the endpoint answer the same URL
/// with the same value.
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
///
/// `const id = context.params.id`, where `context` is one of the arguments FastAPI
/// does not pass and `id` is a parameter the route declares. Returns the name.
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
///
/// A request or response body in FastAPI is a `BaseModel`; that makes it
/// validated and documented, and it is the whole reason to declare it.
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
