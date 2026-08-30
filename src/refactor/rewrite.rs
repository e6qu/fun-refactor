//! Micro-rewrites: the small, local transformations editors offer as code actions.

use super::Refusal;
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use std::path::Path;
use tree_sitter::Node;

/// A transformation that can apply at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rewrite {
    /// Swap an if/else's branches, negating the condition.
    InvertIf,
    /// Push a negation through `&&`/`||`, flipping the operator.
    DeMorgan,
    /// Turn a whole-body `if` into an early-return guard.
    GuardClause,
    /// Move a nested function to module scope.
    HoistFunction,
}

impl Rewrite {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rewrite::InvertIf => "invert-if",
            Rewrite::DeMorgan => "de-morgan",
            Rewrite::GuardClause => "guard-clause",
            Rewrite::HoistFunction => "hoist-function",
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Rewrite::InvertIf => "swap the branches and negate the condition",
            Rewrite::DeMorgan => "distribute the negation over the operator",
            Rewrite::GuardClause => "return early instead of nesting the body",
            Rewrite::HoistFunction => "move this nested function to module scope",
        }
    }

    pub fn from_name(name: &str) -> Option<Rewrite> {
        match name {
            "invert-if" => Some(Rewrite::InvertIf),
            "de-morgan" => Some(Rewrite::DeMorgan),
            "guard-clause" => Some(Rewrite::GuardClause),
            "hoist-function" => Some(Rewrite::HoistFunction),
            _ => None,
        }
    }

    pub const ALL: &'static [Rewrite] = &[
        Rewrite::InvertIf,
        Rewrite::DeMorgan,
        Rewrite::GuardClause,
        Rewrite::HoistFunction,
    ];
}

/// A rewrite worked out but not applied.
#[derive(Debug)]
pub struct RewritePlan {
    pub rewrite: Rewrite,
    pub edits: EditSet,
    /// The construct before the change, for display.
    pub before: String,
}

/// Which rewrites apply at `offset`.
pub fn available(index: &Index, file: &Path, offset: usize) -> Result<Vec<Rewrite>> {
    let mut found = Vec::new();
    for rewrite in Rewrite::ALL {
        let Ok(plan) = apply(index, file, offset, *rewrite) else {
            continue;
        };
        if crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).is_ok() {
            found.push(*rewrite);
        }
    }
    Ok(found)
}

/// Apply `rewrite` at `offset`.
pub fn apply(index: &Index, file: &Path, offset: usize, rewrite: Rewrite) -> Result<RewritePlan> {
    if let Some(language) = index.file(file).map(|i| i.language) {
        crate::capabilities::record(crate::capabilities::Capability::MicroRewrites, language);
    }
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let language = info.language;

    if !supported(language) {
        return Err(Refusal::Unsupported {
            operation: rewrite.as_str().to_string(),
            language,
            because: "",
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    // Most rewrites replace one span.
    let replacements = match rewrite {
        Rewrite::InvertIf => vec![invert_if(&parsed, &source, offset, language)?],
        Rewrite::DeMorgan => vec![de_morgan(&parsed, &source, offset, language)?],
        Rewrite::GuardClause => vec![guard_clause(&parsed, &source, offset, language)?],
        Rewrite::HoistFunction => hoist_function(&parsed, &source, offset, language)?,
    };

    let before = replacements
        .first()
        .map(|(span, _)| span.text(&source).to_string())
        .unwrap_or_default();
    let mut edits = EditSet::new();
    for (span, replacement) in replacements {
        edits.add(
            file.to_path_buf(),
            Edit::new(span, replacement, rewrite.as_str().to_string()),
        );
    }

    Ok(RewritePlan {
        rewrite,
        edits,
        before,
    })
}

pub fn supported(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Bash
            | Language::Java
    )
}

/// Move the nested function at `offset` to module scope.
fn hoist_function(
    parsed: &Parsed,
    source: &str,
    offset: usize,
    language: Language,
) -> Result<Vec<(Span, String)>> {
    if language != Language::Rust {
        return Err(Refusal::Unsupported {
            operation: "hoist-function".to_string(),
            language,
            because: "an inner function there can capture the enclosing scope, and \
                      hoisting it would change what its names mean",
        }
        .into());
    }
    let node = parsed
        .descendant_at(offset, offset)
        .ok_or_else(|| Refusal::Declined {
            detail: "nothing at that position".to_string(),
        })
        .map_err(anyhow::Error::from)?;

    // The innermost function containing the offset, and the top-level item above it.
    let mut nested = None;
    let mut top = None;
    let mut current = Some(node);
    while let Some(at) = current {
        if at.kind() == "function_item" && nested.is_none() {
            nested = Some(at);
        }
        if at.parent().is_some_and(|p| p.kind() == "source_file") {
            top = Some(at);
        }
        current = at.parent();
    }
    let (Some(nested), Some(top)) = (nested, top) else {
        return Err(Refusal::Declined {
            detail: "that position is not inside a function".to_string(),
        }
        .into());
    };
    if Span::from(nested) == Span::from(top) {
        return Err(Refusal::Declined {
            detail: "that function is already at module scope; point inside a nested one"
                .to_string(),
        }
        .into());
    }

    let name = nested
        .child_by_field_name("name")
        .map(|n| Span::from(n).text(source).to_string())
        .unwrap_or_default();
    // A module-level item of the same name would now be two definitions of one thing.
    if parsed
        .root()
        .named_children(&mut parsed.root().walk())
        .any(|item| {
            item.child_by_field_name("name")
                .is_some_and(|n| Span::from(n).text(source) == name)
        })
    {
        return Err(Refusal::Declined {
            detail: format!(
                "the module already defines `{name}`, so the hoisted function would collide \
             with it"
            ),
        }
        .into());
    }

    // The lines the nested function occupies, indentation and doc comments included.
    let line_start = source[..nested.start_byte()]
        .rfind('\n')
        .map(|at| at + 1)
        .unwrap_or(0);
    let indent = &source[line_start..nested.start_byte()];
    let mut from = line_start;
    // A comment right above the function is about the function, and travels with it.
    loop {
        let above_end = match from.checked_sub(1) {
            Some(end) if source.as_bytes().get(end) == Some(&b'\n') => end,
            _ => break,
        };
        let above_start = source[..above_end]
            .rfind('\n')
            .map(|at| at + 1)
            .unwrap_or(0);
        let line = &source[above_start..above_end];
        match line.strip_prefix(indent) {
            Some(rest) if rest.starts_with("//") => from = above_start,
            _ => break,
        }
    }
    let to = source[nested.end_byte()..]
        .find('\n')
        .map(|at| nested.end_byte() + at + 1)
        .unwrap_or(source.len());

    // The function, re-indented for module scope.
    let dedented: String = source[from..to]
        .lines()
        .map(|line| line.strip_prefix(indent).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let hoisted = format!("\n\n{}", dedented.trim_end());

    Ok(vec![
        (Span::new(from, blank_after(source, to)), String::new()),
        (Span::new(top.end_byte(), top.end_byte()), hoisted),
    ])
}

/// The end of the run of blank lines starting at `from`.
fn blank_after(source: &str, from: usize) -> usize {
    let mut at = from;
    for line in source[from..].split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        at += line.len();
    }
    at
}

/// The pieces of an `if`, however the grammar spells them.
struct IfParts {
    /// The expression a negation applies to.
    condition: Span,
    consequence: Span,
    alternative: Option<Span>,
    /// The `else` leads to another `if` and not to a block.
    chained: bool,
}

/// Locate an `if`'s condition and branches.
fn if_parts(node: Node<'_>) -> Option<IfParts> {
    let condition = condition_expression(node.child_by_field_name("condition")?);

    if let Some(consequence) = node
        .child_by_field_name("consequence")
        .or_else(|| node.child_by_field_name("body"))
    {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        let else_clause = node.child_by_field_name("alternative").or_else(|| {
            children
                .iter()
                .find(|c| c.kind().contains("else") || c.kind().contains("elif"))
                .copied()
        });
        return Some(IfParts {
            condition,
            consequence: Span::from(consequence),
            alternative: else_clause.and_then(else_body_of).map(Span::from),
            chained: else_clause.is_some_and(continues_into_another_if),
        });
    }

    // Bash: walk the children, using the keywords as boundaries.
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    let then_end = children
        .iter()
        .find(|c| c.kind() == "then")
        .map(|c| c.end_byte())?;
    let else_clause = children.iter().find(|c| c.kind().contains("else"));
    let fi_start = children
        .iter()
        .find(|c| c.kind() == "fi")
        .map(|c| c.start_byte());

    let consequence_end = else_clause
        .map(|c| c.start_byte())
        .or(fi_start)
        .unwrap_or(node.end_byte());

    let alternative = else_clause.and_then(|clause| {
        let mut inner = clause.walk();
        let kids: Vec<Node> = clause.children(&mut inner).collect();
        let start = kids
            .iter()
            .find(|c| c.kind() == "else")
            .map(|c| c.end_byte())?;
        Some(Span::new(start, clause.end_byte()))
    });

    Some(IfParts {
        condition,
        consequence: Span::new(then_end, consequence_end),
        alternative,
        chained: else_clause.is_some_and(|c| c.kind().contains("elif")),
    })
}

/// What a negation applies to, looking through the brackets a grammar folds into the condition.
fn condition_expression(condition: Node<'_>) -> Span {
    if condition.kind().contains("parenthesized") {
        let mut cursor = condition.walk();
        let inner: Vec<Node> = condition.named_children(&mut cursor).collect();
        if let Some(first) = inner.first() {
            return Span::from(*first);
        }
    }
    Span::from(condition)
}

/// Reports whether this `else` leads to another `if` rather than to a block.
fn continues_into_another_if(clause: Node<'_>) -> bool {
    if clause.kind().contains("elif") {
        return true;
    }
    let mut cursor = clause.walk();
    let children: Vec<Node> = clause.named_children(&mut cursor).collect();
    children
        .iter()
        .any(|c| c.kind().starts_with("if_") || c.kind() == "if_expression")
}

/// Reports whether this `if` binds what it tested.
fn binds_a_payload(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children.iter().any(|child| child.kind() == "payload")
}

/// Swap an if/else's branches and negate its condition.
fn invert_if(
    parsed: &Parsed,
    source: &str,
    offset: usize,
    language: Language,
) -> Result<(Span, String)> {
    let node = enclosing_if(parsed, offset)
        .ok_or_else(|| Refusal::Declined {
            detail: "no `if` at that position".to_string(),
        })
        .map_err(anyhow::Error::from)?;
    if binds_a_payload(node) {
        return Err(Refusal::Declined {
            detail: "this `if` binds what it tested; there is no condition here to negate, and \
             the payload would have nothing to bind"
                .to_string(),
        }
        .into());
    }

    let parts = if_parts(node)
        .ok_or_else(|| anyhow::anyhow!("could not find the condition and branches"))?;
    if parts.chained {
        return Err(Refusal::Declined {
            detail: "this `if` continues into an `else if`; it reaches its later conditions only when this one is false, so swapping the branches would change \
             which of them run".to_string(),
        }
        .into());
    }
    let alternative = parts.alternative.ok_or_else(|| {
        anyhow::Error::from(Refusal::Declined {
            detail: "this `if` has no `else`; there is nothing to swap it with".to_string(),
        })
    })?;
    let negated = negate(parts.condition.text(source), language);

    let whole = Span::from(node);
    let text = whole.text(source);
    let mut out = String::with_capacity(text.len());

    // Splice the three parts back in, so the spacing, keywords and comments between
    // them survive byte for byte.
    let base = whole.start;
    let cond = parts.condition;
    let cons = parts.consequence;
    let alt = alternative;

    out.push_str(&text[..cond.start - base]);
    out.push_str(&negated);
    out.push_str(&text[cond.end - base..cons.start - base]);
    out.push_str(alt.text(source));
    out.push_str(&text[cons.end - base..alt.start - base]);
    out.push_str(cons.text(source));
    out.push_str(&text[alt.end - base..]);

    Ok((whole, out))
}

/// The block an else clause wraps.
fn else_body_of(alternative: Node<'_>) -> Option<Node<'_>> {
    if alternative.kind().contains("block") {
        return Some(alternative);
    }
    let mut cursor = alternative.walk();
    let children: Vec<Node> = alternative.named_children(&mut cursor).collect();
    children
        .iter()
        .find(|c| c.kind().contains("block"))
        .copied()
        .or_else(|| children.iter().find_map(|c| else_body_of(*c)))
}

/// Push a negation through a boolean operator.
fn de_morgan(
    parsed: &Parsed,
    source: &str,
    offset: usize,
    language: Language,
) -> Result<(Span, String)> {
    // tree-sitter-zig reads `!(a and b)` as an `error_union_type`.
    let unary = enclosing_kind(parsed, offset, |k| {
        k.contains("unary") || k.contains("not_operator") || k == "error_union_type"
    })
    .ok_or_else(|| Refusal::Declined {
        detail: "no negation at that position".to_string(),
    })
    .map_err(anyhow::Error::from)?;

    let span = Span::from(unary);
    let text = span.text(source);
    let (not_token, and_op, or_op) = boolean_spelling(language);
    if !text.trim_start().starts_with(not_token.trim()) {
        return Err(Refusal::Declined {
            detail: "the expression at that position is not a negation".to_string(),
        }
        .into());
    }

    // The operand, with any wrapping parentheses removed.
    let mut cursor = unary.walk();
    let operand = unary
        .named_children(&mut cursor)
        .next()
        .ok_or_else(|| anyhow::anyhow!("could not find what is being negated"))?;
    let inner = strip_parentheses(Span::from(operand), source);
    let inner_text = inner.text(source);

    let (left, op, right) = split_boolean(inner_text, and_op, or_op)
        .ok_or_else(|| Refusal::Declined {
            detail: "the negated expression is not an `and`/`or`".to_string(),
        })
        .map_err(anyhow::Error::from)?;

    let flipped = if op == and_op { or_op } else { and_op };
    let rewritten = format!(
        "{} {flipped} {}",
        negate(left.trim(), language),
        negate(right.trim(), language)
    );

    // `!(a && b)` is one operand and `!a || !b` is two, and the negation took the brackets that
    // held them together with it.
    if unary.parent().is_some_and(|p| binds_operands(p.kind())) {
        if language == Language::Bash {
            return Err(Refusal::Declined {
                detail: "the result needs grouping to keep its meaning here, and `( … )` \
                 opens a subshell in shell"
                    .to_string(),
            }
            .into());
        }
        return Ok((span, format!("({rewritten})")));
    }
    Ok((span, rewritten))
}

/// Reports whether this node takes the expression inside it as its own operand.
fn binds_operands(kind: &str) -> bool {
    kind.contains("binary")
        || kind.contains("unary")
        || kind.contains("boolean_operator")
        || kind.contains("not_operator")
}

/// Turn a trailing whole-body `if` into an early return.
fn guard_clause(
    parsed: &Parsed,
    source: &str,
    offset: usize,
    language: Language,
) -> Result<(Span, String)> {
    let node = enclosing_if(parsed, offset)
        .ok_or_else(|| Refusal::Declined {
            detail: "no `if` at that position".to_string(),
        })
        .map_err(anyhow::Error::from)?;
    if binds_a_payload(node) {
        return Err(Refusal::Declined {
            detail: "this `if` binds what it tested; there is no condition here to negate, and \
             the payload would have nothing to bind"
                .to_string(),
        }
        .into());
    }

    let parts = if_parts(node)
        .ok_or_else(|| anyhow::anyhow!("could not find the condition and branches"))?;
    if parts.alternative.is_some() {
        return Err(Refusal::Declined {
            detail: "this `if` has an `else`; invert it instead of guarding".to_string(),
        }
        .into());
    }

    // The `if` must come last in its enclosing block, or an early return would skip whatever
    // follows.
    let (statement, block) = statement_in_block(node)
        .ok_or_else(|| Refusal::Declined {
            detail: "the `if` has no enclosing block".to_string(),
        })
        .map_err(anyhow::Error::from)?;
    let mut cursor = block.walk();
    let siblings: Vec<Node> = block
        .named_children(&mut cursor)
        .filter(|c| !c.kind().contains("comment"))
        .collect();
    if siblings.last().copied().map(Span::from) != Some(Span::from(statement)) {
        return Err(Refusal::Declined {
            detail: "the `if` is not the last statement in its block; returning early would \
             skip the statements after it"
                .to_string(),
        }
        .into());
    }

    // The block decides what "early exit" means.
    let exit = early_exit(block, source, language)?;

    let negated = negate(parts.condition.text(source), language);
    let body = strip_block(parts.consequence, source);
    let indent = crate::edit::line_indent(source, Span::from(node).start);

    // Reuse the source's own header: everything from the `if` keyword up to the body, with the
    // condition negated in place.
    let start = Span::from(node).start;
    let header = format!(
        "{}{negated}{}",
        &source[start..parts.condition.start],
        &source[parts.condition.end..parts.consequence.start]
    );
    let header = header.trim_end();

    // The body already sits one level past the `if`, so the file's own unit measures the
    // difference: tabs in Go, two spaces in most TypeScript.
    let unit = crate::edit::indent_unit(source);
    let guard = match language {
        Language::Python => format!("{header}\n{indent}{unit}{exit}\n"),
        Language::Bash => format!("{header}\n{indent}{unit}{exit}\n{indent}fi\n"),
        // Go's grammar accepts the semicolon, and nobody writes one.
        Language::Go => format!("{header} {{\n{indent}{unit}{exit}\n{indent}}}\n"),
        _ => format!("{header} {{\n{indent}{unit}{exit};\n{indent}}}\n"),
    };

    // The body loses one level of indentation as it leaves the block.
    let dedented: Vec<String> = body
        .text(source)
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                let stripped = line
                    .strip_prefix(&format!("{indent}{unit}"))
                    .unwrap_or_else(|| line.trim_start());
                format!("{indent}{stripped}")
            }
        })
        .collect();
    let body_text = dedented
        .iter()
        .rev()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    // The guard ends in a newline, so every body line, including the first, needs
    // the indentation above it.
    Ok((Span::from(node), format!("{guard}{body_text}")))
}

/// The statement that exits the block early: `continue` in a loop, `return` in a function.
fn early_exit(block: Node<'_>, source: &str, language: Language) -> Result<&'static str> {
    let mut current = block;
    while let Some(parent) = current.parent() {
        let kind = parent.kind();
        if kind.contains("for") || kind.contains("while") || kind == "loop_expression" {
            return Ok("continue");
        }
        if kind.contains("function") || kind.contains("method") || kind.contains("closure") {
            if declares_a_return_value(parent, source, language) {
                return Err(Refusal::Declined {
                    detail: "this function returns a value, so an early exit needs one too, and \
                     what to return is not something this can decide"
                        .to_string(),
                }
                .into());
            }
            return Ok("return");
        }
        current = parent;
    }
    // No loop and no function: a bare block, a module body, a shell script.
    Ok("return")
}

/// Reports whether this function declares that it returns something.
fn declares_a_return_value(function: Node<'_>, source: &str, language: Language) -> bool {
    // Every grammar in the set names the return type field, except Go, which calls it
    // `result`, and shell, which has none.
    for field in ["return_type", "result", "type"] {
        if let Some(node) = function.child_by_field_name(field) {
            let text = Span::from(node).text(source).trim();
            let nothing = match language {
                Language::Rust => text.is_empty() || text == "()",
                // Java and Zig both write "returns nothing" as a `void` that is present in the
                // source.
                Language::Zig | Language::Java => text == "void",
                Language::TypeScript | Language::Tsx => {
                    text.trim_start_matches(':').trim() == "void"
                }
                _ => text.is_empty(),
            };
            if !nothing {
                return true;
            }
        }
    }
    false
}

/// The statement a node forms, and the block that statement sits in.
fn statement_in_block(node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    let mut current = node;
    for _ in 0..8 {
        let parent = current.parent()?;
        if crate::refactor::is_statement_container(parent.kind()) {
            return Some((current, parent));
        }
        current = parent;
    }
    None
}

/// The innermost `if` construct containing `offset`.
fn enclosing_if<'a>(parsed: &'a Parsed, offset: usize) -> Option<Node<'a>> {
    enclosing_kind(parsed, offset, |k| {
        k.starts_with("if_") || k == "if_expression" || k == "if_statement"
    })
}

/// The innermost ancestor of the node at `offset` matching `predicate`.
fn enclosing_kind<'a>(
    parsed: &'a Parsed,
    offset: usize,
    predicate: impl Fn(&str) -> bool,
) -> Option<Node<'a>> {
    let mut node = parsed.root().descendant_for_byte_range(offset, offset)?;
    for _ in 0..12 {
        if node.is_named() && predicate(node.kind()) {
            return Some(node);
        }
        node = node.parent()?;
    }
    None
}

/// How a language spells negation and the boolean operators.
fn boolean_spelling(language: Language) -> (&'static str, &'static str, &'static str) {
    match language {
        Language::Python => ("not ", "and", "or"),
        // Zig writes the two logical operators as words, as Python does, and negates with a
        // sigil, as C does.
        Language::Zig => ("!", "and", "or"),
        // Shell negates a command with `!
        Language::Bash => ("! ", "&&", "||"),
        _ => ("!", "&&", "||"),
    }
}

/// Where `needle` sits at the top level of `text`, outside every bracket.
fn top_level_find(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && text[i..].starts_with(needle) => return Some(i),
            _ => {}
        }
    }
    None
}

/// Negate an expression, simplifying instead of piling up operators.
fn negate(expression: &str, language: Language) -> String {
    let trimmed = expression.trim();
    let (not_token, and_op, or_op) = boolean_spelling(language);

    // A double negative cancels when the `!` covers the whole expression.
    if split_boolean(trimmed, and_op, or_op).is_none() {
        if let Some(rest) = trimmed.strip_prefix(not_token) {
            let inner = rest.trim();
            return strip_outer_parentheses(inner).to_string();
        }
    }

    // A comparison flips to its opposite instead of gaining a `!`.
    if split_boolean(trimmed, and_op, or_op).is_none() {
        let mut flips: Vec<(&str, &str)> = vec![
            (" == ", " != "),
            (" != ", " == "),
            (" >= ", " < "),
            (" <= ", " > "),
            (" > ", " <= "),
            (" < ", " >= "),
        ];
        if language == Language::Python {
            // Longest first, or ` is ` matches inside ` is not `.
            flips.splice(
                0..0,
                [
                    (" is not ", " is "),
                    (" not in ", " in "),
                    (" is ", " is not "),
                    (" in ", " not in "),
                ],
            );
        }
        for (from, to) in flips {
            if let Some(at) = top_level_find(trimmed, from) {
                return format!("{}{to}{}", &trimmed[..at], &trimmed[at + from.len()..]);
            }
        }
    }

    // A compound expression needs brackets so the negation binds to all of it.
    if language != Language::Bash && trimmed.contains(' ') && !is_parenthesised(trimmed) {
        format!("{not_token}({trimmed})")
    } else {
        format!("{not_token}{trimmed}")
    }
}

/// Split on the outermost `&&` or `||`, ignoring bracketed sections.
fn split_boolean<'a>(
    text: &'a str,
    and_op: &'static str,
    or_op: &'static str,
) -> Option<(&'a str, &'static str, &'a str)> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                for op in [and_op, or_op] {
                    if text[i..].starts_with(op) {
                        // Word operators must not match inside an identifier.
                        let alpha = op.chars().all(|c| c.is_alphabetic());
                        let before_ok = !alpha || i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                        let after = i + op.len();
                        let after_ok =
                            !alpha || after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
                        if before_ok && after_ok {
                            return Some((&text[..i], op, &text[after..]));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Remove one layer of wrapping parentheses from a span.
fn strip_parentheses(span: Span, source: &str) -> Span {
    let text = span.text(source);
    if is_parenthesised(text.trim()) {
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        return Span::new(span.start + lead + 1, span.end - trail - 1);
    }
    span
}

/// The statements inside a block, without its braces.
fn strip_block(span: Span, source: &str) -> Span {
    let text = span.text(source);
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let lead = text.len() - text.trim_start().len();
        let trail = text.len() - text.trim_end().len();
        return Span::new(span.start + lead + 1, span.end - trail - 1);
    }
    span
}

/// Reports whether one pair of brackets wraps the whole expression.
fn is_parenthesised(text: &str) -> bool {
    if !(text.starts_with('(') && text.ends_with(')')) {
        return false;
    }
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                // A bracket closing before the end leaves the outer pair unmatched.
                if depth == 0 && i + 1 != text.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn strip_outer_parentheses(text: &str) -> &str {
    if is_parenthesised(text) {
        text[1..text.len() - 1].trim()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negation_cancels_a_double_negative() {
        assert_eq!(negate("!ready", Language::Rust), "ready");
        assert_eq!(negate("!(a && b)", Language::Rust), "a && b");
        assert_eq!(negate("not ready", Language::Python), "ready");
    }

    #[test]
    fn negation_flips_a_comparison_rather_than_wrapping_it() {
        assert_eq!(negate("a == b", Language::Rust), "a != b");
        assert_eq!(negate("x < y", Language::Rust), "x >= y");
        assert_eq!(negate("x >= y", Language::Rust), "x < y");
    }

    #[test]
    fn python_negation_uses_python_spelling() {
        assert_eq!(negate("ready", Language::Python), "not ready");
        assert_eq!(negate("x is None", Language::Python), "x is not None");
        assert_eq!(negate("k in d", Language::Python), "k not in d");
    }

    #[test]
    fn negation_brackets_a_compound_expression() {
        assert_eq!(negate("a && b", Language::Rust), "!(a && b)");
        assert_eq!(negate("ready", Language::Rust), "!ready");
    }

    #[test]
    fn boolean_split_ignores_bracketed_operators() {
        let (l, op, r) = split_boolean("(a || b) && c", "&&", "||").unwrap();
        assert_eq!(op, "&&");
        assert_eq!(l.trim(), "(a || b)");
        assert_eq!(r.trim(), "c");
    }

    #[test]
    fn word_operators_do_not_match_inside_identifiers() {
        // `android` contains "and" but is one identifier.
        assert!(split_boolean("android", "and", "or").is_none());
        let (l, op, _) = split_boolean("a and b", "and", "or").unwrap();
        assert_eq!(op, "and");
        assert_eq!(l.trim(), "a");
    }

    #[test]
    fn parenthesis_detection_needs_a_matching_outer_pair() {
        assert!(is_parenthesised("(a && b)"));
        assert!(!is_parenthesised("(a) && (b)"));
        assert!(!is_parenthesised("a && b"));
    }

    #[test]
    fn rewrite_names_round_trip() {
        for rewrite in Rewrite::ALL {
            assert_eq!(Rewrite::from_name(rewrite.as_str()), Some(*rewrite));
            assert!(!rewrite.describe().is_empty());
        }
        assert_eq!(Rewrite::from_name("nonsense"), None);
    }
}
