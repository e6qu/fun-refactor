use super::super::Project;
use crate::application_ir::{validate_routes, ApplicationNode, HttpExpression, HttpRoute};
use crate::parse::Parsers;
use crate::transpile::ir::{Expr, Function, Item, Stmt};
use anyhow::Result;
use serde_json::{json, Number, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum Framework<'a> {
    Next,
    Fastapi,
    Express,
    Go,
    Other(&'a str),
}

impl<'a> Framework<'a> {
    fn from_name(name: &'a str) -> Self {
        match name {
            "nextjs-app" | "nextjs" => Self::Next,
            "fastapi" => Self::Fastapi,
            "express" => Self::Express,
            "go-net-http" => Self::Go,
            other => Self::Other(other),
        }
    }

    fn name(self) -> &'a str {
        match self {
            Self::Next => "nextjs",
            Self::Fastapi => "fastapi",
            Self::Express => "express",
            Self::Go => "go-net-http",
            Self::Other(name) => name,
        }
    }
}

struct Handler<'a> {
    name: &'a str,
    path: &'a str,
}

fn handler(node: &ApplicationNode) -> Option<Handler<'_>> {
    node.children.iter().find_map(|child| {
        let value = child.data.get("handler")?;
        Some(Handler {
            name: value.get("name")?.as_str()?,
            path: value.get("path")?.as_str()?,
        })
    })
}

fn functions(
    project: &Project<'_>,
    cache: &mut BTreeMap<(PathBuf, String), Vec<Function>>,
    path: &Path,
    framework: Framework<'_>,
) -> Result<Vec<Function>> {
    let key = (path.to_path_buf(), framework.name().to_owned());
    if let Some(functions) = cache.get(&key) {
        return Ok(functions.clone());
    }
    let absolute = project.root.join(path);
    let source = project
        .sources
        .get(&absolute)
        .ok_or_else(|| anyhow::anyhow!("captured handler source is unavailable."))?;
    let found: Vec<Function> = if matches!(framework, Framework::Fastapi) {
        crate::transpile::fastapi::route_functions(source)?
            .into_iter()
            .map(|(_, _, function)| function)
            .collect()
    } else {
        let language = crate::lang::detect(path)
            .ok_or_else(|| anyhow::anyhow!("handler language is unavailable."))?;
        let parsed = Parsers::new().parse(language, source)?;
        if parsed.has_errors() {
            anyhow::bail!("captured handler source does not parse cleanly.");
        }
        crate::transpile::read_module(language, source, parsed.root())?
            .items
            .into_iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some(function),
                _ => None,
            })
            .collect()
    };
    cache.insert(key, found.clone());
    Ok(found)
}

fn integer(value: &str) -> Option<Value> {
    let value = value.parse::<i64>().ok()?;
    (-9_007_199_254_740_991..=9_007_199_254_740_991)
        .contains(&value)
        .then(|| Value::Number(Number::from(value)))
}

fn field<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
    match expr {
        Expr::Field { of, name: actual } if actual == name => Some(of),
        _ => None,
    }
}

fn call<'a>(expr: &'a Expr) -> Option<(&'a Expr, &'a [Expr])> {
    match expr {
        Expr::Call { callee, args } => Some((callee, args)),
        _ => None,
    }
}

fn named(expr: &Expr, expected: &str) -> bool {
    matches!(expr, Expr::Name(name) if name == expected)
}

struct Bindings<'a> {
    framework: Framework<'a>,
    path: &'a BTreeSet<String>,
    next_params: Option<&'a str>,
    request: Option<&'a str>,
}

fn path_binding(expr: &Expr, bindings: &Bindings<'_>) -> Option<String> {
    let candidate = match bindings.framework {
        Framework::Fastapi => match expr {
            Expr::Name(name) => Some(name.as_str()),
            _ => None,
        },
        Framework::Next => match expr {
            Expr::Index { of, index }
                if bindings.next_params.is_some_and(|name| named(of, name)) =>
            {
                match &**index {
                    Expr::Str(name) => Some(name.as_str()),
                    _ => None,
                }
            }
            Expr::Field { of, name }
                if bindings
                    .next_params
                    .is_some_and(|binding| named(of, binding)) =>
            {
                Some(name.as_str())
            }
            _ => None,
        },
        Framework::Express => match expr {
            Expr::Index { of, index } if matches!(&**of, Expr::Field { of, name } if name == "params" && bindings.request.is_some_and(|request| named(of, request))) => {
                match &**index {
                    Expr::Str(name) => Some(name.as_str()),
                    _ => None,
                }
            }
            Expr::Field { of, name } if matches!(&**of, Expr::Field { of, name: field } if field == "params" && bindings.request.is_some_and(|request| named(of, request))) => {
                Some(name.as_str())
            }
            _ => None,
        },
        Framework::Go => {
            let (callee, args) = call(expr)?;
            let receiver = field(callee, "PathValue")?;
            if args.len() != 1
                || !bindings
                    .request
                    .is_some_and(|request| named(receiver, request))
            {
                return None;
            }
            match &args[0] {
                Expr::Str(name) => Some(name.as_str()),
                _ => None,
            }
        }
        Framework::Other(_) => None,
    }?;
    bindings
        .path
        .contains(candidate)
        .then(|| candidate.to_owned())
}

fn expression(expr: &Expr, bindings: &Bindings<'_>) -> Option<HttpExpression> {
    let value = match expr {
        Expr::Str(value) => HttpExpression::Literal {
            value: Value::String(value.clone()),
        },
        Expr::Bool(value) => HttpExpression::Literal {
            value: Value::Bool(*value),
        },
        Expr::Null => HttpExpression::Literal { value: Value::Null },
        Expr::Int(value) => HttpExpression::Literal {
            value: integer(value)?,
        },
        Expr::ListLit(items) => HttpExpression::Array {
            items: items
                .iter()
                .map(|item| expression(item, bindings))
                .collect::<Option<_>>()?,
        },
        Expr::MapLit(fields) => {
            let mut mapped = BTreeMap::new();
            for (key, value) in fields {
                let Expr::Str(key) = key else {
                    return None;
                };
                if mapped
                    .insert(key.clone(), expression(value, bindings)?)
                    .is_some()
                {
                    return None;
                }
            }
            HttpExpression::Object { fields: mapped }
        }
        _ => HttpExpression::Path {
            name: path_binding(expr, bindings)?,
        },
    };
    Some(value)
}

fn status_map(expr: &Expr) -> Option<u16> {
    let Expr::MapLit(fields) = expr else {
        return None;
    };
    if fields.len() != 1 || !matches!(&fields[0].0, Expr::Str(name) if name == "status") {
        return None;
    }
    let Expr::Int(value) = &fields[0].1 else {
        return None;
    };
    value.parse().ok()
}

fn next(function: &Function, path: &BTreeSet<String>) -> Option<(u16, HttpExpression)> {
    if !function.exported || !function.is_async {
        return None;
    }
    let (params, returned) = match function.body.as_slice() {
        [Stmt::Return(Some(returned))] => (None, returned),
        [Stmt::Let {
            name,
            value: Some(Expr::Await(value)),
            mutable: false,
            ..
        }, Stmt::Return(Some(returned))]
            if matches!(&**value, Expr::Field { of, name: field } if field == "params" && matches!(&**of, Expr::Name(_))) =>
        {
            (Some(name.as_str()), returned)
        }
        _ => return None,
    };
    if !path.is_empty() && params.is_none() {
        return None;
    }
    let (callee, args) = call(returned)?;
    if !matches!(callee, Expr::Field { of, name } if name == "json" && named(of, "Response"))
        || !(1..=2).contains(&args.len())
    {
        return None;
    }
    let status = args.get(1).map(status_map).unwrap_or(Some(200))?;
    let response = expression(
        &args[0],
        &Bindings {
            framework: Framework::Next,
            path,
            next_params: params,
            request: None,
        },
    )?;
    Some((status, response))
}

fn fastapi(function: &Function, path: &BTreeSet<String>) -> Option<(u16, HttpExpression)> {
    let [Stmt::Return(Some(returned))] = function.body.as_slice() else {
        return None;
    };
    let (callee, args) = call(returned)?;
    if !named(callee, "JSONResponse") {
        return None;
    }
    let mut content = None;
    let mut status = None;
    for argument in args {
        let Expr::Keyword { name, value } = argument else {
            return None;
        };
        match name.as_str() {
            "content" if content.is_none() => content = Some(&**value),
            "status_code" if status.is_none() => {
                let Expr::Int(value) = &**value else {
                    return None;
                };
                status = value.parse().ok();
            }
            _ => return None,
        }
    }
    let response = expression(
        content?,
        &Bindings {
            framework: Framework::Fastapi,
            path,
            next_params: None,
            request: None,
        },
    )?;
    Some((status?, response))
}

fn express(function: &Function, path: &BTreeSet<String>) -> Option<(u16, HttpExpression)> {
    let returned = match function.body.as_slice() {
        [Stmt::Return(Some(value))] | [Stmt::Expr(value)] => value,
        _ => return None,
    };
    let (json_callee, json_args) = call(returned)?;
    if json_args.len() != 1 {
        return None;
    }
    let status_call = field(json_callee, "json")?;
    let (status_callee, status_args) = call(status_call)?;
    if status_args.len() != 1 {
        return None;
    }
    let response_name = match field(status_callee, "status")? {
        Expr::Name(name) => name,
        _ => return None,
    };
    let Expr::Int(status) = &status_args[0] else {
        return None;
    };
    let request = function
        .params
        .iter()
        .map(|parameter| parameter.name.as_str())
        .find(|name| *name != response_name)?;
    let response = expression(
        &json_args[0],
        &Bindings {
            framework: Framework::Express,
            path,
            next_params: None,
            request: Some(request),
        },
    )?;
    Some((status.parse().ok()?, response))
}

fn go(function: &Function, path: &BTreeSet<String>) -> Option<(u16, HttpExpression)> {
    let (status, encoded) = match function.body.as_slice() {
        [Stmt::Expr(status), Stmt::Expr(encoded)] => (status, encoded),
        [Stmt::Expr(status), Stmt::Assign { value: encoded, .. }] => (status, encoded),
        _ => return None,
    };
    let (status_callee, status_args) = call(status)?;
    if status_args.len() != 1 {
        return None;
    }
    let writer = match field(status_callee, "WriteHeader")? {
        Expr::Name(name) => name,
        _ => return None,
    };
    let Expr::Int(status) = &status_args[0] else {
        return None;
    };
    let (encode_callee, encode_args) = call(encoded)?;
    if encode_args.len() != 1 {
        return None;
    }
    let encoder = field(encode_callee, "Encode")?;
    let (new_encoder, writer_args) = call(encoder)?;
    if writer_args.len() != 1 || !named(&writer_args[0], writer) {
        return None;
    }
    if !matches!(new_encoder, Expr::Field { of, name } if name == "NewEncoder" && named(of, "json"))
    {
        return None;
    }
    let request = function
        .params
        .iter()
        .map(|parameter| parameter.name.as_str())
        .find(|name| *name != writer)?;
    let response = expression(
        &encode_args[0],
        &Bindings {
            framework: Framework::Go,
            path,
            next_params: None,
            request: Some(request),
        },
    )?;
    Some((status.parse().ok()?, response))
}

fn normalize(
    function: &Function,
    framework: Framework<'_>,
    method: &str,
    path: &str,
) -> Option<HttpRoute> {
    let provisional = HttpRoute {
        method: method.to_owned(),
        path: path.to_owned(),
        status: 200,
        response: HttpExpression::Literal { value: Value::Null },
    };
    let parameters = provisional.parameters().ok()?;
    let (status, response) = match framework {
        Framework::Next => next(function, &parameters),
        Framework::Fastapi => fastapi(function, &parameters),
        Framework::Express => express(function, &parameters),
        Framework::Go => go(function, &parameters),
        Framework::Other(_) => None,
    }?;
    let route = HttpRoute {
        method: method.to_owned(),
        path: path.to_owned(),
        status,
        response,
    };
    route.validate().ok()?;
    Some(route)
}

fn normalize_node(
    project: &Project<'_>,
    cache: &mut BTreeMap<(PathBuf, String), Vec<Function>>,
    node: &mut ApplicationNode,
) {
    if node.kind == "route" {
        let route = node.data.get("route").cloned().unwrap_or(Value::Null);
        let framework_name = route
            .get("framework")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let framework = Framework::from_name(framework_name);
        let method = route.get("method").and_then(Value::as_str);
        let path = route.get("url").and_then(Value::as_str);
        let selected = handler(node).and_then(|handler| {
            let functions = functions(project, cache, Path::new(handler.path), framework).ok()?;
            functions
                .into_iter()
                .find(|function| function.name == handler.name)
        });
        node.route = selected
            .as_ref()
            .zip(method.zip(path))
            .and_then(|(function, (method, path))| normalize(function, framework, method, path));
        let portable = node.route.is_some();
        node.data.insert(
            "normalization".into(),
            if portable {
                json!({"status": "portable", "basis": "syntax-derived-shared-ir", "runtime_proved": false})
            } else {
                json!({"status": "manual", "reason": "The handler exceeds the bounded literal JSON and path-binding subset.", "runtime_proved": false})
            },
        );
        node.boundary = Some(if portable {
            "The response is normalized from syntax-derived shared IR; middleware and runtime behavior remain unproved."
        } else {
            "Source evidence is retained, but executable response semantics require manual normalization."
        }.into());
    }
    for child in &mut node.children {
        normalize_node(project, cache, child);
    }
}

fn collect(node: &ApplicationNode, routes: &mut Vec<HttpRoute>) {
    if let Some(route) = &node.route {
        routes.push(route.clone());
    }
    for child in &node.children {
        collect(child, routes);
    }
}

fn reject_portable(node: &mut ApplicationNode) {
    if node.route.take().is_some() {
        node.data.insert(
            "normalization".into(),
            json!({"status": "manual", "reason": "Portable route matchers overlap within this application.", "runtime_proved": false}),
        );
        node.boundary = Some(
            "Overlapping source matchers require an explicit routing decision before conversion."
                .into(),
        );
    }
    for child in &mut node.children {
        reject_portable(child);
    }
}

pub(super) fn routes(project: &Project<'_>, applications: &mut [ApplicationNode]) -> Result<()> {
    let mut cache = BTreeMap::new();
    for application in applications {
        normalize_node(project, &mut cache, application);
        let mut normalized = Vec::new();
        collect(application, &mut normalized);
        if !normalized.is_empty() && validate_routes(&normalized).is_err() {
            reject_portable(application);
        }
    }
    Ok(())
}
