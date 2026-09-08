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
        .args(["git", "worktree", "resume-removal"])
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

fn prepared(root: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    create(root);
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(root)
            .args(["git", "worktree", "remove", "../owned"])
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
    let preview = run(&[]);
    let bin = root.parent().unwrap().join("bin");
    fs::create_dir(&bin).unwrap();
    let shim = bin.join("git");
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    fs::write(
        &shim,
        r##"#!/bin/sh
case " $* " in
  *" check-ref-format "*)
    for record in "$FR_ROOT"/.git/fr-worktree-removal-*/record.json; do
      if [ -f "$record" ]; then
        echo preserve > "$FR_DEST/late-file"
      fi
    done
    ;;
esac
exec "$FR_ACTUAL_GIT" "$@"
"##,
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args([
            "git",
            "worktree",
            "remove",
            "../owned",
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ])
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env("FR_ROOT", root)
        .env("FR_DEST", root.parent().unwrap().join("owned"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(result["applied"].is_null(), "{result}");
    fs::remove_file(root.parent().unwrap().join("owned/late-file")).unwrap();
    result["removal_record"].as_str().unwrap().into()
}

fn resume(root: &Path, record: &Path) -> Value {
    let preview = report(root, &[record.to_str().unwrap()]);
    assert_eq!(preview["can_resume"], true, "{preview}");
    report(
        root,
        &[
            record.to_str().unwrap(),
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
    )
}

#[test]
fn inspects_without_writes_and_resumes_partial_checkout() {
    let temp = fixture();
    let root = temp.path().join("main");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/binary"), [0, 255, 13]).unwrap();
    commit(&root);
    let record = prepared(&root);
    let target = temp.path().join("owned");
    let before = fs::read(&record).unwrap();
    fs::remove_file(target.join("file.txt")).unwrap();
    let preview = report(&root, &[record.to_str().unwrap(), "--limit", "1"]);
    assert_eq!(preview["can_resume"], true, "{preview}");
    assert!(preview["counts"]["missing"].as_u64().unwrap() > 0);
    assert_eq!(preview["page"]["returned"], 1);
    assert_eq!(fs::read(&record).unwrap(), before);
    let applied = resume(&root, &record);
    assert_eq!(applied["applied"], true, "{applied}");
    assert!(!target.exists());
    assert!(record.with_file_name("complete").is_file());
    assert_eq!(git(&root, &["show", "owned:file.txt"]), b"base\n");
    let done = report(&root, &[record.to_str().unwrap()]);
    assert_eq!(done["can_resume"], false);
    assert_eq!(done["state"], "complete-marker-present");
}

#[test]
fn resumes_every_private_metadata_deletion_prefix_and_missing_roots() {
    for removed in 0..=10 {
        let temp = fixture();
        let root = temp.path().join("main");
        let record = prepared(&root);
        let data: Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        let metadata = Path::new(data["receipt"]["metadata"].as_str().unwrap());
        fs::remove_dir_all(temp.path().join("owned")).unwrap();
        let names = data["metadata_bytes"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>();
        for name in names.iter().take(removed) {
            fs::remove_file(metadata.join(name)).unwrap();
        }
        if removed > names.len() {
            fs::remove_dir_all(metadata).unwrap();
        }
        let result = resume(&root, &record);
        assert_eq!(result["applied"], true, "prefix {removed}: {result}");
        assert!(!metadata.exists());
        assert!(record.with_file_name("complete").is_file());
    }
}

#[test]
fn reports_blockers_and_preserves_changed_replaced_and_extra_content() {
    for fault in [
        "changed",
        "extra",
        "directory",
        "symlink",
        "replacement",
        "metadata",
        "lock",
        "archive-lock",
    ] {
        use std::os::unix::fs::symlink;
        let temp = fixture();
        let root = temp.path().join("main");
        let record = prepared(&root);
        let target = temp.path().join("owned");
        let data: Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        let metadata = Path::new(data["receipt"]["metadata"].as_str().unwrap());
        match fault {
            "changed" => fs::write(target.join("file.txt"), "changed").unwrap(),
            "extra" => fs::write(target.join("extra"), "preserve").unwrap(),
            "directory" => fs::create_dir(target.join("extra")).unwrap(),
            "symlink" => {
                fs::remove_file(target.join("file.txt")).unwrap();
                symlink(root.join("file.txt"), target.join("file.txt")).unwrap();
            }
            "replacement" => {
                fs::rename(target.join("file.txt"), temp.path().join("old")).unwrap();
                fs::write(target.join("file.txt"), "base\n").unwrap();
            }
            "metadata" => fs::write(metadata.join("private"), "preserve").unwrap(),
            "lock" => fs::write(metadata.join("index.lock"), "preserve").unwrap(),
            _ => fs::write(record.with_file_name("resume.lock"), "preserve").unwrap(),
        }
        let preview = report(&root, &[record.to_str().unwrap(), "--limit", "1"]);
        assert_eq!(preview["can_resume"], false, "{fault}: {preview}");
        assert!(preview["counts"]["blockers"].as_u64().unwrap() > 0);
        error(
            &root,
            &[
                record.to_str().unwrap(),
                "--write",
                "--basis",
                preview["basis"].as_str().unwrap(),
            ],
            "cannot resume",
        );
        assert!(target.join(".git").is_file());
    }
}

#[test]
fn stale_basis_and_changed_branch_refuse_without_deletions() {
    let temp = fixture();
    let root = temp.path().join("main");
    let record = prepared(&root);
    let preview = report(&root, &[record.to_str().unwrap()]);
    fs::remove_file(temp.path().join("owned/file.txt")).unwrap();
    error(
        &root,
        &[
            record.to_str().unwrap(),
            "--write",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        "stale removal resumption basis",
    );
    git(
        &root,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "later",
        ],
    );
    git(&root, &["update-ref", "refs/heads/owned", "HEAD"]);
    let preview = report(&root, &[record.to_str().unwrap()]);
    assert_eq!(preview["can_resume"], false);
    assert!(temp.path().join("owned/.git").is_file());
}

#[test]
fn rejects_malformed_foreign_and_unsafe_archives() {
    let temp = fixture();
    let root = temp.path().join("main");
    let record = prepared(&root);
    let original = fs::read(&record).unwrap();
    for fault in ["schema", "destination", "inventory", "metadata", "digest"] {
        let mut data: Value = serde_json::from_slice(&original).unwrap();
        match fault {
            "schema" => data["schema"] = 99.into(),
            "destination" => data["proposal"]["destination"] = root.to_str().unwrap().into(),
            "inventory" => data["proposal"]["files"][0]["path"] = "../main/file.txt".into(),
            "metadata" => {
                data["snapshot"]["metadata"]["../HEAD"] =
                    data["snapshot"]["metadata"]["HEAD"].clone();
            }
            _ => data["snapshot"]["metadata"]["HEAD"]["digest"] = "forged".into(),
        }
        fs::write(&record, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(
            !fr(&root, &[record.to_str().unwrap()])
                .output()
                .unwrap()
                .status
                .success(),
            "{fault}"
        );
        assert!(temp.path().join("owned/file.txt").is_file());
    }
    fs::write(&record, original).unwrap();
    let foreign = fixture();
    error(
        &foreign.path().join("main"),
        &[record.to_str().unwrap()],
        "inside its shared repository archive",
    );
}

#[test]
fn late_changes_are_preserved_and_a_fresh_review_can_resume() {
    use std::os::unix::fs::PermissionsExt;
    for fault in ["extra", "changed"] {
        let temp = fixture();
        let root = temp.path().join("main");
        let record = prepared(&root);
        let preview = report(&root, &[record.to_str().unwrap()]);
        let actual = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        let shim = temp.path().join("bin/git");
        fs::write(
            &shim,
            r##"#!/bin/sh
case " $* " in
  *" show-ref "*)
    count=0
    if [ -f "$FR_COUNT" ]; then count=$(cat "$FR_COUNT"); fi
    count=$((count + 1))
    echo "$count" > "$FR_COUNT"
    if [ "$count" = 4 ]; then
      if [ "$FR_FAULT" = extra ]; then echo preserve > "$FR_DEST/late-file";
      else echo preserve > "$FR_DEST/file.txt"; fi
    fi
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
                record.to_str().unwrap(),
                "--write",
                "--basis",
                preview["basis"].as_str().unwrap(),
            ],
        )
        .env(
            "PATH",
            format!(
                "{}:{}",
                shim.parent().unwrap().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env("FR_COUNT", temp.path().join("count"))
        .env("FR_DEST", temp.path().join("owned"))
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
        assert!(result["can_resume"].is_null(), "{result}");
        assert_eq!(result["state"], "unconfirmed");
        let preserved = temp.path().join("owned").join(if fault == "extra" {
            "late-file"
        } else {
            "file.txt"
        });
        assert_eq!(fs::read(&preserved).unwrap(), b"preserve\n");
        assert!(!record.with_file_name("resume.lock").exists());
        let blocked = report(&root, &[record.to_str().unwrap()]);
        assert_eq!(blocked["can_resume"], false);
        if fault == "extra" {
            fs::remove_file(preserved).unwrap();
        } else {
            fs::write(preserved, b"base\n").unwrap();
        }
        assert_eq!(resume(&root, &record)["applied"], true);
    }
}

#[test]
fn replaced_directories_are_not_traversed_and_completion_markers_are_respected() {
    use std::os::unix::fs::symlink;
    let temp = fixture();
    let root = temp.path().join("main");
    let record = prepared(&root);
    let target = temp.path().join("owned");
    let moved = temp.path().join("moved");
    fs::rename(&target, &moved).unwrap();
    symlink(&root, &target).unwrap();
    let preview = report(&root, &[record.to_str().unwrap(), "--limit", "500"]);
    assert_eq!(preview["can_resume"], false);
    assert!(preview["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["state"] == "uninspected"));
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"base\n");
    fs::remove_file(&target).unwrap();
    fs::rename(moved, &target).unwrap();
    fs::write(record.with_file_name("complete"), "invalid").unwrap();
    let invalid = report(&root, &[record.to_str().unwrap()]);
    assert_eq!(invalid["can_resume"], false);
    assert!(invalid["counts"]["blockers"].as_u64().unwrap() > 0);
    fs::remove_file(record.with_file_name("complete")).unwrap();
    assert_eq!(resume(&root, &record)["applied"], true);
}

#[test]
fn accepts_sha256_archives_from_another_linked_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    fs::write(root.join("file.txt"), b"base\n").unwrap();
    commit(&root);
    let record = prepared(&root);
    git(
        &root,
        &["worktree", "add", "-q", "-b", "caller", "../caller"],
    );
    let caller = temp.path().join("caller");
    let before = git(&caller, &["rev-parse", "HEAD"]);
    let result = resume(&caller, &record);
    assert_eq!(result["applied"], true, "{result}");
    assert_eq!(result["commit"].as_str().unwrap().len(), 64);
    assert_eq!(git(&caller, &["rev-parse", "HEAD"]), before);
    assert_eq!(fs::read(caller.join("file.txt")).unwrap(), b"base\n");
}
