//! The workspace index: symbols, references and their resolution.
//!
//! Extraction is per-file and knows nothing beyond the file it read. The index is
//! where facts from many files meet and references become resolved edges, each
//! carrying a [`Confidence`] that says how much the resolution can be trusted
//! (PLAN.md D4). Refactorings act on `exact`/`import-qualified` edges and refuse to
//! silently rewrite anything weaker.

use crate::extract::Extractor;
use crate::lang::{Language, LanguageClass};
use crate::model::*;
use crate::parse::Parsers;
use crate::scan::{scan, ScanOptions, ScanResult};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Per-file data retained after merging into the global index.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub language: Language,
    pub scopes: Vec<Scope>,
    pub imports: Vec<Import>,
    /// Global ids of symbols defined in this file.
    pub symbols: Vec<SymbolId>,
    /// Indices into [`Index::references`] for references in this file.
    pub references: Vec<usize>,
    /// True if the file failed to parse cleanly; resolutions from it are suspect.
    pub had_parse_errors: bool,
}

/// A resolved workspace.
#[derive(Debug, Default)]
pub struct Index {
    /// All symbols, keyed by their global [`SymbolId`] (== position in this vec).
    pub symbols: Vec<Symbol>,
    /// All references; `target` holds a global [`SymbolId`].
    pub references: Vec<Reference>,
    files: BTreeMap<PathBuf, FileInfo>,
    /// Files skipped during scanning, reported rather than silently dropped.
    pub skipped: Vec<(PathBuf, String)>,
}

impl Index {
    /// Build an index for a workspace root.
    pub fn build(root: &Path, options: &ScanOptions) -> Result<Self> {
        let scan_result = scan(root, options)?;
        Self::build_from_scan(&scan_result)
    }

    pub fn build_from_scan(scan_result: &ScanResult) -> Result<Self> {
        Self::build_with_cache(scan_result, crate::cache::Cache::open().as_ref())
    }

    /// Build an index, reusing previously extracted facts where the content matches.
    ///
    /// Parsing and extraction dominate indexing cost and depend only on a file's bytes
    /// and the query set, so an unchanged file need never be looked at twice.
    pub fn build_with_cache(
        scan_result: &ScanResult,
        cache: Option<&crate::cache::Cache>,
    ) -> Result<Self> {
        use rayon::prelude::*;

        let mut index = Index::default();
        for (path, size) in &scan_result.skipped_too_large {
            index
                .skipped
                .push((path.clone(), format!("exceeds size limit ({size} bytes)")));
        }

        // Extraction is per-file and shares nothing, so it parallelises cleanly. The
        // results are collected in scan order and merged serially afterwards, because
        // symbol ids are assigned by position and must not depend on thread timing.
        let extracted: Vec<Result<Option<(usize, FileFacts)>>> = scan_result
            .files
            .par_iter()
            .enumerate()
            .map(|(position, file)| {
                let source = match std::fs::read_to_string(&file.path) {
                    Ok(s) => s,
                    // Reported by the caller once results are merged.
                    Err(e) => return Ok(Some((position, Self::unreadable_placeholder(&file.path, e.to_string())))),
                };

                // A cached entry carries its own parse-error flag, so a hit skips
                // parsing entirely rather than reparsing to ask.
                if let Some(cache) = cache {
                    let key = crate::cache::Cache::key(file.language, &source);
                    if let Some(facts) = cache.get(&key, &file.path) {
                        return Ok(Some((position, facts)));
                    }
                }

                // Parsers and compiled queries are not shareable across threads, so
                // each worker builds its own. Query compilation is the cost here and
                // it is paid once per thread, not once per file.
                let parsers = Parsers::new();
                let mut extractor = Extractor::new();
                let parsed = parsers.parse(file.language, &source)?;
                let had_parse_errors = parsed.has_errors();
                let mut facts = extractor
                    .extract(&parsed, &file.path, &source)
                    .with_context(|| format!("extracting facts from {}", file.path.display()))?;
                facts.had_parse_errors = had_parse_errors;

                if let Some(cache) = cache {
                    let key = crate::cache::Cache::key(file.language, &source);
                    cache.put(&key, &facts);
                }
                Ok(Some((position, facts)))
            })
            .collect();

        let mut ordered: Vec<(usize, FileFacts)> = Vec::with_capacity(extracted.len());
        for outcome in extracted {
            if let Some(entry) = outcome? {
                ordered.push(entry);
            }
        }
        ordered.sort_by_key(|(position, _)| *position);

        for (position, facts) in ordered {
            let file = &scan_result.files[position];
            if let Some(reason) = facts.unreadable.clone() {
                index.skipped.push((file.path.clone(), reason));
                continue;
            }
            let had_errors = facts.had_parse_errors;
            index.add_file(facts, file.language, had_errors);
        }

        index.resolve();
        Ok(index)
    }

    /// Placeholder facts marking a file that could not be read.
    ///
    /// Carrying the failure through the parallel stage keeps the reporting in one
    /// place instead of needing a second channel out of the worker.
    fn unreadable_placeholder(path: &Path, error: String) -> FileFacts {
        FileFacts {
            path: path.to_path_buf(),
            unreadable: Some(error),
            ..Default::default()
        }
    }

    /// Build an index from sources held in memory rather than read from disk.
    ///
    /// A cascading refactoring rewrites files and must re-resolve against the result
    /// before deciding what to do next; writing each intermediate state to disk just
    /// to read it back would be both slower and observable.
    pub fn build_from_sources(sources: &[(PathBuf, Language, String)]) -> Result<Self> {
        let parsers = Parsers::new();
        let mut extractor = Extractor::new();
        let mut index = Index::default();

        for (path, language, source) in sources {
            let parsed = parsers.parse(*language, source)?;
            let had_parse_errors = parsed.has_errors();
            let mut facts = extractor
                .extract(&parsed, path, source)
                .with_context(|| format!("extracting facts from {}", path.display()))?;
            facts.had_parse_errors = had_parse_errors;
            index.add_file(facts, *language, had_parse_errors);
        }

        index.resolve();
        Ok(index)
    }

    /// Merge one file's facts, remapping file-local ids into the global namespace.
    pub fn add_file(&mut self, facts: FileFacts, language: Language, had_parse_errors: bool) {
        let base = self.symbols.len() as u32;
        let remap = |local: SymbolId| SymbolId(local.0 + base);

        let mut symbol_ids = Vec::with_capacity(facts.symbols.len());
        for mut symbol in facts.symbols {
            symbol.id = remap(symbol.id);
            symbol.container = symbol.container.map(remap);
            symbol_ids.push(symbol.id);
            self.symbols.push(symbol);
        }

        let mut reference_ids = Vec::with_capacity(facts.references.len());
        for reference in facts.references {
            reference_ids.push(self.references.len());
            self.references.push(reference);
        }

        self.files.insert(
            facts.path,
            FileInfo {
                language,
                scopes: facts.scopes,
                imports: facts.imports,
                symbols: symbol_ids,
                references: reference_ids,
                had_parse_errors,
            },
        );
    }

    pub fn file(&self, path: &Path) -> Option<&FileInfo> {
        self.files.get(path)
    }

    pub fn files(&self) -> impl Iterator<Item = (&PathBuf, &FileInfo)> {
        self.files.iter()
    }

    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0 as usize)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Resolve every reference to a symbol, tagging each with a confidence.
    fn resolve(&mut self) {
        // Resolve against an immutable view first, then write the answers back:
        // resolution reads the whole index, so it cannot hold a mutable borrow.
        let resolutions: Vec<(usize, Option<SymbolId>, Confidence)> = {
            let mut by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
            for symbol in &self.symbols {
                by_name
                    .entry(symbol.name.as_str())
                    .or_default()
                    .push(symbol.id);
            }

            let mut out = Vec::with_capacity(self.references.len());
            for (path, info) in &self.files {
                for ref_idx in &info.references {
                    let reference = &self.references[*ref_idx];
                    let (target, confidence) = self.resolve_one(reference, path, info, &by_name);
                    out.push((*ref_idx, target, confidence));
                }
            }
            out
        };

        for (idx, target, confidence) in resolutions {
            self.references[idx].target = target;
            self.references[idx].confidence = confidence;
        }
    }

    fn resolve_one(
        &self,
        reference: &Reference,
        path: &Path,
        info: &FileInfo,
        by_name: &HashMap<&str, Vec<SymbolId>>,
    ) -> (Option<SymbolId>, Confidence) {
        let candidates = match by_name.get(reference.name.as_str()) {
            Some(c) => c,
            None => return (None, Confidence::NameOnly),
        };

        // 1. Lexical scope: the innermost definition in this file whose scope encloses
        //    the reference. This is what makes shadowing correct.
        let scope_chain = self.scope_chain(info, reference.scope);
        let in_file: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.file == path)
            .collect();

        let scoped = in_file
            .iter()
            .filter(|s| scope_chain.contains(&s.scope))
            .min_by_key(|s| {
                // Innermost enclosing scope wins; ties break toward the nearest
                // preceding definition, which is how shadowing reads.
                let depth = scope_chain
                    .iter()
                    .position(|sc| *sc == s.scope)
                    .unwrap_or(usize::MAX);
                (depth, distance(s.name_span.start, reference.span.start))
            });
        if let Some(symbol) = scoped {
            return (Some(symbol.id), Confidence::Exact);
        }

        // 2. Any other definition in the same file (e.g. a function defined below its
        //    use, or a sibling scope for languages that hoist).
        if in_file.len() == 1 {
            return (Some(in_file[0].id), Confidence::Exact);
        }
        if in_file.len() > 1 {
            // Ambiguous within the file: report the nearest but do not claim certainty.
            let nearest = in_file
                .iter()
                .min_by_key(|s| distance(s.name_span.start, reference.span.start))
                .unwrap();
            return (Some(nearest.id), Confidence::NameOnly);
        }

        // 3. Bound by an import in this file: resolve into the imported file when the
        //    import path identifies one, else accept the unique exported match.
        if let Some(import) = self.import_binding(info, &reference.name) {
            let imported_file = self.resolve_import_path(path, &import.path);
            let original = import
                .names
                .iter()
                .find(|n| n.local == reference.name)
                .map(|n| n.original.clone())
                .unwrap_or_else(|| reference.name.clone());

            let mut matches: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| s.name == original)
                .collect();
            if let Some(target_file) = &imported_file {
                matches.retain(|s| &s.file == target_file);
            }
            matches.retain(|s| s.exported || imported_file.is_some());

            if matches.len() == 1 {
                return (Some(matches[0].id), Confidence::ImportQualified);
            }
            if !matches.is_empty() {
                // A glob import or an ambiguous path: plausible but unproven.
                return (Some(matches[0].id), Confidence::FieldBased);
            }
        }

        // 4. String-keyed references. In CSS, HTML, XML and Markdown a reference
        //    names its target globally: `class="btn"` refers to whatever declares
        //    `.btn`, in any file. That is the language's actual rule, not a
        //    heuristic, so a unique kind match resolves exactly. This is what lets a
        //    CSS class rename reach HTML and TSX.
        if reference.kind == ReferenceKind::StringRef {
            // Fragment references (`href="#top"`) name the id or heading without it.
            let bare = reference.name.strip_prefix('#').unwrap_or(&reference.name);
            let targets: Vec<&Symbol> = self
                .symbols
                .iter()
                .filter(|s| s.kind.is_string_keyed() && s.name == bare)
                .collect();

            let kinds: HashSet<SymbolKind> = targets.iter().map(|s| s.kind).collect();
            match kinds.len() {
                1 => return (Some(targets[0].id), Confidence::Exact),
                0 => {}
                // The same name declared as two different kinds is genuinely
                // ambiguous; report it rather than pick one.
                _ => return (Some(targets[0].id), Confidence::FieldBased),
            }
        }

        // 5. Field access without a known receiver type: name-matched at best.
        if reference.kind == ReferenceKind::Field {
            let members: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| matches!(s.kind, SymbolKind::Field | SymbolKind::Method))
                .collect();
            if members.len() == 1 {
                return (Some(members[0].id), Confidence::FieldBased);
            }
            if !members.is_empty() {
                return (None, Confidence::FieldBased);
            }
        }

        // 6. Directory-scoped languages: Terraform's module is a directory, so a
        //    definition anywhere beside this file is in scope. Names are unique per
        //    namespace there, so a single match is exact; several mean the namespace
        //    (`var.` versus `local.`) decides, which this layer cannot see, so those
        //    are reported rather than rewritten.
        if reference.language.resolves_by_directory() {
            let dir = path.parent();
            let siblings: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| s.file.parent() == dir)
                .collect();
            match siblings.len() {
                1 => return (Some(siblings[0].id), Confidence::Exact),
                0 => {}
                _ => return (Some(siblings[0].id), Confidence::FieldBased),
            }
        }

        // 7. A single exported definition anywhere in the workspace. Plausible, but
        //    nothing proved this file can see it, so it stays name-only.
        let exported: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.exported)
            .collect();
        if exported.len() == 1 {
            return (Some(exported[0].id), Confidence::NameOnly);
        }

        (None, Confidence::NameOnly)
    }

    /// The scope chain for a reference, innermost first.
    fn scope_chain(&self, info: &FileInfo, scope: ScopeId) -> Vec<ScopeId> {
        let mut chain = vec![scope];
        let mut current = scope;
        for _ in 0..info.scopes.len() {
            let Some(parent) = info
                .scopes
                .iter()
                .find(|s| s.id == current)
                .and_then(|s| s.parent)
            else {
                break;
            };
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// The import in `info` that binds `name`, if any.
    fn import_binding<'a>(&self, info: &'a FileInfo, name: &str) -> Option<&'a Import> {
        info.imports.iter().find(|import| {
            import.alias.as_deref() == Some(name)
                || import.names.iter().any(|n| n.local == name)
                // A glob import can bind anything, so it is a possible source.
                || import.is_glob
                // `use a::b::c;` binds `c`; `import ./utils` binds via the file stem.
                || import
                    .path
                    .rsplit(['/', ':', '.'])
                    .find(|s| !s.is_empty())
                    .is_some_and(|last| last == name)
        })
    }

    /// Map an import path to a file in the workspace.
    ///
    /// Handles the relative-path forms used by TypeScript, Python, SCSS and Bash.
    /// Module systems that need build configuration (Rust crate paths, Go module
    /// paths, tsconfig `paths` aliases) are not resolved here; callers see the
    /// weaker confidence that results rather than a wrong answer.
    fn resolve_import_path(&self, from: &Path, import_path: &str) -> Option<PathBuf> {
        if import_path.is_empty() {
            return None;
        }
        let dir = from.parent()?;

        if import_path.starts_with('.') || import_path.starts_with('/') {
            let base = dir.join(import_path.trim_start_matches("./"));
            let candidates = [
                base.clone(),
                base.with_extension("ts"),
                base.with_extension("tsx"),
                base.with_extension("py"),
                base.with_extension("scss"),
                base.with_extension("css"),
                base.with_extension("sh"),
                base.join("index.ts"),
                base.join("index.tsx"),
                base.join("__init__.py"),
            ];
            return candidates.into_iter().find(|c| self.files.contains_key(c));
        }

        // Dotted module paths (Python `pkg.mod`, Rust `crate::mod`) map to a file
        // whose stem matches the final segment — accepted only when unambiguous.
        let last = import_path
            .rsplit(['/', ':', '.'])
            .find(|s| !s.is_empty())?;
        let matches: Vec<&PathBuf> = self
            .files
            .keys()
            .filter(|p| p.file_stem().and_then(|s| s.to_str()) == Some(last))
            .collect();
        if matches.len() == 1 {
            return Some(matches[0].clone());
        }
        None
    }

    /// All references that resolve to `symbol`.
    pub fn references_to(&self, symbol: SymbolId) -> Vec<&Reference> {
        self.references
            .iter()
            .filter(|r| r.target == Some(symbol))
            .collect()
    }

    /// References that merely share a name with `symbol` but resolved elsewhere or
    /// not at all. These are what a rename must report rather than rewrite.
    pub fn unresolved_matching(&self, symbol: SymbolId) -> Vec<&Reference> {
        let Some(sym) = self.symbol(symbol) else {
            return Vec::new();
        };
        self.references
            .iter()
            .filter(|r| r.name == sym.name && r.target != Some(symbol))
            .collect()
    }

    /// Every definition site of the entity `symbol` belongs to.
    ///
    /// Usually just the symbol itself. For kinds with no canonical definition — CSS
    /// classes, custom properties — it is every site that declares the same name, so
    /// a rename rewrites all of them rather than half.
    pub fn definition_group(&self, symbol: SymbolId) -> Vec<SymbolId> {
        let Some(sym) = self.symbol(symbol) else {
            return Vec::new();
        };
        if !sym.kind.allows_multiple_definitions() {
            return vec![symbol];
        }
        self.symbols
            .iter()
            .filter(|s| s.name == sym.name && s.kind == sym.kind)
            .map(|s| s.id)
            .collect()
    }

    /// Do these symbols all denote the same entity?
    pub fn is_one_entity(&self, symbols: &[&Symbol]) -> bool {
        let Some(first) = symbols.first() else {
            return false;
        };
        first.kind.allows_multiple_definitions()
            && symbols
                .iter()
                .all(|s| s.name == first.name && s.kind == first.kind)
    }

    /// Find a symbol by name, optionally narrowed to a file.
    pub fn find_symbols(&self, name: &str, in_file: Option<&Path>) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.name == name)
            .filter(|s| in_file.is_none_or(|f| s.file == f))
            .collect()
    }

    /// The definition at a byte offset in a file, whether the offset is on the
    /// definition's identifier or on a reference to it.
    pub fn definition_at(&self, path: &Path, offset: usize) -> Option<&Symbol> {
        let info = self.files.get(path)?;
        if let Some(symbol) = info
            .symbols
            .iter()
            .filter_map(|id| self.symbol(*id))
            .find(|s| s.name_span.contains_offset(offset))
        {
            return Some(symbol);
        }
        let reference = info
            .references
            .iter()
            .map(|i| &self.references[*i])
            .find(|r| r.span.contains_offset(offset))?;
        reference.target.and_then(|t| self.symbol(t))
    }

    /// The reference at a byte offset, if any.
    pub fn reference_at(&self, path: &Path, offset: usize) -> Option<&Reference> {
        let info = self.files.get(path)?;
        info.references
            .iter()
            .map(|i| &self.references[*i])
            .find(|r| r.span.contains_offset(offset))
    }

    /// Summary counts for reporting.
    pub fn stats(&self) -> IndexStats {
        let mut by_confidence: BTreeMap<&'static str, usize> = BTreeMap::new();
        for r in &self.references {
            *by_confidence.entry(r.confidence.as_str()).or_default() += 1;
        }
        IndexStats {
            files: self.files.len(),
            symbols: self.symbols.len(),
            references: self.references.len(),
            resolved: self.references.iter().filter(|r| r.target.is_some()).count(),
            by_confidence,
            files_with_parse_errors: self
                .files
                .values()
                .filter(|f| f.had_parse_errors)
                .count(),
            imperative_files: self
                .files
                .values()
                .filter(|f| f.language.class() == LanguageClass::Imperative)
                .count(),
        }
    }

    /// Names defined more than once across the workspace — the cases where
    /// name-based resolution would be wrong.
    pub fn ambiguous_names(&self) -> Vec<(&str, usize)> {
        let mut counts: HashMap<&str, HashSet<&Path>> = HashMap::new();
        for s in &self.symbols {
            counts.entry(&s.name).or_default().insert(&s.file);
        }
        let mut out: Vec<(&str, usize)> = counts
            .into_iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|(name, files)| (name, files.len()))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out
    }
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    pub references: usize,
    pub resolved: usize,
    pub by_confidence: BTreeMap<&'static str, usize>,
    pub files_with_parse_errors: usize,
    pub imperative_files: usize,
}

fn distance(a: usize, b: usize) -> usize {
    a.abs_diff(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::SourceFile;

    /// Build an index from in-memory sources written to a temp workspace.
    fn index_of(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
            scanned.files.push(SourceFile {
                language: crate::lang::detect(&path).unwrap(),
                path,
            });
        }
        let index = Index::build_from_scan(&scanned).unwrap();
        (tmp, index)
    }

    #[test]
    fn resolves_a_local_call_exactly() {
        let (_tmp, index) = index_of(&[("a.rs", "fn helper() {}\nfn main() { helper(); }\n")]);
        let helper = index.find_symbols("helper", None);
        assert_eq!(helper.len(), 1);
        let refs = index.references_to(helper[0].id);
        assert_eq!(refs.len(), 1, "got {refs:?}");
        assert_eq!(refs[0].confidence, Confidence::Exact);
    }

    #[test]
    fn shadowing_resolves_to_the_innermost_definition() {
        let src = "fn f() {\n    let x = 1;\n    {\n        let x = 2;\n        use_it(x);\n    }\n}\n";
        let (_tmp, index) = index_of(&[("a.rs", src)]);

        let inner_def_offset = src.rfind("let x").unwrap() + 4;
        let inner = index.definition_at(Path::new("dummy"), 0);
        assert!(inner.is_none(), "unknown file yields nothing");

        let path = _tmp.path().join("a.rs");
        let use_offset = src.find("use_it(x)").unwrap() + 7;
        let resolved = index
            .definition_at(&path, use_offset)
            .expect("x should resolve");
        // The use of `x` must bind to the inner `let x`, not the outer one.
        assert_eq!(resolved.name, "x");
        assert!(
            resolved.name_span.start >= inner_def_offset - 4,
            "expected the inner definition at {}, got {}",
            inner_def_offset,
            resolved.name_span.start
        );
    }

    #[test]
    fn same_name_in_two_files_is_not_conflated() {
        // This is the funveil call-graph failure mode the index must not repeat.
        let (_tmp, index) = index_of(&[
            ("a.rs", "fn parse() {}\nfn a_main() { parse(); }\n"),
            ("b.rs", "fn parse() {}\nfn b_main() { parse(); }\n"),
        ]);

        let parses = index.find_symbols("parse", None);
        assert_eq!(parses.len(), 2, "each file defines its own parse");

        // Each call must resolve to the definition in its OWN file.
        for symbol in &parses {
            let refs = index.references_to(symbol.id);
            assert_eq!(refs.len(), 1, "parse in {:?} got {refs:?}", symbol.file);
            assert_eq!(refs[0].file, symbol.file);
            assert_eq!(refs[0].confidence, Confidence::Exact);
        }
    }

    #[test]
    fn unresolved_references_are_reported_not_guessed() {
        let (_tmp, index) = index_of(&[("a.rs", "fn main() { nowhere_defined(); }\n")]);
        let r = index
            .references
            .iter()
            .find(|r| r.name == "nowhere_defined")
            .unwrap();
        assert_eq!(r.target, None);
        assert_eq!(r.confidence, Confidence::NameOnly);
    }

    #[test]
    fn stats_count_confidence_tiers() {
        let (_tmp, index) = index_of(&[("a.rs", "fn helper() {}\nfn main() { helper(); }\n")]);
        let stats = index.stats();
        assert_eq!(stats.files, 1);
        assert!(stats.symbols >= 2);
        assert!(stats.resolved >= 1);
        assert!(stats.by_confidence.contains_key("exact"));
    }

    #[test]
    fn ambiguous_names_are_listed() {
        let (_tmp, index) = index_of(&[
            ("a.rs", "fn parse() {}\n"),
            ("b.rs", "fn parse() {}\n"),
            ("c.rs", "fn unique_name() {}\n"),
        ]);
        let ambiguous = index.ambiguous_names();
        assert!(ambiguous.iter().any(|(n, c)| *n == "parse" && *c == 2));
        assert!(!ambiguous.iter().any(|(n, _)| *n == "unique_name"));
    }

    #[test]
    fn unreadable_files_are_skipped_and_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.rs");
        // Invalid UTF-8 cannot be read as source.
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let scanned = ScanResult {
            files: vec![SourceFile {
                path: path.clone(),
                language: Language::Rust,
            }],
            skipped_too_large: Vec::new(),
        };
        let index = Index::build_from_scan(&scanned).unwrap();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.skipped.len(), 1, "skip must be visible");
    }

    #[test]
    fn terraform_names_resolve_across_the_module_directory() {
        // Terraform's unit of scope is the directory, so `var.region` in main.tf
        // refers to the `variable "region"` declared in variables.tf. Without this a
        // rename would update the declaration and leave every use dangling.
        let (_tmp, index) = index_of(&[
            ("variables.tf", "variable \"region\" {\n  default = \"eu-west-1\"\n}\n"),
            (
                "main.tf",
                "resource \"aws_s3_bucket\" \"b\" {\n  region = var.region\n}\n",
            ),
        ]);
        let region = index.find_symbols("region", None);
        assert_eq!(region.len(), 1, "got {region:?}");

        let refs = index.references_to(region[0].id);
        assert_eq!(refs.len(), 1, "got {refs:?}");
        assert_eq!(refs[0].confidence, Confidence::Exact);
        assert!(refs[0].file.ends_with("main.tf"));
    }

    #[test]
    fn terraform_names_do_not_resolve_across_module_directories() {
        // A separate directory is a separate module; its variables are unrelated.
        let (_tmp, index) = index_of(&[
            ("variables.tf", "variable \"region\" {\n  default = \"a\"\n}\n"),
            ("child/main.tf", "output \"o\" {\n  value = var.region\n}\n"),
        ]);
        let region = index.find_symbols("region", None);
        assert_eq!(region.len(), 1);
        assert!(
            index.references_to(region[0].id).is_empty(),
            "a different directory is a different module"
        );
    }

    #[test]
    fn definition_at_works_from_both_definition_and_use_sites() {
        let src = "fn target() {}\nfn caller() { target(); }\n";
        let (tmp, index) = index_of(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let at_def = index.definition_at(&path, src.find("target").unwrap() + 1);
        let at_use = index.definition_at(&path, src.rfind("target").unwrap() + 1);
        assert_eq!(at_def.map(|s| s.id), at_use.map(|s| s.id));
        assert_eq!(at_def.unwrap().name, "target");
    }
}
