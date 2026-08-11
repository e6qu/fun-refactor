//! Every command, run for real, and a check that this file keeps up.
//!
//! ## The layers
//!
//! 1. **Unit**, `#[cfg(test)] mod tests` beside the code, for the pieces whose
//!    correctness is local: span arithmetic, negation, import liveness, the hash of a
//!    subtree.
//! 2. **Integration**, `tests/*.rs` against the library. Most of the suite. A
//!    workspace is written to a temp directory, indexed, and a refactoring is planned
//!    and applied, so the assertion is about the resulting bytes instead of an
//!    intermediate.
//! 3. **End-to-end**, `tests/cli.rs` and this file, running the binary. Argument
//!    parsing, path resolution, exit codes and the text a person reads. This layer
//!    did not exist until two bugs were found living in it: `--path` filters built by
//!    joining the default root `.`, which matched nothing and reported that as
//!    nothing found, and target paths read from the shell's directory instead of the
//!    workspace `-C` names.
//! 4. **Real repositories**, helm/helm and grafana/grafana, run by hand and recorded
//!    in TUTORIAL.md and BUGS.md with the measurements. Not automated here: pinning a
//!    500 MB clone into CI buys less than the numbers in BUGS.md already do, and the
//!    bugs it found were found by *reading* the output, which a test cannot do.
//!
//! ## What this file asserts
//!
//! Every subcommand runs against a small polyglot workspace without panicking, and
//! every one is named by at least one end-to-end test. The second half is the part
//! that keeps the layer honest: a new command with no coverage fails the build here
//! instead of shipping untested, which is exactly how the CLI layer came to have no
//! tests at all.

use std::path::Path;
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

/// A workspace with enough of each language for every command to have something to do.
fn workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let files: &[(&str, &str)] = &[
        (
            "svc/a.go",
            "package svc\n\nimport \"fmt\"\n\n\
             // Helper adds one.\n\
             func Helper(x int) int {\n\treturn x + 1\n}\n\n\
             func caller() int {\n\tv := Helper(1)\n\tif v > 0 {\n\t\tfmt.Println(v)\n\t}\n\treturn v\n}\n",
        ),
        (
            "svc/b.go",
            "package svc\n\nfunc main() {\n\t_ = caller()\n}\n",
        ),
        (
            "web/app.ts",
            "export function render(name: string): string {\n  const greeting = \"hi \" + name;\n  return greeting;\n}\n",
        ),
        ("web/app.css", ".panel {\n  color: red;\n}\n"),
        ("chart/Chart.yaml", "apiVersion: v2\nname: demo\nversion: 0.1.0\n"),
        ("chart/values.yaml", "appName: demo\nimage:\n  tag: v1\n"),
        (
            "chart/templates/pod.yaml",
            "metadata:\n  name: \"{{ .Values.appName }}\"\n  tag: \"{{ .Values.image.tag }}\"\n",
        ),
        ("README.md", "# Demo\n\nSee [the service](svc/a.go).\n"),
        // A recipe, so `fr recipe` has something to run in the shared fixture.
        (
            "tidy.recipe",
            "schema 1\n\nrecipe tidy {\n  rename to \"Increment\" where name=\"Helper\" kind=function\n}\n",
        ),
        ("run.sh", "#!/bin/bash\nmain() {\n  echo hello\n}\nmain\n"),
    ];
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    tmp
}

struct Outcome {
    code: Option<i32>,
    text: String,
}

fn run(root: &Path, cache: &Path, args: &[&str]) -> Outcome {
    let output = Command::new(FR)
        .arg("-C")
        .arg(root)
        .args(args)
        .env("FUN_REFACTOR_CACHE", cache)
        .output()
        .expect("fr should be runnable");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Outcome {
        code: output.status.code(),
        text,
    }
}

/// One representative invocation per subcommand.
///
/// Some of these are expected to refuse, deleting a symbol four things use, inlining
/// a function. A refusal is a fine outcome; a panic is not, and neither is a message
/// that does not say what went wrong.
fn invocations() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("capabilities", vec!["capabilities"]),
        ("cache", vec!["cache"]),
        ("scan", vec!["scan"]),
        ("parse", vec!["parse", "--stats"]),
        ("symbols", vec!["symbols"]),
        ("def", vec!["def", "svc/a.go:10:7"]),
        ("type", vec!["type", "svc/a.go:6:6"]),
        ("refs", vec!["refs", "svc/a.go:6:6"]),
        ("implementations", vec!["implementations", "svc/a.go:6:6"]),
        ("usages", vec!["usages", "svc/a.go:6:6"]),
        ("callers", vec!["callers", "svc/a.go:6:6", "--depth", "2"]),
        ("callees", vec!["callees", "svc/a.go:9:6", "--depth", "2"]),
        ("rename", vec!["rename", "svc/a.go:6:6", "Increment"]),
        ("extract", vec!["extract", "web/app.ts:2:20-2:33", "prefix"]),
        ("inline", vec!["inline", "web/app.ts:2:9"]),
        ("signature", vec!["signature", "svc/a.go:6:6", "remove:0"]),
        ("move", vec!["move", "svc/a.go:6:6", "svc/c.go"]),
        ("delete", vec!["delete", "svc/a.go:6:6"]),
        ("duplicates", vec!["duplicates", "--min-tokens", "40"]),
        ("unused", vec!["unused", "--language", "go"]),
        ("imports", vec!["imports", "svc/a.go"]),
        // A stylesheet is the one thing in this fixture that can be rewritten as
        // another language; every programming language in it refuses, by design.
        ("translate", vec!["translate", "web/app.css", "scss"]),
        (
            "remove-flag",
            vec!["remove-flag", "USE_NEW", "--value", "true"],
        ),
        ("rewrite", vec!["rewrite", "svc/a.go:12:2"]),
        (
            "restructure",
            vec!["restructure", "Helper($X)", "Increment($X)", "--lang", "go"],
        ),
        ("flow", vec!["flow", "fwd", "chart/values.yaml:1:1"]),
        ("stitch", vec!["stitch"]),
        ("impact", vec!["impact", "svc/a.go:6:6"]),
        ("graph", vec!["graph"]),
        ("entrypoints", vec!["entrypoints"]),
        ("recipe", vec!["recipe", "tidy.recipe"]),
        // The fixture has no Next.js route, so this exercises the refusal.
        ("openapi", vec!["openapi"]),
    ]
}

#[test]
fn every_command_runs_without_panicking() {
    let tmp = workspace();
    let cache = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();

    for (name, args) in invocations() {
        let outcome = run(tmp.path(), cache.path(), &args);
        // 101 is a Rust panic; anything else is the program deciding something.
        if outcome.code == Some(101) || outcome.text.contains("panicked at") {
            failures.push(format!(
                "`fr {}` panicked:\n{}",
                args.join(" "),
                outcome.text
            ));
            continue;
        }
        if outcome.code.is_none() {
            failures.push(format!("`fr {}` was killed by a signal", args.join(" ")));
            continue;
        }
        // A refusal has to say something. An empty failure is indistinguishable from
        // a crash to whoever is reading the terminal.
        if outcome.code != Some(0) && outcome.text.trim().is_empty() {
            failures.push(format!("`fr {name}` failed silently"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_command_writes_nothing_unless_asked() {
    // The promise the whole CLI rests on: a command without --write is a question.
    let tmp = workspace();
    let cache = tempfile::tempdir().unwrap();

    let before = snapshot(tmp.path());
    for (_, args) in invocations() {
        run(tmp.path(), cache.path(), &args);
    }
    let after = snapshot(tmp.path());

    assert_eq!(
        before, after,
        "a command changed the workspace without --write"
    );
}

/// Every file and its bytes, so a stray write of any kind shows up.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let name = path.strip_prefix(root).unwrap().display().to_string();
                out.push((name, bytes));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_subcommand_is_left_without_an_end_to_end_test() {
    // The check that keeps this layer from decaying the way it started. `fr --help`
    // is the real surface; anything it lists has to be exercised somewhere here.
    let output = Command::new(FR)
        .arg("--help")
        .output()
        .expect("fr --help should run");
    let help = String::from_utf8_lossy(&output.stdout).to_string();

    let listed: Vec<String> = help
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("Options:") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .map(|w| w.to_string())
        .filter(|w| w != "help")
        .collect();

    assert!(
        listed.len() > 20,
        "expected to parse the command list, got {listed:?}"
    );

    let smoke: Vec<&str> = invocations().into_iter().map(|(name, _)| name).collect();
    let deep = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli.rs"))
        .expect("tests/cli.rs is the other end-to-end file");

    let missing: Vec<&String> = listed
        .iter()
        .filter(|name| !smoke.contains(&name.as_str()) && !deep.contains(&format!("\"{name}\"")))
        .collect();

    assert!(
        missing.is_empty(),
        "these subcommands have no end-to-end test: {missing:?}\n\
         Add one to `invocations()` here, or a specific case to tests/cli.rs."
    );
}
