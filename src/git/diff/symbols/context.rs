use super::{language, patch, source};
use crate::analysis::call_graph::Family;
use crate::capabilities::{support, Capability};
use crate::extract::Extractor;
use crate::git::snapshot::{inventory, working_file, Inventory};
use crate::lang::Language;
use crate::model::FileFacts;
use crate::parse::Parsers;
use anyhow::{bail, ensure, Context as _, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

pub(super) struct Snapshot {
    pub path: PathBuf,
    pub language: Language,
    pub source: String,
    pub facts: FileFacts,
}

pub(in crate::git::diff) struct Context {
    pub(super) sides: [Vec<Snapshot>; 2],
    pub(super) coverage: Value,
    paths: BTreeSet<String>,
    index: Inventory,
    before: Inventory,
    working: Option<Inventory>,
    focus_text: Option<String>,
    pub(super) workspace: bool,
}

impl Context {
    pub(in crate::git::diff) fn workspace_paths(
        root: &Path,
        focus: &str,
        base: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        let mut paths = BTreeSet::new();
        let index = super::checked(
            root,
            &["--literal-pathspecs".into(), "ls-files".into(), "-z".into()],
        )?;
        for row in crate::git::status::records(&index)? {
            paths.insert(
                std::str::from_utf8(row)
                    .context("workspace call paths must use UTF-8")?
                    .to_owned(),
            );
        }
        if let Some(base) = base {
            let tree = super::checked(
                root,
                &[
                    "ls-tree".into(),
                    "-r".into(),
                    "--name-only".into(),
                    "-z".into(),
                    base.into(),
                ],
            )?;
            for row in crate::git::status::records(&tree)? {
                paths.insert(
                    std::str::from_utf8(row)
                        .context("workspace call paths must use UTF-8")?
                        .to_owned(),
                );
            }
        }
        let paths = paths
            .into_iter()
            .filter(|path| path != focus)
            .filter(|path| {
                super::language(Path::new(path))
                    .is_some_and(|language| support(Capability::CallGraph, language).is_yes())
            })
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        ensure!(
            crate::git::git_call_expansion_allowed(paths.len() + 1, 0, 1),
            "workspace call context exceeds 256 source files; use explicit --include paths."
        );
        Ok(paths)
    }

    pub(in crate::git::diff) fn capture(
        root: &Path,
        focus: &str,
        include: &[PathBuf],
        base: Option<&str>,
        staged: bool,
        workspace: bool,
    ) -> Result<Self> {
        ensure!(
            workspace || include.len() <= 32,
            "include accepts at most 32 context paths."
        );
        ensure!(
            !workspace || include.len() <= 255,
            "workspace call context exceeds 256 source files."
        );
        let mut paths = BTreeSet::new();
        for path in include {
            ensure!(
                !path.as_os_str().is_empty()
                    && path.components().all(|p| matches!(p, Component::Normal(_))),
                "call context requires repository-relative file paths without parent traversal."
            );
            let path = path.components().collect::<PathBuf>();
            let path = path.to_str().context("call context paths must use UTF-8")?;
            if path != focus {
                paths.insert(path.to_owned());
            }
        }
        ensure!(
            !paths.is_empty(),
            "include requires a context file distinct from the focus path."
        );
        paths.insert(focus.to_owned());
        let filter_paths = paths
            .iter()
            .flat_map(|path| path.bytes().chain([0]))
            .collect::<Vec<_>>();
        crate::git::process::require_no_filters(root, &filter_paths)?;
        let index = inventory(root, &paths, None)?;
        let before = if let Some(base) = base {
            inventory(root, &paths, Some(base))?
        } else if staged {
            Inventory::new()
        } else {
            index.clone()
        };
        let mut working = (!staged).then(Inventory::new);
        let mut working_texts = BTreeMap::new();
        if let Some(working) = &mut working {
            for path in &paths {
                if index.contains_key(path) {
                    if let Some((blob, text)) = working_file(root, path)? {
                        working.insert(path.clone(), blob);
                        working_texts.insert(path.clone(), text);
                    }
                }
            }
        }
        let after = working.as_ref().unwrap_or(&index);
        let mut sides = [Vec::new(), Vec::new()];
        let mut coverage = Vec::new();
        let mut source_bytes = 0usize;
        let parsers = Parsers::new();
        let mut extractor = Extractor::new();
        for path in &paths {
            ensure!(
                index.contains_key(path) || before.contains_key(path),
                "call context path is absent from the selected index and commit: {path:?}."
            );
            if path == focus {
                continue;
            }
            let path = Path::new(path);
            let language = language(path).context("unsupported call context file extension")?;
            ensure!(
                support(Capability::CallGraph, language).is_yes(),
                "unsupported call context language: {language:?}."
            );
            let mut entry = json!({"path":path,"language":language,"hierarchy_supported":Family::of(language).is_some()});
            for (side, inventory) in [&before, after].into_iter().enumerate() {
                let name = ["before", "after"][side];
                let Some(blob) = inventory.get(path.to_str().unwrap()) else {
                    entry[name] = json!({"status":"absent","blob":null,"mode":null});
                    continue;
                };
                let text = if side == 1 && !staged {
                    working_texts
                        .remove(path.to_str().unwrap())
                        .context("missing captured working source")?
                } else {
                    source(root, path.to_str().unwrap(), &blob.oid, false)?
                };
                if text.contains('\0') {
                    bail!("binary call context is unsupported: {:?}.", path);
                }
                source_bytes = source_bytes
                    .checked_add(text.len())
                    .context("workspace call source size overflow")?;
                ensure!(
                    !workspace
                        || crate::git::git_call_expansion_allowed(paths.len(), source_bytes, 1),
                    "workspace call context exceeds 64 MiB of captured source."
                );
                let parsed = parsers.parse(language, &text)?;
                let facts = extractor.extract(&parsed, path, &text)?;
                entry[name] = json!({"status":if facts.gaps.is_empty() {"parsed"} else {"partial"},
                    "blob":blob.oid,"mode":blob.mode,"source_basis":if side == 1 && !staged {"raw-worktree"} else {"git-blob"},"bytes":text.len(),"gaps":facts.gaps.iter().map(|gap|gap.as_str()).collect::<Vec<_>>()});
                sides[side].push(Snapshot {
                    path: path.to_path_buf(),
                    language,
                    source: text,
                    facts,
                });
            }
            coverage.push(entry);
        }
        let working_revision = working
            .as_ref()
            .map(|working| {
                serde_json::to_vec(working).map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            })
            .transpose()?;
        Ok(Self {
            sides,
            coverage: json!({"scope":match (workspace,staged) {(true,true)=>"workspace-staged-files",(true,false)=>"workspace-working-files",(false,true)=>"explicit-staged-files",(false,false)=>"explicit-working-files"},
                "focus":focus,"files":coverage,"working_revision":working_revision,"file_limit":if workspace {256}else{33},
                "source_bytes":source_bytes,"source_byte_limit":workspace.then_some(67_108_864usize)}),
            paths,
            index,
            before,
            working,
            focus_text: working_texts.remove(focus),
            workspace,
        })
    }

    pub(super) fn focus_source(&self, path: &str, oid: &str) -> Result<Option<&str>> {
        if let Some(working) = &self.working {
            ensure!(
                working.get(path).is_some_and(|blob| blob.oid == oid),
                "focus snapshot differs from the Git diff; retry or inspect conversion attributes."
            );
        }
        Ok(self.focus_text.as_deref())
    }

    pub(in crate::git::diff) fn recheck(
        &self,
        root: &Path,
        focus: &str,
        observed: &patch::Observation,
    ) -> Result<()> {
        ensure!(
            inventory(root, &self.paths, None)? == self.index,
            "selected Git index entries changed during call inspection; retry the query."
        );
        if let Some(working) = &self.working {
            for path in &self.paths {
                if self.index.contains_key(path) {
                    let observed = working_file(root, path)?.map(|(blob, _)| blob);
                    ensure!(
                        observed.as_ref() == working.get(path),
                        "selected working files changed during call inspection; retry the query."
                    );
                }
            }
            ensure!(
                inventory(root, &self.paths, None)? == self.index,
                "selected Git index entries changed during call inspection; retry the query."
            );
        }
        let before = self.before.get(focus);
        let after = self.working.as_ref().unwrap_or(&self.index).get(focus);
        if observed.change.is_some() {
            let ids = (before.map(|b| b.oid.clone()), after.map(|b| b.oid.clone()));
            let mut observed_ids = observed.blobs.clone();
            if self.working.is_some()
                && observed.hunks == 0
                && !observed.binary
                && before.is_some()
                && after.is_some()
            {
                if let Some((before, after)) = &mut observed_ids {
                    if after.is_none() {
                        *after = before.clone();
                    }
                }
            }
            ensure!(
                observed_ids.as_ref() == Some(&ids),
                "focus diff differs from captured call context basis; retry the query."
            );
        } else {
            ensure!(
                before.map(|b| &b.oid) == after.map(|b| &b.oid)
                    && (self.working.is_some() || before == after),
                "focus diff differs from captured call context basis; retry the query."
            );
        }
        Ok(())
    }
}
