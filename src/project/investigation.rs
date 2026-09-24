use super::{hash, Project};
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const ANALYZER: &str = "fr-investigation-2";

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    from: PathBuf,
    #[arg(long)]
    transition: Option<String>,
    #[arg(long, requires_all = ["checks_digest", "check_step"])]
    checks_from: Option<PathBuf>,
    #[arg(long, requires = "checks_from")]
    checks_digest: Option<String>,
    #[arg(long, requires = "checks_from")]
    check_step: Option<String>,
    #[arg(long, requires_all = ["proofs_digest", "proof_step"], conflicts_with = "checks_from")]
    proofs_from: Option<PathBuf>,
    #[arg(long, requires = "proofs_from")]
    proofs_digest: Option<String>,
    #[arg(long, requires = "proofs_from")]
    proof_step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: String,
    pub goal: String,
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub hypotheses: Vec<String>,
    #[serde(default)]
    pub questions: Vec<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub question: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub inputs: Vec<Dependency>,
    #[serde(default)]
    pub state: State,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub required_checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_proofs: Vec<crate::spec::retained::Requirement>,
    #[serde(default)]
    pub satisfies: Vec<String>,
    #[serde(default)]
    pub action: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_input: Option<Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    #[default]
    Pending,
    Ready,
    Running,
    Satisfied,
    Blocked,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub key: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyKind {
    Source,
    Configuration,
    Lookup,
    Analyzer,
    Workspace,
    CheckConfiguration,
    CheckSources,
    CheckToolchain,
    DeclarationAnalyzer,
    ProofInputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub kind: EvidenceKind,
    pub input_digest: String,
    pub passed: bool,
    pub reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    Observation,
    Check,
    ModelProof,
    SourceCorrespondence,
}

pub fn transition_allowed(from: State, to: State, prerequisites: bool, evidence: bool) -> bool {
    match (from, to) {
        (State::Ready, State::Running) => prerequisites,
        (State::Running, State::Satisfied) => prerequisites && evidence,
        (State::Ready | State::Running, State::Blocked) => true,
        (_, State::Pending) => true,
        _ => false,
    }
}

pub fn check_scope_covered(
    workspace: bool,
    configuration: bool,
    sources: bool,
    toolchain: bool,
) -> bool {
    workspace && configuration && sources && toolchain
}

impl Project<'_> {
    fn dependency_digest(&self, dependency: &Dependency) -> Result<String> {
        Ok(match dependency.kind {
            DependencyKind::Analyzer => hash((
                ANALYZER,
                env!("CARGO_PKG_VERSION"),
                include_str!("investigation.rs"),
            ))?,
            DependencyKind::DeclarationAnalyzer => super::correspondence::analyzer_digest()?,
            DependencyKind::ProofInputs => {
                match crate::spec::retained::input_digest(&self.root, &dependency.key) {
                    Ok(digest) => digest,
                    Err(error) if dependency.digest.is_some() => {
                        hash(("unavailable-proof-inputs", error.to_string()))?
                    }
                    Err(error) => return Err(error),
                }
            }
            DependencyKind::Workspace => self.revision.clone(),
            DependencyKind::CheckConfiguration => {
                crate::checks::evidence::configuration_digest(&self.root)?
            }
            DependencyKind::CheckSources => crate::history::source_revision(&self.root)?,
            DependencyKind::CheckToolchain => {
                crate::checks::evidence::toolchain_digest(&self.root)?
            }
            DependencyKind::Lookup => {
                let mut candidates = self
                    .index
                    .symbols
                    .iter()
                    .filter(|s| s.name == dependency.key)
                    .map(|s| {
                        (
                            s.file
                                .strip_prefix(&self.root)
                                .unwrap_or(&s.file)
                                .to_path_buf(),
                            s.name_span,
                            s.kind,
                        )
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by_cached_key(|item| format!("{item:?}"));
                hash(candidates)?
            }
            DependencyKind::Source | DependencyKind::Configuration => {
                let path = std::path::Path::new(&dependency.key);
                ensure!(
                    !path.is_absolute()
                        && path.components().all(|c| matches!(
                            c,
                            std::path::Component::Normal(_) | std::path::Component::CurDir
                        )),
                    "dependency path must stay in the workspace."
                );
                let path = self.root.join(path);
                ensure!(self.sources.contains_key(&path) || self.manifests.snapshots.contains_key(&path)
                    || self.lockfiles.snapshots.contains_key(&path) || !path.try_exists()?,
                    "dependency exists outside the indexed snapshot; declare an admitted snapshot input.");
                hash((
                    self.sources.get(&path),
                    self.manifests.snapshots.get(&path),
                    self.lockfiles.snapshots.get(&path),
                ))?
            }
        })
    }

    pub(super) fn investigation(&self, options: &Options) -> Result<Value> {
        ensure!(
            std::fs::metadata(&options.from)?.len() <= 1_048_576,
            "plan exceeds 1 MiB"
        );
        let mut plan: Plan = serde_json::from_slice(&crate::vfs::read(&options.from)?)?;
        ensure!(
            plan.schema == "fr-investigation-plan-1",
            "unsupported plan schema"
        );
        ensure!(
            !plan.goal.trim().is_empty() && !plan.acceptance.is_empty(),
            "goal and acceptance criteria are required."
        );
        ensure!(plan.steps.len() <= 256, "plan exceeds 256 steps");
        let mut ids = BTreeSet::new();
        for step in &plan.steps {
            ensure!(
                step.action_input.as_ref().is_none_or(Value::is_object),
                "task action input must be an object"
            );
            ensure!(
                !step.id.is_empty() && ids.insert(step.id.clone()),
                "duplicate or empty step identity"
            );
            ensure!(
                !step.inputs.is_empty(),
                "step {} needs explicit input dependencies.",
                step.id
            );
            ensure!(
                step.required_proofs.len() <= 64,
                "step exceeds 64 required proofs."
            );
            let mut proofs = BTreeSet::new();
            for proof in &step.required_proofs {
                ensure!(crate::spec::retained::requirement_valid(proof) && proofs.insert(proof_id(proof)?),
                    "required proofs need distinct, normalized package, module and theorem identities.");
                ensure!(
                    step.inputs
                        .iter()
                        .any(|input| input.kind == DependencyKind::ProofInputs
                            && input.key == proof.package),
                    "required proofs need a proof-inputs dependency for their package."
                );
            }
            ensure!(
                step.satisfies.iter().all(|c| plan.acceptance.contains(c)),
                "unknown acceptance criterion"
            );
        }
        let mut order = Vec::new();
        let mut visited = BTreeSet::new();
        while order.len() < plan.steps.len() {
            let before = order.len();
            for (i, step) in plan.steps.iter().enumerate() {
                ensure!(
                    step.depends_on.iter().all(|id| ids.contains(id)),
                    "unknown prerequisite"
                );
                if !visited.contains(&step.id)
                    && step.depends_on.iter().all(|id| visited.contains(id))
                {
                    visited.insert(step.id.clone());
                    order.push(i);
                }
            }
            ensure!(before < order.len(), "task dependency cycle");
        }
        let event = options
            .transition
            .as_deref()
            .map(|s| s.split_once(':').context("transition must be STEP:EVENT"))
            .transpose()?;
        if let Some((id, _)) = event {
            ensure!(ids.contains(id), "unknown transition step");
        }
        let check_attachment = if let Some(path) = &options.checks_from {
            ensure!(
                std::fs::metadata(path)?.len() <= 1_048_576,
                "check report exceeds 1 MiB"
            );
            let report: Value = serde_json::from_slice(&crate::vfs::read(path)?)?;
            let digest = hash(&report)?;
            ensure!(
                Some(&digest) == options.checks_digest.as_ref(),
                "check report digest mismatch"
            );
            let step = options
                .check_step
                .as_deref()
                .context("check step is required")?;
            ensure!(ids.contains(step), "unknown check attachment step");
            Some((
                digest,
                crate::checks::evidence::validate(&self.root, &report)?,
            ))
        } else {
            None
        };
        let proof_attachment = if let Some(path) = &options.proofs_from {
            ensure!(
                std::fs::metadata(path)?.len() <= 1_048_576,
                "proof report exceeds 1 MiB."
            );
            let report: Value = serde_json::from_slice(&crate::vfs::read(path)?)?;
            let digest = hash(&report)?;
            ensure!(
                Some(&digest) == options.proofs_digest.as_ref(),
                "proof report digest mismatch."
            );
            ensure!(
                options
                    .proof_step
                    .as_ref()
                    .is_some_and(|step| ids.contains(step)),
                "unknown proof attachment step."
            );
            Some((
                digest,
                crate::spec::retained::validate(&self.root, &report)?,
            ))
        } else {
            None
        };
        let mut states = BTreeMap::new();
        let mut bases = BTreeMap::new();
        let mut invalidated = Vec::new();
        for i in order {
            let step = &mut plan.steps[i];
            let prerequisites = step
                .depends_on
                .iter()
                .all(|id| states[id] == State::Satisfied);
            let original_state = step.state;
            let reset = event == Some((step.id.as_str(), "reset"));
            let mut changed = false;
            for input in &mut step.inputs {
                let current = self.dependency_digest(input)?;
                if reset || (input.digest.is_none() && step.state == State::Pending) {
                    input.digest = Some(current);
                } else if input.digest.as_ref() != Some(&current) {
                    changed = true;
                }
            }
            let parent_bases = step
                .depends_on
                .iter()
                .map(|id| &bases[id])
                .collect::<Vec<_>>();
            let basis = hash((
                ANALYZER,
                &plan.goal,
                &plan.acceptance,
                &step.id,
                &step.question,
                &step.inputs,
                parent_bases,
                &step.required_checks,
                &step.satisfies,
                &step.action,
                &step.action_input,
            ))?;
            let basis = if step.required_proofs.is_empty() {
                basis
            } else {
                hash((basis, &step.required_proofs))?
            };
            if options.check_step.as_deref() == Some(&step.id) {
                let has = |kind| step.inputs.iter().any(|input| input.kind == kind);
                ensure!(check_scope_covered(has(DependencyKind::Workspace), has(DependencyKind::CheckConfiguration),
                    has(DependencyKind::CheckSources), has(DependencyKind::CheckToolchain)),
                    "checked steps require workspace, check-configuration, check-sources and check-toolchain dependencies.");
                ensure!(
                    !changed
                        && !reset
                        && prerequisites
                        && matches!(step.state, State::Ready | State::Running),
                    "check evidence requires a current ready or running step."
                );
                let (digest, results) = check_attachment.as_ref().unwrap();
                for (name, passed) in results {
                    ensure!(
                        step.required_checks.contains(name),
                        "attached check is not required by this step."
                    );
                    step.evidence
                        .retain(|e| e.kind != EvidenceKind::Check || e.id != *name);
                    step.evidence.push(Evidence {
                        id: name.clone(),
                        kind: EvidenceKind::Check,
                        input_digest: basis.clone(),
                        passed: *passed,
                        reference: format!("fr-check-report-1:{digest}"),
                    });
                }
            }
            if options.proof_step.as_deref() == Some(&step.id) {
                ensure!(
                    !changed
                        && !reset
                        && prerequisites
                        && matches!(step.state, State::Ready | State::Running),
                    "proof evidence requires a current ready or running step."
                );
                let (digest, results) = proof_attachment.as_ref().unwrap();
                let selected = step
                    .required_proofs
                    .iter()
                    .filter(|proof| {
                        results
                            .iter()
                            .any(|(package, _, _)| *package == proof.package)
                    })
                    .collect::<Vec<_>>();
                ensure!(
                    !selected.is_empty(),
                    "proof report covers no required theorem."
                );
                for proof in selected {
                    ensure!(
                        results.contains(&(
                            proof.package.clone(),
                            proof.spec.clone(),
                            proof.theorem.clone()
                        )),
                        "required theorem is missing from the checked package."
                    );
                    let id = proof_id(proof)?;
                    step.evidence
                        .retain(|e| e.kind != EvidenceKind::ModelProof || e.id != id);
                    step.evidence.push(Evidence {
                        id,
                        kind: EvidenceKind::ModelProof,
                        input_digest: basis.clone(),
                        passed: true,
                        reference: format!("fr-proof-evidence-1:{digest}"),
                    });
                }
            }
            if reset {
                step.state = State::Pending;
                step.evidence.clear();
            }
            let dependent_stale = step.depends_on.iter().any(|id| states[id] == State::Stale);
            let evidence_stale = step.evidence.iter().any(|e| e.input_digest != basis);
            if changed || dependent_stale || evidence_stale {
                step.state = State::Stale;
                invalidated.push(step.id.clone());
            } else if matches!(step.state, State::Pending | State::Ready | State::Running) {
                step.state = if prerequisites {
                    State::Ready
                } else {
                    State::Pending
                };
            }
            let evidence_ok = !step.evidence.is_empty()
                && step
                    .evidence
                    .iter()
                    .all(|e| e.passed && e.input_digest == basis && !e.reference.is_empty())
                && step.required_proofs.iter().all(|proof| {
                    proof_id(proof).is_ok_and(|id| {
                        step.evidence.iter().any(|e| {
                            e.kind == EvidenceKind::ModelProof
                                && e.id == id
                                && e.passed
                                && e.reference.starts_with("fr-proof-evidence-1:")
                        })
                    })
                })
                && step.required_checks.iter().all(|check| {
                    step.evidence
                        .iter()
                        .any(|e| e.kind == EvidenceKind::Check && &e.id == check && e.passed)
                });
            if let Some((id, action)) = event {
                if id == step.id && action != "reset" {
                    let to = match action {
                        "start" => State::Running,
                        "satisfy" => State::Satisfied,
                        "block" => State::Blocked,
                        _ => bail!("unknown transition event"),
                    };
                    let from = if action == "satisfy"
                        && original_state == State::Running
                        && step.state == State::Ready
                    {
                        State::Running
                    } else {
                        step.state
                    };
                    ensure!(
                        transition_allowed(from, to, prerequisites, evidence_ok),
                        "step transition refused: {}",
                        step.id
                    );
                    step.state = to;
                }
            }
            if step.state == State::Satisfied && (!prerequisites || !evidence_ok) {
                step.state = State::Stale;
                invalidated.push(step.id.clone());
            }
            bases.insert(step.id.clone(), basis);
            states.insert(step.id.clone(), step.state);
        }
        let complete = plan.questions.is_empty()
            && plan.acceptance.iter().all(|criterion| {
                plan.steps.iter().any(|step| {
                    step.state == State::Satisfied && step.satisfies.contains(criterion)
                })
            });
        Ok(
            json!({"schema": "fr-investigation-resume-1", "revision": self.revision, "handle_prefix": format!("frp1:{}:", &self.revision[..32]), "coverage": self.coverage(),
            "plan": plan, "input_digests": bases, "invalidated": invalidated, "complete": complete,
            "claim": "validated agent-reported evidence; references do not attest tool execution.",
            "check_attachment": check_attachment.map(|(digest, results)| json!({"digest":digest,"results":results,
                "trust":"trusted caller digest binds retained command output, not an execution attestation"})),
            "proof_attachment": proof_attachment.map(|(digest, results)| json!({"digest":digest,"results":results,
                "trust":"Trusted caller digest binds retained local Lean output. No execution attestation or source implementation proof."})),
            "mutation_authority": false, "dependency_policy": "explicit inputs; use workspace dependency when coverage is uncertain."}),
        )
    }
}

fn proof_id(proof: &crate::spec::retained::Requirement) -> Result<String> {
    Ok(format!(
        "fr-model-proof-1:{}",
        hash((&proof.package, &proof.spec, &proof.theorem))?
    ))
}
