//! An element id named from code, and the markup that declares it.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

/// The string-keyed references this file makes.
fn names(source: &str) -> Vec<String> {
    let parsed = Parsers::new().parse(Language::TypeScript, source).unwrap();
    let facts = Extractor::new()
        .extract(&parsed, Path::new("t.ts"), source)
        .unwrap();
    facts
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::StringRef)
        .map(|r| r.name.clone())
        .collect()
}

#[test]
fn get_element_by_id_names_an_element_id() {
    let found = names("const el = document.getElementById(\"open-path\");\n");
    assert!(found.contains(&"open-path".to_string()), "{found:?}");
}

#[test]
fn a_query_selector_naming_one_id_names_that_id() {
    // The name is the id the markup declares, without the `#` a selector spells it with.
    let found = names("const el = document.querySelector(\"#panel\");\n");
    assert!(found.contains(&"panel".to_string()), "{found:?}");
}

#[test]
fn a_query_selector_naming_one_class_names_that_class() {
    // The same, without the `.` a selector spells a class with.
    let found = names("const all = document.querySelectorAll(\".card\");\n");
    assert!(found.contains(&"card".to_string()), "{found:?}");
}

#[test]
fn a_compound_selector_is_left_alone() {
    let found = names("const el = document.querySelector(\"div.card > a\");\n");
    assert!(
        !found.iter().any(|n| n.contains('>')),
        "a compound selector should reach nothing: {found:?}"
    );
}

#[test]
fn a_method_of_the_same_name_on_something_else_is_not_this_call() {
    // The receiver decides.
    let found = names("const el = cache.get(\"open-path\");\n");
    assert!(
        !found.contains(&"open-path".to_string()),
        "a plain map read is not a DOM lookup: {found:?}"
    );
}

#[test]
fn the_edge_is_reported_and_never_rewritten() {
    // A string that happens to match an id is indistinguishable from one that names it.
    let parsed = Parsers::new()
        .parse(
            Language::TypeScript,
            "const el = document.getElementById(\"open-path\");\n",
        )
        .unwrap();
    let facts = Extractor::new()
        .extract(
            &parsed,
            Path::new("t.ts"),
            "const el = document.getElementById(\"open-path\");\n",
        )
        .unwrap();
    let found = facts
        .references
        .iter()
        .find(|r| r.name == "open-path")
        .expect("the reference");
    assert_eq!(found.kind, ReferenceKind::StringRef);
    assert_ne!(
        found.confidence,
        Confidence::Exact,
        "nothing proves the string names an id"
    );
}
