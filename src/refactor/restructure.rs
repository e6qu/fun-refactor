//! Pattern-based restructuring: rewrite code matching a shape.
//!
//! `$NAME` in a pattern is a metavariable. It matches any single node and binds its text. The
//! same name in the template puts that text back. Matching is structural, so `$A + $B` matches
//! an addition however it is spaced, and never matches inside a string or comment.
//!
//! Matching is syntactic only, with no name resolution and no type information. The
//! reparse every edit goes through is the only check on the result.
//!
//! # How a fragment is parsed
//!
//! A pattern is a *fragment*. `old_api($X)` is not a valid Rust item, and `var.$X` is not a
//! valid Terraform file. Three steps parse one anyway:
//!
//! 1. Encode `$NAME` as the ordinary identifier `FrMetaNAME`, legal in every supported grammar.
//!    `$$NAME` escapes the sigil and stands for a literal `$NAME`, which Bash, SCSS and
//!    Helm sources contain.
//! 2. Parse the encoded fragment inside every per-language wrapper in [`fragment_wrappers`]
//!    that accepts it. One fragment can be several shapes in one language. A CSS pattern may
//!    be a rule, a declaration or a selector. `A | B` in Rust is a bitwise or and an
//!    or-pattern. Only the target says which was meant. Every shape searches, and the first
//!    to match a node anywhere is the one the caller wrote.
//! 3. Match that node structurally against every node of every target file.
//!
//! A wrapper can contribute punctuation the fragment did not write. The CSS declaration wrapper
//! adds the `;` that makes `color: $X` a declaration, and tree-sitter puts that `;` inside the
//! declaration node. The match trims back to the end of the matched node's last named child, so
//! rewriting `color: red;` replaces `color: red` and leaves the terminator.
//!
//! # Members
//!
//! A variant of an enum, a field of a struct and an arm of a match are members. So are a case
//! of a switch and an entry of an object literal. Each parses only inside the thing that holds
//! it, and adding one is most of what changing a program means.
//!
//! A member is written with the separator that puts it in its list, `Scss,`. Most grammars
//! leave that separator out of the member's own node. So a match takes the target's separator
//! with it, and rewriting `Scss,` as two variants leaves two commas rather than three. A
//! trailing separator is optional after the last member, and its absence matches too.
//!
//! # Macro bodies
//!
//! No grammar knows what a Rust macro does with its arguments. `matches!(l, A | B)` holds a
//! flat run of tokens where the source holds an or-pattern. A pattern still has tokens of its
//! own, and those are compared against runs of macro tokens. Bracket nesting is counted, so a
//! metavariable binds `item.name()` whole rather than stopping at the comma inside it.
//!
//! A shape that matched a node wins over one that matched only tokens. Every shape of a
//! pattern has the same tokens, and only a node says which shape was meant.
//!
//! # Per-language notes
//!
//! - **Bash**: a pattern is a command (`curl $URL`), matched against `command` nodes. `$X` is a
//!   metavariable, so a literal expansion must be written `$$X`.
//! - **HCL**: an attribute (`count = $N`) or a block parses bare. A bare expression (`var.$X`)
//!   is wrapped in an attribute.
//! - **YAML/Helm**: a pattern is a mapping pair or a nested mapping. Helm `{{ ... }}` actions
//!   are masked before the YAML parse (see [`crate::parse`]), so they carry no structure. A
//!   pattern may not contain one, and any match whose span covers or touches one is dropped.
//!   The tree never saw those bytes.
//! - **CSS/SCSS**: a rule, a declaration or a selector, in that order of preference.
//! - **HTML/XML**: an element. An XML attribute value is one token including its quotes,
//!   because tree-sitter-xml makes no node for the text between them. A metavariable standing
//!   for a whole quoted leaf binds the target's text without its quotes.
//! - **Markdown**: a block (`## $TITLE`) or an inline (`[$TEXT](old/url)`). The grammar folds a
//!   heading's padding into its content node, so matching compares and binds metavariable text
//!   trimmed.

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
    ///
    /// A comment between the pattern's own tokens has no place in the template, so the
    /// rewrite would delete it. The plan skips those sites and reports them, so a run
    /// that calls itself complete never drops `foo(1, /* why */ 2)` in silence.
    pub skipped_with_comments: Vec<(PathBuf, usize)>,
}

/// Where this pattern matches, rewriting nothing.
///
/// The find half of [`apply`], exposed because a recipe's `matches=` predicate asks this
/// question about a symbol and has no template to offer. Running `apply` with the
/// pattern as its own template answers nothing, since a rewrite that changes nothing is
/// skipped and every match would be discarded.
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

    // Helm's `{{ ... }}` actions are masked out before the YAML parse, so a pattern
    // containing one would be matched against blanks. Refuse instead of matching
    // nothing in silence.
    if language == Language::Helm && pattern.contains("{{") {
        return Err(Refusal::Unsupported {
            operation: "a pattern containing a '{{ ... }}' template action".to_string(),
            language,
            because: "those bytes are masked to whitespace before the YAML parse, so they \
                      carry no structure to match",
        }
        .into());
    }

    // `$X` is invalid syntax in most languages, so the pattern becomes ordinary
    // identifiers before parsing. Left alone it parses into ERROR nodes everywhere
    // except Rust, where `$` is macro syntax.
    let original_pattern = pattern.to_string();
    let encoded = encode_metavariables(pattern);
    let shapes = fragment_shapes(&parsers, language, &encoded, pattern)?;
    let compiled = compile_shapes(&shapes, pattern)?;

    // A metavariable the pattern never binds has nothing to substitute, so the
    // template would emit the literal text `$Y`. The reparse check catches the result
    // and reports "would not parse", which says nothing about the mistake. Name it
    // here, where the mistake is.
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

    // Both are properties of the template, so they are decided once instead of per
    // match site.
    // Bracket only where `( … )` groups a sub-expression. A CSS selector's parent is
    // an `attribute_selector` or a `descendant_selector`, whose names read like
    // operator kinds while bracketing there is a syntax error.
    let groups = crate::refactor::inline::groups_with_parentheses(language);
    // Bracket a metavariable only where the template binds it more tightly than the
    // pattern already did. A method call binds its receiver, so reading the template
    // alone brackets `$X.len()` even when the pattern bound it just as tightly. An
    // identity rewrite must rewrite nothing.
    let tight = match groups {
        true => {
            let by_template = tightly_bound_metavariables(&parsers, language, template);
            let by_pattern = tightly_bound_metavariables(&parsers, language, pattern);
            by_template.difference(&by_pattern).cloned().collect()
        }
        false => HashSet::new(),
    };
    let template_binds = groups && template_is_an_operator_expression(&parsers, language, template);

    // One fragment can be more than one shape in one language, and only the target says
    // which. `A | B` is a bitwise or and an or-pattern, and the wrapper that parses first
    // cannot tell. Every shape searches, and the earliest one that finds anything is the
    // one the caller meant; a shape that matches nowhere had nothing to say.
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
                // written between the pattern's own tokens. Writing over it would delete
                // it, so this leaves the site alone and reports it.
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
                // expression. Whatever the call was an operand of now binds into it.
                if template_binds && matched_in_a_tight_place(&parsed, span) {
                    replacement = format!("({replacement})");
                }
                // Skip a rewrite that changes nothing, and one that moves only
                // whitespace. A template is written on one line, so substituting it over
                // a receiver the author put on its own line pulls the line up. D2 says
                // this tool never pretty-prints, and that covers layout it did not come
                // to change.
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

    // A shape is chosen by the nodes it matched. Every shape finds the same runs of macro
    // tokens, so those say nothing about which shape was meant. A shape that found only
    // them is no better than the first. Those runs travel with whichever shape wins, since
    // a rewrite that reaches inside a macro reaches inside every one.
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

/// A compiled pattern: the fragment's node, the text it was parsed from, and whether
/// its wrapper contributed trailing punctuation a match must not swallow.
struct Pattern<'a> {
    root: Node<'a>,
    source: &'a str,
    trim_trailing: bool,
    /// The separator the pattern was written with and the node left out, if any.
    ///
    /// `Scss,` is how a variant appears in the source. A match takes the target's comma
    /// with it, or the rewrite leaves one behind.
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
///
/// Refuses only when no wrapper does, since a fragment that parses as nothing is a
/// fragment that is not a whole piece of code.
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
        // The node must begin where the fragment begins, since a node starting inside
        // the wrapper belongs to the wrapper. It may reach past the fragment only into
        // punctuation the wrapper itself supplied.
        if root.start_byte() != offset || root.end_byte() > end + suffix.len() {
            continue;
        }
        // A member of a list is written with the separator that puts it there. `Scss,` is
        // how a variant appears, and `pub run: f64,` how a field does. Most grammars leave
        // that separator out of the member's own node, so the node stops short of what was
        // written. Accept the shortfall when it is only the separator.
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
        // Name the mistake, which is nearly always a pattern that is not a whole piece
        // of code. Naming the wrappers here would describe the machinery instead.
        anyhow::bail!("'{display}' is not valid {language}; check for unbalanced brackets.");
    }
    Ok(shapes)
}

/// The pattern node of every shape, refusing the ones that would match everything.
fn compile_shapes<'a>(shapes: &'a [Shape], display: &str) -> Result<Vec<Pattern<'a>>> {
    // Two wrappers can hold the fragment the same way. A shape is what its tree is, so
    // one that repeats an earlier tree would search every file a second time for the
    // same answer.
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

/// A tree written out as the kinds it is made of.
///
/// Two shapes with the same signature over the same text match the same nodes.
/// `Node::to_sexp` says this already, and says it in C. The string it returns is
/// allocated by the parser and freed by the caller. That is one allocation and one
/// crossing of the boundary per shape. Walking the nodes here costs neither.
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
///
/// The template helpers read one shape of a fragment to decide how tightly it binds.
/// That question is about the fragment's own text, and no target answers it.
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
///
/// A language gets more than one entry when a fragment can legitimately be more than one shape.
/// Every wrapper that parses gives a shape, and the target decides between them.
fn fragment_wrappers(language: Language) -> &'static [(&'static str, &'static str)] {
    match language {
        // An expression or statement inside a function body, or a whole item. Then the
        // members: a variant, a field, an arm of a match, and the pattern on its left. A
        // member is neither an item nor an expression, so it needs a wrapper of its own.
        // Adding a variant, and the arms that go with it, is most of what changing an
        // enum means.
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
        // A statement inside a method inside a class, a member inside a class, or a
        // whole type. Java has no top level below the type, so even a bare expression
        // needs two wrappers before the grammar will look at it.
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
        // Python and the JS family accept a bare expression statement. Beyond that: a
        // member of an interface or a class, a case of a switch, a property of an object.
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
        // A fragment of JSON is a value, a member, or a whole document. Only a
        // document parses on its own, so the other two get the brackets that
        // make one.
        Language::Json => &[
            ("", "\n"),
            ("{\"__fr_pattern\": ", "}\n"),
            ("[", "]\n"),
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
    // Descend through single-child wrappers: a node of identical extent adds nothing
    // to the shape, and a statement container is punctuation the wrapper asked for.
    //
    // Descend only when the child starts where the container does. A container whose
    // child begins later carries leading syntax of its own, such as `raise` in `raise
    // Invalid(x)`, and that syntax belongs to the pattern. Descending past it starts
    // the fragment inside itself. That rejects every statement pattern in a language
    // with an empty wrapper, such as Python, shell and YAML.
    loop {
        let named: Vec<Node> = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        };
        // The fragment can sit inside a container the wrapper opened, such as the braces
        // holding an enum's variants. The container begins at its own brace, so the node
        // covering the fragment is the container rather than the member that was
        // written. Step into the child that begins where the fragment does, and stop
        // there. That child is a member of a list, and its role there is the point of
        // writing it. Descending to the identifier inside a variant would match that
        // name everywhere it is written, variant or not.
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
///
/// `$$NAME` is an escaped literal `$NAME` and keeps its sigil. Bash expansions, SCSS
/// variables and Helm values all carry a real `$`, and a pattern must be able to say so.
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
/// A node whose text is `FrMetaX` is the ordinary case. A *leaf* whose text is
/// `"FrMetaX"` is the quoted case, which exists because some grammars never break a
/// quoted value into a node. tree-sitter-xml's `AttValue` is one token, quotes included,
/// so `id="$X"` has nothing else to bind to.
///
/// The quoted spelling stays restricted to leaves. Where a grammar exposes the text
/// inside the quotes, a metavariable binds that inner node, and a pattern string stays a
/// pattern string.
fn metavariable(node: Node<'_>, source: &str) -> Option<Metavariable> {
    // Some grammars fold padding into a node. tree-sitter-yaml keeps the space after
    // a `-` sequence marker inside the item, so compare trimmed.
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

/// One place the pattern matched.
struct Match {
    span: Span,
    /// Whether this site is a run of macro tokens rather than a node.
    ///
    /// The pattern's tokens are the same whatever shape it parsed as, so every shape
    /// finds the same runs. Only a structural match says which shape the caller meant.
    tokens: bool,
    bindings: HashMap<String, String>,
    /// Comments inside the match that no metavariable binding carries over.
    ///
    /// A comment between the pattern's own tokens has nowhere to go in the template, so
    /// the rewrite would drop it. One inside a bound span travels with that binding.
    stranded_comments: Vec<Span>,
}

/// Every match of the pattern in the file, with its metavariable bindings.
fn find_matches(parsed: &Parsed, source: &str, pattern: &Pattern<'_>) -> Vec<Match> {
    let mut results = Vec::new();
    // Markdown's inline content forms a sub-tree of its own, holding its links and
    // emphasis. A pattern over them matches nothing in the block tree.
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
                // Never rewrite inside something already being rewritten. Nested
                // edits overlap, and the engine rejects them.
                continue;
            }
        }
        // A macro's arguments are a bag of tokens. tree-sitter cannot know what
        // `matches!` or `format!` does with them, so it records the whole run flat.
        // `A | B` inside one is a run of tokens and nothing more. The run is still
        // something to match against, and a shape written in Rust has a run of its own.
        if is_macro_tokens(node) {
            results.extend(token_matches(node, source, pattern));
        }
        stack.extend(node.named_children(&mut cursor));
    }

    results.sort_by_key(|found| found.span);
    // A node can appear in two trees at once. Markdown's `inline` is the block
    // grammar's opaque leaf and the inline grammar's root, and rewriting one match
    // twice produces an overlapping edit the engine rejects.
    results.dedup_by_key(|found| found.span);
    // Two matches that overlap without being equal cannot both be rewritten either. The
    // walk already drops a match nested in another. A token run can overlap one that no
    // walk order relates, so the earlier of the pair wins.
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
///
/// The whole invocation, not the bracketed part. `println!` is two tokens outside the
/// brackets, and a pattern naming the macro has to match them.
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
    // Matching it here as well would report the same site twice.
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
///
/// A metavariable binds the run up to the pattern's next literal token, counting
/// brackets so that a comma inside a call belongs to the call. As the last token of a
/// pattern it binds one token, or one bracketed group.
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
///
/// A comment inside a bound span travels into the template with that binding's text.
/// One between the pattern's own tokens has no place in the template, and rewriting
/// over it would delete what somebody wrote.
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

/// Reports whether a match runs into a Helm `{{ ... }}` action.
///
/// The action's bytes are spaces to the YAML parser, so the tree says nothing about
/// them. A span covering one would be rewritten from a parse that never saw it. A span
/// *ending where one begins* is worse. `name: web-{{ .V.x }}` parses as the
/// complete-looking scalar `web-`, so a match there binds half of the real value. Both
/// are dropped, and adjacency counts.
fn touches_template_action(actions: &[Span], span: Span) -> bool {
    actions
        .iter()
        .any(|action| action.start <= span.end && span.start <= action.end)
}

/// The span a match rewrites.
///
/// A wrapper can supply trailing punctuation, such as the `;` that turns `color: $X`
/// into a CSS declaration. The pattern never asked for the target's equivalent
/// punctuation, so the match stops at the last named child.
///
/// The opposite case is a pattern written with its own separator, `Scss,`. There the
/// match takes the target's separator too, since the template carries one and two
/// commas would be left where one was.
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
    // A node can carry the whitespace that follows it. Go's switch case runs to the line
    // the next case starts on, and so does the statement list inside it. Rewriting that
    // far pulls the closing brace onto the last line of the case. The template holds no
    // trailing whitespace, so neither does the match.
    let end = source[..end].trim_end().len().max(start);
    match pattern.separator {
        Some(separator) => Span::new(start, separator_end(source, end, separator)),
        None => Span::new(start, end),
    }
}

/// The byte after the separator that follows `from`, or `from` where none does.
///
/// A separator is optional after the last member of a list. Where one is missing, the
/// source is written a legal way and the match still holds.
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
///
/// Every grammar here puts the word in the kind: `comment`, `line_comment`,
/// `block_comment`, `Comment`. A comment is an extra, so it can sit between any two
/// children of any node. Counting it as one of them makes `foo(1, /* why */ 2)` a
/// three-argument call that `foo($A, $B)` cannot match.
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
///
/// `bound` collects the span each metavariable matched. A caller can then ask which bytes
/// of the match the template carries over.
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
///
/// `$$NAME` is an escaped literal and binds nothing, so this skips it, as
/// substitution does.
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

/// Node kinds that bind their operands, so a captured expression dropped into one
/// keeps its own shape only if it is bracketed.
///
/// The list matches by substring because six grammars name one thing six ways:
/// `binary_expression`, `binary_operator`, `comparison_operator`, `boolean_operator`,
/// `not_operator`, `unary_expression`, `field_expression`, `member_expression`.
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
///
/// `double($X)` → `$X * 2` reads as "the captured expression, times two". Substituting
/// the text of `x + 1` gives `x + 1 * 2`, which is `x + 2`. A captured expression lands
/// in a context it was not written for, and brackets keep its shape.
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
///
/// Inside `2 * double(y)`, the rewrite `double($X)` → `$X / 2` gives `2 * y / 2`. For
/// integers that differs from `2 * (y / 2)`.
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
///
/// A name, a literal, a call and an already-bracketed expression all qualify. Text
/// carrying an operator outside brackets does not.
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
        // A quoted string is one thing, whatever is written inside it. Read otherwise,
        // `"price * quantity"` looks like an expression with an operator, and an
        // identity rewrite brackets it as `("price * quantity").len()`.
        //
        // Double quotes only. A single quote opens a character in Rust and Zig and
        // marks a lifetime in Rust, where `&'a str` has no closing one. Reading it as a
        // delimiter would swallow the rest of the text.
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
///
/// Whitespace is one such choice. A template is written on one line, so substituting it
/// over a receiver the author put on its own line pulls the line up. A trailing comma is
/// the other. `Some(x,)` comes back as `Some(x)`, which compiles and means the same, in
/// a file the user did not ask to change.
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
    use crate::scan::{scan, ScanOptions};
    use std::path::Path;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

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
        // The quoted-metavariable spelling exists for grammars that make a quoted
        // value one opaque token. Rust is not one of those, so `f("$X")` must keep
        // matching strings only, not every argument that happens to be there.
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
