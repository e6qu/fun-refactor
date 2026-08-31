//! Writing the shared representation as Lean 4.
//!
//! Lean is the one target here that is a proof assistant, and the one whose `def` is a
//! mathematical definition rather than a recipe. Two facts shape everything below.
//!
//! A `do` block carries the imperative half. Lean 4 has `let mut`, `while`, `for`,
//! `break`, `continue` and `return`, and `Id.run do` turns a block of them into a pure
//! value. So a loop keeps its shape, and the output reads like the source.
//!
//! Order is load-bearing. Lean reads a file top to bottom and refuses a name it has not
//! yet met, which no other target here does. So the writer sorts the module's
//! declarations by what they name, and puts a cycle inside `mutual`.

use super::*;

/// The names Lean 4 will not take as an identifier.
///
/// `tests/translate_lean.rs` asks the grammar which words these are, one file per
/// word. A list that falls behind the grammar fails a build, rather than writing a
/// `prefix` field that swallows the declaration after it.
pub(super) const RESERVED: &[&str] = &[
    "abbrev",
    "at",
    "attribute",
    "axiom",
    "builtin_initialize",
    "by",
    "calc",
    "catch",
    "class",
    "constant",
    "def",
    "deriving",
    "do",
    "elab",
    "else",
    "end",
    "example",
    "exists",
    "export",
    "extends",
    "finally",
    "for",
    "forall",
    "from",
    "fun",
    "have",
    "if",
    "import",
    "in",
    "include",
    "inductive",
    "infix",
    "infixl",
    "infixr",
    "initialize",
    "instance",
    "lemma",
    "let",
    "local",
    "macro",
    "macro_rules",
    "match",
    "meta",
    "mut",
    "mutual",
    "namespace",
    "noncomputable",
    "notation",
    "omit",
    "opaque",
    "open",
    "partial",
    "postfix",
    "prefix",
    "prelude",
    "private",
    "protected",
    "public",
    "return",
    "scoped",
    "section",
    "set_option",
    "show",
    "structure",
    "syntax",
    "then",
    "theorem",
    "try",
    "universe",
    "universes",
    "unless",
    "unsafe",
    "variable",
    "where",
    "while",
    "with",
];

/// Every declaration a `structure` or `inductive` gets, so that a generated type prints,
/// compares, and stands as the default of a `partial def`.
const DERIVING: &str = "deriving Repr, Inhabited, BEq";

/// A value as text, which for a fraction is not what Lean's own `toString` writes.
///
/// Every other target here prints a whole-valued fraction without a fractional part, and
/// prints the rest without trailing zeros. Lean prints six decimal places either way, so
/// a transcript that agrees everywhere else would disagree here for the formatting alone.
fn shown(out: &mut Out, e: &Expr) -> String {
    let rendered = arg(out, e);
    match static_type(out, e) {
        Some(Type::Float) => {
            out.lean_helpers.insert("frShow");
            format!("frShow {rendered}")
        }
        Some(Type::String) => rendered,
        _ => format!("toString {rendered}"),
    }
}

/// The same, for a place that takes text already: a hole in an interpolated string.
fn shown_in_hole(out: &mut Out, e: &Expr) -> String {
    match static_type(out, e) {
        Some(Type::Float) => {
            out.lean_helpers.insert("frShow");
            let rendered = arg(out, e);
            format!("frShow {rendered}")
        }
        _ => expr(out, e),
    }
}

/// A module, written out.
pub(super) fn write(out: &mut Out, module: &Module) {
    // A function that fails is one that acts. Lean's `panic!` answers with the type's
    // default value and carries on. A failure a caller means to catch has to be a
    // `throw`, and a `throw` needs a monad to leave through.
    out.lean_io = io_functions(module);
    out.lean_io.extend(out.throwing.iter().cloned());
    // A caller of a failing function fails with it.
    loop {
        let known = out.lean_io.clone();
        let grew: Vec<String> = every_function(module)
            .filter(|f| !known.contains(&f.name) && calls_any(&f.body, &known))
            .map(|f| f.name.clone())
            .collect();
        if grew.is_empty() {
            break;
        }
        out.lean_io.extend(grew);
    }
    out.lean_partial = self_recursive(module);
    out.lean_constructed = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) if r.methods.iter().any(|m| m.is_constructor) => Some(r.name.clone()),
            _ => None,
        })
        .collect();

    for line in &module.doc {
        out.line(&format!("-- {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }
    let at = out.text.len();
    for group in in_declaration_order(module) {
        match group.as_slice() {
            [only] => item(out, module, *only),
            // Lean sees the members of a `mutual` block all at once, which is the only
            // way it reads a cycle.
            several => {
                out.line("mutual");
                for index in several {
                    item(out, module, *index);
                }
                out.line("end");
                out.blank();
            }
        }
    }
    // Ahead of the declarations that call them, since Lean reads a file once.
    helpers(out, at);
    // `import` precedes every other command, and the writer knows by now whether it
    // reached the standard library. `ty` and the collection literals say so as they go.
    // One place decides, rather than two that can disagree.
    if out.lean_helpers.contains("Std") {
        out.text.insert_str(0, "import Std\n\n");
    }
}

/// The definitions the writer's own lowerings turned out to need, inserted where every
/// use of them can see them.
fn helpers(out: &mut Out, at: usize) {
    if out.lean_helpers.is_empty() {
        return;
    }
    let mut scratch = Out::new(Language::Lean);
    if out.lean_helpers.contains("frShow") {
        scratch.line("/-- A value as text, spelled the way the other targets spell it.");
        scratch.line("A whole-valued fraction loses its fractional part, and the rest");
        scratch.line("lose their trailing zeros. Lean writes six decimals either way. -/");
        scratch.line("def frShow (value : Float) : String := Id.run do");
        scratch.open();
        scratch.line("if !value.isFinite then");
        scratch.open();
        scratch.line("return toString value");
        scratch.close();
        scratch.line("if value == value.round then");
        scratch.open();
        scratch.line("return toString value.toInt64");
        scratch.close();
        scratch.line(concat!(
            "let digits := List.reverse (List.dropWhile (fun c => c == '0') ",
            "(List.reverse (toString value).toList))"
        ));
        scratch.line("let shown := String.ofList digits");
        scratch.line("if shown.endsWith \".\" then");
        scratch.open();
        scratch.line("return shown ++ \"0\"");
        scratch.close();
        scratch.line("return shown");
        scratch.close();
        scratch.blank();
    }
    if out.lean_helpers.contains("frTrunc") {
        scratch.line("/-- A fraction with its fractional part dropped, toward zero. -/");
        scratch.line("def frTrunc (value : Float) : Float :=");
        scratch.open();
        scratch.line("if value < 0.0 then value.ceil else value.floor");
        scratch.close();
        scratch.blank();
    }
    if out.lean_helpers.contains("frRem") {
        scratch.line("/-- The remainder of a division over fractions, which Lean has no");
        scratch.line("operator for. It rounds toward zero, as every other target here does. -/");
        scratch.line("def frRem (a : Float) (b : Float) : Float :=");
        scratch.open();
        scratch.line("a - frTrunc (a / b) * b");
        scratch.close();
        scratch.blank();
    }
    out.text.insert_str(at, &scratch.text);
}

/// One top-level item.
fn item(out: &mut Out, module: &Module, index: usize) {
    match &module.items[index] {
        Item::Function(f) => {
            function(out, f, None);
            out.blank();
        }
        Item::Record(r) => record(out, r),
        Item::Sum(s) => sum(out, s),
        Item::Newtype(n) => {
            for line in &n.doc {
                out.line(&format!("-- {line}"));
            }
            // `abbrev` and not `def`. A distinct type over `Int` needs its own `Repr`,
            // arithmetic and coercions. Inventing those invents a design the source
            // never wrote.
            let base = ty(out, &n.base);
            out.line(&format!("abbrev {} := {base}", out.name(&n.name)));
            out.fidelity.notes.push(format!(
                "`{}` is an abbreviation in Lean: a distinct type over {} would need \
                 its own instances, and the source declared none",
                n.name, n.base
            ));
            out.fidelity.newtypes += 1;
            out.blank();
        }
        Item::Constant(c) => {
            for line in &c.doc {
                out.line(&format!("-- {line}"));
            }
            let annotation =
                c.ty.as_ref()
                    .map(|t| ty(out, t))
                    .map(|t| format!(" : {t}"))
                    .unwrap_or_default();
            let value = expr(out, &c.value);
            out.line(&format!("def {}{annotation} := {value}", out.name(&c.name)));
            out.fidelity.constants += 1;
            out.blank();
        }
        Item::Import { text, line, .. } => {
            out.fidelity.imports_listed += 1;
            let header = out.comment(&format!(
                "the source imported this at line {line}; the equivalent here is yours \
                 to add"
            ));
            out.line(&header);
            for l in text.lines() {
                let commented = out.comment(l);
                out.line(&commented);
            }
            out.blank();
        }
        Item::Test { doc, name, body } => {
            for l in doc {
                out.line(&format!("-- {l}"));
            }
            let slug = out.legal(camel(&format!("test {name}")));
            out.line(&format!("def {slug} : IO Unit := do"));
            out.open();
            let was = std::mem::replace(&mut out.lean_in_io, true);
            let displaced = std::mem::replace(&mut out.lean_mut, mutated(body));
            block(out, body, None);
            out.lean_mut = displaced;
            out.lean_in_io = was;
            out.close();
            // `#eval` runs it during elaboration, the only moment a Lean file has.
            out.line(&format!("#eval {slug}"));
            out.fidelity.functions += 1;
            out.blank();
        }
        Item::Statement(stmt) if calls_declared_main(out, stmt) => out.note_once(ENTRY_DROPPED),
        Item::Statement(stmt) => carried_statement(out, stmt, expr),
        Item::Unsupported(u) => {
            carry(out, u);
            out.blank();
        }
    }
}

/// A record: a `structure`, and its methods as definitions in the namespace the
/// structure opens. `p.area` reaches `Shape.area` by that namespace alone, so a method
/// call crosses without a word changing.
fn record(out: &mut Out, r: &Record) {
    for line in &r.doc {
        out.line(&format!("-- {line}"));
    }
    let name = out.name(&r.name);
    out.line(&format!("structure {name} where"));
    out.open();
    for f in &r.fields {
        for line in &f.doc {
            out.line(&format!("-- {line}"));
        }
        let declared = match &f.ty {
            Some(t) => ty(out, t),
            None => unknown(out, &f.name),
        };
        out.line(&format!("{} : {declared}", out.field(&f.name)));
    }
    out.close();
    out.line(DERIVING);
    out.fidelity.records += 1;
    out.blank();

    // A field with a value the source declared becomes the one function that builds the
    // record with it, since a Lean field takes no default.
    let defaults: Vec<&Field> = r.fields.iter().filter(|f| f.default.is_some()).collect();
    if !defaults.is_empty() {
        out.fidelity.notes.push(format!(
            "`{}` declares a starting value for {}; a Lean field takes none, so \
             `{name}.default` carries them",
            r.name,
            defaults
                .iter()
                .map(|f| format!("`{}`", f.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let mut spelled: BTreeMap<String, usize> = BTreeMap::new();
    for m in &methods_of(out, r, false) {
        let seen = spelled.entry(m.name.clone()).or_insert(0);
        *seen += 1;
        let mut renamed = m.clone();
        if *seen > 1 {
            out.note_once(
                "overloads share a name the target refuses to repeat; later overloads \
                 take a numbered name.",
            );
            renamed.name = format!("{}{}", m.name, *seen);
        }
        function(out, &renamed, Some(&name));
        out.blank();
    }
}

/// A closed choice: an `inductive`, each variant a constructor of it.
fn sum(out: &mut Out, s: &Sum) {
    for line in &s.doc {
        out.line(&format!("-- {line}"));
    }
    let name = out.name(&s.name);
    out.line(&format!("inductive {name} where"));
    out.open();
    for variant in &s.variants {
        for line in &variant.doc {
            out.line(&format!("-- {line}"));
        }
        let spelled = out.legal(camel(&variant.name));
        let mut fields: Vec<String> = Vec::new();
        for f in &variant.fields {
            let declared = match &f.ty {
                Some(t) => ty(out, t),
                None => unknown(out, &f.name),
            };
            fields.push(format!(" ({} : {declared})", out.field(&f.name)));
        }
        out.line(&format!("| {spelled}{}", fields.concat()));
    }
    out.close();
    out.line(DERIVING);
    out.fidelity.sums += 1;
    out.blank();
}

/// One `def`. `owner` names the structure whose namespace it goes in, where it has one.
fn function(out: &mut Out, f: &Function, owner: Option<&str>) {
    let f = &with_hoisted_bindings(f, &out.function_returns);
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);
    settle_list_element_types(f, out);
    settle_set_element_types(f, out);
    settle_inferred_bindings(f, out);
    out.fn_returns = f.returns.clone();
    let known_returns = out.function_returns.clone();
    settle_call_bindings(f, &known_returns, &mut out.binding_types);

    for line in &f.doc {
        out.line(&format!("-- {line}"));
    }
    if f.is_async {
        let note = out.comment(
            "declared async in the source; Lean has no `async`, and this runs to \
             completion where it stands.",
        );
        out.line(&note);
    }

    let mut foreign = false;
    let mut unannotated = false;
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    // The receiver is the first argument and the reason `p.area` resolves. A constructor
    // takes none: it makes the value the others act on.
    if let (Some(owner), true) = (owner, f.receiver_binding.is_some() && !f.is_constructor) {
        params.push(format!("({} : {owner})", receiver_word(out.language)));
    }
    for p in &f.params {
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(format!("({spelled} : Unit)"));
            continue;
        }
        let declared = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                ty(out, t)
            }
            None => {
                unannotated = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("({spelled} : {declared})"));
    }

    // Lean writes a return type on every definition and infers none across a `do`.
    let answers = match &f.returns {
        Some(Type::Unit) | None if !returns_a_value(f) => Type::Unit,
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            t.clone()
        }
        None => {
            unannotated = true;
            inferred_return(out, f).unwrap_or_else(|| {
                out.fidelity
                    .notes
                    .push(format!("`{}` had no declared type in the source", f.name));
                Type::Unit
            })
        }
    };

    let in_io = out.lean_io.contains(&f.name);
    let answer = match in_io {
        true => {
            let inner = atom(out, &answers);
            format!("IO {inner}")
        }
        false => ty(out, &answers),
    };
    let name = match owner {
        Some(owner) => format!("{owner}.{}", out.function_name(f)),
        None => out.function_name(f),
    };
    // `partial` is the honest word for a loop Lean cannot see the end of. It asks for a
    // default value of the answer, not a termination proof.
    let word = match out.lean_partial.contains(&f.name) {
        true => "partial def",
        false => "def",
    };
    // A body with statements in it is a `do` block, pure or not, and `Id.run` turns one
    // of those into a value.
    let opener = match (in_io, f.body.is_empty()) {
        (_, true) => String::new(),
        (true, false) => " do".to_string(),
        (false, false) => " Id.run do".to_string(),
    };
    out.line(&format!(
        "{word} {name}{}{} : {answer} :={opener}",
        match params.is_empty() {
            true => String::new(),
            false => " ".to_string(),
        },
        params.join(" ")
    ));
    out.open();
    let was = std::mem::replace(&mut out.lean_in_io, in_io);
    let displaced = std::mem::replace(&mut out.lean_mut, mutated(&f.body));
    block(out, &f.body, Some(&answers));
    out.lean_mut = displaced;
    out.lean_in_io = was;
    out.close();

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

/// A body. An empty `do` is a syntax error. So an empty body answers with the one value
/// its type has, or says out loud that it has none.
fn block(out: &mut Out, body: &[Stmt], answers: Option<&Type>) {
    if body.is_empty() {
        empty_body(out, answers);
        return;
    }
    // A `do` block answers with its last element. Where the source wrote a value there
    // and no `return`, that value is the answer and naming it would drop it.
    let answers_with_a_value = matches!(answers, Some(t) if *t != Type::Unit);
    let before = out.text.len();
    statements(out, body, answers_with_a_value);
    close_block(out, before, answers);
}

/// Finish a block, where what went into it left no element to finish on.
///
/// A body of comments leaves the `do` with nothing in it. A body ending in one leaves
/// the layout open, and the declaration after it lands inside this one.
fn close_block(out: &mut Out, before: usize, answers: Option<&Type>) {
    let last = out.text[before..]
        .lines()
        .map(str::trim_start)
        .rfind(|line| !line.is_empty());
    if last.is_some_and(|line| !line.starts_with("--")) {
        return;
    }
    empty_body(out, answers);
}

/// What a body with nothing in it answers with.
fn empty_body(out: &mut Out, answers: Option<&Type>) {
    match answers {
        // An invented value would be a guess at what the body did.
        Some(t) if *t != Type::Unit => out.line("panic! \"not translated\""),
        _ if out.lean_in_io => out.line("pure ()"),
        _ => out.line("()"),
    }
}

/// A body under a `do`-carrying keyword: `then`, `else`, a loop, a match arm.
fn nested(out: &mut Out, body: &[Stmt], tail: bool) {
    out.open();
    if body.is_empty() {
        // Every branch of a Lean `if` produces a value, so an empty one produces the
        // only value `Unit` has.
        out.line("pure ()");
    } else {
        let before = out.text.len();
        statements(out, body, tail);
        close_block(out, before, None);
    }
    out.close();
}

/// A run of statements. `tail` says that the last one's value is the block's.
fn statements(out: &mut Out, body: &[Stmt], tail: bool) {
    for (at, s) in body.iter().enumerate() {
        let last = at + 1 == body.len();
        // Nothing between here and the end of the block leaves it. So the deferral is
        // a plain reordering: the rest runs, then the cleanup. Lean has no scope-exit
        // hook, and this case needs none.
        if let Stmt::Defer(cleanup) = s {
            if !exits_anywhere(&body[at + 1..]) {
                if tail {
                    out.note_once(
                        "a deferral moved past the value this block answers with, so \
                         the cleanup runs last and answers in its place.",
                    );
                }
                statements(out, &body[at + 1..], false);
                statements(out, cleanup, tail);
                return;
            }
        }
        stmt(out, s, tail && last);
    }
}

fn stmt(out: &mut Out, s: &Stmt, tail: bool) {
    match s {
        Stmt::Comment(text) => {
            for line in text.lines() {
                out.line(&format!("-- {line}"));
            }
        }
        Stmt::Return(None) => out.line("return"),
        Stmt::Return(Some(e)) => {
            let want = out.fn_returns.clone();
            let value = coerced(out, e, want.as_ref());
            out.line(&format!("return {value}"));
        }
        Stmt::Let {
            name,
            ty: declared,
            value,
            mutable,
        } => {
            let spelled = out.name(name);
            // The annotation is not decoration. Lean reads a bare `0` as a `Nat`, whose
            // subtraction stops at zero. A counter the source called an integer says so
            // here, or counts differently.
            let settled = declared
                .clone()
                .or_else(|| out.binding_types.get(name).cloned())
                .filter(writable);
            let annotation = settled
                .as_ref()
                .map(|t| ty(out, t))
                .map(|t| format!(" : {t}"))
                .unwrap_or_default();
            // A binding nothing writes to is not `mut`, whatever the source said, and
            // Lean warns about the ones that are.
            let word = match *mutable || out.lean_mut.contains(name) {
                true => "let mut",
                false => "let",
            };
            let rendered = match value {
                Some(v) => coerced(out, v, settled.as_ref()),
                // Lean has no uninitialised binding; `default` is the value of the type
                // and not a guess about the source.
                None => "default".to_string(),
            };
            out.line(&format!("{word} {spelled}{annotation} := {rendered}"));
        }
        Stmt::Assign { target, value } => assign(out, target, value),
        Stmt::TupleAssign {
            names,
            value,
            declares,
            source,
            line,
        } => {
            let rendered = expr(out, value);
            let spelled: Vec<String> = names.iter().map(|n| out.name(n)).collect();
            match declares {
                true => out.line(&format!("let ({}) := {rendered}", spelled.join(", "))),
                // Lean's `do` reassigns one name at a time, and a tuple pattern on the
                // left of `:=` declares rather than reassigns.
                false => carry(
                    out,
                    &Unsupported {
                        construct: "multiple assignment".to_string(),
                        source: source.clone(),
                        line: *line,
                    },
                ),
            }
        }
        Stmt::If {
            condition,
            then,
            otherwise,
        } => {
            let test = expr(out, condition);
            // Both branches answer, or neither does: an `if` with one branch is a
            // statement in Lean's `do` and produces nothing.
            let branches_answer = tail && !otherwise.is_empty();
            out.line(&format!("if {test} then"));
            nested(out, then, branches_answer);
            // An `if` with no `else` is a statement in Lean's `do` and needs no branch.
            if !otherwise.is_empty() {
                out.line("else");
                nested(out, otherwise, branches_answer);
            }
        }
        Stmt::IfPresent {
            binding,
            value,
            then,
            otherwise,
        } => {
            let subject = expr(out, value);
            let bound = out.name(binding);
            out.line(&format!("match {subject} with"));
            out.line(&format!("| some {bound} =>"));
            nested(out, then, tail);
            out.line("| none =>");
            nested(out, otherwise, tail);
        }
        Stmt::While { condition, body } => {
            let test = expr(out, condition);
            // The brackets are load-bearing. Lean applies by juxtaposition, so a bare
            // `while i < 3 do` reads the `do` block as an argument of `3`.
            out.line(&format!("while ({test}) do"));
            nested(out, body, false);
        }
        Stmt::WhilePresent {
            binding,
            value,
            body,
        } => {
            // Lean has no `while let`. `repeat` with the match inside re-reads the
            // subject each pass, as the source's loop does.
            let bound = out.name(binding);
            out.line("repeat");
            out.open();
            let subject = expr(out, value);
            out.line(&format!("match {subject} with"));
            out.line(&format!("| some {bound} =>"));
            nested(out, body, false);
            out.line("| none => break");
            out.close();
        }
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => {
            let over = expr(out, iterable);
            let bound = out.name(binding);
            out.line(&format!("for {bound} in {over} do"));
            nested(out, body, false);
        }
        Stmt::ForEachIndexed {
            index,
            binding,
            iterable,
            body,
        } => {
            let over = arg(out, iterable);
            let i = out.name(index);
            let bound = out.name(binding);
            // `zipIdx` pairs each element with its position, counted from zero, as the
            // source's loop counts.
            out.line(&format!("for ({bound}, {i}) in {over}.zipIdx do"));
            out.open();
            out.line(&format!("let {i} := Int.ofNat {i}"));
            out.close();
            nested(out, body, false);
        }
        Stmt::CountedFor {
            init,
            condition,
            update,
            body,
            source,
            line,
        } => counted_for(out, init, condition, update, body, source, *line),
        // A chain and not a `match`: Lean matches a value against the constructors of
        // its type, and the literals here select on equality. A `Float` has no
        // constructors to match at all.
        Stmt::Switch {
            subject,
            arms,
            default,
        } => {
            if arms.is_empty() {
                statements(out, default, tail);
                return;
            }
            let over = arg(out, subject);
            chain(out, &over, arms, default, tail);
        }
        Stmt::MatchVariants {
            subject,
            sum,
            arms,
            default,
        } => {
            let over = expr(out, subject);
            out.line(&format!("match {over} with"));
            for arm in arms {
                let variant = constructor(out, sum, &arm.variant);
                let bound: Vec<String> = arm
                    .bindings
                    .iter()
                    .map(|(_, local)| format!(" {}", out.name(local)))
                    .collect();
                out.line(&format!("| {variant}{} =>", bound.concat()));
                nested(out, &arm.body, tail);
            }
            if !default.is_empty() {
                out.line("| _ =>");
                nested(out, default, tail);
            }
        }
        Stmt::Block(body) => {
            // Lean's `do` scopes each binding to the rest of the block, so the braces the
            // source wrote have nothing to add.
            statements(out, body, tail);
        }
        Stmt::Expr(e) => expression_statement(out, e, tail),
        Stmt::Assert { condition, message } => {
            let test = expr(out, condition);
            let words = match message {
                Some(m) => arg(out, m),
                None => quoted(Language::Lean, "assertion failed").to_string(),
            };
            out.line(&format!("if !({test}) then"));
            out.open();
            out.line(&failure(out, &words));
            out.close();
        }
        Stmt::Throw(e) => {
            let value = error_value(out, e);
            out.line(&failure(out, &value));
        }
        Stmt::Try {
            body,
            catches,
            finally,
            source,
            line,
        } => {
            if !out.lean_in_io {
                carry(
                    out,
                    &Unsupported {
                        construct: "try".to_string(),
                        source: source.clone(),
                        line: *line,
                    },
                );
                return;
            }
            out.line("try");
            nested(out, body, tail);
            for catch in catches {
                let bound = catch
                    .binding
                    .as_deref()
                    .map(|b| out.name(b))
                    .unwrap_or_else(|| "_".to_string());
                out.line(&format!("catch {bound} =>"));
                out.catch_bindings.push(bound);
                nested(out, &catch.body, tail);
                out.catch_bindings.pop();
            }
            if catches.is_empty() {
                out.line("catch _ =>");
                out.open();
                out.line("pure ()");
                out.close();
            }
            // Lean has no `finally`. Running the block after the `try` is the same thing
            // for every path that does not leave the function, and this says which.
            if !finally.is_empty() {
                out.note_once(
                    "Lean has no `finally`; the block runs after the `try`, which \
                     differs where a handler leaves the function early.",
                );
                statements(out, finally, false);
            }
        }
        Stmt::Break => out.line("break"),
        Stmt::Continue => out.line("continue"),
        Stmt::BreakWith { label, value } => {
            let rendered = match value {
                Some(v) => expr(out, v),
                None => String::new(),
            };
            carry_labeled_break(out, label, &rendered);
        }
        Stmt::LocalFunction(f) => {
            // A nested function is a binding whose value is a function, which is the one
            // form Lean has for it inside a block.
            let params: Vec<String> = f.params.iter().map(|p| out.name(&p.name)).collect();
            let name = out.name(&f.name);
            out.line(&format!(
                "let {name} := fun {} => Id.run do",
                params.join(" ")
            ));
            nested(out, &f.body, returns_a_value(f));
        }
        // `statements` reorders every deferral whose scope nothing leaves early. What
        // reaches here is the rest, and Lean has no hook that runs on the way out.
        Stmt::Defer(body) | Stmt::ErrDefer(body) => {
            out.note_once(
                "something leaves this scope early, and Lean has no hook that runs on \
                 the way out. The deferred statements stand where the source put them.",
            );
            statements(out, body, false);
        }
        Stmt::Unsupported(u) => carry(out, u),
    }
}

/// The arms of a choice, each `if` inside the `else` of the one before it.
///
/// `else if` on one line says the same thing and reads better, and the grammar cannot
/// read it: B832. One shape for both is worth more than the line it saves.
fn chain(out: &mut Out, over: &str, arms: &[(Vec<Expr>, Vec<Stmt>)], default: &[Stmt], tail: bool) {
    let Some(((literals, body), rest)) = arms.split_first() else {
        statements(out, default, tail);
        return;
    };
    let test = literals
        .iter()
        .map(|l| {
            let value = arg(out, l);
            format!("{over} == {value}")
        })
        .collect::<Vec<_>>()
        .join(" || ");
    out.line(&format!("if {test} then"));
    nested(out, body, tail);
    out.line("else");
    out.open();
    let before = out.text.len();
    chain(out, over, rest, default, tail);
    close_block(out, before, None);
    out.close();
}

/// A counted loop: a range where the header walks one, and a `while` where it does not.
fn counted_for(
    out: &mut Out,
    init: &Option<Box<Stmt>>,
    condition: &Option<Expr>,
    update: &Option<Box<Stmt>>,
    body: &[Stmt],
    source: &str,
    line: usize,
) {
    if let Some((name, start, bound, step)) =
        counted_range(init.as_deref(), condition.as_ref(), update.as_deref(), body)
    {
        if step == 1 {
            let from = arg(out, start);
            let to = arg(out, bound);
            let spelled = out.name(name);
            out.line(&format!(
                "for {spelled} in [Int.toNat {from}:Int.toNat {to}] do"
            ));
            out.open();
            out.line(&format!("let {spelled} := Int.ofNat {spelled}"));
            out.close();
            nested(out, body, false);
            return;
        }
    }
    // Anything else is the loop written out: the counter, the test, and the step where
    // the source put it. `continue` would skip the step, so a body with one carries.
    let Some(init) = init else {
        carry(
            out,
            &Unsupported {
                construct: "counted for".to_string(),
                source: source.to_string(),
                line,
            },
        );
        return;
    };
    if continues_here(body) {
        out.note_once(
            "a `continue` here would skip the loop's own step. This carries the header \
             rather than write a step Lean jumps over.",
        );
        carry(
            out,
            &Unsupported {
                construct: "counted for".to_string(),
                source: source.to_string(),
                line,
            },
        );
        return;
    }
    stmt(out, init, false);
    let test = match condition {
        Some(c) => expr(out, c),
        None => "true".to_string(),
    };
    out.line(&format!("while ({test}) do"));
    out.open();
    statements(out, body, false);
    if let Some(update) = update {
        stmt(out, update, false);
    }
    out.close();
}

/// An assignment. Lean reassigns a name, and a field or an element through the record or
/// the array that holds it.
fn assign(out: &mut Out, target: &Expr, value: &Expr) {
    let rendered = expr(out, value);
    match target {
        Expr::Name(name) => {
            let spelled = out.value_name(name);
            out.line(&format!("{spelled} := {rendered}"));
        }
        // `p.x := v` is `p := { p with x := v }`, and Lean's `do` writes the short form.
        Expr::Field { of, name } => {
            let holder = expr(out, of);
            let field = out.field(name);
            out.line(&format!(
                "{holder} := {{ {holder} with {field} := {rendered} }}"
            ));
        }
        Expr::Index { of, index } => {
            let holder = expr(out, of);
            let at = arg(out, index);
            match holds_a_map(out, of) {
                true => out.line(&format!("{holder} := {holder}.insert {at} {rendered}")),
                false => out.line(&format!(
                    "{holder} := {holder}.set! (Int.toNat {at}) {rendered}"
                )),
            }
        }
        other => {
            let left = expr(out, other);
            out.note_once(
                "Lean assigns to a name, a field or an element, and this target is none \
                 of the three.",
            );
            out.line(&out.comment(&format!("{MARKER}: `{left} := {rendered}`")));
        }
    }
}

/// A statement that is an expression. The ones that grow a collection are assignments in
/// Lean, since its collections answer with a new value rather than changing in place.
fn expression_statement(out: &mut Out, e: &Expr, tail: bool) {
    if let Expr::Call { callee, args } = e {
        if let (Some(receiver), Some(name)) = callee_parts(callee) {
            let grows = match name {
                "append" => Some("push"),
                "add" => Some("insert"),
                "remove" => Some("erase"),
                _ => None,
            };
            if let (Some(method), [only]) = (grows, args.as_slice()) {
                let holder = expr(out, &receiver.clone());
                let value = arg(out, only);
                out.line(&format!("{holder} := {holder}.{method} {value}"));
                return;
            }
        }
    }
    // A call that acts is the statement, and takes no arrow: nothing here reads what it
    // produced.
    if let Expr::Call { callee, args } = e {
        if acts_here(out, callee) {
            let rendered = call_text(out, callee, args);
            out.line(&rendered);
            return;
        }
    }
    let rendered = expr(out, e);
    // The last element of a `do` block is the block's value, and this is a value the
    // source wrote where a `return` would go.
    if tail || (out.lean_in_io && performs_io(out, e)) {
        out.line(&rendered);
        return;
    }
    // Anything else answers with a value, and a `do` block will not take a value it has
    // no name for.
    out.line(&format!("let _ := {rendered}"));
}

/// The one way this body reports a failure, given the monad it is in.
fn failure(out: &Out, message: &str) -> String {
    match out.lean_in_io {
        true => format!("throw (IO.userError {message})"),
        false => format!("panic! {message}"),
    }
}

/// A thrown value as the text of the failure it becomes.
fn error_value(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Str(_) | Expr::Template(_) => arg(out, e),
        Expr::Call { args, .. } | Expr::New { args, .. } => match args.as_slice() {
            [only @ (Expr::Str(_) | Expr::Template(_))] => arg(out, only),
            _ => {
                let rendered = expr(out, e);
                format!("(toString {rendered})")
            }
        },
        _ => {
            let rendered = arg(out, e);
            format!("(toString {rendered})")
        }
    }
}

/// Does this expression reach a runtime?
fn performs_io(out: &Out, e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            let (_, name) = callee_parts(callee);
            name.is_some_and(|n| n == "print" || out.lean_io.contains(n))
                || args.iter().any(|a| performs_io(out, a))
        }
        Expr::Await(inner) | Expr::Propagate(inner) | Expr::Unary { operand: inner, .. } => {
            performs_io(out, inner)
        }
        Expr::Binary { left, right, .. } => performs_io(out, left) || performs_io(out, right),
        _ => false,
    }
}

// ============================================================
// Types
// ============================================================

/// A type, as Lean spells it.
fn ty(out: &mut Out, t: &Type) -> String {
    match t {
        Type::Unit => "Unit".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::String => "String".to_string(),
        // `Array` and not `List`: the source indexes and grows these, and a Lean list
        // does neither in constant time.
        Type::List(inner) => format!("Array {}", atom(out, inner)),
        Type::Set(inner) => {
            out.lean_helpers.insert("Std");
            format!("Std.HashSet {}", atom(out, inner))
        }
        Type::Map(k, v) => {
            out.lean_helpers.insert("Std");
            format!("Std.HashMap {} {}", atom(out, k), atom(out, v))
        }
        Type::Optional(inner) => format!("Option {}", atom(out, inner)),
        Type::Tuple(parts) => format!(
            "({})",
            parts
                .iter()
                .map(|p| atom(out, p))
                .collect::<Vec<_>>()
                .join(" × ")
        ),
        Type::Fn { params, returns } => {
            let mut arrow: Vec<String> = params.iter().map(|p| atom(out, p)).collect();
            if arrow.is_empty() {
                arrow.push("Unit".to_string());
            }
            arrow.push(atom(out, returns));
            arrow.join(" → ")
        }
        Type::Named { name, args } => {
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
                .join(".");
            match args.is_empty() {
                true => clean,
                // Lean applies a type to its arguments by writing them beside it.
                false => format!(
                    "{clean} {}",
                    args.iter()
                        .map(|a| atom(out, a))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            }
        }
    }
}

/// Can Lean read this type as written? A binding whose settled type holds a name no
/// language spells is better left to inference than annotated with a word.
fn writable(t: &Type) -> bool {
    match t {
        Type::Named { name, args } => Type::is_writable_name(name) && args.iter().all(writable),
        Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => writable(inner),
        Type::Map(k, v) => writable(k) && writable(v),
        Type::Tuple(parts) => parts.iter().all(writable),
        Type::Fn { params, returns } => params.iter().all(writable) && writable(returns),
        _ => true,
    }
}

/// The same type, bracketed where it would otherwise take the next word as an argument.
fn atom(out: &mut Out, t: &Type) -> String {
    let text = ty(out, t);
    match text.contains(' ') && !text.starts_with('(') {
        true => format!("({text})"),
        false => text,
    }
}

// ============================================================
// Expressions
// ============================================================

/// An expression, bracketed where it would otherwise run into the word beside it. Lean
/// applies a function by juxtaposition, so an argument has to be one word or in brackets.
fn arg(out: &mut Out, e: &Expr) -> String {
    let text = expr(out, e);
    match atomic(&text) {
        true => text,
        false => format!("({text})"),
    }
}

/// Is this text one thing already, needing no brackets around it?
fn atomic(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if text.starts_with('(') && text.ends_with(')') && balanced_to_the_end(text) {
        return true;
    }
    if (text.starts_with("#[") || text.starts_with('{') || text.starts_with('⟨'))
        && !text.contains(' ')
    {
        return true;
    }
    // A name, a dotted path, a number, or a string with no space in it is one word.
    !text.contains(' ')
        && !text.contains('←')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || "_.?!\"'[]#".contains(c))
}

/// Do the brackets that open this text close only at its end?
fn balanced_to_the_end(text: &str) -> bool {
    let mut depth = 0usize;
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + c.len_utf8() == text.len();
                }
            }
            _ => {}
        }
    }
    false
}

fn expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(text) => {
            let cleaned = text.replace('_', "");
            match cleaned.starts_with('-') {
                // A negative literal in argument position needs its own brackets.
                true => format!("({cleaned})"),
                false => cleaned,
            }
        }
        Expr::Float(text) => {
            let cleaned = text.replace('_', "");
            let spelled = match cleaned.contains('.') || cleaned.contains('e') {
                true => cleaned,
                // Lean reads a bare digit as a `Nat` unless the literal spells the point.
                false => format!("{cleaned}.0"),
            };
            match spelled.starts_with('-') {
                true => format!("({spelled})"),
                false => spelled,
            }
        }
        Expr::Str(text) => quoted(Language::Lean, text),
        Expr::Bool(true) => "true".to_string(),
        Expr::Bool(false) => "false".to_string(),
        Expr::Null => "none".to_string(),
        Expr::Name(name) => out.value_name(name),
        Expr::Field { of, name } => {
            let holder = arg(out, of);
            format!("{holder}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            let holder = arg(out, of);
            let at = arg(out, index);
            match (holds_a_map(out, of), string_valued(out, of)) {
                (true, _) => format!("{holder}.get! {at}"),
                (_, true) => format!("({holder}.get! ⟨Int.toNat {at}⟩)"),
                _ => format!("{holder}[Int.toNat {at}]!"),
            }
        }
        Expr::Call { callee, args } | Expr::New { callee, args } => call(out, callee, args),
        Expr::Binary { op, left, right } => binary(out, *op, left, right),
        Expr::Unary { op, operand } => match op {
            UnaryOp::Not => format!("!{}", arg(out, operand)),
            UnaryOp::Neg => format!("-{}", arg(out, operand)),
            // `x!` on an `Option` is `get!`: the assertion the source made, said out loud.
            UnaryOp::Unwrap => format!("{}.get!", arg(out, operand)),
        },
        // Lean names the value a monadic action produces with an arrow, and a `do` block
        // is the only place one stands.
        Expr::Await(inner) | Expr::Propagate(inner) => {
            // The arrow belongs to the call that acts, and `call` writes it there. The
            // operand here is whatever the source wrapped, perhaps arithmetic over one.
            // A second arrow would ask for the value of a value.
            expr(out, inner)
        }
        Expr::Keyword { name, value } => {
            // Lean has no keyword argument; the name is the parameter's own and the
            // position carries it.
            out.note_once(
                "Lean passes arguments by position, so this drops the name a call site \
                 wrote and lets the order carry it.",
            );
            let _ = name;
            expr(out, value)
        }
        Expr::Cast { ty: target, value } => cast(out, target, value),
        Expr::InstanceOf { value, ty: target } => {
            // A Lean value has one type, known at elaboration, so the question a runtime
            // test asks does not arise.
            let subject = arg(out, value);
            let named = arg(out, target);
            out.note_once(
                "a runtime type test has no counterpart in Lean, where a value has one \
                 type and the elaborator already knows it.",
            );
            format!("(/- {MARKER}: {subject} is {named} -/ true)")
        }
        Expr::RecordLit { ty: name, fields } => {
            let declared = out.name(name);
            let assignments: Vec<String> = fields
                .iter()
                .map(|(field, value)| {
                    let rendered = expr(out, value);
                    format!("{} := {rendered}", out.field(field))
                })
                .collect();
            format!("({{ {} : {declared} }})", assignments.join(", "))
        }
        Expr::Coalesce { value, fallback } => {
            let subject = arg(out, value);
            let otherwise = arg(out, fallback);
            format!("({subject}.getD {otherwise})")
        }
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            let test = expr(out, condition);
            let yes = expr(out, then);
            let no = expr(out, otherwise);
            format!("(if {test} then {yes} else {no})")
        }
        Expr::Variant { sum, name, fields } => {
            let spelled = constructor(out, sum, name);
            match fields.is_empty() {
                true => spelled,
                false => {
                    let rendered: Vec<String> = fields.iter().map(|(_, v)| arg(out, v)).collect();
                    format!("({spelled} {})", rendered.join(" "))
                }
            }
        }
        Expr::Tuple(parts) => {
            let rendered: Vec<String> = parts.iter().map(|p| expr(out, p)).collect();
            format!("({})", rendered.join(", "))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| expr(out, i)).collect();
            format!("#[{}]", rendered.join(", "))
        }
        Expr::SetLit(items) => {
            out.lean_helpers.insert("Std");
            let rendered: Vec<String> = items.iter().map(|i| arg(out, i)).collect();
            match rendered.is_empty() {
                true => "Std.HashSet.emptyWithCapacity".to_string(),
                false => format!("(Std.HashSet.ofList [{}])", rendered.join(", ")),
            }
        }
        Expr::MapLit(entries) => {
            out.lean_helpers.insert("Std");
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    let key = expr(out, k);
                    let value = expr(out, v);
                    format!("({key}, {value})")
                })
                .collect();
            format!("(Std.HashMap.ofList [{}])", rendered.join(", "))
        }
        Expr::Template(parts) => template(out, parts),
        Expr::Lambda { params, body, .. } => {
            let bound: Vec<String> = params.iter().map(|p| out.name(&p.name)).collect();
            let rendered = expr(out, body);
            match bound.is_empty() {
                true => format!("(fun _ => {rendered})"),
                false => format!("(fun {} => {rendered})", bound.join(" ")),
            }
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let over = arg(out, iterable);
            let bound = out.name(binding);
            let kept = match condition {
                Some(c) => {
                    let test = expr(out, c);
                    format!("{over}.filter (fun {bound} => {test})")
                }
                None => over,
            };
            let mapped = expr(out, element);
            format!("(({kept}).map (fun {bound} => {mapped}))")
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            // Lean nests `/- -/`, so the only source that cannot ride inside one is
            // source that closes it.
            match u.source.contains("-/") || u.source.contains("/-") {
                true => format!("(/- {MARKER}: line {} -/ default)", u.line),
                false => format!("(/- {MARKER}: {} -/ default)", u.source),
            }
        }
    }
}

/// `(T) x`, `x as T`: Lean converts rather than reasserts, and each conversion is a
/// different function.
fn cast(out: &mut Out, target: &Expr, value: &Expr) -> String {
    let rendered = arg(out, value);
    let Expr::Name(name) = target else {
        let named = expr(out, target);
        out.note_once(
            "a cast to something other than a named type has no counterpart in Lean, \
             where a conversion is a function and not an assertion.",
        );
        return format!("(/- {MARKER}: as {named} -/ {rendered})");
    };
    match name.as_str() {
        "int" | "i64" | "i32" | "long" | "number" | "Int" => truncate(out, value),
        "float" | "f64" | "f32" | "double" | "Float" => format!("(Float.ofInt {rendered})"),
        "str" | "string" | "String" => {
            let text = shown(out, value);
            format!("({text})")
        }
        "bool" | "Bool" => rendered,
        other => {
            out.note_once(&format!(
                "`{other}` is not a conversion Lean has a function for, so the value \
                 crosses as it is."
            ));
            rendered
        }
    }
}

/// A value as a whole number, which in Lean means saying which way it rounds.
fn truncate(out: &mut Out, value: &Expr) -> String {
    let rendered = arg(out, value);
    match static_type(out, value) {
        // `Float.toInt` does not exist; the trip through `Int64` truncates toward zero,
        // which is what every source language's cast does.
        Some(Type::Float) => format!("({rendered}.toInt64.toInt)"),
        _ => rendered,
    }
}

/// A value in the type the place it lands in asks for.
///
/// Lean converts between whole and fractional numbers by naming the conversion. The
/// languages read here coerce silently. So a `number` that came from a length arrives
/// whole, and Lean refuses it where a fraction goes.
fn coerced(out: &mut Out, e: &Expr, want: Option<&Type>) -> String {
    let (Some(want), Some(have)) = (want, static_type(out, e)) else {
        return expr(out, e);
    };
    match (want, &have) {
        (Type::Float, Type::Int) => {
            let rendered = arg(out, e);
            format!("(Float.ofInt {rendered})")
        }
        (Type::Int, Type::Float) => {
            let rendered = arg(out, e);
            format!("({rendered}.toInt64.toInt)")
        }
        _ => expr(out, e),
    }
}

/// A variant's constructor, which lives in the namespace its own type opens.
fn constructor(out: &Out, sum: &str, variant: &str) -> String {
    format!("{}.{}", out.name(sum), out.legal(camel(variant)))
}

/// A call, and the arrow that names what it produces where it produces it in `IO`.
///
/// Lean's `←` may stand anywhere inside a `do` block. So a call that acts can be an
/// operand of one that computes, and neither leaves the expression.
fn call(out: &mut Out, callee: &Expr, args: &[Expr]) -> String {
    let text = call_text(out, callee, args);
    match acts_here(out, callee) {
        true => format!("(← {text})"),
        false => text,
    }
}

/// Does this callee name a function that answers in `IO`, in a place an arrow may stand?
fn acts_here(out: &Out, callee: &Expr) -> bool {
    out.lean_in_io && matches!(callee_parts(callee), (_, Some(name)) if out.lean_io.contains(name))
}

/// The call itself. The builtins come first, since the shared vocabulary spells them
/// Python's way and Lean spells almost none of them the same.
fn call_text(out: &mut Out, callee: &Expr, args: &[Expr]) -> String {
    if let Some(rendered) = builtin(out, callee, args) {
        return rendered;
    }
    let (receiver, name) = callee_parts(callee);
    // A record built by calling its own name is a construction. Where the source wrote
    // a constructor, the call meant that function.
    if let (None, Some(name)) = (receiver, name) {
        if let Some(fields) = out.records.get(name) {
            let built = match out.lean_constructed.contains(name) {
                true => out.legal(constructor_name(Language::Lean, name)),
                false if fields.len() == args.len() => "mk".to_string(),
                // A construction naming fewer values than the record has fields is one
                // no positional form can spell.
                false => {
                    out.note_once(&format!(
                        "`{name}` takes {} field(s) and this call passes {}; Lean builds \
                         a structure from all of them or from none.",
                        fields.len(),
                        args.len()
                    ));
                    "mk".to_string()
                }
            };
            let rendered: Vec<String> = args.iter().map(|a| arg(out, a)).collect();
            return match rendered.is_empty() {
                true => format!("{}.{built}", out.name(name)),
                false => format!("({}.{built} {})", out.name(name), rendered.join(" ")),
            };
        }
    }
    let head = match callee {
        Expr::Field { of, name } => {
            let holder = arg(out, of);
            format!("{holder}.{}", out.field(name))
        }
        other => arg(out, other),
    };
    if args.is_empty() {
        return head;
    }
    let rendered: Vec<String> = args.iter().map(|a| arg(out, a)).collect();
    format!("({head} {})", rendered.join(" "))
}

/// The shared vocabulary's own calls, as Lean spells them.
fn builtin(out: &mut Out, callee: &Expr, args: &[Expr]) -> Option<String> {
    let (receiver, name) = callee_parts(callee);
    let name = name?;
    if receiver.is_none() && shadows_builtin(out, name) {
        return None;
    }
    Some(match (receiver, name, args) {
        (None, "print", _) => {
            // One line, whatever the source printed, with the arguments spaced the way
            // every other target here spaces them.
            let text = match args {
                [Expr::Template(parts)] => template(out, parts),
                [only @ Expr::Str(_)] => expr(out, only),
                _ => {
                    let holes: Vec<String> = args
                        .iter()
                        .map(|a| {
                            let rendered = shown_in_hole(out, a);
                            format!("{{{rendered}}}")
                        })
                        .collect();
                    format!("s!\"{}\"", holes.join(" "))
                }
            };
            format!("IO.println {text}")
        }
        (None, "len", [x]) => {
            let subject = arg(out, x);
            let counted = match (
                holds_a_map(out, x) || holds_a_set(out, x),
                string_valued(out, x),
            ) {
                (true, _) => format!("{subject}.size"),
                (_, true) => format!("{subject}.length"),
                _ => format!("{subject}.size"),
            };
            format!("(Int.ofNat {counted})")
        }
        // Text already is its own text.
        (None, "str", [x @ (Expr::Str(_) | Expr::Template(_))]) => expr(out, x),
        (None, "str", [x]) => {
            let rendered = shown(out, x);
            format!("({rendered})")
        }
        (None, "int", [x]) => truncate(out, x),
        (None, "float", [x]) => {
            let subject = arg(out, x);
            format!("(Float.ofInt {subject})")
        }
        (None, "abs", [x]) => {
            let subject = arg(out, x);
            format!("({subject}.natAbs)")
        }
        // Truncation answers in the type it took: a fraction stays one, as `Math.trunc`
        // and Zig's `@trunc` both leave it.
        (None, "trunc", [x]) => match static_type(out, x) {
            Some(Type::Float) => {
                out.lean_helpers.insert("frTrunc");
                let subject = arg(out, x);
                format!("(frTrunc {subject})")
            }
            _ => arg(out, x),
        },
        (None, "slice", [of, from, to]) => {
            let subject = arg(out, of);
            let start = arg(out, from);
            let end = arg(out, to);
            format!("({subject}.extract (Int.toNat {start}) (Int.toNat {end}))")
        }
        (Some(of), "upper", []) => {
            let subject = arg(out, &of.clone());
            format!("{subject}.toUpper")
        }
        (Some(of), "lower", []) => {
            let subject = arg(out, &of.clone());
            format!("{subject}.toLower")
        }
        (Some(of), "strip", []) => {
            let subject = arg(out, &of.clone());
            format!("{subject}.trim")
        }
        (Some(of), "contains", [x]) => {
            let subject = arg(out, &of.clone());
            let value = arg(out, x);
            match string_valued(out, of) {
                true => format!("({subject}.splitOn {value}).length > 1"),
                false => format!("({subject}.contains {value})"),
            }
        }
        (Some(of), "join", [xs]) => {
            let separator = arg(out, &of.clone());
            let parts = arg(out, xs);
            format!("(String.intercalate {separator} {parts}.toList)")
        }
        (Some(of), "append", [x]) => {
            let subject = arg(out, &of.clone());
            let value = arg(out, x);
            format!("({subject}.push {value})")
        }
        (Some(of), "add", [x]) => {
            let subject = arg(out, &of.clone());
            let value = arg(out, x);
            format!("({subject}.insert {value})")
        }
        _ => return None,
    })
}

/// An interpolated string. Lean's `s!` takes an expression in each hole, so the parts
/// cross as they are.
fn template(out: &mut Out, parts: &[TemplatePart]) -> String {
    let mut text = String::from("s!\"");
    for part in parts {
        match part {
            TemplatePart::Text(literal) => {
                let quoted = quoted(Language::Lean, literal);
                let inner = quoted
                    .strip_prefix('"')
                    .and_then(|q| q.strip_suffix('"'))
                    .unwrap_or(&quoted);
                // A brace is interpolation syntax and escapes with a backslash.
                text.push_str(&inner.replace('{', "\\{").replace('}', "\\}"));
            }
            TemplatePart::Expr(e) => {
                let rendered = shown_in_hole(out, e);
                text.push_str(&format!("{{{rendered}}}"));
            }
        }
    }
    text.push('"');
    text
}

/// A binary operator. The three that decide an answer rather than spell one are division,
/// remainder and exclusive or.
fn binary(out: &mut Out, op: BinaryOp, left: &Expr, right: &Expr) -> String {
    let fractional = matches!(static_type(out, left), Some(Type::Float))
        || matches!(static_type(out, right), Some(Type::Float));
    match op {
        // Lean's `/` on `Int` rounds toward negative infinity and its `%` is the
        // Euclidean remainder. Neither is what a C-family `/` and `%` do, so each names
        // the rounding it wants.
        BinaryOp::Div if !fractional => {
            let l = arg(out, left);
            let r = arg(out, right);
            return format!("(Int.tdiv {l} {r})");
        }
        BinaryOp::Rem if !fractional => {
            let l = arg(out, left);
            let r = arg(out, right);
            return format!("(Int.tmod {l} {r})");
        }
        // Lean has no `%` over fractions at all, so a helper spells the remainder.
        BinaryOp::Rem => {
            out.lean_helpers.insert("frTrunc");
            out.lean_helpers.insert("frRem");
            let l = arg(out, left);
            let r = arg(out, right);
            return format!("(frRem {l} {r})");
        }
        BinaryOp::FloorDiv => {
            let l = arg(out, left);
            let r = arg(out, right);
            return format!("(Int.fdiv {l} {r})");
        }
        BinaryOp::FloorRem => {
            let l = arg(out, left);
            let r = arg(out, right);
            return format!("(Int.fmod {l} {r})");
        }
        // Python's `/` answers with a fraction whatever it divides.
        BinaryOp::TrueDiv => {
            let l = fractionally(out, left);
            let r = fractionally(out, right);
            return format!("({l} / {r})");
        }
        // Lean core has no exclusive or on `Int`; `Nat` has one, and the trip through it
        // is only defined for values that are not negative.
        BinaryOp::Xor => {
            let l = arg(out, left);
            let r = arg(out, right);
            if matches!(static_type(out, left), Some(Type::Bool)) {
                return format!("(xor {l} {r})");
            }
            out.note_once(
                "Lean core has no exclusive or on `Int`; this goes through `Nat`, which \
                 holds for values that are not negative.",
            );
            return format!("(Int.ofNat (Nat.xor (Int.toNat {l}) (Int.toNat {r})))");
        }
        _ => {}
    }
    let spelled = match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div | BinaryOp::TrueDiv => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::FloorDiv | BinaryOp::FloorRem | BinaryOp::Xor => {
            unreachable!("each of these answered above")
        }
    };
    let l = expr(out, left);
    let r = expr(out, right);
    format!(
        "{} {spelled} {}",
        binary_operand(l, left, op, false),
        binary_operand(r, right, op, true)
    )
}

/// An operand as a fraction, converted where it is a whole number.
fn fractionally(out: &mut Out, e: &Expr) -> String {
    let rendered = arg(out, e);
    match static_type(out, e) {
        Some(Type::Float) => rendered,
        _ => format!("(Float.ofInt {rendered})"),
    }
}

/// Does this expression hold text?
fn string_valued(out: &Out, e: &Expr) -> bool {
    matches!(static_type(out, e), Some(Type::String))
}

// ============================================================
// What the writer works out before writing
// ============================================================

/// Every function that reaches a runtime, and everything that calls one.
///
/// Lean separates what computes from what acts, and the separation is in the type. A
/// function that prints answers in `IO`, and so does every function that calls it.
fn io_functions(module: &Module) -> std::collections::BTreeSet<String> {
    let mut found: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for f in every_function(module) {
        // The entry point is the one function a runtime calls, so it is in `IO` whatever
        // its body does.
        if f.name == "main" || acts(&f.body) {
            found.insert(f.name.clone());
        }
    }
    // A caller of an action is an action, however far up the chain it sits.
    loop {
        let mut grew = false;
        for f in every_function(module) {
            if found.contains(&f.name) {
                continue;
            }
            if calls_any(&f.body, &found) {
                found.insert(f.name.clone());
                grew = true;
            }
        }
        if !grew {
            return found;
        }
    }
}

/// Does this body do something a pure definition cannot?
fn acts(body: &[Stmt]) -> bool {
    body.iter().any(|s| {
        let here = match s {
            Stmt::Try { .. } => true,
            Stmt::Expr(e) | Stmt::Return(Some(e)) => prints(e),
            Stmt::Let { value: Some(e), .. } => prints(e),
            Stmt::Assign { value, .. } => prints(value),
            _ => false,
        };
        here || sub_bodies(s).iter().any(|inner| acts(inner))
    })
}

/// Does this expression print?
fn prints(e: &Expr) -> bool {
    match e {
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            matches!(callee_parts(callee), (None, Some("print"))) || args.iter().any(prints)
        }
        Expr::Binary { left, right, .. } => prints(left) || prints(right),
        Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
            prints(operand)
        }
        _ => false,
    }
}

/// The functions that call themselves. Lean asks for a termination proof from every
/// recursive `def`, and `partial` is the answer that asks for none.
fn self_recursive(module: &Module) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    for f in every_function(module) {
        let mut itself = std::collections::BTreeSet::new();
        itself.insert(f.name.clone());
        if calls_any(&f.body, &itself) {
            found.insert(f.name.clone());
        }
    }
    found
}

/// Does this body call any of these?
fn calls_any(body: &[Stmt], names: &std::collections::BTreeSet<String>) -> bool {
    fn in_expr(e: &Expr, names: &std::collections::BTreeSet<String>) -> bool {
        match e {
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                matches!(callee_parts(callee), (_, Some(name)) if names.contains(name))
                    || in_expr(callee, names)
                    || args.iter().any(|a| in_expr(a, names))
            }
            Expr::Binary { left, right, .. } => in_expr(left, names) || in_expr(right, names),
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Propagate(operand) => {
                in_expr(operand, names)
            }
            Expr::Field { of, .. } => in_expr(of, names),
            Expr::Index { of, index } => in_expr(of, names) || in_expr(index, names),
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => in_expr(condition, names) || in_expr(then, names) || in_expr(otherwise, names),
            Expr::Coalesce { value, fallback } => in_expr(value, names) || in_expr(fallback, names),
            Expr::Tuple(parts) | Expr::ListLit(parts) | Expr::SetLit(parts) => {
                parts.iter().any(|p| in_expr(p, names))
            }
            Expr::MapLit(entries) => entries
                .iter()
                .any(|(k, v)| in_expr(k, names) || in_expr(v, names)),
            Expr::Template(parts) => parts.iter().any(|part| match part {
                TemplatePart::Expr(inner) => in_expr(inner, names),
                TemplatePart::Text(_) => false,
            }),
            Expr::Lambda { body, .. } => in_expr(body, names),
            Expr::RecordLit { fields, .. } => fields.iter().any(|(_, v)| in_expr(v, names)),
            Expr::Variant { fields, .. } => fields.iter().any(|(_, v)| in_expr(v, names)),
            Expr::Keyword { value, .. } => in_expr(value, names),
            Expr::Cast { ty, value } | Expr::InstanceOf { ty, value } => {
                in_expr(ty, names) || in_expr(value, names)
            }
            Expr::Comprehension {
                element,
                iterable,
                condition,
                ..
            } => {
                in_expr(element, names)
                    || in_expr(iterable, names)
                    || condition.as_ref().is_some_and(|c| in_expr(c, names))
            }
            _ => false,
        }
    }
    fn in_stmt(s: &Stmt, names: &std::collections::BTreeSet<String>) -> bool {
        let here = match s {
            Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Throw(e) => in_expr(e, names),
            Stmt::Let { value: Some(e), .. } => in_expr(e, names),
            Stmt::Assign { target, value } => in_expr(target, names) || in_expr(value, names),
            Stmt::TupleAssign { value, .. } => in_expr(value, names),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => in_expr(condition, names),
            Stmt::IfPresent { value, .. } | Stmt::WhilePresent { value, .. } => {
                in_expr(value, names)
            }
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                in_expr(iterable, names)
            }
            Stmt::Switch { subject, .. } | Stmt::MatchVariants { subject, .. } => {
                in_expr(subject, names)
            }
            Stmt::Assert { condition, message } => {
                in_expr(condition, names) || message.as_ref().is_some_and(|m| in_expr(m, names))
            }
            Stmt::LocalFunction(f) => calls_any(&f.body, names),
            _ => false,
        };
        here || sub_bodies(s).iter().any(|inner| calls_any(inner, names))
    }
    body.iter().any(|s| in_stmt(s, names))
}

/// Every function the module declares, loose or on a record.
fn every_function(module: &Module) -> impl Iterator<Item = &Function> {
    module.items.iter().flat_map(|item| match item {
        Item::Function(f) => vec![f],
        Item::Record(r) => r.methods.iter().collect(),
        _ => Vec::new(),
    })
}

/// Every name this body writes to.
fn mutated(body: &[Stmt]) -> std::collections::BTreeSet<String> {
    fn root(e: &Expr) -> Option<&str> {
        match e {
            Expr::Name(n) => Some(n),
            Expr::Field { of, .. } | Expr::Index { of, .. } => root(of),
            _ => None,
        }
    }
    fn walk(body: &[Stmt], found: &mut std::collections::BTreeSet<String>) {
        for s in body {
            match s {
                Stmt::Assign { target, .. } => {
                    if let Some(name) = root(target) {
                        found.insert(name.to_string());
                    }
                }
                // Lean answers a growing collection with a new value, so the statement
                // that grows it writes to the name.
                Stmt::Expr(Expr::Call { callee, .. }) => {
                    if let (Some(receiver), Some("append" | "add" | "remove")) =
                        callee_parts(callee)
                    {
                        if let Some(name) = root(receiver) {
                            found.insert(name.to_string());
                        }
                    }
                }
                _ => {}
            }
            for inner in sub_bodies(s) {
                walk(inner, found);
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    walk(body, &mut found);
    found
}

// ============================================================
// Order
// ============================================================

/// The module's items, grouped so that each group comes after everything it names, and a
/// cycle stays together in one group.
///
/// Lean is the only target here that needs this. It reads a file once, top to bottom,
/// and refuses a name it has not yet met.
fn in_declaration_order(module: &Module) -> Vec<Vec<usize>> {
    let declared: BTreeMap<String, usize> = module
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| declares(item).map(|name| (name, index)))
        .collect();
    let edges: Vec<std::collections::BTreeSet<usize>> = module
        .items
        .iter()
        .map(|item| {
            let mut named = std::collections::BTreeSet::new();
            names_in(item, &mut |name| {
                if let Some(index) = declared.get(name) {
                    named.insert(*index);
                }
            });
            named
        })
        .collect();
    strongly_connected(module.items.len(), &edges)
}

/// The name a top-level item declares, where it declares one.
fn declares(item: &Item) -> Option<String> {
    match item {
        Item::Function(f) => Some(f.name.clone()),
        Item::Record(r) => Some(r.name.clone()),
        Item::Sum(s) => Some(s.name.clone()),
        Item::Newtype(n) => Some(n.name.clone()),
        Item::Constant(c) => Some(c.name.clone()),
        _ => None,
    }
}

/// Every name this item mentions, whatever it mentions it as.
fn names_in(item: &Item, found: &mut impl FnMut(&str)) {
    fn in_type(t: &Type, found: &mut dyn FnMut(&str)) {
        match t {
            Type::Named { name, args } => {
                found(name);
                for arg in args {
                    in_type(arg, found);
                }
            }
            Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => in_type(inner, found),
            Type::Map(k, v) => {
                in_type(k, found);
                in_type(v, found);
            }
            Type::Tuple(parts) => {
                for part in parts {
                    in_type(part, found);
                }
            }
            Type::Fn { params, returns } => {
                for param in params {
                    in_type(param, found);
                }
                in_type(returns, found);
            }
            _ => {}
        }
    }
    fn in_expr(e: &Expr, found: &mut dyn FnMut(&str)) {
        match e {
            Expr::Name(name) => found(name),
            Expr::Variant { sum, .. } => found(sum),
            Expr::RecordLit { ty, fields } => {
                found(ty);
                for (_, value) in fields {
                    in_expr(value, found);
                }
            }
            Expr::Field { of, .. } => in_expr(of, found),
            Expr::Index { of, index } => {
                in_expr(of, found);
                in_expr(index, found);
            }
            Expr::Call { callee, args } | Expr::New { callee, args } => {
                in_expr(callee, found);
                for arg in args {
                    in_expr(arg, found);
                }
            }
            Expr::Binary { left, right, .. } => {
                in_expr(left, found);
                in_expr(right, found);
            }
            Expr::Unary { operand, .. }
            | Expr::Await(operand)
            | Expr::Propagate(operand)
            | Expr::Keyword { value: operand, .. } => in_expr(operand, found),
            Expr::Cast { ty, value } | Expr::InstanceOf { ty, value } => {
                in_expr(ty, found);
                in_expr(value, found);
            }
            Expr::Coalesce { value, fallback } => {
                in_expr(value, found);
                in_expr(fallback, found);
            }
            Expr::Ternary {
                condition,
                then,
                otherwise,
            } => {
                in_expr(condition, found);
                in_expr(then, found);
                in_expr(otherwise, found);
            }
            Expr::Tuple(parts) | Expr::ListLit(parts) | Expr::SetLit(parts) => {
                for part in parts {
                    in_expr(part, found);
                }
            }
            Expr::MapLit(entries) => {
                for (k, v) in entries {
                    in_expr(k, found);
                    in_expr(v, found);
                }
            }
            Expr::Template(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(inner) = part {
                        in_expr(inner, found);
                    }
                }
            }
            Expr::Lambda { body, .. } => in_expr(body, found),
            Expr::Comprehension {
                element,
                iterable,
                condition,
                ..
            } => {
                in_expr(element, found);
                in_expr(iterable, found);
                if let Some(c) = condition {
                    in_expr(c, found);
                }
            }
            _ => {}
        }
    }
    fn in_stmt(s: &Stmt, found: &mut dyn FnMut(&str)) {
        match s {
            Stmt::Return(Some(e)) | Stmt::Expr(e) | Stmt::Throw(e) => in_expr(e, found),
            Stmt::Let { ty, value, .. } => {
                if let Some(t) = ty {
                    in_type(t, found);
                }
                if let Some(v) = value {
                    in_expr(v, found);
                }
            }
            Stmt::Assign { target, value } => {
                in_expr(target, found);
                in_expr(value, found);
            }
            Stmt::TupleAssign { value, .. } => in_expr(value, found),
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => in_expr(condition, found),
            Stmt::IfPresent { value, .. } | Stmt::WhilePresent { value, .. } => {
                in_expr(value, found)
            }
            Stmt::ForEach { iterable, .. } | Stmt::ForEachIndexed { iterable, .. } => {
                in_expr(iterable, found)
            }
            Stmt::CountedFor {
                condition: Some(c), ..
            } => in_expr(c, found),
            Stmt::Switch { subject, arms, .. } => {
                in_expr(subject, found);
                for (literals, _) in arms {
                    for literal in literals {
                        in_expr(literal, found);
                    }
                }
            }
            Stmt::MatchVariants { subject, sum, .. } => {
                in_expr(subject, found);
                found(sum);
            }
            Stmt::Assert { condition, message } => {
                in_expr(condition, found);
                if let Some(m) = message {
                    in_expr(m, found);
                }
            }
            Stmt::LocalFunction(f) => in_function(f, found),
            _ => {}
        }
        for inner in sub_bodies(s) {
            for s in inner {
                in_stmt(s, found);
            }
        }
    }
    fn in_function(f: &Function, found: &mut dyn FnMut(&str)) {
        if let Some(receiver) = &f.receiver {
            found(receiver);
        }
        for p in &f.params {
            if let Some(t) = &p.ty {
                in_type(t, found);
            }
            if let Some(d) = &p.default {
                in_expr(d, found);
            }
        }
        if let Some(t) = &f.returns {
            in_type(t, found);
        }
        // A name this body binds is this body's, whatever else the module calls the
        // same word. `fn celsius(fahrenheit: f64)` reads `fahrenheit` and means its
        // own parameter, and reading it as the function next to it invents a cycle.
        let mut bound: std::collections::BTreeSet<String> =
            f.params.iter().map(|p| p.name.clone()).collect();
        bound_names(&f.body, &mut bound);
        let mut outer = |name: &str| {
            if !bound.contains(name) {
                found(name);
            }
        };
        for s in &f.body {
            in_stmt(s, &mut outer);
        }
    }
    match item {
        Item::Function(f) => in_function(f, found),
        Item::Record(r) => {
            if let Some(base) = &r.extends {
                found(base);
            }
            for f in &r.fields {
                if let Some(t) = &f.ty {
                    in_type(t, found);
                }
                if let Some(d) = &f.default {
                    in_expr(d, found);
                }
            }
            for m in &r.methods {
                in_function(m, found);
            }
        }
        Item::Sum(s) => {
            for v in &s.variants {
                for f in &v.fields {
                    if let Some(t) = &f.ty {
                        in_type(t, found);
                    }
                }
            }
        }
        Item::Newtype(n) => in_type(&n.base, found),
        Item::Constant(c) => {
            if let Some(t) = &c.ty {
                in_type(t, found);
            }
            in_expr(&c.value, found);
        }
        Item::Test { body, .. } => {
            for s in body {
                in_stmt(s, found);
            }
        }
        Item::Statement(s) => in_stmt(s, found),
        Item::Import { .. } | Item::Unsupported(_) => {}
    }
}

/// Tarjan's algorithm: each group holds the nodes that reach one another, and a group
/// comes out only after every group it points at.
fn strongly_connected(
    count: usize,
    edges: &[std::collections::BTreeSet<usize>],
) -> Vec<Vec<usize>> {
    struct Walk<'e> {
        edges: &'e [std::collections::BTreeSet<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        groups: Vec<Vec<usize>>,
    }
    impl Walk<'_> {
        fn visit(&mut self, node: usize) {
            self.index[node] = Some(self.next);
            self.low[node] = self.next;
            self.next += 1;
            self.stack.push(node);
            self.on_stack[node] = true;
            for &to in &self.edges[node] {
                match self.index[to] {
                    None => {
                        self.visit(to);
                        self.low[node] = self.low[node].min(self.low[to]);
                    }
                    Some(at) if self.on_stack[to] => self.low[node] = self.low[node].min(at),
                    Some(_) => {}
                }
            }
            if self.low[node] != self.index[node].unwrap_or_default() {
                return;
            }
            let mut group = Vec::new();
            while let Some(popped) = self.stack.pop() {
                self.on_stack[popped] = false;
                group.push(popped);
                if popped == node {
                    break;
                }
            }
            // Inside a group the source's own order is the only order there is.
            group.sort_unstable();
            self.groups.push(group);
        }
    }
    let mut walk = Walk {
        edges,
        index: vec![None; count],
        low: vec![0; count],
        on_stack: vec![false; count],
        stack: Vec::new(),
        next: 0,
        groups: Vec::new(),
    };
    for node in 0..count {
        if walk.index[node].is_none() {
            walk.visit(node);
        }
    }
    walk.groups
}
