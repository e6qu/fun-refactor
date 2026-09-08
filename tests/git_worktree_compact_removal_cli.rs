#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fr@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn fr(root: &Path, operation: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fr"));
    cmd.args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "worktree", operation])
        .args(args);
    cmd
}

fn decode(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn report(root: &Path, operation: &str, args: &[&str]) -> Value {
    decode(fr(root, operation, args).output().unwrap())
}

fn reviewed(root: &Path, operation: &str, args: &[&str]) -> Value {
    let preview = report(root, operation, args);
    let mut args = args.to_vec();
    args.extend(["--basis", preview["basis"].as_str().unwrap(), "--write"]);
    let result = report(root, operation, &args);
    assert_eq!(result["applied"], true, "{result}");
    result
}

fn refused(root: &Path, operation: &str, args: &[&str], expected: &str) {
    let out = fr(root, operation, args).output().unwrap();
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains(expected),
        "{value}"
    );
}

fn fixture(sha256: bool) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    fs::create_dir(&root).unwrap();
    git(
        &root,
        &[
            "init",
            "-q",
            "-b",
            "main",
            if sha256 {
                "--object-format=sha256"
            } else {
                "--object-format=sha1"
            },
        ],
    );
    fs::write(root.join("file.txt"), b"base\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "fixture"]);
    reviewed(&root, "create", &["../owned", "--branch", "owned"]);
    let removed = reviewed(&root, "remove", &["../owned"]);
    let record = PathBuf::from(removed["removal_record"].as_str().unwrap());
    (temp, root, record)
}

#[test]
fn reviews_compacts_and_inspects_retained_audit_without_changing_git() {
    let (_temp, root, record) = fixture(false);
    let original = fs::read(&record).unwrap();
    let refs = git(&root, &["show-ref"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    let completion = fs::read(record.with_file_name("complete")).unwrap();
    fs::write(record.with_file_name("unrelated"), b"preserve").unwrap();
    let preview = report(&root, "compact-removal", &[record.to_str().unwrap()]);
    assert_eq!(preview["state"], "full");
    assert_eq!(preview["can_compact"], true);
    assert_eq!(preview["record_bytes"], original.len());
    assert_eq!(fs::read(&record).unwrap(), original);
    assert!(!record.with_file_name("summary.json").exists());
    let applied = reviewed(&root, "compact-removal", &[record.to_str().unwrap()]);
    assert_eq!(applied["state"], "compacted");
    assert!(!record.exists());
    let summary = fs::read(record.with_file_name("summary.json")).unwrap();
    assert!(summary.len() < original.len());
    let data: Value = serde_json::from_slice(&summary).unwrap();
    assert!(data["original_record"]["digest"].is_string());
    assert!(data["metadata_bytes"].is_number());
    assert!(data.get("snapshot").is_none());
    assert_eq!(
        fs::read(record.with_file_name("complete")).unwrap(),
        completion
    );
    assert_eq!(
        fs::read(record.with_file_name("unrelated")).unwrap(),
        b"preserve"
    );
    assert!(!record.with_file_name("resume.lock").exists());
    assert_eq!(git(&root, &["show-ref"]), refs);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    let done = report(&root, "compact-removal", &[record.to_str().unwrap()]);
    assert_eq!(done["state"], "compacted");
    assert_eq!(done["can_compact"], false);
    refused(
        &root,
        "compact-removal",
        &[
            record.to_str().unwrap(),
            "--basis",
            done["basis"].as_str().unwrap(),
            "--write",
        ],
        "cannot compact",
    );
    let audit = report(&root, "resume-removal", &[record.to_str().unwrap()]);
    assert_eq!(audit["state"], "compacted");
    assert_eq!(audit["can_resume"], false);
    refused(
        &root,
        "resume-removal",
        &[
            record.to_str().unwrap(),
            "--basis",
            audit["basis"].as_str().unwrap(),
            "--write",
        ],
        "cannot resume",
    );
}

#[test]
fn incomplete_removals_reappeared_paths_and_locks_block_compaction() {
    for fault in [
        "missing-marker",
        "invalid-marker",
        "checkout",
        "metadata",
        "dangling",
        "lock",
    ] {
        let (temp, root, record) = fixture(false);
        let original = fs::read(&record).unwrap();
        let data: Value = serde_json::from_slice(&original).unwrap();
        match fault {
            "missing-marker" => fs::remove_file(record.with_file_name("complete")).unwrap(),
            "invalid-marker" => fs::write(record.with_file_name("complete"), b"invalid").unwrap(),
            "checkout" => fs::create_dir(temp.path().join("owned")).unwrap(),
            "metadata" => fs::create_dir(data["receipt"]["metadata"].as_str().unwrap()).unwrap(),
            "dangling" => {
                std::os::unix::fs::symlink(temp.path().join("missing"), temp.path().join("owned"))
                    .unwrap()
            }
            _ => fs::write(record.with_file_name("resume.lock"), b"preserve").unwrap(),
        }
        let preview = report(&root, "compact-removal", &[record.to_str().unwrap()]);
        assert_eq!(preview["can_compact"], false, "{fault}: {preview}");
        refused(
            &root,
            "compact-removal",
            &[
                record.to_str().unwrap(),
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
            "cannot compact",
        );
        assert_eq!(fs::read(&record).unwrap(), original);
        assert!(!record.with_file_name("summary.json").exists());
        if fault == "lock" {
            assert_eq!(
                fs::read(record.with_file_name("resume.lock")).unwrap(),
                b"preserve"
            );
        }
    }
}

#[test]
fn changed_record_marker_and_path_state_invalidate_review_basis() {
    use std::os::unix::fs::PermissionsExt;
    for fault in ["bytes", "identity", "mode", "marker", "path"] {
        let (temp, root, record) = fixture(false);
        let preview = report(&root, "compact-removal", &[record.to_str().unwrap()]);
        let original = fs::read(&record).unwrap();
        match fault {
            "bytes" => {
                let mut bytes = original.clone();
                bytes.push(b'\n');
                fs::write(&record, bytes).unwrap();
            }
            "identity" => {
                fs::rename(&record, record.with_file_name("original")).unwrap();
                fs::write(&record, &original).unwrap();
            }
            "mode" => fs::set_permissions(&record, fs::Permissions::from_mode(0o640)).unwrap(),
            "marker" => {
                let marker = record.with_file_name("complete");
                let bytes = fs::read(&marker).unwrap();
                fs::rename(&marker, record.with_file_name("old-complete")).unwrap();
                fs::write(marker, bytes).unwrap();
            }
            _ => fs::create_dir(temp.path().join("owned")).unwrap(),
        }
        refused(
            &root,
            "compact-removal",
            &[
                record.to_str().unwrap(),
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
            "stale archive compaction basis",
        );
        assert!(record.is_file());
        assert!(!record.with_file_name("summary.json").exists());
    }
}

#[test]
fn refuses_foreign_malformed_and_symlinked_archives() {
    let (temp, root, record) = fixture(false);
    let original = fs::read(&record).unwrap();
    fs::write(record.with_file_name("summary.json"), b"{}").unwrap();
    refused(
        &root,
        "compact-removal",
        &[record.to_str().unwrap()],
        "missing field",
    );
    fs::remove_file(record.with_file_name("summary.json")).unwrap();
    let (_other, other_root, _) = fixture(false);
    refused(
        &other_root,
        "compact-removal",
        &[record.to_str().unwrap()],
        "inside its shared repository archive",
    );
    fs::rename(&record, temp.path().join("original")).unwrap();
    std::os::unix::fs::symlink(temp.path().join("original"), &record).unwrap();
    refused(
        &root,
        "compact-removal",
        &[record.to_str().unwrap()],
        "unsupported ownership metadata file",
    );
    assert_eq!(fs::read(temp.path().join("original")).unwrap(), original);
}

fn interrupt_after_summary(root: &Path, record: &Path, fault: &str) -> Value {
    use std::os::unix::fs::PermissionsExt;
    let preview = report(root, "compact-removal", &[record.to_str().unwrap()]);
    let bin = root.parent().unwrap().join("bin");
    fs::create_dir(&bin).unwrap();
    let shim = bin.join("git");
    fs::write(
        &shim,
        r##"#!/bin/sh
case " $* " in
  *" check-ref-format "*)
    if [ -f "$FR_ARCHIVE/summary.json" ] && [ ! -f "$FR_ARCHIVE/injected" ]; then
      echo injected > "$FR_ARCHIVE/injected"
      if [ "$FR_FAULT" = lock ]; then
        mv "$FR_ARCHIVE/resume.lock" "$FR_ARCHIVE/old-lock"
        echo preserve > "$FR_ARCHIVE/resume.lock"
      else
        mkdir "$FR_DEST"
      fi
    fi
    ;;
esac
exec "$FR_ACTUAL_GIT" "$@"
"##,
    )
    .unwrap();
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    decode(
        fr(
            root,
            "compact-removal",
            &[
                record.to_str().unwrap(),
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
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
        .env("FR_ARCHIVE", record.parent().unwrap())
        .env("FR_DEST", root.parent().unwrap().join("owned"))
        .env("FR_FAULT", fault)
        .output()
        .unwrap(),
    )
}

#[test]
fn durable_summary_allows_fresh_review_after_interrupted_compaction() {
    let (temp, root, record) = fixture(false);
    let original = fs::read(&record).unwrap();
    let result = interrupt_after_summary(&root, &record, "path");
    assert!(result["applied"].is_null(), "{result}");
    assert_eq!(result["state"], "unconfirmed");
    assert_eq!(fs::read(&record).unwrap(), original);
    let summary = fs::read(record.with_file_name("summary.json")).unwrap();
    let blocked = report(&root, "compact-removal", &[record.to_str().unwrap()]);
    assert_eq!(blocked["state"], "compaction-pending");
    assert_eq!(blocked["can_compact"], false);
    let audit = report(&root, "resume-removal", &[record.to_str().unwrap()]);
    assert_eq!(audit["can_resume"], false);
    fs::remove_dir(temp.path().join("owned")).unwrap();
    reviewed(&root, "compact-removal", &[record.to_str().unwrap()]);
    assert!(!record.exists());
    assert_eq!(
        fs::read(record.with_file_name("summary.json")).unwrap(),
        summary
    );
}

#[test]
fn replaced_archive_lock_is_preserved_and_pending_record_cannot_be_substituted() {
    let (_temp, root, record) = fixture(false);
    let original = fs::read(&record).unwrap();
    let result = interrupt_after_summary(&root, &record, "lock");
    assert!(result["applied"].is_null(), "{result}");
    assert_eq!(
        fs::read(record.with_file_name("resume.lock")).unwrap(),
        b"preserve\n"
    );
    assert_eq!(fs::read(&record).unwrap(), original);
    fs::remove_file(record.with_file_name("resume.lock")).unwrap();
    fs::remove_file(record.with_file_name("old-lock")).unwrap();
    let mut changed = original.clone();
    changed.push(b'\n');
    fs::write(&record, changed).unwrap();
    refused(
        &root,
        "compact-removal",
        &[record.to_str().unwrap()],
        "differs from its compaction summary",
    );
    fs::write(&record, original).unwrap();
    reviewed(&root, "compact-removal", &[record.to_str().unwrap()]);
}

#[test]
fn summary_validates_ownership_marker_and_paths_after_record_discard() {
    let (_temp, root, record) = fixture(false);
    reviewed(&root, "compact-removal", &[record.to_str().unwrap()]);
    let summary = record.with_file_name("summary.json");
    let original = fs::read(&summary).unwrap();
    for fault in [
        "schema",
        "identity",
        "destination",
        "metadata",
        "commit",
        "completion",
        "unknown",
    ] {
        let mut data: Value = serde_json::from_slice(&original).unwrap();
        match fault {
            "schema" => data["schema"] = 99.into(),
            "identity" => data["archive_identity"][1] = 0.into(),
            "destination" => {
                data["destination"] = root.canonicalize().unwrap().to_str().unwrap().into()
            }
            "metadata" => data["metadata"] = "/tmp/foreign".into(),
            "commit" => data["commit"] = "bad".into(),
            "completion" => data["completion"]["digest"] = "bad".into(),
            _ => data["extra"] = true.into(),
        }
        fs::write(&summary, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(
            !fr(&root, "compact-removal", &[record.to_str().unwrap()])
                .output()
                .unwrap()
                .status
                .success(),
            "{fault}"
        );
    }
    fs::write(&summary, original).unwrap();
    fs::remove_file(record.with_file_name("complete")).unwrap();
    refused(
        &root,
        "compact-removal",
        &[record.to_str().unwrap()],
        "completion marker changed",
    );
}

#[test]
fn accepts_historical_branches_packed_sha256_and_linked_callers() {
    let (temp, root, record) = fixture(true);
    git(&root, &["commit", "--allow-empty", "-qm", "later"]);
    git(&root, &["branch", "-D", "owned"]);
    git(&root, &["pack-refs", "--all"]);
    git(
        &root,
        &["worktree", "add", "-q", "-b", "caller", "../caller"],
    );
    let caller = temp.path().join("caller");
    let refs = git(&root, &["show-ref"]);
    let applied = reviewed(&caller, "compact-removal", &[record.to_str().unwrap()]);
    assert_eq!(applied["commit"].as_str().unwrap().len(), 64);
    assert_eq!(git(&root, &["show-ref"]), refs);
    let commit = applied["commit"].as_str().unwrap();
    fs::remove_file(
        root.join(".git/objects")
            .join(&commit[..2])
            .join(&commit[2..]),
    )
    .unwrap();
    let inspect = report(&caller, "resume-removal", &[record.to_str().unwrap()]);
    assert_eq!(inspect["state"], "compacted");
    assert_eq!(fs::read(caller.join("file.txt")).unwrap(), b"base\n");
}
