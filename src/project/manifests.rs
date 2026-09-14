use super::bounded_text;
use crate::scan::ScanOptions;
use anyhow::{ensure, Result};
use ignore::WalkBuilder;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ecosystem {
    Cargo,
    Npm,
    Go,
    Python,
}

impl Ecosystem {
    fn of(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()? {
            "Cargo.toml" => Some(Self::Cargo),
            "package.json" => Some(Self::Npm),
            "go.mod" => Some(Self::Go),
            "pyproject.toml" => Some(Self::Python),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Npm => "npm",
            Self::Go => "go-modules",
            Self::Python => "python-project",
        }
    }
}

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
        if Ecosystem::of(entry.path()).is_none() {
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
        ensure!(
            super::manifest_inventory_allowed(result.len(), 0),
            "project manifest inventory exceeds 1024 files."
        );
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
            let ecosystem = Ecosystem::of(path).unwrap();
            if ecosystem == Ecosystem::Go {
                result.read_go(relative, source);
                ensure!(
                    super::manifest_inventory_allowed(snapshots.len(), result.declarations.len()),
                    "project manifest inventory exceeds 65536 declarations."
                );
                result
                    .documents
                    .insert(relative.to_path_buf(), json!({"ecosystem":"go-modules"}));
                continue;
            }
            let parsed = if matches!(ecosystem, Ecosystem::Cargo | Ecosystem::Python) {
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
                if ecosystem == Ecosystem::Python {
                    result.read_python(relative, &parsed);
                } else {
                    result.read(relative, &parsed, ecosystem == Ecosystem::Cargo);
                }
                ensure!(
                    super::manifest_inventory_allowed(snapshots.len(), result.declarations.len()),
                    "project manifest inventory exceeds 65536 declarations."
                );
                result.documents.insert(relative.to_path_buf(), parsed);
            } else {
                result.gap(
                    relative,
                    if matches!(ecosystem, Ecosystem::Cargo | Ecosystem::Python) {
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

    fn dependency_row(
        &mut self,
        manifest: &Path,
        name: &str,
        requirement: &str,
        section: &str,
        scope: &str,
    ) {
        self.declaration(
            manifest,
            json!({"kind":"dependency","name":bounded_text(name,160),
                "requirement":bounded_text(requirement,512),"section":section,"scope":scope,
                "target_condition":null,"resolution":"not-attempted"}),
        );
    }

    fn python_requirement(
        &mut self,
        manifest: &Path,
        value: &Value,
        section: &str,
        group: Option<&str>,
    ) {
        let Some(requirement) = value.as_str() else {
            self.gap(manifest, &format!("{section} contains a non-string entry."));
            return;
        };
        let name = requirement
            .trim()
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(character, '[' | '<' | '>' | '=' | '!' | '~' | '@' | ';')
            })
            .next()
            .unwrap_or("");
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        {
            self.gap(
                manifest,
                &format!("{section} contains an invalid requirement."),
            );
            return;
        }
        let before = self.declarations.len();
        self.dependency_row(manifest, name, requirement, section, "package");
        if let Some(group) = group {
            self.declarations[before].1["group"] = bounded_text(group, 160);
        }
    }

    fn python_array(
        &mut self,
        manifest: &Path,
        value: Option<&Value>,
        section: &str,
        group: Option<&str>,
    ) {
        let Some(value) = value else { return };
        let Some(values) = value.as_array() else {
            self.gap(manifest, &format!("{section} must be an array."));
            return;
        };
        for value in values {
            self.python_requirement(manifest, value, section, group);
        }
    }

    fn read_python(&mut self, manifest: &Path, doc: &Value) {
        let before = self.declarations.len();
        let project = doc.get("project").and_then(Value::as_object);
        let poetry = doc
            .get("tool")
            .and_then(|tool| tool.get("poetry"))
            .and_then(Value::as_object);
        self.python_array(
            manifest,
            project.and_then(|project| project.get("dependencies")),
            "project.dependencies",
            None,
        );
        if let Some(groups) = project
            .and_then(|project| project.get("optional-dependencies"))
            .and_then(Value::as_object)
        {
            for (group, values) in groups {
                self.python_array(
                    manifest,
                    Some(values),
                    "project.optional-dependencies",
                    Some(group),
                );
            }
        }
        if let Some(groups) = doc.get("dependency-groups").and_then(Value::as_object) {
            for (group, values) in groups {
                self.python_array(manifest, Some(values), "dependency-groups", Some(group));
            }
        }
        self.python_array(
            manifest,
            doc.get("build-system")
                .and_then(|build| build.get("requires")),
            "build-system.requires",
            None,
        );
        if let Some(dependencies) = poetry
            .and_then(|poetry| poetry.get("dependencies"))
            .and_then(Value::as_object)
        {
            for (name, spec) in dependencies {
                if name == "python" {
                    continue;
                }
                let requirement = spec
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| serde_json::to_string(spec).ok())
                    .unwrap_or_default();
                self.dependency_row(
                    manifest,
                    name,
                    &requirement,
                    "tool.poetry.dependencies",
                    "package",
                );
            }
        }
        if let Some(groups) = poetry
            .and_then(|poetry| poetry.get("group"))
            .and_then(Value::as_object)
        {
            for (group, table) in groups {
                if let Some(dependencies) = table.get("dependencies").and_then(Value::as_object) {
                    for (name, spec) in dependencies {
                        let requirement = spec
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| serde_json::to_string(spec).ok())
                            .unwrap_or_default();
                        let position = self.declarations.len();
                        self.dependency_row(
                            manifest,
                            name,
                            &requirement,
                            "tool.poetry.group.dependencies",
                            "package",
                        );
                        self.declarations[position].1["group"] = bounded_text(group, 160);
                    }
                }
            }
        }
        let uv_workspace = doc
            .get("tool")
            .and_then(|tool| tool.get("uv"))
            .and_then(|uv| uv.get("workspace"));
        if let Some(workspace) = uv_workspace {
            for (key, kind) in [
                ("members", "workspace-member-pattern"),
                ("exclude", "workspace-exclude-pattern"),
            ] {
                self.patterns(manifest, workspace.get(key), kind);
            }
        }
        let package = project.or(poetry);
        let mut row = json!({"manifest":bounded_text(&manifest.to_string_lossy(),512),
            "root":bounded_text(&manifest.parent().unwrap_or(Path::new("")).to_string_lossy(),512),
            "ecosystem":Ecosystem::Python.name(),"package_declared":package.is_some(),
            "workspace_declared":uv_workspace.is_some(),"declarations":self.declarations.len()-before,
            "basis":"manifest-declaration","name":null,"version":null});
        for key in ["name", "version"] {
            if let Some(value) = package.and_then(|package| package.get(key)) {
                if let Some(value) = value.as_str() {
                    row[key] = bounded_text(value, 160);
                } else {
                    self.gap(
                        manifest,
                        &format!("package {key} has an unsupported shape."),
                    );
                }
            }
        }
        self.packages.push(row);
    }

    fn read_go(&mut self, manifest: &Path, source: &str) {
        let before = self.declarations.len();
        let mut module = None;
        let mut go_version = None;
        let mut toolchain = None;
        let mut block = None;
        for raw in source.lines() {
            let code = raw.split("//").next().unwrap_or("").trim();
            if code.is_empty() {
                continue;
            }
            if code == ")" {
                block = None;
                continue;
            }
            let (directive, value) = if let Some(directive) = block {
                (directive, code)
            } else {
                let (directive, value) = code.split_once(char::is_whitespace).unwrap_or((code, ""));
                let value = value.trim();
                if value == "(" {
                    block = Some(directive);
                    continue;
                }
                (directive, value)
            };
            match directive {
                "module" => module = Some(value.trim_matches('"').to_owned()),
                "go" => go_version = Some(value.to_owned()),
                "toolchain" => toolchain = Some(value.to_owned()),
                "require" => {
                    let fields = value.split_whitespace().collect::<Vec<_>>();
                    if fields.len() >= 2 {
                        let position = self.declarations.len();
                        self.dependency_row(manifest, fields[0], fields[1], "require", "package");
                        self.declarations[position].1["indirect"] =
                            json!(raw.contains("// indirect"));
                    } else {
                        self.gap(manifest, "require has an unsupported shape.");
                    }
                }
                "replace" => {
                    let Some((from, to)) = value.split_once("=>") else {
                        self.gap(manifest, "replace has an unsupported shape.");
                        continue;
                    };
                    let from = from.split_whitespace().collect::<Vec<_>>();
                    let to = to.split_whitespace().collect::<Vec<_>>();
                    if from.is_empty() || to.is_empty() || from.len() > 2 || to.len() > 2 {
                        self.gap(manifest, "replace has an unsupported shape.");
                        continue;
                    }
                    self.declaration(
                        manifest,
                        json!({"kind":"dependency-replacement","name":bounded_text(from[0],160),
                            "from_version":from.get(1).map(|value|bounded_text(value,512)),
                            "target":bounded_text(to[0],512),"target_version":to.get(1).map(|value|bounded_text(value,512)),
                            "local":to.len()==1,"section":"replace","scope":"package","resolution":"not-attempted"}),
                    );
                }
                "exclude" => {
                    let fields = value.split_whitespace().collect::<Vec<_>>();
                    if fields.len() == 2 {
                        self.declaration(manifest, json!({"kind":"dependency-exclusion","name":bounded_text(fields[0],160),
                            "requirement":bounded_text(fields[1],512),"section":"exclude","scope":"package","resolution":"not-attempted"}));
                    } else {
                        self.gap(manifest, "exclude has an unsupported shape.");
                    }
                }
                "retract" => self.declaration(
                    manifest,
                    json!({"kind":"version-retraction","requirement":bounded_text(value,512),
                        "section":"retract","scope":"package","resolution":"not-attempted"}),
                ),
                "tool" => self.declaration(
                    manifest,
                    json!({"kind":"tool-dependency","name":bounded_text(value,160),
                        "section":"tool","scope":"package","resolution":"not-attempted"}),
                ),
                "godebug" => {
                    let (name, value) = value.split_once('=').unwrap_or((value, ""));
                    if name.is_empty() || value.is_empty() {
                        self.gap(manifest, "godebug has an unsupported shape.");
                    } else {
                        self.declaration(
                            manifest,
                            json!({"kind":"build-setting","name":bounded_text(name,160),
                                "value":bounded_text(value,512),"section":"godebug","scope":"package"}),
                        );
                    }
                }
                _ => self.gap(
                    manifest,
                    &format!("unsupported go.mod directive: {directive}."),
                ),
            }
        }
        if module.as_deref().is_none_or(str::is_empty) {
            self.gap(manifest, "go.mod has no module path.");
        }
        self.packages.push(json!({"manifest":bounded_text(&manifest.to_string_lossy(),512),
            "root":bounded_text(&manifest.parent().unwrap_or(Path::new("")).to_string_lossy(),512),
            "ecosystem":Ecosystem::Go.name(),"package_declared":module.is_some(),"workspace_declared":false,
            "declarations":self.declarations.len()-before,"basis":"manifest-declaration",
            "name":module.map(|value|bounded_text(&value,160)),"version":null,
            "go_version":go_version,"toolchain":toolchain}));
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
