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
        .args(["git", "changes"])
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
fn pages_metadata_and_counts_across_all_three_comparisons() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    for name in ["mixed.rs", "gone", "mode", "binary", "linkish", "rename"] {
        fs::write(root.join(name), "base\n").unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o644)).unwrap();
    }
    commit(root);
    fs::write(root.join("mixed.rs"), "staged\n").unwrap();
    git(root, &["add", "mixed.rs"]);
    fs::write(root.join("mixed.rs"), "working\n").unwrap();
    fs::remove_file(root.join("gone")).unwrap();
    git(root, &["config", "core.filemode", "true"]);
    fs::set_permissions(root.join("mode"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("binary"), b"binary\0content").unwrap();
    fs::remove_file(root.join("linkish")).unwrap();
    symlink("missing", root.join("linkish")).unwrap();
    let unusual = "renamed\t\n:(glob)*é";
    git(root, &["mv", "rename", unusual]);
    fs::write(root.join("untracked"), "not selected\n").unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join(".fr-history")).unwrap();
    fs::write(root.join(".fr-history/state.json"), "broken").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let all = report(root, &["--limit", "500"]);
    assert_eq!(
        all["counts"],
        json!({"paths":5,"added":0,"deleted":1,"modified":3,"type_changed":1,"binary":1,"detail_candidates":4})
    );
    assert_eq!(all["scope"], "index-to-worktree");
    let first = report(root, &["--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let rest = report(
        &root.join("nested"),
        &["--limit", "500", "--cursor", cursor],
    );
    assert_eq!(
        rest["entries"],
        json!(&all["entries"].as_array().unwrap()[1..])
    );
    let type_changed = all["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "linkish")
        .unwrap();
    assert_eq!(type_changed["detail_candidate"], false);
    let staged = report(root, &["--staged"]);
    assert_eq!(staged["counts"]["paths"], 3);
    assert!(staged["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["path"] == unusual && e["status"] == "A"));
    assert_eq!(staged["counts"]["deleted"], 1);
    let since = report(root, &["--since", "HEAD"]);
    assert_eq!(since["counts"]["paths"], 7);
    assert_eq!(
        since["base_commit"],
        String::from_utf8(git(root, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
    );
    assert!(!since.to_string().contains("not selected"));
    error(root, &["--staged", "--cursor", cursor], "stale cursor");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn cursor_identity_is_metadata_and_rejects_changed_counts_and_invalid_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    for name in ["a", "b"] {
        fs::write(root.join(name), "base\n").unwrap();
    }
    commit(root);
    for name in ["a", "b"] {
        fs::write(root.join(name), "first edit\n").unwrap();
    }
    let first = report(root, &["--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    fs::write(root.join("b"), "different bytes\n").unwrap();
    assert_eq!(
        report(root, &["--cursor", cursor])["changes_revision"],
        first["changes_revision"]
    );
    fs::write(root.join("b"), "two\nlines\n").unwrap();
    error(root, &["--cursor", cursor], "stale cursor");
    for limit in ["0", "501"] {
        error(root, &["--limit", limit], "between");
    }
    let current = report(root, &[]);
    let beyond = format!(
        "frc1:{}:99999",
        current["changes_revision"].as_str().unwrap()
    );
    error(root, &["--cursor", &beyond], "beyond");
    error(root, &["--cursor", "bad"], "invalid Git changes cursor");
    error(root, &["--since=-R"], "resolve");
    assert!(!fr(root, &["--staged", "--since", "HEAD"])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn handles_unborn_empty_and_mode_only_changes_without_index_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    assert_eq!(report(root, &["--staged"])["clean"], true);
    fs::write(root.join("empty"), "").unwrap();
    git(root, &["add", "empty"]);
    let value = report(root, &["--staged"]);
    assert_eq!(value["base_commit"], Value::Null);
    assert_eq!(value["entries"][0]["added_lines"], 0);
    assert_eq!(value["entries"][0]["status"], "A");
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("empty"), "temporary\n").unwrap();
    fs::write(root.join("empty"), "").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    assert_eq!(report(root, &[])["clean"], true);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    git(root, &["config", "core.filemode", "true"]);
    fs::set_permissions(root.join("empty"), fs::Permissions::from_mode(0o755)).unwrap();
    let value = report(root, &[]);
    assert_eq!(value["entries"][0]["added_lines"], 0);
    assert_eq!(value["entries"][0]["after_mode"], "100755");
}

#[test]
fn guards_filters_in_index_and_selected_commit_and_external_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    for path in ["code.rs", "historical", "filtered"] {
        fs::write(root.join(path), "base\n").unwrap();
    }
    commit(root);
    git(root, &["rm", "historical"]);
    git(root, &["commit", "-qm", "delete"]);
    for key in [
        "filter.driver.clean",
        "diff.external",
        "diff.driver.textconv",
        "core.fsmonitor",
    ] {
        git(root, &["config", key, "touch should-not-run"]);
    }
    fs::write(
        root.join(".gitattributes"),
        "code.rs diff=driver\nhistorical filter=driver\n",
    )
    .unwrap();
    fs::write(root.join("code.rs"), "changed\n").unwrap();
    assert_eq!(report(root, &[])["counts"]["paths"], 1);
    error(root, &["--since", "HEAD~1"], "content filters");
    fs::write(root.join(".gitattributes"), "filtered filter=driver\n").unwrap();
    error(root, &["--limit", "1"], "content filters");
    assert!(!root.join("should-not-run").exists());
}

#[test]
fn linked_worktrees_and_inherited_git_settings_preserve_query_roots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("code"), "base\n").unwrap();
    commit(root);
    let other = tempfile::tempdir().unwrap();
    let linked = other.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", linked.to_str().unwrap()],
    );
    fs::write(linked.join("code"), "changed\n").unwrap();
    let output = fr(&linked, &[])
        .env("GIT_DIR", "/missing")
        .env("GIT_WORK_TREE", "/missing")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["repository_root"],
        linked.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(value["counts"]["paths"], 1);
    assert_eq!(report(root, &[])["clean"], true);
    git(&linked, &["add", "code"]);
    assert_eq!(report(&linked, &[])["clean"], true);
    assert_eq!(report(&linked, &["--staged"])["counts"]["paths"], 1);
}

#[test]
fn conflicts_refuse_and_staged_submodules_stay_outside_scope() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("code"), "base\n").unwrap();
    commit(root);
    let oid = String::from_utf8(git(root, &["rev-parse", "HEAD"])).unwrap();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},submodule", oid.trim()),
        ],
    );
    assert_eq!(report(root, &["--staged"])["clean"], true);
    git(root, &["reset", "--quiet", "HEAD"]);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("code"), "other\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("code"), "main\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    let index = fs::read(root.join(".git/index")).unwrap();
    for args in [vec![], vec!["--staged"], vec!["--since", "HEAD"]] {
        error(root, &args, "unmerged");
    }
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn sha256_commit_selection_hands_literal_paths_to_symbol_details() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    let unusual = ":(glob)*[?]é\n.rs";
    for path in ["other.rs", unusual] {
        fs::write(root.join(path), "fn before() {}\n").unwrap();
    }
    commit(root);
    for path in ["other.rs", unusual] {
        fs::write(root.join(path), "fn after() {}\n").unwrap();
    }
    let value = report(root, &["--since", "HEAD", "--limit", "1"]);
    let base = value["base_commit"].as_str().unwrap();
    assert_eq!(base.len(), 64);
    let cursor = value["page"]["next"].as_str().unwrap();
    assert_eq!(value["entries"][0]["path"], unusual);
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "diff", unusual, "--symbols", "--since", base])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let details: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(details["entries"][0]["name"]["text"], "before");
    assert_eq!(details["entries"][1]["name"]["text"], "after");
    git(root, &["commit", "--allow-empty", "-qm", "advance HEAD"]);
    error(
        root,
        &["--since", "HEAD", "--cursor", cursor],
        "stale cursor",
    );
    assert_eq!(
        report(root, &["--since", base, "--cursor", cursor])["changes_revision"],
        value["changes_revision"]
    );
}
