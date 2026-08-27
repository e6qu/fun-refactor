//! A field and a method under one name stay two symbols with two use lists.

use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};

const ORDER_RS: &str = "pub struct Order {\n    pub name: String,\n    pub cents: u64,\n}\n\n\
    impl Order {\n    pub fn name(&self) -> &str {\n        &self.name\n    }\n}\n\n\
    pub fn describe(order: &Order) -> String {\n    let label = order.name();\n    \
    let len = order.name.len();\n    format!(\"{label} {len} {}\", order.cents)\n}\n";

fn indexed(source: &str) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("lib.rs"), source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn uses_of(index: &Index, kind: fun_refactor::model::SymbolKind) -> Vec<String> {
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == "name" && s.kind == kind)
        .expect("the symbol")
        .id;
    index
        .references_to(id)
        .into_iter()
        .map(|r| {
            let source = std::fs::read_to_string(&r.file).unwrap();
            source[r.span.start..r.span.end.min(r.span.start + 40)].to_string()
        })
        .collect()
}

#[test]
fn the_field_collects_the_accesses_and_only_those() {
    let (_tmp, index) = indexed(ORDER_RS);
    let uses = uses_of(&index, fun_refactor::model::SymbolKind::Field);
    assert_eq!(
        uses.len(),
        2,
        "&self.name and order.name.len(), nothing else: {uses:?}"
    );
}

#[test]
fn the_method_collects_the_calls_and_only_those() {
    let (_tmp, index) = indexed(ORDER_RS);
    let uses = uses_of(&index, fun_refactor::model::SymbolKind::Method);
    assert_eq!(uses.len(), 1, "order.name() alone: {uses:?}");
}
