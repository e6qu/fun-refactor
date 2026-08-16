//! The translate listing tells the truth about blocked targets, and the flags that
//! unblock them work.
//!
//! A target whose destination existed used to vanish from the listing, which taught
//! the reader the pair did not exist. And with no `--out` and no `--force`, the only
//! way past the collision was to move the other file by hand.

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
