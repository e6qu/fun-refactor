//! Tree-sitter parsing for every supported language.
//!
//! Grammars are loaded once into a [`Parsers`] handle and reused. Parsing never
//! rewrites the source: byte offsets in the resulting tree always index the original
//! text, which is what the edit engine relies on.

use crate::lang::Language;
use crate::span::Span;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

/// Loaded tree-sitter grammars.
pub struct Parsers;

impl Parsers {
    pub fn new() -> Self {
        Self
    }

    /// The tree-sitter grammar used for a language.
    ///
    /// SCSS is parsed with the CSS grammar (the only one available in this grammar
    /// set); SCSS-only constructs therefore surface as parse errors, which callers
    /// see via [`Parsed::has_errors`] rather than silently mis-parsing.
    fn grammar(lang: Language) -> tree_sitter::Language {
        match lang {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Zig => tree_sitter_zig::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Bash => tree_sitter_bash::LANGUAGE.into(),
            Language::Html => tree_sitter_html::LANGUAGE.into(),
            Language::Css | Language::Scss => tree_sitter_css::LANGUAGE.into(),
            Language::Hcl => tree_sitter_hcl::LANGUAGE.into(),
            Language::Yaml | Language::Helm => tree_sitter_yaml::LANGUAGE.into(),
            Language::Xml => tree_sitter_xml::LANGUAGE_XML.into(),
            Language::Markdown => tree_sitter_markdown_fork::language(),
        }
    }

    /// Parse `source` as `lang`.
    pub fn parse(&self, lang: Language, source: &str) -> Result<Parsed> {
        let mut parser = Parser::new();
        let grammar = Self::grammar(lang);
        parser
            .set_language(&grammar)
            .with_context(|| format!("loading {lang} grammar"))?;

        // Helm templates are not valid YAML. Mask the Go template actions with spaces
        // of identical byte length so the YAML grammar sees well-formed input while
        // every byte offset in the tree still refers to the original source.
        let (parse_input, template_actions) = if lang == Language::Helm {
            let actions = find_template_actions(source);
            (mask_spans(source, &actions), actions)
        } else {
            (source.to_string(), Vec::new())
        };

        let tree = parser
            .parse(&parse_input, None)
            .with_context(|| format!("parsing {lang} source"))?;

        Ok(Parsed {
            language: lang,
            tree,
            template_actions,
        })
    }
}

impl Default for Parsers {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed source file.
pub struct Parsed {
    pub language: Language,
    pub tree: Tree,
    /// Byte spans of Helm `{{ ... }}` template actions, masked out before YAML parsing.
    pub template_actions: Vec<Span>,
}

impl Parsed {
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Does the tree contain any ERROR or MISSING node?
    pub fn has_errors(&self) -> bool {
        self.root().has_error()
    }

    /// Byte spans of every ERROR and MISSING node, in source order.
    ///
    /// The edit engine compares these before and after an edit: an edit that
    /// introduces new syntax errors is rejected rather than written.
    pub fn error_spans(&self) -> Vec<Span> {
        let mut errors = Vec::new();
        let mut cursor = self.root().walk();
        let mut recurse = true;
        loop {
            let node = cursor.node();
            if node.is_error() || node.is_missing() {
                errors.push(Span::from(node));
                // No need to descend into a subtree already known to be broken.
                recurse = false;
            }
            if recurse && cursor.goto_first_child() {
                continue;
            }
            recurse = true;
            if cursor.goto_next_sibling() {
                continue;
            }
            loop {
                if !cursor.goto_parent() {
                    errors.sort();
                    // Some grammars flag a subtree as erroneous without producing an
                    // ERROR or MISSING node anywhere in it (tree-sitter-zig does this
                    // for an empty container body). Reporting nothing here would let
                    // the edit engine's before/after comparison see no change and
                    // accept an edit that broke the file, so fall back to the
                    // narrowest node that still reports an error.
                    if errors.is_empty() && self.root().has_error() {
                        errors.push(innermost_error_span(self.root()));
                    }
                    return errors;
                }
                if cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// The smallest named node whose span contains `offset`.
    pub fn node_at(&self, offset: usize) -> Option<Node<'_>> {
        self.root()
            .named_descendant_for_byte_range(offset, offset.saturating_add(1))
    }
}

/// Descend to the smallest node that still reports an error.
///
/// Used when a tree is flagged erroneous but contains no ERROR or MISSING node.
fn innermost_error_span(root: Node<'_>) -> Span {
    let mut node = root;
    loop {
        let mut cursor = node.walk();
        let next = node.children(&mut cursor).find(|child| child.has_error());
        match next {
            Some(child) => node = child,
            None => return Span::from(node),
        }
    }
}

/// Locate Go template actions `{{ ... }}`, tolerating `{{- -}}` trim markers.
///
/// Quoted strings inside an action may legally contain `}}`, so the scan tracks
/// quoting rather than searching for the first closing delimiter.
fn find_template_actions(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            let start = i;
            i += 2;
            let mut quote: Option<u8> = None;
            while i < bytes.len() {
                let b = bytes[i];
                match quote {
                    Some(q) => {
                        if b == b'\\' {
                            i += 1;
                        } else if b == q {
                            quote = None;
                        }
                    }
                    None => {
                        if b == b'"' || b == b'\'' || b == b'`' {
                            quote = Some(b);
                        } else if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                            i += 2;
                            break;
                        }
                    }
                }
                i += 1;
            }
            spans.push(Span::new(start, i.min(bytes.len())));
        } else {
            i += 1;
        }
    }
    spans
}

/// Replace each span with spaces of the same byte length, preserving newlines so
/// line numbering and every byte offset outside the spans stay identical.
fn mask_spans(source: &str, spans: &[Span]) -> String {
    let mut out = source.as_bytes().to_vec();
    for span in spans {
        for b in &mut out[span.start..span.end] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    // Masking only ever replaces ASCII bytes with ASCII spaces, so UTF-8 stays valid.
    String::from_utf8(out).expect("masking preserves UTF-8 validity")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lang: Language, src: &str) -> Parsed {
        Parsers::new().parse(lang, src).expect("parse succeeds")
    }

    #[test]
    fn every_language_grammar_loads_and_parses() {
        // One representative snippet per language: the grammar set must be mutually
        // compatible and every language must produce a usable tree.
        let cases: &[(Language, &str)] = &[
            (Language::Rust, "fn main() { println!(\"hi\"); }\n"),
            (Language::Go, "package main\n\nfunc main() {}\n"),
            (Language::Zig, "pub fn main() void {}\n"),
            (Language::TypeScript, "export function f(a: number) { return a; }\n"),
            (Language::Tsx, "export const App = () => <div className=\"x\" />;\n"),
            (Language::Python, "def main():\n    return 1\n"),
            (Language::Bash, "main() {\n  echo hi\n}\n"),
            (Language::Html, "<html><body id=\"root\"></body></html>\n"),
            (Language::Css, ".btn { color: red; }\n"),
            (Language::Hcl, "resource \"aws_s3_bucket\" \"b\" {\n  bucket = var.name\n}\n"),
            (Language::Yaml, "key: value\nlist:\n  - a\n"),
            (Language::Xml, "<root><child id=\"a\"/></root>\n"),
            (Language::Markdown, "# Title\n\nSome text.\n"),
        ];

        for (lang, src) in cases {
            let parsed = parse(*lang, src);
            assert_eq!(parsed.language, *lang);
            assert!(
                !parsed.has_errors(),
                "{lang} produced parse errors: {:?}",
                parsed.error_spans()
            );
            assert!(parsed.root().end_byte() > 0, "{lang} produced an empty tree");
        }
    }

    #[test]
    fn error_spans_report_broken_syntax() {
        let parsed = parse(Language::Rust, "fn main( { let x = ; }\n");
        assert!(parsed.has_errors());
        assert!(
            !parsed.error_spans().is_empty(),
            "expected at least one error span"
        );
    }

    #[test]
    fn tsx_grammar_handles_jsx_that_ts_grammar_rejects() {
        let jsx = "const A = () => <div>{x}</div>;\n";
        assert!(!parse(Language::Tsx, jsx).has_errors());
        // The distinction is real: the plain TypeScript grammar cannot parse this.
        assert!(parse(Language::TypeScript, jsx).has_errors());
    }

    #[test]
    fn helm_template_actions_are_masked_preserving_offsets() {
        let src = "metadata:\n  name: {{ .Values.name }}\n  ns: {{- .Release.ns -}}\n";
        let parsed = parse(Language::Helm, src);
        assert!(
            !parsed.has_errors(),
            "masked Helm should parse as YAML: {:?}",
            parsed.error_spans()
        );
        assert_eq!(parsed.template_actions.len(), 2);
        // Spans must still index the ORIGINAL source, not the masked copy.
        assert_eq!(
            parsed.template_actions[0].text(src),
            "{{ .Values.name }}"
        );
        assert_eq!(
            parsed.template_actions[1].text(src),
            "{{- .Release.ns -}}"
        );
    }

    #[test]
    fn template_action_with_braces_in_string_is_not_truncated() {
        let src = "a: {{ printf \"}}\" }}\n";
        let actions = find_template_actions(src);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].text(src), "{{ printf \"}}\" }}");
    }

    #[test]
    fn masking_preserves_length_and_newlines() {
        let src = "a: {{ x }}\nb: 2\n";
        let masked = mask_spans(src, &find_template_actions(src));
        assert_eq!(masked.len(), src.len());
        assert_eq!(masked.matches('\n').count(), src.matches('\n').count());
        assert_eq!(&masked[0..3], "a: ");
        assert_eq!(&masked[11..], "b: 2\n");
    }

    #[test]
    fn node_at_finds_innermost_node() {
        let src = "fn alpha() {}\n";
        let parsed = parse(Language::Rust, src);
        let offset = src.find("alpha").unwrap();
        let node = parsed.node_at(offset).expect("node at identifier");
        assert_eq!(Span::from(node).text(src), "alpha");
    }

    #[test]
    fn scss_specific_syntax_is_reported_not_silently_accepted() {
        // SCSS runs on the CSS grammar; `$vars` and `@mixin` are not CSS. The tool
        // must surface that rather than pretend the parse succeeded.
        let parsed = parse(Language::Scss, "@mixin theme($c) { color: $c; }\n");
        assert!(
            parsed.has_errors(),
            "SCSS-only syntax should be visible as parse errors under the CSS grammar"
        );
    }
}
