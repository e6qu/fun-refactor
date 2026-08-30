//! Pattern-based restructuring: rewrite code matching a shape.

use super::Refusal;
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tree_sitter::Node;

/// A restructuring worked out but not applied.
#[derive(Debug)]
pub struct RestructurePlan {
    pub pattern: String,
    pub template: String,
    pub edits: EditSet,
    /// One entry per rewritten site: file and the text that matched.
    pub matches: Vec<(PathBuf, String)>,
    /// Sites the pattern matched and the rewrite left alone: file and byte offset.
    pub skipped_with_comments: Vec<(PathBuf, usize)>,
}

/// Where this pattern matches, rewriting nothing.
pub fn locate(index: &Index, language: Language, pattern: &str) -> Result<Vec<(PathBuf, Span)>> {
    let parsers = Parsers::new();
    let encoded = encode_metavariables(pattern);
    let shapes = fragment_shapes(&parsers, language, &encoded, pattern)?;
    let compiled = compile_shapes(&shapes, pattern)?;

    let mut found: Vec<(Vec<(PathBuf, Span)>, usize)> =
        compiled.iter().map(|_| (Vec::new(), 0)).collect();
    let mut hit = compiled.len() - 1;
    for (path, info) in index.files() {
        if info.language != language {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let parsed = parsers.parse(language, &source)?;
        for (shape, pattern) in compiled.iter().enumerate().take(hit + 1) {
            for found_here in find_matches(&parsed, &source, pattern) {
                found[shape].0.push((path.clone(), found_here.span));
                if !found_here.tokens {
                    found[shape].1 += 1;
                    hit = hit.min(shape);
                }
            }
        }
    }
    let chosen = found
        .iter()
        .position(|(_, structural)| *structural > 0)
        .or_else(|| found.iter().position(|(sites, _)| !sites.is_empty()));
    Ok(match chosen {
        Some(shape) => found.swap_remove(shape).0,
        None => Vec::new(),
    })
}

/// Rewrite every occurrence of `pattern` as `template` across the workspace.
pub fn apply(
    index: &Index,
    language: Language,
    pattern: &str,
    template: &str,
) -> Result<RestructurePlan> {
    crate::capabilities::record(crate::capabilities::Capability::Restructure, language);
    let parsers = Parsers::new();

    // Helm's `{{ ...
    if language == Language::Helm && pattern.contains("{{") {
        return Err(Refusal::Unsupported {
            operation: "a pattern containing a '{{ ... }}' template action".to_string(),
            language,
            because: "a mask blanks those bytes before the YAML parse, so they carry no \
                      structure to match",
        }
        .into());
    }

    // `$X` is invalid syntax in most languages, so the pattern becomes ordinary identifiers
    // before parsing.
    let original_pattern = pattern.to_string();
    let encoded = encode_metavariables(pattern);
    let shapes = fragment_shapes(&parsers, language, &encoded, pattern)?;
    let compiled = compile_shapes(&shapes, pattern)?;

    // A metavariable the pattern never binds has nothing to substitute, so the template would
    // emit the literal text `$Y`.
    let bound = metavariable_names(pattern);
    let unbound: Vec<String> = metavariable_names(template)
        .into_iter()
        .filter(|name| !bound.contains(name))
        .collect();
    if !unbound.is_empty() {
        let listed = unbound
            .iter()
            .map(|n| format!("${n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let known = if bound.is_empty() {
            "the pattern binds none".to_string()
        } else {
            let names: Vec<String> = bound.iter().map(|n| format!("${n}")).collect();
            format!("the pattern binds {}", names.join(", "))
        };
        return Err(Refusal::InvalidName {
            name: template.to_string(),
            reason: format!(
                "{listed} is not bound by the pattern, so there is nothing to put there \
. {known}. Write `$${}` for a literal dollar sign.",
                unbound[0]
            ),
        }
        .into());
    }

    let groups = crate::refactor::inline::groups_with_parentheses(language);
    // Bracket a metavariable only where the template binds it more tightly than the pattern
    // already did.
    let tight = match groups {
        true => {
            let by_template = tightly_bound_metavariables(&parsers, language, template);
            let by_pattern = tightly_bound_metavariables(&parsers, language, pattern);
            by_template.difference(&by_pattern).cloned().collect()
        }
        false => HashSet::new(),
    };
    let template_binds = groups && template_is_an_operator_expression(&parsers, language, template);

    // One fragment can be more than one shape in one language, and only the target says which.
    let mut found: Vec<Found> = compiled.iter().map(|_| Found::default()).collect();
    let mut hit = compiled.len() - 1;

    for (path, info) in index.files() {
        if info.language != language {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let parsed = parsers.parse(language, &source)?;

        for (shape, compiled) in compiled.iter().enumerate().take(hit + 1) {
            let into = &mut found[shape];
            for site in find_matches(&parsed, &source, compiled) {
                let span = site.span;
                // The template places every bound piece and says nothing about a comment
                // written between the pattern's own tokens.
                if let Some(first) = site.stranded_comments.first() {
                    into.skipped_with_comments.push((path.clone(), first.start));
                    if !site.tokens {
                        into.structural += 1;
                        hit = hit.min(shape);
                    }
                    continue;
                }
                let mut replacement = substitute(template, &site.bindings, &tight);
                // The match sat where a call sat, and the replacement is an operator
                // expression.
                if template_binds && matched_in_a_tight_place(&parsed, span) {
                    replacement = format!("({replacement})");
                }
                // Skip a rewrite that changes nothing, and one that moves only whitespace.
                if same_but_for_layout(&replacement, span.text(&source)) {
                    continue;
                }
                into.matches
                    .push((path.clone(), span.text(&source).to_string()));
                into.edits.add(
                    path.clone(),
                    Edit::new(span, replacement, "restructure".to_string()),
                );
                if !site.tokens {
                    into.structural += 1;
                    hit = hit.min(shape);
                }
            }
        }
    }

    // The nodes a pattern matched choose its shape.
    let chosen = found
        .iter()
        .position(|shape| shape.structural > 0)
        .or_else(|| {
            found.iter().position(|shape| {
                !shape.matches.is_empty() || !shape.skipped_with_comments.is_empty()
            })
        });
    let chosen = match chosen {
        Some(shape) => found.swap_remove(shape),
        None => Found::default(),
    };

    Ok(RestructurePlan {
        // Report the pattern the caller wrote, leaving the wrapped and encoded form
        // to the parser.
        pattern: original_pattern,
        template: template.to_string(),
        edits: chosen.edits,
        matches: chosen.matches,
        skipped_with_comments: chosen.skipped_with_comments,
    })
}

/// What one shape of the pattern found across the workspace.
#[derive(Default)]
struct Found {
    edits: EditSet,
    matches: Vec<(PathBuf, String)>,
    skipped_with_comments: Vec<(PathBuf, usize)>,
    /// How many of those sites were nodes rather than runs of macro tokens.
    structural: usize,
}

/// Reports whether the matched span sits as an operand of something that binds.
fn matched_in_a_tight_place(parsed: &Parsed, span: Span) -> bool {
    parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)
        .and_then(|node| node.parent())
        .is_some_and(|parent| binds_its_operands(parent.kind()))
}

/// A compiled pattern: the fragment's node, its source text, and whether the wrapper
/// added trailing punctuation a match must not swallow.
struct Pattern<'a> {
    root: Node<'a>,
    source: &'a str,
    trim_trailing: bool,
    /// The separator the pattern carries, and the node it leaves out.
    separator: Option<char>,
}

/// One way the fragment parses: the wrapper's parse and where the fragment sits in it.
struct Shape {
    parsed: Parsed,
    source: String,
    offset: usize,
    len: usize,
    trim_trailing: bool,
    separator: Option<char>,
}

impl Shape {
    /// The fragment's node inside this shape's parse.
    fn compile(&self) -> Option<Pattern<'_>> {
        Some(Pattern {
            root: fragment_root(&self.parsed, self.offset, self.len)?,
            source: &self.source,
            trim_trailing: self.trim_trailing,
            separator: self.separator,
        })
    }
}

/// The fragment's node in each wrapper that accepts it, most specific first.
fn fragment_shapes(
    parsers: &Parsers,
    language: Language,
    encoded: &str,
    display: &str,
) -> Result<Vec<Shape>> {
    let mut shapes = Vec::new();
    for (prefix, suffix) in fragment_wrappers(language) {
        let source = format!("{prefix}{encoded}{suffix}");
        let parsed = parsers.parse(language, &source)?;
        if parsed.has_errors() {
            continue;
        }
        let offset = prefix.len();
        let end = offset + encoded.len();
        let Some(root) = fragment_root(&parsed, offset, encoded.len()) else {
            continue;
        };
        // The node must begin where the fragment begins, since a node starting inside the
        // wrapper belongs to the wrapper.
        if root.start_byte() != offset || root.end_byte() > end + suffix.len() {
            continue;
        }
        // Spell a list member with the separator that places it.
        let separator = if root.end_byte() < end {
            match encoded[root.end_byte() - offset..].trim() {
                "," => Some(','),
                ";" => Some(';'),
                _ => continue,
            }
        } else {
            None
        };
        let trim_trailing = root.end_byte() > end;
        shapes.push(Shape {
            parsed,
            source,
            offset,
            len: encoded.len(),
            trim_trailing,
            separator,
        });
    }
    if shapes.is_empty() {
        // Name the mistake, which is nearly always a pattern that is not a whole piece of code.
        return Err(Refusal::Declined {
            detail: format!("'{display}' is not valid {language}; check for unbalanced brackets."),
        }
        .into());
    }
    Ok(shapes)
}

/// The pattern node of every shape, refusing the ones that would match everything.
fn compile_shapes<'a>(shapes: &'a [Shape], display: &str) -> Result<Vec<Pattern<'a>>> {
    // Two wrappers can hold the fragment the same way.
    let mut seen = HashSet::new();
    let compiled: Vec<Pattern<'a>> = shapes
        .iter()
        .filter_map(Shape::compile)
        // A pattern that is only a metavariable would match every node in the file.
        .filter(|pattern| metavariable(pattern.root, pattern.source).is_none())
        .filter(|pattern| {
            let span = Span::from(pattern.root);
            seen.insert((
                shape_signature(pattern.root),
                span.text(pattern.source).to_string(),
                pattern.trim_trailing,
                pattern.separator,
            ))
        })
        .collect();
    if compiled.is_empty() {
        return Err(Refusal::InvalidName {
            name: display.to_string(),
            reason: "a pattern that is only a metavariable would match everything".into(),
        }
        .into());
    }
    Ok(compiled)
}

/// A tree spelled as the kinds it holds.
fn shape_signature(node: Node<'_>) -> String {
    let mut signature = String::new();
    write_signature(node, &mut signature);
    signature
}

fn write_signature(node: Node<'_>, out: &mut String) {
    out.push('(');
    out.push_str(node.kind());
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        write_signature(child, out);
    }
    out.push(')');
}

/// The fragment inside the first wrapper that accepts it.
fn parse_fragment(
    parsers: &Parsers,
    language: Language,
    encoded: &str,
    display: &str,
) -> Result<Shape> {
    let mut shapes = fragment_shapes(parsers, language, encoded, display)?;
    Ok(shapes.remove(0))
}

/// Minimal syntax that makes a fragment parse as a whole file, most specific first.
fn fragment_wrappers(language: Language) -> &'static [(&'static str, &'static str)] {
    match language {
        // An expression or statement inside a function body, or a whole item.
        Language::Rust => &[
            ("fn __fr_pattern() { ", "; }"),
            ("", "\n"),
            ("enum FrPattern { ", " }"),
            ("struct FrPattern { ", " }"),
            ("fn __fr_pattern() { match __fr_subject { ", " } }"),
            ("fn __fr_pattern() { match __fr_subject { ", " => () } }"),
        ],
        Language::Go => &[
            ("package p\n\nfunc __frPattern() {\n", "\n}\n"),
            ("package p\n\n", "\n"),
            ("package p\n\ntype FrPattern struct {\n", "\n}\n"),
            (
                "package p\n\nfunc __frPattern() {\n\tswitch {\n",
                "\n\t}\n}\n",
            ),
        ],
        Language::Zig => &[
            ("pub fn __fr_pattern() void {\n", ";\n}\n"),
            ("", "\n"),
            ("const FrPattern = struct {\n", "\n};\n"),
            (
                "pub fn __fr_pattern() void {\n    switch (x) {\n",
                "\n    }\n}\n",
            ),
        ],
        // A statement inside a method inside a class, a member inside a class, or a whole type.
        Language::Java => &[
            ("class FrPattern { void frPattern() {\n", ";\n} }\n"),
            ("class FrPattern {\n", "\n}\n"),
            ("", "\n"),
            ("enum FrPattern { ", " }"),
            (
                "class FrPattern { void frPattern() { switch (x) {\n",
                "\n} } }\n",
            ),
        ],
        // Python and the JS family accept a bare expression statement.
        Language::TypeScript | Language::Tsx => &[
            ("", "\n"),
            ("interface FrPattern {\n", "\n}\n"),
            ("class FrPattern {\n", "\n}\n"),
            ("switch (x) {\n", "\n}\n"),
            ("const __frPattern = {\n", "\n};\n"),
        ],
        Language::Python => &[
            ("", "\n"),
            ("class FrPattern:\n    ", "\n"),
            ("__fr_pattern = {\n", "\n}\n"),
        ],
        // A bash command, pipeline or compound statement is already a whole script.
        Language::Bash => &[("", "\n")],
        // An attribute or block stands alone; a bare expression needs an attribute.
        Language::Hcl => &[("", "\n"), ("__fr_pattern = ", "\n")],
        // A mapping pair, a sequence item or a nested mapping is a whole document.
        Language::Yaml | Language::Helm => &[("", "\n")],
        // A rule set stands alone; a declaration needs a rule; a selector needs a body.
        Language::Css | Language::Scss => &[
            ("", "\n"),
            ("__fr_pattern {\n", ";\n}\n"),
            ("", " { __fr_pattern: 0 }\n"),
        ],
        // The same three shapes, written the way the indented syntax writes them.
        Language::Sass => &[
            ("", "\n"),
            ("__fr_pattern\n  ", "\n"),
            ("", "\n  __fr_pattern: 0\n"),
        ],
        // An element is a document on its own; several siblings need a root.
        Language::Html => &[("", "\n"), ("<html><body>", "</body></html>\n")],
        Language::Xml => &[("", "\n"), ("<__fr_pattern>", "</__fr_pattern>\n")],
        Language::Markdown => &[("", "\n")],
        // A fragment of JSON is a value, a member, or a whole document.
        Language::Json => &[("", "\n"), ("{\"__fr_pattern\": ", "}\n"), ("[", "]\n")],
        // A term, a whole declaration, or the body of one.
        Language::Lean => &[
            ("", "\n"),
            ("def __frPattern := ", "\n"),
            ("example : Nat := ", "\n"),
        ],
    }
}

/// The outermost node covering exactly the fragment inside its wrapper.
fn fragment_root<'a>(parsed: &'a Parsed, offset: usize, len: usize) -> Option<Node<'a>> {
    let span = Span::new(offset, offset + len);
    let mut node = parsed.descendant_at(span.start, span.end)?;

    // Widen through wrappers of identical extent, then narrow past statement
    // containers the wrapper introduced.
    while let Some(parent) = node.parent() {
        if Span::from(parent) == Span::from(node) {
            node = parent;
        } else {
            break;
        }
    }
    // Descend through single-child wrappers: a node of identical extent adds nothing to the
    // shape, and a statement container is punctuation the wrapper asked for.
    loop {
        let named: Vec<Node> = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        };
        // The fragment can sit inside a container the wrapper opened, such as the braces
        // holding an enum's variants.
        if node.start_byte() < span.start {
            return named.iter().find(|c| c.start_byte() == span.start).copied();
        }
        let only_child = named.len() == 1
            && named[0].start_byte() == node.start_byte()
            && (Span::from(named[0]) == Span::from(node) || node.kind().contains("statement"));
        if !only_child {
            return Some(node);
        }
        node = named[0];
    }
}

/// The identifier prefix a metavariable is encoded as, chosen to be valid in every
/// supported language and vanishingly unlikely to occur in real code.
const META: &str = "FrMeta";

/// Rewrite `$NAME` into an ordinary identifier so the pattern parses.
fn encode_metavariables(pattern: &str) -> String {
    let re = Regex::new(r"\$(\$?)([A-Za-z_][A-Za-z0-9_]*)").expect("static regex");
    re.replace_all(pattern, |caps: &regex::Captures<'_>| {
        let name = &caps[2];
        if caps[1].is_empty() {
            format!("{META}{name}")
        } else {
            format!("${name}")
        }
    })
    .into_owned()
}

/// A metavariable as it appears in a pattern.
struct Metavariable {
    name: String,
    /// The node's text is the metavariable *in quotes*, so the value it binds is the
    /// target's text with its quotes removed.
    quoted: bool,
}

/// If `node` is an encoded metavariable, which one.
fn metavariable(node: Node<'_>, source: &str) -> Option<Metavariable> {
    // Some grammars fold padding into a node.
    let text = Span::from(node).text(source).trim();
    if let Some(name) = meta_name(text) {
        return Some(Metavariable {
            name,
            quoted: false,
        });
    }
    if node.named_child_count() == 0 {
        if let Some(inner) = strip_quotes(text) {
            if let Some(name) = meta_name(inner) {
                return Some(Metavariable { name, quoted: true });
            }
        }
    }
    None
}

/// The metavariable name inside an encoded identifier, when the text is one.
fn meta_name(text: &str) -> Option<String> {
    let name = text.strip_prefix(META)?;
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_string())
    } else {
        None
    }
}

/// The text inside a matched pair of quotes, where quotes surround it.
fn strip_quotes(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if first != '"' && first != '\'' {
        return None;
    }
    let rest = text.strip_prefix(first)?;
    rest.strip_suffix(first)
}

/// The text a metavariable binds when it matches `node`.
fn binding_text(node: Node<'_>, source: &str, quoted: bool) -> String {
    let text = Span::from(node).text(source).trim();
    if quoted {
        strip_quotes(text).unwrap_or(text).to_string()
    } else {
        text.to_string()
    }
}

/// One place the pattern matched.
struct Match {
    span: Span,
    /// Whether this site is a run of macro tokens rather than a node.
    tokens: bool,
    bindings: HashMap<String, String>,
    /// Comments inside the match that no metavariable binding carries over.
    stranded_comments: Vec<Span>,
}

/// Every match of the pattern in the file, with its metavariable bindings.
fn find_matches(parsed: &Parsed, source: &str, pattern: &Pattern<'_>) -> Vec<Match> {
    let mut results = Vec::new();
    // Markdown's inline content forms a sub-tree of its own, holding its links and emphasis.
    let mut stack: Vec<Node> = parsed.roots().collect();
    let mut cursor = parsed.root().walk();

    while let Some(node) = stack.pop() {
        let mut bindings = HashMap::new();
        let mut bound = Vec::new();
        if matches_node(
            node,
            source,
            pattern.root,
            pattern.source,
            &mut bindings,
            &mut bound,
        ) {
            let span = match_span(node, source, pattern);
            if !touches_template_action(&parsed.masked_spans, span) {
                results.push(Match {
                    span,
                    tokens: false,
                    bindings,
                    stranded_comments: stranded_comments(node, span, &bound),
                });
                // Never rewrite inside a span this pass already rewrote.
                continue;
            }
        }
        // A macro's arguments are a bag of tokens.
        if is_macro_tokens(node) {
            results.extend(token_matches(node, source, pattern));
        }
        stack.extend(node.named_children(&mut cursor));
    }

    results.sort_by_key(|found| found.span);
    results.dedup_by_key(|found| found.span);
    // Two matches that overlap without matching each other cannot both rewrite.
    let mut kept: Vec<Match> = Vec::new();
    for found in results {
        if kept
            .last()
            .is_some_and(|last| last.span.end > found.span.start)
        {
            continue;
        }
        kept.push(found);
    }
    kept
}

/// Reports whether this node is a macro, whose body is a run of tokens.
fn is_macro_tokens(node: Node<'_>) -> bool {
    matches!(node.kind(), "macro_invocation" | "macro_definition")
}

/// The tokens of a node: its leaves in order, comments left out.
fn leaves<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut tokens = Vec::new();
    let mut stack = vec![node];
    let mut cursor = node.walk();
    while let Some(current) = stack.pop() {
        if is_a_comment(current) {
            continue;
        }
        if current.child_count() == 0 {
            tokens.push(current);
            continue;
        }
        let mut children: Vec<Node> = current.children(&mut cursor).collect();
        children.reverse();
        stack.extend(children);
    }
    tokens.sort_by_key(|token| token.start_byte());
    tokens
}

/// Every run of macro tokens the pattern's own tokens match.
fn token_matches(node: Node<'_>, source: &str, pattern: &Pattern<'_>) -> Vec<Match> {
    let wanted = leaves(pattern.root);
    // A one-token pattern is an ordinary node inside the run, and the walk reaches it.
    if wanted.len() < 2 {
        return Vec::new();
    }
    let tokens = leaves(node);
    let mut found = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let mut bindings = HashMap::new();
        let mut bound = Vec::new();
        let end = match_tokens(
            &tokens[at..],
            &wanted,
            source,
            pattern.source,
            &mut bindings,
            &mut bound,
        );
        match end {
            Some(end) if end > 0 => {
                let span = Span::new(tokens[at].start_byte(), tokens[at + end - 1].end_byte());
                found.push(Match {
                    span,
                    tokens: true,
                    stranded_comments: stranded_comments(node, span, &bound),
                    bindings,
                });
                at += end;
            }
            _ => at += 1,
        }
    }
    found
}

/// Match the pattern's tokens against the front of `tokens`, returning how many it took.
fn match_tokens(
    tokens: &[Node<'_>],
    wanted: &[Node<'_>],
    source: &str,
    pattern_source: &str,
    bindings: &mut HashMap<String, String>,
    bound: &mut Vec<Span>,
) -> Option<usize> {
    let text = |node: &Node<'_>, of: &str| -> String { Span::from(*node).text(of).to_string() };
    let mut at = 0;
    for (index, want) in wanted.iter().enumerate() {
        if let Some(meta) = metavariable(*want, pattern_source) {
            let next = wanted.get(index + 1).map(|node| text(node, pattern_source));
            let start = at;
            let mut depth = 0i32;
            loop {
                let here = text(tokens.get(at)?, source);
                if depth == 0 && at > start && next.as_deref() == Some(here.as_str()) {
                    break;
                }
                depth += nesting(&here);
                if depth < 0 {
                    return None;
                }
                at += 1;
                if next.is_none() && depth == 0 {
                    break;
                }
            }
            let span = Span::new(tokens[start].start_byte(), tokens[at - 1].end_byte());
            let text = span.text(source).trim();
            let text = match meta.quoted {
                true => strip_quotes(text).unwrap_or(text),
                false => text,
            };
            match bindings.get(&meta.name) {
                Some(existing) if existing != text => return None,
                Some(_) => {}
                None => {
                    bindings.insert(meta.name, text.to_string());
                }
            }
            bound.push(span);
            continue;
        }
        if text(tokens.get(at)?, source) != text(want, pattern_source) {
            return None;
        }
        at += 1;
    }
    Some(at)
}

/// What a token does to bracket nesting.
fn nesting(token: &str) -> i32 {
    match token {
        "(" | "[" | "{" => 1,
        ")" | "]" | "}" => -1,
        _ => 0,
    }
}

/// Comments inside a match that no metavariable binding would carry over.
fn stranded_comments(node: Node<'_>, span: Span, bound: &[Span]) -> Vec<Span> {
    let mut stranded = Vec::new();
    let mut stack = vec![node];
    let mut cursor = node.walk();
    while let Some(current) = stack.pop() {
        let here = Span::from(current);
        if here.start >= span.end || here.end <= span.start {
            continue;
        }
        if is_a_comment(current) {
            if !bound
                .iter()
                .any(|b| b.start <= here.start && here.end <= b.end)
            {
                stranded.push(here);
            }
            continue;
        }
        stack.extend(current.children(&mut cursor));
    }
    stranded.sort_by_key(|comment| comment.start);
    stranded
}

/// Reports whether a match runs into a Helm `{{ ...
fn touches_template_action(actions: &[Span], span: Span) -> bool {
    actions
        .iter()
        .any(|action| action.start <= span.end && span.start <= action.end)
}

/// The span a match rewrites.
fn match_span(node: Node<'_>, source: &str, pattern: &Pattern<'_>) -> Span {
    let start = node.start_byte();
    let mut end = node.end_byte();
    if pattern.trim_trailing {
        let mut cursor = node.walk();
        if let Some(last) = node.named_children(&mut cursor).last() {
            if last.end_byte() > start {
                end = last.end_byte();
            }
        }
    }
    // A node can carry the whitespace that follows it.
    let end = source[..end].trim_end().len().max(start);
    match pattern.separator {
        Some(separator) => Span::new(start, separator_end(source, end, separator)),
        None => Span::new(start, end),
    }
}

/// The byte after the separator that follows `from`, or `from` where none does.
fn separator_end(source: &str, from: usize, separator: char) -> usize {
    let mut at = from;
    for c in source[from..].chars() {
        if c == separator {
            return at + c.len_utf8();
        }
        if c == ' ' || c == '\t' {
            at += c.len_utf8();
            continue;
        }
        return from;
    }
    from
}

/// Reports whether this node is a comment.
fn is_a_comment(node: Node<'_>) -> bool {
    node.kind().to_ascii_lowercase().contains("comment")
}

/// The named children of a node, without the comments interleaved among them.
fn shape_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !is_a_comment(*child))
        .collect()
}

/// Structural match of one node against one pattern node.
fn matches_node(
    node: Node<'_>,
    source: &str,
    pattern: Node<'_>,
    pattern_source: &str,
    bindings: &mut HashMap<String, String>,
    bound: &mut Vec<Span>,
) -> bool {
    // A metavariable matches any node, but must bind consistently: `$A + $A`
    // requires both sides to be the same text.
    if let Some(meta) = metavariable(pattern, pattern_source) {
        let text = binding_text(node, source, meta.quoted);
        let same = match bindings.get(&meta.name) {
            Some(existing) => *existing == text,
            None => {
                bindings.insert(meta.name, text);
                true
            }
        };
        if same {
            bound.push(Span::from(node));
        }
        return same;
    }

    if node.kind() != pattern.kind() {
        return false;
    }

    let pattern_children = shape_children(pattern);

    // A leaf in the pattern must match the target's text exactly.
    if pattern_children.is_empty() {
        return Span::from(node).text(source) == Span::from(pattern).text(pattern_source);
    }

    let node_children = shape_children(node);
    if node_children.len() != pattern_children.len() {
        return false;
    }

    node_children
        .iter()
        .zip(pattern_children.iter())
        .all(|(n, p)| matches_node(*n, source, *p, pattern_source, bindings, bound))
}

/// Turn encoded metavariables back into `$NAME` for display.
#[cfg_attr(not(test), allow(dead_code))]
fn decode_metavariables(encoded: &str) -> String {
    encoded.replace(META, "$")
}

/// Every metavariable a pattern or template names, in order of first appearance.
fn metavariable_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '$' {
            continue;
        }
        let rest = &text[i + 1..];
        if let Some(after) = rest.strip_prefix('$') {
            if after.starts_with(|ch: char| ch.is_alphabetic() || ch == '_') {
                chars.next();
                continue;
            }
        }
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Node kinds that bind their operands, so a captured expression dropped into one keeps its own
/// shape only if it is bracketed.
fn binds_its_operands(kind: &str) -> bool {
    const TIGHT: &[&str] = &[
        "binary",
        "unary",
        "boolean_operator",
        "comparison",
        "not_operator",
        "field",
        "member",
        "selector",
        "attribute",
        "subscript",
        "index",
        "range",
        "await",
        "negated",
    ];
    TIGHT.iter().any(|tight| kind.contains(tight))
}

/// Metavariables the template places where an operator will bind them.
fn tightly_bound_metavariables(
    parsers: &Parsers,
    language: Language,
    template: &str,
) -> HashSet<String> {
    let mut tight = HashSet::new();
    let encoded = encode_metavariables(template);
    let Ok(shape) = parse_fragment(parsers, language, &encoded, template) else {
        // The reparse check downstream reports an unparseable template, so add no
        // brackets here.
        return tight;
    };
    let Some(compiled) = shape.compile() else {
        return tight;
    };
    let (root, source) = (compiled.root, compiled.source);

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(meta) = metavariable(node, source) {
            if node.parent().is_some_and(|p| binds_its_operands(p.kind())) {
                tight.insert(meta.name);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    tight
}

/// Reports whether the template binds its operands, letting the *match site's* context
/// reassociate it.
fn template_is_an_operator_expression(
    parsers: &Parsers,
    language: Language,
    template: &str,
) -> bool {
    let encoded = encode_metavariables(template);
    let Ok(shape) = parse_fragment(parsers, language, &encoded, template) else {
        return false;
    };
    shape
        .compile()
        .is_some_and(|compiled| binds_its_operands(compiled.root.kind()))
}

/// Reports whether this text is a single thing, needing no brackets wherever it lands.
fn is_atomic(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let mut depth = 0i32;
    let mut previous = ' ';
    let mut in_string = false;
    let mut escaped = false;
    for character in trimmed.chars() {
        // Treat a quoted string as one thing, whatever it holds.
        if in_string {
            match (escaped, character) {
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => escaped = false,
            }
            previous = character;
            continue;
        }
        if character == '"' {
            in_string = true;
            previous = character;
            continue;
        }
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ' ' if depth == 0 => {
                // A space at the top level marks more than one thing: `x + 1`,
                // `not ready`, `a as i64`.
                return false;
            }
            '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!' | '&' | '|' | '^' | '?'
                if depth == 0 && previous != ' ' =>
            {
                return false;
            }
            _ => {}
        }
        previous = character;
    }
    true
}

/// Reports whether two texts differ only in what the author chose and the tool did not.
fn same_but_for_layout(a: &str, b: &str) -> bool {
    fn reduced(text: &str) -> String {
        let dense: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let mut out = String::with_capacity(dense.len());
        let mut chars = dense.chars().peekable();
        while let Some(c) = chars.next() {
            if c == ',' && matches!(chars.peek(), Some(')' | ']' | '}')) {
                continue;
            }
            out.push(c);
        }
        out
    }
    reduced(a) == reduced(b)
}

/// Bracket a binding the template will bind more tightly than its source did.
fn grouped(value: &str, needs_grouping: bool) -> String {
    match needs_grouping && !is_atomic(value) {
        true => format!("({value})"),
        false => value.to_string(),
    }
}

/// Replace `$NAME` in the template with its binding.
fn substitute(
    template: &str,
    bindings: &HashMap<String, String>,
    tight: &HashSet<String>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let rest = &template[i + 1..];
        // `$$NAME` is an escaped literal `$NAME`, matching the pattern's escape.
        if let Some(after) = rest.strip_prefix('$') {
            if after.starts_with(|ch: char| ch.is_alphabetic() || ch == '_') {
                out.push('$');
                chars.next();
                continue;
            }
        }
        // Read the longest identifier following the sigil.
        let len = rest
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..len];
        match bindings.get(name) {
            Some(value) => {
                out.push_str(&grouped(value, tight.contains(name)));
                for _ in 0..len {
                    chars.next();
                }
            }
            // An unbound metavariable stays as written, so a typo in the template
            // shows up in the diff.
            None => out.push('$'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use std::path::Path;

    use crate::testing::workspace;

    fn rendered(plan: &RestructurePlan, path: &Path) -> String {
        let original = crate::vfs::read_to_string(path).unwrap();
        match plan.edits.edits_for(path) {
            Some(edits) => apply_to_string(&original, edits).unwrap(),
            None => original,
        }
    }

    #[test]
    fn rewrites_a_call_shape() {
        let src = "fn f() {\n    let a = old_api(1);\n    let b = old_api(2);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "old_api($X)", "new_api($X, None)").unwrap();

        assert_eq!(plan.matches.len(), 2);
        assert_eq!(
            rendered(&plan, &tmp.path().join("a.rs")),
            "fn f() {\n    let a = new_api(1, None);\n    let b = new_api(2, None);\n}\n"
        );
    }

    #[test]
    fn matching_is_structural_not_textual() {
        // Different spacing, same shape.
        let src = "fn f() {\n    let a = old_api( 1 );\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "old_api($X)", "new_api($X)").unwrap();
        assert_eq!(plan.matches.len(), 1);
        assert!(rendered(&plan, &tmp.path().join("a.rs")).contains("new_api(1)"));
    }

    #[test]
    fn never_matches_inside_strings_or_comments() {
        let src = "fn f() {\n    // old_api(1) in a comment\n    let s = \"old_api(2)\";\n}\n";
        let (_tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "old_api($X)", "new_api($X)").unwrap();
        assert!(
            plan.matches.is_empty(),
            "text inside strings and comments is not code: {:?}",
            plan.matches
        );
    }

    #[test]
    fn a_repeated_metavariable_must_bind_consistently() {
        let src = "fn f() {\n    let a = add(x, x);\n    let b = add(y, z);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "add($A, $A)", "double($A)").unwrap();

        assert_eq!(plan.matches.len(), 1, "only the x,x call matches");
        let out = rendered(&plan, &tmp.path().join("a.rs"));
        assert!(out.contains("double(x)"), "got:\n{out}");
        assert!(out.contains("add(y, z)"), "got:\n{out}");
    }

    #[test]
    fn binds_several_metavariables() {
        let src = "fn f() {\n    let c = combine(a, b);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "combine($X, $Y)", "combine($Y, $X)").unwrap();
        assert!(rendered(&plan, &tmp.path().join("a.rs")).contains("combine(b, a)"));
    }

    #[test]
    fn a_literal_pattern_matches_only_that_literal() {
        let src = "fn f() {\n    g(1);\n    g(2);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "g(1)", "h(1)").unwrap();
        assert_eq!(plan.matches.len(), 1);
        let out = rendered(&plan, &tmp.path().join("a.rs"));
        assert!(out.contains("h(1)") && out.contains("g(2)"), "got:\n{out}");
    }

    #[test]
    fn refuses_a_pattern_that_is_only_a_metavariable() {
        let (_tmp, index) = workspace(&[("a.rs", "fn f() {}\n")]);
        let err = apply(&index, Language::Rust, "$X", "$X").unwrap_err();
        assert!(
            err.downcast_ref::<Refusal>().is_some(),
            "should refuse explicitly: {err}"
        );
    }

    #[test]
    fn only_the_requested_language_is_touched() {
        let (tmp, index) = workspace(&[
            ("a.rs", "fn f() { old_api(1); }\n"),
            ("b.py", "old_api(1)\n"),
        ]);
        let plan = apply(&index, Language::Rust, "old_api($X)", "new_api($X)").unwrap();
        assert_eq!(plan.matches.len(), 1);
        assert!(plan.matches[0].0.ends_with("a.rs"));
        assert_eq!(rendered(&plan, &tmp.path().join("b.py")), "old_api(1)\n");
    }

    #[test]
    fn works_for_python() {
        let src = "def f():\n    return old(1)\n";
        let (tmp, index) = workspace(&[("a.py", src)]);
        let plan = apply(&index, Language::Python, "old($X)", "new($X)").unwrap();
        assert_eq!(
            rendered(&plan, &tmp.path().join("a.py")),
            "def f():\n    return new(1)\n"
        );
    }

    #[test]
    fn the_result_still_parses() {
        let (_tmp, index) = workspace(&[("a.rs", "fn f() { old_api(1); }\n")]);
        let plan = apply(&index, Language::Rust, "old_api($X)", "new_api($X, None)").unwrap();
        let outcomes =
            crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn metavariable_encoding_round_trips() {
        let encoded = encode_metavariables("f($A, $B)");
        assert!(
            !encoded.contains('$'),
            "must parse as an identifier: {encoded}"
        );
        assert_eq!(decode_metavariables(&encoded), "f($A, $B)");
    }

    #[test]
    fn an_unbound_template_variable_is_left_visible() {
        // A typo in the template must show up in the diff, not vanish.
        let bindings = HashMap::from([("X".to_string(), "1".to_string())]);
        assert_eq!(
            substitute("f($X, $TYPO)", &bindings, &HashSet::new()),
            "f(1, $TYPO)"
        );
    }

    #[test]
    fn a_doubled_sigil_is_a_literal_dollar_on_both_sides() {
        // Bash, SCSS and Helm all spell real things with `$`.
        assert_eq!(encode_metavariables("echo $$HOME $X"), "echo $HOME FrMetaX");
        let bindings = HashMap::from([("X".to_string(), "1".to_string())]);
        assert_eq!(
            substitute("echo $$HOME $X", &bindings, &HashSet::new()),
            "echo $HOME 1"
        );
    }

    #[test]
    fn a_lone_sigil_is_left_alone() {
        assert_eq!(encode_metavariables("cost: $ 5"), "cost: $ 5");
        assert_eq!(
            substitute("cost: $ 5", &HashMap::new(), &HashSet::new()),
            "cost: $ 5"
        );
    }

    #[test]
    fn quotes_are_stripped_only_in_matched_pairs() {
        assert_eq!(strip_quotes("\"a\""), Some("a"));
        assert_eq!(strip_quotes("'a'"), Some("a"));
        assert_eq!(strip_quotes("\"a'"), None);
        assert_eq!(strip_quotes("a"), None);
        assert_eq!(strip_quotes("\""), None, "one quote is not a pair");
    }

    #[test]
    fn a_pattern_string_stays_a_string_where_the_grammar_exposes_its_text() {
        // The quoted-metavariable spelling exists for grammars that make a quoted value one
        // opaque token.
        let src = "fn f() {\n    g(\"one\");\n    g(2);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "g(\"$X\")", "h($X)").unwrap();
        assert_eq!(
            plan.matches.len(),
            1,
            "only the string argument: {:?}",
            plan.matches
        );
        let out = rendered(&plan, &tmp.path().join("a.rs"));
        assert!(
            out.contains("h(one)") && out.contains("g(2)"),
            "got:\n{out}"
        );
    }

    #[test]
    fn a_statement_pattern_does_not_swallow_its_terminator() {
        // The Rust wrapper supplies the `;`, so the match stops before the target's.
        let src = "fn f() {\n    let x = old(1);\n    let y = 2;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = apply(&index, Language::Rust, "let x = old($V)", "let x = new($V)").unwrap();
        assert_eq!(plan.matches.len(), 1);
        assert_eq!(
            rendered(&plan, &tmp.path().join("a.rs")),
            "fn f() {\n    let x = new(1);\n    let y = 2;\n}\n"
        );
    }

    #[test]
    fn a_match_touching_a_template_action_is_dropped() {
        let actions = [Span::new(10, 20)];
        assert!(
            touches_template_action(&actions, Span::new(12, 15)),
            "inside"
        );
        assert!(
            touches_template_action(&actions, Span::new(0, 10)),
            "abutting"
        );
        assert!(
            touches_template_action(&actions, Span::new(20, 25)),
            "abutting"
        );
        assert!(!touches_template_action(&actions, Span::new(0, 9)));
        assert!(!touches_template_action(&actions, Span::new(21, 25)));
    }

    #[test]
    fn every_language_has_at_least_one_fragment_wrapper() {
        for language in Language::ALL {
            assert!(
                !fragment_wrappers(*language).is_empty(),
                "{language} has no way to parse a fragment"
            );
        }
    }
}
