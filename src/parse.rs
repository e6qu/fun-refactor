//! Tree-sitter parsing for every supported language.
//!
//! Grammars are loaded once into a [`Parsers`] handle and reused. Parsing never
//! rewrites the source: byte offsets in the resulting tree always index the original
//! text, which is what the edit engine relies on. That holds for the two languages
//! that are not parsed as written — Helm, whose template actions are masked to keep
//! the YAML grammar happy, and Markdown, whose inline content is parsed a second time
//! by a second grammar — because both preserve every byte offset.

use crate::lang::Language;
use crate::model::FactGap;
use crate::span::Span;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Range, Tree};

/// Loaded tree-sitter grammars.
pub struct Parsers;

impl Parsers {
    pub fn new() -> Self {
        Self
    }

    /// The tree-sitter grammar used for a language.
    ///
    /// One dialect gets its own entry instead of borrowing a near neighbour's:
    /// SCSS on the CSS grammar reported every `$variable` and `@mixin` as a parse
    /// error, so it has the SCSS grammar. What a grammar still cannot express
    /// surfaces through [`Parsed::has_errors`] instead of being mis-parsed silently.
    fn grammar(lang: Language) -> Option<tree_sitter::Language> {
        // Each arm is behind its own feature, because a grammar is a megabyte of C
        // parse table and a browser build takes only what it will use. Absent means
        // absent, not broken: [`Self::parse`] says which build you are running.
        match lang {
            #[cfg(feature = "lang-rust")]
            Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            #[cfg(feature = "lang-go")]
            Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
            #[cfg(feature = "lang-java")]
            Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
            #[cfg(feature = "lang-zig")]
            Language::Zig => Some(tree_sitter_zig::LANGUAGE.into()),
            #[cfg(feature = "lang-typescript")]
            Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            #[cfg(feature = "lang-typescript")]
            Language::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
            #[cfg(feature = "lang-python")]
            Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
            #[cfg(feature = "lang-bash")]
            Language::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
            #[cfg(feature = "lang-html")]
            Language::Html => Some(tree_sitter_html::LANGUAGE.into()),
            #[cfg(feature = "lang-css")]
            Language::Css => Some(tree_sitter_css::LANGUAGE.into()),
            // SCSS is a superset of CSS, and its own grammar knows the extra half.
            #[cfg(feature = "lang-scss")]
            Language::Scss => Some(tree_sitter_scss::language()),
            #[cfg(feature = "lang-hcl")]
            Language::Hcl => Some(tree_sitter_hcl::LANGUAGE.into()),
            #[cfg(feature = "lang-yaml")]
            Language::Yaml | Language::Helm => Some(tree_sitter_yaml::LANGUAGE.into()),
            #[cfg(feature = "lang-xml")]
            Language::Xml => Some(tree_sitter_xml::LANGUAGE_XML.into()),
            #[cfg(feature = "lang-markdown")]
            Language::Markdown => Some(tree_sitter_md_025::LANGUAGE.into()),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    /// Can this build parse `lang` at all?
    ///
    /// A browser build may leave grammars out to save a megabyte each, and the
    /// caller that loads a whole repository needs to know *before* it hands over a
    /// file — a workspace should not fail to open because one README is in a
    /// language this binary was not compiled with.
    pub fn supports(lang: Language) -> bool {
        Self::grammar(lang).is_some()
    }

    /// The second grammar a language needs, for grammars that parse block structure
    /// and inline content separately.
    ///
    /// Markdown is the only such language: its block grammar leaves the contents of
    /// every paragraph, heading and table cell as an opaque `inline` node, and the
    /// inline grammar is what turns those bytes into links, labels and destinations.
    fn inline_grammar(lang: Language) -> Option<tree_sitter::Language> {
        match lang {
            #[cfg(feature = "lang-markdown")]
            Language::Markdown => Some(tree_sitter_md_025::INLINE_LANGUAGE.into()),
            _ => None,
        }
    }

    /// Parse `source` as `lang`.
    pub fn parse(&self, lang: Language, source: &str) -> Result<Parsed> {
        let mut parser = Parser::new();
        let grammar = Self::grammar(lang).ok_or_else(|| {
            anyhow::anyhow!(
                "this build has no {lang} grammar. The terminal build includes every \
                 language; a browser build takes only the ones it was compiled with."
            )
        })?;
        parser
            .set_language(&grammar)
            .with_context(|| format!("loading {lang} grammar"))?;

        // Two languages are not parsed as written, and both replace bytes with the same
        // number of bytes so every offset in the tree still indexes the original source.
        let (parse_input, masked_spans) = match lang {
            // Helm templates are not valid YAML.
            Language::Helm => {
                let actions = find_template_actions(source);
                (mask_spans(source, &actions), actions)
            }
            // `tree-sitter-scss` 1.0 has no rule for `#{...}` in a declaration value, and
            // the ERROR node it produces is not the expression: it runs to the end of the
            // file, so one interpolated value costs every fact below it. An identifier in
            // its place leaves the declaration well formed. What the filler then hides —
            // the variables and calls written inside the braces — is put back by
            // `interpolation_references`, so the parse is the only thing that changes.
            Language::Scss => {
                let spans = find_scss_interpolations(source);
                (fill_spans(source, &spans, b'x'), spans)
            }
            _ => (source.to_string(), Vec::new()),
        };

        let tree = parser
            .parse(&parse_input, None)
            .with_context(|| format!("parsing {lang} source"))?;

        let inline_trees = match Self::inline_grammar(lang) {
            Some(grammar) => parse_inline_content(&grammar, &tree, &parse_input)
                .with_context(|| format!("parsing {lang} inline content"))?,
            None => Vec::new(),
        };

        let mut parsed = Parsed {
            language: lang,
            tree,
            inline_trees,
            masked_spans,
            gaps: Vec::new(),
        };
        if parsed.has_errors() {
            parsed.gaps.push(FactGap::SyntaxErrors);
        }
        if parsed
            .masked_spans
            .iter()
            .any(|span| in_key_position(source, *span))
        {
            parsed.gaps.push(FactGap::TemplatedKeys);
        }
        Ok(parsed)
    }
}

impl Default for Parsers {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse every opaque inline node of a block tree with the inline grammar.
///
/// Each sub-tree is parsed from the *whole* source with tree-sitter's included ranges
/// restricted to one inline node, so every byte offset in the result indexes the
/// original document — the same property Helm masking preserves, and the one the edit
/// engine depends on. Parsing the ranges one node at a time and not all at once
/// also keeps the nodes independent: a stray `[` at the end of one paragraph cannot
/// pair with a `]` in the next.
fn parse_inline_content(
    grammar: &tree_sitter::Language,
    block_tree: &Tree,
    source: &str,
) -> Result<Vec<Tree>> {
    let mut parser = Parser::new();
    parser
        .set_language(grammar)
        .context("loading the inline grammar")?;

    let mut trees = Vec::new();
    for ranges in inline_ranges(block_tree) {
        parser
            .set_included_ranges(&ranges)
            .context("restricting the inline parse to one node")?;
        let tree = parser.parse(source, None).context("inline parse")?;
        trees.push(tree);
    }
    Ok(trees)
}

/// The byte ranges holding inline content, one entry per inline node.
///
/// A node's own named children are cut out of its ranges: multi-line inline content
/// inside a list item or block quote carries `block_continuation` markers, and feeding
/// the `>` or the indent to the inline parser would make it part of the text.
fn inline_ranges(block_tree: &Tree) -> Vec<Vec<Range>> {
    let mut all = Vec::new();
    let mut cursor = block_tree.walk();
    let mut recurse = true;
    loop {
        let node = cursor.node();
        // `pipe_table_cell` holds inline content too, so links in a table are found.
        if matches!(node.kind(), "inline" | "pipe_table_cell") {
            let ranges = ranges_excluding_children(node);
            if ranges.iter().any(|r| r.end_byte > r.start_byte) {
                all.push(ranges);
            }
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
                return all;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A node's range minus the ranges of its named children.
fn ranges_excluding_children(node: Node<'_>) -> Vec<Range> {
    let mut remaining = node.range();
    let mut ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let child = child.range();
        ranges.push(Range {
            start_byte: remaining.start_byte,
            start_point: remaining.start_point,
            end_byte: child.start_byte,
            end_point: child.start_point,
        });
        remaining.start_byte = child.end_byte;
        remaining.start_point = child.end_point;
    }
    ranges.push(remaining);
    ranges
}

/// A parsed source file.
pub struct Parsed {
    pub language: Language,
    pub tree: Tree,
    /// Sub-trees of the inline content, for languages whose grammar splits block and
    /// inline parsing (Markdown). Every byte offset in them indexes the original
    /// source, exactly as in [`Parsed::tree`].
    pub inline_trees: Vec<Tree>,
    /// Byte spans of Helm `{{ ... }}` template actions, masked out before YAML parsing.
    pub masked_spans: Vec<Span>,
    /// Why facts drawn from this tree fall short of the file, empty when they do not.
    /// Settled here because the reasons need the original source, which the tree does
    /// not keep, and because every caller that indexes a file needs the same answer.
    pub gaps: Vec<FactGap>,
}

impl Parsed {
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// The root of every inline sub-tree, in source order.
    pub fn inline_roots(&self) -> impl Iterator<Item = Node<'_>> {
        self.inline_trees.iter().map(|tree| tree.root_node())
    }

    /// Every root that describes this file: the block tree, then the inline sub-trees.
    pub fn roots(&self) -> impl Iterator<Item = Node<'_>> {
        std::iter::once(self.root()).chain(self.inline_roots())
    }

    /// Does any tree contain an ERROR or MISSING node?
    pub fn has_errors(&self) -> bool {
        self.roots().any(|root| root.has_error())
    }

    /// Byte spans of every ERROR and MISSING node, in source order.
    ///
    /// The edit engine compares these before and after an edit: an edit that
    /// introduces new syntax errors is rejected and not written.
    pub fn error_spans(&self) -> Vec<Span> {
        let mut errors = Vec::new();
        for root in self.roots() {
            collect_error_spans(root, &mut errors);
        }
        errors.sort();
        errors
    }

    /// The smallest named node whose span contains `offset`, across every tree.
    pub fn node_at(&self, offset: usize) -> Option<Node<'_>> {
        self.smallest_covering(offset, offset.saturating_add(1), |root, start, end| {
            root.named_descendant_for_byte_range(start, end)
        })
    }

    /// The smallest node covering `start..end`, across every tree.
    ///
    /// Where an inline sub-tree and the block tree both answer, the inline answer
    /// wins: it describes the same bytes, and describes them in more detail.
    pub fn descendant_at(&self, start: usize, end: usize) -> Option<Node<'_>> {
        self.smallest_covering(start, end.max(start), |root, start, end| {
            root.descendant_for_byte_range(start, end)
        })
    }

    fn smallest_covering(
        &self,
        start: usize,
        end: usize,
        lookup: impl Fn(Node<'_>, usize, usize) -> Option<Node<'_>>,
    ) -> Option<Node<'_>> {
        let mut best: Option<Node<'_>> = None;
        for root in self.roots() {
            // An inline sub-tree covers only its own node, and still answers for a
            // range outside it — with its root. Those answers are discarded here.
            let Some(node) =
                lookup(root, start, end).filter(|n| n.start_byte() <= start && end <= n.end_byte())
            else {
                continue;
            };
            let len = |n: &Node<'_>| n.end_byte() - n.start_byte();
            // `<=` and not `<`: the sub-trees come last, so a tie goes to the
            // more detailed one.
            if best.is_none_or(|current| len(&node) <= len(&current)) {
                best = Some(node);
            }
        }
        best
    }
}

/// Append the span of every ERROR and MISSING node under `root`.
fn collect_error_spans(root: Node<'_>, errors: &mut Vec<Span>) {
    let before = errors.len();
    let mut cursor = root.walk();
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
                // Some grammars flag a subtree as erroneous without producing an
                // ERROR or MISSING node anywhere in it (tree-sitter-zig does this
                // for an empty container body). Reporting nothing here would let
                // the edit engine's before/after comparison see no change and
                // accept an edit that broke the file, so fall back to the
                // narrowest node that still reports an error.
                if errors.len() == before && root.has_error() {
                    errors.push(innermost_error_span(root));
                }
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
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

/// Locate SCSS interpolations `#{ ... }`.
///
/// Braces nest — `#{map-get($m, #{$k})}` is one interpolation — so the scan counts them
/// instead of stopping at the first `}`.
fn find_scss_interpolations(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != b'#' || bytes[i + 1] != b'{' {
            i += 1;
            continue;
        }
        let mut depth = 0usize;
        let mut end = i + 1;
        while end < bytes.len() {
            match bytes[end] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        // An unterminated `#{` is a syntax error in the file itself. Leaving it alone
        // lets the grammar say so instead of masking the evidence.
        if end == bytes.len() {
            break;
        }
        spans.push(Span::new(i, end + 1));
        i = end + 1;
    }
    spans
}

/// Replace every byte of each span with `filler`, keeping newlines so line numbers hold.
fn fill_spans(source: &str, spans: &[Span], filler: u8) -> String {
    let mut out = source.as_bytes().to_vec();
    for span in spans {
        for byte in &mut out[span.start..span.end] {
            if *byte != b'\n' {
                *byte = filler;
            }
        }
    }
    String::from_utf8(out).expect("filling with ASCII preserves UTF-8 validity")
}

/// Locate Go template actions `{{ ... }}`, tolerating `{{- -}}` trim markers.
///
/// Quoted strings inside an action may legally contain `}}`, so the scan tracks
/// quoting instead of searching for the first closing delimiter.
fn find_template_actions(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            let start = i;
            i += 2;

            // A comment's body is opaque: `{{- /* #j={{ $j }} */}}` is one action, and
            // stopping at the first `}}` left ` */}}` behind as text the YAML grammar
            // then had to make sense of.
            let after_dash = i + usize::from(bytes.get(i) == Some(&b'-'));
            let body = source[after_dash.min(source.len())..].trim_start();
            if body.starts_with("/*") {
                let opened = after_dash + (source[after_dash..].len() - body.len());
                match source[opened..].find("*/") {
                    Some(at) => {
                        i = opened + at + 2;
                        while i < bytes.len() && bytes[i] != b'}' {
                            i += 1;
                        }
                        i = (i + 2).min(bytes.len());
                    }
                    None => i = bytes.len(),
                }
                spans.push(Span::new(start, i));
                continue;
            }

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

/// Replace each span with filler of the same byte length, preserving newlines so
/// line numbering and every byte offset outside the spans stay identical.
///
/// The filler cannot be one character. An action alone on its line — a `{{- if }}`
/// wrapping other keys — has to become blank, or the line reads as a stray scalar.
/// But an action *within* a value has to become scalar text: masking
/// `name: {{.Release.Name}}-{{.Chart.Name}}` with spaces leaves `name:` followed by a
/// lone `-`, which YAML rejects. So the choice is made per line, by whether anything
/// other than the actions is on it.
fn mask_spans(source: &str, spans: &[Span]) -> String {
    let mut out = source.as_bytes().to_vec();
    for span in spans {
        // Scalar filler is only right where a *value* is expected. An action alone on
        // its line is structural and must vanish, and so must one in key position: a
        // key masked to `xxx` is reported under the action's own source text, giving a
        // symbol named `{{ $key }}` that renaming would happily rewrite. Blanked, the
        // entry is absent instead, which `FactGap::TemplatedKeys` then reports.
        let filler = if line_is_only_actions(source, spans, *span)
            || starts_the_line(source, *span)
            || supplies_the_block_below(source, spans, *span)
            || in_key_position(source, *span)
        {
            b' '
        } else {
            // A plain-scalar character, so the surrounding text still parses as a value.
            b'x'
        };
        // The scalar filler belongs to the line the action starts on. An action that
        // runs over a newline continues on lines of its own, and a scalar character at
        // the start of one of those is content the surrounding structure did not ask
        // for — inside a block scalar it lands at column zero and ends the block.
        // `SERVICE_NAMES="{{` with its body on the next line does exactly that.
        let mut seen_newline = false;
        for b in &mut out[span.start..span.end] {
            match *b == b'\n' {
                true => seen_newline = true,
                false => *b = if seen_newline { b' ' } else { filler },
            }
        }

        // Spaces are wrong in one place: the first line of a block scalar. YAML rejects
        // a *leading* empty line indented further than the content, and a masked action
        // is a whitespace run as long as the action was, so
        //
        //     redis.conf: |-
        //       {{- $password := include "redis.password" . }}
        //       user default on …
        //
        // became forty-nine spaces above content indented two. A `#` there is ordinary
        // block-scalar content at the indentation the block wants.
        //
        // Only there. An action-only line anywhere else is legal as spaces, and a `#`
        // at a lower indentation than the block would *end* the scalar — which is what
        // `health-configmap.yaml` does with `{{- if … }}` written at column zero inside
        // an indented script.
        if filler == b' ' && starts_the_line(source, *span) && opens_a_block_scalar(source, *span) {
            out[span.start] = b'#';
        }
    }
    // Masking only ever replaces bytes with ASCII, so UTF-8 stays valid.
    String::from_utf8(out).expect("masking preserves UTF-8 validity")
}

/// Does this action stand where a mapping key belongs?
///
/// Such an entry reaches the index under no name at all — the mask blanks it, and a
/// blank key matches no capture — which is why the gap is reported and not left to
/// the parse. Whether the blank *also* trips the grammar depends on what surrounds it:
/// `{{ $k }}: v` alone under its parent parses, the same line beside a second pair does
/// not, so the parse error cannot be the signal.
fn in_key_position(source: &str, span: Span) -> bool {
    let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // A block sequence entry's `- ` is structure, not a key, so an action after it is
    // still in key position.
    let opens_the_entry = source[line_start..span.start]
        .trim()
        .chars()
        .all(|c| c == '-');
    opens_the_entry && source[span.end..].trim_start_matches(' ').starts_with(':')
}

/// Is this span the first thing on its line?
fn starts_the_line(source: &str, span: Span) -> bool {
    let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..span.start].trim().is_empty()
}

/// Is every non-whitespace byte on this span's line part of some template action?
fn line_is_only_actions(source: &str, spans: &[Span], span: Span) -> bool {
    let bytes = source.as_bytes();
    let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = source[span.end..]
        .find('\n')
        .map(|i| span.end + i)
        .unwrap_or(source.len());

    (line_start..line_end)
        .all(|i| bytes[i].is_ascii_whitespace() || spans.iter().any(|s| s.contains_offset(i)))
}

/// Is this span on the first line after a block scalar's header?
///
/// `key: |-`, `key: >`, and the `+`/`-` chomping variants. Only there does a
/// whitespace-only line have to be no wider than the content that follows it.
fn opens_a_block_scalar(source: &str, span: Span) -> bool {
    let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let Some(previous) = source[..line_start.saturating_sub(1)].rsplit('\n').next() else {
        return false;
    };
    // The indicator is the last thing on the line, whether the block belongs to a key
    // (`redis.conf: |-`) or to a sequence item (`- |`).
    matches!(
        previous.split_whitespace().next_back(),
        Some("|") | Some("|-") | Some("|+") | Some(">") | Some(">-") | Some(">+")
    )
}

/// Does this action stand where a *block* goes instead of a scalar?
///
/// `labels: {{- include "common.labels.standard" . | nindent 4 }}` with lines indented
/// under it renders to a nested mapping, so filling it with a scalar leaves
/// `labels: xxxx` above a deeper mapping — which no YAML parser accepts, and which cost
/// 48 of 92 files across three `bitnami/charts` charts. Every failure there was this.
///
/// The following line's indentation is what distinguishes the two. Where the action
/// supplies the block, the value must be empty for the block below to attach to the key.
fn supplies_the_block_below(source: &str, spans: &[Span], span: Span) -> bool {
    let indent_of = |line: &str| line.len() - line.trim_start().len();
    let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let here = indent_of(&source[line_start..span.start]);

    let mut at = source[span.end..]
        .find('\n')
        .map(|i| span.end + i + 1)
        .unwrap_or(source.len());
    while at < source.len() {
        let end = source[at..]
            .find('\n')
            .map(|i| at + i)
            .unwrap_or(source.len());
        let line = &source[at..end];
        // A line that is only actions masks to blanks, so it settles nothing.
        let blank = line.trim().is_empty()
            || (at..end).all(|i| {
                source.as_bytes()[i].is_ascii_whitespace()
                    || spans.iter().any(|s| s.contains_offset(i))
            });
        if !blank {
            return indent_of(line) > here;
        }
        at = end + 1;
    }
    false
}

/// The expression a declaration binds, where the grammar exposes one.
///
/// One definition, because there were three and they disagreed. Each was written against
/// the languages its caller had in front of it, and each missed a different grammar: a
/// Java local's value hangs off a declarator and not off the declaration, so `fr inline`
/// called every Java local uninitialised; `fr type` answered `var` — the keyword that
/// means "work it out" — as though it were the type written down, because it could not
/// reach the value to work anything out from.
///
/// The shapes, and why each is here:
///
/// * `value`, `right`, `default_value` — what most grammars call the field.
/// * `declarator` — Java and the C family put the name and the value together, because
///   one statement may declare several: `int a = 1, b = 2;`.
/// * `expression_list` — Go wraps every bound value in a list, even where it binds one.
/// * no field at all — Zig gives a variable declaration's children no names, so the value
///   is the last of them, unless the declaration states a type and binds nothing.
pub fn declaration_value<'t>(declaration: Node<'t>) -> Option<Node<'t>> {
    let bound = ["value", "right", "default_value"]
        .iter()
        .find_map(|field| declaration.child_by_field_name(field))
        .or_else(|| {
            declaration
                .child_by_field_name("declarator")
                .and_then(declaration_value)
        })
        .or_else(|| zig_bound_value(declaration))
        .or_else(|| after_the_equals(declaration))?;

    match bound.kind() == "expression_list" && bound.named_child_count() == 1 {
        true => bound.named_child(0),
        false => Some(bound),
    }
}

/// Zig's variable declaration names none of its children, so the value is the last one —
/// unless that is the type it states, in which case it binds nothing.
fn zig_bound_value<'t>(declaration: Node<'t>) -> Option<Node<'t>> {
    if declaration.kind() != "variable_declaration" {
        return None;
    }
    let count = u32::try_from(declaration.named_child_count()).ok()?;
    let last = declaration.named_child(count.checked_sub(1)?)?;
    (Some(last) != declaration.child_by_field_name("type")).then_some(last)
}

/// The node after an `=`, for a grammar that names neither side.
fn after_the_equals<'t>(declaration: Node<'t>) -> Option<Node<'t>> {
    let mut cursor = declaration.walk();
    let children: Vec<Node<'t>> = declaration.children(&mut cursor).collect();
    let at = children.iter().position(|c| c.kind() == "=")?;
    children
        .get(at + 1)
        .copied()
        .filter(|c| c.kind() != ";" && c.kind() != "}")
}

/// Whether a type as written is a placeholder for one the compiler works out.
///
/// Java's `var`, C++'s `auto` and Go's `:=` all mean "not stated". Reporting the keyword
/// as the declared type answers the question with the question.
pub fn is_an_inferred_type(written: &str) -> bool {
    matches!(written.trim(), "var" | "auto" | "let" | "const")
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
            (
                Language::TypeScript,
                "export function f(a: number) { return a; }\n",
            ),
            (
                Language::Tsx,
                "export const App = () => <div className=\"x\" />;\n",
            ),
            (Language::Python, "def main():\n    return 1\n"),
            (Language::Bash, "main() {\n  echo hi\n}\n"),
            (Language::Html, "<html><body id=\"root\"></body></html>\n"),
            (Language::Css, ".btn { color: red; }\n"),
            (
                Language::Hcl,
                "resource \"aws_s3_bucket\" \"b\" {\n  bucket = var.name\n}\n",
            ),
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
            assert!(
                parsed.root().end_byte() > 0,
                "{lang} produced an empty tree"
            );
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
    fn helm_actions_are_masked_preserving_offsets() {
        let src = "metadata:\n  name: {{ .Values.name }}\n  ns: {{- .Release.ns -}}\n";
        let parsed = parse(Language::Helm, src);
        assert!(
            !parsed.has_errors(),
            "masked Helm should parse as YAML: {:?}",
            parsed.error_spans()
        );
        assert_eq!(parsed.masked_spans.len(), 2);
        // Spans must still index the ORIGINAL source, not the masked copy.
        assert_eq!(parsed.masked_spans[0].text(src), "{{ .Values.name }}");
        assert_eq!(parsed.masked_spans[1].text(src), "{{- .Release.ns -}}");
    }

    #[test]
    fn template_action_with_braces_in_string_is_not_truncated() {
        let src = "a: {{ printf \"}}\" }}\n";
        let actions = find_template_actions(src);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].text(src), "{{ printf \"}}\" }}");
    }

    #[test]
    fn adjacent_actions_in_a_value_still_parse() {
        // `name: {{.Release.Name}}-{{.Chart.Name}}` is ordinary Helm. Masking with
        // spaces left `name:` followed by a lone `-`, which YAML rejects — 28 of the
        // 37 parse failures across the Helm repository were this one shape.
        let src = "metadata:\n  name: {{.Release.Name}}-{{.Chart.Name}}\n";
        let parsed = parse(Language::Helm, src);
        assert!(
            !parsed.has_errors(),
            "adjacent actions in a value should parse: {:?}",
            parsed.error_spans()
        );
        assert_eq!(parsed.masked_spans.len(), 2);
    }

    #[test]
    fn an_action_alone_on_its_line_still_masks_to_blank() {
        // The other half of the same choice: a structural action has to vanish, or
        // the line reads as a stray scalar.
        let src = "spec:\n  {{- if .Values.enabled }}\n  replicas: 1\n  {{- end }}\n";
        let parsed = parse(Language::Helm, src);
        assert!(
            !parsed.has_errors(),
            "a structural action should mask to blank: {:?}",
            parsed.error_spans()
        );
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
    fn markdown_inline_content_is_parsed_into_sub_trees() {
        // The block grammar leaves paragraph text as an opaque `inline` node; the
        // second pass is what turns it into a link.
        let src = "# Title\n\nSee [text](#title) here.\n";
        let parsed = parse(Language::Markdown, src);
        assert!(!parsed.has_errors(), "{:?}", parsed.error_spans());

        // One sub-tree for the heading's content, one for the paragraph's.
        assert_eq!(parsed.inline_roots().count(), 2);
        let link = parsed
            .inline_roots()
            .find_map(|root| {
                let mut cursor = root.walk();
                let found = root
                    .named_children(&mut cursor)
                    .find(|n| n.kind() == "inline_link");
                found
            })
            .expect("the paragraph's link");
        assert_eq!(Span::from(link).text(src), "[text](#title)");
    }

    #[test]
    fn markdown_inline_spans_index_the_original_document() {
        // The property everything downstream depends on: the inline parser is handed
        // the whole source with its ranges narrowed, so it can only ever report
        // offsets into the original document.
        let lead = "Filler paragraph that shifts every following offset.\n\n";
        let src = format!("{lead}A [label][target] link.\n");
        let parsed = parse(Language::Markdown, &src);

        let root = parsed.inline_roots().last().expect("the second paragraph");
        assert_eq!(root.start_byte(), lead.len());

        let offset = src.find("target").unwrap();
        let node = parsed.node_at(offset).expect("a node at the label");
        // The inline tree answers, not the block tree's opaque `inline` node.
        assert_eq!(node.kind(), "link_label");
        assert_eq!(Span::from(node).text(&src), "[target]");
    }

    #[test]
    fn markdown_that_aborted_the_previous_grammar_now_parses() {
        // tree-sitter-markdown-fork 0.7 hit an assert() in its C++ inline scanner on
        // this shape and called abort(), killing the process mid-run.
        let cell = "c".repeat(400);
        let dash = "-".repeat(400);
        let src = format!(
            "| {} |\n| {} |\n",
            [cell.as_str(); 4].join(" | "),
            [dash.as_str(); 4].join(" | ")
        );
        let parsed = parse(Language::Markdown, &src);
        assert!(!parsed.has_errors(), "{:?}", parsed.error_spans());
    }

    #[test]
    fn scss_specific_syntax_parses_on_its_own_grammar() {
        // $variables, @mixin and @include are not CSS, and the CSS grammar rejects
        // them; SCSS gets the grammar that knows them.
        let parsed = parse(
            Language::Scss,
            "$brand: #fff;\n@mixin theme($c) { color: $c; }\n.btn { @include theme($brand); }\n",
        );
        assert!(
            !parsed.has_errors(),
            "SCSS should parse cleanly: {:?}",
            parsed.error_spans()
        );
    }

    #[test]
    fn plain_css_still_parses_as_css() {
        let parsed = parse(Language::Css, ".btn { color: red; }\n");
        assert!(!parsed.has_errors());
    }
}
