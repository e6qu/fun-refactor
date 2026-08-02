//! Move to file for the languages that used to refuse: Zig, Bash, YAML and Helm.
//!
//! Each of the three has a different answer to "what else has to change":
//!
//! - **Zig** resolves through `@import`, so a moved declaration is reached by a new
//!   namespace. The tests assert the `const … = @import(…)` line byte for byte and
//!   the qualifier on every use.
//! - **Bash** has no import that binds a name — `source` splices a whole script in —
//!   so a moved function needs its surviving callers to source its new home. What
//!   `source` cannot say is what a computed path put in scope, and that refuses.
//! - **YAML / Helm** address a values key by its path, and a top-level key's path
//!   does not mention its file. Nothing needs repointing; what needs saying is that
//!   `helm install` reads only `values.yaml`.
//!
//! HTML and XML stay refused, and there is a test for that too: an element has no
//! name another document imports.
//!
//! Every successful move goes through `edit::plan(…, ReparseStrict)` and is then
//! re-indexed, so a move that produces text the tool can no longer resolve fails here
//! rather than in someone's repository.

use fun_refactor::{
    edit::{self, apply_to_string, Validation},
    index::Index,
    model::{SymbolId, SymbolKind},
    refactor::{move_symbol, Refusal},
    scan::{scan, ScanOptions},
};
use std::path::{Path, PathBuf};

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

fn symbol_id(index: &Index, name: &str, file: Option<&Path>) -> SymbolId {
    let found = index.find_symbols(name, file);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one '{name}', got {:?}",
        found.iter().map(|s| (&s.file, s.kind)).collect::<Vec<_>>()
    );
    found[0].id
}

fn symbol_of_kind(index: &Index, name: &str, kind: SymbolKind) -> SymbolId {
    let found: Vec<_> = index
        .find_symbols(name, None)
        .into_iter()
        .filter(|s| s.kind == kind)
        .collect();
    assert_eq!(found.len(), 1, "expected one {kind:?} named '{name}'");
    found[0].id
}

/// Validate by reparsing every touched file, then write it. Returns what changed.
fn commit(plan: &move_symbol::MovePlan) -> Vec<PathBuf> {
    let outcomes = edit::plan(&plan.edits, Validation::ReparseStrict)
        .expect("the move must survive a strict reparse");
    let changed = outcomes
        .iter()
        .filter(|o| o.changed())
        .map(|o| o.path.clone())
        .collect();
    edit::commit(&outcomes).unwrap();
    changed
}

fn applied(plan: &move_symbol::MovePlan, path: &Path) -> String {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    match plan.edits.edits_for(path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    }
}

fn error(result: anyhow::Result<move_symbol::MovePlan>) -> String {
    match result {
        Ok(plan) => panic!(
            "expected a refusal, got a plan touching {} file(s)",
            plan.edits.file_count()
        ),
        Err(e) => e.to_string(),
    }
}

fn refusal(result: anyhow::Result<move_symbol::MovePlan>) -> String {
    match result {
        Ok(_) => panic!("expected a refusal, got a plan"),
        Err(e) => {
            assert!(
                e.downcast_ref::<Refusal>().is_some(),
                "expected a structured refusal, got: {e}"
            );
            e.to_string()
        }
    }
}

// ===========================================================================
// Zig: a file is a namespace reached through `@import`.
// ===========================================================================

#[test]
fn zig_qualifies_the_uses_left_behind_in_the_source_file() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "pub fn thing() void {}\npub fn user() void {\n    thing();\n}\n",
        ),
        ("b.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("b.zig"),
    )
    .unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("a.zig"),
        "const b = @import(\"b.zig\");\npub fn user() void {\n    b.thing();\n}\n"
    );
    assert_eq!(
        ws.read("b.zig"),
        "pub const K: i32 = 1;\n\npub fn thing() void {}\n"
    );
    assert_eq!(plan.imports_added, vec![ws.path("a.zig")]);

    // The moved declaration is still reachable from where it is used.
    let rebuilt = ws.index();
    let moved = symbol_id(&rebuilt, "thing", Some(&ws.path("b.zig")));
    assert!(
        !rebuilt.references_to(moved).is_empty(),
        "the qualified call must still resolve to the moved declaration"
    );
}

#[test]
fn zig_reuses_an_import_the_file_already_has() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "const helpers = @import(\"helpers.zig\");\npub fn thing() void {}\npub fn user() void {\n    thing();\n}\n",
        ),
        ("helpers.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("helpers.zig"),
    )
    .unwrap();
    commit(&plan);

    // No second import: the file already names that namespace `helpers`.
    assert_eq!(
        ws.read("a.zig"),
        "const helpers = @import(\"helpers.zig\");\npub fn user() void {\n    helpers.thing();\n}\n"
    );
    assert!(
        plan.imports_added.is_empty(),
        "nothing was added: {:?}",
        plan.imports_added
    );
}

#[test]
fn zig_repoints_a_qualified_use_in_another_file() {
    let ws = Workspace::new(&[
        ("a.zig", "pub fn thing() void {}\npub fn other() void {}\n"),
        (
            "caller.zig",
            "const a = @import(\"a.zig\");\npub fn go() void {\n    a.thing();\n    a.other();\n}\n",
        ),
        ("dest.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("dest.zig"),
    )
    .unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("caller.zig"),
        "const a = @import(\"a.zig\");\nconst dest = @import(\"dest.zig\");\n\
         pub fn go() void {\n    dest.thing();\n    a.other();\n}\n"
    );
    // `a` is still used for `other`, so no "unused import" warning.
    assert!(
        !plan.warnings.iter().any(|w| w.contains("may now be unused")),
        "got: {:?}",
        plan.warnings
    );
}

#[test]
fn zig_warns_when_the_old_namespace_is_left_with_nothing_to_name() {
    let ws = Workspace::new(&[
        ("a.zig", "pub fn thing() void {}\n"),
        (
            "caller.zig",
            "const a = @import(\"a.zig\");\npub fn go() void {\n    a.thing();\n}\n",
        ),
        ("dest.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("dest.zig"),
    )
    .unwrap();
    assert!(
        plan.warnings.iter().any(|w| w.contains("may now be unused")),
        "got: {:?}",
        plan.warnings
    );
    commit(&plan);
}

#[test]
fn zig_moves_into_a_subdirectory() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "pub fn thing() void {}\npub fn user() void {\n    thing();\n}\n",
        ),
        ("sub/dest.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("sub/dest.zig"),
    )
    .unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("a.zig"),
        "const dest = @import(\"sub/dest.zig\");\npub fn user() void {\n    dest.thing();\n}\n"
    );
}

#[test]
fn zig_refuses_a_destination_that_would_need_a_climbing_import_path() {
    // Zig rejects an `@import` that leaves the module root, and where that root is
    // cannot be read off two file paths.
    let ws = Workspace::new(&[
        (
            "sub/a.zig",
            "pub fn thing() void {}\npub fn user() void {\n    thing();\n}\n",
        ),
        ("dest.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let message = refusal(move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("dest.zig"),
    ));
    assert!(message.contains("climbs above"), "got: {message}");
    assert!(message.contains("module root"), "got: {message}");
}

#[test]
fn zig_moves_upward_when_nothing_needs_an_import() {
    // With no remaining use there is no `@import` to write, so the path that could
    // not be computed is never needed. The move is then plainly correct.
    let ws = Workspace::new(&[
        ("sub/a.zig", "pub fn thing() void {}\n"),
        ("dest.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("dest.zig"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("sub/a.zig"), "");
    assert_eq!(
        ws.read("dest.zig"),
        "pub const K: i32 = 1;\n\npub fn thing() void {}\n"
    );
}

#[test]
fn zig_refuses_to_strand_a_declaration_that_is_not_pub() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "fn thing() void {}\npub fn user() void {\n    thing();\n}\n",
        ),
        ("b.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let message = error(move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("b.zig"),
    ));
    assert!(message.contains("is not `pub`"), "got: {message}");
    assert!(message.contains("1 use site(s)"), "got: {message}");
}

#[test]
fn zig_moves_a_private_declaration_nothing_uses() {
    let ws = Workspace::new(&[
        ("a.zig", "fn thing() void {}\npub const K: i32 = 1;\n"),
        ("b.zig", "pub const J: i32 = 2;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("b.zig"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("a.zig"), "pub const K: i32 = 1;\n");
    assert_eq!(ws.read("b.zig"), "pub const J: i32 = 2;\n\nfn thing() void {}\n");
}

#[test]
fn zig_refuses_a_name_the_destination_already_declares() {
    let ws = Workspace::new(&[
        ("a.zig", "pub fn thing() void {}\n"),
        ("b.zig", "pub const thing: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "thing", Some(&ws.path("a.zig")));
    let message = refusal(move_symbol::to_file(&index, id, &ws.path("b.zig")));
    assert!(message.contains("already defined in"), "got: {message}");
}

#[test]
fn zig_refuses_a_declaration_nested_in_a_container() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "pub const Point = struct {\n    pub fn scale() void {}\n};\n",
        ),
        ("b.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let message = error(move_symbol::to_file(
        &index,
        symbol_id(&index, "scale", None),
        &ws.path("b.zig"),
    ));
    assert!(message.contains("top-level"), "got: {message}");
}

#[test]
fn zig_carries_its_doc_comment() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "//! Module docs.\n\n/// What it does.\npub fn thing() void {}\n",
        ),
        ("b.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("b.zig"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("a.zig"), "//! Module docs.\n\n");
    assert_eq!(
        ws.read("b.zig"),
        "pub const K: i32 = 1;\n\n/// What it does.\npub fn thing() void {}\n"
    );
}

#[test]
fn zig_reports_an_import_the_moved_code_depended_on() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "const std = @import(\"std\");\npub fn thing() void {\n    _ = std.mem;\n}\n",
        ),
        ("b.zig", "pub const K: i32 = 1;\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "thing", None),
        &ws.path("b.zig"),
    )
    .unwrap();
    assert!(
        plan.warnings.iter().any(|w| w.contains("std")),
        "the destination does not import std: {:?}",
        plan.warnings
    );
}

// ===========================================================================
// Bash: no import binds a name, only `source`.
// ===========================================================================

#[test]
fn bash_sources_the_new_home_from_the_script_that_still_calls_it() {
    let ws = Workspace::new(&[
        (
            "app.sh",
            "#!/usr/bin/env bash\nset -euo pipefail\n\ngreet() {\n  echo \"hi $1\"\n}\n\ngreet world\n",
        ),
        ("lib.sh", "#!/usr/bin/env bash\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib.sh"),
    )
    .unwrap();
    commit(&plan);

    assert_eq!(
        ws.read("app.sh"),
        "#!/usr/bin/env bash\nset -euo pipefail\nsource \"./lib.sh\"\n\n\ngreet world\n"
    );
    assert_eq!(
        ws.read("lib.sh"),
        "#!/usr/bin/env bash\n\ngreet() {\n  echo \"hi $1\"\n}\n"
    );
    assert_eq!(plan.imports_added, vec![ws.path("app.sh")]);
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("working directory")),
        "the caveat that makes the added line honest: {:?}",
        plan.warnings
    );

    // The call resolves to the function in its new file.
    let rebuilt = ws.index();
    let moved = symbol_id(&rebuilt, "greet", Some(&ws.path("lib.sh")));
    assert_eq!(
        rebuilt.symbol(moved).map(|s| s.kind),
        Some(SymbolKind::Function)
    );
}

#[test]
fn bash_writes_an_explicitly_relative_path_because_a_bare_name_searches_path() {
    let ws = Workspace::new(&[
        ("app.sh", "greet() {\n  echo hi\n}\ngreet\n"),
        ("lib/shared.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib/shared.sh"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("app.sh"), "source \"./lib/shared.sh\"\ngreet\n");
}

#[test]
fn bash_adds_nothing_where_the_destination_is_already_sourced() {
    let ws = Workspace::new(&[
        (
            "app.sh",
            "source ./lib.sh\n\ngreet() {\n  echo hi\n}\n\ngreet\n",
        ),
        ("lib.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib.sh"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("app.sh"), "source ./lib.sh\n\n\ngreet\n");
    assert!(
        plan.imports_added.is_empty(),
        "nothing to add: {:?}",
        plan.imports_added
    );
}

#[test]
fn bash_sources_the_new_home_from_a_downstream_caller_too() {
    let ws = Workspace::new(&[
        ("lib.sh", "greet() {\n  echo hi\n}\n"),
        ("app.sh", "source ./lib.sh\ngreet\n"),
        ("dest.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("dest.sh"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("lib.sh"), "");
    assert_eq!(
        ws.read("app.sh"),
        "source ./lib.sh\nsource \"./dest.sh\"\ngreet\n"
    );
    assert_eq!(ws.read("dest.sh"), "x=1\n\ngreet() {\n  echo hi\n}\n");
}

#[test]
fn bash_refuses_a_caller_that_sources_a_computed_path() {
    let ws = Workspace::new(&[
        ("app.sh", "greet() {\n  echo hi\n}\n"),
        ("other.sh", "d=.\nsource \"$d/app.sh\"\ngreet\n"),
        ("lib.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let message = refusal(move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib.sh"),
    ));
    assert!(message.contains("not a literal"), "got: {message}");
}

#[test]
fn bash_reports_a_caller_that_never_sourced_the_definition() {
    let ws = Workspace::new(&[
        ("app.sh", "greet() {\n  echo hi\n}\n"),
        ("other.sh", "greet\n"),
        ("lib.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib.sh"),
    )
    .unwrap();
    assert!(
        plan.warnings.iter().any(|w| w.contains("never sources")),
        "got: {:?}",
        plan.warnings
    );
    let changed = commit(&plan);
    assert!(
        !changed.contains(&ws.path("other.sh")),
        "a script that was calling something else is left alone: {changed:?}"
    );
    assert_eq!(ws.read("other.sh"), "greet\n");
}

#[test]
fn bash_refuses_a_name_the_destination_already_defines() {
    let ws = Workspace::new(&[
        ("app.sh", "greet() {\n  echo hi\n}\n"),
        ("lib.sh", "greet() {\n  echo bye\n}\n"),
    ]);
    let index = ws.index();
    let id = index.find_symbols("greet", Some(&ws.path("app.sh")))[0].id;
    let message = refusal(move_symbol::to_file(&index, id, &ws.path("lib.sh")));
    assert!(message.contains("already defined in"), "got: {message}");
}

#[test]
fn bash_refuses_to_move_a_variable() {
    let ws = Workspace::new(&[("app.sh", "NAME=x\n"), ("lib.sh", "y=1\n")]);
    let index = ws.index();
    let message = error(move_symbol::to_file(
        &index,
        symbol_id(&index, "NAME", None),
        &ws.path("lib.sh"),
    ));
    assert!(
        message.contains("only a function can be moved"),
        "got: {message}"
    );
}

#[test]
fn bash_carries_the_comment_above_the_function_but_not_the_shebang() {
    let ws = Workspace::new(&[
        (
            "app.sh",
            "#!/usr/bin/env bash\n# Say hello.\ngreet() {\n  echo hi\n}\n",
        ),
        ("lib.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "greet", None),
        &ws.path("lib.sh"),
    )
    .unwrap();
    commit(&plan);
    assert_eq!(ws.read("app.sh"), "#!/usr/bin/env bash\n");
    assert_eq!(
        ws.read("lib.sh"),
        "x=1\n\n# Say hello.\ngreet() {\n  echo hi\n}\n"
    );
}

#[test]
fn bash_leaves_a_recursive_call_alone_because_it_travels_with_the_function() {
    let ws = Workspace::new(&[
        (
            "app.sh",
            "countdown() {\n  [ \"$1\" -eq 0 ] && return\n  countdown $(( $1 - 1 ))\n}\n",
        ),
        ("lib.sh", "x=1\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "countdown", None),
        &ws.path("lib.sh"),
    )
    .unwrap();
    // Nothing outside the function calls it, so nothing gains a `source`.
    assert!(
        plan.imports_added.is_empty(),
        "got: {:?}",
        plan.imports_added
    );
    commit(&plan);
    assert_eq!(ws.read("app.sh"), "");
}

// ===========================================================================
// YAML and Helm: a top-level key's path does not mention its file.
// ===========================================================================

#[test]
fn helm_moves_a_values_key_and_warns_that_only_values_yaml_is_read() {
    let ws = Workspace::new(&[
        ("chart/Chart.yaml", "name: demo\n"),
        ("chart/values.yaml", "image:\n  tag: v1\nreplicas: 2\n"),
        ("chart/values-prod.yaml", "nodeSelector: {}\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "replicas", Some(&ws.path("chart/values.yaml")));
    let plan = move_symbol::to_file(&index, id, &ws.path("chart/values-prod.yaml")).unwrap();

    // The path stays `.Values.replicas`, so nothing whatsoever is repointed.
    assert!(
        plan.imports_added.is_empty(),
        "a key path names no file: {:?}",
        plan.imports_added
    );
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("only values.yaml by default") && w.contains("-f")),
        "got: {:?}",
        plan.warnings
    );

    let changed = commit(&plan);
    assert_eq!(changed.len(), 2, "only the two values files: {changed:?}");
    assert_eq!(ws.read("chart/values.yaml"), "image:\n  tag: v1\n");
    assert_eq!(
        ws.read("chart/values-prod.yaml"),
        "nodeSelector: {}\n\nreplicas: 2\n"
    );
}

#[test]
fn helm_moves_a_whole_subtree() {
    let ws = Workspace::new(&[
        ("chart/Chart.yaml", "name: demo\n"),
        (
            "chart/values.yaml",
            "replicas: 2\nimage:\n  repository: nginx\n  tag: v1\n",
        ),
        ("chart/values-prod.yaml", "nodeSelector: {}\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "image", Some(&ws.path("chart/values.yaml")));
    let plan = move_symbol::to_file(&index, id, &ws.path("chart/values-prod.yaml")).unwrap();
    commit(&plan);

    assert_eq!(ws.read("chart/values.yaml"), "replicas: 2\n");
    assert_eq!(
        ws.read("chart/values-prod.yaml"),
        "nodeSelector: {}\n\nimage:\n  repository: nginx\n  tag: v1\n"
    );

    // The nesting under the key survives byte for byte, so the path of every leaf is
    // what it was.
    let rebuilt = ws.index();
    let tag = symbol_id(&rebuilt, "tag", None);
    assert_eq!(
        rebuilt
            .symbol(tag)
            .and_then(|s| s.container)
            .and_then(|c| rebuilt.symbol(c))
            .map(|s| s.name.as_str()),
        Some("image")
    );
}

#[test]
fn yaml_moves_a_key_between_plain_documents() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "alpha: 1\nbeta:\n  x: 2\n"),
        ("conf/extra.yaml", "gamma: 3\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "beta", None),
        &ws.path("conf/extra.yaml"),
    )
    .unwrap();
    commit(&plan);

    assert_eq!(ws.read("conf/base.yaml"), "alpha: 1\n");
    assert_eq!(ws.read("conf/extra.yaml"), "gamma: 3\n\nbeta:\n  x: 2\n");
    // Not a values.yaml, so nothing about `helm install` applies.
    assert!(
        !plan.warnings.iter().any(|w| w.contains("helm install")),
        "got: {:?}",
        plan.warnings
    );
}

#[test]
fn yaml_warns_when_the_destination_is_in_another_directory() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "alpha: 1\nbeta: 2\n"),
        ("other/extra.yaml", "gamma: 3\n"),
    ]);
    let index = ws.index();
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "beta", None),
        &ws.path("other/extra.yaml"),
    )
    .unwrap();
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("not in the same directory")),
        "got: {:?}",
        plan.warnings
    );
    commit(&plan);
}

#[test]
fn yaml_carries_a_comment_on_the_key_but_leaves_the_file_header() {
    let ws = Workspace::new(&[
        (
            "conf/base.yaml",
            "# What this file is.\nalpha: 1\n\n# Why beta exists.\nbeta: 2\n",
        ),
        ("conf/extra.yaml", "gamma: 3\n"),
    ]);
    let index = ws.index();

    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "beta", None),
        &ws.path("conf/extra.yaml"),
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("conf/extra.yaml")),
        "gamma: 3\n\n# Why beta exists.\nbeta: 2\n"
    );
    assert_eq!(
        applied(&plan, &ws.path("conf/base.yaml")),
        "# What this file is.\nalpha: 1\n\n"
    );

    // The header comment opens the file, so it describes the file rather than the
    // first key, and stays where it is.
    let plan = move_symbol::to_file(
        &index,
        symbol_id(&index, "alpha", None),
        &ws.path("conf/extra.yaml"),
    )
    .unwrap();
    assert_eq!(
        applied(&plan, &ws.path("conf/extra.yaml")),
        "gamma: 3\n\nalpha: 1\n"
    );
    assert_eq!(
        applied(&plan, &ws.path("conf/base.yaml")),
        "# What this file is.\n\n# Why beta exists.\nbeta: 2\n"
    );
}

#[test]
fn yaml_refuses_a_nested_key() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "image:\n  tag: v1\n"),
        ("conf/extra.yaml", "gamma: 3\n"),
    ]);
    let index = ws.index();
    let message = error(move_symbol::to_file(
        &index,
        symbol_id(&index, "tag", None),
        &ws.path("conf/extra.yaml"),
    ));
    assert!(message.contains("nested under `image`"), "got: {message}");
    assert!(message.contains("only a top-level key"), "got: {message}");
}

#[test]
fn yaml_refuses_a_key_the_destination_already_has() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "beta: 2\n"),
        ("conf/extra.yaml", "beta: 3\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "beta", Some(&ws.path("conf/base.yaml")));
    let message = refusal(move_symbol::to_file(&index, id, &ws.path("conf/extra.yaml")));
    assert!(message.contains("already defined in"), "got: {message}");
}

#[test]
fn yaml_refuses_a_destination_holding_several_documents() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "beta: 2\n"),
        ("conf/extra.yaml", "gamma: 3\n---\ndelta: 4\n"),
    ]);
    let index = ws.index();
    let id = symbol_id(&index, "beta", Some(&ws.path("conf/base.yaml")));
    let message = error(move_symbol::to_file(&index, id, &ws.path("conf/extra.yaml")));
    assert!(message.contains("more than one document"), "got: {message}");
}

#[test]
fn yaml_refuses_an_anchor() {
    let ws = Workspace::new(&[
        ("conf/base.yaml", "defaults: &shared\n  a: 1\nuse: *shared\n"),
        ("conf/extra.yaml", "gamma: 3\n"),
    ]);
    let index = ws.index();
    let message = error(move_symbol::to_file(
        &index,
        symbol_of_kind(&index, "shared", SymbolKind::Anchor),
        &ws.path("conf/extra.yaml"),
    ));
    assert!(message.contains("An anchor is resolved"), "got: {message}");
}

// ===========================================================================
// What stays refused, and what each language offers.
// ===========================================================================

#[test]
fn html_and_xml_stay_refused_by_name() {
    for (source, destination, code, other) in [
        (
            "a.html",
            "b.html",
            "<div id=\"thing\">x</div>\n",
            "<p>y</p>\n",
        ),
        (
            "a.xml",
            "b.xml",
            "<root><item id=\"thing\"/></root>\n",
            "<root/>\n",
        ),
    ] {
        let ws = Workspace::new(&[(source, code), (destination, other)]);
        let index = ws.index();
        let found = index.find_symbols("thing", None);
        assert!(!found.is_empty(), "{source}: nothing named `thing` extracted");
        let message = refusal(move_symbol::to_file(
            &index,
            found[0].id,
            &ws.path(destination),
        ));
        assert!(
            message.contains("is not supported for"),
            "{source} got: {message}"
        );
        assert!(
            message.contains("no name that another document imports"),
            "{source} got: {message}"
        );
    }
}

#[test]
fn movable_lists_what_the_new_languages_can_move() {
    let ws = Workspace::new(&[
        (
            "a.zig",
            "const std = @import(\"std\");\npub fn thing() void {}\npub const Point = struct {\n    x: i32,\n};\n",
        ),
        ("run.sh", "NAME=x\ngreet() {\n  echo hi\n}\n"),
        ("conf.yaml", "top:\n  nested: 1\n"),
        ("page.html", "<div id=\"thing\">x</div>\n"),
    ]);
    let index = ws.index();

    let names = |file: &str| -> Vec<String> {
        move_symbol::movable(&index, &ws.path(file))
            .iter()
            .map(|id| index.symbol(*id).unwrap().name.clone())
            .collect()
    };

    let zig = names("a.zig");
    assert!(zig.contains(&"thing".to_string()), "got {zig:?}");
    assert!(zig.contains(&"Point".to_string()), "got {zig:?}");
    assert!(
        !zig.contains(&"x".to_string()),
        "a struct field is not movable: {zig:?}"
    );

    assert_eq!(
        names("run.sh"),
        ["greet"],
        "only a function; a variable's value depends on when it ran"
    );
    assert_eq!(
        names("conf.yaml"),
        ["top"],
        "only the top-level key; a nested key's path is its address"
    );
    assert!(names("page.html").is_empty());
}
