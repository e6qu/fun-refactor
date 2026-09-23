use super::{hash, occurrence::SourceOrigins, page, Project};
use crate::{lang::Language, span::Span};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tree_sitter::Node;

pub(super) const RULE: &str = "python-body-origins-1";

#[derive(clap::Args, Default)]
pub(super) struct OriginOptions {
    #[arg(long, requires = "body", conflicts_with = "locators_only")]
    pub origins: bool,
    #[arg(long, default_value_t = 40, requires = "origins")]
    pub origin_limit: usize,
    #[arg(long, requires = "origins")]
    pub origin_cursor: Option<String>,
    #[arg(long, requires = "origins", conflicts_with = "origin_cursor")]
    pub origin_pointer: Option<String>,
}

type Mappings = BTreeMap<String, (Vec<Span>, &'static str)>;

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk()).collect()
}

fn bind(out: &mut Mappings, path: &str, node: Node<'_>) {
    out.insert(path.into(), (vec![node.into()], "syntax-node"));
}

fn expression(out: &mut Mappings, path: &str, node: Node<'_>, value: &Value) {
    let kind = value["kind"].as_str().unwrap_or_default();
    if node.kind() == "parenthesized_expression" {
        if let Some(inner) = node.named_child(0) {
            expression(out, path, inner, value);
        }
        return;
    }
    let admitted = match node.kind() {
        "identifier" => kind == "name",
        "integer" => kind == "int",
        "float" => kind == "float",
        "string" => kind == "str",
        "true" | "false" => kind == "bool",
        "none" => kind == "null",
        "call" => kind == "call",
        "binary_operator" | "boolean_operator" | "comparison_operator" => kind == "binary",
        "unary_operator" | "not_operator" => kind == "unary",
        "conditional_expression" => kind == "ternary",
        "attribute" => kind == "field",
        "subscript" => kind == "index",
        "keyword_argument" => kind == "keyword",
        "tuple" | "expression_list" => kind == "tuple",
        "list" => kind == "list-lit",
        _ => false,
    };
    if !admitted {
        return;
    }
    bind(out, path, node);
    let mut field = |syntax: &str, ir: &str| {
        if let Some(child) = node.child_by_field_name(syntax) {
            expression(
                out,
                &format!("{path}/value/{ir}"),
                child,
                &value["value"][ir],
            );
        }
    };
    match kind {
        "call" => {
            field("function", "callee");
            if let (Some(arguments), Some(values)) = (
                node.child_by_field_name("arguments"),
                value["value"]["args"].as_array(),
            ) {
                let arguments = children(arguments);
                if arguments.len() == values.len() {
                    for (i, (node, value)) in arguments.into_iter().zip(values).enumerate() {
                        expression(out, &format!("{path}/value/args/{i}"), node, value);
                    }
                }
            }
        }
        "binary" => {
            let parts = children(node);
            if parts.len() == 2 {
                expression(
                    out,
                    &format!("{path}/value/left"),
                    parts[0],
                    &value["value"]["left"],
                );
                expression(
                    out,
                    &format!("{path}/value/right"),
                    parts[1],
                    &value["value"]["right"],
                );
            }
        }
        "unary" => field("argument", "operand"),
        "field" => field("object", "of"),
        "index" => {
            field("value", "of");
            field("subscript", "index");
        }
        "keyword" => field("value", "value"),
        "ternary" => {
            let parts = children(node);
            if parts.len() == 3 {
                for (node, name) in parts.into_iter().zip(["then", "condition", "otherwise"]) {
                    expression(
                        out,
                        &format!("{path}/value/{name}"),
                        node,
                        &value["value"][name],
                    );
                }
            }
        }
        "tuple" | "list-lit" => {
            let parts = children(node);
            if let Some(values) = value["value"]
                .as_array()
                .filter(|values| values.len() == parts.len())
            {
                for (i, (node, value)) in parts.into_iter().zip(values).enumerate() {
                    expression(out, &format!("{path}/value/{i}"), node, value);
                }
            }
        }
        _ => (),
    }
}

fn block(out: &mut Mappings, path: &str, node: Node<'_>, values: &Value) {
    let nodes: Vec<_> = children(node)
        .into_iter()
        .enumerate()
        .filter_map(|(i, node)| {
            let inner = node.named_child(0).map(|n| n.kind());
            (!(node.kind() == "expression_statement"
                && (inner == Some("ellipsis") || i == 0 && inner == Some("string"))))
            .then_some(node)
        })
        .collect();
    let Some(values) = values
        .as_array()
        .filter(|values| values.len() == nodes.len())
    else {
        return;
    };
    for (i, (node, value)) in nodes.into_iter().zip(values).enumerate() {
        statement(out, &format!("{path}/{i}"), node, value);
    }
}

fn statement(out: &mut Mappings, path: &str, node: Node<'_>, value: &Value) {
    bind(out, path, node);
    let kind = value["kind"].as_str().unwrap_or_default();
    let inner = node.named_child(0);
    match (node.kind(), kind) {
        ("return_statement", "return") | ("raise_statement", "throw") => {
            if let Some(inner) = inner {
                expression(out, &format!("{path}/value"), inner, &value["value"]);
            }
        }
        ("expression_statement", "let" | "assign") => {
            let Some(inner) = inner else {
                return;
            };
            let (Some(left), Some(right)) = (
                inner.child_by_field_name("left"),
                inner.child_by_field_name("right"),
            ) else {
                return;
            };
            if inner.kind() == "augmented_assignment"
                && kind == "assign"
                && value["value"]["value"]["kind"] == "binary"
            {
                expression(
                    out,
                    &format!("{path}/value/target"),
                    left,
                    &value["value"]["target"],
                );
                let combined = format!("{path}/value/value");
                out.insert(
                    combined.clone(),
                    (
                        vec![left.into(), right.into()],
                        "augmented-assignment-combination",
                    ),
                );
                expression(
                    out,
                    &format!("{combined}/value/left"),
                    left,
                    &value["value"]["value"]["value"]["left"],
                );
                expression(
                    out,
                    &format!("{combined}/value/right"),
                    right,
                    &value["value"]["value"]["value"]["right"],
                );
            } else if inner.kind() == "assignment" {
                if kind == "assign" {
                    expression(
                        out,
                        &format!("{path}/value/target"),
                        left,
                        &value["value"]["target"],
                    );
                }
                expression(
                    out,
                    &format!("{path}/value/value"),
                    right,
                    &value["value"]["value"],
                );
            }
        }
        ("expression_statement", "expr") => {
            if let Some(inner) = inner {
                expression(out, &format!("{path}/value"), inner, &value["value"]);
            }
        }
        ("if_statement" | "elif_clause", "if") => {
            if let Some(condition) = node.child_by_field_name("condition") {
                expression(
                    out,
                    &format!("{path}/value/condition"),
                    condition,
                    &value["value"]["condition"],
                );
            }
            if let Some(body) = node.child_by_field_name("consequence") {
                block(
                    out,
                    &format!("{path}/value/then"),
                    body,
                    &value["value"]["then"],
                );
            }
            let mut index = 0;
            for clause in children(node) {
                match clause.kind() {
                    "elif_clause" => {
                        if let Some(value) = value["value"]["otherwise"].get(index) {
                            statement(
                                out,
                                &format!("{path}/value/otherwise/{index}"),
                                clause,
                                value,
                            );
                        }
                        index += 1;
                    }
                    "else_clause" => {
                        if let (Some(body), Some(values)) = (
                            clause.child_by_field_name("body"),
                            value["value"]["otherwise"].as_array(),
                        ) {
                            let mut temporary = Mappings::new();
                            block(
                                &mut temporary,
                                "",
                                body,
                                &json!(&values[index.min(values.len())..]),
                            );
                            for (suffix, mapping) in temporary {
                                let (first, rest) =
                                    suffix[1..].split_once('/').unwrap_or((&suffix[1..], ""));
                                let offset = first.parse::<usize>().expect("block index") + index;
                                out.insert(
                                    format!(
                                        "{path}/value/otherwise/{offset}{}",
                                        if rest.is_empty() {
                                            String::new()
                                        } else {
                                            format!("/{rest}")
                                        }
                                    ),
                                    mapping,
                                );
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
        ("while_statement", "while") => {
            if let Some(condition) = node.child_by_field_name("condition") {
                expression(
                    out,
                    &format!("{path}/value/condition"),
                    condition,
                    &value["value"]["condition"],
                );
            }
            if let Some(body) = node.child_by_field_name("body") {
                block(
                    out,
                    &format!("{path}/value/body"),
                    body,
                    &value["value"]["body"],
                );
            }
        }
        ("assert_statement", "assert") => {
            for (node, name) in children(node).into_iter().zip(["condition", "message"]) {
                expression(
                    out,
                    &format!("{path}/value/{name}"),
                    node,
                    &value["value"][name],
                );
            }
        }
        _ => (),
    }
}

fn tagged(value: &Value, path: &str, rows: &mut Vec<(String, String)>) {
    match value {
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                rows.push((path.into(), kind.into()));
            }
            for (key, value) in object {
                tagged(
                    value,
                    &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                    rows,
                );
            }
        }
        Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                tagged(value, &format!("{path}/{i}"), rows);
            }
        }
        _ => (),
    }
}

impl Project<'_> {
    pub(super) fn semantic_origins(
        &self,
        target: usize,
        root: Node<'_>,
        report: &Value,
        options: &OriginOptions,
    ) -> Result<Value> {
        ensure!(
            (1..=256).contains(&options.origin_limit),
            "origin limit must be between 1 and 256"
        );
        let node = &self.nodes[target];
        let file = self.root.join(&node.path);
        let symbol = node.symbol.and_then(|id| self.index.symbol(id));
        let mut mappings = Mappings::new();
        if let Some(symbol) = symbol.filter(|symbol| symbol.language == Language::Python) {
            let mut pending = vec![root];
            while let Some(syntax) = pending.pop() {
                if syntax.kind() == "function_definition"
                    && syntax
                        .child_by_field_name("name")
                        .is_some_and(|name| Span::from(name) == symbol.name_span)
                {
                    let function_path = "/model/items/0";
                    let original = crate::transpile::function_at(
                        Language::Python,
                        &self.sources[&file],
                        syntax,
                    )?;
                    let original = serde_json::to_value(original)?;
                    let selected = &report["model"]["items"][0]["value"];
                    let parameter_identity = |function: &Value| {
                        function["params"].as_array().map(|params| {
                            params
                                .iter()
                                .map(|p| (&p["name"], &p["kind"], &p["default"]))
                                .map(|p| json!(p))
                                .collect::<Vec<_>>()
                        })
                    };
                    if original["name"] != selected["name"]
                        || original["body"] != selected["body"]
                        || parameter_identity(&original) != parameter_identity(selected)
                    {
                        break;
                    }
                    if report
                        .pointer(&format!("{function_path}/kind"))
                        .and_then(Value::as_str)
                        == Some("function")
                    {
                        bind(&mut mappings, function_path, syntax);
                        if let Some(body) = syntax.child_by_field_name("body") {
                            block(
                                &mut mappings,
                                &format!("{function_path}/value/body"),
                                body,
                                &report["model"]["items"][0]["value"]["body"],
                            );
                        }
                    }
                    break;
                }
                pending.extend(children(syntax));
            }
        }
        let analyzer = hash((
            RULE,
            include_str!("semantic_origins.rs"),
            include_str!("../transpile/read.rs"),
            include_str!("../transpile/normalize.rs"),
        ))?;
        let input = hash((
            &self.revision,
            &file,
            &self.sources[&file],
            &analyzer,
            &report["semantic_basis"],
        ))?;
        let mut nodes = Vec::new();
        tagged(&report["model"], "/model", &mut nodes);
        let mut rows = Vec::new();
        let body_available = report["body_identity"]["status"] == "available";
        for (pointer, kind) in nodes {
            let (origins, rule) = match mappings.get(&pointer) {
                Some((spans, rule)) => {
                    let occurrences = spans
                        .iter()
                        .map(|span| self.occurrence(&file, *span, "semantic-origin"))
                        .collect::<Result<Vec<_>>>()?;
                    let origins = match occurrences.as_slice() {
                        [occurrence] => SourceOrigins::Exact {
                            occurrence: occurrence.clone(),
                        },
                        _ => SourceOrigins::Multiple { occurrences },
                    };
                    (origins, *rule)
                }
                None => (
                    SourceOrigins::Absent {
                        reason: "synthesized, transformed or outside the admitted body mapping."
                            .into(),
                    },
                    "unmapped-node",
                ),
            };
            rows.push(json!({"id":format!("frso1:{}", hash((&report["semantic_basis"], &pointer))?), "pointer":pointer,
                "body_pointer":pointer.strip_prefix("/model/items/0/value/body").filter(|_| body_available).map(|suffix|format!("/body{suffix}")),
                "kind":kind,"origins":origins,"rule":format!("{RULE}:{rule}")}));
        }
        if let Some(pointer) = &options.origin_pointer {
            ensure!(
                rows.iter().any(|row| row["pointer"] == *pointer),
                "origin pointer does not name a returned semantic node."
            );
            rows.retain(|row| row["pointer"] == *pointer);
        }
        let key = format!("frso-page:{}", hash((&input, &options.origin_pointer))?);
        let (start, end, paging) = page(
            rows.len(),
            options.origin_limit,
            options.origin_cursor.as_deref(),
            &key,
        )?;
        let complete = report["status"] == "returned" && start == 0 && end == rows.len();
        let arguments = [
            "project",
            "semantic",
            report["selection"]["handle"].as_str().unwrap(),
            "--body",
            "--origins",
            "--nodes",
            "4096",
        ];
        let continuation = paging["next"].as_str().map(|cursor| {
            let mut arguments: Vec<String> = arguments.iter().map(|s| s.to_string()).collect();
            arguments.extend([
                "--origin-limit".into(),
                options.origin_limit.to_string(),
                "--origin-cursor".into(),
                cursor.into(),
            ]);
            json!({"arguments":arguments})
        });
        Ok(
            json!({"schema":"fr-semantic-origins-1","revision":self.revision,"semantic_basis":report["semantic_basis"],
            "input_digest":input,"analyzer":analyzer,"rule_version":RULE,"items":&rows[start..end],"page":paging,
            "inputs":{"source_path":node.path,"source_digest":hash(&self.sources[&file])?,"semantic_basis":report["semantic_basis"],"analyzer":analyzer},
            "complete":complete,"scope":"selected Python declaration body; other nodes retain absent origins.",
            "claim":"syntax provenance, not behavioral equivalence or compiler evidence.","confidence":"structural",
            "follow":{"arguments":arguments},"continuation":continuation,"mutation_authority":false}),
        )
    }
}
