//! CSS/SCSS fact extraction.
//!
//! Each dialect runs on its own grammar and its own query file, so the SCSS tests below pin the
//! two things that could drift. That Sass-only syntax really parses, and that the CSS half of
//! an SCSS file still extracts as CSS does.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
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

#[test]
fn class_selectors_define_selectors_without_the_dot() {
    let src = ".btn, .btn-primary { color: red; }\n";
    let f = facts(Language::Css, src);
    assert_eq!(names(&f, SymbolKind::Selector), ["btn", "btn-primary"]);

    let btn = f.symbols.iter().find(|s| s.name == "btn").unwrap();
    // The name span is the bare class name: a rename rewrites `btn`. It is not `.btn`.
    assert_eq!(btn.name_span.text(src), "btn");
    assert_eq!(btn.full_span.text(src), ".btn");
    assert!(btn.full_span.contains(btn.name_span));
}

#[test]
fn every_selector_occurrence_is_a_definition_site() {
    // CSS has no single canonical definition of a class; a rename must rewrite
    // all of them, so all of them are recorded.
    let src = ".btn { color: red; }\n.btn:hover { color: blue; }\n.a .btn { top: 0; }\n";
    let f = facts(Language::Css, src);
    assert_eq!(names(&f, SymbolKind::Selector), ["btn", "btn", "a", "btn"]);
}

#[test]
fn pseudo_classes_and_elements_are_not_symbols() {
    // The grammar reuses `class_name` for `:hover` and `:root`; those are CSS
    // keywords, not renameable names.
    let src = ":root { --x: 1px; }\na:hover { color: red; }\np::before { content: \"\"; }\n";
    let f = facts(Language::Css, src);
    let selectors = names(&f, SymbolKind::Selector);
    assert!(selectors.is_empty(), "got {selectors:?}");
}

#[test]
fn id_selectors_define_element_ids_without_the_hash() {
    let src = "#main .inner { color: red; }\n";
    let f = facts(Language::Css, src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["main"]);

    let main = f.symbols.iter().find(|s| s.name == "main").unwrap();
    assert_eq!(main.name_span.text(src), "main");
    assert_eq!(main.full_span.text(src), "#main");
    // The class in the same compound selector is still its own definition.
    assert_eq!(names(&f, SymbolKind::Selector), ["inner"]);
}

#[test]
fn custom_property_definition_and_var_usage_pair_up() {
    // The headline CSS rename story: `--brand-color` is defined once and used
    // through `var()`, and both sites carry the identical name.
    let src = ":root { --brand-color: red; }\n.btn { color: var(--brand-color); }\n";
    let f = facts(Language::Css, src);

    let def = f
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Property)
        .expect("custom property definition");
    assert_eq!(def.name, "--brand-color");
    assert_eq!(def.name_span.text(src), "--brand-color");
    assert_eq!(def.full_span.text(src), "--brand-color: red;");

    let uses: Vec<_> = f
        .references
        .iter()
        .filter(|r| r.name == "--brand-color")
        .collect();
    assert_eq!(uses.len(), 1, "got {uses:?}");
    assert_eq!(uses[0].kind, ReferenceKind::Identifier);
    assert_eq!(uses[0].span.text(src), "--brand-color");
    // The use site is the one inside `var(...)`, not the declaration.
    assert!(uses[0].span.start > src.find("var(").unwrap());
    assert_eq!(def.name, uses[0].name);
}

#[test]
fn ordinary_declarations_are_not_custom_properties() {
    let src = ".btn { color: red; margin-top: 0; }\n";
    let f = facts(Language::Css, src);
    let props = names(&f, SymbolKind::Property);
    assert!(props.is_empty(), "got {props:?}");
}

#[test]
fn var_fallback_value_is_not_a_reference() {
    let src = ".btn { color: var(--brand, blue); }\n";
    let f = facts(Language::Css, src);
    let refs: Vec<_> = f.references.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(refs, ["--brand"]);
}

#[test]
fn nested_var_fallback_is_also_a_reference() {
    let src = ".btn { color: var(--a, var(--b)); }\n";
    let f = facts(Language::Css, src);
    let mut refs: Vec<_> = f.references.iter().map(|r| r.name.as_str()).collect();
    refs.sort();
    assert_eq!(refs, ["--a", "--b"]);
}

#[test]
fn keyframes_name_defines_and_animation_name_references_it() {
    let src = "@keyframes slide { from { top: 0; } }\n.a { animation-name: slide; }\n";
    let f = facts(Language::Css, src);

    let def = f
        .symbols
        .iter()
        .find(|s| s.name == "slide")
        .expect("keyframes definition");
    assert_eq!(def.kind, SymbolKind::Selector);
    assert_eq!(def.name_span.text(src), "slide");
    assert!(def.full_span.text(src).starts_with("@keyframes slide"));

    let use_site = f
        .references
        .iter()
        .find(|r| r.name == "slide")
        .expect("animation-name reference");
    assert_eq!(use_site.kind, ReferenceKind::Identifier);
    assert!(use_site.span.start > src.find("animation-name").unwrap());
}

#[test]
fn imports_capture_the_path_without_quotes() {
    let src = "@import \"other.css\";\n@import url(\"theme.css\");\n@import url(plain.css);\n";
    let f = facts(Language::Css, src);
    let paths: Vec<_> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["other.css", "theme.css", "plain.css"]);
    assert!(f.imports.iter().all(|i| !i.is_glob));
}

#[test]
fn namespace_prefix_is_defined_and_referenced() {
    let src = "@namespace svg url(http://www.w3.org/2000/svg);\nsvg|circle { fill: red; }\n";
    let f = facts(Language::Css, src);
    assert_eq!(names(&f, SymbolKind::Module), ["svg"]);
    let use_site = f
        .references
        .iter()
        .find(|r| r.name == "svg")
        .expect("namespace selector reference");
    assert!(use_site.span.start > src.find("svg|circle").unwrap() - 1);
}

#[test]
fn blocks_nest_as_scopes() {
    let src = "@media (min-width: 100px) { .m { color: red; } }\n";
    let f = facts(Language::Css, src);
    let inner = f.scope_at(src.find("color").unwrap()).unwrap();
    let outer = f.scope_at(src.find("@media").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
}

#[test]
fn a_realistic_stylesheet_parses_cleanly() {
    let src = concat!(
        "@import \"reset.css\";\n",
        ":root { --gap: 4px; }\n",
        "@media screen and (min-width: 40em) {\n",
        "  #main > .card:not(.hidden) { gap: var(--gap); }\n",
        "}\n",
        "@supports (display: grid) { .g { display: grid; } }\n",
    );
    let parsed = Parsers::new().parse(Language::Css, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );
    let f = facts(Language::Css, src);
    assert_eq!(names(&f, SymbolKind::ElementId), ["main"]);
    assert_eq!(names(&f, SymbolKind::Property), ["--gap"]);
    let mut selectors = names(&f, SymbolKind::Selector);
    selectors.sort();
    assert_eq!(selectors, ["card", "g", "hidden"]);
}

// ------------------------------------------------------------------ SCSS SCSS has its own
// grammar. These tests hold the line in both directions: the Sass-only constructs parse. The
// same source is still an error under CSS, the dialects are different languages, not one
// language with a lenient mode.

#[test]
fn scss_variables_parse_on_the_scss_grammar() {
    // SCSS has its own grammar now, so `$variables` are syntax and not errors.
    let src = "$brand: #3366ff;\n.a { color: $brand; }\n";
    let parsed = Parsers::new().parse(Language::Scss, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "SCSS variables should parse: {:?}",
        parsed.error_spans()
    );
}

#[test]
fn scss_mixins_parse_on_the_scss_grammar() {
    let src = "@mixin theme($c) { color: $c; }\n.a { @include theme(red); }\n";
    let parsed = Parsers::new().parse(Language::Scss, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "@mixin/@include should parse: {:?}",
        parsed.error_spans()
    );
}

#[test]
fn scss_use_rule_parses_on_the_scss_grammar() {
    let src = "@use 'sass:math';\n";
    let parsed = Parsers::new().parse(Language::Scss, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "@use should parse: {:?}",
        parsed.error_spans()
    );
}

#[test]
fn the_css_grammar_still_rejects_scss_syntax() {
    // The dialects are genuinely different languages, so they get
    // different grammars: the same source parsed as CSS is still an error.
    let src = "$brand: #3366ff;\n";
    let as_css = Parsers::new().parse(Language::Css, src).unwrap();
    assert!(as_css.has_errors(), "`$brand` is not CSS");
}

#[test]
fn plain_css_inside_an_scss_file_still_yields_facts() {
    // The dialect flag does not change the queries; a CSS-compatible SCSS file
    // extracts as CSS does.
    let src = ".btn { color: var(--brand-color); }\n";
    let scss = facts(Language::Scss, src);
    let css = facts(Language::Css, src);
    assert_eq!(names(&scss, SymbolKind::Selector), ["btn"]);
    assert_eq!(scss.symbols.len(), css.symbols.len());
    assert_eq!(scss.references.len(), css.references.len());
    assert_eq!(scss.references[0].name, "--brand-color");
}

#[test]
fn nested_rule_sets_do_parse_and_yield_both_selectors() {
    // Nesting is native CSS in this grammar, so it works in both dialects.
    let src = ".outer { color: red; .inner { color: blue; } }\n";
    let parsed = Parsers::new().parse(Language::Scss, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "unexpected parse errors: {:?}",
        parsed.error_spans()
    );
    let f = facts(Language::Scss, src);
    assert_eq!(names(&f, SymbolKind::Selector), ["outer", "inner"]);
    // The `&`-prefixed form is SCSS-only and does not survive.
    let amp = ".outer { &.inner { color: blue; } }\n";
    assert_eq!(
        names(&facts(Language::Scss, amp), SymbolKind::Selector),
        ["outer", "inner"]
    );
}
