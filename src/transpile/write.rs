//! Writing the IR out as a language.

use super::ir::*;
use crate::lang::Language;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

/// What kind of thing a name names, since the conventions differ by kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A struct, class, interface, `PascalCase` in every one of them.
    Type,
    /// A module-level constant, `SCREAMING_SNAKE` in most of them.
    Constant,
    /// A function or method.
    Function,
    /// A field, parameter or local.
    Value,
}

/// Does this expression call one of the module's failing functions anywhere?
mod lean;

fn contains_failing_call(out: &Out, e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } => {
            matches!(&**callee, Expr::Name(n) if out.throwing.contains(n.as_str()))
                || contains_failing_call(out, callee)
                || args.iter().any(|a| contains_failing_call(out, a))
        }
        Expr::Binary { left, right, .. } => {
            contains_failing_call(out, left) || contains_failing_call(out, right)
        }
        Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
            contains_failing_call(out, operand)
        }
        _ => false,
    }
}

/// Every name this body assigns to, indexes into, or grows through a method.
fn rust_mutated_names(body: &[Stmt]) -> std::collections::BTreeSet<String> {
    fn root(e: &Expr) -> Option<&str> {
        match e {
            Expr::Name(n) => Some(n),
            Expr::Field { of, .. } | Expr::Index { of, .. } => root(of),
            _ => None,
        }
    }
    fn in_expr(e: &Expr, found: &mut std::collections::BTreeSet<String>) {
        match e {
            Expr::Call { callee, args } => {
                if let Expr::Field { of, name } = &**callee {
                    if matches!(name.as_str(), "append" | "push" | "add" | "insert" | "pop") {
                        if let Some(n) = root(of) {
                            found.insert(n.to_string());
                        }
                    }
                }
                in_expr(callee, found);
                for a in args {
                    in_expr(a, found);
                }
            }
            Expr::Binary { left, right, .. } => {
                in_expr(left, found);
                in_expr(right, found);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                in_expr(operand, found)
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        in_expr(e, found);
                    }
                }
            }
            _ => {}
        }
    }
    fn walk(body: &[Stmt], found: &mut std::collections::BTreeSet<String>) {
        for stmt in body {
            match stmt {
                Stmt::Assign { target, value } => {
                    if let Some(n) = root(target) {
                        found.insert(n.to_string());
                    }
                    in_expr(value, found);
                }
                Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Throw(e) => in_expr(e, found),
                Stmt::Let { value: Some(e), .. } => in_expr(e, found),
                _ => {}
            }
            for inner in sub_bodies(stmt) {
                walk(inner, found);
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    walk(body, &mut found);
    found
}

/// Can any statement in this body leave the enclosing scope early?
fn exits_anywhere(body: &[Stmt]) -> bool {
    fn expr_fails(e: &Expr) -> bool {
        match e {
            Expr::Propagate(_) => true,
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                expr_fails(callee) || args.iter().any(expr_fails)
            }
            Expr::Binary { left, right, .. } => expr_fails(left) || expr_fails(right),
            Expr::Unary { operand, .. } | Expr::Await(operand) => expr_fails(operand),
            _ => false,
        }
    }
    body.iter().any(|stmt| {
        matches!(
            stmt,
            Stmt::Return(_) | Stmt::Throw(_) | Stmt::Break | Stmt::Continue
        ) || match stmt {
            Stmt::Expr(e) | Stmt::Return(Some(e)) => expr_fails(e),
            Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => expr_fails(e),
            _ => false,
        } || sub_bodies(stmt).into_iter().any(|b| exits_anywhere(b))
    })
}

/// Does any statement in this body return, at any depth?
fn returns_anywhere(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| {
        matches!(stmt, Stmt::Return(_)) || sub_bodies(stmt).into_iter().any(|b| returns_anywhere(b))
    })
}

/// A throwing function, restated in the Result idiom the Go and Zig writers speak.
fn with_failure_idiom(f: &Function, throwing: &std::collections::BTreeSet<String>) -> Function {
    let mut out = f.clone();
    let throws = f.receiver.is_none() && throwing.contains(&f.name);

    let mut counter = 0usize;
    extract_failing_calls(&mut out.body, throwing, &mut counter);

    if !throws {
        return out;
    }
    out.returns = Some(Type::Named {
        name: "Result".to_string(),
        args: vec![out.returns.take().unwrap_or(Type::Unit), Type::String],
    });
    fn wrap(body: &mut [Stmt]) {
        for stmt in body.iter_mut() {
            for inner in sub_bodies_mut(stmt) {
                wrap(inner);
            }
            match stmt {
                Stmt::Return(value) => {
                    let inner = match value.take() {
                        Some(v) => v,
                        None => Expr::Tuple(Vec::new()),
                    };
                    *value = Some(Expr::Call {
                        callee: Box::new(Expr::Name("Ok".to_string())),
                        args: vec![inner],
                    });
                }
                Stmt::Throw(e) => {
                    let payload = std::mem::replace(e, Expr::Null);
                    *stmt = Stmt::Return(Some(Expr::Call {
                        callee: Box::new(Expr::Name("Err".to_string())),
                        args: vec![payload],
                    }));
                }
                _ => {}
            }
        }
    }
    wrap(&mut out.body);
    if !matches!(out.body.last(), Some(Stmt::Return(_))) {
        out.body.push(Stmt::Return(Some(Expr::Call {
            callee: Box::new(Expr::Name("Ok".to_string())),
            args: vec![Expr::Tuple(Vec::new())],
        })));
    }
    out
}

/// Hoist every call to a failing function out of nested expressions.
fn extract_failing_calls(
    body: &mut Vec<Stmt>,
    throwing: &std::collections::BTreeSet<String>,
    counter: &mut usize,
) {
    fn hoist(
        e: &mut Expr,
        throwing: &std::collections::BTreeSet<String>,
        counter: &mut usize,
        lifted: &mut Vec<Stmt>,
        root_call: bool,
    ) {
        // Children first, so the innermost call binds first.
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                hoist(callee, throwing, counter, lifted, false);
                for a in args {
                    hoist(a, throwing, counter, lifted, false);
                }
            }
            Expr::Binary { left, right, .. } => {
                hoist(left, throwing, counter, lifted, false);
                hoist(right, throwing, counter, lifted, false);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) => {
                hoist(operand, throwing, counter, lifted, false);
            }
            Expr::Propagate(inner) => {
                // The propagated call is one unit; only its arguments can hide
                // further failing calls.
                if let Expr::Call { callee, args } | Expr::New { callee, args } = inner.as_mut() {
                    hoist(callee, throwing, counter, lifted, false);
                    for a in args {
                        hoist(a, throwing, counter, lifted, false);
                    }
                } else {
                    hoist(inner, throwing, counter, lifted, false);
                }
                let direct = matches!(inner.as_ref(), Expr::Call { .. } | Expr::New { .. });
                if !direct {
                    let unwrapped = std::mem::replace(inner.as_mut(), Expr::Null);
                    *e = unwrapped;
                    return;
                }
                // Already marked: the source spelled the propagation itself.
                if root_call {
                    return;
                }
                *counter += 1;
                let temp = format!("__fr_value{counter}");
                let inner = std::mem::replace(e, Expr::Name(temp.clone()));
                lifted.push(Stmt::Let {
                    name: temp,
                    ty: None,
                    value: Some(inner),
                    mutable: false,
                });
                return;
            }
            Expr::Template(parts) => {
                for part in parts.iter_mut() {
                    if let TemplatePart::Expr(e) = part {
                        hoist(e, throwing, counter, lifted, false);
                    }
                }
            }
            _ => {}
        }
        let failing = matches!(e, Expr::Call { callee, .. }
            if matches!(&**callee, Expr::Name(n) if throwing.contains(n.as_str())));
        if failing {
            let call = std::mem::replace(e, Expr::Null);
            if root_call {
                *e = Expr::Propagate(Box::new(call));
                return;
            }
            *counter += 1;
            let temp = format!("__fr_value{counter}");
            *e = Expr::Name(temp.clone());
            lifted.push(Stmt::Let {
                name: temp,
                ty: None,
                value: Some(Expr::Propagate(Box::new(call))),
                mutable: false,
            });
        }
    }

    let mut rebuilt = Vec::with_capacity(body.len());
    for mut stmt in body.drain(..) {
        for inner in sub_bodies_mut(&mut stmt) {
            extract_failing_calls(inner, throwing, counter);
        }
        let mut lifted = Vec::new();
        match &mut stmt {
            Stmt::Let { value: Some(v), .. } => hoist(v, throwing, counter, &mut lifted, true),
            Stmt::Expr(e) => hoist(e, throwing, counter, &mut lifted, true),
            Stmt::Return(Some(v)) => hoist(v, throwing, counter, &mut lifted, false),
            Stmt::Assign { value, .. } => hoist(value, throwing, counter, &mut lifted, false),
            Stmt::Throw(e) => hoist(e, throwing, counter, &mut lifted, false),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                hoist(condition, throwing, counter, &mut lifted, false)
            }
            _ => {}
        }
        rebuilt.extend(lifted);
        rebuilt.push(stmt);
    }
    *body = rebuilt;
}

/// The functions of this module that can fail, transitively.
fn throwing_functions(module: &Module) -> std::collections::BTreeSet<String> {
    fn direct_throw(body: &[Stmt]) -> bool {
        body.iter().any(|stmt| match stmt {
            Stmt::Throw(_) => true,
            Stmt::Expr(e) | Stmt::Return(Some(e)) => propagates(e),
            Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => propagates(e),
            // A throw inside a `try` is caught there, unless a catch rethrows.
            Stmt::Try { catches, .. } => catches.iter().any(|c| direct_throw(&c.body)),
            _ => false,
        }) || sub_walk(body, &direct_throw)
    }
    fn propagates(e: &Expr) -> bool {
        match e {
            Expr::Propagate(_) => true,
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                propagates(callee) || args.iter().any(propagates)
            }
            Expr::Binary { left, right, .. } => propagates(left) || propagates(right),
            Expr::Unary { operand, .. } | Expr::Await(operand) => propagates(operand),
            Expr::Template(parts) => parts.iter().any(|p| match p {
                TemplatePart::Expr(e) => propagates(e),
                TemplatePart::Text(_) => false,
            }),
            _ => false,
        }
    }
    fn sub_walk(body: &[Stmt], test: &dyn Fn(&[Stmt]) -> bool) -> bool {
        body.iter().any(|stmt| match stmt {
            // Count only what escapes the `try` body. The arms above cover the rest.
            Stmt::Try { .. } => false,
            other => sub_bodies(other).into_iter().any(|b| test(b)),
        })
    }
    fn calls_any(body: &[Stmt], set: &std::collections::BTreeSet<String>) -> bool {
        fn expr_calls(e: &Expr, set: &std::collections::BTreeSet<String>) -> bool {
            match e {
                Expr::Call { callee, args } => {
                    matches!(&**callee, Expr::Name(n) if set.contains(n.as_str()))
                        || expr_calls(callee, set)
                        || args.iter().any(|a| expr_calls(a, set))
                }
                Expr::New { callee, args } => {
                    expr_calls(callee, set) || args.iter().any(|a| expr_calls(a, set))
                }
                Expr::Binary { left, right, .. } => expr_calls(left, set) || expr_calls(right, set),
                Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                    expr_calls(operand, set)
                }
                Expr::Template(parts) => parts.iter().any(|p| match p {
                    TemplatePart::Expr(e) => expr_calls(e, set),
                    TemplatePart::Text(_) => false,
                }),
                _ => false,
            }
        }
        body.iter().any(|stmt| match stmt {
            Stmt::Try { catches, .. } => catches.iter().any(|c| calls_any(&c.body, set)),
            Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Throw(e) => expr_calls(e, set),
            Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => expr_calls(e, set),
            other => sub_bodies(other).into_iter().any(|b| calls_any(b, set)),
        })
    }

    let functions: Vec<(&String, &Vec<Stmt>)> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((&f.name, &f.body)),
            _ => None,
        })
        .collect();
    let mut set: std::collections::BTreeSet<String> = functions
        .iter()
        .filter(|(_, body)| direct_throw(body))
        .map(|(name, _)| (*name).clone())
        .collect();
    loop {
        let before = set.len();
        for (name, body) in &functions {
            if !set.contains(name.as_str()) && calls_any(body, &set) {
                set.insert((*name).clone());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

/// Rewrite every `return v` under these statements to store the value, raise
/// the flag, and leave the closure empty-handed.
fn route_returns_through_flag(body: &mut [Stmt], ret: &str, flag: &str) {
    for stmt in body.iter_mut() {
        if let Stmt::Return(value) = stmt {
            let mut routed = Vec::new();
            if let Some(value) = value.take() {
                routed.push(Stmt::Assign {
                    target: Expr::Name(ret.to_string()),
                    value,
                });
            }
            routed.push(Stmt::Assign {
                target: Expr::Name(flag.to_string()),
                value: Expr::Bool(true),
            });
            routed.push(Stmt::Return(None));
            *stmt = Stmt::Block(routed);
            continue;
        }
        for inner in sub_bodies_mut(stmt) {
            route_returns_through_flag(inner, ret, flag);
        }
    }
}

/// Rewrite every `return v` under these statements into the try-closure's success channel:
/// `return Ok(Some(v))`, spelled through a builtin the rust writer alone recognises.
fn route_returns_through_some(body: &mut [Stmt]) {
    for stmt in body.iter_mut() {
        if let Stmt::Return(value) = stmt {
            let args = value.take().map(|v| vec![v]).unwrap_or_default();
            *stmt = Stmt::Return(Some(Expr::Call {
                callee: Box::new(Expr::Name("__fr_ok_some".to_string())),
                args,
            }));
            continue;
        }
        for inner in sub_bodies_mut(stmt) {
            route_returns_through_some(inner);
        }
    }
}

/// Insert `disarm` before every `return` under these statements, nested loops included: a
/// `return` anywhere leaves the function.
fn disarm_before_returns(body: &mut Vec<Stmt>, disarm: &Stmt) {
    let mut at = 0;
    while at < body.len() {
        if matches!(body[at], Stmt::Return(_)) {
            body.insert(at, disarm.clone());
            at += 2;
            continue;
        }
        for inner in sub_bodies_mut(&mut body[at]) {
            disarm_before_returns(inner, disarm);
        }
        at += 1;
    }
}

/// A function with its nested-block bindings hoisted to the top.
fn with_hoisted_bindings(f: &Function, returns_of: &BTreeMap<String, Type>) -> Function {
    #[derive(Default)]
    struct Seen {
        count: usize,
        min_depth: usize,
        ty: Option<Type>,
        order: usize,
    }
    fn note(name: &str, ty: Option<Type>, depth: usize, seen: &mut Vec<(String, Seen)>) {
        match seen.iter_mut().find(|(n, _)| n == name) {
            Some((_, entry)) => {
                entry.count += 1;
                entry.min_depth = entry.min_depth.min(depth);
            }
            None => {
                let order = seen.len();
                seen.push((
                    name.to_string(),
                    Seen {
                        count: 1,
                        min_depth: depth,
                        ty,
                        order,
                    },
                ));
            }
        }
    }
    fn walk(body: &[Stmt], depth: usize, seen: &mut Vec<(String, Seen)>) {
        for stmt in body {
            match stmt {
                Stmt::Let {
                    name, ty, value, ..
                } => note(
                    name,
                    ty.clone().or_else(|| value.as_ref().and_then(value_type)),
                    depth,
                    seen,
                ),
                // A destructuring declares its names too; unhoisted they die at the brace of
                // whatever block a lowering wrapped around them.
                Stmt::TupleAssign {
                    names,
                    declares: true,
                    ..
                } => {
                    for name in names {
                        if name != "_" {
                            note(name, None, depth, seen);
                        }
                    }
                }
                _ => {}
            }
            for inner in sub_bodies(stmt) {
                walk(inner, depth + 1, seen);
            }
        }
    }
    fn value_type(_: &Expr) -> Option<Type> {
        None
    }
    fn rewrite(body: &mut [Stmt], hoisted: &[String]) {
        for stmt in body.iter_mut() {
            if let Stmt::Let { name, value, .. } = stmt {
                if hoisted.contains(name) {
                    let value = value.take().unwrap_or(Expr::Null);
                    *stmt = Stmt::Assign {
                        target: Expr::Name(name.clone()),
                        value,
                    };
                }
            }
            if let Stmt::TupleAssign {
                names, declares, ..
            } = stmt
            {
                if *declares && names.iter().any(|n| hoisted.contains(n)) {
                    *declares = false;
                }
            }
            for inner in sub_bodies_mut(stmt) {
                rewrite(inner, hoisted);
            }
        }
    }

    let mut seen: Vec<(String, Seen)> = Vec::new();
    walk(&f.body, 0, &mut seen);
    let mut hoist: Vec<(String, Seen)> = seen
        .into_iter()
        .filter(|(_, entry)| entry.count > 1 || entry.min_depth > 0)
        .collect();
    if hoist.is_empty() {
        return f.clone();
    }
    hoist.sort_by_key(|(_, entry)| entry.order);
    let names: Vec<String> = hoist.iter().map(|(n, _)| n.clone()).collect();

    let mut out = f.clone();
    rewrite(&mut out.body, &names);
    // The declared type, or the return type of the call first assigned to it: the block-scoped
    // targets have to write one.
    let first_type = |name: &str| -> Option<Type> {
        fn first_value<'a>(body: &'a [Stmt], name: &str) -> Option<&'a Expr> {
            for stmt in body {
                if let Stmt::Assign {
                    target: Expr::Name(n),
                    value,
                } = stmt
                {
                    if n == name {
                        return Some(value);
                    }
                }
                for inner in sub_bodies(stmt) {
                    if let Some(found) = first_value(inner, name) {
                        return Some(found);
                    }
                }
            }
            None
        }
        match first_value(&out.body, name)? {
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Name(f) => returns_of.get(f.as_str()).cloned(),
                _ => None,
            },
            Expr::Int(_) => Some(Type::Int),
            Expr::Float(_) => Some(Type::Float),
            Expr::Str(_) | Expr::Template(_) => Some(Type::String),
            Expr::Bool(_) => Some(Type::Bool),
            _ => None,
        }
    };
    let declarations: Vec<Stmt> = hoist
        .iter()
        .map(|(name, entry)| Stmt::Let {
            name: name.clone(),
            ty: entry.ty.clone().or_else(|| first_type(name)),
            value: None,
            mutable: true,
        })
        .collect();
    out.body.splice(0..0, declarations);
    out
}

/// The statement bodies nested under one statement.
pub(super) fn sub_bodies(stmt: &Stmt) -> Vec<&Vec<Stmt>> {
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
        | Stmt::ErrDefer(body)
        | Stmt::Block(body) => vec![body],
        Stmt::Switch { arms, default, .. } => {
            let mut all: Vec<&Vec<Stmt>> = arms.iter().map(|(_, arm)| arm).collect();
            all.push(default);
            all
        }
        Stmt::MatchVariants { arms, default, .. } => {
            let mut all: Vec<&Vec<Stmt>> = arms.iter().map(|arm| &arm.body).collect();
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
            all.extend(catches.iter().map(|c| &c.body));
            all.push(finally);
            all
        }
        _ => Vec::new(),
    }
}

pub(super) fn sub_bodies_mut(stmt: &mut Stmt) -> Vec<&mut Vec<Stmt>> {
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
        | Stmt::ErrDefer(body)
        | Stmt::Block(body) => vec![body],
        Stmt::Switch { arms, default, .. } => {
            let mut all: Vec<&mut Vec<Stmt>> = arms.iter_mut().map(|(_, arm)| arm).collect();
            all.push(default);
            all
        }
        Stmt::MatchVariants { arms, default, .. } => {
            let mut all: Vec<&mut Vec<Stmt>> = arms.iter_mut().map(|arm| &mut arm.body).collect();
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

/// Spell every name this module declares the target language's way.
type Spellings = (BTreeMap<String, String>, BTreeMap<String, String>);

/// Whether a value is one scalar literal, `-80.0` included.
fn scalar_literal(value: &Expr) -> bool {
    match value {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => true,
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
        } => scalar_literal(operand),
        _ => false,
    }
}

fn spellings(language: Language, module: &Module) -> Spellings {
    fn spell(language: Language, name: &str, kind: Kind, exported: bool) -> String {
        match kind {
            Kind::Type => pascal(name),
            Kind::Constant => match language {
                Language::Go => go_name(name, exported),
                // Zig does not shout.
                Language::Zig => snake_always(name),
                // Neither does Lean, where a constant is a `def` like any other.
                Language::Lean => camel(name),
                _ => screaming(name),
            },
            Kind::Function => match language {
                Language::Rust => snake_always(name),
                // Python says "not for outside this module" with a leading underscore.
                Language::Python => match exported || name.starts_with('_') || name == "main" {
                    true => snake_always(name),
                    false => format!("_{}", snake_always(name)),
                },
                Language::Go => go_name(name, exported),
                _ => camel(name),
            },
            Kind::Value => match language {
                Language::Rust | Language::Python | Language::Zig => snake_always(name),
                Language::Go => go_name(name, exported),
                _ => camel(name),
            },
        }
    }

    let mut map = BTreeMap::new();
    let mut fields = BTreeMap::new();
    let into = |map: &mut BTreeMap<String, String>, name: &str, kind: Kind, exported: bool| {
        // `_` is the word for "no name".
        if name.is_empty() || name.chars().all(|c| c == '_') {
            return;
        }
        let spelled = spell(language, name, kind, exported);
        // A rename that produces nothing is not a rename.
        if spelled.is_empty() || spelled == name {
            return;
        }
        map.insert(name.to_string(), spelled);
    };
    let mut add = |name: &str, kind: Kind, exported: bool| into(&mut map, name, kind, exported);

    fn walk_stmts(stmts: &[Stmt], add: &mut impl FnMut(&str, Kind, bool)) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, .. } => add(name, Kind::Value, false),
                Stmt::TupleAssign { names, .. } => {
                    for name in names {
                        add(name, Kind::Value, false);
                    }
                }
                Stmt::ForEach { binding, body, .. } => {
                    add(binding, Kind::Value, false);
                    walk_stmts(body, add);
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    walk_stmts(then, add);
                    walk_stmts(otherwise, add);
                }
                Stmt::IfPresent {
                    binding,
                    then,
                    otherwise,
                    ..
                } => {
                    add(binding, Kind::Value, false);
                    walk_stmts(then, add);
                    walk_stmts(otherwise, add);
                }
                Stmt::WhilePresent { binding, body, .. } => {
                    add(binding, Kind::Value, false);
                    walk_stmts(body, add);
                }
                Stmt::Switch { arms, default, .. } => {
                    for (_, body) in arms {
                        walk_stmts(body, add);
                    }
                    walk_stmts(default, add);
                }
                Stmt::Defer(cleanup) | Stmt::ErrDefer(cleanup) | Stmt::Block(cleanup) => {
                    walk_stmts(cleanup, add)
                }
                Stmt::ForEachIndexed {
                    index,
                    binding,
                    body,
                    ..
                } => {
                    add(index, Kind::Value, false);
                    add(binding, Kind::Value, false);
                    walk_stmts(body, add);
                }
                Stmt::While { body, .. } => walk_stmts(body, add),
                // A counted `for` declares its counter in the header.
                Stmt::CountedFor {
                    init, update, body, ..
                } => {
                    for header in [init, update].iter().copied().flatten() {
                        walk_stmts(std::slice::from_ref(header), add);
                    }
                    walk_stmts(body, add);
                }
                _ => {}
            }
        }
    }

    fn walk_function(f: &Function, add: &mut impl FnMut(&str, Kind, bool)) {
        // Claim no spelling: every target picks its own word for a constructor.
        if !f.is_constructor {
            add(&f.name, Kind::Function, f.exported);
        }
        for param in &f.params {
            add(&param.name, Kind::Value, false);
        }
        walk_stmts(&f.body, add);
    }

    for item in &module.items {
        match item {
            Item::Function(f) => walk_function(f, &mut add),
            Item::Statement(stmt) => walk_stmts(std::slice::from_ref(stmt), &mut add),
            Item::Newtype(n) => add(&n.name, Kind::Type, n.exported),
            Item::Record(r) => {
                add(&r.name, Kind::Type, r.exported);
                for field in &r.fields {
                    into(&mut fields, &field.name, Kind::Value, field.exported);
                }
                for method in &r.methods {
                    walk_function(method, &mut add);
                }
            }
            // A `const` bound to a literal is a constant and takes the `SCREAMING_SNAKE`
            // convention.
            Item::Constant(c) => {
                // A scalar takes the target's constant convention.
                let screaming = c.name.chars().any(|ch| ch.is_ascii_uppercase())
                    && !c.name.chars().any(|ch| ch.is_ascii_lowercase());
                let kind = match &c.value {
                    value if scalar_literal(value) => Some(Kind::Constant),
                    _ if screaming => None,
                    _ => Some(Kind::Value),
                };
                if let Some(kind) = kind {
                    add(&c.name, kind, c.exported);
                }
            }
            Item::Sum(s) => {
                add(&s.name, Kind::Type, s.exported);
                for variant in &s.variants {
                    add(&variant.name, Kind::Type, s.exported);
                    for field in &variant.fields {
                        into(&mut fields, &field.name, Kind::Value, field.exported);
                    }
                }
            }
            Item::Test { body, .. } => walk_stmts(body, &mut add),
            Item::Import { .. } | Item::Unsupported(_) => {}
        }
    }
    // Go's entry point is the one name whose spelling is load-bearing.
    if language == Language::Go
        && module.items.iter().any(
            |item| matches!(item, Item::Function(f) if f.name == "main" && f.params.is_empty()),
        )
    {
        map.remove("main");
    }
    (map, fields)
}

/// This parameter's text here, and whether the calling convention survived.
fn spell_param(out: &Out, kind: ParamKind, raw: &str, changed: &mut bool) -> Option<String> {
    // A bare `*` or `/` is punctuation standing where a parameter would go.
    if kind == ParamKind::Marker {
        if out.language == Language::Python {
            return Some(raw.to_string());
        }
        *changed = true;
        return None;
    }
    let name = out.name(raw);
    let language = out.language;
    match (kind, language) {
        (ParamKind::Normal, _) => Some(name.to_string()),
        (ParamKind::Marker, _) => None,
        (ParamKind::VarArgs, Language::Python) => Some(format!("*{name}")),
        (ParamKind::VarArgs, Language::TypeScript | Language::Tsx) => Some(format!("...{name}")),
        (ParamKind::VarArgs, Language::Go) => Some(format!("{name} ...")),
        (ParamKind::VarArgs, _) => {
            *changed = true;
            Some(name.to_string())
        }
        (ParamKind::KeywordArgs, Language::Python) => Some(format!("**{name}")),
        (ParamKind::KeywordArgs, _) => {
            *changed = true;
            Some(name.to_string())
        }
    }
}

/// Does the target reserve this word?
fn reserved(language: Language, name: &str) -> bool {
    const RUST: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "box", "final", "macro", "override", "priv",
        "try", "typeof", "unsized", "virtual", "yield",
    ];
    const GO: &[&str] = &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ];
    const PYTHON: &[&str] = &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "None", "True", "False",
    ];
    const TYPESCRIPT: &[&str] = &[
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "null",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
    ];
    const JAVA: &[&str] = &[
        "abstract",
        "assert",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "int",
        "interface",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "strictfp",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "void",
        "volatile",
        "while",
        "true",
        "false",
        "null",
    ];
    const ZIG: &[&str] = &[
        "addrspace",
        "align",
        "allowzero",
        "and",
        "anyframe",
        "anytype",
        "asm",
        "async",
        "await",
        "break",
        "callconv",
        "catch",
        "comptime",
        "const",
        "continue",
        "defer",
        "else",
        "enum",
        "errdefer",
        "error",
        "export",
        "extern",
        "fn",
        "for",
        "if",
        "inline",
        "linksection",
        "noalias",
        "noinline",
        "nosuspend",
        "opaque",
        "or",
        "orelse",
        "packed",
        "pub",
        "resume",
        "return",
        "struct",
        "suspend",
        "switch",
        "test",
        "threadlocal",
        "try",
        "union",
        "unreachable",
        "usingnamespace",
        "var",
        "volatile",
        "while",
    ];
    let list = match language {
        Language::Rust => RUST,
        Language::Go => GO,
        Language::Java => JAVA,
        Language::Python => PYTHON,
        Language::Zig => ZIG,
        Language::TypeScript | Language::Tsx => TYPESCRIPT,
        Language::Lean => lean::RESERVED,
        _ => return false,
    };
    list.contains(&name)
}

/// What this language calls the receiver inside a method body.
fn receiver_word(language: Language) -> &'static str {
    match language {
        Language::Java | Language::TypeScript | Language::Tsx => "this",
        // Go's convention is a one- or two-letter abbreviation of the type.
        _ => "self",
    }
}

/// The marker that heads every carried-over fragment.
pub const MARKER: &str = "fun-refactor: not translated";

/// The sibling this import line resolved to inside a directory sweep, if any.
fn sibling_import(target: &Option<ImportTarget>) -> Option<(&str, &[ImportedName])> {
    let target = target.as_ref()?;
    let stem = target.resolved.as_deref()?;
    if target.names.is_empty() {
        return None;
    }
    Some((stem, &target.names))
}

pub fn write(language: Language, module: &Module) -> Result<(String, Fidelity)> {
    write_in_context(language, module, module)
}

/// Write `module`, spelling names as declared by `context`.
pub fn write_in_context(
    language: Language,
    module: &Module,
    context: &Module,
) -> Result<(String, Fidelity)> {
    let mut out = Out::new(language);
    // What the sweep had to change about this file travels with it, so the
    // header says it rather than the reader discovering a renamed type.
    out.fidelity
        .notes
        .extend(module.sweep_notes.iter().cloned());
    let (names, fields) = spellings(language, context);
    out.names = names;
    out.fields = fields;
    out.declared_types = context
        .items
        .iter()
        .flat_map(|item| match item {
            Item::Record(r) => vec![r.name.clone()],
            Item::Newtype(n) => vec![n.name.clone()],
            Item::Sum(s) => std::iter::once(s.name.clone())
                .chain(s.variants.iter().map(|v| v.name.clone()))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    out.newtypes = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Newtype(n) => Some((n.name.clone(), n.base.clone())),
            _ => None,
        })
        .collect();
    out.records = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some((
                r.name.clone(),
                r.fields.iter().map(|f| f.name.clone()).collect(),
            )),
            _ => None,
        })
        .collect();
    // A literal may name a subset of the fields, and the rest take the values the record
    // declares.
    out.record_field_defaults = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some((
                r.name.clone(),
                r.fields
                    .iter()
                    .filter_map(|f| f.default.clone().map(|d| (f.name.clone(), d)))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    out.record_field_types = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some((
                r.name.clone(),
                r.fields
                    .iter()
                    .filter_map(|f| f.ty.clone().map(|ty| (f.name.clone(), ty)))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    // A name that is a property somewhere and never a field is safe to rewrite.
    let mut properties: std::collections::BTreeSet<String> = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(
                r.methods
                    .iter()
                    .filter(|m| m.is_property)
                    .map(|m| m.name.clone()),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    for fields in out.records.values() {
        for field in fields {
            properties.remove(field);
        }
    }
    out.properties = properties;
    out.methods = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(
                r.methods
                    .iter()
                    .filter(|m| !m.is_constructor)
                    .map(|m| m.name.clone()),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    out.functions = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((
                f.name.clone(),
                f.params
                    .iter()
                    .filter(|p| p.kind == ParamKind::Normal)
                    .map(|p| (p.name.clone(), p.default.clone()))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    out.throwing = throwing_functions(context);
    // A record's methods declare returns as much as a loose function does.
    out.function_returns = context
        .items
        .iter()
        .flat_map(|item| match item {
            Item::Function(f) => f
                .returns
                .clone()
                .map(|t| (f.name.clone(), t))
                .into_iter()
                .collect::<Vec<_>>(),
            Item::Record(r) => r
                .methods
                .iter()
                .filter_map(|m| m.returns.clone().map(|t| (m.name.clone(), t)))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    out.function_param_types = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((
                f.name.clone(),
                f.params
                    .iter()
                    .filter(|p| p.kind == ParamKind::Normal)
                    .map(|p| p.ty.clone())
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    out.result_returns = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => {
                result_ok(&out.declared_types, f.returns.as_ref()).map(|ok| (f.name.clone(), ok))
            }
            _ => None,
        })
        .collect();
    out.sums = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect();
    out.sum_items = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some((s.name.clone(), s.clone())),
            _ => None,
        })
        .collect();
    for item in &context.items {
        let Item::Sum(sum) = item else { continue };
        let spellings = hoisted_variant_names(&mut out, context, sum);
        for (variant, spelled) in sum.variants.iter().zip(spellings) {
            out.variant_spellings
                .insert((sum.name.clone(), variant.name.clone()), spelled);
        }
    }
    // A target without inheritance can still hold what a base in the same module contributed.
    let flattened = match language {
        Language::Rust | Language::Go | Language::Zig => {
            let (module, notes) = flatten_local_bases(module);
            out.fidelity.notes.extend(notes);
            Some(module)
        }
        _ => None,
    };
    let module = flattened.as_ref().unwrap_or(module);
    // Go and Zig have no expression that builds a collection.
    let looped = match language {
        Language::Go | Language::Zig => Some(loops_for_comprehensions(module)),
        _ => None,
    };
    let module = looped.as_ref().unwrap_or(module);
    // Zig has no closure: a function value is a function, declared at the top of the file.
    let lifted = match language {
        Language::Zig => Some(functions_for_lambdas(module)),
        _ => None,
    };
    let module = lifted.as_ref().unwrap_or(module);
    // Rust and Java will not read a whole number as a fractional one.
    let spelled = match language {
        Language::Rust | Language::Java => Some(numbers_as_declared(module)),
        _ => None,
    };
    let module = spelled.as_ref().unwrap_or(module);

    match language {
        Language::Rust => rust(&mut out, module),
        Language::Python => python(&mut out, module),
        Language::Go => go(&mut out, module),
        Language::Java => java(&mut out, module),
        Language::Zig => zig(&mut out, module),
        Language::TypeScript | Language::Tsx => typescript(&mut out, module),
        Language::Bash => bash(&mut out, module),
        Language::Lean => lean::write(&mut out, module),
        other => bail!(
            "there is no writer for {other}: it has no functions or records to write \
             these into"
        ),
    }
    let unnameable: Vec<String> = out.unnameable.borrow().iter().cloned().collect();
    for name in unnameable {
        out.fidelity.notes.push(format!(
            "{language} cannot spell `{name}`, so this writes `{}`, and every \
             use of it needs a real name",
            sanitise(&name)
        ));
    }
    let escaped: Vec<String> = out.escaped.borrow().iter().cloned().collect();
    for name in escaped {
        out.fidelity.notes.push(format!(
            "`{name}` is a keyword in {language} and cannot be an identifier there; this \
             adds a suffix, and every use of it needs a real name"
        ));
    }
    Ok((out.finish(), out.fidelity))
}

struct Out {
    language: Language,
    text: String,
    indent: usize,
    fidelity: Fidelity,
    /// Spell this module's own names the target language's way.
    names: BTreeMap<String, String>,
    /// Names that are not identifiers at all in any of these languages.
    unnameable: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// Names that had to be escaped because the target reserves them.
    escaped: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// The types this module declares.
    declared_types: std::collections::BTreeSet<String>,
    /// Packages the Go this writer produced needs to import.
    go_imports: std::collections::BTreeSet<&'static str>,
    /// The lowering helpers the Zig this writer produced turned out to need.
    zig_helpers: std::collections::BTreeSet<&'static str>,
    /// Parameters this Java method takes as a functional interface.
    functional_params: std::collections::BTreeSet<String>,
    /// Inside a record's `impl`, the type parameters the struct already declares, by the field
    /// each stands for.
    record_generics: Vec<(String, String)>,
    /// The record whose `impl` this writer sits in, so a method returning it says `Self`.
    record_written: Option<String>,
    /// Bindings whose value the Zig writer knows to be text, by watching the `let`s it writes.
    zig_strings: std::collections::BTreeSet<String>,
    /// The distinct types this module declares, name to base.
    newtypes: std::collections::BTreeMap<String, Type>,
    /// Each declared record's field names, in order.
    records: std::collections::BTreeMap<String, Vec<String>>,
    /// The value each record field starts at, where the record declares one.
    record_field_defaults: std::collections::BTreeMap<String, Vec<(String, Expr)>>,
    /// The module's own choices, whole, for building a variant of one.
    sum_items: std::collections::BTreeMap<String, Sum>,
    /// Each hoisted variant's name in the output, keyed by (sum, variant).
    variant_spellings: std::collections::BTreeMap<(String, String), String>,
    /// Method names the module reads as data: `@property`, a TypeScript getter.
    properties: std::collections::BTreeSet<String>,
    /// The names of the methods the module's records declare.
    methods: std::collections::BTreeSet<String>,
    /// Each declared function's parameters, in order, with their defaults.
    functions: std::collections::BTreeMap<String, Vec<(String, Option<Expr>)>>,
    /// The declared parameter types of this module's functions, by name.
    function_param_types: std::collections::BTreeMap<String, Vec<Option<Type>>>,
    /// The declared return types, for the format spec a call's value takes.
    function_returns: std::collections::BTreeMap<String, Type>,
    /// This module's functions that can fail: a throw in the body, or a call to one that can,
    /// transitively.
    throwing: std::collections::BTreeSet<String>,
    /// May this statement propagate a failure outward?
    can_propagate: bool,
    /// Does this function return `Result`, so its returns wrap `Ok`?
    fn_throws: bool,
    /// What this function answers, for targets that convert number types only when told.
    fn_returns: Option<Type>,
    /// Sits inside a loop's own header.
    in_loop_header: bool,
    /// One counter for the names a lowering has to invent.
    lowering_names: usize,
    /// The names the Rust body writes to or grows, which must bind `mut` whatever the source's
    /// own mutability said.
    rust_mutated: std::collections::BTreeSet<String>,
    /// The `try` block the Zig writer is inside: its label, the catch binding, and
    /// the catch body every failing call repeats before breaking out.
    zig_try: Option<(String, String, Vec<Stmt>)>,
    /// This Zig function's growable lists: `std.ArrayList`, reached through `.items`.
    zig_dyn: std::collections::BTreeSet<String>,
    /// Text a writer could not put where the expression it replaced stood.
    pending: Vec<String>,
    /// The types of this body's names, as the source declared them.
    binding_types: std::collections::BTreeMap<String, Type>,
    /// What this record declares its fields to be.
    field_types: std::collections::BTreeMap<String, Type>,
    /// The same, for every record in view, keyed by the record's name.
    record_field_types:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, Type>>,
    /// The same, for record fields, which are a separate namespace.
    fields: BTreeMap<String, String>,
    /// The fields this method body may name without a receiver.
    receiver_fields: BTreeMap<String, String>,
    /// The ok type of the `Result` this Go function returns, where it returns one.
    go_result: Option<Type>,
    /// Sits inside a `func TestX(t *testing.T)`.
    in_test: bool,
    /// How many error bindings this Go body has introduced.
    go_errors: usize,
    /// Each declared function's `Result` ok type, where its return is one.
    result_returns: BTreeMap<String, Type>,
    /// The sums this module declares.
    sums: std::collections::BTreeSet<String>,
    /// The bindings of the catch clauses in view, innermost last.
    catch_bindings: Vec<String>,
    /// The exception classes this TypeScript module throws or catches and the language does not
    /// have.
    ts_exceptions: std::collections::BTreeSet<&'static str>,
    /// Did this Rust body ask for floor division?
    needs_floor_div: bool,
    /// The functions whose Lean answers in `IO`, because they reach a runtime.
    lean_io: std::collections::BTreeSet<String>,
    /// The functions whose Lean is `partial`, because Lean cannot see them terminate.
    lean_partial: std::collections::BTreeSet<String>,
    /// The names the Lean body under the writer assigns to: its `let mut` ones.
    lean_mut: std::collections::BTreeSet<String>,
    /// Whether the Lean function under the writer answers in `IO`.
    lean_in_io: bool,
    /// The records whose module writes a constructor for them, so that a call naming one
    /// reaches the constructor and not the plain construction.
    lean_constructed: std::collections::BTreeSet<String>,
    /// The definitions the Lean this writer produced turned out to need.
    lean_helpers: std::collections::BTreeSet<&'static str>,
}

impl Out {
    fn new(language: Language) -> Self {
        Out {
            language,
            text: String::new(),
            indent: 0,
            fidelity: Fidelity::default(),
            names: BTreeMap::new(),
            unnameable: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            escaped: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            declared_types: std::collections::BTreeSet::new(),
            go_imports: std::collections::BTreeSet::new(),
            zig_helpers: std::collections::BTreeSet::new(),
            functional_params: std::collections::BTreeSet::new(),
            record_generics: Vec::new(),
            record_written: None,
            zig_strings: std::collections::BTreeSet::new(),
            newtypes: std::collections::BTreeMap::new(),
            records: std::collections::BTreeMap::new(),
            record_field_defaults: std::collections::BTreeMap::new(),
            properties: std::collections::BTreeSet::new(),
            sum_items: std::collections::BTreeMap::new(),
            variant_spellings: std::collections::BTreeMap::new(),
            methods: std::collections::BTreeSet::new(),
            functions: std::collections::BTreeMap::new(),
            function_param_types: std::collections::BTreeMap::new(),
            function_returns: std::collections::BTreeMap::new(),
            throwing: std::collections::BTreeSet::new(),
            can_propagate: false,
            fn_throws: false,
            fn_returns: None,
            in_loop_header: false,
            lowering_names: 0,
            rust_mutated: std::collections::BTreeSet::new(),
            zig_try: None,
            zig_dyn: std::collections::BTreeSet::new(),
            pending: Vec::new(),
            binding_types: std::collections::BTreeMap::new(),
            field_types: std::collections::BTreeMap::new(),
            record_field_types: std::collections::BTreeMap::new(),
            fields: BTreeMap::new(),
            receiver_fields: BTreeMap::new(),
            go_result: None,
            in_test: false,
            go_errors: 0,
            result_returns: BTreeMap::new(),
            sums: std::collections::BTreeSet::new(),
            catch_bindings: Vec::new(),
            ts_exceptions: std::collections::BTreeSet::new(),
            needs_floor_div: false,
            lean_io: std::collections::BTreeSet::new(),
            lean_partial: std::collections::BTreeSet::new(),
            lean_mut: std::collections::BTreeSet::new(),
            lean_in_io: false,
            lean_constructed: std::collections::BTreeSet::new(),
            lean_helpers: std::collections::BTreeSet::new(),
        }
    }

    /// A name for the next error binding this Go body introduces.
    fn fresh_go_error(&mut self) -> String {
        self.go_errors += 1;
        match self.go_errors {
            1 => "err".to_string(),
            n => format!("err{n}"),
        }
    }

    /// This name in the target's convention, or unchanged if it is not ours.
    fn name(&self, raw: &str) -> String {
        let spelled = self
            .names
            .get(raw)
            .cloned()
            .unwrap_or_else(|| raw.to_string());
        self.legal(spelled)
    }

    /// The same name, made writable where the target will not take it as written.
    fn legal(&self, spelled: String) -> String {
        // Only TypeScript names a member by an expression, as in `[Symbol.dispose]()`.
        if !is_writable_identifier(&spelled) {
            self.unnameable.borrow_mut().insert(spelled.clone());
            return sanitise(&spelled);
        }
        if !reserved(self.language, &spelled) {
            return spelled;
        }
        // The receiver's own word is never an escape problem: it reaches here only because a
        // method body used it.
        if spelled == receiver_word(self.language) {
            return spelled;
        }
        self.escaped.borrow_mut().insert(spelled.clone());
        match self.language {
            // Rust and Zig both have a spelling for this, and under it the name stays the same
            // identifier instead of becoming a different one.
            Language::Rust => match spelled.as_str() {
                "crate" | "super" | "Self" => format!("{spelled}_"),
                _ => format!("r#{spelled}"),
            },
            Language::Zig => format!("@\"{spelled}\""),
            _ => format!("{spelled}_"),
        }
    }

    /// The name to write for this function: its own, or the target's word for a constructor.
    fn function_name(&self, f: &Function) -> String {
        match (f.is_constructor, f.receiver.as_deref()) {
            (true, Some(owner)) => self.legal(constructor_name(self.language, owner)),
            _ => self.name(&f.name),
        }
    }

    /// This field name in the target's convention, or unchanged if it is not ours.
    fn is_foreign(&self, ty: &Type) -> bool {
        match ty {
            Type::Named { name, .. } => !self.declared_types.contains(name),
            Type::List(inner) | Type::Optional(inner) => self.is_foreign(inner),
            Type::Map(k, v) => self.is_foreign(k) || self.is_foreign(v),
            _ => false,
        }
    }

    fn field(&self, raw: &str) -> String {
        let spelled = match self.fields.get(raw) {
            Some(spelled) => spelled.clone(),
            // A caller reaches a method like a field and spells it like a function.
            None if self.methods.contains(raw) => self
                .names
                .get(raw)
                .cloned()
                .unwrap_or_else(|| raw.to_string()),
            None => raw.to_string(),
        };
        self.legal(spelled)
    }

    /// Spell the receiver this target's way inside a method body.
    fn bind_receiver(&mut self, bound: &str) -> Option<String> {
        let word = receiver_word(self.language).to_string();
        self.names.insert(bound.to_string(), word)
    }

    fn unbind_receiver(&mut self, bound: &str, previous: Option<String>) {
        match previous {
            Some(name) => self.names.insert(bound.to_string(), name),
            None => self.names.remove(bound),
        };
    }

    /// Start writing a method body: bind the receiver and the fields it reaches bare.
    fn enter_method(&mut self, f: &Function) -> MethodScope {
        let bound = f.receiver_binding.clone();
        let displaced_name = bound.as_deref().map(|b| self.bind_receiver(b));
        let displaced_fields = std::mem::take(&mut self.receiver_fields);
        let displaced_types = std::mem::take(&mut self.field_types);
        if bound.is_some() {
            self.receiver_fields = self.fields_reached_bare(f);
            // `this.total / 2` asks what `total` is, and the record the method belongs to says
            // so.
            self.field_types = f
                .receiver
                .as_deref()
                .and_then(|r| self.record_field_types.get(r))
                .cloned()
                .unwrap_or_default();
        }
        MethodScope {
            bound,
            displaced_name,
            displaced_fields,
            displaced_types,
        }
    }

    /// Finish a method body, putting back whatever the enclosing one had bound.
    fn leave_method(&mut self, scope: MethodScope) {
        if let (Some(b), Some(p)) = (scope.bound.as_deref(), scope.displaced_name) {
            self.unbind_receiver(b, p);
        }
        self.receiver_fields = scope.displaced_fields;
        self.field_types = scope.displaced_types;
    }

    /// The fields of this method's own record, each spelled the target's way.
    fn fields_reached_bare(&self, f: &Function) -> BTreeMap<String, String> {
        let Some(declared) = f.receiver.as_deref().and_then(|r| self.records.get(r)) else {
            return BTreeMap::new();
        };
        let mut nearer: std::collections::BTreeSet<String> =
            f.params.iter().map(|p| p.name.clone()).collect();
        bound_names(&f.body, &mut nearer);
        declared
            .iter()
            .filter(|name| !nearer.contains(*name))
            .map(|name| (name.clone(), self.field(name)))
            .collect()
    }

    /// This name read as a value.
    fn value_name(&self, raw: &str) -> String {
        match self.receiver_fields.get(raw) {
            Some(field) => format!("{}.{field}", receiver_word(self.language)),
            None => self.name(raw),
        }
    }

    /// One level of indentation, in the width the target's own sources use.
    fn step(&self) -> &'static str {
        match self.language {
            // Lean's layout rules make indentation part of the syntax, and its own
            // sources indent by two.
            Language::Lean => "  ",
            _ => "    ",
        }
    }

    /// One line of output, at the current indent.
    fn line(&mut self, text: &str) {
        if text.is_empty() {
            self.text.push('\n');
            return;
        }
        let step = self.step();
        for piece in text.split('\n') {
            if !piece.is_empty() {
                for _ in 0..self.indent {
                    self.text.push_str(step);
                }
                self.text.push_str(piece);
            }
            self.text.push('\n');
        }
    }

    fn blank(&mut self) {
        if !self.text.ends_with("\n\n") && !self.text.is_empty() {
            self.text.push('\n');
        }
    }

    fn open(&mut self) {
        self.indent += 1;
    }

    fn close(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    /// Add a note the first time it comes up.
    fn note_once(&mut self, text: &str) {
        if !self.fidelity.notes.iter().any(|n| n == text) {
            self.fidelity.notes.push(text.to_string());
        }
    }

    /// Record a fragment that had no counterpart, and say where it came from.
    fn carried(&mut self, what: &Unsupported) {
        self.fidelity.carried_verbatim += 1;
        self.fidelity.notes.push(format!(
            "line {}: {} carried over unchanged",
            what.line, what.construct
        ));
    }

    fn finish(&self) -> String {
        self.text.clone()
    }

    /// `text` as a comment, every line of it.
    fn comment(&self, text: &str) -> String {
        let marker = match self.language {
            Language::Python | Language::Bash => "#",
            Language::Lean => "--",
            _ => "//",
        };
        // Zig rejects a tab inside a comment, and carried source brings the indentation the
        // other language wrote.
        let text = match self.language {
            Language::Zig => text.replace('\t', "    "),
            _ => text.to_string(),
        };
        text.split('\n')
            .map(|line| match line.is_empty() {
                true => marker.to_string(),
                false => format!("{marker} {line}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// What [`Out::enter_method`] displaced, so [`Out::leave_method`] can put it back.
struct MethodScope {
    bound: Option<String>,
    displaced_name: Option<Option<String>>,
    displaced_fields: BTreeMap<String, String>,
    displaced_types: BTreeMap<String, Type>,
}

/// Every name this body declares, at any depth, added to `into`.
fn bound_names(body: &[Stmt], into: &mut std::collections::BTreeSet<String>) {
    for stmt in body {
        match stmt {
            Stmt::BreakWith { .. } => {}
            Stmt::LocalFunction(f) => {
                into.insert(f.name.clone());
            }
            Stmt::Let { name, .. } => {
                into.insert(name.clone());
            }
            Stmt::TupleAssign { names, .. } => into.extend(names.iter().cloned()),
            Stmt::If {
                then, otherwise, ..
            } => {
                bound_names(then, into);
                bound_names(otherwise, into);
            }
            Stmt::IfPresent {
                binding,
                then,
                otherwise,
                ..
            } => {
                into.insert(binding.clone());
                bound_names(then, into);
                bound_names(otherwise, into);
            }
            Stmt::While { body, .. } => bound_names(body, into),
            Stmt::CountedFor {
                init, update, body, ..
            } => {
                for header in [init, update].iter().copied().flatten() {
                    bound_names(std::slice::from_ref(header), into);
                }
                bound_names(body, into);
            }
            Stmt::WhilePresent { binding, body, .. } => {
                into.insert(binding.clone());
                bound_names(body, into);
            }
            Stmt::ForEach {
                binding,
                body,
                iterable: _,
            } => {
                into.insert(binding.clone());
                bound_names(body, into);
            }
            Stmt::ForEachIndexed {
                index,
                binding,
                body,
                ..
            } => {
                into.insert(index.clone());
                into.insert(binding.clone());
                bound_names(body, into);
            }
            Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => bound_names(body, into),
            Stmt::Switch { arms, default, .. } => {
                for (_, body) in arms {
                    bound_names(body, into);
                }
                bound_names(default, into);
            }
            Stmt::MatchVariants { arms, default, .. } => {
                for arm in arms {
                    for (_, local) in &arm.bindings {
                        into.insert(local.clone());
                    }
                    bound_names(&arm.body, into);
                }
                bound_names(default, into);
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                bound_names(body, into);
                for catch in catches {
                    if let Some(binding) = &catch.binding {
                        into.insert(binding.clone());
                    }
                    bound_names(&catch.body, into);
                }
                bound_names(finally, into);
            }
            Stmt::Return(_)
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

/// One statement rendered onto one line, for a loop header that holds one.
fn header_line(out: &mut Out, stmt: &Stmt, write: &dyn Fn(&mut Out, &Stmt)) -> Option<String> {
    let held = std::mem::take(&mut out.text);
    let indent = std::mem::replace(&mut out.indent, 0);
    write(out, stmt);
    let rendered = std::mem::replace(&mut out.text, held);
    out.indent = indent;
    let trimmed = rendered.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }
    Some(trimmed.trim_end_matches(';').to_string())
}

/// The three clauses of a `for` header, each on its own line, for the targets that write the
/// whole header.
fn counted_header(
    out: &mut Out,
    init: Option<&Stmt>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    write: &dyn Fn(&mut Out, &Stmt),
    render: &dyn Fn(&mut Out, &Expr) -> String,
) -> Option<(String, String, String)> {
    let start = match init {
        Some(stmt) => header_line(out, stmt, write)?,
        None => String::new(),
    };
    let test = condition.map(|c| render(out, c)).unwrap_or_default();
    let step = match update {
        Some(stmt) => match c_style_step(out, stmt) {
            Some(step) => step,
            None => header_line(out, stmt, write)?,
        },
        None => String::new(),
    };
    Some((start, test, step))
}

/// `i = i + 1` written as the `i++` the C family reaches for.
fn c_style_step(out: &Out, stmt: &Stmt) -> Option<String> {
    let Stmt::Assign {
        target: Expr::Name(name),
        value,
    } = stmt
    else {
        return None;
    };
    let Expr::Binary { op, left, right } = value else {
        return None;
    };
    if !matches!(left.as_ref(), Expr::Name(n) if n == name) {
        return None;
    }
    if !matches!(right.as_ref(), Expr::Int(one) if one == "1") {
        return None;
    }
    let spelled = out.value_name(name);
    match op {
        BinaryOp::Add => Some(format!("{spelled}++")),
        BinaryOp::Sub => Some(format!("{spelled}--")),
        _ => None,
    }
}

/// The inside of a C-family `for (…)`, with the empty header spelled `;;`.
fn c_style_header(start: &str, test: &str, step: &str) -> String {
    match (start.is_empty(), test.is_empty(), step.is_empty()) {
        (true, true, true) => ";;".to_string(),
        _ => format!("{start}; {test}; {step}").trim_end().to_string(),
    }
}

/// Does a `continue` in this body belong to the loop this body is?
fn continues_here(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Continue => true,
        Stmt::If {
            then, otherwise, ..
        }
        | Stmt::IfPresent {
            then, otherwise, ..
        } => continues_here(then) || continues_here(otherwise),
        Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => continues_here(body),
        Stmt::Switch { arms, default, .. } => {
            arms.iter().any(|(_, body)| continues_here(body)) || continues_here(default)
        }
        Stmt::MatchVariants { arms, default, .. } => {
            arms.iter().any(|arm| continues_here(&arm.body)) || continues_here(default)
        }
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            continues_here(body)
                || catches.iter().any(|c| continues_here(&c.body))
                || continues_here(finally)
        }
        _ => false,
    })
}

/// The counted loop as a range over one name, when its header is that simple.
fn counted_range<'s>(
    init: Option<&'s Stmt>,
    condition: Option<&'s Expr>,
    update: Option<&'s Stmt>,
    body: &[Stmt],
) -> Option<(&'s str, &'s Expr, &'s Expr, i64)> {
    let (name, start) = match init? {
        Stmt::Let {
            name,
            value: Some(start),
            ..
        } => (name.as_str(), start),
        Stmt::Assign {
            target: Expr::Name(name),
            value,
        } => (name.as_str(), value),
        _ => return None,
    };
    let Stmt::Assign {
        target: Expr::Name(stepped),
        value,
    } = update?
    else {
        return None;
    };
    if stepped != name {
        return None;
    }
    let Expr::Binary { op, left, right } = value else {
        return None;
    };
    if !matches!(left.as_ref(), Expr::Name(n) if n == name) {
        return None;
    }
    let Expr::Int(size) = right.as_ref() else {
        return None;
    };
    let size: i64 = size.replace('_', "").parse().ok()?;
    let step = match op {
        BinaryOp::Add => size,
        BinaryOp::Sub => -size,
        _ => return None,
    };
    let Expr::Binary {
        op: test,
        left: subject,
        right: bound,
    } = condition?
    else {
        return None;
    };
    if !matches!(subject.as_ref(), Expr::Name(n) if n == name) {
        return None;
    }
    // A body that moves the counter itself is not walking a range.
    if assigns_to(body, name) {
        return None;
    }
    match (step > 0, test) {
        (true, BinaryOp::Lt) | (false, BinaryOp::Gt) => Some((name, start, bound, step)),
        _ => None,
    }
}

/// Does anything under these statements assign to `name`?
fn assigns_to(body: &[Stmt], name: &str) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Assign {
            target: Expr::Name(target),
            ..
        } => target == name,
        Stmt::If {
            then, otherwise, ..
        }
        | Stmt::IfPresent {
            then, otherwise, ..
        } => assigns_to(then, name) || assigns_to(otherwise, name),
        Stmt::While { body, .. }
        | Stmt::WhilePresent { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachIndexed { body, .. }
        | Stmt::Defer(body)
        | Stmt::ErrDefer(body)
        | Stmt::Block(body) => assigns_to(body, name),
        Stmt::CountedFor {
            init, update, body, ..
        } => {
            [init, update]
                .iter()
                .copied()
                .flatten()
                .any(|s| assigns_to(std::slice::from_ref(s), name))
                || assigns_to(body, name)
        }
        Stmt::Switch { arms, default, .. } => {
            arms.iter().any(|(_, body)| assigns_to(body, name)) || assigns_to(default, name)
        }
        Stmt::MatchVariants { arms, default, .. } => {
            arms.iter().any(|arm| assigns_to(&arm.body, name)) || assigns_to(default, name)
        }
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            assigns_to(body, name)
                || catches.iter().any(|c| assigns_to(&c.body, name))
                || assigns_to(finally, name)
        }
        _ => false,
    })
}

/// The counted loop as its own source, for a writer that cannot spell it.
fn counted_original(source: &str, line: usize) -> Unsupported {
    Unsupported {
        construct: "counted for loop".to_string(),
        source: source.to_string(),
        line,
    }
}

/// Write a carried-over fragment as a comment, whole, so nothing is lost.
fn zig_map_shape(out: &Out, ty: Option<&Type>, entries: &[(Expr, Expr)]) -> (String, String) {
    let declared = match ty {
        Some(Type::Map(k, v)) => Some((zig_type(k), zig_type(v))),
        _ => None,
    };
    let from_entries = |pick: fn(&(Expr, Expr)) -> &Expr| -> String {
        match entries.first().map(pick) {
            Some(Expr::Str(_) | Expr::Template(_)) => "[]const u8".to_string(),
            Some(Expr::Float(_)) => "f64".to_string(),
            Some(Expr::Bool(_)) => "bool".to_string(),
            _ => "i64".to_string(),
        }
    };
    let keys = declared
        .as_ref()
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| from_entries(|(k, _)| k));
    let values = declared
        .map(|(_, v)| v)
        .unwrap_or_else(|| from_entries(|(_, v)| v));
    let _ = out;
    match keys == "[]const u8" {
        true => ("std.StringHashMap".to_string(), values),
        false => ("std.AutoHashMap".to_string(), format!("{keys}, {values}")),
    }
}

/// Does this map hold owned strings as its keys?
fn owned_keys(out: &Out, of: &Expr) -> bool {
    let Expr::Name(name) = of else {
        return false;
    };
    matches!(out.binding_types.get(name), Some(Type::Map(k, _)) if **k == Type::String)
}

/// Does this expression name a binding the writer knows to hold a map?
fn holds_a_map(out: &Out, of: &Expr) -> bool {
    let Expr::Name(name) = of else {
        return false;
    };
    matches!(out.binding_types.get(name), Some(Type::Map(_, _)))
}

/// Does this name hold a set?
fn holds_a_set(out: &Out, of: &Expr) -> bool {
    let Expr::Name(name) = of else {
        return false;
    };
    matches!(out.binding_types.get(name), Some(Type::Set(_)))
}

/// The type a literal states about itself, where it states one.
fn literal_type_of(e: &Expr) -> Option<Type> {
    match e {
        Expr::Int(_) => Some(Type::Int),
        Expr::Float(_) => Some(Type::Float),
        Expr::Str(_) | Expr::Template(_) => Some(Type::String),
        Expr::Bool(_) => Some(Type::Bool),
        _ => None,
    }
}

/// What a map literal's keys and values are, from its first entry.
fn map_literal_types(entries: &[(Expr, Expr)]) -> (Option<Type>, Option<Type>) {
    match entries.first() {
        Some((k, v)) => (literal_type_of(k), literal_type_of(v)),
        None => (None, None),
    }
}

/// What a map literal holds, as TypeScript spells it.
fn ts_map_values(entries: &[(Expr, Expr)]) -> &'static str {
    match entries.first().map(|(_, v)| v) {
        Some(Expr::Int(_) | Expr::Float(_)) => "number",
        Some(Expr::Str(_) | Expr::Template(_)) => "string",
        Some(Expr::Bool(_)) => "boolean",
        _ => "any",
    }
}

fn carry(out: &mut Out, what: &Unsupported) {
    out.carried(what);
    let header = out.comment(&format!(
        "{MARKER}: {} from line {}",
        what.construct, what.line
    ));
    out.line(&header);
    for line in what.source.lines() {
        let commented = out.comment(line);
        out.line(&commented);
    }
}

/// These statements as Rust text, for a carry that keeps its body.
pub(super) fn render_rust_stmts(stmts: &[Stmt]) -> String {
    let mut scratch = Out::new(Language::Rust);
    rust_block(&mut scratch, stmts, None);
    scratch.text
}

/// Does this expression read `name` anywhere inside it?
fn expr_reads(e: &Expr, name: &str) -> bool {
    let mut found = false;
    let mut probe = e.clone();
    crate::transpile::read::each_expr(&mut probe, &mut |inner| {
        if matches!(inner, Expr::Name(n) if n == name) {
            found = true;
        }
    });
    found
}

/// The discriminator literal a variant answers to on the wire.
fn wire_tag(out: &Out, sum: &str, variant: &str) -> String {
    out.sum_items
        .get(sum)
        .and_then(|s| s.variants.iter().find(|v| v.name == variant))
        .and_then(|v| v.tag.clone())
        .unwrap_or_else(|| snake_always(variant))
}

/// A hoisted variant's class name in the output, collision dodge included.
fn variant_spelling(out: &Out, sum: &str, variant: &str) -> String {
    out.variant_spellings
        .get(&(sum.to_string(), variant.to_string()))
        .cloned()
        .unwrap_or_else(|| out.name(variant))
}

/// `snake_case`, for Rust and Python.
pub(super) fn snake_always(name: &str) -> String {
    // A separator goes before an uppercase letter only where a word starts.
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 && !out.ends_with('_') {
            let previous = chars[i - 1];
            let starts_word = previous.is_lowercase()
                || previous.is_numeric()
                || (previous.is_uppercase() && chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
            if starts_word {
                out.push('_');
            }
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// `SCREAMING_SNAKE`, for a module-level constant.
fn screaming(name: &str) -> String {
    snake_always(name).to_uppercase()
}

/// `camelCase`, for TypeScript and unexported Go.
pub(super) fn camel(name: &str) -> String {
    // Lower SCREAMING_SNAKE before re-casing it, or `MIN_CELSIUS`
    // becomes `MINCELSIUS` and not `minCelsius`.
    let screaming = name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric());
    let source = if screaming {
        name.to_lowercase()
    } else {
        name.to_string()
    };

    // A leading underscore is Python's and Rust's word for "not for outside this module", not a
    // word boundary.
    let source = source.trim_start_matches('_').to_string();
    let mut out = String::with_capacity(source.len());
    let mut upper_next = false;
    for (i, c) in source.chars().enumerate() {
        if c == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else if i == 0 {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `PascalCase`, for type names everywhere and exported Go.
pub(super) fn pascal(name: &str) -> String {
    let camel = camel(name);
    let mut chars = camel.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => camel,
    }
}

/// What this module calls its floor-division helper.
fn floor_div_name(out: &Out) -> String {
    let taken = out.functions.contains_key("floor_div")
        || out.names.values().any(|spelled| spelled == "floor_div");
    match taken {
        true => "floor_div_helper".to_string(),
        false => "floor_div".to_string(),
    }
}

fn rust(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Statement(stmt) if calls_declared_main(out, stmt) => {
                out.note_once(ENTRY_DROPPED);
            }
            Item::Statement(stmt) => carried_statement(out, stmt, rust_expr),
            Item::Constant(c) => {
                // This language evaluates a constant at compile time.
                if contains_unsupported(&c.value) {
                    let rendered = rust_expr(out, &c.value);
                    let header = out.comment(&format!(
                        "{MARKER}: a constant whose value did not translate; written \
                         as a const it would stop the build at compile-time evaluation."
                    ));
                    out.line(&header);
                    let declaration = out.comment(&format!("const {} = {rendered};", c.name));
                    out.line(&declaration);
                    out.blank();
                    continue;
                }
                // The type is not decoration.
                for line in &c.doc {
                    out.line(&format!("/// {line}"));
                }
                let ty = c.ty.as_ref().map(rust_type).unwrap_or_else(|| {
                    rust_literal_type(&c.value).unwrap_or_else(|| "&str".to_string())
                });
                let visibility = if c.exported { "pub " } else { "" };
                let value = match &c.value {
                    Expr::ListLit(items) => format!(
                        "&[{}]",
                        items
                            .iter()
                            .map(|i| rust_expr(out, i))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    other => rust_expr(out, other),
                };
                out.line(&format!(
                    "{visibility}const {}: {ty} = {value};",
                    out.name(&c.name)
                ));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                for line in &r.doc {
                    out.line(&format!("/// {line}"));
                }
                let visibility = if r.exported { "pub " } else { "" };
                let type_name = out.name(&r.name);
                // A field the source never typed is a field whose type the caller picks, which
                // in Rust is a parameter on the struct.
                let mut undeclared = Vec::new();
                out.record_generics.clear();
                for f in &r.fields {
                    if f.ty.is_none() {
                        let parameter = format!("T{}", undeclared.len());
                        out.record_generics
                            .push((f.name.clone(), parameter.clone()));
                        undeclared.push(parameter);
                    }
                }
                let generics = match undeclared.is_empty() {
                    true => String::new(),
                    false => format!("<{}>", undeclared.join(", ")),
                };
                inherited_base(out, r, false);
                // What every other language hands a record for free: copying it,
                // printing it, and comparing two of them.
                if r.fields.iter().all(|f| derivable(f.ty.as_ref())) {
                    out.line("#[derive(Clone, Debug, PartialEq)]");
                }
                out.line(&format!("{visibility}struct {type_name}{generics} {{"));
                out.open();
                let mut taken = undeclared.iter();
                for f in &r.fields {
                    for line in &f.doc {
                        out.line(&format!("/// {line}"));
                    }
                    let ty = match &f.ty {
                        Some(t) => rust_type(t),
                        None => taken.next().cloned().unwrap_or_else(|| "()".to_string()),
                    };
                    let field_visibility = if f.exported { "pub " } else { "" };
                    let field_name = out.field(&f.name);
                    out.line(&format!("{field_visibility}{field_name}: {ty},"));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();
                // Field defaults have no slot in a struct; `Default` is where
                // Rust keeps a record's starting values.
                if r.fields.iter().any(|f| f.default.is_some()) {
                    out.line(&format!(
                        "impl{generics} Default for {type_name}{generics} {{"
                    ));
                    out.open();
                    out.line("fn default() -> Self {");
                    out.open();
                    out.line("Self {");
                    out.open();
                    for f in &r.fields {
                        let field_name = out.field(&f.name);
                        let value = match &f.default {
                            Some(value) => rust_expr(out, value),
                            None => "Default::default()".to_string(),
                        };
                        out.line(&format!("{field_name}: {value},"));
                    }
                    out.close();
                    out.line("}");
                    out.close();
                    out.line("}");
                    out.close();
                    out.line("}");
                    out.blank();
                }

                if !r.methods.is_empty() {
                    // Rust declares methods apart from the type, which the
                    // record's method list becomes.
                    let type_name = out.name(&r.name);
                    out.line(&format!("impl{generics} {type_name}{generics} {{"));
                    out.open();
                    out.record_written = Some(r.name.clone());
                    // Java overloads share a name and Rust has no overloading, so two `fn add`
                    // in one `impl` do not compile.
                    let mut spelled: std::collections::BTreeMap<String, usize> =
                        std::collections::BTreeMap::new();
                    for m in &methods_of(out, r, false) {
                        let seen = spelled.entry(m.name.clone()).or_insert(0);
                        *seen += 1;
                        let mut renamed = m.clone();
                        if *seen > 1 {
                            out.note_once(
                                "overloads share a name the target refuses to repeat; \
                                 later overloads take a numbered name.",
                            );
                            renamed.name = format!("{}{}", m.name, *seen);
                        }
                        rust_function(out, &renamed, renamed.receiver_binding.is_some());
                    }
                    out.record_written = None;
                    out.record_generics.clear();
                    out.close();
                    out.line("}");
                    out.blank();
                }
            }
            Item::Function(f) => {
                rust_function(out, f, false);
                out.blank();
            }
            Item::Import { text, line, .. } => {
                out.fidelity.imports_listed += 1;
                let header = out.comment(&format!(
                    "the source imported this at line {line}; the equivalent here is \
                     yours to add"
                ));
                out.line(&header);
                for l in text.lines() {
                    let commented = out.comment(l);
                    out.line(&commented);
                }
                out.blank();
            }
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("/// {line}"));
                }
                let visibility = if n.exported { "pub " } else { "" };
                out.line(&format!(
                    "{visibility}struct {}(pub {});",
                    out.name(&n.name),
                    rust_type(&n.base)
                ));
                out.fidelity.newtypes += 1;
                out.blank();
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("/// {line}"));
                }
                // Zig's doctest form names a test after the declaration it covers,
                // and here the two would share one namespace; the prefix keeps both.
                let slug = test_slug(name);
                let slug = match out.functions.contains_key(&slug) {
                    true => format!("test_{slug}"),
                    false => slug,
                };
                out.line("#[test]");
                out.line(&format!("fn {slug}() {{"));
                out.open();
                rust_block(out, body, None);
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                for line in &s.doc {
                    out.line(&format!("/// {line}"));
                }
                let visibility = if s.exported { "pub " } else { "" };
                let type_name = out.name(&s.name);
                out.line(&format!("{visibility}enum {type_name} {{"));
                out.open();
                for variant in &s.variants {
                    for line in &variant.doc {
                        out.line(&format!("/// {line}"));
                    }
                    let variant_name = out.name(&variant.name);
                    if variant.fields.is_empty() {
                        out.line(&format!("{variant_name},"));
                        continue;
                    }
                    let mut fields = Vec::new();
                    for f in &variant.fields {
                        let ty =
                            f.ty.as_ref()
                                .map(rust_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        fields.push(format!("{}: {ty}", out.field(&f.name)));
                    }
                    out.line(&format!("{variant_name} {{ {} }},", fields.join(", ")));
                }
                out.close();
                out.line("}");
                out.fidelity.sums += 1;
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }

    if out.zig_helpers.contains("rust_defer") {
        out.blank();
        out.line("/// Runs its closure when dropped: a scope-exit hook. `defer` arrives");
        out.line("/// this way, and `errdefer` disarms it on the successful path.");
        out.line("struct FrDefer<F: FnMut()>(Option<F>);");
        out.line("impl<F: FnMut()> Drop for FrDefer<F> {");
        out.open();
        out.line("fn drop(&mut self) {");
        out.open();
        out.line("if let Some(mut run) = self.0.take() {");
        out.open();
        out.line("run();");
        out.close();
        out.line("}");
        out.close();
        out.line("}");
        out.close();
        out.line("}");
        out.blank();
    }
    if out.zig_helpers.contains("rust_floor_rem") {
        out.blank();
        out.line("/// The remainder that goes with division rounding toward negative");
        out.line("/// infinity: `%` truncates and `rem_euclid` never goes negative.");
        out.line("fn fr_floor_rem(dividend: i64, divisor: i64) -> i64 {");
        out.open();
        out.line("let remainder = dividend % divisor;");
        out.line("match remainder != 0 && (remainder < 0) != (divisor < 0) {");
        out.open();
        out.line("true => remainder + divisor,");
        out.line("false => remainder,");
        out.close();
        out.line("}");
        out.close();
        out.line("}");
        out.blank();
    }
    if out.needs_floor_div {
        let name = floor_div_name(out);
        out.blank();
        out.line("/// Division that rounds toward negative infinity.");
        out.line("///");
        out.line("/// The standard library's Euclidean division keeps the remainder");
        out.line("/// positive, which is another answer when the divisor is negative.");
        out.line(&format!("fn {name}(dividend: i64, divisor: i64) -> i64 {{"));
        out.open();
        out.line("let quotient = dividend / divisor;");
        out.line("let remainder = dividend % divisor;");
        out.line("match remainder != 0 && (remainder < 0) != (divisor < 0) {");
        out.open();
        out.line("true => quotient - 1,");
        out.line("false => quotient,");
        out.close();
        out.line("}");
        out.close();
        out.line("}");
        out.blank();
    }
}

/// Can Rust derive the ordinary traits over a field of this type?
fn derivable(ty: Option<&Type>) -> bool {
    match ty {
        None => true,
        Some(Type::Named { name, args }) => {
            Type::is_writable_name(name) && args.iter().all(|a| derivable(Some(a)))
        }
        Some(Type::List(inner) | Type::Optional(inner)) => derivable(Some(inner)),
        Some(Type::Map(k, v)) => derivable(Some(k)) && derivable(Some(v)),
        Some(Type::Tuple(parts)) => parts.iter().all(|p| derivable(Some(p))),
        Some(_) => true,
    }
}

fn rust_function(out: &mut Out, f: &Function, method: bool) {
    // Bindings made inside blocks die at their brace here too, and the closures a `try` lowers
    // to cannot capture an uninitialized slot.
    let f = &with_hoisted_bindings(f, &out.function_returns);
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);
    out.rust_mutated = rust_mutated_names(&f.body);
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);

    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        out.line(&out.comment("declared async in the source"));
    }
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        // A body that writes to a field needs the receiver to be writable, and `&self` is not.
        let word = receiver_word(out.language);
        let mutation = match assigns_to_receiver(f, word) {
            true => "mut ",
            false => "",
        };
        params.push(format!("&{mutation}{word}"));
    }
    let mut foreign = false;
    let mut unannotated = false;
    for p in &f.params {
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(spelled);
            continue;
        }
        let ty = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                rust_type(t)
            }
            None => {
                unannotated = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("{spelled}: {ty}"));
    }
    // A function that can fail says so in its signature here: `Result<T, String>`,
    // with the message as the error the way every thrown message crossed.
    let throws = f.receiver.is_none() && out.throwing.contains(&f.name);
    out.can_propagate = throws;
    out.fn_throws = throws;
    let returns = match &f.returns {
        _ if throws => {
            let ok = match &f.returns {
                None | Some(Type::Unit) => "()".to_string(),
                Some(t) => {
                    if out.is_foreign(t) {
                        foreign = true;
                    }
                    rust_type(t)
                }
            };
            format!(" -> Result<{ok}, String>")
        }
        Some(Type::Unit) => String::new(),
        // A source that annotated nothing still hands a value back, and this target has to name
        // its type.
        None if returns_a_value(f) => {
            unannotated = true;
            let ty = match inferred_return(out, f) {
                Some(ty) => rust_type(&ty),
                None => unknown(out, &f.name),
            };
            format!(" -> {ty}")
        }
        None => String::new(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            // A method that hands back the record it lives on names it without the arguments
            // the struct declares.
            let names_its_own = match (t, &out.record_written) {
                (Type::Named { name, args }, Some(record)) => name == record && args.is_empty(),
                _ => false,
            };
            match names_its_own {
                true => " -> Self".to_string(),
                false => format!(" -> {}", rust_type(t)),
            }
        }
    };
    let visibility = if f.exported { "pub " } else { "" };
    // Each parameter the source left untyped becomes a type of its own, named in order.
    let called = called_parameters(f);
    let mut generics: Vec<String> = Vec::new();
    for (at_param, param) in params.iter_mut().enumerate() {
        while let Some(at) = param.find(TYPE_THE_CALLER_DECIDES) {
            // A parameter the body calls is a function, and no widest type is callable.
            let calls = f
                .params
                .get(at_param)
                .and_then(|p| called.get(&p.name).copied());
            match calls {
                Some(arity) => {
                    let arguments = vec!["i64".to_string(); arity].join(", ");
                    let answers = f
                        .returns
                        .as_ref()
                        .map(rust_type)
                        .unwrap_or_else(|| "i64".to_string());
                    let spelled = format!("impl Fn({arguments}) -> {answers}");
                    param.replace_range(at..at + TYPE_THE_CALLER_DECIDES.len(), &spelled);
                }
                None => {
                    let held = f.params.get(at_param).and_then(|p| {
                        out.record_generics
                            .iter()
                            .find(|(field, _)| *field == p.name)
                            .map(|(_, parameter)| parameter.clone())
                    });
                    match held {
                        // Skip: the struct declares this one, and a second would shadow it
                        // under a different type.
                        Some(name) => {
                            param.replace_range(at..at + TYPE_THE_CALLER_DECIDES.len(), &name)
                        }
                        None => {
                            let name = format!("T{}", generics.len());
                            param.replace_range(at..at + TYPE_THE_CALLER_DECIDES.len(), &name);
                            generics.push(name);
                        }
                    }
                }
            }
        }
    }
    let mut returns = returns;
    while let Some(at) = returns.find(TYPE_THE_CALLER_DECIDES) {
        let name = format!("T{}", generics.len());
        returns.replace_range(at..at + TYPE_THE_CALLER_DECIDES.len(), &name);
        generics.push(name);
    }
    let bound = match generics.is_empty() {
        true => String::new(),
        false => format!("<{}>", generics.join(", ")),
    };
    out.line(&format!(
        "{visibility}fn {}{bound}({}){returns} {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    rust_block(out, &f.body, f.returns.as_ref());
    // The success path a body falls off the end of still has to be said.
    if throws && !matches!(f.body.last(), Some(Stmt::Return(_)) | Some(Stmt::Throw(_))) {
        out.line("Ok(())");
    }
    out.close();
    out.line("}");
    out.can_propagate = false;
    out.fn_throws = false;

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// `var x; switch { every arm assigns x }` folded back into `let x = match ...;`.
fn switch_binding_expression(out: &mut Out, body: &[Stmt], at: usize) -> Option<String> {
    let Stmt::Let {
        name,
        ty,
        value: None,
        ..
    } = &body[at]
    else {
        return None;
    };
    let Some(Stmt::Switch {
        subject,
        arms,
        default,
    }) = body.get(at + 1)
    else {
        return None;
    };
    let assigned = |stmts: &[Stmt]| -> Option<Expr> {
        match stmts {
            [Stmt::Assign {
                target: Expr::Name(n),
                value,
            }] if n == name => Some(value.clone()),
            _ => None,
        }
    };
    // Collect every value before rendering anything.
    let mut values = Vec::new();
    for (_, arm) in arms {
        values.push(assigned(arm)?);
    }
    let default_value = assigned(default)?;
    let mut rendered = Vec::new();
    for ((literals, _), value) in arms.iter().zip(&values) {
        let pattern: Vec<String> = literals.iter().map(|l| rust_expr(out, l)).collect();
        rendered.push(format!(
            "{} => {}",
            pattern.join(" | "),
            rust_expr(out, value)
        ));
    }
    rendered.push(format!("_ => {}", rust_expr(out, &default_value)));
    let annotation = rust_binding_annotation(ty);
    Some(format!(
        "let {}{annotation} = match {} {{ {} }};",
        out.name(name),
        rust_expr(out, subject),
        rendered.join(", ")
    ))
}

fn rust_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        out.line("todo!()");
        return;
    }
    let mut at = 0usize;
    while at < body.len() {
        if let Some(folded) = switch_binding_expression(out, body, at) {
            out.line(&folded);
            at += 2;
            continue;
        }
        let stmt = &body[at];
        at += 1;
        match stmt {
            Stmt::Block(stmts) => {
                out.line("{");
                out.open();
                rust_block(out, stmts, None);
                out.close();
                out.line("}");
            }
            Stmt::LocalFunction(f) => {
                rust_function(out, f, false);
            }
            Stmt::BreakWith { label, value } => {
                let rendered = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_default();
                carry_labeled_break(out, label, &rendered);
            }
            Stmt::Return(value) => {
                let mut text = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_default();
                // `return 0` under a signature that promised a float: Go and Zig
                // coerce the untyped literal, Rust refuses it.
                if matches!(returns, Some(Type::Float)) && matches!(value, Some(Expr::Int(_))) {
                    text.push_str(".0");
                }
                // A literal under a signature that promised `String` is a `&str`
                // everywhere else and a type error here.
                if matches!(returns, Some(Type::String)) && matches!(value, Some(Expr::Str(_))) {
                    text.push_str(".to_string()");
                }
                // Rust converts between its number types only when told to.
                let widens = matches!(returns, Some(Type::Float))
                    && !matches!(value, Some(Expr::Int(_)))
                    && value
                        .as_ref()
                        .is_some_and(|v| matches!(static_type(out, v), Some(Type::Int)));
                if widens {
                    text = format!("({text}) as f64");
                }
                // A throwing function returns `Result`, so its successes wrap.
                if out.fn_throws {
                    text = match text.is_empty() {
                        true => "Ok(())".to_string(),
                        false => format!("Ok({text})"),
                    };
                }
                out.line(&format!("return {text};"));
            }
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                let annotation = rust_binding_annotation(ty);
                let m = if *mutable || out.rust_mutated.contains(name) {
                    "mut "
                } else {
                    ""
                };
                let mut v = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_else(|| "Default::default()".to_string());
                // A literal under a `String` annotation is a `&str` everywhere else.
                if matches!(
                    (ty, value.as_ref()),
                    (Some(Type::String), Some(Expr::Str(_)))
                ) {
                    v.push_str(".to_string()");
                }
                // The same inside a list: `vec!["a"]` under `Vec<String>` is a
                // list of `&str` and will not compile.
                if let (Some(Type::List(element)), Some(Expr::ListLit(items))) =
                    (ty, value.as_ref())
                {
                    if **element == Type::String && items.iter().all(|i| matches!(i, Expr::Str(_)))
                    {
                        let owned: Vec<String> = items
                            .iter()
                            .map(|i| format!("{}.to_string()", rust_expr(out, i)))
                            .collect();
                        v = format!("vec![{}]", owned.join(", "));
                    }
                }
                let bound = out.name(name);
                out.line(&format!("let {m}{bound}{annotation} = {v};"));
            }
            Stmt::Assign { target, value } => {
                // `m[k] = v` on a map is an insert here: `HashMap` has no
                // `IndexMut`, so the index form is `E0594` and does not compile.
                if let Expr::Index { of, index } = target {
                    if holds_a_map(out, of) {
                        let map = rust_expr(out, of);
                        let key = match (owned_keys(out, of), index.as_ref()) {
                            (true, Expr::Str(_)) => {
                                format!("{}.to_string()", rust_expr(out, index))
                            }
                            _ => rust_expr(out, index),
                        };
                        let v = rust_expr(out, value);
                        out.line(&format!("{map}.insert({key}, {v});"));
                        continue;
                    }
                }
                let t = rust_expr(out, target);
                let v = rust_expr(out, value);
                out.line(&format!("{t} = {v};"));
            }
            Stmt::TupleAssign {
                names,
                value,
                declares,
                ..
            } => {
                let v = rust_expr(out, value);
                let bound = joined(names, |n| match *declares && n != "_" {
                    true => format!("mut {}", out.name(n)),
                    false => out.name(n),
                });
                let keyword = if *declares { "let " } else { "" };
                out.line(&format!("{keyword}({bound}) = {v};"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = rust_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                rust_block(out, then, returns);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    rust_block(out, otherwise, returns);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::IfPresent {
                binding,
                value,
                then,
                otherwise,
            } => {
                let v = rust_expr(out, value);
                let bound = out.name(binding);
                out.line(&format!("if let Some({bound}) = {v} {{"));
                out.open();
                rust_block(out, then, returns);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    rust_block(out, otherwise, returns);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::MatchVariants {
                subject,
                sum,
                arms,
                default,
            } => {
                let s = rust_expr(out, subject);
                let owner = out.name(sum);
                out.line(&format!("match {s} {{"));
                out.open();
                for arm in arms {
                    let variant = out.name(&arm.variant);
                    let pattern = match arm.bindings.is_empty() {
                        true => format!("{owner}::{variant}"),
                        false => {
                            let bound = arm
                                .bindings
                                .iter()
                                .map(|(field, local)| {
                                    let field = out.field(field);
                                    let local = out.name(local);
                                    match field == local {
                                        true => field,
                                        false => format!("{field}: {local}"),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{owner}::{variant} {{ {bound}, .. }}")
                        }
                    };
                    out.line(&format!("{pattern} => {{"));
                    out.open();
                    rust_block(out, &arm.body, returns);
                    out.close();
                    out.line("}");
                }
                if default.is_empty() {
                    out.line("_ => {}");
                } else {
                    out.line("_ => {");
                    out.open();
                    rust_block(out, default, returns);
                    out.close();
                    out.line("}");
                }
                out.close();
                out.line("}");
            }
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                let mut s = rust_expr(out, subject);
                // A `String` subject matches `&str` literals only through its view.
                let stringly = arms
                    .iter()
                    .flat_map(|(literals, _)| literals)
                    .any(|l| matches!(l, Expr::Str(_)));
                if stringly && matches!(static_type(out, subject), Some(Type::String)) {
                    s = format!("{s}.as_str()");
                }
                let integral_labels = arms
                    .iter()
                    .flat_map(|(literals, _)| literals)
                    .all(|l| matches!(l, Expr::Int(_)));
                if integral_labels && matches!(static_type(out, subject), Some(Type::Float)) {
                    s = format!("({s}) as i64");
                }
                out.line(&format!("match {s} {{"));
                out.open();
                for (literals, body) in arms {
                    let pattern: Vec<String> = literals.iter().map(|l| rust_expr(out, l)).collect();
                    out.line(&format!("{} => {{", pattern.join(" | ")));
                    out.open();
                    rust_block(out, body, returns);
                    out.close();
                    out.line("}");
                }
                // Rust demands exhaustiveness, so emit the default arm even where the
                // source had none.
                if default.is_empty() {
                    out.line("_ => {}");
                } else {
                    out.line("_ => {");
                    out.open();
                    rust_block(out, default, returns);
                    out.close();
                    out.line("}");
                }
                out.close();
                out.line("}");
            }
            Stmt::Defer(cleanup) if !exits_anywhere(&body[at..]) => {
                // Nothing between here and the end of the scope can leave it, so the deferral
                // is a plain reordering: the rest runs, then the cleanup.
                rust_block(out, &body[at..], returns);
                rust_block(out, cleanup, None);
                return;
            }
            Stmt::ErrDefer(cleanup) | Stmt::Defer(cleanup) => {
                // A guard runs the cleanup when the scope exits, however it exits.
                let failure_only = matches!(stmt, Stmt::ErrDefer(_));
                out.zig_helpers.insert("rust_defer");
                out.lowering_names += 1;
                let guard = format!("__fr_guard{}", out.lowering_names);
                out.line(&format!("let mut {guard} = FrDefer(Some(|| {{"));
                out.open();
                rust_block(out, cleanup, None);
                out.close();
                out.line("}));");
                out.line(&format!("let _ = &mut {guard};"));
                let mut rest: Vec<Stmt> = body[at..].to_vec();
                if failure_only {
                    let disarm = Stmt::Assign {
                        target: Expr::Field {
                            of: Box::new(Expr::Name(guard.clone())),
                            name: "0".to_string(),
                        },
                        value: Expr::Null,
                    };
                    disarm_before_returns(&mut rest, &disarm);
                    rest.push(disarm);
                }
                rust_block(out, &rest, returns);
                return;
            }
            Stmt::WhilePresent {
                binding,
                value,
                body,
            } => {
                let v = rust_expr(out, value);
                let bound = out.name(binding);
                out.line(&format!("while let Some({bound}) = {v} {{"));
                out.open();
                rust_block(out, body, returns);
                out.close();
                out.line("}");
            }
            Stmt::While { condition, body } => {
                let c = rust_expr(out, condition);
                out.line(&format!("while {c} {{"));
                out.open();
                rust_block(out, body, returns);
                out.close();
                out.line("}");
            }
            // Rust has no counted header, so the start goes before the loop and the step at the
            // foot of the body.
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                source,
                line,
            } => {
                // Rust's range counts up by one.
                let by_one =
                    counted_range(init.as_deref(), condition.as_ref(), update.as_deref(), body)
                        .filter(|(_, _, _, step)| *step == 1);
                if let Some((name, start, bound, _)) = by_one {
                    let (start, bound) = (rust_expr(out, start), rust_expr(out, bound));
                    let name = out.name(name);
                    out.line(&format!("for {name} in {start}..{bound} {{"));
                    out.open();
                    rust_block(out, body, returns);
                    out.close();
                    out.line("}");
                } else if update.is_some() && continues_here(body) {
                    carry(out, &counted_original(source, *line));
                } else {
                    let scoped = init.is_some();
                    if scoped {
                        out.line("{");
                        out.open();
                    }
                    if let Some(init) = init {
                        rust_block(out, std::slice::from_ref(init.as_ref()), None);
                    }
                    match condition {
                        Some(c) => {
                            let c = rust_expr(out, c);
                            out.line(&format!("while {c} {{"));
                        }
                        None => out.line("loop {"),
                    }
                    out.open();
                    rust_block(out, body, returns);
                    if let Some(update) = update {
                        rust_block(out, std::slice::from_ref(update.as_ref()), None);
                    }
                    out.close();
                    out.line("}");
                    if scoped {
                        out.close();
                        out.line("}");
                    }
                }
            }
            Stmt::ForEachIndexed {
                index,
                binding,
                iterable,
                body,
            } => {
                let it = rust_expr(out, iterable);
                let i = out.name(index);
                let bound = out.name(binding);
                out.line(&format!("for ({i}, {bound}) in {it}.iter().enumerate() {{"));
                out.open();
                rust_block(out, body, returns);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let mut it = rust_expr(out, iterable);
                // A named collection iterates by reference, or the first loop eats it.
                if matches!(iterable, Expr::Name(_)) && !it.starts_with('&') {
                    it = format!("{it}.iter().cloned()");
                }
                let bound = out.name(binding);
                out.line(&format!("for {bound} in {it} {{"));
                out.open();
                rust_block(out, body, returns);
                out.close();
                out.line("}");
            }
            Stmt::Expr(e) => {
                let text = rust_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Assert { condition, message } => {
                let c = rust_expr(out, condition);
                match message {
                    // The message is the macro's own format string, so a literal rides along
                    // with its braces doubled.
                    Some(Expr::Str(text)) => {
                        let literal = quoted(Language::Rust, text)
                            .replace('{', "{{")
                            .replace('}', "}}");
                        out.line(&format!("assert!({c}, {literal});"));
                    }
                    // A message that is not a literal goes in as an argument to the macro's own
                    // `{}`.
                    Some(other) => {
                        let rendered = rust_expr(out, other);
                        out.line(&format!("assert!({c}, \"{{}}\", {rendered});"));
                    }
                    None => out.line(&format!("assert!({c});")),
                }
            }
            Stmt::Break => out.line("break;"),
            Stmt::Continue => out.line("continue;"),
            // Rust models failure in the return type and has no catch block, so carry
            // this whole.
            Stmt::Try {
                body: tried,
                catches,
                finally,
                source: _,
                line: _,
            } => {
                if catches.is_empty() && !finally.is_empty() && !exits_anywhere(tried) {
                    // A try that only ever finishes runs its body and then its
                    // finally; there is nothing to catch.
                    rust_block(out, tried, None);
                    rust_block(out, finally, None);
                    return;
                }
                // A finally that must survive early exits is the same guard a
                // defer needs.
                if catches.is_empty() {
                    out.zig_helpers.insert("rust_defer");
                    out.lowering_names += 1;
                    let guard = format!("__fr_guard{}", out.lowering_names);
                    out.line(&format!("let mut {guard} = FrDefer(Some(|| {{"));
                    out.open();
                    rust_block(out, finally, None);
                    out.close();
                    out.line("}));");
                    out.line(&format!("let _ = &mut {guard};"));
                    rust_block(out, tried, returns);
                    return;
                }
                if returns_anywhere(tried) {
                    // The closure that catches also swallows returns, so a return inside
                    // travels out through an Option and returns here.
                    out.lowering_names += 1;
                    let caught = format!("__fr_caught{}", out.lowering_names);
                    let ret_ty = returns.map(rust_type).unwrap_or_else(|| "()".to_string());
                    out.line(&format!(
                        "let {caught}: Result<Option<{ret_ty}>, String> = (|| {{"
                    ));
                    out.open();
                    let was = out.can_propagate;
                    out.can_propagate = true;
                    let mut wrapped = tried.clone();
                    route_returns_through_some(&mut wrapped);
                    rust_block(out, &wrapped, None);
                    out.line("Ok(None)");
                    out.can_propagate = was;
                    out.close();
                    out.line("})();");
                    if !finally.is_empty() {
                        rust_block(out, finally, None);
                    }
                    let first = &catches[0];
                    if catches.len() > 1 {
                        out.fidelity.notes.push(format!(
                            "a try with {} catch arms folded into one: the arms \
                             selected by exception class, and the classes did not cross",
                            catches.len()
                        ));
                    }
                    let binding = first
                        .binding
                        .as_deref()
                        .map(|b| out.name(b))
                        .unwrap_or_else(|| "_".to_string());
                    out.line(&format!("match {caught} {{"));
                    out.open();
                    out.line(&format!("Err({binding}) => {{"));
                    out.open();
                    rust_block(out, &first.body, returns);
                    out.close();
                    out.line("}");
                    match returns.is_some() {
                        true => out.line("Ok(Some(__fr_ret)) => return __fr_ret,"),
                        false => out.line("Ok(Some(())) => return,"),
                    }
                    out.line("Ok(None) => {}");
                    out.close();
                    out.line("}");
                    return;
                }
                {
                    out.lowering_names += 1;
                    let caught = format!("__fr_caught{}", out.lowering_names);
                    out.line(&format!("let {caught}: Result<(), String> = (|| {{"));
                    out.open();
                    let was = out.can_propagate;
                    out.can_propagate = true;
                    rust_block(out, tried, None);
                    out.line("Ok(())");
                    out.can_propagate = was;
                    out.close();
                    out.line("})();");
                    let first = &catches[0];
                    if catches.len() > 1 {
                        out.fidelity.notes.push(format!(
                            "a try with {} catch arms folded into one: the arms \
                             selected by exception class, and the classes did not cross",
                            catches.len()
                        ));
                    }
                    let binding = first
                        .binding
                        .as_deref()
                        .map(|b| out.name(b))
                        .unwrap_or_else(|| "_".to_string());
                    out.line(&format!("if let Err({binding}) = {caught} {{"));
                    out.open();
                    rust_block(out, &first.body, None);
                    out.close();
                    out.line("}");
                    if !finally.is_empty() {
                        rust_block(out, finally, None);
                    }
                }
            }
            Stmt::Throw(value) => {
                let rendered = match value {
                    Expr::Str(_) => format!("{}.to_string()", rust_expr(out, value)),
                    other => rust_expr(out, other),
                };
                out.line(&format!("return Err({rendered});"));
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                out.line(&line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

/// Whether a `/` operand rules out arithmetic: a string on either side.
fn divides_a_string(out: &Out, left: &Expr, right: &Expr) -> bool {
    let stringish =
        |e: &Expr| matches!(e, Expr::Str(_)) || static_type(out, e) == Some(Type::String);
    stringish(left) || stringish(right)
}

/// The Rust type a literal constant says on its own.
fn rust_literal_type(value: &Expr) -> Option<String> {
    Some(match value {
        Expr::Int(_) => "i64".to_string(),
        Expr::Float(_) => "f64".to_string(),
        Expr::Str(_) => "&str".to_string(),
        Expr::Bool(_) => "bool".to_string(),
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
        } => rust_literal_type(operand)?,
        Expr::ListLit(items) => {
            let inner = rust_literal_type(items.first()?)?;
            let same = items
                .iter()
                .all(|i| rust_literal_type(i).as_deref() == Some(inner.as_str()));
            if !same {
                return None;
            }
            format!("&[{inner}]")
        }
        _ => return None,
    })
}

/// The `: T` on a binding, where Rust lets a binding carry one.
fn rust_binding_annotation(ty: &Option<Type>) -> String {
    match ty {
        Some(Type::Fn { .. }) | None => String::new(),
        Some(t) => format!(": {}", rust_type(t)),
    }
}

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        Type::String => "String".to_string(),
        Type::List(inner) => format!("Vec<{}>", rust_type(inner)),
        Type::Set(inner) => format!("std::collections::HashSet<{}>", rust_type(inner)),
        Type::Map(k, v) => format!(
            "std::collections::HashMap<{}, {}>",
            rust_type(k),
            rust_type(v)
        ),
        // A parameter position wants `impl Fn`; a field or a binding wants a boxed one.
        Type::Fn { params, returns } => format!(
            "impl Fn({}) -> {}",
            joined(params, rust_type),
            rust_type(returns)
        ),
        Type::Optional(inner) => format!("Option<{}>", rust_type(inner)),
        // `(A,)` and not `(A)`: without the comma the parentheses are grouping.
        Type::Tuple(parts) => match parts.as_slice() {
            [one] => format!("({},)", rust_type(one)),
            _ => format!("({})", joined(parts, rust_type)),
        },
        Type::Named { name, args } => generic(name, args, "<", ">", "::", rust_type),
    }
}

/// One side of a binary expression, bracketed when the enclosing operator would bind into it.
fn binary_operand(text: String, operand: &Expr, enclosing: BinaryOp, on_the_right: bool) -> String {
    let inner = match operand {
        Expr::Binary { op, .. } => op.precedence(),
        // A conditional binds looser than any operator in the table, so it always needs the
        // brackets.
        Expr::Ternary { .. } | Expr::Coalesce { .. } => 0,
        _ => return text,
    };
    let outer = enclosing.precedence();
    // A comparison inside a comparison is bracketed whatever the table says.
    let compares = |op: BinaryOp| {
        matches!(
            op,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
        )
    };
    if let Expr::Binary { op, .. } = operand {
        if compares(*op) && compares(enclosing) {
            return format!("({text})");
        }
    }
    match inner < outer || (on_the_right && inner == outer) {
        true => format!("({text})"),
        false => text,
    }
}

/// The operand of `!` or `-`, bracketed when it is not a single thing.
fn unary_operand(text: String, operand: &Expr) -> String {
    match operand {
        Expr::Binary { .. } | Expr::Ternary { .. } | Expr::Coalesce { .. } => {
            format!("({text})")
        }
        _ => text,
    }
}

/// The receiver of a `.field` or a `[index]`, bracketed when the reach would bind into it.
fn receiver(text: String, of: &Expr) -> String {
    match of {
        Expr::Binary { .. }
        | Expr::Ternary { .. }
        | Expr::Coalesce { .. }
        | Expr::Unary { .. }
        | Expr::Await(_)
        | Expr::Lambda { .. }
        | Expr::Int(_)
        | Expr::Float(_) => format!("({text})"),
        _ => text,
    }
}

/// A construction target is a path, and a dotted name from another language
/// walks that path with `::`.
fn rust_path(out: &mut Out, callee: &Expr) -> String {
    fn dotted(e: &Expr) -> Option<String> {
        match e {
            Expr::Name(name) => Some(name.replace('.', "::")),
            Expr::Field { of, name } => Some(format!("{}::{name}", dotted(of)?)),
            _ => None,
        }
    }
    match dotted(callee) {
        Some(path) => path,
        None => rust_expr(out, callee),
    }
}

fn rust_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), rust_expr(out, value)))
                .collect();
            match rendered.is_empty() {
                true => format!("{} {{}}", out.name(ty)),
                false => format!("{} {{ {} }}", out.name(ty), rendered.join(", ")),
            }
        }
        // `Option::unwrap_or`, which a Rust reader expects and what the IR's
        // `Optional` becomes.
        Expr::Coalesce { value, fallback } => format!(
            "{}.unwrap_or({})",
            rust_expr(out, value),
            rust_expr(out, fallback)
        ),
        // Rust's `if` is an expression already.
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "if {} {{ {} }} else {{ {} }}",
            rust_expr(out, condition),
            rust_expr(out, then),
            rust_expr(out, otherwise)
        ),
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => quoted(Language::Rust, v),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            // A caller reaches a sum's variant through the type, and Rust spells that reach
            // `::`.
            if matches!(of.as_ref(), Expr::Name(n) if out.sums.contains(n)) {
                let owner = rust_expr(out, of);
                return format!("{owner}::{}", out.name(name));
            }
            let object = receiver(rust_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the parentheses does not
            // exist in this language.
            if out.properties.contains(name) {
                return format!("{object}.{}()", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            format!(
                "{}[{}]",
                receiver(rust_expr(out, of), of),
                rust_expr(out, index)
            )
        }
        Expr::Call { callee, args } => {
            if reaches_super(callee) && !shadows_builtin(out, "super") {
                let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
                let source = super_source(callee, &rendered);
                out.carried(&Unsupported {
                    construct: "super".into(),
                    source: source.clone(),
                    line: 0,
                });
                return format!("todo!(/* {MARKER}: {} */)", source.replace("*/", "* /"));
            }
            if let Some(mapped) = rust_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            if let Some(filler) = carried_keywords(out, callee, args, settled.is_some()) {
                return filler;
            }
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let failing = matches!(callee.as_ref(), Expr::Name(n)
                if out.throwing.contains(n.as_str()));
            let suffix = match (failing, out.can_propagate) {
                (true, true) => "?",
                (true, false) => ".expect(\"unhandled failure\")",
                (false, _) => "",
            };
            let mut rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            // An integer literal where the signature declared a float: Go and the dynamic
            // sources coerce, Rust refuses.
            if let Expr::Name(name) = callee.as_ref() {
                if let Some(param_types) = out.function_param_types.get(name.as_str()) {
                    for (at, arg) in args.iter().enumerate() {
                        let float = matches!(param_types.get(at), Some(Some(Type::Float)));
                        if float && matches!(arg, Expr::Int(_) | Expr::Unary { .. }) {
                            if let Some(text) = rendered.get_mut(at) {
                                if !text.contains('.')
                                    && text.chars().all(|c| c.is_ascii_digit() || c == '-')
                                {
                                    text.push_str(".0");
                                }
                            }
                        }
                        // A literal where the signature declared `String` is a `&str`
                        // everywhere else and a type error here.
                        let string = matches!(param_types.get(at), Some(Some(Type::String)));
                        if string && matches!(arg, Expr::Str(_)) {
                            if let Some(text) = rendered.get_mut(at) {
                                text.push_str(".to_string()");
                            }
                        }
                    }
                }
            }
            // Build the struct: `Point(0, 0)` constructs, and naming the fields is the
            // only spelling that compiles here.
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = rust_expr(out, callee);
                let pairs = record_pairs(out, &fields, &rendered);
                return format!("{target} {{ {} }}", pairs.join(", "));
            }
            format!(
                "{}({}){suffix}",
                rust_expr(out, callee),
                rendered.join(", ")
            )
        }
        // Floor division rounds toward negative infinity, and Rust has no integer method that
        // does.
        Expr::Binary {
            op: BinaryOp::TrueDiv,
            left,
            right,
        } => {
            if divides_a_string(out, left, right) {
                let rendered = format!("{} / {}", rust_expr(out, left), rust_expr(out, right))
                    .replace("*/", "* /");
                out.carried(&Unsupported {
                    construct: "`/` on a non-number".into(),
                    source: rendered.clone(),
                    line: 0,
                });
                return format!("todo!(/* {MARKER}: {rendered} */)");
            }
            let side = |out: &mut Out, e: &Expr| {
                // A written number takes the float spelling.
                if let Expr::Int(n) = e {
                    return format!("{n}.0");
                }
                let text = binary_operand(rust_expr(out, e), e, BinaryOp::Div, false);
                match static_type(out, e) {
                    Some(Type::Float) => text,
                    _ => format!("{text} as f64"),
                }
            };
            let dividend = side(out, left);
            let divisor = side(out, right);
            format!("{dividend} / {divisor}")
        }
        Expr::Binary {
            op: BinaryOp::FloorDiv,
            left,
            right,
        } => {
            let dividend = rust_expr(out, left);
            let divisor = rust_expr(out, right);
            // A float divides without truncating, so its floor is the method.
            if static_type(out, left) == Some(Type::Float)
                || static_type(out, right) == Some(Type::Float)
            {
                let target = receiver(
                    format!(
                        "{} / {}",
                        binary_operand(dividend, left, BinaryOp::Div, false),
                        binary_operand(divisor, right, BinaryOp::Div, true)
                    ),
                    &Expr::Binary {
                        op: BinaryOp::Div,
                        left: left.clone(),
                        right: right.clone(),
                    },
                );
                return format!("{target}.floor()");
            }
            out.needs_floor_div = true;
            format!("{}({dividend}, {divisor})", floor_div_name(out))
        }
        // The remainder that goes with that division.
        Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        } => {
            let dividend = rust_expr(out, left);
            let divisor = rust_expr(out, right);
            out.zig_helpers.insert("rust_floor_rem");
            format!("fr_floor_rem({dividend}, {divisor})")
        }
        Expr::Binary { op, left, right } => {
            // `n <= 0` with `n: f64`: Go and the others coerce the untyped literal, Rust
            // refuses the comparison.
            fn float_side(out: &Out, e: &Expr) -> bool {
                matches!(static_type(out, e), Some(Type::Float))
            }
            fn rendered(out: &mut Out, e: &Expr, other: &Expr) -> String {
                let floats = matches!(e, Expr::Int(_)) && float_side(out, other);
                let text = rust_expr(out, e);
                match floats {
                    true => format!("{text}.0"),
                    false => text,
                }
            }
            let left_text = rendered(out, left, right);
            let right_text = rendered(out, right, left);
            format!(
                "{} {} {}",
                binary_operand(left_text, left, *op, false),
                op.c_like(),
                binary_operand(right_text, right, *op, true)
            )
        }
        // Standard Rust has async syntax but no executor.
        Expr::Await(inner) => {
            out.note_once(
                "an `await` runs blocking here: standard Rust has no executor to suspend on.",
            );
            rust_expr(out, inner)
        }
        // `try (check(n) + 1)` propagates the failure of the call inside.
        Expr::Propagate(inner) => match contains_failing_call(out, inner) {
            true => rust_expr(out, inner),
            false => format!("{}?", rust_expr(out, inner)),
        },
        // Rust has no universal spelling for construction: `X::new`, `X { ..
        Expr::New { callee, args } => {
            // A construction whose arguments already name their fields is a struct literal as
            // written: `Point{}` and `Circle{Radius: n}` arrive this way from Go.
            let keywords: Option<Vec<(&String, &Expr)>> = args
                .iter()
                .map(|a| match a {
                    Expr::Keyword { name, value } => Some((name, value.as_ref())),
                    _ => None,
                })
                .collect();
            if let Some(pairs) = &keywords {
                let named = match callee.as_ref() {
                    Expr::Name(n) => out.records.contains_key(n),
                    _ => false,
                };
                if named && (args.is_empty() || !pairs.is_empty()) {
                    let target = rust_expr(out, callee);
                    let rendered: Vec<String> = pairs
                        .iter()
                        .map(|(field, value)| {
                            format!("{}: {}", out.field(field), rust_expr(out, value))
                        })
                        .collect();
                    return match rendered.is_empty() {
                        true => format!("{target} {{}}"),
                        false => format!("{target} {{ {} }}", rendered.join(", ")),
                    };
                }
            }
            // A construction whose arguments all name fields is a struct literal whatever the
            // type: the fields are the constructor.
            if let Some(pairs) = keywords {
                if !args.is_empty() {
                    let target = rust_path(out, callee);
                    let rendered: Vec<String> = pairs
                        .iter()
                        .map(|(field, value)| {
                            format!("{}: {}", out.field(field), rust_expr(out, value))
                        })
                        .collect();
                    return format!("{target} {{ {} }}", rendered.join(", "));
                }
            }
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = rust_expr(out, callee);
                let pairs = record_pairs(out, &fields, &rendered);
                return format!("{target} {{ {} }}", pairs.join(", "));
            }
            // A positional construction of a type this file does not declare:
            // the convention every Rust type spells its constructor by.
            let target = rust_path(out, callee);
            format!("{target}::new({})", rendered.join(", "))
        }
        // Rust asks this with a `match` on an enum or with `Any::downcast`.
        Expr::Cast { ty, value } => {
            format!("({} as {})", rust_expr(out, value), rust_expr(out, ty))
        }
        // Rust asks this of a type-erased value through `Any`; the downcast
        // probe is the language's own spelling of the question.
        Expr::InstanceOf { value, ty } => {
            let rendered = rust_expr(out, value);
            let named = rust_path(out, ty);
            format!("(&{rendered} as &dyn std::any::Any).is::<{named}>()")
        }
        Expr::Keyword { name: _, value } => {
            out.note_once(
                "a named argument passes by position here: the target does not name arguments.",
            );
            rust_expr(out, value)
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::Unwrap => {
                    return format!(
                        "{}.unwrap()",
                        unary_operand(rust_expr(out, operand), operand)
                    );
                }
            };
            format!("{sign}{}", unary_operand(rust_expr(out, operand), operand))
        }
        Expr::Variant { sum, name, fields } => {
            let owner = out.name(sum);
            let variant = out.name(name);
            match fields.is_empty() {
                true => format!("{owner}::{variant}"),
                false => {
                    let rendered = joined(fields, |(f, v)| {
                        format!("{}: {}", out.field(f), rust_expr(out, v))
                    });
                    format!("{owner}::{variant} {{ {rendered} }}")
                }
            }
        }
        Expr::Tuple(items) => match items.as_slice() {
            [one] => format!("({},)", rust_expr(out, one)),
            _ => format!("({})", joined(items, |i| rust_expr(out, i))),
        },
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| rust_expr(out, i)).collect();
            format!("vec![{}]", rendered.join(", "))
        }
        // A set built in place.
        Expr::SetLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| rust_expr(out, i)).collect();
            match rendered.is_empty() {
                true => "std::collections::HashSet::new()".to_string(),
                false => format!("std::collections::HashSet::from([{}])", rendered.join(", ")),
            }
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    // The literal fixes the key type. A borrowed key here makes the map
                    // a `HashMap<&str, _>`, and the next `insert` of an owned string
                    // does not compile.
                    let key = match k {
                        Expr::Str(_) => format!("{}.to_string()", rust_expr(out, k)),
                        _ => rust_expr(out, k),
                    };
                    format!("({key}, {})", rust_expr(out, v))
                })
                .collect();
            format!("std::collections::HashMap::from([{}])", rendered.join(", "))
        }
        Expr::Template(parts) => {
            // Rust interpolates by position, so the expressions become arguments.
            let mut literal = String::new();
            let mut args = Vec::new();
            for part in parts {
                match part {
                    TemplatePart::Text(text) => {
                        literal.push_str(&text.replace('{', "{{").replace('}', "}}"))
                    }
                    TemplatePart::Expr(e) => {
                        literal.push_str("{}");
                        args.push(rust_expr(out, e));
                    }
                }
            }
            if args.is_empty() {
                format!("{}.to_string()", quoted(Language::Rust, &literal))
            } else {
                format!(
                    "format!({}, {})",
                    quoted(Language::Rust, &literal),
                    args.join(", ")
                )
            }
        }
        Expr::Lambda { params, body, .. } => {
            let rendered: Vec<String> = params
                .iter()
                .map(|p| match &p.ty {
                    Some(t) => format!("{}: {}", out.name(&p.name), rust_type(t)),
                    None => out.name(&p.name),
                })
                .collect();
            // Give the body the parameters, the same way it knows a binding.
            let outer: Vec<(String, Option<Type>)> = params
                .iter()
                .map(|p| (p.name.clone(), out.binding_types.get(&p.name).cloned()))
                .collect();
            for p in params {
                match &p.ty {
                    Some(t) => {
                        out.binding_types.insert(p.name.clone(), t.clone());
                    }
                    None => {
                        out.binding_types.remove(&p.name);
                    }
                }
            }
            let value = rust_expr(out, body);
            for (name, held) in outer {
                match held {
                    Some(t) => out.binding_types.insert(name, t),
                    None => out.binding_types.remove(&name),
                };
            }
            format!("|{}| {value}", rendered.join(", "))
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let it = rust_expr(out, iterable);
            let name = out.name(binding);
            // `filter` hands the closure a reference, so a condition written against the
            // element compared a `&T` with a `T`.
            let filter = condition
                .as_ref()
                .map(|c| format!(".filter(|&{name}| {})", rust_expr(out, c)))
                .unwrap_or_default();
            // `collect` is generic over what it builds, and a bare one leaves the type to be
            // inferred from a later use.
            format!(
                "{it}.iter().cloned(){filter}.map(|{name}| {}).collect::<Vec<_>>()",
                rust_expr(out, element)
            )
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            // The carried text rides inside `todo!`'s format string, where a brace from the
            // source reads as a hole and stops the build.
            let payload = u
                .source
                .replace('"', "'")
                .replace('{', "{{")
                .replace('}', "}}");
            format!("todo!(\"{MARKER}: {payload}\")")
        }
    }
}

fn python(out: &mut Out, module: &Module) {
    if !module.doc.is_empty() {
        out.line("\"\"\"");
        for line in &module.doc {
            out.line(line);
        }
        out.line("\"\"\"");
        out.blank();
    }

    let needs_dataclass = module.items.iter().any(|i| match i {
        Item::Record(r) => !r.fields.is_empty(),
        Item::Sum(s) => s.variants.iter().any(|v| !v.fields.is_empty()),
        _ => false,
    });
    // A list or map default renders through `field(default_factory=...)`.
    let needs_factory = module.items.iter().any(|i| match i {
        Item::Record(r) => r
            .fields
            .iter()
            .any(|f| matches!(f.default, Some(Expr::ListLit(_)) | Some(Expr::MapLit(_)))),
        _ => false,
    });
    if needs_dataclass {
        match needs_factory {
            true => out.line("from dataclasses import dataclass, field"),
            false => out.line("from dataclasses import dataclass"),
        }
        out.blank();
    }
    if module.items.iter().any(|i| matches!(i, Item::Newtype(_))) {
        out.line("from typing import NewType");
        out.blank();
    }
    // `typing.Callable` is how this writer spells a function type, and an
    // annotation naming a module the file never imported raises on the way in.
    if module_mentions_a_function_type(module) {
        out.line("import typing");
        out.blank();
    }
    // The guarded entry may need a module of its own.
    let entry = module.items.iter().find_map(|item| match item {
        Item::Statement(stmt) => entry_function(module, stmt),
        _ => None,
    });
    if let Some(f) = entry {
        if f.is_async {
            out.line("import asyncio");
            out.blank();
        }
        if entry_takes_arguments(f) {
            out.line("import sys");
            out.blank();
        }
    }

    // A helper called from `main` has to be defined before the statement that runs it, and that
    // statement stands at the end of the file.
    if python_needs_truncating_remainder(module) {
        out.line("def fr_trunc_rem(dividend: int, divisor: int) -> int:");
        out.open();
        out.line("\"\"\"The remainder that goes with division truncating toward zero.");
        out.blank();
        out.line("Python's own `%` rounds with `//`, toward negative infinity, and the");
        out.line("two answer differently whenever the operands have different signs.");
        out.line("\"\"\"");
        out.line("remainder = dividend % divisor");
        out.line("if remainder != 0 and (remainder < 0) != (dividend < 0):");
        out.open();
        out.line("return remainder - divisor");
        out.close();
        out.line("return remainder");
        out.close();
        out.blank();
    }

    for item in &module.items {
        match item {
            // A comment is not part of the program, so it needs no guard; guarded,
            // its block would hold nothing and raise instead.
            Item::Statement(Stmt::Comment(text)) => {
                out.line(&format!("# {text}"));
                out.blank();
            }
            Item::Statement(stmt) => {
                out.line("if __name__ == \"__main__\":");
                out.open();
                match entry_function(module, stmt) {
                    // A Java `main(String[] args)` receives the program's arguments, which
                    // Python spells `sys.argv` less the interpreter's own name.
                    Some(f) => {
                        let name = out.name(&f.name);
                        let arguments = match entry_takes_arguments(f) {
                            true => "sys.argv[1:]",
                            false => "",
                        };
                        let call = match f.is_async {
                            true => format!("asyncio.run({name}({arguments}))"),
                            false => format!("{name}({arguments})"),
                        };
                        python_line(out, &call);
                    }
                    None => python_block(out, std::slice::from_ref(stmt)),
                }
                out.close();
                out.blank();
            }
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("# {line}"));
                }
                let annotation =
                    c.ty.as_ref()
                        .map(|t| format!(": {}", python_type(t)))
                        .unwrap_or_default();
                let value = python_expr(out, &c.value);
                let const_name = out.name(&c.name);
                out.line(&format!("{const_name}{annotation} = {value}"));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                // A record with fields is a dataclass; one with only methods is a
                // plain class, because `@dataclass` on it would say nothing.
                if !r.fields.is_empty() {
                    out.line("@dataclass");
                }
                let type_name = out.name(&r.name);
                // Python spells the base in parentheses after the name.
                let base = inherited_base(out, r, true)
                    .map(|base| match base.as_str() {
                        "Error" if !out.declared_types.contains("Error") => "Exception".to_string(),
                        _ => base,
                    })
                    .map(|base| format!("({base})"))
                    .unwrap_or_default();
                out.line(&format!("class {type_name}{base}:"));
                out.open();
                if !r.doc.is_empty() {
                    out.line("\"\"\"");
                    for line in &r.doc {
                        out.line(line);
                    }
                    out.line("\"\"\"");
                    out.blank();
                }
                for f in &r.fields {
                    for line in &f.doc {
                        out.line(&format!("# {line}"));
                    }
                    let annotation =
                        f.ty.as_ref()
                            .map(python_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_name = out.field(&f.name);
                    // A mutable default shared between instances is the classic Python trap,
                    // and the dataclass machinery refuses it outright.
                    let default = f.default.as_ref().map(|d| match d {
                        Expr::ListLit(items) if items.is_empty() => {
                            " = field(default_factory=list)".to_string()
                        }
                        Expr::MapLit(entries) if entries.is_empty() => {
                            " = field(default_factory=dict)".to_string()
                        }
                        Expr::ListLit(_) | Expr::MapLit(_) => {
                            format!(" = field(default_factory=lambda: {})", python_expr(out, d))
                        }
                        _ => format!(" = {}", python_expr(out, d)),
                    });
                    out.line(&format!(
                        "{field_name}: {annotation}{}",
                        default.unwrap_or_default()
                    ));
                }
                if r.fields.is_empty() && r.methods.is_empty() && r.doc.is_empty() {
                    out.line("pass");
                }
                for m in &methods_of(out, r, false) {
                    out.blank();
                    // A method with no receiver is not an instance method, and Python
                    // will pass it one anyway unless told otherwise.
                    if m.receiver_binding.is_none() {
                        out.line("@staticmethod");
                    }
                    if m.is_property {
                        out.line("@property");
                    }
                    python_function(out, m, m.receiver_binding.is_some());
                }
                out.close();
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                python_function(out, f, false);
                out.blank();
            }
            Item::Import { text, line, target } => {
                // An import a sweep resolved names a sibling translated beside this file, so it
                // crosses as a real import.
                if let Some((stem, names)) = sibling_import(target) {
                    let list: Vec<String> = names
                        .iter()
                        .map(|n| match &n.alias {
                            Some(alias) => format!("{} as {alias}", out.name(&n.name)),
                            None => out.name(&n.name),
                        })
                        .collect();
                    out.line(&format!("from .{stem} import {}", list.join(", ")));
                    out.blank();
                    continue;
                }
                out.fidelity.imports_listed += 1;
                let header = out.comment(&format!(
                    "the source imported this at line {line}; the equivalent here is \
                     yours to add"
                ));
                out.line(&header);
                for l in text.lines() {
                    let commented = out.comment(l);
                    out.line(&commented);
                }
                out.blank();
            }
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("# {line}"));
                }
                let name = out.name(&n.name);
                out.line(&format!(
                    "{name} = NewType(\"{name}\", {})",
                    python_type(&n.base)
                ));
                out.fidelity.newtypes += 1;
                out.blank();
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("# {line}"));
                }
                let slug = test_slug(name);
                let prefixed = match slug.starts_with("test") {
                    true => slug,
                    false => format!("test_{slug}"),
                };
                out.line(&format!("def {prefixed}():"));
                out.open();
                python_block(out, body);
                out.close();
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                // One class per variant and a union alias naming the choice.
                let names = hoisted_variant_names(out, module, s);
                for (variant, variant_name) in s.variants.iter().zip(&names) {
                    for line in &variant.doc {
                        out.line(&format!("# {line}"));
                    }
                    if !variant.fields.is_empty() {
                        out.line("@dataclass");
                    }
                    out.line(&format!("class {variant_name}:"));
                    out.open();
                    for f in &variant.fields {
                        for line in &f.doc {
                            out.line(&format!("# {line}"));
                        }
                        let annotation =
                            f.ty.as_ref()
                                .map(python_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        out.line(&format!("{field_name}: {annotation}"));
                    }
                    if variant.fields.is_empty() {
                        out.line("pass");
                    }
                    out.close();
                    out.blank();
                }
                for line in &s.doc {
                    out.line(&format!("# {line}"));
                }
                let type_name = out.name(&s.name);
                out.line(&format!("{type_name} = {}", names.join(" | ")));
                out.fidelity.sums += 1;
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

/// Does any body here take a remainder the source truncated?
fn python_needs_truncating_remainder(module: &Module) -> bool {
    fn in_expr(e: &Expr) -> bool {
        let mut e = e.clone();
        fn walk(e: &mut Expr) -> bool {
            if matches!(
                e,
                Expr::Binary {
                    op: BinaryOp::Rem,
                    ..
                }
            ) {
                return true;
            }
            subexpressions_mut(e).into_iter().any(walk)
        }
        walk(&mut e)
    }
    fn in_body(body: &[Stmt]) -> bool {
        let mut body = body.to_vec();
        body.iter_mut().any(|stmt| {
            statement_expressions_mut(stmt)
                .into_iter()
                .any(|e| in_expr(e))
                || sub_bodies_mut(stmt).into_iter().any(|inner| in_body(inner))
        })
    }
    module.items.iter().any(|item| match item {
        Item::Function(f) => in_body(&f.body),
        Item::Record(r) => r.methods.iter().any(|m| in_body(&m.body)),
        Item::Test { body, .. } => in_body(body),
        Item::Statement(stmt) => in_body(std::slice::from_ref(stmt)),
        _ => false,
    })
}

fn python_function(out: &mut Out, f: &Function, method: bool) {
    let mut deferred_defaults: Vec<(String, Expr)> = Vec::new();
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push(receiver_word(out.language).to_string());
    }
    let mut foreign = false;
    let mut unannotated = false;
    for p in &f.params {
        let annotation = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                format!(": {}", python_type(t))
            }
            None => {
                unannotated = true;
                String::new()
            }
        };
        let reads_a_parameter = p
            .default
            .as_ref()
            .is_some_and(|d| f.params.iter().any(|other| expr_reads(d, &other.name)));
        let default = match (&p.default, reads_a_parameter) {
            (Some(_), true) => " = None".to_string(),
            (Some(d), false) => format!(" = {}", python_expr(out, d)),
            (None, _) => String::new(),
        };
        // The sentinel widens the type it stands in for, and a checker rejects `width: float
        // = None`.
        let annotation = match reads_a_parameter && !annotation.is_empty() {
            true => format!("{} | None", annotation),
            false => annotation,
        };
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(spelled);
            continue;
        }
        if reads_a_parameter {
            if let Some(d) = &p.default {
                deferred_defaults.push((spelled.clone(), d.clone()));
            }
        }
        params.push(format!("{spelled}{annotation}{default}"));
    }
    let returns = match &f.returns {
        None => String::new(),
        Some(Type::Unit) => " -> None".to_string(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            format!(" -> {}", python_type(t))
        }
    };
    let prefix = if f.is_async { "async def" } else { "def" };
    out.line(&format!(
        "{prefix} {}({}){returns}:",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    if !f.doc.is_empty() {
        if f.doc.len() == 1 {
            out.line(&format!("\"\"\"{}\"\"\"", f.doc[0]));
        } else {
            out.line("\"\"\"");
            for line in &f.doc {
                out.line(line);
            }
            out.line("\"\"\"");
        }
    }
    for (name, value) in &deferred_defaults {
        out.line(&format!("if {name} is None:"));
        out.open();
        let rendered = python_expr(out, value);
        out.line(&format!("{name} = {rendered}"));
        out.close();
    }
    python_block(out, &f.body);
    out.close();

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// A line, preceded by anything an expression could not say where it stood.
fn python_line(out: &mut Out, text: &str) {
    let pending = std::mem::take(&mut out.pending);
    for note in pending {
        for line in note.lines() {
            let commented = out.comment(line);
            out.line(&commented);
        }
    }
    out.line(text);
}

fn python_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        python_line(out, "raise NotImplementedError");
        return;
    }
    let mut wrote = false;
    for (at, stmt) in body.iter().enumerate() {
        wrote |= !matches!(
            stmt,
            Stmt::Unsupported(_) | Stmt::Expr(Expr::Null) | Stmt::Comment(_)
        );
        match stmt {
            // No block scope here: the statements stand in place.
            Stmt::Block(stmts) => python_block(out, stmts),
            Stmt::LocalFunction(f) => python_function(out, f, false),
            Stmt::BreakWith { label, value } => {
                let rendered = value
                    .as_ref()
                    .map(|v| python_expr(out, v))
                    .unwrap_or_default();
                carry_labeled_break(out, label, &rendered);
            }
            Stmt::Return(value) => {
                // A returned Result speaks this language's own failure handling: the ok value
                // returns bare, and the Err raises.
                if let Some((ok, payload)) = value.as_ref().and_then(|v| result_call(out, v)) {
                    out.note_once(RESULT_RAISED);
                    match (ok, payload) {
                        (true, Some(p)) => {
                            let rendered = python_expr(out, p);
                            python_line(out, &format!("return {rendered}"));
                        }
                        (true, None) => python_line(out, "return"),
                        (false, payload) => {
                            let rendered = match payload {
                                Some(p) => match error_variant(out, p) {
                                    Some(variant) => quoted(Language::Python, variant),
                                    None => python_expr(out, p),
                                },
                                None => String::new(),
                            };
                            python_line(out, &format!("raise Exception({rendered})"));
                        }
                    }
                    continue;
                }
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", python_expr(out, v)))
                    .unwrap_or_default();
                python_line(out, &format!("return{text}"));
            }
            Stmt::Let {
                name, ty, value, ..
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", python_type(t)))
                    .unwrap_or_default();
                let v = value
                    .as_ref()
                    .map(|v| python_expr(out, v))
                    .unwrap_or_else(|| "None".to_string());
                let bound = out.name(name);
                python_line(out, &format!("{bound}{annotation} = {v}"));
            }
            Stmt::Assign { target, value } => {
                let t = python_expr(out, target);
                let v = python_expr(out, value);
                python_line(out, &format!("{t} = {v}"));
            }
            // Python declares nothing, so both forms are the one line.
            Stmt::TupleAssign { names, value, .. } => {
                let v = python_expr(out, value);
                let bound = joined(names, |n| out.name(n));
                python_line(out, &format!("{bound} = {v}"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = python_expr(out, condition);
                python_line(out, &format!("if {c}:"));
                out.open();
                python_block(out, then);
                out.close();
                if !otherwise.is_empty() {
                    // Fold `else: if ...` into `elif` where that is all it holds.
                    if otherwise.len() == 1 {
                        if let Stmt::If { .. } = &otherwise[0] {
                            python_line(out, "else:");
                            out.open();
                            python_block(out, otherwise);
                            out.close();
                            continue;
                        }
                    }
                    python_line(out, "else:");
                    out.open();
                    python_block(out, otherwise);
                    out.close();
                }
            }
            Stmt::IfPresent {
                binding,
                value,
                then,
                otherwise,
            } => {
                let v = python_expr(out, value);
                let bound = out.name(binding);
                python_line(out, &format!("{bound} = {v}"));
                python_line(out, &format!("if {bound} is not None:"));
                out.open();
                python_block(out, then);
                out.close();
                if !otherwise.is_empty() {
                    python_line(out, "else:");
                    out.open();
                    python_block(out, otherwise);
                    out.close();
                }
            }
            Stmt::MatchVariants {
                subject,
                arms,
                default,
                ..
            } => {
                let s = python_expr(out, subject);
                for (at, arm) in arms.iter().enumerate() {
                    let head = if at == 0 { "if" } else { "elif" };
                    out.line(&format!(
                        "{head} isinstance({s}, {}):",
                        out.name(&arm.variant)
                    ));
                    out.open();
                    for (field, local) in &arm.bindings {
                        out.line(&format!("{} = {s}.{}", out.name(local), out.field(field)));
                    }
                    if arm.bindings.is_empty() && arm.body.is_empty() {
                        out.line("pass");
                    }
                    python_block(out, &arm.body);
                    out.close();
                }
                if !default.is_empty() {
                    out.line("else:");
                    out.open();
                    python_block(out, default);
                    out.close();
                }
            }
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                let s = python_expr(out, subject);
                python_line(out, &format!("match {s}:"));
                out.open();
                for (literals, body) in arms {
                    let pattern: Vec<String> =
                        literals.iter().map(|l| python_expr(out, l)).collect();
                    python_line(out, &format!("case {}:", pattern.join(" | ")));
                    out.open();
                    python_block(out, body);
                    out.close();
                }
                if !default.is_empty() {
                    python_line(out, "case _:");
                    out.open();
                    python_block(out, default);
                    out.close();
                }
                out.close();
            }
            Stmt::Defer(cleanup) => {
                python_line(out, "try:");
                out.open();
                let rest = &body[at + 1..];
                if rest.is_empty() {
                    python_line(out, "pass");
                } else {
                    python_block(out, rest);
                }
                out.close();
                python_line(out, "finally:");
                out.open();
                python_block(out, cleanup);
                out.close();
                return;
            }
            // `errdefer` runs only on the failure path, and here failure is an
            // exception: clean up and let it keep flying.
            Stmt::ErrDefer(cleanup) => {
                python_line(out, "try:");
                out.open();
                let rest = &body[at + 1..];
                if rest.is_empty() {
                    python_line(out, "pass");
                } else {
                    python_block(out, rest);
                }
                out.close();
                python_line(out, "except BaseException:");
                out.open();
                python_block(out, cleanup);
                python_line(out, "raise");
                out.close();
                return;
            }
            Stmt::WhilePresent {
                binding,
                value,
                body,
            } => {
                let v = python_expr(out, value);
                let bound = out.name(binding);
                python_line(out, "while True:");
                out.open();
                python_line(out, &format!("{bound} = {v}"));
                python_line(out, &format!("if {bound} is None:"));
                out.open();
                python_line(out, "break");
                out.close();
                python_block(out, body);
                out.close();
            }
            Stmt::While { condition, body } => {
                let c = python_expr(out, condition);
                python_line(out, &format!("while {c}:"));
                out.open();
                python_block(out, body);
                out.close();
            }
            // Python has no counted header either, and says the same loop with
            // the start above it and the step at the foot of the body.
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                source,
                line,
            } => {
                if let Some((name, start, bound, step)) =
                    counted_range(init.as_deref(), condition.as_ref(), update.as_deref(), body)
                {
                    let (start, bound) = (python_expr(out, start), python_expr(out, bound));
                    let stepping = match step {
                        1 => String::new(),
                        other => format!(", {other}"),
                    };
                    let name = out.name(name);
                    python_line(
                        out,
                        &format!("for {name} in range({start}, {bound}{stepping}):"),
                    );
                    out.open();
                    python_block(out, body);
                    out.close();
                } else if update.is_some() && continues_here(body) {
                    carry(out, &counted_original(source, *line));
                } else {
                    if let Some(init) = init {
                        python_block(out, std::slice::from_ref(init.as_ref()));
                    }
                    let c = condition
                        .as_ref()
                        .map(|c| python_expr(out, c))
                        .unwrap_or_else(|| "True".to_string());
                    python_line(out, &format!("while {c}:"));
                    out.open();
                    python_block(out, body);
                    if let Some(update) = update {
                        python_block(out, std::slice::from_ref(update.as_ref()));
                    }
                    out.close();
                }
            }
            Stmt::ForEachIndexed {
                index,
                binding,
                iterable,
                body,
            } => {
                let it = python_expr(out, iterable);
                let i = out.name(index);
                let bound = out.name(binding);
                python_line(out, &format!("for {i}, {bound} in enumerate({it}):"));
                out.open();
                python_block(out, body);
                out.close();
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = python_expr(out, iterable);
                let bound = out.name(binding);
                python_line(out, &format!("for {bound} in {it}:"));
                out.open();
                python_block(out, body);
                out.close();
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = python_expr(out, e);
                python_line(out, &text);
            }
            Stmt::Assert { condition, message } => {
                let c = python_expr(out, condition);
                match message {
                    Some(m) => {
                        let rendered = python_expr(out, m);
                        python_line(out, &format!("assert {c}, {rendered}"));
                    }
                    None => python_line(out, &format!("assert {c}")),
                }
            }
            Stmt::Break => {
                python_line(out, "break");
            }
            Stmt::Continue => {
                python_line(out, "continue");
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                python_line(out, "try:");
                out.open();
                python_block(out, body);
                out.close();
                for clause in catches {
                    let selector = clause
                        .ty
                        .as_ref()
                        .map(python_type)
                        .unwrap_or_else(|| "Exception".to_string());
                    let bound = clause
                        .binding
                        .as_ref()
                        .map(|b| format!(" as {}", out.name(b)))
                        .unwrap_or_default();
                    python_line(out, &format!("except {selector}{bound}:"));
                    out.open();
                    python_block(out, &clause.body);
                    out.close();
                }
                if !finally.is_empty() {
                    python_line(out, "finally:");
                    out.open();
                    python_block(out, finally);
                    out.close();
                }
            }
            Stmt::Throw(value) => {
                // A bare message is not raisable here; the plainest exception class
                // carries it, and `str(e)` in every catch reads it back out.
                let rendered = match value {
                    Expr::Str(_) | Expr::Template(_) => {
                        format!("Exception({})", python_expr(out, value))
                    }
                    other => python_expr(out, other),
                };
                python_line(out, &format!("raise {rendered}"));
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                python_line(out, &line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
    // Give a body of carried comments a statement, or Python will not parse it.
    if !wrote {
        python_line(out, "raise NotImplementedError");
    }
}

/// Does any signature or field in this module carry a function type?
fn module_mentions_a_function_type(module: &Module) -> bool {
    fn in_type(ty: &Type) -> bool {
        match ty {
            Type::Fn { .. } => true,
            Type::List(inner) | Type::Optional(inner) => in_type(inner),
            Type::Map(k, v) => in_type(k) || in_type(v),
            Type::Tuple(parts) => parts.iter().any(in_type),
            Type::Named { args, .. } => args.iter().any(in_type),
            _ => false,
        }
    }
    fn in_function(f: &Function) -> bool {
        f.params.iter().any(|p| p.ty.as_ref().is_some_and(in_type))
            || f.returns.as_ref().is_some_and(in_type)
    }
    module.items.iter().any(|item| match item {
        Item::Function(f) => in_function(f),
        Item::Record(r) => {
            r.fields.iter().any(|f| f.ty.as_ref().is_some_and(in_type))
                || r.methods.iter().any(in_function)
        }
        Item::Constant(c) => c.ty.as_ref().is_some_and(in_type),
        _ => false,
    })
}

pub(super) fn python_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "None".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float".to_string(),
        Type::String => "str".to_string(),
        Type::List(inner) => format!("list[{}]", python_type(inner)),
        Type::Set(inner) => format!("set[{}]", python_type(inner)),
        Type::Tuple(parts) => format!("tuple[{}]", joined(parts, python_type)),
        Type::Map(k, v) => format!("dict[{}, {}]", python_type(k), python_type(v)),
        Type::Optional(inner) => format!("{} | None", python_type(inner)),
        Type::Fn { params, returns } => format!(
            "typing.Callable[[{}], {}]",
            joined(params, python_type),
            python_type(returns)
        ),
        // Keep the arguments apart: Python spells generics with brackets, and a literal
        // `Result<(), String>` is not an annotation.
        Type::Named { name, args } => generic(name, args, "[", "]", ".", python_type),
    }
}

/// The types this function's names hold: its parameters, and the locals it declares a type for.
fn declared_bindings(f: &Function) -> std::collections::BTreeMap<String, Type> {
    let mut types = std::collections::BTreeMap::new();
    for p in &f.params {
        if let Some(ty) = &p.ty {
            types.insert(p.name.clone(), ty.clone());
        }
    }
    // The whole body, nested blocks included.
    fn walk(stmts: &[Stmt], types: &mut std::collections::BTreeMap<String, Type>) {
        for stmt in stmts {
            match stmt {
                Stmt::Let {
                    name,
                    ty: None,
                    value: Some(value),
                    ..
                } => {
                    // No annotation, but a literal says what it is.
                    let inferred = match value {
                        Expr::Str(_) | Expr::Template(_) => Some(Type::String),
                        Expr::Int(_) => Some(Type::Int),
                        Expr::Float(_) => Some(Type::Float),
                        Expr::Bool(_) => Some(Type::Bool),
                        // A map literal says what it holds by holding it. Where an
                        // entry does not say, the nameless type stands for "a map, and
                        // the rest is not settled here". Every reader of this asks
                        // whether the binding holds a map, and none writes it out.
                        Expr::MapLit(entries) => {
                            let (key, value) = map_literal_types(entries);
                            Some(Type::Map(
                                Box::new(key.unwrap_or_else(|| Type::named(""))),
                                Box::new(value.unwrap_or_else(|| Type::named(""))),
                            ))
                        }
                        // A list literal says what it holds by holding it.
                        Expr::ListLit(items) => items
                            .first()
                            .and_then(literal_type_of)
                            .map(|element| Type::List(Box::new(element))),
                        // The canonical conversions and string methods answer text.
                        Expr::Call { callee, .. } => match callee.as_ref() {
                            Expr::Name(n) if n == "str" => Some(Type::String),
                            Expr::Name(n) if n == "len" || n == "int" => Some(Type::Int),
                            Expr::Field { name, .. }
                                if matches!(
                                    name.as_str(),
                                    "upper" | "lower" | "strip" | "join"
                                ) =>
                            {
                                Some(Type::String)
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(ty) = inferred {
                        types.entry(name.clone()).or_insert(ty);
                    }
                }
                Stmt::Let {
                    name, ty: Some(ty), ..
                } => {
                    types.insert(name.clone(), ty.clone());
                }
                Stmt::If {
                    then, otherwise, ..
                }
                | Stmt::IfPresent {
                    then, otherwise, ..
                } => {
                    walk(then, types);
                    walk(otherwise, types);
                }
                // A loop over a list of a known element type binds a name of that type, and the
                // body asks what it is.
                Stmt::ForEach {
                    binding,
                    iterable,
                    body,
                } => {
                    if let Expr::Name(over) = iterable {
                        if let Some(Type::List(element)) = types.get(over) {
                            let element = (**element).clone();
                            types.insert(binding.clone(), element);
                        }
                    }
                    walk(body, types);
                }
                Stmt::While { body, .. }
                | Stmt::WhilePresent { body, .. }
                | Stmt::ForEachIndexed { body, .. }
                | Stmt::Defer(body)
                | Stmt::ErrDefer(body)
                | Stmt::Block(body) => walk(body, types),
                // `for (int i = 0; ...)` declares the counter in the header,
                // and a body dividing by it asks what type it is.
                Stmt::CountedFor { init, body, .. } => {
                    if let Some(init) = init {
                        walk(std::slice::from_ref(init.as_ref()), types);
                    }
                    walk(body, types);
                }
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    walk(body, types);
                    for catch in catches {
                        walk(&catch.body, types);
                    }
                    walk(finally, types);
                }
                Stmt::Switch { arms, default, .. } => {
                    for (_, body) in arms {
                        walk(body, types);
                    }
                    walk(default, types);
                }
                _ => {}
            }
        }
    }
    walk(&f.body, &mut types);
    settle_empty_collections(f, &mut types);
    types
}

/// Type the unannotated bindings whose value is a call to a function with a declared return.
fn settle_inferred_bindings(f: &Function, out: &mut Out) {
    fn walk(body: &[Stmt], out: &mut Out) {
        for stmt in body {
            if let Stmt::Let {
                name,
                ty: None,
                value: Some(value),
                ..
            } = stmt
            {
                if !out.binding_types.contains_key(name) {
                    if let Some(told) = static_type(out, value) {
                        out.binding_types.insert(name.clone(), told);
                    }
                }
            }
            let mut stmt = stmt.clone();
            for inner in sub_bodies_mut(&mut stmt) {
                walk(inner, out);
            }
        }
    }
    walk(&f.body, out);
}

/// The element type of a set built empty and filled by adding.
fn settle_set_element_types(f: &Function, out: &mut Out) {
    fn sets(body: &[Stmt], names: &mut Vec<(String, bool)>) {
        for stmt in body {
            if let Stmt::Let {
                name,
                ty: None,
                value: Some(Expr::SetLit(items)),
                ..
            } = stmt
            {
                names.push((name.clone(), items.is_empty()));
            }
            for inner in sub_bodies(stmt) {
                sets(inner, names);
            }
        }
    }
    let mut names = Vec::new();
    sets(&f.body, &mut names);
    for (name, empty) in names {
        let element = match empty {
            false => None,
            true => {
                let mut found = Vec::new();
                given_to(&f.body, &name, "add", &mut found);
                found
                    .first()
                    .and_then(|first| static_type(out, first))
                    .or_else(|| found.first().and_then(|first| literal_type_of(first)))
            }
        };
        if let Some(element) = element {
            out.binding_types.insert(name, Type::Set(Box::new(element)));
        }
    }
}

fn settle_list_element_types(f: &Function, out: &mut Out) {
    fn empty_lists(body: &[Stmt], names: &mut Vec<String>) {
        for stmt in body {
            if let Stmt::Let {
                name,
                ty: None,
                value: Some(Expr::ListLit(items)),
                ..
            } = stmt
            {
                if items.is_empty() {
                    names.push(name.clone());
                }
            }
            for inner in sub_bodies(stmt) {
                empty_lists(inner, names);
            }
        }
    }
    let mut names = Vec::new();
    empty_lists(&f.body, &mut names);
    for name in names {
        let mut values = Vec::new();
        given_to(&f.body, &name, "append", &mut values);
        let mut settled: Option<Type> = None;
        for value in values {
            match (static_type(out, value), &settled) {
                (Some(ty), None) => settled = Some(ty),
                (Some(ty), Some(first)) if ty == *first => {}
                // Appends that disagree describe no one element type, and a
                // guess would be worse than the widest type.
                _ => {
                    settled = None;
                    break;
                }
            }
        }
        if let Some(ty) = settled {
            out.binding_types.insert(name, Type::List(Box::new(ty)));
        }
    }
}

fn settle_call_bindings(
    f: &Function,
    returns: &std::collections::BTreeMap<String, Type>,
    types: &mut std::collections::BTreeMap<String, Type>,
) {
    each_stmt(&f.body, &mut |stmt| {
        if let Stmt::Let {
            name,
            ty: None,
            value: Some(value),
            ..
        } = stmt
        {
            let mut value = value;
            while let Expr::Await(inner) | Expr::Propagate(inner) = value {
                value = inner;
            }
            if let Expr::Call { callee, .. } = value {
                if let Expr::Name(called) = callee.as_ref() {
                    if let Some(ty) = returns.get(called.as_str()) {
                        types.entry(name.clone()).or_insert_with(|| ty.clone());
                    }
                }
            }
        }
    });
}

/// Whether this method's body assigns to a field of its receiver.
fn assigns_to_receiver(f: &Function, word: &str) -> bool {
    fn is_receiver_field(target: &Expr, word: &str) -> bool {
        match target {
            Expr::Field { of, .. } => matches!(of.as_ref(), Expr::Name(n) if n == word),
            Expr::Index { of, .. } => is_receiver_field(of, word),
            _ => false,
        }
    }
    let mut found = false;
    each_stmt(&f.body, &mut |stmt| match stmt {
        Stmt::Assign { target, .. } => found |= is_receiver_field(target, word),
        Stmt::TupleAssign { .. } => {}
        _ => {}
    });
    found
}

/// Visit every statement in a body, nested blocks included.
fn each_stmt(stmts: &[Stmt], visit: &mut dyn FnMut(&Stmt)) {
    for stmt in stmts {
        visit(stmt);
        match stmt {
            Stmt::If {
                then, otherwise, ..
            }
            | Stmt::IfPresent {
                then, otherwise, ..
            } => {
                each_stmt(then, visit);
                each_stmt(otherwise, visit);
            }
            Stmt::While { body, .. }
            | Stmt::WhilePresent { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachIndexed { body, .. }
            | Stmt::Defer(body)
            | Stmt::ErrDefer(body)
            | Stmt::Block(body) => each_stmt(body, visit),
            Stmt::CountedFor { init, body, .. } => {
                if let Some(init) = init {
                    each_stmt(std::slice::from_ref(init.as_ref()), visit);
                }
                each_stmt(body, visit);
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                each_stmt(body, visit);
                for catch in catches {
                    each_stmt(&catch.body, visit);
                }
                each_stmt(finally, visit);
            }
            Stmt::Switch { arms, default, .. } => {
                for (_, body) in arms {
                    each_stmt(body, visit);
                }
                each_stmt(default, visit);
            }
            _ => {}
        }
    }
}

/// Give a local that starts as an empty list the element type it goes on to hold.
fn settle_empty_collections(f: &Function, types: &mut std::collections::BTreeMap<String, Type>) {
    fn empty_lists(stmts: &[Stmt], found: &mut Vec<String>) {
        each_stmt(stmts, &mut |stmt| {
            if let Stmt::Let {
                name,
                ty: None,
                value: Some(Expr::ListLit(items)),
                ..
            } = stmt
            {
                if items.is_empty() {
                    found.push(name.clone());
                }
            }
        });
    }

    fn appended(stmts: &[Stmt], target: &str, found: &mut Vec<Expr>) {
        each_stmt(stmts, &mut |stmt| {
            let Stmt::Expr(Expr::Call { callee, args }) = stmt else {
                return;
            };
            if let (Some(of), Some("append")) = callee_parts(callee) {
                if matches!(of, Expr::Name(n) if n == target) {
                    if let [x] = args.as_slice() {
                        found.push(x.clone());
                    }
                }
            }
        });
    }

    let mut empties = Vec::new();
    empty_lists(&f.body, &mut empties);
    for name in empties {
        if types.contains_key(&name) {
            continue;
        }
        let mut elements = Vec::new();
        appended(&f.body, &name, &mut elements);
        let element = elements.iter().find_map(literal_type).or_else(|| {
            // Nothing appended to it in a shape this can read.
            match (&f.returns, returns_name(&f.body, &name)) {
                (Some(Type::List(item)), true) => Some((**item).clone()),
                _ => None,
            }
        });
        if let Some(element) = element {
            types.insert(name, Type::List(Box::new(element)));
        }
    }
}

/// The type of an expression that carries its own, with no context to consult.
fn literal_type(e: &Expr) -> Option<Type> {
    match e {
        Expr::Int(_) => Some(Type::Int),
        Expr::Float(_) => Some(Type::Float),
        Expr::Str(_) => Some(Type::String),
        Expr::Bool(_) => Some(Type::Bool),
        Expr::New { callee, .. } => match callee.as_ref() {
            Expr::Name(name) => Some(Type::Named {
                name: name.clone(),
                args: Vec::new(),
            }),
            _ => None,
        },
        Expr::RecordLit { ty, .. } => Some(Type::Named {
            name: ty.clone(),
            args: Vec::new(),
        }),
        _ => None,
    }
}

/// Whether the body returns this binding by name.
fn returns_name(stmts: &[Stmt], name: &str) -> bool {
    let mut found = false;
    each_stmt(stmts, &mut |stmt| {
        if let Stmt::Return(Some(Expr::Name(n))) = stmt {
            found |= n == name;
        }
    });
    found
}

/// What this expression holds, as far as the source said so.
fn static_type(out: &Out, e: &Expr) -> Option<Type> {
    match e {
        Expr::SetLit(items) => items
            .first()
            .and_then(|first| static_type(out, first))
            .map(|element| Type::Set(Box::new(element))),
        Expr::Int(_) => Some(Type::Int),
        Expr::Float(_) => Some(Type::Float),
        Expr::Str(_) => Some(Type::String),
        Expr::Bool(_) => Some(Type::Bool),
        // A bare name in a method body is a local, a parameter, or a field of the record the
        // method belongs to.
        Expr::Name(name) => out
            .binding_types
            .get(name)
            .or_else(|| out.field_types.get(name))
            .cloned(),
        Expr::Propagate(inner) | Expr::Await(inner) => static_type(out, inner),
        // `+` with a string on either side is concatenation, and the whole of it is a string
        // however the other side is typed.
        Expr::Call { callee, args } => match (callee.as_ref(), args.len()) {
            (Expr::Name(name), 1) if name == "len" => Some(Type::Int),
            (Expr::Name(name), 1) if name == "str" => Some(Type::String),
            (Expr::Name(name), 1) if name == "int" => Some(Type::Int),
            (Expr::Name(name), 1) if name == "trunc" => static_type(out, &args[0]),
            (Expr::Name(name), 1) if name == "float" => Some(Type::Float),
            (Expr::Name(name), 1) if name == "bool" => Some(Type::Bool),
            // Otherwise a call's static type is the callee's declared return.
            (Expr::Name(f), _) => out.function_returns.get(f.as_str()).cloned(),
            // `b.get()` answers what `get` declares.
            (Expr::Field { name, .. }, _) => out.function_returns.get(name.as_str()).cloned(),
            _ => None,
        },
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } => {
            let (left, right) = (static_type(out, left), static_type(out, right));
            if left.as_ref() == Some(&Type::String) || right.as_ref() == Some(&Type::String) {
                return Some(Type::String);
            }
            let left = left?;
            (left == right?).then_some(left)
        }
        // Arithmetic keeps the type of its operands where both agree.
        Expr::Binary {
            op:
                BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::FloorDiv
                | BinaryOp::Rem
                | BinaryOp::FloorRem
                | BinaryOp::Div,
            left,
            right,
        } => {
            let left = static_type(out, left)?;
            (left == static_type(out, right)?).then_some(left)
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
        } => static_type(out, operand),
        _ => None,
    }
}

fn holds_an_integer(out: &Out, e: &Expr) -> bool {
    static_type(out, e) == Some(Type::Int)
}

/// Is either side of a comparison a string the source declared?
fn compares_strings(out: &Out, left: &Expr, right: &Expr) -> bool {
    static_type(out, left) == Some(Type::String) || static_type(out, right) == Some(Type::String)
}

fn python_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // A dataclass takes its fields as keyword arguments, in any order.
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}={}", out.field(name), python_expr(out, value)))
                .collect();
            format!("{}({})", out.name(ty), rendered.join(", "))
        }
        Expr::Coalesce { value, fallback } => match nameable(value) {
            true => format!(
                "{} if {} is not None else {}",
                python_expr(out, value),
                python_expr(out, value),
                python_expr(out, fallback)
            ),
            false => {
                out.lowering_names += 1;
                let bound = format!("fr_opt_{}", out.lowering_names);
                format!(
                    "{bound} if ({bound} := {}) is not None else {}",
                    python_expr(out, value),
                    python_expr(out, fallback)
                )
            }
        },
        // Python puts the condition in the middle, which is the only thing that
        // differs.
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "{} if {} else {}",
            python_expr(out, then),
            python_expr(out, condition),
            python_expr(out, otherwise)
        ),
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => if *v { "True" } else { "False" }.to_string(),
        Expr::Str(v) => quoted(Language::Python, v),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            // A member reached through `super` travels through the call this language spells
            // the reach with.
            if matches!(of.as_ref(), Expr::Name(n) if n == "super")
                && !shadows_builtin(out, "super")
            {
                return format!("super().{}", out.field(name));
            }
            let object = receiver(python_expr(out, of), of);
            // A property read stays a read here, and a property is a method:
            // its spelling lives in the method namespace, not the field one.
            if out.properties.contains(name) {
                return format!("{object}.{}", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            format!(
                "{}[{}]",
                receiver(python_expr(out, of), of),
                python_expr(out, index)
            )
        }
        Expr::Call { callee, args } => {
            // The canonical containment is this language's own operator.
            if let (Expr::Field { of, name }, [needle]) = (callee.as_ref(), args.as_slice()) {
                if name == "contains" {
                    return format!(
                        "{} in {}",
                        python_expr(out, needle),
                        python_expr(out, &of.clone())
                    );
                }
            }
            // The canonical `trunc` cuts toward zero, and so does `int`.
            if let (Expr::Name(name), [x]) = (callee.as_ref(), args.as_slice()) {
                if name == "trunc" {
                    return format!("int({})", python_expr(out, &x.clone()));
                }
            }
            // The unsigned right shift, through the 64-bit unsigned view.
            if let (Expr::Name(name), [x, n]) = (callee.as_ref(), args.as_slice()) {
                if name == "ushr" {
                    return format!(
                        "((({}) & 0xFFFFFFFFFFFFFFFF) >> ({}))",
                        python_expr(out, x),
                        python_expr(out, n)
                    );
                }
            }
            // The canonical slice is this language's own subscript.
            if let (Expr::Name(name), [of, from, to]) = (callee.as_ref(), args.as_slice()) {
                if name == "slice" {
                    return format!(
                        "{}[{}:{}]",
                        receiver(python_expr(out, of), of),
                        python_expr(out, from),
                        python_expr(out, to)
                    );
                }
            }
            let rendered: Vec<String> = args.iter().map(|a| python_expr(out, a)).collect();
            // The canonical call to `super` is the base constructor, which this
            // language spells `super().__init__`.
            if matches!(callee.as_ref(), Expr::Name(n) if n == "super")
                && !shadows_builtin(out, "super")
            {
                return format!("super().__init__({})", rendered.join(", "));
            }
            format!("{}({})", python_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => {
            // Python compares against None with `is`, not `==`.
            let against_none = matches!(**right, Expr::Null) || matches!(**left, Expr::Null);
            let spelling = match (op, against_none) {
                (BinaryOp::Eq, true) => "is",
                (BinaryOp::Ne, true) => "is not",
                (other, _) => other.python(),
            };
            // Every other language here truncates when it divides two integers.
            if *op == BinaryOp::Rem && holds_an_integer(out, left) && holds_an_integer(out, right) {
                out.zig_helpers.insert("python_trunc_rem");
                return format!(
                    "fr_trunc_rem({}, {})",
                    python_expr(out, left),
                    python_expr(out, right)
                );
            }
            if *op == BinaryOp::Div && holds_an_integer(out, left) && holds_an_integer(out, right) {
                return format!(
                    "int({} / {})",
                    binary_operand(python_expr(out, left), left, *op, false),
                    binary_operand(python_expr(out, right), right, *op, true)
                );
            }
            // `"n: " + x` concatenates in TypeScript and Java by turning the number into text;
            // Python raises instead.
            if *op == BinaryOp::Add {
                let text = |e: &Expr| static_type(out, e) == Some(Type::String);
                let number =
                    |e: &Expr| matches!(static_type(out, e), Some(Type::Int | Type::Float));
                if text(left) && number(right) {
                    return format!(
                        "{} + str({})",
                        binary_operand(python_expr(out, left), left, *op, false),
                        python_expr(out, right)
                    );
                }
                if number(left) && text(right) {
                    return format!(
                        "str({}) + {}",
                        python_expr(out, left),
                        binary_operand(python_expr(out, right), right, *op, true)
                    );
                }
            }
            format!(
                "{} {spelling} {}",
                binary_operand(python_expr(out, left), left, *op, false),
                binary_operand(python_expr(out, right), right, *op, true)
            )
        }
        Expr::Await(inner) => format!("await {}", python_expr(out, inner)),
        Expr::Propagate(inner) => {
            out.note_once(
                "a `?`/`try` crosses as the bare expression: an error here \
                 propagates on its own.",
            );
            python_expr(out, inner)
        }
        // Construction in Python is a call.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| python_expr(out, a)).collect();
            format!("{}({})", python_expr(out, callee), rendered.join(", "))
        }
        Expr::Cast { value, .. } => python_expr(out, value),
        Expr::InstanceOf { value, ty } => {
            let rendered = python_expr(out, value);
            format!("isinstance({rendered}, {})", python_expr(out, ty))
        }
        Expr::Keyword { name, value } => {
            let rendered = python_expr(out, value);
            format!("{name}={rendered}")
        }
        Expr::Unary { op, operand } => {
            let rendered = unary_operand(python_expr(out, operand), operand);
            match op {
                UnaryOp::Not => format!("not {rendered}"),
                UnaryOp::Neg => format!("-{rendered}"),
                // The value stands for itself: using None where a value was
                // promised raises at the same spot the source would trap.
                UnaryOp::Unwrap => python_expr(out, operand),
            }
        }
        // A variant is its own dataclass here, and the sum only an alias over
        // them, so the variant's constructor is the whole spelling.
        Expr::Variant { sum, name, fields } => {
            let rendered = joined(fields, |(f, v)| {
                format!("{}={}", out.field(f), python_expr(out, v))
            });
            format!("{}({rendered})", variant_spelling(out, sum, name))
        }
        // `(a,)` and not `(a)`: without the comma the parentheses are grouping.
        Expr::Tuple(items) => match items.as_slice() {
            [one] => format!("({},)", python_expr(out, one)),
            _ => format!("({})", joined(items, |i| python_expr(out, i))),
        },
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| python_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        // Spell an empty set `set()`: `{}` means an empty dict.
        Expr::SetLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| python_expr(out, i)).collect();
            match rendered.is_empty() {
                true => "set()".to_string(),
                false => format!("{{{}}}", rendered.join(", ")),
            }
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", python_expr(out, k), python_expr(out, v)))
                .collect();
            format!("{{{}}}", rendered.join(", "))
        }
        // An f-string quotes its text and leaves its expressions as code.
        Expr::Template(parts) => {
            let mut rendered: Vec<(bool, String)> = Vec::new();
            for part in parts {
                match part {
                    TemplatePart::Text(text) => rendered.push((false, text.clone())),
                    TemplatePart::Expr(e) => {
                        let text = python_expr(out, e);
                        rendered.push((true, text));
                    }
                }
            }
            // Before 3.12 an f-string's expression may contain neither a backslash nor the
            // quote that delimits the literal.
            let expressible = rendered
                .iter()
                .all(|(is_expr, text)| !is_expr || !text.contains(['"', '\\']));
            if expressible {
                let mut body = String::new();
                for (is_expr, text) in &rendered {
                    match is_expr {
                        true => {
                            body.push('{');
                            body.push_str(text);
                            body.push('}');
                        }
                        false => body.push_str(
                            &escaped(Language::Python, text)
                                .replace('{', "{{")
                                .replace('}', "}}"),
                        ),
                    }
                }
                return format!("f\"{body}\"");
            }
            rendered
                .iter()
                .map(|(is_expr, text)| match is_expr {
                    true => format!("str({text})"),
                    false => quoted(Language::Python, text),
                })
                .collect::<Vec<_>>()
                .join(" + ")
        }
        // A Python lambda takes no annotations, so a typed parameter crosses by name.
        Expr::Lambda { params, body, .. } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(&p.name)).collect();
            match rendered.is_empty() {
                true => format!("lambda: {}", python_expr(out, body)),
                false => format!("lambda {}: {}", rendered.join(", "), python_expr(out, body)),
            }
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let name = out.name(binding);
            let guard = condition
                .as_ref()
                .map(|c| format!(" if {}", python_expr(out, c)))
                .unwrap_or_default();
            format!(
                "[{} for {name} in {}{guard}]",
                python_expr(out, element),
                python_expr(out, iterable)
            )
        }
        // Python has no inline comment, so the note cannot stand where the value did.
        Expr::Unsupported(u) => {
            out.carried(u);
            out.pending.push(format!("{MARKER}: {}", u.source));
            "None".to_string()
        }
    }
}

fn go(out: &mut Out, module: &Module) {
    let package = go_package(module);
    out.line(&format!("package {package}"));
    out.blank();
    // `func TestX(t *testing.T)` names a package the file must import itself.
    if module.items.iter().any(|i| matches!(i, Item::Test { .. })) {
        out.line("import \"testing\"");
        out.blank();
    }
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Statement(stmt) if calls_declared_main(out, stmt) => {
                out.note_once(ENTRY_DROPPED);
            }
            Item::Statement(stmt) => carried_statement(out, stmt, go_expr),
            Item::Constant(c) => {
                let name = out.name(&c.name);
                for line in &c.doc {
                    out.line(&format!("// {name} {line}"));
                }
                // Go's `const` holds compile-time scalars and nothing else: `const Docs =
                // []any{…}` and `const Root = Path(…)` both refuse to build.
                let keyword = match scalar_literal(&c.value) {
                    true => "const",
                    false => "var",
                };
                let value = match &c.value {
                    Expr::ListLit(items)
                        if !items.is_empty() && items.iter().all(|i| matches!(i, Expr::Str(_))) =>
                    {
                        format!(
                            "[]string{{{}}}",
                            items
                                .iter()
                                .map(|i| go_expr(out, i))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                    other => go_expr(out, other),
                };
                out.line(&format!("{keyword} {name} = {value}"));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                let name = out.name(&r.name);
                for line in &r.doc {
                    out.line(&format!("// {name} {line}"));
                }
                inherited_base(out, r, false);
                out.line(&format!("type {name} struct {{"));
                out.open();
                for f in &r.fields {
                    let ty =
                        f.ty.as_ref()
                            .map(go_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_name = out.field(&f.name);
                    out.line(&format!("{field_name} {ty}"));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();
                // Field defaults have no slot in a struct; the `New` constructor
                // is where Go keeps a record's starting values.
                if r.fields.iter().any(|f| f.default.is_some()) {
                    out.line(&format!("func New{name}() {name} {{"));
                    out.open();
                    let pairs: Vec<String> = r
                        .fields
                        .iter()
                        .filter_map(|f| {
                            f.default.as_ref().map(|value| {
                                format!("{}: {}", out.field(&f.name), go_expr(out, value))
                            })
                        })
                        .collect();
                    out.line(&format!("return {name}{{{}}}", pairs.join(", ")));
                    out.close();
                    out.line("}");
                    out.blank();
                }
                for m in &methods_of(out, r, false) {
                    go_function(
                        out,
                        m,
                        m.receiver_binding.is_some().then_some(name.as_str()),
                    );
                    out.blank();
                }
            }
            Item::Function(f) => {
                go_function(out, f, None);
                out.blank();
            }
            Item::Import { text, line, .. } => {
                out.fidelity.imports_listed += 1;
                let header = out.comment(&format!(
                    "the source imported this at line {line}; the equivalent here is \
                     yours to add"
                ));
                out.line(&header);
                for l in text.lines() {
                    let commented = out.comment(l);
                    out.line(&commented);
                }
                out.blank();
            }
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("// {line}"));
                }
                out.line(&format!("type {} {}", out.name(&n.name), go_type(&n.base)));
                out.fidelity.newtypes += 1;
                out.blank();
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("// {line}"));
                }
                out.line(&format!(
                    "func Test{}(t *testing.T) {{",
                    pascal(&test_slug(name))
                ));
                out.open();
                out.line("_ = t");
                out.in_test = true;
                go_block(out, body, None);
                out.in_test = false;
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                // Go has no closed choice.
                let name = out.name(&s.name);
                for line in &s.doc {
                    out.line(&format!("// {name} {line}"));
                }
                let marker = format!("is{}", pascal(&s.name));
                out.line(&format!("type {name} interface{{ {marker}() }}"));
                out.blank();
                let names = hoisted_variant_names(out, module, s);
                for (variant, variant_name) in s.variants.iter().zip(&names) {
                    for line in &variant.doc {
                        out.line(&format!("// {variant_name} {line}"));
                    }
                    out.line(&format!("type {variant_name} struct {{"));
                    out.open();
                    for f in &variant.fields {
                        let ty =
                            f.ty.as_ref()
                                .map(go_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        out.line(&format!("{field_name} {ty}"));
                    }
                    out.close();
                    out.line("}");
                    out.blank();
                    out.line(&format!("func ({variant_name}) {marker}() {{}}"));
                    out.blank();
                }
                out.fidelity.sums += 1;
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }

    // The packages the body needs, inserted under the package clause where Go requires them,
    if out.zig_helpers.contains("go_floor_rem") {
        out.blank();
        out.line("// The remainder that goes with division rounding toward negative");
        out.line("// infinity. Go's own `%` takes its sign from the dividend.");
        out.line("func frFloorRem(dividend int, divisor int) int {");
        out.open();
        out.line("remainder := dividend % divisor");
        out.line("if remainder != 0 && (remainder < 0) != (divisor < 0) {");
        out.open();
        out.line("return remainder + divisor");
        out.close();
        out.line("}");
        out.line("return remainder");
        out.close();
        out.line("}");
        out.blank();
    }

    if !out.go_imports.is_empty() {
        let block: String = out
            .go_imports
            .iter()
            .map(|package| format!("import \"{package}\"\n"))
            .chain(std::iter::once("\n".to_string()))
            .collect();
        let clause = format!("package {}\n\n", go_package(module));
        let clause = clause.as_str();
        if let Some(at) = out.text.find(clause) {
            out.text.insert_str(at + clause.len(), &block);
        }
    }
}

/// The package clause this module belongs under.
fn go_package(module: &Module) -> String {
    let entry = module
        .items
        .iter()
        .any(|item| matches!(item, Item::Function(f) if f.name == "main" && f.params.is_empty()));
    if entry {
        return "main".to_string();
    }
    let named: String = module
        .name
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    // A name starting with a digit is not an identifier, and an empty one is no name at all.
    match named.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => named,
        _ => "translated".to_string(),
    }
}

/// The field names to construct this callee's record with, when a positional construction can
/// be mapped onto them.
fn positional_record(out: &Out, callee: &Expr, arity: usize) -> Option<Vec<String>> {
    let Expr::Name(name) = callee else {
        return None;
    };
    out.records
        .get(name)
        .filter(|fields| fields.len() == arity && arity > 0)
        .cloned()
}

/// The call's arguments settled into their declared positions, when its keywords can be.
fn carried_expr_filler(out: &Out) -> String {
    match out.language {
        Language::Rust => "todo!()".to_string(),
        Language::Python => "None".to_string(),
        Language::Go => "any(nil)".to_string(),
        Language::TypeScript => "undefined".to_string(),
        Language::Java => "null".to_string(),
        Language::Zig => "undefined".to_string(),
        _ => "null".to_string(),
    }
}

/// A short spelling of an argument for a carry message.
fn expr_hint(e: &Expr) -> String {
    match e {
        Expr::Str(s) => format!("{s:?}"),
        Expr::Int(v) | Expr::Float(v) => v.clone(),
        Expr::Name(n) => n.clone(),
        _ => "..".to_string(),
    }
}

/// A keyword call on a callee this module declares that still would not settle: the name or the
/// arity is wrong.
/// The filler a target writes where it cannot spell a call's keyword arguments.
/// Every value a body hands to `name.<verb>(x)`, at any depth.
fn given_to<'a>(body: &'a [Stmt], name: &str, verb: &str, found: &mut Vec<&'a Expr>) {
    for stmt in body {
        if let Stmt::Expr(Expr::Call { callee, args }) = stmt {
            let onto = match callee.as_ref() {
                Expr::Field { of, name: called } if called == verb => match of.as_ref() {
                    Expr::Name(n) => Some(n.as_str()),
                    _ => None,
                },
                _ => None,
            };
            if onto == Some(name) {
                if let Some(first) = args.first() {
                    found.push(first);
                }
            }
        }
        for inner in sub_bodies(stmt) {
            given_to(inner, name, verb, found);
        }
    }
}

/// A record literal as the constructor call the class-shaped targets write.
fn record_as_constructor(
    out: &mut Out,
    ty: &str,
    fields: &[(String, Expr)],
    render: fn(&mut Out, &Expr) -> String,
) -> String {
    match constructor_order(out, ty, fields) {
        Some(taken) => {
            let rendered: Vec<String> = taken.iter().map(|value| render(out, value)).collect();
            format!("new {}({})", out.name(ty), rendered.join(", "))
        }
        // A literal naming a subset of the fields, or a record this file does not
        // declare, has no order to fill a constructor with.
        None => render(
            out,
            &Expr::New {
                callee: Box::new(Expr::Name(ty.to_string())),
                args: fields
                    .iter()
                    .map(|(name, value)| Expr::Keyword {
                        name: name.clone(),
                        value: Box::new(value.clone()),
                    })
                    .collect(),
            },
        ),
    }
}

/// A record's fields beside the values a positional call passed them.
fn record_pairs(out: &Out, fields: &[String], rendered: &[String]) -> Vec<String> {
    fields
        .iter()
        .zip(rendered.iter())
        .map(|(field, value)| format!("{}: {value}", out.field(field)))
        .collect()
}

/// Carry a labeled break, which no target here spells.
fn carry_labeled_break(out: &mut Out, label: &str, rendered: &str) {
    let source = format!("break :{label} {rendered}");
    out.carried(&Unsupported {
        construct: "a labeled break".into(),
        source: source.clone(),
        line: 0,
    });
    let commented = out.comment(&format!("{MARKER}: {source}"));
    out.line(&commented);
}

fn carried_keywords(out: &mut Out, callee: &Expr, args: &[Expr], settled: bool) -> Option<String> {
    if !keywords_must_carry(out, callee, args, settled) {
        return None;
    }
    let rendered = joined(args, |a| match a {
        Expr::Keyword { name, value } => format!("{name}={}", expr_hint(value)),
        _ => expr_hint(a),
    });
    let named = match callee {
        Expr::Name(n) => n.clone(),
        _ => "the call".to_string(),
    };
    out.carried(&Unsupported {
        construct: "keyword argument".into(),
        source: format!("{named}({rendered})"),
        line: 0,
    });
    Some(carried_expr_filler(out))
}

fn keywords_must_carry(out: &Out, callee: &Expr, args: &[Expr], settled: bool) -> bool {
    if settled || !args.iter().any(|a| matches!(a, Expr::Keyword { .. })) {
        return false;
    }
    matches!(callee, Expr::Name(name) if out.functions.contains_key(name))
}

fn resolve_keywords(out: &Out, callee: &Expr, args: &[Expr]) -> Option<Vec<Expr>> {
    if !args.iter().any(|a| matches!(a, Expr::Keyword { .. })) {
        return None;
    }
    let Expr::Name(name) = callee else {
        return None;
    };
    let parameters = out.functions.get(name)?;
    let mut slots: Vec<Option<Expr>> = vec![None; parameters.len()];
    let mut position = 0usize;
    for argument in args {
        match argument {
            Expr::Keyword { name, value } => {
                let at = parameters.iter().position(|(p, _)| p == name)?;
                if slots[at].is_some() {
                    return None;
                }
                slots[at] = Some(value.as_ref().clone());
            }
            plain => {
                if position >= slots.len() || slots[position].is_some() {
                    return None;
                }
                slots[position] = Some(plain.clone());
                position += 1;
            }
        }
    }
    slots
        .into_iter()
        .zip(parameters.iter())
        .map(|(slot, (_, default))| slot.or_else(|| default.clone()))
        .collect()
}

/// Does this body leave on its own, making a `break` after it one statement too many?
fn leaves_on_its_own(body: &[Stmt]) -> bool {
    matches!(
        body.last(),
        Some(Stmt::Return(_)) | Some(Stmt::Throw(_)) | Some(Stmt::Break) | Some(Stmt::Continue)
    )
}

/// A test's prose name as an identifier: the words joined, everything else gone.
fn test_slug(name: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    match out.is_empty() {
        true => "unnamed".to_string(),
        false => out,
    }
}

/// Go says "exported" with a capital letter, which makes the convention the ACL.
fn go_name(name: &str, exported: bool) -> String {
    if exported {
        pascal(name)
    } else {
        camel(name)
    }
}

fn go_function(out: &mut Out, f: &Function, receiver: Option<&str>) {
    // Bindings made inside blocks die at their brace here too.
    let f = &with_hoisted_bindings(f, &out.function_returns);
    // A function that can fail takes this target's failure idiom back: the pair
    // return, the error checks, the hoisted calls.
    let f = &with_failure_idiom(f, &out.throwing.clone());
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);
    // What the body declared, for the return type a Python source never wrote.
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    let name = out.function_name(f);
    for line in &f.doc {
        out.line(&format!("// {name} {line}"));
    }
    if f.is_async {
        out.line(
            &out.comment(
                "declared async in the source; Go has no async. Call this from a goroutine",
            ),
        );
    }
    let mut foreign = false;
    let mut unannotated = false;
    let mut changed = false;
    let params: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| {
            let spelled = spell_param(out, p.kind, &p.name, &mut changed)?;
            if p.kind != ParamKind::Normal {
                return Some(spelled);
            }
            let ty = match &p.ty {
                Some(t) => {
                    if out.is_foreign(t) {
                        foreign = true;
                    }
                    go_type(t)
                }
                None => {
                    unannotated = true;
                    unknown(out, &p.name)
                }
            };
            Some(format!("{spelled} {ty}"))
        })
        .collect();
    // A declared `Result<T, E>` is Go's own `(T, error)` pair, and the body's `Ok`, `Err` and
    // propagations become the returns and checks that pair means.
    let result = result_ok(&out.declared_types, f.returns.as_ref());
    let returns = if let Some(ok) = &result {
        if out.is_foreign(ok) {
            foreign = true;
        }
        out.note_once(
            "a Result's error side crosses as Go's own error: the error's identity \
             becomes its message.",
        );
        match ok {
            Type::Unit => " error".to_string(),
            ok => format!(" ({}, error)", go_type(ok)),
        }
    } else {
        match &f.returns {
            Some(Type::Unit) => String::new(),
            // A source that annotated nothing still hands a value back, and Go has to name its
            // type.
            None if returns_a_value(f) => {
                unannotated = true;
                let ty = match inferred_return(out, f) {
                    Some(ty) => go_type(&ty),
                    None => unknown(out, &f.name),
                };
                format!(" {ty}")
            }
            None => String::new(),
            Some(Type::Tuple(parts)) => {
                if parts.iter().any(|p| out.is_foreign(p)) {
                    foreign = true;
                }
                format!(" ({})", joined(parts, go_type))
            }
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                format!(" {}", go_type(t))
            }
        }
    };
    out.go_result = result;
    out.go_errors = 0;
    // Go's convention is a one- or two-letter abbreviation, and there is no letter guaranteed
    // not to be a parameter's name already.
    let receiver = receiver
        .map(|r| format!("({} *{r}) ", receiver_word(out.language)))
        .unwrap_or_default();
    out.line(&format!(
        "func {receiver}{name}({}){returns} {{",
        params.join(", ")
    ));
    out.open();
    go_block(out, &f.body, f.returns.as_ref());
    // Go refuses a function that promises a value and has no path returning one.
    if !returns.is_empty() && !f.body.is_empty() && !body_leaves(&f.body) {
        out.line(&format!(
            "panic({})",
            quoted(Language::Go, &format!("{MARKER}: this body has no return"))
        ));
    }
    out.close();
    out.line("}");
    out.go_result = None;

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// An `Err(...)` payload as the error value Go returns.
fn go_error_value(out: &mut Out, e: &Expr) -> String {
    let errorf = |out: &mut Out, literal: &str, args: &[String]| {
        out.go_imports.insert("fmt");
        format!(
            "fmt.Errorf({}, {})",
            quoted(Language::Go, literal),
            args.join(", ")
        )
    };
    match e {
        Expr::Template(parts) => match literal_text(parts) {
            Some(text) => {
                out.go_imports.insert("errors");
                format!("errors.New({})", quoted(Language::Go, &text))
            }
            None => {
                // A literal `%` in the text would read as a verb of its own.
                let mut literal = String::new();
                let mut args = Vec::new();
                for part in parts {
                    match part {
                        TemplatePart::Text(text) => literal.push_str(&text.replace('%', "%%")),
                        TemplatePart::Expr(e) => {
                            literal.push_str("%v");
                            args.push(go_expr(out, e));
                        }
                    }
                }
                errorf(out, &literal, &args)
            }
        },
        Expr::Str(text) => {
            out.go_imports.insert("errors");
            format!("errors.New({})", quoted(Language::Go, text))
        }
        // A sum's variant as the failure, `ParseError.Empty`: Go's error has no variants, so
        // the variant's name becomes the message.
        Expr::Field { of, name } if matches!(of.as_ref(), Expr::Name(n) if out.sums.contains(n)) => {
            out.go_imports.insert("errors");
            format!("errors.New({})", quoted(Language::Go, name))
        }
        other => {
            let rendered = go_expr(out, other);
            errorf(out, "%v", &[rendered])
        }
    }
}

/// The Result mechanics of the enclosing function, written as Go's error returns.
fn go_lift_propagates(out: &mut Out, stmt: &Stmt) -> Option<(Vec<(String, String)>, Stmt)> {
    fn lift(e: &mut Expr, out: &mut Out, found: &mut Vec<(String, String)>) {
        if let Expr::Propagate(inner) = e {
            let call = go_expr(out, inner);
            let name = format!("frProp{}", out.lowering_names + found.len() + 1);
            found.push((name.clone(), call));
            *e = Expr::Name(name);
            return;
        }
        match e {
            Expr::Field { of, .. } => lift(of, out, found),
            Expr::Index { of, index } => {
                lift(of, out, found);
                lift(index, out, found);
            }
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                lift(callee, out, found);
                for a in args {
                    lift(a, out, found);
                }
            }
            Expr::Binary { left, right, .. } => {
                lift(left, out, found);
                lift(right, out, found);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) => lift(operand, out, found),
            Expr::Coalesce { value, fallback } => {
                lift(value, out, found);
                lift(fallback, out, found);
            }
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => {
                lift(condition, out, found);
                lift(then, out, found);
                lift(otherwise, out, found);
            }
            Expr::Tuple(items) | Expr::ListLit(items) => {
                for item in items {
                    lift(item, out, found);
                }
            }
            Expr::MapLit(entries) => {
                for (k, v) in entries {
                    lift(k, out, found);
                    lift(v, out, found);
                }
            }
            Expr::Keyword { value, .. } => lift(value, out, found),
            Expr::Variant { fields, .. } | Expr::RecordLit { fields, .. } => {
                for (_, v) in fields {
                    lift(v, out, found);
                }
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        lift(e, out, found);
                    }
                }
            }
            Expr::Cast { value, ty } => {
                lift(value, out, found);
                lift(ty, out, found);
            }
            _ => {}
        }
    }
    let mut rewritten = stmt.clone();
    let mut found = Vec::new();
    {
        let mut visit = |e: &mut Expr| lift(e, out, &mut found);
        match &mut rewritten {
            Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Throw(e) => visit(e),
            Stmt::Let { value: Some(e), .. } => visit(e),
            Stmt::Assign { target, value } => {
                visit(target);
                visit(value);
            }
            Stmt::TupleAssign { value, .. } => visit(value),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => visit(condition),
            Stmt::IfPresent { value, .. } | Stmt::WhilePresent { value, .. } => visit(value),
            Stmt::Switch { subject, .. } => visit(subject),
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                visit(iterable)
            }
            _ => {}
        }
    }
    if found.is_empty() {
        return None;
    }
    out.lowering_names += found.len();
    Some((found, rewritten))
}

fn go_propagate_stmt(out: &mut Out, stmt: &Stmt) -> bool {
    let (bound, inner) = match stmt {
        Stmt::Expr(Expr::Propagate(inner)) => (None, inner),
        Stmt::Let {
            name,
            value: Some(Expr::Propagate(inner)),
            ..
        } => (Some(name.clone()), inner),
        _ => return false,
    };
    // The settled idiom already writes these where the function returns an
    // error; this covers the contexts it cannot.
    if out.go_result.is_some() {
        return false;
    }
    let call = go_expr(out, inner);
    let err = out.fresh_go_error();
    match &bound {
        Some(name) => {
            let target = out.name(name);
            out.line(&format!("{target}, {err} := {call}"));
        }
        None => out.line(&format!("{err} := {call}")),
    }
    out.line(&format!("if {err} != nil {{"));
    out.open();
    match out.in_test {
        true => out.line(&format!("t.Fatal({err})")),
        false => out.line(&format!("panic({err})")),
    }
    out.close();
    out.line("}");
    true
}

fn go_result_stmt(out: &mut Out, stmt: &Stmt) -> bool {
    let Some(ok_ty) = out.go_result.clone() else {
        return false;
    };
    let failure = |err: &str| match &ok_ty {
        Type::Unit => format!("return {err}"),
        ty => format!("return {}, {err}", go_zero(ty)),
    };
    let named = |callee: &Expr, name: &str| matches!(callee, Expr::Name(n) if n == name);
    match stmt {
        Stmt::Return(Some(Expr::Call { callee, args })) if named(callee.as_ref(), "Ok") => {
            match args.as_slice() {
                [] => out.line("return nil"),
                [Expr::Tuple(items)] if items.is_empty() => {
                    let line = match &ok_ty {
                        Type::Unit => "return nil".to_string(),
                        ty => format!("return {}, nil", go_zero(ty)),
                    };
                    out.line(&line);
                }
                // `return Ok(f()?)`: the propagated call binds beside its error, and
                // only the settled value travels back with `nil`.
                [Expr::Propagate(inner)] => {
                    let call = go_expr(out, inner);
                    let err = out.fresh_go_error();
                    let value = format!("value{}", out.go_errors);
                    out.line(&format!("{value}, {err} := {call}"));
                    out.line(&format!("if {err} != nil {{"));
                    out.open();
                    out.line(&failure(&err));
                    out.close();
                    out.line("}");
                    out.line(&format!("return {value}, nil"));
                }
                [value] => {
                    let v = go_expr(out, value);
                    out.line(&format!("return {v}, nil"));
                }
                _ => return false,
            }
            true
        }
        Stmt::Return(Some(Expr::Call { callee, args })) if named(callee.as_ref(), "Err") => {
            let [value] = args.as_slice() else {
                return false;
            };
            let err = go_error_value(out, value);
            out.line(&failure(&err));
            true
        }
        // Zig's `return;` inside an `E!void` function is the success path.
        Stmt::Return(None) if ok_ty == Type::Unit => {
            out.line("return nil");
            true
        }
        Stmt::Let {
            name,
            value: Some(Expr::Propagate(inner)),
            ..
        } => {
            let call = go_expr(out, inner);
            let err = out.fresh_go_error();
            let bound = out.name(name);
            out.line(&format!("{bound}, {err} := {call}"));
            out.line(&format!("if {err} != nil {{"));
            out.open();
            out.line(&failure(&err));
            out.close();
            out.line("}");
            true
        }
        stmt => {
            // A propagated call whose value the source discards: `try f();` in Zig.
            let inner = match stmt {
                Stmt::Expr(Expr::Propagate(inner)) => inner,
                Stmt::Assign {
                    target: Expr::Name(discard),
                    value: Expr::Propagate(inner),
                } if discard == "_" => inner,
                _ => return false,
            };
            let call = go_expr(out, inner);
            let err = out.fresh_go_error();
            // The callee decides how many results the binding takes: a known unit ok
            // side returns the error alone here.
            let error_alone = matches!(
                inner.as_ref(),
                Expr::Call { callee, .. }
                    if matches!(
                        callee.as_ref(),
                        Expr::Name(n) if matches!(out.result_returns.get(n), Some(Type::Unit))
                    )
            );
            match error_alone {
                true => out.line(&format!("{err} := {call}")),
                false => out.line(&format!("_, {err} := {call}")),
            }
            out.line(&format!("if {err} != nil {{"));
            out.open();
            out.line(&failure(&err));
            out.close();
            out.line("}");
            true
        }
    }
}

/// Whether this body's last statement leaves the function on every path.
fn body_leaves(body: &[Stmt]) -> bool {
    match body.last() {
        Some(Stmt::Return(_)) | Some(Stmt::Throw(_)) => true,
        Some(Stmt::If {
            then, otherwise, ..
        })
        | Some(Stmt::IfPresent {
            then, otherwise, ..
        }) => !otherwise.is_empty() && body_leaves(then) && body_leaves(otherwise),
        Some(Stmt::MatchVariants { arms, default, .. }) => {
            !default.is_empty()
                && body_leaves(default)
                && arms.iter().all(|arm| body_leaves(&arm.body))
        }
        // A `finally` that leaves ends the function whatever the body did.
        Some(Stmt::Try {
            body,
            catches,
            finally,
            ..
        }) => {
            body_leaves(finally)
                || (body_leaves(body) && catches.iter().all(|c| body_leaves(&c.body)))
        }
        Some(Stmt::Switch { arms, default, .. }) => {
            !default.is_empty()
                && body_leaves(default)
                && arms.iter().all(|(_, body)| body_leaves(body))
        }
        // `for {}` with nothing to break out of never falls through.
        Some(Stmt::While {
            condition, body, ..
        }) => matches!(condition, Expr::Bool(true)) && !has_break(body),
        _ => false,
    }
}

/// Whether a loop body can break out of the loop it belongs to.
fn has_break(body: &[Stmt]) -> bool {
    let mut found = false;
    each_stmt(body, &mut |stmt| found |= matches!(stmt, Stmt::Break));
    found
}

fn go_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        out.line(&out.comment(&format!("{MARKER}: the source had no body to translate")));
        if let Some(t) = returns {
            let zero = go_zero(t);
            out.line(&format!("return {zero}"));
        }
        return;
    }
    for (at, stmt) in body.iter().enumerate() {
        if go_result_stmt(out, stmt) {
            continue;
        }
        if go_propagate_stmt(out, stmt) {
            continue;
        }
        // A propagated call buried inside the statement's expressions lifts
        // out first: bind, check, and let the statement read the binding.
        if out.go_result.is_none() {
            if let Some((lifted, rewritten)) = go_lift_propagates(out, stmt) {
                for (name, call) in lifted {
                    let err = out.fresh_go_error();
                    out.line(&format!("{name}, {err} := {call}"));
                    out.line(&format!("if {err} != nil {{"));
                    out.open();
                    match out.in_test {
                        true => out.line(&format!("t.Fatal({err})")),
                        false => out.line(&format!("panic({err})")),
                    }
                    out.close();
                    out.line("}");
                }
                go_block(out, std::slice::from_ref(&rewritten), returns);
                continue;
            }
        }
        match stmt {
            Stmt::Block(stmts) => {
                out.line("{");
                out.open();
                go_block(out, stmts, None);
                out.close();
                out.line("}");
            }
            // A function literal bound to the name; the header writes the
            // binding, so the body renders under a `:=`.
            Stmt::LocalFunction(f) => {
                let bound = out.name(&f.name);
                let params: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| {
                        let ty =
                            p.ty.as_ref()
                                .map(go_type)
                                .unwrap_or_else(|| "any".to_string());
                        format!("{} {ty}", out.name(&p.name))
                    })
                    .collect();
                let answer = f
                    .returns
                    .as_ref()
                    .map(|t| format!(" {}", go_type(t)))
                    .unwrap_or_default();
                out.line(&format!(
                    "{bound} := func({}){answer} {{",
                    params.join(", ")
                ));
                out.open();
                go_block(out, &f.body, f.returns.as_ref());
                out.close();
                out.line("}");
                out.line(&format!("_ = {bound}"));
            }
            Stmt::BreakWith { label, value } => {
                let rendered = value.as_ref().map(|v| go_expr(out, v)).unwrap_or_default();
                carry_labeled_break(out, label, &rendered);
            }
            // Go has no ternary.
            Stmt::Return(Some(Expr::Ternary {
                condition,
                then,
                otherwise,
            })) => {
                let c = go_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                let a = go_expr(out, then);
                out.line(&format!("return {a}"));
                out.close();
                out.line("}");
                let b = go_expr(out, otherwise);
                out.line(&format!("return {b}"));
            }
            Stmt::Let {
                name,
                ty: Some(ty),
                value:
                    Some(Expr::Ternary {
                        condition,
                        then,
                        otherwise,
                    }),
                ..
            } => {
                let bound = out.name(name);
                out.line(&format!("var {bound} {}", go_type(ty)));
                let c = go_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                let a = go_expr(out, then);
                out.line(&format!("{bound} = {a}"));
                out.close();
                out.line("} else {");
                out.open();
                let b = go_expr(out, otherwise);
                out.line(&format!("{bound} = {b}"));
                out.close();
                out.line("}");
            }
            Stmt::Assign {
                target,
                value:
                    Expr::Ternary {
                        condition,
                        then,
                        otherwise,
                    },
            } => {
                let t = go_expr(out, target);
                let c = go_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                let a = go_expr(out, then);
                out.line(&format!("{t} = {a}"));
                out.close();
                out.line("} else {");
                out.open();
                let b = go_expr(out, otherwise);
                out.line(&format!("{t} = {b}"));
                out.close();
                out.line("}");
            }
            Stmt::Return(value) => {
                // `return a, b`: the one place Go spells a tuple, so the tuple
                // dissolves into the statement instead of reaching go_expr.
                let text = match value {
                    Some(Expr::Tuple(items)) => {
                        format!(" {}", joined(items, |i| go_expr(out, i)))
                    }
                    // Go converts between its number types only when told to, and a length is
                    // an `int`.
                    Some(v) => {
                        let widens = matches!(returns, Some(Type::Float))
                            && matches!(static_type(out, v), Some(Type::Int));
                        let rendered = go_expr(out, v);
                        match widens {
                            true => format!(" float64({rendered})"),
                            false => format!(" {rendered}"),
                        }
                    }
                    None => String::new(),
                };
                out.line(&format!("return{text}"));
            }
            // A binding with no value is a `var` declaration: `x := nil` is not Go at
            // all, because `nil` alone has no type to infer.
            Stmt::Let {
                name,
                ty,
                value: None,
                ..
            } => {
                let bound = out.name(name);
                let declared = ty
                    .as_ref()
                    .map(go_type)
                    .unwrap_or_else(|| "any".to_string());
                out.line(&format!("var {bound} {declared}"));
            }
            Stmt::Let {
                name,
                value: Some(value),
                ..
            } => {
                // `[]any{}` under a signature promising `[]Point` is a file Go refuses.
                let declared = out.binding_types.get(name).cloned();
                let v = match (value, declared.as_ref()) {
                    (Expr::ListLit(items), Some(ty)) if items.is_empty() => {
                        format!("{}{{}}", go_type(ty))
                    }
                    // The same for a map built empty and filled afterwards.
                    (Expr::MapLit(entries), Some(ty @ Type::Map(_, _))) if entries.is_empty() => {
                        format!("{}{{}}", go_type(ty))
                    }
                    // Take a list of whole numbers as floats where the declaration says
                    // floats.
                    (Expr::ListLit(items), Some(ty @ Type::List(_))) => {
                        let rendered: Vec<String> = items.iter().map(|i| go_expr(out, i)).collect();
                        format!("{}{{{}}}", go_type(ty), rendered.join(", "))
                    }
                    _ => go_expr(out, value),
                };
                let bound = out.name(name);
                // A written number infers `int`, and the declaration may say `float64`.
                let written_number = |e: &Expr| {
                    matches!(
                        e,
                        Expr::Int(_)
                            | Expr::Float(_)
                            | Expr::Unary {
                                op: UnaryOp::Neg,
                                ..
                            }
                    )
                };
                let spelled_number = !out.in_loop_header
                    && matches!(declared.as_ref(), Some(Type::Float | Type::Int))
                    && written_number(value);
                match (spelled_number, v == "nil") {
                    (true, _) => {
                        let told = go_type(declared.as_ref().expect("the guard said so"));
                        out.line(&format!("var {bound} {told} = {v}"));
                    }
                    (_, true) => out.line(&format!("var {bound} any = nil")),
                    _ => out.line(&format!("{bound} := {v}")),
                }
            }
            Stmt::Assign { target, value } => {
                let t = go_expr(out, target);
                let v = go_expr(out, value);
                out.line(&format!("{t} = {v}"));
            }
            Stmt::TupleAssign {
                names,
                value,
                declares,
                ..
            } => {
                // A tuple on the right is Go's own comma list, and not a value.
                let v = match value {
                    Expr::Tuple(items) => joined(items, |i| go_expr(out, i)),
                    other => go_expr(out, other),
                };
                let bound = joined(names, |n| out.name(n));
                let operator = if *declares { ":=" } else { "=" };
                out.line(&format!("{bound} {operator} {v}"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                // `if _, ok := m[k]; ok` is how Go asks whether a key is there.
                let asks = match condition {
                    Expr::Call { callee, args } => match (callee.as_ref(), args.as_slice()) {
                        (Expr::Field { of, name }, [key]) if name == "contains" => {
                            match holds_a_set(out, of) {
                                true => Some((of.clone(), key.clone())),
                                false => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let c = match asks {
                    Some((of, key)) => format!(
                        "_, frOk := {}[{}]; frOk",
                        go_expr(out, &of),
                        go_expr(out, &key)
                    ),
                    None => go_expr(out, condition),
                };
                out.line(&format!("if {c} {{"));
                out.open();
                go_block(out, then, None);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    go_block(out, otherwise, None);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::IfPresent {
                binding,
                value,
                then,
                otherwise,
            } => {
                // The optional is a pointer here, and the payload is one dereference away.
                let v = go_expr(out, value);
                let bound = out.name(binding);
                out.line(&format!("if {bound}Ptr := {v}; {bound}Ptr != nil {{"));
                out.open();
                out.line(&format!("{bound} := *{bound}Ptr"));
                go_block(out, then, None);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    go_block(out, otherwise, None);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::MatchVariants {
                subject,
                arms,
                default,
                ..
            } => {
                let s = go_expr(out, subject);
                let bound = arms.iter().any(|arm| !arm.bindings.is_empty());
                match bound {
                    true => out.line(&format!("switch v := {s}.(type) {{")),
                    false => out.line(&format!("switch {s}.(type) {{")),
                }
                for arm in arms {
                    out.line(&format!("case {}:", out.name(&arm.variant)));
                    out.open();
                    for (field, local) in &arm.bindings {
                        out.line(&format!("{} := v.{}", out.name(local), out.field(field)));
                    }
                    go_block(out, &arm.body, returns);
                    out.close();
                }
                if !default.is_empty() {
                    out.line("default:");
                    out.open();
                    go_block(out, default, returns);
                    out.close();
                }
                out.line("}");
            }
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                let s = go_expr(out, subject);
                out.line(&format!("switch {s} {{"));
                for (literals, body) in arms {
                    let pattern: Vec<String> = literals.iter().map(|l| go_expr(out, l)).collect();
                    out.line(&format!("case {}:", pattern.join(", ")));
                    out.open();
                    go_block(out, body, None);
                    out.close();
                }
                if !default.is_empty() {
                    out.line("default:");
                    out.open();
                    go_block(out, default, None);
                    out.close();
                }
                out.line("}");
            }
            Stmt::Defer(cleanup) => match cleanup.as_slice() {
                [Stmt::Expr(call)] => {
                    let rendered = go_expr(out, call);
                    out.line(&format!("defer {rendered}"));
                }
                _ => {
                    out.line("defer func() {");
                    out.open();
                    go_block(out, cleanup, None);
                    out.close();
                    out.line("}()");
                }
            },
            // The failure-only cleanup arms a flag that the successful path turns off before
            // returning.
            Stmt::ErrDefer(cleanup) => {
                out.lowering_names += 1;
                let flag = format!("frFailed{}", out.lowering_names);
                out.line(&format!("{flag} := true"));
                out.line("defer func() {");
                out.open();
                out.line(&format!("if {flag} {{"));
                out.open();
                go_block(out, cleanup, None);
                out.close();
                out.line("}");
                out.close();
                out.line("}()");
                let mut rest: Vec<Stmt> = body[at + 1..].to_vec();
                let disarm = Stmt::Assign {
                    target: Expr::Name(flag),
                    value: Expr::Bool(false),
                };
                disarm_before_returns(&mut rest, &disarm);
                rest.push(disarm);
                go_block(out, &rest, returns);
                return;
            }
            Stmt::WhilePresent {
                binding,
                value,
                body,
            } => {
                let bound = out.name(binding);
                out.line("for {");
                out.open();
                // A propagated call in the test re-tries every pass, and its
                // failure leaves the way this function's failures leave.
                let v = match value {
                    Expr::Propagate(inner) => {
                        let call = go_expr(out, inner);
                        let err = out.fresh_go_error();
                        out.line(&format!("{bound}Try, {err} := {call}"));
                        out.line(&format!("if {err} != nil {{"));
                        out.open();
                        match out.go_result.clone() {
                            Some(Type::Unit) => out.line(&format!("return {err}")),
                            Some(ty) => out.line(&format!("return {}, {err}", go_zero(&ty))),
                            None => match out.in_test {
                                true => out.line(&format!("t.Fatal({err})")),
                                false => out.line(&format!("panic({err})")),
                            },
                        }
                        out.close();
                        out.line("}");
                        format!("{bound}Try")
                    }
                    _ => go_expr(out, value),
                };
                out.line(&format!("{bound}Ptr := {v}"));
                out.line(&format!("if {bound}Ptr == nil {{"));
                out.open();
                out.line("break");
                out.close();
                out.line("}");
                out.line(&format!("{bound} := *{bound}Ptr"));
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            Stmt::While { condition, body } => {
                // Go spells `while` as a one-clause `for`.
                let c = go_expr(out, condition);
                out.line(&format!("for {c} {{"));
                out.open();
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            // `for` is Go's own word for this, in all three of its spellings.
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                source,
                line,
            } => {
                out.in_loop_header = true;
                let parts = counted_header(
                    out,
                    init.as_deref(),
                    condition.as_ref(),
                    update.as_deref(),
                    &|out, stmt| go_block(out, std::slice::from_ref(stmt), None),
                    &|out, e| go_expr(out, e),
                );
                out.in_loop_header = false;
                match parts {
                    Some((start, test, step)) => {
                        // Go writes the bare loop as `for {` and the one-clause
                        // loop as `for cond {`, with no semicolons at all.
                        let header = match (start.is_empty(), test.is_empty(), step.is_empty()) {
                            (true, true, true) => String::new(),
                            (true, false, true) => format!("{test} "),
                            _ => format!("{} ", c_style_header(&start, &test, &step)),
                        };
                        out.line(&format!("for {header}{{"));
                        out.open();
                        go_block(out, body, None);
                        out.close();
                        out.line("}");
                    }
                    None => carry(out, &counted_original(source, *line)),
                }
            }
            Stmt::ForEachIndexed {
                index,
                binding,
                iterable,
                body,
            } => {
                let it = go_expr(out, iterable);
                let i = out.name(index);
                let bound = out.name(binding);
                out.line(&format!("for {i}, {bound} := range {it} {{"));
                out.open();
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = go_expr(out, iterable);
                let bound = out.name(binding);
                out.line(&format!("for _, {bound} := range {it} {{"));
                out.open();
                go_block(out, body, None);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                // `rows.append(x)` grows in place everywhere else; Go's `append`
                // returns the grown slice, so as a statement it must assign back.
                if let Expr::Call { callee, args } = e {
                    if let (Some(of), Some("append")) = callee_parts(callee) {
                        if let [x] = args.as_slice() {
                            let target = go_expr(out, &of.clone());
                            let value = go_expr(out, x);
                            out.line(&format!("{target} = append({target}, {value})"));
                            continue;
                        }
                    }
                }
                // Only a call can stand as a statement in Go.
                let text = go_expr(out, e);
                match e {
                    Expr::Call { .. } | Expr::New { .. } => out.line(&text),
                    _ => out.line(&format!("_ = {text}")),
                }
            }
            Stmt::Assert { condition, message } => {
                let c = go_expr(out, condition);
                let rendered = match message {
                    Some(m) => go_expr(out, m),
                    None => quoted(Language::Go, "assertion failed"),
                };
                out.line(&format!("if !({c}) {{"));
                out.open();
                out.line(&format!("panic({rendered})"));
                out.close();
                out.line("}");
            }
            Stmt::Break => out.line("break"),
            Stmt::Continue => out.line("continue"),
            // Go returns an error value.
            Stmt::Try {
                body: tried,
                catches,
                finally,
                source,
                line,
            } => {
                if catches.is_empty() && !finally.is_empty() {
                    // A finally with nothing to catch is this language's own
                    // `defer`: it runs however the body leaves.
                    out.line("defer func() {");
                    out.open();
                    go_block(out, finally, None);
                    out.close();
                    out.line("}()");
                    go_block(out, tried, None);
                    return;
                }
                if catches.is_empty() {
                    carry(
                        out,
                        &Unsupported {
                            construct: "try".into(),
                            source: source.clone(),
                            line: *line,
                        },
                    );
                } else {
                    let routed = returns_anywhere(tried);
                    let ret = format!("frRet{}", out.lowering_names + 1);
                    let flag = format!("frReturned{}", out.lowering_names + 1);
                    if routed {
                        let declared = returns.map(go_type).unwrap_or_else(|| "any".to_string());
                        out.line(&format!("var {ret} {declared}"));
                        out.line(&format!("_ = {ret}"));
                        out.line(&format!("{flag} := false"));
                        out.line(&format!("_ = {flag}"));
                    }
                    let err = out.fresh_go_error();
                    out.line(&format!("{err} := func() error {{"));
                    out.open();
                    let outer = out.go_result.take();
                    out.go_result = Some(Type::Unit);
                    let mut counter = out.lowering_names;
                    let mut tried = tried.clone();
                    if routed {
                        route_returns_through_flag(&mut tried, &ret, &flag);
                    }
                    extract_failing_calls(&mut tried, &out.throwing.clone(), &mut counter);
                    out.lowering_names = counter;
                    go_block(out, &tried, None);
                    out.line("return nil");
                    out.go_result = outer;
                    out.close();
                    out.line("}()");
                    if catches.len() > 1 {
                        out.fidelity.notes.push(format!(
                            "a try with {} catch arms folded into one: the arms \
                             selected by exception class, and the classes did not cross",
                            catches.len()
                        ));
                    }
                    let first = &catches[0];
                    out.line(&format!("if {err} != nil {{"));
                    out.open();
                    if let Some(binding) = &first.binding {
                        let bound = out.name(binding);
                        out.line(&format!("{bound} := {err}.Error()"));
                        // The read may not survive into every catch body; Go
                        // refuses an unused binding.
                        out.line(&format!("_ = {bound}"));
                    }
                    go_block(out, &first.body, None);
                    out.close();
                    out.line("}");
                    if !finally.is_empty() {
                        go_block(out, finally, None);
                    }
                    if routed {
                        // The stored return leaves the function, coerced the
                        // way any return here is.
                        let tail = Stmt::If {
                            condition: Expr::Name(flag.clone()),
                            then: vec![match returns.is_some() {
                                true => Stmt::Return(Some(Expr::Name(ret.clone()))),
                                false => Stmt::Return(None),
                            }],
                            otherwise: Vec::new(),
                        };
                        go_block(out, std::slice::from_ref(&tail), returns);
                    }
                }
            }
            Stmt::Throw(value) => {
                // Where the failure can move outward it becomes the error return.
                match out.go_result.clone() {
                    Some(ok_ty) => {
                        let err = go_error_value(out, value);
                        let line = match &ok_ty {
                            Type::Unit => format!("return {err}"),
                            ty => format!("return {}, {err}", go_zero(ty)),
                        };
                        out.line(&line);
                    }
                    None => {
                        let rendered = go_expr(out, value);
                        out.line(&format!("panic({rendered})"));
                    }
                }
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                out.line(&line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

fn go_type(ty: &Type) -> String {
    match ty {
        // Go writes "returns nothing" by writing nothing, which the return position handles.
        Type::Unit => "struct{}".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float64".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("[]{}", go_type(inner)),
        // Go has no set.
        Type::Set(inner) => format!("map[{}]struct{{}}", go_type(inner)),
        Type::Map(k, v) => format!("map[{}]{}", go_type(k), go_type(v)),
        Type::Optional(inner) => format!("*{}", go_type(inner)),
        // Go can only say several-types-as-one in a function's results, and the signature
        // writer spells that itself.
        Type::Fn { params, returns } => match returns.as_ref() {
            Type::Unit => format!("func({})", joined(params, go_type)),
            _ => format!("func({}) {}", joined(params, go_type), go_type(returns)),
        },
        Type::Tuple(parts) => format!("Unwritable_tuple_{}", parts.len()),
        Type::Named { name, args } => go_named(name, args),
    }
}

/// A type carried across by name, with the one qualifier Go allows.
fn go_named(name: &str, args: &[Type]) -> String {
    let full = generic(name, args, "[", "]", ".", go_type);
    let (path, rest) = match full.split_once('[') {
        Some((path, rest)) => (path, Some(rest)),
        None => (full.as_str(), None),
    };
    let segments: Vec<&str> = path.split('.').collect();
    let short = segments[segments.len().saturating_sub(2)..].join(".");
    match rest {
        Some(rest) => format!("{short}[{rest}"),
        None => short,
    }
}

/// Go has no `undefined`; a function that must return something returns its zero.
fn go_zero(ty: &Type) -> String {
    match ty {
        Type::Bool => "false".to_string(),
        Type::Int => "0".to_string(),
        Type::Float => "0".to_string(),
        Type::String => "\"\"".to_string(),
        Type::List(_) | Type::Set(_) | Type::Map(_, _) | Type::Optional(_) => "nil".to_string(),
        Type::Unit => String::new(),
        Type::Fn { .. } => "nil".to_string(),
        // The zero of several results is each one's zero, which only a return can say.
        Type::Tuple(parts) => joined(parts, go_zero),
        Type::Named { name, .. } => format!("{}{{}}", go_named(name, &[])),
    }
}

fn go_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Go has no set.
        Expr::SetLit(items) => {
            let element = items
                .first()
                .and_then(|first| static_type(out, first))
                .unwrap_or(Type::String);
            let rendered: Vec<String> = items
                .iter()
                .map(|i| format!("{}: {{}}", go_expr(out, i)))
                .collect();
            format!(
                "map[{}]struct{{}}{{{}}}",
                go_type(&element),
                rendered.join(", ")
            )
        }
        // Go names its fields in a literal, in any order, like the source did.
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), go_expr(out, value)))
                .collect();
            format!("{}{{{}}}", out.name(ty), rendered.join(", "))
        }
        // Go has nothing for this: not an operator, not a standard function.
        Expr::Coalesce { value, fallback } => {
            out.lowering_names += 1;
            let bound = format!("frOpt{}", out.lowering_names);
            format!(
                "func() any {{ {bound} := any({}); if {bound} != nil {{ return {bound} }}; return {} }}()",
                go_expr(out, value),
                go_expr(out, fallback)
            )
        }
        // Go has no conditional expression; a closure gives the `if` somewhere to put its
        // result.
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            let answer = match (static_type(out, then), static_type(out, otherwise)) {
                (Some(a), Some(b)) if a == b => go_type(&a),
                _ => "any".to_string(),
            };
            format!(
                "func() {answer} {{ if {} {{ return {} }}; return {} }}()",
                go_expr(out, condition),
                go_expr(out, then),
                go_expr(out, otherwise)
            )
        }
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => quoted(Language::Go, v),
        Expr::Null => "nil".to_string(),
        Expr::Name(n) => out.value_name(n),
        // Not re-cased: `reading.get(…)` names a real method on a value whose type
        // this does not know, and `reading.Get(…)` is a different method.
        Expr::Field { of, name } => {
            let object = receiver(go_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the parentheses does not
            // exist in this language.
            if out.properties.contains(name) {
                return format!("{object}.{}()", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => format!(
            "{}[{}]",
            receiver(go_expr(out, of), of),
            go_expr(out, index)
        ),
        Expr::Call { callee, args } => {
            if reaches_super(callee) && !shadows_builtin(out, "super") {
                let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
                let source = super_source(callee, &rendered);
                out.carried(&Unsupported {
                    construct: "super".into(),
                    source: source.clone(),
                    line: 0,
                });
                return format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"));
            }
            if let Some(mapped) = go_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            if let Some(filler) = carried_keywords(out, callee, args, settled.is_some()) {
                return filler;
            }
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = go_expr(out, callee);
                let pairs = record_pairs(out, &fields, &rendered);
                return format!("{target}{{{}}}", pairs.join(", "));
            }
            format!("{}({})", go_expr(out, callee), rendered.join(", "))
        }
        // Floor division rounds toward negative infinity and Go's `/` truncates,
        // so the exact spelling goes through the one floor the library has.
        Expr::Binary {
            op: BinaryOp::FloorDiv,
            left,
            right,
        } => {
            out.go_imports.insert("math");
            format!(
                "int(math.Floor(float64({}) / float64({})))",
                go_expr(out, left),
                go_expr(out, right)
            )
        }
        // Go's `%` is integers only.
        Expr::Binary {
            op: BinaryOp::Rem,
            left,
            right,
        } if matches!(static_type(out, left), Some(Type::Float))
            || matches!(static_type(out, right), Some(Type::Float)) =>
        {
            out.go_imports.insert("math");
            format!("math.Mod({}, {})", go_expr(out, left), go_expr(out, right))
        }
        // The remainder that goes with that division.
        Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        } => {
            out.zig_helpers.insert("go_floor_rem");
            format!(
                "frFloorRem({}, {})",
                go_expr(out, left),
                go_expr(out, right)
            )
        }
        // Go's `/` truncates two integers, and the source's did not.
        Expr::Binary {
            op: BinaryOp::TrueDiv,
            left,
            right,
        } => {
            if divides_a_string(out, left, right) {
                let rendered = format!("{} / {}", go_expr(out, left), go_expr(out, right))
                    .replace("*/", "* /");
                out.carried(&Unsupported {
                    construct: "`/` on a non-number".into(),
                    source: rendered.clone(),
                    line: 0,
                });
                return format!("nil /* {MARKER}: {rendered} */");
            }
            let side = |out: &mut Out, e: &Expr| {
                if let Expr::Int(n) = e {
                    return format!("{n}.0");
                }
                let text = binary_operand(go_expr(out, e), e, BinaryOp::Div, false);
                match static_type(out, e) {
                    Some(Type::Float) => text,
                    _ => format!("float64({text})"),
                }
            };
            format!("{} / {}", side(out, left), side(out, right))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            binary_operand(go_expr(out, left), left, *op, false),
            op.c_like(),
            binary_operand(go_expr(out, right), right, *op, true)
        ),
        // Go has no `await`.
        Expr::Await(inner) => {
            out.note_once(
                "an `await` runs blocking here: Go suspends by parking a goroutine, not by awaiting.",
            );
            go_expr(out, inner)
        }
        // Go propagates nothing: an error is a value somebody must return.
        Expr::Propagate(inner) => {
            let source = format!("{}?", go_expr(out, inner));
            out.carried(&Unsupported {
                construct: "error propagation".into(),
                source: source.clone(),
                line: 0,
            });
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // `NewThing(..)` is the Go convention, but it is a convention and not a rule.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = go_expr(out, callee);
                let pairs = record_pairs(out, &fields, &rendered);
                return format!("{target}{{{}}}", pairs.join(", "));
            }
            // A composite literal names at most `pkg.Type`; a deeper foreign
            // path keeps its last two steps, which is all Go can say of it.
            let target = match callee.as_ref() {
                Expr::Name(name) if name.matches('.').count() > 1 => {
                    let steps: Vec<&str> = name.rsplit('.').take(2).collect();
                    format!("{}.{}", steps[1], steps[0])
                }
                _ => go_expr(out, callee),
            };
            let named: Option<Vec<String>> = args
                .iter()
                .map(|a| match a {
                    Expr::Keyword { name, value } => {
                        Some(format!("{}: {}", out.field(name), go_expr(out, value)))
                    }
                    _ => None,
                })
                .collect();
            match named {
                // Named arguments are the composite literal's fields.
                Some(pairs) if !pairs.is_empty() => {
                    format!("{target}{{{}}}", pairs.join(", "))
                }
                // Positional ones fill the composite in declaration order,
                // which is what a Go literal without field names means.
                _ => format!("{target}{{{}}}", rendered.join(", ")),
            }
        }
        // Go spells this as a two-value type assertion, which is a statement.
        Expr::Cast { ty, value } => {
            format!("{}({})", go_expr(out, ty), go_expr(out, value))
        }
        // Go asks this with a type assertion; the two-value form as a closure
        // makes it an expression.
        Expr::InstanceOf { value, ty } => {
            let rendered = go_expr(out, value);
            let named = go_expr(out, ty);
            format!("func() bool {{ _, frOk := any({rendered}).({named}); return frOk }}()")
        }
        Expr::Keyword { name: _, value } => {
            out.note_once(
                "a named argument passes by position here: the target does not name arguments.",
            );
            go_expr(out, value)
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::Unwrap => {
                    return go_expr(out, operand);
                }
            };
            format!("{sign}{}", unary_operand(go_expr(out, operand), operand))
        }
        // The variant is a struct of Go's marker-interface convention, so
        // building one is building that struct.
        Expr::Variant { sum, name, fields } => {
            let rendered = joined(fields, |(f, v)| {
                format!("{}: {}", out.field(f), go_expr(out, v))
            });
            // Parenthesised because Go refuses a bare composite literal where a
            // block could follow: `if x == Go{}` reads the brace as the body.
            format!("({}{{{rendered}}})", variant_spelling(out, sum, name))
        }
        Expr::Tuple(items) => {
            out.note_once("a tuple outside a return travels as a slice here.");
            let rendered: Vec<String> = items.iter().map(|i| go_expr(out, i)).collect();
            format!("[]any{{{}}}", rendered.join(", "))
        }
        Expr::ListLit(items) => {
            // The element type comes from the elements, the way a map's does.
            let element = items
                .first()
                .and_then(literal_type_of)
                .as_ref()
                .map(go_type)
                .unwrap_or_else(|| "any".into());
            let rendered: Vec<String> = items.iter().map(|i| go_expr(out, i)).collect();
            format!("[]{element}{{{}}}", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            // The value type comes from the entries.
            let (keys, values) = map_literal_types(entries);
            let keys = keys
                .as_ref()
                .map(go_type)
                .unwrap_or_else(|| "string".into());
            let values = values.as_ref().map(go_type).unwrap_or_else(|| "any".into());
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", go_expr(out, k), go_expr(out, v)))
                .collect();
            format!("map[{keys}]{values}{{{}}}", rendered.join(", "))
        }
        Expr::Template(parts) => {
            // Go has no interpolation; `fmt.Sprintf` is how it says this.
            let mut literal = String::new();
            let mut args = Vec::new();
            for part in parts {
                match part {
                    TemplatePart::Text(text) => literal.push_str(text),
                    TemplatePart::Expr(e) => {
                        literal.push_str("%v");
                        args.push(go_expr(out, e));
                    }
                }
            }
            if args.is_empty() {
                quoted(Language::Go, &literal)
            } else {
                format!(
                    "fmt.Sprintf({}, {})",
                    quoted(Language::Go, &literal),
                    args.join(", ")
                )
            }
        }
        // Go writes no closure without every type spelled.
        Expr::Lambda {
            params,
            returns,
            body,
        } => {
            let typed: Option<Vec<String>> = params
                .iter()
                .map(|p| {
                    p.ty.as_ref()
                        .map(|t| format!("{} {}", out.name(&p.name), go_type(t)))
                })
                .collect();
            if let (Some(typed), Some(answers)) = (typed, returns) {
                let value = go_expr(out, body);
                return format!(
                    "func({}) {} {{ return {value} }}",
                    typed.join(", "),
                    go_type(answers)
                );
            }
            let rendered: Vec<String> = params.iter().map(|p| out.name(&p.name)).collect();
            let source = format!("({}) => {}", rendered.join(", "), go_expr(out, body));
            out.carried(&Unsupported {
                construct: "closure".into(),
                source: source.clone(),
                line: 0,
            });
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go has no comprehension and no map/filter on slices; writing a loop here
        // would be inventing statements from an expression.
        Expr::Comprehension { .. } => {
            out.carried(&Unsupported {
                construct: "comprehension".into(),
                source: "a comprehension, which Go spells as a loop".into(),
                line: 0,
            });
            // A bare `nil` has no type for `:=` to infer, so the stand-in asserts one.
            "any(nil)".to_string()
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("any(nil) /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

fn typescript(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Statement(stmt) => {
                ts_block(out, std::slice::from_ref(stmt));
                out.blank();
            }
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                let annotation =
                    c.ty.as_ref()
                        .map(|t| format!(": {}", ts_type(t)))
                        .unwrap_or_default();
                let value = ts_expr(out, &c.value);
                let export = if c.exported { "export " } else { "" };
                out.line(&format!(
                    "{export}const {}{annotation} = {value};",
                    out.name(&c.name)
                ));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                for line in &r.doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                let export = if r.exported { "export " } else { "" };
                // A record with no methods is an interface: it is data, and an interface is
                // what TypeScript calls that.
                if r.methods.is_empty() && r.fields.iter().all(|f| f.default.is_none()) {
                    let type_name = out.name(&r.name);
                    let base = ts_base(out, r)
                        .map(|base| format!(" extends {base}"))
                        .unwrap_or_default();
                    out.line(&format!("{export}interface {type_name}{base} {{"));
                    out.open();
                    for f in &r.fields {
                        for line in &f.doc {
                            out.line(&format!("/** {} */", block_comment_safe(line)));
                        }
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        out.line(&format!("{field_name}: {ty};"));
                    }
                    out.close();
                    out.line("}");
                } else {
                    let type_name = out.name(&r.name);
                    let base = ts_base(out, r)
                        .map(|base| format!(" extends {base}"))
                        .unwrap_or_default();
                    out.line(&format!("{export}class {type_name}{base} {{"));
                    out.open();
                    for f in &r.fields {
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        let default = f
                            .default
                            .as_ref()
                            .map(|d| format!(" = {}", ts_expr(out, d)))
                            .unwrap_or_default();
                        out.line(&format!("{field_name}: {ty}{default};"));
                    }
                    // Under `strictPropertyInitialization` a constructor has to give a field
                    // with no starting value one.
                    let methods = methods_of(out, r, false);
                    let already = methods.iter().any(|m| m.is_constructor);
                    if !r.fields.is_empty() && !already {
                        out.blank();
                        let taken: Vec<String> = r
                            .fields
                            .iter()
                            .map(|f| {
                                let ty =
                                    f.ty.as_ref()
                                        .map(ts_type)
                                        .unwrap_or_else(|| "unknown".to_string());
                                format!("{}: {ty}", out.field(&f.name))
                            })
                            .collect();
                        out.line(&format!("constructor({}) {{", taken.join(", ")));
                        out.open();
                        for f in &r.fields {
                            let field = out.field(&f.name);
                            out.line(&format!("this.{field} = {field};"));
                        }
                        out.close();
                        out.line("}");
                    }
                    for m in &methods {
                        out.blank();
                        ts_function(out, m, true);
                    }
                    out.close();
                    out.line("}");
                }
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                ts_function(out, f, false);
                out.blank();
            }
            Item::Import { text, line, target } => {
                // An import a sweep resolved names a sibling translated beside this file, so it
                // crosses as a real import.
                if let Some((stem, names)) = sibling_import(target) {
                    let list: Vec<String> = names
                        .iter()
                        .map(|n| match &n.alias {
                            Some(alias) => format!("{} as {alias}", out.name(&n.name)),
                            None => out.name(&n.name),
                        })
                        .collect();
                    out.line(&format!(
                        "import {{ {} }} from \"./{stem}\";",
                        list.join(", ")
                    ));
                    out.blank();
                    continue;
                }
                out.fidelity.imports_listed += 1;
                let header = out.comment(&format!(
                    "the source imported this at line {line}; the equivalent here is \
                     yours to add"
                ));
                out.line(&header);
                for l in text.lines() {
                    let commented = out.comment(l);
                    out.line(&commented);
                }
                out.blank();
            }
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("// {line}"));
                }
                let name = out.name(&n.name);
                let brand = format!("{}Brand", camel(&n.name));
                let export = if n.exported { "export " } else { "" };
                let base = ts_type(&n.base);
                out.line(&format!("declare const {brand}: unique symbol;"));
                out.line(&format!(
                    "{export}type {name} = {base} & {{ readonly [{brand}]: true }};"
                ));
                out.line(&format!(
                    "{export}function {name}(value: {base}): {name} {{"
                ));
                out.open();
                out.line(&format!("return value as {name};"));
                out.close();
                out.line("}");
                out.fidelity.newtypes += 1;
                out.blank();
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                out.note_once(
                    "a test crossed as a plain function: the language ships no runner, so \
                     wire it into yours by hand.",
                );
                out.line(&format!(
                    "export function {}(): void {{",
                    camel(&format!("test_{}", test_slug(name)))
                ));
                out.open();
                ts_block(out, body);
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                // One object type per variant, told apart by a literal field, and a union alias
                // naming the choice.
                let export = if s.exported { "export " } else { "" };
                let tag = discriminator(s);
                let names = hoisted_variant_names(out, module, s);
                for (variant, variant_name) in s.variants.iter().zip(&names) {
                    for line in &variant.doc {
                        out.line(&format!("/** {} */", block_comment_safe(line)));
                    }
                    out.line(&format!("{export}interface {variant_name} {{"));
                    out.open();
                    out.line(&format!(
                        "readonly {tag}: \"{}\";",
                        variant
                            .tag
                            .clone()
                            .unwrap_or_else(|| snake_always(&variant.name))
                    ));
                    for f in &variant.fields {
                        for line in &f.doc {
                            out.line(&format!("/** {} */", block_comment_safe(line)));
                        }
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        out.line(&format!("{field_name}: {ty};"));
                    }
                    out.close();
                    out.line("}");
                    out.blank();
                }
                for line in &s.doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                let type_name = out.name(&s.name);
                out.line(&format!(
                    "{export}type {type_name} = {};",
                    names.join(" | ")
                ));
                out.fidelity.sums += 1;
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }

    if out.zig_helpers.contains("ts_floor_rem") {
        out.blank();
        out.line("// The remainder that goes with division rounding toward negative");
        out.line("// infinity. TypeScript's own `%` takes its sign from the dividend.");
        out.line("function frFloorRem(dividend: number, divisor: number): number {");
        out.open();
        out.line("const remainder = dividend % divisor;");
        out.line("if (remainder !== 0 && (remainder < 0) !== (divisor < 0)) {");
        out.open();
        out.line("return remainder + divisor;");
        out.close();
        out.line("}");
        out.line("return remainder;");
        out.close();
        out.line("}");
        out.blank();
    }

    // fact is the same move the Go writer makes with its imports.
    if !out.ts_exceptions.is_empty() {
        let block: String = out
            .ts_exceptions
            .iter()
            .map(|name| format!("class {name} extends Error {{}}\n"))
            .chain(std::iter::once("\n".to_string()))
            .collect();
        out.text.insert_str(0, &block);
    }
}

/// The base a TypeScript record extends, spelled as this language's own.
fn ts_base(out: &mut Out, record: &Record) -> Option<String> {
    let base = inherited_base(out, record, true)?;
    if out.declared_types.contains(&base) {
        return Some(base);
    }
    match base.as_str() {
        "Exception" | "ValueError" | "RuntimeError" => Some("Error".to_string()),
        "ABC" | "abc.ABC" => {
            out.fidelity.notes.push(format!(
                "`{}` extends `{base}` in the source; TypeScript has no abstract \
                 base classes, so this drops the base and keeps the methods.",
                record.name
            ));
            None
        }
        _ => Some(base),
    }
}

/// A canonical exception name, as TypeScript spells it, noted for declaration.
fn ts_exception_name(out: &mut Out, name: &str) -> Option<&'static str> {
    if out.declared_types.contains(name) {
        return None;
    }
    let mapped = match name {
        "Exception" => "Error",
        "TypeError" => "TypeError",
        "ValueError" => "ValueError",
        "KeyError" => "KeyError",
        "RuntimeError" => "RuntimeError",
        _ => return None,
    };
    if mapped != "Error" && mapped != "TypeError" {
        out.ts_exceptions.insert(mapped);
    }
    Some(mapped)
}

/// A thrown canonical exception, as a `new` of the class this file spells it with.
fn ts_thrown(out: &mut Out, value: &Expr) -> Option<String> {
    let (callee, args) = match value {
        Expr::Call { callee, args } | Expr::New { callee, args } => (callee, args),
        _ => return None,
    };
    let Expr::Name(name) = callee.as_ref() else {
        return None;
    };
    let mapped = ts_exception_name(out, name)?;
    let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
    Some(format!("new {mapped}({})", rendered.join(", ")))
}

/// What each variant answers to where it lives beside every other type.
fn hoisted_variant_names(out: &mut Out, module: &Module, s: &Sum) -> Vec<String> {
    let cached: Vec<String> = s
        .variants
        .iter()
        .filter_map(|v| {
            out.variant_spellings
                .get(&(s.name.clone(), v.name.clone()))
                .cloned()
        })
        .collect();
    if cached.len() == s.variants.len() {
        return cached;
    }
    let mut taken: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for item in &module.items {
        match item {
            Item::Record(r) => {
                taken.insert(out.name(&r.name));
            }
            Item::Newtype(n) => {
                taken.insert(out.name(&n.name));
            }
            Item::Sum(other) => {
                taken.insert(out.name(&other.name));
                if other.name != s.name {
                    for v in &other.variants {
                        taken.insert(out.name(&v.name));
                    }
                }
            }
            _ => {}
        }
    }
    let mut names = Vec::new();
    for variant in &s.variants {
        let base = out.name(&variant.name);
        if taken.contains(&base) {
            let renamed = format!("{}{base}", out.name(&s.name));
            out.fidelity.notes.push(format!(
                "variant `{}` of `{}` crosses as `{renamed}`: the file already \
                 declares a type called `{base}`.",
                variant.name, s.name
            ));
            names.push(renamed);
        } else {
            names.push(base);
        }
    }
    names
}

/// The field that tells the variants of a sum apart, avoiding any name they use.
fn discriminator(s: &Sum) -> String {
    let taken: std::collections::BTreeSet<String> = s
        .variants
        .iter()
        .flat_map(|v| v.fields.iter())
        .map(|f| camel(&f.name))
        .collect();
    for candidate in ["kind", "tag", "variant", "discriminant"] {
        if !taken.contains(candidate) {
            return candidate.to_string();
        }
    }
    (2..)
        .map(|n| format!("kind{n}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("the numbers do not run out")
}

/// `inside_class` says where it lands, a different question from whether it takes a
/// receiver, a class holds `static empty()` beside `label()`.
fn ts_function(out: &mut Out, f: &Function, inside_class: bool) {
    let called = called_parameters(f);
    // Bindings made inside blocks die at their brace here; the source's did not.
    let f = &with_hoisted_bindings(f, &out.function_returns);
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);
    // TypeScript has one number type, so `/` needs to know which of them the
    // source declared: an integer division truncates and this one does not.
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    for line in &f.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    let mut foreign = false;
    let mut unannotated = false;
    let mut changed = false;
    let params: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| {
            let spelled = spell_param(out, p.kind, &p.name, &mut changed)?;
            let annotation = match &p.ty {
                Some(t) => {
                    if out.is_foreign(t) {
                        foreign = true;
                    }
                    format!(": {}", ts_type(t))
                }
                None => {
                    unannotated = true;
                    // `unknown` is a type here, and a correct one for a value nothing
                    // describes.
                    match called.get(&p.name).copied() {
                        Some(arity) => {
                            let answers = f
                                .returns
                                .as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| "unknown".to_string());
                            let taken: Vec<String> =
                                (0..arity).map(|at| format!("a{at}: {answers}")).collect();
                            format!(": ({}) => {answers}", taken.join(", "))
                        }
                        None => ": unknown".to_string(),
                    }
                }
            };
            let default = p
                .default
                .as_ref()
                .map(|d| format!(" = {}", ts_expr(out, d)))
                .unwrap_or_default();
            Some(format!("{spelled}{annotation}{default}"))
        })
        .collect();
    // An async function returns a promise of its declared type.
    let wrap = |rendered: String| {
        if f.is_async {
            format!(": Promise<{rendered}>")
        } else {
            format!(": {rendered}")
        }
    };
    let returns = match &f.returns {
        None if f.is_async => ": Promise<void>".to_string(),
        None => String::new(),
        Some(Type::Unit) => wrap("void".to_string()),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            wrap(ts_type(t))
        }
    };
    // `export` comes before `async`, and a method takes neither.
    let asynchrony = if f.is_async { "async " } else { "" };
    let prefix = if inside_class {
        let modifier = match f.receiver_binding.is_some() {
            true => "",
            false => "static ",
        };
        // `get total()`: the accessors read it as data, and TypeScript has the
        // idiom to keep that true.
        let accessor = if f.is_property && f.params.is_empty() {
            "get "
        } else {
            ""
        };
        format!("{modifier}{accessor}{asynchrony}")
    } else if f.exported {
        format!("export {asynchrony}function ")
    } else {
        format!("{asynchrony}function ")
    };
    out.line(&format!(
        "{prefix}{}({}){returns} {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    ts_block(out, &f.body);
    out.close();
    out.line("}");

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

fn ts_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line(&format!("throw new Error(\"{MARKER}\");"));
        return;
    }
    for (at, stmt) in body.iter().enumerate() {
        match stmt {
            Stmt::Block(stmts) => {
                out.line("{");
                out.open();
                ts_block(out, stmts);
                out.close();
                out.line("}");
            }
            Stmt::LocalFunction(f) => {
                let mut local = (**f).clone();
                local.exported = false;
                ts_function(out, &local, false);
            }
            Stmt::BreakWith { label, value } => {
                let rendered = value.as_ref().map(|v| ts_expr(out, v)).unwrap_or_default();
                carry_labeled_break(out, label, &rendered);
            }
            Stmt::Return(value) => {
                // A returned Result speaks this language's own failure handling: the
                // ok value returns bare, and the Err becomes a throw.
                if let Some((ok, payload)) = value.as_ref().and_then(|v| result_call(out, v)) {
                    out.note_once(RESULT_RAISED);
                    match (ok, payload) {
                        (true, Some(p)) => {
                            let rendered = ts_expr(out, p);
                            out.line(&format!("return {rendered};"));
                        }
                        (true, None) => out.line("return;"),
                        (false, payload) => {
                            let rendered = match payload {
                                Some(p) => match error_variant(out, p) {
                                    Some(variant) => quoted(out.language, variant),
                                    None => ts_expr(out, p),
                                },
                                None => String::new(),
                            };
                            out.line(&format!("throw new Error({rendered});"));
                        }
                    }
                    continue;
                }
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", ts_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text};"));
            }
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                // A binding whose initializer carried whole keeps its name, so the statements
                // after it still parse.
                let annotation = match (ty, value) {
                    (Some(t), _) => format!(": {}", ts_type(t)),
                    (None, Some(Expr::Unsupported(_))) => ": any".to_string(),
                    (None, Some(Expr::MapLit(entries))) => {
                        format!(": Record<string, {}>", ts_map_values(entries))
                    }
                    (None, _) => String::new(),
                };
                let keyword = if *mutable { "let" } else { "const" };
                let bound = out.name(name);
                match value {
                    Some(v) => {
                        let v = ts_expr(out, v);
                        out.line(&format!("{keyword} {bound}{annotation} = {v};"));
                    }
                    // A hoisted slot: declared here, assigned where the source
                    // assigned it.
                    None => out.line(&format!("{keyword} {bound}{annotation};")),
                }
            }
            Stmt::Assign { target, value } => {
                let t = ts_expr(out, target);
                let v = ts_expr(out, value);
                out.line(&format!("{t} = {v};"));
            }
            Stmt::TupleAssign {
                names,
                value,
                declares,
                ..
            } => {
                let v = ts_expr(out, value);
                let bound = joined(names, |n| out.name(n));
                let keyword = if *declares { "let " } else { "" };
                out.line(&format!("{keyword}[{bound}] = {v};"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = ts_expr(out, condition);
                out.line(&format!("if ({c}) {{"));
                out.open();
                ts_block(out, then);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    ts_block(out, otherwise);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::IfPresent {
                binding,
                value,
                then,
                otherwise,
            } => {
                let v = ts_expr(out, value);
                let bound = out.name(binding);
                out.line(&format!("const {bound} = {v};"));
                out.line(&format!("if ({bound} !== null) {{"));
                out.open();
                ts_block(out, then);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    ts_block(out, otherwise);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::MatchVariants {
                subject,
                sum,
                arms,
                default,
            } => {
                let s = ts_expr(out, subject);
                let tag = out
                    .sum_items
                    .get(sum)
                    .map(discriminator)
                    .unwrap_or_else(|| "kind".to_string());
                out.line(&format!("switch ({s}.{tag}) {{"));
                out.open();
                for arm in arms {
                    out.line(&format!(
                        "case \"{}\": {{",
                        wire_tag(out, sum, &arm.variant)
                    ));
                    out.open();
                    for (field, local) in &arm.bindings {
                        out.line(&format!(
                            "const {} = {s}.{};",
                            out.name(local),
                            out.field(field)
                        ));
                    }
                    ts_block(out, &arm.body);
                    out.line("break;");
                    out.close();
                    out.line("}");
                }
                if !default.is_empty() {
                    out.line("default: {");
                    out.open();
                    ts_block(out, default);
                    out.close();
                    out.line("}");
                }
                out.close();
                out.line("}");
            }
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                let s = ts_expr(out, subject);
                out.line(&format!("switch ({s}) {{"));
                out.open();
                for (literals, body) in arms {
                    for literal in literals {
                        let l = ts_expr(out, literal);
                        out.line(&format!("case {l}:"));
                    }
                    out.open();
                    ts_block(out, body);
                    if !leaves_on_its_own(body) {
                        out.line("break;");
                    }
                    out.close();
                }
                if !default.is_empty() {
                    out.line("default:");
                    out.open();
                    ts_block(out, default);
                    out.close();
                }
                out.close();
                out.line("}");
            }
            Stmt::Defer(cleanup) => {
                out.line("try {");
                out.open();
                let rest = &body[at + 1..];
                if !rest.is_empty() {
                    ts_block(out, rest);
                }
                out.close();
                out.line("} finally {");
                out.open();
                ts_block(out, cleanup);
                out.close();
                out.line("}");
                return;
            }
            // `errdefer` runs only on the failure path: clean up and rethrow.
            Stmt::ErrDefer(cleanup) => {
                out.line("try {");
                out.open();
                let rest = &body[at + 1..];
                if !rest.is_empty() {
                    ts_block(out, rest);
                }
                out.close();
                out.line("} catch (fr_err) {");
                out.open();
                ts_block(out, cleanup);
                out.line("throw fr_err;");
                out.close();
                out.line("}");
                return;
            }
            Stmt::WhilePresent {
                binding,
                value,
                body,
            } => {
                let v = ts_expr(out, value);
                let bound = out.name(binding);
                out.line("while (true) {");
                out.open();
                out.line(&format!("const {bound} = {v};"));
                out.line(&format!("if ({bound} === null) {{"));
                out.open();
                out.line("break;");
                out.close();
                out.line("}");
                ts_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::While { condition, body } => {
                let c = ts_expr(out, condition);
                out.line(&format!("while ({c}) {{"));
                out.open();
                ts_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                source,
                line,
            } => {
                let parts = counted_header(
                    out,
                    init.as_deref(),
                    condition.as_ref(),
                    update.as_deref(),
                    &|out, stmt| ts_block(out, std::slice::from_ref(stmt)),
                    &|out, e| ts_expr(out, e),
                );
                match parts {
                    Some((start, test, step)) => {
                        let header = c_style_header(&start, &test, &step);
                        out.line(&format!("for ({header}) {{"));
                        out.open();
                        ts_block(out, body);
                        out.close();
                        out.line("}");
                    }
                    None => carry(out, &counted_original(source, *line)),
                }
            }
            Stmt::ForEachIndexed {
                index,
                binding,
                iterable,
                body,
            } => {
                // No indexed form over an arbitrary iterable, so the counter
                // walks alongside.
                let it = ts_expr(out, iterable);
                let i = out.name(index);
                let bound = out.name(binding);
                out.line(&format!("let {i} = 0;"));
                out.line(&format!("for (const {bound} of {it}) {{"));
                out.open();
                ts_block(out, body);
                out.line(&format!("{i} += 1;"));
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = ts_expr(out, iterable);
                let bound = out.name(binding);
                out.line(&format!("for (const {bound} of {it}) {{"));
                out.open();
                ts_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = ts_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Assert { condition, message } => {
                let c = ts_expr(out, condition);
                // The thrown message has to be text; a message that is anything
                // else is said through `String`, which is the same words.
                let rendered = match message {
                    Some(m @ (Expr::Str(_) | Expr::Template(_))) => ts_expr(out, m),
                    Some(m) => format!("String({})", ts_expr(out, m)),
                    None => quoted(out.language, "assertion failed"),
                };
                out.line(&format!("if (!({c})) {{"));
                out.open();
                out.line(&format!("throw new Error({rendered});"));
                out.close();
                out.line("}");
            }
            Stmt::Break => out.line("break;"),
            Stmt::Continue => out.line("continue;"),
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                out.line("try {");
                out.open();
                ts_block(out, body);
                out.close();
                // TypeScript has one catch clause and no types on it.
                if !catches.is_empty() {
                    let bound = catches
                        .iter()
                        .find_map(|c| c.binding.clone())
                        .unwrap_or_else(|| "error".to_string());
                    let bound = out.name(&bound);
                    // The clause bodies render knowing which names are caught errors,
                    // so their `str(e)` can say `.message` instead of `String(e)`.
                    let caught: Vec<String> =
                        catches.iter().filter_map(|c| c.binding.clone()).collect();
                    out.catch_bindings.extend(caught.iter().cloned());
                    out.line(&format!("}} catch ({bound}) {{"));
                    out.open();
                    let typed: Vec<&Catch> = catches.iter().filter(|c| c.ty.is_some()).collect();
                    if typed.is_empty() {
                        for clause in catches {
                            ts_block(out, &clause.body);
                        }
                    } else {
                        for (index, clause) in catches.iter().enumerate() {
                            match &clause.ty {
                                Some(ty) => {
                                    let keyword = if index == 0 { "if" } else { "} else if" };
                                    // A canonical exception narrows against the class
                                    // this file declares for it, so the names line up.
                                    let rendered = match ty {
                                        Type::Named { name, args } if args.is_empty() => {
                                            ts_exception_name(out, name)
                                                .map(str::to_string)
                                                .unwrap_or_else(|| ts_type(ty))
                                        }
                                        _ => ts_type(ty),
                                    };
                                    out.line(&format!(
                                        "{keyword} ({bound} instanceof {rendered}) {{"
                                    ));
                                }
                                None => out.line("} else {"),
                            }
                            out.open();
                            ts_block(out, &clause.body);
                            out.close();
                        }
                        if catches.iter().all(|c| c.ty.is_some()) {
                            out.line("} else {");
                            out.open();
                            out.line(&format!("throw {bound};"));
                            out.close();
                        }
                        out.line("}");
                    }
                    out.close();
                    out.catch_bindings
                        .truncate(out.catch_bindings.len() - caught.len());
                }
                if !finally.is_empty() {
                    out.line("} finally {");
                    out.open();
                    ts_block(out, finally);
                    out.close();
                }
                out.line("}");
            }
            Stmt::Throw(value) => {
                // A bare message throws wrapped, so `(e as Error).message` and every other
                // catch-side read stays true.
                let rendered = match value {
                    Expr::Str(_) | Expr::Template(_) => {
                        format!("new Error({})", ts_expr(out, value))
                    }
                    Expr::Name(n) if out.catch_bindings.iter().any(|b| b == n) => out.value_name(n),
                    other => ts_thrown(out, other).unwrap_or_else(|| ts_expr(out, other)),
                };
                out.line(&format!("throw {rendered};"));
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                out.line(&line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
}

/// A record literal's values in the order the record declares its fields.
fn constructor_order(out: &Out, ty: &str, fields: &[(String, Expr)]) -> Option<Vec<Expr>> {
    let declared = out.records.get(ty)?;
    let defaults = out.record_field_defaults.get(ty);
    let mut taken = Vec::new();
    for name in declared {
        // The literal's value, or the one the record declares for a field the literal left out.
        let given = fields.iter().find(|(f, _)| f == name).map(|(_, v)| v);
        let held = given
            .or_else(|| defaults.and_then(|d| d.iter().find(|(f, _)| f == name).map(|(_, v)| v)))?;
        taken.push(held.clone());
    }
    // A literal naming something the record has not got is not this record.
    match fields.iter().all(|(f, _)| declared.contains(f)) {
        true => Some(taken),
        false => None,
    }
}

fn ts_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        Type::Bool => "boolean".to_string(),
        // TypeScript has one numeric type, and pretending otherwise would be a lie
        // about the signature.
        Type::Int | Type::Float => "number".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("{}[]", ts_type(inner)),
        Type::Set(inner) => format!("Set<{}>", ts_type(inner)),
        Type::Map(k, v) => format!("Record<{}, {}>", ts_type(k), ts_type(v)),
        Type::Optional(inner) => format!("{} | null", ts_type(inner)),
        Type::Fn { params, returns } => {
            let named = params
                .iter()
                .enumerate()
                .map(|(at, p)| format!("a{at}: {}", ts_type(p)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({named}) => {}", ts_type(returns))
        }
        Type::Tuple(parts) => format!("[{}]", joined(parts, ts_type)),
        Type::Named { name, args } => generic(name, args, "<", ">", ".", ts_type),
    }
}

fn ts_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // TypeScript builds a record by calling a constructor, which takes its arguments in the
        // order the class declares its fields.
        Expr::RecordLit { ty, fields } => record_as_constructor(out, ty, fields, ts_expr),
        Expr::Coalesce { value, fallback } => {
            format!("{} ?? {}", ts_expr(out, value), ts_expr(out, fallback))
        }
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "{} ? {} : {}",
            ts_expr(out, condition),
            ts_expr(out, then),
            ts_expr(out, otherwise)
        ),
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => quoted(out.language, v),
        Expr::Null => "null".to_string(),
        // `super` is the base reached, this language's own keyword, and not a name to re-case
        // or escape.
        Expr::Name(n) if n == "super" && !shadows_builtin(out, "super") => "super".to_string(),
        // Read a caught exception for its words: every other language binds the message.
        Expr::Name(n) if out.catch_bindings.iter().any(|b| b == n) => {
            format!("({} as Error).message", out.value_name(n))
        }
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            let object = receiver(ts_expr(out, of), of);
            // A property read stays a read here, and a property is a method:
            // its spelling lives in the method namespace, not the field one.
            if out.properties.contains(name) {
                return format!("{object}.{}", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => format!(
            "{}[{}]",
            receiver(ts_expr(out, of), of),
            ts_expr(out, index)
        ),
        Expr::Call { callee, args } => {
            if let Some(mapped) = ts_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            if let Some(filler) = carried_keywords(out, callee, args, settled.is_some()) {
                return filler;
            }
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
            format!("{}({})", ts_expr(out, callee), rendered.join(", "))
        }
        // Floor division has no operator here; `Math.floor` over the plain
        // division says the same number.
        Expr::Binary {
            op: BinaryOp::FloorDiv,
            left,
            right,
        } => format!(
            "Math.floor({} / {})",
            binary_operand(ts_expr(out, left), left, BinaryOp::Div, false),
            binary_operand(ts_expr(out, right), right, BinaryOp::Div, true)
        ),
        // The remainder that goes with that division.
        Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        } => {
            out.zig_helpers.insert("ts_floor_rem");
            format!(
                "frFloorRem({}, {})",
                ts_expr(out, left),
                ts_expr(out, right)
            )
        }
        Expr::Binary { op, left, right } => {
            // TypeScript has one number type and `/` on it never truncates, so `half(7)`
            // answered 3.5 where the source said 3.
            if *op == BinaryOp::Div && holds_an_integer(out, left) && holds_an_integer(out, right) {
                return format!(
                    "Math.trunc({} / {})",
                    binary_operand(ts_expr(out, left), left, *op, false),
                    binary_operand(ts_expr(out, right), right, *op, true)
                );
            }
            let spelling = match op {
                BinaryOp::Eq => "===",
                BinaryOp::Ne => "!==",
                other => other.c_like(),
            };
            format!(
                "{} {spelling} {}",
                binary_operand(ts_expr(out, left), left, *op, false),
                binary_operand(ts_expr(out, right), right, *op, true)
            )
        }
        Expr::Await(inner) => format!("await {}", ts_expr(out, inner)),
        Expr::Propagate(inner) => {
            out.note_once(
                "a `?`/`try` crosses as the bare expression: an error here \
                 propagates on its own.",
            );
            ts_expr(out, inner)
        }
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
            format!("new {}({})", ts_expr(out, callee), rendered.join(", "))
        }
        Expr::Cast { ty, value } => {
            format!("({} as {})", ts_expr(out, value), ts_expr(out, ty))
        }
        Expr::InstanceOf { value, ty } => {
            let rendered = ts_expr(out, value);
            // A canonical exception narrows against the class this file declares for
            // it, so `isinstance(e, ValueError)` and the throws name one class.
            if let Expr::Name(name) = ty.as_ref() {
                if let Some(mapped) = ts_exception_name(out, name) {
                    return format!("{rendered} instanceof {mapped}");
                }
            }
            format!("{rendered} instanceof {}", ts_expr(out, ty))
        }
        // A named argument becomes the options-object idiom: one property,
        // named what the source named it.
        Expr::Keyword { name, value } => {
            format!("{{ {name}: {} }}", ts_expr(out, value))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::Unwrap => {
                    return format!("{}!", unary_operand(ts_expr(out, operand), operand));
                }
            };
            format!("{sign}{}", unary_operand(ts_expr(out, operand), operand))
        }
        // The checker narrows on the discriminator field, so the literal
        // must be the one the type declared.
        Expr::Variant { sum, name, fields } => {
            let tag = out
                .sum_items
                .get(sum)
                .map(discriminator)
                .unwrap_or_else(|| "kind".to_string());
            let mut parts = vec![format!("{tag}: \"{}\"", wire_tag(out, sum, name))];
            for (f, v) in fields {
                parts.push(format!("{}: {}", out.field(f), ts_expr(out, v)));
            }
            format!("{{ {} }}", parts.join(", "))
        }
        // TypeScript spells a tuple's value as an array and only its type apart.
        Expr::Tuple(items) => format!("[{}]", joined(items, |i| ts_expr(out, i))),
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| ts_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        Expr::SetLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| ts_expr(out, i)).collect();
            match rendered.is_empty() {
                true => "new Set()".to_string(),
                false => format!("new Set([{}])", rendered.join(", ")),
            }
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    // Write a plain-identifier key bare, the way a person writing
                    // TypeScript would.
                    let key = match k {
                        Expr::Str(text) if is_identifier(text) => text.clone(),
                        other => format!("[{}]", ts_expr(out, other)),
                    };
                    format!("{key}: {}", ts_expr(out, v))
                })
                .collect();
            format!("{{ {} }}", rendered.join(", "))
        }
        Expr::Template(parts) => {
            let mut body = String::new();
            for part in parts {
                match part {
                    // A literal `${` in the text would open a substitution of its
                    // own, so it is escaped along with the delimiters.
                    TemplatePart::Text(text) => body.push_str(
                        &text
                            .replace('\\', "\\\\")
                            .replace('`', "\\`")
                            .replace("${", "\\${"),
                    ),
                    TemplatePart::Expr(e) => {
                        body.push_str("${");
                        body.push_str(&ts_expr(out, e));
                        body.push('}');
                    }
                }
            }
            format!("`{body}`")
        }
        Expr::Lambda {
            params,
            returns,
            body,
        } => {
            // A parameter the source typed keeps its type.
            let rendered: Vec<String> = params
                .iter()
                .map(|p| match &p.ty {
                    Some(t) => format!("{}: {}", out.name(&p.name), ts_type(t)),
                    None => format!("{}: any", out.name(&p.name)),
                })
                .collect();
            let answers = match returns {
                Some(t) => format!(": {}", ts_type(t)),
                None => String::new(),
            };
            let value = ts_expr(out, body);
            // An object literal standing bare after `=>` reads as a block, so it
            // takes the brackets that keep it a value.
            let value = match body.as_ref() {
                Expr::MapLit(_) => format!("({value})"),
                _ => value,
            };
            format!("({}){answers} => {value}", rendered.join(", "))
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let name = out.name(binding);
            let it = ts_expr(out, iterable);
            let filter = condition
                .as_ref()
                .map(|c| format!(".filter(({name}) => {})", ts_expr(out, c)))
                .unwrap_or_default();
            // `[x for x in xs if p(x)]` keeps every element it selects.
            let identity = matches!(element.as_ref(), Expr::Name(n) if *n == *binding);
            if identity && !filter.is_empty() {
                format!("{it}{filter}")
            } else {
                format!("{it}{filter}.map(({name}) => {})", ts_expr(out, element))
            }
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("null /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

/// Java has no top level below the type, so a module lives *inside* a class.
fn java(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    // `List`, `Map` and `Optional` are the three names this writer reaches for that Java does
    // not have in scope.
    let needed = java_utilities(module);
    if !needed.is_empty() {
        for name in &needed {
            out.line(&format!("import java.util.{name};"));
        }
        out.blank();
    }

    for item in &module.items {
        if let Item::Import { text, line, .. } = item {
            out.fidelity.imports_listed += 1;
            let header = out.comment(&format!(
                "the source imported this at line {line}; the equivalent here is yours to add."
            ));
            out.line(&header);
            for l in text.lines() {
                let commented = out.comment(l);
                out.line(&commented);
            }
            out.blank();
        }
    }

    // Everything that is not a type declaration has nowhere else to live.
    let loose: Vec<&Item> = module
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Constant(_)
                    | Item::Function(_)
                    | Item::Newtype(_)
                    | Item::Test { .. }
                    | Item::Statement(_)
                    | Item::Unsupported(_)
            )
        })
        .collect();
    // Records and sums, in source order: both are types Java writes at the top level.
    let types: Vec<&Item> = module
        .items
        .iter()
        .filter(|i| matches!(i, Item::Record(_) | Item::Sum(_)))
        .collect();

    // **One public class per file**, named after the file.
    if loose.is_empty() {
        for (index, item) in types.iter().enumerate() {
            if index > 0 {
                out.blank();
            }
            match item {
                Item::Record(r) => java_record(out, r, index == 0),
                Item::Sum(s) => java_sum(out, module, s, index == 0),
                _ => unreachable!("filtered to records and sums"),
            }
        }
        return;
    }

    for item in &types {
        match item {
            Item::Record(r) => java_record(out, r, false),
            Item::Sum(s) => java_sum(out, module, s, false),
            _ => unreachable!("filtered to records and sums"),
        }
        out.blank();
    }

    let name = module
        .name
        .as_deref()
        .map(pascal)
        .unwrap_or_else(|| "Module".to_string());
    out.line(&format!("public final class {name} {{"));
    out.open();
    let last = loose.len().saturating_sub(1);
    for (index, item) in loose.iter().enumerate() {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                let ty =
                    c.ty.as_ref()
                        .map(java_type)
                        .unwrap_or_else(|| java_inferred(&c.value));
                let value = java_expr(out, &c.value);
                let const_name = out.name(&c.name);
                out.line(&format!("public static final {ty} {const_name} = {value};"));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Function(f) => java_function(out, f, true),
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                out.line(&format!(
                    "public record {}({} value) {{}}",
                    pascal(&n.name),
                    java_type(&n.base)
                ));
                out.fidelity.newtypes += 1;
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("/** {} */", block_comment_safe(line)));
                }
                out.note_once(
                    "a test crossed as a plain method: the language ships no runner, so \
                     wire it into yours by hand.",
                );
                out.line(&format!(
                    "static void {}() {{",
                    camel(&format!("test_{}", test_slug(name)))
                ));
                out.open();
                java_block(out, body, None);
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
            }
            Item::Statement(stmt) if calls_declared_main(out, stmt) => {
                out.note_once(ENTRY_DROPPED);
            }
            Item::Statement(stmt) => carried_statement(out, stmt, java_expr),
            Item::Unsupported(u) => carry(out, u),
            _ => {}
        }
        if index != last {
            out.blank();
        }
    }
    // The number-display helper, when a template needed it: a whole double shows
    // whole, the way the dynamic sources displayed it.
    if out.zig_helpers.contains("java_show") {
        out.blank();
        out.line("static String frShow(double value) {");
        out.open();
        out.line("if (value == Math.rint(value) && !Double.isInfinite(value)) {");
        out.open();
        out.line("return String.valueOf((long) value);");
        out.close();
        out.line("}");
        out.line("return String.valueOf(value);");
        out.close();
        out.line("}");
    }
    out.close();
    out.line("}");
}

fn java_record(out: &mut Out, record: &Record, public: bool) {
    for line in &record.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    if !public && record.exported {
        let note = out.comment(
            "package-private: Java allows one public class per file and this file's own \
             class has that name. Move this to its own file to export it.",
        );
        out.line(&note);
    }
    let visibility = if public { "public " } else { "" };
    let name = out.name(&record.name);
    let base = inherited_base(out, record, true)
        .map(|base| format!(" extends {base}"))
        .unwrap_or_default();
    out.line(&format!("{visibility}class {name}{base} {{"));
    out.open();
    for field in &record.fields {
        let ty = field
            .ty
            .as_ref()
            .map(java_type)
            .unwrap_or_else(|| unknown(out, &field.name));
        let field_visibility = if field.exported { "public" } else { "private" };
        let field_name = out.field(&field.name);
        // Java starts a field where the declaration says.
        let default = field
            .default
            .as_ref()
            .map(|d| format!(" = {}", java_expr(out, d)))
            .unwrap_or_default();
        out.line(&format!("{field_visibility} {ty} {field_name}{default};"));
    }
    // Three of these languages build a record from a literal and have no constructor to carry.
    let methods = methods_of(out, record, true);
    let already = methods.iter().any(|m| m.is_constructor);
    if !record.fields.is_empty() && !already {
        out.blank();
        let taken: Vec<String> = record
            .fields
            .iter()
            .map(|f| {
                let ty =
                    f.ty.as_ref()
                        .map(java_type)
                        .unwrap_or_else(|| "Object".to_string());
                format!("{ty} {}", out.field(&f.name))
            })
            .collect();
        out.line(&format!("{name}({}) {{", taken.join(", ")));
        out.open();
        for f in &record.fields {
            let field = out.field(&f.name);
            out.line(&format!("this.{field} = {field};"));
        }
        out.close();
        out.line("}");
    }
    if !record.fields.is_empty() && !record.methods.is_empty() {
        out.blank();
    }
    for (index, method) in methods.iter().enumerate() {
        if index > 0 {
            out.blank();
        }
        java_function(out, method, method.receiver_binding.is_none());
    }
    out.close();
    out.line("}");
    out.fidelity.records += 1;
}

/// A sum as Java spells one: a sealed interface over records, one per variant.
fn java_sum(out: &mut Out, module: &Module, s: &Sum, public: bool) {
    for line in &s.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    if !public && s.exported {
        let note = out.comment(
            "package-private: Java allows one public class per file and this file's own \
             class has that name. Move this to its own file to export it.",
        );
        out.line(&note);
    }
    let visibility = if public { "public " } else { "" };
    let name = out.name(&s.name);
    let names = hoisted_variant_names(out, module, s);
    out.line(&format!(
        "{visibility}sealed interface {name} permits {} {{}}",
        names.join(", ")
    ));
    for (variant, variant_name) in s.variants.iter().zip(&names) {
        out.blank();
        for line in &variant.doc {
            out.line(&format!("/** {} */", block_comment_safe(line)));
        }
        let mut components = Vec::new();
        for f in &variant.fields {
            let ty =
                f.ty.as_ref()
                    .map(java_type)
                    .unwrap_or_else(|| unknown(out, &f.name));
            components.push(format!("{ty} {}", out.field(&f.name)));
        }
        out.line(&format!(
            "record {variant_name}({}) implements {name} {{}}",
            components.join(", ")
        ));
    }
    out.fidelity.sums += 1;
}

fn java_function(out: &mut Out, f: &Function, is_static: bool) {
    let called = called_parameters(f);
    // A parameter that holds a function is invoked through the interface Java wrapped it in:
    // `f.apply(n)`, never `f(n)`, which names a method of the class.
    out.functional_params = f
        .params
        .iter()
        .filter(|p| match &p.ty {
            Some(Type::Fn { params, .. }) => params.len() == 1,
            Some(_) => false,
            None => called.get(&p.name) == Some(&1),
        })
        .map(|p| p.name.clone())
        .collect();
    // Bindings made inside blocks die at their brace here; the source's did not.
    let f = &with_hoisted_bindings(f, &out.function_returns);
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    for line in &f.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    if f.is_async {
        let note = out.comment(
            "declared async in the source; Java has no async. Return a CompletableFuture \
             or call this from an executor",
        );
        out.line(&note);
    }

    let mut foreign = false;
    let mut unannotated = false;
    let mut changed = false;
    let params: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| {
            let spelled = spell_param(out, p.kind, &p.name, &mut changed)?;
            if p.kind != ParamKind::Normal {
                return Some(spelled);
            }
            let ty = match &p.ty {
                Some(t) => {
                    if out.is_foreign(t) {
                        foreign = true;
                    }
                    java_type(t)
                }
                None => {
                    unannotated = true;
                    // A parameter the body calls is a function, and `Object` has no `apply`.
                    match called.get(&p.name).copied() {
                        Some(1) => {
                            let answers = f
                                .returns
                                .as_ref()
                                .map(java_boxed)
                                .unwrap_or_else(|| "Object".to_string());
                            out.fidelity
                                .notes
                                .push(format!("`{}` had no declared type in the source", p.name));
                            format!("java.util.function.Function<{answers}, {answers}>")
                        }
                        _ => unknown(out, &p.name),
                    }
                }
            };
            Some(format!("{ty} {spelled}"))
        })
        .collect();

    let returns = match &f.returns {
        Some(Type::Unit) => "void".to_string(),
        // `void` over a body that returns a value does not compile, and a
        // source that annotates nothing still returns one.
        None if returns_a_value(f) => {
            unannotated = true;
            match inferred_return(out, f) {
                Some(ty) => java_type(&ty),
                None => unknown(out, &f.name),
            }
        }
        None => "void".to_string(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            java_type(t)
        }
    };

    // The runtime looks for a `public static void main(String[])` and starts nothing else.
    let entry = is_static && !f.is_constructor && f.name == "main";
    // A source that said `private` says it here.
    let visibility = match (f.exported || entry, f.is_private) {
        (true, _) => "public ",
        (false, true) => "private ",
        (false, false) => "",
    };
    // A constructor writes no return type at all.
    let returns = match f.is_constructor {
        true => String::new(),
        false => format!("{returns} "),
    };
    let modifier = if is_static && !f.is_constructor {
        "static "
    } else {
        ""
    };
    // A niladic `main` runs only on the JDKs that finalised instance main methods.
    let params = match entry && params.is_empty() {
        true => vec!["String[] args".to_string()],
        false => params,
    };
    out.line(&format!(
        "{visibility}{modifier}{returns}{}({}) {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    java_block(out, &f.body, f.returns.as_ref());
    out.close();
    out.line("}");

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

fn java_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        // A method that returns something must return something; one that does not can be
        // empty.
        if matches!(returns, Some(t) if *t != Type::Unit) {
            out.line("throw new UnsupportedOperationException(\"not translated\");");
        }
        return;
    }
    for (at, stmt) in body.iter().enumerate() {
        if let Stmt::Defer(cleanup) = stmt {
            out.line("try {");
            out.open();
            let rest = &body[at + 1..];
            if !rest.is_empty() {
                java_block(out, rest, returns);
            }
            out.close();
            out.line("} finally {");
            out.open();
            java_block(out, cleanup, None);
            out.close();
            out.line("}");
            return;
        }
        // `errdefer` runs only on the failure path: clean up and rethrow.
        if let Stmt::ErrDefer(cleanup) = stmt {
            out.line("try {");
            out.open();
            let rest = &body[at + 1..];
            if !rest.is_empty() {
                java_block(out, rest, returns);
            }
            out.close();
            out.line("} catch (RuntimeException frErr) {");
            out.open();
            java_block(out, cleanup, None);
            out.line("throw frErr;");
            out.close();
            out.line("}");
            return;
        }
        java_stmt(out, stmt);
    }
}

fn java_stmt(out: &mut Out, stmt: &Stmt) {
    match stmt {
        Stmt::Block(stmts) => {
            out.line("{");
            out.open();
            java_block(out, stmts, None);
            out.close();
            out.line("}");
        }
        Stmt::LocalFunction(f) => {
            let bound = out.name(&f.name);
            out.line(&format!("var {bound} = new Object() {{"));
            out.open();
            java_function(out, f, false);
            out.close();
            out.line("};");
        }
        Stmt::BreakWith { label, value } => {
            let rendered = value
                .as_ref()
                .map(|v| java_expr(out, v))
                .unwrap_or_default();
            carry_labeled_break(out, label, &rendered);
        }
        Stmt::Comment(text) => {
            let line = out.comment(text);
            out.line(&line);
        }
        Stmt::Return(value) => {
            // A returned Result speaks this language's own failure handling: the ok value
            // returns bare, and the Err becomes a throw.
            if let Some((ok, payload)) = value.as_ref().and_then(|v| result_call(out, v)) {
                out.note_once(RESULT_RAISED);
                match (ok, payload) {
                    (true, Some(p)) => {
                        let rendered = java_expr(out, p);
                        out.line(&format!("return {rendered};"));
                    }
                    (true, None) => out.line("return;"),
                    (false, payload) => {
                        let rendered = match payload {
                            Some(p) => match error_variant(out, p) {
                                Some(variant) => quoted(Language::Java, variant),
                                None => java_expr(out, p),
                            },
                            None => String::new(),
                        };
                        out.line(&format!("throw new RuntimeException({rendered});"));
                    }
                }
                return;
            }
            let text = value
                .as_ref()
                .map(|e| format!(" {}", java_expr(out, e)))
                .unwrap_or_default();
            out.line(&format!("return{text};"));
        }
        Stmt::Throw(value) => {
            // A bare message throws wrapped, and `getMessage()` reads it back.
            let rendered = match value {
                Expr::Str(_) | Expr::Template(_) => {
                    format!("new RuntimeException({})", java_expr(out, value))
                }
                Expr::Name(n) if out.catch_bindings.iter().any(|b| b == n) => out.value_name(n),
                other => java_expr(out, other),
            };
            out.line(&format!("throw {rendered};"));
        }
        Stmt::Let {
            name, ty, value, ..
        } => {
            let rendered = value
                .as_ref()
                .map(|v| java_expr(out, v))
                .unwrap_or_else(|| "null".to_string());
            // `var` is Java 10 and inference is the compiler's job, not this tool's: writing a
            // type it has not got would be a guess.
            let declared = ty.as_ref().map(java_type).unwrap_or_else(|| match value {
                Some(Expr::Unsupported(_)) | None => "Object".to_string(),
                // A lambda has no type of its own here: it takes the one the context asks for,
                // and `var` asks for nothing.
                Some(Expr::Lambda { params, .. }) => match params.len() {
                    1 => "java.util.function.Function<Integer, Integer>".to_string(),
                    2 => "java.util.function.BiFunction<Integer, Integer, Integer>".to_string(),
                    _ => "Object".to_string(),
                },
                // Name the type: an empty collection tells `var` nothing, and the diamond
                // fills in `Object`.
                Some(Expr::ListLit(items)) if items.is_empty() => out
                    .binding_types
                    .get(name)
                    .map(java_type)
                    .unwrap_or_else(|| "var".to_string()),
                Some(Expr::MapLit(entries)) if entries.is_empty() => out
                    .binding_types
                    .get(name)
                    .map(java_type)
                    .unwrap_or_else(|| "var".to_string()),
                _ => "var".to_string(),
            });
            let bound = out.name(name);
            match value {
                // A hoisted slot: declared here, assigned where the source assigned it.
                None => out.line(&format!("{declared} {bound};")),
                Some(_) => out.line(&format!("{declared} {bound} = {rendered};")),
            }
        }
        Stmt::Assign { target, value } => {
            // `d[k] = v` is `d.put(k, v)` here.
            if let Expr::Index { of, index } = target {
                let listish = matches!(static_type(out, of), Some(Type::List(_)));
                let object = java_expr(out, of);
                let at = java_expr(out, index);
                let right = java_expr(out, value);
                let verb = if listish { "set" } else { "put" };
                out.line(&format!("{object}.{verb}({at}, {right});"));
                return;
            }
            let left = java_expr(out, target);
            let right = java_expr(out, value);
            out.line(&format!("{left} = {right};"));
        }
        // Java has no tuple and no multiple return, so there is no line to write.
        Stmt::TupleAssign { names, value, .. } => {
            out.lowering_names += 1;
            let bound = format!("frTup{}", out.lowering_names);
            let v = java_expr(out, value);
            out.line(&format!("var {bound} = {v};"));
            for (at, name) in names.iter().enumerate() {
                if name == "_" {
                    continue;
                }
                let named = out.name(name);
                out.line(&format!("var {named} = {bound}.get({at});"));
            }
        }
        Stmt::If {
            condition,
            then,
            otherwise,
        } => {
            let c = java_expr(out, condition);
            out.line(&format!("if ({c}) {{"));
            out.open();
            java_block(out, then, None);
            out.close();
            if otherwise.is_empty() {
                out.line("}");
            } else {
                out.line("} else {");
                out.open();
                java_block(out, otherwise, None);
                out.close();
                out.line("}");
            }
        }
        Stmt::IfPresent {
            binding,
            value,
            then,
            otherwise,
        } => {
            // The optional cannot unwrap in place, so a second binding holds it and the branch
            // takes the payload out under the name the source bound.
            let v = java_expr(out, value);
            let bound = out.name(binding);
            out.line(&format!("var {bound}Maybe = {v};"));
            out.line(&format!("if ({bound}Maybe.isPresent()) {{"));
            out.open();
            out.line(&format!("var {bound} = {bound}Maybe.get();"));
            java_block(out, then, None);
            out.close();
            if otherwise.is_empty() {
                out.line("}");
            } else {
                out.line("} else {");
                out.open();
                java_block(out, otherwise, None);
                out.close();
                out.line("}");
            }
        }
        Stmt::MatchVariants {
            subject,
            arms,
            default,
            ..
        } => {
            let s = java_expr(out, subject);
            for (at, arm) in arms.iter().enumerate() {
                let variant = out.name(&arm.variant);
                let head = if at == 0 { "if" } else { "} else if" };
                out.line(&format!("{head} ({s} instanceof {variant}) {{"));
                out.open();
                for (field, local) in &arm.bindings {
                    out.line(&format!(
                        "var {} = (({variant}) {s}).{}();",
                        out.name(local),
                        out.field(field)
                    ));
                }
                java_block(out, &arm.body, None);
                out.close();
            }
            if default.is_empty() {
                out.line("}");
            } else {
                out.line("} else {");
                out.open();
                java_block(out, default, None);
                out.close();
                out.line("}");
            }
        }
        Stmt::Switch {
            subject,
            arms,
            default,
        } => {
            let mut s = java_expr(out, subject);
            // A `number` from TypeScript is a double here, and a double cannot select a switch.
            let integral_labels = arms
                .iter()
                .flat_map(|(literals, _)| literals)
                .all(|l| matches!(l, Expr::Int(_)));
            if integral_labels && matches!(static_type(out, subject), Some(Type::Float)) {
                s = format!("(int) ({s})");
            }
            out.line(&format!("switch ({s}) {{"));
            out.open();
            for (literals, body) in arms {
                for literal in literals {
                    let l = java_expr(out, literal);
                    out.line(&format!("case {l}:"));
                }
                out.open();
                java_block(out, body, None);
                if !leaves_on_its_own(body) {
                    out.line("break;");
                }
                out.close();
            }
            if !default.is_empty() {
                out.line("default:");
                out.open();
                java_block(out, default, None);
                out.close();
            }
            out.close();
            out.line("}");
        }
        // The block above takes a defer together with what follows it; one that
        // arrives here alone still runs its body at the same point.
        Stmt::Defer(cleanup) => {
            out.line("try {");
            out.open();
            out.close();
            out.line("} finally {");
            out.open();
            java_block(out, cleanup, None);
            out.close();
            out.line("}");
        }
        // Reached only when the block walker did not intercept; the failure-only
        // cleanup wraps nothing, so it can only clean up and rethrow nothing.
        Stmt::ErrDefer(cleanup) => {
            out.line("try {");
            out.open();
            out.close();
            out.line("} catch (RuntimeException frErr) {");
            out.open();
            java_block(out, cleanup, None);
            out.line("throw frErr;");
            out.close();
            out.line("}");
        }
        Stmt::WhilePresent {
            binding,
            value,
            body,
        } => {
            let v = java_expr(out, value);
            let bound = out.name(binding);
            out.line("while (true) {");
            out.open();
            out.line(&format!("var {bound}Maybe = {v};"));
            out.line(&format!("if ({bound}Maybe.isEmpty()) {{"));
            out.open();
            out.line("break;");
            out.close();
            out.line("}");
            out.line(&format!("var {bound} = {bound}Maybe.get();"));
            java_block(out, body, None);
            out.close();
            out.line("}");
        }
        Stmt::While { condition, body } => {
            let c = java_expr(out, condition);
            out.line(&format!("while ({c}) {{"));
            out.open();
            java_block(out, body, None);
            out.close();
            out.line("}");
        }
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            source,
            line,
        } => {
            let parts = counted_header(
                out,
                init.as_deref(),
                condition.as_ref(),
                update.as_deref(),
                &java_stmt,
                &|out, e| java_expr(out, e),
            );
            match parts {
                Some((start, test, step)) => {
                    let header = c_style_header(&start, &test, &step);
                    out.line(&format!("for ({header}) {{"));
                    out.open();
                    java_block(out, body, None);
                    out.close();
                    out.line("}");
                }
                None => carry(out, &counted_original(source, *line)),
            }
        }
        Stmt::ForEachIndexed {
            index,
            binding,
            iterable,
            body,
        } => {
            // No indexed form over an arbitrary iterable, so the counter walks
            // alongside.
            let it = java_expr(out, iterable);
            let i = out.name(index);
            let bound = out.name(binding);
            out.line(&format!("int {i} = 0;"));
            out.line(&format!("for (var {bound} : {it}) {{"));
            out.open();
            java_block(out, body, None);
            out.line(&format!("{i} += 1;"));
            out.close();
            out.line("}");
        }
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => {
            let it = java_expr(out, iterable);
            let bound = out.name(binding);
            // The element type is the collection's, where the collection's is known.
            let element = match static_type(out, iterable) {
                Some(Type::List(inner)) => java_type(&inner),
                _ => "var".to_string(),
            };
            out.line(&format!("for ({element} {bound} : {it}) {{"));
            out.open();
            java_block(out, body, None);
            out.close();
            out.line("}");
        }
        Stmt::Try {
            body,
            catches,
            finally,
            ..
        } => {
            out.line("try {");
            out.open();
            java_block(out, body, None);
            out.close();
            for clause in catches {
                // Java's catch must name a type; the languages that do not have one
                // catch everything, which is `Exception`.
                let selector = clause
                    .ty
                    .as_ref()
                    .map(java_type)
                    .unwrap_or_else(|| "Exception".to_string());
                let bound = clause
                    .binding
                    .as_ref()
                    .map(|b| out.name(b))
                    .unwrap_or_else(|| "error".to_string());
                out.line(&format!("}} catch ({selector} {bound}) {{"));
                out.open();
                // The body renders knowing its binding is a caught error, so a
                // `str(e)` can say `.getMessage()` instead of `String.valueOf`.
                if let Some(b) = &clause.binding {
                    out.catch_bindings.push(b.clone());
                }
                java_block(out, &clause.body, None);
                if clause.binding.is_some() {
                    out.catch_bindings.pop();
                }
                out.close();
            }
            if !finally.is_empty() {
                out.line("} finally {");
                out.open();
                java_block(out, finally, None);
                out.close();
            }
            out.line("}");
        }
        Stmt::Expr(Expr::Null) => {}
        Stmt::Expr(e) => {
            let text = java_expr(out, e);
            out.line(&format!("{text};"));
        }
        Stmt::Assert { condition, message } => {
            // Java's own `assert` is off unless the JVM is asked; the longhand
            // check runs everywhere, as the source's check did.
            let c = java_expr(out, condition);
            let rendered = match message {
                Some(m @ (Expr::Str(_) | Expr::Template(_))) => java_expr(out, m),
                Some(m) => format!("String.valueOf({})", java_expr(out, m)),
                None => quoted(Language::Java, "assertion failed"),
            };
            out.line(&format!("if (!({c})) {{"));
            out.open();
            out.line(&format!("throw new Error({rendered});"));
            out.close();
            out.line("}");
        }
        Stmt::Break => out.line("break;"),
        Stmt::Continue => out.line("continue;"),
        Stmt::Unsupported(u) => carry(out, u),
    }
}

/// The `java.util` types this module's Java will name.
fn java_utilities(module: &Module) -> std::collections::BTreeSet<&'static str> {
    let mut needed = std::collections::BTreeSet::new();

    fn in_type(ty: &Type, needed: &mut std::collections::BTreeSet<&'static str>) {
        match ty {
            Type::List(inner) => {
                needed.insert("List");
                in_type(inner, needed);
            }
            Type::Map(k, v) => {
                needed.insert("Map");
                in_type(k, needed);
                in_type(v, needed);
            }
            Type::Set(inner) => {
                needed.insert("Set");
                in_type(inner, needed);
            }
            Type::Optional(inner) => {
                needed.insert("Optional");
                in_type(inner, needed);
            }
            Type::Named { args, .. } => args.iter().for_each(|a| in_type(a, needed)),
            _ => {}
        }
    }
    fn in_expr(e: &Expr, needed: &mut std::collections::BTreeSet<&'static str>) {
        match e {
            // `List.of(…)` and `Map.of(…)` are how this writer spells a literal.
            Expr::ListLit(items) => {
                needed.insert("List");
                needed.insert("ArrayList");
                items.iter().for_each(|i| in_expr(i, needed));
            }
            // `new HashSet<>(Set.of(…))` is how this writer spells a set.
            Expr::SetLit(items) => {
                needed.insert("Set");
                needed.insert("HashSet");
                items.iter().for_each(|i| in_expr(i, needed));
            }
            Expr::MapLit(entries) => {
                needed.insert("Map");
                entries.iter().for_each(|(k, v)| {
                    in_expr(k, needed);
                    in_expr(v, needed);
                });
            }
            Expr::Binary { left, right, .. } => {
                in_expr(left, needed);
                in_expr(right, needed);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) => in_expr(operand, needed),
            Expr::Call { args, .. } | Expr::New { args, .. } => {
                args.iter().for_each(|a| in_expr(a, needed))
            }
            Expr::Lambda { body, .. } => in_expr(body, needed),
            _ => {}
        }
    }
    fn in_stmt(s: &Stmt, needed: &mut std::collections::BTreeSet<&'static str>) {
        match s {
            Stmt::Let { ty, value, .. } => {
                if let Some(ty) = ty {
                    in_type(ty, needed);
                }
                if let Some(value) = value {
                    in_expr(value, needed);
                }
            }
            Stmt::Expr(e) | Stmt::Return(Some(e)) => in_expr(e, needed),
            Stmt::Assign { value, .. } => in_expr(value, needed),
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                in_expr(condition, needed);
                then.iter().for_each(|s| in_stmt(s, needed));
                otherwise.iter().for_each(|s| in_stmt(s, needed));
            }
            Stmt::While { condition, body } => {
                in_expr(condition, needed);
                body.iter().for_each(|s| in_stmt(s, needed));
            }
            Stmt::ForEach { iterable, body, .. } => {
                in_expr(iterable, needed);
                body.iter().for_each(|s| in_stmt(s, needed));
            }
            _ => {}
        }
    }
    fn in_function(f: &Function, needed: &mut std::collections::BTreeSet<&'static str>) {
        f.params
            .iter()
            .filter_map(|p| p.ty.as_ref())
            .for_each(|t| in_type(t, needed));
        if let Some(ty) = &f.returns {
            in_type(ty, needed);
        }
        f.body.iter().for_each(|s| in_stmt(s, needed));
    }

    for item in &module.items {
        match item {
            Item::Function(f) => in_function(f, &mut needed),
            Item::Record(r) => {
                r.fields
                    .iter()
                    .filter_map(|f| f.ty.as_ref())
                    .for_each(|t| in_type(t, &mut needed));
                r.methods.iter().for_each(|m| in_function(m, &mut needed));
            }
            Item::Constant(c) => {
                if let Some(ty) = &c.ty {
                    in_type(ty, &mut needed);
                }
                in_expr(&c.value, &mut needed);
            }
            Item::Sum(s) => s
                .variants
                .iter()
                .flat_map(|v| v.fields.iter())
                .filter_map(|f| f.ty.as_ref())
                .for_each(|t| in_type(t, &mut needed)),
            _ => {}
        }
    }
    needed
}

/// An argument, with the one thing Java cannot pass by name turned into a value.
fn java_argument(out: &mut Out, e: &Expr) -> String {
    let Expr::Name(name) = e else {
        return java_expr(out, e);
    };
    let Some(params) = out.functions.get(name).cloned() else {
        return java_expr(out, e);
    };
    let called = out.name(name);
    let bound: Vec<String> = (0..params.len()).map(|at| format!("a{at}")).collect();
    match bound.as_slice() {
        [only] => format!("{only} -> {called}({only})"),
        _ => format!("({}) -> {called}({})", bound.join(", "), bound.join(", ")),
    }
}

fn java_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        Type::Bool => "boolean".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "double".to_string(),
        Type::String => "String".to_string(),
        Type::List(inner) => format!("List<{}>", java_boxed(inner)),
        Type::Set(inner) => format!("Set<{}>", java_boxed(inner)),
        Type::Map(k, v) => format!("Map<{}, {}>", java_boxed(k), java_boxed(v)),
        // Java's `Optional<T>` is the closest thing it has, and it is a real type
        // instead of a nullable annotation.
        Type::Optional(inner) => format!("Optional<{}>", java_boxed(inner)),
        // Java has no function type of its own.
        Type::Fn { params, returns } => match (params.as_slice(), returns.as_ref()) {
            ([], Type::Unit) => "Runnable".to_string(),
            ([], answer) => format!("java.util.function.Supplier<{}>", java_boxed(answer)),
            ([one], Type::Unit) => {
                format!("java.util.function.Consumer<{}>", java_boxed(one))
            }
            ([one], Type::Bool) => {
                format!("java.util.function.Predicate<{}>", java_boxed(one))
            }
            ([one], answer) => format!(
                "java.util.function.Function<{}, {}>",
                java_boxed(one),
                java_boxed(answer)
            ),
            ([first, second], Type::Unit) => format!(
                "java.util.function.BiConsumer<{}, {}>",
                java_boxed(first),
                java_boxed(second)
            ),
            ([first, second], answer) => format!(
                "java.util.function.BiFunction<{}, {}, {}>",
                java_boxed(first),
                java_boxed(second),
                java_boxed(answer)
            ),
            // Past two arguments Java names no interface, and inventing one
            // would declare a type the source never had.
            (many, _) => format!("Unwritable_function_{}", many.len()),
        },
        // Java has no tuple type.
        Type::Tuple(parts) => format!("Unwritable_tuple_{}", parts.len()),
        Type::Named { name, args } => generic(name, args, "<", ">", ".", java_boxed),
    }
}

/// A generic argument in Java cannot be a primitive: `List<int>` does not compile.
fn java_boxed(ty: &Type) -> String {
    match ty {
        Type::Bool => "Boolean".to_string(),
        Type::Int => "Integer".to_string(),
        Type::Float => "Double".to_string(),
        Type::Unit => "Void".to_string(),
        other => java_type(other),
    }
}

/// The type of a constant, when the source did not write one down.
fn java_inferred(value: &Expr) -> String {
    match value {
        Expr::Int(_) => "int".to_string(),
        Expr::Float(_) => "double".to_string(),
        Expr::Bool(_) => "boolean".to_string(),
        Expr::Str(_) => "String".to_string(),
        _ => "Object".to_string(),
    }
}

fn java_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Java builds a record by calling a constructor, which takes its arguments in the order
        // the class declares its fields.
        Expr::RecordLit { ty, fields } => record_as_constructor(out, ty, fields, java_expr),
        // Java spells it as a static call, and has to name the value twice to do it.
        Expr::Coalesce { value, fallback } => match nameable(value) {
            true => format!(
                "Objects.requireNonNullElse({}, {})",
                java_expr(out, value),
                java_expr(out, fallback)
            ),
            false => format!(
                "java.util.Optional.ofNullable({}).orElse({})",
                java_expr(out, value),
                java_expr(out, fallback)
            ),
        },
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "{} ? {} : {}",
            java_expr(out, condition),
            java_expr(out, then),
            java_expr(out, otherwise)
        ),
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Str(v) => quoted(Language::Java, v),
        Expr::Bool(v) => v.to_string(),
        Expr::Null => "null".to_string(),
        // `super` is the base reached, this language's own keyword, and not a name to re-case
        // or escape.
        Expr::Name(n) if n == "super" && !shadows_builtin(out, "super") => "super".to_string(),
        // Read a caught exception for its words: every other language binds the message.
        Expr::Name(n) if out.catch_bindings.iter().any(|b| b == n) => {
            format!("{}.getMessage()", out.value_name(n))
        }
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            let object = receiver(java_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the parentheses does not
            // exist in this language.
            if out.properties.contains(name) {
                return format!("{object}.{}()", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            let object = receiver(java_expr(out, of), of);
            let at = java_expr(out, index);
            // A subscript is `get` on a collection and `[…]` on an array, and which
            // this is depends on a type nothing here tracks.
            format!("{object}.get({at})")
        }
        Expr::Call { callee, args } => {
            // A functional-interface parameter is invoked through `apply`.
            if let Expr::Name(name) = callee.as_ref() {
                if out.functional_params.contains(name) && args.len() == 1 {
                    let spelled = out.name(name);
                    let argument = java_expr(out, &args[0]);
                    return format!("{spelled}.apply({argument})");
                }
            }
            if let Some(mapped) = java_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            if let Some(filler) = carried_keywords(out, callee, args, settled.is_some()) {
                return filler;
            }
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| java_argument(out, a)).collect();
            if let Expr::Name(name) = callee.as_ref() {
                if out.newtypes.contains_key(name) {
                    return format!("new {}({})", out.name(name), rendered.join(", "));
                }
            }
            // Java has no callable values: `f()()` and a called lambda are not in the grammar,
            // however the value arrived.
            if matches!(
                callee.as_ref(),
                Expr::Call { .. } | Expr::New { .. } | Expr::Lambda { .. }
            ) {
                let target = java_expr(out, callee);
                let source = format!("{target}({})", rendered.join(", "));
                out.carried(&Unsupported {
                    construct: "a call to a value".into(),
                    source: source.clone(),
                    line: 0,
                });
                return format!("null /* {MARKER}: {} */", source.replace("*/", "* /"));
            }
            format!("{}({})", java_expr(out, callee), rendered.join(", "))
        }
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| java_expr(out, a)).collect();
            format!("new {}({})", java_expr(out, callee), rendered.join(", "))
        }
        Expr::Cast { ty, value } => {
            format!("(({}) {})", java_expr(out, ty), java_expr(out, value))
        }
        // Java's `/` truncates two integers, silently and with the wrong answer.
        Expr::Binary {
            op: BinaryOp::TrueDiv,
            left,
            right,
        } => {
            let side = |out: &mut Out, e: &Expr| {
                if let Expr::Int(n) = e {
                    return format!("{n}.0");
                }
                let text = binary_operand(java_expr(out, e), e, BinaryOp::Div, false);
                match static_type(out, e) {
                    Some(Type::Float) => text,
                    _ => format!("(double) {text}"),
                }
            };
            format!("{} / {}", side(out, left), side(out, right))
        }
        Expr::InstanceOf { value, ty } => {
            let rendered = java_expr(out, value);
            format!("{rendered} instanceof {}", java_expr(out, ty))
        }
        // Floor division is a library call here, and `Math.floorDiv` is the one
        // that rounds the way the source's operator did.
        Expr::Binary {
            op: BinaryOp::FloorDiv,
            left,
            right,
        } => format!(
            "Math.floorDiv({}, {})",
            java_expr(out, left),
            java_expr(out, right)
        ),
        // The remainder that goes with that division, which Java names too.
        Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        } => format!(
            "Math.floorMod({}, {})",
            java_expr(out, left),
            java_expr(out, right)
        ),
        Expr::Binary { op, left, right } => {
            // `==` on a Java String compares references.
            if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && compares_strings(out, left, right) {
                let call = format!(
                    "java.util.Objects.equals({}, {})",
                    java_expr(out, left),
                    java_expr(out, right)
                );
                return match op {
                    BinaryOp::Ne => format!("!{call}"),
                    _ => call,
                };
            }
            format!(
                "{} {} {}",
                binary_operand(java_expr(out, left), left, *op, false),
                op.c_like(),
                binary_operand(java_expr(out, right), right, *op, true)
            )
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::Unwrap => {
                    return java_expr(out, operand);
                }
            };
            format!("{sign}{}", unary_operand(java_expr(out, operand), operand))
        }
        // A variant is a record of the sealed interface, and a record takes
        // its components positionally, in declared order.
        Expr::Variant { sum, name, fields } => {
            let declared: Vec<String> = out
                .sum_items
                .get(sum)
                .and_then(|s| s.variants.iter().find(|v| &v.name == name))
                .map(|v| v.fields.iter().map(|f| f.name.clone()).collect())
                .unwrap_or_else(|| fields.iter().map(|(f, _)| f.clone()).collect());
            let mut ordered: Vec<String> = Vec::new();
            for field in &declared {
                if let Some((_, value)) = fields.iter().find(|(f, _)| f == field) {
                    ordered.push(java_expr(out, value));
                }
            }
            format!(
                "new {}({})",
                variant_spelling(out, sum, name),
                ordered.join(", ")
            )
        }
        // Java has no tuple value.
        Expr::Tuple(items) => {
            out.note_once("a tuple travels as a List here.");
            let rendered: Vec<String> = items.iter().map(|i| java_expr(out, i)).collect();
            format!("java.util.List.of({})", rendered.join(", "))
        }
        // `Set.of` is immutable.
        Expr::SetLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| java_expr(out, i)).collect();
            match rendered.is_empty() {
                true => "new HashSet<>()".to_string(),
                false => format!("new HashSet<>(Set.of({}))", rendered.join(", ")),
            }
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| java_expr(out, i)).collect();
            // `List.of` is immutable, and a source that wrote a list literal is free to grow it
            // a line later.
            match rendered.is_empty() {
                true => "new ArrayList<>()".to_string(),
                false => format!("new ArrayList<>(List.of({}))", rendered.join(", ")),
            }
        }
        Expr::MapLit(entries) => {
            // `Map.of` is immutable, and the source puts keys into this after building it.
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}, {}", java_expr(out, k), java_expr(out, v)))
                .collect();
            match rendered.is_empty() {
                true => "new java.util.HashMap<>()".to_string(),
                false => format!("new java.util.HashMap<>(Map.of({}))", rendered.join(", ")),
            }
        }
        // Java's text blocks and `formatted` are neither of these, and `+` is what a
        // reader will recognise.
        Expr::Template(parts) => {
            let rendered: Vec<String> = parts
                .iter()
                .map(|part| match part {
                    TemplatePart::Text(text) => quoted(Language::Java, text),
                    // A double concatenated as text prints `10.0` here where every other target
                    // prints `10`.
                    TemplatePart::Expr(e) if matches!(static_type(out, e), Some(Type::Float)) => {
                        out.zig_helpers.insert("java_show");
                        format!("frShow({})", java_expr(out, e))
                    }
                    // `"diff " + a - b` subtracts from a string.
                    TemplatePart::Expr(e @ Expr::Binary { .. }) => {
                        format!("({})", java_expr(out, e))
                    }
                    TemplatePart::Expr(e) => java_expr(out, e),
                })
                .collect();
            match rendered.is_empty() {
                true => "\"\"".to_string(),
                false => rendered.join(" + "),
            }
        }
        Expr::Lambda { params, body, .. } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(&p.name)).collect();
            format!("({}) -> {}", rendered.join(", "), java_expr(out, body))
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let name = out.name(binding);
            let it = java_expr(out, iterable);
            let filter = condition
                .as_ref()
                .map(|c| format!(".filter({name} -> {})", java_expr(out, c)))
                .unwrap_or_default();
            let identity = matches!(element.as_ref(), Expr::Name(n) if *n == *binding);
            let map = if identity {
                String::new()
            } else {
                format!(".map({name} -> {})", java_expr(out, element))
            };
            format!("{it}.stream(){filter}{map}.toList()")
        }
        // Java has no `await`.
        Expr::Await(inner) => {
            out.note_once(
                "an `await` runs blocking here: Java suspends on a virtual thread, not by awaiting.",
            );
            java_expr(out, inner)
        }
        Expr::Propagate(inner) => {
            out.note_once(
                "a `?`/`try` crosses as the bare expression: an exception here \
                 propagates on its own.",
            );
            java_expr(out, inner)
        }
        Expr::Keyword { name: _, value } => {
            out.note_once(
                "a named argument passes by position here: the target does not name arguments.",
            );
            java_expr(out, value)
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("null /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

/// Zig.
fn zig(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    // Zig reaches its standard library through a binding the file has to make.
    if uses_the_standard_library(module) {
        out.line("const std = @import(\"std\");");
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Statement(stmt) if calls_declared_main(out, stmt) => {
                out.note_once(ENTRY_DROPPED);
            }
            Item::Statement(stmt) => carried_statement(out, stmt, zig_expr),
            Item::Import { text, line, .. } => {
                out.fidelity.imports_listed += 1;
                let header = out.comment(&format!(
                    "the source imported this at line {line}; the equivalent here is \
                     yours to add"
                ));
                out.line(&header);
                for l in text.lines() {
                    let commented = out.comment(l);
                    out.line(&commented);
                }
                out.blank();
            }
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/// {line}"));
                }
                let annotation =
                    c.ty.as_ref()
                        .map(|t| format!(": {}", zig_type(t)))
                        .unwrap_or_default();
                let value = zig_expr(out, &c.value);
                let name = out.name(&c.name);
                let visibility = if c.exported { "pub " } else { "" };
                zig_line(
                    out,
                    &format!("{visibility}const {name}{annotation} = {value};"),
                );
                out.fidelity.constants += 1;
                out.blank();
            }
            // A struct is a value bound to a `const`, so this reads as a
            // declaration of a constant that happens to be a type.
            Item::Record(r) => {
                for line in &r.doc {
                    out.line(&format!("/// {line}"));
                }
                let name = out.name(&r.name);
                let visibility = if r.exported { "pub " } else { "" };
                inherited_base(out, r, false);
                out.line(&format!("{visibility}const {name} = struct {{"));
                out.open();
                for f in &r.fields {
                    let ty =
                        f.ty.as_ref()
                            .map(zig_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_name = out.field(&f.name);
                    // Zig has the syntax: `count: usize = 0,`.
                    let default = f
                        .default
                        .as_ref()
                        .map(|d| format!(" = {}", zig_expr(out, d)))
                        .unwrap_or_default();
                    out.line(&format!("{field_name}: {ty}{default},"));
                }
                let mut spelled: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for m in &methods_of(out, r, false) {
                    out.blank();
                    let seen = spelled.entry(m.name.clone()).or_insert(0);
                    *seen += 1;
                    let mut renamed = m.clone();
                    if *seen > 1 {
                        out.note_once(
                            "overloads share a name the target refuses to repeat;                              later overloads take a numbered name.",
                        );
                        renamed.name = format!("{}{}", m.name, *seen);
                    }
                    zig_function(
                        out,
                        &renamed,
                        renamed.receiver_binding.is_some().then_some(name.as_str()),
                    );
                }
                out.close();
                out.line("};");
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                zig_function(out, f, None);
                out.blank();
            }
            Item::Newtype(n) => {
                for line in &n.doc {
                    out.line(&format!("// {line}"));
                }
                let name = out.name(&n.name);
                if n.base == Type::Int {
                    out.line(&format!("pub const {name} = enum(i64) {{ _ }};"));
                } else {
                    out.line(&format!("pub const {name} = {};", zig_type(&n.base)));
                    out.fidelity.notes.push(format!(
                        "`{}` is an alias in Zig: a distinct type over {} has no \
                         spelling this tool writes there",
                        n.name, n.base
                    ));
                }
                out.fidelity.newtypes += 1;
                out.blank();
            }
            Item::Test { doc, name, body } => {
                for line in doc {
                    out.line(&format!("/// {line}"));
                }
                out.line(&format!("test \"{name}\" {{"));
                out.open();
                let mutated = zig_mutated(body);
                zig_block(out, body, None, &mutated);
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                for line in &s.doc {
                    out.line(&format!("/// {line}"));
                }
                let name = out.name(&s.name);
                let visibility = if s.exported { "pub " } else { "" };
                out.line(&format!("{visibility}const {name} = union(enum) {{"));
                out.open();
                for variant in &s.variants {
                    for line in &variant.doc {
                        out.line(&format!("/// {line}"));
                    }
                    let variant_name = out.legal(snake_always(&variant.name));
                    match variant.fields.as_slice() {
                        [] => out.line(&format!("{variant_name}: void,")),
                        // A single field named for its value is the payload itself; wrapping it
                        // in a one-field struct would make every use site say `.value.value`.
                        [only] if only.name == "value" => {
                            let ty = only
                                .ty
                                .as_ref()
                                .map(zig_type)
                                .unwrap_or_else(|| unknown(out, &only.name));
                            out.line(&format!("{variant_name}: {ty},"));
                        }
                        fields => {
                            let mut spelled = Vec::new();
                            for f in fields {
                                let ty =
                                    f.ty.as_ref()
                                        .map(zig_type)
                                        .unwrap_or_else(|| unknown(out, &f.name));
                                spelled.push(format!("{}: {ty}", out.field(&f.name)));
                            }
                            out.line(&format!(
                                "{variant_name}: struct {{ {} }},",
                                spelled.join(", ")
                            ));
                        }
                    }
                }
                out.close();
                out.line("};");
                out.fidelity.sums += 1;
                out.blank();
            }
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }

    if !out.zig_helpers.is_empty() {
        if !out.text.contains("const std = @import(\"std\");") {
            let import = "const std = @import(\"std\");\n\n";
            let at = out
                .text
                .find(|c: char| c != '\n')
                .filter(|_| !out.text.starts_with("//"))
                .unwrap_or(0);
            let at = match out.text.starts_with("//") {
                // Header comments stay on top; the import goes after the first blank.
                true => out.text.find("\n\n").map(|i| i + 2).unwrap_or(0),
                false => at,
            };
            out.text.insert_str(at, import);
        }
        out.blank();
        if out.zig_helpers.contains("print") {
            out.line("/// The canonical `print`: formatted, to stdout, one line.");
            out.line("fn frPrint(comptime format: []const u8, args: anytype) void {");
            out.open();
            out.line("var buffer: [4096]u8 = undefined;");
            out.line(
                "var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);",
            );
            out.line("writer.interface.print(format, args) catch unreachable;");
            out.line("writer.interface.flush() catch unreachable;");
            out.close();
            out.line("}");
            out.blank();
        }
        if out.zig_helpers.contains("format") {
            out.line("/// A formatted string as a value.");
            out.line("///");
            out.line("/// Allocated from the page allocator and never freed: the source language");
            out.line("/// managed this memory, and a draft that must not leak would have to");
            out.line("/// invent an owner the source never named.");
            out.line("fn frFormat(comptime format: []const u8, args: anytype) []u8 {");
            out.open();
            out.line(
                "return std.fmt.allocPrint(std.heap.page_allocator, format, args) catch unreachable;",
            );
            out.close();
            out.line("}");
            out.blank();
        }
    }
}

/// Will this module's Zig need `std`?
fn uses_the_standard_library(module: &Module) -> bool {
    fn in_expr(types: &std::collections::BTreeMap<String, Type>, e: &Expr) -> bool {
        match e {
            Expr::Binary { op, left, right } => {
                (matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                    && (declared_string(types, left) || declared_string(types, right)))
                    || in_expr(types, left)
                    || in_expr(types, right)
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) => in_expr(types, operand),
            Expr::Call { args, .. } | Expr::New { args, .. } => {
                args.iter().any(|a| in_expr(types, a))
            }
            _ => false,
        }
    }
    fn declared_string(types: &std::collections::BTreeMap<String, Type>, e: &Expr) -> bool {
        match e {
            Expr::Str(_) => true,
            Expr::Name(name) => types.get(name) == Some(&Type::String),
            _ => false,
        }
    }
    fn in_stmt(types: &std::collections::BTreeMap<String, Type>, s: &Stmt) -> bool {
        match s {
            // The library check itself: `std.debug.assert` is a reason the
            // binding exists at all.
            Stmt::Assert { .. } => true,
            Stmt::Expr(e) | Stmt::Return(Some(e)) => in_expr(types, e),
            Stmt::Let { value: Some(e), .. } => in_expr(types, e),
            Stmt::Assign { value, .. } => in_expr(types, value),
            Stmt::If {
                condition,
                then,
                otherwise,
            }
            | Stmt::IfPresent {
                value: condition,
                then,
                otherwise,
                ..
            } => {
                in_expr(types, condition)
                    || then.iter().any(|s| in_stmt(types, s))
                    || otherwise.iter().any(|s| in_stmt(types, s))
            }
            Stmt::While {
                condition: value,
                body,
            }
            | Stmt::WhilePresent { value, body, .. }
            | Stmt::ForEach {
                iterable: value,
                body,
                ..
            }
            | Stmt::ForEachIndexed {
                iterable: value,
                body,
                ..
            } => in_expr(types, value) || body.iter().any(|s| in_stmt(types, s)),
            Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => {
                body.iter().any(|s| in_stmt(types, s))
            }
            Stmt::Switch {
                subject,
                arms,
                default,
            } => {
                in_expr(types, subject)
                    || arms
                        .iter()
                        .any(|(_, body)| body.iter().any(|s| in_stmt(types, s)))
                    || default.iter().any(|s| in_stmt(types, s))
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                body.iter().any(|s| in_stmt(types, s))
                    || catches
                        .iter()
                        .any(|c| c.body.iter().any(|s| in_stmt(types, s)))
                    || finally.iter().any(|s| in_stmt(types, s))
            }
            _ => false,
        }
    }
    fn in_function(f: &Function) -> bool {
        let types = declared_bindings(f);
        f.body.iter().any(|s| in_stmt(&types, s))
    }
    module.items.iter().any(|item| match item {
        Item::Function(f) => in_function(f),
        Item::Record(r) => r.methods.iter().any(in_function),
        _ => false,
    })
}

fn zig_function(out: &mut Out, f: &Function, receiver: Option<&str>) {
    // Bindings made inside blocks die at their brace here too.
    let f = &with_hoisted_bindings(f, &out.function_returns);
    out.zig_dyn = zig_growable_lists(&f.body);
    // A function that can fail takes this target's failure idiom back: the error
    // union in the signature, `try` at every failing call.
    let f = &with_failure_idiom(f, &out.throwing.clone());
    // The source's word for the receiver, spelled this target's way inside this body.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    out.fn_returns = f.returns.clone();
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        let note = out.comment(
            "declared async in the source; Zig removed `async` in 0.11 and has not \
             brought it back. This runs to completion.",
        );
        out.line(&note);
    }

    let mut foreign = false;
    let mut unannotated = false;
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if let Some(ty) = receiver {
        let word = receiver_word(out.language);
        let through_a_pointer = zig_mutated(&f.body).contains(word)
            || f.receiver_binding
                .as_deref()
                .is_some_and(|bound| zig_mutated(&f.body).contains(bound));
        let ty = match through_a_pointer {
            true => format!("*{ty}"),
            false => ty.to_string(),
        };
        params.push(format!("{word}: {ty}"));
    }
    for p in &f.params {
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(spelled);
            continue;
        }
        // Zig writes a type on every parameter and infers none.
        let ty = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                zig_type(t)
            }
            None => {
                unannotated = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("{spelled}: {ty}"));
    }

    let returns = match &f.returns {
        Some(Type::Unit) => "void".to_string(),
        // `void` over a body that returns a value does not compile, and a source that annotates
        // nothing still returns one.
        None if returns_a_value(f) => {
            unannotated = true;
            match inferred_return(out, f) {
                Some(ty) => zig_type(&ty),
                None => {
                    out.fidelity
                        .notes
                        .push(format!("`{}` had no declared type in the source", f.name));
                    "@TypeOf(undefined)".to_string()
                }
            }
        }
        None => "void".to_string(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            zig_type(t)
        }
    };

    // `main` must be `pub` whatever the source said: the entry point is the one
    // function the language itself calls.
    let entry = receiver.is_none() && f.name == "main";
    let visibility = if f.exported || entry { "pub " } else { "" };
    out.line(&format!(
        "{visibility}fn {}({}) {returns} {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    // Zig rejects a `var` nothing writes to.
    let mutated = zig_mutated(&f.body);
    zig_block(out, &f.body, f.returns.as_ref(), &mutated);
    out.close();
    out.line("}");

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
    }
    if unannotated {
        out.fidelity.signatures_untyped += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// The names bound to an empty list literal: the lists this function grows.
fn zig_growable_lists(body: &[Stmt]) -> std::collections::BTreeSet<String> {
    fn walk(body: &[Stmt], found: &mut std::collections::BTreeSet<String>) {
        for stmt in body {
            if let Stmt::Let { name, value, .. } = stmt {
                if matches!(value, Some(Expr::ListLit(items)) if items.is_empty()) {
                    found.insert(name.clone());
                }
            }
            for inner in sub_bodies(stmt) {
                walk(inner, found);
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    walk(body, &mut found);
    found
}

/// Every name this body writes to, including through a field or an index.
fn zig_mutated(body: &[Stmt]) -> std::collections::BTreeSet<String> {
    fn root(e: &Expr) -> Option<&str> {
        match e {
            Expr::Name(n) => Some(n),
            Expr::Field { of, .. } | Expr::Index { of, .. } => root(of),
            _ => None,
        }
    }
    fn walk(body: &[Stmt], found: &mut std::collections::BTreeSet<String>) {
        for stmt in body {
            match stmt {
                Stmt::Assign { target, .. } => {
                    if let Some(name) = root(target) {
                        found.insert(name.to_string());
                    }
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    walk(then, found);
                    walk(otherwise, found);
                }
                Stmt::While { body, .. } | Stmt::ForEach { body, .. } => walk(body, found),
                Stmt::CountedFor {
                    init, update, body, ..
                } => {
                    for header in [init, update].iter().copied().flatten() {
                        walk(std::slice::from_ref(header), found);
                    }
                    walk(body, found);
                }
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    walk(body, found);
                    for catch in catches {
                        walk(&catch.body, found);
                    }
                    walk(finally, found);
                }
                _ => {}
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    walk(body, &mut found);
    found
}

fn zig_block(
    out: &mut Out,
    body: &[Stmt],
    returns: Option<&Type>,
    mutated: &std::collections::BTreeSet<String>,
) {
    if body.is_empty() {
        // A function that returns something has to return something, and an invented
        // value would be a guess at what the body did.
        if matches!(returns, Some(t) if *t != Type::Unit) {
            out.line("@panic(\"not translated\");");
        }
        return;
    }
    for stmt in body {
        zig_stmt(out, stmt, mutated);
    }
}

/// A line, preceded by anything an expression could not say where it stood.
fn zig_line(out: &mut Out, text: &str) {
    let pending = std::mem::take(&mut out.pending);
    for note in pending {
        for line in note.lines() {
            let commented = out.comment(line);
            out.line(&commented);
        }
    }
    out.line(text);
}

fn zig_stmt(out: &mut Out, stmt: &Stmt, mutated: &std::collections::BTreeSet<String>) {
    match stmt {
        Stmt::Block(stmts) => {
            zig_line(out, "{");
            out.open();
            zig_block(out, stmts, None, mutated);
            out.close();
            out.line("}");
        }
        Stmt::LocalFunction(f) => {
            let bound = out.name(&f.name);
            zig_line(out, &format!("const {bound} = struct {{"));
            out.open();
            zig_function(out, f, None);
            out.close();
            out.line(&format!("}}.{};", out.name(&f.name)));
        }
        Stmt::BreakWith { label, value } => {
            let rendered = value.as_ref().map(|v| zig_expr(out, v)).unwrap_or_default();
            carry_labeled_break(out, label, &rendered);
        }
        Stmt::Comment(text) => {
            let line = out.comment(text);
            out.line(&line);
        }
        Stmt::Return(value) => {
            // A returned Result is native here: the coercion at the `return` decides success or
            // failure by the value.
            if let Some((ok, payload)) = value.as_ref().and_then(|v| result_call(out, v)) {
                if !ok {
                    let rendered = match payload {
                        Some(Expr::Str(message)) => format!("error.{}", zig_error_name(message)),
                        // A caught binding rethrows as the error value it is.
                        Some(other) => zig_expr(out, other),
                        None => "error.Failure".to_string(),
                    };
                    zig_line(out, &format!("return {rendered};"));
                    return;
                }
                // Zig converts between its number types only when told to, and a length is an
                // integer.
                let widens = matches!(out.fn_returns, Some(Type::Float))
                    && payload.is_some_and(|p| matches!(static_type(out, p), Some(Type::Int)));
                let text = payload
                    .map(|p| {
                        let rendered = zig_expr(out, p);
                        match widens {
                            true => format!(" @as(f64, @floatFromInt({rendered}))"),
                            false => format!(" {rendered}"),
                        }
                    })
                    .unwrap_or_default();
                zig_line(out, &format!("return{text};"));
                return;
            }
            // Zig converts between its number types only when told to, and a length is an
            // integer.
            let widens = matches!(out.fn_returns, Some(Type::Float))
                && value
                    .as_ref()
                    .is_some_and(|e| matches!(static_type(out, e), Some(Type::Int)));
            let text = value
                .as_ref()
                .map(|e| {
                    let rendered = zig_expr(out, e);
                    match widens {
                        true => format!(" @as(f64, @floatFromInt({rendered}))"),
                        false => format!(" {rendered}"),
                    }
                })
                .unwrap_or_default();
            zig_line(out, &format!("return{text};"));
        }
        // Zig has no exceptions: a failure is a value in the return type.
        Stmt::Throw(value) => {
            let message = match value {
                Expr::Str(_) => zig_expr(out, value),
                Expr::New { callee, args } => args
                    .iter()
                    .find(|a| matches!(a, Expr::Str(_)))
                    .map(|m| zig_expr(out, m))
                    .unwrap_or_else(|| {
                        let named = zig_expr(out, callee);
                        format!("\"{}\"", named.replace('"', ""))
                    }),
                other => zig_expr(out, other),
            };
            zig_line(out, &format!("@panic({message});"));
        }
        Stmt::Let {
            name,
            ty,
            value,
            mutable,
        } => {
            let rendered = value
                .as_ref()
                .map(|v| zig_expr(out, v))
                .unwrap_or_else(|| "undefined".to_string());
            let keyword = if *mutable && mutated.contains(name) {
                "var"
            } else {
                "const"
            };
            if let Some(Expr::Propagate(inner)) = value {
                let call = zig_failing_call(out, inner);
                let bound = out.name(name);
                zig_line(out, &format!("{keyword} {bound} = {call};"));
                return;
            }
            if holds_a_map(out, &Expr::Name(name.clone()))
                || matches!(value.as_ref(), Some(Expr::MapLit(_)))
            {
                let entries: &[(Expr, Expr)] = match value.as_ref() {
                    Some(Expr::MapLit(entries)) => entries,
                    _ => &[],
                };
                let bound = out.name(name);
                let (kind, values) = zig_map_shape(out, ty.as_ref(), entries);
                zig_line(
                    out,
                    &format!("var {bound} = {kind}({values}).init(std.heap.page_allocator);"),
                );
                for (key, value) in entries {
                    let k = zig_expr(out, key);
                    let v = zig_expr(out, value);
                    zig_line(out, &format!("{bound}.put({k}, {v}) catch unreachable;"));
                }
                return;
            }
            // A set goes through an allocator here, the same as a map, and its
            // members become the puts that fill it.
            if let Some(Expr::SetLit(items)) = value.as_ref() {
                let element = match ty.as_ref() {
                    Some(Type::Set(inner)) => zig_type(inner),
                    _ => items
                        .first()
                        .and_then(|first| static_type(out, first))
                        .map(|t| zig_type(&t))
                        .unwrap_or_else(|| "[]const u8".to_string()),
                };
                let bound = out.name(name);
                let built = match element.as_str() {
                    "[]const u8" => "std.StringHashMap(void)".to_string(),
                    other => format!("std.AutoHashMap({other}, void)"),
                };
                zig_line(
                    out,
                    &format!("var {bound} = {built}.init(std.heap.page_allocator);"),
                );
                for item in items {
                    let member = zig_expr(out, item);
                    zig_line(
                        out,
                        &format!("{bound}.put({member}, {{}}) catch unreachable;"),
                    );
                }
                return;
            }
            // An empty list that grows is `std.ArrayList`; a fixed array cannot
            // append.
            if out.zig_dyn.contains(name) {
                let element = match ty.as_ref() {
                    Some(Type::List(inner)) => zig_type(inner),
                    _ => "i64".to_string(),
                };
                let bound = out.name(name);
                zig_line(
                    out,
                    &format!("var {bound}: std.ArrayList({element}) = .empty;"),
                );
                return;
            }
            // An array literal is not a slice.
            let rendered = match (ty.as_ref(), value.as_ref()) {
                // The element type comes from the declaration rather than the items.
                (Some(Type::List(element)), Some(Expr::ListLit(items))) if !items.is_empty() => {
                    let written: Vec<String> = items.iter().map(|i| zig_expr(out, i)).collect();
                    format!("&[_]{}{{ {} }}", zig_type(element), written.join(", "))
                }
                _ => rendered,
            };
            let annotation = match (ty.as_ref(), keyword, value.as_ref()) {
                (Some(t), _, _) => format!(": {}", zig_type(t)),
                // A `var` must carry a fixed-size type; a comptime-known integer stays comptime
                // and will not compile as one.
                (None, "var", Some(v)) if zig_hole_spec(out, v) == "d" => ": i64".to_string(),
                _ => String::new(),
            };
            if matches!(value.as_ref(), Some(Expr::Str(_) | Expr::Template(_))) {
                out.zig_strings.insert(name.clone());
            }
            let bound = out.name(name);
            zig_line(out, &format!("{keyword} {bound}{annotation} = {rendered};"));
        }
        Stmt::Assign { target, value } => {
            if let Expr::Index { of, index } = target {
                if holds_a_map(out, of) {
                    let map = zig_expr(out, of);
                    let key = zig_expr(out, index);
                    let v = zig_expr(out, value);
                    zig_line(out, &format!("{map}.put({key}, {v}) catch unreachable;"));
                    return;
                }
            }
            let left = zig_expr(out, target);
            let right = zig_expr(out, value);
            zig_line(out, &format!("{left} = {right};"));
        }
        // Zig returns one value, so a pair has nothing to come from here.
        Stmt::TupleAssign {
            names,
            value,
            declares,
            ..
        } => {
            out.lowering_names += 1;
            let bound = format!("frTup{}", out.lowering_names);
            let v = zig_expr(out, value);
            zig_line(out, &format!("const {bound} = {v};"));
            let keyword = if *declares { "const " } else { "" };
            for (at, name) in names.iter().enumerate() {
                if name == "_" {
                    continue;
                }
                let named = out.name(name);
                out.line(&format!("{keyword}{named} = {bound}[{at}];"));
            }
        }
        Stmt::If {
            condition,
            then,
            otherwise,
        } => {
            let c = zig_expr(out, condition);
            zig_line(out, &format!("if ({c}) {{"));
            out.open();
            zig_block(out, then, None, mutated);
            out.close();
            if otherwise.is_empty() {
                out.line("}");
            } else {
                out.line("} else {");
                out.open();
                zig_block(out, otherwise, None, mutated);
                out.close();
                out.line("}");
            }
        }
        Stmt::IfPresent {
            binding,
            value,
            then,
            otherwise,
        } => {
            let v = zig_expr(out, value);
            let bound = out.name(binding);
            zig_line(out, &format!("if ({v}) |{bound}| {{"));
            out.open();
            zig_block(out, then, None, mutated);
            out.close();
            if otherwise.is_empty() {
                out.line("}");
            } else {
                out.line("} else {");
                out.open();
                zig_block(out, otherwise, None, mutated);
                out.close();
                out.line("}");
            }
        }
        Stmt::MatchVariants {
            subject,
            arms,
            default,
            ..
        } => {
            let s = zig_expr(out, subject);
            out.line(&format!("switch ({s}) {{"));
            out.open();
            for arm in arms {
                let tag = snake_always(&arm.variant);
                match arm.bindings.as_slice() {
                    [] => out.line(&format!(".{tag} => {{")),
                    [(field, local)] if field == "value" => {
                        out.line(&format!(".{tag} => |{}| {{", out.name(local)));
                    }
                    _ => out.line(&format!(".{tag} => |fields_of_{tag}| {{")),
                }
                out.open();
                if arm.bindings.len() > 1
                    || matches!(arm.bindings.as_slice(), [(f, _)] if f != "value")
                {
                    for (field, local) in &arm.bindings {
                        out.line(&format!(
                            "const {} = fields_of_{tag}.{};",
                            out.name(local),
                            out.field(field)
                        ));
                    }
                }
                zig_block(out, &arm.body, None, mutated);
                out.close();
                out.line("},");
            }
            if default.is_empty() {
                out.line("else => {},");
            } else {
                out.line("else => {");
                out.open();
                zig_block(out, default, None, mutated);
                out.close();
                out.line("},");
            }
            out.close();
            out.line("}");
        }
        Stmt::Switch {
            subject,
            arms,
            default,
        } => {
            // Zig cannot switch on strings; an `eql` chain says the same thing.
            let integral_labels = arms
                .iter()
                .flat_map(|(literals, _)| literals)
                .all(|l| matches!(l, Expr::Int(_)));
            if integral_labels && matches!(static_type(out, subject), Some(Type::Float)) {
                let s = zig_expr(out, subject);
                let cast = format!("@as(i64, @intFromFloat({s}))");
                zig_line(out, &format!("switch ({cast}) {{"));
                out.open();
                for (literals, body) in arms {
                    let pattern: Vec<String> = literals.iter().map(|l| zig_expr(out, l)).collect();
                    out.line(&format!("{} => {{", pattern.join(", ")));
                    out.open();
                    zig_block(out, body, None, mutated);
                    out.close();
                    out.line("},");
                }
                match default.is_empty() {
                    true => out.line("else => {},"),
                    false => {
                        out.line("else => {");
                        out.open();
                        zig_block(out, default, None, mutated);
                        out.close();
                        out.line("},");
                    }
                }
                out.close();
                out.line("}");
                return;
            }
            let stringly = arms
                .iter()
                .flat_map(|(literals, _)| literals)
                .any(|l| matches!(l, Expr::Str(_)));
            if stringly {
                let s = zig_expr(out, subject);
                for (at, (literals, body)) in arms.iter().enumerate() {
                    let tests: Vec<String> = literals
                        .iter()
                        .map(|l| format!("std.mem.eql(u8, {s}, {})", zig_expr(out, l)))
                        .collect();
                    let keyword = if at == 0 { "if" } else { "} else if" };
                    zig_line(out, &format!("{keyword} ({}) {{", tests.join(" or ")));
                    out.open();
                    zig_block(out, body, None, mutated);
                    out.close();
                }
                if default.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    zig_block(out, default, None, mutated);
                    out.close();
                    out.line("}");
                }
                return;
            }
            let s = zig_expr(out, subject);
            zig_line(out, &format!("switch ({s}) {{"));
            out.open();
            for (literals, body) in arms {
                let pattern: Vec<String> = literals.iter().map(|l| zig_expr(out, l)).collect();
                out.line(&format!("{} => {{", pattern.join(", ")));
                out.open();
                zig_block(out, body, None, mutated);
                out.close();
                out.line("},");
            }
            // Zig demands exhaustiveness, so emit the else arm even where the source had
            // none.
            if default.is_empty() {
                out.line("else => {},");
            } else {
                out.line("else => {");
                out.open();
                zig_block(out, default, None, mutated);
                out.close();
                out.line("},");
            }
            out.close();
            out.line("}");
        }
        Stmt::Defer(cleanup) => match cleanup.as_slice() {
            [Stmt::Expr(call)] => {
                let rendered = zig_expr(out, call);
                zig_line(out, &format!("defer {rendered};"));
            }
            _ => {
                zig_line(out, "defer {");
                out.open();
                zig_block(out, cleanup, None, mutated);
                out.close();
                out.line("}");
            }
        },
        Stmt::ErrDefer(cleanup) => match cleanup.as_slice() {
            [Stmt::Expr(call)] => {
                let rendered = zig_expr(out, call);
                zig_line(out, &format!("errdefer {rendered};"));
            }
            _ => {
                zig_line(out, "errdefer {");
                out.open();
                zig_block(out, cleanup, None, mutated);
                out.close();
                out.line("}");
            }
        },
        Stmt::WhilePresent {
            binding,
            value,
            body,
        } => {
            let v = zig_expr(out, value);
            let bound = out.name(binding);
            zig_line(out, &format!("while ({v}) |{bound}| {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        Stmt::While { condition, body } => {
            let c = zig_expr(out, condition);
            zig_line(out, &format!("while ({c}) {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        // Zig writes the step as a continue expression, which also runs when the body says
        // `continue`.
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            source,
            line,
        } => {
            let step = match update {
                Some(update) => {
                    match header_line(out, update, &|out, stmt| zig_stmt(out, stmt, mutated)) {
                        Some(step) => Some(step),
                        None => {
                            carry(out, &counted_original(source, *line));
                            return;
                        }
                    }
                }
                None => None,
            };
            if let Some(init) = init {
                zig_stmt(out, init, mutated);
            }
            let test = condition
                .as_ref()
                .map(|c| zig_expr(out, c))
                .unwrap_or_else(|| "true".to_string());
            let stepping = step.map(|s| format!(" : ({s})")).unwrap_or_default();
            zig_line(out, &format!("while ({test}){stepping} {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        // `for (xs) |x| { … }`, the binding goes in a payload after the header rather
        // than inside it.
        Stmt::ForEachIndexed {
            index,
            binding,
            iterable,
            body,
        } => {
            let mut it = zig_expr(out, iterable);
            if matches!(iterable, Expr::Name(n) if out.zig_dyn.contains(n.as_str())) {
                it.push_str(".items");
            }
            let i = out.name(index);
            let bound = out.name(binding);
            zig_line(out, &format!("for ({it}, 0..) |{bound}, {i}| {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => {
            let mut it = zig_expr(out, iterable);
            // A growable list iterates its elements.
            if matches!(iterable, Expr::Name(n) if out.zig_dyn.contains(n.as_str())) {
                it.push_str(".items");
            }
            let bound = out.name(binding);
            zig_line(out, &format!("for ({it}) |{bound}| {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        // `try/catch`, lowered to a labeled block: each failing call catches, runs the handler,
        // and breaks out.
        Stmt::Expr(Expr::Propagate(inner)) => {
            let call = zig_failing_call(out, inner);
            zig_line(out, &format!("_ = {call};"));
        }
        Stmt::Try {
            body: tried,
            catches,
            finally,
            source,
            line,
        } => {
            if catches.is_empty() && !finally.is_empty() {
                // A finally with nothing to catch is this language's own `defer`.
                out.line("{");
                out.open();
                out.line("defer {");
                out.open();
                let finally_mutated = zig_mutated(finally);
                for stmt in finally {
                    zig_stmt(out, stmt, &finally_mutated);
                }
                out.close();
                out.line("}");
                let tried_mutated = zig_mutated(tried);
                for stmt in tried {
                    zig_stmt(out, stmt, &tried_mutated);
                }
                out.close();
                out.line("}");
                return;
            }
            // A `return` inside the labeled block leaves the function, the way the source's
            // `return` inside the `try` did.
            if catches.is_empty() {
                carry(
                    out,
                    &Unsupported {
                        construct: "try/catch".into(),
                        source: source.clone(),
                        line: *line,
                    },
                );
                return;
            }
            out.lowering_names += 1;
            let label = format!("frTry{}", out.lowering_names);
            if catches.len() > 1 {
                out.fidelity.notes.push(format!(
                    "a try with {} catch arms folded into one: the arms selected by \
                     exception class, and the classes did not cross",
                    catches.len()
                ));
            }
            let first = catches[0].clone();
            let binding = first.binding.clone().unwrap_or_else(|| "_".to_string());
            let mut tried = tried.clone();
            let mut counter = out.lowering_names;
            extract_failing_calls(&mut tried, &out.throwing.clone(), &mut counter);
            out.lowering_names = counter;
            out.line(&format!("{label}: {{"));
            out.open();
            out.zig_try = Some((label.clone(), binding.clone(), first.body.clone()));
            if binding != "_" {
                out.catch_bindings.push(binding);
            }
            let inner_mutated = zig_mutated(&tried);
            for stmt in &tried {
                zig_stmt(out, stmt, &inner_mutated);
            }
            if out.zig_try.take().is_some() && first.binding.is_some() {
                out.catch_bindings.pop();
            }
            out.close();
            out.line("}");
            if !finally.is_empty() {
                let finally_mutated = zig_mutated(finally);
                for stmt in finally {
                    zig_stmt(out, stmt, &finally_mutated);
                }
            }
        }
        Stmt::Expr(Expr::Null) => {}
        // Zig has no bare expression statement: a value has to go somewhere.
        Stmt::Expr(e) => {
            let text = zig_expr(out, e);
            match e {
                Expr::Call { .. } => zig_line(out, &format!("{text};")),
                _ => zig_line(out, &format!("_ = {text};")),
            }
        }
        Stmt::Assert { condition, message } => {
            let c = zig_expr(out, condition);
            // The library check takes no message, so the words the source gave
            // ride above the statement, where a Zig comment can go.
            if let Some(m) = message {
                let rendered = zig_expr(out, m);
                out.pending
                    .push(format!("the assert's message: {rendered}"));
            }
            zig_line(out, &format!("std.debug.assert({c});"));
        }
        Stmt::Break => out.line("break;"),
        Stmt::Continue => out.line("continue;"),
        Stmt::Unsupported(u) => carry(out, u),
    }
}

fn zig_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        // Zig has no string type.
        Type::String => "[]const u8".to_string(),
        Type::List(inner) => format!("[]const {}", zig_type(inner)),
        // Zig has no set either.
        Type::Set(inner) => match inner.as_ref() {
            Type::String => "std.StringHashMap(void)".to_string(),
            other => format!("std.AutoHashMap({}, void)", zig_type(other)),
        },
        // Hashing a slice by its contents and hashing it by its address are different maps in
        // Zig.
        Type::Map(key, value) => match key.as_ref() {
            Type::String => format!("std.StringHashMap({})", zig_type(value)),
            other => format!("std.AutoHashMap({}, {})", zig_type(other), zig_type(value)),
        },
        Type::Optional(inner) => format!("?{}", zig_type(inner)),
        // Zig has no closures, so a function value is a pointer to a function.
        Type::Fn { params, returns } => format!(
            "*const fn ({}) {}",
            joined(params, zig_type),
            zig_type(returns)
        ),
        Type::Tuple(parts) => format!("struct {{ {} }}", joined(parts, zig_type)),
        // The shared `Result<T, E>` is this language's own error union, `E!T`.
        Type::Named { name, args } if name == "Result" && args.len() == 2 => {
            // A qualified error name keeps its path, spelled with dots the way every other Zig
            // name here is: `anyhow::Error` is not something this grammar reads.
            let err = match &args[1] {
                Type::Named { name, args }
                    if args.is_empty() && name != "error" && Type::is_writable_name(name) =>
                {
                    zig_path(&generic(name, &[], "(", ")", ".", zig_type))
                }
                _ => "anyerror".to_string(),
            };
            format!("{err}!{}", zig_type(&args[0]))
        }
        // Apply a generic type rather than bracket it: `ArrayList(u8)`.
        Type::Named { name, args } => {
            let path = zig_path(&generic(name, &[], "(", ")", ".", zig_type));
            if args.is_empty() {
                return path;
            }
            let rendered: Vec<String> = args.iter().map(zig_type).collect();
            format!("{path}({})", rendered.join(", "))
        }
    }
}

/// A type name carried across, with any part Zig reserves written its way.
fn zig_path(name: &str) -> String {
    name.split('.')
        .map(|part| match reserved(Language::Zig, part) {
            true => format!("@\"{part}\""),
            false => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Queue the source of something with no counterpart, and stand `undefined` in for it.
fn zig_carry(out: &mut Out, construct: &str, source: String) -> String {
    out.carried(&Unsupported {
        construct: construct.into(),
        source: source.clone(),
        line: 0,
    });
    out.pending.push(format!("{MARKER}: {source}"));
    "undefined".to_string()
}

/// A map key that is not a string literal, flattened to a field-name spelling.
fn zig_expr_immut_placeholder(e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Name(n) => n.clone(),
        _ => "key".to_string(),
    }
}

fn zig_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Zig's sets go through an allocator, so a literal only makes sense where a binding can
        // hold it.
        Expr::SetLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| zig_expr(out, i)).collect();
            zig_carry(out, "set literal", format!("{{{}}}", rendered.join(", ")))
        }
        // Zig names its fields with a leading dot, in any order.
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!(".{} = {}", out.field(name), zig_expr(out, value)))
                .collect();
            format!("{}{{ {} }}", out.name(ty), rendered.join(", "))
        }
        // Zig has the operator, and means this by it.
        Expr::Coalesce { value, fallback } => format!(
            "{} orelse {}",
            zig_expr(out, value),
            zig_expr(out, fallback)
        ),
        // Zig's `if` is an expression, and takes the branches without braces.
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => format!(
            "if ({}) {} else {}",
            zig_expr(out, condition),
            zig_expr(out, then),
            zig_expr(out, otherwise)
        ),
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Str(v) => quoted(Language::Zig, v),
        Expr::Bool(v) => v.to_string(),
        Expr::Null => "null".to_string(),
        // Read a caught error for its words: here the name carries the message.
        Expr::Name(n) if out.catch_bindings.iter().any(|b| b == n) => {
            format!("@errorName({})", out.value_name(n))
        }
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            let object = receiver(zig_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the parentheses does not
            // exist in this language.
            if out.properties.contains(name) {
                return format!("{object}.{}()", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        // `[…]` on a slice or an array and `.get(…)` on a map, and which this is depends on a
        // type nothing here tracks.
        Expr::Index { of, index } => {
            // Reach a hash map through `get`, which answers an optional.
            if holds_a_map(out, of) {
                let map = receiver(zig_expr(out, of), of);
                let at = zig_expr(out, index);
                return format!("{map}.get({at}).?");
            }
            let mut object = receiver(zig_expr(out, of), of);
            // A growable list's elements live behind `.items`.
            if matches!(&**of, Expr::Name(n) if out.zig_dyn.contains(n.as_str())) {
                object.push_str(".items");
            }
            let at = zig_expr(out, index);
            format!("{object}[{at}]")
        }
        Expr::Call { callee, args } => {
            if reaches_super(callee) && !shadows_builtin(out, "super") {
                let rendered: Vec<String> = args.iter().map(|a| zig_expr(out, a)).collect();
                let source = super_source(callee, &rendered);
                return zig_carry(out, "super", source);
            }
            if let Some(mapped) = zig_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            if let Some(filler) = carried_keywords(out, callee, args, settled.is_some()) {
                return filler;
            }
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| zig_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = zig_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!(".{} = {value}", out.field(field)))
                    .collect();
                return format!("{target}{{ {} }}", pairs.join(", "));
            }
            if let Expr::Name(name) = callee.as_ref() {
                if let Some(base) = out.newtypes.get(name) {
                    let inner = rendered.join(", ");
                    let spelled = out.name(name);
                    return if *base == Type::Int {
                        format!("@as({spelled}, @enumFromInt({inner}))")
                    } else {
                        format!("@as({spelled}, {inner})")
                    };
                }
            }
            format!("{}({})", zig_expr(out, callee), rendered.join(", "))
        }
        // Zig has no `new`.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| zig_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = zig_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!(".{} = {value}", out.field(field)))
                    .collect();
                return format!("{target}{{ {} }}", pairs.join(", "));
            }
            let target = zig_expr(out, callee);
            let named: Option<Vec<String>> = args
                .iter()
                .map(|a| match a {
                    Expr::Keyword { name, value } => {
                        Some(format!(".{} = {}", out.field(name), zig_expr(out, value)))
                    }
                    _ => None,
                })
                .collect();
            match named {
                Some(pairs) if !pairs.is_empty() => {
                    format!("{target}{{ {} }}", pairs.join(", "))
                }
                _ => format!("{target}.init({})", rendered.join(", ")),
            }
        }
        // Floor division is a builtin here, and the builtin rounds the way the
        // source's operator did.
        Expr::Binary {
            op: BinaryOp::FloorDiv,
            left,
            right,
        } => format!(
            "@divFloor({}, {})",
            zig_expr(out, left),
            zig_expr(out, right)
        ),
        // `@mod` is the remainder that goes with `@divFloor`, and `@rem` the one that goes with
        // `@divTrunc`.
        Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        } => format!("@mod({}, {})", zig_expr(out, left), zig_expr(out, right)),
        // Zig refuses `/` and `%` on signed integers outright: the caller has to say which
        // rounding.
        Expr::Binary {
            op: BinaryOp::Div,
            left,
            right,
        } if holds_an_integer(out, left) && holds_an_integer(out, right) => format!(
            "@divTrunc({}, {})",
            zig_expr(out, left),
            zig_expr(out, right)
        ),
        // The language refuses `%` on signed integers outright: the caller chooses a rounding.
        Expr::Binary {
            op: BinaryOp::Rem,
            left,
            right,
        } => format!("@rem({}, {})", zig_expr(out, left), zig_expr(out, right)),
        // Zig refuses `/` on signed integers outright, and the source divided
        // as floats anyway.
        Expr::Binary {
            op: BinaryOp::TrueDiv,
            left,
            right,
        } => {
            let side = |out: &mut Out, e: &Expr| {
                if let Expr::Int(n) = e {
                    return format!("{n}.0");
                }
                let text = binary_operand(zig_expr(out, e), e, BinaryOp::Div, false);
                match static_type(out, e) {
                    Some(Type::Float) => text,
                    _ => format!("@as(f64, @floatFromInt({text}))"),
                }
            };
            format!("{} / {}", side(out, left), side(out, right))
        }
        Expr::Binary { op, left, right } => {
            // A Zig string is a `[]const u8`, and `==` on a slice is not a comparison the
            // compiler will accept at all.
            if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && compares_strings(out, left, right) {
                let call = format!(
                    "std.mem.eql(u8, {}, {})",
                    zig_expr(out, left),
                    zig_expr(out, right)
                );
                return match op {
                    BinaryOp::Ne => format!("!{call}"),
                    _ => call,
                };
            }
            // Joining two slices in Zig means allocating, and this function takes no allocator
            // parameter.
            if *op == BinaryOp::Add && compares_strings(out, left, right) {
                let source = format!("{} + {}", zig_expr(out, left), zig_expr(out, right));
                out.carried(&Unsupported {
                    construct: "joining two strings, which needs an allocator here".into(),
                    source: source.clone(),
                    line: 0,
                });
                // Zig has no block comment, so a marker beside the value would swallow the rest
                // of the line.
                return format!(
                    "@compileError(\"{MARKER}: joining two strings needs an allocator: {}\")",
                    source.replace('"', "'")
                );
            }
            format!(
                "{} {} {}",
                binary_operand(zig_expr(out, left), left, *op, false),
                zig_binary(*op),
                binary_operand(zig_expr(out, right), right, *op, true)
            )
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
                UnaryOp::Unwrap => {
                    return format!("{}.?", unary_operand(zig_expr(out, operand), operand));
                }
            };
            format!("{sign}{}", unary_operand(zig_expr(out, operand), operand))
        }
        // The union's own spelling: a dot-literal for a bare tag, the
        // one-field initializer for a payload.
        Expr::Variant { name, fields, .. } => match fields.as_slice() {
            [] => format!(".{}", snake_always(name)),
            [(single, value)] if single == "value" => {
                format!(".{{ .{} = {} }}", snake_always(name), zig_expr(out, value))
            }
            _ => {
                let rendered = joined(fields, |(f, v)| {
                    format!(".{} = {}", out.field(f), zig_expr(out, v))
                });
                format!(".{{ .{} = .{{ {rendered} }} }}", snake_always(name))
            }
        },
        // Zig's anonymous struct literal is its tuple: `.{ a, b }`.
        Expr::Tuple(items) => format!(".{{ {} }}", joined(items, |i| zig_expr(out, i))),
        // A list literal is an array whose length the compiler counts.
        Expr::ListLit(items) => {
            let element = match items.first().map(|first| zig_hole_spec(out, first)) {
                Some("d") => "i64",
                Some("s") => "[]const u8",
                _ => "i64",
            };
            let rendered: Vec<String> = items.iter().map(|i| zig_expr(out, i)).collect();
            format!("[_]{element}{{ {} }}", rendered.join(", "))
        }
        // Zig's runtime maps go through an allocator.
        Expr::MapLit(entries) => {
            let field = |k: &Expr| match k {
                Expr::Str(text)
                    if !text.is_empty()
                        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !text.starts_with(|c: char| c.is_ascii_digit()) =>
                {
                    format!(".{text}")
                }
                Expr::Str(text) => format!(".@\"{}\"", text.replace('"', "\\\"")),
                other => format!(".@\"{}\"", zig_expr_immut_placeholder(other)),
            };
            let mut rendered: Vec<String> = Vec::new();
            for (k, v) in entries {
                let key = field(k);
                let value = zig_expr(out, v);
                rendered.push(format!("{key} = {value}"));
            }
            format!(".{{ {} }}", rendered.join(", "))
        }
        Expr::Template(parts) => {
            // A template with nothing in it but text is a string, and saying otherwise
            // would report a gap that is not there.
            if let Some(text) = literal_text(parts) {
                return quoted(Language::Zig, &text);
            }
            out.zig_helpers.insert("format");
            let (format, holes) = zig_template(out, parts);
            format!("frFormat(\"{format}\", .{{ {holes} }})")
        }
        // Zig writes no closure without every type spelled, and the source
        // spelled none; inventing them would be a guess about the call sites.
        Expr::Lambda { params, body, .. } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(&p.name)).collect();
            let value = zig_expr(out, body);
            zig_carry(
                out,
                "closure",
                format!("({}) => {value}", rendered.join(", ")),
            )
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let it = zig_expr(out, iterable);
            let name = out.name(binding);
            let filter = condition
                .as_ref()
                .map(|c| format!(" if {}", zig_expr(out, c)))
                .unwrap_or_default();
            let body = zig_expr(out, element);
            // Zig has no iterator adaptors and no way to build a collection without an
            // allocator.
            zig_carry(
                out,
                "comprehension",
                format!("{body} for {name} in {it}{filter}"),
            )
        }
        // Zig asks this with a tagged-union `switch`, which needs the union, and a
        // type arriving from a language with runtime classes does not have one.
        Expr::Cast { ty, value } => {
            format!("@as({}, {})", zig_expr(out, ty), zig_expr(out, value))
        }
        // Zig types are compile-time facts.
        Expr::InstanceOf { value, ty } => {
            out.note_once(
                "an `instanceof` compares types at compile time here: Zig has no runtime type test.",
            );
            let rendered = zig_expr(out, value);
            let named = zig_expr(out, ty);
            format!("@TypeOf({rendered}) == {named}")
        }
        // Zig removed `async` in 0.11.
        Expr::Await(inner) => {
            out.note_once("an `await` runs blocking here: Zig has no async to suspend on.");
            zig_expr(out, inner)
        }
        Expr::Propagate(inner) => format!("try {}", zig_expr(out, inner)),
        // Zig calls positionally and has nothing that names an argument.
        Expr::Keyword { name: _, value } => {
            out.note_once(
                "a named argument passes by position here: the target does not name arguments.",
            );
            zig_expr(out, value)
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            out.pending.push(format!("{MARKER}: {}", u.source));
            "undefined".to_string()
        }
    }
}

/// The text of a template that interpolates nothing, where the node holds one.
fn literal_text(parts: &[TemplatePart]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        match part {
            TemplatePart::Text(piece) => text.push_str(piece),
            TemplatePart::Expr(_) => return None,
        }
    }
    Some(text)
}

/// Zig spells the two logical operators as words and the rest as C does.
fn zig_binary(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        other => other.c_like(),
    }
}

/// A named type, spelled the way this target spells generics.
fn joined<T>(parts: &[T], mut render: impl FnMut(&T) -> String) -> String {
    parts.iter().map(&mut render).collect::<Vec<_>>().join(", ")
}

fn generic(
    name: &str,
    args: &[Type],
    open: &str,
    close: &str,
    // How this language separates the parts of a qualified name: `::` in Rust, `.` in the rest.
    path_separator: &str,
    render: fn(&Type) -> String,
) -> String {
    if !Type::is_writable_name(name) {
        return format!("Unwritable_{}", sanitise(name));
    }
    let clean = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .split("::")
        .flat_map(|part| part.split('.'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(path_separator);
    if args.is_empty() {
        return clean;
    }
    let rendered: Vec<String> = args.iter().map(render).collect();
    format!("{clean}{open}{}{close}", rendered.join(", "))
}

/// A string literal, spelled the way this target spells one.
fn quoted(language: Language, value: &str) -> String {
    format!("\"{}\"", escaped(language, value))
}

/// The inside of a string literal: the escapes, without the quotes around them.
fn escaped(language: Language, value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Every one of these languages takes UTF-8 in a string literal, so a
            // printable character stands for itself whatever its code point.
            c if !c.is_control() => out.push(c),
            // Java has no `\xNN`, so spell both with the form it does have.
            c if language == Language::Java => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\x{:02x}", c as u32)),
        }
    }
    out
}

/// The ok type of `Result<T, E>`, where the name means the shared Result.
fn result_ok(declared: &std::collections::BTreeSet<String>, ty: Option<&Type>) -> Option<Type> {
    match ty {
        Some(Type::Named { name, args })
            if name == "Result" && args.len() == 2 && !declared.contains("Result") =>
        {
            Some(args[0].clone())
        }
        _ => None,
    }
}

/// Does anything under this expression carry no translation?
fn contains_unsupported(e: &Expr) -> bool {
    match e {
        Expr::Unsupported(_) => true,
        Expr::SetLit(items) => items.iter().any(contains_unsupported),
        Expr::Variant { fields, .. } => fields.iter().any(|(_, v)| contains_unsupported(v)),
        Expr::Field { of, .. } => contains_unsupported(of),
        Expr::Index { of, index } => contains_unsupported(of) || contains_unsupported(index),
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            contains_unsupported(callee) || args.iter().any(contains_unsupported)
        }
        Expr::Binary { left, right, .. } => {
            contains_unsupported(left) || contains_unsupported(right)
        }
        Expr::Unary { operand, .. } => contains_unsupported(operand),
        Expr::Await(inner) | Expr::Propagate(inner) => contains_unsupported(inner),
        Expr::Keyword { value, .. } => contains_unsupported(value),
        Expr::Cast { ty, value } => contains_unsupported(ty) || contains_unsupported(value),
        Expr::InstanceOf { value, ty } => contains_unsupported(value) || contains_unsupported(ty),
        Expr::RecordLit { fields, .. } => {
            fields.iter().any(|(_, value)| contains_unsupported(value))
        }
        Expr::Coalesce { value, fallback } => {
            contains_unsupported(value) || contains_unsupported(fallback)
        }
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            contains_unsupported(condition)
                || contains_unsupported(then)
                || contains_unsupported(otherwise)
        }
        Expr::Tuple(items) | Expr::ListLit(items) => items.iter().any(contains_unsupported),
        Expr::MapLit(entries) => entries
            .iter()
            .any(|(k, v)| contains_unsupported(k) || contains_unsupported(v)),
        Expr::Template(parts) => parts.iter().any(|part| match part {
            TemplatePart::Expr(e) => contains_unsupported(e),
            TemplatePart::Text(_) => false,
        }),
        Expr::Comprehension {
            element,
            iterable,
            condition,
            ..
        } => {
            contains_unsupported(element)
                || contains_unsupported(iterable)
                || condition.as_deref().is_some_and(contains_unsupported)
        }
        Expr::Lambda { body, .. } => contains_unsupported(body),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Name(_) => false,
    }
}

/// A returned `Ok(...)` or `Err(...)`, where the value is one.
fn result_call<'e>(out: &Out, value: &'e Expr) -> Option<(bool, Option<&'e Expr>)> {
    let Expr::Call { callee, args } = value else {
        return None;
    };
    let Expr::Name(name) = callee.as_ref() else {
        return None;
    };
    let ok = match name.as_str() {
        "Ok" => true,
        "Err" => false,
        _ => return None,
    };
    if out.functions.contains_key(name) || out.declared_types.contains(name) {
        return None;
    }
    match args.as_slice() {
        [] => Some((ok, None)),
        [Expr::Tuple(items)] if items.is_empty() => Some((ok, None)),
        [one] => Some((ok, Some(one))),
        _ => None,
    }
}

/// Is this Err payload a sum's variant, and what is the variant called?
fn error_variant<'e>(out: &Out, payload: &'e Expr) -> Option<&'e str> {
    let Expr::Field { of, name } = payload else {
        return None;
    };
    matches!(of.as_ref(), Expr::Name(n) if out.sums.contains(n)).then_some(name.as_str())
}

/// The words the exception languages translate a returned `Err` with.
const RESULT_RAISED: &str = "a Result crosses as its own failure here: Ok returns, Err raises.";

/// Is this top-level statement exactly the entry call to a `main` this module declares?
fn calls_declared_main(out: &Out, stmt: &Stmt) -> bool {
    let Stmt::Expr(Expr::Call { callee, .. }) = stmt else {
        return false;
    };
    matches!(callee.as_ref(), Expr::Name(name) if name == "main")
        && out.functions.contains_key("main")
}

/// The words every self-running target drops the entry call with.
const ENTRY_DROPPED: &str = "this drops the source's entry call: the target runs main itself.";

/// The module function the entry statement calls with no arguments, if that is one.
fn entry_function<'m>(module: &'m Module, stmt: &Stmt) -> Option<&'m Function> {
    let Stmt::Expr(Expr::Call { callee, args }) = stmt else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let Expr::Name(name) = callee.as_ref() else {
        return None;
    };
    module.items.iter().find_map(|item| match item {
        Item::Function(f) if f.name == *name => Some(f),
        _ => None,
    })
}

/// How many arguments a call to this function cannot leave out.
fn required_parameters(f: &Function) -> usize {
    f.params
        .iter()
        .filter(|p| p.kind == ParamKind::Normal && p.default.is_none())
        .count()
}

/// Does this entry function receive the program's arguments?
fn entry_takes_arguments(f: &Function) -> bool {
    f.name == "main" && required_parameters(f) == 1
}

/// A top-level statement, in a target whose top level only declares.
fn carried_statement(out: &mut Out, stmt: &Stmt, render: impl FnOnce(&mut Out, &Expr) -> String) {
    let rendered = match stmt {
        Stmt::Expr(e) => render(out, e),
        _ => String::new(),
    };
    out.fidelity.carried_verbatim += 1;
    out.fidelity
        .notes
        .push("at the top level: top-level statement carried over unchanged".to_string());
    let text = match rendered.is_empty() {
        true => format!("{MARKER}: a top-level statement runs here in the source"),
        false => format!("{MARKER}: at the top level the source runs `{rendered}`"),
    };
    out.line(&out.comment(&text));
    out.blank();
}

// The canonical spellings are Python's, because its reader needs no normalising: `print(x)`,
// `len(x)`, `str(x)`, `.append`, `.upper`, `.lower`, `.strip`, and `sep.join(xs)`.

/// The module with every same-module base laid flat into its extenders.
fn flatten_local_bases(module: &Module) -> (Module, Vec<String>) {
    let bases: std::collections::BTreeMap<String, Record> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some((r.name.clone(), r.clone())),
            _ => None,
        })
        .collect();
    let mut notes = Vec::new();
    let mut flattened = module.clone();
    for item in flattened.items.iter_mut() {
        let Item::Record(record) = item else { continue };
        if record.extends.is_none() {
            continue;
        }
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(record.name.clone());
        let mut inherited_fields: Vec<Field> = Vec::new();
        let mut inherited_methods: Vec<Function> = Vec::new();
        let mut absorbed: Vec<String> = Vec::new();
        let mut next = record.extends.clone();
        let mut fully_local = true;
        while let Some(base_name) = next {
            let plain = base_name
                .split('<')
                .next()
                .unwrap_or(base_name.as_str())
                .trim()
                .to_string();
            let Some(base) = bases.get(&plain) else {
                fully_local = false;
                break;
            };
            if !seen.insert(plain.clone()) {
                break;
            }
            for field in &base.fields {
                let taken = record.fields.iter().any(|own| own.name == field.name)
                    || inherited_fields.iter().any(|f| f.name == field.name);
                if !taken {
                    inherited_fields.push(field.clone());
                }
            }
            for method in &base.methods {
                let taken = method.is_constructor
                    || record.methods.iter().any(|own| own.name == method.name)
                    || inherited_methods.iter().any(|m| m.name == method.name);
                if !taken {
                    inherited_methods.push(method.clone());
                }
            }
            absorbed.push(plain);
            next = base.extends.clone();
        }
        if absorbed.is_empty() {
            continue;
        }
        let own_fields = std::mem::take(&mut record.fields);
        record.fields = inherited_fields;
        record.fields.extend(own_fields);
        record.methods.extend(inherited_methods);
        notes.push(format!(
            "`{}` extends `{}`; this language has no inheritance, so what the base \
             holds is laid flat into the record itself.",
            record.name,
            absorbed.join("`, then `")
        ));
        if fully_local {
            record.extends = None;
        }
    }
    (flattened, notes)
}

/// Does this callee reach through `super`, into the base the source called?
fn reaches_super(callee: &Expr) -> bool {
    match callee {
        Expr::Name(n) => n == "super",
        Expr::Field { of, .. } => matches!(of.as_ref(), Expr::Name(n) if n == "super"),
        _ => false,
    }
}

/// The carried spelling of a call through `super`, for the writers without one.
fn super_source(callee: &Expr, rendered: &[String]) -> String {
    let through = match callee {
        Expr::Field { name, .. } => format!("super.{name}"),
        _ => "super".to_string(),
    };
    format!("{through}({})", rendered.join(", "))
}

/// The receiver and name a call's callee spells, where it spells one.
fn callee_parts(callee: &Expr) -> (Option<&Expr>, Option<&str>) {
    match callee {
        Expr::Name(n) => (None, Some(n.as_str())),
        Expr::Field { of, name } => (Some(of), Some(name.as_str())),
        _ => (None, None),
    }
}

/// A module that declares its own `print` or `len` means those, not the builtin.
fn shadows_builtin(out: &Out, name: &str) -> bool {
    out.functions.contains_key(name) || out.declared_types.contains(name)
}

fn rust_builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        // The unsigned right shift, spelled through the unsigned view.
        (None, "ushr", [x, n]) => format!(
            "((({}) as u64) >> ({})) as i64",
            rust_expr(out, x),
            rust_expr(out, n)
        ),
        // The try-with-returns closure's own channel: a return inside the
        // tried body travels out as `Ok(Some(v))`.
        (None, "__fr_ok_some", []) => "Ok(Some(()))".to_string(),
        (None, "__fr_ok_some", [v]) => format!("Ok(Some({}))", rust_expr(out, v)),
        (None, "slice", [of, from, to]) => format!(
            "{}[{}..{}].to_owned()",
            rust_expr(out, of),
            rust_expr(out, from),
            rust_expr(out, to)
        ),
        (None, "print", _) => {
            let holes = vec!["{}"; args.len()].join(" ");
            format!(
                "println!(\"{holes}\"{})",
                args.iter()
                    .map(|a| format!(", {}", rust_expr(out, a)))
                    .collect::<String>()
            )
        }
        // A length is a `usize` here and an ordinary integer everywhere else.
        (None, "len", [x]) => format!("({}.len() as i64)", rust_expr(out, x)),
        // The canonical `int(x)`: a number cut toward zero, which is what `as`
        // does between Rust's own number types.
        (None, "int", [x]) => format!("(({}) as i64)", rust_expr(out, x)),
        // `trunc` cuts toward zero and answers the same kind of number.
        (None, "trunc", [x]) => match static_type(out, x) {
            Some(Type::Float) => format!("({}).trunc()", rust_expr(out, x)),
            _ => rust_expr(out, x),
        },
        (None, "str", [x]) => format!("{}.to_string()", rust_expr(out, x)),
        // A set spells the four collection words its own way, and `insert` is
        // the one that differs from every other target's.
        (Some(of), "add", [x]) if holds_a_set(out, of) => {
            // A set of `String` takes an owned one, and a literal is a `&str`.
            let owns = matches!(
                out.binding_types.get(match of {
                    Expr::Name(n) => n.as_str(),
                    _ => "",
                }),
                Some(Type::Set(inner)) if **inner == Type::String
            );
            let member = match (owns, x) {
                (true, Expr::Str(_)) => format!("{}.to_string()", rust_expr(out, x)),
                _ => rust_expr(out, x),
            };
            format!("{}.insert({member})", rust_expr(out, &of.clone()))
        }
        (Some(of), "remove", [x]) if holds_a_set(out, of) => {
            // `remove` takes a borrow of what the set can be looked up by.
            let borrows = !matches!(x, Expr::Str(_));
            let member = match borrows {
                true => format!("&{}", rust_expr(out, x)),
                false => rust_expr(out, x),
            };
            format!("{}.remove({member})", rust_expr(out, &of.clone()))
        }
        (Some(of), "contains", [x]) if holds_a_set(out, of) => format!(
            "{}.contains({})",
            rust_expr(out, &of.clone()),
            rust_expr(out, x)
        ),
        (Some(of), "append", [x]) => {
            let mut pushed = rust_expr(out, x);
            // An integer literal into a float list takes the float spelling.
            if matches!(static_type(out, of), Some(Type::List(inner)) if *inner == Type::Float)
                && matches!(x, Expr::Int(_))
            {
                pushed.push_str(".0");
            }
            format!("{}.push({pushed})", rust_expr(out, &of.clone()))
        }
        (Some(of), "upper", []) => format!("{}.to_uppercase()", rust_expr(out, &of.clone())),
        (Some(of), "lower", []) => format!("{}.to_lowercase()", rust_expr(out, &of.clone())),
        (Some(of), "strip", []) => format!("{}.trim()", rust_expr(out, &of.clone())),
        (Some(of), "join", [xs]) if matches!(of, Expr::Str(_)) => {
            format!(
                "{}.join({})",
                rust_expr(out, xs),
                rust_expr(out, &of.clone())
            )
        }
        _ => return None,
    })
}

fn ts_builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        // The unsigned right shift; the operator here is 32-bit.
        (None, "ushr", [x, n]) => format!("({}) >>> ({})", ts_expr(out, x), ts_expr(out, n)),
        (None, "slice", [of, from, to]) => format!(
            "{}.slice({}, {})",
            ts_expr(out, of),
            ts_expr(out, from),
            ts_expr(out, to)
        ),
        (None, "print", _) => {
            let rendered = joined(args, |a| ts_expr(out, a));
            format!("console.log({rendered})")
        }
        // An object has no `.length`.
        (None, "len", [x]) if holds_a_map(out, x) => {
            format!("Object.keys({}).length", ts_expr(out, x))
        }
        (None, "len", [x]) if holds_a_set(out, x) => format!("{}.size", ts_expr(out, x)),
        (None, "len", [x]) => format!("{}.length", ts_expr(out, x)),
        (None, "int", [x]) => format!("Math.trunc({})", ts_expr(out, x)),
        (None, "trunc", [x]) => format!("Math.trunc({})", ts_expr(out, x)),
        // Inside a catch, the exception as text is its message: `String(e)` leads
        // with the class name, which is not what the source printed.
        (None, "str", [Expr::Name(bound)]) if out.catch_bindings.iter().any(|b| b == bound) => {
            format!("({} as Error).message", out.name(bound))
        }
        (None, "str", [x]) => format!("String({})", ts_expr(out, x)),
        (Some(of), "append", [x]) => {
            format!("{}.push({})", ts_expr(out, &of.clone()), ts_expr(out, x))
        }
        (Some(of), "add", [x]) if holds_a_set(out, of) => {
            format!("{}.add({})", ts_expr(out, &of.clone()), ts_expr(out, x))
        }
        (Some(of), "remove", [x]) if holds_a_set(out, of) => {
            format!("{}.delete({})", ts_expr(out, &of.clone()), ts_expr(out, x))
        }
        (Some(of), "contains", [x]) if holds_a_set(out, of) => {
            format!("{}.has({})", ts_expr(out, &of.clone()), ts_expr(out, x))
        }
        (Some(of), "contains", [x]) => {
            format!(
                "{}.includes({})",
                ts_expr(out, &of.clone()),
                ts_expr(out, x)
            )
        }
        (Some(of), "upper", []) => format!("{}.toUpperCase()", ts_expr(out, &of.clone())),
        (Some(of), "lower", []) => format!("{}.toLowerCase()", ts_expr(out, &of.clone())),
        (Some(of), "strip", []) => format!("{}.trim()", ts_expr(out, &of.clone())),
        (Some(of), "join", [xs]) => {
            format!("{}.join({})", ts_expr(out, xs), ts_expr(out, &of.clone()))
        }
        _ => return None,
    })
}

fn go_builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        // The unsigned right shift, spelled through the unsigned view.
        (None, "ushr", [x, n]) => format!(
            "int64(uint64({}) >> uint({}))",
            go_expr(out, x),
            go_expr(out, n)
        ),
        (None, "slice", [of, from, to]) => format!(
            "{}[{}:{}]",
            go_expr(out, of),
            go_expr(out, from),
            go_expr(out, to)
        ),
        (None, "print", _) => {
            out.go_imports.insert("fmt");
            let rendered = joined(args, |a| go_expr(out, a));
            format!("fmt.Println({rendered})")
        }
        (None, "int", [x]) => format!("int({})", go_expr(out, x)),
        (None, "trunc", [x]) => match static_type(out, x) {
            Some(Type::Float) => {
                out.go_imports.insert("math");
                format!("math.Trunc({})", go_expr(out, x))
            }
            _ => go_expr(out, x),
        },
        (None, "str", [x]) => {
            out.go_imports.insert("fmt");
            format!("fmt.Sprint({})", go_expr(out, x))
        }
        (Some(of), "upper", []) => {
            out.go_imports.insert("strings");
            format!("strings.ToUpper({})", go_expr(out, &of.clone()))
        }
        (Some(of), "lower", []) => {
            out.go_imports.insert("strings");
            format!("strings.ToLower({})", go_expr(out, &of.clone()))
        }
        (Some(of), "strip", []) => {
            out.go_imports.insert("strings");
            format!("strings.TrimSpace({})", go_expr(out, &of.clone()))
        }
        (Some(of), "join", [xs]) => {
            out.go_imports.insert("strings");
            format!(
                "strings.Join({}, {})",
                go_expr(out, xs),
                go_expr(out, &of.clone())
            )
        }
        // A Go set is a map to `bool`.
        (Some(of), "add", [x]) if holds_a_set(out, of) => format!(
            "{}[{}] = struct{{}}{{}}",
            go_expr(out, &of.clone()),
            go_expr(out, x)
        ),
        (Some(of), "remove", [x]) if holds_a_set(out, of) => {
            format!("delete({}, {})", go_expr(out, &of.clone()), go_expr(out, x))
        }
        // Membership needs the two-value read, which only an `if` header has room for.
        (Some(of), "contains", [x]) if holds_a_set(out, of) => {
            let asked = format!("{}[{}]", go_expr(out, &of.clone()), go_expr(out, x));
            out.carried(&Unsupported {
                construct: "asking about membership outside an `if`".into(),
                source: asked.clone(),
                line: 0,
            });
            format!("false /* {MARKER}: {asked} */")
        }
        (Some(of), "contains", [x]) => {
            out.go_imports.insert("strings");
            format!(
                "strings.Contains({}, {})",
                go_expr(out, &of.clone()),
                go_expr(out, x)
            )
        }
        _ => return None,
    })
}

fn java_builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        // `xs.sort()` sorts in place by natural order.
        (Some(of), "sort", []) => {
            format!(
                "java.util.Collections.sort({})",
                java_expr(out, &of.clone())
            )
        }
        // The unsigned right shift is native here.
        (None, "ushr", [x, n]) => {
            format!("({}) >>> ({})", java_expr(out, x), java_expr(out, n))
        }
        (None, "slice", [of, from, to]) => format!(
            "{}.substring({}, {})",
            java_expr(out, of),
            java_expr(out, from),
            java_expr(out, to)
        ),
        (None, "print", _) => {
            // Every argument that is itself an operation takes brackets: joined
            // bare, `"diff" + " " + a - b` subtracts from a string.
            let rendered = args
                .iter()
                .map(|a| match a {
                    Expr::Binary { .. } => format!("({})", java_expr(out, a)),
                    _ => java_expr(out, a),
                })
                .collect::<Vec<_>>()
                .join(" + \" \" + ");
            format!("System.out.println({rendered})")
        }
        // Inside a catch, the exception as text is its message: `String.valueOf(e)`
        // leads with the class name, which is not what the source printed.
        (None, "str", [Expr::Name(bound)]) if out.catch_bindings.iter().any(|b| b == bound) => {
            format!("{}.getMessage()", out.name(bound))
        }
        (None, "str", [x]) => format!("String.valueOf({})", java_expr(out, x)),
        // A set answers Java's own four words, so only `len` needs saying.
        (None, "len", [x]) if holds_a_set(out, x) => format!("{}.size()", java_expr(out, x)),
        // Spell `len` by what it measures: a list answers `size()`, text `length()`.
        (None, "len", [x]) => {
            let spelled = match static_type(out, x) {
                Some(Type::String) => "length()",
                _ => "size()",
            };
            format!("{}.{spelled}", java_expr(out, &x.clone()))
        }
        // A narrowing cast in Java cuts toward zero, which is what this means.
        (None, "int", [x]) => format!("(int) ({})", java_expr(out, x)),
        // Through `long` and back: the cast cuts toward zero and the result stays the kind of
        // number the source had.
        (None, "trunc", [x]) => match static_type(out, x) {
            Some(Type::Float) => format!("(double) (long) ({})", java_expr(out, x)),
            _ => java_expr(out, x),
        },
        (Some(of), "append", [x]) => {
            format!("{}.add({})", java_expr(out, &of.clone()), java_expr(out, x))
        }
        (Some(of), "upper", []) => format!("{}.toUpperCase()", java_expr(out, &of.clone())),
        (Some(of), "lower", []) => format!("{}.toLowerCase()", java_expr(out, &of.clone())),
        (Some(of), "join", [xs]) => {
            format!(
                "String.join({}, {})",
                java_expr(out, &of.clone()),
                java_expr(out, xs)
            )
        }
        _ => return None,
    })
}

fn zig_builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        // The unsigned right shift, spelled through the unsigned view.
        (None, "ushr", [x, n]) => format!(
            "@as(i64, @bitCast(@as(u64, @bitCast({})) >> @as(u6, @intCast({}))))",
            zig_expr(out, x),
            zig_expr(out, n)
        ),
        (None, "slice", [of, from, to]) => format!(
            "{}[{}..{}]",
            zig_expr(out, of),
            zig_expr(out, from),
            zig_expr(out, to)
        ),
        // Into a helper over stdout rather than `std.debug.print`.
        (None, "print", _) => {
            out.zig_helpers.insert("print");
            // A lone template arg spreads into the format string; anything else becomes one
            // hole per argument, space-separated the way every other target's print separates
            // them.
            if let [Expr::Template(parts)] = args {
                let (format, holes) = zig_template(out, parts);
                return Some(format!("frPrint(\"{format}\\n\", .{{ {holes} }})"));
            }
            let specs: Vec<String> = args
                .iter()
                .map(|a| format!("{{{}}}", zig_hole_spec(out, a)))
                .collect();
            let rendered = joined(args, |a| zig_expr(out, a));
            format!("frPrint(\"{}\\n\", .{{ {rendered} }})", specs.join(" "))
        }
        (None, "len", [Expr::Name(n)]) if out.zig_dyn.contains(n.as_str()) => {
            format!("@as(i64, @intCast({}.items.len))", out.value_name(n))
        }
        // A hash map answers `count()`; `len` is a field only a slice has.
        (None, "len", [x]) if holds_a_map(out, x) => {
            format!("@as(i64, @intCast({}.count()))", zig_expr(out, x))
        }
        (None, "len", [x]) if holds_a_set(out, x) => {
            format!("@as(i64, @intCast({}.count()))", zig_expr(out, x))
        }
        (None, "len", [x]) => format!("@as(i64, @intCast({}.len))", zig_expr(out, x)),
        // Zig names the conversion by what it converts from, and it truncates.
        (None, "int", [x]) => match static_type(out, x) {
            Some(Type::Float) => format!("@as(i64, @intFromFloat({}))", zig_expr(out, x)),
            _ => format!("@as(i64, @intCast({}))", zig_expr(out, x)),
        },
        (None, "trunc", [x]) => match static_type(out, x) {
            Some(Type::Float) => format!("@trunc({})", zig_expr(out, x)),
            _ => zig_expr(out, x),
        },
        // A growable list appends through the allocator the lowering owns.
        (Some(of), "append", [x]) => {
            let target = zig_expr(out, &of.clone());
            format!(
                "{target}.append(std.heap.page_allocator, {}) catch unreachable",
                zig_expr(out, x)
            )
        }
        // A Zig set is a hash map whose values carry nothing.
        (Some(of), "add", [x]) if holds_a_set(out, of) => format!(
            "{}.put({}, {{}}) catch unreachable",
            zig_expr(out, &of.clone()),
            zig_expr(out, x)
        ),
        (Some(of), "remove", [x]) if holds_a_set(out, of) => format!(
            "_ = {}.remove({})",
            zig_expr(out, &of.clone()),
            zig_expr(out, x)
        ),
        (Some(of), "contains", [x]) if holds_a_set(out, of) => format!(
            "{}.contains({})",
            zig_expr(out, &of.clone()),
            zig_expr(out, x)
        ),
        (Some(of), "contains", [x]) => {
            format!(
                "std.mem.indexOf(u8, {}, {}) != null",
                zig_expr(out, &of.clone()),
                zig_expr(out, x)
            )
        }
        (Some(of), "upper", []) => {
            format!(
                "std.ascii.allocUpperString(std.heap.page_allocator, {}) catch unreachable",
                zig_expr(out, &of.clone())
            )
        }
        (Some(of), "lower", []) => {
            format!(
                "std.ascii.allocLowerString(std.heap.page_allocator, {}) catch unreachable",
                zig_expr(out, &of.clone())
            )
        }
        // `str` of something already text is the text: a Zig string is a slice of bytes, and a
        // literal is one from birth.
        (None, "str", [x @ (Expr::Str(_) | Expr::Template(_))]) => zig_expr(out, x),
        (None, "str", [Expr::Name(bound)]) if out.catch_bindings.iter().any(|b| b == bound) => {
            format!("@errorName({})", out.value_name(bound))
        }
        (None, "str", [x]) => {
            out.zig_helpers.insert("format");
            let spec = zig_hole_spec(out, x);
            format!("frFormat(\"{{{spec}}}\", .{{ {} }})", zig_expr(out, x))
        }
        _ => return None,
    })
}

/// The message as an error identifier: Zig's errors are names, not strings.
fn zig_error_name(message: &str) -> String {
    let mut name: String = message
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

/// One failing call as Zig spells it: `try` where the failure moves outward, or a
/// `catch` that runs the handler and leaves the block where a `try/catch` stands.
fn zig_failing_call(out: &mut Out, inner: &Expr) -> String {
    let call = zig_expr(out, inner);
    match out.zig_try.clone() {
        Some((label, binding, handler)) => {
            let mut text = format!("{call} catch |{binding}| {{");
            let mut body = Out::new(Language::Zig);
            body.names = out.names.clone();
            body.fields = out.fields.clone();
            body.zig_helpers = std::mem::take(&mut out.zig_helpers);
            body.catch_bindings = out.catch_bindings.clone();
            body.binding_types = out.binding_types.clone();
            body.function_returns = out.function_returns.clone();
            body.indent = out.indent + 1;
            let mutated = zig_mutated(&handler);
            for stmt in &handler {
                zig_stmt(&mut body, stmt, &mutated);
            }
            out.zig_helpers = std::mem::take(&mut body.zig_helpers);
            out.fidelity.notes.extend(body.fidelity.notes);
            out.fidelity.carried_verbatim += body.fidelity.carried_verbatim;
            text.push('\n');
            text.push_str(&body.text);
            for _ in 0..out.indent + 1 {
                text.push_str("    ");
            }
            text.push_str(&format!("break :{label};"));
            text.push('\n');
            for _ in 0..out.indent {
                text.push_str("    ");
            }
            text.push('}');
            text
        }
        None => format!("try {call}"),
    }
}

/// A template's format string and its comma-joined hole expressions, Zig-spelled.
fn zig_template(out: &mut Out, parts: &[TemplatePart]) -> (String, String) {
    let mut format = String::new();
    let mut holes: Vec<String> = Vec::new();
    for part in parts {
        match part {
            TemplatePart::Text(text) => {
                // Braces are format syntax and escape by doubling; the rest escapes
                // the way any Zig string does, minus the quotes `quoted` adds.
                let text = text.replace('{', "{{").replace('}', "}}");
                let quoted = quoted(Language::Zig, &text);
                // Exactly the two delimiters.
                let inner = quoted
                    .strip_prefix('"')
                    .and_then(|q| q.strip_suffix('"'))
                    .unwrap_or(&quoted);
                format.push_str(inner);
            }
            TemplatePart::Expr(e) => {
                format.push_str(&format!("{{{}}}", zig_hole_spec(out, e)));
                holes.push(zig_expr(out, e));
            }
        }
    }
    (format, holes.join(", "))
}

/// The format spec one hole takes: `d` for a number, `s` for text, `any` otherwise.
fn zig_hole_spec(out: &Out, e: &Expr) -> &'static str {
    match e {
        Expr::Int(_) | Expr::Float(_) => "d",
        Expr::Str(_) | Expr::Template(_) => "s",
        Expr::Name(name) => match out.binding_types.get(name.as_str()) {
            _ if out.catch_bindings.iter().any(|b| b == name) => "s",
            Some(Type::Int) | Some(Type::Float) => "d",
            Some(Type::String) => "s",
            _ if out.zig_strings.contains(name.as_str()) => "s",
            _ => "any",
        },
        Expr::Call { callee, .. } => match callee.as_ref() {
            Expr::Name(name) if name == "str" => "s",
            Expr::Name(name) if name == "int" || name == "len" => "d",
            Expr::Name(name) => match out.function_returns.get(name.as_str()) {
                Some(Type::Int) | Some(Type::Float) => "d",
                Some(Type::String) => "s",
                _ => "any",
            },
            Expr::Field { name, .. }
                if matches!(name.as_str(), "upper" | "lower" | "strip" | "join") =>
            {
                "s"
            }
            _ => "any",
        },
        Expr::Binary {
            op:
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::FloorDiv
                | BinaryOp::Rem,
            left,
            right,
        } => match (zig_hole_spec(out, left), zig_hole_spec(out, right)) {
            ("d", _) | (_, "d") => "d",
            _ => "any",
        },
        Expr::Binary { .. } => "any",
        _ => "any",
    }
}

/// The type this record extends, where the target can express one.
fn inherited_base(out: &mut Out, record: &Record, inheritable: bool) -> Option<String> {
    let base = record.extends.clone()?;
    if inheritable {
        return Some(base);
    }
    // Said in the output too, beside the type, because that file is the one a reader of the
    // draft has in front of them.
    out.line(&out.comment(&format!(
        "{MARKER}: extends {base}; whatever `{base}` contributed is not here"
    )));
    out.fidelity.notes.push(format!(
        "`{}` extends `{base}` in the source; {} has no inheritance, so whatever \
         `{base}` contributed is not here",
        record.name, out.language
    ));
    None
}

/// What this target calls a function that makes a value of `owner`.
fn constructor_name(language: Language, owner: &str) -> String {
    match language {
        Language::Python => "__init__".to_string(),
        Language::TypeScript | Language::Tsx => "constructor".to_string(),
        Language::Java => pascal(owner),
        Language::Go => format!("New{}", pascal(owner)),
        Language::Zig => "init".to_string(),
        _ => "new".to_string(),
    }
}

/// A record's methods, with only the first constructor left as one.
fn receiver_assignments(method: &Function, record: &str) -> Option<Vec<Stmt>> {
    let [Stmt::Return(Some(Expr::RecordLit { ty, fields }))] = method.body.as_slice() else {
        return None;
    };
    if ty != record {
        return None;
    }
    let receiver = method.receiver_binding.clone()?;
    Some(
        fields
            .iter()
            .map(|(name, value)| Stmt::Assign {
                target: Expr::Field {
                    of: Box::new(Expr::Name(receiver.clone())),
                    name: name.clone(),
                },
                value: value.clone(),
            })
            .collect(),
    )
}

fn methods_of(out: &mut Out, record: &Record, overloads_allowed: bool) -> Vec<Function> {
    let mut seen = false;
    let mut methods = record.methods.clone();
    for method in methods.iter_mut() {
        if !method.is_constructor {
            continue;
        }
        // Whether a constructor takes a receiver, and whether it says what it returns, is a
        // fact about the target and not about the source.
        match out.language {
            // A constructor here acts on a value that already exists, so it takes the receiver
            // and says nothing about what it returns.
            Language::Python | Language::Java | Language::TypeScript | Language::Tsx => {
                if method.receiver_binding.is_none() {
                    method.receiver_binding = Some(receiver_word(out.language).to_string());
                }
                method.returns = None;
                // A source that builds and returns its record, `Counter { value.
                if let Some(assignments) = receiver_assignments(method, &record.name) {
                    method.body = assignments;
                }
            }
            // The other three have no constructor, only a habit: a plain function that
            // *returns* the type, which is the whole of what makes it one.
            _ => {
                // The canonical build-and-return body is already this shape, whatever the
                // source bound as a receiver.
                let builds_and_returns = matches!(
                    method.body.as_slice(),
                    [Stmt::Return(Some(Expr::RecordLit { ty, .. }))] if *ty == record.name
                );
                let assigns_through_a_receiver =
                    method.receiver_binding.is_some() && !builds_and_returns;
                if !method.body.is_empty() && assigns_through_a_receiver {
                    out.fidelity.notes.push(format!(
                        "`{}` has a constructor whose body assigns through a receiver; \
                         {} builds a value and returns it instead, so that body has no \
                         counterpart and is not here",
                        record.name, out.language
                    ));
                }
                if assigns_through_a_receiver {
                    method.body = Vec::new();
                }
                method.receiver_binding = None;
                method.returns = Some(Type::named(record.name.clone()));
            }
        }
        if seen && !overloads_allowed {
            method.is_constructor = false;
            out.fidelity.notes.push(format!(
                "`{}` declares more than one constructor and {} allows one; this \
                 becomes an ordinary function called `{}`",
                record.name,
                out.language,
                out.name(&method.name)
            ));
        }
        seen = true;
    }
    methods
}

/// Text that can sit inside a `/* ...
fn block_comment_safe(text: &str) -> String {
    text.replace("*/", "* /")
}

/// Can a writer name this value twice without doing anything twice?
fn nameable(e: &Expr) -> bool {
    match e {
        Expr::Name(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Null => true,
        Expr::Field { of, .. } => nameable(of),
        Expr::Index { of, index } => nameable(of) && nameable(index),
        _ => false,
    }
}

/// A name that is not a type, turned into something that at least parses.
fn sanitise(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Is this a name a target can put where a name goes?
fn is_writable_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$' | '#'))
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

/// Can a writer spell this as a bare object key?
fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !text.chars().next().is_some_and(|c| c.is_numeric())
}

/// Is this a type the IR only knows the name of?
fn returned_values(f: &Function) -> Vec<&Expr> {
    fn walk<'a>(stmts: &'a [Stmt], into: &mut Vec<&'a Expr>) {
        for stmt in stmts {
            match stmt {
                Stmt::Return(Some(value)) => into.push(value),
                Stmt::If {
                    then, otherwise, ..
                }
                | Stmt::IfPresent {
                    then, otherwise, ..
                } => {
                    walk(then, into);
                    walk(otherwise, into);
                }
                Stmt::While { body, .. }
                | Stmt::WhilePresent { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachIndexed { body, .. }
                | Stmt::Defer(body)
                | Stmt::ErrDefer(body)
                | Stmt::Block(body) => walk(body, into),
                Stmt::CountedFor { body, .. } => walk(body, into),
                Stmt::Switch { arms, default, .. } => {
                    for (_, body) in arms {
                        walk(body, into);
                    }
                    walk(default, into);
                }
                Stmt::MatchVariants { arms, default, .. } => {
                    for arm in arms {
                        walk(&arm.body, into);
                    }
                    walk(default, into);
                }
                Stmt::Try {
                    body,
                    catches,
                    finally,
                    ..
                } => {
                    walk(body, into);
                    for catch in catches {
                        walk(&catch.body, into);
                    }
                    walk(finally, into);
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    walk(&f.body, &mut found);
    found
}

/// Does this function hand a value back, whatever the source said about it?
fn returns_a_value(f: &Function) -> bool {
    !returned_values(f).is_empty()
}

/// The type every return of this body agrees on, where they agree.
fn inferred_return(out: &Out, f: &Function) -> Option<Type> {
    let mut found: Option<Type> = None;
    for value in returned_values(f) {
        let ty = static_type(out, value)?;
        match &found {
            None => found = Some(ty),
            Some(first) if *first == ty => {}
            Some(_) => return None,
        }
    }
    found
}

fn unknown(out: &mut Out, of: &str) -> String {
    out.fidelity
        .notes
        .push(format!("`{of}` had no declared type in the source"));
    match out.language {
        // `()` is the type of no value, not of an unknown one.
        Language::Rust => TYPE_THE_CALLER_DECIDES.to_string(),
        Language::Python => "object".to_string(),
        Language::Go => "any".to_string(),
        // Zig has no dynamic type; `anytype` says the caller decides, which is exactly
        // true of a parameter whose type the source never wrote down.
        Language::Zig => "anytype".to_string(),
        // `unknown` is a type in TypeScript and a word in Java, where the widest one is
        // `Object`.
        Language::Java => "Object".to_string(),
        _ => "unknown".to_string(),
    }
}

/// The expressions a statement holds directly, for rewriting in place.
fn statement_expressions_mut(stmt: &mut Stmt) -> Vec<&mut Expr> {
    match stmt {
        Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => vec![e],
        Stmt::Let { value: Some(e), .. } => vec![e],
        Stmt::Assign { target, value } => vec![target, value],
        Stmt::If { condition, .. } | Stmt::While { condition, .. } => vec![condition],
        Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => vec![iterable],
        Stmt::WhilePresent { value, .. } | Stmt::IfPresent { value, .. } => vec![value],
        Stmt::Switch { subject, .. } | Stmt::MatchVariants { subject, .. } => vec![subject],
        Stmt::TupleAssign { value, .. } => vec![value],
        Stmt::Assert { condition, message } => match message {
            Some(m) => vec![condition, m],
            None => vec![condition],
        },
        _ => Vec::new(),
    }
}

/// The expressions an expression holds, for rewriting in place.
fn subexpressions_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            let mut out = vec![&mut **callee];
            out.extend(args.iter_mut());
            out
        }
        Expr::Binary { left, right, .. } => vec![&mut **left, &mut **right],
        Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
            vec![&mut **operand]
        }
        Expr::Field { of, .. } => vec![&mut **of],
        Expr::Index { of, index } => vec![&mut **of, &mut **index],
        Expr::Cast { value, .. } => vec![&mut **value],
        Expr::InstanceOf { value, .. } => vec![&mut **value],
        Expr::Keyword { value, .. } => vec![&mut **value],
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => vec![&mut **condition, &mut **then, &mut **otherwise],
        Expr::ListLit(items) | Expr::Tuple(items) => items.iter_mut().collect(),
        Expr::MapLit(entries) => entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
        Expr::RecordLit { fields, .. } => fields.iter_mut().map(|(_, v)| v).collect(),
        Expr::Template(parts) => parts
            .iter_mut()
            .filter_map(|p| match p {
                TemplatePart::Expr(inner) => Some(inner),
                TemplatePart::Text(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Every comprehension in this module, written as the loop that builds it.
fn numbers_as_declared(module: &Module) -> Module {
    fn spell(e: &mut Expr, wanted: &Type) {
        match (&mut *e, wanted) {
            (Expr::Int(n), Type::Float) => *e = Expr::Float(format!("{n}.0")),
            // `-7` is a negated `7`, and it is the `7` that carries the spelling.
            (
                Expr::Unary {
                    op: UnaryOp::Neg,
                    operand,
                },
                Type::Float,
            ) => spell(operand, wanted),
            (Expr::ListLit(items), Type::List(element)) => {
                for item in items {
                    spell(item, element);
                }
            }
            (Expr::MapLit(entries), Type::Map(_, value)) => {
                for (_, held) in entries {
                    spell(held, value);
                }
            }
            _ => {}
        }
    }
    fn walk(
        body: &mut [Stmt],
        fields_of: &std::collections::BTreeMap<String, Vec<(String, Type)>>,
    ) {
        for stmt in body.iter_mut() {
            if let Stmt::Let {
                ty: Some(ty),
                value: Some(value),
                ..
            } = stmt
            {
                spell(value, ty);
            }
            // Give a field declared fractional a fractional literal, wherever the body
            // builds the record.
            for e in statement_expressions_mut(stmt) {
                in_record_literals(e, fields_of);
            }
            for inner in sub_bodies_mut(stmt) {
                walk(inner, fields_of);
            }
        }
    }
    fn in_record_literals(
        e: &mut Expr,
        fields_of: &std::collections::BTreeMap<String, Vec<(String, Type)>>,
    ) {
        match e {
            Expr::RecordLit { ty, fields } => {
                if let Some(declared) = fields_of.get(ty) {
                    for (name, value) in fields.iter_mut() {
                        if let Some((_, wanted)) = declared.iter().find(|(f, _)| f == name) {
                            spell(value, wanted);
                        }
                    }
                }
            }
            // `new Box(9)` fills the fields in declaration order.
            Expr::New { callee, args } | Expr::Call { callee, args } => {
                if let Expr::Name(ty) = callee.as_ref() {
                    if let Some(declared) = fields_of.get(ty) {
                        for (argument, (_, wanted)) in args.iter_mut().zip(declared) {
                            spell(argument, wanted);
                        }
                    }
                }
            }
            _ => {}
        }
        for inner in subexpressions_mut(e) {
            in_record_literals(inner, fields_of);
        }
    }
    let fields_of: std::collections::BTreeMap<String, Vec<(String, Type)>> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some((
                r.name.clone(),
                r.fields
                    .iter()
                    .filter_map(|f| f.ty.clone().map(|ty| (f.name.clone(), ty)))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let mut spelled = module.clone();
    for item in spelled.items.iter_mut() {
        match item {
            Item::Function(f) => walk(&mut f.body, &fields_of),
            Item::Record(r) => {
                for m in r.methods.iter_mut() {
                    walk(&mut m.body, &fields_of);
                }
            }
            Item::Statement(stmt) => {
                let mut one = vec![stmt.clone()];
                walk(&mut one, &fields_of);
                *stmt = one.remove(0);
            }
            _ => {}
        }
    }
    spelled
}

/// Every named lambda that captures nothing, lifted to a function of its own.
fn functions_for_lambdas(module: &Module) -> Module {
    let mut lifted = module.clone();
    let mut made: Vec<Item> = Vec::new();
    for item in lifted.items.iter_mut() {
        let Item::Function(f) = item else { continue };
        let bound: std::collections::BTreeSet<String> = f
            .params
            .iter()
            .map(|p| p.name.clone())
            .chain(declared_names(&f.body))
            .collect();
        lift_in(&mut f.body, &bound, &mut made);
    }
    // A lifted function has to stand before the entry statement that runs the
    // program, or Zig sees a call to a name declared nowhere yet.
    let at = lifted
        .items
        .iter()
        .position(|i| matches!(i, Item::Statement(_)))
        .unwrap_or(lifted.items.len());
    for (offset, made) in made.into_iter().enumerate() {
        lifted.items.insert(at + offset, made);
    }
    lifted
}

/// Every name these statements bind, so a lambda reading one is known to capture.
fn declared_names(body: &[Stmt]) -> Vec<String> {
    let mut found = Vec::new();
    for stmt in body {
        match stmt {
            Stmt::Let { name, .. } => found.push(name.clone()),
            Stmt::ForEach { binding, .. } => found.push(binding.clone()),
            _ => {}
        }
        for inner in sub_bodies(stmt) {
            found.extend(declared_names(inner));
        }
    }
    found
}

/// Replace each liftable lambda in these statements with the name of a function.
fn lift_in(body: &mut Vec<Stmt>, bound: &std::collections::BTreeSet<String>, made: &mut Vec<Item>) {
    // Take the vector by value: `retain` needs one, and the markers go at the end.
    for stmt in body.iter_mut() {
        // A binding whose whole value is a lambda keeps its name: `add_one`
        // becomes `fn addOne`, which is what a reader of the source expects.
        if let Stmt::Let {
            name,
            value: Some(value),
            ..
        } = stmt
        {
            if let Some(f) = liftable(value, name, bound) {
                made.push(Item::Function(f));
                *stmt = Stmt::Comment(format!("{name} is a function below"));
                continue;
            }
        }
        for inner in sub_bodies_mut(stmt) {
            lift_in(inner, bound, made);
        }
    }
    // The comments left where a binding stood say nothing Zig needs.
    body.retain(|s| !matches!(s, Stmt::Comment(text) if text.ends_with("is a function below")));
}

/// The function a lambda stands for, when it reads nothing from around it.
fn liftable(e: &Expr, name: &str, bound: &std::collections::BTreeSet<String>) -> Option<Function> {
    let Expr::Lambda {
        params,
        returns,
        body,
    } = e
    else {
        return None;
    };
    // Every type spelled, or the function has no signature to write.
    if params.iter().any(|p| p.ty.is_none()) || returns.is_none() {
        return None;
    }
    let its_own: std::collections::BTreeSet<&str> =
        params.iter().map(|p| p.name.as_str()).collect();
    let mut reads = Vec::new();
    names_read(body, &mut reads);
    if reads
        .iter()
        .any(|n| bound.contains(n) && !its_own.contains(n.as_str()))
    {
        return None;
    }
    Some(Function {
        doc: Vec::new(),
        name: name.to_string(),
        receiver: None,
        receiver_binding: None,
        params: params.clone(),
        returns: returns.clone(),
        body: vec![Stmt::Return(Some((**body).clone()))],
        exported: false,
        is_async: false,
        is_property: false,
        is_constructor: false,
        is_private: false,
    })
}

/// Every name this expression reads.
fn names_read(e: &Expr, found: &mut Vec<String>) {
    let mut e = e.clone();
    fn walk(e: &mut Expr, found: &mut Vec<String>) {
        if let Expr::Name(n) = e {
            found.push(n.clone());
        }
        for inner in subexpressions_mut(e) {
            walk(inner, found);
        }
    }
    walk(&mut e, found);
}

fn loops_for_comprehensions(module: &Module) -> Module {
    fn lower(body: &[Stmt], next: &mut usize) -> Vec<Stmt> {
        let mut out: Vec<Stmt> = Vec::new();
        for stmt in body {
            let mut before: Vec<Stmt> = Vec::new();
            let mut stmt = stmt.clone();
            // The bodies first, so a comprehension nested in a loop lowers inside the loop it
            // belongs to.
            for inner in sub_bodies_mut(&mut stmt) {
                *inner = lower(inner, next);
            }
            // Build a binding whose whole value is a comprehension in place, naming
            // nothing in between.
            if let Stmt::Let {
                name,
                value: Some(Expr::Comprehension { .. }),
                ..
            } = &stmt
            {
                let name = name.clone();
                let Stmt::Let { value, .. } = &mut stmt else {
                    unreachable!("just matched a binding");
                };
                let built = value.take().expect("just matched a value");
                let mut filled: Vec<Stmt> = Vec::new();
                let mut placed = Expr::Null;
                fill_into(&name, built, &mut filled);
                let _ = &mut placed;
                out.push(Stmt::Let {
                    name,
                    ty: None,
                    value: Some(Expr::ListLit(Vec::new())),
                    mutable: true,
                });
                out.extend(filled);
                continue;
            }
            for e in statement_expressions_mut(&mut stmt) {
                hoist(e, &mut before, next);
            }
            out.extend(before);
            out.push(stmt);
        }
        out
    }
    /// The loop that fills `name` from this comprehension.
    fn fill_into(name: &str, built: Expr, into: &mut Vec<Stmt>) {
        let Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } = built
        else {
            return;
        };
        let appended = Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(Expr::Name(name.to_string())),
                name: "append".to_string(),
            }),
            args: vec![*element],
        });
        let step = match condition {
            Some(c) => Stmt::If {
                condition: *c,
                then: vec![appended],
                otherwise: Vec::new(),
            },
            None => appended,
        };
        into.push(Stmt::ForEach {
            binding,
            iterable: *iterable,
            body: vec![step],
        });
    }

    /// Replace each comprehension with a name, and say how to fill it.
    fn hoist(e: &mut Expr, before: &mut Vec<Stmt>, next: &mut usize) {
        for inner in subexpressions_mut(e) {
            hoist(inner, before, next);
        }
        let Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } = e
        else {
            return;
        };
        let name = format!("frBuilt{next}");
        *next += 1;
        let appended = Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(Expr::Name(name.clone())),
                name: "append".to_string(),
            }),
            args: vec![(**element).clone()],
        });
        let step = match condition {
            Some(c) => Stmt::If {
                condition: (**c).clone(),
                then: vec![appended],
                otherwise: Vec::new(),
            },
            None => appended,
        };
        before.push(Stmt::Let {
            name: name.clone(),
            ty: None,
            value: Some(Expr::ListLit(Vec::new())),
            mutable: true,
        });
        before.push(Stmt::ForEach {
            binding: binding.clone(),
            iterable: (**iterable).clone(),
            body: vec![step],
        });
        *e = Expr::Name(name);
    }
    let mut next = 0usize;
    let mut lowered = module.clone();
    for item in &mut lowered.items {
        match item {
            Item::Function(f) => f.body = lower(&f.body, &mut next),
            Item::Record(r) => {
                for method in r.methods.iter_mut() {
                    method.body = lower(&method.body, &mut next);
                }
            }
            Item::Statement(stmt) => {
                let mut lowered = lower(std::slice::from_ref(stmt), &mut next);
                *stmt = match lowered.len() {
                    1 => lowered.remove(0),
                    // The loop and the statement it feeds are one statement's
                    // worth of work, and a block is how the IR holds several.
                    _ => Stmt::Block(lowered),
                };
            }
            _ => {}
        }
    }
    lowered
}

/// The parameters this body calls, and how many arguments each call passes.
fn called_parameters(f: &Function) -> std::collections::BTreeMap<String, usize> {
    fn walk(
        stmts: &[Stmt],
        untyped: &std::collections::BTreeSet<String>,
        found: &mut std::collections::BTreeMap<String, usize>,
    ) {
        fn in_expr(
            e: &Expr,
            untyped: &std::collections::BTreeSet<String>,
            found: &mut std::collections::BTreeMap<String, usize>,
        ) {
            if let Expr::Call { callee, args } = e {
                if let Expr::Name(name) = callee.as_ref() {
                    if untyped.contains(name) {
                        found.insert(name.clone(), args.len());
                    }
                }
                in_expr(callee, untyped, found);
                for a in args {
                    in_expr(a, untyped, found);
                }
                return;
            }
            for inner in [e] {
                match inner {
                    Expr::Binary { left, right, .. } => {
                        in_expr(left, untyped, found);
                        in_expr(right, untyped, found);
                    }
                    Expr::Unary { operand, .. }
                    | Expr::Await(operand)
                    | Expr::Propagate(operand) => in_expr(operand, untyped, found),
                    Expr::Field { of, .. } => in_expr(of, untyped, found),
                    Expr::Index { of, index } => {
                        in_expr(of, untyped, found);
                        in_expr(index, untyped, found);
                    }
                    Expr::Template(parts) => {
                        for part in parts {
                            if let TemplatePart::Expr(x) = part {
                                in_expr(x, untyped, found);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for stmt in stmts {
            match stmt {
                Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Throw(e) => {
                    in_expr(e, untyped, found)
                }
                Stmt::Let { value: Some(e), .. } => in_expr(e, untyped, found),
                Stmt::Assign { value, .. } => in_expr(value, untyped, found),
                Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                    in_expr(condition, untyped, found)
                }
                _ => {}
            }
            for inner in sub_bodies(stmt) {
                walk(inner, untyped, found);
            }
        }
    }
    let untyped: std::collections::BTreeSet<String> = f
        .params
        .iter()
        .filter(|p| p.ty.is_none())
        .map(|p| p.name.clone())
        .collect();
    let mut found = std::collections::BTreeMap::new();
    if !untyped.is_empty() {
        walk(&f.body, &untyped, &mut found);
    }
    found
}

/// The stand-in Rust uses for a type the source never wrote.
const TYPE_THE_CALLER_DECIDES: &str = "\u{0}caller-decides";

/// What the bash writer tracks beside [`Out`].
struct BashCx {
    /// Functions whose body returns a value: their calls capture `$(f …)`, and a
    /// call in statement position discards `>/dev/null`.
    value_fns: std::collections::BTreeSet<String>,
    /// Names bound to arrays, spelled `"${xs[@]}"` where a sequence is wanted.
    arrays: std::collections::BTreeSet<String>,
    /// Names holding text.
    strings: std::collections::BTreeSet<String>,
}

/// Does any path through these statements return a value?
fn bash_returns_value(body: &[Stmt]) -> bool {
    body.iter().any(|s| match s {
        Stmt::Return(value) => value.is_some(),
        Stmt::If {
            then, otherwise, ..
        } => bash_returns_value(then) || bash_returns_value(otherwise),
        Stmt::While { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachIndexed { body, .. }
        | Stmt::WhilePresent { body, .. }
        | Stmt::Block(body) => bash_returns_value(body),
        Stmt::CountedFor { body, .. } => bash_returns_value(body),
        Stmt::Switch { arms, default, .. } => {
            arms.iter().any(|(_, b)| bash_returns_value(b)) || bash_returns_value(default)
        }
        _ => false,
    })
}

fn bash(out: &mut Out, module: &Module) {
    let mut bx = BashCx {
        value_fns: std::collections::BTreeSet::new(),
        arrays: std::collections::BTreeSet::new(),
        strings: std::collections::BTreeSet::new(),
    };
    for item in &module.items {
        if let Item::Function(f) = item {
            // `-> None` and `void` announce that nothing comes back; only a
            // real type or a returned value makes the function a value source.
            let announces_value = match &f.returns {
                None | Some(Type::Unit) => false,
                Some(Type::Named { name, .. }) => !matches!(name.as_str(), "None" | "void"),
                Some(_) => true,
            };
            if announces_value || bash_returns_value(&f.body) {
                bx.value_fns.insert(f.name.clone());
            }
        }
    }
    out.blank();
    for item in &module.items {
        match item {
            Item::Function(f) => {
                bash_function(out, &mut bx, f);
                out.blank();
            }
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("# {line}"));
                }
                let name = c.name.clone();
                match bash_value(out, &mut bx, &c.value, &name) {
                    Some(rendered) => {
                        out.line(&format!("readonly {name}={rendered}"));
                        out.fidelity.constants += 1;
                    }
                    None => bash_carry_stmt(
                        out,
                        "a constant this subset cannot spell",
                        &format!("{} = …", c.name),
                    ),
                }
            }
            Item::Statement(stmt) => bash_block(out, &mut bx, std::slice::from_ref(stmt), false),
            Item::Import { text, .. } => {
                out.fidelity.imports_listed += 1;
                let commented = out.comment(&format!("import: {text}"));
                out.line(&commented);
            }
            Item::Record(r) => bash_carry_stmt(
                out,
                "a record declaration; bash has no data types to declare",
                &r.name.clone(),
            ),
            Item::Sum(s) => bash_carry_stmt(
                out,
                "a sum declaration; bash has no data types to declare",
                &s.name.clone(),
            ),
            Item::Newtype(n) => bash_carry_stmt(
                out,
                "a distinct type; bash has no types to distinguish",
                &n.name.clone(),
            ),
            Item::Test { name, .. } => {
                bash_carry_stmt(out, "a test; bash has no runner to hand it to", name)
            }
            Item::Unsupported(u) => carry(out, u),
        }
    }
}

/// A construct with no bash counterpart: counted, named, and left visible.
fn bash_carry_stmt(out: &mut Out, construct: &str, source: &str) {
    out.carried(&Unsupported {
        construct: construct.to_string(),
        source: source.to_string(),
        line: 0,
    });
    let header = out.comment(&format!("{MARKER}: {construct}"));
    out.line(&header);
    for line in source.lines() {
        let commented = out.comment(line);
        out.line(&commented);
    }
}

fn bash_function(out: &mut Out, bx: &mut BashCx, f: &Function) {
    for line in &f.doc {
        out.line(&format!("# {line}"));
    }
    let typed = f.params.iter().any(|p| p.ty.is_some()) || f.returns.is_some();
    if typed {
        out.note_once(
            "bash writes no types: parameters and returns keep their names and lose \
             their annotations",
        );
        out.fidelity.signatures_with_foreign_types += 1;
    } else {
        out.fidelity.signatures_complete += 1;
    }
    if f.is_async {
        out.note_once("bash has no async: the function runs when called");
    }
    out.fidelity.functions += 1;
    out.line(&format!("{}() {{", f.name));
    out.open();
    for (i, p) in f.params.iter().enumerate() {
        out.line(&format!("local {}=\"${}\"", p.name, i + 1));
    }
    bash_guarded_block(out, bx, &f.body, true);
    out.close();
    out.line("}");
}

fn bash_block(out: &mut Out, bx: &mut BashCx, body: &[Stmt], local: bool) {
    for stmt in body {
        bash_stmt(out, bx, stmt, local);
    }
}

fn bash_stmt(out: &mut Out, bx: &mut BashCx, stmt: &Stmt, local: bool) {
    match stmt {
        Stmt::Comment(text) => {
            let commented = out.comment(text);
            out.line(&commented);
        }
        Stmt::Unsupported(u) => carry(out, u),
        Stmt::Block(stmts) => bash_block(out, bx, stmts, local),
        Stmt::LocalFunction(f) => bash_function(out, bx, f),
        Stmt::Break => out.line("break"),
        Stmt::Continue => out.line("continue"),
        Stmt::Return(None) => out.line("return"),
        Stmt::Return(Some(value)) => {
            // A bash function's value is its stdout: the caller captures `$(f …)`.
            match bash_word(out, bx, value) {
                Some(word) => {
                    out.note_once(
                        "a bash function returns its value on stdout; callers \
                         capture it with command substitution",
                    );
                    out.line(&format!("printf '%s\\n' {word}"));
                    out.line("return 0");
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Let {
            name,
            value: Some(Expr::ListLit(elements)),
            ..
        } => {
            let words: Option<Vec<String>> =
                elements.iter().map(|e| bash_word(out, bx, e)).collect();
            match words {
                Some(words) => {
                    bx.arrays.insert(name.clone());
                    let keyword = if local { "local " } else { "" };
                    out.line(&format!("{keyword}{name}=({})", words.join(" ")));
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Let { name, value, .. } => {
            let rendered = match value {
                Some(v) => bash_value(out, bx, v, name),
                None => Some("\"\"".to_string()),
            };
            match rendered {
                Some(rendered) => {
                    let keyword = if local { "local " } else { "" };
                    out.line(&format!("{keyword}{name}={rendered}"));
                    match value.as_ref().is_some_and(|v| bash_texty_in(bx, v)) {
                        true => {
                            bx.strings.insert(name.clone());
                        }
                        false => {
                            bx.strings.remove(name);
                        }
                    }
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Assign {
            target: Expr::Name(name),
            value: Expr::ListLit(elements),
        } => {
            let words: Option<Vec<String>> =
                elements.iter().map(|e| bash_word(out, bx, e)).collect();
            match words {
                Some(words) => {
                    bx.arrays.insert(name.clone());
                    out.line(&format!("{name}=({})", words.join(" ")));
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Assign {
            target: Expr::Name(name),
            value,
        } => match bash_value(out, bx, value, name) {
            Some(rendered) => {
                out.line(&format!("{name}={rendered}"));
                if bash_texty_in(bx, value) {
                    bx.strings.insert(name.clone());
                }
            }
            None => bash_carry_rendered(out, stmt),
        },
        Stmt::Assign {
            target: Expr::Index { of, index },
            value,
        } => {
            let assigned = (|| {
                let Expr::Name(array) = of.as_ref() else {
                    return None;
                };
                let at = bash_arith(out, bx, index)?;
                let word = bash_word(out, bx, value)?;
                Some(format!("{array}[{at}]={word}"))
            })();
            match assigned {
                Some(line) => out.line(&line),
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::If {
            condition,
            then,
            otherwise,
        } => match bash_cond(out, bx, condition) {
            Some(cond) => {
                out.line(&format!("if {cond}; then"));
                out.open();
                bash_guarded_block(out, bx, then, local);
                out.close();
                if !otherwise.is_empty() {
                    out.line("else");
                    out.open();
                    bash_guarded_block(out, bx, otherwise, local);
                    out.close();
                }
                out.line("fi");
            }
            None => bash_carry_rendered(out, stmt),
        },
        Stmt::While { condition, body } => match bash_cond(out, bx, condition) {
            Some(cond) => {
                out.line(&format!("while {cond}; do"));
                out.open();
                bash_guarded_block(out, bx, body, local);
                out.close();
                out.line("done");
            }
            None => bash_carry_rendered(out, stmt),
        },
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => {
            let sequence = match iterable {
                Expr::Name(name) => Some(format!("\"${{{name}[@]}}\"")),
                Expr::ListLit(elements) => {
                    let words: Option<Vec<String>> =
                        elements.iter().map(|e| bash_word(out, bx, e)).collect();
                    words.map(|w| w.join(" "))
                }
                Expr::Call { .. } => bash_word(out, bx, iterable),
                _ => None,
            };
            match sequence {
                Some(sequence) => {
                    out.line(&format!("for {binding} in {sequence}; do"));
                    out.open();
                    bash_guarded_block(out, bx, body, local);
                    out.close();
                    out.line("done");
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::ForEachIndexed {
            index,
            binding,
            iterable,
            body,
        } => {
            // No indexed form over a sequence: a counter walks beside the loop.
            let keyword = if local { "local " } else { "" };
            out.line(&format!("{keyword}{index}=0"));
            let counted = Stmt::ForEach {
                binding: binding.clone(),
                iterable: iterable.clone(),
                body: {
                    let mut with_step = body.clone();
                    with_step.push(Stmt::Assign {
                        target: Expr::Name(index.clone()),
                        value: Expr::Binary {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Name(index.clone())),
                            right: Box::new(Expr::Int("1".to_string())),
                        },
                    });
                    with_step
                },
            };
            bash_stmt(out, bx, &counted, local);
        }
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            ..
        } => {
            let header = (|| {
                let init = match init {
                    Some(s) => bash_arith_assign(out, bx, s)?,
                    None => String::new(),
                };
                let cond = match condition {
                    Some(c) => bash_arith(out, bx, c)?,
                    None => String::new(),
                };
                let update = match update {
                    Some(s) => bash_arith_assign(out, bx, s)?,
                    None => String::new(),
                };
                Some(format!("for (( {init}; {cond}; {update} )); do"))
            })();
            match header {
                Some(header) => {
                    out.line(&header);
                    out.open();
                    bash_guarded_block(out, bx, body, local);
                    out.close();
                    out.line("done");
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Switch {
            subject,
            arms,
            default,
        } => {
            let written = (|| {
                let subject = bash_word(out, bx, subject)?;
                let mut rendered: Vec<(String, &Vec<Stmt>)> = Vec::new();
                for (selectors, body) in arms {
                    let words: Option<Vec<String>> = selectors
                        .iter()
                        .map(|s| match s {
                            Expr::Str(text) => Some(bash_quote(text)),
                            Expr::Int(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect();
                    rendered.push((words?.join("|"), body));
                }
                Some((subject, rendered))
            })();
            match written {
                Some((subject, rendered)) => {
                    out.line(&format!("case {subject} in"));
                    out.open();
                    for (selector, body) in rendered {
                        out.line(&format!("{selector})"));
                        out.open();
                        bash_guarded_block(out, bx, body, local);
                        out.line(";;");
                        out.close();
                    }
                    if !default.is_empty() {
                        out.line("*)");
                        out.open();
                        bash_guarded_block(out, bx, default, local);
                        out.line(";;");
                        out.close();
                    }
                    out.close();
                    out.line("esac");
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Assert { condition, message } => {
            // The check every target can make: test, complain on stderr, stop.
            match bash_cond(out, bx, condition) {
                Some(cond) => {
                    let said = message
                        .as_ref()
                        .and_then(|m| bash_word(out, bx, m))
                        .unwrap_or_else(|| "\"assertion failed\"".to_string());
                    out.line(&format!("if ! {cond}; then"));
                    out.open();
                    out.line(&format!("printf '%s\\n' {said} >&2"));
                    out.line("exit 1");
                    out.close();
                    out.line("fi");
                }
                None => bash_carry_rendered(out, stmt),
            }
        }
        Stmt::Expr(Expr::Call { callee, args }) => {
            let command = (|| {
                // `nums.append(3)` arrives as a member call; bash grows the array.
                if let Expr::Field { of, name } = callee.as_ref() {
                    let Expr::Name(array) = of.as_ref() else {
                        return None;
                    };
                    if !matches!(name.as_str(), "append" | "push" | "add") {
                        return None;
                    }
                    let [element] = args.as_slice() else {
                        return None;
                    };
                    let word = bash_word(out, bx, element)?;
                    bx.arrays.insert(array.clone());
                    return Some(format!("{array}+=({word})"));
                }
                let Expr::Name(name) = callee.as_ref() else {
                    return None;
                };
                if name == "print" {
                    let [one] = args.as_slice() else { return None };
                    let word = bash_word(out, bx, one)?;
                    return Some(format!("printf '%s\\n' {word}"));
                }
                if name == "append" || name == "push" {
                    let [Expr::Name(array), element] = args.as_slice() else {
                        return None;
                    };
                    let word = bash_word(out, bx, element)?;
                    bx.arrays.insert(array.clone());
                    return Some(format!("{array}+=({word})"));
                }
                if !out.functions.contains_key(name) {
                    return None;
                }
                let words: Option<Vec<String>> =
                    args.iter().map(|a| bash_word(out, bx, a)).collect();
                let mut line = name.clone();
                for word in words? {
                    line.push(' ');
                    line.push_str(&word);
                }
                // A value-returning function prints; a statement call is not
                // reading, so the value goes nowhere instead of onto stdout.
                if bx.value_fns.contains(name) {
                    line.push_str(" > /dev/null");
                }
                Some(line)
            })();
            match command {
                Some(line) => out.line(&line),
                None => bash_carry_rendered(out, stmt),
            }
        }
        other => bash_carry_rendered(out, other),
    }
}

/// Write a branch body, and `:` after it when nothing in it became a command.
fn bash_guarded_block(out: &mut Out, bx: &mut BashCx, body: &[Stmt], local: bool) {
    let before = out.text.len();
    bash_block(out, bx, body, local);
    let has_command = out.text[before..].lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    if !has_command {
        out.line(":");
    }
}

/// Carry a statement this subset cannot spell, rendered so the reader sees it.
fn bash_carry_rendered(out: &mut Out, stmt: &Stmt) {
    let rendered = render_rust_stmts(std::slice::from_ref(stmt));
    bash_carry_stmt(
        out,
        "a statement outside bash's subset",
        rendered.trim_end(),
    );
}

/// A string as one safely quoted bash word.
fn bash_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The right-hand side of `name=…`, which is a word with one extra option: an
/// arithmetic value writes `$(( … ))` outright.
fn bash_value(out: &mut Out, bx: &mut BashCx, e: &Expr, _name: &str) -> Option<String> {
    bash_word(out, bx, e)
}

/// One word: usable as a command argument, a test operand, or the right of `=`.
fn bash_word(out: &mut Out, bx: &mut BashCx, e: &Expr) -> Option<String> {
    match e {
        Expr::Int(text) | Expr::Float(text) => Some(text.clone()),
        Expr::Str(text) => Some(bash_quote(text)),
        Expr::Bool(true) => Some("1".to_string()),
        Expr::Bool(false) => Some("0".to_string()),
        Expr::Name(name) => Some(format!("\"${{{name}}}\"")),
        Expr::Index { of, index } => {
            let Expr::Name(array) = of.as_ref() else {
                return None;
            };
            let at = bash_arith(out, bx, index)?;
            Some(format!("\"${{{array}[{at}]}}\""))
        }
        Expr::Call { callee, args } => {
            // `"negative".to_string()` and `x.toString()`: bash values are text
            // already, so the conversion is the value itself.
            if let Expr::Field { of, name } = callee.as_ref() {
                if matches!(name.as_str(), "to_string" | "toString") && args.is_empty() {
                    return bash_word(out, bx, of);
                }
                return None;
            }
            let Expr::Name(name) = callee.as_ref() else {
                return None;
            };
            // The canonical `str(x)`: bash values are text already, so the
            // conversion is the value itself.
            if name == "str" {
                let [one] = args.as_slice() else { return None };
                return bash_word(out, bx, one);
            }
            // Bash arithmetic is integers only, so it is already cut toward zero.
            if name == "trunc" {
                let [one] = args.as_slice() else { return None };
                return bash_word(out, bx, one);
            }
            if name == "len" {
                let [of] = args.as_slice() else { return None };
                let Expr::Name(of) = of else { return None };
                return match bx.arrays.contains(of) {
                    true => Some(format!("\"${{#{of}[@]}}\"")),
                    false => Some(format!("\"${{#{of}}}\"")),
                };
            }
            if !out.functions.contains_key(name) {
                return None;
            }
            let words: Option<Vec<String>> = args.iter().map(|a| bash_word(out, bx, a)).collect();
            let mut call = format!("\"$({name}");
            for word in words? {
                call.push(' ');
                call.push_str(&word);
            }
            call.push_str(")\"");
            Some(call)
        }
        Expr::Binary { op, .. } => {
            if *op == BinaryOp::Add && bash_texty_in(bx, e) {
                let mut text = String::from("\"");
                bash_concat_into(out, bx, e, &mut text)?;
                text.push('"');
                return Some(text);
            }
            let inside = bash_arith(out, bx, e)?;
            Some(format!("$(( {inside} ))"))
        }
        Expr::Unary {
            op: UnaryOp::Neg, ..
        } => {
            let inside = bash_arith(out, bx, e)?;
            Some(format!("$(( {inside} ))"))
        }
        Expr::Template(parts) => {
            let mut text = String::from("\"");
            for part in parts {
                match part {
                    TemplatePart::Text(t) => {
                        text.push_str(&t.replace('\\', "\\\\").replace('"', "\\\""))
                    }
                    TemplatePart::Expr(e) => match e {
                        Expr::Name(name) => text.push_str(&format!("${{{name}}}")),
                        Expr::Index { of, index } => {
                            let Expr::Name(array) = of.as_ref() else {
                                return None;
                            };
                            let at = bash_arith(out, bx, index)?;
                            text.push_str(&format!("${{{array}[{at}]}}"));
                        }
                        other => {
                            let word = bash_word(out, bx, other)?;
                            let inner = word
                                .strip_prefix('"')
                                .and_then(|w| w.strip_suffix('"'))
                                .map(|w| w.to_string())
                                .unwrap_or(word);
                            text.push_str(&inner);
                        }
                    },
                }
            }
            text.push('"');
            Some(text)
        }
        _ => None,
    }
}

/// An expression inside `(( … ))`, where names go bare and text has no place.
fn bash_arith(out: &mut Out, bx: &mut BashCx, e: &Expr) -> Option<String> {
    match e {
        Expr::Int(text) => Some(text.clone()),
        Expr::Bool(true) => Some("1".to_string()),
        Expr::Bool(false) => Some("0".to_string()),
        Expr::Name(name) => Some(name.clone()),
        Expr::Index { of, index } => {
            let Expr::Name(array) = of.as_ref() else {
                return None;
            };
            let at = bash_arith(out, bx, index)?;
            Some(format!("${{{array}[{at}]}}"))
        }
        Expr::Call { .. } => bash_word(out, bx, e),
        Expr::Binary { op, left, right } => {
            // Bash's `%` truncates, and a source that floors answers a different number
            // whenever the operands have different signs.
            if *op == BinaryOp::FloorRem {
                if !matches!(
                    right.as_ref(),
                    Expr::Name(_) | Expr::Int(_) | Expr::Index { .. }
                ) {
                    return None;
                }
                let dividend = bash_arith(out, bx, left)?;
                let divisor = bash_arith(out, bx, right)?;
                return Some(format!(
                    "((({dividend}) % ({divisor})) + ({divisor})) % ({divisor})"
                ));
            }
            let spelled = match op {
                BinaryOp::FloorDiv => "/",
                BinaryOp::TrueDiv => return None,
                other => other.c_like(),
            };
            let left = bash_arith(out, bx, left)?;
            let right = bash_arith(out, bx, right)?;
            Some(format!("{left} {spelled} {right}"))
        }
        Expr::Unary { op, operand } => {
            let inner = bash_arith(out, bx, operand)?;
            match op {
                UnaryOp::Neg => Some(format!("-({inner})")),
                UnaryOp::Not => Some(format!("!({inner})")),
                UnaryOp::Unwrap => None,
            }
        }
        _ => None,
    }
}

/// `i=0` or `i=i+1` as the inside of a `for (( … ))` header.
fn bash_arith_assign(out: &mut Out, bx: &mut BashCx, stmt: &Stmt) -> Option<String> {
    match stmt {
        Stmt::Assign {
            target: Expr::Name(name),
            value,
        }
        | Stmt::Let {
            name,
            value: Some(value),
            ..
        } => {
            let rendered = bash_arith(out, bx, value)?;
            Some(format!("{name}={rendered}"))
        }
        _ => None,
    }
}

/// A condition after `if` or `while`.
fn bash_cond(out: &mut Out, bx: &mut BashCx, e: &Expr) -> Option<String> {
    match e {
        Expr::Bool(true) => Some("true".to_string()),
        Expr::Bool(false) => Some("false".to_string()),
        Expr::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            let inner = bash_cond(out, bx, operand)?;
            Some(format!("! {inner}"))
        }
        Expr::Binary { op, left, right } => match op {
            BinaryOp::And => {
                let l = bash_cond(out, bx, left)?;
                let r = bash_cond(out, bx, right)?;
                Some(format!("{l} && {r}"))
            }
            BinaryOp::Or => {
                let l = bash_cond(out, bx, left)?;
                let r = bash_cond(out, bx, right)?;
                Some(format!("{{ {l} || {r}; }}"))
            }
            BinaryOp::Eq | BinaryOp::Ne if bash_texty(left) || bash_texty(right) => {
                let l = bash_word(out, bx, left)?;
                let r = bash_word(out, bx, right)?;
                let spelled = if *op == BinaryOp::Eq { "==" } else { "!=" };
                Some(format!("[[ {l} {spelled} {r} ]]"))
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let inside = bash_arith(out, bx, e)?;
                Some(format!("(( {inside} ))"))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Is this expression text rather than a number, as far as the writer can see?
fn bash_texty(e: &Expr) -> bool {
    matches!(e, Expr::Str(_) | Expr::Template(_))
}

/// [`bash_texty`], with the writer's knowledge of which names hold text.
fn bash_texty_in(bx: &BashCx, e: &Expr) -> bool {
    match e {
        Expr::Str(_) | Expr::Template(_) => true,
        Expr::Name(name) => bx.strings.contains(name),
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } => bash_texty_in(bx, left) || bash_texty_in(bx, right),
        _ => false,
    }
}

/// Write a concatenation's pieces into one double-quoted bash word.
fn bash_concat_into(out: &mut Out, bx: &mut BashCx, e: &Expr, text: &mut String) -> Option<()> {
    match e {
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } => {
            bash_concat_into(out, bx, left, text)?;
            bash_concat_into(out, bx, right, text)
        }
        Expr::Str(t) => {
            text.push_str(&t.replace('\\', "\\\\").replace('"', "\\\""));
            Some(())
        }
        Expr::Int(t) => {
            text.push_str(t);
            Some(())
        }
        Expr::Name(name) => {
            text.push_str(&format!("${{{name}}}"));
            Some(())
        }
        Expr::Template(_) | Expr::Call { .. } | Expr::Index { .. } => {
            let word = bash_word(out, bx, e)?;
            let inner = word
                .strip_prefix('"')
                .and_then(|w| w.strip_suffix('"'))
                .map(|w| w.to_string())
                .unwrap_or(word);
            text.push_str(&inner);
            Some(())
        }
        _ => None,
    }
}
