use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const ORIGINAL: &str =
    "fn helper() {}\nfn main() { helper(); }\n\n\n\n\n\n\n\n\nfn untouched() {}\n";

fn fr(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
    command
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args);
    command
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    success(
        Command::new("git")
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap(),
    )
}

fn plan(root: &Path) {
    fs::write(root.join("app.rs"), ORIGINAL).unwrap();
    success(
        fr(root, &["rename", "helper", "renamed", "--save-plan"])
            .output()
            .unwrap(),
    );
}

fn checked(output: Output, applicable: bool) -> Value {
    assert_eq!(
        output.status.code(),
        Some(if applicable { 0 } else { 1 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["applicable"], applicable);
    assert!(report.get("patch").is_none());
    report
}

fn error(output: Output, message: &str) {
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["error"]["message"]
            .as_str()
            .unwrap()
            .contains(message),
        "{report}"
    );
    assert!(report.get("applicable").is_none());
}

#[test]
fn git_checks_context_index_conflicts_and_reverse_without_writes() {
    let source = tempfile::tempdir().unwrap();
    let receiving = tempfile::tempdir().unwrap();
    let root = receiving.path();
    plan(source.path());
    fs::write(root.join("app.rs"), ORIGINAL).unwrap();
    let args = [
        "history",
        "patch",
        "1",
        "--git-check",
        "--against",
        root.to_str().unwrap(),
    ];
    error(
        fr(source.path(), &args).output().unwrap(),
        "Git working tree",
    );
    git(root, &["init", "-q"]);
    fs::write(root.join("unrelated.txt"), "staged\n").unwrap();
    git(root, &["add", "app.rs", "unrelated.txt"]);
    fs::write(root.join("unrelated.txt"), "unstaged\n").unwrap();
    fs::write(root.join("untracked.txt"), "untracked\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let journal = fs::read(source.path().join(".fr-history/state.json")).unwrap();
    let report = checked(fr(source.path(), &args).output().unwrap(), true);
    assert_eq!(report["scope"], "worktree");
    assert_eq!(report["git_exit_code"], 0);
    assert_eq!(report["checked_files"], 1);
    assert_eq!(
        report["configuration"],
        "repository-only-without-content-filters"
    );
    let mut with_index = args.to_vec();
    with_index.push("--index");
    assert_eq!(
        checked(fr(source.path(), &with_index).output().unwrap(), true)["scope"],
        "index-and-worktree"
    );
    fs::write(
        root.join("app.rs"),
        ORIGINAL.replace("untouched", "outside_hunk_drift"),
    )
    .unwrap();
    checked(fr(source.path(), &args).output().unwrap(), true);
    checked(fr(source.path(), &with_index).output().unwrap(), false);
    fs::write(root.join("app.rs"), "conflicting source\n").unwrap();
    let conflict = checked(fr(source.path(), &args).output().unwrap(), false);
    assert!(!conflict["stderr"].as_str().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        "conflicting source\n"
    );
    fs::write(root.join("app.rs"), ORIGINAL.replace("helper", "renamed")).unwrap();
    let mut reverse = args.to_vec();
    reverse.push("--reverse");
    let reversed = checked(fr(source.path(), &reverse).output().unwrap(), true);
    assert_eq!(reversed["record_basis"], report["record_basis"]);
    assert_eq!(reversed["reverse"], true);
    reverse.push("--index");
    checked(fr(source.path(), &reverse).output().unwrap(), false);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    git(root, &["add", "app.rs"]);
    let staged_result = fs::read(root.join(".git/index")).unwrap();
    checked(fr(source.path(), &reverse).output().unwrap(), true);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), staged_result);
    assert_eq!(
        fs::read(source.path().join(".fr-history/state.json")).unwrap(),
        journal
    );
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("untracked.txt")).unwrap(),
        "untracked\n"
    );
    assert_eq!(
        fs::read_to_string(source.path().join("app.rs")).unwrap(),
        ORIGINAL
    );
}

#[test]
fn nested_receivers_use_their_paths_even_with_unusual_directory_names() {
    let source = tempfile::tempdir().unwrap();
    let repository = tempfile::Builder::new()
        .prefix("fr repo\n")
        .tempdir()
        .unwrap();
    let nested = repository.path().join("nested space\t\n\"é");
    fs::create_dir(&nested).unwrap();
    plan(source.path());
    git(repository.path(), &["init", "-q"]);
    fs::write(repository.path().join("app.rs"), "wrong outer file\n").unwrap();
    fs::write(nested.join("app.rs"), ORIGINAL).unwrap();
    let args = [
        "history",
        "patch",
        "1",
        "--git-check",
        "--against",
        nested.to_str().unwrap(),
    ];
    let report = checked(fr(source.path(), &args).output().unwrap(), true);
    assert_eq!(
        report["repository_root"],
        repository.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        report["receiving_root"],
        nested.canonicalize().unwrap().to_str().unwrap()
    );
    fs::write(repository.path().join("app.rs"), ORIGINAL).unwrap();
    fs::write(nested.join("app.rs"), "wrong nested file\n").unwrap();
    checked(fr(source.path(), &args).output().unwrap(), false);
    fs::remove_file(nested.join("app.rs")).unwrap();
    std::os::unix::fs::symlink(source.path().join("app.rs"), nested.join("app.rs")).unwrap();
    let mismatch = checked(fr(source.path(), &args).output().unwrap(), false);
    assert!(!mismatch["stderr"].as_str().unwrap().is_empty());
}

#[test]
fn content_filters_never_run_and_reserved_attribute_values_cannot_bypass_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    plan(root);
    git(root, &["init", "-q"]);
    git(
        root,
        &[
            "config",
            "filter.tripwire.clean",
            "touch fr-filter-ran; cat",
        ],
    );
    git(
        root,
        &[
            "config",
            "filter.tripwire.process",
            "touch fr-process-ran; exit 1",
        ],
    );
    git(
        root,
        &[
            "config",
            "core.fsmonitor",
            "touch fr-fsmonitor-ran; printf x",
        ],
    );
    let args = ["history", "patch", "1", "--git-check"];
    fs::write(root.join(".gitattributes"), "app.rs filter=tripwire\n").unwrap();
    error(fr(root, &args).output().unwrap(), "content filters");
    fs::write(
        root.join(".gitattributes"),
        "unrelated.txt filter=tripwire\n",
    )
    .unwrap();
    checked(fr(root, &args).output().unwrap(), true);
    for name in ["unset", "unspecified"] {
        git(
            root,
            &[
                "config",
                &format!("filter.{name}.clean"),
                "touch fr-reserved-ran; cat",
            ],
        );
        fs::write(
            root.join(".gitattributes"),
            format!("app.rs filter={name}\n"),
        )
        .unwrap();
        error(fr(root, &args).output().unwrap(), "filter drivers named");
        git(
            root,
            &["config", "--unset", &format!("filter.{name}.clean")],
        );
    }
    fs::remove_file(root.join(".gitattributes")).unwrap();
    fs::write(
        root.join(".git/info/attributes"),
        "app.rs filter=tripwire\n",
    )
    .unwrap();
    error(fr(root, &args).output().unwrap(), "content filters");
    for marker in [
        "fr-filter-ran",
        "fr-process-ran",
        "fr-fsmonitor-ran",
        "fr-reserved-ran",
    ] {
        assert!(!root.join(marker).exists(), "{marker}");
    }
}

#[test]
fn git_environment_cannot_redirect_checks_and_git_is_optional_for_other_modes() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let root = dir.path();
    plan(root);
    git(root, &["init", "-q"]);
    git(elsewhere.path(), &["init", "-q"]);
    fs::write(elsewhere.path().join("app.rs"), "wrong source\n").unwrap();
    let args = ["history", "patch", "1", "--git-check"];
    let mut command = fr(root, &args);
    command
        .env("GIT_DIR", elsewhere.path().join(".git"))
        .env("GIT_WORK_TREE", elsewhere.path())
        .env("GIT_INDEX_FILE", elsewhere.path().join(".git/index"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.worktree")
        .env("GIT_CONFIG_VALUE_0", elsewhere.path());
    checked(command.output().unwrap(), true);
    error(
        fr(root, &args)
            .env("PATH", "/nonexistent-fr-git-test")
            .output()
            .unwrap(),
        "running Git",
    );
    success(
        fr(root, &["history", "patch", "1", "--check"])
            .env("PATH", "/nonexistent-fr-git-test")
            .output()
            .unwrap(),
    );
    success(
        fr(root, &["history", "patch", "1"])
            .env("PATH", "/nonexistent-fr-git-test")
            .output()
            .unwrap(),
    );
    for flags in [vec!["--check", "--git-check"], vec!["--index"]] {
        let mut args = vec!["history", "patch", "1"];
        args.extend(flags);
        let rejected = fr(root, &args).output().unwrap();
        assert_eq!(rejected.status.code(), Some(2));
        assert!(rejected.stdout.is_empty());
    }
}

#[test]
fn linked_worktrees_and_repository_whitespace_rules_are_observed() {
    let source = tempfile::tempdir().unwrap();
    let receiving = tempfile::tempdir().unwrap();
    let root = source.path();
    fs::write(
        root.join("app.rs"),
        ORIGINAL.replace("fn helper() {}", "fn helper() {}  "),
    )
    .unwrap();
    success(
        fr(root, &["rename", "helper", "renamed", "--save-plan"])
            .output()
            .unwrap(),
    );
    git(root, &["init", "-q"]);
    git(root, &["add", "app.rs"]);
    git(
        root,
        &[
            "-c",
            "user.name=fr fixture",
            "-c",
            "user.email=fr@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
    );
    let linked = receiving.path().join("linked");
    git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    let args = [
        "history",
        "patch",
        "1",
        "--git-check",
        "--index",
        "--against",
        linked.to_str().unwrap(),
    ];
    let pointer = fs::read(linked.join(".git")).unwrap();
    let index_path =
        String::from_utf8(git(&linked, &["rev-parse", "--git-path", "index"])).unwrap();
    let index = fs::read(index_path.trim()).unwrap();
    let global_config = receiving.path().join("global.config");
    fs::write(&global_config, "[apply]\nwhitespace = error-all\n").unwrap();
    let report = checked(
        fr(root, &args)
            .env("GIT_CONFIG_GLOBAL", &global_config)
            .output()
            .unwrap(),
        true,
    );
    assert_eq!(
        report["repository_root"],
        linked.canonicalize().unwrap().to_str().unwrap()
    );
    git(root, &["config", "apply.whitespace", "error-all"]);
    checked(fr(root, &args).output().unwrap(), false);
    git(root, &["config", "apply.whitespace", "nowarn"]);
    checked(fr(root, &args).output().unwrap(), true);
    assert_eq!(fs::read(linked.join(".git")).unwrap(), pointer);
    assert_eq!(fs::read(index_path.trim()).unwrap(), index);
}
