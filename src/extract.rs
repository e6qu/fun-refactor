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
use tree_sitter::{Node, Query, QueryCursor};

/// The query source for a language, for callers that need to fingerprint them.
pub fn query_source_for(lang: Language) -> Option<&'static str> {
    query_source(lang)
}

/// Query sources per language, embedded at compile time.
fn query_source(lang: Language) -> Option<&'static str> {
    Some(match lang {
        Language::Rust => include_str!("../queries/rust/facts.scm"),
        Language::Go => include_str!("../queries/go/facts.scm"),
        Language::Python => include_str!("../queries/python/facts.scm"),
        Language::TypeScript | Language::Tsx => include_str!("../queries/typescript/facts.scm"),
        Language::Zig => include_str!("../queries/zig/facts.scm"),
        Language::Java => include_str!("../queries/java/facts.scm"),
        Language::Bash => include_str!("../queries/bash/facts.scm"),
        Language::Hcl => include_str!("../queries/hcl/facts.scm"),
        Language::Css => include_str!("../queries/css/facts.scm"),
        Language::Scss => include_str!("../queries/scss/facts.scm"),
        Language::Html => include_str!("../queries/html/facts.scm"),
        Language::Xml => include_str!("../queries/xml/facts.scm"),
        Language::Yaml | Language::Helm => include_str!("../queries/yaml/facts.scm"),
        Language::Markdown => include_str!("../queries/markdown/facts.scm"),
    })
}

/// Separates the halves of a query file that compile against different grammars.
///
/// Markdown is parsed by two grammars — one for block structure, one for the inline
/// content the block grammar leaves opaque — and a query only compiles against the
/// grammar whose node names it uses. Both halves stay in one file per language, so
/// `queries/<language>/facts.scm` remains the whole story for that language.
const INLINE_SECTION: &str = "; ==== inline grammar ====";

/// The half of a language's fact queries that compiles against its main grammar.
fn block_query_source(lang: Language) -> Option<&'static str> {
    let source = query_source(lang)?;
    Some(match source.split_once(INLINE_SECTION) {
        Some((block, _)) => block,
        None => source,
    })
}

/// The half that compiles against the inline grammar, for languages that have one.
fn inline_query_source(lang: Language) -> Option<&'static str> {
    query_source(lang)?
        .split_once(INLINE_SECTION)
        .map(|(_, inline)| inline)
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

/// Narrow a captured span to the bytes that actually name something.
///
/// Grammars vary in how tightly they bound a name. Some expose only a quoted string
/// node (tree-sitter-xml's attribute values have no inner text node at all), and
/// Markdown's ATX headings include the padding after the `#`. Since a rename rewrites
/// exactly the bytes of a name span, an unrefined span would produce `id=""new""` or
/// re-insert the original padding. Trimming here fixes every language at once.
fn refine_name_span(span: Span, source: &str, lang: Language) -> Span {
    let text = span.text(source);

    // Leading and trailing whitespace is never part of a name.
    let start_trim = text.len() - text.trim_start().len();
    let end_trim = text.len() - text.trim_end().len();
    let mut start = span.start + start_trim;
    let mut end = span.end - end_trim;

    // A matched pair of surrounding quotes belongs to the syntax, not the name.
    if end > start + 1 {
        let bytes = source.as_bytes();
        let first = bytes[start];
        let last = bytes[end - 1];
        if first == last && matches!(first, b'"' | b'\'' | b'`') {
            start += 1;
            end -= 1;
        }
    }

    let span = Span::new(start, end.max(start));
    if lang == Language::Markdown {
        return trim_markdown_syntax(span, source);
    }
    span
}

/// Strip the Markdown syntax the grammar leaves inside a name.
///
/// A link label is a single node that includes its brackets (`[label]`), and an ATX
/// heading's content includes the optional closing marker (`## Title ##`). A rename
/// rewrites exactly the name span, so leaving either in would write `new: /a` for a
/// link reference definition, or leave a stray `##` on a renamed heading.
fn trim_markdown_syntax(span: Span, source: &str) -> Span {
    let text = span.text(source);
    if text.len() > 1 && text.starts_with('[') && text.ends_with(']') {
        return Span::new(span.start + 1, span.end - 1);
    }
    // CommonMark only reads a trailing run of `#` as a closing marker when a space
    // precedes it, which is what keeps `# C#` naming the heading `C#`.
    let without_marker = text.trim_end_matches('#');
    if without_marker.len() < text.len() && without_marker.ends_with(char::is_whitespace) {
        return Span::new(span.start, span.start + without_marker.trim_end().len());
    }
    span
}

/// Split a whitespace-separated attribute value into one span per token.
///
/// `class="btn btn-primary"` is a single token in every HTML-ish grammar, but it
/// references two CSS classes. Renaming one must rewrite only its own bytes, so the
/// value is fanned out into separate references rather than treated as one name.
fn split_value_spans(span: Span, source: &str) -> Vec<Span> {
    let text = span.text(source);
    if !text.trim().contains(char::is_whitespace) {
        return vec![span];
    }
    let mut spans = Vec::new();
    let mut offset = 0;
    for token in text.split_whitespace() {
        // `find` from the running offset keeps repeated tokens on distinct spans.
        if let Some(found) = text[offset..].find(token) {
            let start = span.start + offset + found;
            spans.push(Span::new(start, start + token.len()));
            offset += found + token.len();
        }
    }
    spans
}

/// What a reference was written against, if it was written as a member of something.
///
/// `w.contextWithTimeout(…)` yields `w`; `time.Now()` yields `time`; a bare
/// `helper()` yields nothing. Read from the tree rather than captured by a query,
/// because every grammar spells the shape differently but all of them put the
/// receiver first and the member last.
fn receiver_of(root: Node<'_>, span: Span, source: &str) -> Option<String> {
    const MEMBER_SHAPES: &[&str] = &[
        "selector_expression", // Go
        "member_expression",   // TypeScript, JavaScript
        "attribute",           // Python
        "field_expression",    // Rust, Zig
        "method_invocation",   // Java
        "field_access",        // Java
        "scoped_identifier",
        "scoped_type_identifier",
    ];
    // What the member was read from, where the grammar names it. Preferred over the
    // positional rule below, which assumes the member is the last child: Java's
    // `a.m(x)` is one `method_invocation` whose children are the object, the name and
    // the *argument list*, so the member is not last and the positional rule saw no
    // receiver at all — leaving every method call in the language unresolved.
    const RECEIVER_FIELDS: &[&str] = &["object", "operand", "receiver"];
    let node = root.descendant_for_byte_range(span.start, span.end)?;
    let parent = node.parent()?;

    // Terraform writes its namespace as the first segment of a traversal:
    // `var.azs`, `local.azs`, `module.azs`, and each names a different declaration.
    // The segments are flat siblings under one `expression`, so the namespace is the
    // first `variable_expr` rather than anything above this node. Without it,
    // `var.azs` and an `output "azs"` in the same directory are indistinguishable.
    if parent.kind() == "get_attr" {
        let expression = parent.parent()?;
        let mut cursor = expression.walk();
        let first = expression.named_children(&mut cursor).next()?;
        if first.kind() == "variable_expr" {
            return Some(Span::from(first).text(source).to_string());
        }
        return None;
    }

    if !MEMBER_SHAPES.contains(&parent.kind()) {
        return None;
    }

    for field in RECEIVER_FIELDS {
        if let Some(receiver) = parent.child_by_field_name(field) {
            // Only when this node really is the member of that receiver, not the
            // receiver itself: `a.b` names `a` once as an object and once as a member.
            if Span::from(receiver) != span {
                return Some(Span::from(receiver).text(source).to_string());
            }
            return None;
        }
    }

    let mut cursor = parent.walk();
    let children: Vec<Node> = parent.named_children(&mut cursor).collect();
    // The member itself is last; anything before it is what it was read from.
    let last = children.last()?;
    if Span::from(*last) != span || children.len() < 2 {
        return None;
    }
    Some(Span::from(children[0]).text(source).to_string())
}

/// The `.Values` paths a Helm template names, as references to the values file.
///
/// One reference per path, spanning the *last* segment only: renaming the key `tag`
/// under `image` must rewrite `tag` in `{{ .Values.image.tag }}` and leave `image`
/// alone. The segment before it is recorded as the receiver, which is what lets
/// resolution tell `image.tag` from a `tag` under something else.
fn values_references(
    source: &str,
    parsed: &Parsed,
    path: &Path,
    scope_at: &impl Fn(usize) -> crate::model::ScopeId,
) -> Vec<Reference> {
    let template = crate::helm::Template::of(source, parsed);
    let mut out = Vec::new();
    for action in &template.actions {
        for reference in &action.refs {
            let Some(segments) = reference.values_path() else {
                continue;
            };
            let Some(last) = segments.last() else {
                continue;
            };
            // Locate the final segment inside the chain's own bytes, so the span is
            // the key and not the whole `.Values.image.tag`.
            let text = reference.span.text(source);
            let Some(offset) = text.rfind(last.as_str()) else {
                continue;
            };
            let start = reference.span.start + offset;
            let span = Span::new(start, start + last.len());
            out.push(Reference {
                name: last.clone(),
                span,
                file: path.to_path_buf(),
                language: Language::Helm,
                scope: scope_at(span.start),
                target: None,
                confidence: Confidence::NameOnly,
                kind: ReferenceKind::StringRef,
                receiver: segments.len().checked_sub(2).map(|i| segments[i].clone()),
                receiver_is_path: true,
            });
        }
    }
    out
}

/// Was the receiver written as a path (`A::b`) rather than against a value (`a.b`)?
fn receiver_is_path(root: Node<'_>, span: Span) -> bool {
    root.descendant_for_byte_range(span.start, span.end)
        .and_then(|n| n.parent())
        .is_some_and(|p| p.kind().starts_with("scoped_") || p.kind() == "get_attr")
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
    inline_queries: HashMap<Language, Query>,
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            queries: HashMap::new(),
            inline_queries: HashMap::new(),
        }
    }

    fn compile_query(&mut self, lang: Language, grammar: &tree_sitter::Language) -> Result<()> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.queries.entry(lang) {
            let source = block_query_source(lang)
                .with_context(|| format!("no fact queries defined for {lang}"))?;
            let query = Query::new(grammar, source)
                .with_context(|| format!("compiling {lang} fact queries"))?;
            slot.insert(query);
        }
        Ok(())
    }

    fn compile_inline_query(
        &mut self,
        lang: Language,
        grammar: &tree_sitter::Language,
    ) -> Result<()> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.inline_queries.entry(lang) {
            let source = inline_query_source(lang)
                .with_context(|| format!("no inline fact queries defined for {lang}"))?;
            let query = Query::new(grammar, source)
                .with_context(|| format!("compiling {lang} inline fact queries"))?;
            slot.insert(query);
        }
        Ok(())
    }

    /// Extract every fact from one parsed file.
    pub fn extract(&mut self, parsed: &Parsed, path: &Path, source: &str) -> Result<FileFacts> {
        let lang = parsed.language;
        let root = parsed.root();
        self.compile_query(lang, &root.language())?;
        // A language whose grammar splits block and inline parsing (Markdown) hands
        // over one sub-tree per inline node. Their spans index the same source as the
        // block tree's, so their facts join the same pass.
        if let Some(inline_root) = parsed.inline_roots().next() {
            self.compile_inline_query(lang, &inline_root.language())?;
        }
        let mut units: Vec<(&Query, Node<'_>)> = vec![(&self.queries[&lang], root)];
        if let Some(query) = self.inline_queries.get(&lang) {
            units.extend(parsed.inline_roots().map(|node| (query, node)));
        }

        // Pass 1: collect raw captures grouped by match.
        let mut raw_scopes: Vec<Span> = vec![Span::from(root)];
        let mut raw_defs: Vec<RawDef> = Vec::new();
        let mut raw_refs: Vec<RawRef> = Vec::new();
        let mut imports: Vec<Import> = Vec::new();
        // (container span, name span) for constructs that qualify nested symbols.
        let mut containers: Vec<(Span, Span)> = Vec::new();

        for (query, root) in units {
            let capture_names: Vec<String> = query
                .capture_names()
                .iter()
                .map(|s| s.to_string())
                .collect();
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
                    } else if let Some(kind) =
                        cap_name.strip_prefix("definition.").and_then(symbol_kind)
                    {
                        def = Some((kind, span));
                    } else if let Some(kind) =
                        cap_name.strip_prefix("reference.").and_then(reference_kind)
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
                        let name_span = refine_name_span(name_span, source, lang);
                        if !name_span.is_empty() {
                            raw_defs.push(RawDef {
                                kind,
                                full_span,
                                name_span,
                                exported,
                            });
                        }
                    }
                }

                if let (Some(span), Some(name_span)) = (container_span, container_name) {
                    containers.push((span, name_span));
                }

                if is_import {
                    imports.push(import_parts.build(source, path));
                }
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

        // One identifier position is one definition. Several patterns may legitimately
        // match the same node (a language often needs one pattern per parent context),
        // and without merging them a rename would emit two edits over the same bytes
        // and be rejected as a conflict.
        merge_duplicate_defs(&mut raw_defs);

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
            //
            // Locals and parameters are deliberately excluded: a method's parameter
            // belongs to the method, not the type, so qualifying it would produce
            // nonsense like `Widget::self`.
            let qualifier = if d.kind.is_local() {
                None
            } else {
                containers
                    .iter()
                    .filter(|(span, name_span)| {
                        span.contains(d.full_span) && *name_span != d.name_span
                    })
                    .min_by_key(|(span, _)| span.len())
                    .map(|(_, name_span)| name_span.text(source).to_string())
            };

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
            let refined = refine_name_span(r.span, source, lang);
            if def_name_spans.contains(&refined) || refined.is_empty() {
                continue;
            }
            // A string reference may name several things at once (`class="a b"`),
            // so it fans out into one reference per token.
            let spans = if r.kind == ReferenceKind::StringRef {
                split_value_spans(refined, source)
            } else {
                vec![refined]
            };
            for span in spans {
                references.push(Reference {
                    name: span.text(source).to_string(),
                    span,
                    file: path.to_path_buf(),
                    language: lang,
                    scope: scope_at(span.start),
                    target: None,
                    // Resolution happens in the index, which can see other files.
                    confidence: Confidence::NameOnly,
                    kind: r.kind,
                    receiver: receiver_of(root, span, source),
                    receiver_is_path: receiver_is_path(root, span),
                });
            }
        }
        // Helm hides its references from the grammar. A template action is masked
        // before parsing so the surrounding YAML still has valid structure, which
        // means `{{ .Values.image.tag }}` reaches the query as filler. The actions are
        // parsed separately, and the values paths they name become references here so
        // that `fr refs`, `fr rename` and go-to-definition see what provenance always
        // could.
        if lang == Language::Helm {
            references.extend(values_references(source, parsed, path, &scope_at));
        }

        // One identifier can match several patterns (a call is also an identifier).
        // Keep the most specific kind per span so each use site appears exactly once.
        references.sort_by_key(|r| (r.span, reference_specificity(r.kind)));
        references.dedup_by_key(|r| r.span);

        Ok(FileFacts {
            path: path.to_path_buf(),
            // The caller knows whether the parse was clean; extraction does not set it.
            had_parse_errors: parsed.has_errors(),
            unreadable: None,
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
            path: self.path.map(|s| trim(s.text(source))).unwrap_or_default(),
            alias: self.alias.map(|s| s.text(source).to_string()),
            names,
            span,
            file: path.to_path_buf(),
            is_glob: self.is_glob,
        }
    }
}

/// Collapse definitions that describe the same identifier into one.
///
/// The survivor keeps the most specific kind, the widest full span (so a delete or
/// move takes the whole construct) and export visibility if any duplicate had it.
fn merge_duplicate_defs(defs: &mut Vec<RawDef>) {
    defs.sort_by_key(|d| {
        (
            d.name_span,
            kind_specificity(d.kind),
            std::cmp::Reverse(d.full_span.len()),
        )
    });

    let mut merged: Vec<RawDef> = Vec::with_capacity(defs.len());
    for def in defs.drain(..) {
        match merged.last_mut() {
            Some(previous) if previous.name_span == def.name_span => {
                previous.exported |= def.exported;
                if def.full_span.len() > previous.full_span.len() {
                    previous.full_span = def.full_span;
                }
            }
            _ => merged.push(def),
        }
    }
    *defs = merged;
}

/// Ranks symbol kinds from most to least specific, for duplicate resolution.
fn kind_specificity(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Method => 0,
        SymbolKind::Parameter => 1,
        SymbolKind::Field => 2,
        SymbolKind::Constant => 3,
        SymbolKind::Anchor | SymbolKind::Heading | SymbolKind::LinkDef => 4,
        SymbolKind::Selector | SymbolKind::Property | SymbolKind::ElementId => 5,
        SymbolKind::Function
        | SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Trait
        | SymbolKind::Interface
        | SymbolKind::Enum
        | SymbolKind::TypeAlias
        | SymbolKind::Module
        | SymbolKind::Block
        | SymbolKind::Key => 6,
        // The most generic kinds lose to anything more informative.
        SymbolKind::Variable => 7,
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
        let r = f
            .reference_at(impl_offset)
            .expect("impl type is a reference");
        assert_eq!(r.name, "S");
        assert_eq!(r.kind, ReferenceKind::Type);
    }

    #[test]
    fn name_spans_are_trimmed_of_quotes_and_padding() {
        // Whitespace padding is never part of a name.
        let src = r#"x = "  padded  ""#;
        assert_eq!(
            refine_name_span(Span::new(5, 15), src, Language::Rust).text(src),
            "padded"
        );

        // A matched quote pair belongs to the syntax, not the name: without this a
        // rename would write id=""new"".
        let quoted = "id=\"main\"";
        let span = Span::new(3, 9);
        assert_eq!(span.text(quoted), "\"main\"");
        assert_eq!(
            refine_name_span(span, quoted, Language::Html).text(quoted),
            "main"
        );

        // Padding around a Markdown heading title, and the optional closing marker
        // that this grammar keeps inside the heading content.
        let heading = "#   Title   ";
        let span = Span::new(1, heading.len());
        assert_eq!(
            refine_name_span(span, heading, Language::Markdown).text(heading),
            "Title"
        );
        let closed = "#   Title   #";
        let span = Span::new(1, closed.len());
        assert_eq!(
            refine_name_span(span, closed, Language::Markdown).text(closed),
            "Title"
        );
        // A heading that really ends in `#` keeps it: no space precedes the run.
        let sharp = "# C#";
        let span = Span::new(1, sharp.len());
        assert_eq!(
            refine_name_span(span, sharp, Language::Markdown).text(sharp),
            "C#"
        );
        // A link label is one node including its brackets.
        let label = "[label]: /a";
        assert_eq!(
            refine_name_span(Span::new(0, 7), label, Language::Markdown).text(label),
            "label"
        );

        // Unmatched quotes are left alone rather than half-trimmed.
        let odd = "\"unclosed";
        assert_eq!(
            refine_name_span(Span::new(0, odd.len()), odd, Language::Rust).text(odd),
            "\"unclosed"
        );
    }

    #[test]
    fn multi_valued_attributes_split_into_one_reference_per_token() {
        // `class="btn btn-primary"` names two CSS classes; renaming one must rewrite
        // only its own bytes.
        let src = "class=\"btn btn-primary\"";
        let value = Span::new(7, 22);
        assert_eq!(value.text(src), "btn btn-primary");

        let spans = split_value_spans(value, src);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text(src), "btn");
        assert_eq!(spans[1].text(src), "btn-primary");

        // A single-valued attribute stays one span.
        let single = "class=\"solo\"";
        let span = Span::new(7, 11);
        assert_eq!(split_value_spans(span, single).len(), 1);
    }

    #[test]
    fn repeated_tokens_get_distinct_spans() {
        let src = "a a";
        let spans = split_value_spans(Span::new(0, 3), src);
        assert_eq!(spans.len(), 2);
        assert_ne!(spans[0], spans[1], "each occurrence needs its own span");
        assert_eq!(spans[0], Span::new(0, 1));
        assert_eq!(spans[1], Span::new(2, 3));
    }

    #[test]
    fn duplicate_definitions_of_one_identifier_are_merged() {
        // Several patterns often match the same node — languages need one pattern
        // per parent context. Two symbols over identical bytes would make a rename
        // emit two edits at the same span, which the edit engine rejects.
        let mut defs = vec![
            RawDef {
                kind: SymbolKind::Variable,
                full_span: Span::new(0, 10),
                name_span: Span::new(4, 5),
                exported: false,
            },
            RawDef {
                kind: SymbolKind::Constant,
                full_span: Span::new(0, 20),
                name_span: Span::new(4, 5),
                exported: true,
            },
        ];
        merge_duplicate_defs(&mut defs);

        assert_eq!(defs.len(), 1);
        // The more specific kind wins, exports are preserved, and the widest span
        // survives so a delete takes the whole construct.
        assert_eq!(defs[0].kind, SymbolKind::Constant);
        assert!(defs[0].exported);
        assert_eq!(defs[0].full_span, Span::new(0, 20));
    }

    #[test]
    fn distinct_identifiers_are_not_merged() {
        let mut defs = vec![
            RawDef {
                kind: SymbolKind::Variable,
                full_span: Span::new(0, 10),
                name_span: Span::new(4, 5),
                exported: false,
            },
            RawDef {
                kind: SymbolKind::Variable,
                full_span: Span::new(11, 20),
                name_span: Span::new(15, 16),
                exported: false,
            },
        ];
        merge_duplicate_defs(&mut defs);
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn parameters_are_not_qualified_by_the_enclosing_type() {
        // A method's parameter belongs to the method, not the type: `Widget::self`
        // would be nonsense.
        let src = "struct S;\nimpl S {\n    fn m(&self, count: i32) {}\n}\n";
        let f = facts(Language::Rust, src);
        let param = f
            .symbols
            .iter()
            .find(|s| s.name == "count")
            .expect("parameter extracted");
        assert_eq!(param.kind, SymbolKind::Parameter);
        assert_eq!(param.qualifier, None);
        // The method itself is still qualified.
        let method = f.symbols.iter().find(|s| s.name == "m").unwrap();
        assert_eq!(method.qualifier.as_deref(), Some("S"));
    }

    #[test]
    fn no_identifier_yields_two_symbols_in_practice() {
        // Across a realistic sample, every definition must own a distinct identifier.
        let src = "struct S { field: i32 }\nimpl S {\n    fn m(&self, x: i32) -> i32 { let y = x; y }\n}\n";
        let f = facts(Language::Rust, src);
        let mut spans: Vec<_> = f.symbols.iter().map(|s| s.name_span).collect();
        let before = spans.len();
        spans.sort();
        spans.dedup();
        assert_eq!(
            before,
            spans.len(),
            "duplicate definitions: {:?}",
            f.symbols
        );
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
        assert_eq!(
            f.symbol_at(def_offset).map(|s| s.name.as_str()),
            Some("alpha")
        );

        let call_offset = src.rfind("alpha").unwrap() + 1;
        assert_eq!(
            f.reference_at(call_offset).map(|r| r.name.as_str()),
            Some("alpha")
        );
    }
}
