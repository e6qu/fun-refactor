//! Which same-named variable counts as a rebinding, and what a multi-line one deletes.
//!
//! `fr inline` refused whenever another symbol of the same name appeared later in the
//! same file, whatever scope it was in. Two functions that each declare `let s` are not
//! a rebinding, and reusing a local name across functions is the common case: 6,166 of
//! this repository's 9,147 locals share a name with another local in the same file, and
//! the refusal covered 4,940 of them.
//!
//! The check is per-scope now. A rebinding in the same scope still refuses, in every
//! language, because there the value really does differ per use.

use fun_refactor::index::Index;
use fun_refactor::refactor::inline;
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

/// The definition of `name` that starts nearest after `after` bytes.
fn nth_named(index: &Index, name: &str, nth: usize) -> fun_refactor::model::SymbolId {
    let mut found = index.find_symbols(name, None);
    found.sort_by_key(|s| s.full_span.start);
    found[nth].id
}

#[test]
fn the_same_name_in_two_functions_is_two_variables() {
    let (_tmp, index) = workspace(&[(
        "src/lib.rs",
        "pub fn first(w: i32) -> i32 {\n    let s = w + 1;\n    s * 2\n}\n\n\
         pub fn second(w: i32) -> i32 {\n    let s = w - 1;\n    s * 3\n}\n",
    )]);
    let plan = inline::variable(&index, nth_named(&index, "s", 0)).expect("inlinable");
    assert_eq!(plan.edits.edit_count(), 2, "the use and the declaration");
}

#[test]
fn a_rebinding_in_the_same_scope_still_refuses() {
    let (_tmp, index) = workspace(&[(
        "src/lib.rs",
        "pub fn f(w: i32) -> i32 {\n    let s = w + 1;\n    let a = s;\n    let s = w + 2;\n    let b = s;\n    a + b\n}\n",
    )]);
    let err = inline::variable(&index, nth_named(&index, "s", 0))
        .expect_err("shadowed in the same block")
        .to_string();
    assert!(err.contains("assigned again"), "{err}");
}

#[test]
fn a_reassignment_still_refuses_where_the_language_has_no_second_binding() {
    // Python rebinds by assigning, so both uses read one symbol whose value changed.
    let (_tmp, index) = workspace(&[(
        "app.py",
        "def f(w):\n    s = w + 1\n    a = s\n    s = w + 2\n    b = s\n    return a + b\n",
    )]);
    let err = inline::variable(&index, nth_named(&index, "s", 0))
        .expect_err("reassigned")
        .to_string();
    assert!(err.contains("assigned again"), "{err}");
}

#[test]
fn a_name_shadowed_only_inside_a_nested_block_is_still_inlinable() {
    let (_tmp, index) = workspace(&[(
        "src/lib.rs",
        "pub fn f(w: i32) -> i32 {\n    let s = w + 1;\n    let inner = { let s = 9; s };\n    s + inner\n}\n",
    )]);
    inline::variable(&index, nth_named(&index, "s", 0)).expect("the outer s is inlinable");
}

#[test]
fn a_declaration_spanning_several_lines_is_removed_whole() {
    // The removal span read the line before the construct and the line after it from
    // the same offset, so a construct ending on a later line asked for `source[end..start]`
    // and panicked. `web/sample/infra/main.tf` in this repository is such a file.
    let (tmp, index) = workspace(&[(
        "main.tf",
        "locals {\n  name = \"x\"\n  tags = {\n    Service = local.name\n  }\n}\n\n\
         resource \"aws_vpc\" \"v\" {\n  tags = local.tags\n}\n",
    )]);
    let target = index
        .find_symbols("tags", None)
        .into_iter()
        .min_by_key(|s| s.full_span.start)
        .expect("the local");
    let plan = inline::variable(&index, target.id).expect("no panic, and a plan");

    let source = std::fs::read_to_string(tmp.path().join("main.tf")).unwrap();
    let after = fun_refactor::edit::apply_to_string(
        &source,
        plan.edits.edits_for(&tmp.path().join("main.tf")).unwrap(),
    )
    .expect("the edits apply");
    assert!(!after.contains("local.tags"), "the use is gone:\n{after}");
    assert!(
        after.contains("locals {\n  name = \"x\"\n}"),
        "the declaration left no blank line behind:\n{after}"
    );
    assert!(after.contains("Service = local.name"), "{after}");
}
