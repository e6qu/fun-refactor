use crate::lang::Language;
use crate::project::framework_kernel;
use tree_sitter::Node;

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

pub(super) struct ServiceCall {
    pub target: String,
    pub target_kind: &'static str,
    pub method: Option<String>,
    pub line: usize,
    pub basis: &'static str,
    pub query_or_fragment_omitted: bool,
    pub credentials_omitted: bool,
}

pub(super) struct ServiceGap {
    pub line: usize,
    pub reason: &'static str,
    pub basis: &'static str,
}

#[derive(Default)]
pub(super) struct ServiceCalls {
    pub entries: Vec<ServiceCall>,
    pub gaps: Vec<ServiceGap>,
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.is_extra())
        .collect()
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn literal(node: Node<'_>, source: &str) -> Option<String> {
    if !matches!(node.kind(), "string" | "string_literal") {
        return None;
    }
    let raw = text(node, source);
    let quote = raw.chars().next()?;
    if !matches!(quote, '\'' | '"') || raw.contains(['\\', '\n', '\r']) {
        return None;
    }
    raw.strip_prefix(quote)
        .and_then(|value| value.strip_suffix(quote))
        .map(str::to_owned)
}

fn sanitize_target(raw: &str) -> (String, &'static str, bool, bool) {
    let boundary = raw.find(['?', '#']).unwrap_or(raw.len());
    let mut target = raw[..boundary].to_owned();
    let kind = match framework_kernel::service_target_kind(
        raw.starts_with("http://") || raw.starts_with("https://"),
        raw.starts_with('/'),
    ) {
        2 => "external-http",
        1 => "local-http",
        _ => "relative-http",
    };
    let credentials_omitted = if let Some(scheme) = target.find("://") {
        let authority_start = scheme + 3;
        let authority_end = target[authority_start..]
            .find('/')
            .map_or(target.len(), |offset| authority_start + offset);
        if let Some(at) = target[authority_start..authority_end].rfind('@') {
            target.replace_range(authority_start..authority_start + at + 1, "");
            true
        } else {
            false
        }
    } else {
        false
    };
    let flags =
        framework_kernel::service_redaction_flags(boundary < raw.len(), credentials_omitted);
    (target, kind, flags & 1 != 0, flags & 2 != 0)
}

fn typescript_call(call: Node<'_>, source: &str) -> Option<(Option<String>, &'static str)> {
    let function = call.child_by_field_name("function")?;
    if function.kind() == "identifier" && text(function, source) == "fetch" {
        return Some((None, "web-fetch-call"));
    }
    if function.kind() != "member_expression" {
        return None;
    }
    let object = function.child_by_field_name("object")?;
    let property = function.child_by_field_name("property")?;
    let method = text(property, source);
    (object.kind() == "identifier" && text(object, source) == "axios" && METHODS.contains(&method))
        .then(|| (Some(method.to_uppercase()), "axios-method-call"))
}

fn python_call(call: Node<'_>, source: &str) -> Option<(Option<String>, &'static str)> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "attribute" {
        return None;
    }
    let object = function.child_by_field_name("object")?;
    let attribute = function.child_by_field_name("attribute")?;
    let receiver = text(object, source);
    let method = text(attribute, source);
    (object.kind() == "identifier"
        && matches!(receiver, "requests" | "httpx")
        && METHODS.contains(&method))
    .then(|| {
        (
            Some(method.to_uppercase()),
            if receiver == "requests" {
                "python-requests-method-call"
            } else {
                "python-httpx-method-call"
            },
        )
    })
}

pub(super) fn read(function: Node<'_>, language: Language, source: &str) -> ServiceCalls {
    let root = if function.kind() == "variable_declarator" {
        function.child_by_field_name("value").unwrap_or(function)
    } else {
        function
    };
    let mut stack = vec![root];
    let mut result = ServiceCalls::default();
    while let Some(node) = stack.pop() {
        let is_nested_callable = node != root
            && matches!(
                node.kind(),
                "function_declaration"
                    | "function_expression"
                    | "arrow_function"
                    | "function_definition"
                    | "lambda"
            );
        if is_nested_callable {
            continue;
        }
        let call_kind = match language {
            Language::TypeScript | Language::Tsx if node.kind() == "call_expression" => {
                typescript_call(node, source)
            }
            Language::Python if node.kind() == "call" => python_call(node, source),
            _ => None,
        };
        if let Some((method, basis)) = call_kind {
            let argument = node
                .child_by_field_name("arguments")
                .into_iter()
                .flat_map(children)
                .find(|argument| argument.kind() != "keyword_argument");
            if let Some(target) = argument.and_then(|argument| literal(argument, source)) {
                let (target, target_kind, query_or_fragment_omitted, credentials_omitted) =
                    sanitize_target(&target);
                result.entries.push(ServiceCall {
                    target,
                    target_kind,
                    method,
                    line: node.start_position().row + 1,
                    basis,
                    query_or_fragment_omitted,
                    credentials_omitted,
                });
            } else {
                result.gaps.push(ServiceGap {
                    line: node.start_position().row + 1,
                    reason:
                        "The outbound HTTP target is dynamic or exceeds the plain string subset.",
                    basis,
                });
            }
        }
        stack.extend(children(node));
    }
    result.entries.sort_by_key(|entry| entry.line);
    result.gaps.sort_by_key(|gap| gap.line);
    result
}
