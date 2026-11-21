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

pub(super) fn moving_arguments(site: &Site<'_>, spans: &[Span]) -> Result<()> {
    let uncertain: Vec<_> = spans
        .iter()
        .filter(|span| argument(site, **span, true).is_err())
        .collect();
    if uncertain.len() == 1 {
        let span = *uncertain[0];
        let projection = site
            .parsed
            .root()
            .descendant_for_byte_range(span.start, span.end);
        if projection.is_some_and(|node| member_read(node, site.source))
            && spans.iter().all(|other| {
                *other == span
                    || argument(site, *other, false).is_ok()
                    || isolated_parameter_read(site, *other)
            })
        {
            return Ok(());
        }
    }
    for span in spans {
        argument(site, *span, true)?;
    }
    Ok(())
}

fn member_read(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "attribute"
        | "member_expression"
        | "field_expression"
        | "field_access"
        | "selector_expression" => {
            let object = ["object", "value", "operand", "argument"]
                .iter()
                .find_map(|name| node.child_by_field_name(name))
                .or_else(|| node.named_child(0));
            object.is_some_and(|object| {
                matches!(object.kind(), "identifier" | "self" | "this")
                    || member_read(object, source)
            })
        }
        "reference_expression" if Span::from(node).text(source).starts_with('&') => node
            .child_by_field_name("value")
            .is_some_and(|value| member_read(value, source)),
        _ => false,
    }
}

fn isolated_parameter_read(site: &Site<'_>, span: Span) -> bool {
    let Some(node) = site
        .parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
    else {
        return false;
    };
    if node.kind() != "identifier" {
        return false;
    }
    let name = span.text(site.source);
    let mut parent = node.parent();
    while let Some(scope) = parent {
        if let Some(parameters) = super::parameter_list(scope) {
            let occurrences: Vec<_> = super::descendants(scope)
                .into_iter()
                .filter(|part| {
                    part.kind() == "identifier" && Span::from(*part).text(site.source) == name
                })
                .collect();
            return occurrences.len() == 2
                && occurrences
                    .iter()
                    .any(|part| Span::from(parameters).contains(Span::from(*part)))
                && !super::descendants(scope).into_iter().any(|part| {
                    part.kind() == "identifier"
                        && matches!(
                            Span::from(part).text(site.source),
                            "eval" | "exec" | "arguments"
                        )
                });
        }
        parent = scope.parent();
    }
    false
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
    items: &[Span],
    change: &Change,
) -> Result<()> {
    let parameters: Vec<_> = list
        .named_children(&mut list.walk())
        .filter(|node| !node.kind().contains("comment"))
        .collect();
    for parameter in &parameters {
        if !(language == Language::Python && !matches!(change, Change::Add { .. })) && (matches!(
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
            }))
        {
            return Err(refuse("signature change cannot preserve variadic, grouped or keyword-only parameter bindings; use explicit positional parameters"));
        }
    }
    let parameters: Vec<_> = parameters
        .into_iter()
        .filter(|node| items.contains(&Span::from(*node)))
        .collect();
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
    if matches!(
        language,
        Language::Python | Language::TypeScript | Language::Tsx
    ) {
        for parameter in &affected {
            if parameter
                .child_by_field_name("value")
                .is_some_and(|value| !simple_value(value, source, false))
            {
                return Err(refuse("changing this parameter would remove or reorder a default expression whose evaluation may have effects or fail"));
            }
        }
    }
    if language != Language::Python {
        return Ok(());
    }
    if list
        .parent()
        .and_then(|node| node.parent())
        .is_some_and(|node| node.kind() == "decorated_definition")
    {
        return Err(refuse("a decorator can replace this function's calling convention; its signature binding is not established"));
    }
    if let Change::Add {
        at, declaration, ..
    } = change
    {
        let added_source = format!("def __fr({declaration}):\n    pass\n");
        let parsed = Parsers::new().parse(language, &added_source)?;
        let added = parsed
            .root()
            .named_child(0)
            .and_then(|node| node.child_by_field_name("parameters"));
        let Some(added) = added
            .filter(|node| !parsed.has_errors() && node.named_child_count() == 1)
            .and_then(|node| node.named_child(0))
        else {
            return Err(refuse(
                "add requires exactly one valid parameter declaration",
            ));
        };
        let mut defaults: Vec<_> = parameters
            .iter()
            .map(|node| node.child_by_field_name("value").is_some())
            .collect();
        if *at <= defaults.len() {
            defaults.insert(*at, added.child_by_field_name("value").is_some());
            if defaults.windows(2).any(|pair| pair == [true, false]) {
                return Err(refuse(
                    "adding this parameter would put a required parameter after a default",
                ));
            }
        }
    }
    for parameter in affected {
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
            ) || part.kind() == "variable_name"
                && matches!(Span::from(part).text(source), "RANDOM" | "SRANDOM")
                || part.kind() == "expansion" && Span::from(part).text(source).contains(['=', '?'])
        }) {
            return Err(refuse(format!("cannot remove or reorder shell argument `{}` at {}: its evaluation may have effects or fail",
                span.text(source), super::location(file, span.start))));
        }
    }
    Ok(())
}
