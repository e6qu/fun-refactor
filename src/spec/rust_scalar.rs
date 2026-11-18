use super::{FormalFunction, FormalProperty};
use crate::formal_kernel::{Operator, Term, Value};
use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use tree_sitter::Node;

pub(super) fn validate_source(root: &Path, function: &FormalFunction) -> Result<()> {
    let source = super::declaration_text(&root.join(&function.source), &function.symbol)?;
    let parsed = crate::parse::Parsers::new().parse(crate::lang::Language::Rust, &source)?;
    let numeric = function
        .inputs
        .iter()
        .chain(std::iter::once(&function.output))
        .find(|binding| binding.rust_type != "bool")
        .map(|binding| binding.rust_type.as_str());
    let mut pending = vec![parsed.root()];
    while let Some(node) = pending.pop() {
        let text = &source[node.byte_range()];
        match node.kind() {
            "source_file"
            | "function_item"
            | "visibility_modifier"
            | "parameters"
            | "parameter"
            | "identifier"
            | "block"
            | "let_declaration"
            | "expression_statement"
            | "return_expression"
            | "if_expression"
            | "else_clause"
            | "binary_expression"
            | "unary_expression"
            | "parenthesized_expression"
            | "boolean_literal"
            | "line_comment"
            | "block_comment" => (),
            "primitive_type" => ensure!(
                text == "bool" || Some(text) == numeric,
                "local types must use the signature's scalar arithmetic."
            ),
            "integer_literal" => ensure!(
                text.bytes().all(|byte| byte.is_ascii_digit()),
                "rust-scalar-model requires unsuffixed decimal literals."
            ),
            other => bail!("rust-scalar-model has no source rule for {other}."),
        }
        if !matches!(node.kind(), "line_comment" | "block_comment") {
            pending.extend(node.named_children(&mut node.walk()));
        }
    }
    let declaration = parsed
        .root()
        .named_children(&mut parsed.root().walk())
        .find(|node| node.kind() == "function_item")
        .context("missing Rust function")?;
    let body = declaration
        .child_by_field_name("body")
        .context("missing Rust body")?;
    let bindings = function
        .inputs
        .iter()
        .map(|binding| (binding.name.clone(), binding.rust_type.clone()))
        .collect();
    check_block(
        body,
        &source,
        &bindings,
        Some(&function.output.rust_type),
        &function.output.rust_type,
    )
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
        .collect()
}

fn operator<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.child_by_field_name("operator")
        .map(|op| &source[op.byte_range()])
        .unwrap_or_else(|| {
            source[node.byte_range()]
                .trim_start()
                .get(..1)
                .unwrap_or("")
        })
}

fn hint<'a>(
    node: Node<'_>,
    source: &str,
    bindings: &'a BTreeMap<String, String>,
) -> Option<&'a str> {
    match node.kind() {
        "identifier" => bindings.get(&source[node.byte_range()]).map(String::as_str),
        "boolean_literal" => Some("bool"),
        "binary_expression"
            if matches!(
                operator(node, source),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
            ) =>
        {
            Some("bool")
        }
        "unary_expression" if operator(node, source) == "!" => Some("bool"),
        "binary_expression" | "unary_expression" | "parenthesized_expression" => children(node)
            .into_iter()
            .find_map(|child| hint(child, source, bindings)),
        _ => None,
    }
}

fn check_block(
    node: Node<'_>,
    source: &str,
    outer: &BTreeMap<String, String>,
    expected: Option<&str>,
    output: &str,
) -> Result<()> {
    let mut bindings = outer.clone();
    for child in children(node) {
        match child.kind() {
            "let_declaration" => {
                let name = child
                    .child_by_field_name("pattern")
                    .context("missing local name")?;
                ensure!(
                    name.kind() == "identifier",
                    "scalar locals require simple names."
                );
                let value = child
                    .child_by_field_name("value")
                    .context("missing local value")?;
                let ty = child
                    .child_by_field_name("type")
                    .map(|ty| &source[ty.byte_range()])
                    .or_else(|| hint(value, source, &bindings))
                    .context("local arithmetic needs an explicit i64 or u64 type.")?
                    .to_owned();
                check_expression(value, source, &bindings, Some(&ty), output)?;
                bindings.insert(source[name.byte_range()].to_owned(), ty);
            }
            "expression_statement" => {
                for expression in children(child) {
                    check_expression(expression, source, &bindings, None, output)?;
                }
            }
            _ => check_expression(child, source, &bindings, expected, output)?,
        }
    }
    Ok(())
}

fn check_expression(
    node: Node<'_>,
    source: &str,
    bindings: &BTreeMap<String, String>,
    expected: Option<&str>,
    output: &str,
) -> Result<()> {
    let inferred = hint(node, source, bindings);
    if let (Some(expected), Some(inferred)) = (expected, inferred) {
        ensure!(
            expected == inferred,
            "scalar expression has incompatible source types."
        );
    }
    let ty = expected.or(inferred);
    let parts = children(node);
    match node.kind() {
        "identifier" => ensure!(inferred.is_some(), "unbound scalar name."),
        "boolean_literal" => (),
        "integer_literal" => ensure!(matches!(ty, Some("i64" | "u64")),
            "integer literals need a declared i64 or u64 context; Rust defaults stay outside this rule."),
        "parenthesized_expression" => {
            check_expression(*parts.first().context("empty expression")?, source, bindings, ty, output)?;
        }
        "unary_expression" => {
            let operand_type = if operator(node, source) == "!" { "bool" } else {
                ensure!(ty == Some("i64"), "unary minus requires i64 arithmetic.");
                "i64"
            };
            check_expression(*parts.first().context("missing operand")?, source, bindings, Some(operand_type), output)?;
        }
        "binary_expression" => {
            let left = node.child_by_field_name("left").context("missing left operand")?;
            let right = node.child_by_field_name("right").context("missing right operand")?;
            let op = operator(node, source);
            let operand_type = match op {
                "&&" | "||" => Some("bool"),
                "==" | "!=" | "<" | "<=" | ">" | ">=" => hint(left, source, bindings).or_else(|| hint(right, source, bindings)),
                "+" | "-" | "*" | "/" | "%" => ty,
                _ => bail!("unsupported scalar operator {op}."),
            }.context("arithmetic comparison needs a declared operand type.")?;
            check_expression(left, source, bindings, Some(operand_type), output)?;
            check_expression(right, source, bindings, Some(operand_type), output)?;
        }
        "return_expression" => {
            check_expression(*parts.first().context("missing return value")?, source, bindings, Some(output), output)?;
        }
        "if_expression" => {
            let condition = node.child_by_field_name("condition").context("missing condition")?;
            check_expression(condition, source, bindings, Some("bool"), output)?;
            for part in parts.into_iter().filter(|part| part.id() != condition.id()) {
                check_expression(part, source, bindings, expected, output)?;
            }
        }
        "block" => check_block(node, source, bindings, expected, output)?,
        "else_clause" => {
            for part in parts { check_expression(part, source, bindings, expected, output)?; }
        }
        other => bail!("missing scalar expression rule for {other}."),
    }
    Ok(())
}

pub(super) fn property(function: &FormalFunction) -> Result<FormalProperty> {
    ensure!(
        crate::lang::detect(&function.source) == Some(crate::lang::Language::Rust),
        "rust-scalar-model requires a Rust function."
    );
    let bindings = function
        .inputs
        .iter()
        .chain(std::iter::once(&function.output));
    let mut numeric = None;
    for binding in bindings {
        match binding.rust_type.as_str() {
            "bool" => (),
            ty @ ("i64" | "u64") => {
                ensure!(
                    numeric.is_none_or(|previous| previous == ty),
                    "rust-scalar-model requires one integer type throughout the signature."
                );
                numeric = Some(ty);
            }
            _ => bail!("rust-scalar-model admits only bool, i64 and u64."),
        }
    }
    ensure!(
        function.inputs.len() <= 8,
        "rust-scalar-model admits at most eight inputs."
    );
    let evaluation = function
        .evaluation
        .as_ref()
        .context("missing executable term")?;
    validate_term(&evaluation.term)?;
    let fuel = required_fuel(&evaluation.term);
    ensure!(fuel <= 64, "scalar expression exceeds the depth limit.");
    let signed = numeric != Some("u64");
    let parameters = function
        .inputs
        .iter()
        .map(|input| format!("({} : {})", input.name, input.lean_type))
        .collect::<Vec<_>>()
        .join(" ");
    let bounds = function
        .inputs
        .iter()
        .filter(|input| input.rust_type != "bool")
        .map(|input| {
            let name = &input.name;
            if signed {
                format!(
                    "(_h_{name} : -9223372036854775808 ≤ {name} ∧ {name} ≤ 9223372036854775807)"
                )
            } else {
                format!("(_h_{name} : {name} ≤ 18446744073709551615)")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let environment = function
        .inputs
        .iter()
        .map(|input| value(&input.name, &input.rust_type))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = function
        .inputs
        .iter()
        .map(|input| input.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let result = value(
        &format!("({} {arguments})", function.model),
        &function.output.rust_type,
    );
    Ok(FormalProperty {
        kind: "rust-scalar-model".into(),
        name: format!("{}_rust_scalar_model", function.model),
        proposition: format!("{parameters} {bounds} : FrRustScalar.eval {signed} {fuel} [{environment}] {}Term = some {result}", function.model),
        proof_status: "unproved".into(),
        agent_spec: None,
    })
}

fn required_fuel(term: &Term) -> usize {
    1 + match term {
        Term::Let { value, body } => required_fuel(value).max(required_fuel(body)),
        Term::If {
            condition,
            then,
            otherwise,
        } => required_fuel(condition)
            .max(required_fuel(then))
            .max(required_fuel(otherwise)),
        Term::Binary { left, right, .. } => required_fuel(left).max(required_fuel(right)),
        Term::Not { operand } | Term::Neg { operand } => required_fuel(operand),
        _ => 0,
    }
}

fn value(expression: &str, ty: &str) -> String {
    match ty {
        "bool" => format!("(FrPureKernel.Value.bool {expression})"),
        "u64" => format!("(FrPureKernel.Value.int ({expression} : Int))"),
        _ => format!("(FrPureKernel.Value.int {expression})"),
    }
}

fn validate_term(term: &Term) -> Result<()> {
    match term {
        Term::Value {
            value: Value::Bool(_) | Value::Int(_),
        }
        | Term::Bound { .. } => Ok(()),
        Term::Let { value, body } => {
            validate_term(value)?;
            validate_term(body)
        }
        Term::If {
            condition,
            then,
            otherwise,
        } => {
            validate_term(condition)?;
            validate_term(then)?;
            validate_term(otherwise)
        }
        Term::Binary {
            operator,
            left,
            right,
        } => {
            ensure!(
                !matches!(operator, Operator::Xor),
                "integer bit operations need a separate Rust model."
            );
            validate_term(left)?;
            validate_term(right)
        }
        Term::Not { operand } | Term::Neg { operand } => validate_term(operand),
        _ => bail!("rust-scalar-model excludes collections and partial access."),
    }
}
