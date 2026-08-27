//! JSON fact extraction, exercised through the public API.
//!
//! A JSON document is a tree of keys, and the key path *is* the API. So every
//! member key is a definition whose containment gives it a path, the same way
//! a values file's keys do. JSON has no anchors, no aliases and no comments, so
//! there is no intra-file reference edge at all. A reference into a document
//! comes from outside it.
//!
//! Terraform accepts `.tf` and `.tf.json` as two spellings of one language, and
//! a workspace may hold both. A build that could not read JSON could not follow
//! a reference out of one.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn json(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Json, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "the fixture does not parse: {src}\n{}",
        parsed.tree.root_node().to_sexp()
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

fn qualified(f: &FileFacts) -> Vec<String> {
    f.symbols.iter().map(|s| s.qualified_name()).collect()
}

fn sym<'a>(f: &'a FileFacts, qualified_name: &str) -> &'a Symbol {
    f.symbols
        .iter()
        .find(|s| s.qualified_name() == qualified_name)
        .unwrap_or_else(|| panic!("no symbol {qualified_name}: {:?}", qualified(f)))
}

#[test]
fn every_member_key_is_a_definition() {
    let facts = json("{\n  \"name\": \"thing\",\n  \"version\": 1\n}\n");
    let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"name"), "{names:?}");
    assert!(names.contains(&"version"), "{names:?}");
    assert_eq!(sym(&facts, "name").kind, SymbolKind::Key);
}

#[test]
fn a_nested_key_carries_the_path_that_addresses_it() {
    // The path is the whole of a key's identity. `tag` on its own names nothing
    // a caller can reach; `image::tag` does.
    let facts = json("{\n  \"image\": {\n    \"tag\": \"v1\"\n  }\n}\n");
    let paths = qualified(&facts);
    assert!(paths.contains(&"image".to_string()), "{paths:?}");
    assert!(paths.contains(&"image::tag".to_string()), "{paths:?}");
}

#[test]
fn a_key_inside_an_array_of_objects_still_carries_its_path() {
    let facts = json("{\n  \"steps\": [\n    { \"run\": \"make\" }\n  ]\n}\n");
    let paths = qualified(&facts);
    assert!(paths.contains(&"steps".to_string()), "{paths:?}");
    assert!(paths.contains(&"steps::run".to_string()), "{paths:?}");
}

#[test]
fn the_name_is_the_text_between_the_quotes() {
    // A rename rewrites the key and leaves the quotes where they are. Spanning
    // them would put a second pair inside the first.
    let source = "{\n  \"name\": \"thing\"\n}\n";
    let facts = json(source);
    let key = sym(&facts, "name");
    assert_eq!(key.name_span.text(source), "name");
    // The quote is still there on either side of it.
    assert_eq!(source.as_bytes()[key.name_span.end], b'"');
}

#[test]
fn a_value_is_not_a_definition() {
    // Only keys are the API. A string value that happens to read like a name is
    // data, and reporting it would make every rename ask about the wrong thing.
    let facts = json("{\n  \"name\": \"version\"\n}\n");
    let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names.iter().filter(|n| **n == "version").count(),
        0,
        "{names:?}"
    );
}

#[test]
fn a_document_that_is_an_array_reports_the_keys_inside_it() {
    let facts = json("[\n  { \"id\": 1 },\n  { \"id\": 2 }\n]\n");
    let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names.iter().filter(|n| **n == "id").count(), 2, "{names:?}");
}
