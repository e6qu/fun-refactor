use super::injection::{hook, Event};
use super::*;
use crate::history::{
    act, lock, store_record, Action, FailureOutcome, FailurePhase, HistoryFailure, Pending,
    Snapshot, Status,
};
use std::cell::RefCell;
use std::rc::Rc;

fn regular(content: &str, mode: u32) -> Option<Snapshot> {
    Some(Snapshot {
        content: content.into(),
        mode,
        kind: SnapshotKind::Regular,
    })
}

fn link(content: &str) -> Option<Snapshot> {
    Some(Snapshot {
        content: content.into(),
        mode: 0,
        kind: SnapshotKind::Symlink,
    })
}

fn fixture(root: &Path, action: Action) -> Vec<Change> {
    let changes = [
        (
            "0.txt",
            regular("before λ\n", 0o751),
            regular("after 名\n", 0o751),
        ),
        ("1.txt", None, regular("created\n", 0o600)),
        ("2.txt", regular("remove\n", 0o640), None),
        ("3.txt", regular("", 0o640), regular("", 0o740)),
        ("4.txt", link("dangling-before"), link("dangling-after")),
    ]
    .into_iter()
    .map(|(path, before, after)| Change {
        path: path.into(),
        before,
        after,
    })
    .collect::<Vec<_>>();
    for change in &changes {
        if let Some(before) = &change.before {
            let path = root.join(&change.path);
            if before.kind == SnapshotKind::Symlink {
                symlink(&before.content, path).unwrap();
            } else {
                fs::write(&path, &before.content).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(before.mode)).unwrap();
            }
        }
    }
    let _lock = lock(root).unwrap();
    let mut history = History::read(root).unwrap();
    store_record(
        &mut history,
        changes.clone(),
        false,
        "host-fault-fixture",
        None,
    )
    .unwrap();
    drop(_lock);
    match action {
        Action::Apply => (),
        Action::Undo => {
            act(root, Action::Apply, 1, true).unwrap();
        }
        Action::Redo => {
            act(root, Action::Apply, 1, true).unwrap();
            act(root, Action::Undo, 1, true).unwrap();
        }
        Action::Recover => {
            let mut history = History::read(root).unwrap();
            history.pending = Some(Pending {
                id: 1,
                action: Action::Apply,
            });
            history.save().unwrap();
            install(root, &changes[..4]).unwrap();
        }
    }
    changes
}

fn assert_files(root: &Path, changes: &[Change], after: bool) {
    for change in changes {
        let path = root.join(&change.path);
        let expected = if after { &change.after } else { &change.before };
        match expected {
            None => assert_eq!(
                fs::symlink_metadata(&path).unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            ),
            Some(expected) if expected.kind == SnapshotKind::Symlink => {
                assert!(fs::symlink_metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_symlink());
                assert_eq!(
                    fs::read_link(&path).unwrap(),
                    std::path::PathBuf::from(&expected.content)
                );
            }
            Some(expected) => {
                let metadata = fs::symlink_metadata(&path).unwrap();
                assert!(metadata.is_file());
                assert_eq!(
                    fs::read(&path).unwrap(),
                    expected.content.as_bytes(),
                    "{}",
                    path.display()
                );
                assert_eq!(metadata.permissions().mode() & 0o7777, expected.mode);
            }
        }
    }
}

fn status_before(action: Action) -> Status {
    match action {
        Action::Undo => Status::Applied,
        Action::Redo => Status::Undone,
        _ => Status::Planned,
    }
}

fn assert_journal(history: &History, status: Status) {
    assert!(history.pending.is_none());
    assert_eq!(history.records.len(), 1);
    assert_eq!(history.record(1).unwrap().status, status);
    assert_eq!(
        history.applied,
        if status == Status::Applied {
            vec![1]
        } else {
            vec![]
        }
    );
    assert_eq!(
        history.redo,
        if status == Status::Undone {
            vec![1]
        } else {
            vec![]
        }
    );
}

fn settle(root: &Path, changes: &[Change], action: Action) {
    let mut history = History::read(root).unwrap();
    let pending = history.pending.is_some();
    if pending {
        act(root, Action::Recover, 1, true).unwrap();
        history = History::read(root).unwrap();
    }
    let completed = !pending
        && action != Action::Recover
        && history.record(1).unwrap().status != status_before(action);
    let after = matches!(
        (action, completed),
        (Action::Undo, false) | (Action::Apply | Action::Redo, true)
    );
    assert_files(root, changes, after);
    let status = if completed {
        if action == Action::Undo {
            Status::Undone
        } else {
            Status::Applied
        }
    } else {
        status_before(action)
    };
    assert_journal(&history, status);
}

fn trace(action: Action) -> Vec<Event> {
    let dir = tempfile::tempdir().unwrap();
    let changes = fixture(dir.path(), action);
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let guard = hook(move |event| {
        observed.borrow_mut().push(event);
        Ok(())
    });
    act(dir.path(), action, 1, true).unwrap();
    drop(guard);
    assert_files(
        dir.path(),
        &changes,
        matches!(action, Action::Apply | Action::Redo),
    );
    Rc::try_unwrap(events).unwrap().into_inner()
}

fn injected() -> anyhow::Error {
    std::io::Error::other("injected host boundary failure").into()
}

#[test]
fn handled_failures_cover_every_host_boundary_and_resume_from_disk() {
    let mut boundaries = 0;
    for action in [Action::Apply, Action::Undo, Action::Redo, Action::Recover] {
        let events = trace(action);
        println!("host-recovery {action:?} boundaries: {}", events.len());
        boundaries += events.len();
        for (index, expected) in events.iter().enumerate() {
            let expected_phase = if action == Action::Recover {
                FailurePhase::Recovery
            } else if index
                < events
                    .iter()
                    .position(|event| event.operation == Operation::CreateDirectory)
                    .unwrap()
            {
                FailurePhase::Preparation
            } else if index
                > events
                    .iter()
                    .rposition(|event| event.operation == Operation::PublishDirectorySync)
                    .unwrap()
            {
                FailurePhase::Finalization
            } else {
                FailurePhase::Installation
            };
            let dir = tempfile::tempdir().unwrap();
            let changes = fixture(dir.path(), action);
            let expected = expected.clone();
            let hit = Rc::new(RefCell::new(false));
            let observed = hit.clone();
            let mut visits = 0;
            let guard = hook(move |event| {
                let fail = visits == index;
                visits += 1;
                if fail {
                    assert_eq!(
                        (event.operation, event.after),
                        (expected.operation, expected.after)
                    );
                    *observed.borrow_mut() = true;
                    Err(injected())
                } else {
                    Ok(())
                }
            });
            let error = act(dir.path(), action, 1, true).unwrap_err();
            drop(guard);
            assert!(*hit.borrow(), "{action:?} boundary {index}");
            let payload = crate::cli::json_error(&error);
            assert_eq!(payload["error"]["kind"], "io");
            assert_eq!(payload["error"]["history"]["transaction"], 1);
            assert_eq!(
                payload["error"]["history"]["action"],
                serde_json::json!(action)
            );
            let report = error.downcast_ref::<HistoryFailure>().unwrap();
            assert_eq!(report.transaction, 1);
            assert_eq!(report.action, action);
            assert_eq!(report.phase, expected_phase);
            assert_eq!(
                report.outcome,
                match expected_phase {
                    FailurePhase::Preparation => FailureOutcome::Unchanged,
                    FailurePhase::Installation => FailureOutcome::RolledBack,
                    FailurePhase::Recovery => FailureOutcome::RecoveryIncomplete,
                    FailurePhase::Finalization => FailureOutcome::FinalizationUncertain,
                }
            );
            assert_eq!(
                report.journal_pending,
                Some(History::read(dir.path()).unwrap().pending.is_some())
            );
            if report.outcome == FailureOutcome::RolledBack {
                assert_files(dir.path(), &changes, action == Action::Undo);
            }
            settle(dir.path(), &changes, action);
        }
    }
    println!("host-recovery handled boundaries: {boundaries}");
}

#[test]
fn process_exits_cover_every_host_boundary_and_resume_from_disk() {
    const CHILD: &str = "FR_HOST_FAULT_ROOT";
    if let Ok(root) = std::env::var(CHILD) {
        let index: usize = std::env::var("FR_HOST_FAULT_INDEX")
            .unwrap()
            .parse()
            .unwrap();
        let action: Action =
            serde_json::from_str(&std::env::var("FR_HOST_FAULT_ACTION").unwrap()).unwrap();
        let mut visits = 0;
        let _guard = hook(move |_| {
            if visits == index {
                std::process::exit(77);
            }
            visits += 1;
            Ok(())
        });
        act(Path::new(&root), action, 1, true).unwrap();
        panic!("process interruption did not run");
    }
    let mut boundaries = 0;
    for action in [Action::Apply, Action::Undo, Action::Redo, Action::Recover] {
        let events = trace(action);
        println!("host-recovery {action:?} boundaries: {}", events.len());
        boundaries += events.len();
        for index in 0..events.len() {
            let dir = tempfile::tempdir().unwrap();
            let changes = fixture(dir.path(), action);
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "history::host::tests::process_exits_cover_every_host_boundary_and_resume_from_disk", "--nocapture"])
                .env(CHILD, dir.path()).env("FR_HOST_FAULT_INDEX", index.to_string())
                .env("FR_HOST_FAULT_ACTION", serde_json::to_string(&action).unwrap()).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(77),
                "{action:?} {index}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            settle(dir.path(), &changes, action);
        }
    }
    println!("host-recovery process-exit boundaries: {boundaries}");
}

#[test]
fn altered_staged_bytes_modes_and_kinds_never_reach_targets() {
    for alteration in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let changes = fixture(dir.path(), Action::Apply);
        let mut changed = false;
        let guard = hook(move |event| {
            if !changed && event.operation == Operation::CheckStage && !event.after {
                changed = true;
                match alteration {
                    0 => fs::write(&event.path, "unreviewed data")?,
                    1 => fs::set_permissions(&event.path, fs::Permissions::from_mode(0o600))?,
                    _ => {
                        fs::remove_file(&event.path)?;
                        symlink("foreign-target", &event.path)?;
                    }
                }
            }
            Ok(())
        });
        let error = act(dir.path(), Action::Apply, 1, true).unwrap_err();
        drop(guard);
        assert_eq!(
            error.downcast_ref::<HistoryFailure>().unwrap().outcome,
            FailureOutcome::RolledBack
        );
        assert_files(dir.path(), &changes, false);
    }
}

#[test]
fn recovery_rechecks_paths_already_restored_before_clearing_pending() {
    let dir = tempfile::tempdir().unwrap();
    let changes = fixture(dir.path(), Action::Recover);
    let path = dir.path().join("0.txt");
    let changed_path = path.clone();
    let guard = hook(move |event| {
        if event.operation == Operation::RecoveryCheck && !event.after {
            fs::write(&changed_path, "user edit")?;
        }
        Ok(())
    });
    let error = act(dir.path(), Action::Recover, 1, true).unwrap_err();
    drop(guard);
    assert_eq!(
        error.downcast_ref::<HistoryFailure>().unwrap().outcome,
        FailureOutcome::RecoveryIncomplete
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "user edit");
    assert!(History::read(dir.path()).unwrap().pending.is_some());
    fs::write(&path, &changes[0].before.as_ref().unwrap().content).unwrap();
    act(dir.path(), Action::Recover, 1, true).unwrap();
    assert_files(dir.path(), &changes, false);
}

#[test]
fn rollback_failure_retains_both_errors_and_can_resume() {
    let dir = tempfile::tempdir().unwrap();
    let changes = fixture(dir.path(), Action::Apply);
    let mut publication_failed = false;
    let mut recovery_failed = false;
    let guard = hook(move |event| {
        if !publication_failed && event.operation == Operation::Publish && event.after {
            publication_failed = true;
            return Err(injected());
        }
        if publication_failed && !recovery_failed && event.operation == Operation::StageWrite {
            recovery_failed = true;
            return Err(std::io::Error::other("injected rollback failure").into());
        }
        Ok(())
    });
    let error = act(dir.path(), Action::Apply, 1, true).unwrap_err();
    drop(guard);
    let report = error.downcast_ref::<HistoryFailure>().unwrap();
    assert_eq!(report.phase, FailurePhase::Installation);
    assert_eq!(report.outcome, FailureOutcome::RecoveryIncomplete);
    assert_eq!(report.journal_pending, Some(true));
    assert!(report
        .recovery_error
        .as_ref()
        .unwrap()
        .contains("injected rollback failure"));
    assert!(format!("{error:#}").contains("injected host boundary failure"));
    settle(dir.path(), &changes, Action::Apply);
}

#[test]
fn already_restored_recovery_syncs_parents_before_clearing_pending() {
    let dir = tempfile::tempdir().unwrap();
    let changes = fixture(dir.path(), Action::Apply);
    let mut history = History::read(dir.path()).unwrap();
    history.pending = Some(Pending {
        id: 1,
        action: Action::Apply,
    });
    history.save().unwrap();
    let guard = hook(|event| {
        if event.operation == Operation::SyncAncestor {
            return Err(injected());
        }
        assert!(!matches!(
            event.operation,
            Operation::Publish | Operation::Remove
        ));
        Ok(())
    });
    let error = act(dir.path(), Action::Recover, 1, true).unwrap_err();
    drop(guard);
    assert_eq!(
        error
            .downcast_ref::<HistoryFailure>()
            .unwrap()
            .journal_pending,
        Some(true)
    );
    assert_files(dir.path(), &changes, false);
    settle(dir.path(), &changes, Action::Recover);
}

#[test]
fn altered_staged_symlink_refuses_and_rolls_back_prior_writes() {
    let dir = tempfile::tempdir().unwrap();
    let changes = fixture(dir.path(), Action::Apply);
    let guard = hook(|event| {
        if event.operation == Operation::StageSymlink && event.after {
            fs::remove_file(&event.path)?;
            symlink("changed-target", &event.path)?;
        }
        Ok(())
    });
    let error = act(dir.path(), Action::Apply, 1, true).unwrap_err();
    drop(guard);
    assert_eq!(
        error.downcast_ref::<HistoryFailure>().unwrap().outcome,
        FailureOutcome::RolledBack
    );
    assert_files(dir.path(), &changes, false);
    assert_journal(&History::read(dir.path()).unwrap(), Status::Planned);
}
