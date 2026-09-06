use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
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
        .args(["git", "diff"])
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
fn literal_paths_scopes_pagination_and_cursors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let name = ":(glob)*[?]é\n.txt";
    let mut lines = (1..81).map(|i| format!("line {i}\n")).collect::<Vec<_>>();
    fs::write(root.join(name), lines.concat()).unwrap();
    fs::write(root.join("other.txt"), "base\n").unwrap();
    commit(root);
    lines[4] = "staged\n".into();
    fs::write(root.join(name), lines.concat()).unwrap();
    git(root, &["--literal-pathspecs", "add", "--", name]);
    lines[24] = "working one\n".into();
    lines[64] = "working two\n".into();
    fs::write(root.join(name), lines.concat()).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join(".fr-history")).unwrap();
    fs::write(root.join(".fr-history/state.json"), "broken").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let all = report(root, &[name, "--limit", "500"]);
    assert_eq!(all["counts"], json!({"hunks":2,"added":2,"deleted":2}));
    assert_eq!(all["path"], name);
    let first = report(root, &[name, "--limit", "2"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut rows = first["entries"].as_array().unwrap().clone();
    let mut next = Some(cursor.to_owned());
    while let Some(token) = next {
        let page = report(
            &root.join("nested"),
            &[name, "--limit", "3", "--cursor", &token],
        );
        assert_eq!(page["diff_revision"], all["diff_revision"]);
        rows.extend(page["entries"].as_array().unwrap().iter().cloned());
        next = page["page"]["next"].as_str().map(str::to_owned);
    }
    assert_eq!(rows, *all["entries"].as_array().unwrap());
    let staged = report(root, &[name, "--staged"]);
    assert_eq!(staged["counts"]["added"], 1);
    assert_eq!(staged["scope"], "head-to-index");
    let since = report(root, &[name, "--since", "HEAD"]);
    assert_eq!(since["counts"]["added"], 3);
    assert_eq!(
        since["base_commit"],
        String::from_utf8(git(root, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
    );
    error(
        root,
        &[name, "--staged", "--cursor", cursor],
        "stale cursor",
    );
    error(root, &["other.txt", "--cursor", cursor], "stale cursor");
    let beyond = format!("frd1:{}:999999", all["diff_revision"].as_str().unwrap());
    error(root, &[name, "--cursor", &beyond], "beyond");
    for limit in ["0", "501"] {
        error(root, &[name, "--limit", limit], "between");
    }
    lines[24] = "further work\n".into();
    fs::write(root.join(name), lines.concat()).unwrap();
    error(root, &[name, "--cursor", cursor], "stale cursor");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn excerpts_preserve_crlf_and_missing_newlines_and_hash_hidden_suffixes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("text"), "old\r\nlast").unwrap();
    commit(root);
    let huge = "名".repeat(500);
    fs::write(root.join("text"), format!("{huge}A\r\nchanged")).unwrap();
    let all = report(root, &["text"]);
    let rows = all["entries"].as_array().unwrap();
    let long = rows
        .iter()
        .find(|r| r["content"]["truncated"] == true)
        .unwrap();
    assert_eq!(long["content"]["bytes"], 1502);
    assert_eq!(long["content"]["text"].as_str().unwrap().len(), 1023);
    assert!(rows.iter().any(|r| r["content"]["text"] == "old\r"));
    assert!(rows
        .iter()
        .any(|r| r["kind"] == "no-newline" && r["old_line"] == 2));
    assert!(rows
        .iter()
        .any(|r| r["kind"] == "no-newline" && r["new_line"] == 2));
    let first = report(root, &["text", "--limit", "1"]);
    fs::write(root.join("text"), format!("{huge}B\r\nchanged")).unwrap();
    error(
        root,
        &["text", "--cursor", first["page"]["next"].as_str().unwrap()],
        "stale cursor",
    );
}

#[test]
fn unborn_empty_mode_binary_deletion_and_invalid_text() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("empty"), "").unwrap();
    fs::write(root.join("text"), "base\n").unwrap();
    fs::write(root.join("binary"), b"a\0b").unwrap();
    git(root, &["add", "."]);
    let empty = report(root, &["empty", "--staged"]);
    assert_eq!(empty["base_commit"], Value::Null);
    assert_eq!(empty["changed"], true);
    assert_eq!(empty["change"]["status"], "A");
    assert_eq!(empty["page"]["total"], 0);
    assert_eq!(report(root, &["text", "--staged"])["counts"]["added"], 1);
    git(root, &["commit", "-qm", "base"]);
    assert_eq!(report(root, &["text"])["changed"], false);
    git(root, &["config", "core.filemode", "true"]);
    fs::set_permissions(root.join("text"), fs::Permissions::from_mode(0o755)).unwrap();
    let mode = report(root, &["text"]);
    assert_eq!(mode["changed"], true);
    assert_eq!(mode["page"]["total"], 0);
    assert_eq!(mode["change"]["after_mode"], "100755");
    fs::write(root.join("binary"), b"new\0blob").unwrap();
    let binary = report(root, &["binary"]);
    assert_eq!(binary["binary"], true);
    assert_eq!(binary["counts"]["added"], Value::Null);
    assert_eq!(binary["page"]["total"], 0);
    git(root, &["rm", "-f", "text"]);
    assert_eq!(report(root, &["text", "--staged"])["change"]["status"], "D");
    assert_eq!(
        report(root, &["text", "--since", "HEAD"])["counts"]["deleted"],
        1
    );
    error(root, &["text"], "absent");
    fs::write(root.join("empty"), b"bad\xff\n").unwrap();
    error(root, &["empty"], "non-UTF-8");
}

#[test]
fn refuses_unsupported_selections_and_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/file"), "base\n").unwrap();
    fs::write(root.join("-dash"), "base\n").unwrap();
    symlink("dir/file", root.join("link")).unwrap();
    commit(root);
    assert_eq!(report(root, &["--", "-dash"])["changed"], false);
    for path in ["dir", "link", "../outside", "/absolute", "untracked"] {
        assert!(!fr(root, &[path]).output().unwrap().status.success());
    }
    error(root, &["dir/file", "--since=-R"], "resolve");
    error(root, &["link", "--since", "HEAD"], "regular files");
    fs::remove_file(root.join("dir/file")).unwrap();
    symlink("missing", root.join("dir/file")).unwrap();
    error(root, &["dir/file"], "unsupported Git diff state");
    git(root, &["checkout", "--", "dir/file"]);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("dir/file"), "other\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("dir/file"), "main\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    error(root, &["dir/file"], "unmerged");
}

#[test]
fn guards_external_commands_filters_and_index_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/filtered"), "base\n").unwrap();
    fs::write(root.join("text"), "base\n").unwrap();
    commit(root);
    for key in [
        "diff.external",
        "diff.driver.textconv",
        "core.fsmonitor",
        "filter.driver.clean",
    ] {
        git(root, &["config", key, "touch should-not-run"]);
    }
    fs::write(
        root.join(".gitattributes"),
        "text diff=driver\ndir/filtered filter=driver\n",
    )
    .unwrap();
    fs::write(root.join("text"), "changed\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let out = fr(root, &["text"])
        .env("GIT_DIR", "/missing")
        .env("GIT_WORK_TREE", "/missing")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    error(root, &["dir/filtered"], "content filters");
    error(root, &["dir"], "directories");
    fs::write(root.join("text"), "base\n").unwrap();
    assert_eq!(report(root, &["text"])["changed"], false);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(!root.join("should-not-run").exists());
}

#[test]
fn missing_promisor_objects_do_not_trigger_fetch_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("text"), "base\n").unwrap();
    commit(root);
    let oid = String::from_utf8(git(root, &["rev-parse", "HEAD:text"])).unwrap();
    let oid = oid.trim();
    fs::remove_file(root.join(".git/objects").join(&oid[..2]).join(&oid[2..])).unwrap();
    let helper = root.join("fetch-helper");
    fs::write(
        &helper,
        format!(
            "#!/bin/sh\ntouch '{}'\nexit 1\n",
            root.join("fetch-ran").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        root,
        &[
            "config",
            "remote.origin.url",
            &format!("ext::{}", helper.display()),
        ],
    );
    git(root, &["config", "remote.origin.promisor", "true"]);
    git(root, &["config", "protocol.ext.allow", "always"]);
    fs::write(root.join("text"), "changed\n").unwrap();
    let out = fr(root, &["text"])
        .env("GIT_NO_LAZY_FETCH", "0")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!root.join("fetch-ran").exists());
    assert!(!git_output(root, &["diff", "--", "text"]).status.success());
    assert!(root.join("fetch-ran").exists());
}

#[test]
fn linked_worktree_uses_its_own_files_index_and_cursor_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("text"), "base\n").unwrap();
    commit(root);
    let linked_parent = tempfile::tempdir().unwrap();
    let linked = linked_parent.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", linked.to_str().unwrap()],
    );
    for path in [root, &linked] {
        fs::write(path.join("text"), "same change\n").unwrap();
    }
    let first = report(root, &["text", "--limit", "1"]);
    let worktree = report(&linked, &["text"]);
    assert_eq!(
        worktree["repository_root"],
        linked.canonicalize().unwrap().to_str().unwrap()
    );
    assert_ne!(first["diff_revision"], worktree["diff_revision"]);
    error(
        &linked,
        &["text", "--cursor", first["page"]["next"].as_str().unwrap()],
        "stale cursor",
    );
    git(&linked, &["add", "text"]);
    assert_eq!(report(&linked, &["text"])["changed"], false);
    assert_eq!(report(root, &["text"])["changed"], true);
    assert_eq!(report(&linked, &["text", "--staged"])["changed"], true);
}
