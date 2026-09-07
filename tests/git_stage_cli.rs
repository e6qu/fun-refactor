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

#[test]
fn stage_preview_reports_actions_without_writing_index_source_or_objects() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    for name in ["update.txt", "remove.txt", "same.txt", "unrelated.txt"] {
        fs::write(root.join(name), "old\n").unwrap();
    }
    commit(root);
    fs::write(root.join("update.txt"), "staged\n").unwrap();
    fs::write(root.join("unrelated.txt"), "unrelated staged\n").unwrap();
    git(root, &["add", "update.txt", "unrelated.txt"]);
    fs::write(root.join("update.txt"), "new working body\n").unwrap();
    fs::write(root.join("unrelated.txt"), "unrelated working\n").unwrap();
    fs::write(root.join("add.txt"), "unique new preview content\n").unwrap();
    fs::remove_file(root.join("remove.txt")).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let args = ["update.txt", "remove.txt", "same.txt", "add.txt"];
    let value = report(root, &args);
    assert_eq!(value["operation"], "stage-preview");
    assert_eq!(value["applied"], false);
    assert_eq!(value["write_supported"], false);
    assert_eq!(
        value["counts"],
        json!({"paths":4,"add":1,"remove":1,"update":1,"unchanged":1})
    );
    assert_eq!(value["entries"][0]["path"], "add.txt");
    assert!(value["entries"][0]["before"].is_null());
    let oid = value["entries"][0]["after"]["oid"].as_str().unwrap();
    assert!(!git_output(root, &["cat-file", "-e", oid]).status.success());
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read_to_string(root.join("update.txt")).unwrap(),
        "new working body\n"
    );
    assert!(!root.join(".fr-history").exists());
    assert!(!value.to_string().contains("working body"));
    assert!(!value.to_string().contains("unrelated"));
    assert!(!fr(root, &["add.txt", "--write"])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn stage_basis_ignores_unrelated_edits_and_rejects_selected_drift() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("file.txt"), "old\n").unwrap();
    fs::write(root.join("other.txt"), "other\n").unwrap();
    commit(root);
    fs::write(root.join("file.txt"), "new\n").unwrap();
    let preview = report(root, &["file.txt"]);
    let basis = preview["basis"].as_str().unwrap();
    assert_eq!(report(root, &["file.txt", "file.txt"])["basis"], basis);
    fs::write(root.join("other.txt"), "changed elsewhere\n").unwrap();
    git(root, &["add", "other.txt"]);
    assert_eq!(
        report(root, &["file.txt", "--basis", basis])["basis_verified"],
        true
    );
    fs::write(root.join("file.txt"), "alt\n").unwrap();
    error(root, &["file.txt", "--basis", basis], "stale staging basis");
    fs::write(root.join("file.txt"), "new\n").unwrap();
    git(root, &["add", "file.txt"]);
    error(root, &["file.txt", "--basis", basis], "stale staging basis");
    error(
        root,
        &["other.txt", "--basis", basis],
        "stale staging basis",
    );
}

#[test]
fn stage_preview_uses_raw_bytes_and_can_preview_unborn_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join(".gitattributes"), "*.txt text eol=lf\n").unwrap();
    fs::write(root.join("file.txt"), "raw\r\n").unwrap();
    let value = report(root, &["file.txt"]);
    assert_eq!(value["staging_semantics"], "raw-bytes-owner-executable");
    assert_eq!(value["entries"][0]["working_bytes"], 5);
    assert_eq!(value["entries"][0]["action"], "add");
    assert!(!root.join(".git/index").exists());
    fs::write(root.join("file.txt"), "raw\n").unwrap();
    assert_ne!(report(root, &["file.txt"])["basis"], value["basis"]);
}

#[test]
fn stage_preview_rejects_ignored_untracked_paths_but_allows_tracked_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("tracked.txt"), "old\n").unwrap();
    commit(root);
    fs::write(root.join(".gitignore"), "*.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "ignored\n").unwrap();
    assert_eq!(
        report(root, &["tracked.txt"])["entries"][0]["action"],
        "unchanged"
    );
    error(root, &["ignored.txt"], "ignored untracked paths");
    fs::write(root.join(".gitattributes"), "unselected filter=blocked\n").unwrap();
    report(root, &["tracked.txt"]);
    fs::write(root.join(".gitattributes"), "tracked.txt filter=blocked\n").unwrap();
    error(root, &["tracked.txt"], "content filters");
}

#[cfg(unix)]
#[test]
fn stage_preview_rejects_unsafe_paths_and_binds_owner_executable_modes() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("file.txt"), "content\n").unwrap();
    fs::write(root.join("binary.txt"), b"a\0b").unwrap();
    fs::write(root.join("nonutf8.txt"), [0xff]).unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    symlink("file.txt", root.join("link.txt")).unwrap();
    commit(root);
    for path in [
        "../file.txt",
        "/tmp/file.txt",
        "missing",
        "directory",
        "link.txt",
        "binary.txt",
        "nonutf8.txt",
    ] {
        let output = fr(root, &[path]).output().unwrap();
        assert!(!output.status.success(), "{path}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value.get("entries").is_none());
    }
    let original = report(root, &["file.txt"]);
    fs::set_permissions(root.join("file.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    git(root, &["config", "core.filemode", "false"]);
    let changed = report(root, &["file.txt"]);
    assert_eq!(changed["entries"][0]["action"], "update");
    assert_eq!(changed["entries"][0]["after"]["mode"], "100755");
    assert_ne!(changed["basis"], original["basis"]);
    error(root, &vec!["file.txt"; 33], "1 through 32");
}

#[test]
fn stage_preview_selects_linked_indexes_and_literal_sha256_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    let name = "file 名[*].txt";
    fs::write(root.join(name), "old\n").unwrap();
    commit(root);
    let work_dir = tempfile::tempdir().unwrap();
    let work = work_dir.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", work.to_str().unwrap()],
    );
    fs::write(work.join(name), "new\n").unwrap();
    fs::create_dir(work.join("nested")).unwrap();
    let value = report(&work.join("nested"), &[name]);
    assert_eq!(
        value["repository_root"],
        work.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(value["entries"][0]["path"], name);
    assert_eq!(
        value["entries"][0]["after"]["oid"].as_str().unwrap().len(),
        64
    );
    error(
        root,
        &[name, "--basis", value["basis"].as_str().unwrap()],
        "stale staging basis",
    );
    assert_eq!(report(root, &[name])["entries"][0]["action"], "unchanged");
}

#[cfg(unix)]
#[test]
fn stage_preview_refuses_source_and_index_races_without_partial_reports() {
    use std::os::unix::fs::PermissionsExt;
    for action in [
        "printf 'raced\\n' > file.txt",
        "\"$FR_ACTUAL_GIT\" add file.txt",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init(root);
        fs::write(root.join("file.txt"), "old\n").unwrap();
        commit(root);
        fs::write(root.join("file.txt"), "new\n").unwrap();
        let actual = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(actual.status.success());
        let shim = root.join("shim");
        fs::create_dir(&shim).unwrap();
        fs::write(shim.join("git"), format!("#!/bin/sh\ncase \" $* \" in\n*' hash-object '*)\n  \"$FR_ACTUAL_GIT\" \"$@\" || exit $?\n  {action}\n  ;;\n*) exec \"$FR_ACTUAL_GIT\" \"$@\" ;;\nesac\n")).unwrap();
        fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
        let output = fr(root, &["file.txt"])
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
                .contains("changed during staging preview"),
            "{value}"
        );
        assert!(value.get("entries").is_none());
    }
}

#[test]
fn stage_preview_limits_conflict_checks_to_selected_entries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("selected.txt"), "old\n").unwrap();
    fs::write(root.join("conflict.txt"), "base\n").unwrap();
    commit(root);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("conflict.txt"), "other\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("conflict.txt"), "main\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    fs::write(root.join("selected.txt"), "new\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    assert_eq!(
        report(root, &["selected.txt"])["entries"][0]["action"],
        "update"
    );
    error(root, &["conflict.txt"], "unmerged Git snapshot");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}
