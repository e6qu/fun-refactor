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
        programs
            .push(json!({"check":check.name,"path":path,"sha256":hex::encode(hash.finalize())}));
    }
    Ok(json!({"schema":"fr-check-toolchain-1","programs":programs,
        "executor":crate::project::hash((include_str!("../checks.rs"), include_str!("evidence.rs")))?,
        "scope":"command executable bytes; environment, dynamic libraries and interpreter imports require declared identity checks."}))
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
