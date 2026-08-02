//! Markdown fact extraction.
//!
//! The grammar is tree-sitter-markdown-fork, which parses inline structure into
//! the same tree as the block structure — so links, labels and destinations are
//! all reachable. The grammar hands ATX heading names back with the padding around
//! the text still attached; the extractor trims it, so a heading's name is the title
//! alone and a rename rewrites exactly those bytes. Anchor destinations still include
//! their leading `#`, which is pinned here rather than hidden.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Markdown, src).unwrap();
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

fn names(f: &FileFacts, kind: SymbolKind) -> Vec<&str> {
    f.symbols
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| s.name.as_str())
        .collect()
}

fn refs(f: &FileFacts) -> Vec<&str> {
    f.references.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn atx_headings_define_headings_without_the_markers() {
    let src = "# Title One\n\n## Sub Two\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Heading), ["Title One", "Sub Two"]);

    // The name span is the title alone: no marker, no padding, so renaming a
    // heading rewrites exactly the title bytes.
    let first = &f.symbols[0];
    assert_eq!(first.name_span.text(src), "Title One");
    assert_eq!(first.full_span.text(src), "# Title One");
    assert!(first.full_span.contains(first.name_span));
}

#[test]
fn atx_heading_padding_and_closing_markers() {
    let src = "#   Spaced Title   #\n";
    let f = facts(src);
    // The closing `#` is outside the name; the surrounding spaces are inside it.
    assert_eq!(names(&f, SymbolKind::Heading), ["Spaced Title"]);
    assert_eq!(f.symbols[0].full_span.text(src), "#   Spaced Title   #");
}

#[test]
fn setext_heading_names_have_no_padding() {
    let src = "Under Line\n==========\n\nSecond\n------\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Heading), ["Under Line", "Second"]);
    assert_eq!(f.symbols[0].name_span.text(src), "Under Line");
    assert_eq!(f.symbols[0].full_span.text(src), "Under Line\n==========");
}

#[test]
fn heading_with_inline_markup_keeps_the_whole_content_as_the_name() {
    let src = "## Has `code` and *em*\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Heading), ["Has `code` and *em*"]);
}

#[test]
fn link_reference_definition_defines_the_bare_label() {
    let src = "[label]: http://example.com \"Title\"\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::LinkDef), ["label"]);

    let def = &f.symbols[0];
    // Brackets excluded, destination and title outside the name span.
    assert_eq!(def.name_span.text(src), "label");
    assert_eq!(def.full_span.text(src), "[label]: http://example.com \"Title\"");
}

#[test]
fn full_reference_links_reference_the_label() {
    let src = "See [the text][label].\n\n[label]: /a\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::LinkDef), ["label"]);

    let uses: Vec<_> = f.references.iter().filter(|r| r.name == "label").collect();
    assert_eq!(uses.len(), 1, "got {uses:?}");
    assert_eq!(uses[0].kind, ReferenceKind::StringRef);
    assert_eq!(uses[0].span.text(src), "label");
    // The display text is not a reference: renaming the label must not touch it.
    assert!(!refs(&f).contains(&"the text"));
}

#[test]
fn shortcut_and_collapsed_reference_links_reference_the_label() {
    let src = "[shortcut] and [collapsed][] here.\n\n[shortcut]: /a\n[collapsed]: /b\n";
    let f = facts(src);
    let mut defs = names(&f, SymbolKind::LinkDef);
    defs.sort();
    assert_eq!(defs, ["collapsed", "shortcut"]);

    let mut r = refs(&f);
    r.sort();
    assert_eq!(r, ["collapsed", "shortcut"]);
}

#[test]
fn inline_links_to_anchors_are_references_and_keep_the_hash() {
    let src = "# Title One\n\nJump to [it](#title-one).\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Heading), ["Title One"]);

    let anchor = f
        .references
        .iter()
        .find(|r| r.name.starts_with('#'))
        .expect("anchor reference");
    // KNOWN GAP: `link_destination` is one node, so the `#` is in the span.
    assert_eq!(anchor.name, "#title-one");
    assert_eq!(anchor.span.text(src), "#title-one");
    assert_eq!(anchor.kind, ReferenceKind::StringRef);
}

#[test]
fn external_and_relative_link_destinations_are_not_anchor_references() {
    let src = "[a](http://example.com) [b](./other.md) [c](other.md#frag)\n";
    let f = facts(src);
    assert!(refs(&f).is_empty(), "got {:?}", refs(&f));
}

#[test]
fn anchor_links_inside_lists_are_found_too() {
    let src = "# Sec\n\n- item [a](#sec)\n- item [b](#sec)\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["#sec", "#sec"]);
}

#[test]
fn fenced_code_block_info_string_is_an_identifier_reference() {
    let src = "```rust\nlet x = 1;\n```\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["rust"]);
    assert_eq!(f.references[0].kind, ReferenceKind::Identifier);
    assert_eq!(f.references[0].span.text(src), "rust");
}

#[test]
fn code_block_contents_produce_no_facts() {
    let src = "```rust\n# not a heading\n[not]: a-link-def\n```\n";
    let f = facts(src);
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    assert_eq!(refs(&f), ["rust"]);
}

#[test]
fn footnote_definitions_and_uses_both_surface_as_references() {
    // NOT AVAILABLE: this grammar has no footnote node — `[^fn]: text` is a
    // paragraph starting with a shortcut link — so there is no LinkDef symbol
    // for a footnote. Both occurrences are still found, which is what a rename
    // needs, so this is a missing symbol rather than a missing edit site.
    let src = "Text with a note[^fn].\n\n[^fn]: the note\n";
    let f = facts(src);
    assert!(names(&f, SymbolKind::LinkDef).is_empty());
    assert_eq!(refs(&f), ["^fn", "^fn"]);
}

#[test]
fn headings_do_not_nest_into_scopes() {
    // The grammar emits a flat block list with no `section` node, so every fact
    // in the file shares the document scope.
    let src = "# A\n\ntext\n\n## B\n\nmore\n";
    let f = facts(src);
    assert_eq!(f.scopes.len(), 1);
    assert_eq!(f.symbols.len(), 2);
    assert!(f.symbols.iter().all(|s| s.scope == f.scopes[0].id));
    // Nor does the `##` heading become a child symbol of the `#` heading.
    assert!(f.symbols.iter().all(|s| s.container.is_none()));
}

#[test]
fn a_realistic_document_parses_and_extracts() {
    let src = concat!(
        "# Guide\n",
        "\n",
        "See [installation](#installation) and the [reference][ref].\n",
        "\n",
        "## Installation\n",
        "\n",
        "```bash\n",
        "cargo install x\n",
        "```\n",
        "\n",
        "[ref]: https://example.com/ref\n",
    );
    let parsed = Parsers::new().parse(Language::Markdown, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );

    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Heading), ["Guide", "Installation"]);
    assert_eq!(names(&f, SymbolKind::LinkDef), ["ref"]);

    let mut r = refs(&f);
    r.sort();
    assert_eq!(r, ["#installation", "bash", "ref"]);

    // The anchor and the heading it points at differ by slugging: the heading
    // name is ` Installation`, the link is `#installation`. Reconciling the two
    // is the index's job, and needs the trimming noted at the top of the file.
    let heading = f
        .symbols
        .iter()
        .find(|s| s.name.trim() == "Installation")
        .unwrap();
    assert_eq!(
        heading.name.trim().to_lowercase(),
        f.references
            .iter()
            .find(|r| r.name.starts_with('#'))
            .unwrap()
            .name
            .trim_start_matches('#')
    );
}
