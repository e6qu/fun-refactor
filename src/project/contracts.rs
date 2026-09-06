use super::{bounded_text, Project};
use crate::lang::Language;
use crate::model::Symbol;
use crate::parse::Parsed;
use crate::transpile::routes::Endpoint;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tree_sitter::Node;

fn declaration<'a>(parsed: &'a Parsed, symbol: &Symbol) -> Option<Node<'a>> {
    let mut node = parsed.node_at(symbol.name_span.start)?;
    loop {
        if matches!(
            node.kind(),
            "function_item"
                | "function_definition"
                | "function_declaration"
                | "method_declaration"
                | "method_definition"
        ) && node.child_by_field_name("name").is_some_and(|name| {
            name.start_byte() == symbol.name_span.start && name.end_byte() == symbol.name_span.end
        }) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn type_spelling(ty: Node<'_>, owner: Node<'_>, source: &str) -> String {
    let spelling = text(ty, source).trim();
    let mut spelling = if ty.kind() == "type_annotation" {
        spelling.strip_prefix(':').unwrap_or(spelling).trim()
    } else {
        spelling
    }
    .to_owned();
    if let Some(dimensions) = owner.child_by_field_name("dimensions") {
        spelling.push_str(text(dimensions, source));
    }
    spelling
}

pub(super) fn simple_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn axum_input<'a>(ty: Node<'a>, source: &str) -> Option<(&'static str, Node<'a>)> {
    if ty.kind() != "generic_type" {
        return None;
    }
    let name = text(ty.child_by_field_name("type")?, source);
    let location = match name.rsplit("::").next()? {
        "Path" => "path",
        "Query" => "query",
        "Json" | "Form" => "body",
        _ => return None,
    };
    let args = ty.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    let mut types = args.named_children(&mut cursor).filter(|n| !n.is_extra());
    let inner = types.next()?;
    types.next().is_none().then_some((location, inner))
}

fn spring_input(parameter: Node<'_>, source: &str) -> Option<(&'static str, Option<String>)> {
    let mut cursor = parameter.walk();
    let modifiers = parameter
        .named_children(&mut cursor)
        .find(|n| n.kind() == "modifiers")?;
    let mut cursor = modifiers.walk();
    let mut found = Vec::new();
    for annotation in modifiers.named_children(&mut cursor) {
        if !matches!(annotation.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        let Some(name) = annotation.child_by_field_name("name") else {
            continue;
        };
        let location = match text(name, source).rsplit('.').next()? {
            "PathVariable" => "path",
            "RequestParam" => "query",
            "RequestBody" => "body",
            "RequestHeader" => "header",
            "CookieValue" => "cookie",
            _ => continue,
        };
        let binding = parameter
            .child_by_field_name("name")
            .map(|n| text(n, source));
        let mut wire_name = binding.map(str::to_owned);
        if let Some(arguments) = annotation.child_by_field_name("arguments") {
            let mut supplied_names = Vec::new();
            let mut cursor = arguments.walk();
            for argument in arguments
                .named_children(&mut cursor)
                .filter(|n| !n.is_extra())
            {
                let value = if argument.kind() == "element_value_pair" {
                    match argument.child_by_field_name("key").map(|n| text(n, source)) {
                        Some("name" | "value") => argument.child_by_field_name("value"),
                        _ => continue,
                    }
                } else {
                    Some(argument)
                };
                supplied_names.push(value.and_then(|n| {
                    let raw = text(n, source);
                    (n.kind() == "string_literal" && !raw.contains('\\'))
                        .then(|| raw.strip_prefix('"')?.strip_suffix('"').map(str::to_owned))
                        .flatten()
                }));
            }
            wire_name = match supplied_names.len() {
                0 => wire_name,
                1 => supplied_names.remove(0).filter(|s| !s.is_empty()),
                _ => None,
            };
        }
        found.push((location, if location == "body" { None } else { wire_name }));
    }
    (found.len() == 1).then(|| found.remove(0))
}

impl Project<'_> {
    pub(super) fn contract_rows(
        &self,
        route: &str,
        endpoint: &Endpoint,
        candidates: &[&Symbol],
        parsed: &Parsed,
        source: &str,
    ) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        let mut names = BTreeSet::new();
        let mut unsupported_segments = 0usize;
        for segment in endpoint.url.split('/') {
            let name = segment
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .filter(|name| simple_name(name));
            if let Some(name) = name {
                if names.insert(name) {
                    rows.push(json!({"kind": "route-contract-field", "route": route,
                        "handler": null, "direction": "request", "location": "path",
                        "name": bounded_text(name, 160), "binding": null, "declared_type": null,
                        "payload_type": null, "required": null, "line": endpoint.line,
                        "basis": "literal-path-segment", "status": "candidate", "confidence": null}));
                }
            } else if segment.contains(['{', '}']) {
                unsupported_segments += 1;
            }
        }
        if unsupported_segments > 0 {
            rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": null,
                "basis": "literal-path-segment", "segments": unsupported_segments,
                "reason": "Path markers exceed the simple whole-segment parameter subset; their names and constraints remain unknown."}));
        }
        if candidates.is_empty() {
            rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": null,
                "basis": "handler-signature", "reason": "No named same-file handler candidate; request and response types remain unknown."}));
        }
        for candidate in candidates {
            let handler = self.endpoint(candidate.id)?;
            let Some(function) = declaration(parsed, candidate) else {
                rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                    "basis": "handler-signature", "reason": "No supported declaration node for this handler candidate."}));
                continue;
            };
            let mut unknown_inputs = 0usize;
            if let Some(parameters) = function.child_by_field_name("parameters") {
                let mut cursor = parameters.walk();
                for parameter in parameters
                    .named_children(&mut cursor)
                    .filter(|n| !n.is_extra())
                {
                    let ty = parameter.child_by_field_name("type");
                    let input = match parsed.language {
                        Language::Rust => {
                            ty.and_then(|ty| axum_input(ty, source))
                                .map(|(location, inner)| {
                                    (location, None, Some(inner), "axum-extractor-type")
                                })
                        }
                        Language::Java => {
                            spring_input(parameter, source).map(|(location, name)| {
                                (location, name, None, "spring-parameter-annotation")
                            })
                        }
                        _ => None,
                    };
                    let (Some(ty), Some((location, name, inner, basis))) = (ty, input) else {
                        unknown_inputs += 1;
                        continue;
                    };
                    let binding = parameter
                        .child_by_field_name("name")
                        .or_else(|| parameter.child_by_field_name("pattern"));
                    rows.push(json!({"kind": "route-contract-field", "route": route,
                        "handler": handler, "direction": "request", "location": location,
                        "name": name.as_deref().map(|n| bounded_text(n, 160)),
                        "binding": binding.map(|n| bounded_text(text(n, source), 160)),
                        "declared_type": bounded_text(&type_spelling(ty, parameter, source), 512),
                        "payload_type": inner.map(|n| bounded_text(text(n, source), 512)),
                        "required": null, "line": parameter.start_position().row + 1,
                        "basis": basis, "status": "candidate", "confidence": "name-only"}));
                }
            }
            if unknown_inputs > 0 {
                rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                    "basis": "handler-signature", "parameters": unknown_inputs,
                    "reason": "Parameters have no supported request binding; context objects, aliases and custom extractors need further inspection."}));
            }
            let returned = match parsed.language {
                Language::Java => function.child_by_field_name("type"),
                Language::Go => function.child_by_field_name("result"),
                _ => function.child_by_field_name("return_type"),
            };
            if let Some(ty) = returned {
                let spelling = type_spelling(ty, function, source);
                rows.push(json!({"kind": "route-contract-field", "route": route,
                    "handler": handler, "direction": "response", "location": "return",
                    "name": null, "binding": null, "declared_type": bounded_text(&spelling, 512),
                    "payload_type": null, "required": null, "line": ty.start_position().row + 1,
                    "basis": "declared-return-type", "status": "candidate", "confidence": null}));
            } else {
                rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                    "basis": "handler-signature", "reason": "No explicit return type; response shape remains unknown."}));
            }
        }
        let request_fields = rows.iter().filter(|r| r["direction"] == "request").count();
        let response_fields = rows.iter().filter(|r| r["direction"] == "response").count();
        let gaps = rows
            .iter()
            .filter(|r| r["kind"] == "route-contract-gap")
            .count();
        rows.push(json!({"kind": "route-contract", "route": route, "status": "candidate",
            "completeness": "partial", "request_fields": request_fields, "response_fields": response_fields,
            "handler_candidates": candidates.len(), "gap_count": gaps,
            "basis": "route-and-handler-signature", "confidence": null}));
        Ok(rows)
    }
}
