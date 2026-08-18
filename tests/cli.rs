//! End-to-end tests: the binary, invoked the way a person invokes it.
//!
//! Every other test in this suite calls the library directly, which leaves the layer between a
//! command line and that library untested. Two real bugs lived there and neither was visible
//! from below. `--path` filters were built by joining the default root `.`, so they never
//! matched the absolute paths in the index and every filtered report came back empty, a clean
//! bill of health that meant "the filter matched nothing". A refactoring tool that reports "no
//! findings" when it means "I looked in the wrong place" is worse than one that crashes.
//!
//! So these run `fr` itself: argument parsing, path resolution, exit codes and the text a
//! person reads.

use std::path::Path;
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

struct Workspace {
    tmp: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Workspace {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        Workspace {
            tmp,
            cache: tempfile::tempdir().unwrap(),
        }
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    /// Run `fr` in the workspace and return (stdout+stderr, success).
    fn run(&self, args: &[&str]) -> (String, bool) {
        let output = Command::new(FR)
            .arg("-C")
            .arg(self.root())
            .args(args)
            .env("FUN_REFACTOR_CACHE", self.cache.path())
            .output()
            .expect("fr should run");
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (text, output.status.success())
    }
}

/// A function long enough to be found as a duplicate, and dead enough to be listed.
fn go_helper(name: &str) -> String {
    format!(
        "package p\n\nfunc {name}(items []int, factor int) int {{\n\
         \ttotal := 0\n\
         \tfor _, item := range items {{\n\
         \t\tif item > 0 {{\n\
         \t\t\ttotal += item * factor\n\
         \t\t}} else {{\n\
         \t\t\ttotal -= item / factor\n\
         \t\t}}\n\
         \t}}\n\
         \tif total > 100 {{\n\
         \t\ttotal = 100\n\
         \t}}\n\
         \treturn total\n\
         }}\n"
    )
}

fn workspace() -> Workspace {
    Workspace::new(&[
        ("keep/a.go", &go_helper("alpha")),
        ("drop/b.go", &go_helper("beta")),
        ("notes.md", "# A heading nothing links to\n\nSome prose.\n"),
    ])
}

#[test]
fn a_path_filter_actually_narrows_the_report() {
    // The regression this file exists for. `--path` was joined onto the default root
    // `.`, producing `./keep`, which starts_with-matches no absolute path, so the
    // report was empty and read as "no duplication".
    let ws = workspace();

    let (all, ok) = ws.run(&["duplicates", "--language", "go", "--min-tokens", "40"]);
    assert!(ok, "{all}");
    assert!(
        all.contains("keep/a.go"),
        "unfiltered run finds both:\n{all}"
    );
    assert!(
        all.contains("drop/b.go"),
        "unfiltered run finds both:\n{all}"
    );

    let (filtered, ok) = ws.run(&[
        "duplicates",
        "--language",
        "go",
        "--min-tokens",
        "40",
        "--path",
        "keep",
    ]);
    assert!(ok, "{filtered}");
    assert!(
        filtered.contains("No duplication"),
        "one copy is not a duplication:\n{filtered}"
    );
}

#[test]
fn a_path_filter_that_matches_nothing_says_so_instead_of_reporting_nothing() {
    let ws = workspace();
    let (out, ok) = ws.run(&["duplicates", "--path", "does-not-exist"]);
    assert!(!ok, "a path that cannot be resolved is an error:\n{out}");
    assert!(out.contains("cannot resolve --path"), "got:\n{out}");
}

#[test]
fn a_language_filter_narrows_the_unused_report() {
    let ws = workspace();

    let (everything, ok) = ws.run(&["unused"]);
    assert!(ok, "{everything}");
    assert!(
        everything.contains("heading"),
        "a Markdown heading nothing links to is a finding:\n{everything}"
    );

    let (go_only, ok) = ws.run(&["unused", "--language", "go"]);
    assert!(ok, "{go_only}");
    assert!(
        !go_only.contains("heading"),
        "--language go excludes it:\n{go_only}"
    );
}

#[test]
fn an_unknown_language_is_refused_against_the_known_list() {
    let ws = workspace();
    let (out, ok) = ws.run(&["unused", "--language", "golang"]);
    assert!(!ok, "a typo must not silently narrow the report:\n{out}");
    assert!(out.contains("unknown language 'golang'"), "got:\n{out}");
    assert!(
        out.contains("go"),
        "the message lists what is known:\n{out}"
    );
}

#[test]
fn internal_hides_what_might_be_a_public_api() {
    let ws = Workspace::new(&[(
        "a.go",
        "package p\n\nfunc Exported() int {\n\treturn 1\n}\n\nfunc hidden() int {\n\treturn 2\n}\n",
    )]);

    let (all, ok) = ws.run(&["unused", "--language", "go"]);
    assert!(ok, "{all}");
    assert!(all.contains("Exported"), "{all}");
    assert!(all.contains("exported"), "and it is tagged as such:\n{all}");

    let (internal, ok) = ws.run(&["unused", "--language", "go", "--internal"]);
    assert!(ok, "{internal}");
    assert!(internal.contains("hidden"), "{internal}");
    assert!(
        !internal.contains("Exported"),
        "--internal is for what is definitely dead here:\n{internal}"
    );
}

#[test]
fn a_bare_name_defined_twice_is_refused_with_both_locations() {
    // How a target is named, and what happens when the name is not enough.
    let ws = Workspace::new(&[
        (
            "one/a.go",
            "package one\n\nfunc Handle() int {\n\treturn 1\n}\n",
        ),
        (
            "two/b.go",
            "package two\n\nfunc Handle() int {\n\treturn 2\n}\n",
        ),
    ]);
    let (out, ok) = ws.run(&["refs", "Handle"]);
    assert!(!ok, "guessing between them would be worse:\n{out}");
    assert!(out.contains("defined 2 times"), "got:\n{out}");
    assert!(out.contains("one/a.go"), "both are listed:\n{out}");
    assert!(out.contains("two/b.go"), "both are listed:\n{out}");
}

#[test]
fn a_position_names_the_one_that_was_meant() {
    let ws = Workspace::new(&[
        (
            "one/a.go",
            "package one\n\nfunc Handle() int {\n\treturn 1\n}\n\nfunc use() int {\n\treturn Handle()\n}\n",
        ),
        ("two/b.go", "package two\n\nfunc Handle() int {\n\treturn 2\n}\n"),
    ]);
    let (out, ok) = ws.run(&["refs", "one/a.go:3:6"]);
    assert!(ok, "{out}");
    assert!(out.contains("1 reference(s)"), "got:\n{out}");
    assert!(out.contains("one/a.go:8"), "got:\n{out}");
}

#[test]
fn a_diff_is_printed_and_nothing_is_written_without_write() {
    let ws = Workspace::new(&[(
        "a.go",
        "package p\n\nfunc helper() int {\n\treturn 1\n}\n\nfunc caller() int {\n\treturn helper()\n}\n",
    )]);
    let before = std::fs::read_to_string(ws.root().join("a.go")).unwrap();

    let (out, ok) = ws.run(&["rename", "a.go:3:6", "renamed"]);
    assert!(ok, "{out}");
    assert!(out.contains("-func helper"), "a diff is shown:\n{out}");
    assert!(out.contains("+func renamed"), "a diff is shown:\n{out}");
    assert_eq!(
        std::fs::read_to_string(ws.root().join("a.go")).unwrap(),
        before,
        "nothing is written without --write"
    );

    let (out, ok) = ws.run(&["rename", "a.go:3:6", "renamed", "--write"]);
    assert!(ok, "{out}");
    let after = std::fs::read_to_string(ws.root().join("a.go")).unwrap();
    assert!(after.contains("func renamed()"), "{after}");
    assert!(
        after.contains("return renamed()"),
        "the call site in the same package moves too:\n{after}"
    );
}

#[test]
fn json_output_is_json() {
    let ws = workspace();
    let (out, ok) = ws.run(&[
        "--json",
        "duplicates",
        "--language",
        "go",
        "--min-tokens",
        "40",
    ]);
    assert!(ok, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(out.trim()).expect("valid JSON");
    assert!(parsed.is_array(), "got {parsed}");
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

#[test]
fn the_capability_matrix_is_printable_and_totals_add_up() {
    let ws = workspace();
    let (out, ok) = ws.run(&["capabilities"]);
    assert!(ok, "{out}");
    let lines: Vec<&str> = out.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("supported,"))
        .expect("a total line");
    let tail = lines[start..].join(" ");
    let numbers: Vec<usize> = tail
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.parse().ok())
        .collect();
    assert!(numbers.len() >= 4, "expected four totals in: {tail}");
    assert_eq!(
        numbers[0] + numbers[1] + numbers[2],
        numbers[3],
        "supported + n/a + refused should be the whole matrix: {tail}"
    );
}

#[test]
fn a_long_unused_report_says_what_it_is_mostly_made_of() {
    // `spring-petclinic` answers this with 3,554 findings, of which 3,395 are CSS
    // selectors in one vendored stylesheet. True, and useless as read: the fourteen
    // methods somebody came for are somewhere in the scroll, and the count alone does
    // not say so. Below fifty findings the list is its own summary and this stays quiet.
    let mut css = String::new();
    for i in 0..80 {
        css.push_str(&format!(".unused-{i} {{ color: red; }}\n"));
    }
    let ws = Workspace::new(&[
        ("vendor/bundle.css", &css),
        ("keep/a.go", &go_helper("alpha")),
    ]);

    let (out, ok) = ws.run(&["unused"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("80 selector"),
        "the breakdown should name what dominates:\n{out}"
    );
    assert!(
        out.contains("of them in") && out.contains("bundle.css"),
        "one file holding most of the answer should be named:\n{out}"
    );

    // Spread out, and short: no breakdown at all.
    let (small, ok) = ws.run(&["unused", "--language", "go"]);
    assert!(ok, "{small}");
    assert!(
        !small.contains("of them in"),
        "a short answer needs no summary of itself:\n{small}"
    );
}

#[test]
fn what_unused_reports_can_be_given_to_delete() {
    // The next command after `fr unused` is `fr delete`. A name is not enough to name a symbol
    // with: in `helm/helm`, 34 of the first 40 candidates are defined twice. So `fr delete
    // <name>` answered "defined 2 times; give a position", which the list had no way to
    // provide. It reports `file:line:col` now, in both renderings.
    let ws = Workspace::new(&[
        ("a/one.go", "package a\n\nfunc Shared() int {\n\treturn 1\n}\n"),
        (
            "b/two.go",
            "package b\n\nfunc Shared() int {\n\treturn 2\n}\n\nfunc Used() int {\n\treturn Shared()\n}\n",
        ),
    ]);

    let (json, ok) = ws.run(&["unused", "--json"]);
    assert!(ok, "{json}");
    let listed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let dead = listed["unused"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|s| {
            s["name"] == "Shared" && s["file"].as_str().is_some_and(|f| f.contains("a/one.go"))
        })
        .unwrap_or_else(|| panic!("the unreferenced `Shared` should be listed: {json}"));

    let target = format!(
        "{}:{}:{}",
        dead["file"].as_str().expect("a file"),
        dead["line"].as_u64().expect("a line"),
        dead["col"].as_u64().expect("a column")
    );
    let (out, ok) = ws.run(&["delete", &target]);
    assert!(ok, "the position `fr unused` gave should delete:\n{out}");
    assert!(out.contains("deleted Shared"), "got:\n{out}");

    // And the human rendering carries the same position.
    let (text, ok) = ws.run(&["unused"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("one.go:3:6"),
        "the list should say where:\n{text}"
    );
}

#[test]
fn a_closed_stdout_ends_the_run_quietly_instead_of_panicking() {
    // `fr symbols --json | head -c 100` used to panic with a broken-pipe abort once
    // `head` had taken its fill. A reader closing the pipe early is an ordinary end
    // of the run. The fixture is large enough that the pipe buffer cannot hide the
    // closed read end from the writer.
    let mut source = String::new();
    for i in 0..900 {
        source.push_str(&format!("pub fn generated_{i}() -> i64 {{\n    {i}\n}}\n"));
    }
    let ws = Workspace::new(&[("big.rs", &source)]);

    let mut child = Command::new(FR)
        .arg("-C")
        .arg(ws.root())
        .args(["symbols", "--json"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("fr should start");
    // Dropping the read end is `head` exiting; every later write meets a closed pipe.
    drop(child.stdout.take());
    let status = child.wait().expect("fr should finish");
    assert_eq!(
        status.code(),
        Some(0),
        "a closed pipe is a quiet, successful end"
    );
}

#[test]
fn each_kind_of_domain_failure_has_its_own_exit_code() {
    // Every domain failure used to exit 1. A script had to parse prose to tell
    // "no such symbol" from "two symbols answer to that name". The codes are in
    // `fr --help`; this pins them.
    let ws = Workspace::new(&[
        (
            "one/a.go",
            "package one\n\nfunc Handle() int {\n\treturn 1\n}\n\nfunc Other() int {\n\treturn Handle()\n}\n",
        ),
        ("two/b.go", "package two\n\nfunc Handle() int {\n\treturn 2\n}\n"),
    ]);
    let code = |args: &[&str]| {
        Command::new(FR)
            .arg("-C")
            .arg(ws.root())
            .args(args)
            .env("FUN_REFACTOR_CACHE", ws.cache.path())
            .output()
            .expect("fr should run")
            .status
            .code()
    };
    assert_eq!(
        code(&["extract", "one/a.go:4:9-4:2", "x"]),
        Some(2),
        "invalid input is 2, clap's own code."
    );
    assert_eq!(code(&["def", "nosuch"]), Some(3), "not found is 3");
    assert_eq!(
        code(&["remove-flag", "NOT_A_FLAG"]),
        Some(3),
        "a flag nothing declares is not found."
    );
    assert_eq!(code(&["def", "Handle"]), Some(4), "ambiguous is 4");
    // Renaming onto a name the same package already declares is a refusal.
    assert_eq!(
        code(&["rename", "one/a.go:3:6", "Other"]),
        Some(5),
        "a refusal is 5"
    );
    // Deleting a symbol something still calls is a refusal too. It used to exit 1
    // because the message was raised as a plain error instead of a Refusal.
    assert_eq!(
        code(&["delete", "one/a.go:3:6"]),
        Some(5),
        "a blocked delete is a refusal."
    );
    // A position naming a file that does not exist, or a place past the end of one
    // that does, is a target that was not found. Both used to leak exit 1.
    assert_eq!(
        code(&["def", "one/missing.go:3:6"]),
        Some(3),
        "a missing file in a position is not found."
    );
    assert_eq!(
        code(&["def", "one/a.go:999:1"]),
        Some(3),
        "a position past the end of the file is not found."
    );
    // A required position left out, and a position that does not parse, are faults
    // in the command line. Both used to leak exit 1, and the second was silently
    // reinterpreted as a symbol name.
    assert_eq!(
        code(&["rewrite", "invert-if", "--write"]),
        Some(2),
        "a rewrite without a position is invalid input."
    );
    let output = Command::new(FR)
        .arg("-C")
        .arg(ws.root())
        .args(["def", "one/a.go:abc"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a malformed position is invalid input, never a symbol name."
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("'abc' is not a line number"),
        "the parse failure is named: {stderr}"
    );
    let (out, ok) = ws.run(&["--help"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("Exit codes:"),
        "the long help documents the codes:\n{out}"
    );
}

#[test]
fn an_inverted_range_is_refused_with_both_ends_named() {
    // `fr extract "a.go:8:20-8:5"` used to panic in the span constructor, which
    // reported byte offsets instead of the typo.
    let ws = workspace();
    let output = Command::new(FR)
        .arg("-C")
        .arg(ws.root())
        .args(["extract", "keep/a.go:8:20-8:5", "x"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert_eq!(output.status.code(), Some(2), "invalid input exits 2");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("the range's end (8:5) comes before its start (8:20)"),
        "both ends are named:\n{stderr}"
    );
    assert!(
        stderr.contains("path:start_line:start_col-end_line:end_col"),
        "the right shape is spelled out:\n{stderr}"
    );
}

#[test]
fn a_zero_column_is_told_that_columns_start_at_one() {
    // Column 0 used to be read quietly as column 1, which shifts every answer by
    // one for a caller counting from 0.
    let ws = workspace();
    let output = Command::new(FR)
        .arg("-C")
        .arg(ws.root())
        .args(["def", "keep/a.go:3:0"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert_eq!(output.status.code(), Some(2), "invalid input exits 2");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("columns start at 1."), "got:\n{stderr}");
}

#[test]
fn human_listings_print_workspace_relative_paths() {
    // The docs quote relative paths and the diff headers already print them; the
    // listings printed absolute ones. JSON keeps the absolute spelling.
    let ws = Workspace::new(&[(
        "one/a.go",
        "package one\n\nfunc Handle() int {\n\treturn 1\n}\n\nfunc use() int {\n\treturn Handle()\n}\n",
    )]);
    let canonical = ws.root().canonicalize().expect("the root resolves");
    let absolute = canonical.display().to_string();

    for args in [
        vec!["refs", "Handle"],
        vec!["usages", "Handle"],
        vec!["def", "Handle"],
        vec!["symbols"],
        vec!["unused"],
    ] {
        let (out, ok) = ws.run(&args);
        assert!(ok, "{args:?}:\n{out}");
        assert!(out.contains("one/a.go"), "{args:?} names the file:\n{out}");
        assert!(
            !out.contains(&absolute),
            "{args:?} spells it relative to the root:\n{out}"
        );
    }

    // The JSON stays absolute, so a program joining outputs does no path arithmetic.
    let (json, ok) = ws.run(&["refs", "Handle", "--json"]);
    assert!(ok, "{json}");
    assert!(
        json.contains(&absolute),
        "JSON keeps absolute paths:\n{json}"
    );
}

#[test]
fn a_misspelled_symbol_name_gets_a_nearest_name_suggestion() {
    let ws = Workspace::new(&[(
        "a.go",
        "package p\n\nfunc greet() int {\n\treturn 1\n}\n\nfunc use() int {\n\treturn greet()\n}\n",
    )]);
    let (out, ok) = ws.run(&["refs", "gret"]);
    assert!(!ok, "{out}");
    assert!(out.contains("no symbol named 'gret'"), "got:\n{out}");
    assert!(out.contains("Did you mean 'greet'?"), "got:\n{out}");

    // The same names ride in the JSON error as data.
    let (json, _) = ws.run(&["refs", "gret", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(
        json.lines()
            .take_while(|l| !l.starts_with("Error:"))
            .collect::<Vec<_>>()
            .join("\n")
            .as_str(),
    )
    .expect("valid JSON");
    assert_eq!(parsed["error"]["suggestions"][0], "greet", "{parsed}");
}

#[test]
fn translating_a_file_into_its_own_language_points_at_the_listing() {
    let ws = Workspace::new(&[("x.py", "def f():\n    return 1\n")]);
    let (out, ok) = ws.run(&["translate", "x.py", "python"]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("Run 'fr translate x.py' to list the languages it can be written as."),
        "got:\n{out}"
    );
}

#[test]
fn a_diff_header_is_workspace_relative_so_git_apply_accepts_it() {
    // Headers used to read `--- a//absolute/path`, which `git apply -p1` refuses.
    let ws = Workspace::new(&[(
        "src/a.go",
        "package p\n\nfunc helper() int {\n\treturn 1\n}\n\nfunc caller() int {\n\treturn helper()\n}\n",
    )]);
    let (out, ok) = ws.run(&["rename", "src/a.go:3:6", "renamed"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("--- a/src/a.go") && out.contains("+++ b/src/a.go"),
        "headers are relative to the workspace root:\n{out}"
    );
    assert!(
        !out.contains("--- a//"),
        "no absolute header survives:\n{out}"
    );
}

#[test]
fn the_language_filter_has_one_name() {
    // Five commands took `--lang` and two took `--language`, for the same filter, with
    // nothing to say which was which. It cost two mistyped invocations while writing
    // these tests. `--lang` is the name; `--language` stays as an alias so anything
    // already written keeps working.
    let ws = workspace();
    for command in [
        vec!["symbols", "--lang", "go"],
        vec!["symbols", "--language", "go"],
        vec!["unused", "--lang", "go"],
        vec!["unused", "--language", "go"],
        vec!["duplicates", "--lang", "go"],
        vec!["duplicates", "--language", "go"],
    ] {
        let (out, ok) = ws.run(&command);
        assert!(ok, "{command:?} should be accepted:\n{out}");
    }

    // And the two spellings mean the same thing.
    let (short, _) = ws.run(&["symbols", "--lang", "go"]);
    let (long, _) = ws.run(&["symbols", "--language", "go"]);
    assert_eq!(short, long);
}
