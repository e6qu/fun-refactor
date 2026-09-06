use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub(crate) fn run(root: &Path, args: &[OsString], input: Option<&[u8]>) -> Result<Output> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|name| name.starts_with("GIT_")) {
            command.env_remove(name);
        }
    }
    command
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .args([
            "--no-pager",
            "--no-lazy-fetch",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.attributesFile=/dev/null",
        ])
        .args(args);
    if let Some(input) = input {
        let mut file = tempfile::tempfile()?;
        file.write_all(input)?;
        file.seek(SeekFrom::Start(0))?;
        command.stdin(Stdio::from(file));
    } else {
        command.stdin(Stdio::null());
    }
    command
        .output()
        .context("running Git; install Git to use this inspection.")
}

pub(crate) fn diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]).into_owned()
}

pub(crate) fn repository_root(root: &Path) -> Result<PathBuf> {
    let repository = run(root, &["rev-parse".into(), "--show-toplevel".into()], None)?;
    if !repository.status.success() {
        bail!(
            "directory must be inside a Git working tree. {}",
            diagnostic(&repository.stderr)
        );
    }
    let repository_path = std::str::from_utf8(&repository.stdout)?
        .strip_suffix('\n')
        .context("Git did not report a working tree root")?;
    Path::new(repository_path)
        .canonicalize()
        .context("resolving the Git working tree root")
}

pub(crate) fn require_no_filters(root: &Path, paths: &[u8]) -> Result<()> {
    let reserved = run(
        root,
        &[
            "config".into(),
            "--null".into(),
            "--get-regexp".into(),
            "^filter\\.(unspecified|unset)\\.(clean|process)$".into(),
        ],
        None,
    )?;
    if reserved.status.success() {
        bail!("Git filter drivers named unset or unspecified are unsupported for Git inspection.");
    }
    if reserved.status.code() != Some(1) {
        bail!(
            "checking Git filter configuration: {}",
            diagnostic(&reserved.stderr)
        );
    }
    let attributes = run(
        root,
        &[
            "check-attr".into(),
            "-z".into(),
            "--stdin".into(),
            "filter".into(),
        ],
        Some(paths),
    )?;
    if !attributes.status.success() {
        bail!(
            "checking Git content filters: {}",
            diagnostic(&attributes.stderr)
        );
    }
    let fields = attributes
        .stdout
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    if fields.len() != paths.iter().filter(|byte| **byte == 0).count() * 3 + 1
        || fields.last() != Some(&&b""[..])
    {
        bail!("Git returned an incomplete attribute report.");
    }
    for (path, row) in paths.split(|byte| *byte == 0).zip(fields.chunks_exact(3)) {
        if row[0] != path || row[1] != b"filter" {
            bail!("Git returned an unexpected attribute path.");
        }
        if !matches!(row[2], b"unspecified" | b"unset") {
            bail!(
                "Git content filters are unsupported for Git inspection: {:?}",
                String::from_utf8_lossy(path)
            );
        }
    }
    Ok(())
}
