use super::{FormalFunction, FormalProperty};
use crate::formal_kernel::{Operator, Term, Value};
use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

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
                format!("(h_{name} : -9223372036854775808 ≤ {name} ∧ {name} ≤ 9223372036854775807)")
            } else {
                format!("(h_{name} : {name} ≤ 18446744073709551615)")
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
        Term::If { condition, then, otherwise } => required_fuel(condition)
            .max(required_fuel(then)).max(required_fuel(otherwise)),
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
