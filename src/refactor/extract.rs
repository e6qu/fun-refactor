//! Extract a subexpression into a named binding.

use super::Refusal;
use crate::edit::{full_line_span, line_indent, Edit, EditSet};
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
    /// How many occurrences of the expression this replaced.
    pub occurrences: usize,
}

/// Extract the expression covering `span` into a binding called `name`.
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
    crate::capabilities::record(crate::capabilities::Capability::ExtractVariable, language);

    // The config languages have no bindings, so each one gets the construct that plays the same
    // role.
    match language {
        Language::Hcl => return hcl_local(index, file, span, name, all_occurrences),
        Language::Yaml | Language::Helm => {
            return yaml_anchor(index, file, span, name, all_occurrences)
        }
        Language::Css | Language::Scss | Language::Sass => {
            return css_custom_property(index, file, span, name, all_occurrences)
        }
        Language::Markdown => {
            return markdown_link_definition(index, file, span, name, all_occurrences)
        }
        // Bash has bindings, but neither the generic statement walk nor the generic
        // reference spelling survives contact with shell quoting, so it gets its own.
        Language::Bash => return bash_variable(index, file, span, name, all_occurrences),
        // XML's only binding form is the internal DTD entity.
        Language::Xml => return xml_entity(file, span, name, all_occurrences),
        _ => {}
    }

    if !supports_imperative_extract(language) {
        return Err(Refusal::Unsupported {
            operation: "extract variable".into(),
            language,
            because: "",
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    let expr = expression_at(&parsed, span).ok_or_else(|| {
        anyhow::anyhow!(
            "no expression at bytes {span} in {}; select a complete expression",
            file.display()
        )
    })?;
    let expr_span = Span::from(expr);
    let expr_text = expr_span.text(&source).to_string();

    // A statement is not a value.
    if expr.kind().contains("statement") || expr.kind().contains("declaration") {
        anyhow::bail!(
            "the selection is a {}, and a binding holds an expression; \
             `--function` extracts statements into a function",
            expr.kind().replace('_', " ")
        );
    }

    // Extracting a bare name would only alias it, which is never the intent.
    if expr.child_count() == 0 && expr.kind().contains("identifier") {
        anyhow::bail!("'{expr_text}' is already a name; extracting it would only create an alias");
    }

    // An expression that *is* its statement has nothing left behind it.
    if expr
        .parent()
        .is_some_and(|p| p.kind().contains("expression_statement") && p.named_child_count() == 1)
    {
        anyhow::bail!(
            "`{expr_text}` is the whole of its statement; extracting it would leave a \
             statement that only names the binding"
        );
    }

    // A name already defined in this file would collide or shadow.
    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let statement = enclosing_statement(expr).ok_or_else(|| {
        anyhow::anyhow!("could not find a statement to insert the binding before")
    })?;
    let statement_span = Span::from(statement);

    // Every name the expression uses has to mean the same thing where the binding goes.
    if let Some(unreachable) =
        a_name_that_does_not_reach(index, info, expr_span, statement_span.start)
    {
        anyhow::bail!(
            "`{unreachable}` is introduced between the binding's position and this \
             expression, so the binding would not compile"
        );
    }

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
pub(crate) fn supports_imperative_extract(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Java
    )
}

/// Can a value be extracted into a named binding in this language?
pub fn supports_extract(language: Language) -> bool {
    supports_imperative_extract(language)
        || matches!(
            language,
            Language::Hcl
                | Language::Yaml
                | Language::Helm
                | Language::Css
                | Language::Scss
                | Language::Sass
                | Language::Markdown
                | Language::Bash
                | Language::Xml
        )
}

/// Spell a binding each language's way.
fn render_binding(language: Language, name: &str, value: &str) -> String {
    match language {
        Language::Rust => format!("let {name} = {value};"),
        Language::Go => format!("{name} := {value}"),
        Language::Zig => format!("const {name} = {value};"),
        Language::TypeScript | Language::Tsx => format!("const {name} = {value};"),
        Language::Python => format!("{name} = {value}"),
        // `var` infers, the way `let` and `:=` do; a spelled type would need
        // inference this tool does not do.
        Language::Java => format!("var {name} = {value};"),
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
fn a_name_that_does_not_reach(
    index: &Index,
    info: &crate::index::FileInfo,
    expression: Span,
    at: usize,
) -> Option<String> {
    let reachable: std::collections::HashSet<crate::model::ScopeId> = info
        .scope_at(at)
        .map(|scope| info.scope_chain(scope))
        .unwrap_or_default()
        .into_iter()
        .collect();

    info.references
        .iter()
        .map(|i| &index.references[*i])
        .find(|reference| {
            expression.contains(reference.span) && !reachable.contains(&reference.scope)
        })
        .map(|reference| reference.name.clone())
}

fn enclosing_statement(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        if crate::refactor::is_statement_container(parent.kind()) {
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

    use crate::testing::workspace;

    fn apply(plan: &ExtractPlan, path: &Path) -> String {
        let original = crate::vfs::read_to_string(path).unwrap();
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
        let err = variable(
            &index,
            &path,
            Span::new(start, start + 5),
            "existing",
            false,
        )
        .unwrap_err();
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
        // An extraction must not break the file.
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
    /// The region assigns to this name, so the new function's copy changes and the changed
    /// value has to travel back.
    pub mutated: bool,
    /// What the call passes, where that is not the parameter's own name.
    pub argument: Option<String>,
}

impl Parameter {
    /// The expression the call site hands to this parameter.
    fn argument(&self) -> String {
        self.argument.clone().unwrap_or_else(|| self.name.clone())
    }
}

impl Parameter {
    fn render(&self, language: Language) -> String {
        let mutability = match (language, self.mutated) {
            (Language::Rust, true) => "mut ",
            _ => "",
        };
        match (language, &self.type_annotation) {
            (Language::Python, _) => self.name.clone(),
            // Go writes the type after the name with no colon between them;
            // Java writes it before.
            (Language::Go, Some(ty)) => format!("{} {ty}", self.name),
            (Language::Java, Some(ty)) => format!("{ty} {}", self.name),
            (_, Some(ty)) => format!("{mutability}{}: {ty}", self.name),
            (_, None) => format!("{mutability}{}", self.name),
        }
    }
}

/// Extract the statements covering `span` into a new function called `name`.
pub fn function(index: &Index, file: &Path, span: Span, name: &str) -> Result<ExtractFunctionPlan> {
    if let Some(language) = index.file(file).map(|i| i.language) {
        crate::capabilities::record(crate::capabilities::Capability::ExtractFunction, language);
    }
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let language = info.language;

    // Each of these languages has something that plays a function's role: a shell function, an
    // SCSS mixin, a Helm named template.
    match language {
        Language::Helm => return helm_named_template(file, span, name),
        Language::Bash => return bash_function(index, file, span, name),
        Language::Scss | Language::Sass => return sass_mixin(index, file, span, name, language),
        // A mixin is a Sass invention.
        Language::Css => anyhow::bail!(
            "plain CSS has no mixin, function or any other construct that names a group \
             of declarations, so there is nothing to extract into. `@mixin` / `@include` \
             are Sass, and the SCSS grammar is the only one here that parses them. Rename \
             {} to `.scss` if you meant SCSS",
            file.display()
        ),
        _ => {}
    }

    if !supports_imperative_extract_function(language) {
        return Err(Refusal::Unsupported {
            operation: "extract function".into(),
            language,
            because: "",
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

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    let run = statement_region(&parsed, span, &source)
        .ok_or_else(|| anyhow::anyhow!("select one or more complete statements to extract."))?;
    let region = run.span;

    // The moved statements keep their original bytes.
    if let Some(owner) = straddled_block(&run) {
        let lines = crate::span::LineIndex::new(&source);
        let at = lines.line_col(Span::from(owner).start, &source);
        return Err(Refusal::NotHere {
            operation: "extracting a function".into(),
            detail: format!(
                "the selection crosses the body of the `{}` at {}:{}. One end is inside \
                 that body and the other is outside it, and a call cannot span that \
                 boundary.",
                block_keyword(owner),
                file.display(),
                at.line
            ),
        }
        .into());
    }

    // A `break` targets a loop outside the region and no answer reaches it.
    let escaping = escaping_control_flow(&parsed, region);
    let enclosing_returns = enclosing_definition_node(&parsed, region)
        .and_then(|node| crate::analysis::types::return_type(&parsed, &source, Span::from(node)));
    let leaving = match escaping {
        Some("return") => match early_exit(language, enclosing_returns.as_deref()) {
            Some(form) => Some(form),
            None => {
                return Err(Refusal::NotHere {
                    operation: "extracting a function".into(),
                    detail: format!(
                        "the selected code contains a `return` that leaves the enclosing \
                         function. {language} has no value a call can answer with that it \
                         could not answer anyway. Select a run of statements that falls off \
                         its end, or lift the `return` out of the selection first."
                    ),
                }
                .into());
            }
        },
        Some(kind) => {
            return Err(Refusal::NotHere {
                operation: "extracting a function".into(),
                detail: format!(
                    "the selected code contains a `{kind}` that leaves the enclosing \
                     function, and a call cannot reproduce that. Select a run of \
                     statements that falls off its end, or lift the `{kind}` out of the \
                     selection first."
                ),
            }
            .into());
        }
        None => None,
    };

    if yields_to_caller(&parsed, region) {
        anyhow::bail!(
            "the selected code contains a `yield`, which belongs to the function whose \
             iteration the caller is driving; a call cannot hand that back, so this \
             region cannot be extracted as-is"
        );
    }

    // Carry an `await` across by marking the extracted function async and awaiting the
    // call.
    let is_async = awaits(&parsed, region);
    if is_async && !awaits_with_a_keyword(language) {
        anyhow::bail!(
            "the selected code awaits, and {language} does not spell that as a prefix \
             keyword this can move onto the extracted function, so the region cannot be \
             extracted as-is"
        );
    }

    let enclosing = enclosing_function(index, file, region.start).ok_or_else(|| {
        anyhow::anyhow!(
            "the selection is not inside a function, so there is nothing to extract from."
        )
    })?;
    let enclosing_span = index
        .symbol(enclosing)
        .map(|s| s.full_span)
        .unwrap_or(region);

    // Data flow in: names used in the region whose definition lives outside it.
    let mut parameters: Vec<Parameter> = Vec::new();
    let mut parameter_ids: Vec<(String, crate::model::SymbolId)> = Vec::new();
    let mut seen_params = std::collections::HashSet::new();
    for reference in references_within(index, file, region) {
        let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
            continue;
        };
        // Whether a binding is local is a question of scope, not of mutability: a
        // function-scoped `const` is every bit as local as a `let`.
        if !is_value_binding(target.kind) || !enclosing_span.contains(target.name_span) {
            continue;
        }
        if region.contains(target.name_span) {
            continue;
        }
        if seen_params.insert(target.name.clone()) {
            parameter_ids.push((target.name.clone(), target.id));
            parameters.push(Parameter {
                name: target.name.clone(),
                type_annotation: known_type(index, &parsed, &source, target, language),
                mutated: false,
                argument: None,
            });
        }
    }
    // Rust's inline format captures read a binding from inside a string.
    if language == Language::Rust {
        for name in format_captured_names(&parsed, &source, region) {
            if seen_params.contains(&name) {
                continue;
            }
            let Some(target) = index.find_symbols(&name, Some(file)).into_iter().find(|s| {
                is_value_binding(s.kind)
                    && enclosing_span.contains(s.name_span)
                    && !region.contains(s.name_span)
            }) else {
                continue;
            };
            seen_params.insert(name.clone());
            parameter_ids.push((name.clone(), target.id));
            parameters.push(Parameter {
                name: name.clone(),
                type_annotation: known_type(index, &parsed, &source, target, language),
                mutated: false,
                argument: None,
            });
        }
    }
    parameters.sort_by(|a, b| a.name.cmp(&b.name));

    // A parameter the region assigns to is a copy: the caller's binding keeps its old value in
    // every one of these languages.
    let assigned = assigned_names(&parsed, region, &source);
    let mut carried: Vec<String> = Vec::new();
    for (name, id) in &parameter_ids {
        if !assigned.contains(name) {
            continue;
        }
        let read_after = index.references_to(*id).iter().any(|r| {
            r.file == *file && r.span.start >= region.end && enclosing_span.contains(r.span)
        });
        if read_after {
            carried.push(name.clone());
        }
    }
    carried.sort();
    if !carried.is_empty() && language == Language::Zig {
        anyhow::bail!(
            "the selected code assigns to {}, declared outside it and read after it. \
             A Zig parameter cannot be assigned, so the change cannot travel back, \
             and this region cannot be extracted as-is.",
            carried.join(", ")
        );
    }
    for parameter in parameters.iter_mut() {
        parameter.mutated = carried.contains(&parameter.name);
    }

    // Data flow out: locals the region defines and later code still reads.
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
    returns.extend(carried.iter().cloned());
    returns.sort();
    returns.dedup();
    let carried: std::collections::BTreeSet<String> = carried.into_iter().collect();

    // The type of every returned binding.
    let return_types: Vec<Option<String>> = returns
        .iter()
        .map(|name| {
            info.symbols
                .iter()
                .filter_map(|id| index.symbol(*id))
                .find(|s| &s.name == name && region.contains(s.name_span))
                .and_then(|s| known_type(index, &parsed, &source, s, language))
                .or_else(|| {
                    parameters
                        .iter()
                        .find(|p| p.name == *name)
                        .and_then(|p| p.type_annotation.clone())
                })
        })
        .collect();

    // Languages that require types on parameters cannot have them invented.
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
        let unknown: Vec<&str> = returns
            .iter()
            .zip(&return_types)
            .filter(|(_, ty)| ty.is_none())
            .map(|(name, _)| name.as_str())
            .collect();
        if !unknown.is_empty() {
            anyhow::bail!(
                "cannot extract: {language} requires a return type, and nothing states \
                 or derives the type of {}. Annotate the declaration(s) and try again",
                unknown.join(", ")
            );
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

    // Go spells several results as a parenthesised list of types.
    let return_type: Option<String> = match return_types.as_slice() {
        [] => None,
        [only] => only.clone(),
        many => many
            .iter()
            .cloned()
            .collect::<Option<Vec<String>>>()
            .map(|types| format!("({})", types.join(", "))),
    };

    // A method's receiver is implicit: the region reads `this` or `self` and no parameter
    // carries it.
    let placement = definition_placement(&parsed, &source, region, language)?;
    let receiver = match placement.from_a_member.then(|| implicit_receiver(language)) {
        Some(Some(keyword)) => receiver_uses(&parsed, &source, region, keyword),
        _ => Vec::new(),
    };
    let mut body = region.text(&source).to_string();
    if !receiver.is_empty() {
        let carrier = carried_receiver(&parsed, &source, region, language, &parameters)?;
        for use_span in receiver.iter().rev() {
            let start = use_span.start - region.start;
            let end = use_span.end - region.start;
            body.replace_range(start..end, &carrier.name);
        }
        parameters.insert(0, carrier);
    }

    if leaving.is_some() && !returns.is_empty() {
        return Err(Refusal::NotHere {
            operation: "extracting a function".into(),
            detail: format!(
                "the selected code both leaves the enclosing function with a `return` \
                 and produces {} that the code after it reads. The extracted function \
                 can answer one of those, and answering both needs a shape this does \
                 not invent. Extract them separately.",
                returns.join(", ")
            ),
        }
        .into());
    }

    // The region's `return`s hand their value to the caller instead.
    let (body, return_type) = match &leaving {
        None => (body, return_type),
        Some(form) => {
            let mut rewritten = body.clone();
            for (statement, value) in returns_within(&parsed, region, &source).into_iter().rev() {
                let carried_value = value
                    .map(|v| v.text(&source).to_string())
                    .unwrap_or_default();
                let start = statement.start - region.start;
                let end = statement.end - region.start;
                if start > rewritten.len() || end > rewritten.len() {
                    continue;
                }
                rewritten.replace_range(start..end, &(form.carrying)(&carried_value));
            }
            if !form.falling_through.is_empty() {
                if !rewritten.ends_with('\n') {
                    rewritten.push('\n');
                }
                rewritten.push_str(&form.falling_through);
            }
            (rewritten, Some(form.answers.clone()))
        }
    };

    let indent = line_indent(&source, region.start);
    let call = match &leaving {
        None => render_call(language, name, &parameters, &returns, &carried, is_async),
        Some(form) => {
            let args = parameters
                .iter()
                .map(Parameter::argument)
                .collect::<Vec<_>>()
                .join(", ");
            let bare = match is_async {
                true => format!("await {name}({args})"),
                false => format!("{name}({args})"),
            };
            (form.call)(&bare).replace('\n', &format!("\n{indent}"))
        }
    };
    let definition = render_function(
        language,
        Signature {
            name,
            parameters: &parameters,
            returns: &returns,
            is_async,
            return_type: return_type.as_deref(),
        },
        &body,
        Indentation {
            outer: &indent,
            unit: &crate::edit::indent_unit(&source),
            lead: &placement.indent,
        },
    );

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(region, call, format!("call {name}")),
    );
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(placement.at, placement.at),
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

/// Where the extracted definition is spliced, and what it came out of.
struct Placement {
    /// Byte offset the definition goes at.
    at: usize,
    /// The indentation every line of the definition carries.
    indent: String,
    /// The region sits in a member of a class, an interface or an `impl` block.
    from_a_member: bool,
}

/// Where the new definition goes.
fn definition_placement(
    parsed: &Parsed,
    source: &str,
    region: Span,
    language: Language,
) -> Result<Placement> {
    let function = enclosing_definition_node(parsed, region).ok_or_else(|| {
        anyhow::anyhow!(
            "the selection is not inside a function, so there is nothing to extract from."
        )
    })?;

    let mut outermost = function;
    let mut from_a_member = false;
    let mut current = function;
    while let Some(parent) = current.parent() {
        if crate::refactor::is_type_definition(parent.kind()) {
            outermost = parent;
            from_a_member = true;
        } else if crate::refactor::is_function_definition(parent.kind()) {
            break;
        }
        current = parent;
    }

    // Java is the one language here whose extracted definition stays a member.
    let node = match language {
        Language::Java => function,
        _ => outermost,
    };
    Ok(Placement {
        at: node.end_byte(),
        indent: line_indent(source, node.start_byte()),
        from_a_member,
    })
}

/// The innermost function the region sits in.
fn enclosing_definition_node<'a>(parsed: &'a Parsed, region: Span) -> Option<Node<'a>> {
    let mut current = parsed
        .root()
        .descendant_for_byte_range(region.start, region.start)?;
    loop {
        if crate::refactor::is_function_definition(current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// The word a method of this language uses for its receiver, where the receiver is implicit.
fn implicit_receiver(language: Language) -> Option<&'static str> {
    match language {
        Language::TypeScript | Language::Tsx | Language::Java => Some("this"),
        Language::Rust => Some("self"),
        _ => None,
    }
}

/// Occurrences of the receiver keyword in the region that mean the enclosing method's receiver.
fn receiver_uses(parsed: &Parsed, source: &str, region: Span, keyword: &str) -> Vec<Span> {
    let mut found: Vec<Span> = collect_nodes(parsed.root(), |node| {
        let span = Span::from(node);
        if !region.contains(span) || span.text(source) != keyword {
            return false;
        }
        if !matches!(node.kind(), "this" | "self" | "identifier") {
            return false;
        }
        let mut current = node;
        while let Some(parent) = current.parent() {
            if !region.contains(Span::from(parent)) {
                return true;
            }
            if crate::refactor::is_function_definition(parent.kind())
                && parent.kind() != "arrow_function"
            {
                return false;
            }
            current = parent;
        }
        true
    })
    .into_iter()
    .map(Span::from)
    .collect();
    found.sort_by_key(|span| span.start);
    found
}

/// The parameter that carries a method's receiver into the extracted function.
fn carried_receiver(
    parsed: &Parsed,
    source: &str,
    region: Span,
    language: Language,
    parameters: &[Parameter],
) -> Result<Parameter> {
    let keyword = implicit_receiver(language).unwrap_or("this");
    let refuse = |detail: String| -> anyhow::Error {
        Refusal::NotHere {
            operation: "extracting a function".into(),
            detail,
        }
        .into()
    };
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        return Err(refuse(format!(
            "the selected code reads `{keyword}`, and the extracted function is not a \
             method, so it has no receiver to read it from. {language} cannot be handed \
             one as an ordinary parameter here."
        )));
    }

    let class = enclosing_class_node(parsed, region).ok_or_else(|| {
        refuse(
            "the selected code reads `this` outside a class, so what it means is decided \
             by the call and not by the text. A call to the extracted function would \
             decide it differently."
                .into(),
        )
    })?;
    let Some(class_name) = class
        .child_by_field_name("name")
        .map(|n| Span::from(n).text(source).to_string())
    else {
        return Err(refuse(
            "the selected code reads `this` inside a class expression with no name. The \
             parameter that would carry it has no type to be given."
                .into(),
        ));
    };
    if class.child_by_field_name("type_parameters").is_some() {
        return Err(refuse(format!(
            "the selected code reads `this` inside the generic class `{class_name}`. \
             Nothing here says which type arguments its receiver has. So the parameter \
             that would carry it cannot be typed."
        )));
    }
    if let Some(member) = unreachable_member(parsed, source, region, class) {
        return Err(refuse(format!(
            "the selected code reads `{member}`, which `{class_name}` does not expose. A \
             function outside the class cannot read it, so the region cannot be extracted \
             out of the class."
        )));
    }
    if node_in_region(parsed, region, |kind| kind == "super") {
        return Err(refuse(
            "the selected code reads `super`, which names the class's own base and means \
             nothing outside the class body."
                .into(),
        ));
    }

    let taken: std::collections::HashSet<String> = collect_nodes(parsed.root(), |node| {
        node.kind() == "identifier" && region.contains(Span::from(node))
    })
    .into_iter()
    .map(|node| Span::from(node).text(source).to_string())
    .chain(parameters.iter().map(|p| p.name.clone()))
    .collect();
    let mut name = "self".to_string();
    let mut suffix = 2;
    while taken.contains(&name) {
        name = format!("self{suffix}");
        suffix += 1;
    }

    Ok(Parameter {
        name,
        type_annotation: Some(class_name),
        mutated: false,
        argument: Some(keyword.to_string()),
    })
}

/// The class whose body the region sits in.
fn enclosing_class_node<'a>(parsed: &'a Parsed, region: Span) -> Option<Node<'a>> {
    let mut current = parsed
        .root()
        .descendant_for_byte_range(region.start, region.start)?;
    loop {
        if crate::refactor::is_type_definition(current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// The names Rust format strings inside the region capture inline.
fn format_captured_names(parsed: &Parsed, source: &str, region: Span) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if span.end <= region.start || span.start >= region.end {
            continue;
        }
        if node.kind().contains("string") && region.contains(span) {
            let text = span.text(source);
            let bytes = text.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] != b'{' {
                    i += 1;
                    continue;
                }
                if bytes.get(i + 1) == Some(&b'{') {
                    i += 2;
                    continue;
                }
                let start = i + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if end > start
                    && !bytes[start].is_ascii_digit()
                    && matches!(bytes.get(end), Some(b'}') | Some(b':'))
                {
                    names.push(text[start..end].to_string());
                }
                i = end.max(start) + 1;
            }
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    names.sort();
    names.dedup();
    names
}

/// A member the region reads that nothing outside the class may read.
fn unreachable_member(
    parsed: &Parsed,
    source: &str,
    region: Span,
    class: Node<'_>,
) -> Option<String> {
    if let Some(node) = collect_nodes(parsed.root(), |node| {
        node.kind() == "private_property_identifier" && region.contains(Span::from(node))
    })
    .first()
    {
        return Some(format!("this.{}", Span::from(*node).text(source)));
    }

    let hidden: std::collections::HashSet<String> = collect_nodes(class, |node| {
        matches!(
            node.kind(),
            "method_definition" | "public_field_definition" | "method_signature"
        )
    })
    .into_iter()
    .filter(|member| {
        let mut cursor = member.walk();
        member
            .children(&mut cursor)
            .any(|child| matches!(child.kind(), "accessibility_modifier"))
            && matches!(
                Span::from(*member).text(source).split_whitespace().next(),
                Some("private") | Some("protected")
            )
    })
    .filter_map(|member| member.child_by_field_name("name"))
    .map(|name| Span::from(name).text(source).to_string())
    .collect();

    collect_nodes(parsed.root(), |node| {
        node.kind() == "member_expression"
            && region.contains(Span::from(node))
            && node
                .child_by_field_name("object")
                .is_some_and(|object| object.kind() == "this")
    })
    .into_iter()
    .filter_map(|node| node.child_by_field_name("property"))
    .map(|property| Span::from(property).text(source).to_string())
    .find(|property| hidden.contains(property))
    .map(|property| format!("this.{property}"))
}

/// Names the region assigns to: the plain-name left side of an assignment, augmented or not,
/// and the operand of an increment.
fn assigned_names(
    parsed: &Parsed,
    region: Span,
    source: &str,
) -> std::collections::BTreeSet<String> {
    fn names_in_target(
        node: tree_sitter::Node<'_>,
        source: &str,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        let kind = node.kind();
        if kind.contains("identifier") {
            out.insert(Span::from(node).text(source).to_string());
            return;
        }
        // Go writes `a, b = f()` with an expression_list on the left; Python writes `a, b =
        // f()` with a pattern_list.
        if kind.contains("list") || kind.contains("pattern") || kind.contains("tuple") {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind().contains("identifier") {
                    out.insert(Span::from(child).text(source).to_string());
                }
            }
        }
    }

    fn walk(
        node: tree_sitter::Node<'_>,
        region: Span,
        source: &str,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        let span = Span::from(node);
        if span.end <= region.start || span.start >= region.end {
            return;
        }
        let kind = node.kind();
        if kind.contains("assignment") || kind.contains("augmented") {
            if let Some(target) = node
                .child_by_field_name("left")
                .or_else(|| node.named_child(0))
            {
                names_in_target(target, source, out);
            }
        }
        if kind == "update_expression" {
            if let Some(target) = node
                .child_by_field_name("argument")
                .or_else(|| node.named_child(0))
            {
                names_in_target(target, source, out);
            }
        }
        // tree-sitter-zig spells an assignment statement with the declaration's own
        // kind; the difference is the missing `var` or `const` in front.
        if kind == "variable_declaration" {
            let mut cursor = node.walk();
            let children: Vec<tree_sitter::Node<'_>> = node.children(&mut cursor).collect();
            let declares = children.iter().any(|c| matches!(c.kind(), "var" | "const"));
            if !declares {
                if let Some(first) = children.first() {
                    names_in_target(*first, source, out);
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, region, source, out);
        }
    }

    let mut out = std::collections::BTreeSet::new();
    walk(parsed.root(), region, source, &mut out);
    out
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
    matches!(
        language,
        Language::Rust | Language::Go | Language::Zig | Language::Java
    )
}

/// Languages whose extraction goes through the generic data-flow path.
fn supports_imperative_extract_function(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Java
    )
}

/// Can a region be extracted into something callable in this language?
pub fn supports_extract_function(language: Language) -> bool {
    supports_imperative_extract_function(language)
        || matches!(
            language,
            Language::Helm | Language::Bash | Language::Scss | Language::Sass
        )
}

/// Widen a selection to the complete statements it touches.
fn statement_region<'tree>(
    parsed: &'tree Parsed,
    span: Span,
    source: &str,
) -> Option<StatementRun<'tree>> {
    let text = span.text(source);
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    let content = Span::new(span.start + lead, span.end.saturating_sub(trail));
    if content.is_empty() {
        return None;
    }

    let first = statement_at(parsed, content.start, content)?;
    let last = statement_at(parsed, content.end.saturating_sub(1), content)?;
    Some(StatementRun {
        span: Span::new(
            Span::from(first).start.min(content.start),
            Span::from(last).end.max(content.end),
        ),
        first,
        last,
    })
}

/// A selection widened to whole statements, and the statements its two ends landed on.
struct StatementRun<'tree> {
    span: Span,
    /// The statement the selection starts on.
    first: Node<'tree>,
    /// The statement the selection ends on.
    last: Node<'tree>,
}

/// The statement covering `offset`, or the nearest one inside `content` when the offset sits on
/// a comment.
fn statement_at<'tree>(parsed: &'tree Parsed, offset: usize, content: Span) -> Option<Node<'tree>> {
    let node = parsed.root().descendant_for_byte_range(offset, offset)?;
    if !is_comment(node.kind()) {
        return statement_ancestor(node);
    }
    let mut best: Option<Node<'tree>> = None;
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(current) = stack.pop() {
        let span = Span::from(current);
        if !span.overlaps(content) {
            continue;
        }
        if content.contains(span) && !is_comment(current.kind()) && is_statement(current) {
            let better = match best {
                None => true,
                Some(other) => {
                    let other = Span::from(other);
                    (span.start, other.end) < (other.start, span.end)
                }
            };
            if better {
                best = Some(current);
            }
        }
        stack.extend(current.children(&mut cursor));
    }
    best
}

/// Is this node a statement, meaning a named direct child of a statement container?
fn is_statement(node: Node<'_>) -> bool {
    node.is_named()
        && node
            .parent()
            .is_some_and(|parent| crate::refactor::is_statement_container(parent.kind()))
}

fn is_comment(kind: &str) -> bool {
    kind.contains("comment")
}

/// The ancestor of `node` that is a statement: a named direct child of a container.
fn statement_ancestor(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        let parent = current.parent()?;
        if current.is_named() && crate::refactor::is_statement_container(parent.kind()) {
            return Some(current);
        }
        current = parent;
    }
}

/// The construct owning the block one end of `region` sits in, when its two ends sit in
/// different blocks.
fn straddled_block<'tree>(run: &StatementRun<'tree>) -> Option<Node<'tree>> {
    let (first_block, last_block) = (run.first.parent()?, run.last.parent()?);

    // A region is extractable when it runs over whole statements of one block.
    let mut cursor = first_block.walk();
    if first_block
        .children(&mut cursor)
        .any(|child| Span::from(child).end == run.span.end)
    {
        return None;
    }

    let deeper = if ancestor_count(first_block) >= ancestor_count(last_block) {
        first_block
    } else {
        last_block
    };
    deeper.parent()
}

/// How many ancestors `node` has, for comparing two blocks by depth.
fn ancestor_count(node: Node<'_>) -> usize {
    let mut count = 0;
    let mut current = node;
    while let Some(parent) = current.parent() {
        count += 1;
        current = parent;
    }
    count
}

/// The keyword introducing a construct, `for`, `if`, `else`, so a refusal can name it.
fn block_keyword(node: Node<'_>) -> String {
    match node.child(0) {
        Some(child) if !child.is_named() => child.kind().to_string(),
        _ => node.kind().to_string(),
    }
}

/// A `return`, `break` or `continue` inside the region that leaves it.
/// How a target says "the caller should return this", and "carry on". Only the two
/// that keep absence apart from every value a function answers can say it.
struct EarlyExit {
    answers: String,
    /// `return x` inside the region, rewritten.
    carrying: fn(&str) -> String,
    /// What it answers where the region falls off its end.
    falling_through: String,
    /// The call, and the return it drives.
    call: fn(&str) -> String,
}

fn early_exit(language: Language, enclosing_returns: Option<&str>) -> Option<EarlyExit> {
    match (language, enclosing_returns) {
        (Language::Rust, Some(ty)) => Some(EarlyExit {
            answers: format!("Option<{ty}>"),
            carrying: |value| format!("return Some({value})"),
            falling_through: "None".to_string(),
            call: |call| format!("if let Some(answer) = {call} {{\n    return answer;\n}}"),
        }),
        (Language::Rust, None) => Some(EarlyExit {
            answers: "bool".to_string(),
            carrying: |_| "return true".to_string(),
            falling_through: "false".to_string(),
            call: |call| format!("if {call} {{\n    return;\n}}"),
        }),
        // A declared zero: Go needs the value and no named type's zero is known here.
        (Language::Go, Some(ty)) => Some(EarlyExit {
            answers: format!("({ty}, bool)"),
            carrying: |value| format!("return {value}, true"),
            falling_through: format!("\tvar zero {ty}\n\treturn zero, false"),
            call: |call| format!("if answer, ok := {call}; ok {{\n\treturn answer\n}}"),
        }),
        (Language::Go, None) => Some(EarlyExit {
            answers: "bool".to_string(),
            carrying: |_| "return true".to_string(),
            falling_through: "\treturn false".to_string(),
            call: |call| format!("if {call} {{\n\treturn\n}}"),
        }),
        // Python and TypeScript answer `None` for absence and for a value alike.
        _ => None,
    }
}

/// Every `return` inside the region: its whole statement, and the value it carries.
fn returns_within(parsed: &Parsed, region: Span, source: &str) -> Vec<(Span, Option<Span>)> {
    let mut out = Vec::new();
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if !span.overlaps(region) {
            continue;
        }
        // Never the bare keyword: its span stops before the value.
        let returns = node.is_named()
            && (node.kind() == "return_statement" || node.kind() == "return_expression");
        if region.contains(span) && returns {
            let value = node
                .named_children(&mut node.walk())
                .find(|child| !child.kind().contains("comment"))
                .map(Span::from)
                .filter(|v| !v.text(source).trim().is_empty());
            out.push((span, value));
        }
        stack.extend(node.children(&mut cursor));
    }
    out.sort_by_key(|(span, _)| span.start);
    out
}

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
            // A break or continue belonging to a loop inside the region does no harm.
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

/// Does the region `yield`?
fn yields_to_caller(parsed: &Parsed, region: Span) -> bool {
    node_in_region(parsed, region, |kind| kind.contains("yield"))
}

/// Does the region `await`?
fn awaits(parsed: &Parsed, region: Span) -> bool {
    node_in_region(parsed, region, |kind| kind.contains("await"))
}

/// Is there a node wholly inside the region whose kind `wanted` accepts?
fn node_in_region(parsed: &Parsed, region: Span, wanted: impl Fn(&str) -> bool) -> bool {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if !span.overlaps(region) {
            continue;
        }
        if region.contains(span) && wanted(node.kind()) {
            return true;
        }
        stack.extend(node.children(&mut cursor));
    }
    false
}

/// Does this language write `await` as a prefix keyword.
fn awaits_with_a_keyword(language: Language) -> bool {
    matches!(
        language,
        Language::Python | Language::TypeScript | Language::Tsx
    )
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
fn declared_type(parsed: &Parsed, source: &str, declaration: Span) -> Option<String> {
    let node = parsed
        .root()
        .descendant_for_byte_range(declaration.start, declaration.end)?;
    // Java puts the type on the declaration and the name on a declarator inside it.
    let ty = node
        .child_by_field_name("type")
        .or_else(|| node.parent().and_then(|p| p.child_by_field_name("type")))?;
    let text = Span::from(ty).text(source).trim();
    let bare = text.strip_prefix(':').unwrap_or(text).trim();
    // `var` is the word for "the compiler worked it out", which is exactly the
    // type this has no way to recover.
    (!bare.is_empty() && bare != "var").then(|| bare.to_string())
}

/// The type of a binding: what the source wrote, or what follows from what it wrote.
fn known_type(
    index: &Index,
    parsed: &Parsed,
    source: &str,
    symbol: &crate::model::Symbol,
    language: Language,
) -> Option<String> {
    if let Some(written) = declared_type(parsed, source, symbol.full_span) {
        return Some(written);
    }
    if !requires_explicit_types(language) {
        return None;
    }
    crate::analysis::types::of(index, symbol.id)
        .ok()
        .and_then(|known| known.ty().map(str::to_string))
}

/// The call that replaces the extracted region.
fn render_call(
    language: Language,
    name: &str,
    parameters: &[Parameter],
    returns: &[String],
    carried: &std::collections::BTreeSet<String>,
    is_async: bool,
) -> String {
    let args = parameters
        .iter()
        .map(Parameter::argument)
        .collect::<Vec<_>>()
        .join(", ");
    let call = match is_async {
        true => format!("await {name}({args})"),
        false => format!("{name}({args})"),
    };

    // A returned binding that already exists at the call site is assigned, never re-declared.
    let all_carried = !returns.is_empty() && returns.iter().all(|r| carried.contains(r));
    match (returns.len(), language) {
        (0, Language::Python | Language::Go) => call,
        (0, _) => format!("{call};"),
        (1, Language::Python) => format!("{} = {call}", returns[0]),
        (1, Language::Rust) if all_carried => format!("{} = {call};", returns[0]),
        (1, Language::Rust) => format!("let {} = {call};", returns[0]),
        (1, Language::Go) if all_carried => format!("{} = {call}", returns[0]),
        (1, Language::Go) => format!("{} := {call}", returns[0]),
        (1, Language::Java) if all_carried => format!("{} = {call};", returns[0]),
        (1, Language::Java) => format!("var {} = {call};", returns[0]),
        (1, _) if all_carried => format!("{} = {call};", returns[0]),
        (1, _) => format!("const {} = {call};", returns[0]),
        (_, Language::Python) => format!("{} = {call}", returns.join(", ")),
        // `:=` is legal while at least one name on the left is new.
        (_, Language::Go) if all_carried => format!("{} = {call}", returns.join(", ")),
        (_, Language::Go) => format!("{} := {call}", returns.join(", ")),
        _ => format!("{call};"),
    }
}

/// How the file being edited is indented.
#[derive(Clone, Copy)]
struct Indentation<'a> {
    outer: &'a str,
    unit: &'a str,
    lead: &'a str,
}

/// The new function definition.
struct Signature<'a> {
    name: &'a str,
    parameters: &'a [Parameter],
    /// Bindings the region produced that the code after it still reads.
    returns: &'a [String],
    /// The region awaited, so the function does too and the call awaits it back.
    is_async: bool,
    /// The declared type of the single returned binding, where the source stated one.
    return_type: Option<&'a str>,
}

fn render_function(
    language: Language,
    signature: Signature<'_>,
    body: &str,
    indent: Indentation<'_>,
) -> String {
    let Signature {
        name,
        parameters,
        returns,
        is_async,
        return_type,
    } = signature;
    let Indentation {
        outer: indent,
        unit: body_indent,
        lead,
    } = indent;
    let params = parameters
        .iter()
        .map(|p| p.render(language))
        .collect::<Vec<_>>()
        .join(", ");
    let reindented = body
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                let stripped = line.strip_prefix(indent).unwrap_or(line);
                format!("{lead}{body_indent}{stripped}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // `async` goes in front of the keyword that opens the definition, and only the
    // languages that spell it that way ever get here.
    let prefix = match is_async {
        true => "async ",
        false => "",
    };

    match language {
        Language::Python => {
            let tail = match returns.len() {
                0 => String::new(),
                _ => format!("\n{lead}{body_indent}return {}", returns.join(", ")),
            };
            // Two blank lines before a definition at module scope and one before a method.
            let separation = match lead.is_empty() {
                true => "\n\n\n",
                false => "\n\n",
            };
            format!("{separation}{lead}{prefix}def {name}({params}):\n{reindented}{tail}")
        }
        Language::Rust => {
            let ret = match return_type {
                Some(ty) => format!(" -> {ty}"),
                None => String::new(),
            };
            let tail = match returns.first() {
                Some(r) => format!("\n{lead}{body_indent}{r}"),
                None => String::new(),
            };
            format!("\n\n{lead}fn {name}({params}){ret} {{\n{reindented}{tail}\n{lead}}}")
        }
        Language::Zig => {
            // Zig writes the return type after the parameter list with no arrow, and
            // it is not optional: a function that yields nothing still says `void`.
            let ret = return_type.unwrap_or("void");
            let tail = match returns.first() {
                Some(r) => format!("\n{lead}{body_indent}return {r};"),
                None => String::new(),
            };
            format!("\n\n{lead}fn {name}({params}) {ret} {{\n{reindented}{tail}\n{lead}}}")
        }
        Language::Go => {
            let ret = match return_type {
                Some(ty) => format!(" {ty}"),
                None => String::new(),
            };
            let tail = match returns.len() {
                0 => String::new(),
                _ => format!("\n{lead}{body_indent}return {}", returns.join(", ")),
            };
            format!("\n\n{lead}func {name}({params}){ret} {{\n{reindented}{tail}\n{lead}}}")
        }
        Language::Java => {
            // The new method sits beside the one it came from, inside the same class.
            let ret = return_type.unwrap_or("void");
            let tail = match returns.first() {
                Some(r) => format!("\n{lead}{body_indent}return {r};"),
                None => String::new(),
            };
            format!("\n\n{lead}static {ret} {name}({params}) {{\n{reindented}{tail}\n{lead}}}")
        }
        _ => {
            let tail = match returns.first() {
                Some(r) => format!("\n{lead}{body_indent}return {r};"),
                None => String::new(),
            };
            format!("\n\n{lead}{prefix}function {name}({params}) {{\n{reindented}{tail}\n{lead}}}")
        }
    }
}

// None of these languages has a binding form.

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
    parsed.descendant_at(span.start, span.end.max(span.start))
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

/// The innermost *strict* ancestor of `node` with this kind.
fn ancestor_or_self_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    match node.kind() == kind {
        true => Some(node),
        false => strict_ancestor_of_kind(node, kind),
    }
}

fn strict_ancestor_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut current = node.parent()?;
    loop {
        if current.kind() == kind {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// The first child of `node` with this kind, anonymous children included.
fn child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
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

/// Extract a Terraform expression into a `locals` entry.
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

    let source = crate::vfs::read_to_string(file)?;
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
            && n.named_child(0).is_some_and(|c| {
                c.kind() == "identifier" && Span::from(c).text(&source) == "locals"
            })
    })
    .into_iter()
    .next();

    // Skip the `locals` block: rewriting there points an entry at itself, and Terraform
    // rejects the cycle.
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
            Edit::new(
                *target,
                format!("local.{name}"),
                format!("use local.{name}"),
            ),
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

/// Extract a repeated YAML scalar into an anchor plus aliases.
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

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;

    if let Some(action) = parsed.masked_spans.iter().find(|a| a.overlaps(span)) {
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

    let mut occurrences: Vec<Span> = collect_nodes(parsed.root(), |n| {
        yaml_is_anchorable(n, &source) && Span::from(n).text(&source) == value_text
    })
    .into_iter()
    .map(Span::from)
    .collect();
    if occurrences.is_empty() {
        occurrences = vec![selected_span];
    }

    // An anchor binds a name to a value and an alias spends it.
    if !all_occurrences {
        let others = occurrences.len().saturating_sub(1);
        let detail = match others {
            0 => format!(
                "`{value_text}` is written once in {}, so an anchor would bind a name \
                 that nothing spends",
                file.display()
            ),
            _ => format!(
                "an anchor alone changes nothing; pass --all to alias the {others} other \
                 occurrence(s) of `{value_text}` in {}",
                file.display()
            ),
        };
        return Err(Refusal::NotHere {
            operation: "extract a YAML anchor".into(),
            detail,
        }
        .into());
    }

    if let Some(clash) = occurrences
        .iter()
        .find(|s| parsed.masked_spans.iter().any(|a| a.overlaps(**s)))
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
    // An anchored or aliased node already names a value.
    !children.is_empty()
        && children.iter().all(|c| {
            matches!(
                c.kind(),
                "plain_scalar" | "double_quote_scalar" | "single_quote_scalar"
            )
        })
}

/// Extract a declaration value into a custom property declared in `:root`.
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
    if scss_variable && !matches!(language, Language::Scss | Language::Sass) {
        anyhow::bail!(
            "`{name}` asks for an SCSS `$variable`, but {} is plain CSS, which has no \
             such syntax. Use a name without the `$` to extract a CSS custom property, \
             which works in both dialects",
            file.display()
        );
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the selection cannot be located reliably",
            file.display()
        );
    }

    // An SCSS variable keeps its `$`; a custom property is normalised to `--name`.
    let property = if scss_variable || name.starts_with("--") {
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
        // Skip the `:root` rule taking the declaration: rewriting there defines the
        // property in terms of itself.
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

    // The indented syntax ends a declaration at the line and has no braces, so what is
    // written differs from what the braced syntax writes.
    let indented = language == Language::Sass;
    let declaration = match indented {
        true => format!("{property}: {value_text}"),
        false => format!("{property}: {value_text};"),
    };

    // SCSS declares a variable at the top level of the stylesheet, outside any `:root`
    // rule.
    if scss_variable {
        let top = css_insertion_point(&parsed, &source);
        let insert_at = sass_variable_insertion_point(
            &parsed,
            &source,
            &value_text,
            top,
            targets.iter().map(|t| t.start).min().unwrap_or(usize::MAX),
            file,
        )?;
        let mut edits = EditSet::new();
        edits.add(
            file.to_path_buf(),
            Edit::new(
                Span::new(insert_at, insert_at),
                match insert_at == top {
                    // At the top of the file, a blank line separates it from what follows.
                    true => format!("{declaration}\n\n"),
                    // Below the declarations it reads, where a blank line already stands.
                    false => format!("{declaration}\n"),
                },
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
                // An empty `:root` rule: the declaration is the body.
                None => match indented {
                    true => (block.start_byte(), format!("{declaration}\n")),
                    false => (block.start_byte() + 1, format!("\n  {declaration}\n")),
                },
            }
        }
        None => (
            css_insertion_point(&parsed, &source),
            match indented {
                true => format!(":root\n  {declaration}\n\n"),
                false => format!(":root {{\n  {declaration}\n}}\n\n"),
            },
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

/// Where a new `$variable` declaration may go: after every `$variable` its value reads.
fn sass_variable_insertion_point(
    parsed: &Parsed,
    source: &str,
    value_text: &str,
    top: usize,
    first_use: usize,
    file: &Path,
) -> Result<usize> {
    let read: Vec<&str> = value_text
        .split(|c: char| !(c.is_alphanumeric() || c == '$' || c == '-' || c == '_'))
        .filter(|word| word.starts_with('$') && word.len() > 1)
        .collect();
    if read.is_empty() {
        return Ok(top);
    }

    let mut after = top;
    for declaration in collect_nodes(parsed.root(), |n| n.kind() == "declaration") {
        let Some(name) = declaration.named_child(0) else {
            continue;
        };
        let name = Span::from(name).text(source);
        if !read.contains(&name) {
            continue;
        }
        let end = source[..declaration.end_byte()].trim_end().len();
        if end > first_use {
            anyhow::bail!(
                "`{name}` is declared after the value being extracted is used in {}, so \
                 the new declaration has nowhere to stand: it would read a variable that \
                 does not exist yet. Move `{name}` above that use first",
                file.display()
            );
        }
        // The line the declaration ends on, and the newline after it.
        after = after.max(match source[end..].find('\n') {
            Some(offset) => end + offset + 1,
            None => source.len(),
        });
    }
    Ok(after)
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

/// Where a new rule may go: after any leading `@charset` and `@import`, which CSS
/// requires before every rule.
fn css_insertion_point(parsed: &Parsed, source: &str) -> usize {
    let root = parsed.root();
    let mut cursor = root.walk();
    let mut offset = 0usize;
    for child in root.named_children(&mut cursor) {
        // Sass requires `@use` and `@forward` before any other rule.
        if matches!(
            child.kind(),
            "import_statement"
                | "charset_statement"
                | "comment"
                | "single_line_comment"
                | "use_statement"
                | "forward_statement"
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

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Markdown, &source)?;

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let link = markdown_link_ancestor(node).ok_or_else(|| {
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
        parsed
            .roots()
            .flat_map(|root| {
                collect_nodes(root, |n| {
                    MARKDOWN_LINK_KINDS.contains(&n.kind())
                        && markdown_inline_destination(n, &source)
                            .is_some_and(|(_, d)| d == destination)
                })
            })
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

/// The Markdown node kinds that are links.
const MARKDOWN_LINK_KINDS: [&str; 4] = [
    "inline_link",
    "full_reference_link",
    "collapsed_reference_link",
    "shortcut_link",
];

/// The innermost link enclosing `node`, whichever of the four spellings it is.
fn markdown_link_ancestor(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        if MARKDOWN_LINK_KINDS.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
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
    // Blocks hang off `section` nodes and not off the document, so the last block
    // is at the bottom of the last section.
    let mut node = parsed.root();
    loop {
        let mut cursor = node.walk();
        let last = node.named_children(&mut cursor).last();
        match last {
            Some(last) if last.kind() == "link_reference_definition" => return true,
            Some(last) if last.kind() == "section" => node = last,
            _ => return false,
        }
    }
}

/// Extract a region of a Helm template into a named template in `_helpers.tpl`.
fn helm_named_template(file: &Path, span: Span, name: &str) -> Result<ExtractFunctionPlan> {
    if name.is_empty() || name.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(invalid(
            name,
            "a Helm template name may not be empty or contain whitespace or a quote",
        ));
    }

    // The include name is `<chart>.<template>` by convention, and a chart's templates share one
    // flat namespace across every chart in a release.
    let chart_root = helm_chart_root(file).ok_or_else(|| {
        anyhow::anyhow!(
            "no Chart.yaml above {}, so the chart name is unknown. A named template is \
             addressed as `<chart>.<name>` across every chart in a release, and an \
             include under the wrong name renders empty instead of failing",
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

    let source = crate::vfs::read_to_string(file)?;
    let region = whole_lines(&source, span);
    if region.text(&source).trim().is_empty() {
        anyhow::bail!("the selection at bytes {span} is blank; select the lines to extract");
    }
    let body = region.text(&source).to_string();

    let destination = helm_helpers_path(file, &chart_root);
    let existing = crate::vfs::read_to_string(&destination).unwrap_or_default();
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
        if crate::vfs::exists(current.join("Chart.yaml"))
            || crate::vfs::exists(current.join("chart.yaml"))
        {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// The chart's `name:` from its Chart.yaml.
fn helm_chart_name(chart_root: &Path) -> Option<String> {
    let path = if crate::vfs::exists(chart_root.join("Chart.yaml")) {
        chart_root.join("Chart.yaml")
    } else {
        chart_root.join("chart.yaml")
    };
    let text = crate::vfs::read_to_string(path).ok()?;
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

// Shell has bindings, so "extract variable" means what it means everywhere else.

/// A shell variable or function name: a letter or underscore, then word characters.
fn is_shell_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The node kinds that hold a value worth naming.
const BASH_VALUE_KINDS: &[&str] = &[
    "command_substitution",
    "process_substitution",
    "arithmetic_expansion",
    "string",
    "raw_string",
    "ansi_c_string",
    "translated_string",
    "concatenation",
    "word",
    "number",
];

/// Extract a command substitution or literal into a shell variable.
fn bash_variable(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    all_occurrences: bool,
) -> Result<ExtractPlan> {
    if !is_shell_name(name) {
        return Err(invalid(
            name,
            "a shell variable name must start with a letter or underscore and contain \
             only letters, digits and underscores",
        ));
    }
    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Bash, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the selection cannot be located reliably",
            file.display()
        );
    }

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;

    if let Some(existing) = bash_expansion_at(node) {
        anyhow::bail!(
            "`{}` is already a variable expansion; extracting it would only create an alias",
            Span::from(existing).text(&source)
        );
    }

    let value = bash_extractable(node).ok_or_else(|| {
        anyhow::anyhow!(
            "no command substitution or literal at bytes {span} in {}; select a `$( … )`, \
             a quoted string or a bare word",
            file.display()
        )
    })?;
    if strict_ancestor_of_kind(value, "command_name").is_some() || value.parent().is_none() {
        anyhow::bail!(
            "`{}` is the name of a command, not a value; a variable in command position \
             would be re-split and re-globbed before the shell looked it up",
            Span::from(value).text(&source)
        );
    }
    if strict_ancestor_of_kind(value, "heredoc_body").is_some() {
        anyhow::bail!(
            "the selection is inside a here-document body, whose bytes are data rather \
             than a value position; a binding cannot be spliced in front of it"
        );
    }

    let value_span = Span::from(value);
    let value_text = value_span.text(&source).to_string();

    let statement = bash_statement(value)?;
    let statement_span = Span::from(statement);
    let indent = line_indent(&source, statement_span.start);

    // Rewrite only from the insertion point onwards: an earlier occurrence would read a
    // variable nothing has set.
    let targets: Vec<Node> = if all_occurrences {
        let found = collect_nodes(parsed.root(), |n| {
            n.kind() == value.kind()
                && n.start_byte() >= statement_span.start
                && Span::from(n).text(&source) == value_text
        });
        if found.is_empty() {
            vec![value]
        } else {
            found
        }
    } else {
        vec![value]
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(statement_span.start, statement_span.start),
            format!("{name}={value_text}\n{indent}"),
            format!("introduce {name}"),
        ),
    );
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(
                Span::from(*target),
                bash_reference(*target, &source, name),
                format!("use {name}"),
            ),
        );
    }

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: value_text,
        edits,
        occurrences: targets.len(),
    })
}

/// The `$X` / `${X}` the selection lands on, if it lands on one.
fn bash_expansion_at(node: Node<'_>) -> Option<Node<'_>> {
    if matches!(node.kind(), "expansion" | "simple_expansion") {
        return Some(node);
    }
    if node.kind() == "variable_name" || node.kind() == "special_variable_name" {
        let parent = node.parent()?;
        if matches!(parent.kind(), "expansion" | "simple_expansion") {
            return Some(parent);
        }
    }
    None
}

/// The value node covering the selection: itself or the innermost ancestor that holds
/// a value, stopping before the search escapes into statement territory.
fn bash_extractable(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        if BASH_VALUE_KINDS.contains(&current.kind()) {
            return Some(current);
        }
        let parent = current.parent()?;
        if bash_is_statement_container(parent.kind()) {
            return None;
        }
        current = parent;
    }
}

/// Node kinds whose children are complete statements, so a binding may be spliced in
/// front of any one of them.
fn bash_is_statement_container(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "compound_statement"
            | "subshell"
            | "do_group"
            | "case_item"
            | "else_clause"
            | "command_substitution"
            | "process_substitution"
    )
}

/// The statement the value belongs to, the one the binding goes in front of.
fn bash_statement(node: Node<'_>) -> Result<Node<'_>> {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            anyhow::bail!("the selection is not inside a statement");
        };
        if bash_is_statement_container(parent.kind()) {
            return Ok(current);
        }
        match parent.kind() {
            // `if`/`elif` hold their condition as an ordinary child alongside the
            // body, so position relative to `then` is what tells them apart.
            "if_statement" | "elif_clause" => match child_of_kind(parent, "then") {
                Some(then) if current.start_byte() >= then.end_byte() => return Ok(current),
                _ => anyhow::bail!(
                    "the selection is part of the condition of an `if`; a binding \
                         spliced in front of it would become the command whose exit \
                         status is tested"
                ),
            },
            "while_statement" | "until_statement" | "c_style_for_statement" => anyhow::bail!(
                "the selection is part of a loop's condition, which the shell \
                 re-evaluates on every iteration; hoisting it into a variable before \
                 the loop would evaluate it exactly once"
            ),
            _ => current = parent,
        }
    }
}

/// Spell one occurrence so the shell still sees the same words.
fn bash_reference(occurrence: Node<'_>, source: &str, name: &str) -> String {
    // Inside double quotes the expansion is already protected from splitting, and a
    // second pair of quotes would end the string and not nest inside it.
    if strict_ancestor_of_kind(occurrence, "string").is_some() {
        return format!("${{{name}}}");
    }
    if bash_would_split(occurrence, source) {
        // The shell splits the original on `$IFS` and glob-expands it where it stands.
        format!("${name}")
    } else {
        format!("\"${name}\"")
    }
}

/// Would the bytes at this position have been word-split and glob-expanded?
fn bash_would_split(node: Node<'_>, source: &str) -> bool {
    // The shell never splits the right-hand side of an assignment.
    if node
        .parent()
        .is_some_and(|p| p.kind() == "variable_assignment")
    {
        return false;
    }
    match node.kind() {
        // A quoted literal is already exactly one word.
        "string" | "raw_string" | "ansi_c_string" | "translated_string" => false,
        // A bare word cannot contain whitespace, so only globbing is in play.
        "word" | "number" => Span::from(node).text(source).contains(['*', '?', '[']),
        // Anything computed at run time can expand to any number of words.
        _ => true,
    }
}

/// Extract statements into a shell function.
fn bash_function(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
) -> Result<ExtractFunctionPlan> {
    if !is_shell_name(name) {
        return Err(invalid(
            name,
            "a shell function name must start with a letter or underscore and contain \
             only letters, digits and underscores",
        ));
    }
    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Bash, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the selection cannot be located reliably",
            file.display()
        );
    }

    let region = whole_lines(&source, span);
    if region.text(&source).trim().is_empty() {
        anyhow::bail!("the selection at bytes {span} is blank; select the lines to extract");
    }

    if let Some(cut) = bash_straddling_node(&parsed, region) {
        anyhow::bail!(
            "the selection cuts across a `{}` at bytes {}; select whole statements. A \
             call can replace a statement but not part of one",
            cut.kind(),
            Span::from(cut)
        );
    }

    let positional = bash_positional_parameters(&parsed, region, &source);
    if !positional.is_empty() {
        anyhow::bail!(
            "the selected code reads the positional parameter(s) {}. Inside a shell \
             function those name that function's own arguments, so moving the code \
             would rebind them to whatever the call passes, which is nothing. Read \
             them into named variables first",
            positional.join(", ")
        );
    }

    if let Some(word) = bash_escaping_control_flow(&parsed, region, &source) {
        anyhow::bail!(
            "the selected code contains a `{word}` that leaves the enclosing function or \
             loop; a call cannot reproduce that, so this region cannot be extracted as-is"
        );
    }

    if let Some(local) = bash_local_used_after(&parsed, region, &source) {
        anyhow::bail!(
            "the selection declares `local {local}` and `{local}` is read after it. A \
             `local` belongs to the function that declares it, so moving the declaration \
             into a new function would leave the later read seeing the outer value"
        );
    }

    let enclosing = bash_enclosing_function(&parsed, region.start);
    let insert_at = match enclosing {
        Some(function) => full_line_span(&source, function.start_byte()).start,
        None => bash_script_top(&parsed, &source),
    };
    let definition_indent = line_indent(&source, insert_at);
    let region_indent = line_indent(&source, region.start);
    let body = region.text(&source).to_string();

    let mut definition = format!("{definition_indent}{name}() {{\n");
    for line in body.lines() {
        if line.trim().is_empty() {
            definition.push('\n');
        } else {
            let stripped = line.strip_prefix(region_indent.as_str()).unwrap_or(line);
            definition.push_str(&format!("{definition_indent}  {stripped}\n"));
        }
    }
    definition.push_str(&format!("{definition_indent}}}\n\n"));

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            definition,
            format!("define {name}"),
        ),
    );
    edits.add(
        file.to_path_buf(),
        Edit::new(
            region,
            format!("{region_indent}{name}\n"),
            format!("call {name}"),
        ),
    );

    Ok(ExtractFunctionPlan {
        name: name.to_string(),
        edits,
        parameters: Vec::new(),
        returns: Vec::new(),
        body,
    })
}

/// A node with one end inside the region and the other outside it.
fn bash_straddling_node<'a>(parsed: &'a Parsed, region: Span) -> Option<Node<'a>> {
    collect_nodes(parsed.root(), |n| {
        let span = Span::from(n);
        span.overlaps(region) && !region.contains(span) && !span.contains(region)
    })
    .into_iter()
    .next()
}

/// `$1`, `$2`, `$@`, `$*` and `$#` read inside the region.
fn bash_positional_parameters(parsed: &Parsed, region: Span, source: &str) -> Vec<String> {
    let mut found: Vec<String> = collect_nodes(parsed.root(), |n| {
        if !region.contains(Span::from(n)) {
            return false;
        }
        let text = Span::from(n).text(source);
        match n.kind() {
            // `$0` is the script's own name and keeps it inside a function.
            "variable_name" => text.chars().all(|c| c.is_ascii_digit()) && text != "0",
            "special_variable_name" => matches!(text, "@" | "*" | "#"),
            _ => false,
        }
    })
    .into_iter()
    .map(|n| format!("${}", Span::from(n).text(source)))
    .collect();
    found.sort();
    found.dedup();
    found
}

/// A `return`, `break` or `continue` in the region that would leave it.
fn bash_escaping_control_flow(parsed: &Parsed, region: Span, source: &str) -> Option<&'static str> {
    for node in collect_nodes(parsed.root(), |n| {
        n.kind() == "command" && region.contains(Span::from(n))
    }) {
        let Some(word) = node
            .child_by_field_name("name")
            .map(|n| Span::from(n).text(source).trim())
        else {
            continue;
        };
        match word {
            "return" => return Some("return"),
            "break" | "continue" if !has_enclosing_loop_within(node, region) => {
                return Some(if word == "break" { "break" } else { "continue" })
            }
            _ => {}
        }
    }
    None
}

/// A `local` the region declares and later code reads.
fn bash_local_used_after(parsed: &Parsed, region: Span, source: &str) -> Option<String> {
    let declared: Vec<String> = collect_nodes(parsed.root(), |n| {
        n.kind() == "declaration_command"
            && region.contains(Span::from(n))
            && ["local", "declare", "typeset"]
                .iter()
                .any(|keyword| child_of_kind(n, keyword).is_some())
    })
    .into_iter()
    .flat_map(|n| {
        let mut cursor = n.walk();
        let names: Vec<String> = n
            .named_children(&mut cursor)
            .filter_map(|c| match c.kind() {
                "variable_assignment" => c.child_by_field_name("name"),
                "variable_name" => Some(c),
                _ => None,
            })
            .map(|c| Span::from(c).text(source).to_string())
            .collect();
        names
    })
    .collect();
    if declared.is_empty() {
        return None;
    }

    collect_nodes(parsed.root(), |n| {
        n.kind() == "variable_name"
            && n.start_byte() >= region.end
            && declared.iter().any(|d| d == Span::from(n).text(source))
    })
    .into_iter()
    .next()
    .map(|n| Span::from(n).text(source).to_string())
}

/// The innermost `f() { … }` whose bytes contain `offset`.
fn bash_enclosing_function(parsed: &Parsed, offset: usize) -> Option<Node<'_>> {
    collect_nodes(parsed.root(), |n| {
        n.kind() == "function_definition" && Span::from(n).contains_offset(offset)
    })
    .into_iter()
    .min_by_key(|n| Span::from(*n).len())
}

/// Where a definition may go at the top of a script.
fn bash_script_top(parsed: &Parsed, source: &str) -> usize {
    let root = parsed.root();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        return full_line_span(source, child.start_byte()).start;
    }
    source.len()
}

/// Extract declarations into an SCSS `@mixin`, called back through `@include`.
fn sass_mixin(
    index: &Index,
    file: &Path,
    span: Span,
    name: &str,
    language: Language,
) -> Result<ExtractFunctionPlan> {
    // The two Sass syntaxes name their nodes differently and write their blocks
    // differently, and everything between those two ends is the same operation.
    let indented = language == Language::Sass;
    if name.is_empty()
        || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(invalid(
            name,
            "a mixin name must start with a letter or underscore and contain only \
             letters, digits, underscores and dashes",
        ));
    }
    if !index.find_symbols(name, Some(file)).is_empty() {
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(language, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} does not parse cleanly, so the selection cannot be located reliably",
            file.display()
        );
    }

    // A line selection starts on the line's indentation, which belongs to no
    // declaration, so the search starts at the first real content inside it.
    let text = span.text(&source);
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    let content = Span::new(span.start + lead, span.end.saturating_sub(trail));
    if content.is_empty() {
        anyhow::bail!("the selection at bytes {span} is blank; select the declarations to extract");
    }

    // The node covering the whole selection, and not the one standing at its first byte.
    let node = parsed
        .root()
        .descendant_for_byte_range(content.start, content.end)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let block = ancestor_or_self_of_kind(node, "block").ok_or_else(|| {
        anyhow::anyhow!(
            "the selection at bytes {span} is not inside a rule's body; a mixin holds a \
             rule's declarations, so there have to be some to move"
        )
    })?;
    if !block.parent().is_some_and(|p| p.kind() == "rule_set") {
        anyhow::bail!(
            "the selection is inside a `{}`, not a rule. Only declarations written \
             directly in a rule can move into a mixin",
            block
                .parent()
                .map(|p| p.kind().to_string())
                .unwrap_or_default()
        );
    }

    let mut cursor = block.walk();
    let selected: Vec<Node> = block
        .named_children(&mut cursor)
        .filter(|c| Span::from(*c).overlaps(content))
        .collect();
    let (Some(first), Some(last)) = (selected.first(), selected.last()) else {
        anyhow::bail!("the selection at bytes {span} covers no complete declaration");
    };
    if let Some(other) = selected.iter().find(|c| c.kind() != "declaration") {
        anyhow::bail!(
            "the selection contains a `{}`; only declarations move into a mixin \
             unchanged, so this region is refused and not reinterpreted",
            other.kind()
        );
    }
    // A declaration in the indented syntax ends at the start of the next line.
    let end = source[..last.end_byte()].trim_end().len();
    let region = Span::new(first.start_byte(), end.max(first.start_byte()));

    // A `$variable` the selection declares itself travels with it.
    let (declaration_name, use_kind) = match indented {
        true => ("variable_name", "variable_value"),
        false => ("property_name", "variable"),
    };
    let declared_inside: Vec<String> = collect_nodes(parsed.root(), |n| {
        n.kind() == declaration_name
            && region.contains(Span::from(n))
            && Span::from(n).text(&source).starts_with('$')
    })
    .into_iter()
    .map(|n| Span::from(n).text(&source).to_string())
    .collect();

    let mut parameters: Vec<Parameter> = Vec::new();
    for node in collect_nodes(parsed.root(), |n| {
        n.kind() == use_kind && region.contains(Span::from(n))
    }) {
        let text = Span::from(node).text(&source).to_string();
        if declared_inside.contains(&text) || parameters.iter().any(|p| p.name == text) {
            continue;
        }
        parameters.push(Parameter {
            name: text,
            type_annotation: None,
            mutated: false,
            argument: None,
        });
    }

    let signature = if parameters.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            parameters
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let body = region.text(&source).to_string();
    let region_indent = line_indent(&source, region.start);
    let mut definition = match indented {
        true => format!("@mixin {name}{signature}\n"),
        false => format!("@mixin {name}{signature} {{\n"),
    };
    for line in body.lines() {
        if line.trim().is_empty() {
            definition.push('\n');
        } else {
            let stripped = line.strip_prefix(region_indent.as_str()).unwrap_or(line);
            definition.push_str(&format!("  {stripped}\n"));
        }
    }
    if !indented {
        definition.push('}');
        definition.push('\n');
    }
    definition.push('\n');

    let insert_at = css_insertion_point(&parsed, &source);
    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            definition,
            format!("define @mixin {name}"),
        ),
    );
    edits.add(
        file.to_path_buf(),
        Edit::new(
            region,
            match indented {
                true => format!("@include {name}{signature}"),
                false => format!("@include {name}{signature};"),
            },
            format!("include {name}"),
        ),
    );

    Ok(ExtractFunctionPlan {
        name: name.to_string(),
        edits,
        parameters,
        returns: Vec::new(),
        body,
    })
}

/// Extract repeated text into an internal-subset entity.
fn xml_entity(file: &Path, span: Span, name: &str, all_occurrences: bool) -> Result<ExtractPlan> {
    if !is_xml_name(name) {
        return Err(invalid(
            name,
            "an XML entity name must start with a letter or underscore and contain only \
             letters, digits, `.`, `-`, `_` and `:`",
        ));
    }
    if matches!(name, "lt" | "gt" | "amp" | "quot" | "apos") {
        return Err(invalid(
            name,
            "that is one of XML's five predefined entities, which may not be redeclared \
             with a different value",
        ));
    }

    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Xml, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the selection cannot be located reliably",
            file.display()
        );
    }

    let doctype = collect_nodes(parsed.root(), |n| n.kind() == "doctypedecl")
        .into_iter()
        .next();
    if let Some(existing) = doctype.and_then(|d| xml_entity_declaration(d, &source, name)) {
        let _ = existing;
        return Err(Refusal::NameCollision {
            existing: name.to_string(),
            file: file.to_path_buf(),
        }
        .into());
    }

    let node = descendant_at(&parsed, span)
        .ok_or_else(|| anyhow::anyhow!("bytes {span} are outside {}", file.display()))?;
    let inner = xml_text_extent(node).ok_or_else(|| {
        anyhow::anyhow!(
            "no attribute value or element text at bytes {span} in {}; an entity stands \
             in for text, so select the text of an attribute value or the character \
             data of an element",
            file.display()
        )
    })?;

    // The selection is clipped to the text it lands in.
    let start = span.start.max(inner.start);
    let end = span.end.min(inner.end).max(start);
    let raw = &source[start..end];
    let lead = raw.len() - raw.trim_start().len();
    let trail = raw.len() - raw.trim_end().len();
    let target = Span::new(start + lead, end.saturating_sub(trail));
    if target.is_empty() {
        anyhow::bail!(
            "the selection at bytes {span} covers no text in {}; select the characters \
             the entity should stand for",
            file.display()
        );
    }
    let value_text = target.text(&source).to_string();

    if value_text.contains(['<', '&', '%']) {
        anyhow::bail!(
            "`{value_text}` contains `<`, `&` or `%`. Those are markup inside an entity \
             value and would be re-parsed and not copied, so the entity would not \
             stand for the same text"
        );
    }
    let quote = match (value_text.contains('"'), value_text.contains('\'')) {
        (false, _) => '"',
        (true, false) => '\'',
        (true, true) => anyhow::bail!(
            "`{value_text}` contains both `\"` and `'`, so there is no quote character \
             left to write the entity declaration with"
        ),
    };

    let mut targets = vec![target];
    if all_occurrences {
        for node in collect_nodes(parsed.root(), |n| {
            matches!(n.kind(), "AttValue" | "CharData")
        }) {
            let Some(extent) = xml_text_extent(node) else {
                continue;
            };
            let text = extent.text(&source);
            let mut base = 0usize;
            while let Some(found) = text[base..].find(&value_text) {
                let at = extent.start + base + found;
                targets.push(Span::new(at, at + value_text.len()));
                base += found + value_text.len();
            }
        }
    }
    targets.sort();
    targets.dedup();

    let declaration = format!("<!ENTITY {name} {quote}{value_text}{quote}>");
    let (insert_at, insert_text) = match doctype {
        Some(doctype) => {
            let declarations = named_children_of_kind(doctype, "GEDecl");
            match declarations.last() {
                Some(last) => {
                    let indent = line_indent(&source, last.start_byte());
                    (last.end_byte(), format!("\n{indent}{declaration}"))
                }
                None => match child_of_kind(doctype, "[") {
                    // An internal subset that is present but empty.
                    Some(bracket) => (bracket.end_byte(), format!("\n  {declaration}\n")),
                    // `<!DOCTYPE root>`, the subset has to be opened first.
                    None => (
                        doctype.end_byte().saturating_sub(1),
                        format!(" [\n  {declaration}\n]"),
                    ),
                },
            }
        }
        None => {
            let root = xml_root_element(&parsed).ok_or_else(|| {
                anyhow::anyhow!(
                    "{} has no root element, so a `<!DOCTYPE>` cannot name one",
                    file.display()
                )
            })?;
            let root_name = child_of_kind(root, "STag")
                .or_else(|| child_of_kind(root, "EmptyElemTag"))
                .and_then(|tag| child_of_kind(tag, "Name"))
                .map(|n| Span::from(n).text(&source).to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "the root element of {} has no name to write into the \
                         `<!DOCTYPE>`",
                        file.display()
                    )
                })?;
            (
                full_line_span(&source, root.start_byte()).start,
                format!("<!DOCTYPE {root_name} [\n  {declaration}\n]>\n"),
            )
        }
    };

    let mut edits = EditSet::new();
    edits.add(
        file.to_path_buf(),
        Edit::new(
            Span::new(insert_at, insert_at),
            insert_text,
            format!("declare &{name};"),
        ),
    );
    for target in &targets {
        edits.add(
            file.to_path_buf(),
            Edit::new(*target, format!("&{name};"), format!("use &{name};")),
        );
    }

    Ok(ExtractPlan {
        name: name.to_string(),
        expression: value_text,
        edits,
        occurrences: targets.len(),
    })
}

/// An XML `Name`, restricted to the ASCII subset this tool is prepared to rewrite.
fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

/// The text a node holds: an attribute value without its quotes, or character data.
fn xml_text_extent(node: Node<'_>) -> Option<Span> {
    let span = Span::from(node);
    match node.kind() {
        "AttValue" if span.len() >= 2 => Some(Span::new(span.start + 1, span.end - 1)),
        "CharData" => Some(span),
        _ => None,
    }
}

/// The `<!ENTITY name …>` declaration for `name`, if the doctype has one.
fn xml_entity_declaration<'a>(doctype: Node<'a>, source: &str, name: &str) -> Option<Node<'a>> {
    named_children_of_kind(doctype, "GEDecl")
        .into_iter()
        .find(|d| child_of_kind(*d, "Name").is_some_and(|n| Span::from(n).text(source) == name))
}

/// The document's root element.
fn xml_root_element<'a>(parsed: &'a Parsed) -> Option<Node<'a>> {
    let root = parsed.root();
    let mut cursor = root.walk();
    let found = root
        .named_children(&mut cursor)
        .find(|c| c.kind() == "element");
    found
}
