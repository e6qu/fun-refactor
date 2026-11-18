use super::{configuration, Check, Configuration};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

fn executable(root: &Path, check: &Check) -> Result<PathBuf> {
    let cwd = root.join(&check.cwd);
    let program = Path::new(&check.argv[0]);
    let candidates = if program.is_absolute() {
        vec![program.to_path_buf()]
    } else if program.components().count() > 1 {
        vec![cwd.join(program)]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| {
                if directory.is_absolute() {
                    directory.join(program)
                } else {
                    cwd.join(directory).join(program)
                }
            })
            .collect()
    };
    candidates
        .into_iter()
        .find(|path| {
            let Ok(metadata) = path.metadata() else {
                return false;
            };
            if !metadata.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
        .with_context(|| format!("cannot resolve executable for check {}", check.name))?
        .canonicalize()
        .map_err(Into::into)
}

pub(super) fn toolchain(root: &Path, config: &Configuration) -> Result<Value> {
    let mut programs = Vec::new();
    for check in &config.checks {
        let path = executable(root, check)?;
        let mut file = File::open(&path)?;
        ensure!(
            file.metadata()?.len() <= 268_435_456,
            "check executable exceeds identity budget"
        );
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        let mut bytes = 0;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n;
            ensure!(
                bytes <= 268_435_456,
                "check executable exceeds identity budget"
            );
            hash.update(&buffer[..n]);
        }
        let mut files = Vec::new();
        for declared in &check.identity_files {
            let path = if declared.is_absolute() {
                declared.clone()
            } else {
                root.join(&check.cwd).join(declared)
            };
            let resolved = path.canonicalize()?;
            ensure!(
                resolved.metadata()?.is_file(),
                "declared identity must be a regular file."
            );
            let mut file = File::open(&resolved)?;
            ensure!(
                file.metadata()?.is_file(),
                "declared identity must be a regular file."
            );
            ensure!(
                file.metadata()?.len() <= 268_435_456,
                "declared identity file exceeds 256 MiB."
            );
            let mut identity = Sha256::new();
            let mut count = 0;
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                count += n;
                ensure!(
                    count <= 268_435_456,
                    "declared identity file exceeds 256 MiB."
                );
                identity.update(&buffer[..n]);
            }
            files.push(json!({"declared":declared,"path":resolved,"bytes":count,"sha256":hex::encode(identity.finalize())}));
        }
        let environment = check
            .environment
            .iter()
            .map(|name| {
                let value = std::env::var_os(name).map(|value| value.as_encoded_bytes().to_vec());
                Ok(json!({"name":name,"digest":crate::project::hash(value)?}))
            })
            .collect::<Result<Vec<_>>>()?;
        programs.push(
            json!({"check":check.name,"path":path,"sha256":hex::encode(hash.finalize()),
            "identity_files":files,"environment":environment}),
        );
    }
    Ok(
        json!({"schema":"fr-check-toolchain-2","programs":programs,"build_configuration":build_configuration(root)?,
        "executor":crate::project::hash((include_str!("../checks.rs"), include_str!("evidence.rs")))?,
        "scope":"command executable bytes, workspace Rust build inputs, and declared environment keys and identity files. Undeclared libraries, imports and dependencies remain outside coverage."}),
    )
}

fn build_configuration(root: &Path) -> Result<Value> {
    let mut files = std::collections::BTreeMap::new();
    let mut total = 0;
    for entry in ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | ".fr-history" | "target" | "node_modules" | ".lake")
            )
        })
        .build()
    {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str());
        let cargo_config = path
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|name| name == ".cargo")
            && matches!(name, Some("config" | "config.toml"));
        if !cargo_config
            && !matches!(
                name,
                Some("Cargo.toml" | "Cargo.lock" | "rust-toolchain" | "rust-toolchain.toml")
            )
        {
            continue;
        }
        ensure!(!path.is_dir(), "Rust build input must be a regular file.");
        ensure!(
            files.len() < 1024,
            "Rust build input count exceeds 1024 files."
        );
        let resolved = path.canonicalize()?;
        ensure!(
            resolved.metadata()?.is_file(),
            "Rust build input must be a regular file."
        );
        let mut bytes = Vec::new();
        File::open(&resolved)?
            .take(4_194_305)
            .read_to_end(&mut bytes)?;
        total += bytes.len();
        ensure!(
            bytes.len() <= 4_194_304 && total <= 67_108_864,
            "Rust build inputs exceed the content budget."
        );
        files.insert(path.strip_prefix(root)?.to_string_lossy().into_owned(),
            json!({"path":resolved,"bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes))}));
    }
    Ok(json!({"schema":"fr-rust-build-inputs-1","files":files,
        "scope":"workspace Cargo manifests, lockfiles, toolchain files and .cargo configuration; directory symlinks and dependency directories are excluded."}))
}

pub(super) fn validate_declarations(check: &Check) -> Result<()> {
    let mut names = BTreeSet::new();
    ensure!(
        check.environment.len() <= 32 && check.identity_files.len() <= 16,
        "check identity declarations exceed their limits."
    );
    for name in &check.environment {
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && names.insert(name)
                && name.bytes().enumerate().all(|(i, byte)| byte == b'_'
                    || byte.is_ascii_alphabetic()
                    || i > 0 && byte.is_ascii_digit()),
            "environment identities need unique variable names."
        );
    }
    let mut files = BTreeSet::new();
    for path in &check.identity_files {
        ensure!(
            !path.as_os_str().is_empty()
                && files.insert(path)
                && (path.is_absolute()
                    || path.components().all(|c| matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    ))),
            "identity files need distinct absolute or confined relative paths."
        );
    }
    Ok(())
}

pub(crate) fn configuration_digest(root: &Path) -> Result<String> {
    Ok(configuration(root)?.1)
}

pub(crate) fn toolchain_digest(root: &Path) -> Result<String> {
    crate::project::hash(toolchain(root, &configuration(root)?.0)?)
}

pub(crate) fn validate(root: &Path, report: &Value) -> Result<Vec<(String, bool)>> {
    ensure!(
        report["schema"] == "fr-checks-1" && report["root"] == json!(root.canonicalize()?),
        "check report belongs to another workspace or schema."
    );
    let (config, basis) = configuration(root)?;
    ensure!(
        report["executed"] == true
            && report["configuration_stable"] == true
            && report["source_snapshot_stable"] == true
            && report["toolchain_stable"] == true,
        "check evidence needs executed, stable source, configuration and toolchain inputs."
    );
    ensure!(
        report["basis"] == basis,
        "check configuration changed since execution."
    );
    ensure!(
        report["source_revision"] == crate::history::source_revision(root)?,
        "check source snapshot changed since execution."
    );
    ensure!(
        report["toolchain"] == toolchain(root, &config)?,
        "check toolchain changed since execution"
    );
    let results = report["results"]
        .as_array()
        .context("check results are missing")?;
    ensure!(
        !results.is_empty() && results.len() <= config.checks.len(),
        "check evidence needs bounded results"
    );
    let mut names = BTreeSet::new();
    let mut accepted = Vec::new();
    for result in results {
        let name = result["name"]
            .as_str()
            .context("check result has no name")?;
        ensure!(names.insert(name), "duplicate check result");
        let check = config
            .checks
            .iter()
            .find(|check| check.name == name)
            .context("unknown check result")?;
        ensure!(
            result["argv"] == json!(check.argv)
                && result["cwd"] == json!(check.cwd)
                && result["covers"] == json!(check.covers)
                && result["source_snapshot_stable"] == true,
            "check result differs from its declared inputs."
        );
        let passed = result["passed"]
            .as_bool()
            .context("check result has no outcome")?;
        ensure!(
            !passed
                || result["exit_code"] == 0
                    && result["timed_out"] == false
                    && result["output_limit_exceeded"] == false
                    && result["error"].is_null()
                    && result["termination_error"].is_null(),
            "passing check has failure diagnostics"
        );
        accepted.push((name.to_owned(), passed));
    }
    ensure!(
        report["passed"] == accepted.iter().all(|(_, passed)| *passed),
        "aggregate check outcome differs from results."
    );
    Ok(accepted)
}
