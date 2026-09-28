{
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
            && coverage["unsupported_files"] == 0
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