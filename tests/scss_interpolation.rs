//! SCSS `#{ ... }`, and the facts written inside it.
//!
//! `tree-sitter-scss` 1.0 has no rule for interpolation in a declaration value. Its error
//! node runs to the end of the file, so one interpolated value cost every fact below it.
//! `grammars/scss` reads the declaration, and the variables and calls between the braces are
//! ordinary nodes that the query matches where it matches them anywhere else.
//!
//! Measured over `twbs/bootstrap` and `jgthms/bulma`, 276 stylesheets: the published grammar
//! fails on 203 of them and this one on none.

use fun_refactor::extract::Extractor;
use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;
use std::path::Path;

fn parsed_and_facts(source: &str) -> (usize, Vec<String>, Vec<String>) {
    let parsers = Parsers::new();
    let parsed = parsers
        .parse(Language::Scss, source)
        .expect("the grammar loads");
    let facts = Extractor::new()
        .extract(&parsed, Path::new("t.scss"), source)
        .expect("extraction");
    (
        parsed.error_spans().len(),
        facts.symbols.into_iter().map(|s| s.name).collect(),
        facts.references.into_iter().map(|r| r.name).collect(),
    )
}

#[test]
fn an_interpolated_declaration_value_parses() {
    for source in [
        ".a { color: #{$v}; }",
        ".a { --x: #{$v}; }",
        ".a { margin: 0 #{$gutter} 0; }",
    ] {
        let (errors, _, _) = parsed_and_facts(source);
        assert_eq!(errors, 0, "{source}");
    }
}

#[test]
fn an_interpolated_value_no_longer_costs_the_rest_of_the_file() {
    // The reason this one form was worth masking. The error did not stay in the declaration, it
    // ran to the end of the file and took every definition with it.
    let source = ".a {\n  color: #{$v};\n}\n\n.b { color: red; }\n\n.c { color: blue; }\n";
    let (errors, symbols, _) = parsed_and_facts(source);
    assert_eq!(errors, 0);
    // A selector symbol is named without its leading `.`.
    for expected in ["a", "b", "c"] {
        assert!(
            symbols.iter().any(|s| s == expected),
            "{expected} missing from {symbols:?}"
        );
    }
}

#[test]
fn what_the_braces_hold_is_read_as_references() {
    let (errors, _, references) =
        parsed_and_facts(".a { color: #{escape-svg($fill)}; width: #{$w}; }");
    assert_eq!(errors, 0);
    for expected in ["escape-svg", "$fill", "$w"] {
        assert!(
            references.iter().any(|r| r == expected),
            "{expected} missing from {references:?}"
        );
    }
}

#[test]
fn a_bare_word_inside_the_braces_names_nothing() {
    // Units and keywords are not symbols, and reporting them would invent references
    // to things no file defines.
    let (_, _, references) = parsed_and_facts(".a { width: #{$w + 2px}; }");
    assert_eq!(references.iter().filter(|r| *r == "px").count(), 0);
    assert!(references.iter().any(|r| r == "$w"), "{references:?}");
}

#[test]
fn an_interpolated_name_is_the_text_the_file_holds() {
    // A selector built around an interpolation is named by what the file says, braces
    // and all: that is the text a rename has to rewrite.
    let (_, symbols, _) = parsed_and_facts(".btn-#{$variant} { color: red; }");
    assert!(
        symbols.iter().any(|s| s.contains("#{$variant}")),
        "{symbols:?}"
    );
}

#[test]
fn braces_inside_the_braces_are_one_interpolation() {
    let source = ".a { color: #{map-get($m, #{$k})}; }";
    let (errors, _, references) = parsed_and_facts(source);
    assert_eq!(errors, 0);
    for expected in ["map-get", "$m", "$k"] {
        assert!(
            references.iter().any(|r| r == expected),
            "{expected} missing from {references:?}"
        );
    }
}

#[test]
fn an_unterminated_interpolation_stays_a_syntax_error() {
    // A fault in the file, and it is reported as one.
    let (errors, _, _) = parsed_and_facts(".a { color: #{$v; }");
    assert!(errors > 0);
}

#[test]
fn plain_css_is_untouched() {
    let (errors, symbols, references) =
        parsed_and_facts(".a { color: red; width: calc(1px + 2px); }");
    assert_eq!(errors, 0);
    assert!(symbols.iter().any(|s| s == "a"), "{symbols:?}");
    assert!(references.iter().any(|r| r == "calc"), "{references:?}");
}
