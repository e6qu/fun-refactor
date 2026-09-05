use super::manifests::Manifests;
use super::{bounded_text, workspace_pattern_matches};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod workspaces;

fn text(value: &str) -> Value {
    bounded_text(value, 512)
}

fn path_text(path: &Path) -> Value {
    text(&path.to_string_lossy())
}

fn directories(manifests: &Manifests, root: &Path) -> BTreeSet<PathBuf> {
    manifests
        .snapshots
        .keys()
        .filter_map(|path| path.strip_prefix(root).ok())
        .flat_map(|path| path.parent().into_iter().flat_map(Path::ancestors))
        .map(Path::to_path_buf)
        .collect()
}

fn relative_directory(
    base: &Path,
    raw: &str,
    known: &BTreeSet<PathBuf>,
) -> Result<PathBuf, &'static str> {
    if raw.starts_with('/') || raw.starts_with('~') {
        return Err("absolute-or-home-path-unsupported");
    }
    if raw
        .chars()
        .any(|c| c.is_control() || "\\:%*?[]{}!".contains(c))
    {
        return Err("path-syntax-unsupported");
    }
    let mut result = base.to_path_buf();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if !result.pop() {
                    return Err("outside-selected-root");
                }
            }
            _ => {
                result.push(part);
                if !known.contains(&result) {
                    return Err("directory-not-observed");
                }
            }
        }
    }
    Ok(result)
}

fn pattern(raw: &str) -> Result<Vec<String>, &'static str> {
    if raw.is_empty() || raw.starts_with('/') {
        return Err("pattern-syntax-unsupported");
    }
    let mut result = Vec::new();
    for part in raw.split('/') {
        if part == "." || part.is_empty() {
            continue;
        }
        if part == ".."
            || (part != "*"
                && part
                    .chars()
                    .any(|c| c.is_control() || "\\:%*?[]{}!~".contains(c)))
        {
            return Err("pattern-syntax-unsupported");
        }
        result.push(part.to_owned());
    }
    Ok(result)
}

fn package(doc: &Value, cargo: bool) -> Option<&Value> {
    if cargo {
        doc.get("package").filter(|p| p.is_object())
    } else {
        Some(doc)
    }
}

fn local_dependency(
    manifests: &Manifests,
    known: &BTreeSet<PathBuf>,
    manifest: &Path,
    cargo: bool,
    name: &str,
    spec: &Value,
) -> Option<Value> {
    let inherited = cargo && spec.get("workspace").is_some();
    let path = if cargo {
        spec.get("path").and_then(Value::as_str)
    } else {
        spec.as_str().and_then(|s| {
            s.strip_prefix("file:")
                .or_else(|| (s.starts_with("./") || s.starts_with("../")).then_some(s))
        })
    };
    if !inherited && path.is_none() && !(cargo && spec.get("path").is_some()) {
        return None;
    }
    let mut row = json!({"kind": "local-dependency", "name": bounded_text(name, 160),
        "basis": "explicit-path", "status": "unresolved", "target_manifest": null,
        "declared_path": path.map(text), "validation": if cargo { "manifest-location-and-name-only" } else { "manifest-location-only" }});
    if inherited {
        row["basis"] = json!("workspace-inheritance");
        row["reason"] = json!("workspace-inheritance-unresolved");
        return Some(row);
    }
    let Some(path) = path else {
        row["reason"] = json!("invalid-path-field");
        return Some(row);
    };
    if cargo && spec.get("git").is_some() {
        row["reason"] = json!("conflicting-dependency-sources");
        return Some(row);
    }
    if !cargo && path.is_empty() {
        row["reason"] = json!("empty-file-specifier");
        return Some(row);
    }
    let base = manifest.parent().unwrap_or(Path::new(""));
    let directory = match relative_directory(base, path, known) {
        Ok(path) => path,
        Err(reason) => {
            row["reason"] = json!(reason);
            return Some(row);
        }
    };
    let target = directory.join(if cargo { "Cargo.toml" } else { "package.json" });
    let Some(target_package) = manifests
        .documents
        .get(&target)
        .and_then(|doc| package(doc, cargo))
    else {
        row["reason"] = json!("target-package-unavailable");
        return Some(row);
    };
    if cargo {
        let expected = spec.get("package").map_or(Some(name), Value::as_str);
        if expected.is_none() || target_package.get("name").and_then(Value::as_str) != expected {
            row["reason"] = json!("package-name-mismatch");
            row["candidate_manifest"] = path_text(&target);
            return Some(row);
        }
    }
    row["status"] = json!("linked");
    row["target_manifest"] = path_text(&target);
    row["target_name"] = target_package
        .get("name")
        .and_then(Value::as_str)
        .map(|s| bounded_text(s, 160))
        .unwrap_or(Value::Null);
    row["version_check"] = json!("not-performed");
    Some(row)
}

fn dependencies(
    manifests: &Manifests,
    known: &BTreeSet<PathBuf>,
    manifest: &Path,
    doc: &Value,
    cargo: bool,
    rows: &mut Vec<Value>,
    ownership: &workspaces::Analysis,
) {
    let mut scopes = vec![("package", None, doc)];
    if cargo {
        if let Some(workspace) = doc.get("workspace") {
            scopes.push(("workspace", None, workspace));
        }
        if let Some(targets) = doc.get("target").and_then(Value::as_object) {
            scopes.extend(
                targets
                    .iter()
                    .map(|(target, table)| ("target", Some(target.as_str()), table)),
            );
        }
    }
    for (scope, target, table) in scopes {
        let sections: &[&str] = if !cargo {
            &[
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ]
        } else if scope == "workspace" {
            &["dependencies"]
        } else {
            &["dependencies", "dev-dependencies", "build-dependencies"]
        };
        for section in sections {
            let Some(entries) = table.get(section).and_then(Value::as_object) else {
                continue;
            };
            for (name, spec) in entries {
                let row = if cargo && scope != "workspace" && spec.get("workspace").is_some() {
                    Some(ownership.inherited(manifests, known, manifest, name, spec))
                } else {
                    local_dependency(manifests, known, manifest, cargo, name, spec)
                };
                if let Some(mut row) = row {
                    row["manifest"] = path_text(manifest);
                    row["section"] = json!(section);
                    row["scope"] = json!(scope);
                    row["target_condition"] = target.map(text).unwrap_or(Value::Null);
                    rows.push(row);
                }
            }
        }
    }
}

fn exclusions(doc: &Value, cargo: bool) -> Result<Vec<(String, Vec<String>)>, &'static str> {
    let Some(exclude) = cargo
        .then(|| doc.get("workspace")?.get("exclude"))
        .flatten()
    else {
        return Ok(Vec::new());
    };
    let Some(array) = exclude.as_array() else {
        return Err("unsupported-exclusion");
    };
    array
        .iter()
        .map(|entry| {
            let raw = entry.as_str().ok_or("unsupported-exclusion")?;
            Ok((
                raw.to_owned(),
                pattern(raw).map_err(|_| "unsupported-exclusion")?,
            ))
        })
        .collect()
}

fn workspace(
    manifests: &Manifests,
    manifest: &Path,
    doc: &Value,
    cargo: bool,
    rows: &mut Vec<Value>,
) {
    let members = if cargo {
        doc.get("workspace").and_then(|w| w.get("members"))
    } else {
        doc.get("workspaces")
            .map(|w| w.get("packages").unwrap_or(w))
    };
    let Some(members) = members else { return };
    let template = json!({"kind": "workspace-member-match", "manifest": path_text(manifest),
        "basis": "declared-pattern", "membership": "candidate", "target_manifest": null});
    let Some(members) = members.as_array() else {
        let mut row = template;
        row["status"] = json!("unresolved");
        row["reason"] = json!("invalid-members-array");
        rows.push(row);
        return;
    };
    let excluded = exclusions(doc, cargo);
    let base = manifest.parent().unwrap_or(Path::new(""));
    for member in members {
        let mut row = template.clone();
        row["pattern"] = member.as_str().map(text).unwrap_or(Value::Null);
        row["status"] = json!("unresolved");
        let parts = match member
            .as_str()
            .ok_or("invalid-member-pattern")
            .and_then(pattern)
        {
            Ok(parts) => parts,
            Err(reason) => {
                row["reason"] = json!(reason);
                rows.push(row);
                continue;
            }
        };
        let mut matched = false;
        for (target, target_doc) in &manifests.documents {
            if target.file_name() != manifest.file_name() || package(target_doc, cargo).is_none() {
                continue;
            }
            let Some(relative) = target.parent().and_then(|p| p.strip_prefix(base).ok()) else {
                continue;
            };
            let components: Vec<_> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            if !workspace_pattern_matches(&parts, &components) {
                continue;
            }
            matched = true;
            let mut item = row.clone();
            item["candidate_manifest"] = path_text(target);
            match &excluded {
                Err(reason) => item["reason"] = json!(reason),
                Ok(exclusions) => {
                    if let Some((raw, _)) = exclusions
                        .iter()
                        .find(|(_, p)| workspace_pattern_matches(p, &components))
                    {
                        item["status"] = json!("excluded");
                        item["excluded_by"] = text(raw);
                    } else if cargo
                        && target != manifest
                        && (target_doc.get("workspace").is_some()
                            || target_doc
                                .get("package")
                                .and_then(|p| p.get("workspace"))
                                .is_some())
                    {
                        item["reason"] = json!("workspace-ownership-unresolved");
                    } else {
                        item["status"] = json!("matched");
                        item["target_manifest"] = path_text(target);
                    }
                }
            }
            rows.push(item);
        }
        if !matched {
            row["reason"] = json!("no-observed-package-match");
            rows.push(row);
        }
    }
}

pub(super) fn collect(manifests: &Manifests, root: &Path, selected: Option<&Path>) -> Vec<Value> {
    let known = directories(manifests, root);
    let ownership = workspaces::Analysis::new(manifests, root);
    let mut rows = Vec::new();
    for (manifest, doc) in &manifests.documents {
        if selected.is_some_and(|selected| selected != manifest) {
            continue;
        }
        let cargo = manifest
            .file_name()
            .is_some_and(|name| name == "Cargo.toml");
        dependencies(
            manifests, &known, manifest, doc, cargo, &mut rows, &ownership,
        );
        workspace(manifests, manifest, doc, cargo, &mut rows);
    }
    rows
}

pub(super) fn ownership(manifests: &Manifests, root: &Path, selected: Option<&Path>) -> Vec<Value> {
    workspaces::Analysis::new(manifests, root).rows(manifests, selected)
}
