//! A refusal that names a language must be about that language.
//!
//! `Refusal::Unsupported` says "{operation} is not supported for {language}". Ten sites once
//! wrote a reason naming no language into the field called `language`; they were found. The fix
//! was to add a `because` field so the language field could be the language. That made the
//! mistake easier to avoid and did not stop it. Five more sites were doing it afterwards:
//!
//! * `fr move` told a Rust user that Rust was unsupported when the destination was outside
//!   `src/`, found by the capability audit, because a `✓` in the matrix and a refusal naming
//!   the language cannot both be right.
//! * Four more interpolated a path into the reason: two crate roots, a missing `lib.rs`, a
//!   Terraform directory, a Zig relative path.
//!
//! The field is `&'static str` now, so a reason with a path in it does not compile. That makes
//! this hold and not what documents it. These tests check the property the type cannot:
//! that the pairs the code refuses agree with the matrix.

use fun_refactor::capabilities::{support, Capability};
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, Index) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let scanned = scan(dir.path(), &ScanOptions::default()).expect("scan");
    let index = Index::build_from_scan(&scanned).expect("index");
    let root = dir.path().to_path_buf();
    (dir, root, index)
}

fn symbol(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

/// Does this message claim the language cannot do it?
fn blames_the_language(said: &str, language: Language) -> bool {
    said.contains(&format!("is not supported for {}", language.name()))
}

#[test]
fn a_rust_move_to_the_wrong_place_does_not_blame_rust() {
    // The one the capability audit found. `fr move` supports Rust; what it cannot do is
    // derive a module path for a file outside `src/`.
    let (_tmp, root, index) = workspace(&[
        (
            "Cargo.toml",
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub fn width() -> usize {\n    1\n}\n"),
    ]);
    let said = fun_refactor::refactor::move_symbol::to_file(
        &index,
        symbol(&index, "width"),
        &root.join("outside.rs"),
    )
    .expect_err("a destination outside src/ has no module path")
    .to_string();

    assert!(!blames_the_language(&said, Language::Rust), "{said}");
    assert!(
        said.contains("src/"),
        "and it says what is actually wrong: {said}"
    );
}

#[test]
fn a_move_between_crate_roots_does_not_blame_rust() {
    let (_tmp, root, index) = workspace(&[
        (
            "one/Cargo.toml",
            "[package]\nname = \"one\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("one/src/lib.rs", "pub fn width() -> usize {\n    1\n}\n"),
        (
            "two/Cargo.toml",
            "[package]\nname = \"two\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("two/src/lib.rs", "pub fn other() -> usize {\n    2\n}\n"),
    ]);
    let said = fun_refactor::refactor::move_symbol::to_file(
        &index,
        symbol(&index, "width"),
        &root.join("two/src/moved.rs"),
    )
    .expect_err("a move between crates needs a dependency edge")
    .to_string();

    assert!(!blames_the_language(&said, Language::Rust), "{said}");
    assert!(said.contains("crate roots"), "{said}");
}

#[test]
fn a_terraform_move_between_directories_does_not_blame_terraform() {
    let (_tmp, root, index) = workspace(&[
        ("a/main.tf", "locals {\n  base = \"service\"\n}\n"),
        ("b/other.tf", "locals {\n  spare = 1\n}\n"),
    ]);
    let said = fun_refactor::refactor::move_symbol::to_file(
        &index,
        symbol(&index, "base"),
        &root.join("b/other.tf"),
    )
    .expect_err("a Terraform module is the directory")
    .to_string();

    assert!(!blames_the_language(&said, Language::Hcl), "{said}");
    assert!(said.contains("module"), "{said}");
}

#[test]
fn a_language_that_really_cannot_still_says_so() {
    // The other half. Removing the free text would have been the easy fix and would have thrown
    // away the explanations that are true. `fr signature` on a Terraform variable is refused
    // because Terraform's arguments are named. That is a fact about Terraform and belongs in
    // the sentence.
    let (_tmp, _root, index) = workspace(&[(
        "main.tf",
        "variable \"enabled\" {\n  type = bool\n}\n\noutput \"e\" {\n  value = var.enabled\n}\n",
    )]);
    let said = fun_refactor::refactor::signature::change(
        &index,
        symbol(&index, "enabled"),
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .expect_err("a Terraform variable has no parameter order")
    .to_string();

    assert!(said.contains("hcl"), "it names the language: {said}");
    assert!(said.contains("named and not"), "and why: {said}");
}

#[test]
fn no_refusal_calls_a_language_unsupported_that_the_matrix_supports() {
    // The contradiction the audit found, asked of every claimed cell at once: a `✓` and a
    // refusal naming the language cannot both be true. `capability_claims.rs` drives every
    // cell; this states the rule the drivers there are checking.
    for language in Language::ALL {
        for capability in Capability::ALL {
            let supported = support(*capability, *language).is_yes();
            let reason = support(*capability, *language).reason();
            if supported {
                assert!(
                    reason.is_none(),
                    "{} claims {} and carries a reason it cannot: {reason:?}",
                    language.name(),
                    capability.label()
                );
            }
        }
    }
}
