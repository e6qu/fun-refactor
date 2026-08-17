//! Every corpus file, translated to every target, with the losses pinned.
//!
//! Not a spot check: the vendored corpora are real code from real projects, and this
//! holds two facts about them. Every translation plans without a refusal or a panic.
//! And what the drafts carry as untranslated only ever shrinks: the counts below are
//! a ratchet, like the no-noop budget. A rise fails the build. A fall fails too, and
//! asks you to lower the number here, so the ledger keeps meaning something.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What each corpus, translated everywhere, still cannot say, by construct.
///
/// Some numbers here have moved up as well as down, and both moves were honest.
/// The Zig reader once read no call arguments at all. A call whose argument
/// could not translate looked clean by losing the argument, and reading the
/// arguments made those statements carry truthfully. Four counts rose. And a
/// crossing that empties a big container, the payload `if`, the `test` block,
/// surfaces the losses its body used to swallow under one line. One count
/// falls; its contents' counts rise.
///
/// The same happened when the Zig reader learned that a branch or a loop body can
/// be one bare statement: `if (x) return e;` used to drop its return without a
/// word, and reading it made the statements that cannot cross carry visibly, so
/// `return_expression` and `expression_statement` rose. A `while` with a step
/// clause now carries whole instead of quietly losing its step, which is the
/// `while_statement` line.
const CARRIED: &[(&str, usize)] = &[
    ("await", 26),
    ("anonymous_struct_initializer", 55),
    ("binary_expression", 5),
    ("boolean", 10),
    ("call_expression", 15),
    ("labeled_type_expression", 35),
    ("null_coercion_expression", 15),
    ("while_expression", 5),
    ("builtin_function", 20),
    ("catch_expression", 55),
    ("comptime_declaration", 5),
    ("comptime_statement", 20),
    ("conditional expression", 6),
    ("defer", 60),
    ("errdefer", 22),
    ("comptime_expression", 6),
    ("error propagation", 20),
    ("error_set_declaration", 10),
    ("enum_declaration", 5),
    ("expression_statement", 620),
    ("field_expression", 5),
    ("for_statement", 15),
    ("function_declaration", 5),
    ("if_statement", 127),
    ("instanceof", 66),
    ("joining two strings, which needs an allocator here", 2),
    ("keyword argument", 25),
    ("labeled_statement", 45),
    ("lexical_declaration", 5),
    ("map literal", 11),
    ("multiline_string", 5),
    ("new", 39),
    ("return_expression", 20),
    ("return_statement", 10),
    ("switch_expression", 58),
    ("test_declaration", 10),
    ("throw", 4),
    ("try", 10),
    ("try/catch", 5),
    ("tuple", 2),
    ("variable_declaration", 420),
    ("while_statement", 5),
];

fn corpus_files() -> Vec<(PathBuf, Language)> {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus"));
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the corpus directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let language = match path.extension().and_then(|e| e.to_str()) {
                Some("py") => Language::Python,
                Some("java") => Language::Java,
                Some("zig") => Language::Zig,
                Some("ts") | Some("tsx") => Language::TypeScript,
                _ => continue,
            };
            out.push((path, language));
        }
    }
    assert!(!out.is_empty(), "the corpus is gone");
    out
}

#[test]
fn every_corpus_file_translates_everywhere_and_the_losses_only_shrink() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let mut carried: BTreeMap<String, usize> = BTreeMap::new();
    let mut planned = 0usize;

    for (path, from) in corpus_files() {
        for target in transpile::SUPPORTED {
            if *target == from {
                continue;
            }
            let stem = path.file_stem().expect("a stem").to_string_lossy();
            let out = tmp
                .path()
                .join(format!("{}_{}.{:?}", stem, target, planned))
                .with_extension(match target {
                    Language::Rust => "rs",
                    Language::Go => "go",
                    Language::Java => "java",
                    Language::Python => "py",
                    Language::TypeScript => "ts",
                    Language::Zig => "zig",
                    other => panic!("no extension for {other}"),
                });
            let plan = transpile::plan_to(&path, *target, Some(&out), false)
                .unwrap_or_else(|e| panic!("{} -> {target} refused: {e}", path.display()));
            planned += 1;
            for note in &plan.fidelity.notes {
                if let Some(rest) = note.split(": ").nth(1) {
                    if let Some(construct) = rest.strip_suffix(" carried over unchanged") {
                        *carried.entry(construct.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }

    assert!(planned >= 55, "the corpus shrank to {planned} translations");

    let pinned: BTreeMap<String, usize> = CARRIED
        .iter()
        .map(|(construct, n)| (construct.to_string(), *n))
        .collect();
    let mut wrong = Vec::new();
    for (construct, n) in &carried {
        match pinned.get(construct) {
            Some(p) if n == p => {}
            Some(p) if n < p => wrong.push(format!(
                "{construct}: {n} carried, ledger says {p}. It shrank; lower the ledger."
            )),
            Some(p) => wrong.push(format!(
                "{construct}: {n} carried, ledger says {p}. A construct that translated \
                 stopped translating."
            )),
            None => wrong.push(format!(
                "{construct}: {n} carried and the ledger has no line for it."
            )),
        }
    }
    for (construct, p) in &pinned {
        if !carried.contains_key(construct) {
            wrong.push(format!(
                "{construct}: nothing carried, ledger says {p}. It is done; delete the line."
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the corpus ledger and the sweep disagree:\n  {}",
        wrong.join("\n  ")
    );
}
