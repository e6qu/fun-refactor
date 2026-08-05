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
    /// A struct, class, interface — `PascalCase` in every one of them.
    Type,
    /// A module-level constant — `SCREAMING_SNAKE` in most of them. Go spells it like
    /// anything else, because there the capital letter means exported, and Zig does not
    /// shout at all.
    Constant,
    /// A function or method.
    ///
    /// The same as [`Kind::Value`] in every target but Zig, whose style guide splits
    /// what the others join: `camelCase` for what you call, `snake_case` for what you
    /// bind. One `Kind` for both spelled every local in a Zig file as a function.
    Function,
    /// A field, parameter or local.
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
                // Zig does not shout. Its standard library writes `std.math.pi`, and
                // a capital there would say "type" rather than "constant".
                Language::Zig => snake_always(name),
                _ => screaming(name),
            },
            Kind::Function => match language {
                Language::Rust | Language::Python => snake_always(name),
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
        // `_` is not a name, it is the word for "no name" — Rust, Go, Python and Zig
        // all use it — and putting it through a convention asked what the empty word
        // is called in `camelCase`. The answer was the empty string, so every `_ = x;`
        // in a Zig file came out as ` = x;`.
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
        // A constructor's own name is never written — every target has its own word for
        // one — so it must not claim a spelling. Java names it after the class, and
        // letting it into the map meant every Java class came out named after its
        // constructor: `class a` where the source said `class A`.
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
fn spell_param(out: &Out, kind: ParamKind, raw: &str, changed: &mut bool) -> Option<String> {
    // A bare `*` or `/` is punctuation standing where a parameter would go. Putting it
    // through the naming map asked what `*` is called in TypeScript, and the answer —
    // "not a name this language can spell" — was a true sentence about a thing that is
    // never written down.
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
        _ => return false,
    };
    list.contains(&name)
}

/// What this language calls the receiver inside a method body.
///
/// The readers normalise nothing: each records the word its own source used, because
/// Go lets the author choose it. This is the other half — the word to put back on —
/// and it is a fact about the target rather than about the source.
fn receiver_word(language: Language) -> &'static str {
    match language {
        Language::Java | Language::TypeScript | Language::Tsx => "this",
        // Go's convention is a one- or two-letter abbreviation of the type, and there
        // is no way to pick one that is guaranteed not to collide with a parameter.
        // `self` is not a keyword there and cannot collide with anything the source
        // declared, because a Go file that used it would have been read as a receiver.
        _ => "self",
    }
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
        Language::Java => java(&mut out, module),
        Language::Zig => zig(&mut out, module),
        Language::TypeScript | Language::Tsx => typescript(&mut out, module),
        other => bail!(
            "there is no writer for {other}: it has no functions or records to write \
             these into"
        ),
    }
    let unnameable: Vec<String> = out.unnameable.borrow().iter().cloned().collect();
    for name in unnameable {
        out.fidelity.notes.push(format!(
            "`{name}` is not a name {language} can spell; it is written `{}`, and every \
             use of it needs a real name",
            sanitise(&name)
        ));
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
    /// Names that are not identifiers at all in any of these languages.
    ///
    /// Reported rather than quietly reshaped, because the replacement is a name the
    /// source never used and every call to it has to be found by hand.
    unnameable: std::cell::RefCell<std::collections::BTreeSet<String>>,
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
    /// Text a writer could not put where the expression it replaced stood.
    ///
    /// Zig is the only target here with no block comment: `//` runs to the end of the
    /// line, so a carried fragment written beside an expression would swallow the rest
    /// of the statement — including its semicolon. It is queued here and flushed as
    /// whole-line comments above the statement, which is the only place in Zig a
    /// comment can go.
    pending: Vec<String>,
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
            unnameable: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            escaped: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            declared_types: std::collections::BTreeSet::new(),
            pending: Vec::new(),
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

    /// The same name, made writable where the target will not take it as written.
    fn legal(&self, spelled: String) -> String {
        // A TypeScript member can be named by an expression — `[Symbol.dispose]()` —
        // and no other language here has anything of the kind. Written through, it
        // produced `pub fn [symbol.dispose](&self)`, which is not Rust. A qualified
        // path is a different matter and must survive untouched: `std::fmt::Display`
        // and `sync.Mutex` name real things.
        if !is_writable_identifier(&spelled) {
            self.unnameable.borrow_mut().insert(spelled.clone());
            return sanitise(&spelled);
        }
        if !reserved(self.language, &spelled) {
            return spelled;
        }
        // The receiver's own word is never an escape problem: it reaches here only
        // because a method body used it, and it is exactly the word this target binds.
        // Rust also refuses to raw-escape `self`, so escaping it replaced a correct
        // file with `r#self`, which is a compile error.
        if spelled == receiver_word(self.language) {
            return spelled;
        }
        self.escaped.borrow_mut().insert(spelled.clone());
        match self.language {
            // Rust and Zig both have a spelling for exactly this, and under it the
            // name stays the same identifier rather than becoming a different one.
            // Rust's does not stretch to the three words that name a scope: `r#crate`,
            // `r#super` and `r#Self` are rejected the same way `r#self` is.
            Language::Rust => match spelled.as_str() {
                "crate" | "super" | "Self" => format!("{spelled}_"),
                _ => format!("r#{spelled}"),
            },
            Language::Zig => format!("@\"{spelled}\""),
            _ => format!("{spelled}_"),
        }
    }

    /// The name to write for this function: its own, or the target's word for a
    /// constructor.
    ///
    /// A constructor's *name* is not information — it is the type's name in Java, a
    /// fixed word in Python and TypeScript, and a habit in the other three. What the IR
    /// carries is that it is one.
    fn function_name(&self, f: &Function) -> String {
        match (f.is_constructor, f.receiver.as_deref()) {
            (true, Some(owner)) => self.legal(constructor_name(self.language, owner)),
            _ => self.name(&f.name),
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

    /// Spell the receiver this target's way while a method body is written.
    ///
    /// Returns what was there before, for [`Out::unbind_receiver`]. The mapping goes
    /// through the same [`Out::names`] every other rename does, so a body reaches it
    /// by the one route rather than by a second rule that can drift from the first.
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

    /// One line of output, at the current indent.
    ///
    /// Text with newlines in it becomes several lines, each indented. It arrives that
    /// way more often than it looks: a `/* ... */` comment is a single node however
    /// many lines it spans, and pushing it through whole indented the first line and
    /// left the rest hanging in column one — with only the first carrying whatever
    /// marker made it a comment.
    fn line(&mut self, text: &str) {
        if text.is_empty() {
            self.text.push('\n');
            return;
        }
        for piece in text.split('\n') {
            if !piece.is_empty() {
                for _ in 0..self.indent {
                    self.text.push_str("    ");
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

    /// `text` as a comment — every line of it.
    ///
    /// A marker on the first line only is not a comment, it is one comment followed by
    /// whatever the rest of the lines happen to parse as. Multi-line text reaches here
    /// from every `/* ... */` in every source.
    fn comment(&self, text: &str) -> String {
        let marker = match self.language {
            Language::Python => "#",
            _ => "//",
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
                inherited_base(out, r, false);
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
                    for m in &methods_of(out, r, false) {
                        rust_function(out, m, m.receiver_binding.is_some());
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        out.line(&out.comment("declared async in the source"));
    }
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push(format!("&{}", receiver_word(out.language)));
    }
    let mut foreign = false;
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
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    rust_block(out, &f.body);
    out.close();
    out.line("}");

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
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

/// One side of a binary expression, bracketed when the enclosing operator would bind
/// into it.
///
/// The writers rendered `left op right` and nothing else, so a group the source wrote
/// was a group the translation lost: `(a + b) * c` came out of every one of them as
/// `a + b * c`, and `a - (b - c)` as `a - b - c`. Neither is the same number.
///
/// Brackets are decided from precedence rather than copied from the source, so the
/// result is right even where the two languages disagree about binding — and a group
/// that was never needed does not survive the trip either.
///
/// The right-hand side takes brackets at *equal* precedence as well, because every
/// operator here associates to the left: `a - (b - c)` needs them and `(a - b) - c`
/// does not.
fn binary_operand(text: String, operand: &Expr, enclosing: BinaryOp, on_the_right: bool) -> String {
    let inner = match operand {
        Expr::Binary { op, .. } => op.precedence(),
        // A conditional binds looser than any operator in the table, so it always
        // needs the brackets. Everything else — a name, a literal, a call, an index —
        // is one thing and never does.
        Expr::Ternary { .. } | Expr::Coalesce { .. } => 0,
        _ => return text,
    };
    let outer = enclosing.precedence();
    match inner < outer || (on_the_right && inner == outer) {
        true => format!("({text})"),
        false => text,
    }
}

/// The operand of `!` or `-`, bracketed when it is not a single thing.
///
/// `-(a + b)` is not `-a + b`, and `!(a and b)` is not `!a and b` — which is the whole
/// of De Morgan's law and the reason it is a refactoring in its own right.
fn unary_operand(text: String, operand: &Expr) -> String {
    match operand {
        Expr::Binary { .. } | Expr::Ternary { .. } | Expr::Coalesce { .. } => {
            format!("({text})")
        }
        _ => text,
    }
}

fn rust_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // `Option::unwrap_or`, which is what a Rust reader expects and what the IR's
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
            binary_operand(rust_expr(out, left), left, *op, false),
            op.c_like(),
            binary_operand(rust_expr(out, right), right, *op, true)
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
            format!("{sign}{}", unary_operand(rust_expr(out, operand), operand))
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
                format!("{}.to_string()", quoted(Language::Rust, &literal))
            } else {
                format!(
                    "format!({}, {})",
                    quoted(Language::Rust, &literal),
                    args.join(", ")
                )
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
                // Python spells the base in parentheses after the name.
                let base = inherited_base(out, r, true)
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
                    out.line(&format!("{field_name}: {annotation}"));
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    if method {
        params.push(receiver_word(out.language).to_string());
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
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
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
    python_block(out, &f.body);
    out.close();

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
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

/// A line, preceded by anything an expression could not say where it stood.
///
/// Python has no inline comment: `#` runs to the end of the line, so a note written
/// inside a call's parentheses swallows the closing one. The note used to go only to
/// the fidelity report, which left a bare `None` sitting in the file where a value had
/// been — true in the report and a lie in the code. It goes above the statement now,
/// which is where Zig puts its own for exactly the same reason.
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
                    // `else: if ...` is written `elif` when that is all it is.
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
            Stmt::While { condition, body } => {
                let c = python_expr(out, condition);
                python_line(out, &format!("while {c}:"));
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
                let rendered = python_expr(out, value);
                python_line(out, &format!("raise {rendered}"));
            }
            Stmt::Comment(text) => {
                let line = out.comment(text);
                python_line(out, &line);
            }
            Stmt::Unsupported(u) => carry(out, u),
        }
    }
    // A body that is only carried-over comments still needs a statement to be Python.
    if !wrote {
        python_line(out, "raise NotImplementedError");
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
        // Python has to name the value twice, and naming a call twice calls it twice.
        Expr::Coalesce { value, fallback } => match nameable(value) {
            true => format!(
                "{} if {} is not None else {}",
                python_expr(out, value),
                python_expr(out, value),
                python_expr(out, fallback)
            ),
            false => {
                let source = format!(
                    "{} ?? {}",
                    python_expr(out, value),
                    python_expr(out, fallback)
                );
                out.carried(&Unsupported {
                    construct: "?? on a value that cannot be named twice".into(),
                    source: source.clone(),
                    line: 0,
                });
                out.pending.push(format!("{MARKER}: {source}"));
                "None".to_string()
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
                binary_operand(python_expr(out, left), left, *op, false),
                binary_operand(python_expr(out, right), right, *op, true)
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
        Expr::Unary { op, operand } => {
            let rendered = unary_operand(python_expr(out, operand), operand);
            match op {
                UnaryOp::Not => format!("not {rendered}"),
                UnaryOp::Neg => format!("-{rendered}"),
            }
        }
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
        // An f-string quotes its text and leaves its expressions as code. Escaping the
        // assembled body as one string put a backslash in front of every quote inside
        // `{...}` — and `f"{x.replace(\"-\", \" \")}"` is not a string Python reads.
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
            // Before 3.12 an f-string's expression may contain neither a backslash nor
            // the quote that delimits the literal. Where one does, the same thing said
            // with `+` and `str` is exact and reads in every version.
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
        // It is queued and written above the statement by `python_line`.
        Expr::Unsupported(u) => {
            out.carried(u);
            out.pending.push(format!("{MARKER}: {}", u.source));
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    let name = out.function_name(f);
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
    // Go's convention is a one- or two-letter abbreviation, and there is no letter
    // guaranteed not to be a parameter's name already. The word the body uses and the
    // word the signature binds have to be the same one, so both come from here.
    let receiver = receiver
        .map(|r| format!("({} *{r}) ", receiver_word(out.language)))
        .unwrap_or_default();
    out.line(&format!(
        "func {receiver}{name}({}){returns} {{",
        params.join(", ")
    ));
    out.open();
    go_block(out, &f.body, f.returns.as_ref());
    out.close();
    out.line("}");

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
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
        Type::Named { name, args } => go_named(name, args),
    }
}

/// A type carried across by name, with the one qualifier Go allows.
///
/// Go names a foreign type `package.Name` and has no third level: `crate.model.Symbol`
/// is not a type there, it is a field of a field, and every signature mentioning one
/// failed to parse. The last two segments are what a Go author would write after
/// importing that package — which the header already lists — so the path is shortened
/// rather than flattened, and the name itself is untouched.
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
        Type::List(_) | Type::Map(_, _) | Type::Optional(_) => "nil".to_string(),
        Type::Unit => String::new(),
        Type::Named { name, .. } => format!("{}{{}}", go_named(name, &[])),
    }
}

fn go_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Go has nothing for this: not an operator, not a standard function. Writing
        // the `if` it would take needs somewhere to put the result, which does not
        // exist inside an argument list.
        Expr::Coalesce { value, fallback } => {
            let source = format!("{} ?? {}", go_expr(out, value), go_expr(out, fallback));
            out.carried(&Unsupported {
                construct: "??".into(),
                source: source.clone(),
                line: 0,
            });
            format!("nil /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go is the one language here with no conditional expression, and turning one
        // into an `if` statement needs somewhere to put the result — which does not
        // exist inside an argument list.
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => {
            let source = format!(
                "{} ? {} : {}",
                go_expr(out, condition),
                go_expr(out, then),
                go_expr(out, otherwise)
            );
            out.carried(&Unsupported {
                construct: "conditional expression".into(),
                source: source.clone(),
                line: 0,
            });
            format!("nil /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Int(v) => v.clone(),
        Expr::Float(v) => v.clone(),
        Expr::Bool(v) => v.to_string(),
        Expr::Str(v) => quoted(Language::Go, v),
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
            binary_operand(go_expr(out, left), left, *op, false),
            op.c_like(),
            binary_operand(go_expr(out, right), right, *op, true)
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
            format!("{sign}{}", unary_operand(go_expr(out, operand), operand))
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
                quoted(Language::Go, &literal)
            } else {
                format!(
                    "fmt.Sprintf({}, {})",
                    quoted(Language::Go, &literal),
                    args.join(", ")
                )
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
                // A record with no methods is an interface: it is data, and an
                // interface is what TypeScript calls that.
                if r.methods.is_empty() {
                    let type_name = out.name(&r.name);
                    let base = inherited_base(out, r, true)
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
                    let base = inherited_base(out, r, true)
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
                        out.line(&format!("{field_name}: {ty};"));
                    }
                    for m in &methods_of(out, r, false) {
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

/// `inside_class` is where it is written, which is not the same question as whether it
/// takes a receiver — a class holds `static empty()` beside `label()`, and one `bool`
/// answering both put `export function` inside a class body.
fn ts_function(out: &mut Out, f: &Function, inside_class: bool) {
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    for line in &f.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    let mut foreign = false;
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
    let prefix = if inside_class {
        let modifier = match f.receiver_binding.is_some() {
            true => "",
            false => "static ",
        };
        format!("{modifier}{asynchrony}")
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

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
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
            format!(
                "{} {spelling} {}",
                binary_operand(ts_expr(out, left), left, *op, false),
                binary_operand(ts_expr(out, right), right, *op, true)
            )
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
            format!("{sign}{}", unary_operand(ts_expr(out, operand), operand))
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
            // `[x for x in xs if p(x)]` keeps every element it selects, so the map is
            // the identity and writing it out says nothing: `xs.filter(p).map((x) => x)`
            // is `xs.filter(p)` with three extra words.
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

// ------------------------------------------------------------------------- Java

/// Java has no top level below the type, so a module is written *inside* a class.
///
/// That is the one structural thing this writer does that no other does. Every other
/// target takes the module's items and writes them out; Java has to invent a container
/// for them, and a public class must be named after its file — which is why [`Module`]
/// carries a name at all.
///
/// A record with methods becomes its own class beside it; the loose functions and
/// constants become `static` members of the file's class, because that is what a Java
/// file full of free functions has to be.
fn java(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        if let Item::Import { text, line } = item {
            out.fidelity.imports_listed += 1;
            let header = out.comment(&format!(
                "the source imported this at line {line}; the equivalent here is yours to add"
            ));
            out.line(&header);
            for l in text.lines() {
                let commented = out.comment(l);
                out.line(&commented);
            }
            out.blank();
        }
    }

    // Everything that is not a record has nowhere else to live.
    let loose: Vec<&Item> = module
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                Item::Constant(_) | Item::Function(_) | Item::Unsupported(_)
            )
        })
        .collect();
    let records: Vec<&Record> = module
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Record(r) => Some(r),
            _ => None,
        })
        .collect();

    // **One public class per file**, named after the file. That is not a convention in
    // Java, it is a rule the compiler enforces, and it is the whole reason this writer
    // has to make a choice no other one does. A module that is only a record gives its
    // name to the file; a module with loose functions keeps the file's own class public
    // and writes its records as package-private siblings beside it.
    if loose.is_empty() {
        for (index, record) in records.iter().enumerate() {
            if index > 0 {
                out.blank();
            }
            java_record(out, record, index == 0);
        }
        return;
    }

    for record in &records {
        java_record(out, record, false);
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
            Item::Unsupported(u) => carry(out, u),
            _ => {}
        }
        if index != last {
            out.blank();
        }
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
        out.line(&format!("{field_visibility} {ty} {field_name};"));
    }
    if !record.fields.is_empty() && !record.methods.is_empty() {
        out.blank();
    }
    for (index, method) in methods_of(out, record, true).iter().enumerate() {
        if index > 0 {
            out.blank();
        }
        java_function(out, method, method.receiver_binding.is_none());
    }
    out.close();
    out.line("}");
    out.fidelity.records += 1;
}

fn java_function(out: &mut Out, f: &Function, is_static: bool) {
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    for line in &f.doc {
        out.line(&format!("/** {} */", block_comment_safe(line)));
    }
    if f.is_async {
        let note = out.comment(
            "declared async in the source; Java has no async — return a CompletableFuture \
             or call this from an executor",
        );
        out.line(&note);
    }

    let mut foreign = false;
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
                    foreign = true;
                    unknown(out, &p.name)
                }
            };
            Some(format!("{ty} {spelled}"))
        })
        .collect();

    let returns = match &f.returns {
        Some(Type::Unit) | None => "void".to_string(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            java_type(t)
        }
    };

    let visibility = if f.exported { "public" } else { "private" };
    // A constructor writes no return type at all. `void` would make it a method that
    // happens to have the class's name — which compiles, and is not a constructor.
    let returns = match f.is_constructor {
        true => String::new(),
        false => format!("{returns} "),
    };
    let modifier = if is_static && !f.is_constructor {
        " static "
    } else {
        " "
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

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
}

fn java_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        // A method that returns something must return something; one that does not can
        // be empty, and an invented value would be a guess at what the body did.
        if matches!(returns, Some(t) if *t != Type::Unit) {
            out.line("throw new UnsupportedOperationException(\"not translated\");");
        }
        return;
    }
    for stmt in body {
        java_stmt(out, stmt);
    }
}

fn java_stmt(out: &mut Out, stmt: &Stmt) {
    match stmt {
        Stmt::Comment(text) => {
            let line = out.comment(text);
            out.line(&line);
        }
        Stmt::Return(value) => {
            let text = value
                .as_ref()
                .map(|e| format!(" {}", java_expr(out, e)))
                .unwrap_or_default();
            out.line(&format!("return{text};"));
        }
        Stmt::Throw(value) => {
            let rendered = java_expr(out, value);
            out.line(&format!("throw {rendered};"));
        }
        Stmt::Let {
            name, ty, value, ..
        } => {
            let rendered = value
                .as_ref()
                .map(|v| java_expr(out, v))
                .unwrap_or_else(|| "null".to_string());
            // `var` is Java 10 and inference is the compiler's job, not this tool's:
            // writing a type it has not got would be a guess.
            let declared = ty
                .as_ref()
                .map(java_type)
                .unwrap_or_else(|| "var".to_string());
            let bound = out.name(name);
            out.line(&format!("{declared} {bound} = {rendered};"));
        }
        Stmt::Assign { target, value } => {
            // `d[k] = v` is `d.put(k, v)` here. Java has no assignable subscript on a
            // collection, and `d.get(k) = v` — which is what rendering the target as an
            // expression produces — is not a statement in the language at all.
            if let Expr::Index { of, index } = target {
                let object = java_expr(out, of);
                let at = java_expr(out, index);
                let right = java_expr(out, value);
                out.line(&format!("{object}.put({at}, {right});"));
                return;
            }
            let left = java_expr(out, target);
            let right = java_expr(out, value);
            out.line(&format!("{left} = {right};"));
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
        Stmt::While { condition, body } => {
            let c = java_expr(out, condition);
            out.line(&format!("while ({c}) {{"));
            out.open();
            java_block(out, body, None);
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
            // The element type is the collection's, which this does not track.
            out.line(&format!("for (var {bound} : {it}) {{"));
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
                java_block(out, &clause.body, None);
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
        Stmt::Break => out.line("break;"),
        Stmt::Continue => out.line("continue;"),
        Stmt::Unsupported(u) => carry(out, u),
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
        Type::Map(k, v) => format!("Map<{}, {}>", java_boxed(k), java_boxed(v)),
        // Java's `Optional<T>` is the closest thing it has, and it is a real type
        // rather than a nullable annotation.
        Type::Optional(inner) => format!("Optional<{}>", java_boxed(inner)),
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
        // Java spells it as a static call, and has to name the value twice to do it.
        Expr::Coalesce { value, fallback } => match nameable(value) {
            true => format!(
                "Objects.requireNonNullElse({}, {})",
                java_expr(out, value),
                java_expr(out, fallback)
            ),
            false => {
                let source = format!("{} ?? {}", java_expr(out, value), java_expr(out, fallback));
                out.carried(&Unsupported {
                    construct: "?? on a value that cannot be named twice".into(),
                    source: source.clone(),
                    line: 0,
                });
                format!("null /* {MARKER}: {} */", source.replace("*/", "* /"))
            }
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
        Expr::Name(n) => out.name(n),
        Expr::Field { of, name } => {
            let object = java_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        Expr::Index { of, index } => {
            let object = java_expr(out, of);
            let at = java_expr(out, index);
            // A subscript is `get` on a collection and `[…]` on an array, and which
            // this is depends on a type nothing here tracks.
            format!("{object}.get({at})")
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| java_expr(out, a)).collect();
            format!("{}({})", java_expr(out, callee), rendered.join(", "))
        }
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| java_expr(out, a)).collect();
            format!("new {}({})", java_expr(out, callee), rendered.join(", "))
        }
        Expr::InstanceOf { value, ty } => {
            let rendered = java_expr(out, value);
            format!("{rendered} instanceof {}", java_expr(out, ty))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            binary_operand(java_expr(out, left), left, *op, false),
            op.c_like(),
            binary_operand(java_expr(out, right), right, *op, true)
        ),
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", unary_operand(java_expr(out, operand), operand))
        }
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| java_expr(out, i)).collect();
            format!("List.of({})", rendered.join(", "))
        }
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}, {}", java_expr(out, k), java_expr(out, v)))
                .collect();
            format!("Map.of({})", rendered.join(", "))
        }
        // Java's text blocks and `formatted` are neither of these, and `+` is what a
        // reader will recognise.
        Expr::Template(parts) => {
            let rendered: Vec<String> = parts
                .iter()
                .map(|part| match part {
                    TemplatePart::Text(text) => quoted(Language::Java, text),
                    TemplatePart::Expr(e) => java_expr(out, e),
                })
                .collect();
            match rendered.is_empty() {
                true => "\"\"".to_string(),
                false => rendered.join(" + "),
            }
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
        // Java has no `await`: a suspension point is a `CompletableFuture.join()` or a
        // virtual thread, and which one is a fact about the program.
        Expr::Await(inner) => {
            let rendered = java_expr(out, inner);
            let source = format!("await {rendered}");
            out.carried(&Unsupported {
                construct: "await".into(),
                source: source.clone(),
                line: 0,
            });
            format!("null /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Keyword { name, value } => {
            let rendered = java_expr(out, value);
            let source = format!("{name}={rendered}");
            out.carried(&Unsupported {
                construct: "keyword argument".into(),
                source: source.clone(),
                line: 0,
            });
            format!("null /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            format!("null /* {MARKER}: {} */", u.source.replace("*/", "* /"))
        }
    }
}

// ------------------------------------------------------------------------- Zig

/// Zig.
///
/// Two facts about the language shape this writer. **A type is a value**: a struct is
/// what a `const` is bound to, so a record is written `const Reading = struct { … };`
/// and its methods live inside it rather than beside it. And **there is no block
/// comment** — `//` runs to the end of the line — so a carried-over fragment cannot be
/// written beside the expression it replaced. It goes above the statement instead, via
/// [`Out::pending`].
///
/// What has no counterpart and is carried rather than guessed at: `new`, `await`,
/// `throw`, `try`/`catch`, map literals, interpolated strings and comprehensions. Zig
/// models failure in the return type and removed `async` in 0.11, so an exception or a
/// suspension point arriving from another language has nowhere to land, and inventing
/// an error set would be inventing the program's vocabulary of failures.
fn zig(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    for item in &module.items {
        match item {
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
            // A struct is a value bound to a `const`, which is why this reads as a
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
                // `const Foo = struct {};` is valid Zig and the grammar this tool reads
                // with cannot parse it, so the output would be refused by its own
                // check. An empty `comptime` block does nothing, says so, and both
                // Zig and the grammar accept it. Recorded in BUGS.md as upstream.
                if r.fields.is_empty() && r.methods.is_empty() {
                    let note = out.comment(
                        "a struct with nothing in it; the empty comptime block stands \
                         in for `struct {}`, which tree-sitter-zig cannot parse",
                    );
                    out.line(&note);
                    out.line("comptime {}");
                }
                for f in &r.fields {
                    let ty =
                        f.ty.as_ref()
                            .map(zig_type)
                            .unwrap_or_else(|| unknown(out, &f.name));
                    let field_name = out.field(&f.name);
                    out.line(&format!("{field_name}: {ty},"));
                }
                for m in &methods_of(out, r, false) {
                    out.blank();
                    zig_function(
                        out,
                        m,
                        m.receiver_binding.is_some().then_some(name.as_str()),
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
            Item::Unsupported(u) => {
                carry(out, u);
                out.blank();
            }
        }
    }
}

fn zig_function(out: &mut Out, f: &Function, receiver: Option<&str>) {
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let bound = f.receiver_binding.clone();
    let previous = bound.as_ref().map(|b| out.bind_receiver(b));

    for line in &f.doc {
        out.line(&format!("/// {line}"));
    }
    if f.is_async {
        let note = out.comment(
            "declared async in the source; Zig removed `async` in 0.11 and has not \
             brought it back — this runs to completion",
        );
        out.line(&note);
    }

    let mut foreign = false;
    let mut changed = false;
    let mut params: Vec<String> = Vec::new();
    // A method takes its own type as an ordinary first parameter; there is no
    // receiver syntax to put it in.
    if let Some(ty) = receiver {
        params.push(format!("{}: {ty}", receiver_word(out.language)));
    }
    for p in &f.params {
        let Some(spelled) = spell_param(out, p.kind, &p.name, &mut changed) else {
            continue;
        };
        if p.kind != ParamKind::Normal {
            params.push(spelled);
            continue;
        }
        // Zig writes a type on every parameter and infers none of them, so one the
        // source never declared becomes `anytype` and is counted.
        let ty = match &p.ty {
            Some(t) => {
                if out.is_foreign(t) {
                    foreign = true;
                }
                zig_type(t)
            }
            None => {
                foreign = true;
                unknown(out, &p.name)
            }
        };
        params.push(format!("{spelled}: {ty}"));
    }

    let returns = match &f.returns {
        Some(Type::Unit) | None => "void".to_string(),
        Some(t) => {
            if out.is_foreign(t) {
                foreign = true;
            }
            zig_type(t)
        }
    };

    let visibility = if f.exported { "pub " } else { "" };
    out.line(&format!(
        "{visibility}fn {}({}) {returns} {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    // Zig rejects a `var` nothing writes to, so which keyword a binding takes is a
    // fact about the rest of the body rather than about the binding. Only the Rust
    // reader records mutability at all; every other one says "mutable" because it has
    // nothing better to say, and taking that at its word made a `const` file into a
    // `var` one that will not build.
    let mutated = zig_mutated(&f.body);
    zig_block(out, &f.body, f.returns.as_ref(), &mutated);
    out.close();
    out.line("}");

    if let (Some(b), Some(p)) = (bound.as_deref(), previous) {
        out.unbind_receiver(b, p);
    }
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed {
        out.fidelity.signatures_complete += 1;
    }
}

/// Every name this body writes to, including through a field or an index.
///
/// `r.value = 1` and `xs[0] = 1` both need `r` and `xs` to be `var`, so the root of
/// the target is what counts rather than the whole expression.
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
///
/// Every arm of [`zig_stmt`] renders its expressions first and writes afterwards, so
/// by the time this is called the queue holds exactly the fragments belonging to this
/// statement.
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
        Stmt::Comment(text) => {
            let line = out.comment(text);
            out.line(&line);
        }
        Stmt::Return(value) => {
            let text = value
                .as_ref()
                .map(|e| format!(" {}", zig_expr(out, e)))
                .unwrap_or_default();
            zig_line(out, &format!("return{text};"));
        }
        // Zig has no exceptions: a failure is a value in the return type, and which
        // error it is belongs to an error set this has no way to name.
        Stmt::Throw(value) => {
            let rendered = zig_expr(out, value);
            let source = format!("throw {rendered}");
            out.carried(&Unsupported {
                construct: "throw".into(),
                source: source.clone(),
                line: 0,
            });
            out.pending.push(format!("{MARKER}: {source}"));
            zig_line(out, "unreachable;");
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
            let annotation = ty
                .as_ref()
                .map(|t| format!(": {}", zig_type(t)))
                .unwrap_or_default();
            let keyword = if *mutable && mutated.contains(name) {
                "var"
            } else {
                "const"
            };
            let bound = out.name(name);
            zig_line(out, &format!("{keyword} {bound}{annotation} = {rendered};"));
        }
        Stmt::Assign { target, value } => {
            let left = zig_expr(out, target);
            let right = zig_expr(out, value);
            zig_line(out, &format!("{left} = {right};"));
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
        Stmt::While { condition, body } => {
            let c = zig_expr(out, condition);
            zig_line(out, &format!("while ({c}) {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        // `for (xs) |x| { … }` — the binding goes in a payload after the header rather
        // than inside it.
        Stmt::ForEach {
            binding,
            iterable,
            body,
        } => {
            let it = zig_expr(out, iterable);
            let bound = out.name(binding);
            zig_line(out, &format!("for ({it}) |{bound}| {{"));
            out.open();
            zig_block(out, body, None, mutated);
            out.close();
            out.line("}");
        }
        Stmt::Try { source, line, .. } => carry(
            out,
            &Unsupported {
                construct: "try/catch".into(),
                source: source.clone(),
                line: *line,
            },
        ),
        Stmt::Expr(Expr::Null) => {}
        // Zig has no bare expression statement: a value has to go somewhere. A call is
        // the one exception, and everything else is discarded into `_`, which is what
        // Zig would make you write by hand.
        Stmt::Expr(e) => {
            let text = zig_expr(out, e);
            match e {
                Expr::Call { .. } => zig_line(out, &format!("{text};")),
                _ => zig_line(out, &format!("_ = {text};")),
            }
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
        // Zig has no string type. A string is a slice of bytes that does not change,
        // and that is what a literal is.
        Type::String => "[]const u8".to_string(),
        Type::List(inner) => format!("[]const {}", zig_type(inner)),
        // Hashing a slice by its contents and hashing it by its address are different
        // maps in Zig, and the standard library makes you pick: `AutoHashMap` cannot
        // take a string key at all.
        Type::Map(key, value) => match key.as_ref() {
            Type::String => format!("std.StringHashMap({})", zig_type(value)),
            other => format!("std.AutoHashMap({}, {})", zig_type(other), zig_type(value)),
        },
        Type::Optional(inner) => format!("?{}", zig_type(inner)),
        // A generic type is a function of its arguments, so it is applied rather than
        // bracketed: `ArrayList(u8)`, not `ArrayList<u8>`.
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
///
/// A type written through by name can be a word this language spells something else
/// with: Go's `error` is Zig's keyword for an error set, and a signature returning it
/// did not parse. `@"error"` is how Zig writes an identifier that collides with one of
/// its own words, and under it the name still says what the source said.
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
///
/// `undefined` is Zig's own word for a value that has not been decided yet, and using
/// it is deliberate: the program will not run until a person replaces it.
fn zig_carry(out: &mut Out, construct: &str, source: String) -> String {
    out.carried(&Unsupported {
        construct: construct.into(),
        source: source.clone(),
        line: 0,
    });
    out.pending.push(format!("{MARKER}: {source}"));
    "undefined".to_string()
}

fn zig_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Zig has the operator, and means exactly this by it.
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
        Expr::Name(n) => out.name(n),
        Expr::Field { of, name } => {
            let object = zig_expr(out, of);
            format!("{object}.{}", out.field(name))
        }
        // `[…]` on a slice or an array and `.get(…)` on a map, and which this is
        // depends on a type nothing here tracks. Zig's indexable is the slice.
        Expr::Index { of, index } => {
            let object = zig_expr(out, of);
            let at = zig_expr(out, index);
            format!("{object}[{at}]")
        }
        Expr::Call { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| zig_expr(out, a)).collect();
            format!("{}({})", zig_expr(out, callee), rendered.join(", "))
        }
        // Zig has no `new`. A value is made by whatever function on the type returns
        // one, and which function that is — `init`, a literal, an allocator call — is
        // a fact about the type rather than about this expression.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| zig_expr(out, a)).collect();
            let target = zig_expr(out, callee);
            zig_carry(out, "new", format!("new {target}({})", rendered.join(", ")))
        }
        Expr::Binary { op, left, right } => format!(
            "{} {} {}",
            binary_operand(zig_expr(out, left), left, *op, false),
            zig_binary(*op),
            binary_operand(zig_expr(out, right), right, *op, true)
        ),
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sign}{}", unary_operand(zig_expr(out, operand), operand))
        }
        // An anonymous list is `.{ … }`, and what it coerces to is decided by where it
        // is used rather than by the literal.
        Expr::ListLit(items) => {
            let rendered: Vec<String> = items.iter().map(|i| zig_expr(out, i)).collect();
            format!(".{{ {} }}", rendered.join(", "))
        }
        // Zig's maps are built at run time through an allocator; there is no literal.
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", zig_expr(out, k), zig_expr(out, v)))
                .collect();
            zig_carry(out, "map literal", format!("{{ {} }}", rendered.join(", ")))
        }
        Expr::Template(parts) => {
            // A template with nothing in it but text is a string, and saying otherwise
            // would report a gap that is not there.
            if let Some(text) = literal_text(parts) {
                return quoted(Language::Zig, &text);
            }
            let rendered: Vec<String> = parts
                .iter()
                .map(|part| match part {
                    TemplatePart::Text(text) => quoted(Language::Zig, text),
                    TemplatePart::Expr(e) => zig_expr(out, e),
                })
                .collect();
            // Zig formats at run time, into a writer or an allocator, and choosing one
            // is a decision about the program.
            zig_carry(out, "interpolated string", rendered.join(" ++ "))
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
            // allocator, so this is a `for` loop over one — and which allocator is a
            // decision this cannot make.
            zig_carry(
                out,
                "comprehension",
                format!("{body} for {name} in {it}{filter}"),
            )
        }
        // Zig asks this with a tagged-union `switch`, which needs the union — and a
        // type arriving from a language with runtime classes does not have one.
        Expr::InstanceOf { value, ty } => {
            let rendered = zig_expr(out, value);
            let named = zig_expr(out, ty);
            zig_carry(out, "instanceof", format!("{rendered} instanceof {named}"));
            "false".to_string()
        }
        Expr::Await(inner) => {
            let rendered = zig_expr(out, inner);
            zig_carry(out, "await", format!("await {rendered}"))
        }
        // Zig calls positionally and has nothing that names an argument.
        Expr::Keyword { name, value } => {
            let rendered = zig_expr(out, value);
            zig_carry(out, "keyword argument", format!("{name}={rendered}"))
        }
        Expr::Unsupported(u) => {
            out.carried(u);
            out.pending.push(format!("{MARKER}: {}", u.source));
            "undefined".to_string()
        }
    }
}

/// The text of a template that interpolates nothing, if that is what this is.
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
    // How this language separates the parts of a qualified name: `::` in Rust, `.` in
    // the rest. Named for what it is, because "separator" beside a list of arguments
    // reads as the argument separator — and the Java writer was written passing `", "`
    // on that reading, turning `sync.Mutex` into `sync, Mutex`.
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
///
/// The IR holds the string's **value**; putting escapes back on is the writer's job,
/// and Rust's `{:?}` was doing it for all six. That is Rust's spelling: it writes
/// `\u{1f600}` for anything it considers non-printable, which is a syntax error in
/// Python, Java, TypeScript and Go. Only the four escapes every one of these languages
/// agrees on are written unconditionally; anything else that cannot appear literally
/// takes the target's own form.
fn quoted(language: Language, value: &str) -> String {
    format!("\"{}\"", escaped(language, value))
}

/// The inside of a string literal: the escapes, without the quotes around them.
///
/// Apart from [`quoted`] because an f-string quotes its *text* and leaves its
/// expressions as code, so the two halves cannot be escaped together.
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
            // Java has no `\xNN`, so the one form it does have is used for both.
            c if language == Language::Java => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\x{:02x}", c as u32)),
        }
    }
    out
}

/// The type this record extends, where the target can express one.
///
/// Three of these six languages have inheritance and three do not, so `inheritable`
/// says which kind of target is asking. `None` comes back either because the source
/// declared no base or because this language has none — and the second of those leaves
/// a note, since dropping it silently made `class JsonPrimitive extends JsonElement`
/// into a class that extends nothing. That is a different type, and the output said
/// nothing about it.
fn inherited_base(out: &mut Out, record: &Record, inheritable: bool) -> Option<String> {
    let base = record.extends.clone()?;
    if inheritable {
        return Some(base);
    }
    out.fidelity.notes.push(format!(
        "`{}` extends `{base}` in the source; {} has no inheritance, so whatever \
         `{base}` contributed is not here",
        record.name, out.language
    ));
    None
}

/// What this target calls a function that makes a value of `owner`.
///
/// Three of these six languages have a constructor and three have a habit. Java names
/// it after the class, Python calls it `__init__` and TypeScript calls it `constructor`;
/// Rust writes `Thing::new`, Go writes `NewThing` and Zig writes `Thing.init`. Which of
/// those a file gets is a fact about the target, which is why the IR carries *that it is
/// one* rather than what it is called.
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
///
/// Java is the one target here that overloads constructors; everywhere else a type has
/// exactly one. The rest keep the names their source gave them and are reported, since
/// a caller of `Thing(a, b)` has to be told what to write instead.
fn methods_of(out: &mut Out, record: &Record, overloads_allowed: bool) -> Vec<Function> {
    let mut seen = false;
    let mut methods = record.methods.clone();
    for method in methods.iter_mut() {
        if !method.is_constructor {
            continue;
        }
        // Whether a constructor takes a receiver, and whether it says what it returns,
        // is a fact about the target rather than about the source. Python's `__init__`
        // takes `self` and returns nothing; Java's and TypeScript's take neither; and
        // the three that spell it by habit take no receiver and return the type,
        // because returning the type is the whole of what makes them one.
        match out.language {
            // A constructor here acts on a value that already exists, so it takes the
            // receiver and says nothing about what it returns. A source whose
            // constructor had none — Rust builds and returns instead — still needs one
            // bound, or Python writes `@staticmethod def __init__(n)` and TypeScript
            // writes `static constructor(n)`.
            Language::Python | Language::Java | Language::TypeScript | Language::Tsx => {
                if method.receiver_binding.is_none() {
                    method.receiver_binding = Some(receiver_word(out.language).to_string());
                }
                method.returns = None;
            }
            // The other three have no constructor, only a habit: a plain function that
            // *returns* the type, which is the whole of what makes it one. It has no
            // receiver — and the source's body, which assigns through one, therefore has
            // nowhere to run. Saying so is the honest answer; writing `self.n = n`
            // inside a function that binds no `self` is not.
            _ => {
                if !method.body.is_empty() {
                    out.fidelity.notes.push(format!(
                        "`{}` has a constructor whose body assigns through a receiver; \
                         {} builds a value and returns it instead, so that body has no \
                         counterpart and is not here",
                        record.name, out.language
                    ));
                }
                method.receiver_binding = None;
                method.body = Vec::new();
                method.returns = Some(Type::named(record.name.clone()));
            }
        }
        if seen && !overloads_allowed {
            method.is_constructor = false;
            out.fidelity.notes.push(format!(
                "`{}` declares more than one constructor and {} allows one; this is \
                 written as an ordinary function called `{}`",
                record.name,
                out.language,
                out.name(&method.name)
            ));
        }
        seen = true;
    }
    methods
}

/// Text that can sit inside a `/* ... */` comment.
///
/// `*/` closes one, and a doc comment that quotes a glob — `app/**/route.ts` — carries
/// that sequence in the middle of a sentence. Java and TypeScript both wrote it
/// through, so the comment ended early and the rest of the sentence was parsed as
/// code: three words, two template strings and an optional chain, none of which the
/// author wrote.
fn block_comment_safe(text: &str) -> String {
    text.replace("*/", "* /")
}

/// Can this value be written twice without doing anything twice?
///
/// Python and Java can only ask "is this absent" by naming the value, so `a ?? b`
/// becomes two mentions of `a`. That is free for a name, a literal or a field read, and
/// it is a second call for anything else — which would make the program do more than it
/// did. Those are carried instead.
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
///
/// Generous on purpose: a qualified path is a name — `std::fmt::Display`, `sync.Mutex`
/// — and shortening one would point a signature at something that does not exist. What
/// this refuses is a name that is not made of name characters at all.
fn is_writable_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$' | '#'))
        && !name.starts_with(|c: char| c.is_ascii_digit())
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
        // Zig has no dynamic type; `anytype` says the caller decides, which is exactly
        // true of a parameter whose type the source never wrote down.
        Language::Zig => "anytype".to_string(),
        _ => "unknown".to_string(),
    }
}
