use super::{author::Plan, Project};
use crate::lang::Language;
use crate::model::{ReferenceKind, Symbol, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::json;
use tree_sitter::Node;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Rename { name: String },
    RemoveParameter { index: usize },
    MoveParameter { from: usize, to: usize },
}

fn ancestor<'a>(parsed: &'a Parsed, span: Span, kind: &str) -> Result<Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end);
    while let Some(current) = node {
        if current.kind() == kind {
            return Ok(current);
        }
        node = current.parent();
    }
    anyhow::bail!("structural refactor cannot locate the required {kind} syntax.")
}

fn literal(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "integer_literal" | "boolean_literal" | "char_literal"
    ) || node.kind() == "unary_expression"
        && node.child(0).is_some_and(|n| n.kind() == "-")
        && node.named_child_count() == 1
        && node
            .named_child(0)
            .is_some_and(|n| n.kind() == "integer_literal")
}

fn validate_scalar_calls(project: &Project<'_>, symbol: &Symbol) -> Result<()> {
    let source = &project.sources[&symbol.file];
    let parsed = Parsers::new().parse(Language::Rust, source)?;
    let function = ancestor(&parsed, symbol.name_span, "function_item")?;
    let parameters = function
        .child_by_field_name("parameters")
        .context("missing parameters.")?;
    let mut cursor = parameters.walk();
    for parameter in parameters.named_children(&mut cursor) {
        let ty = parameter
            .child_by_field_name("type")
            .context("signature refactor requires scalar parameters.")?;
        ensure!(
            ty.kind() == "primitive_type"
                && matches!(
                    &source[ty.byte_range()],
                    "bool"
                        | "char"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                ),
            "signature refactor requires primitive scalar parameters."
        );
    }
    for reference in project.index.references_to(symbol.id) {
        if reference.kind != ReferenceKind::Call {
            continue;
        }
        let source = &project.sources[&reference.file];
        let parsed = Parsers::new().parse(Language::Rust, source)?;
        let call = ancestor(&parsed, reference.span, "call_expression")?;
        let function = call
            .child_by_field_name("function")
            .context("missing call function.")?;
        ensure!(
            function.start_byte() <= reference.span.start
                && function.end_byte() >= reference.span.end,
            "signature refactor requires a direct call."
        );
        let arguments = call
            .child_by_field_name("arguments")
            .context("missing call arguments.")?;
        let mut cursor = arguments.walk();
        ensure!(arguments.named_children(&mut cursor).all(literal),
            "signature refactor requires literal scalar call arguments; effects and evaluation order remain obligations.");
    }
    Ok(())
}

pub(super) fn plan(project: &Project<'_>, handle: &str, input: &str) -> Result<Plan> {
    let request: Request =
        serde_json::from_str(input).context("invalid structural refactor request.")?;
    let node = project.target(handle)?;
    let symbol = project.nodes[node]
        .symbol
        .and_then(|id| project.index.symbol(id))
        .context("structural refactor requires a declaration.")?;
    ensure!(
        symbol.language == Language::Rust && symbol.kind == SymbolKind::Function,
        "structural refactor requires a Rust free function."
    );
    let (edits, detail) = match request {
        Request::Rename { name } => {
            ensure!(name.len() <= 256, "rename name exceeds 256 bytes.");
            let plan = crate::refactor::rename::plan(project.index, symbol.id, &name)?;
            ensure!(plan.warnings.is_empty(), "structural rename has unresolved, dispatch or textual warnings; resolve them first.");
            (
                plan.edits,
                json!({"operation": "rename", "old_name": plan.old_name,
                "new_name": plan.new_name, "reference_edits": plan.reference_edits}),
            )
        }
        request => {
            let (change, detail) = match request {
                Request::RemoveParameter { index } => {
                    ensure!(index < 64, "parameter index must be below 64.");
                    (
                        crate::refactor::signature::Change::Remove(index),
                        json!({"operation": "remove-parameter", "index": index}),
                    )
                }
                Request::MoveParameter { from, to } => {
                    ensure!(
                        from < 64 && to < 64 && from != to,
                        "parameter positions must be distinct and below 64."
                    );
                    (
                        crate::refactor::signature::Change::Move { from, to },
                        json!({"operation": "move-parameter", "from": from, "to": to}),
                    )
                }
                Request::Rename { .. } => unreachable!(),
            };
            validate_scalar_calls(project, symbol)?;
            let plan = crate::refactor::signature::change(project.index, symbol.id, change)?;
            ensure!(
                plan.notes.is_empty(),
                "structural signature has unaccounted sites or notes."
            );
            (
                plan.edits,
                json!({"request": detail, "call_sites": plan.call_sites}),
            )
        }
    };
    ensure!(!edits.is_empty(), "structural refactor produced no edits.");
    for path in edits.paths() {
        let source = project
            .sources
            .get(path)
            .context("structural refactor leaves the captured source scope.")?;
        ensure!(
            crate::vfs::read_to_string(path)? == *source,
            "structural refactor source changed during planning."
        );
    }
    Ok(Plan {
        edits,
        report: json!({"operation": "refactor", "handle": handle,
        "detail": detail, "changed": true, "behavior_checked": false,
        "preservation": "bytes outside planned edits; source behavior requires independent checks.",
        "scope": "Rust free functions and indexed references; external consumers and dynamic dispatch excluded."}),
    })
}
