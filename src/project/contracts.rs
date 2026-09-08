use super::{bounded_text, fast_routes, hash, routes::RouteDeclaration, schemas, Project};
use crate::lang::Language;
use crate::model::Symbol;
use crate::parse::Parsed;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tree_sitter::Node;

fn declaration<'a>(parsed: &'a Parsed, symbol: &Symbol) -> Option<Node<'a>> {
    let mut node = parsed.node_at(symbol.name_span.start)?;
    loop {
        if node.kind() == "variable_declarator"
            && node.child_by_field_name("name").is_some_and(|name| {
                name.start_byte() == symbol.name_span.start
                    && name.end_byte() == symbol.name_span.end
            })
        {
            return node
                .child_by_field_name("value")
                .filter(|n| matches!(n.kind(), "arrow_function" | "function_expression"));
        }
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

fn fastapi_fields(
    route: &str,
    handler: &Value,
    function: Node<'_>,
    decorator: Node<'_>,
    source: &str,
) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut unknown = 0usize;
    for parameter in function
        .child_by_field_name("parameters")
        .into_iter()
        .flat_map(fast_routes::children)
        .filter(|n| !matches!(n.kind(), "keyword_separator" | "positional_separator"))
    {
        let Some(input) = fast_routes::input(parameter, source) else {
            unknown += 1;
            continue;
        };
        rows.push(json!({"kind": "route-contract-field", "route": route, "handler": handler,
            "direction": "request", "location": input.location, "name": input.name.as_deref().map(|s| bounded_text(s, 160)),
            "binding": bounded_text(&input.binding, 160), "declared_type": input.ty.as_deref().map(|s| bounded_text(s, 512)),
            "payload_type": null, "required": null, "binding_kind": input.marker,
            "line": parameter.start_position().row + 1, "basis": "fastapi-explicit-binding", "status": "candidate", "confidence": "name-only"}));
    }
    if unknown > 0 {
        rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
            "parameters": unknown, "basis": "fastapi-explicit-binding",
            "reason": "Parameters lack one supported explicit binding; implicit classification, aliases and dependencies remain unknown."}));
    }
    if let Some(call) = fast_routes::children(decorator)
        .into_iter()
        .find(|n| n.kind() == "call")
    {
        let models = fast_routes::keyword(call, "response_model", source);
        if let [model] = models.as_slice() {
            if fast_routes::simple_type(*model, source, 0) {
                rows.push(json!({"kind": "route-contract-field", "route": route, "handler": handler,
                    "direction": "response", "location": "response-model", "name": null, "binding": null,
                    "declared_type": bounded_text(fast_routes::text(*model, source), 512), "payload_type": null, "required": null,
                    "model_state": if model.kind() == "none" { "disabled" } else { "declared" },
                    "line": model.start_position().row + 1, "basis": "fastapi-response-model", "status": "candidate", "confidence": null}));
            } else {
                rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                    "basis": "fastapi-response-model", "reason": "The response_model expression exceeds the supported type subset."}));
            }
        } else if !models.is_empty() {
            rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                "basis": "fastapi-response-model", "reason": "Competing response_model arguments leave model selection unknown."}));
        }
        if call
            .child_by_field_name("arguments")
            .into_iter()
            .flat_map(fast_routes::children)
            .any(|n| n.kind() == "dictionary_splat")
        {
            rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                "basis": "fastapi-response-model", "reason": "Expanded decorator options may carry additional response metadata."}));
        }
    }
    rows
}

impl Project<'_> {
    fn contract_type_rows(
        &self,
        route: &str,
        symbol: &Symbol,
        ty: Node<'_>,
        source: &str,
        field: &mut Value,
    ) -> Result<Vec<Value>> {
        let id = format!(
            "frpctf1:{}",
            &hash((
                route,
                symbol.id,
                ty.start_byte(),
                &field["basis"],
                &field["location"]
            ))?[..32]
        );
        field["id"] = json!(id);
        field["type_reference_count"] = Value::Null;
        let Some(names) = schemas::declared_type_names(ty, source, symbol.language) else {
            return Ok(vec![
                json!({"kind": "route-contract-gap", "route": route, "field": id,
                "basis": "declared-type-references", "reason": "The type expression or language exceeds reference inspection; no partial type-name candidates accompany this field."}),
            ]);
        };
        field["type_reference_count"] = json!(names.len());
        let mut rows = Vec::new();
        for name in names {
            let candidates = self.type_candidates(&name, &symbol.file);
            let reference = format!("frpctr1:{}", &hash((&id, &name))?[..32]);
            rows.push(json!({"kind": "route-contract-type-reference", "id": reference, "route": route, "field": id,
                "name": bounded_text(&name, 160), "candidate_count": candidates.len(),
                "status": match candidates.len() { 0 => "unresolved", 1 => "candidate", _ => "ambiguous" },
                "basis": "same-file-type-name", "confidence": null}));
            for candidate in candidates {
                rows.push(json!({"kind": "route-contract-type-candidate", "route": route, "reference": reference,
                    "target": self.endpoint(candidate.id)?, "basis": "same-file-type-name", "status": "candidate", "confidence": "name-only"}));
            }
        }
        Ok(rows)
    }

    pub(super) fn contract_rows(
        &self,
        route: &str,
        declaration: &RouteDeclaration,
        candidates: &[&Symbol],
        parsed: &Parsed,
        source: &str,
        types: bool,
    ) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        let endpoint = &declaration.endpoint;
        let mut names = BTreeSet::new();
        let mut unsupported_segments = 0usize;
        if let Some(parameter) = &declaration.next_catch_all {
            rows.push(json!({"kind": "route-contract-field", "route": route,
                "handler": null, "direction": "request", "location": "path",
                "name": bounded_text(&parameter.name, 160), "binding": null, "declared_type": null,
                "payload_type": null, "required": null, "line": endpoint.line,
                "segment_kind": if parameter.optional { "optional-catch-all" } else { "catch-all" },
                "min_segments": if parameter.optional { 0 } else { 1 }, "max_segments": null,
                "basis": "nextjs-catch-all-path", "status": "candidate", "confidence": null}));
        }
        for segment in endpoint.url.split('/') {
            if declaration.next_catch_all.is_some() && segment.starts_with("{...") {
                continue;
            }
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
            let Some(function) = self::declaration(parsed, candidate) else {
                rows.push(json!({"kind": "route-contract-gap", "route": route, "handler": handler,
                    "basis": "handler-signature", "reason": "No supported declaration node for this handler candidate."}));
                continue;
            };
            let mut has_response_model = false;
            if let Some(decorator) = declaration
                .fast_decorator
                .and_then(|offset| parsed.node_at(offset))
            {
                let fields = fastapi_fields(route, &handler, function, decorator, source);
                has_response_model = fields.iter().any(|r| {
                    r["basis"] == "fastapi-response-model" && r["kind"] == "route-contract-field"
                });
                rows.extend(fields);
            }
            let mut unknown_inputs = 0usize;
            if function.child_by_field_name("parameter").is_some() {
                unknown_inputs += 1;
            }
            if let Some(parameters) = function
                .child_by_field_name("parameters")
                .filter(|_| declaration.fast_decorator.is_none())
            {
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
                    let mut field = json!({"kind": "route-contract-field", "route": route,
                        "handler": handler, "direction": "request", "location": location,
                        "name": name.as_deref().map(|n| bounded_text(n, 160)),
                        "binding": binding.map(|n| bounded_text(text(n, source), 160)),
                        "declared_type": bounded_text(&type_spelling(ty, parameter, source), 512),
                        "payload_type": inner.map(|n| bounded_text(text(n, source), 512)),
                        "required": null, "line": parameter.start_position().row + 1,
                        "basis": basis, "status": "candidate", "confidence": "name-only"});
                    if types && parsed.language == Language::Rust {
                        rows.extend(
                            self.contract_type_rows(route, candidate, ty, source, &mut field)?,
                        );
                    }
                    rows.push(field);
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
                let mut field = json!({"kind": "route-contract-field", "route": route,
                    "handler": handler, "direction": "response", "location": "return",
                    "name": null, "binding": null, "declared_type": bounded_text(&spelling, 512),
                    "payload_type": null, "required": null, "line": ty.start_position().row + 1,
                    "basis": "declared-return-type", "status": "candidate", "confidence": null});
                if types {
                    rows.extend(self.contract_type_rows(route, candidate, ty, source, &mut field)?);
                }
                rows.push(field);
            } else if !has_response_model {
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
