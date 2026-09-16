use super::{agent_intent::Manifest, author, migration, task_change, Project};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const SCHEMA: &str = "fr-intent-action-2";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Action {
    schema: String,
    operation: Operation,
    #[serde(default)]
    proof_expectation: ProofExpectation,
    #[serde(default)]
    guide: Option<GuideBinding>,
    #[serde(default = "default_diff")]
    pub(crate) diff_bytes: usize,
    #[serde(default = "default_report")]
    report_bytes: usize,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GuideBinding {
    goal: Value,
    basis: String,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ProofExpectation {
    #[default]
    None,
    Model,
    Implementation,
}

fn default_diff() -> usize {
    4_096
}
fn default_report() -> usize {
    65_536
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    Capability {
        capability: crate::capabilities::Capability,
        #[serde(default)]
        parameters: std::collections::BTreeMap<String, String>,
        #[serde(default)]
        range: Option<crate::span::Span>,
        #[serde(default)]
        checks: Vec<String>,
        #[serde(default)]
        delivery: Option<task_change::Delivery>,
    },
    ProjectQuery {
        requests: Vec<super::batch::Request>,
    },
    SurfaceEdit {
        edit: String,
        to: String,
        checks: Vec<String>,
        delivery: task_change::Delivery,
    },
    PropertyTask {},
    ProofTask {
        obligation: String,
    },
    TaskChange {
        task_change: Value,
    },
    AuthorBatch {
        author_batch: Value,
        checks: Vec<String>,
        delivery: task_change::Delivery,
    },
    Recipe {
        recipe: String,
        checks: Vec<String>,
        delivery: task_change::Delivery,
    },
    FrameworkMigration {
        feature: String,
        to: migration::Target,
        out: PathBuf,
        #[serde(default)]
        register_with: Option<String>,
        #[serde(default)]
        dependency_manifest: Option<PathBuf>,
        #[serde(default)]
        dependency_requirement: Vec<String>,
        #[serde(default)]
        cutover: bool,
        checks: Vec<String>,
        delivery: task_change::Delivery,
    },
    FormalPlan {
        properties: Vec<String>,
        #[serde(default)]
        agent_properties: Vec<crate::spec::AgentProperty>,
        #[serde(default)]
        package: Option<PathBuf>,
        #[serde(default)]
        checks: Vec<String>,
        #[serde(default)]
        delivery: Option<task_change::Delivery>,
    },
    ProofSubmission {
        obligation: String,
        tactics: String,
        checks: Vec<String>,
        delivery: task_change::Delivery,
    },
}

impl Operation {
    fn code(&self) -> usize {
        match self {
            Self::Capability { capability, .. } => {
                if capability.agent_form().writes {
                    11
                } else {
                    10
                }
            }
            Self::ProjectQuery { .. } => 6,
            Self::SurfaceEdit { .. } => 7,
            Self::PropertyTask { .. } => 8,
            Self::ProofTask { .. } => 9,
            Self::TaskChange { .. } => 0,
            Self::AuthorBatch { .. } => 1,
            Self::Recipe { .. } => 2,
            Self::FrameworkMigration { .. } => 3,
            Self::FormalPlan { .. } => 4,
            Self::ProofSubmission { .. } => 5,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            Self::Capability { .. } => "capability",
            Self::ProjectQuery { .. } => "project-query",
            Self::SurfaceEdit { .. } => "surface-edit",
            Self::PropertyTask { .. } => "property-task",
            Self::ProofTask { .. } => "proof-task",
            Self::TaskChange { .. } => "task-change",
            Self::AuthorBatch { .. } => "author-batch",
            Self::Recipe { .. } => "recipe",
            Self::FrameworkMigration { .. } => "framework-migration",
            Self::FormalPlan { .. } => "formal-plan",
            Self::ProofSubmission { .. } => "proof-submission",
        }
    }
}

pub fn intent_action_purpose_allowed(purpose: usize, operation: usize) -> bool {
    matches!((purpose, operation), (2, 0..=2 | 7 | 11) | (3, 3) | (4, 4..=5 | 8..=9) | (0..=4, 6 | 10))
}

pub(crate) struct Change {
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
}
pub(crate) struct Removal {
    pub path: PathBuf,
    pub original: String,
}
pub(crate) struct Prepared {
    pub report: Value,
    pub report_bytes: usize,
    pub changes: Vec<Change>,
    pub removals: Vec<Removal>,
    pub checks: Option<crate::checks::Selection>,
    pub delivery: Option<task_change::Delivery>,
    pub targets: Vec<String>,
    pub operation: usize,
}
impl Prepared {
    fn empty(kind: &str, operation: usize) -> Self {
        Self {
            report: json!({"schema": "fr-intent-operation-review-2", "kind": kind,
                "ready": true, "executed": false,
                "claims": {"model_theorem_checked": false, "implementation_correspondence": false}}),
            report_bytes: 65_536,
            changes: vec![],
            removals: vec![],
            checks: None,
            delivery: None,
            targets: vec![],
            operation,
        }
    }
    fn edits(&mut self, edits: &crate::edit::EditSet) -> Result<()> {
        self.changes.extend(
            crate::edit::plan(edits, crate::edit::Validation::ReparseStrict)?
                .into_iter()
                .filter(crate::edit::FileOutcome::changed)
                .map(|outcome| Change {
                    path: outcome.path,
                    original: outcome.original,
                    updated: outcome.updated,
                }),
        );
        Ok(())
    }
    fn delivery(
        &mut self,
        root: &Path,
        checks: &[String],
        delivery: &task_change::Delivery,
    ) -> Result<()> {
        ensure!(
            delivery.check_output_bytes <= 65_536,
            "intent action check output exceeds 65536 bytes."
        );
        let selection = crate::checks::select(root, checks)?
            .context("writing intent actions require declared project checks.")?;
        self.report["checks"] = json!({"configuration_basis": selection.configuration_basis,
            "names": selection.checks, "coverage": selection.coverage});
        self.report["delivery"] = serde_json::to_value(delivery)?;
        self.checks = Some(selection);
        self.delivery = Some(delivery.clone());
        Ok(())
    }
}

impl Action {
    pub(super) fn validate(&self, purpose: usize) -> Result<()> {
        ensure!(
            self.schema == SCHEMA,
            "tagged intent action schema must be {SCHEMA}."
        );
        ensure!(
            intent_action_purpose_allowed(purpose, self.operation.code()),
            "intent action kind is outside its declared purpose."
        );
        ensure!(!matches!(self.proof_expectation, ProofExpectation::Implementation),
            "no intent action establishes general implementation correspondence without separate evidence.");
        ensure!(
            !matches!(self.proof_expectation, ProofExpectation::Model) || purpose == 4,
            "model proof expectations require purpose prove."
        );
        ensure!(
            self.diff_bytes <= 65_536,
            "intent action diff limit exceeds 65536 bytes."
        );
        ensure!(
            (256..=1_048_576).contains(&self.report_bytes),
            "intent action report limit must be 256 through 1048576 bytes."
        );
        Ok(())
    }
}

impl Project<'_> {
    pub(super) fn prepare_intent_action(
        &self,
        action: &Action,
        intent: &Manifest,
    ) -> Result<Prepared> {
        let anchor = self.resolve_handle(&intent.target)?;
        let node = &self.nodes[anchor];
        let mut prepared = Prepared::empty(action.operation.name(), action.operation.code());
        prepared.report_bytes = action.report_bytes;
        prepared.report["input_sha256"] = json!(hex::encode(Sha256::digest(serde_json::to_vec(
            &serde_json::to_value(action)?
        )?)));
        if let Some(binding) = &action.guide {
            let goal = super::agent_guide::normalized_goal(&serde_json::to_vec(&binding.goal)?)?;
            let guide = self.agent_guide_bytes(&serde_json::to_vec(&goal)?)?;
            ensure!(
                guide["basis"] == binding.basis
                    && guide["target"]["handle"] == intent.target
                    && guide["purpose"] == serde_json::to_value(intent.purpose)?
                    && guide["route"]["admitted"] == true,
                "intent guide basis, purpose, target or admission changed."
            );
            ensure!(
                goal["proof"].as_str().unwrap_or("none")
                    == serde_json::to_value(action.proof_expectation)?,
                "intent proof expectation differs from its guide goal."
            );
            self.validate_guided_operation(
                &goal["operation"],
                &guide,
                &action.operation,
                &intent.target,
            )?;
            prepared.report["guide_basis"] = json!(binding.basis);
        }
        match &action.operation {
            Operation::Capability {
                capability,
                parameters,
                range,
                checks,
                delivery,
            } => {
                let plan =
                    self.intent_capability(&intent.target, *capability, parameters, *range)?;
                prepared.edits(&plan.edits)?;
                prepared.report["plan"] = plan.report;
                prepared.targets.push(intent.target.clone());
                if capability.agent_form().writes {
                    prepared.delivery(
                        &self.root,
                        checks,
                        delivery
                            .as_ref()
                            .context("writing capability requires delivery")?,
                    )?;
                } else {
                    ensure!(
                        checks.is_empty() && delivery.is_none(),
                        "read-only capabilities cannot declare source delivery."
                    );
                }
            }
            Operation::ProjectQuery { requests } => {
                ensure!(requests.iter().any(|request| request.arguments.iter().any(|argument|
                    matches!(argument, super::batch::Argument::Literal(value) if value == &intent.target))),
                    "project query action must include the exact intent target.");
                prepared.report["plan"] = self.batch_manifest(
                    super::batch::Manifest {
                        schema: super::batch::SCHEMA.into(),
                        requests: requests.clone(),
                    },
                    action.report_bytes,
                    "intent",
                )?;
                prepared.targets.push(intent.target.clone());
            }
            Operation::SurfaceEdit {
                edit,
                to,
                checks,
                delivery,
            } => {
                let plan = self.edit_surface(&author::EditSurfaceOptions {
                    edit: edit.clone(),
                    to: to.clone(),
                    diff_bytes: action.diff_bytes,
                    write: false,
                })?;
                prepared.edits(&plan.edits)?;
                self.require_intent_path(
                    &intent.target,
                    &prepared
                        .changes
                        .iter()
                        .map(|change| change.path.clone())
                        .collect::<Vec<_>>(),
                )?;
                prepared.targets.push(intent.target.clone());
                prepared.report["plan"] = plan.report;
                prepared.delivery(&self.root, checks, delivery)?;
            }
            Operation::PropertyTask {} => {
                ensure!(
                    node.symbol.is_some(),
                    "property task intent requires a declaration target."
                );
                prepared.report["plan"] = serde_json::to_value(crate::spec::property_task(
                    &self.root,
                    &format!("{}::{}", node.path.display(), node.name),
                    intent.token_limit,
                )?)?;
                prepared.targets.push(intent.target.clone());
            }
            Operation::ProofTask { obligation } => {
                ensure!(
                    node.path
                        .extension()
                        .is_some_and(|extension| extension == "lean"),
                    "proof task intent must select a Lean goal or file."
                );
                ensure!(
                    node.symbol.is_none() || node.name == *obligation,
                    "proof obligation must equal the selected intent declaration."
                );
                prepared.report["plan"] = serde_json::to_value(crate::spec::proof_task(
                    &self.root,
                    &format!("{}::{obligation}", node.path.display()),
                    intent.token_limit,
                    false,
                )?)?;
                prepared.targets.push(intent.target.clone());
            }
            Operation::TaskChange { task_change: input } => {
                let task = self.task_change_bytes(
                    &serde_json::to_vec(input)?,
                    action.diff_bytes,
                    action.report_bytes,
                )?;
                prepared.targets = task.report["targets"]
                    .as_array()
                    .context("task action has no resolved targets")?
                    .iter()
                    .map(|row| {
                        row["handle"]
                            .as_str()
                            .map(str::to_owned)
                            .context("task action target has no handle")
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    prepared.targets.contains(&intent.target),
                    "resolved task action must include the intent target."
                );
                prepared.edits(&task.plan.edits)?;
                prepared.delivery(&self.root, &task.checks.checks, &task.delivery)?;
                prepared.report["plan"] = task.report;
                prepared.report["author"] = task.plan.report;
            }
            Operation::AuthorBatch {
                author_batch,
                checks,
                delivery,
            } => {
                prepared.targets = author_batch["operations"]
                    .as_array()
                    .context("author action needs operations")?
                    .iter()
                    .map(|row| {
                        row["handle"]
                            .as_str()
                            .map(str::to_owned)
                            .context("author action needs exact handles")
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    prepared.targets.contains(&intent.target),
                    "author action must include the intent target."
                );
                let manifest: author::BatchManifest = serde_json::from_value(author_batch.clone())?;
                let plan = self.author_batch_manifest(manifest, action.diff_bytes)?;
                prepared.edits(&plan.edits)?;
                prepared.report["plan"] = plan.report;
                prepared.delivery(&self.root, checks, delivery)?;
            }
            Operation::Recipe {
                recipe,
                checks,
                delivery,
            } => {
                let parsed = crate::recipe::parse(recipe)?;
                if let Some(id) = node.symbol {
                    ensure!(self.index.symbols.iter().filter(|symbol| symbol.name == node.name
                        && symbol.file == self.root.join(&node.path)).count() == 1,
                        "recipe cannot distinguish equal declaration names in one file; use exact task handles.");
                    for step in parsed.recipes.iter().flat_map(|recipe| &recipe.steps) {
                        let selected = crate::recipe::intent_step_symbols(
                            step,
                            self.index,
                            &crate::recipe::Options {
                                root: &self.root,
                                catalogs: &[],
                            },
                        )?;
                        ensure!(
                            selected == [id],
                            "declaration recipe must resolve exactly the intent target."
                        );
                    }
                    let path = node.path.to_string_lossy();
                    ensure!(parsed.recipes.iter().flat_map(|recipe| &recipe.steps).all(|step| {
                        let exact = |field: &str, value: &str| step.selector.iter().any(|predicate|
                            matches!(predicate, crate::recipe::Predicate::Equals { field: f, value: v } if f == field && v == value));
                        exact("name", &node.name) && exact("in", &path)
                    }), "declaration recipe steps must select the exact intent name and path.");
                }
                let sources = self
                    .scanned
                    .files
                    .iter()
                    .map(|file| {
                        (
                            file.path.clone(),
                            (file.language, self.sources[&file.path].clone()),
                        )
                    })
                    .collect::<crate::recipe::Sources>();
                struct ResetVfs;
                impl Drop for ResetVfs {
                    fn drop(&mut self) {
                        crate::vfs::use_filesystem();
                    }
                }
                let reset = ResetVfs;
                let (report, after) = crate::recipe::run_file(
                    &parsed,
                    sources.clone(),
                    &crate::recipe::Options {
                        root: &self.root,
                        catalogs: &[],
                    },
                )?;
                drop(reset);
                ensure!(
                    report.ok,
                    "intent recipe failed its expectations or requirements."
                );
                let mut edits = crate::edit::EditSet::new();
                for (path, (language, text)) in &after {
                    let before = sources
                        .get(path)
                        .map(|(_, text)| text.as_str())
                        .unwrap_or("");
                    if before != text {
                        edits.add(
                            path.clone(),
                            crate::edit::Edit::new(
                                crate::span::Span::new(0, before.len()),
                                text,
                                "recipe transaction",
                            ),
                        );
                        edits.declare_language(path.clone(), *language);
                    }
                }
                prepared.edits(&edits)?;
                self.require_intent_path(
                    &intent.target,
                    &prepared
                        .changes
                        .iter()
                        .map(|change| change.path.clone())
                        .collect::<Vec<_>>(),
                )?;
                prepared.targets.push(intent.target.clone());
                prepared.report["plan"] = serde_json::to_value(report)?;
                prepared.delivery(&self.root, checks, delivery)?;
            }
            Operation::FrameworkMigration {
                feature,
                to,
                out,
                register_with,
                dependency_manifest,
                dependency_requirement,
                cutover,
                checks,
                delivery,
            } => {
                let plan = self.migrate_feature(&migration::Options {
                    feature: feature.clone(),
                    to: *to,
                    out: out.clone(),
                    register_with: register_with.clone(),
                    dependency_manifest: dependency_manifest.clone(),
                    dependency_requirement: dependency_requirement.clone(),
                    checks: checks.clone(),
                    cutover: *cutover,
                    diff_bytes: action.diff_bytes,
                    write: false,
                })?;
                let source_paths = plan.report["migration"]["source_files"]
                    .as_array()
                    .context("migration has no source files")?
                    .iter()
                    .map(|path| {
                        path.as_str()
                            .map(|path| self.root.join(path))
                            .context("invalid migration source path")
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.require_intent_path(&intent.target, &source_paths)?;
                prepared.edits(&plan.edits)?;
                prepared
                    .changes
                    .extend(plan.connected_changes.into_iter().map(|change| Change {
                        path: change.path,
                        original: change.original,
                        updated: change.updated,
                    }));
                prepared
                    .removals
                    .extend(plan.source_removal.into_iter().map(|removal| Removal {
                        path: removal.path,
                        original: removal.original,
                    }));
                prepared.targets.push(intent.target.clone());
                prepared.report["plan"] = plan.report;
                prepared.delivery(&self.root, checks, delivery)?;
            }
            Operation::FormalPlan {
                properties,
                agent_properties,
                package,
                checks,
                delivery,
            } => {
                ensure!(
                    node.symbol.is_some(),
                    "formal plan intent requires a declaration target."
                );
                let target = format!("{}::{}", node.path.display(), node.name);
                let plan = crate::spec::formal_plan_from_agent_specs(
                    &self.root,
                    &target,
                    properties,
                    agent_properties,
                )?;
                prepared.targets.push(intent.target.clone());
                prepared.report["plan"] = serde_json::to_value(&plan)?;
                if let Some(package) = package {
                    let snapshot = self.stage_intent_proof_package(package, &[])?;
                    let snapshot_root = snapshot.path().canonicalize()?;
                    let initialized = crate::spec::init(&snapshot_root, package)?;
                    for file in initialized.files.into_iter().filter(|file| !file.existing) {
                        std::fs::create_dir_all(
                            file.path
                                .parent()
                                .context("initialization file has no parent")?,
                        )?;
                        crate::vfs::write(&file.path, file.content)?;
                        prepared.changes.push(Change {
                            path: self.root.join(file.path.strip_prefix(&snapshot_root)?),
                            original: String::new(),
                            updated: file.content.to_owned(),
                        });
                    }
                    let scaffold =
                        crate::spec::scaffold_formal_plan(snapshot.path(), plan, package)?;
                    for file in scaffold
                        .files
                        .into_iter()
                        .filter(|file| file.original != file.updated)
                    {
                        let path = self.root.join(file.path.strip_prefix(&snapshot_root)?);
                        if let Some(change) = prepared
                            .changes
                            .iter_mut()
                            .find(|change| change.path == path)
                        {
                            change.updated = file.updated;
                        } else {
                            prepared.changes.push(Change {
                                path,
                                original: file.original,
                                updated: file.updated,
                            });
                        }
                    }
                    prepared.delivery(
                        &self.root,
                        checks,
                        delivery
                            .as_ref()
                            .context("formal scaffold action requires delivery")?,
                    )?;
                    prepared.report["proof_validation"] =
                        self.validate_intent_proof_package(package, &prepared.changes)?;
                } else {
                    ensure!(
                        checks.is_empty() && delivery.is_none(),
                        "read-only formal plans cannot declare source delivery."
                    );
                }
            }
            Operation::ProofSubmission {
                obligation,
                tactics,
                checks,
                delivery,
            } => {
                ensure!(
                    node.path
                        .extension()
                        .is_some_and(|extension| extension == "lean"),
                    "proof intent must select a Lean goal or file."
                );
                ensure!(
                    node.symbol.is_none() || node.name == *obligation,
                    "proof obligation must equal the selected intent declaration."
                );
                ensure!(
                    tactics.len() <= 65_536,
                    "intent tactics exceed 65536 bytes."
                );
                let mut scratch = tempfile::Builder::new()
                    .prefix("fr-intent-tactics-")
                    .suffix(".txt")
                    .tempfile()?;
                scratch.write_all(tactics.as_bytes())?;
                scratch.flush()?;
                let target = format!("{}::{obligation}", node.path.display());
                let package = node
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .context("proof target has no package")?;
                let snapshot = self.stage_intent_proof_package(package, &[])?;
                let proof = crate::spec::prove(snapshot.path(), &target, scratch.path())?;
                prepared.report["proof"] = json!({"goal_id": proof.goal_id, "obligation": proof.obligation,
                    "proof_digest": proof.proof_digest, "receipt": proof.receipt});
                prepared.report["claims"]["model_theorem_checked"] = json!(true);
                prepared.changes.push(Change {
                    path: self
                        .root
                        .join(proof.spec.strip_prefix(snapshot.path().canonicalize()?)?),
                    original: proof.original,
                    updated: proof.updated,
                });
                prepared.targets.push(intent.target.clone());
                prepared.delivery(&self.root, checks, delivery)?;
                prepared.report["proof_validation"] =
                    self.validate_intent_proof_package(package, &prepared.changes)?;
            }
        }
        if let Some(binding) = &action.guide {
            let goal = super::agent_guide::normalized_goal(&serde_json::to_vec(&binding.goal)?)?;
            if prepared.delivery.is_some() {
                if let Some(checks) = goal["checks"]
                    .as_array()
                    .filter(|checks| !checks.is_empty())
                {
                    let expected = checks
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<BTreeSet<_>>();
                    let actual = prepared.report["checks"]["names"]
                        .as_array()
                        .context("guided action has no checks")?
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<BTreeSet<_>>();
                    ensure!(
                        expected == actual,
                        "intent checks differ from its navigator goal."
                    );
                }
                if !goal["delivery"].is_null() {
                    ensure!(
                        goal["delivery"] == prepared.report["delivery"],
                        "intent delivery differs from its navigator goal."
                    );
                }
            }
        }
        prepared.report["proof_expectation"] = serde_json::to_value(action.proof_expectation)?;
        ensure!(
            !matches!(action.proof_expectation, ProofExpectation::Model)
                || prepared.report["claims"]["model_theorem_checked"] == true,
            "model proof expectation requires an accepted agent-authored theorem."
        );
        let mut unique = BTreeSet::new();
        prepared
            .targets
            .retain(|target| unique.insert(target.clone()));
        for target in &prepared.targets {
            self.resolve_handle(target)?;
        }
        ensure!(
            prepared.targets.len() <= 32,
            "intent action exceeds 32 exact targets."
        );
        if prepared.delivery.is_some() {
            ensure!(
                !prepared.changes.is_empty() || !prepared.removals.is_empty(),
                "intent action produced no source change."
            );
        }
        Ok(prepared)
    }

    fn validate_guided_operation(
        &self,
        goal: &Value,
        guide: &Value,
        operation: &Operation,
        target: &str,
    ) -> Result<()> {
        let mut effective = goal.clone();
        if goal["kind"] == "automatic" {
            effective = match guide["route"]["id"].as_str() {
                Some("semantic-change") => json!({"kind":"semantic-change"}),
                Some("formalization") => json!({"kind":"formalize"}),
                Some("proof") => {
                    let node = &self.nodes[self.resolve_handle(target)?];
                    json!({"kind":"proof","obligation":node.name})
                }
                _ => effective,
            };
        }
        let goal = &effective;
        let admitted = match (goal["kind"].as_str(), operation) {
            (
                Some("capability"),
                Operation::Capability {
                    capability,
                    parameters,
                    range,
                    ..
                },
            ) => {
                let mut expected: std::collections::BTreeMap<String, String> =
                    serde_json::from_value(goal["parameters"].clone())?;
                let range_matches = if let Some(authored) = expected.remove("range") {
                    let (path, start, end) = crate::span::parse_range(&authored)?;
                    let node = &self.nodes[self.resolve_handle(target)?];
                    ensure!(
                        path == node.path,
                        "guided extraction range must use the selected path."
                    );
                    let source = &self.sources[&self.root.join(&path)];
                    let lines = crate::span::LineIndex::new(source);
                    for position in [start, end] {
                        let line = lines
                            .line_span(position.line)
                            .context("guided range line is absent")?;
                        ensure!(
                            position.col > 0
                                && position.col <= line.text(source).chars().count() + 1,
                            "guided range column is absent."
                        );
                    }
                    *range
                        == Some(crate::span::Span::new(
                            lines
                                .offset(start, source)
                                .context("guided range start is absent")?,
                            lines
                                .offset(end, source)
                                .context("guided range end is absent")?,
                        ))
                } else {
                    *range == serde_json::from_value(goal["range"].clone())?
                };
                goal["capability"] == serde_json::to_value(capability)?
                    && expected == *parameters
                    && range_matches
            }
            (Some("automatic"), Operation::ProjectQuery { .. }) => true,
            (Some("recipe"), Operation::Recipe { recipe, .. }) => {
                let parsed = crate::recipe::parse(recipe)?;
                parsed
                    .recipes
                    .iter()
                    .flat_map(|recipe| &recipe.steps)
                    .all(|step| {
                        step.operation.describe().split_whitespace().next() == goal["verb"].as_str()
                    })
            }
            (Some("semantic-scalar"), Operation::TaskChange { task_change })
            | (
                Some("semantic-scalar"),
                Operation::AuthorBatch {
                    author_batch: task_change,
                    ..
                },
            ) => task_change
                .get("targets")
                .or_else(|| task_change.get("operations"))
                .unwrap_or(&Value::Null)
                .as_array()
                .is_some_and(|targets| {
                    targets.len() == 1 && {
                        let scalar = &targets[0]["scalar"];
                        targets[0]["handle"] == target
                            && targets[0]["op"] == "edit-body-scalar"
                            && scalar["operation"] == goal["operation"]
                            && scalar["from"] == goal["from"]
                            && scalar["to"] == goal["to"]
                    }
                }),
            (Some("semantic-change" | "semantic-body"), Operation::TaskChange { task_change })
            | (
                Some("semantic-change" | "semantic-body"),
                Operation::AuthorBatch {
                    author_batch: task_change,
                    ..
                },
            ) => {
                let op = if goal["kind"] == "semantic-body" {
                    "replace-body-semantic"
                } else {
                    "edit-body-semantic"
                };
                task_change
                    .get("targets")
                    .or_else(|| task_change.get("operations"))
                    .unwrap_or(&Value::Null)
                    .as_array()
                    .is_some_and(|targets| {
                        targets.len() == 1
                            && targets[0]["handle"] == target
                            && targets[0]["op"] == op
                    })
            }
            (Some("surface-edit"), Operation::SurfaceEdit { .. }) => true,
            (Some("framework-migration"), Operation::FrameworkMigration { to, feature, .. }) => {
                goal["to"] == serde_json::to_value(to)?
                    && guide["route"]["evidence"]["compatible_features"]
                        .as_array()
                        .is_some_and(|features| features.iter().any(|item| item == feature))
            }
            (Some("formalize"), Operation::FormalPlan { .. } | Operation::PropertyTask {}) => true,
            (
                Some("proof"),
                Operation::ProofSubmission { obligation, .. } | Operation::ProofTask { obligation },
            ) => goal["obligation"] == *obligation,
            _ => false,
        };
        ensure!(
            admitted,
            "authored intent operation differs from its navigator goal."
        );
        Ok(())
    }

    fn require_intent_path(&self, target: &str, paths: &[PathBuf]) -> Result<()> {
        let node = &self.nodes[self.resolve_handle(target)?];
        ensure!(
            paths.iter().any(|path| if node.kind == "directory" {
                path.starts_with(self.root.join(&node.path))
            } else {
                path == &self.root.join(&node.path)
            }),
            "intent action source must include the selected intent path."
        );
        Ok(())
    }

    fn stage_intent_proof_package(
        &self,
        package: &Path,
        changes: &[Change],
    ) -> Result<tempfile::TempDir> {
        let package = package.strip_prefix(&self.root).unwrap_or(package);
        ensure!(
            !package.is_absolute()
                && package
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "intent proof package must be workspace-relative."
        );
        crate::spec::init(&self.root, package)?;
        let scratch = tempfile::tempdir()?;
        let mut total = 0usize;
        for (path, source) in &self.sources {
            total = total
                .checked_add(source.len())
                .context("proof snapshot size overflow")?;
            ensure!(total <= 67_108_864, "proof snapshot exceeds 64 MiB.");
            let dest = scratch.path().join(path.strip_prefix(&self.root)?);
            std::fs::create_dir_all(dest.parent().context("source has no parent")?)?;
            crate::vfs::write(dest, source)?;
        }
        fn copy_package(
            from: &Path,
            to: &Path,
            total: &mut usize,
            files: &mut usize,
        ) -> Result<()> {
            std::fs::create_dir_all(to)?;
            for entry in crate::vfs::read_dir(from)? {
                let name = entry.file_name().context("package entry has no name")?;
                if matches!(name.to_str(), Some(".lake" | ".git")) {
                    continue;
                }
                let metadata = std::fs::symlink_metadata(&entry)?;
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "proof package contains a symlink."
                );
                if metadata.is_dir() {
                    copy_package(&entry, &to.join(name), total, files)?;
                } else {
                    ensure!(
                        metadata.is_file(),
                        "proof package contains a non-regular file."
                    );
                    *files += 1;
                    *total = total
                        .checked_add(usize::try_from(metadata.len())?)
                        .context("proof snapshot size overflow")?;
                    ensure!(
                        *files <= 4096 && *total <= 67_108_864,
                        "proof snapshot exceeds its file or byte bound."
                    );
                    std::fs::copy(&entry, to.join(name))?;
                }
            }
            Ok(())
        }
        if self.root.join(package).try_exists()? {
            copy_package(
                &self.root.join(package),
                &scratch.path().join(package),
                &mut total,
                &mut 0,
            )?;
        } else {
            std::fs::create_dir_all(scratch.path().join(package))?;
        }
        for change in changes {
            let path = scratch.path().join(change.path.strip_prefix(&self.root)?);
            std::fs::create_dir_all(path.parent().context("proof change has no parent")?)?;
            crate::vfs::write(path, &change.updated)?;
        }
        Ok(scratch)
    }

    fn validate_intent_proof_package(&self, package: &Path, changes: &[Change]) -> Result<Value> {
        let package = package.strip_prefix(&self.root).unwrap_or(package);
        let scratch = self.stage_intent_proof_package(package, changes)?;
        fn digest_files(
            path: &Path,
            base: &Path,
            entries: &mut std::collections::BTreeMap<String, String>,
        ) -> Result<()> {
            for entry in crate::vfs::read_dir(path)? {
                if std::fs::symlink_metadata(&entry)?.is_dir() {
                    digest_files(&entry, base, entries)?;
                } else {
                    entries.insert(
                        entry.strip_prefix(base)?.to_string_lossy().into_owned(),
                        hex::encode(Sha256::digest(crate::vfs::read(&entry)?)),
                    );
                }
            }
            Ok(())
        }
        let mut staged = std::collections::BTreeMap::new();
        digest_files(scratch.path(), scratch.path(), &mut staged)?;
        let snapshot_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&staged)?));
        let inputs = vec![package.to_owned()];
        let strict = crate::spec::check_strict(scratch.path(), &inputs, false)?;
        ensure!(
            strict.ok(),
            "planned proof package failed strict source correspondence."
        );
        let verification = crate::spec::verify_planned_package(scratch.path(), &inputs)?;
        ensure!(
            verification.report.ok() && verification.packages.iter().all(|package| package.passed),
            "planned proof package failed Lake build: {}",
            verification
                .packages
                .iter()
                .filter(|package| !package.passed)
                .map(|package| package.output.chars().take(4096).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        );
        Ok(
            json!({"schema": "fr-intent-proof-validation-1", "checked_snapshot": "planned", "snapshot_sha256": snapshot_sha256,
            "warning_policy": "named-obligations-permitted",
            "strict_correspondence": true, "lake_build": true,
            "implementation_correspondence": false,
            "remaining_obligations": strict.debts.len()}),
        )
    }
}

pub(crate) fn review_basis(packet: &Value) -> Result<String> {
    let mut review = packet.clone();
    review["serialized_bytes"] = json!(0);
    review["action"]
        .as_object_mut()
        .context("intent review has no action")?
        .remove("basis");
    Ok(format!(
        "fraa2:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&(
            "fr-agent-action-review-2",
            review
        ),)?))
    ))
}

pub fn intent_review_mode(
    purpose: usize,
    operation: usize,
    complete: bool,
    writable: bool,
    write: bool,
    basis_supplied: bool,
    basis_matches: bool,
) -> usize {
    if !intent_action_purpose_allowed(purpose, operation) || !complete {
        return 2;
    }
    match (writable, write, basis_supplied, basis_matches) {
        (_, false, false, _) => 0,
        (true, true, true, true) => 1,
        _ => 2,
    }
}

#[allow(clippy::too_many_arguments)] // Keep the audited Rust/Python/Lean scalar signature identical.
pub fn intent_review_complete(
    target_count: usize,
    evidence_count: usize,
    checks_declared: bool,
    writable: bool,
    proof_required: bool,
    proof_checked: bool,
    implementation_requested: bool,
    implementation_evidence: bool,
    diff_complete: bool,
) -> bool {
    (1..=32).contains(&target_count)
        && evidence_count == target_count - 1
        && (!writable || checks_declared)
        && (!proof_required || proof_checked)
        && (!implementation_requested || implementation_evidence)
        && diff_complete
}
