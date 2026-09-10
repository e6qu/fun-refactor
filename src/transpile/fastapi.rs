//! FastAPI endpoints as a Next.js App Router tree.

use super::ir::*;
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// The HTTP methods a FastAPI decorator names.
const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// One decorated handler, as the source declares it.
struct Endpoint {
    /// The method in the target's spelling: `GET`.
    method: String,
    /// The URL as FastAPI writes it: `/pets/{pet_id}`.
    path: String,
    handler: Function,
}

/// One file the translation writes.
#[derive(Debug)]
pub struct RouteFile {
    pub destination: PathBuf,
    /// The URL this file serves, in Next.js's spelling: `/pets/[petId]`.
    pub route: String,
    /// The methods it exports, in declaration order.
    pub methods: Vec<String>,
    pub output: String,
}

/// A FastAPI module translated into a route tree.
#[derive(Debug)]
pub struct AppPlan {
    pub source: PathBuf,
    pub routes: Vec<RouteFile>,
    pub endpoints: Vec<(String, String)>,
    pub models: Vec<Record>,
    pub fidelity: Fidelity,
    /// What the module declared that a route tree has no place for.
    pub notes: Vec<String>,
    /// The writes, unapplied, so a caller can show them before committing to them.
    pub edits: crate::edit::EditSet,
}

/// Is this file a FastAPI application?
pub fn is_fastapi_module(source: &str) -> Result<bool> {
    let parsed = Parsers::new().parse(Language::Python, source)?;
    if parsed.has_errors() {
        return Ok(false);
    }
    Ok(!endpoint_nodes(parsed.root(), source).is_empty())
}

/// Translate a FastAPI module into a Next.js route tree.
pub fn plan_to(path: &Path, out: Option<&Path>, force: bool) -> Result<AppPlan> {
    crate::capabilities::record(crate::capabilities::Capability::Translate, Language::Python);
    let Some(language) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if language != Language::Python {
        bail!(
            "{} is {language}. A FastAPI application is Python.",
            path.display()
        );
    }

    let source = crate::vfs::read_to_string(path)?;
    let parsers = Parsers::new();
    let parsed = parsers.parse(Language::Python, &source)?;
    if parsed.has_errors() {
        bail!(
            "{} does not parse cleanly, so anything read out of it would be a guess",
            path.display()
        );
    }

    let found = endpoint_nodes(parsed.root(), &source);
    if found.is_empty() {
        bail!(
            "{} declares no endpoint. A FastAPI handler carries a decorator naming its \
             method and its URL, `@router.get(\"/pets\")`. A Next.js route tree stands on \
             those URLs, so a module without one has nothing to place.",
            path.display()
        );
    }

    let module = super::read_module(Language::Python, &source, parsed.root())?;
    let models: Vec<Record> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(record) => Some(record.clone()),
            _ => None,
        })
        .collect();

    let mut endpoints = Vec::new();
    let mut notes = Vec::new();
    for (method, url, node) in found {
        let handler = super::read::function_at(Language::Python, &source, node)?;
        for param in &handler.params {
            if depends_on_the_framework(param) {
                notes.push(format!(
                    "{}: FastAPI fills `{}` in, and a Next.js handler has no \
                     equivalent. It stays a parameter for you to supply.",
                    handler.name, param.name
                ));
            }
        }
        endpoints.push(Endpoint {
            method: method.to_ascii_uppercase(),
            path: url,
            handler,
        });
    }

    // One file per URL, holding every method declared for it.
    let mut by_route: BTreeMap<String, Vec<Endpoint>> = BTreeMap::new();
    for endpoint in endpoints {
        by_route
            .entry(endpoint.path.clone())
            .or_default()
            .push(endpoint);
    }

    let root = match out {
        Some(out) => out.to_path_buf(),
        None => path.parent().unwrap_or(Path::new(".")).join("app"),
    };

    // The handlers, so the writer spells their names the target's way.
    let mut context = module.clone();
    context.items.extend(
        by_route
            .values()
            .flatten()
            .map(|endpoint| Item::Function(endpoint.handler.clone())),
    );

    let migrated_models = reachable_models(
        by_route
            .values()
            .flatten()
            .map(|endpoint| &endpoint.handler),
        &models,
    )
    .into_iter()
    .cloned()
    .collect::<Vec<_>>();
    let mut routes = Vec::new();
    let mut fidelity = Fidelity {
        functions: by_route.values().map(|group| group.len()).sum(),
        records: migrated_models.len(),
        ..Fidelity::default()
    };
    let mut edits = crate::edit::EditSet::new();
    for (url, group) in &by_route {
        let (file, report) = write_route(url, group, &models, &root, &context)?;
        let written = parsers.parse(Language::TypeScript, &file.output)?;
        if written.has_errors() {
            bail!(
                "the route this produced does not parse as TypeScript. That is a defect \
                 in the translator; this wrote nothing.\n\n{}",
                file.output
            );
        }
        if crate::vfs::exists(&file.destination) && !force {
            bail!(
                "{} already exists; translating {} would overwrite it. --force \
                 overwrites, --out chooses another directory.",
                file.destination.display(),
                path.display()
            );
        }
        // Only what the source brought.
        fidelity.carried_verbatim += report.carried_verbatim;
        fidelity.imports_listed += report.imports_listed;
        fidelity.notes.extend(report.notes);
        edits.add(
            file.destination.clone(),
            crate::edit::Edit::new(
                crate::span::Span::new(0, 0),
                &file.output,
                format!("translate {} to a Next.js route", path.display()),
            ),
        );
        edits.declare_language(file.destination.clone(), Language::TypeScript);
        routes.push(file);
    }
    let endpoints = by_route
        .values()
        .flatten()
        .map(|endpoint| (endpoint.method.clone(), endpoint.path.clone()))
        .collect();
    Ok(AppPlan {
        source: path.to_path_buf(),
        routes,
        endpoints,
        models: migrated_models,
        fidelity,
        notes,
        edits,
    })
}

/// Reports whether this parameter is one the framework supplies.
fn depends_on_the_framework(param: &Param) -> bool {
    matches!(&param.default, Some(Expr::Call { callee, .. })
        if matches!(&**callee, Expr::Name(name) if name == "Depends"))
}

/// Every `@router.get("/x")`-decorated definition, as (method, url, function node).
fn endpoint_nodes<'a>(root: Node<'a>, source: &str) -> Vec<(String, String, Node<'a>)> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    let mut cursor = root.walk();
    while let Some(node) = stack.pop() {
        if node.kind() == "decorated_definition" {
            let children: Vec<Node> = node.children(&mut cursor).collect();
            let route = children
                .iter()
                .filter(|child| child.kind() == "decorator")
                .find_map(|child| route_decorator(&source[child.byte_range()]));
            let inner = children
                .iter()
                .find(|child| child.kind() == "function_definition");
            if let (Some((method, url)), Some(function)) = (route, inner) {
                found.push((method, url, *function));
                continue;
            }
        }
        stack.extend(node.children(&mut cursor));
    }
    found.sort_by_key(|(_, _, node)| node.start_byte());
    found
}

/// The method and URL a decorator names, when it names both.
fn route_decorator(text: &str) -> Option<(String, String)> {
    let text = text.trim().strip_prefix('@')?;
    let (receiver, rest) = text.split_once('.')?;
    if receiver.is_empty() {
        return None;
    }
    let (method, arguments) = rest.split_once('(')?;
    let method = method.trim();
    if !METHODS.contains(&method) {
        return None;
    }
    let url = arguments.trim_start();
    let quote = url.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let url = &url[1..];
    let end = url.find(quote)?;
    Some((method.to_string(), url[..end].to_string()))
}

/// `{item_id}` → `item_id`, `{path:path}` → `path`, in URL order.
fn path_parameters(url: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut rest = url;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        let (name, catch_all) = match inner.split_once(':') {
            Some((name, kind)) => (name, kind == "path"),
            None => (inner, false),
        };
        found.push((name.to_string(), catch_all));
        rest = &rest[open + close + 1..];
    }
    found
}

/// The URL, in the spelling a Next.js route tree uses for its directories.
fn route_directory(url: &str) -> PathBuf {
    let mut directory = PathBuf::new();
    for segment in url.trim_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
            directory.push(segment);
            continue;
        };
        let (name, catch_all) = match inner.split_once(':') {
            Some((name, kind)) => (name, kind == "path"),
            None => (inner, false),
        };
        directory.push(match catch_all {
            true => format!("[...{name}]"),
            false => format!("[{name}]"),
        });
    }
    directory
}

/// The URL as this tool reports it, in Next.js's spelling.
fn route_display(url: &str) -> String {
    let directory = route_directory(url);
    match directory.to_string_lossy().as_ref() {
        "" => "/".to_string(),
        segments => format!("/{segments}"),
    }
}

/// Write one route file: every method declared for one URL.
fn write_route(
    url: &str,
    group: &[Endpoint],
    models: &[Record],
    root: &Path,
    context: &Module,
) -> Result<(RouteFile, Fidelity)> {
    let parameters = path_parameters(url);
    let mut items = Vec::new();

    // The shapes the handlers name.
    let named = reachable_models(group.iter().map(|endpoint| &endpoint.handler), models);
    for model in &named {
        let mut model = (*model).clone();
        model.exported = true;
        // `class Pet(BaseModel)` says "this is a shape FastAPI validates", and the shape is
        // what crosses.
        if model.extends.as_deref() == Some("BaseModel") {
            model.extends = None;
        }
        items.push(Item::Record(model));
    }

    if !parameters.is_empty() {
        items.push(Item::Record(Record {
            doc: vec!["The path parameters, named by the directories they sit in.".into()],
            name: "RouteParams".into(),
            fields: parameters
                .iter()
                .map(|(name, _)| Field {
                    doc: Vec::new(),
                    name: name.clone(),
                    ty: Some(Type::String),
                    default: None,
                    exported: true,
                })
                .collect(),
            extends: None,
            exported: false,
            methods: Vec::new(),
        }));
    }
    items.push(Item::Record(Record {
        doc: vec!["What the framework hands a handler beside the request.".into()],
        name: "RouteContext".into(),
        fields: vec![Field {
            doc: Vec::new(),
            name: "params".into(),
            ty: Some(match parameters.is_empty() {
                true => Type::Map(Box::new(Type::String), Box::new(Type::String)),
                false => Type::named("RouteParams"),
            }),
            default: None,
            exported: true,
        }],
        extends: None,
        exported: false,
        methods: Vec::new(),
    }));

    let mut methods = Vec::new();
    for endpoint in group {
        items.push(Item::Function(handler_for(endpoint, &parameters, models)));
        methods.push(endpoint.method.clone());
    }

    let module = Module {
        doc: vec![format!(
            "{}, translated from a FastAPI handler.",
            route_display(url)
        )],
        name: Some("route".into()),
        items,
        sweep_notes: Vec::new(),
    };
    let (mut output, mut fidelity) =
        super::write_module_in_preserving_fields(Language::TypeScript, &module, context)?;
    add_body_validation(&mut output, group, &parameters, models, &mut fidelity)?;

    Ok((
        RouteFile {
            destination: root.join(route_directory(url)).join("route.ts"),
            route: route_display(url),
            methods,
            output,
        },
        fidelity,
    ))
}

fn reachable_models<'m, 'h>(
    handlers: impl IntoIterator<Item = &'h Function>,
    models: &'m [Record],
) -> Vec<&'m Record> {
    let mut names = BTreeSet::new();
    for handler in handlers {
        for param in &handler.params {
            collect_type_names(param.ty.as_ref(), &mut names);
        }
        collect_type_names(handler.returns.as_ref(), &mut names);
    }
    loop {
        let before = names.len();
        for model in models {
            if !names.contains(&model.name) {
                continue;
            }
            for field in &model.fields {
                collect_type_names(field.ty.as_ref(), &mut names);
            }
        }
        if names.len() == before {
            break;
        }
    }
    models
        .iter()
        .filter(|model| names.contains(&model.name))
        .collect()
}

fn collect_type_names(ty: Option<&Type>, names: &mut BTreeSet<String>) {
    let mut stack = ty.into_iter().collect::<Vec<_>>();
    while let Some(ty) = stack.pop() {
        match ty {
            Type::Named { name, args } => {
                names.insert(name.clone());
                stack.extend(args);
            }
            Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => stack.push(inner),
            Type::Map(key, value) => {
                stack.push(key);
                stack.push(value);
            }
            Type::Tuple(parts) => stack.extend(parts),
            Type::Fn { params, returns } => {
                stack.extend(params);
                stack.push(returns);
            }
            Type::Unit | Type::Bool | Type::Int | Type::Float | Type::String => {}
        }
    }
}

/// The Next.js handler for one endpoint.
fn handler_for(endpoint: &Endpoint, parameters: &[(String, bool)], models: &[Record]) -> Function {
    let mut body = Vec::new();
    for param in &endpoint.handler.params {
        if depends_on_the_framework(param) {
            continue;
        }
        if let Some(statement) = read_parameter(param, parameters, models) {
            body.push(statement);
        }
    }
    body.extend(endpoint.handler.body.iter().cloned());
    respond(&mut body);

    Function {
        doc: endpoint.handler.doc.clone(),
        name: endpoint.method.clone(),
        receiver: None,
        receiver_binding: None,
        params: vec![
            Param {
                name: "request".into(),
                ty: Some(Type::named("Request")),
                default: None,
                kind: ParamKind::Normal,
            },
            Param {
                name: "context".into(),
                ty: Some(Type::named("RouteContext")),
                default: None,
                kind: ParamKind::Normal,
            },
        ],
        // The writer puts the `Promise` on an async function's return type.
        returns: Some(Type::named("Response")),
        body,
        exported: true,
        is_async: true,
        is_property: false,
        is_constructor: false,
        is_private: false,
    }
}

/// The line that gives one handler parameter its value.
fn read_parameter(param: &Param, parameters: &[(String, bool)], models: &[Record]) -> Option<Stmt> {
    let path_name = parameters
        .iter()
        .find(|(name, _)| name == &param.name)
        .map(|(name, _)| name);

    let value = if let Some(path_name) = path_name {
        let read = Expr::Field {
            of: Box::new(Expr::Field {
                of: Box::new(Expr::Name("context".into())),
                name: "params".into(),
            }),
            name: path_name.clone(),
        };
        // A path parameter arrives as text.
        match param.ty {
            Some(Type::Int) | Some(Type::Float) => Expr::Call {
                callee: Box::new(Expr::Name("Number".into())),
                args: vec![read],
            },
            _ => read,
        }
    } else if names_a_model(param, models) {
        Expr::Await(Box::new(Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(Expr::Name("request".into())),
                name: "json".into(),
            }),
            args: Vec::new(),
        }))
    } else {
        let search = Expr::Field {
            of: Box::new(Expr::New {
                callee: Box::new(Expr::Name("URL".into())),
                args: vec![Expr::Field {
                    of: Box::new(Expr::Name("request".into())),
                    name: "url".into(),
                }],
            }),
            name: "searchParams".into(),
        };
        Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(search),
                name: "get".into(),
            }),
            args: vec![Expr::Str(param.name.clone())],
        }
    };

    Some(Stmt::Let {
        // The source's own name, so the references in the body reach this binding.
        name: param.name.clone(),
        // Only where the value has the declared type.
        ty: param.ty.clone().filter(|_| names_a_model(param, models)),
        value: Some(value),
        mutable: false,
    })
}

/// Does this parameter's type name a shape the module declares?
fn names_a_model(param: &Param, models: &[Record]) -> bool {
    matches!(&param.ty, Some(Type::Named { name, .. })
        if models.iter().any(|model| &model.name == name))
}

fn add_body_validation(
    output: &mut String,
    group: &[Endpoint],
    parameters: &[(String, bool)],
    models: &[Record],
    fidelity: &mut Fidelity,
) -> Result<()> {
    let helper = fresh_generated_name(output, "frMigrationValidate");
    let mut additions = Vec::new();
    for endpoint in group {
        let candidates = endpoint
            .handler
            .params
            .iter()
            .filter(|param| !depends_on_the_framework(param))
            .filter(|param| !parameters.iter().any(|(name, _)| name == &param.name))
            .filter_map(|param| {
                let Some(Type::Named { name, .. }) = &param.ty else {
                    return None;
                };
                models
                    .iter()
                    .find(|model| model.name == *name)
                    .map(|model| (param, model))
            })
            .collect::<Vec<_>>();
        let supported = candidates
            .first()
            .is_some_and(|(_, model)| validation_record_supported(model, models));
        let automatic = crate::project::framework_kernel::nextjs_body_validation_automatic(
            candidates.len(),
            supported,
        );
        if !automatic {
            if !candidates.is_empty() {
                fidelity.notes.push(format!(
                    "{} has {} body model candidate(s), and the generated Next.js route cannot enforce their complete declared shape at runtime.",
                    endpoint.handler.name,
                    candidates.len()
                ));
            }
            continue;
        }
        let (param, model) = candidates[0];
        let value = super::write::camel(&param.name);
        let model_name = super::write::pascal(&model.name);
        let binding = format!("    const {value}: {model_name} = await request.json();\n");
        if output.matches(&binding).count() != 1 {
            bail!(
                "runtime validation could not select the generated `{}` body binding uniquely.",
                endpoint.method
            );
        }
        let errors = fresh_generated_name(output, "frMigrationErrors");
        let validation = format!(
            "{binding}    const {errors} = {helper}({value}, {}, [\"body\"]);\n    if ({errors}.length > 0) {{\n        return Response.json({{ detail: {errors} }}, {{ status: 422 }});\n    }}\n",
            validation_record_shape(model, models)
        );
        *output = output.replacen(&binding, &validation, 1);
        additions.push(model.name.clone());
    }
    if additions.is_empty() {
        return Ok(());
    }
    let marker = output.find("export async function ").ok_or_else(|| {
        anyhow::anyhow!("the generated Next.js route has no exported handler for validation")
    })?;
    output.insert_str(marker, &validation_helper(&helper));
    fidelity.notes.push(format!(
        "generated Next.js runtime validation covers the declared structure of {} body model(s)",
        additions.len()
    ));
    Ok(())
}

fn fresh_generated_name(output: &str, base: &str) -> String {
    let mut candidate = base.to_owned();
    while output.contains(&candidate) {
        candidate.push('_');
    }
    candidate
}

fn validation_record_supported(record: &Record, models: &[Record]) -> bool {
    validation_fields_supported(record, models, &mut vec![record.name.as_str()], 0)
}

fn validation_fields_supported<'a>(
    record: &'a Record,
    models: &'a [Record],
    visiting: &mut Vec<&'a str>,
    depth: usize,
) -> bool {
    depth < 16
        && record.fields.iter().all(|field| {
            field.ty.as_ref().is_some_and(|ty| {
                validation_type_supported(ty, models, visiting, depth.saturating_add(1))
            })
        })
}

fn validation_type_supported<'a>(
    ty: &'a Type,
    models: &'a [Record],
    visiting: &mut Vec<&'a str>,
    depth: usize,
) -> bool {
    if depth >= 16 {
        return false;
    }
    match ty {
        Type::Unit | Type::Bool | Type::Int | Type::Float | Type::String => true,
        Type::List(inner) | Type::Optional(inner) => {
            validation_type_supported(inner, models, visiting, depth.saturating_add(1))
        }
        Type::Map(key, value) => {
            matches!(key.as_ref(), Type::String)
                && validation_type_supported(value, models, visiting, depth.saturating_add(1))
        }
        Type::Tuple(items) => items
            .iter()
            .all(|item| validation_type_supported(item, models, visiting, depth.saturating_add(1))),
        Type::Named { name, args } if args.is_empty() && !visiting.contains(&name.as_str()) => {
            let Some(record) = models.iter().find(|model| model.name == *name) else {
                return false;
            };
            visiting.push(name);
            let supported = validation_fields_supported(record, models, visiting, depth);
            visiting.pop();
            supported
        }
        Type::Set(_) | Type::Named { .. } | Type::Fn { .. } => false,
    }
}

fn validation_record_shape(record: &Record, models: &[Record]) -> String {
    let fields = record
        .fields
        .iter()
        .map(|field| {
            format!(
                "{}: {{ required: {}, shape: {} }}",
                serde_json::to_string(&field.name).expect("a JSON field name"),
                field.default.is_none(),
                validation_type_shape(field.ty.as_ref().expect("a supported field"), models)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ kind: \"record\", fields: {{ {fields} }} }}")
}

fn validation_type_shape(ty: &Type, models: &[Record]) -> String {
    match ty {
        Type::Unit => "{ kind: \"null\" }".to_owned(),
        Type::Bool => "{ kind: \"boolean\" }".to_owned(),
        Type::Int => "{ kind: \"integer\" }".to_owned(),
        Type::Float => "{ kind: \"number\" }".to_owned(),
        Type::String => "{ kind: \"string\" }".to_owned(),
        Type::List(inner) => format!(
            "{{ kind: \"list\", item: {} }}",
            validation_type_shape(inner, models)
        ),
        Type::Map(_, value) => format!(
            "{{ kind: \"map\", value: {} }}",
            validation_type_shape(value, models)
        ),
        Type::Optional(inner) => format!(
            "{{ kind: \"optional\", item: {} }}",
            validation_type_shape(inner, models)
        ),
        Type::Tuple(items) => format!(
            "{{ kind: \"tuple\", items: [{}] }}",
            items
                .iter()
                .map(|item| validation_type_shape(item, models))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Named { name, .. } => validation_record_shape(
            models
                .iter()
                .find(|model| model.name == *name)
                .expect("a supported named model"),
            models,
        ),
        Type::Set(_) | Type::Fn { .. } => unreachable!("unsupported validation shape"),
    }
}

fn validation_helper(name: &str) -> String {
    format!(
        r#"type {name}Shape =
    | {{ kind: "null" | "boolean" | "integer" | "number" | "string" }}
    | {{ kind: "list" | "optional", item: {name}Shape }}
    | {{ kind: "map", value: {name}Shape }}
    | {{ kind: "tuple", items: {name}Shape[] }}
    | {{ kind: "record", fields: {{ [key: string]: {{ required: boolean, shape: {name}Shape }} }} }};

type {name}Error = {{
    type: string;
    loc: (string | number)[];
    msg: string;
    input: unknown;
}};

function {name}(value: unknown, shape: {name}Shape, loc: (string | number)[]): {name}Error[] {{
    const fail = (type: string, msg: string): {name}Error[] => [{{ type, loc, msg, input: value }}];
    switch (shape.kind) {{
        case "null": return value === null ? [] : fail("none_required", "Input should be null");
        case "boolean": return typeof value === "boolean" ? [] : fail("bool_type", "Input should be a valid boolean");
        case "integer": return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value) ? [] : fail("int_type", "Input should be a valid integer");
        case "number": return typeof value === "number" && Number.isFinite(value) ? [] : fail("float_type", "Input should be a valid number");
        case "string": return typeof value === "string" ? [] : fail("string_type", "Input should be a valid string");
        case "optional": return value === null ? [] : {name}(value, shape.item, loc);
        case "list":
            return Array.isArray(value)
                ? value.flatMap((item, index) => {name}(item, shape.item, [...loc, index]))
                : fail("list_type", "Input should be a valid array");
        case "map":
            return typeof value === "object" && value !== null && !Array.isArray(value)
                ? Object.entries(value).flatMap(([key, item]) => {name}(item, shape.value, [...loc, key]))
                : fail("dict_type", "Input should be a valid object");
        case "tuple":
            if (!Array.isArray(value) || value.length !== shape.items.length) {{
                return fail("tuple_type", "Input should be a valid tuple");
            }}
            return shape.items.flatMap((item, index) => {name}(value[index], item, [...loc, index]));
        case "record":
            if (typeof value !== "object" || value === null || Array.isArray(value)) {{
                return fail("model_type", "Input should be a valid object");
            }}
            return Object.entries(shape.fields).flatMap(([key, field]) => {{
                if (!Object.prototype.hasOwnProperty.call(value, key)) {{
                    return field.required
                        ? [{{ type: "missing", loc: [...loc, key], msg: "Field required", input: value }}]
                        : [];
                }}
                return {name}((value as {{ [key: string]: unknown }})[key], field.shape, [...loc, key]);
            }});
    }}
}}

"#
    )
}

/// Turn every returned value into a response.
fn respond(body: &mut [Stmt]) {
    for statement in body.iter_mut() {
        match statement {
            Stmt::Return(value) => {
                // FastAPI serialises a bare `return` as a JSON null with status 200,
                // so the handler answers the same body either way.
                let returned = value.take().unwrap_or(Expr::Null);
                *value = Some(Expr::Call {
                    callee: Box::new(Expr::Field {
                        of: Box::new(Expr::Name("Response".into())),
                        name: "json".into(),
                    }),
                    args: vec![returned],
                });
            }
            Stmt::If {
                then, otherwise, ..
            }
            | Stmt::IfPresent {
                then, otherwise, ..
            } => {
                respond(then);
                respond(otherwise);
            }
            Stmt::While { body, .. }
            | Stmt::CountedFor { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachIndexed { body, .. }
            | Stmt::WhilePresent { body, .. }
            | Stmt::Defer(body)
            | Stmt::ErrDefer(body) => respond(body),
            Stmt::Switch { arms, default, .. } => {
                for (_, arm) in arms.iter_mut() {
                    respond(arm);
                }
                respond(default);
            }
            Stmt::MatchVariants { arms, default, .. } => {
                for arm in arms.iter_mut() {
                    respond(&mut arm.body);
                }
                respond(default);
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                respond(body);
                for catch in catches.iter_mut() {
                    respond(&mut catch.body);
                }
                respond(finally);
            }
            _ => {}
        }
    }
}
