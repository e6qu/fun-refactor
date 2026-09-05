use super::{
    directories, exclusions, local_dependency, package, path_text, pattern, relative_directory,
    text,
};
use crate::project::manifests::Manifests;
use crate::project::workspace_pattern_matches;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

struct Owner {
    root: Result<PathBuf, &'static str>,
    basis: &'static str,
}

#[derive(Clone)]
struct Member {
    root: PathBuf,
    basis: &'static str,
    via: Option<PathBuf>,
}

pub(super) struct Analysis {
    owners: BTreeMap<PathBuf, Owner>,
    members: BTreeMap<PathBuf, Member>,
    reasons: BTreeMap<PathBuf, &'static str>,
}

fn owner(
    manifests: &Manifests,
    root: &Path,
    known: &BTreeSet<PathBuf>,
    manifest: &Path,
    doc: &Value,
) -> Owner {
    let base = manifest.parent().unwrap_or(Path::new(""));
    if let Some(explicit) = doc.get("package").and_then(|p| p.get("workspace")) {
        let found = (|| {
            if doc.get("workspace").is_some() {
                return Err("conflicting-workspace-ownership");
            }
            let raw = explicit.as_str().ok_or("invalid-package-workspace")?;
            let target = relative_directory(base, raw, known)?.join("Cargo.toml");
            if !manifests
                .documents
                .get(&target)
                .and_then(|d| d.get("workspace"))
                .is_some_and(Value::is_object)
            {
                return Err("workspace-root-unavailable");
            }
            Ok(target)
        })();
        return Owner {
            root: found,
            basis: "package-workspace",
        };
    }
    for directory in base.ancestors() {
        let candidate = directory.join("Cargo.toml");
        if let Some(doc) = manifests.documents.get(&candidate) {
            if let Some(workspace) = doc.get("workspace") {
                let found = if workspace.is_object() {
                    Ok(candidate.clone())
                } else {
                    Err("invalid-workspace-table")
                };
                return Owner {
                    root: found,
                    basis: if candidate == manifest {
                        "own-workspace"
                    } else {
                        "ancestor-workspace"
                    },
                };
            }
        } else if manifests.snapshots.contains_key(&root.join(&candidate)) {
            return Owner {
                root: Err("workspace-ancestor-unavailable"),
                basis: "ancestor-workspace",
            };
        }
    }
    Owner {
        root: Err("workspace-root-not-observed"),
        basis: "ancestor-workspace",
    }
}

fn membership(
    manifests: &Manifests,
    root: &Path,
    target: &Path,
) -> Result<Option<&'static str>, &'static str> {
    let doc = &manifests.documents[root];
    let workspace = &doc["workspace"];
    let members = match workspace.get("members") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or("unsupported-members")?
            .iter()
            .map(|v| {
                pattern(v.as_str().ok_or("unsupported-members")?).map_err(|_| "unsupported-members")
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let excluded = exclusions(doc, true)?;
    let relative = target
        .parent()
        .unwrap_or(Path::new(""))
        .strip_prefix(root.parent().unwrap_or(Path::new("")))
        .map_err(|_| "member-outside-workspace-subset")?;
    let parts: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if excluded
        .iter()
        .any(|(_, p)| workspace_pattern_matches(p, &parts))
    {
        return Err("workspace-excluded");
    }
    if target == root {
        return Ok(Some("root-package"));
    }
    Ok(members
        .iter()
        .any(|p| workspace_pattern_matches(p, &parts))
        .then_some("declared-member"))
}

fn specs(doc: &Value) -> Vec<(&str, &Value)> {
    let mut tables = vec![doc];
    if let Some(targets) = doc.get("target").and_then(Value::as_object) {
        tables.extend(targets.values());
    }
    tables
        .into_iter()
        .flat_map(|table| {
            ["dependencies", "dev-dependencies", "build-dependencies"]
                .into_iter()
                .filter_map(move |key| table.get(key).and_then(Value::as_object))
                .flat_map(|entries| entries.iter().map(|(name, value)| (name.as_str(), value)))
        })
        .collect()
}

fn inherited_spec<'a>(
    manifests: &'a Manifests,
    root: &Path,
    name: &str,
    spec: &Value,
) -> Result<&'a Value, &'static str> {
    let fields = spec.as_object().ok_or("invalid-inherited-dependency")?;
    if spec.get("workspace") != Some(&json!(true)) {
        return Err("invalid-inherited-dependency");
    }
    if fields
        .keys()
        .any(|k| !["workspace", "optional", "features"].contains(&k.as_str()))
        || spec.get("optional").is_some_and(|v| !v.is_boolean())
        || spec
            .get("features")
            .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
    {
        return Err("unsupported-inherited-fields");
    }
    let value = manifests
        .documents
        .get(root)
        .and_then(|d| d.get("workspace"))
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.get(name))
        .ok_or("workspace-dependency-not-declared")?;
    if value.get("workspace").is_some() || value.get("optional").is_some() {
        return Err("invalid-workspace-dependency");
    }
    Ok(value)
}

fn path_target(
    manifests: &Manifests,
    known: &BTreeSet<PathBuf>,
    base: &Path,
    name: &str,
    spec: &Value,
) -> Option<PathBuf> {
    if spec.get("workspace").is_some() || spec.get("git").is_some() {
        return None;
    }
    let raw = spec.get("path")?.as_str()?;
    let target = relative_directory(base.parent()?, raw, known)
        .ok()?
        .join("Cargo.toml");
    let expected = spec.get("package").map_or(Some(name), Value::as_str)?;
    if manifests
        .documents
        .get(&target)?
        .get("package")?
        .get("name")?
        .as_str()?
        != expected
    {
        return None;
    }
    Some(target)
}

impl Analysis {
    pub fn new(manifests: &Manifests, root: &Path) -> Self {
        let known = directories(manifests, root);
        let owners: BTreeMap<_, _> = manifests
            .documents
            .iter()
            .filter(|(p, d)| {
                p.file_name().is_some_and(|n| n == "Cargo.toml") && package(d, true).is_some()
            })
            .map(|(p, d)| (p.clone(), owner(manifests, root, &known, p, d)))
            .collect();
        let mut result = Self {
            owners,
            members: BTreeMap::new(),
            reasons: BTreeMap::new(),
        };
        for (path, owner) in &result.owners {
            if let Ok(workspace) = &owner.root {
                match membership(manifests, workspace, path) {
                    Ok(Some(basis)) => {
                        result.members.insert(
                            path.clone(),
                            Member {
                                root: workspace.clone(),
                                basis,
                                via: None,
                            },
                        );
                    }
                    Ok(None) => {
                        result
                            .reasons
                            .insert(path.clone(), "package-not-observed-member");
                    }
                    Err(reason) => {
                        result.reasons.insert(path.clone(), reason);
                    }
                }
            }
        }
        loop {
            let before = result.members.len();
            for (source, member) in result.members.clone() {
                for (name, spec) in specs(&manifests.documents[&source]) {
                    let (base, effective) = if spec.get("workspace").is_some() {
                        let Ok(spec) = inherited_spec(manifests, &member.root, name, spec) else {
                            continue;
                        };
                        (&member.root, spec)
                    } else {
                        (&source, spec)
                    };
                    let Some(target) = path_target(manifests, &known, base, name, effective) else {
                        continue;
                    };
                    if result.members.contains_key(&target)
                        || !result
                            .owners
                            .get(&target)
                            .is_some_and(|o| o.root.as_ref().ok() == Some(&member.root))
                        || membership(manifests, &member.root, &target).is_err()
                    {
                        continue;
                    }
                    result.reasons.remove(&target);
                    result.members.insert(
                        target,
                        Member {
                            root: member.root.clone(),
                            basis: "automatic-path-member",
                            via: Some(source.clone()),
                        },
                    );
                }
            }
            if before == result.members.len() {
                break;
            }
        }
        result
    }

    fn reason(&self, manifest: &Path) -> &'static str {
        self.owners
            .get(manifest)
            .and_then(|o| o.root.as_ref().err())
            .copied()
            .or_else(|| self.reasons.get(manifest).copied())
            .unwrap_or("package-not-observed-member")
    }

    pub fn inherited(
        &self,
        manifests: &Manifests,
        known: &BTreeSet<PathBuf>,
        manifest: &Path,
        name: &str,
        spec: &Value,
    ) -> Value {
        let mut row = json!({"kind": "local-dependency", "name": super::bounded_text(name, 160),
            "basis": "workspace-inheritance", "status": "unresolved", "target_manifest": null});
        let Some(member) = self.members.get(manifest) else {
            row["reason"] = json!(self.reason(manifest));
            return row;
        };
        row["workspace_manifest"] = path_text(&member.root);
        let effective = match inherited_spec(manifests, &member.root, name, spec) {
            Ok(value) => value,
            Err(reason) => {
                row["reason"] = json!(reason);
                return row;
            }
        };
        if let Some(mut linked) =
            local_dependency(manifests, known, &member.root, true, name, effective)
        {
            linked["basis"] = json!("workspace-inheritance");
            linked["workspace_manifest"] = path_text(&member.root);
            linked["membership_basis"] = json!(member.basis);
            linked["features_check"] = json!("not-performed");
            return linked;
        }
        row["reason"] = json!("workspace-dependency-not-local");
        row
    }

    pub fn rows(&self, manifests: &Manifests, selected: Option<&Path>) -> Vec<Value> {
        let mut rows = Vec::new();
        for (path, doc) in &manifests.documents {
            if !path.file_name().is_some_and(|n| n == "Cargo.toml")
                || selected.is_some_and(|s| s != path)
            {
                continue;
            }
            if let Some(owner) = self.owners.get(path) {
                let mut row = json!({"kind": "workspace-membership", "manifest": path_text(path),
                    "ownership_basis": owner.basis, "workspace_manifest": owner.root.as_ref().ok().map(|p| path_text(p)),
                    "scope": "observed-manifests", "validation": "package-manager-unchecked"});
                if let Some(member) = self.members.get(path) {
                    row["status"] = json!("member");
                    row["membership_basis"] = json!(member.basis);
                    row["via_manifest"] = member
                        .via
                        .as_ref()
                        .map(|p| path_text(p))
                        .unwrap_or(Value::Null);
                } else {
                    row["status"] = json!("unresolved");
                    row["reason"] = text(self.reason(path));
                }
                rows.push(row);
            } else if doc.get("workspace").is_some_and(Value::is_object) {
                rows.push(json!({"kind": "workspace-root", "manifest": path_text(path), "status": "observed", "virtual": true}));
            }
        }
        rows
    }
}
