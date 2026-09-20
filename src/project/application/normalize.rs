use super::super::Project;
use crate::application_ir::{
    validate_middleware, ApplicationNode, HttpDependency, HttpExpression, HttpInput,
    HttpInputSource, HttpMiddleware, HttpRoute, HttpScalar,
};
use crate::parse::Parsers;
use crate::transpile::ir::{Expr, Function, Item, Param, ParamKind, Stmt, Type};
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

#[derive(Clone, Default)]
struct Module {
    functions: Vec<Function>,
    constants: Vec<(String, Expr)>,
}

fn module(
    project: &Project<'_>,
    cache: &mut BTreeMap<(PathBuf, String), Module>,
    path: &Path,
    framework: Framework<'_>,
) -> Result<Module> {
    let key = (path.to_path_buf(), framework.name().to_owned());
    if let Some(module) = cache.get(&key) {
        return Ok(module.clone());
    }
    let absolute = project.root.join(path);
    let source = project
        .sources
        .get(&absolute)
        .ok_or_else(|| anyhow::anyhow!("captured handler source is unavailable."))?;
    let found = if matches!(framework, Framework::Fastapi) {
        Module {
            functions: crate::transpile::fastapi::route_functions(source)?
                .into_iter()
                .map(|(_, _, function)| function)
                .collect(),
            constants: Vec::new(),
        }
    } else {
        let language = crate::lang::detect(path)
            .ok_or_else(|| anyhow::anyhow!("handler language is unavailable."))?;
        let parsed = Parsers::new().parse(language, source)?;
        if parsed.has_errors() {
            anyhow::bail!("captured handler source does not parse cleanly.");
        }
        let mut found = Module::default();
        for item in crate::transpile::read_module(language, source, parsed.root())?.items {
            match item {
                Item::Function(function) => found.functions.push(function),
                Item::Constant(constant) => found
                    .constants
                    .push((constant.name.clone(), constant.value.clone())),
                _ => (),
            }
        }
        found
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

fn call(expr: &Expr) -> Option<(&Expr, &[Expr])> {
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
    inputs: &'a BTreeMap<String, String>,
    validated: &'a [String],
}

fn input_binding(expr: &Expr, bindings: &Bindings<'_>) -> Option<String> {
    if let Expr::Name(name) = expr {
        return bindings.inputs.get(name).cloned();
    }
    if let Expr::Field { of, name } = expr {
        if let Expr::Field {
            of: result,
            name: data,
        } = &**of
        {
            if data == "data"
                && matches!(&**result, Expr::Name(var) if bindings.validated.contains(var))
            {
                return bindings.inputs.get(name).cloned();
            }
        }
    }
    None
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

fn service_call(expr: &Expr) -> Option<HttpExpression> {
    let Expr::Call { callee, args } = expr else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let Expr::Field { of, name } = &**callee else {
        return None;
    };
    if name != "json" {
        return None;
    }
    let Expr::Call {
        callee: request,
        args,
    } = &**of
    else {
        return None;
    };
    let [Expr::Str(path)] = args.as_slice() else {
        return None;
    };
    let Expr::Field {
        of: client,
        name: method,
    } = &**request
    else {
        return None;
    };
    if !matches!(&**client, Expr::Name(client) if client == "requests" || client == "httpx") {
        return None;
    }
    let method = method.to_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    ) || !path.starts_with('/')
    {
        return None;
    }
    Some(HttpExpression::Service {
        method,
        path: path.clone(),
    })
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
        _ => match input_binding(expr, bindings) {
            Some(name) => HttpExpression::Input { name },
            None => {
                if matches!(bindings.framework, Framework::Fastapi) {
                    if let Some(service) = service_call(expr) {
                        return Some(service);
                    }
                }
                HttpExpression::Path {
                    name: path_binding(expr, bindings)?,
                }
            }
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

fn collected_inputs(
    validated: &[Validated],
) -> (Vec<HttpInput>, BTreeMap<String, String>, Vec<String>) {
    let mut inputs = Vec::new();
    let mut names = BTreeMap::new();
    let mut variables = Vec::new();
    for entry in validated {
        variables.push(entry.variable.clone());
        for input in &entry.inputs {
            names.insert(input.name.clone(), input.name.clone());
            inputs.push(input.clone());
        }
    }
    (inputs, names, variables)
}

fn next(
    function: &Function,
    path: &BTreeSet<String>,
    constants: &[(String, Expr)],
) -> Option<(Vec<HttpInput>, u16, HttpExpression)> {
    if !function.exported || !function.is_async {
        return None;
    }
    let (params, rest) = match function.body.as_slice() {
        [Stmt::Let {
            name,
            value: Some(Expr::Await(value)),
            mutable: false,
            ..
        }, rest @ ..]
            if matches!(&**value, Expr::Field { of, name: field } if field == "params" && matches!(&**of, Expr::Name(_))) =>
        {
            (Some(name.as_str()), rest)
        }
        rest => (None, rest),
    };
    let (validated, consumed) = ts_validation_prefixes(rest, Framework::Next, constants)?;
    for entry in &validated {
        if !function
            .params
            .iter()
            .any(|param| param.name == entry.request)
        {
            return None;
        }
    }
    let [Stmt::Return(Some(returned))] = &rest[consumed..] else {
        return None;
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
    let (inputs, names, variables) = collected_inputs(&validated);
    let response = expression(
        &args[0],
        &Bindings {
            framework: Framework::Next,
            path,
            next_params: params,
            request: None,
            inputs: &names,
            validated: &variables,
        },
    )?;
    Some((inputs, status, response))
}

fn zod_scalar(spec: &Expr, source: HttpInputSource) -> Option<HttpScalar> {
    if !matches!(spec, Expr::Call { .. }) {
        return None;
    }
    let mut methods = Vec::new();
    let mut current = spec;
    loop {
        match current {
            Expr::Name(name) if name == "z" => break,
            Expr::Call { callee, args } if args.is_empty() => {
                let Expr::Field { of, name } = &**callee else {
                    return None;
                };
                methods.push(name.as_str());
                current = of;
            }
            Expr::Field { of, name } => {
                methods.push(name.as_str());
                current = of;
            }
            _ => return None,
        }
    }
    methods.reverse();
    match (source, methods.as_slice()) {
        (_, ["string"]) => Some(HttpScalar::String),
        (HttpInputSource::JsonBody, ["number", "int"])
        | (HttpInputSource::Query, ["coerce", "number", "int"]) => Some(HttpScalar::Integer),
        (HttpInputSource::JsonBody, ["boolean"]) => Some(HttpScalar::Boolean),
        _ => None,
    }
}

fn zod_fields(
    constants: &[(String, Expr)],
    schema: &str,
    source: HttpInputSource,
) -> Option<Vec<(String, HttpScalar)>> {
    let (_, value) = constants.iter().find(|(name, _)| name == schema)?;
    let (callee, args) = call(value)?;
    let Expr::Field { of, name } = callee else {
        return None;
    };
    if name != "object" || !matches!(&**of, Expr::Name(base) if base == "z") {
        return None;
    }
    let [Expr::MapLit(entries)] = args else {
        return None;
    };
    let mut fields = Vec::new();
    for (key, spec) in entries {
        let name = match key {
            Expr::Str(key) => key.clone(),
            Expr::Name(key) => key.clone(),
            _ => return None,
        };
        fields.push((name, zod_scalar(spec, source)?));
    }
    Some(fields)
}

fn ts_guard_422(statement: &Stmt, variable: &str, framework: Framework<'_>) -> bool {
    let Stmt::If {
        condition,
        then,
        otherwise,
    } = statement
    else {
        return false;
    };
    if !otherwise.is_empty()
        || !matches!(
            condition,
            Expr::Unary {
                op: crate::transpile::ir::UnaryOp::Not,
                operand,
            } if matches!(&**operand, Expr::Field { of, name } if name == "success" && matches!(&**of, Expr::Name(var) if var == variable))
        )
    {
        return false;
    }
    let [Stmt::Return(Some(failure))] = then.as_slice() else {
        return false;
    };
    let Some((callee, args)) = call(failure) else {
        return false;
    };
    match framework {
        Framework::Express => {
            let Some(status_call) = field(callee, "json") else {
                return false;
            };
            let Some((status_callee, status_args)) = call(status_call) else {
                return false;
            };
            args.len() == 1
                && status_args.len() == 1
                && field(status_callee, "status").is_some()
                && matches!(&status_args[0], Expr::Int(status) if status == "422")
        }
        Framework::Next => {
            args.len() == 2
                && matches!(callee, Expr::Field { of, name } if name == "json" && named(of, "Response"))
                && status_map(&args[1]) == Some(422)
        }
        _ => false,
    }
}

fn ts_validation_argument(
    argument: &Expr,
    framework: Framework<'_>,
) -> Option<(HttpInputSource, String)> {
    match framework {
        Framework::Express => {
            let Expr::Field { of, name } = argument else {
                return None;
            };
            let Expr::Name(request) = &**of else {
                return None;
            };
            let source = match name.as_str() {
                "query" => HttpInputSource::Query,
                "body" => HttpInputSource::JsonBody,
                _ => return None,
            };
            Some((source, request.clone()))
        }
        Framework::Next => {
            if let Expr::Await(inner) = argument {
                let (callee, args) = call(inner)?;
                if !args.is_empty() {
                    return None;
                }
                let Expr::Field { of, name } = callee else {
                    return None;
                };
                if name != "json" {
                    return None;
                }
                let Expr::Name(request) = &**of else {
                    return None;
                };
                return Some((HttpInputSource::JsonBody, request.clone()));
            }
            let (callee, args) = call(argument)?;
            let Expr::Field { of, name } = callee else {
                return None;
            };
            if name != "fromEntries" || !matches!(&**of, Expr::Name(base) if base == "Object") {
                return None;
            }
            let [url] = args else {
                return None;
            };
            let Expr::Field {
                of: constructed,
                name: search_params,
            } = url
            else {
                return None;
            };
            if search_params != "searchParams" {
                return None;
            }
            let Expr::New { callee, args } = &**constructed else {
                return None;
            };
            if !matches!(&**callee, Expr::Name(name) if name == "URL") {
                return None;
            }
            let [Expr::Field { of, name }] = args.as_slice() else {
                return None;
            };
            if name != "url" {
                return None;
            }
            let Expr::Name(request) = &**of else {
                return None;
            };
            Some((HttpInputSource::Query, request.clone()))
        }
        _ => None,
    }
}

struct Validated {
    variable: String,
    request: String,
    inputs: Vec<HttpInput>,
}

fn ts_validation_prefixes(
    body: &[Stmt],
    framework: Framework<'_>,
    constants: &[(String, Expr)],
) -> Option<(Vec<Validated>, usize)> {
    let mut validated = Vec::new();
    let mut index = 0;
    while let [Stmt::Let {
        name,
        value: Some(value),
        mutable: false,
        ..
    }, guard, ..] = &body[index..]
    {
        let Some((callee, args)) = call(value) else {
            break;
        };
        let Expr::Field { of, name: method } = callee else {
            break;
        };
        if method != "safeParse" {
            break;
        }
        let Expr::Name(schema) = &**of else {
            break;
        };
        let [argument] = args else {
            break;
        };
        let Some((source, request)) = ts_validation_argument(argument, framework) else {
            break;
        };
        let Some(fields) = zod_fields(constants, schema, source) else {
            break;
        };
        if !ts_guard_422(guard, name, framework) {
            break;
        }
        if validated
            .iter()
            .any(|entry: &Validated| entry.variable == *name)
        {
            break;
        }
        let inputs = fields
            .into_iter()
            .map(|(field, scalar)| HttpInput {
                name: field,
                source,
                scalar,
            })
            .collect();
        validated.push(Validated {
            variable: name.clone(),
            request,
            inputs,
        });
        index += 2;
    }
    Some((validated, index))
}

fn go_query_key(expr: &Expr) -> Option<(String, String)> {
    let (callee, args) = call(expr)?;
    let Expr::Field { of, name } = callee else {
        return None;
    };
    if name != "Get" {
        return None;
    }
    let (inner, inner_args) = call(of)?;
    if !inner_args.is_empty() {
        return None;
    }
    let Expr::Field { of, name } = inner else {
        return None;
    };
    if name != "Query" {
        return None;
    }
    let Expr::Field { of, name } = &**of else {
        return None;
    };
    if name != "URL" {
        return None;
    }
    let Expr::Name(request) = &**of else {
        return None;
    };
    let [Expr::Str(key)] = args else {
        return None;
    };
    Some((key.clone(), request.clone()))
}

type GoValidated = (String, String, String, String);

fn go_validation_prefixes(body: &[Stmt]) -> Option<(Vec<GoValidated>, usize)> {
    let mut validated = Vec::new();
    let mut index = 0;
    while let [Stmt::Try {
        body: attempted,
        catches,
        finally,
        ..
    }, ..] = &body[index..]
    {
        if !finally.is_empty() {
            break;
        }
        let [Stmt::Let {
            name,
            value: Some(value),
            mutable: false,
            ..
        }] = attempted.as_slice()
        else {
            break;
        };
        let [catch] = catches.as_slice() else {
            break;
        };
        if catch.binding.is_none() {
            break;
        }
        let Some((callee, args)) = call(value) else {
            break;
        };
        if !matches!(callee, Expr::Field { of, name } if name == "Atoi" && matches!(&**of, Expr::Name(pkg) if pkg == "strconv"))
        {
            break;
        }
        let [argument] = args else {
            break;
        };
        let [Stmt::Expr(status), Stmt::Return(None)] = catch.body.as_slice() else {
            break;
        };
        let Some((status_callee, status_args)) = call(status) else {
            break;
        };
        let Some(writer) = field(status_callee, "WriteHeader") else {
            break;
        };
        let Expr::Name(writer) = writer else {
            break;
        };
        if status_args.len() != 1 || !matches!(&status_args[0], Expr::Int(code) if code == "422") {
            break;
        }
        let Some((key, request)) = go_query_key(argument) else {
            break;
        };
        validated.push((name.clone(), key, request, writer.clone()));
        index += 1;
    }
    Some((validated, index))
}

fn fastapi_callee(expr: &Expr, expected: &str) -> bool {
    match expr {
        Expr::Name(name) => name == expected,
        Expr::Field { name, .. } => name == expected,
        _ => false,
    }
}

fn fastapi_scalar(ty: &Type) -> Option<HttpScalar> {
    match ty {
        Type::String => Some(HttpScalar::String),
        Type::Int => Some(HttpScalar::Integer),
        Type::Bool => Some(HttpScalar::Boolean),
        _ => None,
    }
}

fn fastapi_input(parameter: &Param) -> Option<HttpInput> {
    if parameter.kind != ParamKind::Normal {
        return None;
    }
    let scalar = fastapi_scalar(parameter.ty.as_ref()?)?;
    let (callee, arguments) = call(parameter.default.as_ref()?)?;
    let (source, source_code) = if fastapi_callee(callee, "Query") {
        (HttpInputSource::Query, 0)
    } else if fastapi_callee(callee, "Body") {
        (HttpInputSource::JsonBody, 1)
    } else {
        return None;
    };
    let mut alias = None;
    let mut alias_safe = true;
    let mut embedded = false;
    let mut extra_metadata = false;
    for argument in arguments {
        let Expr::Keyword { name, value } = argument else {
            extra_metadata = true;
            continue;
        };
        match (name.as_str(), &**value) {
            ("alias", Expr::Str(name)) if alias.is_none() => alias = Some(name.clone()),
            ("alias", _) => alias_safe = false,
            ("embed", Expr::Bool(true)) if source_code == 1 && !embedded => embedded = true,
            _ => extra_metadata = true,
        }
    }
    if !crate::framework_kernel::application_fastapi_input_admitted(
        source_code,
        scalar.code(),
        alias_safe,
        embedded,
        extra_metadata,
    ) {
        return None;
    }
    Some(HttpInput {
        name: alias.unwrap_or_else(|| parameter.name.clone()),
        source,
        scalar,
    })
}

fn dotted(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.clone()),
        Expr::Field { of, name } => Some(format!("{}.{name}", dotted(of)?)),
        _ => None,
    }
}

fn fastapi_dependency(parameter: &Param) -> Option<HttpDependency> {
    if parameter.kind != ParamKind::Normal {
        return None;
    }
    let (callee, arguments) = call(parameter.default.as_ref()?)?;
    let security = if fastapi_callee(callee, "Depends") {
        false
    } else if fastapi_callee(callee, "Security") {
        true
    } else {
        return None;
    };
    let mut provider = None;
    let mut configured = false;
    for argument in arguments {
        match argument {
            Expr::Keyword { name, value } if name == "dependency" && provider.is_none() => {
                provider = dotted(value);
            }
            Expr::Keyword { .. } => configured = true,
            value if provider.is_none() => provider = dotted(value),
            _ => configured = true,
        }
    }
    let provider = provider?;
    if !crate::framework_kernel::application_dependency_admitted(
        crate::application_ir::dotted_identifier(&provider),
        configured,
    ) {
        return None;
    }
    Some(HttpDependency {
        binding: parameter.name.clone(),
        provider,
        security,
    })
}

fn fastapi(
    function: &Function,
    path: &BTreeSet<String>,
) -> Option<(Vec<HttpInput>, Vec<HttpDependency>, u16, HttpExpression)> {
    let mut inputs = Vec::new();
    let mut dependencies = Vec::new();
    let mut bindings = BTreeMap::new();
    for parameter in &function.params {
        if path.contains(&parameter.name) {
            continue;
        }
        if let Some(input) = fastapi_input(parameter) {
            if bindings
                .insert(parameter.name.clone(), input.name.clone())
                .is_some()
            {
                return None;
            }
            inputs.push(input);
        } else {
            dependencies.push(fastapi_dependency(parameter)?);
        }
    }
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
            inputs: &bindings,
            validated: &[],
        },
    )?;
    Some((inputs, dependencies, status?, response))
}

fn express(
    function: &Function,
    path: &BTreeSet<String>,
    constants: &[(String, Expr)],
) -> Option<(Vec<HttpInput>, u16, HttpExpression)> {
    let (validated, consumed) =
        ts_validation_prefixes(&function.body, Framework::Express, constants)?;
    let returned = match &function.body[consumed..] {
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
    if !validated.iter().all(|entry| entry.request == *request) {
        return None;
    }
    let (inputs, names, variables) = collected_inputs(&validated);
    let response = expression(
        &json_args[0],
        &Bindings {
            framework: Framework::Express,
            path,
            next_params: None,
            request: Some(request),
            inputs: &names,
            validated: &variables,
        },
    )?;
    Some((inputs, status.parse().ok()?, response))
}

fn go(
    function: &Function,
    path: &BTreeSet<String>,
) -> Option<(Vec<HttpInput>, u16, HttpExpression)> {
    let (validated, consumed) = go_validation_prefixes(&function.body)?;
    let (status, encoded) = match &function.body[consumed..] {
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
    if !validated
        .iter()
        .all(|(_, _, prefix_request, prefix_writer)| {
            prefix_request == request && prefix_writer == writer
        })
    {
        return None;
    }
    let mut inputs = Vec::new();
    let mut names = BTreeMap::new();
    for (local, key, _, _) in &validated {
        names.insert(local.clone(), key.clone());
        inputs.push(HttpInput {
            name: key.clone(),
            source: HttpInputSource::Query,
            scalar: HttpScalar::Integer,
        });
    }
    let response = expression(
        &encode_args[0],
        &Bindings {
            framework: Framework::Go,
            path,
            next_params: None,
            request: Some(request),
            inputs: &names,
            validated: &[],
        },
    )?;
    Some((inputs, status.parse().ok()?, response))
}

fn normalize(
    function: &Function,
    framework: Framework<'_>,
    constants: &[(String, Expr)],
    method: &str,
    path: &str,
) -> Option<HttpRoute> {
    let provisional = HttpRoute {
        method: method.to_owned(),
        path: path.to_owned(),
        inputs: Vec::new(),
        dependencies: Vec::new(),
        status: 200,
        response: HttpExpression::Literal { value: Value::Null },
    };
    let parameters = provisional.parameters().ok()?;
    let (inputs, dependencies, status, response) = match framework {
        Framework::Next => next(function, &parameters, constants)
            .map(|(inputs, status, response)| (inputs, Vec::new(), status, response)),
        Framework::Fastapi => fastapi(function, &parameters),
        Framework::Express => express(function, &parameters, constants)
            .map(|(inputs, status, response)| (inputs, Vec::new(), status, response)),
        Framework::Go => go(function, &parameters)
            .map(|(inputs, status, response)| (inputs, Vec::new(), status, response)),
        Framework::Other(_) => None,
    }?;
    let route = HttpRoute {
        method: method.to_owned(),
        path: path.to_owned(),
        inputs,
        dependencies,
        status,
        response,
    };
    route.validate().ok()?;
    Some(route)
}

fn fastapi_middleware(project: &Project<'_>, root: &str) -> Option<Vec<HttpMiddleware>> {
    let source = project.sources.get(&project.root.join(root))?;
    let parsed = Parsers::new()
        .parse(crate::lang::Language::Python, source)
        .ok()?;
    if parsed.has_errors() {
        return None;
    }
    let entries = super::super::fast_routes::middleware(&parsed, source);
    if entries.is_empty() {
        return Some(Vec::new());
    }
    let total = entries.len();
    let resolved = entries.iter().filter(|entry| entry.name.is_some()).count();
    let configured = entries.iter().filter(|entry| entry.configured).count();
    if !crate::framework_kernel::application_middleware_chain_admitted(total, resolved, configured)
    {
        return None;
    }
    let mut chain: Vec<_> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| HttpMiddleware {
            name: entry.name.clone().unwrap(),
            request_order: crate::framework_kernel::middleware_request_order(total, index),
        })
        .collect();
    chain.sort_by_key(|entry| entry.request_order);
    validate_middleware(&chain).ok()?;
    Some(chain)
}

fn normalize_node(
    project: &Project<'_>,
    cache: &mut BTreeMap<(PathBuf, String), Module>,
    node: &mut ApplicationNode,
    application_framework: Option<String>,
) {
    let application_framework = if node.kind == "application" {
        node.data
            .get("application")
            .and_then(|value| value.get("framework"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or(application_framework)
    } else {
        application_framework
    };
    if node.kind == "application" && application_framework.as_deref() == Some("fastapi") {
        if let Some(root) = node
            .data
            .get("application")
            .and_then(|value| value.get("root"))
            .and_then(Value::as_str)
        {
            match fastapi_middleware(project, root) {
                Some(chain) if !chain.is_empty() => {
                    node.middleware = chain;
                    node.data.insert(
                        "middleware_normalization".into(),
                        json!({"status": "portable", "basis": "direct-name-reverse-registration-order", "runtime_proved": false}),
                    );
                }
                Some(_) => (),
                None => {
                    node.data.insert(
                        "middleware_normalization".into(),
                        json!({"status": "manual", "reason": "The middleware chain exceeds the direct-name unconfigured subset.", "runtime_proved": false}),
                    );
                }
            }
        }
    }
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
            let module = module(project, cache, Path::new(handler.path), framework).ok()?;
            module
                .functions
                .iter()
                .find(|function| function.name == handler.name)
                .map(|function| (function.clone(), module.constants.clone()))
        });
        node.route = selected.as_ref().zip(method.zip(path)).and_then(
            |((function, constants), (method, path))| {
                normalize(function, framework, constants, method, path)
            },
        );
        let portable = node.route.is_some();
        let validated = node
            .route
            .as_ref()
            .is_some_and(|route| !route.inputs.is_empty());
        node.data.insert(
            "normalization".into(),
            if validated {
                json!({"status": "portable", "basis": "syntax-derived-validated-http-ir", "runtime_proved": false})
            } else if portable {
                json!({"status": "portable", "basis": "syntax-derived-shared-ir", "runtime_proved": false})
            } else {
                json!({"status": "manual", "reason": "The handler exceeds the bounded literal JSON and path-binding subset.", "runtime_proved": false})
            },
        );
        node.boundary = Some(if validated {
            "Required request inputs and the successful response are normalized; validation errors use the portable IR contract, while middleware and runtime behavior remain unproved."
        } else if portable {
            "The response is normalized from syntax-derived shared IR; middleware and runtime behavior remain unproved."
        } else {
            "Source evidence is retained, but executable response semantics require manual normalization."
        }.into());
    }
    if node.kind == "component"
        && matches!(
            application_framework.as_deref(),
            Some("react" | "nextjs-app")
        )
    {
        node.component = (|| {
            let path = node.source.get("path")?.as_str()?;
            let line = node.source.get("line")?.as_u64()? as usize;
            let name = node.data.get("component")?.get("name")?.as_str();
            let source = project.sources.get(&project.root.join(path))?;
            let parsed = Parsers::new()
                .parse(crate::lang::Language::Tsx, source)
                .ok()?;
            if parsed.has_errors() {
                return None;
            }
            super::super::components::static_component(
                &parsed,
                source,
                line,
                name,
                application_framework.as_deref() == Some("react"),
            )
        })();
        let portable = node.component.is_some();
        node.data.insert(
            "normalization".into(),
            if portable {
                json!({"status": "portable", "basis": "literal-intrinsic-jsx-tree", "runtime_proved": false})
            } else {
                json!({"status": "manual", "reason": "The component exceeds the bounded static intrinsic JSX subset.", "runtime_proved": false})
            },
        );
        node.boundary = Some(
            if portable {
                "The intrinsic JSX tree is normalized; framework rendering and styling remain unproved."
            } else {
                "Source evidence is retained, but dynamic rendering semantics require manual normalization."
            }
            .into(),
        );
    }
    for child in &mut node.children {
        normalize_node(project, cache, child, application_framework.clone());
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

fn demote_service_route(node: &mut ApplicationNode, method: &str, path: &str) -> bool {
    if node
        .route
        .as_ref()
        .is_some_and(|route| route.method == method && route.path == path)
    {
        node.route = None;
        node.data.insert(
            "normalization".into(),
            json!({"status": "manual", "reason": "The service call target is ambiguous, unresolved or nonlocal within this application.", "runtime_proved": false}),
        );
        node.boundary = Some(
            "Source evidence is retained, but the service call requires an explicit target decision."
                .into(),
        );
        return true;
    }
    node.children
        .iter_mut()
        .any(|child| demote_service_route(child, method, path))
}

fn demote_unresolved_service_calls(application: &mut ApplicationNode) {
    loop {
        let mut normalized = Vec::new();
        collect(application, &mut normalized);
        let invalid = normalized.iter().enumerate().find_map(|(index, route)| {
            crate::application_ir::route_service_calls(route)
                .into_iter()
                .any(|(method, path)| {
                    crate::application_ir::route_service_target_count(
                        &normalized,
                        index,
                        &method,
                        &path,
                    ) != 1
                })
                .then(|| (route.method.clone(), route.path.clone()))
        });
        let Some((method, path)) = invalid else {
            return;
        };
        demote_service_route(application, &method, &path);
    }
}

pub(super) fn routes(project: &Project<'_>, applications: &mut [ApplicationNode]) -> Result<()> {
    let mut cache = BTreeMap::new();
    for application in applications {
        normalize_node(project, &mut cache, application, None);
        let mut normalized = Vec::new();
        collect(application, &mut normalized);
        if !normalized.is_empty() && crate::application_ir::route_matchers_overlap(&normalized) {
            reject_portable(application);
            continue;
        }
        demote_unresolved_service_calls(application);
    }
    Ok(())
}
