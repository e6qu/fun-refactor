//! Every corpus file, translated to every target, with nothing carried. Not a spot check: the
//! vendored corpora are real code from real projects, and this holds two facts about them.
//! Every translation plans without a refusal or a panic. And nothing is carried over verbatim:
//! every construct in these files has a defined lowering into every target. This used to be a
//! ratcheted ledger of losses. The ledger reached zero and became this assertion, so a
//! translation that starts carrying again fails the build naming the construct.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeMap;
use std::path::PathBuf;

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
            assert!(
                plan.fidelity.is_complete(),
                "{} -> {target} is not complete: carried={} translated={}",
                path.display(),
                plan.fidelity.carried_verbatim,
                plan.fidelity.translated()
            );
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

    assert!(
        carried.is_empty(),
        "the corpus carried constructs verbatim; every construct needs a defined lowering:\n  {}",
        carried
            .iter()
            .map(|(construct, n)| format!("{construct}: {n}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
