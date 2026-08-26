//! Reading a syntax tree into the IR.
//!
//! One reader per language. Each walks the named nodes, recognises the constructs the IR has,
//! and wraps everything else in [`Unsupported`] with the original text and its line. A reader
//! never guesses. It reports an unrecognised node, so a dropped statement is never silent.

use super::ir::*;
use crate::lang::Language;
use crate::span::LineIndex;
use anyhow::{bail, Result};
use tree_sitter::Node;

/// `file_stem` is the source file's own name, and only Zig wants it. The
/// file-as-struct idiom names its type `Self`, and everyone else calls the type by
/// the file's name.
/// One function, read from its own `function_definition` node.
///
/// The module reader hands a decorated definition to `Unsupported`, because a decorator
/// changes behaviour and no target carries the same one. A reader that knows what a
/// particular decorator means reads the function under it with this. `fastapi.rs` knows
/// what `@router.get("/users")` means, and the handler beneath it is ordinary Python.
pub(crate) fn function_at(language: Language, source: &str, node: Node<'_>) -> Result<Function> {
    let lines = LineIndex::new(source);
    let cx = Cx {
        source,
        lines: &lines,
    };
    match language {
        Language::Python => Ok(python::function(&cx, node, None)),
        other => bail!("no reader takes a single {other} function"),
    }
}

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
        Language::Bash => bash::module(&cx, root),
        other => bail!(
            "there is no reader for {other}: translating out of it would mean inventing \
             what its constructs mean."
        ),
    };
    settle_widest_types(&mut module, language);
    settle_called_parameters(&mut module);
    settle_lambda_types(&mut module);
    settle_passed_bindings(&mut module);
    if matches!(language, Language::Java) {
        settle_java_applications(&mut module);
    }
    settle_methods(&mut module);
    // Each language's spelling of the shared builtins, folded to the canonical one the
    // writers' tables spell back out. See `normalize.rs`.
    super::normalize::normalize(&mut module, language);
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
/// end with a call. Without one, a translated program parses, runs and prints nothing.
/// The synthesized statement is that call. The self-running targets drop it again and
/// say so, so the entry crosses every pairing without doubling anywhere.
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

/// The widest type a language has, read back as the nothing it stands for.
///
/// A source that annotates a parameter with nothing is written out with whatever
/// the target's widest type is: `object`, `any`, `unknown`, `Object`, `anytype`.
/// Read again as a type of that name, the annotation the source never wrote
/// came back as one. A round trip gained what it should have preserved. The
/// widest type is how each language spells "the caller decides", and that is
/// what an unannotated parameter said.
fn settle_widest_types(module: &mut Module, language: Language) {
    let widest = match language {
        Language::Python => "object",
        Language::Go => "any",
        Language::Java => "Object",
        Language::TypeScript | Language::Tsx => "unknown",
        Language::Zig => "anytype",
        _ => return,
    };
    fn clear(f: &mut Function, widest: &str) {
        for p in f.params.iter_mut() {
            let names_it = matches!(&p.ty, Some(Type::Named { name, args }) if name == widest && args.is_empty());
            if names_it {
                p.ty = None;
            }
        }
    }
    for item in &mut module.items {
        match item {
            Item::Function(f) => clear(f, widest),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    clear(method, widest);
                }
            }
            _ => {}
        }
    }
}

/// A parameter the body calls holds a function, and this says which one.
///
/// The type is read off the call rather than guessed. Each argument's type
/// comes from the typed parameter passed there. The answer comes from the
/// return the enclosing function declares. Left untyped, Go wrote `f any` and Zig wrote
/// `anytype`, and neither can be called.
fn settle_called_parameters(module: &mut Module) {
    fn calls_of(body: &mut [Stmt], name: &str) -> Vec<Vec<Expr>> {
        let mut found = Vec::new();
        each_expr_in_stmts(body, &mut |e| {
            if let Expr::Call { callee, args } = e {
                if matches!(callee.as_ref(), Expr::Name(n) if n == name) {
                    found.push(args.clone());
                }
            }
        });
        found
    }
    fn settle(f: &mut Function) {
        let known: Vec<(String, Option<Type>)> = f
            .params
            .iter()
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
        let mut body = f.body.clone();
        let answers = f.returns.clone();
        for p in f.params.iter_mut() {
            if p.ty.is_some() {
                continue;
            }
            let calls = calls_of(&mut body, &p.name);
            let Some(first) = calls.first() else { continue };
            // One arity or none: two different call shapes is not one function.
            if calls.iter().any(|c| c.len() != first.len()) {
                continue;
            }
            let Some(answers) = answers.clone() else {
                continue;
            };
            // Every argument has to say what it is. `f(x)` where nothing knows
            // `x` says only that `f` is callable, which is what was already
            // known.
            let mut taken = Vec::new();
            for argument in first {
                let told = match argument {
                    Expr::Name(n) => known
                        .iter()
                        .find(|(name, _)| name == n)
                        .and_then(|(_, ty)| ty.clone()),
                    // `f(f(n))`: the inner call answers what this one answers.
                    Expr::Call { callee, .. }
                        if matches!(callee.as_ref(), Expr::Name(n) if *n == p.name) =>
                    {
                        Some(answers.clone())
                    }
                    _ => None,
                };
                match told {
                    Some(t) => taken.push(t),
                    None => {
                        taken.clear();
                        break;
                    }
                }
            }
            if taken.len() != first.len() {
                continue;
            }
            p.ty = Some(Type::Fn {
                params: taken,
                returns: Box::new(answers),
            });
        }
    }
    for item in &mut module.items {
        match item {
            Item::Function(f) => settle(f),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    settle(method);
                }
            }
            _ => {}
        }
    }
}

/// A binding takes the type of the slot it is passed to.
///
/// `numbers = [4, 5, 6]` says a list of whole numbers. Passed to a parameter
/// declared `number[]`, which TypeScript's one numeric type makes a list of
/// floats, that is the type it holds. Left alone, Go declared `[]int` and
/// refused the call. What each writer spells the literals as is its own
/// business, and the value is untouched: rewriting `4` to `4.0` here changed
/// what Python printed.
fn settle_passed_bindings(module: &mut Module) {
    let declared: std::collections::BTreeMap<String, (Vec<Option<Type>>, Option<Type>)> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((
                f.name.clone(),
                (
                    f.params.iter().map(|p| p.ty.clone()).collect(),
                    f.returns.clone(),
                ),
            )),
            _ => None,
        })
        .collect();
    fn settle(
        f: &mut Function,
        declared: &std::collections::BTreeMap<String, (Vec<Option<Type>>, Option<Type>)>,
    ) {
        // A binding passed to a fractional slot holds fractional values.
        let mut wanted: std::collections::BTreeMap<String, Type> =
            std::collections::BTreeMap::new();
        let mut body = std::mem::take(&mut f.body);
        each_expr_in_stmts(&mut body, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Name(name) = callee.as_ref() else {
                return;
            };
            let Some((params, _)) = declared.get(name) else {
                return;
            };
            for (argument, param) in args.iter().zip(params) {
                let Some(param) = param else { continue };
                if let Expr::Name(bound) = argument {
                    wanted.insert(bound.clone(), param.clone());
                }
            }
        });
        for stmt in body.iter_mut() {
            let Stmt::Let { name, ty, .. } = stmt else {
                continue;
            };
            let Some(asked) = wanted.get(name) else {
                continue;
            };
            if ty.is_none() {
                *ty = Some(asked.clone());
            }
        }
        f.body = body;
    }
    let known = declared.clone();
    for item in &mut module.items {
        match item {
            Item::Function(f) => settle(f, &known),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    settle(method, &known);
                }
            }
            _ => {}
        }
    }
}

/// `f.apply(x)` on a function-typed name is `f(x)`.
///
/// Java has no call syntax for a function value; it calls the interface's one
/// method. Read literally, every target got `f.apply(n)`, a method of something
/// they do not have. The Java writer puts the `.apply` back.
fn settle_java_applications(module: &mut Module) {
    fn settle(f: &mut Function) {
        let holds: std::collections::BTreeSet<String> = f
            .params
            .iter()
            .filter(|p| matches!(p.ty, Some(Type::Fn { .. })))
            .map(|p| p.name.clone())
            .collect();
        if holds.is_empty() {
            return;
        }
        let mut body = std::mem::take(&mut f.body);
        each_expr_in_stmts(&mut body, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Field { of, name } = callee.as_ref() else {
                return;
            };
            if name != "apply" {
                return;
            }
            let Expr::Name(held) = of.as_ref() else {
                return;
            };
            if !holds.contains(held) {
                return;
            }
            *e = Expr::Call {
                callee: Box::new(Expr::Name(held.clone())),
                args: std::mem::take(args),
            };
        });
        f.body = body;
    }
    for item in &mut module.items {
        match item {
            Item::Function(f) => settle(f),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    settle(method);
                }
            }
            _ => {}
        }
    }
}

/// A lambda takes the type of the slot it goes into.
///
/// `lambda n: n + 1` says nothing about `n`. Passed to a parameter declared
/// `func(int) int`, it says everything: that is the only type the call admits.
/// Without this, Go wrote a carried comment where a closure belonged, because a
/// closure there needs every type spelled.
fn settle_lambda_types(module: &mut Module) {
    let mut declared: std::collections::BTreeMap<String, Vec<Option<Type>>> =
        std::collections::BTreeMap::new();
    for item in &module.items {
        if let Item::Function(f) = item {
            declared.insert(
                f.name.clone(),
                f.params.iter().map(|p| p.ty.clone()).collect(),
            );
        }
    }
    /// Give a lambda the shape a function type asks of it.
    fn shape(lambda: &mut Expr, wanted: &Type) {
        let (Expr::Lambda {
            params,
            returns,
            ..
        }, Type::Fn {
            params: taken,
            returns: answers,
        }) = (&mut *lambda, wanted)
        else {
            return;
        };
        if params.len() != taken.len() {
            return;
        }
        for (p, t) in params.iter_mut().zip(taken) {
            if p.ty.is_none() {
                p.ty = Some(t.clone());
            }
        }
        if returns.is_none() {
            *returns = Some((**answers).clone());
        }
    }
    fn settle(f: &mut Function, declared: &std::collections::BTreeMap<String, Vec<Option<Type>>>) {
        // Which local names hold a lambda, so an argument that names one can be
        // followed back to where it was bound.
        let mut wanted: std::collections::BTreeMap<String, Type> =
            std::collections::BTreeMap::new();
        let mut body = std::mem::take(&mut f.body);
        each_expr_in_stmts(&mut body, &mut |e| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Name(name) = callee.as_ref() else {
                return;
            };
            let Some(params) = declared.get(name) else {
                return;
            };
            for (argument, param) in args.iter_mut().zip(params) {
                let Some(param) = param else { continue };
                if !matches!(param, Type::Fn { .. }) {
                    continue;
                }
                match argument {
                    Expr::Lambda { .. } => shape(argument, param),
                    Expr::Name(bound) => {
                        wanted.insert(bound.clone(), param.clone());
                    }
                    _ => {}
                }
            }
        });
        for stmt in body.iter_mut() {
            let Stmt::Let { name, ty, value, .. } = stmt else {
                continue;
            };
            let Some(asked) = wanted.get(name) else {
                continue;
            };
            if let Some(value) = value {
                if matches!(value, Expr::Lambda { .. }) {
                    shape(value, asked);
                    if ty.is_none() {
                        *ty = Some(asked.clone());
                    }
                }
            }
        }
        f.body = body;
    }
    let known = declared.clone();
    for item in &mut module.items {
        match item {
            Item::Function(f) => settle(f, &known),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    settle(method, &known);
                }
            }
            _ => {}
        }
    }
}

/// Put every method with the type it belongs to, and bind the receiver of any that has nowhere
/// to go.
///
/// Rust and Go declare methods apart from their type; Python, TypeScript, Java and Zig
/// declare them inside it. The IR keeps them with the type, so one shape can become the
/// other. A method left at the top level comes out as a free function. Its body still reaches
/// through a receiver that nothing in the output binds. A Python `def label(prefix)` whose
/// body says `self.name`.
///
/// A method whose type is not in this file, an `impl` on somebody else's struct, has no record
/// to join. Its receiver becomes an ordinary first parameter, which Go and Zig write
/// anyway and Python's `self` has always been.
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
    /// **Comments are not.** Every one of these grammars makes a comment an *extra*. It can
    /// appear between any two nodes anywhere in the tree. Look inside a parameter list,
    /// between two struct fields, or in the middle of an argument list. Every reader here
    /// reads named children positionally or through a catch-all arm, and both read a comment
    /// as whatever they expected in that position. A comment inside a Rust parameter list
    /// becomes four invented parameters that every target writes into the signature.
    ///
    /// This filter runs here, once, rather than in the twenty places that would each have to
    /// remember. [`Cx::children_with_comments`] asks for comments by name.
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

/// The assertion `x!` applied where it belongs: the rightmost operand.
///
/// The grammar gives a `non_null_expression` the whole expression to its left,
/// so an assertion on the last term of a sum arrives wrapping the sum. The
/// operator is postfix and binds tighter than any binary one. So the assertion
/// travels down the right spine to the term it was written on.
fn assert_the_last_operand(e: Expr) -> Expr {
    match e {
        Expr::Binary { op, left, right } => Expr::Binary {
            op,
            left,
            right: Box::new(assert_the_last_operand(*right)),
        },
        other => Expr::Unary {
            op: UnaryOp::Unwrap,
            operand: Box::new(other),
        },
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
/// The six grammars spell it three ways: `comment`, `line_comment`, `block_comment`.
/// Rust adds `inner_doc_comment_marker`. Matching on the substring rather than the
/// list stops a seventh language arriving with a fourth spelling and reading as a
/// parameter.
fn is_comment(node: Node<'_>) -> bool {
    node.kind().contains("comment")
}

/// A call whose *callee* could not be translated is not a call this understands.
///
/// Rendering it as `None()` gives the target something that parses and means nothing.
/// `HashMap::new()` became that. Carrying the whole call instead puts the original in
/// front of whoever finishes the file.
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
    // Exhaustive on purpose, no `_` arm. A missed variant gives a silent wrong answer
    // instead of a gap. `session?.user.id` inside an object literal came out as
    // `None.id`, with the original nowhere in the file. The compiler asks about every
    // variant added later.
    fn bad(e: &Expr) -> bool {
        match e {
            Expr::Unsupported(_) => true,
            Expr::Field { of, .. } => bad(of),
            Expr::Index { of, index } => bad(of) || bad(index),
            Expr::Call { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::Binary { left, right, .. } => bad(left) || bad(right),
            Expr::SetLit(items) => items.iter().any(bad),
            Expr::Unary { operand, .. } => bad(operand),
            Expr::Await(inner) | Expr::Propagate(inner) => bad(inner),
            Expr::New { callee, args } => bad(callee) || args.iter().any(bad),
            Expr::RecordLit { fields, .. } => fields.iter().any(|(_, value)| bad(value)),
            Expr::Variant { fields, .. } => fields.iter().any(|(_, value)| bad(value)),
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
            Expr::Lambda { body, .. } => bad(body),
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
        Stmt::Assert { condition, message } => bad(condition) || message.as_ref().is_some_and(bad),
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
/// Rendering the understood half and a placeholder for the rest produces lines like
/// `sums = None`. Those parse and lie, with the original nowhere in the file. Carrying
/// the whole statement instead puts the source in front of whoever finishes the draft.
fn keep_whole(cx: &Cx, node: Node<'_>, built: Stmt) -> Stmt {
    if binds_a_pattern(&built) {
        return Stmt::Unsupported(cx.unsupported(node));
    }
    // A binding whose initializer failed *as a whole* keeps its name: the marker
    // stands alone as the value and composes with nothing. Carried whole, the
    // declaration vanishes into a comment while every later statement still reads
    // the name. An initializer with a failure *inside* it still carries whole,
    // because a marker spliced mid-expression reads as an operand and gives `None.id`.
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
/// pattern text through produces `for _, (sensor, mean) := range …`, which Go cannot
/// parse. Even where it parses it says the wrong thing, so a destructuring carries
/// whole.
fn binds_a_pattern(stmt: &Stmt) -> bool {
    let plain = |name: &str| {
        !name.is_empty()
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !name.chars().next().is_some_and(|c| c.is_numeric())
    };
    match stmt {
        Stmt::ForEach { binding, .. } => !plain(binding),
        Stmt::ForEachIndexed { index, binding, .. } => !plain(index) || !plain(binding),
        Stmt::Let { name, .. } => !plain(name),
        Stmt::TupleAssign { names, .. } => !names.iter().all(|n| plain(n)),
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
        // carries its own ` * ` leader. A writer expects one entry per line and puts the
        // target's marker on each. A single entry with newlines gets a marker on its first
        // line only, leaving the rest of a paragraph in the file as code.
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
                    target: None,
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
        super::settle_variants(&mut module);
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
                // A view of the same text is the text, everywhere but here.
                ("as_str" | "as_ref", []) => *e = of.as_ref().clone(),
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
        // `vec![a, b]` is the list literal every target spells.
        if name == "vec" {
            let tokens = cx
                .children(node)
                .into_iter()
                .find(|c| c.kind() == "token_tree")?;
            let mut cursor = tokens.walk();
            let children: Vec<Node> = tokens.children(&mut cursor).collect();
            let inner = children.get(1..children.len().saturating_sub(1))?;
            // A macro body is tokens, not a tree: `"a".to_string()` arrives as
            // four of them and no expression. Each element between the commas is
            // read by parsing its own text, which is how it would have been read
            // anywhere else in the file.
            let mut items = Vec::new();
            let mut group: Vec<Node> = Vec::new();
            let mut take = |group: &mut Vec<Node>| -> Option<()> {
                match group.as_slice() {
                    [] => {}
                    [only] => items.push(expr(cx, *only)),
                    [first, .., last] => {
                        let text = &cx.source[first.start_byte()..last.end_byte()];
                        items.push(super::reparsed(text, Language::Rust)?);
                    }
                }
                group.clear();
                Some(())
            };
            for child in inner.iter() {
                match child.kind() {
                    "," => take(&mut group)?,
                    _ if child.is_named() => group.push(*child),
                    _ => {}
                }
            }
            take(&mut group)?;
            return Some(Expr::ListLit(items));
        }
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
            let read = match group.as_slice() {
                [] => return None,
                // One node is an expression already parsed; a run of several is an expression
                // the macro kept as loose tokens. Its source text is right there between the
                // first and last of them. And parsing that text asks the real parser instead of
                // guessing at precedence.
                [only] if only.is_named() && !only.kind().contains("comment") => expr(cx, *only),
                [only] => {
                    let _ = only;
                    return None;
                }
                run => {
                    let start = run.first()?.start_byte();
                    let end = run.last()?.end_byte();
                    reparse_expression(&cx.source[start..end])?
                }
            };
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

    /// A run of macro tokens, parsed as the expression its text spells.
    ///
    /// The macro's grammar keeps arguments as loose tokens; their source text is a
    /// whole expression, and the parser that reads every other expression reads it.
    fn reparse_expression(text: &str) -> Option<Expr> {
        let wrapped = format!("fn __fr_reparse() {{ ({text}); }}");
        let parsed = crate::parse::Parsers::new()
            .parse(crate::lang::Language::Rust, &wrapped)
            .ok()?;
        if parsed.has_errors() {
            return None;
        }
        let lines = LineIndex::new(&wrapped);
        let cx = Cx {
            source: &wrapped,
            lines: &lines,
        };
        let mut found = None;
        let mut stack = vec![parsed.root()];
        while let Some(node) = stack.pop() {
            if node.kind() == "parenthesized_expression" {
                found = cx.children(node).into_iter().find(|c| c.is_named());
                break;
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        let inner = found?;
        let read = expr(&cx, inner);
        match read {
            Expr::Unsupported(_) => None,
            other => Some(other),
        }
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

    /// `assert!`, `assert_eq!` and `assert_ne!`, read as the checks they are.
    ///
    /// A macro's arguments arrive as raw tokens. The everyday forms, one or two
    /// expressions and an optional literal message, are what tests are made of,
    /// and those cross as [`Stmt::Assert`]. A format message with arguments, or
    /// an argument deeper than the shapes below, stays carried whole.
    /// Rebuilding an expression out of loose tokens would be guessing at
    /// precedence the parser never decided.
    fn assert_macro(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let name = cx.field_text(node, "macro")?;
        if !matches!(name.as_str(), "assert" | "assert_eq" | "assert_ne") {
            return None;
        }
        let tokens = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "token_tree")?;
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
        // A trailing comma leaves an empty last group; it is punctuation and not
        // an argument.
        if groups.last().is_some_and(Vec::is_empty) {
            groups.pop();
        }
        let one = |group: &[Node]| -> Option<Expr> {
            match group {
                [only] if only.is_named() && !only.kind().contains("comment") => {
                    let read = expr(cx, *only);
                    (!matches!(read, Expr::Unsupported(_))).then_some(read)
                }
                // One operator between two operands: `total >= 0` is the shape
                // nearly every assert condition has. It is also the one shape
                // loose tokens spell without a precedence to guess. The operator may
                // arrive as several punctuation tokens, `>` then `=`, and the
                // pieces joined are the operator written.
                [left, middle @ .., right]
                    if left.is_named()
                        && right.is_named()
                        && !middle.is_empty()
                        && middle.iter().all(|t| !t.is_named()) =>
                {
                    let operator: String = middle.iter().map(|t| cx.text(*t)).collect();
                    let op = super::binary_op(&operator)?;
                    let left = expr(cx, *left);
                    let right = expr(cx, *right);
                    let clean = !matches!(left, Expr::Unsupported(_))
                        && !matches!(right, Expr::Unsupported(_));
                    clean.then(|| Expr::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    })
                }
                _ => None,
            }
        };
        let (condition, rest) = match name.as_str() {
            "assert" => (one(groups.first()?)?, &groups[1..]),
            _ => {
                let left = one(groups.first()?)?;
                let right = one(groups.get(1)?)?;
                let op = match name.as_str() {
                    "assert_eq" => BinaryOp::Eq,
                    _ => BinaryOp::Ne,
                };
                (
                    Expr::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    &groups[2..],
                )
            }
        };
        let message = match rest {
            [] => None,
            [only] => {
                let [literal] = only.as_slice() else {
                    return None;
                };
                if literal.kind() != "string_literal" {
                    return None;
                }
                Some(Expr::Str(super::unquote(&cx.text(*literal))))
            }
            _ => return None,
        };
        Some(Stmt::Assert { condition, message })
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
        // `match f(x) { Ok(v) => ..., Err(e) => ... }` is this language's try/catch,
        // and the IR's `Try` is what every other language spells it with.
        if let Some(handled) = match_result(cx, subject, body) {
            return Some(handled);
        }
        let as_return = node.parent().is_some_and(|p| {
            p.kind() == "expression_statement"
                && p.next_named_sibling().is_none()
                && !cx.text(p).trim_end().ends_with(';')
        });
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut variant_arms: Vec<(VariantPattern, Vec<Stmt>)> = Vec::new();
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
            match literal_patterns(cx, pattern) {
                Some(literals) => arms.push((literals, arm_body)),
                None => {
                    variant_arms.push((variant_pattern(cx, pattern)?, arm_body));
                }
            }
        }
        // Literal arms and variant arms are two different statements; a match
        // that mixes them carries whole.
        match (arms.is_empty(), variant_arms.is_empty()) {
            (false, true) => Some(Stmt::Switch {
                subject: expr(cx, subject),
                arms,
                default,
            }),
            (true, false) => {
                let mut owners = variant_arms.iter().map(|((sum, _, _), _)| sum.clone());
                let sum = owners.next()?;
                if owners.any(|other| other != sum) {
                    return None;
                }
                let built = variant_arms
                    .into_iter()
                    .map(|((_, variant, bindings), body)| VariantArm {
                        variant,
                        bindings,
                        body,
                    })
                    .collect();
                Some(Stmt::MatchVariants {
                    subject: expr(cx, subject),
                    sum,
                    arms: built,
                    default,
                })
            }
            _ => None,
        }
    }

    /// A match over `Ok`/`Err`, as the try/catch it spells.
    ///
    /// The success arm's binding becomes an ordinary `let` of the call, and the
    /// failure arm becomes the catch. Order is free and either side may be `_`.
    fn match_result(cx: &Cx, subject: Node<'_>, body: Node<'_>) -> Option<Stmt> {
        let mut ok: Option<(String, Vec<Stmt>)> = None;
        let mut err: Option<(String, Vec<Stmt>)> = None;
        for arm in cx.children(body) {
            if arm.kind() != "match_arm" {
                continue;
            }
            if cx.field(arm, "condition").is_some() {
                return None;
            }
            let pattern = cx.field(arm, "pattern")?;
            let text = cx.text(pattern);
            let (name, binding) = text
                .trim()
                .split_once('(')
                .map(|(n, rest)| (n.trim(), rest.trim_end_matches(')').trim()))?;
            if !binding.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            let value = cx.field(arm, "value")?;
            let arm_body = if value.kind() == "block" {
                block(cx, value)
            } else {
                vec![stmt(cx, value)]
            };
            match name {
                "Ok" => ok = Some((binding.to_string(), arm_body)),
                "Err" => err = Some((binding.to_string(), arm_body)),
                _ => return None,
            }
        }
        let (ok, err) = (ok?, err?);
        let mut tried = Vec::new();
        let value = expr(cx, subject);
        match ok.0.as_str() {
            "_" => tried.push(Stmt::Expr(value)),
            bound => tried.push(Stmt::Let {
                name: bound.to_string(),
                ty: None,
                value: Some(value),
                mutable: false,
            }),
        }
        tried.extend(ok.1);
        Some(Stmt::Try {
            body: tried,
            catches: vec![Catch {
                binding: match err.0.as_str() {
                    "_" => None,
                    bound => Some(bound.to_string()),
                },
                ty: None,
                body: err.1,
            }],
            finally: Vec::new(),
            source: cx.text(subject),
            line: cx.line(subject),
        })
    }

    /// A variant pattern taken apart: the sum, the variant, and the payload
    /// fields the arm binds as (field, local).
    type VariantPattern = (String, String, Vec<(String, String)>);

    /// A pattern selecting one variant: `Shape::Point`, `Shape::Circle { radius }`,
    /// `Shape::Circle { radius: r, .. }`. A tuple pattern has no field names to
    /// bind and stays a carry.
    ///
    /// The sum's name, the variant's, and the payload fields the arm binds as
    /// (field, local).
    fn variant_pattern(cx: &Cx, pattern: Node<'_>) -> Option<VariantPattern> {
        let scoped = |node: Node<'_>| -> Option<(String, String)> {
            if !matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier") {
                return None;
            }
            let text = cx.text(node);
            let (head, tail) = text.rsplit_once("::")?;
            let head = head.rsplit("::").next().unwrap_or(head);
            Some((head.to_string(), tail.to_string()))
        };
        // The grammar wraps each arm's pattern in a `match_pattern` node.
        let pattern = match pattern.kind() {
            "match_pattern" => cx
                .children(pattern)
                .into_iter()
                .find(|c| c.is_named())
                .unwrap_or(pattern),
            _ => pattern,
        };
        match pattern.kind() {
            "scoped_identifier" => {
                let (sum, variant) = scoped(pattern)?;
                Some((sum, variant, Vec::new()))
            }
            "struct_pattern" => {
                let ty = cx.field(pattern, "type")?;
                let (sum, variant) = scoped(ty)?;
                let mut bindings = Vec::new();
                for field in cx.children(pattern) {
                    if field.id() == ty.id() {
                        continue;
                    }
                    match field.kind() {
                        "field_pattern" => {
                            let mut named = cx.children(field).into_iter().filter(|c| c.is_named());
                            let name = cx.text(named.next()?);
                            let local = named
                                .next()
                                .map(|n| cx.text(n))
                                .unwrap_or_else(|| name.clone());
                            // A nested pattern in the binding slot is more than
                            // a rename and carries the whole match.
                            if !local.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                return None;
                            }
                            bindings.push((name, local));
                        }
                        "remaining_field_pattern" => {}
                        _ if !field.is_named() => {}
                        _ => return None,
                    }
                }
                Some((sum, variant, bindings))
            }
            _ => None,
        }
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
        // A type this function declares for itself says the caller decides what
        // it is, which is what a source that annotated nothing said. Read as a
        // type name, it crossed to the other languages as a type they had
        // never heard of. A round trip gained a type the source lacked.
        let decides: std::collections::BTreeSet<String> = cx
            .field(node, "type_parameters")
            .map(|list| {
                cx.children(list)
                    .into_iter()
                    .flat_map(|p| cx.children(p))
                    .filter(|c| c.kind() == "type_identifier")
                    .map(|c| cx.text(c))
                    .collect()
            })
            .unwrap_or_default();
        let mut params = Vec::new();
        let mut receiver_name = None;
        if let Some(list) = cx.field(node, "parameters") {
            for p in cx.children(list) {
                match p.kind() {
                    // `&self` carries the receiver, which the IR records separately.
                    "self_parameter" => receiver_name = Some("self".to_string()),
                    "parameter" => params.push(Param {
                        name: plain(cx.field_text(p, "pattern").unwrap_or_default()),
                        ty: cx
                            .field(p, "type")
                            .filter(|t| !decides.contains(cx.text(*t).trim()))
                            .map(|t| ty(cx, t)),
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
            is_private: false,
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
                tag: None,
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

    /// `Fn(A) -> R` in any of the wrappers Rust puts around it.
    fn callable(text: &str) -> Option<Type> {
        let mut bare = text.trim();
        // A boxed one is still one; the box says where it lives.
        for outer in ["Box<", "std::boxed::Box<", "Rc<", "Arc<"] {
            if let Some(inner) = bare.strip_prefix(outer).and_then(|s| s.strip_suffix('>')) {
                bare = inner.trim();
            }
        }
        let bare = bare
            .trim_start_matches('&')
            .trim_start()
            .trim_start_matches("impl ")
            .trim_start_matches("dyn ")
            .trim_start();
        let after = ["Fn", "FnMut", "FnOnce", "fn"]
            .iter()
            .find_map(|word| bare.strip_prefix(word))
            .filter(|rest| rest.trim_start().starts_with('('))?;
        let (inside, rest) = super::parenthesised(after.trim_start())?;
        let params = super::parameter_types(&inside, ty_text);
        // `Fn(A)` with nothing after it returns nothing.
        let returns = match rest.trim().strip_prefix("->") {
            Some(answer) => ty_text(answer),
            None if rest.trim().is_empty() => Type::Unit,
            None => return None,
        };
        Some(Type::Fn {
            params,
            returns: Box::new(returns),
        })
    }

    /// A Rust type from its text.
    ///
    /// The reference comes off **first**. `&HashMap<K, V>` is a `HashMap`. Checking the
    /// containers before stripping the `&` read every map, list and option passed by
    /// reference as a plain name. Rust passes most of them that way.
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

        // `impl Fn(i64) -> i64`, `fn(i64) -> i64`, `Box<dyn Fn(i64) -> i64>`: the
        // several ways Rust names a callable. Every one of them is the same
        // question to the other five languages, and the answer is their own
        // function type.
        if let Some(built) = callable(bare) {
            return built;
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
            ("HashSet" | "BTreeSet", [inner]) => Type::Set(Box::new(ty_text(inner))),
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
                    | "loop_expression"
                    | "break_expression"
                    | "continue_expression"
                    | "assignment_expression"
                    | "compound_assignment_expr"
                    | "match_expression" => stmt(cx, *inner),
                    "macro_invocation" => match assert_macro(cx, *inner) {
                        Some(check) => check,
                        None => Stmt::Expr(expr(cx, *inner)),
                    },
                    _ => Stmt::Expr(expr(cx, *inner)),
                },
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "assignment_expression" => {
                let target = cx
                    .field(node, "left")
                    .map(|l| expr(cx, l))
                    .unwrap_or(Expr::Null);
                match target.is_assignable() {
                    false => Stmt::Unsupported(cx.unsupported(node)),
                    true => Stmt::Assign {
                        target,
                        value: cx
                            .field(node, "right")
                            .map(|r| expr(cx, r))
                            .unwrap_or(Expr::Null),
                    },
                }
            }
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
                if !target.is_assignable() {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
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
                | "struct_expression"
                | "scoped_identifier"
                // A tail expression may be a cast, a reference, a closure or a
                // parenthesised expression. Left off, a body ending in one had
                // no value and the whole function crossed as a comment.
                | "type_cast_expression"
                | "reference_expression"
                | "closure_expression"
                | "parenthesized_expression"
                | "array_expression"
                | "tuple_expression"
                | "self"
        )
    }

    /// The receiver, name and arguments of `x.f(…)`, turbofish or not.
    fn method_call<'t>(cx: &Cx, node: Node<'t>) -> Option<(Node<'t>, String, Vec<Node<'t>>)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let mut callee = cx.field(node, "function")?;
        if callee.kind() == "generic_function" {
            callee = cx.field(callee, "function")?;
        }
        if callee.kind() != "field_expression" {
            return None;
        }
        let name = cx.field_text(callee, "field")?;
        let receiver = cx.field(callee, "value")?;
        let args = cx
            .field(node, "arguments")
            .map(|a| cx.children(a))
            .unwrap_or_default();
        Some((receiver, name, args))
    }

    /// The parameter and body of a one-argument closure, `|x| e`.
    fn one_argument_closure<'t>(cx: &Cx, node: Node<'t>) -> Option<(String, Node<'t>)> {
        if node.kind() != "closure_expression" {
            return None;
        }
        let parameters = cx.children(cx.field(node, "parameters")?);
        let [only] = parameters.as_slice() else {
            return None;
        };
        // `|&n|` and `|n|` bind the same element; the pattern says how it is
        // taken, and no target has a way to take it differently.
        let bound = cx.text(*only);
        let name = bound.trim_start_matches(['&', '*']).to_string();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        Some((name, cx.field(node, "body")?))
    }

    /// `xs.iter().filter(p).map(f).collect()`, the comprehension Rust spells as a chain.
    fn chain(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        let (mut receiver, method, args) = method_call(cx, node)?;
        if method != "collect" || !args.is_empty() {
            return None;
        }
        let mut element: Option<(String, Node<'_>)> = None;
        let mut condition: Option<(String, Node<'_>)> = None;
        while let Some((inner, name, args)) = method_call(cx, receiver) {
            match (name.as_str(), args.as_slice()) {
                ("map", [only]) if element.is_none() => {
                    element = Some(one_argument_closure(cx, *only)?)
                }
                ("filter", [only]) if condition.is_none() => {
                    condition = Some(one_argument_closure(cx, *only)?)
                }
                // These say how the elements are handed over, which is a
                // question only Rust asks. The elements are the same either way.
                ("iter" | "into_iter" | "cloned" | "copied", []) => {}
                // Anything else in the chain does something this shape cannot
                // say. Reading the rest and dropping it would lose it silently.
                _ => return None,
            }
            receiver = inner;
        }
        let (binding, element) = match (element, &condition) {
            (Some(pair), _) => pair,
            // A bare `.filter(p).collect()` keeps the element it tested.
            (None, Some((bound, _))) => (bound.clone(), receiver),
        (None, None) => return None,
        };
        let condition = match condition {
            // Two names is two scopes, and this shape holds one.
            Some((bound, _)) if bound != binding => return None,
            Some((_, predicate)) => Some(Box::new(expr(cx, predicate))),
            None => None,
        };
        let element = match element.id() == receiver.id() {
            true => Expr::Name(binding.clone()),
            false => expr(cx, element),
        };
        Some(Expr::Comprehension {
            element: Box::new(element),
            binding,
            iterable: Box::new(expr(cx, receiver)),
            condition,
        })
    }

    pub(super) fn expr(cx: &Cx, node: Node<'_>) -> Expr {
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
            // A borrow is Rust bookkeeping; the value is what crosses.
            "reference_expression" => cx
                .children(node)
                .into_iter()
                .find(|c| c.is_named() && !c.kind().contains("comment"))
                .map(|inner| expr(cx, inner))
                .unwrap_or(Expr::Null),
            // `()` is the unit value, and the IR calls that a tuple with nothing in
            // it. Left unread, `Ok(())` carried the whole statement around it.
            "unit_expression" => Expr::Tuple(Vec::new()),
            // `[3, 1, 2]` is the list literal every target spells.
            "array_expression" => Expr::ListLit(
                cx.children(node)
                    .iter()
                    .filter(|c| c.is_named() && !c.kind().contains("comment"))
                    .map(|n| expr(cx, *n))
                    .collect(),
            ),
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
            "call_expression" => {
                // `xs.iter().filter(p).map(f).collect()` is a comprehension
                // written the way Rust writes one. Read as an ordinary call, it
                // named `iter`, `map` and `collect`, none of which the target
                // has. The whole binding came out as a comment.
                if let Some(built) = chain(cx, node) {
                    return built;
                }
                // `Vec::new()` and `String::new()` build the empty values every
                // target spells as literals.
                let callee_text = cx.field(node, "function").map(|f| cx.text(f));
                match callee_text.as_deref() {
                    Some("Vec::new") => return Expr::ListLit(Vec::new()),
                    Some(
                        "HashSet::new"
                        | "BTreeSet::new"
                        | "std::collections::HashSet::new"
                        | "std::collections::BTreeSet::new",
                    ) => return Expr::SetLit(Vec::new()),
                    Some("String::new") => return Expr::Str(String::new()),
                    _ => {}
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
            // `Shape::Point` read as a value: a variant candidate, kept only where
            // the settle pass finds the sum among this module's own.
            "scoped_identifier" => match cx.text(node).rsplit_once("::") {
                Some((head, tail)) => Expr::Variant {
                    sum: head.to_string(),
                    name: tail.to_string(),
                    fields: Vec::new(),
                },
                None => Expr::Name(cx.text(node)),
            },
            // `Counter { value: 0, step }`, the one way Rust builds a record, and the
            // line every constructor is made of. Nothing read it, so every constructor
            // body in every target came out as "not translated".
            "struct_expression" => {
                let ty = cx
                    .field(node, "name")
                    .map(|n| cx.text(n))
                    .unwrap_or_default();
                // `StopReason::Conditional { … }` builds an enum variant, and it
                // reads as one here. The settle pass at the end of the module
                // keeps it only where the head names one of this module's own
                // sums. Anything else goes back to being carried, as it always
                // was.
                if let Some((head, tail)) = ty.rsplit_once("::") {
                    let mut fields = Vec::new();
                    if let Some(body) = cx.field(node, "body") {
                        for initialiser in cx.children(body) {
                            if initialiser.kind() == "field_initializer" {
                                let name = cx.field_text(initialiser, "field").unwrap_or_default();
                                let value = cx
                                    .field(initialiser, "value")
                                    .map(|v| expr(cx, v))
                                    .unwrap_or(Expr::Null);
                                fields.push((name, value));
                            }
                        }
                    }
                    return Expr::Variant {
                        sum: head.to_string(),
                        name: tail.to_string(),
                        fields,
                    };
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
                    // `*x` reads the value behind a reference. No language here
                    // has references to read behind, so the value is what
                    // crosses. Left unread, every borrowed operand in a body
                    // came out as a comment.
                    (None, Some(inner)) if text.starts_with('*') => expr(cx, *inner),
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `items.len() as i64` converts between two of Rust's integer
            // widths. No other language here has widths to convert between, so
            // the value crosses and the cast does not. Unread, every length in
            // a signature that declared one came out as a comment.
            "type_cast_expression" => match cx.field(node, "value") {
                Some(value) => expr(cx, value),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `|x| e` and `|x: T| e`, the one-expression closure. A block body is
            // a function that wants a name and stays carried.
            "closure_expression" => {
                let params: Option<Vec<Param>> = cx
                    .field(node, "parameters")
                    .map(|list| {
                        cx.children(list)
                            .into_iter()
                            .map(|p| match p.kind() {
                                "identifier" => {
                                    Some(super::lambda_param(plain(cx.text(p)), None))
                                }
                                // `|n: i64|`: the grammar gives the name and the
                                // type as siblings under the parameter.
                                "parameter" => {
                                    let name = cx.field(p, "pattern").filter(|n| {
                                        n.kind() == "identifier"
                                    })?;
                                    Some(super::lambda_param(
                                        plain(cx.text(name)),
                                        cx.field(p, "type").map(|t| ty(cx, t)),
                                    ))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| Some(Vec::new()));
                match (params, cx.field(node, "body")) {
                    (Some(params), Some(body)) => {
                        let value = match body.kind() {
                            "block" => super::only_returned(&block(cx, body)),
                            _ => Some(expr(cx, body)),
                        };
                        match value {
                            Some(value) => Expr::Lambda {
                                params,
                                returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
                                body: Box::new(value),
                            },
                            None => Expr::Unsupported(cx.unsupported(node)),
                        }
                    }
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
                    let text = cx.text(child);
                    let target = import_target(&text);
                    module.items.push(Item::Import {
                        text,
                        line: cx.line(child),
                        target,
                    })
                }
                "function_definition" => {
                    module.items.push(Item::Function(function(cx, child, None)))
                }
                "class_definition" => {
                    let record = record(cx, child, &mut carried);
                    module.items.push(Item::Record(record));
                }
                // `@dataclass class User:` is the typed-Python idiom for a record.
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
                            // One entry per line. A writer puts its comment
                            // marker in front of each entry. An entry holding
                            // embedded newlines came out with the marker on
                            // its first line, raw prose after, and a file no
                            // target parses.
                            module.doc.extend(
                                super::unquote(&cx.text(*n))
                                    .lines()
                                    .map(|l| l.trim_end().to_string()),
                            );
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
                        tag: None,
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

    pub(super) fn function(cx: &Cx, node: Node<'_>, receiver: Option<String>) -> Function {
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
            // Python's convention, which is all there is to go on. A dunder is
            // not that convention. `__init__` is how the language spells a
            // public constructor, and reading its underscores as "private" left
            // every translated class unconstructible.
            exported: is_exported_python_name(&cx.field_text(node, "name").unwrap_or_default()),
            is_async: cx.text(node).starts_with("async "),
            is_property: false,
            is_constructor: cx.field_text(node, "name").as_deref() == Some("__init__"),
            is_private: false,
        }
    }

    /// Turn every re-binding into an assignment.
    ///
    /// Python has no declaration keyword, so `x = 1` declares the first time and assigns
    /// every time after. Reading all of them as declarations produced `let total = total +
    /// x;` inside a Rust loop. That shadows rather than accumulates, so the value outside the
    /// loop never changed. Nothing downstream can catch that: it parses, it type-checks, and
    /// it is the wrong program.
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
                // A name already bound makes the whole statement an assignment.
                // The targets that need the distinction cannot declare some of
                // the names and assign the rest in one line either.
                Stmt::TupleAssign {
                    names, declares, ..
                } => {
                    *declares = names.iter().all(|n| !bound.contains(n));
                    bound.extend(names.iter().cloned());
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    rebindings(then, bound);
                    rebindings(otherwise, bound);
                }
                Stmt::While { body, .. } => rebindings(body, bound),
                Stmt::CountedFor {
                    init, update, body, ..
                } => {
                    for header in [init, update].iter_mut().flat_map(|h| h.as_deref_mut()) {
                        rebindings(std::slice::from_mut(header), bound);
                    }
                    rebindings(body, bound);
                }
                Stmt::ForEach { binding, body, .. } => {
                    bound.insert(binding.clone());
                    rebindings(body, bound);
                }
                Stmt::ForEachIndexed {
                    index,
                    binding,
                    body,
                    ..
                } => {
                    bound.insert(index.clone());
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
        let mut annotated: Vec<(String, Type)> = Vec::new();
        if let Some(body) = cx.field(node, "body") {
            for item in cx.children(body) {
                match item.kind() {
                    "function_definition" => {
                        if cx.field_text(item, "name").as_deref() == Some("__init__") {
                            annotated.extend(annotated_self_fields(cx, item));
                        }
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
                    // A member this does not recognise is not a member that is not there.
                    // Every one of these readers ended its member loop with `_ => {}`. A
                    // `@staticmethod` disappeared from a class that way, while the report
                    // still said every signature had carried across intact. A record has no
                    // room for a construct it cannot translate, so it goes beside the type.
                    _ => carried.push(Item::Unsupported(cx.unsupported(item))),
                }
            }
        }
        let bases: Vec<String> = cx
            .field(node, "superclasses")
            .map(|list| cx.children(list))
            .unwrap_or_default()
            .into_iter()
            .filter(|b| b.is_named() && b.kind() != "comment")
            .map(|b| cx.text(b))
            .collect();
        let mut record = Record {
            doc: docstring(cx, cx.field(node, "body")),
            name,
            fields,
            // `class A(B, C):`, the bases are the class's argument list. One
            // base slot exists in the targets that inherit at all, and the
            // first base is the one `super()` dispatches to, so it rides. The
            // rest are said beside the type. Dropping every base because there
            // were two left `super.cost()` in a class extending nothing, which
            // is not a program in any of them.
            extends: bases.first().cloned(),
            exported: true,
            methods,
        };
        if bases.len() > 1 {
            record.doc.push(format!(
                "the source also declares `{}` as a base; one base is all that carries.",
                bases[1..].join(", ")
            ));
        }
        derive_constructor_shape(&mut record);
        // The annotations `__init__` wrote on its own field assignments. The
        // derived field takes the type the source spelled out, which the value
        // alone could not say.
        for (field_name, field_ty) in annotated {
            if let Some(field) = record
                .fields
                .iter_mut()
                .find(|f| f.name == field_name && f.ty.is_none())
            {
                field.ty = Some(field_ty);
            }
        }
        record
    }

    /// The types `__init__` writes on its own field assignments.
    ///
    /// `self.entries: list[str] = []` declares the field and its type at once.
    /// The assignment crosses as a plain one, and the annotation would vanish
    /// with it. Read as a binding instead, its dotted "name" was no name at
    /// all. The whole field assignment then carried as a comment, deleting the
    /// field.
    fn annotated_self_fields(cx: &Cx, function_node: Node<'_>) -> Vec<(String, Type)> {
        let Some(body) = cx.field(function_node, "body") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for statement in cx.children(body) {
            if statement.kind() != "expression_statement" {
                continue;
            }
            let Some(inner) = cx.children(statement).first().copied() else {
                continue;
            };
            if inner.kind() != "assignment" {
                continue;
            }
            let (Some(target), Some(ty_node)) = (cx.field(inner, "left"), cx.field(inner, "type"))
            else {
                continue;
            };
            // Only fields of the receiver: an annotated write into some other
            // object declares nothing here.
            if target.kind() != "attribute"
                || cx.field(target, "object").map(|o| cx.text(o)).as_deref() != Some("self")
            {
                continue;
            }
            let Some(field_name) = cx.field_text(target, "attribute") else {
                continue;
            };
            out.push((field_name, ty(cx, ty_node)));
        }
        out
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
        super::promote_constructions(module, &types);
        settle_variant_constructions(module);
    }

    /// A class consumed into a sum is constructed the same way. The call
    /// becomes that variant, keyword arguments as its fields and positional
    /// ones matched against the variant's declared order.
    fn settle_variant_constructions(module: &mut Module) {
        let variants: std::collections::BTreeMap<String, (String, Vec<String>)> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Sum(s) => Some(s.variants.iter().map(|v| {
                    (
                        v.name.clone(),
                        (
                            s.name.clone(),
                            v.fields.iter().map(|f| f.name.clone()).collect(),
                        ),
                    )
                })),
                _ => None,
            })
            .flatten()
            .collect();
        if variants.is_empty() {
            return;
        }
        // A carried construct that binds one of these names shadows it for
        // the whole function. A nested `def Card(...)` means `Card(number)`
        // calls the local, whatever the module's sums say. The carried source
        // is the nested definition's only trace, so it is the thing read.
        let shadowed_in = |body: &[Stmt], name: &str| -> bool {
            let mut found = false;
            let mut probe = body.to_vec();
            super::each_stmt_in_stmts(&mut probe, &mut |stmt| {
                let carried = match stmt {
                    Stmt::Unsupported(u) => Some(u),
                    Stmt::Expr(Expr::Unsupported(u)) => Some(u),
                    _ => None,
                };
                if let Some(u) = carried {
                    let def = format!("def {name}(");
                    let bind = format!("{name} =");
                    if u.source.contains(&def) || u.source.starts_with(&bind) {
                        found = true;
                    }
                }
            });
            found
        };
        for item in &mut module.items {
            let Item::Function(f) = item else { continue };
            let shadowed: Vec<String> = variants
                .keys()
                .filter(|name| shadowed_in(&f.body, name))
                .cloned()
                .collect();
            if shadowed.is_empty() {
                continue;
            }
            super::each_expr_in_stmts(&mut f.body, &mut |e| {
                if let Expr::Call { callee, .. } = e {
                    if matches!(callee.as_ref(), Expr::Name(n) if shadowed.contains(n)) {
                        let Expr::Call { callee, args } = e else {
                            unreachable!("just matched");
                        };
                        let source = format!(
                            "{}({} argument(s))",
                            match callee.as_ref() {
                                Expr::Name(n) => n.clone(),
                                _ => String::new(),
                            },
                            args.len()
                        );
                        *e = Expr::Unsupported(Unsupported {
                            construct: "a call to a shadowed name".to_string(),
                            source,
                            line: 0,
                        });
                    }
                }
            });
        }
        super::each_expr_in_module(module, &mut |e| {
            if let Expr::Call { callee, args } = e {
                if let Expr::Name(n) = callee.as_ref() {
                    if let Some((sum, declared)) = variants.get(n) {
                        let name = n.clone();
                        let taken = std::mem::take(args);
                        let mut fields = Vec::new();
                        let mut position = 0usize;
                        for arg in taken {
                            match arg {
                                Expr::Keyword { name, value } => fields.push((name, *value)),
                                other => {
                                    let field = declared
                                        .get(position)
                                        .cloned()
                                        .unwrap_or_else(|| "value".to_string());
                                    position += 1;
                                    fields.push((field, other));
                                }
                            }
                        }
                        *e = Expr::Variant {
                            sum: sum.clone(),
                            name,
                            fields,
                        };
                    }
                }
            }
        });
    }

    /// The pieces of an import line, where the line has the named form.
    ///
    /// `from m import a, b as c` yields the module and the names, and a plain
    /// `import m` yields the module alone. Forms a sweep cannot rewrite,
    /// `import a, b`, `import m as n` and `from m import *`, yield `None` and
    /// travel as text.
    pub(super) fn import_target(text: &str) -> Option<ImportTarget> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix("from ") {
            let (module, names) = rest.split_once(" import ")?;
            let module = module.trim().to_string();
            let list = names.trim().trim_start_matches('(').trim_end_matches(')');
            if list.contains('*') {
                return None;
            }
            let relative = module.starts_with('.');
            return Some(ImportTarget {
                module,
                relative,
                names: super::import_names(list, " as ")?,
                resolved: None,
            });
        }
        let module = text.strip_prefix("import ")?.trim();
        if module.contains(',') || module.contains(char::is_whitespace) {
            return None;
        }
        Some(ImportTarget {
            module: module.to_string(),
            relative: module.starts_with('.'),
            names: Vec::new(),
            resolved: None,
        })
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

    /// Whether Python's naming convention calls this name part of the surface.
    ///
    /// A leading underscore marks a name as internal, and a name wrapped in two
    /// on each side is a protocol method the language itself calls.
    fn is_exported_python_name(name: &str) -> bool {
        if name.starts_with("__") && name.ends_with("__") && name.len() > 4 {
            return true;
        }
        !name.starts_with('_')
    }

    fn annotated_field(cx: &Cx, node: Node<'_>) -> Option<Field> {
        let name = cx.field_text(node, "left")?;
        Some(Field {
            doc: Vec::new(),
            name: name.clone(),
            ty: cx.field(node, "type").map(|t| ty(cx, t)),
            // `retries: int = 3` starts the field at 3. Dropped, the field was
            // undefined at run time in every target that has the syntax.
            default: cx.field(node, "right").and_then(|v| field_default(cx, v)),
            exported: !name.starts_with('_'),
        })
    }

    /// The value a field starts at, with any declaration wrapper taken off.
    ///
    /// `field(default_factory=list)` is how a dataclass says "a new empty list
    /// per instance", which is a plain `[]` everywhere else. Read literally,
    /// `field` crossed as a call to a function no target declares.
    ///
    /// `Field(min_length=8)` and `Relationship(...)` declare the field rather
    /// than start it. Only their `default` and `default_factory` are values.
    fn field_default(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        /// The helpers that declare a field instead of giving it a value:
        /// `dataclasses.field`, and pydantic's and SQLModel's own two.
        const DECLARING: &[&str] = &["field", "Field", "Relationship"];

        let read = expr(cx, node);
        let Expr::Call { callee, args } = &read else {
            return Some(read);
        };
        if !matches!(callee.as_ref(), Expr::Name(n) if DECLARING.contains(&n.as_str())) {
            return Some(read);
        }
        let mut default = None;
        for arg in args {
            let Expr::Keyword { name, value } = arg else {
                continue;
            };
            match name.as_str() {
                "default" => default = Some((**value).clone()),
                "default_factory" => {
                    default = Some(match value.as_ref() {
                        Expr::Name(factory) if factory == "list" => Expr::ListLit(Vec::new()),
                        Expr::Name(factory) if factory == "dict" => Expr::MapLit(Vec::new()),
                        // `default_factory=lambda: [1, 2]` builds that value.
                        Expr::Lambda { params, body, .. } if params.is_empty() => (**body).clone(),
                        // Any other factory is called once per instance, and
                        // the other languages write a call in that slot too.
                        other => Expr::Call {
                            callee: Box::new(other.clone()),
                            args: Vec::new(),
                        },
                    })
                }
                _ => {}
            }
        }
        default
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
        // lower case. Shouting the name of `schema = z.object(...)` would read wrong. Every
        // one of those was then lost on the way home. Two rules were deciding one thing and
        // disagreeing.
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
        // `Callable[[int], int]`, the one function type Python spells.
        for prefix in ["Callable[", "typing.Callable[", "collections.abc.Callable["] {
            let Some(inside) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(']'))
            else {
                continue;
            };
            let parts = super::comma_parts(inside);
            let [taken, answer] = parts.as_slice() else {
                continue;
            };
            let Some(taken) = taken.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            else {
                continue;
            };
            return Type::Fn {
                params: super::comma_parts(taken)
                    .iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| ty_text(p))
                    .collect(),
                returns: Box::new(ty_text(answer)),
            };
        }
        for prefix in ["set[", "Set[", "frozenset["] {
            if let Some(inner) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(']'))
            {
                return Type::Set(Box::new(ty_text(inner)));
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
            // A bare `...` is Python's stub body, the whole of an abstract
            // method. Skipped, a body of nothing but one takes each writer's own
            // empty-body path, which says the stub out loud and still compiles.
            if child.kind() == "expression_statement" {
                if let Some(inner) = cx.children(*child).first() {
                    if inner.kind() == "ellipsis" {
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
            // `assert c, "m"`: the condition first, the message after the comma.
            "assert_statement" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [condition] => Stmt::Assert {
                        condition: expr(cx, *condition),
                        message: None,
                    },
                    [condition, message] => Stmt::Assert {
                        condition: expr(cx, *condition),
                        message: Some(expr(cx, *message)),
                    },
                    _ => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
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
                    // `a, b = b, a` settles two names at once, and read as one
                    // target it carried whole: the swap never happened.
                    if let Some(left) = cx.field(*inner, "left") {
                        if matches!(left.kind(), "pattern_list" | "tuple_pattern") {
                            return tuple_assign(cx, node, *inner, left);
                        }
                    }
                    let target = cx.field(*inner, "left");
                    let value = cx.field(*inner, "right").map(|v| expr(cx, v));
                    // An annotated assignment to a bare name is a binding with a type.
                    // `self.entries: list[str] = []` is annotated too and is not a
                    // binding. Read as one, its dotted "name" was no name at all,
                    // and the whole field assignment carried as a comment.
                    if is_new_name(cx, *inner) {
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
                // A `...` that reaches statement handling outside a body's own
                // walk, under the entry guard, is still only Python's `pass`.
                Some(inner) if inner.kind() == "ellipsis" => Stmt::Expr(Expr::Null),
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
            // `match x:` with literal cases is the value dispatch every target has.
            // A destructuring pattern is a different thing and carries whole.
            "match_statement" => {
                let subject = cx
                    .field(node, "subject")
                    .map(|s| expr(cx, s))
                    .unwrap_or(Expr::Null);
                let Some(body) = cx.field(node, "body") else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
                let mut default: Vec<Stmt> = Vec::new();
                for clause in cx.children(body) {
                    if clause.kind() != "case_clause" {
                        continue;
                    }
                    let patterns: Vec<Node> = cx
                        .children(clause)
                        .into_iter()
                        .filter(|c| c.kind() == "case_pattern")
                        .collect();
                    let consequence = cx
                        .field(clause, "consequence")
                        .map(|b| block(cx, b))
                        .unwrap_or_default();
                    let texts: Vec<String> = patterns
                        .iter()
                        .map(|p| cx.text(*p).trim().to_string())
                        .collect();
                    if texts.iter().any(|t| t == "_") {
                        default = consequence;
                        continue;
                    }
                    let mut literals = Vec::new();
                    for pattern in &patterns {
                        let inner = cx
                            .children(*pattern)
                            .into_iter()
                            .find(|c| c.is_named())
                            .unwrap_or(*pattern);
                        let read = expr(cx, inner);
                        match read {
                            Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => {
                                literals.push(read)
                            }
                            _ => return Stmt::Unsupported(cx.unsupported(node)),
                        }
                    }
                    arms.push((literals, consequence));
                }
                Stmt::Switch {
                    subject,
                    arms,
                    default,
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
    /// `a, b = b, a`: several names settled at once, when all of them are plain.
    ///
    /// Whether these names are new is settled afterwards by [`rebindings`],
    /// which is where Python's one rule about that already lives.
    fn tuple_assign(cx: &Cx, statement: Node<'_>, assignment: Node<'_>, left: Node<'_>) -> Stmt {
        let names = cx.children(left);
        if names.is_empty() || !names.iter().all(|n| n.kind() == "identifier") {
            return Stmt::Unsupported(cx.unsupported(statement));
        }
        let value = match cx.field(assignment, "right") {
            Some(right) if matches!(right.kind(), "expression_list") => {
                Expr::Tuple(cx.children(right).iter().map(|n| expr(cx, *n)).collect())
            }
            Some(right) => expr(cx, right),
            None => Expr::Null,
        };
        if has_unsupported_expr(&Stmt::Expr(value.clone())) {
            return Stmt::Unsupported(cx.unsupported(statement));
        }
        Stmt::TupleAssign {
            names: names.iter().map(|n| cx.text(*n)).collect(),
            value,
            declares: true,
            source: cx.text(statement),
            line: cx.line(statement),
        }
    }

    fn is_new_name(cx: &Cx, assignment: Node<'_>) -> bool {
        cx.field(assignment, "left")
            .map(|l| l.kind() == "identifier")
            .unwrap_or(false)
    }

    /// Is this node the bare `super()` call that reaches the base class?
    ///
    /// `super().__init__(args)` and `super().m(args)` are how Python reaches it.
    /// Read literally, the inner call crossed as a call to a function named
    /// `super`, which no target declares. The canonical shapes are
    /// `Call(Name("super"), args)` for the base constructor and
    /// `Call(Field(Name("super"), m), args)` for a base method. Each writer
    /// spells them its own way.
    fn reaches_the_base(cx: &Cx, node: Node<'_>) -> bool {
        node.kind() == "call"
            && cx.field_text(node, "function").as_deref() == Some("super")
            && cx
                .field(node, "arguments")
                .is_some_and(|a| cx.children(a).is_empty())
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
                // An f-string interpolates. Dropping the braces would turn `f"{c} below the
                // floor"` into the literal text `{c} below the floor`. The fragment travels
                // carried, so the output states a gap rather than a wrong answer.
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
            "attribute" => {
                let name = cx.field_text(node, "attribute").unwrap_or_default();
                // `super().m` reaches the base class; the inner call is the reach
                // itself and not a call to anything named `super`.
                if cx
                    .field(node, "object")
                    .is_some_and(|o| reaches_the_base(cx, o))
                {
                    return Expr::Field {
                        of: Box::new(Expr::Name("super".to_string())),
                        name,
                    };
                }
                Expr::Field {
                    of: Box::new(
                        cx.field(node, "object")
                            .map(|o| expr(cx, o))
                            .unwrap_or(Expr::Null),
                    ),
                    name,
                }
            }
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
            "call" => {
                let callee = cx
                    .field(node, "function")
                    .map(|f| expr(cx, f))
                    .unwrap_or(Expr::Null);
                let args: Vec<Expr> = cx
                    .field(node, "arguments")
                    .map(|a| cx.children(a).iter().map(|n| expr(cx, *n)).collect())
                    .unwrap_or_default();
                // `super().__init__(args)` calls the base constructor. The
                // `__init__` is Python's word for one and not the IR's, so the
                // canonical form is the call to `super` itself.
                if let Expr::Field { of, name } = &callee {
                    if name == "__init__" && matches!(of.as_ref(), Expr::Name(n) if n == "super") {
                        return Expr::Call {
                            callee: Box::new(Expr::Name("super".to_string())),
                            args,
                        };
                    }
                }
                call_or_carry(cx, node, callee, args)
            }
            // `lambda x: e`, the one-expression function. A default, a splat or a
            // pattern in the parameter list is more than the shared shape and
            // carries whole.
            "lambda" => {
                let params: Option<Vec<Param>> = cx
                    .field(node, "parameters")
                    .map(|list| {
                        cx.children(list)
                            .into_iter()
                            .map(|p| {
                                (p.kind() == "identifier")
                                    .then(|| super::lambda_param(cx.text(p), None))
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| Some(Vec::new()));
                match (params, cx.field(node, "body")) {
                    (Some(params), Some(body)) => Expr::Lambda {
                        params,
                        returns: None,
                        body: Box::new(expr(cx, body)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
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
                // `is not` and `not in` are two tokens. Reading only the first turned `x is
                // not None` into `x == None`. Reading only the first says the opposite of the
                // input.
                let mut cursor = node.walk();
                let operator: String = node
                    .children(&mut cursor)
                    .filter(|c| !c.is_named())
                    .map(|c| cx.text(c))
                    .collect::<Vec<_>>()
                    .join(" ");
                // Python's `/` yields a float whatever it divides, and C's `/`
                // truncates. One spelling, two operations, and reading both as
                // the same one made `cents / 100` an integer division in every
                // target whose `/` is C's.
                // `needle in hay` and `not in` are the containment every target
                // spells as a method.
                if matches!(operator.trim(), "in" | "not in") {
                    let contains = Expr::Call {
                        callee: Box::new(Expr::Field {
                            of: Box::new(
                                cx.field(node, "right")
                                    .or_else(|| {
                                        node.child(node.child_count().saturating_sub(1) as u32)
                                    })
                                    .map(|r| expr(cx, r))
                                    .unwrap_or(Expr::Null),
                            ),
                            name: "contains".to_string(),
                        }),
                        args: vec![cx
                            .field(node, "left")
                            .or_else(|| node.child(0))
                            .map(|l| expr(cx, l))
                            .unwrap_or(Expr::Null)],
                    };
                    return match operator.trim() {
                        "in" => contains,
                        _ => Expr::Unary {
                            op: UnaryOp::Not,
                            operand: Box::new(contains),
                        },
                    };
                }
                let op = super::binary_op(&operator).map(|op| match op {
                    BinaryOp::Div => BinaryOp::TrueDiv,
                    other => other,
                });
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

mod go {
    use super::*;

    /// `func NewX() X { return X{F: v} }` is how a Go record keeps its field defaults: the
    /// lowering this tool writes. And an idiom on its own. Read back, the values return to the
    /// fields and the constructor disappears, so a record with defaults survives the round
    /// trip.
    fn settle_default_constructors(module: &mut Module) {
        let mut defaults: Vec<(String, Vec<(String, Expr)>)> = Vec::new();
        module.items.retain(|item| {
            let Item::Function(m) = item else {
                return true;
            };
            if !m.is_constructor || !m.params.is_empty() {
                return true;
            }
            let Some(owner) = m.receiver.clone() else {
                return true;
            };
            let [Stmt::Return(Some(value))] = m.body.as_slice() else {
                return true;
            };
            let pairs: Option<Vec<(String, Expr)>> = match value {
                Expr::New { callee, args }
                    if matches!(callee.as_ref(), Expr::Name(n) if *n == owner) =>
                {
                    args.iter()
                        .map(|a| match a {
                            Expr::Keyword { name, value } => {
                                Some((name.clone(), (**value).clone()))
                            }
                            _ => None,
                        })
                        .collect()
                }
                Expr::RecordLit { ty, fields } if *ty == owner => Some(fields.clone()),
                // At this point in the read the literal is still a variant candidate. The
                // settle that would rename it to the record runs later, so the candidate's own
                // shape is matched here.
                Expr::Variant { sum, name, fields } if sum.is_empty() && *name == owner => {
                    Some(fields.clone())
                }
                _ => None,
            };
            match pairs {
                Some(pairs) => {
                    defaults.push((owner, pairs));
                    false
                }
                None => true,
            }
        });
        for (owner, pairs) in defaults {
            let record = module.items.iter_mut().find_map(|item| match item {
                Item::Record(r) if r.name == owner => Some(r),
                _ => None,
            });
            if let Some(record) = record {
                for (name, value) in pairs {
                    if let Some(field) = record.fields.iter_mut().find(|f| f.name == name) {
                        field.default = Some(value);
                    }
                }
            }
        }
    }

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
                    target: None,
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
        settle_default_constructors(&mut module);
        settle_sums(&mut module);
        settle_builtins(&mut module);
        settle_variants(&mut module);
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
            // A member named in a concrete position keeps its struct beside
            // the variant. A function returning `Point` cannot return a
            // variant of `Shape`. Consuming the struct outright rewrote its
            // values while the signature kept the type, which no target
            // accepts. The sum still forms, and a construction of a
            // dual-named type settles by the position it stands in.
            fn concrete(ty: &Type, out: &mut Vec<String>) {
                match ty {
                    Type::Named { name, args } => {
                        out.push(name.clone());
                        for arg in args {
                            concrete(arg, out);
                        }
                    }
                    Type::List(t) | Type::Optional(t) => concrete(t, out),
                    Type::Map(k, v) => {
                        concrete(k, out);
                        concrete(v, out);
                    }
                    Type::Tuple(parts) => parts.iter().for_each(|t| concrete(t, out)),
                    _ => {}
                }
            }
            let concretely_used: std::collections::BTreeSet<String> = {
                let mut named = Vec::new();
                for item in &module.items {
                    match item {
                        Item::Function(f) => {
                            for ty in f.params.iter().filter_map(|p| p.ty.as_ref()) {
                                concrete(ty, &mut named);
                            }
                            if let Some(ty) = &f.returns {
                                concrete(ty, &mut named);
                            }
                        }
                        Item::Record(r) => {
                            for ty in r.fields.iter().filter_map(|f| f.ty.as_ref()) {
                                concrete(ty, &mut named);
                            }
                        }
                        _ => {}
                    }
                }
                named
                    .into_iter()
                    .filter(|n| members.iter().any(|m| &m.name == n))
                    .collect()
            };
            let variants: Vec<Variant> = members
                .iter()
                .map(|member| Variant {
                    doc: member.doc.clone(),
                    name: member.name.clone(),
                    tag: None,
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
            consumed.extend(
                members
                    .into_iter()
                    .map(|m| m.name)
                    .filter(|name| !concretely_used.contains(name)),
            );
            // A member kept beside its variant sheds the marker method. The
            // variant carries the membership now, and the marker written back
            // out would come home as a function the source never had.
            for item in &mut module.items {
                if let Item::Record(r) = item {
                    if concretely_used.contains(&r.name) {
                        r.methods.retain(|m| m.name != marker);
                    }
                }
            }
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
            is_private: false,
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
                // A map whose values carry nothing is a set: membership is all
                // it can answer, and `map[T]struct{}` is how Go spells one.
                if value.trim() == "struct{}" {
                    return Type::Set(Box::new(ty_text(key)));
                }
                return Type::Map(Box::new(ty_text(key)), Box::new(ty_text(value)));
            }
        }
        // `func(int) int`, Go's function type. A pointer check must not come
        // first: `func(*T) T` starts with neither.
        if let Some(after) = trimmed.strip_prefix("func") {
            if let Some((inside, rest)) = super::parenthesised(after.trim_start()) {
                let params = super::parameter_types(&inside, ty_text);
                let returns = match rest.trim().is_empty() {
                    true => Type::Unit,
                    false => ty_text(&rest),
                };
                return Type::Fn {
                    params,
                    returns: Box::new(returns),
                };
            }
        }
        // A pointer is Go's way of saying a value may be absent.
        if let Some(inner) = trimmed.strip_prefix('*') {
            return Type::Optional(Box::new(ty_text(inner)));
        }
        named_with_args(trimmed, &ty_text)
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        // tree-sitter-go puts a `statement_list` between a block and its statements, so a
        // block's only child is that wrapper. Reading the children directly gave one unknown
        // node, and carried *every Go function body ever translated* into the output as a
        // single comment. The round-trip tests never saw it, because a body that is entirely
        // a comment still parses.
        let children = cx.children_with_comments(node);
        let statements = match children.as_slice() {
            [only] if only.kind() == "statement_list" => cx.children_with_comments(*only),
            _ => children,
        };
        let mut out: Vec<Stmt> = Vec::new();
        let mut hoisted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for node in statements {
            let mut produced = stmts(cx, node);
            // A hoisted header stands where the branch stood, and two sibling
            // branches may bind the same names. Each was its own scope in Go;
            // here they share one, so the second settles the names again.
            if produced.len() > 1 {
                if let Some(header) = produced.first_mut() {
                    settle_again(header, &mut hoisted);
                }
            }
            out.append(&mut produced);
        }
        out
    }

    /// Turn a hoisted header that re-binds names already hoisted into a plain
    /// assignment, and remember the names either way.
    fn settle_again(stmt: &mut Stmt, seen: &mut std::collections::BTreeSet<String>) {
        match stmt {
            Stmt::TupleAssign {
                names, declares, ..
            } => {
                if !names.is_empty() && names.iter().all(|n| seen.contains(n)) {
                    *declares = false;
                }
                seen.extend(names.iter().cloned());
            }
            Stmt::Let { name, value, .. } if seen.contains(name) => {
                let target = Expr::Name(name.clone());
                let value = value.take().unwrap_or(Expr::Null);
                *stmt = Stmt::Assign { target, value };
            }
            Stmt::Let { name, .. } => {
                seen.insert(name.clone());
            }
            _ => {}
        }
    }

    /// One source statement as the statements it becomes.
    ///
    /// Go's `if` may run a statement in its header, and the IR's branch has no
    /// room for one. `if m, ok := t.Min(); ok { }` dropped the whole header
    /// with nothing said, so the branch tested a name the output never bound.
    /// The header goes on the line before instead. That widens the scope of
    /// what it binds, and every target here already scopes it that way.
    fn stmts(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        let header = match node.kind() {
            "if_statement" => cx.field(node, "initializer"),
            _ => None,
        };
        let branch = keep_whole(cx, node, stmt(cx, node));
        match header {
            Some(init) => vec![keep_whole(cx, init, stmt(cx, init)), branch],
            None => vec![branch],
        }
    }

    /// An initializer with a failure anywhere inside it, carried as one whole.
    ///
    /// `ch := make(chan int, 4)` read its callee and lost the channel type, so
    /// the whole statement carried. Every later use of `ch` then named a
    /// binding the output never declared. Collapsing the failed initializer to
    /// a single carried value keeps the declaration: [`keep_whole`]'s rule for
    /// a `Let` whose value failed as a whole.
    fn whole_or_read(cx: &Cx, node: Node<'_>, read: Expr) -> Expr {
        match has_unsupported_expr(&Stmt::Expr(read.clone())) {
            true => Expr::Unsupported(cx.unsupported(node)),
            false => read,
        }
    }

    /// `a, b := f()` and `a, b = b, a`: several names settled at once.
    ///
    /// Only plain names on the left. A target with an index or a field in it is
    /// a shape the IR does not hold, and it carries whole.
    fn tuple_assign(cx: &Cx, node: Node<'_>, names: &[Node<'_>], declares: bool) -> Stmt {
        if !names.iter().all(|n| n.kind() == "identifier") {
            return Stmt::Unsupported(cx.unsupported(node));
        }
        let value = match cx.field(node, "right") {
            Some(right) => match cx.children(right).as_slice() {
                [only] => expr(cx, *only),
                several => Expr::Tuple(several.iter().map(|n| expr(cx, *n)).collect()),
            },
            None => Expr::Null,
        };
        if has_unsupported_expr(&Stmt::Expr(value.clone())) {
            return Stmt::Unsupported(cx.unsupported(node));
        }
        let bound: Vec<String> = names.iter().map(|n| cx.text(*n)).collect();
        // `_, ok := m[k]` is the membership question, and Go has no other way
        // to ask it. The pair it binds means nothing to any other target.
        if let Some((present, asks)) = super::comma_ok_membership(&bound, &value) {
            return Stmt::Let {
                name: present,
                ty: Some(Type::Bool),
                value: Some(asks),
                mutable: false,
            };
        }
        Stmt::TupleAssign {
            names: bound,
            value,
            declares,
            source: cx.text(node),
            line: cx.line(node),
        }
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
            // `return x` wraps its value in an `expression_list`, the same shape that hid
            // every function body. `return a, b` is Go's multiple return, and it crosses as a
            // tuple. Mapping it to nothing turns a two-value return into a bare `return`.
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
            "short_var_declaration" => {
                let left = cx.field(node, "left");
                let names: Vec<Node> = left.map(|l| cx.children(l)).unwrap_or_default();
                // `x, err := f()` binds a pair, which is what Go's second return
                // value is for. Read as one name it became a binding called
                // `x, err`, and every later use of either was undeclared.
                if names.len() > 1 {
                    return tuple_assign(cx, node, &names, true);
                }
                Stmt::Let {
                    name: cx.field_text(node, "left").unwrap_or_default(),
                    ty: None,
                    value: cx
                        .field(node, "right")
                        .map(|v| whole_or_read(cx, v, unlisted(cx, v))),
                    mutable: true,
                }
            }
            // `var wg sync.WaitGroup` declares a name whose type usually cannot
            // cross. The binding still has to exist: dropped whole, every later
            // statement read a name the output never declared. A value-less
            // declaration carries its own text as the initializer, the
            // keep_whole shape that keeps the name. One with a value reads like
            // `:=`.
            "var_declaration" => {
                let specs: Vec<Node> = cx
                    .children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "var_spec")
                    .collect();
                let [spec] = specs.as_slice() else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let names: Vec<Node> = {
                    let mut cursor = spec.walk();
                    spec.children_by_field_name("name", &mut cursor).collect()
                };
                let [name] = names.as_slice() else {
                    // `var a, b int` binds two names and the IR binds one.
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let value = cx
                    .field(*spec, "value")
                    .and_then(|v| cx.children(v).first().copied());
                match value {
                    Some(v) => Stmt::Let {
                        name: cx.text(*name),
                        ty: cx.field(*spec, "type").map(|t| ty(cx, t)),
                        value: Some(whole_or_read(cx, v, expr(cx, v))),
                        mutable: true,
                    },
                    None => Stmt::Let {
                        name: cx.text(*name),
                        ty: None,
                        value: Some(Expr::Unsupported(cx.unsupported(node))),
                        mutable: true,
                    },
                }
            }
            "assignment_statement" => {
                let names: Vec<Node> = cx
                    .field(node, "left")
                    .map(|l| cx.children(l))
                    .unwrap_or_default();
                if names.len() > 1 {
                    return tuple_assign(cx, node, &names, false);
                }
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
                        "if_statement" => stmts(cx, alt),
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
            // `for` is Go's only loop keyword and it has four spellings. Three of
            // them were carried as comments, which lost the loop and left every
            // name its header bound undeclared.
            // `switch x { case a: ... default: ... }`, the value dispatch every
            // target has. Go's cases break by themselves, which is the IR's rule too.
            "expression_switch_statement" => {
                let children = cx.children(node);
                let subject = children
                    .iter()
                    .find(|c| !matches!(c.kind(), "expression_case" | "default_case" | "comment"))
                    .map(|c| expr(cx, *c))
                    .unwrap_or(Expr::Null);
                let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
                let mut default: Vec<Stmt> = Vec::new();
                for child in &children {
                    match child.kind() {
                        "expression_case" => {
                            let patterns = cx
                                .field(*child, "value")
                                .map(|v| match v.kind() {
                                    "expression_list" => {
                                        cx.children(v).into_iter().map(|p| expr(cx, p)).collect()
                                    }
                                    _ => vec![expr(cx, v)],
                                })
                                .unwrap_or_default();
                            // The grammar wraps the arm's statements in one list.
                            let body: Vec<Stmt> = cx
                                .children(*child)
                                .into_iter()
                                .skip(1)
                                .filter(|c| c.kind() != "expression_list")
                                .flat_map(|c| match c.kind() {
                                    "statement_list" => cx
                                        .children(c)
                                        .into_iter()
                                        .map(|inner| stmt(cx, inner))
                                        .collect(),
                                    _ => vec![stmt(cx, c)],
                                })
                                .collect();
                            arms.push((patterns, body));
                        }
                        "default_case" => {
                            default = cx
                                .children(*child)
                                .into_iter()
                                .flat_map(|c| match c.kind() {
                                    "statement_list" => cx
                                        .children(c)
                                        .into_iter()
                                        .map(|inner| stmt(cx, inner))
                                        .collect(),
                                    _ => vec![stmt(cx, c)],
                                })
                                .collect();
                        }
                        _ => {}
                    }
                }
                Stmt::Switch {
                    subject,
                    arms,
                    default,
                }
            }
            "for_statement" => {
                let body = cx
                    .field(node, "body")
                    .map(|b| block(cx, b))
                    .unwrap_or_default();
                if let Some(clause) = cx
                    .children(node)
                    .into_iter()
                    .find(|c| c.kind() == "range_clause")
                {
                    let bound = cx
                        .field(clause, "left")
                        .map(|l| cx.text(l))
                        .unwrap_or_default();
                    let mut names = bound.split(',').map(|n| n.trim().to_string());
                    let first = names.next().unwrap_or_default();
                    let second = names.next();
                    let iterable = cx
                        .field(clause, "right")
                        .map(|r| expr(cx, r))
                        .unwrap_or(Expr::Null);
                    // `for i, v := range xs` binds the position beside the value.
                    // Dropping the first name left every use of it undeclared.
                    // `_` there is Go's word for a name nothing wants, and the
                    // loop is the plain one over the values.
                    return match second {
                        Some(binding) if first != "_" => Stmt::ForEachIndexed {
                            index: first,
                            binding,
                            iterable,
                            body,
                        },
                        Some(binding) => Stmt::ForEach {
                            binding,
                            iterable,
                            body,
                        },
                        None => Stmt::ForEach {
                            binding: first,
                            iterable,
                            body,
                        },
                    };
                }
                let clause = cx
                    .children(node)
                    .into_iter()
                    .find(|c| c.kind() == "for_clause");
                if let Some(clause) = clause {
                    return Stmt::CountedFor {
                        init: cx
                            .field(clause, "initializer")
                            .map(|i| Box::new(stmt(cx, i))),
                        condition: cx.field(clause, "condition").map(|c| expr(cx, c)),
                        update: cx.field(clause, "update").map(|u| Box::new(stmt(cx, u))),
                        body,
                        source: cx.text(node),
                        line: cx.line(node),
                    };
                }
                // What is left is `for cond { }` or the bare `for { }`. The first
                // is a `while` in every target; the second is a loop with no test.
                match cx.children(node).first() {
                    Some(condition) if condition.kind() != "block" => Stmt::While {
                        condition: expr(cx, *condition),
                        body,
                    },
                    _ => Stmt::CountedFor {
                        init: None,
                        condition: None,
                        update: None,
                        body,
                        source: cx.text(node),
                        line: cx.line(node),
                    },
                }
            }
            "inc_statement" | "dec_statement" => {
                let op = match node.kind() {
                    "inc_statement" => BinaryOp::Add,
                    _ => BinaryOp::Sub,
                };
                match cx.children(node).first() {
                    Some(target) => {
                        let target = expr(cx, *target);
                        Stmt::Assign {
                            target: target.clone(),
                            value: Expr::Binary {
                                op,
                                left: Box::new(target),
                                right: Box::new(Expr::Int("1".to_string())),
                            },
                        }
                    }
                    None => Stmt::Unsupported(cx.unsupported(node)),
                }
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
            // `func(n int) int { return n + 1 }`, Go's function value. Its body is
            // a block, and a block whose only statement returns is the one
            // expression a lambda holds.
            "func_literal" => {
                let params: Vec<Param> = cx
                    .field(node, "parameters")
                    .map(|list| {
                        cx.children(list)
                            .into_iter()
                            .filter(|p| p.kind() == "parameter_declaration")
                            .filter_map(|p| {
                                Some(super::lambda_param(
                                    cx.field_text(p, "name")?,
                                    cx.field(p, "type").map(|t| ty(cx, t)),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let body = cx.field(node, "body").map(|b| block(cx, b)).unwrap_or_default();
                match super::only_returned(&body) {
                    Some(value) => Expr::Lambda {
                        params,
                        returns: cx.field(node, "result").map(|t| ty(cx, t)),
                        body: Box::new(value),
                    },
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
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
            // `Point{}` and `Circle{Radius: n}` build a value of a named
            // type, and read as variant candidates. The settle pass attributes
            // each to the sum that answers the name. A record answering
            // instead makes it a construction; nothing answering carries it. A
            // slice or map literal names no bare type and stays carried.
            "composite_literal" => {
                // `[]int{3, 1, 2}` is the list literal every target spells.
                if cx
                    .field(node, "type")
                    .is_some_and(|t| t.kind() == "slice_type" || t.kind() == "array_type")
                {
                    let elements = cx
                        .field(node, "body")
                        .map(|body| {
                            cx.children(body)
                                .into_iter()
                                .filter(|c| c.is_named() && c.kind() != "comment")
                                // The grammar wraps each value in `literal_element`.
                                .map(|c| match c.kind() {
                                    "literal_element" => cx
                                        .children(c)
                                        .into_iter()
                                        .find(|inner| inner.is_named())
                                        .unwrap_or(c),
                                    _ => c,
                                })
                                .map(|c| expr(cx, c))
                                .collect()
                        })
                        .unwrap_or_default();
                    return Expr::ListLit(elements);
                }
                // `map[string]int64{…}` is the map literal every target spells.
                // Carried, the binding it initialised was left with nothing in
                // it, and the first key stored raised.
                if cx
                    .field(node, "type")
                    .is_some_and(|t| t.kind() == "map_type")
                {
                    let mut entries = Vec::new();
                    if let Some(body) = cx.field(node, "body") {
                        for element in cx.children(body) {
                            if !element.is_named() || element.kind() == "comment" {
                                continue;
                            }
                            if element.kind() != "keyed_element" {
                                return Expr::Unsupported(cx.unsupported(node));
                            }
                            let mut parts =
                                cx.children(element).into_iter().filter(|c| c.is_named());
                            let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
                                return Expr::Unsupported(cx.unsupported(node));
                            };
                            fn unwrap<'t>(cx: &Cx, n: Node<'t>) -> Node<'t> {
                                match n.kind() {
                                    "literal_element" => cx
                                        .children(n)
                                        .into_iter()
                                        .find(|c| c.is_named())
                                        .unwrap_or(n),
                                    _ => n,
                                }
                            }
                            entries.push((expr(cx, unwrap(cx, key)), expr(cx, unwrap(cx, value))));
                        }
                    }
                    return Expr::MapLit(entries);
                }
                // `struct{}{}` is the value Go writes where nothing is
                // carried, and it is what a set stores under each member.
                if cx
                    .field(node, "type")
                    .is_some_and(|t| t.kind() == "struct_type")
                    && cx.text(node).replace(char::is_whitespace, "") == "struct{}{}"
                {
                    return Expr::Null;
                }
                let named = cx
                    .field(node, "type")
                    .filter(|t| t.kind() == "type_identifier");
                let Some(ty) = named else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                let mut fields = Vec::new();
                if let Some(body) = cx.field(node, "body") {
                    for element in cx.children(body) {
                        if !element.is_named() || element.kind() == "comment" {
                            continue;
                        }
                        if element.kind() != "keyed_element" {
                            return Expr::Unsupported(cx.unsupported(node));
                        }
                        let mut parts = cx.children(element).into_iter().filter(|c| c.is_named());
                        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
                            return Expr::Unsupported(cx.unsupported(node));
                        };
                        // The grammar wraps both sides in `literal_element`.
                        let key = match key.kind() {
                            "literal_element" => cx
                                .children(key)
                                .into_iter()
                                .find(|c| c.is_named())
                                .unwrap_or(key),
                            _ => key,
                        };
                        let value = match value.kind() {
                            "literal_element" => cx
                                .children(value)
                                .into_iter()
                                .find(|c| c.is_named())
                                .unwrap_or(value),
                            _ => value,
                        };
                        fields.push((cx.text(key), expr(cx, value)));
                    }
                }
                Expr::Variant {
                    sum: String::new(),
                    name: cx.text(ty),
                    fields,
                }
            }
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

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

    /// A Java enum: the sum of its constants. A method a constant overrides
    /// becomes one function switching on the constant's name; the members
    /// after the `;` cross the way class members do.
    fn java_enum(cx: &Cx, node: Node<'_>, carried: &mut Vec<Item>) -> Option<Sum> {
        let name = cx.field_text(node, "name")?;
        let body = cx.field(node, "body")?;
        let mut variants = Vec::new();
        let mut overrides: Vec<(String, String, Function)> = Vec::new();
        for member in cx.children(body) {
            match member.kind() {
                "enum_constant" => {
                    let constant = cx.field_text(member, "name").or_else(|| {
                        cx.children(member)
                            .into_iter()
                            .find(|c| c.kind() == "identifier")
                            .map(|c| cx.text(c))
                    })?;
                    variants.push(Variant {
                        doc: doc_above(cx, member, &["/**", "*", "//"]),
                        name: constant.clone(),
                        tag: None,
                        fields: Vec::new(),
                    });
                    if let Some(class_body) = cx
                        .children(member)
                        .into_iter()
                        .find(|c| c.kind() == "class_body")
                    {
                        for inner in cx.children(class_body) {
                            if inner.kind() == "method_declaration" {
                                overrides.push((
                                    constant.clone(),
                                    cx.field_text(inner, "name").unwrap_or_default(),
                                    function(cx, inner),
                                ));
                            }
                        }
                    }
                }
                "enum_body_declarations" => {
                    for inner in cx.children(member) {
                        if inner.kind() == "method_declaration" {
                            carried.push(Item::Function(function(cx, inner)));
                        }
                    }
                }
                _ => {}
            }
        }
        // One function per overridden method name, selecting by constant.
        let mut names: Vec<String> = Vec::new();
        for (_, method, _) in &overrides {
            if !names.contains(method) {
                names.push(method.clone());
            }
        }
        for method in names {
            let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
            let mut template: Option<Function> = None;
            for (constant, named, f) in &overrides {
                if named != &method {
                    continue;
                }
                template.get_or_insert_with(|| f.clone());
                arms.push((vec![Expr::Str(constant.clone())], f.body.clone()));
            }
            let mut dispatch = template?;
            dispatch.body = vec![Stmt::Switch {
                subject: Expr::Name("it".to_string()),
                arms,
                default: Vec::new(),
            }];
            dispatch.params.insert(
                0,
                Param {
                    name: "it".to_string(),
                    ty: Some(Type::Named {
                        name: name.clone(),
                        args: Vec::new(),
                    }),
                    default: None,
                    kind: ParamKind::Normal,
                },
            );
            carried.push(Item::Function(dispatch));
        }
        Some(Sum {
            doc: doc_above(cx, node, &["/**", "*", "//"]),
            name,
            variants,
            exported: modifier_text(cx, node).contains("public"),
        })
    }

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        // A member a record cannot keep still has to reach the reader.
        let mut carried: Vec<Item> = Vec::new();
        let mut interfaces: Vec<String> = Vec::new();
        for child in cx.children(root) {
            match child.kind() {
                // The package clause names the compilation unit; it is not an import
                // and there is nothing in another language for it to become.
                "comment" | "line_comment" | "block_comment" | "package_declaration" => {}
                "import_declaration" => module.items.push(Item::Import {
                    text: cx.text(child),
                    line: cx.line(child),
                    target: None,
                }),
                "class_declaration" | "interface_declaration" | "record_declaration" => {
                    if child.kind() == "interface_declaration" {
                        if let Some(name) = cx.field_text(child, "name") {
                            interfaces.push(name);
                        }
                    }
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
                // An enum is a closed choice. Constant bodies override
                // methods per constant: inheritance said as dispatch, so each
                // overridden method crosses as one function switching on the
                // constant's name.
                "enum_declaration" => match java_enum(cx, child, &mut carried) {
                    Some(sum) => module.items.push(Item::Sum(sum)),
                    None => module.items.push(Item::Unsupported(cx.unsupported(child))),
                },
                _ => module.items.push(Item::Unsupported(cx.unsupported(child))),
            }
        }
        module.items.extend(carried);
        settle_accessors(&mut module);
        settle_builtins(&mut module);
        settle_interface_sums(&mut module, &interfaces);
        settle_entry_arguments(&mut module);
        settle_variants(&mut module);
        settle_variant_narrowing(&mut module);
        module
    }

    /// `main(String[] args)` with a body that never reads `args`.
    ///
    /// The runtime looks for that one signature. So the parameter is how Java
    /// spells "the entry point", not something the source chose. Read as data,
    /// it came back out as an argument the original never had. A body that
    /// does read it is a program taking arguments, and keeps it.
    fn settle_entry_arguments(module: &mut Module) {
        fn settle(f: &mut Function) {
            if f.name != "main" || f.params.len() != 1 {
                return;
            }
            let takes_strings = matches!(
                &f.params[0].ty,
                Some(Type::List(inner)) if **inner == Type::String
            );
            if !takes_strings {
                return;
            }
            let name = f.params[0].name.clone();
            let mut read = false;
            each_expr_in_stmts(&mut f.body, &mut |e| {
                if matches!(e, Expr::Name(n) if *n == name) {
                    read = true;
                }
            });
            if !read {
                f.params.clear();
            }
        }
        for item in &mut module.items {
            match item {
                Item::Function(f) => settle(f),
                Item::Record(record) => record.methods.iter_mut().for_each(settle),
                _ => {}
            }
        }
    }

    /// An empty interface with records implementing it is a closed choice.
    ///
    /// `sealed interface Shape permits Point, Circle` beside records that
    /// implement it is Java's most explicit sum declaration. It crossed as an
    /// empty struct, the returns of both variants type-wrong under a clean
    /// header. The idiom Go spells with a marker method settles the same way:
    /// interface consumed, records become variants. A member with methods of
    /// its own is more than a variant and holds the whole sum back.
    fn settle_interface_sums(module: &mut Module, interfaces: &[String]) {
        for interface in interfaces {
            let shell = module.items.iter().position(|item| {
                matches!(item, Item::Record(r)
                    if &r.name == interface && r.fields.is_empty() && r.methods.is_empty())
            });
            let Some(at) = shell else { continue };
            let members: Vec<Record> = module
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Record(r) if r.extends.as_deref() == Some(interface.as_str()) => {
                        Some(r.clone())
                    }
                    _ => None,
                })
                .collect();
            if members.is_empty() || members.iter().any(|r| !r.methods.is_empty()) {
                continue;
            }
            let exported = module
                .items
                .iter()
                .any(|item| matches!(item, Item::Record(r) if &r.name == interface && r.exported));
            let variants: Vec<Variant> = members
                .iter()
                .map(|member| Variant {
                    doc: member.doc.clone(),
                    name: member.name.clone(),
                    tag: None,
                    fields: member.fields.clone(),
                })
                .collect();
            module.items[at] = Item::Sum(Sum {
                doc: Vec::new(),
                name: interface.clone(),
                variants,
                exported,
            });
            let consumed: std::collections::BTreeSet<String> =
                members.into_iter().map(|m| m.name).collect();
            module
                .items
                .retain(|item| !matches!(item, Item::Record(r) if consumed.contains(&r.name)));
        }
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
        // `implements Greeter` is part of the type. One base slot exists, so a
        // single interface with no superclass rides in it. Anything more is said
        // beside the type instead of dropped without a word, which is what
        // happened to every record's `implements` clause.
        let interfaces: Vec<String> = cx
            .field(node, "interfaces")
            .map(|clause| {
                cx.children(clause)
                    .into_iter()
                    .flat_map(|list| cx.children(list))
                    .map(|t| cx.text(t))
                    .collect()
            })
            .unwrap_or_default();
        if record.extends.is_none() && interfaces.len() == 1 {
            record.extends = Some(interfaces[0].clone());
        } else if !interfaces.is_empty() {
            record.doc.push(format!(
                "the source also declares `implements {}`; one base is all that carries.",
                interfaces.join(", ")
            ));
        }
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
                            // `final List<String> history = new ArrayList<>()`
                            // starts the field with a list. Dropped, every
                            // method that appended to it hit a null.
                            default: cx.field(declarator, "value").map(|v| expr(cx, v)),
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
                // A member this does not recognise is not a member that is not there. A
                // member loop ending in `_ => {}` drops what it does not recognise, and the
                // report still counts every signature as carried.
                _ => carried.push(Item::Unsupported(cx.unsupported(member))),
            }
        }
        // A record derives an accessor from each field, and a compact body that
        // spells one out declares it twice. In targets where the field crosses as
        // data the pair collided, `name: string` beside `name(): string`, so the
        // field wins. A body that did more than return the field is said beside
        // the field it stood for.
        if node.kind() == "record_declaration" {
            let field_names: std::collections::BTreeSet<String> =
                record.fields.iter().map(|f| f.name.clone()).collect();
            let mut overridden: Vec<String> = Vec::new();
            record.methods.retain(|method| {
                let collides = field_names.contains(&method.name)
                    && method.params.is_empty()
                    && !method.is_constructor;
                if collides && !spells_the_accessor(method) {
                    overridden.push(method.name.clone());
                }
                !collides
            });
            for method_name in overridden {
                if let Some(field) = record.fields.iter_mut().find(|f| f.name == method_name) {
                    field.doc.push(format!(
                        "the source overrode the record's `{method_name}()` accessor \
                         with a body of its own; the field carries and that body does not."
                    ));
                }
            }
        }
        (Some(record), constants)
    }

    /// Is this method the accessor a record derives anyway: `return field;`?
    fn spells_the_accessor(method: &Function) -> bool {
        match method.body.as_slice() {
            [Stmt::Return(Some(Expr::Name(n)))] => *n == method.name,
            [Stmt::Return(Some(Expr::Field { of, name }))] => {
                *name == method.name && matches!(of.as_ref(), Expr::Name(this) if this == "this")
            }
            _ => false,
        }
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
            is_private: modifier_text(cx, node).contains("private"),
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
                "Runnable" => Type::Fn {
                    params: Vec::new(),
                    returns: Box::new(Type::Unit),
                },
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
        // `java.util.function.Predicate<T>` names the same interface as
        // `Predicate<T>`, and this writer spells it in full. Read as an
        // unnamed generic, a function type came back from a round trip with
        // its arity and nothing else.
        let base = cx
            .children(node)
            .into_iter()
            .find(|c| matches!(c.kind(), "type_identifier" | "scoped_type_identifier"))
            .map(|c| {
                let text = cx.text(c);
                text.rsplit('.').next().unwrap_or(&text).to_string()
            })
            .unwrap_or_default();
        let arguments: Vec<Type> = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "type_arguments")
            .map(|a| cx.children(a).into_iter().map(|t| ty_of(cx, t)).collect())
            .unwrap_or_default();
        match (base.as_str(), arguments.as_slice()) {
            ("Set" | "HashSet" | "TreeSet" | "LinkedHashSet", [inner]) => {
                Type::Set(Box::new(inner.clone()))
            }
            ("List" | "ArrayList" | "Collection" | "Iterable", [inner]) => {
                Type::List(Box::new(inner.clone()))
            }
            ("Map" | "HashMap", [key, value]) => {
                Type::Map(Box::new(key.clone()), Box::new(value.clone()))
            }
            ("Optional", [inner]) => Type::Optional(Box::new(inner.clone())),
            // Java names one interface per function shape. Each is the same
            // question the other five spell with a function type.
            ("Function", [takes, answers]) => Type::Fn {
                params: vec![takes.clone()],
                returns: Box::new(answers.clone()),
            },
            ("UnaryOperator", [both]) => Type::Fn {
                params: vec![both.clone()],
                returns: Box::new(both.clone()),
            },
            ("BiFunction", [first, second, answers]) => Type::Fn {
                params: vec![first.clone(), second.clone()],
                returns: Box::new(answers.clone()),
            },
            ("Supplier", [answers]) => Type::Fn {
                params: Vec::new(),
                returns: Box::new(answers.clone()),
            },
            ("Consumer", [takes]) => Type::Fn {
                params: vec![takes.clone()],
                returns: Box::new(Type::Unit),
            },
            ("BiConsumer", [first, second]) => Type::Fn {
                params: vec![first.clone(), second.clone()],
                returns: Box::new(Type::Unit),
            },
            ("Predicate", [takes]) => Type::Fn {
                params: vec![takes.clone()],
                returns: Box::new(Type::Bool),
            },
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

    /// One clause of a `for` header, which is an expression and not a statement.
    ///
    /// `i++` and `i = 0` stand alone there, with no semicolon and no
    /// `expression_statement` around them. Read as plain expressions they lost
    /// the assignment they are.
    fn header_stmt(cx: &Cx, node: Node<'_>) -> Stmt {
        match node.kind() {
            "local_variable_declaration" => stmt(cx, node),
            "assignment_expression" => {
                let target = cx
                    .field(node, "left")
                    .map(|l| expr(cx, l))
                    .unwrap_or(Expr::Null);
                let value = cx
                    .field(node, "right")
                    .map(|r| expr(cx, r))
                    .unwrap_or(Expr::Null);
                let operator = cx.field_text(node, "operator").unwrap_or_default();
                if operator == "=" {
                    return Stmt::Assign { target, value };
                }
                match super::desugar_compound(target, &operator, value) {
                    Some(assign) => assign,
                    None => Stmt::Unsupported(cx.unsupported(node)),
                }
            }
            "update_expression" => match step_of(cx, node) {
                Some(step) => step,
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            _ => Stmt::Expr(expr(cx, node)),
        }
    }

    /// `i++` and `i--` as the assignment each one is.
    fn step_of(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let text = cx.text(node);
        let op = if text.contains("++") {
            BinaryOp::Add
        } else if text.contains("--") {
            BinaryOp::Sub
        } else {
            return None;
        };
        let target = expr(cx, *cx.children(node).first()?);
        Some(Stmt::Assign {
            target: target.clone(),
            value: Expr::Binary {
                op,
                left: Box::new(target),
                right: Box::new(Expr::Int("1".to_string())),
            },
        })
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
                // `int a = 1, b = 2;` is two bindings in one statement and the
                // IR holds one per Let: they group.
                let mutable = !cx.text(node).trim_start().starts_with("final");
                let lets: Vec<Stmt> = declarators
                    .iter()
                    .map(|d| Stmt::Let {
                        name: cx.field_text(*d, "name").unwrap_or_default(),
                        ty: cx.field(node, "type").map(|t| ty_of(cx, t)),
                        value: cx.field(*d, "value").map(|v| expr(cx, v)),
                        mutable,
                    })
                    .collect();
                match lets.len() {
                    0 => Stmt::Unsupported(cx.unsupported(node)),
                    1 => lets.into_iter().next().expect("one"),
                    _ => Stmt::Block(lets),
                }
            }
            // One node covers `=` and `+=` alike, and reading them alike turned
            // `total += item` into `total = item`. `i++` is a third spelling of
            // the same thing, and [`header_stmt`] knows all three.
            "expression_statement" => match cx.children(node).first().copied() {
                Some(inner) => header_stmt(cx, inner),
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
            // `for (int i = 0; i < n; i++)` counts, and the IR says so. Carried as
            // a comment it took the whole body with it.
            "for_statement" => {
                let clauses = |name: &str| -> Vec<Node> {
                    let mut cursor = node.walk();
                    node.children_by_field_name(name, &mut cursor).collect()
                };
                let (init, update) = (clauses("init"), clauses("update"));
                // `for (i = 0, j = n; ...)` runs two statements in one clause and
                // the IR holds one. Carried whole, it stays readable.
                if init.len() > 1 || update.len() > 1 {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                // `for (int i = 0, n = …; …)` declares twice in one clause;
                // the bindings hoist just before the loop, in their order.
                let read_init = init.first().map(|i| header_stmt(cx, *i));
                if let Some(Stmt::Block(lets)) = read_init {
                    let mut grouped = lets;
                    grouped.push(Stmt::CountedFor {
                        init: None,
                        condition: cx.field(node, "condition").map(|c| condition(cx, c)),
                        update: update.first().map(|u| Box::new(header_stmt(cx, *u))),
                        body: cx
                            .field(node, "body")
                            .map(|b| branch(cx, b))
                            .unwrap_or_default(),
                        source: cx.text(node),
                        line: cx.line(node),
                    });
                    return Stmt::Block(grouped);
                }
                Stmt::CountedFor {
                    init: read_init.map(Box::new),
                    condition: cx.field(node, "condition").map(|c| condition(cx, c)),
                    update: update.first().map(|u| Box::new(header_stmt(cx, *u))),
                    body: cx
                        .field(node, "body")
                        .map(|b| branch(cx, b))
                        .unwrap_or_default(),
                    source: cx.text(node),
                    line: cx.line(node),
                }
            }
            // `for (X x : xs)` is the loop every language here has. A C-style `for` is
            // not, and is carried.
            // `switch (x) { case a: ... default: ... }`. Fallthrough is the one
            // thing the IR's switch does not model, so a group that falls into the
            // next carries whole.
            "switch_expression" | "switch_statement" => {
                let subject = cx
                    .field(node, "condition")
                    .map(|c| {
                        let inner = cx
                            .children(c)
                            .into_iter()
                            .find(|n| n.is_named())
                            .unwrap_or(c);
                        expr(cx, inner)
                    })
                    .unwrap_or(Expr::Null);
                let Some(block) = cx.field(node, "body") else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
                let mut default: Vec<Stmt> = Vec::new();
                for group in cx.children(block) {
                    if group.kind() != "switch_block_statement_group" {
                        continue;
                    }
                    let mut patterns: Vec<Expr> = Vec::new();
                    let mut is_default = false;
                    let mut body: Vec<Stmt> = Vec::new();
                    for piece in cx.children(group) {
                        match piece.kind() {
                            "switch_label" => {
                                match cx.children(piece).into_iter().find(|c| c.is_named()) {
                                    Some(value) => patterns.push(expr(cx, value)),
                                    None => is_default = true,
                                }
                            }
                            _ => body.push(stmt(cx, piece)),
                        }
                    }
                    // `break` closes a group the way the IR already assumes.
                    if matches!(body.last(), Some(Stmt::Break)) {
                        body.pop();
                    } else if !matches!(body.last(), Some(Stmt::Return(_)) | Some(Stmt::Throw(_))) {
                        // Anything else falls through into the next group, which the
                        // shared switch has no way to say.
                        return Stmt::Unsupported(cx.unsupported(node));
                    }
                    match is_default {
                        true => default = body,
                        false => arms.push((patterns, body)),
                    }
                }
                Stmt::Switch {
                    subject,
                    arms,
                    default,
                }
            }
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
            // `new int[] { 3, 1, 2 }` is the list literal every target spells; the
            // initializer alone appears where the type is inferred.
            "array_creation_expression" | "array_initializer" => {
                let elements = match node.kind() {
                    "array_initializer" => Some(node),
                    _ => cx
                        .children(node)
                        .into_iter()
                        .find(|c| c.kind() == "array_initializer"),
                };
                match elements {
                    Some(list) => Expr::ListLit(
                        cx.children(list)
                            .into_iter()
                            .filter(|c| c.is_named() && c.kind() != "comment")
                            .map(|c| expr(cx, c))
                            .collect(),
                    ),
                    // `new int[5]` sizes without contents, which no other target
                    // spells as a literal.
                    None => Expr::Unsupported(cx.unsupported(node)),
                }
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
                let operator = cx.field_text(node, "operator").unwrap_or_default();
                // `>>>` shifts as unsigned bits; every target spells that its
                // own way, so it settles to a canonical call.
                if operator.trim() == ">>>" {
                    return Expr::Call {
                        callee: Box::new(Expr::Name("ushr".to_string())),
                        args: vec![
                            cx.field(node, "left")
                                .map(|l| expr(cx, l))
                                .unwrap_or(Expr::Null),
                            cx.field(node, "right")
                                .map(|r| expr(cx, r))
                                .unwrap_or(Expr::Null),
                        ],
                    };
                }
                match super::binary_op(&operator) {
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
            // `x -> e` and `(int x) -> e`, the one-expression lambda. A block
            // body is more than the shared shape and stays carried.
            "lambda_expression" => {
                let params: Option<Vec<Param>> =
                    cx.field(node, "parameters")
                        .and_then(|list| match list.kind() {
                            "identifier" => Some(vec![super::lambda_param(cx.text(list), None)]),
                            "inferred_parameters" => cx
                                .children(list)
                                .into_iter()
                                .map(|p| {
                                    (p.kind() == "identifier")
                                        .then(|| super::lambda_param(cx.text(p), None))
                                })
                                .collect(),
                            "formal_parameters" => cx
                                .children(list)
                                .into_iter()
                                .filter(|p| p.kind() == "formal_parameter")
                                .map(|p| {
                                    let name = cx.field_text(p, "name")?;
                                    Some(super::lambda_param(
                                        name,
                                        cx.field(p, "type").map(|t| ty_of(cx, t)),
                                    ))
                                })
                                .collect(),
                            _ => None,
                        });
                match (params, cx.field(node, "body")) {
                    (Some(params), Some(body)) => {
                        let value = match body.kind() {
                            "block" => super::only_returned(&block(cx, body)),
                            _ => Some(expr(cx, body)),
                        };
                        match value {
                            Some(value) => Expr::Lambda {
                                params,
                                returns: None,
                                body: Box::new(value),
                            },
                            None => Expr::Unsupported(cx.unsupported(node)),
                        }
                    }
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
/// Two things shape this reader. A `variable_declaration` with no `var` or `const` in front
/// of it is an **assignment**. The grammar reuses the node for both. So telling the two apart
/// means reading the keyword instead of the node kind. A type is a value there too. `const
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

    /// The one `.name = value` pair of an anonymous initializer.
    fn variant_field<'t>(cx: &Cx, a: Node<'t>) -> Option<(String, Node<'t>)> {
        let parts = cx.children(a);
        let target = parts.first()?;
        let name = dot_literal(cx, *target)?;
        let value = parts.get(1).copied()?;
        Some((name, value))
    }

    /// Does this value's tree hold an `error{...}` set declaration?
    fn contains_error_set(node: Node<'_>) -> bool {
        if node.kind() == "error_set_declaration" {
            return true;
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        children.into_iter().any(contains_error_set)
    }

    /// Qualify a dot literal against the annotation that names its type.
    fn qualify_dot_literal(read: Expr, annotation: &str) -> Expr {
        let owner = || Box::new(Expr::Name(annotation.to_string()));
        match read {
            Expr::Variant { sum, name, fields } if sum.is_empty() && fields.is_empty() => {
                Expr::Field { of: owner(), name }
            }
            Expr::Call { callee, args } => match *callee {
                Expr::Variant { sum, name, fields } if sum.is_empty() && fields.is_empty() => {
                    Expr::Call {
                        callee: Box::new(Expr::Field { of: owner(), name }),
                        args,
                    }
                }
                other => Expr::Call {
                    callee: Box::new(other),
                    args,
                },
            },
            // `.{ .value = 9 }` under a `Box` annotation builds a `Box`. Named
            // as a call taking keywords, the targets that build a record
            // through a constructor got an object literal instead of arguments.
            Expr::RecordLit { ty, fields } if ty.is_empty() => match *owner() {
                // A dotted name belongs to another module, and this file
                // declares no fields for it, so only a plain one builds a
                // record here.
                Expr::Name(named) if !named.contains('.') => {
                    Expr::RecordLit { ty: named, fields }
                }
                other => Expr::New {
                    callee: Box::new(other),
                    args: fields
                        .into_iter()
                        .map(|(name, value)| Expr::Keyword {
                            name,
                            value: Box::new(value),
                        })
                        .collect(),
                },
            },
            Expr::Propagate(inner) => {
                Expr::Propagate(Box::new(qualify_dot_literal(*inner, annotation)))
            }
            other => other,
        }
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
                // `fn F(comptime T: type) type { return struct { … }. }` is a generic record.
                // The comptime parameters erase, and the struct takes the function's name.
                "function_declaration" if returns_type(cx, child) => {
                    match generic_record(cx, child, &mut carried) {
                        Some(item) => module.items.push(item),
                        None => module.items.push(Item::Unsupported(cx.unsupported(child))),
                    }
                }
                "function_declaration" => module.items.push(match function(cx, child) {
                    Some(f) => Item::Function(f),
                    None => Item::Unsupported(cx.unsupported(child)),
                }),
                // `test "name" { … }` is a named test. The form that names a
                // declaration instead of a string reruns that declaration's
                // tests, and carries.
                // `comptime { … }` runs its checks when the compiler does;
                // a test that runs them is when these targets check things.
                "comptime_declaration" => {
                    let body = cx
                        .children(child)
                        .iter()
                        .find(|c| c.kind() == "block")
                        .map(|b| block(cx, *b))
                        .unwrap_or_default();
                    module.items.push(Item::Test {
                        doc: vec!["ran at compile time in the source".to_string()],
                        name: "comptime checks".to_string(),
                        body,
                    });
                }
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
        // A called dot literal is a decl-literal call, not a variant build.
        // It settles here by the member's name, before variant attribution can
        // claim it for a sum that happens to share the name.
        for item in module.items.iter_mut() {
            super::each_expr_in_item(item, &mut |e| {
                if let Expr::Call { callee, .. } = e {
                    if let Expr::Variant { sum, name, fields } = callee.as_ref() {
                        if sum.is_empty() && fields.is_empty() {
                            **callee = Expr::Name(name.clone());
                        }
                    }
                }
            });
        }
        settle_builtins(&mut module);
        settle_error_returns(&mut module, &error_sets);
        super::settle_variants(&mut module);
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
                Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => {
                    in_type(inner, from, to)
                }
                Type::Fn { params, returns } => {
                    for param in params {
                        in_type(param, from, to);
                    }
                    in_type(returns, from, to);
                }
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
        let subject_node = children.first().copied()?;
        let subject = expr(cx, subject_node);
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
            // A range selects by comparison, which a case label cannot say.
            if parts.iter().any(|c| matches!(c.kind(), "..." | "..")) {
                return None;
            }
            let arrow = parts.iter().position(|c| c.kind() == "=>")?;
            // `=> |payload| body` binds the selected value; the lowering binds
            // the switched value itself, which is what a target without
            // payloads can hold of it.
            let payload: Option<String> = parts[arrow + 1..]
                .iter()
                .find(|c| c.kind() == "payload")
                .and_then(|p| {
                    all(*p)
                        .iter()
                        .find(|c| c.kind() == "identifier")
                        .map(|id| cx.text(*id))
                });
            let body_node = parts[arrow + 1..]
                .iter()
                .find(|c| c.is_named() && c.kind() != "payload")
                .copied();
            let mut body = match body_node {
                Some(body_node) if is_body(body_node) => body_of(cx, body_node),
                // `=> X catch |e| handler` tries for the arm's value.
                Some(body_node) if body_node.kind() == "catch_expression" => {
                    match catch_pieces(cx, body_node) {
                        Some((attempted, binding, handler)) => vec![Stmt::Try {
                            body: vec![Stmt::Expr(attempted)],
                            catches: vec![Catch {
                                binding,
                                ty: None,
                                body: handler,
                            }],
                            finally: Vec::new(),
                            source: cx.text(body_node),
                            line: cx.line(body_node),
                        }],
                        None => vec![Stmt::Expr(expr(cx, body_node))],
                    }
                }
                // `=> blk: { … break :blk v; }` runs once; a valued break is
                // the arm's value, which the let-position lowering assigns.
                Some(body_node) if body_node.kind() == "labeled_type_expression" => {
                    let inner = cx
                        .children(body_node)
                        .into_iter()
                        .find(|c| c.kind() == "block");
                    match inner {
                        Some(inner) => {
                            let mut stmts = body_of(cx, inner);
                            stmts.push(Stmt::Break);
                            vec![Stmt::While {
                                condition: Expr::Bool(true),
                                body: stmts,
                            }]
                        }
                        None => vec![Stmt::Unsupported(cx.unsupported(body_node))],
                    }
                }
                // A statement arm as itself; an expression arm as the value it
                // is, which the let-position lowering then assigns.
                Some(body_node) => match stmt(cx, body_node) {
                    Stmt::Unsupported(u) => match expr(cx, body_node) {
                        Expr::Unsupported(_) => vec![Stmt::Unsupported(u)],
                        value => vec![Stmt::Expr(value)],
                    },
                    read => vec![read],
                },
                // `=> unreachable` and other childless arms: the text decides.
                None => {
                    let tail = parts[arrow + 1..]
                        .iter()
                        .map(|c| cx.text(*c))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if tail.trim_start().starts_with("unreachable") {
                        vec![Stmt::Throw(Expr::Str("unreachable".to_string()))]
                    } else {
                        return None;
                    }
                }
            };
            if let Some(bound) = payload {
                if bound != "_" {
                    body.insert(
                        0,
                        Stmt::Let {
                            name: bound,
                            ty: None,
                            value: Some(subject.clone()),
                            mutable: false,
                        },
                    );
                }
            }
            if parts[..arrow].iter().any(|c| c.kind() == "else") {
                default = body;
                continue;
            }
            let mut literals = Vec::new();
            for value in parts[..arrow].iter().filter(|c| c.is_named()) {
                let literal = match value.kind() {
                    "integer" | "float" | "string" | "char_literal" | "character" => {
                        expr(cx, *value)
                    }
                    // `.tag` selects a variant; its tag string is the label.
                    "field_expression" => {
                        let tag = dot_literal(cx, *value)?;
                        Expr::Str(tag)
                    }
                    // `error.X` selects a failure by name.
                    "error_type" => match expr(cx, *value) {
                        Expr::Field { name, .. } => Expr::Str(name),
                        _ => return None,
                    },
                    _ => return None,
                };
                literals.push(literal);
            }
            if literals.is_empty() {
                return None;
            }
            arms.push((literals, body));
        }
        Some(Stmt::Switch {
            subject,
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

    /// Does this function declare that it answers a type?
    fn returns_type(cx: &Cx, node: Node<'_>) -> bool {
        // The return type is the last named child before the body block.
        let parts = cx.children(node);
        let block_at = parts.iter().position(|c| c.kind() == "block");
        match block_at {
            Some(at) if at > 0 => parts[..at]
                .last()
                .is_some_and(|c| cx.text(*c).trim() == "type"),
            _ => false,
        }
    }

    /// The struct a type-returning function builds, read as a record named
    /// after the function.
    fn generic_record(cx: &Cx, node: Node<'_>, carried: &mut Vec<Item>) -> Option<Item> {
        let name = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(c))?;
        let body = cx
            .children(node)
            .into_iter()
            .find(|c| c.kind() == "block")?;
        // In statement position the return arrives wrapped.
        let returned = cx
            .children(body)
            .into_iter()
            .filter_map(|c| match c.kind() {
                "return_expression" => Some(c),
                "expression_statement" => cx
                    .children(c)
                    .into_iter()
                    .find(|inner| inner.kind() == "return_expression"),
                _ => None,
            })
            .next()?;
        let built = cx
            .children(returned)
            .into_iter()
            .find(|c| c.kind() == "struct_declaration")?;
        let exported = cx.text(node).trim_start().starts_with("pub");
        Some(Item::Record(record(
            cx, node, name, exported, built, carried,
        )))
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
        // Neither is `const X = @import("m.zig").X;`, which reaches into one.
        if cx.text(value).starts_with("@import") {
            return Some(Item::Import {
                text: cx.text(node),
                line: cx.line(node),
                target: None,
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
                return tagged_union(cx, node, name, exported, value, carried).map(Item::Sum);
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
        // `const E = A || error{X};` unions sets. The alias keeps the union's spelling as text,
        // which is all a target without error sets can hold.
        if contains_error_set(value) {
            return Some(Item::Constant(Constant {
                doc: doc_above(cx, node, &["///", "//"]),
                name,
                ty: None,
                value: Expr::Str(cx.text(value)),
                exported,
            }));
        }

        // A dot literal in the value resolves against the annotation, the
        // same way a binding's would.
        let mut read = expr(cx, value);
        if let Some(annotated) = after(&parts, ":", "=") {
            read = qualify_dot_literal(read, cx.text(annotated).trim());
        }
        Some(Item::Constant(Constant {
            doc: doc_above(cx, node, &["///", "//"]),
            name,
            ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
            value: read,
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
                tag: None,
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
                tag: None,
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
        carried: &mut Vec<Item>,
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
                        tag: None,
                        fields,
                    });
                }
                "comment" => {}
                // A method on the union has no slot in a sum. It goes beside the type as a free
                // function taking the union first, which is how its body already reads it.
                "function_declaration" => match function(cx, member) {
                    Some(f) => carried.push(Item::Function(f)),
                    None => carried.push(Item::Unsupported(cx.unsupported(member))),
                },
                // A nested declaration goes beside the type too.
                "variable_declaration" => match declaration(cx, member, carried) {
                    Some(item) => carried.push(item),
                    None => carried.push(Item::Unsupported(cx.unsupported(member))),
                },
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
                "function_declaration" if returns_type(cx, member) => {
                    match generic_record(cx, member, carried) {
                        Some(item) => carried.push(item),
                        None => carried.push(Item::Unsupported(cx.unsupported(member))),
                    }
                }
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
                // A member this does not recognise is not a member that is not there. Every
                // reader here ended its member loop with `_ => {}`. A `@staticmethod`
                // disappeared from a class that way, while the report said every signature
                // had carried across intact.
                // A test declared inside the struct is still a test; it goes
                // beside the type like a method the record cannot keep.
                "test_declaration" => carried.push(match test_block(cx, member) {
                    Some(t) => t,
                    None => Item::Unsupported(cx.unsupported(member)),
                }),
                // The binding that names the record itself adds nothing.
                "variable_declaration" if binds_this(cx, member) => {}
                // A nested declaration, a type, a constant, an error set,
                // goes beside the record; the file shares one namespace there.
                "variable_declaration" if !binds_this(cx, member) => {
                    match declaration(cx, member, carried) {
                        Some(item) => carried.push(item),
                        None => carried.push(Item::Unsupported(cx.unsupported(member))),
                    }
                }
                _ => carried.push(Item::Unsupported(cx.unsupported(member))),
            }
        }
        // The overload lowering numbers later overloads, `add`, `add2`, and reading the
        // numbered name back beside its base restores the overload. So a container of them
        // survives the round trip.
        let bases: Vec<String> = record.methods.iter().map(|m| m.name.clone()).collect();
        for method in record.methods.iter_mut() {
            let trimmed = method.name.trim_end_matches(|c: char| c.is_ascii_digit());
            if trimmed.len() < method.name.len()
                && !trimmed.is_empty()
                && bases.iter().any(|b| b == trimmed)
            {
                method.name = trimmed.to_string();
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
                // `comptime T: type` is Zig's generics: the parameter is a *type*, supplied
                // where another language would write `<T>`. The IR has no generic parameters.
                // Reading one as an ordinary parameter produced `func Lazy(comptime type,
                // comptime type) type`, a signature that means something else in every
                // target.
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

        // A `comptime` parameter is a *type*, supplied where another language writes `<T>`. The
        // IR has no generic parameters. And reading one as an ordinary parameter produced `func
        // Lazy(comptime type, comptime type) type`, a signature that means something else in
        // every target.
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
            is_private: false,
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
            // The grammar's name for `?T`. Reading it as `optional_type`, the name it looks
            // like it should carry, matched nothing. Every optional in every Zig file then
            // crossed as a foreign type spelled `?T`.
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
        // A generic type here is a name *applied* to its arguments, as in `ArrayList(u8)`,
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
        // `fn (i64) i64`, and the pointer to one this tool's own writer emits.
        // Zig has no closure, so a function value is always one of these.
        let callable = text
            .trim_start_matches('*')
            .trim_start()
            .trim_start_matches("const ")
            .trim_start();
        if let Some(after) = callable.strip_prefix("fn") {
            if let Some((inside, rest)) = super::parenthesised(after.trim_start()) {
                let params = super::parameter_types(&inside, from_text);
                let returns = match rest.trim().is_empty() {
                    true => Type::Unit,
                    false => from_text(&rest),
                };
                return Type::Fn {
                    params,
                    returns: Box::new(returns),
                };
            }
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
                        // A hash map whose values carry nothing is a set:
                        // membership is all it can answer.
                        ("StringHashMap", [Type::Unit]) => Type::Set(Box::new(Type::String)),
                        ("AutoHashMap", [key, Type::Unit]) => Type::Set(Box::new(key.clone())),
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

    /// Unfold each `x orelse return/break/continue` in this statement: bind the optional, guard
    /// on null with the control flow the source named. And let the statement read the bound
    /// value.
    fn settle_orelse_controls(mut built: Stmt, guards: &mut usize) -> Stmt {
        fn control_of(callee: &str, args: &[Expr]) -> Option<Stmt> {
            match callee {
                "__fr_orelse_return" => Some(Stmt::Return(args.first().cloned())),
                "__fr_orelse_continue" => Some(Stmt::Continue),
                "__fr_orelse_break" => Some(match args.first() {
                    Some(Expr::Str(label)) => Stmt::BreakWith {
                        label: label.clone(),
                        value: None,
                    },
                    _ => Stmt::Break,
                }),
                _ => None,
            }
        }
        /// Find the first control-marked coalesce, replace it with `name`, and
        /// hand back its value and control statement.
        fn extract(e: &mut Expr, name: &str) -> Option<(Expr, Stmt)> {
            if let Expr::Coalesce { value, fallback } = e {
                if let Expr::Call { callee, args } = fallback.as_ref() {
                    if let Expr::Name(marker) = callee.as_ref() {
                        if let Some(control) = control_of(marker, args) {
                            let value = std::mem::replace(value.as_mut(), Expr::Null);
                            *e = Expr::Name(name.to_string());
                            return Some((value, control));
                        }
                    }
                }
            }
            match e {
                Expr::Field { of, .. } => extract(of, name),
                Expr::Index { of, index } => extract(of, name).or_else(|| extract(index, name)),
                Expr::Call { callee, args } | Expr::New { callee, args } => {
                    extract(callee, name).or_else(|| args.iter_mut().find_map(|a| extract(a, name)))
                }
                Expr::Binary { left, right, .. } => {
                    extract(left, name).or_else(|| extract(right, name))
                }
                Expr::Unary { operand, .. } => extract(operand, name),
                Expr::Await(inner) | Expr::Propagate(inner) => extract(inner, name),
                Expr::Coalesce { value, fallback } => {
                    extract(value, name).or_else(|| extract(fallback, name))
                }
                Expr::Ternary {
                    condition,
                    then,
                    otherwise,
                } => extract(condition, name)
                    .or_else(|| extract(then, name))
                    .or_else(|| extract(otherwise, name)),
                Expr::Tuple(items) | Expr::ListLit(items) => {
                    items.iter_mut().find_map(|it| extract(it, name))
                }
                Expr::MapLit(entries) => entries
                    .iter_mut()
                    .find_map(|(k, v)| extract(k, name).or_else(|| extract(v, name))),
                Expr::Keyword { value, .. } => extract(value, name),
                Expr::Cast { value, ty } => extract(value, name).or_else(|| extract(ty, name)),
                Expr::Variant { fields, .. } | Expr::RecordLit { fields, .. } => {
                    fields.iter_mut().find_map(|(_, v)| extract(v, name))
                }
                Expr::Template(parts) => parts.iter_mut().find_map(|p| match p {
                    TemplatePart::Expr(e) => extract(e, name),
                    TemplatePart::Text(_) => None,
                }),
                _ => None,
            }
        }
        loop {
            let name = format!("fr_opt{guards}");
            let found = {
                let mut hit = None;
                let mut try_expr = |e: &mut Expr| {
                    if hit.is_none() {
                        hit = extract(e, &name);
                    }
                };
                match &mut built {
                    Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Throw(e) => try_expr(e),
                    Stmt::Let { value: Some(e), .. } => try_expr(e),
                    Stmt::Assign { target, value } => {
                        try_expr(target);
                        try_expr(value);
                    }
                    Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                        try_expr(condition)
                    }
                    Stmt::IfPresent { value, .. } | Stmt::WhilePresent { value, .. } => {
                        try_expr(value)
                    }
                    Stmt::Switch { subject, .. } => try_expr(subject),
                    Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                        try_expr(iterable)
                    }
                    _ => {}
                }
                hit
            };
            let Some((value, control)) = found else {
                return built;
            };
            *guards += 1;
            built = Stmt::Block(vec![
                Stmt::Let {
                    name,
                    ty: None,
                    value: Some(value),
                    mutable: false,
                },
                Stmt::If {
                    condition: Expr::Binary {
                        op: BinaryOp::Eq,
                        left: Box::new(Expr::Name(format!("fr_opt{}", *guards - 1))),
                        right: Box::new(Expr::Null),
                    },
                    then: vec![control],
                    otherwise: Vec::new(),
                },
                built,
            ]);
        }
    }

    /// Run the step before each `continue` in this body, nested loops excluded:
    /// their continues are their own.
    fn step_before_continues(body: &mut Vec<Stmt>, step: &Stmt) {
        let mut at = 0;
        while at < body.len() {
            match &mut body[at] {
                Stmt::Continue => {
                    body.insert(at, step.clone());
                    at += 2;
                    continue;
                }
                Stmt::If {
                    then, otherwise, ..
                }
                | Stmt::IfPresent {
                    then, otherwise, ..
                } => {
                    step_before_continues(then, step);
                    step_before_continues(otherwise, step);
                }
                Stmt::Block(inner) | Stmt::Defer(inner) | Stmt::ErrDefer(inner) => {
                    step_before_continues(inner, step);
                }
                Stmt::Switch { arms, default, .. } => {
                    for (_, arm) in arms.iter_mut() {
                        step_before_continues(arm, step);
                    }
                    step_before_continues(default, step);
                }
                Stmt::Try {
                    body: tried,
                    catches,
                    finally,
                    ..
                } => {
                    step_before_continues(tried, step);
                    for catch in catches.iter_mut() {
                        step_before_continues(&mut catch.body, step);
                    }
                    step_before_continues(finally, step);
                }
                _ => {}
            }
            at += 1;
        }
    }

    fn block(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut guards = 0usize;
        for n in cx.children_with_comments(node) {
            if let Some(lowered) = catch_lowering(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(lowered) = value_switch(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(lowered) = value_while(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(lowered) = orelse_block_guard(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(local) = local_function(cx, n) {
                out.push(local);
                continue;
            }
            if let Some(lowered) = value_if_guard(cx, n) {
                out.extend(lowered);
                continue;
            }
            if let Some(lowered) = return_if_payload(cx, n) {
                out.push(lowered);
                continue;
            }
            if let Some(switch) = return_switch(cx, n) {
                out.push(switch);
                continue;
            }
            let built = settle_orelse_controls(stmt(cx, n), &mut guards);
            out.push(keep_whole(cx, n, built));
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
    /// The pieces of `X catch |e| handler`: the attempted value, the error
    /// binding, and the handler's statements. None for the dismissing forms
    /// and for anything that does not read whole.
    fn catch_pieces(cx: &Cx, catch_node: Node<'_>) -> Option<(Expr, Option<String>, Vec<Stmt>)> {
        let parts = cx.children(catch_node);
        let left = parts.first().copied()?;
        // The dismissing forms read as the value; the expression arm has them.
        let text = cx.text(catch_node);
        let dismissed = {
            let handler = text.rsplit("catch").next().unwrap_or("").trim();
            handler == "unreachable" || handler == "{}"
        };
        if dismissed {
            return None;
        }
        let binding = parts.iter().find(|c| c.kind() == "payload").and_then(|p| {
            all(*p)
                .iter()
                .find(|c| c.kind() == "identifier")
                .map(|id| cx.text(*id))
        });
        let handler_node = parts
            .iter()
            .skip(1)
            .find(|c| c.is_named() && c.kind() != "payload")
            .copied()?;
        let handler = if is_body(handler_node) {
            body_of(cx, handler_node)
        } else {
            match stmt(cx, handler_node) {
                Stmt::Unsupported(_) => match expr(cx, handler_node) {
                    Expr::Unsupported(_) => return None,
                    value => vec![Stmt::Expr(value)],
                },
                read => vec![read],
            }
        };
        if handler.iter().any(has_unsupported_stmt) {
            return None;
        }
        let attempted = expr(cx, left);
        if matches!(attempted, Expr::Unsupported(_)) {
            return None;
        }
        Some((attempted, binding, handler))
    }

    /// `X catch |e| handler`, with a real handler, lowered to try/catch around
    /// the statement that holds it: a binding assigns inside the try, a bare
    /// call runs inside it, a return returns from it.
    fn catch_lowering(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        // Find the catch expression this statement is built around.
        #[derive(Clone)]
        enum Shape {
            Bind(String),
            Run,
            Return,
        }
        let (catch_node, shape) = match node.kind() {
            "variable_declaration" => {
                let text = cx.text(node);
                let declares = text.trim_start().starts_with("var ")
                    || text.trim_start().starts_with("const ");
                let parts = all(node);
                let value = after(&parts, "=", ";")?;
                if value.kind() != "catch_expression" {
                    return None;
                }
                let name = parts
                    .iter()
                    .find(|c| c.kind() == "identifier")
                    .map(|c| cx.text(*c))?;
                match (declares, name.as_str()) {
                    (true, _) => (value, Shape::Bind(name)),
                    // `_ = X catch handler;` runs the try for its effect.
                    (false, "_") => (value, Shape::Run),
                    _ => return None,
                }
            }
            "expression_statement" => {
                let inner = cx.children(node).into_iter().next()?;
                match inner.kind() {
                    "catch_expression" => (inner, Shape::Run),
                    // `return X catch handler;`: try to return X; on failure
                    // the handler answers instead.
                    "return_expression" => {
                        let value = cx.children(inner).into_iter().next()?;
                        if value.kind() != "catch_expression" {
                            return None;
                        }
                        (value, Shape::Return)
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };
        let (attempted, binding, handler) = catch_pieces(cx, catch_node)?;
        let source = cx.text(node);
        let line = cx.line(node);
        let catches = vec![Catch {
            binding,
            ty: None,
            body: handler,
        }];
        Some(match shape {
            Shape::Bind(name) => vec![
                Stmt::Let {
                    name: name.clone(),
                    ty: None,
                    value: None,
                    mutable: true,
                },
                Stmt::Try {
                    body: vec![Stmt::Assign {
                        target: Expr::Name(name),
                        value: attempted,
                    }],
                    catches,
                    finally: Vec::new(),
                    source,
                    line,
                },
            ],
            Shape::Run => vec![Stmt::Try {
                body: vec![Stmt::Expr(attempted)],
                catches,
                finally: Vec::new(),
                source,
                line,
            }],
            Shape::Return => vec![Stmt::Try {
                body: vec![Stmt::Return(Some(attempted))],
                catches,
                finally: Vec::new(),
                source,
                line,
            }],
        })
    }

    /// Does this statement or anything under it carry?
    fn has_unsupported_stmt(stmt: &Stmt) -> bool {
        if matches!(stmt, Stmt::Unsupported(_)) {
            return true;
        }
        if super::has_unsupported_expr(stmt) {
            return true;
        }
        match stmt {
            Stmt::If {
                then, otherwise, ..
            }
            | Stmt::IfPresent {
                then, otherwise, ..
            } => then.iter().chain(otherwise).any(has_unsupported_stmt),
            Stmt::While { body, .. }
            | Stmt::WhilePresent { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachIndexed { body, .. }
            | Stmt::Defer(body)
            | Stmt::ErrDefer(body)
            | Stmt::Block(body) => body.iter().any(has_unsupported_stmt),
            Stmt::Switch { arms, default, .. } => arms
                .iter()
                .flat_map(|(_, body)| body)
                .chain(default)
                .any(has_unsupported_stmt),
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => body
                .iter()
                .chain(catches.iter().flat_map(|c| c.body.iter()))
                .chain(finally)
                .any(has_unsupported_stmt),
            _ => false,
        }
    }

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
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let declared = Stmt::Let {
            name: name.clone(),
            ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
            value: None,
            mutable: true,
        };
        if let Some((subject, arms, default)) = switch_arm_values(cx, value) {
            let assign = |value: Expr| Stmt::Assign {
                target: Expr::Name(name.clone()),
                value,
            };
            return Some(vec![
                declared,
                Stmt::Switch {
                    subject,
                    arms: arms
                        .into_iter()
                        .map(|(literals, value)| (literals, vec![assign(value)]))
                        .collect(),
                    default: vec![assign(default)],
                },
            ]);
        }
        // The fuller shapes: payloads, variant tags, block bodies. The arms
        // run as statements, and an arm that ends in a value assigns it.
        if value.kind() == "switch_expression" {
            let Some(Stmt::Switch {
                subject,
                mut arms,
                mut default,
            }) = switch_stmt(cx, value)
            else {
                return None;
            };
            fn assign_tail(body: &mut [Stmt], name: &str) {
                if let Some(last) = body.last_mut() {
                    match last {
                        Stmt::Expr(_) => {
                            let Stmt::Expr(e) = std::mem::replace(last, Stmt::Break) else {
                                unreachable!("just matched");
                            };
                            *last = Stmt::Assign {
                                target: Expr::Name(name.to_string()),
                                value: e,
                            };
                        }
                        // A try's own value sits at the end of its body.
                        Stmt::Try { body: tried, .. } => assign_tail(tried, name),
                        _ => {}
                    }
                }
            }
            for (_, body) in arms.iter_mut() {
                assign_tail(body, &name);
                settle_arm_breaks(body, &name);
            }
            assign_tail(&mut default, &name);
            settle_arm_breaks(&mut default, &name);
            return Some(vec![
                declared,
                Stmt::Switch {
                    subject,
                    arms,
                    default,
                },
            ]);
        }
        None
    }

    /// `const x = while (it) |v| { … break e. … } else fallback;`: the loop produces a value by
    /// breaking with one, or the fallback on exhaustion. The lowering binds the fallback first
    /// and each valued break assigns. `const a, const b = value;`: the value binds once, and
    /// the names take its parts by position. The value may be a call, a labeled block, a
    /// switch, or an optional guarded by `orelse` control flow.
    fn tuple_let(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        let parts = all(node);
        let assign_at = parts.iter().position(|c| cx.text(*c) == "=")?;
        let names: Vec<String> = parts[..assign_at]
            .iter()
            .filter(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))
            .collect();
        if names.len() < 2 {
            return None;
        }
        let value = after(&parts, "=", ";")?;
        let tmp = "fr_tup".to_string();
        let mut lowered: Vec<Stmt> = match value.kind() {
            // A labeled block runs once and breaks with the tuple.
            "labeled_type_expression" => {
                let inner = cx
                    .children(value)
                    .into_iter()
                    .find(|c| c.kind() == "block")?;
                let mut stmts = body_of(cx, inner);
                // Settled before the run-once wrapper goes on, so the wrapper
                // is the break's consumer and not a loop to route around.
                settle_any_labeled_breaks(&mut stmts, &tmp);
                stmts.push(Stmt::Break);
                vec![
                    Stmt::Let {
                        name: tmp.clone(),
                        ty: None,
                        value: None,
                        mutable: true,
                    },
                    Stmt::While {
                        condition: Expr::Bool(true),
                        body: stmts,
                    },
                ]
            }
            // A switch selects the tuple; each arm assigns it.
            "switch_expression" => {
                let Stmt::Switch {
                    subject,
                    mut arms,
                    mut default,
                } = switch_stmt(cx, value)?
                else {
                    return None;
                };
                let retail = |body: &mut Vec<Stmt>| {
                    if let Some(Stmt::Expr(_)) = body.last() {
                        let Some(Stmt::Expr(e)) = body.pop() else {
                            unreachable!("just matched");
                        };
                        body.push(Stmt::Assign {
                            target: Expr::Name(tmp.clone()),
                            value: e,
                        });
                    }
                };
                for (_, body) in arms.iter_mut() {
                    retail(body);
                    settle_arm_breaks(body, &tmp);
                }
                retail(&mut default);
                settle_arm_breaks(&mut default, &tmp);
                vec![
                    Stmt::Let {
                        name: tmp.clone(),
                        ty: None,
                        value: None,
                        mutable: true,
                    },
                    Stmt::Switch {
                        subject,
                        arms,
                        default,
                    },
                ]
            }
            // Anything else, a call or an optional with an `orelse` guard,
            // binds directly; the guard settles like any other.
            _ => {
                let read = expr(cx, value);
                if matches!(read, Expr::Unsupported(_)) {
                    return None;
                }
                let mut guards = 0usize;
                vec![settle_orelse_controls(
                    Stmt::Let {
                        name: tmp.clone(),
                        ty: None,
                        value: Some(read),
                        mutable: false,
                    },
                    &mut guards,
                )]
            }
        };
        if lowered.iter().any(has_unsupported_stmt) {
            return None;
        }
        lowered.push(Stmt::TupleAssign {
            names,
            value: Expr::Name(tmp),
            declares: true,
            source: cx.text(node),
            line: cx.line(node),
        });
        Some(lowered)
    }

    /// `return if (opt) |v| e else { … };`: test, bind, and return on the
    /// present path; the else block speaks for itself.
    fn return_if_payload(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        if node.kind() != "expression_statement" {
            return None;
        }
        let returned = cx.children(node).into_iter().next()?;
        if returned.kind() != "return_expression" {
            return None;
        }
        let value = cx.children(returned).into_iter().next()?;
        if value.kind() != "if_expression" {
            return None;
        }
        let children = cx.children(value);
        let payload = children.iter().find(|c| c.kind() == "payload")?;
        let binding = cx
            .children(*payload)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(c))?;
        let condition = expr(cx, *children.first()?);
        if matches!(condition, Expr::Unsupported(_)) {
            return None;
        }
        let then_node = children
            .iter()
            .skip(1)
            .find(|c| !matches!(c.kind(), "payload" | "else_clause" | "comment"))?;
        let then = match is_body(*then_node) {
            true => body_of(cx, *then_node),
            false => vec![Stmt::Return(Some(expr(cx, *then_node)))],
        };
        let otherwise = children
            .iter()
            .find(|c| c.kind() == "else_clause")
            .and_then(|e| {
                cx.children(*e)
                    .into_iter()
                    .find(|c| !matches!(c.kind(), "payload" | "comment"))
            })
            .map(|b| match is_body(b) {
                true => body_of(cx, b),
                false => vec![Stmt::Return(Some(expr(cx, b)))],
            })
            .unwrap_or_default();
        let lowered = Stmt::IfPresent {
            binding,
            value: condition,
            then,
            otherwise,
        };
        (!has_unsupported_stmt(&lowered)).then_some(lowered)
    }

    /// `const x = if (opt) |v| e else fallback orelse control;`: bind through an if-present,
    /// the else settles into the binding. And the trailing orelse guards on null.
    fn value_if_guard(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        if node.kind() != "variable_declaration" {
            return None;
        }
        let text = cx.text(node);
        if !(text.trim_start().starts_with("var ") || text.trim_start().starts_with("const ")) {
            return None;
        }
        let parts = all(node);
        let value = after(&parts, "=", ";")?;
        // The optional trailing `orelse control` wraps the if.
        let (if_node, control) = match value.kind() {
            "if_expression" => (value, None),
            "binary_expression" if all(value).iter().any(|c| c.kind() == "orelse") => {
                let operands: Vec<Node> = cx
                    .children(value)
                    .into_iter()
                    .filter(|c| c.kind() != "orelse")
                    .collect();
                let [left, right] = operands.as_slice() else {
                    return None;
                };
                if left.kind() != "if_expression" {
                    return None;
                }
                let control = match right.kind() {
                    "break_expression" => Stmt::Break,
                    "continue_expression" => Stmt::Continue,
                    "return_expression" => {
                        Stmt::Return(cx.children(*right).first().map(|v| expr(cx, *v)))
                    }
                    _ => return None,
                };
                (*left, Some(control))
            }
            _ => return None,
        };
        let children = cx.children(if_node);
        let payload = children.iter().find(|c| c.kind() == "payload")?;
        let binding = cx
            .children(*payload)
            .into_iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(c))?;
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let condition = expr(cx, *children.first()?);
        if matches!(condition, Expr::Unsupported(_)) {
            return None;
        }
        let assign = |value: Expr| Stmt::Assign {
            target: Expr::Name(name.clone()),
            value,
        };
        let then_node = children
            .iter()
            .skip(1)
            .find(|c| !matches!(c.kind(), "payload" | "else_clause" | "comment"))?;
        let then = match is_body(*then_node) {
            true => return None,
            false => vec![assign(expr(cx, *then_node))],
        };
        let otherwise = children
            .iter()
            .find(|c| c.kind() == "else_clause")
            .and_then(|e| {
                cx.children(*e)
                    .into_iter()
                    .find(|c| !matches!(c.kind(), "payload" | "comment"))
            })
            .map(|b| match b.kind() {
                // A labeled block runs once; its valued breaks settle into
                // the binding.
                "labeled_type_expression" => {
                    let inner = cx.children(b).into_iter().find(|c| c.kind() == "block");
                    match inner {
                        Some(inner) => {
                            let mut stmts = body_of(cx, inner);
                            settle_any_labeled_breaks(&mut stmts, &name);
                            stmts.push(Stmt::Break);
                            vec![Stmt::While {
                                condition: Expr::Bool(true),
                                body: stmts,
                            }]
                        }
                        None => vec![Stmt::Unsupported(cx.unsupported(b))],
                    }
                }
                _ if is_body(b) => body_of(cx, b),
                _ => vec![assign(expr(cx, b))],
            })
            .unwrap_or_default();
        let mut lowered = vec![
            Stmt::Let {
                name: name.clone(),
                ty: None,
                value: None,
                mutable: true,
            },
            Stmt::IfPresent {
                binding,
                value: condition,
                then,
                otherwise,
            },
        ];
        if let Some(control) = control {
            lowered.push(Stmt::If {
                condition: Expr::Binary {
                    op: BinaryOp::Eq,
                    left: Box::new(Expr::Name(name)),
                    right: Box::new(Expr::Null),
                },
                then: vec![control],
                otherwise: Vec::new(),
            });
        }
        if lowered.iter().any(has_unsupported_stmt) {
            return None;
        }
        Some(lowered)
    }

    /// `const f = struct { fn f(…) … }.f;` declares a local function; the
    /// binding's name is the function's.
    fn local_function(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        if node.kind() != "variable_declaration" {
            return None;
        }
        let text = cx.text(node);
        if !text.trim_start().starts_with("const ") {
            return None;
        }
        let parts = all(node);
        let value = after(&parts, "=", ";")?;
        if value.kind() != "field_expression" {
            return None;
        }
        let pieces = cx.children(value);
        let [container, member] = pieces.as_slice() else {
            return None;
        };
        if container.kind() != "struct_declaration" {
            return None;
        }
        let wanted = cx.text(*member);
        let declared = cx
            .children(*container)
            .into_iter()
            .find(|c| c.kind() == "function_declaration")?;
        let mut f = function(cx, declared)?;
        if f.name != wanted {
            return None;
        }
        let bound = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        f.name = bound;
        Some(Stmt::LocalFunction(Box::new(f)))
    }

    /// `const x = v orelse { …; return; };`: bind the optional, and on null
    /// run the block, which leaves the scope.
    fn orelse_block_guard(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        if node.kind() != "variable_declaration" {
            return None;
        }
        let text = cx.text(node);
        if !(text.trim_start().starts_with("var ") || text.trim_start().starts_with("const ")) {
            return None;
        }
        let parts = all(node);
        let value = after(&parts, "=", ";")?;
        if value.kind() != "binary_expression" {
            return None;
        }
        if !all(value).iter().any(|c| c.kind() == "orelse") {
            return None;
        }
        let operands: Vec<Node> = cx
            .children(value)
            .into_iter()
            .filter(|c| c.kind() != "orelse")
            .collect();
        let [left, right] = operands.as_slice() else {
            return None;
        };
        if !matches!(right.kind(), "block" | "block_expression") {
            return None;
        }
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let bound = expr(cx, *left);
        if matches!(bound, Expr::Unsupported(_)) {
            return None;
        }
        let block_node = match right.kind() {
            "block" => *right,
            _ => cx.children(*right).first().copied()?,
        };
        let fallback = body_of(cx, block_node);
        if fallback.iter().any(has_unsupported_stmt) {
            return None;
        }
        Some(vec![
            Stmt::Let {
                name: name.clone(),
                ty: None,
                value: Some(bound),
                mutable: false,
            },
            Stmt::If {
                condition: Expr::Binary {
                    op: BinaryOp::Eq,
                    left: Box::new(Expr::Name(name)),
                    right: Box::new(Expr::Null),
                },
                then: fallback,
                otherwise: Vec::new(),
            },
        ])
    }

    fn value_while(cx: &Cx, node: Node<'_>) -> Option<Vec<Stmt>> {
        if node.kind() != "variable_declaration" {
            return None;
        }
        let text = cx.text(node);
        if !(text.trim_start().starts_with("var ") || text.trim_start().starts_with("const ")) {
            return None;
        }
        let parts = all(node);
        let value = after(&parts, "=", ";")?;
        if value.kind() != "while_expression" {
            return None;
        }
        let name = parts
            .iter()
            .find(|c| c.kind() == "identifier")
            .map(|c| cx.text(*c))?;
        let children = cx.children(value);
        // The `else` is a bare keyword here, and the fallback is the named
        // node after it.
        let every = all(value);
        let at_else = every.iter().position(|c| c.kind() == "else")?;
        let fallback = every[at_else + 1..]
            .iter()
            .find(|c| c.is_named())
            .map(|e| expr(cx, *e))?;
        if matches!(fallback, Expr::Unsupported(_)) {
            return None;
        }
        let mut body = children
            .iter()
            .find(|c| is_body(**c))
            .map(|b| body_of(cx, *b))?;
        settle_labeled_breaks(&mut body, "", Some(&name));
        let condition = children.first().copied()?;
        let payload = children
            .iter()
            .find(|c| c.kind() == "payload")
            .and_then(|p| {
                all(*p)
                    .iter()
                    .find(|c| c.kind() == "identifier")
                    .map(|id| cx.text(*id))
            });
        let looped = match payload {
            Some(binding) => Stmt::WhilePresent {
                binding,
                value: expr(cx, condition),
                body,
            },
            None => Stmt::While {
                condition: expr(cx, condition),
                body,
            },
        };
        Some(vec![
            Stmt::Let {
                name,
                ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
                value: Some(fallback),
                mutable: true,
            },
            looped,
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

    /// A switch in expression position, said as the ternary chain it selects:
    /// equality tests per label, comparisons per range, the else last.
    fn switch_value_expr(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        let children = cx.children(node);
        let subject = expr(cx, *children.first()?);
        if matches!(subject, Expr::Unsupported(_)) {
            return None;
        }
        let mut arms: Vec<(Expr, Expr)> = Vec::new();
        let mut fallback: Option<Expr> = None;
        for case in children.iter().skip(1) {
            if case.kind() == "comment" {
                continue;
            }
            if case.kind() != "switch_case" {
                return None;
            }
            let parts = all(*case);
            let arrow = parts.iter().position(|c| c.kind() == "=>")?;
            if parts[arrow + 1..].iter().any(|c| c.kind() == "payload") {
                return None;
            }
            let value_node = parts[arrow + 1..].iter().find(|c| c.is_named())?;
            let value = expr(cx, *value_node);
            if matches!(value, Expr::Unsupported(_)) {
                return None;
            }
            if parts[..arrow].iter().any(|c| c.kind() == "else") {
                fallback = Some(value);
                continue;
            }
            // The labels: literals, variant tags, and `a...b` ranges.
            let mut test: Option<Expr> = None;
            let mut at = 0;
            let labels: Vec<&Node> = parts[..arrow].iter().collect();
            while at < labels.len() {
                let this = labels[at];
                if !this.is_named() {
                    at += 1;
                    continue;
                }
                let ranged = labels
                    .get(at + 1)
                    .is_some_and(|c| c.kind() == "..." || c.kind() == "..");
                let one = if ranged {
                    let low = label_value(cx, *this)?;
                    let high = label_value(cx, **labels.get(at + 2)?)?;
                    at += 3;
                    Expr::Binary {
                        op: BinaryOp::And,
                        left: Box::new(Expr::Binary {
                            op: BinaryOp::Ge,
                            left: Box::new(subject.clone()),
                            right: Box::new(low),
                        }),
                        right: Box::new(Expr::Binary {
                            op: BinaryOp::Le,
                            left: Box::new(subject.clone()),
                            right: Box::new(high),
                        }),
                    }
                } else {
                    let label = label_value(cx, *this)?;
                    at += 1;
                    Expr::Binary {
                        op: BinaryOp::Eq,
                        left: Box::new(subject.clone()),
                        right: Box::new(label),
                    }
                };
                test = Some(match test {
                    None => one,
                    Some(prior) => Expr::Binary {
                        op: BinaryOp::Or,
                        left: Box::new(prior),
                        right: Box::new(one),
                    },
                });
            }
            arms.push((test?, value));
        }
        let mut answer = fallback?;
        for (test, value) in arms.into_iter().rev() {
            answer = Expr::Ternary {
                condition: Box::new(test),
                then: Box::new(value),
                otherwise: Box::new(answer),
            };
        }
        Some(answer)
    }

    /// One switch label as the value it compares against.
    fn label_value(cx: &Cx, node: Node<'_>) -> Option<Expr> {
        match node.kind() {
            "integer" | "float" | "string" | "char_literal" | "character" => Some(expr(cx, node)),
            "field_expression" => dot_literal(cx, node).map(Expr::Str),
            "error_type" => match expr(cx, node) {
                Expr::Field { name, .. } => Some(Expr::Str(name)),
                _ => None,
            },
            _ => None,
        }
    }

    /// `std.debug.assert(c)`, read as the check it is.    /// `std.debug.assert(c)`, read as the check it is.
    ///
    /// A condition the reader cannot take whole leaves the call as an ordinary
    /// expression, which the enclosing statement then carries.
    fn assert_call(cx: &Cx, node: Node<'_>) -> Option<Stmt> {
        let parts = cx.children(node);
        let callee = parts.first()?;
        if callee.kind() != "field_expression" {
            return None;
        }
        let path: String = cx
            .text(*callee)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if path != "std.debug.assert" {
            return None;
        }
        let arguments: Vec<Node> = parts
            .iter()
            .skip(1)
            .filter(|c| !c.kind().contains("comment"))
            .copied()
            .collect();
        let [condition] = arguments.as_slice() else {
            return None;
        };
        let read = expr(cx, *condition);
        (!matches!(read, Expr::Unsupported(_))).then_some(Stmt::Assert {
            condition: read,
            message: None,
        })
    }

    /// The statements inside a `block_expression`, or the one statement without braces.
    /// Rewrite every labeled break under these statements, whatever its label:
    /// a valued one assigns `target` and breaks; a bare one breaks. Used where
    /// an arm-local label has no other consumer.
    fn settle_any_labeled_breaks(stmts: &mut Vec<Stmt>, target: &str) {
        settle_labeled_breaks(stmts, "*", Some(target));
    }

    /// Settle a switch arm's labeled breaks. An arm that is a run-once loop,
    /// the labeled-block lowering, settles inside the loop, which is the
    /// break's consumer; anything else settles as written.
    fn settle_arm_breaks(body: &mut Vec<Stmt>, target: &str) {
        if let [Stmt::While {
            condition: Expr::Bool(true),
            body: inner,
        }] = body.as_mut_slice()
        {
            settle_any_labeled_breaks(inner, target);
            return;
        }
        settle_any_labeled_breaks(body, target);
    }

    /// Rewrite every `break :label v` under these statements: into `target = v`
    /// followed by `break` when a target is given, a plain `break` otherwise.
    /// Other labels stay for their own blocks to consume.
    fn settle_labeled_breaks(stmts: &mut Vec<Stmt>, label: &str, target: Option<&str>) {
        /// Does anything under here break with this label?
        fn breaks_with(stmts: &[Stmt], label: &str) -> bool {
            stmts.iter().any(|stmt| match stmt {
                Stmt::BreakWith { label: l, .. } => l == label || label == "*",
                Stmt::If {
                    then, otherwise, ..
                }
                | Stmt::IfPresent {
                    then, otherwise, ..
                } => breaks_with(then, label) || breaks_with(otherwise, label),
                Stmt::While { body, .. }
                | Stmt::WhilePresent { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachIndexed { body, .. }
                | Stmt::Block(body)
                | Stmt::Defer(body)
                | Stmt::ErrDefer(body) => breaks_with(body, label),
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    breaks_with(body, label)
                        || catches.iter().any(|c| breaks_with(&c.body, label))
                        || breaks_with(finally, label)
                }
                Stmt::Switch { arms, default, .. } => {
                    arms.iter().any(|(_, b)| breaks_with(b, label)) || breaks_with(default, label)
                }
                _ => false,
            })
        }
        /// Rewrite matches; `flag` is raised beside each so an enclosing loop
        /// can be left too.
        fn rewrite(stmts: &mut Vec<Stmt>, label: &str, target: Option<&str>, flag: Option<&str>) {
            for stmt in stmts.iter_mut() {
                let matches_label = matches!(
                    stmt,
                    Stmt::BreakWith { label: l, .. } if l == label || label == "*"
                );
                if matches_label {
                    let Stmt::BreakWith { value, .. } = std::mem::replace(stmt, Stmt::Break) else {
                        unreachable!("just matched");
                    };
                    let mut routed = Vec::new();
                    if let (Some(target), Some(value)) = (target, value) {
                        routed.push(Stmt::Assign {
                            target: Expr::Name(target.to_string()),
                            value: *value,
                        });
                    }
                    if let Some(flag) = flag {
                        routed.push(Stmt::Assign {
                            target: Expr::Name(flag.to_string()),
                            value: Expr::Bool(true),
                        });
                    }
                    if !routed.is_empty() {
                        routed.push(Stmt::Break);
                        *stmt = Stmt::Block(routed);
                    }
                    continue;
                }
                match stmt {
                    Stmt::If {
                        then, otherwise, ..
                    }
                    | Stmt::IfPresent {
                        then, otherwise, ..
                    } => {
                        rewrite(then, label, target, flag);
                        rewrite(otherwise, label, target, flag);
                    }
                    // A nested loop's own `break` only leaves the loop, so the
                    // labeled break raises a flag the caller tests after it.
                    Stmt::While { .. }
                    | Stmt::WhilePresent { .. }
                    | Stmt::ForEach { .. }
                    | Stmt::ForEachIndexed { .. }
                    | Stmt::CountedFor { .. } => {}
                    Stmt::Block(body) | Stmt::Defer(body) | Stmt::ErrDefer(body) => {
                        rewrite(body, label, target, flag);
                    }
                    Stmt::Try {
                        body,
                        catches,
                        finally,
                        ..
                    } => {
                        rewrite(body, label, target, flag);
                        for catch in catches.iter_mut() {
                            rewrite(&mut catch.body, label, target, flag);
                        }
                        rewrite(finally, label, target, flag);
                    }
                    // A `break` inside a switch arm would leave the switch where
                    // the target writes one natively; it stays for the writer.
                    Stmt::Switch { .. } => {}
                    _ => {}
                }
            }
            // Loops whose bodies break with the label: the break becomes a
            // flag raise, and the flag leaves the enclosing scope after.
            let mut at = 0;
            while at < stmts.len() {
                let looped = matches!(
                    &stmts[at],
                    Stmt::While { body, .. }
                    | Stmt::WhilePresent { body, .. }
                    | Stmt::ForEach { body, .. }
                    | Stmt::ForEachIndexed { body, .. }
                        if breaks_with(body, label)
                ) || matches!(
                    &stmts[at],
                    Stmt::CountedFor { body, .. } if breaks_with(body, label)
                );
                if looped {
                    let flag_name = format!("fr_broke_{label}");
                    let body = match &mut stmts[at] {
                        Stmt::While { body, .. }
                        | Stmt::WhilePresent { body, .. }
                        | Stmt::ForEach { body, .. }
                        | Stmt::ForEachIndexed { body, .. }
                        | Stmt::CountedFor { body, .. } => body,
                        _ => unreachable!("just matched"),
                    };
                    rewrite(body, label, target, Some(&flag_name));
                    stmts.insert(
                        at,
                        Stmt::Let {
                            name: flag_name.clone(),
                            ty: None,
                            value: Some(Expr::Bool(false)),
                            mutable: true,
                        },
                    );
                    stmts.insert(
                        at + 2,
                        Stmt::If {
                            condition: Expr::Name(flag_name),
                            then: vec![Stmt::Break],
                            otherwise: Vec::new(),
                        },
                    );
                    at += 3;
                    continue;
                }
                at += 1;
            }
        }
        rewrite(stmts, label, target, None);
    }

    fn body_of(cx: &Cx, node: Node<'_>) -> Vec<Stmt> {
        match node.kind() {
            // A braced body arrives wrapped, and an `else { … }` arrives wrapped twice. The
            // grammar treats every block as labelable whether or not it carries a label.
            "block" => block(cx, node),
            // A labeled body, `if (c) blk: { … break :blk; … }`, runs once,
            // and each `break :blk` leaves it early: the run-once loop.
            "block_expression" | "labeled_statement" | "labeled_type_expression" => {
                let label = cx
                    .children(node)
                    .iter()
                    .find(|c| c.kind() == "block_label")
                    .and_then(|l| cx.children(*l).first().map(|id| cx.text(*id)));
                let inner = cx.children(node).into_iter().find(|c| c.kind() == "block");
                match (label, inner) {
                    (Some(label), Some(inner)) => {
                        let mut stmts = block(cx, inner);
                        settle_labeled_breaks(&mut stmts, &label, None);
                        stmts.push(Stmt::Break);
                        vec![Stmt::While {
                            condition: Expr::Bool(true),
                            body: stmts,
                        }]
                    }
                    (None, Some(inner)) => block(cx, inner),
                    _ => vec![keep_whole(cx, node, stmt(cx, node))],
                }
            }
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
                // `unreachable;` has no children: the statement is the claim
                // itself, and reaching it is the failure it throws everywhere.
                None if cx.text(node).starts_with("unreachable") => {
                    Stmt::Throw(Expr::Str("unreachable".to_string()))
                }
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "return_expression" => Stmt::Return(cx.children(node).first().map(|e| expr(cx, *e))),
            // `break` leaves the loop; `break :label v` leaves the labeled
            // block with a value, and the labeled-block lowering consumes it.
            "break_expression" => {
                let parts = cx.children(node);
                let label = parts
                    .iter()
                    .find(|c| c.kind() == "break_label")
                    .and_then(|l| cx.children(*l).first().map(|id| cx.text(*id)));
                let value = parts
                    .iter()
                    .find(|c| c.kind() != "break_label")
                    .map(|v| expr(cx, *v));
                match (label, value) {
                    (None, None) => Stmt::Break,
                    // `break v` gives the enclosing value-loop its value; the
                    // loop's own lowering consumes it.
                    (None, Some(value)) => Stmt::BreakWith {
                        label: String::new(),
                        value: Some(Box::new(value)),
                    },
                    (Some(label), value) => Stmt::BreakWith {
                        label,
                        value: value.map(Box::new),
                    },
                }
            }
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
                    return tuple_let(cx, node)
                        .map(Stmt::Block)
                        .unwrap_or_else(|| Stmt::Unsupported(cx.unsupported(node)));
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
                    // A write through a pointer lands on what it points at: the dereference
                    // unwraps. And a pointer capture in a loop has already been rewritten to
                    // the element it walks. The receiver itself cannot be rebound in half the
                    // targets, so `self.* = …` stays carried.
                    if target.kind() == "dereference_expression"
                        && cx
                            .children(target)
                            .first()
                            .is_some_and(|inner| cx.text(*inner) == "self")
                    {
                        return Stmt::Comment(format!(
                            "{}: the debug poison has nothing to mark here",
                            cx.text(node).trim_end_matches(';')
                        ));
                    }
                    return Stmt::Assign {
                        target: expr(cx, target),
                        value: expr(cx, value),
                    };
                }
                // `const x = blk: { ... break :blk v; };` runs the block once
                // and takes the break's value. The lowering declares x, loops
                // once, and each labeled break assigns and leaves.
                if value.kind() == "labeled_type_expression" {
                    let bound = cx.text(target);
                    let vparts = cx.children(value);
                    let label = vparts
                        .iter()
                        .find(|c| c.kind() == "block_label")
                        .and_then(|l| cx.children(*l).first().map(|id| cx.text(*id)));
                    let inner = vparts.iter().find(|c| c.kind() == "block").copied();
                    if let (Some(label), Some(inner)) = (label, inner) {
                        let mut stmts = body_of(cx, inner);
                        settle_labeled_breaks(&mut stmts, &label, Some(&bound));
                        stmts.push(Stmt::Break);
                        return Stmt::Block(vec![
                            Stmt::Let {
                                name: bound,
                                ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
                                value: None,
                                mutable: true,
                            },
                            Stmt::While {
                                condition: Expr::Bool(true),
                                body: stmts,
                            },
                        ]);
                    }
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                // An error set has no value to bind. The alias keeps the set's spelling as
                // text, which is all a target without error sets can hold of it.
                if contains_error_set(value) {
                    return Stmt::Let {
                        name: cx.text(target),
                        ty: None,
                        value: Some(Expr::Str(cx.text(value))),
                        mutable: false,
                    };
                }
                let mut read = expr(cx, value);
                // A dot literal names a member of the declared type, written with
                // the type left to inference: `var list: ArrayList(u8) = .empty;`
                // means `ArrayList(u8).empty`, `.init(x)` means the type's `init`,
                // and `.{ .a = 1 }` builds the type by naming its fields. The
                // annotation says what to qualify each with.
                if let Some(annotated) = after(&parts, ":", "=") {
                    read = qualify_dot_literal(read, cx.text(annotated).trim());
                }
                Stmt::Let {
                    name: cx.text(target),
                    ty: after(&parts, ":", "=").map(|t| ty_of(cx, t)),
                    value: Some(read),
                    mutable: text.trim_start().starts_with("var "),
                }
            }
            // A statement-position block, labeled or not, and the labeled
            // loop. A bare block groups and scopes; a labeled one may be left
            // early by `break :label`, which the loop lowering makes a `break`.
            "labeled_statement" => {
                let parts = cx.children(node);
                let label = parts
                    .iter()
                    .find(|c| c.kind() == "block_label")
                    .and_then(|l| cx.children(*l).first().map(|id| cx.text(*id)));
                let Some(inner) = parts.iter().find(|c| c.kind() != "block_label").copied() else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                match (inner.kind(), label) {
                    ("block", None) => Stmt::Block(body_of(cx, inner)),
                    ("block", Some(label)) => {
                        // Run once; `break :label` becomes the loop's own break.
                        let mut stmts = body_of(cx, inner);
                        settle_labeled_breaks(&mut stmts, &label, None);
                        stmts.push(Stmt::Break);
                        Stmt::While {
                            condition: Expr::Bool(true),
                            body: stmts,
                        }
                    }
                    (_, label) => {
                        let inner = stmt(cx, inner);
                        match label {
                            None => inner,
                            Some(label) => {
                                let mut wrapped = vec![inner];
                                settle_labeled_breaks(&mut wrapped, &label, None);
                                wrapped.pop().expect("the one statement put in")
                            }
                        }
                    }
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
                // `if (maybe) |value| { … }` tests an optional and binds its payload. A
                // `|*value|` pointer capture writes through the original, and an error union's
                // `else |err|` binds a second payload. Neither has a crossing, so both carry
                // whole.
                if let Some(payload) = children.iter().find(|c| c.kind() == "payload") {
                    let bindings: Vec<Node> = cx
                        .children(*payload)
                        .into_iter()
                        .filter(|c| c.kind() == "identifier")
                        .collect();
                    let by_pointer = all(*payload).iter().any(|c| c.kind() == "*");
                    let else_payload = else_clause.and_then(|e| {
                        let mut cursor = e.walk();
                        let found = e.children(&mut cursor).find(|c| c.kind() == "payload");
                        found
                    });
                    // `if (f(x)) |v| { … } else |e| { … }` branches on an error
                    // union, which is this language's try/catch.
                    if let (Some(else_payload), [binding], false) =
                        (else_payload, bindings.as_slice(), by_pointer)
                    {
                        let caught = cx
                            .children(else_payload)
                            .into_iter()
                            .find(|c| c.kind() == "identifier")
                            .map(|c| cx.text(c));
                        let value = children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null);
                        let mut tried = vec![match cx.text(*binding).as_str() {
                            "_" => Stmt::Expr(value),
                            bound => Stmt::Let {
                                name: bound.to_string(),
                                ty: None,
                                value: Some(value),
                                mutable: false,
                            },
                        }];
                        tried.extend(then);
                        return Stmt::Try {
                            body: tried,
                            catches: vec![Catch {
                                binding: caught,
                                ty: None,
                                body: otherwise,
                            }],
                            finally: Vec::new(),
                            source: cx.text(node),
                            line: cx.line(node),
                        };
                    }
                    let else_binds = else_payload.is_some();
                    if let ([binding], false) = (bindings.as_slice(), else_binds) {
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
                // `while (it.next()) |item|` loops on an optional's payload. A `|*item|`
                // pointer capture writes through the original, a continue-expression (`: (i +=
                // 1)`) has no slot, and an `else` here runs on exhaustion. None of the three
                // crosses. `: (i += 1)` runs after each pass. The lowering runs it at the
                // bottom of the body and before each continue.
                let mut body = body;
                if stepped {
                    let all_parts = all(node);
                    let colon = all_parts.iter().position(|c| c.kind() == ":");
                    let step_node = colon
                        .and_then(|at| all_parts[at + 1..].iter().find(|c| c.is_named()).copied());
                    let Some(step_node) = step_node else {
                        return Stmt::Unsupported(cx.unsupported(node));
                    };
                    let step = match stmt(cx, step_node) {
                        Stmt::Unsupported(_) => return Stmt::Unsupported(cx.unsupported(node)),
                        read => read,
                    };
                    step_before_continues(&mut body, &step);
                    body.push(step);
                }
                if let Some(payload) = children.iter().find(|c| c.kind() == "payload") {
                    let bindings: Vec<Node> = cx
                        .children(*payload)
                        .into_iter()
                        .filter(|c| c.kind() == "identifier")
                        .collect();
                    let by_pointer = all(*payload).iter().any(|c| c.kind() == "*");
                    let has_else = children.iter().any(|c| c.kind() == "else_clause");
                    if let ([binding], false, false) = (bindings.as_slice(), by_pointer, has_else) {
                        return Stmt::WhilePresent {
                            binding: cx.text(*binding),
                            value: children.first().map(|c| expr(cx, *c)).unwrap_or(Expr::Null),
                            body,
                        };
                    }
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
                let _ = &bindings;
                // Ordered captures, `*name` marked: a pointer capture walks its
                // element in place, and rewrites to an index below.
                let capture_parts = all(*payload);
                let mut captures: Vec<(String, bool)> = Vec::new();
                let mut pointered = false;
                for part in &capture_parts {
                    match part.kind() {
                        "*" => pointered = true,
                        "identifier" => {
                            captures.push((cx.text(*part), pointered));
                            pointered = false;
                        }
                        _ => {}
                    }
                }
                if let ([seq], [(name, false)]) = (sequences.as_slice(), captures.as_slice()) {
                    return Stmt::ForEach {
                        binding: name.clone(),
                        iterable: expr(cx, *seq),
                        body,
                    };
                }
                // The general zip: walk the first sequence by an index, the `0..` capture's
                // name when there is one. And read the others by the same index. A pointer
                // capture's name rewrites to the element it walks, so writes land in the
                // sequence.
                if sequences.len() == captures.len() && !sequences.is_empty() {
                    let index = sequences
                        .iter()
                        .zip(&captures)
                        .find(|(seq, _)| seq.kind() == "range_expression")
                        .map(|(_, (name, _))| name.clone())
                        .unwrap_or_else(|| "fr_i".to_string());
                    let mut prelude: Vec<Stmt> = Vec::new();
                    let mut renames: Vec<(String, Expr)> = Vec::new();
                    let mut first: Option<(String, Expr)> = None;
                    for (seq, (name, by_pointer)) in sequences.iter().zip(&captures) {
                        if seq.kind() == "range_expression" {
                            continue;
                        }
                        let walked = expr(cx, *seq);
                        let element = Expr::Index {
                            of: Box::new(walked.clone()),
                            index: Box::new(Expr::Name(index.clone())),
                        };
                        match (&first, by_pointer) {
                            (None, false) => first = Some((name.clone(), walked)),
                            (None, true) => {
                                first = Some(("fr_elem".to_string(), walked));
                                renames.push((name.clone(), element));
                            }
                            (Some(_), false) => prelude.push(Stmt::Let {
                                name: name.clone(),
                                ty: None,
                                value: Some(element),
                                mutable: false,
                            }),
                            (Some(_), true) => renames.push((name.clone(), element)),
                        }
                    }
                    let Some((binding, iterable)) = first else {
                        return Stmt::Unsupported(cx.unsupported(node));
                    };
                    let mut body = body;
                    if !renames.is_empty() {
                        super::each_expr_in_stmts(&mut body, &mut |e| {
                            if let Expr::Name(n) = e {
                                if let Some((_, element)) =
                                    renames.iter().find(|(from, _)| from == n)
                                {
                                    *e = element.clone();
                                }
                            }
                        });
                    }
                    let mut full = prelude;
                    full.extend(body);
                    return Stmt::ForEachIndexed {
                        index,
                        binding,
                        iterable,
                        body: full,
                    };
                }
                Stmt::Unsupported(cx.unsupported(node))
            }
            // `std.debug.assert(c)` is the language's own check, and it crosses
            // as the assert it is instead of a call through a path no target
            // declares.
            "call_expression" => match assert_call(cx, node) {
                Some(check) => check,
                None => Stmt::Expr(expr(cx, node)),
            },
            // `comptime stmt` runs at compile time there; the translation has
            // only runtime, so it runs then. The check still checks.
            "comptime_statement" | "comptime_expression" => match cx.children(node).first() {
                Some(inner) => stmt(cx, *inner),
                None if cx.text(node).contains("unreachable") => {
                    Stmt::Throw(Expr::Str("unreachable".to_string()))
                }
                None => Stmt::Unsupported(cx.unsupported(node)),
            },
            "unreachable" => Stmt::Throw(Expr::Str("unreachable".to_string())),
            "field_expression" | "identifier" | "try_expression" | "catch_expression" => {
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
                    // `if (c) t else null`: the null is a keyword, not a named
                    // child, so the else operand is on the side.
                    [condition, then]
                        if !is_body(*then)
                            && all(node)
                                .iter()
                                .any(|c| !c.is_named() && cx.text(*c) == "null") =>
                    {
                        Expr::Ternary {
                            condition: Box::new(expr(cx, *condition)),
                            then: Box::new(expr(cx, *then)),
                            otherwise: Box::new(Expr::Null),
                        }
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // A switch used as a value selects by comparison.
            "switch_expression" => match switch_value_expr(cx, node) {
                Some(selected) => selected,
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `comptime e` evaluates there at compile time; here it is the value.
            "comptime_expression" => match cx.children(node).first() {
                Some(inner) => expr(cx, *inner),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "integer" => Expr::Int(cx.text(node)),
            "float" => Expr::Float(cx.text(node)),
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            // The grammar wraps the keyword in a `boolean` node.
            "boolean" => Expr::Bool(cx.text(node) == "true"),
            // `'@'` is a number there; the character is what it names, and the
            // one-character string is what every target can compare.
            "character" => Expr::Str(super::unquote(&cx.text(node)).replace("\\n", "\n")),
            "null" | "undefined" => Expr::Null,
            "string" => Expr::Str(super::unquote(&cx.text(node))),
            "identifier" | "builtin_type" => Expr::Name(cx.text(node)),
            // `X{ .field = v }` builds X by naming its fields;
            // `[_]u32{ 1, 2, 3 }` over an array type is a list.
            "struct_initializer" => {
                let parts = cx.children(node);
                if let [ty, items] = parts.as_slice() {
                    if ty.kind() == "array_type" && items.kind() == "initializer_list" {
                        return Expr::ListLit(
                            cx.children(*items).iter().map(|i| expr(cx, *i)).collect(),
                        );
                    }
                }
                let named = parts.first().and_then(|c| match c.kind() {
                    "identifier" | "field_expression" => Some(cx.text(*c)),
                    // `lsp.T(?void){ … }`: the comptime arguments are type
                    // syntax no target can hold; the path names the type.
                    "call_expression" => cx.children(*c).first().map(|f| cx.text(*f)),
                    _ => None,
                });
                let Some(named) = named else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                let fields: Vec<(String, Expr)> = parts
                    .iter()
                    .find(|c| c.kind() == "initializer_list")
                    .map(|list| {
                        cx.children(*list)
                            .into_iter()
                            .filter(|c| c.kind() == "assignment_expression")
                            .filter_map(|a| variant_field(cx, a))
                            .map(|(name, value)| (name, expr(cx, value)))
                            .collect()
                    })
                    .unwrap_or_default();
                // `Box{ .value = 9 }` builds a record, which the IR has a node
                // for. Written as a call taking keywords, the targets that
                // build a record through a constructor got an object literal
                // where an argument list belonged. A dotted name belongs to
                // another module and this file declares no fields for it, so
                // it carries as the construction it is.
                match named.contains('.') {
                    false => Expr::RecordLit { ty: named, fields },
                    true => Expr::New {
                        callee: Box::new(Expr::Name(named)),
                        args: fields
                            .into_iter()
                            .map(|(name, value)| Expr::Keyword {
                                name,
                                value: Box::new(value),
                            })
                            .collect(),
                    },
                }
            }
            // A type in argument position names itself; the pointer wrapper
            // has nothing to add where every value is a reference.
            "pointer_type" => match cx.children(node).first() {
                Some(inner) => expr(cx, *inner),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            "field_expression" => {
                let parts = cx.children(node);
                match (parts.first(), parts.last()) {
                    (Some(of), Some(name)) if parts.len() >= 2 => Expr::Field {
                        of: Box::new(expr(cx, *of)),
                        name: cx.text(*name),
                    },
                    // `.foo` with no object: a member of whatever the position
                    // expects. The settle pass attributes it to a sum where one
                    // answers; unattributed it stays a bare tag.
                    (Some(member), _)
                        if parts.len() == 1
                            && member.kind() == "identifier"
                            && cx.text(node).starts_with('.') =>
                    {
                        Expr::Variant {
                            sum: String::new(),
                            name: cx.text(*member),
                            fields: Vec::new(),
                        }
                    }
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
                // The word operators (`and`, `or`, `orelse`) are named nodes in
                // this grammar; the symbol ones are punctuation.
                let word_op = |c: &Node| matches!(c.kind(), "and" | "or" | "orelse");
                let operator = parts
                    .iter()
                    .find(|c| !c.is_named() || word_op(c))
                    .map(|c| cx.text(*c))
                    .unwrap_or_default();
                let operands: Vec<Node> = parts
                    .iter()
                    .filter(|c| c.is_named() && !word_op(c))
                    .copied()
                    .collect();
                // `a orelse b` is Zig's word for exactly the question `??` asks.
                // `a orelse return`/`break`/`continue` guards instead: the
                // fallback is control flow, encoded for the statement builder
                // to unfold into a binding and an if.
                if operator == "++" && operands.len() == 2 {
                    return Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(expr(cx, operands[0])),
                        right: Box::new(expr(cx, operands[1])),
                    };
                }
                if operator == "orelse" && operands.len() == 2 {
                    let fallback = match operands[1].kind() {
                        "return_expression" => {
                            let value: Vec<Expr> = cx
                                .children(operands[1])
                                .into_iter()
                                .map(|v| expr(cx, v))
                                .collect();
                            Expr::Call {
                                callee: Box::new(Expr::Name("__fr_orelse_return".into())),
                                args: value,
                            }
                        }
                        "break_expression" => {
                            let label: Vec<Expr> = cx
                                .children(operands[1])
                                .iter()
                                .filter(|c| c.kind() == "break_label")
                                .filter_map(|l| cx.children(*l).first().copied())
                                .map(|id| Expr::Str(cx.text(id)))
                                .collect();
                            Expr::Call {
                                callee: Box::new(Expr::Name("__fr_orelse_break".into())),
                                args: label,
                            }
                        }
                        "continue_expression" => Expr::Call {
                            callee: Box::new(Expr::Name("__fr_orelse_continue".into())),
                            args: Vec::new(),
                        },
                        _ => expr(cx, operands[1]),
                    };
                    return Expr::Coalesce {
                        value: Box::new(expr(cx, operands[0])),
                        fallback: Box::new(fallback),
                    };
                }
                // `null` is a keyword here, not a named node, so a null
                // comparison has one named operand and the keyword on the side.
                let null_side = parts.iter().any(|c| !c.is_named() && cx.text(*c) == "null");
                match super::binary_op(&operator) {
                    Some(op) if operands.len() == 2 => Expr::Binary {
                        op,
                        left: Box::new(expr(cx, operands[0])),
                        right: Box::new(expr(cx, operands[1])),
                    },
                    Some(op) if operands.len() == 1 && null_side => Expr::Binary {
                        op,
                        left: Box::new(expr(cx, operands[0])),
                        right: Box::new(Expr::Null),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // The grammar reads `!x` in value position as an error-union
            // type; with one operand it is the negation it looks like.
            "error_union_type" => {
                let named: Vec<Node> = cx.children(node);
                let bang = all(node).first().map(|c| c.kind() == "!").unwrap_or(false);
                match (named.as_slice(), bang) {
                    ([operand], true) => Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(expr(cx, *operand)),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `?T` in value position: the type it wraps, said plainly.
            "optional_type" => match cx.children(node).first() {
                Some(inner) => expr(cx, *inner),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // A parenthesised expression is its inside.
            "parenthesized_expression" => match cx.children(node).first() {
                Some(inner) => expr(cx, *inner),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `p.*` reads the value the pointer holds; targets without
            // pointers hold the value itself.
            "dereference_expression" => match cx.children(node).first() {
                Some(inner) => expr(cx, *inner),
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `x.?` asserts the optional holds a value and uses it.
            "null_coercion_expression" => match cx.children(node).first() {
                Some(inner) => Expr::Unary {
                    op: UnaryOp::Unwrap,
                    operand: Box::new(expr(cx, *inner)),
                },
                None => Expr::Unsupported(cx.unsupported(node)),
            },
            // `\\line` continuation lines, joined by the newlines they imply.
            "multiline_string" => {
                let text = cx.text(node);
                let joined: Vec<&str> = text
                    .lines()
                    .map(|l| l.trim_start().trim_start_matches("\\\\"))
                    .collect();
                Expr::Str(joined.join("\n"))
            }
            // `{}` in value position is the void value.
            "block" if cx.children(node).is_empty() => Expr::Null,
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
            "index_expression" => {
                let parts = cx.children(node);
                match parts.as_slice() {
                    [of, index]
                        if index.kind() != "range_expression"
                            && !(index.kind() == "binary_expression"
                                && cx
                                    .children(*index)
                                    .first()
                                    .is_some_and(|c| c.kind() == "range_expression")) =>
                    {
                        Expr::Index {
                            of: Box::new(expr(cx, *of)),
                            index: Box::new(expr(cx, *index)),
                        }
                    }
                    [of, index]
                        if index.kind() == "binary_expression"
                            && cx
                                .children(*index)
                                .first()
                                .is_some_and(|c| c.kind() == "range_expression") =>
                    {
                        let of_expr = expr(cx, *of);
                        let pieces = cx.children(*index);
                        let [range, rhs] = pieces.as_slice() else {
                            return Expr::Unsupported(cx.unsupported(node));
                        };
                        let op = all(*index)
                            .iter()
                            .find(|c| !c.is_named())
                            .map(|c| cx.text(*c))
                            .and_then(|op| super::binary_op(&op));
                        let bounds = cx.children(*range);
                        let ([from, to], Some(op)) = (bounds.as_slice(), op) else {
                            return Expr::Unsupported(cx.unsupported(node));
                        };
                        Expr::Call {
                            callee: Box::new(Expr::Name("slice".to_string())),
                            args: vec![
                                of_expr,
                                expr(cx, *from),
                                Expr::Binary {
                                    op,
                                    left: Box::new(expr(cx, *to)),
                                    right: Box::new(expr(cx, *rhs)),
                                },
                            ],
                        }
                    }
                    // `s[a .. b]` is the canonical slice; an open end runs to
                    // the length.
                    [of, range] if range.kind() == "range_expression" => {
                        let of_expr = expr(cx, *of);
                        let bounds = cx.children(*range);
                        let text = cx.text(*range);
                        let (from, to) = match bounds.as_slice() {
                            [a, b] => (expr(cx, *a), expr(cx, *b)),
                            [one] if text.trim_start().starts_with("..") => {
                                (Expr::Int("0".to_string()), expr(cx, *one))
                            }
                            [one] => (
                                expr(cx, *one),
                                Expr::Call {
                                    callee: Box::new(Expr::Name("len".to_string())),
                                    args: vec![of_expr.clone()],
                                },
                            ),
                            _ => return Expr::Unsupported(cx.unsupported(node)),
                        };
                        Expr::Call {
                            callee: Box::new(Expr::Name("slice".to_string())),
                            args: vec![of_expr, from, to],
                        }
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `.{ .one = n }` builds a variant of whatever union the position
            // expects. Which union is settled at the end of the module, where
            // the sums are known. A candidate no sum answers for goes back to
            // being carried.
            "anonymous_struct_initializer" => {
                let assignments: Vec<Node> = cx
                    .children(node)
                    .iter()
                    .find(|c| c.kind() == "initializer_list")
                    .map(|list| {
                        cx.children(*list)
                            .into_iter()
                            .filter(|c| c.kind() == "assignment_expression")
                            .collect()
                    })
                    .unwrap_or_default();

                // `.{ a, b }`: no assignments, only positions. The tuple of its
                // values, which is how a Zig format call carries its arguments.
                let positional: Vec<Node> = cx
                    .children(node)
                    .iter()
                    .find(|c| c.kind() == "initializer_list")
                    .map(|list| {
                        cx.children(*list)
                            .into_iter()
                            .filter(|c| c.is_named() && !c.kind().contains("comment"))
                            .collect()
                    })
                    .unwrap_or_default();
                if assignments.is_empty() {
                    return Expr::Tuple(positional.iter().map(|v| expr(cx, *v)).collect());
                }

                match assignments.as_slice() {
                    [one] => match variant_field(cx, *one) {
                        Some((name, value)) => {
                            let fields = match value.kind() {
                                // `{}` is void: the variant is a bare tag.
                                "block" if cx.children(value).is_empty() => Vec::new(),
                                // A nested anonymous initializer is the payload's
                                // own fields, laid out flat.
                                "anonymous_struct_initializer" => match expr(cx, value) {
                                    Expr::Variant {
                                        name: f, fields, ..
                                    } if fields.len() == 1 => {
                                        vec![(
                                            f,
                                            fields
                                                .into_iter()
                                                .next()
                                                .map(|(_, v)| v)
                                                .unwrap_or(Expr::Null),
                                        )]
                                    }
                                    Expr::RecordLit { fields, .. } => fields,
                                    other => vec![("value".to_string(), other)],
                                },
                                _ => vec![("value".to_string(), expr(cx, value))],
                            };
                            Expr::Variant {
                                sum: String::new(),
                                name,
                                fields,
                            }
                        }
                        None => Expr::Unsupported(cx.unsupported(node)),
                    },
                    // Several assignments are a record built anonymously. The
                    // annotation on the binding names its type; without one it
                    // settles to a map of its fields.
                    many => Expr::RecordLit {
                        ty: String::new(),
                        fields: many
                            .iter()
                            .filter_map(|a| variant_field(cx, *a))
                            .map(|(name, value)| (name, expr(cx, value)))
                            .collect(),
                    },
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
                    ("@as", [_ty]) if cx.text(node).replace(' ', "").ends_with(",null)") => {
                        Expr::Null
                    }
                    ("@as" | "@intCast" | "@floatCast" | "@truncate", [ty, value]) => {
                        let ty = expr(cx, *ty);
                        let value = expr(cx, *value);
                        match ty {
                            // A type spelling with no expression form: the
                            // value stands alone, its type re-asserted by the
                            // context that already knows it.
                            Expr::Unsupported(_) => value,
                            ty => Expr::Cast {
                                ty: Box::new(ty),
                                value: Box::new(value),
                            },
                        }
                    }
                    ("@min" | "@max", _) => Expr::Call {
                        callee: Box::new(Expr::Name(name.trim_start_matches('@').to_string())),
                        args: args.iter().map(|a| expr(cx, *a)).collect(),
                    },
                    // The error's name is its message everywhere else.
                    ("@errorName", [value]) => Expr::Call {
                        callee: Box::new(Expr::Name("str".to_string())),
                        args: vec![expr(cx, *value)],
                    },
                    // Zig spells the division and remainder the other five spell as operators.
                    // `@mod` is Euclidean where `%` truncates. They agree wherever the operands
                    // are non-negative, and a program for which that differs is telling every
                    // target something Zig-shaped.
                    // The two roundings are different operators, and reading
                    // both as one made every negative quotient wrong.
                    ("@divTrunc", [left, right]) => Expr::Binary {
                        op: BinaryOp::Div,
                        left: Box::new(expr(cx, *left)),
                        right: Box::new(expr(cx, *right)),
                    },
                    ("@divFloor", [left, right]) => Expr::Binary {
                        op: BinaryOp::FloorDiv,
                        left: Box::new(expr(cx, *left)),
                        right: Box::new(expr(cx, *right)),
                    },
                    ("@rem", [left, right]) => Expr::Binary {
                        op: BinaryOp::Rem,
                        left: Box::new(expr(cx, *left)),
                        right: Box::new(expr(cx, *right)),
                    },
                    ("@mod", [left, right]) => Expr::Binary {
                        op: BinaryOp::FloorRem,
                        left: Box::new(expr(cx, *left)),
                        right: Box::new(expr(cx, *right)),
                    },                    // The single-argument casts reassert a type the annotation
                    // already names; the value itself is what crosses.
                    (
                        "@intCast" | "@truncate" | "@enumFromInt" | "@intFromEnum" | "@intFromBool"
                        | "@errorCast" | "@ptrCast" | "@constCast",
                        [value],
                    ) => expr(cx, *value),
                    ("@intFromFloat", [value]) => Expr::Call {
                        callee: Box::new(Expr::Name("int".to_string())),
                        args: vec![expr(cx, *value)],
                    },
                    ("@floatFromInt", [value]) => Expr::Call {
                        callee: Box::new(Expr::Name("float".to_string())),
                        args: vec![expr(cx, *value)],
                    },
                    // A source location is a description of a place; the line is
                    // the part every target can hold.
                    ("@src", []) => Expr::Str(format!("line {}", cx.line(node))),
                    ("@typeName", [ty]) => Expr::Str(cx.text(*ty)),
                    // `@import("x").Y` reaches into a module by its stem, and the
                    // stem is how the other languages name an imported module.
                    ("@import", [path]) => {
                        let stem = cx
                            .text(*path)
                            .trim_matches('"')
                            .rsplit('/')
                            .next()
                            .unwrap_or_default()
                            .trim_end_matches(".zig")
                            .to_string();
                        Expr::Name(stem)
                    }
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
            // `X catch unreachable` and `X catch {}` assert the failure away; the
            // value is X. A catch with a real handler stays carried for now.
            "catch_expression" => {
                let text = cx.text(node);
                let handler = text.rsplit("catch").next().unwrap_or("").trim();
                let dismissed = handler == "unreachable" || handler == "{}";
                match (cx.children(node).first(), dismissed) {
                    (Some(left), true) => expr(cx, *left),
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            // `error.x` is the failure value itself, spelled as the field the
            // settle pass reads: `Err` when it crosses a `return`.
            "error_type" => Expr::Field {
                of: Box::new(Expr::Name("error".to_string())),
                name: cx
                    .children(node)
                    .into_iter()
                    .find(|c| c.kind() == "identifier")
                    .map(|c| cx.text(c))
                    .unwrap_or_default(),
            },
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
                            module.items.push(Item::Unsupported(cx.unsupported(child)));
                        } else if let Some(mut sum) = inline_union(cx, node) {
                            sum.doc = doc_above(cx, child, &["///", "//", "/**", "*/", "*"]);
                            sum.exported = exported;
                            module.items.push(Item::Sum(sum));
                        } else {
                            module.items.push(Item::Unsupported(cx.unsupported(child)));
                        }
                    }
                },
                "import_statement" => {
                    let text = cx.text(child);
                    let target = import_target(&text);
                    module.items.push(Item::Import {
                        text,
                        line: cx.line(child),
                        target,
                    })
                }
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
        settle_kind_literals(&mut module);
        settle_variant_narrowing(&mut module);
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

    /// The pieces of an import line, where the clause is named bindings alone.
    ///
    /// `import { a, b as c } from "./m"` yields the module and the names. A
    /// default or namespace clause binds the whole module under one name, which
    /// no sibling translation declares, so those yield `None` and travel as text.
    pub(super) fn import_target(text: &str) -> Option<ImportTarget> {
        let text = text.trim().trim_end_matches(';').trim();
        let rest = text.strip_prefix("import")?.trim();
        let rest = rest.strip_prefix("type ").unwrap_or(rest).trim();
        let inside = rest.strip_prefix('{')?;
        let (list, tail) = inside.split_once('}')?;
        let module = unquote(tail.trim().strip_prefix("from")?.trim());
        // `import type { A }` and `import { type A }` name one binding; the
        // keyword tells the checker how to treat it.
        let entries: Vec<String> = list
            .split(',')
            .map(|entry| {
                let entry = entry.trim();
                entry.strip_prefix("type ").unwrap_or(entry).to_string()
            })
            .collect();
        Some(ImportTarget {
            relative: module.starts_with('.'),
            module,
            names: super::import_names(&entries.join(","), " as ")?,
            resolved: None,
        })
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

    /// `type X = { kind: "a" } | { kind: "b"; n: number }` written inline.
    ///
    /// The same discriminated union as the named form, with the members spelled
    /// in place. Each member must be an object of plain fields sharing a
    /// literal-typed one. The literal names the variant, pascal-cased, the way
    /// the writers spell it back. Anything looser, a method, a member with no
    /// literal, a non-object member, stays carried.
    fn inline_union(cx: &Cx, node: Node<'_>) -> Option<Sum> {
        let value = cx.field(node, "value")?;
        if value.kind() != "union_type" {
            return None;
        }
        fn flatten<'t>(cx: &Cx, node: Node<'t>, into: &mut Vec<Node<'t>>) -> bool {
            match node.kind() {
                "union_type" => cx
                    .children(node)
                    .into_iter()
                    .all(|part| flatten(cx, part, into)),
                "object_type" => {
                    into.push(node);
                    true
                }
                _ => false,
            }
        }
        let mut members = Vec::new();
        if !flatten(cx, value, &mut members) || members.is_empty() {
            return None;
        }
        let fields_of = |object: Node<'_>| -> Option<Vec<Field>> {
            let mut fields = Vec::new();
            for member in cx.children(object) {
                match member.kind() {
                    "comment" => {}
                    "property_signature" => fields.push(Field {
                        doc: Vec::new(),
                        name: cx.field_text(member, "name")?,
                        ty: cx.field(member, "type").map(|t| ty(cx, t)),
                        default: None,
                        exported: true,
                    }),
                    _ => return None,
                }
            }
            Some(fields)
        };
        let members: Option<Vec<Vec<Field>>> = members.into_iter().map(fields_of).collect();
        let members = members?;
        let literal = |field: &Field| {
            matches!(&field.ty, Some(Type::Named { name, .. })
                if name.starts_with('"') || name.starts_with('\''))
        };
        // The field every member declares with a literal type tells them apart.
        let discriminator = members.first()?.iter().find(|f| {
            literal(f)
                && members
                    .iter()
                    .all(|m| m.iter().any(|o| o.name == f.name && literal(o)))
        })?;
        let discriminator = discriminator.name.clone();
        let variants: Option<Vec<Variant>> = members
            .iter()
            .map(|fields| {
                let tag =
                    fields
                        .iter()
                        .find(|f| f.name == discriminator)
                        .and_then(|f| match &f.ty {
                            Some(Type::Named { name, .. }) => {
                                Some(name.trim_matches(['"', '\'']).to_string())
                            }
                            _ => None,
                        })?;
                Some(Variant {
                    doc: Vec::new(),
                    name: super::super::write::pascal(&tag),
                    tag: Some(tag.clone()),
                    fields: fields
                        .iter()
                        .filter(|f| f.name != discriminator)
                        .cloned()
                        .collect(),
                })
            })
            .collect();
        Some(Sum {
            doc: Vec::new(),
            name: cx.field_text(node, "name").unwrap_or_default(),
            variants: variants?,
            exported: false,
        })
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
                    // The literal the source wrote in the discriminator field:
                    // `kind: "idle"` on an interface named `FIdle`.
                    tag: discriminators.first().and_then(|d| {
                        member
                            .fields
                            .iter()
                            .find(|f| &f.name == d)
                            .and_then(|f| match &f.ty {
                                Some(Type::Named { name, .. })
                                    if name.starts_with('"') || name.starts_with('\'') =>
                                {
                                    Some(name.trim_matches(['"', '\'']).to_string())
                                }
                                _ => None,
                            })
                    }),
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

    /// An object literal that spells a variant of one of the module's sums.
    ///
    /// `{ kind: "circle", radius: n }` is how TypeScript builds a value of a
    /// discriminated union, and it crossed as a map. `HashMap::from([("kind",
    /// "circle")])` landed in a position that wants `Shape`, wrong-typed
    /// instead of carried. An object is its variant when one literal-string
    /// entry names exactly one sum's variant and the other keys are that
    /// variant's declared fields. Anything looser stays the map it was.
    fn settle_kind_literals(module: &mut Module) {
        let variants: Vec<(String, String, String, std::collections::BTreeSet<String>)> = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Sum(s) => Some(s.variants.iter().map(|v| {
                    (
                        s.name.clone(),
                        v.name.clone(),
                        v.tag
                            .clone()
                            .unwrap_or_else(|| crate::transpile::write::snake_always(&v.name)),
                        v.fields.iter().map(|f| f.name.clone()).collect(),
                    )
                })),
                _ => None,
            })
            .flatten()
            .collect();
        if variants.is_empty() {
            return;
        }
        let settle = |e: &mut Expr, preferred: Option<&str>| {
            let Expr::MapLit(entries) = e else { return };
            let mut tag: Option<usize> = None;
            for (at, (key, value)) in entries.iter().enumerate() {
                if matches!(key, Expr::Str(_)) && matches!(value, Expr::Str(_)) {
                    tag = match tag {
                        None => Some(at),
                        // Two literal-string entries cannot both be the
                        // discriminator; leave the map alone.
                        Some(_) => return,
                    };
                }
            }
            let Some(at) = tag else { return };
            let Expr::Str(tag_value) = &entries[at].1 else {
                return;
            };
            let rest: Option<std::collections::BTreeSet<String>> = entries
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != at)
                .map(|(_, (key, _))| match key {
                    Expr::Str(name) => Some(name.clone()),
                    _ => None,
                })
                .collect();
            let Some(rest) = rest else { return };
            let mut answering: Vec<&(String, String, String, std::collections::BTreeSet<String>)> =
                variants
                    .iter()
                    .filter(|(_, _, tag, fields)| tag == tag_value && rest.is_subset(fields))
                    .collect();
            // Two sums answering the same tag is ordinary: two state unions in
            // one file both holding an "idle". The position's declared type,
            // where one is written, says which was meant.
            if answering.len() > 1 {
                if let Some(preferred) = preferred {
                    answering.retain(|(sum, _, _, _)| sum == preferred);
                }
            }
            let [(sum, name, _, _)] = answering.as_slice() else {
                return;
            };
            let (sum, name) = (sum.clone(), name.clone());
            let fields = std::mem::take(entries)
                .into_iter()
                .enumerate()
                .filter(|(i, _)| *i != at)
                .map(|(_, (key, value))| match key {
                    Expr::Str(field) => (field, value),
                    _ => unreachable!("checked above"),
                })
                .collect();
            *e = Expr::Variant { sum, name, fields };
        };
        // First the positions whose declared type names the sum: a `return`
        // under a signature, a binding with an annotation. Then the rest.
        let sum_of = |ty: Option<&Type>| -> Option<String> {
            match ty {
                Some(Type::Named { name, .. })
                    if variants.iter().any(|(sum, _, _, _)| sum == name) =>
                {
                    Some(name.clone())
                }
                _ => None,
            }
        };
        for item in &mut module.items {
            let Item::Function(f) = item else { continue };
            let returned = sum_of(f.returns.as_ref());
            super::each_stmt_in_stmts(&mut f.body, &mut |stmt| match stmt {
                Stmt::Return(Some(value)) => {
                    if let Some(sum) = &returned {
                        super::each_expr(value, &mut |e| settle(e, Some(sum.as_str())));
                    }
                }
                Stmt::Let {
                    ty: Some(ty),
                    value: Some(value),
                    ..
                } => {
                    if let Some(sum) = sum_of(Some(ty)) {
                        super::each_expr(value, &mut |e| settle(e, Some(sum.as_str())));
                    }
                }
                _ => {}
            });
        }
        super::each_expr_in_module(module, &mut |e| settle(e, None));
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
                        let mut default = cx.field(p, "value").map(|v| expr(cx, v));
                        if p.kind() == "optional_parameter" {
                            t = Some(Type::Optional(Box::new(
                                t.unwrap_or(named_with_args("unknown", &named_or_scalar)),
                            )));
                            // `punct?: string` lets a caller leave the argument
                            // out, and the parameter is then absent. A target that
                            // spells absence with a default needs one. Without it,
                            // Python declared the optional and still required it,
                            // and every valid call was a TypeError.
                            if default.is_none() {
                                default = Some(Expr::Null);
                            }
                        }
                        params.push(Param {
                            name,
                            ty: t,
                            default,
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
            is_private: false,
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
    /// A TypeScript member is public unless it says otherwise, which is the opposite of what
    /// a free function does. Reading both the same way made every translated method private
    /// in Java and unreachable in Go, Rust and Zig. It also made every `private` field
    /// public, the same mistake pointing the other way.
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
                        // This language says `private` in so many words, and
                        // the two that also do carry the word across.
                        method.is_private = !method.exported;
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
                    // A member this does not recognise is not a member that is not there. Every
                    // reader here ended its member loop with `_ => {}`. A
                    // `@staticmethod` disappeared from a class while the report said every
                    // signature had carried across intact.
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
    /// `Promise<Record<string, string>>` resolved its outer layer and left the inner one as
    /// an opaque name. A round trip then produced `Record[str, str]` in Python.
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
        // `(n: number) => number`, TypeScript's function type. Left unread it
        // ran together into one unwritable name, and a parameter holding a
        // function took a type nothing could call.
        if let Some((inside, rest)) = super::parenthesised(trimmed) {
            if let Some(answer) = rest.strip_prefix("=>") {
                return Type::Fn {
                    params: super::parameter_types(&inside, ty_text),
                    returns: Box::new(ty_text(answer)),
                };
            }
        }
        if let Some(element) = trimmed.strip_suffix("[]") {
            return Type::List(Box::new(named_or_scalar(element)));
        }
        for prefix in ["Set<", "ReadonlySet<"] {
            if let Some(inner) = trimmed
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix('>'))
            {
                return Type::Set(Box::new(ty_text(inner)));
            }
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
    /// The expression a `{ return e; }` block returns, as a node.
    fn returned_node<'t>(cx: &Cx, block: Node<'t>) -> Option<Node<'t>> {
        let statements = cx.children(block);
        let [only] = statements.as_slice() else {
            return None;
        };
        if only.kind() != "return_statement" {
            return None;
        }
        cx.children(*only).into_iter().find(|c| c.is_named())
    }

    fn one_arg_arrow<'t>(cx: &Cx, node: Node<'t>) -> Option<(String, Node<'t>)> {
        if node.kind() != "arrow_function" {
            return None;
        }
        // A block whose only statement returns holds one expression, and that is
        // the shape a comprehension takes. Refused, `xs.map((x) => { return
        // x * 2; })` crossed as a method the target has not got.
        let body = cx.field(node, "body")?;
        let body = match body.kind() {
            "statement_block" => match super::only_returned(&block(cx, body)) {
                Some(_) => returned_node(cx, body)?,
                None => return None,
            },
            _ => body,
        };
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
            // The keyword is its own node here. `super(m)` and `super.m()` then
            // read as the ordinary call and field shapes over this name. That
            // canonical form is one every writer spells its own way.
            "identifier" | "property_identifier" | "this" | "super" => Expr::Name(cx.text(node)),
            // `a?.b` is not `a.b`: the null check must survive. Where the
            // object is a name or a path, so evaluating it twice is reading it
            // twice, the conditional spells the check in every target. An
            // object with effects stays carried.
            "member_expression" if has_optional_chain(node) => {
                let object = cx.field(node, "object").map(|o| expr(cx, o));
                let name = cx.field_text(node, "property").unwrap_or_default();
                fn path_only(e: &Expr) -> bool {
                    match e {
                        Expr::Name(_) => true,
                        Expr::Field { of, .. } => path_only(of),
                        Expr::Ternary {
                            condition,
                            then,
                            otherwise,
                        } => {
                            // A nested `?.` already lowered to a conditional
                            // over a path.
                            path_only(condition) || (path_only(then) && path_only(otherwise))
                        }
                        Expr::Null => true,
                        Expr::Binary { left, right, .. } => path_only(left) && path_only(right),
                        _ => false,
                    }
                }
                match object {
                    Some(object) if path_only(&object) => Expr::Ternary {
                        condition: Box::new(Expr::Binary {
                            op: BinaryOp::Eq,
                            left: Box::new(object.clone()),
                            right: Box::new(Expr::Null),
                        }),
                        then: Box::new(Expr::Null),
                        otherwise: Box::new(Expr::Field {
                            of: Box::new(object),
                            name,
                        }),
                    },
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
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
                // `a ?? b` asks whether the left side is absent. That is a question instead of
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
            // `x as T`, `x satisfies T` and `x!` are assertions to the type checker and have
            // no runtime effect whatever. The value is the expression, so the translation
            // comes out exact rather than as a gap. Leaving them unhandled carried a whole
            // statement over something that meant nothing.
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
            "as_expression" | "satisfies_expression" => cx
                .children(node)
                .first()
                .map(|n| expr(cx, *n))
                .unwrap_or(Expr::Null),
            // `x!` asserts the value is there and uses it.
            // `x!` asserts the value is there. The grammar hands back the whole
            // expression to its left as the operand, so `a + b!` arrives as
            // `(a + b)!`, which is not what TypeScript means and not what it
            // does: the assertion is postfix and binds to `b` alone. Read that
            // way, `total + m.get(k)!` became `.unwrap()` on a sum.
            "non_null_expression" => match cx.children(node).first() {
                Some(inner) => assert_the_last_operand(expr(cx, *inner)),
                None => Expr::Null,
            },
            // `(x) => e`, the one-expression arrow. A block body is a function
            // that wants a name. A type, a default or a pattern in the parameter
            // list is more than the shared shape. All of those stay carried.
            "arrow_function" => {
                let params: Option<Vec<Param>> = match cx.field(node, "parameter") {
                    Some(p) => Some(vec![super::lambda_param(cx.text(p), None)]),
                    None => cx
                        .field(node, "parameters")
                        .map(|list| {
                            cx.children(list)
                                .into_iter()
                                .map(|p| match p.kind() {
                                    "identifier" => Some(super::lambda_param(cx.text(p), None)),
                                    // A default value is more than the shared
                                    // shape. A type is not. Refusing one meant
                                    // `(n: number) => n + 1` could not be read
                                    // at all.
                                    "required_parameter" if cx.field(p, "value").is_none() => {
                                        let name = cx
                                            .field(p, "pattern")
                                            .filter(|n| n.kind() == "identifier")?;
                                        Some(super::lambda_param(
                                            cx.text(name),
                                            cx.field(p, "type").map(|t| ty(cx, t)),
                                        ))
                                    }
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| Some(Vec::new())),
                };
                match (params, cx.field(node, "body")) {
                    (Some(params), Some(body)) => {
                        let value = match body.kind() {
                            "statement_block" => super::only_returned(&block(cx, body)),
                            _ => Some(expr(cx, body)),
                        };
                        match value {
                            Some(value) => Expr::Lambda {
                                params,
                                returns: cx.field(node, "return_type").map(|t| ty(cx, t)),
                                body: Box::new(value),
                            },
                            None => Expr::Unsupported(cx.unsupported(node)),
                        }
                    }
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }
}

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
            Stmt::BreakWith { value, .. } => {
                if let Some(value) = value {
                    each_expr(value, visit);
                }
            }
            Stmt::LocalFunction(f) => each_expr_in_stmts(&mut f.body, visit),
            Stmt::Let { value, .. } => {
                if let Some(value) = value {
                    each_expr(value, visit);
                }
            }
            Stmt::Assign { target, value } => {
                each_expr(target, visit);
                each_expr(value, visit);
            }
            Stmt::TupleAssign { value, .. } => each_expr(value, visit),
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
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                ..
            } => {
                if let Some(init) = init {
                    each_expr_in_stmts(std::slice::from_mut(init), visit);
                }
                if let Some(condition) = condition {
                    each_expr(condition, visit);
                }
                if let Some(update) = update {
                    each_expr_in_stmts(std::slice::from_mut(update), visit);
                }
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
            Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => {
                each_expr_in_stmts(body, visit)
            }
            Stmt::MatchVariants {
                subject,
                arms,
                default,
                ..
            } => {
                each_expr(subject, visit);
                for arm in arms.iter_mut() {
                    each_expr_in_stmts(&mut arm.body, visit);
                }
                each_expr_in_stmts(default, visit);
            }
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
            Stmt::Assert { condition, message } => {
                each_expr(condition, visit);
                if let Some(message) = message {
                    each_expr(message, visit);
                }
            }
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
            Stmt::BreakWith { .. } => {}
            Stmt::LocalFunction(f) => each_stmt_in_stmts(&mut f.body, visit),
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
            Stmt::CountedFor {
                init, update, body, ..
            } => {
                for header in [init, update].into_iter().flatten() {
                    each_stmt_in_stmts(std::slice::from_mut(header), visit);
                }
                each_stmt_in_stmts(body, visit);
            }
            Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => {
                each_stmt_in_stmts(body, visit)
            }
            Stmt::MatchVariants { arms, default, .. } => {
                for arm in arms.iter_mut() {
                    each_stmt_in_stmts(&mut arm.body, visit);
                }
                each_stmt_in_stmts(default, visit);
            }
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
            | Stmt::TupleAssign { .. }
            | Stmt::Assign { .. }
            | Stmt::Expr(_)
            | Stmt::Assert { .. }
            | Stmt::Comment(_)
            | Stmt::Throw(_)
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Unsupported(_) => {}
        }
    }
}

/// Children first, then the node itself, so a rewrite sees settled children.
/// The import a carried line spells, where the language has a parser for it.
///
/// A sweep needs this for imports the readers left as text. An import inside
/// a function body is carried whole. The sweep is the only place that knows
/// the file it names is being translated beside it.
pub(super) fn parse_import(language: Language, text: &str) -> Option<ImportTarget> {
    match language {
        Language::Python => python::import_target(text),
        Language::TypeScript | Language::Tsx => typescript::import_target(text),
        _ => None,
    }
}

pub(super) fn each_expr(e: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    match e {
        Expr::SetLit(items) => {
            for item in items {
                each_expr(item, visit);
            }
        }
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
        Expr::Variant { fields, .. } => {
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
        Expr::Lambda { body, .. } => each_expr(body, visit),
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
/// A comma-separated import list, each entry a name or `name <separator> alias`.
///
/// `None` when any entry is not a plain identifier. An entry this cannot read
/// is an import the sweep must not rewrite, so the whole line stays text.
fn import_names(list: &str, separator: &str) -> Option<Vec<ImportedName>> {
    let identifier =
        |name: &str| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    let mut names = Vec::new();
    for entry in list.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (name, alias) = match entry.split_once(separator) {
            Some((name, alias)) => (name.trim(), Some(alias.trim().to_string())),
            None => (entry, None),
        };
        if !identifier(name) || !alias.as_deref().map(identifier).unwrap_or(true) {
            return None;
        }
        names.push(ImportedName {
            name: name.to_string(),
            alias,
        });
    }
    Some(names)
}

/// Calls that build a known record are constructions.
///
/// Python spells construction as a call. Its reader promotes calls to the
/// file's own types. A directory sweep calls this again with every record the
/// sweep declares, so a sibling's class is constructed and not called.
pub(crate) fn promote_constructions(
    module: &mut Module,
    types: &std::collections::BTreeSet<String>,
) {
    if types.is_empty() {
        return;
    }
    each_expr_in_module(module, &mut |e| {
        if let Expr::Call { callee, args } = e {
            if matches!(callee.as_ref(), Expr::Name(n) if types.contains(n)) {
                let callee = callee.clone();
                let args = std::mem::take(args);
                *e = Expr::New { callee, args };
            }
        }
    });
}

pub(super) fn each_expr_in_module(module: &mut Module, visit: &mut dyn FnMut(&mut Expr)) {
    for item in module.items.iter_mut() {
        each_expr_in_item(item, visit);
    }
}

/// One item's slice of [`each_expr_in_module`], for the passes that need to
/// know which declaration they are inside.
fn each_expr_in_item(item: &mut Item, visit: &mut dyn FnMut(&mut Expr)) {
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

/// Keep the variant candidates this module's own sums answer for; carry the rest.
///
/// A reader cannot know the sums while it reads expressions, so `Shape::Point`
/// and `Vec::new` both arrive as candidates. Here the sums are known. A candidate
/// the module declares stays and takes the sum's plain name. A candidate in
/// callee position or naming anything else goes back to being carried, which
/// every such path was before candidates existed.
/// Branches that ask "which variant is this?" become the match they are.
///
/// The construction crossed a pass before the consumption did: `s.kind ==
/// "circle"` and `s.radius` went to Rust verbatim, against an enum that
/// declares neither. An `if`/`else if` chain or a `switch` is a variant
/// match when its literals name exactly one module sum's variants through
/// one subject's field. Each arm's payload reads through the
/// subject become plain locals, so every writer can spell the narrowing its
/// own way. A chain that mixes in any other condition stays what it was.
fn settle_variant_narrowing(module: &mut Module) {
    use std::collections::BTreeMap;
    let sums: BTreeMap<String, BTreeMap<String, Vec<String>>> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some((
                s.name.clone(),
                s.variants
                    .iter()
                    .map(|v| {
                        (
                            v.name.clone(),
                            v.fields.iter().map(|f| f.name.clone()).collect(),
                        )
                    })
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    if sums.is_empty() {
        return;
    }
    // The variant a literal names, when exactly one sum answers for it.
    let tags: std::collections::BTreeMap<(String, String), String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some(s.variants.iter().map(|v| {
                (
                    (s.name.clone(), v.name.clone()),
                    v.tag
                        .clone()
                        .unwrap_or_else(|| crate::transpile::write::snake_always(&v.name)),
                )
            })),
            _ => None,
        })
        .flatten()
        .collect();
    let variant_of = |lit: &str| -> Option<(String, String)> {
        let mut found = None;
        for (sum, variants) in &sums {
            for name in variants.keys() {
                let answers = tags
                    .get(&(sum.clone(), name.clone()))
                    .is_some_and(|tag| tag == lit);
                if answers {
                    match found {
                        None => found = Some((sum.clone(), name.clone())),
                        Some(_) => return None,
                    }
                }
            }
        }
        found
    };
    // `cond` as "this subject's field equals this literal", either way round.
    fn tag_test(cond: &Expr) -> Option<(&Expr, &str)> {
        let Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
        } = cond
        else {
            return None;
        };
        match (left.as_ref(), right.as_ref()) {
            (Expr::Field { of, .. }, Expr::Str(lit)) => Some((of.as_ref(), lit.as_str())),
            (Expr::Str(lit), Expr::Field { of, .. }) => Some((of.as_ref(), lit.as_str())),
            _ => None,
        }
    }
    // Replace the payload reads of `variant` through `subject` with locals and
    // say which fields were read.
    fn bind_payload(
        body: &mut [Stmt],
        subjects: &[String],
        fields: &[String],
    ) -> Vec<(String, String)> {
        let mut bound: Vec<(String, String)> = Vec::new();
        each_expr_in_stmts(body, &mut |e| {
            // `c.radius()` is Java reading the payload through the record's
            // accessor; the bare field read is everyone else's spelling.
            if let Expr::Call { callee, args } = e {
                if args.is_empty() {
                    if let Expr::Field { of, name } = callee.as_ref() {
                        if subjects.contains(&format!("{of:?}")) && fields.contains(name) {
                            let name = name.clone();
                            if !bound.iter().any(|(f, _)| f == &name) {
                                bound.push((name.clone(), name.clone()));
                            }
                            *e = Expr::Name(name);
                            return;
                        }
                    }
                }
            }
            if let Expr::Field { of, name } = e {
                if subjects.contains(&format!("{of:?}")) && fields.contains(name) {
                    if !bound.iter().any(|(f, _)| f == name) {
                        bound.push((name.clone(), name.clone()));
                    }
                    *e = Expr::Name(name.clone());
                }
            }
        });
        bound
    }
    let mut settle = |stmt: &mut Stmt| {
        match stmt {
            Stmt::If { condition, .. } => {
                let test_of = |cond: &Expr| -> Option<(Expr, (String, String))> {
                    if let Some((subject, lit)) = tag_test(cond) {
                        return variant_of(lit).map(|found| (subject.clone(), found));
                    }
                    if let Expr::InstanceOf { value, ty } = cond {
                        if let Expr::Name(n) = ty.as_ref() {
                            let mut answering = sums
                                .iter()
                                .filter(|(_, variants)| variants.contains_key(n.as_str()))
                                .map(|(sum, _)| sum.clone());
                            if let (Some(sum), None) = (answering.next(), answering.next()) {
                                return Some((value.as_ref().clone(), (sum, n.clone())));
                            }
                        }
                    }
                    None
                };
                let Some((subject, _)) = test_of(condition) else {
                    return;
                };
                let key = format!("{subject:?}");
                // Walk the chain, collecting arms while every link keeps the shape.
                let mut arms: Vec<VariantArm> = Vec::new();
                let mut default: Vec<Stmt> = Vec::new();
                let mut sum_name = String::new();
                let mut current = std::mem::replace(stmt, Stmt::Expr(Expr::Null));
                let rest = &mut current;
                loop {
                    let Stmt::If {
                        condition,
                        then,
                        otherwise,
                    } = rest
                    else {
                        unreachable!("only an if enters the loop");
                    };
                    let settled = test_of(condition)
                        .filter(|(s, _)| format!("{s:?}") == key)
                        .map(|(_, found)| found);
                    let Some((sum, variant)) = settled else {
                        // A link that stopped matching: the whole remainder is
                        // the default, and the chain closes here.
                        break;
                    };
                    sum_name = sum;
                    let fields = sums[&sum_name][&variant].clone();
                    let mut body = std::mem::take(then);
                    // `var c = (Circle) s;` re-names the narrowed subject.
                    // The alias reads like the subject from here on, and the
                    // cast itself has nothing left to say.
                    let mut keys = vec![key.clone()];
                    body.retain(|stmt| {
                        if let Stmt::Let {
                            name,
                            value: Some(Expr::Cast { ty, value }),
                            ..
                        } = stmt
                        {
                            let casts_subject = format!("{value:?}") == key
                                && matches!(ty.as_ref(), Expr::Name(n) if *n == variant);
                            if casts_subject {
                                keys.push(format!("{:?}", Expr::Name(name.clone())));
                                return false;
                            }
                        }
                        true
                    });
                    let bindings = bind_payload(&mut body, &keys, &fields);
                    arms.push(VariantArm {
                        variant,
                        bindings,
                        body,
                    });
                    match otherwise.as_mut_slice() {
                        [Stmt::If { .. }] => {
                            let mut chain = std::mem::take(otherwise);
                            *rest = chain.pop().expect("just matched one");
                            continue;
                        }
                        _ => {
                            default = std::mem::take(otherwise);
                            *rest = Stmt::Expr(Expr::Null);
                            break;
                        }
                    }
                }
                // A remainder that broke the shape becomes the default arm.
                if !matches!(rest, Stmt::Expr(Expr::Null)) {
                    default = vec![std::mem::replace(rest, Stmt::Expr(Expr::Null))];
                }
                *stmt = Stmt::MatchVariants {
                    subject,
                    sum: sum_name,
                    arms,
                    default,
                };
            }
            Stmt::Switch {
                subject: Expr::Field { of, .. },
                arms,
                default,
            } => {
                let all: Option<Vec<(String, String)>> = arms
                    .iter()
                    .map(|(literals, _)| match literals.as_slice() {
                        [Expr::Str(lit)] => variant_of(lit),
                        _ => None,
                    })
                    .collect();
                let Some(settled) = all else { return };
                let Some(sum) = settled.first().map(|(s, _)| s.clone()) else {
                    return;
                };
                if settled.iter().any(|(s, _)| *s != sum) {
                    return;
                }
                let subject = of.as_ref().clone();
                let key = format!("{subject:?}");
                let built: Vec<VariantArm> = std::mem::take(arms)
                    .into_iter()
                    .zip(settled)
                    .map(|((_, mut body), (_, variant))| {
                        let fields = sums[&sum][&variant].clone();
                        let bindings = bind_payload(&mut body, std::slice::from_ref(&key), &fields);
                        VariantArm {
                            variant,
                            bindings,
                            body,
                        }
                    })
                    .collect();
                *stmt = Stmt::MatchVariants {
                    subject,
                    sum,
                    arms: built,
                    default: std::mem::take(default),
                };
            }
            _ => {}
        }
    };
    for item in &mut module.items {
        match item {
            Item::Function(f) => each_stmt_in_stmts(&mut f.body, &mut settle),
            Item::Record(r) => {
                for m in &mut r.methods {
                    each_stmt_in_stmts(&mut m.body, &mut settle);
                }
            }
            _ => {}
        }
    }
}

fn settle_variants(module: &mut Module) {
    use std::collections::{BTreeMap, BTreeSet};
    let records: BTreeSet<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();
    let sums: BTreeMap<String, BTreeSet<String>> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some((
                s.name.clone(),
                s.variants.iter().map(|v| v.name.clone()).collect(),
            )),
            _ => None,
        })
        .collect();
    let variant_fields: BTreeMap<(String, String), Vec<String>> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some(s.variants.iter().map(|v| {
                (
                    (s.name.clone(), v.name.clone()),
                    v.fields.iter().map(|f| f.name.clone()).collect(),
                )
            })),
            _ => None,
        })
        .flatten()
        .collect();
    let demoted = |sum: &str, name: &str, fields: &[(String, Expr)]| {
        let source = match fields.is_empty() {
            true => format!("{sum}::{name}"),
            false => format!("{sum}::{name} {{ .. }}"),
        };
        Expr::Unsupported(Unsupported {
            construct: "a name reached through a path".to_string(),
            source,
            line: 0,
        })
    };
    let mut items = std::mem::take(&mut module.items);
    for item in &mut items {
        // A match read as a variant match must name one of this module's own
        // sums. `match dir` over an imported enum has no declaration here to
        // check against, and writing `isinstance(dir, North)` into Python
        // invented a name. The demotion renders the arms back as Rust, so the
        // carry keeps its body.
        if let Item::Function(f) = item {
            each_stmt_in_stmts(&mut f.body, &mut |stmt| {
                if let Stmt::MatchVariants { sum, arms, .. } = stmt {
                    let known = sums
                        .get(sum.as_str())
                        .is_some_and(|variants| arms.iter().all(|a| variants.contains(&a.variant)));
                    if !known {
                        let rendered =
                            crate::transpile::write::render_rust_stmts(std::slice::from_ref(stmt));
                        *stmt = Stmt::Unsupported(Unsupported {
                            construct: "a match on a foreign choice".to_string(),
                            source: rendered,
                            line: 0,
                        });
                    }
                }
            });
        }
        let returning: Option<String> = match item {
            Item::Function(f) => match &f.returns {
                Some(Type::Named { name, .. }) => Some(name.clone()),
                _ => None,
            },
            _ => None,
        };
        each_expr_in_item(item, &mut |e| {
            if let Expr::Call { callee, args } = e {
                // A path used as a callee is `Vec::new()` or a tuple-variant build;
                // neither has a crossing. The walk settles children first, so by now
                // the callee is either a still-valid variant (a tuple-variant build)
                // or the carried path this pass demoted it to; either way, demoting
                // only the callee left the marker being called, and `None()` ran in
                // Python. The whole call carries.
                let path_callee = match callee.as_ref() {
                    // An anonymous candidate (`.init(x)`, sum still empty) is a
                    // dot-literal call; normalize settles it by member name.
                    Expr::Variant { sum, .. } if sum.is_empty() => None,
                    Expr::Variant { sum, name, .. } => Some(format!("{sum}::{name}")),
                    Expr::Unsupported(u) if u.construct == "a name reached through a path" => {
                        Some(u.source.clone())
                    }
                    _ => None,
                };
                if let Some(path) = path_callee {
                    let source = format!("{path}({} argument(s))", args.len());
                    *e = Expr::Unsupported(Unsupported {
                        construct: "a call through a path".to_string(),
                        source,
                        line: 0,
                    });
                    return;
                }
            }
            if let Expr::New { callee, args } = e {
                // `new Point()` built a record that a sum has since consumed. The
                // construction is the variant's, arguments matched against the
                // declared fields in order, keywords by their names.
                if let Expr::Name(n) = callee.as_ref() {
                    if !records.contains(n.as_str()) {
                        let answering: Vec<(&String, &BTreeSet<String>)> = sums
                            .iter()
                            .filter(|(_, variants)| variants.contains(n.as_str()))
                            .collect();
                        if let [(sum, _)] = answering.as_slice() {
                            let declared = variant_fields
                                .get(&((*sum).clone(), n.clone()))
                                .cloned()
                                .unwrap_or_default();
                            let name = n.clone();
                            let sum = (*sum).clone();
                            let taken = std::mem::take(args);
                            let mut fields = Vec::new();
                            let mut position = 0usize;
                            for arg in taken {
                                match arg {
                                    Expr::Keyword { name, value } => fields.push((name, *value)),
                                    other => {
                                        let field = declared
                                            .get(position)
                                            .cloned()
                                            .unwrap_or_else(|| "value".to_string());
                                        position += 1;
                                        fields.push((field, other));
                                    }
                                }
                            }
                            *e = Expr::Variant { sum, name, fields };
                            return;
                        }
                    }
                }
            }
            if let Expr::Variant { sum, name, fields } = e {
                // An anonymous candidate names no sum at all. It is attributed
                // when exactly one of the module's sums answers to the variant's
                // name, and carried when none or several do. A candidate naming
                // one of the module's own records is that record being built. A
                // name that is both, a struct kept beside its variant, settles by
                // the enclosing function's return type. Returning the struct
                // builds the struct; anything else builds the variant.
                if sum.is_empty() {
                    let also_variant = sums
                        .values()
                        .any(|variants| variants.contains(name.as_str()));
                    let build_record = records.contains(name.as_str())
                        && (!also_variant || returning.as_deref() == Some(name.as_str()));
                    if build_record {
                        // The IR has a node for building a record.
                        // Written as a call taking keywords, the two targets
                        // that build a record through a constructor got an
                        // object literal where an argument list belonged.
                        *e = Expr::RecordLit {
                            ty: name.clone(),
                            fields: std::mem::take(fields),
                        };
                        return;
                    }
                    let answering: Vec<&String> = sums
                        .iter()
                        .filter(|(_, variants)| variants.contains(name.as_str()))
                        .map(|(owner, _)| owner)
                        .collect();
                    if let [only] = answering.as_slice() {
                        *sum = (*only).clone();
                    }
                    return;
                }
                let plain = sum.rsplit([':', '.']).next().unwrap_or(sum).to_string();
                let answered = sums
                    .get(&plain)
                    .is_some_and(|variants| variants.contains(name.as_str()));
                match answered {
                    true => *sum = plain,
                    false => *e = demoted(sum, name, fields),
                }
            }
        });
    }
    module.items = items;
}

/// The pieces between top-level commas, nesting respected.
///
/// What the tuple spellings share: Rust and Go put types between `(` and `)`,
/// TypeScript between `[` and `]`, Zig between `struct {` and `}`. Each reader
/// strips its own brackets and splits the inside here.
/// One expression, read from its own text.
///
/// A macro body is a token tree and not a syntax tree. An element of `vec![…]`
/// more involved than a literal has no node to read. Parsing the text on its
/// own gives the tree the enclosing parse never built.
fn reparsed(text: &str, language: Language) -> Option<Expr> {
    let wrapped = match language {
        Language::Rust => format!("fn frOne() {{ let frOne = {text}; }}"),
        _ => return None,
    };
    let parsed = crate::parse::Parsers::new().parse(language, &wrapped).ok()?;
    if parsed.has_errors() {
        return None;
    }
    let lines = LineIndex::new(&wrapped);
    let cx = Cx {
        source: &wrapped,
        lines: &lines,
    };
    let mut found = None;
    fn hunt(cx: &Cx, node: Node<'_>, found: &mut Option<Expr>) {
        if node.kind() == "let_declaration" {
            if let Some(value) = cx.field(node, "value") {
                *found = Some(rust::expr(cx, value));
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            hunt(cx, child, found);
        }
    }
    hunt(&cx, parsed.tree.root_node(), &mut found);
    found.filter(|e| !matches!(e, Expr::Unsupported(_)))
}

/// The one expression a `{ return e }` body stands for.
///
/// A function value written with a block whose only statement returns is the
/// same function value written as an expression. Refused, every Go `func(n int)
/// int { return n + 1 }` and every Rust `|n| { n + 1 }` crossed as a comment.
fn only_returned(body: &[Stmt]) -> Option<Expr> {
    match body {
        [Stmt::Return(Some(e))] => Some(e.clone()),
        _ => None,
    }
}

/// `_, ok := m[k]` asks whether a key is there, and Go has no other way to.
///
/// Read as a two-value assignment, the `ok` it binds meant nothing to any other
/// target and the membership question crossed as a tuple nobody had.
fn comma_ok_membership(names: &[String], value: &Expr) -> Option<(String, Expr)> {
    let [_, present] = names else {
        return None;
    };
    let Expr::Index { of, index } = value else {
        return None;
    };
    Some((
        present.clone(),
        Expr::Call {
            callee: Box::new(Expr::Field {
                of: of.clone(),
                name: "contains".to_string(),
            }),
            args: vec![(**index).clone()],
        },
    ))
}

/// A lambda parameter, named and typed where the source typed it.
fn lambda_param(name: String, ty: Option<Type>) -> Param {
    Param {
        name,
        ty,
        default: None,
        kind: ParamKind::Normal,
    }
}

/// The inside of a leading `(…)` and the text after it, brackets balanced.
///
/// `(a: A, b: B) => R` gives `("a: A, b: B", "=> R")`. Splitting on the first
/// `)` instead read `(f: (n: number) => number)` as ending in the middle.
fn parenthesised(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    if !text.starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' | '<' | '{' => depth += 1,
            ')' | ']' | '>' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text[1..at].to_string(), text[at + 1..].trim().to_string()));
                }
            }
            _ => {}
        }
    }
    None
}

/// A parameter list where each entry may carry a name: `a: A` or just `A`.
fn parameter_types(inside: &str, of: impl Fn(&str) -> Type) -> Vec<Type> {
    comma_parts(inside)
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once(':') {
            Some((_, ty)) => of(ty.trim()),
            None => of(p),
        })
        .collect()
}

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
/// The reader drops width on purpose, so `i64`, `int` and `number` all become [`Type::Int`].
/// Carrying a width into a language that has none would invent a guarantee. The writer says
/// so when it matters.
fn scalar(text: &str) -> Option<Type> {
    let t = text.trim().trim_start_matches('&').trim();
    Some(match t {
        "bool" | "boolean" => Type::Bool,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" | "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16"
        | "uint32" | "uint64" => Type::Int,
        "f32" | "f64" | "float" | "float32" | "float64" => Type::Float,
        "String" | "str" | "string" => Type::String,
        "()" | "None" | "void" | "struct{}" => Type::Unit,
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
        "//" => BinaryOp::FloorDiv,
        "%" => BinaryOp::Rem,
        "==" | "===" | "is" => BinaryOp::Eq,
        "!=" | "!==" | "is not" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        "<=" => BinaryOp::Le,
        ">" => BinaryOp::Gt,
        ">=" => BinaryOp::Ge,
        "&&" | "and" => BinaryOp::And,
        "||" | "or" => BinaryOp::Or,
        "^" => BinaryOp::Xor,
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
/// The IR holds what the string *is*, not how the source spelled it. Carrying the spelling
/// made every writer escape the backslash again on the way out. A string holding a newline
/// then crossed as one holding a backslash and an `n`. The output parsed, so nothing caught
/// it; every string with an escape in it came out meaning something else.
///
/// A backslash before anything this does not recognise stays as written. Python does the same
/// with `"\d"`, and the others cannot produce one, because an unknown escape is a compile
/// error in every one of them.
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

mod bash {
    //! Bash into the shared form.
    //!
    //! A script is statements top to bottom, functions among them, which is Python's
    //! shape. What crosses is the computational subset: variables, arithmetic,
    //! strings, tests, the four loop forms, `case`, and functions with the calls
    //! between them. An `echo` is the canonical print, and `$(f …)` over an
    //! in-module function is a call.
    //!
    //! A pipeline, a redirection, an external command: none of these has a
    //! counterpart the six targets share. Each carries loudly as the construct
    //! it is.
    //!
    //! Positional parameters become named ones: a function reading `$1` and `$2`
    //! declares `a1` and `a2`. Every target requires parameters to have names, and
    //! `$1` is bash's way of not writing one.

    use super::*;

    pub fn module(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = read_items(cx, root);
        settle_types(&mut module);
        module
    }

    fn read_items(cx: &Cx, root: Node<'_>) -> Module {
        let mut module = Module::default();
        let names = function_names(cx, root);
        // A comment right above a function is its doc, the way every target
        // understands one. A comment above anything else stands where it is.
        let mut pending: Vec<String> = Vec::new();
        for child in cx.children_with_comments(root) {
            match child.kind() {
                "comment" => {
                    let text = cx.text(child);
                    // The interpreter line is a fact about bash, not a statement.
                    if text.starts_with("#!") {
                        continue;
                    }
                    pending.push(text.trim_start_matches('#').trim().to_string());
                }
                "function_definition" => {
                    let mut f = function(cx, child, &names);
                    f.doc = std::mem::take(&mut pending);
                    module.items.push(Item::Function(f));
                }
                _ => {
                    for line in pending.drain(..) {
                        module.items.push(Item::Statement(Stmt::Comment(line)));
                    }
                    module.items.push(Item::Statement(stmt(cx, child, &names)));
                }
            }
        }
        for line in pending {
            module.items.push(Item::Statement(Stmt::Comment(line)));
        }
        module
    }

    /// The types the source states without writing any down.
    ///
    /// Bash has no annotations, and the six targets want them. Two places state a
    /// type anyway: the literals a function's own callers pass, and the literals
    /// its body returns. Where every statement agrees, the parameter or return
    /// takes that type. Where any disagrees, nothing is claimed, and the target's
    /// own settling has the same untyped draft Python gives it.
    fn settle_types(module: &mut Module) {
        use std::collections::BTreeMap;
        let mut arguments: BTreeMap<(String, usize), Vec<Type>> = BTreeMap::new();
        let mut note_call = |e: &Expr| {
            let Expr::Call { callee, args } = e else {
                return;
            };
            let Expr::Name(name) = callee.as_ref() else {
                return;
            };
            for (at, arg) in args.iter().enumerate() {
                if let Some(ty) = literal_ty(arg) {
                    arguments.entry((name.clone(), at)).or_default().push(ty);
                }
            }
        };
        for item in &module.items {
            match item {
                Item::Function(f) => walk_stmts(&f.body, &mut note_call),
                Item::Statement(stmt) => walk_stmts(std::slice::from_ref(stmt), &mut note_call),
                _ => {}
            }
        }
        for item in &mut module.items {
            let Item::Function(f) = item else { continue };
            for (at, param) in f.params.iter_mut().enumerate() {
                if param.ty.is_some() {
                    continue;
                }
                if let Some(seen) = arguments.get(&(f.name.clone(), at)) {
                    if let Some(first) = seen.first() {
                        if seen.iter().all(|t| t == first) {
                            param.ty = Some(first.clone());
                        }
                    }
                }
            }
            if f.returns.is_none() {
                let mut returned: Vec<Type> = Vec::new();
                collect_returns(&f.body, &mut returned);
                if let Some(first) = returned.first() {
                    if !returned.is_empty() && returned.iter().all(|t| t == first) {
                        f.returns = Some(first.clone());
                    }
                }
            }
        }
    }

    /// The type a literal argument states.
    fn literal_ty(e: &Expr) -> Option<Type> {
        match e {
            Expr::Int(_) => Some(Type::Int),
            Expr::Float(_) => Some(Type::Float),
            Expr::Str(_) | Expr::Template(_) => Some(Type::String),
            Expr::Bool(_) => Some(Type::Bool),
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } => literal_ty(operand),
            _ => None,
        }
    }

    /// Every call expression in these statements, visited once.
    fn walk_stmts(stmts: &[Stmt], note: &mut impl FnMut(&Expr)) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { value: Some(v), .. } => walk_expr(v, note),
                Stmt::Assign { target, value } => {
                    walk_expr(target, note);
                    walk_expr(value, note);
                }
                Stmt::Return(Some(v)) | Stmt::Expr(v) => walk_expr(v, note),
                Stmt::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    walk_expr(condition, note);
                    walk_stmts(then, note);
                    walk_stmts(otherwise, note);
                }
                Stmt::While { condition, body } => {
                    walk_expr(condition, note);
                    walk_stmts(body, note);
                }
                Stmt::ForEach { iterable, body, .. } => {
                    walk_expr(iterable, note);
                    walk_stmts(body, note);
                }
                Stmt::CountedFor {
                    init,
                    condition,
                    update,
                    body,
                    ..
                } => {
                    if let Some(init) = init {
                        walk_stmts(std::slice::from_ref(init), note);
                    }
                    if let Some(condition) = condition {
                        walk_expr(condition, note);
                    }
                    if let Some(update) = update {
                        walk_stmts(std::slice::from_ref(update), note);
                    }
                    walk_stmts(body, note);
                }
                Stmt::Switch {
                    subject,
                    arms,
                    default,
                } => {
                    walk_expr(subject, note);
                    for (selectors, arm) in arms {
                        for s in selectors {
                            walk_expr(s, note);
                        }
                        walk_stmts(arm, note);
                    }
                    walk_stmts(default, note);
                }
                Stmt::Block(inner) => walk_stmts(inner, note),
                _ => {}
            }
        }
    }

    fn walk_expr(e: &Expr, note: &mut impl FnMut(&Expr)) {
        note(e);
        match e {
            Expr::Call { callee, args } => {
                walk_expr(callee, note);
                for a in args {
                    walk_expr(a, note);
                }
            }
            Expr::Binary { left, right, .. } => {
                walk_expr(left, note);
                walk_expr(right, note);
            }
            Expr::Unary { operand, .. } => walk_expr(operand, note),
            Expr::Index { of, index } => {
                walk_expr(of, note);
                walk_expr(index, note);
            }
            Expr::Field { of, .. } => walk_expr(of, note),
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(inner) = part {
                        walk_expr(inner, note);
                    }
                }
            }
            Expr::ListLit(items) => {
                for item in items {
                    walk_expr(item, note);
                }
            }
            _ => {}
        }
    }

    /// The types of the values these statements return, structure walked whole.
    fn collect_returns(stmts: &[Stmt], out: &mut Vec<Type>) {
        for stmt in stmts {
            match stmt {
                Stmt::Return(Some(v)) => {
                    if let Some(ty) = returned_ty(v) {
                        out.push(ty);
                    }
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    collect_returns(then, out);
                    collect_returns(otherwise, out);
                }
                Stmt::While { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::CountedFor { body, .. }
                | Stmt::Block(body) => collect_returns(body, out),
                Stmt::Switch { arms, default, .. } => {
                    for (_, arm) in arms {
                        collect_returns(arm, out);
                    }
                    collect_returns(default, out);
                }
                _ => {}
            }
        }
    }

    /// The type a returned expression states, one step deeper than a literal:
    /// arithmetic returns numbers.
    fn returned_ty(e: &Expr) -> Option<Type> {
        if let Some(ty) = literal_ty(e) {
            return Some(ty);
        }
        match e {
            Expr::Binary {
                op:
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::FloorDiv
                    | BinaryOp::Rem,
                ..
            } => Some(Type::Int),
            _ => None,
        }
    }

    fn function_names(cx: &Cx, root: Node<'_>) -> Vec<String> {
        cx.children(root)
            .into_iter()
            .filter(|c| c.kind() == "function_definition")
            .filter_map(|c| cx.field_text(c, "name"))
            .collect()
    }

    /// The highest positional parameter a body reads.
    ///
    /// `$1` counts one digit, because `$10` is `${1}0` to bash itself; `${10}`
    /// counts them all, because the braces are how bash writes the tenth.
    fn arity(source: &str) -> usize {
        let mut highest = 0usize;
        for (at, c) in source.char_indices() {
            if c != '$' {
                continue;
            }
            let rest = &source[at + 1..];
            let digits: String = match rest.strip_prefix('{') {
                Some(braced) => braced.chars().take_while(|d| d.is_ascii_digit()).collect(),
                None => rest
                    .chars()
                    .take(1)
                    .filter(|d| d.is_ascii_digit())
                    .collect(),
            };
            if let Ok(n) = digits.parse::<usize>() {
                highest = highest.max(n);
            }
        }
        highest
    }

    /// The name a positional parameter crosses under.
    fn positional(n: usize) -> String {
        format!("a{n}")
    }

    fn function(cx: &Cx, node: Node<'_>, names: &[String]) -> Function {
        let name = cx.field_text(node, "name").unwrap_or_default();
        let body_node = cx.field(node, "body").or_else(|| {
            cx.children(node)
                .into_iter()
                .find(|c| c.kind() == "compound_statement")
        });
        let params: Vec<Param> = body_node
            .map(|b| arity(&cx.text(b)))
            .map(|n| {
                (1..=n)
                    .map(|i| Param {
                        name: positional(i),
                        ty: None,
                        default: None,
                        kind: ParamKind::Normal,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut body = body_node.map(|b| block(cx, b, names)).unwrap_or_default();
        // A bash function's value is its stdout, and the caller captures `$(f …)`.
        // An echo directly above a bare status return is the value leaving the
        // function, and so is an echo standing last in the body. Both read as the
        // return they are. An echo along the way is printing, and stays printing.
        settle_value_prints(&mut body);
        let prints = |s: &Stmt| {
            matches!(s, Stmt::Expr(Expr::Call { callee, .. })
                if matches!(callee.as_ref(), Expr::Name(n) if n == "print"))
        };
        let already_returns = body.iter().any(|s| matches!(s, Stmt::Return(Some(_))));
        let one_print_at_the_end = body.last().is_some_and(prints)
            && (already_returns || body.iter().filter(|s| prints(s)).count() == 1);
        if one_print_at_the_end {
            if let Some(Stmt::Expr(Expr::Call { mut args, .. })) = body.pop() {
                body.push(Stmt::Return(Some(args.remove(0))));
            }
        }
        Function {
            doc: Vec::new(),
            name,
            receiver: None,
            receiver_binding: None,
            params,
            returns: None,
            body,
            exported: true,
            is_async: false,
            is_property: false,
            is_constructor: false,
            is_private: false,
        }
    }

    /// Turn `echo value` above a bare `return` into the return it is.
    ///
    /// The pair is bash's spelling of "leave with this value". The caller reads
    /// the echo through `$(f …)`, and the status return carries no value of its own. A
    /// status of 0 counts as bare; any other status is a real exit code and the
    /// pair is left alone.
    fn settle_value_prints(body: &mut Vec<Stmt>) {
        let prints = |s: &Stmt| {
            matches!(s, Stmt::Expr(Expr::Call { callee, .. })
                if matches!(callee.as_ref(), Expr::Name(n) if n == "print"))
        };
        let bare_return = |s: &Stmt| match s {
            Stmt::Return(None) => true,
            Stmt::Return(Some(Expr::Int(text))) => text == "0",
            _ => false,
        };
        let mut at = 0;
        while at < body.len() {
            match &mut body[at] {
                Stmt::If {
                    then, otherwise, ..
                } => {
                    settle_value_prints(then);
                    settle_value_prints(otherwise);
                }
                Stmt::Switch { arms, default, .. } => {
                    for (_, arm) in arms.iter_mut() {
                        settle_value_prints(arm);
                    }
                    settle_value_prints(default);
                }
                _ => {}
            }
            if prints(&body[at]) && body.get(at + 1).is_some_and(bare_return) {
                let Stmt::Expr(Expr::Call { mut args, .. }) = body.remove(at) else {
                    unreachable!("just matched a print");
                };
                body[at] = Stmt::Return(Some(args.remove(0)));
            }
            at += 1;
        }
    }

    /// The statements of a `{ … }`, `do … done` or `then … fi` body.
    fn block(cx: &Cx, node: Node<'_>, names: &[String]) -> Vec<Stmt> {
        cx.children_with_comments(node)
            .into_iter()
            .map(|c| match c.kind() {
                "comment" => Stmt::Comment(cx.text(c).trim_start_matches('#').trim().to_string()),
                _ => stmt(cx, c, names),
            })
            .collect()
    }

    fn stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        match node.kind() {
            "variable_assignment" => assignment(cx, node, names),
            "declaration_command" => declaration(cx, node, names),
            "command" => command_stmt(cx, node, names),
            "if_statement" => if_stmt(cx, node, names),
            "while_statement" => while_stmt(cx, node, names),
            "for_statement" => for_stmt(cx, node, names),
            "c_style_for_statement" => counted_for(cx, node, names),
            "case_statement" => case_stmt(cx, node, names),
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// `x=5`: bash declares by assigning, so the first spelling and every later one
    /// are the same statement. The writers that separate the two settle it the way
    /// they settle Python's assignments.
    fn assignment(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let Some(name) = cx.field_text(node, "name") else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let value_node = cx.field(node, "value");
        let value = match value_node {
            Some(v) => word_expr(cx, v, names),
            None => Expr::Str(String::new()),
        };
        if matches!(value, Expr::Unsupported(_)) {
            return Stmt::Unsupported(cx.unsupported(node));
        }
        // The `+=` between name and value is an anonymous token; the text shows it.
        let appending = value_node.is_some_and(|v| {
            cx.text(node)[name.len()..v.start_byte() - node.start_byte()].contains("+=")
        });
        // `arr[i]=v` assigns one element; the name field carries the subscript.
        if let Some((array, index)) = name.split_once('[') {
            if appending {
                return Stmt::Unsupported(cx.unsupported(node));
            }
            let index = index.trim_end_matches(']').trim_start_matches('$');
            let index = if index.chars().all(|c| c.is_ascii_digit()) {
                Expr::Int(index.to_string())
            } else if index.chars().all(|c| c.is_alphanumeric() || c == '_') {
                Expr::Name(index.to_string())
            } else {
                return Stmt::Unsupported(cx.unsupported(node));
            };
            return Stmt::Assign {
                target: Expr::Index {
                    of: Box::new(Expr::Name(array.to_string())),
                    index: Box::new(index),
                },
                value,
            };
        }
        if appending {
            // `xs+=(v)` grows the array; `n+=2` grows the number; text grows text.
            return match value {
                Expr::ListLit(elements) => {
                    let appends: Vec<Stmt> = elements
                        .into_iter()
                        .map(|element| {
                            Stmt::Expr(Expr::Call {
                                callee: Box::new(Expr::Field {
                                    of: Box::new(Expr::Name(name.clone())),
                                    name: "append".to_string(),
                                }),
                                args: vec![element],
                            })
                        })
                        .collect();
                    match appends.len() {
                        1 => appends.into_iter().next().expect("one"),
                        _ => Stmt::Block(appends),
                    }
                }
                value => Stmt::Assign {
                    target: Expr::Name(name.clone()),
                    value: Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Name(name)),
                        right: Box::new(value),
                    },
                },
            };
        }
        Stmt::Assign {
            target: Expr::Name(name),
            value,
        }
    }

    /// `local x=5`, `readonly N=3`, `declare x`: a declaration with bash's own
    /// keywords. `export` also reaches the environment, which no target has; the
    /// binding crosses and the export is noted by the carry.
    fn declaration(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let children = cx.children(node);
        // The keyword is an anonymous token, so it is read from the raw first child
        // and never appears among the named ones.
        let keyword = node.child(0).map(|c| cx.text(c)).unwrap_or_default();
        let assignments: Vec<Node<'_>> = children
            .iter()
            .filter(|c| c.kind() == "variable_assignment")
            .copied()
            .collect();
        let plain: Vec<Node<'_>> = children
            .iter()
            .filter(|c| c.kind() == "variable_name")
            .copied()
            .collect();
        match (keyword.as_str(), assignments.as_slice(), plain.as_slice()) {
            ("local" | "declare" | "readonly", [one], []) => {
                let Some(name) = cx.field_text(*one, "name") else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let value = cx.field(*one, "value").map(|v| word_expr(cx, v, names));
                if matches!(value, Some(Expr::Unsupported(_))) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Stmt::Let {
                    name,
                    ty: None,
                    value,
                    mutable: keyword != "readonly",
                }
            }
            ("local" | "declare", [], [one]) => Stmt::Let {
                name: cx.text(*one),
                ty: None,
                value: None,
                mutable: true,
            },
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    /// A command in statement position: `echo`, `return`, a call to a function this
    /// file declares. Anything else is a program outside this file, and carries.
    fn command_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let Some(name) = cx.field(node, "name") else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let command = cx.text(name);
        let args: Vec<Node<'_>> = cx
            .children(node)
            .into_iter()
            .skip_while(|c| c.start_byte() <= name.start_byte())
            .collect();
        match command.as_str() {
            "break" if args.is_empty() => Stmt::Break,
            "continue" if args.is_empty() => Stmt::Continue,
            "echo" => {
                let mut parts: Vec<Expr> = Vec::new();
                for arg in &args {
                    // `-n` and friends change how echo prints, which no print does.
                    if cx.text(*arg).starts_with('-') {
                        return Stmt::Unsupported(cx.unsupported(node));
                    }
                    parts.push(word_expr(cx, *arg, names));
                }
                if parts.iter().any(|p| matches!(p, Expr::Unsupported(_))) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                // `echo one two` prints its arguments joined by single spaces.
                let printed = match parts.len() {
                    0 => Expr::Str(String::new()),
                    1 => parts.remove(0),
                    _ => {
                        let mut joined: Vec<TemplatePart> = Vec::new();
                        for (i, part) in parts.into_iter().enumerate() {
                            if i > 0 {
                                joined.push(TemplatePart::Text(" ".to_string()));
                            }
                            match part {
                                Expr::Str(text) => joined.push(TemplatePart::Text(text)),
                                other => joined.push(TemplatePart::Expr(other)),
                            }
                        }
                        Expr::Template(joined)
                    }
                };
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Name("print".to_string())),
                    args: vec![printed],
                })
            }
            "return" => {
                let value = args.first().map(|a| word_expr(cx, *a, names));
                if matches!(value, Some(Expr::Unsupported(_))) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Stmt::Return(value)
            }
            _ if names.iter().any(|n| n == &command) => {
                let args: Vec<Expr> = args.iter().map(|a| word_expr(cx, *a, names)).collect();
                if args.iter().any(|a| matches!(a, Expr::Unsupported(_))) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Name(command)),
                    args,
                })
            }
            _ => Stmt::Unsupported(cx.unsupported(node)),
        }
    }

    fn if_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let children = cx.children(node);
        let Some((head, rest)) = children.split_first() else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let Some(cond) = condition(cx, *head, names) else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let mut then: Vec<Stmt> = Vec::new();
        let mut clauses: Vec<Node<'_>> = Vec::new();
        for c in rest {
            match c.kind() {
                "elif_clause" | "else_clause" => clauses.push(*c),
                "comment" => then.push(Stmt::Comment(
                    cx.text(*c).trim_start_matches('#').trim().to_string(),
                )),
                _ => then.push(stmt(cx, *c, names)),
            }
        }
        let mut otherwise: Vec<Stmt> = Vec::new();
        for clause in clauses.iter().rev() {
            match clause.kind() {
                "else_clause" => {
                    otherwise = cx
                        .children(*clause)
                        .into_iter()
                        .map(|c| stmt(cx, c, names))
                        .collect();
                }
                "elif_clause" => {
                    let kids = cx.children(*clause);
                    let Some((chead, cbody)) = kids.split_first() else {
                        return Stmt::Unsupported(cx.unsupported(node));
                    };
                    let Some(ccond) = condition(cx, *chead, names) else {
                        return Stmt::Unsupported(cx.unsupported(node));
                    };
                    let inner: Vec<Stmt> = cbody.iter().map(|c| stmt(cx, *c, names)).collect();
                    otherwise = vec![Stmt::If {
                        condition: ccond,
                        then: inner,
                        otherwise: std::mem::take(&mut otherwise),
                    }];
                }
                _ => {}
            }
        }
        Stmt::If {
            condition: cond,
            then,
            otherwise,
        }
    }

    fn while_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let children = cx.children(node);
        let Some((head, rest)) = children.split_first() else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let Some(cond) = condition(cx, *head, names) else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let body: Vec<Stmt> = rest
            .iter()
            .filter(|c| c.kind() == "do_group")
            .flat_map(|group| block(cx, *group, names))
            .collect();
        Stmt::While {
            condition: cond,
            body,
        }
    }

    /// `for x in one two three`: each word in turn. `for x in "${xs[@]}"`: each
    /// element of the array, which is the same loop over a named sequence.
    fn for_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let Some(binding) = cx.field_text(node, "variable") else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let children = cx.children(node);
        let values: Vec<Node<'_>> = children
            .iter()
            .filter(|c| !matches!(c.kind(), "variable_name" | "do_group" | "comment"))
            .copied()
            .collect();
        let iterable = match values.as_slice() {
            [one] => match whole_array(cx, *one) {
                Some(name) => Expr::Name(name),
                None => {
                    let element = word_expr(cx, *one, names);
                    match element {
                        Expr::Unsupported(_) => return Stmt::Unsupported(cx.unsupported(node)),
                        one => Expr::ListLit(vec![one]),
                    }
                }
            },
            several => {
                let elements: Vec<Expr> =
                    several.iter().map(|v| word_expr(cx, *v, names)).collect();
                if elements.iter().any(|e| matches!(e, Expr::Unsupported(_))) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                Expr::ListLit(elements)
            }
        };
        let body: Vec<Stmt> = children
            .iter()
            .filter(|c| c.kind() == "do_group")
            .flat_map(|group| block(cx, *group, names))
            .collect();
        Stmt::ForEach {
            binding,
            iterable,
            body,
        }
    }

    /// `"${xs[@]}"`: the whole array, one element per pass, spaces preserved.
    fn whole_array(cx: &Cx, node: Node<'_>) -> Option<String> {
        let text = cx.text(node);
        let inner = text
            .trim_matches('"')
            .strip_prefix("${")?
            .strip_suffix("[@]}")?;
        let plain = !inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || c == '_');
        plain.then(|| inner.to_string())
    }

    fn counted_for(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        // The header's first clause declares the counter, in every target's spelling
        // of this loop, so the assignment bash writes reads as the binding it is.
        let init = cx
            .field(node, "initializer")
            .map(|n| match arith_stmt(cx, n, names) {
                Stmt::Assign {
                    target: Expr::Name(name),
                    value,
                } => Stmt::Let {
                    name,
                    ty: None,
                    value: Some(value),
                    mutable: true,
                },
                other => other,
            })
            .map(Box::new);
        let condition = cx.field(node, "condition").map(|n| arith(cx, n, names));
        let update = cx
            .field(node, "update")
            .map(|n| Box::new(arith_stmt(cx, n, names)));
        let carried = init
            .as_deref()
            .is_some_and(|s| matches!(s, Stmt::Unsupported(_)))
            || matches!(condition, Some(Expr::Unsupported(_)))
            || update
                .as_deref()
                .is_some_and(|s| matches!(s, Stmt::Unsupported(_)));
        if carried {
            return Stmt::Unsupported(cx.unsupported(node));
        }
        let body: Vec<Stmt> = cx
            .children(node)
            .into_iter()
            .filter(|c| c.kind() == "do_group")
            .flat_map(|group| block(cx, group, names))
            .collect();
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            source: cx.text(node),
            line: cx.line(node),
        }
    }

    fn case_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        let Some(value) = cx.field(node, "value") else {
            return Stmt::Unsupported(cx.unsupported(node));
        };
        let subject = word_expr(cx, value, names);
        if matches!(subject, Expr::Unsupported(_)) {
            return Stmt::Unsupported(cx.unsupported(node));
        }
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Vec<Stmt> = Vec::new();
        for item in cx
            .children(node)
            .into_iter()
            .filter(|c| c.kind() == "case_item")
        {
            let kids = cx.children(item);
            let (patterns, body): (Vec<Node<'_>>, Vec<Node<'_>>) =
                kids.into_iter().partition(|c| {
                    matches!(
                        c.kind(),
                        "word" | "string" | "raw_string" | "number" | "extglob_pattern"
                    )
                });
            let body: Vec<Stmt> = body.into_iter().map(|c| stmt(cx, c, names)).collect();
            let texts: Vec<String> = patterns.iter().map(|p| cx.text(*p)).collect();
            if texts.iter().any(|t| t == "*") {
                default = body;
                continue;
            }
            // A glob selects by shape, and no target's switch does. The whole
            // statement carries rather than matching fewer strings than the source.
            if texts.iter().any(|t| t.contains(['*', '?', '['])) {
                return Stmt::Unsupported(cx.unsupported(node));
            }
            // `two|three)` arrives as one pattern; the alternatives are its pieces.
            let mut selectors: Vec<Expr> = Vec::new();
            for text in &texts {
                for piece in text.split('|') {
                    let piece = piece.trim();
                    let unquoted = piece
                        .strip_prefix('"')
                        .and_then(|p| p.strip_suffix('"'))
                        .or_else(|| piece.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')))
                        .unwrap_or(piece);
                    if unquoted.is_empty() {
                        return Stmt::Unsupported(cx.unsupported(node));
                    }
                    if unquoted.chars().all(|c| c.is_ascii_digit()) {
                        selectors.push(Expr::Int(unquoted.to_string()));
                    } else {
                        selectors.push(Expr::Str(unquoted.to_string()));
                    }
                }
            }
            arms.push((selectors, body));
        }
        Stmt::Switch {
            subject,
            arms,
            default,
        }
    }

    /// The condition a loop or branch tests: `[ … ]`, `[[ … ]]`, `(( … ))`, or a
    /// negation of one. A condition that is a command's exit status has no
    /// counterpart the targets share, so the whole construct carries.
    fn condition(cx: &Cx, node: Node<'_>, names: &[String]) -> Option<Expr> {
        match node.kind() {
            "test_command" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => test_expr(cx, *one, names),
                    _ => None,
                }
            }
            "arithmetic_expansion" | "compound_statement" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => {
                        Some(arith(cx, *one, names)).filter(|e| !matches!(e, Expr::Unsupported(_)))
                    }
                    _ => None,
                }
            }
            "negated_command" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => condition(cx, *one, names).map(|c| Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(c),
                    }),
                    _ => None,
                }
            }
            "list" => {
                // `[ a ] && [ b ]`: both must hold; `||`: either.
                let kids = cx.children(node);
                if kids.len() != 2 {
                    return None;
                }
                let text = cx.text(node);
                let op = if text.contains("&&") {
                    BinaryOp::And
                } else if text.contains("||") {
                    BinaryOp::Or
                } else {
                    return None;
                };
                let left = condition(cx, kids[0], names)?;
                let right = condition(cx, kids[1], names)?;
                Some(Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            _ => None,
        }
    }

    /// The inside of `[ … ]` and `[[ … ]]`.
    fn test_expr(cx: &Cx, node: Node<'_>, names: &[String]) -> Option<Expr> {
        match node.kind() {
            "binary_expression" => {
                let left = cx.field(node, "left")?;
                let right = cx.field(node, "right")?;
                let operator = cx.field_text(node, "operator")?;
                let op = match operator.as_str() {
                    "-eq" | "=" | "==" => BinaryOp::Eq,
                    "-ne" | "!=" => BinaryOp::Ne,
                    "-lt" | "<" => BinaryOp::Lt,
                    "-le" => BinaryOp::Le,
                    "-gt" | ">" => BinaryOp::Gt,
                    "-ge" => BinaryOp::Ge,
                    "&&" => BinaryOp::And,
                    "||" => BinaryOp::Or,
                    _ => return None,
                };
                let (left, right) = match (op, ()) {
                    (BinaryOp::And | BinaryOp::Or, ()) => {
                        (test_expr(cx, left, names)?, test_expr(cx, right, names)?)
                    }
                    _ => (word_expr(cx, left, names), word_expr(cx, right, names)),
                };
                if matches!(left, Expr::Unsupported(_)) || matches!(right, Expr::Unsupported(_)) {
                    return None;
                }
                Some(Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            "unary_expression" => {
                let text = cx.text(node);
                let inner = cx.children(node);
                let [operand] = inner.as_slice() else {
                    return None;
                };
                let value = word_expr(cx, *operand, names);
                if matches!(value, Expr::Unsupported(_)) {
                    return None;
                }
                // `-z`: empty; `-n`: not empty. Both are a comparison with the
                // empty string, which every target can spell.
                if text.starts_with("-z") {
                    return Some(Expr::Binary {
                        op: BinaryOp::Eq,
                        left: Box::new(value),
                        right: Box::new(Expr::Str(String::new())),
                    });
                }
                if text.starts_with("-n") {
                    return Some(Expr::Binary {
                        op: BinaryOp::Ne,
                        left: Box::new(value),
                        right: Box::new(Expr::Str(String::new())),
                    });
                }
                if text.starts_with('!') {
                    return test_expr(cx, *operand, names).map(|e| Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(e),
                    });
                }
                None
            }
            "parenthesized_expression" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => test_expr(cx, *one, names),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A statement inside `(( … ))`: an assignment, a step, or a bare expression.
    fn arith_stmt(cx: &Cx, node: Node<'_>, names: &[String]) -> Stmt {
        match node.kind() {
            "variable_assignment" => assignment(cx, node, names),
            "binary_expression" => {
                // `i+=2` and `i=i+2` both arrive as binary expressions here.
                let text = cx.text(node);
                let Some(operator) = cx.field_text(node, "operator") else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let (Some(left), Some(right)) = (cx.field(node, "left"), cx.field(node, "right"))
                else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                let target = arith(cx, left, names);
                let value = arith(cx, right, names);
                if matches!(target, Expr::Unsupported(_)) || matches!(value, Expr::Unsupported(_)) {
                    return Stmt::Unsupported(cx.unsupported(node));
                }
                let combined = |op: BinaryOp| Stmt::Assign {
                    target: target.clone(),
                    value: Expr::Binary {
                        op,
                        left: Box::new(target.clone()),
                        right: Box::new(value.clone()),
                    },
                };
                match operator.as_str() {
                    "=" => Stmt::Assign { target, value },
                    "+=" => combined(BinaryOp::Add),
                    "-=" => combined(BinaryOp::Sub),
                    "*=" => combined(BinaryOp::Mul),
                    "/=" => combined(BinaryOp::Div),
                    "%=" => combined(BinaryOp::Rem),
                    _ => {
                        let _ = text;
                        Stmt::Unsupported(cx.unsupported(node))
                    }
                }
            }
            "postfix_expression" => {
                // `i++` and `i--`.
                let text = cx.text(node);
                let name = text.trim_end_matches(['+', '-']).trim().to_string();
                let op = if text.ends_with("++") {
                    BinaryOp::Add
                } else if text.ends_with("--") {
                    BinaryOp::Sub
                } else {
                    return Stmt::Unsupported(cx.unsupported(node));
                };
                Stmt::Assign {
                    target: Expr::Name(name.clone()),
                    value: Expr::Binary {
                        op,
                        left: Box::new(Expr::Name(name)),
                        right: Box::new(Expr::Int("1".to_string())),
                    },
                }
            }
            _ => {
                let value = arith(cx, node, names);
                match value {
                    Expr::Unsupported(_) => Stmt::Unsupported(cx.unsupported(node)),
                    value => Stmt::Expr(value),
                }
            }
        }
    }

    /// An expression inside `(( … ))`, where a bare name is a variable and `$x`
    /// says the same thing.
    fn arith(cx: &Cx, node: Node<'_>, names: &[String]) -> Expr {
        match node.kind() {
            "number" => Expr::Int(cx.text(node)),
            "word" => {
                let text = cx.text(node);
                if text.chars().all(|c| c.is_ascii_digit()) {
                    Expr::Int(text)
                } else if text.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    Expr::Name(text)
                } else {
                    Expr::Unsupported(cx.unsupported(node))
                }
            }
            "variable_name" => Expr::Name(cx.text(node)),
            "simple_expansion" | "expansion" => word_expr(cx, node, names),
            "binary_expression" => {
                let (Some(left), Some(right), Some(operator)) = (
                    cx.field(node, "left"),
                    cx.field(node, "right"),
                    cx.field_text(node, "operator"),
                ) else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                let op = match operator.as_str() {
                    "+" => BinaryOp::Add,
                    "-" => BinaryOp::Sub,
                    "*" => BinaryOp::Mul,
                    "/" => BinaryOp::Div,
                    "%" => BinaryOp::Rem,
                    "==" => BinaryOp::Eq,
                    "!=" => BinaryOp::Ne,
                    "<" => BinaryOp::Lt,
                    "<=" => BinaryOp::Le,
                    ">" => BinaryOp::Gt,
                    ">=" => BinaryOp::Ge,
                    "&&" => BinaryOp::And,
                    "||" => BinaryOp::Or,
                    _ => return Expr::Unsupported(cx.unsupported(node)),
                };
                let left = arith(cx, left, names);
                let right = arith(cx, right, names);
                if matches!(left, Expr::Unsupported(_)) || matches!(right, Expr::Unsupported(_)) {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            "unary_expression" => {
                let inner = cx.children(node);
                let [operand] = inner.as_slice() else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                let value = arith(cx, *operand, names);
                if matches!(value, Expr::Unsupported(_)) {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                let text = cx.text(node);
                if text.starts_with('-') {
                    Expr::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(value),
                    }
                } else if text.starts_with('!') {
                    Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(value),
                    }
                } else {
                    Expr::Unsupported(cx.unsupported(node))
                }
            }
            "parenthesized_expression" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => arith(cx, *one, names),
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }

    /// A word in command or test position. A literal, a variable, a string with
    /// expansions, an arithmetic expansion, or a substitution over a function this
    /// file declares.
    fn word_expr(cx: &Cx, node: Node<'_>, names: &[String]) -> Expr {
        match node.kind() {
            "word" => {
                let text = cx.text(node);
                let numeric = !text.is_empty()
                    && text
                        .strip_prefix('-')
                        .unwrap_or(&text)
                        .chars()
                        .all(|c| c.is_ascii_digit());
                if numeric {
                    Expr::Int(text)
                } else {
                    Expr::Str(text)
                }
            }
            "number" => Expr::Int(cx.text(node)),
            "raw_string" => Expr::Str(cx.text(node).trim_matches('\'').to_string()),
            "string" => string_expr(cx, node, names),
            "concatenation" => {
                let parts: Vec<Expr> = cx
                    .children(node)
                    .into_iter()
                    .map(|c| word_expr(cx, c, names))
                    .collect();
                if parts.iter().any(|p| matches!(p, Expr::Unsupported(_))) {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                let mut joined: Vec<TemplatePart> = Vec::new();
                for part in parts {
                    match part {
                        Expr::Str(text) => joined.push(TemplatePart::Text(text)),
                        other => joined.push(TemplatePart::Expr(other)),
                    }
                }
                Expr::Template(joined)
            }
            "simple_expansion" => {
                let inner = cx.text(node);
                let name = inner.trim_start().trim_start_matches('$');
                expansion_name(cx, node, name)
            }
            "expansion" => {
                // The grammar folds whitespace between two expansions into the
                // second one's span; the name starts at the `$`.
                let inner = cx.text(node);
                let name = inner
                    .trim_start()
                    .trim_start_matches('$')
                    .trim_start_matches('{')
                    .trim_end_matches('}');
                // `${#xs[@]}` is the array's length, which every target spells.
                if let Some(counted) = name.strip_prefix('#') {
                    if let Some(array) = counted.strip_suffix("[@]") {
                        return Expr::Call {
                            callee: Box::new(Expr::Name("len".to_string())),
                            args: vec![Expr::Name(array.to_string())],
                        };
                    }
                    return Expr::Call {
                        callee: Box::new(Expr::Name("len".to_string())),
                        args: vec![Expr::Name(counted.to_string())],
                    };
                }
                // `${xs[i]}` reads one element.
                if let Some((array, index)) = name.split_once('[') {
                    let index = index.trim_end_matches(']');
                    if index == "@" || index == "*" {
                        return Expr::Name(array.to_string());
                    }
                    let index_expr = if index.chars().all(|c| c.is_ascii_digit()) {
                        Expr::Int(index.to_string())
                    } else if index.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        Expr::Name(index.to_string())
                    } else {
                        return Expr::Unsupported(cx.unsupported(node));
                    };
                    return Expr::Index {
                        of: Box::new(Expr::Name(array.to_string())),
                        index: Box::new(index_expr),
                    };
                }
                expansion_name(cx, node, name)
            }
            "arithmetic_expansion" => {
                let inner = cx.children(node);
                match inner.as_slice() {
                    [one] => arith(cx, *one, names),
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "command_substitution" => {
                let inner = cx.children(node);
                let [command] = inner.as_slice() else {
                    return Expr::Unsupported(cx.unsupported(node));
                };
                if command.kind() != "command" {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                match command_stmt(cx, *command, names) {
                    Stmt::Expr(call @ Expr::Call { .. }) => call,
                    _ => Expr::Unsupported(cx.unsupported(node)),
                }
            }
            "array" => {
                let elements: Vec<Expr> = cx
                    .children(node)
                    .into_iter()
                    .map(|c| word_expr(cx, c, names))
                    .collect();
                if elements.iter().any(|e| matches!(e, Expr::Unsupported(_))) {
                    return Expr::Unsupported(cx.unsupported(node));
                }
                Expr::ListLit(elements)
            }
            _ => Expr::Unsupported(cx.unsupported(node)),
        }
    }

    /// A variable read by name: `$x`, `${x}`, or a positional that crosses under
    /// the name its parameter declares.
    fn expansion_name(cx: &Cx, node: Node<'_>, name: &str) -> Expr {
        if let Ok(n) = name.parse::<usize>() {
            if n >= 1 {
                return Expr::Name(positional(n));
            }
        }
        let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        match plain {
            true => Expr::Name(name.to_string()),
            false => Expr::Unsupported(cx.unsupported(node)),
        }
    }

    /// A double-quoted string: text and expansions interleaved.
    fn string_expr(cx: &Cx, node: Node<'_>, names: &[String]) -> Expr {
        let inner = cx.children(node);
        if inner.is_empty() {
            return Expr::Str(cx.text(node).trim_matches('"').to_string());
        }
        let mut parts: Vec<TemplatePart> = Vec::new();
        for piece in inner {
            match piece.kind() {
                "string_content" => parts.push(TemplatePart::Text(cx.text(piece))),
                _ => {
                    // Whitespace the grammar folded into this piece's span is text
                    // of the string, and dropping it would join two words.
                    let raw = cx.text(piece);
                    if let Some(at) = raw.find('$').filter(|at| *at > 0) {
                        parts.push(TemplatePart::Text(raw[..at].to_string()));
                    }
                    match word_expr(cx, piece, names) {
                        Expr::Unsupported(_) => return Expr::Unsupported(cx.unsupported(node)),
                        Expr::Str(text) => parts.push(TemplatePart::Text(text)),
                        other => parts.push(TemplatePart::Expr(other)),
                    }
                }
            }
        }
        match parts.as_slice() {
            [TemplatePart::Text(text)] => Expr::Str(text.clone()),
            [TemplatePart::Expr(e)] => e.clone(),
            _ => Expr::Template(parts),
        }
    }
}
