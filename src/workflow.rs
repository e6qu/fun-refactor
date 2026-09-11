//! Bounded orchestration for one reviewed source-history transaction.

use crate::checks;
use crate::history::{self, Action, History, Status};
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_MANIFEST_BYTES: u64 = 65_536;
const MAX_PATCH_PATH_BYTES: usize = 4_096;

#[derive(Args)]
pub struct Options {
    #[arg(
        long,
        value_name = "MANIFEST",
        help = "Reviewed workflow manifest, at most 64 KiB."
    )]
    pub from: PathBuf,
    #[arg(
        long,
        help = "Execute the preflighted workflow and write its patch artifact."
    )]
    pub write: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Manifest {
    schema: u32,
    transaction: u64,
    transaction_context_basis: String,
    checks: CheckRequest,
    #[serde(default)]
    exercise_reversal: bool,
    patch: Option<PatchRequest>,
    #[serde(default = "default_output_bytes")]
    check_output_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckRequest {
    basis: String,
    names: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchRequest {
    output: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Apply = 0,
    CheckApplied = 1,
    Undo = 2,
    CheckRestored = 3,
    Redo = 4,
    DeliverPatch = 5,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::CheckApplied => "check-applied",
            Self::Undo => "undo",
            Self::CheckRestored => "check-restored",
            Self::Redo => "redo",
            Self::DeliverPatch => "deliver-patch",
        }
    }
}

/// Return the next state (`0` planned, `1` applied, `2` refused).
///
/// Stage codes follow [`Stage`]. This small policy is mirrored in Lean.
pub fn workflow_stage_state(applied: bool, stage: usize) -> usize {
    match (applied, stage) {
        (false, 0 | 3) => usize::from(stage == 0),
        (false, 4) => 1,
        (true, 1 | 5) => 1,
        (true, 2) => 0,
        _ => 2,
    }
}

fn default_output_bytes() -> usize {
    4_096
}

fn stages(exercise_reversal: bool, patch: bool) -> Vec<Stage> {
    let mut stages = vec![Stage::Apply, Stage::CheckApplied];
    if exercise_reversal {
        stages.extend([
            Stage::Undo,
            Stage::CheckRestored,
            Stage::Redo,
            Stage::CheckApplied,
        ]);
    }
    if patch {
        stages.push(Stage::DeliverPatch);
    }
    stages
}

fn read_manifest(path: &Path) -> Result<(Manifest, Vec<u8>)> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading workflow manifest {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "workflow manifest must be a regular file"
    );
    ensure!(
        metadata.len() <= MAX_MANIFEST_BYTES,
        "workflow manifest exceeds 65536 bytes"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_MANIFEST_BYTES,
        "workflow manifest exceeds 65536 bytes"
    );
    Ok((serde_json::from_slice(&bytes)?, bytes))
}

fn digest_parts(parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    format!("{:x}", digest.finalize())
}

fn basis_matches(supplied: &str, complete: &str) -> bool {
    supplied == complete
        || supplied.len() >= 32 && supplied.len() < complete.len() && complete.starts_with(supplied)
}

fn patch_path(root: &Path, requested: &Path) -> Result<PathBuf> {
    ensure!(
        !requested.as_os_str().is_empty()
            && !requested.is_absolute()
            && requested.as_os_str().as_encoded_bytes().len() <= MAX_PATCH_PATH_BYTES
            && requested
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "workflow patch output must be a relative normal path of at most 4096 bytes"
    );
    ensure!(
        !requested.components().any(|component| {
            matches!(component.as_os_str().to_str(), Some(".git" | ".fr-history"))
        }),
        "workflow patch output cannot enter .git or .fr-history"
    );
    let mut path = root.to_path_buf();
    let components = requested.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("workflow patch output cannot traverse a symlink")
            }
            Ok(_) if index + 1 == components.len() => {
                bail!(
                    "workflow patch output already exists: {}",
                    requested.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!("workflow patch output parent is not a directory")
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if index + 1 != components.len() {
                    bail!("workflow patch output parent does not exist")
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

fn write_patch(path: &Path, patch: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("workflow patch output has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(patch.as_bytes())?;
    temporary.flush()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}

struct Preflight {
    root: PathBuf,
    manifest: Manifest,
    manifest_digest: String,
    workflow_basis: String,
    resolved_checks: Vec<String>,
    check_basis: String,
    patch: String,
    patch_digest: String,
    patch_path: Option<PathBuf>,
    report: Value,
}

fn preflight(root: &Path, options: &Options) -> Result<Preflight> {
    let root = root.canonicalize()?;
    ensure!(root.is_dir(), "workflow root must be a directory");
    let (manifest, manifest_bytes) = read_manifest(&root.join(&options.from))?;
    ensure!(manifest.schema == 1, "workflow manifest requires schema 1");
    ensure!(
        manifest.transaction > 0,
        "workflow transaction must be positive"
    );
    ensure!(
        manifest.check_output_bytes <= 65_536,
        "workflow check output budget must be between 0 and 65536 bytes"
    );

    let history = History::read(&root)?;
    history.ensure_ready()?;
    let record = history.record(manifest.transaction)?;
    ensure!(
        record.status == Status::Planned,
        "workflow requires a planned transaction"
    );
    let complete_context = history::transaction_context_basis(record);
    ensure!(
        manifest.transaction_context_basis == complete_context,
        "stale or conflicting workflow transaction context basis"
    );
    history::act_with_context(
        &root,
        Action::Apply,
        manifest.transaction,
        false,
        true,
        Some(&complete_context),
    )?;

    let selection = checks::select(&root, &manifest.checks.names)?
        .context("workflow requires at least one declared check")?;
    ensure!(
        basis_matches(&manifest.checks.basis, &selection.configuration_basis),
        "workflow check configuration basis is missing or stale"
    );
    if let Some(required) = &record.required_checks {
        ensure!(
            required.configuration_basis == selection.configuration_basis
                && required.checks == selection.checks,
            "workflow checks do not satisfy the transaction's required selection"
        );
    }

    let export = history::export_patch(&root, manifest.transaction, false)?;
    let patch_digest = format!("{:x}", Sha256::digest(export.patch.as_bytes()));
    let resolved_path = manifest
        .patch
        .as_ref()
        .map(|request| patch_path(&root, &request.output))
        .transpose()?;
    if let Some(request) = &manifest.patch {
        ensure!(
            !record
                .changes
                .iter()
                .any(|change| change.path == request.output),
            "workflow patch output conflicts with a transaction target"
        );
    }

    let manifest_digest = format!("{:x}", Sha256::digest(&manifest_bytes));
    let workflow_basis = digest_parts(&[
        b"fr-workflow-1",
        manifest_digest.as_bytes(),
        record.basis.as_bytes(),
        complete_context.as_bytes(),
        selection.configuration_basis.as_bytes(),
        patch_digest.as_bytes(),
    ]);
    let planned_stages = stages(manifest.exercise_reversal, manifest.patch.is_some());
    let report = json!({
        "schema": "fr-workflow-1",
        "manifest_sha256": manifest_digest,
        "workflow_basis": workflow_basis,
        "transaction": manifest.transaction,
        "transaction_status": record.status,
        "record_basis": record.basis,
        "transaction_context_basis": complete_context,
        "checks": {
            "basis": selection.configuration_basis,
            "names": selection.checks,
            "coverage": selection.coverage,
            "output_bytes": manifest.check_output_bytes,
        },
        "exercise_reversal": manifest.exercise_reversal,
        "patch": manifest.patch.as_ref().map(|request| json!({
            "output": request.output,
            "bytes": export.patch.len(),
            "sha256": patch_digest,
            "format": export.format,
        })),
        "stages": planned_stages.iter().map(|stage| json!({
            "stage": stage.name(), "status": "pending"
        })).collect::<Vec<_>>(),
        "ready": true,
        "executed": false,
        "passed": Value::Null,
    });

    Ok(Preflight {
        root,
        manifest,
        manifest_digest,
        workflow_basis,
        resolved_checks: selection.checks,
        check_basis: selection.configuration_basis,
        patch: export.patch,
        patch_digest,
        patch_path: resolved_path,
        report,
    })
}

fn compact_check(report: Value) -> Value {
    json!({
        "basis": report["basis"],
        "source_revision": report["source_revision"],
        "source_snapshot_stable": report["source_snapshot_stable"],
        "passed": report["passed"],
        "results": report["results"],
        "recorded_evidence": report.get("recorded_evidence").cloned(),
    })
}

fn current_status(root: &Path, transaction: u64) -> Value {
    History::read(root)
        .ok()
        .and_then(|history| {
            history
                .record(transaction)
                .ok()
                .map(|record| json!(record.status))
        })
        .unwrap_or(Value::Null)
}

fn fail_stage(
    mut report: Value,
    root: &Path,
    transaction: u64,
    index: usize,
    error: String,
) -> Value {
    report["stages"][index]["status"] = json!("failed");
    report["stages"][index]["error"] = json!(error);
    report["transaction_status"] = current_status(root, transaction);
    report["executed"] = json!(true);
    report["passed"] = json!(false);
    report
}

pub struct Outcome {
    pub report: Value,
    pub passed: bool,
}

pub fn run(root: &Path, options: &Options) -> Result<Outcome> {
    let preflight = preflight(root, options)?;
    if !options.write {
        return Ok(Outcome {
            report: preflight.report,
            passed: true,
        });
    }

    let Preflight {
        root,
        manifest,
        manifest_digest,
        workflow_basis,
        resolved_checks,
        check_basis,
        patch,
        patch_digest,
        patch_path,
        mut report,
    } = preflight;
    let transaction = manifest.transaction;
    let planned_stages = stages(manifest.exercise_reversal, manifest.patch.is_some());
    let mut applied = false;

    for (index, stage) in planned_stages.into_iter().enumerate() {
        let next = workflow_stage_state(applied, stage as usize);
        if next == 2 {
            return Ok(Outcome {
                report: fail_stage(
                    report,
                    &root,
                    transaction,
                    index,
                    "workflow lifecycle policy refused the stage".into(),
                ),
                passed: false,
            });
        }
        let result = match stage {
            Stage::Apply | Stage::Undo | Stage::Redo => {
                let action = match stage {
                    Stage::Apply => Action::Apply,
                    Stage::Undo => Action::Undo,
                    Stage::Redo => Action::Redo,
                    _ => unreachable!(),
                };
                let context = matches!(action, Action::Apply | Action::Redo)
                    .then_some(manifest.transaction_context_basis.as_str());
                history::act_with_context(&root, action, transaction, true, false, context).map(
                    |transition| {
                        report["stages"][index]["result"] = transition;
                    },
                )
            }
            Stage::CheckApplied | Stage::CheckRestored => {
                let options = checks::Options {
                    run: resolved_checks.clone(),
                    basis: Some(check_basis.clone()),
                    output_bytes: manifest.check_output_bytes,
                    quiet_success: true,
                    no_declarations: true,
                    record_for: (stage == Stage::CheckApplied).then_some(transaction),
                };
                checks::report(&root, &options).and_then(|check| {
                    let passed = check["passed"] == true;
                    report["stages"][index]["result"] = compact_check(check);
                    ensure!(passed, "declared checks failed");
                    Ok(())
                })
            }
            Stage::DeliverPatch => {
                let path = patch_path
                    .as_deref()
                    .context("workflow patch stage has no output path")?;
                patch_path_for_delivery(&root, &manifest, path)?;
                write_patch(path, &patch).map(|()| {
                    report["stages"][index]["result"] = json!({
                        "output": manifest.patch.as_ref().unwrap().output,
                        "bytes": patch.len(),
                        "sha256": patch_digest,
                    });
                })
            }
        };
        if let Err(error) = result {
            return Ok(Outcome {
                report: fail_stage(report, &root, transaction, index, format!("{error:#}")),
                passed: false,
            });
        }
        report["stages"][index]["status"] = json!("passed");
        applied = next == 1;
    }

    report["manifest_sha256"] = json!(manifest_digest);
    report["workflow_basis"] = json!(workflow_basis);
    report["transaction_status"] = current_status(&root, transaction);
    report["executed"] = json!(true);
    report["passed"] = json!(true);
    Ok(Outcome {
        report,
        passed: true,
    })
}

fn patch_path_for_delivery(root: &Path, manifest: &Manifest, expected: &Path) -> Result<()> {
    let requested = &manifest
        .patch
        .as_ref()
        .context("missing patch request")?
        .output;
    let current = patch_path(root, requested)?;
    ensure!(
        current == expected,
        "workflow patch output resolved differently after validation"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_policy_accepts_only_state_appropriate_steps() {
        let expected = [[1, 2, 2, 0, 1, 2], [2, 1, 0, 2, 2, 1]];
        for (applied, row) in expected.into_iter().enumerate() {
            for (stage, next) in row.into_iter().enumerate() {
                assert_eq!(workflow_stage_state(applied == 1, stage), next);
            }
        }
    }

    #[test]
    fn generated_lifecycles_start_planned_and_finish_applied() {
        for exercise in [false, true] {
            for patch in [false, true] {
                let mut applied = false;
                for stage in stages(exercise, patch) {
                    let next = workflow_stage_state(applied, stage as usize);
                    assert_ne!(next, 2);
                    applied = next == 1;
                }
                assert!(applied);
            }
        }
    }
}
