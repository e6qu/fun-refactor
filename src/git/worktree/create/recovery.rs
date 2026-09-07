use super::{absent, checked, checkout, common, directory, line, ownership, Proposal};
use crate::git::worktree::RecoverOptions;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

struct Capture {
    plan: Proposal,
    receipt: ownership::Receipt,
    receipt_bytes: Vec<u8>,
    blobs: Vec<Vec<u8>>,
}

#[derive(Serialize)]
struct Observation {
    missing: Vec<String>,
    files: BTreeMap<String, (u64, u64, u32, String)>,
    directories: BTreeMap<String, (u64, u64)>,
    index_digest: Option<String>,
    #[serde(skip)]
    index: Option<Vec<u8>>,
}

fn capture(root: &Path, options: &RecoverOptions) -> Result<Capture> {
    let requested = root.canonicalize()?;
    let root = crate::git::process::repository_root(&requested)?;
    ensure!(
        requested.starts_with(&root),
        "Git resolved a working tree outside the requested directory."
    );
    let input = root.join(&options.path);
    directory(&input)?;
    let destination = input.canonicalize()?;
    ensure!(
        crate::git::process::repository_root(&destination)? == destination,
        "recovery requires a worktree root."
    );
    let shared = common(&root)?;
    ensure!(
        common(&destination)? == shared,
        "recovery destination belongs to another repository."
    );
    let output = checked(
        &destination,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let metadata = Path::new(line(&output)?)
        .parent()
        .context("worktree index needs a directory.")?
        .canonicalize()?;
    ensure!(
        metadata.parent() == Some(shared.join("worktrees").as_path()),
        "recovery requires a linked worktree."
    );
    let (receipt, receipt_bytes) = ownership::Receipt::read(&metadata.join("fr-creation.json"))?;
    ensure!(
        !receipt.complete,
        "worktree creation is already complete; recovery will not restore later deletions."
    );
    ensure!(
        receipt.common == shared && receipt.destination == destination,
        "ownership receipt belongs to another worktree."
    );
    let files = checkout::inventory(&root, &receipt.commit)?;
    let tree = checked(
        &root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{tree}}", receipt.commit),
        ],
    )?;
    ensure!(
        line(&tree)? == receipt.tree,
        "ownership receipt tree differs from its commit."
    );
    let registrations = checked(
        &root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?;
    let plan = Proposal {
        version: env!("CARGO_PKG_VERSION"),
        root,
        common: shared,
        common_identity: receipt.common_identity,
        destination,
        parent_identity: receipt.parent_identity,
        branch: receipt.branch.clone(),
        from: receipt.commit.clone(),
        commit: receipt.commit.clone(),
        tree: receipt.tree.clone(),
        registrations: ownership::digest(&registrations),
        files,
    };
    checkout::check_registration(&plan)?;
    let blobs = checkout::blobs(&plan)?;
    Ok(Capture {
        plan,
        receipt,
        receipt_bytes,
        blobs,
    })
}

fn observe(capture: &Capture) -> Result<Observation> {
    let plan = &capture.plan;
    capture.receipt.check()?;
    checkout::check_registration(plan)?;
    let registrations = checked(
        &plan.root,
        &["worktree", "list", "--porcelain", "-z", "--expire=now"],
    )?;
    ensure!(
        ownership::digest(&registrations) == plan.registrations,
        "worktree registrations changed during recovery."
    );
    ensure!(
        ownership::bytes(&capture.receipt.path(), 64 * 1024)? == capture.receipt_bytes,
        "ownership receipt changed."
    );
    let expected = plan
        .files
        .iter()
        .zip(&capture.blobs)
        .map(|(entry, blob)| (entry.path.as_str(), (entry, blob)))
        .collect::<BTreeMap<_, _>>();
    let directories = plan
        .files
        .iter()
        .flat_map(|entry| Path::new(&entry.path).ancestors().skip(1))
        .map(|path| path.to_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let mut observed = Observation {
        missing: Vec::new(),
        files: BTreeMap::new(),
        directories: BTreeMap::new(),
        index_digest: None,
        index: None,
    };
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let directory_path = plan.destination.join(&relative);
        observed.directories.insert(
            relative
                .to_str()
                .context("recovery paths must use UTF-8.")?
                .to_owned(),
            directory(&directory_path)?,
        );
        for entry in fs::read_dir(&directory_path)? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let name = path.to_str().context("recovery paths must use UTF-8.")?;
            if name == ".git" {
                continue;
            }
            let stat = fs::symlink_metadata(entry.path())?;
            if stat.file_type().is_dir() {
                ensure!(
                    directories.contains(name),
                    "unexpected recovery directory: {name:?}."
                );
                pending.push(path);
            } else {
                let (entry, blob) = expected
                    .get(name)
                    .with_context(|| format!("unexpected recovery file: {name:?}."))?;
                ensure!(
                    stat.file_type().is_file(),
                    "recovery refuses non-regular files: {name:?}."
                );
                let raw = ownership::bytes(&plan.destination.join(&path), entry.size as u64)
                    .with_context(|| format!("reading recovery file {name:?}."))?;
                ensure!(
                    crate::git::worktree_recovery_file_allowed(
                        true,
                        raw == **blob,
                        (stat.mode() & 0o100 != 0) == (entry.mode == "100755")
                    ),
                    "recovery file differs from committed bytes or mode: {name:?}."
                );
                observed.files.insert(
                    name.to_owned(),
                    (stat.dev(), stat.ino(), stat.mode(), ownership::digest(&raw)),
                );
            }
        }
    }
    observed.missing = plan
        .files
        .iter()
        .filter(|entry| !observed.files.contains_key(&entry.path))
        .map(|entry| entry.path.clone())
        .collect();
    let index = checkout::index_path(plan)?;
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        ensure!(
            absent(&index.parent().unwrap().join(marker))?,
            "recovery refuses an active Git operation: {marker}."
        );
    }
    if !absent(&index)? {
        let raw = ownership::bytes(&index, 64 * 1024 * 1024)?;
        checkout::verify_index(plan)?;
        ensure!(
            ownership::bytes(&index, 64 * 1024 * 1024)? == raw,
            "recovery index changed during inspection."
        );
        observed.index_digest = Some(ownership::digest(&raw));
        observed.index = Some(raw);
    }
    capture.receipt.check()?;
    Ok(observed)
}

fn basis(capture: &Capture, observation: &Observation) -> Result<String> {
    Ok(format!(
        "frwtr1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &capture.plan,
            ownership::digest(&capture.receipt_bytes),
            observation
        ))?)
    ))
}

pub(in crate::git::worktree) fn report(root: &Path, options: &RecoverOptions) -> Result<Value> {
    ensure!(
        (1..=500).contains(&options.limit),
        "limit must be between 1 and 500."
    );
    let capture = capture(root, options)?;
    let lease = options
        .write
        .then(|| ownership::Lease::acquire(capture.receipt.path().with_extension("lock")))
        .transpose()?;
    let observed = observe(&capture)?;
    let token = basis(&capture, &observed)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == token,
            "stale worktree recovery basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1, "operation":if options.write {"worktree-recover"} else {"worktree-recover-preview"},
        "applied":false,"basis":token,"basis_verified":options.basis.is_some(),"destination":capture.plan.destination,
        "repository_root":capture.plan.root,"ownership_record":capture.receipt.path(),"branch":format!("refs/heads/{}",capture.plan.branch),
        "commit":capture.plan.commit,"tree":capture.plan.tree,"checkout":"raw-blobs","existing_files":observed.files.len(),
        "index_action":if observed.index.is_some(){"preserve"}else{"create"},
        "missing":observed.missing.iter().take(options.limit).collect::<Vec<_>>(),
        "page":{"total":observed.missing.len(),"returned":observed.missing.len().min(options.limit),
            "omitted":observed.missing.len().saturating_sub(options.limit)}});
    if let Some(lease) = lease {
        let current = observe(&capture)?;
        ensure!(
            basis(&capture, &current)? == token,
            "worktree recovery basis changed before writing."
        );
        let outcome = (|| -> Result<()> {
            lease.check()?;
            let index_lease = checkout::prepare_index(&capture.plan, current.index.as_deref())?;
            checkout::populate(
                &capture.plan,
                &capture.blobs,
                capture.receipt.destination_identity,
                true,
            )?;
            let finished = observe(&capture)?;
            ensure!(
                finished.missing.is_empty(),
                "recovery still has missing committed files."
            );
            if let Some(before) = current.index {
                ensure!(
                    finished.index == Some(before),
                    "recovery index changed before completion."
                );
            }
            checkout::sync(&capture.plan)?;
            index_lease.check()?;
            capture.receipt.finish(&lease, &capture.receipt_bytes)?;
            Ok(())
        })();
        match outcome {
            Ok(()) => result["applied"] = json!(true),
            Err(error) => {
                result["applied"] = Value::Null;
                result["warning"] = json!("Recovery is incomplete or unconfirmed. Inspect the checkout and ownership receipt before retrying.");
                let diagnostic = format!("{error:#}");
                result["diagnostic"] =
                    json!(crate::git::process::diagnostic(diagnostic.as_bytes()));
                result["diagnostic_truncated"] = json!(diagnostic.len() > 16 * 1024);
            }
        }
    }
    Ok(result)
}
