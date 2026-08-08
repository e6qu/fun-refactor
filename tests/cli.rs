//! End-to-end tests: the binary, invoked the way a person invokes it.
//!
//! Every other test in this suite calls the library directly, which leaves the layer
//! between a command line and that library untested. Two real bugs lived there and
//! neither was visible from below: `--path` filters were built by joining the default
//! root `.`, so they never matched the absolute paths in the index and every filtered
//! report came back empty — a clean bill of health that meant "the filter matched
//! nothing". A refactoring tool that reports "no findings" when it means "I looked in
//! the wrong place" is worse than one that crashes.
//!
//! So these run `fr` itself: argument parsing, path resolution, exit codes and the
//! text a person actually reads.

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
