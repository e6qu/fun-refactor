use super::{batch, hash, Project};
use crate::lang::Language;
use crate::model::SymbolKind;
use anyhow::{ensure, Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

const SCHEMA: &str = "fr-project-task-1";
const MAX_INPUT_BYTES: u64 = 65_536;

#[derive(clap::Args)]
pub struct Options {
    #[arg(
        long,
        help = "JSON task manifest path, or - for standard input; at most 64 KiB."
    )]
    from: PathBuf,
    #[arg(
        long,
        default_value_t = 65_536,
        help = "Shared serialized query-report budget, from 256 through 1048576 bytes."
    )]
    report_bytes: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    requests: Vec<batch::Request>,
    targets: Vec<Target>,
    #[serde(default)]
    checks: Vec<String>,
    delivery: Option<Delivery>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    id: String,
    handle: batch::Argument,
    op: AuthorOperation,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Delivery {
    #[serde(default)]
    exercise_reversal: bool,
    patch: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum AuthorOperation {
    ReplaceBody,
    ReplaceDeclaration,
    InsertDeclaration,
    OrganizeImports,
}

impl AuthorOperation {
    fn name(self) -> &'static str {
        match self {
            Self::ReplaceBody => "replace-body",
            Self::ReplaceDeclaration => "replace-declaration",
            Self::InsertDeclaration => "insert-declaration",
            Self::OrganizeImports => "organize-imports",
        }
    }

    fn code(self) -> usize {
        match self {
            Self::ReplaceBody => 0,
            Self::ReplaceDeclaration => 1,
            Self::InsertDeclaration => 2,
            Self::OrganizeImports => 3,
        }
    }

    fn needs_fragment(self) -> bool {
        !matches!(self, Self::OrganizeImports)
    }
}

fn read_manifest(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading project task manifest from standard input")?;
    } else {
        let path = root.join(path);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "project task manifest must be a regular file."
        );
        fs::File::open(&path)
            .with_context(|| format!("opening project task manifest {}", path.display()))?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("reading project task manifest")?;
    }
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "project task manifest exceeds 64 KiB."
    );
    Ok(bytes)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn safe_patch_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.as_os_str().as_encoded_bytes().len() <= 4096
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && !path
            .components()
            .any(|component| matches!(component.as_os_str().to_str(), Some(".git" | ".fr-history")))
}

fn reference_value(
    argument: &batch::Argument,
    requests: &BTreeMap<&str, &Value>,
) -> Result<String> {
    match argument {
        batch::Argument::Literal(value) => Ok(value.clone()),
        batch::Argument::Reference(reference) => requests
            .get(reference.request.as_str())
            .with_context(|| {
                format!(
                    "task target references unknown request '{}'.",
                    reference.request
                )
            })?
            .get("report")
            .context("task target references a query omitted by the report budget.")?
            .pointer(&reference.pointer)
            .with_context(|| {
                format!(
                    "task target reference '{}' has no value at '{}'.",
                    reference.request, reference.pointer
                )
            })?
            .as_str()
            .context("task target references a non-string result.")
            .map(str::to_owned),
    }
}

fn language_code(language: Language) -> usize {
    Language::ALL
        .iter()
        .position(|candidate| *candidate == language)
        .expect("all languages have a policy code")
}

fn target_code(file: bool, kind: Option<SymbolKind>) -> usize {
    if file {
        return 0;
    }
    match kind {
        Some(SymbolKind::Function) => 1,
        Some(SymbolKind::Method) => 2,
        Some(SymbolKind::Variable) => 3,
        Some(SymbolKind::Module) => 4,
        Some(SymbolKind::Trait) => 5,
        _ => 6,
    }
}

/// Conservative target-level authoring admission. Fragment syntax and exact tree shape are
/// deliberately checked later by the existing author preview.
pub fn task_author_target_candidate(operation: usize, language: usize, target: usize) -> bool {
    match operation {
        0 => {
            matches!(language, 0 | 1 | 3 | 4 | 5)
                && (matches!(target, 1 | 2) || matches!(language, 4 | 5) && target == 3)
        }
        1 => language == 0 && matches!(target, 1 | 2),
        2 => language == 0 && matches!(target, 0 | 2 | 4 | 5),
        3 => target == 0,
        _ => false,
    }
}

impl Project<'_> {
    pub(super) fn task(&self, options: &Options) -> Result<Value> {
        ensure!(
            (256..=1_048_576).contains(&options.report_bytes),
            "report bytes must be between 256 and 1048576."
        );
        let bytes = read_manifest(&self.root, &options.from)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .context("project task input must be a task manifest.")?;
        ensure!(
            manifest.schema == SCHEMA,
            "project task manifest schema must be {SCHEMA}."
        );
        ensure!(
            (1..=16).contains(&manifest.targets.len()),
            "project task needs 1 through 16 targets."
        );
        let mut target_ids = BTreeSet::new();
        for target in &manifest.targets {
            ensure!(
                valid_id(&target.id),
                "project task target IDs need 1 through 80 ASCII letters, digits, '.', '_' or '-'."
            );
            ensure!(
                target_ids.insert(&target.id),
                "project task target IDs must be unique."
            );
            match &target.handle {
                batch::Argument::Literal(handle) => ensure!(
                    !handle.is_empty() && handle.len() <= 512,
                    "project task literal handles need 1 through 512 bytes."
                ),
                batch::Argument::Reference(reference) => ensure!(
                    valid_id(&reference.request)
                        && !reference.pointer.is_empty()
                        && reference.pointer.starts_with('/')
                        && reference.pointer.len() <= 512,
                    "project task target references need a valid request ID and a 1 through 512 byte JSON pointer."
                ),
            }
        }
        ensure!(
            manifest.checks.len() <= 32,
            "project task accepts at most 32 declared checks."
        );
        ensure!(
            manifest.delivery.is_none() || !manifest.checks.is_empty(),
            "project task delivery requires at least one declared check."
        );
        if let Some(path) = manifest
            .delivery
            .as_ref()
            .and_then(|delivery| delivery.patch.as_deref())
        {
            ensure!(
                safe_patch_path(path),
                "project task patch path must be a relative path without parent components."
            );
        }

        let task_basis = format!("frpt1:{}", hash((SCHEMA, &manifest))?);
        let batch_manifest = batch::Manifest {
            schema: batch::SCHEMA.to_owned(),
            requests: manifest.requests.clone(),
        };
        let mut report = self.batch_manifest(batch_manifest, options.report_bytes, "task")?;
        let request_rows = report["requests"]
            .as_array()
            .context("task query results must be an array")?;
        let requests = request_rows
            .iter()
            .filter_map(|request| request["id"].as_str().map(|id| (id, request)))
            .collect::<BTreeMap<_, _>>();

        let mut author_operations = Vec::with_capacity(manifest.targets.len());
        let mut target_rows = Vec::with_capacity(manifest.targets.len());
        let mut resolved_targets = Vec::with_capacity(manifest.targets.len());
        for target in &manifest.targets {
            let handle = reference_value(&target.handle, &requests)?;
            let node_id = self.resolve_handle(&handle)?;
            let node = &self.nodes[node_id];
            let symbol = node.symbol.and_then(|id| self.index.symbol(id));
            let is_file = node.kind == "file";
            let language = if let Some(symbol) = symbol {
                symbol.language
            } else {
                ensure!(
                    is_file,
                    "task authoring targets must be file or declaration handles."
                );
                self.index
                    .file(&self.root.join(&node.path))
                    .context("task target file is not indexed.")?
                    .language
            };
            let kind = symbol.map(|symbol| symbol.kind);
            let candidate = task_author_target_candidate(
                target.op.code(),
                language_code(language),
                target_code(is_file, kind),
            );
            ensure!(
                candidate,
                "task target '{}' does not support requested operation '{}'.",
                target.id,
                target.op.name()
            );
            let fragment = target
                .op
                .needs_fragment()
                .then(|| format!("<FRAGMENT:{}>", target.id));
            let mut operation = json!({
                "op": target.op.name(),
                "handle": handle,
            });
            if let Some(fragment) = &fragment {
                operation["from"] = json!(fragment);
            }
            author_operations.push(operation);
            target_rows.push(json!({
                "id": target.id,
                "handle": handle,
                "path": node.path,
                "language": language,
                "kind": if is_file { Value::String("file".into()) } else { json!(kind) },
                "operation": target.op,
                "eligibility": "target-supported",
                "syntax_preflighted": false
            }));
            resolved_targets.push((target.id.clone(), handle, target.op));
        }

        let selection = crate::checks::select(&self.root, &manifest.checks)?;
        let checks = selection.as_ref().map_or_else(
            || json!({"selected": false, "names": []}),
            |selection| json!({
                "selected": true,
                "basis": selection.configuration_basis,
                "names": selection.checks,
                "coverage": selection.coverage,
                "command": ["fr", "checks", "--run", selection.checks.join(","), "--basis", &selection.configuration_basis, "--quiet-success", "--no-declarations"]
            }),
        );
        let delivery = manifest.delivery.as_ref();
        let workflow_template = delivery.map(|delivery| json!({
            "schema": 1,
            "transaction": "<TRANSACTION_ID>",
            "transaction-context-basis": "<TRANSACTION_CONTEXT_BASIS>",
            "checks": {
                "basis": selection.as_ref().map(|value| value.configuration_basis.as_str()).unwrap_or("<CHECK_BASIS>"),
                "names": selection.as_ref().map(|value| value.checks.as_slice()).unwrap_or(&[])
            },
            "exercise-reversal": delivery.exercise_reversal,
            "patch": delivery.patch.as_ref().map(|path| json!({"output": path}))
        }));
        let resolution_basis = format!(
            "frpt2:{}",
            hash((
                SCHEMA,
                &self.revision,
                &task_basis,
                &report["resolution_basis"],
                &resolved_targets,
                &manifest.checks
            ))?
        );
        report.as_object_mut().unwrap().remove("manifest_basis");
        report.as_object_mut().unwrap().remove("resolution_basis");
        report["task_basis"] = json!(task_basis);
        report["task_resolution_basis"] = json!(resolution_basis);
        report["targets"] = json!(target_rows);
        report["checks"] = checks;
        report["author_manifest_template"] = json!({
            "revision": self.revision,
            "operations": author_operations
        });
        report["workflow_manifest_template"] = workflow_template.unwrap_or(Value::Null);
        report["next"] = json!({
            "author-preview": "fr author batch --from <AUTHOR_MANIFEST>",
            "author-save": "fr author batch --from <AUTHOR_MANIFEST> --save-plan --plan-basis <PLAN_CONTEXT_BASIS>",
            "workflow-preview": "fr workflow --from <WORKFLOW_MANIFEST>",
            "workflow-write": "fr workflow --from <WORKFLOW_MANIFEST> --write --basis <WORKFLOW_BASIS>"
        });
        Ok(report)
    }
}
