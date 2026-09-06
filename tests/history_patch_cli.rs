use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn fr(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .env("PATH", "/nonexistent-fr-patch-test")
        .args(["--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "core.autocrlf=false"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn saved_plan_exports_without_git_and_applies_without_disturbing_unrelated_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let original = "fn helper() {}\nfn main() { helper(); }";
    fs::write(root.join("app.rs"), original).unwrap();
    let plan: Value = serde_json::from_slice(&success(fr(
        root,
        &["--json", "rename", "helper", "renamed", "--save-plan"],
    )))
    .unwrap();
    assert_eq!(plan["transaction"], 1);
    let patch = success(fr(root, &["history", "patch", "1"]));
    assert!(patch.starts_with(b"diff --git "));
    let report: Value =
        serde_json::from_slice(&success(fr(root, &["--json", "history", "patch", "1"]))).unwrap();
    assert_eq!(report["patch"].as_str().unwrap().as_bytes(), patch);
    assert_eq!(report["format"], "git-text-diff");
    assert_eq!(report["mode_scope"], "regular-or-executable");
    assert_eq!(report["status"], "planned");
    assert_eq!(report["reverse"], false);
    assert_eq!(report["files"], 1);
    success(git(root, &["init", "-q"], b""));
    fs::write(root.join("unrelated.txt"), "baseline\n").unwrap();
    success(git(root, &["add", "app.rs", "unrelated.txt"], b""));
    fs::write(root.join("unrelated.txt"), "staged\n").unwrap();
    success(git(root, &["add", "unrelated.txt"], b""));
    fs::write(root.join("unrelated.txt"), "unstaged\n").unwrap();
    fs::write(root.join("untracked.txt"), "untracked\n").unwrap();
    fs::write(root.join("app.rs"), "conflicting drift\n").unwrap();
    let status = success(git(root, &["status", "--porcelain=v1", "-z"], b""));
    let index = fs::read(root.join(".git/index")).unwrap();
    let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
    assert_eq!(success(fr(root, &["history", "patch", "1"])), patch);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        success(git(root, &["status", "--porcelain=v1", "-z"], b"")),
        status
    );
    assert!(!git(root, &["apply", "--check", "-"], &patch)
        .status
        .success());
    assert!(!git(root, &["apply", "-"], &patch).status.success());
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        "conflicting drift\n"
    );
    fs::write(root.join("app.rs"), original).unwrap();
    success(git(root, &["apply", "--check", "-"], &patch));
    success(git(root, &["apply", "-"], &patch));
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        original.replace("helper", "renamed")
    );
    let reverse = success(fr(root, &["history", "patch", "1", "--reverse"]));
    success(git(root, &["apply", "--check", "-"], &reverse));
    success(git(root, &["apply", "-"], &reverse));
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("untracked.txt")).unwrap(),
        "untracked\n"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(root.join(".fr-history/state.json")).unwrap(),
        journal
    );
}

#[test]
fn failed_patch_export_has_no_partial_patch_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let missing = fr(dir.path(), &["history", "patch", "42"]);
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    fs::write(dir.path().join("app.rs"), "fn helper() {}\n").unwrap();
    success(fr(
        dir.path(),
        &["rename", "helper", "renamed", "--save-plan"],
    ));
    let path = dir.path().join(".fr-history/state.json");
    let mut journal: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    journal["records"][0]["changes"][0]["after"]["content"] = "binary\0snapshot".into();
    fs::write(path, serde_json::to_vec(&journal).unwrap()).unwrap();
    let binary = fr(dir.path(), &["history", "patch", "1"]);
    assert!(!binary.status.success());
    assert!(binary.stdout.is_empty());
    assert!(String::from_utf8_lossy(&binary.stderr).contains("binary snapshot"));
}
