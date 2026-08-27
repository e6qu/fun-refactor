//! The Zig file-as-struct idiom crosses as a record.

use fun_refactor::lang::Language;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

const STORE: &str = "const Store = @This();\n\nio: i64,\nconfig: i64,\n\n\
                     pub fn size(self: *Store) i64 {\n    return self.io;\n}\n";

#[test]
fn a_named_this_binding_becomes_the_record() {
    let (_tmp, root) = workspace(&[("store.zig", STORE)]);
    let plan = fun_refactor::transpile::plan(&root.join("store.zig"), Language::TypeScript)
        .expect("a draft");
    assert_eq!(plan.fidelity.records, 1, "the file is one record");
    assert!(
        plan.output.contains("export class Store"),
        "got:\n{}",
        plan.output
    );
    assert!(plan.output.contains("io: number"), "got:\n{}", plan.output);
    assert!(
        !plan.output.contains(fun_refactor::transpile::MARKER),
        "everything in the file has a counterpart:\n{}",
        plan.output
    );
}

#[test]
fn the_receiver_method_joins_the_record() {
    let (_tmp, root) = workspace(&[("store.zig", STORE)]);
    let plan =
        fun_refactor::transpile::plan(&root.join("store.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("impl Store"),
        "the method belongs to the type:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("fn size(&self)"),
        "got:\n{}",
        plan.output
    );
}

#[test]
fn a_self_binding_takes_the_file_name() {
    // zls's own spelling: the binding is `Self`, and the name everyone importing the
    // file uses for the type is the file's.
    let source = "const Self = @This();\n\nlimit: i64,\n\n\
                  pub fn room(self: *Self) i64 {\n    return self.limit;\n}\n";
    let (_tmp, root) = workspace(&[("Budget.zig", source)]);
    let plan =
        fun_refactor::transpile::plan(&root.join("Budget.zig"), Language::Rust).expect("a draft");
    assert!(
        plan.output.contains("pub struct Budget"),
        "got:\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("impl Budget"),
        "the output must name the type, not `Self`:\n{}",
        plan.output
    );
    assert_eq!(
        plan.fidelity.signatures_with_foreign_types, 0,
        "`Self` would name a type the output never declares."
    );
}

#[test]
fn a_file_that_is_not_a_struct_is_untouched() {
    let source = "pub fn twice(n: i64) i64 {\n    return n * 2;\n}\n";
    let (_tmp, root) = workspace(&[("math.zig", source)]);
    let plan = fun_refactor::transpile::plan(&root.join("math.zig"), Language::TypeScript)
        .expect("a draft");
    assert_eq!(
        plan.fidelity.records, 0,
        "no fields and no binding means no record:\n{}",
        plan.output
    );
}
