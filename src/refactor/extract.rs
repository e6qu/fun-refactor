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
use crate::model::SymbolKind;
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

    // The config languages have no bindings, so each gets the construct that plays
    // the same role there: a Terraform `local`, a YAML anchor, a CSS custom property,
    // a Markdown link reference definition.
    match language {
        Language::Hcl => return hcl_local(index, file, span, name, all_occurrences),
        Language::Yaml | Language::Helm => {
            return yaml_anchor(index, file, span, name, all_occurrences)
        }
        Language::Css | Language::Scss => {
            return css_custom_property(index, file, span, name, all_occurrences)
        }
        Language::Markdown => {
            return markdown_link_definition(index, file, span, name, all_occurrences)
        }
        _ => {}
    }

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
        // HTML has no construct that names a value, so the matrix marks the cell n/a.
        let (tmp, index) = workspace(&[("page.html", "<html><body>hi</body></html>\n")]);
        let path = tmp.path().join("page.html");
        let err = variable(&index, &path, Span::new(6, 12), "x", false).unwrap_err();
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

    // Helm's analogue of a function is a named template, which lives in `_helpers.tpl`
    // and is called through `include`.
    if language == Language::Helm {
        return helm_named_template(file, span, name);
    }

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

// ------------------------------------------------------- config languages
//
// None of these languages has a binding form, so "extract variable" means the
// construct that plays the same role: name a value once and refer to it. What that
// construct is differs per language, and so does where it has to be written, but the
// shape of the work is identical everywhere — splice a declaration in, replace the
// occurrences with a reference to it, touch nothing else.

/// Every node in the tree, in source order, that `keep` accepts.
fn collect_nodes<'a>(root: Node<'a>, mut keep: impl FnMut(Node<'a>) -> bool) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if keep(node) {
            out.push(node);
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out.sort_by_key(|n| (n.start_byte(), n.end_byte()));
    out
}

/// The smallest node covering the selection.
fn descendant_at<'a>(parsed: &'a Parsed, span: Span) -> Option<Node<'a>> {
    parsed
        .root()
        .descendant_for_byte_range(span.start, span.end.max(span.start))
}

/// The innermost ancestor of `node` (or `node` itself) with this kind.
fn ancestor_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut current = node;
    loop {
        if current.kind() == kind {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Named children of `node` with a given kind.
fn named_children_of_kind<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| c.kind() == kind)
        .collect()
}

fn invalid(name: &str, reason: &str) -> anyhow::Error {
    Refusal::InvalidName {
        name: name.to_string(),
        reason: reason.to_string(),
    }
    .into()
}

// ------------------------------------------------------------ Terraform / HCL

/// Extract a Terraform expression into a `locals` entry.
///
/// The entry joins the first `locals` block in the file, or a new one written at the
/// top when the file has none. Terraform's scope is the module directory, so the name
/// is checked against every declaration in that directory, not just this file.
fn hcl_local(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
    {
        return Err(invalid(
            name,
            "a Terraform local must start with a letter or underscore and contain only \
             letters, digits, underscores and dashes",
        ));
    }

    if let Some(existing) = hcl_module_collision(index, file, name) {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: existing.clone(),
        }
        .into());
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Hcl, &source)?;

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let expr = ancestor_of_kind(node, "expression").ok_or_else(|| {
        anyhow::anyhow!(
            "no Terraform expression at bytes {span} in {}; select a complete expression \
             such as an attribute's value",
            file.display()
        )
    })?;
    let expr_span = Span::from(expr);
    let expr_text = expr_span.text(&source).to_string();

    // `local.x` already names a value; extracting it would only add a second name.
    let trimmed = expr_text.trim();
    if let Some(rest) = trimmed
        .strip_prefix("local.")
        .or_else(|| trimmed.strip_prefix("var."))
    {
        if !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            anyhow::bail!(
                "`{trimmed}` is already a named value; extracting it would only create an alias"
            );
        }
    }

    let locals_block = collect_nodes(parsed.root(), |n| {
        n.kind() == "block"
            && n.named_child(0)
                .is_some_and(|c| c.kind() == "identifier" && Span::from(c).text(&source) == "locals")
    })
    .into_iter()
    .next();

    // An occurrence inside the `locals` block would be rewritten to a reference to the
    // entry being defined there, which is a cycle Terraform rejects.
    let locals_span = locals_block.map(Span::from);
    let targets: Vec<Span> = if all_occurrences {
        let found: Vec<Span> = collect_nodes(parsed.root(), |n| {
            n.kind() == "expression" && Span::from(n).text(&source) == expr_text
        })
        .into_iter()
        .map(Span::from)
        .filter(|s| !locals_span.is_some_and(|l| l.contains(*s)))
        .collect();
        if found.is_empty() {
            vec![expr_span]
        } else {
            found
        }
    } else {
        vec![expr_span]
    };

    let (insert_at, insert_text) = match locals_block {
        Some(block) => {
            let body = named_children_of_kind(block, "body")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("the `locals` block has no body"))?;
            let attributes = named_children_of_kind(body, "attribute");
            match attributes.last() {
                Some(last) => {
                    let indent = line_indent(&source, last.start_byte());
                    // Past the end of that attribute's line, so a trailing comment
                    // stays with the attribute it annotates.
                    let after = source[last.end_byte()..]
                        .find('\n')
                        .map(|i| last.end_byte() + i)
                        .unwrap_or(source.len());
                    (after, format!("\n{indent}{name} = {expr_text}"))
                }
                None => {
                    let brace = named_children_of_kind(block, "block_start")
                        .into_iter()
                        .next()
                        .map(|b| b.end_byte())
                        .unwrap_or(body.start_byte());
                    (brace, format!("\n  {name} = {expr_text}"))
                }
            }
        }
        None => (0, format!("locals {{\n  {name} = {expr_text}\n}}\n\n")),
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            insert_text,
            format!("declare local.{name}"),
        ),
    );
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(*target, format!("local.{name}"), format!("use local.{name}")),
        );
    }

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: expr_text,
        edits,
        occurrences: targets.len(),
    })
}

/// A declaration of `name` anywhere in the same Terraform module directory.
fn hcl_module_collision(index: &Index, file: &Path, name: &str) -> Option<std::path::PathBuf> {
    let dir = file.parent();
    index
        .find_symbols(name, None)
        .into_iter()
        .find(|s| s.language == Language::Hcl && s.file.parent() == dir)
        .map(|s| s.file.clone())
}

// ------------------------------------------------------------------ Helm / YAML

/// Extract a repeated YAML scalar into an anchor plus aliases.
///
/// YAML requires an anchor to precede its aliases, so the *first* occurrence in the
/// document carries `&name` and every later one becomes `*name`, whichever occurrence
/// was selected.
fn yaml_anchor(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    let language = index
        .file(file)
        .map(|i| i.language)
        .unwrap_or(Language::Yaml);

    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '{' | '}' | ',' | '&' | '*'))
    {
        return Err(invalid(
            name,
            "a YAML anchor name may not be empty or contain whitespace, `[`, `]`, `{`, \
             `}`, `,`, `&` or `*`",
        ));
    }

    if index
        .find_symbols(name, Some(file))
        .iter()
        .any(|s| s.kind == SymbolKind::Anchor)
    {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    if let Some(action) = parsed.template_actions.iter().find(|a| a.overlaps(span)) {
        anyhow::bail!(
            "the selection at bytes {span} overlaps the template action `{}`. Helm \
             `{{{{ ... }}}}` actions are masked out before the YAML parse, so nothing \
             inside one is visible as YAML and no anchor can be placed there",
            action.text(&source)
        );
    }

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let selected = yaml_anchorable(node, &source).ok_or_else(|| {
        anyhow::anyhow!(
            "no anchorable scalar at bytes {span} in {}; select a scalar value of a \
             mapping key or a sequence item (a value that already carries an anchor or \
             alias, and a block scalar, cannot be anchored)",
            file.display()
        )
    })?;
    let selected_span = Span::from(selected);
    let value_text = selected_span.text(&source).to_string();

    let occurrences: Vec<Span> = if all_occurrences {
        collect_nodes(parsed.root(), |n| {
            yaml_is_anchorable(n, &source) && Span::from(n).text(&source) == value_text
        })
        .into_iter()
        .map(Span::from)
        .collect()
    } else {
        vec![selected_span]
    };
    let occurrences = if occurrences.is_empty() {
        vec![selected_span]
    } else {
        occurrences
    };

    if let Some(clash) = occurrences
        .iter()
        .find(|s| parsed.template_actions.iter().any(|a| a.overlaps(**s)))
    {
        anyhow::bail!(
            "an occurrence at bytes {clash} overlaps a masked Helm template action; \
             refusing to rewrite bytes the YAML parse never saw"
        );
    }

    // The anchor has to come first in document order or the aliases dangle.
    let anchor_at = occurrences[0];
    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(anchor_at.start, anchor_at.start),
            format!("&{name} "),
            format!("anchor {name}"),
        ),
    );
    for alias in occurrences.iter().skip(1) {
        edits.add(
            file.to_path_buf(),
            Edit::new(*alias, format!("*{name}"), format!("alias {name}")),
        );
    }

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: value_text,
        edits,
        occurrences: occurrences.len(),
    })
}

/// The node an anchor would attach to, walking out from the selection.
fn yaml_anchorable<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    let mut current = node;
    loop {
        if yaml_is_anchorable(current, source) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Is this node a plain scalar sitting in a value position?
fn yaml_is_anchorable(node: Node<'_>, _source: &str) -> bool {
    if node.kind() != "flow_node" {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let in_value_position = match parent.kind() {
        "block_mapping_pair" | "flow_pair" => parent
            .child_by_field_name("value")
            .is_some_and(|v| v.id() == node.id()),
        "block_sequence_item" | "flow_sequence" => true,
        _ => false,
    };
    if !in_value_position {
        return false;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    // An anchored or aliased node already names a value; a block scalar spans lines,
    // and splicing one at an alias would depend on the alias site's indentation.
    !children.is_empty()
        && children.iter().all(|c| {
            matches!(
                c.kind(),
                "plain_scalar" | "double_quote_scalar" | "single_quote_scalar"
            )
        })
}

// ---------------------------------------------------------------------- CSS/SCSS

/// Extract a declaration value into a custom property declared in `:root`.
///
/// A custom property is the form that works in both dialects, and is what a bare
/// name produces. In an SCSS file a name written with a leading `$` produces an SCSS
/// variable instead, which its own grammar understands.
fn css_custom_property(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    let language = index
        .file(file)
        .map(|i| i.language)
        .unwrap_or(Language::Css);

    // `$name` asks for an SCSS variable, which only the SCSS grammar understands.
    let scss_variable = name.starts_with('$');
    if scss_variable && language != Language::Scss {
        anyhow::bail!(
            "`{name}` asks for an SCSS `$variable`, but {} is plain CSS, which has no \
             such syntax. Use a name without the `$` to extract a CSS custom property, \
             which works in both dialects",
            file.display()
        );
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;
    if parsed.has_errors() {
        if false {
            anyhow::bail!(
                "{} does not parse under the CSS grammar, which is the only one available \
                 for SCSS here — SCSS-only syntax (`$variables`, `@mixin`/`@include`, \
                 `@use`) is not CSS. The selection cannot be located reliably in a broken \
                 tree, so nothing is rewritten",
                file.display()
            );
        }
        anyhow::bail!(
            "{} has syntax errors, so the selection cannot be located reliably",
            file.display()
        );
    }

    // An SCSS variable keeps its `$`; a custom property is normalised to `--name`.
    let property = if scss_variable {
        name.to_string()
    } else if name.starts_with("--") {
        name.to_string()
    } else {
        format!("--{name}")
    };
    if !index.find_symbols(&property, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: property,
            file: file.to_path_buf(),
        }
        .into());
    }

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let value = css_declaration_value(node).ok_or_else(|| {
        anyhow::anyhow!(
            "no declaration value at bytes {span} in {}; select the value of a \
             declaration, such as the colour in `color: #3366ff`",
            file.display()
        )
    })?;
    let value_span = Span::from(value);
    let value_text = value_span.text(&source).to_string();

    let root_rule = css_root_rule(&parsed, &source);
    let root_span = root_rule.map(Span::from);

    let targets: Vec<Span> = if all_occurrences {
        let found: Vec<Span> = collect_nodes(parsed.root(), |n| {
            n.is_named()
                && n.kind() == value.kind()
                && n.parent().is_some_and(|p| p.kind() == "declaration")
                && Span::from(n).text(&source) == value_text
        })
        .into_iter()
        .map(Span::from)
        // A rewrite inside the `:root` rule the declaration is being added to would
        // define the property in terms of itself.
        .filter(|s| !root_span.is_some_and(|r| r.contains(*s)))
        .collect();
        if found.is_empty() {
            vec![value_span]
        } else {
            found
        }
    } else {
        vec![value_span]
    };

    let declaration = format!("{property}: {value_text};");

    // An SCSS variable is declared at the top level of the stylesheet, not inside a
    // `:root` rule — that is a CSS custom property's home, and `$vars` are resolved
    // by the compiler rather than the cascade.
    if scss_variable {
        let insert_at = css_insertion_point(&parsed, &source);
        let mut edits = EditSet::new();
        edits.add(
            file.to_path_buf(),
            Edit::new(
                Span::new(insert_at, insert_at),
                format!("{declaration}\n\n"),
                format!("declare {property}"),
            ),
        );
        for target in &targets {
            edits.add(
                file.to_path_buf(),
                Edit::new(*target, property.clone(), format!("use {property}")),
            );
        }
        return Ok(ExtractPlan {
            name: property,
            expression: value_text,
            edits,
            occurrences: targets.len(),
        });
    }

    let (insert_at, insert_text) = match root_rule {
        Some(rule) => {
            let block = named_children_of_kind(rule, "block")
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("the `:root` rule has no block"))?;
            let declarations = named_children_of_kind(block, "declaration");
            match declarations.last() {
                Some(last) => {
                    if Span::from(block).text(&source).contains('\n') {
                        let indent = line_indent(&source, last.start_byte());
                        (last.end_byte(), format!("\n{indent}{declaration}"))
                    } else {
                        (last.end_byte(), format!(" {declaration}"))
                    }
                }
                None => (
                    block.start_byte() + 1,
                    format!("\n  {declaration}\n"),
                ),
            }
        }
        None => (
            css_insertion_point(&parsed, &source),
            format!(":root {{\n  {declaration}\n}}\n\n"),
        ),
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            insert_text,
            format!("declare {property}"),
        ),
    );
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(
                *target,
                format!("var({property})"),
                format!("use var({property})"),
            ),
        );
    }

    Ok(ExtractPlan {
        name: property,
        expression: value_text,
        edits,
        occurrences: targets.len(),
    })
}

/// The value node of the declaration containing `node`, if the selection is in one.
fn css_declaration_value<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        if parent.kind() == "declaration" {
            if !current.is_named() || current.kind() == "property_name" {
                return None;
            }
            return Some(current);
        }
        current = parent;
    }
}

/// The `:root { }` rule of a stylesheet, if it has one.
fn css_root_rule<'a>(parsed: &'a Parsed, source: &str) -> Option<Node<'a>> {
    collect_nodes(parsed.root(), |n| {
        n.kind() == "rule_set"
            && named_children_of_kind(n, "selectors")
                .first()
                .is_some_and(|s| Span::from(*s).text(source).trim() == ":root")
    })
    .into_iter()
    .next()
}

/// Where a new rule may be written: after any leading `@charset` / `@import`, which
/// CSS requires to come before every rule.
fn css_insertion_point(parsed: &Parsed, source: &str) -> usize {
    let root = parsed.root();
    let mut cursor = root.walk();
    let mut offset = 0usize;
    for child in root.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "import_statement" | "charset_statement" | "comment"
        ) {
            offset = child.end_byte();
        } else {
            break;
        }
    }
    if offset == 0 {
        return 0;
    }
    // Step over the newline that ends the last leading statement.
    match source[offset..].find('\n') {
        Some(0) => offset + 1,
        _ => offset,
    }
}

// ---------------------------------------------------------------------- Markdown

/// Turn an inline link's destination into a link reference definition.
fn markdown_link_definition(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    if name.is_empty() || name.contains(['[', ']', '\n']) {
        return Err(invalid(
            name,
            "a link reference label may not be empty or contain `[`, `]` or a newline",
        ));
    }
    if index
        .find_symbols(name, Some(file))
        .iter()
        .any(|s| s.kind == SymbolKind::LinkDef)
    {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Markdown, &source)?;

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let link = ancestor_of_kind(node, "link").ok_or_else(|| {
        anyhow::anyhow!(
            "no link at bytes {span} in {}; select an inline link `[text](destination)`",
            file.display()
        )
    })?;
    let (parens, destination) = markdown_inline_destination(link, &source).ok_or_else(|| {
        anyhow::anyhow!(
            "the link at bytes {span} in {} has no inline `(destination)`; only an \
             inline link can become a reference definition",
            file.display()
        )
    })?;

    let targets: Vec<Span> = if all_occurrences {
        collect_nodes(parsed.root(), |n| {
            n.kind() == "link"
                && markdown_inline_destination(n, &source)
                    .is_some_and(|(_, d)| d == destination)
        })
        .into_iter()
        .filter_map(|n| markdown_inline_destination(n, &source).map(|(p, _)| p))
        .collect()
    } else {
        vec![parens]
    };
    let targets = if targets.is_empty() {
        vec![parens]
    } else {
        targets
    };

    let mut edits = EditSet::new();
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(*target, format!("[{name}]"), format!("use [{name}]")),
        );
    }

    // The definition goes at the end of the document, beside any others already there.
    let mut text = String::new();
    if !source.is_empty() && !source.ends_with('\n') {
        text.push('\n');
    }
    if !markdown_ends_with_definition(&parsed) {
        text.push('\n');
    }
    text.push_str(&format!("[{name}]: {destination}\n"));
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(source.len(), source.len()),
            text,
            format!("define [{name}]"),
        ),
    );

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: destination,
        edits,
        occurrences: targets.len(),
    })
}

/// The `(destination)` of an inline link: the span including both parentheses, and
/// the text between them (destination plus any title, verbatim).
fn markdown_inline_destination(link: Node<'_>, source: &str) -> Option<(Span, String)> {
    let destination = named_children_of_kind(link, "link_destination")
        .into_iter()
        .next()?;
    let open = source[..destination.start_byte()].rfind('(')?;
    let close = link.end_byte();
    if close == 0 || source.as_bytes().get(close - 1) != Some(&b')') {
        return None;
    }
    Some((
        Span::new(open, close),
        source[open + 1..close - 1].trim().to_string(),
    ))
}

/// Does the document already end with a link reference definition?
fn markdown_ends_with_definition(parsed: &Parsed) -> bool {
    let root = parsed.root();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .last()
        .is_some_and(|n| n.kind() == "link_reference_definition")
}

// ------------------------------------------------------- Helm named template

/// Extract a region of a Helm template into a named template in `_helpers.tpl`.
///
/// The selected bytes move verbatim — a Helm template is text, and reformatting it
/// would change the rendered output. The region is widened to whole lines because a
/// named template is included on a line of its own.
fn helm_named_template(file: &Path, span: Span, name: &str) -> Result<ExtractFunctionPlan> {
    if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(invalid(
            name,
            "a Helm template name may not be empty or contain whitespace or a quote",
        ));
    }

    // The include name is `<chart>.<template>` by convention, and a chart's templates
    // share one flat namespace across every chart in a release. Guessing the chart
    // name would produce an include that silently renders nothing.
    let chart_root = helm_chart_root(file).ok_or_else(|| {
        anyhow::anyhow!(
            "no Chart.yaml above {}, so the chart name is unknown. A named template is \
             addressed as `<chart>.<name>` across every chart in a release, and an \
             include under the wrong name renders empty rather than failing",
            file.display()
        )
    })?;
    let chart_name = helm_chart_name(&chart_root).ok_or_else(|| {
        anyhow::anyhow!(
            "{} has no top-level `name:` key, so the chart name is unknown",
            chart_root.join("Chart.yaml").display()
        )
    })?;
    let template_name = if name.contains('.') {
        name.to_string()
    } else {
        format!("{chart_name}.{name}")
    };

    let source = std::fs::read_to_string(file)?;
    let region = whole_lines(&source, span);
    if region.text(&source).trim().is_empty() {
        anyhow::bail!("the selection at bytes {span} is blank; select the lines to extract");
    }
    let body = region.text(&source).to_string();

    let destination = helm_helpers_path(file, &chart_root);
    let existing = std::fs::read_to_string(&destination).unwrap_or_default();
    if existing.contains(&format!("define \"{template_name}\"")) {
        return Err(Refusal::NameCollision {
            existing: template_name,
            file: destination,
        }
        .into());
    }

    let indent = line_indent(&source, region.start);
    let mut definition = format!("{{{{- define \"{template_name}\" -}}}}\n{body}");
    if !definition.ends_with('\n') {
        definition.push('\n');
    }
    definition.push_str("{{- end -}}\n");

    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            region,
            format!("{indent}{{{{ include \"{template_name}\" . }}}}\n"),
            format!("include {template_name}"),
        ),
    );
    edits.add(
        destination,
        Edit::new(
            Span::new(existing.len(), existing.len()),
            format!("{separator}{definition}"),
            format!("define {template_name}"),
        ),
    );

    Ok(ExtractFunctionPlan {
        name: template_name,
        edits,
        parameters: Vec::new(),
        returns: Vec::new(),
        body,
    })
}

/// Widen a selection to the whole lines it touches, trailing newline included.
fn whole_lines(source: &str, span: Span) -> Span {
    let start = source[..span.start.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end_search = span.end.max(start).min(source.len());
    // A selection that already ends on a line boundary covers whole lines as it is;
    // widening further would swallow the line after it.
    if end_search > start && source[..end_search].ends_with('\n') {
        return Span::new(start, end_search);
    }
    let end = match source[end_search..].find('\n') {
        Some(i) => end_search + i + 1,
        None => source.len(),
    };
    Span::new(start, end)
}

/// The chart directory above `file`: the nearest ancestor holding a `Chart.yaml`.
fn helm_chart_root(file: &Path) -> Option<std::path::PathBuf> {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current.join("Chart.yaml").exists() || current.join("chart.yaml").exists() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// The chart's `name:` from its Chart.yaml.
fn helm_chart_name(chart_root: &Path) -> Option<String> {
    let path = if chart_root.join("Chart.yaml").exists() {
        chart_root.join("Chart.yaml")
    } else {
        chart_root.join("chart.yaml")
    };
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("name:") {
            let value = value.trim().trim_matches(['"', '\'']);
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Where the chart's shared templates live: `<chart>/templates/_helpers.tpl`.
fn helm_helpers_path(file: &Path, chart_root: &Path) -> std::path::PathBuf {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("templates"))
        {
            return current.join("_helpers.tpl");
        }
        if current == chart_root {
            break;
        }
        dir = current.parent();
    }
    chart_root.join("templates").join("_helpers.tpl")
}
