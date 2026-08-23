//! The open entries in BUGS.md, held to what they say.
//!
//! Eight of the thirteen are limits of the published grammars, and
//! `tests/known_grammar_gaps.rs` pins every one of those from both sides. The rest are this
//! tool's own behaviour, and until now they were prose: a description of what happens, with
//! nothing to notice when it stopped happening. B11 said `@content` was a gap after it had
//! stopped being one, and nothing noticed for months.
//!
//! Each test here asserts the *whole* of its entry, both what the tool does not do and what it
//! does instead, because every one of these stands on the second half. An incomplete answer
//! that says so is a different thing from a wrong one. A test that checked only the
//! incompleteness would pass just as well if the report went away.
//!
//! A failure here means the entry is out of date. The entry is what to update.

use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

#[test]
fn dispatch_is_followed_as_far_as_the_source_declares_it() {
    // B5. A call through a trait object resolves to no implementation, so reachability fans it
    // out to every type that declares itself an implementation. What is left is undecidable and
    // not unimplemented: a function held in a struct field and called through it is declared a
    // method of nothing. So there is no method set to look it up in.
    let (_tmp, root) = workspace(&[
        (
            "a.rs",
            "pub trait Shape {\n    fn area(&self) -> f64;\n}\npub struct Circle;\n\
             impl Shape for Circle {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\n\
             pub fn total(s: &dyn Shape) -> f64 {\n    s.area()\n}\n",
        ),
        (
            "b.rs",
            "pub struct Held {\n    pub run: fn() -> f64,\n}\n\
             pub fn go(h: &Held) -> f64 {\n    (h.run)()\n}\n\
             pub fn candidate() -> f64 {\n    2.0\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::detect(&index).expect("entry points");
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        !dead.contains(&"area"),
        "the implementation is reached through the trait, so it is not dead: {dead:?}"
    );
    assert!(
        dead.contains(&"candidate"),
        "B5's remaining half is a function reached only through a struct field, and it is \
         still listed. If it no longer is, update the entry: {dead:?}"
    );
}
#[test]
fn a_values_answer_names_the_channel_it_was_never_told_about() {
    // B13. Given some of the inputs and not others, the competition is decided *given the
    // inputs supplied*. The answer names the channel that was left out. Given none, nothing is
    // decided at all. Neither one infers an invocation.
    let (_tmp, root) = workspace(&[
        ("Chart.yaml", "name: chart\nversion: 0.1.0\n"),
        ("values.yaml", "replicas: 1\n"),
        (
            "templates/deploy.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: {{ .Values.replicas }}\n",
        ),
    ]);
    let index = index_of(&root);
    let key = symbol(&index, "replicas");
    use fun_refactor::analysis::provenance::{self, ValuesInputs};

    // The report says this in its stops: a channel outside the workspace that could
    // pre-empt every source listed is a stop, and so is a competition the supplied inputs
    // settle. Both name what they were never told about.
    let said = |report: &provenance::Provenance| -> String {
        report
            .stops
            .iter()
            .map(|(_, reason)| reason.to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let nothing_supplied = ValuesInputs::parse(&[], &[], &[]).expect("no inputs");
    let without =
        provenance::provenance_with_inputs(&index, key, 5, &nothing_supplied).expect("a report");
    assert!(
        said(&without).contains("overridden externally"),
        "with no inputs the answer is undecided and says so: {}",
        said(&without)
    );

    let some_supplied =
        ValuesInputs::parse(&[], &["replicas=3".to_string()], &[]).expect("one --set");
    let with =
        provenance::provenance_with_inputs(&index, key, 5, &some_supplied).expect("a report");
    assert!(
        said(&with).contains("given the inputs supplied"),
        "with some inputs the answer is decided given them, and names what is missing: {}",
        said(&with)
    );
}
