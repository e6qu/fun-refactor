//! A signature that goes out and comes back must be the same signature.
//!
//! "The output parses" is the weakest objective bar and it found nine defects. This is the next
//! one up, and it asks a question parsing cannot. **did anything go missing on the way?** A
//! translation may drop a parameter, invent one, or lose a function altogether. The target's
//! grammar is perfectly happy with the file either way.
//!
//! The check is a round trip. Read the source into the IR, translate it, read the *result* back
//! into the IR, and compare. The IR is the only place two files written in different languages
//! can be compared at all.
//!
//! What is compared is deliberately narrow: **which functions exist, and what their parameters
//! are called.** Types are where the legitimate differences live. Go writes `struct{}` for
//! nothing at all, Zig writes a slice where TypeScript writes an array. A check that argued
//! about those would spend its life growing exceptions. A parameter appearing or vanishing is
//! never legitimate.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use fun_refactor::transpile::ir::{Expr, Function, Item, Module, ParamKind, TemplatePart, Type};
use std::path::{Path, PathBuf};

/// Every function in a module, wherever it is written.
///
/// Java has no top level below the type and Zig writes methods inside their struct.
/// So the same function is a module item in one language and a record's method in
/// another.
/// Where it sits is the translation working; what it is called is the promise.
fn functions(module: &Module) -> Vec<&Function> {
    let mut found = Vec::new();
    for item in &module.items {
        match item {
            Item::Function(f) => found.push(f),
            Item::Record(r) => found.extend(r.methods.iter()),
            _ => {}
        }
    }
    found
}

/// A name with the conventions taken back off, so `userName` and `user_name` compare
/// equal. Underscores go too: Go's exported capital and Python's leading underscore are
/// spellings of visibility, not of the name.
fn plain(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// What must survive: the function's name, and the name of every ordinary parameter.
///
/// `*args`, `**kwargs` and a bare `*` are left out on purpose. Only Python has them, so
/// a round trip through anywhere else loses the calling convention, which the fidelity
/// report already says, in those words.
fn signature(f: &Function) -> (String, Vec<String>) {
    let params = f
        .params
        .iter()
        .filter(|p| p.kind == ParamKind::Normal)
        .map(|p| format!("{}: {}", plain(&p.name), shape(p.ty.as_ref())))
        .collect();
    // A constructor's name is not information: Java names it after the class, Python calls it
    // `__init__`. Rust, Go and Zig call it `new`, `NewThing` and `init` by habit. Comparing
    // those would be comparing the two targets, not the translation.
    let name = match f.is_constructor {
        true => "<constructor>".to_string(),
        false => plain(&f.name),
    };
    (name, params)
}

fn signatures(module: &Module) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> =
        functions(module).iter().map(|f| signature(f)).collect();
    out.sort();
    out
}

/// A type with the target's spelling taken back off.
///
/// Which scalar is not compared: TypeScript has one numeric type. So an `i64` that goes through
/// it comes back a `number` and there is nothing wrong with that. What is compared is the
/// *shape*. A list stays a list, an optional stays optional, a named type keeps its name.
/// None of those can change for a good reason.
fn shape(ty: Option<&Type>) -> String {
    match ty {
        // Go writes nothing at all for a function that returns nothing, so "returns
        // `()`" and "declares no return type" are the same sentence there.
        None | Some(Type::Unit) => "nothing".to_string(),
        Some(Type::Bool) => "bool".to_string(),
        Some(Type::Int) | Some(Type::Float) => "number".to_string(),
        Some(Type::String) => "string".to_string(),
        Some(Type::List(inner)) => format!("list<{}>", shape(Some(inner))),
        Some(Type::Set(inner)) => format!("set<{}>", shape(Some(inner))),
        Some(Type::Map(key, value)) => {
            format!("map<{},{}>", shape(Some(key)), shape(Some(value)))
        }
        Some(Type::Optional(inner)) => format!("option<{}>", shape(Some(inner))),
        // A function type keeps its arity and the shape of each part. What it
        // is spelled with is each language's own business.
        Some(Type::Fn { params, returns }) => {
            let taken: Vec<String> = params.iter().map(|p| shape(Some(p))).collect();
            format!("fn<({}),{}>", taken.join(","), shape(Some(returns)))
        }
        Some(Type::Tuple(parts)) => {
            let inner: Vec<String> = parts.iter().map(|p| shape(Some(p))).collect();
            format!("tuple<{}>", inner.join(","))
        }
        // A name this tool cannot write at all, such as a Python `str | Any`, a Rust closure,
        // is replaced by a placeholder, which is a rename and the one exception this
        // check has to allow. The fidelity report is where that loss is stated.
        Some(Type::Named { name, .. }) if name.starts_with("Unwritable") => {
            "<unwritable>".to_string()
        }
        // Each target has a word for "the source did not say": `any`, `unknown`,
        // `object`, `anytype`. Reading one back is reading "no type was declared", which
        // is what it was.
        Some(Type::Named { name, args })
            if args.is_empty()
                && matches!(name.as_str(), "any" | "unknown" | "object" | "anytype") =>
        {
            "nothing".to_string()
        }
        // A character is an integral type in Java and a byte in Zig. The IR has no
        // character, so which number it is depends on where it has been.
        Some(Type::Named { name, args })
            if args.is_empty() && matches!(name.as_str(), "char" | "rune" | "byte") =>
        {
            "number".to_string()
        }
        // The last segment only. A qualified name says where a type came from. Go has room for
        // exactly one level of that, `crate::model::Reference` is `model.Reference` there and
        // cannot be anything else. What must not change is which type it is.
        Some(Type::Named { name, args }) => {
            let last = name.rsplit([':', '.']).next().unwrap_or(name);
            format!("{}/{}", plain(last), args.len())
        }
    }
}

/// Every field of every record, and every module constant.
///
/// A field that vanishes is as bad as a parameter that vanishes, and nothing
/// was checking. They are listed with the type they belong to, since two records may
/// both have a `name`.
fn fields(module: &Module) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in &module.items {
        match item {
            Item::Record(r) => {
                for field in &r.fields {
                    out.push((plain(&r.name), plain(&field.name)));
                }
            }
            // Java has no top level below the type. So a module constant is written as a
            // `static final` field of the file's class and read back as one. Which side of that
            // line it sits on is the translation working.
            Item::Constant(c) => out.push((String::new(), plain(&c.name))),
            _ => {}
        }
    }
    out.sort();
    out
}

/// Does anything under this expression carry no translation?
///
/// The Rust writer refuses to commit such a value to a `const`. A marker that is a
/// fine draft in a body stops the build at compile-time evaluation there. The
/// constant carries whole as a comment instead, which the fidelity report states, and
/// which this check must not call an unexplained loss.
fn untranslatable(e: &Expr) -> bool {
    match e {
        Expr::Variant { fields, .. } => fields.iter().any(|(_, v)| untranslatable(v)),
        Expr::SetLit(items) => items.iter().any(untranslatable),
        Expr::Unsupported(_) => true,
        Expr::Field { of, .. } => untranslatable(of),
        Expr::Index { of, index } => untranslatable(of) || untranslatable(index),
        Expr::Call { callee, args } | Expr::New { callee, args } => {
            untranslatable(callee) || args.iter().any(untranslatable)
        }
        Expr::Binary { left, right, .. } => untranslatable(left) || untranslatable(right),
        Expr::Unary { operand, .. } => untranslatable(operand),
        Expr::Await(inner) | Expr::Propagate(inner) => untranslatable(inner),
        Expr::Keyword { value, .. } => untranslatable(value),
        Expr::Cast { ty, value } => untranslatable(ty) || untranslatable(value),
        Expr::InstanceOf { value, ty } => untranslatable(value) || untranslatable(ty),
        Expr::RecordLit { fields, .. } => fields.iter().any(|(_, value)| untranslatable(value)),
        Expr::Coalesce { value, fallback } => untranslatable(value) || untranslatable(fallback),
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => untranslatable(condition) || untranslatable(then) || untranslatable(otherwise),
        Expr::Tuple(items) | Expr::ListLit(items) => items.iter().any(untranslatable),
        Expr::MapLit(entries) => entries
            .iter()
            .any(|(k, v)| untranslatable(k) || untranslatable(v)),
        Expr::Template(parts) => parts.iter().any(|part| match part {
            TemplatePart::Expr(e) => untranslatable(e),
            TemplatePart::Text(_) => false,
        }),
        Expr::Comprehension {
            element,
            iterable,
            condition,
            ..
        } => {
            untranslatable(element)
                || untranslatable(iterable)
                || condition.as_deref().is_some_and(untranslatable)
        }
        Expr::Lambda { body, .. } => untranslatable(body),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Name(_) => false,
    }
}

/// The constants Rust carries as comments: their values did not translate.
fn comment_carried_constants(module: &Module) -> Vec<(String, String)> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Constant(c) if untranslatable(&c.value) => Some((String::new(), plain(&c.name))),
            _ => None,
        })
        .collect()
}

/// Translate `file` into `to`, and read the result back.
fn there_and_back(file: &Path, to: Language) -> Module {
    let plan =
        transpile::plan(file, to).unwrap_or_else(|e| panic!("{} -> {to}: {e}", file.display()));
    let directory = tempfile::tempdir().expect("a temporary directory");
    let name = plan
        .destination
        .file_name()
        .expect("the destination has a name");
    let written = directory.path().join(name);
    std::fs::write(&written, &plan.output).expect("writing the translation");
    transpile::read_file(&written)
        .unwrap_or_else(|e| panic!("{} -> {to} -> back: {e}", file.display()))
}

fn sources(directory: &str, extension: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(directory);
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == extension) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every function that went out came back, with the parameters it left with.
fn nothing_goes_missing(files: &[PathBuf], least: usize) {
    assert!(files.len() >= least, "found only {} files", files.len());
    for file in files {
        let source = transpile::read_file(file).expect("the source reads");
        let before = signatures(&source);
        let fields_before = fields(&source);
        let from = fun_refactor::lang::detect(file).expect("a language");
        for to in transpile::COMPLETE {
            if *to == from {
                continue;
            }
            let there = there_and_back(file, *to);
            let after = signatures(&there);
            // The whole list at once, sorted, instead of each name looked up in turn: a name is
            // not unique. Java overloads `add(Boolean)` beside `add(Character)`, and Zig writes
            // a `deinit` in every struct in the file. So looking one up by name compares two
            // different functions and calls the difference a defect. A placeholder stands for a
            // type that could not be written, so it compares equal to whatever it replaced. A
            // parameter's name always has to match. Its type has to match unless one side holds
            // a placeholder for something this tool cannot write: a tuple, a closure, a union.
            // The placeholder *is* the loss, and the fidelity report states it.
            let same_parameters = |a: &[String], b: &[String]| {
                a.len() == b.len()
                    && a.iter().zip(b.iter()).all(|(x, y)| {
                        if x == y {
                            return true;
                        }
                        let (xn, xt) = x.split_once(": ").unwrap_or((x, ""));
                        let (yn, yt) = y.split_once(": ").unwrap_or((y, ""));
                        xn == yn && (xt.contains("<unwritable>") || yt.contains("<unwritable>"))
                    })
            };
            let alike = |a: &(String, Vec<String>), b: &(String, Vec<String>)| {
                a.0 == b.0 && same_parameters(&a.1, &b.1)
            };
            let mut missing: Vec<_> = before
                .iter()
                .filter(|s| !after.iter().any(|t| alike(s, t)))
                .collect();
            let mut gained: Vec<_> = after
                .iter()
                .filter(|s| !before.iter().any(|t| alike(s, t)))
                .collect();
            // A constructor may change its name in either direction, because in three of these
            // languages "constructor" *is* a naming convention. Java overloads them and nobody
            // else does, so a second one written elsewhere keeps its source's name and the
            // report says so. And a Rust `new_handle` that returns a `Handle` is written
            // `NewHandle` in Go, the way Go spells a constructor, so it comes back as
            // `Handle::new`. Both are the signature surviving under the target's own
            // convention, which is the promise. What is never allowed is the parameters
            // changing.
            loop {
                let pair = missing.iter().enumerate().find_map(|(at, (name, params))| {
                    let constructor = name == "<constructor>";
                    gained
                        .iter()
                        .position(|(other, p)| {
                            (constructor || other == "<constructor>") && same_parameters(params, p)
                        })
                        .map(|other| (at, other))
                });
                let Some((mine, theirs)) = pair else { break };
                missing.remove(mine);
                gained.remove(theirs);
            }
            // A test crossing a target with no runner of its own degrades to a
            // plain function named `test…`, and the writer's note says so. That
            // function coming back is the test under another name, not a gain.
            let degraded: Vec<String> = source
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Test { name, .. } => Some(format!("test{}", plain(name))),
                    _ => None,
                })
                .collect();
            gained.retain(|(name, params)| !(params.is_empty() && degraded.contains(&plain(name))));
            // Java overloads its methods, and Rust and Zig refuse two members
            // spelled alike. So later overloads take a numbered name, and the
            // writer's note says so. Coming back, `add2` is the `add` that was
            // lost and not a method the source lacked. The pair moves together:
            // a numbered name is only forgiven where the name it was made from
            // went missing, so a real loss still fails.
            let numbered: Vec<(String, Vec<String>)> = gained
                .iter()
                .filter(|(name, params)| {
                    let bare = plain(name);
                    let Some(root) = bare.strip_suffix(|c: char| c.is_ascii_digit()) else {
                        return false;
                    };
                    missing
                        .iter()
                        .any(|(lost, held)| plain(lost) == root && held == params)
                })
                .map(|pair| (*pair).clone())
                .collect();
            for (name, params) in numbered {
                let root = plain(&name);
                let root = root.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                if let Some(at) = missing
                    .iter()
                    .position(|(lost, held)| plain(lost) == root && *held == params)
                {
                    missing.remove(at);
                }
                gained.retain(|(n, p)| !(*n == name && *p == params));
            }
            // Java and TypeScript build a record by calling a constructor and
            // have no literal to build it with. A record that crosses into one
            // arrives with the constructor its fields imply. Coming back,
            // that constructor is the record's own shape and not a function the
            // source lacked. A constructor the source *did* declare and that
            // went missing still fails, because losses are checked apart.
            let builds_a_record = |params: &Vec<String>| {
                source.items.iter().any(|item| match item {
                    Item::Record(r) => r.fields.len() == params.len(),
                    _ => false,
                })
            };
            gained.retain(|(name, params)| name != "<constructor>" || !builds_a_record(params));
            assert!(
                missing.is_empty() && gained.is_empty(),
                "{} -> {to} -> {from} did not come back the same\n  lost:   {missing:?}\n  gained: {gained:?}",
                file.display()
            );

            let after = fields(&there);
            // The Rust writer carries a constant whose value did not translate as a
            // comment, since a `todo!()` in a `const` stops the build. It does not
            // come back from there. The fidelity report states that loss; this check
            // is for the ones nothing states.
            let stated = match to {
                Language::Rust => comment_carried_constants(&source),
                _ => Vec::new(),
            };
            let lost: Vec<_> = fields_before
                .iter()
                .filter(|f| !after.contains(f) && !stated.contains(f))
                .collect();
            assert!(
                lost.is_empty(),
                "{} -> {to} -> {from} lost data: {lost:?}\n  came back: {after:?}",
                file.display()
            );
        }
    }
}

#[test]
fn the_tools_own_rust_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("src", "rs"), 30);
}

#[test]
fn the_playgrounds_typescript_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("web/src", "ts"), 5);
}

#[test]
fn the_petstore_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("tests/petstore", "ts"), 8);
}

#[test]
fn the_samples_survive_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("web/sample", "go"), 3);
}

#[test]
fn the_vendored_python_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("tests/corpus/fastapi", "py"), 3);
}

#[test]
fn the_vendored_java_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("tests/corpus/gson", "java"), 3);
}

#[test]
fn the_vendored_zig_survives_a_round_trip_through_every_language() {
    nothing_goes_missing(&sources("tests/corpus/zls", "zig"), 2);
}
