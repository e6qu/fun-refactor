use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CONFIG: &str = ".fr/checks.json";
const MAX_CONFIG: u64 = 65536;
const MAX_CAPTURE: u64 = 16 * 1024 * 1024;

#[derive(clap::Args)]
pub struct Options {
    #[arg(
        long,
        value_delimiter = ',',
        help = "Execute only these declared check names."
    )]
    pub run: Vec<String>,
    #[arg(
        long,
        requires = "run",
        help = "Configuration digest, or at least 32 leading hex characters, from the reviewed listing."
    )]
    pub basis: Option<String>,
    #[arg(
        long,
        default_value_t = 4096,
        help = "Retained bytes per output stream, at most 65536."
    )]
    pub output_bytes: usize,
    #[arg(
        long,
        requires = "run",
        help = "Omit successful stream text; retain bounded failure diagnostics."
    )]
    pub quiet_success: bool,
    #[arg(
        long,
        requires = "run",
        help = "Omit reviewed command declarations; retain names, basis and execution results."
    )]
    pub no_declarations: bool,
    #[arg(
        long,
        value_name = "TRANSACTION",
        requires = "run",
        help = "Record passing evidence against this applied source-history transaction."
    )]
    pub record_for: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    schema: u32,
    checks: Vec<Check>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Check {
    name: String,
    argv: Vec<String>,
    cwd: PathBuf,
    timeout_seconds: u64,
    covers: Vec<String>,
}

pub(crate) struct Selection {
    pub configuration_basis: String,
    pub checks: Vec<String>,
    pub coverage: Vec<Value>,
}

fn confined(root: &Path, relative: &Path, directory: bool) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => path.push(part),
            _ => bail!("Check paths must stay inside the project root."),
        }
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!(
                "Check paths cannot traverse symlinks: {}.",
                relative.display()
            );
        }
    }
    let metadata = fs::metadata(&path)?;
    if directory && !metadata.is_dir() || !directory && !metadata.is_file() {
        bail!("Invalid check path type: {}.", relative.display());
    }
    Ok(path)
}

fn configuration(root: &Path) -> Result<(Configuration, String)> {
    let path = confined(root, Path::new(CONFIG), false)
        .context("Declare project checks in .fr/checks.json before listing or running them.")?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_CONFIG + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CONFIG {
        bail!("Check configuration exceeds 65536 bytes.");
    }
    let config: Configuration = serde_json::from_slice(&bytes)?;
    if config.schema != 1 || config.checks.is_empty() || config.checks.len() > 32 {
        bail!("Check configuration needs schema 1 and between 1 and 32 checks.");
    }
    let mut names = BTreeSet::new();
    for check in &config.checks {
        if check.name.is_empty()
            || check.name.len() > 64
            || !check
                .name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            || !names.insert(&check.name)
        {
            bail!("Check names must be unique ASCII identifiers of at most 64 bytes.");
        }
        if check.argv.is_empty()
            || check.argv.len() > 128
            || check.argv[0].is_empty()
            || check
                .argv
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
            || !(1..=3600).contains(&check.timeout_seconds)
            || check.covers.is_empty()
            || check.covers.len() > 32
            || check
                .covers
                .iter()
                .any(|item| item.is_empty() || item.len() > 512 || item.contains('\0'))
        {
            bail!(
                "Invalid argv, timeout or coverage declaration for check {}.",
                check.name
            );
        }
        confined(root, &check.cwd, true)?;
    }
    Ok((config, format!("{:x}", Sha256::digest(&bytes))))
}

pub(crate) fn select(root: &Path, names: &[String]) -> Result<Option<Selection>> {
    if names.is_empty() {
        return Ok(None);
    }
    let (configuration, configuration_basis) = configuration(root)?;
    let selected = names.iter().collect::<BTreeSet<_>>();
    if selected.len() != names.len()
        || selected.iter().any(|name| {
            !configuration
                .checks
                .iter()
                .any(|check| &check.name == *name)
        })
    {
        bail!("Select existing check names once each; inspect fr checks first.");
    }
    let checks = configuration
        .checks
        .iter()
        .filter(|check| selected.contains(&check.name))
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();
    let coverage = configuration
        .checks
        .iter()
        .filter(|check| selected.contains(&check.name))
        .map(|check| json!({"name": check.name, "covers": check.covers}))
        .collect();
    Ok(Some(Selection {
        configuration_basis,
        checks,
        coverage,
    }))
}

fn output(file: &mut File, limit: usize) -> Result<Value> {
    let total = file.metadata()?.len();
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(limit as u64).read_to_end(&mut bytes)?;
    Ok(json!({
        "text": String::from_utf8_lossy(&bytes),
        "retained_bytes": bytes.len(), "omitted_bytes": total.saturating_sub(bytes.len() as u64),
        "encoding": "UTF-8 with replacement; byte counts precede decoding"
    }))
}

fn execute(root: &Path, check: &Check, limit: usize, quiet_success: bool) -> Result<Value> {
    let cwd = confined(root, &check.cwd, true)?;
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let started = Instant::now();
    let child = Command::new(&check.argv[0])
        .args(&check.argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn();
    let mut timed_out = false;
    let mut output_limit = false;
    let (status, error) = match child {
        Ok(mut child) => {
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => (),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(error);
                    }
                }
                timed_out = started.elapsed() >= Duration::from_secs(check.timeout_seconds);
                output_limit = stdout.metadata()?.len() > MAX_CAPTURE
                    || stderr.metadata()?.len() > MAX_CAPTURE;
                if timed_out || output_limit {
                    let _ = child.kill();
                    break child.wait();
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            match status {
                Ok(status) => (Some(status), None),
                Err(error) => (None, Some(error.to_string())),
            }
        }
        Err(error) => (None, Some(error.to_string())),
    };
    output_limit |=
        stdout.metadata()?.len() > MAX_CAPTURE || stderr.metadata()?.len() > MAX_CAPTURE;
    let passed = status.is_some_and(|s| s.success()) && !timed_out && !output_limit;
    let retained = if passed && quiet_success { 0 } else { limit };
    Ok(json!({
        "name": check.name, "argv": check.argv, "cwd": check.cwd, "covers": check.covers,
        "passed": passed,
        "exit_code": status.and_then(|s| s.code()), "error": error,
        "timed_out": timed_out, "output_limit_exceeded": output_limit,
        "elapsed_ms": started.elapsed().as_millis(),
        "stdout": output(&mut stdout, retained)?, "stderr": output(&mut stderr, retained)?
    }))
}

pub fn check_evidence_acceptable(
    executed: bool,
    commands_passed: bool,
    configuration_stable: bool,
    source_snapshot_stable: bool,
) -> bool {
    executed && commands_passed && configuration_stable && source_snapshot_stable
}

pub fn check_requirement_satisfied(
    requirement_present: bool,
    configuration_matches: bool,
    check_names_match: bool,
) -> bool {
    !requirement_present || configuration_matches && check_names_match
}

pub fn report(root: &Path, options: &Options) -> Result<Value> {
    if options.no_declarations && options.run.is_empty() {
        bail!("Declaration omission requires selected checks and a reviewed configuration basis.");
    }
    if options.output_bytes > 65536 {
        bail!("Check output budget must be between 0 and 65536 bytes.");
    }
    let root = root.canonicalize()?;
    let (config, basis) = configuration(&root)?;
    let selected: BTreeSet<_> = options.run.iter().collect();
    if selected.len() != options.run.len()
        || selected
            .iter()
            .any(|name| !config.checks.iter().any(|check| &check.name == *name))
    {
        bail!("Select existing check names once each; inspect fr checks first.");
    }
    let reviewed = options.basis.as_deref().is_some_and(|supplied| {
        supplied == basis
            || (supplied.len() >= 32 && supplied.len() < basis.len() && basis.starts_with(supplied))
    });
    if !selected.is_empty() && !reviewed {
        bail!("Check configuration basis is missing or stale; inspect fr checks before execution.");
    }
    let selected_names = config
        .checks
        .iter()
        .filter(|check| selected.contains(&check.name))
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();
    let source_revision = (!selected.is_empty())
        .then(|| crate::history::source_revision(&root))
        .transpose()?;
    if let (Some(transaction), Some(revision)) = (options.record_for, source_revision.as_deref()) {
        crate::history::check_evidence_target(
            &root,
            transaction,
            revision,
            &basis,
            &selected_names,
        )?;
    }
    let mut results = Vec::new();
    for check in &config.checks {
        if selected.contains(&check.name) {
            let mut result = execute(&root, check, options.output_bytes, options.quiet_success)?;
            let after = crate::history::source_revision(&root)?;
            let stable = source_revision.as_ref() == Some(&after);
            if !stable {
                result["source_revision_after"] = json!(after);
            }
            result["source_snapshot_stable"] = json!(stable);
            results.push(result);
        }
    }
    let commands_passed = results.iter().all(|result| result["passed"] == true);
    let configuration_stable = configuration(&root)?.1 == basis;
    let source_snapshot_stable = results
        .iter()
        .all(|result| result["source_snapshot_stable"] == true);
    let passed = (!results.is_empty()).then(|| {
        check_evidence_acceptable(
            true,
            commands_passed,
            configuration_stable,
            source_snapshot_stable,
        )
    });
    let mut recorded_evidence = Value::Null;
    if let (Some(transaction), Some(source_revision), Some(true)) =
        (options.record_for, source_revision.as_ref(), passed)
    {
        let names = selected_names.clone();
        let receipt = crate::history::check_evidence_receipt(&basis, source_revision, &names)?;
        let evidence = crate::history::CheckEvidence {
            receipt: receipt.clone(),
            configuration_basis: basis.clone(),
            source_revision: source_revision.clone(),
            checks: names,
        };
        let created = crate::history::record_check_evidence(&root, transaction, evidence)?;
        recorded_evidence = json!({
            "transaction": transaction,
            "receipt": receipt,
            "created": created,
        });
    }
    let mut report = json!({
        "schema": "fr-checks-1", "root": root, "configuration": CONFIG, "basis": basis,
        "executed": !results.is_empty(), "passed": passed,
        "checks": config.checks, "results": results,
        "not_run": config.checks.iter().filter(|check| !selected.contains(&check.name)).map(|check| &check.name).collect::<Vec<_>>(),
        "coverage_authority": "project declarations; passing commands do not prove coverage or equivalence",
        "execution": "inherited environment; direct argv; no command sandbox; direct-child timeout; temporary file capture",
        "source_snapshot_checked": !results.is_empty()
    });
    if let Some(source_revision) = source_revision {
        report["configuration_stable"] = json!(configuration_stable);
        report["source_revision"] = json!(source_revision);
        report["source_snapshot_stable"] = json!(source_snapshot_stable);
    }
    if !recorded_evidence.is_null() {
        report["recorded_evidence"] = recorded_evidence;
    }
    if options.no_declarations {
        report.as_object_mut().unwrap().remove("checks");
        for result in report["results"].as_array_mut().unwrap() {
            for key in ["argv", "cwd", "covers"] {
                result.as_object_mut().unwrap().remove(key);
            }
        }
        report["declarations_omitted"] = json!(true);
    }
    Ok(report)
}
