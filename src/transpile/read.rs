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

/// `file_stem` is the source file's own name, and only Zig wants it. The
/// file-as-struct idiom names its type `Self`, and everyone else calls the type by
/// the file's name.
pub fn read(
    language: Language,
    source: &str,
    root: Node<'_>,
    file_stem: Option<&str>,
) -> Result<Module> {
    let lines = LineIndex::new(source);
    let cx = Cx {
        source,
        lines: &lines,
    };
    let mut module = match language {
        Language::Rust => rust::module(&cx, root),
        Language::Python => python::module(&cx, root),
        Language::Go => go::module(&cx, root),
        Language::Java => java::module(&cx, root),
        Language::Zig => zig::module(&cx, root, file_stem),
        Language::TypeScript | Language::Tsx => typescript::module(&cx, root),
        other => bail!(
            "there is no reader for {other}: translating out of it would mean inventing \
             what its constructs mean."
        ),
    };
    settle_methods(&mut module);
    // Only for the languages that run `main` implicitly. Python and TypeScript run a
    // module top to bottom. A file of theirs without the call genuinely never runs
    // it, and inventing one would change what importing does.
    if matches!(
        language,
        Language::Rust | Language::Go | Language::Java | Language::Zig
    ) {
        settle_entry(&mut module);
    }
    Ok(module)
}

/// Append the program's own entry where the source language runs `main` implicitly.
///
/// Rust, Go, Java and Zig never write `main();`: declaring the function is the whole
/// arrangement. Python and TypeScript run a module top to bottom, so their programs
/// end with a call. Without one, a translated program parsed, ran and printed
/// nothing. The synthesized statement is that call. The self-running targets drop it
/// again and say so, which lets the entry cross every pairing without doubling
/// anywhere.
fn settle_entry(module: &mut Module) {
    let declares_main = module
        .items
        .iter()
        .any(|item| matches!(item, Item::Function(f) if f.name == "main"));
    if !declares_main {
        return;
    }
    let already_called = module.items.iter().any(|item| match item {
        Item::Statement(Stmt::Expr(Expr::Call { callee, .. })) => {
            matches!(callee.as_ref(), Expr::Name(name) if name == "main")
        }
        _ => false,
    });
    if already_called {
        return;
    }
    module.items.push(Item::Statement(Stmt::Expr(Expr::Call {
        callee: Box::new(Expr::Name("main".to_string())),
        args: Vec::new(),
    })));
}

/// Put every method with the type it belongs to, and bind the receiver of any that has nowhere
/// to go.
///
/// Rust and Go declare methods apart from their type; Python, TypeScript, Java and Zig declare
/// them inside it. The IR keeps them with the type, which lets one shape become the
/// other. The Rust reader said so in a comment while pushing them out as top-level functions.
/// Every writer then wrote them as free functions, with the body still reaching through a
/// receiver that nothing in the output binds. A Python `def label(prefix)` whose body says
/// `self.name`.
///
/// A method whose type is not in this file, an `impl` on somebody else's struct, has no record
/// to join. Its receiver becomes an ordinary first parameter, which Go and Zig write
/// anyway and what Python's `self` has always been.
fn settle_methods(module: &mut Module) {
    let declared: std::collections::BTreeSet<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();

    let mut orphaned: Vec<(String, Function)> = Vec::new();
    let mut kept = Vec::new();
    for item in std::mem::take(&mut module.items) {
        match item {
            Item::Function(f) if f.receiver.as_ref().is_some_and(|r| declared.contains(r)) => {
                let owner = f.receiver.clone().expect("checked");
                orphaned.push((owner, f));
            }
            // A constructor has no receiver by definition: it makes the value rather
            // than acting on one. Giving it its own type as a first parameter would be
            // reading `Handle::new(files)` as `new(handle, files)`.
            Item::Function(f) if f.is_constructor => kept.push(Item::Function(f)),
            Item::Function(mut f) if f.receiver.is_some() => {
                let ty = f.receiver.clone().expect("checked");
                let name = f.receiver_binding.clone().unwrap_or_else(|| "self".into());
                f.params.insert(
                    0,
                    Param {
                        name,
                        ty: Some(Type::named(ty)),
                        default: None,
                        kind: ParamKind::Normal,
                    },
                );
                f.receiver = None;
                f.receiver_binding = None;
                kept.push(Item::Function(f));
            }
            other => kept.push(other),
        }
    }

    for item in kept.iter_mut() {
        if let Item::Record(record) = item {
            for (owner, method) in orphaned.iter() {
                if *owner == record.name {
                    record.methods.push(method.clone());
                }
            }
            // Every method knows the type it belongs to, however it got here. A writer needs
            // that to spell a constructor at all. Three of these languages name one after its
            // type and the other three name it by habit.
            for method in record.methods.iter_mut() {
                method.receiver = Some(record.name.clone());
            }
        }
    }
    module.items = kept;
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

    /// Named children, which every reader below walks. The named children that are part
    /// of the structure.
    ///
    /// **Comments are not.** Every one of these grammars makes a comment an *extra*, which
    /// means it can appear between any two nodes anywhere in the tree, inside a parameter list,
    /// between two struct fields, in the middle of an argument list. Every reader here reads
    /// named children either positionally or through a catch-all arm. Both of those read a
    /// comment as whatever they were expecting in that position. A comment inside a Rust
    /// parameter list therefore became four invented parameters called `// how this language
    /// separates the parts of a qualified name`, which every target dutifully wrote into the
    /// signature.
    ///
    /// So they are filtered here, once, and not in the twenty places that would each have to
    /// remember. The one place that genuinely wants them, [`Cx::children_with_comments`], asks
    /// for them by name.
    fn children<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        self.children_with_comments(node)
            .into_iter()
            .filter(|c| !is_comment(*c))
            .collect()
    }

    /// The named children, comments included, for a statement block, which translates
    /// them instead of skipping them.
    fn children_with_comments<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).collect()
    }
}

/// A Rust number without the type written into it.
///
/// `0usize` and `1.5f64` put the width in the literal, which is a spelling only Rust has. Every
/// other target here reads it as an identifier glued to a number and refuses the file. The IR
/// carries the type separately, so the digits are what crosses.
fn unsuffixed(text: &str) -> String {
    const SUFFIXES: &[&str] = &[
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
        "f32", "f64",
    ];
    for suffix in SUFFIXES {
        if let Some(head) = text.strip_suffix(suffix) {
            // `0x1u8` is a suffix; `0xf64` is three hex digits. Only strip where what
            // is left is still a number.
            if head.ends_with(|c: char| c.is_ascii_digit() || c == '.' || c == '_') {
                return head.trim_end_matches('_').to_string();
            }
        }
    }
    text.to_string()
}

/// Is this node a comment, in whichever grammar produced it?
///
/// The six grammars spell it three ways — `comment`, `line_comment`, `block_comment`,
/// and Rust adds `inner_doc_comment_marker`. Matching on the substring and not on
/// the list means a seventh language cannot arrive with a fourth spelling and be read
/// as a parameter.
fn is_comment(node: Node<'_>) -> bool {
    node.kind().contains("comment")
}

/// A call whose *callee* could not be translated is not a call this understands.
///
/// Rendering it as `None()` would be syntactically valid in the target and complete
/// nonsense, `HashMap::new()` became that. Carrying the whole call instead
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
/// Only the statement's own expressions, a condition, a value, a target, never the
/// statements nested inside it. One bad line in a loop body should cost that line, not
/// the loop.
fn has_unsupported_expr(stmt: &Stmt) -> bool {
    // Exhaustive on purpose, no `_` arm. The three cases this originally missed were `MapLit`,
    // `Template` and `Comprehension`. Each produced a silent wrong answer instead of a gap:
    // `session?.user.id` inside an object literal came out as `None.id`, with the original
    // nowhere in the file. A new variant must not be able to join them quietly, so the compiler
    // is made to ask.
    fn bad(e: &Expr) -> bool {
        match e {
            Expr::Unsupported(_) => true,
            Expr::Field { of, .. } => bad(of),
            Expr::Index { of, index } => bad(of) || bad(index),
            Expr::Call { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::Binary { left, right, .. } => bad(left) || bad(right),
            Expr::Unary { operand, .. } => bad(operand),
            Expr::Await(inner) | Expr::Propagate(inner) => bad(inner),
            Expr::New { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::RecordLit { fields, .. } => fields.iter().any(|(_, value)| bad(value)),
            Expr::InstanceOf { value, ty } => bad(value) || bad(ty),
            Expr::Cast { ty, value } => bad(ty) || bad(value),
            Expr::Keyword { value, .. } => bad(value),
            Expr::Coalesce { value, fallback } => bad(value) || bad(fallback),
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => bad(condition) || bad(then) || bad(otherwise),
            Expr::Tuple(items) | Expr::ListLit(items) => items.iter().any(bad),
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
        Stmt::IfPresent { value, .. } | Stmt::WhilePresent { value, .. } => bad(value),
        Stmt::Switch { subject, arms, .. } => {
            bad(subject) || arms.iter().any(|(literals, _)| literals.iter().any(bad))
        }
        Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => bad(iterable),
        _ => false,
    }
}

/// A statement this only half understands is a statement it does not understand.
///
/// Rendering the understood half and a placeholder for the rest produced lines like
/// `sums = None`, syntactically fine, semantically a lie, and with the original
/// nowhere in the file. Carrying the whole statement instead puts the source in front
/// of whoever finishes the draft, which is the point.
fn keep_whole(cx: &Cx, node: Node<'_>, built: Stmt) -> Stmt {
    if binds_a_pattern(&built) {
        return Stmt::Unsupported(cx.unsupported(node));
    }
    // A binding whose initializer failed *as a whole* keeps its name: the marker
    // stands alone as the value and composes with nothing. Carried whole, the
    // declaration vanished into a comment while every later statement still read
    // the name, so one untranslatable initializer poisoned the lines after it. An
    // initializer with a failure *inside* it still carries whole, because a marker
    // spliced mid-expression reads as an operand and produced `None.id`.
    if let Stmt::Let {
        value: Some(Expr::Unsupported(_)),
        ..
    } = &built
    {
        return built;
    }
    if has_unsupported_expr(&built) {
        return Stmt::Unsupported(cx.unsupported(node));
    }
    built
}

/// Does this statement bind something that is not a plain name?
///
/// `for (sensor, mean) in …` destructures, and the IR binds one name. Carrying the
/// pattern text through produced `for _, (sensor, mean) := range …`, which Go cannot
/// parse, and would have been wrong even where it did parse. A destructuring is not
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
        // A `/** ... */` is one node however many lines it spans, and each of its inner lines
        // carries its own ` * ` leader. One entry per line is what a writer expects: it puts
        // the target's marker on each of them. A single entry with newlines in it got a marker
        // on the first line only, leaving the rest of a paragraph sitting in the file as code.
        for line in cleaned.trim().lines().rev() {
            let line = line.trim();
            let stripped = line.strip_prefix("* ").or_else(|| line.strip_prefix("*"));
            lines.push(stripped.unwrap_or(line).trim().to_string());
        }
        previous = sibling.prev_named_sibling();
    }
    lines.reverse();
    lines
}

/// Does this function make a value of `owner`, by that language's convention?
///
/// Rust, Go and Zig have no constructor: they have a habit, `Thing::new`, `NewThing`,
/// `Thing.init`. The habit is only a constructor when it also *returns the thing*. A `new` that
/// returns something else is an ordinary function with a common name. Reading it as a
/// constructor would move it somewhere it does not belong.
fn constructs(
    name: &str,
    owner: &str,
    returns: Option<&Type>,
    has_receiver: bool,
) -> Option<String> {
    if has_receiver {
        return None;
    }
    let expected = match name {
        "new" | "init" => owner.to_string(),
        other => match other.strip_prefix("New") {
            Some(rest) if !rest.is_empty() => rest.to_string(),
            _ => return None,
        },
    };
    let mut ty = returns;
    // `*Thing` and `Option<Thing>` are still ways of returning a Thing.
    while let Some(Type::Optional(inner)) = ty {
        ty = Some(inner.as_ref());
    }
    match ty {
        Some(Type::Named { name, .. }) if *name == expected => Some(expected),
        Some(Type::Named { name, .. }) if name == "Self" && !expected.is_empty() => Some(expected),
        _ => None,
    }
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
                // `#[test]` marks the next function as a test; the attribute is
                // that mark and not a construct of its own.
                "attribute_item"
                    if cx.text(child).trim() == "#[test]"
                        && child
                            .next_named_sibling()
                            .is_some_and(|n| n.kind() == "function_item") => {}
                "function_item" => {
                    let tested = child.prev_named_sibling().is_some_and(|p| {
                        p.kind() == "attribute_item" && cx.text(p).trim() == "#[test]"
                    });
                    let f = function(cx, child, None);
                    module.items.push(match tested {
                        true => Item::Test {
                            doc: f.doc,
                            name: f.name,
                            body: f.body,
                        },
                        false => Item::Function(f),
                    });
                }
                "struct_item" => module.items.push(match record(cx, child) {
                    Some(r) => Item::Record(r),
                    None => Item::Unsupported(cx.unsupported(child)),
                }),
                "enum_item" => module.items.push(match sum(cx, child) {
                    Some(s) => Item::Sum(s),
                    None => Item::Unsupported(cx.unsupported(child)),
                }),
                "const_item" | "static_item" => {
                    if let Some(c) = constant(cx, child) {
                        module.items.push(Item::Constant(c));
                    }
                }
                // Methods live in an `impl` block, apart from the type they belong to.
                // The IR keeps them with the type, so they are attached here.
                "impl_item" => {
                    // `impl<'a> Ctx<'a>` is an impl on `Ctx`. Keeping the arguments made the
                    // owner `Ctx<'a>`, which matches no record in the file. So the methods of
                    // every generic type became free functions with a `self` parameter bolted
                    // on.
                    let owner = cx
                        .field_text(child, "type")
                        .map(|text| match text.split_once('<') {
                            Some((head, _)) => head.trim().to_string(),
                            None => text,
                        })
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
                                let mut f = function(cx, item, Some(owner.clone()));
                                f.is_constructor = super::constructs(
                                    &f.name,
                                    &owner,
                                    f.returns.as_ref(),
                                    f.receiver_binding.is_some(),
                                )
                                .is_some();
                                module.items.push(Item::Function(f));
                            }
                        }
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        settle_builtins(&mut module);
        module
    }

    /// The everyday library spellings, rewritten to the table's canonical ones.
    ///
    /// `s.is_empty()`, `x.to_string()` and friends have exact counterparts in every
    /// target; written through unchanged, each was a compile error there. The
    /// canonical names are Python's, and every writer turns them back into its own.
    fn settle_builtins(module: &mut Module) {
        super::each_expr_in_module(module, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Field { of, name } = callee.as_mut() else {
                return;
            };
            let call = |callee: &str, args: Vec<Expr>| Expr::Call {
                callee: Box::new(Expr::Name(callee.to_string())),
                args,
            };
            match (name.as_str(), args.as_slice()) {
                ("len", []) => *e = call("len", vec![of.as_ref().clone()]),
                ("to_string" | "to_owned", []) => *e = call("str", vec![of.as_ref().clone()]),
                // `is_empty` is a length compared with zero, which is the one way
                // every target here can say it.
                ("is_empty", []) => {
                    *e = Expr::Binary {
                        op: BinaryOp::Eq,
                        left: Box::new(call("len", vec![of.as_ref().clone()])),
                        right: Box::new(Expr::Int("0".to_string())),
                    }
                }
                ("push", [_]) => *name = "append".to_string(),
                ("to_uppercase", []) => *name = "upper".to_string(),
                ("to_lowercase", []) => *name = "lower".to_string(),
                ("trim", []) => *name = "strip".to_string(),
                _ => {}
            }
        });
    }

    /// `format!` and the print macros, read as the interpolation they are.
    ///
    /// A macro's arguments arrive as raw tokens, not as parsed expressions. The simple
    /// calls, one literal and bare arguments, are what real code formats with, and
    /// those carry as a template. Anything richer, a `{:?}`, an expression between the
    /// commas, stays carried whole. Rebuilding an expression out of loose tokens would
    /// be guessing at precedence the parser never decided.
    fn format_macro(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        let name = cx.field_text(node, "macro")?;
        let printing = matches!(name.as_str(), "println" | "print" | "eprintln" | "eprint");
        if !printing && name != "format" {
            return None;
        }
        let tokens = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "token_tree")?;
        // The token tree's own parentheses are its first and last children; the commas
        // between them separate the arguments. A nested delimiter arrives as a nested
        // token tree, so its commas never show up at this level.
        let mut cursor = tokens.walk();
        let children: Vec<Node> = tokens.children(&mut cursor).collect();
        let inner = children.get(1..children.len().saturating_sub(1))?;
        let mut groups: Vec<Vec<Node>> = vec![Vec::new()];
        for child in inner {
            match child.kind() {
                "," => groups.push(Vec::new()),
                _ => groups
                    .last_mut()
                    .expect("one group always exists")
                    .push(*child),
            }
        }
        let (first, rest) = groups.split_first()?;
        let [literal] = first.as_slice() else {
            return None;
        };
        if literal.kind() != "string_literal" {
            return None;
        }
        let mut args = Vec::new();
        for group in rest {
            let [only] = group.as_slice() else {
                return None;
            };
            if !only.is_named() || only.kind().contains("comment") {
                return None;
            }
            let read = expr(cx, *only);
            if matches!(read, Expr::Unsupported(_)) {
                return None;
            }
            args.push(read);
        }
        let parts = template_parts(&super::unquote(&cx.text(*literal)), &args)?;
        let value = match parts.as_slice() {
            [TemplatePart::Text(text)] => Expr::Str(text.clone()),
            [] => Expr::Str(String::new()),
            _ => Expr::Template(parts),
        };
        Some(match printing {
            true => Expr::Call {
                callee: Box::new(Expr::Name("print".to_string())),
                args: vec![value],
            },
            false => value,
        })
    }

    /// A format string's pieces, with each hole filled by its argument.
    ///
    /// `{}` takes the next argument and `{name}` reads the binding. A format spec is
    /// more than interpolation. An argument count that does not match the holes means
    /// the string was not the simple kind. Either way the caller carries the macro
    /// whole.
    fn template_parts(text: &str, args: &[Expr]) -> Option<Vec<TemplatePart>> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut used = 0usize;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    current.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    current.push('}');
                }
                '{' => {
                    let mut inside = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(c) => inside.push(c),
                            None => return None,
                        }
                    }
                    let value = if inside.is_empty() {
                        let arg = args.get(used)?.clone();
                        used += 1;
                        arg
                    } else if inside.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !inside.starts_with(|c: char| c.is_ascii_digit())
                    {
                        Expr::Name(inside)
                    } else {
                        return None;
                    };
                    if !current.is_empty() {
                        parts.push(TemplatePart::Text(std::mem::take(&mut current)));
                    }
                    parts.push(TemplatePart::Expr(value));
                }
                '}' => return None,
                c => current.push(c),
            }
        }
        if used != args.len() {
            return None;
        }
        if !current.is_empty() {
            parts.push(TemplatePart::Text(current));
        }
        Some(parts)
    }

    /// A `match` whose arms are selected by literals, as a switch.
    ///
    /// A guard, a binding, a range, or any pattern with structure makes this a
    /// match in the full sense; the caller carries those whole. An arm's bare
    /// expression becomes a return when the match sits in tail position, which
    /// is Rust's implicit one.
    fn match_switch(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let subject = cx.field(node, "value")?;
        let body = cx.field(node, "body")?;
        let as_return = node.parent().is_some_and(|p| {
            p.kind() == "expression_statement"
                && p.next_named_sibling().is_none()
                && !cx.text(p).trim_end().ends_with(';')
        });
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Vec<Stmt> = Vec::new();
        for arm in cx.children(body) {
            if arm.kind() != "match_arm" {
                continue;
            }
            if cx.field(arm, "condition").is_some() {
                return None;
            }
            let pattern = cx.field(arm, "pattern")?;
            let value = cx.field(arm, "value")?;
            let arm_body = if value.kind() == "block" {
                block(cx, value)
            } else if as_return {
                vec![Stmt::Return(Some(expr(cx, value)))]
            } else {
                vec![Stmt::Expr(expr(cx, value))]
            };
            if cx.text(pattern).trim() == "_" {
                default = arm_body;
                continue;
            }
            let literals = literal_patterns(cx, pattern)?;
            arms.push((literals, arm_body));
        }
        Some(Stmt::Switch {
            subject: expr(cx, subject),
            arms,
            default,
        })
    }

    /// The literals under a match pattern: one, or several joined by `|`.
    fn literal_patterns(cx: &Cx, pattern: Node<'_>) -> Option<Vec<Expr>> {
        fn gather(cx: &Cx, node: Node<'_>, out: &mut Vec<Expr>) -> bool {
            match node.kind() {
                "or_pattern" => cx
                    .children(node)
                    .into_iter()
                    .all(|part| gather(cx, part, out)),
                "integer_literal" | "string_literal" | "raw_string_literal" | "char_literal"
                | "boolean_literal" => {
                    out.push(expr(cx, node));
                    true
                }
                _ => false,
            }
        }
        let mut out = Vec::new();
        cx.children(pattern)
            .into_iter()
            .all(|part| gather(cx, part, &mut out))
            .then_some(out)
            .filter(|literals| !literals.is_empty())
    }

    /// The binding and the tested value of `let Some(x) = e`, when the pattern is
    /// that one shape: a plain name inside `Some`.
    fn some_capture(cx: &Cx, condition: Node<'_>) -> Option<(String, Expr)> {
        let pattern = cx.field(condition, "pattern")?;
        if pattern.kind() != "tuple_struct_pattern" {
            return None;
        }
        let parts = cx.children(pattern);
        let [head, inner] = parts.as_slice() else {
            return None;
        };
        if cx.text(*head) != "Some" || inner.kind() != "identifier" {
            return None;
        }
        let value = cx.field(condition, "value")?;
        Some((plain(cx.text(*inner)), expr(cx, value)))
    }

    /// A Rust identifier without the escape that made it writable.
    ///
    /// `r#where` *is* the identifier `where`: the prefix is how Rust spells a name that
    /// collides with a keyword. It is not part of the name. Every writer here puts the escape
    /// on when it needs to. So leaving it on the way back in made the name grow an `r` each
    /// time it crossed.
    fn plain(name: String) -> String {
        match name.strip_prefix("r#") {
            Some(rest) => rest.to_string(),
            None => name,
        }
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let name = plain(cx.field_text(node, "name").unwrap_or_default());
        let mut params = Vec::new();
        let mut receiver_name = None;
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                match p.kind() {
                    // `&self` carries the receiver, which the IR records separately.
                    "self_parameter" => receiver_name = Some("self".to_string()),
                    "parameter" => params.push(Param {
                        name: plain(cx.field_text(p, "pattern").unwrap_or_default()),
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
            receiver_binding: receiver_name,
            params,
            returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
            body: cx
                .field(node, "body")
                .map(|b| function_body(cx, b))
                .unwrap_or_default(),
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
            is_async: cx.text(node).starts_with("async ") || cx.text(node).contains("async fn"),
            is_property: false,
            is_constructor: false,
        }
    }

    /// A `struct` with named fields. A tuple struct is not one.
    ///
    /// `pub struct Wrapper(Vec<T>);` has a field with no name, and a record in the IR is a
    /// *named* product. So reading one gave a record with no fields at all, and the payload
    /// type vanished without a word. There is no honest name to give it: Rust calls it `0`, and
    /// no target here can spell a field called that.
    fn record(cx: &Cx, node: Node<'_>) -> Option<Record> {
        if cx
            .children(node)
            .iter()
            .any(|c| c.kind() == "ordered_field_declaration_list")
        {
            return None;
        }
        let mut fields = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for f in cx.children(body) {
                if f.kind() != "field_declaration" {
                    continue;
                }
                fields.push(Field {
                    doc: doc_above(cx, f, &["///", "//"]),
                    name: plain(cx.field_text(f, "name").unwrap_or_default()),
                    ty: cx.field(f, "type").map(|t| ty(cx, t)),
                    default: None,
                    exported: f
                        .children(&mut f.walk())
                        .any(|c| c.kind() == "visibility_modifier"),
                });
            }
        }
        Some(Record {
            doc: doc_above(cx, node, &["///", "//"]),
            name: plain(cx.field_text(node, "name").unwrap_or_default()),
            fields,
            // Rust composes and does not inherit: a trait is a contract. It is not a base.
            extends: None,
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
            methods: Vec::new(),
        })
    }

    /// An enum, unit variants and payloads alike.
    fn sum(cx: &Cx, node: Node<'_>) -> Option<Sum> {
        let name = plain(cx.field_text(node, "name")?);
        let body = cx.field(node, "body")?;
        let mut variants = Vec::new();
        for v in cx.children(body) {
            if v.kind() != "enum_variant" {
                continue;
            }
            let variant_name = plain(cx.field_text(v, "name")?);
            let mut doc = doc_above(cx, v, &["///", "//"]);
            // An explicit discriminant has no slot in the IR: kept as words, not
            // dropped in silence.
            if let Some(value) = cx.field(v, "value") {
                doc.push(format!(
                    "the source gave this the value `{}`",
                    cx.text(value).trim()
                ));
            }
            let mut fields = Vec::new();
            if let Some(payload) = cx.field(v, "body") {
                match payload.kind() {
                    "field_declaration_list" => {
                        for f in cx.children(payload) {
                            if f.kind() != "field_declaration" {
                                continue;
                            }
                            fields.push(Field {
                                doc: doc_above(cx, f, &["///", "//"]),
                                name: plain(cx.field_text(f, "name").unwrap_or_default()),
                                ty: cx.field(f, "type").map(|t| ty(cx, t)),
                                // A variant's fields are as reachable as the enum.
                                default: None,
                                exported: true,
                            });
                        }
                    }
                    "ordered_field_declaration_list" => {
                        let types: Vec<Node<'_>> = cx
                            .children(payload)
                            .into_iter()
                            .filter(|c| {
                                !matches!(
                                    c.kind(),
                                    "visibility_modifier" | "attribute_item" | "line_comment"
                                )
                            })
                            .collect();
                        // A tuple payload has no field names, and every target here
                        // wants one. One value is *the* value; more get counted
                        // names, said out loud.
                        match types.as_slice() {
                            [only] => fields.push(Field {
                                doc: Vec::new(),
                                name: "value".to_string(),
                                ty: Some(ty(cx, *only)),
                                default: None,
                                exported: true,
                            }),
                            many => {
                                doc.push(
                                    "the source did not name the payload's fields; \
                                     f0, f1 … are this tool's."
                                        .to_string(),
                                );
                                for (index, t) in many.iter().enumerate() {
                                    fields.push(Field {
                                        doc: Vec::new(),
                                        name: format!("f{index}"),
                                        ty: Some(ty(cx, *t)),
                                        default: None,
                                        exported: true,
                                    });
                                }
                            }
                        }
                    }
                    _ => return None,
                }
            }
            variants.push(Variant {
                doc,
                name: variant_name,
                fields,
            });
        }
        Some(Sum {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            variants,
            exported: node
                .children(&mut node.walk())
                .any(|c| c.kind() == "visibility_modifier"),
        })
    }

    fn constant(cx: &Cx, node: Node<'_>) -> Option<Constant> {
        Some(Constant {
            doc: doc_above(cx, node, &["///", "//"]),
            name: plain(cx.field_text(node, "name")?),
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
        super::scalar(&text).unwrap_or_else(|| ty_text(text.trim()))
    }

    /// A Rust type from its text.
    ///
    /// The reference comes off **first**. `&HashMap<K, V>` is a `HashMap`, and checking
    /// the containers before stripping the `&` meant every map, list and option passed
    /// by reference, which in Rust is most of them, was read as a name instead.
    fn ty_text(text: &str) -> Type {
        let trimmed = text.trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }

        // A lifetime is not part of the type: `&'a str` is a `&str` that says how long it
        // lives. No other language here has anywhere to put that.
        let mut bare = trimmed.trim_start_matches('&').trim_start();
        if let Some(rest) = bare.strip_prefix('\'') {
            bare = rest
                .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
                .trim_start();
        }
        let bare = bare.trim_start_matches("mut ").trim();
        if let Some(t) = super::scalar(bare) {
            return t;
        }

        // `&[T]` is a list, and so is `[T; N]`.
        if let Some(inner) = bare.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let element = inner.split(';').next().unwrap_or(inner);
            return Type::List(Box::new(ty_text(element)));
        }

        // `(A, B)` is a tuple; `(A)` is only grouping, and `(A,)` says tuple anyway.
        if let Some(inside) = bare.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let mut parts = super::comma_parts(inside);
            let trailing = parts.last().is_some_and(String::is_empty);
            if trailing {
                parts.pop();
            }
            if (parts.len() >= 2 || (trailing && parts.len() == 1))
                && parts.iter().all(|p| !p.is_empty())
            {
                return Type::Tuple(parts.iter().map(|p| ty_text(p)).collect());
            }
        }

        // `std::collections::HashMap<K, V>` is a `HashMap`. The writer spells the path
        // in full, so a reader that only knew the bare name could not read back what
        // this tool writes.
        let Some((head, rest)) = bare.split_once('<') else {
            return named_with_args(bare, &ty_text);
        };
        let Some(arguments) = rest.strip_suffix('>') else {
            return named_with_args(bare, &ty_text);
        };
        let base = head.rsplit("::").next().unwrap_or(head).trim();
        // A lifetime is not a type argument. `Node<'_>` is a `Node`, and reading the
        // `'_` as an argument gave a type with an empty name.
        let arguments: Vec<&str> = split_arguments(arguments)
            .into_iter()
            .filter(|argument| !argument.starts_with('\''))
            .collect();
        match (base, arguments.as_slice()) {
            ("Vec" | "VecDeque", [inner]) => Type::List(Box::new(ty_text(inner))),
            ("Option", [inner]) => Type::Optional(Box::new(ty_text(inner))),
            ("HashMap" | "BTreeMap", [key, value]) => {
                Type::Map(Box::new(ty_text(key)), Box::new(ty_text(value)))
            }
            (_, []) => Type::named(base),
            _ => Type::Named {
                name: base.to_string(),
                args: arguments.into_iter().map(ty_text).collect(),
            },
        }
    }

    /// The top-level arguments of a generic, split on commas that are not inside one.
    fn split_arguments(text: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut start = 0;
        for (at, c) in text.char_indices() {
            match c {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(text[start..at].trim());
                    start = at + 1;
                }
                _ => {}
            }
        }
        out.push(text[start..].trim());
        out
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        cx.children_with_comments(node)
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    /// A function's body, where the last expression is the value it returns.
    ///
    /// `fn f(a: i64) -> i64 { a + 1 }` is the ordinary way to write a Rust function and the
    /// tail is its result. Reading it as a plain statement dropped the return in every target
    /// at once. Python got a function that returns `None`, Zig one that says `_ = a + 1;`. Go,
    /// Java and TypeScript ones that do not compile, each still declaring the return type the
    /// signature carried across.
    ///
    /// Only the body's own tail. A tail inside an `if` is a return too. Reading it as one needs
    /// the whole of Rust's block-expression rule; that is left as it was and not half-done.
    fn function_body(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        let mut body = block(cx, node);
        // The tail is an expression the grammar did not wrap in a statement. Anything
        // that already ends in `;` is an `expression_statement` and is not one.
        let tail_is_a_value = cx.children_with_comments(node).last().is_some_and(|last| {
            !last.kind().ends_with("statement") && !last.kind().contains("comment")
        });
        if tail_is_a_value {
            if let Some(Stmt::Expr(value)) = body.pop() {
                body.push(Stmt::Return(Some(value)));
            } else {
                // Not an expression after all, a trailing `if` or loop, which the
                // reader has already turned into its own statement.
                body = block(cx, node);
            }
        }
        body
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
            "let_declaration" => {
                let bound = plain(cx.field_text(node, "pattern").unwrap_or_default());
                let value = cx.field(node, "value").map(|v| expr(cx, v));
                // `let _ = f();` binds nothing. It is a call whose result is deliberately
                // dropped, which every target here can say. Reading it as a binding wrote
                // `const = f();`, a declaration with no name.
                if bound == "_" || bound.is_empty() {
                    return match value {
                        Some(value) => Stmt::Expr(value),
                        None => Stmt::Unsupported(cx.unsupported(node)),
                    };
                }
                Stmt::Let {
                    name: bound,
                    ty: cx.field(node, "type").map(|t| ty(cx, t)),
                    value,
                    mutable: cx.text(node).starts_with("let mut "),
                }
            }
            "expression_statement" => match cx.children(node).first() {
                Some(inner) => match inner.kind() {
                    "return_expression"
                    | "if_expression"
                    | "while_expression"
                    | "for_expression"
                    | "assignment_expression"
                    | "compound_assignment_expr"
                    | "match_expression" => stmt(cx, *inner),
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
            "match_expression" => match match_switch(cx, node) {
                Some(switch) => switch,
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "compound_assignment_expr" => {
                let target = cx
                    .field(node, "left")
                    .map(|l| expr(cx, l))
                    .unwrap_or(Expr::Null);
                let value = cx
                    .field(node, "right")
                    .map(|r| expr(cx, r))
                    .unwrap_or(Expr::Null);
                let operator = cx.field_text(node, "operator").unwrap_or_default();
                match super::desugar_compound(target, &operator, value) {
                    Some(assign) => assign,
                    None => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
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
                let then = cx
                    .field(node, "consequence")
                    .map(|b| block(cx, b))
                    .unwrap_or_default();
                // `if let Some(x) = e` tests an optional and binds its payload,
                // which every target can say. Any other pattern is a match in
                // disguise and carries whole.
                if let Some(condition) = cx.field(node, "condition") {
                    if condition.kind() == "let_condition" {
                        return match some_capture(cx, condition) {
                            Some((binding, value)) => Stmt::IfPresent {
                                binding,
                                value,
                                then,
                                otherwise,
                            },
                            None => Stmt::Unsupported(cx.unsupported(node)),
                        };
                    }
                }
                Stmt::If {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    then,
                    otherwise,
                }
            }
            "while_expression" => {
                let body = cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default();
                // `while let Some(x) = e` loops on an optional's payload, which
                // every target can say. Any other pattern is a match in disguise
                // and carries whole.
                if let Some(condition) = cx.field(node, "condition") {
                    if condition.kind() == "let_condition" {
                        return match some_capture(cx, condition) {
                            Some((binding, value)) => Stmt::WhilePresent {
                                binding,
                                value,
                                body,
                            },
                            None => Stmt::Unsupported(cx.unsupported(node)),
                        };
                    }
                }
                Stmt::While {
                    condition: cx
                        .field(node, "condition")
                        .map(|c| expr(cx, c))
                        .unwrap_or(Expr::Null),
                    body,
                }
            }
            "for_expression" => Stmt::ForEach {
                binding: plain(cx.field_text(node, "pattern").unwrap_or_default()),
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
                | "raw_string_literal"
                | "char_literal"
                | "boolean_literal"
                | "call_expression"
                | "binary_expression"
                | "unary_expression"
                | "field_expression"
                | "index_expression"
                | "macro_invocation"
                | "unit_expression"
        )
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            "tuple_expression" => {
                Expr::Tuple(cx.children(node).iter().map(|n| expr(cx, *n)).collect())
            }
            // `if a { b } else { c }` used as a value. Only where each branch is a block
            // holding one expression and nothing else: anything longer is a statement. There is
            // nowhere inside an argument list to put one.
            "if_expression" => {
                fn branch<'t>(cx: &Cx, b: Node<'t>) -> Option<Node<'t>> {
                    let inner = match b.kind() {
                        "block" => cx.children(b),
                        "else_clause" => {
                            let block = cx.children(b).into_iter().next()?;
                            cx.children(block)
                        }
                        _ => return None,
                    };
                    match inner.as_slice() {
                        [only] if is_expression(only.kind()) => Some(*only),
                        _ => None,
                    }
                }
                let parts = cx.children(node);
                match parts.as_slice() {
                    [condition, then, otherwise] => {
                        match (branch(cx, *then), branch(cx, *otherwise)) {
                            (Some(then), Some(otherwise)) => Expr::Ternary {
                                condition: Box::new(expr(cx, *condition)),
                                then: Box::new(expr(cx, then)),
                                otherwise: Box::new(expr(cx, otherwise)),
                            },
                            _ => Expr::Unsupported(cx.unsupported(node)),
                        }
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "await_expression" => match node.named_child(0) {
                Some(inner) => Expr::Await(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `x?`: evaluate, and on failure leave the function with the failure.
            "try_expression" => match node.named_child(0) {
                Some(inner) => Expr::Propagate(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "integer_literal" => Expr::Int(unsuffixed(&cx.text(node))),
            "float_literal" => Expr::Float(unsuffixed(&cx.text(node))),
            "boolean_literal" => Expr::Bool(cx.text(node) == "true"),
            // `r"\d+"` and `b"bytes"` are strings too. Reading only the plain form left every
            // regex in the file as "no counterpart", and a constant bound to one stopped being
            // a constant. Its name lost the convention that goes with a literal value.
            "string_literal" | "raw_string_literal" | "char_literal" => {
                Expr::Str(super::unquote(&cx.text(node)))
            }
            "identifier" | "self" => Expr::Name(plain(cx.text(node))),
            // `()` is the unit value, and the IR calls that a tuple with nothing in
            // it. Left unread, `Ok(())` carried the whole statement around it.
            "unit_expression" => Expr::Tuple(Vec::new()),
            "macro_invocation" => match format_macro(cx, node) {
                Some(read) => read,
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "field_expression" => {
                let name = plain(cx.field_text(node, "field").unwrap_or_default());
                // `self.0` reaches into a tuple struct, and a tuple struct is a Rust idea. No
                // other target here has a field with a number for a name. Writing `.0` into any
                // of them produces something that is either a syntax error or a decimal point.
                if name.chars().all(|c| c.is_ascii_digit()) {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                Expr::Field {
                    of: Box::new(
                        cx.field(node, "value")
                            .map(|v| expr(cx, v))
                            .unwrap_or(Expr::Null),
                    ),
                    name,
                }
            }
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
            // `Counter { value: 0, step }`, the one way Rust builds a record, and the
            // line every constructor is made of. Nothing read it, so every constructor
            // body in every target came out as "not translated".
            "struct_expression" => {
                let ty = cx
                    .field(node, "name")
                    .map(|n| cx.text(n))
                    .unwrap_or_default();
                // `StopReason::Conditional { … }` builds an enum variant, not a record. No
                // target here has a tagged union to build one in. Writing the path through
                // produced Go that says `StopReason::Conditional{…}`, which Go does not parse.
                if ty.contains("::") {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                let mut fields = Vec::new();
                if let Some(body) = cx.field(node, "body") {
                    for initialiser in cx.children(body) {
                        match initialiser.kind() {
                            "field_initializer" => {
                                let name = cx.field_text(initialiser, "field").unwrap_or_default();
                                let value = cx
                                    .field(initialiser, "value")
                                    .map(|v| expr(cx, v))
                                    .unwrap_or(Expr::Null);
                                fields.push((name, value));
                            }
                            // `Counter { step }` is `step: step`, the shorthand every
                            // Rust file is written in.
                            "shorthand_field_initializer" | "shorthand_field_identifier" => {
                                let name = cx.text(initialiser);
                                fields.push((name.clone(), Expr::Name(name)));
                            }
                            // `..other` carries fields this cannot name, so the record
                            // it would build is not the one the source wrote.
                            _ => return Expr::Unsupported(cx.unsupported(node)),
                        }
                    }
                }
                Expr::RecordLit { ty, fields }
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
        // A member a record cannot keep still has to reach the reader.
        let mut carried: Vec<Item> = Vec::new();
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
                "class_definition" => {
                    let record = record(cx, child, &mut carried);
                    module.items.push(Item::Record(record));
                }
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
                    // behaviour, a route, a cache, a retry, is not a record and its
                    // meaning would be lost silently.
                    let structural = decorators
                        .iter()
                        .all(|d| matches!(d.as_str(), "dataclass" | "dataclasses.dataclass"));
                    match (inner, structural) {
                        (Some(node), true) if node.kind() == "class_definition" => module
                            .items
                            .push(Item::Record(record(cx, node, &mut carried))),
                        (Some(node), true) => {
                            module.items.push(Item::Function(function(cx, node, None)))
                        }
                        _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                "expression_statement" => {
                    // A module docstring, a module-level constant, or a statement
                    // the module runs on import, which is part of the program.
                    let inner = cx.children(child);
                    match inner.first() {
                        Some(n) if n.kind() == "string" && module.items.is_empty() => {
                            module.doc.push(super::unquote(&cx.text(*n)));
                        }
                        Some(n) if matches!(n.kind(), "assignment") => {
                            if let Some(nt) = newtype(cx, *n) {
                                module.items.push(Item::Newtype(nt));
                            } else if let Some(c) = constant(cx, *n) {
                                module.items.push(Item::Constant(c));
                            } else {
                                module.items.push(Item::Unsupported(cx.unsupported(child)));
                            }
                        }
                        Some(n) if n.kind() == "call" => {
                            module.items.push(Item::Statement(Stmt::Expr(expr(cx, *n))));
                        }
                        _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                // `if __name__ == "__main__":` is how a Python file says "this part
                // is the program". The guard itself has no counterpart; what it
                // guards does, and dropping both left translated programs that ran
                // and did nothing.
                "if_statement" if main_guard(cx, child) => {
                    if let Some(body) = cx.field(child, "consequence") {
                        for statement in block(cx, body) {
                            module.items.push(Item::Statement(statement));
                        }
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        settle_unions(&mut module);
        settle_constructions(&mut module);
        module.items.extend(carried);
        module
    }

    /// Turn `Payment = Card | Cash` into a sum when the members are this file's own
    /// method-less classes.
    ///
    /// The union-of-dataclasses idiom, and the shape this tool's own Python writer
    /// emits. The alias reads as a constant whose value nothing could translate; the
    /// members read as records. Together they are one closed choice.
    fn settle_unions(module: &mut Module) {
        let locals: std::collections::BTreeMap<String, Record> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Record(r) if r.methods.is_empty() => Some((r.name.clone(), r.clone())),
                _ => None,
            })
            .collect();

        let mut consumed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for item in module.items.iter_mut() {
            let Item::Constant(c) = item else { continue };
            let Expr::Unsupported(u) = &c.value else {
                continue;
            };
            let members: Vec<&str> = u.source.split('|').map(str::trim).collect();
            let named = members.len() > 1
                && members.iter().all(|m| {
                    !m.is_empty()
                        && m.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                        && locals.contains_key(*m)
                        && !consumed.contains(*m)
                });
            if !named {
                continue;
            }
            let variants: Vec<Variant> = members
                .iter()
                .map(|name| {
                    let member = &locals[*name];
                    Variant {
                        doc: member.doc.clone(),
                        name: member.name.clone(),
                        fields: member.fields.clone(),
                    }
                })
                .collect();
            consumed.extend(members.iter().map(|m| m.to_string()));
            *item = Item::Sum(Sum {
                doc: c.doc.clone(),
                name: c.name.clone(),
                variants,
                exported: c.exported,
            });
        }
        module
            .items
            .retain(|item| !matches!(item, Item::Record(r) if consumed.contains(&r.name)));
    }

    fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
        let mut params = Vec::new();
        // Python names the receiver in the parameter list. So what it is called is the author's
        // choice, `self` by convention, `cls` on a classmethod, anything at all if they felt
        // like it.
        let mut receiver_name = None;
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
                        // Only inside a class. A module-level `def f(self, uri)` is an ordinary
                        // function whose first parameter happens to be called `self`. Stripping
                        // it there lost a parameter, which a round trip through Python
                        // did to every method of a Zig file-struct.
                        if receiver.is_some() && (name == "self" || name == "cls") {
                            receiver_name = Some(name);
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
                        if receiver.is_some() && name == "self" {
                            receiver_name = Some(name);
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
        let mut body = body_node.map(|b| block(cx, b)).unwrap_or_default();
        let mut bound: std::collections::BTreeSet<String> = params
            .iter()
            .map(|p| p.name.clone())
            .chain(receiver_name.clone())
            .collect();
        rebindings(&mut body, &mut bound);
        Function {
            doc: docstring(cx, body_node),
            name: cx.field_text(node, "name").unwrap_or_default(),
            receiver,
            receiver_binding: receiver_name,
            params,
            returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
            body,
            // Python's convention, which is all there is to go on.
            exported: !cx
                .field_text(node, "name")
                .unwrap_or_default()
                .starts_with('_'),
            is_async: cx.text(node).starts_with("async "),
            is_property: false,
            is_constructor: cx.field_text(node, "name").as_deref() == Some("__init__"),
        }
    }

    /// Turn every re-binding into an assignment.
    ///
    /// Python has no declaration keyword, so `x = 1` declares the first time and
    /// assigns every time after. Reading all of them as declarations produced
    /// `let total = total + x;` inside a Rust loop, which shadows and not
    /// accumulates, so the value outside the loop never changed. Nothing downstream
    /// can catch that: it parses, it type-checks, and it is the wrong program.
    ///
    /// One set carried through the body in order is exactly Python's rule, because its
    /// scope is the function and not the block.
    fn rebindings(body: &mut [Stmt], bound: &mut std::collections::BTreeSet<String>) {
        for stmt in body.iter_mut() {
            match stmt {
                // An annotated `x: int = 1` is a declaration whatever came before it.
                Stmt::Let {
                    name, ty, value, ..
                } if ty.is_none() && bound.contains(name) => {
                    let target = Expr::Name(name.clone());
                    let value = value.take().unwrap_or(Expr::Null);
                    *stmt = Stmt::Assign { target, value };
                }
                Stmt::Let { name, .. } => {
                    bound.insert(name.clone());
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    rebindings(then, bound);
                    rebindings(otherwise, bound);
                }
                Stmt::While { body, .. } => rebindings(body, bound),
                Stmt::ForEach { binding, body, .. } => {
                    bound.insert(binding.clone());
                    rebindings(body, bound);
                }
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    rebindings(body, bound);
                    for catch in catches.iter_mut() {
                        if let Some(name) = &catch.binding {
                            bound.insert(name.clone());
                        }
                        rebindings(&mut catch.body, bound);
                    }
                    rebindings(finally, bound);
                }
                _ => {}
            }
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

    fn record(cx: &Cx, node: Node<'_>, carried: &mut Vec<Item>) -> Record {
        let name = cx.field_text(node, "name").unwrap_or_default();
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for item in cx.children(body) {
                match item.kind() {
                    "function_definition" => {
                        methods.push(function(cx, item, Some(name.clone())));
                    }
                    // `@staticmethod def f(x)` is still a method. Reading it as
                    // something unrecognised dropped it, including the ones this
                    // tool's own Python writer emits.
                    "decorated_definition" => match decorated_method(cx, item, &name) {
                        Some(method) => methods.push(method),
                        None => carried.push(Item::Unsupported(cx.unsupported(item))),
                    },
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
                    "pass_statement" => {}
                    // A member this does not recognise is not a member that is not there. Every one
                    // of these readers had a `_ => {}` at the end of its member loop, which is how
                    // a `@staticmethod` disappeared from a class and the report still said every
                    // signature had carried across intact. A record has no room for a construct it
                    // cannot translate, so it goes beside the type.
                    _ => carried.push(Item::Unsupported(cx.unsupported(item))),
                }
            }
        }
        let mut record = Record {
            doc: docstring(cx, cx.field(node, "body")),
            name,
            fields,
            // `class A(B):`, the bases are the class's argument list. Only a single one is
            // carried: multiple inheritance has no counterpart in the two other languages that
            // inherit at all. Picking one of them would be a guess.
            extends: cx
                .field(node, "superclasses")
                .map(|list| cx.children(list))
                .filter(|bases| bases.len() == 1)
                .and_then(|bases| bases.first().map(|b| cx.text(*b))),
            exported: true,
            methods,
        };
        derive_constructor_shape(&mut record);
        record
    }

    /// What `__init__` says the instances hold.
    ///
    /// `self.name = name` declares a field as surely as an annotation does, and most
    /// classes declare most of their fields this way. Read as nothing, every record
    /// crossed as an empty struct while its methods went on reading `self.price`
    /// from a field the target never had. A field assigned from a parameter takes
    /// the parameter's type; one assigned a literal takes the literal's.
    ///
    /// A constructor that only assigns becomes the build-and-return shape the
    /// writers already turn into each target's own constructor. One that computes
    /// anything else keeps its body: rewriting it would be a guess about what the
    /// rest was for.
    fn derive_constructor_shape(record: &mut Record) {
        let name = record.name.clone();
        let Some(ctor) = record.methods.iter_mut().find(|m| m.is_constructor) else {
            return;
        };
        let receiver = ctor
            .receiver_binding
            .clone()
            .unwrap_or_else(|| "self".to_string());
        let mut assigns: Vec<(String, Expr)> = Vec::new();
        let mut only_assigns = true;
        for stmt in &ctor.body {
            match stmt {
                Stmt::Assign {
                    target: Expr::Field { of, name },
                    value,
                } if matches!(of.as_ref(), Expr::Name(n) if *n == receiver) => {
                    assigns.push((name.clone(), value.clone()));
                }
                Stmt::Comment(_) => {}
                _ => only_assigns = false,
            }
        }
        if assigns.is_empty() {
            return;
        }
        for (field, value) in &assigns {
            if record.fields.iter().any(|f| f.name == *field) {
                continue;
            }
            let ty = ctor
                .params
                .iter()
                .find(|p| matches!(value, Expr::Name(n) if n == &p.name))
                .and_then(|p| p.ty.clone())
                .or(match value {
                    Expr::Int(_) => Some(Type::Int),
                    Expr::Float(_) => Some(Type::Float),
                    Expr::Str(_) | Expr::Template(_) => Some(Type::String),
                    Expr::Bool(_) => Some(Type::Bool),
                    _ => None,
                });
            record.fields.push(Field {
                doc: Vec::new(),
                name: field.clone(),
                ty,
                default: None,
                exported: !field.starts_with('_'),
            });
        }
        if only_assigns {
            ctor.body = vec![Stmt::Return(Some(Expr::RecordLit {
                ty: name,
                fields: assigns,
            }))];
        }
    }

    /// Calls that build this module's own types are constructions.
    ///
    /// Python spells construction as a call, so `Ledger()` reached the targets as
    /// one. In Rust that names nothing, and in TypeScript a class cannot be
    /// called without `new`. The names the module itself declares are not a guess.
    fn settle_constructions(module: &mut Module) {
        let types: std::collections::BTreeSet<String> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Record(r) => Some(r.name.clone()),
                _ => None,
            })
            .collect();
        if types.is_empty() {
            return;
        }
        super::each_expr_in_module(module, &mut |e| {
            if let Expr::Call { callee, args } = e {
                if matches!(callee.as_ref(), Expr::Name(n) if types.contains(n)) {
                    let callee = callee.clone();
                    let args = std::mem::take(args);
                    *e = Expr::New { callee, args };
                }
            }
        });
    }

    /// Is this `if __name__ == "__main__":`, either quoting?
    fn main_guard(cx: &Cx, node: Node<'_>) -> bool {
        cx.field(node, "condition")
            .map(|c| cx.text(c).split_whitespace().collect::<String>())
            .is_some_and(|c| c == "__name__==\"__main__\"" || c == "__name__=='__main__'")
    }

    /// A decorated method, when the decorators only say what kind of method it is.
    ///
    /// `@staticmethod`, `@classmethod` and `@property` describe the *shape* of the binding. A
    /// decorator that changes behaviour, a route, a cache, a retry, is not a method this
    /// understands. Reading one as an ordinary method would drop the part that mattered.
    fn decorated_method(cx: &Cx, node: Node<'_>, owner: &str) -> Option<Function> {
        const SHAPE: &[&str] = &[
            "staticmethod",
            "classmethod",
            "property",
            "abstractmethod",
            "override",
            "typing.override",
            "abc.abstractmethod",
        ];
        let children = cx.children(node);
        let structural = children
            .iter()
            .filter(|n| n.kind() == "decorator")
            .all(|n| {
                let text = cx.text(*n);
                SHAPE.contains(&text.trim_start_matches('@').trim())
            });
        let is_property = children
            .iter()
            .filter(|n| n.kind() == "decorator")
            .any(|n| cx.text(*n).trim_start_matches('@').trim() == "property");
        let inner = children
            .into_iter()
            .find(|n| n.kind() == "function_definition")?;
        structural.then(|| {
            let mut method = function(cx, inner, Some(owner.to_string()));
            method.is_property = is_property;
            method
        })
    }

    fn annotated_field(cx: &Cx, node: Node<'_>) -> Option<Field> {
        let name = cx.field_text(node, "left")?;
        Some(Field {
            doc: Vec::new(),
            name: name.clone(),
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            default: None,
            exported: !name.starts_with('_'),
        })
    }

    /// `Pence = NewType("Pence", int)`, read as the distinct type it declares.
    ///
    /// Read as a constant, the call crossed into every target as a value.
    /// `NewType`, `int` and the quotes crossed with it, in five spellings, each
    /// of which parses and refers to nothing.
    fn newtype(cx: &Cx, node: Node<'_>) -> Option<Newtype> {
        let name = cx.field_text(node, "left")?;
        let call = cx.field(node, "right").filter(|r| r.kind() == "call")?;
        let callee = cx.field_text(call, "function")?;
        if callee != "NewType" && callee != "typing.NewType" {
            return None;
        }
        let args = cx.field(call, "arguments")?;
        let base = cx
            .children(args)
            .into_iter()
            .find(|a| !matches!(a.kind(), "string" | "comment"))?;
        Some(Newtype {
            doc: Vec::new(),
            name,
            base: ty(cx, base),
            exported: true,
        })
    }

    fn constant(cx: &Cx, node: Node<'_>) -> Option<Constant> {
        let name = cx.field_text(node, "left")?;
        // Python has no `const`, so a module-level binding is the only thing a constant can
        // look like. Requiring SCREAMING_SNAKE meant this tool could not read back what it
        // writes. Its own Python writer spells a constant bound to anything but a literal in
        // lower case, on the grounds that shouting the name of `schema = z.object(...)` would
        // be wrong. Every one of those was then lost on the way home. Two rules were deciding
        // one thing and disagreeing.
        if name.is_empty() {
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
        for prefix in ["tuple[", "Tuple["] {
            if let Some(inside) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(']'))
            {
                let parts = super::comma_parts(inside);
                // `tuple[X, ...]` is "any number of X", which is a list's shape and
                // not this one; the ellipsis falls through and is carried by name.
                if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p != "...") {
                    return Type::Tuple(parts.iter().map(|p| named_or_scalar(p)).collect());
                }
            }
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
        let children = cx.children_with_comments(node);
        let mut out = Vec::new();
        for (i, child) in children.iter().enumerate() {
            // The docstring is the function's doc. It is not its first statement.
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

    /// `except ValueError as e:`, the type and the binding, either of which may be
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
                    // `except E as name`, the type first, the name after `as`.
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
        // In Python, `print(e)` prints `str(e)`: the exception used as text is its
        // message. Carried bare, a target printed the object its own way. TypeScript's
        // `console.log` leads with the class name and a stack, and the words stopped
        // matching. The rewrite is scoped to this binding, inside this catch, where it
        // stands as text: a print argument or an f-string hole.
        if let Some(bound) = &binding {
            let as_text = |e: &mut Expr| {
                if matches!(e, Expr::Name(n) if n == bound) {
                    *e = Expr::Call {
                        callee: Box::new(Expr::Name("str".to_string())),
                        args: vec![std::mem::replace(e, Expr::Null)],
                    };
                }
            };
            super::each_expr_in_stmts(&mut body, &mut |e| match e {
                Expr::Call { callee, args } if matches!(callee.as_ref(), Expr::Name(n) if n == "print") =>
                {
                    args.iter_mut().for_each(as_text);
                }
                Expr::Template(parts) => {
                    for part in parts.iter_mut() {
                        if let TemplatePart::Expr(inner) = part {
                            as_text(inner);
                        }
                    }
                }
                _ => {}
            });
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
                Some(inner) if inner.kind() == "augmented_assignment" => {
                    let target = cx
                        .field(*inner, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null);
                    let value = cx
                        .field(*inner, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null);
                    let operator = cx.field_text(*inner, "operator").unwrap_or_default();
                    match super::desugar_compound(target, &operator, value) {
                        Some(assign) => assign,
                        None => Stmt::Unsupported(cx.unsupported(node)),
                    }
                }
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
            "for_statement" => {
                let body = cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default();
                // `for i, item in enumerate(xs)` is the element beside its
                // position, which every target can say.
                if let Some(counted) = enumerated(cx, node) {
                    let (index, binding, iterable) = counted;
                    return Stmt::ForEachIndexed {
                        index,
                        binding,
                        iterable,
                        body,
                    };
                }
                Stmt::ForEach {
                    binding: cx.field_text(node, "left").unwrap_or_default(),
                    iterable: cx
                        .field(node, "right")
                        .map(|v| expr(cx, v))
                        .unwrap_or(Expr::Null),
                    body,
                }
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// The pieces of `for i, item in enumerate(xs)`, when the loop is that
    /// shape. Two bare names sit on the left, `enumerate` of one expression on
    /// the right.
    fn enumerated(cx: &Cx, node: Node<'_>) -> Option<(String, String, Expr)> {
        let left = cx.field(node, "left")?;
        if left.kind() != "pattern_list" {
            return None;
        }
        let names: Vec<Node> = cx.children(left);
        let [index, binding] = names.as_slice() else {
            return None;
        };
        if index.kind() != "identifier" || binding.kind() != "identifier" {
            return None;
        }
        let right = cx.field(node, "right")?;
        if right.kind() != "call"
            || cx.field_text(right, "function").as_deref() != Some("enumerate")
        {
            return None;
        }
        let arguments = cx.field(right, "arguments")?;
        let arguments: Vec<Node> = cx.children(arguments);
        let [sequence] = arguments.as_slice() else {
            return None;
        };
        Some((cx.text(*index), cx.text(*binding), expr(cx, *sequence)))
    }

    /// Python does not distinguish declaration from assignment. Treated as a binding
    /// when it is a bare name, which a writer needs to emit `let`.
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
            // `b if a else c`, the value first, then the condition. The keywords are
            // punctuation, so the three named children are in source order and the
            // condition is the middle one.
            "conditional_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [then, condition, otherwise] => Expr::Ternary {
                        condition: Box::new(expr(cx, *condition)),
                        then: Box::new(expr(cx, *then)),
                        otherwise: Box::new(expr(cx, *otherwise)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "await" => match node.named_child(0) {
                Some(inner) => Expr::Await(Box::new(expr(cx, inner))),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "integer" => Expr::Int(cx.text(node)),
            "float" => Expr::Float(cx.text(node)),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "none" => Expr::Null,
            // `(a, b)` and the bare `a, b` of `return a, b` are the same tuple.
            "tuple" | "expression_list" => {
                Expr::Tuple(cx.children(node).iter().map(|n| expr(cx, *n)).collect())
            }
            "string" => {
                // An f-string interpolates. Dropping the braces would turn
                // `f"{c} below the floor"` into the literal text `{c} below the
                // floor`, not a gap but a wrong answer, so it is carried instead.
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
                // says, a wrong answer instead of a missing one.
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
            "keyword_argument" => Expr::Keyword {
                name: cx.field_text(node, "name").unwrap_or_default(),
                value: Box::new(
                    cx.field(node, "value")
                        .map(|v| expr(cx, v))
                        .unwrap_or(Expr::Null),
                ),
            },
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
                    let mut f = function(cx, child, None, None);
                    // `func TestX(t *testing.T)` is the language's own test
                    // convention. The parameter is the runner's handle, not data,
                    // and a leading `_ = t` only exists to quiet the compiler, so
                    // neither crosses.
                    let handle_ty = |ty: &Option<Type>| match ty {
                        Some(Type::Named { name, .. }) => name.contains("testing.T"),
                        // `*testing.T` reads as an optional, the way every
                        // pointer here does.
                        Some(Type::Optional(inner)) => {
                            matches!(inner.as_ref(), Type::Named { name, .. } if name.contains("testing.T"))
                        }
                        _ => false,
                    };
                    let runner_handle = f.params.len() == 1
                        && f.params[0].name == "t"
                        && handle_ty(&f.params[0].ty);
                    if runner_handle && f.name.starts_with("Test") && f.name.len() > 4 {
                        let mut body = f.body;
                        if matches!(
                            body.first(),
                            Some(Stmt::Assign { target: Expr::Name(t), value: Expr::Name(v) })
                                if t == "_" && v == "t"
                        ) {
                            body.remove(0);
                        }
                        module.items.push(Item::Test {
                            doc: f.doc,
                            name: f.name.trim_start_matches("Test").to_string(),
                            body,
                        });
                        continue;
                    }
                    // Go's constructor is a naming habit: `NewThing` that returns one. Naming
                    // the type it makes is what puts it back with that type, a top-level
                    // function belongs to nothing. `NewEdit` written as Rust would have come
                    // out `new_edit` beside the `impl`.
                    if let Some(owner) = super::constructs(&f.name, "", f.returns.as_ref(), false) {
                        f.is_constructor = true;
                        f.receiver = Some(owner);
                    }
                    module.items.push(Item::Function(f));
                }
                "method_declaration" => {
                    let owner = cx
                        .field(child, "receiver")
                        .and_then(|r| cx.children(r).first().copied())
                        .and_then(|p| cx.field(p, "type"))
                        .map(|t| cx.text(t).trim_start_matches('*').to_string())
                        .unwrap_or_default();
                    // Go lets the author name the receiver, and most do: `func (c
                    // *Collector) Add` binds `c`, not `self`.
                    let bound = cx
                        .field(child, "receiver")
                        .and_then(|r| cx.children(r).first().copied())
                        .and_then(|p| cx.children(p).first().copied())
                        .filter(|n| n.kind() == "identifier")
                        .map(|n| cx.text(n));
                    pending.push((owner.clone(), function(cx, child, Some(owner), bound)));
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
        settle_sums(&mut module);
        settle_builtins(&mut module);
        module
    }

    /// The everyday library spellings, rewritten to the table's canonical ones.
    ///
    /// `fmt.Println` and the `strings` helpers have exact counterparts everywhere;
    /// written through unchanged, each was a compile error in every target. The
    /// package-qualified call becomes the canonical method form, and the writers
    /// turn it back into whatever their language says.
    fn settle_builtins(module: &mut Module) {
        super::each_expr_in_module(module, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Field { of, name } = callee.as_ref() else {
                return;
            };
            let Expr::Name(package) = of.as_ref() else {
                return;
            };
            let method = |target: &Expr, name: &str| Expr::Field {
                of: Box::new(target.clone()),
                name: name.to_string(),
            };
            let replacement = match (package.as_str(), name.as_str(), args.as_slice()) {
                ("fmt", "Println", _) => Expr::Call {
                    callee: Box::new(Expr::Name("print".to_string())),
                    args: std::mem::take(args),
                },
                ("strings", "ToUpper", [x]) => Expr::Call {
                    callee: Box::new(method(x, "upper")),
                    args: Vec::new(),
                },
                ("strings", "ToLower", [x]) => Expr::Call {
                    callee: Box::new(method(x, "lower")),
                    args: Vec::new(),
                },
                ("strings", "TrimSpace", [x]) => Expr::Call {
                    callee: Box::new(method(x, "strip")),
                    args: Vec::new(),
                },
                ("strings", "Join", [xs, sep]) => Expr::Call {
                    callee: Box::new(method(sep, "join")),
                    args: vec![xs.clone()],
                },
                ("strconv", "Itoa", _) => Expr::Call {
                    callee: Box::new(Expr::Name("str".to_string())),
                    args: std::mem::take(args),
                },
                _ => return,
            };
            *e = replacement;
        });
    }

    /// Turn the marker-interface convention back into the sum it spells.
    ///
    /// Go has no closed choice. `type Shape interface{ isShape() }` with the
    /// method on each member is how one is written, by hand and by this tool's own
    /// Go writer. Read literally, the interface is unsupported and every member
    /// gains a phantom `isShape` method that no other language wants.
    fn settle_sums(module: &mut Module) {
        let markers: Vec<(usize, String)> = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(at, item)| match item {
                Item::Unsupported(u) => marker_interface(&u.source).map(|name| (at, name)),
                _ => None,
            })
            .collect();

        let mut consumed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for (at, name) in markers {
            let marker = format!("is{name}");
            let members: Vec<Record> = module
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Record(r)
                        if r.methods.iter().any(|m| {
                            m.name == marker && m.params.is_empty() && m.returns.is_none()
                        }) =>
                    {
                        Some(r.clone())
                    }
                    _ => None,
                })
                .collect();
            if members.is_empty() {
                continue;
            }
            // A member with anything beyond the marker is more than a variant;
            // converting it would drop its other methods on the floor.
            if members
                .iter()
                .any(|r| r.methods.iter().any(|m| m.name != marker))
            {
                continue;
            }
            let variants: Vec<Variant> = members
                .iter()
                .map(|member| Variant {
                    doc: member.doc.clone(),
                    name: member.name.clone(),
                    fields: member.fields.clone(),
                })
                .collect();
            let exported = members.iter().any(|m| m.exported);
            module.items[at] = Item::Sum(Sum {
                doc: Vec::new(),
                name,
                variants,
                exported,
            });
            consumed.extend(members.into_iter().map(|m| m.name));
        }
        module
            .items
            .retain(|item| !matches!(item, Item::Record(r) if consumed.contains(&r.name)));
    }

    /// The name in `type X interface{ isX() }`, if the text has that shape and no more.
    fn marker_interface(source: &str) -> Option<String> {
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        let at = compact.find("interface{")?;
        let name = compact[..at].trim_start_matches("type").to_string();
        let body = compact[at + "interface{".len()..].strip_suffix('}')?;
        (!name.is_empty() && body == format!("is{name}()")).then_some(name)
    }

    fn function(
        cx: &Cx,
        node: Node<'_>,
        receiver: Option<String>,
        receiver_name: Option<String>,
    ) -> Function {
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
            receiver_binding: receiver_name,
            params,
            returns: cx.field(node, "result").map(|t| ty(cx, t)),
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            is_async: false,
            is_property: false,
            is_constructor: false,
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
                        default: None,
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
            // Go embeds and does not inherit.
            extends: None,
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
        // `(int, error)`: Go writes several results as a parenthesised list, and the
        // grammar hands it over as the same `parameter_list` a signature uses. Read as
        // text it became an unwritable name in every signature. Read as the tuple
        // it is, every target can spell it or say it cannot.
        if node.kind() == "parameter_list" {
            let parts: Vec<Type> = cx
                .children(node)
                .iter()
                .filter(|c| c.kind() == "parameter_declaration")
                .filter_map(|c| cx.field(*c, "type").map(|t| ty(cx, t)))
                .collect();
            match parts.as_slice() {
                [only] => return only.clone(),
                [] => {}
                _ => return Type::Tuple(parts),
            }
        }
        ty_text(cx.text(node).trim())
    }

    /// A Go type from its text.
    ///
    /// The entry point and the recursion are the same function. When they were not, the value
    /// of a `map[string][]SymbolId` resolved one layer and lost the slice. The outer map was
    /// read here and the inner type by a helper that only knew scalars.
    fn ty_text(text: &str) -> Type {
        let trimmed = text.trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }
        if let Some(inner) = trimmed.strip_prefix("[]") {
            return Type::List(Box::new(ty_text(inner)));
        }
        if let Some(inner) = trimmed.strip_prefix("map[") {
            if let Some((key, value)) = inner.split_once(']') {
                return Type::Map(Box::new(ty_text(key)), Box::new(ty_text(value)));
            }
        }
        // A pointer is Go's way of saying a value may be absent.
        if let Some(inner) = trimmed.strip_prefix('*') {
            return Type::Optional(Box::new(ty_text(inner)));
        }
        named_with_args(trimmed, &ty_text)
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        // tree-sitter-go puts a `statement_list` between a block and its statements,
        // so a block's only child is that wrapper. Reading the children directly gave
        // one unknown node and carried *every Go function body ever translated* into
        // the output as a single comment, invisible to the round-trip tests, because
        // a body that is entirely a comment still parses.
        let children = cx.children_with_comments(node);
        let statements = match children.as_slice() {
            [only] if only.kind() == "statement_list" => cx.children_with_comments(*only),
            _ => children,
        };
        statements
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    /// The one expression inside a `left`/`right` list, or the list carried
    /// whole: `a, b := f()` binds a pair, which the IR cannot say.
    fn unlisted(cx: &Cx, node: Node<'_>) -> Expr {
        if node.kind() == "expression_list" {
            let items = cx.children(node);
            return match items.as_slice() {
                [only] => expr(cx, *only),
                _ => Expr::Unsupported(cx.unsupported(node)),
            };
        }
        expr(cx, node)
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
            // `return x` wraps its value in an `expression_list`, the same shape that hid every
            // function body. `return a, b` is Go's multiple return, and it crosses as a tuple.
            // Mapping it to nothing, as this arm once did, turned a two-value return into a
            // bare `return` with nothing said.
            "return_statement" => Stmt::Return(cx.children(node).first().map(|e| {
                match (e.kind(), cx.children(*e).as_slice()) {
                    ("expression_list", [only]) => expr(cx, *only),
                    ("expression_list", several) => {
                        Expr::Tuple(several.iter().map(|n| expr(cx, *n)).collect())
                    }
                    _ => expr(cx, *e),
                }
            })),
            "break_statement" => Stmt::Break,
            "continue_statement" => Stmt::Continue,
            // Both sides of `:=` and `=` arrive wrapped in an `expression_list`,
            // even when they hold one expression. Passing the wrapper to `expr`
            // carried every such statement whole.
            "short_var_declaration" => Stmt::Let {
                name: cx.field_text(node, "left").unwrap_or_default(),
                ty: None,
                value: cx.field(node, "right").map(|v| unlisted(cx, v)),
                mutable: true,
            },
            "assignment_statement" => {
                let target = cx
                    .field(node, "left")
                    .map(|l| unlisted(cx, l))
                    .unwrap_or(Expr::Null);
                let value = cx
                    .field(node, "right")
                    .map(|r| unlisted(cx, r))
                    .unwrap_or(Expr::Null);
                // One node covers `=` and `+=` alike, and reading them alike
                // turned `total += item` into `total = item`.
                let operator = {
                    let mut cursor = node.walk();
                    let found = node
                        .children(&mut cursor)
                        .find(|c| !c.is_named())
                        .map(|c| cx.text(c));
                    found.unwrap_or_default()
                };
                if operator == "=" {
                    Stmt::Assign { target, value }
                } else {
                    match super::desugar_compound(target, &operator, value) {
                        Some(assign) => assign,
                        None => Stmt::Unsupported(cx.unsupported(node)),
                    }
                }
            }
            "expression_statement" => cx
                .children(node)
                .first()
                .map(|inner| Stmt::Expr(expr(cx, *inner)))
                .unwrap_or_else(|| Stmt::Unsupported(cx.unsupported(node))),
            "defer_statement" => match cx.children(node).first() {
                Some(deferred) => Stmt::Defer(vec![Stmt::Expr(expr(cx, *deferred))]),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
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

/// Java.
///
/// The shape that makes Java different from every other language here: it has **no top level
/// below the type**. A file is a class, and every function is a method of it. So reading a Java
/// file means unwrapping one class to get at the module inside, and writing one means wrapping
/// the module back up.
///
/// A `static final` field is Java's only way to write a module constant. So it reads as one; an
/// instance field reads as a field of the record.
mod java {
    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        // A member a record cannot keep still has to reach the reader.
        let mut carried: Vec<Item> = Vec::new();
        for child in cx.children(root) {
            match child.kind() {
                // The package clause names the compilation unit; it is not an import
                // and there is nothing in another language for it to become.
                "comment" | "line_comment" | "block_comment" | "package_declaration" => {}
                "import_declaration" => module.items.push(Item::Import {
                    text: cx.text(child),
                    line: cx.line(child),
                }),
                "class_declaration" | "interface_declaration" | "record_declaration" => {
                    let before = carried.len();
                    let (record, constants) = type_declaration(cx, child, &mut carried);
                    module
                        .items
                        .extend(constants.into_iter().map(Item::Constant));
                    let hoisted = carried.len() > before;
                    // A class whose every member left as a hoisted sibling was only
                    // ever their namespace. An empty `class Orders: pass` beside the
                    // things it held says less than nothing.
                    let shell = |r: &Record| {
                        hoisted
                            && r.fields.is_empty()
                            && r.methods.is_empty()
                            && r.extends.is_none()
                    };
                    match record {
                        Some(record) if !shell(&record) => module.items.push(Item::Record(record)),
                        _ => {}
                    }
                }
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        module.items.extend(carried);
        settle_accessors(&mut module);
        settle_builtins(&mut module);
        module
    }

    /// The everyday library spellings, rewritten to the table's canonical ones.
    ///
    /// `System.out.println` and the `String` statics have exact counterparts in
    /// every target; written through unchanged, each was a compile error there.
    fn settle_builtins(module: &mut Module) {
        super::each_expr_in_module(module, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            if let Expr::Field { of, name } = callee.as_mut() {
                {
                    let is_system_out = matches!(
                        of.as_ref(),
                        Expr::Field { of: inner, name: out_name }
                            if out_name == "out" && matches!(inner.as_ref(), Expr::Name(s) if s == "System")
                    );
                    match (is_system_out, name.as_str()) {
                        (true, "println") => {
                            *e = Expr::Call {
                                callee: Box::new(Expr::Name("print".to_string())),
                                args: std::mem::take(args),
                            };
                        }
                        (false, "valueOf") if matches!(of.as_ref(), Expr::Name(s) if s == "String") =>
                        {
                            *e = Expr::Call {
                                callee: Box::new(Expr::Name("str".to_string())),
                                args: std::mem::take(args),
                            };
                        }
                        (false, "join")
                            if matches!(of.as_ref(), Expr::Name(s) if s == "String")
                                && args.len() == 2 =>
                        {
                            let xs = args.pop().expect("two arguments");
                            let sep = args.pop().expect("two arguments");
                            *e = Expr::Call {
                                callee: Box::new(Expr::Field {
                                    of: Box::new(sep),
                                    name: "join".to_string(),
                                }),
                                args: vec![xs],
                            };
                        }
                        (false, "toUpperCase") if args.is_empty() => *name = "upper".to_string(),
                        (false, "toLowerCase") if args.is_empty() => *name = "lower".to_string(),
                        _ => {}
                    }
                }
            }
        });
    }

    /// A record's accessor calls become the field reads they are.
    ///
    /// `record Order(boolean paid)` gives its callers `o.paid()`. The record
    /// crosses as fields, so the call form reaches a target where `paid` is data
    /// and calling it fails. Only a name that is a field of this module's records
    /// and a method of nothing rewrites; anything shared stays a call.
    fn settle_accessors(module: &mut Module) {
        let mut fields: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut methods: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for item in &module.items {
            match item {
                Item::Record(r) => {
                    fields.extend(r.fields.iter().map(|f| f.name.clone()));
                    methods.extend(r.methods.iter().map(|m| m.name.clone()));
                }
                Item::Function(f) => {
                    methods.insert(f.name.clone());
                }
                _ => {}
            }
        }
        let accessors: std::collections::BTreeSet<String> =
            fields.difference(&methods).cloned().collect();
        if accessors.is_empty() {
            return;
        }
        super::each_expr_in_module(module, &mut |e| {
            if let Expr::Call { callee, args } = e {
                if args.is_empty() {
                    if let Expr::Field { name, .. } = callee.as_ref() {
                        if accessors.contains(name) {
                            *e = (**callee).clone();
                        }
                    }
                }
            }
        });
    }

    /// A class, interface or record, plus the module constants hidden inside it.
    fn type_declaration(
        cx: &Cx,
        node: Node<'_>,
        carried: &mut Vec<Item>,
    ) -> (Option<Record>, Vec<Constant>) {
        let Some(name) = cx.field_text(node, "name") else {
            return (None, Vec::new());
        };
        let mut record = Record {
            doc: doc_above(cx, node, &["///", "//", "/**", "*"]),
            name,
            fields: Vec::new(),
            extends: cx
                .field_text(node, "superclass")
                .map(|text| text.trim_start_matches("extends").trim().to_string()),
            exported: cx.text(node).starts_with("public") || is_public(cx, node),
            methods: Vec::new(),
        };
        let mut constants = Vec::new();

        let Some(body) = cx.field(node, "body") else {
            return (Some(record), constants);
        };

        // A record's parameters are its fields: `record Reading(String sensor, double value)`.
        if let Some(parameters) = cx.field(node, "parameters") {
            for parameter in cx.children(parameters) {
                if let (Some(name), Some(ty)) = (
                    cx.field_text(parameter, "name"),
                    cx.field(parameter, "type"),
                ) {
                    record.fields.push(Field {
                        doc: Vec::new(),
                        name,
                        ty: Some(ty_of(cx, ty)),
                        default: None,
                        exported: true,
                    });
                }
            }
        }

        for member in cx.children(body) {
            match member.kind() {
                "field_declaration" => {
                    let modifiers = modifier_text(cx, member);
                    let public = modifiers.contains("public");
                    for declarator in cx.children(member) {
                        if declarator.kind() != "variable_declarator" {
                            continue;
                        }
                        let Some(name) = cx.field_text(declarator, "name") else {
                            continue;
                        };
                        let ty = cx.field(member, "type").map(|t| ty_of(cx, t));
                        // `static final` is Java's only spelling for a module constant,
                        // and every other language here has a real one.
                        if modifiers.contains("static") && modifiers.contains("final") {
                            if let Some(value) = cx.field(declarator, "value") {
                                constants.push(Constant {
                                    doc: doc_above(cx, member, &["///", "//", "/**", "*"]),
                                    name,
                                    ty,
                                    value: expr(cx, value),
                                    exported: public,
                                });
                                continue;
                            }
                        }
                        record.fields.push(Field {
                            doc: doc_above(cx, member, &["///", "//", "/**", "*"]),
                            name,
                            ty,
                            default: None,
                            exported: public,
                        });
                    }
                }
                // A constructor is a method that makes the type and not acting on one, and
                // every target spells it its own way. So what carries is that it *is* one, not
                // what it is called.
                //
                // A static method never touches the instance, so the class is only its
                // namespace. It crosses as a module function: written as a method, every
                // target gave it a receiver its Java call sites never pass.
                "method_declaration" | "constructor_declaration" => {
                    let is_static = member.kind() == "method_declaration"
                        && modifier_text(cx, member).contains("static");
                    match is_static {
                        true => carried.push(Item::Function(function(cx, member))),
                        false => record.methods.push(function(cx, member)),
                    }
                }
                // A type declared inside another is still a type; Java nests them for
                // namespacing and the record it declares crosses as a sibling. Dropped,
                // `record Order(...)` left `main` constructing a name nothing defined,
                // while the fidelity header still counted the record as carried.
                "class_declaration" | "interface_declaration" | "record_declaration" => {
                    let (inner, inner_constants) = type_declaration(cx, member, carried);
                    carried.extend(inner_constants.into_iter().map(Item::Constant));
                    if let Some(inner) = inner {
                        carried.push(Item::Record(inner));
                    }
                }
                "comment" | "{" | "}" => {}
                // A member this does not recognise is not a member that is not
                // there. Every reader here ended its member loop with `_ => {}`,
                // which is how a `@staticmethod` disappeared from a class while the
                // report said every signature had carried across intact.
                _ => carried.push(Item::Unsupported(cx.unsupported(member))),
            }
        }
        (Some(record), constants)
    }

    fn is_public(cx: &Cx, node: Node<'_>) -> bool {
        modifier_text(cx, node).contains("public")
    }

    /// The `modifiers` node's text, which is where Java keeps `public`, `static` and
    /// `final`, and its annotations.
    fn modifier_text(cx: &Cx, node: Node<'_>) -> String {
        cx.children(node)
            .into_iter()
            .find(|c| c.kind() == "modifiers")
            .map(|m| cx.text(m))
            .unwrap_or_default()
    }

    fn function(cx: &Cx, node: Node<'_>) -> Function {
        let mut params = Vec::new();
        if let Some(list) = cx.field(node, "parameters") {
            for parameter in cx.children(list) {
                match parameter.kind() {
                    "formal_parameter" => {
                        if let Some(name) = cx.field_text(parameter, "name") {
                            params.push(Param {
                                name,
                                ty: cx.field(parameter, "type").map(|t| ty_of(cx, t)),
                                default: None,
                                kind: ParamKind::Normal,
                            });
                        }
                    }
                    // `String... args` is a variadic, which most of the other targets
                    // have a spelling for.
                    "spread_parameter" => {
                        if let Some(declarator) = cx
                            .children(parameter)
                            .into_iter()
                            .find(|c| c.kind() == "variable_declarator")
                        {
                            if let Some(name) = cx.field_text(declarator, "name") {
                                params.push(Param {
                                    name,
                                    ty: cx.field(parameter, "type").map(|t| ty_of(cx, t)),
                                    default: None,
                                    kind: ParamKind::VarArgs,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let returns = cx.field(node, "type").map(|t| ty_of(cx, t));
        Function {
            doc: doc_above(cx, node, &["///", "//", "/**", "*"]),
            name: cx.field_text(node, "name").unwrap_or_default(),
            receiver: None,
            // Java names the receiver for you and the word is a keyword, so it can
            // never be anything else.
            receiver_binding: Some("this".to_string()),
            params,
            returns,
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            exported: is_public(cx, node),
            is_async: false,
            is_property: false,
            is_constructor: node.kind() == "constructor_declaration",
        }
    }

    fn ty_of(cx: &Cx, node: Node<'_>) -> Type {
        match node.kind() {
            "void_type" => Type::Unit,
            "boolean_type" => Type::Bool,
            "integral_type" => Type::Int,
            "floating_point_type" => Type::Float,
            "array_type" => cx
                .field(node, "element")
                .map(|e| Type::List(Box::new(ty_of(cx, e))))
                .unwrap_or_else(|| Type::named("Object")),
            "generic_type" => generic(cx, node),
            _ => match cx.text(node).as_str() {
                "String" | "CharSequence" => Type::String,
                "Integer" | "Long" | "Short" | "Byte" => Type::Int,
                "Double" | "Float" => Type::Float,
                "Boolean" => Type::Bool,
                "Object" => Type::named("Object"),
                other => Type::named(other),
            },
        }
    }

    /// `List<String>`, `Map<String, Integer>`, the two containers that correspond.
    fn generic(cx: &Cx, node: Node<'_>) -> Type {
        let base = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "type_identifier")
            .map(|c| cx.text(c))
            .unwrap_or_default();
        let arguments: Vec<Type> = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "type_arguments")
            .map(|a| cx.children(a).into_iter().map(|t| ty_of(cx, t)).collect())
            .unwrap_or_default();
        match (base.as_str(), arguments.as_slice()) {
            ("List" | "ArrayList" | "Collection" | "Iterable" | "Set", [inner]) => {
                Type::List(Box::new(inner.clone()))
            }
            ("Map" | "HashMap", [key, value]) => {
                Type::Map(Box::new(key.clone()), Box::new(value.clone()))
            }
            ("Optional", [inner]) => Type::Optional(Box::new(inner.clone())),
            _ => Type::Named {
                name: base,
                args: arguments,
            },
        }
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        cx.children_with_comments(node)
            .iter()
            .map(|n| keep_whole(cx, *n, stmt(cx, *n)))
            .collect()
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            "comment" | "line_comment" | "block_comment" => {
                Stmt::Comment(super::uncomment(&cx.text(node)))
            }
            "return_statement" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "throw_statement" => match cx.children(node).first() {
                Some(value) => Stmt::Throw(thrown(expr(cx, *value))),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "break_statement" => Stmt::Break,
            "continue_statement" => Stmt::Continue,
            "local_variable_declaration" => {
                let declarators: Vec<Node> = cx
                    .children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "variable_declarator")
                    .collect();
                // `int a = 1, b = 2;` is two bindings in one statement and the IR has a place
                // for one. Carrying it whole keeps the source in front of the reader and not
                // silently dropping the second.
                match declarators.as_slice() {
                    [only] => Stmt::Let {
                        name: cx.field_text(*only, "name").unwrap_or_default(),
                        ty: cx.field(node, "type").map(|t| ty_of(cx, t)),
                        value: cx.field(*only, "value").map(|v| expr(cx, v)),
                        mutable: !cx.text(node).trim_start().starts_with("final"),
                    },
                    _ => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
            "expression_statement" => match cx.children(node).first().copied() {
                Some(inner) if inner.kind() == "assignment_expression" => {
                    let target = cx
                        .field(inner, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null);
                    let value = cx
                        .field(inner, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null);
                    // One node covers `=` and `+=` alike, and reading them alike
                    // turned `total += item` into `total = item`.
                    let operator = cx.field_text(inner, "operator").unwrap_or_default();
                    if operator == "=" {
                        Stmt::Assign { target, value }
                    } else {
                        match super::desugar_compound(target, &operator, value) {
                            Some(assign) => assign,
                            None => Stmt::Unsupported(cx.unsupported(node)),
                        }
                    }
                }
                Some(inner) => Stmt::Expr(expr(cx, inner)),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "if_statement" => Stmt::If {
                condition: cx
                    .field(node, "condition")
                    .map(|c| condition(cx, c))
                    .unwrap_or(Expr::Null),
                then: cx
                    .field(node, "consequence")
                    .map(|b| branch(cx, b))
                    .unwrap_or_default(),
                otherwise: cx
                    .field(node, "alternative")
                    .map(|b| branch(cx, b))
                    .unwrap_or_default(),
            },
            "while_statement" => Stmt::While {
                condition: cx
                    .field(node, "condition")
                    .map(|c| condition(cx, c))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| branch(cx, b))
                    .unwrap_or_default(),
            },
            // `for (X x : xs)` is the loop every language here has. A C-style `for` is
            // not, and is carried.
            "enhanced_for_statement" => Stmt::ForEach {
                binding: cx.field_text(node, "name").unwrap_or_default(),
                iterable: cx
                    .field(node, "value")
                    .map(|v| expr(cx, v))
                    .unwrap_or(Expr::Null),
                body: cx
                    .field(node, "body")
                    .map(|b| branch(cx, b))
                    .unwrap_or_default(),
            },
            "try_statement" | "try_with_resources_statement" => {
                let mut catches = Vec::new();
                let mut finally = Vec::new();
                for child in cx.children(node) {
                    match child.kind() {
                        "catch_clause" => {
                            // `catch (IllegalStateException error)` holds a `catch_type` and an
                            // identifier as plain children and not as named fields. So asking
                            // for fields lost both the exception type and the name the body
                            // uses.
                            let parameter = cx
                                .children(child)
                                .into_iter()
                                .find(|c| c.kind() == "catch_formal_parameter");
                            let parts: Vec<Node> =
                                parameter.map(|p| cx.children(p)).unwrap_or_default();
                            let binding = parts
                                .iter()
                                .find(|c| c.kind() == "identifier")
                                .map(|c| cx.text(*c));
                            // The clause's type crosses under its canonical name too,
                            // or `except IllegalArgumentException` selected a class
                            // the target never declared while the raises said
                            // `ValueError`.
                            let selector = parts.iter().find(|c| c.kind() == "catch_type").map(
                                |t| match ty_of(cx, *t) {
                                    Type::Named { name, args }
                                        if args.is_empty() && exception_name(&name).is_some() =>
                                    {
                                        Type::named(exception_name(&name).expect("checked"))
                                    }
                                    other => other,
                                },
                            );
                            let mut body = cx
                                .field(child, "body")
                                .map(|b| block(cx, b))
                                .unwrap_or_default();
                            // `e.getMessage()` inside the catch is the exception as
                            // text, and the canonical spelling of that is `str(e)`.
                            if let Some(bound) = &binding {
                                super::each_expr_in_stmts(&mut body, &mut |e| {
                                    let Expr::Call { callee, args } = e else {
                                        return;
                                    };
                                    if !args.is_empty() {
                                        return;
                                    }
                                    let Expr::Field { of, name } = callee.as_ref() else {
                                        return;
                                    };
                                    let ours = name == "getMessage"
                                        && matches!(of.as_ref(), Expr::Name(n) if n == bound);
                                    if ours {
                                        *e = Expr::Call {
                                            callee: Box::new(Expr::Name("str".to_string())),
                                            args: vec![Expr::Name(bound.clone())],
                                        };
                                    }
                                });
                            }
                            catches.push(Catch {
                                binding,
                                ty: selector,
                                body,
                            });
                        }
                        "finally_clause" => {
                            finally = cx
                                .children(child)
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
                        .field(node, "body")
                        .map(|b| block(cx, b))
                        .unwrap_or_default(),
                    catches,
                    finally,
                    source: cx.text(node),
                    line: cx.line(node),
                }
            }
            "block" => match block(cx, node).as_slice() {
                [] => Stmt::Expr(Expr::Null),
                _ => Stmt::Unsupported(cx.unsupported(node)),
            },
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// Java's everyday exception names, spelled as the canonical (Python) ones.
    ///
    /// `IllegalArgumentException` is the complaint `ValueError` makes, and the general
    /// `Exception` and `RuntimeException` are the general `Exception`. Written through
    /// unchanged, every `raise` in a translated file named a class the target never
    /// declared. A name outside the table is the program's own and is not touched.
    fn exception_name(name: &str) -> Option<&'static str> {
        Some(match name {
            "IllegalArgumentException" => "ValueError",
            "Exception" | "RuntimeException" => "Exception",
            _ => return None,
        })
    }

    /// A thrown `new SomeException(...)`, with the class name crossed to canonical.
    fn thrown(value: Expr) -> Expr {
        let Expr::New { callee, args } = value else {
            return value;
        };
        let mapped = match callee.as_ref() {
            Expr::Name(name) => exception_name(name),
            _ => None,
        };
        match mapped {
            Some(name) => Expr::New {
                callee: Box::new(Expr::Name(name.to_string())),
                args,
            },
            None => Expr::New { callee, args },
        }
    }

    /// A branch is a block or a single statement, `if (x) return;` has no braces.
    fn branch(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        if node.kind() == "block" {
            return block(cx, node);
        }
        vec![keep_whole(cx, node, stmt(cx, node))]
    }

    /// Java names an `if`'s condition as the parenthesised expression, brackets included.
    fn condition(cx: &Cx, node: Node<'_>) -> Expr {
        if node.kind() == "parenthesized_expression" {
            return cx
                .children(node)
                .first()
                .map(|inner| expr(cx, *inner))
                .unwrap_or(Expr::Null);
        }
        expr(cx, node)
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            // `a ? b : c`, the operands are the named children and the `?` and `:`
            // between them are punctuation.
            "ternary_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [condition, then, otherwise] => Expr::Ternary {
                        condition: Box::new(expr(cx, *condition)),
                        then: Box::new(expr(cx, *then)),
                        otherwise: Box::new(expr(cx, *otherwise)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal" => {
                Expr::Int(cx.text(node).trim_end_matches(['L', 'l']).to_string())
            }
            "decimal_floating_point_literal" => Expr::Float(
                cx.text(node)
                    .trim_end_matches(['f', 'F', 'd', 'D'])
                    .to_string(),
            ),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "null_literal" => Expr::Null,
            "string_literal" => Expr::Str(super::unquote(&cx.text(node))),
            "character_literal" => Expr::Str(super::unquote(&cx.text(node))),
            "identifier" | "this" => Expr::Name(cx.text(node)),
            "parenthesized_expression" => cx
                .children(node)
                .first()
                .map(|inner| expr(cx, *inner))
                .unwrap_or(Expr::Null),
            "field_access" => Expr::Field {
                of: Box::new(
                    cx.field(node, "object")
                        .map(|o| expr(cx, o))
                        .unwrap_or(Expr::Null),
                ),
                name: cx.field_text(node, "field").unwrap_or_default(),
            },
            "array_access" => Expr::Index {
                of: Box::new(
                    cx.field(node, "array")
                        .map(|a| expr(cx, a))
                        .unwrap_or(Expr::Null),
                ),
                index: Box::new(
                    cx.field(node, "index")
                        .map(|i| expr(cx, i))
                        .unwrap_or(Expr::Null),
                ),
            },
            "method_invocation" => {
                let name = cx.field_text(node, "name").unwrap_or_default();
                let callee = match cx.field(node, "object") {
                    Some(object) => Expr::Field {
                        of: Box::new(expr(cx, object)),
                        name,
                    },
                    None => Expr::Name(name),
                };
                let args = cx
                    .field(node, "arguments")
                    .map(|a| cx.children(a).into_iter().map(|n| expr(cx, n)).collect())
                    .unwrap_or_default();
                call_or_carry(cx, node, callee, args)
            }
            "object_creation_expression" => Expr::New {
                callee: Box::new(
                    cx.field(node, "type")
                        .map(|t| {
                            // `new ArrayList<>()`, the diamond is Java's syntax, not
                            // part of the name, and `ArrayList<>()` is not a call in
                            // any of the targets.
                            let text = cx.text(t);
                            Expr::Name(
                                text.split(['<', '['])
                                    .next()
                                    .unwrap_or(&text)
                                    .trim()
                                    .to_string(),
                            )
                        })
                        .unwrap_or(Expr::Null),
                ),
                args: cx
                    .field(node, "arguments")
                    .map(|a| cx.children(a).into_iter().map(|n| expr(cx, n)).collect())
                    .unwrap_or_default(),
            },
            "cast_expression" => Expr::Cast {
                ty: Box::new(
                    cx.field(node, "type")
                        .map(|t| Expr::Name(cx.text(t)))
                        .unwrap_or(Expr::Null),
                ),
                value: Box::new(
                    cx.field(node, "value")
                        .map(|v| expr(cx, v))
                        .unwrap_or(Expr::Null),
                ),
            },
            "instanceof_expression" => Expr::InstanceOf {
                value: Box::new(
                    cx.field(node, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null),
                ),
                ty: Box::new(
                    cx.field(node, "right")
                        .map(|r| Expr::Name(cx.text(r)))
                        .unwrap_or(Expr::Null),
                ),
            },
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
                let operand = cx
                    .field(node, "operand")
                    .map(|o| expr(cx, o))
                    .unwrap_or(Expr::Null);
                match cx.field_text(node, "operator").as_deref() {
                    Some("!") => Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    Some("-") => Expr::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // A cast is not free in Java the way `as` is in TypeScript: it checks at
            // run time and throws. Dropping it would drop the check.
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

/// Zig.
///
/// Two things shape this reader. A `variable_declaration` with no `var` or `const` in front of
/// it is an **assignment**. It is not a declaration — the grammar reuses the node. So telling the two
/// apart means reading the keyword instead of the node kind. And a type is a value: `const
/// Reading = struct { … };` is a `variable_declaration` whose value happens to be a struct,
/// which is where records come from.
///
/// What deliberately does not cross: `try`, `catch`, error unions and `comptime`. Zig models
/// failure in the return type and no other target here has anything to put there. So each is
/// carried with the original beside it.
mod zig {
    use super::*;

    /// Every child, punctuation included.
    ///
    /// `cx.children` gives the named nodes only, and in this grammar the `:` before a type, the
    /// `=` before a value and every operator are anonymous. So the shape of a declaration is
    /// invisible without them. Reading a binary expression by position instead put the right
    /// operand where the operator should have been. Every piece of arithmetic in the file came
    /// out as "no counterpart".
    fn all<'t>(node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.children(&mut cursor).collect()
    }

    /// The node after `token`, up to the next `stop` token.
    ///
    /// Named-ness is not the test. `undefined` is an anonymous token in this grammar and it is
    /// a perfectly good value. So requiring a named node lost every constant this tool's own
    /// Zig writer emits for something it could not translate.
    fn after<'t>(parts: &[Node<'t>], token: &str, stop: &str) -> Option<Node<'t>> {
        let at = parts.iter().position(|c| c.kind() == token)?;
        parts.get(at + 1).filter(|c| c.kind() != stop).copied()
    }

    /// `.empty`: a field expression with no object, only the leading dot.
    fn dot_literal(cx: &Cx, node: Node<'_>) -> Option<String> {
        if node.kind() != "field_expression" {
            return None;
        }
        let parts = all(node);
        match parts.as_slice() {
            [dot, member] if dot.kind() == "." && member.kind() == "identifier" => {
                Some(cx.text(*member))
            }
            _ => None,
        }
    }

    pub fn module(cx: &Cx, root: Node<'_>, file_stem: Option<&str>) -> Module {
        let mut module = Module::default();
        // A method a record cannot keep still has to reach the reader, and a record has
        // no room for one. It goes beside the type instead, as a carried comment.
        let mut carried: Vec<Item> = Vec::new();
        // The error sets the file declares, so the return pass can tell an error
        // variant from a success value.
        let mut error_sets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        // The file-as-struct idiom: fields at file scope make the file itself a
        // struct, and `const Self = @This();` says so in as many words. The type's
        // name is the binding's. When the binding is the conventional `Self`,
        // everyone importing the file calls the type by the file's name.
        let binding = this_binding(cx, root);
        let has_file_fields = cx
            .children(root)
            .iter()
            .any(|c| c.kind() == "container_field");
        let record_name = match (&binding, has_file_fields) {
            (Some(name), _) if name != "Self" => Some(name.clone()),
            (Some(_), _) | (None, true) => file_stem.map(str::to_string),
            (None, false) => None,
        };
        let mut file_record = record_name.map(|name| Record {
            doc: Vec::new(),
            name,
            fields: Vec::new(),
            extends: None,
            exported: true,
            methods: Vec::new(),
        });
        let mut record_at: Option<usize> = None;

        for child in cx.children(root) {
            match child.kind() {
                "comment" => {}
                "container_field" if file_record.is_some() => {
                    let record = file_record.as_mut().expect("checked");
                    record_at.get_or_insert(module.items.len());
                    match field(cx, child, true) {
                        Some(field) => record.fields.push(field),
                        None => carried.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                "function_declaration" => module.items.push(match function(cx, child) {
                    Some(f) => Item::Function(f),
                    None => Item::Unsupported(cx.unsupported(child)),
                }),
                // `test "name" { … }` is a named test. The form that names a
                // declaration instead of a string reruns that declaration's
                // tests, and carries.
                "test_declaration" => module.items.push(match test_block(cx, child) {
                    Some(t) => t,
                    None => Item::Unsupported(cx.unsupported(child)),
                }),
                "variable_declaration" if file_record.is_some() && binds_this(cx, child) => {
                    // The binding *is* the record's name; carrying it as a constant
                    // would declare the type twice.
                    record_at.get_or_insert(module.items.len());
                }
                "variable_declaration" => match declaration(cx, child, &mut carried) {
                    Some(item) => {
                        if let (Item::Sum(sum), true) = (&item, is_error_set(child)) {
                            error_sets.insert(sum.name.clone());
                        }
                        module.items.push(item);
                    }
                    None => module.items.push(Item::Unsupported(cx.unsupported(child))),
                },
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        if let Some(record) = file_record {
            // Inside the file the type went by the binding's name, `Self` most of
            // the time, and outside it goes by the record's. A signature still
            // saying `Self` would name a type the output never declares.
            if let Some(binding) = &binding {
                if *binding != record.name {
                    for item in module.items.iter_mut() {
                        if let Item::Function(f) = item {
                            rename_type(f, binding, &record.name);
                        }
                    }
                }
            }
            let at = record_at.unwrap_or(module.items.len());
            module
                .items
                .insert(at.min(module.items.len()), Item::Record(record));
        }
        module.items.extend(carried);
        settle_builtins(&mut module);
        settle_error_returns(&mut module, &error_sets);
        module
    }

    /// Is this declaration's value an error set?
    fn is_error_set(node: Node<'_>) -> bool {
        let parts = all(node);
        after(&parts, "=", ";").is_some_and(|value| value.kind() == "error_set_declaration")
    }

    /// The everyday spelling, rewritten to the table's canonical one.
    ///
    /// `.len` on a slice is the length every other language asks for with `len(x)`,
    /// `.length` or `.size()`. Written through as a field, it was a compile error in
    /// every target. Only where no record of the module declares a field called
    /// `len`, because then the read might be that field.
    fn settle_builtins(module: &mut Module) {
        let field_named_len = module.items.iter().any(
            |item| matches!(item, Item::Record(r) if r.fields.iter().any(|f| f.name == "len")),
        );
        if field_named_len {
            return;
        }
        super::each_expr_in_module(module, &mut |e| {
            if let Expr::Field { of, name } = e {
                if name == "len" {
                    let of = of.clone();
                    *e = Expr::Call {
                        callee: Box::new(Expr::Name("len".to_string())),
                        args: vec![*of],
                    };
                }
            }
        });
    }

    /// The success and failure paths of an error-union function, said as `Ok` and
    /// `Err`.
    ///
    /// Zig coerces at the `return`: a plain value succeeds and an error variant
    /// fails, written nowhere but the value itself. The IR's `Result` keeps the two
    /// apart the way Rust spells them. The Go writer turns the same body into its
    /// `(T, error)` returns from there. A function that can only fail may also fall
    /// off its end: the success path with nothing to say. It gains the
    /// `return Ok(())` the targets need to hear.
    fn settle_error_returns(module: &mut Module, error_sets: &std::collections::BTreeSet<String>) {
        let failing = |value: &Expr| match value {
            Expr::Field { of, .. } => {
                matches!(of.as_ref(), Expr::Name(n) if n == "error" || error_sets.contains(n))
            }
            _ => false,
        };
        let wrap = |name: &str, value: Expr| Expr::Call {
            callee: Box::new(Expr::Name(name.to_string())),
            args: vec![value],
        };
        let mut settle = |f: &mut Function| {
            let Some(Type::Named { name, args }) = &f.returns else {
                return;
            };
            if name != "Result" || args.len() != 2 {
                return;
            }
            let unit = args[0] == Type::Unit;
            super::each_stmt_in_stmts(&mut f.body, &mut |stmt| {
                let Stmt::Return(value) = stmt else { return };
                match value.take() {
                    Some(read) if failing(&read) => *value = Some(wrap("Err", read)),
                    Some(read) => *value = Some(wrap("Ok", read)),
                    None => *value = Some(wrap("Ok", Expr::Tuple(Vec::new()))),
                }
            });
            if unit && !matches!(f.body.last(), Some(Stmt::Return(_))) {
                f.body
                    .push(Stmt::Return(Some(wrap("Ok", Expr::Tuple(Vec::new())))));
            }
        };
        for item in module.items.iter_mut() {
            match item {
                Item::Function(f) => settle(f),
                Item::Record(r) => r.methods.iter_mut().for_each(&mut settle),
                _ => {}
            }
        }
    }

    /// Substitute one type name for another everywhere a signature says it.
    fn rename_type(function: &mut Function, from: &str, to: &str) {
        fn in_type(ty: &mut Type, from: &str, to: &str) {
            match ty {
                Type::Named { name, args } => {
                    if name == from {
                        *name = to.to_string();
                    }
                    for arg in args {
                        in_type(arg, from, to);
                    }
                }
                Type::List(inner) | Type::Optional(inner) => in_type(inner, from, to),
                Type::Tuple(parts) => {
                    for part in parts {
                        in_type(part, from, to);
                    }
                }
                Type::Map(key, value) => {
                    in_type(key, from, to);
                    in_type(value, from, to);
                }
                Type::Unit | Type::Bool | Type::Int | Type::Float | Type::String => {}
            }
        }
        if function.receiver.as_deref() == Some(from) {
            function.receiver = Some(to.to_string());
        }
        for param in function.params.iter_mut() {
            if let Some(ty) = param.ty.as_mut() {
                in_type(ty, from, to);
            }
        }
        if let Some(returns) = function.returns.as_mut() {
            in_type(returns, from, to);
        }
    }

    /// The name bound to `@This()` at the top level, if any.
    fn this_binding(cx: &Cx, root: Node<'_>) -> Option<String> {
        cx.children(root)
            .into_iter()
            .filter(|c| c.kind() == "variable_declaration")
            .find(|c| binds_this(cx, *c))
            .and_then(|c| {
                all(c)
                    .iter()
                    .find(|part| part.kind() == "identifier")
                    .map(|part| cx.text(*part))
            })
    }

    /// A `switch` whose cases are selected by literals, as the shared switch.
    ///
    /// A range (`200...299`), a capture (`|v|`), or anything else with structure
    /// makes this the full construct, and the caller carries it whole.
    fn switch_stmt(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let children = cx.children(node);
        let subject = children.first().copied()?;
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Vec<Stmt> = Vec::new();
        for case in children.iter().skip(1) {
            if case.kind() == "comment" {
                continue;
            }
            if case.kind() != "switch_case" {
                return None;
            }
            let parts = all(*case);
            if parts
                .iter()
                .any(|c| matches!(c.kind(), "payload" | "..." | ".."))
            {
                return None;
            }
            let arrow = parts.iter().position(|c| c.kind() == "=>")?;
            let body_node = parts.get(arrow + 1).copied()?;
            let body = if is_body(body_node) {
                body_of(cx, body_node)
            } else {
                vec![stmt(cx, body_node)]
            };
            if parts[..arrow].iter().any(|c| c.kind() == "else") {
                default = body;
                continue;
            }
            let mut literals = Vec::new();
            for value in parts[..arrow].iter().filter(|c| c.is_named()) {
                if !matches!(
                    value.kind(),
                    "integer" | "float" | "string" | "char_literal"
                ) {
                    return None;
                }
                literals.push(expr(cx, *value));
            }
            if literals.is_empty() {
                return None;
            }
            arms.push((literals, body));
        }
        Some(Stmt::Switch {
            subject: expr(cx, subject),
            arms,
            default,
        })
    }

    /// `test "name" { … }`, with the name unquoted.
    fn test_block(cx: &Cx, node: Node<'_>) -> Option<Item> {
        let children = cx.children(node);
        // `test "prose name" { … }` and `test declName { … }` are both tests; the
        // identifier form names the declaration it covers. Requiring the string
        // dropped every identifier-named test in a file, whole.
        let name = children
            .iter()
            .find(|c| c.kind() == "string")
            .map(|s| super::unquote(&cx.text(*s)))
            .or_else(|| {
                children
                    .iter()
                    .find(|c| c.kind() == "identifier")
                    .map(|s| cx.text(*s))
            })?;
        let body = children
            .iter()
            .find(|c| c.kind() == "block")
            .map(|b| block(cx, *b))
            .unwrap_or_default();
        Some(Item::Test {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            body,
        })
    }

    /// Is this declaration `const X = @This();`?
    fn binds_this(cx: &Cx, node: Node<'_>) -> bool {
        let parts = all(node);
        after(&parts, "=", ";")
            .is_some_and(|v| v.kind() == "builtin_function" && cx.text(v).starts_with("@This"))
    }

    /// `const X = …;` at the top level: an import, a struct, or a constant.
    fn declaration(cx: &Cx, node: Node<'_>, carried: &mut Vec<Item>) -> Option<Item> {
        let parts = all(node);
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let exported = cx.text(node).trim_start().starts_with("pub");
        let value = after(&parts, "=", ";")?;

        // `const std = @import("std");` is a dependency. It is not a constant.
        if value.kind() == "builtin_function" && cx.text(value).starts_with("@import") {
            return Some(Item::Import {
                text: cx.text(node),
                line: cx.line(node),
            });
        }

        if value.kind() == "struct_declaration" {
            return Some(Item::Record(record(
                cx, node, name, exported, value, carried,
            )));
        }
        // An enum is a choice with bare variants; `union(enum)` is a choice with
        // payloads. Both are sums. A bare `union` is neither. It overlays its
        // members in memory and knows nothing about which one is live.
        // Flattening it into anything would invent a meaning it does not have.
        if value.kind() == "enum_declaration" {
            return Some(Item::Sum(plain_enum(cx, node, name, exported, value)));
        }
        if value.kind() == "union_declaration" {
            if cx.text(value).trim_start().starts_with("union(enum") {
                return tagged_union(cx, node, name, exported, value).map(Item::Sum);
            }
            return None;
        }
        // `const E = error{A, B};` declares a closed set of failure names, which is a
        // sum with unit variants. Read as a constant, the value had no counterpart.
        // The set crossed as a comment while every signature naming it went through
        // as an unwritable type.
        if value.kind() == "error_set_declaration" {
            return Some(Item::Sum(error_set(cx, node, name, exported, value)));
        }

        Some(Item::Constant(Constant {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
            value: expr(cx, value),
            exported,
        }))
    }

    /// `error{A, B}`: the failure names, one unit variant each.
    fn error_set(cx: &Cx, node: Node<'_>, name: String, exported: bool, body: Node<'_>) -> Sum {
        let variants = cx
            .children(body)
            .into_iter()
            .filter(|c| c.kind() == "identifier")
            .map(|c| Variant {
                doc: doc_above(cx, c, &["///", "//"]),
                name: cx.text(c),
                fields: Vec::new(),
            })
            .collect();
        Sum {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            variants,
            exported,
        }
    }

    /// `enum { a, b }`: a choice whose variants carry nothing.
    fn plain_enum(cx: &Cx, node: Node<'_>, name: String, exported: bool, body: Node<'_>) -> Sum {
        let mut variants = Vec::new();
        for member in cx.children(body) {
            if member.kind() != "container_field" {
                continue;
            }
            let parts = all(member);
            let Some(variant_name) = parts.first().map(|c| cx.text(*c)) else {
                continue;
            };
            let mut doc = doc_above(cx, member, &["///", "//"]);
            // An explicit tag value has no slot in the IR: kept as words, not
            // dropped in silence.
            if let Some(value) = after(&parts, "=", "\u{0}") {
                doc.push(format!(
                    "the source gave this the value `{}`",
                    cx.text(value).trim()
                ));
            }
            variants.push(Variant {
                doc,
                name: variant_name,
                fields: Vec::new(),
            });
        }
        Sum {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            variants,
            exported,
        }
    }

    /// `union(enum)`: a choice whose variants carry payloads.
    fn tagged_union(
        cx: &Cx,
        node: Node<'_>,
        name: String,
        exported: bool,
        body: Node<'_>,
    ) -> Option<Sum> {
        let mut variants = Vec::new();
        for member in cx.children(body) {
            match member.kind() {
                "container_field" => {
                    let parts = all(member);
                    let variant_name = parts.first().map(|c| cx.text(*c))?;
                    let doc = doc_above(cx, member, &["///", "//"]);
                    let payload = after(&parts, ":", "=");
                    let fields = match payload {
                        // `done: void` carries nothing; the variant is the news.
                        None => Vec::new(),
                        Some(t) if cx.text(t).trim() == "void" => Vec::new(),
                        // An anonymous struct payload has named fields of its own.
                        Some(t) if t.kind() == "struct_declaration" => cx
                            .children(t)
                            .into_iter()
                            .filter(|f| f.kind() == "container_field")
                            .filter_map(|f| field(cx, f, exported))
                            .collect(),
                        // Anything else is the payload itself, one value.
                        Some(t) => vec![Field {
                            doc: Vec::new(),
                            name: "value".to_string(),
                            ty: Some(ty_of(cx, t)),
                            default: None,
                            exported,
                        }],
                    };
                    variants.push(Variant {
                        doc,
                        name: variant_name,
                        fields,
                    });
                }
                "comment" => {}
                // A declaration inside the union, a method or a nested type, has no
                // slot in a sum. Refusing the whole union keeps it carried verbatim
                // instead of half-translated.
                _ => return None,
            }
        }
        Some(Sum {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            variants,
            exported,
        })
    }

    fn record(
        cx: &Cx,
        node: Node<'_>,
        name: String,
        exported: bool,
        body: Node<'_>,
        carried: &mut Vec<Item>,
    ) -> Record {
        let mut record = Record {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            fields: Vec::new(),
            // Zig has no inheritance at all.
            extends: None,
            exported,
            methods: Vec::new(),
        };
        for member in cx.children(body) {
            match member.kind() {
                "container_field" => match field(cx, member, exported) {
                    Some(field) => record.fields.push(field),
                    None => carried.push(Item::Unsupported(cx.unsupported(member))),
                },
                "function_declaration" => match function(cx, member) {
                    Some(mut f) => {
                        f.is_constructor = super::constructs(
                            &f.name,
                            &record.name,
                            f.returns.as_ref(),
                            f.receiver_binding.is_some(),
                        )
                        .is_some();
                        record.methods.push(f);
                    }
                    None => carried.push(Item::Unsupported(cx.unsupported(member))),
                },
                // A member this does not recognise is not a member that is not
                // there. Every reader here ended its member loop with `_ => {}`,
                // which is how a `@staticmethod` disappeared from a class while the
                // report said every signature had carried across intact.
                _ => carried.push(Item::Unsupported(cx.unsupported(member))),
            }
        }
        record
    }

    /// A struct field, wherever the struct is: in a declaration, or the file itself.
    fn field(cx: &Cx, member: Node<'_>, exported: bool) -> Option<Field> {
        let parts = all(member);
        let name = parts.first().map(|c| cx.text(*c))?;
        let mut doc = doc_above(cx, member, &["///", "//"]);
        // A default is dropped, because no other language here puts one
        // on a plain struct field. Dropped and said, where it was silent.
        if let Some(default) = after(&parts, "=", "\u{0}") {
            doc.push(format!(
                "the source gave this a default: `{}`",
                cx.text(default).trim()
            ));
        }
        Some(Field {
            doc,
            name,
            // `x: i32`, the type is whatever follows the colon, and stops
            // before the `=` of a default.
            ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
            default: None,
            // Zig has no per-field visibility; a field of an exported type
            // is reachable wherever the type is.
            exported,
        })
    }

    /// A function, unless its signature is a compile-time computation.
    fn function(cx: &Cx, node: Node<'_>) -> Option<Function> {
        let children = cx.children(node);
        let name = children
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))
            .unwrap_or_default();

        let mut params = Vec::new();
        let mut receiver = None;
        let mut receiver_name = None;
        let mut comptime = false;
        if let Some(list) = children.iter().find(|c| c.kind() == "parameters") {
            for parameter in cx.children(*list) {
                if parameter.kind() != "parameter" {
                    continue;
                }
                // `comptime T: type` is Zig's generics: the parameter is a *type*,
                // supplied where another language would write `<T>`. The IR has no
                // generic parameters, and reading it as an ordinary one produced
                // `func Lazy(comptime type, comptime type) type`, a signature that
                // means something else in every target.
                if cx.text(parameter).trim_start().starts_with("comptime") {
                    comptime = true;
                    continue;
                }
                let parts = all(parameter);
                let Some(parameter_name) = parts.first().map(|c| cx.text(*c)) else {
                    continue;
                };
                let ty = after(&parts, ":", ")").map(|t| ty_of(cx, t));
                // `self: Reading` is the receiver, spelled as an ordinary parameter.
                if parameter_name == "self" {
                    receiver = ty.as_ref().map(|t| t.to_string());
                    receiver_name = Some(parameter_name);
                    continue;
                }
                params.push(Param {
                    name: parameter_name,
                    ty,
                    default: None,
                    kind: ParamKind::Normal,
                });
            }
        }

        // The return type sits between the parameter list and the body.
        let returns = children
            .iter()
            .position(|c| c.kind() == "parameters")
            .and_then(|at| children.get(at + 1))
            .filter(|c| c.kind() != "block")
            .map(|t| ty_of(cx, *t));

        // A `comptime` parameter is a *type*, supplied where another language writes
        // `<T>`. The IR has no generic parameters, and reading one as an ordinary
        // parameter produced `func Lazy(comptime type, comptime type) type`, a
        // signature that means something else in every target.
        if comptime {
            return None;
        }

        Some(Function {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            receiver,
            receiver_binding: receiver_name,
            params,
            returns,
            body: children
                .iter()
                .find(|c| c.kind() == "block")
                .map(|b| block(cx, *b))
                .unwrap_or_default(),
            exported: cx.text(node).trim_start().starts_with("pub"),
            is_async: false,
            is_property: false,
            is_constructor: false,
        })
    }

    fn ty_of(cx: &Cx, node: Node<'_>) -> Type {
        let text = cx.text(node);
        match node.kind() {
            "builtin_type" => builtin(text.trim()).unwrap_or_else(|| Type::named(text.trim())),
            // `[]const u8` is Zig's string; `[]T` is a slice of anything else.
            "slice_type" | "array_type" => {
                let element = cx
                    .children(node)
                    .into_iter()
                    .next_back()
                    .map(|e| ty_of(cx, e))
                    .unwrap_or_else(|| Type::named("anytype"));
                if element == Type::Int && text.contains("u8") {
                    return Type::String;
                }
                Type::List(Box::new(element))
            }
            // The grammar's name for `?T`. Reading it as `optional_type`, which is
            // what it looks like it should be called, matched nothing, so every
            // optional in every Zig file crossed as a foreign type spelled `?T`.
            "nullable_type" => Type::Optional(Box::new(
                cx.children(node)
                    .into_iter()
                    .next_back()
                    .map(|inner| ty_of(cx, inner))
                    .unwrap_or_else(|| Type::named("anytype")),
            )),
            // A pointer is how Zig writes a reference, and the languages that have no
            // pointers still have the thing being pointed at. The other readers do the
            // same with Rust's `&T`.
            "pointer_type" => cx
                .children(node)
                .into_iter()
                .next_back()
                .map(|inner| ty_of(cx, inner))
                .unwrap_or_else(|| Type::named("anytype")),
            // `E!T` models failure in the return type, and so does `Result<T, E>`: the
            // shared name the Rust reader already produces. Carrying it as that name
            // lets the writers that translate Results translate this too, instead of
            // writing an unwritable type through every signature.
            "error_union_type" => match split_error_union(&type_text(&text)) {
                Some((err, ok)) => Type::Named {
                    name: "Result".to_string(),
                    args: vec![from_text(ok), error_side(err)],
                },
                None => Type::named(type_text(&text)),
            },
            // The grammar binds `?` tighter than `.`, so `?http.Request` arrives as a
            // field expression whose left side is a nullable `http`, inside out. The
            // text is the only way back to what was written.
            _ => from_text(&type_text(&text)),
        }
    }

    /// The two sides of `E!T`, split at the union's own `!`.
    ///
    /// Depth matters: `error{A, B}!void` keeps its braces together, and a `!` inside a
    /// nested set belongs to that set. No top-level `!` means the text was not an
    /// error union after all.
    fn split_error_union(text: &str) -> Option<(&str, &str)> {
        let mut depth = 0i32;
        for (at, c) in text.char_indices() {
            match c {
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                '!' if depth == 0 => return Some((&text[..at], &text[at + 1..])),
                _ => {}
            }
        }
        None
    }

    /// The error half of an error union, as a type the targets can carry.
    ///
    /// A named set keeps its name, and the writers can point at the sum it became.
    /// `anyerror`, a bare `!T` and an inline `error{...}` name nothing that outlives
    /// this signature, so they cross as the generic `error`.
    fn error_side(err: &str) -> Type {
        let err = err.trim();
        let anonymous = err.is_empty()
            || err == "anyerror"
            || err.starts_with("error{")
            || err.starts_with("error {")
            || !Type::is_writable_name(err);
        match anonymous {
            true => Type::named("error"),
            false => Type::named(err),
        }
    }

    /// A type name with the whitespace collapsed and the comments taken out.
    ///
    /// A Zig type can span lines and hold doc comments, an error union over an
    /// anonymous union does, and `cx.text` returns all of it. Written through as a
    /// name, that produced two hundred characters of prose where a type should be.
    fn type_text(text: &str) -> String {
        text.lines()
            .map(|line| line.split("//").next().unwrap_or("").trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A type read from its text, for the shapes the grammar hands back inside out.
    fn from_text(text: &str) -> Type {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix('?') {
            return Type::Optional(Box::new(from_text(rest)));
        }
        // A generic type here is a name *applied* to its arguments — `ArrayList(u8)`,
        // which the writer emits. Reading the whole thing as one name turned
        // `HashSet(Thing)` into a type called `HashSet(Thing)`.
        // A slice is a list. The grammar gives this its own node most of the time, and
        // the rest of the time it arrives here as text.
        if let Some(rest) = text.strip_prefix("[]") {
            let element = rest.trim_start().trim_start_matches("const ").trim();
            return match element {
                "u8" => Type::String,
                other => Type::List(Box::new(from_text(other))),
            };
        }
        // `struct { A, B }` with only types inside is Zig's tuple, and it is what this
        // tool's own writer emits for one. A `:` inside names a field, a real struct.
        if let Some(inside) = text
            .strip_prefix("struct")
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix('{'))
            .and_then(|s| s.strip_suffix('}'))
        {
            let parts = super::comma_parts(inside);
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && !p.contains(':')) {
                return Type::Tuple(parts.iter().map(|p| from_text(p)).collect());
            }
        }
        if let Some((head, rest)) = text.split_once('(') {
            if let Some(args) = rest.strip_suffix(')') {
                let head = head.trim();
                if !head.is_empty() {
                    // Depth-aware: `HashSet(struct { A, B })` has one argument, and a
                    // bare split at every comma read it as two halves of nothing.
                    let arguments: Vec<Type> = super::comma_parts(args)
                        .iter()
                        .map(|a| from_text(a))
                        .collect();
                    // The two maps this tool's own Zig writer emits. Reading them back
                    // as ordinary named types made a `dict[str, str]` cross once and
                    // never come home.
                    let base = head.rsplit('.').next().unwrap_or(head);
                    return match (base, arguments.as_slice()) {
                        ("StringHashMap", [value]) => {
                            Type::Map(Box::new(Type::String), Box::new(value.clone()))
                        }
                        ("AutoHashMap", [key, value]) => {
                            Type::Map(Box::new(key.clone()), Box::new(value.clone()))
                        }
                        ("ArrayList", [element]) => Type::List(Box::new(element.clone())),
                        _ => Type::Named {
                            name: head.to_string(),
                            args: arguments,
                        },
                    };
                }
            }
        }
        // A pointer is how Zig writes a reference, and the languages without pointers
        // still have the thing being pointed at.
        if let Some(rest) = text.strip_prefix('*') {
            return from_text(rest.trim_start().trim_start_matches("const ").trim_start());
        }
        builtin(text).unwrap_or_else(|| match text {
            "" => Type::named("anytype"),
            other => Type::named(other),
        })
    }

    /// Zig's own scalar names, wherever a type is read from.
    fn builtin(text: &str) -> Option<Type> {
        Some(match text {
            "bool" => Type::Bool,
            "void" | "noreturn" => Type::Unit,
            "f16" | "f32" | "f64" | "f80" | "f128" => Type::Float,
            other if other.len() > 1 && other.starts_with(['i', 'u']) => {
                match other[1..].chars().all(|c| c.is_ascii_digit())
                    || other == "isize"
                    || other == "usize"
                {
                    true => Type::Int,
                    false => return None,
                }
            }
            _ => return None,
        })
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        let mut out = Vec::new();
        for n in cx.children_with_comments(node) {
            if let Some(lowered) = value_switch(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(switch) = return_switch(cx, n) {
                out.push(switch);
                continue;
            }
            out.push(keep_whole(cx, n, stmt(cx, n)));
        }
        out
    }

    /// A switch whose every case is a literal selecting one value expression.
    ///
    /// The shape a switch must have to stand where a value goes. A payload, a range,
    /// a block body or a missing `else` makes it the full construct, and the caller
    /// carries it whole. Without the `else`, some path would never produce the value.
    #[allow(clippy::type_complexity)]
    fn switch_arm_values(cx: &Cx, node: Node<'_>) -> Option<(Expr, Vec<(Vec<Expr>, Expr)>, Expr)> {
        if node.kind() != "switch_expression" {
            return None;
        }
        let children = cx.children(node);
        let subject = expr(cx, *children.first()?);
        if matches!(subject, Expr::Unsupported(_)) {
            return None;
        }
        let mut arms: Vec<(Vec<Expr>, Expr)> = Vec::new();
        let mut default = None;
        for case in children.iter().skip(1) {
            if case.kind() == "comment" {
                continue;
            }
            if case.kind() != "switch_case" {
                return None;
            }
            let parts = all(*case);
            if parts
                .iter()
                .any(|c| matches!(c.kind(), "payload" | "..." | ".."))
            {
                return None;
            }
            let arrow = parts.iter().position(|c| c.kind() == "=>")?;
            let body_node = parts.get(arrow + 1).copied()?;
            if is_body(body_node) {
                return None;
            }
            let value = expr(cx, body_node);
            if matches!(value, Expr::Unsupported(_)) {
                return None;
            }
            if parts[..arrow].iter().any(|c| c.kind() == "else") {
                default = Some(value);
                continue;
            }
            let mut literals = Vec::new();
            for selector in parts[..arrow].iter().filter(|c| c.is_named()) {
                if !matches!(
                    selector.kind(),
                    "integer" | "float" | "string" | "char_literal"
                ) {
                    return None;
                }
                literals.push(expr(cx, *selector));
            }
            if literals.is_empty() {
                return None;
            }
            arms.push((literals, value));
        }
        Some((subject, arms, default?))
    }

    /// `const x = switch (s) { a => va, else => ve };`: a switch standing where a
    /// value goes.
    ///
    /// The IR's switch is a statement, so the reader says the same thing longhand:
    /// declare the binding, then a switch whose every arm assigns it. Every writer
    /// already writes that shape, and the Rust writer folds the pair back into a
    /// `match` expression.
    fn value_switch(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        if node.kind() != "variable_declaration" {
            return None;
        }
        let text = cx.text(node);
        let declares =
            text.trim_start().starts_with("var ") || text.trim_start().starts_with("const ");
        if !declares {
            return None;
        }
        let parts = all(node);
        if parts.iter().any(|c| c.kind() == ",") {
            return None;
        }
        let value = after(&parts, "=", ";")?;
        let (subject, arms, default) = switch_arm_values(cx, value)?;
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let assign = |value: Expr| Stmt::Assign {
            target: Expr::Name(name.clone()),
            value,
        };
        Some(vec![
            Stmt::Let {
                name: name.clone(),
                ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
                value: None,
                mutable: true,
            },
            Stmt::Switch {
                subject,
                arms: arms
                    .into_iter()
                    .map(|(literals, value)| (literals, vec![assign(value)]))
                    .collect(),
                default: vec![assign(default)],
            },
        ])
    }

    /// `return switch (s) { a => va, else => ve };`, said as a switch whose arms
    /// return.
    fn return_switch(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let node = match node.kind() {
            "expression_statement" => cx.children(node).into_iter().next()?,
            _ => node,
        };
        if node.kind() != "return_expression" {
            return None;
        }
        let value = cx.children(node).into_iter().next()?;
        let (subject, arms, default) = switch_arm_values(cx, value)?;
        Some(Stmt::Switch {
            subject,
            arms: arms
                .into_iter()
                .map(|(literals, value)| (literals, vec![Stmt::Return(Some(value))]))
                .collect(),
            default: vec![Stmt::Return(Some(default))],
        })
    }

    /// The statements inside a `block_expression`, or the one statement without braces.
    fn body_of(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        match node.kind() {
            // A braced body arrives wrapped, and an `else { … }` arrives wrapped
            // twice: the grammar treats every block as labelable whether or not it
            // carries a label.
            "block_expression" | "labeled_statement" => cx
                .children(node)
                .into_iter()
                .find(|c| c.kind() == "block")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            "block" => block(cx, node),
            _ => vec![keep_whole(cx, node, stmt(cx, node))],
        }
    }

    /// Is this node the braced body of something?
    fn is_body(node: Node<'_>) -> bool {
        matches!(
            node.kind(),
            "block_expression" | "block" | "labeled_statement"
        )
    }

    fn stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            "comment" => Stmt::Comment(super::uncomment(&cx.text(node))),
            "expression_statement" => match cx.children(node).first().copied() {
                Some(inner) => stmt(cx, inner),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "return_expression" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            "break_expression" => Stmt::Break,
            "continue_expression" => Stmt::Continue,
            // The grammar reuses this node for both a declaration and an assignment;
            // only the keyword tells them apart.
            "variable_declaration" => {
                let text = cx.text(node);
                let parts = all(node);
                let declares = text.trim_start().starts_with("var ")
                    || text.trim_start().starts_with("const ");
                // The first *named* child: `var sum = 0` starts with the keyword,
                // which is punctuation, and taking it declared a variable called `var`.
                // `const a, const b = pair;` binds two names and the IR binds one.
                // Reading the first and dropping the rest kept `const a = pair;` and
                // lost `b` without a word.
                if parts.iter().any(|c| c.kind() == ",") {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                let Some(target) = parts.iter().find(|c| c.is_named()).copied() else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                // The operator may be `=` or a compound `+=`; both end in the
                // one character, and `==` never appears in statement position.
                let operator = parts
                    .iter()
                    .find(|c| !c.is_named() && c.kind().ends_with('=') && c.kind() != "==")
                    .map(|c| cx.text(*c))
                    .unwrap_or_default();
                let value = parts
                    .iter()
                    .position(|c| !c.is_named() && cx.text(*c) == operator)
                    .and_then(|at| parts.get(at + 1))
                    .filter(|c| c.kind() != ";")
                    .copied();
                let Some(value) = value else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                if !declares && operator != "=" {
                    return match super::desugar_compound(
                        expr(cx, target),
                        &operator,
                        expr(cx, value),
                    ) {
                        Some(assign) => assign,
                        None => Stmt::Unsupported(cx.unsupported(node)),
                    };
                }
                if !declares {
                    // Writing *through* a pointer has no counterpart. Stripped, the
                    // write became a rebinding of the pointer itself, and when
                    // the pointer was the receiver, an assignment to `this`.
                    if target.kind() == "dereference_expression" {
                        return Stmt::Unsupported(cx.unsupported(node));
                    }
                    return Stmt::Assign {
                        target: expr(cx, target),
                        value: expr(cx, value),
                    };
                }
                let mut read = expr(cx, value);
                // `.empty` names a member of the declared type, written with the
                // type left to inference: `var list: ArrayList(u8) = .empty;` means
                // `ArrayList(u8).empty`. The annotation says what to qualify it
                // with; without one there is nothing to say, and it stays carried.
                if matches!(read, Expr::Unsupported(_)) {
                    if let (Some(member), Some(annotated)) =
                        (dot_literal(cx, value), after(&parts, ":", "="))
                    {
                        read = Expr::Field {
                            of: Box::new(Expr::Name(cx.text(annotated).trim().to_string())),
                            name: member,
                        };
                    }
                }
                Stmt::Let {
                    name: cx.text(target),
                    ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
                    value: Some(read),
                    mutable: text.trim_start().starts_with("var "),
                }
            }
            "if_statement" => {
                let children = cx.children(node);
                // The branch may be a block, and it may be one bare statement:
                // `if (x) return y;` has no braces. Requiring a block dropped that
                // return without a word, and the translated guard tested its
                // condition and did nothing.
                let then = children
                    .iter()
                    .skip(1)
                    .find(|c| !matches!(c.kind(), "payload" | "else_clause" | "comment"))
                    .map(|b| body_of(cx, *b))
                    .unwrap_or_default();
                // The else branch is one level down, inside an `else_clause`.
                let else_clause = children.iter().find(|c| c.kind() == "else_clause");
                let otherwise = else_clause
                    .and_then(|e| {
                        cx.children(*e)
                            .into_iter()
                            .find(|c| !matches!(c.kind(), "payload" | "comment"))
                    })
                    .map(|b| body_of(cx, b))
                    .unwrap_or_default();
                // `if (maybe) |value| { … }` tests an optional and binds its
                // payload. A `|*value|` pointer capture writes through the
                // original, and an error union's `else |err|` binds a second
                // payload; neither has a crossing, so both carry whole.
                if let Some(payload) = children.iter().find(|c| c.kind() == "payload") {
                    let bindings: Vec<Node> = cx
                        .children(*payload)
                        .into_iter()
                        .filter(|c| c.kind() == "identifier")
                        .collect();
                    let by_pointer = all(*payload).iter().any(|c| c.kind() == "*");
                    let else_binds = else_clause.is_some_and(|e| {
                        let mut cursor = e.walk();
                        let found = e.children(&mut cursor).any(|c| c.kind() == "payload");
                        found
                    });
                    if let ([binding], false, false) = (bindings.as_slice(), by_pointer, else_binds)
                    {
                        return Stmt::IfPresent {
                            binding: cx.text(*binding),
                            value: children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null),
                            then,
                            otherwise,
                        };
                    }
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                let condition = children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null);
                Stmt::If {
                    condition,
                    then,
                    otherwise,
                }
            }
            "while_statement" => {
                let children = cx.children(node);
                // A step clause is a bare `:` with an expression, and it is not the
                // body. A loop that has one and no block would hand the step over as
                // the body, so only the stepless loop takes the one-statement form.
                let stepped = all(node).iter().any(|c| c.kind() == ":");
                let body = children
                    .iter()
                    .skip(1)
                    .find(|c| is_body(**c))
                    .or_else(|| {
                        children.iter().skip(1).find(|c| {
                            !stepped && !matches!(c.kind(), "payload" | "else_clause" | "comment")
                        })
                    })
                    .map(|b| body_of(cx, *b))
                    .unwrap_or_default();
                // `while (it.next()) |item|` loops on an optional's payload. A
                // `|*item|` pointer capture writes through the original, a
                // continue-expression (`: (i += 1)`) has no slot, and an `else`
                // here runs on exhaustion; none of the three crosses.
                if let Some(payload) = children.iter().find(|c| c.kind() == "payload") {
                    let bindings: Vec<Node> = cx
                        .children(*payload)
                        .into_iter()
                        .filter(|c| c.kind() == "identifier")
                        .collect();
                    let by_pointer = all(*payload).iter().any(|c| c.kind() == "*");
                    let has_else = children.iter().any(|c| c.kind() == "else_clause");
                    // The step clause is a bare `:` and a parenthesised expression,
                    // with no wrapper node to name.
                    let extras = all(node).iter().any(|c| c.kind() == ":");
                    if let ([binding], false, false, false) =
                        (bindings.as_slice(), by_pointer, has_else, extras)
                    {
                        return Stmt::WhilePresent {
                            binding: cx.text(*binding),
                            value: children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null),
                            body,
                        };
                    }
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                // A stepless loop crosses; one with a step clause has nowhere to put
                // it, and dropping the step turned a counting loop into a spin.
                if stepped {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Stmt::While {
                    condition: children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null),
                    body,
                }
            }
            // `for (xs) |x| { … }`, the binding is in the payload.
            "for_statement" => {
                let children = cx.children(node);
                let Some(payload) = children.iter().find(|c| c.kind() == "payload") else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let bindings: Vec<Node> = cx
                    .children(*payload)
                    .into_iter()
                    .filter(|c| c.kind() == "identifier")
                    .collect();
                let sequences: Vec<Node> = children
                    .iter()
                    .take_while(|c| c.kind() != "payload")
                    .copied()
                    .collect();
                // The body follows the payload, braced or not: `for (xs) |x| use(x);`
                // is a loop too, and requiring a block dropped its one statement.
                let body = children
                    .iter()
                    .skip_while(|c| c.kind() != "payload")
                    .skip(1)
                    .find(|c| c.kind() != "comment")
                    .map(|b| body_of(cx, *b))
                    .unwrap_or_default();
                // `for (xs, 0..) |x, i|` counts as it goes, and every target can
                // say that. Two real sequences in step, `for (xs, ys) |x, y|`,
                // still carry: the IR binds one name to one iterable.
                if bindings.len() == 2 && sequences.len() == 2 {
                    let counted = sequences[1].kind() == "range_expression"
                        && cx.text(sequences[1]).trim() == "0..";
                    if counted {
                        return Stmt::ForEachIndexed {
                            index: cx.text(bindings[1]),
                            binding: cx.text(bindings[0]),
                            iterable: expr(cx, sequences[0]),
                            body,
                        };
                    }
                }
                if bindings.len() != 1 || sequences.len() != 1 {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Stmt::ForEach {
                    binding: cx.text(bindings[0]),
                    iterable: expr(cx, sequences[0]),
                    body,
                }
            }
            // A `for` or `while` may carry a label; the loop is inside it.
            "labeled_statement" => match cx
                .children(node)
                .into_iter()
                .find(|c| matches!(c.kind(), "for_statement" | "while_statement"))
            {
                Some(loop_node) => stmt(cx, loop_node),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "call_expression" | "field_expression" | "identifier" | "try_expression" => {
                Stmt::Expr(expr(cx, node))
            }
            "switch_expression" => match switch_stmt(cx, node) {
                Some(switch) => switch,
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "defer_statement" | "errdefer_statement" => {
                // `errdefer |err| ...` binds the error; the binding has nowhere to
                // cross, so the payload form carries whole.
                if all(node).iter().any(|c| c.kind() == "payload") {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                let Some(deferred) = cx.children(node).first().copied() else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let body = if is_body(deferred) {
                    body_of(cx, deferred)
                } else {
                    vec![stmt(cx, deferred)]
                };
                match node.kind() {
                    "errdefer_statement" => Stmt::ErrDefer(body),
                    _ => Stmt::Defer(body),
                }
            }
            // At statement level an assignment hides in a `variable_declaration`;
            // inside a `defer` or a step clause it arrives as itself.
            "assignment_expression" => {
                let parts = all(node);
                let Some(target) = parts.iter().find(|c| c.is_named()).copied() else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let operator = parts
                    .iter()
                    .find(|c| !c.is_named() && c.kind().ends_with('=') && c.kind() != "==")
                    .map(|c| cx.text(*c))
                    .unwrap_or_default();
                let value = parts
                    .iter()
                    .position(|c| !c.is_named() && cx.text(*c) == operator)
                    .and_then(|at| parts.get(at + 1))
                    .copied();
                let Some(value) = value else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                if operator == "=" {
                    return Stmt::Assign {
                        target: expr(cx, target),
                        value: expr(cx, value),
                    };
                }
                match super::desugar_compound(expr(cx, target), &operator, expr(cx, value)) {
                    Some(assign) => assign,
                    None => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    fn expr(cx: &Cx, node: Node<'_>) -> Expr {
        match node.kind() {
            // `if (a) b else c` used as a value. A braced branch is a block, and a
            // block is a statement, reading one as an expression would need somewhere
            // to put the result.
            "if_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [condition, then, otherwise] if !is_body(*then) && !is_body(*otherwise) => {
                        Expr::Ternary {
                            condition: Box::new(expr(cx, *condition)),
                            then: Box::new(expr(cx, *then)),
                            otherwise: Box::new(expr(cx, *otherwise)),
                        }
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "integer" => Expr::Int(cx.text(node)),
            "float" => Expr::Float(cx.text(node)),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            "null" | "undefined" => Expr::Null,
            "string" => Expr::Str(super::unquote(&cx.text(node))),
            "identifier" | "builtin_type" => Expr::Name(cx.text(node)),
            "field_expression" => {
                let parts = cx.children(node);
                match (parts.first(), parts.last()) {
                    (Some(of), Some(name)) if parts.len() >= 2 => Expr::Field {
                        of: Box::new(expr(cx, *of)),
                        name: cx.text(*name),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "call_expression" => {
                let parts = cx.children(node);
                let Some(callee) = parts.first().copied() else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                // This grammar has no argument-list node: the arguments hang off
                // the call directly, after the callee. Looking for one anyway
                // found nothing, and every translated Zig call lost its
                // arguments without a word said.
                let args = parts
                    .iter()
                    .skip(1)
                    .filter(|c| !c.kind().contains("comment"))
                    .map(|n| expr(cx, *n))
                    .collect();
                call_or_carry(cx, node, expr(cx, callee), args)
            }
            // The operator is punctuation, so it is not among the named children:
            // `a * b` has two of those and the `*` is between them.
            "binary_expression" => {
                let parts = all(node);
                let operator = parts
                    .iter()
                    .find(|c| !c.is_named())
                    .map(|c| cx.text(*c))
                    .unwrap_or_default();
                let operands: Vec<Node> = parts.iter().filter(|c| c.is_named()).copied().collect();
                // `a orelse b` is Zig's word for exactly the question `??` asks.
                if operator == "orelse" && operands.len() == 2 {
                    return Expr::Coalesce {
                        value: Box::new(expr(cx, operands[0])),
                        fallback: Box::new(expr(cx, operands[1])),
                    };
                }
                match super::binary_op(&operator) {
                    Some(op) if operands.len() == 2 => Expr::Binary {
                        op,
                        left: Box::new(expr(cx, operands[0])),
                        right: Box::new(expr(cx, operands[1])),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "unary_expression" => {
                let parts = all(node);
                let operand = parts
                    .iter()
                    .find(|c| c.is_named())
                    .map(|o| expr(cx, *o))
                    .unwrap_or(Expr::Null);
                match parts.first().map(|c| cx.text(*c)).as_deref() {
                    Some("!") => Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    Some("-") => Expr::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    // A pointer is how Zig writes a reference, and the languages
                    // without pointers still have the thing being pointed at. The
                    // type reader already strips them the same way.
                    Some("&") => operand,
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `p.*` reads through the pointer; see `&` above.
            "dereference_expression" => cx
                .children(node)
                .first()
                .map(|inner| expr(cx, *inner))
                .unwrap_or_else(|| Expr::Unsupported(cx.unsupported(node))),
            "index_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [of, index] if index.kind() != "range_expression" => Expr::Index {
                        of: Box::new(expr(cx, *of)),
                        index: Box::new(expr(cx, *index)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `[_]u32{ 1, 2, 3 }` over an array type is a list, the same value
            // every target spells as its own literal.
            "struct_initializer" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [ty, items]
                        if ty.kind() == "array_type" && items.kind() == "initializer_list" =>
                    {
                        Expr::ListLit(cx.children(*items).iter().map(|i| expr(cx, *i)).collect())
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // The cast family reasserts a type over a value, which every language
            // here can spell. `@min` and `@max` are calls everywhere. The rest of
            // the builtins have no counterpart and are carried.
            "builtin_function" => {
                let name = cx
                    .children(node)
                    .first()
                    .filter(|c| c.kind() == "builtin_identifier")
                    .map(|c| cx.text(*c))
                    .unwrap_or_default();
                let args: Vec<Node> = cx
                    .children(node)
                    .iter()
                    .find(|c| c.kind() == "arguments")
                    .map(|a| cx.children(*a))
                    .unwrap_or_default();
                match (name.as_str(), args.as_slice()) {
                    ("@as" | "@intCast" | "@floatCast" | "@truncate", [ty, value]) => Expr::Cast {
                        ty: Box::new(expr(cx, *ty)),
                        value: Box::new(expr(cx, *value)),
                    },
                    ("@min" | "@max", _) => Expr::Call {
                        callee: Box::new(Expr::Name(name.trim_start_matches('@').to_string())),
                        args: args.iter().map(|a| expr(cx, *a)).collect(),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `try f()`: evaluate, and on failure leave the function with the failure.
            "try_expression" => {
                let inner = all(node).into_iter().find(|c| c.kind() != "try");
                match inner {
                    Some(inner) => Expr::Propagate(Box::new(expr(cx, inner))),
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `catch`, `orelse` and `comptime` are how Zig says what the others
            // say with exceptions and generics, and none of them has a counterpart.
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

mod typescript {
    /// Does this access use `?.`?
    ///
    /// The grammar makes `optional_chain` a child instead of a field, so the only way
    /// to ask is to look. Worth asking: `a?.b` and `a.b` differ where it
    /// matters, and the difference is invisible in the text this reader keeps.
    fn has_optional_chain(node: Node<'_>) -> bool {
        let mut cursor = node.walk();
        let found = node
            .children(&mut cursor)
            .any(|child| child.kind() == "optional_chain");
        found
    }

    use super::*;

    /// A union alias whose members might be this file's own records. Noted during
    /// the walk, settled after it, when every member can be looked up.
    struct UnionAlias {
        at: usize,
        name: String,
        doc: Vec<String>,
        exported: bool,
        members: Vec<String>,
    }

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        // A member a record cannot keep still has to reach the reader.
        let mut carried: Vec<Item> = Vec::new();
        let brands = brand_symbols(cx, root);
        let mut unions: Vec<UnionAlias> = Vec::new();
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
                "ambient_declaration" if declares_brand(cx, node, &brands) => {}
                "type_alias_declaration" => match branded(cx, node, &brands, exported) {
                    Some(nt) => module.items.push(Item::Newtype(nt)),
                    None => {
                        if let Some(members) = union_members(cx, node) {
                            unions.push(UnionAlias {
                                at: module.items.len(),
                                name: cx.field_text(node, "name").unwrap_or_default(),
                                doc: doc_above(cx, child, &["///", "//", "/**", "*/", "*"]),
                                exported,
                                members,
                            });
                        }
                        module.items.push(Item::Unsupported(cx.unsupported(child)));
                    }
                },
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
                    let mut r = record(cx, node, &mut carried);
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
                // The statement is the program: `main();` at the bottom of the file
                // runs it. As an unsupported construct it crossed as a comment, and
                // the translated program parsed, ran and did nothing.
                "expression_statement" => module.items.push(Item::Statement(stmt(cx, node))),
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        settle_record_returns(&mut module);
        settle_builtins(&mut module);
        settle_unions(&mut module, unions);
        // A brand travels with a constructor function bearing its own name; this
        // tool's TypeScript writer emits one. Read back as content it duplicates
        // the newtype, and its lower-case spelling wins over the type's.
        let newtype_names: std::collections::BTreeSet<String> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Newtype(n) => Some(n.name.clone()),
                _ => None,
            })
            .collect();
        module
            .items
            .retain(|item| !matches!(item, Item::Function(f) if newtype_names.contains(&f.name)));
        module.items.extend(carried);
        module
    }

    /// The member names of `type X = A | B | C`, when every member is a bare name.
    fn union_members(cx: &Cx, node: Node<'_>) -> Option<Vec<String>> {
        let value = cx.field(node, "value")?;
        if value.kind() != "union_type" {
            return None;
        }
        fn flatten(cx: &Cx, node: Node<'_>, into: &mut Vec<String>) -> bool {
            match node.kind() {
                "union_type" => cx
                    .children(node)
                    .into_iter()
                    .all(|part| flatten(cx, part, into)),
                "type_identifier" => {
                    into.push(cx.text(node));
                    true
                }
                _ => false,
            }
        }
        let mut members = Vec::new();
        flatten(cx, value, &mut members).then_some(members)
    }

    /// Turn `type X = A | B` into a sum when A and B are this file's own records.
    ///
    /// The discriminated-union idiom: each member is an object type, told apart by a
    /// field holding a distinct literal. The literal field is the union's plumbing
    /// and not a variant's data, so it is stripped; the variant's name carries the
    /// distinction from here on. An alias over anything else, a member declared in
    /// another file, a member with methods, stays carried verbatim.
    fn settle_unions(module: &mut Module, unions: Vec<UnionAlias>) {
        let mut consumed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for alias in unions {
            let members: Option<Vec<Record>> = alias
                .members
                .iter()
                .map(|name| {
                    module.items.iter().find_map(|item| match item {
                        Item::Record(r)
                            if r.name == *name
                                && r.methods.is_empty()
                                && !consumed.contains(name) =>
                        {
                            Some(r.clone())
                        }
                        _ => None,
                    })
                })
                .collect();
            let Some(members) = members else { continue };

            // A field every member declares with a literal type is the discriminator.
            let literal = |field: &Field| {
                matches!(&field.ty, Some(Type::Named { name, .. })
                    if name.starts_with('"') || name.starts_with('\''))
            };
            let discriminators: Vec<String> = members
                .first()
                .map(|first| {
                    first
                        .fields
                        .iter()
                        .filter(|f| literal(f))
                        .filter(|f| {
                            members
                                .iter()
                                .all(|m| m.fields.iter().any(|o| o.name == f.name && literal(o)))
                        })
                        .map(|f| f.name.clone())
                        .collect()
                })
                .unwrap_or_default();

            let variants: Vec<Variant> = members
                .iter()
                .map(|member| Variant {
                    doc: member.doc.clone(),
                    name: member.name.clone(),
                    fields: member
                        .fields
                        .iter()
                        .filter(|f| !discriminators.contains(&f.name))
                        .cloned()
                        .collect(),
                })
                .collect();

            module.items[alias.at] = Item::Sum(Sum {
                doc: alias.doc,
                name: alias.name,
                variants,
                exported: alias.exported,
            });
            consumed.extend(alias.members);
        }
        module
            .items
            .retain(|item| !matches!(item, Item::Record(r) if consumed.contains(&r.name)));
    }

    /// A `switch` whose cases are selected by literals and leave before the
    /// next case, as the shared switch.
    ///
    /// A case that falls through with statements of its own has no slot, since
    /// the IR's arms are disjoint. An empty case stacks its literal onto the
    /// next arm, which is the same construct spelled with fall-through.
    fn ts_switch(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let subject = cx.field(node, "value")?;
        let subject = cx.children(subject).into_iter().next().unwrap_or(subject);
        let body = cx.field(node, "body")?;
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Vec<Stmt> = Vec::new();
        let mut pending: Vec<Expr> = Vec::new();
        for case in cx.children(body) {
            match case.kind() {
                "comment" => {}
                "switch_case" => {
                    let value = cx.field(case, "value")?;
                    if !matches!(value.kind(), "number" | "string" | "true" | "false") {
                        return None;
                    }
                    let statements: Vec<Node> = cx
                        .children(case)
                        .into_iter()
                        .filter(|c| c.id() != value.id())
                        .collect();
                    if statements.is_empty() {
                        pending.push(expr(cx, value));
                        continue;
                    }
                    let (arm_body, leaves) = case_body(cx, &statements);
                    if !leaves {
                        return None;
                    }
                    let mut literals = std::mem::take(&mut pending);
                    literals.push(expr(cx, value));
                    arms.push((literals, arm_body));
                }
                "switch_default" => {
                    if !pending.is_empty() {
                        return None;
                    }
                    let statements: Vec<Node> = cx.children(case);
                    let (body, _) = case_body(cx, &statements);
                    default = body;
                }
                _ => return None,
            }
        }
        if !pending.is_empty() {
            return None;
        }
        Some(Stmt::Switch {
            subject: expr(cx, subject),
            arms,
            default,
        })
    }

    /// The case's statements with the trailing `break` taken off, and whether
    /// the case leaves on its own.
    fn case_body(cx: &Cx, statements: &[Node]) -> (Vec<Stmt>, bool) {
        let mut nodes: Vec<Node> = statements.to_vec();
        let had_break = nodes.last().is_some_and(|n| n.kind() == "break_statement");
        if had_break {
            nodes.pop();
        }
        let body: Vec<Stmt> = nodes.iter().map(|n| stmt(cx, *n)).collect();
        let leaves =
            had_break || matches!(body.last(), Some(Stmt::Return(_)) | Some(Stmt::Throw(_)));
        (body, leaves)
    }

    /// `const { a, b } = e`, lowered to what every target can say.
    ///
    /// One binding for the value, then one per name. Python and Go have no object
    /// pattern; the lowering says what the pattern means, exactly. A pattern using
    /// renames, defaults or nesting stays unsupported, because its lowering is a
    /// different one.
    fn destructured(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        if !matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
            return None;
        }
        let declarator = cx
            .children(node)
            .into_iter()
            .find(|d| d.kind() == "variable_declarator")?;
        let pattern = cx
            .field(declarator, "name")
            .filter(|n| n.kind() == "object_pattern")?;
        let mut names = Vec::new();
        for member in cx.children(pattern) {
            if member.kind() == "shorthand_property_identifier_pattern" {
                names.push(cx.text(member));
            } else {
                return None;
            }
        }
        if names.is_empty() {
            return None;
        }
        let value = cx.field(declarator, "value").map(|v| expr(cx, v))?;
        // One name reads straight through the field; several bind the value once.
        if let [name] = names.as_slice() {
            return Some(vec![Stmt::Let {
                name: name.clone(),
                ty: None,
                value: Some(Expr::Field {
                    of: Box::new(value),
                    name: name.clone(),
                }),
                mutable: false,
            }]);
        }
        let temp = format!("{}_parts", names.join("_"));
        let mut body = vec![Stmt::Let {
            name: temp.clone(),
            ty: None,
            value: Some(value),
            mutable: false,
        }];
        for name in &names {
            body.push(Stmt::Let {
                name: name.clone(),
                ty: None,
                value: Some(Expr::Field {
                    of: Box::new(Expr::Name(temp.clone())),
                    name: name.clone(),
                }),
                mutable: false,
            });
        }
        Some(body)
    }

    /// The names declared as `declare const x: unique symbol` at the top level.
    fn brand_symbols(cx: &Cx, root: Node<'_>) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        for child in cx.children(root) {
            if child.kind() != "ambient_declaration" {
                continue;
            }
            let text = cx.text(child);
            if !text.contains("unique symbol") {
                continue;
            }
            for declaration in cx.children(child) {
                if declaration.kind() != "lexical_declaration" {
                    continue;
                }
                for d in cx.children(declaration) {
                    if d.kind() == "variable_declarator" {
                        if let Some(name) = cx.field_text(d, "name") {
                            out.insert(name);
                        }
                    }
                }
            }
        }
        out
    }

    /// Is this ambient declaration one of the brand symbols an alias consumed?
    fn declares_brand(
        cx: &Cx,
        node: Node<'_>,
        brands: &std::collections::BTreeSet<String>,
    ) -> bool {
        let text = cx.text(node);
        text.contains("unique symbol") && brands.iter().any(|b| text.contains(b.as_str()))
    }

    /// `type Pence = number & { readonly [penceBrand]: true }`, the brand idiom,
    /// read as the distinct type it declares.
    fn branded(
        cx: &Cx,
        node: Node<'_>,
        brands: &std::collections::BTreeSet<String>,
        exported: bool,
    ) -> Option<Newtype> {
        let name = cx.field_text(node, "name")?;
        let value = cx.field(node, "value")?;
        if value.kind() != "intersection_type" {
            return None;
        }
        let parts = cx.children(value);
        let base = parts.iter().find(|p| p.kind() == "predefined_type")?;
        let marker = parts.iter().find(|p| p.kind() == "object_type")?;
        let marker_text = cx.text(*marker);
        if !brands.iter().any(|b| marker_text.contains(b.as_str())) {
            return None;
        }
        Some(Newtype {
            doc: Vec::new(),
            name,
            base: ty(cx, *base),
            exported,
        })
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
            receiver_binding: receiver.as_ref().map(|_| "this".to_string()),
            receiver,
            params,
            returns,
            body: cx
                .field(node, "body")
                .map(|b| block(cx, b))
                .unwrap_or_default(),
            exported: false,
            is_async,
            is_property: false,
            is_constructor: cx.field_text(node, "name").as_deref() == Some("constructor"),
        }
    }

    /// The one type this class extends, if it declares one.
    ///
    /// `class A extends B implements C, D` puts all of them in one clause. Only
    /// `extends` is a base; the rest are contracts, which the other languages spell
    /// differently or not at all.
    fn heritage(cx: &Cx, node: Node<'_>) -> Option<String> {
        let body = cx.field(node, "body")?;
        let clause = cx
            .children(node)
            .into_iter()
            .take_while(|c| c.id() != body.id())
            .find(|c| c.kind() == "class_heritage")?;
        let extends = cx
            .children(clause)
            .into_iter()
            .find(|c| c.kind() == "extends_clause")?;
        cx.children(extends).first().map(|base| cx.text(*base))
    }

    /// A returned object literal is the record the signature promised.
    ///
    /// `summarize(): Summary` returning `{ open, closed, titles }` crossed as a
    /// map, so the Python caller got a dict where the dataclass reader used
    /// attributes. Only a literal with string keys rewrites, every key a field
    /// of the declared record, every undefaulted field present. Anything else
    /// stays the map it is.
    fn settle_record_returns(module: &mut Module) {
        let records: std::collections::BTreeMap<String, (Vec<String>, Vec<String>)> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Record(r) => Some((
                    r.name.clone(),
                    (
                        r.fields.iter().map(|f| f.name.clone()).collect(),
                        r.fields
                            .iter()
                            .filter(|f| f.default.is_none())
                            .map(|f| f.name.clone())
                            .collect(),
                    ),
                )),
                _ => None,
            })
            .collect();
        if records.is_empty() {
            return;
        }
        let mut settle = |f: &mut Function| {
            let Some(Type::Named { name, args }) = f.returns.clone() else {
                return;
            };
            let Some((fields, required)) = records.get(&name).filter(|_| args.is_empty()) else {
                return;
            };
            super::each_stmt_in_stmts(&mut f.body, &mut |stmt| {
                let Stmt::Return(Some(value)) = stmt else {
                    return;
                };
                let Expr::MapLit(entries) = value else {
                    return;
                };
                let keys: Option<Vec<String>> = entries
                    .iter()
                    .map(|(k, _)| match k {
                        Expr::Str(s) | Expr::Name(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                let Some(keys) = keys else { return };
                let shaped = keys.iter().all(|k| fields.contains(k))
                    && required.iter().all(|r| keys.contains(r));
                if !shaped {
                    return;
                }
                let fields_out: Vec<(String, Expr)> = keys
                    .into_iter()
                    .zip(entries.iter().map(|(_, v)| v.clone()))
                    .collect();
                *value = Expr::RecordLit {
                    ty: name.clone(),
                    fields: fields_out,
                };
            });
        };
        let mut items = std::mem::take(&mut module.items);
        for item in items.iter_mut() {
            match item {
                Item::Function(f) => settle(f),
                Item::Record(r) => r.methods.iter_mut().for_each(&mut settle),
                _ => {}
            }
        }
        module.items = items;
    }

    /// The everyday library spellings, rewritten to the table's canonical ones.
    ///
    /// `console.log`, `.push`, `.toUpperCase`, `.trim` and `.length` all have exact
    /// counterparts in every target, and written through unchanged each was a
    /// compile error there. The canonical names are the Python spellings; the
    /// writers turn them back into whatever their language says.
    fn settle_builtins(module: &mut Module) {
        let field_named_length = module.items.iter().any(
            |item| matches!(item, Item::Record(r) if r.fields.iter().any(|f| f.name == "length")),
        );
        super::each_expr_in_module(module, &mut |e| {
            if let Expr::Field { of, name } = e {
                if name == "length" && !field_named_length {
                    let of = of.clone();
                    *e = Expr::Call {
                        callee: Box::new(Expr::Name("len".to_string())),
                        args: vec![*of],
                    };
                    return;
                }
            }
            let Expr::Call { callee, args } = e else {
                return;
            };
            match callee.as_mut() {
                Expr::Field { of, name } => match (of.as_ref(), name.as_str()) {
                    (Expr::Name(n), "log") if n == "console" => {
                        *e = Expr::Call {
                            callee: Box::new(Expr::Name("print".to_string())),
                            args: std::mem::take(args),
                        };
                    }
                    (_, "push") if args.len() == 1 => *name = "append".to_string(),
                    (_, "toUpperCase") if args.is_empty() => *name = "upper".to_string(),
                    (_, "toLowerCase") if args.is_empty() => *name = "lower".to_string(),
                    (_, "trim") if args.is_empty() => *name = "strip".to_string(),
                    // `xs.join(sep)` and the canonical `sep.join(xs)` put the
                    // separator on opposite sides; the swap is the translation.
                    (_, "join") if args.len() == 1 => {
                        let xs = of.clone();
                        let sep = args.pop().expect("one argument");
                        *e = Expr::Call {
                            callee: Box::new(Expr::Field {
                                of: Box::new(sep),
                                name: "join".to_string(),
                            }),
                            args: vec![*xs],
                        };
                    }
                    _ => {}
                },
                Expr::Name(n) if n == "String" && args.len() == 1 => *n = "str".to_string(),
                _ => {}
            }
        });
    }

    /// Is this class member reachable from outside the class?
    ///
    /// A TypeScript member is public unless it says otherwise, which is the opposite
    /// of what a free function does. Reading both the same way made every translated
    /// method private in Java and unreachable in Go, Rust and Zig, and made every
    /// `private` field public, which is the same mistake pointing the other way.
    fn is_visible(cx: &Cx, member: Node<'_>) -> bool {
        !cx.children(member).iter().any(|c| {
            c.kind() == "accessibility_modifier"
                && matches!(cx.text(*c).as_str(), "private" | "protected")
        })
    }

    fn record(cx: &Cx, node: Node<'_>, carried: &mut Vec<Item>) -> Record {
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
                        // `rows: T[] = [];` starts every instance somewhere. Lost,
                        // the dataclass this became required an argument nothing
                        // passes, and construction raised.
                        default: cx.field(member, "value").map(|v| expr(cx, v)),
                        exported: is_visible(cx, member),
                    }),
                    "method_definition" | "method_signature" => {
                        let mut method = function(cx, member, Some(name.clone()));
                        method.exported = is_visible(cx, member);
                        // `get total()` is read as data at its use sites, which is
                        // a fact about every accessor and not about this body.
                        let mut cursor = member.walk();
                        method.is_property =
                            member.children(&mut cursor).any(|c| c.kind() == "get");
                        drop(cursor);
                        // `constructor(public x: number)` declares the field and
                        // assigns it, in the parameter list. Read as a parameter
                        // alone, the class came out with no fields at all.
                        if method.is_constructor {
                            fields.extend(parameter_properties(cx, member));
                        }
                        methods.push(method);
                    }
                    // A member this does not recognise is not a member that is not
                    // there. Every reader here ended its member loop with `_ => {}`,
                    // which is how a `@staticmethod` disappeared from a class while the
                    // report said every signature had carried across intact.
                    _ => carried.push(Item::Unsupported(cx.unsupported(member))),
                }
            }
        }
        Record {
            doc: Vec::new(),
            name,
            fields,
            extends: heritage(cx, node),
            exported: false,
            methods,
        }
    }

    /// The fields a constructor's parameter list declares.
    ///
    /// An accessibility modifier in front of a parameter, `public x: number`,
    /// makes it a class field with that name and type.
    fn parameter_properties(cx: &Cx, constructor: Node<'_>) -> Vec<Field> {
        let Some(parameters) = cx.field(constructor, "parameters") else {
            return Vec::new();
        };
        cx.children(parameters)
            .into_iter()
            .filter(|p| {
                let mut cursor = p.walk();
                let modified = p
                    .children(&mut cursor)
                    .any(|c| c.kind() == "accessibility_modifier");
                modified
            })
            .filter_map(|p| {
                let name = cx.field(p, "pattern").map(|n| cx.text(n))?;
                Some(Field {
                    doc: Vec::new(),
                    name,
                    ty: cx.field(p, "type").map(|t| ty(cx, t)),
                    default: None,
                    exported: is_visible(cx, p),
                })
            })
            .collect()
    }

    fn ty(cx: &Cx, node: Node<'_>) -> Type {
        // A `type_annotation` wraps the type after the colon.
        let inner = cx.children(node).first().copied().unwrap_or(node);
        ty_text(&cx.text(inner))
    }

    /// Resolve a type from its text, recursing through generic arguments.
    ///
    /// The entry point and the recursion are the same function. When they were not,
    /// `Promise<Record<string, string>>` resolved its outer layer and left the inner one as an
    /// opaque name, so a round trip produced `Record[str, str]` in Python.
    fn ty_text(text: &str) -> Type {
        // `readonly string[]` is a `string[]` that says you may not write to it, and no
        // other language here has anywhere to put that. Left on, it made every
        // read-only array in the file an element type this tool could not write.
        let trimmed = text
            .trim()
            .trim_start_matches(':')
            .trim()
            .trim_start_matches("readonly ")
            .trim();
        if let Some(t) = super::scalar(trimmed) {
            return t;
        }
        if let Some(element) = trimmed.strip_suffix("[]") {
            return Type::List(Box::new(named_or_scalar(element)));
        }
        // `[A, B]` between brackets is TypeScript's tuple type, one element included.
        if let Some(inside) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let parts = super::comma_parts(inside);
            if !parts.is_empty() && parts.iter().all(|p| !p.is_empty()) {
                return Type::Tuple(parts.iter().map(|p| named_or_scalar(p)).collect());
            }
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
        let mut out = Vec::new();
        for n in cx.children_with_comments(node) {
            if let Some(lowered) = destructured(cx, n) {
                out.extend(lowered);
                continue;
            }
            out.push(keep_whole(cx, n, stmt(cx, n)));
        }
        out
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
                Some(value) => Stmt::Throw(thrown(expr(cx, *value))),
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "try_statement" => {
                let mut catches = Vec::new();
                let mut finally = Vec::new();
                if let Some(clause) = cx.field(node, "handler") {
                    let binding = cx.field(clause, "parameter").map(|p| cx.text(p));
                    let mut body = cx
                        .field(clause, "body")
                        .map(|b| block(cx, b))
                        .unwrap_or_default();
                    // `e.message` inside the catch is the exception as text, and the
                    // canonical spelling of that is `str(e)`. Scoped to this clause's
                    // own binding: a `.message` on anything else is somebody's field.
                    if let Some(bound) = &binding {
                        super::each_expr_in_stmts(&mut body, &mut |e| {
                            let Expr::Field { of, name } = e else {
                                return;
                            };
                            let ours = name == "message"
                                && matches!(of.as_ref(), Expr::Name(n) if n == bound);
                            if ours {
                                *e = Expr::Call {
                                    callee: Box::new(Expr::Name("str".to_string())),
                                    args: vec![Expr::Name(bound.clone())],
                                };
                            }
                        });
                    }
                    catches.push(Catch {
                        binding,
                        // TypeScript catches everything; there is no type to select on.
                        ty: None,
                        body,
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
                Some(inner) if inner.kind() == "augmented_assignment_expression" => {
                    let target = cx
                        .field(inner, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null);
                    let value = cx
                        .field(inner, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null);
                    let operator = cx.field_text(inner, "operator").unwrap_or_default();
                    match super::desugar_compound(target, &operator, value) {
                        Some(assign) => assign,
                        None => Stmt::Unsupported(cx.unsupported(node)),
                    }
                }
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
            "switch_statement" => match ts_switch(cx, node) {
                Some(switch) => switch,
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
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

    /// A thrown `new Error(...)`, with the class name crossed to the canonical one.
    ///
    /// The canonical names are Python's: `Error` is the general `Exception`,
    /// `RangeError` is the complaint `ValueError` makes, and `TypeError` keeps its
    /// name in both. Written through unchanged, every `raise` in a translated file
    /// named a class the target never declared. A name outside the table is the
    /// program's own and is not touched.
    fn thrown(value: Expr) -> Expr {
        let Expr::New { callee, args } = value else {
            return value;
        };
        let mapped = match callee.as_ref() {
            Expr::Name(name) => match name.as_str() {
                "Error" => Some("Exception"),
                "RangeError" => Some("ValueError"),
                "TypeError" => Some("TypeError"),
                _ => None,
            },
            _ => None,
        };
        match mapped {
            Some(name) => Expr::New {
                callee: Box::new(Expr::Name(name.to_string())),
                args,
            },
            None => Expr::New { callee, args },
        }
    }

    /// One arrow function of one parameter: `(x) => body`, or `x => body`.
    ///
    /// Anything else, a destructured parameter, a block body, a named callback, is
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
        let method = cx.field_text(callee, "property")?;
        if method != "map" && method != "filter" {
            return None;
        }
        let args = cx.children(cx.field(node, "arguments")?);
        if args.len() != 1 {
            return None;
        }

        // A bare `xs.filter(p)` is `[x for x in xs if p(x)]`, the same comprehension
        // with the identity element. Reading only `.map(...)` meant a plain filter,
        // which is the commoner of the two, came out as a comment.
        if method == "filter" {
            let (binding, predicate) = one_arg_arrow(cx, args[0])?;
            return Some(Expr::Comprehension {
                element: Box::new(Expr::Name(binding.clone())),
                binding,
                iterable: Box::new(expr(cx, cx.field(callee, "object")?)),
                condition: Some(Box::new(expr(cx, predicate))),
            });
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
                    // Two different names is two different scopes. It is not one loop.
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
            // `a ? b : c`, the operands are the named children and the `?` and `:`
            // between them are punctuation.
            "ternary_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [condition, then, otherwise] => Expr::Ternary {
                        condition: Box::new(expr(cx, *condition)),
                        then: Box::new(expr(cx, *then)),
                        otherwise: Box::new(expr(cx, *otherwise)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
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
            // `a?.b` is not `a.b`. Neither Python, Rust nor Go has optional chaining. Writing
            // the plain access drops the null check silently, the translation would compile,
            // run, and throw where the original returned undefined. Carried instead.
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
                    // `{ species }` is `{ species: species }`, the shorthand every
                    // modern TypeScript file is written in. Reading it as something
                    // unrecognised refused the whole object, and with it the statement
                    // the object was in.
                    if pair.kind() == "shorthand_property_identifier" {
                        let name = cx.text(pair);
                        entries.push((Expr::Str(name.clone()), Expr::Name(name)));
                        continue;
                    }
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
            // `instanceof` is spelled as an operator here and as a builtin in Python. It is the
            // same question either way, so it is its own node.
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
                let left = || {
                    cx.field(node, "left")
                        .map(|l| expr(cx, l))
                        .unwrap_or(Expr::Null)
                };
                let right = || {
                    cx.field(node, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null)
                };
                let operator = cx.field_text(node, "operator").unwrap_or_default();
                // `a ?? b` asks whether the left side is absent, which is a question instead of
                // an arithmetic operator, half these languages spell it with a word or a
                // method. One cannot spell it at all.
                if operator == "??" {
                    return Expr::Coalesce {
                        value: Box::new(left()),
                        fallback: Box::new(right()),
                    };
                }
                match super::binary_op(&operator) {
                    Some(op) => Expr::Binary {
                        op,
                        left: Box::new(left()),
                        right: Box::new(right()),
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
            // the translation is exact instead of a gap, and leaving them
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
/// Nesting is respected, so `Result<Vec<T>, E>` yields two arguments instead of
/// three. A name with no brackets is itself with no arguments.
/// Visit every expression under these statements, innermost first, mutably.
///
/// The post-passes settle what a reader could not know locally: which calls
/// construct a module's own types, which member reads are properties. All walk
/// the same tree. Each pass writing its own recursion is how one of them misses the
/// statement variant added for the other.
fn each_expr_in_stmts(stmts: &mut [Stmt], visit: &mut dyn FnMut(&mut Expr)) {
    for stmt in stmts {
        match stmt {
            Stmt::Return(value) => {
                if let Some(value) = value {
                    each_expr(value, visit);
                }
            }
            Stmt::Let { value, .. } => {
                if let Some(value) = value {
                    each_expr(value, visit);
                }
            }
            Stmt::Assign { target, value } => {
                each_expr(target, visit);
                each_expr(value, visit);
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                each_expr(condition, visit);
                each_expr_in_stmts(then, visit);
                each_expr_in_stmts(otherwise, visit);
            }
            Stmt::IfPresent {
                value,
                then,
                otherwise,
                ..
            } => {
                each_expr(value, visit);
                each_expr_in_stmts(then, visit);
                each_expr_in_stmts(otherwise, visit);
            }
            Stmt::While { condition, body } => {
                each_expr(condition, visit);
                each_expr_in_stmts(body, visit);
            }
            Stmt::WhilePresent { value, body, .. } => {
                each_expr(value, visit);
                each_expr_in_stmts(body, visit);
            }
            Stmt::ForEach { iterable, body, .. } | Stmt::ForEachIndexed { iterable, body, .. } => {
                each_expr(iterable, visit);
                each_expr_in_stmts(body, visit);
            }
            Stmt::Defer(body) | Stmt::ErrDefer(body) => each_expr_in_stmts(body, visit),
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                each_expr(subject, visit);
                for (literals, body) in arms {
                    for literal in literals {
                        each_expr(literal, visit);
                    }
                    each_expr_in_stmts(body, visit);
                }
                each_expr_in_stmts(default, visit);
            }
            Stmt::Expr(e) | Stmt::Throw(e) => each_expr(e, visit),
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                each_expr_in_stmts(body, visit);
                for catch in catches {
                    each_expr_in_stmts(&mut catch.body, visit);
                }
                each_expr_in_stmts(finally, visit);
            }
            Stmt::Comment(_) | Stmt::Break | Stmt::Continue | Stmt::Unsupported(_) => {}
        }
    }
}

/// Visit every statement under these, containers recursed, mutably.
///
/// The statement-level sibling of [`each_expr_in_stmts`], for the passes that
/// care where an expression stands. A map literal is only a record when a
/// `return` hands it to a signature that promised one.
fn each_stmt_in_stmts(stmts: &mut [Stmt], visit: &mut dyn FnMut(&mut Stmt)) {
    for stmt in stmts {
        visit(stmt);
        match stmt {
            Stmt::If {
                then, otherwise, ..
            }
            | Stmt::IfPresent {
                then, otherwise, ..
            } => {
                each_stmt_in_stmts(then, visit);
                each_stmt_in_stmts(otherwise, visit);
            }
            Stmt::While { body, .. }
            | Stmt::WhilePresent { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachIndexed { body, .. } => each_stmt_in_stmts(body, visit),
            Stmt::Defer(body) | Stmt::ErrDefer(body) => each_stmt_in_stmts(body, visit),
            Stmt::Switch { arms, default, .. } => {
                for (_, body) in arms {
                    each_stmt_in_stmts(body, visit);
                }
                each_stmt_in_stmts(default, visit);
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                each_stmt_in_stmts(body, visit);
                for catch in catches {
                    each_stmt_in_stmts(&mut catch.body, visit);
                }
                each_stmt_in_stmts(finally, visit);
            }
            Stmt::Return(_)
            | Stmt::Let { .. }
            | Stmt::Assign { .. }
            | Stmt::Expr(_)
            | Stmt::Comment(_)
            | Stmt::Throw(_)
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Unsupported(_) => {}
        }
    }
}

/// Children first, then the node itself, so a rewrite sees settled children.
fn each_expr(e: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    match e {
        Expr::Field { of, .. } => each_expr(of, visit),
        Expr::Index { of, index } => {
            each_expr(of, visit);
            each_expr(index, visit);
        }
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            each_expr(callee, visit);
            for arg in args {
                each_expr(arg, visit);
            }
        }
        Expr::Binary { left, right, .. } => {
            each_expr(left, visit);
            each_expr(right, visit);
        }
        Expr::Unary { operand, .. } => each_expr(operand, visit),
        Expr::Await(inner) | Expr::Propagate(inner) => each_expr(inner, visit),
        Expr::Keyword { value, .. } => each_expr(value, visit),
        Expr::Cast { ty, value } => {
            each_expr(ty, visit);
            each_expr(value, visit);
        }
        Expr::InstanceOf { value, ty } => {
            each_expr(value, visit);
            each_expr(ty, visit);
        }
        Expr::RecordLit { fields, .. } => {
            for (_, value) in fields {
                each_expr(value, visit);
            }
        }
        Expr::Coalesce { value, fallback } => {
            each_expr(value, visit);
            each_expr(fallback, visit);
        }
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            each_expr(condition, visit);
            each_expr(then, visit);
            each_expr(otherwise, visit);
        }
        Expr::Tuple(items) | Expr::ListLit(items) => {
            for item in items {
                each_expr(item, visit);
            }
        }
        Expr::MapLit(entries) => {
            for (key, value) in entries {
                each_expr(key, visit);
                each_expr(value, visit);
            }
        }
        Expr::Template(parts) => {
            for part in parts {
                if let TemplatePart::Expr(inner) = part {
                    each_expr(inner, visit);
                }
            }
        }
        Expr::Comprehension {
            element,
            iterable,
            condition,
            ..
        } => {
            each_expr(element, visit);
            each_expr(iterable, visit);
            if let Some(condition) = condition {
                each_expr(condition, visit);
            }
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Name(_)
        | Expr::Unsupported(_) => {}
    }
    visit(e);
}

/// The same walk over everything a module holds.
fn each_expr_in_module(module: &mut Module, visit: &mut dyn FnMut(&mut Expr)) {
    for item in module.items.iter_mut() {
        match item {
            Item::Function(f) => each_expr_in_stmts(&mut f.body, visit),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    each_expr_in_stmts(&mut method.body, visit);
                }
            }
            Item::Constant(c) => each_expr(&mut c.value, visit),
            Item::Test { body, .. } => each_expr_in_stmts(body, visit),
            Item::Statement(stmt) => each_expr_in_stmts(std::slice::from_mut(stmt), visit),
            Item::Newtype(_) | Item::Sum(_) | Item::Import { .. } | Item::Unsupported(_) => {}
        }
    }
}

/// The pieces between top-level commas, nesting respected.
///
/// What the tuple spellings share: Rust and Go put types between `(` and `)`,
/// TypeScript between `[` and `]`, Zig between `struct {` and `}`. Each reader
/// strips its own brackets and splits the inside here.
fn comma_parts(inside: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in inside.chars() {
        match c {
            '<' | '[' | '(' | '{' => {
                depth += 1;
                current.push(c);
            }
            '>' | ']' | ')' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    parts.push(current.trim().to_string());
    parts
}

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

/// The scalar types that mean the same thing in every language here.
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

/// `target op= value` as the statement it abbreviates: `target = target op value`.
///
/// The IR has one assignment and no operator on it, so the operator moves into
/// the value. An operator this does not recognise returns nothing, and the
/// caller carries the statement whole. The alternative was the Go reader
/// quietly turning `total += item` into `total = item`.
fn desugar_compound(target: Expr, operator: &str, value: Expr) -> Option<Stmt> {
    let op = binary_op(operator.trim().trim_end_matches('='))?;
    Some(Stmt::Assign {
        target: target.clone(),
        value: Expr::Binary {
            op,
            left: Box::new(target),
            right: Box::new(value),
        },
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

/// The text of a string literal, without its quotes or prefix. The text of a comment, without
/// whichever marker the source language used.
///
/// The marker is the only thing that differs between them. So stripping it here and letting
/// each writer add its own is the whole of comment translation.
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
    // A `/* ... */` is one node however many lines it spans, and each of its inner
    // lines carries its own ` * ` leader. Leaving those on wrote a JSDoc block into a
    // language that does not have one, with the asterisks still in it.
    body.lines()
        .map(|line| {
            line.trim()
                .strip_prefix("* ")
                .or_else(|| line.trim().strip_prefix("*"))
                .unwrap_or(line.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn unquote(text: &str) -> String {
    // `r"..."`, `b"..."`, `r#"..."#`: a raw literal has no escapes at all, and the
    // backslashes in it are the value. Decoding them would turn a regex into
    // something that no longer matches.
    let prefix: String = text.chars().take_while(|c| c.is_alphabetic()).collect();
    let raw = prefix.contains('r') || prefix.contains('R');
    let t = text
        .trim_start_matches(|c: char| c.is_alphabetic())
        .trim_start_matches('#');
    for quote in ["\"\"\"", "'''", "\"", "'", "`"] {
        if let Some(inner) = t.strip_prefix(quote).and_then(|s| s.strip_suffix(quote)) {
            return match raw {
                true => inner.to_string(),
                false => unescape(inner),
            };
        }
    }
    t.to_string()
}

/// A string literal's **value**, with the escapes read and not carried.
///
/// The IR holds what the string *is*, not how the source spelled it. Carrying the
/// spelling meant every writer escaped the backslash again on the way out, so a string
/// holding a newline crossed as one holding a backslash and an `n`. The output parsed,
/// so nothing caught it; every string with an escape in it came out meaning something
/// else.
///
/// A backslash before anything this does not recognise is kept as written, which is
/// what Python does with `"\d"` and what the others cannot produce at all, since an
/// unknown escape is a compile error in every one of them.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(escape) = chars.next() else {
            out.push('\\');
            break;
        };
        match escape {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '0' => out.push('\0'),
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '\\' | '"' | '\'' | '`' | '$' | '/' => out.push(escape),
            // A line continuation: the newline and the indent after it are not part of
            // the string.
            '\n' => {
                while chars
                    .peek()
                    .is_some_and(|c| c.is_whitespace() && *c != '\n')
                {
                    chars.next();
                }
            }
            // `\xNN` everywhere, `\uXXXX` in Java, TypeScript, Python and Go, and
            // `\u{...}` in Rust, Zig and modern TypeScript. All three name a code point.
            'x' | 'u' | 'U' => {
                let mut digits = String::new();
                if escape == 'u' && chars.peek() == Some(&'{') {
                    chars.next();
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c == '}' {
                            break;
                        }
                        digits.push(c);
                    }
                } else {
                    let width = match escape {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    while digits.len() < width
                        && chars.peek().is_some_and(|c| c.is_ascii_hexdigit())
                    {
                        digits.push(chars.next().expect("peeked"));
                    }
                }
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(c) => out.push(c),
                    // Half of a UTF-16 surrogate pair, or digits that name nothing.
                    // Neither has a character to stand for it, so the spelling stays.
                    None => {
                        out.push('\\');
                        out.push(escape);
                        out.push_str(&digits);
                    }
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}
