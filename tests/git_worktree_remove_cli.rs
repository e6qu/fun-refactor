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
        .args(["git", "worktree", "remove"])
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

fn create(root: &Path) -> Value {
    let invoke = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(root)
            .args(["git", "worktree", "create", "../owned", "--branch", "owned"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let preview = invoke(&[]);
    let result = invoke(&["--write", "--basis", preview["basis"].as_str().unwrap()]);
    assert_eq!(result["applied"], true, "{result}");
    result
}

#[test]
fn removes_reviewed_worktree_archives_metadata_and_retains_branch() {
    let temp = fixture();
    let root = temp.path().join("main");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/binary"), [0, 255, 13]).unwrap();
    commit(&root);
    let created = create(&root);
    let index = fs::read(root.join(".git/index")).unwrap();
    let preview = report(&root, &["../owned", "--limit", "1"]);
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["page"]["total"], 2);
    let result = report(
        &root,
        &[
            "../owned",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["applied"], true, "{result}");
    assert!(!temp.path().join("owned").exists());
    assert!(!Path::new(created["ownership_record"].as_str().unwrap())
        .parent()
        .unwrap()
        .exists());
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        String::from_utf8(git(&root, &["rev-parse", "owned"]))
            .unwrap()
            .trim(),
        preview["commit"].as_str().unwrap()
    );
    let record = Path::new(result["removal_record"].as_str().unwrap());
    let archived: Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
    assert!(archived["metadata_bytes"]["index"].is_array());
    assert!(record.with_file_name("complete").is_file());
    assert_eq!(
        String::from_utf8(git(&root, &["worktree", "list", "--porcelain"]))
            .unwrap()
            .matches("worktree ")
            .count(),
        1
    );
}

#[test]
fn accepts_later_commits_on_owned_branch() {
    let temp = fixture();
    let root = temp.path().join("main");
    create(&root);
    let target = temp.path().join("owned");
    fs::write(target.join("file.txt"), "later\n").unwrap();
    commit(&target);
    let preview = report(&root, &["../owned"]);
    let result = report(
        &root,
        &[
            "../owned",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["applied"], true, "{result}");
    assert_eq!(git(&root, &["show", "owned:file.txt"]), b"later\n");
}

#[test]
fn refuses_changed_missing_untracked_ignored_and_empty_directory_content() {
    for kind in [
        "changed",
        "missing",
        "untracked",
        "ignored",
        "directory",
        "mode",
        "symlink",
    ] {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = fixture();
        let root = temp.path().join("main");
        fs::write(root.join(".gitignore"), "ignored\n").unwrap();
        commit(&root);
        create(&root);
        let target = temp.path().join("owned");
        match kind {
            "changed" => fs::write(target.join("file.txt"), "changed").unwrap(),
            "missing" => fs::remove_file(target.join("file.txt")).unwrap(),
            "directory" => fs::create_dir(target.join("extra")).unwrap(),
            "mode" => {
                fs::set_permissions(target.join("file.txt"), fs::Permissions::from_mode(0o755))
                    .unwrap()
            }
            "symlink" => {
                fs::remove_file(target.join("file.txt")).unwrap();
                symlink(root.join("file.txt"), target.join("file.txt")).unwrap();
            }
            other => fs::write(target.join(other), "preserve").unwrap(),
        }
        let out = fr(&root, &["../owned"]).output().unwrap();
        assert!(!out.status.success(), "{kind}");
        assert!(target.join(".git").is_file());
    }
}

#[test]
fn refuses_stale_basis_self_removal_unknown_metadata_and_foreign_locks() {
    let temp = fixture();
    let root = temp.path().join("main");
    let created = create(&root);
    let target = temp.path().join("owned");
    let preview = report(&root, &["../owned"]);
    error(&target, &["."], "invoking worktree");
    let metadata = Path::new(created["ownership_record"].as_str().unwrap())
        .parent()
        .unwrap();
    for name in [
        "index.lock",
        "HEAD.lock",
        "fr-creation.lock",
        "fr-stage",
        "MERGE_HEAD",
        "private",
    ] {
        fs::write(metadata.join(name), "preserve").unwrap();
        assert!(!fr(&root, &["../owned"]).output().unwrap().status.success());
        assert_eq!(fs::read(metadata.join(name)).unwrap(), b"preserve");
        fs::remove_file(metadata.join(name)).unwrap();
    }
    fs::write(target.join("file.txt"), "later\n").unwrap();
    commit(&target);
    error(
        &root,
        &[
            "../owned",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        "stale worktree removal basis",
    );
    assert!(target.join("file.txt").is_file());
}

#[test]
fn refuses_unowned_worktrees_and_changed_index() {
    let temp = fixture();
    let root = temp.path().join("main");
    git(
        &root,
        &["worktree", "add", "-q", "-b", "foreign", "../foreign"],
    );
    error(&root, &["../foreign"], "no readable ownership receipt");
    create(&root);
    let target = temp.path().join("owned");
    git(&target, &["update-index", "--assume-unchanged", "file.txt"]);
    assert!(!fr(&root, &["../owned"]).output().unwrap().status.success());
    assert!(target.join("file.txt").is_file());
}

#[test]
fn late_content_survives_partial_removal_with_a_durable_record() {
    use std::os::unix::fs::PermissionsExt;
    for fault in ["extra", "changed"] {
        let temp = fixture();
        let root = temp.path().join("main");
        create(&root);
        let preview = report(&root, &["../owned"]);
        let actual = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let shim = bin.join("git");
        fs::write(
            &shim,
            r##"#!/bin/sh
case " $* " in
  *" check-ref-format "*)
    for record in "$FR_ROOT"/.git/fr-worktree-removal-*/record.json; do
      if [ -f "$record" ]; then
        count=0
        if [ -f "$FR_COUNT" ]; then count=$(cat "$FR_COUNT"); fi
        count=$((count + 1))
        echo "$count" > "$FR_COUNT"
        if [ "$count" = 3 ]; then
          if [ "$FR_FAULT" = extra ]; then
            echo preserve > "$FR_DEST/late-ignored"
          else
            echo preserve > "$FR_DEST/file.txt"
          fi
        fi
      fi
    done
    ;;
esac
exec "$FR_ACTUAL_GIT" "$@"
"##,
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        let out = fr(
            &root,
            &[
                "../owned",
                "--write",
                "--basis",
                preview["basis"].as_str().unwrap(),
            ],
        )
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env("FR_ROOT", &root)
        .env("FR_DEST", temp.path().join("owned"))
        .env("FR_COUNT", temp.path().join("count"))
        .env("FR_FAULT", fault)
        .output()
        .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(result["applied"].is_null(), "{result}");
        let preserved = if fault == "extra" {
            "late-ignored"
        } else {
            "file.txt"
        };
        assert_eq!(
            fs::read(temp.path().join("owned").join(preserved)).unwrap(),
            b"preserve\n"
        );
        let record = Path::new(result["removal_record"].as_str().unwrap());
        assert!(record.is_file());
        assert!(!record.with_file_name("complete").exists());
        assert_eq!(git(&root, &["show", "owned:file.txt"]), b"base\n");
        if fault == "extra" {
            assert!(!temp.path().join("owned/file.txt").exists());
        }
    }
}

#[test]
fn refuses_pending_receipts_changed_branches_and_metadata_drift() {
    let temp = fixture();
    let root = temp.path().join("main");
    let created = create(&root);
    let receipt = Path::new(created["ownership_record"].as_str().unwrap());
    let original = fs::read(receipt).unwrap();
    let mut pending: Value = serde_json::from_slice(&original).unwrap();
    pending["complete"] = false.into();
    fs::write(receipt, serde_json::to_vec(&pending).unwrap()).unwrap();
    error(&root, &["../owned"], "completed ownership receipt");
    fs::write(receipt, original).unwrap();
    let preview = report(&root, &["../owned"]);
    fs::write(
        receipt.parent().unwrap().join("COMMIT_EDITMSG"),
        b"review changed",
    )
    .unwrap();
    error(
        &root,
        &[
            "../owned",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale worktree removal basis",
    );
    let target = temp.path().join("owned");
    git(&target, &["switch", "-c", "different"]);
    assert!(!fr(&root, &["../owned"]).output().unwrap().status.success());
    assert!(target.join("file.txt").is_file());
}

#[test]
fn supports_sha256_and_invocation_from_another_linked_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    fs::write(root.join("file.txt"), b"base\n").unwrap();
    commit(&root);
    create(&root);
    git(
        &root,
        &["worktree", "add", "-q", "-b", "caller", "../caller"],
    );
    let caller = temp.path().join("caller");
    let preview = report(&caller, &["../owned"]);
    assert_eq!(preview["commit"].as_str().unwrap().len(), 64);
    let result = report(
        &caller,
        &[
            "../owned",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["applied"], true, "{result}");
    assert_eq!(git(&caller, &["show", "owned:file.txt"]), b"base\n");
    assert!(caller.join("file.txt").is_file());
}
