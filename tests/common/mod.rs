//! The compile gate's harness: a workspace on disk, and the rule every case obeys.
//!
//! Shared because two files drive it. `output_compiles.rs` puts the commands that move a
//! declaration through it — rename, signature, move, inline — and `rewrites_compile.rs`
//! puts the commands that rewrite one in place. One harness, so a language added to it is
//! added for both, and the rule about what may reach disk is stated once.

#![allow(dead_code)]

use fun_refactor::edit::EditSet;
use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;
use std::process::Command;

/// Hold a gate to its coverage, where a hole would otherwise be invisible.
///
/// Each gate file prints the tools it drove and the ones it skipped. Under `cargo test`
/// that output is captured, so on CI nobody ever sees it: a validator that is not
/// installed skips its cases, says so into a void, and the run goes green looking exactly
/// like one that checked everything.
///
/// So the rule differs by where it runs. On a laptop a missing tool is ordinary and the
/// line is a note. On CI it is a hole in the build, and this fails instead.
pub fn require_on_ci(what: &str, missing: &[String]) {
    if missing.is_empty() || std::env::var("CI").is_err() {
        return;
    }
    panic!(
        "{what}: {} not installed on CI, so those cases checked nothing: {}",
        missing.len(),
        missing.join(", ")
    );
}

/// A workspace on disk that can be indexed, edited and compiled.
#[derive(Clone, Copy, PartialEq)]
pub enum Toolchain {
    Cargo,
    Tsc,
    Go,
    Python,
    Zig,
    Javac,
    /// `bash -n` and shellcheck: a script that does not parse is not a script.
    Bash,
    /// `terraform validate`, which reads references and not only syntax.
    Terraform,
    /// `helm lint`, which renders the chart and checks it against Kubernetes' schemas.
    Helm,
    Xmllint,
    /// The same tool in HTML mode, where the exit code says nothing and the output does.
    XmllintHtml,
}

impl Toolchain {
    /// The program that has to be on `PATH` for this toolchain to run.
    pub fn program(&self) -> &'static str {
        match self {
            Toolchain::Cargo => "cargo",
            Toolchain::Tsc => "tsc",
            Toolchain::Go => "go",
            Toolchain::Python => "python3",
            Toolchain::Zig => "zig",
            Toolchain::Javac => "javac",
            Toolchain::Bash => "bash",
            Toolchain::Terraform => "terraform",
            Toolchain::Helm => "helm",
            Toolchain::Xmllint | Toolchain::XmllintHtml => "xmllint",
        }
    }

    pub fn is_available(&self) -> bool {
        // `go --version` is an error; the subcommand is `go version`. Asking the wrong
        // way reported Go as absent on a machine that has it, and every Go case skipped
        // itself while the run stayed green.
        let version_flag = match self {
            Toolchain::Go | Toolchain::Zig | Toolchain::Terraform | Toolchain::Helm => "version",
            _ => "--version",
        };
        Command::new(self.program())
            .arg(version_flag)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// What this toolchain checks, in one line, for the report at the end of the file.
    pub fn covers(&self) -> &'static str {
        match self {
            Toolchain::Cargo => "cargo check --all-targets",
            Toolchain::Tsc => "tsc --noEmit",
            Toolchain::Go => "go build ./...",
            Toolchain::Python => "python -m compileall, then import and call the fixture",
            Toolchain::Zig => "zig build-lib, from the root that reaches every file",
            Toolchain::Javac => "javac over every source in the workspace",
            Toolchain::Bash => "bash -n, shellcheck, then the script itself",
            Toolchain::Terraform => "terraform validate, which resolves references",
            Toolchain::Helm => "helm lint, which renders the chart and checks the schemas",
            Toolchain::Xmllint => "xmllint, for well-formedness",
            Toolchain::XmllintHtml => {
                "xmllint --html, whose report is its output and not its status"
            }
        }
    }
}

pub struct Workspace {
    dir: tempfile::TempDir,
    toolchain: Toolchain,
}

impl Workspace {
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Cargo, files)
    }

    pub fn typescript(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Tsc, files)
    }

    pub fn go(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Go, files)
    }

    pub fn python(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Python, files)
    }

    pub fn zig(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Zig, files)
    }

    pub fn java(files: &[(&str, &str)]) -> Self {
        Self::with(Toolchain::Javac, files)
    }

    pub fn with(toolchain: Toolchain, files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        for (name, content) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&path, content).expect("write");
        }
        Self { dir, toolchain }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn index(&self) -> Index {
        let scanned = scan(self.dir.path(), &ScanOptions::default()).expect("scan");
        Index::build_from_scan(&scanned).expect("index")
    }

    pub fn apply(&self, edits: &EditSet) {
        for (path, file_edits) in edits.iter() {
            let before = std::fs::read_to_string(path).unwrap_or_default();
            let after =
                fun_refactor::edit::apply_to_string(&before, file_edits).expect("the edits apply");
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(path, after).expect("write");
        }
    }

    /// Compile the workspace with the compiler for its language.
    pub fn compiles(&self) -> Result<(), String> {
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
            // Three passes. The shell's own parser and shellcheck at error severity —
            // a warning is style and an error is a script that will not run — and then
            // the script itself, because neither of the first two can see a call to a
            // function that moved to another file. `fr move` writes the `source` line
            // that makes it work, and only running the thing checks that it did.
            Toolchain::Bash => {
                for script in self.files_ending(".sh") {
                    for (program, args) in [
                        ("bash", vec!["-n".to_string()]),
                        ("shellcheck", vec!["-S".to_string(), "error".to_string()]),
                    ] {
                        if !Toolchain::Bash.is_available() && program == "bash" {
                            continue;
                        }
                        let run = Command::new(program)
                            .args(&args)
                            .arg(&script)
                            .current_dir(self.dir.path())
                            .output();
                        let Ok(run) = run else { continue };
                        if !run.status.success() {
                            return Err(format!(
                                "{}{}",
                                String::from_utf8_lossy(&run.stdout),
                                String::from_utf8_lossy(&run.stderr)
                            ));
                        }
                    }
                }
                // The entry script, which the fixture makes self-checking.
                let entry = self.dir.path().join("run.sh");
                if entry.exists() {
                    let run = Command::new("bash")
                        .arg(&entry)
                        .current_dir(self.dir.path())
                        .output()
                        .expect("bash runs");
                    if !run.status.success() {
                        return Err(format!(
                            "{}{}",
                            String::from_utf8_lossy(&run.stdout),
                            String::from_utf8_lossy(&run.stderr)
                        ));
                    }
                }
                return Ok(());
            }
            // `validate` needs the module initialised, and with no backend it needs no
            // credentials and reaches no network.
            Toolchain::Terraform => {
                let _ = Command::new("terraform")
                    .args(["init", "-backend=false", "-no-color"])
                    .current_dir(self.dir.path())
                    .output();
                Command::new("terraform")
                    .args(["validate", "-no-color"])
                    .current_dir(self.dir.path())
                    .output()
                    .expect("terraform runs")
            }
            Toolchain::Helm => Command::new("helm")
                .arg("lint")
                .arg(self.dir.path())
                .output()
                .expect("helm runs"),
            Toolchain::Xmllint => {
                for file in self.files_ending(".xml") {
                    let run = Command::new("xmllint")
                        .arg("--noout")
                        .arg(&file)
                        .current_dir(self.dir.path())
                        .output()
                        .expect("xmllint runs");
                    if !run.status.success() {
                        return Err(String::from_utf8_lossy(&run.stderr).to_string());
                    }
                }
                return Ok(());
            }
            // An HTML parser recovers from anything, so it exits 0 whatever it read. What
            // it *says* is the answer: silence means well formed.
            Toolchain::XmllintHtml => {
                for file in self.files_ending(".html") {
                    let run = Command::new("xmllint")
                        .args(["--noout", "--html"])
                        .arg(&file)
                        .current_dir(self.dir.path())
                        .output()
                        .expect("xmllint runs");
                    let said = String::from_utf8_lossy(&run.stderr).to_string();
                    if !said.trim().is_empty() {
                        return Err(said);
                    }
                }
                return Ok(());
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

    /// Every file in the workspace with this extension, deepest last.
    fn files_ending(&self, suffix: &str) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![self.dir.path().to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                match path.is_dir() {
                    true => stack.push(path),
                    false => {
                        if path.to_string_lossy().ends_with(suffix) {
                            out.push(path);
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// The workspace as the in-memory commands see it.
    pub fn sources(
        &self,
    ) -> std::collections::BTreeMap<PathBuf, (fun_refactor::lang::Language, String)> {
        let scanned = scan(self.dir.path(), &ScanOptions::default()).expect("scan");
        scanned
            .files
            .iter()
            .filter_map(|file| {
                let text = std::fs::read_to_string(&file.path).ok()?;
                Some((file.path.clone(), (file.language, text)))
            })
            .collect()
    }

    pub fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).expect("read")
    }
}

pub fn rustc_is_available() -> bool {
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
pub fn plain() -> Vec<(&'static str, &'static str)> {
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
pub fn awkward() -> Vec<(&'static str, &'static str)> {
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

pub fn the_free_function(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
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
pub fn gate(what: &str, ws: &Workspace, planned: anyhow::Result<EditSet>) -> bool {
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

pub fn must_plan(what: &str, ws: &Workspace, planned: anyhow::Result<EditSet>) {
    assert!(
        gate(what, ws, planned),
        "{what} refused on a workspace with nothing awkward in it"
    );
}
