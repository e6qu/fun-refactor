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
    /// Why this file's facts are incomplete, empty when they are not. Resolutions from
    /// a file with any gap are suspect.
    pub gaps: Vec<FactGap>,
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

/// Parse one file and extract everything the index keeps about it.
///
/// Depends only on this file's bytes, which is what makes the result reusable: an
/// edit elsewhere cannot change it.
///
/// The parsers and the extractor are passed in rather than made here because
/// [`Extractor::new`] compiles the whole query set. Building one per file turned a
/// 2.9-second index of `zod` into a 19-second one — the same mistake the parallel
/// path avoids by noting that "query compilation is the cost here and it is paid once
/// per thread, not once per file".
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
    ///
    /// Parsing and extraction dominate indexing cost and depend only on a file's bytes
    /// and the query set, so an unchanged file need never be looked at twice.
    #[cfg(feature = "cli")]
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
                let source = match crate::vfs::read_to_string(&file.path) {
                    Ok(s) => s,
                    // Reported by the caller once results are merged.
                    Err(e) => {
                        return Ok(Some((
                            position,
                            Self::unreadable_placeholder(&file.path, e.to_string()),
                        )))
                    }
                };

                // A cached entry carries its own gaps, so a hit skips parsing entirely
                // rather than reparsing to ask.
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
                let facts = extractor
                    .extract(&parsed, &file.path, &source)
                    .with_context(|| format!("extracting facts from {}", file.path.display()))?;

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
            index.add_file(facts, file.language);
        }

        index.resolve();
        Ok(index)
    }

    /// Placeholder facts marking a file that could not be read.
    ///
    /// Carrying the failure through the parallel stage keeps the reporting in one
    /// place instead of needing a second channel out of the worker. Only the scanning
    /// path can meet an unreadable file; sources handed over in memory are already
    /// read.
    #[cfg(feature = "cli")]
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
        let mut extracted = Vec::with_capacity(sources.len());
        for (path, language, source) in sources {
            let facts = extract_facts(&parsers, &mut extractor, path, *language, source)?;
            extracted.push((path.clone(), *language, facts));
        }
        Ok(Self::build_from_facts(&extracted))
    }

    /// Build an index from facts that have already been extracted.
    ///
    /// Extraction is per-file and depends only on a file's bytes; resolution is
    /// global and depends on all of them. Separating the two is what lets a caller
    /// that changed one file out of four hundred re-extract one file and re-resolve,
    /// rather than parse the other three hundred and ninety-nine again. The on-disk
    /// fact cache splits the work the same way for the same reason.
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

    /// Resolve a reference, and cap what the answer is allowed to claim.
    ///
    /// [`Confidence::Exact`] means "safe to edit" and [`Confidence::FieldBased`] means
    /// "matched by member name without knowing the receiver's type". Which of those a
    /// member access on an ordinary value deserves is not a per-branch judgement: it is
    /// the definition of the tier, and it holds however the branch below arrived at an
    /// answer.
    ///
    /// It is enforced here rather than in the branches because there are twenty-eight
    /// places pairing a symbol with a tier, and each one was an opportunity to disagree.
    /// Two of them did, with the same reasoning — one definition of the name means
    /// "there is nothing to be wrong about" — and it is wrong the same way both times:
    /// the workspace does not contain every type. A `boto3` client and an `aws-sdk`
    /// instance both have members this tool has never seen, and `fr rename` edited calls
    /// on them. Fixing the first branch left the second, which is what a rule living at
    /// its use sites does.
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

        // Three receivers whose type is not a guess: the enclosing instance, a module
        // path, and an import binding — all three name something the source declared.
        // Anything else is a value this tool has not typed.
        let known = matches!(receiver, "this" | "self")
            || reference.receiver_is_path
            || self.import_binding(info, receiver).is_some();

        // Weaker of the two: the tiers are ordered strongest first.
        match known {
            true => (target, confidence),
            false => (target, confidence.max(Confidence::FieldBased)),
        }
    }

    fn resolve_by_evidence(
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
        // Was this written as a member of something, and if so, of a value or of a
        // package? An import binding before the dot means a package-qualified call to
        // a function; anything else means a value, so the name is one of its members.
        // A value receiver leaves the type unknown, so only a member can follow it.
        // A path receiver names a type or a module and is handled below, where it can
        // be matched against a symbol's qualifier directly.
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
        // Written as a member of something — either `x.field` or a call on a value.
        let member_access = reference.kind == ReferenceKind::Field || called_on_a_value;
        let plausible = |s: &Symbol| {
            // A candidate in another language is only a candidate where the two
            // languages have a way of naming each other's declarations. Without this
            // a Rust `out.push(…)` resolved to a Zig `Ring.push`.
            if !crate::lang::may_resolve_across(reference.language, s.language, s.kind) {
                return false;
            }
            // A binding is not in scope inside its own initialiser. Go's
            // `templatesDirExists := check(templatesDirExists(p))` calls the package
            // function and then shadows it; Rust's `let x = x + 1` reads the outer
            // `x`; Python's `x = f(x)` reads the previous one. Resolving to the name
            // being declared makes the thing it was declared from look dead.
            if matches!(
                s.kind,
                SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Parameter
            ) && s.file == path
                && s.full_span.contains(reference.span)
                && s.name_span != reference.span
            {
                return false;
            }
            let is_member = matches!(s.kind, SymbolKind::Field | SymbolKind::Method);
            if member_access {
                // `i.provData` names a field of `i`, never the local `provData` two
                // lines up. Without this the nearest-definition rule below binds the
                // access to the local and the field reads as never used.
                is_member
            } else if bare_call {
                !is_member
            } else {
                true
            }
        };

        // 0a. A Terraform namespace. `var.azs`, `local.azs` and `module.azs` name
        //     three different declarations, and an `output "azs"` beside them names a
        //     fourth that no traversal ever reaches. The namespace is written down,
        //     so the kind it implies is not a guess — without it, `var.azs` in
        //     terraform-aws-vpc resolved to the module's `output "azs"`.
        if reference.language == Language::Hcl {
            if let Some(namespace) = reference.receiver.as_deref() {
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
                        .collect();
                    match declared.len() {
                        1 => return (Some(declared[0].id), Confidence::Exact),
                        0 => {}
                        // A `variable "x"` and a `locals { x }` in one module: both
                        // are variables here, and telling them apart needs the block
                        // each was declared in, which this layer does not carry.
                        _ => return (Some(declared[0].id), Confidence::FieldBased),
                    }
                }
            }
        }

        // 0. A path prefix that names a type: `Patterns::from_low_args(…)` in Rust
        //     reaches an associated function through the type it belongs to. The
        //     prefix is recorded as the receiver, and a symbol carries the same name
        //     as its qualifier, so the two match directly — no type inference needed,
        //     because the type was written down.
        //
        //     This runs before every other rule: ripgrep declares four
        //     `from_low_args` methods in one file, so the nearest-in-file rule below
        //     would pick whichever sat closest and leave the other three looking
        //     dead. `Patterns::` already says which.
        if let Some(prefix) = reference
            .receiver
            .as_deref()
            .filter(|_| reference.receiver_is_path)
        {
            let by_qualifier: Vec<&Symbol> = candidates
                .iter()
                .filter_map(|id| self.symbol(*id))
                .filter(|s| s.qualifier.as_deref() == Some(prefix))
                .collect();
            match by_qualifier.len() {
                1 => return (Some(by_qualifier[0].id), Confidence::Exact),
                0 => {}
                // Two types of that name in the workspace; the path says which only
                // if the imports are followed, which this layer does not do.
                _ => return (None, Confidence::FieldBased),
            }

            // `super::` is the module a file's directory forms, `self::` its own.
            // Both put the target beside this file, which is the same shape as a Go
            // package and is resolved the same way.
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
        }

        let scope_chain = self.scope_chain(info, reference.scope);
        let in_file: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.file == path && plausible(s))
            .collect();
        let candidates: Vec<SymbolId> = candidates
            .iter()
            .copied()
            .filter(|id| self.symbol(*id).is_some_and(plausible))
            .collect();
        let candidates = &candidates;

        // A member access is a scope lookup only where the scope chain can settle it.
        // `c.run(1)` names a member of whatever `c` is, and the lexical scope has
        // nothing to say about that — but it was answering anyway, with `Exact`, by
        // picking whichever same-named method sat in an enclosing scope. With two
        // classes declaring `run`, a call on one was attributed to the other, and
        // renaming that one rewrote the call and left the real one behind.
        //
        // Two things keep the scope chain in play, and between them they cover the
        // cases where it is not a guess: a receiver that *is* the enclosing instance —
        // `this.run`, `self.run` mean a member of the type this code is written in —
        // and a name only one member in the workspace has, where there is nothing to
        // be wrong about. Anything else falls through to step 5, which says "either".
        let receiver_is_self = reference
            .receiver
            .as_deref()
            .is_some_and(|r| matches!(r, "this" | "self"));
        let member_candidates = candidates
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| matches!(s.kind, SymbolKind::Field | SymbolKind::Method))
            .count();
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
                // "Preceding" has to be part of the key, not just the distance. Go
                // re-declares a name with `:=` mid-function, and a use above that
                // line reads the earlier binding however close the later one sits:
                //     var ret map[string]any
                //     if m == nil { return ret }      // this one
                //     ret, err := conv(m)             // not this one, 15 bytes nearer
                // Only value bindings are ordered this way; a function may be called
                // above where it is written.
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
            // Proximity is evidence for a binding and not for a callable. `let x`
            // twice in one block is shadowing, and the nearer one is the answer; two
            // methods declared in one class body are an overload set, and the nearer
            // one is a coin flip. Java's `add(int)` beside `add(String)` resolved both
            // bare `add(...)` calls to whichever was written second, at `Exact` — so a
            // rename rewrote calls belonging to the other one.
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

        // 2. Any other definition in the same file (e.g. a function defined below its
        //    use, or a sibling scope for languages that hoist).
        //
        //
        //    Naming a plausible target for a member access is useful — it is the tier
        //    that says how much to trust it, and `resolve_one` caps that for every
        //    branch at once rather than each branch remembering to.
        if in_file.len() == 1 {
            return (Some(in_file[0].id), Confidence::Exact);
        }
        if in_file.len() > 1 && !member_access {
            // Ambiguous within the file: report the nearest but do not claim certainty.
            let nearest = in_file
                .iter()
                .min_by_key(|s| distance(s.name_span.start, reference.span.start))
                .unwrap();
            return (Some(nearest.id), Confidence::NameOnly);
        }
        // For a member access, proximity is not evidence. Two types declaring the
        // same method in one file are equally plausible targets, and picking the one
        // written nearer the call is a coin flip that reads as an answer — it made
        // the other method look dead. Step 5 says "either" instead.

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

        // 4a. A Helm `.Values` path names a key in *this chart's* values file. Two
        //     charts in one workspace routinely declare `image` or `name`, and the
        //     global string-keyed rule below would pick whichever came first. The
        //     receiver is the segment before the key, so `image.tag` does not match a
        //     `tag` declared under something else.
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
                    // A chart declaring one key twice — in values.yaml and a
                    // values-prod.yaml beside it — is normal, and which one wins is
                    // the question `fr flow back` answers with the invocation.
                    _ => return (Some(in_chart[0].id), Confidence::FieldBased),
                }
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

        // 5. Member access without a known receiver type: name-matched at best.
        //
        //    A call written through a selector arrives as a plain `Call`, because
        //    `w.contextWithTimeout(…)` and `time.Now()` are one syntax in Go and the
        //    grammars capture only the callee. What separates them is the receiver:
        //    an import binding means a package-qualified call to a function, and
        //    anything else means a value, so the name is one of its members. Without
        //    that distinction a method call resolves to a package-level function of
        //    the same name, and the method itself reads as dead.
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
                return (None, Confidence::FieldBased);
            }
        }

        // 6. Package-scoped languages. Go's package is the directory: a top-level
        //    declaration in one file is visible, unqualified, from every file beside
        //    it, with no import to record the fact. Only top-level declarations are
        //    in that scope — a method or a struct field is reached through a value,
        //    which step 5 handles.
        //
        //    "Top-level" is `qualifier`, not `container`: a Go method's receiver type
        //    is declared elsewhere, so the method links to no containing symbol and
        //    carries the type's name as its qualifier instead. Testing `container`
        //    would let every method into package scope, which is how `Circle.Area()`
        //    briefly resolved to a bare `Area`.
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
                // A package can declare one name twice, under mutually exclusive
                // build tags — Go's `//go:build windows` and `//go:build !windows`.
                // Which definition a call means depends on the build, so choosing
                // one would rewrite half of a pair and break the other build.
                _ => return (None, Confidence::FieldBased),
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
    /// Every reference to the entity `symbol` names.
    ///
    /// Some kinds are declared in several places and are still one thing: a CSS class
    /// written in a stylesheet and again in a theme. A reference resolves to one of
    /// those sites, so counting per site under-reports — `fr refs` on the second
    /// declaration of `.sensor-table` found nothing while `fr rename` on the same
    /// position changed five sites. You look before you leap, see nothing, and five
    /// things move. `definition_group` is the identity here, and it returns just this
    /// symbol for every kind that has one declaration site.
    pub fn references_to(&self, symbol: SymbolId) -> Vec<&Reference> {
        let group = self.definition_group(symbol);
        self.references
            .iter()
            .filter(|r| r.target.is_some_and(|t| group.contains(&t)))
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
    /// How many files each gap was found in, by [`FactGap::as_str`]. A gap nobody hit
    /// is absent rather than zero, as with `by_confidence`.
    pub files_by_gap: BTreeMap<&'static str, usize>,
    pub imperative_files: usize,
}

fn distance(a: usize, b: usize) -> usize {
    a.abs_diff(b)
}

/// Is this a chart's values file, rather than a manifest that merely has keys?
///
/// `.Values.name` names a key a values file supplies. Every template in the chart is
/// YAML with keys of its own, and matching those would point a reference at whatever
/// manifest happened to use the same word.
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
    fn index_of(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            crate::vfs::write(&path, content).unwrap();
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
        // Visible means a reader can act on it: which file, and why. A skip that says
        // neither is a count, and a count is not a report.
        let (skipped, reason) = &index.skipped[0];
        assert_eq!(skipped, &path);
        assert!(!reason.is_empty(), "the skip gives no reason");
    }

    #[test]
    fn terraform_names_resolve_across_the_module_directory() {
        // Terraform's unit of scope is the directory, so `var.region` in main.tf
        // refers to the `variable "region"` declared in variables.tf. Without this a
        // rename would update the declaration and leave every use dangling.
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
