//! Local investigation plans. Plan claims never authorize mutation or attest tool execution.
use super::{hash, Project};
use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const ANALYZER: &str = "fr-investigation-1";

#[derive(Args)]
pub struct Options {
    /// JSON plan to validate and resume; the updated plan is returned without writing it.
    #[arg(long)]
    from: PathBuf,
    /// Explicit step transition: STEP:start, STEP:satisfy, STEP:block, or STEP:reset.
    #[arg(long)]
    transition: Option<String>,
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
    #[serde(default)]
    pub satisfies: Vec<String>,
    /// Existing guide/discovery arguments. Stored as data; never executed by resumption.
    #[serde(default)]
    pub action: Vec<String>,
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
    /// None captures an input only for a new pending step or an explicit reset.
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyKind {
    Source,
    Configuration,
    Lookup,
    Analyzer,
    Workspace,
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

/// Small admission kernel: stale prerequisites or missing acceptance evidence refuse completion.
pub fn transition_allowed(from: State, to: State, prerequisites: bool, evidence: bool) -> bool {
    match (from, to) {
        (State::Ready, State::Running) => prerequisites,
        (State::Running, State::Satisfied) => prerequisites && evidence,
        (State::Ready | State::Running, State::Blocked) => true,
        (_, State::Pending) => true,
        _ => false,
    }
}

impl Project<'_> {
    fn dependency_digest(&self, dependency: &Dependency) -> Result<String> {
        Ok(match dependency.kind {
            DependencyKind::Analyzer => hash((ANALYZER, env!("CARGO_PKG_VERSION")))?,
            DependencyKind::Workspace => self.revision.clone(),
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
                    "dependency path must stay in the workspace"
                );
                let path = self.root.join(path);
                // Only bytes already admitted into the selected snapshot may be dependencies.
                ensure!(self.sources.contains_key(&path) || self.manifests.snapshots.contains_key(&path)
                    || self.lockfiles.snapshots.contains_key(&path) || !path.try_exists()?,
                    "dependency exists outside the indexed snapshot; use a workspace dependency and an external check");
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
        let mut plan: Plan = serde_json::from_slice(&std::fs::read(&options.from)?)?;
        ensure!(
            plan.schema == "fr-investigation-plan-1",
            "unsupported plan schema"
        );
        ensure!(
            !plan.goal.trim().is_empty() && !plan.acceptance.is_empty(),
            "goal and acceptance criteria are required"
        );
        ensure!(plan.steps.len() <= 256, "plan exceeds 256 steps");
        let mut ids = BTreeSet::new();
        for step in &plan.steps {
            ensure!(
                !step.id.is_empty() && ids.insert(step.id.clone()),
                "duplicate or empty step identity"
            );
            ensure!(
                !step.inputs.is_empty(),
                "step {} needs explicit input dependencies",
                step.id
            );
            ensure!(
                step.satisfies.iter().all(|c| plan.acceptance.contains(c)),
                "unknown acceptance criterion"
            );
        }
        // Topological order makes dependency propagation independent of JSON ordering.
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
            let basis = hash((ANALYZER, &step.inputs, parent_bases))?;
            if reset {
                step.state = State::Pending;
                step.evidence.clear();
            }
            let dependent_stale = step.depends_on.iter().any(|id| states[id] == State::Stale);
            let evidence_stale = step.evidence.iter().any(|e| e.input_digest != basis);
            if changed || dependent_stale || evidence_stale {
                step.state = State::Stale;
                invalidated.push(step.id.clone());
            } else if step.state == State::Running {
                // A reopened process cannot attest that the interrupted operation completed.
                step.state = if prerequisites {
                    State::Ready
                } else {
                    State::Pending
                };
            } else if matches!(step.state, State::Pending | State::Ready) {
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
                    // A supplied running record may complete only after all inputs were revalidated.
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
            "claim": "validated agent-reported evidence; references do not attest tool execution",
            "mutation_authority": false, "dependency_policy": "explicit inputs; use workspace dependency when coverage is uncertain"}),
        )
    }
}
