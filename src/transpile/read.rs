//! Reading a syntax tree into the IR.
//!
//! One reader per language. Each is a walk over named nodes that recognises the
//! constructs the IR has, and wraps everything else in [`Unsupported`] carrying the
//! original text and its line. A reader never guesses: an unrecognised node is
//! reported, not approximated, because a translation that quietly drops a statement is
//! worse than one that says it could not manage it.

use super::ir::*;
use crate::lang::Language;
use crate::span::LineIndex;
use anyhow::{bail, Result};
use tree_sitter::Node;

pub fn read(language: Language, source: &str, root: Node<'_>) -> Result<Module> {
    let lines = LineIndex::new(source);
    let cx = Cx {
        source,
        lines: &lines,
    };
    match language {
        Language::Rust => Ok(rust::module(&cx, root)),
        Language::Python => Ok(python::module(&cx, root)),
        Language::Go => Ok(go::module(&cx, root)),
        Language::TypeScript | Language::Tsx => Ok(typescript::module(&cx, root)),
        other => bail!(
            "there is no reader for {other}: translating out of it would mean inventing \
             what its constructs mean"
        ),
    }
}

/// Everything a reader needs that is not the node itself.
struct Cx<'a> {
    source: &'a str,
    lines: &'a LineIndex,
}

impl Cx<'_> {
    fn text(&self, node: Node<'_>) -> String {
        self.source[node.start_byte()..node.end_byte()].to_string()
    }

    fn line(&self, node: Node<'_>) -> usize {
        self.lines.line_col(node.start_byte(), self.source).line
    }

    fn unsupported(&self, node: Node<'_>) -> Unsupported {
        Unsupported {
            construct: node.kind().to_string(),
            source: self.text(node),
            line: self.line(node),
        }
    }

    fn field<'t>(&self, node: Node<'t>, name: &str) -> Option<Node<'t>> {
        node.child_by_field_name(name)
    }

    fn field_text(&self, node: Node<'_>, name: &str) -> Option<String> {
        self.field(node, name).map(|n| self.text(n))
    }

    /// Named children, which is what every reader below walks.
    fn children<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).collect()
    }
}

/// A call whose *callee* could not be translated is not a call this understands.
///
/// Rendering it as `None()` would be syntactically valid in the target and complete
/// nonsense — `HashMap::new()` became exactly that. Carrying the whole call instead
/// puts the original in front of whoever finishes the file.
fn call_or_carry(cx: &Cx, node: Node<'_>, callee: Expr, args: Vec<Expr>) -> Expr {
    if matches!(callee, Expr::Unsupported(_)) {
        return Expr::Unsupported(cx.unsupported(node));
    }
    Expr::Call {
        callee: Box::new(callee),
        args,
    }
}

/// Does this statement's *own* expression contain something untranslatable?
///
/// Only the statement's own expressions — a condition, a value, a target — never the
/// statements nested inside it. One bad line in a loop body should cost that line, not
/// the loop.
fn has_unsupported_expr(stmt: &Stmt) -> bool {
    // Exhaustive on purpose — no `_` arm. The three cases this originally missed
    // were `MapLit`, `Template` and `Comprehension`, and each produced a silent wrong
    // answer rather than a gap: `session?.user.id` inside an object literal came out
    // as `None.id`, with the original nowhere in the file. A new variant must not be
    // able to join them quietly, so the compiler is made to ask.
    fn bad(e: &Expr) -> bool {
        match e {
            Expr::Unsupported(_) => true,
            Expr::Field { of, .. } => bad(of),
            Expr::Index { of, index } => bad(of) || bad(index),
            Expr::Call { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::Binary { left, right, .. } => bad(left) || bad(right),
            Expr::Unary { operand, .. } => bad(operand),
            Expr::Await(inner) => bad(inner),
            Expr::New { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::InstanceOf { value, ty } => bad(value) || bad(ty),
            Expr::Keyword { value, .. } => bad(value),
            Expr::ListLit(items) => items.iter().any(bad),
            Expr::MapLit(entries) => entries.iter().any(|(k, v)| bad(k) || bad(v)),
            Expr::Template(parts) => parts.iter().any(|part| match part {
                TemplatePart::Expr(e) => bad(e),
                TemplatePart::Text(_) => false,
            }),
            Expr::Comprehension {
                element,
                iterable,
                condition,
                ..
            } => bad(element) || bad(iterable) || condition.as_deref().is_some_and(bad),
            Expr::Int(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Null
            | Expr::Name(_) => false,
        }
    }
    match stmt {
        Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Throw(e) => bad(e),
        Stmt::Let { value: Some(e), .. } => bad(e),
        Stmt::Assign { target, value } => bad(target) || bad(value),
        Stmt::If { condition, .. } | Stmt::While { condition, .. } => bad(condition),
        Stmt::ForEach { iterable, .. } => bad(iterable),
        _ => false,
    }
}

/// A statement this only half understands is a statement it does not understand.
///
/// Rendering the understood half and a placeholder for the rest produced lines like
/// `sums = None` — syntactically fine, semantically a lie, and with the original
/// nowhere in the file. Carrying the whole statement instead puts the source in front
/// of whoever finishes the draft, which is the point.
fn keep_whole(cx: &Cx, node: Node<'_>, built: Stmt) -> Stmt {
    if has_unsupported_expr(&built) || binds_a_pattern(&built) {
        return Stmt::Unsupported(cx.unsupported(node));
    }
    built
}

/// Does this statement bind something that is not a plain name?
///
/// `for (sensor, mean) in …` destructures, and the IR binds one name. Carrying the
/// pattern text through produced `for _, (sensor, mean) := range …`, which Go cannot
/// parse — and would have been wrong even where it did parse. A destructuring is not
/// a binding this understands.
fn binds_a_pattern(stmt: &Stmt) -> bool {
    let plain = |name: &str| {
        !name.is_empty()
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !name.chars().next().is_some_and(|c| c.is_numeric())
    };
    match stmt {
        Stmt::ForEach { binding, .. } => !plain(binding),
        Stmt::Let { name, .. } => !plain(name),
        _ => false,
    }
}

/// Doc comments immediately above a node, in order, stripped of their markers.
fn doc_above(cx: &Cx, node: Node<'_>, markers: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut previous = node.prev_named_sibling();
    while let Some(sibling) = previous {
        if !sibling.kind().contains("comment") {
            break;
        }
        // Only directly above: a blank line between is a comment about something else.
        let between = &cx.source[sibling.end_byte()..node.start_byte()];
        if between.matches('\n').count() > 1 {
            break;
        }
        let text = cx.text(sibling);
        let mut cleaned = text.trim();
        for marker in markers {
            cleaned = cleaned.strip_prefix(marker).unwrap_or(cleaned);
        }
        // A block comment ends as well as begins: `/** Build a greeting. */` left the
        // `*/` in the docstring of every function that came back the other way.
        for terminator in ["*/", "-->"] {
            cleaned = cleaned.strip_suffix(terminator).unwrap_or(cleaned);
        }
        lines.push(cleaned.trim().to_string());
        previous = sibling.prev_named_sibling();
    }
    lines.reverse();
    lines
}

// ------------------------------------------------------------------------- Rust

mod rust {
    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        for child in cx.children(root) {
            match child.kind() {
                "inner_doc_comment_marker" | "line_comment" | "block_comment" => {}
                "use_declaration" => module.items.push(Item::Import {
                    text: cx.text(child),
                    line: cx.line(child),
                }),
                "function_item" => module.items.push(Item::Function(function(cx, child, None))),
                "struct_item" => module.items.push(Item::Record(record(cx, child))),
                "const_item" | "static_item" => {
                    if let Some(c) = constant(cx, child) {
                        module.items.push(Item::Constant(c));
                    }
                }
                // Methods live in an `impl` block, apart from the type they belong to.
                // The IR keeps them with the type, so they are attached here.
                "impl_item" => {
                    let owner = cx
                        .field_text(child, "type")
                        .unwrap_or_else(|| "Self".to_string());
                    let trait_impl = cx.field(child, "trait").is_some();
                    if trait_impl {
                        // `impl Trait for T` is a contract, not a set of methods: the
                        // target language may have no such notion, so it is reported.
                        module.items.push(Item::Unsupported(cx.unsupported(child)));
                        continue;
                    }
                    if let Some(body) = cx.field(child, "body") {
                        for item in cx.children(body) {
                            if item.kind() == "function_item" {
                                module.items.push(Item::Function(function(
                                    cx,
                                    item,
                                    Some(owner.clone()),
                                )));
                            }
                        }
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        module
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let name = cx.field_text(node, "name").unwrap_or_default();
        let mut params = Vec::new();
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                match p.kind() {
                    // `&self` carries the receiver, which the IR records separately.
                    "self_parameter" => {}
                    "parameter" => params.push(Param {
                        name: cx.field_text(p, "pattern").unwrap_or_default(),
                        ty: cx.field(p, "type").map(|t| ty(cx, t)),
                        default: None,
                        kind: ParamKind::Normal,
                    }),
                    _ => params.push(Param {
                        name: cx.text(p),
                        ty: None,
                        default: None,
                        kind: ParamKind::Normal,
                    }),
                }
            }
        }
        Function {
            doc: doc_above(cx, node, &["///", "//!", "//"]),
            name,
            receiver,
            params,
            returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
            is_async: cx.text(node).starts_with("async ") || cx.text(node).contains("async fn"),
        }
    }

    fn record(cx: &Cx, node: Node<'_>) -> Record {
        let mut fields = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for f in cx.children(body) {
                if f.kind() != "field_declaration" {
                    continue;
                }
                fields.push(Field {
                    doc: doc_above(cx, f, &["///", "//"]),
                    name: cx.field_text(f, "name").unwrap_or_default(),
                    ty: cx.field(f, "type").map(|t| ty(cx, t)),
                    exported: f
                        .children(&mut f.walk())
                        .any(|c| c.kind() == "visibility_modifier"),
                });
            }
        }
        Record {
            doc: doc_above(cx, node, &["///", "//"]),
            name: cx.field_text(node, "name").unwrap_or_default(),
            fields,
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
            methods: Vec::new(),
        }
    }

    fn constant(cx: &Cx, node: Node<'_>) -> Option<Constant> {
        Some(Constant {
            doc: doc_above(cx, node, &["///", "//"]),
            name: cx.field_text(node, "name")?,
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            value: cx
                .field(node, "value")
                .map(|v| expr(cx, v))
                .unwrap_or(Expr::Null),
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
        })
    }

    fn ty(cx: &Cx, node: Node<'_>) -> Type {
        let text = cx.text(node);
        super::scalar(&text).unwrap_or_else(|| {
            let trimmed = text.trim();
            // `Vec<T>`, `Option<T>`, `HashMap<K, V>` are the three the IR knows.
            if let Some(inner) = trimmed
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
            {
                return Type::List(Box::new(named_or_scalar(inner)));
            }
            if let Some(inner) = trimmed
                .strip_prefix("Option<")
                .and_then(|s| s.strip_suffix('>'))
            {
                return Type::Optional(Box::new(named_or_scalar(inner)));
            }
            for prefix in ["HashMap<", "BTreeMap<"] {
                if let Some(inner) = trimmed
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix('>'))
                {
                    if let Some((k, v)) = inner.split_once(',') {
                        return Type::Map(
                            Box::new(named_or_scalar(k)),
                            Box::new(named_or_scalar(v)),
                        );
                    }
                }
            }
            let bare = trimmed
                .trim_start_matches('&')
                .trim_start_matches("mut ")
                .trim();
            // `&[T]` is a list, and so is `[T; N]`.
            if let Some(inner) = bare.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let element = inner.split(';').next().unwrap_or(inner);
                return Type::List(Box::new(named_or_scalar(element)));
            }
            if let Some(t) = super::scalar(bare) {
                return t;
            }
            named_with_args(bare, &named_or_scalar)
        })
    }

    fn named_or_scalar(text: &str) -> Type {
        let bare = text
            .trim()
            .trim_start_matches('&')
            .trim_start_matches("mut ")
            .trim();
        super::scalar(bare).unwrap_or_else(|| named_with_args(bare, &named_or_scalar))
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        cx.children(node)
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            // A comment is not an untranslatable construct: every one of these
            // languages has one and only the marker differs. Reading it as a failure
            // put ordinary prose in the output under a "not translated" marker and
            // counted it among the real gaps.
            "comment" | "line_comment" | "block_comment" => {
                Stmt::Comment(super::uncomment(&cx.text(node)))
            }
            "return_expression" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "let_declaration" => Stmt::Let {
                name: cx.field_text(node, "pattern").unwrap_or_default(),
                ty: cx.field(node, "type").map(|t| ty(cx, t)),
                value: cx.field(node, "value").map(|v| expr(cx, v)),
                mutable: cx.text(node).starts_with("let mut "),
            },
            "expression_statement" => match cx.children(node).first() {
                Some(inner) => match inner.kind() {
                    "return_expression"
                    | "if_expression"
                    | "while_expression"
                    | "for_expression"
                    | "assignment_expression" => stmt(cx, *inner),
                    _ => Stmt::Expr(expr(cx, *inner)),
                },
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "assignment_expression" => Stmt::Assign {
                target: cx
                    .field(node, "left")
                    .map(|l| expr(cx, l))
                    .unwrap_or(Expr::Null),
                value: cx
                    .field(node, "right")
                    .map(|r| expr(cx, r))
                    .unwrap_or(Expr::Null),
            },
            "if_expression" => {
                let otherwise = cx
                    .field(node, "alternative")
                    .map(|alt| {
                        // `else if` arrives as an `else_clause` wrapping another `if`.
                        let inner = cx.children(alt);
                        match inner.first() {
                            Some(first) if first.kind() == "if_expression" => {
                                vec![stmt(cx, *first)]
                            }
                            Some(first) if first.kind() == "block" => block(cx, *first),
                            _ => Vec::new(),
                        }
                    })
                    .unwrap_or_default();
                Stmt::If {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    then: cx
                        .field(node, "consequence")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    otherwise,
                }
            }
            "while_expression" => Stmt::While {
                condition: cx
                    .field(node, "condition")
                    .map(|c| expr(cx, c))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            "for_expression" => Stmt::ForEach {
                binding: cx.field_text(node, "pattern").unwrap_or_default(),
                iterable: cx
                    .field(node, "value")
                    .map(|v| expr(cx, v))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            "break_expression" => Stmt::Break,
            "continue_expression" => Stmt::Continue,
            // The tail expression of a block is Rust's implicit return.
            _ if is_expression(node.kind()) => {
                if node.next_sibling().is_none() {
                    Stmt::Return(Some(expr(cx, node)))
                } else {
                    Stmt::Expr(expr(cx, node))
                }
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    fn is_expression(kind: &str) -> bool {
        matches!(
            kind,
            "identifier"
                | "integer_literal"
                | "float_literal"
                | "string_literal"
                | "boolean_literal"
                | "call_expression"
                | "binary_expression"
                | "unary_expression"
                | "field_expression"
                | "index_expression"
        )
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            "await_expression" => match node.named_child(0) {
                Some(inner) => Expr::Await(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "integer_literal" => Expr::Int(cx.text(node)),
            "float_literal" => Expr::Float(cx.text(node)),
            "boolean_literal" => Expr::Bool(cx.text(node) == "true"),
            "string_literal" => Expr::Str(super::unquote(&cx.text(node))),
            "identifier" | "self" => Expr::Name(cx.text(node)),
            "field_expression" => Expr::Field {
                of: Box::new(
                    cx.field(node, "value")
                        .map(|v| expr(cx, v))
                        .unwrap_or(Expr::Null),
                ),
                name: cx.field_text(node, "field").unwrap_or_default(),
            },
            "index_expression" => {
                let parts = cx.children(node);
                Expr::Index {
                    of: Box::new(parts.first().map(|n| expr(cx, *n)).unwrap_or(Expr::Null)),
                    index: Box::new(parts.get(1).map(|n| expr(cx, *n)).unwrap_or(Expr::Null)),
                }
            }
            "call_expression" => call_or_carry(
                cx,
                node,
                cx.field(node, "function")
                    .map(|f| expr(cx, f))
                    .unwrap_or(Expr::Null),
                cx.field(node, "arguments")
                    .map(|a| cx.children(a).iter().map(|n| expr(cx, *n)).collect())
                    .unwrap_or_default(),
            ),
            "binary_expression" => {
                match super::binary_op(&cx.field_text(node, "operator").unwrap_or_default()) {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(
                            cx.field(node, "left")
                                .map(|l| expr(cx, l))
                                .unwrap_or(Expr::Null),
                        ),
                        right: Box::new(
                            cx.field(node, "right")
                                .map(|r| expr(cx, r))
                                .unwrap_or(Expr::Null),
                        ),
                    },
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "unary_expression" => {
                let text = cx.text(node);
                let op = if text.starts_with('!') {
                    Some(UnaryOp::Not)
                } else if text.starts_with('-') {
                    Some(UnaryOp::Neg)
                } else {
                    None
                };
                match (op, cx.children(node).first()) {
                    (Some(op), Some(inner)) => Expr::Unary {
                        op,
                        operand: Box::new(expr(cx, *inner)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "parenthesized_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

// ----------------------------------------------------------------------- Python

mod python {
    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        for child in cx.children(root) {
            match child.kind() {
                "comment" => {}
                "import_statement" | "import_from_statement" | "future_import_statement" => {
                    module.items.push(Item::Import {
                        text: cx.text(child),
                        line: cx.line(child),
                    })
                }
                "function_definition" => {
                    module.items.push(Item::Function(function(cx, child, None)))
                }
                "class_definition" => module.items.push(Item::Record(record(cx, child))),
                // `@dataclass class User:` is the typed-Python idiom for a record, and
                // the decorator used to make the whole class untranslatable.
                "decorated_definition" => {
                    let decorators: Vec<String> = cx
                        .children(child)
                        .iter()
                        .filter(|n| n.kind() == "decorator")
                        .map(|n| cx.text(*n).trim_start_matches('@').trim().to_string())
                        .collect();
                    let inner = cx
                        .children(child)
                        .into_iter()
                        .find(|n| matches!(n.kind(), "class_definition" | "function_definition"));
                    // Only the decorators that describe a *shape*. One that changes
                    // behaviour — a route, a cache, a retry — is not a record and its
                    // meaning would be lost silently.
                    let structural = decorators
                        .iter()
                        .all(|d| matches!(d.as_str(), "dataclass" | "dataclasses.dataclass"));
                    match (inner, structural) {
                        (Some(node), true) if node.kind() == "class_definition" => {
                            module.items.push(Item::Record(record(cx, node)))
                        }
                        (Some(node), true) => {
                            module.items.push(Item::Function(function(cx, node, None)))
                        }
                        _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                "expression_statement" => {
                    // A module docstring, or a module-level constant.
                    let inner = cx.children(child);
                    match inner.first() {
                        Some(n) if n.kind() == "string" && module.items.is_empty() => {
                            module.doc.push(super::unquote(&cx.text(*n)));
                        }
                        Some(n) if matches!(n.kind(), "assignment") => {
                            if let Some(c) = constant(cx, *n) {
                                module.items.push(Item::Constant(c));
                            } else {
                                module.items.push(Item::Unsupported(cx.unsupported(child)));
                            }
                        }
                        _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        module
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let mut params = Vec::new();
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                match p.kind() {
                    // `*` and `/` are rules about the parameters around them; `*args`
                    // and `**kwargs` take the rest. None of the four is an ordinary
                    // parameter, and reading them as one produced signatures no other
                    // language will parse.
                    "positional_separator" | "keyword_separator" => params.push(Param {
                        name: cx.text(p),
                        ty: None,
                        default: None,
                        kind: ParamKind::Marker,
                    }),
                    "list_splat_pattern" | "dictionary_splat_pattern" => params.push(Param {
                        name: cx.text(p).trim_start_matches('*').to_string(),
                        ty: None,
                        default: None,
                        kind: if p.kind() == "list_splat_pattern" {
                            ParamKind::VarArgs
                        } else {
                            ParamKind::KeywordArgs
                        },
                    }),
                    "identifier" => {
                        let name = cx.text(p);
                        if name == "self" || name == "cls" {
                            continue;
                        }
                        params.push(Param {
                            name,
                            ty: None,
                            default: None,
                            kind: ParamKind::Normal,
                        });
                    }
                    "typed_parameter" => {
                        let name = cx
                            .children(p)
                            .first()
                            .map(|n| cx.text(*n))
                            .unwrap_or_default();
                        if name == "self" {
                            continue;
                        }
                        params.push(Param {
                            name,
                            ty: cx.field(p, "type").map(|t| ty(cx, t)),
                            default: None,
                            kind: ParamKind::Normal,
                        });
                    }
                    "default_parameter" | "typed_default_parameter" => params.push(Param {
                        name: cx.field_text(p, "name").unwrap_or_default(),
                        ty: cx.field(p, "type").map(|t| ty(cx, t)),
                        default: cx.field(p, "value").map(|v| expr(cx, v)),
                        kind: ParamKind::Normal,
                    }),
                    _ => params.push(Param {
                        name: cx.text(p),
                        ty: None,
                        default: None,
                        kind: ParamKind::Normal,
                    }),
                }
            }
        }
        let body_node = cx.field(node, "body");
        Function {
            doc: docstring(cx, body_node),
            name: cx.field_text(node, "name").unwrap_or_default(),
            receiver,
            params,
            returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
            body: body_node.map(|b| block(cx, b)).unwrap_or_default(),
            // Python's convention, which is all there is to go on.
            exported: !cx
                .field_text(node, "name")
                .unwrap_or_default()
                .starts_with('_'),
            is_async: cx.text(node).starts_with("async "),
        }
    }

    /// The first string statement of a body, which is Python's doc comment.
    fn docstring(cx: &Cx, body: Option<Node<'_>>) -> Vec<String> {
        let Some(body) = body else {
            return Vec::new();
        };
        let Some(first) = cx.children(body).first().copied() else {
            return Vec::new();
        };
        if first.kind() != "expression_statement" {
            return Vec::new();
        }
        match cx.children(first).first() {
            Some(s) if s.kind() == "string" => super::unquote(&cx.text(*s))
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn record(cx: &Cx, node: Node<'_>) -> Record {
        let name = cx.field_text(node, "name").unwrap_or_default();
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for item in cx.children(body) {
                match item.kind() {
                    "function_definition" => {
                        methods.push(function(cx, item, Some(name.clone())));
                    }
                    // A dataclass-style annotated field: `name: str`.
                    "expression_statement" => {
                        if let Some(inner) = cx.children(item).first() {
                            if inner.kind() == "assignment" || inner.kind() == "type" {
                                if let Some(f) = annotated_field(cx, *inner) {
                                    fields.push(f);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Record {
            doc: docstring(cx, cx.field(node, "body")),
            name,
            fields,
            exported: true,
            methods,
        }
    }

    fn annotated_field(cx: &Cx, node: Node<'_>) -> Option<Field> {
        let name = cx.field_text(node, "left")?;
        Some(Field {
            doc: Vec::new(),
            name: name.clone(),
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            exported: !name.starts_with('_'),
        })
    }

    fn constant(cx: &Cx, node: Node<'_>) -> Option<Constant> {
        let name = cx.field_text(node, "left")?;
        // Only a name that looks like a constant; a module-level `x = 1` is a variable.
        if !name
            .chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
        {
            return None;
        }
        Some(Constant {
            doc: Vec::new(),
            name,
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            value: cx
                .field(node, "right")
                .map(|v| expr(cx, v))
                .unwrap_or(Expr::Null),
            exported: true,
        })
    }

    fn ty(cx: &Cx, node: Node<'_>) -> Type {
        ty_text(&cx.text(node))
    }

    /// Resolve a type from its text, recursing through generic arguments.
    fn ty_text(text: &str) -> Type {
        let trimmed = text.trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }
        for (prefix, build) in [
            ("list[", 0usize),
            ("List[", 0),
            ("Optional[", 1),
            ("dict[", 2),
            ("Dict[", 2),
        ] {
            if let Some(inner) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(']'))
            {
                return match build {
                    0 => Type::List(Box::new(named_or_scalar(inner))),
                    1 => Type::Optional(Box::new(named_or_scalar(inner))),
                    _ => match inner.split_once(',') {
                        Some((k, v)) => {
                            Type::Map(Box::new(named_or_scalar(k)), Box::new(named_or_scalar(v)))
                        }
                        None => named_with_args(trimmed, &named_or_scalar),
                    },
                };
            }
        }
        if let Some(inner) = trimmed.strip_suffix(" | None") {
            return Type::Optional(Box::new(named_or_scalar(inner)));
        }
        named_with_args(trimmed, &named_or_scalar)
    }

    fn named_or_scalar(text: &str) -> Type {
        ty_text(text)
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        let children = cx.children(node);
        let mut out = Vec::new();
        for (i, child) in children.iter().enumerate() {
            // The docstring is the function's doc, not its first statement.
            if i == 0 && child.kind() == "expression_statement" {
                if let Some(inner) = cx.children(*child).first() {
                    if inner.kind() == "string" {
                        continue;
                    }
                }
            }
            out.push(keep_whole(cx, *child, stmt(cx, *child)));
        }
        out
    }

    /// `except ValueError as e:` — the type and the binding, either of which may be
    /// absent, and the body.
    fn except_clause(cx: &Cx, node: Node<'_>) -> Catch {
        let mut selector = None;
        let mut binding = None;
        let mut body = Vec::new();
        let mut seen_as = false;
        for child in cx.children(node) {
            match child.kind() {
                "block" => body = block(cx, child),
                "as_pattern" => {
                    // `except E as name` — the type first, the name after `as`.
                    let parts = cx.children(child);
                    if let Some(first) = parts.first() {
                        selector = Some(ty(cx, *first));
                    }
                    if let Some(last) = parts.last().filter(|l| l.kind() == "as_pattern_target") {
                        binding = Some(cx.text(*last));
                    }
                    seen_as = true;
                }
                _ if !seen_as && selector.is_none() => selector = Some(ty(cx, child)),
                _ => {}
            }
        }
        Catch {
            binding,
            ty: selector,
            body,
        }
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            // A comment is not an untranslatable construct: every one of these
            // languages has one and only the marker differs. Reading it as a failure
            // put ordinary prose in the output under a "not translated" marker and
            // counted it among the real gaps.
            "comment" | "line_comment" | "block_comment" => {
                Stmt::Comment(super::uncomment(&cx.text(node)))
            }
            "return_statement" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "raise_statement" => match cx.children(node).first() {
                Some(value) => Stmt::Throw(expr(cx, *value)),
                // A bare `raise` re-raises the exception being handled. There is no
                // expression to carry and no counterpart anywhere else.
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "try_statement" => {
                let mut catches = Vec::new();
                let mut finally = Vec::new();
                for clause in cx.children(node) {
                    match clause.kind() {
                        "except_clause" => catches.push(except_clause(cx, clause)),
                        "finally_clause" => {
                            finally = cx
                                .children(clause)
                                .into_iter()
                                .find(|c| c.kind() == "block")
                                .map(|b| block(cx, b))
                                .unwrap_or_default();
                        }
                        _ => {}
                    }
                }
                Stmt::Try {
                    body: cx
                        .children(node)
                        .into_iter()
                        .find(|c| c.kind() == "block")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    catches,
                    finally,
                    source: cx.text(node),
                    line: cx.line(node),
                }
            }
            "pass_statement" => Stmt::Expr(Expr::Null),
            "break_statement" => Stmt::Break,
            "continue_statement" => Stmt::Continue,
            "expression_statement" => match cx.children(node).first() {
                Some(inner) if inner.kind() == "assignment" => {
                    let target = cx.field(*inner, "left");
                    let value = cx.field(*inner, "right").map(|v| expr(cx, v));
                    // An annotated assignment is a binding with a type.
                    if cx.field(*inner, "type").is_some() || is_new_name(cx, *inner) {
                        Stmt::Let {
                            name: target.map(|t| cx.text(t)).unwrap_or_default(),
                            ty: cx.field(*inner, "type").map(|t| ty(cx, t)),
                            value,
                            mutable: true,
                        }
                    } else {
                        Stmt::Assign {
                            target: target.map(|t| expr(cx, t)).unwrap_or(Expr::Null),
                            value: value.unwrap_or(Expr::Null),
                        }
                    }
                }
                Some(inner) => Stmt::Expr(expr(cx, *inner)),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "if_statement" => {
                let mut otherwise = Vec::new();
                for clause in cx.children(node) {
                    match clause.kind() {
                        "elif_clause" => {
                            otherwise.push(Stmt::If {
                                condition: cx
                                    .field(clause, "condition")
                                    .map(|c| expr(cx, c))
                                    .unwrap_or(Expr::Null),
                                then: cx
                                    .field(clause, "consequence")
                                    .map(|b| block(cx, b))
                                    .unwrap_or_default(),
                                otherwise: Vec::new(),
                            });
                        }
                        "else_clause" => {
                            if let Some(body) = cx.field(clause, "body") {
                                otherwise.extend(block(cx, body));
                            }
                        }
                        _ => {}
                    }
                }
                Stmt::If {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    then: cx
                        .field(node, "consequence")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    otherwise,
                }
            }
            "while_statement" => Stmt::While {
                condition: cx
                    .field(node, "condition")
                    .map(|c| expr(cx, c))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            "for_statement" => Stmt::ForEach {
                binding: cx.field_text(node, "left").unwrap_or_default(),
                iterable: cx
                    .field(node, "right")
                    .map(|v| expr(cx, v))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// Python does not distinguish declaration from assignment. Treated as a binding
    /// when it is a bare name, which is what a writer needs to emit `let`.
    fn is_new_name(cx: &Cx, assignment: Node<'_>) -> bool {
        cx.field(assignment, "left")
            .map(|l| l.kind() == "identifier")
            .unwrap_or(false)
    }

    /// Is this `isinstance(value, Type)` with exactly two arguments?
    fn is_isinstance(cx: &Cx, node: Node<'_>) -> bool {
        cx.field_text(node, "function").as_deref() == Some("isinstance")
            && cx
                .field(node, "arguments")
                .map(|a| cx.children(a).len())
                .unwrap_or(0)
                == 2
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            "await" => match node.named_child(0) {
                Some(inner) => Expr::Await(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "integer" => Expr::Int(cx.text(node)),
            "float" => Expr::Float(cx.text(node)),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "none" => Expr::Null,
            "string" => {
                // An f-string interpolates. Dropping the braces would turn
                // `f"{c} below the floor"` into the literal text `{c} below the
                // floor` — not a gap but a wrong answer, so it is carried instead.
                if cx
                    .children(node)
                    .iter()
                    .any(|c| c.kind() == "interpolation")
                {
                    let mut parts = Vec::new();
                    for child in cx.children(node) {
                        match child.kind() {
                            "string_content" => parts.push(TemplatePart::Text(cx.text(child))),
                            "interpolation" => {
                                // `{x!r}` and `{x:>3}` convert or format, which is
                                // more than an interpolation and is not translated.
                                let inner = cx.children(child);
                                if inner.len() != 1 {
                                    return Expr::Unsupported(cx.unsupported(node));
                                }
                                parts.push(TemplatePart::Expr(expr(cx, inner[0])));
                            }
                            _ => {}
                        }
                    }
                    return Expr::Template(parts);
                }
                Expr::Str(super::unquote(&cx.text(node)))
            }
            "identifier" => Expr::Name(cx.text(node)),
            "attribute" => Expr::Field {
                of: Box::new(
                    cx.field(node, "object")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                name: cx.field_text(node, "attribute").unwrap_or_default(),
            },
            "subscript" => Expr::Index {
                of: Box::new(
                    cx.field(node, "value")
                        .map(|v| expr(cx, v))
                        .unwrap_or(Expr::Null),
                ),
                index: Box::new(
                    cx.field(node, "subscript")
                        .map(|s| expr(cx, s))
                        .unwrap_or(Expr::Null),
                ),
            },
            // `isinstance(x, T)` is the same question TypeScript asks with
            // `instanceof`, so it reads as the same node and round-trips.
            "call" if is_isinstance(cx, node) => {
                let args = cx
                    .field(node, "arguments")
                    .map(|a| cx.children(a))
                    .unwrap_or_default();
                Expr::InstanceOf {
                    value: Box::new(expr(cx, args[0])),
                    ty: Box::new(expr(cx, args[1])),
                }
            }
            "call" => call_or_carry(
                cx,
                node,
                cx.field(node, "function")
                    .map(|f| expr(cx, f))
                    .unwrap_or(Expr::Null),
                cx.field(node, "arguments")
                    .map(|a| cx.children(a).iter().map(|n| expr(cx, *n)).collect())
                    .unwrap_or_default(),
            ),
            "list" => Expr::ListLit(cx.children(node).iter().map(|n| expr(cx, *n)).collect()),
            "dictionary" => {
                let mut entries = Vec::new();
                for pair in cx.children(node) {
                    if pair.kind() != "pair" {
                        return Expr::Unsupported(cx.unsupported(node));
                    }
                    let (Some(k), Some(v)) = (cx.field(pair, "key"), cx.field(pair, "value"))
                    else {
                        return Expr::Unsupported(cx.unsupported(node));
                    };
                    entries.push((expr(cx, k), expr(cx, v)));
                }
                Expr::MapLit(entries)
            }
            "list_comprehension" => {
                let clauses = cx.children(node);
                let Some(element) = clauses.first() else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                let mut binding = None;
                let mut iterable = None;
                let mut condition = None;
                let mut extra = false;
                for clause in &clauses[1..] {
                    match clause.kind() {
                        "for_in_clause" if binding.is_none() => {
                            binding = cx.field_text(*clause, "left");
                            iterable = cx.field(*clause, "right").map(|r| expr(cx, r));
                        }
                        "if_clause" if condition.is_none() => {
                            condition = cx.children(*clause).first().map(|c| expr(cx, *c));
                        }
                        // A second `for` or `if` is a nested comprehension, which does
                        // not map onto one filter and one map.
                        _ => extra = true,
                    }
                }
                match (binding, iterable, extra) {
                    (Some(binding), Some(iterable), false) => Expr::Comprehension {
                        element: Box::new(expr(cx, *element)),
                        binding,
                        iterable: Box::new(iterable),
                        condition: condition.map(Box::new),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "comparison_operator" | "boolean_operator" | "binary_operator" => {
                // `is not` and `not in` are two tokens. Reading only the first turned
                // `x is not None` into `x == None`, which is the opposite of what it
                // says — a wrong answer rather than a missing one.
                let mut cursor = node.walk();
                let operator: String = node
                    .children(&mut cursor)
                    .filter(|c| !c.is_named())
                    .map(|c| cx.text(c))
                    .collect::<Vec<_>>()
                    .join(" ");
                let op = super::binary_op(&operator);
                match op {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(
                            cx.field(node, "left")
                                .or_else(|| node.child(0))
                                .map(|l| expr(cx, l))
                                .unwrap_or(Expr::Null),
                        ),
                        right: Box::new(
                            cx.field(node, "right")
                                .or_else(|| node.child(2))
                                .map(|r| expr(cx, r))
                                .unwrap_or(Expr::Null),
                        ),
                    },
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "not_operator" => Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(
                    cx.field(node, "argument")
                        .map(|a| expr(cx, a))
                        .unwrap_or(Expr::Null),
                ),
            },
            "unary_operator" => Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(
                    cx.field(node, "argument")
                        .map(|a| expr(cx, a))
                        .unwrap_or(Expr::Null),
                ),
            },
            "parenthesized_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

// --------------------------------------------------------------------------- Go

mod go {
    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        // Methods are declared apart from their type, as in Rust, and are attached to
        // the record once both have been seen.
        let mut pending: Vec<(String, Function)> = Vec::new();
        for child in cx.children(root) {
            match child.kind() {
                "comment" | "package_clause" => {}
                "import_declaration" => module.items.push(Item::Import {
                    text: cx.text(child),
                    line: cx.line(child),
                }),
                "function_declaration" => {
                    module.items.push(Item::Function(function(cx, child, None)))
                }
                "method_declaration" => {
                    let owner = cx
                        .field(child, "receiver")
                        .and_then(|r| cx.children(r).first().copied())
                        .and_then(|p| cx.field(p, "type"))
                        .map(|t| cx.text(t).trim_start_matches('*').to_string())
                        .unwrap_or_default();
                    pending.push((owner.clone(), function(cx, child, Some(owner))));
                }
                "type_declaration" => {
                    for spec in cx.children(child) {
                        if spec.kind() == "type_spec" {
                            match record(cx, spec) {
                                Some(r) => module.items.push(Item::Record(r)),
                                None => module.items.push(Item::Unsupported(cx.unsupported(spec))),
                            }
                        }
                    }
                }
                "const_declaration" | "var_declaration" => {
                    for spec in cx.children(child) {
                        if let Some(c) = constant(cx, spec) {
                            module.items.push(Item::Constant(c));
                        }
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        for (owner, method) in pending {
            if let Some(Item::Record(record)) = module
                .items
                .iter_mut()
                .find(|i| matches!(i, Item::Record(r) if r.name == owner))
            {
                record.methods.push(method);
            } else {
                module.items.push(Item::Function(method));
            }
        }
        module
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let mut params = Vec::new();
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                if p.kind() != "parameter_declaration" {
                    continue;
                }
                let ty_node = cx.field(p, "type");
                // `a, b int` declares two parameters of one type.
                let names: Vec<Node> = cx
                    .children(p)
                    .into_iter()
                    .filter(|n| n.kind() == "identifier")
                    .collect();
                if names.is_empty() {
                    params.push(Param {
                        name: String::new(),
                        ty: ty_node.map(|t| ty(cx, t)),
                        default: None,
                        kind: ParamKind::Normal,
                    });
                }
                for name in names {
                    params.push(Param {
                        name: cx.text(name),
                        ty: ty_node.map(|t| ty(cx, t)),
                        default: None,
                        kind: ParamKind::Normal,
                    });
                }
            }
        }
        let name = cx.field_text(node, "name").unwrap_or_default();
        Function {
            doc: doc_above(cx, node, &["//"]),
            exported: name.chars().next().is_some_and(|c| c.is_uppercase()),
            name,
            receiver,
            params,
            returns: cx.field(node, "result").map(|t| ty(cx, t)),
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            is_async: false,
        }
    }

    fn record(cx: &Cx, spec: Node<'_>) -> Option<Record> {
        let name = cx.field_text(spec, "name")?;
        let ty_node = cx.field(spec, "type")?;
        if ty_node.kind() != "struct_type" {
            return None;
        }
        let mut fields = Vec::new();
        for list in cx.children(ty_node) {
            for f in cx.children(list) {
                if f.kind() != "field_declaration" {
                    continue;
                }
                let field_ty = cx.field(f, "type").map(|t| ty(cx, t));
                for n in cx.children(f) {
                    if n.kind() != "field_identifier" {
                        continue;
                    }
                    let field_name = cx.text(n);
                    fields.push(Field {
                        doc: doc_above(cx, f, &["//"]),
                        exported: field_name.chars().next().is_some_and(|c| c.is_uppercase()),
                        name: field_name,
                        ty: field_ty.clone(),
                    });
                }
            }
        }
        Some(Record {
            doc: doc_above(cx, spec, &["//"]),
            exported: name.chars().next().is_some_and(|c| c.is_uppercase()),
            name,
            fields,
            methods: Vec::new(),
        })
    }

    fn constant(cx: &Cx, spec: Node<'_>) -> Option<Constant> {
        if !matches!(spec.kind(), "const_spec" | "var_spec") {
            return None;
        }
        let name = cx.field_text(spec, "name")?;
        Some(Constant {
            doc: doc_above(cx, spec, &["//"]),
            exported: name.chars().next().is_some_and(|c| c.is_uppercase()),
            name,
            ty: cx.field(spec, "type").map(|t| ty(cx, t)),
            value: cx
                .field(spec, "value")
                .and_then(|v| cx.children(v).first().copied())
                .map(|v| expr(cx, v))
                .unwrap_or(Expr::Null),
        })
    }

    fn ty(cx: &Cx, node: Node<'_>) -> Type {
        let text = cx.text(node);
        let trimmed = text.trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }
        if let Some(inner) = trimmed.strip_prefix("[]") {
            return Type::List(Box::new(named_or_scalar(inner)));
        }
        if let Some(inner) = trimmed.strip_prefix("map[") {
            if let Some((k, v)) = inner.split_once(']') {
                return Type::Map(Box::new(named_or_scalar(k)), Box::new(named_or_scalar(v)));
            }
        }
        if let Some(inner) = trimmed.strip_prefix('*') {
            return Type::Optional(Box::new(named_or_scalar(inner)));
        }
        named_with_args(trimmed, &named_or_scalar)
    }

    fn named_or_scalar(text: &str) -> Type {
        super::scalar(text).unwrap_or_else(|| super::named_with_args(text.trim(), &named_or_scalar))
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        cx.children(node)
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            // A comment is not an untranslatable construct: every one of these
            // languages has one and only the marker differs. Reading it as a failure
            // put ordinary prose in the output under a "not translated" marker and
            // counted it among the real gaps.
            "comment" | "line_comment" | "block_comment" => {
                Stmt::Comment(super::uncomment(&cx.text(node)))
            }
            "return_statement" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "break_statement" => Stmt::Break,
            "continue_statement" => Stmt::Continue,
            "short_var_declaration" => Stmt::Let {
                name: cx.field_text(node, "left").unwrap_or_default(),
                ty: None,
                value: cx.field(node, "right").map(|v| expr(cx, v)),
                mutable: true,
            },
            "assignment_statement" => Stmt::Assign {
                target: cx
                    .field(node, "left")
                    .map(|l| expr(cx, l))
                    .unwrap_or(Expr::Null),
                value: cx
                    .field(node, "right")
                    .map(|r| expr(cx, r))
                    .unwrap_or(Expr::Null),
            },
            "expression_statement" => cx
                .children(node)
                .first()
                .map(|inner| Stmt::Expr(expr(cx, *inner)))
                .unwrap_or_else(|| Stmt::Unsupported(cx.unsupported(node))),
            "if_statement" => {
                let otherwise = cx
                    .field(node, "alternative")
                    .map(|alt| match alt.kind() {
                        "if_statement" => vec![stmt(cx, alt)],
                        _ => block(cx, alt),
                    })
                    .unwrap_or_default();
                Stmt::If {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    then: cx
                        .field(node, "consequence")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    otherwise,
                }
            }
            "for_statement" => {
                // `for range` is the only Go loop the IR has; a three-clause `for` is
                // not a for-each and is carried whole.
                if let Some(clause) = cx
                    .children(node)
                    .into_iter()
                    .find(|c| c.kind() == "range_clause")
                {
                    let binding = cx
                        .field(clause, "left")
                        .map(|l| cx.text(l))
                        .unwrap_or_default();
                    // `for i, v := range xs` binds two; the IR binds the value.
                    let binding = binding
                        .split(',')
                        .next_back()
                        .unwrap_or(&binding)
                        .trim()
                        .to_string();
                    return Stmt::ForEach {
                        binding,
                        iterable: cx
                            .field(clause, "right")
                            .map(|r| expr(cx, r))
                            .unwrap_or(Expr::Null),
                        body: cx
                            .field(node, "body")
                            .map(|b| block(cx, b))
                            .unwrap_or_default(),
                    };
                }
                Stmt::Unsupported(cx.unsupported(node))
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            "int_literal" => Expr::Int(cx.text(node)),
            "float_literal" => Expr::Float(cx.text(node)),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "nil" => Expr::Null,
            "interpreted_string_literal" | "raw_string_literal" => {
                Expr::Str(super::unquote(&cx.text(node)))
            }
            "identifier" | "field_identifier" | "type_identifier" => Expr::Name(cx.text(node)),
            "selector_expression" => Expr::Field {
                of: Box::new(
                    cx.field(node, "operand")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                name: cx.field_text(node, "field").unwrap_or_default(),
            },
            "index_expression" => Expr::Index {
                of: Box::new(
                    cx.field(node, "operand")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                index: Box::new(
                    cx.field(node, "index")
                        .map(|i| expr(cx, i))
                        .unwrap_or(Expr::Null),
                ),
            },
            "call_expression" => call_or_carry(
                cx,
                node,
                cx.field(node, "function")
                    .map(|f| expr(cx, f))
                    .unwrap_or(Expr::Null),
                cx.field(node, "arguments")
                    .map(|a| cx.children(a).iter().map(|n| expr(cx, *n)).collect())
                    .unwrap_or_default(),
            ),
            "binary_expression" => {
                match super::binary_op(&cx.field_text(node, "operator").unwrap_or_default()) {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(
                            cx.field(node, "left")
                                .map(|l| expr(cx, l))
                                .unwrap_or(Expr::Null),
                        ),
                        right: Box::new(
                            cx.field(node, "right")
                                .map(|r| expr(cx, r))
                                .unwrap_or(Expr::Null),
                        ),
                    },
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "unary_expression" => {
                let text = cx.text(node);
                let op = if text.starts_with('!') {
                    Some(UnaryOp::Not)
                } else if text.starts_with('-') {
                    Some(UnaryOp::Neg)
                } else {
                    None
                };
                match (op, cx.field(node, "operand")) {
                    (Some(op), Some(inner)) => Expr::Unary {
                        op,
                        operand: Box::new(expr(cx, inner)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "parenthesized_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

// ------------------------------------------------------------------- TypeScript

mod typescript {
    /// Does this access use `?.`?
    ///
    /// The grammar makes `optional_chain` a child rather than a field, so the only way
    /// to ask is to look. Worth asking: `a?.b` and `a.b` differ exactly where it
    /// matters, and the difference is invisible in the text this reader keeps.
    fn has_optional_chain(node: Node<'_>) -> bool {
        let mut cursor = node.walk();
        let found = node
            .children(&mut cursor)
            .any(|child| child.kind() == "optional_chain");
        found
    }

    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        for child in cx.children(root) {
            let (node, exported) = match child.kind() {
                "export_statement" => match cx.children(child).first() {
                    Some(inner) => (*inner, true),
                    None => (child, true),
                },
                _ => (child, false),
            };
            match node.kind() {
                "comment" => {}
                "import_statement" => module.items.push(Item::Import {
                    text: cx.text(child),
                    line: cx.line(child),
                }),
                "function_declaration" => {
                    let mut f = function(cx, node, None);
                    f.exported = exported;
                    f.doc = doc_above(cx, child, &["///", "//", "/**", "*/", "*"]);
                    module.items.push(Item::Function(f));
                }
                "class_declaration" | "interface_declaration" => {
                    let mut r = record(cx, node);
                    r.exported = exported;
                    r.doc = doc_above(cx, child, &["///", "//", "/**", "*/", "*"]);
                    module.items.push(Item::Record(r));
                }
                "lexical_declaration" => {
                    for d in cx.children(node) {
                        if d.kind() != "variable_declarator" {
                            continue;
                        }
                        module.items.push(Item::Constant(Constant {
                            doc: doc_above(cx, child, &["///", "//", "/**", "*/", "*"]),
                            name: cx.field_text(d, "name").unwrap_or_default(),
                            ty: cx.field(d, "type").map(|t| ty(cx, t)),
                            value: cx
                                .field(d, "value")
                                .map(|v| expr(cx, v))
                                .unwrap_or(Expr::Null),
                            exported,
                        }));
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        module
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let mut params = Vec::new();
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                match p.kind() {
                    "required_parameter" | "optional_parameter" => {
                        let name = cx.field_text(p, "pattern").unwrap_or_default();
                        let mut t = cx.field(p, "type").map(|t| ty(cx, t));
                        if p.kind() == "optional_parameter" {
                            t = Some(Type::Optional(Box::new(
                                t.unwrap_or(named_with_args("unknown", &named_or_scalar)),
                            )));
                        }
                        params.push(Param {
                            name,
                            ty: t,
                            default: cx.field(p, "value").map(|v| expr(cx, v)),
                            kind: ParamKind::Normal,
                        });
                    }
                    _ => params.push(Param {
                        name: cx.text(p),
                        ty: None,
                        default: None,
                        kind: ParamKind::Normal,
                    }),
                }
            }
        }
        let is_async = cx.text(node).starts_with("async ");
        let returns = cx.field(node, "return_type").map(|t| ty(cx, t)).map(|t| {
            // `async f(): Promise<T>` and `async def f() -> T` say the same thing.
            // Carrying the wrapper through would make the Python signature claim a
            // type that does not exist there.
            match (&t, is_async) {
                (Type::Named { name, args }, true) if name == "Promise" && args.len() == 1 => {
                    args[0].clone()
                }
                _ => t,
            }
        });
        Function {
            doc: Vec::new(),
            name: cx.field_text(node, "name").unwrap_or_default(),
            receiver,
            params,
            returns,
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            exported: false,
            is_async,
        }
    }

    fn record(cx: &Cx, node: Node<'_>) -> Record {
        let name = cx.field_text(node, "name").unwrap_or_default();
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for member in cx.children(body) {
                match member.kind() {
                    "public_field_definition" | "property_signature" => fields.push(Field {
                        doc: Vec::new(),
                        name: cx.field_text(member, "name").unwrap_or_default(),
                        ty: cx.field(member, "type").map(|t| ty(cx, t)),
                        exported: true,
                    }),
                    "method_definition" | "method_signature" => {
                        methods.push(function(cx, member, Some(name.clone())))
                    }
                    _ => {}
                }
            }
        }
        Record {
            doc: Vec::new(),
            name,
            fields,
            exported: false,
            methods,
        }
    }

    fn ty(cx: &Cx, node: Node<'_>) -> Type {
        // A `type_annotation` wraps the type after the colon.
        let inner = cx.children(node).first().copied().unwrap_or(node);
        ty_text(&cx.text(inner))
    }

    /// Resolve a type from its text, recursing through generic arguments.
    ///
    /// The entry point and the recursion are the same function: when they were not,
    /// `Promise<Record<string, string>>` resolved its outer layer and left the inner
    /// one as an opaque name, so a round trip produced `Record[str, str]` in Python.
    fn ty_text(text: &str) -> Type {
        let trimmed = text.trim().trim_start_matches(':').trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }
        if let Some(element) = trimmed.strip_suffix("[]") {
            return Type::List(Box::new(named_or_scalar(element)));
        }
        for prefix in ["Array<", "ReadonlyArray<"] {
            if let Some(element) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix('>'))
            {
                return Type::List(Box::new(named_or_scalar(element)));
            }
        }
        if let Some(inner) = trimmed
            .strip_prefix("Record<")
            .and_then(|s| s.strip_suffix('>'))
        {
            if let Some((k, v)) = inner.split_once(',') {
                return Type::Map(Box::new(named_or_scalar(k)), Box::new(named_or_scalar(v)));
            }
        }
        for suffix in [" | null", " | undefined"] {
            if let Some(base) = trimmed.strip_suffix(suffix) {
                return Type::Optional(Box::new(named_or_scalar(base)));
            }
        }
        named_with_args(trimmed, &named_or_scalar)
    }

    fn named_or_scalar(text: &str) -> Type {
        ty_text(text)
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        cx.children(node)
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            // A comment is not an untranslatable construct: every one of these
            // languages has one and only the marker differs. Reading it as a failure
            // put ordinary prose in the output under a "not translated" marker and
            // counted it among the real gaps.
            "comment" | "line_comment" | "block_comment" => {
                Stmt::Comment(super::uncomment(&cx.text(node)))
            }
            "return_statement" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "throw_statement" => match cx.children(node).first() {
                Some(value) => Stmt::Throw(expr(cx, *value)),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "try_statement" => {
                let mut catches = Vec::new();
                let mut finally = Vec::new();
                if let Some(clause) = cx.field(node, "handler") {
                    catches.push(Catch {
                        binding: cx.field(clause, "parameter").map(|p| cx.text(p)),
                        // TypeScript catches everything; there is no type to select on.
                        ty: None,
                        body: cx
                            .field(clause, "body")
                            .map(|b| block(cx, b))
                            .unwrap_or_default(),
                    });
                }
                if let Some(clause) = cx.field(node, "finalizer") {
                    finally = cx
                        .children(clause)
                        .into_iter()
                        .find(|c| c.kind() == "statement_block")
                        .map(|b| block(cx, b))
                        .unwrap_or_else(|| block(cx, clause));
                }
                Stmt::Try {
                    body: cx
                        .field(node, "body")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    catches,
                    finally,
                    source: cx.text(node),
                    line: cx.line(node),
                }
            }
            "break_statement" => Stmt::Break,
            "continue_statement" => Stmt::Continue,
            "lexical_declaration" | "variable_declaration" => {
                match cx.children(node).first().copied() {
                    Some(d) if d.kind() == "variable_declarator" => Stmt::Let {
                        name: cx.field_text(d, "name").unwrap_or_default(),
                        ty: cx.field(d, "type").map(|t| ty(cx, t)),
                        value: cx.field(d, "value").map(|v| expr(cx, v)),
                        mutable: cx.text(node).starts_with("let "),
                    },
                    _ => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
            "expression_statement" => match cx.children(node).first().copied() {
                Some(inner) if inner.kind() == "assignment_expression" => Stmt::Assign {
                    target: cx
                        .field(inner, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null),
                    value: cx
                        .field(inner, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null),
                },
                Some(inner) => Stmt::Expr(expr(cx, inner)),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "if_statement" => {
                let otherwise = cx
                    .field(node, "alternative")
                    .map(|alt| {
                        let inner = cx.children(alt);
                        match inner.first() {
                            Some(first) if first.kind() == "if_statement" => vec![stmt(cx, *first)],
                            Some(first) if first.kind() == "statement_block" => block(cx, *first),
                            _ => Vec::new(),
                        }
                    })
                    .unwrap_or_default();
                Stmt::If {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    then: cx
                        .field(node, "consequence")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    otherwise,
                }
            }
            "while_statement" => Stmt::While {
                condition: cx
                    .field(node, "condition")
                    .map(|c| expr(cx, c))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            "for_in_statement" => Stmt::ForEach {
                binding: cx.field_text(node, "left").unwrap_or_default(),
                iterable: cx
                    .field(node, "right")
                    .map(|r| expr(cx, r))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default(),
            },
            "statement_block" => {
                let inner = block(cx, node);
                if inner.len() == 1 {
                    inner.into_iter().next().unwrap()
                } else {
                    Stmt::Unsupported(cx.unsupported(node))
                }
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// One arrow function of one parameter: `(x) => body`, or `x => body`.
    ///
    /// Anything else — a destructured parameter, a block body, a named callback — is
    /// not the shape a comprehension has, and pretending otherwise would invent one.
    fn one_arg_arrow<'t>(cx: &Cx, node: Node<'t>) -> Option<(String, Node<'t>)> {
        if node.kind() != "arrow_function" {
            return None;
        }
        let body = cx.field(node, "body")?;
        if body.kind() == "statement_block" {
            return None;
        }
        let parameter = match cx.field(node, "parameter") {
            Some(p) => cx.text(p),
            None => {
                let list = cx.field(node, "parameters")?;
                let params = cx.children(list);
                if params.len() != 1 {
                    return None;
                }
                let only = params[0];
                match only.kind() {
                    "required_parameter" => cx.field(only, "pattern").map(|p| cx.text(p))?,
                    "identifier" => cx.text(only),
                    _ => return None,
                }
            }
        };
        Some((parameter, body))
    }

    /// `xs.map(f)` and `xs.filter(p).map(f)`, which is a comprehension written the
    /// way TypeScript writes one.
    fn chain(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        let callee = cx.field(node, "function")?;
        if callee.kind() != "member_expression" {
            return None;
        }
        if cx.field_text(callee, "property")? != "map" {
            return None;
        }
        let args = cx.children(cx.field(node, "arguments")?);
        if args.len() != 1 {
            return None;
        }
        let (binding, element) = one_arg_arrow(cx, args[0])?;

        // The receiver is either the collection, or a `.filter(...)` on it.
        let receiver = cx.field(callee, "object")?;
        let (iterable, condition) = if receiver.kind() == "call_expression" {
            match cx
                .field(receiver, "function")
                .filter(|f| f.kind() == "member_expression")
                .filter(|f| cx.field_text(*f, "property").as_deref() == Some("filter"))
            {
                Some(filter_callee) => {
                    let filter_args = cx.children(cx.field(receiver, "arguments")?);
                    if filter_args.len() != 1 {
                        return None;
                    }
                    let (filter_binding, predicate) = one_arg_arrow(cx, filter_args[0])?;
                    // Two different names is two different scopes, not one loop.
                    if filter_binding != binding {
                        return None;
                    }
                    (cx.field(filter_callee, "object")?, Some(predicate))
                }
                None => (receiver, None),
            }
        } else {
            (receiver, None)
        };

        Some(Expr::Comprehension {
            element: Box::new(expr(cx, element)),
            binding,
            iterable: Box::new(expr(cx, iterable)),
            condition: condition.map(|c| Box::new(expr(cx, c))),
        })
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            "await_expression" => match node.named_child(0) {
                Some(inner) => Expr::Await(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "number" => {
                let text = cx.text(node);
                if text.contains('.') {
                    Expr::Float(text)
                } else {
                    Expr::Int(text)
                }
            }
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "null" | "undefined" => Expr::Null,
            "string" => Expr::Str(super::unquote(&cx.text(node))),
            "identifier" | "property_identifier" | "this" => Expr::Name(cx.text(node)),
            // `a?.b` is not `a.b`. Neither Python, Rust nor Go has optional
            // chaining, and writing the plain access drops the null check silently —
            // the translation would compile, run, and throw where the original
            // returned undefined. Carried instead.
            "member_expression" if has_optional_chain(node) => {
                Expr::Unsupported(cx.unsupported(node))
            }
            "member_expression" => Expr::Field {
                of: Box::new(
                    cx.field(node, "object")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                name: cx.field_text(node, "property").unwrap_or_default(),
            },
            "subscript_expression" if has_optional_chain(node) => {
                Expr::Unsupported(cx.unsupported(node))
            }
            "subscript_expression" => Expr::Index {
                of: Box::new(
                    cx.field(node, "object")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                index: Box::new(
                    cx.field(node, "index")
                        .map(|i| expr(cx, i))
                        .unwrap_or(Expr::Null),
                ),
            },
            "call_expression" => {
                if let Some(comprehension) = chain(cx, node) {
                    return comprehension;
                }
                call_or_carry(
                    cx,
                    node,
                    cx.field(node, "function")
                        .map(|f| expr(cx, f))
                        .unwrap_or(Expr::Null),
                    cx.field(node, "arguments")
                        .map(|a| cx.children(a).iter().map(|n| expr(cx, *n)).collect())
                        .unwrap_or_default(),
                )
            }
            "array" => Expr::ListLit(cx.children(node).iter().map(|n| expr(cx, *n)).collect()),
            "template_string" => {
                let mut parts = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "string_fragment" => parts.push(TemplatePart::Text(cx.text(child))),
                        "template_substitution" => {
                            let inner = cx.children(child);
                            if inner.len() != 1 {
                                return Expr::Unsupported(cx.unsupported(node));
                            }
                            parts.push(TemplatePart::Expr(expr(cx, inner[0])));
                        }
                        _ => {}
                    }
                }
                Expr::Template(parts)
            }
            "object" => {
                let mut entries = Vec::new();
                for pair in cx.children(node) {
                    if pair.kind() != "pair" {
                        return Expr::Unsupported(cx.unsupported(node));
                    }
                    let (Some(k), Some(v)) = (cx.field(pair, "key"), cx.field(pair, "value"))
                    else {
                        return Expr::Unsupported(cx.unsupported(node));
                    };
                    // A bare key is a name in the tree and a string in the IR.
                    let key = match k.kind() {
                        "property_identifier" => Expr::Str(cx.text(k)),
                        _ => expr(cx, k),
                    };
                    entries.push((key, expr(cx, v)));
                }
                Expr::MapLit(entries)
            }
            // `instanceof` is spelled as an operator here and as a builtin in
            // Python; it is the same question either way, so it is its own node.
            "binary_expression"
                if cx.field_text(node, "operator").as_deref() == Some("instanceof") =>
            {
                Expr::InstanceOf {
                    value: Box::new(
                        cx.field(node, "left")
                            .map(|l| expr(cx, l))
                            .unwrap_or(Expr::Null),
                    ),
                    ty: Box::new(
                        cx.field(node, "right")
                            .map(|r| expr(cx, r))
                            .unwrap_or(Expr::Null),
                    ),
                }
            }
            "binary_expression" => {
                match super::binary_op(&cx.field_text(node, "operator").unwrap_or_default()) {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(
                            cx.field(node, "left")
                                .map(|l| expr(cx, l))
                                .unwrap_or(Expr::Null),
                        ),
                        right: Box::new(
                            cx.field(node, "right")
                                .map(|r| expr(cx, r))
                                .unwrap_or(Expr::Null),
                        ),
                    },
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "unary_expression" => {
                let text = cx.text(node);
                let op = if text.starts_with('!') {
                    Some(UnaryOp::Not)
                } else if text.starts_with('-') {
                    Some(UnaryOp::Neg)
                } else {
                    None
                };
                match (op, cx.field(node, "argument")) {
                    (Some(op), Some(inner)) => Expr::Unary {
                        op,
                        operand: Box::new(expr(cx, inner)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "parenthesized_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            // `x as T`, `x satisfies T` and `x!` are assertions to the type checker
            // and have no runtime effect whatever. The value is the expression, so
            // the translation is exact rather than a gap — and leaving them
            // unhandled carried a whole statement over something that meant nothing.
            "new_expression" => Expr::New {
                callee: Box::new(
                    cx.field(node, "constructor")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                ),
                args: cx
                    .field(node, "arguments")
                    .map(|a| cx.children(a).into_iter().map(|n| expr(cx, n)).collect())
                    .unwrap_or_default(),
            },
            "as_expression" | "satisfies_expression" | "non_null_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

// ------------------------------------------------------------------------ shared

/// Split `Name<A, B>` or `Name[A, B]` into its base and its arguments.
///
/// Nesting is respected, so `Result<Vec<T>, E>` yields two arguments rather than
/// three. A name with no brackets is itself with no arguments.
fn split_generic(text: &str) -> (String, Vec<String>) {
    let trimmed = text.trim();
    let (open, close) = if trimmed.ends_with('>') {
        ('<', '>')
    } else if trimmed.ends_with(']') {
        ('[', ']')
    } else {
        return (trimmed.to_string(), Vec::new());
    };
    let Some(at) = trimmed.find(open) else {
        return (trimmed.to_string(), Vec::new());
    };
    let base = trimmed[..at].trim().to_string();
    let inside = &trimmed[at + 1..trimmed.len() - close.len_utf8()];

    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in inside.chars() {
        match c {
            '<' | '[' | '(' => {
                depth += 1;
                current.push(c);
            }
            '>' | ']' | ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    (base, args)
}

/// A named type, with its arguments read recursively through `resolve`.
fn named_with_args(text: &str, resolve: &dyn Fn(&str) -> Type) -> Type {
    let (base, args) = split_generic(text);
    Type::Named {
        name: base,
        args: args.iter().map(|a| resolve(a)).collect(),
    }
}

/// The scalar types that mean the same thing in all four languages.
///
/// Width is deliberately dropped: `i64` and `int` and `number` are all [`Type::Int`],
/// because carrying a width into a language that has none would be inventing a
/// guarantee. The writer says so when it matters.
fn scalar(text: &str) -> Option<Type> {
    let t = text.trim().trim_start_matches('&').trim();
    Some(match t {
        "bool" | "boolean" => Type::Bool,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" | "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16"
        | "uint32" | "uint64" => Type::Int,
        "f32" | "f64" | "float" | "float32" | "float64" => Type::Float,
        "String" | "str" | "string" => Type::String,
        "()" | "None" | "void" => Type::Unit,
        // TypeScript's `number` is a float, and saying so is more honest than
        // pretending an integer type it does not have.
        "number" => Type::Float,
        _ => return None,
    })
}

fn binary_op(text: &str) -> Option<BinaryOp> {
    Some(match text.trim() {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Rem,
        "==" | "===" | "is" => BinaryOp::Eq,
        "!=" | "!==" | "is not" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        "<=" => BinaryOp::Le,
        ">" => BinaryOp::Gt,
        ">=" => BinaryOp::Ge,
        "&&" | "and" => BinaryOp::And,
        "||" | "or" => BinaryOp::Or,
        _ => return None,
    })
}

/// The text of a string literal, without its quotes or prefix.
/// The text of a comment, without whichever marker the source language used.
///
/// The marker is the only thing that differs between these four, so stripping it here
/// and letting each writer add its own is the whole of comment translation.
fn uncomment(text: &str) -> String {
    let text = text.trim();
    let body = text
        .strip_prefix("///")
        .or_else(|| text.strip_prefix("//!"))
        .or_else(|| text.strip_prefix("//"))
        .or_else(|| text.strip_prefix("#"))
        .or_else(|| {
            text.strip_prefix("/*")
                .map(|rest| rest.strip_suffix("*/").unwrap_or(rest))
        })
        .unwrap_or(text);
    body.trim().to_string()
}

fn unquote(text: &str) -> String {
    let t = text
        .trim_start_matches(|c: char| c.is_alphabetic())
        .trim_start_matches('#');
    for quote in ["\"\"\"", "'''", "\"", "'", "`"] {
        if let Some(inner) = t.strip_prefix(quote).and_then(|s| s.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    t.to_string()
}
