//! What URLs a file serves, in the five frameworks that are not Next.js.
//!
//! `fr openapi` builds a contract from a tree of route files, and it could read
//! one shape of tree: a Next.js `app/api` directory. A service written with
//! Express, Flask, axum, gin or Spring declares the same thing. The command
//! had nothing to say about any of them.
//!
//! Each framework says it its own way. Express and gin call a method on a
//! router and hand it a path. Flask and Spring put the path in a decorator or
//! an annotation above the handler. axum builds a router by chaining `.route`
//! calls. What they agree about is the pair that matters: a method and a URL,
//! answered by a named function.
//!
//! Path parameters differ too. Express, gin and axum write `:id`, Flask writes
//! `<int:id>`, Spring and OpenAPI write `{id}`. The last is the one a contract
//! uses, so every reader spells its own into that.

use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::Result;
use std::path::Path;
use tree_sitter::Node;

/// The methods a route declaration may name, lowercased.
const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// One endpoint a file declares, whatever framework declared it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The HTTP method, uppercased.
    pub method: String,
    /// The URL it answers, with path parameters spelled `{name}`.
    pub url: String,
    /// The function that answers it, where the declaration names one.
    pub handler: Option<String>,
    /// The line the declaration sits on, for a report that has to point at it.
    pub line: usize,
}

/// The frameworks this reads, one per way of declaring a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framework {
    Express,
    Flask,
    Axum,
    Gin,
    Spring,
}

impl std::fmt::Display for Framework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Framework::Express => "express",
            Framework::Flask => "flask",
            Framework::Axum => "axum",
            Framework::Gin => "gin",
            Framework::Spring => "spring",
        };
        f.write_str(name)
    }
}

/// Every endpoint this file declares, and the framework that declared them.
///
/// `None` where the file declares no routes at all. A file in one of these
/// languages that uses none of its framework is not an error. Most files in a
/// service are not route files.
pub fn endpoints_in(path: &Path) -> Result<Option<(Framework, Vec<Endpoint>)>> {
    let Some(language) = crate::lang::detect(path) else {
        return Ok(None);
    };
    let source = std::fs::read_to_string(path)?;
    Ok(endpoints_of(&source, language))
}

/// The same, from text a caller already has.
pub fn endpoints_of(source: &str, language: Language) -> Option<(Framework, Vec<Endpoint>)> {
    let parsed = Parsers::new().parse(language, source).ok()?;
    let root = parsed.tree.root_node();
    let lines = LineIndex::new(source);
    let found = match language {
        Language::TypeScript | Language::Tsx => express(root, source, &lines),
        Language::Python => flask(root, source, &lines),
        Language::Rust => axum(root, source, &lines),
        Language::Go => gin(root, source, &lines),
        Language::Java => spring(root, source, &lines),
        _ => return None,
    };
    found.filter(|(_, endpoints)| !endpoints.is_empty())
}

/// Where each byte falls, so a report can name a line.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (at, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(at + 1);
            }
        }
        Self { starts }
    }

    fn line(&self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(at) => at + 1,
            Err(at) => at,
        }
    }
}

/// Every node in the tree, in the order the file writes them.
fn walk(root: Node<'_>) -> Vec<Node<'_>> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    let mut cursor = root.walk();
    while let Some(node) = stack.pop() {
        found.push(node);
        stack.extend(node.children(&mut cursor));
    }
    found.sort_by_key(|n| n.start_byte());
    found
}

/// The text between a pair of quotes, when the text is a string literal.
fn quoted(text: &str) -> Option<String> {
    let text = text.trim();
    let quote = text.chars().next().filter(|c| "\"'`".contains(*c))?;
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// A URL with this framework's path parameters spelled the way a contract does.
///
/// Express, gin and axum write `:id`. Flask writes `<id>` and `<int:id>`, where
/// the part before the colon is a converter and not the name. Spring and
/// OpenAPI write `{id}` already.
fn canonical_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    let mut rest = url;
    while !rest.is_empty() {
        match rest.chars().next() {
            Some(':') => {
                let name: String = rest[1..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if name.is_empty() {
                    out.push(':');
                    rest = &rest[1..];
                    continue;
                }
                out.push_str(&format!("{{{name}}}"));
                rest = &rest[1 + name.len()..];
            }
            Some('<') => {
                let Some(end) = rest.find('>') else {
                    out.push('<');
                    rest = &rest[1..];
                    continue;
                };
                let inside = &rest[1..end];
                // `<int:pet_id>` names a converter and then the parameter.
                let name = inside.rsplit(':').next().unwrap_or(inside);
                out.push_str(&format!("{{{name}}}"));
                rest = &rest[end + 1..];
            }
            Some(c) => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
            None => break,
        }
    }
    out
}

/// The name a handler argument gives, where it names one.
fn handler_name(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches(')').trim();
    // `listPets`, `handlers.ListPets`, `crate::pets::list`: the last segment is
    // the function, and the path in front of it says where it lives.
    let last = text.rsplit(['.', ':']).next()?.trim();
    let plain = last
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then(|| last.to_string())?;
    (!plain.is_empty()).then_some(plain)
}

/// `app.get("/pets", listPets)` and `router.post("/pets/:id", update)`.
fn express(root: Node<'_>, source: &str, lines: &LineIndex) -> Option<(Framework, Vec<Endpoint>)> {
    let mut found = Vec::new();
    for node in walk(root) {
        if node.kind() != "call_expression" {
            continue;
        }
        let Some(callee) = node.child_by_field_name("function") else {
            continue;
        };
        if callee.kind() != "member_expression" {
            continue;
        }
        let Some(property) = callee.child_by_field_name("property") else {
            continue;
        };
        let method = source[property.byte_range()].to_lowercase();
        if !METHODS.contains(&method.as_str()) {
            continue;
        }
        // The receiver is the app or a router, and calling `.get` on anything
        // else is a different question: `map.get(k)` is not a route.
        let Some(object) = callee.child_by_field_name("object") else {
            continue;
        };
        let receiver = source[object.byte_range()].to_lowercase();
        if !receiver.contains("app") && !receiver.contains("router") {
            continue;
        }
        let Some(arguments) = node.child_by_field_name("arguments") else {
            continue;
        };
        let mut cursor = arguments.walk();
        let given: Vec<Node> = arguments
            .children(&mut cursor)
            .filter(|c| c.is_named())
            .collect();
        let Some(url) = given.first().and_then(|n| quoted(&source[n.byte_range()])) else {
            continue;
        };
        found.push(Endpoint {
            method: method.to_uppercase(),
            url: canonical_url(&url),
            handler: given.get(1).and_then(|n| handler_name(&source[n.byte_range()])),
            line: lines.line(node.start_byte()),
        });
    }
    Some((Framework::Express, found))
}

/// `@app.route("/pets", methods=["GET"])` and `@app.get("/pets")`.
fn flask(root: Node<'_>, source: &str, lines: &LineIndex) -> Option<(Framework, Vec<Endpoint>)> {
    let mut found = Vec::new();
    for node in walk(root) {
        if node.kind() != "decorated_definition" {
            continue;
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let handler = children
            .iter()
            .find(|c| c.kind() == "function_definition")
            .and_then(|f| f.child_by_field_name("name"))
            .map(|n| source[n.byte_range()].to_string());
        for decorator in children.iter().filter(|c| c.kind() == "decorator") {
            let text = source[decorator.byte_range()].trim();
            let Some(rest) = text.strip_prefix('@') else {
                continue;
            };
            let Some((receiver, call)) = rest.split_once('.') else {
                continue;
            };
            if !receiver.to_lowercase().contains("app") && !receiver.to_lowercase().contains("bp") {
                continue;
            }
            let Some((word, arguments)) = call.split_once('(') else {
                continue;
            };
            let Some(url) = quoted(arguments) else {
                continue;
            };
            let url = canonical_url(&url);
            let word = word.trim().to_lowercase();
            // `@app.get(…)` names one method. `@app.route(…)` names them in a
            // `methods=` argument, and names `GET` when it names none.
            let methods: Vec<String> = match word.as_str() {
                "route" => match arguments.find("methods") {
                    Some(at) => METHODS
                        .iter()
                        .filter(|m| arguments[at..].to_lowercase().contains(*m))
                        .map(|m| m.to_uppercase())
                        .collect(),
                    None => vec!["GET".to_string()],
                },
                other if METHODS.contains(&other) => vec![other.to_uppercase()],
                _ => continue,
            };
            for method in methods {
                found.push(Endpoint {
                    method,
                    url: url.clone(),
                    handler: handler.clone(),
                    line: lines.line(decorator.start_byte()),
                });
            }
        }
    }
    Some((Framework::Flask, found))
}

/// `Router::new().route("/pets", get(list_pets).post(create_pet))`.
fn axum(root: Node<'_>, source: &str, lines: &LineIndex) -> Option<(Framework, Vec<Endpoint>)> {
    let mut found = Vec::new();
    for node in walk(root) {
        if node.kind() != "call_expression" {
            continue;
        }
        let Some(callee) = node.child_by_field_name("function") else {
            continue;
        };
        if callee.kind() != "field_expression" {
            continue;
        }
        let Some(field) = callee.child_by_field_name("field") else {
            continue;
        };
        if source[field.byte_range()].trim() != "route" {
            continue;
        }
        let Some(arguments) = node.child_by_field_name("arguments") else {
            continue;
        };
        let mut cursor = arguments.walk();
        let given: Vec<Node> = arguments
            .children(&mut cursor)
            .filter(|c| c.is_named())
            .collect();
        let Some(url) = given.first().and_then(|n| quoted(&source[n.byte_range()])) else {
            continue;
        };
        let url = canonical_url(&url);
        // The second argument is a chain of method routers: `get(f).post(g)`.
        // Each names one method and the handler that answers it.
        let Some(chain) = given.get(1) else { continue };
        for call in walk(*chain) {
            if call.kind() != "call_expression" {
                continue;
            }
            let Some(inner) = call.child_by_field_name("function") else {
                continue;
            };
            let named = match inner.kind() {
                "identifier" => source[inner.byte_range()].to_string(),
                "field_expression" => match inner.child_by_field_name("field") {
                    Some(f) => source[f.byte_range()].to_string(),
                    None => continue,
                },
                _ => continue,
            };
            let method = named.trim().to_lowercase();
            if !METHODS.contains(&method.as_str()) {
                continue;
            }
            let handler = call
                .child_by_field_name("arguments")
                .and_then(|a| handler_name(source[a.byte_range()].trim_start_matches('(')));
            found.push(Endpoint {
                method: method.to_uppercase(),
                url: url.clone(),
                handler,
                line: lines.line(call.start_byte()),
            });
        }
    }
    found.sort_by_key(|e| e.line);
    Some((Framework::Axum, found))
}

/// `r.GET("/pets", listPets)` and `group.POST("/pets/:id", update)`.
fn gin(root: Node<'_>, source: &str, lines: &LineIndex) -> Option<(Framework, Vec<Endpoint>)> {
    let mut found = Vec::new();
    for node in walk(root) {
        if node.kind() != "call_expression" {
            continue;
        }
        let Some(callee) = node.child_by_field_name("function") else {
            continue;
        };
        if callee.kind() != "selector_expression" {
            continue;
        }
        let Some(field) = callee.child_by_field_name("field") else {
            continue;
        };
        // gin spells the method in capitals, which is what tells a route call
        // from `strings.Split` and every other method on a receiver.
        let named = source[field.byte_range()].to_string();
        if named != named.to_uppercase() {
            continue;
        }
        let method = named.to_lowercase();
        if !METHODS.contains(&method.as_str()) {
            continue;
        }
        let Some(arguments) = node.child_by_field_name("arguments") else {
            continue;
        };
        let mut cursor = arguments.walk();
        let given: Vec<Node> = arguments
            .children(&mut cursor)
            .filter(|c| c.is_named())
            .collect();
        let Some(url) = given.first().and_then(|n| quoted(&source[n.byte_range()])) else {
            continue;
        };
        found.push(Endpoint {
            method: named,
            url: canonical_url(&url),
            handler: given.get(1).and_then(|n| handler_name(&source[n.byte_range()])),
            line: lines.line(node.start_byte()),
        });
    }
    Some((Framework::Gin, found))
}

/// `@GetMapping("/pets")` and `@RequestMapping(value = "/pets", method = GET)`.
fn spring(root: Node<'_>, source: &str, lines: &LineIndex) -> Option<(Framework, Vec<Endpoint>)> {
    let mut found = Vec::new();
    // A class-level `@RequestMapping` prefixes every method under it.
    let mut prefix = String::new();
    for node in walk(root) {
        if matches!(node.kind(), "class_declaration") {
            if let Some(at) = class_prefix(node, source) {
                prefix = at;
            }
        }
        if !matches!(node.kind(), "method_declaration") {
            continue;
        }
        let handler = node
            .child_by_field_name("name")
            .map(|n| source[n.byte_range()].to_string());
        for annotation in annotations_of(node, source) {
            let Some((methods, url)) = mapping(&annotation) else {
                continue;
            };
            let whole = format!("{prefix}{url}");
            for method in methods {
                found.push(Endpoint {
                    method,
                    url: canonical_url(&whole),
                    handler: handler.clone(),
                    line: lines.line(node.start_byte()),
                });
            }
        }
    }
    Some((Framework::Spring, found))
}

/// The path a class-level `@RequestMapping` puts in front of its methods.
fn class_prefix(node: Node<'_>, source: &str) -> Option<String> {
    for annotation in annotations_of(node, source) {
        if !annotation.contains("RequestMapping") {
            continue;
        }
        if let Some((_, url)) = mapping(&annotation) {
            return Some(url);
        }
    }
    None
}

/// Every annotation written above this declaration, as text.
fn annotations_of(node: Node<'_>, source: &str) -> Vec<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() == "modifiers")
        .flat_map(|m| {
            let mut inner = m.walk();
            m.children(&mut inner)
                .filter(|c| matches!(c.kind(), "annotation" | "marker_annotation"))
                .map(|c| source[c.byte_range()].to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The methods and path a Spring mapping annotation names.
fn mapping(text: &str) -> Option<(Vec<String>, String)> {
    let text = text.trim().strip_prefix('@')?;
    let (word, rest) = match text.split_once('(') {
        Some((word, rest)) => (word.trim(), rest),
        None => (text.trim(), ""),
    };
    let methods = match word {
        "GetMapping" => vec!["GET".to_string()],
        "PostMapping" => vec!["POST".to_string()],
        "PutMapping" => vec!["PUT".to_string()],
        "PatchMapping" => vec!["PATCH".to_string()],
        "DeleteMapping" => vec!["DELETE".to_string()],
        // `@RequestMapping` names its methods in an argument, and names `GET`
        // when it names none.
        "RequestMapping" => {
            let named: Vec<String> = METHODS
                .iter()
                .filter(|m| rest.contains(&m.to_uppercase()))
                .map(|m| m.to_uppercase())
                .collect();
            match named.is_empty() {
                true => vec!["GET".to_string()],
                false => named,
            }
        }
        _ => return None,
    };
    // The path is the first string in the argument list, whether it is written
    // bare or as `value = "…"` or `path = "…"`. A mapping with no path at all
    // answers the class's own.
    let url = quoted_anywhere(rest).unwrap_or_default();
    Some((methods, url))
}

/// The first string literal anywhere in this text.
fn quoted_anywhere(text: &str) -> Option<String> {
    let at = text.find('"')?;
    quoted(&text[at..])
}
