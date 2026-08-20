//! What the write commands do when turned on real code.
//!
//! Every defect here surfaced by running a command over this repository and
//! compiling the result: a signature change refused at every `assert_eq!`, an
//! extraction missed a format-string capture, and a move left a written path
//! naming the module the symbol had left.

use fun_refactor::index::Index;
use fun_refactor::refactor::{extract, move_symbol, signature};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

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

fn applied(edits: &fun_refactor::edit::EditSet, path: &Path) -> String {
    let source = std::fs::read_to_string(path).unwrap();
    fun_refactor::edit::apply_to_string(&source, edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn a_qualified_path_in_a_macro_resolves_and_a_signature_reaches_it() {
    // `assert_eq!(myc::model::slug("A"), "a")` is tokens to the grammar. The
    // tokens spell a path and a call, and both halves used to be invisible:
    // the reference resolved name-only, and the signature change refused.
    let (tmp, index) = workspace(&[
        (
            "src/model.rs",
            "pub fn slug(s: &str) -> String {\n    s.to_lowercase()\n}\n",
        ),
        ("src/lib.rs", "pub mod model;\n"),
        (
            "tests/t.rs",
            "#[test]\nfn t() {\n    assert_eq!(myc::model::slug(\"A\"), \"a\");\n}\n",
        ),
    ]);
    let symbol = index
        .symbols
        .iter()
        .find(|s| s.name == "slug")
        .expect("the fn");
    let refs = index.references_to(symbol.id);
    assert_eq!(refs.len(), 1, "the macro call resolves");
    assert!(
        refs[0].confidence.is_safe_to_rewrite(),
        "a written path is evidence: {:?}",
        refs[0].confidence
    );

    let plan = signature::change(
        &index,
        symbol.id,
        signature::Change::parse("add:1:upper: bool:false").unwrap(),
    )
    .expect("a plan");
    let out = applied(&plan.edits, &tmp.path().join("tests/t.rs"));
    assert!(
        out.contains("myc::model::slug(\"A\", false)"),
        "the macro call takes the new argument.\n{out}"
    );
}

#[test]
fn an_extraction_passes_what_a_format_string_captures() {
    let src = "pub fn report(kinds: &[(&str, usize)]) {\n    \
        let total: usize = kinds.iter().map(|(_, n)| n).sum();\n    \
        let mut names: Vec<String> = Vec::new();\n    \
    for (kind, count) in kinds {\n        names.push(format!(\"{kind} ({count})\"));\n    }\n    \
    println!(\"{total} file(s): {}\", names.join(\", \"));\n}\n";
    let (tmp, index) = workspace(&[("lib.rs", src)]);
    let path = tmp.path().join("lib.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    let start = source.find("let mut names").unwrap();
    let end = source.rfind("names.join").unwrap();
    let end = source[end..].find(';').unwrap() + end + 1;
    let plan = extract::function(
        &index,
        &path,
        fun_refactor::span::Span::new(start, end),
        "print_kinds",
    )
    .expect("a plan");
    let out = applied(&plan.edits, &path);
    assert!(
        out.contains("total: usize"),
        "`{{total}}` inside the println is a read, and travels as a parameter.\n{out}"
    );
}

#[test]
fn a_move_repoints_a_written_path() {
    let (tmp, index) = workspace(&[
        (
            "src/a.rs",
            "pub(crate) fn widen(n: usize) -> usize {\n    n + 1\n}\n",
        ),
        ("src/b.rs", "pub fn empty() {}\n"),
        (
            "src/c.rs",
            "pub fn go() -> usize {\n    crate::a::widen(2)\n}\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod b;\npub mod c;\n"),
    ]);
    let symbol = index
        .symbols
        .iter()
        .find(|s| s.name == "widen")
        .expect("the fn");
    let plan =
        move_symbol::to_file(&index, symbol.id, &tmp.path().join("src/b.rs")).expect("a plan");
    let out = applied(&plan.edits, &tmp.path().join("src/c.rs"));
    assert!(
        out.contains("crate::b::widen(2)"),
        "the written path names the new module.\n{out}"
    );
    assert!(
        !out.contains("use crate::b::widen"),
        "a repointed path needs no use besides.\n{out}"
    );
}

#[test]
fn a_move_carries_no_use_the_destination_already_has() {
    let (tmp, index) = workspace(&[
        (
            "src/a.rs",
            "use crate::util::{measure, Edit};\n\n\
             pub(crate) fn widen(n: usize) -> usize {\n    measure(n) + Edit::SIZE\n}\n\n\
             pub fn keep(n: usize) -> usize {\n    measure(n)\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::util::{measure, Edit};\n\n\
             pub fn empty() -> usize {\n    measure(Edit::SIZE)\n}\n",
        ),
        (
            "src/util.rs",
            "pub fn measure(n: usize) -> usize {\n    n\n}\n\n\
             pub struct Edit;\n\nimpl Edit {\n    pub const SIZE: usize = 4;\n}\n",
        ),
        ("src/lib.rs", "pub mod a;\npub mod b;\npub mod util;\n"),
    ]);
    let symbol = index
        .symbols
        .iter()
        .find(|s| s.name == "widen")
        .expect("the fn");
    let plan =
        move_symbol::to_file(&index, symbol.id, &tmp.path().join("src/b.rs")).expect("a plan");
    let out = applied(&plan.edits, &tmp.path().join("src/b.rs"));
    assert_eq!(
        out.matches("measure").count(),
        3,
        "one existing import binds it; no second use arrives.\n{out}"
    );
    assert!(
        !out.contains("use crate::util::measure;"),
        "the destination already binds the name.\n{out}"
    );
}
