use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn fr(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .env("PATH", "/nonexistent-fr-patch-test")
        .args(["--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "core.autocrlf=false"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn checked(root: &Path, args: &[&str], matches: bool) -> Value {
    let output = fr(root, args);
    assert_eq!(
        output.status.code(),
        Some(if matches { 0 } else { 1 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["matches_patch_basis"], matches);
    assert!(report.get("patch").is_none());
    assert!(output.stderr.is_empty());
    report
}

#[test]
fn saved_plan_exports_without_git_and_applies_without_disturbing_unrelated_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let original = "fn helper() {}\nfn main() { helper(); }";
    fs::write(root.join("app.rs"), original).unwrap();
    let plan: Value = serde_json::from_slice(&success(fr(
        root,
        &["--json", "rename", "helper", "renamed", "--save-plan"],
    )))
    .unwrap();
    assert_eq!(plan["transaction"], 1);
    let patch = success(fr(root, &["history", "patch", "1"]));
    assert!(patch.starts_with(b"diff --git "));
    let report: Value =
        serde_json::from_slice(&success(fr(root, &["--json", "history", "patch", "1"]))).unwrap();
    assert_eq!(report["patch"].as_str().unwrap().as_bytes(), patch);
    assert_eq!(report["format"], "git-text-diff");
    assert_eq!(report["mode_scope"], "regular-or-executable");
    assert_eq!(report["status"], "planned");
    assert_eq!(report["reverse"], false);
    assert_eq!(report["files"], 1);
    success(git(root, &["init", "-q"], b""));
    fs::write(root.join("unrelated.txt"), "baseline\n").unwrap();
    success(git(root, &["add", "app.rs", "unrelated.txt"], b""));
    fs::write(root.join("unrelated.txt"), "staged\n").unwrap();
    success(git(root, &["add", "unrelated.txt"], b""));
    fs::write(root.join("unrelated.txt"), "unstaged\n").unwrap();
    fs::write(root.join("untracked.txt"), "untracked\n").unwrap();
    fs::write(root.join("app.rs"), "conflicting drift\n").unwrap();
    let status = success(git(root, &["status", "--porcelain=v1", "-z"], b""));
    let index = fs::read(root.join(".git/index")).unwrap();
    let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
    assert_eq!(success(fr(root, &["history", "patch", "1"])), patch);
    let check = checked(root, &["history", "patch", "1", "--check"], false);
    assert_eq!(check["files"][0]["content_matches"], false);
    assert_eq!(check["files"][0]["git_mode_matches"], true);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        success(git(root, &["status", "--porcelain=v1", "-z"], b"")),
        status
    );
    assert!(!git(root, &["apply", "--check", "-"], &patch)
        .status
        .success());
    assert!(!git(root, &["apply", "-"], &patch).status.success());
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        "conflicting drift\n"
    );
    fs::write(root.join("app.rs"), original).unwrap();
    checked(root, &["--json", "history", "patch", "1", "--check"], true);
    success(git(root, &["apply", "--check", "-"], &patch));
    success(git(root, &["apply", "-"], &patch));
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        original.replace("helper", "renamed")
    );
    let reverse = success(fr(root, &["history", "patch", "1", "--reverse"]));
    checked(
        root,
        &["history", "patch", "1", "--check", "--reverse"],
        true,
    );
    success(git(root, &["apply", "--check", "-"], &reverse));
    success(git(root, &["apply", "-"], &reverse));
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("untracked.txt")).unwrap(),
        "untracked\n"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(root.join(".fr-history/state.json")).unwrap(),
        journal
    );
}

#[test]
fn writes_a_new_patch_artifact_with_bounded_identity_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("app.rs"), "fn helper() {}\n").unwrap();
    success(fr(root, &["rename", "helper", "renamed", "--save-plan"]));
    let expected = success(fr(root, &["history", "patch", "1"]));
    let report: Value = serde_json::from_slice(&success(fr(
        root,
        &["history", "patch", "1", "--output", "change.patch"],
    )))
    .unwrap();
    assert!(report.get("patch").is_none());
    assert_eq!(report["patch_bytes"], expected.len());
    assert_eq!(report["output"], "change.patch");
    assert_eq!(fs::read(root.join("change.patch")).unwrap(), expected);
    assert_eq!(report["patch_sha256"].as_str().unwrap().len(), 64);

    let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
    let refused = fr(root, &["history", "patch", "1", "--output", "change.patch"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("already exists"));
    assert_eq!(fs::read(root.join("change.patch")).unwrap(), expected);
    assert_eq!(
        fs::read(root.join(".fr-history/state.json")).unwrap(),
        journal
    );
}

#[test]
fn basis_check_reports_receiving_content_existence_and_permission_differences() {
    let source = tempfile::tempdir().unwrap();
    let receiving = tempfile::tempdir().unwrap();
    let original = format!(
        "fn helper() {{}}\nfn main() {{ helper(); }}\n{}fn untouched() {{}}\n",
        "\n".repeat(20)
    );
    fs::write(source.path().join("app.rs"), &original).unwrap();
    fs::set_permissions(
        source.path().join("app.rs"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    success(fr(
        source.path(),
        &["rename", "helper", "renamed", "--save-plan"],
    ));
    let args = [
        "history",
        "patch",
        "1",
        "--check",
        "--against",
        receiving.path().to_str().unwrap(),
    ];
    let missing = checked(source.path(), &args, false);
    assert_eq!(missing["checked_files"], 1);
    assert_eq!(missing["files"][0]["expected_exists"], true);
    assert_eq!(missing["files"][0]["actual_exists"], false);
    assert_eq!(missing["files"][0]["actual_mode"], Value::Null);
    let path = receiving.path().join("app.rs");
    fs::write(&path, &original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let projected = checked(source.path(), &args, true);
    assert_eq!(projected["matches_recorded_snapshots"], false);
    assert_eq!(projected["files"][0]["matches_recorded_snapshot"], false);
    assert_eq!(projected["files"][0]["expected_mode"], 0o600);
    assert_eq!(projected["files"][0]["actual_mode"], 0o644);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = checked(source.path(), &args, false);
    assert_eq!(executable["files"][0]["content_matches"], true);
    assert_eq!(executable["files"][0]["git_mode_matches"], false);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        checked(source.path(), &args, true)["matches_recorded_snapshots"],
        true
    );
    success(git(receiving.path(), &["init", "-q"], b""));
    let patch = success(fr(source.path(), &["history", "patch", "1"]));
    fs::write(&path, original.replace("untouched", "outside_hunk_drift")).unwrap();
    success(git(receiving.path(), &["apply", "--check", "-"], &patch));
    checked(source.path(), &args, false);
    fs::write(&path, &original).unwrap();
    success(git(receiving.path(), &["apply", "-"], &patch));
    checked(source.path(), &args, false);
    let mut reverse_args = args.to_vec();
    reverse_args.push("--reverse");
    let reverse = checked(source.path(), &reverse_args, true);
    assert_eq!(reverse["reverse"], true);
    assert_eq!(reverse["record_basis"], projected["record_basis"]);
    assert_eq!(
        fs::read_to_string(source.path().join("app.rs")).unwrap(),
        original
    );
    assert!(fs::symlink_metadata(receiving.path().join(".fr-history")).is_err());
}

#[test]
fn basis_check_refuses_unsafe_or_unreadable_receiving_targets_without_partial_reports() {
    let source = tempfile::tempdir().unwrap();
    let receiving = tempfile::tempdir().unwrap();
    fs::write(source.path().join("app.rs"), "fn helper() {}\n").unwrap();
    success(fr(
        source.path(),
        &["rename", "helper", "renamed", "--save-plan"],
    ));
    let args = [
        "history",
        "patch",
        "1",
        "--check",
        "--against",
        receiving.path().to_str().unwrap(),
    ];
    let path = receiving.path().join("app.rs");
    symlink(source.path().join("app.rs"), &path).unwrap();
    let linked = fr(source.path(), &args);
    assert!(!linked.status.success());
    assert!(linked.stdout.is_empty());
    assert!(String::from_utf8_lossy(&linked.stderr).contains("symlink"));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let directory = fr(source.path(), &args);
    assert!(!directory.status.success());
    assert!(directory.stdout.is_empty());
    fs::remove_dir(&path).unwrap();
    fs::write(&path, [0xff, 0xfe]).unwrap();
    let mut json_args = args.to_vec();
    json_args.push("--json");
    let binary = fr(source.path(), &json_args);
    assert!(!binary.status.success());
    let error: Value = serde_json::from_slice(&binary.stdout).unwrap();
    assert_eq!(error["error"]["kind"], "io");
    assert!(error.get("files").is_none());
    let invalid = fr(
        source.path(),
        &[
            "history",
            "patch",
            "1",
            "--against",
            receiving.path().to_str().unwrap(),
        ],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    let file_root = fr(
        source.path(),
        &[
            "history",
            "patch",
            "1",
            "--check",
            "--against",
            path.to_str().unwrap(),
        ],
    );
    assert!(!file_root.status.success());
    assert!(file_root.stdout.is_empty());
}

#[test]
fn failed_patch_export_has_no_partial_patch_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let missing = fr(dir.path(), &["history", "patch", "42"]);
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    fs::write(dir.path().join("app.rs"), "fn helper() {}\n").unwrap();
    success(fr(
        dir.path(),
        &["rename", "helper", "renamed", "--save-plan"],
    ));
    let path = dir.path().join(".fr-history/state.json");
    let mut journal: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    journal["records"][0]["changes"][0]["after"]["content"] = "binary\0snapshot".into();
    fs::write(path, serde_json::to_vec(&journal).unwrap()).unwrap();
    let binary = fr(dir.path(), &["history", "patch", "1"]);
    assert!(!binary.status.success());
    assert!(binary.stdout.is_empty());
    assert!(String::from_utf8_lossy(&binary.stderr).contains("binary snapshot"));
}
