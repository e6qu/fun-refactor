use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

fn query(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn preview_names_the_file_and_applies_as_a_git_patch() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let input = temporary.path().join("body.rs");
    fs::write(&input, "{ 2 }").unwrap();
    let source = "fn value() -> i32 { 1 }\n";
    let file = root.join("app.rs");
    fs::write(&file, source).unwrap();
    let found = query(&file, &["project", "find", "value"]);
    let handle = found["rows"][0][0].as_str().unwrap();
    let preview = query(
        &file,
        &[
            "author",
            "replace-body",
            handle,
            "--from",
            input.to_str().unwrap(),
        ],
    );
    let diff = preview["diff"].as_str().unwrap();
    assert!(diff.starts_with("--- a/app.rs\n+++ b/app.rs\n"), "{diff}");
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let patch = temporary.path().join("preview.patch");
    fs::write(&patch, diff).unwrap();
    let applied = Command::new("git")
        .current_dir(&root)
        .args(["apply", "--"])
        .arg(&patch)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "fn value() -> i32 { 2 }\n"
    );
}
