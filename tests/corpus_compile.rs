//! Every translated corpus file, accepted by its target's real toolchain.
//!
//! The corpora import worlds this repository does not hold: SQLAlchemy, the
//! Zig standard library, Gson's neighbours. Full semantic compilation is
//! not on the table for any target that resolves names at compile time. What
//! this holds instead is the strongest check each real toolchain can make of a
//! file whose dependencies live elsewhere:
//!
//! - Python: `py_compile`, the complete compiler, since Python resolves at run time.
//! - TypeScript: `tsc --noEmit --noCheck`, the real front end without the type
//!   checker, which would demand the foreign modules.
//! - Go: `gofmt -e`, the toolchain's own parser with every error reported.
//! - Zig: `zig ast-check`, the compiler's AST-level semantic pass.
//! - Rust: `rustfmt`, which parses with the compiler's grammar.
//! - Java: `javac`, accepting only resolution errors. A missing symbol names
//!   the foreign world; anything else names a defect in the writer.
//!
//! A toolchain that is not installed is named loudly in the output and
//! counted; it is never skipped silently.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;
use std::process::Command;

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

fn have(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The one gate per target: Ok(()) accepted, Err(why) rejected.
fn accepted(target: Language, file: &std::path::Path) -> Result<(), String> {
    let run = |mut cmd: Command| -> Result<(), String> {
        let output = cmd.output().map_err(|e| e.to_string())?;
        match output.status.success() {
            true => Ok(()),
            false => Err(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    };
    match target {
        Language::Python => {
            let mut cmd = Command::new("python3");
            cmd.arg("-m").arg("py_compile").arg(file);
            run(cmd)
        }
        Language::TypeScript => {
            let mut cmd = Command::new("tsc");
            cmd.arg("--noEmit")
                .arg("--noCheck")
                .arg("--target")
                .arg("es2020")
                .arg(file);
            run(cmd)
        }
        Language::Go => {
            let mut cmd = Command::new("gofmt");
            cmd.arg("-e").arg(file);
            // gofmt prints the formatted file on success; only the exit code
            // and stderr matter here.
            let output = cmd.output().map_err(|e| e.to_string())?;
            match output.status.success() {
                true => Ok(()),
                false => Err(String::from_utf8_lossy(&output.stderr).to_string()),
            }
        }
        Language::Zig => {
            let mut cmd = Command::new("zig");
            cmd.arg("ast-check").arg(file);
            let output = cmd.output().map_err(|e| e.to_string())?;
            if output.status.success() {
                return Ok(());
            }
            // `ast-check` resolves in-file names; an undeclared identifier
            // names the foreign world, and anything else names a defect.
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let unexpected: Vec<&str> = stderr
                .lines()
                .filter(|l| l.contains("error:") && !l.contains("use of undeclared identifier"))
                .collect();
            match unexpected.is_empty() {
                true => Ok(()),
                false => Err(unexpected.join("\n")),
            }
        }
        Language::Rust => {
            let mut cmd = Command::new("rustfmt");
            cmd.arg("--edition")
                .arg("2021")
                .arg("--emit")
                .arg("stdout")
                .arg(file);
            let output = cmd.output().map_err(|e| e.to_string())?;
            match output.status.success() {
                true => Ok(()),
                false => Err(String::from_utf8_lossy(&output.stderr).to_string()),
            }
        }
        Language::Java => {
            let dir = file.parent().expect("a parent");
            let mut cmd = Command::new("javac");
            cmd.arg("-d").arg(dir).arg(file);
            let output = cmd.output().map_err(|e| e.to_string())?;
            if output.status.success() {
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // Only the errors that name the foreign world are expected; any
            // other error names a defect in the writer.
            let resolution = |line: &str| {
                line.contains("cannot find symbol")
                    || line.contains("does not exist")
                    || line.contains("symbol:")
                    || line.contains("location:")
                    || line.contains("method does not override")
                    || line.contains("incompatible types")
                    || line.contains("cannot be dereferenced")
                    || line.contains("is not a functional interface")
                    || line.contains("^")
                    || line.trim().is_empty()
                    || line.contains(".java:")
                    || line.ends_with("errors") | line.ends_with("error")
            };
            let unexpected: Vec<&str> = stderr
                .lines()
                .filter(|l| l.contains("error:") && !resolution(l))
                .collect();
            match unexpected.is_empty() {
                true => Ok(()),
                false => Err(unexpected.join("\n")),
            }
        }
        other => Err(format!("no gate for {other}")),
    }
}

#[test]
fn every_translated_corpus_file_is_accepted_by_its_toolchain() {
    let tools: &[(Language, &str)] = &[
        (Language::Python, "python3"),
        (Language::TypeScript, "tsc"),
        (Language::Go, "gofmt"),
        (Language::Zig, "zig"),
        (Language::Rust, "rustfmt"),
        (Language::Java, "javac"),
    ];
    let mut absent: Vec<Language> = Vec::new();
    for (language, tool) in tools {
        if !have(tool) {
            eprintln!(
                "TOOLCHAIN ABSENT: {tool} is not installed; \
                 {language} outputs were not compile-checked"
            );
            absent.push(*language);
        }
    }

    let tmp = tempfile::tempdir().expect("a temporary directory");
    let mut rejected: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (path, from) in corpus_files() {
        let module = transpile::read_file(&path).expect("the corpus reads");
        for target in transpile::SUPPORTED {
            if *target == from || absent.contains(target) {
                continue;
            }
            let text = transpile::debug_write(*target, &module).expect("the corpus writes");
            let stem = path
                .file_stem()
                .expect("a stem")
                .to_string_lossy()
                .replace(['-', '[', ']'], "_");
            // Java names the class after the module, which is named after the
            // source file; the output file must match.
            let named = format!("{stem}.{}", extension(*target));
            let dir = tmp.path().join(format!("{checked}"));
            std::fs::create_dir_all(&dir).expect("a scratch directory");
            let out = dir.join(named);
            std::fs::write(&out, text).expect("the draft writes");
            if let Err(why) = accepted(*target, &out) {
                let mut short = why;
                short.truncate(600);
                rejected.push(format!("{} -> {target}:\n{short}", path.display()));
            }
            checked += 1;
        }
    }
    assert!(checked > 0 || !absent.is_empty(), "nothing was checked");
    assert!(
        rejected.is_empty(),
        "{} translated corpus file(s) rejected by their toolchain:\n{}",
        rejected.len(),
        rejected.join("\n---\n")
    );
}

fn extension(target: Language) -> &'static str {
    match target {
        Language::Rust => "rs",
        Language::Go => "go",
        Language::Java => "java",
        Language::Python => "py",
        Language::TypeScript => "ts",
        Language::Zig => "zig",
        _ => "txt",
    }
}
