//! What a name means in Rust depends on what is written before it.
//!
//! Two ways the index claimed `Exact` where the syntax cannot support it. A bare call
//! resolved to a method — Rust has no implicit self, so `width(…)` inside an `impl`
//! cannot mean `self.width(…)`. And a dotted name inside a macro resolved to a free
//! function, because a macro body is tokens: `assert_eq!(f.scope_at(30), …)` reaches the
//! query as a bare identifier with no receiver at all.
//!
//! Both fed `fr rename` and `fr signature`, which rewrite on `Exact`.

use fun_refactor::index::Index;
use fun_refactor::model::{Confidence, SymbolKind};
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

fn indexed(source: &str) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

/// What the reference named `name` inside `needle` resolved to.
fn target_of(
    index: &Index,
    needle: &str,
    name: &str,
    source: &str,
) -> Option<(SymbolKind, Option<String>)> {
    let at = source.find(needle).expect("the snippet is in the source");
    let end = at + needle.len();
    index
        .references
        .iter()
        .filter(|r| r.span.start >= at && r.span.end <= end && r.name == name)
        .find_map(|r| r.target.and_then(|t| index.symbol(t)))
        .map(|s| (s.kind, s.qualifier.clone()))
}

const BOTH: &str = "pub struct Holder { pub items: Vec<u8> }\n\
\n\
pub fn width(items: &[u8], n: usize) -> usize { items.len() + n }\n\
\n\
impl Holder {\n\
    pub fn width(&self, n: usize) -> usize {\n\
        width(&self.items, n)\n\
    }\n\
}\n";

#[test]
fn a_bare_call_cannot_mean_a_method() {
    // There is no implicit self: `width(…)` here is the free function, whatever sits
    // in the `impl` around it.
    let (_tmp, index) = indexed(BOTH);
    assert_eq!(
        target_of(&index, "width(&self.items, n)", "width", BOTH),
        Some((SymbolKind::Function, None))
    );
}

#[test]
fn a_bare_call_cannot_mean_a_field_either() {
    // A field holding a closure is called as `(self.f)()`; a bare `f()` is not it.
    let source = "pub struct Holder { pub run: fn() -> u8 }\n\
                  pub fn run() -> u8 { 1 }\n\
                  impl Holder {\n    pub fn go(&self) -> u8 { run() }\n}\n";
    let (_tmp, index) = indexed(source);
    assert_eq!(
        target_of(&index, "run() }", "run", source),
        Some((SymbolKind::Function, None))
    );
}

#[test]
fn a_path_receiver_still_reaches_an_associated_function() {
    // The reason Rust was excluded from the rule. `Foo::new()` and `Self::new()` record
    // a receiver with `receiver_is_path`, so neither is a bare call.
    let source = "pub struct Foo;\n\
                  impl Foo {\n    pub fn new() -> Foo { Foo }\n\
                  \n    pub fn twice() -> Foo { Self::new() }\n}\n\
                  pub fn make() -> Foo { Foo::new() }\n";
    let (_tmp, index) = indexed(source);
    for call in ["Self::new()", "Foo::new()"] {
        assert_eq!(
            target_of(&index, call, "new", source).map(|(k, _)| k),
            Some(SymbolKind::Method),
            "{call}"
        );
    }
}

#[test]
fn a_dotted_name_in_a_macro_is_a_member_access() {
    let source = format!(
        "{BOTH}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    \
         fn t() {{ let h = Holder {{ items: vec![] }}; assert_eq!(h.width(2), 2); }}\n}}\n"
    );
    let (_tmp, index) = indexed(&source);
    assert_eq!(
        target_of(&index, "h.width(2)", "width", &source).map(|(k, _)| k),
        Some(SymbolKind::Method),
        "it is a call on `h`, whatever the grammar saw inside the macro"
    );
}

#[test]
fn a_plain_call_in_a_macro_is_still_rewritable() {
    // The blunt version of this fix distrusted every token in every macro, which left
    // 12,989 references in this repository unrewritable to fix four.
    let source = "pub fn helper() -> u8 { 1 }\n\
                  #[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    \
                  fn t() { assert_eq!(helper(), 1); }\n}\n";
    let (_tmp, index) = indexed(source);
    let at = source.find("helper(), 1").expect("in source");
    let reference = index
        .references
        .iter()
        .find(|r| r.span.start == at)
        .expect("the call inside the macro is a reference");
    assert_eq!(reference.confidence, Confidence::Exact);
    assert!(reference.confidence.is_safe_to_rewrite());
}

#[test]
fn renaming_the_free_function_leaves_the_method_calls_alone() {
    let source = format!(
        "{BOTH}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    \
         fn t() {{ let h = Holder {{ items: vec![] }}; assert_eq!(h.width(2), 2); }}\n}}\n"
    );
    let (_tmp, index) = indexed(&source);
    let free = index
        .symbols
        .iter()
        .find(|s| s.name == "width" && s.kind == SymbolKind::Function)
        .expect("the free function");
    let plan = rename::plan(&index, free.id, "width_of").expect("a plan");
    let rewritten: Vec<&str> = plan
        .edits
        .iter()
        .flat_map(|(_, edits)| edits.iter().map(|e| e.span.text(&source)))
        .collect();
    assert_eq!(
        rewritten.len(),
        2,
        "the definition and the one real call: {rewritten:?}"
    );
}
