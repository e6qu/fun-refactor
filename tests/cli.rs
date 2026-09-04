//! End-to-end tests: the binary, invoked the way a person invokes it.

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

/// A function long enough to match as a duplicate, and dead enough to list.
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
    // The regression this file exists for.
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
    assert!(!ok, "a path that resolves to nothing fails:\n{out}");
    assert!(out.contains("cannot resolve --path"), "got:\n{out}");
}

#[test]
fn a_language_filter_narrows_the_unused_report() {
    let ws = workspace();

    // Both Go functions land in the unfiltered report; the language filter
    // keeps one directory's language and drops the other symbol kinds with it.
    let (everything, ok) = ws.run(&["unused"]);
    assert!(ok, "{everything}");
    assert!(
        everything.contains("beta") && everything.contains("alpha"),
        "both Go functions are findings.\n{everything}"
    );

    let (go_only, ok) = ws.run(&["unused", "--language", "go"]);
    assert!(ok, "{go_only}");
    assert!(
        go_only.contains("beta"),
        "--language go keeps the Go findings.\n{go_only}"
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
    // How to name a target, and what happens where the name falls short.
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
    // `spring-petclinic` answers this with 3,554 findings, of which 3,395 are CSS selectors in
    // one vendored stylesheet.
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
        "the summary names the one file holding most of the answer:\n{out}"
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
    // The next command after `fr unused` is `fr delete`.
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
        .unwrap_or_else(|| panic!("the report holds the unreferenced `Shared`: {json}"));

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
fn a_name_nothing_declares_lists_the_sites_that_write_it() {
    // `<a href="#section-two">` with no element carrying that id had no report anywhere.
    let ws = Workspace::new(&[(
        "index.html",
        "<html><body>\n<a href=\"#section-two\">Jump</a>\n</body></html>\n",
    )]);

    let (said, ok) = ws.run(&["usages", "section-two"]);
    assert!(!ok, "{said}");
    assert!(said.contains("reach no definition"), "got:\n{said}");
    assert!(said.contains("index.html:2:11"), "got:\n{said}");
}

#[test]
fn a_signature_change_that_would_strand_a_read_is_a_refusal() {
    // These printed a considered refusal under exit 1, the code for a crash, while `fr --help`
    // promised 5 for one.
    let ws = Workspace::new(&[
        (
            "m.scss",
            "@mixin box($pad, $col) {\n  padding: $pad;\n  color: $col;\n}\n\
             .a { @include box(4px, red); }\n",
        ),
        (
            "main.tf",
            "module \"net\" {\n  source = \"./modules/net\"\n  cidr = \"10.0.0.0/16\"\n}\n",
        ),
        (
            "modules/net/main.tf",
            "variable \"cidr\" {\n  type = string\n}\n\noutput \"o\" {\n  value = var.cidr\n}\n",
        ),
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
    assert_eq!(code(&["signature", "box", "remove:0"]), Some(5));
    assert_eq!(code(&["signature", "net", "remove:0"]), Some(5));

    let (said, ok) = ws.run(&["signature", "box", "remove:0"]);
    assert!(!ok, "{said}");
    assert!(said.contains("still reads `$pad`"), "got:\n{said}");
}

#[test]
fn each_kind_of_domain_failure_has_its_own_exit_code() {
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
    // Deleting a symbol something still calls is a refusal too.
    assert_eq!(
        code(&["delete", "one/a.go:3:6"]),
        Some(5),
        "a blocked delete is a refusal."
    );
    // A position naming a file that does not exist finds nothing.
    assert_eq!(
        code(&["def", "one/missing.go:3:6"]),
        Some(3),
        "a position naming a missing file finds nothing."
    );
    assert_eq!(
        code(&["def", "one/a.go:999:1"]),
        Some(3),
        "a position past the end of the file finds nothing."
    );
    // A required position left out, and a position that does not parse, are faults in the
    // command line.
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
fn a_pattern_that_matches_nothing_is_not_found() {
    // `fr restructure` reported a typed pattern as a finished job: it printed one line and
    // exited 0.
    let ws = Workspace::new(&[("m.py", "def f(x):\n    return x\n")]);
    let output = Command::new(FR)
        .arg("-C")
        .arg(ws.root())
        .args([
            "restructure",
            "no_such_fn($X)",
            "other($X)",
            "--lang",
            "python",
            "--write",
        ])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert_eq!(output.status.code(), Some(3), "a pattern found nothing");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no python code matches"),
        "the pattern is named:\n{stderr}"
    );

    let json = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(ws.root())
        .args([
            "restructure",
            "no_such_fn($X)",
            "other($X)",
            "--lang",
            "python",
        ])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert_eq!(json.status.code(), Some(3));
    let report: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("--json emits one JSON object");
    assert_eq!(report["error"]["kind"], "not-found");
}

#[test]
fn a_match_the_template_cannot_be_written_over_is_json_and_not_prose() {
    // The skipped occurrences went to stdout in `--json` mode as well.
    let ws = Workspace::new(&[(
        "m.py",
        "def g(x):\n    return x\n\n\ny = g(1)\nz = g(  # keep\n    2\n)\n",
    )]);
    let output = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(ws.root())
        .args(["restructure", "g($X)", "h($X)", "--lang", "python"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("--json emits one JSON object");
    let skipped = report["skipped_occurrences"]
        .as_array()
        .expect("the skipped matches are data");
    assert_eq!(skipped.len(), 1, "got: {report}");
    assert_eq!(skipped[0]["line"], 6);
}

#[test]
fn a_run_and_an_explain_agree_on_how_long_the_recipe_is() {
    let ws = Workspace::new(&[
        ("m.py", "def a(x):\n    return x\n\n\ndef b(x):\n    return a(x)\n"),
        (
            "r.recipe",
            "schema 1\n\nrecipe two {\n  rename to \"a2\" where name=\"a\" kind=function\n  signature \"remove:0\" where name=\"b\" kind=function\n  rename to \"b2\" where name=\"b\" kind=function\n}\n",
        ),
    ]);
    let count = |text: &str| -> String {
        text.lines()
            .find(|line| line.contains("step(s)"))
            .unwrap_or_default()
            .to_string()
    };
    let (explained, ok) = ws.run(&["recipe", "r.recipe", "--explain"]);
    assert!(ok, "{explained}");
    let (ran, _) = ws.run(&["recipe", "r.recipe"]);
    assert_eq!(
        count(&explained),
        count(&ran),
        "explain:\n{explained}\nrun:\n{ran}"
    );
    assert!(
        ran.contains("the run reached 2 of them"),
        "how far the run got is its own line:\n{ran}."
    );
}

#[test]
fn a_recipe_formatter_prints_checks_and_writes_the_canonical_source() {
    let ws = Workspace::new(&[(
        "r.recipe",
        "schema 1\nrecipe tidy { delete id \"dead\" where unused on-refusal allow\nexpect step \"dead\" changed = 0 }\n\n",
    )]);

    let (printed, ok) = ws.run(&["recipe", "fmt", "r.recipe"]);
    assert!(ok, "{printed}");
    assert!(
        printed.contains("delete where unused on-refusal allow id \"dead\""),
        "the formatter keeps the meaning but owns modifier order:\n{printed}."
    );
    let output = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(ws.root())
        .args(["recipe", "fmt", "r.recipe"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("--json emits one document");
    assert_eq!(report["changed"], true);
    assert!(
        report["formatted"]
            .as_str()
            .is_some_and(|text| text.contains("expect step \"dead\" changed = 0 files")),
        "the formatted source is machine-readable: {report}."
    );
    let (checked, ok) = ws.run(&["recipe", "fmt", "r.recipe", "--check"]);
    assert!(!ok, "a compact source needs formatting:\n{checked}");
    assert!(
        checked.contains("not in canonical recipe layout"),
        "{checked}"
    );

    let (written, ok) = ws.run(&["recipe", "fmt", "r.recipe", "--write"]);
    assert!(ok, "{written}");
    let (checked, ok) = ws.run(&["recipe", "fmt", "r.recipe", "--check"]);
    assert!(ok, "{checked}");
    assert!(
        std::fs::read_to_string(ws.root().join("r.recipe"))
            .unwrap()
            .contains("expect step \"dead\" changed = 0 files"),
        "the writer owns optional noise too"
    );
}

#[test]
fn a_recipe_formatter_sweeps_directories_without_partial_writes() {
    let compact = "schema 1\nrecipe tidy { delete where unused }\n";
    let ws = Workspace::new(&[
        ("recipes/first.recipe", compact),
        ("recipes/nested/second.recipe", compact),
        ("recipes/ignored.recipe", compact),
        (".gitignore", "recipes/ignored.recipe\n"),
    ]);

    let (checked, ok) = ws.run(&["recipe", "fmt", "recipes", "--check"]);
    assert!(
        !ok,
        "a directory with compact recipes needs formatting.\n{checked}"
    );
    assert!(checked.contains("2 recipe files"), "{checked}");
    assert!(checked.contains("first.recipe"), "{checked}");
    assert!(checked.contains("second.recipe"), "{checked}");
    assert!(!checked.contains("ignored.recipe"), "{checked}");

    let (written, ok) = ws.run(&["recipe", "fmt", "recipes", "--write"]);
    assert!(ok, "{written}");
    assert!(
        std::fs::read_to_string(ws.root().join("recipes/first.recipe"))
            .unwrap()
            .contains("\n\nrecipe tidy"),
        "the sweep formatted its first file."
    );
    assert_eq!(
        std::fs::read_to_string(ws.root().join("recipes/ignored.recipe")).unwrap(),
        compact,
        "the sweep honours the workspace ignore rules."
    );

    let output = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(ws.root())
        .args(["recipe", "fmt", "recipes"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    assert!(output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the sweep is JSON data");
    assert_eq!(report["files_checked"], 2, "{report}");
    assert_eq!(report["files_needing_format"], 0, "{report}");

    let atomic = Workspace::new(&[("good.recipe", compact), ("bad.recipe", "schema 1\n")]);
    let (failed, ok) = atomic.run(&["recipe", "fmt", "good.recipe", "bad.recipe", "--write"]);
    assert!(!ok, "an invalid recipe stops the complete write.\n{failed}");
    assert_eq!(
        std::fs::read_to_string(atomic.root().join("good.recipe")).unwrap(),
        compact,
        "a malformed later file did not leave the earlier one half-formatted."
    );

    let rooted = Workspace::new(&[(
        "recipes/self-hosted-transaction.recipe",
        "schema 1\n\nrecipe local-only {\n  delete where unused allow-empty\n}\n",
    )]);
    let (explained, ok) = rooted.run(&[
        "recipe",
        "recipes/self-hosted-transaction.recipe",
        "--explain",
    ]);
    assert!(ok, "{explained}");
    assert!(
        explained.contains("recipe local-only"),
        "-C resolves a relative recipe below its own workspace.\n{explained}"
    );
}

#[test]
fn spec_check_reports_the_projects_kernel_anchors_as_json() {
    let output = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["spec", "check"])
        .output()
        .expect("fr should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the spec check is JSON data");
    let anchors = report["anchors"].as_array().expect("anchor reports");
    assert!(anchors.len() >= 2, "{report}");
    assert!(
        anchors.iter().all(|anchor| anchor["status"] == "fresh"),
        "{report}"
    );
    assert_eq!(report["obligations"], 0, "{report}");
}

#[test]
fn spec_sync_previews_then_renews_a_stale_anchor_without_touching_its_model() {
    let ws = Workspace::new(&[
        ("src/code.rs", "pub fn current() -> usize { 2 }\n"),
        (
            "specs/code.lean",
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n-- end.\n",
        ),
    ]);
    let before = std::fs::read_to_string(ws.root().join("specs/code.lean")).unwrap();

    let (preview, ok) = ws.run(&["spec", "sync", "specs"]);
    assert!(ok, "{preview}");
    assert!(preview.contains("deadbeef"), "{preview}");
    assert!(preview.contains("Nothing written"), "{preview}");
    assert_eq!(
        std::fs::read_to_string(ws.root().join("specs/code.lean")).unwrap(),
        before
    );

    let (written, ok) = ws.run(&["spec", "sync", "specs", "--write"]);
    assert!(ok, "{written}");
    let after = std::fs::read_to_string(ws.root().join("specs/code.lean")).unwrap();
    assert!(!after.contains("deadbeef"), "{after}");
    assert!(
        after.contains("def current : Nat := 2\n-- end.\n"),
        "{after}"
    );

    let (checked, ok) = ws.run(&["spec", "check", "specs"]);
    assert!(ok, "{checked}");
}

#[test]
fn spec_sync_refuses_to_renew_any_anchor_when_one_declaration_is_missing() {
    let ws = Workspace::new(&[
        ("src/code.rs", "pub fn current() -> usize { 2 }\n"),
        (
            "specs/code.lean",
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n\n\
             -- fr:spec src/code.rs::gone @ deadbeef\ndef gone : Nat := 0\n-- end.\n",
        ),
    ]);
    let (out, ok) = ws.run(&["spec", "sync", "specs", "--write"]);
    assert!(!ok, "{out}");
    assert!(out.contains("Nothing written"), "{out}");
    assert!(std::fs::read_to_string(ws.root().join("specs/code.lean"))
        .unwrap()
        .contains("current @ deadbeef"));
}

#[test]
fn an_import_kept_for_a_reason_says_what_the_reason_was() {
    // The planner works the reason out for every import it holds back, and the command threw
    // all of them away.
    let ws = Workspace::new(&[(
        "pk/__init__.py",
        "import json\n\n\ndef f():\n    return 1\n",
    )]);
    let (said, ok) = ws.run(&["imports", "pk/__init__.py"]);
    assert!(ok, "{said}");
    assert!(
        said.contains("package __init__.py"),
        "the reason is missing:\n{said}."
    );

    let output = Command::new(FR)
        .arg("--json")
        .arg("-C")
        .arg(ws.root())
        .args(["imports", "pk/__init__.py"])
        .env("FUN_REFACTOR_CACHE", ws.cache.path())
        .output()
        .expect("fr should run");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("--json emits one JSON object");
    let kept = report["kept_imports"]
        .as_array()
        .expect("the kept imports are data");
    assert_eq!(kept.len(), 1, "got: {report}.");
    assert_eq!(kept[0]["line"], 1);
    assert!(
        kept[0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("package __init__.py")),
        "the reason travels into the JSON: {report}."
    );
}

#[test]
fn an_inverted_range_is_refused_with_both_ends_named() {
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
    // The docs quote relative paths and the diff headers already print them; the listings
    // printed absolute ones.
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
        out.contains("Run 'fr translate x.py' for the languages that can hold it."),
        "got:\n{out}"
    );
}

#[test]
fn a_diff_header_is_workspace_relative_so_git_apply_accepts_it() {
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
    // Five commands took `--lang` and two took `--language`, for the same filter, with nothing
    // to say which was which.
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

/// Asked from a subdirectory, `fr` used to answer about that subdirectory.
mod from_a_subdirectory {
    use super::*;

    fn project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("pkg/deep")).unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(
            tmp.path().join("pkg/deep/h.py"),
            "def helper():\n    return 1\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("main.py"),
            "from pkg.deep.h import helper\n\n\ndef go():\n    return helper()\n",
        )
        .unwrap();
        tmp
    }

    fn run_in(dir: &Path, cache: &Path, args: &[&str]) -> (String, bool) {
        let output = Command::new(FR)
            .current_dir(dir)
            .args(args)
            .env("FUN_REFACTOR_CACHE", cache)
            .output()
            .expect("fr should run");
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (text, output.status.success())
    }

    #[test]
    fn a_use_one_directory_up_is_found() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("pkg/deep");
        let (out, ok) = run_in(&deep, cache.path(), &["usages", "h.py:1:5"]);
        assert!(ok, "the command should succeed.\n{out}");
        assert!(
            out.contains("2 use(s)"),
            "the caller above is a use.\n{out}"
        );
        assert!(
            out.contains("the project"),
            "widening the root is said out loud.\n{out}"
        );
    }

    #[test]
    fn delete_refuses_what_the_file_above_still_calls() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("pkg/deep");
        let (out, ok) = run_in(&deep, cache.path(), &["delete", "h.py:1:5"]);
        assert!(!ok, "a used symbol is not deletable.\n{out}");
        assert!(
            out.contains("main.py"),
            "the refusal names the caller, so a reader can deal with it.\n{out}"
        );
    }

    #[test]
    fn a_stated_root_is_left_alone() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("pkg/deep");
        let (out, ok) = run_in(&deep, cache.path(), &["-C", ".", "usages", "h.py:1:5"]);
        assert!(ok, "the command should succeed.\n{out}");
        assert!(
            out.contains("0 use(s)"),
            "`-C .` means this directory, and nothing above it.\n{out}"
        );
        assert!(
            !out.contains("the project"),
            "nothing widened, so nothing says otherwise.\n{out}"
        );
    }
}

/// An ignored file was unreachable, and the refusal blamed the cursor.
mod ignored_files {
    use super::*;

    fn project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("build")).unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "build/\n").unwrap();
        std::fs::write(tmp.path().join("build/g.py"), "def gen():\n    return 1\n").unwrap();
        std::fs::write(tmp.path().join("a.py"), "x = 1\n").unwrap();
        tmp
    }

    fn run(tmp: &Path, cache: &Path, args: &[&str]) -> (String, bool) {
        let output = Command::new(FR)
            .arg("-C")
            .arg(tmp)
            .args(args)
            .env("FUN_REFACTOR_CACHE", cache)
            .output()
            .expect("fr should run");
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        (text, output.status.success())
    }

    #[test]
    fn the_refusal_blames_the_file_and_not_the_position() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let (out, ok) = run(tmp.path(), cache.path(), &["usages", "build/g.py:1:5"]);
        assert!(!ok, "an unindexed file is not answerable.\n{out}");
        assert!(
            out.contains("not in the workspace this indexed"),
            "the reason is the file, not the cursor.\n{out}"
        );
        assert!(out.contains("--no-ignore"), "the way out is named.\n{out}");
    }

    #[test]
    fn no_ignore_brings_the_file_in() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let (out, ok) = run(
            tmp.path(),
            cache.path(),
            &["--no-ignore", "usages", "build/g.py:1:5"],
        );
        assert!(
            ok,
            "the symbol resolves once the scan reaches the file.\n{out}"
        );
        assert!(out.contains("gen"), "and it is the right symbol.\n{out}");
    }

    #[test]
    fn the_flag_named_in_the_advice_exists() {
        let tmp = project();
        let cache = tempfile::tempdir().unwrap();
        let (out, ok) = run(tmp.path(), cache.path(), &["--no-ignore", "scan"]);
        assert!(ok, "--no-ignore is a real flag.\n{out}");
        assert!(out.contains("build/g.py"), "and it reads the file.\n{out}");
    }
}

/// `fr symbols <file>` was a usage error; every sibling listing takes paths.
#[test]
fn symbols_narrows_to_the_paths_it_is_given() {
    let ws = workspace();
    let (all, ok) = ws.run(&["symbols"]);
    assert!(ok, "{all}");
    let (narrowed, ok) = ws.run(&["symbols", "keep"]);
    assert!(ok, "{narrowed}");
    assert!(
        narrowed.contains("alpha"),
        "the report holds the kept directory's symbol:\n{narrowed}"
    );
    assert!(
        !narrowed.contains("beta"),
        "the other directory's symbol is not:\n{narrowed}"
    );
    assert!(
        all.lines().count() > narrowed.lines().count(),
        "the paths narrow the listing"
    );
}

/// Deleting the only user of an import leaves the import, when caution about traits keeps it.
#[test]
fn delete_names_the_import_it_kept() {
    let ws = Workspace::new(&[(
        "lib.rs",
        "use std::collections::BTreeMap;\n\npub fn dead() -> BTreeMap<String, i64> {\n    \
         BTreeMap::new()\n}\n\npub fn live() -> i64 {\n    7\n}\n",
    )]);
    let (out, ok) = ws.run(&["delete", "lib.rs:3:8"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("BTreeMap") && out.contains("stays"),
        "the report names the import that stayed, up front:\n{out}"
    );
}

/// `fr refs --json` says which sites a rename would rewrite.
#[test]
fn refs_json_marks_what_a_rename_would_rewrite() {
    let ws = Workspace::new(&[(
        "sink.go",
        "package p\n\ntype BatchSink struct {\n\tpending []int\n}\n\n\
         func (s *BatchSink) Add(n int) {\n\ts.pending = append(s.pending, n)\n}\n",
    )]);
    let (out, ok) = ws.run(&["--json", "refs", "sink.go:4:2"]);
    assert!(ok, "{out}");
    let json_start = out.find('{').expect("json");
    let payload: serde_json::Value = serde_json::from_str(&out[json_start..]).expect("parses");
    let refs = payload["references"].as_array().expect("references");
    assert!(!refs.is_empty(), "the field has uses:\n{out}");
    assert!(
        refs.iter().all(|r| r["rewritable"] == true),
        "a declared receiver lifts the use into the rewrite:\n{out}"
    );
}
