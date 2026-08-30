//! The translate listing tells the truth about blocked targets, and the flags that unblock them
//! work.

use std::path::Path;
use std::process::Command;

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn run(root: &Path, args: &[&str]) -> (String, bool) {
    let cache = tempfile::tempdir().unwrap();
    let output = Command::new(FR)
        .arg("-C")
        .arg(root)
        .args(args)
        .env("FUN_REFACTOR_CACHE", cache.path())
        .output()
        .expect("fr should run");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text, output.status.success())
}

const MONEY: &str = "\
def double(total: int) -> int:
    return total * 2
";

#[test]
fn a_blocked_target_is_listed_with_its_reason() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("money.py"), MONEY).unwrap();
    std::fs::write(tmp.path().join("money.ts"), "export const other = 1;\n").unwrap();

    let (text, ok) = run(tmp.path(), &["translate", "money.py"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("typescript") && text.contains("blocked:"),
        "the occupied target is missing or silent:\n{text}"
    );
    assert!(text.contains("--force") && text.contains("--out"), "{text}");
}

#[test]
fn out_chooses_the_destination() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("money.py"), MONEY).unwrap();

    let (text, ok) = run(
        tmp.path(),
        &[
            "translate",
            "money.py",
            "typescript",
            "--out",
            "drafts/m.ts",
            "--write",
        ],
    );
    assert!(ok, "{text}");
    let written = tmp.path().join("drafts/m.ts");
    assert!(written.exists(), "{text}");
    let content = std::fs::read_to_string(written).unwrap();
    assert!(content.contains("function double"), "{content}");
}

#[test]
fn force_overwrites_and_its_absence_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("money.py"), MONEY).unwrap();
    std::fs::write(tmp.path().join("money.ts"), "export const other = 1;\n").unwrap();

    let (text, ok) = run(tmp.path(), &["translate", "money.py", "typescript"]);
    assert!(!ok, "an overwrite went through unasked:\n{text}");
    assert!(text.contains("--force"), "{text}");

    let (text, ok) = run(
        tmp.path(),
        &["translate", "money.py", "typescript", "--force", "--write"],
    );
    assert!(ok, "{text}");
    let content = std::fs::read_to_string(tmp.path().join("money.ts")).unwrap();
    assert!(content.contains("function double"), "{content}");
}

#[test]
fn the_workspace_imports_sweep_reports_what_it_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("a.py"),
        "import os\nimport json\n\n\ndef read() -> str:\n    return json.dumps({})\n",
    )
    .unwrap();

    let (text, ok) = run(tmp.path(), &["imports"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("a.py") && text.contains("removing 1 import(s)"),
        "{text}"
    );

    let (text, ok) = run(tmp.path(), &["imports", "--write"]);
    assert!(ok, "{text}");
    let content = std::fs::read_to_string(tmp.path().join("a.py")).unwrap();
    assert!(!content.contains("import os"), "{content}");
}

#[test]
fn a_rejected_edit_names_the_line_it_stops_parsing_at() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("m.py"),
        "def parse(row: str) -> list[str] | None:\n    match row.split(\",\"):\n        case [a, b]:\n            return [a, b]\n        case _:\n            return None\n",
    )
    .unwrap();

    let (text, ok) = run(tmp.path(), &["extract", "m.py:3:9-4:25", "helper"]);
    assert!(!ok, "{text}");
    assert!(
        text.contains("stops parsing at line"),
        "the refusal carries no evidence:\n{text}"
    );
}

#[test]
fn a_directory_translates_as_a_sweep_with_every_skip_named() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("twice.py"),
        "def twice(n: int) -> int:\n    return n * 2\n",
    )
    .unwrap();
    std::fs::write(
        pkg.join("thrice.py"),
        "def thrice(n: int) -> int:\n    return n * 3\n",
    )
    .unwrap();
    std::fs::write(pkg.join("style.css"), "body { color: red; }\n").unwrap();
    std::fs::write(
        pkg.join("half.go"),
        "package main\n\nfunc half(n int) int {\n\treturn n / 2\n}\n",
    )
    .unwrap();

    let (text, ok) = run(tmp.path(), &["translate", "pkg", "go"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("2 file(s) under"),
        "the sweep counts what it translated. {text}"
    );
    assert!(
        text.contains("1 file(s) are already go."),
        "the sweep counts a file already in the target and never re-translates it. {text}"
    );
    assert!(
        text.contains("1 css file(s) have no reader"),
        "the sweep skips a language with no reader, by name. {text}"
    );
    assert!(
        text.contains("func Twice(n int) int"),
        "the diff shows the drafts. {text}"
    );
    assert!(!pkg.join("twice.go").exists(), "a dry run writes nothing");

    let (text, ok) = run(tmp.path(), &["translate", "pkg", "go", "--write"]);
    assert!(ok, "{text}");
    assert!(pkg.join("twice.go").exists(), "{text}");
    assert!(pkg.join("thrice.go").exists(), "{text}");
}

#[test]
fn a_directory_without_a_target_language_is_refused_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("a.py"), "def a():\n    pass\n").unwrap();

    let (text, ok) = run(tmp.path(), &["translate", "pkg"]);
    assert!(!ok);
    assert!(
        text.contains("name the target language"),
        "the refusal says what is missing: {text}"
    );
}

#[test]
fn a_sweep_skips_an_occupied_destination_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("a.py"), "def a(n: int) -> int:\n    return n\n").unwrap();
    std::fs::write(pkg.join("a.go"), "package main\n").unwrap();

    let (text, ok) = run(tmp.path(), &["translate", "pkg", "go"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("already exists, so this skipped its source. --force overwrites."),
        "an occupied destination is a named skip: {text}"
    );
}
