//! Each language's spelling of the shared builtins, folded into one canonical form.
//!
//! The IR speaks one small vocabulary for the operations every language has: `print`,
//! `len`, `str`, and the rest of the names the writers' builtin tables spell out per
//! target. The Rust reader already folds `println!` into that vocabulary. This pass
//! does the same for the languages whose print is an ordinary call: `fmt.Println` and
//! its family, `console.log`, `System.out.println`.
//!
//! Folding happens here, after the read, rather than inside each reader: the readers
//! stay faithful to their grammars, and the vocabulary lives in one place beside the
//! table of what it contains.

use super::ir::*;
use crate::lang::Language;

/// Rewrite `module`'s expressions into the canonical vocabulary for `language`.
pub fn normalize(module: &mut Module, language: Language) {
    strip_lowering_helpers(module);
    let rewrite: fn(Expr) -> Expr = match language {
        Language::Go => {
            go_module(module);
            go
        }
        Language::TypeScript | Language::Tsx => {
            settle_exception_classes(module);
            typescript
        }
        Language::Java => {
            settle_exception_classes(module);
            java
        }
        Language::Python => {
            settle_exception_classes(module);
            return;
        }
        Language::Rust => {
            settle_result_idiom(module);
            return;
        }
        Language::Zig => {
            zig_module(module);
            settle_result_idiom(module);
            return;
        }
        _ => return,
    };
    for item in &mut module.items {
        match item {
            Item::Function(f) => map_function(f, rewrite),
            Item::Record(r) => {
                for method in &mut r.methods {
                    map_function(method, rewrite);
                }
            }
            Item::Statement(s) => map_stmt(s, rewrite),
            Item::Test { body, .. } => {
                for s in body {
                    map_stmt(s, rewrite);
                }
            }
            _ => {}
        }
    }
}

fn map_function(function: &mut Function, rewrite: fn(Expr) -> Expr) {
    for stmt in &mut function.body {
        map_stmt(stmt, rewrite);
    }
    for param in &mut function.params {
        if let Some(default) = &mut param.default {
            map_expr(default, rewrite);
        }
    }
}

/// Apply `rewrite` to every expression under `stmt`, bottom-up.
fn map_stmt(stmt: &mut Stmt, rewrite: fn(Expr) -> Expr) {
    let bodies: Vec<&mut Vec<Stmt>> = match stmt {
        Stmt::Return(value) => {
            if let Some(value) = value {
                map_expr(value, rewrite);
            }
            return;
        }
        Stmt::Let { value, .. } => {
            if let Some(value) = value {
                map_expr(value, rewrite);
            }
            return;
        }
        Stmt::Assign { target, value } => {
            map_expr(target, rewrite);
            map_expr(value, rewrite);
            return;
        }
        Stmt::TupleAssign { value, .. } => {
            map_expr(value, rewrite);
            return;
        }
        Stmt::If {
            condition,
            then,
            otherwise,
        } => {
            map_expr(condition, rewrite);
            vec![then, otherwise]
        }
        Stmt::IfPresent {
            value,
            then,
            otherwise,
            ..
        } => {
            map_expr(value, rewrite);
            vec![then, otherwise]
        }
        Stmt::While { condition, body } => {
            map_expr(condition, rewrite);
            vec![body]
        }
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                map_stmt(init, rewrite);
            }
            if let Some(condition) = condition {
                map_expr(condition, rewrite);
            }
            if let Some(update) = update {
                map_stmt(update, rewrite);
            }
            vec![body]
        }
        Stmt::ForEach { iterable, body, .. } => {
            map_expr(iterable, rewrite);
            vec![body]
        }
        Stmt::ForEachIndexed { iterable, body, .. } => {
            map_expr(iterable, rewrite);
            vec![body]
        }
        Stmt::WhilePresent { value, body, .. } => {
            map_expr(value, rewrite);
            vec![body]
        }
        Stmt::Defer(body) | Stmt::ErrDefer(body) => vec![body],
        Stmt::Switch {
            subject,
            arms,
            default,
        } => {
            map_expr(subject, rewrite);
            for (patterns, arm) in arms.iter_mut() {
                for pattern in patterns {
                    map_expr(pattern, rewrite);
                }
                for s in arm {
                    map_stmt(s, rewrite);
                }
            }
            vec![default]
        }
        Stmt::MatchVariants {
            subject,
            arms,
            default,
            ..
        } => {
            map_expr(subject, rewrite);
            for arm in arms.iter_mut() {
                for s in &mut arm.body {
                    map_stmt(s, rewrite);
                }
            }
            vec![default]
        }
        Stmt::Expr(e) => {
            map_expr(e, rewrite);
            return;
        }
        Stmt::Assert { condition, message } => {
            map_expr(condition, rewrite);
            if let Some(message) = message {
                map_expr(message, rewrite);
            }
            return;
        }
        Stmt::Throw(e) => {
            map_expr(e, rewrite);
            return;
        }
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            for catch in catches.iter_mut() {
                for s in &mut catch.body {
                    map_stmt(s, rewrite);
                }
            }
            vec![body, finally]
        }
        Stmt::Comment(_) | Stmt::Unsupported(_) | Stmt::Break | Stmt::Continue => return,
    };
    for body in bodies {
        for s in body {
            map_stmt(s, rewrite);
        }
    }
}

/// Apply `rewrite` to `expr` and everything under it, children first.
fn map_expr(expr: &mut Expr, rewrite: fn(Expr) -> Expr) {
    let walk = |e: &mut Expr| map_expr(e, rewrite);
    match expr {
        Expr::Field { of, .. } => walk(of),
        Expr::Index { of, index } => {
            walk(of);
            walk(index);
        }
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            walk(callee);
            for a in args {
                walk(a);
            }
        }
        Expr::Binary { left, right, .. } => {
            walk(left);
            walk(right);
        }
        Expr::Unary { operand, .. } => walk(operand),
        Expr::Await(inner) | Expr::Propagate(inner) => walk(inner),
        Expr::Template(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    walk(e);
                }
            }
        }
        _ => {}
    }
    let owned = std::mem::replace(expr, Expr::Null);
    *expr = rewrite(owned);
}

/// The pieces of a call: receiver path (dotted) and arguments, if `expr` is a call.
fn call_parts(expr: &Expr) -> Option<(Vec<&str>, &Vec<Expr>)> {
    let Expr::Call { callee, args } = expr else {
        return None;
    };
    let mut path = Vec::new();
    let mut at: &Expr = callee;
    loop {
        match at {
            Expr::Name(name) => {
                path.push(name.as_str());
                break;
            }
            Expr::Field { of, name } => {
                path.push(name.as_str());
                at = of;
            }
            _ => return None,
        }
    }
    path.reverse();
    Some((path, args))
}

fn print_call(args: Vec<Expr>) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Name("print".into())),
        args,
    }
}

// ------------------------------------------------------------------------- Go

/// `fmt.Println`, `fmt.Printf` and `fmt.Sprintf`, folded to `print` and templates.
fn go(expr: Expr) -> Expr {
    if let Some(folded) = fold_concat(&expr) {
        return folded;
    }
    let Some((path, args)) = call_parts(&expr) else {
        return expr;
    };
    match path.as_slice() {
        ["fmt", "Println"] => {
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            print_call(args)
        }
        // `Printf` whose format ends in a newline says the same thing `print` says.
        // One that does not would need a print-without-newline in every target, so it
        // stays as it was and the ledger shows it.
        ["fmt", "Printf"] => match sprintf_template(args) {
            Some(Expr::Template(mut parts)) if template_ends_with_newline(&parts) => {
                trim_trailing_newline(&mut parts);
                print_call(vec![flatten_template(parts)])
            }
            _ => expr,
        },
        ["fmt", "Sprintf"] => sprintf_template(args).unwrap_or(expr),
        _ => expr,
    }
}

/// A `%`-verb format string and its arguments, as a template.
///
/// Only the verbs whose meaning every target shares: `%d`, `%s`, `%v`, `%f`, and `%%`
/// for a literal percent. Width, precision or flags mean formatting this cannot
/// promise, so the call stays as written.
fn sprintf_template(args: &[Expr]) -> Option<Expr> {
    let (Expr::Str(format), rest) = args.split_first()? else {
        return None;
    };
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut text = String::new();
    let mut holes = rest.iter();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            text.push(c);
            continue;
        }
        match chars.next()? {
            '%' => text.push('%'),
            'd' | 's' | 'v' | 'f' => {
                if !text.is_empty() {
                    parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                }
                parts.push(TemplatePart::Expr(holes.next()?.clone()));
            }
            _ => return None,
        }
    }
    if holes.next().is_some() {
        return None;
    }
    if !text.is_empty() {
        parts.push(TemplatePart::Text(text));
    }
    Some(Expr::Template(parts))
}

fn template_ends_with_newline(parts: &[TemplatePart]) -> bool {
    matches!(parts.last(), Some(TemplatePart::Text(t)) if t.ends_with('\n'))
}

fn trim_trailing_newline(parts: &mut Vec<TemplatePart>) {
    if let Some(TemplatePart::Text(t)) = parts.last_mut() {
        t.pop();
        if t.is_empty() {
            parts.pop();
        }
    }
}

/// A template that is one bare expression is that expression.
fn flatten_template(parts: Vec<TemplatePart>) -> Expr {
    match parts.as_slice() {
        [TemplatePart::Expr(e)] => e.clone(),
        _ => Expr::Template(parts),
    }
}

// ----------------------------------------------------------------- TypeScript

/// `console.log` folded to `print`, and `Math.trunc` of a division folded to the
/// floor-division operator the other targets spell natively.
fn typescript(expr: Expr) -> Expr {
    if let Some(folded) = fold_concat(&expr) {
        return folded;
    }
    let Some((path, args)) = call_parts(&expr) else {
        return expr;
    };
    match (path.as_slice(), args.as_slice()) {
        (["console", "log"], _) => {
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            print_call(args)
        }
        // `Math.trunc(a / b)` is how this language spells the division every other
        // target truncates natively. `Math.floor` says the same thing for the
        // non-negative operands real code feeds it, and both read back as the
        // operator, so a round trip is the identity.
        (["Math", "trunc" | "floor"], [Expr::Binary { op: BinaryOp::Div, .. }]) => {
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            let Some(Expr::Binary { left, right, .. }) = args.into_iter().next() else {
                unreachable!("the guard said so");
            };
            Expr::Binary {
                op: BinaryOp::FloorDiv,
                left,
                right,
            }
        }
        _ => expr,
    }
}

// ----------------------------------------------------------------------- Java

/// `System.out.println` folded to `print`, and literal-bearing `+` chains folded to
/// templates.
fn java(expr: Expr) -> Expr {
    if let Some(folded) = fold_concat(&expr) {
        return folded;
    }
    let Some((path, _)) = call_parts(&expr) else {
        return expr;
    };
    match path.as_slice() {
        ["System", "out", "println"] => {
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            print_call(args)
        }
        _ => expr,
    }
}

/// A `+` chain with a string literal in it, as a template.
///
/// `"n " + n` concatenates by converting in Java and TypeScript, and a template says
/// that portably: the writers spell it as an f-string, a template literal, `format!`,
/// or `Sprintf`. A chain with no literal stays as written, since `+` on two unknowns
/// may be arithmetic.
fn fold_concat(expr: &Expr) -> Option<Expr> {
    fn leaves<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
        match expr {
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => {
                leaves(left, out);
                leaves(right, out);
            }
            other => out.push(other),
        }
    }
    let Expr::Binary {
        op: BinaryOp::Add, ..
    } = expr
    else {
        return None;
    };
    let mut flat = Vec::new();
    leaves(expr, &mut flat);
    // A template in the chain is a string too: the fold runs bottom-up, so the inner
    // half of `"q " + q + " r " + r` is already a template when the outer `+` arrives.
    if !flat
        .iter()
        .any(|leaf| matches!(leaf, Expr::Str(_) | Expr::Template(_)))
    {
        return None;
    }
    let mut parts: Vec<TemplatePart> = Vec::new();
    for leaf in flat {
        match leaf {
            Expr::Str(text) => match parts.last_mut() {
                Some(TemplatePart::Text(existing)) => existing.push_str(text),
                _ => parts.push(TemplatePart::Text(text.clone())),
            },
            // A template inside the chain splices in, so nesting never stacks.
            Expr::Template(inner) => parts.extend(inner.iter().cloned()),
            other => parts.push(TemplatePart::Expr(other.clone())),
        }
    }
    Some(Expr::Template(parts))
}

// ------------------------------------------------------------------------ Zig

/// Zig's printing, folded to `print`, with the writer plumbing it runs on dropped.
///
/// A Zig program prints through a writer it has to build: a buffer, a writer over
/// stdout, an interface pointer, a flush. None of that is what the program *says*; it
/// is how Zig says it. The bindings that exist only to carry the writer are dropped,
/// the `print` calls through them become the canonical `print`, and `std.debug.print`
/// folds the same way.
fn zig_module(module: &mut Module) {
    for item in &mut module.items {
        match item {
            Item::Function(f) => zig_function(f),
            Item::Record(r) => {
                for method in &mut r.methods {
                    zig_function(method);
                }
            }
            _ => {}
        }
    }
}

fn zig_function(f: &mut Function) {
    // The writer aliases: bound to stdout machinery, or to a field of one.
    let mut writers: Vec<String> = Vec::new();
    // The names the writer machinery consumed: its buffer, the process `init`.
    let mut plumbing: Vec<String> = Vec::new();

    let mut kept = Vec::new();
    for stmt in f.body.drain(..) {
        match stmt {
            Stmt::Let { name, value, .. } if value.as_ref().is_some_and(mentions_stdout) => {
                collect_names(value.as_ref().unwrap(), &mut plumbing);
                writers.push(name);
            }
            Stmt::Let { name, value, .. }
                if value
                    .as_ref()
                    .is_some_and(|v| mentions_any(v, &writers)) =>
            {
                writers.push(name);
            }
            other => kept.push(other),
        }
    }
    // The buffer the writer consumed, wherever it was declared.
    f.body = kept
        .into_iter()
        .filter(|stmt| match stmt {
            Stmt::Let { name, .. } => !plumbing.contains(name),
            _ => true,
        })
        .collect();

    zig_statements(&mut f.body, &writers);

    // The entry point's `init: std.process.Init` is the runtime handing itself over,
    // and its error-union return is the same plumbing. The canonical main takes
    // nothing and returns nothing.
    if f.name == "main" && f.receiver.is_none() {
        f.params.retain(|p| {
            !matches!(&p.ty, Some(Type::Named { name, .. }) if name.contains("process.Init"))
        });
        f.returns = None;
        // The `return Ok(())` the reader synthesised for an error-union main has
        // nothing to say once main returns nothing.
        let unit_ok = |e: &Expr| {
            matches!(e, Expr::Call { callee, args }
                if matches!(&**callee, Expr::Name(n) if n == "Ok")
                    && matches!(args.as_slice(), [Expr::Tuple(items)] if items.is_empty()))
        };
        for stmt in &mut f.body {
            if let Stmt::Return(value) = stmt {
                if value.as_ref().is_some_and(unit_ok) {
                    *value = None;
                }
            }
        }
        while matches!(f.body.last(), Some(Stmt::Return(None))) {
            f.body.pop();
        }
    }
}

fn zig_statements(body: &mut Vec<Stmt>, writers: &[String]) {
    let mut kept = Vec::new();
    for mut stmt in body.drain(..) {
        // Recurse first, so prints inside branches and loops fold too.
        for inner in substatements(&mut stmt) {
            zig_statements(inner, writers);
        }
        match &mut stmt {
            Stmt::Expr(e) => {
                let plain = strip_propagate(e.clone());
                if let Some(folded) = zig_print(&plain, writers) {
                    kept.push(Stmt::Expr(folded));
                    continue;
                }
                if is_flush(&plain, writers) {
                    continue;
                }
                kept.push(stmt);
            }
            _ => kept.push(stmt),
        }
    }
    *body = kept;
}

/// The bodies nested directly under one statement.
fn substatements(stmt: &mut Stmt) -> Vec<&mut Vec<Stmt>> {
    match stmt {
        Stmt::If {
            then, otherwise, ..
        }
        | Stmt::IfPresent {
            then, otherwise, ..
        } => vec![then, otherwise],
        Stmt::While { body, .. }
        | Stmt::CountedFor { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachIndexed { body, .. }
        | Stmt::WhilePresent { body, .. }
        | Stmt::Defer(body)
        | Stmt::ErrDefer(body) => vec![body],
        Stmt::Switch { arms, default, .. } => {
            let mut all: Vec<&mut Vec<Stmt>> = arms.iter_mut().map(|(_, arm)| arm).collect();
            all.push(default);
            all
        }
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            let mut all = vec![body];
            all.extend(catches.iter_mut().map(|c| &mut c.body));
            all.push(finally);
            all
        }
        _ => Vec::new(),
    }
}

fn strip_propagate(e: Expr) -> Expr {
    match e {
        Expr::Propagate(inner) => *inner,
        other => other,
    }
}

/// `writer.print("n {d}\n", .{n})` or `std.debug.print(...)`, as the canonical print.
fn zig_print(e: &Expr, writers: &[String]) -> Option<Expr> {
    let Expr::Call { callee, args } = e else {
        return None;
    };
    let through_writer = matches!(&**callee, Expr::Field { of, name }
        if name == "print" && matches!(&**of, Expr::Name(n) if writers.contains(n)));
    let through_debug = matches!(call_path(callee).as_deref(), Some(["std", "debug", "print"]));
    if !through_writer && !through_debug {
        return None;
    }
    let (Some(Expr::Str(format)), values) = (args.first(), args.get(1)) else {
        return None;
    };
    let values: Vec<Expr> = match values {
        Some(Expr::Tuple(items)) => items.clone(),
        None => Vec::new(),
        _ => return None,
    };
    let parts = zig_format(format, &values)?;
    Some(print_call(vec![flatten_template(parts)]))
}

/// A Zig format string and its arguments, as template parts, minus the one trailing
/// newline `print` implies.
fn zig_format(format: &str, values: &[Expr]) -> Option<Vec<TemplatePart>> {
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut text = String::new();
    let mut holes = values.iter();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut spec = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        inside => spec.push(inside),
                    }
                }
                // Only the plain specs; a width or a base is formatting this cannot
                // promise every target reproduces.
                if !matches!(spec.as_str(), "" | "d" | "s" | "any") {
                    return None;
                }
                if !text.is_empty() {
                    parts.push(TemplatePart::Text(std::mem::take(&mut text)));
                }
                parts.push(TemplatePart::Expr(holes.next()?.clone()));
            }
            other => text.push(other),
        }
    }
    if holes.next().is_some() {
        return None;
    }
    if !text.is_empty() {
        parts.push(TemplatePart::Text(text));
    }
    if let Some(TemplatePart::Text(t)) = parts.last_mut() {
        if t.ends_with('\n') {
            t.pop();
            if t.is_empty() {
                parts.pop();
            }
        }
    }
    Some(parts)
}

fn is_flush(e: &Expr, writers: &[String]) -> bool {
    matches!(e, Expr::Call { callee, args }
        if args.is_empty()
            && matches!(&**callee, Expr::Field { of, name }
                if name == "flush" && matches!(&**of, Expr::Name(n) if writers.contains(n))))
}

/// The dotted path of a callee, if it is one.
fn call_path(callee: &Expr) -> Option<Vec<&str>> {
    let mut path = Vec::new();
    let mut at = callee;
    loop {
        match at {
            Expr::Name(name) => {
                path.push(name.as_str());
                break;
            }
            Expr::Field { of, name } => {
                path.push(name.as_str());
                at = of;
            }
            _ => return None,
        }
    }
    path.reverse();
    Some(path)
}

/// Does this expression reach stdout machinery anywhere inside it?
fn mentions_stdout(e: &Expr) -> bool {
    match e {
        Expr::Field { of, name } => name == "stdout" || mentions_stdout(of),
        Expr::Call { callee, args } => {
            mentions_stdout(callee) || args.iter().any(mentions_stdout)
        }
        Expr::Name(name) => name == "getStdOut",
        Expr::Unary { operand, .. } => mentions_stdout(operand),
        _ => false,
    }
}

/// Does this expression name any of `names`?
fn mentions_any(e: &Expr, names: &[String]) -> bool {
    match e {
        Expr::Name(n) => names.iter().any(|w| w == n),
        Expr::Field { of, .. } => mentions_any(of, names),
        Expr::Unary { operand, .. } => mentions_any(operand, names),
        Expr::Call { callee, args } => {
            mentions_any(callee, names) || args.iter().any(|a| mentions_any(a, names))
        }
        _ => false,
    }
}

/// Every name an expression mentions, for the plumbing set.
fn collect_names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Name(n) => out.push(n.clone()),
        Expr::Field { of, .. } => collect_names(of, out),
        Expr::Unary { operand, .. } => collect_names(operand, out),
        Expr::Call { callee, args } => {
            collect_names(callee, out);
            for a in args {
                collect_names(a, out);
            }
        }
        _ => {}
    }
}

// -------------------------------------------------------- the Result idiom

/// A function returning `Result<T, message>`, said the way the exception languages
/// say it: the return type is `T`, `Err` is a throw, `Ok` unwraps, and `?` re-throws.
///
/// The IR keeps one canonical form for failure, and it is the exception one: three of
/// the six writers spell it natively, and the other three lower it back. A module
/// that mixes idioms is its own worst reader.
fn settle_result_idiom(module: &mut Module) {
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            let Some(Type::Named { name, args }) = &f.returns else {
                continue;
            };
            if name != "Result" || args.len() != 2 {
                continue;
            }
            // Only a message-shaped failure side: a typed error enum is a real type
            // the target has to hear about, and stays as written.
            if !matches!(args[1], Type::String) && !error_name(&args[1]) {
                continue;
            }
            let ok = args[0].clone();
            f.returns = match ok {
                Type::Unit => None,
                other => Some(other),
            };
            settle_result_statements(&mut f.body);
        }
    }
}

/// Is this the `error`-flavoured type Zig's reader spells for an error set?
fn error_name(ty: &Type) -> bool {
    matches!(ty, Type::Named { name, args } if args.is_empty() && name.contains("error"))
}

fn settle_result_statements(body: &mut Vec<Stmt>) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            settle_result_statements(inner);
        }
        if let Stmt::Return(value) = stmt {
            match value.take() {
                Some(Expr::Call { callee, mut args })
                    if matches!(&*callee, Expr::Name(n) if n == "Ok") && args.len() == 1 =>
                {
                    let inner = args.remove(0);
                    *value = match inner {
                        Expr::Tuple(items) if items.is_empty() => None,
                        other => Some(other),
                    };
                }
                Some(Expr::Call { callee, mut args })
                    if matches!(&*callee, Expr::Name(n) if n == "Err") && args.len() == 1 =>
                {
                    // Zig's failure value is `error.negative`, and the name after
                    // the dot is the message every other language throws.
                    let payload = match args.remove(0) {
                        Expr::Field { of, name }
                            if matches!(&*of, Expr::Name(n) if n == "error") =>
                        {
                            Expr::Str(name)
                        }
                        other => unwrap_str_call(other),
                    };
                    *stmt = Stmt::Throw(payload);
                }
                other => *value = other,
            }
        }
    }
}

/// `str("negative")` is `"negative"`: the conversion of a literal is the literal.
fn unwrap_str_call(e: Expr) -> Expr {
    match e {
        Expr::Call { callee, mut args }
            if matches!(&*callee, Expr::Name(n) if n == "str") && args.len() == 1 =>
        {
            match args.remove(0) {
                literal @ (Expr::Str(_) | Expr::Template(_)) => literal,
                other => Expr::Call {
                    callee: Box::new(Expr::Name("str".into())),
                    args: vec![other],
                },
            }
        }
        other => other,
    }
}

// ------------------------------------------------- builtin exception classes

/// `ValueError("x")`, `new Error("x")`, `new RuntimeException("x")`: the class is the
/// language's furniture and the message is the meaning.
///
/// A throw of a builtin class around one message becomes a throw of the message, and a
/// catch filtered on a builtin class becomes the plain catch. A class this module
/// declares stays: that one is the program's own type, not furniture.
fn settle_exception_classes(module: &mut Module) {
    let declared: Vec<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();
    let builtin = |name: &str| {
        (name.ends_with("Error") || name.ends_with("Exception") || name == "Error")
            && !declared.iter().any(|d| d == name)
    };

    fn walk(body: &mut Vec<Stmt>, builtin: &dyn Fn(&str) -> bool) {
        for stmt in body.iter_mut() {
            for inner in substatements(stmt) {
                walk(inner, builtin);
            }
            match stmt {
                Stmt::Throw(value) => {
                    if let Expr::Call { callee, args } | Expr::New { callee, args } = value {
                        if let (Expr::Name(name), [message @ (Expr::Str(_) | Expr::Template(_))]) =
                            (&**callee, args.as_slice())
                        {
                            if builtin(name) {
                                *value = message.clone();
                            }
                        }
                    }
                }
                Stmt::Try { catches, .. } => {
                    for catch in catches {
                        if let Some(Type::Named { name, .. }) = &catch.ty {
                            if builtin(name) {
                                catch.ty = None;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    for item in &mut module.items {
        match item {
            Item::Function(f) => walk(&mut f.body, &builtin),
            Item::Record(r) => {
                for method in &mut r.methods {
                    walk(&mut method.body, &builtin);
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------- the Go error idiom

/// `(T, error)` returns and `if err != nil` checks, said as the canonical failure.
///
/// A function returning the pair returns `T` and throws. `return v, nil` is a plain
/// return, `return 0, errors.New("x")` is a throw, and the bind-then-check pair is
/// either a propagation or, when both branches carry on, a try/catch.
fn go_module(module: &mut Module) {
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            let pair = matches!(&f.returns, Some(Type::Tuple(parts))
                if parts.len() == 2
                    && matches!(&parts[1], Type::Named { name, args } if name == "error" && args.is_empty()));
            if pair {
                let Some(Type::Tuple(mut parts)) = f.returns.take() else {
                    unreachable!("the guard said so");
                };
                f.returns = match parts.remove(0) {
                    Type::Unit => None,
                    other => Some(other),
                };
                settle_go_returns(&mut f.body);
            }
            settle_go_checks(&mut f.body);
        }
    }
}

fn settle_go_returns(body: &mut Vec<Stmt>) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            settle_go_returns(inner);
        }
        let Stmt::Return(value) = stmt else { continue };
        let Some(Expr::Tuple(items)) = value else {
            continue;
        };
        if items.len() != 2 {
            continue;
        }
        let error = items.pop().expect("two items");
        let ok = items.pop().expect("one left");
        match error {
            Expr::Null => *value = Some(ok),
            other => {
                *stmt = Stmt::Throw(go_error_message(other));
            }
        }
    }
}

/// The message inside `errors.New("x")` or a one-verb `fmt.Errorf`; anything else
/// throws as itself.
fn go_error_message(e: Expr) -> Expr {
    if let Some((path, args)) = call_parts(&e) {
        if path.as_slice() == ["errors", "New"] && args.len() == 1 {
            return args[0].clone();
        }
        if path.as_slice() == ["fmt", "Errorf"] {
            if let Some(template) = sprintf_template(args) {
                return template;
            }
        }
    }
    e
}

fn settle_go_checks(body: &mut Vec<Stmt>) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            settle_go_checks(inner);
        }
    }
    let mut rebuilt: Vec<Stmt> = Vec::with_capacity(body.len());
    let mut stmts = std::mem::take(body).into_iter().peekable();
    while let Some(stmt) = stmts.next() {
        // The pair: `v, err := f(x)` and the `if err != nil` right under it.
        let Stmt::TupleAssign { names, value, .. } = &stmt else {
            rebuilt.push(stmt);
            continue;
        };
        let [bound, err] = names.as_slice() else {
            rebuilt.push(stmt);
            continue;
        };
        let err = err.clone();
        let checks = matches!(stmts.peek(), Some(Stmt::If { condition, .. })
            if matches!(condition, Expr::Binary { op: BinaryOp::Ne, left, right }
                if matches!(&**left, Expr::Name(n) if *n == err)
                    && matches!(&**right, Expr::Null)));
        if !checks {
            rebuilt.push(stmt);
            continue;
        }
        let (bound, call) = (bound.clone(), value.clone());
        let Some(Stmt::If {
            then, otherwise, ..
        }) = stmts.next()
        else {
            unreachable!("the peek said so");
        };
        let bind = |value| match bound.as_str() {
            "_" => Stmt::Expr(value),
            name => Stmt::Let {
                name: name.to_string(),
                ty: None,
                value: Some(value),
                mutable: false,
            },
        };
        // `if err != nil { return _, err }` alone is a propagation. The return pass
        // has already turned that return into a rethrow when the function's own
        // signature was the pair.
        let rethrows = otherwise.is_empty()
            && match then.as_slice() {
                [Stmt::Return(Some(Expr::Tuple(items)))] => {
                    matches!(items.last(), Some(Expr::Name(n)) if *n == err)
                }
                [Stmt::Throw(Expr::Name(n))] => *n == err,
                _ => false,
            };
        if rethrows {
            rebuilt.push(bind(Expr::Propagate(Box::new(call))));
            continue;
        }
        // Otherwise both branches carry on: the else is the success path and the
        // check is the catch.
        let mut caught = then;
        rewrite_error_reads(&mut caught, &err);
        let mut tried = vec![bind(call)];
        tried.extend(otherwise);
        rebuilt.push(Stmt::Try {
            body: tried,
            catches: vec![Catch {
                binding: Some(err.clone()),
                ty: None,
                body: caught,
            }],
            finally: Vec::new(),
            source: String::new(),
            line: 0,
        });
    }
    *body = rebuilt;
}

/// `err.Error()` inside a catch is the message read the canonical way: `str(err)`.
fn rewrite_error_reads(body: &mut Vec<Stmt>, err: &str) {
    fn in_expr(e: &mut Expr, err: &str) {
        let fix = |e: &mut Expr| in_expr(e, err);
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                fix(callee);
                for a in args {
                    fix(a);
                }
            }
            Expr::Binary { left, right, .. } => {
                fix(left);
                fix(right);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                fix(operand)
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        fix(e);
                    }
                }
            }
            Expr::Field { of, .. } | Expr::Index { of, .. } => fix(of),
            _ => {}
        }
        let is_read = matches!(e, Expr::Call { callee, args }
            if args.is_empty()
                && matches!(&**callee, Expr::Field { of, name }
                    if name == "Error" && matches!(&**of, Expr::Name(n) if n == err)));
        if is_read {
            *e = Expr::Call {
                callee: Box::new(Expr::Name("str".to_string())),
                args: vec![Expr::Name(err.to_string())],
            };
        }
    }
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            rewrite_error_reads(inner, err);
        }
        match stmt {
            Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => in_expr(e, err),
            Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => in_expr(e, err),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => in_expr(condition, err),
            _ => {}
        }
    }
}

// ---------------------------------------------------------- our own furniture

/// The helpers this tool's writers emit, folded back out on the way in.
///
/// `frShow`, `frPrint` and `frFormat` are lowerings, not program: a translated file
/// read back must yield the program alone, or a round trip grows a function the
/// source never had.
fn strip_lowering_helpers(module: &mut Module) {
    let ours = |name: &str| matches!(name, "frShow" | "frPrint" | "frFormat");
    // The definition may not even have parsed as a function: Zig's `comptime format`
    // parameter reads as no function at all, and the helper then sits carried. Either
    // way it is furniture, recognised by its name.
    module.items.retain(|item| match item {
        Item::Function(f) => !ours(&f.name),
        Item::Unsupported(u) => !["frShow", "frPrint", "frFormat"]
            .iter()
            .any(|name| u.source.trim_start().starts_with(&format!("fn {name}("))),
        _ => true,
    });
    fn fix(e: &mut Expr) {
        let walk = |e: &mut Expr| fix(e);
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                walk(callee);
                for a in args {
                    walk(a);
                }
            }
            Expr::Binary { left, right, .. } => {
                walk(left);
                walk(right);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                walk(operand)
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        walk(e);
                    }
                }
            }
            Expr::Field { of, .. } | Expr::Index { of, .. } => walk(of),
            _ => {}
        }
        // `frShow(x)` displays `x`; the value is `x` itself. `frPrint` is the
        // canonical print, and `frFormat` a template as a value.
        if let Expr::Call { callee, args } = e {
            match &**callee {
                Expr::Name(n) if n == "frShow" && args.len() == 1 => {
                    *e = args.remove(0);
                }
                Expr::Name(n) if n == "frPrint" || n == "frFormat" => {
                    let printing = n == "frPrint";
                    let values = match args.get(1) {
                        Some(Expr::Tuple(items)) => items.clone(),
                        None => Vec::new(),
                        _ => return,
                    };
                    let Some(Expr::Str(format)) = args.first() else {
                        return;
                    };
                    let Some(mut parts) = zig_format(format, &values) else {
                        return;
                    };
                    if !printing {
                        // The format helper implies no newline; `zig_format`
                        // trimmed one that print implies, so it goes back.
                        if let Some(TemplatePart::Text(t)) = parts.last_mut() {
                            if !format.ends_with('\n') {
                            } else {
                                t.push('\n');
                            }
                        }
                    }
                    let value = flatten_template(parts);
                    *e = match printing {
                        true => print_call(vec![value]),
                        false => value,
                    };
                }
                _ => {}
            }
        }
    }
    fn walk_stmts(body: &mut Vec<Stmt>) {
        for stmt in body.iter_mut() {
            for inner in substatements(stmt) {
                walk_stmts(inner);
            }
            match stmt {
                Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => fix(e),
                Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => fix(e),
                Stmt::If { condition, .. } | Stmt::While { condition, .. } => fix(condition),
                _ => {}
            }
        }
    }
    for item in &mut module.items {
        match item {
            Item::Function(f) => walk_stmts(&mut f.body),
            Item::Record(r) => {
                for method in &mut r.methods {
                    walk_stmts(&mut method.body);
                }
            }
            _ => {}
        }
    }
}
