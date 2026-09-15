use super::resume::{record, COMPLETE};
use super::{absent, branch, configuration, ownership};
use crate::git::worktree::{RedoRemovalOptions, RemoveOptions, UndoRemovalOptions};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Restoring {
    schema: u32,
    state: String,
    record_digest: String,
    destination: PathBuf,
    branch: String,
    commit: String,
    basis: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Restored {
    schema: u32,
    state: String,
    record_digest: String,
    destination: PathBuf,
    branch: String,
    commit: String,
    ownership_record: PathBuf,
}

fn marker_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn marker(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    if absent(path)? {
        return Ok(None);
    }
    Ok(Some(ownership::bytes(path, limit)?))
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}

fn complete(loaded: &record::Loaded) -> Result<Vec<u8>> {
    let path = loaded.path.with_file_name("complete");
    let bytes = marker(&path, 1024)?.context("completed removal marker is missing.")?;
    ensure!(bytes == COMPLETE, "invalid removal completion marker.");
    Ok(bytes)
}

fn removed_plan(loaded: &record::Loaded) -> Result<super::super::Proposal> {
    loaded.check()?;
    complete(loaded)?;
    configuration(&loaded.root, loaded.plan.worktree_config)?;
    branch::check(&loaded.root, &loaded.plan.branch, &loaded.plan.commit)?;
    let checkout_absent = absent(&loaded.plan.destination)?;
    let metadata_absent = absent(&loaded.trees[1].root)?;
    let registrations = super::super::checked(
        &loaded.root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?;
    let entries = super::super::records::parse(&registrations, &loaded.root)?;
    let unoccupied = entries.iter().all(|entry| {
        let path = Path::new(&entry.path);
        entry.branch.as_deref() != Some(&format!("refs/heads/{}", loaded.plan.branch))
            && !loaded.plan.destination.starts_with(path)
            && !path.starts_with(&loaded.plan.destination)
    });
    ensure!(
        crate::git::worktree_removal_reversal_allowed(
            true,
            checkout_absent,
            metadata_absent,
            unoccupied
        ),
        "removed worktree endpoints or branch registration are occupied."
    );
    let mut plan = loaded.plan.clone();
    plan.existing_branch = true;
    plan.from = format!("refs/heads/{}", plan.branch);
    plan.registrations = ownership::digest(&registrations);
    Ok(plan)
}

fn undo_basis(loaded: &record::Loaded, plan: &super::super::Proposal) -> Result<String> {
    Ok(format!(
        "frwtdu1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &loaded.root,
            &loaded.path,
            loaded.archive_identity,
            &loaded.raw,
            plan
        ))?)
    ))
}

fn restoring(loaded: &record::Loaded, bytes: &[u8]) -> Result<Restoring> {
    let value: Restoring =
        serde_json::from_slice(bytes).context("invalid restoration preparation marker.")?;
    ensure!(
        value.schema == 1
            && value.state == "restoring"
            && value.record_digest == loaded.raw.digest
            && value.destination == loaded.plan.destination
            && value.branch == loaded.plan.branch
            && value.commit == loaded.plan.commit,
        "restoration preparation does not match the removal archive."
    );
    Ok(value)
}

fn completed_restoration(
    loaded: &record::Loaded,
    preparation_bytes: &[u8],
) -> Result<(Restored, String)> {
    restoring(loaded, preparation_bytes)?;
    let capture = super::super::recovery::capture(&loaded.root, &loaded.plan.destination, true)?;
    ensure!(
        capture.plan.destination == loaded.plan.destination
            && capture.plan.branch == loaded.plan.branch
            && capture.plan.commit == loaded.plan.commit
            && capture.plan.tree == loaded.plan.tree
            && capture.plan.files == loaded.plan.files,
        "recovered worktree differs from the archived committed state."
    );
    let snapshot = super::observe(&capture, &[])?;
    let observation = super::basis(&capture, &snapshot)?;
    let restored = Restored {
        schema: 1,
        state: "restored".to_owned(),
        record_digest: loaded.raw.digest.clone(),
        destination: loaded.plan.destination.clone(),
        branch: loaded.plan.branch.clone(),
        commit: loaded.plan.commit.clone(),
        ownership_record: capture.receipt.path(),
    };
    let basis = format!(
        "frwtduf1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &loaded.raw,
            preparation_bytes,
            observation
        ))?)
    );
    Ok((restored, basis))
}

fn confirm_restoration(
    loaded: &record::Loaded,
    options: &UndoRemovalOptions,
    preparation_bytes: Vec<u8>,
) -> Result<Value> {
    let (restored, basis) = completed_restoration(loaded, &preparation_bytes)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale worktree restoration confirmation basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1,"operation":if options.write {"worktree-removal-undo-confirm"} else {"worktree-removal-undo-confirm-preview"},
        "applied":false,"basis":basis,"basis_verified":options.basis.is_some(),"repository_root":loaded.root,
        "removal_record":loaded.path,"destination":loaded.plan.destination,
        "branch":format!("refs/heads/{}",loaded.plan.branch),"commit":loaded.plan.commit,"tree":loaded.plan.tree,
        "ownership_record":restored.ownership_record,"state":"restoration-ready-to-confirm"});
    if !options.write {
        return Ok(result);
    }
    let archive = ownership::Lease::acquire(loaded.path.with_file_name("resume.lock"))?;
    loaded.check()?;
    let current = marker(&loaded.path.with_file_name("restoring.json"), 64 * 1024)?
        .context("restoration preparation marker disappeared.")?;
    ensure!(
        current == preparation_bytes,
        "restoration preparation marker changed."
    );
    let (current_restored, current_basis) = completed_restoration(loaded, &current)?;
    ensure!(
        current_basis == basis,
        "worktree restoration changed before confirmation."
    );
    archive.check()?;
    publish(
        &loaded.path.with_file_name("restored.json"),
        &marker_bytes(&current_restored)?,
    )?;
    let restoring_path = loaded.path.with_file_name("restoring.json");
    fs::remove_file(&restoring_path)?;
    File::open(restoring_path.parent().unwrap())?.sync_all()?;
    result["applied"] = json!(true);
    result["state"] = json!("restored");
    Ok(result)
}

pub(in crate::git::worktree) fn undo(root: &Path, options: &UndoRemovalOptions) -> Result<Value> {
    let loaded = record::Loaded::read(root, &options.record)?;
    ensure!(
        marker(&loaded.path.with_file_name("restored.json"), 64 * 1024)?.is_none(),
        "worktree removal is already restored."
    );
    if let Some(preparation) = marker(&loaded.path.with_file_name("restoring.json"), 64 * 1024)? {
        return confirm_restoration(&loaded, options, preparation);
    }
    let plan = removed_plan(&loaded)?;
    let basis = undo_basis(&loaded, &plan)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale worktree removal undo basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1,"operation":if options.write {"worktree-removal-undo"} else {"worktree-removal-undo-preview"},
        "applied":false,"basis":basis,"basis_verified":options.basis.is_some(),"repository_root":loaded.root,
        "removal_record":loaded.path,"destination":plan.destination,"branch":format!("refs/heads/{}",plan.branch),
        "commit":plan.commit,"tree":plan.tree,"branch_action":"attach-existing","checkout":"raw-blobs"});
    if !options.write {
        return Ok(result);
    }
    let archive = ownership::Lease::acquire(loaded.path.with_file_name("resume.lock"))?;
    let checked_plan = removed_plan(&loaded)?;
    ensure!(
        undo_basis(&loaded, &checked_plan)? == basis,
        "worktree removal undo basis changed before writing."
    );
    let restoring = Restoring {
        schema: 1,
        state: "restoring".to_owned(),
        record_digest: loaded.raw.digest.clone(),
        destination: plan.destination.clone(),
        branch: plan.branch.clone(),
        commit: plan.commit.clone(),
        basis: basis.clone(),
    };
    let restoring_path = loaded.path.with_file_name("restoring.json");
    let restoring_bytes = marker_bytes(&restoring)?;
    match marker(&restoring_path, 64 * 1024)? {
        Some(current) => ensure!(current == restoring_bytes, "restoration marker differs."),
        None => publish(&restoring_path, &restoring_bytes)?,
    }
    let blobs = super::super::checkout::blobs(&plan)?;
    let branch_lease = branch::Lease::acquire(&plan.root, &plan.branch, &plan.commit)?;
    archive.check()?;
    let outcome = super::super::checkout::apply(&plan, &blobs);
    drop(branch_lease);
    match outcome {
        Ok(ownership_record) => {
            let restored = Restored {
                schema: 1,
                state: "restored".to_owned(),
                record_digest: loaded.raw.digest.clone(),
                destination: plan.destination.clone(),
                branch: plan.branch.clone(),
                commit: plan.commit.clone(),
                ownership_record: ownership_record.clone(),
            };
            publish(
                &loaded.path.with_file_name("restored.json"),
                &marker_bytes(&restored)?,
            )?;
            fs::remove_file(&restoring_path)?;
            File::open(restoring_path.parent().unwrap())?.sync_all()?;
            result["applied"] = json!(true);
            result["ownership_record"] = json!(ownership_record);
            result["state"] = json!("restored");
        }
        Err(error) => {
            result["applied"] = Value::Null;
            result["state"] = json!("restoration-incomplete");
            result["warning"] = json!("Restoration is incomplete or unconfirmed. Inspect the destination and use worktree recover when an ownership or preparation record exists.");
            let diagnostic = format!("{error:#}");
            result["diagnostic"] = json!(crate::git::process::diagnostic(diagnostic.as_bytes()));
            result["diagnostic_truncated"] = json!(diagnostic.len() > 16 * 1024);
        }
    }
    Ok(result)
}

fn restored(loaded: &record::Loaded) -> Result<(Restored, Vec<u8>)> {
    loaded.check()?;
    complete(loaded)?;
    ensure!(
        marker(&loaded.path.with_file_name("redone.json"), 64 * 1024)?.is_none(),
        "worktree removal was already repeated."
    );
    ensure!(
        marker(&loaded.path.with_file_name("restoring.json"), 64 * 1024)?.is_none(),
        "worktree restoration remains incomplete."
    );
    let path = loaded.path.with_file_name("restored.json");
    let bytes = marker(&path, 64 * 1024)?.context("worktree removal has not been restored.")?;
    let value: Restored = serde_json::from_slice(&bytes).context("invalid restoration marker.")?;
    ensure!(
        value.schema == 1
            && value.state == "restored"
            && value.record_digest == loaded.raw.digest
            && value.destination == loaded.plan.destination
            && value.branch == loaded.plan.branch
            && value.commit == loaded.plan.commit,
        "restoration marker does not match the removal archive."
    );
    Ok((value, bytes))
}

fn redo_preview(loaded: &record::Loaded, restored: &Restored) -> Result<Value> {
    let capture = super::super::recovery::capture(&loaded.root, &loaded.plan.destination, true)?;
    ensure!(
        capture.receipt.path() == restored.ownership_record
            && capture.plan.destination == loaded.plan.destination
            && capture.plan.branch == loaded.plan.branch
            && capture.plan.commit == loaded.plan.commit
            && capture.plan.tree == loaded.plan.tree
            && capture.plan.files == loaded.plan.files,
        "restored worktree differs from the archived committed state."
    );
    super::report(
        &loaded.root,
        &RemoveOptions {
            path: loaded.plan.destination.clone(),
            basis: None,
            write: false,
            limit: 1,
        },
    )
}

pub(in crate::git::worktree) fn redo(root: &Path, options: &RedoRemovalOptions) -> Result<Value> {
    let loaded = record::Loaded::read(root, &options.record)?;
    let (restored_state, restored_bytes) = restored(&loaded)?;
    let removal = redo_preview(&loaded, &restored_state)?;
    let removal_basis = removal["basis"]
        .as_str()
        .context("missing removal basis.")?;
    let basis = format!(
        "frwtdx1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &loaded.raw,
            &restored_bytes,
            removal_basis
        ))?)
    );
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == basis,
            "stale worktree removal redo basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1,"operation":if options.write {"worktree-removal-redo"} else {"worktree-removal-redo-preview"},
        "applied":false,"basis":basis,"basis_verified":options.basis.is_some(),"repository_root":loaded.root,
        "source_removal_record":loaded.path,"destination":loaded.plan.destination,
        "branch":format!("refs/heads/{}",loaded.plan.branch),"commit":loaded.plan.commit,"tree":loaded.plan.tree,
        "branch_action":"retain"});
    if !options.write {
        return Ok(result);
    }
    let archive = ownership::Lease::acquire(loaded.path.with_file_name("resume.lock"))?;
    let (current_restored, current_bytes) = restored(&loaded)?;
    ensure!(
        current_bytes == restored_bytes,
        "restoration marker changed before repeated removal."
    );
    let current = redo_preview(&loaded, &current_restored)?;
    ensure!(
        current["basis"].as_str() == Some(removal_basis),
        "worktree removal redo basis changed before writing."
    );
    archive.check()?;
    let removed = super::report(
        &loaded.root,
        &RemoveOptions {
            path: loaded.plan.destination.clone(),
            basis: Some(removal_basis.to_owned()),
            write: true,
            limit: 1,
        },
    )?;
    result["applied"] = removed["applied"].clone();
    if removed["applied"] == true {
        let next = removed["removal_record"]
            .as_str()
            .context("repeated removal omitted its archive.")?;
        let marker = json!({"schema":1,"state":"redone","record_digest":loaded.raw.digest,
            "restored_digest":ownership::digest(&restored_bytes),"next_removal_record":next});
        publish(
            &loaded.path.with_file_name("redone.json"),
            &marker_bytes(&marker)?,
        )?;
        result["state"] = json!("removed");
        result["next_removal_record"] = json!(next);
    } else {
        result["state"] = json!("removal-incomplete");
        for name in [
            "warning",
            "diagnostic",
            "diagnostic_truncated",
            "removal_record",
        ] {
            if let Some(value) = removed.get(name) {
                result[name] = value.clone();
            }
        }
    }
    Ok(result)
}
