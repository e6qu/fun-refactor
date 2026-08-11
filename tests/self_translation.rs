//! Translating the tool's own source, and the other real code in this repository.
//!
//! Fixtures are written by whoever writes the assertion, and they pass. Real code was
//! written by somebody who had never heard of this tool, and it is full of the things
//! nobody thinks to put in a fixture: a comment in the middle of a parameter list, a
//! literal with its width written into it, a doc comment quoting a glob, a string with
//! an escape in it.
//!
//! The bar is the weakest one that is still objective and the strongest one available
//! without six compilers: **whatever comes out must be a file the target's own grammar
//! accepts.** That found nine defects the first time it ran, across ninety-seven of two
//! hundred and thirty-five translations, including three that had been quietly
//! changing the meaning of every string and every JSDoc block since the transpiler
//! landed.

use fun_refactor::transpile;
use std::path::{Path, PathBuf};

/// Every file under `directory` with this extension, sorted so a failure names the
/// same file every time.
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

/// Translate every one of `files` into every other supported language.
fn every_target(files: &[PathBuf], what: &str, least: usize) {
    assert!(
        files.len() >= least,
        "expected at least {least} {what} files to translate, found {}",
        files.len()
    );
    let parsers = fun_refactor::parse::Parsers::new();
    let mut checked = 0;
    for file in files {
        let from = fun_refactor::lang::detect(file).expect("a language");
        for to in transpile::SUPPORTED {
            if *to == from {
                continue;
            }
            let plan = transpile::plan(file, *to).unwrap_or_else(|e| {
                panic!("{} -> {to}: {e}", file.display());
            });
            let parsed = parsers
                .parse(*to, &plan.output)
                .unwrap_or_else(|e| panic!("{} -> {to}: {e}", file.display()));
            assert!(
                !parsed.has_errors(),
                "{} -> {to} produced something {to} cannot parse",
                file.display()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, files.len() * (transpile::SUPPORTED.len() - 1));
}

#[test]
fn the_tools_own_rust_translates_into_something_that_parses() {
    // Twenty thousand lines of Rust nobody wrote to be translated. It is where the
    // comment inside a parameter list came from, four invented parameters named after
    // the sentence that had been sitting between two real ones.
    every_target(&sources("src", "rs"), "Rust", 30);
}

#[test]
fn the_playgrounds_typescript_translates_into_something_that_parses() {
    // Real TypeScript, and the source of the JSDoc defects: a `/** ... */` is one node
    // however many lines it spans, and one of them quotes `app/**/route.ts`, which
    // closes the comment in the middle of a sentence.
    every_target(&sources("web/src", "ts"), "TypeScript", 5);
}

#[test]
fn the_petstore_translates_into_something_that_parses() {
    // A Next.js API tree is TypeScript before it is anything else, and this one is the
    // most idiomatic in the repository: zod builder chains, shorthand properties,
    // nullish coalescing, optional chaining, and a schema module every route imports.
    every_target(&sources("tests/petstore", "ts"), "TypeScript", 8);
}

#[test]
fn the_sample_go_translates_into_something_that_parses() {
    every_target(&sources("web/sample", "go"), "Go", 3);
}

#[test]
fn the_vendored_python_translates_into_something_that_parses() {
    // Somebody else's code entirely: see tests/corpus/PROVENANCE.md.
    every_target(&sources("tests/corpus/fastapi", "py"), "Python", 3);
}

#[test]
fn the_vendored_java_translates_into_something_that_parses() {
    // google/gson. Nothing in this repository is written in Java, so without a corpus
    // the Java reader is exercised only by fixtures somebody wrote to pass.
    every_target(&sources("tests/corpus/gson", "java"), "Java", 3);
}

#[test]
fn the_vendored_zig_translates_into_something_that_parses() {
    // zigtools/zls, and every one of these was a defect the first time it was read: a
    // pointer type, an optional type, `comptime` parameters, `_` as a parameter name,
    // and a destructuring that silently kept the first name and dropped the rest.
    every_target(&sources("tests/corpus/zls", "zig"), "Zig", 2);
}
