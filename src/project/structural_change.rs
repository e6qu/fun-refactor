use super::{author::Plan, Project};
use crate::edit::Edit;
use crate::lang::Language;
use crate::model::{ReferenceKind, Symbol, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::refactor::WarningKind;
use crate::span::Span;
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::json;
use tree_sitter::Node;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Request {
    Rename {
        name: String,
        documentation: Documentation,
    },
    RemoveParameter {
        index: usize,
    },
    MoveParameter {
        from: usize,
        to: usize,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Documentation {
    Preserve,
    Update,
}

fn documentation_node(mut node: Node<'_>, language: Language, source: &str) -> bool {
    loop {
        if node.kind().contains("comment") || language == Language::Markdown {
            return true;
        }
        if language == Language::Python && node.kind() == "string" {
            let text = &source[node.byte_range()];
            let literal = text.trim_start_matches(['r', 'R', 'u', 'U']);
            if (literal.starts_with('"') || literal.starts_with('\''))
                && node.parent().is_some_and(|statement| {
                    statement.kind() == "expression_statement"
                        && statement.named_child_count() == 1
                        && statement.parent().is_some_and(|body| {
                            (body.kind() == "module"
                                || body.kind() == "block"
                                    && body.parent().is_some_and(|owner| {
                                        matches!(
                                            owner.kind(),
                                            "function_definition" | "class_definition"
                                        )
                                    }))
                                && body
                                    .named_children(&mut body.walk())
                                    .find(|child| child.kind() != "comment")
                                    .is_some_and(|first| first.id() == statement.id())
                        })
                })
            {
                return true;
            }
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

fn update_documentation(
    project: &Project<'_>,
    symbol: &Symbol,
    name: &str,
    plan: &mut crate::refactor::rename::RenamePlan,
) -> Result<usize> {
    let mentions = crate::mentions::of(project.index, &symbol.name)?;
    let mut updated = Vec::new();
    for mention in mentions {
        if plan.edits.iter().any(|(file, edits)| {
            file == &mention.file && edits.iter().any(|edit| edit.span.overlaps(mention.span))
        }) {
            continue;
        }
        let source = project
            .sources
            .get(&mention.file)
            .context("documentation is outside the captured source scope")?;
        let language = project
            .index
            .file(&mention.file)
            .context("documentation has no indexed language")?
            .language;
        let parsed = Parsers::new().parse(language, source)?;
        let node = parsed
            .root()
            .descendant_for_byte_range(mention.span.start, mention.span.end)
            .context("documentation mention has no syntax")?;
        if documentation_node(node, language, source) {
            plan.edits.add(
                mention.file.clone(),
                Edit::new(
                    mention.span,
                    name,
                    format!("rename explicit documentation reference to {}", symbol.name),
                ),
            );
            updated.push((mention.file, mention.line, mention.col));
        }
    }
    plan.warnings.retain(|warning| {
        warning.kind != WarningKind::TextualOccurrence
            || !updated.contains(&(warning.file.clone(), warning.line, warning.col))
    });
    Ok(updated.len())
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

fn literal_argument(text: &str) -> Result<bool> {
    let source = format!("const FR_ARGUMENT: () = {text};");
    let parsed = Parsers::new().parse(Language::Rust, &source)?;
    Ok(!parsed.has_errors()
        && parsed.root().named_child_count() == 1
        && parsed
            .root()
            .named_child(0)
            .and_then(|node| node.child_by_field_name("value"))
            .is_some_and(literal))
}

fn validate_scalar_calls(
    project: &Project<'_>,
    symbol: &Symbol,
    change: &crate::refactor::signature::Change,
) -> Result<()> {
    ensure!(project.index.definition_group(symbol.id).len() == 1,
        "signature changes require one declaration; conditional variants need a separate reviewed migration.");
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
        if let Some((_, arguments)) =
            crate::refactor::signature::macro_call_arguments(&parsed, reference.span)
        {
            let safe = match change {
                crate::refactor::signature::Change::Remove(index) => arguments
                    .get(*index)
                    .map(|span| literal_argument(span.text(source)))
                    .transpose()?
                    .unwrap_or(false),
                crate::refactor::signature::Change::Move { .. } => {
                    let mut safe = true;
                    for span in arguments {
                        safe &= literal_argument(span.text(source))?;
                    }
                    safe
                }
                crate::refactor::signature::Change::Add { .. } => false,
            };
            ensure!(safe, "signature refactor requires literal scalar call arguments inside macros; effects and evaluation order remain obligations.");
            continue;
        }
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
        let safe = match change {
            crate::refactor::signature::Change::Remove(index) => arguments
                .named_children(&mut cursor)
                .filter(|node| !node.kind().contains("comment"))
                .nth(*index)
                .is_some_and(literal),
            crate::refactor::signature::Change::Move { .. } => {
                arguments.named_children(&mut cursor).all(literal)
            }
            crate::refactor::signature::Change::Add { .. } => false,
        };
        ensure!(safe,
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
        matches!(
            symbol.language,
            Language::Rust | Language::Python | Language::TypeScript | Language::Tsx
        ) && symbol.kind == SymbolKind::Function,
        "structural refactor requires a Rust, Python or TypeScript free function."
    );
    let (edits, detail) = match request {
        Request::Rename {
            name,
            documentation,
        } => {
            ensure!(name.len() <= 256, "rename name exceeds 256 bytes.");
            let mut plan = crate::refactor::rename::plan(project.index, symbol.id, &name)?;
            let documentation_edits = match documentation {
                Documentation::Preserve => 0,
                Documentation::Update => update_documentation(project, symbol, &name, &mut plan)?,
            };
            ensure!(plan.warnings.is_empty(), "structural rename has unresolved, dispatch or textual warnings; resolve them first.");
            (
                plan.edits,
                json!({"operation": "rename", "old_name": plan.old_name,
                "new_name": plan.new_name, "reference_edits": plan.reference_edits,
                "documentation_edits": documentation_edits}),
            )
        }
        request => {
            ensure!(
                symbol.language == Language::Rust,
                "signature changes require a Rust free function."
            );
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
            validate_scalar_calls(project, symbol, &change)?;
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
    let mut ranges = Vec::new();
    for (path, replacements) in edits.iter() {
        for replacement in replacements {
            ranges.push(json!({"path": path.strip_prefix(&project.root)?,
                "before_span": replacement.span, "after_bytes": replacement.replacement.len()}));
        }
    }
    Ok(Plan {
        edits,
        report: json!({"operation": "refactor", "handle": handle,
        "detail": detail, "edits": ranges, "changed": true, "behavior_checked": false,
        "preservation": "bytes outside planned edits; source behavior requires independent checks.",
        "scope": "Rust, Python and TypeScript free-function renames; Rust scalar signatures. Documentation updates exclude executable strings. External consumers and dynamic dispatch remain unchecked."}),
    })
}
