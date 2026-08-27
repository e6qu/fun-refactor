//! Moving a Rust item when the things that use it are not all in the same crate.

use fun_refactor::index::Index;
use fun_refactor::refactor::move_symbol;
use fun_refactor::scan::{scan, ScanOptions};

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

const MANIFEST: &str =
    "[package]\nname = \"sample-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LIB: &str = "pub mod helper;\npub mod other;\n";
const HELPER: &str = "pub fn moved_thing() -> u8 {\n    7\n}\n";
const OTHER: &str = "pub fn stays() -> u8 {\n    1\n}\n";

fn added_imports(plan: &move_symbol::MovePlan, file: &std::path::Path) -> Vec<String> {
    plan.edits
        .edits_for(file)
        .unwrap_or(&[])
        .iter()
        .map(|e| e.replacement.trim().to_string())
        .filter(|r| r.starts_with("use "))
        .collect()
}

#[test]
fn a_consumer_outside_the_crate_imports_it_by_package_name() {
    let (tmp, index) = workspace(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("src/helper.rs", HELPER),
        ("src/other.rs", OTHER),
        (
            "tests/it.rs",
            "use sample_crate::helper::moved_thing;\n\n#[test]\nfn t() { assert_eq!(moved_thing(), 7); }\n",
        ),
    ]);
    let id = index.find_symbols("moved_thing", None)[0].id;
    let plan = move_symbol::to_file(&index, id, &tmp.path().join("src/other.rs")).expect("a plan");

    assert_eq!(
        added_imports(&plan, &tmp.path().join("tests/it.rs")),
        ["use sample_crate::other::moved_thing;"],
        "an integration test reaches the library by its package name"
    );
}

#[test]
fn a_consumer_inside_the_crate_still_says_crate() {
    let (tmp, index) = workspace(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("src/helper.rs", HELPER),
        ("src/other.rs", OTHER),
        (
            "src/user.rs",
            "use crate::helper::moved_thing;\n\npub fn use_it() -> u8 { moved_thing() }\n",
        ),
    ]);
    let id = index.find_symbols("moved_thing", None)[0].id;
    let plan = move_symbol::to_file(&index, id, &tmp.path().join("src/other.rs")).expect("a plan");

    assert_eq!(
        added_imports(&plan, &tmp.path().join("src/user.rs")),
        ["use crate::other::moved_thing;"]
    );
}

#[test]
fn a_path_attribute_in_prose_does_not_refuse_the_move() {
    // The whole workspace refused because one doc comment reads `#[path::name]`.
    let (tmp, index) = workspace(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        (
            "src/helper.rs",
            "/// Is it annotated: `#[name]`, `#[path::name]` or `@name`?\npub fn moved_thing() -> u8 {\n    7\n}\n",
        ),
        ("src/other.rs", OTHER),
    ]);
    let id = index.find_symbols("moved_thing", None)[0].id;
    move_symbol::to_file(&index, id, &tmp.path().join("src/other.rs"))
        .expect("prose is not an attribute");
}

#[test]
fn a_real_path_attribute_still_refuses() {
    let (tmp, index) = workspace(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "pub mod helper;\n#[path = \"elsewhere/other.rs\"]\npub mod other;\n",
        ),
        ("src/helper.rs", HELPER),
        ("src/other.rs", OTHER),
    ]);
    let id = index.find_symbols("moved_thing", None)[0].id;
    let err = move_symbol::to_file(&index, id, &tmp.path().join("src/other.rs"))
        .expect_err("a module's file no longer follows from its path")
        .to_string();
    assert!(err.contains("#[path]"), "{err}");
}
