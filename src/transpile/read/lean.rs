//! Reading Lean 4 into the shared representation.
//!
//! The writer's counterpart, and the smaller half of the job. The writer had to decide
//! what every construct of six languages means in Lean. This reads back the subset the
//! writer produces, plus the Lean a person writes in the same shape.
//!
//! Two things about the tree shape the whole file.
//!
//! A `do` block's elements are not always siblings. A plain `let` may parse as the term
//! it also is, taking everything after it as its `body`. A run of statements then
//! arrives as a chain. Both readings are correct Lean, and `elements` flattens either
//! into one sequence.
//!
//! Lean's own spellings go back to the shared ones. `IO.println` is `print`, `Int.tdiv`
//! is the division that truncates, `Array` is a list. The writer's table read backwards,
//! so that a file that made the round trip comes back as what it left as.

use super::*;
use crate::transpile::write::pascal;

pub(super) fn module(cx: &Cx, root: Node<'_>) -> Module {
    let mut module = Module::default();
    for node in cx.children_with_comments(root) {
        item(cx, node, &mut module);
    }
    settle_methods_read_as_fields(&mut module);
    module
}

/// `p.area` reads a field and calls a method with the same three words, and only the
/// module says which one it is.
///
/// Lean writes a method on a structure as a definition in the namespace the structure
/// opens. The reader holds both sets by the end of the file.
fn settle_methods_read_as_fields(module: &mut Module) {
    let mut methods: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut declared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    // Writing a function's name calls it where the function takes no arguments, and
    // names it where it takes some. One of no arguments can only be the call.
    let mut niladic: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for item in &module.items {
        match item {
            Item::Function(f) if f.receiver.is_some() => {
                methods.insert(f.name.clone());
            }
            Item::Function(f) if f.params.is_empty() => {
                niladic.insert(f.name.clone());
            }
            Item::Record(r) => {
                for field in &r.fields {
                    declared.insert(field.name.clone());
                }
            }
            _ => {}
        }
    }
    methods.retain(|m| !declared.contains(m));
    if methods.is_empty() && niladic.is_empty() {
        return;
    }
    super::each_expr_in_module(module, &mut |e| {
        let called = match e {
            Expr::Field { name, .. } => methods.contains(name),
            Expr::Name(name) => niladic.contains(name),
            _ => false,
        };
        if !called {
            return;
        }
        let read = e.clone();
        *e = Expr::Call {
            callee: Box::new(read),
            args: Vec::new(),
        };
    });
}

/// One top-level declaration.
fn item(cx: &Cx, node: Node<'_>, module: &mut Module) {
    match node.kind() {
        "structure" => module.items.push(record(cx, node)),
        "inductive" | "class_inductive" => module.items.push(sum(cx, node)),
        "definition" => module.items.push(definition(cx, node)),
        "import" => module.items.push(Item::Import {
            text: cx.text(node),
            line: cx.line(node),
            target: None,
        }),
        // The declarations inside see one another, and nothing else about a `mutual`
        // block survives a language without one.
        "mutual" => {
            for inner in cx.children(node) {
                item(cx, inner, module);
            }
        }
        // A `#eval` of a name runs it, which is how the writer spells a test.
        "hash_command" => {}
        kind if kind.contains("comment") => {}
        _ => module.items.push(Item::Unsupported(cx.unsupported(node))),
    }
}

/// The doc lines above a declaration.
fn doc(cx: &Cx, node: Node<'_>) -> Vec<String> {
    doc_above(cx, node, &["/--", "--!", "--", "/-!", "/-"])
        .into_iter()
        .map(|line| line.trim_end_matches("-/").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Every child under a repeated field name, which tree-sitter gives one at a time.
fn fields<'t>(cx: &Cx, node: Node<'t>, name: &str) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    let found: Vec<Node<'t>> = node
        .children_by_field_name(name, &mut cursor)
        .filter(|c| c.is_named())
        .collect();
    let _ = cx;
    found
}

/// The word a declaration opens with: `def`, `abbrev`, `theorem`.
fn keyword(cx: &Cx, node: Node<'_>) -> String {
    cx.text(node)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

// ============================================================
// Declarations
// ============================================================

/// A `structure`, which is what a record crosses as.
fn record(cx: &Cx, node: Node<'_>) -> Item {
    let name = cx.field_text(node, "name").unwrap_or_default();
    let mut record = Record {
        doc: doc(cx, node),
        name,
        fields: Vec::new(),
        extends: None,
        // Lean has no visibility on a declaration, and a file's declarations are its
        // module's surface.
        exported: true,
        methods: Vec::new(),
    };
    for field in fields(cx, node, "fields") {
        let Some(name) = cx.field_text(field, "name") else {
            continue;
        };
        record.fields.push(Field {
            doc: doc(cx, field),
            name,
            ty: cx.field(field, "type").map(|t| ty(cx, t)),
            default: cx.field(field, "value").map(|v| expr(cx, v)),
            exported: true,
        });
    }
    Item::Record(record)
}

/// An `inductive`, which is what a closed choice crosses as.
fn sum(cx: &Cx, node: Node<'_>) -> Item {
    let name = cx.field_text(node, "name").unwrap_or_default();
    let mut sum = Sum {
        doc: doc(cx, node),
        name,
        variants: Vec::new(),
        exported: true,
    };
    for constructor in fields(cx, node, "constructors") {
        let Some(name) = cx
            .field(constructor, "name")
            .and_then(|n| cx.children(n).last().map(|part| cx.text(*part)))
        else {
            continue;
        };
        let mut variant = Variant {
            doc: doc(cx, constructor),
            name: pascal(&name),
            tag: None,
            fields: Vec::new(),
        };
        for binder in cx
            .field(constructor, "binders")
            .map(|b| cx.children(b))
            .unwrap_or_default()
        {
            for (name, declared) in binder_names(cx, binder) {
                variant.fields.push(Field {
                    doc: Vec::new(),
                    name,
                    ty: declared,
                    default: None,
                    exported: true,
                });
            }
        }
        sum.variants.push(variant);
    }
    Item::Sum(sum)
}

/// The names one binder declares, each with the type they share.
fn binder_names(cx: &Cx, binder: Node<'_>) -> Vec<(String, Option<Type>)> {
    let declared = cx.field(binder, "type").map(|t| ty(cx, t));
    let named = fields(cx, binder, "name");
    match named.is_empty() {
        // A binder with no name field is a bare type, which names nothing to bind.
        true => Vec::new(),
        false => named
            .into_iter()
            .map(|n| (cx.text(n), declared.clone()))
            .collect(),
    }
}

/// A `def`, an `abbrev` or a `theorem`. Which one decides what it becomes.
fn definition(cx: &Cx, node: Node<'_>) -> Item {
    let names = cx
        .field(node, "name")
        .map(|n| cx.children(n))
        .unwrap_or_default();
    let Some(last) = names.last() else {
        return Item::Unsupported(cx.unsupported(node));
    };
    let name = cx.text(*last);
    // `def Reading.label` names the namespace and the declaration, and the namespace is
    // the type the method belongs to.
    let owner = (names.len() > 1).then(|| cx.text(names[0]));
    let binders: Vec<Node<'_>> = cx
        .field(node, "binders")
        .map(|b| cx.children(b))
        .unwrap_or_default();
    let declared = cx.field(node, "type");
    let body = cx.field(node, "body");

    // `abbrev Meters := Int` names a type and nothing else.
    if keyword(cx, node) == "abbrev" {
        let base = body.map(|b| ty(cx, b)).unwrap_or(Type::Unit);
        return Item::Newtype(Newtype {
            doc: doc(cx, node),
            name,
            base,
            exported: true,
        });
    }

    // A declaration with no arguments and a value is a constant, unless its body is a
    // block, which makes it a function of nothing.
    let acts = declared.is_some_and(|t| in_io(cx, t));
    if binders.is_empty() && !acts {
        if let Some(value) = body.filter(|b| !is_block(cx, *b)) {
            return Item::Constant(Constant {
                doc: doc(cx, node),
                name,
                ty: declared.map(|t| ty(cx, t)),
                value: expr(cx, value),
                exported: true,
            });
        }
    }

    let mut params = Vec::new();
    let mut receiver_binding = None;
    for binder in binders {
        for (bound, declared) in binder_names(cx, binder) {
            // The receiver is the first argument, and the reason `p.area` resolves.
            let is_receiver = owner.is_some()
                && params.is_empty()
                && receiver_binding.is_none()
                && declared
                    .as_ref()
                    .is_some_and(|t| named_is(t, owner.as_ref()));
            if is_receiver {
                receiver_binding = Some(bound);
                continue;
            }
            params.push(Param {
                name: bound,
                ty: declared,
                default: None,
                kind: ParamKind::Normal,
            });
        }
    }

    let returns = declared.map(|t| ty(cx, t)).map(|t| match t {
        // `IO Unit` answers with nothing, and every other target says so by saying
        // nothing.
        Type::Unit => Type::Unit,
        other => other,
    });
    let function = Function {
        doc: doc(cx, node),
        name,
        receiver: owner,
        receiver_binding,
        params,
        returns,
        body: body.map(|b| block(cx, b)).unwrap_or_default(),
        exported: true,
        is_async: false,
        is_property: false,
        is_constructor: false,
        is_private: false,
    };
    Item::Function(function)
}

/// Is this type the named one?
fn named_is(t: &Type, name: Option<&String>) -> bool {
    matches!((t, name), (Type::Named { name: t, .. }, Some(n)) if t == n)
}

/// Does this type say the definition acts, rather than computes?
fn in_io(cx: &Cx, node: Node<'_>) -> bool {
    if node.kind() != "application" {
        return false;
    }
    cx.field_text(node, "name").as_deref() == Some("IO")
}

/// Is this body a block of statements rather than one expression?
fn is_block(cx: &Cx, node: Node<'_>) -> bool {
    match node.kind() {
        "do" => true,
        // `Id.run do` is a block that answers with a value.
        "application" => run_block(cx, node).is_some(),
        _ => false,
    }
}

/// The `do` inside `Id.run do`, which is how a pure body holds statements.
fn run_block<'t>(cx: &Cx, node: Node<'t>) -> Option<Node<'t>> {
    let name = cx.field(node, "name")?;
    if cx.text(name).replace(char::is_whitespace, "") != "Id.run" {
        return None;
    }
    fields(cx, node, "arguments")
        .into_iter()
        .find(|a| a.kind() == "do")
}

// ============================================================
// Statements
// ============================================================

/// A definition's body as statements.
fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
    match node.kind() {
        "do" => elements(cx, node),
        "application" => match run_block(cx, node) {
            Some(inner) => elements(cx, inner),
            None => vec![answer(cx, node)],
        },
        // A body that is one expression answers with it, which every reader here writes
        // as a return.
        _ => vec![answer(cx, node)],
    }
}

/// A body that is one expression, as the statement that answers with it.
fn answer(cx: &Cx, node: Node<'_>) -> Stmt {
    match node.kind() {
        "match" => match_statement(cx, node, true),
        "if" => if_statement(cx, node, true),
        _ => Stmt::Return(Some(expr(cx, node))),
    }
}

/// The elements of a `do` block, however the tree nested them.
fn elements(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    for child in cx.children_with_comments(node) {
        push_element(cx, child, &mut out);
    }
    out
}

/// One element, and the tail a nested `let` swallowed.
fn push_element(cx: &Cx, node: Node<'_>, out: &mut Vec<Stmt>) {
    // A term `let` takes the rest of the block as its `body`. The binding is the same
    // either way, and what follows it is a sibling in every other reading.
    if node.kind() == "let" {
        out.push(term_let(cx, node));
        if let Some(rest) = cx.field(node, "body") {
            push_element(cx, rest, out);
        }
        return;
    }
    out.push(statement(cx, node));
}

/// `let x := v` written as the term it also is.
fn term_let(cx: &Cx, node: Node<'_>) -> Stmt {
    let named = cx.field_text(node, "name").unwrap_or_default();
    // `let mut x` reads as a `let` of `mut` taking `x`: `mut` sits where a name goes,
    // and no rule forbids it there. B831.
    let (name, mutable) = match named == "mut" {
        true => (
            cx.field(node, "parameters")
                .map(|p| cx.text(p).trim().to_string())
                .unwrap_or_default(),
            true,
        ),
        false => (named, false),
    };
    let declared = cx.field(node, "type").map(|t| ty(cx, t));
    Stmt::Let {
        name,
        value: given(cx, node).map(|v| named_by(v, declared.as_ref())),
        ty: declared,
        mutable,
    }
}

fn statement(cx: &Cx, node: Node<'_>) -> Stmt {
    match node.kind() {
        kind if kind.contains("comment") => {
            let text = cx.text(node);
            let text = text.trim().trim_start_matches('-').trim();
            Stmt::Comment(text.to_string())
        }
        "do_let" => binding(cx, node, "pattern", false),
        "let_mut" => binding(cx, node, "name", true),
        // `let x ← f` names what an action produced, which is a binding like any other.
        "let_bind" => Stmt::Let {
            name: cx.field_text(node, "name").unwrap_or_default(),
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            value: given(cx, node),
            mutable: false,
        },
        "let" => term_let(cx, node),
        "reassign" => reassign(cx, node),
        "do_return" | "return" => Stmt::Return(cx.field(node, "value").map(|v| expr(cx, v))),
        "do_break" | "break" => Stmt::Break,
        "do_continue" | "continue" => Stmt::Continue,
        "do_if" | "if" => if_statement(cx, node, false),
        "do_while" => Stmt::While {
            condition: cx
                .field(node, "condition")
                .map(|c| expr(cx, c))
                .unwrap_or(Expr::Bool(true)),
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
        },
        "for_in" => for_statement(cx, node),
        "do_match" | "match" => match_statement(cx, node, false),
        "try" => try_statement(cx, node),
        _ => match thrown(&expr(cx, node)) {
            Some(value) => Stmt::Throw(value),
            None => Stmt::Expr(expr(cx, node)),
        },
    }
}

/// What a `throw` throws, where the expression is one.
///
/// `IO.userError` is the only failure a Lean `do` block raises without a type of its
/// own, and the words inside it are the failure.
fn thrown(e: &Expr) -> Option<Expr> {
    let Expr::Call { callee, args } = e else {
        return None;
    };
    if !matches!(&**callee, Expr::Name(n) if n == "throw") {
        return None;
    }
    let only = args.first()?;
    Some(match only {
        Expr::Call { callee, args } if path_of(callee).as_deref() == Some("IO.userError") => {
            args.first().cloned().unwrap_or_else(|| only.clone())
        }
        other => other.clone(),
    })
}

/// A `let`, with the type it declares handed to the value it binds.
fn binding(cx: &Cx, node: Node<'_>, named: &str, mutable: bool) -> Stmt {
    let declared = cx.field(node, "type").map(|t| ty(cx, t));
    Stmt::Let {
        name: cx.field_text(node, named).unwrap_or_default(),
        value: given(cx, node).map(|v| named_by(v, declared.as_ref())),
        ty: declared,
        mutable,
    }
}

/// `x := v`, and the shapes that are not assignments at all.
///
/// Lean answers a growing collection with a new value. So `xs := xs.append v` spells
/// the append every other language here writes as a statement on its own.
fn reassign(cx: &Cx, node: Node<'_>) -> Stmt {
    let name = cx.field_text(node, "name").unwrap_or_default();
    let value = cx
        .field(node, "value")
        .map(|v| expr(cx, v))
        .unwrap_or(Expr::Null);
    match grows_in_place(&name, &value) {
        Some(stmt) => stmt,
        None => Stmt::Assign {
            target: Expr::Name(name),
            value,
        },
    }
}

/// The statement a self-assignment means, where it means one.
fn grows_in_place(name: &str, value: &Expr) -> Option<Stmt> {
    let Expr::Call { callee, args } = value else {
        return None;
    };
    let Expr::Field { of, name: called } = &**callee else {
        return None;
    };
    if !matches!(&**of, Expr::Name(n) if n == name) {
        return None;
    }
    Some(match (called.as_str(), args.as_slice()) {
        ("append" | "add" | "remove", [_]) => Stmt::Expr(value.clone()),
        // `xs := xs.set! i v` and `m := m.insert k v` both write through a key.
        ("set!" | "insert", [index, held]) => Stmt::Assign {
            target: Expr::Index {
                of: of.clone(),
                index: Box::new(index.clone()),
            },
            value: held.clone(),
        },
        _ => return None,
    })
}

/// `if c then … else …`, as a statement or as the value a body answers with.
fn if_statement(cx: &Cx, node: Node<'_>, answers: bool) -> Stmt {
    let condition = cx
        .field(node, "condition")
        .map(|c| expr(cx, c))
        .unwrap_or(Expr::Bool(true));
    let branch = |name: &str| -> Vec<Stmt> {
        match cx.field(node, name) {
            Some(b) if answers => block(cx, b),
            Some(b) => block_or_statement(cx, b),
            None => Vec::new(),
        }
    };
    Stmt::If {
        condition,
        then: branch("then"),
        otherwise: branch("else"),
    }
}

/// A branch, which is a `do` where it holds several statements and one where it does not.
fn block_or_statement(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
    match node.kind() {
        "do" => elements(cx, node),
        _ => {
            let mut out = Vec::new();
            push_element(cx, node, &mut out);
            out
        }
    }
}

/// `for x in xs do`, and the range form a counted loop crosses as.
fn for_statement(cx: &Cx, node: Node<'_>) -> Stmt {
    let binding = cx.field_text(node, "var").unwrap_or_default();
    let body = cx
        .field(node, "body")
        .map(|b| block_or_statement(cx, b))
        .unwrap_or_default();
    let Some(iterable) = cx.field(node, "iterable") else {
        return Stmt::ForEach {
            binding,
            iterable: Expr::Null,
            body,
        };
    };
    // `[a:b]` walks the numbers from `a` up to `b`, which is the counted loop every
    // other language here writes with a header.
    if let Some((from, to)) = range(cx, iterable) {
        return Stmt::CountedFor {
            init: Some(Box::new(Stmt::Let {
                name: binding.clone(),
                ty: Some(Type::Int),
                value: Some(from),
                mutable: true,
            })),
            condition: Some(Expr::Binary {
                op: BinaryOp::Lt,
                left: Box::new(Expr::Name(binding.clone())),
                right: Box::new(to),
            }),
            update: Some(Box::new(Stmt::Assign {
                target: Expr::Name(binding.clone()),
                value: Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Name(binding)),
                    right: Box::new(Expr::Int("1".to_string())),
                },
            })),
            body,
            source: cx.text(node),
            line: cx.line(node),
        };
    }
    Stmt::ForEach {
        binding,
        iterable: expr(cx, iterable),
        body,
    }
}

/// The two ends of `[a:b]`, where the iterable is one.
fn range(cx: &Cx, node: Node<'_>) -> Option<(Expr, Expr)> {
    let text = cx.text(node);
    let inside = text.strip_prefix('[')?.strip_suffix(']')?;
    let (from, to) = inside.split_once(':')?;
    if to.contains(':') {
        return None;
    }
    Some((
        reparsed(cx, node, from.trim())?,
        reparsed(cx, node, to.trim())?,
    ))
}

/// A fragment of Lean read as an expression, for the places the tree hands over text.
fn reparsed(cx: &Cx, at: Node<'_>, text: &str) -> Option<Expr> {
    if text.is_empty() {
        return None;
    }
    // The common shapes, without a second parse: a name, a number, a call.
    if text.chars().all(|c| c.is_ascii_digit()) {
        return Some(Expr::Int(text.to_string()));
    }
    if text
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return Some(Expr::Name(text.to_string()));
    }
    let _ = at;
    // `Int.toNat n`, which is how the writer spells the end of a range.
    let mut parts = text.split_whitespace();
    let head = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    if head == "Int.toNat" && rest.len() == 1 {
        return Some(Expr::Name(rest[0].to_string()));
    }
    Some(Expr::Unsupported(Unsupported {
        construct: "range bound".to_string(),
        source: text.to_string(),
        line: cx.line(at),
    }))
}

/// `match x with | … => …`.
fn match_statement(cx: &Cx, node: Node<'_>, answers: bool) -> Stmt {
    let subject = cx
        .field(node, "scrutinees")
        .map(|s| expr(cx, s))
        .unwrap_or(Expr::Null);
    let arms: Vec<Node<'_>> = cx
        .children(node)
        .into_iter()
        .filter(|c| c.kind() == "match_arm" || c.kind() == "do_match_arm")
        .collect();

    let mut variants: Vec<VariantArm> = Vec::new();
    let mut literals: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
    let mut default: Vec<Stmt> = Vec::new();
    let mut sum = String::new();
    for arm in arms {
        let body = match cx.field(arm, "body") {
            Some(b) if answers => block(cx, b),
            Some(b) => block_or_statement(cx, b),
            None => Vec::new(),
        };
        let patterns = fields(cx, arm, "patterns");
        let Some(first) = patterns.first().copied() else {
            continue;
        };
        // `_` takes whatever the arms above did not.
        if cx.text(first).trim() == "_" {
            default = body;
            continue;
        }
        match variant_pattern(cx, first) {
            Some(matched) => {
                if sum.is_empty() {
                    sum = matched.owner;
                }
                variants.push(VariantArm {
                    variant: pascal(&matched.variant),
                    bindings: matched.bindings,
                    body,
                });
            }
            None => literals.push((patterns.iter().map(|p| expr(cx, *p)).collect(), body)),
        }
    }

    match variants.is_empty() {
        true => Stmt::Switch {
            subject,
            arms: literals,
            default,
        },
        false => Stmt::MatchVariants {
            subject,
            sum,
            arms: variants,
            default,
        },
    }
}

/// A constructor pattern taken apart: which type, which variant, and what it binds.
struct VariantMatch {
    owner: String,
    variant: String,
    /// The payload fields the arm reads, as (field, local). Lean binds a constructor's
    /// payload by position, so the two are the same word.
    bindings: Vec<(String, String)>,
}

/// A constructor pattern: the type it belongs to, the variant, and what it binds.
fn variant_pattern(cx: &Cx, node: Node<'_>) -> Option<VariantMatch> {
    let (head, bound) = match node.kind() {
        "projection" => (node, Vec::new()),
        "application" => (
            cx.field(node, "name")?,
            fields(cx, node, "arguments")
                .into_iter()
                .map(|a| cx.text(a))
                .collect(),
        ),
        _ => return None,
    };
    if head.kind() != "projection" {
        return None;
    }
    let owner = cx.field_text(head, "term")?;
    let variant = cx.field_text(head, "name")?;
    // A constructor's name opens lower case, and a type's opens upper.
    if !owner.starts_with(char::is_uppercase) {
        return None;
    }
    let bindings = bound.into_iter().map(|name| (name.clone(), name)).collect();
    Some(VariantMatch {
        owner,
        variant,
        bindings,
    })
}

/// `try … catch e => …`.
fn try_statement(cx: &Cx, node: Node<'_>) -> Stmt {
    // A `try` body is a sequence, and the tree gives one `body` field per element of
    // it. Taking the first would lose every statement after the first.
    let sequence = |name: &str| -> Vec<Stmt> {
        fields(cx, node, name)
            .into_iter()
            .flat_map(|part| block_or_statement(cx, part))
            .collect()
    };
    let body = sequence("body");
    let handled = sequence("handler");
    let catches = match handled.is_empty() {
        true => Vec::new(),
        false => vec![Catch {
            binding: cx.field_text(node, "var"),
            ty: None,
            body: handled,
        }],
    };
    Stmt::Try {
        body,
        catches,
        finally: Vec::new(),
        source: cx.text(node),
        line: cx.line(node),
    }
}

// ============================================================
// Expressions
// ============================================================

fn expr(cx: &Cx, node: Node<'_>) -> Expr {
    match node.kind() {
        "identifier" | "escaped_identifier" => name(cx, node),
        "number" => number(cx, node),
        "float" => Expr::Float(cx.text(node)),
        "true" => Expr::Bool(true),
        "false" => Expr::Bool(false),
        "hole" | "sorry" => Expr::Null,
        "string" | "interpolated_string" | "char" => text(cx, node),
        "parenthesized" => match cx.children(node).first() {
            Some(inner) => expr(cx, *inner),
            None => Expr::Null,
        },
        "projection" => {
            let of = cx.field(node, "term").map(|t| expr(cx, t));
            let named = cx.field_text(node, "name").unwrap_or_default();
            let read = match of {
                Some(of) => Expr::Field {
                    of: Box::new(of),
                    name: named,
                },
                None => Expr::Name(named),
            };
            if let Some(value) = path_of(&read).as_deref().and_then(constant_path) {
                return value;
            }
            // `xs.size` and `s.toUpper` take no argument, so neither ever arrives as a
            // call. Lean names these on the value itself, and a structure field rarely
            // carries one of the names.
            match method(&read, &[]) {
                Some(shared) => shared,
                None => read,
            }
        }
        "application" => application(cx, node),
        "binary_expression" => binary(cx, node),
        "unary_expression" | "prefix_expression" => unary(cx, node),
        "structure_instance" => structure_instance(cx, node),
        "array" | "list" => {
            Expr::ListLit(cx.children(node).into_iter().map(|c| expr(cx, c)).collect())
        }
        "tuple" | "anonymous_constructor" => {
            Expr::Tuple(cx.children(node).into_iter().map(|c| expr(cx, c)).collect())
        }
        // `xs[i]!` asserts the element is there, which is what an index is everywhere
        // that does not make the assertion explicit.
        "subscript" => subscript(cx, node),
        "named_argument" => match (cx.field_text(node, "name"), cx.field(node, "value")) {
            (Some(name), Some(value)) => Expr::Keyword {
                name,
                value: Box::new(expr(cx, value)),
            },
            _ => carried(cx, node),
        },
        "if" => Expr::Ternary {
            condition: Box::new(
                cx.field(node, "condition")
                    .map(|c| expr(cx, c))
                    .unwrap_or(Expr::Bool(true)),
            ),
            then: Box::new(
                cx.field(node, "then")
                    .map(|t| expr(cx, t))
                    .unwrap_or(Expr::Null),
            ),
            otherwise: Box::new(
                cx.field(node, "else")
                    .map(|t| expr(cx, t))
                    .unwrap_or(Expr::Null),
            ),
        },
        "fun" => lambda(cx, node),
        "match" => carried(cx, node),
        // `← f` names what an action produced, and every target here reads a call for it.
        "bind" | "arrow_bind" | "do_bind" => match cx.children(node).first() {
            Some(inner) => expr(cx, *inner),
            None => Expr::Null,
        },
        _ => carried(cx, node),
    }
}

fn carried(cx: &Cx, node: Node<'_>) -> Expr {
    // `(← f x)` arrives as a parenthesised thing the grammar has no rule for. Its one
    // child is the call, and the arrow says only where the value comes from.
    let inner = cx.children(node);
    if cx.text(node).trim_start().starts_with('←') {
        if let Some(one) = inner.first() {
            return expr(cx, *one);
        }
    }
    Expr::Unsupported(cx.unsupported(node))
}

/// A record built without naming its type, which the binding beside it names.
///
/// `let b : Box := { value := 9 }` puts the type on the binding, and Lean takes it from
/// there. Every target here writes the type at the construction.
fn named_by(value: Expr, declared: Option<&Type>) -> Expr {
    let Expr::RecordLit { ty, fields } = value else {
        return value;
    };
    if !ty.is_empty() {
        return Expr::RecordLit { ty, fields };
    }
    match declared {
        Some(Type::Named { name, .. }) => Expr::RecordLit {
            ty: name.clone(),
            fields,
        },
        _ => Expr::RecordLit { ty, fields },
    }
}

/// The value a binding holds, where `default` stands for none.
///
/// Lean has no uninitialised binding, so the writer puts the type's own default where
/// the source wrote nothing. Reading that back as a value would invent one.
fn given(cx: &Cx, node: Node<'_>) -> Option<Expr> {
    let value = cx.field(node, "value")?;
    if cx.text(value).trim() == "default" {
        return None;
    }
    Some(expr(cx, value))
}

/// A bare name, and the few that are values rather than names.
fn name(cx: &Cx, node: Node<'_>) -> Expr {
    match cx.text(node).as_str() {
        "true" => Expr::Bool(true),
        "false" => Expr::Bool(false),
        "none" => Expr::Null,
        other => Expr::Name(other.to_string()),
    }
}

/// A path that is a value: the empty collections, which take no argument and so never
/// arrive as a call.
fn constant_path(path: &str) -> Option<Expr> {
    Some(match path {
        "Std.HashSet.emptyWithCapacity" | "HashSet.emptyWithCapacity" | "Std.HashSet.empty" => {
            Expr::SetLit(Vec::new())
        }
        "Std.HashMap.emptyWithCapacity" | "HashMap.emptyWithCapacity" | "Std.HashMap.empty" => {
            Expr::MapLit(Vec::new())
        }
        _ => return None,
    })
}

fn number(cx: &Cx, node: Node<'_>) -> Expr {
    let text = cx.text(node);
    match text.contains('.') || text.contains('e') {
        true => Expr::Float(text),
        false => Expr::Int(text),
    }
}

/// A string, plain or interpolated.
fn text(cx: &Cx, node: Node<'_>) -> Expr {
    let inner = match node.kind() {
        "interpolated_string" => cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "string")
            .unwrap_or(node),
        _ => node,
    };
    let holes: Vec<Node<'_>> = cx
        .children(inner)
        .into_iter()
        .filter(|c| c.kind() == "interpolation")
        .collect();
    if holes.is_empty() {
        return Expr::Str(unquoted(&cx.text(inner)));
    }
    let mut parts = Vec::new();
    let mut at = inner.start_byte() + 1;
    for hole in holes {
        let before = &cx.source[at..hole.start_byte()];
        if !before.is_empty() {
            parts.push(TemplatePart::Text(unescaped(before)));
        }
        match cx.children(hole).first() {
            Some(one) => parts.push(TemplatePart::Expr(expr(cx, *one))),
            None => parts.push(TemplatePart::Text(cx.text(hole))),
        }
        at = hole.end_byte();
    }
    let after = &cx.source[at..inner.end_byte().saturating_sub(1)];
    if !after.is_empty() {
        parts.push(TemplatePart::Text(unescaped(after)));
    }
    Expr::Template(parts)
}

/// A string literal's text, without its quotes and with its escapes read.
fn unquoted(text: &str) -> String {
    let inner = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    unescaped(inner)
}

fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            // A brace escapes because interpolation claims it, and it stands for itself.
            Some('{') => out.push('{'),
            Some('}') => out.push('}'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `f a b`, and the shared spellings Lean writes its own way.
fn application(cx: &Cx, node: Node<'_>) -> Expr {
    // The tree nests an application per argument, so the head of a chain carries the
    // rest.
    let Some((callee, args)) = gathered(cx, node) else {
        return carried(cx, node);
    };
    let callee = Box::new(callee);
    if let Some(shared) = builtin(cx, &callee, &args) {
        return shared;
    }
    if let Some(shared) = method(&callee, &args) {
        return shared;
    }
    match args.is_empty() {
        true => *callee,
        false => Expr::Call { callee, args },
    }
}

/// A call and every argument it takes.
///
/// The tree nests one application per argument, so `m.insert k v` arrives as `m.insert k`
/// applied to `v`. Reading the inner one as a shared call names it before its arguments
/// are in hand, and a map's two-argument insert reads as a set's one-argument add.
fn gathered(cx: &Cx, node: Node<'_>) -> Option<(Expr, Vec<Expr>)> {
    let head = cx.field(node, "name")?;
    let mut args: Vec<Expr> = fields(cx, node, "arguments")
        .into_iter()
        .map(|a| expr(cx, a))
        .collect();
    if head.kind() != "application" {
        return Some((expr(cx, head), args));
    }
    let (callee, mut earlier) = gathered(cx, head)?;
    earlier.append(&mut args);
    Some((callee, earlier))
}

/// A call on a receiver, where Lean's name for it is not the shared one.
///
/// Lean answers a growing collection with a new value rather than changing one, so
/// `xs.push x` reads as the `append` every other language here writes.
fn method(callee: &Expr, args: &[Expr]) -> Option<Expr> {
    let Expr::Field { of, name } = callee else {
        return None;
    };
    let on = |name: &str, args: Vec<Expr>| Expr::Call {
        callee: Box::new(Expr::Field {
            of: of.clone(),
            name: name.to_string(),
        }),
        args,
    };
    Some(match (name.as_str(), args) {
        ("push", [one]) => on("append", vec![one.clone()]),
        ("erase", [one]) => on("remove", vec![one.clone()]),
        ("toUpper", []) => on("upper", Vec::new()),
        ("toLower", []) => on("lower", Vec::new()),
        ("trim", []) => on("strip", Vec::new()),
        ("contains", [one]) => on("contains", vec![one.clone()]),
        // A set answers `insert` with the one value it holds. A map answers it with a
        // key and a value, which is a write through the key: `reassign` reads that.
        ("insert", [one]) => on("add", vec![one.clone()]),
        // `size` and `length` count, and every reader here spells that `len`.
        ("size" | "length", []) => Expr::Call {
            callee: Box::new(Expr::Name("len".to_string())),
            args: vec![(**of).clone()],
        },
        ("get!", [one]) => Expr::Index {
            of: of.clone(),
            index: Box::new(one.clone()),
        },
        ("getD", [fallback]) => Expr::Coalesce {
            value: of.clone(),
            fallback: Box::new(fallback.clone()),
        },
        ("extract", [from, to]) => Expr::Call {
            callee: Box::new(Expr::Name("slice".to_string())),
            args: vec![(**of).clone(), from.clone(), to.clone()],
        },
        // `xs.filter p |>.map f` is how Lean builds a collection from another, and the
        // shared vocabulary has one construct for the whole shape.
        ("map", [Expr::Lambda { params, body, .. }]) => {
            let binding = params.first()?.name.clone();
            let (iterable, condition) = filtered(of, &binding);
            Expr::Comprehension {
                element: body.clone(),
                binding,
                iterable: Box::new(iterable),
                condition,
            }
        }
        // A `filter` with no `map` after it keeps what it takes, so the element is the
        // binding itself.
        ("filter", [Expr::Lambda { params, body, .. }]) => {
            let binding = params.first()?.name.clone();
            Expr::Comprehension {
                element: Box::new(Expr::Name(binding.clone())),
                binding,
                iterable: of.clone(),
                condition: Some(body.clone()),
            }
        }
        _ => return None,
    })
}

/// What a `map` runs over, and the test a `filter` put in front of it.
fn filtered(of: &Expr, binding: &str) -> (Expr, Option<Box<Expr>>) {
    let Expr::Call { callee, args } = of else {
        return (of.clone(), None);
    };
    let Expr::Field { of: inner, name } = &**callee else {
        return (of.clone(), None);
    };
    if name != "filter" {
        return (of.clone(), None);
    }
    // One binding for the whole comprehension, so the two lambdas have to agree on it.
    match args.first() {
        Some(Expr::Lambda { params, body, .. })
            if params.first().is_some_and(|p| p.name == binding) =>
        {
            ((**inner).clone(), Some(body.clone()))
        }
        _ => (of.clone(), None),
    }
}

/// Lean's spelling of a shared call, read back as the shared one.
fn builtin(cx: &Cx, callee: &Expr, args: &[Expr]) -> Option<Expr> {
    let path = path_of(callee)?;
    let call = |name: &str, args: Vec<Expr>| Expr::Call {
        callee: Box::new(Expr::Name(name.to_string())),
        args,
    };
    let binary = |op: BinaryOp, args: &[Expr]| Expr::Binary {
        op,
        left: Box::new(args[0].clone()),
        right: Box::new(args[1].clone()),
    };
    let _ = cx;
    Some(match (path.as_str(), args) {
        ("IO.println", [one]) => call("print", vec![one.clone()]),
        ("IO.print", [one]) => call("print", vec![one.clone()]),
        ("toString", [one]) => call("str", vec![one.clone()]),
        ("frShow", [one]) => call("str", vec![one.clone()]),
        ("Int.ofNat", [one]) => one.clone(),
        ("Int.toNat", [one]) => one.clone(),
        ("Float.ofInt", [one]) => call("float", vec![one.clone()]),
        ("frTrunc", [one]) => call("trunc", vec![one.clone()]),
        // The four divisions Lean names because its own `/` rounds the other way.
        ("Int.tdiv", [_, _]) => binary(BinaryOp::Div, args),
        ("Int.tmod", [_, _]) => binary(BinaryOp::Rem, args),
        ("Int.fdiv", [_, _]) => binary(BinaryOp::FloorDiv, args),
        ("Int.fmod", [_, _]) => binary(BinaryOp::FloorRem, args),
        ("frRem", [_, _]) => binary(BinaryOp::Rem, args),
        // A map and a set arrive built from a list of what they hold.
        ("Std.HashMap.ofList" | "HashMap.ofList", [Expr::ListLit(entries)]) => Expr::MapLit(
            entries
                .iter()
                .filter_map(|e| match e {
                    Expr::Tuple(pair) if pair.len() == 2 => {
                        Some((pair[0].clone(), pair[1].clone()))
                    }
                    _ => None,
                })
                .collect(),
        ),
        ("Std.HashSet.ofList" | "HashSet.ofList", [Expr::ListLit(items)]) => {
            Expr::SetLit(items.clone())
        }
        ("String.intercalate", [separator, parts]) => Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(separator.clone()),
                name: "join".to_string(),
            }),
            args: vec![parts.clone()],
        },
        _ => return None,
    })
}

/// A callee as the dotted path it spells, where it spells one.
fn path_of(callee: &Expr) -> Option<String> {
    match callee {
        Expr::Name(name) => Some(name.clone()),
        Expr::Field { of, name } => Some(format!("{}.{name}", path_of(of)?)),
        _ => None,
    }
}

/// `a + b`, with the operator read from between its operands.
fn binary(cx: &Cx, node: Node<'_>) -> Expr {
    let left = cx.field(node, "left");
    let right = cx.field(node, "right");
    let (Some(left), Some(right)) = (left, right) else {
        return carried(cx, node);
    };
    let spelled = cx.source[left.end_byte()..right.start_byte()].trim();
    let Some(op) = operator(spelled) else {
        return carried(cx, node);
    };
    let read = Expr::Binary {
        op,
        left: Box::new(expr(cx, left)),
        right: Box::new(expr(cx, right)),
    };
    match holds_text(&read) {
        Some(shared) => shared,
        None => read,
    }
}

/// `(s.splitOn x).length > 1` asks whether `s` holds `x`, which Lean has no shorter way
/// to ask and every other language here spells `contains`.
fn holds_text(e: &Expr) -> Option<Expr> {
    let Expr::Binary {
        op: BinaryOp::Gt,
        left,
        right,
    } = e
    else {
        return None;
    };
    if !matches!(&**right, Expr::Int(one) if one == "1") {
        return None;
    }
    let Expr::Call { callee, args } = &**left else {
        return None;
    };
    if !matches!(&**callee, Expr::Name(n) if n == "len") {
        return None;
    }
    let Some(Expr::Call { callee, args }) = args.first() else {
        return None;
    };
    let Expr::Field { of, name } = &**callee else {
        return None;
    };
    if name != "splitOn" {
        return None;
    }
    Some(Expr::Call {
        callee: Box::new(Expr::Field {
            of: of.clone(),
            name: "contains".to_string(),
        }),
        args: args.clone(),
    })
}

/// The shared operator a Lean one means.
fn operator(spelled: &str) -> Option<BinaryOp> {
    Some(match spelled {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        // Lean's own `/` and `%` on `Int` round toward negative infinity and take the
        // Euclidean remainder. Python's `//` and `%` mean the same, and nothing else
        // here does.
        "/" => BinaryOp::FloorDiv,
        "%" => BinaryOp::FloorRem,
        "==" => BinaryOp::Eq,
        "!=" | "≠" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        "<=" | "≤" => BinaryOp::Le,
        ">" => BinaryOp::Gt,
        ">=" | "≥" => BinaryOp::Ge,
        "&&" | "∧" => BinaryOp::And,
        "||" | "∨" => BinaryOp::Or,
        "^^^" => BinaryOp::Xor,
        "++" => BinaryOp::Add,
        _ => return None,
    })
}

fn unary(cx: &Cx, node: Node<'_>) -> Expr {
    let children = cx.children(node);
    let Some(operand) = children.first() else {
        return carried(cx, node);
    };
    let spelled = cx.source[node.start_byte()..operand.start_byte()].trim();
    let op = match spelled {
        "!" | "¬" => UnaryOp::Not,
        "-" => UnaryOp::Neg,
        _ => return carried(cx, node),
    };
    Expr::Unary {
        op,
        operand: Box::new(expr(cx, *operand)),
    }
}

/// `xs[i]!`, and the `!` that says the element is there.
fn subscript(cx: &Cx, node: Node<'_>) -> Expr {
    let children = cx.children(node);
    let (Some(of), Some(index)) = (children.first(), children.get(1)) else {
        return carried(cx, node);
    };
    Expr::Index {
        of: Box::new(expr(cx, *of)),
        index: Box::new(expr(cx, *index)),
    }
}

/// `{ x := 1, y := "a" : P }`, which is how Lean builds a structure.
fn structure_instance(cx: &Cx, node: Node<'_>) -> Expr {
    let ty = cx.field_text(node, "type").unwrap_or_default();
    let fields = cx
        .children(node)
        .into_iter()
        .filter(|c| c.kind() == "field_assignment")
        .filter_map(|f| {
            let name = cx.field_text(f, "name")?;
            let value = cx.field(f, "value").map(|v| expr(cx, v))?;
            Some((name, value))
        })
        .collect();
    Expr::RecordLit { ty, fields }
}

/// `fun x => e`.
fn lambda(cx: &Cx, node: Node<'_>) -> Expr {
    let children = cx.children(node);
    let Some(body) = children.last() else {
        return carried(cx, node);
    };
    let params = children[..children.len().saturating_sub(1)]
        .iter()
        .flat_map(|b| match b.kind() {
            "explicit_binder" | "binders" => binder_names(cx, *b),
            _ => vec![(cx.text(*b), None)],
        })
        .map(|(name, ty)| Param {
            name,
            ty,
            default: None,
            kind: ParamKind::Normal,
        })
        .collect();
    Expr::Lambda {
        params,
        returns: None,
        body: Box::new(expr(cx, *body)),
    }
}

// ============================================================
// Types
// ============================================================

/// A type, read back into the shared vocabulary.
fn ty(cx: &Cx, node: Node<'_>) -> Type {
    let text = cx.text(node);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    ty_text(&text)
}

/// The same, from the text, since a type arrives as one word as often as as a tree.
fn ty_text(text: &str) -> Type {
    let text = text.trim();
    // Brackets around a whole type say only where it ends.
    if let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        if balanced(inner) {
            return ty_text(inner);
        }
    }
    match text {
        "Unit" | "PUnit" => return Type::Unit,
        "Bool" => return Type::Bool,
        "Int" | "Nat" | "Int64" | "UInt64" | "USize" => return Type::Int,
        "Float" => return Type::Float,
        "String" | "Char" => return Type::String,
        _ => {}
    }
    // `A → B`, which is how Lean writes a function.
    if let Some((params, returns)) = split_arrow(text) {
        return Type::Fn {
            params: params.iter().map(|p| ty_text(p)).collect(),
            returns: Box::new(ty_text(&returns)),
        };
    }
    // `A × B`, several types travelling as one.
    if text.contains('×') && balanced(text) {
        let parts: Vec<Type> = split_top(text, '×').iter().map(|p| ty_text(p)).collect();
        if parts.len() > 1 {
            return Type::Tuple(parts);
        }
    }
    let mut words = split_application(text);
    let head = words.remove(0);
    let arg = |at: usize| -> Type { words.get(at).map(|w| ty_text(w)).unwrap_or(Type::Unit) };
    match (head.as_str(), words.len()) {
        // `Array` and not `List`: the writer chose it because the source indexes and
        // grows these, and reading it back means the same thing.
        ("Array" | "List", 1) => Type::List(Box::new(arg(0))),
        ("Option", 1) => Type::Optional(Box::new(arg(0))),
        ("Std.HashSet" | "HashSet", 1) => Type::Set(Box::new(arg(0))),
        ("Std.HashMap" | "HashMap", 2) => Type::Map(Box::new(arg(0)), Box::new(arg(1))),
        // `IO T` says the definition acts, and `T` is what it answers with.
        ("IO", 1) => arg(0),
        ("Except", 2) => arg(1),
        (_, 0) => Type::named(head),
        _ => Type::Named {
            name: head,
            args: words.iter().map(|w| ty_text(w)).collect(),
        },
    }
}

/// A type applied to its arguments, split where the brackets allow.
fn split_application(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut word = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                word.push(c);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                word.push(c);
            }
            ' ' if depth == 0 => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            _ => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    if out.is_empty() {
        out.push(text.to_string());
    }
    out
}

/// The parameters and the answer of `A → B → C`, split at the top level.
fn split_arrow(text: &str) -> Option<(Vec<String>, String)> {
    let parts = split_top(text, '→');
    if parts.len() < 2 {
        return None;
    }
    let mut parts = parts;
    let returns = parts.pop()?;
    Some((parts, returns))
}

/// Split on a separator the brackets do not enclose.
fn split_top(text: &str, separator: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut word = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                word.push(c);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                word.push(c);
            }
            c if c == separator && depth == 0 => out.push(std::mem::take(&mut word)),
            _ => word.push(c),
        }
    }
    out.push(word);
    out.into_iter()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Do the brackets in this text open and close?
fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}
