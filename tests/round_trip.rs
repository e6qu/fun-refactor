//! A signature that goes out and comes back must be the same signature.
//!
//! "The output parses" is the weakest objective bar and it found nine defects. This is
//! the next one up, and it asks a question parsing cannot: **did anything go missing on
//! the way?** A translation that drops a parameter, or invents one, or loses a function
//! altogether, produces a file the target's grammar is perfectly happy with.
//!
//! The check is a round trip. Read the source into the IR, translate it, read the
//! *result* back into the IR, and compare. The IR is the only place two files written
//! in different languages can be compared at all.
//!
//! What is compared is deliberately narrow: **which functions exist, and what their
//! parameters are called.** Types are where the legitimate differences live — Go writes
//! `struct{}` for nothing at all, Zig writes a slice where TypeScript writes an array —
//! and a check that argued about those would spend its life growing exceptions. A
//! parameter appearing or vanishing is never legitimate.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use fun_refactor::transpile::ir::{Function, Item, Module, ParamKind};
use std::path::{Path, PathBuf};

/// Every function in a module, wherever it is written.
///
/// Java has no top level below the type and Zig writes methods inside their struct, so
/// the same function is a module item in one language and a record's method in another.
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
/// a round trip through anywhere else loses the calling convention — which the fidelity
/// report already says, in those words.
fn signature(f: &Function) -> (String, Vec<String>) {
    let params = f
        .params
        .iter()
        .filter(|p| p.kind == ParamKind::Normal)
        .map(|p| plain(&p.name))
        .collect();
    (plain(&f.name), params)
}

fn signatures(module: &Module) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> =
        functions(module).iter().map(|f| signature(f)).collect();
    out.sort();
    out
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
        let before = signatures(&transpile::read_file(file).expect("the source reads"));
        let from = fun_refactor::lang::detect(file).expect("a language");
        for to in transpile::SUPPORTED {
            if *to == from {
                continue;
            }
            let after = signatures(&there_and_back(file, *to));
            // The whole list at once, sorted, rather than each name looked up in turn:
            // a name is not unique. Java overloads `add(Boolean)` beside
            // `add(Character)`, and Zig writes a `deinit` in every struct in the file,
            // so looking one up by name compares two different functions and calls the
            // difference a defect.
            let missing: Vec<_> = before.iter().filter(|s| !after.contains(s)).collect();
            let gained: Vec<_> = after.iter().filter(|s| !before.contains(s)).collect();
            assert!(
                missing.is_empty() && gained.is_empty(),
                "{} -> {to} -> {from} did not come back the same\n  lost:   {missing:?}\n  gained: {gained:?}",
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
