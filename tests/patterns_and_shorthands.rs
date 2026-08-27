//! What a pattern reads and what a shorthand writes.

use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn symbol_at<'a>(
    index: &'a Index,
    name: &str,
    kind: fun_refactor::model::SymbolKind,
) -> &'a fun_refactor::model::Symbol {
    index
        .symbols
        .iter()
        .find(|s| s.name == name && s.kind == kind)
        .unwrap_or_else(|| panic!("no {name}"))
}

const IR: &str = "pub enum Shape {\n    Circle { radius: f64 },\n    Square(f64),\n}\n";
const WRITER: &str = "use crate::ir::Shape;\n\npub fn area(s: &Shape) -> f64 {\n    \
    match s {\n        Shape::Circle { radius } => 3.14 * radius * radius,\n        \
    Shape::Square(side) => side * side,\n    }\n}\n";
const LIB: &str = "pub mod ir;\npub mod writer;\n";

#[test]
fn a_variant_matched_in_another_file_is_a_use() {
    let (_tmp, index) = workspace(&[
        ("src/ir.rs", IR),
        ("src/writer.rs", WRITER),
        ("src/lib.rs", LIB),
    ]);
    let square = symbol_at(&index, "Square", fun_refactor::model::SymbolKind::Constant);
    assert_eq!(
        square.qualifier.as_deref(),
        Some("Shape"),
        "the enum qualifies its variants the way Java's does its constants"
    );
    let uses = index
        .references_to(square.id)
        .iter()
        .filter(|r| r.file.ends_with("writer.rs"))
        .count();
    assert_eq!(uses, 1, "the match arm reads the variant");
}

#[test]
fn a_variant_rename_reaches_the_match_arms() {
    let (_tmp, index) = workspace(&[
        ("src/ir.rs", IR),
        ("src/writer.rs", WRITER),
        ("src/lib.rs", LIB),
    ]);
    let square = symbol_at(&index, "Square", fun_refactor::model::SymbolKind::Constant);
    let plan = rename::plan(&index, square.id, "Box4").expect("a plan");
    let writer_edits = plan
        .edits
        .iter()
        .filter(|(file, _)| file.ends_with("writer.rs"))
        .map(|(_, edits)| edits.len())
        .sum::<usize>();
    assert_eq!(writer_edits, 1, "the arm in the other file renames too");
}

#[test]
fn a_field_read_only_by_destructuring_is_alive() {
    let (_tmp, index) = workspace(&[(
        "lib.rs",
        "pub enum Stmt {\n    ForEach { iterable: i64 },\n}\n\n\
         pub fn read(s: &Stmt) -> i64 {\n    match s {\n        \
         Stmt::ForEach { iterable } => *iterable,\n    }\n}\n",
    )]);
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    let unused = fun_refactor::refactor::delete::find_unused(&index, &entrypoints);
    let dead: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !dead.contains(&"iterable"),
        "destructuring reads the field: {dead:?}"
    );
}

#[test]
fn a_field_written_only_by_a_struct_literal_is_alive() {
    let (_tmp, index) = workspace(&[(
        "lib.rs",
        "pub struct Facts {\n    pub named: i64,\n    pub shorthand: i64,\n}\n\n\
         pub fn build(shorthand: i64) -> Facts {\n    \
         Facts { named: 1, shorthand }\n}\n",
    )]);
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    let unused = fun_refactor::refactor::delete::find_unused(&index, &entrypoints);
    let dead: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(!dead.contains(&"named"), "{dead:?}");
    assert!(!dead.contains(&"shorthand"), "{dead:?}");
}

const SHORTHAND: &str = "pub struct Facts {\n    pub count: usize,\n}\n\n\
    pub fn build() -> Facts {\n    let count = 3;\n    Facts { count }\n}\n\n\
    pub fn read(f: &Facts) -> usize {\n    f.count\n}\n";

#[test]
fn renaming_the_local_expands_the_shorthand() {
    let (tmp, index) = workspace(&[("lib.rs", SHORTHAND)]);
    let local = index
        .symbols
        .iter()
        .find(|s| s.name == "count" && s.kind == fun_refactor::model::SymbolKind::Variable)
        .expect("the local");
    let plan = rename::plan(&index, local.id, "total").expect("a plan");
    let source = std::fs::read_to_string(tmp.path().join("lib.rs")).unwrap();
    let out = fun_refactor::edit::apply_to_string(
        &source,
        plan.edits.edits_for(&tmp.path().join("lib.rs")).unwrap(),
    )
    .unwrap();
    assert!(
        out.contains("Facts { count: total }"),
        "the field keeps its name when the local goes.\n{out}"
    );
}

#[test]
fn renaming_the_field_expands_the_shorthand_the_other_way() {
    let (tmp, index) = workspace(&[("lib.rs", SHORTHAND)]);
    let field = symbol_at(&index, "count", fun_refactor::model::SymbolKind::Field);
    let plan = rename::plan(&index, field.id, "size").expect("a plan");
    let source = std::fs::read_to_string(tmp.path().join("lib.rs")).unwrap();
    let out = fun_refactor::edit::apply_to_string(
        &source,
        plan.edits.edits_for(&tmp.path().join("lib.rs")).unwrap(),
    )
    .unwrap();
    assert!(
        out.contains("Facts { size: count }"),
        "the local keeps its name when the field goes.\n{out}"
    );
    assert!(
        out.contains("f.size"),
        "a receiver declared `&Facts` reaches the field, sigil and all.\n{out}"
    );
}

#[test]
fn a_local_does_not_answer_calls_outside_its_scope() {
    // The parameter sits nearer to the call than the function of the same name.
    let filler = "    // filler\n".repeat(40);
    let source = format!(
        "mod reader {{\n    fn settle(stmt: &mut i64) {{\n        *stmt += 1;\n    }}\n\n\
             fn caller(node: i64) -> i64 {{\n        stmt(node)\n    }}\n\n{filler}\n\
             fn stmt(n: i64) -> i64 {{\n        n * 2\n    }}\n}}\n"
    );
    let (_tmp, index) = workspace(&[("lib.rs", &source)]);
    let function = symbol_at(&index, "stmt", fun_refactor::model::SymbolKind::Function);
    assert_eq!(
        index.references_to(function.id).len(),
        1,
        "the call belongs to the function"
    );
    let parameter = symbol_at(&index, "stmt", fun_refactor::model::SymbolKind::Parameter);
    assert_eq!(
        index.references_to(parameter.id).len(),
        1,
        "the parameter keeps only its in-scope read"
    );
}
