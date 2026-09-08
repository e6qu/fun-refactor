use serde_json::Value;
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
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "status"])
        .args(args);
    command
}

fn report(root: &Path, args: &[&str]) -> Value {
    let out = fr(root, args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn error(out: Output, expected: &str) {
    assert!(!out.status.success());
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains(expected),
        "{json}"
    );
    assert!(json.get("entries").is_none());
}

#[test]
fn pages_preserve_all_git_states_paths_and_index_without_loading_history() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    let empty = report(root, &[]);
    assert_eq!(empty["unborn"], true);
    assert_eq!(empty["clean"], true);
    assert_eq!(empty["page"]["next"], Value::Null);
    for name in [
        "mixed.txt",
        "gone.txt",
        "from.txt",
        "mode.txt",
        "linkish.txt",
    ] {
        fs::write(root.join(name), format!("base {name}\n")).unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o644)).unwrap();
    }
    fs::write(root.join(".gitignore"), "*.skip\n.fr-history/\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("mixed.txt"), "staged\n").unwrap();
    git(root, &["add", "mixed.txt"]);
    fs::write(root.join("mixed.txt"), "unstaged\n").unwrap();
    fs::remove_file(root.join("gone.txt")).unwrap();
    let renamed = "to space\t\n\"é.txt";
    git(root, &["mv", "from.txt", renamed]);
    fs::set_permissions(root.join("mode.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::remove_file(root.join("linkish.txt")).unwrap();
    symlink("missing-target", root.join("linkish.txt")).unwrap();
    fs::create_dir(root.join("new")).unwrap();
    fs::write(root.join("new/deep.txt"), "untracked\n").unwrap();
    fs::write(root.join(" leading\nfile.txt"), "untracked\n").unwrap();
    fs::write(root.join("ignored.skip"), "ignored\n").unwrap();
    fs::create_dir(root.join(".fr-history")).unwrap();
    fs::write(root.join(".fr-history/state.json"), "invalid journal").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let first = report(root, &["--limit", "2"]);
    assert_eq!(
        first["counts"],
        serde_json::json!({"paths":7,"staged":2,"unstaged":4,"untracked":2,"conflicted":0})
    );
    let mut rows = first["entries"].as_array().unwrap().clone();
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut next = Some(cursor.to_owned());
    while let Some(cursor) = next {
        let page = report(&root.join("new"), &["--limit", "2", "--cursor", &cursor]);
        assert_eq!(page["status_revision"], first["status_revision"]);
        assert_eq!(page["counts"], first["counts"]);
        rows.extend(page["entries"].as_array().unwrap().iter().cloned());
        next = page["page"]["next"].as_str().map(str::to_owned);
    }
    assert_eq!(rows.len(), 7);
    assert!(rows
        .windows(2)
        .all(|pair| pair[0]["path"].as_str() < pair[1]["path"].as_str()));
    let rename = rows.iter().find(|row| row["path"] == renamed).unwrap();
    assert_eq!(rename["original_path"], "from.txt");
    assert_eq!(rename["similarity"], 100);
    let mixed = rows.iter().find(|row| row["path"] == "mixed.txt").unwrap();
    assert_eq!(mixed["xy"], "MM");
    assert_eq!(report(root, &["--kind", "staged"])["page"]["total"], 2);
    assert_eq!(report(root, &["--kind", "unstaged"])["page"]["total"], 4);
    assert_eq!(report(root, &["--kind", "untracked"])["page"]["total"], 2);
    assert_eq!(report(root, &["--kind", "conflicted"])["page"]["total"], 0);
    error(
        fr(root, &["--cursor", cursor, "--kind", "staged"])
            .output()
            .unwrap(),
        "stale cursor",
    );
    fs::write(root.join("mixed.txt"), "different unstaged bytes\n").unwrap();
    assert_eq!(
        report(root, &["--cursor", cursor])["status_revision"],
        first["status_revision"]
    );
    fs::write(root.join("newer.txt"), "new\n").unwrap();
    error(
        fr(root, &["--cursor", cursor]).output().unwrap(),
        "stale cursor",
    );
    fs::remove_file(root.join("newer.txt")).unwrap();
    let beyond = format!(
        "frg1:{}:{}",
        first["status_revision"].as_str().unwrap(),
        usize::MAX
    );
    error(fr(root, &["--cursor", &beyond]).output().unwrap(), "beyond");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read_to_string(root.join(".fr-history/state.json")).unwrap(),
        "invalid journal"
    );
}

#[test]
fn conflicts_are_separate_from_staged_changes_and_preserve_merge_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("conflict.txt"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    git(root, &["checkout", "-qb", "side"]);
    fs::write(root.join("conflict.txt"), "side\n").unwrap();
    git(root, &["commit", "-qam", "side"]);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("conflict.txt"), "main\n").unwrap();
    git(root, &["commit", "-qam", "main"]);
    assert!(!git_output(root, &["merge", "side"]).status.success());
    let index = fs::read(root.join(".git/index")).unwrap();
    let state = report(root, &["--kind", "conflicted"]);
    assert_eq!(
        state["counts"],
        serde_json::json!({"paths":1,"staged":0,"unstaged":0,"untracked":0,"conflicted":1})
    );
    assert_eq!(state["entries"][0]["xy"], "UU");
    assert_eq!(state["entries"][0]["index_status"], Value::Null);
    assert_eq!(state["entries"][0]["worktree_status"], Value::Null);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn linked_worktrees_and_environment_isolation_select_the_right_repository() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    git(source.path(), &["init", "-q", "-b", "main"]);
    fs::write(source.path().join("base.txt"), "base\n").unwrap();
    git(source.path(), &["add", "."]);
    git(source.path(), &["commit", "-qm", "base"]);
    let linked = destination.path().join("linked\nworkspace");
    git(
        source.path(),
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    fs::write(linked.join("untracked.txt"), "new\n").unwrap();
    let pointer = fs::read(linked.join(".git")).unwrap();
    let out = fr(&linked, &[])
        .env("GIT_DIR", source.path().join(".git"))
        .env("GIT_WORK_TREE", source.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let state: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        state["repository_root"],
        linked.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(state["branch"], "(detached)");
    assert_eq!(state["entries"][0]["path"], "untracked.txt");
    assert_eq!(fs::read(linked.join(".git")).unwrap(), pointer);
    assert_eq!(report(source.path(), &[])["clean"], true);
}

#[test]
fn submodules_are_outside_the_status_scope_even_when_the_gitlink_is_staged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    let module = root.join("module");
    fs::create_dir(&module).unwrap();
    git(&module, &["init", "-q"]);
    fs::write(module.join("file"), "base\n").unwrap();
    git(&module, &["add", "."]);
    git(&module, &["commit", "-qm", "base"]);
    git(root, &["add", "module"]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(module.join("file"), "changed\n").unwrap();
    git(&module, &["commit", "-qam", "change"]);
    fs::write(module.join("file"), "dirty\n").unwrap();
    assert_eq!(report(root, &[])["clean"], true);
    git(root, &["add", "module"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let state = report(root, &[]);
    assert_eq!(state["submodules"], "ignored");
    assert_eq!(state["clean"], true);
    assert_eq!(state["counts"]["paths"], 0);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn status_refuses_filters_and_missing_git_and_validates_page_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    error(fr(root, &[]).output().unwrap(), "Git working tree");
    git(root, &["init", "-q"]);
    fs::write(root.join("app.txt"), "base\n").unwrap();
    git(root, &["add", "app.txt"]);
    fs::write(root.join(".gitattributes"), "app.txt filter=tripwire\n").unwrap();
    git(
        root,
        &["config", "filter.tripwire.clean", "touch filter-ran; cat"],
    );
    git(
        root,
        &["config", "core.fsmonitor", "touch monitor-ran; printf x"],
    );
    error(fr(root, &[]).output().unwrap(), "content filters");
    assert!(!root.join("filter-ran").exists());
    assert!(!root.join("monitor-ran").exists());
    fs::remove_file(root.join(".gitattributes")).unwrap();
    report(root, &[]);
    assert!(!root.join("filter-ran").exists());
    assert!(!root.join("monitor-ran").exists());
    error(
        fr(root, &[])
            .env("PATH", "/nonexistent-fr-git-test")
            .output()
            .unwrap(),
        "running Git",
    );
    for limit in ["0", "501", "18446744073709551615"] {
        error(
            fr(root, &["--limit", limit]).output().unwrap(),
            "limit must be",
        );
    }
}
