//! Does the code that a refactoring writes still compile?
//!
//! The edit engine parses a file before an edit and after it, and rejects an edit that
//! introduces a syntax error. Four defects passed that check and reached the repository:
//! an attribute separated from the import it guarded, an integration test that imported
//! the library as `crate::`, a signature that changed with no call site updated, and a
//! method call renamed to a method that does not exist. Every one of them parses.
//!
//! This runs the compiler for the language over the result. A language whose compiler is
//! absent is named in the output of the run, so a pass cannot mean that nothing was
//! checked.

use fun_refactor::edit::EditSet;
use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;
use std::process::Command;

/// A workspace on disk that can be indexed, edited and compiled.
struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        for (name, content) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&path, content).expect("write");
        }
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn index(&self) -> Index {
        let scanned = scan(self.dir.path(), &ScanOptions::default()).expect("scan");
        Index::build_from_scan(&scanned).expect("index")
    }

    fn apply(&self, edits: &EditSet) {
        for (path, file_edits) in edits.iter() {
            let before = std::fs::read_to_string(path).unwrap_or_default();
            let after =
                fun_refactor::edit::apply_to_string(&before, file_edits).expect("the edits apply");
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(path, after).expect("write");
        }
    }

    /// Compile the workspace. `Err` holds what the compiler said.
    fn compiles(&self) -> Result<(), String> {
        let output = Command::new("cargo")
            .args(["check", "--quiet", "--all-targets"])
            .current_dir(self.dir.path())
            .env("CARGO_TARGET_DIR", shared_target_dir())
            .env("RUSTFLAGS", "-A warnings")
            .output()
            .expect("cargo runs");
        match output.status.success() {
            true => Ok(()),
            false => Err(String::from_utf8_lossy(&output.stderr).to_string()),
        }
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).expect("read")
    }
}

/// One target directory for every case in this binary, so the standard library and the
/// fixture's own artifacts are built once.
fn shared_target_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("fun-refactor-compile-gate");
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn rustc_is_available() -> bool {
    Command::new("cargo")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A crate with nothing awkward in it. Every reference to `width` resolves exactly, so
/// every command has enough to work with and the result has to compile.
///
/// One import sits behind a feature, because sorting imports moves whole lines and an
/// attribute occupies one of its own.
fn plain() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Cargo.toml",
            "[package]\nname = \"gate-plain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [features]\nextra = []\n",
        ),
        (
            "src/lib.rs",
            "pub mod holder;\npub mod util;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
        ),
        (
            "src/holder.rs",
            "pub struct Holder {\n    pub items: Vec<u8>,\n}\n\n\
             pub fn width(items: &[u8], n: usize) -> usize {\n    items.len() + n\n}\n",
        ),
        (
            "src/util.rs",
            "use crate::holder::{width, Holder};\n#[cfg(feature = \"extra\")]\n\
             use crate::extra::Extra;\nuse std::fmt::Debug;\n\n\
             pub fn describe(h: &Holder, d: &dyn Debug) -> String {\n    \
             let total = width(&h.items, 1);\n    format!(\"{} {d:?}\", total * 2)\n}\n\n\
             #[cfg(feature = \"extra\")]\npub fn extra(e: &Extra) -> u8 {\n    e.value\n}\n",
        ),
        (
            "src/extra.rs",
            "pub struct Extra {\n    pub value: u8,\n}\n",
        ),
        (
            "tests/it.rs",
            "use gate_plain::holder::{width, Holder};\n\n\
             #[test]\nfn t() {\n    let h = Holder { items: vec![1, 2] };\n    \
             let n = width(&h.items, 1);\n    assert_eq!(n, 3);\n}\n",
        ),
    ]
}

/// The shapes the four known defects needed. A free function and a method share a name,
/// and an integration test calls both from inside `assert_eq!`, where a macro body is
/// tokens and a receiver is not recorded.
fn awkward() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Cargo.toml",
            "[package]\nname = \"gate-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [features]\nextra = []\n",
        ),
        (
            "src/lib.rs",
            "pub mod holder;\npub mod util;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
        ),
        (
            "src/holder.rs",
            "pub struct Holder {\n    pub items: Vec<u8>,\n}\n\n\
             pub fn width(items: &[u8], n: usize) -> usize {\n    items.len() + n\n}\n\n\
             impl Holder {\n    pub fn width(&self, n: usize) -> usize {\n        \
             width(&self.items, n)\n    }\n}\n",
        ),
        (
            "src/util.rs",
            "use crate::holder::Holder;\n#[cfg(feature = \"extra\")]\nuse crate::extra::Extra;\n\
             use std::fmt::Debug;\n\n\
             pub fn describe(h: &Holder, d: &dyn Debug) -> String {\n    \
             format!(\"{} {:?}\", h.width(1), d)\n}\n\n\
             #[cfg(feature = \"extra\")]\npub fn extra(e: &Extra) -> u8 {\n    e.value\n}\n",
        ),
        (
            "src/extra.rs",
            "pub struct Extra {\n    pub value: u8,\n}\n",
        ),
        (
            "tests/it.rs",
            "use gate_fixture::holder::{width, Holder};\n\n\
             #[test]\nfn the_method_and_the_function_agree() {\n    \
             let h = Holder { items: vec![1, 2] };\n    \
             assert_eq!(h.width(1), 3);\n    \
             assert_eq!(width(&h.items, 1), 3);\n}\n",
        ),
    ]
}

#[test]
fn the_fixture_compiles_before_anything_touches_it() {
    if !rustc_is_available() {
        panic!("cargo is not on PATH, so this gate checked nothing");
    }
    for (name, files) in [("plain", plain()), ("awkward", awkward())] {
        let ws = Workspace::new(&files);
        if let Err(e) = ws.compiles() {
            panic!("the {name} fixture is broken before any refactoring:\n{e}");
        }
    }
}

// ------------------------------------------------------------------ the gate

fn the_free_function(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name && s.kind == fun_refactor::model::SymbolKind::Function)
        .unwrap_or_else(|| panic!("no free function named {name}"))
        .id
}

/// The one rule this file enforces: a plan that reaches disk has to compile.
///
/// A refusal is a result and not a failure, so `Err` passes. What cannot pass is a plan
/// that the tool was willing to write and that the compiler then rejects.
fn gate(what: &str, ws: &Workspace, planned: anyhow::Result<EditSet>) -> bool {
    match planned {
        Err(refusal) => {
            eprintln!("  {what}: refused — {refusal}");
            false
        }
        Ok(edits) => {
            assert!(
                !edits.is_empty(),
                "{what} planned no edits and did not refuse"
            );
            ws.apply(&edits);
            if let Err(compiler) = ws.compiles() {
                panic!("{what} produced code that does not compile:\n{compiler}");
            }
            true
        }
    }
}

fn must_plan(what: &str, ws: &Workspace, planned: anyhow::Result<EditSet>) {
    assert!(
        gate(what, ws, planned),
        "{what} refused on a workspace with nothing awkward in it"
    );
}

// ------------------------------------------------- every command, plain crate

#[test]
fn organising_imports_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("src/util.rs")).map(|p| p.edits);
    must_plan("organising imports", &ws, planned);
    assert!(
        ws.read("src/util.rs")
            .contains("#[cfg(feature = \"extra\")]\nuse crate::extra::Extra;"),
        "the attribute lost its import:\n{}",
        ws.read("src/util.rs")
    );
}

#[test]
fn moving_a_symbol_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.rs"))
        .map(|p| p.edits);
    must_plan("moving a symbol", &ws, planned);
    assert!(
        ws.read("tests/it.rs").contains("gate_plain::util::width"),
        "the integration test was not repointed:\n{}",
        ws.read("tests/it.rs")
    );
}

#[test]
fn changing_a_signature_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a signature", &ws, planned);
}

#[test]
fn renaming_a_function_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "span_of").map(|p| p.edits);
    must_plan("renaming a function", &ws, planned);
}

#[test]
fn inlining_a_variable_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    must_plan("inlining a variable", &ws, planned);
}

// ------------------------------------------ every command, the awkward crate

/// Here a plan is optional. A refusal is the right answer when a use site cannot be
/// verified. Writing a plan that does not compile is the only outcome this forbids.
#[test]
fn no_command_writes_a_broken_workspace() {
    let mut refused = Vec::new();
    for (what, plan) in [
        (
            "rename",
            Box::new(|ws: &Workspace, index: &Index| {
                let _ = ws;
                fun_refactor::refactor::rename::plan(
                    index,
                    the_free_function(index, "width"),
                    "span_of",
                )
                .map(|p| p.edits)
            }) as Box<dyn Fn(&Workspace, &Index) -> anyhow::Result<EditSet>>,
        ),
        (
            "move",
            Box::new(|ws: &Workspace, index: &Index| {
                fun_refactor::refactor::move_symbol::to_file(
                    index,
                    the_free_function(index, "width"),
                    &ws.path("src/util.rs"),
                )
                .map(|p| p.edits)
            }),
        ),
        (
            "signature",
            Box::new(|ws: &Workspace, index: &Index| {
                let _ = ws;
                fun_refactor::refactor::signature::change(
                    index,
                    the_free_function(index, "width"),
                    fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
                )
                .map(|p| p.edits)
            }),
        ),
        (
            "imports",
            Box::new(|ws: &Workspace, index: &Index| {
                fun_refactor::refactor::imports::plan(index, &ws.path("src/util.rs"))
                    .map(|p| p.edits)
            }),
        ),
    ] {
        let ws = Workspace::new(&awkward());
        let index = ws.index();
        if !gate(what, &ws, plan(&ws, &index)) {
            refused.push(what);
        }
    }
    eprintln!("refused on the awkward crate: {refused:?}");
}

/// What this gate covers, said out loud.
///
/// A gate that silently checks nothing is worse than no gate. This names the languages
/// it drives and the languages it does not, so a green run cannot be mistaken for
/// coverage it does not have.
#[test]
fn the_gate_states_what_it_covers() {
    assert!(
        rustc_is_available(),
        "cargo is not on PATH, so every case in this file checked nothing"
    );
    eprintln!("compile gate: rust — cargo check --all-targets, every command that writes");
    for (language, probe) in [("typescript", "tsc"), ("go", "go"), ("python", "python3")] {
        let available = Command::new(probe)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        eprintln!(
            "compile gate: {language} — not driven yet (its compiler is {})",
            match available {
                true => "installed here",
                false => "absent here",
            }
        );
    }
}
