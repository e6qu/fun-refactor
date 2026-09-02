//! Differential execution: a program translated is a program that still runs.

#![cfg(feature = "full-audit")]

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every cell that holds: `group source->target`, sorted.
const PASSING: &[&str] = &[
    "asynchrony go->java",
    "asynchrony go->lean",
    "asynchrony go->python",
    "asynchrony go->rust",
    "asynchrony go->typescript",
    "asynchrony go->zig",
    "asynchrony java->go",
    "asynchrony java->lean",
    "asynchrony java->python",
    "asynchrony java->rust",
    "asynchrony java->typescript",
    "asynchrony java->zig",
    "asynchrony lean->go",
    "asynchrony lean->java",
    "asynchrony lean->python",
    "asynchrony lean->rust",
    "asynchrony lean->typescript",
    "asynchrony lean->zig",
    "asynchrony python->go",
    "asynchrony python->java",
    "asynchrony python->lean",
    "asynchrony python->rust",
    "asynchrony python->typescript",
    "asynchrony python->zig",
    "asynchrony rust->go",
    "asynchrony rust->java",
    "asynchrony rust->lean",
    "asynchrony rust->python",
    "asynchrony rust->typescript",
    "asynchrony rust->zig",
    "asynchrony typescript->go",
    "asynchrony typescript->java",
    "asynchrony typescript->lean",
    "asynchrony typescript->python",
    "asynchrony typescript->rust",
    "asynchrony typescript->zig",
    "asynchrony zig->go",
    "asynchrony zig->java",
    "asynchrony zig->lean",
    "asynchrony zig->python",
    "asynchrony zig->rust",
    "asynchrony zig->typescript",
    "bindings bash->go",
    "bindings bash->java",
    "bindings bash->lean",
    "bindings bash->python",
    "bindings bash->rust",
    "bindings bash->typescript",
    "bindings bash->zig",
    "bindings go->bash",
    "bindings go->java",
    "bindings go->lean",
    "bindings go->python",
    "bindings go->rust",
    "bindings go->typescript",
    "bindings go->zig",
    "bindings java->bash",
    "bindings java->go",
    "bindings java->lean",
    "bindings java->python",
    "bindings java->rust",
    "bindings java->typescript",
    "bindings java->zig",
    "bindings lean->bash",
    "bindings lean->go",
    "bindings lean->java",
    "bindings lean->python",
    "bindings lean->rust",
    "bindings lean->typescript",
    "bindings lean->zig",
    "bindings python->bash",
    "bindings python->go",
    "bindings python->java",
    "bindings python->lean",
    "bindings python->rust",
    "bindings python->typescript",
    "bindings python->zig",
    "bindings rust->bash",
    "bindings rust->go",
    "bindings rust->java",
    "bindings rust->lean",
    "bindings rust->python",
    "bindings rust->typescript",
    "bindings rust->zig",
    "bindings typescript->bash",
    "bindings typescript->go",
    "bindings typescript->java",
    "bindings typescript->lean",
    "bindings typescript->python",
    "bindings typescript->rust",
    "bindings typescript->zig",
    "bindings zig->bash",
    "bindings zig->go",
    "bindings zig->java",
    "bindings zig->lean",
    "bindings zig->python",
    "bindings zig->rust",
    "bindings zig->typescript",
    "cleanup go->java",
    "cleanup go->lean",
    "cleanup go->python",
    "cleanup go->rust",
    "cleanup go->typescript",
    "cleanup go->zig",
    "cleanup java->go",
    "cleanup java->lean",
    "cleanup java->python",
    "cleanup java->rust",
    "cleanup java->typescript",
    "cleanup java->zig",
    "cleanup lean->go",
    "cleanup lean->java",
    "cleanup lean->python",
    "cleanup lean->rust",
    "cleanup lean->typescript",
    "cleanup lean->zig",
    "cleanup python->go",
    "cleanup python->java",
    "cleanup python->lean",
    "cleanup python->rust",
    "cleanup python->typescript",
    "cleanup python->zig",
    "cleanup rust->go",
    "cleanup rust->java",
    "cleanup rust->lean",
    "cleanup rust->python",
    "cleanup rust->typescript",
    "cleanup rust->zig",
    "cleanup typescript->go",
    "cleanup typescript->java",
    "cleanup typescript->lean",
    "cleanup typescript->python",
    "cleanup typescript->rust",
    "cleanup typescript->zig",
    "cleanup zig->go",
    "cleanup zig->java",
    "cleanup zig->lean",
    "cleanup zig->python",
    "cleanup zig->rust",
    "cleanup zig->typescript",
    "closures go->java",
    "closures go->lean",
    "closures go->python",
    "closures go->rust",
    "closures go->typescript",
    "closures go->zig",
    "closures java->go",
    "closures java->lean",
    "closures java->python",
    "closures java->rust",
    "closures java->typescript",
    "closures java->zig",
    "closures lean->go",
    "closures lean->java",
    "closures lean->python",
    "closures lean->rust",
    "closures lean->typescript",
    "closures lean->zig",
    "closures python->go",
    "closures python->java",
    "closures python->lean",
    "closures python->rust",
    "closures python->typescript",
    "closures python->zig",
    "closures rust->go",
    "closures rust->java",
    "closures rust->lean",
    "closures rust->python",
    "closures rust->typescript",
    "closures rust->zig",
    "closures typescript->go",
    "closures typescript->java",
    "closures typescript->lean",
    "closures typescript->python",
    "closures typescript->rust",
    "closures typescript->zig",
    "closures zig->go",
    "closures zig->java",
    "closures zig->lean",
    "closures zig->python",
    "closures zig->rust",
    "closures zig->typescript",
    "collections bash->go",
    "collections bash->java",
    "collections bash->lean",
    "collections bash->python",
    "collections bash->rust",
    "collections bash->typescript",
    "collections bash->zig",
    "collections go->bash",
    "collections go->java",
    "collections go->lean",
    "collections go->python",
    "collections go->rust",
    "collections go->typescript",
    "collections go->zig",
    "collections java->bash",
    "collections java->go",
    "collections java->lean",
    "collections java->python",
    "collections java->rust",
    "collections java->typescript",
    "collections java->zig",
    "collections lean->bash",
    "collections lean->go",
    "collections lean->java",
    "collections lean->python",
    "collections lean->rust",
    "collections lean->typescript",
    "collections lean->zig",
    "collections python->bash",
    "collections python->go",
    "collections python->java",
    "collections python->lean",
    "collections python->rust",
    "collections python->typescript",
    "collections python->zig",
    "collections rust->bash",
    "collections rust->go",
    "collections rust->java",
    "collections rust->lean",
    "collections rust->python",
    "collections rust->typescript",
    "collections rust->zig",
    "collections typescript->bash",
    "collections typescript->go",
    "collections typescript->java",
    "collections typescript->lean",
    "collections typescript->python",
    "collections typescript->rust",
    "collections typescript->zig",
    "collections zig->bash",
    "collections zig->go",
    "collections zig->java",
    "collections zig->lean",
    "collections zig->python",
    "collections zig->rust",
    "collections zig->typescript",
    "comprehensions go->java",
    "comprehensions go->lean",
    "comprehensions go->python",
    "comprehensions go->rust",
    "comprehensions go->typescript",
    "comprehensions go->zig",
    "comprehensions java->go",
    "comprehensions java->lean",
    "comprehensions java->python",
    "comprehensions java->rust",
    "comprehensions java->typescript",
    "comprehensions java->zig",
    "comprehensions lean->go",
    "comprehensions lean->java",
    "comprehensions lean->python",
    "comprehensions lean->rust",
    "comprehensions lean->typescript",
    "comprehensions lean->zig",
    "comprehensions python->go",
    "comprehensions python->java",
    "comprehensions python->lean",
    "comprehensions python->rust",
    "comprehensions python->typescript",
    "comprehensions python->zig",
    "comprehensions rust->go",
    "comprehensions rust->java",
    "comprehensions rust->lean",
    "comprehensions rust->python",
    "comprehensions rust->typescript",
    "comprehensions rust->zig",
    "comprehensions typescript->go",
    "comprehensions typescript->java",
    "comprehensions typescript->lean",
    "comprehensions typescript->python",
    "comprehensions typescript->rust",
    "comprehensions typescript->zig",
    "comprehensions zig->go",
    "comprehensions zig->java",
    "comprehensions zig->lean",
    "comprehensions zig->python",
    "comprehensions zig->rust",
    "comprehensions zig->typescript",
    "control bash->go",
    "control bash->java",
    "control bash->lean",
    "control bash->python",
    "control bash->rust",
    "control bash->typescript",
    "control bash->zig",
    "control go->bash",
    "control go->java",
    "control go->lean",
    "control go->python",
    "control go->rust",
    "control go->typescript",
    "control go->zig",
    "control java->bash",
    "control java->go",
    "control java->lean",
    "control java->python",
    "control java->rust",
    "control java->typescript",
    "control java->zig",
    "control lean->bash",
    "control lean->go",
    "control lean->java",
    "control lean->python",
    "control lean->rust",
    "control lean->typescript",
    "control lean->zig",
    "control python->bash",
    "control python->go",
    "control python->java",
    "control python->lean",
    "control python->rust",
    "control python->typescript",
    "control python->zig",
    "control rust->bash",
    "control rust->go",
    "control rust->java",
    "control rust->lean",
    "control rust->python",
    "control rust->typescript",
    "control rust->zig",
    "control typescript->bash",
    "control typescript->go",
    "control typescript->java",
    "control typescript->lean",
    "control typescript->python",
    "control typescript->rust",
    "control typescript->zig",
    "control zig->bash",
    "control zig->go",
    "control zig->java",
    "control zig->lean",
    "control zig->python",
    "control zig->rust",
    "control zig->typescript",
    "dispatch go->java",
    "dispatch go->lean",
    "dispatch go->python",
    "dispatch go->rust",
    "dispatch go->typescript",
    "dispatch go->zig",
    "dispatch java->go",
    "dispatch java->lean",
    "dispatch java->python",
    "dispatch java->rust",
    "dispatch java->typescript",
    "dispatch java->zig",
    "dispatch lean->go",
    "dispatch lean->java",
    "dispatch lean->python",
    "dispatch lean->rust",
    "dispatch lean->typescript",
    "dispatch lean->zig",
    "dispatch python->go",
    "dispatch python->java",
    "dispatch python->lean",
    "dispatch python->rust",
    "dispatch python->typescript",
    "dispatch python->zig",
    "dispatch rust->go",
    "dispatch rust->java",
    "dispatch rust->lean",
    "dispatch rust->python",
    "dispatch rust->typescript",
    "dispatch rust->zig",
    "dispatch typescript->go",
    "dispatch typescript->java",
    "dispatch typescript->lean",
    "dispatch typescript->python",
    "dispatch typescript->rust",
    "dispatch typescript->zig",
    "dispatch zig->go",
    "dispatch zig->java",
    "dispatch zig->lean",
    "dispatch zig->python",
    "dispatch zig->rust",
    "dispatch zig->typescript",
    "errors go->java",
    "errors go->lean",
    "errors go->python",
    "errors go->rust",
    "errors go->typescript",
    "errors go->zig",
    "errors java->go",
    "errors java->lean",
    "errors java->python",
    "errors java->rust",
    "errors java->typescript",
    "errors java->zig",
    "errors lean->go",
    "errors lean->java",
    "errors lean->python",
    "errors lean->rust",
    "errors lean->typescript",
    "errors lean->zig",
    "errors python->go",
    "errors python->java",
    "errors python->lean",
    "errors python->rust",
    "errors python->typescript",
    "errors python->zig",
    "errors rust->go",
    "errors rust->java",
    "errors rust->lean",
    "errors rust->python",
    "errors rust->typescript",
    "errors rust->zig",
    "errors typescript->go",
    "errors typescript->java",
    "errors typescript->lean",
    "errors typescript->python",
    "errors typescript->rust",
    "errors typescript->zig",
    "errors zig->go",
    "errors zig->java",
    "errors zig->lean",
    "errors zig->python",
    "errors zig->rust",
    "errors zig->typescript",
    "generics go->java",
    "generics go->lean",
    "generics go->python",
    "generics go->rust",
    "generics go->typescript",
    "generics go->zig",
    "generics java->go",
    "generics java->lean",
    "generics java->python",
    "generics java->rust",
    "generics java->typescript",
    "generics java->zig",
    "generics lean->go",
    "generics lean->java",
    "generics lean->python",
    "generics lean->rust",
    "generics lean->typescript",
    "generics lean->zig",
    "generics python->go",
    "generics python->java",
    "generics python->lean",
    "generics python->rust",
    "generics python->typescript",
    "generics python->zig",
    "generics rust->go",
    "generics rust->java",
    "generics rust->lean",
    "generics rust->python",
    "generics rust->typescript",
    "generics rust->zig",
    "generics typescript->go",
    "generics typescript->java",
    "generics typescript->lean",
    "generics typescript->python",
    "generics typescript->rust",
    "generics typescript->zig",
    "generics zig->go",
    "generics zig->java",
    "generics zig->lean",
    "generics zig->python",
    "generics zig->rust",
    "generics zig->typescript",
    "maps go->java",
    "maps go->lean",
    "maps go->python",
    "maps go->rust",
    "maps go->typescript",
    "maps go->zig",
    "maps java->go",
    "maps java->lean",
    "maps java->python",
    "maps java->rust",
    "maps java->typescript",
    "maps java->zig",
    "maps lean->go",
    "maps lean->java",
    "maps lean->python",
    "maps lean->rust",
    "maps lean->typescript",
    "maps lean->zig",
    "maps python->go",
    "maps python->java",
    "maps python->lean",
    "maps python->rust",
    "maps python->typescript",
    "maps python->zig",
    "maps rust->go",
    "maps rust->java",
    "maps rust->lean",
    "maps rust->python",
    "maps rust->typescript",
    "maps rust->zig",
    "maps typescript->go",
    "maps typescript->java",
    "maps typescript->lean",
    "maps typescript->python",
    "maps typescript->rust",
    "maps typescript->zig",
    "maps zig->go",
    "maps zig->java",
    "maps zig->lean",
    "maps zig->python",
    "maps zig->rust",
    "maps zig->typescript",
    "numbers go->java",
    "numbers go->lean",
    "numbers go->python",
    "numbers go->rust",
    "numbers go->typescript",
    "numbers go->zig",
    "numbers java->go",
    "numbers java->lean",
    "numbers java->python",
    "numbers java->rust",
    "numbers java->typescript",
    "numbers java->zig",
    "numbers lean->go",
    "numbers lean->java",
    "numbers lean->python",
    "numbers lean->rust",
    "numbers lean->typescript",
    "numbers lean->zig",
    "numbers python->go",
    "numbers python->java",
    "numbers python->lean",
    "numbers python->rust",
    "numbers python->typescript",
    "numbers python->zig",
    "numbers rust->go",
    "numbers rust->java",
    "numbers rust->lean",
    "numbers rust->python",
    "numbers rust->typescript",
    "numbers rust->zig",
    "numbers typescript->go",
    "numbers typescript->java",
    "numbers typescript->lean",
    "numbers typescript->python",
    "numbers typescript->rust",
    "numbers typescript->zig",
    "numbers zig->go",
    "numbers zig->java",
    "numbers zig->lean",
    "numbers zig->python",
    "numbers zig->rust",
    "numbers zig->typescript",
    "sets go->java",
    "sets go->lean",
    "sets go->python",
    "sets go->rust",
    "sets go->typescript",
    "sets go->zig",
    "sets java->go",
    "sets java->lean",
    "sets java->python",
    "sets java->rust",
    "sets java->typescript",
    "sets java->zig",
    "sets lean->go",
    "sets lean->java",
    "sets lean->python",
    "sets lean->rust",
    "sets lean->typescript",
    "sets lean->zig",
    "sets python->go",
    "sets python->java",
    "sets python->lean",
    "sets python->rust",
    "sets python->typescript",
    "sets python->zig",
    "sets rust->go",
    "sets rust->java",
    "sets rust->lean",
    "sets rust->python",
    "sets rust->typescript",
    "sets rust->zig",
    "sets typescript->go",
    "sets typescript->java",
    "sets typescript->lean",
    "sets typescript->python",
    "sets typescript->rust",
    "sets typescript->zig",
    "sets zig->go",
    "sets zig->java",
    "sets zig->lean",
    "sets zig->python",
    "sets zig->rust",
    "sets zig->typescript",
    "strings go->java",
    "strings go->lean",
    "strings go->python",
    "strings go->rust",
    "strings go->typescript",
    "strings go->zig",
    "strings java->go",
    "strings java->lean",
    "strings java->python",
    "strings java->rust",
    "strings java->typescript",
    "strings java->zig",
    "strings lean->go",
    "strings lean->java",
    "strings lean->python",
    "strings lean->rust",
    "strings lean->typescript",
    "strings lean->zig",
    "strings python->go",
    "strings python->java",
    "strings python->lean",
    "strings python->rust",
    "strings python->typescript",
    "strings python->zig",
    "strings rust->go",
    "strings rust->java",
    "strings rust->lean",
    "strings rust->python",
    "strings rust->typescript",
    "strings rust->zig",
    "strings typescript->go",
    "strings typescript->java",
    "strings typescript->lean",
    "strings typescript->python",
    "strings typescript->rust",
    "strings typescript->zig",
    "strings zig->go",
    "strings zig->java",
    "strings zig->lean",
    "strings zig->python",
    "strings zig->rust",
    "strings zig->typescript",
];

/// The languages of the suite, with how each names and runs its programs.
const LANGUAGES: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
    Language::Bash,
    Language::Lean,
];

fn extension(language: Language) -> &'static str {
    match language {
        Language::Rust => "rs",
        Language::Go => "go",
        Language::Java => "java",
        Language::Python => "py",
        Language::TypeScript => "ts",
        Language::Zig => "zig",
        Language::Bash => "sh",
        Language::Lean => "lean",
        other => panic!("{other} is not in the suite"),
    }
}

/// Does the suite read a native program in this language? A target with no reader has
/// none to read, and asking for one would be asking for a program nothing checks.
fn is_a_source(language: Language) -> bool {
    transpile::can_be_read(language)
}

/// The file stem a program takes.
fn stem(language: Language) -> &'static str {
    match language {
        Language::Java => "Main",
        _ => "main",
    }
}

/// The toolchain a language needs, probed once by asking its version.
fn toolchain(language: Language) -> (&'static str, &'static [&'static str]) {
    match language {
        Language::Rust => ("rustc", &["--version"]),
        Language::Go => ("go", &["version"]),
        Language::Java => ("java", &["--version"]),
        Language::Python => ("python3", &["--version"]),
        Language::TypeScript => ("tsc", &["--version"]),
        Language::Zig => ("zig", &["version"]),
        Language::Bash => ("bash", &["--version"]),
        Language::Lean => ("lean", &["--version"]),
        other => panic!("{other} is not in the suite"),
    }
}

fn available(language: Language) -> bool {
    let (binary, args) = toolchain(language);
    Command::new(binary)
        .args(args)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Compile and run one program, returning its stdout.
fn run_program(language: Language, file: &Path, scratch: &Path) -> Result<String, String> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).to_string();
    let finish = |label: &str, out: std::process::Output| -> Result<String, String> {
        match out.status.success() {
            true => Ok(text(&out.stdout)),
            false => Err(format!(
                "{label} failed:\n{}\n{}",
                text(&out.stdout),
                text(&out.stderr)
            )),
        }
    };
    match language {
        Language::Rust => {
            let binary = scratch.join("program");
            let compile = Command::new("rustc")
                .args(["-o"])
                .arg(&binary)
                .arg(file)
                .arg("--edition=2021")
                .output()
                .map_err(|e| e.to_string())?;
            if !compile.status.success() {
                return Err(format!("rustc failed:\n{}", text(&compile.stderr)));
            }
            finish(
                "run",
                Command::new(&binary).output().map_err(|e| e.to_string())?,
            )
        }
        Language::Go => finish(
            "go run",
            Command::new("go")
                .arg("run")
                .arg(file)
                .env("GO111MODULE", "off")
                .output()
                .map_err(|e| e.to_string())?,
        ),
        Language::Java => finish(
            "java",
            Command::new("java")
                .arg(file)
                .output()
                .map_err(|e| e.to_string())?,
        ),
        Language::Python => finish(
            "python3",
            Command::new("python3")
                .arg(file)
                .output()
                .map_err(|e| e.to_string())?,
        ),
        Language::TypeScript => {
            // tsc writes the JavaScript beside its input, so this copies the input into the
            // scratch directory first.
            let copied = scratch.join("main.ts");
            if file != copied {
                std::fs::copy(file, &copied).map_err(|e| e.to_string())?;
            }
            let compile = Command::new("tsc")
                .args(["--target", "es2020", "--module", "commonjs", "--strict"])
                .arg(&copied)
                .output()
                .map_err(|e| e.to_string())?;
            if !compile.status.success() {
                return Err(format!(
                    "tsc failed:\n{}\n{}",
                    text(&compile.stdout),
                    text(&compile.stderr)
                ));
            }
            finish(
                "node",
                Command::new("node")
                    .arg(scratch.join("main.js"))
                    .output()
                    .map_err(|e| e.to_string())?,
            )
        }
        Language::Zig => finish(
            "zig run",
            Command::new("zig")
                .arg("run")
                .arg(file)
                .output()
                .map_err(|e| e.to_string())?,
        ),
        Language::Bash => finish(
            "bash",
            Command::new("bash")
                .arg(file)
                .output()
                .map_err(|e| e.to_string())?,
        ),
        // `--run` elaborates the file and then calls its `main`, which is the whole of
        // running a Lean program without a `lake` project around it.
        Language::Lean => finish(
            "lean --run",
            Command::new("lean")
                .arg("--run")
                .arg(file)
                .output()
                .map_err(|e| e.to_string())?,
        ),
        other => panic!("{other} is not in the suite"),
    }
}

fn groups() -> Vec<PathBuf> {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/conformance"));
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("the conformance directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    assert!(!found.is_empty(), "the conformance suite is gone");
    found
}

#[test]
fn every_translation_still_prints_the_same_transcript() {
    let have: Vec<Language> = LANGUAGES
        .iter()
        .copied()
        .filter(|l| available(*l))
        .collect();
    let mut missing = Vec::new();
    for language in LANGUAGES {
        if !have.contains(language) {
            let (binary, _) = toolchain(*language);
            println!("conformance: {language} skipped, {binary} is not on PATH");
            missing.push(format!("{language} (`{binary}`)"));
        }
    }
    assert!(
        !have.is_empty(),
        "no toolchain at all; this run would check nothing"
    );
    // A toolchain absent from CI leaves its whole column unrun while the job goes
    // green. That is the shape of hole this suite exists to catch.
    common::require_on_ci("the conformance suite", &missing);

    let mut passing = BTreeSet::new();
    let mut failures = Vec::new();

    for group in groups() {
        let name = group.file_name().unwrap().to_string_lossy().to_string();

        let mut transcript: Option<(Language, String)> = None;
        for language in LANGUAGES {
            let file = group
                .join(stem(*language))
                .with_extension(extension(*language));
            if !is_a_source(*language) {
                continue;
            }
            // Bash writes its computational subset and sits the other groups out, out loud.
            if !file.exists() {
                assert_eq!(
                    *language,
                    Language::Bash,
                    "{name}: no {language} program at {}",
                    file.display()
                );
                println!("conformance: {name} has no {language} native; it sits this group out");
                continue;
            }
            if !have.contains(language) {
                continue;
            }
            let scratch = tempfile::tempdir().expect("a scratch directory");
            let out = run_program(*language, &file, scratch.path())
                .unwrap_or_else(|e| panic!("{name}: the native {language} program: {e}"));
            match &transcript {
                None => transcript = Some((*language, out)),
                Some((first, expected)) => assert_eq!(
                    expected, &out,
                    "{name}: the native {language} and {first} programs disagree"
                ),
            }
        }
        let Some((_, transcript)) = transcript else {
            continue;
        };

        // Every translation of every source, against that transcript.
        for source in LANGUAGES {
            if !is_a_source(*source) {
                continue;
            }
            let file = group.join(stem(*source)).with_extension(extension(*source));
            if !file.exists() {
                continue;
            }
            for target in LANGUAGES {
                if target == source || !have.contains(target) {
                    continue;
                }
                // A group bash sat out is not asked of it as a target either: the
                // programs use what the subset does not have.
                if *target == Language::Bash
                    && !group.join(stem(*target)).with_extension("sh").exists()
                {
                    continue;
                }
                let cell = format!("{name} {source}->{target}");
                let scratch = tempfile::tempdir().expect("a scratch directory");
                let out = scratch
                    .path()
                    .join(stem(*target))
                    .with_extension(extension(*target));
                let translated = match transpile::plan_to(&file, *target, Some(&out), false) {
                    Ok(plan) => plan,
                    Err(e) => {
                        failures.push(format!("{cell}: refused: {e}"));
                        continue;
                    }
                };
                std::fs::write(&out, &translated.output).expect("write the translation");
                match run_program(*target, &out, scratch.path()) {
                    Ok(printed) if printed == transcript => {
                        passing.insert(cell);
                    }
                    Ok(printed) => failures.push(format!(
                        "{cell}: ran but printed differently.\n--- expected\n{transcript}--- got\n{printed}"
                    )),
                    Err(e) => failures.push(format!("{cell}: {e}")),
                }
            }
        }
    }

    // What failed, one line each, so a run is its own worklist.
    for failure in &failures {
        let mut lines = failure.lines();
        let first = lines.next().unwrap_or_default();
        println!("conformance: {first}");
        if std::env::var("CONFORMANCE_DETAIL").is_ok() {
            for line in lines.take(30) {
                println!("    {line}");
            }
        }
    }

    // The ledger, ratcheted in both directions among the cells that could run.
    let pinned: BTreeSet<String> = PASSING.iter().map(|s| s.to_string()).collect();
    let mut wrong = Vec::new();
    for cell in &pinned {
        let target_of = |cell: &str| -> Option<Language> {
            let name = cell.rsplit("->").next()?;
            LANGUAGES.iter().copied().find(|l| l.to_string() == name)
        };
        let runnable = target_of(cell).is_some_and(|t| have.contains(&t));
        if runnable && !passing.contains(cell) {
            let detail = failures
                .iter()
                .find(|f| f.starts_with(cell))
                .cloned()
                .unwrap_or_default();
            wrong.push(format!(
                "{cell}: pinned as passing and no longer does. {detail}"
            ));
        }
    }
    for cell in &passing {
        if !pinned.contains(cell) {
            wrong.push(format!(
                "{cell}: passes and the ledger omits it; add it to PASSING."
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the conformance ledger and the run disagree:\n  {}",
        wrong.join("\n  ")
    );
}
