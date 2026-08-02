//! Inline a variable: replace its uses with its value and remove the binding.
//!
//! Inlining is only safe when the answer is provably the same afterwards, so the
//! preconditions are checked and refused rather than assumed: the binding must be
//! assigned exactly once, every use must resolve to it, and no name inside its value
//! may mean something different at a use site (PLAN.md D8).

use super::Refusal;
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Confidence, SymbolId, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::path::PathBuf;
use tree_sitter::Node;

/// An inline worked out but not applied.
#[derive(Debug)]
pub struct InlinePlan {
    pub name: String,
    /// The value substituted at each use site.
    pub value: String,
    pub edits: EditSet,
    /// Number of use sites rewritten.
    pub use_sites: usize,
}

/// Inline the variable `symbol`.
pub fn variable(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // The config languages name values through their own constructs; each has its own
    // inliner, mirroring the extraction that produced it.
    match (sym.language, sym.kind) {
        (Language::Hcl, SymbolKind::Variable) => return hcl_local(index, symbol),
        (Language::Yaml | Language::Helm, SymbolKind::Anchor) => {
            return yaml_anchor(index, symbol)
        }
        (Language::Css | Language::Scss, SymbolKind::Property) => {
            return css_custom_property(index, symbol)
        }
        (Language::Markdown, SymbolKind::LinkDef) => return markdown_link_definition(index, symbol),
        _ => {}
    }

    if !matches!(sym.kind, SymbolKind::Variable | SymbolKind::Constant) {
        anyhow::bail!(
            "'{}' is a {}; only variables and constants can be inlined",
            sym.name,
            sym.kind.as_str()
        );
    }

    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    // The value bound to the definition.
    let node = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
        .ok_or_else(|| anyhow::anyhow!("could not locate the binding"))?;
    let value = node
        .child_by_field_name("value")
        .or_else(|| node.child_by_field_name("right"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "'{}' has no initialiser, so there is nothing to inline",
                sym.name
            )
        })?;
    let value_span = Span::from(value);
    let value_text = value_span.text(&source).to_string();

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "'{}' has no uses; inlining would only delete it — use `fr delete` if that is the intent",
            sym.name
        );
    }

    // Every use must be provably this binding.
    for reference in &references {
        if !reference.confidence.is_safe_to_rewrite() {
            return Err(Refusal::TooWeak {
                confidence: reference.confidence,
                detail: format!(
                    "a use of '{}' at {}:{} did not resolve conclusively",
                    sym.name,
                    reference.file.display(),
                    LineIndex::new(&source)
                        .line_col(reference.span.start, &source)
                        .line
                ),
            }
            .into());
        }
    }

    // Reassignment means the value differs per use, so a single substitution is wrong.
    if let Some(second) = other_assignment(index, symbol, &source, &parsed, sym.name.as_str()) {
        let pos = LineIndex::new(&source).line_col(second, &source);
        anyhow::bail!(
            "'{}' is assigned again at line {}; inlining would change behaviour",
            sym.name,
            pos.line
        );
    }

    // A name inside the value must mean the same thing at every use site, or the
    // substituted expression would silently bind to something else.
    if let Some(captured) = shadowed_name(index, &value_span, &references, &sym.file) {
        return Err(Refusal::NameCollision {
            existing: captured,
            file: sym.file.clone(),
        }
        .into());
    }

    let mut edits = EditSet::new();
    for reference in &references {
        edits.add(
            reference.file.clone(),
            Edit::new(
                reference.span,
                value_text.clone(),
                format!("inline {}", sym.name),
            ),
        );
    }

    // Remove the binding, taking its whole line when nothing else is on it.
    let line = full_line_span(&source, sym.full_span.start);
    let removal = if line.text(&source).trim() == sym.full_span.text(&source).trim() {
        line
    } else {
        sym.full_span
    };
    edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("remove binding of {}", sym.name)),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: value_text,
        edits,
        use_sites: references.len(),
    })
}

/// A later assignment to the same name, if any.
fn other_assignment(
    index: &Index,
    symbol: SymbolId,
    source: &str,
    parsed: &crate::parse::Parsed,
    name: &str,
) -> Option<usize> {
    let sym = index.symbol(symbol)?;
    // Another definition of the same name in the same file is a rebinding.
    let rebound = index
        .find_symbols(name, Some(&sym.file))
        .into_iter()
        .find(|s| s.id != symbol && s.full_span.start > sym.full_span.end)
        .map(|s| s.name_span.start);
    if rebound.is_some() {
        return rebound;
    }

    // An assignment expression whose left side is this name.
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        if node.kind().contains("assignment") {
            if let Some(left) = node.child_by_field_name("left") {
                let span = Span::from(left);
                if span.text(source) == name && span.start != sym.name_span.start {
                    return Some(span.start);
                }
            }
        }
        stack.extend(node.children(&mut cursor));
    }
    None
}

/// A name used inside the value that is redefined between the binding and a use.
fn shadowed_name(
    index: &Index,
    value_span: &Span,
    references: &[&crate::model::Reference],
    file: &PathBuf,
) -> Option<String> {
    let info = index.file(file)?;
    let inner: Vec<&crate::model::Reference> = info
        .references
        .iter()
        .map(|i| &index.references[*i])
        .filter(|r| value_span.contains(r.span))
        .collect();

    for name_ref in inner {
        let target = name_ref.target?;
        for use_site in references {
            if use_site.file != *file {
                // The value would move to another file, where its names may mean
                // anything at all.
                return Some(name_ref.name.clone());
            }
            // What does this name resolve to at the use site?
            let at_use = index.definition_at(&use_site.file, use_site.span.start);
            let _ = at_use;
            let resolved_here = index
                .references
                .iter()
                .filter(|r| r.file == use_site.file && r.name == name_ref.name)
                .filter(|r| r.span.start > use_site.span.start.saturating_sub(200))
                .find_map(|r| r.target);
            if let Some(other) = resolved_here {
                if other != target {
                    return Some(name_ref.name.clone());
                }
            }
        }
    }
    None
}

/// Confidence of the weakest use site, for reporting.
pub fn weakest_use(index: &Index, symbol: SymbolId) -> Option<Confidence> {
    index
        .references_to(symbol)
        .iter()
        .map(|r| r.confidence)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{scan, ScanOptions};
    use std::path::Path;

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

    fn apply(plan: &InlinePlan, path: &Path) -> String {
        let original = std::fs::read_to_string(path).unwrap();
        apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
    }

    fn var_at(index: &Index, path: &Path, needle_offset: usize) -> SymbolId {
        index
            .definition_at(path, needle_offset)
            .expect("a definition at that offset")
            .id
    }

    #[test]
    fn inlines_a_single_use_and_removes_the_binding() {
        let src = "fn f() {\n    let x = compute();\n    use_it(x);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let x").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        assert_eq!(plan.use_sites, 1);
        assert_eq!(apply(&plan, &path), "fn f() {\n    use_it(compute());\n}\n");
    }

    #[test]
    fn inlines_every_use() {
        let src = "fn f() {\n    let n = a + b;\n    p(n);\n    q(n);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let n").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        assert_eq!(plan.use_sites, 2);
        assert_eq!(apply(&plan, &path), "fn f() {\n    p(a + b);\n    q(a + b);\n}\n");
    }

    #[test]
    fn surrounding_code_is_untouched() {
        let src = "fn f() {\n    // keep me\n    let x = 1;\n    g(x);\n    // and me\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let x").unwrap() + 4);

        let out = apply(&variable(&index, id).unwrap(), &path);
        assert!(out.contains("// keep me"), "got:\n{out}");
        assert!(out.contains("// and me"), "got:\n{out}");
        assert!(out.contains("g(1);"), "got:\n{out}");
    }

    #[test]
    fn refuses_when_the_variable_is_reassigned() {
        let src = "fn f() {\n    let mut x = 1;\n    x = 2;\n    g(x);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let mut x").unwrap() + 8);

        let err = variable(&index, id).unwrap_err().to_string();
        assert!(err.contains("assigned again"), "got: {err}");
    }

    #[test]
    fn refuses_a_binding_with_no_uses() {
        let src = "fn f() {\n    let unused = 1;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let unused").unwrap() + 4);

        let err = variable(&index, id).unwrap_err().to_string();
        assert!(err.contains("no uses"), "got: {err}");
    }

    #[test]
    fn refuses_non_variables() {
        let src = "fn helper() {}\nfn f() { helper(); }\n";
        // The temp dir is bound so it outlives the index that points into it.
        let (_tmp, index) = workspace(&[("a.rs", src)]);
        let id = index.find_symbols("helper", None)[0].id;

        let err = variable(&index, id).unwrap_err().to_string();
        assert!(err.contains("only variables"), "got: {err}");
    }

    #[test]
    fn the_result_still_parses() {
        let src = "fn f() {\n    let x = a + b;\n    g(x);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let x").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        let outcomes =
            crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn extract_then_inline_returns_the_original() {
        // The two refactorings are inverses; round-tripping must restore the file.
        let src = "fn f() {\n    let total = price * quantity + 10;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let start = src.find("price * quantity").unwrap();
        let extracted = super::super::extract::variable(
            &index,
            &path,
            Span::new(start, start + "price * quantity".len()),
            "subtotal",
            false,
        )
        .unwrap();
        let after_extract =
            apply_to_string(src, extracted.edits.edits_for(&path).unwrap()).unwrap();
        std::fs::write(&path, &after_extract).unwrap();

        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        let index2 = Index::build_from_scan(&scanned).unwrap();
        let id = var_at(&index2, &path, after_extract.find("let subtotal").unwrap() + 4);

        let inlined = variable(&index2, id).unwrap();
        let after_inline =
            apply_to_string(&after_extract, inlined.edits.edits_for(&path).unwrap()).unwrap();

        assert_eq!(after_inline, src);
    }

    #[test]
    fn works_for_python() {
        let src = "def f():\n    x = a + b\n    g(x)\n";
        let (tmp, index) = workspace(&[("a.py", src)]);
        let path = tmp.path().join("a.py");
        let id = var_at(&index, &path, src.find("x = a + b").unwrap());

        let plan = variable(&index, id).unwrap();
        assert_eq!(apply(&plan, &path), "def f():\n    g(a + b)\n");
    }
}

// ------------------------------------------------------------------- inline call

/// An inlined call worked out but not applied.
#[derive(Debug)]
pub struct InlineCallPlan {
    pub function: String,
    /// The expression substituted at the call site.
    pub expansion: String,
    pub edits: EditSet,
}

/// Replace the call at `offset` with the callee's body.
///
/// Only calls whose result is provably identical are inlined. gopls's inliner exists
/// to preserve evaluation order, effects and shadowing across arbitrary bodies; this
/// one takes the conservative half of that problem — a single-expression callee whose
/// arguments cannot be duplicated unsafely — and refuses everything else by name.
pub fn call(index: &Index, file: &std::path::Path, offset: usize) -> Result<InlineCallPlan> {
    let reference = index
        .reference_at(file, offset)
        .ok_or_else(|| anyhow::anyhow!("no call at that position"))?;
    if reference.kind != crate::model::ReferenceKind::Call {
        anyhow::bail!("'{}' at that position is not a call", reference.name);
    }
    if !reference.confidence.is_safe_to_rewrite() {
        return Err(Refusal::TooWeak {
            confidence: reference.confidence,
            detail: format!("the callee of '{}' was not resolved conclusively", reference.name),
        }
        .into());
    }

    let callee = reference
        .target
        .and_then(|t| index.symbol(t))
        .ok_or_else(|| anyhow::anyhow!("'{}' does not resolve to a definition", reference.name))?;

    // Inlining a function into itself would not terminate.
    if callee.file == *file && callee.full_span.contains_offset(offset) {
        anyhow::bail!("'{}' calls itself here; inlining would not terminate", callee.name);
    }

    let callee_source = std::fs::read_to_string(&callee.file)?;
    let callee_parsed = Parsers::new().parse(callee.language, &callee_source)?;
    let declaration = callee_parsed
        .root()
        .descendant_for_byte_range(callee.full_span.start, callee.full_span.end)
        .ok_or_else(|| anyhow::anyhow!("could not locate the definition of '{}'", callee.name))?;

    let body_expression = single_expression_body(declaration, &callee_source).ok_or_else(|| {
        anyhow::anyhow!(
            "'{}' is not a single-expression function; inlining a multi-statement body \
             would have to preserve evaluation order and shadowing, which is not supported",
            callee.name
        )
    })?;

    // Pair parameters with the arguments written at this call site.
    let caller_source = std::fs::read_to_string(file)?;
    let caller_parsed = Parsers::new().parse(reference.language, &caller_source)?;
    let call_node = enclosing_call(&caller_parsed, reference.span)
        .ok_or_else(|| anyhow::anyhow!("could not locate the call expression"))?;
    let call_span = Span::from(call_node);

    let parameters = parameter_names(declaration, &callee_source);
    let arguments = argument_texts(call_node, &caller_source);
    if parameters.len() != arguments.len() {
        anyhow::bail!(
            "'{}' takes {} parameter(s) but the call passes {}; inlining would change \
             the meaning",
            callee.name,
            parameters.len(),
            arguments.len()
        );
    }

    // Substituting an argument more than once would evaluate it more than once.
    let body_text = body_expression.text(&callee_source);
    for (parameter, argument) in parameters.iter().zip(arguments.iter()) {
        let uses = count_word(body_text, parameter);
        if uses > 1 && !is_duplicable(argument) {
            anyhow::bail!(
                "'{parameter}' is used {uses} times in the body and the argument \
                 `{argument}` is not a simple value; inlining would evaluate it more \
                 than once"
            );
        }
    }

    let mut expansion = substitute_words(body_text, &parameters, &arguments);
    // The body was an expression in its own right; parenthesise it so it keeps its
    // meaning inside whatever expression the call sat in.
    if needs_parentheses(&expansion) {
        expansion = format!("({expansion})");
    }

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            call_span,
            expansion.clone(),
            format!("inline call to {}", callee.name),
        ),
    );

    Ok(InlineCallPlan {
        function: callee.name.clone(),
        expansion,
        edits,
    })
}

/// The single expression a function body evaluates to, if that is all it does.
fn single_expression_body<'a>(
    declaration: tree_sitter::Node<'a>,
    source: &str,
) -> Option<Span> {
    let body = declaration.child_by_field_name("body")?;
    // Some grammars interpose a list node between a block and its statements
    // (tree-sitter-go wraps them in `statement_list`); descend through it, or the
    // wrapper itself looks like the single statement.
    let mut block = body;
    loop {
        let mut cursor = block.walk();
        let children: Vec<tree_sitter::Node> = block
            .named_children(&mut cursor)
            .filter(|n| !n.kind().contains("comment"))
            .collect();
        match children.as_slice() {
            [only] if only.kind().ends_with("_list") => block = *only,
            _ => break,
        }
    }

    let mut cursor = block.walk();
    let statements: Vec<tree_sitter::Node> = block
        .named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
        .collect();
    if statements.len() != 1 {
        return None;
    }

    // Peel the wrappers grammars put between a block and the value it yields:
    // an expression statement, a return statement, or a return expression nested
    // inside one (Zig spells it `expression_statement > return_expression`). Doing
    // this in a loop rather than a fixed order keeps the `return` keyword from being
    // inlined along with the value.
    let mut node = statements[0];
    for _ in 0..4 {
        let kind = node.kind();

        let is_return = kind.contains("return") || {
            let mut walker = node.walk();
            let found = node.children(&mut walker).any(|c| c.kind() == "return");
            found
        };
        let is_wrapper = kind.contains("expression_statement");

        if !is_return && !is_wrapper {
            break;
        }
        // A statement kind we do not recognise cannot be reduced to one expression.
        let mut inner = node.walk();
        node = node.named_children(&mut inner).next()?;
    }

    // Anything still statement-shaped is not a single expression.
    let kind = node.kind();
    if kind.contains("statement") || kind.contains("declaration") {
        return None;
    }
    let _ = source;
    Some(Span::from(node))
}

/// Parameter names of a declaration, in order.
fn parameter_names(declaration: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    // Most grammars expose the list through a `parameters` field; those that do not
    // still name the node itself for what it holds.
    let list = declaration.child_by_field_name("parameters").or_else(|| {
        let mut cursor = declaration.walk();
        let found = declaration
            .named_children(&mut cursor)
            .find(|c| c.kind().contains("parameter"));
        found
    });
    let Some(list) = list else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
        .map(|n| {
            // A typed parameter names itself in its first identifier child.
            let name = n
                .child_by_field_name("pattern")
                .or_else(|| n.child_by_field_name("name"))
                .unwrap_or(n);
            Span::from(name).text(source).trim().to_string()
        })
        .collect()
}

/// Argument texts of a call, in order.
fn argument_texts(call: tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    // As with parameters, not every grammar exposes an `arguments` field.
    let list = call.child_by_field_name("arguments").or_else(|| {
        let mut cursor = call.walk();
        let found = call
            .named_children(&mut cursor)
            .find(|c| c.kind().contains("argument"));
        found
    });
    // Some grammars have no argument-list node at all: tree-sitter-zig hangs the
    // arguments directly off the call, after the callee.
    let Some(list) = list else {
        let mut cursor = call.walk();
        let children: Vec<tree_sitter::Node> = call
            .named_children(&mut cursor)
            .filter(|n| !n.kind().contains("comment"))
            .collect();
        return children
            .into_iter()
            .skip(1)
            .map(|n| Span::from(n).text(source).trim().to_string())
            .collect();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
        .map(|n| Span::from(n).text(source).trim().to_string())
        .collect()
}

/// The call expression containing `span`.
fn enclosing_call<'a>(parsed: &'a crate::parse::Parsed, span: Span) -> Option<tree_sitter::Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..8 {
        if node.kind().contains("call") {
            return Some(node);
        }
        node = node.parent()?;
    }
    None
}

/// May this argument be substituted more than once without changing behaviour?
fn is_duplicable(argument: &str) -> bool {
    // A bare name or a literal has no effects and costs nothing to repeat.
    let trimmed = argument.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '"')
        && !trimmed.contains('(')
}

/// Count whole-word occurrences of `word`.
fn count_word(haystack: &str, word: &str) -> usize {
    haystack
        .match_indices(word)
        .filter(|(i, _)| word_boundary(haystack, *i, word.len()))
        .count()
}

/// Replace whole-word occurrences of each name with its argument.
fn substitute_words(body: &str, names: &[String], values: &[String]) -> String {
    let mut out = body.to_string();
    for (name, value) in names.iter().zip(values.iter()) {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        let mut base = 0usize;
        while let Some(found) = rest.find(name.as_str()) {
            let absolute = base + found;
            if word_boundary(&out, absolute, name.len()) {
                result.push_str(&rest[..found]);
                result.push_str(value);
            } else {
                result.push_str(&rest[..found + name.len()]);
            }
            rest = &rest[found + name.len()..];
            base = absolute + name.len();
        }
        result.push_str(rest);
        out = result;
    }
    out
}

fn word_boundary(haystack: &str, offset: usize, len: usize) -> bool {
    let before = haystack[..offset]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let after = haystack[offset + len..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    before && after
}

/// Does the expansion need wrapping to survive its new context?
fn needs_parentheses(expansion: &str) -> bool {
    let trimmed = expansion.trim();
    // A single token or an already-bracketed expression is safe as-is.
    if trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return false;
    }
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        return false;
    }
    // An operator that is not already inside brackets could re-associate with the
    // expression the call site sits in.
    needs_grouping(trimmed)
}

// ------------------------------------------------------- config languages
//
// Each of these is the exact inverse of the corresponding extraction: substitute the
// named value at every use site and delete the declaration, including the container
// the extraction created when nothing else is left in it. Everything else in the file
// keeps its bytes.

/// The node whose byte range is exactly, or most closely, the given span.
fn node_covering<'a>(parsed: &'a Parsed, span: Span) -> Option<Node<'a>> {
    parsed.root().descendant_for_byte_range(span.start, span.end)
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

fn named_children_of_kind<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| c.kind() == kind)
        .collect()
}

/// The span to delete for a construct that owns whole lines: the lines it covers,
/// plus the blank lines directly after it.
///
/// This is what makes an extraction reversible — the blank line an extraction wrote
/// to separate its new block from the rest of the file goes away with the block.
fn block_removal_span(source: &str, inner: Span) -> Span {
    let first = full_line_span(source, inner.start);
    let last_offset = inner.end.saturating_sub(1).max(inner.start);
    let last = full_line_span(source, last_offset);

    // Only take whole lines when the construct really has them to itself.
    if !source[first.start..inner.start].trim().is_empty()
        || !source[inner.end.min(source.len())..last.end.min(source.len())]
            .trim()
            .is_empty()
    {
        return inner;
    }

    let mut end = last.end;
    while end < source.len() {
        let line = full_line_span(source, end);
        if line.end <= end || !line.text(source).trim().is_empty() {
            break;
        }
        end = line.end;
    }
    Span::new(first.start, end)
}

/// The span to delete for a construct sharing its line with other code: itself, plus
/// the whitespace directly before it so no double space is left behind.
fn tight_removal_span(source: &str, inner: Span) -> Span {
    let line = full_line_span(source, inner.start);
    if source[line.start..inner.start].trim().is_empty()
        && source[inner.end.min(source.len())..line.end.min(source.len())]
            .trim()
            .is_empty()
    {
        return line;
    }
    let mut start = inner.start;
    while start > 0 && matches!(source.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    Span::new(start, inner.end)
}

/// Does substituting this text into a larger expression need parentheses to keep its
/// meaning? True when it contains an operator that could re-associate.
fn needs_grouping(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'+' | b'-' | b'*' | b'/' | b'%' | b'?' | b'<' | b'>' | b'=' | b'&' | b'|'
                    if depth == 0 =>
                {
                    return true;
                }
                _ => {}
            },
        }
        i += 1;
    }
    false
}

// ------------------------------------------------------------ Terraform / HCL

/// Inline a `locals` entry: substitute its expression at every `local.<name>` and
/// delete the entry, taking the `locals` block with it when it becomes empty.
fn hcl_local(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;
    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let node = node_covering(&parsed, sym.full_span)
        .ok_or_else(|| anyhow::anyhow!("could not locate the declaration of '{}'", sym.name))?;
    let attribute = ancestor_of_kind(node, "attribute")
        .filter(|a| Span::from(*a) == sym.full_span)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "'{}' is a `{}` block, not a `locals` entry. A module input is part of the \
                 module's call surface; changing it is a signature change, not an inline",
                sym.name,
                sym.full_span
                    .text(&source)
                    .split_whitespace()
                    .next()
                    .unwrap_or("block")
            )
        })?;
    let locals = ancestor_of_kind(attribute, "block")
        .filter(|b| {
            b.named_child(0)
                .is_some_and(|c| Span::from(c).text(&source) == "locals")
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "'{}' is not declared in a `locals` block, so there is no `local.{}` to \
                 substitute",
                sym.name,
                sym.name
            )
        })?;

    let value = named_children_of_kind(attribute, "expression")
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("'{}' has no value to inline", sym.name))?;
    let value_text = Span::from(value).text(&source).to_string();

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "local.{} has no uses; inlining would only delete it — use `fr delete` if \
             that is the intent",
            sym.name
        );
    }
    for reference in &references {
        if !reference.confidence.is_safe_to_rewrite() {
            return Err(Refusal::TooWeak {
                confidence: reference.confidence,
                detail: format!(
                    "a use of local.{} at {}:{} did not resolve conclusively — the \
                     module declares more than one thing by that name",
                    sym.name,
                    reference.file.display(),
                    LineIndex::new(&source)
                        .line_col(reference.span.start, &source)
                        .line
                ),
            }
            .into());
        }
    }

    // A local belongs to one module, which in Terraform is one directory. A use of the
    // same name from another directory is a different module's business and this tool
    // has no way to tell whether it was meant to be this value.
    let foreign = hcl_foreign_local_uses(index, &sym.file, &sym.name);
    if !foreign.is_empty() {
        anyhow::bail!(
            "`local.{}` is also used in {}, which is a different Terraform module \
             directory. Locals are module-scoped, so those uses cannot be rewritten \
             from here and would be left naming a local that no longer exists",
            sym.name,
            foreign
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let mut edits = EditSet::new();
    // A module spans several files, so each use site is read and parsed in its own.
    let mut per_file: std::collections::HashMap<PathBuf, (String, Parsed)> =
        std::collections::HashMap::new();
    for reference in &references {
        let site = match per_file.entry(reference.file.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let text = std::fs::read_to_string(&reference.file)?;
                let tree = Parsers::new().parse(reference.language, &text)?;
                e.insert((text, tree))
            }
        };
        let (site_source, site_parsed) = (&site.0, &site.1);

        let prefix_start = reference
            .span
            .start
            .checked_sub("local.".len())
            .filter(|start| &site_source[*start..reference.span.start] == "local.")
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "a use of '{}' at {} byte {} is not written as `local.{}`; refusing \
                     to guess what it meant",
                    sym.name,
                    reference.file.display(),
                    reference.span.start,
                    sym.name
                )
            })?;
        let traversal = Span::new(prefix_start, reference.span.end);

        // `local.x.attr` reads an attribute off the local; substituting a value that
        // has no such attribute would produce a configuration that does not evaluate.
        if let Some(next) = site_source[traversal.end..].chars().next() {
            if next == '.' || next == '[' {
                anyhow::bail!(
                    "`local.{}` at byte {} is read further with `{}`; inlining the value \
                     underneath an attribute or index read is not supported",
                    sym.name,
                    traversal.start,
                    next
                );
            }
        }

        // Terraform has no operator precedence rescue: an inlined sum inside a product
        // would bind differently, so it is grouped when it is not the whole expression.
        let enclosing = node_covering(site_parsed, traversal)
            .and_then(|n| ancestor_of_kind(n, "expression"))
            .map(Span::from);
        let replacement = if needs_grouping(&value_text) && enclosing != Some(traversal) {
            format!("({value_text})")
        } else {
            value_text.clone()
        };

        edits.add(
            reference.file.clone(),
            Edit::new(traversal, replacement, format!("inline local.{}", sym.name)),
        );
    }

    // The `locals` block an extraction created goes away with its last entry.
    let siblings = ancestor_of_kind(attribute, "body")
        .map(|body| named_children_of_kind(body, "attribute").len())
        .unwrap_or(1);
    let removal = if siblings <= 1 {
        block_removal_span(&source, Span::from(locals))
    } else {
        tight_removal_span(&source, sym.full_span)
    };
    edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("remove local.{}", sym.name)),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: value_text,
        edits,
        use_sites: references.len(),
    })
}

/// `.tf` files in other directories that spell `local.<name>`.
fn hcl_foreign_local_uses(index: &Index, file: &std::path::Path, name: &str) -> Vec<PathBuf> {
    let dir = file.parent();
    let mut sources: std::collections::HashMap<PathBuf, String> = std::collections::HashMap::new();
    let mut out: Vec<PathBuf> = Vec::new();

    for reference in &index.references {
        if reference.language != Language::Hcl
            || reference.name != name
            || reference.file.parent() == dir
        {
            continue;
        }
        let text = sources
            .entry(reference.file.clone())
            .or_insert_with(|| std::fs::read_to_string(&reference.file).unwrap_or_default());
        if reference.span.start >= "local.".len()
            && text.get(reference.span.start - "local.".len()..reference.span.start)
                == Some("local.")
            && !out.contains(&reference.file)
        {
            out.push(reference.file.clone());
        }
    }
    out.sort();
    out
}

// ------------------------------------------------------------------ Helm / YAML

/// Inline a YAML anchor: substitute its value at every `*alias` and drop the `&name`.
fn yaml_anchor(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;
    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let node = node_covering(&parsed, sym.full_span)
        .ok_or_else(|| anyhow::anyhow!("could not locate the anchor '&{}'", sym.name))?;
    let anchor = named_children_of_kind(node, "anchor")
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("'&{}' is not an anchor node", sym.name))?;

    let mut value_start = anchor.end_byte();
    while value_start < sym.full_span.end
        && source.as_bytes()[value_start].is_ascii_whitespace()
    {
        value_start += 1;
    }
    if value_start >= sym.full_span.end {
        anyhow::bail!(
            "'&{}' anchors an empty node, so there is nothing to inline",
            sym.name
        );
    }
    let value_text = source[value_start..sym.full_span.end].trim_end().to_string();
    if value_text.contains('\n') {
        anyhow::bail!(
            "'&{}' anchors a block collection spanning several lines. Substituting it \
             at an alias would have to re-indent the spliced lines to each alias's \
             depth, which would not be a byte-preserving edit; only anchors on a \
             single-line value can be inlined",
            sym.name
        );
    }

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "'&{}' has no aliases; inlining would only delete the anchor",
            sym.name
        );
    }
    if let Some(elsewhere) = references.iter().find(|r| r.file != sym.file) {
        anyhow::bail!(
            "'*{}' is used in {}, but a YAML anchor is only visible inside the document \
             that declares it; that use names something else",
            sym.name,
            elsewhere.file.display()
        );
    }

    let mut edits = EditSet::new();
    for reference in &references {
        let start = reference
            .span
            .start
            .checked_sub(1)
            .filter(|s| source.as_bytes()[*s] == b'*')
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "a use of '{}' at byte {} is not written as an alias",
                    sym.name,
                    reference.span.start
                )
            })?;
        // `<<: *name` splices a mapping in; a scalar cannot be merged.
        let line = full_line_span(&source, start);
        if line.text(&source).trim_start().starts_with("<<:") {
            anyhow::bail!(
                "'*{}' is used as a merge key (`<<:`), which requires a mapping; the \
                 anchored value is a scalar, so the merge cannot be inlined",
                sym.name
            );
        }
        edits.add(
            reference.file.clone(),
            Edit::new(
                Span::new(start, reference.span.end),
                value_text.clone(),
                format!("inline *{}", sym.name),
            ),
        );
    }

    edits.add(
        sym.file.clone(),
        Edit::new(
            Span::new(sym.full_span.start, value_start),
            "",
            format!("remove anchor &{}", sym.name),
        ),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: value_text,
        edits,
        use_sites: references.len(),
    })
}

// ---------------------------------------------------------------------- CSS/SCSS

/// Inline a custom property: substitute its value at every `var(--name)` and delete
/// the declaration, taking an emptied rule with it.
fn css_custom_property(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // A custom property redeclared per scope has a different value at each use site;
    // one substitution would be wrong at all but one of them.
    let group = index.definition_group(symbol);
    if group.len() > 1 {
        let sites: Vec<String> = group
            .iter()
            .filter_map(|id| index.symbol(*id))
            .map(|s| s.file.display().to_string())
            .collect();
        anyhow::bail!(
            "'{}' is declared {} times ({}); which declaration wins at a given use site \
             is a cascade question, so inlining one value everywhere would change meaning",
            sym.name,
            group.len(),
            sites.join(", ")
        );
    }

    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} does not parse cleanly under the CSS grammar{}; the declaration cannot \
             be located reliably",
            sym.file.display(),
            if sym.language == Language::Scss {
                " — which is the only grammar available for SCSS here, so SCSS-only \
                 syntax such as `$variables`, `@mixin` and `@use` is not understood"
            } else {
                ""
            }
        );
    }

    let declaration = node_covering(&parsed, sym.full_span)
        .and_then(|n| ancestor_of_kind(n, "declaration"))
        .ok_or_else(|| anyhow::anyhow!("could not locate the declaration of '{}'", sym.name))?;
    let value_span = css_declaration_value_span(declaration).ok_or_else(|| {
        anyhow::anyhow!("'{}' has no value, so there is nothing to inline", sym.name)
    })?;
    let value_text = value_span.text(&source).to_string();

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "'{}' has no `var()` uses; inlining would only delete it — use `fr delete` \
             if that is the intent",
            sym.name
        );
    }

    let mut edits = EditSet::new();
    let mut parsed_cache: std::collections::HashMap<PathBuf, (String, Parsed)> =
        std::collections::HashMap::new();
    for reference in &references {
        let entry = match parsed_cache.entry(reference.file.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let text = std::fs::read_to_string(&reference.file)?;
                let tree = Parsers::new().parse(reference.language, &text)?;
                e.insert((text, tree))
            }
        };
        let call = node_covering(&entry.1, reference.span)
            .and_then(|n| ancestor_of_kind(n, "call_expression"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "a use of '{}' at {}:{} is not inside a `var()` call",
                    sym.name,
                    reference.file.display(),
                    reference.span.start
                )
            })?;
        edits.add(
            reference.file.clone(),
            Edit::new(
                Span::from(call),
                value_text.clone(),
                format!("inline {}", sym.name),
            ),
        );
    }

    // A `:root` rule an extraction created goes away with its last declaration.
    let block = declaration.parent().filter(|p| p.kind() == "block");
    let siblings = block
        .map(|b| {
            let mut cursor = b.walk();
            b.named_children(&mut cursor)
                .filter(|c| !c.kind().contains("comment"))
                .count()
        })
        .unwrap_or(2);
    let removal = if siblings <= 1 {
        match block.and_then(|b| b.parent()) {
            Some(rule) => block_removal_span(&source, Span::from(rule)),
            None => tight_removal_span(&source, sym.full_span),
        }
    } else {
        tight_removal_span(&source, sym.full_span)
    };
    edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("remove {}", sym.name)),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: value_text,
        edits,
        use_sites: references.len(),
    })
}

/// The value part of a declaration: everything after the property name and the colon,
/// with the trailing semicolon left out.
fn css_declaration_value_span(declaration: Node<'_>) -> Option<Span> {
    let mut cursor = declaration.walk();
    let values: Vec<Node> = declaration
        .named_children(&mut cursor)
        .filter(|c| c.kind() != "property_name" && !c.kind().contains("comment"))
        .collect();
    let first = values.first()?;
    let last = values.last()?;
    Some(Span::new(first.start_byte(), last.end_byte()))
}

// ---------------------------------------------------------------------- Markdown

/// Inline a link reference definition: rewrite every reference link as an inline link
/// and delete the definition.
fn markdown_link_definition(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;
    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let definition = node_covering(&parsed, sym.full_span)
        .and_then(|n| ancestor_of_kind(n, "link_reference_definition"))
        .ok_or_else(|| anyhow::anyhow!("could not locate the definition of '[{}]'", sym.name))?;
    let destination_node = named_children_of_kind(definition, "link_destination")
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("'[{}]' has no destination", sym.name))?;
    // Anything after the destination is the optional title, which belongs with it.
    let destination = source[destination_node.start_byte()..definition.end_byte()]
        .trim()
        .to_string();

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "'[{}]' has no reference links; inlining would only delete it — use \
             `fr delete` if that is the intent",
            sym.name
        );
    }
    if let Some(elsewhere) = references.iter().find(|r| r.file != sym.file) {
        anyhow::bail!(
            "'[{}]' is used in {}, but a link reference definition only applies to the \
             document that contains it; that use resolves to nothing there",
            sym.name,
            elsewhere.file.display()
        );
    }

    let mut edits = EditSet::new();
    for reference in &references {
        let link = node_covering(&parsed, reference.span)
            .and_then(|n| ancestor_of_kind(n, "link"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "a use of '[{}]' at byte {} is not a link",
                    sym.name,
                    reference.span.start
                )
            })?;
        let text = named_children_of_kind(link, "link_text")
            .into_iter()
            .next()
            .map(|t| Span::from(t).text(&source).to_string())
            .unwrap_or_else(|| sym.name.clone());
        edits.add(
            reference.file.clone(),
            Edit::new(
                Span::from(link),
                format!("[{text}]({destination})"),
                format!("inline [{}]", sym.name),
            ),
        );
    }

    edits.add(
        sym.file.clone(),
        Edit::new(
            markdown_definition_removal(&source, Span::from(definition)),
            "",
            format!("remove [{}]", sym.name),
        ),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: destination,
        edits,
        use_sites: references.len(),
    })
}

/// The lines a link reference definition occupies, plus the blank line before it when
/// it is the last thing in the document — the separator an extraction wrote.
fn markdown_definition_removal(source: &str, definition: Span) -> Span {
    let line = full_line_span(source, definition.start);
    let mut start = line.start;
    if line.end >= source.len() && start > 0 {
        let previous = full_line_span(source, start - 1);
        if previous.text(source).trim().is_empty() {
            start = previous.start;
        }
    }
    Span::new(start, line.end)
}
