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

    // `$X` is not valid syntax in most languages, so the pattern is rewritten into
    // ordinary identifiers before parsing. Without this the pattern would parse into
    // ERROR nodes everywhere except Rust, where `$` happens to be macro syntax.
    let original_pattern = pattern.to_string();
    let encoded = encode_metavariables(pattern);
    // A pattern is a fragment, not a file: `old_api(x)` is not a valid Rust item.
    // Parsing it inside a minimal wrapper gives the grammar the context it needs.
    let (prefix, suffix) = fragment_wrapper(language);
    let pattern_source = format!("{prefix}{encoded}{suffix}");
    let pattern_parsed = parsers.parse(language, &pattern_source)?;
    if pattern_parsed.has_errors() {
        anyhow::bail!("'{pattern}' is not valid {language} syntax");
    }
    let pattern_root = fragment_root(&pattern_parsed, prefix.len(), encoded.len())
        .ok_or_else(|| anyhow::anyhow!("could not parse the pattern as {language}: '{pattern}'"))?;
    let pattern = pattern_source.as_str();

    // A pattern that is only a metavariable would match every node in the file.
    if metavariable_name(pattern_root, pattern).is_some() {
        return Err(Refusal::InvalidName {
            name: pattern.to_string(),
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

        for (span, bindings) in find_matches(&parsed, &source, pattern_root, pattern) {
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

/// Minimal syntax that makes an expression fragment parse as a whole file.
fn fragment_wrapper(language: Language) -> (&'static str, &'static str) {
    match language {
        Language::Rust => ("fn __fr_pattern() { ", "; }"),
        Language::Go => ("package p\n\nfunc __frPattern() {\n", "\n}\n"),
        Language::Zig => ("pub fn __fr_pattern() void {\n", ";\n}\n"),
        // Python and the JS family accept a bare expression statement.
        _ => ("", ""),
    }
}

/// The outermost node covering exactly the fragment inside its wrapper.
fn fragment_root<'a>(parsed: &'a Parsed, offset: usize, len: usize) -> Option<Node<'a>> {
    let span = Span::new(offset, offset + len);
    let mut node = parsed.root().descendant_for_byte_range(span.start, span.end)?;

    // Widen through wrappers of identical extent, then narrow past statement
    // containers the wrapper introduced.
    while let Some(parent) = node.parent() {
        if Span::from(parent) == Span::from(node) {
            node = parent;
        } else {
            break;
        }
    }
    // Descend through single-child wrappers such as expression_statement.
    loop {
        let named: Vec<Node> = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        };
        if named.len() == 1 && Span::from(named[0]) == Span::from(node) {
            node = named[0];
        } else if named.len() == 1 && node.kind().contains("statement") {
            node = named[0];
        } else {
            return Some(node);
        }
    }
}

/// The identifier prefix a metavariable is encoded as, chosen to be valid in every
/// supported language and vanishingly unlikely to occur in real code.
const META: &str = "FrMeta";

/// Rewrite `$NAME` into an ordinary identifier so the pattern parses.
fn encode_metavariables(pattern: &str) -> String {
    let re = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("static regex");
    re.replace_all(pattern, format!("{META}$1")).into_owned()
}

/// If `node` is an encoded metavariable, its name.
fn metavariable_name(node: Node<'_>, source: &str) -> Option<String> {
    let text = Span::from(node).text(source);
    let name = text.strip_prefix(META)?;
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_string())
    } else {
        None
    }
}

/// Every match of the pattern in the file, with its metavariable bindings.
fn find_matches(
    parsed: &Parsed,
    source: &str,
    pattern_root: Node<'_>,
    pattern_source: &str,
) -> Vec<(Span, HashMap<String, String>)> {
    let mut results = Vec::new();
    let mut stack = vec![parsed.root()];
    let mut cursor = parsed.root().walk();

    while let Some(node) = stack.pop() {
        let mut bindings = HashMap::new();
        if matches_node(node, source, pattern_root, pattern_source, &mut bindings) {
            results.push((Span::from(node), bindings));
            // Do not rewrite inside something already being rewritten: nested edits
            // would overlap and be rejected.
            continue;
        }
        stack.extend(node.named_children(&mut cursor));
    }

    results.sort_by_key(|(span, _)| *span);
    results
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
    if let Some(name) = metavariable_name(pattern, pattern_source) {
        let text = Span::from(node).text(source).to_string();
        return match bindings.get(&name) {
            Some(existing) => *existing == text,
            None => {
                bindings.insert(name, text);
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
        // Read the longest identifier following the sigil.
        let rest = &template[i + 1..];
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
        assert_eq!(rendered(&plan, &tmp.path().join("a.py")), "def f():\n    return new(1)\n");
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
        assert!(!encoded.contains('$'), "must parse as an identifier: {encoded}");
        assert_eq!(decode_metavariables(&encoded), "f($A, $B)");
    }

    #[test]
    fn an_unbound_template_variable_is_left_visible() {
        // A typo in the template must show up in the diff, not vanish.
        let bindings = HashMap::from([("X".to_string(), "1".to_string())]);
        assert_eq!(substitute("f($X, $TYPO)", &bindings), "f(1, $TYPO)");
    }
}
