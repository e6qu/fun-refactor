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
#[derive(Clone, Copy, PartialEq)]
enum Toolchain {
    Cargo,
    Tsc,
    Go,
    Python,
    Zig,
    Javac,
}

impl Toolchain {
    /// The program that has to be on `PATH` for this toolchain to run.
    fn program(&self) -> &'static str {
        match self {
            Toolchain::Cargo => "cargo",
            Toolchain::Tsc => "tsc",
            Toolchain::Go => "go",
            Toolchain::Python => "python3",
            Toolchain::Zig => "zig",
            Toolchain::Javac => "javac",
        }
    }

    fn is_available(&self) -> bool {
        // `go --version` is an error; the subcommand is `go version`. Asking the wrong
        // way reported Go as absent on a machine that has it, and every Go case skipped
        // itself while the run stayed green.
        let version_flag = match self {
            Toolchain::Go | Toolchain::Zig => "version",
            _ => "--version",
        };
        Command::new(self.program())
            .arg(version_flag)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// What this toolchain checks, in one line, for the report at the end of the file.
    fn covers(&self) -> &'static str {
        match self {
            Toolchain::Cargo => "cargo check --all-targets",
            Toolchain::Tsc => "tsc --noEmit",
            Toolchain::Go => "go build ./...",
            Toolchain::Python => "python -m compileall, then import and call the fixture",
            Toolchain::Zig => "zig build-lib, from the root that reaches every file",
            Toolchain::Javac => "javac over every source in the workspace",
        }
    }
}

struct Workspace {
    dir: tempfile::TempDir,
    toolchain: Toolchain,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Cargo, files)
    }

    fn typescript(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Tsc, files)
    }

    fn go(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Go, files)
    }

    fn python(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Python, files)
    }

    fn zig(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Zig, files)
    }

    fn java(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Javac, files)
    }

    fn with(toolchain: Toolchain, files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        for (name, content) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&path, content).expect("write");
        }
        Self { dir, toolchain }
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

    /// Compile the workspace with the compiler for its language.
    fn compiles(&self) -> Result<(), String> {
        let output = match self.toolchain {
            Toolchain::Cargo => Command::new("cargo")
                .args(["check", "--quiet", "--all-targets"])
                .current_dir(self.dir.path())
                // Its own build directory. Sharing one across the cases in this file made
                // the result depend on what another case had just built, and the check that
                // this gate can fail passed alone and failed in the suite.
                .env("CARGO_TARGET_DIR", self.dir.path().join("target"))
                .env("RUSTFLAGS", "-A warnings")
                .output()
                .expect("cargo runs"),
            Toolchain::Tsc => Command::new("tsc")
                .args(["--noEmit", "--project", "."])
                .current_dir(self.dir.path())
                .output()
                .expect("tsc runs"),
            Toolchain::Go => Command::new("go")
                .args(["build", "./..."])
                .current_dir(self.dir.path())
                // Its own build and module cache, for the reason the Rust arm has one.
                .env("GOCACHE", self.dir.path().join("gocache"))
                .env("GOFLAGS", "-mod=mod")
                .output()
                .expect("go runs"),
            // Python states nothing until it runs, so compiling every file is only the
            // first half. Importing the fixture resolves every `from … import …` against
            // what the module now exports, and calling into it runs the code the edit
            // changed. A rename that missed a call site is an error in neither half of
            // the language and shows up in the second half of this.
            Toolchain::Python => {
                let compiled = Command::new("python3")
                    .args(["-m", "compileall", "-q", "."])
                    .current_dir(self.dir.path())
                    .output()
                    .expect("python3 runs");
                if !compiled.status.success() {
                    return Err(format!(
                        "{}{}",
                        String::from_utf8_lossy(&compiled.stdout),
                        String::from_utf8_lossy(&compiled.stderr)
                    ));
                }
                Command::new("python3")
                    .args(["-c", "import main; main.check()"])
                    .current_dir(self.dir.path())
                    .output()
                    .expect("python3 runs")
            }
            // Zig analyses what the root reaches and nothing else, so the fixture's root
            // calls into every file for there to be anything to check. `-fno-emit-bin`
            // keeps the artefact out of the workspace the next index would scan.
            Toolchain::Zig => Command::new("zig")
                .args(["build-lib", "main.zig", "-fno-emit-bin"])
                .arg("--cache-dir")
                .arg(self.dir.path().join("zig-cache"))
                .current_dir(self.dir.path())
                .output()
                .expect("zig runs"),
            // Every source named at once, because javac resolves across the set it is
            // given and would otherwise read a stale class file for a file it was not.
            Toolchain::Javac => {
                let mut sources: Vec<PathBuf> = std::fs::read_dir(self.dir.path())
                    .expect("read_dir")
                    .filter_map(|entry| {
                        let path = entry.ok()?.path();
                        (path.extension()? == "java").then_some(path)
                    })
                    .collect();
                sources.sort();
                Command::new("javac")
                    .arg("-d")
                    .arg(self.dir.path().join("classes"))
                    .args(&sources)
                    .current_dir(self.dir.path())
                    .output()
                    .expect("javac runs")
            }
        };
        match output.status.success() {
            true => Ok(()),
            // tsc reports on stdout.
            false => Err(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).expect("read")
    }
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

#[test]
fn extracting_an_expression_from_inside_a_closure_compiles_or_refuses() {
    // The binding goes at the start of the enclosing statement, and that statement is
    // outside the closure. `i` exists only inside it.
    let mut files = plain();
    files.push((
        "src/counts.rs",
        "use crate::holder::Holder;\n\n\
         pub fn wide(h: &Holder) -> Vec<u8> {\n    \
         h.items.iter().filter(|i| **i > 1 && **i < 9).copied().collect()\n}\n",
    ));
    files[1] = (
        "src/lib.rs",
        "pub mod holder;\npub mod util;\npub mod counts;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
    );
    let ws = Workspace::new(&files);
    let index = ws.index();
    let source = ws.read("src/counts.rs");
    let at = source.find("**i > 1 && **i < 9").expect("the expression");
    let span = fun_refactor::span::Span::new(at, at + "**i > 1 && **i < 9".len());
    let planned = fun_refactor::refactor::extract::variable(
        &index,
        &ws.path("src/counts.rs"),
        span,
        "wide_enough",
        false,
    )
    .map(|p| p.edits);
    gate("extracting from inside a closure", &ws, planned);
}

#[test]
fn a_guard_clause_in_a_function_that_returns_a_value_compiles_or_refuses() {
    // An early exit needs a value here, and a bare `return` does not compile. The `if`
    // is nested deeply enough that a bounded walk never reaches the function.
    let mut files = plain();
    files.push((
        "src/deep.rs",
        "pub fn pick(a: bool, b: bool) -> u8 {\n    match a {\n        true => {\n            \
         {\n                if b {\n                    let _ = 1;\n                }\n            }\n        \
         }\n        false => {}\n    }\n    7\n}\n",
    ));
    files[1] = (
        "src/lib.rs",
        "pub mod holder;\npub mod util;\npub mod deep;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
    );
    let ws = Workspace::new(&files);
    let index = ws.index();
    let source = ws.read("src/deep.rs");
    let at = source.find("if b {").expect("the if");
    let planned = fun_refactor::refactor::rewrite::apply(
        &index,
        &ws.path("src/deep.rs"),
        at,
        fun_refactor::refactor::rewrite::Rewrite::GuardClause,
    )
    .map(|p| p.edits);
    gate(
        "a guard clause in a function that returns a value",
        &ws,
        planned,
    );
}

// ------------------------------------------------------- TypeScript

fn tsc_is_available() -> bool {
    Command::new("tsc")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The same shapes as the Rust fixture, in TypeScript: a free function and a method of
/// one name, an import of the function from another module, and a caller of both.
fn typescript() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "tsconfig.json",
            // `moduleResolution: node` was removed in a later TypeScript, and CI has one.
            // `bundler` is accepted by every version that has it and needs no package
            // layout on disk.
            "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
        ),
        (
            "src/holder.ts",
            "export function width(items: number[], n: number): number {\n               return items.length + n;\n}\n\n             export class Holder {\n  items: number[] = [];\n\n               width(n: number): number {\n    return width(this.items, n);\n  }\n}\n",
        ),
        (
            "src/util.ts",
            "import { width, Holder } from \"./holder\";\n\n             export function describe(h: Holder): string {\n               const total = width(h.items, 1);\n  return `${total * 2}`;\n}\n",
        ),
        (
            "src/index.ts",
            "export { width, Holder } from \"./holder\";\nexport { describe } from \"./util\";\n",
        ),
        (
            "src/main.ts",
            "import { Holder, width } from \"./index\";\nimport { describe } from \"./util\";\n\nconst h = new Holder();\nconsole.log(describe(h), h.width(1), width(h.items, 2));\n",
        ),
    ]
}

#[test]
fn the_typescript_fixture_compiles_before_anything_touches_it() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    if let Err(e) = ws.compiles() {
        panic!("the TypeScript fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_typescript_function_compiles() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a TypeScript function", &ws, planned);
}

#[test]
fn organising_typescript_imports_compiles() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("src/main.ts")).map(|p| p.edits);
    // Organising an import list that is already tidy may plan nothing.
    match planned {
        Ok(edits) if edits.is_empty() => {}
        other => {
            gate("organising TypeScript imports", &ws, other);
        }
    }
}

#[test]
fn moving_a_typescript_symbol_compiles_or_refuses() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    gate("moving a TypeScript symbol", &ws, planned);
}

#[test]
fn changing_a_typescript_signature_compiles_or_refuses() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    gate("changing a TypeScript signature", &ws, planned);
}

/// A barrel that hands on everything the file next to it exports.
///
/// `export * from "./holder"` names nothing, so nothing about the statement says which
/// symbols travel through it. Moving one out left the star naming a file that no longer
/// had it, and every reader of the barrel gained a second binding of the same name.
fn typescript_star_barrel() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "tsconfig.json",
            "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
        ),
        (
            "src/holder.ts",
            "export function width(items: number[], n: number): number {\n  return items.length + n;\n}\n\nexport const version = 1;\n",
        ),
        ("src/util.ts", "export const other = 1;\n"),
        ("src/index.ts", "export * from \"./holder\";\n"),
        (
            "src/main.ts",
            "import { width } from \"./index\";\nconsole.log(width([1], 2));\n",
        ),
    ]
}

#[test]
fn moving_out_from_under_a_named_barrel_compiles() {
    if !Toolchain::Tsc.is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    must_plan("moving out from under a named barrel", &ws, planned);
    let barrel = ws.read("src/index.ts");
    assert!(
        barrel.contains("width") && barrel.contains("./util"),
        "the barrel still has to hand the name on:\n{barrel}"
    );
    assert!(
        barrel.contains("Holder"),
        "and it must not drop the names beside it:\n{barrel}"
    );
}

#[test]
fn moving_out_from_under_a_star_barrel_compiles() {
    if !Toolchain::Tsc.is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript_star_barrel());
    if let Err(e) = ws.compiles() {
        panic!("the star barrel fixture is broken before any refactoring:\n{e}");
    }
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    must_plan("moving out from under a star barrel", &ws, planned);
    let reader = ws.read("src/main.ts");
    assert_eq!(
        reader.matches("import").count(),
        1,
        "the reader already had the name through the barrel:\n{reader}"
    );
}

// ------------------------------------------------------------------- Go

/// The same shapes again in Go: a free function and a method sharing a name, an import
/// of the function from another package, and a caller of both.
fn go_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module gate\n\ngo 1.21\n"),
        (
            "holder/holder.go",
            "package holder\n\n\
             type Holder struct {\n\tItems []byte\n}\n\n\
             func Width(items []byte, n int) int {\n\treturn len(items) + n\n}\n\n\
             func (h *Holder) Width(n int) int {\n\treturn Width(h.Items, n)\n}\n",
        ),
        (
            "util/util.go",
            "package util\n\n\
             import (\n\t\"fmt\"\n\n\t\"gate/holder\"\n)\n\n\
             func Describe(h *holder.Holder) string {\n\t\
             total := holder.Width(h.Items, 1)\n\treturn fmt.Sprintf(\"%d\", total*2)\n}\n",
        ),
        (
            "main.go",
            "package main\n\n\
             import (\n\t\"fmt\"\n\n\t\"gate/holder\"\n\t\"gate/util\"\n)\n\n\
             func main() {\n\t\
             h := &holder.Holder{Items: []byte{1, 2}}\n\t\
             fmt.Println(util.Describe(h), h.Width(1), holder.Width(h.Items, 2))\n}\n",
        ),
    ]
}

#[test]
fn the_go_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    if let Err(e) = ws.compiles() {
        panic!("the Go fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_go_function_compiles() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "SpanOf").map(|p| p.edits);
    must_plan("renaming a Go function", &ws, planned);
}

#[test]
fn changing_a_go_signature_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    gate("changing a Go signature", &ws, planned);
}

#[test]
fn moving_a_go_symbol_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned =
        fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util/util.go"))
            .map(|p| p.edits);
    gate("moving a Go symbol", &ws, planned);
}

#[test]
fn organising_go_imports_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("main.go")).map(|p| p.edits);
    match planned {
        Ok(edits) if edits.is_empty() => {}
        other => {
            gate("organising Go imports", &ws, other);
        }
    }
}

// --------------------------------------------------------------- Python

/// The same shapes in Python. `check` is what the toolchain calls, so the assertions in
/// it are the behaviour a refactoring has to preserve and not only the names.
fn python_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "holder.py",
            "def width(items, n):\n    return len(items) + n\n\n\n\
             class Holder:\n    def __init__(self):\n        self.items = []\n\n    \
             def width(self, n):\n        return width(self.items, n)\n",
        ),
        (
            "util.py",
            "from holder import Holder, width\n\n\n\
             def describe(h: Holder) -> str:\n    \
             total = width(h.items, 1)\n    return f\"{total * 2}\"\n",
        ),
        (
            "main.py",
            "from holder import Holder, width\nfrom util import describe\n\n\n\
             def check():\n    h = Holder()\n    h.items = [1, 2]\n    \
             assert describe(h) == \"6\", describe(h)\n    \
             assert h.width(1) == 3, h.width(1)\n    \
             assert width(h.items, 2) == 4, width(h.items, 2)\n",
        ),
    ]
}

#[test]
fn the_python_fixture_runs_before_anything_touches_it() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    if let Err(e) = ws.compiles() {
        panic!("the Python fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_python_function_compiles() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "span_of").map(|p| p.edits);
    must_plan("renaming a Python function", &ws, planned);
}

#[test]
fn changing_a_python_signature_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    gate("changing a Python signature", &ws, planned);
}

#[test]
fn moving_a_python_symbol_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util.py"))
        .map(|p| p.edits);
    gate("moving a Python symbol", &ws, planned);
}

#[test]
fn inlining_a_python_variable_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    gate("inlining a Python variable", &ws, planned);
}

// ------------------------------------------------------------------ Zig

/// The same shapes again in Zig: a free function and a method of one name, a second file
/// importing the first, and a root that calls both.
///
/// The root matters more here than in the other languages. Zig analyses what it can reach
/// from the file it is given and leaves the rest alone, so a declaration nothing calls is
/// never type-checked and a gate pointed at it would pass on anything.
fn zig_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "holder.zig",
            "pub const Holder = struct {\n    items: []const u8,\n\n    \
             pub fn width(self: Holder, n: usize) usize {\n        \
             return self.items.len + n;\n    }\n};\n\n\
             pub fn width(items: []const u8, n: usize) usize {\n    return items.len + n;\n}\n",
        ),
        (
            "util.zig",
            "const holder = @import(\"holder.zig\");\n\n\
             pub fn describe(h: holder.Holder) usize {\n    \
             const total = holder.width(h.items, 1);\n    return total * 2;\n}\n",
        ),
        (
            "main.zig",
            "const holder = @import(\"holder.zig\");\nconst util = @import(\"util.zig\");\n\n\
             pub fn main() void {\n    const h = holder.Holder{ .items = \"ab\" };\n    \
             _ = util.describe(h);\n    _ = h.width(1);\n    \
             _ = holder.width(h.items, 2);\n}\n",
        ),
    ]
}

#[test]
fn the_zig_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    if let Err(e) = ws.compiles() {
        panic!("the Zig fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_zig_function_compiles() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a Zig function", &ws, planned);
}

#[test]
fn changing_a_zig_signature_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    gate("changing a Zig signature", &ws, planned);
}

#[test]
fn moving_a_zig_symbol_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util.zig"))
        .map(|p| p.edits);
    gate("moving a Zig symbol", &ws, planned);
}

#[test]
fn inlining_a_zig_variable_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    gate("inlining a Zig variable", &ws, planned);
}

// ----------------------------------------------------------------- Java

/// The same shapes in Java, where they land differently: a class has no top level, so the
/// free function is a static method on a class of its own and the method is on the record.
fn java_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Widths.java",
            "public class Widths {\n    public static int width(byte[] items, int n) {\n        \
             return items.length + n;\n    }\n}\n",
        ),
        (
            "Holder.java",
            "public class Holder {\n    public byte[] items = new byte[0];\n\n    \
             public int width(int n) {\n        return Widths.width(items, n);\n    }\n}\n",
        ),
        (
            "Describe.java",
            "public class Describe {\n    public static String describe(Holder h) {\n        \
             int total = Widths.width(h.items, 1);\n        \
             return String.valueOf(total * 2);\n    }\n}\n",
        ),
        (
            "Main.java",
            "public class Main {\n    public static void main(String[] args) {\n        \
             Holder h = new Holder();\n        h.items = new byte[] { 1, 2 };\n        \
             System.out.println(\n            Describe.describe(h) + h.width(1) + \
             Widths.width(h.items, 2));\n    }\n}\n",
        ),
    ]
}

#[test]
fn the_java_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    if let Err(e) = ws.compiles() {
        panic!("the Java fixture is broken before any refactoring:\n{e}");
    }
}

/// The static method, which is the free function's counterpart here.
fn the_static_method(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name && s.qualifier.as_deref() == Some("Widths"))
        .unwrap_or_else(|| panic!("no static method named {name} on Widths"))
        .id
}

#[test]
fn renaming_a_java_method_compiles() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let id = the_static_method(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a Java method", &ws, planned);
}

#[test]
fn changing_a_java_signature_compiles_or_refuses() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let id = the_static_method(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    gate("changing a Java signature", &ws, planned);
}

#[test]
fn inlining_a_java_variable_compiles_or_refuses() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    gate("inlining a Java variable", &ws, planned);
}

/// The gate has to be able to fail.
///
/// Every other test here passes when the tool behaves. That is also what they would do if
/// the compiler stopped running: a wrong path, a missing project file, an exit code
/// nobody reads. This breaks each fixture on purpose and checks the compiler says so.
#[test]
fn the_gate_reports_a_workspace_that_does_not_compile() {
    let rust = Workspace::new(&plain());
    std::fs::write(
        rust.path("src/util.rs"),
        "pub fn describe() -> String {\n    no_such_function()\n}\n",
    )
    .expect("write");
    assert!(
        rust.compiles().is_err(),
        "cargo check accepted a call to a function that does not exist"
    );

    if !tsc_is_available() {
        eprintln!("compile gate: the TypeScript half of this check was skipped, tsc is absent");
        return;
    }
    let ts = Workspace::typescript(&typescript());
    std::fs::write(
        ts.path("src/util.ts"),
        "import { nothing } from \"./holder\";\nexport const x = nothing;\n",
    )
    .expect("write");
    assert!(
        ts.compiles().is_err(),
        "tsc accepted an import of a name that is not exported"
    );

    if Toolchain::Go.is_available() {
        let go = Workspace::go(&go_files());
        std::fs::write(
            go.path("util/util.go"),
            "package util\n\nimport \"gate/holder\"\n\n\
             func Describe(h *holder.Holder) string {\n\treturn holder.NoSuchFunction(h)\n}\n",
        )
        .expect("write");
        assert!(
            go.compiles().is_err(),
            "go build accepted a call to a function that does not exist"
        );
    } else {
        eprintln!("compile gate: the Go half of this check was skipped, go is absent");
    }

    if Toolchain::Python.is_available() {
        // Python compiles a name that does not exist and fails when it is read, which is
        // exactly why this toolchain runs the fixture as well as compiling it.
        let python = Workspace::python(&python_files());
        std::fs::write(
            python.path("util.py"),
            "from holder import Holder, no_such_function\n\n\n\
             def describe(h: Holder) -> str:\n    return no_such_function(h)\n",
        )
        .expect("write");
        assert!(
            python.compiles().is_err(),
            "importing a name that the module does not define was accepted"
        );

        // And the half that only running catches: every name still resolves, and the
        // answer changed.
        let changed = Workspace::python(&python_files());
        std::fs::write(
            changed.path("holder.py"),
            "def width(items, n):\n    return len(items) + n + 1\n\n\n\
             class Holder:\n    def __init__(self):\n        self.items = []\n\n    \
             def width(self, n):\n        return width(self.items, n)\n",
        )
        .expect("write");
        assert!(
            changed.compiles().is_err(),
            "the fixture's assertions did not notice a changed answer"
        );
    } else {
        eprintln!("compile gate: the Python half of this check was skipped, python3 is absent");
    }

    if Toolchain::Zig.is_available() {
        let zig = Workspace::zig(&zig_files());
        std::fs::write(
            zig.path("util.zig"),
            "const holder = @import(\"holder.zig\");\n\n\
             pub fn describe(h: holder.Holder) usize {\n    \
             return holder.noSuchFunction(h);\n}\n",
        )
        .expect("write");
        assert!(
            zig.compiles().is_err(),
            "zig accepted a call to a declaration the imported file does not have"
        );
    } else {
        eprintln!("compile gate: the Zig half of this check was skipped, zig is absent");
    }

    if Toolchain::Javac.is_available() {
        let java = Workspace::java(&java_files());
        std::fs::write(
            java.path("Describe.java"),
            "public class Describe {\n    public static String describe(Holder h) {\n        \
             return Widths.noSuchMethod(h);\n    }\n}\n",
        )
        .expect("write");
        assert!(
            java.compiles().is_err(),
            "javac accepted a call to a method that does not exist"
        );
    } else {
        eprintln!("compile gate: the Java half of this check was skipped, javac is absent");
    }
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
    for (language, toolchain) in [
        ("rust", Toolchain::Cargo),
        ("typescript", Toolchain::Tsc),
        ("go", Toolchain::Go),
        ("python", Toolchain::Python),
        ("zig", Toolchain::Zig),
        ("java", Toolchain::Javac),
    ] {
        eprintln!(
            "compile gate: {language} — {} ({})",
            toolchain.covers(),
            match toolchain.is_available() {
                true => "ran here",
                false => "skipped, its toolchain is absent here",
            }
        );
    }
    // The languages this file has no fixture for. Naming them is the point: a green run
    // covers six languages out of sixteen, and saying so keeps it from reading as more.
    // The ten below have no compiler to run — a stylesheet, a manifest and a document
    // are checked by parsing them, which the edit engine already does.
    eprintln!(
        "compile gate: not driven — bash, html, css, scss, hcl, yaml, helm, xml, markdown, tsx"
    );
}
