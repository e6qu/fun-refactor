use crate::parse::Parsed;
use crate::transpile::routes::Endpoint;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

pub(super) struct FastRoute {
    pub endpoint: Endpoint,
    pub name_offset: usize,
    pub decorator_offset: usize,
}

#[derive(Default)]
pub(super) struct FastRoutes {
    pub entries: Vec<FastRoute>,
    pub gaps: Vec<(usize, &'static str)>,
    pub claimed_lines: BTreeSet<usize>,
}

pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| !n.is_extra())
        .collect()
}

pub(super) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

pub(super) fn literal(node: Node<'_>, source: &str) -> Option<String> {
    let raw = text(node, source);
    let quote = raw.chars().next()?;
    if node.kind() != "string" || !matches!(quote, '\'' | '"') {
        return None;
    }
    let value = raw.strip_prefix(quote)?.strip_suffix(quote)?;
    (!value.contains([quote, '\\', '\n', '\r'])).then(|| value.to_owned())
}

fn receivers(parsed: &Parsed, source: &str) -> BTreeSet<String> {
    let nodes = children(parsed.root());
    let mut constructors = BTreeSet::new();
    for node in &nodes {
        let from_fastapi = node.kind() == "import_from_statement"
            && node
                .child_by_field_name("module_name")
                .is_some_and(|m| text(m, source) == "fastapi");
        if !from_fastapi && node.kind() != "import_statement" {
            continue;
        }
        for import in children(*node) {
            if node.child_by_field_name("module_name") == Some(import) {
                continue;
            }
            let (name, alias) = if import.kind() == "aliased_import" {
                let (Some(name), Some(alias)) = (
                    import.child_by_field_name("name"),
                    import.child_by_field_name("alias"),
                ) else {
                    continue;
                };
                (text(name, source), text(alias, source))
            } else {
                (text(import, source), text(import, source))
            };
            if from_fastapi && matches!(name, "FastAPI" | "APIRouter") {
                constructors.insert(alias.to_owned());
            } else if !from_fastapi && name == "fastapi" {
                constructors.insert(format!("{alias}.FastAPI"));
                constructors.insert(format!("{alias}.APIRouter"));
            }
        }
    }
    let mut bindings: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for node in nodes {
        if node.kind() != "expression_statement" {
            continue;
        }
        for assignment in children(node)
            .into_iter()
            .filter(|n| n.kind() == "assignment")
        {
            let Some(left) = assignment
                .child_by_field_name("left")
                .filter(|n| n.kind() == "identifier")
            else {
                continue;
            };
            let valid = assignment
                .child_by_field_name("right")
                .filter(|n| n.kind() == "call")
                .and_then(|n| n.child_by_field_name("function"))
                .is_some_and(|n| constructors.contains(text(n, source)));
            bindings
                .entry(text(left, source).to_owned())
                .or_default()
                .push(valid);
        }
    }
    bindings
        .into_iter()
        .filter_map(|(name, occurrences)| (occurrences == [true]).then_some(name))
        .collect()
}

fn simple_callable(node: Node<'_>, source: &str) -> Option<String> {
    let value = text(node, source);
    matches!(node.kind(), "identifier" | "attribute")
        .then(|| {
            value
                .split('.')
                .all(|part| {
                    let mut chars = part.chars();
                    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                        && chars.all(|c| c.is_alphanumeric() || c == '_')
                })
                .then(|| value.to_owned())
        })
        .flatten()
}

pub(super) struct Dependency {
    pub binding: String,
    pub provider: Option<String>,
    pub marker: String,
    pub line: usize,
}

pub(super) fn dependencies(parameter: Node<'_>, source: &str) -> Vec<Dependency> {
    let Some(binding) = parameter
        .child_by_field_name("name")
        .or_else(|| {
            children(parameter)
                .into_iter()
                .find(|node| node.kind() == "identifier")
        })
        .filter(|node| node.kind() == "identifier")
    else {
        return Vec::new();
    };
    let mut stack = vec![parameter];
    let mut calls = Vec::new();
    while let Some(node) = stack.pop() {
        if node.kind() == "call"
            && node
                .child_by_field_name("function")
                .is_some_and(|function| {
                    matches!(
                        text(function, source).rsplit('.').next(),
                        Some("Depends" | "Security")
                    )
                })
        {
            calls.push(node);
            continue;
        }
        stack.extend(children(node));
    }
    calls.sort_by_key(Node::start_byte);
    calls
        .into_iter()
        .map(|call| {
            let function = call.child_by_field_name("function").unwrap();
            let marker = text(function, source)
                .rsplit('.')
                .next()
                .unwrap()
                .to_owned();
            let mut providers: Vec<_> = call
                .child_by_field_name("arguments")
                .into_iter()
                .flat_map(children)
                .filter(|node| {
                    node.kind() != "keyword_argument" && node.kind() != "dictionary_splat"
                })
                .collect();
            providers.extend(keyword(call, "dependency", source));
            let provider = match providers.as_slice() {
                [provider] => simple_callable(*provider, source),
                _ => None,
            };
            Dependency {
                binding: text(binding, source).to_owned(),
                provider,
                marker,
                line: call.start_position().row + 1,
            }
        })
        .collect()
}

pub(super) struct Middleware {
    pub name: Option<String>,
    pub line: usize,
    pub form: &'static str,
}

pub(super) fn middleware(parsed: &Parsed, source: &str) -> Vec<Middleware> {
    let receivers = receivers(parsed, source);
    let mut result = Vec::new();
    for node in children(parsed.root()) {
        if node.kind() == "decorated_definition" {
            let parts = children(node);
            let function = parts
                .iter()
                .find(|part| part.kind() == "function_definition");
            for decorator in parts.iter().filter(|part| part.kind() == "decorator") {
                let Some(call) = children(*decorator)
                    .into_iter()
                    .find(|part| part.kind() == "call")
                else {
                    continue;
                };
                let Some(attribute) = call
                    .child_by_field_name("function")
                    .filter(|part| part.kind() == "attribute")
                else {
                    continue;
                };
                let (Some(object), Some(method)) = (
                    attribute.child_by_field_name("object"),
                    attribute.child_by_field_name("attribute"),
                ) else {
                    continue;
                };
                let http = call
                    .child_by_field_name("arguments")
                    .into_iter()
                    .flat_map(children)
                    .filter(|argument| argument.kind() != "keyword_argument")
                    .filter_map(|argument| literal(argument, source))
                    .collect::<Vec<_>>();
                if receivers.contains(text(object, source))
                    && text(method, source) == "middleware"
                    && http == ["http"]
                {
                    result.push(Middleware {
                        name: function
                            .and_then(|function| function.child_by_field_name("name"))
                            .map(|name| text(name, source).to_owned()),
                        line: decorator.start_position().row + 1,
                        form: "fastapi-http-decorator",
                    });
                }
            }
        } else if node.kind() == "expression_statement" {
            for call in children(node)
                .into_iter()
                .filter(|part| part.kind() == "call")
            {
                let Some(attribute) = call
                    .child_by_field_name("function")
                    .filter(|part| part.kind() == "attribute")
                else {
                    continue;
                };
                let (Some(object), Some(method)) = (
                    attribute.child_by_field_name("object"),
                    attribute.child_by_field_name("attribute"),
                ) else {
                    continue;
                };
                if !receivers.contains(text(object, source))
                    || text(method, source) != "add_middleware"
                {
                    continue;
                }
                let names: Vec<_> = call
                    .child_by_field_name("arguments")
                    .into_iter()
                    .flat_map(children)
                    .filter(|argument| {
                        argument.kind() != "keyword_argument"
                            && argument.kind() != "dictionary_splat"
                    })
                    .collect();
                result.push(Middleware {
                    name: names
                        .first()
                        .filter(|_| names.len() == 1)
                        .and_then(|name| simple_callable(*name, source)),
                    line: call.start_position().row + 1,
                    form: "fastapi-add-middleware",
                });
            }
        }
    }
    result.sort_by_key(|middleware| middleware.line);
    result
}

pub(super) fn keyword<'a>(call: Node<'a>, name: &str, source: &str) -> Vec<Node<'a>> {
    call.child_by_field_name("arguments")
        .into_iter()
        .flat_map(children)
        .filter(|n| {
            n.kind() == "keyword_argument"
                && n.child_by_field_name("name")
                    .is_some_and(|key| text(key, source) == name)
        })
        .filter_map(|n| n.child_by_field_name("value"))
        .collect()
}

pub(super) fn read(parsed: &Parsed, source: &str) -> FastRoutes {
    let mut result = FastRoutes::default();
    let receivers = receivers(parsed, source);
    for decorated in children(parsed.root())
        .into_iter()
        .filter(|n| n.kind() == "decorated_definition")
    {
        let parts = children(decorated);
        let Some(function) = parts.iter().find(|n| n.kind() == "function_definition") else {
            continue;
        };
        let Some(name) = function.child_by_field_name("name") else {
            continue;
        };
        for decorator in parts.iter().filter(|n| n.kind() == "decorator") {
            let Some(call) = children(*decorator)
                .into_iter()
                .find(|n| n.kind() == "call")
            else {
                continue;
            };
            let Some(attribute) = call
                .child_by_field_name("function")
                .filter(|n| n.kind() == "attribute")
            else {
                continue;
            };
            let (Some(object), Some(method)) = (
                attribute.child_by_field_name("object"),
                attribute.child_by_field_name("attribute"),
            ) else {
                continue;
            };
            if !receivers.contains(text(object, source)) {
                continue;
            }
            let method = text(method, source);
            if !METHODS.contains(&method) && !matches!(method, "api_route" | "route") {
                continue;
            }
            let line = decorator.start_position().row + 1;
            result.claimed_lines.insert(line);
            if !METHODS.contains(&method) {
                result.gaps.push((line, "Only verb decorators supply FastAPI route evidence; method-list decorators need further inspection."));
                continue;
            }
            let mut paths = keyword(call, "path", source);
            if let Some(first) = call.child_by_field_name("arguments").and_then(|n| {
                children(n)
                    .into_iter()
                    .find(|n| n.kind() != "keyword_argument" && n.kind() != "dictionary_splat")
            }) {
                paths.push(first);
            }
            let url = if paths.len() == 1 {
                literal(paths[0], source).filter(|s| s.starts_with('/'))
            } else {
                None
            };
            let Some(url) = url else {
                result.gaps.push((
                    line,
                    "The decorator path is not one plain absolute string literal.",
                ));
                continue;
            };
            result.entries.push(FastRoute {
                endpoint: Endpoint {
                    method: method.to_uppercase(),
                    url,
                    handler: Some(text(name, source).to_owned()),
                    line,
                },
                name_offset: name.start_byte(),
                decorator_offset: decorator.start_byte(),
            });
        }
    }
    result
}

pub(super) fn simple_type(node: Node<'_>, source: &str, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match node.kind() {
        "identifier" | "none" => true,
        "attribute" | "subscript" | "generic_type" | "type" | "type_parameter" | "union_type" => {
            let parts = children(node);
            !parts.is_empty() && parts.into_iter().all(|n| simple_type(n, source, depth + 1))
        }
        "binary_operator"
            if node
                .child_by_field_name("operator")
                .is_some_and(|n| text(n, source) == "|") =>
        {
            children(node)
                .into_iter()
                .all(|n| simple_type(n, source, depth + 1))
        }
        _ => false,
    }
}

pub(super) struct Input {
    pub binding: String,
    pub name: Option<String>,
    pub location: &'static str,
    pub ty: Option<String>,
    pub marker: String,
}

pub(super) fn input(parameter: Node<'_>, source: &str) -> Option<Input> {
    let binding = parameter.child_by_field_name("name").or_else(|| {
        children(parameter)
            .into_iter()
            .find(|n| n.kind() == "identifier")
    })?;
    if binding.kind() != "identifier" {
        return None;
    }
    let mut ty = parameter.child_by_field_name("type");
    let mut markers = Vec::new();
    if let Some(default) = parameter
        .child_by_field_name("value")
        .filter(|n| n.kind() == "call")
    {
        markers.push(default);
    }
    if let Some(annotation) = ty {
        let inner = if annotation.kind() == "type" {
            children(annotation).first().copied()?
        } else {
            annotation
        };
        if inner.kind() == "generic_type" {
            let parts = children(inner);
            if parts
                .first()
                .is_some_and(|n| text(*n, source).rsplit('.').next() == Some("Annotated"))
            {
                let arguments = parts
                    .iter()
                    .find(|n| n.kind() == "type_parameter")
                    .map(|n| children(*n))?;
                ty = arguments.first().copied();
                for metadata in arguments.into_iter().skip(1) {
                    let metadata = if metadata.kind() == "type" {
                        children(metadata).first().copied()?
                    } else {
                        metadata
                    };
                    if metadata.kind() != "call" {
                        return None;
                    }
                    markers.push(metadata);
                }
            }
        }
    }
    if markers.len() != 1 {
        return None;
    }
    let call = markers[0];
    let function = call.child_by_field_name("function")?;
    let marker = text(function, source).rsplit('.').next()?;
    let location = match marker {
        "Path" => "path",
        "Query" => "query",
        "Header" => "header",
        "Cookie" => "cookie",
        "Body" | "Form" | "File" => "body",
        _ => return None,
    };
    let aliases = keyword(call, "alias", source);
    let expanded = call
        .child_by_field_name("arguments")
        .into_iter()
        .flat_map(children)
        .any(|n| n.kind() == "dictionary_splat");
    let name = match aliases.as_slice() {
        _ if expanded => None,
        [] if !matches!(location, "body" | "header") => Some(text(binding, source).to_owned()),
        [alias] => literal(*alias, source).filter(|s| !s.is_empty()),
        _ => None,
    };
    Some(Input {
        binding: text(binding, source).to_owned(),
        name,
        location,
        ty: ty
            .filter(|n| simple_type(*n, source, 0))
            .map(|n| text(n, source).to_owned()),
        marker: marker.to_owned(),
    })
}
