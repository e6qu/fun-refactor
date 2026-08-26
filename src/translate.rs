//! Rewriting a file as another language.
//!
//! Two different promises share this command. Where one grammar contains the other, the
//! result is the same bytes under the target's extension, checked by the target's parser.
//! Between programming languages, `src/transpile` produces a draft: signatures carried with
//! their types where possible, and every construct without a counterpart marked, never
//! silently dropped.
//!
//! Some languages contain others. SCSS is a superset of CSS, TSX is TypeScript with JSX, a Helm
//! template is YAML with actions, XHTML is both HTML and XML. For a file using no feature the
//! target lacks, converting between those is a rename plus two checks:
//!
//! 1. The pair appears in [`targets`], a declared relationship between the two grammars.
//!    Without it an empty file would "convert" to anything.
//! 2. The text parses cleanly under the target grammar. SCSS using nesting does not parse as
//!    CSS, and the refusal says where.
//!
//! The result goes beside the original, same stem, target's extension. The original stays: a
//! conversion that deletes its input cannot be reversed from the diff.

use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// What a file of this language can be rewritten as.
///
/// Only relationships where one grammar contains the other. The direction matters: every CSS
/// file is SCSS, but only some SCSS files are CSS. So the parse in [`plan`] is not optional in
/// either direction.
pub fn targets(from: Language) -> &'static [Language] {
    use Language::*;
    match from {
        Css => &[Scss],
        // Downhill needs the parse to agree: nesting, `&`, `$variables`, `@mixin`
        // and `@use` are all SCSS the CSS grammar rejects.
        Scss => &[Css],
        TypeScript => &[Tsx],
        // Only a `.tsx` with no JSX in it is TypeScript.
        Tsx => &[TypeScript],
        // A manifest is a template with no actions in it.
        Yaml => &[Helm],
        Helm => &[Yaml],
        // XHTML is the intersection; the parse decides whether this file is in it.
        Html => &[Xml],
        Xml => &[Html],
        // The one pair here whose bytes change: a document renders to the markup it
        // describes. One way only. HTML back to Markdown would have to decide what
        // is prose and what is structure, and that is authorship, not translation.
        Markdown => &[Html],
        _ => &[],
    }
}

/// One thing a file could be rewritten as, and what that would produce.
#[derive(Debug, Clone)]
pub struct Option_ {
    pub target: Language,
    /// Where the result would be written.
    pub destination: PathBuf,
    /// `None` where the result is the same bytes under a different extension, and the
    /// fidelity of the draft where it is a translation.
    pub fidelity: std::option::Option<crate::transpile::Fidelity>,
    /// Why this target cannot be produced right now, when it cannot.
    ///
    /// A blocked target used to vanish from the listing, and a listing that hides
    /// an entry teaches the reader the pair does not exist.
    pub blocked: std::option::Option<String>,
}

/// Everything `path` could be rewritten as, worked out by asking for each one.
///
/// The list and the answer come from the same call, which keeps them from disagreeing.
/// They were two: the listing walked one set of languages and the request checked another, so
/// `fr translate x.py tsx` succeeded while nothing ever offered it.
///
/// A target that would fail is left out. The reason is available from
/// [`crate::transpile::plan`] or [`plan`] for a caller that asks for it by name.
pub fn options_for(path: &Path) -> Vec<Option_> {
    let Some(from) = crate::lang::detect(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();

    for target in targets(from) {
        let Ok(destination) = destination_for(path, *target) else {
            continue;
        };
        if crate::vfs::exists(&destination) {
            out.push(Option_ {
                target: *target,
                destination,
                fidelity: None,
                blocked: Some(BLOCKED_BY_EXISTING.to_string()),
            });
            continue;
        }
        if let Ok(planned) = plan(path, *target) {
            out.push(Option_ {
                target: *target,
                destination: planned.destination,
                fidelity: None,
                blocked: None,
            });
        }
    }

    // Plus every language this can be translated into, which is a different and much
    // weaker promise, a draft instead of the same bytes.
    if crate::transpile::can_be_read(from) {
        for target in crate::transpile::SUPPORTED {
            if *target == from || out.iter().any(|o| o.target == *target) {
                continue;
            }
            let Ok(destination) = destination_for(path, *target) else {
                continue;
            };
            if crate::vfs::exists(&destination) {
                out.push(Option_ {
                    target: *target,
                    destination,
                    fidelity: None,
                    blocked: Some(BLOCKED_BY_EXISTING.to_string()),
                });
                continue;
            }
            if let Ok(planned) = crate::transpile::plan(path, *target) {
                out.push(Option_ {
                    target: *target,
                    destination: planned.destination,
                    fidelity: Some(planned.fidelity),
                    blocked: None,
                });
            }
        }
    }
    out
}

/// Why a language cannot be rewritten as another, in words a person can act on.
///
/// Returned instead of a silent empty list so the interface can say *why* the button
/// is doing nothing, which for the imperative languages is the whole story.
pub fn why_not(from: Language, to: Language) -> String {
    use crate::lang::LanguageClass;
    if from == to {
        return format!("{from} is already {to}");
    }
    if from.class() == LanguageClass::Imperative && to.class() == LanguageClass::Imperative {
        let has_reader = crate::transpile::can_be_read(from);
        let has_writer = crate::transpile::can_be_written(to);
        let missing = match (has_reader, has_writer) {
            (false, false) => format!("a reader for {from} and a writer for {to}"),
            (false, true) => format!("a reader for {from}"),
            (true, false) => format!("a writer for {to}"),
            (true, true) => {
                return format!("{from} translates into {to} as a draft; ask for it by name.")
            }
        };
        return format!(
            "translating {from} into {to} needs {missing}, which this build lacks. \
             See `src/transpile/` for the pairs it has."
        );
    }
    format!(
        "{to} does not contain {from}: there is no rule that turns one into the other \
         without inventing meaning"
    )
}

/// Why a language can be rewritten as nothing at all.
///
/// Said once, and not by picking an arbitrary target and explaining that pair.
pub fn why_nothing(from: Language) -> String {
    use crate::lang::LanguageClass;
    if from.class() == LanguageClass::Imperative {
        // This used to say "nothing here can do it. So nothing here pretends to", which was
        // true when it was written and became false the day the transpiler landed: Rust, Go,
        // Python and TypeScript translate into one another. A message that denies a capability
        // the tool has misinforms, because the reader believes it and stops looking.
        let supported: Vec<String> = crate::transpile::SUPPORTED
            .iter()
            .map(|language| language.to_string())
            .collect();
        format!(
            "{from} is a programming language, and there is no reader or writer for it \
             here. Translating between programming languages needs one of each, and this \
             build has both for {}. Adding {from} means writing that pair. See \
             `src/transpile/`.",
            supported.join(", ")
        )
    } else {
        format!("no other grammar this build has contains {from}")
    }
}

/// The languages a file could be written *as*, the question asked backwards.
///
/// [`targets`] answers "what can this file become". A recipe naming a target asks
/// the other one, "can anything become this", and it asks before it has a file in
/// hand. Both answers come from the same two tables, so neither can drift.
pub fn sources_for(to: Language) -> Vec<Language> {
    Language::ALL
        .iter()
        .copied()
        .filter(|from| *from != to)
        .filter(|from| targets(*from).contains(&to) || crate::transpile::can_be_written(to))
        .collect()
}

/// Why nothing at all can be written as this language.
pub fn why_nothing_into(to: Language) -> String {
    use crate::lang::LanguageClass;
    if to.class() == LanguageClass::Imperative {
        format!(
            "{to} is a programming language, and there is no writer for it here. \
             Translating into a programming language needs one, and this build has \
             writers for {}. See `src/transpile/`.",
            crate::transpile::SUPPORTED
                .iter()
                .map(|language| language.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("no grammar this build has is contained by {to}, and nothing renders to it")
    }
}

/// The one reason a listing shows a target it cannot produce.
pub const BLOCKED_BY_EXISTING: &str =
    "the destination already exists. --force overwrites it, --out chooses another path.";

/// A rewrite that has been worked out but not applied.
#[derive(Debug)]
pub struct TranslatePlan {
    pub from: Language,
    pub to: Language,
    pub source: PathBuf,
    /// Same stem, same directory, the target's canonical extension.
    pub destination: PathBuf,
    pub edits: EditSet,
}

/// Where the rewritten file goes: same directory, same stem, the target's extension.
pub fn destination_for(path: &Path, to: Language) -> Result<PathBuf> {
    let extension = to
        .extensions()
        .first()
        .ok_or_else(|| anyhow::anyhow!("{to} has no file extension to write"))?;
    let Some(stem) = path.file_stem() else {
        bail!("{} has no file name", path.display());
    };
    // Java ties the file's name to the public class inside it. So `sensors.py` has to become
    // `Sensors.java`, not `sensors.java`, which will not compile whatever is written in it.
    let stem = match to {
        Language::Java => pascal_case(&stem.to_string_lossy()),
        _ => stem.to_string_lossy().to_string(),
    };
    let mut destination = path.to_path_buf();
    destination.set_file_name(format!("{stem}.{extension}"));
    Ok(destination)
}

/// `sensor_readings` -> `SensorReadings`, for a language that names files after types.
fn pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == '.' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Work out how to rewrite `path` as `to`, refusing when it is not the same file.
pub fn plan(path: &Path, to: Language) -> Result<TranslatePlan> {
    plan_to(path, to, None, false)
}

/// [`plan`], with the destination and the overwrite decision in the caller's hands.
pub fn plan_to(
    path: &Path,
    to: Language,
    out: std::option::Option<&Path>,
    force: bool,
) -> Result<TranslatePlan> {
    crate::capabilities::record(crate::capabilities::Capability::Translate, to);
    let Some(from) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if !targets(from).contains(&to) {
        bail!("{}", why_not(from, to));
    }

    let source = crate::vfs::read_to_string(path)?;
    let destination = match out {
        Some(out) => out.to_path_buf(),
        None => destination_for(path, to)?,
    };
    if crate::vfs::exists(&destination) && !force {
        bail!(
            "{} already exists; rewriting {} would overwrite it. --force overwrites, \
             --out chooses another path.",
            destination.display(),
            path.display()
        );
    }

    // The grammar is the oracle. A superset conversion is still checked, because the
    // supersets are only supersets in the parts of them anyone documents.
    let parsers = Parsers::new();
    if !Parsers::supports(to) {
        bail!("this build has no {to} grammar, so it cannot check the result");
    }

    // The render pair: the bytes change, so the source parses under its own grammar
    // and the *output* is what has to satisfy the target's.
    if from == Language::Markdown && to == Language::Html {
        let parsed = parsers.parse(from, &source)?;
        let html = markdown_to_html(&parsed, &source);
        let check = parsers.parse(to, &html)?;
        if check.has_errors() {
            bail!(
                "the html this render produced does not parse. That is a defect in the                  renderer and not in your file; the output is not written."
            );
        }
        let existing = crate::vfs::read_to_string(&destination)
            .map(|s| s.len())
            .unwrap_or(0);
        let mut edits = EditSet::new();
        edits.add(
            destination.clone(),
            Edit::new(
                crate::span::Span::new(0, existing),
                &html,
                format!("render {} as {to}", path.display()),
            ),
        );
        edits.declare_language(destination.clone(), to);
        return Ok(TranslatePlan {
            from,
            to,
            source: path.to_path_buf(),
            destination,
            edits,
        });
    }

    let parsed = parsers.parse(to, &source)?;
    if parsed.has_errors() {
        let where_ = first_error(&parsed, &source)
            .map(|at| format!(". First at line {}, column {}", at.line, at.col))
            .unwrap_or_default();
        bail!(
            "this file uses {from} the {to} grammar does not accept{where_}. Rewriting it \
             would need a compiler, not a rename."
        );
    }

    // The bytes are unchanged; what changes is which grammar reads them. Writing the
    // whole text as one edit lets the engine's own reparse check see the new file in
    // its new language, which is the same proof again from the other side.
    // Overwriting is replacing; see the same edit in `transpile::plan_to`.
    let existing = crate::vfs::read_to_string(&destination)
        .map(|s| s.len())
        .unwrap_or(0);
    let mut edits = EditSet::new();
    edits.add(
        destination.clone(),
        Edit::new(
            crate::span::Span::new(0, existing),
            &source,
            format!("rewrite {} as {to}", path.display()),
        ),
    );
    edits.declare_language(destination.clone(), to);

    Ok(TranslatePlan {
        from,
        to,
        source: path.to_path_buf(),
        destination,
        edits,
    })
}

/// Line and column of the first syntax error, for a refusal that points at something.
fn first_error(parsed: &crate::parse::Parsed, source: &str) -> Option<crate::span::LineCol> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    let mut earliest: Option<usize> = None;
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            earliest =
                Some(earliest.map_or(node.start_byte(), |e: usize| e.min(node.start_byte())));
            continue;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    let at = earliest?;
    let index = crate::span::LineIndex::new(source);
    Some(index.line_col(at, source))
}

/// Markdown rendered as the HTML it describes.
///
/// The defined subset: headings, paragraphs, lists tight, block quotes, fenced
/// and indented code, thematic breaks. Emphasis, strong emphasis, code spans,
/// inline links and images cross, with raw HTML blocks passed through. A construct outside it,
/// a reference-style link, a table extension, is emitted as its escaped text under
/// a marker comment, never dropped in silence.
fn markdown_to_html(parsed: &crate::parse::Parsed, source: &str) -> String {
    use std::collections::HashMap;
    let mut inline_roots: HashMap<u64, tree_sitter::Node> = HashMap::new();
    for root in parsed.inline_trees.iter().map(|t| t.root_node()) {
        inline_roots.insert(span_key(root.start_byte(), root.end_byte()), root);
    }
    let mut out = String::new();
    render_blocks(parsed.root(), source, &inline_roots, &mut out);
    out
}

/// A byte range as one number, so the map key stays a scalar.
fn span_key(start: usize, end: usize) -> u64 {
    ((start as u64) << 32) | end as u64
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_blocks(
    node: tree_sitter::Node,
    source: &str,
    inline_roots: &std::collections::HashMap<u64, tree_sitter::Node>,
    out: &mut String,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        render_block(child, source, inline_roots, out);
    }
}

fn render_block(
    node: tree_sitter::Node,
    source: &str,
    inline_roots: &std::collections::HashMap<u64, tree_sitter::Node>,
    out: &mut String,
) {
    let text_of = |n: tree_sitter::Node| &source[n.byte_range()];
    match node.kind() {
        "document" | "section" => render_blocks(node, source, inline_roots, out),
        "atx_heading" | "setext_heading" => {
            let mut cursor = node.walk();
            let level = node
                .children(&mut cursor)
                .find_map(|c| match c.kind() {
                    "atx_h1_marker" | "setext_h1_underline" => Some(1),
                    "atx_h2_marker" | "setext_h2_underline" => Some(2),
                    "atx_h3_marker" => Some(3),
                    "atx_h4_marker" => Some(4),
                    "atx_h5_marker" => Some(5),
                    "atx_h6_marker" => Some(6),
                    _ => None,
                })
                .unwrap_or(1);
            out.push_str(&format!("<h{level}>"));
            render_inline_children(node, source, inline_roots, out);
            out.push_str(&format!("</h{level}>\n"));
        }
        "paragraph" => {
            out.push_str("<p>");
            render_inline_children(node, source, inline_roots, out);
            out.push_str("</p>\n");
        }
        "list" => {
            let ordered = match node.named_child(0) {
                Some(item) => {
                    let mut c = item.walk();
                    let mut markers = item.children(&mut c);
                    markers
                        .any(|m| matches!(m.kind(), "list_marker_dot" | "list_marker_parenthesis"))
                }
                None => false,
            };
            let tag = if ordered { "ol" } else { "ul" };
            out.push_str(&format!("<{tag}>\n"));
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                if item.kind() != "list_item" {
                    continue;
                }
                out.push_str("<li>");
                // Tight rendering: an item\'s own paragraph is its content, and a
                // nested block below it renders as the block it is.
                let mut inner = item.walk();
                for piece in item.named_children(&mut inner) {
                    match piece.kind() {
                        "paragraph" => render_inline_children(piece, source, inline_roots, out),
                        kind if kind.starts_with("list_marker") => {}
                        "block_continuation" => {}
                        _ => render_block(piece, source, inline_roots, out),
                    }
                }
                out.push_str("</li>\n");
            }
            out.push_str(&format!("</{tag}>\n"));
        }
        "block_quote" => {
            out.push_str("<blockquote>\n");
            let mut cursor = node.walk();
            for piece in node.named_children(&mut cursor) {
                if piece.kind() == "block_quote_marker" {
                    continue;
                }
                render_block(piece, source, inline_roots, out);
            }
            out.push_str("</blockquote>\n");
        }
        "fenced_code_block" | "indented_code_block" => {
            let mut cursor = node.walk();
            let language = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "info_string")
                .map(|c| text_of(c).trim().to_string());
            let mut cursor = node.walk();
            let content = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "code_fence_content")
                .map(|c| text_of(c).to_string())
                .unwrap_or_else(|| text_of(node).to_string());
            match language {
                Some(language) if !language.is_empty() => out.push_str(&format!(
                    "<pre><code class=\"language-{}\">",
                    escape_html(&language)
                )),
                _ => out.push_str("<pre><code>"),
            }
            out.push_str(&escape_html(&content));
            out.push_str("</code></pre>\n");
        }
        "thematic_break" => out.push_str("<hr />\n"),
        "html_block" => {
            out.push_str(text_of(node));
            out.push('\n');
        }
        // A definition is consumed by the links that reference it; alone it renders
        // nothing. The reference links themselves are outside the subset and say so.
        "link_reference_definition" => {}
        "block_continuation" | "minus_metadata" | "plus_metadata" => {}
        other => {
            out.push_str(&format!(
                "<!-- {}: not translated: {} -->\n<p>{}</p>\n",
                crate::transpile::MARKER,
                other,
                escape_html(text_of(node).trim_end())
            ));
        }
    }
}

/// The inline content under a block node: each `inline` child rendered through the
/// inline grammar\'s own tree for that byte range.
fn render_inline_children(
    node: tree_sitter::Node,
    source: &str,
    inline_roots: &std::collections::HashMap<u64, tree_sitter::Node>,
    out: &mut String,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "inline" {
            continue;
        }
        match inline_roots.get(&span_key(child.start_byte(), child.end_byte())) {
            Some(root) => render_inline(*root, source, out),
            None => out.push_str(&escape_html(&source[child.byte_range()])),
        }
    }
}

/// One inline node: its text, with the spans the grammar marks rendered as tags and
/// the delimiters they hang on dropped.
fn render_inline(node: tree_sitter::Node, source: &str, out: &mut String) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    let mut at = node.start_byte();
    for child in &children {
        if child.start_byte() > at {
            out.push_str(&escape_html(&source[at..child.start_byte()]));
        }
        match child.kind() {
            "emphasis" => {
                out.push_str("<em>");
                render_inline(*child, source, out);
                out.push_str("</em>");
            }
            "strong_emphasis" => {
                out.push_str("<strong>");
                render_inline(*child, source, out);
                out.push_str("</strong>");
            }
            "code_span" => {
                let inner = source[child.byte_range()].trim_matches('`');
                out.push_str("<code>");
                out.push_str(&escape_html(inner));
                out.push_str("</code>");
            }
            "inline_link" => {
                let mut c = child.walk();
                let dest = child
                    .named_children(&mut c)
                    .find(|n| n.kind() == "link_destination")
                    .map(|n| source[n.byte_range()].to_string())
                    .unwrap_or_default();
                let mut c = child.walk();
                let text = child
                    .named_children(&mut c)
                    .find(|n| n.kind() == "link_text");
                out.push_str(&format!("<a href=\"{}\">", escape_html(&dest)));
                match text {
                    Some(text) => render_inline(text, source, out),
                    None => out.push_str(&escape_html(&dest)),
                }
                out.push_str("</a>");
            }
            "image" => {
                let mut c = child.walk();
                let dest = child
                    .named_children(&mut c)
                    .find(|n| n.kind() == "link_destination")
                    .map(|n| source[n.byte_range()].to_string())
                    .unwrap_or_default();
                let mut c = child.walk();
                let alt = child
                    .named_children(&mut c)
                    .find(|n| n.kind() == "image_description")
                    .map(|n| source[n.byte_range()].to_string())
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<img src=\"{}\" alt=\"{}\" />",
                    escape_html(&dest),
                    escape_html(&alt)
                ));
            }
            "hard_line_break" => out.push_str("<br />\n"),
            "backslash_escape" => {
                out.push_str(&escape_html(&source[child.byte_range()][1..]));
            }
            "emphasis_delimiter" | "code_span_delimiter" => {}
            // Raw inline HTML crosses as itself; anything else crosses as its text,
            // escaped, so nothing is dropped even where nothing is understood.
            "html_tag" => out.push_str(&source[child.byte_range()]),
            _ => out.push_str(&escape_html(&source[child.byte_range()])),
        }
        at = at.max(child.end_byte());
    }
    if node.end_byte() > at {
        out.push_str(&escape_html(&source[at..node.end_byte()]));
    }
}
