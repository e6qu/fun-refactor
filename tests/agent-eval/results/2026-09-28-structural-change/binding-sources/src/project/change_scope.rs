use super::{hash, Project};
use crate::analysis::entrypoints::Catalog;
use crate::model::{Reference, ReferenceKind, SymbolId};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Read;
use std::path::Path;

const MAP: &str = ".fr/check-scopes.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub key: String,
    pub digest: String,
}

#[derive(Args)]
pub struct Options {
    #[arg(required = true, num_args = 1..)]
    targets: Vec<String>,
    #[arg(long, default_value_t = 4)]
    depth: usize,
    #[arg(long, default_value_t = 128)]
    nodes: usize,
    #[arg(long, default_value_t = 512)]
    references: usize,
    #[arg(long, default_value_t = 65536)]
    bytes: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selector {
    path: String,
    name: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    targets: Vec<Selector>,
    depth: usize,
    nodes: usize,
    references: usize,
    bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckMap {
    schema: u32,
    checks: Vec<Association>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Association {
    name: String,
    paths: Vec<String>,
}

pub fn review_ready(complete: bool, certain: bool, mapped: bool, checks: bool) -> bool {
    complete && certain && mapped && checks
}

fn relative(path: &str) -> bool {
    !Path::new(path).is_absolute()
        && !path.contains('\\')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn mapping(root: &Path) -> Result<(Vec<Association>, String, Option<String>)> {
    let mut path = root.to_path_buf();
    for part in MAP.split('/') {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "check scope map cannot traverse symlinks"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), hash("absent-check-scope-map")?, None))
            }
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(path.is_file(), "check scope map must be a regular file");
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "check scope map exceeds 64 KiB");
    let map: CheckMap = serde_json::from_slice(&bytes)?;
    ensure!(
        map.schema == 1 && map.checks.len() <= 32,
        "invalid check scope map schema or count"
    );
    let mut names = BTreeSet::new();
    for check in &map.checks {
        ensure!(
            names.insert(check.name.clone()) && !check.paths.is_empty() && check.paths.len() <= 64,
            "check scope associations need distinct names and 1..64 paths."
        );
        let mut paths = BTreeSet::new();
        ensure!(
            check
                .paths
                .iter()
                .all(|p| relative(p) && p.len() <= 512 && paths.insert(p)),
            "check scope paths must be distinct normalized relative paths."
        );
    }
    let selected = crate::checks::select(root, &names.into_iter().collect::<Vec<_>>())?;
    Ok((
        map.checks,
        hash(bytes)?,
        selected.map(|s| s.configuration_basis),
    ))
}

fn validate(query: &Query) -> Result<()> {
    ensure!(
        !query.targets.is_empty()
            && query.targets.len() <= 16
            && query.depth <= 16
            && (1..=256).contains(&query.nodes)
            && (1..=4096).contains(&query.references)
            && (1024..=1048576).contains(&query.bytes),
        "invalid change scope budgets"
    );
    let mut distinct = BTreeSet::new();
    for target in &query.targets {
        ensure!(
            relative(&target.path)
                && target.path.len() <= 512
                && !target.name.is_empty()
                && target.name.len() <= 256
                && distinct.insert((&target.path, &target.name, &target.kind)),
            "change scope requires distinct bounded declaration selectors."
        );
    }
    Ok(())
}

impl Project<'_> {
    fn scope_seeds(&self, query: &Query) -> Result<Vec<SymbolId>> {
        validate(query)?;
        query
            .targets
            .iter()
            .map(|target| {
                let path = self.root.join(&target.path);
                let found: Vec<_> = self
                    .index
                    .symbols
                    .iter()
                    .filter(|s| {
                        s.file == path && s.name == target.name && s.kind.as_str() == target.kind
                    })
                    .collect();
                ensure!(
                    found.len() == 1,
                    "change scope requires one exact declaration per selector."
                );
                Ok(found[0].id)
            })
            .collect()
    }

    fn scope_digest(&self, query: &Query, map: &str, checks: &Option<String>) -> Result<String> {
        self.scope_seeds(query)?;
        hash((
            &self.revision,
            crate::cache::fact_semantics_fingerprint(),
            query,
            map,
            checks,
            include_str!("change_scope.rs"),
            include_str!("../index.rs"),
            include_str!("../index/references.rs"),
            include_str!("../model.rs"),
            include_str!("../analysis/entrypoints.rs"),
            include_str!("occurrence.rs"),
            include_str!("relationships.rs"),
            format!("{:?}", Catalog::builtin()?.rules),
        ))
    }

    pub(super) fn change_scope_dependency(&self, key: &str) -> Result<String> {
        ensure!(key.len() <= 16384, "change scope dependency exceeds 16 KiB");
        let query: Query = serde_json::from_str(key)?;
        let (_, map, checks) = mapping(&self.root)?;
        self.scope_digest(&query, &map, &checks)
    }

    fn consumer(&self, reference: &Reference) -> Option<SymbolId> {
        self.index
            .file(&reference.file)
            .into_iter()
            .flat_map(|f| &f.symbols)
            .filter_map(|id| self.index.symbol(*id))
            .filter(|s| !s.kind.is_local() && s.full_span.contains(reference.span))
            .min_by_key(|s| (s.full_span.len(), s.id))
            .map(|s| s.id)
    }

    pub(super) fn change_scope(&self, options: &Options) -> Result<Value> {
        let mut targets = Vec::new();
        for handle in &options.targets {
            let node = self.target(handle)?;
            let symbol = self.nodes[node]
                .symbol
                .and_then(|id| self.index.symbol(id))
                .context("change scope requires declaration handles.")?;
            targets.push(Selector {
                path: self.nodes[node].path.to_string_lossy().into_owned(),
                name: symbol.name.clone(),
                kind: symbol.kind.as_str().into(),
            });
        }
        let query = Query {
            targets,
            depth: options.depth,
            nodes: options.nodes,
            references: options.references,
            bytes: options.bytes,
        };
        let seeds = self.scope_seeds(&query)?;
        ensure!(
            seeds.len() <= query.nodes,
            "node budget cannot omit selected declarations."
        );
        let (associations, map_digest, check_basis) = mapping(&self.root)?;
        let digest = self.scope_digest(&query, &map_digest, &check_basis)?;
        let mut reached: BTreeMap<_, _> = seeds.iter().map(|id| (*id, 0usize)).collect();
        let mut queue: VecDeque<_> = seeds.iter().copied().collect();
        let mut edges = Vec::new();
        let mut unresolved = Vec::new();
        let mut files = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut cutoffs = BTreeSet::new();
        let mut certain = true;
        while let Some(target) = queue.pop_front() {
            let distance = reached[&target];
            files.insert(
                self.index
                    .symbol(target)
                    .unwrap()
                    .file
                    .strip_prefix(&self.root)?
                    .to_string_lossy()
                    .into_owned(),
            );
            let mut references = self.index.references_to(target);
            references.extend(
                self.index
                    .unresolved_matching(target)
                    .into_iter()
                    .filter(|r| r.target.is_none()),
            );
            references.sort_by_key(|r| (&r.file, r.span, r.target));
            for reference in references {
                if reference.kind == ReferenceKind::Textual
                    || !seen.insert((
                        target,
                        reference.file.clone(),
                        reference.span,
                        reference.target,
                    ))
                {
                    continue;
                }
                if edges.len() + unresolved.len() == query.references {
                    cutoffs.insert("reference-budget");
                    break;
                }
                let occurrence =
                    self.occurrence(&reference.file, reference.span, "consumer-reference")?;
                files.insert(occurrence.path.clone());
                if reference.target.is_none() {
                    certain = false;
                    unresolved.push(
                        json!({"target": self.endpoint(target)?, "occurrence": occurrence,
                        "reason": "same-name unresolved reference; relationship not established."}),
                    );
                    continue;
                }
                let consumer = self.consumer(reference);
                certain &= reference.confidence.is_safe_to_rewrite();
                edges.push(json!({"target": self.endpoint(target)?, "consumer": consumer.map(|id| self.endpoint(id)).transpose()?,
                    "occurrence": occurrence, "reference_kind": reference.kind, "confidence": reference.confidence,
                    "distance": distance + 1, "status": "indexed-candidate"}));
                if let Some(consumer) = consumer {
                    if reached.contains_key(&consumer) {
                        continue;
                    }
                    if distance >= query.depth {
                        cutoffs.insert("depth-budget");
                    } else if reached.len() == query.nodes {
                        cutoffs.insert("node-budget");
                    } else {
                        reached.insert(consumer, distance + 1);
                        queue.push_back(consumer);
                    }
                }
            }
        }
        let catalog = Catalog::builtin()?.tests_in_snapshot(self.index, &self.sources);
        let tests = catalog
            .entries
            .iter()
            .filter(|entry| reached.contains_key(&entry.symbol))
            .map(|entry| {
                Ok(
                    json!({"test": self.endpoint(entry.symbol)?, "rule": entry.rule,
                "status": "catalog-candidate", "distance": reached[&entry.symbol]}),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut checks = Vec::new();
        let mut mapped = BTreeSet::new();
        let mut missing = BTreeSet::new();
        for association in associations {
            let matched: Vec<_> = association
                .paths
                .iter()
                .filter(|p| files.contains(*p))
                .cloned()
                .collect();
            for path in &association.paths {
                let absolute = self.root.join(path);
                if !self.sources.contains_key(&absolute)
                    && !self.manifests.snapshots.contains_key(&absolute)
                    && !self.lockfiles.snapshots.contains_key(&absolute)
                {
                    missing.insert(path.clone());
                }
            }
            if !matched.is_empty() {
                mapped.extend(matched.iter().cloned());
                checks.push(json!({"name": association.name, "matched_paths": matched,
                    "declared_paths": association.paths, "status": "declared-candidate"}));
            }
        }
        let uncovered: Vec<_> = files.difference(&mapped).cloned().collect();
        let coverage = self.coverage();
        let scan_complete = coverage["skipped_files"] == 0
            && coverage["skipped_symlinks"] == 0
            && coverage["files_by_gap"]
                .as_object()
                .is_some_and(|g| g.is_empty());
        let complete = cutoffs.is_empty() && scan_complete && catalog.gaps.is_empty();
        let ready = review_ready(
            complete,
            certain,
            uncovered.is_empty() && missing.is_empty(),
            !checks.is_empty(),
        );
        let mut report = json!({"schema": "fr-change-scope-1", "revision": self.revision,
            "handle_prefix": format!("frp1:{}:", &self.revision[..32]),
            "input_digest": digest, "dependency": {"kind": "change-scope", "key": serde_json::to_string(&query)?, "digest": digest},
            "targets": seeds.iter().map(|id| self.endpoint(*id)).collect::<Result<Vec<_>>>()?,
            "consumers": reached.iter().map(|(id, distance)| Ok(json!({"declaration": self.endpoint(*id)?, "distance": distance}))).collect::<Result<Vec<_>>>()?,
            "references": edges, "unresolved": unresolved, "test_candidates": tests,
            "affected_paths": files, "check_candidates": checks, "unmapped_paths": uncovered,
            "missing_mapped_paths": missing, "check_configuration_basis": check_basis, "map_digest": map_digest,
            "coverage": coverage, "catalog_gaps": catalog.gaps.len(), "cutoffs": cutoffs,
            "indexed_complete": complete, "review_ready": ready, "runtime_coverage": false,
            "limits": {"depth": query.depth, "nodes": query.nodes, "references": query.references, "bytes": query.bytes},
            "scope": "Indexed references and lexical containing declarations; textual mentions excluded. Test and check candidates do not establish execution or behavioral coverage.",
            "limitations": "Dynamic dispatch, reflection, external consumers and unindexed files remain outside this relation. Exact-file check associations are user declarations. Resumption revalidates the whole selected workspace; no mutation is authorized."});
        let mut omitted = 0;
        while serde_json::to_vec(&report)?.len() + 128 > query.bytes {
            let mut removed = false;
            for name in [
                "references",
                "unresolved",
                "consumers",
                "test_candidates",
                "check_candidates",
                "affected_paths",
                "unmapped_paths",
                "missing_mapped_paths",
            ] {
                let rows = report[name].as_array_mut().unwrap();
                if !rows.is_empty() {
                    let keep = rows.len() / 2;
                    omitted += rows.len() - keep;
                    rows.truncate(keep);
                    removed = true;
                    break;
                }
            }
            report["indexed_complete"] = json!(false);
            report["review_ready"] = json!(false);
            report["omitted_records"] = json!(omitted);
            report["cutoffs"] = json!(cutoffs
                .iter()
                .copied()
                .chain(["output-budget"])
                .collect::<Vec<_>>());
            ensure!(
                removed,
                "change scope metadata exceeds response budget; increase --bytes."
            );
        }
        let (_, current_map, current_checks) = mapping(&self.root)?;
        ensure!(
            current_map == map_digest && current_checks == check_basis,
            "check scope inputs changed during discovery."
        );
        Ok(report)
    }
}
pub(super) fn validate_bound(
    project: &Project<'_>,
    binding: &Binding,
    handles: &[String],
    checks: &[String],
) -> Result<Value> {
    ensure!(
        project.change_scope_dependency(&binding.key)? == binding.digest,
        "change scope inputs changed; rediscover consumers and review again."
    );
    let query: Query = serde_json::from_str(&binding.key)?;
    let targets = project
        .scope_seeds(&query)?
        .iter()
        .map(|id| project.handle(project.symbol_nodes[id]))
        .collect();
    let report = project.change_scope(&Options {
        targets,
        depth: query.depth,
        nodes: query.nodes,
        references: query.references,
        bytes: query.bytes,
    })?;
    ensure!(
        report["review_ready"] == true,
        "change scope has gaps; resolve them before scoped delivery."
    );
    let allowed: BTreeSet<_> = report["consumers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["declaration"]["handle"].as_str())
        .collect();
    ensure!(
        handles
            .iter()
            .all(|handle| allowed.contains(handle.as_str())),
        "task-change target is outside the reviewed consumer scope."
    );
    ensure!(
        report["check_candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| checks.iter().any(|name| row["name"] == *name)),
        "scoped delivery must select every declared candidate check."
    );
    Ok(json!({"input_digest": binding.digest, "review_ready": true,
        "targets": handles, "checks": report["check_candidates"],
        "affected_paths": report["affected_paths"],
        "runtime_coverage": false}))
}
