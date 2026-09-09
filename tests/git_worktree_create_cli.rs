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

fn apply(root: &Path, path: &str, branch: &str) -> Value {
    let preview = report(root, &[path, "--branch", branch]);
    let value = report(
        root,
        &[
            path,
            "--branch",
            branch,
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(value["applied"], true, "{value}");
    value
}

#[test]
fn previews_without_mutation_and_creates_raw_isolated_checkout() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture();
    let root = temp.path().join("main");
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/run\nscript"), b"#!/bin/sh\r\nexit 0\r\n").unwrap();
    fs::set_permissions(
        root.join("src/run\nscript"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(root.join("binary"), [0, 255, 1, 0]).unwrap();
    commit(&root);
    let committed_index = git(&root, &["ls-files", "--stage", "-z"]);
    fs::write(root.join("file.txt"), "staged\n").unwrap();
    git(&root, &["add", "file.txt"]);
    fs::write(root.join("file.txt"), "working\n").unwrap();
    fs::write(root.join("untracked"), "retain\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let head = git(&root, &["rev-parse", "HEAD"]);
    let refs = git(&root, &["show-ref"]);
    let registrations = git(&root, &["worktree", "list", "--porcelain", "-z"]);
    let preview = report(
        &root,
        &["../task", "--branch", "agent/task", "--limit", "1"],
    );
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["page"]["total"], 3);
    assert_eq!(preview["files"].as_array().unwrap().len(), 1);
    assert!(!temp.path().join("task").exists());
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(
        git(&root, &["worktree", "list", "--porcelain", "-z"]),
        registrations
    );
    let value = report(
        &root,
        &[
            "../task",
            "--branch",
            "agent/task",
            "--limit",
            "500",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(value["applied"], true, "{value}");
    let task = temp.path().join("task");
    assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(task.join("binary")).unwrap(), [0, 255, 1, 0]);
    assert_eq!(
        fs::read(task.join("src/run\nscript")).unwrap(),
        b"#!/bin/sh\r\nexit 0\r\n"
    );
    assert_ne!(
        fs::metadata(task.join("src/run\nscript"))
            .unwrap()
            .permissions()
            .mode()
            & 0o100,
        0
    );
    assert_eq!(git(&task, &["ls-files", "--stage", "-z"]), committed_index);
    assert_eq!(git(&task, &["status", "--porcelain"]), b"");
    assert_eq!(
        git(&task, &["symbolic-ref", "HEAD"]),
        b"refs/heads/agent/task\n"
    );
    assert!(git(&root, &["worktree", "list", "--porcelain"])
        .windows(6)
        .any(|part| part == b"locked"));
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"working\n");
    assert_eq!(fs::read(root.join("untracked")).unwrap(), b"retain\n");
}

#[test]
fn stale_bases_cover_start_branch_destination_registrations_and_parent() {
    let temp = fixture();
    let root = temp.path().join("main");
    let preview = report(&root, &["../task", "--branch", "topic"]);
    let basis = preview["basis"].as_str().unwrap();
    error(
        &root,
        &["../other", "--branch", "topic", "--basis", basis, "--write"],
        "stale worktree creation basis",
    );
    error(
        &root,
        &["../task", "--branch", "other", "--basis", basis, "--write"],
        "stale worktree creation basis",
    );
    git(&root, &["commit", "--allow-empty", "-qm", "advance"]);
    error(
        &root,
        &["../task", "--branch", "topic", "--basis", basis, "--write"],
        "stale worktree creation basis",
    );
    let preview = report(&root, &["../task", "--branch", "topic"]);
    git(&root, &["worktree", "add", "--detach", "-q", "../other"]);
    error(
        &root,
        &[
            "../task",
            "--branch",
            "topic",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale worktree creation basis",
    );
    let parent = temp.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let preview = report(&root, &["../parent/task", "--branch", "topic"]);
    fs::rename(&parent, temp.path().join("old-parent")).unwrap();
    fs::create_dir(&parent).unwrap();
    error(
        &root,
        &[
            "../parent/task",
            "--branch",
            "topic",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale worktree creation basis",
    );
    assert!(
        !git_output(&root, &["show-ref", "--verify", "refs/heads/topic"])
            .status
            .success()
    );
}

#[test]
fn refuses_occupied_paths_branches_nested_destinations_and_unsupported_states() {
    use std::os::unix::fs::symlink;
    let temp = fixture();
    let root = temp.path().join("main");
    fs::create_dir(temp.path().join("occupied")).unwrap();
    error(
        &root,
        &["../occupied", "--branch", "topic"],
        "destination already exists",
    );
    fs::write(temp.path().join("file"), "keep").unwrap();
    error(
        &root,
        &["../file", "--branch", "topic"],
        "destination already exists",
    );
    symlink("missing", temp.path().join("link")).unwrap();
    error(
        &root,
        &["../link", "--branch", "topic"],
        "destination already exists",
    );
    error(
        &root,
        &["nested", "--branch", "topic"],
        "outside Git repositories",
    );
    error(
        &root,
        &[".git/new", "--branch", "topic"],
        "overlaps Git metadata",
    );
    error(
        &root,
        &["../missing/task", "--branch", "topic"],
        "parent must already exist",
    );
    error(
        &root,
        &["../task", "--branch", "main"],
        "branch already exists",
    );
    error(
        &root,
        &["../task", "--branch", "bad..branch"],
        "Git inspection failed",
    );
    error(
        &root,
        &["../task", "--branch", "HEAD"],
        "Git inspection failed",
    );
    error(
        &root,
        &["../task", "--branch", "topic", "--limit", "0"],
        "limit must",
    );
    assert!(!fr(&root, &["../task", "--branch", "topic", "--write"])
        .output()
        .unwrap()
        .status
        .success());
    git(
        &root,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    );
    error(&root, &["../task", "--branch", "alias"], "already symbolic");
    assert_eq!(fs::read(temp.path().join("file")).unwrap(), b"keep");
}

#[test]
fn supports_repository_worktree_configuration() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(&root, &["config", "extensions.worktreeConfig", "true"]);
    let preview = report(&root, &["../task", "--branch", "topic"]);
    assert_eq!(preview["worktree_config"], true, "{preview}");
    let created = report(
        &root,
        &[
            "../task",
            "--branch",
            "topic",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(created["applied"], true, "{created}");
    assert_eq!(created["worktree_config"], true, "{created}");
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(created["ownership_record"].as_str().unwrap()).unwrap()
        )
        .unwrap()["worktree_config"],
        true
    );
}

#[test]
fn source_edits_do_not_stale_basis_and_filters_hooks_and_redirects_do_not_run() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture();
    let root = temp.path().join("main");
    fs::write(
        root.join(".gitattributes"),
        "* filter=sentinel text eol=crlf ident\n",
    )
    .unwrap();
    commit(&root);
    git(
        &root,
        &["config", "filter.sentinel.smudge", "touch filter-ran; cat"],
    );
    git(
        &root,
        &["config", "filter.sentinel.clean", "touch filter-ran; cat"],
    );
    git(&root, &["config", "filter.sentinel.required", "true"]);
    git(&root, &["config", "core.autocrlf", "true"]);
    fs::write(
        root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\ntouch hook-ran\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(
        root.join(".git/hooks/post-checkout"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let preview = report(&root, &["../task", "--branch", "topic"]);
    fs::write(root.join("file.txt"), "working change\n").unwrap();
    let out = fr(
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
    .env("GIT_DIR", "/missing")
    .env("GIT_INDEX_FILE", "/missing")
    .env("GIT_WORK_TREE", "/missing")
    .output()
    .unwrap();
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["applied"], true, "{value}");
    let task = temp.path().join("task");
    assert_eq!(fs::read(task.join("file.txt")).unwrap(), b"base\n");
    assert!(!task.join("filter-ran").exists() && !task.join("hook-ran").exists());
    assert!(!root.join("filter-ran").exists() && !root.join("hook-ran").exists());
}

#[test]
fn supports_linked_invocation_sha256_and_empty_committed_tree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    error(
        &root,
        &["../task", "--branch", "topic"],
        "Git inspection failed",
    );
    git(&root, &["commit", "--allow-empty", "-qm", "empty"]);
    let value = apply(&root, "../task", "topic");
    assert_eq!(value["commit"].as_str().unwrap().len(), 64);
    assert_eq!(value["page"]["total"], 0);
    let task = temp.path().join("task");
    fs::create_dir(task.join("nested")).unwrap();
    let value = apply(&task.join("nested"), "../second", "second");
    assert_eq!(value["page"]["total"], 0);
    assert_eq!(
        git(&temp.path().join("second"), &["symbolic-ref", "HEAD"]),
        b"refs/heads/second\n"
    );
}

#[test]
fn refuses_symlink_submodule_missing_blob_and_registered_destination() {
    use std::os::unix::fs::symlink;
    let temp = fixture();
    let root = temp.path().join("main");
    symlink("file.txt", root.join("link")).unwrap();
    commit(&root);
    error(
        &root,
        &["../task", "--branch", "topic"],
        "regular blobs only",
    );
    git(&root, &["rm", "-q", "link"]);
    let head = String::from_utf8(git(&root, &["rev-parse", "HEAD"])).unwrap();
    git(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},module", head.trim()),
        ],
    );
    git(&root, &["commit", "-qm", "module"]);
    error(
        &root,
        &["../task", "--branch", "topic"],
        "regular blobs only",
    );
    git(&root, &["update-index", "--force-remove", "module"]);
    git(&root, &["commit", "-qm", "remove module"]);
    git(&root, &["worktree", "add", "--detach", "-q", "../missing"]);
    fs::remove_dir_all(temp.path().join("missing")).unwrap();
    error(
        &root,
        &["../missing", "--branch", "topic"],
        "overlaps a registered worktree",
    );
    let oid = String::from_utf8(git(&root, &["rev-parse", "HEAD:file.txt"])).unwrap();
    fs::remove_file(
        root.join(".git/objects")
            .join(&oid[..2])
            .join(oid[2..].trim()),
    )
    .unwrap();
    error(&root, &["../task", "--branch", "topic"], "blob is missing");
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

#[test]
fn creation_failures_report_partial_state_and_preserve_foreign_content() {
    for fault in [
        "after-add",
        "before-index",
        "after-index",
        "file-collision",
        "branch-race",
    ] {
        let temp = fixture();
        let root = temp.path().join("main");
        let destination = temp.path().join("task");
        let index = fs::read(root.join(".git/index")).unwrap();
        let head = git(&root, &["rev-parse", "HEAD"]);
        let preview = report(&root, &["../task", "--branch", "topic"]);
        let (bin, real) = shim(temp.path());
        let out = fr(
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
        .env("FR_DEST", &destination)
        .output()
        .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["applied"], Value::Null, "{fault}: {value}");
        assert_eq!(value["destination_present"], true);
        assert!(value["warning"]
            .as_str()
            .unwrap()
            .contains("before retrying"));
        assert_eq!(value["observed_branch_commit"], preview["commit"]);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
        if fault == "file-collision" {
            assert_eq!(fs::read(destination.join("file.txt")).unwrap(), b"foreign");
        } else {
            assert!(!destination.join("file.txt").exists());
        }
    }
}

#[test]
fn rejects_oversized_blobs_and_case_collisions_before_creation() {
    let temp = fixture();
    let root = temp.path().join("main");
    let oid = String::from_utf8(git(&root, &["rev-parse", "HEAD:file.txt"])).unwrap();
    git(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{},Dir/a", oid.trim()),
        ],
    );
    git(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{},dir/b", oid.trim()),
        ],
    );
    git(&root, &["commit", "-qm", "case collision"]);
    error(
        &root,
        &["../task", "--branch", "topic"],
        "paths collide under case folding",
    );
    git(&root, &["update-index", "--force-remove", "Dir/a", "dir/b"]);
    let file = fs::File::create(root.join("large")).unwrap();
    file.set_len(33 * 1024 * 1024).unwrap();
    commit(&root);
    error(&root, &["../task", "--branch", "topic"], "32 MiB per blob");
    assert!(!temp.path().join("task").exists());
    assert!(
        !git_output(&root, &["show-ref", "--verify", "refs/heads/topic"])
            .status
            .success()
    );
}

#[test]
fn explicit_old_commit_checkout_and_drift_after_blob_capture() {
    let temp = fixture();
    let root = temp.path().join("main");
    let old = String::from_utf8(git(&root, &["rev-parse", "HEAD"])).unwrap();
    fs::write(root.join("file.txt"), "new commit\n").unwrap();
    commit(&root);
    let preview = report(&root, &["../old", "--branch", "old", "--from", "HEAD~1"]);
    assert_eq!(preview["commit"], old.trim());
    let value = report(
        &root,
        &[
            "../old",
            "--branch",
            "old",
            "--from",
            "HEAD~1",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(value["applied"], true, "{value}");
    assert_eq!(
        fs::read(temp.path().join("old/file.txt")).unwrap(),
        b"base\n"
    );
    let preview = report(&root, &["../task", "--branch", "topic"]);
    let (bin, real) = shim(temp.path());
    let out = fr(
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
    .env("FR_FAULT", "source-drift")
    .output()
    .unwrap();
    assert!(!out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("basis changed before writing"),
        "{value}"
    );
    assert!(!temp.path().join("task").exists());
    assert!(
        !git_output(&root, &["show-ref", "--verify", "refs/heads/topic"])
            .status
            .success()
    );
}
