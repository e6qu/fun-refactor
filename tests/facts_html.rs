//! HTML fact extraction.
//!
//! HTML is the consumer side of CSS: element ids are defined here, CSS classes
//! are used here. The tests pin the exact name spans, because those are the bytes
//! a cross-language rename rewrites.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Html, src).unwrap();
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
fn id_attribute_defines_an_element_id_without_quotes() {
    let src = "<div id=\"main\"></div>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["main"]);

    let id = &f.symbols[0];
    // The name span is the value only: a rename must not eat the quotes.
    assert_eq!(id.name_span.text(src), "main");
    assert_eq!(id.full_span.text(src), "id=\"main\"");
    assert!(id.full_span.contains(id.name_span));
}

#[test]
fn single_quoted_and_unquoted_ids_are_handled_alike() {
    let src = "<div id='one'></div><div id=two></div>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["one", "two"]);
    assert_eq!(f.symbols[0].name_span.text(src), "one");
    assert_eq!(f.symbols[1].name_span.text(src), "two");
}

#[test]
fn id_matching_is_case_insensitive_but_anchored() {
    // `ID=` is legal HTML; `data-id=` is a different attribute entirely.
    let src = "<div ID=\"a\" data-id=\"b\"></div>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["a"]);
}

#[test]
fn class_attribute_is_a_string_reference_to_a_css_class() {
    let src = "<div class=\"btn\"></div>\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["btn"]);
    assert_eq!(f.references[0].kind, ReferenceKind::StringRef);
    assert_eq!(f.references[0].span.text(src), "btn");
    // The class attribute defines nothing: `.btn` is defined in the stylesheet.
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
}

#[test]
fn multi_class_values_split_into_one_reference_per_class() {
    // A query cannot subdivide a token, so the grammar hands us `page dark` as one
    // span; the extractor fans it out. Each class gets its own span so renaming one
    // rewrites only its own bytes.
    let src = "<div class=\"page dark\"></div>\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["page", "dark"]);
    assert_eq!(f.references[0].span.text(src), "page");
    assert_eq!(f.references[1].span.text(src), "dark");
    // The spans must be disjoint and sit inside the original value.
    assert!(f.references[0].span.end <= f.references[1].span.start);
}

#[test]
fn unquoted_class_values_are_captured_too() {
    let src = "<div class=bare></div>\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["bare"]);
}

#[test]
fn label_for_and_aria_attributes_reference_element_ids() {
    let src = concat!(
        "<label for=\"name-input\">Name</label>\n",
        "<input id=\"name-input\">\n",
        "<div aria-labelledby=\"name-input\"></div>\n",
    );
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["name-input"]);

    let uses: Vec<_> = f
        .references
        .iter()
        .filter(|r| r.name == "name-input")
        .collect();
    assert_eq!(uses.len(), 2, "got {uses:?}");
    assert!(uses.iter().all(|r| r.kind == ReferenceKind::StringRef));
    // The definition's own value is not double-counted as a reference.
    let def_start = src.find("id=\"name-input\"").unwrap();
    assert!(uses.iter().all(|r| r.span.start != def_start + 4));
}

#[test]
fn in_document_anchor_hrefs_are_references() {
    let src = "<a href=\"#sec\">go</a>\n<h2 id=\"sec\">Sec</h2>\n";
    let f = facts(src);
    let anchor = f
        .references
        .iter()
        .find(|r| r.kind == ReferenceKind::StringRef && r.name == "sec")
        .expect("anchor reference");
    // The grammar offers no node for the fragment alone, so the extractor narrows the
    // captured destination to it. Naming the reference `#sec` matched no symbol, and a
    // rename writing over that span would have taken the `#` with it.
    assert_eq!(anchor.span.text(src), "sec");
    assert_eq!(anchor.kind, ReferenceKind::StringRef);
    assert_eq!(names(&f, SymbolKind::ElementId), ["sec"]);
}

#[test]
fn an_href_names_something_only_when_a_fragment_says_which() {
    // A bare file name is a document, not a symbol in it; `#` alone is the top of the
    // page; and an absolute URL's fragment belongs to another site's document.
    let src = concat!(
        "<a href=\"other.html\">x</a>",
        "<a href=\"https://example.com/p#top\">y</a>",
        "<a href=\"#\">z</a>\n",
    );
    assert!(refs(&facts(src)).is_empty(), "got {:?}", refs(&facts(src)));

    // With a fragment, it names the id — in this document or another one.
    let src = "<a href=\"other.html#sec\">x</a>\n";
    assert_eq!(refs(&facts(src)), ["sec"]);
}

#[test]
fn link_href_and_script_src_are_imports() {
    let src = concat!(
        "<link rel=\"stylesheet\" href=\"theme.css\">\n",
        "<script src=\"app.js\"></script>\n",
    );
    let f = facts(src);
    let mut paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["app.js", "theme.css"]);
    // The import span is the start tag that declares the dependency.
    let css = f.imports.iter().find(|i| i.path == "theme.css").unwrap();
    assert_eq!(
        css.span.text(src),
        "<link rel=\"stylesheet\" href=\"theme.css\">"
    );
    assert!(f.imports.iter().all(|i| !i.is_glob));
}

#[test]
fn anchor_href_is_not_mistaken_for_an_import() {
    let src = "<a href=\"page.html\">x</a>\n";
    let f = facts(src);
    assert!(f.imports.is_empty(), "got {:?}", f.imports);
}

#[test]
fn tag_names_are_never_symbols() {
    let src = "<section><article><p>text</p></article></section>\n";
    let f = facts(src);
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    assert!(f.references.is_empty(), "got {:?}", f.references);
}

#[test]
fn elements_nest_as_scopes() {
    let src = "<body id=\"b\"><div id=\"d\"><span id=\"s\"></span></div></body>\n";
    let f = facts(src);
    let outer = f.scope_at(src.find("id=\"b\"").unwrap()).unwrap();
    let inner = f.scope_at(src.find("id=\"s\"").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
}

#[test]
fn a_realistic_document_extracts_the_css_facing_facts() {
    let src = concat!(
        "<!DOCTYPE html>\n",
        "<html>\n",
        "  <head>\n",
        "    <link rel=\"stylesheet\" href=\"theme.css\">\n",
        "    <script src=\"app.js\"></script>\n",
        "  </head>\n",
        "  <body id=\"root\" class=\"page\">\n",
        "    <a href=\"#main\">skip</a>\n",
        "    <main id=\"main\" class=\"content\">\n",
        "      <label for=\"q\">Search</label>\n",
        "      <input id=\"q\" class=\"field\">\n",
        "    </main>\n",
        "  </body>\n",
        "</html>\n",
    );
    let parsed = Parsers::new().parse(Language::Html, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );

    let f = facts(src);
    let mut ids = names(&f, SymbolKind::ElementId);
    ids.sort();
    assert_eq!(ids, ["main", "q", "root"]);

    let mut r = refs(&f);
    r.sort();
    assert_eq!(r, ["content", "field", "main", "page", "q"]);
    assert_eq!(f.imports.len(), 2);
}
