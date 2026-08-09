//! Markdown's two-grammar parse: the block tree plus one sub-tree per inline node.
//!
//! The grammar this replaced (tree-sitter-markdown-fork 0.7) parsed both layers into
//! one tree, and `abort()`ed inside its C++ inline scanner on a wide table — an
//! `assert()` failure no in-process handler can catch, so a single file made the tool
//! unusable over a whole repository. The maintained grammar splits the two layers,
//! which buys correctness at the price of a second parse pass whose spans must still
//! index the original document. Every test here exists to pin one half of that trade:
//! the crash is gone, and no span drifted.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::model::{SymbolKind, *};
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::{extract::Extractor, lang::Language, parse::Parsers};
use std::path::Path;

fn parse(src: &str) -> fun_refactor::parse::Parsed {
    Parsers::new().parse(Language::Markdown, src).unwrap()
}

fn facts(src: &str) -> FileFacts {
    Extractor::new()
        .extract(&parse(src), Path::new("t.md"), src)
        .unwrap()
}

/// The input that killed the old grammar: four columns of ~400 characters each.
fn wide_table() -> String {
    let row = |fill: char| {
        format!(
            "| {} |\n",
            (0..4)
                .map(|_| fill.to_string().repeat(400))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    format!("{}{}", row('c'), row('-'))
}

#[test]
fn a_wide_table_parses_instead_of_aborting_the_process() {
    // This is a reproduction, not a regression guard: the old grammar did not return
    // an error here, it called abort(), so this test could not fail — the process
    // died and took the whole run with it.
    let src = wide_table();
    let parsed = parse(&src);
    assert!(
        !parsed.has_errors(),
        "wide table should parse cleanly: {:?}",
        parsed.error_spans()
    );
    assert_eq!(parsed.root().kind(), "document");
    // Every header cell is inline content, so the second pass really did run over
    // the text that used to trip the scanner.
    assert_eq!(parsed.inline_roots().count(), 4);
}

#[test]
fn a_wide_table_extracts_facts_without_errors() {
    let src = wide_table();
    let f = facts(&src);
    assert!(f.gaps.is_empty());
    assert!(
        f.symbols.is_empty(),
        "a table defines nothing: {:?}",
        f.symbols
    );
}

#[test]
fn links_inside_a_wide_table_cell_keep_their_document_offsets() {
    // The crash input, with a link buried in the last cell.
    let mut src = wide_table();
    src.push_str(&format!(
        "| {} | [see][ref] |\n\n[ref]: /a\n",
        "x".repeat(400)
    ));

    let f = facts(&src);
    let label = f
        .references
        .iter()
        .find(|r| r.name == "ref")
        .unwrap_or_else(|| panic!("link label in a table cell: {:?}", f.references));
    assert_eq!(label.span.text(&src), "ref");
    // Far past the end of any single inline fragment: the span is a document offset.
    assert!(label.span.start > 1000);
}

#[test]
fn inline_spans_are_offsets_into_the_original_document() {
    // The property the whole two-phase parse hangs on. The inline parser is handed
    // the whole source with its included ranges narrowed to one node, so a span it
    // produces indexes the original document — never the extracted fragment.
    let filler = "padding text that pushes every real offset far to the right.\n\n";
    let src = format!("{filler}Read the [manual text][manual] first.\n\n[manual]: /m\n");

    let f = facts(&src);
    let label = f
        .references
        .iter()
        .find(|r| r.name == "manual")
        .unwrap_or_else(|| panic!("expected the label reference: {:?}", f.references));

    // Exactly the label bytes, at the offset the label really occupies — which is
    // past the whole filler paragraph, not an offset into the inline fragment.
    assert_eq!(label.span.text(&src), "manual");
    assert_eq!(label.span.start, src.find("[manual]").unwrap() + 1);
    assert!(label.span.start > filler.len());

    // And the same for the definition it points at.
    let def = f
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::LinkDef)
        .expect("link reference definition");
    assert_eq!(def.name_span.text(&src), "manual");
    assert_eq!(def.name_span.start, src.rfind("[manual]").unwrap() + 1);
}

#[test]
fn inline_content_in_a_block_quote_skips_the_quote_markers() {
    // A quoted paragraph carries `block_continuation` nodes inside its inline node.
    // They are cut out of the ranges handed to the inline parser, so `>` never
    // becomes part of the text — and the spans on the far side of one still line up.
    let src = "> quoted [a](#sec) text\n> and [b](#sec) more\n";
    let f = facts(src);
    let anchors: Vec<_> = f.references.iter().filter(|r| r.name == "#sec").collect();
    assert_eq!(anchors.len(), 2, "got {:?}", f.references);
    assert_eq!(anchors[0].span.text(src), "#sec");
    assert_eq!(anchors[1].span.text(src), "#sec");
    assert_eq!(anchors[1].span.start, src.rfind("#sec").unwrap());
}

#[test]
fn inline_nodes_are_parsed_apart_so_brackets_do_not_pair_across_them() {
    // One parse per inline node, not one parse over all of them: an unclosed `[` at
    // the end of a paragraph must not pair with a `]` in the next one.
    let src = "First paragraph ends with [\n\nsecond starts with ](#nope) here.\n";
    let f = facts(src);
    assert!(
        !f.references.iter().any(|r| r.name == "#nope"),
        "brackets paired across a paragraph boundary: {:?}",
        f.references
    );
}

#[test]
fn a_mixed_document_yields_every_kind_of_fact() {
    let src = concat!(
        "# Guide\n",
        "\n",
        "See [installation](#installation) and the [reference][ref].\n",
        "A [shortcut] too, and an image ![logo][ref].\n",
        "\n",
        "Setext Heading\n",
        "==============\n",
        "\n",
        "| column | link |\n",
        "| ------ | ---- |\n",
        "| a      | [cell](#guide) |\n",
        "\n",
        "## Installation\n",
        "\n",
        "```bash\n",
        "cargo install fun-refactor  # [not]: a link def\n",
        "```\n",
        "\n",
        "[ref]: https://example.com/ref\n",
        "[shortcut]: /s\n",
    );

    let parsed = parse(src);
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );

    let f = facts(src);

    let headings: Vec<_> = f
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Heading)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(headings, ["Guide", "Setext Heading", "Installation"]);

    let mut defs: Vec<_> = f
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::LinkDef)
        .map(|s| s.name.as_str())
        .collect();
    defs.sort();
    assert_eq!(defs, ["ref", "shortcut"]);

    let mut refs: Vec<_> = f.references.iter().map(|r| r.name.as_str()).collect();
    refs.sort();
    assert_eq!(
        refs,
        [
            "#guide",        // a link from inside a table cell
            "#installation", // an inline anchor link
            "bash",          // the code fence's info string
            "ref",           // [reference][ref]
            "ref",           // ![logo][ref]
            "shortcut",      // [shortcut]
        ]
    );

    // Code fence contents are not Markdown: the `[not]:` inside it defines nothing.
    assert!(f.symbols.iter().all(|s| s.name != "not"));

    // Every span still points at what it claims to.
    for symbol in &f.symbols {
        assert_eq!(symbol.name_span.text(src), symbol.name);
    }
    for reference in &f.references {
        assert_eq!(reference.span.text(src), reference.name);
    }
}

/// Build a workspace on disk and index it.
fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

#[test]
fn renaming_a_link_reference_definition_rewrites_the_whole_file() {
    // End to end: the definition lives in the block tree, every use in an inline
    // sub-tree. A span that indexed the fragment rather than the document would
    // corrupt the file here, silently and everywhere.
    let doc = concat!(
        "# Guide\n",
        "\n",
        "Read the [manual][guide-ref] and the [other one][guide-ref].\n",
        "\n",
        "A [guide-ref] shortcut and an image ![alt][guide-ref].\n",
        "\n",
        "[guide-ref]: https://example.com/guide\n",
    );
    let (tmp, index) = workspace(&[("doc.md", doc)]);

    let target = index
        .find_symbols("guide-ref", None)
        .first()
        .expect("the link definition is a symbol")
        .id;
    let plan = rename::plan(&index, target, "handbook").unwrap();

    let path = tmp.path().join("doc.md");
    let rewritten = apply_to_string(doc, plan.edits.edits_for(&path).unwrap()).unwrap();
    assert_eq!(
        rewritten,
        concat!(
            "# Guide\n",
            "\n",
            "Read the [manual][handbook] and the [other one][handbook].\n",
            "\n",
            "A [handbook] shortcut and an image ![alt][handbook].\n",
            "\n",
            "[handbook]: https://example.com/guide\n",
        )
    );
}

#[test]
fn renaming_a_heading_leaves_the_markers_alone() {
    // The closing marker is inside the node the grammar hands over as the heading's
    // content; a rename must still rewrite the title and nothing else.
    let doc = "## Overview ##\n\nBody text.\n";
    let (tmp, index) = workspace(&[("doc.md", doc)]);

    let target = index.find_symbols("Overview", None)[0].id;
    let plan = rename::plan(&index, target, "Summary").unwrap();

    let path = tmp.path().join("doc.md");
    let rewritten = apply_to_string(doc, plan.edits.edits_for(&path).unwrap()).unwrap();
    assert_eq!(rewritten, "## Summary ##\n\nBody text.\n");
}
