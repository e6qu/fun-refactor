use super::{FormalPlan, ScaffoldFile};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Component, Path};

const SNAPSHOT: &str = "fr-model-snapshot-1";
const REQUEST: &str = "fr-model-comparison-1";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    schema: String,
    target: String,
    source: String,
    model: FormalPlan,
    digest: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Relation {
    Equivalent,
    Refines,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    schema: String,
    name: String,
    before: Snapshot,
    after: String,
    relation: Relation,
    arguments: Vec<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    request: Request,
    after: Snapshot,
}

pub struct Plan {
    pub report: Value,
    pub files: Vec<ScaffoldFile>,
}

fn confined(root: &Path, relative: &Path) -> Result<()> {
    ensure!(!relative.as_os_str().is_empty(), "empty comparison path");
    let mut path = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(part) = part else {
            anyhow::bail!("comparison paths must be normalized and relative.");
        };
        path.push(part);
        if crate::vfs::is_in_memory() {
            continue;
        }
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "comparison paths cannot traverse symlinks."
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn snapshot_digest(snapshot: &Snapshot) -> Result<String> {
    crate::project::hash(serde_json::to_value((
        &snapshot.schema,
        &snapshot.target,
        &snapshot.source,
        &snapshot.model,
    ))?)
}

pub fn capture(root: &Path, target: &str) -> Result<Snapshot> {
    let root = super::planning_root(root)?;
    let (path, _) = target
        .split_once("::")
        .context("snapshot requires path::function")?;
    confined(&root, Path::new(path))?;
    ensure!(
        matches!(
            crate::lang::detect(Path::new(path)),
            Some(crate::lang::Language::Rust | crate::lang::Language::Python)
        ),
        "model comparisons currently admit Rust and Python sources."
    );
    let source = bounded_read(&root.join(path), 65536)?;
    ensure!(
        source.len() <= 65536,
        "model snapshot source exceeds 64 KiB."
    );
    let workspace = crate::vfs::new_handle([(root.join(path), source.clone())]);
    let model = crate::vfs::with_handle(&workspace, || super::formal_plan(&root, target, &[]))?;
    ensure!(
        model.kernel.inputs.len() <= 8
            && model
                .kernel
                .inputs
                .iter()
                .chain(std::iter::once(&model.kernel.output))
                .all(|binding| binding.lean_type == "Bool"),
        "model comparisons require at most eight Boolean parameters and a Boolean result."
    );
    ensure!(
        bounded_read(&root.join(path), 65536)? == source,
        "source changed during model capture."
    );
    let mut snapshot = Snapshot {
        schema: SNAPSHOT.into(),
        target: target.into(),
        source,
        model,
        digest: String::new(),
    };
    snapshot.digest = snapshot_digest(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<()> {
    ensure!(
        snapshot.schema == SNAPSHOT && snapshot.digest == snapshot_digest(snapshot)?,
        "model snapshot digest or schema mismatch."
    );
    let (path, _) = snapshot
        .target
        .split_once("::")
        .context("invalid snapshot target")?;
    ensure!(
        snapshot.source.len() <= 65536,
        "model snapshot source exceeds 64 KiB."
    );
    let root = Path::new("__fr_retained_model__");
    let workspace = crate::vfs::new_handle([(root.join(path), snapshot.source.clone())]);
    let regenerated = crate::vfs::with_handle(&workspace, || capture(root, &snapshot.target))?;
    ensure!(
        serde_json::to_value(regenerated)? == serde_json::to_value(snapshot)?,
        "snapshot model differs from retained source."
    );
    Ok(())
}

fn validate(comparison: &Comparison) -> Result<()> {
    let request = &comparison.request;
    ensure!(
        request.schema == REQUEST,
        "unsupported model comparison schema."
    );
    ensure!(
        !request.name.is_empty()
            && request.name.len() <= 64
            && request.name.as_bytes()[0].is_ascii_uppercase()
            && request.name.bytes().all(|b| b.is_ascii_alphanumeric()),
        "comparison name requires a capitalized ASCII identifier of at most 64 bytes."
    );
    validate_snapshot(&request.before)?;
    validate_snapshot(&comparison.after)?;
    ensure!(
        request.after == comparison.after.target,
        "comparison after target differs"
    );
    ensure!(
        request.arguments.len() == comparison.after.model.kernel.inputs.len()
            && request
                .arguments
                .iter()
                .all(|index| *index < request.before.model.kernel.inputs.len()),
        "argument mapping needs one valid old parameter index for each new parameter."
    );
    Ok(())
}

fn render(comparison: &Comparison, proofs: &BTreeMap<String, String>) -> Result<String> {
    let request = &comparison.request;
    let old = &request.before.model.kernel;
    let new = &comparison.after.model.kernel;
    let binders = (0..old.inputs.len())
        .map(|i| format!("(v{i} : Bool)"))
        .collect::<Vec<_>>()
        .join(" ");
    let old_args = (0..old.inputs.len())
        .map(|i| format!(" v{i}"))
        .collect::<String>();
    let new_args = request
        .arguments
        .iter()
        .map(|i| format!(" v{i}"))
        .collect::<String>();
    let before = format!("Before.{}{old_args}", old.model);
    let after = format!("After.{}{new_args}", new.model);
    let proposition = match request.relation {
        Relation::Equivalent => format!("({before}) = ({after})"),
        Relation::Refines => format!("({after}) = true → ({before}) = true"),
    };
    let theorem = if binders.is_empty() {
        proposition
    } else {
        format!("∀ {binders}, {proposition}")
    };
    ensure!(
        proofs.keys().all(|name| name == "preserves"),
        "comparison contains an unexpected proof region."
    );
    let proof = proofs
        .get("preserves")
        .cloned()
        .unwrap_or_else(|| "  -- fr:debt preserves\n  sorry\n".into());
    Ok(format!("-- fr:comparison {}\nnamespace FrSpecs.{}\n\nnamespace Before\nset_option linter.unusedVariables false in\n{}\nend Before\n\nnamespace After\nset_option linter.unusedVariables false in\n{}\nend After\n\n-- fr:property model-comparison agent-authored\ntheorem preserves : {theorem} := by\n  -- fr:proof-begin preserves\n{proof}  -- fr:proof-end preserves\n\nend FrSpecs.{}\n",
        crate::project::hash(comparison)?, request.name, old.lean_definition, new.lean_definition, request.name))
}

pub fn prepare(root: &Path, request_path: &Path, package: &Path) -> Result<Plan> {
    let input = bounded_read(request_path, 262144)?;
    ensure!(
        input.len() <= 262144,
        "model comparison request exceeds 256 KiB."
    );
    let request: Request = serde_json::from_str(&input)?;
    let after = capture(root, &request.after)?;
    let comparison = Comparison { request, after };
    validate(&comparison)?;
    confined(root, package)?;
    let initialized = super::init(root, package)?;
    ensure!(
        initialized.files.iter().all(|file| file.existing),
        "initialize the comparison package with fr spec init first."
    );
    let name = &comparison.request.name;
    let module = package.join("FrSpecs").join(format!("{name}.lean"));
    let manifest = module.with_extension("refinement.json");
    let mut files = Vec::new();
    for (relative, updated) in [
        (module.clone(), render(&comparison, &BTreeMap::new())?),
        (
            manifest,
            format!("{}\n", serde_json::to_string_pretty(&comparison)?),
        ),
    ] {
        confined(root, &relative)?;
        let path = root.join(&relative);
        ensure!(
            !crate::vfs::exists(&path),
            "comparison files already exist; choose a new name for a new claim."
        );
        files.push(ScaffoldFile {
            path,
            original: String::new(),
            updated,
        });
    }
    let basis = crate::project::hash((&comparison, package, "new-comparison-files"))?;
    let report = json!({"schema":"fr-comparison-review-1", "basis":basis, "package":package,
        "module":module, "theorem":"preserves", "before_digest":comparison.request.before.digest,
        "after_digest":comparison.after.digest, "relation":comparison.request.relation,
        "arguments":comparison.request.arguments, "source_implementation_proved":false,
        "claim":"Old/new generated Boolean model relation only. Historical source identity is retained content, not authenticated repository provenance. Parsing, extraction and source/model correspondence remain trusted or unproved.",
        "changes":files.iter().map(|file| { let path = file.path.strip_prefix(root).unwrap().to_string_lossy();
            json!({"path":path,"diff":crate::edit::unified_diff(&file.original,&file.updated,&path)}) }).collect::<Vec<_>>()});
    ensure!(
        serde_json::to_vec(&report)?.len() <= 1_048_576,
        "comparison review exceeds 1 MiB"
    );
    Ok(Plan { report, files })
}

pub(super) fn validate_module(root: &Path, spec: &Path, text: &str) -> Result<()> {
    let manifest = spec.with_extension("refinement.json");
    let marked = text
        .lines()
        .any(|line| line.starts_with("-- fr:comparison "));
    if !marked && !crate::vfs::exists(&manifest) {
        return Ok(());
    }
    confined(root, manifest.strip_prefix(root)?)?;
    let input = bounded_read(&manifest, 524288)
        .context("comparison requires its retained snapshot manifest.")?;
    ensure!(
        input.len() <= 524288,
        "comparison manifest exceeds 512 KiB."
    );
    let comparison: Comparison = serde_json::from_str(&input)?;
    validate(&comparison)?;
    ensure!(
        serde_json::to_value(capture(root, &comparison.request.after)?)?
            == serde_json::to_value(&comparison.after)?,
        "comparison source changed; retain a new old/new claim."
    );
    ensure!(
        spec.file_stem().and_then(|s| s.to_str()) == Some(comparison.request.name.as_str()),
        "comparison module name differs from its manifest."
    );
    ensure!(
        render(&comparison, &super::proof_regions(text)?)? == text,
        "comparison model, relation or generated context changed."
    );
    Ok(())
}
fn bounded_read(path: &Path, limit: usize) -> Result<String> {
    Ok(crate::vfs::read_to_string_bounded(path, limit)?)
}
pub(super) fn comparison_evidence(
    root: &Path,
    spec: &Path,
    text: &str,
    checked: bool,
) -> Result<Option<Value>> {
    if !crate::vfs::exists(spec.with_extension("refinement.json")) {
        return Ok(None);
    }
    validate_module(root, spec, text)?;
    let comparison: Comparison = serde_json::from_str(&bounded_read(
        &spec.with_extension("refinement.json"),
        524288,
    )?)?;
    Ok(Some(
        json!({"spec":spec,"theorem":"preserves", "relation":comparison.request.relation,
        "arguments":comparison.request.arguments,"before_digest":comparison.request.before.digest,
        "after_digest":comparison.after.digest,"before_target":comparison.request.before.target,
        "after_target":comparison.after.target,"domain":"pure Boolean models; at most eight old parameters.",
        "status":if checked { "checked_by_lean" } else { "unchecked" },
        "source_implementation_proved":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "pub fn allow(a: bool) -> bool { a }\n";

    #[test]
    fn replay_refuses_forged_models_and_restores_the_callers_workspace() {
        let root = Path::new("virtual");
        let path = root.join("subject.rs");
        let workspace = crate::vfs::new_handle([(path.clone(), SOURCE.into())]);
        crate::vfs::with_handle(&workspace, || {
            let original = capture(root, "subject.rs::allow").unwrap();
            validate_snapshot(&original).unwrap();
            let mut forged = original.clone();
            forged.model.kernel.lean_definition.push_str("\n-- forged");
            forged.digest = snapshot_digest(&forged).unwrap();
            assert!(validate_snapshot(&forged)
                .unwrap_err()
                .to_string()
                .contains("differs from retained source"));
            for target in ["../subject.rs::allow", "/subject.rs::allow"] {
                let mut escaped = original.clone();
                escaped.target = target.into();
                escaped.digest = snapshot_digest(&escaped).unwrap();
                assert!(validate_snapshot(&escaped)
                    .unwrap_err()
                    .to_string()
                    .contains("normalized and relative"));
            }
            let mut broken = original;
            broken.source = "this is not Rust".into();
            broken.digest = snapshot_digest(&broken).unwrap();
            assert!(validate_snapshot(&broken).is_err());
            assert_eq!(crate::vfs::paths(), std::slice::from_ref(&path));
            assert_eq!(crate::vfs::read_to_string(&path).unwrap(), SOURCE);
        });
        assert!(!crate::vfs::is_in_memory());
    }

    #[test]
    fn virtual_manifest_validation_detects_source_and_context_drift() {
        let root = Path::new("virtual");
        let path = root.join("subject.rs");
        let workspace = crate::vfs::new_handle([(path.clone(), SOURCE.into())]);
        crate::vfs::with_handle(&workspace, || {
            let snapshot = capture(root, "subject.rs::allow").unwrap();
            let comparison = Comparison {
                request: Request {
                    schema: REQUEST.into(),
                    name: "Virtual".into(),
                    before: snapshot.clone(),
                    after: "subject.rs::allow".into(),
                    relation: Relation::Equivalent,
                    arguments: vec![0],
                },
                after: snapshot,
            };
            let module = root.join("specs/FrSpecs/Virtual.lean");
            let manifest = module.with_extension("refinement.json");
            crate::vfs::write(&manifest, serde_json::to_string(&comparison).unwrap()).unwrap();
            let text = render(&comparison, &BTreeMap::new()).unwrap();
            validate_module(root, &module, &text).unwrap();
            assert!(
                validate_module(root, &module, &format!("{text}\n-- altered"))
                    .unwrap_err()
                    .to_string()
                    .contains("generated context changed")
            );
            crate::vfs::write(&path, SOURCE.replace("{ a }", "{ !a }")).unwrap();
            assert!(validate_module(root, &module, &text)
                .unwrap_err()
                .to_string()
                .contains("source changed"));
            crate::vfs::write(&path, SOURCE).unwrap();
            crate::vfs::remove(&manifest).unwrap();
            assert!(validate_module(root, &module, &text)
                .unwrap_err()
                .to_string()
                .contains("requires its retained snapshot"));
            assert_eq!(crate::vfs::read_to_string(&path).unwrap(), SOURCE);
        });
    }

    #[test]
    fn requests_and_manifests_refuse_oversize_before_json_parsing() {
        let root = Path::new("virtual");
        let request = root.join("request.json");
        let module = root.join("specs/FrSpecs/Virtual.lean");
        let workspace = crate::vfs::new_handle([
            (request.clone(), "x".repeat(262145)),
            (module.with_extension("refinement.json"), "x".repeat(524289)),
        ]);
        crate::vfs::with_handle(&workspace, || {
            let error = prepare(root, &request, Path::new("specs")).err().unwrap();
            assert!(error.to_string().contains("exceeds 262144 bytes"));
            let error = validate_module(root, &module, "-- fr:comparison test\n").unwrap_err();
            assert!(format!("{error:#}").contains("exceeds 524288 bytes"));
        });
    }
}
