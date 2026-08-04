//! Micro-rewrites: the small, local transformations editors offer as code actions.
//!
//! Each one is a pure syntactic rewrite of a single construct — no cross-file
//! reasoning, no type information — which is why they are cheap enough to offer a
//! menu of. [`available`] answers "what applies here", the shape an editor needs.
//!
//! Negation is the shared hard part: `!(a && b)` and `a != b` are the same idea
//! spelled differently per language, and a naive `!(...)` wrapper produces
//! double-negatives that read worse than the original. The negation here simplifies
//! as it goes.

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
}

impl Rewrite {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rewrite::InvertIf => "invert-if",
            Rewrite::DeMorgan => "de-morgan",
            Rewrite::GuardClause => "guard-clause",
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Rewrite::InvertIf => "swap the branches and negate the condition",
            Rewrite::DeMorgan => "distribute the negation over the operator",
            Rewrite::GuardClause => "return early instead of nesting the body",
        }
    }

    pub fn from_name(name: &str) -> Option<Rewrite> {
        match name {
            "invert-if" => Some(Rewrite::InvertIf),
            "de-morgan" => Some(Rewrite::DeMorgan),
            "guard-clause" => Some(Rewrite::GuardClause),
            _ => None,
        }
    }

    pub const ALL: &'static [Rewrite] =
        &[Rewrite::InvertIf, Rewrite::DeMorgan, Rewrite::GuardClause];
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
///
/// A rewrite is only offered if its result reparses, so the menu never lists
/// something that applying it would then refuse. It is the same check the commit
/// makes, run early.
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
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let language = info.language;

    if !supported(language) {
        return Err(Refusal::Unsupported {
            operation: rewrite.as_str().to_string(),
            language: language.to_string(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    let (span, replacement) = match rewrite {
        Rewrite::InvertIf => invert_if(&parsed, &source, offset, language)?,
        Rewrite::DeMorgan => de_morgan(&parsed, &source, offset, language)?,
        Rewrite::GuardClause => guard_clause(&parsed, &source, offset, language)?,
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(span, replacement, rewrite.as_str().to_string()),
    );

    Ok(RewritePlan {
        rewrite,
        edits,
        before: span.text(&source).to_string(),
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

/// The pieces of an `if`, however the grammar spells them.
struct IfParts {
    /// The expression a negation applies to. Where a grammar makes the parentheses
    /// part of the condition, this is what sits inside them, so that rewriting it
    /// leaves the brackets the language requires standing.
    condition: Span,
    consequence: Span,
    alternative: Option<Span>,
    /// The `else` leads to another `if` rather than to a block.
    chained: bool,
}

/// Locate an `if`'s condition and branches.
///
/// Grammars disagree about all three. The condition is bare in Rust, Go, Python and
/// Zig but is a `parenthesized_expression` in the C family, and only the inside of
/// it may be rewritten. The consequence is a `consequence` field everywhere except
/// Zig, which calls it `body`. Bash names none of it: its branches are delimited by
/// the `then`, `else` and `fi` keyword tokens, with the statements sitting as bare
/// children in between.
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

/// What a negation applies to, looking through the brackets a grammar folds into
/// the condition. `if (a)` in the C family yields the `a`, not the `(a)`.
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

/// Does this `else` lead to another `if` rather than to a block?
///
/// An `else if` cannot have its branches swapped: the second condition is only ever
/// tested when the first is false, so moving the block out from under it changes
/// which tests run.
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

/// Swap an if/else's branches and negate its condition.
fn invert_if(
    parsed: &Parsed,
    source: &str,
    offset: usize,
    language: Language,
) -> Result<(Span, String)> {
    let node =
        enclosing_if(parsed, offset).ok_or_else(|| anyhow::anyhow!("no `if` at that position"))?;

    let parts = if_parts(node)
        .ok_or_else(|| anyhow::anyhow!("could not find the condition and branches"))?;
    if parts.chained {
        anyhow::bail!(
            "this `if` continues into an `else if`; its later conditions are only \
             tested when this one is false, so swapping the branches would change \
             which of them run"
        );
    }
    let alternative = parts.alternative.ok_or_else(|| {
        anyhow::anyhow!("this `if` has no `else`; there is nothing to swap it with")
    })?;
    let negated = negate(parts.condition.text(source), language);

    let whole = Span::from(node);
    let text = whole.text(source);
    let mut out = String::with_capacity(text.len());

    // Rebuild by splicing the three parts, so everything between them — spacing,
    // keywords, comments — survives exactly.
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
///
/// Zig puts a `labeled_statement` in the way, so the search descends. Returning the
/// clause itself when no block is found would splice the `else` keyword into the
/// consequence position, so an unrecognised shape is `None` and the caller refuses.
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
    let unary = enclosing_kind(parsed, offset, |k| {
        k.contains("unary") || k.contains("not_operator")
    })
    .ok_or_else(|| anyhow::anyhow!("no negation at that position"))?;

    let span = Span::from(unary);
    let text = span.text(source);
    let (not_token, and_op, or_op) = boolean_spelling(language);
    if !text.trim_start().starts_with(not_token.trim()) {
        anyhow::bail!("the expression at that position is not a negation");
    }

    // The operand, with any wrapping parentheses removed.
    let mut cursor = unary.walk();
    let operand = unary
        .named_children(&mut cursor)
        .next()
        .ok_or_else(|| anyhow::anyhow!("could not find what is being negated"))?;
    let inner = strip_parentheses(Span::from(operand), source);
    let inner_text = inner.text(source);

    // Split on the top-level boolean operator.
    let (left, op, right) = split_boolean(inner_text, and_op, or_op)
        .ok_or_else(|| anyhow::anyhow!("the negated expression is not an `and`/`or`"))?;

    let flipped = if op == and_op { or_op } else { and_op };
    let rewritten = format!(
        "{} {flipped} {}",
        negate(left.trim(), language),
        negate(right.trim(), language)
    );

    // `!(a && b)` is one operand; `!a || !b` is two, and the brackets that used to
    // hold it together are gone with the negation. Inside another operator that
    // silently rebinds the expression — `x && !(a && b)` would become
    // `x && !a || !b` — so the grouping has to come back.
    if unary.parent().is_some_and(|p| binds_operands(p.kind())) {
        if language == Language::Bash {
            anyhow::bail!(
                "the result needs grouping to keep its meaning here, and `( … )` \
                 opens a subshell in shell"
            );
        }
        return Ok((span, format!("({rewritten})")));
    }
    Ok((span, rewritten))
}

/// Would this node take the expression inside it as an operand of its own operator?
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
    let node =
        enclosing_if(parsed, offset).ok_or_else(|| anyhow::anyhow!("no `if` at that position"))?;

    let parts = if_parts(node)
        .ok_or_else(|| anyhow::anyhow!("could not find the condition and branches"))?;
    if parts.alternative.is_some() {
        anyhow::bail!("this `if` has an `else`; invert it instead of guarding");
    }

    // The `if` must be the last thing in its enclosing block, or an early return
    // would skip whatever follows. The comparison is against the statement the `if`
    // forms, which may sit inside a wrapper node rather than directly in the block.
    let (statement, block) = statement_in_block(node)
        .ok_or_else(|| anyhow::anyhow!("the `if` has no enclosing block"))?;
    let mut cursor = block.walk();
    let siblings: Vec<Node> = block
        .named_children(&mut cursor)
        .filter(|c| !c.kind().contains("comment"))
        .collect();
    if siblings.last().copied().map(Span::from) != Some(Span::from(statement)) {
        anyhow::bail!(
            "the `if` is not the last statement in its block; returning early would \
             skip the statements after it"
        );
    }

    // What "early exit" means depends on what the block is. Last in a loop body means
    // `continue`; last in a function body means `return`. ripgrep has an `if` ending
    // a `for` body inside a function returning `Result<PathBuf>`, where `return` was
    // both the wrong control flow and the wrong type.
    let exit = early_exit(block, source, language)?;

    let negated = negate(parts.condition.text(source), language);
    let body = strip_block(parts.consequence, source);
    let indent = crate::edit::line_indent(source, Span::from(node).start);

    // Reuse the source's own header — everything from the `if` keyword up to the
    // body — with the condition negated in place. Whatever the language spells
    // around the condition, brackets in Zig and the C family, `:` in Python, `; then`
    // in shell, is preserved rather than reinvented per language.
    let start = Span::from(node).start;
    let header = format!(
        "{}{negated}{}",
        &source[start..parts.condition.start],
        &source[parts.condition.end..parts.consequence.start]
    );
    let header = header.trim_end();

    // The body is already indented one level past the `if`, so the file's own unit
    // is the difference — tabs in Go, two spaces in most TypeScript. Guessing four
    // spaces would reindent every guard this touches.
    let unit = crate::edit::indent_unit(source);
    let guard = match language {
        Language::Python => format!("{header}\n{indent}{unit}{exit}\n"),
        Language::Bash => format!("{header}\n{indent}{unit}{exit}\n{indent}fi\n"),
        // Go's grammar accepts the semicolon, but no Go is written with it.
        Language::Go => format!("{header} {{\n{indent}{unit}{exit}\n{indent}}}\n"),
        _ => format!("{header} {{\n{indent}{unit}{exit};\n{indent}}}\n"),
    };

    // The body loses one level of indentation as it leaves the block. Blank lines
    // stay blank rather than collecting trailing spaces.
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

    // The guard ends in a newline, so every body line — including the first — needs
    // the indentation it was given above.
    Ok((Span::from(node), format!("{guard}{body_text}")))
}

/// The statement that exits the block early: `continue` in a loop, `return` in a
/// function.
///
/// A guard clause replaces nesting with an early exit, and which exit is correct is
/// decided by the block, not by the refactoring. ripgrep's `find_program` ends a
/// `for` body with an `if`; rewriting that to `return` leaves the loop entirely, and
/// leaves it with no value in a function returning `Result<PathBuf>`.
///
/// A function that declares a return type is refused rather than guessed at: what to
/// return early is a decision only the author can make.
fn early_exit(block: Node<'_>, source: &str, language: Language) -> Result<&'static str> {
    let mut current = block;
    for _ in 0..4 {
        let Some(parent) = current.parent() else {
            break;
        };
        let kind = parent.kind();
        if kind.contains("for") || kind.contains("while") || kind == "loop_expression" {
            return Ok("continue");
        }
        if kind.contains("function") || kind.contains("method") || kind.contains("closure") {
            if declares_a_return_value(parent, source, language) {
                anyhow::bail!(
                    "this function returns a value, so an early exit needs one too, and \
                     what to return is not something this can decide"
                );
            }
            return Ok("return");
        }
        current = parent;
    }
    // Neither a loop nor a function: a bare block, a module body, a shell script.
    Ok("return")
}

/// Does this function declare that it returns something?
fn declares_a_return_value(function: Node<'_>, source: &str, language: Language) -> bool {
    // Every grammar in the set names the return type field, except Go, which calls it
    // `result`, and shell, which has none.
    for field in ["return_type", "result", "type"] {
        if let Some(node) = function.child_by_field_name(field) {
            let text = Span::from(node).text(source).trim();
            let nothing = match language {
                Language::Rust => text.is_empty() || text == "()",
                // Java and Zig both spell "returns nothing" as a written `void`
                // rather than as an absent type, so an empty test would read every
                // method as returning something and refuse every guard clause in the
                // language.
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
///
/// The node must be named: a bare `if` keyword token also has the kind "if", and
/// asking a keyword for its condition finds nothing.
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
        // Shell negates a command with `! cmd`, so the sigil needs its space.
        Language::Bash => ("! ", "&&", "||"),
        _ => ("!", "&&", "||"),
    }
}

/// Negate an expression, simplifying rather than piling up operators.
fn negate(expression: &str, language: Language) -> String {
    let trimmed = expression.trim();
    let (not_token, _, _) = boolean_spelling(language);

    // A double negative cancels.
    if let Some(rest) = trimmed.strip_prefix(not_token) {
        let inner = rest.trim();
        return strip_outer_parentheses(inner).to_string();
    }

    // A comparison flips to its opposite rather than gaining a `!`.
    for (from, to) in [
        (" == ", " != "),
        (" != ", " == "),
        (" >= ", " < "),
        (" <= ", " > "),
        (" > ", " <= "),
        (" < ", " >= "),
    ] {
        if trimmed.contains(from) && !trimmed.contains("&&") && !trimmed.contains("||") {
            return trimmed.replacen(from, to, 1);
        }
    }
    if language == Language::Python {
        for (from, to) in [(" is not ", " is "), (" not in ", " in ")] {
            if trimmed.contains(from) {
                return trimmed.replacen(from, to, 1);
            }
        }
        for (from, to) in [(" is ", " is not "), (" in ", " not in ")] {
            if trimmed.contains(from) && !trimmed.contains(" is not ") {
                return trimmed.replacen(from, to, 1);
            }
        }
    }

    // A compound expression needs brackets so the negation binds to all of it —
    // except in shell, where `( … )` opens a subshell. Negating a command there is
    // just `! cmd`, and adding brackets would change what the code does.
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

/// Is the whole expression wrapped in one pair of brackets?
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
                // Closing before the end means these are not a matching outer pair.
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
