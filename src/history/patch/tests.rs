use super::*;
use crate::history::{basis, Change, Record, SnapshotKind};
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::process::{Command, Output, Stdio};

fn snapshot(content: &str, mode: u32) -> Option<Snapshot> {
    Some(Snapshot {
        content: content.to_owned(),
        mode,
        kind: SnapshotKind::Regular,
    })
}

fn link(target: &str) -> Option<Snapshot> {
    Some(Snapshot {
        content: target.to_owned(),
        mode: 0,
        kind: SnapshotKind::Symlink,
    })
}

fn record(changes: Vec<Change>) -> Record {
    Record {
        id: 1,
        status: Status::Planned,
        basis: basis(&changes).unwrap(),
        source_revision: "test-revision".into(),
        validation: "test-snapshots".into(),
        changes,
        compacted_paths: 0,
        compaction_digest: None,
        required_checks: None,
        check_evidence: Vec::new(),
    }
}

fn git(root: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "core.autocrlf=false", "-c", "core.fileMode=true"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn git_ok(root: &Path, args: &[&str], input: &str) -> Vec<u8> {
    let output = git(root, args, input);
    assert!(
        output.status.success(),
        "{args:?}: {}\n{input}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn assert_snapshot(root: &Path, change: &Change, reverse: bool) {
    let path = root.join(&change.path);
    match if reverse {
        &change.before
    } else {
        &change.after
    } {
        Some(expected) => {
            let metadata = fs::symlink_metadata(&path).unwrap();
            if expected.kind == SnapshotKind::Symlink {
                assert!(metadata.file_type().is_symlink(), "{}", path.display());
                assert_eq!(
                    fs::read_link(&path).unwrap().as_os_str().as_bytes(),
                    expected.content.as_bytes()
                );
            } else {
                assert!(metadata.is_file(), "{}", path.display());
                assert_eq!(fs::read(&path).unwrap(), expected.content.as_bytes());
                assert_eq!(
                    metadata.permissions().mode() & 0o100,
                    expected.mode & 0o100,
                    "{}",
                    path.display()
                );
            }
        }
        None => assert_eq!(
            fs::symlink_metadata(&path).unwrap_err().kind(),
            std::io::ErrorKind::NotFound,
            "{}",
            path.display()
        ),
    }
}

#[test]
fn git_round_trips_text_paths_empty_files_modes_and_moves() {
    let name = "odd space\t\n\r\"\\é🦀.txt";
    let tuples = [
        (
            "nested/add.txt",
            None,
            snapshot("created without newline", 0o600),
        ),
        ("remove.txt", snapshot("deleted\n", 0o644), None),
        ("empty-add", None, snapshot("", 0o600)),
        ("empty-remove", snapshot("", 0o644), None),
        ("empty-executable", None, snapshot("", 0o700)),
        (
            "mode-only",
            snapshot("same\n", 0o644),
            snapshot("same\n", 0o755),
        ),
        ("empty-mode", snapshot("", 0o755), snapshot("", 0o644)),
        (
            "mode-and-text",
            snapshot("old\n", 0o755),
            snapshot("new\n", 0o644),
        ),
        (
            name,
            snapshot("α\r\nβ\r\nlast", 0o644),
            snapshot("α\r\nγ\r\nlast\r\n", 0o644),
        ),
        (
            "truncate",
            snapshot("no newline", 0o644),
            snapshot("", 0o644),
        ),
        ("fill", snapshot("", 0o644), snapshot("one\ntwo", 0o644)),
        (
            "newline",
            snapshot("line", 0o644),
            snapshot("line\n", 0o644),
        ),
        (
            "multi-hunk",
            snapshot("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n", 0o644),
            snapshot("new\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\nend\n", 0o644),
        ),
        ("move-from", snapshot("moved\n", 0o755), None),
        ("move-to", None, snapshot("moved\n", 0o755)),
        ("swap-a", snapshot("a\n", 0o644), snapshot("b\n", 0o644)),
        ("swap-b", snapshot("b\n", 0o644), snapshot("a\n", 0o644)),
        ("link-add", None, link("missing-add")),
        ("link-remove", link("missing-remove"), None),
        ("link-change", link("old-target"), link("new-target")),
        (
            "file-to-link",
            snapshot("old file\n", 0o644),
            link("target"),
        ),
        (
            "link-to-file",
            link("old-target"),
            snapshot("new file\n", 0o755),
        ),
        (
            "syntax.txt",
            snapshot("--- before\n+++ after\n@@ hunk\n", 0o644),
            snapshot("diff --git a/b b/b\n\\ No newline at end of file\n", 0o644),
        ),
    ];
    let record = record(
        tuples
            .into_iter()
            .map(|(path, before, after)| Change {
                path: path.into(),
                before,
                after,
            })
            .collect(),
    );
    let forward = render(&record.changes, false).unwrap();
    let reverse = render(&record.changes, true).unwrap();
    assert!(forward.contains("\\ No newline at end of file\n"));
    let mut reordered = record.clone();
    reordered.changes.reverse();
    assert_eq!(forward, render(&reordered.changes, false).unwrap());
    let stored = tempfile::tempdir().unwrap();
    let mut history = History::read(stored.path()).unwrap();
    history.records.push(record.clone());
    fs::create_dir(stored.path().join(".fr-history")).unwrap();
    history.save().unwrap();
    for exported_reverse in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_ok(root, &["init", "-q"], "");
        for change in &record.changes {
            if let Some(before) = &change.before {
                let path = root.join(&change.path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                if before.kind == SnapshotKind::Symlink {
                    symlink(&before.content, &path).unwrap();
                } else {
                    fs::write(&path, &before.content).unwrap();
                    fs::set_permissions(
                        &path,
                        fs::Permissions::from_mode(git_mode(before.mode) & 0o777),
                    )
                    .unwrap();
                }
            }
        }
        git_ok(root, &["add", "."], "");
        let index = fs::read(root.join(".git/index")).unwrap();
        let before = check_patch_basis(stored.path(), &[1], false, Some(root)).unwrap();
        let git_check = check_git_patch(stored.path(), &[1], false, Some(root), false).unwrap();
        assert!(git_check.applicable, "{}", git_check.stderr);
        assert!(
            check_git_patch(stored.path(), &[1], false, Some(root), true)
                .unwrap()
                .applicable
        );
        assert!(before.matches_patch_basis);
        assert!(before.matches_recorded_snapshots);
        assert_eq!(before.checked_files, record.changes.len());
        assert!(before
            .files
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path));
        fs::write(root.join("empty-add"), "collision").unwrap();
        let collision = check_patch_basis(stored.path(), &[1], false, Some(root)).unwrap();
        assert!(!collision.matches_patch_basis);
        let row = collision
            .files
            .iter()
            .find(|file| file.path == Path::new("empty-add"))
            .unwrap();
        assert!(!row.expected_exists);
        assert!(row.actual_exists);
        fs::remove_file(root.join("empty-add")).unwrap();
        git_ok(root, &["apply", "--check", "-"], &forward);
        git_ok(root, &["apply", "--whitespace=nowarn", "-"], &forward);
        for change in &record.changes {
            assert_snapshot(root, change, false);
        }
        assert!(
            !check_patch_basis(stored.path(), &[1], false, Some(root))
                .unwrap()
                .matches_patch_basis
        );
        let after = check_patch_basis(stored.path(), &[1], true, Some(root)).unwrap();
        assert!(
            check_git_patch(stored.path(), &[1], true, Some(root), false)
                .unwrap()
                .applicable
        );
        assert!(
            !check_git_patch(stored.path(), &[1], true, Some(root), true)
                .unwrap()
                .applicable
        );
        assert!(after.matches_patch_basis);
        assert!(!after.matches_recorded_snapshots);
        let (args, patch) = if exported_reverse {
            (vec!["apply", "--check", "-"], &reverse)
        } else {
            (vec!["apply", "--check", "--reverse", "-"], &forward)
        };
        git_ok(root, &args, patch);
        let args = if exported_reverse {
            vec!["apply", "--whitespace=nowarn", "-"]
        } else {
            vec!["apply", "--whitespace=nowarn", "--reverse", "-"]
        };
        git_ok(root, &args, patch);
        for change in &record.changes {
            assert_snapshot(root, change, true);
        }
        assert!(
            check_patch_basis(stored.path(), &[1], false, Some(root))
                .unwrap()
                .matches_patch_basis
        );
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    }
}

#[test]
fn export_reads_only_journal_snapshots_and_retains_record_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let record = record(vec![Change {
        path: "app.rs".into(),
        before: snapshot("fn old() {}\n", 0o644),
        after: snapshot("fn new() {}\n", 0o644),
    }]);
    let mut history = History::read(root).unwrap();
    history.records.push(record.clone());
    fs::create_dir(root.join(".fr-history")).unwrap();
    history.save().unwrap();
    assert!(!fs::read_to_string(root.join(".fr-history/state.json"))
        .unwrap()
        .contains("\"kind\""));
    History::read(root).unwrap();
    let forward = export_patch(root, &[1], false).unwrap();
    let reverse = export_patch(root, &[1], true).unwrap();
    assert_eq!(forward.record_basis, record.basis);
    assert_eq!(reverse.record_basis, record.basis);
    assert_eq!(forward.files, 1);
    assert!(reverse.reverse);
    assert_eq!(
        forward.transactions[0].source_revision,
        record.source_revision
    );
    assert_eq!(forward.transactions[0].validation, record.validation);
    let journal = fs::read(root.join(".fr-history/state.json")).unwrap();
    fs::write(root.join("app.rs"), "unrelated drift").unwrap();
    assert_eq!(
        forward.patch,
        export_patch(root, &[1], false).unwrap().patch
    );
    assert_eq!(
        fs::read(root.join(".fr-history/state.json")).unwrap(),
        journal
    );
    for status in [Status::Applied, Status::Undone, Status::Abandoned] {
        history.records[0].status = status;
        history.applied = if status == Status::Applied {
            vec![1]
        } else {
            vec![]
        };
        history.redo = if status == Status::Undone {
            vec![1]
        } else {
            vec![]
        };
        history.save().unwrap();
        let export = export_patch(root, &[1], false).unwrap();
        assert_eq!(export.transactions[0].status, status);
        assert_eq!(export.patch, forward.patch);
    }
    assert!(export_patch(root, &[2], false).is_err());
}

#[test]
fn selected_edits_produce_one_patch_and_its_exact_inverse() {
    let stored = tempfile::tempdir().unwrap();
    let receiving = tempfile::tempdir().unwrap();
    let first = record(vec![
        Change {
            path: "proof.lean".into(),
            before: snapshot("first sorry\nsecond sorry\n", 0o644),
            after: snapshot("first done\nsecond sorry\n", 0o644),
        },
        Change {
            path: "scratch".into(),
            before: None,
            after: snapshot("temporary\n", 0o644),
        },
    ]);
    let mut second = record(vec![
        Change {
            path: "proof.lean".into(),
            before: snapshot("first done\nsecond sorry\n", 0o644),
            after: snapshot("first done\nsecond done\n", 0o644),
        },
        Change {
            path: "scratch".into(),
            before: snapshot("temporary\n", 0o644),
            after: None,
        },
        Change {
            path: "run.sh".into(),
            before: snapshot("echo checked\n", 0o644),
            after: snapshot("echo checked\n", 0o755),
        },
    ]);
    second.id = 2;
    let mut history = History::read(stored.path()).unwrap();
    history.records = vec![first, second];
    let selected = select(&history, &[1, 2]).unwrap();
    assert_eq!(selected.changes.len(), 2);
    assert_eq!(
        selected
            .transactions
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let forward = render(&selected.changes, false).unwrap();
    let reverse = render(&selected.changes, true).unwrap();
    assert!(!forward.contains("scratch"));
    assert_eq!(
        forward
            .lines()
            .filter(|line| line.starts_with("diff --git "))
            .count(),
        2
    );
    let root = receiving.path();
    git_ok(root, &["init", "-q"], "");
    fs::write(root.join("proof.lean"), "first sorry\nsecond sorry\n").unwrap();
    fs::write(root.join("run.sh"), "echo checked\n").unwrap();
    fs::set_permissions(root.join("run.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    git_ok(root, &["apply", "--check", "-"], &forward);
    git_ok(root, &["apply", "-"], &forward);
    for change in &selected.changes {
        assert_snapshot(root, change, false);
    }
    git_ok(root, &["apply", "--check", "-"], &reverse);
    git_ok(root, &["apply", "-"], &reverse);
    for change in &selected.changes {
        assert_snapshot(root, change, true);
    }
}

#[test]
fn selected_edits_refuse_conflicts_duplicates_missing_payloads_and_no_net_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut history = History::read(dir.path()).unwrap();
    for (index, (before, after)) in [("a", "b"), ("b", "c"), ("c", "a"), ("wrong", "d")]
        .into_iter()
        .enumerate()
    {
        let mut entry = record(vec![Change {
            path: "file".into(),
            before: snapshot(before, 0o644),
            after: snapshot(after, 0o644),
        }]);
        entry.id = index as u64 + 1;
        history.records.push(entry);
    }
    for (ids, message) in [
        (vec![], "at least one"),
        (vec![1, 1], "more than once"),
        (vec![2, 1], "does not follow"),
        (vec![1, 3], "does not follow"),
        (vec![1, 4], "does not follow"),
        (vec![1, 2, 3], "no net change"),
    ] {
        let error = select(&history, &ids).err().unwrap().to_string();
        assert!(error.contains(message), "{ids:?}: {error}");
    }
    assert!(select(&history, &[99]).is_err());
    for before in [link("b"), snapshot("b", 0o755), snapshot("b", 0o600)] {
        history.records[1].changes[0].before = before;
        history.records[1].basis = basis(&history.records[1].changes).unwrap();
        assert!(select(&history, &[1, 2])
            .err()
            .unwrap()
            .to_string()
            .contains("does not follow"));
    }
    history.records[1].changes.clear();
    assert!(select(&history, &[1, 2])
        .err()
        .unwrap()
        .to_string()
        .contains("compacted"));
}

#[test]
fn export_refuses_binary_and_unrepresentable_permissions() {
    for (before, after, reason) in [
        (
            snapshot("text\n", 0o644),
            snapshot("nul\0data", 0o644),
            "binary",
        ),
        (snapshot("nul\0data", 0o644), None, "binary"),
        (
            snapshot("text\n", 0o644),
            snapshot("text\n", 0o600),
            "permission",
        ),
        (
            snapshot("old\n", 0o644),
            snapshot("new\n", 0o664),
            "permission",
        ),
        (
            snapshot("text\n", 0o644),
            snapshot("text\n", 0o654),
            "permission",
        ),
    ] {
        let record = record(vec![Change {
            path: "app.rs".into(),
            before,
            after,
        }]);
        for reverse in [false, true] {
            let error = render(&record.changes, reverse).unwrap_err().to_string();
            assert!(error.contains(reason), "{error}");
        }
    }
}
