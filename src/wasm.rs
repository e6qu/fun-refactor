//! The browser API.

use crate::index::Index;
use crate::lang::Language;
use crate::model::FactGap;
use crate::span::{LineCol, LineIndex, Span};
use serde::Serialize;
use std::path::{Path, PathBuf};
use wasm_bindgen::prelude::*;

/// A loaded repository, and the index over it.
#[wasm_bindgen]
pub struct Workspace {
    index: Index,
    /// This workspace's bytes.
    files: crate::vfs::Handle,
    /// The last file this parsed, and the text behind it.
    parsed: std::cell::RefCell<Option<(PathBuf, String, crate::parse::Parsed)>>,
    /// Paths in the caller's order, so the file list holds still.
    order: Vec<PathBuf>,
    /// What the extractor found in each file, kept so an edit re-extracts only the files it
    /// touched.
    facts: std::collections::BTreeMap<PathBuf, (Language, crate::model::FileFacts)>,
    /// Files whose language this build has no grammar for.
    unsupported: Vec<String>,
}

/// A rendered tree or report, printed by the analysis that owns it.
#[derive(Serialize)]
struct FlowText {
    tree: String,
}

/// The first twenty notes, and a note counting the rest.
fn first_notes(notes: Vec<String>) -> Vec<String> {
    const SHOWN: usize = 20;
    let total = notes.len();
    let mut out: Vec<String> = notes.into_iter().take(SHOWN).collect();
    if total > SHOWN {
        out.push(format!("… and {} more note(s)", total - SHOWN));
    }
    out
}

/// The applied result, with a plan's plain-string notes attached under one name.
fn with_notes(applied: String, notes: &[String]) -> String {
    if notes.is_empty() {
        return applied;
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&applied) else {
        return applied;
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("notes".into(), serde_json::json!(notes));
    }
    serde_json::to_string(&value).unwrap_or(applied)
}

#[derive(Serialize)]
struct Located {
    name: String,
    kind: String,
    path: String,
    line: usize,
}

/// A signature change in the command line's spelling.
fn parse_signature_change(text: &str) -> Result<crate::refactor::signature::Change, String> {
    use crate::refactor::signature::Change;
    let parts: Vec<&str> = text.splitn(4, ':').collect();
    let index = |s: &str| {
        s.parse::<usize>()
            .map_err(|_| format!("'{s}' is not a position"))
    };
    match parts.as_slice() {
        ["remove", at] => Ok(Change::Remove(index(at)?)),
        ["move", from, to] => Ok(Change::Move {
            from: index(from)?,
            to: index(to)?,
        }),
        ["add", at, declaration, argument] => Ok(Change::Add {
            at: index(at)?,
            declaration: declaration.to_string(),
            argument: argument.to_string(),
        }),
        _ => Err(format!(
            "'{text}' is not a change. Use remove:<i>, move:<from>:<to>, or \
             add:<i>:<declaration>:<argument>"
        )),
    }
}

#[derive(Serialize)]
struct Failure {
    error: String,
    /// True when the tool declined on purpose, naming a rule, and wrote nothing.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    refused: bool,
}

fn fail(e: impl std::fmt::Display) -> String {
    serde_json::to_string(&Failure {
        error: e.to_string(),
        refused: false,
    })
    .unwrap_or_else(|_| r#"{"error":"unprintable"}"#.to_string())
}

/// [`fail`] for an error that may be a refusal.
fn failed(e: anyhow::Error) -> String {
    let refused = e.downcast_ref::<crate::refactor::Refusal>().is_some();
    serde_json::to_string(&Failure {
        error: e.to_string(),
        refused,
    })
    .unwrap_or_else(|_| r#"{"error":"unprintable"}"#.to_string())
}

fn ok<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(fail)
}

#[wasm_bindgen]
impl Workspace {
    /// Load a repository.
    #[wasm_bindgen(constructor)]
    pub fn new(files: JsValue) -> Result<Workspace, JsValue> {
        console_error_panic_hook::set_once();
        let map: std::collections::BTreeMap<String, String> = serde_wasm_bindgen::from_value(files)
            .map_err(|e| JsValue::from_str(&format!("expected {{path: text}}: {e}")))?;
        Workspace::load(map).map_err(|e| JsValue::from_str(&e))
    }
}

impl Workspace {
    /// Load a repository from plain Rust values.
    pub fn load(map: std::collections::BTreeMap<String, String>) -> Result<Workspace, String> {
        // The grammars' scanners allocate through a bump allocator that starts at NULL until
        // something hands it a region.
        #[cfg(target_arch = "wasm32")]
        {
            fun_refactor_wasm_libc::init_scanner_heap();
            fun_refactor_wasm_libc::use_rust_allocator_in_tree_sitter();
        }

        let loaded: Vec<(PathBuf, String)> = map
            .into_iter()
            .map(|(path, text)| (PathBuf::from(path), text))
            .collect();

        // The whole workspace goes into the virtual filesystem first.
        let files = crate::vfs::new_handle(loaded.clone());
        crate::vfs::activate(&files);

        let mut sources: Vec<(PathBuf, Language, String)> = Vec::new();
        let mut order: Vec<PathBuf> = Vec::new();
        let mut unsupported: Vec<String> = Vec::new();
        for (path, text) in loaded {
            order.push(path.clone());
            let Some(language) = crate::lang::detect(&path) else {
                continue;
            };
            // A grammar this build omits leaves one file out and never refuses the repository.
            if !crate::parse::Parsers::supports(language) {
                unsupported.push(format!("{} ({language})", path.display()));
                continue;
            }
            sources.push((path, language, text));
        }

        // One parser set and one extractor for the whole workspace.
        let parsers = crate::parse::Parsers::new();
        let mut extractor = crate::extract::Extractor::new();
        let mut facts = std::collections::BTreeMap::new();
        let mut extracted = Vec::with_capacity(sources.len());
        for (path, language, source) in &sources {
            let one =
                crate::index::extract_facts(&parsers, &mut extractor, path, *language, source)
                    .map_err(|e| format!("indexing failed: {e}"))?;
            facts.insert(path.clone(), (*language, one.clone()));
            extracted.push((path.clone(), *language, one));
        }
        let index = Index::build_from_facts(&extracted);

        Ok(Workspace {
            index,
            files,
            parsed: std::cell::RefCell::new(None),
            order,
            facts,
            unsupported,
        })
    }
}

#[wasm_bindgen]
impl Workspace {
    /// Every file loaded, with the language each was recognised as.
    pub fn files(&self) -> String {
        self.enter();
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
        self.enter();
        #[derive(Serialize)]
        struct Stats {
            files: usize,
            symbols: usize,
            references: usize,
            languages: Vec<(String, usize)>,
            unparsed: Vec<String>,
            /// Left out because this build has no grammar for them.
            unsupported: Vec<String>,
        }
        let mut by_language: std::collections::BTreeMap<&str, usize> = Default::default();
        let mut unparsed = Vec::new();
        for (path, info) in self.index.files() {
            *by_language.entry(info.language.name()).or_default() += 1;
            if info.gaps.contains(&FactGap::SyntaxErrors) {
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
            unsupported: self.unsupported.clone(),
        })
    }

    /// Every definition in a file, for an outline.
    pub fn symbols(&self, path: &str) -> String {
        self.enter();
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

    /// Every use of the symbol at this position, each with its confidence.
    pub fn references(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => {
                #[derive(Serialize)]
                struct Ref {
                    path: String,
                    line: usize,
                    col: usize,
                    confidence: String,
                    /// Whether a rename of this symbol would rewrite this site.
                    rewritable: bool,
                }
                let rewritable = crate::refactor::rename::rewritable_spans(&self.index, id);
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
                            rewritable: rewritable.contains(&(r.file.clone(), r.span)),
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
        self.enter();
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        match crate::navigate::definitions_at(&self.index, Path::new(path), offset) {
            Some(found) => ok(&found),
            None => fail("nothing is defined or referenced at that position"),
        }
    }

    /// The type this position's symbol declares, as the source wrote it.
    pub fn declared_type(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        match crate::analysis::types::of(&self.index, id) {
            Ok(declared) => ok(&declared),
            Err(e) => fail(e.to_string()),
        }
    }

    /// Rename the symbol at a position.
    pub fn rename(&mut self, path: &str, line: usize, col: usize, new_name: &str) -> String {
        self.enter();
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        match crate::refactor::rename::plan(&self.index, id, new_name) {
            Ok(plan) => self.apply(plan.edits, plan.warnings),
            Err(e) => failed(e),
        }
    }

    /// Retire a feature flag and the branch that only served it.
    pub fn remove_flag(&mut self, flag: &str, value: bool) -> String {
        self.enter();
        let mut sources: std::collections::BTreeMap<PathBuf, (Language, String)> =
            Default::default();
        for path in &self.order {
            let Some(language) = crate::lang::detect(path) else {
                continue;
            };
            if !crate::parse::Parsers::supports(language) {
                continue;
            }
            let Ok(text) = crate::vfs::read_to_string(path) else {
                continue;
            };
            sources.insert(path.clone(), (language, text));
        }
        match crate::refactor::cascade::remove_flag_in(sources, flag, value) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Code written more than once, compared structurally.
    pub fn duplicates(&self, min_tokens: Option<usize>) -> String {
        self.enter();
        let options = crate::analysis::duplicates::Options {
            min_tokens,
            ..Default::default()
        };
        match crate::analysis::duplicates::find(&self.index, &options) {
            Ok(classes) => ok(&classes),
            Err(e) => failed(e),
        }
    }

    /// Symbols nothing appears to use.
    pub fn unused(&self) -> String {
        self.enter();
        #[derive(Serialize)]
        struct Dead {
            name: String,
            kind: String,
            path: String,
            exported: bool,
        }
        // The catalogs mark a `#[test]` or an HTTP handler as a root.
        let entrypoints = match crate::analysis::entrypoints::Entrypoints::detect(&self.index) {
            Ok(roots) => roots,
            Err(e) => return fail(e),
        };
        let report = crate::refactor::delete::find_unused(&self.index, &entrypoints);
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
        self.enter();
        crate::vfs::read_to_string(PathBuf::from(path)).unwrap_or_default()
    }

    /// What satisfies the abstraction at this position.
    pub fn implementations(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => {
                let found = crate::navigate::implementations_of(&self.index, id);
                ok(&self.locate_symbols(&found))
            }
            Err(e) => fail(e),
        }
    }

    /// Every use. Reports the uncertain ones apart.
    pub fn usages(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => ok(&crate::navigate::usages_of(&self.index, id)),
            Err(e) => fail(e),
        }
    }

    /// The call graph, upwards from here.
    pub fn callers(&self, path: &str, line: usize, col: usize, depth: usize) -> String {
        self.enter();
        self.call_tree(path, line, col, depth, true)
    }

    /// The call graph, downwards from here.
    pub fn callees(&self, path: &str, line: usize, col: usize, depth: usize) -> String {
        self.enter();
        self.call_tree(path, line, col, depth, false)
    }

    /// The shape of the whole call graph.
    pub fn graph(&self) -> String {
        self.enter();
        #[derive(Serialize)]
        struct Shape {
            functions: usize,
            edges: usize,
            hierarchy_edges: usize,
        }
        let graph = crate::analysis::call_graph::CallGraph::build(&self.index);
        ok(&Shape {
            functions: graph.node_count(),
            edges: graph.edge_count(),
            hierarchy_edges: graph.hierarchy_edge_count(),
        })
    }

    /// Where the value at this position came from.
    pub fn flow_back(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        if !crate::analysis::flow::applies_to(&self.index, Path::new(path)) {
            return match self.symbol_at(path, line, col) {
                Ok(id) => match crate::analysis::provenance::provenance(&self.index, id, 8) {
                    Ok(traced) => ok(&FlowText {
                        tree: traced.format_tree(),
                    }),
                    Err(e) => failed(e),
                },
                Err(e) => fail(e),
            };
        }
        match crate::analysis::flow::backward(&self.index, Path::new(path), offset, 8) {
            Ok(flow) => ok(&FlowText {
                tree: flow.format_tree(),
            }),
            Err(e) => failed(e),
        }
    }

    /// Where the value declared at this position goes.
    pub fn flow_forward(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        if !crate::analysis::flow::applies_to(&self.index, Path::new(path)) {
            return match crate::analysis::provenance::consumers(&self.index, id, 8) {
                Ok(traced) => ok(&FlowText {
                    tree: traced.format_tree(),
                }),
                Err(e) => failed(e),
            };
        }
        match crate::analysis::flow::forward(&self.index, id, 8) {
            Ok(flow) => ok(&FlowText {
                tree: flow.format_tree(),
            }),
            Err(e) => failed(e),
        }
    }

    /// Everything a change here could affect.
    pub fn impact(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => match crate::analysis::impact::analyse(&self.index, id, 3) {
                Ok(impact) => ok(&FlowText {
                    tree: crate::analysis::impact::format_report(&self.index, &impact),
                }),
                Err(e) => failed(e),
            },
            Err(e) => fail(e),
        }
    }

    /// Configuration keys and the code that reads them.
    pub fn stitch(&self) -> String {
        self.enter();
        match crate::analysis::stitch::chains(&self.index) {
            Ok(chains) => ok(&FlowText {
                tree: crate::analysis::stitch::format_chains(&chains),
            }),
            Err(e) => failed(e),
        }
    }

    /// Where execution starts.
    pub fn entrypoints(&self) -> String {
        self.enter();
        #[derive(Serialize)]
        struct Entry {
            kind: String,
            name: String,
            path: String,
            line: usize,
        }
        let catalog = match crate::analysis::entrypoints::Catalog::builtin() {
            Ok(catalog) => catalog,
            Err(e) => return fail(e),
        };
        let mut out: Vec<Entry> = Vec::new();
        for found in catalog.detect(&self.index) {
            let Some(symbol) = self.index.symbol(found.symbol) else {
                continue;
            };
            out.push(Entry {
                kind: found.kind.as_str().to_string(),
                name: symbol.qualified_name(),
                path: symbol.file.display().to_string(),
                line: self.line_of(&symbol.file, symbol.name_span.start),
            });
        }
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
        ok(&out)
    }

    /// The parse tree of a file, as a structure a view can walk.
    pub fn ast(&self, path: &str) -> String {
        self.enter();
        #[derive(Serialize)]
        struct TreeNode {
            kind: String,
            /// The grammar's name for this child's role in its parent, when it has one.
            field: Option<String>,
            line: usize,
            col: usize,
            end_line: usize,
            end_col: usize,
            /// The source it covers, for a leaf.
            text: Option<String>,
            children: Vec<TreeNode>,
        }

        let path_buf = PathBuf::from(path);
        let Some(language) = crate::lang::detect(&path_buf) else {
            return fail(format!("no grammar recognises {path}"));
        };
        let Ok(source) = crate::vfs::read_to_string(&path_buf) else {
            return fail("file is not loaded");
        };
        if let Err(e) = self.reparse(&path_buf, language, &source) {
            return fail(e);
        }
        let borrowed = self.parsed.borrow();
        let parsed = &borrowed.as_ref().expect("just parsed").2;
        let lines = LineIndex::new(&source);

        fn build(
            node: tree_sitter::Node<'_>,
            field: Option<String>,
            source: &str,
            lines: &LineIndex,
        ) -> TreeNode {
            let start = lines.line_col(node.start_byte(), source);
            let end = lines.line_col(node.end_byte(), source);
            let mut children = Vec::new();
            let mut cursor = node.walk();
            for (i, child) in node.children(&mut cursor).enumerate() {
                if !child.is_named() {
                    continue;
                }
                let name = node.field_name_for_child(i as u32).map(str::to_string);
                children.push(build(child, name, source, lines));
            }
            // A leaf carries its text; a branch does not, because its text is every
            // descendant's and repeating it makes the tree unreadable.
            let text = if children.is_empty() {
                let slice = &source[node.start_byte()..node.end_byte()];
                Some(slice.chars().take(80).collect())
            } else {
                None
            };
            TreeNode {
                kind: node.kind().to_string(),
                field,
                line: start.line,
                col: start.col,
                end_line: end.line,
                end_col: end.col,
                text,
                children,
            }
        }

        ok(&build(parsed.root(), None, &source, &lines))
    }

    /// Parse `path` unless the memo already holds this exact text.
    fn reparse(&self, path: &Path, language: Language, source: &str) -> Result<(), anyhow::Error> {
        {
            let held = self.parsed.borrow();
            if let Some((held_path, held_source, _)) = held.as_ref() {
                if held_path == path && held_source == source {
                    return Ok(());
                }
            }
        }
        let parsed = crate::parse::Parsers::new().parse(language, source)?;
        *self.parsed.borrow_mut() = Some((path.to_path_buf(), source.to_string(), parsed));
        Ok(())
    }

    /// What the cursor is on: the symbol, and the coordinate that names it.
    pub fn at(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        #[derive(Serialize)]
        struct Here {
            /// `path:line:col`, what `fr` accepts as a target.
            coordinate: String,
            /// The symbol's own definition site, when the cursor is on a use of it.
            definition: Option<String>,
            name: Option<String>,
            kind: Option<String>,
            /// The declaring type or module, where there is one.
            qualifier: Option<String>,
            exported: bool,
            /// The innermost tree node, which a pattern would have to match.
            node: Option<String>,
        }

        let node = self.node_kind_at(path, line, col);
        let Ok(id) = self.symbol_at(path, line, col) else {
            return ok(&Here {
                coordinate: format!("{path}:{line}:{col}"),
                definition: None,
                name: None,
                kind: None,
                qualifier: None,
                exported: false,
                node,
            });
        };
        let Some(symbol) = self.index.symbol(id) else {
            return fail("the symbol is not in the index");
        };
        let at = self.line_of(&symbol.file, symbol.name_span.start);
        ok(&Here {
            coordinate: format!("{path}:{line}:{col}"),
            definition: Some(format!("{}:{}", symbol.file.display(), at)),
            name: Some(symbol.name.clone()),
            kind: Some(symbol.kind.as_str().to_string()),
            qualifier: symbol.qualifier.clone(),
            exported: symbol.exported,
            node,
        })
    }

    /// What this file may become, and why the rest stay out of reach.
    pub fn translations(&self, path: &str) -> String {
        self.enter();
        #[derive(Serialize)]
        struct Option_ {
            language: String,
            /// Where the rewritten file lands.
            destination: Option<String>,
            /// Absent when it can be done.
            unavailable: Option<String>,
            /// Present when this is a translation and not the same bytes, saying how much of it
            /// is real.
            draft: Option<String>,
            /// A port to another framework and not to another language.
            framework: bool,
        }

        // Three constructors and not six literals.
        impl Option_ {
            fn offered(language: &str, destination: String, draft: Option<String>) -> Option_ {
                Option_ {
                    language: language.to_string(),
                    destination: Some(destination),
                    unavailable: None,
                    draft,
                    framework: false,
                }
            }

            fn refused(language: &str, why: String, destination: Option<String>) -> Option_ {
                Option_ {
                    language: language.to_string(),
                    destination,
                    unavailable: Some(why),
                    draft: None,
                    framework: false,
                }
            }

            fn port(self) -> Option_ {
                Option_ {
                    framework: true,
                    ..self
                }
            }
        }
        let path_buf = PathBuf::from(path);
        let Some(from) = crate::lang::detect(&path_buf) else {
            return fail(format!("no grammar recognises {path}"));
        };

        let possible = crate::translate::targets(from);
        let mut out: Vec<Option_> = Vec::new();

        // FastAPI is a framework and not a language.
        if crate::transpile::nextjs::is_api_route(&path_buf) {
            match crate::transpile::nextjs::plan(&path_buf) {
                Ok(plan) => out.push(
                    Option_::offered(
                        "fastapi",
                        plan.destination.display().to_string(),
                        Some(format!(
                            "route {}: {}{}",
                            plan.route,
                            plan.methods.join(", "),
                            if plan.fidelity.carried_verbatim == 0 {
                                String::new()
                            } else {
                                format!(
                                    "; {} construct(s) carried over as comments",
                                    plan.fidelity.carried_verbatim
                                )
                            }
                        )),
                    )
                    .port(),
                ),
                Err(e) => out.push(Option_::refused("fastapi", e.to_string(), None).port()),
            }
        }
        for language in crate::lang::Language::ALL {
            if *language == from {
                continue;
            }
            // A containment is the same bytes; a translation is a draft.
            if !possible.contains(language)
                && crate::transpile::can_be_read(from)
                && crate::transpile::can_be_written(*language)
            {
                match crate::transpile::plan(&path_buf, *language) {
                    Ok(plan) => {
                        let f = &plan.fidelity;
                        out.push(Option_::offered(
                            language.name(),
                            plan.destination.display().to_string(),
                            Some(format!(
                                "a draft. {}/{} signatures complete, {} construct(s) carried over as comments",
                                f.signatures_complete, f.functions, f.carried_verbatim
                            )),
                        ));
                    }
                    Err(e) => out.push(Option_::refused(language.name(), e.to_string(), None)),
                }
                continue;
            }
            if possible.contains(language) {
                // Offered, but the file still has to parse as it, a `.scss` using nesting is
                // not CSS.
                match crate::translate::plan(&path_buf, *language) {
                    Ok(plan) => out.push(Option_::offered(
                        language.name(),
                        plan.destination.display().to_string(),
                        None,
                    )),
                    Err(e) => out.push(Option_::refused(
                        language.name(),
                        e.to_string(),
                        crate::translate::destination_for(&path_buf, *language)
                            .ok()
                            .map(|p| p.display().to_string()),
                    )),
                }
            } else {
                out.push(Option_::refused(
                    language.name(),
                    crate::translate::why_not(from, *language),
                    None,
                ));
            }
        }
        ok(&out)
    }

    /// Write this file as another language, beside the original.
    pub fn translate(&mut self, path: &str, language: &str) -> String {
        self.enter();
        let path_buf = PathBuf::from(path);

        // `fastapi` is not a language, and the translation into it reads the file's
        // path as well as its text, a Next.js route's URL is where it sits on disk.
        if language.eq_ignore_ascii_case("fastapi") {
            return match crate::transpile::nextjs::plan(&path_buf) {
                Ok(plan) => {
                    let f = plan.fidelity.clone();
                    let route = plan.route.clone();
                    let methods = plan.methods.join(", ");
                    let applied = self.apply(plan.edits, Vec::new());
                    with_notes(
                        applied,
                        &std::iter::once(format!(
                            "serving {route} ({methods}); {} construct(s) had no counterpart \
                             and are in the file as comments",
                            f.carried_verbatim
                        ))
                        .chain(first_notes(f.notes))
                        .collect::<Vec<_>>(),
                    )
                }
                Err(e) => failed(e),
            };
        }

        let Some(to) = crate::lang::Language::from_name(language) else {
            return fail(format!("unknown language '{language}'"));
        };
        let Some(from) = crate::lang::detect(&path_buf) else {
            return fail(format!("no grammar recognises {path}"));
        };
        if crate::translate::targets(from).contains(&to) {
            return match crate::translate::plan(&path_buf, to) {
                Ok(plan) => self.apply(plan.edits, Vec::new()),
                Err(e) => failed(e),
            };
        }
        match crate::transpile::plan(&path_buf, to) {
            Ok(plan) => {
                let f = plan.fidelity.clone();
                let applied = self.apply(plan.edits, Vec::new());
                // Send the fidelity with the result: a draft that announces nothing
                // reads as though a person wrote it.
                with_notes(
                    applied,
                    &std::iter::once(format!(
                        "{}/{} signature(s) carried across complete; {} construct(s) had no \
                         counterpart and are in the file as comments",
                        f.signatures_complete, f.functions, f.carried_verbatim
                    ))
                    .chain(first_notes(f.notes))
                    .collect::<Vec<_>>(),
                )
            }
            Err(e) => failed(e),
        }
    }

    /// What this build can do, per language.
    pub fn capabilities(&self) -> String {
        self.enter();
        ok(&crate::capabilities::matrix())
    }

    /// Extract the selected range into a binding.
    pub fn extract_variable(&mut self, path: &str, range: &str, name: &str) -> String {
        self.enter();
        let (file, span) = match self.span_of(path, range) {
            Ok(pair) => pair,
            Err(e) => return fail(e),
        };
        match crate::refactor::extract::variable(&self.index, &file, span, name, false) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Extract the selected statements into a function.
    pub fn extract_function(&mut self, path: &str, range: &str, name: &str) -> String {
        self.enter();
        let (file, span) = match self.span_of(path, range) {
            Ok(pair) => pair,
            Err(e) => return fail(e),
        };
        match crate::refactor::extract::function(&self.index, &file, span, name) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Replace a binding's uses with its value.
    pub fn inline_variable(&mut self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => match crate::refactor::inline::variable(&self.index, id) {
                Ok(plan) => self.apply(plan.edits, Vec::new()),
                Err(e) => failed(e),
            },
            Err(e) => fail(e),
        }
    }

    /// Replace a call with the callee's body.
    pub fn inline_call(&mut self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        match crate::refactor::inline::call(&self.index, Path::new(path), offset) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Change a function's parameters, and every call site.
    pub fn signature(&mut self, path: &str, line: usize, col: usize, change: &str) -> String {
        self.enter();
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        let parsed = match parse_signature_change(change) {
            Ok(parsed) => parsed,
            Err(e) => return fail(e),
        };
        match crate::refactor::signature::change(&self.index, id, parsed) {
            Ok(plan) => {
                let notes = plan.notes.clone();
                let applied = self.apply(plan.edits, Vec::new());
                with_notes(applied, &notes)
            }
            Err(e) => failed(e),
        }
    }

    /// Move a symbol to another file, carrying what it needs.
    pub fn move_symbol(&mut self, path: &str, line: usize, col: usize, dest: &str) -> String {
        self.enter();
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        match crate::refactor::move_symbol::to_file(&self.index, id, Path::new(dest)) {
            Ok(plan) => {
                let notes = plan.warnings.clone();
                let applied = self.apply(plan.edits, Vec::new());
                with_notes(applied, &notes)
            }
            Err(e) => failed(e),
        }
    }

    /// Drop unused imports and sort the rest.
    pub fn organize_imports(&mut self, path: &str) -> String {
        self.enter();
        match crate::refactor::imports::plan(&self.index, Path::new(path)) {
            Ok(plan) => self.apply(plan.edits, plan.warnings),
            Err(e) => failed(e),
        }
    }

    /// Which local transformations apply at this position.
    pub fn rewrites_at(&self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        #[derive(Serialize)]
        struct Available {
            name: String,
            describes: String,
        }
        match crate::refactor::rewrite::available(&self.index, Path::new(path), offset) {
            Ok(found) => ok(&found
                .into_iter()
                .map(|r| Available {
                    name: r.as_str().to_string(),
                    describes: r.describe().to_string(),
                })
                .collect::<Vec<_>>()),
            Err(e) => failed(e),
        }
    }

    /// Apply one of them.
    pub fn rewrite(&mut self, path: &str, line: usize, col: usize, which: &str) -> String {
        self.enter();
        let Ok(offset) = self.offset(path, line, col) else {
            return fail("that position is outside the file");
        };
        let Some(kind) = crate::refactor::rewrite::Rewrite::from_name(which) else {
            return fail(format!("no rewrite called '{which}'"));
        };
        match crate::refactor::rewrite::apply(&self.index, Path::new(path), offset, kind) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Rewrite every occurrence of a pattern.
    pub fn restructure(&mut self, language: &str, pattern: &str, template: &str) -> String {
        self.enter();
        let Some(language) = crate::lang::Language::from_name(language) else {
            return fail(format!("unknown language '{language}'"));
        };
        match crate::refactor::restructure::apply(&self.index, language, pattern, template) {
            Ok(plan) => self.apply(plan.edits, Vec::new()),
            Err(e) => failed(e),
        }
    }

    /// Delete a symbol, refusing while anything uses it.
    pub fn delete(&mut self, path: &str, line: usize, col: usize) -> String {
        self.enter();
        match self.symbol_at(path, line, col) {
            Ok(id) => match crate::refactor::delete::plan(&self.index, id) {
                Ok(plan) => self.apply(plan.edits, plan.warnings),
                Err(e) => failed(e),
            },
            Err(e) => fail(e),
        }
    }

    /// Make this workspace's files the ones the analysis reads.
    fn enter(&self) {
        crate::vfs::activate(&self.files);
    }

    /// The innermost named node covering a position, by name.
    fn node_kind_at(&self, path: &str, line: usize, col: usize) -> Option<String> {
        let offset = self.offset(path, line, col).ok()?;
        let path = PathBuf::from(path);
        let language = crate::lang::detect(&path)?;
        let source = crate::vfs::read_to_string(&path).ok()?;
        self.reparse(&path, language, &source).ok()?;
        let borrowed = self.parsed.borrow();
        let parsed = &borrowed.as_ref()?.2;
        let node = parsed
            .root()
            .named_descendant_for_byte_range(offset, offset)?;
        Some(node.kind().to_string())
    }

    /// A `line:col-line:col` selection, as the command line spells a range.
    fn span_of(&self, path: &str, range: &str) -> Result<(PathBuf, Span), String> {
        let (start, end) = range
            .split_once('-')
            .ok_or_else(|| "a range is line:col-line:col".to_string())?;
        let at = |text: &str| -> Result<usize, String> {
            let (line, col) = text
                .split_once(':')
                .ok_or_else(|| format!("'{text}' is not line:col"))?;
            let line = line
                .parse()
                .map_err(|_| format!("'{text}' is not line:col"))?;
            let col = col
                .parse()
                .map_err(|_| format!("'{text}' is not line:col"))?;
            self.offset(path, line, col)
        };
        Ok((PathBuf::from(path), Span::new(at(start)?, at(end)?)))
    }

    /// The line a byte offset falls on, for display.
    fn line_of(&self, path: &Path, offset: usize) -> usize {
        crate::vfs::read_to_string(path)
            .map(|source| LineIndex::new(&source).line_col(offset, &source).line)
            .unwrap_or(0)
    }

    fn locate_symbols(&self, ids: &[crate::model::SymbolId]) -> Vec<Located> {
        ids.iter()
            .filter_map(|id| self.index.symbol(*id))
            .map(|s| Located {
                name: s.qualified_name(),
                kind: s.kind.as_str().to_string(),
                path: s.file.display().to_string(),
                line: self.line_of(&s.file, s.name_span.start),
            })
            .collect()
    }

    /// The call graph around one symbol, as nodes and edges a browser can draw.
    pub fn graph_around(&self, path: &str, line: usize, col: usize, depth: usize) -> String {
        self.enter();
        let start = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };

        #[derive(Serialize)]
        struct Node {
            id: u32,
            name: String,
            file: String,
            line: usize,
            /// The column of the name, so a click lands on the symbol and not on the
            /// indentation before it.
            col: usize,
            /// Hops from the symbol the reader asked about.
            rank: i32,
        }
        #[derive(Serialize)]
        struct Edge {
            from: u32,
            to: u32,
            /// `call` for a resolved call.
            kind: &'static str,
        }
        #[derive(Serialize)]
        struct Drawing {
            nodes: Vec<Node>,
            edges: Vec<Edge>,
            root: u32,
            /// True when the walk stopped at `depth` with more to see.
            more: bool,
        }

        let graph = crate::analysis::call_graph::CallGraph::build(&self.index);
        let near = graph.neighbourhood(start, depth);

        let nodes: Vec<Node> = near
            .nodes
            .iter()
            .filter_map(|(id, at)| {
                let symbol = self.index.symbol(*id)?;
                let source = crate::vfs::read_to_string(&symbol.file).ok()?;
                let at_name = LineIndex::new(&source).line_col(symbol.name_span.start, &source);
                Some(Node {
                    id: id.0,
                    name: symbol.qualified_name(),
                    file: symbol.file.display().to_string(),
                    line: at_name.line,
                    col: at_name.col,
                    rank: *at,
                })
            })
            .collect();
        let known: std::collections::HashSet<u32> = nodes.iter().map(|n| n.id).collect();
        let edges: Vec<Edge> = near
            .edges
            .iter()
            .filter(|(from, to, _)| known.contains(&from.0) && known.contains(&to.0))
            .map(|(from, to, hierarchy)| Edge {
                from: from.0,
                to: to.0,
                kind: match hierarchy {
                    true => "dispatch",
                    false => "call",
                },
            })
            .collect();
        let more = near.more;

        ok(&Drawing {
            nodes,
            edges,
            root: start.0,
            more,
        })
    }

    /// A call tree, rendered the way the terminal renders it.
    fn call_tree(
        &self,
        path: &str,
        line: usize,
        col: usize,
        depth: usize,
        upwards: bool,
    ) -> String {
        let id = match self.symbol_at(path, line, col) {
            Ok(id) => id,
            Err(e) => return fail(e),
        };
        let graph = crate::analysis::call_graph::CallGraph::build(&self.index);
        use crate::analysis::call_graph::Direction2;
        let direction = if upwards {
            Direction2::Callers
        } else {
            Direction2::Callees
        };
        let traced = graph.trace(id, direction, depth.clamp(1, 8));
        ok(&FlowText {
            tree: traced.format_tree(&self.index),
        })
    }

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
            /// The sites the refactoring declined to touch.
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
        // The edit moved the bytes behind every span, so remeasure them all.
        let written: Vec<PathBuf> = outcomes
            .iter()
            .filter(|o| o.changed())
            .map(|o| o.path.clone())
            .collect();
        if let Err(e) = self.reindex(&written) {
            return fail(e);
        }

        ok(&Applied {
            files: changed,
            warnings: &warnings,
        })
    }

    /// Re-extract the files this wrote, then resolve the whole workspace again.
    fn reindex(&mut self, written: &[PathBuf]) -> anyhow::Result<()> {
        let parsers = crate::parse::Parsers::new();
        let mut extractor = crate::extract::Extractor::new();
        for path in written {
            let Some(language) = crate::lang::detect(path) else {
                continue;
            };
            if !crate::parse::Parsers::supports(language) {
                continue;
            }
            let Ok(text) = crate::vfs::read_to_string(path) else {
                // The edit engine deleted it, so it has no facts any more.
                self.facts.remove(path);
                self.order.retain(|p| p != path);
                continue;
            };
            if !self.order.iter().any(|p| p == path) {
                self.order.push(path.clone());
            }
            let one = crate::index::extract_facts(&parsers, &mut extractor, path, language, &text)?;
            self.facts.insert(path.clone(), (language, one));
        }

        // In the order the user sees, so symbol ids stay stable where the files did.
        let mut all = Vec::with_capacity(self.order.len());
        for path in &self.order {
            if let Some((language, facts)) = self.facts.get(path) {
                all.push((path.clone(), *language, facts.clone()));
            }
        }
        self.index = Index::build_from_facts(&all);
        Ok(())
    }
}

/// The version of the crate this build came from.
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
