use super::{bounded_text, manifests::Manifests};
use anyhow::{bail, ensure, Result};
use clap::Args;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

const MAX_FEATURE_INPUTS: usize = 256;
const MAX_FEATURE_GRAPH: usize = 65_536;
const MAX_ROW_MEMBERS: usize = 16;

#[derive(Args)]
pub struct Options {
    #[arg(long, help = "Select a Cargo.toml path relative to the project root.")]
    pub manifest: PathBuf,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Activate a comma-separated set of declared Cargo features."
    )]
    pub activate: Vec<String>,
    #[arg(long, help = "Do not activate the Cargo default feature.")]
    pub no_default_features: bool,
    #[arg(long, default_value_t = 40)]
    pub limit: usize,
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Default)]
struct Dependency {
    optional: bool,
    required: bool,
    default_features: bool,
    declared_features: BTreeSet<String>,
    sections: BTreeSet<String>,
    target_conditions: BTreeSet<String>,
}

pub struct Evaluation {
    pub rows: Vec<Value>,
    pub summary: Value,
}

fn strings(value: Option<&Value>) -> Option<Vec<String>> {
    value?.as_array().and_then(|values| {
        values
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect()
    })
}

fn dependency_tables(doc: &Value) -> Vec<(&str, Option<&str>, &Value)> {
    let mut tables = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = doc.get(section) {
            tables.push((section, None, table));
        }
    }
    if let Some(targets) = doc.get("target").and_then(Value::as_object) {
        for (condition, target) in targets {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(table) = target.get(section) {
                    tables.push((section, Some(condition.as_str()), table));
                }
            }
        }
    }
    tables
}

fn dependencies(doc: &Value) -> BTreeMap<String, Dependency> {
    let mut result: BTreeMap<String, Dependency> = BTreeMap::new();
    for (section, target, table) in dependency_tables(doc) {
        let Some(table) = table.as_object() else {
            continue;
        };
        for (name, spec) in table {
            let dependency = result.entry(name.clone()).or_default();
            dependency.sections.insert(section.to_owned());
            if let Some(target) = target {
                dependency.target_conditions.insert(target.to_owned());
            }
            let optional = spec
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            dependency.optional |= optional;
            dependency.required |= !optional;
            dependency.default_features |= spec.get("default-features") != Some(&json!(false));
            if let Some(features) = strings(spec.get("features")) {
                dependency.declared_features.extend(features);
            }
        }
    }
    result
}

fn bounded_set(values: Option<&BTreeSet<String>>) -> Value {
    values
        .into_iter()
        .flatten()
        .take(MAX_ROW_MEMBERS)
        .map(|value| bounded_text(value, 160))
        .collect()
}

pub fn evaluate(
    manifests: &Manifests,
    manifest: &Path,
    requested: &[String],
    no_default: bool,
) -> Result<Evaluation> {
    ensure!(
        manifest
            .file_name()
            .is_some_and(|name| name == "Cargo.toml"),
        "package feature evaluation requires a Cargo.toml manifest."
    );
    ensure!(
        requested.len() <= MAX_FEATURE_INPUTS,
        "at most 256 features may be activated."
    );
    for name in requested {
        ensure!(!name.is_empty(), "feature names cannot be empty.");
        ensure!(name.len() <= 160, "feature names cannot exceed 160 bytes.");
    }
    let doc = manifests
        .documents
        .get(manifest)
        .ok_or_else(|| anyhow::anyhow!("manifest has no captured valid document."))?;
    let dependencies = dependencies(doc);
    let feature_table = doc.get("features").and_then(Value::as_object);
    if doc.get("features").is_some() && feature_table.is_none() {
        bail!("Cargo features must be a table.");
    }
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    let mut suppressed = BTreeSet::new();
    if let Some(features) = feature_table {
        for (name, value) in features {
            let Some(members) = strings(Some(value)) else {
                bail!("Cargo feature `{name}` must be an array of strings.");
            };
            for member in &members {
                if let Some(dependency) = member.strip_prefix("dep:") {
                    suppressed.insert(dependency.to_owned());
                }
            }
            graph.insert(name.clone(), members);
        }
    }
    for (name, dependency) in &dependencies {
        if dependency.optional && !suppressed.contains(name) {
            graph
                .entry(name.clone())
                .or_insert_with(|| vec![format!("dep:{name}")]);
        }
    }
    for (name, members) in &graph {
        ensure!(
            !name.is_empty() && name.len() <= 160,
            "Cargo feature names must contain 1 through 160 bytes."
        );
        for member in members {
            ensure!(
                !member.is_empty() && member.len() <= 320,
                "Cargo feature members must contain 1 through 320 bytes."
            );
            if let Some(dependency) = member.strip_prefix("dep:") {
                ensure!(dependencies.get(dependency).is_some_and(|value| value.optional),
                    "Cargo feature `{name}` references non-optional or unknown dependency `{dependency}`.");
            } else if let Some((dependency, requested_feature)) = member.split_once('/') {
                let (dependency, weak) = dependency
                    .strip_suffix('?')
                    .map_or((dependency, false), |dependency| (dependency, true));
                ensure!(
                    !dependency.is_empty()
                        && !requested_feature.is_empty()
                        && dependencies.contains_key(dependency),
                    "Cargo feature `{name}` has an invalid dependency feature member `{member}`."
                );
                ensure!(!weak || dependencies.get(dependency).is_some_and(|value| value.optional),
                    "Cargo feature `{name}` weakly references non-optional dependency `{dependency}`.");
            } else {
                ensure!(
                    graph.contains_key(member),
                    "Cargo feature `{name}` references unknown feature `{member}`."
                );
            }
        }
    }
    let edge_count = graph.values().map(Vec::len).sum::<usize>();
    ensure!(
        super::package_feature_inventory_allowed(graph.len(), edge_count),
        "Cargo feature graph exceeds 65536 features or members."
    );

    let mut roots = BTreeSet::new();
    if !no_default && graph.contains_key("default") {
        roots.insert("default".to_owned());
    }
    for name in requested {
        ensure!(graph.contains_key(name), "unknown Cargo feature `{name}`.");
        roots.insert(name.clone());
    }
    let mut active = BTreeSet::new();
    let mut reasons = BTreeMap::<String, BTreeSet<String>>::new();
    let mut activated_dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    let mut strong_requests = BTreeMap::<String, BTreeSet<String>>::new();
    let mut weak_requests = BTreeMap::<String, BTreeSet<String>>::new();
    let mut pending = VecDeque::new();
    for root in &roots {
        reasons
            .entry(root.clone())
            .or_default()
            .insert("selected".to_owned());
        pending.push_back(root.clone());
    }
    while let Some(feature) = pending.pop_front() {
        if !active.insert(feature.clone()) {
            continue;
        }
        for member in graph.get(&feature).into_iter().flatten() {
            if let Some(dependency) = member.strip_prefix("dep:") {
                if dependencies.contains_key(dependency) {
                    activated_dependencies
                        .entry(dependency.to_owned())
                        .or_default()
                        .insert(feature.clone());
                }
            } else if let Some((dependency, requested_feature)) = member.split_once('/') {
                let (dependency, weak) = dependency
                    .strip_suffix('?')
                    .map_or((dependency, false), |dependency| (dependency, true));
                let requests = if weak {
                    &mut weak_requests
                } else {
                    &mut strong_requests
                };
                requests
                    .entry(dependency.to_owned())
                    .or_default()
                    .insert(requested_feature.to_owned());
                if super::package_feature_dependency_request(
                    true,
                    dependencies.contains_key(dependency),
                    weak,
                    false,
                ) {
                    activated_dependencies
                        .entry(dependency.to_owned())
                        .or_default()
                        .insert(feature.clone());
                }
            } else if graph.contains_key(member) {
                reasons
                    .entry(member.clone())
                    .or_default()
                    .insert(feature.clone());
                pending.push_back(member.clone());
            }
        }
    }

    let mut rows = Vec::with_capacity(graph.len() + dependencies.len());
    for (name, members) in &graph {
        let declared = feature_table.is_some_and(|features| features.contains_key(name));
        rows.push(json!({
            "kind": "package-feature", "manifest": bounded_text(&manifest.to_string_lossy(), 512),
            "name": bounded_text(name, 160), "declared": declared,
            "implicit_optional_dependency": !declared, "default_root": name == "default" && !no_default,
            "activated": active.contains(name), "activation_reasons": bounded_set(reasons.get(name)),
            "activation_reason_count": reasons.get(name).map_or(0, BTreeSet::len),
            "members": members.iter().take(MAX_ROW_MEMBERS).map(|member| bounded_text(member, 160)).collect::<Vec<_>>(),
            "member_count": members.len(), "members_omitted": members.len().saturating_sub(MAX_ROW_MEMBERS),
            "basis": "cargo-manifest-feature-graph"
        }));
    }
    for (name, dependency) in &dependencies {
        let optional_activated = activated_dependencies.contains_key(name);
        let active_dependency = dependency.required || optional_activated;
        let mut requested_features = dependency.declared_features.clone();
        requested_features.extend(strong_requests.get(name).into_iter().flatten().cloned());
        if super::package_feature_dependency_request(
            true,
            dependencies.contains_key(name),
            true,
            active_dependency,
        ) {
            requested_features.extend(weak_requests.get(name).into_iter().flatten().cloned());
        }
        rows.push(json!({
            "kind": "dependency-feature-request", "manifest": bounded_text(&manifest.to_string_lossy(), 512),
            "name": bounded_text(name, 160), "required_candidate": dependency.required,
            "optional": dependency.optional, "activated": active_dependency,
            "activation_sources": bounded_set(activated_dependencies.get(name)),
            "default_features": dependency.default_features,
            "requested_features": bounded_set(Some(&requested_features)),
            "requested_feature_count": requested_features.len(),
            "requested_features_omitted": requested_features.len().saturating_sub(MAX_ROW_MEMBERS),
            "inactive_weak_features": if active_dependency { json!([]) } else { bounded_set(weak_requests.get(name)) },
            "sections": bounded_set(Some(&dependency.sections)),
            "target_conditions": bounded_set(Some(&dependency.target_conditions)),
            "target_evaluation": if dependency.target_conditions.is_empty() { "unconditional-candidate" } else { "not-evaluated" },
            "basis": "cargo-manifest-feature-graph"
        }));
    }
    ensure!(
        rows.len() <= MAX_FEATURE_GRAPH,
        "Cargo feature report exceeds 65536 rows."
    );
    let active_dependencies = dependencies
        .iter()
        .filter(|(name, dependency)| {
            dependency.required || activated_dependencies.contains_key(*name)
        })
        .count();
    Ok(Evaluation {
        summary: json!({"requested": requested.iter().map(|name| bounded_text(name, 160)).collect::<Vec<_>>(),
            "default_features": !no_default, "feature_count": graph.len(),
            "activated_feature_count": active.len(), "dependency_count": dependencies.len(),
            "activated_dependency_candidate_count": active_dependencies,
            "scope": "Cargo manifest feature unification. This report does not evaluate dependency versions, target predicates, resolver versions or build-script behavior."}),
        rows,
    })
}
