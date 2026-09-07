#[cfg(unix)]
mod host {
    use super::super::journal::{require_plain_entries, Journal};
    use super::super::{process, require_not_ignored, status, working_file, Entry};
    use crate::history::Action;
    use anyhow::{ensure, Context, Result};
    use serde_json::{json, Value};
    use std::collections::BTreeSet;
    use std::fs::{self, File, OpenOptions, Permissions};
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};

    pub(crate) struct IndexLock {
        index: PathBuf,
        path: PathBuf,
        file: File,
        before: Option<Vec<u8>>,
        permissions: Option<Permissions>,
        installed: bool,
    }

    fn read_index(path: &Path) -> Result<Option<Vec<u8>>> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_file(),
                    "staging requires a regular Git index."
                );
                Ok(Some(fs::read(path)?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn index_path(root: &Path) -> Result<PathBuf> {
        let output = process::checked(
            root,
            &[
                "rev-parse".into(),
                "--path-format=absolute".into(),
                "--git-path".into(),
                "index".into(),
            ],
        )?;
        let path = PathBuf::from(
            std::str::from_utf8(&output)?
                .strip_suffix('\n')
                .context("missing Git index path")?,
        );
        ensure!(path.is_absolute(), "Git index path must be absolute.");
        Ok(path
            .parent()
            .context("missing Git index directory")?
            .canonicalize()?
            .join("index"))
    }

    fn require_supported(root: &Path) -> Result<()> {
        let shared = process::checked(root, &["rev-parse".into(), "--shared-index-path".into()])?;
        ensure!(
            shared == b"\n" || shared.is_empty(),
            "split indexes are unsupported for staging writes."
        );
        let sparse = process::run(
            root,
            &[
                "config".into(),
                "--bool".into(),
                "--get".into(),
                "core.sparseCheckout".into(),
            ],
            None,
        )?;
        ensure!(
            sparse.status.code() == Some(1)
                || (sparse.status.success() && sparse.stdout == b"false\n"),
            "sparse checkouts are unsupported for staging writes."
        );
        let rows = process::checked(
            root,
            &[
                "ls-files".into(),
                "--sparse".into(),
                "--stage".into(),
                "-z".into(),
            ],
        )?;
        ensure!(
            !status::records(&rows)?
                .iter()
                .any(|row| row.starts_with(b"040000 ")),
            "sparse indexes are unsupported for staging writes."
        );
        Ok(())
    }

    fn prepared(root: &Path, index: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
        let mut options = vec![
            "-c".into(),
            "core.splitIndex=false".into(),
            "-c".into(),
            "core.ignorestat=false".into(),
        ];
        options.extend(args.iter().map(Into::into));
        let output = process::run_with_index(root, &options, input, Some(index))?;
        ensure!(
            output.status.success(),
            "preparing staging index failed: {}",
            process::diagnostic(&output.stderr)
        );
        Ok(output.stdout)
    }

    fn unrelated(rows: &[u8], entries: &[Entry]) -> Result<Vec<Vec<u8>>> {
        status::records(rows)?
            .into_iter()
            .filter_map(|row| {
                let Some(tab) = row.iter().position(|byte| *byte == b'\t') else {
                    return Some(Err(anyhow::anyhow!("invalid prepared index inventory.")));
                };
                (!entries
                    .iter()
                    .any(|entry| entry.path.as_bytes() == &row[tab + 1..]))
                .then(|| Ok(row.to_vec()))
            })
            .collect()
    }

    impl IndexLock {
        pub(crate) fn acquire(root: &Path) -> Result<Self> {
            let index = index_path(root)?;
            let path = index.with_extension("lock");
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .context("cannot acquire Git index.lock; leave existing locks to their owner")?;
            let mut lock = Self {
                index,
                path,
                file,
                before: None,
                permissions: None,
                installed: false,
            };
            lock.before = read_index(&lock.index)?;
            lock.permissions = fs::metadata(&lock.index)
                .ok()
                .map(|metadata| metadata.permissions());
            require_supported(root)?;
            lock.check(root)?;
            Ok(lock)
        }

        fn owns_path(&self) -> bool {
            let Ok(open) = self.file.metadata() else {
                return false;
            };
            let Ok(path) = fs::symlink_metadata(&self.path) else {
                return false;
            };
            path.file_type().is_file() && open.dev() == path.dev() && open.ino() == path.ino()
        }

        pub(crate) fn check(&self, root: &Path) -> Result<()> {
            ensure!(
                self.owns_path(),
                "Git index lock ownership changed; staging refused."
            );
            ensure!(
                index_path(root)? == self.index && read_index(&self.index)? == self.before,
                "Git index changed during staging write; request a new preview."
            );
            Ok(())
        }

        pub(in crate::git::stage) fn apply(
            self,
            root: &Path,
            entries: &[Entry],
            filter_paths: &[u8],
            untracked: &BTreeSet<String>,
        ) -> Result<Value> {
            self.check(root)?;
            let mut journal = Journal::read(root, &self.index)?;
            journal.ready()?;
            if entries.iter().all(|entry| entry.action == "unchanged") {
                return Ok(json!({"index_replaced":false,"directory_synced":null}));
            }
            let id = journal.plan(root, entries)?;
            self.install(root, entries, &mut journal, (Action::Apply, id), || {
                process::require_no_filters(root, filter_paths)?;
                require_not_ignored(root, untracked)?;
                for entry in entries {
                    ensure!(working_file(root, &entry.path)?.map(|(blob, _)| blob) == entry.after,
                        "selected working files changed during staging write; request a new preview.");
                }
                Ok(())
            })
        }

        pub(in crate::git::stage) fn replay(
            self,
            root: &Path,
            entries: &[Entry],
            journal: &mut Journal,
            action: Action,
            id: u64,
        ) -> Result<Value> {
            self.install(root, entries, journal, (action, id), || Ok(()))
        }

        fn install(
            mut self,
            root: &Path,
            entries: &[Entry],
            journal: &mut Journal,
            transition: (Action, u64),
            validate: impl Fn() -> Result<()>,
        ) -> Result<Value> {
            let (action, id) = transition;
            self.check(root)?;
            journal.check()?;
            if entries.iter().all(|entry| entry.action == "unchanged") {
                if self.before.is_some() {
                    File::open(&self.index)?.sync_all()?;
                }
                File::open(self.index.parent().context("missing Git index directory")?)?
                    .sync_all()?;
                self.check(root)?;
                return Ok(
                    json!({"index_replaced":false,"directory_synced":true,"journal":journal.finish(action, id)}),
                );
            }
            let parent = self.index.parent().context("missing Git index directory")?;
            let dir = tempfile::Builder::new()
                .prefix("fr-stage-")
                .tempdir_in(parent)?;
            let alternate = dir.path().join("index");
            if let Some(bytes) = &self.before {
                fs::write(&alternate, bytes)?;
            }
            let original = prepared(root, &alternate, &["ls-files", "--stage", "-v", "-z"], None)?;
            let mut input = Vec::new();
            for entry in entries.iter().filter(|entry| entry.action != "unchanged") {
                if let Some(blob) = &entry.after {
                    let source = entry
                        .source
                        .as_ref()
                        .context("missing captured staging source")?;
                    let oid = prepared(
                        root,
                        &alternate,
                        &["hash-object", "-w", "--no-filters", "--stdin"],
                        Some(source),
                    )?;
                    ensure!(
                        oid == format!("{}\n", blob.oid).as_bytes(),
                        "written staging object differs from reviewed identity."
                    );
                    input.extend_from_slice(
                        format!("{} {}\t{}\0", blob.mode, blob.oid, entry.path).as_bytes(),
                    );
                } else {
                    let blob = entry
                        .before
                        .as_ref()
                        .context("missing removed staging entry")?;
                    input.extend_from_slice(format!("0 {}\t{}\0", blob.oid, entry.path).as_bytes());
                }
            }
            prepared(
                root,
                &alternate,
                &["update-index", "-z", "--index-info"],
                Some(&input),
            )?;
            let after = prepared(root, &alternate, &["ls-files", "--stage", "-v", "-z"], None)?;
            ensure!(
                unrelated(&original, entries)? == unrelated(&after, entries)?,
                "prepared index changed unrelated entries or flags."
            );
            let mut selected = vec!["--literal-pathspecs", "ls-files", "--stage", "-z", "--"];
            selected.extend(entries.iter().map(|entry| entry.path.as_str()));
            let observed = prepared(root, &alternate, &selected, None)?;
            let expected = entries
                .iter()
                .filter_map(|entry| {
                    entry
                        .after
                        .as_ref()
                        .map(|blob| format!("{} {} 0\t{}\0", blob.mode, blob.oid, entry.path))
                })
                .collect::<String>();
            ensure!(
                observed == expected.as_bytes(),
                "prepared index differs from reviewed entries."
            );
            let bytes = read_index(&alternate)?.context("missing prepared index")?;
            self.file.write_all(&bytes)?;
            if let Some(permissions) = &self.permissions {
                self.file.set_permissions(permissions.clone())?;
            }
            self.file.sync_all()?;
            self.check(root)?;
            require_supported(root)?;
            validate()?;
            let changed = entries
                .iter()
                .filter(|entry| entry.action != "unchanged")
                .map(|entry| entry.path.clone())
                .collect();
            require_plain_entries(root, &changed)?;
            self.check(root)?;
            ensure!(
                fs::read(&self.path)? == bytes,
                "prepared index lock content changed; staging refused."
            );
            journal.begin(action, id)?;
            self.check(root)?;
            fs::rename(&self.path, &self.index)
                .context("installing prepared Git index; inspect staging recovery")?;
            self.installed = true;
            let mut result = match File::open(parent).and_then(|directory| directory.sync_all()) {
                Ok(()) => json!({"index_replaced":true,"directory_synced":true}),
                Err(error) => {
                    json!({"index_replaced":true,"directory_synced":false,"warning":error.to_string()})
                }
            };
            result["journal"] = if result["directory_synced"] == true {
                journal.finish(action, id)
            } else {
                json!({"id":id,"finalized":false,"warning":"index installed without confirmed directory sync; inspect staging recovery"})
            };
            Ok(result)
        }
    }

    impl Drop for IndexLock {
        fn drop(&mut self) {
            if !self.installed && self.owns_path() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(unix)]
pub(in crate::git) use host::{index_path, IndexLock};

#[cfg(not(unix))]
pub(in crate::git) struct IndexLock;

#[cfg(not(unix))]
impl IndexLock {
    pub(super) fn acquire(_: &std::path::Path) -> anyhow::Result<Self> {
        anyhow::bail!("staging writes require Unix index lock ownership checks.")
    }
    pub(super) fn apply(
        self,
        _: &std::path::Path,
        _: &[super::Entry],
        _: &[u8],
        _: &std::collections::BTreeSet<String>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("staging writes require Unix index lock ownership checks.")
    }
}

#[cfg(not(unix))]
pub(in crate::git) fn index_path(_: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    anyhow::bail!("staging history requires Unix index lock ownership checks.")
}

#[cfg(not(unix))]
impl IndexLock {
    pub(super) fn replay(
        self,
        _: &std::path::Path,
        _: &[super::Entry],
        _: &mut super::journal::Journal,
        _: crate::history::Action,
        _: u64,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("staging history requires Unix index lock ownership checks.")
    }
}
