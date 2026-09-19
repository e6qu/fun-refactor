use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "fr-application-ir-1";
mod write;
pub use write::{write_routes, write_static_component};

fn encoded_size<T: Serialize + ?Sized>(value: &T, limit: usize) -> Result<usize, String> {
    struct Counter {
        size: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let size = self
                .size
                .checked_add(bytes.len())
                .filter(|size| *size <= self.limit)
                .ok_or_else(|| std::io::Error::other("serialized IR exceeds its byte bound"))?;
            self.size = size;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { size: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|error| error.to_string())?;
    Ok(counter.size)
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpMiddleware {
    pub name: String,
    pub request_order: usize,
}

pub fn dotted_identifier(name: &str) -> bool {
    !name.is_empty() && name.len() <= 160 && name.split('.').all(identifier)
}

pub fn validate_middleware(chain: &[HttpMiddleware]) -> Result<(), String> {
    if chain.len() > 64 {
        return Err("middleware chain exceeds its entry bound.".into());
    }
    let mut orders = BTreeSet::new();
    for entry in chain {
        if !dotted_identifier(&entry.name)
            || !(1..=chain.len()).contains(&entry.request_order)
            || !orders.insert(entry.request_order)
        {
            return Err(
                "middleware entries need dotted names and a unique 1-based request order.".into(),
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteBundle {
    pub schema: String,
    pub routes: Vec<HttpRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub middleware: Vec<HttpMiddleware>,
}

fn service_calls(expr: &HttpExpression, calls: &mut Vec<(String, String)>) {
    match expr {
        HttpExpression::Service { method, path } => calls.push((method.clone(), path.clone())),
        HttpExpression::Object { fields } => fields
            .values()
            .for_each(|value| service_calls(value, calls)),
        HttpExpression::Array { items } => {
            items.iter().for_each(|value| service_calls(value, calls))
        }
        _ => (),
    }
}

pub fn route_service_calls(route: &HttpRoute) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    service_calls(&route.response, &mut calls);
    calls
}

pub fn route_service_target_count(
    routes: &[HttpRoute],
    index: usize,
    method: &str,
    path: &str,
) -> usize {
    routes
        .iter()
        .enumerate()
        .filter(|(target, candidate)| {
            *target != index
                && crate::framework_kernel::service_route_candidate(
                    true,
                    candidate.path == path,
                    true,
                    candidate.method == method,
                )
        })
        .count()
}

pub fn route_matchers_overlap(routes: &[HttpRoute]) -> bool {
    for (index, route) in routes.iter().enumerate() {
        for previous in &routes[..index] {
            let left: Vec<_> = previous.path.split('/').collect();
            let right: Vec<_> = route.path.split('/').collect();
            if left.len() == right.len()
                && left.iter().zip(&right).all(|(left, right)| {
                    left == right || left.starts_with('{') || right.starts_with('{')
                })
                && (previous.path != route.path || previous.method == route.method)
            {
                return true;
            }
        }
    }
    false
}

pub fn validate_routes(routes: &[HttpRoute]) -> Result<(), String> {
    if routes.is_empty() || routes.len() > 256 {
        return Err("route bundle requires 1..256 endpoints".into());
    }
    for (index, route) in routes.iter().enumerate() {
        route.validate()?;
        for (method, path) in route_service_calls(route) {
            if route_service_target_count(routes, index, &method, &path) != 1 {
                return Err(
                    "service calls require exactly one same-bundle route target; ambiguous, unresolved and nonlocal targets remain manual."
                        .into(),
                );
            }
        }
    }
    if route_matchers_overlap(routes) {
        return Err("route matchers overlap or duplicate an endpoint.".into());
    }
    encoded_size(routes, 1_048_576)?;
    Ok(())
}

impl RouteBundle {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "fr-http-application-1" {
            return Err("unsupported HTTP application schema".into());
        }
        validate_routes(&self.routes)?;
        validate_middleware(&self.middleware)?;
        encoded_size(self, 1_048_576)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Adapter {
    Nextjs,
    Fastapi,
    Express,
    GoNetHttp,
    React,
}

impl Adapter {
    pub const ALL: [Self; 5] = [
        Self::Nextjs,
        Self::Fastapi,
        Self::Express,
        Self::GoNetHttp,
        Self::React,
    ];

    pub fn from_framework(name: &str) -> Option<Self> {
        match name {
            "nextjs" | "nextjs-app" => Some(Self::Nextjs),
            "fastapi" => Some(Self::Fastapi),
            "express" => Some(Self::Express),
            "go-net-http" => Some(Self::GoNetHttp),
            "react" => Some(Self::React),
            _ => None,
        }
    }

    pub fn code(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Nextjs => "nextjs",
            Self::Fastapi => "fastapi",
            Self::Express => "express",
            Self::GoNetHttp => "go-net-http",
            Self::React => "react",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureKind {
    JsonRoute,
    PathJsonRoute,
    ValidatedJsonRoute,
    StaticComponent,
}

impl FeatureKind {
    pub const ALL: [Self; 4] = [
        Self::JsonRoute,
        Self::PathJsonRoute,
        Self::ValidatedJsonRoute,
        Self::StaticComponent,
    ];
    pub fn code(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::JsonRoute => "json-route",
            Self::PathJsonRoute => "path-json-route",
            Self::ValidatedJsonRoute => "validated-json-route",
            Self::StaticComponent => "static-component",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HttpInputSource {
    Query,
    JsonBody,
}

impl HttpInputSource {
    pub fn code(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HttpScalar {
    String,
    Integer,
    Boolean,
}

impl HttpScalar {
    pub fn code(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpInput {
    pub name: String,
    pub source: HttpInputSource,
    pub scalar: HttpScalar,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpDependency {
    pub binding: String,
    pub provider: String,
    pub security: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpValidationIssue {
    pub source: HttpInputSource,
    pub name: String,
    pub expected: HttpScalar,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum HttpExpression {
    Literal {
        value: Value,
    },
    Path {
        name: String,
    },
    Input {
        name: String,
    },
    Service {
        method: String,
        path: String,
    },
    Object {
        fields: BTreeMap<String, HttpExpression>,
    },
    Array {
        items: Vec<HttpExpression>,
    },
}

impl HttpExpression {
    pub fn validate(&self, parameters: &BTreeSet<String>) -> Result<(), String> {
        self.validate_with_inputs(parameters, &BTreeSet::new())
    }

    pub fn validate_with_inputs(
        &self,
        parameters: &BTreeSet<String>,
        inputs: &BTreeSet<String>,
    ) -> Result<(), String> {
        fn walk(
            expr: &HttpExpression,
            parameters: &BTreeSet<String>,
            inputs: &BTreeSet<String>,
            depth: usize,
            nodes: &mut usize,
        ) -> Result<(), String> {
            *nodes += 1;
            if depth > 32 || *nodes > 1024 {
                return Err("HTTP expression exceeds its node or depth bound.".into());
            }
            match expr {
                HttpExpression::Literal { value } => {
                    match value {
                        Value::Null | Value::Bool(_) => (),
                        Value::String(value) if value.len() <= 65536 => (),
                        Value::Number(number) if number.as_i64().is_some_and(|value| (-9007199254740991..=9007199254740991).contains(&value)) => (),
                        _ => return Err("literals require JSON primitives and exactly representable signed integers.".into()),
                    }
                }
                HttpExpression::Path { name } if parameters.contains(name) => (),
                HttpExpression::Path { .. } => return Err("HTTP expression refers to an undeclared path parameter.".into()),
                HttpExpression::Input { name } if inputs.contains(name) => (),
                HttpExpression::Input { .. } => return Err("HTTP expression refers to an undeclared request input.".into()),
                HttpExpression::Service { method, path } => {
                    let provisional = HttpRoute {
                        method: method.clone(),
                        path: path.clone(),
                        inputs: Vec::new(),
                        dependencies: Vec::new(),
                        status: 200,
                        response: HttpExpression::Literal { value: Value::Null },
                    };
                    if !matches!(
                        method.as_str(),
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
                    ) || provisional.parameters().is_err()
                        || path.contains('{')
                    {
                        return Err("service calls need a portable method and a bounded literal root-relative path.".into());
                    }
                }
                HttpExpression::Object { fields } => {
                    for (name, value) in fields {
                        if name.len() > 256 { return Err("HTTP object field exceeds its byte bound".into()); }
                        walk(value, parameters, inputs, depth + 1, nodes)?;
                    }
                }
                HttpExpression::Array { items } => for value in items { walk(value, parameters, inputs, depth + 1, nodes)?; },
            }
            Ok(())
        }
        walk(self, parameters, inputs, 0, &mut 0)?;
        encoded_size(self, 1_048_576)?;
        Ok(())
    }

    pub fn evaluate(&self, parameters: &BTreeMap<String, String>) -> Result<Value, String> {
        self.evaluate_with_inputs(parameters, &BTreeMap::new())
    }

    pub fn evaluate_with_inputs(
        &self,
        parameters: &BTreeMap<String, String>,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.validate_with_inputs(
            &parameters.keys().cloned().collect(),
            &inputs.keys().cloned().collect(),
        )?;
        if parameters.len() > 32 || parameters.values().any(|value| value.len() > 4096) {
            return Err("HTTP evaluation parameters exceed their bounds.".into());
        }
        fn has_service(expr: &HttpExpression) -> bool {
            match expr {
                HttpExpression::Service { .. } => true,
                HttpExpression::Object { fields } => fields.values().any(has_service),
                HttpExpression::Array { items } => items.iter().any(has_service),
                _ => false,
            }
        }
        if has_service(self) {
            return Err("service call results require runtime evidence.".into());
        }
        fn run(
            expr: &HttpExpression,
            parameters: &BTreeMap<String, String>,
            inputs: &BTreeMap<String, Value>,
        ) -> Value {
            match expr {
                HttpExpression::Literal { value } => value.clone(),
                HttpExpression::Path { name } => Value::String(parameters[name].clone()),
                HttpExpression::Input { name } => inputs[name].clone(),
                HttpExpression::Service { .. } => Value::Null,
                HttpExpression::Object { fields } => Value::Object(
                    fields
                        .iter()
                        .map(|(name, expr)| (name.clone(), run(expr, parameters, inputs)))
                        .collect(),
                ),
                HttpExpression::Array { items } => Value::Array(
                    items
                        .iter()
                        .map(|expr| run(expr, parameters, inputs))
                        .collect(),
                ),
            }
        }
        let value = run(self, parameters, inputs);
        encoded_size(&value, 1_048_576)?;
        Ok(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRoute {
    pub method: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<HttpInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<HttpDependency>,
    pub status: u16,
    pub response: HttpExpression,
}

impl HttpRoute {
    pub fn parameters(&self) -> Result<BTreeSet<String>, String> {
        if !self.path.starts_with('/')
            || self.path.len() > 2048
            || self.path.contains("//")
            || self.path.len() > 1 && self.path.ends_with('/')
            || !self.path.is_ascii()
        {
            return Err("route needs a bounded absolute literal path.".into());
        }
        let mut parameters = BTreeSet::new();
        for segment in self.path.split('/') {
            if segment.starts_with('{') && segment.ends_with('}') {
                let name = &segment[1..segment.len() - 1];
                if !identifier(name) || !parameters.insert(name.into()) {
                    return Err("path parameters must have unique simple names.".into());
                }
            } else if matches!(segment, "." | "..")
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte))
            {
                return Err("route pattern requires a separately modeled matcher.".into());
            }
        }
        if parameters.len() > 32 {
            return Err("route exceeds its path parameter bound".into());
        }
        Ok(parameters)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
        ) {
            return Err("unsupported portable JSON HTTP method".into());
        }
        if !crate::framework_kernel::application_json_status_admitted(self.status as usize) {
            return Err("status does not admit a portable JSON response body.".into());
        }
        let parameters = self.parameters()?;
        if self.inputs.len() > 32 {
            return Err("route exceeds its request input bound".into());
        }
        let method = match self.method.as_str() {
            "GET" => 0,
            "POST" => 1,
            "PUT" => 2,
            "PATCH" => 3,
            "DELETE" => 4,
            "OPTIONS" => 5,
            _ => unreachable!(),
        };
        let mut inputs = BTreeSet::new();
        for input in &self.inputs {
            if !identifier(&input.name)
                || parameters.contains(&input.name)
                || !inputs.insert(input.name.clone())
                || !crate::framework_kernel::application_request_input_admitted(
                    method,
                    input.source.code(),
                    input.scalar.code(),
                )
            {
                return Err(
                    "request input name, source or scalar type is not portable for this method."
                        .into(),
                );
            }
        }
        if self.dependencies.len() > 16 {
            return Err("route exceeds its dependency bound".into());
        }
        let mut bindings = BTreeSet::new();
        for dependency in &self.dependencies {
            if !identifier(&dependency.binding)
                || !dotted_identifier(&dependency.provider)
                || parameters.contains(&dependency.binding)
                || inputs.contains(&dependency.binding)
                || !bindings.insert(dependency.binding.clone())
            {
                return Err(
                    "dependency bindings need unique names distinct from inputs and path parameters."
                        .into(),
                );
            }
        }
        self.response.validate_with_inputs(&parameters, &inputs)
    }

    pub fn validate_request(
        &self,
        query: &BTreeMap<String, String>,
        body: Option<&Value>,
    ) -> Result<BTreeMap<String, Value>, Vec<HttpValidationIssue>> {
        if self.validate().is_err()
            || query.len() > 64
            || query.values().any(|value| value.len() > 4096)
        {
            return Err(self
                .inputs
                .iter()
                .map(|input| HttpValidationIssue {
                    source: input.source,
                    name: input.name.clone(),
                    expected: input.scalar,
                })
                .collect());
        }
        let body = body.and_then(Value::as_object);
        let mut values = BTreeMap::new();
        let mut issues = Vec::new();
        for input in &self.inputs {
            let value = match input.source {
                HttpInputSource::Query => query
                    .get(&input.name)
                    .and_then(|value| query_scalar(value, input.scalar)),
                HttpInputSource::JsonBody => body
                    .and_then(|body| body.get(&input.name))
                    .and_then(|value| body_scalar(value, input.scalar)),
            };
            if let Some(value) = value {
                values.insert(input.name.clone(), value);
            } else {
                issues.push(HttpValidationIssue {
                    source: input.source,
                    name: input.name.clone(),
                    expected: input.scalar,
                });
            }
        }
        if issues.is_empty() {
            Ok(values)
        } else {
            Err(issues)
        }
    }

    pub fn feature_kind(&self) -> Result<FeatureKind, String> {
        Ok(if !self.inputs.is_empty() {
            FeatureKind::ValidatedJsonRoute
        } else if self.parameters()?.is_empty() {
            FeatureKind::JsonRoute
        } else {
            FeatureKind::PathJsonRoute
        })
    }
}

fn query_scalar(value: &str, scalar: HttpScalar) -> Option<Value> {
    match scalar {
        HttpScalar::String if value.len() <= 4096 => Some(Value::String(value.to_owned())),
        HttpScalar::Integer => {
            if value == "0"
                || value
                    .strip_prefix('-')
                    .unwrap_or(value)
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| byte.is_ascii_digit() && (index > 0 || byte != b'0'))
            {
                integer_value(value)
            } else {
                None
            }
        }
        HttpScalar::Boolean if value == "true" => Some(Value::Bool(true)),
        HttpScalar::Boolean if value == "false" => Some(Value::Bool(false)),
        _ => None,
    }
}

fn integer_value(value: &str) -> Option<Value> {
    let value = value.parse::<i64>().ok()?;
    (-9_007_199_254_740_991..=9_007_199_254_740_991)
        .contains(&value)
        .then(|| Value::Number(value.into()))
}

fn body_scalar(value: &Value, scalar: HttpScalar) -> Option<Value> {
    match (scalar, value) {
        (HttpScalar::String, Value::String(value)) if value.len() <= 4096 => {
            Some(Value::String(value.clone()))
        }
        (HttpScalar::Integer, Value::Number(value)) => value
            .as_i64()
            .filter(|value| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(value))
            .or_else(|| {
                value.as_f64().and_then(|value| {
                    (value.is_finite()
                        && value.fract() == 0.0
                        && (-9_007_199_254_740_991.0..=9_007_199_254_740_991.0).contains(&value))
                    .then_some(value as i64)
                })
            })
            .map(|value| Value::Number(value.into())),
        (HttpScalar::Boolean, Value::Bool(value)) => Some(Value::Bool(*value)),
        _ => None,
    }
}

pub fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || index > 0 && byte.is_ascii_digit()
        })
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentState {
    pub name: String,
    pub setter: String,
    pub initial: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ComponentEvent {
    SetState { state: String, value: Value },
    ToggleState { state: String },
}

fn state_literal(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) => true,
        Value::String(value) => value.len() <= 65_536,
        Value::Number(number) => number
            .as_i64()
            .is_some_and(|value| (-9007199254740991..=9007199254740991).contains(&value)),
        _ => false,
    }
}

fn literal_kind(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::String(_) => 2,
        _ => 3,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum StaticNode {
    Text {
        value: String,
    },
    State {
        name: String,
    },
    Element {
        tag: String,
        attributes: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        events: BTreeMap<String, ComponentEvent>,
        children: Vec<StaticNode>,
    },
}

impl StaticNode {
    fn validate(
        &self,
        depth: usize,
        nodes: &mut usize,
        max_depth: &mut usize,
        states: &BTreeMap<String, Value>,
    ) -> Result<(), String> {
        *nodes += 1;
        *max_depth = (*max_depth).max(depth);
        if depth > 32 || *nodes > 1024 {
            return Err("static component exceeds its node or depth bound.".into());
        }
        match self {
            Self::Text { value } => {
                if value.is_empty() || value.len() > 65_536 || value.trim() != value {
                    return Err("static text must be bounded and have explicit whitespace.".into());
                }
            }
            Self::State { name } => {
                if !states.contains_key(name) {
                    return Err("state references require a declared component state.".into());
                }
            }
            Self::Element {
                tag,
                attributes,
                events,
                children,
            } => {
                if tag.is_empty()
                    || tag.len() > 128
                    || !tag.bytes().enumerate().all(|(index, byte)| {
                        byte.is_ascii_lowercase()
                            || index > 0 && (byte.is_ascii_digit() || byte == b'-')
                    })
                {
                    return Err("static elements require lowercase intrinsic tag names.".into());
                }
                for (name, value) in attributes {
                    if name.is_empty()
                        || name.len() > 128
                        || value.len() > 65_536
                        || name.starts_with("on")
                        || matches!(name.as_str(), "style" | "dangerouslySetInnerHTML")
                        || !name.bytes().enumerate().all(|(index, byte)| {
                            byte.is_ascii_alphabetic()
                                || index > 0
                                    && (byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
                        })
                    {
                        return Err(
                            "static component attribute exceeds the literal safe subset.".into(),
                        );
                    }
                }
                for (name, event) in events {
                    let valid_name = name.len() <= 64
                        && name.strip_prefix("on").is_some_and(|rest| {
                            rest.bytes()
                                .next()
                                .is_some_and(|byte| byte.is_ascii_uppercase())
                                && rest.bytes().all(|byte| byte.is_ascii_alphanumeric())
                        });
                    let valid_event = match event {
                        ComponentEvent::ToggleState { state } => states
                            .get(state)
                            .is_some_and(|initial| initial.is_boolean()),
                        ComponentEvent::SetState { state, value } => {
                            states.get(state).is_some_and(|initial| {
                                state_literal(value) && literal_kind(value) == literal_kind(initial)
                            })
                        }
                    };
                    if !valid_name || !valid_event {
                        return Err(
                            "component events need an on* name and a declared state with a matching literal."
                                .into(),
                        );
                    }
                }
                for child in children {
                    child.validate(depth + 1, nodes, max_depth, states)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StaticComponent {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub client: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state: Vec<ComponentState>,
    pub root: StaticNode,
}

impl StaticComponent {
    pub fn validate(&self) -> Result<(), String> {
        if !identifier(&self.name) || !self.name.chars().next().is_some_and(char::is_uppercase) {
            return Err("static component needs a bounded upper-case identifier.".into());
        }
        if self.state.len() > 16 {
            return Err("component exceeds its state declaration bound.".into());
        }
        let mut states = BTreeMap::new();
        let mut setters = BTreeSet::new();
        for state in &self.state {
            if !identifier(&state.name)
                || state.name.chars().next().is_some_and(char::is_uppercase)
                || !identifier(&state.setter)
                || !setters.insert(state.setter.clone())
                || !state_literal(&state.initial)
            {
                return Err(
                    "component state needs unique simple names and a bounded literal initial value."
                        .into(),
                );
            }
            if states
                .insert(state.name.clone(), state.initial.clone())
                .is_some()
            {
                return Err("component state names must be unique.".into());
            }
        }
        if !crate::framework_kernel::component_hooks_compatible(self.client, self.state.len()) {
            return Err("component state requires an explicit client boundary.".into());
        }
        let mut nodes = 0;
        let mut depth = 0;
        self.root.validate(0, &mut nodes, &mut depth, &states)?;
        let encoded = encoded_size(self, 1_048_576)?;
        if !crate::framework_kernel::application_static_resources_admitted(nodes, depth, encoded) {
            return Err("static component exceeds its resource policy.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationNode {
    pub id: String,
    pub kind: String,
    pub source: Value,
    pub data: BTreeMap<String, Value>,
    pub children: Vec<ApplicationNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<HttpRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub middleware: Vec<HttpMiddleware>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<StaticComponent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationIr {
    pub schema: String,
    pub revision: String,
    pub applications: Vec<ApplicationNode>,
    pub omissions: Value,
    pub runtime_proved: bool,
}

impl ApplicationIr {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA
            || self.runtime_proved
            || self.revision.len() != 64
            || !self
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("application schema or runtime proof claim is invalid.".into());
        }
        let mut ids = BTreeSet::new();
        fn walk(
            node: &ApplicationNode,
            depth: usize,
            ids: &mut BTreeSet<String>,
        ) -> Result<(), String> {
            if depth > 64
                || ids.len() >= 4096
                || node.id.is_empty()
                || node.id.len() > 256
                || !ids.insert(node.id.clone())
            {
                return Err(
                    "application hierarchy has duplicate identities or exceeds its bounds.".into(),
                );
            }
            if let Some(route) = &node.route {
                route.validate()?;
            }
            validate_middleware(&node.middleware)?;
            if let Some(component) = &node.component {
                component.validate()?;
            }
            for child in &node.children {
                walk(child, depth + 1, ids)?;
            }
            Ok(())
        }
        for node in &self.applications {
            walk(node, 0, &mut ids)?;
        }
        encoded_size(self, 4_194_304)?;
        Ok(())
    }
}
