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
        .args(["git", "worktree", "create"])
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

fn existing(root: &Path, branch: &str) -> Value {
    let preview = report(root, &["../task", "--existing-branch", branch]);
    let result = report(
        root,
        &[
            "../task",
            "--existing-branch",
            branch,
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["applied"], true, "{result}");
    result
}

fn other(root: &Path, command: &str, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "worktree", command])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn existing_checkout_preserves_refs_reflog_configuration_and_source_changes() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let temp = fixture();
    let root = temp.path().join("main");
    fs::write(root.join("binary"), [0, 255, 13]).unwrap();
    fs::write(root.join("run"), b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(root.join("run"), fs::Permissions::from_mode(0o755)).unwrap();
    commit(&root);
    git(&root, &["branch", "feature"]);
    git(&root, &["config", "branch.feature.remote", "origin"]);
    git(
        &root,
        &["config", "branch.feature.merge", "refs/heads/upstream"],
    );
    fs::write(root.join("file.txt"), "staged\n").unwrap();
    git(&root, &["add", "file.txt"]);
    fs::write(root.join("file.txt"), "working\n").unwrap();
    fs::write(root.join("untracked"), "preserve").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    let refs = git(&root, &["show-ref"]);
    let reflog = fs::read(root.join(".git/logs/refs/heads/feature")).unwrap();
    let ref_inode = fs::metadata(root.join(".git/refs/heads/feature"))
        .unwrap()
        .ino();
    let preview = report(
        &root,
        &["../task", "--existing-branch", "feature", "--limit", "1"],
    );
    assert_eq!(preview["branch_action"], "retain");
    assert_eq!(preview["from"], "refs/heads/feature");
    assert!(!temp.path().join("task").exists());
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    let result = report(
        &root,
        &[
            "../task",
            "--existing-branch",
            "feature",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["applied"], true, "{result}");
    let task = temp.path().join("task");
    assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(task.join("binary")).unwrap(), [0, 255, 13]);
    assert_eq!(
        fs::metadata(task.join("run")).unwrap().mode() & 0o777,
        0o755
    );
    assert_eq!(
        git(&task, &["symbolic-ref", "HEAD"]),
        b"refs/heads/feature\n"
    );
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(
        fs::metadata(root.join(".git/refs/heads/feature"))
            .unwrap()
            .ino(),
        ref_inode
    );
    assert_eq!(
        fs::read(root.join(".git/logs/refs/heads/feature")).unwrap(),
        reflog
    );
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"working\n");
    let receipt: Value =
        serde_json::from_slice(&fs::read(result["ownership_record"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(receipt["existing_branch"], true);
    let preview = other(&root, "remove", &["../task"]);
    let removed = other(
        &root,
        "remove",
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(removed["applied"], true);
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
}

#[test]
fn refuses_missing_symbolic_occupied_and_ambiguous_branches() {
    let temp = fixture();
    let root = temp.path().join("main");
    error(
        &root,
        &["../task", "--existing-branch", "missing"],
        "does not exist",
    );
    git(
        &root,
        &["update-ref", "refs/remotes/origin/remote-only", "HEAD"],
    );
    error(
        &root,
        &["../task", "--existing-branch", "remote-only"],
        "does not exist",
    );
    error(
        &root,
        &["../task", "--existing-branch", "main"],
        "registered worktree",
    );
    git(
        &root,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    );
    error(
        &root,
        &["../task", "--existing-branch", "alias"],
        "already symbolic",
    );
    git(&root, &["branch", "feature"]);
    git(&root, &["tag", "feature"]);
    assert!(!fr(&root, &["../task", "--existing-branch", "feature"])
        .output()
        .unwrap()
        .status
        .success());
    git(&root, &["tag", "-d", "feature"]);
    git(
        &root,
        &["worktree", "add", "--lock", "../occupied", "feature"],
    );
    error(
        &root,
        &["../task", "--existing-branch", "feature"],
        "registered worktree",
    );
    fs::remove_dir_all(temp.path().join("occupied")).unwrap();
    error(
        &root,
        &["../task", "--existing-branch", "feature"],
        "registered worktree",
    );
    assert!(!temp.path().join("task").exists());
}

#[test]
fn mode_flags_and_changed_branch_tips_require_a_new_review() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["branch", "feature"]);
    for args in [
        vec!["../task"],
        vec!["../task", "--branch", "new", "--existing-branch", "feature"],
        vec!["../task", "--existing-branch", "feature", "--from", "HEAD"],
    ] {
        assert!(!fr(&root, &args).output().unwrap().status.success());
    }
    let preview = report(&root, &["../task", "--existing-branch", "feature"]);
    fs::write(root.join("file.txt"), "later\n").unwrap();
    commit(&root);
    git(&root, &["update-ref", "refs/heads/feature", "HEAD"]);
    error(
        &root,
        &[
            "../task",
            "--existing-branch",
            "feature",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale worktree creation basis",
    );
    let applied = existing(&root, "feature");
    assert_eq!(applied["branch_action"], "retain");
    assert_eq!(
        fs::read(temp.path().join("task/file.txt")).unwrap(),
        b"later\n"
    );
}

#[test]
fn packed_sha256_branch_can_be_attached_from_a_linked_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    fs::write(root.join("file.txt"), b"base\n").unwrap();
    commit(&root);
    git(&root, &["branch", "agent/feature"]);
    git(
        &root,
        &["worktree", "add", "-q", "-b", "caller", "../caller"],
    );
    git(&root, &["pack-refs", "--all", "--prune"]);
    let packed = fs::read(root.join(".git/packed-refs")).unwrap();
    let result = existing(&temp.path().join("caller"), "agent/feature");
    assert_eq!(result["commit"].as_str().unwrap().len(), 64);
    assert_eq!(
        git(&temp.path().join("task"), &["symbolic-ref", "HEAD"]),
        b"refs/heads/agent/feature\n"
    );
    assert_eq!(fs::read(root.join(".git/packed-refs")).unwrap(), packed);
}

fn shim(root: &Path) -> (std::path::PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let bin = root.parent().unwrap().join("bin");
    fs::create_dir(&bin).unwrap();
    let path = bin.join("git");
    fs::write(
        &path,
        r##"#!/bin/sh
case " $* " in
  *" worktree add "*)
    case "$FR_FAULT" in
      after-add)
        "$FR_ACTUAL_GIT" "$@" || exit $?
        exit 97
        ;;
      crash-before-receipt)
        "$FR_ACTUAL_GIT" "$@" || exit $?
        kill -KILL "$PPID"
        sleep 1
        exit 97
        ;;
      locked-update)
        if "$FR_ACTUAL_GIT" -C "$FR_ROOT" update-ref -d refs/heads/feature 2>/dev/null; then
          echo unsafe > "$FR_RESULT"
        else
          echo blocked > "$FR_RESULT"
        fi
        ;;
      occupied)
        "$FR_ACTUAL_GIT" -C "$FR_ROOT" worktree add --no-checkout "$FR_FOREIGN" feature || exit $?
        ;;
    esac
    ;;
esac
exec "$FR_ACTUAL_GIT" "$@"
"##,
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    (
        bin,
        String::from_utf8(actual.stdout).unwrap().trim().to_owned(),
    )
}

fn fault(root: &Path, mode: &str) -> Value {
    let preview = report(root, &["../task", "--existing-branch", "feature"]);
    let (bin, actual) = shim(root);
    let out = fr(
        root,
        &[
            "../task",
            "--existing-branch",
            "feature",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    )
    .env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    )
    .env("FR_ACTUAL_GIT", actual)
    .env("FR_FAULT", mode)
    .env("FR_ROOT", root)
    .env("FR_RESULT", root.parent().unwrap().join("lock-result"))
    .env("FR_FOREIGN", root.parent().unwrap().join("foreign"))
    .output()
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn holds_existing_branch_lease_and_preserves_foreign_locks() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["branch", "feature"]);
    let preview = report(&root, &["../task", "--existing-branch", "feature"]);
    let lock = root.join(".git/refs/heads/feature.lock");
    fs::write(&lock, "foreign").unwrap();
    error(
        &root,
        &[
            "../task",
            "--existing-branch",
            "feature",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        "branch preservation lease",
    );
    assert!(!temp.path().join("task").exists());
    assert_eq!(fs::read(&lock).unwrap(), b"foreign");
    fs::remove_file(lock).unwrap();
    let result = fault(&root, "locked-update");
    assert_eq!(result["applied"], true, "{result}");
    assert_eq!(
        fs::read(temp.path().join("lock-result")).unwrap(),
        b"blocked\n"
    );
    assert_eq!(
        git(&root, &["rev-parse", "feature"]),
        git(&root, &["rev-parse", "main"])
    );
}

#[test]
fn interrupted_existing_checkout_recovers_and_removes_without_changing_branch_history() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["branch", "feature"]);
    let refs = git(&root, &["show-ref"]);
    let log = fs::read(root.join(".git/logs/refs/heads/feature")).unwrap();
    let result = fault(&root, "after-add");
    assert!(result["applied"].is_null(), "{result}");
    let preview = other(&root, "recover", &["../task"]);
    let receipt: Value =
        serde_json::from_slice(&fs::read(preview["ownership_record"].as_str().unwrap()).unwrap())
            .unwrap();
    assert_eq!(receipt["existing_branch"], true);
    assert_eq!(receipt["complete"], false);
    let recovered = other(
        &root,
        "recover",
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(recovered["applied"], true, "{recovered}");
    assert_eq!(
        fs::read(temp.path().join("task/file.txt")).unwrap(),
        b"base\n"
    );
    let preview = other(&root, "remove", &["../task"]);
    let removed = other(
        &root,
        "remove",
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(removed["applied"], true);
    let inspected = other(
        &root,
        "resume-removal",
        &[removed["removal_record"].as_str().unwrap()],
    );
    assert_eq!(inspected["state"], "complete-marker-present");
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(
        fs::read(root.join(".git/logs/refs/heads/feature")).unwrap(),
        log
    );
}

#[test]
fn prepared_recovery_preserves_an_existing_branch_and_its_history() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["branch", "feature"]);
    let refs = git(&root, &["show-ref"]);
    let log = fs::read(root.join(".git/logs/refs/heads/feature")).unwrap();
    let preview = report(&root, &["../task", "--existing-branch", "feature"]);
    let (bin, actual) = shim(&root);
    let out = fr(
        &root,
        &[
            "../task",
            "--existing-branch",
            "feature",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    )
    .env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    )
    .env("FR_ACTUAL_GIT", actual)
    .env("FR_FAULT", "crash-before-receipt")
    .env("FR_ROOT", &root)
    .output()
    .unwrap();
    assert!(!out.status.success());
    let recovery = other(&root, "recover", &["../task"]);
    assert_eq!(recovery["ownership_state"], "prepared", "{recovery}");
    let recovered = other(
        &root,
        "recover",
        &[
            "../task",
            "--basis",
            recovery["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(recovered["applied"], true, "{recovered}");
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(
        fs::read(root.join(".git/logs/refs/heads/feature")).unwrap(),
        log
    );
    assert_eq!(
        git(&temp.path().join("task"), &["symbolic-ref", "HEAD"]),
        b"refs/heads/feature\n"
    );
}

#[test]
fn refuses_a_branch_claimed_between_review_and_registration() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["branch", "feature"]);
    let refs = git(&root, &["show-ref"]);
    let result = fault(&root, "occupied");
    assert!(result["applied"].is_null(), "{result}");
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(
        git(&temp.path().join("foreign"), &["symbolic-ref", "HEAD"]),
        b"refs/heads/feature\n"
    );
    assert!(!temp.path().join("task/file.txt").exists());
}

#[test]
fn receipts_and_removal_archives_without_added_modes_remain_readable() {
    let temp = fixture();
    let root = temp.path().join("main");
    let preview = report(&root, &["../task", "--branch", "feature"]);
    let created = report(
        &root,
        &[
            "../task",
            "--branch",
            "feature",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(created["applied"], true);
    let receipt_path = Path::new(created["ownership_record"].as_str().unwrap());
    let mut receipt: Value = serde_json::from_slice(&fs::read(receipt_path).unwrap()).unwrap();
    receipt.as_object_mut().unwrap().remove("existing_branch");
    receipt.as_object_mut().unwrap().remove("worktree_config");
    receipt.as_object_mut().unwrap().remove("preparation");
    fs::write(receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let preview = other(&root, "remove", &["../task"]);
    let removed = other(
        &root,
        "remove",
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(removed["applied"], true);
    let record_path = Path::new(removed["removal_record"].as_str().unwrap());
    let mut record: Value = serde_json::from_slice(&fs::read(record_path).unwrap()).unwrap();
    record["proposal"]
        .as_object_mut()
        .unwrap()
        .remove("existing_branch");
    record["proposal"]
        .as_object_mut()
        .unwrap()
        .remove("worktree_config");
    record["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("existing_branch");
    record["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("worktree_config");
    record["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("preparation");
    fs::write(record_path, serde_json::to_vec(&record).unwrap()).unwrap();
    fs::remove_file(record_path.with_file_name("complete")).unwrap();
    let preview = other(&root, "resume-removal", &[record_path.to_str().unwrap()]);
    assert_eq!(preview["can_resume"], true, "{preview}");
    let resumed = other(
        &root,
        "resume-removal",
        &[
            record_path.to_str().unwrap(),
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(resumed["applied"], true, "{resumed}");
}
