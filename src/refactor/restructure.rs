//! Pattern-based restructuring: rewrite code matching a shape.
//!
//! `$NAME` in a pattern is a metavariable that matches any single node and binds its
//! text; the same name in the template substitutes that text back. Matching is
//! structural rather than textual, so `$A + $B` matches an addition however it is
//! spaced, and never matches inside a string or comment.
//!
//! This is the escape hatch for the long tail of the refactoring catalog — the
//! transformations nobody ships as a built-in. It is deliberately syntactic: it has
//! no idea what a name means, so it is checked by the same reparse validation as
//! every other edit and never claims more than it knows.
//!
//! # How a fragment is parsed
//!
//! A pattern is a *fragment*, not a file: `old_api($X)` is not a valid Rust item and
//! `var.$X` is not a valid Terraform file. Three steps make one parse anyway:
//!
//! 1. `$NAME` is encoded as the ordinary identifier `FrMetaNAME`, which is a legal
//!    name in every supported grammar. `$$NAME` escapes the sigil and stands for a
//!    literal `$NAME`, which is what Bash, SCSS and Helm sources actually contain.
//! 2. The encoded fragment is tried inside each of a short list of per-language
//!    [`fragment_wrappers`], in order, until one parses without errors *and* yields a
//!    node starting exactly at the fragment. A fragment can be several shapes in the
//!    same language — a CSS pattern may be a whole rule, a declaration or a selector —
//!    and the wrapper that parses identifies which shape was meant.
//! 3. That node is matched structurally against every node of every target file.
//!
//! A wrapper may contribute trailing punctuation the fragment did not write: the CSS
//! declaration wrapper adds the `;` that makes `color: $X` a declaration, and
//! tree-sitter puts that `;` inside the declaration node. When that happens the match
//! is trimmed back to the end of the matched node's last named child, so rewriting
//! `color: red;` replaces `color: red` and leaves the terminator alone.
//!
//! # Per-language notes
//!
//! - **Bash**: a pattern is a command (`curl $URL`), matched against `command` nodes.
//!   Because `$X` is a metavariable, a literal expansion must be written `$$X`.
//! - **HCL**: an attribute (`count = $N`) or a block parses bare; a bare expression
//!   (`var.$X`) is wrapped in an attribute.
//! - **YAML/Helm**: a pattern is a mapping pair or a nested mapping. Helm `{{ ... }}`
//!   actions are masked to spaces before the YAML parse (see [`crate::parse`]), so
//!   they carry no structure: a pattern may not contain one, and any match whose span
//!   covers or runs up against one is dropped rather than rewritten from a tree that
//!   never saw those bytes.
//! - **CSS/SCSS**: a rule, a declaration or a selector, in that order of preference.
//! - **HTML/XML**: an element. An XML attribute value is one token *including* its
//!   quotes (tree-sitter-xml never makes a node for the text between them), so a
//!   metavariable standing for a whole quoted leaf is recognised and binds the
//!   target's text without its quotes.
//! - **Markdown**: a block (`## $TITLE`) or an inline (`[$TEXT](old/url)`). The
//!   grammar folds a heading's padding into its content node, so metavariable text is
//!   compared and bound trimmed.

use super::Refusal;
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::span::Span;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
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
}

/// Rewrite every occurrence of `pattern` as `template` across the workspace.
pub fn apply(
    index: &Index,
    language: Language,
    pattern: &str,
    template: &str,
) -> Result<RestructurePlan> {
    let parsers = Parsers::new();

    // Helm's `{{ ... }}` actions are masked to spaces before the YAML parse, so a
    // pattern containing one would be matched against blanks. Say so rather than
    // silently matching nothing.
    if language == Language::Helm && pattern.contains("{{") {
        return Err(Refusal::Unsupported {
            operation: "a pattern containing a '{{ ... }}' template action (those bytes are \
                        masked to whitespace before the YAML parse, so they carry no structure \
                        to match)"
                .to_string(),
            language: language.to_string(),
        }
        .into());
    }

    // `$X` is not valid syntax in most languages, so the pattern is rewritten into
    // ordinary identifiers before parsing. Without this the pattern would parse into
    // ERROR nodes everywhere except Rust, where `$` happens to be macro syntax.
    let original_pattern = pattern.to_string();
    let encoded = encode_metavariables(pattern);
    let (pattern_parsed, pattern_source, offset, trim_trailing) =
        parse_fragment(&parsers, language, &encoded, pattern)?;
    // Located again rather than returned: a `Node` borrows its tree, and handing the
    // tree back out of `parse_fragment` alongside a node borrowed from it would need a
    // self-referential struct to say. Re-walking a fragment-sized tree costs nothing.
    let pattern_root = fragment_root(&pattern_parsed, offset, encoded.len())
        .ok_or_else(|| anyhow::anyhow!("could not parse the pattern as {language}: '{pattern}'"))?;
    let compiled = Pattern {
        root: pattern_root,
        source: &pattern_source,
        trim_trailing,
    };

    // A pattern that is only a metavariable would match every node in the file.
    if metavariable(pattern_root, &pattern_source).is_some() {
        return Err(Refusal::InvalidName {
            name: original_pattern,
            reason: "a pattern that is only a metavariable would match everything".into(),
        }
        .into());
    }

    let mut edits = EditSet::new();
    let mut matches = Vec::new();

    for (path, info) in index.files() {
        if info.language != language {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let parsed = parsers.parse(language, &source)?;

        for (span, bindings) in find_matches(&parsed, &source, &compiled) {
            let replacement = substitute(template, &bindings);
            // A rewrite that changes nothing is not a rewrite.
            if replacement == span.text(&source) {
                continue;
            }
            matches.push((path.clone(), span.text(&source).to_string()));
            edits.add(
                path.clone(),
                Edit::new(span, replacement, "restructure".to_string()),
            );
        }
    }

    Ok(RestructurePlan {
        // Report the pattern the caller wrote, not the wrapped, encoded form used
        // internally for parsing.
        pattern: original_pattern,
        template: template.to_string(),
        edits,
        matches,
    })
}

/// A compiled pattern: the fragment's node, the text it was parsed from, and whether
/// its wrapper contributed trailing punctuation a match must not swallow.
struct Pattern<'a> {
    root: Node<'a>,
    source: &'a str,
    trim_trailing: bool,
}

/// Parse the encoded fragment inside the first wrapper that accepts it.
///
/// Returns the parse, the wrapped source, the fragment's offset within it, and
/// whether the fragment's node reaches past the fragment into the wrapper.
fn parse_fragment(
    parsers: &Parsers,
    language: Language,
    encoded: &str,
    display: &str,
) -> Result<(Parsed, String, usize, bool)> {
    let wrappers = fragment_wrappers(language);
    for (prefix, suffix) in wrappers {
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
        // The node must begin where the fragment begins — a node that starts inside
        // the wrapper is the wrapper's, not the pattern's — and may reach past the
        // fragment only into punctuation the wrapper itself supplied.
        if root.start_byte() != offset
            || root.end_byte() < end
            || root.end_byte() > end + suffix.len()
        {
            continue;
        }
        let trim_trailing = root.end_byte() > end;
        return Ok((parsed, source, offset, trim_trailing));
    }

    let tried: Vec<String> = wrappers
        .iter()
        .map(|(p, s)| format!("`{}`", format!("{p}<pattern>{s}").replace('\n', "\\n")))
        .collect();
    anyhow::bail!(
        "'{display}' is not a valid {language} fragment: it did not parse in any of the \
         {} wrapper(s) tried ({})",
        wrappers.len(),
        tried.join(", ")
    )
}

/// Minimal syntax that makes a fragment parse as a whole file, most specific first.
///
/// A language gets more than one entry when a fragment can legitimately be more than
/// one shape; the first wrapper that parses decides which shape the pattern is.
fn fragment_wrappers(language: Language) -> &'static [(&'static str, &'static str)] {
    match language {
        // An expression or statement inside a function body, or a whole item.
        Language::Rust => &[("fn __fr_pattern() { ", "; }"), ("", "\n")],
        Language::Go => &[
            ("package p\n\nfunc __frPattern() {\n", "\n}\n"),
            ("package p\n\n", "\n"),
        ],
        Language::Zig => &[("pub fn __fr_pattern() void {\n", ";\n}\n"), ("", "\n")],
        // Python and the JS family accept a bare expression statement.
        Language::TypeScript | Language::Tsx | Language::Python => &[("", "\n")],
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
        // An element is a document on its own; several siblings need a root.
        Language::Html => &[("", "\n"), ("<html><body>", "</body></html>\n")],
        Language::Xml => &[("", "\n"), ("<__fr_pattern>", "</__fr_pattern>\n")],
        Language::Markdown => &[("", "\n")],
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
    // Descend through single-child wrappers: a node of identical extent adds nothing
    // to the shape, and a statement container is punctuation the wrapper asked for.
    loop {
        let named: Vec<Node> = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        };
        let only_child = named.len() == 1
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
///
/// `$$NAME` is an escaped literal `$NAME` and keeps its sigil: Bash expansions, SCSS
/// variables and Helm values are all spelled with a real `$`, and a pattern must be
/// able to say so.
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
///
/// Two spellings are recognised. A node whose text is `FrMetaX` is the ordinary case.
/// A *leaf* whose text is `"FrMetaX"` is the quoted case, which exists because some
/// grammars never break a quoted value into a node — tree-sitter-xml's `AttValue` is
/// one token, quotes included, so `id="$X"` has nothing else to bind to. The quoted
/// spelling is deliberately restricted to leaves: where a grammar does expose the
/// text inside the quotes, that inner node is what a metavariable binds, and a
/// pattern string stays a pattern string rather than matching any node at all.
fn metavariable(node: Node<'_>, source: &str) -> Option<Metavariable> {
    // Some grammars fold padding into a node — tree-sitter-yaml keeps the space
    // after a `-` sequence marker inside the item — so compare trimmed.
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

/// The metavariable name in an encoded identifier, if that is what this text is.
fn meta_name(text: &str) -> Option<String> {
    let name = text.strip_prefix(META)?;
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_string())
    } else {
        None
    }
}

/// The text inside a matched pair of quotes, if the text is quoted.
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

/// Every match of the pattern in the file, with its metavariable bindings.
fn find_matches(
    parsed: &Parsed,
    source: &str,
    pattern: &Pattern<'_>,
) -> Vec<(Span, HashMap<String, String>)> {
    let mut results = Vec::new();
    // Markdown's inline content is a sub-tree of its own, and that is where its links
    // and emphasis are; a pattern over them matches nothing in the block tree.
    let mut stack: Vec<Node> = parsed.roots().collect();
    let mut cursor = parsed.root().walk();

    while let Some(node) = stack.pop() {
        let mut bindings = HashMap::new();
        if matches_node(node, source, pattern.root, pattern.source, &mut bindings) {
            let span = match_span(node, pattern.trim_trailing);
            if !touches_template_action(&parsed.template_actions, span) {
                results.push((span, bindings));
                // Do not rewrite inside something already being rewritten: nested
                // edits would overlap and be rejected.
                continue;
            }
        }
        stack.extend(node.named_children(&mut cursor));
    }

    results.sort_by_key(|(span, _)| *span);
    // A node can appear in two trees at once — Markdown's `inline` is the block
    // grammar's opaque leaf and the inline grammar's root — and one match rewritten
    // twice is an overlapping edit the engine rejects.
    results.dedup_by_key(|(span, _)| *span);
    results
}

/// Does a match run into a Helm `{{ ... }}` action?
///
/// The action's bytes are spaces to the YAML parser, so the tree says nothing about
/// them. A span that covers one would be rewritten from a parse that never saw it,
/// and a span that merely *ends where one begins* is worse: `name: web-{{ .V.x }}`
/// parses as the complete-looking scalar `web-`, so a match there would bind a value
/// that is only half of the real one. Both are dropped; adjacency counts.
fn touches_template_action(actions: &[Span], span: Span) -> bool {
    actions
        .iter()
        .any(|action| action.start <= span.end && span.start <= action.end)
}

/// The span a match rewrites.
///
/// When the pattern's wrapper supplied trailing punctuation — the `;` that turns
/// `color: $X` into a CSS declaration — the target's equivalent punctuation is not
/// part of what the pattern asked for, so the match stops at the last named child.
fn match_span(node: Node<'_>, trim_trailing: bool) -> Span {
    if !trim_trailing {
        return Span::from(node);
    }
    let mut cursor = node.walk();
    match node.named_children(&mut cursor).last() {
        Some(last) if last.end_byte() > node.start_byte() => {
            Span::new(node.start_byte(), last.end_byte())
        }
        _ => Span::from(node),
    }
}

/// Structural match of one node against one pattern node.
fn matches_node(
    node: Node<'_>,
    source: &str,
    pattern: Node<'_>,
    pattern_source: &str,
    bindings: &mut HashMap<String, String>,
) -> bool {
    // A metavariable matches any node, but must bind consistently: `$A + $A`
    // requires both sides to be the same text.
    if let Some(meta) = metavariable(pattern, pattern_source) {
        let text = binding_text(node, source, meta.quoted);
        return match bindings.get(&meta.name) {
            Some(existing) => *existing == text,
            None => {
                bindings.insert(meta.name, text);
                true
            }
        };
    }

    if node.kind() != pattern.kind() {
        return false;
    }

    let pattern_children: Vec<Node> = {
        let mut cursor = pattern.walk();
        pattern.named_children(&mut cursor).collect()
    };

    // A leaf in the pattern must match the target's text exactly.
    if pattern_children.is_empty() {
        return Span::from(node).text(source) == Span::from(pattern).text(pattern_source);
    }

    let node_children: Vec<Node> = {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).collect()
    };
    if node_children.len() != pattern_children.len() {
        return false;
    }

    node_children
        .iter()
        .zip(pattern_children.iter())
        .all(|(n, p)| matches_node(*n, source, *p, pattern_source, bindings))
}

/// Turn encoded metavariables back into `$NAME` for display.
#[cfg_attr(not(test), allow(dead_code))]
fn decode_metavariables(encoded: &str) -> String {
    encoded.replace(META, "$")
}

/// Replace `$NAME` in the template with its binding.
fn substitute(template: &str, bindings: &HashMap<String, String>) -> String {
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
                out.push_str(value);
                for _ in 0..len {
                    chars.next();
                }
            }
            // An unbound metavariable is left as written rather than silently
            // dropped, so a typo in the template is visible in the diff.
            None => out.push('$'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{scan, ScanOptions};
    use std::path::Path;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn rendered(plan: &RestructurePlan, path: &Path) -> String {
        let original = std::fs::read_to_string(path).unwrap();
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
        assert_eq!(substitute("f($X, $TYPO)", &bindings), "f(1, $TYPO)");
    }

    #[test]
    fn a_doubled_sigil_is_a_literal_dollar_on_both_sides() {
        // Bash, SCSS and Helm all spell real things with `$`.
        assert_eq!(encode_metavariables("echo $$HOME $X"), "echo $HOME FrMetaX");
        let bindings = HashMap::from([("X".to_string(), "1".to_string())]);
        assert_eq!(substitute("echo $$HOME $X", &bindings), "echo $HOME 1");
    }

    #[test]
    fn a_lone_sigil_is_left_alone() {
        assert_eq!(encode_metavariables("cost: $ 5"), "cost: $ 5");
        assert_eq!(substitute("cost: $ 5", &HashMap::new()), "cost: $ 5");
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
        // The quoted-metavariable spelling exists for grammars that make a quoted
        // value one opaque token. Rust is not one of those, so `f("$X")` must keep
        // matching strings only — not every argument that happens to be there.
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
