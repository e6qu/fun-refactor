use super::process::{self, checked};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Blob {
    pub mode: String,
    pub oid: String,
}

pub(super) type Inventory = BTreeMap<String, Blob>;

pub(super) fn inventory(
    root: &Path,
    paths: &BTreeSet<String>,
    commit: Option<&str>,
) -> Result<Inventory> {
    let mut args: Vec<OsString> = vec!["--literal-pathspecs".into()];
    if let Some(commit) = commit {
        args.extend(["ls-tree".into(), "-z".into(), commit.into()]);
    } else {
        args.extend(["ls-files".into(), "--stage".into(), "-z".into()]);
    }
    args.push("--".into());
    args.extend(paths.iter().map(Into::into));
    let output = checked(root, &args)?;
    let mut result = Inventory::new();
    for row in crate::git::status::records(&output)? {
        let row = std::str::from_utf8(row).context("non-UTF-8 Git snapshot inventory")?;
        let (metadata, path) = row
            .split_once('\t')
            .context("invalid Git snapshot inventory")?;
        ensure!(
            paths.contains(path),
            "Git snapshot requires explicit file paths; directories are unsupported."
        );
        let fields = metadata.split(' ').collect::<Vec<_>>();
        ensure!(fields.len() == 3, "invalid Git snapshot inventory fields.");
        let oid = if commit.is_some() {
            ensure!(
                fields[1] == "blob",
                "Git snapshot requires regular file blobs."
            );
            fields[2]
        } else {
            ensure!(
                fields[2] == "0",
                "unmerged Git snapshot requires conflict inspection."
            );
            fields[1]
        };
        ensure!(
            matches!(fields[0], "100644" | "100755") && process::oid(oid),
            "Git snapshot requires regular file blobs."
        );
        ensure!(
            result
                .insert(
                    path.to_owned(),
                    Blob {
                        mode: fields[0].to_owned(),
                        oid: oid.to_owned()
                    }
                )
                .is_none(),
            "duplicate Git snapshot inventory path."
        );
    }
    Ok(result)
}

pub(super) fn working_file(root: &Path, path: &str) -> Result<Option<(Blob, String)>> {
    let mut selected = root.to_path_buf();
    for component in Path::new(path).components() {
        selected.push(component);
        let metadata = match std::fs::symlink_metadata(&selected) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            !metadata.file_type().is_symlink(),
            "working Git snapshot traverses a symlink."
        );
    }
    let metadata = std::fs::symlink_metadata(&selected)?;
    ensure!(
        metadata.is_file(),
        "working Git snapshot requires a regular file."
    );
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        format!(
            "{:o}",
            crate::history::git_mode(metadata.permissions().mode())
        )
    };
    #[cfg(not(unix))]
    let mode = "100644".to_owned();
    let text = crate::vfs::read_to_string(&selected).context("reading working Git snapshot")?;
    let oid = blob_oid(root, &text)?;
    Ok(Some((Blob { mode, oid }, text)))
}

pub(super) fn blob_oid(root: &Path, text: &str) -> Result<String> {
    let output = crate::git::process::run(
        root,
        &[
            "hash-object".into(),
            "--no-filters".into(),
            "--stdin".into(),
        ],
        Some(text.as_bytes()),
    )?;
    if !output.status.success() {
        bail!("cannot hash captured source bytes");
    }
    let oid = std::str::from_utf8(&output.stdout)?
        .strip_suffix('\n')
        .context("invalid captured source hash")?;
    if !process::oid(oid) {
        bail!("invalid captured source hash");
    }
    Ok(oid.to_owned())
}
