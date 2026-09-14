use crate::edit::{Edit, EditSet};
use crate::extract::Extractor;
use crate::lang::detect;
use crate::parse::Parsers;
use crate::span::Span;
use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub const LEAN_TOOLCHAIN: &str = "leanprover/lean4:v4.28.0";

const LAKEFILE: &str = r#"name = "fr-specs"
version = "0.1.0"
defaultTargets = ["FrSpecs"]

[[lean_lib]]
name = "FrSpecs"
"#;

const ROOT_MODULE: &str = r#"/-
This is the checked root of the project's Lean specification package.
Import each model here so `fr spec verify` builds it.
-/

namespace FrSpecs

end FrSpecs
"#;

#[derive(Debug, Serialize)]
pub struct InitPlan {
    pub package: PathBuf,
    pub toolchain: &'static str,
    pub files: Vec<InitFile>,
}

#[derive(Debug, Serialize)]
pub struct InitFile {
    pub path: PathBuf,
    pub content: &'static str,
    pub existing: bool,
}

#[derive(Debug)]
pub struct ScaffoldPlan {
    pub source: PathBuf,
    pub symbol: String,
    pub model: String,
    pub module: String,
    pub hash: String,
    pub regenerated: bool,
    pub handwritten_bytes: usize,
    pub files: Vec<ScaffoldFile>,
}

#[derive(Debug)]
pub struct ScaffoldFile {
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
}

#[derive(Debug)]
pub struct CiPlan {
    pub package: PathBuf,
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
    pub max_debt: usize,
}

#[derive(Debug, Serialize)]
pub struct Evidence {
    pub schema: u8,
    pub verification: Verification,
    pub properties: Vec<PropertyReport>,
    pub declared_assumptions: Vec<DeclaredAssumption>,
    pub axiom_analysis: &'static str,
    pub trusted_components: Vec<&'static str>,
    pub correspondence: CorrespondenceEvidence,
    pub remaining_obligations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PropertyReport {
    pub spec: PathBuf,
    pub line: usize,
    pub name: String,
    pub kind: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DeclaredAssumption {
    pub spec: PathBuf,
    pub line: usize,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct CorrespondenceEvidence {
    pub source_identity: &'static str,
    pub signature_surface: &'static str,
    pub tested_implementation_model: bool,
    pub proved_implementation_model: bool,
}

pub const FORMAL_PLAN_SCHEMA: &str = "fr-formal-plan-1";
pub const FORMAL_GOALS_SCHEMA: &str = "fr-formal-goals-1";
pub const PROOF_TASK_SCHEMA: &str = "fr-proof-task-1";
pub const PROOF_ATTEMPT_SCHEMA: &str = "fr-proof-attempt-1";
pub const PROPERTY_TASK_SCHEMA: &str = "fr-property-task-1";
pub const AGENT_PROPERTY_SCHEMA: &str = "fr-formal-property-1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalPlan {
    pub schema: String,
    pub target: FormalTarget,
    pub kernel: FormalKernel,
    pub properties: Vec<FormalProperty>,
    pub correspondence: FormalCorrespondence,
    pub assumptions: Vec<String>,
    pub obligations: Vec<String>,
    pub object_digest: String,
    pub actions: FormalActions,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalTarget {
    pub source: PathBuf,
    pub symbol: String,
    pub source_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalKernel {
    pub module: String,
    pub model: String,
    pub inputs: Vec<FormalBinding>,
    pub output: FormalBinding,
    pub semantic_ir: serde_json::Value,
    pub lean_definition: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalBinding {
    pub name: String,
    pub rust_type: String,
    pub lean_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalProperty {
    pub kind: String,
    pub name: String,
    pub proposition: String,
    pub proof_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_spec: Option<AgentProperty>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProperty {
    pub schema: String,
    pub task_digest: String,
    pub name: String,
    pub parameters: Vec<AgentPropertyParameter>,
    pub proposition: AgentProposition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPropertyParameter {
    pub name: String,
    pub lean_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AgentTerm {
    Variable {
        name: String,
    },
    Model {
        arguments: Vec<AgentTerm>,
    },
    Boolean {
        value: bool,
    },
    Integer {
        value: i64,
        lean_type: String,
    },
    Unary {
        operator: String,
        operand: Box<AgentTerm>,
    },
    Binary {
        operator: String,
        left: Box<AgentTerm>,
        right: Box<AgentTerm>,
    },
    If {
        condition: Box<AgentTerm>,
        then: Box<AgentTerm>,
        otherwise: Box<AgentTerm>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AgentProposition {
    Equals {
        left: AgentTerm,
        right: AgentTerm,
    },
    NotEquals {
        left: AgentTerm,
        right: AgentTerm,
    },
    LessThan {
        left: AgentTerm,
        right: AgentTerm,
    },
    LessOrEqual {
        left: AgentTerm,
        right: AgentTerm,
    },
    GreaterThan {
        left: AgentTerm,
        right: AgentTerm,
    },
    GreaterOrEqual {
        left: AgentTerm,
        right: AgentTerm,
    },
    Holds {
        term: AgentTerm,
    },
    Not {
        proposition: Box<AgentProposition>,
    },
    And {
        propositions: Vec<AgentProposition>,
    },
    Or {
        propositions: Vec<AgentProposition>,
    },
    Implies {
        premise: Box<AgentProposition>,
        conclusion: Box<AgentProposition>,
    },
}

#[derive(Debug, Serialize)]
pub struct PropertyTask {
    pub schema: &'static str,
    pub target: FormalTarget,
    pub kernel: PropertyKernelContext,
    pub contract: PropertyContract,
    pub templates: Vec<PropertyTemplate>,
    pub object_digest: String,
    pub actions: PropertyTaskActions,
    pub token_budget: FormalGoalBudget,
}

#[derive(Debug, Serialize)]
pub struct PropertyKernelContext {
    pub model: String,
    pub inputs: Vec<FormalBinding>,
    pub output: FormalBinding,
}

#[derive(Debug, Serialize)]
pub struct PropertyContract {
    pub author: &'static str,
    pub format: &'static str,
    pub proof_author: &'static str,
    pub allowed_terms: Vec<&'static str>,
    pub allowed_propositions: Vec<&'static str>,
    pub allowed_unary_operators: Vec<&'static str>,
    pub allowed_binary_operators: Vec<&'static str>,
    pub max_parameters: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
}

#[derive(Debug, Serialize)]
pub struct PropertyTemplate {
    pub kind: &'static str,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PropertyTaskActions {
    pub plan: Vec<String>,
    pub scaffold: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalCorrespondence {
    pub source_identity: String,
    pub signature_surface: String,
    pub model_generation: String,
    pub implementation_model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormalActions {
    pub scaffold: Vec<String>,
    pub goals: Vec<String>,
    pub verify: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FormalCandidates {
    pub schema: &'static str,
    pub candidates: Vec<FormalCandidate>,
    pub omitted: usize,
    pub limits: FormalLimits,
}

#[derive(Debug, Serialize)]
pub struct FormalCandidate {
    pub target: String,
    pub source_hash: String,
    pub eligible: bool,
    pub reason: String,
    pub suggested_properties: Vec<&'static str>,
    pub action: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FormalLimits {
    pub returned: usize,
    pub limit: usize,
}

#[derive(Debug)]
pub struct FormalScaffoldPlan {
    pub plan_digest: String,
    pub source: PathBuf,
    pub symbol: String,
    pub model: String,
    pub module: String,
    pub obligations: usize,
    pub regenerated: bool,
    pub proof_bytes_preserved: usize,
    pub files: Vec<ScaffoldFile>,
}

#[derive(Debug, Serialize)]
pub struct FormalGoals {
    pub schema: &'static str,
    pub commitment: FormalGoalCommitment,
    pub catalog: Vec<FormalGoalCatalog>,
    pub omitted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revealed: Option<FormalGoalDetail>,
    pub token_budget: FormalGoalBudget,
}

#[derive(Debug, Serialize)]
pub struct FormalGoalCommitment {
    pub object_schema: &'static str,
    pub object_root: String,
}

#[derive(Debug, Serialize)]
pub struct FormalGoalCatalog {
    pub id: String,
    pub name: String,
    pub spec: PathBuf,
    pub line: usize,
    pub object_digest: String,
    pub reveal: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormalGoalDetail {
    pub id: String,
    pub name: String,
    pub spec: PathBuf,
    pub line: usize,
    pub theorem: String,
    pub source_anchor: Option<String>,
    pub signature_map: Option<String>,
    pub proof_region: String,
    pub object_digest: String,
    pub prove_template: Vec<String>,
    pub verify: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FormalGoalBudget {
    pub limit: usize,
    pub used_upper_bound: usize,
    pub measurement: &'static str,
}

#[derive(Debug)]
pub struct ProofPlan {
    pub spec: PathBuf,
    pub obligation: String,
    pub original: String,
    pub updated: String,
    pub goal_id: String,
    pub proof_digest: String,
    pub receipt: String,
}

#[derive(Debug, Serialize)]
pub struct ProofTask {
    pub schema: &'static str,
    pub goal: FormalGoalDetail,
    pub contract: ProofInputContract,
    pub templates: Vec<ProofTemplate>,
    pub object_digest: String,
    pub actions: ProofTaskActions,
    pub token_budget: FormalGoalBudget,
}

#[derive(Debug, Serialize)]
pub struct ProofInputContract {
    pub author: &'static str,
    pub format: &'static str,
    pub insertion_point: &'static str,
    pub normalization: &'static str,
    pub forbidden: Vec<&'static str>,
    pub checker: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ProofTemplate {
    pub kind: &'static str,
    pub lines: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ProofTaskActions {
    pub check: Vec<String>,
    pub apply: Vec<String>,
    pub verify: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProofAttempt {
    pub schema: &'static str,
    pub goal_id: String,
    pub proof_digest: String,
    pub checker: &'static str,
    pub passed: bool,
    pub diagnostics: Vec<ProofDiagnostic>,
    pub diagnostics_omitted: usize,
    pub receipt: Option<String>,
    pub actions: ProofAttemptActions,
    pub token_budget: FormalGoalBudget,
}

#[derive(Debug, Serialize)]
pub struct ProofDiagnostic {
    pub severity: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub context: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProofAttemptActions {
    pub revise: Vec<String>,
    pub apply: Option<Vec<String>>,
    pub verify: Vec<String>,
}

pub fn formal_candidate_admitted(
    source_is_rust: bool,
    top_level: bool,
    typed: bool,
    pure: bool,
    body_supported: bool,
) -> bool {
    source_is_rust && top_level && typed && pure && body_supported
}

pub fn formal_property_admitted(
    known_kind: bool,
    one_input: bool,
    input_matches_output: bool,
    boolean_surface: bool,
) -> bool {
    known_kind && one_input && (input_matches_output || boolean_surface)
}

pub fn proof_submission_admitted(
    tactics_only: bool,
    nonempty: bool,
    within_limit: bool,
    no_placeholders: bool,
    unique_region: bool,
    syntax_valid: bool,
    lean_passed: bool,
) -> bool {
    tactics_only
        && nonempty
        && within_limit
        && no_placeholders
        && unique_region
        && syntax_valid
        && lean_passed
}

pub fn agent_property_admitted(
    schema_matches: bool,
    task_matches: bool,
    safe_names: bool,
    types_disclosed: bool,
    within_limits: bool,
    terms_well_typed: bool,
    proposition_well_typed: bool,
) -> bool {
    schema_matches
        && task_matches
        && safe_names
        && types_disclosed
        && within_limits
        && terms_well_typed
        && proposition_well_typed
}

pub fn agent_term_operator_admitted(operator: u8, operand_type: u8) -> bool {
    match operator {
        0 => operand_type == 0,
        1 => operand_type == 2,
        2..=4 => matches!(operand_type, 1 | 2),
        5 | 6 => operand_type == 0,
        _ => false,
    }
}

pub fn agent_relation_admitted(relation: u8, left_type: u8, right_type: u8) -> bool {
    match relation {
        0 | 1 => left_type < 4 && left_type == right_type,
        2..=5 => matches!(left_type, 1 | 2) && left_type == right_type,
        6 => left_type == 0,
        _ => false,
    }
}

pub fn init(root: &Path, requested: &Path) -> Result<InitPlan> {
    let root = root
        .canonicalize()
        .with_context(|| format!("reading workspace root {}", root.display()))?;
    if !root.is_dir() {
        bail!("spec initialization requires a workspace directory.");
    }
    let relative = if requested.is_absolute() {
        requested.strip_prefix(&root).with_context(|| {
            format!(
                "spec package {} must stay inside {}.",
                requested.display(),
                root.display()
            )
        })?
    } else {
        requested
    };
    if relative.as_os_str().is_empty()
        || relative.components().any(|part| {
            !matches!(part, Component::Normal(_))
                || matches!(part.as_os_str().to_str(), Some(".git" | ".fr-history"))
        })
    {
        bail!("invalid spec package path {}.", requested.display());
    }

    let package = root.join(relative);
    let mut current = root.clone();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "spec package path traverses a symlink: {}.",
                    current.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "spec package path is not a directory: {}.",
                    current.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    let definitions = [
        ("lean-toolchain", concat!("leanprover/lean4:v4.28.0", "\n")),
        ("lakefile.toml", LAKEFILE),
        ("FrSpecs.lean", ROOT_MODULE),
    ];
    let mut files = Vec::with_capacity(definitions.len());
    for (name, content) in definitions {
        let path = package.join(name);
        let existing = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                let current = crate::vfs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                if current != content && name != "FrSpecs.lean" {
                    bail!(
                        "refusing to replace existing spec package file {}.",
                        path.display()
                    );
                }
                true
            }
            Ok(_) => bail!(
                "spec package target is not a regular file: {}.",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        files.push(InitFile {
            path,
            content,
            existing,
        });
    }
    Ok(InitPlan {
        package,
        toolchain: LEAN_TOOLCHAIN,
        files,
    })
}

pub fn scaffold(root: &Path, target: &str, requested_package: &Path) -> Result<ScaffoldPlan> {
    let package_plan = init(root, requested_package)?;
    if package_plan.files.iter().any(|file| !file.existing) {
        bail!(
            "{} is not initialized; run `fr spec init {} --write` first.",
            package_plan.package.display(),
            requested_package.display()
        );
    }
    let (source, symbol) = target.split_once("::").ok_or_else(|| {
        anyhow::anyhow!("a scaffold target needs `<source-path>::<qualified-symbol>`.")
    })?;
    if source.is_empty() || symbol.is_empty() {
        bail!("a scaffold target needs `<source-path>::<qualified-symbol>`.");
    }
    let source = PathBuf::from(source);
    if source.is_absolute()
        || source
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("a scaffold source must be a workspace-relative path.");
    }
    let source_path = root.join(&source);
    let signature = rust_signature(&source_path, symbol)?;
    let hash = declaration_hash(
        &mut Parsers::new(),
        &mut Extractor::new(),
        &source_path,
        symbol,
    )?;
    let mapped = signature
        .iter()
        .map(|part| {
            if !lean_identifier(&part.name) && part.name != "return" {
                bail!(
                    "Rust parameter `{}` needs a simple identifier for scaffolding.",
                    part.name
                );
            }
            Ok(SignaturePart {
                name: part.name.clone(),
                ty: rust_type_to_lean(&part.ty)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let model = format!("{}Model", lower_identifier(symbol));
    let module = scaffold_module(&source, symbol);
    let mapping = signature
        .iter()
        .zip(&mapped)
        .map(|(source, model)| {
            format!(
                "{}: {} => {}: {}",
                source.name, source.ty, model.name, model.ty
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let parameters = mapped[..mapped.len() - 1]
        .iter()
        .map(|part| format!(" ({} : {})", part.name, part.ty))
        .collect::<String>();
    let return_type = &mapped
        .last()
        .context("Rust signature has no return type")?
        .ty;
    let generated_region = format!(
        "-- fr:generated-begin scaffold\n-- fr:spec {}::{} @ {}\n-- fr:signature {}\ndef {}{} : {} :=\n-- fr:generated-end scaffold\n",
        source.display(),
        symbol,
        hash,
        mapping,
        model,
        parameters,
        return_type
    );
    let model_path = package_plan
        .package
        .join("FrSpecs")
        .join(format!("{module}.lean"));
    let (model_original, generated, regenerated, handwritten_bytes) =
        match std::fs::symlink_metadata(&model_path) {
            Ok(metadata) if metadata.is_file() => {
                let original = crate::vfs::read_to_string(&model_path)?;
                let anchors = anchors_in(&original)?;
                if anchors.len() != 1 || anchors[0].1 != source || anchors[0].2 != symbol {
                    bail!(
                        "existing model {} does not belong to {}::{}.",
                        model_path.display(),
                        source.display(),
                        symbol
                    );
                }
                let (updated, preserved) = replace_generated_region(&original, &generated_region)?;
                (original, updated, true, preserved)
            }
            Ok(_) => bail!(
                "existing model is not a regular file: {}.",
                model_path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let handwritten = "-- fr:handwritten-begin model-and-proofs\n  by\n    -- fr:debt model-semantics\n    sorry\n-- fr:handwritten-end model-and-proofs\n";
                (
                    String::new(),
                    format!(
                        "namespace FrSpecs\n\n{generated_region}\n{handwritten}\nend FrSpecs\n"
                    ),
                    false,
                    handwritten.len(),
                )
            }
            Err(error) => return Err(error.into()),
        };
    let parsers = Parsers::new();
    let parsed = parsers.parse(crate::lang::Language::Lean, &generated)?;
    if parsed.has_errors() {
        bail!("generated Lean scaffold did not parse; no files changed.");
    }
    let root_path = package_plan.package.join("FrSpecs.lean");
    let root_original = crate::vfs::read_to_string(&root_path)?;
    let import = format!("import FrSpecs.{module}");
    let root_updated = if root_original.lines().any(|line| line.trim() == import) {
        root_original.clone()
    } else {
        format!("{import}\n{root_original}")
    };
    Ok(ScaffoldPlan {
        source,
        symbol: symbol.to_string(),
        model,
        module,
        hash,
        regenerated,
        handwritten_bytes,
        files: vec![
            ScaffoldFile {
                path: model_path,
                original: model_original,
                updated: generated,
            },
            ScaffoldFile {
                path: root_path,
                original: root_original,
                updated: root_updated,
            },
        ],
    })
}

pub fn formal_candidates(
    root: &Path,
    inputs: &[PathBuf],
    limit: usize,
    respect_ignore: bool,
) -> Result<FormalCandidates> {
    if !(1..=128).contains(&limit) {
        bail!("formal candidate limit must be between 1 and 128.");
    }
    let root = root.canonicalize()?;
    let requested = if inputs.is_empty() {
        vec![root.clone()]
    } else {
        inputs
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                }
            })
            .collect()
    };
    let mut rows = Vec::new();
    for input in requested {
        let canonical = input
            .canonicalize()
            .with_context(|| format!("reading candidate path {}", input.display()))?;
        if !canonical.starts_with(&root) {
            bail!("formal candidate path must stay inside the workspace.");
        }
        let walker = WalkBuilder::new(&canonical)
            .hidden(respect_ignore)
            .git_ignore(respect_ignore)
            .git_global(respect_ignore)
            .git_exclude(respect_ignore)
            .require_git(false)
            .build();
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type().is_some_and(|kind| kind.is_file())
                || path.extension().is_none_or(|extension| extension != "rs")
            {
                continue;
            }
            let module = match crate::transpile::read_file(path) {
                Ok(module) => module,
                Err(_) => continue,
            };
            let relative = path.strip_prefix(&root)?.to_path_buf();
            for item in module.items {
                let crate::transpile::ir::Item::Function(function) = item else {
                    continue;
                };
                let target = format!("{}::{}", relative.display(), function.name);
                let hash = declaration_hash(
                    &mut Parsers::new(),
                    &mut Extractor::new(),
                    path,
                    &function.name,
                )?;
                let result = formal_function(&root, &target);
                let (eligible, reason, suggested_properties) = match result {
                    Ok(formal) => (
                        true,
                        "typed pure body maps to the deterministic Lean kernel subset".to_string(),
                        suggested_properties(&formal.inputs, &formal.output),
                    ),
                    Err(error) => (false, error.to_string(), Vec::new()),
                };
                rows.push(FormalCandidate {
                    target: target.clone(),
                    source_hash: hash,
                    eligible,
                    reason,
                    suggested_properties,
                    action: vec!["spec".into(), "plan".into(), target],
                });
            }
        }
    }
    rows.sort_by(|left, right| left.target.cmp(&right.target));
    rows.dedup_by(|left, right| left.target == right.target);
    let omitted = rows.len().saturating_sub(limit);
    rows.truncate(limit);
    Ok(FormalCandidates {
        schema: "fr-formal-candidates-1",
        limits: FormalLimits {
            returned: rows.len(),
            limit,
        },
        candidates: rows,
        omitted,
    })
}

pub fn property_task(root: &Path, target: &str, token_limit: usize) -> Result<PropertyTask> {
    if !(1_024..=16_384).contains(&token_limit) {
        bail!("property task token limit must be between 1024 and 16384.");
    }
    let root = root.canonicalize()?;
    let formal = formal_function(&root, target)?;
    property_task_for_formal(&formal, token_limit)
}

pub fn formal_plan(root: &Path, target: &str, property_kinds: &[String]) -> Result<FormalPlan> {
    formal_plan_from_agent_specs(root, target, property_kinds, &[])
}

pub fn formal_plan_with_agent_properties(
    root: &Path,
    target: &str,
    property_kinds: &[String],
    property_paths: &[PathBuf],
) -> Result<FormalPlan> {
    let root = root.canonicalize()?;
    let mut properties = Vec::new();
    for path in property_paths {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        let text = crate::vfs::read_to_string(&path)
            .with_context(|| format!("reading agent property {}", path.display()))?;
        properties.push(
            serde_json::from_str(&text)
                .with_context(|| format!("parsing agent property {}", path.display()))?,
        );
    }
    formal_plan_from_agent_specs(&root, target, property_kinds, &properties)
}

fn formal_plan_from_agent_specs(
    root: &Path,
    target: &str,
    property_kinds: &[String],
    agent_specs: &[AgentProperty],
) -> Result<FormalPlan> {
    if property_kinds.len() + agent_specs.len() > 16 {
        bail!("a formal plan accepts at most 16 properties.");
    }
    let unique = property_kinds.iter().collect::<BTreeSet<_>>();
    if unique.len() != property_kinds.len() {
        bail!("a formal plan cannot repeat a property kind.");
    }
    let root = root.canonicalize()?;
    let formal = formal_function(&root, target)?;
    let mut properties = property_kinds
        .iter()
        .map(|kind| formal_property(kind, &formal))
        .collect::<Result<Vec<_>>>()?;
    for property in agent_specs {
        properties.push(agent_formal_property(property, &formal)?);
    }
    let unique_names = properties
        .iter()
        .map(|property| property.name.as_str())
        .collect::<BTreeSet<_>>();
    if unique_names.len() != properties.len() {
        bail!("a formal plan cannot repeat a theorem name.");
    }
    let target = FormalTarget {
        source: formal.source,
        symbol: formal.symbol,
        source_hash: formal.source_hash,
    };
    let kernel = FormalKernel {
        module: formal.module,
        model: formal.model,
        inputs: formal.inputs,
        output: formal.output,
        semantic_ir: formal.semantic_ir,
        lean_definition: formal.lean_definition,
    };
    let correspondence = FormalCorrespondence {
        source_identity: "sha256-anchored-declaration".into(),
        signature_surface: "strict-rust-lean-map".into(),
        model_generation: "deterministic-supported-semantic-ir".into(),
        implementation_model: "generated-model-with-executable-comparison-required".into(),
    };
    let assumptions = vec![
        "Rust parsing, semantic IR extraction and SHA-256 remain trusted.".into(),
        "The supported kernel excludes effects, mutation, calls, unsafe code and partial operations."
            .into(),
    ];
    let obligations = properties
        .iter()
        .map(|property| property.name.clone())
        .collect();
    let addressed = serde_json::json!({
        "schema": FORMAL_PLAN_SCHEMA,
        "target": &target,
        "kernel": &kernel,
        "properties": &properties,
        "correspondence": &correspondence,
        "assumptions": &assumptions,
        "obligations": &obligations,
    });
    let object_digest = crate::project::object_merkle(&addressed)?;
    Ok(FormalPlan {
        schema: FORMAL_PLAN_SCHEMA.into(),
        target,
        kernel,
        properties,
        correspondence,
        assumptions,
        obligations,
        object_digest,
        actions: FormalActions {
            scaffold: vec![
                "spec".into(),
                "scaffold".into(),
                "--from".into(),
                "<PLAN_FILE>".into(),
            ],
            goals: vec!["spec".into(), "goals".into(), "specs".into()],
            verify: vec!["spec".into(), "verify".into(), "specs".into()],
        },
    })
}

pub fn scaffold_formal(
    root: &Path,
    plan_path: &Path,
    requested_package: &Path,
) -> Result<FormalScaffoldPlan> {
    let text = crate::vfs::read_to_string(plan_path)
        .with_context(|| format!("reading formal plan {}", plan_path.display()))?;
    let supplied: FormalPlan = serde_json::from_str(&text)
        .with_context(|| format!("parsing formal plan {}", plan_path.display()))?;
    if supplied.schema != FORMAL_PLAN_SCHEMA {
        bail!("formal plan schema must be {FORMAL_PLAN_SCHEMA}.");
    }
    let target = format!(
        "{}::{}",
        supplied.target.source.display(),
        supplied.target.symbol
    );
    let kinds = supplied
        .properties
        .iter()
        .filter(|property| property.agent_spec.is_none())
        .map(|property| property.kind.clone())
        .collect::<Vec<_>>();
    let agent_specs = supplied
        .properties
        .iter()
        .filter_map(|property| property.agent_spec.clone())
        .collect::<Vec<_>>();
    let expected = formal_plan_from_agent_specs(root, &target, &kinds, &agent_specs)?;
    if serde_json::to_value(&supplied)? != serde_json::to_value(&expected)? {
        bail!("formal plan does not match the current source, kernel, properties and actions.");
    }
    let package_plan = init(root, requested_package)?;
    if package_plan.files.iter().any(|file| !file.existing) {
        bail!(
            "{} is not initialized; run `fr spec init {} --write` first.",
            package_plan.package.display(),
            requested_package.display()
        );
    }
    let model_path = package_plan
        .package
        .join("FrSpecs")
        .join(format!("{}.lean", expected.kernel.module));
    let original = match std::fs::symlink_metadata(&model_path) {
        Ok(metadata) if metadata.is_file() => crate::vfs::read_to_string(&model_path)?,
        Ok(_) => bail!(
            "existing model is not a regular file: {}.",
            model_path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let proofs = if original.is_empty() {
        BTreeMap::new()
    } else {
        proof_regions(&original)?
    };
    let proof_bytes_preserved = proofs.values().map(String::len).sum();
    let updated = render_formal_module(&expected, &proofs)?;
    let parsed = Parsers::new().parse(crate::lang::Language::Lean, &updated)?;
    if parsed.has_errors() {
        bail!("generated formal kernel did not parse as Lean; no files changed.");
    }
    check_generated_formal_module(&package_plan.package, &updated)?;
    let root_path = package_plan.package.join("FrSpecs.lean");
    let root_original = crate::vfs::read_to_string(&root_path)?;
    let import = format!("import FrSpecs.{}", expected.kernel.module);
    let root_updated = if root_original.lines().any(|line| line.trim() == import) {
        root_original.clone()
    } else {
        format!("{import}\n{root_original}")
    };
    Ok(FormalScaffoldPlan {
        plan_digest: expected.object_digest,
        source: expected.target.source,
        symbol: expected.target.symbol,
        model: expected.kernel.model,
        module: expected.kernel.module,
        obligations: expected.properties.len(),
        regenerated: !original.is_empty(),
        proof_bytes_preserved,
        files: vec![
            ScaffoldFile {
                path: model_path,
                original,
                updated,
            },
            ScaffoldFile {
                path: root_path,
                original: root_original,
                updated: root_updated,
            },
        ],
    })
}

pub fn formal_goals(
    root: &Path,
    inputs: &[PathBuf],
    selected: Option<&str>,
    limit: usize,
    token_limit: usize,
    respect_ignore: bool,
) -> Result<FormalGoals> {
    if !(1..=64).contains(&limit) {
        bail!("formal goal limit must be between 1 and 64.");
    }
    if !(1_024..=16_384).contains(&token_limit) {
        bail!("formal goal token limit must be between 1024 and 16384.");
    }
    let root = root.canonicalize()?;
    let report = check(&root, inputs, respect_ignore)?;
    let mut details = Vec::new();
    for debt in report.debts {
        let Some(name) = debt.name.clone() else {
            continue;
        };
        let source = crate::vfs::read_to_string(&debt.spec)?;
        let begin = format!("-- fr:proof-begin {name}");
        let end = format!("-- fr:proof-end {name}");
        let starts = source.match_indices(&begin).collect::<Vec<_>>();
        let ends = source.match_indices(&end).collect::<Vec<_>>();
        if starts.len() != 1 || ends.len() != 1 || starts[0].0 >= ends[0].0 {
            continue;
        }
        let theorem = theorem_before(&source, debt.line)?;
        let before = source
            .lines()
            .take(debt.line.saturating_sub(1))
            .collect::<Vec<_>>();
        let source_anchor = before
            .iter()
            .rev()
            .find(|line| line.trim().starts_with("-- fr:spec "))
            .map(|line| line.trim().to_string());
        let signature_map = before
            .iter()
            .rev()
            .find(|line| line.trim().starts_with("-- fr:signature "))
            .map(|line| line.trim().to_string());
        let spec = debt
            .spec
            .strip_prefix(&root)
            .unwrap_or(&debt.spec)
            .to_path_buf();
        let addressed = serde_json::json!({
            "name": name,
            "spec": spec,
            "line": debt.line,
            "theorem": theorem,
            "source_anchor": source_anchor,
            "signature_map": signature_map,
            "proof_region": name,
        });
        let id = crate::project::object_merkle(&addressed)?;
        details.push(FormalGoalDetail {
            id: id.clone(),
            name: name.clone(),
            spec: spec.clone(),
            line: debt.line,
            theorem,
            source_anchor,
            signature_map,
            proof_region: name.clone(),
            object_digest: id,
            prove_template: vec![
                "spec".into(),
                "prove".into(),
                format!("{}::{name}", spec.display()),
                "--from".into(),
                "<PROOF_FILE>".into(),
            ],
            verify: vec!["spec".into(), "verify".into(), spec.display().to_string()],
        });
    }
    details.sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    let object_root = crate::project::object_merkle(&serde_json::to_value(&details)?)?;
    let revealed = match selected {
        Some(id) => Some(
            details
                .iter()
                .find(|detail| detail.id == id)
                .cloned()
                .context("formal goal ID is absent or stale")?,
        ),
        None => None,
    };
    let mut catalog = details
        .iter()
        .take(limit)
        .map(|detail| FormalGoalCatalog {
            id: detail.id.clone(),
            name: detail.name.clone(),
            spec: detail.spec.clone(),
            line: detail.line,
            object_digest: detail.object_digest.clone(),
            reveal: vec![
                "spec".into(),
                "goals".into(),
                detail.spec.display().to_string(),
                "--goal".into(),
                detail.id.clone(),
                "--token-limit".into(),
                token_limit.to_string(),
            ],
        })
        .collect::<Vec<_>>();
    loop {
        let omitted = details.len().saturating_sub(catalog.len());
        let provisional = serde_json::json!({
            "schema": FORMAL_GOALS_SCHEMA,
            "commitment": {"object_schema": "fr-merkle-object-1", "object_root": object_root},
            "catalog": catalog,
            "omitted": omitted,
            "revealed": revealed,
            "token_budget": {"limit": token_limit, "used_upper_bound": token_limit, "measurement": "serialized_utf8_bytes"},
        });
        let upper_bound = serde_json::to_vec_pretty(&provisional)?.len();
        if upper_bound <= token_limit {
            return Ok(FormalGoals {
                schema: FORMAL_GOALS_SCHEMA,
                commitment: FormalGoalCommitment {
                    object_schema: "fr-merkle-object-1",
                    object_root,
                },
                catalog,
                omitted,
                revealed,
                token_budget: FormalGoalBudget {
                    limit: token_limit,
                    used_upper_bound: upper_bound,
                    measurement: "serialized_utf8_bytes",
                },
            });
        }
        if revealed.is_some() || catalog.is_empty() {
            bail!("formal goal disclosure cannot fit the selected response ceiling.");
        }
        catalog.pop();
    }
}

pub fn proof_task(
    root: &Path,
    target: &str,
    token_limit: usize,
    respect_ignore: bool,
) -> Result<ProofTask> {
    if !(1_024..=16_384).contains(&token_limit) {
        bail!("proof task token limit must be between 1024 and 16384.");
    }
    let root = root.canonicalize()?;
    let (spec, obligation) = proof_target(target)?;
    let evidence = check_strict(&root, std::slice::from_ref(&spec), respect_ignore)?;
    if !evidence.ok() {
        bail!("formal proof target has stale source or signature evidence.");
    }
    checked_proof_package(&root, &root.join(&spec))?;
    let catalog = formal_goals(
        &root,
        std::slice::from_ref(&spec),
        None,
        64,
        16_384,
        respect_ignore,
    )?;
    let item = catalog
        .catalog
        .iter()
        .find(|item| item.spec == spec && item.name == obligation)
        .with_context(|| format!("formal proof target `{target}` is absent or already proved"))?;
    let revealed = formal_goals(
        &root,
        std::slice::from_ref(&spec),
        Some(&item.id),
        1,
        16_384,
        respect_ignore,
    )?
    .revealed
    .context("selected formal proof goal was not revealed")?;
    let contract = ProofInputContract {
        author: "agent",
        format: "utf8-lean-tactics",
        insertion_point: "inside-existing-by-block",
        normalization: "trim-outer-whitespace-indent-two-spaces",
        forbidden: vec![
            "leading-by",
            "sorry",
            "admit",
            "unsolved-placeholder",
            "proof-region-marker",
        ],
        checker: LEAN_TOOLCHAIN,
    };
    let templates = vec![
        ProofTemplate {
            kind: "direct",
            lines: vec!["<agent-written-tactics>"],
        },
        ProofTemplate {
            kind: "structured-calculation",
            lines: vec![
                "calc",
                "  <expression> = <expression> := by",
                "    <agent-written-tactics>",
            ],
        },
    ];
    let core = serde_json::json!({
        "schema": PROOF_TASK_SCHEMA,
        "goal": &revealed,
        "contract": &contract,
        "templates": &templates,
    });
    let object_digest = crate::project::object_merkle(&core)?;
    let actions = ProofTaskActions {
        check: vec![
            "spec".into(),
            "proof-check".into(),
            target.into(),
            "--from".into(),
            "<PROOF_FILE>".into(),
        ],
        apply: vec![
            "spec".into(),
            "prove".into(),
            target.into(),
            "--from".into(),
            "<PROOF_FILE>".into(),
            "--write".into(),
        ],
        verify: vec!["spec".into(), "verify".into(), spec.display().to_string()],
    };
    let provisional = serde_json::json!({
        "schema": PROOF_TASK_SCHEMA,
        "goal": &revealed,
        "contract": &contract,
        "templates": &templates,
        "object_digest": &object_digest,
        "actions": &actions,
        "token_budget": {"limit": token_limit, "used_upper_bound": token_limit, "measurement": "serialized_utf8_bytes"},
    });
    let upper_bound = serde_json::to_vec_pretty(&provisional)?.len();
    if upper_bound > token_limit {
        bail!("proof task cannot fit the selected response ceiling.");
    }
    Ok(ProofTask {
        schema: PROOF_TASK_SCHEMA,
        goal: revealed,
        contract,
        templates,
        object_digest,
        actions,
        token_budget: FormalGoalBudget {
            limit: token_limit,
            used_upper_bound: upper_bound,
            measurement: "serialized_utf8_bytes",
        },
    })
}

pub fn proof_check(
    root: &Path,
    target: &str,
    proof_path: &Path,
    token_limit: usize,
) -> Result<ProofAttempt> {
    if !(1_024..=16_384).contains(&token_limit) {
        bail!("proof check token limit must be between 1024 and 16384.");
    }
    let root = root.canonicalize()?;
    let prepared = prepare_proof(&root, target, proof_path)?;
    check_prepared_proof(&root, target, proof_path, &prepared, token_limit)
}

pub fn prove(root: &Path, target: &str, proof_path: &Path) -> Result<ProofPlan> {
    let root = root.canonicalize()?;
    let mut prepared = prepare_proof(&root, target, proof_path)?;
    let attempt = check_prepared_proof(&root, target, proof_path, &prepared, 4_096)?;
    if !attempt.passed {
        let reason = attempt
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.as_str())
            .unwrap_or("Lean rejected the submitted tactics");
        bail!("proof failed Lean verification: {reason}");
    }
    prepared.receipt = attempt.receipt.context("accepted proof has no receipt")?;
    Ok(prepared)
}

fn proof_target(target: &str) -> Result<(PathBuf, String)> {
    let (spec, obligation) = target
        .rsplit_once("::")
        .ok_or_else(|| anyhow::anyhow!("a proof target needs `<spec-path>::<obligation-name>`."))?;
    if spec.is_empty()
        || obligation.is_empty()
        || !obligation.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        bail!("a proof target needs `<spec-path>::<obligation-name>`.");
    }
    let spec = PathBuf::from(spec);
    if spec.is_absolute()
        || spec
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || spec.extension().is_none_or(|extension| extension != "lean")
    {
        bail!("a proof target must name a workspace-relative Lean file.");
    }
    Ok((spec, obligation.into()))
}

fn prepare_proof(root: &Path, target: &str, proof_path: &Path) -> Result<ProofPlan> {
    let (spec, obligation) = proof_target(target)?;
    let path = root.join(&spec);
    let original = crate::vfs::read_to_string(&path)?;
    let proof = crate::vfs::read_to_string(proof_path)?;
    let proof = proof.trim();
    if proof.is_empty() || proof.len() > 65_536 {
        bail!("proof tactics must contain between 1 and 65536 UTF-8 bytes.");
    }
    if proof.split_whitespace().next() == Some("by") {
        bail!("proof input contains tactics only; omit the leading `by`.");
    }
    if ["sorry", "admit", "?_", "fr:proof-", "fr:debt"]
        .iter()
        .any(|forbidden| proof.contains(forbidden))
    {
        bail!("proof input cannot carry placeholders, debt or proof-region markers.");
    }
    let begin = format!("-- fr:proof-begin {obligation}");
    let end = format!("-- fr:proof-end {obligation}");
    let starts = original.match_indices(&begin).collect::<Vec<_>>();
    let ends = original.match_indices(&end).collect::<Vec<_>>();
    if starts.len() != 1 || ends.len() != 1 || starts[0].0 >= ends[0].0 {
        bail!("formal model needs one ordered proof region for `{obligation}`.");
    }
    let content_start = original[starts[0].0..]
        .find('\n')
        .map(|offset| starts[0].0 + offset + 1)
        .context("proof begin marker needs a following line")?;
    let content_end = original[..ends[0].0]
        .rfind("  -- fr:proof-end")
        .unwrap_or(ends[0].0);
    let rendered = proof
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect::<String>();
    let mut updated = String::with_capacity(original.len() + rendered.len());
    updated.push_str(&original[..content_start]);
    updated.push_str(&rendered);
    updated.push_str(&original[content_end..]);
    let parsed = Parsers::new().parse(crate::lang::Language::Lean, &updated)?;
    if parsed.has_errors() {
        bail!("proof replacement does not parse as Lean; no files changed.");
    }
    let task = proof_task(root, target, 16_384, true)?;
    let proof_digest = crate::project::object_merkle(&serde_json::json!(proof))?;
    Ok(ProofPlan {
        spec: path,
        obligation,
        original,
        updated,
        goal_id: task.goal.id,
        proof_digest,
        receipt: String::new(),
    })
}

fn check_prepared_proof(
    root: &Path,
    target: &str,
    proof_path: &Path,
    prepared: &ProofPlan,
    token_limit: usize,
) -> Result<ProofAttempt> {
    let package = checked_proof_package(root, &prepared.spec)?;
    let mut scratch = tempfile::Builder::new()
        .prefix("fr-proof-check-")
        .suffix(".lean")
        .tempfile()?;
    scratch
        .as_file_mut()
        .write_all(prepared.updated.as_bytes())?;
    scratch.as_file_mut().flush()?;
    let output = Command::new("lake")
        .args(["env", "lean"])
        .arg(scratch.path())
        .current_dir(&package)
        .output()
        .with_context(|| format!("running Lean proof check in {}", package.display()))?;
    let passed =
        proof_submission_admitted(true, true, true, true, true, true, output.status.success());
    let mut raw = String::from_utf8_lossy(&output.stdout).to_string();
    raw.push_str(&String::from_utf8_lossy(&output.stderr));
    let mut diagnostics = if passed {
        Vec::new()
    } else {
        lean_diagnostics(&raw, scratch.path(), root)
    };
    let all_diagnostics = diagnostics.len();
    let receipt = passed
        .then(|| {
            crate::project::object_merkle(&serde_json::json!({
                "schema": "fr-proof-receipt-1",
                "goal_id": prepared.goal_id,
                "proof_digest": prepared.proof_digest,
                "checker": LEAN_TOOLCHAIN,
            }))
        })
        .transpose()?;
    let actions = ProofAttemptActions {
        revise: vec![
            "spec".into(),
            "proof-check".into(),
            target.into(),
            "--from".into(),
            proof_path.display().to_string(),
        ],
        apply: passed.then(|| {
            vec![
                "spec".into(),
                "prove".into(),
                target.into(),
                "--from".into(),
                proof_path.display().to_string(),
                "--write".into(),
            ]
        }),
        verify: vec![
            "spec".into(),
            "verify".into(),
            prepared.spec.display().to_string(),
        ],
    };
    loop {
        let provisional = serde_json::json!({
            "schema": PROOF_ATTEMPT_SCHEMA,
            "goal_id": &prepared.goal_id,
            "proof_digest": &prepared.proof_digest,
            "checker": LEAN_TOOLCHAIN,
            "passed": passed,
            "diagnostics": &diagnostics,
            "diagnostics_omitted": all_diagnostics.saturating_sub(diagnostics.len()),
            "receipt": &receipt,
            "actions": &actions,
            "token_budget": {"limit": token_limit, "used_upper_bound": token_limit, "measurement": "serialized_utf8_bytes"},
        });
        let upper_bound = serde_json::to_vec_pretty(&provisional)?.len();
        if upper_bound <= token_limit {
            return Ok(ProofAttempt {
                schema: PROOF_ATTEMPT_SCHEMA,
                goal_id: prepared.goal_id.clone(),
                proof_digest: prepared.proof_digest.clone(),
                checker: LEAN_TOOLCHAIN,
                passed,
                diagnostics_omitted: all_diagnostics.saturating_sub(diagnostics.len()),
                diagnostics,
                receipt,
                actions,
                token_budget: FormalGoalBudget {
                    limit: token_limit,
                    used_upper_bound: upper_bound,
                    measurement: "serialized_utf8_bytes",
                },
            });
        }
        if diagnostics.pop().is_none() {
            bail!("proof check report cannot fit the selected response ceiling.");
        }
    }
}

fn check_generated_formal_module(package: &Path, source: &str) -> Result<()> {
    let toolchain_path = package.join("lean-toolchain");
    let toolchain = crate::vfs::read_to_string(&toolchain_path)
        .with_context(|| format!("reading pinned Lean toolchain {}", toolchain_path.display()))?;
    if toolchain.trim() != LEAN_TOOLCHAIN {
        bail!("formal scaffolding requires the fr pinned Lean toolchain {LEAN_TOOLCHAIN}.");
    }
    let mut scratch = tempfile::Builder::new()
        .prefix("fr-property-check-")
        .suffix(".lean")
        .tempfile_in(package)
        .context("creating temporary Lean property check")?;
    scratch.as_file_mut().write_all(source.as_bytes())?;
    scratch.as_file_mut().flush()?;
    let output = Command::new("lake")
        .args(["env", "lean"])
        .arg(scratch.path())
        .current_dir(package)
        .output()
        .context("running Lean property elaboration")?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr)
            .replace(&scratch.path().display().to_string(), "<PROPERTY_CHECK>")
            .chars()
            .take(2_048)
            .collect::<String>();
        bail!("generated formal property failed Lean elaboration: {diagnostic}");
    }
    Ok(())
}

fn checked_proof_package(root: &Path, spec: &Path) -> Result<PathBuf> {
    let package = lean_package(root, spec)?;
    let toolchain_path = package.join("lean-toolchain");
    let toolchain = crate::vfs::read_to_string(&toolchain_path)
        .with_context(|| format!("reading pinned Lean toolchain {}", toolchain_path.display()))?;
    if toolchain.trim() != LEAN_TOOLCHAIN {
        bail!("proof checking requires the fr pinned Lean toolchain {LEAN_TOOLCHAIN}.");
    }
    Ok(package)
}

fn lean_diagnostics(output: &str, scratch: &Path, root: &Path) -> Vec<ProofDiagnostic> {
    let normalized = output
        .replace(&scratch.display().to_string(), "<PROOF_CHECK>")
        .replace(&root.display().to_string(), ".");
    let mut diagnostics: Vec<ProofDiagnostic> = Vec::new();
    for line in normalized.lines() {
        let marker = [(": error: ", "error"), (": warning: ", "warning")]
            .into_iter()
            .find_map(|(needle, severity)| {
                line.find(needle).map(|index| (index, needle, severity))
            });
        if let Some((index, needle, severity)) = marker {
            let location = &line[..index];
            let mut parts = location.rsplitn(3, ':');
            let column = parts.next().and_then(|part| part.parse().ok());
            let line_number = parts.next().and_then(|part| part.parse().ok());
            diagnostics.push(ProofDiagnostic {
                severity: severity.into(),
                line: line_number,
                column,
                message: bounded_text(&line[index + needle.len()..], 512),
                context: Vec::new(),
            });
        } else if let Some(diagnostic) = diagnostics.last_mut() {
            if !line.trim().is_empty() && diagnostic.context.len() < 8 {
                diagnostic.context.push(bounded_text(line, 512));
            }
        }
    }
    if diagnostics.is_empty() {
        diagnostics.push(ProofDiagnostic {
            severity: "error".into(),
            line: None,
            column: None,
            message: "Lean rejected the submitted tactics".into(),
            context: normalized
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(8)
                .map(|line| bounded_text(line, 512))
                .collect(),
        });
    }
    diagnostics
}

fn bounded_text(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn theorem_before(source: &str, debt_line: usize) -> Result<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let start = (0..debt_line.saturating_sub(1))
        .rev()
        .find(|index| {
            let line = lines[*index].trim_start();
            line.starts_with("theorem ") || line.starts_with("lemma ")
        })
        .context("named proof debt has no preceding theorem or lemma")?;
    let mut header = Vec::new();
    for line in &lines[start..debt_line.saturating_sub(1)] {
        header.push(line.trim().to_string());
        if line.contains(":= by") || line.trim_end().ends_with(":=") {
            break;
        }
    }
    let joined = header.join(" ");
    if !joined.contains(":=") {
        bail!("proof goal theorem has no assignment boundary.");
    }
    Ok(joined
        .trim_end_matches("by")
        .trim_end()
        .trim_end_matches(":=")
        .trim()
        .to_string())
}

pub fn ci(root: &Path, requested_package: &Path, max_debt: usize) -> Result<CiPlan> {
    let package_plan = init(root, requested_package)?;
    if package_plan.files.iter().any(|file| !file.existing) {
        bail!(
            "{} is not initialized; run `fr spec init {} --write` first.",
            package_plan.package.display(),
            requested_package.display()
        );
    }
    let root = root.canonicalize()?;
    let package = package_plan.package.strip_prefix(&root)?.to_path_buf();
    let shell_package = shell_quote(&package.display().to_string());
    let yaml_package = serde_json::to_string(&package.display().to_string())?;
    let updated = format!(
        "name: fr Lean verification\n\non:\n  push:\n  pull_request:\n  workflow_dispatch:\n\npermissions:\n  contents: read\n\njobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n\n      - uses: actions/checkout@v4\n\n      - name: Install fr\n        run: cargo install fun-refactor --locked --version {}\n\n      - name: Check source correspondence and proof-debt ratchet\n        run: fr spec check {} --strict --max-debt {}\n\n      - uses: leanprover/lean-action@v1\n        with:\n          lake-package-directory: {}\n          build-args: --wfail\n",
        env!("CARGO_PKG_VERSION"),
        shell_package,
        max_debt,
        yaml_package
    );
    serde_yaml::from_str::<serde_yaml::Value>(&updated)
        .context("generated Lean CI workflow is not valid YAML")?;
    let path = root.join(".github/workflows/fr-lean.yml");
    for directory in [root.join(".github"), root.join(".github/workflows")] {
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "CI workflow path traverses a symlink: {}.",
                    directory.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "CI workflow parent is not a directory: {}.",
                    directory.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let original = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {
            let original = crate::vfs::read_to_string(&path)?;
            if original != updated {
                bail!(
                    "refusing to replace existing CI workflow {}.",
                    path.display()
                );
            }
            original
        }
        Ok(_) => bail!(
            "CI workflow target is not a regular file: {}.",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(CiPlan {
        package,
        path,
        original,
        updated,
        max_debt,
    })
}

pub fn evidence(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Evidence> {
    let verification = verify(root, inputs, respect_ignore)?;
    let files = spec_files(root, inputs, respect_ignore)?;
    let mut properties = Vec::new();
    let mut declared_assumptions = Vec::new();
    let parsers = Parsers::new();
    let mut extractor = Extractor::new();
    for spec in files {
        let source = crate::vfs::read_to_string(&spec)?;
        let parsed = parsers.parse(crate::lang::Language::Lean, &source)?;
        let facts = extractor.extract(&parsed, &spec, &source)?;
        let package = lean_package(root, &spec)?;
        let package_passed = verification
            .packages
            .iter()
            .find(|report| report.package == package)
            .is_some_and(|report| report.passed);
        for symbol in facts.symbols {
            let declaration = symbol.full_span.text(&source).trim_start();
            let keyword = declaration.split_whitespace().next().unwrap_or("");
            let line = crate::span::LineIndex::new(&source)
                .line_col(symbol.name_span.start, &source)
                .line;
            if matches!(keyword, "theorem" | "lemma") {
                properties.push(PropertyReport {
                    spec: spec.clone(),
                    line,
                    name: symbol.qualified_name(),
                    kind: keyword.to_string(),
                    status: if package_passed {
                        "checked_by_lean"
                    } else {
                        "unchecked"
                    },
                });
            } else if matches!(keyword, "axiom" | "opaque" | "constant") {
                declared_assumptions.push(DeclaredAssumption {
                    spec: spec.clone(),
                    line,
                    name: symbol.qualified_name(),
                    kind: keyword.to_string(),
                });
            }
        }
    }
    properties.sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    declared_assumptions
        .sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    let mut remaining_obligations = verification
        .report
        .debts
        .iter()
        .map(|debt| {
            format!(
                "{}:{} proof debt {}",
                debt.spec.display(),
                debt.line,
                debt.name.as_deref().unwrap_or("unnamed")
            )
        })
        .collect::<Vec<_>>();
    remaining_obligations.push(
        "No proof currently connects each Rust implementation to its Lean model.".to_string(),
    );
    Ok(Evidence {
        schema: 1,
        verification,
        properties,
        declared_assumptions,
        axiom_analysis: "Declared Lean axiom, opaque and constant syntax only.",
        trusted_components: vec![
            "Lean kernel, elaborator and compiler.",
            "Lake package and checked-target selection.",
            "fr parsing, declaration spans, signature checks and SHA-256.",
            "Host filesystem and process execution.",
        ],
        correspondence: CorrespondenceEvidence {
            source_identity: "Declaration bytes match each recorded SHA-256 anchor.",
            signature_surface: "Explicit maps match parsed Rust and Lean signatures.",
            tested_implementation_model: false,
            proved_implementation_model: false,
        },
        remaining_obligations,
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn replace_generated_region(original: &str, replacement: &str) -> Result<(String, usize)> {
    const BEGIN: &str = "-- fr:generated-begin scaffold";
    const END: &str = "-- fr:generated-end scaffold";
    let handwritten = handwritten_region(original)?.to_string();
    let start = original
        .find(BEGIN)
        .context("existing model has no generated scaffold region")?;
    if original[start + BEGIN.len()..].contains(BEGIN) {
        bail!("existing model has more than one generated scaffold region.");
    }
    let end_start = original[start..]
        .find(END)
        .map(|offset| start + offset)
        .context("existing model has no generated scaffold end marker")?;
    if original[end_start + END.len()..].contains(END) {
        bail!("existing model has more than one generated scaffold end marker.");
    }
    let end = original[end_start..]
        .find('\n')
        .map_or(original.len(), |offset| end_start + offset + 1);
    let mut updated = String::with_capacity(original.len() + replacement.len());
    updated.push_str(&original[..start]);
    updated.push_str(replacement);
    updated.push_str(&original[end..]);
    if handwritten_region(&updated)? != handwritten {
        bail!("scaffold regeneration did not preserve its handwritten region.");
    }
    Ok((updated, handwritten.len()))
}

fn handwritten_region(text: &str) -> Result<&str> {
    const BEGIN: &str = "-- fr:handwritten-begin model-and-proofs";
    const END: &str = "-- fr:handwritten-end model-and-proofs";
    let start = text
        .find(BEGIN)
        .context("existing model has no handwritten region")?;
    if text[start + BEGIN.len()..].contains(BEGIN) {
        bail!("existing model has more than one handwritten region.");
    }
    let end_start = text[start..]
        .find(END)
        .map(|offset| start + offset)
        .context("existing model has no handwritten end marker")?;
    if text[end_start + END.len()..].contains(END) {
        bail!("existing model has more than one handwritten end marker.");
    }
    let end = text[end_start..]
        .find('\n')
        .map_or(text.len(), |offset| end_start + offset + 1);
    Ok(&text[start..end])
}

fn lean_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn lower_identifier(symbol: &str) -> String {
    let name = symbol.rsplit("::").next().unwrap_or(symbol);
    let mut output = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if !lean_identifier(&output) {
        output.insert(0, 'm');
    }
    output
}

struct FormalFunction {
    source: PathBuf,
    symbol: String,
    source_hash: String,
    module: String,
    model: String,
    inputs: Vec<FormalBinding>,
    output: FormalBinding,
    semantic_ir: serde_json::Value,
    lean_definition: String,
}

fn formal_function(root: &Path, target: &str) -> Result<FormalFunction> {
    let (source, symbol) = target.split_once("::").ok_or_else(|| {
        anyhow::anyhow!("a formal target needs `<source-path>::<top-level-function>`.")
    })?;
    if source.is_empty() || symbol.is_empty() || symbol.contains("::") {
        bail!("a formal target needs `<source-path>::<top-level-function>`.");
    }
    let source = PathBuf::from(source);
    if source.is_absolute()
        || source
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || source.extension().is_none_or(|extension| extension != "rs")
    {
        bail!("a formal target must name a workspace-relative Rust source.");
    }
    let source_path = root.join(&source);
    let declaration = declaration_text(&source_path, symbol)?;
    let header = declaration
        .split_once('{')
        .map_or(declaration.as_str(), |(header, _)| header);
    if header
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == "unsafe")
    {
        bail!("unsafe functions stay outside generated formal kernels.");
    }
    let module = crate::transpile::read_file(&source_path)?;
    let matches = module
        .items
        .into_iter()
        .filter_map(|item| match item {
            crate::transpile::ir::Item::Function(function) if function.name == symbol => {
                Some(function)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [function] = matches.as_slice() else {
        bail!(
            "{} names {} top-level Rust functions called {symbol}.",
            source.display(),
            matches.len()
        );
    };
    let typed = function
        .params
        .iter()
        .all(|parameter| parameter.ty.is_some())
        && function.returns.is_some();
    let pure = !function.is_async
        && function.receiver.is_none()
        && function.params.iter().all(|parameter| {
            parameter.default.is_none() && parameter.kind == crate::transpile::ir::ParamKind::Normal
        });
    let expression = lean_kernel_block(&function.body);
    if !formal_candidate_admitted(
        true,
        function.receiver.is_none(),
        typed,
        pure,
        expression.is_ok(),
    ) {
        if function.receiver.is_some() {
            bail!("methods need an explicit receiver model before kernel generation.");
        }
        if !typed {
            bail!("every parameter and return needs an explicit supported type.");
        }
        if !pure {
            bail!("the kernel subset excludes defaults, unusual parameters and async behavior.");
        }
        return expression.map(|_| unreachable!());
    }
    let expression = expression?;
    let signature = rust_signature(&source_path, symbol)?;
    let mapped = signature
        .iter()
        .map(|part| {
            Ok(FormalBinding {
                name: part.name.clone(),
                rust_type: part.ty.clone(),
                lean_type: rust_type_to_lean(&part.ty)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (output, inputs) = mapped
        .split_last()
        .context("formal target has no return type")?;
    if inputs.iter().any(|input| !lean_identifier(&input.name)) {
        bail!("formal kernel parameters need simple Lean-compatible identifiers.");
    }
    let model = format!("{}Model", lower_identifier(symbol));
    let parameters = inputs
        .iter()
        .map(|input| format!(" ({} : {})", input.name, input.lean_type))
        .collect::<String>();
    let lean_definition = format!(
        "def {model}{parameters} : {} :=\n  {expression}",
        output.lean_type
    );
    let source_hash = declaration_hash(
        &mut Parsers::new(),
        &mut Extractor::new(),
        &source_path,
        symbol,
    )?;
    Ok(FormalFunction {
        source,
        symbol: symbol.into(),
        source_hash,
        module: scaffold_module(Path::new(source_path.strip_prefix(root)?), symbol),
        model,
        inputs: inputs.to_vec(),
        output: output.clone(),
        semantic_ir: serde_json::to_value(&function.body)?,
        lean_definition,
    })
}

fn lean_kernel_block(statements: &[crate::transpile::ir::Stmt]) -> Result<String> {
    use crate::transpile::ir::Stmt;
    let statements = statements
        .iter()
        .filter(|statement| !matches!(statement, Stmt::Comment(_)))
        .collect::<Vec<_>>();
    match statements.as_slice() {
        [Stmt::Return(Some(expression))] | [Stmt::Expr(expression)] => lean_kernel_expr(expression),
        [Stmt::Return(None)] => Ok("()".into()),
        [Stmt::Block(body)] => lean_kernel_block(body),
        [Stmt::If {
            condition,
            then,
            otherwise,
        }] if !otherwise.is_empty() => Ok(format!(
            "if {} then\n    {}\n  else\n    {}",
            lean_kernel_expr(condition)?,
            lean_kernel_block(then)?,
            lean_kernel_block(otherwise)?
        )),
        [Stmt::Let {
            name,
            value: Some(value),
            mutable: false,
            ..
        }, rest @ ..] if !rest.is_empty() && lean_identifier(name) => Ok(format!(
            "let {name} := {};\n  {}",
            lean_kernel_expr(value)?,
            lean_kernel_block_refs(rest)?
        )),
        _ => bail!(
            "the body needs pure returns, immutable lets or complete conditionals; effects and mutation remain explicit boundaries."
        ),
    }
}

fn lean_kernel_block_refs(statements: &[&crate::transpile::ir::Stmt]) -> Result<String> {
    let owned = statements
        .iter()
        .map(|statement| (*statement).clone())
        .collect::<Vec<_>>();
    lean_kernel_block(&owned)
}

fn lean_kernel_expr(expression: &crate::transpile::ir::Expr) -> Result<String> {
    use crate::transpile::ir::{BinaryOp, Expr, UnaryOp};
    match expression {
        Expr::Int(value)
            if value
                .chars()
                .all(|character| character.is_ascii_digit() || character == '-') =>
        {
            Ok(value.clone())
        }
        Expr::Str(value) => Ok(serde_json::to_string(value)?),
        Expr::Bool(value) => Ok(value.to_string()),
        Expr::Name(name) if lean_identifier(name) => Ok(name.clone()),
        Expr::Binary { op, left, right } => {
            let operator = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Eq => "==",
                BinaryOp::Ne => "!=",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                _ => bail!("division, remainder and xor need an explicit semantic policy."),
            };
            Ok(format!(
                "({} {operator} {})",
                lean_kernel_expr(left)?,
                lean_kernel_expr(right)?
            ))
        }
        Expr::Unary {
            op: UnaryOp::Not,
            operand,
        } => Ok(format!("(!{})", lean_kernel_expr(operand)?)),
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
        } => Ok(format!("(-{})", lean_kernel_expr(operand)?)),
        Expr::Ternary {
            condition,
            then,
            otherwise,
        } => Ok(format!(
            "if {} then {} else {}",
            lean_kernel_expr(condition)?,
            lean_kernel_expr(then)?,
            lean_kernel_expr(otherwise)?
        )),
        Expr::Tuple(values) => values
            .iter()
            .map(lean_kernel_expr)
            .collect::<Result<Vec<_>>>()
            .map(|values| format!("({})", values.join(", "))),
        Expr::ListLit(values) => values
            .iter()
            .map(lean_kernel_expr)
            .collect::<Result<Vec<_>>>()
            .map(|values| format!("[{}]", values.join(", "))),
        _ => bail!("an expression uses a call, effect, partial access or unsupported kernel form."),
    }
}

fn property_task_for_formal(formal: &FormalFunction, token_limit: usize) -> Result<PropertyTask> {
    let target = FormalTarget {
        source: formal.source.clone(),
        symbol: formal.symbol.clone(),
        source_hash: formal.source_hash.clone(),
    };
    let kernel = PropertyKernelContext {
        model: formal.model.clone(),
        inputs: formal.inputs.clone(),
        output: formal.output.clone(),
    };
    let contract = PropertyContract {
        author: "agent",
        format: AGENT_PROPERTY_SCHEMA,
        proof_author: "agent",
        allowed_terms: vec![
            "variable", "model", "boolean", "integer", "unary", "binary", "if",
        ],
        allowed_propositions: vec![
            "equals",
            "not-equals",
            "less-than",
            "less-or-equal",
            "greater-than",
            "greater-or-equal",
            "holds",
            "not",
            "and",
            "or",
            "implies",
        ],
        allowed_unary_operators: vec!["not", "negate"],
        allowed_binary_operators: vec!["add", "subtract", "multiply", "and", "or"],
        max_parameters: 8,
        max_nodes: 64,
        max_depth: 16,
    };
    let templates = vec![PropertyTemplate {
        kind: "model-relation",
        value: serde_json::json!({
            "schema": AGENT_PROPERTY_SCHEMA,
            "task_digest": "<copy-property-task-object-digest>",
            "name": "<agent-property-name>",
            "parameters": [{"name": "x", "lean_type": "<choose-a-disclosed-kernel-type>"}],
            "proposition": {
                "kind": "equals",
                "left": {"kind": "model", "arguments": [{"kind": "variable", "name": "x"}]},
                "right": {"kind": "variable", "name": "x"}
            }
        }),
    }];
    let core = serde_json::json!({
        "schema": PROPERTY_TASK_SCHEMA,
        "target": &target,
        "kernel": &kernel,
        "contract": &contract,
        "templates": &templates,
    });
    let object_digest = crate::project::object_merkle(&core)?;
    let actions = PropertyTaskActions {
        plan: vec![
            "spec".into(),
            "plan".into(),
            format!("{}::{}", target.source.display(), target.symbol),
            "--property-from".into(),
            "<PROPERTY_FILE>".into(),
        ],
        scaffold: vec![
            "spec".into(),
            "scaffold".into(),
            "--from".into(),
            "<PLAN_FILE>".into(),
            "--write".into(),
        ],
    };
    let provisional = serde_json::json!({
        "schema": PROPERTY_TASK_SCHEMA,
        "target": &target,
        "kernel": &kernel,
        "contract": &contract,
        "templates": &templates,
        "object_digest": &object_digest,
        "actions": &actions,
        "token_budget": {"limit": token_limit, "used_upper_bound": token_limit, "measurement": "serialized_utf8_bytes"},
    });
    let upper_bound = serde_json::to_vec_pretty(&provisional)?.len();
    if upper_bound > token_limit {
        bail!("property task cannot fit the selected response ceiling.");
    }
    Ok(PropertyTask {
        schema: PROPERTY_TASK_SCHEMA,
        target,
        kernel,
        contract,
        templates,
        object_digest,
        actions,
        token_budget: FormalGoalBudget {
            limit: token_limit,
            used_upper_bound: upper_bound,
            measurement: "serialized_utf8_bytes",
        },
    })
}

fn agent_formal_property(
    property: &AgentProperty,
    formal: &FormalFunction,
) -> Result<FormalProperty> {
    if property.schema != AGENT_PROPERTY_SCHEMA {
        bail!("agent property schema must be {AGENT_PROPERTY_SCHEMA}.");
    }
    let task = property_task_for_formal(formal, 16_384)?;
    if property.task_digest != task.object_digest {
        bail!("agent property task identity is absent, stale or belongs to another model.");
    }
    if !agent_lean_identifier(&property.name) {
        bail!("agent property name must be a safe Lean identifier.");
    }
    if property.parameters.len() > task.contract.max_parameters {
        bail!("agent property accepts at most 8 parameters.");
    }
    let allowed_types = formal
        .inputs
        .iter()
        .chain(std::iter::once(&formal.output))
        .map(|binding| binding.lean_type.as_str())
        .collect::<BTreeSet<_>>();
    let mut parameters = BTreeMap::new();
    for parameter in &property.parameters {
        if !agent_lean_identifier(&parameter.name) {
            bail!("agent property parameter names must be safe Lean identifiers.");
        }
        if !allowed_types.contains(parameter.lean_type.as_str()) {
            bail!("agent property parameter type is outside the disclosed kernel signature.");
        }
        if parameters
            .insert(parameter.name.as_str(), parameter.lean_type.as_str())
            .is_some()
        {
            bail!("agent property parameter names must be unique.");
        }
    }
    let mut nodes = 0;
    let proposition =
        render_agent_proposition(&property.proposition, formal, &parameters, &mut nodes, 1)?;
    if nodes > task.contract.max_nodes {
        bail!("agent property exceeds the 64-node ceiling.");
    }
    let binders = property
        .parameters
        .iter()
        .map(|parameter| format!("({} : {})", parameter.name, parameter.lean_type))
        .collect::<Vec<_>>()
        .join(" ");
    let proposition = if binders.is_empty() {
        format!(": {proposition}")
    } else {
        format!("{binders} : {proposition}")
    };
    Ok(FormalProperty {
        kind: "agent".into(),
        name: format!("{}_{}", formal.model, property.name),
        proposition,
        proof_status: "unproved".into(),
        agent_spec: Some(property.clone()),
    })
}

fn agent_lean_identifier(name: &str) -> bool {
    lean_identifier(name)
        && !matches!(
            name,
            "axiom"
                | "by"
                | "def"
                | "else"
                | "end"
                | "false"
                | "if"
                | "in"
                | "let"
                | "match"
                | "namespace"
                | "then"
                | "theorem"
                | "true"
        )
}

fn agent_property_node(nodes: &mut usize, depth: usize) -> Result<()> {
    *nodes += 1;
    if *nodes > 64 {
        bail!("agent property exceeds the 64-node ceiling.");
    }
    if depth > 16 {
        bail!("agent property exceeds the 16-level depth ceiling.");
    }
    Ok(())
}

fn agent_abstract_type(lean_type: &str) -> u8 {
    match lean_type {
        "Bool" => 0,
        "Nat" => 1,
        "Int" => 2,
        _ => 3,
    }
}

fn render_agent_term(
    term: &AgentTerm,
    formal: &FormalFunction,
    parameters: &BTreeMap<&str, &str>,
    nodes: &mut usize,
    depth: usize,
) -> Result<(String, String)> {
    agent_property_node(nodes, depth)?;
    match term {
        AgentTerm::Variable { name } => parameters
            .get(name.as_str())
            .map(|lean_type| (name.clone(), (*lean_type).to_string()))
            .with_context(|| format!("agent property variable `{name}` is not a parameter")),
        AgentTerm::Model { arguments } => {
            if arguments.len() != formal.inputs.len() {
                bail!("agent property model call has the wrong arity.");
            }
            let mut rendered = Vec::new();
            for (argument, input) in arguments.iter().zip(&formal.inputs) {
                let (argument, actual) =
                    render_agent_term(argument, formal, parameters, nodes, depth + 1)?;
                if actual != input.lean_type {
                    bail!("agent property model argument type does not match its input.");
                }
                rendered.push(format!("({argument})"));
            }
            Ok((
                format!("{} {}", formal.model, rendered.join(" "))
                    .trim()
                    .to_string(),
                formal.output.lean_type.clone(),
            ))
        }
        AgentTerm::Boolean { value } => Ok((value.to_string(), "Bool".into())),
        AgentTerm::Integer { value, lean_type } => {
            if !matches!(lean_type.as_str(), "Int" | "Nat") || (*value < 0 && lean_type == "Nat") {
                bail!("agent property integer literals require Int, or a non-negative Nat.");
            }
            Ok((format!("({value} : {lean_type})"), lean_type.clone()))
        }
        AgentTerm::Unary { operator, operand } => {
            let (operand, lean_type) =
                render_agent_term(operand, formal, parameters, nodes, depth + 1)?;
            let operator_code = match operator.as_str() {
                "not" => 0,
                "negate" => 1,
                _ => u8::MAX,
            };
            if !agent_term_operator_admitted(operator_code, agent_abstract_type(&lean_type)) {
                bail!("agent property unary operator does not accept its operand type.");
            }
            match (operator.as_str(), lean_type.as_str()) {
                ("not", "Bool") => Ok((format!("(!({operand}))"), lean_type)),
                ("negate", "Int") => Ok((format!("(-({operand}))"), lean_type)),
                _ => bail!("agent property unary operator does not accept its operand type."),
            }
        }
        AgentTerm::Binary {
            operator,
            left,
            right,
        } => {
            let (left, left_type) = render_agent_term(left, formal, parameters, nodes, depth + 1)?;
            let (right, right_type) =
                render_agent_term(right, formal, parameters, nodes, depth + 1)?;
            if left_type != right_type {
                bail!("agent property binary operands must have the same type.");
            }
            let operator_code = match operator.as_str() {
                "add" => 2,
                "subtract" => 3,
                "multiply" => 4,
                "and" => 5,
                "or" => 6,
                _ => u8::MAX,
            };
            if !agent_term_operator_admitted(operator_code, agent_abstract_type(&left_type)) {
                bail!("agent property binary operator does not accept its operand type.");
            }
            let symbol = match (operator.as_str(), left_type.as_str()) {
                ("add", "Int" | "Nat") => "+",
                ("subtract", "Int" | "Nat") => "-",
                ("multiply", "Int" | "Nat") => "*",
                ("and", "Bool") => "&&",
                ("or", "Bool") => "||",
                _ => bail!("agent property binary operator does not accept its operand type."),
            };
            Ok((format!("(({left}) {symbol} ({right}))"), left_type))
        }
        AgentTerm::If {
            condition,
            then,
            otherwise,
        } => {
            let (condition, condition_type) =
                render_agent_term(condition, formal, parameters, nodes, depth + 1)?;
            if condition_type != "Bool" {
                bail!("agent property if condition must be Bool.");
            }
            let (then, then_type) = render_agent_term(then, formal, parameters, nodes, depth + 1)?;
            let (otherwise, otherwise_type) =
                render_agent_term(otherwise, formal, parameters, nodes, depth + 1)?;
            if then_type != otherwise_type {
                bail!("agent property if branches must have the same type.");
            }
            Ok((
                format!("(if {condition} then {then} else {otherwise})"),
                then_type,
            ))
        }
    }
}

fn render_agent_proposition(
    proposition: &AgentProposition,
    formal: &FormalFunction,
    parameters: &BTreeMap<&str, &str>,
    nodes: &mut usize,
    depth: usize,
) -> Result<String> {
    agent_property_node(nodes, depth)?;
    let relation = |relation_code: u8,
                    left: &AgentTerm,
                    right: &AgentTerm,
                    symbol: &str,
                    nodes: &mut usize|
     -> Result<String> {
        let (left, left_type) = render_agent_term(left, formal, parameters, nodes, depth + 1)?;
        let (right, right_type) = render_agent_term(right, formal, parameters, nodes, depth + 1)?;
        if left_type != right_type {
            bail!("agent property relation operands must have the same type.");
        }
        if !agent_relation_admitted(
            relation_code,
            agent_abstract_type(&left_type),
            agent_abstract_type(&right_type),
        ) {
            bail!("agent property relation does not accept its operand types.");
        }
        Ok(format!("({left}) {symbol} ({right})"))
    };
    match proposition {
        AgentProposition::Equals { left, right } => relation(0, left, right, "=", nodes),
        AgentProposition::NotEquals { left, right } => relation(1, left, right, "≠", nodes),
        AgentProposition::LessThan { left, right } => relation(2, left, right, "<", nodes),
        AgentProposition::LessOrEqual { left, right } => relation(3, left, right, "≤", nodes),
        AgentProposition::GreaterThan { left, right } => relation(4, left, right, ">", nodes),
        AgentProposition::GreaterOrEqual { left, right } => relation(5, left, right, "≥", nodes),
        AgentProposition::Holds { term } => {
            let (term, lean_type) = render_agent_term(term, formal, parameters, nodes, depth + 1)?;
            if !agent_relation_admitted(6, agent_abstract_type(&lean_type), u8::MAX) {
                bail!("agent property holds requires a Bool term.");
            }
            Ok(format!("({term}) = true"))
        }
        AgentProposition::Not { proposition } => Ok(format!(
            "¬ ({})",
            render_agent_proposition(proposition, formal, parameters, nodes, depth + 1)?
        )),
        AgentProposition::And { propositions } | AgentProposition::Or { propositions } => {
            if !(2..=8).contains(&propositions.len()) {
                bail!("agent property conjunctions and disjunctions require 2 to 8 children.");
            }
            let symbol = if matches!(proposition, AgentProposition::And { .. }) {
                "∧"
            } else {
                "∨"
            };
            let rendered = propositions
                .iter()
                .map(|part| {
                    render_agent_proposition(part, formal, parameters, nodes, depth + 1)
                        .map(|part| format!("({part})"))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(rendered.join(&format!(" {symbol} ")))
        }
        AgentProposition::Implies {
            premise,
            conclusion,
        } => Ok(format!(
            "({}) → ({})",
            render_agent_proposition(premise, formal, parameters, nodes, depth + 1)?,
            render_agent_proposition(conclusion, formal, parameters, nodes, depth + 1)?
        )),
    }
}

fn suggested_properties(inputs: &[FormalBinding], output: &FormalBinding) -> Vec<&'static str> {
    let one_same = inputs.len() == 1 && inputs[0].lean_type == output.lean_type;
    let boolean = one_same && output.lean_type == "Bool";
    let mut properties = Vec::new();
    if one_same {
        properties.extend(["identity", "idempotent", "involutive"]);
    }
    if boolean {
        properties.extend(["preserves-true", "preserves-false"]);
    }
    properties
}

fn formal_property(kind: &str, function: &FormalFunction) -> Result<FormalProperty> {
    let known_kind = matches!(
        kind,
        "identity" | "idempotent" | "involutive" | "preserves-true" | "preserves-false"
    );
    let one_input = function.inputs.len() == 1;
    let same = one_input && function.inputs[0].lean_type == function.output.lean_type;
    let boolean = same && function.output.lean_type == "Bool";
    let needs_boolean = matches!(kind, "preserves-true" | "preserves-false");
    if !formal_property_admitted(known_kind, one_input, same, boolean)
        || (needs_boolean && !boolean)
    {
        bail!("property `{kind}` does not apply to this kernel signature.");
    }
    let input = &function.inputs[0];
    let model = &function.model;
    let (suffix, proposition) = match kind {
        "identity" => (
            "identity",
            format!("(x : {}) : {model} x = x", input.lean_type),
        ),
        "idempotent" => (
            "idempotent",
            format!(
                "(x : {}) : {model} ({model} x) = {model} x",
                input.lean_type
            ),
        ),
        "involutive" => (
            "involutive",
            format!("(x : {}) : {model} ({model} x) = x", input.lean_type),
        ),
        "preserves-true" => ("preserves_true", format!(": {model} true = true")),
        "preserves-false" => ("preserves_false", format!(": {model} false = false")),
        _ => unreachable!(),
    };
    Ok(FormalProperty {
        kind: kind.into(),
        name: format!("{model}_{suffix}"),
        proposition,
        proof_status: "unproved".into(),
        agent_spec: None,
    })
}

fn proof_regions(text: &str) -> Result<BTreeMap<String, String>> {
    let mut regions = BTreeMap::new();
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        let Some(name) = lines[index].trim().strip_prefix("-- fr:proof-begin ") else {
            index += 1;
            continue;
        };
        if regions.contains_key(name) {
            bail!("formal model has duplicate proof region `{name}`.");
        }
        let start = index + 1;
        index = start;
        let expected = format!("-- fr:proof-end {name}");
        while index < lines.len() && lines[index].trim() != expected {
            index += 1;
        }
        if index == lines.len() {
            bail!("formal model has no end marker for proof `{name}`.");
        }
        regions.insert(name.into(), lines[start..index].concat());
        index += 1;
    }
    Ok(regions)
}

fn render_formal_module(plan: &FormalPlan, proofs: &BTreeMap<String, String>) -> Result<String> {
    let mapping = plan
        .kernel
        .inputs
        .iter()
        .chain(std::iter::once(&plan.kernel.output))
        .map(|binding| {
            format!(
                "{}: {} => {}: {}",
                binding.name, binding.rust_type, binding.name, binding.lean_type
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut text = format!(
        "namespace FrSpecs\n\n-- fr:generated-begin formal-kernel\n-- fr:plan {}\n-- fr:spec {}::{} @ {}\n-- fr:signature {}\n{}\n",
        plan.object_digest,
        plan.target.source.display(),
        plan.target.symbol,
        plan.target.source_hash,
        mapping,
        plan.kernel.lean_definition
    );
    for property in &plan.properties {
        let proof = proofs
            .get(&property.name)
            .cloned()
            .unwrap_or_else(|| format!("  -- fr:debt {}\n  sorry\n", property.name));
        text.push_str(&format!(
            "\n-- fr:property {} {}\ntheorem {} {} := by\n  -- fr:proof-begin {}\n{}  -- fr:proof-end {}\n",
            property.kind,
            property.proof_status,
            property.name,
            property.proposition,
            property.name,
            proof,
            property.name
        ));
    }
    text.push_str("-- fr:generated-end formal-kernel\n\n-- fr:handwritten-begin additional-models-and-proofs\n-- fr:handwritten-end additional-models-and-proofs\n\nend FrSpecs\n");
    Ok(text)
}

fn scaffold_module(source: &Path, symbol: &str) -> String {
    let raw = format!("{} {}", source.display(), symbol);
    let mut output = String::new();
    let mut capitalize = true;
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(if capitalize {
                character.to_ascii_uppercase()
            } else {
                character
            });
            capitalize = false;
        } else {
            capitalize = true;
        }
    }
    if output
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        output.insert_str(0, "Model");
    }
    output
}

fn rust_type_to_lean(ty: &str) -> Result<String> {
    let ty = ty.trim();
    let direct = match ty {
        "bool" => Some("Bool"),
        "String" | "str" | "&str" => Some("String"),
        "usize" | "u8" | "u16" | "u32" | "u64" | "u128" => Some("Nat"),
        "isize" | "i8" | "i16" | "i32" | "i64" | "i128" => Some("Int"),
        "()" => Some("Unit"),
        _ => None,
    };
    if let Some(mapped) = direct {
        return Ok(mapped.to_string());
    }
    if let Some(inner) = ty.strip_prefix('&') {
        return rust_type_to_lean(inner.trim_start_matches("mut"));
    }
    for (rust, lean) in [("Option", "Option"), ("Vec", "List"), ("Box", "")] {
        if let Some(inner) = generic_arguments(ty, rust) {
            let mapped = rust_type_to_lean(inner)?;
            return Ok(match lean {
                "" => mapped,
                _ => format!("{lean} {mapped}"),
            });
        }
    }
    if let Some(inner) = generic_arguments(ty, "Result") {
        let parts = inner.split_top_level(',');
        if let [ok, error] = parts.as_slice() {
            return Ok(format!(
                "Except {} {}",
                rust_type_to_lean(error)?,
                rust_type_to_lean(ok)?
            ));
        }
    }
    if ty.starts_with('(') && ty.ends_with(')') {
        let parts = ty[1..ty.len() - 1].split_top_level(',');
        if parts.len() >= 2 {
            return parts
                .into_iter()
                .map(rust_type_to_lean)
                .collect::<Result<Vec<_>>>()
                .map(|parts| format!("({})", parts.join(" × ")));
        }
    }
    bail!("Rust type `{ty}` has no safe Lean scaffold mapping.")
}

fn generic_arguments<'a>(ty: &'a str, outer: &str) -> Option<&'a str> {
    ty.strip_prefix(outer)?.strip_prefix('<')?.strip_suffix('>')
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub anchors: Vec<AnchorReport>,
    pub obligations: usize,
    pub debts: Vec<DebtReport>,
}

#[derive(Debug, Serialize)]
pub struct DebtReport {
    pub spec: PathBuf,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Verification {
    pub report: Report,
    pub packages: Vec<PackageReport>,
}

#[derive(Debug, Serialize)]
pub struct PackageReport {
    pub package: PathBuf,
    pub passed: bool,
    pub output: String,
}

#[derive(Debug)]
pub struct Sync {
    pub report: Report,
    pub edits: EditSet,
    sources: Vec<SourceHash>,
}

#[derive(Debug)]
struct SourceHash {
    path: PathBuf,
    symbol: String,
    hash: String,
}

#[derive(Debug, Serialize)]
pub struct AnchorReport {
    pub spec: PathBuf,
    pub line: usize,
    pub source: PathBuf,
    pub symbol: String,
    pub expected: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureReport>,
    pub status: Status,
}

#[derive(Debug, Serialize)]
pub struct SignatureReport {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Fresh,
    Stale,
    Missing,
}

impl Report {
    pub fn unnamed_debts(&self) -> usize {
        self.debts.iter().filter(|debt| debt.name.is_none()).count()
    }

    pub fn fresh(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Fresh)
            .count()
    }

    pub fn stale(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Stale)
            .count()
    }

    pub fn missing(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| anchor.status == Status::Missing)
            .count()
    }

    pub fn stale_signatures(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.status == Status::Stale)
            })
            .count()
    }

    pub fn missing_signatures(&self) -> usize {
        self.anchors
            .iter()
            .filter(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_some_and(|signature| signature.status == Status::Missing)
            })
            .count()
    }

    pub fn ok(&self) -> bool {
        self.stale() == 0
            && self.missing() == 0
            && self.anchors.iter().all(|anchor| {
                anchor
                    .signature
                    .as_ref()
                    .is_none_or(|signature| signature.status == Status::Fresh)
            })
    }
}

pub fn debt_within_ceiling(obligations: usize, ceiling: usize) -> bool {
    obligations <= ceiling
}

pub fn check(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Report> {
    check_with(root, inputs, respect_ignore, false)
}

pub fn check_strict(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Report> {
    check_with(root, inputs, respect_ignore, true)
}

pub fn verify(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Verification> {
    let report = check_strict(root, inputs, respect_ignore)?;
    if !report.ok() {
        return Ok(Verification {
            report,
            packages: Vec::new(),
        });
    }
    let mut packages = BTreeSet::new();
    for spec in spec_files(root, inputs, respect_ignore)? {
        let package = lean_package(root, &spec)?;
        packages.insert(package);
    }
    let packages = packages
        .into_iter()
        .map(|package| {
            let output = Command::new("lake")
                .args(["build", "--wfail"])
                .current_dir(&package)
                .output()
                .with_context(|| format!("running lake in {}", package.display()))?;
            let mut text = String::from_utf8_lossy(&output.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Ok(PackageReport {
                package,
                passed: output.status.success(),
                output: text,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Verification { report, packages })
}

fn lean_package(root: &Path, spec: &Path) -> Result<PathBuf> {
    let mut here = spec.parent();
    while let Some(directory) = here {
        if crate::vfs::exists(directory.join("lakefile.lean"))
            || crate::vfs::exists(directory.join("lakefile.toml"))
        {
            return Ok(directory.to_path_buf());
        }
        if directory == root {
            break;
        }
        here = directory.parent();
    }
    bail!(
        "{} belongs to no Lean package with a lakefile.",
        spec.display()
    );
}

fn check_with(
    root: &Path,
    inputs: &[PathBuf],
    respect_ignore: bool,
    require_signatures: bool,
) -> Result<Report> {
    let files = spec_files(root, inputs, respect_ignore)?;
    let mut anchors = Vec::new();
    let mut debts = Vec::new();
    let mut parsers = Parsers::new();
    let mut extractor = Extractor::new();

    for spec in files {
        let text = crate::vfs::read_to_string(&spec)
            .with_context(|| format!("reading {}", spec.display()))?;
        debts.extend(debts_in(&spec, &text));
        for (line, source, symbol, expected) in anchors_in(&text)? {
            let source = crate::vfs::normalise(root.join(source));
            let report = if !source.starts_with(root) {
                AnchorReport {
                    spec: spec.clone(),
                    line,
                    source,
                    symbol,
                    expected,
                    actual: None,
                    detail: Some("a spec anchor may not leave the workspace".to_string()),
                    signature: None,
                    status: Status::Missing,
                }
            } else {
                match declaration_hash(&mut parsers, &mut extractor, &source, &symbol) {
                    Ok(actual) if actual.starts_with(&expected) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: Some(actual),
                        detail: None,
                        signature: None,
                        status: Status::Fresh,
                    },
                    Ok(actual) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: Some(actual),
                        detail: None,
                        signature: None,
                        status: Status::Stale,
                    },
                    Err(error) => AnchorReport {
                        spec: spec.clone(),
                        line,
                        source,
                        symbol,
                        expected,
                        actual: None,
                        detail: Some(error.to_string()),
                        signature: None,
                        status: Status::Missing,
                    },
                }
            };
            let mut report = report;
            match signature_mapping(&text, line) {
                Ok(Some(mapping)) => match mapped_signature(&report, &text, line, &mapping) {
                    Ok(()) => {
                        report.signature = Some(SignatureReport {
                            status: Status::Fresh,
                            detail: None,
                        })
                    }
                    Err(error) => {
                        report.signature = Some(SignatureReport {
                            status: Status::Stale,
                            detail: Some(error.to_string()),
                        })
                    }
                },
                Ok(None) if require_signatures => {
                    report.signature = Some(SignatureReport {
                        status: Status::Missing,
                        detail: Some(
                            "the strict check requires an explicit signature map".to_string(),
                        ),
                    })
                }
                Ok(None) => {}
                Err(error) => {
                    report.signature = Some(SignatureReport {
                        status: Status::Missing,
                        detail: Some(error.to_string()),
                    })
                }
            }
            anchors.push(report);
        }
    }
    anchors.sort_by(|left, right| (&left.spec, left.line).cmp(&(&right.spec, right.line)));
    Ok(Report {
        anchors,
        obligations: debts.len(),
        debts,
    })
}

pub fn sync(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Sync> {
    let report = check(root, inputs, respect_ignore)?;
    let mut edits = EditSet::new();
    let mut sources = Vec::new();
    for anchor in report
        .anchors
        .iter()
        .filter(|anchor| anchor.status == Status::Stale)
    {
        let text = crate::vfs::read_to_string(&anchor.spec)
            .with_context(|| format!("reading {}", anchor.spec.display()))?;
        let record = anchor_records(&text)?
            .into_iter()
            .find(|record| record.line == anchor.line)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}:{} lost its spec anchor",
                    anchor.spec.display(),
                    anchor.line
                )
            })?;
        let actual = anchor.actual.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "{}:{} has no source hash to renew",
                anchor.spec.display(),
                anchor.line
            )
        })?;
        sources.push(SourceHash {
            path: anchor.source.clone(),
            symbol: anchor.symbol.clone(),
            hash: actual.to_string(),
        });
        edits.declare_language(&anchor.spec, crate::lang::Language::Lean);
        edits.add(
            &anchor.spec,
            Edit::new(
                record.hash_span,
                actual,
                format!(
                    "renew the source hash for {}::{}",
                    anchor.source.display(),
                    anchor.symbol
                ),
            ),
        );
    }
    Ok(Sync {
        report,
        edits,
        sources,
    })
}

impl Sync {
    pub fn verify_sources(&self) -> Result<()> {
        let mut parsers = Parsers::new();
        let mut extractor = Extractor::new();
        for source in &self.sources {
            let actual =
                declaration_hash(&mut parsers, &mut extractor, &source.path, &source.symbol)?;
            if actual != source.hash {
                bail!(
                    "{}::{} changed after spec sync planned it. Nothing written; re-run against the current source.",
                    source.path.display(),
                    source.symbol
                );
            }
        }
        Ok(())
    }
}

fn spec_files(root: &Path, inputs: &[PathBuf], respect_ignore: bool) -> Result<Vec<PathBuf>> {
    let inputs = match inputs.is_empty() {
        true => [root.join("kernels"), root.join("specs")]
            .into_iter()
            .filter(|path| crate::vfs::exists(path))
            .collect::<Vec<_>>(),
        false => inputs
            .iter()
            .map(|input| match input.is_absolute() {
                true => input.clone(),
                false => root.join(input),
            })
            .collect(),
    };
    let mut files = BTreeSet::new();
    for input in inputs {
        let metadata =
            std::fs::metadata(&input).with_context(|| format!("reading {}", input.display()))?;
        if metadata.is_file() {
            lean_file(&input)?;
            files.insert(input);
            continue;
        }
        let mut found = false;
        let walker = WalkBuilder::new(&input)
            .standard_filters(respect_ignore)
            .hidden(respect_ignore)
            .git_ignore(respect_ignore)
            .require_git(false)
            .build();
        for entry in walker {
            let entry = entry?;
            if entry.file_type().is_some_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lean")
            {
                found = true;
                files.insert(entry.into_path());
            }
        }
        if !found {
            bail!("{} contains no .lean specs", input.display());
        }
    }
    if files.is_empty() {
        bail!("no Lean spec roots found; name a .lean file or directory");
    }
    Ok(files.into_iter().collect())
}

fn lean_file(path: &Path) -> Result<()> {
    if path.extension().is_none_or(|extension| extension != "lean") {
        bail!("{} is not a .lean spec", path.display());
    }
    Ok(())
}

fn anchors_in(text: &str) -> Result<Vec<(usize, PathBuf, String, String)>> {
    anchor_records(text).map(|anchors| {
        anchors
            .into_iter()
            .map(|anchor| (anchor.line, anchor.source, anchor.symbol, anchor.expected))
            .collect()
    })
}

struct Anchor {
    line: usize,
    source: PathBuf,
    symbol: String,
    expected: String,
    hash_span: Span,
}

fn anchor_records(text: &str) -> Result<Vec<Anchor>> {
    const PREFIX: &str = "-- fr:spec ";
    const SEPARATOR: &str = " @ ";
    let mut anchors = Vec::new();
    let mut start = 0;
    for (number, chunk) in text.split_inclusive('\n').enumerate() {
        let line = chunk
            .strip_suffix('\n')
            .unwrap_or(chunk)
            .trim_end_matches('\r');
        let trimmed = line.trim_start();
        let Some(body) = trimmed.strip_prefix(PREFIX) else {
            start += chunk.len();
            continue;
        };
        let number = number + 1;
        let (target, expected) = body
            .split_once(SEPARATOR)
            .ok_or_else(|| anyhow::anyhow!("line {number}: a spec anchor needs ` @ <hash>`"))?;
        let (source, symbol) = target.split_once("::").ok_or_else(|| {
            anyhow::anyhow!("line {number}: a spec anchor needs `<path>::<symbol>`")
        })?;
        if source.is_empty()
            || symbol.is_empty()
            || expected.len() < 8
            || expected.len() > 64
            || !expected.chars().all(|c| c.is_ascii_hexdigit())
        {
            bail!("line {number}: a spec anchor needs a path, symbol and hexadecimal hash");
        }
        let indentation = line.len() - trimmed.len();
        let hash_start = start + indentation + PREFIX.len() + target.len() + SEPARATOR.len();
        anchors.push(Anchor {
            line: number,
            source: PathBuf::from(source),
            symbol: symbol.to_string(),
            expected: expected.to_string(),
            hash_span: Span::new(hash_start, hash_start + expected.len()),
        });
        start += chunk.len();
    }
    Ok(anchors)
}

fn declaration_hash(
    parsers: &mut Parsers,
    extractor: &mut Extractor,
    path: &Path,
    wanted: &str,
) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(declaration_text_with(parsers, extractor, path, wanted)?.as_bytes())
    ))
}

fn declaration_text(path: &Path, wanted: &str) -> Result<String> {
    declaration_text_with(&mut Parsers::new(), &mut Extractor::new(), path, wanted)
}

fn declaration_text_with(
    parsers: &mut Parsers,
    extractor: &mut Extractor,
    path: &Path,
    wanted: &str,
) -> Result<String> {
    let language = detect(path)
        .ok_or_else(|| anyhow::anyhow!("{} has no language this build reads", path.display()))?;
    let source =
        crate::vfs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed = parsers.parse(language, &source)?;
    let facts = extractor.extract(&parsed, path, &source)?;
    let matches = facts
        .symbols
        .iter()
        .filter(|symbol| symbol.qualified_name() == wanted)
        .collect::<Vec<_>>();
    let [symbol] = matches.as_slice() else {
        bail!(
            "{} names {} declarations called {wanted}",
            path.display(),
            matches.len()
        );
    };
    Ok(symbol.full_span.text(&source).to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignaturePart {
    name: String,
    ty: String,
}

fn signature_mapping(
    text: &str,
    anchor_line: usize,
) -> Result<Option<Vec<(SignaturePart, SignaturePart)>>> {
    let Some(line) = text.lines().nth(anchor_line) else {
        return Ok(None);
    };
    let Some(body) = line.trim_start().strip_prefix("-- fr:signature ") else {
        return Ok(None);
    };
    body.split(';')
        .map(|part| {
            let (source, model) = part.trim().split_once(" => ").ok_or_else(|| {
                anyhow::anyhow!(
                    "line {}: a signature map needs `source: Type => model: Type`",
                    anchor_line + 1
                )
            })?;
            Ok((signature_part(source)?, signature_part(model)?))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn signature_part(text: &str) -> Result<SignaturePart> {
    let (name, ty) = text
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("`{text}` needs a name and type"))?;
    if name.trim().is_empty() || ty.trim().is_empty() {
        bail!("`{text}` needs a name and type");
    }
    Ok(SignaturePart {
        name: name.trim().to_string(),
        ty: compact_type(ty),
    })
}

fn mapped_signature(
    anchor: &AnchorReport,
    spec: &str,
    anchor_line: usize,
    mapping: &[(SignaturePart, SignaturePart)],
) -> Result<()> {
    let source = rust_signature(&anchor.source, &anchor.symbol)?;
    let model = lean_signature(spec, anchor_line)?;
    let expected_source = mapping.iter().map(|(source, _)| source).collect::<Vec<_>>();
    let expected_model = mapping.iter().map(|(_, model)| model).collect::<Vec<_>>();
    if source.iter().collect::<Vec<_>>() != expected_source {
        bail!("the Rust signature no longer matches its explicit map");
    }
    if model.iter().collect::<Vec<_>>() != expected_model {
        bail!("the Lean declaration no longer matches its explicit map");
    }
    Ok(())
}

fn rust_signature(path: &Path, wanted: &str) -> Result<Vec<SignaturePart>> {
    if path.extension().is_none_or(|extension| extension != "rs") {
        bail!("explicit signature maps currently require a Rust source declaration");
    }
    let text = crate::vfs::read_to_string(path)?;
    let parsers = Parsers::new();
    let mut extractor = Extractor::new();
    let parsed = parsers.parse(crate::lang::Language::Rust, &text)?;
    let facts = extractor.extract(&parsed, path, &text)?;
    let matches = facts
        .symbols
        .iter()
        .filter(|symbol| symbol.qualified_name() == wanted)
        .collect::<Vec<_>>();
    let [symbol] = matches.as_slice() else {
        bail!(
            "{} names {} declarations called {wanted}",
            path.display(),
            matches.len()
        );
    };
    let declaration = symbol.full_span.text(&text);
    let open = declaration
        .find('(')
        .context("the Rust declaration has no parameters")?;
    let close = matching_delimiter(declaration, open, '(', ')')
        .context("the Rust declaration has no closing parameter list")?;
    let mut parts = declaration[open + 1..close]
        .split_top_level(',')
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .map(signature_part)
        .collect::<Result<Vec<_>>>()?;
    let tail = &declaration[close + 1..];
    let return_type = tail
        .split_once("->")
        .map(|(_, ty)| ty.split('{').next().unwrap_or(ty).trim())
        .unwrap_or("()");
    parts.push(SignaturePart {
        name: "return".to_string(),
        ty: compact_type(return_type),
    });
    Ok(parts)
}

fn lean_signature(spec: &str, anchor_line: usize) -> Result<Vec<SignaturePart>> {
    let mut header = String::new();
    let mut found = false;
    for line in spec.lines().skip(anchor_line) {
        if !found && !line.trim_start().starts_with("def ") {
            continue;
        }
        found = true;
        header.push_str(line.trim());
        header.push(' ');
        if line.contains(":=") {
            break;
        }
    }
    if !found {
        bail!("the signature map needs a following Lean definition");
    }
    let before_body = header
        .split_once(":=")
        .map(|(head, _)| head)
        .context("the mapped Lean definition needs `:=` on its declaration line")?;
    let mut parts = Vec::new();
    let mut rest = before_body
        .strip_prefix("def ")
        .context("a Lean definition starts with `def`")?;
    rest = rest
        .split_once(char::is_whitespace)
        .map(|(_, tail)| tail)
        .unwrap_or("");
    while !rest.trim_start().starts_with(':') {
        let open = rest
            .find('(')
            .context("a mapped Lean parameter needs `(`")?;
        let close = matching_delimiter(rest, open, '(', ')')
            .context("an explicit Lean parameter needs `)`")?;
        parts.push(signature_part(&rest[open + 1..close])?);
        rest = &rest[close + 1..];
    }
    let return_type = rest
        .split_once(':')
        .map(|(_, ty)| ty)
        .context("the mapped Lean definition needs a return type")?;
    parts.push(SignaturePart {
        name: "return".to_string(),
        ty: compact_type(return_type),
    });
    Ok(parts)
}

trait SplitTopLevel {
    fn split_top_level(&self, separator: char) -> Vec<&str>;
}

impl SplitTopLevel for str {
    fn split_top_level(&self, separator: char) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut depth: usize = 0;
        for (offset, character) in self.char_indices() {
            match character {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth = depth.saturating_sub(1),
                _ if character == separator && depth == 0 => {
                    parts.push(&self[start..offset]);
                    start = offset + character.len_utf8();
                }
                _ => {}
            }
        }
        parts.push(&self[start..]);
        parts
    }
}

fn matching_delimiter(text: &str, open: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0;
    for (offset, character) in text[open..].char_indices() {
        match character {
            character if character == left => depth += 1,
            character if character == right => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn compact_type(text: &str) -> String {
    text.split_whitespace().collect()
}

#[cfg(test)]
fn obligations_in(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .flat_map(|line| line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
        .filter(|word| *word == "sorry")
        .count()
}

fn debts_in(spec: &Path, text: &str) -> Vec<DebtReport> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut debts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("--") {
            continue;
        }
        let count = line
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .filter(|word| *word == "sorry")
            .count();
        for _ in 0..count {
            let marker = index
                .checked_sub(1)
                .and_then(|previous| lines[previous].trim().strip_prefix("-- fr:debt "));
            let (name, detail) = match marker {
                Some(name)
                    if !name.is_empty()
                        && name.chars().all(|character| {
                            character.is_ascii_alphanumeric()
                                || matches!(character, '-' | '_' | '.')
                        }) =>
                {
                    (Some(name.to_string()), None)
                }
                Some(_) => (
                    None,
                    Some("the preceding proof-debt name is malformed".to_string()),
                ),
                None => (
                    None,
                    Some("strict checks require `-- fr:debt <name>` before `sorry`".to_string()),
                ),
            };
            debts.push(DebtReport {
                spec: spec.to_path_buf(),
                line: index + 1,
                name,
                detail,
            });
        }
    }
    debts
}

#[cfg(test)]
mod tests {
    use super::{
        anchors_in, check, check_strict, ci, debts_in, declaration_hash, init, lean_package,
        obligations_in, render_agent_proposition, scaffold, sync, AgentProposition, AgentTerm,
        FormalBinding, FormalFunction, Status,
    };
    use crate::extract::Extractor;
    use crate::parse::Parsers;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn reads_anchors_and_counts_only_live_obligations() {
        let anchors = anchors_in(
            "-- fr:spec src/edit.rs::apply_to_string @ deadbeef\ndef x := sorry\n-- sorry\n",
        )
        .unwrap();
        assert_eq!(anchors[0].0, 1);
        assert_eq!(anchors[0].1.to_string_lossy(), "src/edit.rs");
        assert_eq!(anchors[0].2, "apply_to_string");
        assert_eq!(obligations_in("def x := sorry\n-- sorry\n"), 1);
        let debts = debts_in(
            Path::new("Model.lean"),
            "-- fr:debt model-semantics\ndef x := sorry\ndef y := sorry\n-- sorry\n",
        );
        assert_eq!(debts.len(), 2);
        assert_eq!(debts[0].name.as_deref(), Some("model-semantics"));
        assert!(debts[1].name.is_none());
    }

    #[test]
    fn agent_equality_requires_exact_types_beyond_the_abstract_lean_classes() {
        let formal = FormalFunction {
            source: PathBuf::from("src/lib.rs"),
            symbol: "choose".into(),
            source_hash: "0".repeat(64),
            module: "SrcLibRsChoose".into(),
            model: "chooseModel".into(),
            inputs: Vec::new(),
            output: FormalBinding {
                name: "return".into(),
                rust_type: "Option<usize>".into(),
                lean_type: "Option Nat".into(),
            },
            semantic_ir: serde_json::json!({}),
            lean_definition: String::new(),
        };
        let proposition = AgentProposition::Equals {
            left: AgentTerm::Variable { name: "xs".into() },
            right: AgentTerm::Variable {
                name: "result".into(),
            },
        };
        let parameters = BTreeMap::from([("xs", "List Nat"), ("result", "Option Nat")]);
        let error =
            render_agent_proposition(&proposition, &formal, &parameters, &mut 0, 1).unwrap_err();
        assert!(error.to_string().contains("same type"), "{error}");
    }

    #[test]
    fn reports_fresh_stale_and_missing_anchors() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let source = "pub fn current() -> usize { 1 }\n";
        let code = root.join("src/code.rs");
        fs::write(&code, source).unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\ndef current : Nat := sorry\n-- sorry\n",
                &hash[..8]
            ),
        )
        .unwrap();

        let fresh = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(fresh.anchors[0].status, Status::Fresh);
        assert_eq!(fresh.obligations, 1);

        fs::write(&code, "pub fn current() -> usize { 2 }\n").unwrap();
        let stale = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(stale.anchors[0].status, Status::Stale);

        fs::write(
            root.join("specs/code.lean"),
            "-- fr:spec src/code.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let missing = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(missing.anchors[0].status, Status::Missing);

        fs::write(
            root.join("specs/code.lean"),
            "-- fr:spec ../outside.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let outside = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(outside.anchors[0].status, Status::Missing);
        assert!(outside.anchors[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("may not leave")));
    }

    #[test]
    fn sync_renews_stale_hashes_and_leaves_missing_anchors_unplanned() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(&code, "pub fn current() -> usize { 2 }\n").unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        let spec = root.join("specs/code.lean");
        fs::write(
            &spec,
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n",
        )
        .unwrap();

        let planned = sync(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(planned.report.stale(), 1);
        assert_eq!(planned.edits.file_count(), 1);
        let outcome = crate::edit::plan(&planned.edits, crate::edit::Validation::ReparseStrict)
            .unwrap()
            .pop()
            .unwrap();
        assert!(outcome.updated.contains(&hash));
        fs::write(&spec, outcome.updated).unwrap();
        assert!(check(root, &[PathBuf::from("specs")], true).unwrap().ok());

        fs::write(
            &spec,
            "-- fr:spec src/code.rs::current @ deadbeef\ndef current : Nat := 2\n",
        )
        .unwrap();
        let stale_source = sync(root, &[PathBuf::from("specs")], true).unwrap();
        fs::write(&code, "pub fn current() -> usize { 3 }\n").unwrap();
        assert!(stale_source.verify_sources().is_err());

        fs::write(
            &spec,
            "-- fr:spec src/code.rs::gone @ deadbeef\ndef gone : Nat := 0\n",
        )
        .unwrap();
        let missing = sync(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(missing.report.missing(), 1);
        assert!(missing.edits.is_empty());
    }

    #[test]
    fn checks_an_explicit_rust_to_lean_signature_map() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(
            &code,
            "pub fn current(source: &str, offset: usize) -> String { source.into() }\n",
        )
        .unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        let spec = root.join("specs/code.lean");
        fs::write(
            &spec,
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: &str => source: String; offset: usize => offset: Nat; return: String => return: String\ndef current (source : String) (offset : Nat) : String := source\n-- end.\n",
                &hash[..8]
            ),
        )
        .unwrap();

        let fresh = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(
            fresh.anchors[0].signature.as_ref().unwrap().status,
            Status::Fresh
        );
        assert!(fresh.ok());

        fs::write(
            &spec,
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: &str => source: String; offset: usize => offset: Nat; return: String => return: Nat\ndef current (source : String) (offset : Nat) : String := source\n-- end.\n",
                &hash[..8]
            ),
        )
        .unwrap();
        let stale = check(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(
            stale.anchors[0].signature.as_ref().unwrap().status,
            Status::Stale
        );
        assert!(!stale.ok());
    }

    #[test]
    fn strict_check_requires_every_anchor_to_map_its_signature() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(&code, "pub fn current() -> usize { 1 }\n").unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\ndef current : Nat := 1\n",
                &hash[..8]
            ),
        )
        .unwrap();
        assert!(check(root, &[PathBuf::from("specs")], true).unwrap().ok());
        let strict = check_strict(root, &[PathBuf::from("specs")], true).unwrap();
        assert_eq!(strict.missing_signatures(), 1);
        assert!(!strict.ok());
    }

    #[test]
    fn checks_nested_rust_types_against_a_multiline_lean_definition() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("specs")).unwrap();
        let code = root.join("src/code.rs");
        fs::write(
            &code,
            "pub fn current(source: Vec<(String, usize)>, callback: impl Fn(&str, usize) -> String) -> Result<(String, usize), ()> { unimplemented!() }\n",
        )
        .unwrap();
        let hash =
            declaration_hash(&mut Parsers::new(), &mut Extractor::new(), &code, "current").unwrap();
        fs::write(
            root.join("specs/code.lean"),
            format!(
                "-- fr:spec src/code.rs::current @ {}\n-- fr:signature source: Vec<(String, usize)> => source: List (String × Nat); callback: impl Fn(&str, usize) -> String => callback: String → Nat → String; return: Result<(String, usize), ()> => return: Option (String × Nat)\ndef current\n    (source : List (String × Nat))\n    (callback : String → Nat → String)\n    : Option (String × Nat) := none\n",
                &hash[..8]
            ),
        )
        .unwrap();
        let report = check_strict(root, &[PathBuf::from("specs")], true).unwrap();
        assert!(report.ok(), "{report:#?}");
    }

    #[test]
    fn finds_toml_and_lean_package_manifests() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let toml_spec = root.join("toml/Spec.lean");
        let lean_spec = root.join("lean/Spec.lean");
        fs::create_dir_all(toml_spec.parent().unwrap()).unwrap();
        fs::create_dir_all(lean_spec.parent().unwrap()).unwrap();
        fs::write(root.join("toml/lakefile.toml"), "name = \"toml\"\n").unwrap();
        fs::write(root.join("lean/lakefile.lean"), "package lean\n").unwrap();
        fs::write(&toml_spec, "def toml := 1\n").unwrap();
        fs::write(&lean_spec, "def lean := 1\n").unwrap();
        assert_eq!(lean_package(root, &toml_spec).unwrap(), root.join("toml"));
        assert_eq!(lean_package(root, &lean_spec).unwrap(), root.join("lean"));
    }

    #[test]
    fn init_is_bounded_and_idempotent_without_replacing_configuration() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let plan = init(root, Path::new("verification/lean")).unwrap();
        assert_eq!(plan.files.len(), 3);
        assert!(plan.files.iter().all(|file| !file.existing));
        assert!(init(root, Path::new("../outside")).is_err());
        assert!(init(root, &root.parent().unwrap().join("outside")).is_err());

        fs::create_dir_all(root.join("verification/lean")).unwrap();
        for file in &plan.files {
            fs::write(&file.path, file.content).unwrap();
        }
        let repeated = init(root, Path::new("verification/lean")).unwrap();
        assert!(repeated.files.iter().all(|file| file.existing));

        fs::write(
            root.join("verification/lean/lakefile.toml"),
            "owned = true\n",
        )
        .unwrap();
        let error = init(root, Path::new("verification/lean")).unwrap_err();
        assert!(error.to_string().contains("refusing to replace"), "{error}");
    }

    #[test]
    fn init_refuses_a_package_path_through_a_symlink() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("specs")).unwrap();
        let error = init(workspace.path(), Path::new("specs")).unwrap_err();
        assert!(error.to_string().contains("traverses a symlink"), "{error}");
    }

    #[test]
    fn scaffolds_a_strictly_mapped_rust_model_and_checked_import() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn choose(ok: bool, values: Vec<usize>) -> Option<usize> { values.into_iter().next() }\n",
        )
        .unwrap();
        let package = init(root, Path::new("specs")).unwrap();
        for file in package.files {
            fs::create_dir_all(file.path.parent().unwrap()).unwrap();
            fs::write(file.path, file.content).unwrap();
        }

        let plan = scaffold(root, "src/lib.rs::choose", Path::new("specs")).unwrap();
        assert_eq!(plan.module, "SrcLibRsChoose");
        assert_eq!(plan.model, "chooseModel");
        assert!(plan.files[0].updated.contains(
            "ok: bool => ok: Bool; values: Vec<usize> => values: List Nat; return: Option<usize> => return: Option Nat"
        ));
        assert!(plan.files[0]
            .updated
            .contains("def chooseModel (ok : Bool) (values : List Nat) : Option Nat :="));
        for file in plan.files {
            fs::create_dir_all(file.path.parent().unwrap()).unwrap();
            fs::write(file.path, file.updated).unwrap();
        }
        let checked = check_strict(root, &[PathBuf::from("specs")], true).unwrap();
        assert!(checked.ok(), "{checked:#?}");
        assert_eq!(checked.obligations, 1);
    }

    #[test]
    fn source_anchors_allow_qualified_symbols_after_the_source_path() {
        let anchors =
            anchors_in("-- fr:spec src/lib.rs::module::Thing::method @ deadbeef\ndef x := 0\n")
                .unwrap();
        assert_eq!(anchors[0].1, PathBuf::from("src/lib.rs"));
        assert_eq!(anchors[0].2, "module::Thing::method");
    }

    #[test]
    fn generates_valid_ci_with_a_selected_debt_ceiling() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let package = init(root, Path::new("verification")).unwrap();
        for file in package.files {
            fs::create_dir_all(file.path.parent().unwrap()).unwrap();
            fs::write(file.path, file.content).unwrap();
        }
        let plan = ci(root, Path::new("verification"), 2).unwrap();
        let yaml: serde_yaml::Value = serde_yaml::from_str(&plan.updated).unwrap();
        assert_eq!(yaml["jobs"]["verify"]["runs-on"], "ubuntu-latest");
        assert!(plan.updated.contains("--strict --max-debt 2"));
        assert!(plan
            .updated
            .contains("lake-package-directory: \"verification\""));
        assert!(plan.updated.contains("build-args: --wfail"));
    }

    #[test]
    fn ci_refuses_to_preview_through_a_workflow_symlink() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let package = init(root, Path::new("specs")).unwrap();
        for file in package.files {
            fs::create_dir_all(file.path.parent().unwrap()).unwrap();
            fs::write(file.path, file.content).unwrap();
        }
        std::os::unix::fs::symlink(outside.path(), root.join(".github")).unwrap();
        let error = ci(root, Path::new("specs"), 0).unwrap_err();
        assert!(error.to_string().contains("traverses a symlink"), "{error}");
    }
}
