use serde_json::Value;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output};

fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args);
    command
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let out = command(root, args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn error(out: Output, message: &str) {
    assert!(!out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["error"]["message"]
            .as_str()
            .unwrap()
            .contains(message),
        "{report}"
    );
}

fn put(root: &Path, name: &str, content: &str, mode: u32) {
    let path = root.join(name);
    fs::write(&path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
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

#[test]
fn delete_preview_save_apply_undo_redo_and_patches_preserve_unrelated_git_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let unusual = " old\t\n名.txt";
    put(root, unusual, "stored text\n", 0o640);
    put(root, "empty.txt", "", 0o600);
    put(root, "other.txt", "unrelated\n", 0o644);
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    put(root, "other.txt", "staged\n", 0o644);
    git(root, &["add", "other.txt"]);
    put(root, "other.txt", "unstaged\n", 0o644);
    let index = fs::read(root.join(".git/index")).unwrap();
    let preview = ok(root, &["file", "delete", "empty.txt", unusual]);
    assert_eq!(preview["changed"], 2);
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["transaction"], Value::Null);
    assert!(!preview.to_string().contains("stored text"));
    assert!(!root.join(".fr-history").exists());
    let saved = ok(
        root,
        &["file", "delete", "empty.txt", unusual, "--save-plan"],
    );
    assert_eq!(saved["basis"], preview["basis"]);
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["transaction"], 1);
    assert!(root.join("empty.txt").exists());
    let patch = ok(root, &["history", "patch", "1"]);
    assert!(patch["patch"]
        .as_str()
        .unwrap()
        .contains("deleted file mode"));
    assert_eq!(
        ok(root, &["history", "patch", "1", "--git-check"])["applicable"],
        true
    );
    assert_eq!(
        ok(root, &["history", "patch", "1", "--check"])["matches_recorded_snapshots"],
        true
    );
    ok(root, &["history", "apply", "1", "--write"]);
    assert!(!root.join(unusual).exists());
    assert!(!root.join("empty.txt").exists());
    assert_eq!(
        ok(root, &["history", "patch", "1", "--reverse", "--git-check"])["applicable"],
        true
    );
    put(root, "new.rs", "fn unrelated() {}\n", 0o644);
    ok(root, &["history", "undo", "1", "--write"]);
    assert_eq!(
        fs::read_to_string(root.join(unusual)).unwrap(),
        "stored text\n"
    );
    assert_eq!(mode(&root.join(unusual)), 0o640);
    assert_eq!(fs::read(root.join("empty.txt")).unwrap(), b"");
    assert_eq!(mode(&root.join("empty.txt")), 0o600);
    ok(root, &["history", "redo", "1", "--write"]);
    assert!(!root.join(unusual).exists());
    assert_eq!(
        fs::read_to_string(root.join("other.txt")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("new.rs")).unwrap(),
        "fn unrelated() {}\n"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn executable_changes_only_owner_bit_and_records_only_changed_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "script.sh", "#!/bin/sh\nexit 0\n", 0o640);
    put(root, "special.txt", "keep bytes\n", 0o1640);
    put(root, "already.txt", "already executable\n", 0o711);
    let report = ok(
        root,
        &[
            "file",
            "executable",
            "script.sh",
            "special.txt",
            "already.txt",
            "--set",
            "on",
            "--write",
        ],
    );
    assert_eq!(report["applied"], true);
    assert_eq!(report["changed"], 2);
    assert_eq!(report["requested"], 3);
    assert_eq!(report["mode_scope"], "owner-execute");
    assert_eq!(mode(&root.join("script.sh")), 0o740);
    assert_eq!(mode(&root.join("special.txt")), 0o1740);
    assert_eq!(mode(&root.join("already.txt")), 0o711);
    assert_eq!(
        fs::read_to_string(root.join("script.sh")).unwrap(),
        "#!/bin/sh\nexit 0\n"
    );
    let shown = ok(root, &["history", "show", "1"]);
    assert_eq!(shown["records"][0]["validation"], "file-snapshots");
    assert_eq!(shown["records"][0]["paths"].as_array().unwrap().len(), 2);
    let patch = ok(root, &["history", "patch", "1"]);
    assert!(patch["patch"]
        .as_str()
        .unwrap()
        .contains("old mode 100644\nnew mode 100755"));
    let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
    for intent in ["--write", "--save-plan"] {
        let noop = ok(
            root,
            &["file", "executable", "script.sh", "--set", "on", intent],
        );
        assert_eq!(noop["changed"], 0);
        assert_eq!(noop["transaction"], Value::Null);
        assert_eq!(noop["applied"], false);
        assert_eq!(noop["saved"], false);
    }
    assert_eq!(
        fs::read(root.join(".fr-history/state.json")).unwrap(),
        journal
    );
    ok(root, &["history", "undo", "1", "--write"]);
    assert_eq!(mode(&root.join("script.sh")), 0o640);
    assert_eq!(mode(&root.join("special.txt")), 0o1640);
    ok(root, &["history", "redo", "1", "--write"]);
    let off = ok(
        root,
        &["file", "executable", "script.sh", "--set", "off", "--write"],
    );
    assert_eq!(off["transaction"], 2);
    assert_eq!(mode(&root.join("script.sh")), 0o640);
    ok(root, &["history", "undo", "2", "--write"]);
    assert_eq!(mode(&root.join("script.sh")), 0o740);
}

#[test]
fn symlink_create_replace_delete_apply_undo_redo_and_patch_checks_are_exact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "entry", "regular before\n", 0o640);
    put(root, "unrelated.txt", "unchanged\n", 0o644);
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let index = fs::read(root.join(".git/index")).unwrap();

    let saved = ok(
        root,
        &[
            "file",
            "symlink",
            "entry",
            "--target",
            "missing-λ",
            "--save-plan",
        ],
    );
    assert_eq!(saved["operation"], "symlink");
    assert_eq!(saved["entries"][0]["before_kind"], "regular");
    assert_eq!(saved["entries"][0]["after_kind"], "symlink");
    assert_eq!(saved["entries"][0]["after_mode"], Value::Null);
    assert_eq!(saved["target"], "missing-λ");
    let exported = ok(root, &["history", "patch", "1"]);
    let patch = exported["patch"].as_str().unwrap();
    assert!(patch.contains("deleted file mode 100644"));
    assert!(patch.contains("new file mode 120000"));
    assert_eq!(exported["mode_scope"], "regular-executable-or-symlink");
    assert_eq!(
        ok(root, &["history", "patch", "1", "--check"])["matches_patch_basis"],
        true
    );
    assert_eq!(
        ok(root, &["history", "patch", "1", "--git-check"])["applicable"],
        true
    );

    ok(root, &["history", "apply", "1", "--write"]);
    assert_eq!(
        fs::read_link(root.join("entry")).unwrap(),
        Path::new("missing-λ")
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    let reverse = ok(root, &["history", "patch", "1", "--reverse", "--check"]);
    assert_eq!(reverse["files"][0]["actual_kind"], "symlink");
    assert_eq!(reverse["matches_recorded_snapshots"], true);
    assert_eq!(
        ok(root, &["history", "patch", "1", "--reverse", "--git-check"])["applicable"],
        true
    );
    ok(root, &["history", "undo", "1", "--write"]);
    assert_eq!(
        fs::read_to_string(root.join("entry")).unwrap(),
        "regular before\n"
    );
    assert_eq!(mode(&root.join("entry")), 0o640);
    ok(root, &["history", "redo", "1", "--write"]);

    let deleted = ok(root, &["file", "delete", "entry", "--write"]);
    assert_eq!(deleted["entries"][0]["before_kind"], "symlink");
    assert!(!root.join("entry").exists());
    assert!(fs::symlink_metadata(root.join("entry")).is_err());
    ok(root, &["history", "undo", "2", "--write"]);
    assert_eq!(
        fs::read_link(root.join("entry")).unwrap(),
        Path::new("missing-λ")
    );

    let created = ok(
        root,
        &[
            "file", "symlink", "new-link", "--target", "-literal", "--write",
        ],
    );
    assert_eq!(created["entries"][0]["before_exists"], false);
    assert_eq!(
        fs::read_link(root.join("new-link")).unwrap(),
        Path::new("-literal")
    );
    let noop = ok(
        root,
        &[
            "file", "symlink", "new-link", "--target", "-literal", "--write",
        ],
    );
    assert_eq!(noop["changed"], 0);
    assert_eq!(noop["transaction"], Value::Null);
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "unchanged\n"
    );
}

#[test]
fn unsafe_or_unsupported_batches_refuse_before_creating_history_or_changing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "good.txt", "keep\n", 0o644);
    put(root, "nul.txt", "a\0b", 0o644);
    fs::write(root.join("invalid.txt"), [0xff]).unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    symlink("good.txt", root.join("link")).unwrap();
    symlink(root, root.join("linked-directory")).unwrap();
    for bad in [
        "missing",
        "directory",
        "linked-directory/good.txt",
        "nul.txt",
        "invalid.txt",
        "../escape",
        ".git/config",
        ".fr-history/state.json",
        "good.txt",
    ] {
        for intent in ["--write", "--save-plan"] {
            let out = command(root, &["file", "delete", "good.txt", bad, intent])
                .output()
                .unwrap();
            error(out, "");
            assert_eq!(fs::read_to_string(root.join("good.txt")).unwrap(), "keep\n");
            assert!(!root.join(".fr-history").exists());
        }
    }
    error(
        command(
            root,
            &["file", "executable", "link", "--set", "on", "--write"],
        )
        .output()
        .unwrap(),
        "regular file",
    );
    for target in ["", &"x".repeat(1024)] {
        error(
            command(
                root,
                &["file", "symlink", "new-link", "--target", target, "--write"],
            )
            .output()
            .unwrap(),
            "between 1 and 1023",
        );
    }
    error(
        command(
            root,
            &[
                "file",
                "delete",
                root.join("good.txt").to_str().unwrap(),
                "--write",
            ],
        )
        .output()
        .unwrap(),
        "invalid history target",
    );
    error(
        command(
            root,
            &["file", "delete", "good.txt", "--write", "--save-plan"],
        )
        .output()
        .unwrap(),
        "choose --save-plan",
    );
    let many = vec!["good.txt"; 501];
    let mut args = vec!["file", "delete", "--write"];
    args.extend(many);
    error(command(root, &args).output().unwrap(), "between 1 and 500");
    let invalid_name = std::ffi::OsString::from_vec(b"bad\xff".to_vec());
    error(
        command(root, &["file", "delete", "--write"])
            .arg(invalid_name)
            .output()
            .unwrap(),
        "UTF-8",
    );
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn saved_file_operations_refuse_stale_content_mode_existence_and_project_sources() {
    for executable in [false, true] {
        for drift in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            put(root, "target.rs", "fn target() {}\n", 0o640);
            put(root, "other.rs", "fn other() {}\n", 0o640);
            let args = if executable {
                vec![
                    "file",
                    "executable",
                    "target.rs",
                    "--set",
                    "on",
                    "--save-plan",
                ]
            } else {
                vec!["file", "delete", "target.rs", "--save-plan"]
            };
            ok(root, &args);
            match drift {
                0 => fs::write(root.join("target.rs"), "fn changed() {}\n").unwrap(),
                1 => fs::set_permissions(root.join("target.rs"), fs::Permissions::from_mode(0o600))
                    .unwrap(),
                2 => fs::remove_file(root.join("target.rs")).unwrap(),
                _ => fs::write(root.join("other.rs"), "fn changed() {}\n").unwrap(),
            }
            let current = fs::read(root.join("target.rs")).ok();
            let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
            error(
                command(root, &["history", "apply", "1", "--write"])
                    .output()
                    .unwrap(),
                "",
            );
            assert_eq!(fs::read(root.join("target.rs")).ok(), current);
            assert_eq!(
                fs::read(root.join(".fr-history/state.json")).unwrap(),
                journal
            );
        }
    }
}

#[test]
fn plain_output_is_json_and_noop_writes_need_neither_git_nor_history() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "already.txt", "keep\n", 0o740);
    let out = Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("-C")
        .arg(root)
        .args([
            "file",
            "executable",
            "already.txt",
            "--set",
            "on",
            "--write",
        ])
        .env("PATH", "/no-git-for-file-operations")
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["transaction"], Value::Null);
    assert!(!root.join(".fr-history").exists());
    let applied = command(root, &["file", "delete", "already.txt", "--write"])
        .env("PATH", "/no-git-for-file-operations")
        .output()
        .unwrap();
    assert!(applied.status.success());
    assert!(!root.join("already.txt").exists());
}
