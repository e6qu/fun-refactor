//! Fact extraction for the indented Sass syntax.
//!
//! `.sass` and `.scss` hold the same language written two ways, and a workspace may hold
//! both. So a name declared in one has to come out of the index looking like the same name
//! declared in the other: same kind, same spelling, same span conventions. Every test here
//! asserts one of those, and several assert it of both dialects at once.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "the fixture does not parse: {src}\n{}",
        parsed.tree.root_node().to_sexp()
    );
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

fn references(f: &FileFacts) -> Vec<&str> {
    f.references.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn a_class_is_named_without_its_dot_in_both_syntaxes() {
    let indented = facts(Language::Sass, ".btn\n  color: red\n");
    assert_eq!(names(&indented, SymbolKind::Selector), ["btn"]);

    let braced = facts(Language::Scss, ".btn { color: red; }\n");
    assert_eq!(names(&braced, SymbolKind::Selector), ["btn"]);

    let btn = indented.symbols.iter().find(|s| s.name == "btn").unwrap();
    assert_eq!(btn.name_span.text(".btn\n  color: red\n"), "btn");
    assert_eq!(btn.full_span.text(".btn\n  color: red\n"), ".btn");
}

#[test]
fn an_id_is_an_element_id_and_a_placeholder_is_a_selector() {
    let f = facts(
        Language::Sass,
        "#main\n  color: red\n\n%card\n  color: blue\n",
    );
    assert_eq!(names(&f, SymbolKind::ElementId), ["main"]);
    assert_eq!(names(&f, SymbolKind::Selector), ["card"]);
}

#[test]
fn a_variable_keeps_its_dollar_at_both_ends() {
    let src = "$brand: #fff\n\n.btn\n  color: $brand\n";
    let f = facts(Language::Sass, src);
    assert_eq!(names(&f, SymbolKind::Property), ["$brand"]);
    assert!(references(&f).contains(&"$brand"), "{:?}", references(&f));

    // The same file in the braced syntax, spelled the same way, so a rename crossing the
    // two rewrites both.
    let braced = facts(Language::Scss, "$brand: #fff;\n\n.btn { color: $brand; }\n");
    assert_eq!(names(&braced, SymbolKind::Property), ["$brand"]);
}

#[test]
fn a_custom_property_keeps_its_dashes() {
    let f = facts(Language::Sass, ".a\n  --gap: 4px\n  margin: var(--gap)\n");
    assert_eq!(names(&f, SymbolKind::Property), ["--gap"]);
    assert!(references(&f).contains(&"--gap"), "{:?}", references(&f));
}

#[test]
fn a_mixin_and_a_function_are_callables_with_parameters() {
    let src = "@mixin card($fg, $bg)\n  color: $fg\n\n@function double($n)\n  @return $n * 2\n";
    let f = facts(Language::Sass, src);
    assert_eq!(names(&f, SymbolKind::Function), ["card", "double"]);
    assert_eq!(names(&f, SymbolKind::Parameter), ["$fg", "$bg", "$n"]);
}

#[test]
fn an_include_is_a_call_and_a_namespace_is_a_module() {
    let src = "@use \"theme\" as t\n\n.a\n  @include card(red)\n  @include t.card(blue)\n";
    let f = facts(Language::Sass, src);
    assert_eq!(names(&f, SymbolKind::Module), ["t"]);

    let calls: Vec<&str> = f
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(calls, ["card", "card"], "both includes are call sites");
}

#[test]
fn an_extend_is_another_occurrence_of_the_selector_it_names() {
    // In CSS every occurrence of a selector declares it, and a rename rewrites all of
    // them. `@extend %card` is one of those occurrences, so it is a definition site and
    // not a reference. The same line in a `.scss` file reads the same way.
    let src = "%card\n  color: red\n\n.a\n  @extend %card\n\n.b\n  @extend .a\n";
    let f = facts(Language::Sass, src);
    let selectors = names(&f, SymbolKind::Selector);
    assert_eq!(
        selectors.iter().filter(|s| **s == "card").count(),
        2,
        "the placeholder and the `@extend` of it: {selectors:?}"
    );
    assert_eq!(
        selectors.iter().filter(|s| **s == "a").count(),
        2,
        "the rule and the `@extend` of it: {selectors:?}"
    );
}

#[test]
fn an_import_carries_the_path_without_its_quotes() {
    let f = facts(
        Language::Sass,
        "@use \"sass:math\"\n@forward \"buttons\"\n@import \"other\"\n",
    );
    let paths: Vec<&str> = f.imports.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["sass:math", "buttons", "other"]);
}

#[test]
fn a_keyframes_name_is_defined_and_animation_name_refers_to_it() {
    let src = "@keyframes slide\n  0%\n    opacity: 0\n\n.a\n  animation-name: slide\n";
    let f = facts(Language::Sass, src);
    assert!(names(&f, SymbolKind::Selector).contains(&"slide"));
    assert!(references(&f).contains(&"slide"), "{:?}", references(&f));
}

#[test]
fn a_nested_rule_declares_the_selector_it_writes() {
    // Each level of nesting is a selector of its own.
    let f = facts(
        Language::Sass,
        ".card\n  color: red\n  .title\n    color: blue\n  &:hover\n    color: green\n",
    );
    let selectors = names(&f, SymbolKind::Selector);
    assert!(selectors.contains(&"card"), "{selectors:?}");
    assert!(selectors.contains(&"title"), "{selectors:?}");
}

#[test]
fn a_declaration_inside_a_rule_is_scoped_to_it() {
    // Two rules may each declare `$x`, and the two are different names.
    let f = facts(
        Language::Sass,
        ".a\n  $x: 1\n  width: $x\n\n.b\n  $x: 2\n  width: $x\n",
    );
    let scopes: Vec<_> = f
        .symbols
        .iter()
        .filter(|s| s.name == "$x")
        .map(|s| s.scope)
        .collect();
    assert_eq!(scopes.len(), 2);
    assert_ne!(scopes[0], scopes[1], "each rule holds its own `$x`");
}
