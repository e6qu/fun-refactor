use super::super::{absent, ownership};
use super::{resume, FileState};
use crate::git::worktree::CompactRemovalOptions;
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

mod summary;
use summary::{Fingerprint, Summary};

struct Capture {
    root: PathBuf,
    path: PathBuf,
    data: Summary,
    record: Option<FileState>,
    summary: Option<FileState>,
    marker: Option<FileState>,
}

#[derive(Serialize)]
struct Observation {
    destination_absent: bool,
    metadata_absent: bool,
    complete: bool,
    unlocked: bool,
}

impl Observation {
    fn allowed(&self) -> bool {
        crate::git::worktree_archive_compaction_allowed(
            self.complete,
            self.destination_absent,
            self.metadata_absent,
            self.unlocked,
        )
    }
}

fn optional(path: &Path, limit: u64) -> Result<Option<FileState>> {
    if absent(path)? {
        Ok(None)
    } else {
        FileState::read_limited(path, limit).map(Some)
    }
}

fn capture(root: &Path, path: &Path) -> Result<Capture> {
    let (root, common, path) = resume::record::location(root, path)?;
    let marker = optional(&path.with_file_name("complete"), 256)?;
    let summary = optional(&path.with_file_name("summary.json"), 64 * 1024)?;
    let mut data = summary
        .as_ref()
        .map(|file| serde_json::from_slice::<Summary>(&file.bytes))
        .transpose()?;
    if let Some(data) = &data {
        data.validate(&root, &common, &path, marker.as_ref())?;
    }
    let record = if absent(&path)? {
        None
    } else {
        let loaded = resume::record::Loaded::read(&root, &path)?;
        loaded.check()?;
        let expected = Summary::from_record(&loaded, marker.as_ref());
        if let Some(data) = &data {
            ensure!(
                *data == expected,
                "full removal record differs from its compaction summary."
            );
        }
        data = Some(expected);
        Some(loaded.raw)
    };
    let data =
        data.ok_or_else(|| anyhow::anyhow!("no full removal record or compaction summary."))?;
    Ok(Capture {
        root,
        path,
        data,
        record,
        summary,
        marker,
    })
}

fn observe(capture: &Capture, lease: Option<&ownership::Lease>) -> Result<Observation> {
    let (_, common, path) = resume::record::location(&capture.root, &capture.path)?;
    ensure!(
        path == capture.path && common == capture.data.common,
        "compaction archive location changed."
    );
    ensure!(
        optional(&path, 128 * 1024 * 1024)? == capture.record
            && optional(&path.with_file_name("summary.json"), 64 * 1024)? == capture.summary
            && optional(&path.with_file_name("complete"), 256)? == capture.marker,
        "compaction archive changed during review."
    );
    if capture.summary.is_some() {
        capture
            .data
            .validate(&capture.root, &common, &path, capture.marker.as_ref())?;
    } else {
        capture.data.check_location(&common, &path)?;
    }
    let unlocked = if let Some(lease) = lease {
        lease.check()?;
        true
    } else {
        absent(&path.with_file_name("resume.lock"))?
    };
    Ok(Observation {
        destination_absent: absent(&capture.data.destination)?,
        metadata_absent: absent(&capture.data.metadata)?,
        complete: capture
            .marker
            .as_ref()
            .is_some_and(|file| file.bytes == resume::COMPLETE),
        unlocked,
    })
}

fn basis(capture: &Capture, observed: &Observation) -> Result<String> {
    Ok(format!(
        "frwtac1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &capture.root,
            &capture.path,
            &capture.data,
            capture.record.as_ref().map(Fingerprint::of),
            capture.summary.as_ref().map(Fingerprint::of),
            capture.marker.as_ref().map(Fingerprint::of),
            observed
        ))?)
    ))
}

fn output(capture: &Capture, observed: &Observation, basis: &str, write: bool) -> Value {
    json!({"schema":1,"operation":if write {"worktree-removal-compact"} else {"worktree-removal-compact-preview"},
        "applied":false,"basis":basis,"repository_root":capture.root,"removal_record":capture.path,
        "summary_record":capture.path.with_file_name("summary.json"),"destination":capture.data.destination,
        "branch":capture.data.branch,"commit":capture.data.commit,"tree":capture.data.tree,
        "state":if capture.record.is_none(){"compacted"}else if capture.summary.is_some(){"compaction-pending"}else{"full"},
        "can_compact":capture.record.is_some() && observed.allowed(),"completion_confirmed":observed.complete,
        "checks":observed,"record_bytes":capture.data.original_record.bytes,
        "summary_bytes":capture.summary.as_ref().map(|file| file.bytes.len()),
        "discard":{"checkout_file_inventory":capture.data.checkout_files,"private_metadata_files":capture.data.metadata_files,
            "private_metadata_bytes":capture.data.metadata_bytes},"atomic_snapshot":false})
}

fn apply(capture: &mut Capture, lease: &ownership::Lease) -> Result<()> {
    if capture.summary.is_none() {
        ensure!(
            observe(capture, Some(lease))?.allowed(),
            "archive no longer qualifies for compaction."
        );
        let encoded = serde_json::to_vec(&capture.data)?;
        ensure!(
            encoded.len() <= 64 * 1024,
            "compaction summary exceeds its size limit."
        );
        let mut temp = tempfile::NamedTempFile::new_in(capture.path.parent().unwrap())?;
        temp.write_all(&encoded)?;
        temp.as_file().sync_all()?;
        lease.check()?;
        temp.persist_noclobber(capture.path.with_file_name("summary.json"))?;
        let saved =
            FileState::read_limited(&capture.path.with_file_name("summary.json"), 64 * 1024)?;
        ensure!(
            saved.bytes == encoded,
            "compaction summary changed during publication."
        );
        capture.summary = Some(saved);
    }
    File::open(capture.path.with_file_name("summary.json"))?.sync_all()?;
    File::open(capture.path.parent().unwrap())?.sync_all()?;
    ensure!(
        observe(capture, Some(lease))?.allowed(),
        "archive no longer qualifies for compaction."
    );
    lease.check()?;
    if let Some(record) = &capture.record {
        record.remove_limited(&capture.path, 128 * 1024 * 1024)?;
    }
    capture.record = None;
    File::open(capture.path.parent().unwrap())?.sync_all()?;
    ensure!(
        observe(capture, Some(lease))?.allowed(),
        "compaction is incomplete or unconfirmed."
    );
    Ok(())
}

pub(in crate::git::worktree) fn report(
    root: &Path,
    options: &CompactRemovalOptions,
) -> Result<Value> {
    let mut capture = capture(root, &options.record)?;
    let observed = observe(&capture, None)?;
    let token = basis(&capture, &observed)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == token,
            "stale archive compaction basis; request a new preview."
        );
    }
    let mut result = output(&capture, &observed, &token, options.write);
    result["basis_verified"] = json!(options.basis.is_some());
    if !options.write {
        return Ok(result);
    }
    ensure!(
        capture.record.is_some() && observed.allowed(),
        "archive cannot compact: require completed removal, absent paths and no active lock."
    );
    let lease = ownership::Lease::acquire(capture.path.with_file_name("resume.lock"))?;
    let current = observe(&capture, Some(&lease))?;
    ensure!(
        basis(&capture, &current)? == token && current.allowed(),
        "archive compaction changed before writing."
    );
    match apply(&mut capture, &lease) {
        Ok(()) => {
            result["applied"] = json!(true);
            result["state"] = json!("compacted");
            result["can_compact"] = json!(false);
            result["summary_bytes"] = json!(capture.summary.as_ref().map(|file| file.bytes.len()));
        }
        Err(error) => {
            result["applied"] = Value::Null;
            result["can_compact"] = Value::Null;
            result["state"] = json!("unconfirmed");
            result["warning"] = json!("Compaction is incomplete or unconfirmed. Inspect the same record path with compact-removal before retrying.");
            let message = format!("{error:#}");
            result["diagnostic"] = json!(crate::git::process::diagnostic(message.as_bytes()));
            result["diagnostic_truncated"] = json!(message.len() > 16 * 1024);
        }
    }
    Ok(result)
}

pub(super) fn inspection(root: &Path, path: &Path) -> Result<Option<Value>> {
    let (_, _, path) = resume::record::location(root, path)?;
    if absent(&path.with_file_name("summary.json"))? {
        return Ok(None);
    }
    let capture = capture(root, &path)?;
    let observed = observe(&capture, None)?;
    let token = basis(&capture, &observed)?;
    let mut result = output(&capture, &observed, &token, false);
    result["operation"] = json!("worktree-removal-inspect");
    result["can_resume"] = json!(false);
    result["detail"] = json!(
        "A compaction summary is present. Use compact-removal to inspect or finish compaction."
    );
    Ok(Some(result))
}
