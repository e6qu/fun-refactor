//! Rust fact-extraction tests: what `queries/rust/facts.scm` actually reports.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(Language::Rust, src).unwrap();
    assert!(
        !parsed.has_errors(),
        "sample must parse cleanly, errors at {:?}",
        parsed.error_spans()
    );
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

/// Every symbol as a reader sees it: `Type::method` where there is a container.
fn qualified(f: &FileFacts) -> Vec<String> {
    f.symbols.iter().map(|s| s.qualified_name()).collect()
}

/// A method of a generic or lifetime-parameterised type is a method of that type.
///
/// The container patterns matched `type: (type_identifier)`, and `impl Ctx<'_>` puts a
/// `generic_type` there, so the methods inside got no container at all: `run` rather
/// than `Ctx::run`, and kind `function` rather than `method`. A `self.hcl_backward(…)`
/// then had no member to resolve to, and 43 of `provenance.rs`'s own methods read as
/// dead code.
#[test]
fn an_impl_on_a_generic_type_still_names_its_methods() {
    let f = facts(
        "pub struct Plain;\n\
         impl Plain {\n    fn a(&self) {}\n}\n\n\
         pub struct Borrowed<'a> { t: &'a str }\n\
         impl Borrowed<'_> {\n    fn b(&self) {}\n}\n\n\
         pub struct Generic<T> { i: T }\n\
         impl<T> Generic<T> {\n    fn c(&self) {}\n}\n\n\
         pub trait Show { fn show(&self); }\n\
         impl<T> Show for Generic<T> {\n    fn show(&self) {}\n}\n",
    );
    let names = qualified(&f);
    for expected in [
        "Plain::a",
        "Borrowed::b",
        "Generic::c",
        "Show::show",
        "Generic::show",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "expected {expected} among {names:?}"
        );
    }
    assert!(
        f.symbols
            .iter()
            .filter(|s| ["a", "b", "c"].contains(&s.name.as_str()))
            .all(|s| s.kind == SymbolKind::Method),
        "all three are methods: {:?}",
        f.symbols
            .iter()
            .map(|s| (&s.name, s.kind))
            .collect::<Vec<_>>()
    );
}
