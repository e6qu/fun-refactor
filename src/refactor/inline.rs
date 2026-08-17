//! Inline a variable: replace its uses with its value and remove the binding.
//!
//! Inlining is only safe when the answer is provably the same afterwards. So the preconditions
//! are checked and refused, not assumed: the binding must be assigned exactly once, every use
//! must resolve to it. No name inside its value may mean something different at a use site
//! (PLAN.md D8).

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
    if let Some(language) = index.symbol(symbol).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::InlineVariable, language);
    }
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    // The config languages name values through their own constructs; each has its own
    // inliner, mirroring the extraction that produced it.
    match (sym.language, sym.kind) {
        (Language::Hcl, SymbolKind::Variable) => return hcl_local(index, symbol),
        (Language::Yaml | Language::Helm, SymbolKind::Anchor) => return yaml_anchor(index, symbol),
        (Language::Css | Language::Scss, SymbolKind::Property) => {
            return css_custom_property(index, symbol)
        }
        (Language::Markdown, SymbolKind::LinkDef) => {
            return markdown_link_definition(index, symbol)
        }
        // Shell quoting decides what a substitution means, so bash cannot go through
        // the generic path, which would splice a value into `${…}` and change it.
        (Language::Bash, SymbolKind::Variable | SymbolKind::Constant) => {
            return bash_variable(index, symbol)
        }
        // XML's binding form is the internal-subset entity. See `xml_entity` for why
        // this arm is not reachable from the CLI yet.
        (Language::Xml, SymbolKind::Constant) => return xml_entity(&sym.file, &sym.name),
        _ => {}
    }

    if !matches!(sym.kind, SymbolKind::Variable | SymbolKind::Constant) {
        anyhow::bail!(
            "'{}' is {}; only variables and constants can be inlined",
            sym.name,
            sym.kind.with_article()
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    // The value bound to the definition.
    let node = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
        .ok_or_else(|| anyhow::anyhow!("could not locate the binding"))?;
    let value = crate::parse::declaration_value(node).ok_or_else(|| {
        anyhow::anyhow!(
            "'{}' has no initialiser, so there is nothing to inline",
            sym.name
        )
    })?;
    let value_span = Span::from(value);
    let value_text = value_span.text(&source).to_string();
    let compound = groups_with_parentheses(sym.language) && !atomic(value);

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "'{}' has no uses; inlining would only delete it. Use `fr delete` if that is the intent",
            sym.name
        );
    }

    // Every use must be provably this binding.
    for reference in &references {
        if !reference.confidence.is_safe_to_rewrite() {
            return Err(Refusal::TooWeak {
                confidence: reference.resolved_confidence(),
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

    // Substituting the value more than once evaluates it more than once. That only
    // matters when evaluating can *do* something: `let v = effect(); v + v` inlined
    // is `effect() + effect()`, the effect twice. `a + b` twice is arithmetic
    // twice, and refusing it swallowed most ordinary inlines.
    if references.len() > 1 && may_run_code(&value_text) {
        anyhow::bail!(
            "'{}' is used {} times and `{}` is not a simple value; \
             inlining would evaluate it more than once",
            sym.name,
            references.len(),
            value_text
        );
    }

    // A name inside the value must mean the same thing at every use site, or the
    // substituted expression would silently bind to something else.
    if let Some(captured) = shadowed_name(index, &value_span, &references, &sym.file) {
        return Err(Refusal::NameCaptured {
            name: captured,
            file: sym.file.clone(),
        }
        .into());
    }

    // A compound value is wrapped only where the use site needs it. Each use sits in
    // its own file and its own expression, so the decision is made per site against
    // that file's parse tree.
    let mut trees: std::collections::HashMap<std::path::PathBuf, (String, crate::parse::Parsed)> =
        std::collections::HashMap::new();
    let mut edits = EditSet::new();
    for reference in &references {
        let bare = if !compound {
            true
        } else if reference.file == sym.file {
            use_site_shielded(&parsed, reference.span)
        } else {
            let (_, tree) = match trees.entry(reference.file.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let text = crate::vfs::read_to_string(&reference.file)?;
                    let tree = Parsers::new().parse(reference.language, &text)?;
                    entry.insert((text, tree))
                }
            };
            use_site_shielded(tree, reference.span)
        };
        let replacement = if bare {
            value_text.clone()
        } else {
            format!("({value_text})")
        };
        edits.add(
            reference.file.clone(),
            Edit::new(reference.span, replacement, format!("inline {}", sym.name)),
        );
    }

    // Remove the binding, taking its whole line when nothing else is on it.
    //
    // A Java declarator is the symbol and the `int` in front of it is not, so removing
    // the symbol's own span left `int ;` behind. The statement goes too, but only where
    // this declarator is the only one in it, because `int a = 1, b = 2, c = 3;` declares
    // three and inlining one must leave the other two alone.
    let binding = sole_declarator_statement(&parsed, sym.full_span).unwrap_or(sym.full_span);
    let line = full_line_span(&source, binding.start);
    let removal = if line.text(&source).trim() == binding.text(&source).trim() {
        line
    } else {
        binding
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

/// The value as it must read at a use site.
///
/// `b = a + 1; return b * 2` inlined to `return a + 1 * 2` computes `a + 2`. Every
/// language with an expression grammar had this.
///
/// The rule errs toward a parenthesis: a pair around an expression never changes its
/// meaning, whereas deciding precedence properly needs a per-grammar operator table
/// that would be wrong somewhere, silently. Left bare: the things no surrounding
/// operator can split, a name, a literal, a call, a field, an index, and anything
/// already wrapped. A use site that is itself delimited is also left bare, because
/// `total = value` holds the whole value however it associates. See [`shielded`].
///
/// Languages without an expression grammar are untouched. A YAML value is not an
/// expression, and `(true)` is not the same scalar as `true`.
/// Does this language group a sub-expression by writing it in parentheses?
///
/// [`substitution`] used to ask whether the language supported extract-variable, whose
/// answer only mostly overlaps. Java groups with parentheses like every C-shaped
/// language here but is absent from that list, having no inferred declaration to
/// extract into. Bash supports the extraction, but `( … )` there opens a subshell.
pub(crate) fn groups_with_parentheses(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::Java
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
    )
}

fn substitution(language: Language, value: tree_sitter::Node<'_>, text: &str) -> String {
    if !groups_with_parentheses(language) || atomic(value) {
        return text.to_string();
    }
    format!("({text})")
}

/// Can no surrounding operator split this expression?
fn atomic(value: tree_sitter::Node<'_>) -> bool {
    const ATOMIC: &[&str] = &[
        "identifier",
        "literal",
        "number",
        "integer",
        "float",
        "string",
        "char",
        "true",
        "false",
        "null",
        "nil",
        "none",
        "self",
        "this",
        "call",
        "invocation",
        "field",
        "selector",
        "attribute",
        "member",
        "subscript",
        "index",
        "parenthes",
    ];
    let kind = value.kind();
    ATOMIC.iter().any(|atom| kind.contains(atom))
}

/// Does the surrounding syntax already hold the whole substituted expression?
///
/// `total = value`, `f(value)`, `return value` and a list element between commas are
/// delimited by their own punctuation. A compound value keeps its meaning there bare,
/// and a parenthesis is noise. `value * 2` is not delimited; the multiplication
/// would bind into the value, so the wrap stays. The words match tree-sitter node
/// kinds across all seven grammars. A parent this list does not recognise keeps
/// the parenthesis, which at worst reads as noise and never changes what runs.
fn shielded(parent_kind: &str) -> bool {
    const SHIELDS: &[&str] = &[
        "statement",
        "declaration",
        "declarator",
        "argument",
        "return",
        "parenthes",
        "array",
        "tuple",
        "list",
        "dictionary",
        "map",
        "set",
        "pair",
        "keyword",
        "assignment",
        "initializer",
        "element",
        "block",
        "substitution",
        "interpolation",
    ];
    SHIELDS.iter().any(|word| parent_kind.contains(word))
}

/// True when the node covering `span` sits in a position its own delimiters protect.
fn use_site_shielded(parsed: &Parsed, span: Span) -> bool {
    let Some(node) = node_covering(parsed, span) else {
        return false;
    };
    match node.parent() {
        Some(parent) => shielded(parent.kind()),
        None => true,
    }
}

fn other_assignment(
    index: &Index,
    symbol: SymbolId,
    source: &str,
    parsed: &crate::parse::Parsed,
    name: &str,
) -> Option<usize> {
    let sym = index.symbol(symbol)?;
    // A later definition of the same name *in the same scope* is a rebinding. In a different
    // scope it is a different variable that happens to share a name, which is most of them.
    // 6,166 of this repository's 9,147 locals share a name with another local in the same file,
    // and refusing on all of them refused nearly every inline anyone would want.
    let rebound = index
        .find_symbols(name, Some(&sym.file))
        .into_iter()
        .find(|s| s.id != symbol && s.scope == sym.scope && s.full_span.start > sym.full_span.end)
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

/// Which symbol does this name mean, read from inside this scope?
///
/// The innermost enclosing scope that declares it wins, which "lexical scope" means.
/// `None` says no scope on the chain declares it, a module-level name, an import, a builtin.
/// Two `None`s agree with each other: the name means whatever it means everywhere in the file.
fn meaning_of(
    index: &Index,
    info: &crate::index::FileInfo,
    scope: Option<crate::model::ScopeId>,
    name: &str,
) -> Option<SymbolId> {
    let scope = scope?;
    for enclosing in info.scope_chain(scope) {
        if let Some(symbol) = info
            .symbols
            .iter()
            .filter_map(|id| index.symbol(*id))
            .find(|s| s.name == name && s.scope == enclosing)
        {
            return Some(symbol.id);
        }
    }
    None
}

/// A name inside the value that would mean something else at a use site.
///
/// Substituting an expression moves every name in it to wherever the variable was used. A name
/// that resolves to a different binding there is a silent change of behaviour.
///
/// Asked of the lexical scopes the index already records. It used to be asked of whichever
/// reference with the same name happened to come first within two hundred bytes of the use
/// site, which is not a question about scope at all. In a seven-line file every reference is
/// within two hundred bytes. So inlining `total = price_of(order)` was refused because the
/// *other* function's parameter is also called `order`. The scope-aware answer was computed on
/// the line above and thrown away.
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
        let here = meaning_of(
            index,
            info,
            info.scope_at(name_ref.span.start),
            &name_ref.name,
        );
        for use_site in references {
            if use_site.file != *file {
                // The value would move to another file, where its names may mean
                // anything at all.
                return Some(name_ref.name.clone());
            }
            let there = meaning_of(
                index,
                info,
                info.scope_at(use_site.span.start),
                &name_ref.name,
            );
            if here != there {
                return Some(name_ref.name.clone());
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

/// The declaration statement a lone declarator belongs to.
///
/// Java and the C family write the type once and the bindings after it. So the symbol is `total
/// = g()` and the statement is `int total = g();`. Removing the symbol alone leaves the type
/// stranded. Where the statement declares more than one name the symbol is all that may go, and
/// this answers `None`.
fn sole_declarator_statement(parsed: &crate::parse::Parsed, symbol: Span) -> Option<Span> {
    let node = parsed
        .root()
        .descendant_for_byte_range(symbol.start, symbol.end)?;
    if Span::from(node) != symbol || !node.kind().contains("declarator") {
        return None;
    }
    let parent = node.parent()?;
    let mut cursor = parent.walk();
    let siblings = parent
        .named_children(&mut cursor)
        .filter(|c| c.kind() == node.kind())
        .count();
    (siblings == 1).then(|| Span::from(parent))
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
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn apply(plan: &InlinePlan, path: &Path) -> String {
        let original = crate::vfs::read_to_string(path).unwrap();
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
        // Both uses sit alone inside an argument list, whose own parentheses and
        // commas already hold the expression, so no extra pair appears.
        assert_eq!(
            apply(&plan, &path),
            "fn f() {\n    p(a + b);\n    q(a + b);\n}\n"
        );
    }

    #[test]
    fn a_declaration_value_needs_no_brackets() {
        let src = "fn f(w: usize, h: usize) -> usize {\n    let base = w * 2 + h * 3;\n    \
                   let scaled = base;\n    scaled\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let base").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        // The declaration's `=` and `;` hold the whole value; a pair here is noise.
        assert!(
            apply(&plan, &path).contains("let scaled = w * 2 + h * 3;"),
            "got:\n{}",
            apply(&plan, &path)
        );
    }

    #[test]
    fn a_use_under_a_tighter_operator_keeps_its_brackets() {
        let src = "fn f(w: usize, h: usize) -> usize {\n    let sum = w + h;\n    sum * 2\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let sum").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        // Without the pair the `*` pulls `h` in and the arithmetic changes.
        assert!(
            apply(&plan, &path).contains("(w + h) * 2"),
            "got:\n{}",
            apply(&plan, &path)
        );
    }

    #[test]
    fn parenthesises_only_where_the_use_site_needs_it() {
        let src = "fn f() {\n    let n = a + b;\n    p(n);\n    let m = n * 2;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = var_at(&index, &path, src.find("let n").unwrap() + 4);

        let plan = variable(&index, id).unwrap();
        // The argument list protects the first use. The second sits under a `*`
        // that would otherwise pull `b` into the multiplication.
        assert_eq!(
            apply(&plan, &path),
            "fn f() {\n    p(a + b);\n    let m = (a + b) * 2;\n}\n"
        );
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
    fn extract_then_inline_returns_the_original_expression() {
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
        crate::vfs::write(&path, &after_extract).unwrap();

        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        let index2 = Index::build_from_scan(&scanned).unwrap();
        let id = var_at(
            &index2,
            &path,
            after_extract.find("let subtotal").unwrap() + 4,
        );

        let inlined = variable(&index2, id).unwrap();
        let after_inline =
            apply_to_string(&after_extract, inlined.edits.edits_for(&path).unwrap()).unwrap();

        // Not byte-for-byte: what comes back is the original with the substituted expression in
        // parentheses. That is the price of not having a precedence table, it never changes
        // what the code does. It is worth saying out loud instead of hiding behind a comparison
        // that strips them.
        assert_eq!(
            after_inline,
            "fn f() {\n    let total = (price * quantity) + 10;\n}\n"
        );
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
/// Only calls whose result is provably identical are inlined. gopls's inliner exists to
/// preserve evaluation order, effects and shadowing across arbitrary bodies. This one takes the
/// conservative half of that problem, a single-expression callee whose arguments cannot be
/// duplicated unsafely, and refuses everything else by name.
pub fn call(index: &Index, file: &std::path::Path, offset: usize) -> Result<InlineCallPlan> {
    if let Some(info) = index.file(file) {
        crate::capabilities::record(crate::capabilities::Capability::InlineCall, info.language);
    }
    let reference = index
        .reference_at(file, offset)
        .ok_or_else(|| anyhow::anyhow!("no call at that position"))?;
    if reference.kind != crate::model::ReferenceKind::Call {
        anyhow::bail!("'{}' at that position is not a call", reference.name);
    }
    if !reference.confidence.is_safe_to_rewrite() {
        return Err(Refusal::TooWeak {
            confidence: reference.resolved_confidence(),
            detail: format!(
                "the callee of '{}' was not resolved conclusively",
                reference.name
            ),
        }
        .into());
    }

    let callee = reference
        .target
        .and_then(|t| index.symbol(t))
        .ok_or_else(|| anyhow::anyhow!("'{}' does not resolve to a definition", reference.name))?;

    // Inlining a function into itself would not terminate.
    if callee.file == *file && callee.full_span.contains_offset(offset) {
        anyhow::bail!(
            "'{}' calls itself here; inlining would not terminate",
            callee.name
        );
    }

    let callee_source = crate::vfs::read_to_string(&callee.file)?;
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
    let caller_source = crate::vfs::read_to_string(file)?;
    let caller_parsed = Parsers::new().parse(reference.language, &caller_source)?;
    let call_node = enclosing_call(&caller_parsed, reference.span)
        .ok_or_else(|| anyhow::anyhow!("could not locate the call expression"))?;
    let call_span = Span::from(call_node);

    let parameters = parameter_names(declaration, &callee_source);
    let arguments = arguments_at(call_node, &caller_source, callee.language);
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
        if uses > 1 && !is_duplicable(&argument.text) {
            let text = &argument.text;
            anyhow::bail!(
                "'{parameter}' is used {uses} times in the body and the argument \
                 `{text}` is not a simple value; inlining would evaluate it more \
                 than once"
            );
        }
    }

    // The body may bind an argument more tightly than the caller wrote it: `n * 2`
    // with `x + 1` for `n` is `x + 1 * 2`, which is a different number.
    let grouped: Vec<String> = arguments.iter().map(|a| a.grouped.clone()).collect();
    let mut expansion = substitute_words(body_text, &parameters, &grouped);
    // The body was an expression in its own right; parenthesise it so it keeps its
    // meaning inside whatever expression the call sat in. When the call site's own
    // delimiters already hold it, `x = (a + b)` is only noise, so the pair stays off.
    let exposed = match call_node.parent() {
        Some(parent) => !shielded(parent.kind()),
        None => false,
    };
    if exposed && needs_parentheses(&expansion) {
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
fn single_expression_body<'a>(declaration: tree_sitter::Node<'a>, source: &str) -> Option<Span> {
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
    // this in a loop instead of a fixed order keeps the `return` keyword from being
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
/// One argument at a call site, in the two forms this needs it.
struct Argument {
    /// As written, for reporting and for deciding whether it may be duplicated.
    text: String,
    /// As it goes into the body, grouped when the body could bind it more tightly
    /// than the caller wrote it.
    grouped: String,
}

fn arguments_at(call: tree_sitter::Node<'_>, source: &str, language: Language) -> Vec<Argument> {
    argument_nodes(call)
        .into_iter()
        .map(|node| {
            let text = Span::from(node).text(source).trim().to_string();
            let grouped = substitution(language, node, &text);
            Argument { text, grouped }
        })
        .collect()
}

fn argument_nodes(call: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
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
        return children.into_iter().skip(1).collect();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
        .collect()
}

/// The call expression containing `span`.
fn enclosing_call<'a>(
    parsed: &'a crate::parse::Parsed,
    span: Span,
) -> Option<tree_sitter::Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..8 {
        // Every grammar in the set spells it with "call" except Java, which says
        // `method_invocation`. Matching on "call" alone meant the capability table
        // claimed `inline --call` for Java while the operation could not find a single
        // call in the language.
        let kind = node.kind();
        if kind.contains("call") || kind.contains("invocation") {
            return Some(node);
        }
        node = node.parent()?;
    }
    None
}

/// Can a call be inlined in this language?
///
/// A predicate the capability table asks, instead of a guess from the language's
/// class. `InlineCall` was the one cell derived from "is it imperative", so adding a
/// language to the enum claimed the capability for it before a line was written.
pub fn supports_call(language: crate::lang::Language) -> bool {
    use crate::lang::Language as L;
    matches!(
        language,
        L::Rust | L::Go | L::Zig | L::TypeScript | L::Tsx | L::Python | L::Java
    )
}

/// May this value be substituted more than once without changing behaviour?
fn is_duplicable(argument: &str) -> bool {
    // A bare name or a literal has no effects and costs nothing to repeat.
    let trimmed = argument.trim();
    if whole_string_literal(trimmed) {
        return true;
    }
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '"')
        && !trimmed.contains('(')
}

/// Could evaluating this text run something, rather than only read values?
///
/// A `(` that follows a name, an index or another call is a call about to happen; a
/// macro's `!(`, an `await`, a `new` and the mutating `++`/`--` are the same hazard
/// in other spellings. A `(` after an operator is only grouping. Text inside a
/// string literal does not run, so one whole literal is exempt before any of this.
fn may_run_code(value: &str) -> bool {
    let trimmed = value.trim();
    if whole_string_literal(trimmed) {
        return false;
    }
    for (at, c) in trimmed.char_indices() {
        if c == '(' {
            let before = trimmed[..at].trim_end().chars().last();
            if matches!(before, Some(b) if b.is_alphanumeric() || matches!(b, '_' | ']' | ')' | '!' | '?'))
            {
                return true;
            }
        }
    }
    trimmed.contains("await ")
        || trimmed.contains("new ")
        || trimmed.contains("++")
        || trimmed.contains("--")
}

/// One double-quoted literal and nothing after it.
///
/// `"hello world"` repeats safely; the character test above rejects its space, and a
/// message string is the most ordinary constant there is. `"a" + f()` also begins and
/// ends with a quote, so the interior has to prove the first literal runs to the end.
fn whole_string_literal(text: &str) -> bool {
    let Some(interior) = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .filter(|_| text.len() >= 2)
    else {
        return false;
    };
    let mut escaped = false;
    for c in interior.chars() {
        if !escaped && c == '"' {
            return false;
        }
        escaped = !escaped && c == '\\';
    }
    true
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

/// Does the expansion need wrapping to survive its new context? Is the whole expression inside
/// one pair of brackets?
///
/// `(a + b)` is. `(a + 1) / (b - 1)` is not. Reading only the first and last character says it
/// is, which left `2 * scale(p + 1, q - 1)` expanding to `2 * (p + 1) / (q - 1)`. For `p = 1, q
/// = 4` the call returns 0 and the expansion is 1.
fn wrapped_in_one_group(text: &str) -> bool {
    if !text.starts_with('(') || !text.ends_with(')') {
        return false;
    }
    let mut depth = 0usize;
    for (offset, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                // Unbalanced text is nothing this can reason about, so it is treated
                // as needing the brackets, and not as already having them.
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return false,
                };
                if depth == 0 {
                    return offset + character.len_utf8() == text.len();
                }
            }
            _ => {}
        }
    }
    false
}

fn needs_parentheses(expansion: &str) -> bool {
    let trimmed = expansion.trim();
    // A single token or an already-bracketed expression is safe as-is.
    if trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return false;
    }
    if wrapped_in_one_group(trimmed) {
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
    parsed.descendant_at(span.start, span.end)
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
/// This makes an extraction reversible, the blank line an extraction wrote
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
///
/// A construct need not fit on one line. An HCL local holding a multi-line object
/// begins on the line its name is on and ends several lines below, so the line before
/// it and the line after it are two different lines, reading both from `inner.start`
/// asked for `source[end..start]` and panicked.
fn tight_removal_span(source: &str, inner: Span) -> Span {
    let end = inner.end.min(source.len());
    let first = full_line_span(source, inner.start);
    let last = full_line_span(source, end);
    if source[first.start..inner.start].trim().is_empty()
        && source[end..last.end.max(end)].trim().is_empty()
    {
        return Span::new(first.start, last.end.max(end));
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
    let source = crate::vfs::read_to_string(&sym.file)?;
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
            "local.{} has no uses; inlining would only delete it. Use `fr delete` if \
             that is the intent",
            sym.name
        );
    }
    for reference in &references {
        if !reference.confidence.is_safe_to_rewrite() {
            return Err(Refusal::TooWeak {
                confidence: reference.resolved_confidence(),
                detail: format!(
                    "a use of local.{} at {}:{} did not resolve conclusively. The \
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
                let text = crate::vfs::read_to_string(&reference.file)?;
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

        // Terraform has no operator precedence rescue: an inlined sum inside a product would
        // bind differently. So it is grouped when it is not the whole expression.
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
            .or_insert_with(|| crate::vfs::read_to_string(&reference.file).unwrap_or_default());
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
    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;

    let node = node_covering(&parsed, sym.full_span)
        .ok_or_else(|| anyhow::anyhow!("could not locate the anchor '&{}'", sym.name))?;
    let anchor = named_children_of_kind(node, "anchor")
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("'&{}' is not an anchor node", sym.name))?;

    let mut value_start = anchor.end_byte();
    while value_start < sym.full_span.end && source.as_bytes()[value_start].is_ascii_whitespace() {
        value_start += 1;
    }
    if value_start >= sym.full_span.end {
        anyhow::bail!(
            "'&{}' anchors an empty node, so there is nothing to inline",
            sym.name
        );
    }
    let value_text = source[value_start..sym.full_span.end]
        .trim_end()
        .to_string();
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

    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} does not parse cleanly{}; the declaration cannot be located reliably",
            sym.file.display(),
            if sym.language == Language::Scss {
                ". Check for SCSS syntax its grammar does not yet cover, such as \
                 empty `@mixin m()` parentheses or a namespaced `@include t.m(…)`"
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
            "'{}' has no `var()` uses; inlining would only delete it. Use `fr delete` \
             if that is the intent",
            sym.name
        );
    }

    // An SCSS `$variable` is used bare. It is not wrapped in `var()`. So its uses are the reference
    // spans themselves and the declaration is a plain top-level statement.
    if sym.name.starts_with('$') {
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
        let line = full_line_span(&source, sym.full_span.start);
        let removal = if line.text(&source).trim() == sym.full_span.text(&source).trim() {
            // Take the blank line the declaration left behind with it.
            let mut end = line.end;
            if end < source.len() {
                let next = full_line_span(&source, end);
                if next.text(&source).trim().is_empty() {
                    end = next.end;
                }
            }
            Span::new(line.start, end)
        } else {
            sym.full_span
        };
        edits.add(
            sym.file.clone(),
            Edit::new(removal, "", format!("remove {}", sym.name)),
        );

        return Ok(InlinePlan {
            name: sym.name.clone(),
            value: value_text,
            edits,
            use_sites: references.len(),
        });
    }

    let mut edits = EditSet::new();
    let mut parsed_cache: std::collections::HashMap<PathBuf, (String, Parsed)> =
        std::collections::HashMap::new();
    for reference in &references {
        let entry = match parsed_cache.entry(reference.file.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let text = crate::vfs::read_to_string(&reference.file)?;
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
    let source = crate::vfs::read_to_string(&sym.file)?;
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
            "'[{}]' has no reference links; inlining would only delete it. Use \
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
            .and_then(markdown_link_ancestor)
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

/// The innermost Markdown link enclosing `node`.
///
/// A reference link is a node of the inline grammar, and it has one kind per
/// spelling: `[t][label]`, `[label][]` and `[label]` are three different nodes.
fn markdown_link_ancestor(node: Node<'_>) -> Option<Node<'_>> {
    const KINDS: [&str; 4] = [
        "inline_link",
        "full_reference_link",
        "collapsed_reference_link",
        "shortcut_link",
    ];
    let mut current = node;
    loop {
        if KINDS.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// The lines a link reference definition occupies, plus the blank line before it when
/// it is the last thing in the document, the separator an extraction wrote.
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

// -------------------------------------------------------------------------- Bash

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

/// The innermost *strict* ancestor of `node` with this kind.
fn strict_ancestor_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut current = node.parent()?;
    loop {
        if current.kind() == kind {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Inline a shell variable: substitute its value at every `$name` / `${name}` and delete the
/// assignment.
///
/// Three shapes refuse here that other languages' inlining accepts:
///
/// * Shell has no block scope, so a second assignment anywhere in the file changes what every
///   later use sees; one substitution cannot serve both.
/// * Every child process this script starts reads an `export`ed variable, and nothing in the
///   workspace shows whether one wants it.
/// * `'$name'` inside single quotes is not a use, the shell expands nothing there. So deleting
///   the assignment would leave text that looks like a use. Reported.
///
/// Quoting decides the substitution, mirroring the extraction: a use inside double quotes takes
/// a quoted value's contents. An unquoted use takes a quoted value only when its contents are a
/// single plain word, since otherwise the shell would word-split and glob-expand it where
/// `$name` never was.
fn bash_variable(index: &Index, symbol: SymbolId) -> Result<InlinePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    if sym.exported {
        anyhow::bail!(
            "`{}` is exported, so it is part of the environment of every command this \
             script runs. Inlining it here would take it out of that environment, and \
             nothing in this workspace can show that no child process reads it",
            sym.name
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the declaration cannot be located reliably",
            sym.file.display()
        );
    }

    let assignment = node_covering(&parsed, sym.full_span)
        .and_then(|n| ancestor_of_kind(n, "variable_assignment"))
        .filter(|a| Span::from(*a) == sym.full_span)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`{}` is not bound by an assignment. A `for` loop variable and a bare \
                 `local {}` have no value to substitute",
                sym.name,
                sym.name
            )
        })?;

    // `FOO=bar cmd` sets FOO for that one command only; `$FOO` anywhere else is a different
    // variable. Removing the prefix would change the command's environment and not inline
    // anything.
    if assignment.parent().is_some_and(|p| p.kind() == "command") {
        anyhow::bail!(
            "`{}=…` is a prefix of a single command, so it is visible only inside that \
             command's environment; a `${}` elsewhere is not a use of it",
            sym.name,
            sym.name
        );
    }

    let value = assignment.child_by_field_name("value").ok_or_else(|| {
        anyhow::anyhow!(
            "`{}=` binds the empty string; there is nothing to inline",
            sym.name
        )
    })?;
    let value_text = Span::from(value).text(&source).to_string();

    // Shell has no block scope: a later assignment changes every use after it.
    if let Some(other) = bash_other_binding(&parsed, &source, &sym.name, sym.full_span) {
        let pos = LineIndex::new(&source).line_col(other, &source);
        anyhow::bail!(
            "`{}` is assigned again at line {}. Shell has no block scope, so every use \
             after that line reads the second value and one substitution cannot be \
             right for both",
            sym.name,
            pos.line
        );
    }

    if let Some(quoted) = bash_single_quoted_mention(&parsed, &source, &sym.name) {
        anyhow::bail!(
            "`{}` at bytes {} is inside single quotes, where the shell expands nothing. \
             That text is a literal `${}` and not a use. Removing the assignment \
             would leave it reading like one",
            Span::from(quoted).text(&source),
            Span::from(quoted),
            sym.name
        );
    }

    let references = index.references_to(symbol);
    if references.is_empty() {
        anyhow::bail!(
            "`{}` has no uses; inlining would only delete it. Use `fr delete` if that \
             is the intent",
            sym.name
        );
    }
    for reference in &references {
        if !reference.confidence.is_safe_to_rewrite() {
            return Err(Refusal::TooWeak {
                confidence: reference.resolved_confidence(),
                detail: format!(
                    "a use of `{}` at {}:{} did not resolve conclusively",
                    sym.name,
                    reference.file.display(),
                    LineIndex::new(&source)
                        .line_col(reference.span.start, &source)
                        .line
                ),
            }
            .into());
        }
        // Shell's namespace is global across everything a script sources. So a use in another
        // file may have been set by a third script between the two.
        if reference.file != sym.file {
            anyhow::bail!(
                "`{}` is used in {}, a different script. Shell variables live in one \
                 global namespace shared by everything a run sources, so what that use \
                 reads depends on the order the scripts run in, which is not visible \
                 here",
                sym.name,
                reference.file.display()
            );
        }
    }

    let mut edits = EditSet::new();
    for reference in &references {
        let use_node = node_covering(&parsed, reference.span).ok_or_else(|| {
            anyhow::anyhow!(
                "a use of `{}` at byte {} could not be located in the tree",
                sym.name,
                reference.span.start
            )
        })?;
        let expansion = bash_expansion_of(use_node, &source, &sym.name)?;
        let (target, replacement) = match bash_redundant_quotes(expansion, &source, &sym.name) {
            // `x="$name"` on the right of an assignment is one word however it is written, so
            // the quotes may go with the expansion and the value keeps its own, which makes an
            // extraction reversible byte for byte.
            Some(quoted) => (quoted, Span::from(value).text(&source).to_string()),
            None => (
                Span::from(expansion),
                bash_substitution(expansion, value, &source, &sym.name)?,
            ),
        };
        edits.add(
            reference.file.clone(),
            Edit::new(target, replacement, format!("inline {}", sym.name)),
        );
    }

    // `local x=…` and `declare -i n=…` own the line, not just the assignment; taking
    // only the assignment would leave a bare `local` behind.
    let removal_target = assignment
        .parent()
        .filter(|p| p.kind() == "declaration_command")
        .filter(|p| named_children_of_kind(*p, "variable_assignment").len() == 1)
        .map(Span::from)
        .unwrap_or(sym.full_span);
    edits.add(
        sym.file.clone(),
        Edit::new(
            tight_removal_span(&source, removal_target),
            "",
            format!("remove binding of {}", sym.name),
        ),
    );

    Ok(InlinePlan {
        name: sym.name.clone(),
        value: value_text,
        edits,
        use_sites: references.len(),
    })
}

/// Another binding of the same name: a second assignment, or a `for` loop that
/// rebinds it. Returns the offset of the rebinding.
fn bash_other_binding(
    parsed: &Parsed,
    source: &str,
    name: &str,
    declaration: Span,
) -> Option<usize> {
    collect_nodes(parsed.root(), |n| {
        if Span::from(n) == declaration {
            return false;
        }
        let bound = match n.kind() {
            "variable_assignment" => n.child_by_field_name("name"),
            "for_statement" => n.child_by_field_name("variable"),
            _ => None,
        };
        bound.is_some_and(|b| Span::from(b).text(source) == name)
    })
    .into_iter()
    .next()
    .map(|n| n.start_byte())
}

/// A `$name` or `${name}` written inside single quotes, where it is literal text.
fn bash_single_quoted_mention<'a>(
    parsed: &'a Parsed,
    source: &str,
    name: &str,
) -> Option<Node<'a>> {
    collect_nodes(parsed.root(), |n| n.kind() == "raw_string")
        .into_iter()
        .find(|n| bash_mentions(Span::from(*n).text(source), name))
}

/// Does this text contain `$name` or `${name}` as a whole word?
fn bash_mentions(text: &str, name: &str) -> bool {
    let braced = format!("${{{name}}}");
    if text.contains(&braced) {
        return true;
    }
    let plain = format!("${name}");
    let mut base = 0usize;
    while let Some(found) = text[base..].find(&plain) {
        let after = base + found + plain.len();
        let next = text[after..].chars().next();
        if next.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            return true;
        }
        base = after;
    }
    false
}

/// The `$name` / `${name}` a reference span sits inside.
fn bash_expansion_of<'a>(node: Node<'a>, source: &str, name: &str) -> Result<Node<'a>> {
    let Some(parent) = node.parent() else {
        anyhow::bail!("a use of `{name}` has no enclosing expansion");
    };
    match parent.kind() {
        "simple_expansion" => Ok(parent),
        "expansion" => {
            // `${name:-default}`, `${#name}`, `${name%.c}` and friends do more than read the
            // variable; substituting the value for the whole expansion would drop the operator.
            // Substituting it inside would not parse.
            let text = Span::from(parent).text(source);
            if text == format!("${{{name}}}") {
                Ok(parent)
            } else {
                anyhow::bail!(
                    "the use `{text}` applies a parameter expansion operator to \
                     `{name}`; the value cannot be substituted without dropping it"
                )
            }
        }
        other => anyhow::bail!(
            "a use of `{name}` at byte {} is written as a bare `{other}` and not \
             `${name}` or `${{{name}}}`; refusing to guess what the shell reads there",
            node.start_byte()
        ),
    }
}

/// The span of a `"$name"` whose quotes are doing no work: a string holding nothing but this
/// expansion, standing on the right of an assignment, where the shell splits nothing whatever
/// the value turns out to be.
///
/// Everywhere else the quotes are load-bearing, `"$name"` as a command argument is one word and
/// the value alone might not be. So this returns `None` and the value is spliced inside the
/// quotes instead.
fn bash_redundant_quotes(expansion: Node<'_>, source: &str, name: &str) -> Option<Span> {
    let string = expansion.parent().filter(|p| p.kind() == "string")?;
    let text = Span::from(string).text(source);
    if text != format!("\"${name}\"") && text != format!("\"${{{name}}}\"") {
        return None;
    }
    let assignment = string
        .parent()
        .filter(|p| p.kind() == "variable_assignment")?;
    let value = assignment.child_by_field_name("value")?;
    (value.id() == string.id()).then(|| Span::from(string))
}

/// The exact bytes to write at one use site.
fn bash_substitution(
    use_site: Node<'_>,
    value: Node<'_>,
    source: &str,
    name: &str,
) -> Result<String> {
    let verbatim = Span::from(value).text(source);
    let inner = || {
        let span = Span::from(value);
        source[span.start + 1..span.end - 1].to_string()
    };
    let in_double_quotes = strict_ancestor_of_kind(use_site, "string").is_some();

    if in_double_quotes {
        return match value.kind() {
            // Already a double-quoted literal: its contents keep their meaning when
            // they move inside another pair of double quotes.
            "string" => Ok(inner()),
            "raw_string" => {
                let text = inner();
                if text.contains(['$', '`', '\\', '"']) {
                    anyhow::bail!(
                        "`{name}` holds the single-quoted text `{text}`, which is \
                         literal, but the use site is inside double quotes where `$`, \
                         backtick, `\\` and `\"` are not. Substituting it would change \
                         what the shell reads"
                    );
                }
                Ok(text)
            }
            "ansi_c_string" => anyhow::bail!(
                "`{name}` holds a `$'…'` string, whose escapes are interpreted when the \
                 assignment runs, not where the value is used; there is no spelling of \
                 it that means the same inside double quotes"
            ),
            _ => Ok(verbatim.to_string()),
        };
    }

    match value.kind() {
        "string" | "raw_string" => {
            let text = inner();
            if bash_is_one_plain_word(&text) {
                Ok(text)
            } else {
                anyhow::bail!(
                    "`{name}` holds `{verbatim}` and this use is unquoted, where the \
                     shell splits on `$IFS` and expands globs. `\"$` `{name}\"` never \
                     did either, so there is no substitution here that keeps the \
                     meaning. Quote the use site first"
                )
            }
        }
        _ => Ok(verbatim.to_string()),
    }
}

/// Is this text a single word that survives word splitting and expansion untouched?
fn bash_is_one_plain_word(text: &str) -> bool {
    !text.is_empty()
        && !text.chars().any(|c| {
            c.is_whitespace()
                || matches!(
                    c,
                    '*' | '?'
                        | '['
                        | ']'
                        | '$'
                        | '`'
                        | '\\'
                        | '~'
                        | '{'
                        | '}'
                        | '"'
                        | '\''
                        | '#'
                        | ';'
                        | '&'
                        | '|'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                )
        })
}

// --------------------------------------------------------------------------- XML

/// Inline an XML internal-subset entity: substitute its replacement text at every `&name;` and
/// delete the `<!ENTITY …>`, taking the `<!DOCTYPE …>` an extraction created with it when
/// nothing else is left inside.
///
/// This is the exact inverse of `extract::variable` on an XML file. It is a standalone function
/// and not only a `variable()` arm because an entity has no [`SymbolId`]:
/// `queries/xml/facts.scm` declares element ids and namespace prefixes and nothing else, so no
/// entity reaches the index and the arm in `variable()` cannot fire until that query captures
/// `(GEDecl (Name) @name) @definition.constant` and `(EntityRef (Name) @reference.identifier)`.
pub fn xml_entity(file: &std::path::Path, name: &str) -> Result<InlinePlan> {
    let source = crate::vfs::read_to_string(file)?;
    let parsed = Parsers::new().parse(Language::Xml, &source)?;
    if parsed.has_errors() {
        anyhow::bail!(
            "{} has syntax errors, so the declaration cannot be located reliably",
            file.display()
        );
    }

    let doctype = collect_nodes(parsed.root(), |n| n.kind() == "doctypedecl")
        .into_iter()
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no `<!DOCTYPE … [ … ]>` internal subset, so it declares no \
                 entities",
                file.display()
            )
        })?;

    let declaration = named_children_of_kind(doctype, "GEDecl")
        .into_iter()
        .find(|d| xml_child_name(*d, &source).as_deref() == Some(name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the internal subset of {} declares no `<!ENTITY {name} …>`",
                file.display()
            )
        })?;

    let value_node = named_children_of_kind(declaration, "EntityValue")
        .into_iter()
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("`<!ENTITY {name}>` has no replacement text to substitute")
        })?;
    let value_span = Span::from(value_node);
    let value = source[value_span.start + 1..value_span.end - 1].to_string();

    if value.contains(['&', '%', '<']) {
        anyhow::bail!(
            "`{name}` expands to `{value}`, which contains markup (`&`, `%` or `<`). \
             That is re-parsed where the entity is referenced, so pasting the text in \
             would not mean the same thing"
        );
    }

    let uses: Vec<Node> = collect_nodes(parsed.root(), |n| {
        n.kind() == "EntityRef" && xml_child_name(n, &source).as_deref() == Some(name)
    });
    if uses.is_empty() {
        anyhow::bail!(
            "`&{name};` is never referenced; inlining would only delete the declaration \
. Use `fr delete` if that is the intent"
        );
    }

    let mut edits = EditSet::new();
    for use_site in &uses {
        // Inside an attribute value the delimiter cannot appear unescaped, and an
        // entity reference is the only way to write it there.
        if let Some(attribute) = strict_ancestor_of_kind(*use_site, "AttValue") {
            let quote = source.as_bytes()[attribute.start_byte()] as char;
            if value.contains(quote) {
                anyhow::bail!(
                    "`{name}` expands to `{value}`, which contains the `{quote}` that \
                     delimits the attribute value at byte {}; substituting it would end \
                     the attribute early",
                    attribute.start_byte()
                );
            }
        }
        edits.add(
            file.to_path_buf(),
            Edit::new(
                Span::from(*use_site),
                value.clone(),
                format!("inline &{name};"),
            ),
        );
    }

    // The doctype an extraction created goes away with its last entity; one that
    // carries anything else keeps its shape and loses only this line.
    let mut cursor = doctype.walk();
    let others = doctype
        .named_children(&mut cursor)
        .filter(|c| c.id() != declaration.id() && c.kind() != "Name")
        .count();
    let removal = if others == 0 {
        block_removal_span(&source, Span::from(doctype))
    } else {
        block_removal_span(&source, Span::from(declaration))
    };
    edits.add(
        file.to_path_buf(),
        Edit::new(removal, "", format!("remove <!ENTITY {name}>")),
    );

    Ok(InlinePlan {
        name: name.to_string(),
        value,
        edits,
        use_sites: uses.len(),
    })
}

/// The text of a node's first `Name` child.
fn xml_child_name(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind() == "Name")
        .map(|c| Span::from(c).text(source).to_string());
    found
}
