//! Writing the IR out as a language.
//!
//! Each writer is idiomatic for its target rather than a transliteration: a record
//! becomes a Rust `struct` with an `impl` block, a Python `@dataclass`, a Go `struct`
//! with methods beside it, a TypeScript `class`. Naming follows the target's
//! convention — `snake_case` in Rust and Python, `camelCase` in TypeScript,
//! `PascalCase` for exported Go — because a file that reads as a foreign language
//! wearing a costume is not a file anyone will keep.
//!
//! **The signature is the contract.** Parameter names, their order, their types and
//! the return type are carried exactly; only the spelling changes. Where a type has no
//! counterpart it is written through by name and counted in the fidelity report,
//! because silently substituting a type is how a signature stops meaning what it said.
//!
//! Anything the reader could not translate is emitted as a comment holding the
//! original source, under a marker. The result is a file you finish, not one you have
//! to diff to discover what is missing.

use super::ir::*;
use crate::lang::Language;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

/// What kind of thing a name names, since the conventions differ by kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A struct, class, interface — `PascalCase` in all four.
    Type,
    /// A module-level constant — `SCREAMING_SNAKE` in three of the four; Go spells it
    /// like anything else, because there the capital letter means exported.
    Constant,
    /// A function, method, field, parameter or local.
    Value,
}

/// How every name this module declares is spelled in the target language.
///
/// Only its own. A name absent from this map is foreign — a library, a builtin,
/// somebody else's field — and is written exactly as the source had it. That is the
/// whole of the safety argument: the tool renames what the file declares and nothing
/// else, which is the same rule its refactorings follow.
type Spellings = (BTreeMap<String, String>, BTreeMap<String, String>);

fn spellings(language: Language, module: &Module) -> Spellings {
    fn spell(language: Language, name: &str, kind: Kind, exported: bool) -> String {
        match kind {
            Kind::Type => pascal(name),
            Kind::Constant => match language {
                Language::Go => go_name(name, exported),
                _ => screaming(name),
            },
            Kind::Value => match language {
                Language::Rust | Language::Python => snake_always(name),
                Language::Go => go_name(name, exported),
                _ => camel(name),
            },
        }
    }

    let mut map = BTreeMap::new();
    let mut fields = BTreeMap::new();
    let into = |map: &mut BTreeMap<String, String>, name: &str, kind: Kind, exported: bool| {
        if name.is_empty() {
            return;
        }
        let spelled = spell(language, name, kind, exported);
        // Only where it differs, so lookups stay meaningful and the map stays small.
        if spelled != name {
            map.insert(name.to_string(), spelled);
        }
    };
    let mut add = |name: &str, kind: Kind, exported: bool| into(&mut map, name, kind, exported);

    fn walk_stmts(stmts: &[Stmt], add: &mut impl FnMut(&str, Kind, bool)) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, .. } => add(name, Kind::Value, false),
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
                Stmt::While { body, .. } => walk_stmts(body, add),
                _ => {}
            }
        }
    }

    fn walk_function(f: &Function, add: &mut impl FnMut(&str, Kind, bool)) {
        add(&f.name, Kind::Value, f.exported);
        for param in &f.params {
            add(&param.name, Kind::Value, false);
        }
        walk_stmts(&f.body, add);
    }

    for item in &module.items {
        match item {
            Item::Function(f) => walk_function(f, &mut add),
            Item::Record(r) => {
                add(&r.name, Kind::Type, r.exported);
                for field in &r.fields {
                    into(&mut fields, &field.name, Kind::Value, field.exported);
                }
                for method in &r.methods {
                    walk_function(method, &mut add);
                }
            }
            // A `const` bound to a literal is a constant and takes the
            // `SCREAMING_SNAKE` convention. One bound to a call is a binding that
            // happens to be immutable — `const schema = z.object({...})` — and
            // shouting its name would be wrong in Python and unstable across a
            // round trip.
            Item::Constant(c) => {
                let kind = match c.value {
                    Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {
                        Kind::Constant
                    }
                    _ => Kind::Value,
                };
                add(&c.name, kind, c.exported)
            }
            Item::Import { .. } | Item::Unsupported(_) => {}
        }
    }
    (map, fields)
}

/// How this parameter is written here, and whether the calling convention survived.
///
/// A `*` marker is not a parameter and is never written. `*args` is exact wherever the
/// target has a variadic and a change of convention where it does not; `**kwargs` is a
/// change of convention everywhere but Python. `changed` is reported rather than
/// hidden, because a caller of the translated function writes the call differently and
/// nothing else in the output would say so.
fn spell_param(
    language: Language,
    kind: ParamKind,
    name: &str,
    changed: &mut bool,
) -> Option<String> {
    match (kind, language) {
        (ParamKind::Normal, _) => Some(name.to_string()),
        (ParamKind::Marker, Language::Python) => Some(name.to_string()),
        (ParamKind::Marker, _) => {
            *changed = true;
            None
        }
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

/// Does the target reserve this word, so that an identifier cannot be spelled it?
///
/// Only true keywords: a name that is merely a builtin (Go's `delete`, Python's `id`)
/// is legal to shadow and renaming it would be churn. The source language's keywords
/// are irrelevant — what matters is whether *this* file will parse.
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
    let list = match language {
        Language::Rust => RUST,
        Language::Go => GO,
        Language::Python => PYTHON,
        Language::TypeScript | Language::Tsx => TYPESCRIPT,
        _ => return false,
    };
    list.contains(&name)
}

/// The marker every carried-over fragment is written under.
pub const MARKER: &str = "fun-refactor: not translated";

pub fn write(language: Language, module: &Module) -> Result<(String, Fidelity)> {
    write_in_context(language, module, module)
}

/// Write `module`, spelling names as declared by `context`.
///
/// The two are the same module except where a caller writes a *piece* of a file on its
/// own — the Next.js translation writes each handler body as its own module so it can
/// indent it into a decorated `def`. Spelling from the piece alone means a call to a
/// helper declared elsewhere in the same file keeps its original casing, so the
/// declaration says `verify_current_user_has_access_to_post` and the call still says
/// `verifyCurrentUserHasAccessToPost`.
pub fn write_in_context(
    language: Language,
    module: &Module,
    context: &Module,
) -> Result<(String, Fidelity)> {
    let mut out = Out::new(language);
    let (names, fields) = spellings(language, context);
    out.names = names;
    out.fields = fields;
    out.declared_types = context
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Record(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();
    match language {
        Language::Rust => rust(&mut out, module),
        Language::Python => python(&mut out, module),
        Language::Go => go(&mut out, module),
        Language::TypeScript | Language::Tsx => typescript(&mut out, module),
        other => bail!(
            "there is no writer for {other}: it has no functions or records to write \
             these into"
        ),
    }
    let escaped: Vec<String> = out.escaped.borrow().iter().cloned().collect();
    for name in escaped {
        out.fidelity.notes.push(format!(
            "`{name}` is a keyword in {language} and cannot be an identifier there; it \
             is written with a suffix, and every use of it needs a real name"
        ));
    }
    Ok((out.finish(), out.fidelity))
}

struct Out {
    language: Language,
    text: String,
    indent: usize,
    fidelity: Fidelity,
    /// How this module's own names are spelled in the target language.
    ///
    /// Every language here has a convention and they disagree: TypeScript writes
    /// `userName`, Python writes `user_name`, Go says "exported" with a capital
    /// letter. Adopting the target's convention is most of what makes a translated
    /// file look written rather than converted.
    ///
    /// It is one map, built once from the declarations and consulted at every
    /// declaration *and* every use, because the alternative — re-casing at each site
    /// with whichever helper was to hand — is how `interface User { userName }` became
    /// `class User: user_name` whose bodies still said `.userName`.
    ///
    /// A name it does not contain is **foreign** and is left exactly as written:
    /// `db.users.find`, `NextResponse`, a library function. Re-casing those would
    /// rename somebody else's API, which is the one thing a translation must not do.
    names: BTreeMap<String, String>,
    /// Names that had to be escaped because the target reserves them.
    ///
    /// Collected while writing and reported at the end. `select` is a name sqlmodel
    /// exports and a keyword in Go, and `select(User)` is not something Go's grammar
    /// will accept — so the file was refused outright, which gives the reader nothing.
    /// Escaping it and *saying so* gives them a draft and the one line to fix.
    escaped: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// The types this module declares.
    ///
    /// A signature mentioning one of them is complete, not "mentioning a type this
    /// tool does not know" — the record is right there in the same output. Reporting
    /// the file's own records as foreign made a perfect translation confess to a
    /// problem it did not have, which is how a fidelity report stops being read.
    declared_types: std::collections::BTreeSet<String>,
    /// The same, for record fields, which are a separate namespace.
    ///
    /// Not folded into `names`: a Rust `Reading { sensor }` with an exported field
    /// becomes Go's `Sensor`, while a *parameter* also called `sensor` stays
    /// lowercase — and one map keyed by name alone gave the parameter the field's
    /// spelling. A field is reached through a receiver and a binding is not, so they
    /// do not share a namespace in any of these languages either.
    fields: BTreeMap<String, String>,
}

impl Out {
    fn new(language: Language) -> Self {
        Out {
            language,
            text: String::new(),
            indent: 0,
            fidelity: Fidelity::default(),
            names: BTreeMap::new(),
            escaped: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            declared_types: std::collections::BTreeSet::new(),
            fields: BTreeMap::new(),
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

    /// The same name, made writable where the target reserves it.
    fn legal(&self, spelled: String) -> String {
        if !reserved(self.language, &spelled) {
            return spelled;
        }
        self.escaped.borrow_mut().insert(spelled.clone());
        match self.language {
            // Rust has a spelling for exactly this and it stays the same identifier.
            Language::Rust => format!("r#{spelled}"),
            _ => format!("{spelled}_"),
        }
    }

    /// This field name in the target's convention, or unchanged if it is not ours.
    ///
    /// A use site is `x.name` and nothing here knows the type of `x`, so this renames
    /// a field of a foreign object that happens to share a name with one this module
    /// declares. The alternative — never renaming a field at a use site — leaves the
    /// declaration and its uses spelled differently, which is worse and also wrong.
    /// Is this a type with no counterpart here, written through by name?
    fn is_foreign(&self, ty: &Type) -> bool {
        match ty {
            Type::Named { name, .. } => !self.declared_types.contains(name),
            Type::List(inner) | Type::Optional(inner) => self.is_foreign(inner),
            Type::Map(k, v) => self.is_foreign(k) || self.is_foreign(v),
            _ => false,
        }
    }

    fn field(&self, raw: &str) -> String {
        let spelled = self
            .fields
            .get(raw)
            .cloned()
            .unwrap_or_else(|| raw.to_string());
        self.legal(spelled)
    }

    fn line(&mut self, text: &str) {
        if !text.is_empty() {
            for _ in 0..self.indent {
                self.text.push_str("    ");
            }
            self.text.push_str(text);
        }
        self.text.push('\n');
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

    fn comment(&self, text: &str) -> String {
        match self.language {
            Language::Python => format!("# {text}"),
            _ => format!("// {text}"),
        }
    }
}

/// Write a carried-over fragment as a comment, whole, so nothing is lost.
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

// ------------------------------------------------------------------ conventions

/// `snake_case`, for Rust and Python.
///
/// A name that already starts with a capital is a type, a class or an imported
/// binding in every one of these languages, and is left alone: `NextResponse.json(x)`
pub(super) fn snake_always(name: &str) -> String {
    // A separator goes before an uppercase letter only where a word actually starts:
    // after a lowercase or a digit, or at the end of a run of capitals that is
    // followed by a lowercase one. Splitting before *every* capital turns
    // `HTTPServer` into `h_t_t_p_server` and `MAX_RETRY` into `m_a_x__r_e_t_r_y`,
    // and real code is full of acronyms.
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
fn camel(name: &str) -> String {
    // SCREAMING_SNAKE has to be lowered before it is re-cased, or `MIN_CELSIUS`
    // becomes `MINCELSIUS` rather than `minCelsius`.
    let screaming = name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric());
    let source = if screaming {
        name.to_lowercase()
    } else {
        name.to_string()
    };

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
fn pascal(name: &str) -> String {
    let camel = camel(name);
    let mut chars = camel.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => camel,
    }
}

// ------------------------------------------------------------------------- Rust

fn rust(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/// {line}"));
                }
                let ty =
                    c.ty.as_ref()
                        .map(rust_type)
                        .unwrap_or_else(|| "&str".to_string());
                let visibility = if c.exported { "pub " } else { "" };
                let value = rust_expr(out, &c.value);
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
                out.line(&format!("{visibility}struct {type_name} {{"));
                out.open();
                for f in &r.fields {
                    for line in &f.doc {
                        out.line(&format!("/// {line}"));
                    }
                    let ty =
                        f.ty.as_ref()
                            .map(rust_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_visibility = if f.exported { "pub " } else { "" };
                    let field_name = out.field(&f.name);
                    out.line(&format!("{field_visibility}{field_name}: {ty},"));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();

                if !r.methods.is_empty() {
                    // Rust declares methods apart from the type, which is what the
                    // record's method list becomes.
                    let type_name = out.name(&r.name);
                    out.line(&format!("impl {type_name} {{"));
                    out.open();
                    for m in &r.methods {
                        rust_function(out, m, true);
                    }
                    out.close();
                    out.line("}");
                    out.blank();
                }
            }
            Item::Function(f) => {
                rust_function(out, f, false);
                out.blank();
            }
            Item::Import { text, line } => {
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
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn rust_function(out: &mut Out, f: &Function, method: bool) {
    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        out.line(&out.comment("declared async in the source"));
    }
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push("&self".to_string());
    }
    let mut foreign = false;
    for p in &f.params {
        let Some(spelled) = spell_param(out.language, p.kind, &out.name(&p.name), &mut changed)
        else {
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
                foreign = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("{spelled}: {ty}"));
    }
    let returns = match &f.returns {
        Some(Type::Unit) | None => String::new(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            format!(" -> {}", rust_type(t))
        }
    };
    let visibility = if f.exported { "pub " } else { "" };
    out.line(&format!(
        "{visibility}fn {}({}){returns} {{",
        out.name(&f.name),
        params.join(", ")
    ));
    out.open();
    rust_block(out, &f.body);
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
}

fn rust_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line("todo!()");
        return;
    }
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_default();
                out.line(&format!("return {text};"));
            }
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", rust_type(t)))
                    .unwrap_or_default();
                let m = if *mutable { "mut " } else { "" };
                let v = value
                    .as_ref()
                    .map(|v| rust_expr(out, v))
                    .unwrap_or_else(|| "Default::default()".to_string());
                let bound = out.name(name);
                out.line(&format!("let {m}{bound}{annotation} = {v};"));
            }
            Stmt::Assign { target, value } => {
                let t = rust_expr(out, target);
                let v = rust_expr(out, value);
                out.line(&format!("{t} = {v};"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = rust_expr(out, condition);
                out.line(&format!("if {c} {{"));
                out.open();
                rust_block(out, then);
                out.close();
                if otherwise.is_empty() {
                    out.line("}");
                } else {
                    out.line("} else {");
                    out.open();
                    rust_block(out, otherwise);
                    out.close();
                    out.line("}");
                }
            }
            Stmt::While { condition, body } => {
                let c = rust_expr(out, condition);
                out.line(&format!("while {c} {{"));
                out.open();
                rust_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::ForEach {
                binding,
                iterable,
                body,
            } => {
                let it = rust_expr(out, iterable);
                let bound = out.name(binding);
                out.line(&format!("for {bound} in {it} {{"));
                out.open();
                rust_block(out, body);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = rust_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Break => out.line("break;"),
            Stmt::Continue => out.line("continue;"),
            // Rust models failure in the return type; there is no catch block to
            // translate a catch block into, so it is carried whole.
            Stmt::Try { source, line, .. } => carry(
                out,
                &Unsupported {
                    construct: "try".into(),
                    source: source.clone(),
                    line: *line,
                },
            ),
            Stmt::Throw(value) => {
                let rendered = rust_expr(out, value);
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

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "i64".to_string(),
        Type::Float => "f64".to_string(),
        Type::String => "String".to_string(),
        Type::List(inner) => format!("Vec<{}>", rust_type(inner)),
        Type::Map(k, v) => format!(
            "std::collections::HashMap<{}, {}>",
            rust_type(k),
            rust_type(v)
        ),
        Type::Optional(inner) => format!("Option<{}>", rust_type(inner)),
        Type::Named { name, args } => generic(name, args, "<", ">", "::", rust_type),
    }
}

fn rust_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => out.name(n),
        Expr::Field { of, name } => {
            let object = rust_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            format!("{}[{}]", rust_expr(out, of), rust_expr(out, index))
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            format!("{}({})", rust_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            rust_expr(out, left),
            op.c_like(),
            rust_expr(out, right)
        ),
        // Rust puts it after; the other two put it in front.
        Expr::Await(inner) => format!("{}.await", rust_expr(out, inner)),
        // Rust has no universal spelling for construction: `X::new`, `X { .. }` and
        // a builder are all idiomatic and which one applies is a fact about the type.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            let target = rust_expr(out, callee);
            let source = format!("new {target}({})", rendered.join(", "));
            out.carried(&Unsupported {
                construct: "new".into(),
                source: source.clone(),
                line: 0,
            });
            format!("todo!(/* {MARKER}: {} */)", source.replace("*/", "* /"))
        }
        // Rust asks this with a `match` on an enum or with `Any::downcast`, and
        // which one applies is a fact about the type rather than about the code.
        Expr::InstanceOf { value, ty } => {
            let rendered = rust_expr(out, value);
            let named = rust_expr(out, ty);
            let source = format!("{rendered} instanceof {named}");
            out.carried(&Unsupported {
                construct: "instanceof".into(),
                source: source.clone(),
                line: 0,
            });
            format!("todo!(/* {MARKER}: {} */)", source.replace("*/", "* /"))
        }
        Expr::Keyword { name, value } => {
            let rendered = rust_expr(out, value);
            let source = format!("{name}={rendered}");
            out.carried(&Unsupported {
                construct: "keyword argument".into(),
                source: source.clone(),
                line: 0,
            });
            format!("todo!(/* {MARKER}: {} */)", source.replace("*/", "* /"))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", rust_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| rust_expr(out, i)).collect();
            format!("vec![{}]", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("({}, {})", rust_expr(out, k), rust_expr(out, v)))
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
                format!("{literal:?}.to_string()")
            } else {
                format!("format!({literal:?}, {})", args.join(", "))
            }
        }
        Expr::Comprehension {
            element,
            binding,
            iterable,
            condition,
        } => {
            let it = rust_expr(out, iterable);
            let name = out.name(binding);
            let filter = condition
                .as_ref()
                .map(|c| format!(".filter(|{name}| {})", rust_expr(out, c)))
                .unwrap_or_default();
            format!(
                "{it}.into_iter(){filter}.map(|{name}| {}).collect()",
                rust_expr(out, element)
            )
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("todo!(\"{}: {}\")", MARKER, u.source.replace('"', "'"))
        }
    }
}

// ----------------------------------------------------------------------- Python

fn python(out: &mut Out, module: &Module) {
    if !module.doc.is_empty() {
        out.line("\"\"\"");
        for line in &module.doc {
            out.line(line);
        }
        out.line("\"\"\"");
        out.blank();
    }

    let needs_dataclass = module
        .items
        .iter()
        .any(|i| matches!(i, Item::Record(r) if !r.fields.is_empty()));
    if needs_dataclass {
        out.line("from dataclasses import dataclass");
        out.blank();
    }

    for item in &module.items {
        match item {
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
                out.line(&format!("class {type_name}:"));
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
                    out.line(&format!("{field_name}: {annotation}"));
                }
                if r.fields.is_empty() && r.methods.is_empty() && r.doc.is_empty() {
                    out.line("pass");
                }
                for m in &r.methods {
                    out.blank();
                    python_function(out, m, true);
                }
                out.close();
                out.fidelity.records += 1;
                out.blank();
            }
            Item::Function(f) => {
                python_function(out, f, false);
                out.blank();
            }
            Item::Import { text, line } => {
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
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn python_function(out: &mut Out, f: &Function, method: bool) {
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push("self".to_string());
    }
    let mut foreign = false;
    for p in &f.params {
        let annotation = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                format!(": {}", python_type(t))
            }
            None => {
                foreign = true;
                String::new()
            }
        };
        let default = p
            .default
            .as_ref()
            .map(|d| format!(" = {}", python_expr(out, d)))
            .unwrap_or_default();
        let Some(spelled) = spell_param(out.language, p.kind, &out.name(&p.name), &mut changed)
        else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(spelled);
            continue;
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
        out.name(&f.name),
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
    python_block(out, &f.body);
    out.close();

    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
}

fn python_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line("raise NotImplementedError");
        return;
    }
    let mut wrote = false;
    for stmt in body {
        // Whether a statement was produced is a property of the statement, asked once
        // here rather than set inside each arm. An arm that forgot left a stray
        // `raise NotImplementedError` after a perfectly good body — which is how the
        // `try` arm arrived broken, and how the next one would have.
        wrote |= !matches!(stmt, Stmt::Unsupported(_) | Stmt::Expr(Expr::Null));
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", python_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text}"));
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
                out.line(&format!("{bound}{annotation} = {v}"));
            }
            Stmt::Assign { target, value } => {
                let t = python_expr(out, target);
                let v = python_expr(out, value);
                out.line(&format!("{t} = {v}"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = python_expr(out, condition);
                out.line(&format!("if {c}:"));
                out.open();
                python_block(out, then);
                out.close();
                if !otherwise.is_empty() {
                    // `else: if ...` is written `elif` when that is all it is.
                    if otherwise.len() == 1 {
                        if let Stmt::If { .. } = &otherwise[0] {
                            out.line("else:");
                            out.open();
                            python_block(out, otherwise);
                            out.close();
                            continue;
                        }
                    }
                    out.line("else:");
                    out.open();
                    python_block(out, otherwise);
                    out.close();
                }
            }
            Stmt::While { condition, body } => {
                let c = python_expr(out, condition);
                out.line(&format!("while {c}:"));
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
                out.line(&format!("for {bound} in {it}:"));
                out.open();
                python_block(out, body);
                out.close();
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = python_expr(out, e);
                out.line(&text);
            }
            Stmt::Break => {
                out.line("break");
            }
            Stmt::Continue => {
                out.line("continue");
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                out.line("try:");
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
                    out.line(&format!("except {selector}{bound}:"));
                    out.open();
                    python_block(out, &clause.body);
                    out.close();
                }
                if !finally.is_empty() {
                    out.line("finally:");
                    out.open();
                    python_block(out, finally);
                    out.close();
                }
            }
            Stmt::Throw(value) => {
                let rendered = python_expr(out, value);
                out.line(&format!("raise {rendered}"));
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                out.line(&line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
    // A body that is only carried-over comments still needs a statement to be Python.
    if !wrote {
        out.line("raise NotImplementedError");
    }
}

fn python_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "None".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float".to_string(),
        Type::String => "str".to_string(),
        Type::List(inner) => format!("list[{}]", python_type(inner)),
        Type::Map(k, v) => format!("dict[{}, {}]", python_type(k), python_type(v)),
        Type::Optional(inner) => format!("{} | None", python_type(inner)),
        // Python spells generics with brackets, which is why the arguments are kept
        // apart: `Result<(), String>` written literally is not a Python annotation.
        Type::Named { name, args } => generic(name, args, "[", "]", ".", python_type),
    }
}

fn python_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => if *v { "True" } else { "False" }.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "None".to_string(),
        Expr::Name(n) => out.name(n),
        Expr::Field { of, name } => {
            let object = python_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            format!("{}[{}]", python_expr(out, of), python_expr(out, index))
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| python_expr(out, a)).collect();
            format!("{}({})", python_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => {
            // Python compares against None with `is`, not `==`. Both work; only one
            // is what a Python programmer writes, and idiom is the point here.
            let against_none = matches!(**right, Expr::Null) || matches!(**left, Expr::Null);
            let spelling = match (op, against_none) {
                (BinaryOp::Eq, true) => "is",
                (BinaryOp::Ne, true) => "is not",
                (other, _) => other.python(),
            };
            format!(
                "{} {spelling} {}",
                python_expr(out, left),
                python_expr(out, right)
            )
        }
        Expr::Await(inner) => format!("await {}", python_expr(out, inner)),
        // Construction in Python is a call.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| python_expr(out, a)).collect();
            format!("{}({})", python_expr(out, callee), rendered.join(", "))
        }
        Expr::InstanceOf { value, ty } => {
            let rendered = python_expr(out, value);
            format!("isinstance({rendered}, {})", python_expr(out, ty))
        }
        Expr::Keyword { name, value } => {
            let rendered = python_expr(out, value);
            format!("{name}={rendered}")
        }
        Expr::Unary { op, operand } => match op {
            UnaryOp::Not => format!("not {}", python_expr(out, operand)),
            UnaryOp::Neg => format!("-{}", python_expr(out, operand)),
        },
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| python_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", python_expr(out, k), python_expr(out, v)))
                .collect();
            format!("{{{}}}", rendered.join(", "))
        }
        Expr::Template(parts) => {
            let mut body = String::new();
            for part in parts {
                match part {
                    TemplatePart::Text(text) => {
                        body.push_str(&text.replace('{', "{{").replace('}', "}}"))
                    }
                    TemplatePart::Expr(e) => {
                        body.push('{');
                        body.push_str(&python_expr(out, e));
                        body.push('}');
                    }
                }
            }
            format!("f{body:?}")
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
        Expr::Unsupported(u) => {
            out.carried(u);
            // No inline comment: in Python a `#` runs to end of line, so a note inside
            // a call's parentheses swallows the closing paren and the file will not
            // parse. The note lives in the fidelity report, where it can be read.
            "None".to_string()
        }
    }
}

// --------------------------------------------------------------------------- Go

fn go(out: &mut Out, module: &Module) {
    out.line("package main");
    out.blank();
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                let name = out.name(&c.name);
                for line in &c.doc {
                    out.line(&format!("// {name} {line}"));
                }
                let value = go_expr(out, &c.value);
                out.line(&format!("const {name} = {value}"));
                out.fidelity.constants += 1;
                out.blank();
            }
            Item::Record(r) => {
                let name = out.name(&r.name);
                for line in &r.doc {
                    out.line(&format!("// {name} {line}"));
                }
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
                for m in &r.methods {
                    go_function(out, m, Some(&name));
                    out.blank();
                }
            }
            Item::Function(f) => {
                go_function(out, f, None);
                out.blank();
            }
            Item::Import { text, line } => {
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
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
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
    let name = out.name(&f.name);
    for line in &f.doc {
        out.line(&format!("// {name} {line}"));
    }
    if f.is_async {
        out.line(
            &out.comment(
                "declared async in the source; Go has no async — call this from a goroutine",
            ),
        );
    }
    let mut foreign = false;
    let mut changed = false;
    let params: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| {
            let spelled = spell_param(out.language, p.kind, &out.name(&p.name), &mut changed)?;
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
                    foreign = true;
                    unknown(out, &p.name)
                }
            };
            Some(format!("{spelled} {ty}"))
        })
        .collect();
    let returns = match &f.returns {
        Some(Type::Unit) | None => String::new(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            format!(" {}", go_type(t))
        }
    };
    let receiver = receiver.map(|r| format!("(s *{r}) ")).unwrap_or_default();
    out.line(&format!(
        "func {receiver}{name}({}){returns} {{",
        params.join(", ")
    ));
    out.open();
    go_block(out, &f.body, f.returns.as_ref());
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
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
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
                let text = value
                    .as_ref()
                    .map(|v| format!(" {}", go_expr(out, v)))
                    .unwrap_or_default();
                out.line(&format!("return{text}"));
            }
            Stmt::Let { name, value, .. } => {
                let v = value
                    .as_ref()
                    .map(|v| go_expr(out, v))
                    .unwrap_or_else(|| "nil".to_string());
                let bound = out.name(name);
                out.line(&format!("{bound} := {v}"));
            }
            Stmt::Assign { target, value } => {
                let t = go_expr(out, target);
                let v = go_expr(out, value);
                out.line(&format!("{t} = {v}"));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let c = go_expr(out, condition);
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
            Stmt::While { condition, body } => {
                // Go spells `while` as a one-clause `for`.
                let c = go_expr(out, condition);
                out.line(&format!("for {c} {{"));
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
                let text = go_expr(out, e);
                out.line(&text);
            }
            Stmt::Break => out.line("break"),
            Stmt::Continue => out.line("continue"),
            // Go returns an error value. A catch block has no counterpart and
            // inventing one would change where the failure is handled.
            Stmt::Try { source, line, .. } => carry(
                out,
                &Unsupported {
                    construct: "try".into(),
                    source: source.clone(),
                    line: *line,
                },
            ),
            Stmt::Throw(value) => {
                let rendered = go_expr(out, value);
                out.line(&format!("panic({rendered})"));
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
        // Go writes "returns nothing" by writing nothing, which the return position
        // handles. Everywhere else — a field, a generic argument — it needs a type,
        // and `Result[, string]` is not one.
        Type::Unit => "struct{}".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float64".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("[]{}", go_type(inner)),
        Type::Map(k, v) => format!("map[{}]{}", go_type(k), go_type(v)),
        Type::Optional(inner) => format!("*{}", go_type(inner)),
        Type::Named { name, args } => generic(name, args, "[", "]", ".", go_type),
    }
}

/// Go has no `undefined`; a function that must return something returns its zero.
fn go_zero(ty: &Type) -> String {
    match ty {
        Type::Bool => "false".to_string(),
        Type::Int => "0".to_string(),
        Type::Float => "0".to_string(),
        Type::String => "\"\"".to_string(),
        Type::List(_) | Type::Map(_, _) | Type::Optional(_) => "nil".to_string(),
        Type::Unit => String::new(),
        Type::Named { name, .. } => format!("{}{{}}", generic(name, &[], "[", "]", ".", go_type)),
    }
}

fn go_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "nil".to_string(),
        Expr::Name(n) => out.name(n),
        // Not re-cased: `reading.get(…)` names a real method on a value whose type
        // this does not know, and `reading.Get(…)` is a different method.
        Expr::Field { of, name } => {
            let object = go_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => format!("{}[{}]", go_expr(out, of), go_expr(out, index)),
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            format!("{}({})", go_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            go_expr(out, left),
            op.c_like(),
            go_expr(out, right)
        ),
        // Go has no `await`. Writing the inner expression alone would turn a
        // suspension point into a plain call, which is the kind of silent change this
        // exists to avoid, so it is carried like any other construct with no
        // counterpart.
        Expr::Await(inner) => {
            let source = format!("await {}", go_expr(out, inner));
            out.carried(&Unsupported {
                construct: "await".into(),
                source: source.clone(),
                line: 0,
            });
            format!("nil /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // `NewThing(..)` is the Go convention, but it is a convention rather than a
        // rule, and a constructor this tool invented would be a name that does not
        // exist.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            let target = go_expr(out, callee);
            let source = format!("new {target}({})", rendered.join(", "));
            out.carried(&Unsupported {
                construct: "new".into(),
                source: source.clone(),
                line: 0,
            });
            format!("nil /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go spells this as a two-value type assertion, which is a statement.
        Expr::InstanceOf { value, ty } => {
            let rendered = go_expr(out, value);
            let named = go_expr(out, ty);
            let source = format!("{rendered} instanceof {named}");
            out.carried(&Unsupported {
                construct: "instanceof".into(),
                source: source.clone(),
                line: 0,
            });
            format!("false /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Keyword { name, value } => {
            let rendered = go_expr(out, value);
            let source = format!("{name}={rendered}");
            out.carried(&Unsupported {
                construct: "keyword argument".into(),
                source: source.clone(),
                line: 0,
            });
            format!("nil /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", go_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| go_expr(out, i)).collect();
            format!("[]any{{{}}}", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", go_expr(out, k), go_expr(out, v)))
                .collect();
            format!("map[string]any{{{}}}", rendered.join(", "))
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
                format!("{literal:?}")
            } else {
                format!("fmt.Sprintf({literal:?}, {})", args.join(", "))
            }
        }
        // Go has no comprehension and no map/filter on slices; writing a loop here
        // would be inventing statements from an expression.
        Expr::Comprehension { .. } => {
            out.carried(&Unsupported {
                construct: "comprehension".into(),
                source: "a comprehension, which Go spells as a loop".into(),
                line: 0,
            });
            "nil".to_string()
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("nil /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

// ------------------------------------------------------------------- TypeScript

fn typescript(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
            Item::Constant(c) => {
                for line in &c.doc {
                    out.line(&format!("/** {line} */"));
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
                    out.line(&format!("/** {line} */"));
                }
                let export = if r.exported { "export " } else { "" };
                // A record with no methods is an interface: it is data, and an
                // interface is what TypeScript calls that.
                if r.methods.is_empty() {
                    let type_name = out.name(&r.name);
                    out.line(&format!("{export}interface {type_name} {{"));
                    out.open();
                    for f in &r.fields {
                        for line in &f.doc {
                            out.line(&format!("/** {line} */"));
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
                    out.line(&format!("{export}class {type_name} {{"));
                    out.open();
                    for f in &r.fields {
                        let ty =
                            f.ty.as_ref()
                                .map(ts_type)
                                .unwrap_or_else(|| unknown(out, &f.name));
                        let field_name = out.field(&f.name);
                        out.line(&format!("{field_name}: {ty};"));
                    }
                    for m in &r.methods {
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
            Item::Import { text, line } => {
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
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn ts_function(out: &mut Out, f: &Function, method: bool) {
    for line in &f.doc {
        out.line(&format!("/** {line} */"));
    }
    let mut foreign = false;
    let mut changed = false;
    let params: Vec<String> = f
        .params
        .iter()
        .filter_map(|p| {
            let spelled = spell_param(out.language, p.kind, &out.name(&p.name), &mut changed)?;
            let annotation = match &p.ty {
                Some(t) => {
                    if out.is_foreign(t) {
                        foreign = true;
                    }
                    format!(": {}", ts_type(t))
                }
                None => {
                    foreign = true;
                    ": unknown".to_string()
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
    // An async function returns a promise of its declared type. Writing the bare type
    // would be a signature that says something the function does not do.
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
    let prefix = if method {
        asynchrony.to_string()
    } else if f.exported {
        format!("export {asynchrony}function ")
    } else {
        format!("{asynchrony}function ")
    };
    out.line(&format!(
        "{prefix}{}({}){returns} {{",
        out.name(&f.name),
        params.join(", ")
    ));
    out.open();
    ts_block(out, &f.body);
    out.close();
    out.line("}");

    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
        out.fidelity.notes.push(format!(
            "`{}` used Python's keyword-only or splat parameters, which {} has no \
             spelling for; the types carried but callers write the call differently",
            f.name, out.language
        ));
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
}

fn ts_block(out: &mut Out, body: &[Stmt]) {
    if body.is_empty() {
        out.line(&format!("throw new Error(\"{MARKER}\");"));
        return;
    }
    for stmt in body {
        match stmt {
            Stmt::Return(value) => {
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
                let annotation = ty
                    .as_ref()
                    .map(|t| format!(": {}", ts_type(t)))
                    .unwrap_or_default();
                let keyword = if *mutable { "let" } else { "const" };
                let v = value
                    .as_ref()
                    .map(|v| ts_expr(out, v))
                    .unwrap_or_else(|| "undefined".to_string());
                let bound = out.name(name);
                out.line(&format!("{keyword} {bound}{annotation} = {v};"));
            }
            Stmt::Assign { target, value } => {
                let t = ts_expr(out, target);
                let v = ts_expr(out, value);
                out.line(&format!("{t} = {v};"));
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
            Stmt::While { condition, body } => {
                let c = ts_expr(out, condition);
                out.line(&format!("while ({c}) {{"));
                out.open();
                ts_block(out, body);
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
                // TypeScript has one catch clause and no types on it. Python's typed
                // `except`s become `instanceof` tests inside it, which is how the same
                // intent is written here — and the trailing `throw` keeps an
                // unmatched error propagating rather than swallowing it.
                if !catches.is_empty() {
                    let bound = catches
                        .iter()
                        .find_map(|c| c.binding.clone())
                        .unwrap_or_else(|| "error".to_string());
                    let bound = out.name(&bound);
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
                                    let rendered = ts_type(ty);
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
                let rendered = ts_expr(out, value);
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

fn ts_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "void".to_string(),
        Type::Bool => "boolean".to_string(),
        // TypeScript has one numeric type, and pretending otherwise would be a lie
        // about the signature.
        Type::Int | Type::Float => "number".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("{}[]", ts_type(inner)),
        Type::Map(k, v) => format!("Record<{}, {}>", ts_type(k), ts_type(v)),
        Type::Optional(inner) => format!("{} | null", ts_type(inner)),
        Type::Named { name, args } => generic(name, args, "<", ">", ".", ts_type),
    }
}

fn ts_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => format!("{v:?}"),
        Expr::Null => "null".to_string(),
        Expr::Name(n) => out.name(n),
        Expr::Field { of, name } => {
            let object = ts_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => format!("{}[{}]", ts_expr(out, of), ts_expr(out, index)),
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
            format!("{}({})", ts_expr(out, callee), rendered.join(", "))
        }
        Expr::Binary { op, left, right } => {
            let spelling = match op {
                BinaryOp::Eq => "===",
                BinaryOp::Ne => "!==",
                other => other.c_like(),
            };
            format!("{} {spelling} {}", ts_expr(out, left), ts_expr(out, right))
        }
        Expr::Await(inner) => format!("await {}", ts_expr(out, inner)),
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| ts_expr(out, a)).collect();
            format!("new {}({})", ts_expr(out, callee), rendered.join(", "))
        }
        Expr::InstanceOf { value, ty } => {
            let rendered = ts_expr(out, value);
            format!("{rendered} instanceof {}", ts_expr(out, ty))
        }
        Expr::Keyword { name, value } => {
            let rendered = ts_expr(out, value);
            let source = format!("{name}={rendered}");
            out.carried(&Unsupported {
                construct: "keyword argument".into(),
                source: source.clone(),
                line: 0,
            });
            format!("undefined /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", ts_expr(out, operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| ts_expr(out, i)).collect();
            format!("[{}]", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    // A string key that is a plain identifier is written bare, which
                    // is what anyone writing TypeScript by hand would do.
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
                    TemplatePart::Text(text) => {
                        body.push_str(&text.replace('\\', "\\\\").replace('`', "\\`"))
                    }
                    TemplatePart::Expr(e) => {
                        body.push_str("${");
                        body.push_str(&ts_expr(out, e));
                        body.push('}');
                    }
                }
            }
            format!("`{body}`")
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
            format!("{it}{filter}.map(({name}) => {})", ts_expr(out, element))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("null /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

// ------------------------------------------------------------------------ shared

/// A named type, spelled the way this target spells generics.
///
/// The name is deliberately *not* case-converted: a `Reading` or an `HttpResponse` is
/// a real type somewhere, and renaming it to suit a convention would point the
/// signature at something that does not exist.
///
/// A name that cannot be written as a type at all — a tuple, a closure, a trait
/// object — becomes the target's unknown type. Emitting it verbatim produced
/// `-> Result<(), String>` in a Python file, which Python cannot parse; and a
/// signature that does not parse is worse than one that admits a gap.
/// A qualified name arrives spelled the source language's way: Go's `sync.Mutex` is
/// not Rust, and Rust's `std::sync::Mutex` is not Go. The path is kept — it says where
/// the type came from — and only `separator` changes.
fn generic(
    name: &str,
    args: &[Type],
    open: &str,
    close: &str,
    separator: &str,
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
        .join(separator);
    if args.is_empty() {
        return clean;
    }
    let rendered: Vec<String> = args.iter().map(render).collect();
    format!("{clean}{open}{}{close}", rendered.join(", "))
}

/// A name that is not a type, turned into something that at least parses.
fn sanitise(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Can this be written as a bare object key?
fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !text.chars().next().is_some_and(|c| c.is_numeric())
}

/// Is this a type the IR only knows the name of?
///
/// A signature containing one is carried across but is not a *complete* translation,
/// A type the source never wrote down. Named rather than guessed.
fn unknown(out: &mut Out, of: &str) -> String {
    out.fidelity
        .notes
        .push(format!("`{of}` had no declared type in the source"));
    match out.language {
        Language::Rust => "()".to_string(),
        Language::Python => "object".to_string(),
        Language::Go => "any".to_string(),
        _ => "unknown".to_string(),
    }
}
