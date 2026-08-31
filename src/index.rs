//! The workspace index: symbols, references and their resolution.

use crate::extract::Extractor;
use crate::lang::{Language, LanguageClass};
use crate::model::*;
use crate::parse::Parsers;
#[cfg(feature = "cli")]
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
    /// Why this file's facts are incomplete, empty when they are not.
    pub gaps: Vec<FactGap>,
    /// The Kubernetes objects this file declares, which another file addresses by name.
    pub kubernetes_objects: Vec<KubernetesObject>,
    /// Import index by the name it binds, built when resolution runs.
    binding_of: HashMap<String, usize>,
    /// Positions of glob imports, which can bind any name.
    glob_imports: Vec<usize>,
    /// Whether the two lookups above are filled.
    bindings_built: bool,
}

impl FileInfo {
    /// The innermost scope containing `offset`.
    pub fn scope_at(&self, offset: usize) -> Option<crate::model::ScopeId> {
        crate::model::scope_at(&self.scopes, offset)
    }

    /// Walk from `scope` outwards to the file root.
    pub fn scope_chain(&self, scope: crate::model::ScopeId) -> Vec<crate::model::ScopeId> {
        crate::model::scope_chain(&self.scopes, scope)
    }
}

/// Which half of a Terraform module's call surface a reference addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleSurface {
    /// An argument inside the `module` block, naming an input variable.
    Input,
    /// The last segment of `module.<label>.<name>`, naming an output.
    Output,
}

/// A resolved workspace.
#[derive(Debug, Default)]
pub struct Index {
    /// All symbols, keyed by their global [`SymbolId`] (== position in this vec).
    pub symbols: Vec<Symbol>,
    /// All references; `target` holds a global [`SymbolId`].
    pub references: Vec<Reference>,
    files: BTreeMap<PathBuf, FileInfo>,
    /// Files skipped during scanning, reported and not silently dropped.
    pub skipped: Vec<(PathBuf, String)>,
    /// Hash of each file's text as this index read it.
    content_hashes: BTreeMap<PathBuf, u64>,
    /// Symbol ids by name, rebuilt when resolution runs.
    name_buckets: HashMap<String, Vec<SymbolId>>,
    /// Files by their stem, rebuilt with the name buckets.
    files_by_stem: HashMap<String, Vec<PathBuf>>,
    /// `pkg/__init__.py` files by the name of `pkg`, for the same reason.
    inits_by_dir: HashMap<String, Vec<PathBuf>>,
    /// A number no two built indexes share, stamped when the buckets rebuild.
    pub generation: u64,
}

/// The hash [`Index::content_hash`] answers with, for callers comparing fresh text.
pub fn content_hash_of(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Extract every file's facts, in parallel where the build has threads.
#[cfg(feature = "cli")]
fn extract_all(
    sources: &[(PathBuf, Language, String)],
) -> Result<Vec<(PathBuf, Language, FileFacts)>> {
    use rayon::prelude::*;
    sources
        .par_iter()
        .map(|(path, language, source)| {
            thread_local! {
                static WORKER: std::cell::RefCell<(Parsers, Extractor)> =
                    std::cell::RefCell::new((Parsers::new(), Extractor::new()));
            }
            let facts = WORKER.with(|worker| {
                let (parsers, extractor) = &mut *worker.borrow_mut();
                extract_facts(parsers, extractor, path, *language, source)
            })?;
            Ok((path.clone(), *language, facts))
        })
        .collect()
}

#[cfg(not(feature = "cli"))]
fn extract_all(
    sources: &[(PathBuf, Language, String)],
) -> Result<Vec<(PathBuf, Language, FileFacts)>> {
    let parsers = Parsers::new();
    let mut extractor = Extractor::new();
    sources
        .iter()
        .map(|(path, language, source)| {
            let facts = extract_facts(&parsers, &mut extractor, path, *language, source)?;
            Ok((path.clone(), *language, facts))
        })
        .collect()
}

/// Parse one file and extract everything the index keeps about it.
pub fn extract_facts(
    parsers: &Parsers,
    extractor: &mut Extractor,
    path: &Path,
    language: Language,
    source: &str,
) -> Result<FileFacts> {
    let parsed = parsers.parse(language, source)?;
    extractor
        .extract(&parsed, path, source)
        .with_context(|| format!("extracting facts from {}", path.display()))
}

impl Index {
    /// Build an index for a workspace root.
    #[cfg(feature = "cli")]
    pub fn build(root: &Path, options: &ScanOptions) -> Result<Self> {
        let scan_result = scan(root, options)?;
        Self::build_from_scan(&scan_result)
    }

    #[cfg(feature = "cli")]
    pub fn build_from_scan(scan_result: &ScanResult) -> Result<Self> {
        Self::build_with_cache(scan_result, crate::cache::Cache::open().as_ref())
    }

    /// Build an index, reusing previously extracted facts where the content matches.
    #[cfg(feature = "cli")]
    pub fn build_with_cache(
        scan_result: &ScanResult,
        cache: Option<&crate::cache::Cache>,
    ) -> Result<Self> {
        Self::build_with_cache_reporting(scan_result, cache, None)
    }

    /// [`Self::build_with_cache`], telling `progress` how far extraction has come.
    #[cfg(feature = "cli")]
    pub fn build_with_cache_reporting(
        scan_result: &ScanResult,
        cache: Option<&crate::cache::Cache>,
        progress: Option<&(dyn Fn(usize, usize) + Sync)>,
    ) -> Result<Self> {
        use rayon::prelude::*;

        let mut index = Index::default();
        for (path, size) in &scan_result.skipped_too_large {
            index
                .skipped
                .push((path.clone(), format!("exceeds size limit ({size} bytes)")));
        }

        // Extraction is per-file and shares nothing, so it parallelises cleanly.
        let total = scan_result.files.len();
        let done = std::sync::atomic::AtomicUsize::new(0);
        type Extracted = (usize, FileFacts, Option<u64>);
        let extracted: Vec<Result<Option<Extracted>>> = scan_result
            .files
            .par_iter()
            .enumerate()
            .map(|(position, file)| {
                let outcome = (|| {
                    let source = match crate::vfs::read_to_string(&file.path) {
                        Ok(s) => s,
                        Err(e) => {
                            return Ok(Some((
                                position,
                                Self::unreadable_placeholder(&file.path, e.to_string()),
                                None,
                            )))
                        }
                    };
                    let hash = Some(content_hash_of(&source));

                    // A cached entry carries its own gaps, so a hit skips parsing entirely
                    // instead of reparsing to ask.
                    if let Some(cache) = cache {
                        let key = crate::cache::Cache::key(file.language, &source);
                        if let Some(facts) = cache.get(&key, &file.path) {
                            return Ok(Some((position, facts, hash)));
                        }
                    }

                    // Parsers and compiled queries do not cross threads, so each worker keeps
                    // its own in a thread-local.
                    thread_local! {
                        static WORKER: std::cell::RefCell<(Parsers, Extractor)> =
                            std::cell::RefCell::new((Parsers::new(), Extractor::new()));
                    }
                    let facts = WORKER.with(|worker| {
                        let (parsers, extractor) = &mut *worker.borrow_mut();
                        let parsed = parsers.parse(file.language, &source)?;
                        extractor
                            .extract(&parsed, &file.path, &source)
                            .with_context(|| {
                                format!("extracting facts from {}", file.path.display())
                            })
                    })?;

                    if let Some(cache) = cache {
                        let key = crate::cache::Cache::key(file.language, &source);
                        cache.put(&key, &facts);
                    }
                    Ok(Some((position, facts, hash)))
                })();
                if let Some(report) = progress {
                    let counted = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    report(counted, total);
                }
                outcome
            })
            .collect();

        let mut ordered: Vec<Extracted> = Vec::with_capacity(extracted.len());
        for outcome in extracted {
            if let Some(entry) = outcome? {
                ordered.push(entry);
            }
        }
        ordered.sort_by_key(|(position, _, _)| *position);

        for (position, facts, hash) in ordered {
            let file = &scan_result.files[position];
            if let Some(reason) = facts.unreadable.clone() {
                index.skipped.push((file.path.clone(), reason));
                continue;
            }
            if let Some(hash) = hash {
                index.content_hashes.insert(file.path.clone(), hash);
            }
            index.add_file(facts, file.language);
        }

        // Resolution is a pure function of the merged facts and costs most of a warm command.
        let workspace_key = cache.map(|_| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            for (path, info) in &index.files {
                hasher.update(path.to_string_lossy().as_bytes());
                hasher.update([0]);
                hasher.update(info.language.name().as_bytes());
                hasher.update([0]);
                let hash = index.content_hashes.get(path).copied().unwrap_or(0);
                hasher.update(hash.to_le_bytes());
                hasher.update([1]);
            }
            let digest = hasher.finalize();
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            format!("resolved-{hex}")
        });
        let cached = workspace_key.as_ref().and_then(|key| {
            cache?
                .get_resolutions(key)
                .filter(|r| r.len() == index.references.len())
        });
        match cached {
            Some(resolutions) => {
                index.rebuild_name_buckets();
                for (reference, (target, confidence)) in
                    index.references.iter_mut().zip(resolutions)
                {
                    reference.target = target;
                    reference.confidence = confidence;
                }
            }
            None => {
                index.resolve();
                if let (Some(cache), Some(key)) = (cache, workspace_key.as_ref()) {
                    let snapshot: Vec<(Option<SymbolId>, Confidence)> = index
                        .references
                        .iter()
                        .map(|r| (r.target, r.confidence))
                        .collect();
                    cache.put_resolutions(key, &snapshot);
                }
            }
        }
        Ok(index)
    }

    /// Placeholder facts marking a file that failed to read.
    #[cfg(feature = "cli")]
    fn unreadable_placeholder(path: &Path, error: String) -> FileFacts {
        FileFacts {
            path: path.to_path_buf(),
            unreadable: Some(error),
            ..Default::default()
        }
    }

    /// Build an index from sources held in memory and not read from disk.
    pub fn build_from_sources(sources: &[(PathBuf, Language, String)]) -> Result<Self> {
        let extracted = extract_all(sources)?;
        let mut index = Self::build_from_facts(&extracted);
        for (path, _, source) in sources {
            index
                .content_hashes
                .insert(path.clone(), content_hash_of(source));
        }
        Ok(index)
    }

    /// Build an index from facts extraction has already produced.
    pub fn build_from_facts(files: &[(PathBuf, Language, FileFacts)]) -> Self {
        let mut index = Index::default();
        for (_, language, facts) in files {
            index.add_file(facts.clone(), *language);
        }
        index.resolve();
        index
    }

    /// Merge one file's facts, remapping file-local ids into the global namespace.
    pub fn add_file(&mut self, facts: FileFacts, language: Language) {
        crate::capabilities::record(crate::capabilities::Capability::Symbols, language);
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
                gaps: facts.gaps,
                kubernetes_objects: facts.kubernetes_objects,
                binding_of: HashMap::new(),
                glob_imports: Vec::new(),
                bindings_built: false,
            },
        );
    }

    pub fn file(&self, path: &Path) -> Option<&FileInfo> {
        self.files.get(path)
    }

    /// The hash of `path`'s text as this index read it, if this index read it.
    pub fn content_hash(&self, path: &Path) -> Option<u64> {
        self.content_hashes.get(path).copied()
    }

    /// Record the text hash a caller built this index from.
    pub fn note_content_hash(&mut self, path: PathBuf, hash: u64) {
        self.content_hashes.insert(path, hash);
    }

    pub fn files(&self) -> impl Iterator<Item = (&PathBuf, &FileInfo)> {
        self.files.iter()
    }

    /// Files the grammar could not read in full, so their facts are partial.
    pub fn unparsed(&self) -> impl Iterator<Item = &PathBuf> {
        self.files
            .iter()
            .filter(|(_, info)| info.gaps.contains(&FactGap::SyntaxErrors))
            .map(|(path, _)| path)
    }

    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.0 as usize)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Fill the by-name buckets [`Index::definition_group`] reads.
    fn rebuild_name_buckets(&mut self) {
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.name_buckets.clear();
        for symbol in &self.symbols {
            self.name_buckets
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol.id);
        }
        self.files_by_stem.clear();
        self.inits_by_dir.clear();
        for info in self.files.values_mut() {
            info.binding_of.clear();
            info.glob_imports.clear();
            for (at, import) in info.imports.iter().enumerate() {
                if import.is_glob {
                    info.glob_imports.push(at);
                }
                let mut bind = |name: &str| {
                    info.binding_of.entry(name.to_string()).or_insert(at);
                };
                if let Some(alias) = import.alias.as_deref() {
                    bind(alias);
                }
                for n in &import.names {
                    bind(&n.local);
                }
                if let Some(last) = import.path.rsplit(['/', ':', '.']).find(|s| !s.is_empty()) {
                    bind(last);
                }
                bind(&import.path);
            }
            info.bindings_built = true;
        }
        for path in self.files.keys() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                self.files_by_stem
                    .entry(stem.to_string())
                    .or_default()
                    .push(path.clone());
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("__init__.py") {
                if let Some(dir) = path
                    .parent()
                    .and_then(|d| d.file_name())
                    .and_then(|n| n.to_str())
                {
                    self.inits_by_dir
                        .entry(dir.to_string())
                        .or_default()
                        .push(path.clone());
                }
            }
        }
    }

    /// The symbols sharing this name, from the buckets where they exist.
    fn named_like<'a>(&'a self, name: &str) -> Box<dyn Iterator<Item = &'a Symbol> + 'a> {
        match self.name_buckets.get(name) {
            Some(ids) => Box::new(ids.iter().filter_map(|id| self.symbol(*id))),
            // A caller mutated symbols after resolution; correctness over speed.
            None => Box::new(self.symbols.iter().filter(move |_| true)),
        }
    }

    fn resolve(&mut self) {
        self.rebuild_name_buckets();
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

    /// Which Terraform namespace addresses this declaration.
    fn terraform_namespace(&self, symbol: &Symbol) -> &'static str {
        match symbol.container.and_then(|c| self.symbol(c)) {
            Some(block) if block.name == "locals" => "local",
            _ => "var",
        }
    }

    /// Resolve a name reached through a `module` block to the declaration it names.
    fn resolve_module_surface(
        &self,
        path: &Path,
        info: &FileInfo,
        label: &str,
        surface: ModuleSurface,
        candidates: &[SymbolId],
    ) -> (Option<SymbolId>, Confidence) {
        let source = info
            .imports
            .iter()
            .find(|import| import.alias.as_deref() == Some(label))
            .map(|import| import.path.as_str());
        let Some(directory) = source
            .filter(|source| source.starts_with('.'))
            .and_then(|source| {
                path.parent()
                    .map(|dir| crate::vfs::normalise(dir.join(source)))
            })
        else {
            return (None, Confidence::NameOnly);
        };
        let declared: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.language == Language::Hcl)
            .filter(|s| s.file.parent() == Some(directory.as_path()))
            .filter(|s| match surface {
                ModuleSurface::Input => {
                    s.kind == SymbolKind::Variable && self.terraform_namespace(s) == "var"
                }
                // The block-type keyword is the symbol's qualifier, recorded at extraction.
                ModuleSurface::Output => s.qualifier.as_deref() == Some("output"),
            })
            .collect();
        match declared.as_slice() {
            [only] => (Some(only.id), Confidence::Exact),
            _ => (None, Confidence::NameOnly),
        }
    }

    /// Resolve a reference, and cap what the answer is allowed to claim.
    fn resolve_one(
        &self,
        reference: &Reference,
        path: &Path,
        info: &FileInfo,
        by_name: &HashMap<&str, Vec<SymbolId>>,
    ) -> (Option<SymbolId>, Confidence) {
        let (target, confidence) = self.resolve_by_evidence(reference, path, info, by_name);

        // Only a member access asks a question about a receiver.
        let Some(receiver) = reference
            .receiver
            .as_deref()
            .filter(|_| matches!(reference.kind, ReferenceKind::Field | ReferenceKind::Call))
        else {
            return (target, confidence);
        };

        // Four receivers whose type is not a guess: the enclosing instance, a module path, an
        // import binding, and a type's own name.
        let known = matches!(receiver, "this" | "self")
            || reference.receiver_is_path
            || self.import_binding(info, receiver).is_some()
            || self.names_a_type(receiver, reference.language)
            // `module.<label>` is a module path in the sense above: that module binds the label
            // block's `source`, which names a directory.
            || (reference.language == Language::Hcl
                && receiver.starts_with("module.")
                && self
                    .import_binding(info, receiver.trim_start_matches("module."))
                    .is_some());

        // Weaker of the two: the tiers run strongest first.
        match known {
            true => (target, confidence),
            false => (target, confidence.max(Confidence::FieldBased)),
        }
    }

    /// Whether a name is one this workspace declares a type under.
    pub(crate) fn names_a_type(&self, name: &str, language: Language) -> bool {
        self.named_like(name).any(|s| {
            s.name == name
                && s.language == language
                && matches!(
                    s.kind,
                    SymbolKind::Class
                        | SymbolKind::Struct
                        | SymbolKind::Interface
                        | SymbolKind::Enum
                        | SymbolKind::Trait
                        | SymbolKind::TypeAlias
                )
        })
    }

    fn resolve_by_evidence(
        &self,
        reference: &Reference,
        path: &Path,
        info: &FileInfo,
        by_name: &HashMap<&str, Vec<SymbolId>>,
    ) -> (Option<SymbolId>, Confidence) {
        let candidates = match by_name.get(reference.name.as_str()) {
            Some(c) => c.as_slice(),
            // A string-keyed reference need not name its target verbatim: `#two-words` is the
            // anchor of a heading written `Two Words`.
            None if reference.kind == ReferenceKind::StringRef => &[],
            None if self.import_binding(info, reference.name.as_str()).is_some() => &[],
            None => return (None, Confidence::NameOnly),
        };

        // 1.
        let called_on_a_value = reference.kind == ReferenceKind::Call
            && !reference.receiver_is_path
            && reference
                .receiver
                .as_deref()
                .is_some_and(|r| self.import_binding(info, r).is_none());
        // A member reached through no receiver at all is not a member here.
        let bare_call = reference.kind == ReferenceKind::Call
            && reference.receiver.is_none()
            && reference.language.members_always_have_a_receiver();
        // Written as a member of something: `x.field`, a call on a value, or a dotted name
        // inside a macro.
        let member_access = reference.kind == ReferenceKind::Field
            || called_on_a_value
            || reference.member_in_macro;
        // Written inside an `import` or `use` statement, which names what a module
        // exports.
        let in_an_import = info
            .imports
            .iter()
            .any(|import| import.span.contains(reference.span));
        let own_scopes = self.scope_chain(info, reference.scope);
        let plausible = |s: &Symbol| {
            // A candidate in another language is only a candidate where the two languages have
            // a way of naming each other's declarations.
            if !crate::lang::may_resolve_across(reference.language, s.language, s.kind) {
                return false;
            }
            // A binding is not in scope inside its own initialiser.
            if matches!(
                s.kind,
                SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter
            ) && s.file == path
                && s.full_span.contains(reference.span)
                && s.name_span != reference.span
            {
                return false;
            }
            // A local is not usable outside its own scope, in any language here.
            if matches!(
                s.kind,
                SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter
            ) && s.scope != crate::model::ScopeId(0)
                && (s.file != path || !own_scopes.contains(&s.scope))
            {
                return false;
            }
            // Markup writes the namespace down: `class="x"` names a class and `href="#x"` names
            // an element id.
            if let Some(expected) = reference.expects {
                if s.kind != expected {
                    return false;
                }
            }
            let is_member = matches!(s.kind, SymbolKind::Field | SymbolKind::Method);
            if in_an_import {
                // An import path names an item that a module exports.
                return !is_member;
            }
            if member_access {
                // `i.provData` names a field of `i`, never the local `provData` two lines up.
                is_member
            } else if bare_call {
                !is_member
            } else if reference.receiver.is_none()
                && reference.kind == ReferenceKind::Identifier
                && reference.language.members_always_have_a_receiver()
            {
                // The mirror of the first rule: a bare `count` in Python or Rust never names a
                // member, only `self.count` does.
                !is_member || (s.kind == SymbolKind::Method && s.scope == reference.scope)
            } else {
                true
            }
        };

        // 0a.
        if reference.language == Language::Hcl {
            if let Some(namespace) = reference.receiver.as_deref() {
                // A name reached through one `module` block.
                if let Some(label) = namespace.strip_prefix("module.") {
                    let surface = match reference.kind {
                        ReferenceKind::Field => ModuleSurface::Output,
                        _ => ModuleSurface::Input,
                    };
                    return self.resolve_module_surface(path, info, label, surface, candidates);
                }
                let wanted = match namespace {
                    "var" | "local" => Some(SymbolKind::Variable),
                    "module" => Some(SymbolKind::Module),
                    _ => None,
                };
                if let Some(kind) = wanted {
                    let dir = path.parent();
                    let declared: Vec<&Symbol> = candidates
                        .iter()
                        .filter_map(|id| self.symbol(*id))
                        .filter(|s| s.kind == kind && s.file.parent() == dir)
                        // A `variable "x"` and a `locals { x }` in one module are both
                        // variables.
                        .filter(|s| {
                            !matches!(namespace, "var" | "local")
                                || self.terraform_namespace(s) == namespace
                        })
                        .collect();
                    match declared.len() {
                        1 => return (Some(declared[0].id), Confidence::Exact),
                        0 => {}
                        // Two declarations in one namespace, in one module.
                        _ => return (Some(declared[0].id), Confidence::FieldBased),
                    }
                }
            }
        }

        // 0.
        if let Some(prefix) = reference.receiver.as_deref().filter(|receiver| {
            reference.receiver_is_path || self.names_a_type(receiver, reference.language)
        }) {
            let by_qualifier: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                // The language matters too.
                .filter(|s| {
                    s.language == reference.language && s.qualifier.as_deref() == Some(prefix)
                })
                .collect();
            match by_qualifier.len() {
                1 => return (Some(by_qualifier[0].id), Confidence::Exact),
                0 => {}
                // Two types of that name in the workspace; the path says which only to a reader
                // that follows the imports, which this layer does not do.
                _ => return (None, Confidence::FieldBased),
            }

            // `super::` is the module a file's directory forms, `self::` its own.
            if matches!(prefix, "super" | "self") {
                let dir = path.parent();
                let siblings: Vec<&Symbol> = candidates
                    .iter()
                    .filter_map(|id| self.symbol(*id))
                    .filter(|s| {
                        s.language == reference.language
                            && s.qualifier.is_none()
                            && s.file.parent() == dir
                    })
                    .collect();
                match siblings.len() {
                    1 => return (Some(siblings[0].id), Confidence::Exact),
                    0 => {}
                    _ => return (None, Confidence::FieldBased),
                }
            }

            // `fun_refactor::model::anchor_slug` writes everything down, and it resolved
            // name-only: no rule read the path.
            let segment = prefix.rsplit("::").next().unwrap_or(prefix);
            let in_that_module: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| {
                    s.language == reference.language
                        && s.qualifier.is_none()
                        && s.is_top_level()
                        && (s.file.file_stem().is_some_and(|stem| stem == segment)
                            || (s
                                .file
                                .file_stem()
                                .is_some_and(|stem| stem == "mod" || stem == "lib")
                                && s.file
                                    .parent()
                                    .and_then(|d| d.file_name())
                                    .is_some_and(|dir| dir == segment)))
                })
                .collect();
            match in_that_module.len() {
                1 => return (Some(in_that_module[0].id), Confidence::ImportQualified),
                0 => {}
                _ => return (None, Confidence::FieldBased),
            }
        }

        // 0b.
        let bound_files = reference
            .receiver
            .as_deref()
            .map(|receiver| self.modules_bound_to(info, path, receiver))
            .unwrap_or_default();
        if !bound_files.is_empty() {
            let over_there: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| bound_files.contains(&s.file) && s.is_top_level())
                .collect();
            match over_there.len() {
                1 => return (Some(over_there[0].id), Confidence::ImportQualified),
                0 => {}
                _ => return (None, Confidence::FieldBased),
            }
        }

        let scope_chain = own_scopes.clone();
        let candidates: Vec<SymbolId> = candidates
            .iter()
            .copied()
            .filter(|id| self.symbol(*id).is_some_and(plausible))
            .collect();

        // A field and a method may share one name, and the use site's syntax says which is
        // meant: `order.name()` is the method, `order.name` the field.
        let candidates: Vec<SymbolId> = {
            let preferred = match reference.kind {
                ReferenceKind::Call => Some(SymbolKind::Method),
                ReferenceKind::Field => Some(SymbolKind::Field),
                _ => None,
            };
            let kind_of = |id: &SymbolId| self.symbol(*id).map(|s| s.kind);
            let has_both = candidates
                .iter()
                .any(|id| kind_of(id) == Some(SymbolKind::Field))
                && candidates
                    .iter()
                    .any(|id| kind_of(id) == Some(SymbolKind::Method));
            match (member_access, preferred, has_both) {
                (true, Some(kind), true) => candidates
                    .into_iter()
                    .filter(|id| match kind_of(id) {
                        Some(SymbolKind::Field) | Some(SymbolKind::Method) => {
                            kind_of(id) == Some(kind)
                        }
                        _ => true,
                    })
                    .collect(),
                _ => candidates,
            }
        };
        let candidates = &candidates;
        // Derived after the field-or-method split above, so the scope lookup cannot
        // resurrect the member the syntax ruled out.
        let in_file: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.file == path)
            .collect();

        // The enclosing instance.
        if member_access
            && reference
                .receiver
                .as_deref()
                .is_some_and(|r| matches!(r, "this" | "self"))
        {
            let owner = info
                .symbols
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| s.full_span.contains(reference.span) && s.qualifier.is_some())
                .min_by_key(|s| s.full_span.end - s.full_span.start)
                .and_then(|s| s.qualifier.clone());
            if let Some(owner) = owner {
                let member = in_file
                    .iter()
                    .filter(|s| matches!(s.kind, SymbolKind::Field | SymbolKind::Method))
                    .find(|s| s.qualifier.as_deref() == Some(owner.as_str()));
                if let Some(member) = member {
                    return (Some(member.id), Confidence::Exact);
                }
            }
        }

        // A member access is a scope lookup only where the scope chain can settle it.
        let receiver_is_self = reference
            .receiver
            .as_deref()
            .is_some_and(|r| matches!(r, "this" | "self"));
        // Two members that are one definition group are one candidate: a property's getter and
        // setter, an overload set.
        let member_candidates = {
            let members: Vec<SymbolId> = candidates
                .iter()
                .copied()
                .filter(|id| {
                    self.symbol(*id)
                        .is_some_and(|s| matches!(s.kind, SymbolKind::Field | SymbolKind::Method))
                })
                .collect();
            self.count_entities(&members)
        };
        let scope_can_settle_it = !member_access || receiver_is_self || member_candidates <= 1;
        let scoped = in_file
            .iter()
            .filter(|_| scope_can_settle_it)
            .filter(|s| scope_chain.contains(&s.scope))
            .min_by_key(|s| {
                // Innermost enclosing scope wins; ties break toward the nearest
                // preceding definition, which is how shadowing reads.
                let depth = scope_chain
                    .iter()
                    .position(|sc| *sc == s.scope)
                    .unwrap_or(usize::MAX);
                // "Preceding" belongs in the key, alongside the distance.
                let ordered = matches!(
                    s.kind,
                    SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter
                );
                let declared_after = ordered && s.name_span.start > reference.span.start;
                (
                    depth,
                    declared_after,
                    distance(s.name_span.start, reference.span.start),
                )
            });
        if let Some(symbol) = scoped {
            // Proximity is evidence for a binding and not for a callable.
            let callable = matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method);
            let tied = in_file
                .iter()
                .filter(|s| {
                    s.id != symbol.id
                        && s.scope == symbol.scope
                        && s.container == symbol.container
                        && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                })
                .count();
            if !(callable && tied > 0) {
                return (Some(symbol.id), Confidence::Exact);
            }
        }

        // 2.
        if in_file.len() == 1 {
            return (Some(in_file[0].id), Confidence::Exact);
        }
        if in_file.len() > 1 {
            let ids: Vec<SymbolId> = in_file.iter().map(|s| s.id).collect();
            if self.count_entities(&ids) == 1 {
                return (Some(ids[0]), Confidence::Exact);
            }
        }
        if in_file.len() > 1 && !member_access {
            // Ambiguous within the file: report the nearest but do not claim certainty.
            let nearest = in_file
                .iter()
                .min_by_key(|s| distance(s.name_span.start, reference.span.start))
                .unwrap();
            return (Some(nearest.id), Confidence::NameOnly);
        }
        // For a member access, proximity is not evidence.

        // 2b.
        if reference.language.splices_sourced_files() {
            let sourced: Vec<PathBuf> = info
                .imports
                .iter()
                .filter_map(|import| self.resolve_import_path(path, &import.path))
                .collect();
            let over_there: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| sourced.contains(&s.file) && s.is_top_level())
                .collect();
            match over_there.len() {
                1 => return (Some(over_there[0].id), Confidence::ImportQualified),
                0 => {}
                _ => return (None, Confidence::FieldBased),
            }
        }

        // 3.
        if let Some(import) = self.import_binding(info, &reference.name) {
            let mut imported_file = self.resolve_import_path(path, &import.path);
            let mut original = import
                .names
                .iter()
                .find(|n| n.local == reference.name)
                .map(|n| n.original.clone())
                .unwrap_or_else(|| reference.name.clone());

            // Candidates are looked up under the name each hop imports, and that name may
            // differ from the one at the use site: `from lib import helper as h2` declares no
            // `h2`, so the definitions to weigh are the ones called `helper`.
            let named = |name: &str| -> Vec<&Symbol> {
                by_name
                    .get(name)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.symbol(*id))
                    .filter(|s| plausible(s))
                    .collect()
            };

            let mut matches: Vec<&Symbol> = named(&original);
            if let Some(target_file) = &imported_file {
                matches.retain(|s| &s.file == target_file);
            }
            matches.retain(|s| s.exported || imported_file.is_some());

            // A barrel declares nothing: `export { width } from "./holder"` exports a name it
            // does not define.
            let mut hops = 0;
            while matches.is_empty() && hops < 8 {
                let Some(current) = imported_file.clone() else {
                    break;
                };
                let Some((onward_path, next_original)) =
                    self.file(&current).and_then(|current_info| {
                        current_info
                            .imports
                            .iter()
                            .filter(|i| i.re_export || current_info.language == Language::Python)
                            .find_map(|i| {
                                i.names
                                    .iter()
                                    .find(|n| n.local == original)
                                    .map(|n| (i.path.clone(), n.original.clone()))
                            })
                            // `export * from "./holder"` names nothing and hands on everything,
                            // so it is the onward hop for any name the file does not export
                            // itself.
                            .or_else(|| {
                                current_info
                                    .imports
                                    .iter()
                                    .find(|i| {
                                        i.re_export && i.names.is_empty() && i.alias.is_none()
                                    })
                                    .map(|i| (i.path.clone(), original.clone()))
                            })
                    })
                else {
                    break;
                };
                imported_file = self.resolve_import_path(&current, &onward_path);
                original = next_original;
                hops += 1;
                let Some(next_file) = &imported_file else {
                    break;
                };
                matches = named(&original);
                matches.retain(|s| &s.file == next_file);
            }

            if matches.len() == 1 {
                return (Some(matches[0].id), Confidence::ImportQualified);
            }
            if !matches.is_empty() {
                // A glob import or an ambiguous path: plausible but unproven.
                return (Some(matches[0].id), Confidence::FieldBased);
            }
        }

        // 4z.
        if let Some((kind, object)) = reference
            .receiver
            .as_deref()
            .filter(|_| matches!(reference.language, Language::Yaml | Language::Helm))
            .and_then(|receiver| receiver.split_once('/'))
        {
            let declaring: Vec<&Path> = self
                .files
                .iter()
                .filter(|(_, info)| {
                    info.kubernetes_objects
                        .iter()
                        .any(|o| o.kind == kind && o.name == object)
                })
                .map(|(path, _)| path.as_path())
                .collect();
            let entries: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| s.kind == SymbolKind::Key)
                .filter(|s| matches!(s.qualifier.as_deref(), Some("data") | Some("stringData")))
                .filter(|s| declaring.contains(&s.file.as_path()))
                .collect();
            return match entries.as_slice() {
                [only] => (Some(only.id), Confidence::Exact),
                // One name declared twice under one object name.
                [first, ..] => (Some(first.id), Confidence::FieldBased),
                // The object is outside the workspace, or declares no such key.
                [] => (None, Confidence::NameOnly),
            };
        }

        // 4a.
        if reference.language == Language::Helm && reference.kind == ReferenceKind::StringRef {
            if let Some(chart) = crate::lang::chart_root(path) {
                let in_chart: Vec<&Symbol> = candidates
                    .iter()
                    .filter_map(|id| self.symbol(*id))
                    .filter(|s| {
                        s.kind == SymbolKind::Key
                            && s.file.starts_with(chart)
                            && is_values_file(&s.file)
                            && s.qualifier.as_deref() == reference.receiver.as_deref()
                    })
                    .collect();
                match in_chart.len() {
                    1 => return (Some(in_chart[0].id), Confidence::Exact),
                    0 => {}
                    // A chart declaring one key twice, in values.yaml and a values-prod.yaml
                    // beside it, is normal.
                    _ => return (Some(in_chart[0].id), Confidence::Exact),
                }
            }
        }

        // 4.
        if reference.kind == ReferenceKind::StringRef {
            // A reference reaches a heading by its anchor, which is a slug of its text and rarely
            // equal to it.
            let name = reference.name.as_str();
            let targets: Vec<&Symbol> = self
                .symbols
                .iter()
                .filter(|s| match reference.expects {
                    // The attribute wrote the namespace down.
                    Some(expected) => s.kind == expected && s.name == name,
                    None => match s.kind {
                        SymbolKind::Heading => anchor_slug(&s.name) == name,
                        kind => kind.is_string_keyed() && s.name == name,
                    },
                })
                .collect();

            let kinds: HashSet<SymbolKind> = targets.iter().map(|s| s.kind).collect();
            match kinds.len() {
                1 => return (Some(targets[0].id), Confidence::Exact),
                0 => {}
                // The same name declared as two different kinds is genuinely ambiguous.
                _ => return (Some(targets[0].id), Confidence::FieldBased),
            }
        }

        // 5.
        let members: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| matches!(s.kind, SymbolKind::Field | SymbolKind::Method))
            .collect();
        if member_access {
            if members.len() == 1 {
                return (Some(members[0].id), Confidence::FieldBased);
            }
            if !members.is_empty() {
                // Several members share the name, and the receiver's settled type says whose
                // member this is.
                if let Some(ty) = crate::refactor::receiver_known_type(self, reference) {
                    let owned: Vec<&&Symbol> = members
                        .iter()
                        .filter(|s| s.qualifier.as_deref() == Some(ty.as_str()))
                        .collect();
                    if let [only] = owned.as_slice() {
                        return (Some(only.id), Confidence::FieldBased);
                    }
                }
                return (None, Confidence::FieldBased);
            }
        }

        // 6.
        if reference.language.packages_by_directory() {
            if let Some(package) = reference
                .receiver
                .as_deref()
                .and_then(|receiver| self.import_binding(info, receiver))
                .and_then(|import| self.package_directory(&import.path))
            {
                let exported_there: Vec<&Symbol> = candidates
                    .iter()
                    .filter_map(|id| self.symbol(*id))
                    .filter(|s| {
                        s.language == reference.language
                            && s.qualifier.is_none()
                            && s.container.is_none()
                            // Go lets one package reach only what another exports, and
                            // spells that as a capital letter.
                            && s.exported
                            && s.file.parent() == Some(package.as_path())
                    })
                    .collect();
                match exported_there.len() {
                    1 => return (Some(exported_there[0].id), Confidence::ImportQualified),
                    0 => {}
                    _ => return (None, Confidence::FieldBased),
                }
            }
        }

        // 6b.
        if reference.language.packages_by_directory() && !called_on_a_value {
            let dir = path.parent();
            let siblings: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| {
                    s.language == reference.language
                        && s.qualifier.is_none()
                        && s.container.is_none()
                        && s.file.parent() == dir
                })
                .collect();
            match siblings.len() {
                1 => return (Some(siblings[0].id), Confidence::Exact),
                0 => {}
                // A package can declare one name twice, under mutually exclusive build tags.
                _ => return (None, Confidence::FieldBased),
            }
        }

        // 6.
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

        // 7.
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
        if info.bindings_built {
            // The earliest of the named binding and any glob, matching the
            // scan's first-hit order.
            let named = info.binding_of.get(name).copied();
            let glob = info.glob_imports.first().copied();
            let at = match (named, glob) {
                (Some(n), Some(g)) => Some(n.min(g)),
                (a, b) => a.or(b),
            }?;
            return info.imports.get(at);
        }
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
                // `import app.flags` binds `app`, and the body reaches it as `app.flags.NAME`.
                || import.path == name
        })
    }

    /// The files a receiver could be, where an import in this file binds it to a module.
    fn modules_bound_to(&self, info: &FileInfo, from: &Path, receiver: &str) -> Vec<PathBuf> {
        let Some(import) = self.import_binding(info, receiver) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = Vec::new();
        if let Some(file) = self.resolve_import_path(from, &import.path) {
            files.push(file);
        }
        if let Some(bound) = import.names.iter().find(|n| n.local == receiver) {
            let separator = match import.path.ends_with('.') {
                true => "",
                false => ".",
            };
            let submodule = format!("{}{separator}{}", import.path, bound.original);
            if let Some(file) = self.resolve_import_path(from, &submodule) {
                files.push(file);
            }
        }

        // A file may hand on what it does not declare: a Sass `@forward "theme"`, a TypeScript
        // `export * from "./holder"`.
        let mut frontier = files.clone();
        for _ in 0..8 {
            let mut onward: Vec<PathBuf> = Vec::new();
            for file in &frontier {
                let Some(info) = self.file(file) else {
                    continue;
                };
                for import in info.imports.iter().filter(|i| i.re_export) {
                    if let Some(next) = self.resolve_import_path(file, &import.path) {
                        if !files.contains(&next) {
                            files.push(next.clone());
                            onward.push(next);
                        }
                    }
                }
            }
            if onward.is_empty() {
                break;
            }
            frontier = onward;
        }

        files.sort();
        files.dedup();
        files
    }

    /// The directory a package import path names, where this workspace holds it.
    fn package_directory(&self, import_path: &str) -> Option<PathBuf> {
        let segments: Vec<&str> = import_path.split('/').filter(|s| !s.is_empty()).collect();
        for start in 0..segments.len() {
            let suffix = segments[start..].join("/");
            let mut matches: Vec<PathBuf> = self
                .files
                .keys()
                .filter_map(|path| path.parent())
                .filter(|dir| dir.ends_with(&suffix))
                .map(|dir| dir.to_path_buf())
                .collect();
            matches.sort();
            matches.dedup();
            match matches.len() {
                1 => return matches.into_iter().next(),
                0 => continue,
                _ => return None,
            }
        }
        None
    }

    /// Map an import path to a file in the workspace.
    pub fn resolve_import_path(&self, from: &Path, import_path: &str) -> Option<PathBuf> {
        if import_path.is_empty() {
            return None;
        }
        let dir = from.parent()?;

        // A path that names a file beside this one, with no `./` in front of it.
        let beside = crate::vfs::normalise(dir.join(import_path));
        if self.files.contains_key(&beside) {
            return Some(beside);
        }

        // A stylesheet names a file without its extension.
        for extension in ["scss", "sass", "css"] {
            let named = beside.with_extension(extension);
            if self.files.contains_key(&named) {
                return Some(named);
            }
            if let Some(stem) = beside.file_name().and_then(|n| n.to_str()) {
                let partial = beside.with_file_name(format!("_{stem}.{extension}"));
                if self.files.contains_key(&partial) {
                    return Some(partial);
                }
            }
        }

        // Python writes a relative import as leading dots and then dotted segments.
        if import_path.starts_with('.') && !import_path.contains('/') {
            let levels = import_path.chars().take_while(|c| *c == '.').count();
            let tail = import_path.trim_start_matches('.');
            let mut base = dir.to_path_buf();
            for _ in 1..levels {
                base = base.parent()?.to_path_buf();
            }
            for segment in tail.split('.').filter(|s| !s.is_empty()) {
                base = base.join(segment);
            }
            let candidates = match tail.is_empty() {
                true => vec![base.join("__init__.py")],
                false => vec![base.with_extension("py"), base.join("__init__.py")],
            };
            if let Some(found) = candidates.into_iter().find(|c| self.files.contains_key(c)) {
                return Some(found);
            }
        }

        if import_path.starts_with('.') || import_path.starts_with('/') {
            let base = crate::vfs::normalise(dir.join(import_path.trim_start_matches("./")));
            let candidates = [
                base.clone(),
                base.with_extension("ts"),
                base.with_extension("tsx"),
                base.with_extension("py"),
                base.with_extension("scss"),
                base.with_extension("sass"),
                base.with_extension("css"),
                base.with_extension("sh"),
                base.join("index.ts"),
                base.join("index.tsx"),
                base.join("__init__.py"),
            ];
            return candidates.into_iter().find(|c| self.files.contains_key(c));
        }

        // `source "$(dirname "$0")/lib.sh"` is how a shell script names a file beside itself,
        // and the prefix is a substitution no static read can evaluate.
        if let Some((_, tail)) = import_path.rsplit_once('/') {
            let beside = crate::vfs::normalise(dir.join(tail));
            if !tail.is_empty() && self.files.contains_key(&beside) {
                return Some(beside);
            }
        }

        // Dotted module paths (Python `pkg.mod`, Rust `crate::mod`) map to a file
        // whose stem matches the final segment, accepted only when unambiguous.
        let last = import_path
            .rsplit(['/', ':', '.'])
            .find(|s| !s.is_empty())?;
        let stem_scan;
        let matches: &[PathBuf] = match self.files_by_stem.is_empty() {
            false => self
                .files_by_stem
                .get(last)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            // A caller resolved before the buckets existed; correctness first.
            true => {
                stem_scan = self
                    .files
                    .keys()
                    .filter(|p| p.file_stem().and_then(|s| s.to_str()) == Some(last))
                    .cloned()
                    .collect::<Vec<_>>();
                &stem_scan
            }
        };
        if matches.len() == 1 {
            return Some(matches[0].clone());
        }
        // Several files share the stem, and the segments before it decide between them.
        if matches.len() > 1 {
            let qualifiers: Vec<&str> = import_path
                .split(['/', ':', '.'])
                .filter(|s| !s.is_empty())
                .collect();
            let qualifiers = &qualifiers[..qualifiers.len().saturating_sub(1)];
            if !qualifiers.is_empty() {
                let qualified: Vec<&PathBuf> = matches
                    .iter()
                    .filter(|file| {
                        let directories: Vec<&str> = file
                            .parent()
                            .map(|dir| {
                                dir.components()
                                    .filter_map(|c| match c {
                                        std::path::Component::Normal(part) => part.to_str(),
                                        _ => None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        directories.ends_with(qualifiers)
                    })
                    .collect();
                if qualified.len() == 1 {
                    return Some(qualified[0].clone());
                }
            }
        }
        // A Python package is a directory, so `pkg` names `pkg/__init__.py`.
        let init_scan;
        let inits: &[PathBuf] = match self.inits_by_dir.is_empty() && self.files_by_stem.is_empty()
        {
            false => self
                .inits_by_dir
                .get(last)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            true => {
                init_scan = self
                    .files
                    .keys()
                    .filter(|p| p.ends_with(Path::new(last).join("__init__.py")))
                    .cloned()
                    .collect::<Vec<_>>();
                &init_scan
            }
        };
        if inits.len() == 1 {
            return Some(inits[0].clone());
        }
        None
    }

    /// All references to the entity `symbol` names.
    pub fn references_to(&self, symbol: SymbolId) -> Vec<&Reference> {
        let group = self.definition_group(symbol);
        self.references
            .iter()
            .filter(|r| r.target.is_some_and(|t| group.contains(&t)))
            .collect()
    }

    /// References that share a name with `symbol` but resolved elsewhere or not at all.
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
    pub fn definition_group(&self, symbol: SymbolId) -> Vec<SymbolId> {
        let Some(sym) = self.symbol(symbol) else {
            return Vec::new();
        };
        if !sym.kind.allows_multiple_definitions() {
            // `self.count = 0` in `__init__` and `self.count = n` in another method are one
            // attribute declared twice, and Python declares attributes only that way.
            if (matches!(sym.language, Language::TypeScript | Language::Tsx)
                && matches!(sym.kind, SymbolKind::Function | SymbolKind::Method))
                || (sym.language == Language::Python && sym.kind == SymbolKind::Method)
            {
                let peers: Vec<SymbolId> = self
                    .named_like(&sym.name)
                    .filter(|s| {
                        s.name == sym.name
                            && s.kind == sym.kind
                            && s.file == sym.file
                            && s.container == sym.container
                    })
                    .map(|s| s.id)
                    .collect();
                if peers.len() > 1 {
                    return peers;
                }
            }
            if sym.language == Language::Python && sym.kind == SymbolKind::Variable {
                let peers: Vec<SymbolId> = self
                    .named_like(&sym.name)
                    .filter(|s| {
                        s.name == sym.name
                            && s.kind == sym.kind
                            && s.file == sym.file
                            && s.scope == sym.scope
                    })
                    .map(|s| s.id)
                    .collect();
                if peers.len() > 1 {
                    return peers;
                }
            }
            // A chart value declared in values.yaml and again in values-prod.yaml is one value
            // with two override layers.
            if sym.kind == SymbolKind::Key
                && matches!(sym.language, Language::Helm | Language::Yaml)
                && is_values_file(&sym.file)
            {
                if let Some(chart) = crate::lang::chart_root(&sym.file) {
                    let layers: Vec<SymbolId> = self
                        .named_like(&sym.name)
                        .filter(|s| {
                            s.name == sym.name
                                && s.kind == SymbolKind::Key
                                && s.qualifier == sym.qualifier
                                && is_values_file(&s.file)
                                && crate::lang::chart_root(&s.file) == Some(chart)
                        })
                        .map(|s| s.id)
                        .collect();
                    if layers.len() > 1 {
                        return layers;
                    }
                }
            }
            if sym.kind == SymbolKind::Field && sym.qualifier.is_some() {
                return self
                    .named_like(&sym.name)
                    .filter(|s| {
                        s.name == sym.name
                            && s.kind == sym.kind
                            && s.qualifier == sym.qualifier
                            && s.file == sym.file
                    })
                    .map(|s| s.id)
                    .collect();
            }
            return vec![symbol];
        }
        // A CSS module's selectors are its own.
        if sym.kind == SymbolKind::Selector && crate::lang::is_css_module(&sym.file) {
            return self
                .named_like(&sym.name)
                .filter(|s| s.name == sym.name && s.kind == sym.kind && s.file == sym.file)
                .map(|s| s.id)
                .collect();
        }
        // A CSS module scopes class names and nothing else. A custom property, an
        // element id and a data attribute group across files as they do anywhere.
        let scoped_to_its_file = sym.kind == SymbolKind::Selector;
        self.named_like(&sym.name)
            .filter(|s| s.name == sym.name && s.kind == sym.kind)
            .filter(|s| !(scoped_to_its_file && crate::lang::is_css_module(&s.file)))
            .map(|s| s.id)
            .collect()
    }

    /// How many distinct entities these symbols denote, with each
    /// definition group counted once.
    pub fn count_entities(&self, ids: &[SymbolId]) -> usize {
        let mut remaining: Vec<SymbolId> = ids.to_vec();
        let mut count = 0usize;
        while let Some(first) = remaining.first().copied() {
            let group = self.definition_group(first);
            remaining.retain(|id| !group.contains(id) && *id != first);
            count += 1;
        }
        count
    }

    /// Whether these symbols all denote the same entity.
    pub fn is_one_entity(&self, symbols: &[&Symbol]) -> bool {
        let Some(first) = symbols.first() else {
            return false;
        };
        if symbols.len() < 2 {
            return false;
        }
        let group = self.definition_group(first.id);
        symbols.iter().all(|s| group.contains(&s.id))
    }

    /// Find a symbol by the name a listing prints: `Type::method`, or a bare leaf.
    pub fn symbols_written(&self, written: &str, in_file: Option<&Path>) -> Vec<&Symbol> {
        if written.contains("::") {
            let qualified: Vec<&Symbol> = self
                .symbols
                .iter()
                .filter(|s| s.qualified_name() == written)
                .filter(|s| in_file.is_none_or(|f| s.file == f))
                .collect();
            if !qualified.is_empty() {
                return qualified;
            }
        }
        self.find_symbols(written, in_file)
    }

    /// Find a symbol by name, optionally narrowed to a file.
    pub fn find_symbols(&self, name: &str, in_file: Option<&Path>) -> Vec<&Symbol> {
        self.named_like(name)
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

    /// Every reference in one file, in the order extraction found them.
    pub fn references_in(&self, path: &Path) -> impl Iterator<Item = &Reference> {
        self.files
            .get(path)
            .into_iter()
            .flat_map(|info| info.references.iter().map(|i| &self.references[*i]))
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
            resolved: self
                .references
                .iter()
                .filter(|r| r.target.is_some())
                .count(),
            by_confidence,
            files_by_gap: self
                .files
                .values()
                .fold(BTreeMap::new(), |mut counts, file| {
                    for gap in &file.gaps {
                        *counts.entry(gap.as_str()).or_default() += 1;
                    }
                    counts
                }),
            imperative_files: self
                .files
                .values()
                .filter(|f| f.language.class() == LanguageClass::Imperative)
                .count(),
        }
    }

    /// Names defined more than once across the workspace, the cases where
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
    /// How many files hold each gap, by [`FactGap::as_str`].
    pub files_by_gap: BTreeMap<&'static str, usize>,
    pub imperative_files: usize,
}

fn distance(a: usize, b: usize) -> usize {
    a.abs_diff(b)
}

/// Whether this path is a chart's values file rather than a manifest with keys.
fn is_values_file(path: &std::path::Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        let lower = n.to_ascii_lowercase();
        lower.starts_with("values") && (lower.ends_with(".yaml") || lower.ends_with(".yml"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::SourceFile;

    /// Build an index from in-memory sources written to a temp workspace.
    use crate::testing::indexed as index_of;

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
        let src =
            "fn f() {\n    let x = 1;\n    {\n        let x = 2;\n        use_it(x);\n    }\n}\n";
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
        // Nothing reads invalid UTF-8 as source.
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let scanned = ScanResult {
            files: vec![SourceFile {
                path: path.clone(),
                language: Language::Rust,
            }],
            ..ScanResult::default()
        };
        let index = Index::build_from_scan(&scanned).unwrap();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.skipped.len(), 1, "skip must be visible");
        // Visible means a reader can act on it: which file, and why.
        let (skipped, reason) = &index.skipped[0];
        assert_eq!(skipped, &path);
        assert!(!reason.is_empty(), "the skip gives no reason");
    }

    #[test]
    fn terraform_names_resolve_across_the_module_directory() {
        // Terraform's unit of scope is the directory, so `var.region` in main.tf refers to the
        // `variable "region"` declared in variables.tf.
        let (_tmp, index) = index_of(&[
            (
                "variables.tf",
                "variable \"region\" {\n  default = \"eu-west-1\"\n}\n",
            ),
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
            (
                "variables.tf",
                "variable \"region\" {\n  default = \"a\"\n}\n",
            ),
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
    fn a_call_through_an_import_alias_resolves_to_the_original() {
        let (_tmp, index) = index_of(&[
            ("lib.py", "def helper():\n    return 2\n"),
            (
                "app.py",
                "from lib import helper as h2\n\ndef run():\n    return h2()\n",
            ),
        ]);
        let helper = index.find_symbols("helper", None);
        assert_eq!(helper.len(), 1, "got {helper:?}");
        let refs = index.references_to(helper[0].id);
        let call = refs
            .iter()
            .find(|r| r.file.ends_with("app.py") && r.kind == ReferenceKind::Call)
            .unwrap_or_else(|| panic!("the aliased call is a reference: {refs:?}"));
        assert_eq!(call.confidence, Confidence::ImportQualified);
    }

    #[test]
    fn an_aliased_python_reexport_chain_resolves_to_the_declaration() {
        // pkg/__init__.py imports `helper` under a new name and app.py imports that name from
        // the package.
        let (_tmp, index) = index_of(&[
            ("lib.py", "def helper():\n    return 2\n"),
            ("pkg/__init__.py", "from lib import helper as help_alias\n"),
            (
                "app.py",
                "from pkg import help_alias\n\ndef run():\n    return help_alias()\n",
            ),
        ]);
        let helper = index.find_symbols("helper", None);
        assert_eq!(helper.len(), 1);
        let refs = index.references_to(helper[0].id);
        assert!(
            refs.iter()
                .any(|r| r.file.ends_with("app.py") && r.kind == ReferenceKind::Call),
            "the call through the re-export chain resolves: {refs:?}"
        );
    }

    #[test]
    fn a_chart_value_declared_in_two_values_files_is_one_entity() {
        let (_tmp, index) = index_of(&[
            ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
            ("c/values.yaml", "replicaCount: 1\n"),
            ("c/values-prod.yaml", "replicaCount: 5\n"),
            (
                "c/templates/deploy.yaml",
                "spec:\n  replicas: {{ .Values.replicaCount }}\n",
            ),
        ]);
        let sites = index.find_symbols("replicaCount", None);
        assert_eq!(sites.len(), 2, "both values files declare it: {sites:?}");
        for site in &sites {
            let group = index.definition_group(site.id);
            assert_eq!(group.len(), 2, "the group holds both sites: {group:?}");
            let refs = index.references_to(site.id);
            assert_eq!(refs.len(), 1, "the template read counts from either site");
            assert_eq!(refs[0].confidence, Confidence::Exact);
        }
        assert!(
            index.is_one_entity(&sites),
            "one value, two override layers"
        );
    }

    #[test]
    fn a_chart_value_never_groups_with_a_neighbouring_chart() {
        let (_tmp, index) = index_of(&[
            ("a/Chart.yaml", "apiVersion: v2\nname: a\nversion: 0.1.0\n"),
            ("a/values.yaml", "replicaCount: 1\n"),
            ("b/Chart.yaml", "apiVersion: v2\nname: b\nversion: 0.1.0\n"),
            ("b/values.yaml", "replicaCount: 9\n"),
        ]);
        let sites = index.find_symbols("replicaCount", None);
        assert_eq!(sites.len(), 2);
        for site in &sites {
            assert_eq!(
                index.definition_group(site.id),
                vec![site.id],
                "chart boundaries hold"
            );
        }
    }

    #[test]
    fn a_nested_chart_value_groups_only_with_the_same_path() {
        // `image.tag` in both values files is one entity; the unrelated top-level
        // `tag` is not part of it.
        let (_tmp, index) = index_of(&[
            ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
            ("c/values.yaml", "tag: loose\nimage:\n  tag: v1\n"),
            ("c/values-prod.yaml", "image:\n  tag: v2\n"),
        ]);
        let nested: Vec<_> = index
            .symbols
            .iter()
            .filter(|s| s.name == "tag" && s.qualifier.as_deref() == Some("image"))
            .collect();
        assert_eq!(nested.len(), 2);
        let group = index.definition_group(nested[0].id);
        assert_eq!(group.len(), 2, "got {group:?}");
        let loose = index
            .symbols
            .iter()
            .find(|s| s.name == "tag" && s.qualifier.is_none())
            .unwrap();
        assert!(
            !group.contains(&loose.id),
            "the loose key is another entity"
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
