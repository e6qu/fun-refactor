//! Extract a subexpression into a named binding.
//!
//! The insertion point matters as much as the extraction: the binding goes at the
//! start of the statement containing the expression, at that statement's own
//! indentation, so the result reads like hand-written code. The expression's
//! original bytes are reused verbatim rather than reprinted, so any comments and
//! spacing inside it survive.

use super::Refusal;
use crate::edit::{line_indent, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use std::path::Path;
use tree_sitter::Node;

/// An extraction worked out but not applied.
#[derive(Debug)]
pub struct ExtractPlan {
    pub name: String,
    /// The extracted text, verbatim.
    pub expression: String,
    pub edits: EditSet,
    /// How many occurrences of the expression were replaced.
    pub occurrences: usize,
}

/// Extract the expression covering `span` into a binding called `name`.
///
/// With `all_occurrences`, every syntactically identical expression in the same
/// enclosing block is replaced too.
pub fn variable(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let language = info.language;

    if !supports_extract(language) {
        return Err(Refusal::Unsupported {
            operation: "extract variable".into(),
            language: language.to_string(),
        }
        .into());
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    let expr = expression_at(&parsed, span).ok_or_else(|| {
        anyhow::anyhow!(
            "no expression at bytes {span} in {}; select a complete expression",
            file.display()
        )
    })?;
    let expr_span = Span::from(expr);
    let expr_text = expr_span.text(&source).to_string();

    // Extracting a bare name would only alias it, which is never the intent.
    if expr.child_count() == 0 && expr.kind().contains("identifier") {
        anyhow::bail!("'{expr_text}' is already a name; extracting it would only create an alias");
    }

    // A name already defined in this file would collide or shadow.
    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let statement = enclosing_statement(expr)
        .ok_or_else(|| anyhow::anyhow!("could not find a statement to insert the binding before"))?;
    let statement_span = Span::from(statement);

    let targets = if all_occurrences {
        identical_siblings(&parsed, &source, expr, &expr_text)
    } else {
        vec![expr_span]
    };

    let indent = line_indent(&source, statement_span.start);
    let binding = format!("{}\n{indent}", render_binding(language, name, &expr_text));

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(statement_span.start, statement_span.start),
            binding,
            format!("introduce {name}"),
        ),
    );
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(*target, name, format!("use {name}")),
        );
    }

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: expr_text,
        edits,
        occurrences: targets.len(),
    })
}

/// Is extract-variable meaningful for this language?
fn supports_extract(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
    )
}

/// How a binding is written in each language.
fn render_binding(language: Language, name: &str, value: &str) -> String {
    match language {
        Language::Rust => format!("let {name} = {value};"),
        Language::Go => format!("{name} := {value}"),
        Language::Zig => format!("const {name} = {value};"),
        Language::TypeScript | Language::Tsx => format!("const {name} = {value};"),
        Language::Python => format!("{name} = {value}"),
        _ => format!("{name} = {value}"),
    }
}

/// The node covering the selection, widened through wrappers of identical extent.
fn expression_at<'a>(parsed: &'a Parsed, span: Span) -> Option<Node<'a>> {
    let node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;

    // Grammars often nest a value inside a wrapper node covering the same bytes;
    // the outermost of those is the one to replace.
    let mut best = node;
    let mut current = node;
    while let Some(parent) = current.parent() {
        if Span::from(parent) == Span::from(node) {
            best = parent;
            current = parent;
        } else {
            break;
        }
    }
    Some(best)
}

/// The statement the expression belongs to: the ancestor whose parent is a block.
fn enclosing_statement(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        let kind = parent.kind();
        if kind.contains("block")
            || kind.contains("body")
            || kind == "source_file"
            || kind == "program"
            || kind == "module"
        {
            return Some(current);
        }
        current = parent;
    }
}

/// Identical expressions elsewhere in the same enclosing block.
fn identical_siblings(parsed: &Parsed, source: &str, expr: Node<'_>, text: &str) -> Vec<Span> {
    let Some(scope) = enclosing_statement(expr).and_then(|s| s.parent()) else {
        return vec![Span::from(expr)];
    };
    let scope_span = Span::from(scope);

    let mut found = Vec::new();
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if !scope_span.contains(span) {
            // Keep descending through ancestors of the scope to reach it.
            if span.contains(scope_span) {
                stack.extend(node.children(&mut cursor));
            }
            continue;
        }
        if node.kind() == expr.kind() && span.text(source) == text {
            found.push(span);
            continue;
        }
        stack.extend(node.children(&mut cursor));
    }
    found.sort();
    found.dedup();
    if found.is_empty() {
        vec![Span::from(expr)]
    } else {
        found
    }
}

/// Suggest a binding name from an expression, for CLI convenience.
pub fn suggest_name(expression: &str) -> String {
    let cleaned: String = expression
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();

    let candidate = match words.as_slice() {
        [] => "extracted".to_string(),
        [single] => single.to_lowercase(),
        [first, rest @ ..] => {
            let tail = rest.last().copied().unwrap_or(first);
            if tail.chars().all(|c| c.is_numeric()) {
                first.to_lowercase()
            } else {
                tail.to_lowercase()
            }
        }
    };

    if candidate.chars().next().is_some_and(|c| c.is_numeric()) {
        format!("value_{candidate}")
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{scan, ScanOptions};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn apply(plan: &ExtractPlan, path: &Path) -> String {
        let original = std::fs::read_to_string(path).unwrap();
        apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
    }

    #[test]
    fn extracts_a_subexpression_into_a_binding() {
        let src = "fn f() {\n    let total = price * quantity + 10;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("price * quantity").unwrap();
        let span = Span::new(start, start + "price * quantity".len());
        let plan = variable(&index, &path, span, "subtotal", false).unwrap();

        assert_eq!(
            apply(&plan, &path),
            "fn f() {\n    let subtotal = price * quantity;\n    let total = subtotal + 10;\n}\n"
        );
    }

    #[test]
    fn inserts_at_the_statements_own_indentation() {
        let src = "fn f() {\n    if x {\n        let y = a + b;\n    }\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("a + b").unwrap();
        let plan = variable(&index, &path, Span::new(start, start + 5), "sum", false).unwrap();

        let out = apply(&plan, &path);
        assert!(
            out.contains("        let sum = a + b;\n        let y = sum;"),
            "binding should match the inner indentation:\n{out}"
        );
    }

    #[test]
    fn preserves_the_expression_bytes_verbatim() {
        // Odd internal spacing survives because the original bytes are reused.
        let src = "fn f() {\n    let y = foo( 1,  2 );\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("foo( 1,  2 )").unwrap();
        let plan = variable(
            &index,
            &path,
            Span::new(start, start + "foo( 1,  2 )".len()),
            "computed",
            false,
        )
        .unwrap();

        assert!(
            apply(&plan, &path).contains("let computed = foo( 1,  2 );"),
            "got:\n{}",
            apply(&plan, &path)
        );
    }

    #[test]
    fn replaces_every_occurrence_when_asked() {
        let src = "fn f() {\n    let a = x * 2;\n    let b = x * 2;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("x * 2").unwrap();
        let plan = variable(&index, &path, Span::new(start, start + 5), "doubled", true).unwrap();

        assert_eq!(plan.occurrences, 2);
        let out = apply(&plan, &path);
        assert_eq!(
            out,
            "fn f() {\n    let doubled = x * 2;\n    let a = doubled;\n    let b = doubled;\n}\n"
        );
        // The expression survives once, in the new binding, and nowhere else.
        assert_eq!(out.matches("x * 2").count(), 1, "got:\n{out}");
    }

    #[test]
    fn single_occurrence_mode_leaves_the_other_alone() {
        let src = "fn f() {\n    let a = x * 2;\n    let b = x * 2;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("x * 2").unwrap();
        let plan = variable(&index, &path, Span::new(start, start + 5), "doubled", false).unwrap();
        assert_eq!(plan.occurrences, 1);
        assert!(apply(&plan, &path).contains("let b = x * 2;"));
    }

    #[test]
    fn refuses_to_extract_a_bare_name() {
        let src = "fn f() {\n    let y = value;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("value").unwrap();
        let err = variable(&index, &path, Span::new(start, start + 5), "alias", false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("already a name"), "got: {err}");
    }

    #[test]
    fn refuses_a_name_already_in_use() {
        let src = "fn f() {\n    let existing = 1;\n    let y = a + b;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("a + b").unwrap();
        let err =
            variable(&index, &path, Span::new(start, start + 5), "existing", false).unwrap_err();
        assert!(
            err.downcast_ref::<Refusal>()
                .is_some_and(|r| matches!(r, Refusal::NameCollision { .. })),
            "got: {err}"
        );
    }

    #[test]
    fn refuses_unsupported_languages() {
        let (tmp, index) = workspace(&[("page.md", "# Title\n\nSome text.\n")]);
        let path = tmp.path().join("page.md");
        let err = variable(&index, &path, Span::new(0, 7), "x", false).unwrap_err();
        assert!(
            err.downcast_ref::<Refusal>()
                .is_some_and(|r| matches!(r, Refusal::Unsupported { .. })),
            "got: {err}"
        );
    }

    #[test]
    fn works_for_python_without_a_keyword() {
        let src = "def f():\n    total = price * qty + 1\n";
        let (tmp, index) = workspace(&[("a.py", src)]);
        let path = tmp.path().join("a.py");

        let start = src.find("price * qty").unwrap();
        let plan = variable(
            &index,
            &path,
            Span::new(start, start + "price * qty".len()),
            "subtotal",
            false,
        )
        .unwrap();

        assert_eq!(
            apply(&plan, &path),
            "def f():\n    subtotal = price * qty\n    total = subtotal + 1\n"
        );
    }

    #[test]
    fn works_for_typescript_with_const() {
        let src = "function f() {\n  const y = a * b + 1;\n}\n";
        let (tmp, index) = workspace(&[("a.ts", src)]);
        let path = tmp.path().join("a.ts");

        let start = src.find("a * b").unwrap();
        let plan = variable(&index, &path, Span::new(start, start + 5), "product", false).unwrap();
        assert!(apply(&plan, &path).contains("const product = a * b;"));
    }

    #[test]
    fn the_result_still_parses() {
        // The strongest guarantee: an extraction must never break the file.
        let src = "fn f() {\n    let total = price * quantity + 10;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("price * quantity").unwrap();
        let plan = variable(
            &index,
            &path,
            Span::new(start, start + "price * quantity".len()),
            "subtotal",
            false,
        )
        .unwrap();

        let outcomes =
            crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn name_suggestions_are_usable_identifiers() {
        assert_eq!(suggest_name("price"), "price");
        assert_eq!(suggest_name("1 + 2"), "value_1");
        assert!(!suggest_name("").is_empty());
    }
}

// ---------------------------------------------------------------- extract function

/// A function extraction worked out but not applied.
#[derive(Debug)]
pub struct ExtractFunctionPlan {
    pub name: String,
    pub edits: EditSet,
    /// Locals read inside the region but defined outside it.
    pub parameters: Vec<Parameter>,
    /// Locals defined inside the region and still used after it.
    pub returns: Vec<String>,
    /// Statements moved into the new function, verbatim.
    pub body: String,
}

/// A parameter the extracted function needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    /// The declared type, where the source states one.
    pub type_annotation: Option<String>,
}

impl Parameter {
    fn render(&self, language: Language) -> String {
        match (language, &self.type_annotation) {
            (Language::Python, _) => self.name.clone(),
            (_, Some(ty)) => format!("{}: {ty}", self.name),
            (_, None) => self.name.clone(),
        }
    }
}

/// Extract the statements covering `span` into a new function called `name`.
///
/// The moved statements keep their original bytes, so comments inside the extracted
/// region survive — the thing gopls is known to lose.
pub fn function(index: &Index, file: &Path, span: Span, name: &str) -> Result<ExtractFunctionPlan> {
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let language = info.language;

    if !supports_extract_function(language) {
        return Err(Refusal::Unsupported {
            operation: "extract function".into(),
            language: language.to_string(),
        }
        .into());
    }

    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    let region = statement_region(&parsed, span, &source)
        .ok_or_else(|| anyhow::anyhow!("select one or more complete statements to extract"))?;

    // A jump out of the region cannot be reproduced by a call, so the extraction
    // would change control flow. Refuse rather than produce something that compiles
    // but behaves differently.
    if let Some(kind) = escaping_control_flow(&parsed, region) {
        anyhow::bail!(
            "the selected code contains a `{kind}` that leaves the enclosing function; \
             a call cannot reproduce that, so this region cannot be extracted as-is"
        );
    }

    let enclosing = enclosing_function(index, file, region.start).ok_or_else(|| {
        anyhow::anyhow!("the selection is not inside a function, so there is nothing to extract from")
    })?;
    let enclosing_span = index
        .symbol(enclosing)
        .map(|s| s.full_span)
        .unwrap_or(region);

    // Data flow in: names used in the region whose definition lives outside it.
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut seen_params = std::collections::HashSet::new();
    for reference in references_within(index, file, region) {
        let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
            continue;
        };
        // Whether a binding is local is a question of scope, not of mutability: a
        // function-scoped `const` is every bit as local as a `let`. Functions and
        // types are reachable from the new function wherever they live, so only
        // value bindings declared inside the enclosing function can need passing.
        if !is_value_binding(target.kind) || !enclosing_span.contains(target.name_span) {
            continue;
        }
        if region.contains(target.name_span) {
            continue;
        }
        if seen_params.insert(target.name.clone()) {
            parameters.push(Parameter {
                name: target.name.clone(),
                type_annotation: declared_type(&parsed, &source, target.full_span),
            });
        }
    }
    parameters.sort_by(|a, b| a.name.cmp(&b.name));

    // Data flow out: locals defined in the region that are still read afterwards.
    let mut returns: Vec<String> = Vec::new();
    for symbol_id in &info.symbols {
        let Some(symbol) = index.symbol(*symbol_id) else {
            continue;
        };
        if !is_value_binding(symbol.kind) || !region.contains(symbol.name_span) {
            continue;
        }
        let used_after = index.references_to(symbol.id).iter().any(|r| {
            r.file == *file && r.span.start >= region.end && enclosing_span.contains(r.span)
        });
        if used_after {
            returns.push(symbol.name.clone());
        }
    }
    returns.sort();
    returns.dedup();

    // The declared type of the single returned binding, where the source states one.
    let return_type = returns.first().and_then(|name| {
        info.symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .find(|s| &s.name == name && region.contains(s.name_span))
            .and_then(|s| declared_type(&parsed, &source, s.full_span))
    });

    // Languages that require types on parameters cannot have them invented. Where a
    // binding's type was never written down there is nothing to recover, so the
    // extraction is refused with the names rather than emitting code that will not
    // compile.
    if requires_explicit_types(language) {
        let untyped: Vec<&str> = parameters
            .iter()
            .filter(|p| p.type_annotation.is_none())
            .map(|p| p.name.as_str())
            .collect();
        if !untyped.is_empty() {
            anyhow::bail!(
                "cannot extract: {language} requires a type on every parameter, and the \
                 type of {} was never written down, so there is none to copy. Annotate \
                 the declaration(s) and try again",
                untyped.join(", ")
            );
        }
        if let Some(returned) = returns.first() {
            if return_type.is_none() {
                anyhow::bail!(
                    "cannot extract: {language} requires a return type, and the type of \
                     '{returned}' was never written down. Annotate its declaration and \
                     try again"
                );
            }
        }
    }

    // More than one out-value needs a tuple or struct, which differs enough per
    // language that guessing would produce something unidiomatic.
    if returns.len() > 1 && !matches!(language, Language::Python | Language::Go) {
        anyhow::bail!(
            "the selected code produces {} values used afterwards ({}); returning several \
             values is not supported for {language} yet",
            returns.len(),
            returns.join(", ")
        );
    }

    let body = region.text(&source).to_string();
    let indent = line_indent(&source, region.start);
    let call = render_call(language, name, &parameters, &returns);
    let definition = render_function(
        language,
        name,
        &parameters,
        &returns,
        return_type.as_deref(),
        &body,
        &indent,
    );

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(region, call, format!("call {name}")),
    );
    // The new function goes after the one it came from, at that function's indentation.
    let insert_at = enclosing_span.end;
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            definition,
            format!("define {name}"),
        ),
    );

    Ok(ExtractFunctionPlan {
        name: name.to_string(),
        edits,
        parameters,
        returns,
        body,
    })
}

/// Kinds that hold a value and therefore have to cross the new function's boundary.
fn is_value_binding(kind: crate::model::SymbolKind) -> bool {
    use crate::model::SymbolKind;
    matches!(
        kind,
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter
    )
}

/// Does this language require a written type on every parameter and return?
fn requires_explicit_types(language: Language) -> bool {
    matches!(language, Language::Rust | Language::Go | Language::Zig)
}

fn supports_extract_function(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
    )
}

/// Widen a selection to the complete statements it touches.
///
/// A line selection starts on the line's indentation, which belongs to no statement,
/// so the search begins at the first real content inside the span and ends at the
/// last.
fn statement_region(parsed: &Parsed, span: Span, source: &str) -> Option<Span> {
    let text = span.text(source);
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    let content = Span::new(span.start + lead, span.end.saturating_sub(trail));
    if content.is_empty() {
        return None;
    }

    let first = parsed
        .root()
        .descendant_for_byte_range(content.start, content.start)?;
    let last = parsed
        .root()
        .descendant_for_byte_range(content.end.saturating_sub(1), content.end.saturating_sub(1))?;

    let start = statement_ancestor(first)?;
    let end = statement_ancestor(last)?;
    Some(Span::new(
        Span::from(start).start.min(content.start),
        Span::from(end).end.max(content.end),
    ))
}

/// Is this node kind a container whose children are statements?
fn is_statement_container(kind: &str) -> bool {
    kind.contains("block")
        || kind.contains("body")
        || kind == "source_file"
        || kind == "module"
        || kind == "program"
}

/// The ancestor of `node` that is a direct child of a statement container.
fn statement_ancestor(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        if is_statement_container(parent.kind()) {
            return Some(current);
        }
        current = parent;
    }
}

/// A `return`, `break` or `continue` inside the region that leaves it.
fn escaping_control_flow(parsed: &Parsed, region: Span) -> Option<&'static str> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if !span.overlaps(region) {
            continue;
        }
        if region.contains(span) {
            let kind = node.kind();
            // A break or continue belonging to a loop inside the region is fine; only
            // the ones escaping the selection matter, and a loop carries its own.
            if kind.contains("return_statement") || kind == "return" {
                return Some("return");
            }
            if (kind.contains("break") || kind.contains("continue"))
                && !has_enclosing_loop_within(node, region)
            {
                return Some(if kind.contains("break") {
                    "break"
                } else {
                    "continue"
                });
            }
        }
        stack.extend(node.children(&mut cursor));
    }
    None
}

/// Does a loop containing `node` also sit inside the region?
fn has_enclosing_loop_within(node: Node<'_>, region: Span) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let span = Span::from(parent);
        if !region.contains(span) {
            return false;
        }
        let kind = parent.kind();
        if kind.contains("for") || kind.contains("while") || kind.contains("loop") {
            return true;
        }
        current = parent;
    }
    false
}

/// The innermost function whose body contains `offset`.
fn enclosing_function(index: &Index, file: &Path, offset: usize) -> Option<crate::model::SymbolId> {
    let info = index.file(file)?;
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind.is_callable() && s.full_span.contains_offset(offset))
        .min_by_key(|s| s.full_span.len())
        .map(|s| s.id)
}

/// References whose span falls inside the region.
fn references_within<'a>(
    index: &'a Index,
    file: &Path,
    region: Span,
) -> Vec<&'a crate::model::Reference> {
    let Some(info) = index.file(file) else {
        return Vec::new();
    };
    info.references
        .iter()
        .map(|i| &index.references[*i])
        .filter(|r| region.contains(r.span))
        .collect()
}

/// The type written at a declaration site, if the source states one.
///
/// There is no type inference here: a binding whose type the programmer left to the
/// compiler has none to recover, and the caller is told rather than guessed at.
fn declared_type(parsed: &Parsed, source: &str, declaration: Span) -> Option<String> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    let ty = node.child_by_field_name("type")?;
    Some(Span::from(ty).text(source).trim().to_string())
}

/// The call that replaces the extracted region.
fn render_call(
    language: Language,
    name: &str,
    parameters: &[Parameter],
    returns: &[String],
) -> String {
    let args = parameters
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let call = format!("{name}({args})");

    match (returns.len(), language) {
        (0, Language::Python) => call,
        (0, _) => format!("{call};"),
        (1, Language::Python) => format!("{} = {call}", returns[0]),
        (1, Language::Rust) => format!("let {} = {call};", returns[0]),
        (1, Language::Go) => format!("{} := {call}", returns[0]),
        (1, _) => format!("const {} = {call};", returns[0]),
        (_, Language::Python) => format!("{} = {call}", returns.join(", ")),
        (_, Language::Go) => format!("{} := {call}", returns.join(", ")),
        _ => format!("{call};"),
    }
}

/// The new function definition.
fn render_function(
    language: Language,
    name: &str,
    parameters: &[Parameter],
    returns: &[String],
    return_type: Option<&str>,
    body: &str,
    indent: &str,
) -> String {
    let params = parameters
        .iter()
        .map(|p| p.render(language))
        .collect::<Vec<_>>()
        .join(", ");

    // The body keeps its original bytes; only its indentation is adjusted.
    let body_indent = match language {
        Language::Python => "    ",
        _ => "    ",
    };
    let reindented = body
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                let stripped = line.strip_prefix(indent).unwrap_or(line);
                format!("{body_indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    match language {
        Language::Python => {
            let tail = match returns.len() {
                0 => String::new(),
                _ => format!("\n{body_indent}return {}", returns.join(", ")),
            };
            format!("\n\ndef {name}({params}):\n{reindented}{tail}")
        }
        Language::Rust => {
            let ret = match return_type {
                Some(ty) => format!(" -> {ty}"),
                None => String::new(),
            };
            let tail = match returns.first() {
                Some(r) => format!("\n{body_indent}{r}"),
                None => String::new(),
            };
            format!("\n\nfn {name}({params}){ret} {{\n{reindented}{tail}\n}}")
        }
        Language::Go => {
            let ret = match return_type {
                Some(ty) => format!(" {ty}"),
                None => String::new(),
            };
            let tail = match returns.len() {
                0 => String::new(),
                _ => format!("\n{body_indent}return {}", returns.join(", ")),
            };
            format!("\n\nfunc {name}({params}){ret} {{\n{reindented}{tail}\n}}")
        }
        _ => {
            let tail = match returns.first() {
                Some(r) => format!("\n{body_indent}return {r};"),
                None => String::new(),
            };
            format!("\n\nfunction {name}({params}) {{\n{reindented}{tail}\n}}")
        }
    }
}
