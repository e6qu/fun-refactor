//! What an attribute above an import belongs to.
//!
//! Sorting moves whole lines within a run of imports. An attribute sits on its own line and is
//! part of the item below it. So leaving it where it was hands it to whichever import sorts
//! into its place. Run over this repository, `fr imports` did that to three of its 49 Rust
//! files, `src/index.rs` among them:
//!
//! ```text #[cfg(feature = "cli")] +use anyhow::{Context, Result}; use crate::scan::{scan,
//! ScanOptions, ScanResult}; ```
//!
//! which compiles under neither setting of the feature, `anyhow` disappears without it, and
//! `crate::scan` is named unconditionally while the module it names is not.

use fun_refactor::index::Index;
use fun_refactor::refactor::imports;
use fun_refactor::scan::{scan, ScanOptions};

fn organized(source: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    let file = tmp.path().join("src/lib.rs");
    std::fs::write(&file, source).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    let plan = imports::plan(&index, &file).expect("a plan");
    match plan.edits.edits_for(&file) {
        Some(edits) => fun_refactor::edit::apply_to_string(source, edits).expect("edits apply"),
        None => source.to_string(),
    }
}

const TAIL: &str = "\npub mod a { pub struct A; }\npub mod b { pub struct B; }\n\
                    #[cfg(feature = \"cli\")]\npub mod scan { pub struct S; }\n\
                    pub fn f(_p: &Path) -> Result<(A, B)> { todo!() }\n";

#[test]
fn an_attribute_moves_with_the_import_it_guards() {
    let source = format!(
        "use crate::a::A;\nuse crate::b::B;\n#[cfg(feature = \"cli\")]\n\
         use crate::scan::S;\nuse anyhow::Result;\nuse std::path::Path;\n{TAIL}"
    );
    let after = organized(&source);
    assert!(
        after.contains("#[cfg(feature = \"cli\")]\nuse crate::scan::S;"),
        "the attribute lost its import:\n{after}"
    );
    // And the sort still happened.
    let anyhow_at = after.find("use anyhow::Result;").expect("anyhow kept");
    let a_at = after.find("use crate::a::A;").expect("a kept");
    assert!(anyhow_at < a_at, "the block was not sorted:\n{after}");
}

#[test]
fn a_multi_line_attribute_travels_whole() {
    let source = format!(
        "use crate::b::B;\n#[cfg(all(\n    feature = \"cli\",\n    unix,\n))]\n\
         use crate::scan::S;\nuse anyhow::Result;\nuse crate::a::A;\nuse std::path::Path;\n{TAIL}"
    );
    let after = organized(&source);
    assert!(
        after.contains("#[cfg(all(\n    feature = \"cli\",\n    unix,\n))]\nuse crate::scan::S;"),
        "the attribute was split from its import:\n{after}"
    );
}

#[test]
fn stacked_attributes_all_travel() {
    let source = format!(
        "use crate::b::B;\n#[cfg(feature = \"cli\")]\n#[allow(unused_imports)]\n\
         use crate::scan::S;\nuse anyhow::Result;\nuse crate::a::A;\nuse std::path::Path;\n{TAIL}"
    );
    let after = organized(&source);
    assert!(
        after.contains("#[cfg(feature = \"cli\")]\n#[allow(unused_imports)]\nuse crate::scan::S;"),
        "an attribute was left behind:\n{after}"
    );
}

#[test]
fn an_attribute_does_not_make_the_line_look_shared() {
    // The check for "something else is on this line" reads the statement's own line. Run
    // against the extended one, every attributed import looked like shared code and the
    // whole block stopped sorting.
    let source = format!(
        "#[cfg(feature = \"cli\")]\nuse crate::scan::S;\nuse crate::b::B;\nuse crate::a::A;\n\
         use anyhow::Result;\nuse std::path::Path;\n{TAIL}"
    );
    let after = organized(&source);
    assert!(
        after.find("use crate::a::A;") < after.find("use crate::b::B;"),
        "the block was not sorted:\n{after}"
    );
    assert!(
        after.contains("#[cfg(feature = \"cli\")]\nuse crate::scan::S;"),
        "{after}"
    );
}
