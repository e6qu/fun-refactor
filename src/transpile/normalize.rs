//! Each language's spelling of the shared builtins, folded into one canonical form.

use super::ir::*;
use crate::lang::Language;

/// Rewrite `module`'s expressions into the canonical vocabulary for `language`.
pub fn normalize(module: &mut Module, language: Language) {
    strip_lowering_helpers(module);
    normalize_language(module, language);
    // After the per-language pass, so Go's `x = append(x, v)` is already the method
    // call this reads.
    settle_list_element_types(module);
    settle_sets(module, language);
    sets_from_maps(module);
    membership_into_conditions(module);
    if matches!(language, Language::Zig) {
        discards_are_calls(module);
    }
    settle_constructors(module);
    settle_boolean_switches(module);
}

/// A membership question asked into a binding the next `if` reads, asked inline.
fn membership_into_conditions(module: &mut Module) {
    fn asks_membership(e: &Expr) -> bool {
        matches!(e, Expr::Call { callee, args }
            if args.len() == 1
                && matches!(callee.as_ref(), Expr::Field { name, .. } if name == "contains"))
    }
    fn walk(body: &mut Vec<Stmt>) {
        for stmt in body.iter_mut() {
            for inner in super::write::sub_bodies_mut(stmt) {
                walk(inner);
            }
        }
        let mut at = 0;
        while at + 1 < body.len() {
            let asked = match &body[at] {
                Stmt::Let {
                    name,
                    value: Some(value),
                    ..
                } if asks_membership(value) => Some((name.clone(), value.clone())),
                Stmt::Assign {
                    target: Expr::Name(name),
                    value,
                } if asks_membership(value) => Some((name.clone(), value.clone())),
                _ => None,
            };
            let Some((name, value)) = asked else {
                at += 1;
                continue;
            };
            let reads_it = matches!(&body[at + 1], Stmt::If { condition, .. }
                if matches!(condition, Expr::Name(n) if *n == name));
            if !reads_it {
                at += 1;
                continue;
            }
            if let Stmt::If { condition, .. } = &mut body[at + 1] {
                *condition = value;
            }
            body.remove(at);
        }
    }
    for item in module.items.iter_mut() {
        match item {
            Item::Function(f) => walk(&mut f.body),
            Item::Record(r) => {
                for m in r.methods.iter_mut() {
                    walk(&mut m.body);
                }
            }
            _ => {}
        }
    }
}

/// `_ = f(x)` is the call, and the discard is Zig asking to be let off.
fn discards_are_calls(module: &mut Module) {
    fn walk(body: &mut [Stmt]) {
        for stmt in body.iter_mut() {
            if let Stmt::Assign {
                target: Expr::Name(bound),
                value,
            } = stmt
            {
                if bound == "_" && matches!(value, Expr::Call { .. }) {
                    *stmt = Stmt::Expr(value.clone());
                    continue;
                }
            }
            for inner in super::write::sub_bodies_mut(stmt) {
                walk(inner);
            }
        }
    }
    for item in module.items.iter_mut() {
        match item {
            Item::Function(f) => walk(&mut f.body),
            Item::Record(r) => {
                for m in r.methods.iter_mut() {
                    walk(&mut m.body);
                }
            }
            _ => {}
        }
    }
}

/// A map whose values carry nothing is a set, and its stores are adds.
fn sets_from_maps(module: &mut Module) {
    fn carries_nothing(e: &Expr) -> bool {
        match e {
            Expr::Null => true,
            Expr::RecordLit { fields, .. } => fields.is_empty(),
            _ => false,
        }
    }
    fn stores_into<'a>(body: &'a [Stmt], name: &str, found: &mut Vec<&'a Expr>) {
        for stmt in body {
            if let Stmt::Assign {
                target: Expr::Index { of, .. },
                value,
            } = stmt
            {
                if matches!(of.as_ref(), Expr::Name(n) if n == name) {
                    found.push(value);
                }
            }
            for inner in super::write::sub_bodies(stmt) {
                stores_into(inner, name, found);
            }
        }
    }
    fn rewrite(body: &mut [Stmt], name: &str) {
        for stmt in body.iter_mut() {
            if let Stmt::Assign {
                target: Expr::Index { of, index },
                value,
            } = stmt
            {
                if matches!(of.as_ref(), Expr::Name(n) if n == name) && carries_nothing(value) {
                    *stmt = Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Field {
                            of: of.clone(),
                            name: "add".to_string(),
                        }),
                        args: vec![(**index).clone()],
                    });
                    continue;
                }
            }
            for inner in super::write::sub_bodies_mut(stmt) {
                rewrite(inner, name);
            }
        }
    }
    fn settle(f: &mut Function) {
        let mut names = Vec::new();
        for stmt in &f.body {
            let Stmt::Let {
                name, ty, value, ..
            } = stmt
            else {
                continue;
            };
            let empty_map = matches!(value, Some(Expr::MapLit(entries)) if entries.is_empty());
            let says_set = matches!(ty, Some(Type::Set(_)))
                || matches!(ty, Some(Type::Map(_, v)) if **v == Type::Unit);
            if !empty_map && !says_set {
                continue;
            }
            let mut stored = Vec::new();
            stores_into(&f.body, name, &mut stored);
            if !says_set && (stored.is_empty() || !stored.iter().all(|v| carries_nothing(v))) {
                continue;
            }
            names.push(name.clone());
        }
        for name in &names {
            let mut body = std::mem::take(&mut f.body);
            rewrite(&mut body, name);
            for stmt in body.iter_mut() {
                let Stmt::Let {
                    name: bound,
                    ty,
                    value,
                    ..
                } = stmt
                else {
                    continue;
                };
                if bound != name {
                    continue;
                }
                if let Some(Expr::MapLit(entries)) = value {
                    let keys = entries.iter().map(|(k, _)| k.clone()).collect();
                    *value = Some(Expr::SetLit(keys));
                }
                if let Some(Type::Map(key, _)) = ty {
                    *ty = Some(Type::Set(key.clone()));
                }
            }
            f.body = body;
        }
    }
    for item in module.items.iter_mut() {
        match item {
            Item::Function(f) => settle(f),
            Item::Record(r) => {
                for m in r.methods.iter_mut() {
                    settle(m);
                }
            }
            _ => {}
        }
    }
}

/// Every construction that builds a set, read as the set it builds.
fn settle_sets(module: &mut Module, language: Language) {
    // The three set words each language spells its own way, onto the canonical `add`, `remove`
    // and `contains`.
    let statement_words: &[(&str, &str, usize)] = match language {
        Language::Rust => &[("insert", "add", 1)],
        Language::TypeScript | Language::Tsx => &[("delete", "remove", 1)],
        Language::Zig => &[("put", "add", 2)],
        _ => &[],
    };
    let words: &[(&str, &str, usize)] = match language {
        Language::TypeScript | Language::Tsx => &[("has", "contains", 1)],
        _ => &[],
    };
    fn rename_statements(module: &mut Module, words: &[(&str, &str, usize)]) {
        fn walk(body: &mut [Stmt], words: &[(&str, &str, usize)]) {
            for stmt in body.iter_mut() {
                if let Stmt::Expr(e) = stmt {
                    for (from, to, argc) in words {
                        let taken = std::mem::replace(e, Expr::Null);
                        *e = match rename_method(taken, from, to, *argc) {
                            Ok(renamed) => renamed,
                            Err(unchanged) => unchanged,
                        };
                    }
                }
                for inner in super::write::sub_bodies_mut(stmt) {
                    walk(inner, words);
                }
            }
        }
        for item in module.items.iter_mut() {
            match item {
                Item::Function(f) => walk(&mut f.body, words),
                Item::Record(r) => {
                    for m in r.methods.iter_mut() {
                        walk(&mut m.body, words);
                    }
                }
                _ => {}
            }
        }
    }
    rename_statements(module, statement_words);
    super::read::each_expr_in_module(module, &mut |e| {
        if let Some(built) = built_set(e) {
            *e = built;
            return;
        }
        // Go's `delete(m, k)` is a builtin taking the collection first, where
        // the other five call a method on it.
        if matches!(language, Language::Go) {
            if let Expr::Call { callee, args } = e {
                let deletes = matches!(callee.as_ref(), Expr::Name(n) if n == "delete");
                if deletes && args.len() == 2 {
                    let of = args[0].clone();
                    let key = args[1].clone();
                    *e = Expr::Call {
                        callee: Box::new(Expr::Field {
                            of: Box::new(of),
                            name: "remove".to_string(),
                        }),
                        args: vec![key],
                    };
                    return;
                }
            }
        }
        for (from, to, argc) in words {
            let taken = std::mem::replace(e, Expr::Null);
            *e = match rename_method(taken, from, to, *argc) {
                Ok(renamed) => renamed,
                Err(unchanged) => unchanged,
            };
        }
        // Zig's `put(k, {})` adds a member and carries no value with it.
        if let Expr::Call { callee, args } = e {
            let adds = matches!(callee.as_ref(), Expr::Field { name, .. } if name == "add");
            if adds
                && args.len() == 2
                && matches!(args[1], Expr::RecordLit { ref fields, .. } if fields.is_empty())
            {
                args.truncate(1);
            }
        }
    });
}

/// A branch on `true` and `false` is an `if`, not a `switch`.
fn settle_boolean_switches(module: &mut Module) {
    fn walk(body: &mut [Stmt]) {
        for stmt in body.iter_mut() {
            for inner in super::write::sub_bodies_mut(stmt) {
                walk(inner);
            }
            let Stmt::Switch {
                subject,
                arms,
                default,
            } = stmt
            else {
                continue;
            };
            // `true` and `false` name every value a boolean has, so a `default`
            // beside them is unreachable and there is nothing to lose.
            let mut then = None;
            let mut otherwise = None;
            let mut only_booleans = !arms.is_empty();
            for (literals, taken) in arms.iter() {
                match literals.as_slice() {
                    [Expr::Bool(true)] => then = Some(taken.clone()),
                    [Expr::Bool(false)] => otherwise = Some(taken.clone()),
                    _ => {
                        only_booleans = false;
                        break;
                    }
                }
            }
            if !only_booleans || then.is_none() {
                continue;
            }
            *stmt = Stmt::If {
                condition: subject.clone(),
                then: then.unwrap_or_default(),
                otherwise: otherwise.unwrap_or_else(|| std::mem::take(default)),
            };
        }
    }
    for item in module.items.iter_mut() {
        match item {
            Item::Function(f) => walk(&mut f.body),
            Item::Record(r) => {
                for m in r.methods.iter_mut() {
                    walk(&mut m.body);
                }
            }
            Item::Test { body, .. } => walk(body),
            Item::Statement(stmt) => {
                let mut one = vec![stmt.clone()];
                walk(&mut one);
                *stmt = one.remove(0);
            }
            _ => {}
        }
    }
}

/// A constructor that only fills fields is the record literal it builds.
fn settle_constructors(module: &mut Module) {
    for item in module.items.iter_mut() {
        let Item::Record(record) = item else { continue };
        let name = record.name.clone();
        for method in record.methods.iter_mut() {
            if !method.is_constructor {
                continue;
            }
            // Already a literal, which is how three of the targets write one.
            if matches!(
                method.body.as_slice(),
                [Stmt::Return(Some(Expr::RecordLit { .. }))]
            ) {
                continue;
            }
            let Some(receiver) = method.receiver_binding.clone() else {
                continue;
            };
            let mut fields = Vec::new();
            let mut only_assigns = !method.body.is_empty();
            for stmt in &method.body {
                let Stmt::Assign {
                    target: Expr::Field { of, name: field },
                    value,
                } = stmt
                else {
                    only_assigns = false;
                    break;
                };
                if !matches!(of.as_ref(), Expr::Name(n) if *n == receiver) {
                    only_assigns = false;
                    break;
                }
                fields.push((field.clone(), value.clone()));
            }
            if !only_assigns {
                continue;
            }
            method.body = vec![Stmt::Return(Some(Expr::RecordLit {
                ty: name.clone(),
                fields,
            }))];
            method.returns = Some(Type::named(&name));
        }
    }
}

fn normalize_language(module: &mut Module, language: Language) {
    // Every language but Python and Go reaches a map through methods, and each has its own
    // words for the same four things.
    settle_maps(module, language);
    settle_map_types(module);
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
            settle_java_lists(module);
            java
        }
        Language::Python => {
            settle_exception_classes(module);
            python
        }
        Language::Rust => {
            settle_result_idiom(module);
            return;
        }
        Language::Zig => {
            zig_module(module);
            settle_result_idiom(module);
            zig_exprs
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
            Item::Constant(c) => map_expr(&mut c.value, rewrite),
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
        Stmt::BreakWith { value, .. } => {
            if let Some(value) = value {
                map_expr(value, rewrite);
            }
            return;
        }
        Stmt::LocalFunction(f) => {
            map_function(f, rewrite);
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
        Stmt::Defer(body) | Stmt::ErrDefer(body) | Stmt::Block(body) => vec![body],
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
        Expr::Tuple(items) | Expr::ListLit(items) => {
            for item in items {
                walk(item);
            }
        }
        Expr::MapLit(entries) => {
            for (key, value) in entries {
                walk(key);
                walk(value);
            }
        }
        Expr::Variant { fields, .. } | Expr::RecordLit { fields, .. } => {
            for (_, value) in fields {
                walk(value);
            }
        }
        Expr::Keyword { value, .. } => walk(value),
        Expr::Cast { value, ty } => {
            walk(value);
            walk(ty);
        }
        Expr::InstanceOf { value, ty } => {
            walk(value);
            walk(ty);
        }
        Expr::Coalesce { value, fallback } => {
            walk(value);
            walk(fallback);
        }
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            walk(condition);
            walk(then);
            walk(otherwise);
        }
        Expr::Lambda { body, .. } => walk(body),
        Expr::Comprehension {
            element,
            iterable,
            condition,
            ..
        } => {
            walk(element);
            walk(iterable);
            if let Some(condition) = condition {
                walk(condition);
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
        // The `strings` package spells what other languages put on the value.
        ["strings", "ToUpper" | "ToLower" | "TrimSpace" | "Contains"] => {
            let Expr::Call { callee, mut args } = expr else {
                unreachable!("call_parts said so");
            };
            let Expr::Field { name, .. } = *callee else {
                unreachable!("the path had two parts");
            };
            let (method, receiver_first) = match name.as_str() {
                "ToUpper" => ("upper", true),
                "ToLower" => ("lower", true),
                "TrimSpace" => ("strip", true),
                _ => ("contains", true),
            };
            let receiver = if receiver_first && !args.is_empty() {
                args.remove(0)
            } else {
                Expr::Null
            };
            Expr::Call {
                callee: Box::new(Expr::Field {
                    of: Box::new(receiver),
                    name: method.to_string(),
                }),
                args,
            }
        }
        ["fmt", "Println"] => {
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            print_call(args)
        }
        // `Printf` whose format ends in a newline says the same thing `print` says.
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
    // `x.length` measures, whatever x is, and a set and a map answer `.size`.
    if let Expr::Field { of, name } = &expr {
        if name == "length" || name == "size" {
            return Expr::Call {
                callee: Box::new(Expr::Name("len".to_string())),
                args: vec![(**of).clone()],
            };
        }
    }
    let expr = match rename_method(expr, "includes", "contains", 1) {
        Ok(done) => return done,
        Err(back) => back,
    };
    let expr = match rename_method(expr, "toUpperCase", "upper", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
    let expr = match rename_method(expr, "toLowerCase", "lower", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
    let expr = match rename_method(expr, "trim", "strip", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
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
        // `word.includes(x)` and the case methods, under their canonical names.
        (["console", "error"], _) => expr,
        // `Math.trunc(a / b)` is how this language spells the division the other five truncate
        // natively.
        (
            ["Math", rounding @ ("trunc" | "floor")],
            [Expr::Binary {
                op: BinaryOp::Div, ..
            }],
        ) => {
            let floors = *rounding == "floor";
            let Expr::Call { args, .. } = expr else {
                unreachable!("call_parts said so");
            };
            let Some(Expr::Binary { left, right, .. }) = args.into_iter().next() else {
                unreachable!("the guard said so");
            };
            match floors {
                true => Expr::Binary {
                    op: BinaryOp::FloorDiv,
                    left,
                    right,
                },
                // Truncation is the canonical `trunc` applied to the quotient, and no operator
                // at all.
                false => Expr::Call {
                    callee: Box::new(Expr::Name("trunc".to_string())),
                    args: vec![Expr::Binary {
                        op: BinaryOp::Div,
                        left,
                        right,
                    }],
                },
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
    // `x.length()` on text measures it, and `x.size()` a collection.
    if let Expr::Call { callee, args } = &expr {
        if args.is_empty() {
            if let Expr::Field { of, name } = &**callee {
                if name == "length" || name == "size" {
                    return Expr::Call {
                        callee: Box::new(Expr::Name("len".to_string())),
                        args: vec![(**of).clone()],
                    };
                }
            }
        }
    }
    let expr = match rename_method(expr, "toUpperCase", "upper", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
    let expr = match rename_method(expr, "toLowerCase", "lower", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
    let expr = match rename_method(expr, "trim", "strip", 0) {
        Ok(done) => return done,
        Err(back) => back,
    };
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
fn fold_concat(expr: &Expr) -> Option<Expr> {
    /// Does any part of this `+` chain say string?
    fn stringy(expr: &Expr) -> bool {
        match expr {
            Expr::Str(_) | Expr::Template(_) => true,
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => stringy(left) || stringy(right),
            _ => false,
        }
    }
    // A subtree with no string in it is arithmetic the source runs first: `1 + 2 + "x"` prints
    // `3x`.
    fn leaves<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
        match expr {
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } if stringy(expr) => {
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
    // A template in the chain is a string too: the fold runs bottom-up.
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
    zig_lists(&mut f.body);
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
                if value.as_ref().is_some_and(|v| mentions_any(v, &writers)) =>
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

    // The entry point's `init: std.process.Init` is the runtime handing itself over, and its
    // error-union return is the same plumbing.
    if f.name == "main" && f.receiver.is_none() {
        f.params.retain(
            |p| !matches!(&p.ty, Some(Type::Named { name, .. }) if name.contains("process.Init")),
        );
        f.returns = None;
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
        | Stmt::ErrDefer(body)
        | Stmt::Block(body) => vec![body],
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
    let through_debug = matches!(
        call_path(callee).as_deref(),
        Some(["std", "debug", "print"])
    );
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
        Expr::Call { callee, args } => mentions_stdout(callee) || args.iter().any(mentions_stdout),
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

// -------------------------------------------------------- the Result idiom.

/// A function returning `Result<T, message>`, said the way the exception languages say it: the
/// return type is `T`, `Err` is a throw, `Ok` unwraps.
fn settle_result_idiom(module: &mut Module) {
    // A declared error enum's variants are failure names, and a name is a message:
    // `Err(ParseError::Empty)` throws "Empty", the same lowering the error sets take.
    let declared_sums: Vec<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Sum(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect();
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
            // Whatever names the failure side, the failure is the channel: the canonical model
            // carries its message.
            let ok = args[0].clone();
            f.returns = match ok {
                Type::Unit => None,
                other => Some(other),
            };
            settle_result_statements(&mut f.body);
        }
    }
    // A throw of a qualified failure name throws the name: the canonical failure is its
    // message.
    for item in module.items.iter_mut() {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            settle_thrown_names(&mut f.body, &declared_sums);
        }
    }
}

/// `throw ParseError.Empty` throws "Empty": the qualified failure name of a
/// declared set dissolves into the message it is everywhere else.
fn settle_thrown_names(body: &mut [Stmt], declared_sums: &[String]) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            settle_thrown_names(inner, declared_sums);
        }
        if let Stmt::Throw(value) = stmt {
            let named = match value {
                Expr::Field { of, name } if matches!(&**of, Expr::Name(n) if declared_sums.contains(n)) => {
                    Some(name.clone())
                }
                Expr::Variant { sum, name, fields }
                    if fields.is_empty() && declared_sums.contains(sum) =>
                {
                    Some(name.clone())
                }
                _ => None,
            };
            if let Some(name) = named {
                *value = Expr::Str(name);
            }
        }
    }
}

fn settle_result_statements(body: &mut [Stmt]) {
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
                        Expr::Field { of, name } if matches!(&*of, Expr::Name(n) if n == "error") => {
                            Expr::Str(name)
                        }
                        // `Err(ParseError::Empty)`: the variant names the failure.
                        Expr::Variant { name, fields, .. } if fields.is_empty() => Expr::Str(name),
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

    fn walk(body: &mut [Stmt], builtin: &dyn Fn(&str) -> bool) {
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
            settle_go_appends(&mut f.body);
            settle_go_checks(&mut f.body);
        }
    }
}

fn settle_go_returns(body: &mut [Stmt]) {
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

/// `x = append(x, v)`, said the way every other list grows: a method call.
fn settle_go_appends(body: &mut [Stmt]) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            settle_go_appends(inner);
        }
        let Stmt::Assign { target, value } = stmt else {
            continue;
        };
        let Expr::Name(bound) = target else { continue };
        let Expr::Call { callee, args } = value else {
            continue;
        };
        if !matches!(&**callee, Expr::Name(n) if n == "append") || args.len() != 2 {
            continue;
        }
        if !matches!(&args[0], Expr::Name(n) if n == bound) {
            continue;
        }
        let pushed = args.pop().expect("two args");
        let list = bound.clone();
        *stmt = Stmt::Expr(Expr::Call {
            callee: Box::new(Expr::Field {
                of: Box::new(Expr::Name(list)),
                name: "append".to_string(),
            }),
            args: vec![pushed],
        });
    }
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
        // `if err != nil { return _, err }` alone is a propagation.
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
fn rewrite_error_reads(body: &mut [Stmt], err: &str) {
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
fn strip_lowering_helpers(module: &mut Module) {
    let ours = |name: &str| matches!(name, "frShow" | "frPrint" | "frFormat");
    // The definition may not even have parsed as a function: Zig's `comptime format` parameter
    // reads as no function at all.
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
        // `frShow(x)` displays `x`; the value is `x` itself.
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
    fn walk_stmts(body: &mut [Stmt]) {
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

/// `std.mem.eql(u8, a, b)` is the string equality every other language spells `==`.
fn zig_exprs(expr: Expr) -> Expr {
    // A bare tag no sum answered for: the value says the tag's name, and a string of it is what
    // every target can hold.
    match expr {
        Expr::Variant { sum, name, fields } if sum.is_empty() && fields.is_empty() => {
            Expr::Str(name)
        }
        // The rewrite runs bottom-up, so a dot-literal callee arrives already settled into its
        // tag string: `.fromPath(a)` is `"fromPath"(a)` by now.
        Expr::Call { callee, args } => {
            if let Expr::Str(name) = callee.as_ref() {
                return zig_exprs_tail(Expr::Call {
                    callee: Box::new(Expr::Name(name.clone())),
                    args,
                });
            }
            zig_exprs_tail(Expr::Call { callee, args })
        }
        Expr::Variant { sum, name, fields } if sum.is_empty() => {
            Expr::MapLit(vec![(Expr::Str(name), single_variant_value(fields))])
        }
        Expr::RecordLit { ty, fields } if ty.is_empty() => Expr::MapLit(
            fields
                .into_iter()
                .map(|(name, value)| (Expr::Str(name), value))
                .collect(),
        ),
        other => zig_exprs_tail(other),
    }
}

/// The one payload a settle-candidate variant carries, unwrapped from its
/// `value` slot.
fn single_variant_value(fields: Vec<(String, Expr)>) -> Expr {
    fields
        .into_iter()
        .next()
        .map(|(_, value)| value)
        .unwrap_or(Expr::Null)
}

fn zig_exprs_tail(expr: Expr) -> Expr {
    // `std.mem.indexOf(u8, hay, needle) != null` is containment.
    if let Expr::Binary {
        op: BinaryOp::Ne,
        left,
        right,
    } = &expr
    {
        if matches!(&**right, Expr::Null) {
            if let Some((path, args)) = call_parts(left) {
                if path.as_slice() == ["std", "mem", "indexOf"] && args.len() == 3 {
                    return Expr::Call {
                        callee: Box::new(Expr::Field {
                            of: Box::new(args[1].clone()),
                            name: "contains".to_string(),
                        }),
                        args: vec![args[2].clone()],
                    };
                }
            }
        }
    }
    // `word.len` measures; the list pass already folded `.items`.
    if let Expr::Field { of, name } = &expr {
        if name == "len" {
            return Expr::Call {
                callee: Box::new(Expr::Name("len".to_string())),
                args: vec![(**of).clone()],
            };
        }
    }
    let Some((path, args)) = call_parts(&expr) else {
        return expr;
    };
    // The allocating case conversions, canonical and allocator-free.
    if let ["std", "ascii", conversion @ ("allocUpperString" | "allocLowerString")] =
        path.as_slice()
    {
        if args.len() == 2 {
            let method = match *conversion {
                "allocUpperString" => "upper",
                _ => "lower",
            };
            return Expr::Call {
                callee: Box::new(Expr::Field {
                    of: Box::new(args[1].clone()),
                    name: method.to_string(),
                }),
                args: Vec::new(),
            };
        }
    }
    if path.as_slice() == ["std", "mem", "eql"] && args.len() == 3 {
        let Expr::Call { mut args, .. } = expr else {
            unreachable!("call_parts said so");
        };
        let right = args.pop().expect("three args");
        let left = args.pop().expect("two left");
        return Expr::Binary {
            op: BinaryOp::Eq,
            left: Box::new(left),
            right: Box::new(right),
        };
    }
    expr
}

// --------------------------------------------------------------- Java lists

/// `ArrayList` spoken canonically: `add` is `append`, `size` is `len`, `get` is an index and
/// `set` an index assignment.
struct MapWords {
    /// What the language calls putting a value under a key.
    set: &'static str,
    /// Reading one back.
    get: &'static str,
    /// Asking whether a key is there.
    has: &'static str,
    /// How many keys there are.
    size: &'static str,
    /// The head of a constructor call: an empty map of that language's kind.
    made_by: &'static [&'static str],
}

fn map_words(language: Language) -> Option<MapWords> {
    let words = match language {
        Language::Rust => MapWords {
            set: "insert",
            get: "get",
            has: "contains_key",
            size: "len",
            made_by: &["HashMap::new", "BTreeMap::new", "HashMap::with_capacity"],
        },
        Language::Java => MapWords {
            set: "put",
            get: "get",
            has: "containsKey",
            size: "size",
            made_by: &["HashMap", "TreeMap", "LinkedHashMap"],
        },
        Language::TypeScript | Language::Tsx => MapWords {
            set: "set",
            get: "get",
            has: "has",
            size: "size",
            made_by: &["Map"],
        },
        Language::Zig => MapWords {
            set: "put",
            get: "get",
            has: "contains",
            size: "count",
            made_by: &["StringHashMap", "AutoHashMap"],
        },
        _ => return None,
    };
    Some(words)
}

/// The type an empty map holds, taken from the first key stored in it.
fn settle_map_types(module: &mut Module) {
    fn literal_type(e: &Expr) -> Option<Type> {
        match e {
            Expr::Int(_) => Some(Type::Int),
            Expr::Float(_) => Some(Type::Float),
            Expr::Str(_) | Expr::Template(_) => Some(Type::String),
            Expr::Bool(_) => Some(Type::Bool),
            _ => None,
        }
    }
    /// The first key and value stored into this map, anywhere below.
    fn first_stored(body: &[Stmt], name: &str) -> Option<(Type, Type)> {
        for stmt in body {
            if let Stmt::Assign {
                target: Expr::Index { of, index },
                value,
            } = stmt
            {
                if matches!(of.as_ref(), Expr::Name(n) if n == name) {
                    if let (Some(k), Some(v)) = (literal_type(index), literal_type(value)) {
                        return Some((k, v));
                    }
                }
            }
            for inner in substatements_ref(stmt) {
                if let Some(found) = first_stored(inner, name) {
                    return Some(found);
                }
            }
        }
        None
    }
    fn walk(body: &mut [Stmt]) {
        for at in 0..body.len() {
            for inner in substatements(&mut body[at]) {
                walk(inner);
            }
            let (name, empty) = match &body[at] {
                Stmt::Let {
                    name,
                    ty: None,
                    value: Some(Expr::MapLit(entries)),
                    ..
                } => (name.clone(), entries.is_empty()),
                _ => continue,
            };
            if !empty {
                continue;
            }
            let Some((keys, values)) = first_stored(&body[at + 1..], &name) else {
                continue;
            };
            if let Stmt::Let { ty, .. } = &mut body[at] {
                *ty = Some(Type::Map(Box::new(keys), Box::new(values)));
            }
        }
    }
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            walk(&mut f.body);
        }
    }
}

/// A map's own vocabulary, read onto the canonical index forms.
fn settle_maps(module: &mut Module, language: Language) {
    let Some(words) = map_words(language) else {
        return;
    };
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            let mut maps: Vec<String> = f
                .params
                .iter()
                .filter(|p| matches!(p.ty, Some(Type::Map(_, _))))
                .map(|p| p.name.clone())
                .collect();
            collect_map_bindings(&mut f.body, &words, &mut maps);
            if maps.is_empty() {
                continue;
            }
            rewrite_map_calls(&mut f.body, &words, &maps);
        }
    }
}

/// The bindings this function shows to be maps, and their constructors made
/// into the empty literal every writer already spells.
fn collect_map_bindings(body: &mut [Stmt], words: &MapWords, maps: &mut Vec<String>) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            collect_map_bindings(inner, words, maps);
        }
        let Stmt::Let {
            name, ty, value, ..
        } = stmt
        else {
            continue;
        };
        if matches!(ty, Some(Type::Map(_, _))) {
            maps.push(name.clone());
        }
        let Some(built) = value else { continue };
        if !made_by(built, words) && !carried_constructor(built, words) {
            continue;
        }
        maps.push(name.clone());
        *built = Expr::MapLit(Vec::new());
        // The type is the literal's now, and an `ArrayList`-shaped annotation
        // would send the writers looking for a list.
        if !matches!(ty, Some(Type::Map(_, _))) {
            *ty = None;
        }
    }
}

/// A constructor the reader could not place, whose text names a map.
fn carried_constructor(e: &Expr, words: &MapWords) -> bool {
    let Expr::Unsupported(what) = e else {
        return false;
    };
    let text = what.source.trim();
    words
        .made_by
        .iter()
        .any(|head| text.starts_with(head) || text.contains(&format!("{head}::")))
}

/// Is this expression one of the ways the language makes an empty map?
fn made_by(e: &Expr, words: &MapWords) -> bool {
    let (Expr::Call { callee, .. } | Expr::New { callee, .. }) = e else {
        return false;
    };
    let named = |name: &str| {
        words
            .made_by
            .iter()
            .any(|head| name == *head || name.starts_with(&format!("{head}<")))
    };
    match callee.as_ref() {
        Expr::Name(name) => named(name.trim_end_matches("<>")),
        // Zig writes `std.StringHashMap(V).init(allocator)`, so the head is a
        // field of the call that named the type.
        Expr::Field { of, name } if name == "init" => match of.as_ref() {
            Expr::Call { callee, .. } => {
                matches!(callee.as_ref(), Expr::Field { name, .. } | Expr::Name(name) if named(name))
            }
            _ => false,
        },
        Expr::Field { name, .. } => named(name),
        _ => false,
    }
}

/// Every call on a known map, written as the index form the writers spell.
fn rewrite_map_calls(body: &mut [Stmt], words: &MapWords, maps: &[String]) {
    fn on_map(e: &Expr, maps: &[String]) -> Option<String> {
        match e {
            Expr::Name(n) if maps.iter().any(|m| m == n) => Some(n.clone()),
            _ => None,
        }
    }
    fn fix(e: &mut Expr, words: &MapWords, maps: &[String]) {
        let each = |e: &mut Expr| fix(e, words, maps);
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                // Not into a member of a map: `m.count()` is one thing, and
                // rewriting the `m.count` half first left `len(m)()`.
                let member_of_a_map = matches!(callee.as_ref(), Expr::Field { of, .. }
                    if on_map(of, maps).is_some());
                if !member_of_a_map {
                    each(callee);
                }
                for a in args {
                    each(a);
                }
            }
            Expr::Binary { left, right, .. } => {
                each(left);
                each(right);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                each(operand)
            }
            Expr::Index { of, index } => {
                each(of);
                each(index);
            }
            Expr::Field { of, .. } => each(of),
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(inner) = part {
                        each(inner);
                    }
                }
            }
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => {
                each(condition);
                each(then);
                each(otherwise);
            }
            _ => {}
        }
        // `m.get(k)!` asserts the key is there, and a map index already says that in every
        // target: Zig writes `.?`, the rest index directly.
        if let Expr::Unary {
            op: UnaryOp::Unwrap,
            operand,
        } = e
        {
            let redundant = matches!(operand.as_ref(), Expr::Index { of, .. }
                if on_map(of, maps).is_some());
            if redundant {
                *e = std::mem::replace(operand.as_mut(), Expr::Null);
            }
            return;
        }
        // TypeScript asks a map its size through a property rather than a call,
        // so the count is a field access and not one.
        if let Expr::Field { of, name } = e {
            if name == words.size {
                if let Some(map) = on_map(of, maps) {
                    *e = Expr::Call {
                        callee: Box::new(Expr::Name("len".to_string())),
                        args: vec![Expr::Name(map)],
                    };
                }
            }
            return;
        }
        let Expr::Call { callee, args } = e else {
            return;
        };
        let Expr::Field { of, name } = callee.as_ref() else {
            return;
        };
        let Some(map) = on_map(of, maps) else {
            return;
        };
        if name == words.get && args.len() == 1 {
            *e = Expr::Index {
                of: Box::new(Expr::Name(map)),
                index: Box::new(args.remove(0)),
            };
        } else if name == words.has && args.len() == 1 {
            *e = Expr::Call {
                callee: Box::new(Expr::Field {
                    of: Box::new(Expr::Name(map)),
                    name: "contains".to_string(),
                }),
                args: vec![args.remove(0)],
            };
        } else if name == words.size && args.is_empty() {
            *e = Expr::Call {
                callee: Box::new(Expr::Name("len".to_string())),
                args: vec![Expr::Name(map)],
            };
        }
    }

    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            rewrite_map_calls(inner, words, maps);
        }
        // `m.insert(k, v)` standing alone is an assignment into the map.
        if let Stmt::Expr(Expr::Call { callee, args }) = stmt {
            if let Expr::Field { of, name } = callee.as_ref() {
                if name == words.set && args.len() == 2 {
                    if let Some(map) = on_map(of, maps) {
                        let value = args.pop().expect("two arguments");
                        let key = args.pop().expect("one left");
                        *stmt = Stmt::Assign {
                            target: Expr::Index {
                                of: Box::new(Expr::Name(map)),
                                index: Box::new(key),
                            },
                            value,
                        };
                        continue;
                    }
                }
            }
        }
        match stmt {
            Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => fix(e, words, maps),
            Stmt::Let { value: Some(e), .. } => fix(e, words, maps),
            Stmt::Assign { target, value } => {
                fix(target, words, maps);
                fix(value, words, maps);
            }
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                fix(condition, words, maps)
            }
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                fix(iterable, words, maps)
            }
            _ => {}
        }
    }
}

/// Only on bindings the function shows to be lists: declared `ArrayList`/`List`, or initialized
/// from a list literal or an `ArrayList` construction.
fn settle_java_lists(module: &mut Module) {
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            let mut lists: Vec<String> = f
                .params
                .iter()
                .filter(|p| is_list_type(&p.ty))
                .map(|p| p.name.clone())
                .collect();
            collect_list_bindings(&f.body, &mut lists);
            if lists.is_empty() {
                continue;
            }
            rewrite_java_lists(&mut f.body, &lists);
        }
    }
}

fn is_list_type(ty: &Option<Type>) -> bool {
    matches!(ty, Some(Type::List(_)))
        || matches!(ty, Some(Type::Named { name, .. })
            if name == "ArrayList" || name == "List" || name == "LinkedList")
}

fn collect_list_bindings(body: &[Stmt], lists: &mut Vec<String>) {
    for stmt in body {
        for inner in substatements_ref(stmt) {
            collect_list_bindings(inner, lists);
        }
        if let Stmt::Let {
            name, ty, value, ..
        } = stmt
        {
            let listish = is_list_type(ty)
                || matches!(value, Some(Expr::ListLit(_)))
                || matches!(value, Some(Expr::New { callee, .. } | Expr::Call { callee, .. })
                    if matches!(&**callee, Expr::Name(n)
                        if n == "ArrayList" || n == "LinkedList"));
            if listish && !lists.contains(name) {
                lists.push(name.clone());
            }
        }
    }
}

/// The read-only view of the nested bodies, for the collectors.
fn substatements_ref(stmt: &Stmt) -> Vec<&Vec<Stmt>> {
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

/// The set a construction stands for, in the words each language uses.
fn built_set(e: &Expr) -> Option<Expr> {
    let (Expr::Call { callee, args } | Expr::New { callee, args }) = e else {
        return None;
    };
    let named = match callee.as_ref() {
        Expr::Name(n) => n.rsplit(['.', ':']).next().unwrap_or(n).to_string(),
        Expr::Field { name, .. } => name.clone(),
        _ => return None,
    };
    let builds = matches!(
        named.as_str(),
        "set" | "Set" | "HashSet" | "TreeSet" | "LinkedHashSet" | "BTreeSet" | "frozenset"
    );
    if !builds {
        return None;
    }
    match args.as_slice() {
        [] => Some(Expr::SetLit(Vec::new())),
        // `new Set([a, b])` and `set([a, b])` take the members as one list.
        [Expr::ListLit(items)] => Some(Expr::SetLit(items.clone())),
        _ => None,
    }
}

/// The literal an immutable collection factory stands for.
fn immutable_collection(e: &Expr) -> Option<Expr> {
    let Expr::Call { callee, args } = e else {
        return None;
    };
    let Expr::Field { of, name } = callee.as_ref() else {
        return None;
    };
    if name != "of" {
        return None;
    }
    let Expr::Name(holder) = of.as_ref() else {
        return None;
    };
    match holder.rsplit('.').next().unwrap_or(holder) {
        "List" | "Set" => Some(Expr::ListLit(args.clone())),
        "Map" => {
            let mut entries = Vec::new();
            for pair in args.chunks(2) {
                if let [k, v] = pair {
                    entries.push((k.clone(), v.clone()));
                }
            }
            Some(Expr::MapLit(entries))
        }
        _ => None,
    }
}

/// The literal inside a mutable collection built around an immutable one.
fn wrapped_collection(e: &Expr) -> Option<Expr> {
    let (Expr::New { callee, args } | Expr::Call { callee, args }) = e else {
        return None;
    };
    let built = match callee.as_ref() {
        Expr::Name(n) => n.rsplit('.').next().unwrap_or(n),
        Expr::Field { name, .. } => name.as_str(),
        _ => return None,
    };
    let [inside] = args.as_slice() else {
        return None;
    };
    let Expr::Call { callee, args } = inside else {
        return None;
    };
    let Expr::Field { of, name } = callee.as_ref() else {
        return None;
    };
    if name != "of" {
        return None;
    }
    let holder = match of.as_ref() {
        Expr::Name(n) => n.rsplit('.').next().unwrap_or(n),
        _ => return None,
    };
    match (built, holder) {
        ("ArrayList" | "LinkedList", "List") => Some(Expr::ListLit(args.clone())),
        ("HashMap" | "TreeMap" | "LinkedHashMap", "Map") => {
            let mut entries = Vec::new();
            let mut pairs = args.chunks(2);
            while let Some([k, v]) = pairs.next() {
                entries.push((k.clone(), v.clone()));
            }
            Some(Expr::MapLit(entries))
        }
        _ => None,
    }
}

fn rewrite_java_lists(body: &mut [Stmt], lists: &[String]) {
    fn empty_construction(e: &Expr) -> bool {
        matches!(e, Expr::New { callee, args } | Expr::Call { callee, args }
            if args.is_empty()
                && matches!(&**callee, Expr::Name(n)
                    if n == "ArrayList" || n == "LinkedList"))
    }
    fn on_list(e: &Expr, lists: &[String]) -> Option<String> {
        match e {
            Expr::Name(n) if lists.iter().any(|l| l == n) => Some(n.clone()),
            _ => None,
        }
    }
    fn fix(e: &mut Expr, lists: &[String]) {
        let each = |e: &mut Expr| fix(e, lists);
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                each(callee);
                for a in args {
                    each(a);
                }
            }
            Expr::Binary { left, right, .. } => {
                each(left);
                each(right);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                each(operand)
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        each(e);
                    }
                }
            }
            Expr::Field { of, .. } | Expr::Index { of, .. } => each(of),
            _ => {}
        }
        let Expr::Call { callee, args } = e else {
            return;
        };
        let Expr::Field { of, name } = &**callee else {
            return;
        };
        let Some(list) = on_list(of, lists) else {
            return;
        };
        match (name.as_str(), args.len()) {
            ("add", 1) => {
                let value = args.remove(0);
                *e = Expr::Call {
                    callee: Box::new(Expr::Field {
                        of: Box::new(Expr::Name(list)),
                        name: "append".to_string(),
                    }),
                    args: vec![value],
                };
            }
            ("size", 0) => {
                *e = Expr::Call {
                    callee: Box::new(Expr::Name("len".to_string())),
                    args: vec![Expr::Name(list)],
                };
            }
            ("get", 1) => {
                let index = args.remove(0);
                *e = Expr::Index {
                    of: Box::new(Expr::Name(list)),
                    index: Box::new(index),
                };
            }
            _ => {}
        }
    }
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            rewrite_java_lists(inner, lists);
        }
        // `list.set(i, v)` as a statement is an index assignment.
        if let Stmt::Expr(Expr::Call { callee, args }) = stmt {
            if let Expr::Field { of, name } = &**callee {
                if name == "set" && args.len() == 2 {
                    if let Some(list) = on_list(of, lists) {
                        let value = args.pop().expect("two args");
                        let index = args.pop().expect("one left");
                        *stmt = Stmt::Assign {
                            target: Expr::Index {
                                of: Box::new(Expr::Name(list)),
                                index: Box::new(index),
                            },
                            value,
                        };
                        continue;
                    }
                }
            }
        }
        if let Stmt::Let { value: Some(v), .. } = stmt {
            if empty_construction(v) {
                *v = Expr::ListLit(Vec::new());
            }
            // `new ArrayList<>(List.of(…))` is the mutable list this writer emits for a list
            // literal, and `new HashMap<>(Map.of(…))` the map.
            if let Some(unwrapped) = wrapped_collection(v) {
                *v = unwrapped;
            }
            // A bare `List.of(…)` or `Map.of(…)` is the literal itself.
            if let Some(unwrapped) = immutable_collection(v) {
                *v = unwrapped;
            }
        }
        match stmt {
            Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => fix(e, lists),
            Stmt::Let { value: Some(e), .. } | Stmt::Assign { value: e, .. } => fix(e, lists),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => fix(condition, lists),
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                fix(iterable, lists)
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------ list element types

/// The element type of a list, read off what the function puts into it.
fn settle_list_element_types(module: &mut Module) {
    for item in &mut module.items {
        let functions: Vec<&mut Function> = match item {
            Item::Function(f) => vec![f],
            Item::Record(r) => r.methods.iter_mut().collect(),
            _ => continue,
        };
        for f in functions {
            let mut names: Vec<String> = Vec::new();
            collect_untyped_lists(&f.body, &mut names);
            for name in names {
                let mut observed: Option<Type> = None;
                let mut settled = true;
                observe_appends(&f.body, &name, &mut observed, &mut settled);
                if let (Some(element), true) = (observed, settled) {
                    retype_list(&mut f.body, &name, element);
                }
            }
        }
    }
}

fn collect_untyped_lists(body: &[Stmt], names: &mut Vec<String>) {
    for stmt in body {
        for inner in substatements_ref(stmt) {
            collect_untyped_lists(inner, names);
        }
        if let Stmt::Let {
            name, ty, value, ..
        } = stmt
        {
            let empty_list = matches!(value, Some(Expr::ListLit(items)) if items.is_empty());
            let loose = matches!(ty, None | Some(Type::List(_)));
            if empty_list && loose && !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
}

fn observe_appends(body: &[Stmt], name: &str, observed: &mut Option<Type>, settled: &mut bool) {
    fn literal_type(e: &Expr) -> Option<Type> {
        match e {
            Expr::Int(_) => Some(Type::Int),
            Expr::Float(_) => Some(Type::Float),
            Expr::Str(_) | Expr::Template(_) => Some(Type::String),
            Expr::Bool(_) => Some(Type::Bool),
            _ => None,
        }
    }
    for stmt in body {
        for inner in substatements_ref(stmt) {
            observe_appends(inner, name, observed, settled);
        }
        let Stmt::Expr(Expr::Call { callee, args }) = stmt else {
            continue;
        };
        let Expr::Field { of, name: method } = &**callee else {
            continue;
        };
        if method != "append" || !matches!(&**of, Expr::Name(n) if n == name) {
            continue;
        }
        match args.first().and_then(literal_type) {
            Some(ty) => match observed {
                None => *observed = Some(ty),
                Some(seen) if *seen == ty => {}
                Some(_) => *settled = false,
            },
            None => *settled = false,
        }
    }
}

fn retype_list(body: &mut [Stmt], name: &str, element: Type) {
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            retype_list(inner, name, element.clone());
        }
        if let Stmt::Let {
            name: bound, ty, ..
        } = stmt
        {
            if bound == name {
                *ty = Some(Type::List(Box::new(element.clone())));
            }
        }
    }
}

/// Zig's ArrayList idiom, folded to the canonical list.
fn zig_lists(body: &mut [Stmt]) {
    let mut lists: Vec<String> = Vec::new();
    collect_zig_lists(body, &mut lists);
    if lists.is_empty() {
        return;
    }
    rewrite_zig_lists(body, &lists);
}

fn collect_zig_lists(body: &[Stmt], lists: &mut Vec<String>) {
    for stmt in body {
        for inner in substatements_ref(stmt) {
            collect_zig_lists(inner, lists);
        }
        let Stmt::Let {
            name, ty, value, ..
        } = stmt
        else {
            continue;
        };
        let list_typed = matches!(ty, Some(Type::Named { name, .. }) if name.contains("ArrayList"))
            || matches!(ty, Some(Type::List(_)));
        let empty_init = matches!(value, Some(Expr::Field { name, .. }) if name == "empty")
            || matches!(value, Some(Expr::ListLit(items)) if items.is_empty());
        if list_typed && empty_init && !lists.contains(name) {
            lists.push(name.clone());
        }
    }
}

fn rewrite_zig_lists(body: &mut [Stmt], lists: &[String]) {
    fn element_of(ty: &Option<Type>) -> Type {
        match ty {
            Some(Type::List(inner)) => (**inner).clone(),
            Some(Type::Named { name, .. }) if name.contains("ArrayList") => {
                let inner = name
                    .split(['(', ')'])
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                match inner.as_str() {
                    "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize"
                    | "isize" => Type::Int,
                    "f32" | "f64" => Type::Float,
                    "[]const u8" => Type::String,
                    other if !other.is_empty() => Type::named(other),
                    _ => Type::named("anytype"),
                }
            }
            _ => Type::named("anytype"),
        }
    }
    fn fix(e: &mut Expr, lists: &[String]) {
        let each = |e: &mut Expr| fix(e, lists);
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                each(callee);
                for a in args {
                    each(a);
                }
            }
            Expr::Binary { left, right, .. } => {
                each(left);
                each(right);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                each(operand)
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        each(e);
                    }
                }
            }
            Expr::Field { of, .. } | Expr::Index { of, .. } => each(of),
            _ => {}
        }
        // `nums.items` is `nums`; the elements are the list.
        if let Expr::Field { of, name } = e {
            if name == "items" && matches!(&**of, Expr::Name(n) if lists.iter().any(|l| l == n)) {
                let inner = std::mem::replace(of.as_mut(), Expr::Null);
                *e = inner;
                return;
            }
        }
        // `nums.append(alloc, x)` sheds the allocator.
        if let Expr::Call { callee, args } = e {
            if let Expr::Field { of, name } = &**callee {
                if name == "append"
                    && args.len() == 2
                    && matches!(&**of, Expr::Name(n) if lists.iter().any(|l| l == n))
                {
                    args.remove(0);
                }
            }
        }
    }
    for stmt in body.iter_mut() {
        for inner in substatements(stmt) {
            rewrite_zig_lists(inner, lists);
        }
        if let Stmt::Let {
            name, ty, value, ..
        } = stmt
        {
            if lists.contains(name) {
                let element = element_of(ty);
                *ty = Some(Type::List(Box::new(element)));
                *value = Some(Expr::ListLit(Vec::new()));
            }
        }
        match stmt {
            Stmt::Expr(e) | Stmt::Throw(e) | Stmt::Return(Some(e)) => fix(e, lists),
            Stmt::Let { value: Some(e), .. } => fix(e, lists),
            Stmt::Assign { target, value } => {
                fix(target, lists);
                fix(value, lists);
            }
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => fix(condition, lists),
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                fix(iterable, lists)
            }
            _ => {}
        }
    }
}

/// Python's own idioms that are not already the canonical spelling.
fn python(expr: Expr) -> Expr {
    if let Some(folded) = fold_concat(&expr) {
        return folded;
    }
    // Python's `%` rounds with its division, toward negative infinity.
    if let Expr::Binary {
        op: BinaryOp::Rem,
        left,
        right,
    } = expr
    {
        return Expr::Binary {
            op: BinaryOp::FloorRem,
            left,
            right,
        };
    }
    // `asyncio.run(main())` is how an async entry says "call main".
    if let Expr::Call { callee, args } = &expr {
        if let Expr::Field { of, name } = callee.as_ref() {
            if matches!(of.as_ref(), Expr::Name(n) if n == "asyncio")
                && name == "run"
                && args.len() == 1
            {
                return args[0].clone();
            }
        }
    }
    expr
}

/// Rename one method on a receiver, keeping everything else.
fn rename_method(expr: Expr, from: &str, to: &str, argc: usize) -> Result<Expr, Expr> {
    match expr {
        Expr::Call { callee, args } if args.len() == argc => match *callee {
            Expr::Field { of, name } if name == from => Ok(Expr::Call {
                callee: Box::new(Expr::Field {
                    of,
                    name: to.to_string(),
                }),
                args,
            }),
            other => Err(Expr::Call {
                callee: Box::new(other),
                args,
            }),
        },
        other => Err(other),
    }
}
