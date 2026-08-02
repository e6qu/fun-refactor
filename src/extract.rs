//! Fact extraction: turn a parsed tree into symbols, references, scopes and imports.
//!
//! All language-specific knowledge lives in tree-sitter query files under
//! `queries/<language>/`, following the capture conventions below. Adding a language
//! means writing queries, not Rust.
//!
//! # Capture conventions
//!
//! - `@scope` — a node introducing a lexical scope.
//! - `@definition.<kind>` — a definition; `<kind>` maps to [`SymbolKind`]. The captured
//!   node is the whole definition (its `full_span`). A sibling `@name` capture in the
//!   same match marks the identifier (its `name_span`) — the bytes a rename rewrites.
//! - `@export` — presence in a definition match marks the symbol as externally visible.
//! - `@reference.<kind>` — a use site; `<kind>` maps to [`ReferenceKind`].
//! - `@import` — an import statement, with optional `@import.path`, `@import.alias`,
//!   `@import.name` and `@import.original` captures.
//! - `@import.glob` — marks a wildcard import.

use crate::lang::Language;
use crate::model::*;
use crate::parse::Parsed;
use crate::span::Span;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

/// Query sources per language, embedded at compile time.
fn query_source(lang: Language) -> Option<&'static str> {
    Some(match lang {
        Language::Rust => include_str!("../queries/rust/facts.scm"),
        Language::Go => include_str!("../queries/go/facts.scm"),
        Language::Python => include_str!("../queries/python/facts.scm"),
        Language::TypeScript | Language::Tsx => include_str!("../queries/typescript/facts.scm"),
        Language::Zig => include_str!("../queries/zig/facts.scm"),
        Language::Bash => include_str!("../queries/bash/facts.scm"),
        Language::Hcl => include_str!("../queries/hcl/facts.scm"),
        Language::Css | Language::Scss => include_str!("../queries/css/facts.scm"),
        Language::Html => include_str!("../queries/html/facts.scm"),
        Language::Xml => include_str!("../queries/xml/facts.scm"),
        Language::Yaml | Language::Helm => include_str!("../queries/yaml/facts.scm"),
        Language::Markdown => include_str!("../queries/markdown/facts.scm"),
    })
}

fn symbol_kind(name: &str) -> Option<SymbolKind> {
    Some(match name {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "struct" => SymbolKind::Struct,
        "trait" => SymbolKind::Trait,
        "interface" => SymbolKind::Interface,
        "enum" => SymbolKind::Enum,
        "type" => SymbolKind::TypeAlias,
        "constant" => SymbolKind::Constant,
        "variable" => SymbolKind::Variable,
        "parameter" => SymbolKind::Parameter,
        "field" => SymbolKind::Field,
        "module" => SymbolKind::Module,
        "block" => SymbolKind::Block,
        "key" => SymbolKind::Key,
        "selector" => SymbolKind::Selector,
        "property" => SymbolKind::Property,
        "anchor" => SymbolKind::Anchor,
        "heading" => SymbolKind::Heading,
        "link-def" => SymbolKind::LinkDef,
        "element-id" => SymbolKind::ElementId,
        _ => return None,
    })
}

fn reference_kind(name: &str) -> Option<ReferenceKind> {
    Some(match name {
        "call" => ReferenceKind::Call,
        "identifier" => ReferenceKind::Identifier,
        "type" => ReferenceKind::Type,
        "field" => ReferenceKind::Field,
        "string" => ReferenceKind::StringRef,
        _ => return None,
    })
}

/// Ranks reference kinds from most to least specific.
///
/// `foo()` matches both a call pattern and the catch-all identifier pattern; the call
/// is the informative answer, so it wins.
fn reference_specificity(kind: ReferenceKind) -> u8 {
    match kind {
        ReferenceKind::Call => 0,
        ReferenceKind::Field => 1,
        ReferenceKind::Type => 2,
        ReferenceKind::StringRef => 3,
        ReferenceKind::Identifier => 4,
    }
}

/// Compiled queries, cached per language.
pub struct Extractor {
    queries: HashMap<Language, Query>,
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            queries: HashMap::new(),
        }
    }

    fn query_for(&mut self, lang: Language, grammar: &tree_sitter::Language) -> Result<&Query> {
        if !self.queries.contains_key(&lang) {
            let source = query_source(lang)
                .with_context(|| format!("no fact queries defined for {lang}"))?;
            let query = Query::new(grammar, source)
                .with_context(|| format!("compiling {lang} fact queries"))?;
            self.queries.insert(lang, query);
        }
        Ok(&self.queries[&lang])
    }

    /// Extract every fact from one parsed file.
    pub fn extract(&mut self, parsed: &Parsed, path: &Path, source: &str) -> Result<FileFacts> {
        let lang = parsed.language;
        let root = parsed.root();
        let grammar = root.language();
        let query = self.query_for(lang, &grammar)?;
        let capture_names: Vec<String> =
            query.capture_names().iter().map(|s| s.to_string()).collect();

        // Pass 1: collect raw captures grouped by match.
        let mut raw_scopes: Vec<Span> = vec![Span::from(root)];
        let mut raw_defs: Vec<RawDef> = Vec::new();
        let mut raw_refs: Vec<RawRef> = Vec::new();
        let mut imports: Vec<Import> = Vec::new();
        // (container span, name span) for constructs that qualify nested symbols.
        let mut containers: Vec<(Span, Span)> = Vec::new();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, root, source.as_bytes());

        while let Some(m) = matches.next() {
            let mut def: Option<(SymbolKind, Span)> = None;
            let mut name: Option<Span> = None;
            let mut exported = false;
            let mut import_parts = ImportParts::default();
            let mut is_import = false;
            let mut container_span: Option<Span> = None;
            let mut container_name: Option<Span> = None;

            for cap in m.captures {
                let cap_name = &capture_names[cap.index as usize];
                let span = Span::from(cap.node);

                if cap_name == "scope" {
                    raw_scopes.push(span);
                } else if cap_name == "name" {
                    name = Some(span);
                } else if cap_name == "export" {
                    exported = true;
                } else if cap_name == "container" {
                    container_span = Some(span);
                } else if cap_name == "container.name" {
                    container_name = Some(span);
                } else if let Some(kind) = cap_name
                    .strip_prefix("definition.")
                    .and_then(symbol_kind)
                {
                    def = Some((kind, span));
                } else if let Some(kind) = cap_name
                    .strip_prefix("reference.")
                    .and_then(reference_kind)
                {
                    raw_refs.push(RawRef { kind, span });
                } else if cap_name == "import" {
                    is_import = true;
                    import_parts.span = Some(span);
                } else if cap_name == "import.path" {
                    import_parts.path = Some(span);
                } else if cap_name == "import.alias" {
                    import_parts.alias = Some(span);
                } else if cap_name == "import.name" {
                    import_parts.names.push(span);
                } else if cap_name == "import.original" {
                    import_parts.originals.push(span);
                } else if cap_name == "import.glob" {
                    import_parts.is_glob = true;
                }
            }

            if let Some((kind, full_span)) = def {
                // A definition without an identifier cannot be renamed or referenced,
                // so it is not a usable symbol.
                if let Some(name_span) = name {
                    raw_defs.push(RawDef {
                        kind,
                        full_span,
                        name_span,
                        exported,
                    });
                }
            }

            if let (Some(span), Some(name_span)) = (container_span, container_name) {
                containers.push((span, name_span));
            }

            if is_import {
                imports.push(import_parts.build(source, path));
            }
        }
        containers.sort();
        containers.dedup();

        // Pass 2: build the scope tree.
        let scopes = build_scopes(&mut raw_scopes);

        let scope_at = |offset: usize| -> ScopeId {
            scopes
                .iter()
                .filter(|s| s.span.contains_offset(offset))
                .min_by_key(|s| s.span.len())
                .map(|s| s.id)
                .unwrap_or(ScopeId(0))
        };

        // Pass 3: materialise symbols, nesting them by span containment.
        raw_defs.sort_by_key(|d| (d.full_span.start, std::cmp::Reverse(d.full_span.end)));
        let mut symbols: Vec<Symbol> = Vec::with_capacity(raw_defs.len());
        for (i, d) in raw_defs.iter().enumerate() {
            let container = raw_defs
                .iter()
                .enumerate()
                .filter(|(j, other)| {
                    *j != i
                        && other.full_span.contains(d.full_span)
                        && other.full_span.len() > d.full_span.len()
                })
                .min_by_key(|(_, other)| other.full_span.len())
                .map(|(j, _)| SymbolId(j as u32));

            // The innermost container enclosing this definition supplies its qualifier.
            let qualifier = containers
                .iter()
                .filter(|(span, name_span)| {
                    span.contains(d.full_span) && *name_span != d.name_span
                })
                .min_by_key(|(span, _)| span.len())
                .map(|(_, name_span)| name_span.text(source).to_string());

            // A function defined inside a type-like container is a method. Deriving
            // this keeps every language from needing separate method captures.
            let kind = match (d.kind, qualifier.is_some()) {
                (SymbolKind::Function, true) => SymbolKind::Method,
                (kind, _) => kind,
            };

            symbols.push(Symbol {
                id: SymbolId(i as u32),
                name: d.name_span.text(source).to_string(),
                kind,
                name_span: d.name_span,
                full_span: d.full_span,
                file: path.to_path_buf(),
                language: lang,
                scope: scope_at(d.name_span.start),
                container,
                qualifier,
                exported: d.exported,
            });
        }

        // Pass 4: references. A capture that coincides with a definition's identifier
        // is the definition itself, not a use of it.
        let def_name_spans: std::collections::HashSet<Span> =
            symbols.iter().map(|s| s.name_span).collect();
        let mut references = Vec::new();
        for r in raw_refs {
            if def_name_spans.contains(&r.span) {
                continue;
            }
            references.push(Reference {
                name: r.span.text(source).to_string(),
                span: r.span,
                file: path.to_path_buf(),
                language: lang,
                scope: scope_at(r.span.start),
                target: None,
                // Resolution happens in the index, which can see other files.
                confidence: Confidence::NameOnly,
                kind: r.kind,
            });
        }
        // One identifier can match several patterns (a call is also an identifier).
        // Keep the most specific kind per span so each use site appears exactly once.
        references.sort_by_key(|r| (r.span, reference_specificity(r.kind)));
        references.dedup_by_key(|r| r.span);

        Ok(FileFacts {
            path: path.to_path_buf(),
            symbols,
            references,
            scopes,
            imports,
        })
    }
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

struct RawDef {
    kind: SymbolKind,
    full_span: Span,
    name_span: Span,
    exported: bool,
}

struct RawRef {
    kind: ReferenceKind,
    span: Span,
}

#[derive(Default)]
struct ImportParts {
    span: Option<Span>,
    path: Option<Span>,
    alias: Option<Span>,
    names: Vec<Span>,
    originals: Vec<Span>,
    is_glob: bool,
}

impl ImportParts {
    fn build(self, source: &str, path: &Path) -> Import {
        let span = self.span.unwrap_or(Span::new(0, 0));
        let trim = |s: &str| s.trim_matches(['"', '\'', '`']).to_string();

        // `@import.original` pairs positionally with `@import.name`; when absent the
        // name is unaliased and stands for itself.
        let names = self
            .names
            .iter()
            .enumerate()
            .map(|(i, name_span)| {
                let local = name_span.text(source).to_string();
                let original = self
                    .originals
                    .get(i)
                    .map(|s| s.text(source).to_string())
                    .unwrap_or_else(|| local.clone());
                ImportedName {
                    original,
                    local,
                    span: *name_span,
                }
            })
            .collect();

        Import {
            path: self
                .path
                .map(|s| trim(s.text(source)))
                .unwrap_or_default(),
            alias: self.alias.map(|s| s.text(source).to_string()),
            names,
            span,
            file: path.to_path_buf(),
            is_glob: self.is_glob,
        }
    }
}

/// Turn raw scope spans into a parent-linked tree, outermost first.
fn build_scopes(raw: &mut Vec<Span>) -> Vec<Scope> {
    raw.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));
    raw.dedup();

    let mut scopes: Vec<Scope> = Vec::with_capacity(raw.len());
    for (i, span) in raw.iter().enumerate() {
        // The parent is the smallest strictly-larger span that contains this one.
        let parent = raw
            .iter()
            .enumerate()
            .filter(|(j, other)| *j != i && other.contains(*span) && other.len() > span.len())
            .min_by_key(|(_, other)| other.len())
            .map(|(j, _)| ScopeId(j as u32));
        scopes.push(Scope {
            id: ScopeId(i as u32),
            span: *span,
            parent,
        });
    }
    scopes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Parsers;

    fn facts(lang: Language, source: &str) -> FileFacts {
        let parsed = Parsers::new().parse(lang, source).unwrap();
        Extractor::new()
            .extract(&parsed, Path::new("test-input"), source)
            .unwrap()
    }

    #[test]
    fn scope_tree_nests_by_containment() {
        let mut raw = vec![Span::new(0, 100), Span::new(10, 50), Span::new(20, 30)];
        let scopes = build_scopes(&mut raw);
        assert_eq!(scopes[0].parent, None);
        assert_eq!(scopes[1].parent, Some(ScopeId(0)));
        assert_eq!(scopes[2].parent, Some(ScopeId(1)));
    }

    #[test]
    fn duplicate_scope_spans_collapse() {
        let mut raw = vec![Span::new(0, 10), Span::new(0, 10)];
        assert_eq!(build_scopes(&mut raw).len(), 1);
    }

    #[test]
    fn rust_functions_and_names() {
        let src = "fn alpha() {}\npub fn beta(x: i32) -> i32 { x }\n";
        let f = facts(Language::Rust, src);
        let names: Vec<_> = f
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"alpha"), "got {names:?}");
        assert!(names.contains(&"beta"), "got {names:?}");

        // name_span must cover only the identifier — this is what rename rewrites.
        let alpha = f.symbols.iter().find(|s| s.name == "alpha").unwrap();
        assert_eq!(alpha.name_span.text(src), "alpha");
        assert!(alpha.full_span.contains(alpha.name_span));
        assert_eq!(alpha.full_span.text(src), "fn alpha() {}");
    }

    #[test]
    fn rust_export_visibility_detected() {
        let src = "fn private_one() {}\npub fn public_one() {}\n";
        let f = facts(Language::Rust, src);
        let private = f.symbols.iter().find(|s| s.name == "private_one").unwrap();
        let public = f.symbols.iter().find(|s| s.name == "public_one").unwrap();
        assert!(!private.exported);
        assert!(public.exported);
    }

    #[test]
    fn rust_methods_are_qualified_by_their_impl_type() {
        let src = "struct S;\nimpl S {\n    fn m(&self) {}\n}\n";
        let f = facts(Language::Rust, src);
        let m = f.symbols.iter().find(|s| s.name == "m").unwrap();
        // A function inside a type container is a method, qualified by the type.
        assert_eq!(m.kind, SymbolKind::Method);
        assert_eq!(m.qualifier.as_deref(), Some("S"));
        assert_eq!(m.qualified_name(), "S::m");

        // Crucially, `impl S` defines no second symbol named S: the struct is the
        // only definition, so renaming S has exactly one definition site.
        let s_defs: Vec<_> = f.symbols.iter().filter(|s| s.name == "S").collect();
        assert_eq!(s_defs.len(), 1, "got {s_defs:?}");
        assert_eq!(s_defs[0].kind, SymbolKind::Struct);

        // The `S` in `impl S` is a type reference, so a rename still rewrites it.
        let impl_offset = src.find("impl S").unwrap() + 5;
        let r = f.reference_at(impl_offset).expect("impl type is a reference");
        assert_eq!(r.name, "S");
        assert_eq!(r.kind, ReferenceKind::Type);
    }

    #[test]
    fn top_level_functions_are_not_methods() {
        let f = facts(Language::Rust, "fn free() {}\n");
        let free = f.symbols.iter().find(|s| s.name == "free").unwrap();
        assert_eq!(free.kind, SymbolKind::Function);
        assert_eq!(free.qualifier, None);
        assert_eq!(free.qualified_name(), "free");
    }

    #[test]
    fn definition_identifiers_are_not_also_references() {
        let src = "fn alpha() {}\nfn beta() { alpha(); }\n";
        let f = facts(Language::Rust, src);
        // `alpha` appears twice in the file but only the call site is a reference.
        let alpha_refs: Vec<_> = f.references.iter().filter(|r| r.name == "alpha").collect();
        assert_eq!(alpha_refs.len(), 1, "got {alpha_refs:?}");
        assert_eq!(alpha_refs[0].kind, ReferenceKind::Call);
        assert!(alpha_refs[0].span.start > src.find("fn beta").unwrap());
    }

    #[test]
    fn references_start_unresolved_at_lowest_confidence() {
        // The extractor sees one file; resolution is the index's job.
        let f = facts(Language::Rust, "fn a() { b(); }\n");
        let r = f.references.iter().find(|r| r.name == "b").unwrap();
        assert_eq!(r.target, None);
        assert_eq!(r.confidence, Confidence::NameOnly);
    }

    #[test]
    fn rust_imports_capture_path_and_names() {
        let src = "use std::collections::HashMap;\nuse foo::bar as baz;\n";
        let f = facts(Language::Rust, src);
        assert!(!f.imports.is_empty(), "expected imports");
        let all: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
        assert!(
            all.iter().any(|p| p.contains("HashMap")),
            "got paths {all:?}"
        );
    }

    #[test]
    fn scopes_cover_function_bodies() {
        let src = "fn outer() {\n    let x = 1;\n    {\n        let y = 2;\n    }\n}\n";
        let f = facts(Language::Rust, src);
        let inner_offset = src.find("let y").unwrap();
        let outer_offset = src.find("let x").unwrap();
        let inner = f.scope_at(inner_offset).unwrap();
        let outer = f.scope_at(outer_offset).unwrap();
        assert_ne!(inner, outer, "nested block should be its own scope");
        assert!(
            f.scope_chain(inner).contains(&outer),
            "inner scope should nest inside the function scope"
        );
    }

    #[test]
    fn symbol_and_reference_lookup_by_offset() {
        let src = "fn alpha() {}\nfn beta() { alpha(); }\n";
        let f = facts(Language::Rust, src);
        let def_offset = src.find("alpha").unwrap() + 1;
        assert_eq!(f.symbol_at(def_offset).map(|s| s.name.as_str()), Some("alpha"));

        let call_offset = src.rfind("alpha").unwrap() + 1;
        assert_eq!(
            f.reference_at(call_offset).map(|r| r.name.as_str()),
            Some("alpha")
        );
    }
}
