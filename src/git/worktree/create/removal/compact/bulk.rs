use super::super::super::super::CompactRemovalOptions;
use crate::git::worktree::CompactRemovalsOptions;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

fn previews(root: &Path, records: &[std::path::PathBuf]) -> Result<Vec<Value>> {
    let mut rows = Vec::with_capacity(records.len());
    let mut selected = BTreeSet::new();
    for record in records {
        let row = super::report(
            root,
            &CompactRemovalOptions {
                record: record.clone(),
                basis: None,
                write: false,
            },
        )?;
        let path = row["removal_record"]
            .as_str()
            .context("removal compaction report lacks its record path")?;
        ensure!(
            selected.insert(path.to_owned()),
            "bulk removal compaction contains a duplicate archive."
        );
        rows.push(row);
    }
    Ok(rows)
}

fn basis(rows: &[Value]) -> Result<String> {
    let observations = rows
        .iter()
        .map(|row| {
            (
                &row["removal_record"],
                &row["basis"],
                &row["state"],
                &row["can_compact"],
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        "frwtacs1:{:x}",
        Sha256::digest(serde_json::to_vec(&observations)?)
    ))
}

fn selected(rows: &[Value]) -> Value {
    json!(rows
        .iter()
        .map(|row| json!({"removal_record":row["removal_record"],
            "summary_record":row["summary_record"],"branch":row["branch"],
            "commit":row["commit"],"record_bytes":row["record_bytes"],
            "discard":row["discard"],"basis":row["basis"]}))
        .collect::<Vec<_>>())
}

fn eligible(rows: &[Value]) -> bool {
    rows.iter()
        .all(|row| row["state"] == "full" && row["can_compact"] == true && row["applied"] == false)
}

pub(in crate::git::worktree) fn report(
    root: &Path,
    options: &CompactRemovalsOptions,
) -> Result<Value> {
    let rows = previews(root, &options.records)?;
    let token = basis(&rows)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == token,
            "stale bulk removal compaction basis; request a new preview."
        );
    }
    ensure!(
        eligible(&rows),
        "bulk removal compaction requires completed, uncompacted and eligible archives."
    );
    let repository_root = rows[0]["repository_root"].clone();
    let total_bytes = rows
        .iter()
        .map(|row| row["record_bytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let mut result = json!({"schema":1,"operation":if options.write{"worktree-removals-compact"}else{"worktree-removals-compact-preview"},
        "repository_root":repository_root,"basis":token,"basis_verified":options.basis.is_some(),
        "applied":false,"can_compact":true,"archives":rows.len(),"record_bytes":total_bytes,
        "selected":selected(&rows),"atomic":false});
    if !options.write {
        return Ok(result);
    }

    let current = previews(root, &options.records)?;
    ensure!(
        basis(&current)? == token && eligible(&current),
        "bulk removal compaction changed before writing."
    );
    let mut completed = Vec::new();
    for row in &current {
        let path = row["removal_record"]
            .as_str()
            .context("removal compaction report lacks its record path")?;
        let individual_basis = row["basis"]
            .as_str()
            .context("removal compaction report lacks its basis")?;
        let outcome = super::report(
            root,
            &CompactRemovalOptions {
                record: path.into(),
                basis: Some(individual_basis.to_owned()),
                write: true,
            },
        );
        match outcome {
            Ok(outcome) if outcome["applied"] == true => {
                completed.push(outcome["removal_record"].clone());
            }
            Ok(outcome) => {
                result["applied"] = Value::Null;
                result["can_compact"] = Value::Null;
                result["completed"] = json!(completed);
                result["stopped_at"] = json!(path);
                result["outcome"] = outcome;
                result["warning"] = json!("Bulk compaction stopped after an incomplete or unconfirmed archive. Inspect every selected record before retrying.");
                return Ok(result);
            }
            Err(error) => {
                result["applied"] = Value::Null;
                result["can_compact"] = Value::Null;
                result["completed"] = json!(completed);
                result["stopped_at"] = json!(path);
                let message = format!("{error:#}");
                result["diagnostic"] = json!(crate::git::process::diagnostic(message.as_bytes()));
                result["diagnostic_truncated"] = json!(message.len() > 16 * 1024);
                result["warning"] = json!("Bulk compaction stopped after an archive changed or refused. Inspect every selected record before retrying.");
                return Ok(result);
            }
        }
    }
    result["applied"] = json!(true);
    result["completed"] = json!(completed);
    Ok(result)
}
