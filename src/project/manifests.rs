use super::bounded_text;
use crate::scan::ScanOptions;
use anyhow::Result;
use ignore::WalkBuilder;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Eq, Serialize)]
pub(super) enum Snapshot {
    Source(String),
    Skipped(String),
}

#[derive(Default)]
pub(super) struct Manifests {
    pub snapshots: BTreeMap<PathBuf, Snapshot>,
    pub packages: Vec<Value>,
    pub declarations: Vec<(PathBuf, Value)>,
    pub gaps: Vec<Value>,
    pub documents: BTreeMap<PathBuf, Value>,
}

pub(super) fn discover(
    selected: &Path,
    options: &ScanOptions,
) -> Result<BTreeMap<PathBuf, Snapshot>> {
    let mut result = BTreeMap::new();
    for entry in WalkBuilder::new(selected)
        .standard_filters(options.respect_ignore)
        .hidden(options.respect_ignore)
        .git_ignore(options.respect_ignore)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".fr-history")
        .build()
    {
        let entry = entry?;
        if !matches!(
            entry.file_name().to_str(),
            Some("Cargo.toml" | "package.json")
        ) {
            continue;
        }
        let snapshot = if entry.path_is_symlink() {
            Snapshot::Skipped("symlink manifest".into())
        } else if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        } else if entry.metadata()?.len() > options.max_file_bytes {
            Snapshot::Skipped("manifest exceeds max-file-size".into())
        } else {
            match crate::vfs::read_to_string(entry.path()) {
                Ok(source) if source.len() as u64 <= options.max_file_bytes => {
                    Snapshot::Source(source)
                }
                Ok(_) => Snapshot::Skipped("manifest exceeds max-file-size".into()),
                Err(_) => Snapshot::Skipped("manifest is unreadable or not UTF-8".into()),
            }
        };
        result.insert(entry.into_path(), snapshot);
    }
    let scope = if selected.is_file() {
        selected.parent().unwrap_or(selected)
    } else {
        selected
    };
    let ancestors: std::collections::BTreeSet<_> = result
        .keys()
        .filter(|path| path.file_name().is_some_and(|n| n == "Cargo.toml"))
        .flat_map(|path| path.parent().into_iter().flat_map(Path::ancestors))
        .filter(|path| path.starts_with(scope))
        .map(|path| path.join("Cargo.toml"))
        .collect();
    for path in ancestors {
        if let std::collections::btree_map::Entry::Vacant(entry) = result.entry(path) {
            match std::fs::symlink_metadata(entry.key()) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    entry.insert(Snapshot::Skipped(
                        "workspace ancestor excluded or unavailable".into(),
                    ));
                }
            }
        }
    }
    Ok(result)
}

fn toml_value(item: &toml_edit::Item) -> Value {
    if let Some(table) = item.as_table_like() {
        return table
            .iter()
            .map(|(key, value)| (key.to_owned(), toml_value(value)))
            .collect();
    }
    match item {
        toml_edit::Item::Value(value) => scalar(value),
        _ => Value::Null,
    }
}

fn scalar(value: &toml_edit::Value) -> Value {
    match value {
        toml_edit::Value::String(s) => json!(s.value()),
        toml_edit::Value::Boolean(b) => json!(b.value()),
        toml_edit::Value::Integer(i) => json!(i.value()),
        toml_edit::Value::Float(f) => json!(f.value()),
        toml_edit::Value::Array(a) => a.iter().map(scalar).collect(),
        toml_edit::Value::InlineTable(t) => {
            t.iter().map(|(k, v)| (k.to_owned(), scalar(v))).collect()
        }
        toml_edit::Value::Datetime(_) => Value::Null,
    }
}

impl Manifests {
    pub fn new(selected: &Path, root: &Path, options: &ScanOptions) -> Result<Self> {
        let snapshots = discover(selected, options)?;
        let mut result = Self::default();
        for (path, snapshot) in &snapshots {
            let relative = path.strip_prefix(root)?;
            let Snapshot::Source(source) = snapshot else {
                if let Snapshot::Skipped(reason) = snapshot {
                    result.gap(relative, reason);
                }
                continue;
            };
            let cargo = path.file_name().is_some_and(|name| name == "Cargo.toml");
            let parsed = if cargo {
                source
                    .parse::<toml_edit::DocumentMut>()
                    .ok()
                    .map(|doc| toml_value(doc.as_item()))
            } else {
                serde_json::from_str::<Value>(source)
                    .ok()
                    .filter(Value::is_object)
            };
            if let Some(parsed) = parsed {
                result.read(relative, &parsed, cargo);
                result.documents.insert(relative.to_path_buf(), parsed);
            } else {
                result.gap(
                    relative,
                    if cargo {
                        "invalid TOML syntax"
                    } else {
                        "invalid JSON object"
                    },
                );
            }
        }
        result.snapshots = snapshots;
        Ok(result)
    }

    fn gap(&mut self, manifest: &Path, reason: &str) {
        self.gaps.push(
            json!({"path": bounded_text(&manifest.to_string_lossy(), 512),
            "reason": bounded_text(reason, 512), "scope": "manifest"}),
        );
    }

    fn declaration(&mut self, manifest: &Path, mut row: Value) {
        row["manifest"] = bounded_text(&manifest.to_string_lossy(), 512);
        row["basis"] = json!("manifest-declaration");
        self.declarations.push((manifest.to_path_buf(), row));
    }

    fn patterns(&mut self, manifest: &Path, value: Option<&Value>, kind: &str) {
        let Some(value) = value else { return };
        let Some(array) = value.as_array() else {
            self.gap(manifest, &format!("{kind} must be an array."));
            return;
        };
        for item in array {
            if let Some(pattern) = item.as_str() {
                self.declaration(
                    manifest,
                    json!({"kind": kind, "pattern": bounded_text(pattern, 512), "expanded": false}),
                );
            } else {
                self.gap(manifest, &format!("{kind} contains a non-string entry."));
            }
        }
    }

    fn dependencies(
        &mut self,
        manifest: &Path,
        table: &Value,
        cargo: bool,
        scope: &str,
        target: Option<&str>,
    ) {
        let sections: &[&str] = if cargo && scope == "workspace" {
            &["dependencies"]
        } else if cargo {
            &["dependencies", "dev-dependencies", "build-dependencies"]
        } else {
            &[
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
            ]
        };
        for section in sections {
            let Some(value) = table.get(section) else {
                continue;
            };
            let Some(entries) = value.as_object() else {
                self.gap(manifest, &format!("{section} must be a table/object."));
                continue;
            };
            for (name, spec) in entries {
                let gaps_before = self.gaps.len();
                let mut row = json!({"kind": "dependency", "name": bounded_text(name, 160),
                    "section": section, "scope": scope, "target_condition": target.map(|s| bounded_text(s, 512)),
                    "resolution": "not-attempted"});
                if let Some(version) = spec.as_str() {
                    row["requirement"] = bounded_text(version, 512);
                } else if cargo && spec.is_object() {
                    for key in [
                        "version", "package", "path", "git", "branch", "tag", "rev", "registry",
                    ] {
                        if let Some(value) = spec.get(key) {
                            if let Some(text) = value.as_str() {
                                row[if key == "version" { "requirement" } else { key }] =
                                    bounded_text(text, 512);
                            } else {
                                self.gap(manifest, &format!("dependency {key} must be a string."));
                            }
                        }
                    }
                    for key in ["workspace", "optional", "default-features"] {
                        if let Some(value) = spec.get(key) {
                            if value.is_boolean() {
                                row[key] = value.clone();
                            } else {
                                self.gap(manifest, &format!("dependency {key} must be a boolean."));
                            }
                        }
                    }
                    if let Some(features) = spec.get("features") {
                        if let Some(features) = features
                            .as_array()
                            .filter(|a| a.iter().all(Value::is_string))
                        {
                            row["feature_count"] = json!(features.len());
                        } else {
                            self.gap(manifest, "dependency features must be an array of strings.");
                        }
                    }
                    let known = [
                        "version",
                        "package",
                        "path",
                        "git",
                        "branch",
                        "tag",
                        "rev",
                        "registry",
                        "workspace",
                        "optional",
                        "default-features",
                        "features",
                    ];
                    row["unreported_fields"] = json!(spec
                        .as_object()
                        .unwrap()
                        .keys()
                        .filter(|k| !known.contains(&k.as_str()))
                        .count());
                } else {
                    self.gap(manifest, "unsupported dependency declaration shape.");
                    row["declaration_status"] = json!("unsupported");
                }
                if self.gaps.len() > gaps_before && row.get("declaration_status").is_none() {
                    row["declaration_status"] = json!("partial");
                }
                self.declaration(manifest, row);
            }
        }
    }

    fn read(&mut self, manifest: &Path, doc: &Value, cargo: bool) {
        let package = if cargo { doc.get("package") } else { Some(doc) };
        if package.is_some_and(|p| !p.is_object()) {
            self.gap(manifest, "package must be a table/object.");
        }
        if cargo && package.is_none() && doc.get("workspace").is_none() {
            self.gap(
                manifest,
                "Cargo manifest has no package or workspace table.",
            );
        }
        let before = self.declarations.len();
        self.dependencies(manifest, doc, cargo, "package", None);
        if cargo {
            if let Some(targets) = doc.get("target") {
                if let Some(targets) = targets.as_object() {
                    for (condition, target) in targets {
                        if target.is_object() {
                            self.dependencies(manifest, target, true, "target", Some(condition));
                        } else {
                            self.gap(manifest, "target entry must be a table.");
                        }
                    }
                } else {
                    self.gap(manifest, "target must be a table.");
                }
            }
            if let Some(workspace) = doc.get("workspace") {
                if workspace.is_object() {
                    self.dependencies(manifest, workspace, true, "workspace", None);
                    for (key, kind) in [
                        ("members", "workspace-member-pattern"),
                        ("exclude", "workspace-exclude-pattern"),
                        ("default-members", "workspace-default-pattern"),
                    ] {
                        self.patterns(manifest, workspace.get(key), kind);
                    }
                } else {
                    self.gap(manifest, "workspace must be a table.");
                }
            }
        } else if let Some(workspaces) = doc.get("workspaces") {
            self.patterns(
                manifest,
                Some(workspaces.get("packages").unwrap_or(workspaces)),
                "workspace-member-pattern",
            );
        }
        let mut row = json!({"manifest": bounded_text(&manifest.to_string_lossy(), 512),
            "root": bounded_text(&manifest.parent().unwrap_or(Path::new("")).to_string_lossy(), 512),
            "ecosystem": if cargo { "cargo" } else { "npm" },
            "package_declared": package.is_some_and(Value::is_object),
            "workspace_declared": doc.get(if cargo { "workspace" } else { "workspaces" }).is_some(),
            "declarations": self.declarations.len() - before, "basis": "manifest-declaration"});
        for key in ["name", "version"] {
            row[key] = Value::Null;
            if let Some(value) = package.and_then(|p| p.get(key)) {
                if let Some(text) = value.as_str() {
                    row[key] = bounded_text(text, 160);
                } else if cargo && key == "version" && value.get("workspace") == Some(&json!(true))
                {
                    row["version_inherited"] = json!(true);
                } else {
                    self.gap(
                        manifest,
                        &format!("package {key} has an unsupported shape."),
                    );
                }
            }
        }
        if !cargo {
            if let Some(value) = doc.get("private") {
                if value.is_boolean() {
                    row["private"] = value.clone();
                } else {
                    self.gap(manifest, "package private must be a boolean.");
                }
            }
        }
        self.packages.push(row);
    }
}
