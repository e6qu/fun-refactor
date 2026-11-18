//! Evaluation and binding checks shared by signature commands and recipes.

use super::{Change, Site};
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::refactor::Refusal;
use crate::span::Span;
use anyhow::Result;
use std::path::Path;
use tree_sitter::Node;

fn refuse(detail: impl Into<String>) -> anyhow::Error {
    Refusal::Declined {
        detail: detail.into(),
    }
    .into()
}

/// Select by binding, not by the written position of a keyword argument.
pub(super) fn python_argument(
    parsed: &Parsed,
    source: &str,
    items: &[Span],
    index: usize,
    name: Option<&str>,
) -> Option<(usize, Span)> {
    let mut positional = 0;
    for (at, span) in items.iter().enumerate() {
        let node = parsed
            .root()
            .descendant_for_byte_range(span.start, span.end)?;
        if node.kind() == "keyword_argument" {
            if node
                .child_by_field_name("name")
                .map(|n| Span::from(n).text(source))
                == name
            {
                return Some((at, *span));
            }
        } else {
            if positional == index {
                return Some((at, *span));
            }
            positional += 1;
        }
    }
    None
}

/// Removing an expression must not discard a call, exception or destructor.
/// Reordering may also move plain reads, but only across other plain reads/literals.
pub(super) fn argument(site: &Site<'_>, span: Span, moving: bool) -> Result<()> {
    let node = site
        .parsed
        .root()
        .descendant_for_byte_range(span.start, span.end);
    let safe = node.is_some_and(|node| {
        if node.kind() == "token_tree" && site.language == Language::Rust {
            // Rust macro arguments are tokens in the surrounding parse tree.
            return rust_macro_literal(span.text(site.source));
        }
        let value = if node.kind() == "keyword_argument" {
            node.child_by_field_name("value").unwrap_or(node)
        } else {
            node
        };
        simple_value(value, site.source, moving)
    });
    if safe {
        return Ok(());
    }
    Err(refuse(format!(
        "cannot {} argument `{}` at {}: its evaluation may have effects or fail; {}",
        if moving { "move" } else { "remove" },
        span.text(site.source),
        super::location(site.file, span.start),
        if moving {
            "keep its evaluation order"
        } else {
            "only literal values can be discarded"
        }
    )))
}

fn rust_macro_literal(text: &str) -> bool {
    let source = format!("const FR_ARGUMENT: () = {text};");
    let Ok(parsed) = Parsers::new().parse(Language::Rust, &source) else {
        return false;
    };
    !parsed.has_errors()
        && parsed.root().named_child_count() == 1
        && parsed
            .root()
            .named_child(0)
            .and_then(|node| node.child_by_field_name("value"))
            .is_some_and(|node| simple_value(node, &source, false))
}

fn simple_value(node: Node<'_>, source: &str, names: bool) -> bool {
    match node.kind() {
        "integer_literal"
        | "float_literal"
        | "boolean_literal"
        | "char_literal"
        | "int_literal"
        | "rune_literal"
        | "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "decimal_floating_point_literal"
        | "hex_floating_point_literal"
        | "character_literal"
        | "integer"
        | "float"
        | "number"
        | "true"
        | "false"
        | "none"
        | "null"
        | "null_literal"
        | "nil"
        | "nil_literal"
        | "color_value"
        | "integer_value"
        | "float_value"
        | "plain_value" => true,
        "identifier" | "variable_name" => names,
        "string" | "string_literal" | "raw_string_literal" | "interpreted_string_literal" => {
            // Interpolated strings can call code even when their outer node is a string.
            !super::descendants(node).into_iter().any(|child| {
                matches!(
                    child.kind(),
                    "interpolation"
                        | "template_substitution"
                        | "command_substitution"
                        | "process_substitution"
                        | "expansion"
                        | "simple_expansion"
                )
            })
        }
        "parenthesized_expression" | "parenthesized_expr" => {
            let children: Vec<_> = node
                .named_children(&mut node.walk())
                .filter(|child| !child.kind().contains("comment"))
                .collect();
            children.len() == 1 && simple_value(children[0], source, names)
        }
        "unary_expression" | "unary_operator" => {
            let text = Span::from(node).text(source).trim_start();
            (text.starts_with('-') || text.starts_with('+'))
                && node.named_child_count() == 1
                && node.named_child(0).is_some_and(|child| {
                    matches!(
                        child.kind(),
                        "integer_literal"
                            | "float_literal"
                            | "int_literal"
                            | "integer"
                            | "float"
                            | "number"
                            | "decimal_integer_literal"
                            | "decimal_floating_point_literal"
                    )
                })
        }
        _ => false,
    }
}

pub(super) fn parameters(
    list: Node<'_>,
    source: &str,
    language: Language,
    change: &Change,
) -> Result<()> {
    let parameters: Vec<_> = list
        .named_children(&mut list.walk())
        .filter(|node| !node.kind().contains("comment"))
        .collect();
    for parameter in &parameters {
        if matches!(
            parameter.kind(),
            "list_splat_pattern"
                | "dictionary_splat_pattern"
                | "keyword_separator"
                | "positional_separator"
                | "variadic_parameter"
                | "variadic_parameter_declaration"
                | "rest_pattern"
                | "spread_parameter"
        ) || parameter
            .children_by_field_name("name", &mut parameter.walk())
            .count()
            > 1
            || parameter.named_children(&mut parameter.walk()).any(|node| {
                matches!(
                    node.kind(),
                    "rest_pattern" | "list_splat_pattern" | "dictionary_splat_pattern"
                )
            })
        {
            return Err(refuse("signature change cannot preserve variadic, grouped or keyword-only parameter bindings; use explicit positional parameters"));
        }
    }
    if language != Language::Python {
        return Ok(());
    }
    // Python evaluates defaults and annotations when it defines the function.
    let affected: Vec<_> = match change {
        Change::Remove(at) => parameters.get(*at).into_iter().copied().collect(),
        Change::Move { from, to } if from != to => parameters
            .iter()
            .skip(*from.min(to))
            .take(from.abs_diff(*to) + 1)
            .copied()
            .collect(),
        _ => Vec::new(),
    };
    for parameter in affected {
        if parameter
            .child_by_field_name("value")
            .is_some_and(|value| !simple_value(value, source, false))
        {
            return Err(refuse("changing this parameter would remove or reorder a default expression whose evaluation may have effects or fail"));
        }
        if parameter
            .child_by_field_name("type")
            .is_some_and(|annotation| {
                super::descendants(annotation).into_iter().any(|node| {
                    matches!(
                        node.kind(),
                        "call" | "binary_operator" | "subscript" | "attribute"
                    )
                })
            })
        {
            return Err(refuse("changing this parameter would remove or reorder an evaluated annotation; its effects are not established"));
        }
    }
    Ok(())
}

pub(super) fn shell_arguments(
    parsed: &Parsed,
    source: &str,
    file: &Path,
    arguments: &[Span],
    change: &Change,
) -> Result<()> {
    let affected: Vec<_> = match change {
        Change::Remove(at) => arguments.get(*at).into_iter().collect(),
        Change::Move { from, to } if from != to => arguments
            .iter()
            .skip(*from.min(to))
            .take(from.abs_diff(*to) + 1)
            .collect(),
        _ => Vec::new(),
    };
    for span in affected {
        let Some(node) = parsed
            .root()
            .descendant_for_byte_range(span.start, span.end)
        else {
            return Err(refuse(
                "cannot locate the shell argument whose evaluation would change",
            ));
        };
        if super::descendants(node).into_iter().any(|part| {
            matches!(
                part.kind(),
                "command_substitution" | "process_substitution" | "arithmetic_expansion"
            ) || part.kind() == "expansion" && Span::from(part).text(source).contains(['=', '?'])
        }) {
            return Err(refuse(format!("cannot remove or reorder shell argument `{}` at {}: its evaluation may have effects or fail",
                span.text(source), super::location(file, span.start))));
        }
    }
    Ok(())
}
