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
            "Cargo.lock" => Some(Self::Cargo),
            "package-lock.json" | "npm-shrinkwrap.json" => Some(Self::Npm),
            "go.sum" => Some(Self::Go),
            "poetry.lock" | "uv.lock" | "Pipfile.lock" => Some(Self::Python),
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

    fn manifest(self) -> &'static str {
        match self {
            Self::Cargo => "Cargo.toml",
            Self::Npm => "package.json",
            Self::Go => "go.mod",
            Self::Python => "pyproject.toml",
        }
    }
}

#[derive(PartialEq, Eq, Serialize)]
pub(super) enum Snapshot {
    Source(String),
    Skipped(String),
}

#[derive(Default)]
pub(super) struct Lockfiles {
    pub snapshots: BTreeMap<PathBuf, Snapshot>,
    pub resolutions: Vec<(PathBuf, Option<PathBuf>, Value)>,
    pub gaps: Vec<Value>,
    overflowed: bool,
}

pub(super) fn discover(
    selected: &Path,
    options: &ScanOptions,
) -> Result<BTreeMap<PathBuf, Snapshot>> {
    let mut result = BTreeMap::new();
    let selected_ecosystem = match selected.file_name().and_then(|name| name.to_str()) {
        Some("Cargo.toml") => Some(Ecosystem::Cargo),
        Some("package.json") => Some(Ecosystem::Npm),
        Some("go.mod") => Some(Ecosystem::Go),
        Some("pyproject.toml") => Some(Ecosystem::Python),
        _ => None,
    };
    let walk_root = selected_ecosystem
        .and_then(|_| selected.parent())
        .unwrap_or(selected);
    for entry in WalkBuilder::new(walk_root)
        .standard_filters(options.respect_ignore)
        .hidden(options.respect_ignore)
        .git_ignore(options.respect_ignore)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".fr-history")
        .build()
    {
        let entry = entry?;
        let Some(ecosystem) = Ecosystem::of(entry.path()) else {
            continue;
        };
        if selected_ecosystem.is_some_and(|selected_ecosystem| {
            ecosystem != selected_ecosystem || entry.path().parent() != selected.parent()
        }) {
            continue;
        }
        let snapshot = if entry.path_is_symlink() {
            Snapshot::Skipped("symlink lockfile".into())
        } else if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        } else if entry.metadata()?.len() > options.max_file_bytes {
            Snapshot::Skipped("lockfile exceeds max-file-size".into())
        } else {
            match crate::vfs::read_to_string(entry.path()) {
                Ok(source) if source.len() as u64 <= options.max_file_bytes => {
                    Snapshot::Source(source)
                }
                Ok(_) => Snapshot::Skipped("lockfile exceeds max-file-size".into()),
                Err(_) => Snapshot::Skipped("lockfile is unreadable or not UTF-8".into()),
            }
        };
        result.insert(entry.into_path(), snapshot);
        ensure!(
            super::lockfile_inventory_allowed(result.len(), 0),
            "project lockfile inventory exceeds 1024 files."
        );
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
        toml_edit::Item::ArrayOfTables(tables) => tables
            .iter()
            .map(|table| toml_value(&toml_edit::Item::Table(table.clone())))
            .collect(),
        toml_edit::Item::Value(value) => match value {
            toml_edit::Value::String(value) => json!(value.value()),
            toml_edit::Value::Boolean(value) => json!(value.value()),
            toml_edit::Value::Integer(value) => json!(value.value()),
            toml_edit::Value::Float(value) => json!(value.value()),
            toml_edit::Value::Array(value) => value
                .iter()
                .map(|value| toml_value(&toml_edit::Item::Value(value.clone())))
                .collect(),
            toml_edit::Value::InlineTable(value) => value
                .iter()
                .map(|(key, value)| {
                    (
                        key.to_owned(),
                        toml_value(&toml_edit::Item::Value(value.clone())),
                    )
                })
                .collect(),
            toml_edit::Value::Datetime(_) => Value::Null,
        },
        _ => Value::Null,
    }
}

fn text(value: Option<&Value>, limit: usize) -> Value {
    value
        .and_then(Value::as_str)
        .map_or(Value::Null, |value| bounded_text(value, limit))
}

impl Lockfiles {
    pub fn new(
        selected: &Path,
        root: &Path,
        options: &ScanOptions,
        manifests: &BTreeMap<PathBuf, super::manifests::Snapshot>,
    ) -> Result<Self> {
        let snapshots = discover(selected, options)?;
        let mut result = Self::default();
        for (path, snapshot) in &snapshots {
            let relative = path.strip_prefix(root)?;
            let ecosystem = Ecosystem::of(path).unwrap();
            let manifest = relative
                .parent()
                .unwrap_or(Path::new(""))
                .join(ecosystem.manifest());
            let manifest = manifests
                .contains_key(&root.join(&manifest))
                .then_some(manifest);
            let Snapshot::Source(source) = snapshot else {
                if let Snapshot::Skipped(reason) = snapshot {
                    result.gap(relative, reason);
                }
                continue;
            };
            match ecosystem {
                Ecosystem::Cargo => result.read_toml(relative, manifest.as_deref(), source, true),
                Ecosystem::Npm => result.read_npm(relative, manifest.as_deref(), source),
                Ecosystem::Go => result.read_go(relative, manifest.as_deref(), source),
                Ecosystem::Python
                    if relative
                        .file_name()
                        .is_some_and(|name| name == "Pipfile.lock") =>
                {
                    result.read_pipfile(relative, manifest.as_deref(), source)
                }
                Ecosystem::Python => result.read_toml(relative, manifest.as_deref(), source, false),
            }
            ensure!(
                !result.overflowed
                    && super::lockfile_inventory_allowed(
                        snapshots.len(),
                        result.resolutions.len() + result.gaps.len(),
                    ),
                "project lockfile inventory exceeds 262144 evidence rows."
            );
        }
        result.snapshots = snapshots;
        Ok(result)
    }

    fn gap(&mut self, lockfile: &Path, reason: &str) {
        if self.resolutions.len() + self.gaps.len() >= 262_144 {
            self.overflowed = true;
            return;
        }
        self.gaps.push(json!({
            "path": bounded_text(&lockfile.to_string_lossy(), 512),
            "reason": bounded_text(reason, 512),
            "scope": "lockfile"
        }));
    }

    fn row(
        &mut self,
        lockfile: &Path,
        manifest: Option<&Path>,
        ecosystem: Ecosystem,
        mut row: Value,
    ) {
        if self.resolutions.len() + self.gaps.len() >= 262_144 {
            self.overflowed = true;
            return;
        }
        row["lockfile"] = bounded_text(&lockfile.to_string_lossy(), 512);
        row["manifest"] = manifest.map_or(Value::Null, |path| {
            bounded_text(&path.to_string_lossy(), 512)
        });
        row["ecosystem"] = json!(ecosystem.name());
        row["basis"] = json!("captured-lockfile-entry");
        self.resolutions
            .push((lockfile.to_path_buf(), manifest.map(Path::to_path_buf), row));
    }

    fn read_toml(&mut self, lockfile: &Path, manifest: Option<&Path>, source: &str, cargo: bool) {
        let Ok(document) = source.parse::<toml_edit::DocumentMut>() else {
            self.gap(lockfile, "invalid TOML syntax");
            return;
        };
        let document = toml_value(document.as_item());
        let Some(packages) = document.get("package").and_then(Value::as_array) else {
            self.gap(lockfile, "lockfile has no package array");
            return;
        };
        for package in packages {
            let Some(name) = package.get("name").and_then(Value::as_str) else {
                self.gap(lockfile, "package entry has no string name");
                continue;
            };
            let Some(version) = package.get("version").and_then(Value::as_str) else {
                self.gap(lockfile, "package entry has no string version");
                continue;
            };
            let ecosystem = if cargo {
                Ecosystem::Cargo
            } else {
                Ecosystem::Python
            };
            self.row(
                lockfile,
                manifest,
                ecosystem,
                json!({
                    "name": bounded_text(name, 160),
                    "version": bounded_text(version, 160),
                    "source": origin(package.get("source")),
                    "checksum": text(package.get("checksum").or_else(|| package.pointer("/sdist/hash")), 512),
                    "dependencies": package.get("dependencies").and_then(Value::as_array).map_or(0, Vec::len),
                    "artifacts": package.get("files").or_else(|| package.get("wheels")).and_then(Value::as_array).map_or(0, Vec::len),
                }),
            );
        }
    }

    fn read_npm(&mut self, lockfile: &Path, manifest: Option<&Path>, source: &str) {
        let Ok(document) = serde_json::from_str::<Value>(source) else {
            self.gap(lockfile, "invalid JSON syntax");
            return;
        };
        let Some(document) = document.as_object() else {
            self.gap(lockfile, "lockfile must be a JSON object");
            return;
        };
        if let Some(packages) = document.get("packages").and_then(Value::as_object) {
            for (location, package) in packages {
                if location.is_empty() {
                    continue;
                }
                let name = package
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| npm_name(location));
                let version = package.get("version").and_then(Value::as_str);
                let Some(name) = name else {
                    self.gap(lockfile, "package entry has no string name");
                    continue;
                };
                if version.is_none() && package.get("link") != Some(&json!(true)) {
                    self.gap(lockfile, "package entry has no string version");
                    continue;
                }
                self.npm_row(lockfile, manifest, name, version, package, Some(location));
            }
            return;
        }
        let Some(dependencies) = document.get("dependencies").and_then(Value::as_object) else {
            self.gap(lockfile, "lockfile has no packages or dependencies object");
            return;
        };
        self.read_npm_dependencies(lockfile, manifest, dependencies, "");
    }

    fn read_npm_dependencies(
        &mut self,
        lockfile: &Path,
        manifest: Option<&Path>,
        dependencies: &serde_json::Map<String, Value>,
        parent: &str,
    ) {
        for (name, package) in dependencies {
            let Some(version) = package.get("version").and_then(Value::as_str) else {
                self.gap(lockfile, "dependency entry has no string version");
                continue;
            };
            let location = if parent.is_empty() {
                format!("node_modules/{name}")
            } else {
                format!("{parent}/node_modules/{name}")
            };
            self.npm_row(
                lockfile,
                manifest,
                name,
                Some(version),
                package,
                Some(&location),
            );
            if let Some(children) = package.get("dependencies").and_then(Value::as_object) {
                self.read_npm_dependencies(lockfile, manifest, children, &location);
            }
        }
    }

    fn npm_row(
        &mut self,
        lockfile: &Path,
        manifest: Option<&Path>,
        name: &str,
        version: Option<&str>,
        package: &Value,
        location: Option<&str>,
    ) {
        self.row(
            lockfile,
            manifest,
            Ecosystem::Npm,
            json!({
                "name": bounded_text(name, 160),
                "version": version.map_or(Value::Null, |value| bounded_text(value, 160)),
                "location": location.map_or(Value::Null, |value| bounded_text(value, 512)),
                "resolved": text(package.get("resolved"), 512),
                "integrity": text(package.get("integrity"), 512),
                "development": package.get("dev").and_then(Value::as_bool).unwrap_or(false),
                "optional": package.get("optional").and_then(Value::as_bool).unwrap_or(false),
                "link": package.get("link").and_then(Value::as_bool).unwrap_or(false),
                "dependencies": package.get("dependencies").and_then(Value::as_object).map_or(0, serde_json::Map::len),
            }),
        );
    }

    fn read_go(&mut self, lockfile: &Path, manifest: Option<&Path>, source: &str) {
        for line in source.lines().filter(|line| !line.trim().is_empty()) {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 3 {
                self.gap(
                    lockfile,
                    "go.sum entry must have module, version and checksum",
                );
                continue;
            }
            let (version, artifact) = fields[1]
                .strip_suffix("/go.mod")
                .map_or((fields[1], "module"), |version| (version, "go-mod"));
            self.row(
                lockfile,
                manifest,
                Ecosystem::Go,
                json!({
                    "name": bounded_text(fields[0], 160),
                    "version": bounded_text(version, 160),
                    "artifact": artifact,
                    "checksum": bounded_text(fields[2], 512),
                }),
            );
        }
    }

    fn read_pipfile(&mut self, lockfile: &Path, manifest: Option<&Path>, source: &str) {
        let Ok(document) = serde_json::from_str::<Value>(source) else {
            self.gap(lockfile, "invalid JSON syntax");
            return;
        };
        for group in ["default", "develop"] {
            let Some(packages) = document.get(group).and_then(Value::as_object) else {
                continue;
            };
            for (name, package) in packages {
                let Some(version) = package.get("version").and_then(Value::as_str) else {
                    self.gap(lockfile, "Pipfile.lock entry has no string version");
                    continue;
                };
                self.row(
                    lockfile,
                    manifest,
                    Ecosystem::Python,
                    json!({
                        "name": bounded_text(name, 160),
                        "version": bounded_text(version.trim_start_matches('='), 160),
                        "group": group,
                        "markers": text(package.get("markers"), 512),
                        "hashes": package.get("hashes").and_then(Value::as_array).map_or(0, Vec::len),
                    }),
                );
            }
        }
    }
}

fn npm_name(location: &str) -> Option<&str> {
    let tail = location.rsplit("node_modules/").next()?;
    if tail.starts_with('@') {
        let mut parts = tail.split('/');
        parts.next()?;
        parts.next()?;
        let end = tail
            .match_indices('/')
            .nth(1)
            .map_or(tail.len(), |(at, _)| at);
        Some(&tail[..end])
    } else {
        tail.split('/').next()
    }
}

fn origin(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let Some(value) = value.as_str() {
        return bounded_text(value, 512);
    }
    let Some(object) = value.as_object() else {
        return Value::Null;
    };
    let mut result = serde_json::Map::new();
    for key in [
        "registry",
        "git",
        "path",
        "url",
        "reference",
        "resolved_reference",
    ] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            result.insert(key.into(), bounded_text(value, 512));
        }
    }
    Value::Object(result)
}
