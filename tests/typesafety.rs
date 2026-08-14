//! The gate for `docs/type-safety.html`.
//!
//! Every example on that page is a file under `tests/typesafety/`, one Python and one
//! TypeScript file per example. Each file's first lines declare what the checker must
//! say about it:
//!
//! - `expect: passes`: `mypy --strict` or `tsc --strict` accepts it.
//! - `expect: fails`: the checker rejects it, and that rejection is the lesson.
//! - `run: yes`: the Python file also executes, so its assertions are executed claims.
//!
//! The page quotes these files through `docs/typesafety-data.js`, which this test
//! regenerates. A claim on the page about what a checker accepts is therefore a claim
//! this test ran. Without that, the page would be sixty statements about `mypy` and
//! `tsc` that nothing ever put to either.
//!
//! The checkers are pinned: mypy 1.18, and the TypeScript compiler
//! from `tests/typesafety/typescript/package.json`. A machine without them skips and
//! says so; CI installs them, and a skip there fails the run.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Passes,
    Fails,
}

#[derive(Debug, Clone)]
struct Example {
    id: String,
    title: String,
    expect_python: Expect,
    expect_typescript: Expect,
    runs: bool,
    /// The id of the example this one is the improved version of, when there is one.
    /// The page shows the two together, with the diff between them.
    improves: Option<String>,
    /// The id of the improved example this one misuses, when there is one. The page
    /// shows it inside that example's block, as the call the checker now rejects.
    misuse_of: Option<String>,
    python: String,
    typescript: String,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn python_dir() -> PathBuf {
    root().join("tests/typesafety/python")
}

fn typescript_dir() -> PathBuf {
    root().join("tests/typesafety/typescript")
}

/// The `key: value` headers in a file's leading comment lines.
fn headers(source: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in source.lines() {
        let Some(comment) = line.strip_prefix("# ").or_else(|| line.strip_prefix("// ")) else {
            break;
        };
        if let Some((key, value)) = comment.split_once(": ") {
            out.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    out
}

fn expectation(headers: &BTreeMap<String, String>, file: &Path) -> Expect {
    match headers.get("expect").map(String::as_str) {
        Some("passes") => Expect::Passes,
        Some("fails") => Expect::Fails,
        other => panic!(
            "{} declares `expect: {other:?}`; it must be `passes` or `fails`",
            file.display()
        ),
    }
}

/// The source as the page shows it: without the harness's own header lines.
fn presentable(source: &str) -> String {
    let mut lines: Vec<&str> = source
        .lines()
        .filter(|line| {
            !(line.starts_with("# expect:")
                || line.starts_with("# run:")
                || line.starts_with("# title:")
                || line.starts_with("# improves:")
                || line.starts_with("# misuse-of:")
                || line.starts_with("// expect:"))
        })
        .collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    lines.join("\n").trim_end().to_string() + "\n"
}

fn examples() -> Vec<Example> {
    let mut python: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in std::fs::read_dir(python_dir()).expect("the python examples directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|e| e == "py") {
            let stem = path
                .file_stem()
                .expect("a stem")
                .to_string_lossy()
                .into_owned();
            python.insert(stem, path);
        }
    }
    let mut typescript: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in std::fs::read_dir(typescript_dir()).expect("the typescript examples directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_some_and(|e| e == "ts") {
            let stem = path
                .file_stem()
                .expect("a stem")
                .to_string_lossy()
                .into_owned();
            typescript.insert(stem, path);
        }
    }

    let py_ids: Vec<&String> = python.keys().collect();
    let ts_ids: Vec<&String> = typescript.keys().collect();
    assert_eq!(
        py_ids, ts_ids,
        "every example needs both languages; the two directories disagree"
    );
    assert!(!python.is_empty(), "no examples were found at all");

    python
        .into_iter()
        .map(|(id, py_path)| {
            let ts_path = &typescript[&id];
            let py_source = std::fs::read_to_string(&py_path).expect("reading the python file");
            let ts_source = std::fs::read_to_string(ts_path).expect("reading the typescript file");
            let py_headers = headers(&py_source);
            let ts_headers = headers(&ts_source);
            let title = py_headers
                .get("title")
                .unwrap_or_else(|| panic!("{} has no `# title:` header", py_path.display()))
                .clone();
            Example {
                expect_python: expectation(&py_headers, &py_path),
                expect_typescript: expectation(&ts_headers, ts_path),
                runs: py_headers.get("run").map(String::as_str) == Some("yes"),
                improves: py_headers.get("improves").cloned(),
                misuse_of: py_headers.get("misuse-of").cloned(),
                python: presentable(&py_source),
                typescript: presentable(&ts_source),
                id,
                title,
            }
        })
        .collect()
}

// ------------------------------------------------------------------ the checkers

fn mypy_available() -> bool {
    Command::new("mypy")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// mypy with the tutorial's configuration, over the given files.
fn mypy(files: &[&str]) -> std::process::Output {
    Command::new("mypy")
        .current_dir(python_dir())
        .arg("--config-file")
        .arg("mypy.ini")
        .args(files)
        .output()
        .expect("running mypy")
}

fn tsc_available() -> bool {
    typescript_dir().join("node_modules/.bin/tsc").exists()
}

/// tsc with the tutorial's flags, over the given files.
fn tsc(files: &[&str]) -> std::process::Output {
    Command::new("npx")
        .current_dir(typescript_dir())
        .args(["--no-install", "tsc", "--noEmit", "--strict", "--noUncheckedIndexedAccess"])
        .args(["--target", "es2022", "--module", "esnext"])
        .args(["--moduleResolution", "bundler", "--skipLibCheck"])
        .args(files)
        .output()
        .expect("running tsc")
}

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn every_python_example_gets_the_verdict_it_declares() {
    if !mypy_available() {
        eprintln!("typesafety: mypy is not installed, so the Python examples went unchecked");
        common::require_on_ci("the type-safety tutorial", &["mypy".to_string()]);
        return;
    }
    let all = examples();

    let passing: Vec<String> = all
        .iter()
        .filter(|e| e.expect_python == Expect::Passes)
        .map(|e| format!("{}.py", e.id))
        .collect();
    let refs: Vec<&str> = passing.iter().map(String::as_str).collect();
    let output = mypy(&refs);
    assert!(
        output.status.success(),
        "an example the page presents as accepted was rejected:\n{}",
        said(&output)
    );

    for example in all.iter().filter(|e| e.expect_python == Expect::Fails) {
        let output = mypy(&[&format!("{}.py", example.id)]);
        assert!(
            !output.status.success(),
            "{}.py is presented as a type error, and mypy accepted it",
            example.id
        );
    }
}

#[test]
fn every_typescript_example_gets_the_verdict_it_declares() {
    if !tsc_available() {
        eprintln!(
            "typesafety: tests/typesafety/typescript/node_modules is missing, so the \
             TypeScript examples went unchecked. Run `npm ci` there."
        );
        common::require_on_ci("the type-safety tutorial", &["tsc".to_string()]);
        return;
    }
    let all = examples();

    let passing: Vec<String> = all
        .iter()
        .filter(|e| e.expect_typescript == Expect::Passes)
        .map(|e| format!("{}.ts", e.id))
        .collect();
    let refs: Vec<&str> = passing.iter().map(String::as_str).collect();
    let output = tsc(&refs);
    assert!(
        output.status.success(),
        "an example the page presents as accepted was rejected:\n{}",
        said(&output)
    );

    for example in all.iter().filter(|e| e.expect_typescript == Expect::Fails) {
        let output = tsc(&[&format!("{}.ts", example.id)]);
        assert!(
            !output.status.success(),
            "{}.ts is presented as a type error, and tsc accepted it",
            example.id
        );
    }
}

#[test]
fn every_run_tagged_example_executes() {
    // `python3` here must be 3.12 or newer: the examples use `type` aliases and
    // PEP 695 generics at run time, not only under the checker.
    let version = Command::new("python3")
        .args(["-c", "import sys; print(sys.version_info >= (3, 12))"])
        .output();
    let modern = version
        .as_ref()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "True");
    if !modern {
        eprintln!("typesafety: python3 is missing or older than 3.12, so nothing ran");
        common::require_on_ci("the type-safety tutorial", &["python3 >= 3.12".to_string()]);
        return;
    }

    let mut ran = 0;
    for example in examples().iter().filter(|e| e.runs) {
        let output = Command::new("python3")
            .current_dir(python_dir())
            .arg(format!("{}.py", example.id))
            .output()
            .expect("running python3");
        assert!(
            output.status.success(),
            "{}.py is presented as running, and it failed:\n{}",
            example.id,
            said(&output)
        );
        ran += 1;
    }
    assert!(
        ran > 0,
        "no example carries `run: yes`, so nothing was executed"
    );
}

// ------------------------------------------------------------------ the page

/// Every before-and-after pair, as (after id, python diff, typescript diff).
///
/// An example that improves another says so in its header, `improves: <id>`, and the
/// page shows the diff between the two beside the improved version. The diff comes
/// from this tool's own engine, `edit::unified_diff`, so the rendering on the
/// tutorial matches the rendering every command prints.
fn improvement_diffs(all: &[Example]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for example in all {
        let Some(before_id) = &example.improves else {
            continue;
        };
        let before = all
            .iter()
            .find(|e| &e.id == before_id)
            .unwrap_or_else(|| panic!("{} improves {before_id}, which does not exist", example.id));
        out.push((
            example.id.clone(),
            fun_refactor::edit::unified_diff(&before.python, &example.python, "example.py"),
            fun_refactor::edit::unified_diff(&before.typescript, &example.typescript, "example.ts"),
        ));
    }
    assert!(
        !out.is_empty(),
        "no example declares `improves:`, so no diff exists"
    );
    out
}

fn data_js() -> String {
    let mut out = String::from(
        "// Generated by `cargo test --test typesafety`. Do not edit.\n\
         //\n\
         // Every example below is a file under tests/typesafety/. The test that\n\
         // writes this file holds each one to its declared verdict. Regenerate with:\n\
         //   UPDATE_SITE_DATA=1 cargo test --test typesafety\n\
         export const EXAMPLES = {\n",
    );
    for example in examples() {
        out.push_str(&format!(
            "  {}: {{\n    title: {},\n    expectPython: {:?},\n    expectTypescript: {:?},\n    runs: {},\n    improves: {},\n    misuseOf: {},\n    python: {},\n    typescript: {},\n  }},\n",
            serde_json::to_string(&example.id).expect("a JSON string"),
            serde_json::to_string(&example.title).expect("a JSON string"),
            match example.expect_python {
                Expect::Passes => "passes",
                Expect::Fails => "fails",
            },
            match example.expect_typescript {
                Expect::Passes => "passes",
                Expect::Fails => "fails",
            },
            example.runs,
            serde_json::to_string(&example.improves).expect("a JSON string"),
            serde_json::to_string(&example.misuse_of).expect("a JSON string"),
            serde_json::to_string(&example.python).expect("a JSON string"),
            serde_json::to_string(&example.typescript).expect("a JSON string"),
        ));
    }
    out.push_str("};\n");
    out.push_str("export const DIFFS = {\n");
    for (after, python, typescript) in improvement_diffs(&examples()) {
        out.push_str(&format!(
            "  {}: {{\n    python: {},\n    typescript: {},\n  }},\n",
            serde_json::to_string(&after).expect("a JSON string"),
            serde_json::to_string(&python).expect("a JSON string"),
            serde_json::to_string(&typescript).expect("a JSON string"),
        ));
    }
    out.push_str("};\n");
    out
}

#[test]
fn the_tutorial_page_shows_the_checked_examples() {
    let path = root().join("docs/typesafety-data.js");
    let generated = data_js();
    if std::env::var("UPDATE_SITE_DATA").is_ok() {
        std::fs::write(&path, &generated).expect("writing the generated data");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/typesafety-data.js is not what the example files say any more. \
         Regenerate it:\n\n    UPDATE_SITE_DATA=1 cargo test --test typesafety\n"
    );
}

/// The checkers' verbatim output for every example the page presents as a type error.
fn errors_js() -> String {
    let mut out = String::from(
        "// Generated by `cargo test --test typesafety`. Do not edit.\n\
         //\n\
         // The checkers' verbatim words for each type-error example. Regenerated \
         together with typesafety-data.js.\n\
         export const ERRORS = {\n",
    );
    for example in examples() {
        if example.expect_python != Expect::Fails && example.expect_typescript != Expect::Fails {
            continue;
        }
        let python = if example.expect_python == Expect::Fails {
            said(&mypy(&[&format!("{}.py", example.id)]))
        } else {
            String::new()
        };
        let typescript = if example.expect_typescript == Expect::Fails {
            said(&tsc(&[&format!("{}.ts", example.id)]))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  {}: {{\n    python: {},\n    typescript: {},\n  }},\n",
            serde_json::to_string(&example.id).expect("a JSON string"),
            serde_json::to_string(python.trim_end()).expect("a JSON string"),
            serde_json::to_string(typescript.trim_end()).expect("a JSON string"),
        ));
    }
    out.push_str("};\n");
    out
}

#[test]
fn the_page_shows_the_checkers_words() {
    if !mypy_available() || !tsc_available() {
        eprintln!("typesafety: a checker is missing; messages unverified.");
        common::require_on_ci(
            "the type-safety tutorial",
            &["mypy".to_string(), "tsc".to_string()],
        );
        return;
    }
    let path = root().join("docs/typesafety-errors.js");
    let generated = errors_js();
    if std::env::var("UPDATE_SITE_DATA").is_ok() {
        std::fs::write(&path, &generated).expect("writing the captured messages");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/typesafety-errors.js is not what the checkers say any more. \
         Regenerate it with the same command as typesafety-data.js.\n"
    );
}

#[test]
fn the_examples_carry_no_comments() {
    for example in examples() {
        for (code, marker) in [(&example.python, "#"), (&example.typescript, "//")] {
            for (at, line) in code.lines().enumerate() {
                let trimmed = line.trim_start();
                let commented = trimmed.starts_with(marker)
                    || line.contains(&format!("  {marker} "))
                    || line.ends_with(&format!(" {marker}"));
                assert!(
                    !commented || trimmed.starts_with("#!"),
                    "{}.{} line {}: the examples explain themselves in the prose, not in comments: {line}",
                    example.id,
                    if *marker == *"#" { "py" } else { "ts" },
                    at + 1,
                );
            }
        }
    }
}

#[test]
fn every_improvement_shows_a_rejection() {
    // alias_compound teaches that an alias enforces nothing, so it has no catch.
    let allowed_without = ["alias_compound"];
    let all = examples();
    for example in all.iter().filter(|e| e.improves.is_some()) {
        if allowed_without.contains(&example.id.as_str()) {
            continue;
        }
        let shown = all
            .iter()
            .any(|e| e.misuse_of.as_deref() == Some(&example.id));
        assert!(
            shown,
            "{} improves an example, and no misuse-of example shows what the checker now rejects",
            example.id
        );
    }
}

#[test]
fn every_example_plays_exactly_one_part() {
    // The page shows an example as a block (an improved version, with its
    // predecessor and the diff), as a row inside another example's block (the
    // predecessor itself, or a misuse), or as a standalone example. Every file
    // must land in one of those parts, or it is content nothing shows.
    let all = examples();
    for example in &all {
        let improved_by = all
            .iter()
            .any(|e| e.improves.as_deref() == Some(&example.id));
        let parts = [
            example.improves.is_some(),
            example.misuse_of.is_some(),
            improved_by,
        ];
        let count = parts.iter().filter(|p| **p).count();
        assert!(
            count <= 1,
            "{} plays {count} parts at once; it can be an improvement, a predecessor \
             or a misuse, and only one of them",
            example.id
        );
        if let Some(target) = &example.misuse_of {
            let owner = all.iter().find(|e| &e.id == target);
            assert!(
                owner.is_some_and(|o| o.improves.is_some()),
                "{} misuses {target}, which is not an improved example",
                example.id
            );
        }
    }
}

#[test]
fn the_page_and_the_examples_agree() {
    // Every example placeholder on the page names a checked example, and every
    // checked example appears on the page exactly once. A file added without a
    // placeholder, or a placeholder pointing at nothing, fails here.
    let html = std::fs::read_to_string(root().join("docs/type-safety.html"))
        .expect("docs/type-safety.html");
    let all = examples();

    for example in &all {
        let improved_by = all
            .iter()
            .any(|e| e.improves.as_deref() == Some(&example.id));
        let expected = if example.improves.is_some() {
            // An improved example is a block: predecessor, diff and improvement
            // together, plus any misuse row.
            Some(format!("data-block=\"{}\"", example.id))
        } else if improved_by || example.misuse_of.is_some() {
            // Shown inside the block of the example that improves on it, or that
            // it misuses. No slot of its own.
            None
        } else {
            Some(format!("data-example=\"{}\"", example.id))
        };
        if let Some(marker) = expected {
            let count = html.matches(&marker).count();
            assert_eq!(
                count, 1,
                "{} needs one {marker} slot, found {count}",
                example.id
            );
        }
    }
    let blocks = html.matches("data-block=\"").count();
    let afters = all.iter().filter(|e| e.improves.is_some()).count();
    assert_eq!(
        blocks, afters,
        "{blocks} block slots for {afters} improved examples"
    );
}
