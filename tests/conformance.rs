//! Differential execution: a program translated is a program that still runs.
//!
//! Every directory under `tests/conformance/` holds one program written six times, once
//! per language, each printing the same transcript to stdout. The natives are checked
//! against each other first, so the suite cannot drift while it grows. Then every
//! source is translated to every other language, compiled with that language's real
//! compiler, run, and its stdout compared to the transcript.
//!
//! The `PASSING` ledger pins which cells hold, and it is a ratchet in both directions.
//! A pinned cell that stops passing fails the build. A cell that starts passing fails
//! too, and asks to be pinned, so the ledger keeps meaning something.
//!
//! A compiler this machine lacks is named in the output and its cells are skipped by
//! name; green never means unchecked. Nothing here talks to a network.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Every cell that holds: `group source->target`, sorted.
const PASSING: &[&str] = &[
    "asynchrony go->java",
    "asynchrony go->python",
    "asynchrony go->rust",
    "asynchrony go->typescript",
    "asynchrony go->zig",
    "asynchrony java->go",
    "asynchrony java->python",
    "asynchrony java->rust",
    "asynchrony java->typescript",
    "asynchrony java->zig",
    "asynchrony python->go",
    "asynchrony python->java",
    "asynchrony python->rust",
    "asynchrony python->typescript",
    "asynchrony python->zig",
    "asynchrony rust->go",
    "asynchrony rust->java",
    "asynchrony rust->python",
    "asynchrony rust->typescript",
    "asynchrony rust->zig",
    "asynchrony typescript->go",
    "asynchrony typescript->java",
    "asynchrony typescript->python",
    "asynchrony typescript->rust",
    "asynchrony typescript->zig",
    "asynchrony zig->go",
    "asynchrony zig->java",
    "asynchrony zig->python",
    "asynchrony zig->rust",
    "asynchrony zig->typescript",
    "bindings bash->go",
    "bindings bash->java",
    "bindings bash->python",
    "bindings bash->rust",
    "bindings bash->typescript",
    "bindings bash->zig",
    "bindings go->bash",
    "bindings go->java",
    "bindings go->python",
    "bindings go->rust",
    "bindings go->typescript",
    "bindings go->zig",
    "bindings java->bash",
    "bindings java->go",
    "bindings java->python",
    "bindings java->rust",
    "bindings java->typescript",
    "bindings java->zig",
    "bindings python->bash",
    "bindings python->go",
    "bindings python->java",
    "bindings python->rust",
    "bindings python->typescript",
    "bindings python->zig",
    "bindings rust->bash",
    "bindings rust->go",
    "bindings rust->java",
    "bindings rust->python",
    "bindings rust->typescript",
    "bindings rust->zig",
    "bindings typescript->bash",
    "bindings typescript->go",
    "bindings typescript->java",
    "bindings typescript->python",
    "bindings typescript->rust",
    "bindings typescript->zig",
    "bindings zig->bash",
    "bindings zig->go",
    "bindings zig->java",
    "bindings zig->python",
    "bindings zig->rust",
    "bindings zig->typescript",
    "cleanup go->java",
    "cleanup go->python",
    "cleanup go->rust",
    "cleanup go->typescript",
    "cleanup go->zig",
    "cleanup java->go",
    "cleanup java->python",
    "cleanup java->rust",
    "cleanup java->typescript",
    "cleanup java->zig",
    "cleanup python->go",
    "cleanup python->java",
    "cleanup python->rust",
    "cleanup python->typescript",
    "cleanup python->zig",
    "cleanup rust->go",
    "cleanup rust->java",
    "cleanup rust->python",
    "cleanup rust->typescript",
    "cleanup rust->zig",
    "cleanup typescript->go",
    "cleanup typescript->java",
    "cleanup typescript->python",
    "cleanup typescript->rust",
    "cleanup typescript->zig",
    "cleanup zig->go",
    "cleanup zig->java",
    "cleanup zig->python",
    "cleanup zig->rust",
    "cleanup zig->typescript",
    "closures go->java",
    "closures go->python",
    "closures go->rust",
    "closures go->typescript",
    "closures go->zig",
    "closures java->go",
    "closures java->python",
    "closures java->rust",
    "closures java->typescript",
    "closures java->zig",
    "closures python->go",
    "closures python->java",
    "closures python->rust",
    "closures python->typescript",
    "closures python->zig",
    "closures rust->go",
    "closures rust->java",
    "closures rust->python",
    "closures rust->typescript",
    "closures rust->zig",
    "closures typescript->go",
    "closures typescript->java",
    "closures typescript->python",
    "closures typescript->rust",
    "closures typescript->zig",
    "closures zig->go",
    "closures zig->java",
    "closures zig->python",
    "closures zig->rust",
    "closures zig->typescript",
    "collections bash->go",
    "collections bash->java",
    "collections bash->python",
    "collections bash->rust",
    "collections bash->typescript",
    "collections bash->zig",
    "collections go->bash",
    "collections go->java",
    "collections go->python",
    "collections go->rust",
    "collections go->typescript",
    "collections go->zig",
    "collections java->bash",
    "collections java->go",
    "collections java->python",
    "collections java->rust",
    "collections java->typescript",
    "collections java->zig",
    "collections python->bash",
    "collections python->go",
    "collections python->java",
    "collections python->rust",
    "collections python->typescript",
    "collections python->zig",
    "collections rust->bash",
    "collections rust->go",
    "collections rust->java",
    "collections rust->python",
    "collections rust->typescript",
    "collections rust->zig",
    "collections typescript->bash",
    "collections typescript->go",
    "collections typescript->java",
    "collections typescript->python",
    "collections typescript->rust",
    "collections typescript->zig",
    "collections zig->bash",
    "collections zig->go",
    "collections zig->java",
    "collections zig->python",
    "collections zig->rust",
    "collections zig->typescript",
    "comprehensions go->java",
    "comprehensions go->python",
    "comprehensions go->rust",
    "comprehensions go->typescript",
    "comprehensions go->zig",
    "comprehensions java->go",
    "comprehensions java->python",
    "comprehensions java->rust",
    "comprehensions java->typescript",
    "comprehensions java->zig",
    "comprehensions python->go",
    "comprehensions python->java",
    "comprehensions python->rust",
    "comprehensions python->typescript",
    "comprehensions python->zig",
    "comprehensions rust->go",
    "comprehensions rust->java",
    "comprehensions rust->python",
    "comprehensions rust->typescript",
    "comprehensions rust->zig",
    "comprehensions typescript->go",
    "comprehensions typescript->java",
    "comprehensions typescript->python",
    "comprehensions typescript->rust",
    "comprehensions typescript->zig",
    "comprehensions zig->go",
    "comprehensions zig->java",
    "comprehensions zig->python",
    "comprehensions zig->rust",
    "comprehensions zig->typescript",
    "control bash->go",
    "control bash->java",
    "control bash->python",
    "control bash->rust",
    "control bash->typescript",
    "control bash->zig",
    "control go->bash",
    "control go->java",
    "control go->python",
    "control go->rust",
    "control go->typescript",
    "control go->zig",
    "control java->bash",
    "control java->go",
    "control java->python",
    "control java->rust",
    "control java->typescript",
    "control java->zig",
    "control python->bash",
    "control python->go",
    "control python->java",
    "control python->rust",
    "control python->typescript",
    "control python->zig",
    "control rust->bash",
    "control rust->go",
    "control rust->java",
    "control rust->python",
    "control rust->typescript",
    "control rust->zig",
    "control typescript->bash",
    "control typescript->go",
    "control typescript->java",
    "control typescript->python",
    "control typescript->rust",
    "control typescript->zig",
    "control zig->bash",
    "control zig->go",
    "control zig->java",
    "control zig->python",
    "control zig->rust",
    "control zig->typescript",
    "dispatch go->java",
    "dispatch go->python",
    "dispatch go->rust",
    "dispatch go->typescript",
    "dispatch go->zig",
    "dispatch java->go",
    "dispatch java->python",
    "dispatch java->rust",
    "dispatch java->typescript",
    "dispatch java->zig",
    "dispatch python->go",
    "dispatch python->java",
    "dispatch python->rust",
    "dispatch python->typescript",
    "dispatch python->zig",
    "dispatch rust->go",
    "dispatch rust->java",
    "dispatch rust->python",
    "dispatch rust->typescript",
    "dispatch rust->zig",
    "dispatch typescript->go",
    "dispatch typescript->java",
    "dispatch typescript->python",
    "dispatch typescript->rust",
    "dispatch typescript->zig",
    "dispatch zig->go",
    "dispatch zig->java",
    "dispatch zig->python",
    "dispatch zig->rust",
    "dispatch zig->typescript",
    "errors go->java",
    "errors go->python",
    "errors go->rust",
    "errors go->typescript",
    "errors go->zig",
    "errors java->go",
    "errors java->python",
    "errors java->rust",
    "errors java->typescript",
    "errors java->zig",
    "errors python->go",
    "errors python->java",
    "errors python->rust",
    "errors python->typescript",
    "errors python->zig",
    "errors rust->go",
    "errors rust->java",
    "errors rust->python",
    "errors rust->typescript",
    "errors rust->zig",
    "errors typescript->go",
    "errors typescript->java",
    "errors typescript->python",
    "errors typescript->rust",
    "errors typescript->zig",
    "errors zig->go",
    "errors zig->java",
    "errors zig->python",
    "errors zig->rust",
    "errors zig->typescript",
    "generics go->java",
    "generics go->python",
    "generics go->rust",
    "generics go->typescript",
    "generics go->zig",
    "generics java->go",
    "generics java->python",
    "generics java->rust",
    "generics java->typescript",
    "generics java->zig",
    "generics python->go",
    "generics python->java",
    "generics python->rust",
    "generics python->typescript",
    "generics python->zig",
    "generics rust->go",
    "generics rust->java",
    "generics rust->python",
    "generics rust->typescript",
    "generics rust->zig",
    "generics typescript->go",
    "generics typescript->java",
    "generics typescript->python",
    "generics typescript->rust",
    "generics typescript->zig",
    "generics zig->go",
    "generics zig->java",
    "generics zig->python",
    "generics zig->rust",
    "generics zig->typescript",
    "maps go->java",
    "maps go->python",
    "maps go->rust",
    "maps go->typescript",
    "maps go->zig",
    "maps java->go",
    "maps java->python",
    "maps java->rust",
    "maps java->typescript",
    "maps java->zig",
    "maps python->go",
    "maps python->java",
    "maps python->rust",
    "maps python->typescript",
    "maps python->zig",
    "maps rust->go",
    "maps rust->java",
    "maps rust->python",
    "maps rust->typescript",
    "maps rust->zig",
    "maps typescript->go",
    "maps typescript->java",
    "maps typescript->python",
    "maps typescript->rust",
    "maps typescript->zig",
    "maps zig->go",
    "maps zig->java",
    "maps zig->python",
    "maps zig->rust",
    "maps zig->typescript",
    "numbers go->java",
    "numbers go->python",
    "numbers go->rust",
    "numbers go->typescript",
    "numbers go->zig",
    "numbers java->go",
    "numbers java->python",
    "numbers java->rust",
    "numbers java->typescript",
    "numbers java->zig",
    "numbers python->go",
    "numbers python->java",
    "numbers python->rust",
    "numbers python->typescript",
    "numbers python->zig",
    "numbers rust->go",
    "numbers rust->java",
    "numbers rust->python",
    "numbers rust->typescript",
    "numbers rust->zig",
    "numbers typescript->go",
    "numbers typescript->java",
    "numbers typescript->python",
    "numbers typescript->rust",
    "numbers typescript->zig",
    "numbers zig->go",
    "numbers zig->java",
    "numbers zig->python",
    "numbers zig->rust",
    "numbers zig->typescript",
    "strings go->java",
    "strings go->python",
    "strings go->rust",
    "strings go->typescript",
    "strings go->zig",
    "strings java->go",
    "strings java->python",
    "strings java->rust",
    "strings java->typescript",
    "strings java->zig",
    "strings python->go",
    "strings python->java",
    "strings python->rust",
    "strings python->typescript",
    "strings python->zig",
    "strings rust->go",
    "strings rust->java",
    "strings rust->python",
    "strings rust->typescript",
    "strings rust->zig",
    "strings typescript->go",
    "strings typescript->java",
    "strings typescript->python",
    "strings typescript->rust",
    "strings typescript->zig",
    "strings zig->go",
    "strings zig->java",
    "strings zig->python",
    "strings zig->rust",
    "strings zig->typescript",
];

/// The languages of the suite, with how each one's programs are named and run.
const LANGUAGES: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
    Language::Bash,
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
        other => panic!("{other} is not in the suite"),
    }
}

/// The file stem a program takes.
///
/// Java's single-file launcher requires the public class to match the file, and the
/// translator names the class after the destination's stem.
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
///
/// `Err` carries what went wrong, compile or run, with the tool's own words.
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
            // tsc writes the JavaScript beside its input, so the input is copied into
            // the scratch directory first. Only when it is not already there: copying
            // a file onto itself truncates it to nothing.
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
    for language in LANGUAGES {
        if !have.contains(language) {
            let (binary, _) = toolchain(*language);
            println!("conformance: {language} skipped, {binary} is not on PATH");
        }
    }
    assert!(
        !have.is_empty(),
        "no toolchain at all; this run would check nothing"
    );

    let mut passing = BTreeSet::new();
    let mut failures = Vec::new();

    for group in groups() {
        let name = group.file_name().unwrap().to_string_lossy().to_string();

        // The natives first: every one that can run must print the same transcript,
        // or the suite itself has drifted and every comparison below is noise.
        let mut transcript: Option<(Language, String)> = None;
        for language in LANGUAGES {
            let file = group
                .join(stem(*language))
                .with_extension(extension(*language));
            // Bash writes its computational subset and sits the other groups out,
            // out loud. The complete six must all be present in every group.
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
                "{cell}: passes and is not pinned; add it to PASSING."
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the conformance ledger and the run disagree:\n  {}",
        wrong.join("\n  ")
    );
}
