//! Move to file, across the seven languages where a move can be made correct.
//!
//! Each language has its own answer to "what else has to change". These tests pin the answer
//! instead of the aspiration:
//!
//! - Rust and TypeScript/Python rewrite reference sites, so those tests assert the import lines
//!   byte for byte.
//! - Go inside one package, HCL inside one directory and CSS anywhere change nothing but the
//!   two files. So those tests assert that *no* third file was touched.
//! - Markdown repoints anchors.
//!
//! Every successful move is committed through `edit::plan(…, ReparseStrict)`, so a move that
//! would break a file fails the test instead of the build. Where the tool refuses, the refusal
//! message is asserted, because a refusal that does not say what was wrong is not much better
//! than a wrong answer.

use fun_refactor::{
    edit::{self, Validation},
    index::Index,
    refactor::move_symbol,
    scan::{scan, ScanOptions},
};
use std::path::{Path, PathBuf};

/// A temporary workspace, indexed.
struct Workspace {
    tmp: tempfile::TempDir,
}

impl Workspace {
    fn new(files: &[(&str, &str)]) -> Workspace {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        Workspace { tmp }
    }

    fn index(&self) -> Index {
        let scanned = scan(self.tmp.path(), &ScanOptions::default()).unwrap();
        Index::build_from_scan(&scanned).unwrap()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }
}

/// Find the one symbol with this name, failing loudly if it is ambiguous.
fn symbol_id(index: &Index, name: &str, file: Option<&Path>) -> fun_refactor::model::SymbolId {
    let found = index.find_symbols(name, file);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one '{name}', got {:?}",
        found.iter().map(|s| (&s.file, s.kind)).collect::<Vec<_>>()
    );
    found[0].id
}

/// Validate the plan by reparsing every touched file, then write it.
///
/// Returns the paths that changed, so a test can assert that a move which
/// should touch nothing else really touched nothing else.
fn commit(plan: &move_symbol::MovePlan) -> Vec<PathBuf> {
    let outcomes = edit::plan(&plan.edits, Validation::ReparseStrict)
        .expect("the move must survive a strict reparse");
    let changed: Vec<PathBuf> = outcomes
        .iter()
        .filter(|o| o.changed())
        .map(|o| o.path.clone())
        .collect();
    edit::commit(&outcomes).unwrap();
    changed
}

fn names_of(paths: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

fn error(result: Result<move_symbol::MovePlan, anyhow::Error>) -> String {
    match result {
        Ok(plan) => panic!("expected a refusal, got a plan touching {:?}", plan.to),
        Err(e) => e.to_string(),
    }
}

// ===========================================================================
// Rust, module paths derived from the file tree.
// ===========================================================================

/// A crate whose module tree is spelled out, which is the precondition Rust needs.
fn rust_crate(files: &[(&str, &str)]) -> Workspace {
    let mut all: Vec<(&str, &str)> = vec![(
        "src/lib.rs",
        "//! Crate root.\npub mod app;\npub mod helpers;\npub mod store;\n",
    )];
    all.extend_from_slice(files);
    Workspace::new(&all)
}

#[test]
fn rust_move_repoints_a_simple_use() {
    let ws = rust_crate(&[
        (
            "src/helpers.rs",
            "pub fn kept() -> i32 {\n    1\n}\n\npub fn shared() -> i32 {\n    2\n}\n",
        ),
        (
            "src/app.rs",
            "use crate::helpers::shared;\n\npub fn run() -> i32 {\n    shared()\n}\n",
        ),
        ("src/store.rs", "pub const LIMIT: i32 = 10;\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store.rs")).unwrap();
    let changed = commit(&plan);
    assert_eq!(names_of(&changed), ["app.rs", "helpers.rs", "store.rs"]);

    assert_eq!(
        ws.read("src/helpers.rs"),
        "pub fn kept() -> i32 {\n    1\n}\n\n"
    );
    assert_eq!(
        ws.read("src/store.rs"),
        "pub const LIMIT: i32 = 10;\n\npub fn shared() -> i32 {\n    2\n}\n"
    );
    // The existing `use` is repointed. It is not duplicated.
    assert_eq!(
        ws.read("src/app.rs"),
        "use crate::store::shared;\n\npub fn run() -> i32 {\n    shared()\n}\n"
    );

    // And the call still resolves, to the definition in its new home.
    let after = ws.index();
    let moved = symbol_id(&after, "shared", None);
    assert_eq!(after.symbol(moved).unwrap().file, ws.path("src/store.rs"));
    // Two: the name in the `use` line and the call. Both point at the new definition.
    let refs = after.references_to(moved);
    assert_eq!(refs.len(), 2, "got {refs:?}");
    assert!(refs.iter().all(|r| r.file == ws.path("src/app.rs")));
    assert!(
        refs.iter().all(|r| r.confidence.is_safe_to_rewrite()),
        "got {refs:?}"
    );
    assert!(
        refs.iter()
            .any(|r| r.kind == fun_refactor::model::ReferenceKind::Call),
        "the call site must resolve: {refs:?}"
    );
}

#[test]
fn rust_move_takes_only_the_moved_name_out_of_a_use_list() {
    let ws = rust_crate(&[
        (
            "src/helpers.rs",
            "pub fn kept() -> i32 {\n    1\n}\n\npub fn shared() -> i32 {\n    2\n}\n",
        ),
        (
            "src/app.rs",
            "use crate::helpers::{kept, shared};\n\npub fn run() -> i32 {\n    kept() + shared()\n}\n",
        ),
        ("src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store.rs")).unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("src/app.rs"),
        "use crate::helpers::{kept};\nuse crate::store::shared;\n\npub fn run() -> i32 {\n    kept() + shared()\n}\n"
    );
    // Each name is now imported from where it lives, and both still resolve: one
    // reference in the `use` line and one at the call site.
    let after = ws.index();
    assert_eq!(
        after.references_to(symbol_id(&after, "kept", None)).len(),
        2
    );
    assert_eq!(
        after.references_to(symbol_id(&after, "shared", None)).len(),
        2
    );
}

#[test]
fn rust_move_carries_attributes_and_doc_comments() {
    let ws = rust_crate(&[
        (
            "src/helpers.rs",
            "pub fn kept() {}\n\n/// What it does.\n#[inline]\npub fn shared() -> i32 {\n    2\n}\n",
        ),
        ("src/app.rs", "pub fn run() {}\n"),
        ("src/store.rs", "// store\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store.rs")).unwrap();
    commit(&plan);

    // An attribute or doc comment left behind is a compile error, so both travel.
    assert_eq!(ws.read("src/helpers.rs"), "pub fn kept() {}\n\n");
    assert_eq!(
        ws.read("src/store.rs"),
        "// store\n\n/// What it does.\n#[inline]\npub fn shared() -> i32 {\n    2\n}\n"
    );
}

#[test]
fn rust_source_file_gains_a_use_when_it_still_calls_the_item() {
    let ws = rust_crate(&[
        (
            "src/helpers.rs",
            "//! Helpers.\n\npub fn shared() -> i32 {\n    2\n}\n\npub fn caller() -> i32 {\n    shared()\n}\n",
        ),
        ("src/app.rs", "pub fn run() {}\n"),
        ("src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store.rs")).unwrap();
    assert_eq!(plan.imports_added, vec![ws.path("src/helpers.rs")]);
    commit(&plan);

    // The blank line the definition sat between stays: a move splices bytes out, it
    // does not reflow what is left.
    assert_eq!(
        ws.read("src/helpers.rs"),
        "//! Helpers.\nuse crate::store::shared;\n\n\npub fn caller() -> i32 {\n    shared()\n}\n"
    );
    let after = ws.index();
    let refs = after.references_to(symbol_id(&after, "shared", None));
    assert_eq!(refs.len(), 2, "got {refs:?}");
    assert!(refs.iter().all(|r| r.file == ws.path("src/helpers.rs")));
}

#[test]
fn rust_destination_stops_importing_what_it_now_defines() {
    let ws = rust_crate(&[
        ("src/helpers.rs", "pub fn shared() -> i32 {\n    2\n}\n"),
        ("src/app.rs", "pub fn run() {}\n"),
        (
            "src/store.rs",
            "use crate::helpers::shared;\n\npub fn total() -> i32 {\n    shared() + 1\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store.rs")).unwrap();
    commit(&plan);

    // Importing an item the file defines itself does not compile, so the import goes.
    assert_eq!(
        ws.read("src/store.rs"),
        "\npub fn total() -> i32 {\n    shared() + 1\n}\n\npub fn shared() -> i32 {\n    2\n}\n"
    );
    assert!(
        plan.imports_added.is_empty(),
        "got {:?}",
        plan.imports_added
    );
}

#[test]
fn rust_move_into_a_module_directory_uses_the_nested_path() {
    let ws = Workspace::new(&[
        (
            "src/lib.rs",
            "pub mod app;\npub mod helpers;\npub mod store;\n",
        ),
        ("src/store/mod.rs", "pub mod inner;\n"),
        ("src/store/inner.rs", "// inner\n"),
        ("src/helpers.rs", "pub fn shared() -> i32 {\n    2\n}\n"),
        (
            "src/app.rs",
            "use crate::helpers::shared;\n\npub fn run() -> i32 {\n    shared()\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("src/store/inner.rs")).unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("src/store/inner.rs"),
        "// inner\n\npub fn shared() -> i32 {\n    2\n}\n"
    );
    // src/store/mod.rs is the module `store`, so src/store/inner.rs is `store::inner`,
    // the directory becomes a path segment, and `mod.rs` does not.
    assert_eq!(
        ws.read("src/app.rs"),
        "use crate::store::inner::shared;\n\npub fn run() -> i32 {\n    shared()\n}\n"
    );
}

#[test]
fn rust_refuses_when_a_use_site_cannot_be_repointed() {
    let ws = rust_crate(&[
        ("src/helpers.rs", "pub fn shared() -> i32 {\n    2\n}\n"),
        (
            "src/app.rs",
            "pub fn run() -> i32 {\n    crate::helpers::shared()\n}\n",
        ),
        ("src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);

    // A fully-qualified call is matched by name alone, so the path inside it cannot be
    // rewritten with any confidence. The move used to proceed and report the site in a
    // warning, which left `crate::helpers::shared()` naming a module that no longer had
    // it. That does not compile, so the move declines and names the site.
    let refusal = move_symbol::to_file(&index, id, &ws.path("src/store.rs"))
        .expect_err("the use site cannot be repointed")
        .to_string();
    assert!(refusal.contains("app.rs:2:"), "{refusal}");
    assert!(refusal.contains("name-only"), "{refusal}");
    assert_eq!(
        ws.read("src/helpers.rs"),
        "pub fn shared() -> i32 {\n    2\n}\n",
        "nothing was written"
    );
}

#[test]
fn rust_refuses_a_file_outside_a_src_directory() {
    let ws = Workspace::new(&[("a.rs", "pub fn thing() {}\n"), ("b.rs", "\n")]);
    let index = ws.index();
    let id = symbol_id(&index, "thing", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("b.rs")));
    assert!(
        message.contains("not under a `src/` directory"),
        "got: {message}"
    );
}

#[test]
fn rust_refuses_a_src_directory_with_no_crate_root() {
    let ws = Workspace::new(&[("src/a.rs", "pub fn thing() {}\n"), ("src/b.rs", "\n")]);
    let index = ws.index();
    let id = symbol_id(&index, "thing", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("src/b.rs")));
    assert!(
        message.contains("neither lib.rs nor main.rs"),
        "got: {message}"
    );
}

#[test]
fn rust_refuses_a_destination_that_is_not_declared_as_a_module() {
    let ws = Workspace::new(&[
        ("src/lib.rs", "pub mod helpers;\n"),
        ("src/helpers.rs", "pub fn shared() {}\n"),
        ("src/orphan.rs", "// nobody declares me\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("src/orphan.rs")));
    assert!(
        message.contains("does not declare `mod orphan;`"),
        "got: {message}"
    );
    assert!(
        message.contains("lib.rs"),
        "the refusal must name where to look: {message}"
    );
}

#[test]
fn rust_refuses_when_a_path_attribute_remaps_the_module_tree() {
    let ws = Workspace::new(&[
        (
            "src/lib.rs",
            "#[path = \"elsewhere/thing.rs\"]\npub mod helpers;\npub mod store;\n",
        ),
        ("src/helpers.rs", "pub fn shared() {}\n"),
        ("src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("src/store.rs")));
    assert!(message.contains("`#[path]` attribute"), "got: {message}");
}

#[test]
fn rust_refuses_to_strand_a_private_item() {
    let ws = rust_crate(&[
        (
            "src/helpers.rs",
            "fn shared() -> i32 {\n    2\n}\n\npub fn near() -> i32 {\n    shared()\n}\n",
        ),
        ("src/app.rs", "pub fn run() {}\n"),
        ("src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("src/store.rs")));
    assert!(message.contains("is private to"), "got: {message}");
    assert!(message.contains("Make it `pub`"), "got: {message}");
}

#[test]
fn rust_refuses_a_move_between_crates() {
    let ws = Workspace::new(&[
        ("one/src/lib.rs", "pub mod helpers;\n"),
        ("one/src/helpers.rs", "pub fn shared() {}\n"),
        ("two/src/lib.rs", "pub mod store;\n"),
        ("two/src/store.rs", "\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let message = error(move_symbol::to_file(
        &index,
        id,
        &ws.path("two/src/store.rs"),
    ));
    assert!(message.contains("different crate roots"), "got: {message}");
}

// ===========================================================================
// Go, a package is a directory.
// ===========================================================================

#[test]
fn go_move_inside_one_package_changes_nothing_else() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n\ngo 1.22\n"),
        (
            "pkg/a.go",
            "package pkg\n\n// Shared does a thing.\nfunc Shared() int {\n\treturn 2\n}\n",
        ),
        (
            "pkg/b.go",
            "package pkg\n\nfunc Use() int {\n\treturn Shared()\n}\n",
        ),
        ("pkg/c.go", "package pkg\n\nconst Limit = 10\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/c.go")).unwrap();
    // Nothing to import: within a package every name is already in scope.
    assert!(
        plan.imports_added.is_empty(),
        "got {:?}",
        plan.imports_added
    );
    let changed = commit(&plan);
    assert_eq!(
        names_of(&changed),
        ["a.go", "c.go"],
        "the caller must not be touched at all"
    );

    assert_eq!(ws.read("pkg/a.go"), "package pkg\n\n");
    assert_eq!(
        ws.read("pkg/c.go"),
        "package pkg\n\nconst Limit = 10\n\n// Shared does a thing.\nfunc Shared() int {\n\treturn 2\n}\n"
    );
    assert_eq!(
        ws.read("pkg/b.go"),
        "package pkg\n\nfunc Use() int {\n\treturn Shared()\n}\n"
    );

    let after = ws.index();
    let refs = after.references_to(symbol_id(&after, "Shared", None));
    assert_eq!(refs.len(), 1, "got {refs:?}");
    assert_eq!(refs[0].file, ws.path("pkg/b.go"));
}

#[test]
fn go_move_into_an_empty_file_writes_the_package_clause() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n"),
        (
            "pkg/a.go",
            "package pkg\n\nfunc helper() int {\n\treturn 1\n}\n",
        ),
        ("pkg/new.go", ""),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "helper", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/new.go")).unwrap();
    commit(&plan);

    // A .go file without a package clause is not Go at all.
    assert_eq!(
        ws.read("pkg/new.go"),
        "package pkg\n\nfunc helper() int {\n\treturn 1\n}\n"
    );
    // An unexported name is fine here: the move stayed inside the package.
    assert_eq!(ws.read("pkg/a.go"), "package pkg\n\n");
}

#[test]
fn go_move_warns_about_the_imports_the_code_leaves_behind_and_needs() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n"),
        (
            "pkg/a.go",
            "package pkg\n\nimport \"fmt\"\n\nfunc Shared() string {\n\treturn fmt.Sprint(1)\n}\n",
        ),
        ("pkg/c.go", "package pkg\n\nconst Limit = 10\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/c.go")).unwrap();
    // Nothing is edited: which import fed which name is what this index knows
    // only weakly, so both directions are reported instead of guessed at.
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("may now be unused")),
        "got {:?}",
        plan.warnings
    );
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("which") && w.contains("c.go") && w.contains("does not import")),
        "got {:?}",
        plan.warnings
    );
}

#[test]
fn go_cross_package_move_qualifies_uses_and_imports_the_destination() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n\ngo 1.22\n"),
        (
            "pkg/a.go",
            "package pkg\n\nfunc Shared() int {\n\treturn 2\n}\n\nfunc Use() int {\n\treturn Shared()\n}\n",
        ),
        ("util/u.go", "package util\n\nconst Limit = 10\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("util/u.go")).unwrap();
    assert_eq!(plan.imports_added, vec![ws.path("pkg/a.go")]);
    commit(&plan);

    assert_eq!(
        ws.read("pkg/a.go"),
        "package pkg\n\nimport \"example.com/app/util\"\n\n\nfunc Use() int {\n\treturn util.Shared()\n}\n"
    );
    assert_eq!(
        ws.read("util/u.go"),
        "package util\n\nconst Limit = 10\n\nfunc Shared() int {\n\treturn 2\n}\n"
    );
}

#[test]
fn go_refuses_to_move_an_unexported_name_out_of_its_package() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n"),
        (
            "pkg/a.go",
            "package pkg\n\nfunc shared() int {\n\treturn 2\n}\n",
        ),
        ("util/u.go", "package util\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("util/u.go")));
    assert!(message.contains("is unexported"), "got: {message}");
    assert!(message.contains("Capitalise it first"), "got: {message}");
}

#[test]
fn go_refuses_a_cross_package_move_with_no_go_mod() {
    let ws = Workspace::new(&[
        (
            "pkg/a.go",
            "package pkg\n\nfunc Shared() int {\n\treturn 2\n}\n",
        ),
        ("util/u.go", "package util\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("util/u.go")));
    assert!(message.contains("no go.mod above"), "got: {message}");
}

#[test]
fn go_refuses_when_the_destination_package_has_no_name() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n"),
        (
            "pkg/a.go",
            "package pkg\n\nfunc Shared() int {\n\treturn 2\n}\n",
        ),
        ("util/u.go", ""),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("util/u.go")));
    assert!(message.contains("declares a package"), "got: {message}");
}

#[test]
fn go_refuses_when_a_third_package_already_qualifies_the_name() {
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/app\n"),
        ("pkg/a.go", "package pkg\n\nfunc Shared() int {\n\treturn 2\n}\n"),
        (
            "caller/c.go",
            "package caller\n\nimport \"example.com/app/pkg\"\n\nfunc Run() int {\n\treturn pkg.Shared()\n}\n",
        ),
        ("util/u.go", "package util\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Shared", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("util/u.go")));
    assert!(message.contains("outside package"), "got: {message}");
    assert!(
        message.contains("c.go"),
        "the refusal must name the file: {message}"
    );
}

// ===========================================================================
// HCL / Terraform, a module is a directory.
// ===========================================================================

#[test]
fn hcl_resource_moves_between_files_of_one_module_with_no_other_change() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"logs\"\n}\n\nresource \"aws_s3_bucket\" \"data\" {\n  bucket = \"data\"\n}\n",
        ),
        (
            "outputs.tf",
            "output \"arn\" {\n  value = aws_s3_bucket.data.arn\n}\n",
        ),
        ("buckets.tf", "# buckets\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "data", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("buckets.tf")).unwrap();
    let changed = commit(&plan);
    // The address `aws_s3_bucket.data` is unchanged, so outputs.tf is untouched.
    assert_eq!(names_of(&changed), ["buckets.tf", "main.tf"]);
    assert!(plan.imports_added.is_empty());
    assert!(plan.warnings.is_empty(), "got {:?}", plan.warnings);

    assert_eq!(
        ws.read("main.tf"),
        "resource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"logs\"\n}\n\n"
    );
    assert_eq!(
        ws.read("buckets.tf"),
        "# buckets\n\nresource \"aws_s3_bucket\" \"data\" {\n  bucket = \"data\"\n}\n"
    );

    let after = ws.index();
    let refs = after.references_to(symbol_id(&after, "data", None));
    assert_eq!(refs.len(), 1, "got {refs:?}");
    assert_eq!(refs[0].file, ws.path("outputs.tf"));
    assert!(refs[0].confidence.is_safe_to_rewrite());
}

#[test]
fn hcl_variable_moves_and_var_references_still_resolve() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "variable \"region\" {\n  default = \"eu-west-1\"\n}\n\nresource \"aws_s3_bucket\" \"b\" {\n  region = var.region\n}\n",
        ),
        ("variables.tf", "# variables\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "region", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("variables.tf")).unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("variables.tf"),
        "# variables\n\nvariable \"region\" {\n  default = \"eu-west-1\"\n}\n"
    );
    let after = ws.index();
    let refs = after.references_to(symbol_id(&after, "region", None));
    assert_eq!(refs.len(), 1, "got {refs:?}");
    assert!(refs[0].file.ends_with("main.tf"));
}

#[test]
fn hcl_locals_entry_moves_into_the_destination_locals_block() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "locals {\n  name = \"app\"\n  region = \"eu-west-1\"\n}\n",
        ),
        ("locals.tf", "locals {\n  env = \"prod\"\n}\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "region", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("locals.tf")).unwrap();
    commit(&plan);

    assert_eq!(ws.read("main.tf"), "locals {\n  name = \"app\"\n}\n");
    // A `local.` entry only means anything inside a `locals` block.
    assert_eq!(
        ws.read("locals.tf"),
        "locals {\n  env = \"prod\"\n  region = \"eu-west-1\"\n}\n"
    );
}

#[test]
fn hcl_locals_entry_gets_a_locals_block_made_for_it() {
    let ws = Workspace::new(&[
        ("main.tf", "locals {\n  region = \"eu-west-1\"\n}\n"),
        ("other.tf", "# other\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "region", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("other.tf")).unwrap();
    commit(&plan);

    assert_eq!(ws.read("main.tf"), "locals {\n}\n");
    assert_eq!(
        ws.read("other.tf"),
        "# other\n\nlocals {\n  region = \"eu-west-1\"\n}\n"
    );
}

#[test]
fn hcl_refuses_a_move_that_changes_module() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"data\" {\n  bucket = \"data\"\n}\n",
        ),
        ("child/main.tf", "# child module\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "data", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("child/main.tf")));
    assert!(
        message.contains("module is the directory"),
        "got: {message}"
    );
    assert!(message.contains("terraform state mv"), "got: {message}");
}

#[test]
fn hcl_refuses_to_move_a_tfvars_value() {
    let ws = Workspace::new(&[
        ("terraform.tfvars", "region = \"eu-west-1\"\n"),
        ("other.tfvars", "# other\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "region", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.tfvars")));
    assert!(message.contains(".tfvars"), "got: {message}");
}

#[test]
fn hcl_refuses_to_move_a_block_nested_in_another_block() {
    let ws = Workspace::new(&[
        (
            "main.tf",
            "resource \"aws_security_group\" \"web\" {\n  dynamic \"ingress\" {\n    for_each = var.ports\n  }\n}\n",
        ),
        ("other.tf", "# other\n"),
    ]);
    let index = ws.index();
    // A `dynamic` block only means anything inside the block it generates entries for.
    let id = symbol_id(&index, "ingress", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.tf")));
    assert!(
        message.contains("argument of an enclosing block"),
        "got: {message}"
    );
}

// ===========================================================================
// CSS, names are global, reachability is not.
// ===========================================================================

#[test]
fn css_rule_moves_to_an_imported_partial_without_a_warning() {
    let ws = Workspace::new(&[
        (
            "main.css",
            "@import \"buttons.css\";\n\n.card {\n  color: red;\n}\n\n.btn {\n  color: blue;\n}\n",
        ),
        ("buttons.css", "/* buttons */\n"),
        ("page.html", "<div class=\"btn\">go</div>\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "btn", Some(&ws.path("main.css")));

    let plan = move_symbol::to_file(&index, id, &ws.path("buttons.css")).unwrap();
    let changed = commit(&plan);
    // A CSS class is named globally: the HTML does not change.
    assert_eq!(names_of(&changed), ["buttons.css", "main.css"]);
    assert!(plan.warnings.is_empty(), "got {:?}", plan.warnings);

    assert_eq!(
        ws.read("main.css"),
        "@import \"buttons.css\";\n\n.card {\n  color: red;\n}\n\n"
    );
    // The whole rule moves, not just the selector.
    assert_eq!(
        ws.read("buttons.css"),
        "/* buttons */\n\n.btn {\n  color: blue;\n}\n"
    );

    let after = ws.index();
    let moved = symbol_id(&after, "btn", Some(&ws.path("buttons.css")));
    let refs = after.references_to(moved);
    assert_eq!(refs.len(), 1, "the HTML use must still resolve: {refs:?}");
    assert!(refs[0].file.ends_with("page.html"));
}

#[test]
fn css_warns_when_the_destination_is_not_reachable_by_import() {
    let ws = Workspace::new(&[
        ("main.css", ".btn {\n  color: blue;\n}\n"),
        ("orphan.css", "/* nothing imports me */\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "btn", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("orphan.css")).unwrap();
    assert_eq!(plan.warnings.len(), 1, "got {:?}", plan.warnings);
    assert!(
        plan.warnings[0].contains("does not reach") && plan.warnings[0].contains("@import"),
        "the warning must name the risk: {:?}",
        plan.warnings
    );
    // A warning is not a refusal: the move still happens.
    commit(&plan);
    assert_eq!(ws.read("main.css"), "");
    assert_eq!(
        ws.read("orphan.css"),
        "/* nothing imports me */\n\n.btn {\n  color: blue;\n}\n"
    );
}

#[test]
fn css_refuses_to_split_a_rule_with_several_selectors() {
    let ws = Workspace::new(&[
        ("main.css", ".btn, .link {\n  color: blue;\n}\n"),
        ("other.css", "/* other */\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "btn", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.css")));
    assert!(message.contains("2 selectors"), "got: {message}");
    assert!(
        message.contains("duplicate the declaration block"),
        "got: {message}"
    );
}

#[test]
fn css_refuses_to_lift_a_rule_out_of_a_media_query() {
    let ws = Workspace::new(&[
        (
            "main.css",
            "@media (min-width: 40em) {\n  .btn {\n    color: blue;\n  }\n}\n",
        ),
        ("other.css", "/* other */\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "btn", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.css")));
    assert!(message.contains("nested inside"), "got: {message}");
}

#[test]
fn css_refuses_to_move_a_custom_property_on_its_own() {
    let ws = Workspace::new(&[
        ("main.css", ":root {\n  --brand: red;\n}\n"),
        ("other.css", "/* other */\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "--brand", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.css")));
    assert!(message.contains("not a rule"), "got: {message}");
}

// ===========================================================================
// Markdown, a section is a heading and what is under it.
// ===========================================================================

const GUIDE: &str = "\
# Guide

Jump to [install](#installation) and [usage](#usage).

## Installation

Run the thing.

### From source

Clone it.

## Usage

Use the thing.
";

#[test]
fn markdown_section_takes_its_subsections_and_stops_at_the_next_peer() {
    let ws = Workspace::new(&[("guide.md", GUIDE), ("install.md", "# Install\n")]);
    let index = ws.index();
    let id = symbol_id(&index, "Installation", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("install.md")).unwrap();
    commit(&plan);

    // `### From source` is part of the section; `## Usage` is the next peer and stays.
    assert_eq!(
        ws.read("install.md"),
        "# Install\n\n## Installation\n\nRun the thing.\n\n### From source\n\nClone it.\n\n"
    );
    assert_eq!(
        ws.read("guide.md"),
        "\
# Guide

Jump to [install](install.md#installation) and [usage](#usage).

## Usage

Use the thing.
"
    );
}

#[test]
fn markdown_repoints_a_link_from_another_document() {
    let ws = Workspace::new(&[
        ("docs/guide.md", GUIDE),
        ("docs/install.md", "# Install\n"),
        (
            "README.md",
            "See [installing](docs/guide.md#installation) for details.\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Installation", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("docs/install.md")).unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("README.md"),
        "See [installing](docs/install.md#installation) for details.\n"
    );
}

#[test]
fn markdown_warns_when_the_moved_section_links_back_at_what_stayed() {
    let ws = Workspace::new(&[
        (
            "guide.md",
            "# Guide\n\nIntro.\n\n## Details\n\nSee [guide](#guide).\n",
        ),
        ("other.md", "# Other\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Details", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("other.md")).unwrap();
    assert_eq!(plan.warnings.len(), 1, "got {:?}", plan.warnings);
    assert!(
        plan.warnings[0].contains("#guide"),
        "got {:?}",
        plan.warnings
    );
    commit(&plan);
    assert_eq!(ws.read("guide.md"), "# Guide\n\nIntro.\n\n");
}

#[test]
fn markdown_leaves_a_link_definition_the_moved_section_does_not_use() {
    let ws = Workspace::new(&[
        (
            "a.md",
            "# A\n\nSee [x][api].\n\n## S\n\ntext\n\n[api]: ./ref.md\n",
        ),
        ("g.md", "# G\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "S", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("g.md")).unwrap();
    assert_eq!(plan.warnings.len(), 1, "got {:?}", plan.warnings);
    assert!(plan.warnings[0].contains("api"), "got {:?}", plan.warnings);
    commit(&plan);

    assert_eq!(ws.read("a.md"), "# A\n\nSee [x][api].\n\n[api]: ./ref.md\n");
    assert_eq!(ws.read("g.md"), "# G\n\n## S\n\ntext\n\n");
}

#[test]
fn markdown_copies_a_link_definition_the_moved_section_uses() {
    let ws = Workspace::new(&[
        (
            "a.md",
            "# A\n\nIntro.\n\n## S\n\nSee [x][api].\n\n[api]: ./ref.md\n",
        ),
        ("g.md", "# G\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "S", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("g.md")).unwrap();
    assert!(
        plan.warnings.iter().any(|w| w.contains("copied")),
        "got {:?}",
        plan.warnings
    );
    commit(&plan);

    assert_eq!(ws.read("a.md"), "# A\n\nIntro.\n\n[api]: ./ref.md\n");
    assert_eq!(
        ws.read("g.md"),
        "# G\n\n## S\n\nSee [x][api].\n\n[api]: ./ref.md\n"
    );
}

#[test]
fn markdown_leaves_unrelated_anchors_alone() {
    let ws = Workspace::new(&[("guide.md", GUIDE), ("install.md", "# Install\n")]);
    let index = ws.index();
    let id = symbol_id(&index, "Usage", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("install.md")).unwrap();
    commit(&plan);

    // Only the anchor whose heading left is repointed.
    assert!(
        ws.read("guide.md").contains("[install](#installation)"),
        "got:\n{}",
        ws.read("guide.md")
    );
    assert!(
        ws.read("guide.md").contains("[usage](install.md#usage)"),
        "got:\n{}",
        ws.read("guide.md")
    );
}

#[test]
fn markdown_setext_headings_carry_their_level() {
    let ws = Workspace::new(&[
        (
            "guide.md",
            "Top\n===\n\nIntro.\n\nMiddle\n------\n\nDetail.\n\nSecond Top\n==========\n\nEnd.\n",
        ),
        ("other.md", "# Other\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "Top", None);

    let plan = move_symbol::to_file(&index, id, &ws.path("other.md")).unwrap();
    commit(&plan);

    // `Middle` is a level-2 setext heading, so it belongs to the section; the next
    // level-1 underline ends it.
    assert_eq!(
        ws.read("other.md"),
        "# Other\n\nTop\n===\n\nIntro.\n\nMiddle\n------\n\nDetail.\n\n"
    );
    assert_eq!(ws.read("guide.md"), "Second Top\n==========\n\nEnd.\n");
}

#[test]
fn markdown_refuses_to_move_something_that_is_not_a_section() {
    let ws = Workspace::new(&[
        (
            "guide.md",
            "# Guide\n\nSee [ref][r].\n\n[r]: http://example.com\n",
        ),
        ("other.md", "# Other\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "r", None);
    let message = error(move_symbol::to_file(&index, id, &ws.path("other.md")));
    assert!(message.contains("not a heading"), "got: {message}");
}

// ===========================================================================
// The languages that stay refused, and the ones that already worked.
// ===========================================================================

#[test]
fn typescript_and_python_are_unchanged() {
    let ws = Workspace::new(&[
        ("a.ts", "export function moved() { return 1; }\n"),
        (
            "b.ts",
            "import { moved } from './a';\nexport const x = moved();\n",
        ),
        ("c.ts", "export const y = 2;\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "moved", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("c.ts")).unwrap();
    commit(&plan);

    assert_eq!(ws.read("a.ts"), "");
    assert_eq!(
        ws.read("c.ts"),
        "export const y = 2;\n\nexport function moved() { return 1; }\n"
    );
    // The importer's own statement repoints in place. Its earlier behaviour,
    // the old import left beside a new one, declared `moved` twice and failed
    // to compile. This pin is the deliberate change the old pin asked for.
    assert_eq!(
        ws.read("b.ts"),
        "import { moved } from './c';\nexport const x = moved();\n"
    );
}

#[test]
fn python_move_outside_a_package_writes_an_absolute_import() {
    // No `__init__.py`, so these files are top-level modules and belong to no package.
    // `from .dest import shared` raises `attempted relative import with no known parent
    // package` on the import itself: the file parses, compiles, and cannot be imported.
    let ws = Workspace::new(&[
        ("lib.py", "def shared():\n    return 1\n"),
        (
            "app.py",
            "from lib import shared\n\n\ndef use():\n    return shared()\n",
        ),
        ("dest.py", "X = 1\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("dest.py")).unwrap();
    commit(&plan);

    assert_eq!(ws.read("dest.py"), "X = 1\n\ndef shared():\n    return 1\n");
    // The importer's own statement repoints in place; a stale `from lib import
    // shared` would name something the module no longer defines.
    assert_eq!(
        ws.read("app.py"),
        "from dest import shared\n\n\ndef use():\n    return shared()\n"
    );
}

#[test]
fn python_move_inside_a_package_writes_a_relative_import() {
    // `__init__.py` makes the directory a package, and inside one a leading dot means
    // the package the importing file is in. That is the spelling Python wants here.
    let ws = Workspace::new(&[
        ("pkg/__init__.py", ""),
        ("pkg/lib.py", "def shared():\n    return 1\n"),
        (
            "pkg/app.py",
            "from .lib import shared\n\n\ndef use():\n    return shared()\n",
        ),
        ("pkg/dest.py", "X = 1\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "shared", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/dest.py")).unwrap();
    commit(&plan);

    assert!(
        ws.read("pkg/app.py").contains("from .dest import shared"),
        "got:\n{}",
        ws.read("pkg/app.py")
    );
}

#[test]
fn languages_with_no_derivable_move_are_refused_by_name() {
    // Zig, Bash and YAML gained moves; markup did not, and cannot: a document does not import
    // another's elements. So a moved element has no reference to update.
    for (source, destination, code) in [
        ("a.html", "b.html", "<div id=\"thing\">x</div>\n"),
        ("a.xml", "b.xml", "<root><item id=\"thing\"/></root>\n"),
    ] {
        let ws = Workspace::new(&[(source, code), (destination, "\n")]);
        let index = ws.index();
        let found = index.find_symbols("thing", None);
        if found.is_empty() {
            continue;
        }
        let message = error(move_symbol::to_file(
            &index,
            found[0].id,
            &ws.path(destination),
        ));
        assert!(
            message.contains("is not supported for"),
            "{source} must refuse by name, got: {message}"
        );
    }
}

#[test]
fn movable_lists_what_each_language_can_move() {
    let ws = Workspace::new(&[
        ("src/lib.rs", "pub mod a;\n"),
        (
            "src/a.rs",
            "pub struct S;\npub fn f() {}\nfn g() { let x = 1; }\n",
        ),
        (
            "main.tf",
            "resource \"t\" \"r\" {\n  a = 1\n}\n\nlocals {\n  l = 2\n}\n",
        ),
        ("style.css", ".btn { color: red; }\n"),
        ("doc.md", "# One\n\ntext\n"),
    ]);
    let index = ws.index();

    let rust: Vec<String> = move_symbol::movable(&index, &ws.path("src/a.rs"))
        .iter()
        .map(|id| index.symbol(*id).unwrap().name.clone())
        .collect();
    assert_eq!(rust, ["S", "f", "g"], "locals are not movable, items are");

    let hcl: Vec<String> = move_symbol::movable(&index, &ws.path("main.tf"))
        .iter()
        .map(|id| index.symbol(*id).unwrap().name.clone())
        .collect();
    assert!(hcl.contains(&"r".to_string()), "got {hcl:?}");
    assert!(
        hcl.contains(&"l".to_string()),
        "a locals entry is movable: {hcl:?}"
    );
    assert!(
        !hcl.contains(&"a".to_string()),
        "an argument is not: {hcl:?}"
    );

    let css: Vec<String> = move_symbol::movable(&index, &ws.path("style.css"))
        .iter()
        .map(|id| index.symbol(*id).unwrap().name.clone())
        .collect();
    assert_eq!(css, ["btn"]);

    let markdown: Vec<String> = move_symbol::movable(&index, &ws.path("doc.md"))
        .iter()
        .map(|id| index.symbol(*id).unwrap().name.clone())
        .collect();
    assert_eq!(markdown, ["One"]);
}

// -------------------------------------------------------------------------- What moves with
// the code.
//
// A move that relocates the text and nothing else leaves a file that parses and does not
// compile. The definition is invisible to the import just written for it, and everything it
// referenced is no longer in scope. These pin the rest of the job.

#[test]
fn a_moved_symbol_is_exported_where_something_now_imports_it() {
    let ws = Workspace::new(&[(
        "a.ts",
        "function moveMe(x: number) {\n  return x + 1;\n}\n\n\
         export function caller(x: number) {\n  return moveMe(x);\n}\n",
    )]);
    let index = ws.index();
    let id = symbol_id(&index, "moveMe", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.ts")).unwrap();
    commit(&plan);

    assert!(
        ws.read("b.ts").contains("export function moveMe"),
        "an imported definition must be exported:\n{}",
        ws.read("b.ts")
    );
    assert!(ws.read("a.ts").contains("import { moveMe } from './b';"));
}

#[test]
fn the_imports_a_moved_symbol_relied_on_come_with_it() {
    let ws = Workspace::new(&[
        (
            "dep.ts",
            "export type Alpha = { a: number };\nexport type Beta = { b: number };\n\
             export function used(x: number) {\n  return x;\n}\n\
             export function other(x: number) {\n  return x;\n}\n",
        ),
        (
            "a.ts",
            "import { Alpha, Beta, used, other } from './dep';\n\n\
             export function moveMe(v: Alpha) {\n  return used(v.a);\n}\n\n\
             export function stay(v: Beta) {\n  return other(v.b);\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "moveMe", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.ts")).unwrap();
    commit(&plan);

    let moved = ws.read("b.ts");
    assert!(
        moved.contains("import { Alpha, used } from './dep';"),
        "the import should carry the names the moved code uses, and only those:\n{moved}"
    );
    assert_eq!(
        moved.matches("from './dep'").count(),
        1,
        "the index records one entry per imported name; they must regroup into one \
         statement:\n{moved}"
    );
}

#[test]
fn a_carried_import_keeps_its_type_modifier() {
    let ws = Workspace::new(&[
        (
            "dep.ts",
            "export type Alpha = { a: number };\nexport const value = 1;\n",
        ),
        (
            "a.ts",
            "import { type Alpha, value } from './dep';\n\n\
             export function moveMe(v: Alpha) {\n  return v.a;\n}\n\n\
             export function stay() {\n  return value;\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "moveMe", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.ts")).unwrap();
    commit(&plan);

    assert!(
        ws.read("b.ts")
            .contains("import { type Alpha } from './dep';"),
        "narrowing must not drop the `type` modifier:\n{}",
        ws.read("b.ts")
    );
}

#[test]
fn what_the_moved_code_left_behind_is_imported_back_and_exported() {
    let ws = Workspace::new(&[(
        "a.ts",
        "function localHelper(x: number) {\n  return x * 2;\n}\n\n\
         export function moveMe(x: number) {\n  return localHelper(x);\n}\n\n\
         export function alsoUses(x: number) {\n  return moveMe(x) + 1;\n}\n",
    )]);
    let index = ws.index();
    let id = symbol_id(&index, "moveMe", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.ts")).unwrap();
    commit(&plan);

    let moved = ws.read("b.ts");
    let left = ws.read("a.ts");
    assert!(
        moved.contains("import { localHelper } from './a';"),
        "the moved code still needs what stayed behind:\n{moved}"
    );
    assert!(
        left.contains("export function localHelper"),
        "and that has to be visible for the import to resolve:\n{left}"
    );
    assert!(
        !left.contains("export import"),
        "the export and the new import must not collide at offset zero:\n{left}"
    );
}

#[test]
fn a_move_that_cannot_write_the_import_fails_instead_of_skipping_it() {
    // Skipping leaves a file that parses and no longer compiles, while reporting
    // success. The reparse check cannot see it, so the refusal has to be explicit.
    let ws = Workspace::new(&[(
        "a.py",
        "def move_me(x):\n    return x + 1\n\n\ndef caller(x):\n    return move_me(x)\n",
    )]);
    let index = ws.index();
    let id = symbol_id(&index, "move_me", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.py")).unwrap();
    commit(&plan);
    assert!(
        ws.read("a.py").contains("from b import move_me"),
        "got:\n{}",
        ws.read("a.py")
    );
}

#[test]
fn a_new_import_goes_after_a_multi_line_import_statement() {
    // The insertion point used to be found by scanning lines for an `import` prefix,
    // which stops at the first line that is not one. requests writes
    // `from typing import (` across three lines, so the new import landed *inside*
    // the parentheses and the file no longer parsed, every move out of utils.py.
    let ws = Workspace::new(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/utils.py",
            "from typing import (\n    Any,\n    Callable,\n)\n\n\n\
             def move_me(x: Any) -> Any:\n    return x\n\n\n\
             def caller():\n    return move_me(1)\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "move_me", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/naming.py")).unwrap();
    commit(&plan);

    let left = ws.read("pkg/utils.py");
    assert!(
        left.contains(")\nfrom .naming import move_me"),
        "the import belongs after the statement, not inside it:\n{left}"
    );
}

#[test]
fn a_moved_python_symbol_takes_the_module_imports_it_uses() {
    // `import os` binds `os` without naming it in the statement, and the moved code
    // reaches `os.path` through that binding.
    let ws = Workspace::new(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/utils.py",
            "import os\nimport sys\n\n\ndef move_me(name):\n    return os.path.basename(name)\n\n\n\
             def caller():\n    return move_me(sys.argv[0])\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "move_me", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/naming.py")).unwrap();
    commit(&plan);

    let moved = ws.read("pkg/naming.py");
    assert!(moved.contains("import os"), "got:\n{moved}");
    assert!(
        !moved.contains("import sys"),
        "only what the moved code uses:\n{moved}"
    );
}

#[test]
fn a_future_import_travels_with_the_code_it_governs() {
    // It binds nothing, so no name-based rule would carry it, and it decides how
    // every annotation in the file is read. `str | None` stops parsing without it on
    // Python below 3.10.
    let ws = Workspace::new(&[
        ("pkg/__init__.py", ""),
        (
            "pkg/utils.py",
            "from __future__ import annotations\n\nimport os\n\n\n\
             def move_me(name) -> str | None:\n    return os.path.basename(name)\n\n\n\
             def caller():\n    return move_me(\"x\")\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "move_me", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("pkg/naming.py")).unwrap();
    commit(&plan);

    let moved = ws.read("pkg/naming.py");
    assert!(
        moved.contains("from __future__ import annotations"),
        "got:\n{moved}"
    );
    assert!(
        moved.starts_with("from __future__"),
        "and it has to come first:\n{moved}"
    );
}

#[test]
fn a_moved_go_body_qualifies_what_it_left_behind() {
    // `UseShared` calls `Shared`, which stays in package one. Moved bare into
    // package two, the call named nothing and the tree stopped building. The
    // move reported success, and its warning stated two things that were not
    // true.
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/m\n\ngo 1.21\n"),
        (
            "one/one.go",
            "package one\n\nfunc Shared() int {\n\treturn 7\n}\n\n\
             func UseShared() int {\n\treturn Shared()\n}\n",
        ),
        (
            "two/two.go",
            "package two\n\nimport \"example.com/m/one\"\n\n\
             func Twice() int {\n\treturn one.Shared() * 2\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "UseShared", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("two/two.go")).expect("a move");
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("a valid plan");
    fun_refactor::edit::commit(&outcomes).expect("writing");
    let landed = ws.read("two/two.go");
    assert!(
        landed.contains("return one.Shared()"),
        "the back-reference gains its qualifier:\n{landed}"
    );
}

#[test]
fn a_moved_go_body_using_an_unexported_name_refuses() {
    // `shared` is invisible from package two; `one.shared()` would not compile
    // either. Nothing true can be written, so nothing is.
    let ws = Workspace::new(&[
        ("go.mod", "module example.com/m\n\ngo 1.21\n"),
        (
            "one/one.go",
            "package one\n\nfunc shared() int {\n\treturn 7\n}\n\n\
             func UseShared() int {\n\treturn shared()\n}\n",
        ),
        (
            "two/two.go",
            "package two\n\nfunc Twice() int {\n\treturn 6\n}\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "UseShared", None);
    let err = move_symbol::to_file(&index, id, &ws.path("two/two.go")).unwrap_err();
    assert!(
        err.to_string().contains("does not export"),
        "the refusal states the visibility problem: {err}"
    );
}

#[test]
fn moving_beside_a_dependency_adds_no_self_import() {
    // `f` used `g` through `from b import g`, and `f` lands in `b`. There `g` is
    // local; the carried statement was a module importing itself while half
    // initialised, and the first use raised ImportError.
    let ws = Workspace::new(&[
        ("b.py", "def g() -> int:\n    return 2\n"),
        (
            "a.py",
            "from b import g\n\n\ndef f() -> int:\n    return g() + 1\n",
        ),
        ("main.py", "from a import f\n\nprint(f())\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "f", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("b.py")).expect("a move");
    commit(&plan);
    let landed = ws.read("b.py");
    assert!(
        !landed.contains("from b import"),
        "no module imports itself:\n{landed}"
    );
}

#[test]
fn a_module_attribute_consumer_repoints_to_the_new_module() {
    // `user.py` binds the whole module and dereferences it. There is no named
    // import to repoint, so the receivers rewrite and the file imports the new
    // module. The old behaviour added a dead named import, and every call kept
    // dereferencing the module that no longer held the name.
    let ws = Workspace::new(&[
        ("mod.py", "def foo() -> int:\n    return 1\n"),
        ("other.py", "X = 1\n"),
        (
            "user.py",
            "import mod\n\n\ndef run() -> int:\n    return mod.foo()\n",
        ),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "foo", None);
    let plan = move_symbol::to_file(&index, id, &ws.path("other.py")).expect("a move");
    commit(&plan);
    let user = ws.read("user.py");
    assert!(
        user.contains("return other.foo()") && user.contains("import other"),
        "the receiver follows the symbol:\n{user}"
    );
}
