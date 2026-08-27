//! Every translated corpus file, taken past the front end by a real compiler.
//!
//! `tests/corpus_compile.rs` asks the strongest question each toolchain can
//! answer about a file whose dependencies live elsewhere. Does it parse, does
//! its AST check, does it format. None of those looks at a type. So a
//! translation could hand `impl Fn(i64) -> i64` a `String`, or call a method on
//! the wrong receiver, and every gate here would pass it.
//!
//! The reason the gate stopped there is real. A corpus file imports a world
//! this repository does not hold: Gson's neighbours, the Zig standard library,
//! SQLAlchemy. `rustc` cannot type-check a file whose every import is missing.
//!
//! So the world is supplied. The compiler is asked what it cannot find, and a
//! stub is written declaring exactly those names and nothing else. The file is
//! compiled again against it. The stub is the assumption made visible: it says,
//! in as many words, which names were taken on trust.
//!
//! What is left after that is either a defect in the writer or a fact about the
//! foreign world that no stub can supply. The count of those is a ratchet: it
//! may fall and it may not rise. Every fall is a bug found this way, which
//! parsing could not have found.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What remains unexplained after the foreign world is supplied.
///
/// Lower is better and this may only fall. Each of these is a diagnostic
/// `rustc` reports about a translated corpus file that stubbing the foreign
/// names did not answer. That makes it a question about the translation.
///
/// The number this gate started at was 1230. The first thing it found was four
/// `fn add` in one `impl`. Java overloads its methods and Rust does not, and
/// every other gate here passed the file. Two of the eleven files carry three
/// quarters of what is left. Both import the Zig standard library, whose
/// surface a stub cannot describe.
const RESIDUE: usize = 1223;

fn corpus_files() -> Vec<PathBuf> {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus"));
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let known = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("py" | "java" | "zig" | "go" | "ts")
            );
            if known {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// One name the compiler could not find, and what it was used as.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Missing {
    /// Used where a type belongs: `fn f(x: JsonElement)`.
    Type(String),
    /// Used where a value belongs: `return JsonNull;`.
    Value(String),
    /// Used as a path: `Character::isLetter(c)`.
    Module(String),
}

/// The names `rustc` says it cannot find in this file.
fn missing_names(file: &Path) -> Vec<Missing> {
    let out = Command::new("rustc")
        .arg("--edition")
        .arg("2021")
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit")
        .arg("metadata")
        .arg("--error-format=json")
        .arg("-o")
        .arg(file.with_extension("rmeta"))
        .arg(file)
        .output();
    let Ok(out) = out else { return Vec::new() };
    let text = String::from_utf8_lossy(&out.stderr);
    let mut found = BTreeSet::new();
    for line in text.lines() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let message = json["message"].as_str().unwrap_or_default();
        // `cannot find type `X` in this scope`, and the value and module forms.
        let Some(rest) = message.strip_prefix("cannot find ") else {
            // `failed to resolve: use of undeclared crate or module `x``
            if let Some(named) = message.strip_prefix("failed to resolve: ") {
                if let Some(name) = between_backticks(named) {
                    found.insert(Missing::Module(name));
                }
            }
            continue;
        };
        let Some(name) = between_backticks(rest) else {
            continue;
        };
        if rest.starts_with("type") {
            found.insert(Missing::Type(name));
        } else if rest.starts_with("value") || rest.starts_with("function") {
            found.insert(Missing::Value(name));
        }
    }
    found.into_iter().collect()
}

/// The text between the first pair of backticks.
fn between_backticks(text: &str) -> Option<String> {
    let start = text.find('`')? + 1;
    let end = text[start..].find('`')? + start;
    let name = &text[start..end];
    let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    plain.then(|| name.to_string())
}

/// A stub declaring exactly the names the compiler could not find.
///
/// Every declaration is as permissive as Rust allows, because the point is to
/// stop the *resolver* complaining and let the type checker speak. A stub that
/// guessed a signature would make the type checker complain about the stub
/// instead of about the translation.
fn stub_for(missing: &[Missing]) -> String {
    let mut out = String::from(
        "// Generated: the foreign world this file imports, declared so the type\n\
         // checker can reach the translation. Each name is one the compiler said\n\
         // it could not find.\n#![allow(dead_code, non_snake_case, non_camel_case_types)]\n\n",
    );
    for name in missing {
        match name {
            Missing::Type(name) => {
                out.push_str(&format!(
                    "#[derive(Clone, Debug, Default, PartialEq)]\npub struct {name};\n"
                ));
            }
            // A value may be called, indexed or read. A unit struct is all
            // three's receiver and the one shape that admits every use.
            Missing::Value(name) => {
                out.push_str(&format!(
                    "#[derive(Clone, Debug, Default, PartialEq)]\n\
                     pub struct fr_{name};\npub const {name}: fr_{name} = fr_{name};\n"
                ));
            }
            Missing::Module(name) => {
                out.push_str(&format!("pub mod {name} {{}}\n"));
            }
        }
    }
    out
}

/// Diagnostics `rustc` still reports once the stub is in front of the file.
fn remaining(file: &Path, stub: &str) -> usize {
    let source = std::fs::read_to_string(file).unwrap_or_default();
    let together = file.with_file_name("with_stub.rs");
    std::fs::write(&together, format!("{stub}\n{source}")).expect("the combined file");
    let out = Command::new("rustc")
        .arg("--edition")
        .arg("2021")
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit")
        .arg("metadata")
        .arg("--error-format=json")
        .arg("-o")
        .arg(together.with_extension("rmeta"))
        .arg(&together)
        .output();
    let Ok(out) = out else { return 0 };
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|j| j["level"] == "error")
        .count()
}

#[test]
fn what_the_type_checker_says_about_a_translation_only_shrinks() {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("rustc is not on PATH; this gate checked nothing");
        return;
    }
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let mut total = 0;
    let mut checked = 0;
    let mut worst: Vec<(String, usize)> = Vec::new();

    for source in corpus_files() {
        let out = tmp.path().join(format!("f{checked}.rs"));
        let Ok(plan) = transpile::plan_to(&source, Language::Rust, Some(&out), true) else {
            continue;
        };
        if std::fs::write(&out, &plan.output).is_err() {
            continue;
        }
        checked += 1;
        // Two rounds: stubbing a name can reveal the next one behind it, and
        // more rounds than that stop finding anything.
        let mut missing = missing_names(&out);
        let mut stub = stub_for(&missing);
        let second = {
            let together = out.with_file_name("with_stub.rs");
            std::fs::write(
                &together,
                format!(
                    "{stub}\n{}",
                    std::fs::read_to_string(&out).unwrap_or_default()
                ),
            )
            .expect("the combined file");
            missing_names(&together)
        };
        for name in second {
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
        missing.sort();
        stub = stub_for(&missing);

        let left = remaining(&out, &stub);
        total += left;
        if left > 0 {
            worst.push((
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                left,
            ));
        }
    }

    assert!(checked > 0, "no corpus file translated into Rust at all");
    worst.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    eprintln!(
        "semantic gate: {total} diagnostic(s) across {checked} translated file(s). \
         Worst: {:?}",
        worst.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        total <= RESIDUE,
        "the type checker reports {total} thing(s) about translated corpus files, \
         and the budget is {RESIDUE}. A translation got worse, or a new corpus file \
         arrived. Worst offenders: {:?}",
        worst.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        total >= RESIDUE.saturating_sub(20),
        "the residue is {total}, well under the budget of {RESIDUE}. Lower the \
         budget in this file so the gain is held."
    );
}
