//! The browser API.
//!
//! A workspace is handed over as a map of path to text — a repository fetched from
//! GitHub, say — and every question is answered against that map. There is no
//! filesystem, no cache and no thread pool, and none of them are needed: the index is
//! built once from source already in memory, and every analysis after that is reading
//! facts.
//!
//! Everything returns JSON, because the alternative is a wasm-bindgen type for each
//! of twenty answers and the shapes are the ones `--json` already prints.
//!
//! Edits are applied to the in-memory workspace, so a refactoring here is a real edit
//! against real bytes. It changes nothing on GitHub; the diff is the artifact.

use crate::index::Index;
use crate::lang::Language;
use crate::span::{LineCol, LineIndex, Span};
use serde::Serialize;
use std::path::{Path, PathBuf};
use wasm_bindgen::prelude::*;

/// A loaded repository, and the index over it.
#[wasm_bindgen]
pub struct Workspace {
    index: Index,
    /// Paths in the order they were given, so the file list a user sees is stable.
    order: Vec<PathBuf>,
}

#[derive(Serialize)]
struct Failure {
    error: String,
}

fn fail(e: impl std::fmt::Display) -> String {
    serde_json::to_string(&Failure {
        error: e.to_string(),
    })
    .unwrap_or_else(|_| r#"{"error":"unprintable"}"#.to_string())
}

fn ok<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| fail(e))
}

#[wasm_bindgen]
impl Workspace {
    /// Load a repository. `files` is `{ "path/to/file.go": "contents", … }`.
    ///
    /// Files in languages this does not parse are ignored rather than refused: a real
    /// repository is full of lockfiles and images, and refusing the whole load
    /// because of one would be useless.
    #[wasm_bindgen(constructor)]
    pub fn new(files: JsValue) -> Result<Workspace, JsValue> {
        console_error_panic_hook::set_once();
        // The grammars' scanners allocate through a bump allocator that starts at
        // NULL until it is given a region. See the wasm-libc crate.
        fun_refactor_wasm_libc::init_scanner_heap();
        fun_refactor_wasm_libc::use_rust_allocator_in_tree_sitter();

        let map: std::collections::BTreeMap<String, String> = serde_wasm_bindgen::from_value(files)
            .map_err(|e| JsValue::from_str(&format!("expected {{path: text}}: {e}")))?;

        let loaded: Vec<(PathBuf, String)> = map
            .into_iter()
            .map(|(path, text)| (PathBuf::from(path), text))
            .collect();

        // The whole workspace goes into the virtual filesystem first: language
        // detection asks whether a `Chart.yaml` sits beside a YAML file, and that has
        // to be answerable before anything is parsed.
        crate::vfs::load(loaded.clone());

        let mut sources: Vec<(PathBuf, Language, String)> = Vec::new();
        let mut order: Vec<PathBuf> = Vec::new();
        for (path, text) in loaded {
            order.push(path.clone());
            if let Some(language) = crate::lang::detect(&path) {
                sources.push((path, language, text));
            }
        }

        let index = Index::build_from_sources(&sources)
            .map_err(|e| JsValue::from_str(&format!("indexing failed: {e}")))?;
        Ok(Workspace { index, order })
    }

    /// Every file loaded, with the language each was recognised as.
    pub fn files(&self) -> String {
        #[derive(Serialize)]
        struct Entry {
            path: String,
            language: Option<String>,
            indexed: bool,
        }
        let entries: Vec<Entry> = self
            .order
            .iter()
            .map(|path| Entry {
                path: path.display().to_string(),
                language: crate::lang::detect(path).map(|l| l.name().to_string()),
                indexed: self.index.file(path).is_some(),
            })
            .collect();
        ok(&entries)
    }

    /// What the index found, as `fr parse --stats` prints it.
    pub fn stats(&self) -> String {
        #[derive(Serialize)]
        struct Stats {
            files: usize,
            symbols: usize,
            references: usize,
            languages: Vec<(String, usize)>,
            unparsed: Vec<String>,
        }
        let mut by_language: std::collections::BTreeMap<&str, usize> = Default::default();
        let mut unparsed = Vec::new();
        for (path, info) in self.index.files() {
            *by_language.entry(info.language.name()).or_default() += 1;
            if info.had_parse_errors {
                unparsed.push(path.display().to_string());
            }
        }
        ok(&Stats {
            files: self.index.files().count(),
            symbols: self.index.symbols.len(),
            references: self.index.references.len(),
            languages: by_language
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            unparsed,
        })
    }

    /// Every definition in a file, for an outline.
    pub fn symbols(&self, path: &str) -> String {
        let path = PathBuf::from(path);
        let Some(info) = self.index.file(&path) else {
            return ok(&Vec::<u8>::new());
        };
        #[derive(Serialize)]
        struct Sym {
            name: String,
            kind: String,
            line: usize,
            col: usize,
            exported: bool,
        }
        let Ok(source) = crate::vfs::read_to_string(&path) else {
            return fail("file is not loaded");
        };
        let lines = LineIndex::new(&source);
        let mut out: Vec<Sym> = info
            .symbols
            .iter()
            .filter_map(|id| self.index.symbol(*id))
            .map(|s| {
                let at = lines.line_col(s.name_span.start, &source);
                Sym {
                    name: s.qualified_name(),
                    kind: s.kind.as_str().to_string(),
                    line: at.line,
                    col: at.col,
                    exported: s.exported,
                }
            })
            .collect();
        out.sort_by_key(|s| (s.line, s.col));
        ok(&out)
    }

    /// Where the symbol at this position is used, with the confidence of each.
    pub fn references(&self, path: &str, line: usize, col: usize) -> String {
        match self.symbol_at(path, line, col) {
            Ok(id) => {
                #[derive(Serialize)]
                struct Ref {
                    path: String,
                    line: usize,
                    col: usize,
                    confidence: String,
                }
                let refs: Vec<Ref> = self
                    .index
                    .references_to(id)
                    .into_iter()
                    .filter_map(|r| {
                        let source = crate::vfs::read_to_string(&r.file).ok()?;
                        let at = LineIndex::new(&source).line_col(r.span.start, &source);
                        Some(Ref {
                            path: r.file.display().to_string(),
                            line: at.line,
                            col: at.col,
                            confidence: r.confidence.as_str().to_string(),
                        })
                    })
                    .collect();
                ok(&refs)
            }
            Err(e) => fail(e),
        }
    }

    /// Where the thing under the cursor is defined.
    pub fn definition(&self, path: &str, line: usize, col: usize) -> String {
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        match crate::navigate::definitions_at(&self.index, Path::new(path), offset) {
            Some(found) => ok(&found),
            None => fail("nothing is defined or referenced at that position"),
        }
    }

    /// Rename the symbol at a position. Returns the edits, and applies them.
    pub fn rename(&mut self, path: &str, line: usize, col: usize, new_name: &str) -> String {
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        match crate::refactor::rename::plan(&self.index, id, new_name) {
            Ok(plan) => self.apply(plan.edits, plan.warnings),
            Err(e) => fail(e),
        }
    }

    /// Code written more than once, compared structurally.
    pub fn duplicates(&self, min_tokens: usize) -> String {
        let options = crate::analysis::duplicates::Options {
            min_tokens,
            ..Default::default()
        };
        match crate::analysis::duplicates::find(&self.index, &options) {
            Ok(classes) => ok(&classes),
            Err(e) => fail(e),
        }
    }

    /// Symbols nothing appears to use.
    pub fn unused(&self) -> String {
        #[derive(Serialize)]
        struct Dead {
            name: String,
            kind: String,
            path: String,
            exported: bool,
        }
        let report = crate::refactor::delete::find_unused(&self.index, &[]);
        let out: Vec<Dead> = report
            .iter()
            .filter_map(|id| self.index.symbol(*id))
            .map(|s| Dead {
                name: s.qualified_name(),
                kind: s.kind.as_str().to_string(),
                path: s.file.display().to_string(),
                exported: s.exported,
            })
            .collect();
        ok(&out)
    }

    /// The current text of a file, which reflects every edit applied so far.
    pub fn read(&self, path: &str) -> String {
        crate::vfs::read_to_string(PathBuf::from(path)).unwrap_or_default()
    }

    // ------------------------------------------------------------- internals

    fn offset(&self, path: &str, line: usize, col: usize) -> Result<usize, String> {
        let source = crate::vfs::read_to_string(PathBuf::from(path)).map_err(|e| e.to_string())?;
        LineIndex::new(&source)
            .offset(LineCol { line, col }, &source)
            .ok_or_else(|| format!("{path}:{line}:{col} is outside the file"))
    }

    fn symbol_at(
        &self,
        path: &str,
        line: usize,
        col: usize,
    ) -> Result<crate::model::SymbolId, String> {
        let offset = self.offset(path, line, col)?;
        let path = PathBuf::from(path);
        // The definition itself, or whatever the reference under the cursor names.
        if let Some(symbol) = self
            .index
            .symbols
            .iter()
            .find(|s| s.file == path && s.name_span.start <= offset && offset < s.name_span.end)
        {
            return Ok(symbol.id);
        }
        let info = self
            .index
            .file(&path)
            .ok_or_else(|| format!("{} is not indexed", path.display()))?;
        info.references
            .iter()
            .map(|i| &self.index.references[*i])
            .find(|r| r.span.start <= offset && offset < r.span.end)
            .and_then(|r| r.target)
            .ok_or_else(|| "nothing to act on at that position".to_string())
    }

    /// Apply an edit set to the in-memory workspace and describe what happened.
    fn apply(
        &mut self,
        edits: crate::edit::EditSet,
        warnings: Vec<crate::refactor::Warning>,
    ) -> String {
        #[derive(Serialize)]
        struct Applied<'a> {
            files: Vec<Changed>,
            /// The sites the refactoring declined to touch, which is the half of the
            /// answer a diff cannot show.
            warnings: &'a [crate::refactor::Warning],
        }
        #[derive(Serialize)]
        struct Changed {
            path: String,
            before: String,
            after: String,
            diff: String,
        }

        let outcomes = match crate::edit::plan(&edits, crate::edit::Validation::ReparseStrict) {
            Ok(outcomes) => outcomes,
            Err(e) => return fail(e),
        };

        let changed: Vec<Changed> = outcomes
            .iter()
            .filter(|o| o.changed())
            .map(|o| Changed {
                path: o.path.display().to_string(),
                before: o.original.clone(),
                after: o.updated.clone(),
                diff: o.unified_diff(),
            })
            .collect();

        if let Err(e) = crate::edit::commit(&outcomes) {
            return fail(e);
        }
        // The edit changed the bytes every span was measured against, so the index is
        // rebuilt rather than patched. A browser workspace is small enough that this
        // is cheaper than being clever, and being clever here is how spans go stale.
        if let Err(e) = self.reindex() {
            return fail(e);
        }

        ok(&Applied {
            files: changed,
            warnings: &warnings,
        })
    }

    fn reindex(&mut self) -> anyhow::Result<()> {
        let mut sources = Vec::new();
        for path in &self.order {
            let Some(language) = crate::lang::detect(path) else {
                continue;
            };
            let Ok(text) = crate::vfs::read_to_string(path) else {
                continue;
            };
            sources.push((path.clone(), language, text));
        }
        self.index = Index::build_from_sources(&sources)?;
        Ok(())
    }
}

/// The span of a symbol's name, for the editor to select.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Not part of the API; keeps `Span` referenced so the import is not dead in builds
/// where every consumer is behind a feature.
#[allow(dead_code)]
fn _span_is_used(s: Span) -> usize {
    s.len()
}
