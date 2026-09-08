#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn git_output(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-c",
            "user.name=fr fixture",
            "-c",
            "user.email=fr@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap()
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = git_output(root, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn fr(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fr"));
    cmd.args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "worktree", "list"])
        .args(args);
    cmd
}
fn report(root: &Path, args: &[&str]) -> Value {
    let out = fr(root, args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn error(root: &Path, args: &[&str], expected: &str) {
    let out = fr(root, args).output().unwrap();
    assert!(!out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains(expected),
        "{value}"
    );
}
fn init(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "fr fixture"]);
    git(root, &["config", "user.email", "fr@example.invalid"]);
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture"]);
}

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    init(&root);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    commit(&root);
    temp
}

fn add(root: &Path, path: &Path, branch: Option<&str>) {
    let mut args = vec!["worktree", "add", "-q"];
    if let Some(branch) = branch {
        args.extend(["-b", branch]);
    } else {
        args.push("--detach");
    }
    args.extend([path.to_str().unwrap(), "HEAD"]);
    git(root, &args);
}

#[test]
fn pages_registered_worktrees_and_identifies_linked_invocation() {
    let temp = fixture();
    let root = temp.path().join("main");
    let linked = temp.path().join("linked space\nline");
    let detached = temp.path().join("detached");
    add(&root, &linked, Some("topic"));
    add(&root, &detached, None);
    let first = report(&root, &["--limit", "1"]);
    assert_eq!(first["page"]["total"], 3);
    assert_eq!(first["entries"][0]["current"], true);
    assert_eq!(first["entries"][0]["main"], true);
    assert_eq!(first["entries"][0]["branch"], "refs/heads/main");
    assert_eq!(first["counts"]["detached"], 1);
    let cursor = first["page"]["next"].as_str().unwrap();
    let rest = report(&root, &["--limit", "500", "--cursor", cursor]);
    assert_eq!(rest["entries"].as_array().unwrap().len(), 2);
    assert_eq!(rest["page"]["next"], Value::Null);
    assert_eq!(rest["worktree_revision"], first["worktree_revision"]);
    assert!(rest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == linked.canonicalize().unwrap().to_str().unwrap()));
    fs::create_dir(linked.join("nested")).unwrap();
    let other = report(&linked.join("nested"), &[]);
    let current = other["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["current"] == true)
        .unwrap();
    assert_eq!(current["branch"], "refs/heads/topic");
    assert_eq!(current["main"], false);
    assert_eq!(other["common_directory"], first["common_directory"]);
    error(&linked, &["--cursor", cursor], "stale cursor");
}

#[test]
fn reports_lock_reasons_and_missing_registrations_without_pruning() {
    let temp = fixture();
    let root = temp.path().join("main");
    let locked = temp.path().join("locked");
    let missing = temp.path().join("missing");
    add(&root, &locked, Some("locked-topic"));
    add(&root, &missing, None);
    let reason = format!("hold\n{}", "é".repeat(300));
    git(
        &root,
        &[
            "worktree",
            "lock",
            "--reason",
            &reason,
            locked.to_str().unwrap(),
        ],
    );
    fs::remove_dir_all(&missing).unwrap();
    let before = git(
        &root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    );
    let value = report(&root, &[]);
    assert_eq!(value["counts"]["locked"], 1);
    assert_eq!(value["counts"]["prunable"], 1);
    let rows = value["entries"].as_array().unwrap();
    let row = rows.iter().find(|row| row["locked"] == true).unwrap();
    assert!(row["lock_reason"].as_str().unwrap().starts_with("hold\n"));
    assert_eq!(row["lock_reason_truncated"], true);
    assert!(row["lock_reason"].as_str().unwrap().len() <= 512);
    let row = rows.iter().find(|row| row["prunable"] == true).unwrap();
    assert!(row["prune_reason"]
        .as_str()
        .unwrap()
        .contains("non-existent"));
    assert_eq!(
        before,
        git(
            &root,
            &["worktree", "list", "--porcelain", "-z", "--expire=now"]
        )
    );
}

#[test]
fn cursor_rejects_head_branch_lock_and_membership_changes() {
    let temp = fixture();
    let root = temp.path().join("main");
    let linked = temp.path().join("linked");
    add(&root, &linked, Some("topic"));
    let cursor = || {
        report(&root, &["--limit", "1"])["page"]["next"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let first = cursor();
    git(&linked, &["commit", "--allow-empty", "-qm", "advance"]);
    error(&root, &["--cursor", &first], "stale cursor");
    let second = cursor();
    git(&linked, &["branch", "-m", "renamed"]);
    error(&root, &["--cursor", &second], "stale cursor");
    let third = cursor();
    git(&root, &["worktree", "lock", linked.to_str().unwrap()]);
    error(&root, &["--cursor", &third], "stale cursor");
    git(&root, &["worktree", "unlock", linked.to_str().unwrap()]);
    let fourth = cursor();
    add(&root, &temp.path().join("extra"), None);
    error(&root, &["--cursor", &fourth], "stale cursor");
}

#[test]
fn metadata_inspection_preserves_dirty_indexes_and_ignores_content_filters() {
    let temp = fixture();
    let root = temp.path().join("main");
    let linked = temp.path().join("linked");
    add(&root, &linked, Some("topic"));
    let first = report(&root, &["--limit", "1"]);
    for tree in [&root, &linked] {
        fs::write(tree.join("file.txt"), "staged\n").unwrap();
        git(tree, &["add", "file.txt"]);
        fs::write(tree.join("file.txt"), "working\n").unwrap();
        fs::write(tree.join("untracked"), "untouched\n").unwrap();
        fs::write(tree.join(".gitattributes"), "* filter=sentinel\n").unwrap();
    }
    git(
        &root,
        &["config", "filter.sentinel.clean", "touch filter-ran; cat"],
    );
    git(&root, &["config", "core.fsmonitor", "touch monitor-ran"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let linked_index = root.join(".git/worktrees/linked/index");
    let other_index = fs::read(&linked_index).unwrap();
    let value = report(
        &root,
        &["--cursor", first["page"]["next"].as_str().unwrap()],
    );
    assert_eq!(value["worktree_revision"], first["worktree_revision"]);
    assert_eq!(value["content_inspected"], false);
    assert_eq!(index, fs::read(root.join(".git/index")).unwrap());
    assert_eq!(other_index, fs::read(linked_index).unwrap());
    for tree in [&root, &linked] {
        assert_eq!(fs::read(tree.join("file.txt")).unwrap(), b"working\n");
        assert_eq!(fs::read(tree.join("untracked")).unwrap(), b"untouched\n");
        assert!(!tree.join("filter-ran").exists());
        assert!(!tree.join("monitor-ran").exists());
    }
}

#[test]
fn unborn_sha256_and_bare_primary_from_linked_tree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("unborn");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    let value = report(&root, &[]);
    assert_eq!(value["entries"][0]["unborn"], true);
    assert_eq!(value["entries"][0]["head"], Value::Null);
    assert!(!root.join(".git/index").exists());
    fs::write(root.join("file"), "data").unwrap();
    commit(&root);
    assert_eq!(
        report(&root, &[])["entries"][0]["head"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let bare = temp.path().join("bare");
    git(
        temp.path(),
        &[
            "clone",
            "--bare",
            "-q",
            root.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    let linked = temp.path().join("linked");
    add(&bare, &linked, Some("topic"));
    let value = report(&linked, &[]);
    assert_eq!(value["entries"][0]["bare"], true);
    assert_eq!(value["entries"][0]["main"], true);
    assert_eq!(value["entries"][1]["current"], true);
    error(&bare, &[], "inside a Git working tree");
}

#[test]
fn rejects_bad_pages_and_ignores_inherited_git_redirects() {
    let temp = fixture();
    let root = temp.path().join("main");
    error(&root, &["--limit", "0"], "limit must");
    error(&root, &["--limit", "501"], "limit must");
    error(&root, &["--cursor", "bad"], "invalid Git worktree cursor");
    let value = report(&root, &[]);
    let prefix = format!("frwt1:{}", value["worktree_revision"].as_str().unwrap());
    error(
        &root,
        &["--cursor", &format!("{prefix}:x")],
        "invalid Git worktree offset",
    );
    error(
        &root,
        &["--cursor", &format!("{prefix}:2")],
        "beyond the result set",
    );
    assert_eq!(
        report(&root, &["--cursor", &format!("{prefix}:1")])["page"]["returned"],
        0
    );
    let out = fr(&root, &[])
        .env("GIT_DIR", "/missing")
        .env("GIT_WORK_TREE", "/missing")
        .env("GIT_INDEX_FILE", "/missing")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.bare")
        .env("GIT_CONFIG_VALUE_0", "true")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    error(temp.path(), &[], "inside a Git working tree");
}

#[test]
fn cursor_includes_omitted_reason_bytes_and_prunable_state() {
    let temp = fixture();
    let root = temp.path().join("main");
    let linked = temp.path().join("linked");
    add(&root, &linked, None);
    let reason = format!("{}first", "x".repeat(512));
    git(
        &root,
        &[
            "worktree",
            "lock",
            "--reason",
            &reason,
            linked.to_str().unwrap(),
        ],
    );
    let first = report(&root, &["--limit", "1"]);
    let all = report(&root, &[]);
    git(&root, &["worktree", "unlock", linked.to_str().unwrap()]);
    let reason = format!("{}second", "x".repeat(512));
    git(
        &root,
        &[
            "worktree",
            "lock",
            "--reason",
            &reason,
            linked.to_str().unwrap(),
        ],
    );
    assert_eq!(report(&root, &[])["entries"], all["entries"]);
    error(
        &root,
        &["--cursor", first["page"]["next"].as_str().unwrap()],
        "stale cursor",
    );
    git(&root, &["worktree", "unlock", linked.to_str().unwrap()]);
    let first = report(&root, &["--limit", "1"]);
    fs::remove_dir_all(&linked).unwrap();
    error(
        &root,
        &["--cursor", first["page"]["next"].as_str().unwrap()],
        "stale cursor",
    );
    assert_eq!(report(&root, &[])["counts"]["prunable"], 1);
}

#[test]
fn symlink_invocation_resolves_current_and_non_utf8_metadata_refuses() {
    use std::os::unix::fs::symlink;

    let temp = fixture();
    let root = temp.path().join("main");
    let alias = temp.path().join("alias");
    symlink(&root, &alias).unwrap();
    assert_eq!(report(&root, &[]), report(&alias, &[]));
    let linked = temp.path().join("linked");
    add(&root, &linked, None);
    fs::write(root.join(".git/worktrees/linked/locked"), b"invalid\xff\n").unwrap();
    error(&root, &[], "non-UTF-8 Git worktree output");
}
