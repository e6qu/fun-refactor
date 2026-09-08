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
        .args(["git", "commit"])
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

use std::os::unix::fs::{symlink, PermissionsExt};

fn fixture(root: &Path) {
    init(root);
    fs::write(root.join("file.txt"), "base\n").unwrap();
    fs::write(root.join("other.txt"), "other\n").unwrap();
    commit(root);
    fs::write(root.join("file.txt"), "staged\r\n").unwrap();
    git(root, &["add", "file.txt"]);
    fs::write(root.join("file.txt"), "working\n").unwrap();
}

fn apply(root: &Path, message: &str) -> Value {
    let preview = report(root, &["-m", message]);
    let result = report(
        root,
        &[
            "-m",
            message,
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    assert_eq!(result["applied"], true);
    assert_eq!(result["tree"], preview["tree"]);
    assert_eq!(result["reference_update"]["acknowledged"], true);
    result
}

#[test]
fn commit_preview_is_bounded_and_writes_no_repository_objects_index_or_refs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("other.txt"), "another staged\n").unwrap();
    git(root, &["add", "other.txt"]);
    let objects = git(root, &["count-objects", "-v"]);
    let refs = git(root, &["show-ref"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let result = report(root, &["-m", "Reviewed change", "--limit", "1"]);
    assert_eq!(result["operation"], "commit-preview");
    assert_eq!(result["applied"], false);
    assert_eq!(result["scope"], "entire-index");
    assert_eq!(result["page"], json!({"total":2,"returned":1,"omitted":1}));
    assert_eq!(result["message"], "Reviewed change\n");
    assert_eq!(result["author"], "fr fixture <fr@example.invalid>");
    assert!(!result.to_string().contains("another staged"));
    assert_eq!(
        result["basis"],
        report(root, &["-m", "Reviewed change\n", "--limit", "500"])["basis"]
    );
    assert_eq!(git(root, &["count-objects", "-v"]), objects);
    assert_eq!(git(root, &["show-ref"]), refs);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(
        !git_output(root, &["cat-file", "-e", result["tree"].as_str().unwrap()])
            .status
            .success()
    );
    assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn commit_publishes_the_entire_staged_tree_and_preserves_working_files_and_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    fs::write(root.join("other.txt"), "also staged\n").unwrap();
    git(root, &["add", "other.txt"]);
    fs::write(root.join("untracked.txt"), "keep\n").unwrap();
    let parent = String::from_utf8(git(root, &["rev-parse", "HEAD"])).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let result = apply(root, "subject\n\nDetailed body with λ.");
    let oid = result["commit"].as_str().unwrap();
    assert_eq!(
        git(root, &["rev-parse", "HEAD"]),
        format!("{oid}\n").as_bytes()
    );
    assert_eq!(git(root, &["rev-parse", "HEAD^"]), parent.as_bytes());
    assert_eq!(git(root, &["show", "HEAD:file.txt"]), b"staged\r\n");
    assert_eq!(git(root, &["show", "HEAD:other.txt"]), b"also staged\n");
    assert_eq!(fs::read(root.join("file.txt")).unwrap(), b"working\n");
    assert_eq!(fs::read(root.join("untracked.txt")).unwrap(), b"keep\n");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(git(root, &["diff", "--cached", "--name-only"]).is_empty());
    let reflog = git(root, &["reflog", "-1", "--format=%gs"]);
    assert_eq!(reflog, b"fr: reviewed commit\n");
    assert!(!root.join(".fr-history").exists());
    assert!(!root.join(".git/fr-stage").exists());
}

#[test]
fn commit_supports_initial_sha256_binary_symlink_and_all_file_deletion_trees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    git(root, &["config", "user.name", "fr fixture"]);
    git(root, &["config", "user.email", "fr@example.invalid"]);
    let name = "file 名[*]\n.dat";
    fs::write(root.join(name), b"raw\0\xffbinary").unwrap();
    symlink(name, root.join("link")).unwrap();
    git(root, &["add", "."]);
    let first = apply(root, "initial");
    assert!(first["head"]["parent"].is_null());
    assert_eq!(first["commit"].as_str().unwrap().len(), 64);
    assert_eq!(
        git(root, &["show", &format!("HEAD:{name}")]),
        b"raw\0\xffbinary"
    );
    assert!(git(root, &["ls-tree", "HEAD", "link"]).starts_with(b"120000 blob "));
    git(root, &["rm", "-q", "--", name, "link"]);
    apply(root, "delete all");
    assert!(git(root, &["ls-tree", "HEAD"]).is_empty());
}

#[test]
fn commit_basis_ignores_working_edits_but_rejects_index_head_message_and_identity_drift() {
    for drift in ["index", "head", "branch", "message", "identity"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        let preview = report(root, &["-m", "reviewed"]);
        fs::write(
            root.join("file.txt"),
            "working edits leave the basis valid\n",
        )
        .unwrap();
        assert_eq!(report(root, &["-m", "reviewed"])["basis"], preview["basis"]);
        let mut message = "reviewed";
        match drift {
            "index" => {
                git(root, &["add", "file.txt"]);
            }
            "head" => {
                git(
                    root,
                    &["commit", "--allow-empty", "--only", "-qm", "external"],
                );
            }
            "branch" => {
                git(root, &["branch", "other"]);
                git(root, &["symbolic-ref", "HEAD", "refs/heads/other"]);
            }
            "message" => {
                message = "different";
            }
            _ => {
                git(root, &["config", "user.name", "changed identity"]);
            }
        }
        let head = git(root, &["rev-parse", "HEAD"]);
        error(
            root,
            &[
                "-m",
                message,
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
            "stale commit basis",
        );
        assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
        assert!(!root.join(".git/index.lock").exists());
    }
}

#[test]
fn commit_refuses_unsafe_states_missing_objects_and_unreviewed_writes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    assert!(!fr(root, &["-m", "msg", "--write"])
        .output()
        .unwrap()
        .status
        .success());
    error(root, &["-m", " "], "nonempty UTF-8");
    error(root, &["-m", "msg", "--limit", "0"], "limit must");
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        fs::write(root.join(".git").join(marker), "active").unwrap();
        error(root, &["-m", "msg"], "active Git operation");
        fs::remove_file(root.join(".git").join(marker)).unwrap();
    }
    git(root, &["update-index", "--assume-unchanged", "other.txt"]);
    error(root, &["-m", "msg"], "flags");
    git(
        root,
        &["update-index", "--no-assume-unchanged", "other.txt"],
    );
    let parent = String::from_utf8(git(root, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_owned();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{parent},submodule"),
        ],
    );
    error(root, &["-m", "msg"], "submodules");
    git(root, &["update-index", "--force-remove", "submodule"]);
    git(
        root,
        &[
            "update-index",
            "--cacheinfo",
            &format!("100644,{},file.txt", "1".repeat(40)),
        ],
    );
    error(root, &["-m", "msg"], "missing or non-blob");
    git(root, &["add", "file.txt"]);
    git(root, &["checkout", "--detach", "-q"]);
    error(root, &["-m", "msg"], "attached local branch");
}

#[test]
fn commit_refuses_pending_staging_and_keeps_completed_staging_history_intact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let mut stage = Command::new(env!("CARGO_BIN_EXE_fr"));
    stage
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "stage", "file.txt"]);
    let preview: Value = serde_json::from_slice(&stage.output().unwrap().stdout).unwrap();
    let output = stage
        .args(["--basis", preview["basis"].as_str().unwrap(), "--write"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = root.join(".git/fr-stage/state.json");
    let original = fs::read(&path).unwrap();
    let mut state: Value = serde_json::from_slice(&original).unwrap();
    state["records"][0]["status"] = json!("planned");
    state["applied"] = json!([]);
    state["pending"] = json!({"id":1,"action":"apply"});
    fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    error(root, &["-m", "msg"], "needs recovery");
    fs::write(&path, &original).unwrap();
    apply(root, "from staged history");
    assert_eq!(fs::read(&path).unwrap(), original);
    error(root, &["-m", "empty"], "nothing staged");
}

#[test]
fn commit_uses_linked_worktree_head_and_ignores_inherited_redirects_hooks_and_signing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    let linked_dir = tempfile::tempdir().unwrap();
    let work = linked_dir.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", work.to_str().unwrap()],
    );
    fs::write(work.join("file.txt"), "linked staged\n").unwrap();
    git(&work, &["add", "file.txt"]);
    fs::create_dir(work.join("nested")).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    for hook in [
        "pre-commit",
        "prepare-commit-msg",
        "commit-msg",
        "post-commit",
        "reference-transaction",
    ] {
        let path = root.join(".git/hooks").join(hook);
        fs::write(&path, "#!/bin/sh\nprintf 'hook ran' > hook-ran\nexit 1\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(root, &["config", "commit.gpgSign", "true"]);
    git(root, &["config", "gpg.program", "/missing-signer"]);
    let preview = report(&work, &["-m", "linked commit"]);
    let output = fr(
        &work.join("nested"),
        &[
            "-m",
            "linked commit",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    )
    .env("GIT_DIR", root.join(".git"))
    .env("GIT_INDEX_FILE", root.join(".git/index"))
    .env("GIT_OBJECT_DIRECTORY", root.join("must-not-create"))
    .env("GIT_AUTHOR_NAME", "unreviewed author")
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["head"]["branch"], "refs/heads/linked");
    assert_eq!(git(&work, &["show", "HEAD:file.txt"]), b"linked staged\n");
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(!work.join("hook-ran").exists());
    assert!(!root.join("must-not-create").exists());
}

fn shim(root: &Path, script: &str, command: &mut Command) {
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(actual.status.success());
    let dir = root.join("shim");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("git"), script).unwrap();
    fs::set_permissions(dir.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    command
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env(
            "PATH",
            format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
        );
}

#[test]
fn commit_aborts_if_head_switches_to_another_branch_with_the_same_parent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    git(root, &["branch", "other"]);
    let parent = git(root, &["rev-parse", "HEAD"]);
    let preview = report(root, &["-m", "reviewed"]);
    let mut command = fr(
        root,
        &[
            "-m",
            "reviewed",
            "--basis",
            preview["basis"].as_str().unwrap(),
            "--write",
        ],
    );
    shim(
        root,
        r#"#!/bin/sh
case " $* " in
*' update-ref '*) "$FR_ACTUAL_GIT" symbolic-ref HEAD refs/heads/other || exit $? ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#,
        &mut command,
    );
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("HEAD changed"),
        "{result}"
    );
    assert_eq!(git(root, &["rev-parse", "refs/heads/main"]), parent);
    assert_eq!(git(root, &["rev-parse", "refs/heads/other"]), parent);
    assert!(!root.join(".git/HEAD.lock").exists());
    assert!(!root.join(".git/refs/heads/other.lock").exists());
    assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn commit_preparation_locks_head_and_reports_publication_despite_a_late_git_failure() {
    for late_failure in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        git(root, &["branch", "other"]);
        let preview = report(root, &["-m", "reviewed"]);
        let mut command = fr(
            root,
            &[
                "-m",
                "reviewed",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
        );
        let script = if late_failure {
            r#"#!/bin/sh
case " $* " in
*' update-ref '*) "$FR_ACTUAL_GIT" "$@" || exit $?; exit 42 ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#
        } else {
            r#"#!/bin/sh
case " $* " in
*' symbolic-ref --quiet --no-recurse HEAD '*)

  if [ -f .git/HEAD.lock ]; then
    if "$FR_ACTUAL_GIT" symbolic-ref HEAD refs/heads/other 2>/dev/null; then
      printf 'unexpected switch' > lock-result

    else
      printf 'blocked' > lock-result
    fi
  fi
  ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#
        };
        shim(root, script, &mut command);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["applied"], true);
        assert_eq!(git(root, &["symbolic-ref", "HEAD"]), b"refs/heads/main\n");
        if late_failure {
            assert!(result["reference_update"]["warning"].is_string());
        } else {
            assert_eq!(fs::read(root.join("lock-result")).unwrap(), b"blocked");
        }
    }
}

#[test]
fn commit_distinguishes_reconciled_publication_from_an_unconfirmed_result() {
    for abort in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        let old = git(root, &["rev-parse", "HEAD"]);
        let preview = report(root, &["-m", "reviewed"]);
        let mut command = fr(
            root,
            &[
                "-m",
                "reviewed",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
        );
        shim(
            root,
            r#"#!/bin/sh
case " $* " in
*' update-ref '*)

  while IFS= read -r line; do
    if [ "$FR_ABORT_COMMIT" = 1 ] && [ "$line" = commit ]; then
      printf 'abort\n'

    else
      printf '%s\n' "$line"
    fi

  done | "$FR_ACTUAL_GIT" "$@" | while IFS= read -r line; do
    case "$line" in
      'commit: ok'|'abort: ok') ;;
      *) printf '%s\n' "$line" ;;
    esac
  done
  exit 42
  ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#,
            &mut command,
        );
        command.env("FR_ABORT_COMMIT", if abort { "1" } else { "0" });
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["reference_update"]["acknowledged"], false);
        assert!(result["reference_update"]["warning"].is_string());
        if abort {
            assert!(result["applied"].is_null());
            assert_eq!(git(root, &["rev-parse", "HEAD"]), old);
        } else {
            assert_eq!(result["applied"], true);
            assert_eq!(
                result["reference_update"]["observed_commit"],
                result["commit"]
            );
        }
        assert!(!root.join(".git/HEAD.lock").exists());
    }
}

#[test]
fn commit_rejects_index_races_and_existing_reference_locks_without_publishing() {
    for race in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fixture(root);
        let old = git(root, &["rev-parse", "HEAD"]);
        let preview = report(root, &["-m", "reviewed"]);
        let mut command = fr(
            root,
            &[
                "-m",
                "reviewed",
                "--basis",
                preview["basis"].as_str().unwrap(),
                "--write",
            ],
        );
        if race {
            shim(
                root,
                r#"#!/bin/sh
case " $* " in
*' commit-tree '*)
  "$FR_ACTUAL_GIT" "$@" || exit $?
  printf 'foreign index' > .git/index
  exit 0
  ;;
esac

exec "$FR_ACTUAL_GIT" "$@"
"#,
                &mut command,
            );
        } else {
            fs::write(root.join(".git/refs/heads/main.lock"), "another writer").unwrap();
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let message = result["error"]["message"].as_str().unwrap();
        assert!(
            message.contains(if race {
                "index changed"
            } else {
                "refused the prepared commit"
            }),
            "{result}"
        );
        assert_eq!(git(root, &["rev-parse", "HEAD"]), old);
        assert!(!root.join(".git/index.lock").exists());
        if !race {
            assert_eq!(
                fs::read(root.join(".git/refs/heads/main.lock")).unwrap(),
                b"another writer"
            );
        }
    }
}

#[test]
fn commit_refuses_unmerged_entries_and_requires_repository_local_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root);
    git(root, &["config", "--unset", "user.email"]);
    error(root, &["-m", "msg"], "Git inspection failed");
    git(root, &["config", "user.email", "fr@example.invalid"]);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("file.txt"), "other branch\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("file.txt"), "main branch\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    fs::remove_file(root.join(".git/MERGE_HEAD")).unwrap();
    error(root, &["-m", "msg"], "unmerged index entries");
}
