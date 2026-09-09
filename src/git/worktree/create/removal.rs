use super::{absent, directory, ownership, recovery};
use crate::git::worktree::RemoveOptions;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::branch;
pub(in crate::git::worktree) mod compact;
pub(in crate::git::worktree) mod resume;

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FileState {
    identity: (u64, u64),
    mode: u32,
    digest: String,
    #[serde(skip)]
    bytes: Vec<u8>,
}

impl FileState {
    fn read(path: &Path) -> Result<Self> {
        Self::read_limited(path, 64 * 1024 * 1024)
    }

    fn read_limited(path: &Path, limit: u64) -> Result<Self> {
        let before = fs::symlink_metadata(path)?;
        let bytes = ownership::bytes(path, limit)?;
        let after = fs::symlink_metadata(path)?;
        ensure!(
            before.dev() == after.dev()
                && before.ino() == after.ino()
                && before.mode() == after.mode(),
            "removal file changed during inspection."
        );
        Ok(Self {
            identity: (after.dev(), after.ino()),
            mode: after.mode(),
            digest: ownership::digest(&bytes),
            bytes,
        })
    }

    fn remove(&self, path: &Path) -> Result<()> {
        self.remove_limited(path, 64 * 1024 * 1024)
    }

    fn remove_limited(&self, path: &Path, limit: u64) -> Result<()> {
        let current = Self::read_limited(path, limit)?;
        ensure!(
            crate::git::worktree_removal_file_allowed(
                self.identity == current.identity,
                self.bytes == current.bytes,
                self.mode == current.mode
            ),
            "reviewed removal file changed: {path:?}."
        );
        fs::remove_file(path)?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    checkout: recovery::Observation,
    metadata: BTreeMap<String, FileState>,
    directories: BTreeMap<String, (u64, u64)>,
    gitfile: FileState,
}

fn configuration(root: &Path, expected_worktree_config: bool) -> Result<()> {
    let output = crate::git::process::run(
        root,
        &super::args(&["config", "--local", "--get", "extensions.refStorage"]),
        None,
    )?;
    ensure!(
        output.status.code() == Some(1)
            || (output.status.success() && super::line(&output.stdout)? == "files"),
        "unsupported removal configuration: extensions.refStorage."
    );
    ensure!(
        crate::git::worktree_configuration_allowed(
            expected_worktree_config,
            super::worktree_config(root)?,
            false,
            false
        ),
        "extensions.worktreeConfig changed after reviewed creation."
    );
    Ok(())
}

fn observe(capture: &recovery::Capture, leases: &[ownership::Lease]) -> Result<Snapshot> {
    let checkout = recovery::observe(capture)?;
    ensure!(
        checkout.missing.is_empty() && checkout.index.is_some(),
        "removal requires a complete clean checkout and index."
    );
    configuration(&capture.plan.root, capture.plan.worktree_config)?;
    branch::check(
        &capture.plan.root,
        &capture.plan.branch,
        &capture.plan.commit,
    )?;
    for lease in leases {
        lease.check()?;
    }
    let metadata = capture.receipt.path().parent().unwrap().to_owned();
    let mut files = BTreeMap::new();
    let mut metadata_bytes = 0usize;
    let mut directories = BTreeMap::new();
    let mut pending = vec![String::new()];
    while let Some(relative) = pending.pop() {
        directories.insert(relative.clone(), directory(&metadata.join(&relative))?);
        for entry in fs::read_dir(metadata.join(&relative))? {
            let entry = entry?;
            let path = Path::new(&relative).join(entry.file_name());
            let name = path.to_str().context("removal metadata must use UTF-8.")?;
            if !leases.is_empty() && ["fr-creation.lock", "index.lock", "HEAD.lock"].contains(&name)
            {
                continue;
            }
            if name == "logs" || name == "refs" {
                directory(&entry.path())?;
                pending.push(name.to_owned());
            } else {
                ensure!(
                    [
                        "HEAD",
                        "index",
                        "commondir",
                        "gitdir",
                        "locked",
                        "fr-creation.json",
                        "config.worktree",
                        "logs/HEAD",
                        "COMMIT_EDITMSG",
                        "ORIG_HEAD"
                    ]
                    .contains(&name),
                    "unreviewable private worktree metadata: {name:?}."
                );
                let state = FileState::read(&entry.path())?;
                metadata_bytes += state.bytes.len();
                ensure!(
                    metadata_bytes <= 16 * 1024 * 1024,
                    "private removal metadata exceeds 16 MiB."
                );
                files.insert(name.to_owned(), state);
            }
        }
    }
    for required in [
        "HEAD",
        "index",
        "commondir",
        "gitdir",
        "locked",
        "fr-creation.json",
    ] {
        ensure!(
            files.contains_key(required),
            "missing private worktree metadata: {required}."
        );
    }
    ensure!(
        files["index"].bytes == *checkout.index.as_ref().unwrap(),
        "index changed during removal inspection."
    );
    ensure!(
        files["fr-creation.json"].bytes == capture.receipt_bytes,
        "ownership receipt changed."
    );
    let gitfile = FileState::read(&capture.plan.destination.join(".git"))?;
    capture.receipt.check()?;
    Ok(Snapshot {
        checkout,
        metadata: files,
        directories,
        gitfile,
    })
}

fn basis(capture: &recovery::Capture, snapshot: &Snapshot) -> Result<String> {
    Ok(format!(
        "frwtd1:{}",
        ownership::digest(&serde_json::to_vec(&(
            &capture.plan,
            &capture.receipt_bytes,
            snapshot
        ))?)
    ))
}

fn archive(capture: &recovery::Capture, snapshot: &Snapshot) -> Result<PathBuf> {
    let parent = &capture.plan.common;
    ensure!(
        directory(parent)? == capture.plan.common_identity,
        "common directory changed before archiving."
    );
    let temporary = tempfile::Builder::new()
        .prefix("fr-worktree-removal-")
        .tempdir_in(parent)?;
    let path = temporary.path().join("record.json");
    let metadata = snapshot
        .metadata
        .iter()
        .map(|(name, state)| (name, &state.bytes))
        .collect::<BTreeMap<_, _>>();
    let record = json!({"schema":1,"state":"prepared","proposal":capture.plan,"receipt":capture.receipt,
        "snapshot":snapshot,"metadata_bytes":metadata,"gitfile_bytes":snapshot.gitfile.bytes});
    let mut file = tempfile::NamedTempFile::new_in(temporary.path())?;
    file.write_all(&serde_json::to_vec(&record)?)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&path)?;
    File::open(temporary.path())?.sync_all()?;
    let retained = temporary.keep();
    File::open(parent)?.sync_all()?;
    Ok(retained.join("record.json"))
}

fn remove_directories(root: &Path, directories: &BTreeMap<String, (u64, u64)>) -> Result<()> {
    let mut entries = directories.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(path, _)| std::cmp::Reverse(Path::new(path).components().count()));
    for (relative, identity) in entries {
        let path = root.join(relative);
        ensure!(
            directory(&path)? == *identity,
            "removal directory identity changed: {path:?}."
        );
        fs::remove_dir(path)
            .context("removal stopped at a non-empty or inaccessible directory.")?;
    }
    Ok(())
}

fn apply(
    capture: &recovery::Capture,
    snapshot: &Snapshot,
    leases: Vec<ownership::Lease>,
) -> Result<()> {
    let root = &capture.plan.destination;
    let metadata = capture.receipt.path().parent().unwrap().to_owned();
    let check = |relative: &str| -> Result<()> {
        ensure!(
            directory(root.parent().unwrap())? == capture.plan.parent_identity,
            "worktree parent changed before unlinking."
        );
        ensure!(
            directory(root)? == capture.receipt.destination_identity
                && directory(&capture.plan.common)? == capture.plan.common_identity,
            "worktree removal ownership changed."
        );
        for parent in Path::new(relative).ancestors().skip(1) {
            let identity = snapshot
                .checkout
                .directories
                .get(parent.to_str().unwrap())
                .context("unreviewed removal parent.")?;
            ensure!(
                directory(&root.join(parent))? == *identity,
                "worktree directory changed before unlinking."
            );
        }
        for (relative, identity) in &snapshot.directories {
            ensure!(
                directory(&metadata.join(relative))? == *identity,
                "private worktree directory changed before unlinking."
            );
        }
        for lease in &leases {
            lease.check()?;
        }
        Ok(())
    };
    for (entry, bytes) in capture.plan.files.iter().zip(&capture.blobs) {
        check(&entry.path)?;
        let (dev, ino, mode, digest) = &snapshot.checkout.files[&entry.path];
        let state = FileState {
            identity: (*dev, *ino),
            mode: *mode,
            digest: digest.clone(),
            bytes: bytes.clone(),
        };
        state.remove(&root.join(&entry.path))?;
    }
    check(".git")?;
    let mut subdirectories = snapshot.checkout.directories.clone();
    subdirectories.remove("");
    remove_directories(root, &subdirectories)?;
    for entry in fs::read_dir(root)? {
        ensure!(
            entry?.file_name() == ".git",
            "removal stopped because the worktree is no longer empty."
        );
    }
    snapshot.gitfile.remove(&root.join(".git"))?;
    ensure!(
        directory(root)? == capture.receipt.destination_identity,
        "worktree directory changed before removal."
    );
    fs::remove_dir(root).context("removal stopped because the worktree is no longer empty.")?;
    File::open(root.parent().unwrap())?.sync_all()?;
    for (name, state) in &snapshot.metadata {
        for (relative, identity) in &snapshot.directories {
            ensure!(
                directory(&metadata.join(relative))? == *identity,
                "private directory changed during removal."
            );
        }
        for lease in &leases {
            lease.check()?;
        }
        state.remove(&metadata.join(name))?;
    }
    drop(leases);
    remove_directories(&metadata, &snapshot.directories)?;
    File::open(metadata.parent().unwrap())?.sync_all()?;
    Ok(())
}

pub(in crate::git::worktree) fn report(root: &Path, options: &RemoveOptions) -> Result<Value> {
    ensure!(
        (1..=500).contains(&options.limit),
        "limit must be between 1 and 500."
    );
    let capture = recovery::capture(root, &options.path, true)?;
    let snapshot = observe(&capture, &[])?;
    let token = basis(&capture, &snapshot)?;
    if let Some(expected) = &options.basis {
        ensure!(
            *expected == token,
            "stale worktree removal basis; request a new preview."
        );
    }
    let mut result = json!({"schema":1,"operation":if options.write {"worktree-remove"} else {"worktree-remove-preview"},
        "applied":false,"basis":token,"basis_verified":options.basis.is_some(),"repository_root":capture.plan.root,
        "destination":capture.plan.destination,"branch":format!("refs/heads/{}",capture.plan.branch),
        "commit":capture.plan.commit,"tree":capture.plan.tree,"branch_action":"retain","atomic_snapshot":false,
        "worktree_config":capture.plan.worktree_config,"worktree_config_file":snapshot.checkout.worktree_config.is_some(),
        "files":capture.plan.files.iter().take(options.limit).collect::<Vec<_>>(),
        "page":{"total":capture.plan.files.len(),"returned":capture.plan.files.len().min(options.limit),
            "omitted":capture.plan.files.len().saturating_sub(options.limit)}});
    if !options.write {
        return Ok(result);
    }
    let metadata = capture.receipt.path().parent().unwrap().to_owned();
    let mut leases = Vec::new();
    for name in ["fr-creation.lock", "index.lock", "HEAD.lock"] {
        leases.push(ownership::Lease::acquire(metadata.join(name))?);
    }
    let _branch = branch::Lease::acquire(
        &capture.plan.root,
        &capture.plan.branch,
        &capture.plan.commit,
    )?;
    ensure!(
        basis(&capture, &observe(&capture, &leases)?)? == token,
        "worktree removal basis changed before writing."
    );
    let record = archive(&capture, &snapshot)?;
    result["removal_record"] = json!(record);
    let _archive_lease = ownership::Lease::acquire(record.with_file_name("resume.lock"))?;
    let outcome = (|| -> Result<()> {
        ensure!(
            basis(&capture, &observe(&capture, &leases)?)? == token,
            "worktree removal changed after archiving."
        );
        apply(&capture, &snapshot, leases)?;
        ensure!(
            absent(&capture.plan.destination)? && absent(&metadata)?,
            "removed worktree paths reappeared."
        );
        let mut complete = tempfile::NamedTempFile::new_in(record.parent().unwrap())?;
        complete
            .write_all(b"Removal completed. The recorded branch and commit were retained.\n")?;
        complete.as_file().sync_all()?;
        complete.persist_noclobber(record.with_file_name("complete"))?;
        File::open(record.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    match outcome {
        Ok(()) => result["applied"] = json!(true),
        Err(error) => {
            result["applied"] = Value::Null;
            result["warning"] = json!("Removal is incomplete or unconfirmed. Inspect with worktree resume-removal before retrying; do not retry creation recovery.");
            let diagnostic = format!("{error:#}");
            result["diagnostic"] = json!(crate::git::process::diagnostic(diagnostic.as_bytes()));
            result["diagnostic_truncated"] = json!(diagnostic.len() > 16 * 1024);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selective_cleanup_preserves_late_content_and_replaced_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        fs::create_dir(&root).unwrap();
        let directories = BTreeMap::from([(String::new(), directory(&root).unwrap())]);
        let file = root.join("reviewed");
        fs::write(&file, b"committed").unwrap();
        let state = FileState::read(&file).unwrap();
        fs::rename(&file, root.join("original")).unwrap();
        fs::write(&file, b"committed").unwrap();
        assert!(state.remove(&file).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"committed");
        let state = FileState::read(&file).unwrap();
        fs::write(&file, b"changed").unwrap();
        assert!(state.remove(&file).is_err());
        fs::write(root.join("late-ignored"), b"preserve").unwrap();
        FileState::read(&file).unwrap().remove(&file).unwrap();
        assert!(remove_directories(&root, &directories).is_err());
        assert_eq!(fs::read(root.join("late-ignored")).unwrap(), b"preserve");
        assert_eq!(fs::read(root.join("original")).unwrap(), b"committed");
    }
}
