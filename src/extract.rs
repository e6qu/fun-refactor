//! Fact extraction: turn a parsed tree into symbols, references, scopes and imports.

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
        Language::Json => include_str!("../queries/json/facts.scm"),
        Language::Lean => include_str!("../queries/lean/facts.scm"),
        Language::Css => include_str!("../queries/css/facts.scm"),
        Language::Scss => include_str!("../queries/scss/facts.scm"),
        Language::Sass => include_str!("../queries/sass/facts.scm"),
        Language::Html => include_str!("../queries/html/facts.scm"),
        Language::Xml => include_str!("../queries/xml/facts.scm"),
        Language::Yaml | Language::Helm => include_str!("../queries/yaml/facts.scm"),
        Language::Markdown => include_str!("../queries/markdown/facts.scm"),
    })
}

/// Separates the halves of a query file that compile against different grammars.
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
        "data-attribute" => SymbolKind::DataAttribute,
        _ => return None,
    })
}

fn reference_kind(name: &str) -> Option<ReferenceKind> {
    Some(match name {
        "call" => ReferenceKind::Call,
        "identifier" => ReferenceKind::Identifier,
        "type" => ReferenceKind::Type,
        // `field.twin` is a second meaning sharing an ordinary span: a
        // shorthand initializer's identifier reads a local and writes a field.
        "field" | "field.twin" => ReferenceKind::Field,
        "string" | "selector" | "element-id" => ReferenceKind::StringRef,
        _ => return None,
    })
}

/// The kind of declaration a capture name says the reference can name.
fn reference_expects(name: &str) -> Option<SymbolKind> {
    Some(match name {
        "selector" => SymbolKind::Selector,
        "element-id" => SymbolKind::ElementId,
        "data-attribute" => SymbolKind::DataAttribute,
        _ => return None,
    })
}

/// Narrow a captured span to the bytes that name something.
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
    // The braced Sass syntax writes a namespaced name as one token: `theme.$brand`,
    // `theme.double`.
    if lang == Language::Scss {
        let text = span.text(source);
        if let Some(dot) = text.rfind('.') {
            let after = span.start + dot + 1;
            if after < span.end && text[..dot].chars().all(is_namespace_character) {
                return Span::new(after, span.end);
            }
        }
    }
    span
}

/// Whether the character may stand in a stylesheet namespace, which is an identifier.
fn is_namespace_character(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Strip the Markdown syntax the grammar leaves inside a name.
fn trim_markdown_syntax(span: Span, source: &str) -> Span {
    let text = span.text(source);
    if text.len() > 1 && text.starts_with('[') && text.ends_with(']') {
        return Span::new(span.start + 1, span.end - 1);
    }
    // CommonMark only reads a trailing run of `#` as a closing marker when a space
    // precedes it, which keeps `# C#` naming the heading `C#`.
    let without_marker = text.trim_end_matches('#');
    if without_marker.len() < text.len() && without_marker.ends_with(char::is_whitespace) {
        return Span::new(span.start, span.start + without_marker.trim_end().len());
    }
    span
}

/// Split a whitespace-separated attribute value into one span per token.
fn split_value_spans(span: Span, source: &str) -> Vec<Span> {
    // Commas separate as much as spaces do: `data-quiz="a,b,c"` names three hooks the way
    // `class="a b"` names two classes.
    let separator = |c: char| c.is_whitespace() || c == ',';
    let text = span.text(source);
    if !text.trim().contains(separator) {
        return vec![span];
    }
    let mut spans = Vec::new();
    let mut offset = 0;
    for token in text.split(separator).filter(|t| !t.is_empty()) {
        // `find` from the running offset keeps repeated tokens on distinct spans.
        if let Some(found) = text[offset..].find(token) {
            let start = span.start + offset + found;
            spans.push(Span::new(start, start + token.len()));
            offset += found + token.len();
        }
    }
    spans
}

/// The receiver a reference names, where it names one.
fn call_in_macro(root: Node<'_>, span: Span, source: &str) -> bool {
    if !inside_token_tree(root, span) {
        return false;
    }
    source[span.end..].starts_with('(')
}

/// Was this reference written as `something.name` inside a macro's token tree?
fn member_in_macro(root: Node<'_>, span: Span, source: &str) -> bool {
    inside_token_tree(root, span) && source[..span.start].trim_end().ends_with('.')
}

fn inside_token_tree(root: Node<'_>, span: Span) -> bool {
    let mut node = root.descendant_for_byte_range(span.start, span.end);
    while let Some(current) = node {
        if current.kind() == "token_tree" {
            return true;
        }
        node = current.parent();
    }
    false
}

fn receiver_of(root: Node<'_>, span: Span, source: &str, language: Language) -> Option<String> {
    // A macro body is tokens, so `myc::model::slug(x)` inside `assert_eq!` has no
    // scoped_identifier to read a path from.
    if language == Language::Rust && source[..span.start].trim_end().ends_with("::") {
        let before = &source[..span.start];
        if inside_token_tree(root, span) {
            let mut end = before.trim_end().len() - 2;
            let mut start = end;
            loop {
                let head = source[..start].trim_end();
                let word = head
                    .char_indices()
                    .rev()
                    .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
                    .last()
                    .map(|(i, _)| i);
                let Some(word_start) = word.filter(|i| *i < head.len()) else {
                    break;
                };
                start = word_start;
                let further = source[..start].trim_end();
                if further.ends_with("::") {
                    start = further.len() - 2;
                    end = end.max(start);
                    continue;
                }
                break;
            }
            let path = source[start..span.start]
                .trim_end()
                .trim_end_matches("::")
                .trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    // The indented Sass syntax gives the namespace a node of its own, under a `module`
    // field: `variable_module`, `call_expression` and `include_statement` all carry one.
    if let Some(node) = root.descendant_for_byte_range(span.start, span.end) {
        if let Some(module) = node.parent().and_then(|p| p.child_by_field_name("module")) {
            if Span::from(module) != span {
                return Some(Span::from(module).text(source).to_string());
            }
        }
    }

    // The braced syntax writes the namespace and the name as one token.
    if language == Language::Scss {
        let before = source[..span.start].strip_suffix('.').unwrap_or_default();
        let namespace: String = before
            .chars()
            .rev()
            .take_while(|c| is_namespace_character(*c))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if !namespace.is_empty() {
            return Some(namespace);
        }
    }

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
    // The receiver the member hangs off, where the grammar names it.
    const RECEIVER_FIELDS: &[&str] = &["object", "operand", "receiver"];
    let node = root.descendant_for_byte_range(span.start, span.end)?;
    let parent = node.parent()?;

    // Terraform writes its namespace as the first segment of a traversal: `var.azs`,
    // `local.azs`, `module.azs`, and each names a different declaration.
    if parent.kind() == "get_attr" {
        let expression = parent.parent()?;
        let mut cursor = expression.walk();
        let segments: Vec<Node> = expression.named_children(&mut cursor).collect();
        let first = segments.first()?;
        if first.kind() != "variable_expr" {
            return None;
        }
        let namespace = Span::from(*first).text(source);
        // `module.net.subnet_id` reaches a declaration in another directory, and the second
        // segment is the module call that says which.
        let position = segments.iter().position(|s| s.id() == parent.id())?;
        if namespace == "module" && position == 2 {
            let label = Span::from(segments[1]).text(source).trim_start_matches('.');
            return Some(format!("module.{label}"));
        }
        return Some(namespace.to_string());
    }

    // An argument of a Terraform `module` block names an input variable of the configuration
    // that block's `source` points at.
    if parent.kind() == "attribute" {
        if let Some(label) = module_block_label(parent, span, source) {
            return Some(format!("module.{label}"));
        }
    }

    if !MEMBER_SHAPES.contains(&parent.kind()) {
        return None;
    }

    for field in RECEIVER_FIELDS {
        if let Some(receiver) = parent.child_by_field_name(field) {
            // Only when this node really is the member of that receiver, not the receiver
            // itself.
            if Span::from(receiver) != span {
                return Some(Span::from(receiver).text(source).to_string());
            }
            return None;
        }
    }

    let mut cursor = parent.walk();
    let children: Vec<Node> = parent.named_children(&mut cursor).collect();
    // The member comes last; everything before it names the receiver.
    let last = children.last()?;
    if Span::from(*last) != span || children.len() < 2 {
        return None;
    }
    Some(Span::from(children[0]).text(source).to_string())
}

/// The label of the Terraform `module` block whose body holds this attribute.
fn module_block_label(attribute: Node<'_>, span: Span, source: &str) -> Option<String> {
    if Span::from(attribute.named_child(0)?) != span {
        return None;
    }
    let block = attribute
        .parent()
        .filter(|b| b.kind() == "body")?
        .parent()?;
    if block.kind() != "block" {
        return None;
    }
    let mut cursor = block.walk();
    let children: Vec<Node<'_>> = block.named_children(&mut cursor).collect();
    let keyword = children.first()?;
    if keyword.kind() != "identifier" || Span::from(*keyword).text(source) != "module" {
        return None;
    }
    let label = children.iter().find(|n| n.kind() == "string_lit")?;
    let mut cursor = label.walk();
    let text = label
        .named_children(&mut cursor)
        .find(|n| n.kind() == "template_literal")?;
    Some(Span::from(text).text(source).to_string())
}

/// What a string reference names: its fragment, itself, or nothing.
fn selector_name(span: Span, source: &str) -> Span {
    let text = span.text(source);
    let names_a_class = text.starts_with('.')
        && text.len() > 1
        && text[1..]
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    match names_a_class {
        true => Span::new(span.start + 1, span.end),
        false => span,
    }
}

fn link_destination(span: Span, source: &str, kind: ReferenceKind) -> Option<Span> {
    if kind != ReferenceKind::StringRef {
        return Some(span);
    }
    let text = span.text(source);
    if text.contains("://") {
        return None;
    }
    let Some(hash) = text.find('#') else {
        return Some(span);
    };
    let start = span.start + hash + 1;
    match start < span.end {
        true => Some(Span::new(start, span.end)),
        // `#` with nothing after it is a link to the top of the page.
        false => None,
    }
}

/// The `.Values` paths a Helm template names, as references to the values file.
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
                expects: None,
                receiver: segments.len().checked_sub(2).map(|i| segments[i].clone()),
                receiver_is_path: true,
                member_in_macro: false,
                twin: false,
            });
        }
    }
    out
}

/// The Kubernetes kinds whose `data` mapping other manifests read a key of.
const KUBERNETES_KEYED_KINDS: &[&str] = &["ConfigMap", "Secret"];

/// The mapping keys whose values hold a `ConfigMap` or `Secret` entry name.
const KUBERNETES_KEY_SELECTORS: &[(&str, &str)] =
    &[("configMapKeyRef", "ConfigMap"), ("secretKeyRef", "Secret")];

/// The mapping under a YAML value node, block or flow.
fn yaml_mapping<'a>(value: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = value.walk();
    let found = value
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "block_mapping" | "flow_mapping"));
    found
}

/// The pairs of a mapping node.
fn yaml_pairs<'a>(mapping: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = mapping.walk();
    mapping
        .named_children(&mut cursor)
        .filter(|child| matches!(child.kind(), "block_mapping_pair" | "flow_pair"))
        .collect()
}

/// A scalar's text and the bytes a rename would rewrite, with any quotes dropped.
fn yaml_scalar(node: Node<'_>, source: &str) -> Option<(String, Span)> {
    let mut span = Span::from(node);
    let mut text = span.text(source);
    // A quoted scalar has no inner-content node in this grammar, so this strips the quotes
    // here.
    for quote in ['"', '\''] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            span = Span::new(span.start + 1, span.end - 1);
            text = span.text(source);
        }
    }
    match text.is_empty() {
        true => None,
        false => Some((text.to_string(), span)),
    }
}

/// The scalar value of the pair named `key` in `mapping`.
fn yaml_entry(mapping: Node<'_>, key: &str, source: &str) -> Option<(String, Span)> {
    for pair in yaml_pairs(mapping) {
        let (name, _) = pair.child_by_field_name("key").and_then(|k| {
            let mut cursor = k.walk();
            let scalar = k.named_children(&mut cursor).next()?;
            yaml_scalar(scalar, source)
        })?;
        if name != key {
            continue;
        }
        let value = pair.child_by_field_name("value")?;
        let mut cursor = value.walk();
        let scalar = value.named_children(&mut cursor).next()?;
        return yaml_scalar(scalar, source);
    }
    None
}

/// The mapping value of the pair named `key` in `mapping`.
fn yaml_entry_mapping<'a>(mapping: Node<'a>, key: &str, source: &str) -> Option<Node<'a>> {
    for pair in yaml_pairs(mapping) {
        let named = pair.child_by_field_name("key").and_then(|k| {
            let mut cursor = k.walk();
            let scalar = k.named_children(&mut cursor).next()?;
            yaml_scalar(scalar, source).map(|(text, _)| text)
        });
        if named.as_deref() != Some(key) {
            continue;
        }
        return pair.child_by_field_name("value").and_then(yaml_mapping);
    }
    None
}

/// The Kubernetes objects a manifest declares, one per document.
fn kubernetes_declarations(root: Node<'_>, source: &str) -> Vec<crate::model::KubernetesObject> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for document in root.named_children(&mut cursor) {
        if document.kind() != "document" {
            continue;
        }
        let Some(mapping) = document
            .named_children(&mut document.walk())
            .find_map(yaml_mapping)
        else {
            continue;
        };
        let Some((kind, _)) = yaml_entry(mapping, "kind", source) else {
            continue;
        };
        if !KUBERNETES_KEYED_KINDS.contains(&kind.as_str()) {
            continue;
        }
        let Some(metadata) = yaml_entry_mapping(mapping, "metadata", source) else {
            continue;
        };
        let Some((name, name_span)) = yaml_entry(metadata, "name", source) else {
            continue;
        };
        out.push(crate::model::KubernetesObject {
            kind,
            name,
            name_span,
        });
    }
    out
}

/// The `configMapKeyRef` and `secretKeyRef` reads a manifest performs.
fn kubernetes_key_references(
    root: Node<'_>,
    source: &str,
    path: &Path,
    lang: Language,
    scope_at: &impl Fn(usize) -> crate::model::ScopeId,
) -> Vec<Reference> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
        if node.kind() != "block_mapping_pair" && node.kind() != "flow_pair" {
            continue;
        }
        let selector = node.child_by_field_name("key").and_then(|k| {
            let mut inner = k.walk();
            let scalar = k.named_children(&mut inner).next()?;
            yaml_scalar(scalar, source).map(|(text, _)| text)
        });
        let Some(selector) = selector else { continue };
        let Some((_, object_kind)) = KUBERNETES_KEY_SELECTORS
            .iter()
            .find(|(written, _)| *written == selector)
        else {
            continue;
        };
        let Some(mapping) = node.child_by_field_name("value").and_then(yaml_mapping) else {
            continue;
        };
        let (Some((object, _)), Some((key, span))) = (
            yaml_entry(mapping, "name", source),
            yaml_entry(mapping, "key", source),
        ) else {
            continue;
        };
        out.push(Reference {
            name: key,
            span,
            file: path.to_path_buf(),
            language: lang,
            scope: scope_at(span.start),
            target: None,
            confidence: Confidence::NameOnly,
            kind: ReferenceKind::StringRef,
            expects: Some(SymbolKind::Key),
            receiver: Some(format!("{object_kind}/{object}")),
            receiver_is_path: true,
            member_in_macro: false,
            twin: false,
        });
    }
    out
}

/// Was the receiver written as a path (`A::b`) and not against a value (`a.b`)?
fn receiver_is_path(root: Node<'_>, span: Span, source: &str, language: Language) -> bool {
    if language == Language::Rust
        && source[..span.start].trim_end().ends_with("::")
        && inside_token_tree(root, span)
    {
        return true;
    }
    root.descendant_for_byte_range(span.start, span.end)
        .and_then(|n| n.parent())
        .is_some_and(|p| p.kind().starts_with("scoped_") || p.kind() == "get_attr")
}

/// Ranks reference kinds from most to least specific.
fn reference_specificity(kind: ReferenceKind) -> u8 {
    match kind {
        ReferenceKind::Call => 0,
        ReferenceKind::Field => 1,
        ReferenceKind::Type => 2,
        ReferenceKind::StringRef => 3,
        ReferenceKind::Identifier => 4,
        ReferenceKind::Textual => 5,
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
        // A language whose grammar splits block and inline parsing (Markdown) hands over one
        // sub-tree per inline node.
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
                let mut binding_body: Option<Span> = None;
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
                    } else if cap_name == "binding.body" {
                        binding_body = Some(span);
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
                        let expects = cap_name
                            .strip_prefix("reference.")
                            .and_then(reference_expects);
                        raw_refs.push(RawRef {
                            kind,
                            span,
                            expects,
                            twin: cap_name.ends_with(".twin"),
                        });
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
                    } else if cap_name == "import.re-export" {
                        import_parts.re_export = true;
                    }
                }

                if let Some((kind, full_span)) = def {
                    // Nothing renames or references a definition with no identifier,
                    // so it is not a usable symbol.
                    if let Some(name_span) = name {
                        let name_span = refine_name_span(name_span, source, lang);
                        if !name_span.is_empty() {
                            let name_spans = match kind {
                                SymbolKind::DataAttribute => split_value_spans(name_span, source),
                                _ => vec![name_span],
                            };
                            for name_span in name_spans {
                                raw_defs.push(RawDef {
                                    kind,
                                    full_span,
                                    name_span,
                                    exported,
                                    binding_body,
                                });
                            }
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

        let scopes = build_scopes(&mut raw_scopes);

        let scope_at = |offset: usize| -> ScopeId {
            scopes
                .iter()
                .filter(|s| s.span.contains_offset(offset))
                .min_by_key(|s| s.span.len())
                .map(|s| s.id)
                .unwrap_or(ScopeId(0))
        };
        // A definition lives in the scope holding it, never the scope it opens.
        let declaration_scope = |name_offset: usize, own: Span| -> ScopeId {
            scopes
                .iter()
                .filter(|s| s.span.contains_offset(name_offset) && s.span != own)
                .min_by_key(|s| s.span.len())
                .map(|s| s.id)
                .unwrap_or(ScopeId(0))
        };

        // One identifier position is one definition.
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

            // A function defined inside a type-like container is a method.
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
                scope: d
                    .binding_body
                    .map(|body| scope_at(body.start))
                    .unwrap_or_else(|| declaration_scope(d.name_span.start, d.full_span)),
                container,
                qualifier,
                exported: d.exported,
            });
        }

        // Pass 4: references.
        let def_name_spans: std::collections::HashSet<Span> =
            symbols.iter().map(|s| s.name_span).collect();
        let mut references = Vec::new();
        for r in raw_refs {
            let refined = refine_name_span(r.span, source, lang);
            if def_name_spans.contains(&refined) || refined.is_empty() {
                continue;
            }
            let spans = if r.kind == ReferenceKind::StringRef {
                split_value_spans(refined, source)
            } else {
                vec![refined]
            };
            for span in spans {
                // `href="#top"`, `[x](#intro)`, `[x](guide.md#intro)`: every grammar here gives
                // the destination as one node, so this splits the fragment off here rather
                // than at resolution.
                let span = match link_destination(span, source, r.kind) {
                    Some(span) => span,
                    // An absolute URL names another document.
                    None => continue,
                };
                // A class written as a selector carries the `.` that says it is one.
                let span = selector_name(span, source);
                // A name applied to arguments is a call, whether or not the grammar said so.
                let kind = match call_in_macro(root, span, source) {
                    true => ReferenceKind::Call,
                    false => r.kind,
                };
                references.push(Reference {
                    name: span.text(source).to_string(),
                    span,
                    file: path.to_path_buf(),
                    language: lang,
                    scope: scope_at(span.start),
                    target: None,
                    // Resolution happens in the index, which can see other files.
                    confidence: Confidence::NameOnly,
                    kind,
                    expects: r.expects,
                    receiver: receiver_of(root, span, source, lang),
                    receiver_is_path: receiver_is_path(root, span, source, lang),
                    member_in_macro: member_in_macro(root, span, source),
                    twin: r.twin,
                });
            }
        }
        // Helm hides its references from the grammar.
        if lang == Language::Helm {
            references.extend(values_references(source, parsed, path, &scope_at));
        }

        // A Kubernetes manifest addresses another one by a name written as a value, which no
        // mapping-key query captures.
        let mut kubernetes_objects = Vec::new();
        if matches!(lang, Language::Yaml | Language::Helm) {
            kubernetes_objects = kubernetes_declarations(root, source);
            references.extend(kubernetes_key_references(
                root, source, path, lang, &scope_at,
            ));
        }

        // One identifier can match several patterns (a call is also an identifier).
        references.sort_by_key(|r| (r.span, r.twin, reference_specificity(r.kind)));
        references.dedup_by_key(|r| (r.span, r.twin));

        Ok(FileFacts {
            path: path.to_path_buf(),
            kubernetes_objects,
            gaps: parsed.gaps.clone(),
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
    binding_body: Option<Span>,
}

struct RawRef {
    kind: ReferenceKind,
    span: Span,
    expects: Option<SymbolKind>,
    /// A second meaning sharing an ordinary span: a shorthand initializer's
    /// identifier reads a local and writes a field, and both must survive.
    twin: bool,
}

#[derive(Default)]
struct ImportParts {
    span: Option<Span>,
    path: Option<Span>,
    alias: Option<Span>,
    names: Vec<Span>,
    originals: Vec<Span>,
    is_glob: bool,
    re_export: bool,
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
            re_export: self.re_export,
        }
    }
}

/// Collapse definitions that describe the same identifier into one.
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
                previous.binding_body = previous.binding_body.or(def.binding_body);
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
        SymbolKind::Selector
        | SymbolKind::Property
        | SymbolKind::ElementId
        | SymbolKind::DataAttribute => 5,
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

        // name_span must cover only the identifier.
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

        // Leave unmatched quotes alone rather than half-trim them.
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
        // Several patterns often match the same node, languages need one pattern per parent
        // context.
        let mut defs = vec![
            RawDef {
                kind: SymbolKind::Variable,
                full_span: Span::new(0, 10),
                name_span: Span::new(4, 5),
                exported: false,
                binding_body: None,
            },
            RawDef {
                kind: SymbolKind::Constant,
                full_span: Span::new(0, 20),
                name_span: Span::new(4, 5),
                exported: true,
                binding_body: None,
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
                binding_body: None,
            },
            RawDef {
                kind: SymbolKind::Variable,
                full_span: Span::new(11, 20),
                name_span: Span::new(15, 16),
                exported: false,
                binding_body: None,
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
        // The method itself keeps its qualifier.
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
