use super::bounded_text;
use crate::scan::ScanOptions;
use anyhow::{ensure, Result};
use clap::Args;
use ignore::WalkBuilder;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::BTreeMap;
use std::io::Read;
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

    fn of_manifest(path: &Path) -> Option<Self> {
        match path.file_name()?.to_str()? {
            "Cargo.toml" => Some(Self::Cargo),
            "package.json" => Some(Self::Npm),
            "go.mod" => Some(Self::Go),
            "pyproject.toml" => Some(Self::Python),
            _ => None,
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
    pub resolutions: Vec<Resolution>,
    pub gaps: Vec<Value>,
    overflowed: bool,
}

pub(super) struct Resolution {
    pub lockfile: PathBuf,
    ecosystem: Ecosystem,
    name: String,
    version: Option<String>,
    checksums: Vec<ArtifactChecksum>,
    pub row: Value,
}

#[derive(Clone)]
struct ArtifactChecksum {
    locator: Option<String>,
    value: String,
    content: &'static str,
}

#[derive(Args)]
pub struct ArtifactOptions {
    #[arg(
        long,
        help = "Select a discovered lockfile relative to the project root."
    )]
    pub lockfile: PathBuf,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub version: String,
    #[arg(
        long,
        help = "Hash this artifact file or extracted Go module directory."
    )]
    pub artifact: PathBuf,
    #[arg(
        long,
        help = "Logical module@version prefix for an extracted Go module directory."
    )]
    pub go_prefix: Option<String>,
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

fn checksum(
    locator: Option<&str>,
    value: Option<&Value>,
    content: &'static str,
) -> Vec<ArtifactChecksum> {
    value
        .and_then(Value::as_str)
        .map_or_else(Vec::new, |value| {
            vec![ArtifactChecksum {
                locator: locator.map(str::to_owned),
                value: value.to_owned(),
                content,
            }]
        })
}

fn python_checksums(package: &Value) -> Vec<ArtifactChecksum> {
    let mut result = checksum(
        package
            .pointer("/sdist/url")
            .and_then(Value::as_str)
            .and_then(|url| url.rsplit('/').next()),
        package.pointer("/sdist/hash"),
        "file",
    );
    for value in [package.get("files"), package.get("wheels")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
    {
        let locator = value
            .get("file")
            .or_else(|| value.get("url"))
            .and_then(Value::as_str)
            .and_then(|value| value.rsplit('/').next());
        result.extend(checksum(locator, value.get("hash"), "file"));
    }
    result
}

fn hexadecimal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        result.push(ALPHABET[((bits >> 18) & 63) as usize] as char);
        result.push(ALPHABET[((bits >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            ALPHABET[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

fn read_digest<D: Digest + Default>(path: &Path, total: &mut u64) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "artifact entry is not a regular file."
    );
    ensure!(
        metadata.len() <= 536_870_912,
        "artifact file exceeds 512 MiB."
    );
    *total = total
        .checked_add(metadata.len())
        .ok_or_else(|| anyhow::anyhow!("artifact byte count overflowed."))?;
    ensure!(*total <= 536_870_912, "artifact input exceeds 512 MiB.");
    let mut file = std::fs::File::open(path)?;
    let mut digest = D::default();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().to_vec())
}

fn go_tree_digest(root: &Path, prefix: &str, go_mod_only: bool) -> Result<(Vec<u8>, usize, u64)> {
    ensure!(
        !prefix.is_empty() && prefix.len() <= 512,
        "--go-prefix must contain 1 through 512 bytes."
    );
    ensure!(
        !prefix.starts_with('/')
            && !prefix.contains('\\')
            && !prefix
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".."),
        "--go-prefix must be a safe slash-separated relative path."
    );
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.file_type().is_dir(),
        "Go checksum verification requires an extracted module directory."
    );
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.path() == root {
            continue;
        }
        ensure!(
            !entry.file_type().is_symlink(),
            "artifact directories cannot contain symlinks."
        );
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(root)?;
        if go_mod_only && relative != Path::new("go.mod") {
            continue;
        }
        let relative = relative
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("artifact directory contains a non-UTF-8 path."))?;
        ensure!(
            !relative.contains(['\\', '\n', '\r']),
            "artifact directory contains a path unsupported by the Go checksum format."
        );
        let logical = format!("{prefix}/{relative}");
        files.push((logical, entry.into_path()));
        ensure!(
            files.len() <= 65_536,
            "artifact directory exceeds 65536 files."
        );
    }
    ensure!(
        !files.is_empty(),
        "artifact directory contains no selected regular files."
    );
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut total = 0u64;
    let mut listing = Sha256::new();
    for (logical, path) in &files {
        let digest = read_digest::<Sha256>(path, &mut total)?;
        listing.update(format!("{}  {logical}\n", hexadecimal(&digest)).as_bytes());
    }
    Ok((listing.finalize().to_vec(), files.len(), total))
}

fn expected_parts(value: &str) -> impl Iterator<Item = (&str, &str, &'static str)> {
    value.split_whitespace().filter_map(|part| {
        let part = part.split('?').next().unwrap_or(part);
        if let Some(digest) = part.strip_prefix("h1:") {
            Some(("h1", digest, "base64"))
        } else if let Some((algorithm, digest)) = part.split_once('-') {
            Some((algorithm, digest, "base64"))
        } else if let Some((algorithm, digest)) = part.split_once(':') {
            Some((algorithm, digest, "hex"))
        } else if part.len() == 64 && part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Some(("sha256", part, "hex"))
        } else {
            None
        }
    })
}

fn file_digests(path: &Path) -> Result<(BTreeMap<&'static str, Vec<u8>>, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "artifact is not a regular file."
    );
    ensure!(
        metadata.len() <= 536_870_912,
        "artifact file exceeds 512 MiB."
    );
    let mut file = std::fs::File::open(path)?;
    let mut sha256 = Sha256::new();
    let mut sha384 = Sha384::new();
    let mut sha512 = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha256.update(&buffer[..count]);
        sha384.update(&buffer[..count]);
        sha512.update(&buffer[..count]);
    }
    Ok((
        BTreeMap::from([
            ("sha256", sha256.finalize().to_vec()),
            ("sha384", sha384.finalize().to_vec()),
            ("sha512", sha512.finalize().to_vec()),
        ]),
        metadata.len(),
    ))
}

impl Lockfiles {
    pub fn verify_artifact(
        &self,
        lockfile: &Path,
        options: &ArtifactOptions,
        artifact: &Path,
    ) -> Result<Value> {
        ensure!(
            !options.name.is_empty() && options.name.len() <= 160,
            "package name must contain 1 through 160 bytes."
        );
        ensure!(
            !options.version.is_empty() && options.version.len() <= 160,
            "package version must contain 1 through 160 bytes."
        );
        let candidates = self
            .resolutions
            .iter()
            .filter(|candidate| {
                candidate.lockfile == lockfile
                    && candidate.version.as_deref() == Some(options.version.as_str())
                    && package_name_equal(candidate.ecosystem, &candidate.name, &options.name)
            })
            .collect::<Vec<_>>();
        let artifact_name = artifact.file_name().and_then(|name| name.to_str());
        let mut expectations = candidates
            .iter()
            .flat_map(|candidate| candidate.checksums.iter())
            .collect::<Vec<_>>();
        if expectations
            .iter()
            .any(|expected| expected.locator.as_deref() == artifact_name)
        {
            expectations.retain(|expected| {
                expected.locator.as_deref() == artifact_name || expected.locator.is_none()
            });
        }
        let needs_go = expectations
            .iter()
            .any(|expected| expected.content.starts_with("go-"));
        let metadata = std::fs::symlink_metadata(artifact)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "artifact path cannot be a symlink."
        );
        ensure!(
            metadata.file_type().is_dir() == needs_go,
            if needs_go {
                "Go checksum verification requires an extracted module directory."
            } else {
                "package checksum verification requires a regular artifact file."
            }
        );
        let (file_hashes, artifact_bytes) = if needs_go {
            (BTreeMap::new(), 0)
        } else {
            file_digests(artifact)?
        };
        let mut go_hashes = BTreeMap::new();
        let mut go_files = 0usize;
        let mut go_bytes = 0u64;
        if needs_go {
            let prefix = options.go_prefix.as_deref().ok_or_else(|| {
                anyhow::anyhow!("--go-prefix is required for Go checksum verification.")
            })?;
            for (content, only_go_mod) in [("go-module", false), ("go-mod", true)] {
                if expectations
                    .iter()
                    .any(|expected| expected.content == content)
                {
                    let (digest, files, bytes) = go_tree_digest(artifact, prefix, only_go_mod)?;
                    go_hashes.insert(content, digest);
                    go_files = go_files.max(files);
                    go_bytes = go_bytes.max(bytes);
                }
            }
        }
        let mut rows = Vec::new();
        let mut checked = 0usize;
        let mut matched = 0usize;
        let mut unsupported = 0usize;
        for expected in &expectations {
            let mut parsed_any = false;
            for (algorithm, wanted, encoding) in expected_parts(&expected.value) {
                parsed_any = true;
                let actual = if algorithm == "h1" {
                    go_hashes.get(expected.content)
                } else {
                    file_hashes.get(algorithm)
                };
                let status = super::artifact_verification_status(true, actual.is_some(), false);
                let Some(actual) = actual else {
                    unsupported += 1;
                    if rows.len() < 64 {
                        rows.push(json!({"algorithm": bounded_text(algorithm, 32), "encoding": encoding,
                            "locator": expected.locator.as_deref().map(|value| bounded_text(value, 512)),
                            "content": expected.content, "status": if status == 1 { "unsupported-algorithm" } else { "invalid" }}));
                    }
                    continue;
                };
                checked += 1;
                let actual_text = if encoding == "hex" {
                    hexadecimal(actual)
                } else {
                    base64(actual)
                };
                let equal = if encoding == "hex" {
                    actual_text.eq_ignore_ascii_case(wanted)
                } else {
                    actual_text.trim_end_matches('=') == wanted.trim_end_matches('=')
                };
                let status = super::artifact_verification_status(true, true, equal);
                if equal {
                    matched += 1;
                }
                if rows.len() < 64 {
                    rows.push(json!({"algorithm": algorithm, "encoding": encoding,
                        "locator": expected.locator.as_deref().map(|value| bounded_text(value, 512)),
                        "content": expected.content, "status": if status == 3 { "matched" } else { "mismatched" },
                        "actual": bounded_text(&actual_text, 512)}));
                }
            }
            if !parsed_any {
                unsupported += 1;
                if rows.len() < 64 {
                    rows.push(json!({
                        "locator": expected.locator.as_deref().map(|value| bounded_text(value, 512)),
                        "content": expected.content,
                        "status": "unrecognized-checksum"
                    }));
                }
            }
        }
        let status = if candidates.is_empty() {
            "lock-entry-not-found"
        } else if expectations.is_empty() {
            "checksum-not-recorded"
        } else if matched > 0 {
            "verified"
        } else if checked > 0 {
            "mismatch"
        } else {
            "unsupported-checksum"
        };
        Ok(json!({
            "schema": "fr-artifact-verification-1", "status": status,
            "lockfile": bounded_text(&lockfile.to_string_lossy(), 512),
            "package": {"name": bounded_text(&options.name, 160), "version": bounded_text(&options.version, 160)},
            "artifact": bounded_text(&options.artifact.to_string_lossy(), 512),
            "candidate_count": candidates.len(), "expectation_count": expectations.len(),
            "checked_count": checked, "matched_count": matched, "unsupported_count": unsupported,
            "checks": rows, "checks_omitted": checked.saturating_add(unsupported).saturating_sub(64),
            "artifact_bytes": if needs_go { go_bytes } else { artifact_bytes },
            "artifact_files": if needs_go { go_files } else { 1 },
            "basis": "captured-lockfile-checksum-and-observed-artifact-bytes"
        }))
    }

    pub fn applicable(&self, root: &Path, manifest: &Path) -> Option<PathBuf> {
        let ecosystem = Ecosystem::of_manifest(manifest)?;
        let directory = manifest.parent().unwrap_or(Path::new(""));
        self.snapshots
            .keys()
            .filter_map(|path| path.strip_prefix(root).ok())
            .filter(|path| Ecosystem::of(path) == Some(ecosystem))
            .filter(|path| {
                path.parent()
                    .is_some_and(|parent| directory.starts_with(parent))
            })
            .max_by_key(|path| path.components().count())
            .map(Path::to_path_buf)
    }

    pub fn declaration_resolution(
        &self,
        root: &Path,
        manifest: &Path,
        identity: Option<&str>,
        declaration: &Value,
    ) -> Option<Value> {
        if declaration.get("kind") != Some(&json!("dependency")) {
            return None;
        }
        let ecosystem = Ecosystem::of_manifest(manifest)?;
        let declared = identity?;
        let resolved = match ecosystem {
            Ecosystem::Cargo => declaration
                .get("package")
                .and_then(Value::as_str)
                .unwrap_or(declared),
            Ecosystem::Npm => declaration
                .get("requirement")
                .and_then(Value::as_str)
                .and_then(npm_alias)
                .unwrap_or(declared),
            Ecosystem::Go | Ecosystem::Python => declared,
        };
        let lockfile = self.applicable(root, manifest);
        let candidates = lockfile
            .as_ref()
            .map(|lockfile| {
                self.resolutions
                    .iter()
                    .filter(|candidate| {
                        super::dependency_resolution_candidate(
                            candidate.lockfile == *lockfile,
                            candidate.ecosystem == ecosystem,
                            package_name_equal(ecosystem, &candidate.name, resolved),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let count = candidates.len();
        let summaries = candidates
            .iter()
            .take(16)
            .map(|candidate| {
                json!({
                    "version": candidate.row.get("version").cloned().unwrap_or(Value::Null),
                    "artifact": candidate.row.get("artifact").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Vec<_>>();
        Some(json!({
            "resolution": if lockfile.is_none() { "lockfile-not-observed" } else if count == 0 { "no-lockfile-candidate" } else { "captured-lockfile-candidates" },
            "resolved_name": bounded_text(resolved, 160),
            "lockfile": lockfile.as_ref().map(|path| bounded_text(&path.to_string_lossy(), 512)),
            "locked_candidates": summaries,
            "locked_candidate_count": count,
            "locked_candidates_omitted": count.saturating_sub(16),
        }))
    }

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
        name: &str,
        artifact: (Option<&str>, Vec<ArtifactChecksum>),
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
        self.resolutions.push(Resolution {
            lockfile: lockfile.to_path_buf(),
            ecosystem,
            name: name.to_owned(),
            version: artifact.0.map(str::to_owned),
            checksums: artifact.1,
            row,
        });
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
            let checksums = if cargo {
                checksum(None, package.get("checksum"), "file")
            } else {
                python_checksums(package)
            };
            self.row(
                lockfile,
                manifest,
                ecosystem,
                name,
                (Some(version), checksums),
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
            name,
            (version, checksum(None, package.get("integrity"), "file")),
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
                fields[0],
                (
                    Some(version),
                    checksum(
                        None,
                        Some(&json!(fields[2])),
                        if artifact == "module" {
                            "go-module"
                        } else {
                            "go-mod"
                        },
                    ),
                ),
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
                    name,
                    (
                        Some(version.trim_start_matches('=')),
                        package.get("hashes").and_then(Value::as_array).map_or_else(Vec::new, |hashes| {
                            hashes.iter().flat_map(|value| checksum(None, Some(value), "file")).collect()
                        }),
                    ),
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

fn npm_alias(requirement: &str) -> Option<&str> {
    let alias = requirement.strip_prefix("npm:")?;
    if alias.starts_with('@') {
        let package_end = alias.find('/')? + 1;
        alias[package_end..]
            .find('@')
            .map_or(Some(alias), |offset| Some(&alias[..package_end + offset]))
    } else {
        Some(alias.split_once('@').map_or(alias, |(name, _)| name))
    }
}

fn package_name_equal(ecosystem: Ecosystem, left: &str, right: &str) -> bool {
    if ecosystem != Ecosystem::Python {
        return left == right;
    }
    python_name(left) == python_name(right)
}

fn python_name(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                result.push('-');
                separator = true;
            }
        } else {
            result.extend(character.to_lowercase());
            separator = false;
        }
    }
    result
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
