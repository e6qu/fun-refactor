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
