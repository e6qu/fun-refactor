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

fn fixture(root: &Path) -> String {
    init(root);
    let source = "from dependency import leaf\ndef changed():\n    number = 1\n    leaf()\n";
    fs::write(root.join("app.py"), source).unwrap();
    fs::write(root.join("dependency.py"), "def leaf():\n    return 1\n").unwrap();
    fs::write(
        root.join("caller.py"),
        "from app import changed\nchanged()\n",
    )
    .unwrap();
    commit(root);
    source.to_owned()
}

#[test]
fn working_context_selects_index_and_commit_bases_independently() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(root.join("app.py"), before.replace("leaf", "staged")).unwrap();
    fs::write(root.join("dependency.py"), "def staged():\n    return 2\n").unwrap();
    git(root, &["add", "."]);
    fs::write(root.join("app.py"), before.replace("leaf", "working")).unwrap();
    fs::write(root.join("dependency.py"), "def working():\n    return 3\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    for (flags, old, new, scope) in [
        (vec![], "staged", "working", "index-to-worktree"),
        (
            vec!["--since", "HEAD"],
            "leaf",
            "working",
            "commit-to-worktree",
        ),
        (vec!["--staged"], "leaf", "staged", "head-to-index"),
    ] {
        let mut args = vec![
            "app.py",
            "--calls",
            "--include",
            "dependency.py",
            "--include",
            "caller.py",
        ];
        args.extend(flags);
        let value = report(root, &args);
        assert_eq!(value["scope"], scope);
        for (side, name) in [("before", old), ("after", new)] {
            let rows = value["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["side"] == side)
                .collect::<Vec<_>>();
            assert!(
                rows.iter().any(|r| r["callee"]["name"]["text"] == name
                    && r["callee"]["path"] == "dependency.py"),
                "{value}"
            );
            assert!(
                rows.iter()
                    .any(|r| r["site"]["path"] == "caller.py" && r["scope_relation"] == "incoming"),
                "{value}"
            );
        }
        if scope != "head-to-index" {
            assert_eq!(
                value["structure"]["coverage"]["context"]["scope"],
                "explicit-working-files"
            );
            assert!(value["structure"]["coverage"]["context"]["working_revision"].is_string());
        }
    }
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn working_context_cursors_cover_hidden_bytes_and_the_selected_set() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    let args = [
        "app.py",
        "--calls",
        "--include",
        "dependency.py",
        "--include",
        "caller.py",
    ];
    let all = report(root, &args);
    let mut page = args.to_vec();
    page.extend(["--limit", "1"]);
    let first = report(root, &page);
    let token = first["page"]["next"].as_str().unwrap();
    let mut next = args.to_vec();
    next.extend(["--cursor", token]);
    assert_eq!(
        report(root, &next)["entries"],
        json!(&all["entries"].as_array().unwrap()[1..])
    );
    error(
        root,
        &[
            "app.py",
            "--calls",
            "--include",
            "caller.py",
            "--cursor",
            token,
        ],
        "stale cursor",
    );
    let mut since = next.clone();
    since.extend(["--since", "HEAD"]);
    error(root, &since, "stale cursor");
    fs::write(root.join("dependency.py"), "def leaf():\n    return 2\n").unwrap();
    assert_eq!(report(root, &args)["entries"], all["entries"]);
    error(root, &next, "stale cursor");
}

#[test]
fn raw_working_context_preserves_crlf_while_focus_requires_diff_correspondence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(root.join(".gitattributes"), "*.py text eol=lf\n").unwrap();
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    let args = ["app.py", "--calls", "--include", "dependency.py"];
    let lf = report(root, &args);
    fs::write(
        root.join("dependency.py"),
        "def leaf():\r\n    return 1\r\n",
    )
    .unwrap();
    let crlf = report(root, &args);
    assert_eq!(lf["entries"], crlf["entries"]);
    let context = &crlf["structure"]["coverage"]["context"]["files"][0];
    assert_eq!(context["after"]["source_basis"], "raw-worktree");
    assert_ne!(context["before"]["blob"], context["after"]["blob"]);
    assert_ne!(lf["structure"]["revision"], crlf["structure"]["revision"]);
    fs::write(
        root.join("app.py"),
        before
            .replace("number = 1", "number = 2")
            .replace('\n', "\r\n"),
    )
    .unwrap();
    error(root, &args, "focus snapshot differs");
}

#[test]
fn missing_working_context_and_untracked_replacements_have_explicit_absent_sides() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    fs::remove_file(root.join("dependency.py")).unwrap();
    let args = ["app.py", "--calls", "--include", "dependency.py"];
    let removed = report(root, &args);
    assert_eq!(
        removed["structure"]["coverage"]["context"]["files"][0]["after"]["status"],
        "absent"
    );
    assert!(removed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["side"] == "after" && r["status"] == "unresolved"));
    git(root, &["rm", "dependency.py"]);
    fs::write(
        root.join("dependency.py"),
        "def leaf():\n    return 'untracked'\n",
    )
    .unwrap();
    error(root, &args, "absent from the selected index and commit");
    let mut since = args.to_vec();
    since.extend(["--since", "HEAD"]);
    let ignored = report(root, &since);
    assert_eq!(
        ignored["structure"]["coverage"]["context"]["files"][0]["after"]["status"],
        "absent"
    );
    assert!(!ignored.to_string().contains("untracked"));
    git(root, &["rm", "-f", "app.py"]);
    let deleted = report(root, &since);
    assert!(deleted["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["side"] == "before"));
}

#[cfg(unix)]
#[test]
fn working_context_refuses_symlinks_and_checks_projected_modes() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    let args = ["app.py", "--calls", "--include", "dependency.py"];
    let first = report(root, &args);
    fs::set_permissions(
        root.join("dependency.py"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let executable = report(root, &args);
    assert_eq!(
        executable["structure"]["coverage"]["context"]["files"][0]["after"]["mode"],
        "100755"
    );
    assert_ne!(
        first["structure"]["revision"],
        executable["structure"]["revision"]
    );
    fs::write(root.join("app.py"), &before).unwrap();
    fs::set_permissions(root.join("app.py"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(report(root, &args)["page"]["total"], 0);
    git(root, &["config", "core.filemode", "false"]);
    assert_eq!(report(root, &args)["page"]["total"], 0);
    fs::remove_file(root.join("dependency.py")).unwrap();
    symlink("caller.py", root.join("dependency.py")).unwrap();
    error(root, &args, "traverses a symlink");
}

#[test]
fn working_context_reports_partial_sources_and_rejects_binary_or_non_utf8_context() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let before = fixture(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    fs::write(
        root.join("dependency.py"),
        "def leaf():\n    return 1\ndef broken(:\n",
    )
    .unwrap();
    let args = ["app.py", "--calls", "--include", "dependency.py"];
    let partial = report(root, &args);
    assert_eq!(
        partial["structure"]["coverage"]["after"]["calls"]["status"],
        "partial"
    );
    fs::write(root.join("dependency.py"), b"a\0b").unwrap();
    error(root, &args, "binary call context");
    fs::write(root.join("dependency.py"), [0xff, 0xfe]).unwrap();
    error(root, &args, "reading working Git snapshot");
}

#[cfg(unix)]
#[test]
fn working_context_rechecks_focus_context_and_missing_files_after_the_diff() {
    use std::os::unix::fs::PermissionsExt;
    for target in ["app.py", "dependency.py", "caller.py"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let before = fixture(root);
        fs::write(
            root.join("app.py"),
            before.replace("number = 1", "number = 2"),
        )
        .unwrap();
        if target == "caller.py" {
            fs::remove_file(root.join(target)).unwrap();
        }
        let actual = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(actual.status.success());
        let shim = root.join("shim");
        fs::create_dir(&shim).unwrap();
        fs::write(shim.join("git"), "#!/bin/sh\ncase \" $* \" in\n*' diff '*)\n  \"$FR_ACTUAL_GIT\" \"$@\" || exit $?\n  printf 'def raced():\\n    pass\\n' > \"$FR_RACE_TARGET\"\n  ;;\n*) exec \"$FR_ACTUAL_GIT\" \"$@\" ;;\nesac\n").unwrap();
        fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
        let output = fr(
            root,
            &[
                "app.py",
                "--calls",
                "--include",
                "dependency.py",
                "--include",
                "caller.py",
            ],
        )
        .env(
            "FR_ACTUAL_GIT",
            String::from_utf8(actual.stdout).unwrap().trim(),
        )
        .env("FR_RACE_TARGET", target)
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
                .contains("working files changed"),
            "{value}"
        );
        assert!(value.get("entries").is_none());
    }
}

#[test]
fn working_context_uses_linked_worktrees_literal_paths_and_sha256() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    let caller = "caller 名[*].py";
    fs::write(root.join("app.py"), "def changed():\n    return 1\n").unwrap();
    fs::write(root.join(caller), "from app import changed\nchanged()\n").unwrap();
    commit(root);
    let work_dir = tempfile::tempdir().unwrap();
    let work = work_dir.path().join("linked");
    git(
        root,
        &["worktree", "add", "-qb", "linked", work.to_str().unwrap()],
    );
    fs::write(work.join("app.py"), "def changed():\n    return 2\n").unwrap();
    fs::create_dir(work.join("nested")).unwrap();
    let args = [
        "app.py",
        "--calls",
        "--include",
        caller,
        "--since",
        "HEAD",
        "--limit",
        "1",
    ];
    let value = report(&work.join("nested"), &args);
    assert_eq!(
        value["repository_root"],
        work.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(value["page"]["total"], 2, "{value}");
    assert_eq!(value["entries"][0]["site"]["path"], caller);
    assert_eq!(
        value["structure"]["coverage"]["context"]["files"][0]["after"]["blob"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let token = value["page"]["next"].as_str().unwrap();
    let mut continued = args.to_vec();
    continued.extend(["--cursor", token]);
    error(root, &continued, "stale cursor");
    assert_eq!(report(root, &args)["changed"], false);
}
