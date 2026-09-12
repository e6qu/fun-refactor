use super::{author, batch, task, Project};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "fr-task-change-1";
const MAX_INPUT_BYTES: u64 = 65_536;

#[derive(Args)]
pub struct Options {
    #[arg(
        long,
        help = "Reviewed task-change manifest path, or - for standard input; at most 64 KiB."
    )]
    pub from: PathBuf,
    #[arg(
        long,
        default_value_t = 65_536,
        help = "Maximum UTF-8 diff bytes, from 0 through 65536."
    )]
    pub diff_bytes: usize,
    #[arg(
        long,
        default_value_t = 65_536,
        help = "Shared serialized query-report budget, from 256 through 1048576 bytes."
    )]
    pub report_bytes: usize,
    #[arg(long, help = "Execute the unchanged reviewed task change.")]
    pub write: bool,
    #[arg(
        long,
        requires = "write",
        help = "Complete task-change basis returned by preview."
    )]
    pub basis: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    requests: Vec<batch::Request>,
    targets: Vec<Target>,
    postconditions: Option<author::BatchPostconditions>,
    checks: Vec<String>,
    delivery: Delivery,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    id: String,
    handle: batch::Argument,
    op: task::AuthorOperation,
    from: Option<PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Delivery {
    #[serde(default)]
    pub exercise_reversal: bool,
    pub patch: Option<PathBuf>,
    #[serde(default = "default_check_output_bytes")]
    pub check_output_bytes: usize,
}

fn default_check_output_bytes() -> usize {
    4_096
}

fn read_manifest(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading task-change manifest from standard input")?;
    } else {
        let path = root.join(path);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "task-change manifest must be a regular file."
        );
        fs::File::open(&path)
            .with_context(|| format!("opening task-change manifest {}", path.display()))?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)?;
    }
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "task-change manifest exceeds 64 KiB."
    );
    Ok(bytes)
}

fn batch_operation(operation: task::AuthorOperation) -> author::BatchOperation {
    match operation {
        task::AuthorOperation::ReplaceBody => author::BatchOperation::ReplaceBody,
        task::AuthorOperation::ReplaceBodySemantic => author::BatchOperation::ReplaceBodySemantic,
        task::AuthorOperation::EditBodySemantic => author::BatchOperation::EditBodySemantic,
        task::AuthorOperation::ReplaceDeclaration => author::BatchOperation::ReplaceDeclaration,
        task::AuthorOperation::InsertDeclaration => author::BatchOperation::InsertDeclaration,
        task::AuthorOperation::OrganizeImports => author::BatchOperation::OrganizeImports,
    }
}

pub(crate) struct Prepared {
    pub plan: author::Plan,
    pub report: Value,
    pub manifest_sha256: String,
    pub checks: crate::checks::Selection,
    pub delivery: Delivery,
}

impl Project<'_> {
    pub(crate) fn task_change(&self, options: &Options) -> Result<Prepared> {
        let bytes = read_manifest(&self.root, &options.from)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .context("task-change input must be a task-change manifest.")?;
        ensure!(
            manifest.schema == SCHEMA,
            "task-change manifest schema must be {SCHEMA}."
        );
        ensure!(
            manifest.delivery.check_output_bytes <= 65_536,
            "task-change check output budget must be between 0 and 65536 bytes."
        );
        ensure!(
            !manifest.checks.is_empty(),
            "task change requires at least one declared check."
        );
        for target in &manifest.targets {
            ensure!(
                target.op.needs_fragment() == target.from.is_some(),
                "task-change target '{}' has an invalid fragment choice.",
                target.id
            );
        }

        let task_manifest = task::Manifest {
            schema: task::SCHEMA.to_owned(),
            requests: manifest.requests.clone(),
            targets: manifest
                .targets
                .iter()
                .map(|target| task::Target {
                    id: target.id.clone(),
                    handle: target.handle.clone(),
                    op: target.op,
                })
                .collect(),
            checks: manifest.checks.clone(),
            delivery: Some(task::Delivery {
                exercise_reversal: manifest.delivery.exercise_reversal,
                patch: manifest.delivery.patch.clone(),
            }),
        };
        let mut task_report = self.task_manifest(task_manifest, options.report_bytes)?;
        let resolved = task_report["targets"]
            .as_array()
            .context("task-change targets must be an array")?;
        ensure!(
            resolved.len() == manifest.targets.len(),
            "task-change target resolution count changed."
        );
        let operations = manifest
            .targets
            .iter()
            .zip(resolved)
            .map(|(target, row)| {
                Ok(author::BatchStep {
                    op: batch_operation(target.op),
                    handle: row["handle"]
                        .as_str()
                        .context("task-change target has no resolved handle")?
                        .to_owned(),
                    from: target.from.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let plan = self.author_batch_manifest(
            author::BatchManifest {
                revision: task_report["revision"].as_str().map(str::to_owned),
                operations,
                postconditions: manifest.postconditions,
            },
            options.diff_bytes,
        )?;
        let checks = crate::checks::select(&self.root, &manifest.checks)?
            .context("task change requires at least one declared check.")?;
        let manifest_sha256 = format!("{:x}", Sha256::digest(&bytes));
        for field in [
            "author_manifest_template",
            "workflow_manifest_template",
            "next",
        ] {
            task_report.as_object_mut().unwrap().remove(field);
        }
        let report = json!({
            "schema": SCHEMA,
            "manifest_sha256": manifest_sha256,
            "revision": task_report["revision"],
            "task_basis": task_report["task_basis"],
            "task_resolution_basis": task_report["task_resolution_basis"],
            "coverage": task_report["coverage"],
            "requests": task_report["requests"],
            "targets": task_report["targets"],
            "checks": task_report["checks"],
            "delivery": {
                "exercise_reversal": manifest.delivery.exercise_reversal,
                "patch": manifest.delivery.patch,
                "check_output_bytes": manifest.delivery.check_output_bytes,
            },
            "executed": false,
            "passed": Value::Null,
        });
        Ok(Prepared {
            plan,
            report,
            manifest_sha256,
            checks,
            delivery: manifest.delivery,
        })
    }
}

pub(crate) fn review_basis(report: &Value, exact_changes: &Value) -> Result<String> {
    Ok(format!(
        "frtc1:{:x}",
        Sha256::digest(serde_json::to_vec(&(
            "fr-task-change-review-1",
            report,
            exact_changes
        ))?)
    ))
}

pub fn task_change_mode(
    complete_review: bool,
    write: bool,
    basis_supplied: bool,
    basis_matches: bool,
) -> usize {
    match (complete_review, write, basis_supplied, basis_matches) {
        (true, false, false, _) => 0,
        (true, true, true, true) => 1,
        _ => 2,
    }
}
