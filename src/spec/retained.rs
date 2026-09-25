use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = "fr-proof-evidence-1";

#[derive(clap::Args)]
pub struct Options {
    #[arg(default_value = "specs")]
    pub package: PathBuf,
    #[arg(long, requires = "basis")]
    pub run: bool,
    #[arg(long, requires = "run")]
    pub basis: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub package: String,
    pub spec: String,
    pub theorem: String,
}

pub fn proof_acceptable(executed: bool, stable: bool, checked: bool, debt_free: bool) -> bool {
    executed && stable && checked && debt_free
}

fn confined(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        !relative.as_os_str().is_empty() && !relative.is_absolute(),
        "proof paths must be relative."
    );
    let mut path = root.to_path_buf();
    for part in relative.components() {
        match part {
            Component::Normal(part) => path.push(part),
            _ => anyhow::bail!("proof paths must be normalized within the workspace."),
        }
        ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "proof paths cannot traverse symlinks."
        );
    }
    Ok(path)
}

fn file_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= 2_147_483_648,
        "proof identity exceeds its file budget."
    );
    let mut digest = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n;
        ensure!(bytes <= 2_147_483_648, "proof identity exceeds 2 GiB.");
        digest.update(&buffer[..n]);
    }
    Ok(hex::encode(digest.finalize()))
}

const ENVIRONMENT: &[&str] = &[
    "PATH",
    "HOME",
    "ELAN_HOME",
    "TMPDIR",
    "LEAN_NUM_THREADS",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
];

fn command(program: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .current_dir(cwd)
        .env("ELAN_TOOLCHAIN", super::LEAN_TOOLCHAIN);
    for name in ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

fn toolchain(package: &Path) -> Result<Value> {
    let mut resolve = command(Path::new("lean"), package);
    resolve.arg("--print-prefix");
    let resolved = crate::checks::bounded_process(resolve, 30, 4096, false)?;
    ensure!(
        resolved["passed"] == true && resolved["stdout"]["omitted_bytes"] == 0,
        "cannot resolve the pinned Lean installation."
    );
    let prefix = PathBuf::from(
        resolved["stdout"]["text"]
            .as_str()
            .context("missing Lean installation")?
            .trim(),
    )
    .canonicalize()?;
    let mut programs = BTreeMap::new();
    for name in ["lean", "lake"] {
        let path = prefix.join("bin").join(name).canonicalize()?;
        programs.insert(name, json!({"path":path,"sha256":file_digest(&path)?}));
    }
    let mut version = command(&prefix.join("bin/lean"), package);
    version.arg("--version");
    let version = crate::checks::bounded_process(version, 30, 4096, false)?;
    let expected = super::LEAN_TOOLCHAIN.rsplit(":v").next().unwrap();
    ensure!(
        version["passed"] == true
            && version["stdout"]["omitted_bytes"] == 0
            && version["stdout"]["text"]
                .as_str()
                .is_some_and(|text| text.contains(&format!("version {expected},"))),
        "resolved Lean executable differs from the pinned version."
    );
    let mut environment = BTreeMap::new();
    for name in ENVIRONMENT {
        environment.insert(
            name,
            crate::project::hash(std::env::var_os(name).map(|s| s.as_encoded_bytes().to_vec()))?,
        );
    }
    Ok(
        json!({"pinned":super::LEAN_TOOLCHAIN,"programs":programs,"environment":environment,
        "trust":"Installed Lean libraries, elaborator, kernel and host execution remain trusted. Executable hashes do not attest execution."}),
    )
}

struct Snapshot {
    inputs: Value,
    files: BTreeMap<PathBuf, Vec<u8>>,
    modules: Vec<PathBuf>,
    package: PathBuf,
}

fn snapshot(root: &Path, package: &Path) -> Result<Snapshot> {
    let directory = confined(root, package)?;
    ensure!(directory.is_dir(), "proof package must be a directory.");
    let mut files = BTreeMap::new();
    let mut size = 0;
    for entry in ignore::WalkBuilder::new(&directory)
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some(".lake" | ".git")))
        .build()
    {
        let entry = entry?;
        ensure!(
            !entry.file_type().is_some_and(|t| t.is_symlink()),
            "proof packages cannot contain symlinks."
        );
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(&directory)?.to_path_buf();
        if relative == Path::new("lake-manifest.json") {
            let mut bytes = Vec::new();
            File::open(entry.path())?
                .take(65_537)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 65_536, "Lean manifest exceeds 64 KiB.");
            let manifest: Value = serde_json::from_slice(&bytes)?;
            ensure!(
                manifest["packages"] == json!([]),
                "retained proofs require a package without external dependencies."
            );
            continue;
        }
        ensure!(files.len() < 128, "proof package exceeds 128 files.");
        let mut bytes = Vec::new();
        File::open(entry.path())?
            .take(4_194_305)
            .read_to_end(&mut bytes)?;
        size += bytes.len();
        ensure!(size <= 4_194_304, "proof package exceeds 4 MiB.");
        files.insert(relative, bytes);
    }
    ensure!(
        files.get(Path::new("lakefile.toml")).map(Vec::as_slice)
            == Some(super::LAKEFILE.as_bytes()),
        "retained proofs require the lakefile generated by fr spec init."
    );
    ensure!(
        !files.contains_key(Path::new("lakefile.lean")),
        "executable Lake configuration is outside retained proof coverage."
    );
    ensure!(
        files
            .get(Path::new("lean-toolchain"))
            .is_some_and(|bytes| String::from_utf8_lossy(bytes).trim() == super::LEAN_TOOLCHAIN),
        "retained proofs require the pinned Lean toolchain."
    );
    let modules = files
        .keys()
        .filter(|p| p.extension().is_some_and(|e| e == "lean"))
        .cloned()
        .collect::<Vec<_>>();
    ensure!(!modules.is_empty(), "proof package has no Lean modules.");
    let identities = files
        .iter()
        .map(|(path, bytes)| {
            (
                path.to_string_lossy().to_string(),
                hex::encode(Sha256::digest(bytes)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let inputs = json!({"sources":crate::history::source_revision(root)?, "files":identities,
        "toolchain":toolchain(&directory)?, "analyzer":crate::project::hash((file_digest(&std::env::current_exe()?)?, include_str!("retained.rs"), include_str!("../spec.rs"),
            include_str!("../checks.rs"), include_str!("../extract.rs"), include_str!("../parse.rs"), env!("CARGO_PKG_VERSION")))?,
        "policy":"All recognized workspace source files and all local package files except generated .lake and an empty dependency manifest. Clean build and direct checking of every Lean module. External packages fall outside this contract."});
    Ok(Snapshot {
        inputs,
        files,
        modules,
        package: directory,
    })
}

pub(crate) fn input_digest(root: &Path, package: &str) -> Result<String> {
    crate::project::hash(snapshot(root, Path::new(package))?.inputs)
}

pub(crate) fn requirement_valid(requirement: &Requirement) -> bool {
    let normalized = |path: &str| {
        !path.is_empty()
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
    };
    normalized(&requirement.package)
        && normalized(&requirement.spec)
        && requirement.spec.ends_with(".lean")
        && !requirement.theorem.trim().is_empty()
        && requirement.theorem.len() <= 256
}

pub fn report(root: &Path, options: &Options) -> Result<Value> {
    let root = root.canonicalize()?;
    let before = snapshot(&root, &options.package)?;
    let basis = crate::project::hash(&before.inputs)?;
    let mut result = json!({"schema":SCHEMA,"root":root,"package":options.package,"input_digest":basis,
        "inputs":before.inputs,"executed":false,"stable":true,"passed":false,"evidence":null,"results":[],
        "modules":before.modules,"mutation_authority":false,"source_implementation_proved":false,
        "claim":"Model theorems under Lean definitions and assumptions. No proof of source implementation correspondence."});
    if !options.run {
        ensure!(
            serde_json::to_vec_pretty(&result)?.len() <= 1_048_576,
            "proof review exceeds 1 MiB."
        );
        return Ok(result);
    }
    ensure!(
        options.basis.as_ref() == Some(&basis),
        "reviewed proof inputs changed; review the package again."
    );
    let selected = before
        .modules
        .iter()
        .map(|path| before.package.join(path))
        .collect::<Vec<_>>();
    let checked = super::check_strict(&root, &selected, false)?;
    for anchor in &checked.anchors {
        confined(&root, &anchor.source)?;
    }
    let debt_free = checked.debts.is_empty() && checked.obligations == 0;
    let mut results = Vec::new();
    if checked.ok() && debt_free {
        let scratch = tempfile::Builder::new()
            .prefix("fr-retained-proof-")
            .tempdir()?;
        for (path, bytes) in &before.files {
            let path = scratch.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap())?;
            crate::vfs::write_bytes(path, bytes)?;
        }
        let tool = |name: &str| -> Result<&Path> {
            Ok(Path::new(
                before.inputs["toolchain"]["programs"][name]["path"]
                    .as_str()
                    .context("missing proof executable")?,
            ))
        };
        let mut build = command(tool("lake")?, scratch.path());
        build.args(["build", "--wfail"]);
        for module in &before.modules {
            build.arg(format!("./{}", module.display()));
        }
        let build = crate::checks::bounded_process(build, 120, 2048, true)?;
        let built = build["passed"] == true;
        results.push(json!({"module":null,"command":"lake build --wfail (all selected modules)","result":build}));
        if built {
            for module in &before.modules {
                let mut check = command(tool("lean")?, scratch.path());
                check
                    .arg("-DwarningAsError=true")
                    .arg(module)
                    .env("LEAN_PATH", scratch.path().join(".lake/build/lib/lean"));
                let checked = crate::checks::bounded_process(check, 30, 2048, true)?;
                results.push(json!({"module":module,"command":"lean -DwarningAsError=true","result":checked}));
            }
        }
    }
    let modules_checked = results.len() == before.modules.len() + 1
        && results.iter().all(|r| r["result"]["passed"] == true);
    let stable = snapshot(&root, &options.package).is_ok_and(|after| after.inputs == before.inputs);
    let passed = proof_acceptable(true, stable, checked.ok() && modules_checked, debt_free);
    let verification = super::Verification {
        report: checked,
        packages: vec![super::PackageReport {
            package: before.package,
            passed,
            output: "See retained per-module execution results.".into(),
        }],
    };
    let evidence = super::collect_evidence(&root, &selected, false, verification)?;
    result["executed"] = json!(true);
    result["stable"] = json!(stable);
    result["passed"] = json!(passed);
    result["results"] = json!(results);
    result["evidence"] = serde_json::to_value(evidence)?;
    ensure!(
        serde_json::to_vec_pretty(&result)?.len() <= 1_048_576,
        "retained proof report exceeds 1 MiB."
    );
    Ok(result)
}

pub(crate) fn validate(root: &Path, report: &Value) -> Result<Vec<(String, String, String)>> {
    ensure!(
        report["schema"] == SCHEMA && report["root"] == json!(root.canonicalize()?),
        "proof report belongs to another workspace or schema."
    );
    let package = report["package"]
        .as_str()
        .context("proof report has no package")?;
    let current = snapshot(root, Path::new(package))?;
    ensure!(
        report["inputs"] == current.inputs
            && report["input_digest"] == crate::project::hash(&current.inputs)?,
        "retained proof inputs changed."
    );
    ensure!(
        report["executed"] == true
            && report["stable"] == true
            && report["passed"] == true
            && report["source_implementation_proved"] == false
            && report["mutation_authority"] == false,
        "proof attachment needs executed, stable passing model evidence."
    );
    ensure!(
        report["modules"] == json!(current.modules),
        "proof module coverage differs from current inputs."
    );
    let results = report["results"]
        .as_array()
        .context("missing proof execution results")?;
    ensure!(
        results.len() == current.modules.len() + 1,
        "proof module results are incomplete."
    );
    for (index, row) in results.iter().enumerate() {
        ensure!(
            row["module"]
                == if index == 0 {
                    Value::Null
                } else {
                    json!(current.modules[index - 1])
                },
            "proof module results are reordered or incomplete."
        );
        let result = &row["result"];
        ensure!(
            result["passed"] == true
                && result["exit_code"] == 0
                && result["timed_out"] == false
                && result["output_limit_exceeded"] == false
                && result["error"].is_null()
                && result["termination_error"].is_null(),
            "proof execution contains failure diagnostics."
        );
    }
    let selected = current
        .modules
        .iter()
        .map(|path| current.package.join(path))
        .collect::<Vec<_>>();
    let checked = super::check_strict(root, &selected, false)?;
    ensure!(
        checked.ok() && checked.debts.is_empty() && checked.obligations == 0,
        "proof source anchors, signatures or debt changed."
    );
    let expected = super::collect_evidence(
        root,
        &selected,
        false,
        super::Verification {
            report: checked,
            packages: vec![super::PackageReport {
                package: current.package.clone(),
                passed: true,
                output: "See retained per-module execution results.".into(),
            }],
        },
    )?;
    ensure!(
        report["evidence"] == serde_json::to_value(&expected)?,
        "retained theorem evidence differs from current declarations."
    );
    let mut accepted = Vec::new();
    for property in expected.properties {
        ensure!(
            property.status == "checked_by_lean",
            "Lean did not check the proof property."
        );
        let identity = (
            package.to_owned(),
            property
                .spec
                .strip_prefix(&current.package)?
                .to_string_lossy()
                .into_owned(),
            property.name,
        );
        ensure!(
            !accepted.contains(&identity),
            "proof theorem identity is ambiguous within its module."
        );
        accepted.push(identity);
    }
    Ok(accepted)
}
