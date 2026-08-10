//! Links to a heading or an element id, and what renaming one has to rewrite.
//!
//! Both query files said the engine strips the `#` when it resolves a fragment. It
//! could not: resolution looked the reference name up verbatim and returned before
//! reaching the code that would have stripped it, so `[jump](#beta)` resolved to
//! nothing and `## Beta` was reported as unused. Renaming the heading then rewrote the
//! heading alone and left the link pointing at an anchor that no longer existed.
//!
//! A heading is referenced by its slug instead of its text, which is the other half:
//! matching `#two-words` to `Two Words` needs the slug on both sides, and so does
//! writing the new one.

use fun_refactor::index::Index;
use fun_refactor::model::{SymbolId, SymbolKind};
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

fn only(index: &Index, name: &str) -> SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
    found[0].id
}

fn resolved_references(index: &Index, name: &str) -> usize {
    let id = only(index, name);
    index.references_to(id).len()
}

#[test]
fn a_link_resolves_to_the_heading_it_names() {
    let (_tmp, index) = workspace(&[("a.md", "# Two Words\n\n[jump](#two-words)\n")]);
    assert_eq!(resolved_references(&index, "Two Words"), 1);
}

#[test]
fn a_link_from_another_file_resolves_too() {
    let (_tmp, index) = workspace(&[
        ("a.md", "[other](b.md#the-detail)\n"),
        ("b.md", "# The detail\n"),
    ]);
    assert_eq!(resolved_references(&index, "The detail"), 1);
}

#[test]
fn an_absolute_url_names_another_document() {
    // Its fragment is not this workspace's to resolve, and entering it in the index
    // under a name no file defines is worse than not seeing it at all.
    let (_tmp, index) = workspace(&[("a.md", "# Top\n\n[out](https://example.com/p#top)\n")]);
    assert_eq!(resolved_references(&index, "Top"), 0);
    assert!(
        !index
            .references
            .iter()
            .any(|r| r.name.contains("://") || r.name == "top"),
        "{:?}",
        index.references.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
}

#[test]
fn a_bare_hash_names_the_top_of_the_page() {
    let (_tmp, index) = workspace(&[("a.md", "# Top\n\n[up](#)\n")]);
    assert!(index.references.is_empty(), "{:?}", index.references);
}

#[test]
fn an_html_fragment_resolves_to_the_element_id() {
    let (_tmp, index) = workspace(&[(
        "p.html",
        "<div id=\"top\">x</div>\n<a href=\"#top\">a</a>\n<a href=\"other.html#top\">b</a>\n",
    )]);
    assert_eq!(resolved_references(&index, "top"), 2);
}

#[test]
fn renaming_a_heading_rewrites_the_links_as_slugs() {
    let (_tmp, index) = workspace(&[
        ("a.md", "# Two Words\n\n[jump](#two-words)\n"),
        ("b.md", "[cross](a.md#two-words)\n"),
    ]);
    let plan = rename::plan(&index, only(&index, "Two Words"), "Three Big Words").unwrap();
    let written: Vec<&str> = plan
        .edits
        .iter()
        .flat_map(|(_, edits)| edits.iter().map(|e| e.replacement.as_str()))
        .collect();
    assert!(
        written.contains(&"Three Big Words"),
        "the heading itself: {written:?}"
    );
    assert_eq!(
        written.iter().filter(|w| **w == "three-big-words").count(),
        2,
        "both links, as slugs: {written:?}"
    );
}

#[test]
fn renaming_an_element_id_rewrites_the_href_verbatim() {
    // An id is not slugged: `href="#top"` names it exactly as written.
    let (_tmp, index) = workspace(&[(
        "p.html",
        "<div id=\"top\">x</div>\n<a href=\"#top\">a</a>\n",
    )]);
    let plan = rename::plan(&index, only(&index, "top"), "lede").unwrap();
    let written: Vec<&str> = plan
        .edits
        .iter()
        .flat_map(|(_, edits)| edits.iter().map(|e| e.replacement.as_str()))
        .collect();
    assert_eq!(written, ["lede", "lede"], "{written:?}");
}

#[test]
fn a_heading_may_be_renamed_to_something_with_spaces_in_it() {
    // Headings are prose. Every other markup name is one token, which is why the rule
    // was written for all of them at once.
    let (_tmp, index) = workspace(&[("a.md", "# Start\n")]);
    let id = only(&index, "Start");
    assert_eq!(index.symbol(id).unwrap().kind, SymbolKind::Heading);
    assert!(rename::plan(&index, id, "Getting Started").is_ok());
    assert!(
        rename::plan(&index, id, "Two\nLines").is_err(),
        "a heading is one line"
    );
}

#[test]
fn a_linked_heading_is_not_reported_unused() {
    let (_tmp, index) = workspace(&[
        ("a.md", "# Linked\n\n# Orphan\n"),
        ("b.md", "[go](a.md#linked)\n"),
    ]);
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    let unused = fun_refactor::refactor::delete::find_unused(&index, &entrypoints);
    let names: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(!names.contains(&"Linked"), "{names:?}");
    assert!(names.contains(&"Orphan"), "{names:?}");
}
