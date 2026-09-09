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

fn history(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "stage-history"])
        .args(args);
    command
}
fn read_history(root: &Path, args: &[&str]) -> Value {
    let output = history(root, args).output().unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn history_error(root: &Path, args: &[&str], message: &str) {
    let output = history(root, args).output().unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains(message),
        "{value}"
    );
}
fn stage(root: &Path, paths: &[&str]) -> u64 {
    let preview = report(root, paths);
    let mut args = paths.to_vec();
    args.extend(["--basis", preview["basis"].as_str().unwrap(), "--write"]);
    report(root, &args)["durability"]["journal"]["id"]
        .as_u64()
        .unwrap()
}
fn replay(root: &Path, args: &[&str]) -> Value {
    let preview = read_history(root, args);
    assert_eq!(preview["applied"], false);
    let mut args = args.to_vec();
    args.extend(["--basis", preview["basis"].as_str().unwrap(), "--write"]);
    let output = read_history(root, &args);
    assert_eq!(output["applied"], true);
    assert_eq!(output["durability"]["journal"]["finalized"], true);
    output
}
fn fixture(root: &Path) {
    init(root);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    fs::write(root.join("other.txt"), "other\n").unwrap();
    commit(root);
    fs::write(root.join("file.txt"), "reviewed\r\n").unwrap();
}

#[test]
fn journal_undo_redo_restore_index_without_touching_working_files_or_unrelated_staging() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let head = git(root, &["rev-parse", "HEAD"]);
    fs::write(root.join("file.txt"), "previous staging\n").unwrap();
    git(root, &["add", "file.txt"]);
    fs::write(root.join("file.txt"), "reviewed\r\n").unwrap();
    fs::set_permissions(root.join("file.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(stage(root, &["file.txt"]), 1);
    let list = read_history(root, &["list"]);
    assert_eq!(list["undo"], 1);
    assert_eq!(list["records"][0]["paths"], 1);
    assert!(!list.to_string().contains("reviewed"));
    let show = read_history(root, &["show", "1"]);
    assert!(!show.to_string().contains("previous staging"));
    assert_eq!(show["record"]["entries"][0]["after"]["mode"], "100755");
    let index = fs::read(root.join(".git/index")).unwrap();
    let state = fs::read(root.join(".git/fr-stage/state.json")).unwrap();
    let undo = read_history(root, &["undo", "1"]);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(root.join(".git/fr-stage/state.json")).unwrap(),
        state
    );
    fs::write(root.join("file.txt"), b"later binary\0work").unwrap();
    fs::write(root.join("other.txt"), "unrelated staged later\n").unwrap();
    git(root, &["add", "other.txt"]);
    read_history(
        root,
        &[
            "undo",
            "1",
            "--basis",
            undo["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(git(root, &["show", ":file.txt"]), b"previous staging\n");
    assert!(git(root, &["ls-files", "--stage", "file.txt"]).starts_with(b"100644 "));
    replay(root, &["redo", "1"]);
    assert_eq!(git(root, &["show", ":file.txt"]), b"reviewed\r\n");
    assert_eq!(
        git(root, &["show", ":other.txt"]),
        b"unrelated staged later\n"
    );
    assert_eq!(
        fs::read(root.join("file.txt")).unwrap(),
        b"later binary\0work"
    );
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn journal_restores_deleted_binary_blobs_after_object_pruning_and_unborn_additions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    fs::write(root.join("binary.dat"), b"prior\0\xffblob").unwrap();
    git(root, &["add", "binary.dat"]);
    let oid = String::from_utf8(git(root, &["rev-parse", ":binary.dat"]))
        .unwrap()
        .trim()
        .to_owned();
    fs::remove_file(root.join("binary.dat")).unwrap();
    stage(root, &["binary.dat"]);
    git(root, &["prune", "--expire=now"]);
    assert!(!git_output(root, &["cat-file", "-e", &oid]).status.success());
    replay(root, &["undo", "1"]);
    assert_eq!(git(root, &["show", ":binary.dat"]), b"prior\0\xffblob");
    assert!(!root.join("binary.dat").exists());
    replay(root, &["redo", "1"]);
    fs::write(root.join("new.txt"), "added\n").unwrap();
    stage(root, &["new.txt"]);
    replay(root, &["undo", "2"]);
    assert!(git(root, &["ls-files", "new.txt"]).is_empty());
    replay(root, &["redo", "2"]);
    assert_eq!(git(root, &["show", ":new.txt"]), b"added\n");
}

#[test]
fn journal_enforces_stack_order_stale_tokens_and_redo_invalidation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stage(root, &["file.txt"]);
    let old = read_history(root, &["undo", "1"]);
    fs::write(root.join("file.txt"), "second\n").unwrap();
    stage(root, &["file.txt"]);
    history_error(root, &["undo", "1"], "top transaction");
    replay(root, &["undo", "2"]);
    history_error(
        root,
        &[
            "undo",
            "1",
            "--basis",
            old["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale staging history basis",
    );
    replay(root, &["undo", "1"]);
    history_error(root, &["redo", "2"], "top transaction");
    let before = read_history(root, &["list"]);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    let preview = report(root, &["file.txt"]);
    let result = report(
        root,
        &[
            "file.txt",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(result["durability"]["index_replaced"], false);
    assert_eq!(read_history(root, &["list"]), before);
    fs::write(root.join("other.txt"), "new branch\n").unwrap();
    assert_eq!(stage(root, &["other.txt"]), 3);
    let list = read_history(root, &["list", "--limit", "2"]);
    assert_eq!(list["total"], 3);
    assert_eq!(list["records"].as_array().unwrap().len(), 2);
    assert_eq!(list["records"][1]["status"], "abandoned");
    history_error(root, &["redo", "1"], "top transaction");
    assert!(!history(root, &["undo", "3", "--write"])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn compaction_retires_old_replay_payloads_without_changing_files_or_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    assert_eq!(stage(root, &["file.txt"]), 1);
    fs::write(root.join("other.txt"), "second\n").unwrap();
    assert_eq!(stage(root, &["other.txt"]), 2);
    fs::write(root.join("file.txt"), "third\n").unwrap();
    assert_eq!(stage(root, &["file.txt"]), 3);

    let index = fs::read(root.join(".git/index")).unwrap();
    let file = fs::read(root.join("file.txt")).unwrap();
    let other = fs::read(root.join("other.txt")).unwrap();
    let journal_path = root.join(".git/fr-stage/state.json");
    let journal = fs::read(&journal_path).unwrap();

    let preview = read_history(root, &["compact", "--keep", "1"]);
    assert_eq!(preview["operation"], "stage-history-compact-preview");
    assert_eq!(preview["records_compacted"], 2);
    assert_eq!(preview["paths_compacted"], 2);
    assert_eq!(preview["undo_after"], 3);
    assert!(
        preview["journal_bytes_after"].as_u64().unwrap()
            < preview["journal_bytes_before"].as_u64().unwrap()
    );
    assert_eq!(fs::read(&journal_path).unwrap(), journal);

    let applied = read_history(
        root,
        &[
            "compact",
            "--keep",
            "1",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(applied["applied"], true);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), file);
    assert_eq!(fs::read(root.join("other.txt")).unwrap(), other);

    let list = read_history(root, &["list"]);
    assert_eq!(list["total"], 3);
    assert_eq!(list["records"][0]["id"], 3);
    assert_eq!(list["records"][0]["compacted"], false);
    assert_eq!(list["records"][1]["id"], 2);
    assert_eq!(list["records"][1]["compacted"], true);
    assert_eq!(list["records"][2]["id"], 1);
    assert_eq!(list["records"][2]["paths"], 1);
    let show = read_history(root, &["show", "1"]);
    assert_eq!(show["record"]["compacted"], true);
    assert!(show["record"]["entries"].is_null());

    replay(root, &["undo", "3"]);
    history_error(root, &["undo", "2"], "top transaction");
    let preview = read_history(root, &["compact", "--keep", "0"]);
    assert_eq!(preview["records_compacted"], 1);
    let result = read_history(
        root,
        &[
            "compact",
            "--keep",
            "0",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert!(result["undo_after"].is_null());
    assert!(result["redo_after"].is_null());
    history_error(root, &["redo", "3"], "top transaction");

    fs::write(root.join("other.txt"), "fourth\n").unwrap();
    assert_eq!(stage(root, &["other.txt"]), 4);
    assert_eq!(
        read_history(root, &["show", "4"])["record"]["compacted"],
        false
    );
}

#[test]
fn compaction_discards_abandoned_payloads_even_when_both_stacks_are_retained() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stage(root, &["file.txt"]);
    replay(root, &["undo", "1"]);
    fs::write(root.join("other.txt"), "new branch\n").unwrap();
    assert_eq!(stage(root, &["other.txt"]), 2);

    let preview = read_history(root, &["compact"]);
    assert_eq!(preview["keep_per_stack"], 100);
    assert_eq!(preview["records_compacted"], 1);
    let applied = read_history(
        root,
        &[
            "compact",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(applied["applied"], true);
    assert_eq!(
        read_history(root, &["show", "1"])["record"]["compacted"],
        true
    );
    assert_eq!(
        read_history(root, &["show", "2"])["record"]["compacted"],
        false
    );
    replay(root, &["undo", "2"]);
}

#[test]
fn compaction_reports_an_unconfirmed_storage_failure_without_touching_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stage(root, &["file.txt"]);
    fs::write(root.join("other.txt"), "second\n").unwrap();
    stage(root, &["other.txt"]);
    let preview = read_history(root, &["compact", "--keep", "1"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let journal_dir = root.join(".git/fr-stage");
    let journal = fs::read(journal_dir.join("state.json")).unwrap();
    fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o500)).unwrap();
    let output = history(
        root,
        &[
            "compact",
            "--keep",
            "1",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    )
    .output()
    .unwrap();
    fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["applied"].is_null());
    assert!(result["warning"].as_str().unwrap().contains("unconfirmed"));
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(journal_dir.join("state.json")).unwrap(), journal);
}

#[test]
fn compaction_refuses_stale_empty_pending_and_corrupt_requests() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stage(root, &["file.txt"]);
    fs::write(root.join("other.txt"), "second\n").unwrap();
    stage(root, &["other.txt"]);
    let stale = read_history(root, &["compact", "--keep", "1"]);
    fs::write(root.join("file.txt"), "third\n").unwrap();
    stage(root, &["file.txt"]);
    history_error(
        root,
        &[
            "compact",
            "--keep",
            "1",
            "--basis",
            stale["basis"].as_str().unwrap(),
            "--write",
        ],
        "stale staging compaction basis",
    );
    history_error(root, &["compact", "--keep", "10001"], "0 through 10000");
    let no_work = read_history(root, &["compact", "--keep", "100"]);
    assert_eq!(no_work["can_compact"], false);
    history_error(
        root,
        &[
            "compact",
            "--keep",
            "100",
            "--basis",
            no_work["basis"].as_str().unwrap(),
            "--write",
        ],
        "no replay payloads",
    );

    replay(root, &["undo", "3"]);
    replay(root, &["undo", "2"]);
    set_pending(root, "undo", false);
    history_error(root, &["compact", "--keep", "0"], "needs recovery");
    replay(root, &["recover"]);
    let preview = read_history(root, &["compact", "--keep", "0"]);
    read_history(
        root,
        &[
            "compact",
            "--keep",
            "0",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    let path = root.join(".git/fr-stage/state.json");
    let mut corrupt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    corrupt["records"][0]["compacted_paths"] = json!(0);
    fs::write(path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    history_error(root, &["list"], "compaction summary");
}

#[test]
fn journal_refuses_selected_external_edits_and_special_flags() {
    for flag in ["--assume-unchanged", "--skip-worktree", "content"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        stage(root, &["file.txt"]);
        if flag == "content" {
            fs::write(root.join("file.txt"), "external\n").unwrap();
            git(root, &["add", "file.txt"]);
        } else {
            git(root, &["update-index", flag, "file.txt"]);
        }
        let index = fs::read(root.join(".git/index")).unwrap();
        history_error(
            root,
            &["undo", "1"],
            if flag == "content" {
                "recorded staging basis"
            } else {
                "flags"
            },
        );
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("intent.txt"), "intent\n").unwrap();
    git(root, &["add", "-N", "intent.txt"]);
    let preview = report(root, &["intent.txt"]);
    error(
        root,
        &[
            "intent.txt",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "intent-to-add",
    );
    assert!(!root.join(".git/fr-stage").exists());
}

fn set_pending(root: &Path, action: &str, installed: bool) {
    let path = root.join(".git/fr-stage/state.json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["pending"] = json!({"id":1,"action":action});
    let target = if action == "apply" {
        state["records"][0]["status"] = json!("planned");
        state["applied"] = json!([]);
        if installed {
            "after"
        } else {
            "before"
        }
    } else if action == "undo" {
        if installed {
            "before"
        } else {
            "after"
        }
    } else {
        state["records"][0]["status"] = json!("undone");
        state["applied"] = json!([]);
        state["redo"] = json!([1]);
        if installed {
            "after"
        } else {
            "before"
        }
    };
    for change in state["records"][0]["changes"].as_array().unwrap() {
        let blob = &change[target]["blob"];
        git(
            root,
            &[
                "update-index",
                "--cacheinfo",
                &format!(
                    "{},{},{}",
                    blob["mode"].as_str().unwrap(),
                    blob["oid"].as_str().unwrap(),
                    change["path"].as_str().unwrap()
                ),
            ],
        );
    }
    fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
}

#[test]
fn recovery_rolls_back_each_pending_transition_before_or_after_installation() {
    for action in ["apply", "undo", "redo"] {
        for installed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            fixture(root);
            stage(root, &["file.txt"]);
            set_pending(root, action, installed);
            fs::write(root.join("other.txt"), "external staging\n").unwrap();
            git(root, &["add", "other.txt"]);
            history_error(root, &["undo", "1"], "needs recovery");
            let preview = report(root, &["other.txt"]);
            error(
                root,
                &[
                    "other.txt",
                    "--basis",
                    preview["basis"].as_str().unwrap(),
                    "--write",
                ],
                "needs recovery",
            );
            replay(root, &["recover"]);
            let list = read_history(root, &["list"]);
            assert!(list["pending"].is_null());
            assert_eq!(
                list["records"][0]["status"],
                match action {
                    "apply" => "abandoned",
                    "undo" => "applied",
                    _ => "undone",
                }
            );
            assert_eq!(
                git(root, &["show", ":file.txt"]),
                if action == "undo" {
                    &b"reviewed\r\n"[..]
                } else {
                    &b"base\n"[..]
                }
            );
            assert_eq!(git(root, &["show", ":other.txt"]), b"external staging\n");
            history_error(root, &["recover"], "no pending");
        }
    }
}

#[test]
fn recovery_refuses_mixed_selected_states_and_foreign_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("other.txt"), "also selected\n").unwrap();
    stage(root, &["file.txt", "other.txt"]);
    set_pending(root, "apply", true);
    git(root, &["reset", "-q", "HEAD", "--", "other.txt"]);
    history_error(root, &["recover"], "recorded staging basis");
    fs::write(root.join("file.txt"), "third state\n").unwrap();
    git(root, &["add", "file.txt"]);
    history_error(root, &["recover"], "recorded staging basis");
}

#[test]
fn journal_is_worktree_local_and_refuses_corruption_or_symlink_storage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    stage(root, &["file.txt"]);
    let work_dir = tempfile::tempdir().unwrap();
    let work = work_dir.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", work.to_str().unwrap()],
    );
    assert_eq!(read_history(&work, &["list"])["total"], 0);
    assert!(!work.join(".fr-history").exists());
    fs::write(work.join("file.txt"), "linked\n").unwrap();
    stage(&work, &["file.txt"]);
    replay(&work, &["undo", "1"]);
    assert_eq!(git(root, &["show", ":file.txt"]), b"reviewed\r\n");
    let state = fs::read(root.join(".git/fr-stage/state.json")).unwrap();
    let mut corrupt: Value = serde_json::from_slice(&state).unwrap();
    corrupt["records"][0]["changes"][0]["path"] = json!("../escape");
    fs::write(
        root.join(".git/fr-stage/state.json"),
        serde_json::to_vec(&corrupt).unwrap(),
    )
    .unwrap();
    history_error(root, &["list"], "record identity or digest");
    fs::write(root.join(".git/fr-stage/state.json"), state).unwrap();
    fs::rename(root.join(".git/fr-stage"), root.join(".git/saved-stage")).unwrap();
    symlink("saved-stage", root.join(".git/fr-stage")).unwrap();
    history_error(root, &["list"], "real directory");
}

#[test]
fn journal_reports_post_install_failure_and_recovers_the_real_pending_write() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let preview = report(root, &["file.txt"]);
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(actual.status.success());
    let shim = root.join("shim");
    fs::create_dir(&shim).unwrap();
    fs::write(
        shim.join("git"),
        r#"#!/bin/sh
case " $* " in
*' --git-path index '*)
  if [ -f .git/fr-stage/state.json ] && [ ! -e .git/saved-stage ]; then
    mv .git/fr-stage .git/saved-stage
    ln -s saved-stage .git/fr-stage
  fi
  ;;
esac
exec "$FR_ACTUAL_GIT" "$@"
"#,
    )
    .unwrap();
    fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    let output = fr(
        root,
        &[
            "file.txt",
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["durability"]["journal"]["finalized"], false);
    assert_eq!(git(root, &["show", ":file.txt"]), b"reviewed\r\n");
    fs::remove_file(root.join(".git/fr-stage")).unwrap();
    fs::rename(root.join(".git/saved-stage"), root.join(".git/fr-stage")).unwrap();
    let state = read_history(root, &["list"]);
    assert_eq!(state["pending"], json!({"id":1,"action":"apply"}));
    replay(root, &["recover"]);
    assert_eq!(git(root, &["show", ":file.txt"]), b"base\n");
}

#[test]
fn journal_failures_before_installation_leave_the_index_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let preview = report(root, &["file.txt"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    fs::create_dir(root.join(".git/fr-stage")).unwrap();
    fs::create_dir(root.join(".git/fr-stage/state.json")).unwrap();
    error(
        root,
        &[
            "file.txt",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
        "regular file",
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(!root.join(".git/index.lock").exists());
}
