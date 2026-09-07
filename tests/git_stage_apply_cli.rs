#![cfg(unix)]

use serde_json::{json, Value};
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
        .args(["git", "stage"])
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
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture"]);
}

use std::os::unix::fs::{symlink, PermissionsExt};

fn apply(root: &Path, paths: &[&str]) -> Value {
    let preview = report(root, paths);
    let mut args = paths.to_vec();
    args.extend(["--basis", preview["basis"].as_str().unwrap(), "--write"]);
    let result = report(root, &args);
    assert_eq!(result["applied"], true);
    assert_eq!(result["operation"], "stage-apply");
    assert_eq!(result["basis_verified"], true);
    assert_eq!(result["entries"], preview["entries"]);
    result
}

fn fixture(root: &Path) {
    init(root);
    for name in [
        "selected.txt",
        "unrelated.txt",
        "assume.txt",
        "skip.txt",
        "remove.txt",
    ] {
        fs::write(root.join(name), "old\n").unwrap();
    }
    commit(root);
    fs::write(root.join("selected.txt"), "reviewed\r\n").unwrap();
}

#[test]
fn apply_stages_raw_actions_and_modes_preserving_unrelated_entries_and_flags() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let head = git(root, &["rev-parse", "HEAD"]);
    fs::write(root.join(".gitattributes"), "*.txt text eol=lf\n").unwrap();
    git(root, &["config", "core.filemode", "false"]);
    fs::set_permissions(root.join("selected.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("unrelated.txt"), "already staged\n").unwrap();
    git(root, &["add", "unrelated.txt"]);
    fs::write(root.join("unrelated.txt"), "unstaged\n").unwrap();
    git(root, &["update-index", "--assume-unchanged", "assume.txt"]);
    git(root, &["update-index", "--skip-worktree", "skip.txt"]);
    let unrelated = git(
        root,
        &[
            "ls-files",
            "--stage",
            "-v",
            "-z",
            "--",
            "unrelated.txt",
            "assume.txt",
            "skip.txt",
        ],
    );
    fs::remove_file(root.join("remove.txt")).unwrap();
    let name = "new 名[*]\t\n.txt";
    fs::write(root.join(name), "new raw\r\n").unwrap();
    let result = apply(root, &["selected.txt", "remove.txt", name]);
    assert_eq!(
        result["counts"],
        json!({"paths":3,"add":1,"update":1,"remove":1,"unchanged":0})
    );
    assert_eq!(
        result["durability"],
        json!({"index_replaced":true,"directory_synced":true})
    );
    assert_eq!(git(root, &["show", ":selected.txt"]), b"reviewed\r\n");
    assert_eq!(git(root, &["show", &format!(":{name}")]), b"new raw\r\n");
    assert!(git(root, &["ls-files", "--stage", "selected.txt"]).starts_with(b"100755 "));
    assert!(git(root, &["ls-files", "remove.txt"]).is_empty());
    assert_eq!(
        unrelated,
        git(
            root,
            &[
                "ls-files",
                "--stage",
                "-v",
                "-z",
                "--",
                "unrelated.txt",
                "assume.txt",
                "skip.txt"
            ]
        )
    );
    assert_eq!(
        fs::read(root.join("selected.txt")).unwrap(),
        b"reviewed\r\n"
    );
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"unstaged\n");
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert!(!root.join(".fr-history").exists());
    assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn apply_requires_review_and_refuses_stale_basis_without_writing_objects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let preview = report(root, &["selected.txt"]);
    let oid = preview["entries"][0]["after"]["oid"].as_str().unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    assert!(!fr(root, &["selected.txt", "--write"])
        .output()
        .unwrap()
        .status
        .success());
    error(
        root,
        &["selected.txt", "--write", "--basis", "wrong"],
        "stale staging basis",
    );
    fs::write(root.join("selected.txt"), "changed\n").unwrap();
    error(
        root,
        &[
            "selected.txt",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        "stale staging basis",
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(!git_output(root, &["cat-file", "-e", oid]).status.success());
    assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn apply_preserves_changes_staged_after_review_and_noop_index_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let preview = report(root, &["selected.txt"]);
    fs::write(root.join("unrelated.txt"), "later staged\n").unwrap();
    git(root, &["add", "unrelated.txt"]);
    report(
        root,
        &[
            "selected.txt",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(git(root, &["show", ":unrelated.txt"]), b"later staged\n");
    let index = fs::read(root.join(".git/index")).unwrap();
    let result = apply(root, &["selected.txt"]);
    assert_eq!(result["durability"]["index_replaced"], false);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn apply_supports_unborn_sha256_and_linked_worktree_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    fs::write(root.join("file.txt"), "first\n").unwrap();
    assert!(!root.join(".git/index").exists());
    apply(root, &["file.txt"]);
    assert_eq!(git(root, &["show", ":file.txt"]), b"first\n");
    git(root, &["commit", "-qm", "first"]);
    let work_dir = tempfile::tempdir().unwrap();
    let work = work_dir.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", work.to_str().unwrap()],
    );
    let index = fs::read(root.join(".git/index")).unwrap();
    fs::write(work.join("file.txt"), "linked\n").unwrap();
    fs::create_dir(work.join("nested")).unwrap();
    apply(&work.join("nested"), &["file.txt"]);
    assert_eq!(git(&work, &["show", ":file.txt"]), b"linked\n");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn apply_refuses_existing_locks_symlink_indexes_split_indexes_and_sparse_checkouts() {
    for kind in ["lock", "symlink-lock", "symlink-index", "split", "sparse"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        let expected = match kind {
            "lock" => {
                fs::write(root.join(".git/index.lock"), "owned by another writer").unwrap();
                "cannot acquire Git index.lock"
            }
            "symlink-lock" => {
                symlink("index", root.join(".git/index.lock")).unwrap();
                "cannot acquire Git index.lock"
            }
            "symlink-index" => {
                fs::rename(root.join(".git/index"), root.join(".git/saved-index")).unwrap();
                symlink("saved-index", root.join(".git/index")).unwrap();
                "regular Git index"
            }
            "split" => {
                git(root, &["update-index", "--split-index"]);
                "split indexes"
            }
            _ => {
                git(root, &["config", "core.sparseCheckout", "true"]);
                "sparse checkouts"
            }
        };
        let preview = report(root, &["selected.txt"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        error(
            root,
            &[
                "selected.txt",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
            expected,
        );
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        if kind == "lock" {
            assert_eq!(
                fs::read(root.join(".git/index.lock")).unwrap(),
                b"owned by another writer"
            );
        } else if kind == "symlink-lock" {
            assert!(fs::symlink_metadata(root.join(".git/index.lock"))
                .unwrap()
                .file_type()
                .is_symlink());
        } else {
            assert!(!root.join(".git/index.lock").exists());
        }
    }
}

#[test]
fn apply_preserves_unrelated_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("unrelated.txt"), "other\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("unrelated.txt"), "main\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    fs::write(root.join("selected.txt"), "after conflict\n").unwrap();
    let conflicts = git(root, &["ls-files", "--unmerged", "-z"]);
    apply(root, &["selected.txt"]);
    assert_eq!(git(root, &["ls-files", "--unmerged", "-z"]), conflicts);
    assert_eq!(git(root, &["show", ":selected.txt"]), b"after conflict\n");
}

#[test]
fn apply_disables_hooks_and_ignores_inherited_index_redirection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(
        root.join(".git/hooks/post-index-change"),
        "#!/bin/sh\nprintf 'hook' > hook-ran\n",
    )
    .unwrap();
    fs::set_permissions(
        root.join(".git/hooks/post-index-change"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let preview = report(root, &["selected.txt"]);
    let foreign = root.join("foreign-index");
    fs::copy(root.join(".git/index"), &foreign).unwrap();
    let original = fs::read(&foreign).unwrap();
    let output = fr(
        root,
        &[
            "selected.txt",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    )
    .env("GIT_INDEX_FILE", &foreign)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read(&foreign).unwrap(), original);
    assert!(!root.join("hook-ran").exists());
    assert_eq!(git(root, &["show", ":selected.txt"]), b"reviewed\r\n");
}

#[test]
fn apply_refuses_preparation_failures_source_races_index_races_and_lost_lock_ownership() {
    for (action, message) in [
        ("exit 42", "preparing staging index failed"),
        (
            "printf 'raced\\n' > selected.txt",
            "working files changed during staging write",
        ),
        (
            "cp .git/index .git/index-before-race; printf 'foreign index' > .git/index",
            "Git index changed during staging write",
        ),
        (
            "mv .git/index.lock .git/displaced-lock; printf 'new owner' > .git/index.lock",
            "lock ownership changed",
        ),
        (
            "printf 'corrupt' > \"$GIT_INDEX_FILE\"",
            "preparing staging index failed",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        let preview = report(root, &["selected.txt"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        let actual = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(actual.status.success());
        let shim = root.join("shim");
        fs::create_dir(&shim).unwrap();
        fs::write(shim.join("git"), format!("#!/bin/sh\ncase \" $* \" in\n*' update-index '*)\n  \"$FR_ACTUAL_GIT\" \"$@\" || exit $?\n  {action}\n  ;;\n*) exec \"$FR_ACTUAL_GIT\" \"$@\" ;;\nesac\n")).unwrap();
        fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
        let output = fr(
            root,
            &[
                "selected.txt",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
        )
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env(
            "PATH",
            format!("{}:{}", shim.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
        assert!(!output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains(message),
            "{value}"
        );
        if message == "Git index changed during staging write" {
            assert_eq!(fs::read(root.join(".git/index")).unwrap(), b"foreign index");
        } else {
            assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        }
        if message == "lock ownership changed" {
            assert_eq!(
                fs::read(root.join(".git/index.lock")).unwrap(),
                b"new owner"
            );
        } else {
            assert!(!root.join(".git/index.lock").exists());
        }
        assert!(!fs::read_dir(root.join(".git")).unwrap().any(|item| item
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("fr-stage-")));
    }
}

#[test]
fn apply_preserves_unrelated_intent_to_add_in_version_four_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("intent.txt"), "pending\n").unwrap();
    git(root, &["add", "--intent-to-add", "intent.txt"]);
    git(root, &["update-index", "--index-version", "4"]);
    let before = git(root, &["ls-files", "--debug", "--", "intent.txt"]);
    apply(root, &["selected.txt"]);
    assert_eq!(
        git(root, &["ls-files", "--debug", "--", "intent.txt"]),
        before
    );
    assert!(git(
        root,
        &["diff", "--cached", "--name-only", "--", "intent.txt"]
    )
    .is_empty());
}

#[test]
fn apply_refuses_directory_file_collision_without_removing_unselected_entries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::create_dir(root.join("parent")).unwrap();
    fs::write(root.join("parent/child.txt"), "keep\n").unwrap();
    commit(root);
    fs::remove_dir_all(root.join("parent")).unwrap();
    fs::write(root.join("parent"), "replacement\n").unwrap();
    error(root, &["parent"], "explicit file paths");
    assert_eq!(git(root, &["show", ":parent/child.txt"]), b"keep\n");
}
