use super::super::{absent, directory, ownership};
use super::branch;
use crate::git::worktree::ResumeRemovalOptions;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

mod inspect;
mod record;

const COMPLETE: &[u8] = b"Removal completed. The recorded branch and commit were retained.\n";

fn basis(loaded: &record::Loaded, observed: &inspect::Observation) -> Result<String> {
    Ok(format!(
        "frwtdr1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &loaded.root,
            &loaded.path,
            loaded.archive_identity,
            &loaded.raw,
            observed
        ))?)
    ))
}

fn finish(loaded: &record::Loaded) -> Result<()> {
    loaded.check()?;
    ensure!(
        absent(&loaded.trees[0].root)? && absent(&loaded.trees[1].root)?,
        "removal paths remain or reappeared."
    );
    branch::check(&loaded.root, &loaded.plan.branch, &loaded.plan.commit)?;
    let mut marker = tempfile::NamedTempFile::new_in(loaded.path.parent().unwrap())?;
    marker.write_all(COMPLETE)?;
    marker.as_file().sync_all()?;
    marker.persist_noclobber(loaded.path.with_file_name("complete"))?;
    File::open(loaded.path.parent().unwrap())?.sync_all()?;
    Ok(())
}

fn apply(
    loaded: &record::Loaded,
    observed: &inspect::Observation,
    archive: &ownership::Lease,
    leases: Vec<ownership::Lease>,
) -> Result<()> {
    let check = || -> Result<()> {
        archive.check()?;
        ensure!(
            absent(&loaded.path.with_file_name("complete"))?,
            "removal completion marker appeared during resumption."
        );
        ensure!(
            directory(loaded.path.parent().unwrap())? == loaded.archive_identity
                && directory(&loaded.plan.common)? == loaded.plan.common_identity
                && directory(loaded.plan.destination.parent().unwrap())?
                    == loaded.plan.parent_identity,
            "removal ownership changed before unlinking."
        );
        for lease in &leases {
            lease.check()?;
        }
        Ok(())
    };
    for (tree, scope) in loaded.trees.iter().zip(["checkout", "metadata"]) {
        for row in &observed.rows {
            if row.scope != scope
                || row.kind != "file"
                || row.state != "remaining"
                || row.path == ".git"
            {
                continue;
            }
            check()?;
            inspect::parents(tree, &row.path)?;
            tree.files[&row.path].remove(&tree.root.join(&row.path))?;
        }
        let directories = inspect::directories(tree, observed, scope);
        for path in directories
            .iter()
            .filter(|path| !path.as_os_str().is_empty())
        {
            check()?;
            inspect::parents(tree, path.to_str().unwrap())?;
            ensure!(
                directory(&tree.root.join(path))? == tree.directories[path.to_str().unwrap()],
                "removal directory changed."
            );
            fs::remove_dir(tree.root.join(path))?;
        }
        if scope == "checkout" {
            if directories.iter().any(|path| path.as_os_str().is_empty()) {
                check()?;
                ensure!(
                    directory(&tree.root)? == tree.directories[""],
                    "removal checkout root changed."
                );
                for entry in fs::read_dir(&tree.root)? {
                    ensure!(
                        entry?.file_name() == ".git",
                        "new content blocks resumed removal."
                    );
                }
                if observed
                    .rows
                    .iter()
                    .any(|row| row.scope == scope && row.path == ".git" && row.state == "remaining")
                {
                    tree.files[".git"].remove(&tree.root.join(".git"))?;
                }
                fs::remove_dir(&tree.root)?;
            }
            ensure!(
                absent(&tree.root)?,
                "checkout reappeared during resumed removal."
            );
            File::open(tree.root.parent().unwrap())?.sync_all()?;
        }
    }
    drop(leases);
    let metadata = &loaded.trees[1];
    if observed
        .rows
        .iter()
        .any(|row| row.scope == "metadata" && row.path.is_empty() && row.state == "remaining")
    {
        archive.check()?;
        ensure!(
            directory(&metadata.root)? == metadata.directories[""],
            "removal metadata root changed."
        );
        fs::remove_dir(&metadata.root)?;
    }
    if !absent(metadata.root.parent().unwrap())? {
        File::open(metadata.root.parent().unwrap())?.sync_all()?;
    }
    finish(loaded)
}

pub(in crate::git::worktree) fn report(
    root: &Path,
    options: &ResumeRemovalOptions,
) -> Result<Value> {
    ensure!(
        (1..=500).contains(&options.limit),
        "limit must be between 1 and 500."
    );
    let loaded = record::Loaded::read(root, &options.record)?;
    let observed = inspect::observe(&loaded, false)?;
    let token = basis(&loaded, &observed)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == token,
            "stale removal resumption basis; request a new inspection."
        );
    }
    let mut rows = observed.rows.iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| {
        if row.blocked() {
            0
        } else if row.state == "remaining" {
            1
        } else {
            2
        }
    });
    let blockers = rows.iter().filter(|row| row.blocked()).count();
    let mut result = json!({"schema":1,"operation":if options.write {"worktree-removal-resume"} else {"worktree-removal-inspect"},
        "applied":false,"basis":token,"basis_verified":options.basis.is_some(),"removal_record":loaded.path,
        "repository_root":loaded.root,"destination":loaded.plan.destination,"branch":format!("refs/heads/{}",loaded.plan.branch),
        "commit":loaded.plan.commit,"branch_action":"retain","atomic_snapshot":false,"can_resume":observed.can_resume(),
        "state":if observed.complete {"complete-marker-present"}else if blockers > 0 {"blocked"}else{"incomplete"},
        "counts":{"remaining":rows.iter().filter(|row| row.state == "remaining").count(),
            "missing":rows.iter().filter(|row| row.state == "missing").count(),"blockers":blockers},
        "entries":rows.iter().take(options.limit).collect::<Vec<_>>(),
        "page":{"total":rows.len(),"returned":rows.len().min(options.limit),"omitted":rows.len().saturating_sub(options.limit)}});
    if !options.write {
        return Ok(result);
    }
    ensure!(
        observed.can_resume(),
        "removal cannot resume: inspect blockers or the completion marker."
    );
    let archive = ownership::Lease::acquire(loaded.path.with_file_name("resume.lock"))?;
    loaded.check()?;
    let metadata = &loaded.trees[1];
    let mut leases = Vec::new();
    if !absent(&metadata.root)? {
        ensure!(
            directory(&metadata.root)? == metadata.directories[""],
            "removal metadata root changed before locking."
        );
        for name in ["fr-creation.lock", "index.lock", "HEAD.lock"] {
            leases.push(ownership::Lease::acquire(metadata.root.join(name))?);
        }
    }
    let _branch = branch::Lease::acquire(&loaded.root, &loaded.plan.branch, &loaded.plan.commit)?;
    archive.check()?;
    for lease in &leases {
        lease.check()?;
    }
    let current = inspect::observe(&loaded, true)?;
    ensure!(
        current.can_resume() && basis(&loaded, &current)? == token,
        "removal resumption changed before writing."
    );
    let outcome = apply(&loaded, &current, &archive, leases);
    match outcome {
        Ok(()) => {
            result["applied"] = json!(true);
            result["can_resume"] = json!(false);
            result["state"] = json!("complete");
        }
        Err(error) => {
            result["applied"] = Value::Null;
            result["can_resume"] = Value::Null;
            result["state"] = json!("unconfirmed");
            result["warning"] = json!("Removal remains incomplete or unconfirmed. Inspect the same removal record before retrying.");
            let message = format!("{error:#}");
            result["diagnostic"] = json!(crate::git::process::diagnostic(message.as_bytes()));
            result["diagnostic_truncated"] = json!(message.len() > 16 * 1024);
        }
    }
    Ok(result)
}
