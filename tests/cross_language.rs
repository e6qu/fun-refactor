//! Cross-language references — the queries no single-language tool can answer.
//!
//! A CSS class named in an HTML attribute and a TSX `className` is one entity
//! spanning three languages and three grammars. No language server sees across that
//! boundary, so these tests are the ones that justify the whole design.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::model::{Confidence, SymbolKind};
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

/// Build a workspace on disk and index it.
fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

/// Apply a rename plan to one file and return the new text.
fn rendered(plan: &rename::RenamePlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    match plan.edits.edits_for(path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

const CSS: &str = ".btn-primary {\n  color: white;\n}\n.btn-primary:hover { opacity: 0.9; }\n";
const HTML: &str = "<div class=\"btn-primary large\">Click</div>\n<span class=\"other btn-primary\"></span>\n";
const TSX: &str = "export const App = () => <button className=\"btn-primary\">Go</button>;\n";

#[test]
fn html_class_attribute_resolves_to_the_css_selector() {
    let (_tmp, index) = workspace(&[("styles.css", CSS), ("index.html", HTML)]);

    let selectors = index.find_symbols("btn-primary", None);
    assert!(!selectors.is_empty(), "the CSS class should be a symbol");
    assert!(selectors.iter().all(|s| s.kind == SymbolKind::Selector));

    // The HTML references must resolve to the CSS definition, across languages.
    let html_refs: Vec<_> = index
        .references
        .iter()
        .filter(|r| r.file.extension().is_some_and(|e| e == "html"))
        .filter(|r| r.name == "btn-primary")
        .collect();
    assert_eq!(html_refs.len(), 2, "got {html_refs:?}");
    for r in &html_refs {
        assert!(r.target.is_some(), "unresolved: {r:?}");
        assert_eq!(r.confidence, Confidence::Exact);
    }
}

#[test]
fn renaming_a_css_class_rewrites_html_and_tsx() {
    let (tmp, index) = workspace(&[
        ("styles.css", CSS),
        ("index.html", HTML),
        ("src/App.tsx", TSX),
    ]);

    let target = index.find_symbols("btn-primary", None)[0].id;
    let plan = rename::plan(&index, target, "btn-cta").unwrap();

    // Three files, three languages, one entity.
    assert_eq!(plan.edits.file_count(), 3, "expected all three files to change");

    let css = rendered(&plan, &tmp.path().join("styles.css"));
    assert_eq!(
        css,
        ".btn-cta {\n  color: white;\n}\n.btn-cta:hover { opacity: 0.9; }\n"
    );

    let tsx = rendered(&plan, &tmp.path().join("src/App.tsx"));
    assert!(tsx.contains("className=\"btn-cta\""), "got {tsx}");
}

#[test]
fn sibling_classes_in_the_same_attribute_are_untouched() {
    // `class="btn-primary large"` names two classes; renaming one must leave the
    // other exactly as written.
    let (tmp, index) = workspace(&[("styles.css", CSS), ("index.html", HTML)]);
    let target = index.find_symbols("btn-primary", None)[0].id;
    let plan = rename::plan(&index, target, "btn-cta").unwrap();

    let html = rendered(&plan, &tmp.path().join("index.html"));
    assert_eq!(
        html,
        "<div class=\"btn-cta large\">Click</div>\n<span class=\"other btn-cta\"></span>\n"
    );
}

#[test]
fn every_definition_site_of_a_css_class_is_renamed() {
    // CSS has no canonical definition: `.btn` and `.btn:hover` both declare the
    // class, so renaming must not leave one behind.
    let (tmp, index) = workspace(&[("styles.css", CSS)]);
    let target = index.find_symbols("btn-primary", None)[0].id;
    let plan = rename::plan(&index, target, "btn-cta").unwrap();

    let css = rendered(&plan, &tmp.path().join("styles.css"));
    assert!(
        !css.contains("btn-primary"),
        "a definition site was left behind:\n{css}"
    );
}

#[test]
fn an_unrelated_class_of_the_same_shape_is_not_touched() {
    let (tmp, index) = workspace(&[
        ("styles.css", ".btn-primary { color: red; }\n.btn-secondary { color: blue; }\n"),
        ("index.html", "<div class=\"btn-primary\"></div>\n<div class=\"btn-secondary\"></div>\n"),
    ]);
    let target = index
        .find_symbols("btn-primary", None)
        .first()
        .unwrap()
        .id;
    let plan = rename::plan(&index, target, "btn-cta").unwrap();

    let html = rendered(&plan, &tmp.path().join("index.html"));
    assert!(html.contains("class=\"btn-cta\""));
    assert!(
        html.contains("class=\"btn-secondary\""),
        "the other class must be untouched:\n{html}"
    );
}

#[test]
fn html_id_and_label_for_resolve_to_each_other() {
    let (_tmp, index) = workspace(&[(
        "form.html",
        "<label for=\"email\">Email</label>\n<input id=\"email\">\n",
    )]);

    let ids = index.find_symbols("email", None);
    assert!(
        ids.iter().any(|s| s.kind == SymbolKind::ElementId),
        "id attribute should define an element id: {ids:?}"
    );

    let for_ref = index
        .references
        .iter()
        .find(|r| r.name == "email")
        .expect("for= should be a reference");
    assert!(for_ref.target.is_some(), "for= should resolve to the id");
}

#[test]
fn renaming_an_element_id_updates_the_label_that_points_at_it() {
    let (tmp, index) = workspace(&[(
        "form.html",
        "<label for=\"email\">Email</label>\n<input id=\"email\">\n",
    )]);
    let target = index
        .find_symbols("email", None)
        .iter()
        .find(|s| s.kind == SymbolKind::ElementId)
        .unwrap()
        .id;

    let plan = rename::plan(&index, target, "user-email").unwrap();
    let html = rendered(&plan, &tmp.path().join("form.html"));
    assert_eq!(
        html,
        "<label for=\"user-email\">Email</label>\n<input id=\"user-email\">\n"
    );
}

#[test]
fn a_css_custom_property_rename_reaches_its_var_uses() {
    let (tmp, index) = workspace(&[(
        "theme.css",
        ":root {\n  --brand: #fff;\n}\n.a { color: var(--brand); }\n.b { border-color: var(--brand); }\n",
    )]);
    let target = index
        .find_symbols("--brand", None)
        .first()
        .expect("custom property should be a symbol")
        .id;

    let plan = rename::plan(&index, target, "--accent").unwrap();
    let css = rendered(&plan, &tmp.path().join("theme.css"));
    assert!(!css.contains("--brand"), "left a use behind:\n{css}");
    assert_eq!(css.matches("--accent").count(), 3, "got:\n{css}");
}

#[test]
fn cross_language_edits_survive_reparse_validation() {
    // The strongest guarantee: every rewritten file still parses in its own
    // language after a cross-language rename.
    let (tmp, index) = workspace(&[
        ("styles.css", CSS),
        ("index.html", HTML),
        ("src/App.tsx", TSX),
    ]);
    let target = index.find_symbols("btn-primary", None)[0].id;
    let plan = rename::plan(&index, target, "btn-cta").unwrap();

    // `plan` reparses each file and refuses anything that would break it.
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("cross-language edits must survive validation");
    assert_eq!(outcomes.len(), 3);
    for outcome in &outcomes {
        assert!(outcome.changed(), "{} did not change", outcome.path.display());
    }
    let _ = Path::new(tmp.path());
}
