//! XML fact extraction.
//!
//! Two things are renameable in XML: the id/idref graph and namespace prefixes.
//! Two grammar limits shape every assertion below, and both are pinned by tests
//! rather than papered over:
//!
//! * an attribute value is only addressable *with* its surrounding quotes, so
//!   `id="a"` yields the name `"a"`, quotes included;
//! * a prefixed name is a single token, so `foo:child` yields one span covering
//!   prefix and local part together.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Xml, src).unwrap();
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
fn id_attribute_defines_an_element_id_with_quotes_trimmed() {
    let src = "<root><child id=\"a\"/></root>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["a"]);

    let id = &f.symbols[0];
    // The grammar gives no node for the text between the quotes, so the query
    // captures the quoted token; the extractor trims the quotes, leaving a span a
    // rename can rewrite directly.
    assert_eq!(id.name_span.text(src), "a");
    assert_eq!(id.full_span.text(src), "id=\"a\"");
    assert!(id.full_span.contains(id.name_span));
}

#[test]
fn xml_id_is_recognised_and_other_attributes_are_not() {
    let src = "<root xml:id=\"top\" name=\"n\" data-id=\"d\"/>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["top"]);
}

#[test]
fn id_matching_is_case_sensitive_as_xml_requires() {
    // Unlike HTML, `ID=` is a different attribute from `id=` in XML.
    let src = "<root ID=\"a\"/>\n";
    let f = facts(src);
    assert!(names(&f, SymbolKind::ElementId).is_empty());
}

#[test]
fn idref_and_ref_attributes_reference_element_ids() {
    let src = "<root><a id=\"x\"/><b idref=\"x\"/><c ref=\"x\"/></root>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["x"]);

    let uses: Vec<_> = f.references.iter().filter(|r| r.name == "x").collect();
    assert_eq!(uses.len(), 2, "got {uses:?}");
    assert!(uses.iter().all(|r| r.kind == ReferenceKind::StringRef));
    // Reference and definition names match exactly, so idref pairs with id.
    assert_eq!(uses[0].name, f.symbols[0].name);
    assert_eq!(uses[0].span.text(src), "x");
}

#[test]
fn fragment_hrefs_reference_ids_but_cross_document_ones_do_not() {
    let src = "<root><a href=\"#x\"/><b href=\"other.xml#x\"/><c href=\"http://e/\"/></root>\n";
    let f = facts(src);
    assert_eq!(refs(&f), ["#x"]);
    // Quotes are trimmed; the `#` stays, since it is part of the fragment syntax
    // rather than padding. Anchor resolution strips it.
    assert_eq!(f.references[0].span.text(src), "#x");
}

#[test]
fn namespace_declaration_defines_a_prefix() {
    let src = "<root xmlns:foo=\"urn:foo\"/>\n";
    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::Module), ["xmlns:foo"]);

    let ns = &f.symbols[0];
    // KNOWN GAP: the span covers `xmlns:foo`, not the bare prefix `foo`.
    assert_eq!(ns.name_span.text(src), "xmlns:foo");
    assert_eq!(ns.full_span.text(src), "xmlns:foo=\"urn:foo\"");
}

#[test]
fn default_namespace_declaration_defines_nothing() {
    // `xmlns="urn:d"` binds no prefix, so there is no name to rename.
    let src = "<root xmlns=\"urn:d\"/>\n";
    let f = facts(src);
    assert!(f.symbols.is_empty(), "got {:?}", f.symbols);
    assert!(f.references.is_empty(), "got {:?}", f.references);
}

#[test]
fn prefixed_element_and_attribute_names_are_references() {
    let src = "<root xmlns:foo=\"urn:foo\"><foo:child foo:attr=\"v\"></foo:child></root>\n";
    let f = facts(src);
    let mut r = refs(&f);
    r.sort();
    // Start tag name, attribute name and end tag name — every occurrence a
    // prefix rename has to visit. Each span is the whole `prefix:local` token.
    assert_eq!(r, ["foo:attr", "foo:child", "foo:child"]);
    assert!(f
        .references
        .iter()
        .all(|r| r.kind == ReferenceKind::Identifier));
}

#[test]
fn unprefixed_names_and_the_xml_prefix_are_not_references() {
    let src = "<root xml:id=\"a\" xml:lang=\"en\"><child/></root>\n";
    let f = facts(src);
    assert!(refs(&f).is_empty(), "got {:?}", refs(&f));
}

#[test]
fn the_xmlns_declaration_is_not_also_a_reference_to_itself() {
    let src = "<root xmlns:foo=\"urn:foo\"/>\n";
    let f = facts(src);
    assert!(refs(&f).is_empty(), "got {:?}", refs(&f));
}

#[test]
fn elements_nest_as_scopes() {
    let src = "<root><mid><leaf id=\"l\"/></mid></root>\n";
    let f = facts(src);
    let outer = f.scope_at(src.find("<mid>").unwrap()).unwrap();
    let inner = f.scope_at(src.find("id=\"l\"").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
}

#[test]
fn a_realistic_document_parses_and_extracts() {
    let src = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<catalog xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xml:id=\"cat\">\n",
        "  <!-- a comment -->\n",
        "  <book id=\"b1\">\n",
        "    <dc:title>T</dc:title>\n",
        "  </book>\n",
        "  <review idref=\"b1\" href=\"#cat\"/>\n",
        "</catalog>\n",
    );
    let parsed = Parsers::new().parse(Language::Xml, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );

    let f = facts(src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["cat", "b1"]);
    assert_eq!(names(&f, SymbolKind::Module), ["xmlns:dc"]);

    let mut r = refs(&f);
    r.sort();
    assert_eq!(r, ["#cat", "b1", "dc:title", "dc:title"]);
}

#[test]
fn imports_are_not_a_thing_in_plain_xml() {
    // XInclude and entity-based includes exist but are not covered; nothing is
    // silently reported as an import.
    let src = "<root><child id=\"a\"/></root>\n";
    assert!(facts(src).imports.is_empty());
}
