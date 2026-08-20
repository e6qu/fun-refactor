//! Writing the IR out as a language.
//!
//! Each writer targets idiom and not transliteration. A record becomes a Rust
//! `struct` with an `impl` block, a Python `@dataclass`, a Go `struct` with methods
//! beside it, a TypeScript `class`. Naming follows the target's convention,
//! `snake_case` in Rust and Python, `camelCase` in TypeScript, `PascalCase` for
//! exported Go.
//!
//! The signature is the contract: parameter names, order, types and return type cross
//! exactly, with only the spelling changed. A type with no counterpart goes through by
//! name and counts in the fidelity report, so no substitution passes silently.
//!
//! Whatever the reader could not translate becomes a comment holding the original
//! source, under a marker.

use super::ir::*;
use crate::lang::Language;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

/// What kind of thing a name names, since the conventions differ by kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A struct, class, interface, `PascalCase` in every one of them.
    Type,
    /// A module-level constant, `SCREAMING_SNAKE` in most of them. Go spells it like
    /// anything else, because there the capital letter means exported, and Zig does not
    /// shout at all.
    Constant,
    /// A function or method.
    ///
    /// The same as [`Kind::Value`] in every target but Zig, whose style guide splits what the
    /// others join. `camelCase` for what you call, `snake_case` for what you bind. One `Kind`
    /// for both spelled every local in a Zig file as a function.
    Function,
    /// A field, parameter or local.
    Value,
}

/// How every name this module declares is spelled in the target language.
///
/// Only its own. A name absent from this map is foreign, a library, a builtin,
/// somebody else's field, and is written as the source had it. That is the
/// whole of the safety argument. The tool renames what the file declares and nothing
/// else, which is the same rule its refactorings follow.
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
                // Zig does not shout. Its standard library writes `std.math.pi`, and
                // a capital there would say "type" and not "constant".
                Language::Zig => snake_always(name),
                _ => screaming(name),
            },
            Kind::Function => match language {
                Language::Rust => snake_always(name),
                // Python says "not for outside this module" with a leading
                // underscore. Without it, Go's unexported `half` came back
                // from a round trip as the exported `Half`, and a package's
                // internals became its API. The entry point is the exception.
                // `main` is what a reader and a runner look for, and a private
                // one reads as a helper nobody calls.
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
        // `_` is not a name, it is the word for "no name". Rust, Go, Python and Zig
        // all use it, and putting it through a convention asked what the empty word
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
                Stmt::Defer(cleanup) | Stmt::ErrDefer(cleanup) => walk_stmts(cleanup, add),
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
                // A counted `for` declares its counter in the header. Skipping
                // the header left that name out of the spelling map, so the
                // declaration and every use of it could take different casings.
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
        // A constructor's own name is never written, every target has its own word for one, so
        // it must not claim a spelling. Java names it after the class, and letting it into the
        // map meant every Java class came out named after its constructor. `class a` where the
        // source said `class A`.
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
            // A `const` bound to a literal is a constant and takes the
            // `SCREAMING_SNAKE` convention. One bound to a call is a binding that
            // happens to be immutable, `const schema = z.object({...})`. Shouting
            // its name would be wrong in Python and unstable across a round trip.
            Item::Constant(c) => {
                // A scalar takes the target's constant convention. A name the
                // author already shouts stays shouted, whatever it holds:
                // `DOCS = ["README.md"]` came out spelt `docs`. The rest keep
                // the value convention, so `routeContextSchema` still snakes
                // where the target snakes and `actions` never shouts.
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
    // Go's entry point is the one name whose spelling is load-bearing. The runtime
    // calls `main`, lowercase and niladic, and an exported `Main` is a program that
    // never starts. Python reads its own `def main` as exported, so the convention
    // wanted a capital, and `package main` came out with no entry point at all.
    if language == Language::Go
        && module.items.iter().any(
            |item| matches!(item, Item::Function(f) if f.name == "main" && f.params.is_empty()),
        )
    {
        map.remove("main");
    }
    (map, fields)
}

/// How this parameter is written here, and whether the calling convention survived.
///
/// A `*` marker is not a parameter and is never written. `*args` is exact wherever the target
/// has a variadic and a change of convention where it does not. `**kwargs` is a change of
/// convention everywhere but Python. `changed` is reported and not hidden. A caller of
/// the translated function writes the call differently, and nothing else in the output
/// would say so.
fn spell_param(out: &Out, kind: ParamKind, raw: &str, changed: &mut bool) -> Option<String> {
    // A bare `*` or `/` is punctuation standing where a parameter would go. Putting it through
    // the naming map asked what `*` is called in TypeScript. The answer, "not a name this
    // language can spell", was a true sentence about a thing that is never written down.
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
/// Only true keywords: a name that is a builtin (Go's `delete`, Python's `id`)
/// is legal to shadow and renaming it would be churn. The source language's keywords
/// are irrelevant, what matters is whether *this* file will parse.
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
/// Go lets the author choose it. This is the other half — the word to put back on,
/// and it is a fact about the target and not about the source.
fn receiver_word(language: Language) -> &'static str {
    match language {
        Language::Java | Language::TypeScript | Language::Tsx => "this",
        // Go's convention is a one- or two-letter abbreviation of the type. There is no way to
        // pick one that is guaranteed not to collide with a parameter. `self` is not a keyword
        // there and cannot collide with anything the source declared. A Go file that
        // used it would have been read as a receiver.
        _ => "self",
    }
}

/// The marker every carried-over fragment is written under.
pub const MARKER: &str = "fun-refactor: not translated";

/// The sibling this import line resolved to inside a directory sweep, if any.
///
/// The sweep resolves and the writer only spells. A resolved import with no
/// named bindings still stays a comment: a namespace or side-effect import
/// binds nothing a sibling's translation declares.
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
///
/// The two are the same module, except where a caller writes a *piece* of a file on its own.
/// The Next.js translation writes each handler body as its own module so it can indent it into
/// a decorated `def`. Spelling from the piece alone means a call to a helper declared elsewhere
/// in the same file keeps its original casing. So the declaration says
/// `verify_current_user_has_access_to_post` and the call still says
/// `verifyCurrentUserHasAccessToPost`.
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
    // Every read of it becomes the call it is in a target without properties. A name
    // that is both stays a read, and the property side keeps the mismatch visible.
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
    // A target without inheritance can still hold what a base in the same module
    // contributed. The base's own fields and methods lay flat into the extending
    // record. The supertype marker then only stands where the base is truly out
    // of reach.
    let flattened = match language {
        Language::Rust | Language::Go | Language::Zig => {
            let (module, notes) = flatten_local_bases(module);
            out.fidelity.notes.extend(notes);
            Some(module)
        }
        _ => None,
    };
    let module = flattened.as_ref().unwrap_or(module);

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
    /// Every language here has a convention and they disagree: TypeScript writes `userName`,
    /// Python writes `user_name`, Go says "exported" with a capital letter. Adopting the
    /// target's convention is most of what makes a translated file look written and not
    /// converted.
    ///
    /// It is one map, built once from the declarations and consulted at every declaration *and*
    /// every use, because the alternative, re-casing at each site with whichever helper was to
    /// hand, is how `interface User { userName }` became `class User. User_name` whose bodies
    /// still said `.userName`.
    ///
    /// A name it does not contain is **foreign** and is left as written: `db.users.find`,
    /// `NextResponse`, a library function. Re-casing those would rename somebody else's API,
    /// which is the one thing a translation must not do.
    names: BTreeMap<String, String>,
    /// Names that are not identifiers at all in any of these languages.
    ///
    /// Reported and not quietly reshaped. The replacement is a name the source
    /// never used, and every call to it has to be found by hand.
    unnameable: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// Names that had to be escaped because the target reserves them.
    ///
    /// Collected while writing and reported at the end. `select` is a name sqlmodel exports and
    /// a keyword in Go, and `select(User)` is not something Go's grammar will accept. So the
    /// file was refused outright, which gives the reader nothing. Escaping it and *saying so*
    /// gives them a draft and the one line to fix.
    escaped: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// The types this module declares.
    ///
    /// A signature mentioning one of them is complete, not "mentioning a type this
    /// tool does not know". The record is right there in the same output. Reporting
    /// the file's own records as foreign made a perfect translation confess to a
    /// problem it did not have. A fidelity report treated that way stops being read.
    declared_types: std::collections::BTreeSet<String>,
    /// Packages the Go this writer produced needs to import.
    ///
    /// `print` becomes `fmt.Println` and `.upper()` becomes `strings.ToUpper`.
    /// Discovered while the body is written, and inserted under the package clause
    /// after, because Go will not compile a file that names a package it never
    /// imported.
    go_imports: std::collections::BTreeSet<&'static str>,
    /// The distinct types this module declares, name to base.
    ///
    /// A call to one is a construction. Two targets spell that their own way:
    /// Java needs `new`, and Zig has no call syntax for a type at all.
    newtypes: std::collections::BTreeMap<String, Type>,
    /// Each declared record's field names, in order.
    ///
    /// `new Point(3, 4)` names no fields, and Rust, Go and Zig construct by
    /// naming them. When the record is declared right here and the arity
    /// matches, the positions map onto these; otherwise the construction
    /// carries.
    records: std::collections::BTreeMap<String, Vec<String>>,
    /// The module's own choices, whole, for building a variant of one.
    ///
    /// The name set in `sums` answers "is this a choice". Making a value of
    /// one needs the declaration itself, for the discriminator TypeScript
    /// writes and the field order Java's record constructor takes.
    sum_items: std::collections::BTreeMap<String, Sum>,
    /// Each hoisted variant's name in the output, keyed by (sum, variant).
    /// The declaration renamer dodges a same-named type by prefixing the
    /// sum's name. A construction site consulting the plain name then called
    /// the type the variant dodged.
    variant_spellings: std::collections::BTreeMap<(String, String), String>,
    /// Method names the module reads as data: `@property`, a TypeScript getter.
    ///
    /// In a target without the idiom the method is ordinary and its accessors
    /// become calls, and this set is how the field-access writer knows which.
    properties: std::collections::BTreeSet<String>,
    /// The names of the methods the module's records declare.
    ///
    /// A method is declared under the function convention and reached like a
    /// field, `store.total_value_cents()`. The use site spelled the name
    /// through the field table, which holds no methods. A two-word method was
    /// therefore declared in one casing and called in another.
    methods: std::collections::BTreeSet<String>,
    /// Each declared function's parameters, in order, with their defaults.
    ///
    /// A keyword argument names its parameter, and five of these languages
    /// call by position alone. When the callee is declared right here, the
    /// keywords settle into their declared positions, defaults filling any
    /// gap; otherwise the argument carries.
    functions: std::collections::BTreeMap<String, Vec<(String, Option<Expr>)>>,
    /// Text a writer could not put where the expression it replaced stood.
    ///
    /// Zig is the only target here with no block comment: `//` runs to the end of the line, so
    /// a carried fragment written beside an expression would swallow the rest of the statement,
    /// including its semicolon. It is queued here and flushed as whole-line comments above the
    /// statement, which is the only place in Zig a comment can go.
    pending: Vec<String>,
    /// The types of the names in the body being written, as the source declared them.
    ///
    /// Three operators need it, all for the same reason: the six languages agree on the
    /// spelling and disagree on the meaning. `/` truncates in four of them and produces a float
    /// in Python. `%` takes its sign from the dividend in four and from the divisor in Python;
    /// `==` compares contents in four, compares references in Java, and does not compile at all
    /// on a Zig slice.
    ///
    /// Nothing is inferred. A name whose type the source never wrote down is not in here, and
    /// the operator is written as it was.
    binding_types: std::collections::BTreeMap<String, Type>,
    /// What the record now being written declares its fields to be. A body
    /// reading one through the receiver knows as much as it does about a
    /// local.
    field_types: std::collections::BTreeMap<String, Type>,
    /// The same, for every record in view, keyed by the record's name.
    record_field_types:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, Type>>,
    /// The same, for record fields, which are a separate namespace.
    ///
    /// Not folded into `names`: a Rust `Reading { sensor }` with an exported field becomes Go's
    /// `Sensor`, while a *parameter* also called `sensor` stays lowercase. One map keyed by
    /// name alone gave the parameter the field's spelling. A field is reached through a
    /// receiver and a binding is not, so they do not share a namespace in any of these
    /// languages either.
    fields: BTreeMap<String, String>,
    /// The fields the method body being written may name without a receiver.
    ///
    /// Keyed by the source's spelling, valued by the target's. Empty outside a
    /// method, and outside one no bare name is ever a field.
    receiver_fields: BTreeMap<String, String>,
    /// The ok type of the `Result` the Go function being written returns, if it is one.
    ///
    /// Go spells that Result as its own `(T, error)` pair, and the body's `Ok`, `Err`
    /// and propagations become the returns and checks that pair means. The type lives
    /// here because a `return Err(...)` sits inside nested blocks that know nothing
    /// of the signature. Each needs the ok side's zero to return beside the error.
    go_result: Option<Type>,
    /// How many error bindings the Go body being written has introduced.
    ///
    /// `:=` needs a new name on its left, and a body with two propagated calls
    /// declared `err` twice. Numbering from the second one on keeps every check its
    /// own binding.
    go_errors: usize,
    /// Each declared function's `Result` ok type, where its return is one.
    ///
    /// A propagated call whose value the source discards still has to bind the
    /// callee's results. How many there are is a fact about the callee, and a callee
    /// whose ok side is unit returns the error alone here.
    result_returns: BTreeMap<String, Type>,
    /// The sums this module declares.
    ///
    /// A variant is reached through its type, `ParseError.Empty`, and how a target
    /// spells that reach is its own. Rust says `ParseError::Empty`. Go's marker
    /// convention has no reach at all and takes the variant's name as the message.
    sums: std::collections::BTreeSet<String>,
    /// The bindings of the catch clauses currently being written, innermost last.
    ///
    /// The canonical `str(e)` on one of these is the exception as text, and Python is
    /// the only target whose plain conversion says the message alone. TypeScript and
    /// Java reach for `.message` and `.getMessage()` instead, and this list is how
    /// their expression writers know the name is a caught error.
    catch_bindings: Vec<String>,
    /// The exception classes this TypeScript module throws or catches and the
    /// language does not have.
    ///
    /// Collected while the bodies are written, and declared once at the top so a
    /// thrown name is always a class the file declares.
    ts_exceptions: std::collections::BTreeSet<&'static str>,
    /// Did the Rust body being written ask for floor division?
    ///
    /// Rust has no integer method that rounds toward negative infinity, so this
    /// writer declares one. Set while an expression is written, read after the
    /// items are, because a helper nothing calls is dead code.
    needs_floor_div: bool,
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
            newtypes: std::collections::BTreeMap::new(),
            records: std::collections::BTreeMap::new(),
            properties: std::collections::BTreeSet::new(),
            sum_items: std::collections::BTreeMap::new(),
            variant_spellings: std::collections::BTreeMap::new(),
            methods: std::collections::BTreeSet::new(),
            functions: std::collections::BTreeMap::new(),
            pending: Vec::new(),
            binding_types: std::collections::BTreeMap::new(),
            field_types: std::collections::BTreeMap::new(),
            record_field_types: std::collections::BTreeMap::new(),
            fields: BTreeMap::new(),
            receiver_fields: BTreeMap::new(),
            go_result: None,
            go_errors: 0,
            result_returns: BTreeMap::new(),
            sums: std::collections::BTreeSet::new(),
            catch_bindings: Vec::new(),
            ts_exceptions: std::collections::BTreeSet::new(),
            needs_floor_div: false,
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
        // A TypeScript member can be named by an expression — `[Symbol.dispose]()`,
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
        // The receiver's own word is never an escape problem: it reaches here only because a
        // method body used it. It is exactly the word this target binds. Rust also refuses to
        // raw-escape `self`, so escaping it replaced a correct file with `r#self`, which is a
        // compile error.
        if spelled == receiver_word(self.language) {
            return spelled;
        }
        self.escaped.borrow_mut().insert(spelled.clone());
        match self.language {
            // Rust and Zig both have a spelling for this, and under it the
            // name stays the same identifier instead of becoming a different one.
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
    /// A constructor's *name* is not information. It is the type's name in Java, a
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
    /// declares. The alternative, never renaming a field at a use site, leaves the
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
        let spelled = match self.fields.get(raw) {
            Some(spelled) => spelled.clone(),
            // A method is reached like a field and spelled like a function. A
            // name that is a method and never a field takes the name table's
            // spelling. A name that is a field somewhere keeps the field one.
            None if self.methods.contains(raw) => self
                .names
                .get(raw)
                .cloned()
                .unwrap_or_else(|| raw.to_string()),
            None => raw.to_string(),
        };
        self.legal(spelled)
    }

    /// Spell the receiver this target's way while a method body is written.
    ///
    /// Returns what was there before, for [`Out::unbind_receiver`]. The mapping goes
    /// through the same [`Out::names`] every other rename does, so a body reaches it
    /// by the one route and not by a second rule that can drift from the first.
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
    ///
    /// Java lets a method body name a field with no receiver at all, and so does
    /// Go once the receiver is dropped. Every target here needs one written.
    /// Without this the bodies said `accounts` where the class declared
    /// `this.accounts`, and TypeScript reported every one of them.
    fn enter_method(&mut self, f: &Function) -> MethodScope {
        let bound = f.receiver_binding.clone();
        let displaced_name = bound.as_deref().map(|b| self.bind_receiver(b));
        let displaced_fields = std::mem::take(&mut self.receiver_fields);
        let displaced_types = std::mem::take(&mut self.field_types);
        if bound.is_some() {
            self.receiver_fields = self.fields_reached_bare(f);
            // `this.total / 2` asks what `total` is, and the record the method
            // belongs to says so. Without this the answer stopped at the class
            // boundary: a local divided as an integer and a field did not.
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
    ///
    /// A parameter or a local of the same name is the nearer declaration here.
    /// It wins, and the name is left as it stands.
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
    ///
    /// A field of the record whose method is being written goes through the
    /// receiver. Every other name is written as itself.
    fn value_name(&self, raw: &str) -> String {
        match self.receiver_fields.get(raw) {
            Some(field) => format!("{}.{field}", receiver_word(self.language)),
            None => self.name(raw),
        }
    }

    /// One line of output, at the current indent.
    ///
    /// Text with newlines in it becomes several lines, each indented. It arrives that
    /// way more often than it looks: a `/* ... */` comment is a single node however
    /// many lines it spans, and pushing it through whole indented the first line and
    /// left the rest hanging in column one, with only the first carrying whatever
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

    /// Add a note the first time it comes up. A loss repeated at every site
    /// reads once; fifty copies of the same sentence bury the others.
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
    ///
    /// A marker on the first line only is not a comment, it is one comment followed by
    /// whatever the rest of the lines happen to parse as. Multi-line text reaches here
    /// from every `/* ... */` in every source.
    fn comment(&self, text: &str) -> String {
        let marker = match self.language {
            Language::Python => "#",
            _ => "//",
        };
        // Zig rejects a tab inside a comment, and carried source brings the
        // indentation the other language wrote. A Go file's tabs made a Zig
        // file its own compiler would not lex.
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
///
/// A local of a field's name hides that field for the whole method here.
/// Where the declaration sits does not change the answer.
fn bound_names(body: &[Stmt], into: &mut std::collections::BTreeSet<String>) {
    for stmt in body {
        match stmt {
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
            Stmt::Defer(body) | Stmt::ErrDefer(body) => bound_names(body, into),
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
///
/// Every writer puts a statement on a line of its own. A `for` header wants
/// three of them side by side, so this catches what the writer emitted and
/// trims it. `None` when the statement needed more than a line, which no header
/// can hold.
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

/// The three clauses of a `for` header, each on its own line, for the targets
/// that write the whole header. `None` when one of them will not fit.
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
///
/// A target with no counted header says the loop longhand, with the step at the
/// foot of the body. A `continue` jumps over that step and the loop never ends.
/// An inner loop's `continue` is its own and does not count.
fn continues_here(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Continue => true,
        Stmt::If {
            then, otherwise, ..
        }
        | Stmt::IfPresent {
            then, otherwise, ..
        } => continues_here(then) || continues_here(otherwise),
        Stmt::Defer(body) | Stmt::ErrDefer(body) => continues_here(body),
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
///
/// `i := 0; i < n; i++` walks a range, and Python and Rust both write one. Said
/// that way the step belongs to the loop, so a `continue` cannot skip it. The
/// last member of the answer is the step, signed.
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
    // A body that moves the counter itself is not walking a range. Handing it
    // one would change how many passes the loop makes.
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
        | Stmt::ErrDefer(body) => assigns_to(body, name),
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

/// A field's starting value, where the target declares no such thing.
///
/// Rust and Go write fields without initializers. The value goes beside the
/// field as a comment, and is counted. A field that quietly lost its starting
/// value is a record every caller has to construct differently.
fn carried_default(out: &mut Out, field: &str, rendered: &str) {
    out.fidelity.carried_verbatim += 1;
    out.fidelity.notes.push(format!(
        "`{field}` started at `{rendered}`, and {} declares no value in a field; \
         every construction of the record has to set it",
        out.language
    ));
    let note = out.comment(&format!("{MARKER}: `{field}` started at `{rendered}`"));
    out.line(&note);
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

/// These statements as Rust text, for a carry that keeps its body.
///
/// A settle pass that walks the IR has no source text left to carry; the
/// statements are the only record. Rendering them back as Rust is the same
/// no-loss carry the source text would have been.
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
///
/// The source's own spelling where it wrote one, the derived snake case where
/// it did not. Deriving unconditionally spelled `FIdle`'s tag `f_idle` while
/// the source and every consumer said `"idle"`.
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
///
/// A name that already starts with a capital is a type, a class or an imported
/// binding in every one of these languages, and is left alone: `NextResponse.json(x)`
pub(super) fn snake_always(name: &str) -> String {
    // A separator goes before an uppercase letter only where a word starts:
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
    // becomes `MINCELSIUS` and not `minCelsius`.
    let screaming = name
        .chars()
        .all(|c| c.is_uppercase() || c == '_' || c.is_numeric());
    let source = if screaming {
        name.to_lowercase()
    } else {
        name.to_string()
    };

    // A leading underscore is Python's and Rust's word for "not for outside
    // this module", not a word boundary. Read as one, `_helper` came out
    // `Helper`, which in Go says exported: the marker inverted its own
    // meaning. Visibility travels in the IR's `exported` flag instead.
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

// ------------------------------------------------------------------------- Rust

/// What this module calls its floor-division helper.
///
/// A module of its own may already declare that name, and shadowing it would
/// change which function every call site reaches. The suffix keeps both.
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
                // A constant is evaluated at compile time here. A `todo!()` in its
                // value stops the build before anything runs, where a body's marker
                // stays a draft. A value holding anything untranslated carries whole
                // instead, name and all, and the inner markers ride along inside the
                // comment.
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
                // The type is not decoration. `RETRY_LIMIT: &str = 3` refused
                // to build; a literal says its own type, and a list of
                // literals becomes a slice. A runtime value keeps the draft
                // declaration it always had: dropping it to a comment lost the
                // entity on every round trip.
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
                    if let Some(value) = &f.default {
                        let rendered = rust_expr(out, value);
                        carried_default(out, &field_name, &rendered);
                    }
                    out.line(&format!("{field_visibility}{field_name}: {ty},"));
                }
                out.close();
                out.line("}");
                out.fidelity.records += 1;
                out.blank();

                if !r.methods.is_empty() {
                    // Rust declares methods apart from the type, which the
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

fn rust_function(out: &mut Out, f: &Function, method: bool) {
    out.binding_types = declared_bindings(f);
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
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
        // A body that writes to a field needs the receiver to be writable, and
        // `&self` is not. Rust refused every translated setter with E0594.
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
    let returns = match &f.returns {
        Some(Type::Unit) => String::new(),
        // A source that annotated nothing still hands a value back, and this
        // target has to name its type. Without one the body did not compile.
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
    rust_block(out, &f.body, f.returns.as_ref());
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
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// `var x; switch { every arm assigns x }` folded back into `let x = match ...;`.
///
/// The Zig reader lowers a value-position switch into that pair, and the other targets
/// write the pair as it stands. Rust cannot. A binding declared without a value has
/// no `Default::default()` the compiler can infer a type for. Assigning arms into a
/// plain `let` does not compile either. The pair is one `match` expression again,
/// which is also what a Rust author would have written.
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
    // Every value is collected before anything is rendered. Rendering can carry
    // notes, and a pair that turns out not to fold must leave no trace of the
    // attempt.
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
    let annotation = ty
        .as_ref()
        .map(|t| format!(": {}", rust_type(t)))
        .unwrap_or_default();
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
            Stmt::TupleAssign {
                names,
                value,
                declares,
                ..
            } => {
                let v = rust_expr(out, value);
                let bound = joined(names, |n| match declares {
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
                let s = rust_expr(out, subject);
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
                // A Rust match must be exhaustive, so the default arm is written
                // even when the source had none.
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
            Stmt::ErrDefer(cleanup) | Stmt::Defer(cleanup) => {
                // Rust has no scope-exit hook short of inventing a guard type,
                // so the body is rendered as Rust and carried as a comment. The
                // failure-path variant carries the same way; the guard it would
                // need is the same guard.
                let failure_only = matches!(stmt, Stmt::ErrDefer(_));
                let mut scratch = Out::new(out.language);
                scratch.names = out.names.clone();
                scratch.fields = out.fields.clone();
                scratch.declared_types = out.declared_types.clone();
                rust_block(&mut scratch, cleanup, None);
                let rendered = scratch.finish();
                out.carried(&Unsupported {
                    construct: match failure_only {
                        true => "errdefer".into(),
                        false => "defer".into(),
                    },
                    source: rendered.trim().to_string(),
                    line: 0,
                });
                let header = match failure_only {
                    true => out.comment(&format!(
                        "{MARKER}: an errdefer runs this when the scope fails:"
                    )),
                    false => out.comment(&format!("{MARKER}: a defer runs this at scope exit:")),
                };
                out.line(&header);
                for line in rendered.lines() {
                    let commented = out.comment(line);
                    out.line(&commented);
                }
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
            // Rust has no counted header, so the start goes before the loop and
            // the step at the foot of the body. A `continue` would skip the step.
            Stmt::CountedFor {
                init,
                condition,
                update,
                body,
                source,
                line,
            } => {
                // Rust's range counts up by one. Any other step is said with
                // `step_by` or a reversal, and neither reads as this loop did.
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
                let it = rust_expr(out, iterable);
                let bound = out.name(binding);
                out.line(&format!("for {bound} in {it} {{"));
                out.open();
                rust_block(out, body, returns);
                out.close();
                out.line("}");
            }
            Stmt::Expr(Expr::Null) => {}
            Stmt::Expr(e) => {
                let text = rust_expr(out, e);
                out.line(&format!("{text};"));
            }
            Stmt::Assert { condition, message } => {
                let c = rust_expr(out, condition);
                match message {
                    // The message is the macro's own format string, so a literal
                    // rides along with its braces doubled. Anything else has no
                    // slot there and is said above the check instead.
                    Some(Expr::Str(text)) => {
                        let literal = quoted(Language::Rust, text)
                            .replace('{', "{{")
                            .replace('}', "}}");
                        out.line(&format!("assert!({c}, {literal});"));
                    }
                    // A message that is not a literal goes in as an argument
                    // to the macro's own `{}`. The macro evaluates it only on
                    // failure. The source said the same, and the other targets
                    // already do it. Rendering it into a comment dropped the
                    // message and any effect computing it had.
                    Some(other) => {
                        let rendered = rust_expr(out, other);
                        out.line(&format!("assert!({c}, \"{{}}\", {rendered});"));
                    }
                    None => out.line(&format!("assert!({c});")),
                }
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

/// Whether a `/` operand rules out arithmetic: a string on either side.
///
/// Python's pathlib overloads `/` as path joining. `ROOT / "tools"` reached
/// the true-division repair and came out `float64(Root) / float64("tools")`,
/// which is not a number and was never a division. What it is cannot be said
/// in the target, so the writers carry it instead.
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
        // `(A,)` and not `(A)`: without the comma the parentheses are grouping.
        Type::Tuple(parts) => match parts.as_slice() {
            [one] => format!("({},)", rust_type(one)),
            _ => format!("({})", joined(parts, rust_type)),
        },
        Type::Named { name, args } => generic(name, args, "<", ">", "::", rust_type),
    }
}

/// One side of a binary expression, bracketed when the enclosing operator would bind into it.
///
/// The writers rendered `left op right` and nothing else. So a group the source wrote was a
/// group the translation lost. `(a + b) * c` came out of every one of them as `a + b * c`, and
/// `a - (b - c)` as `a - b - c`. Neither is the same number.
///
/// Brackets are decided from precedence and not copied from the source. So the result is right
/// even where the two languages disagree about binding. A group that was never needed does not
/// survive the trip either.
///
/// The right-hand side takes brackets at *equal* precedence as well, because every operator
/// here associates to the left. `a - (b - c)` needs them and `(a - b) - c` does not.
fn binary_operand(text: String, operand: &Expr, enclosing: BinaryOp, on_the_right: bool) -> String {
    let inner = match operand {
        Expr::Binary { op, .. } => op.precedence(),
        // A conditional binds looser than any operator in the table, so it always
        // needs the brackets. Everything else — a name, a literal, a call, an index,
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
/// `-(a + b)` is not `-a + b`. `!(a and b)` is not `!a and b`, which is the whole of De
/// Morgan's law and the reason it is a refactoring in its own right.
fn unary_operand(text: String, operand: &Expr) -> String {
    match operand {
        Expr::Binary { .. } | Expr::Ternary { .. } | Expr::Coalesce { .. } => {
            format!("({text})")
        }
        _ => text,
    }
}

/// The receiver of a `.field` or a `[index]`, bracketed when the reach would
/// bind into it.
///
/// The readers drop the source's brackets and the writers restore them from
/// structure. `(a == b).then(x)` written as `a == b.then(x)` is a different
/// expression in every language here. A literal receiver has its own trap:
/// Go and Rust read `2.then` as a float with a name stuck to it.
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

fn rust_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), rust_expr(out, value)))
                .collect();
            format!("{} {{ {} }}", out.name(ty), rendered.join(", "))
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
            // A sum's variant is reached through the type, and Rust spells that
            // reach `::`. A dot there would read as a field access on a type, and
            // Rust has no such thing.
            if matches!(of.as_ref(), Expr::Name(n) if out.sums.contains(n)) {
                let owner = rust_expr(out, of);
                return format!("{owner}::{}", out.name(name));
            }
            let object = receiver(rust_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the
            // parentheses does not exist in this language. A property is a
            // method, so its spelling comes from the name table and not the
            // field one.
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
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            // `Point(0, 0)` from a language whose classes are called is a
            // construction, and this struct's fields are named, so calling it
            // would not compile.
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = rust_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!("{}: {value}", out.field(field)))
                    .collect();
                return format!("{target} {{ {} }}", pairs.join(", "));
            }
            format!("{}({})", rust_expr(out, callee), rendered.join(", "))
        }
        // Floor division rounds toward negative infinity, and Rust has no
        // integer method that does. `div_euclid` keeps the remainder positive
        // instead, so it answers `-3` where the source's `7 // -2` says `-4`.
        // A block spells the floor itself and names each operand once, which
        // matters when an operand is a call.
        // Python's `/` is a float division. Rust's is the operands' own, so an
        // integer side is coerced before the operator sees it.
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
                // A written number takes the float spelling. `100 as f64`
                // compiles and reads as a repair rather than as arithmetic.
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
        Expr::Binary { op, left, right } => {
            // `n <= 0` with `n: f64`: Go and the others coerce the untyped
            // literal, Rust refuses the comparison. The binding's declared
            // type is on record, so the literal takes the spelling it needs.
            fn float_side(out: &Out, e: &Expr) -> bool {
                matches!(e, Expr::Name(n)
                    if matches!(out.binding_types.get(n.as_str()), Some(Type::Float)))
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
        // Rust puts it after; the other two put it in front.
        Expr::Await(inner) => format!("{}.await", rust_expr(out, inner)),
        Expr::Propagate(inner) => format!("{}?", rust_expr(out, inner)),
        // Rust has no universal spelling for construction: `X::new`, `X { .. }` and
        // a builder are all idiomatic and which one applies is a fact about the type.
        Expr::New { callee, args } => {
            // A construction whose arguments already name their fields is a
            // struct literal as written: `Point{}` and `Circle{Radius: n}`
            // arrive this way from Go.
            let keywords: Option<Vec<(&String, &Expr)>> = args
                .iter()
                .map(|a| match a {
                    Expr::Keyword { name, value } => Some((name, value.as_ref())),
                    _ => None,
                })
                .collect();
            if let Some(pairs) = keywords {
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
            let rendered: Vec<String> = args.iter().map(|a| rust_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = rust_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!("{}: {value}", out.field(field)))
                    .collect();
                return format!("{target} {{ {} }}", pairs.join(", "));
            }
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
        // which one applies is a fact about the type and not about the code.
        Expr::Cast { ty, value } => {
            format!("({} as {})", rust_expr(out, value), rust_expr(out, ty))
        }
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
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
            format!("|{}| {}", rendered.join(", "), rust_expr(out, body))
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
            // The carried text rides inside `todo!`'s format string, where a brace
            // from the source reads as a hole and stops the build. Doubling is how
            // Rust spells a brace that is only a brace.
            let payload = u
                .source
                .replace('"', "'")
                .replace('{', "{{")
                .replace('}', "}}");
            format!("todo!(\"{MARKER}: {payload}\")")
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
    // The guarded entry may need a module of its own. `asyncio.run` starts an async
    // main, and `sys.argv` holds the arguments a main that takes them receives.
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

    for item in &module.items {
        match item {
            // The module runs top to bottom here too. The guard is Python's own
            // idiom for "this part is the program". Written bare, the statement
            // would also run on import, and the source's entry never did.
            Item::Statement(stmt) => {
                out.line("if __name__ == \"__main__\":");
                out.open();
                match entry_function(module, stmt) {
                    // A Java `main(String[] args)` receives the program's arguments,
                    // which Python spells `sys.argv` less the interpreter's own name.
                    // An async main needs a loop to run on, which `asyncio.run`
                    // provides; called bare it would build a coroutine and drop it.
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
                // Python spells the base in parentheses after the name. TypeScript's
                // `extends Error` is this language's `Exception`: written through, the
                // class extended a name no Python file declares.
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
                    // A mutable default shared between instances is the classic
                    // Python trap, and the dataclass machinery refuses it outright.
                    // `field(default_factory=...)` builds one per instance, the
                    // same thing the source's per-instance initializer did.
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
                // An import a sweep resolved names a sibling translated beside
                // this file, so it crosses as a real import. The names take
                // this writer's spelling from the same table the declarations
                // use, and the module path becomes a relative one.
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
                // One class per variant and a union alias naming the choice. The
                // alias is the type callers write; `match` narrows by class.
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

fn python_function(out: &mut Out, f: &Function, method: bool) {
    let mut deferred_defaults: Vec<(String, Expr)> = Vec::new();
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);

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
        // Python evaluates a default once, at `def` time, in module scope.
        // The languages that evaluate per call let one parameter's default
        // read another. So `def pad(text, width = text.length + 2)` raised
        // NameError before the module finished importing. Such a default
        // becomes the sentinel idiom, computed in the body where the
        // parameters exist.
        let reads_a_parameter = p
            .default
            .as_ref()
            .is_some_and(|d| f.params.iter().any(|other| expr_reads(d, &other.name)));
        let default = match (&p.default, reads_a_parameter) {
            (Some(_), true) => " = None".to_string(),
            (Some(d), false) => format!(" = {}", python_expr(out, d)),
            (None, _) => String::new(),
        };
        // The sentinel widens the type it stands in for: `width: float = None`
        // is a lie a checker catches.
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
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// A line, preceded by anything an expression could not say where it stood.
///
/// Python has no inline comment: `#` runs to the end of the line, so a note written
/// inside a call's parentheses swallows the closing one. The note used to go only to
/// the fidelity report, which left a bare `None` sitting in the file where a value had
/// been, true in the report and a lie in the code. It goes above the statement now,
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
    for (at, stmt) in body.iter().enumerate() {
        // Whether a statement was produced is a property of the statement, asked once here and
        // not set inside each arm. An arm that forgot left a stray `raise NotImplementedError`
        // after a perfectly good body, which is how the `try` arm arrived broken. How the next
        // one would have.
        wrote |= !matches!(stmt, Stmt::Unsupported(_) | Stmt::Expr(Expr::Null));
        match stmt {
            Stmt::Return(value) => {
                // A returned Result speaks this language's own failure handling: the
                // ok value returns bare, and the Err raises. A propagated call's
                // error already goes the same way.
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
        Type::Tuple(parts) => format!("tuple[{}]", joined(parts, python_type)),
        Type::Map(k, v) => format!("dict[{}, {}]", python_type(k), python_type(v)),
        Type::Optional(inner) => format!("{} | None", python_type(inner)),
        // Python spells generics with brackets, so the arguments are kept
        // apart: `Result<(), String>` written literally is not a Python annotation.
        Type::Named { name, args } => generic(name, args, "[", "]", ".", python_type),
    }
}

/// The types this function's names hold: its parameters, and the locals it declares a type for.
///
/// Nothing is inferred. A binding whose type the source never wrote down is not in here. An
/// operator involving it is written the way it was, the tool does not know what it is operating
/// on. Guessing would be the same mistake in the other direction.
fn declared_bindings(f: &Function) -> std::collections::BTreeMap<String, Type> {
    let mut types = std::collections::BTreeMap::new();
    for p in &f.params {
        if let Some(ty) = &p.ty {
            types.insert(p.name.clone(), ty.clone());
        }
    }
    // The whole body, nested blocks included. An `int v = ...` inside a `try` is as
    // declared as one at the top. Reading only the top level left every such binding
    // untyped for the operators that ask.
    fn walk(stmts: &[Stmt], types: &mut std::collections::BTreeMap<String, Type>) {
        for stmt in stmts {
            match stmt {
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
                Stmt::While { body, .. }
                | Stmt::WhilePresent { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForEachIndexed { body, .. }
                | Stmt::Defer(body)
                | Stmt::ErrDefer(body) => walk(body, types),
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
            | Stmt::ErrDefer(body) => each_stmt(body, visit),
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
///
/// `out = []` says nothing about its elements. The targets that must name a
/// type therefore wrote their word for "no idea". Go produced `out := []any{}`
/// under a signature promising `[]Point`, and the compiler refused the return.
/// What the body appends says what the list holds, and the declared return type
/// says it where nothing is appended.
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
            // Nothing was appended in a shape this can read. A function that
            // returns the list has already said what it holds.
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
        Expr::Int(_) => Some(Type::Int),
        Expr::Float(_) => Some(Type::Float),
        Expr::Str(_) => Some(Type::String),
        Expr::Bool(_) => Some(Type::Bool),
        // A bare name in a method body is a local, a parameter, or a field of
        // the record the method belongs to. The writer decides which when it
        // spells it, and the type question has the same three places to look.
        // Stopping at the first two left `this.total / 2` untruncated, while
        // the same division over a local was right.
        Expr::Name(name) => out
            .binding_types
            .get(name)
            .or_else(|| out.field_types.get(name))
            .cloned(),
        // `+` with a string on either side is concatenation, and the whole of it
        // is a string however the other side is typed. Answering "no idea" here
        // left `"x" + 1 + 2` as `"x" + str(1) + 2`, which raises. Only the first
        // number ever got its coercion.
        // The canonical builtins the readers settle on: their answers have
        // one type each, whichever language wrote the call. Without this a
        // function whose whole body is `return len(items)` had no type to
        // name. The targets that must name one wrote their word for "no idea"
        // over a number.
        Expr::Call { callee, args } => match (callee.as_ref(), args.len()) {
            (Expr::Name(name), 1) if name == "len" => Some(Type::Int),
            (Expr::Name(name), 1) if name == "str" => Some(Type::String),
            (Expr::Name(name), 1) if name == "int" => Some(Type::Int),
            (Expr::Name(name), 1) if name == "float" => Some(Type::Float),
            (Expr::Name(name), 1) if name == "bool" => Some(Type::Bool),
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
        // Arithmetic keeps the type of its operands where both agree. Division is
        // deliberately absent: in Python it is the one operation that does not.
        Expr::Binary {
            op: BinaryOp::Sub | BinaryOp::Mul | BinaryOp::FloorDiv | BinaryOp::Rem,
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
///
/// Either, not both: comparing a declared string with something whose type nobody
/// wrote down is still a string comparison, and Java still gets it wrong.
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
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            // A member reached through `super` is reached through the call this
            // language spells the reach with.
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
            // Python compares against None with `is`, not `==`. Both work; only one
            // is what a Python programmer writes, and idiom is the point here.
            let against_none = matches!(**right, Expr::Null) || matches!(**left, Expr::Null);
            let spelling = match (op, against_none) {
                (BinaryOp::Eq, true) => "is",
                (BinaryOp::Ne, true) => "is not",
                (other, _) => other.python(),
            };
            // Every other language here truncates when it divides two integers. Python's `/`
            // gives a float and its `//` floors. So `7 / 2` and `-7 / 2` are 3.5 and -3.5 where
            // the source meant 3 and -3. `int(a / b)` truncates toward zero, which the
            // source said, at float precision, which is every integer a program of this kind
            // divides. `%` has the same disagreement and no readable Python form: every other
            // language here takes the sign from the dividend, Python from the divisor. So `-7 %
            // 2` is -1 there and 1 here. Writing it exactly means `a - b * int(a / b)`, which
            // is arithmetic nobody would read twice. So the idiomatic operator is kept and the
            // difference is reported instead of being left for someone to find with a negative
            // number.
            if *op == BinaryOp::Rem && holds_an_integer(out, left) && holds_an_integer(out, right) {
                let rendered = format!(
                    "{} % {}",
                    binary_operand(python_expr(out, left), left, *op, false),
                    binary_operand(python_expr(out, right), right, *op, true)
                );
                let note = format!(
                    "`{rendered}` takes its sign from the divisor in Python and from the \
                     dividend in the source language, so the two differ whenever the \
                     operands have different signs"
                );
                if !out.fidelity.notes.contains(&note) {
                    out.fidelity.notes.push(note);
                }
                return rendered;
            }
            if *op == BinaryOp::Div && holds_an_integer(out, left) && holds_an_integer(out, right) {
                return format!(
                    "int({} / {})",
                    binary_operand(python_expr(out, left), left, *op, false),
                    binary_operand(python_expr(out, right), right, *op, true)
                );
            }
            // `"n: " + x` concatenates in TypeScript and Java by turning the number
            // into text; Python raises instead. `str` says the coercion out loud,
            // which builds the same string the source built. Only where the source
            // declared both sides: wrapping a value of unknown type would be a guess.
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
                "a `?`/`try` is written as the bare expression: an error here \
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
        Expr::MapLit(entries) => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", python_expr(out, k), python_expr(out, v)))
                .collect();
            format!("{{{}}}", rendered.join(", "))
        }
        // An f-string quotes its text and leaves its expressions as code. Escaping the
        // assembled body as one string put a backslash in front of every quote inside `{...}`.
        // `f"{x.replace(\"-\", \" \")}"` is not a string Python reads.
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
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
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
                // Go's `const` holds compile-time scalars and nothing else:
                // `const Docs = []any{…}` and `const Root = Path(…)` both
                // refuse to build. Anything beyond a scalar literal is a `var`,
                // which Go initialises at start-up.
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
                    if let Some(value) = &f.default {
                        let rendered = go_expr(out, value);
                        carried_default(out, &field_name, &rendered);
                    }
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
                go_block(out, body, None);
                out.close();
                out.line("}");
                out.fidelity.functions += 1;
                out.blank();
            }
            Item::Sum(s) => {
                // Go has no closed choice. The convention that stands in for one
                // is an interface with an unexported marker method. Only the types
                // declared here can implement it, so the choice is as closed as Go
                // can spell.
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

    // The packages the body turned out to need, inserted under the package
    // clause, where Go requires them, after the body said which they are.
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
///
/// `package main` without a `func main` is a program with no entry point, and
/// `go build` refuses it: every translated library came out unbuildable. A
/// module carrying an entry point is `main`, and everything else takes its name
/// from the file, lowercased and stripped to the letters Go accepts.
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
    // A name starting with a digit is not an identifier, and an empty one is no
    // name at all. Both fall back to a word Go accepts.
    match named.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => named,
        _ => "translated".to_string(),
    }
}

/// The field names to construct this callee's record with, when a positional
/// construction can be mapped onto them. The callee must name a record declared
/// in this module, and the argument count must be the field count.
fn positional_record(out: &Out, callee: &Expr, arity: usize) -> Option<Vec<String>> {
    let Expr::Name(name) = callee else {
        return None;
    };
    out.records
        .get(name)
        .filter(|fields| fields.len() == arity && arity > 0)
        .cloned()
}

/// The call's arguments settled into their declared positions, when its
/// keywords can be. The callee must be a function declared in this module.
/// Each keyword must name one of its parameters, and every position left
/// unfilled must have a declared default to fill it.
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

/// Does this body leave on its own, making a `break` after it one statement too
/// many? Java refuses unreachable code outright, so the answer decides whether
/// the `case` gets one.
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let scope = out.enter_method(f);
    // What the body declared, for the return type a Python source never wrote.
    out.binding_types = declared_bindings(f);

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
    // A declared `Result<T, E>` is Go's own `(T, error)` pair, and the body's `Ok`,
    // `Err` and propagations become the returns and checks that pair means. A unit ok
    // side leaves the error alone, which is how Go spells a function that can only
    // fail. The error type's own identity does not carry: Go's `error` is the message.
    let result = result_ok(&out.declared_types, f.returns.as_ref());
    let returns = if let Some(ok) = &result {
        if out.is_foreign(ok) {
            foreign = true;
        }
        out.note_once(
            "a Result's error side is written as Go's own error: the error's identity \
             becomes its message.",
        );
        match ok {
            Type::Unit => " error".to_string(),
            ok => format!(" ({}, error)", go_type(ok)),
        }
    } else {
        match &f.returns {
            Some(Type::Unit) => String::new(),
            // A source that annotated nothing still hands a value back, and Go
            // has to name its type. Without one the body did not compile.
            None if returns_a_value(f) => {
                unannotated = true;
                let ty = match inferred_return(out, f) {
                    Some(ty) => go_type(&ty),
                    None => unknown(out, &f.name),
                };
                format!(" {ty}")
            }
            None => String::new(),
            // `(int, error)`: the one position Go writes several types at once.
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
    // Go refuses a function that promises a value and has no path returning one.
    // A body whose statements did not translate reaches here, and the file that
    // carried them as comments would not build. Panicking says the draft is not
    // finished where a zero value would have said it was.
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
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// An `Err(...)` payload as the error value Go returns.
///
/// A formatted payload is `fmt.Errorf`, a literal is `errors.New`, and anything else
/// is rendered through `%v`, which makes the value the message whatever it was.
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
        // A sum's variant as the failure, `ParseError.Empty`: Go's error has no
        // variants, so the variant's name becomes the message. Zig's own
        // `@errorName` answers the same.
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
///
/// `return Ok(x)` is `return x, nil`, and `return Err(e)` is the ok side's zero
/// beside an error. A propagated call binds its value next to an `err` that is
/// checked and returned. Only inside a function whose declared return is the shared
/// Result. Anywhere else `Ok` is a name like any other, and the statement falls
/// through to the ordinary arms.
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
            // The Zig reader hands `_ = try f();` over as an assignment to the blank
            // name.
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
///
/// Go's own rule for a terminating statement, in the shapes the IR has. Judging
/// only a trailing `return` put a `panic` after a switch whose every arm
/// returned, which is unreachable code and a defect of its own.
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
        // Otherwise every path out of the attempt has to leave on its own.
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
    for stmt in body {
        if go_result_stmt(out, stmt) {
            continue;
        }
        match stmt {
            // Go has no ternary. One that is the whole of a return, an
            // assignment, or a typed binding is an `if`/`else` said shorter, so
            // Go writes the `if`/`else`. Each arm renders inside its own
            // branch, which keeps the evaluation the source chose.
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
                    Some(v) => format!(" {}", go_expr(out, v)),
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
                // `[]any{}` under a signature promising `[]Point` is a file Go
                // refuses. Where the element type settled, the literal names it.
                let v = match (value, out.binding_types.get(name)) {
                    (Expr::ListLit(items), Some(ty)) if items.is_empty() => {
                        format!("{}{{}}", go_type(ty))
                    }
                    _ => go_expr(out, value),
                };
                let bound = out.name(name);
                out.line(&format!("{bound} := {v}"));
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
            Stmt::IfPresent {
                binding,
                value,
                then,
                otherwise,
            } => {
                // The optional is a pointer here, and the payload is one
                // dereference away. A second binding holds the pointer, so the
                // body reads the name the source bound.
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
            // Go has no failure path a block can watch, so the failure-only
            // cleanup is carried, visibly, rather than run on every exit.
            Stmt::ErrDefer(_) => {
                out.fidelity.carried_verbatim += 1;
                out.fidelity
                    .notes
                    .push("on the failure path: errdefer carried over unchanged".to_string());
                let header = out.comment(&format!(
                    "{MARKER}: an errdefer runs this when the scope fails"
                ));
                out.line(&header);
            }
            Stmt::WhilePresent {
                binding,
                value,
                body,
            } => {
                let v = go_expr(out, value);
                let bound = out.name(binding);
                out.line("for {");
                out.open();
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
                let parts = counted_header(
                    out,
                    init.as_deref(),
                    condition.as_ref(),
                    update.as_deref(),
                    &|out, stmt| go_block(out, std::slice::from_ref(stmt), None),
                    &|out, e| go_expr(out, e),
                );
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
                // Only a call can stand as a statement in Go. Everything else, which
                // is mostly a carried marker, is discarded into `_`, the same thing
                // the Zig writer does for the same reason.
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
        // handles. Everywhere else, a field, a generic argument. It needs a type,
        // and `Result[, string]` is not one.
        Type::Unit => "struct{}".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Float => "float64".to_string(),
        Type::String => "string".to_string(),
        Type::List(inner) => format!("[]{}", go_type(inner)),
        Type::Map(k, v) => format!("map[{}]{}", go_type(k), go_type(v)),
        Type::Optional(inner) => format!("*{}", go_type(inner)),
        // Go can only say several-types-as-one in a function's results, and the
        // signature writer spells that itself. In any other position, a field or
        // an argument, the name will not compile, which is honest. `[](A, B)` from
        // a Rust `Vec<(A, B)>` field did not compile either, unreadably.
        Type::Tuple(parts) => format!("Unwritable_tuple_{}", parts.len()),
        Type::Named { name, args } => go_named(name, args),
    }
}

/// A type carried across by name, with the one qualifier Go allows.
///
/// Go names a foreign type `package.Name` and has no third level: `crate.model.Symbol` is not a
/// type there, it is a field of a field. Every signature mentioning one failed to parse. The
/// last two segments are what a Go author would write after importing that package, which the
/// header already lists. So the path is shortened and not flattened, and the name itself is
/// untouched.
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
        // The zero of several results is each one's zero, which only a return can say.
        Type::Tuple(parts) => joined(parts, go_zero),
        Type::Named { name, .. } => format!("{}{{}}", go_named(name, &[])),
    }
}

fn go_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // Go names its fields in a literal, in any order, like the source did.
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), go_expr(out, value)))
                .collect();
            format!("{}{{{}}}", out.name(ty), rendered.join(", "))
        }
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
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go is the one language here with no conditional expression. Turning one into an `if`
        // statement needs somewhere to put the result, which does not exist inside an argument
        // list.
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
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
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
            // A read of a property is a call here; the idiom that hid the
            // parentheses does not exist in this language. A property is a
            // method, so its spelling comes from the name table and not the
            // field one.
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
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            // A call to a declared record is a construction; Go's conversion
            // syntax `Point(x)` means something else entirely.
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = go_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!("{}: {value}", out.field(field)))
                    .collect();
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
        // Go has no `await`. Writing the inner expression alone would turn a suspension point
        // into a plain call, which is the kind of silent change this exists to avoid. So it is
        // carried like any other construct with no counterpart.
        Expr::Await(inner) => {
            let source = format!("await {}", go_expr(out, inner));
            out.carried(&Unsupported {
                construct: "await".into(),
                source: source.clone(),
                line: 0,
            });
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go propagates nothing: an error is a value somebody must return. Writing
        // the expression bare would turn an early return into a plain call.
        Expr::Propagate(inner) => {
            let source = format!("{}?", go_expr(out, inner));
            out.carried(&Unsupported {
                construct: "error propagation".into(),
                source: source.clone(),
                line: 0,
            });
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // `NewThing(..)` is the Go convention, but it is a convention and not a rule. A
        // constructor this tool invented would be a name that does not exist. A
        // record declared right here is different: its fields are known, and a
        // positional construction maps onto them.
        Expr::New { callee, args } => {
            let rendered: Vec<String> = args.iter().map(|a| go_expr(out, a)).collect();
            if let Some(fields) = positional_record(out, callee, args.len()) {
                let target = go_expr(out, callee);
                let pairs: Vec<String> = fields
                    .iter()
                    .zip(rendered.iter())
                    .map(|(field, value)| format!("{}: {value}", out.field(field)))
                    .collect();
                return format!("{target}{{{}}}", pairs.join(", "));
            }
            let target = go_expr(out, callee);
            let source = format!("new {target}({})", rendered.join(", "));
            out.carried(&Unsupported {
                construct: "new".into(),
                source: source.clone(),
                line: 0,
            });
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        // Go spells this as a two-value type assertion, which is a statement.
        Expr::Cast { ty, value } => {
            format!("{}({})", go_expr(out, ty), go_expr(out, value))
        }
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
            format!("any(nil) /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
        Expr::Unary { op, operand } => {
            let sign = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
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
        // Outside a return Go has no way to say several-values-as-one, and the return
        // is handled where returns are written. What lands here is carried, visibly.
        // An element may carry a marker of its own, and comments do not nest.
        Expr::Tuple(items) => {
            let rendered = joined(items, |i| go_expr(out, i)).replace("*/", "* /");
            out.fidelity.carried_verbatim += 1;
            out.fidelity
                .notes
                .push("outside a return: tuple carried over unchanged".to_string());
            format!("any(nil) /* {MARKER}: tuple ({rendered}) */")
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
        // Go writes no closure without every type spelled, and the source
        // spelled none; inventing them would be a guess about the call sites.
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
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
            // The module runs top to bottom here too, so the statement is one of its own.
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
                // A record with no methods is an interface: it is data, and an
                // interface is what TypeScript calls that. A field that starts
                // at a value needs a class, because an interface declares types
                // and holds no initializer to declare it in.
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
            Item::Import { text, line, target } => {
                // An import a sweep resolved names a sibling translated beside
                // this file, so it crosses as a real import. The names take
                // this writer's spelling from the same table the declarations
                // use, and the module path points at the sibling's stem.
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
                    "a test crossed as a plain function: no runner is part of the \
                     language, so wiring it into yours is left to you.",
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
                // One object type per variant, told apart by a literal field, and a
                // union alias naming the choice. The literal is what lets the
                // checker narrow a `switch` on it.
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

    // The exception classes the bodies turned out to throw or catch, declared once
    // at the top. Every thrown name is then a class the file has. Hoisting after the
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
///
/// Python's exception bases name classes TypeScript does not have. A class
/// extending `Exception` or one of its everyday complaints is extending
/// `Error` here. Written through, the base named a class no TypeScript file
/// declares. `ABC` names no base: it is Python's word for "abstract".
/// TypeScript has no counterpart, so that base is dropped and said, and the
/// methods stay.
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
                 base classes, so the base is dropped and the methods stay.",
                record.name
            ));
            None
        }
        _ => Some(base),
    }
}

/// A canonical exception name, as TypeScript spells it, noted for declaration.
///
/// Python's `Exception` is the base `Error` here and `TypeError` exists natively; the
/// rest keep their names over one-line classes this writer declares. A name the module
/// declares itself is the module's own and is left alone.
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
///
/// Python spells raising as a call, so `raise ValueError(m)` arrives as one; written
/// through, the output called a class without `new` and TypeScript refused it.
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

/// What each variant is called where it must live beside every other type.
///
/// Rust and Zig keep variants inside the enum, so they never collide with the
/// module's other types. The flat-namespace targets hoist each variant to the top
/// level. Rust lets a file declare a struct `Hierarchy` and an enum with a
/// `Hierarchy` variant, and hoisting both would emit two types with one name. The
/// variant steps aside, prefixed with its sum's name, and the rename is in the notes.
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
                "variant `{}` of `{}` is written as `{renamed}`: the file already \
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
///
/// `kind` is the convention. A variant that already has a field spelled `kind`
/// would collide with it, so the next free word steps in. Numbering takes over
/// in the unlikely file where every word is taken.
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

/// `inside_class` is where it is written, which is not the same question as whether it takes a
/// receiver, a class holds `static empty()` beside `label()`. One `bool` answering both put
/// `export function` inside a class body.
fn ts_function(out: &mut Out, f: &Function, inside_class: bool) {
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let scope = out.enter_method(f);
    // TypeScript has one number type, so `/` needs to know which of them the
    // source declared: an integer division truncates and this one does not.
    out.binding_types = declared_bindings(f);

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
                // A binding whose initializer carried whole keeps its name, so
                // the statements after it still parse. `any` keeps the
                // declaration compiling under strict when no type survived to
                // say more.
                let annotation = match (ty, value) {
                    (Some(t), _) => format!(": {}", ts_type(t)),
                    (None, Some(Expr::Unsupported(_))) => ": any".to_string(),
                    (None, _) => String::new(),
                };
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
                // TypeScript has one catch clause and no types on it. Python's typed `except`s
                // become `instanceof` tests inside it, which is how the same intent is written
                // here. The trailing `throw` keeps an unmatched error propagating instead of
                // swallowing it.
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
                let rendered = ts_thrown(out, value).unwrap_or_else(|| ts_expr(out, value));
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
        Type::Tuple(parts) => format!("[{}]", joined(parts, ts_type)),
        Type::Named { name, args } => generic(name, args, "<", ">", ".", ts_type),
    }
}

fn ts_expr(out: &mut Out, e: &Expr) -> String {
    match e {
        // TypeScript and Java build a record by calling a constructor, which takes its
        // arguments in an order the class decides. This writer emits a class with fields and no
        // constructor. So there is no order to put them in. One assembled from the source's
        // declaration order would be a fact about the source and not about anything a caller
        // will call.
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), ts_expr(out, value)))
                .collect();
            let source = format!("{} {{ {} }}", out.name(ty), rendered.join(", "));
            out.carried(&Unsupported {
                construct: "building a record by naming its fields, which needs a \
                            constructor here"
                    .into(),
                source: source.clone(),
                line: 0,
            });
            format!("null /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
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
        // `super` is the base reached, this language's own keyword, and not a
        // name to re-case or escape. `super(m)` and `super.m()` fall out of the
        // ordinary call and field arms once the word survives.
        Expr::Name(n) if n == "super" && !shadows_builtin(out, "super") => "super".to_string(),
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
        Expr::Binary { op, left, right } => {
            // TypeScript has one number type and `/` on it never truncates, so
            // `half(7)` answered 3.5 where the source said 3. Java and the rest
            // truncate toward zero, which is `Math.trunc` and not `Math.floor`:
            // the two disagree on every negative quotient.
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
                "a `?`/`try` is written as the bare expression: an error here \
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
        // A tuple's value in TypeScript is an array; only its type is written apart.
        Expr::Tuple(items) => format!("[{}]", joined(items, |i| ts_expr(out, i))),
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
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
            let value = ts_expr(out, body);
            // An object literal standing bare after `=>` reads as a block, so it
            // takes the brackets that keep it a value.
            let value = match body.as_ref() {
                Expr::MapLit(_) => format!("({value})"),
                _ => value,
            };
            format!("({}) => {value}", rendered.join(", "))
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
            // `[x for x in xs if p(x)]` keeps every element it selects. So the map is the
            // identity and writing it out says nothing: `xs.filter(p).map((x) => x)` is
            // `xs.filter(p)` with three extra words.
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
/// That is the one structural thing this writer does that no other does. Every other target
/// takes the module's items and writes them out; Java has to invent a container for them. A
/// public class must be named after its file. So [`Module`] carries a name at all.
///
/// A record with methods becomes its own class beside it. The loose functions and constants
/// become `static` members of the file's class, because that is what a Java file full of free
/// functions has to be.
fn java(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("// {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    // `List`, `Map` and `Optional` are the three names this writer reaches for that Java does
    // not have in scope. It emitted them and never emitted an import. So a signature the report
    // called "carried across with its types intact" named a type the file had never heard of.
    // `Objects` is written out in full at its use site and needs nothing here.
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

    // **One public class per file**, named after the file. That is not a convention in Java, it
    // is a rule the compiler enforces. It is the whole reason this writer has to make a choice
    // no other one does. A module that is only a record gives its name to the file. A module
    // with loose functions keeps the file's own class public and writes its records as
    // package-private siblings beside it.
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
                    "a test crossed as a plain method: no runner is part of the \
                     language. Wiring it into yours is left to you.",
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
        // Java starts a field where the declaration says. Dropping the value
        // left the field null, and the first method to reach it threw.
        let default = field
            .default
            .as_ref()
            .map(|d| format!(" = {}", java_expr(out, d)))
            .unwrap_or_default();
        out.line(&format!("{field_visibility} {ty} {field_name}{default};"));
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

/// A sum as Java spells one: a sealed interface over records, one per variant.
///
/// The `permits` clause could be omitted with everything in one file, and is written
/// anyway because it states the closed set in as many words.
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);

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
                    unknown(out, &p.name)
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

    // The runtime looks for a `public static void main(String[])` and starts
    // nothing else. Whether the source's entry was exported is a fact about the
    // source: Go's `main` is lower-case and Python's is a plain function. A
    // private one here answers "Main method not found in class".
    let entry = is_static && !f.is_constructor && f.name == "main";
    let visibility = if f.exported || entry {
        "public"
    } else {
        "private"
    };
    // A constructor writes no return type at all. `void` would make it a method that
    // happens to have the class's name, which compiles, and is not a constructor.
    let returns = match f.is_constructor {
        true => String::new(),
        false => format!("{returns} "),
    };
    let modifier = if is_static && !f.is_constructor {
        " static "
    } else {
        " "
    };
    // A niladic `main` runs only on the JDKs that finalised instance main
    // methods. A draft that ran here died on the ordinary ones. The parameter
    // is written whether or not the source's entry took one.
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
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

fn java_block(out: &mut Out, body: &[Stmt], returns: Option<&Type>) {
    if body.is_empty() {
        // A method that returns something must return something; one that does not can be
        // empty. An invented value would be a guess at what the body did.
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
        // `errdefer` runs only on the failure path: clean up and rethrow. The
        // unchecked catch keeps the enclosing signature's `throws` honest.
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
        Stmt::Comment(text) => {
            let line = out.comment(text);
            out.line(&line);
        }
        Stmt::Return(value) => {
            // A returned Result speaks this language's own failure handling: the ok
            // value returns bare, and the Err becomes a throw. `RuntimeException` is
            // the unchecked kind, because this writer declares no `throws` clauses.
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
            // writing a type it has not got would be a guess. A binding whose
            // initializer is `null`, because it carried whole or was never there,
            // gives `var` nothing to infer, so it is declared `Object`.
            let declared = ty.as_ref().map(java_type).unwrap_or_else(|| match value {
                Some(Expr::Unsupported(_)) | None => "Object".to_string(),
                _ => "var".to_string(),
            });
            let bound = out.name(name);
            out.line(&format!("{declared} {bound} = {rendered};"));
        }
        Stmt::Assign { target, value } => {
            // `d[k] = v` is `d.put(k, v)` here. Java has no assignable subscript on a
            // collection. `d.get(k) = v`, which rendering the target as an expression
            // produces, is not a statement in the language at all.
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
        // Java has no tuple and no multiple return, so there is no line to
        // write. Carried whole, the names it binds are at least in the file.
        Stmt::TupleAssign { source, line, .. } => carry(
            out,
            &Unsupported {
                construct: "multiple assignment".to_string(),
                source: source.clone(),
                line: *line,
            },
        ),
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
            // The optional cannot unwrap in place, so a second binding holds it
            // and the branch takes the payload out under the name the source
            // bound.
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
            let s = java_expr(out, subject);
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
///
/// Read from the IR and not from the finished text, because an import has to be written before
/// the class that uses it. Because a `List` inside a string literal is not a use.
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
        // instead of a nullable annotation.
        Type::Optional(inner) => format!("Optional<{}>", java_boxed(inner)),
        // Java has no tuple type. The name will not compile, which is the point:
        // an invented Pair class would compile and claim a shape the source never had.
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
        Expr::RecordLit { ty, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(name, value)| format!("{}: {}", out.field(name), java_expr(out, value)))
                .collect();
            let source = format!("{} {{ {} }}", out.name(ty), rendered.join(", "));
            out.carried(&Unsupported {
                construct: "building a record by naming its fields, which needs a \
                            constructor here"
                    .into(),
                source: source.clone(),
                line: 0,
            });
            format!("null /* {MARKER}: {} */", source.replace("*/", "* /"))
        }
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
        // `super` is the base reached, this language's own keyword, and not a
        // name to re-case or escape. `super(m)` and `super.m()` fall out of the
        // ordinary call and field arms once the word survives.
        Expr::Name(n) if n == "super" && !shadows_builtin(out, "super") => "super".to_string(),
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            let object = receiver(java_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the
            // parentheses does not exist in this language. A property is a
            // method, so its spelling comes from the name table and not the
            // field one.
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
            if let Some(mapped) = java_builtin(out, callee, args) {
                return mapped;
            }
            let settled = resolve_keywords(out, callee, args);
            let args: &[Expr] = settled.as_deref().unwrap_or(args);
            let rendered: Vec<String> = args.iter().map(|a| java_expr(out, a)).collect();
            if let Expr::Name(name) = callee.as_ref() {
                if out.newtypes.contains_key(name) {
                    return format!("new {}({})", out.name(name), rendered.join(", "));
                }
            }
            // Java has no callable values: `f()()` and a called lambda are not in
            // the grammar, however the value arrived. Such a call carries whole.
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
        Expr::Binary { op, left, right } => {
            // `==` on a Java String compares references. Every other language here compares
            // contents, and so did the source. So the translation of `a == b` was a different
            // question with the same spelling, quietly false for two equal strings that were
            // built and not interned.
            //
            // `Objects.equals` and not `a.equals(b)`: it answers for null on either side, which
            // `a.equals(b)` throws on. Written out in full because this writer emits no
            // imports.
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
        // Java has no tuple value. `List.of` would erase the types and claim a
        // collection the source never had, so the tuple is carried, visibly.
        // An element may carry a marker of its own, and comments do not nest.
        Expr::Tuple(items) => {
            let rendered = joined(items, |i| java_expr(out, i)).replace("*/", "* /");
            out.fidelity.carried_verbatim += 1;
            out.fidelity
                .notes
                .push("no tuple here: tuple carried over unchanged".to_string());
            format!("null /* {MARKER}: tuple ({rendered}) */")
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
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
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
        Expr::Propagate(inner) => {
            out.note_once(
                "a `?`/`try` is written as the bare expression: an exception here \
                 propagates on its own.",
            );
            java_expr(out, inner)
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
/// Two facts about the language shape this writer. **A type is a value**: a struct is what a
/// `const` is bound to. So a record is written `const Reading = struct { … };` and its methods
/// live inside it and not beside it. And **there is no block comment**, `//` runs to the end of
/// the line. So a carried-over fragment cannot be written beside the expression it replaced. It
/// goes above the statement instead, via [`Out::pending`].
///
/// What has no counterpart and is carried and not guessed at: `new`, `await`, `throw`,
/// `try`/`catch`, map literals, interpolated strings and comprehensions. Zig models failure in
/// the return type and removed `async` in 0.11, so an exception or a suspension point arriving
/// from another language has nowhere to land. Inventing an error set would be inventing the
/// program's vocabulary of failures.
fn zig(out: &mut Out, module: &Module) {
    for line in &module.doc {
        out.line(&format!("//! {line}"));
    }
    if !module.doc.is_empty() {
        out.blank();
    }

    // Zig reaches its standard library through a binding the file has to make. Nothing
    // here emitted one, so `std.mem.eql`, the only way to compare two strings in this
    // language, named something the file had never heard of.
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
                // `const Foo = struct {};` is valid Zig and the grammar this tool reads with
                // cannot parse it. So the output would be refused by its own check. An empty
                // `comptime` block does nothing, says so, and both Zig and the grammar accept
                // it. Recorded in BUGS.md as upstream.
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
                    // Zig has the syntax: `count: usize = 0,`.
                    let default = f
                        .default
                        .as_ref()
                        .map(|d| format!(" = {}", zig_expr(out, d)))
                        .unwrap_or_default();
                    out.line(&format!("{field_name}: {ty}{default},"));
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
                        // A single field named for its value is the payload itself;
                        // wrapping it in a one-field struct would make every use
                        // site say `.value.value`.
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
}

/// Will this module's Zig need `std`?
///
/// Asked of the IR and not of the finished text, because the binding has to be
/// written before the code that uses it. String equality is the only thing that
/// reaches for `std` today; a second one belongs in this list beside it.
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
            Stmt::Defer(body) | Stmt::ErrDefer(body) => body.iter().any(|s| in_stmt(types, s)),
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
    // The source's word for the receiver, spelled this target's way for as long as
    // this body is being written. Outside a method there is nothing to bind.
    let scope = out.enter_method(f);
    out.binding_types = declared_bindings(f);

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
    // A method takes its own type as an ordinary first parameter; there is no
    // receiver syntax to put it in.
    //
    // By pointer when the body assigns through it. A Zig value parameter is const, so
    // `self: Counter` with `self.value = …` in the body is not a slow method. It is a
    // file that does not compile, from a source that said `&mut self` and a report that
    // said every signature carried across.
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
        // Zig writes a type on every parameter and infers none. One the source
        // never declared becomes `anytype`, and the signature counts as
        // unannotated rather than complete.
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
        // `void` over a body that returns a value does not compile, and a
        // source that annotates nothing still returns one.
        None if returns_a_value(f) => {
            unannotated = true;
            match inferred_return(out, f) {
                Some(ty) => zig_type(&ty),
                None => unknown(out, &f.name),
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

    let visibility = if f.exported { "pub " } else { "" };
    out.line(&format!(
        "{visibility}fn {}({}) {returns} {{",
        out.function_name(f),
        params.join(", ")
    ));
    out.open();
    // Zig rejects a `var` nothing writes to, so which keyword a binding takes is a fact about
    // the rest of the body and not about the binding. Only the Rust reader records mutability
    // at all; every other one says "mutable" because it has nothing better to say. Taking that
    // at its word made a `const` file into a `var` one that will not build.
    let mutated = zig_mutated(&f.body);
    zig_block(out, &f.body, f.returns.as_ref(), &mutated);
    out.close();
    out.line("}");

    out.leave_method(scope);
    out.fidelity.functions += 1;
    if changed {
        out.fidelity.signatures_with_changed_calls += 1;
    }
    if foreign {
        out.fidelity.signatures_with_foreign_types += 1;
    } else if !changed && !unannotated {
        out.fidelity.signatures_complete += 1;
    }
}

/// Every name this body writes to, including through a field or an index.
///
/// `r.value = 1` and `xs[0] = 1` both need `r` and `xs` to be `var`. So the root of the target
/// is what counts instead of the whole expression.
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
            // A returned Result is native here: the coercion at the `return` decides
            // success or failure by the value, so both sides return bare.
            if let Some((_, payload)) = value.as_ref().and_then(|v| result_call(out, v)) {
                let text = payload
                    .map(|p| format!(" {}", zig_expr(out, p)))
                    .unwrap_or_default();
                zig_line(out, &format!("return{text};"));
                return;
            }
            let text = value
                .as_ref()
                .map(|e| format!(" {}", zig_expr(out, e)))
                .unwrap_or_default();
            zig_line(out, &format!("return{text};"));
        }
        // Zig has no exceptions: a failure is a value in the return type. Which error it is
        // belongs to an error set this has no way to name.
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
        // Zig returns one value, so a pair has nothing to come from here.
        Stmt::TupleAssign { source, line, .. } => carry(
            out,
            &Unsupported {
                construct: "multiple assignment".to_string(),
                source: source.clone(),
                line: *line,
            },
        ),
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
            // A Zig switch must be exhaustive, so the else arm is written even
            // when the source had none.
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
        // Zig writes the step as a continue expression, which also runs when the
        // body says `continue`. So the counted loop crosses whole.
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
            let it = zig_expr(out, iterable);
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
        // Zig has no string type. A string is a slice of bytes that does not change,
        // and that is what a literal is.
        Type::String => "[]const u8".to_string(),
        Type::List(inner) => format!("[]const {}", zig_type(inner)),
        // Hashing a slice by its contents and hashing it by its address are different maps in
        // Zig. The standard library makes you pick: `AutoHashMap` cannot take a string key at
        // all.
        Type::Map(key, value) => match key.as_ref() {
            Type::String => format!("std.StringHashMap({})", zig_type(value)),
            other => format!("std.AutoHashMap({}, {})", zig_type(other), zig_type(value)),
        },
        Type::Optional(inner) => format!("?{}", zig_type(inner)),
        Type::Tuple(parts) => format!("struct {{ {} }}", joined(parts, zig_type)),
        // The shared `Result<T, E>` is this language's own error union, `E!T`. Only a
        // named error side can stand before the `!`. A Rust `Result<T, String>`
        // carries a payload no Zig error set can hold. It crosses as `anyerror`, and
        // the message's identity is the part that does not.
        Type::Named { name, args } if name == "Result" && args.len() == 2 => {
            // A qualified error name keeps its path, spelled with dots the way every
            // other Zig name here is: `anyhow::Error` is not something this grammar
            // reads.
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
        // A generic type is a function of its arguments, so it is applied and not
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
/// A type written through by name can be a word this language spells something else with: Go's
/// `error` is Zig's keyword for an error set. A signature returning it did not parse.
/// `@"error"` is how Zig writes an identifier that collides with one of its own words. Under it
/// the name still says what the source said.
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
/// `undefined` is Zig's own word for a value that has not been decided yet. Using it is
/// deliberate: the program will not run until a person replaces it.
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
        Expr::Name(n) => out.value_name(n),
        Expr::Field { of, name } => {
            let object = receiver(zig_expr(out, of), of);
            // A read of a property is a call here; the idiom that hid the
            // parentheses does not exist in this language. A property is a
            // method, so its spelling comes from the name table and not the
            // field one.
            if out.properties.contains(name) {
                return format!("{object}.{}()", out.name(name));
            }
            format!("{object}.{}", out.field(name))
        }
        // `[…]` on a slice or an array and `.get(…)` on a map, and which this is
        // depends on a type nothing here tracks. Zig's indexable is the slice.
        Expr::Index { of, index } => {
            let object = receiver(zig_expr(out, of), of);
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
        // Zig has no `new`. A value is made by whatever function on the type returns one, which
        // function that is, `init`, a literal, an allocator call, is a fact about the type and
        // not about this expression. A record declared right here is different:
        // its fields are known, and a positional construction maps onto them.
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
            zig_carry(out, "new", format!("new {target}({})", rendered.join(", ")))
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
            // A Zig string is a `[]const u8`, and `==` on a slice is not a comparison
            // the compiler will accept at all. The output looked like the other five
            // and did not build.
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
            // Joining two slices in Zig means allocating, and the allocator is a
            // parameter this function does not have, inventing one changes the
            // signature every caller was written against. `a + b` on two slices is not
            // something the compiler accepts, so it went out looking like the other
            // four targets and not building.
            if *op == BinaryOp::Add && compares_strings(out, left, right) {
                let source = format!("{} + {}", zig_expr(out, left), zig_expr(out, right));
                out.carried(&Unsupported {
                    construct: "joining two strings, which needs an allocator here".into(),
                    source: source.clone(),
                    line: 0,
                });
                // Zig has no block comment, so a marker beside the value would swallow the rest
                // of the line. `@compileError` is a value anywhere one is expected, it says why
                // in the compiler's own output. It cannot be mistaken for code that works,
                // which returning an empty slice quietly could.
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
        // An anonymous list is `.{ … }`, and what it coerces to is decided by where it
        // is used and not by the literal.
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
        // Zig writes no closure without every type spelled, and the source
        // spelled none; inventing them would be a guess about the call sites.
        Expr::Lambda { params, body } => {
            let rendered: Vec<String> = params.iter().map(|p| out.name(p)).collect();
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
            // allocator. So this is a `for` loop over one, and which allocator is a decision
            // this cannot make.
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
        Expr::Propagate(inner) => format!("try {}", zig_expr(out, inner)),
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
/// The name is deliberately *not* case-converted: a `Reading` or an `HttpResponse` is a real
/// type somewhere. Renaming it to suit a convention would point the signature at something that
/// does not exist.
///
/// A name that cannot be written as a type at all, a tuple, a closure, a trait object, becomes
/// the target's unknown type. Emitting it verbatim produced `-> Result<(), String>` in a Python
/// file, which Python cannot parse. And a signature that does not parse is worse than one that
/// admits a gap. A qualified name arrives spelled the source language's way: Go's `sync.Mutex`
/// is not Rust, and Rust's `std::sync::Mutex` is not Go. The path is kept. It says where the
/// type came from, and only `separator` changes.
/// The parts rendered and comma-joined, the shape every tuple spelling shares.
fn joined<T>(parts: &[T], mut render: impl FnMut(&T) -> String) -> String {
    parts.iter().map(&mut render).collect::<Vec<_>>().join(", ")
}

fn generic(
    name: &str,
    args: &[Type],
    open: &str,
    close: &str,
    // How this language separates the parts of a qualified name: `::` in Rust, `.` in the rest.
    // Named for what it is, because "separator" beside a list of arguments reads as the
    // argument separator. The Java writer was written passing `", "` on that reading, turning
    // `sync.Mutex` into `sync, Mutex`.
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

/// The ok type of `Result<T, E>`, where the name means the shared Result.
///
/// The Rust reader spells `Result<T, E>` this way and the Zig reader spells `E!T` the
/// same, which is what lets one writer translate both. A module that declares its own
/// type called `Result` means that one, and gets no translation invented for it.
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
///
/// Exhaustive on purpose, no `_` arm, for the same reason the reader's own walk is.
/// A new variant must not be able to slip through quietly. The Rust writer asks
/// before committing a constant, because a marker that is fine in a body stops the
/// build in a `const`.
fn contains_unsupported(e: &Expr) -> bool {
    match e {
        Expr::Unsupported(_) => true,
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
///
/// `true` is the success side. A payload of `()` counts as none, which is how a
/// function that succeeds with nothing to say returns. A module that declares its own
/// `Ok` or `Err` means those, and gets no translation invented for it.
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
///
/// The exception languages and Go have no variant to hand back, so the name becomes
/// the message, which is also what Zig's own `@errorName` answers.
fn error_variant<'e>(out: &Out, payload: &'e Expr) -> Option<&'e str> {
    let Expr::Field { of, name } = payload else {
        return None;
    };
    matches!(of.as_ref(), Expr::Name(n) if out.sums.contains(n)).then_some(name.as_str())
}

/// The words the exception languages translate a returned `Err` with.
const RESULT_RAISED: &str = "a Result crosses as its own failure here: Ok returns, Err raises.";

/// Is this top-level statement exactly the entry call to a `main` this module declares?
///
/// Four of these six targets run `main` themselves, so the call is not information
/// there. Written anywhere, it would either run the program twice or fail to parse.
/// Dropping it is a translation and not a loss, and the note says which.
fn calls_declared_main(out: &Out, stmt: &Stmt) -> bool {
    let Stmt::Expr(Expr::Call { callee, .. }) = stmt else {
        return false;
    };
    matches!(callee.as_ref(), Expr::Name(name) if name == "main")
        && out.functions.contains_key("main")
}

/// The words every self-running target drops the entry call with.
const ENTRY_DROPPED: &str = "the source's entry call is dropped: the target runs main itself.";

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
///
/// Only `main(String[] args)` and its shape: the one-parameter convention is main's.
/// Inventing arguments for any other entry would be a guess about what it takes.
fn entry_takes_arguments(f: &Function) -> bool {
    f.name == "main" && required_parameters(f) == 1
}

/// A top-level statement, in a target whose top level only declares.
///
/// In the source that statement is the program: `main();` at the bottom of a
/// TypeScript file, the call under `if __name__ == "__main__":`. There is
/// nowhere here for it to run, so it is carried beside a marker. Where it is one
/// expression it is rendered, so the reader sees what the source did.
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

// ------------------------------------------------------------ the builtin table
//
// The canonical spellings are Python's, because its reader needs no normalising:
// `print(x)`, `len(x)`, `str(x)`, `.append`, `.upper`, `.lower`, `.strip`, and
// `sep.join(xs)`. Each reader rewrites its own language's spelling into these.
// Each writer rewrites them back out into its own, so one language pair costs
// two edits and not thirty. What the table does not cover is written through
// unchanged, visible in the output, as before.

/// The module with every same-module base laid flat into its extenders.
///
/// `class User(UserBase)` where `UserBase` sits ten lines up loses nothing to
/// a target without inheritance except the sharing. The base's fields and
/// methods belong to every instance of `User`, so they lay flat into it. The extends
/// marker then stands only for a base this module does not hold. Chains flatten
/// transitively; a cycle, which no source language accepts, stops the walk; a
/// method the extender overrides is the extender's.
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
///
/// The canonical shapes are a call to `super` for the base constructor and a
/// call through a `super` field for a base method. Rust, Go and Zig have no
/// inheritance and flattened any same-module base already, so what reaches
/// them calls into a base out of reach. Writing the word through named
/// nothing in any of the three.
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
        (None, "print", _) => {
            let holes = vec!["{}"; args.len()].join(" ");
            format!(
                "println!(\"{holes}\"{})",
                args.iter()
                    .map(|a| format!(", {}", rust_expr(out, a)))
                    .collect::<String>()
            )
        }
        (None, "len", [x]) => format!("{}.len()", rust_expr(out, x)),
        (None, "str", [x]) => format!("{}.to_string()", rust_expr(out, x)),
        (Some(of), "append", [x]) => {
            format!(
                "{}.push({})",
                rust_expr(out, &of.clone()),
                rust_expr(out, x)
            )
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
        (None, "print", _) => {
            let rendered = joined(args, |a| ts_expr(out, a));
            format!("console.log({rendered})")
        }
        (None, "len", [x]) => format!("{}.length", ts_expr(out, x)),
        // Inside a catch, the exception as text is its message: `String(e)` leads
        // with the class name, which is not what the source printed.
        (None, "str", [Expr::Name(bound)]) if out.catch_bindings.iter().any(|b| b == bound) => {
            format!("({} as Error).message", out.name(bound))
        }
        (None, "str", [x]) => format!("String({})", ts_expr(out, x)),
        (Some(of), "append", [x]) => {
            format!("{}.push({})", ts_expr(out, &of.clone()), ts_expr(out, x))
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
        (None, "print", _) => {
            out.go_imports.insert("fmt");
            let rendered = joined(args, |a| go_expr(out, a));
            format!("fmt.Println({rendered})")
        }
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
        (None, "print", _) => {
            let rendered = args
                .iter()
                .map(|a| java_expr(out, a))
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
        (None, "print", _) => {
            let holes = vec!["{any}"; args.len()].join(" ");
            let rendered = joined(args, |a| zig_expr(out, a));
            format!("std.debug.print(\"{holes}\\n\", .{{ {rendered} }})")
        }
        (None, "len", [x]) => format!("{}.len", zig_expr(out, x)),
        _ => return None,
    })
}

/// The type this record extends, where the target can express one.
///
/// Three of these six languages have inheritance and three do not, so `inheritable` says which
/// kind of target is asking. `None` comes back either because the source declared no base or
/// because this language has none. The second of those leaves a note, since dropping it
/// silently made `class JsonPrimitive extends JsonElement` into a class that extends nothing.
/// That is a different type, and the output said nothing about it.
fn inherited_base(out: &mut Out, record: &Record, inheritable: bool) -> Option<String> {
    let base = record.extends.clone()?;
    if inheritable {
        return Some(base);
    }
    // Said in the output too, beside the type, because that file is the one a
    // reader of the draft has in front of them. A note that lives only in the
    // report leaves the struct looking whole.
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
///
/// Three of these six languages have a constructor and three have a habit. Java names it after
/// the class, Python calls it `__init__` and TypeScript calls it `constructor`; Rust writes
/// `Thing::new`, Go writes `NewThing` and Zig writes `Thing.init`. Which of those a file gets
/// is a fact about the target. So the IR carries *that it is one* and not what it is called.
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
/// Java is the one target here that overloads constructors; everywhere else a type has exactly
/// one. The rest keep the names their source gave them and are reported, since a caller of
/// `Thing(a, b)` has to be told what to write instead. A constructor body that builds and
/// returns its own record, rewritten as the field assignments a receiver-taking constructor
/// makes.
///
/// Only a body that is that. A constructor which computes something first is not this shape.
/// Turning it into one would be a guess about what the rest of it was for.
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
        // Whether a constructor takes a receiver, and whether it says what it returns,
        // is a fact about the target and not about the source. Python's `__init__`
        // takes `self` and returns nothing; Java's and TypeScript's take neither; and
        // the three that spell it by habit take no receiver and return the type,
        // because returning the type is the whole of what makes them one.
        match out.language {
            // A constructor here acts on a value that already exists, so it takes the
            // receiver and says nothing about what it returns. A source whose
            // constructor had none. Rust builds and returns instead, still needs one
            // bound, or Python writes `@staticmethod def __init__(n)` and TypeScript
            // writes `static constructor(n)`.
            Language::Python | Language::Java | Language::TypeScript | Language::Tsx => {
                if method.receiver_binding.is_none() {
                    method.receiver_binding = Some(receiver_word(out.language).to_string());
                }
                method.returns = None;
                // A source that builds and returns its record, `Counter { value. 0, step }`,
                // says the same thing a constructor here says by assigning through the
                // receiver. Left as a return it is not a translation: an `__init__` that
                // returns a value raises. A Java constructor that returns one does not compile.
                if let Some(assignments) = receiver_assignments(method, &record.name) {
                    method.body = assignments;
                }
            }
            // The other three have no constructor, only a habit: a plain function that
            // *returns* the type, which is the whole of what makes it one. It has no receiver,
            // so a body that assigns through one has nowhere to run, writing `self.n = n`
            // inside a function that binds no `self` would not be a translation.
            //
            // A body that does not assign through a receiver has somewhere to run. It already
            // builds a value and returns it, which is the shape these three want. Throwing it
            // away was a rule about receiver-assigning constructors applied to every
            // constructor, and it discarded the one line a Rust constructor is made of.
            _ => {
                // The canonical build-and-return body is already this shape, whatever
                // the source bound as a receiver. An `__init__` of plain assignments
                // arrives as one `Return(RecordLit)`. Nothing needs throwing away.
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
/// `*/` closes one, and a doc comment that quotes a glob, `app/**/route.ts`, carries
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
/// it is a second call for anything else, which would make the program do more than it
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
/// Generous on purpose: a qualified path is a name, `std::fmt::Display`, `sync.Mutex`,
/// and shortening one would point a signature at something that does not exist. What
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
/// A type the source never wrote down. Named and not guessed.
/// Every value this body hands back, `return` and tail alike.
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
                | Stmt::ErrDefer(body) => walk(body, into),
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
///
/// A source that annotates nothing still returns things, and the targets that
/// name every return type wrote none. The body is the only evidence there is.
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
        Language::Rust => "()".to_string(),
        Language::Python => "object".to_string(),
        Language::Go => "any".to_string(),
        // Zig has no dynamic type; `anytype` says the caller decides, which is exactly
        // true of a parameter whose type the source never wrote down.
        Language::Zig => "anytype".to_string(),
        _ => "unknown".to_string(),
    }
}
