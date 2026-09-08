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
        .args(["git", "worktree", "recover"])
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

fn shim(temp: &Path) -> (std::path::PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;
    let real = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(real.status.success());
    let bin = temp.join("bin");
    fs::create_dir(&bin).unwrap();
    fs::write(
        bin.join("git"),
        r#"#!/bin/sh
case " $* " in
  *" cat-file --batch "*)
    "$FR_ACTUAL_GIT" "$@" || exit $?
    if [ "$FR_FAULT" = source-drift ]; then
      "$FR_ACTUAL_GIT" commit --allow-empty -qm drift || exit 90
    fi
    exit 0
    ;;
  *" worktree add "*)
    if [ "$FR_FAULT" = branch-race ]; then
      "$FR_ACTUAL_GIT" branch topic HEAD || exit 90
    fi

    "$FR_ACTUAL_GIT" "$@" || exit $?
    if [ "$FR_FAULT" = after-add ]; then exit 42; fi
    if [ "$FR_FAULT" = file-collision ]; then printf foreign > "$FR_DEST/file.txt"; fi
    exit 0
    ;;
  *" read-tree "*)
    if [ "$FR_FAULT" = before-index ]; then exit 42; fi

    "$FR_ACTUAL_GIT" "$@" || exit $?
    if [ "$FR_FAULT" = after-index ]; then exit 42; fi
    if [ "$FR_FAULT" = recover-collision ]; then printf foreign > "$FR_DEST/file.txt"; fi
    exit 0
    ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(bin.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    (
        bin,
        String::from_utf8(real.stdout).unwrap().trim().to_owned(),
    )
}

fn create(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "worktree", "create"])
        .args(args);
    command
}

fn create_report(root: &Path, args: &[&str]) -> Value {
    let out = create(root, args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn pending(fault: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture();
    let root = temp.path().join("main");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/run"), b"#!/bin/sh\r\nexit 0\r\n").unwrap();
    fs::set_permissions(root.join("nested/run"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("empty"), b"").unwrap();
    fs::write(root.join("binary"), [0, 255, 0, 1]).unwrap();
    commit(&root);
    let preview = create_report(&root, &["../task", "--branch", "topic"]);
    let (bin, real) = shim(temp.path());
    let out = create(
        &root,
        &[
            "../task",
            "--branch",
            "topic",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    )
    .env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    )
    .env("FR_ACTUAL_GIT", real)
    .env("FR_FAULT", fault)
    .env("FR_DEST", temp.path().join("task"))
    .output()
    .unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["applied"],
        Value::Null
    );
    temp
}

fn apply(root: &Path) -> Value {
    let preview = report(root, &["../task"]);
    let value = report(
        root,
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(value["applied"], true, "{value}");
    value
}

#[test]
fn resumes_registered_failures_with_binary_modes_and_read_only_preview() {
    use std::os::unix::fs::PermissionsExt;
    for fault in ["after-add", "before-index", "after-index"] {
        let temp = pending(fault);
        let root = temp.path().join("main");
        let task = temp.path().join("task");
        let receipt = root.join(".git/worktrees/task/fr-creation.json");
        let raw = fs::read(&receipt).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&raw).unwrap()["complete"],
            false
        );
        let index = fs::read(root.join(".git/index")).unwrap();
        let refs = git(&root, &["show-ref"]);
        let preview = report(&root, &["../task", "--limit", "1"]);
        assert_eq!(preview["page"]["total"], 4);
        assert_eq!(preview["missing"].as_array().unwrap().len(), 1);
        assert_eq!(preview["index_action"], "create");
        assert_eq!(fs::read(&receipt).unwrap(), raw);
        assert!(!root.join(".git/worktrees/task/index").exists());
        let value = report(
            &root,
            &[
                "../task",
                "--limit",
                "500",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
        );
        assert_eq!(value["applied"], true, "{fault}: {value}");
        assert_eq!(fs::read(task.join("binary")).unwrap(), [0, 255, 0, 1]);
        assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"base\n");
        assert_eq!(
            fs::read(task.join("nested/run")).unwrap(),
            b"#!/bin/sh\r\nexit 0\r\n"
        );
        assert_ne!(
            fs::metadata(task.join("nested/run"))
                .unwrap()
                .permissions()
                .mode()
                & 0o100,
            0
        );
        assert_eq!(git(&task, &["status", "--porcelain"]), b"");
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(git(&root, &["show-ref"]), refs);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&receipt).unwrap()).unwrap()["complete"],
            true
        );
        fs::remove_file(task.join("empty")).unwrap();
        error(&root, &["../task"], "already complete");
        assert!(!task.join("empty").exists());
    }
}

#[test]
fn preserves_matching_files_and_existing_index_without_replacing_inodes() {
    use std::os::unix::fs::MetadataExt;
    let temp = pending("file-collision");
    let root = temp.path().join("main");
    let task = temp.path().join("task");
    error(&root, &["../task"], "metadata file");
    assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"foreign");
    fs::write(task.join("file.txt"), "base\n").unwrap();
    let inode = fs::metadata(task.join("file.txt")).unwrap().ino();
    let index_path = root.join(".git/worktrees/task/index");
    let index = fs::read(&index_path).unwrap();
    let index_inode = fs::metadata(&index_path).unwrap().ino();
    let preview = report(&root, &["../task"]);
    assert_eq!(preview["index_action"], "preserve");
    apply(&root);
    assert_eq!(fs::metadata(task.join("file.txt")).unwrap().ino(), inode);
    assert_eq!(fs::metadata(&index_path).unwrap().ino(), index_inode);
    assert_eq!(fs::read(index_path).unwrap(), index);
}

#[test]
fn refuses_foreign_files_directories_symlinks_and_wrong_modes() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for variant in [
        "extra",
        "ignored",
        "directory",
        "symlink",
        "mode",
        "changed",
    ] {
        let temp = pending("before-index");
        let root = temp.path().join("main");
        let task = temp.path().join("task");
        match variant {
            "extra" => fs::write(task.join("extra"), "keep").unwrap(),
            "ignored" => {
                fs::write(root.join(".git/info/exclude"), "extra\n").unwrap();
                fs::write(task.join("extra"), "keep").unwrap();
            }
            "directory" => fs::create_dir(task.join("extra")).unwrap(),
            "symlink" => symlink(root.join("file.txt"), task.join("file.txt")).unwrap(),
            "mode" => {
                fs::write(task.join("file.txt"), "base\n").unwrap();
                fs::set_permissions(task.join("file.txt"), fs::Permissions::from_mode(0o755))
                    .unwrap();
            }
            "changed" => fs::write(task.join("file.txt"), "edit\n").unwrap(),
            _ => unreachable!(),
        }
        let out = fr(&root, &["../task"]).output().unwrap();
        assert!(!out.status.success(), "{variant}");
        assert!(!root.join(".git/worktrees/task/index").exists());
    }
}

#[test]
fn refuses_staged_changes_flags_intent_to_add_and_branch_changes() {
    for variant in [
        "staged", "assume", "skip", "intent", "branch", "commit", "merge",
    ] {
        let temp = pending("before-index");
        let root = temp.path().join("main");
        let task = temp.path().join("task");
        git(&task, &["read-tree", "HEAD"]);
        match variant {
            "staged" => {
                fs::write(task.join("file.txt"), "edit\n").unwrap();
                git(&task, &["add", "file.txt"]);
                fs::write(task.join("file.txt"), "base\n").unwrap();
            }
            "assume" => {
                git(&task, &["update-index", "--assume-unchanged", "file.txt"]);
            }
            "skip" => {
                git(&task, &["update-index", "--skip-worktree", "file.txt"]);
            }
            "intent" => {
                git(&task, &["rm", "--cached", "empty"]);
                fs::write(task.join("empty"), "").unwrap();
                git(&task, &["add", "-N", "empty"]);
            }
            "branch" => {
                git(&task, &["symbolic-ref", "HEAD", "refs/heads/main"]);
            }
            "commit" => {
                git(&task, &["commit", "--allow-empty", "-qm", "advance"]);
            }
            "merge" => fs::write(root.join(".git/worktrees/task/MERGE_HEAD"), "pending\n").unwrap(),
            _ => unreachable!(),
        }
        let index = fs::read(root.join(".git/worktrees/task/index")).unwrap();
        assert!(
            !fr(&root, &["../task"]).output().unwrap().status.success(),
            "{variant}"
        );
        assert_eq!(
            fs::read(root.join(".git/worktrees/task/index")).unwrap(),
            index
        );
    }
}

#[test]
fn basis_detects_new_matching_content_and_cross_invocation() {
    let temp = pending("before-index");
    let root = temp.path().join("main");
    let task = temp.path().join("task");
    let preview = report(&root, &["../task"]);
    assert_eq!(
        report(&task, &["."])["destination"],
        task.canonicalize().unwrap().to_str().unwrap()
    );
    fs::write(task.join("file.txt"), "base\n").unwrap();
    error(
        &root,
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale worktree recovery basis",
    );
    let preview = report(&root, &["../task"]);
    error(
        &task,
        &[
            task.to_str().unwrap(),
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        "stale worktree recovery basis",
    );
    assert!(!root.join(".git/worktrees/task/index").exists());
    apply(&root);
}

#[test]
fn requires_receipt_and_refuses_replaced_ownership_and_malformed_records() {
    for variant in ["missing", "malformed", "complete", "destination", "gitlink"] {
        let temp = pending("before-index");
        let root = temp.path().join("main");
        let task = temp.path().join("task");
        let receipt = root.join(".git/worktrees/task/fr-creation.json");
        match variant {
            "missing" => fs::remove_file(&receipt).unwrap(),
            "malformed" => fs::write(&receipt, "{}").unwrap(),
            "complete" => {
                let mut value: Value =
                    serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
                value["complete"] = Value::Bool(true);
                fs::write(&receipt, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "destination" => {
                fs::rename(&task, temp.path().join("old-task")).unwrap();
                fs::create_dir(&task).unwrap();
                fs::copy(temp.path().join("old-task/.git"), task.join(".git")).unwrap();
            }
            "gitlink" => {
                fs::rename(task.join(".git"), temp.path().join("old-link")).unwrap();
                fs::copy(temp.path().join("old-link"), task.join(".git")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            !fr(&root, &["../task"]).output().unwrap().status.success(),
            "{variant}"
        );
        assert!(!root.join(".git/worktrees/task/index").exists());
    }
}

#[test]
fn respects_existing_locks_and_can_retry_partial_recovery() {
    let temp = pending("before-index");
    let root = temp.path().join("main");
    let task = temp.path().join("task");
    let metadata = root.join(".git/worktrees/task");
    let preview = report(&root, &["../task"]);
    fs::write(metadata.join("fr-creation.lock"), "foreign").unwrap();
    error(
        &root,
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "cannot acquire worktree lock",
    );
    assert_eq!(
        fs::read(metadata.join("fr-creation.lock")).unwrap(),
        b"foreign"
    );
    fs::remove_file(metadata.join("fr-creation.lock")).unwrap();
    fs::write(metadata.join("index.lock"), "foreign").unwrap();
    let value = report(
        &root,
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(value["applied"], Value::Null);
    assert_eq!(fs::read(metadata.join("index.lock")).unwrap(), b"foreign");
    fs::remove_file(metadata.join("index.lock")).unwrap();
    let real = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let out = fr(
        &root,
        &[
            "../task",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    )
    .env(
        "PATH",
        format!(
            "{}:{}",
            temp.path().join("bin").display(),
            std::env::var("PATH").unwrap()
        ),
    )
    .env(
        "FR_ACTUAL_GIT",
        String::from_utf8(real.stdout).unwrap().trim(),
    )
    .env("FR_FAULT", "recover-collision")
    .env("FR_DEST", &task)
    .output()
    .unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["applied"],
        Value::Null
    );
    assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"foreign");
    assert_eq!(fs::read(task.join("binary")).unwrap(), [0, 255, 0, 1]);
    fs::write(task.join("file.txt"), "base\n").unwrap();
    apply(&root);
}
