//! Organize imports: removing what nothing names, sorting what stays, and refusing
//! everything the tool cannot do without inventing syntax.

use fun_refactor::{
    edit::apply_to_string,
    index::Index,
    refactor::{imports, Refusal, WarningKind},
    scan::{scan, ScanOptions},
};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

/// Organize one file's imports and return the resulting text plus the plan.
fn organize(files: &[(&str, &str)], target: &str) -> (imports::ImportsPlan, String, PathBuf) {
    let (tmp, index) = workspace(files);
    let path = tmp.path().join(target);
    let plan = imports::plan(&index, &path).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let updated = match plan.edits.edits_for(&path) {
        Some(edits) => apply_to_string(&original, edits).unwrap(),
        None => original,
    };
    // The temp dir must outlive the read above; the plan keeps only paths.
    drop(tmp);
    (plan, updated, path)
}

#[test]
fn removes_an_import_nothing_names() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use std::fmt;\n\nfn main() {\n    println!(\"hi\");\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.removed.len(), 1, "got {:?}", plan.removed);
    assert_eq!(plan.removed[0].path, "std::fmt");
    assert_eq!(plan.removed[0].bindings, vec!["fmt".to_string()]);
    assert_eq!(plan.removed[0].line, 1);
    assert_eq!(updated, "\nfn main() {\n    println!(\"hi\");\n}\n");
}

#[test]
fn keeps_an_import_something_names() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use std::fmt;\n\nfn main() {\n    let _: fmt::Result;\n}\n",
        )],
        "a.rs",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(plan.edits.is_empty(), "nothing to do: {:?}", plan.edits);
    assert_eq!(
        updated,
        "use std::fmt;\n\nfn main() {\n    let _: fmt::Result;\n}\n"
    );
}

#[test]
fn a_statement_binding_several_names_survives_if_any_one_is_used() {
    // `HashSet` is unused, but removing it would mean regenerating the brace list.
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use std::collections::{HashMap, HashSet};\n\nfn main() {\n    let _: HashMap<u8, u8>;\n}\n",
        )],
        "a.rs",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(updated.contains("use std::collections::{HashMap, HashSet};"));
}

#[test]
fn a_grouped_plain_import_prunes_its_unused_names() {
    // `import os, sys` binds two modules with one statement.
    let (plan, updated, _) = organize(
        &[("a.py", "import os, sys\n\nprint(os.path.sep)\n")],
        "a.py",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["sys"],
        "got {:?}",
        plan.removed
    );
    assert_eq!(plan.removed[0].bindings, vec!["sys".to_string()]);
    assert_eq!(updated, "import os\n\nprint(os.path.sep)\n");
}

#[test]
fn a_grouped_plain_import_nothing_uses_is_removed_whole() {
    let (plan, updated, _) = organize(&[("a.py", "import os, sys\n\nprint(1)\n")], "a.py");

    assert_eq!(plan.removed.len(), 1, "got {:?}", plan.removed);
    assert_eq!(
        plan.removed[0].bindings,
        vec!["os".to_string(), "sys".to_string()]
    );
    assert_eq!(updated, "\nprint(1)\n");
}

#[test]
fn a_grouped_plain_import_keeps_an_aliased_module_something_names() {
    // The alias is the binding, so the clause that carries it survives whole.
    let (plan, updated, _) = organize(
        &[("a.py", "import os, sys as system\n\nprint(system.path)\n")],
        "a.py",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["os"]
    );
    assert_eq!(updated, "import sys as system\n\nprint(system.path)\n");
}

#[test]
fn a_grouped_plain_import_narrows_around_a_kept_submodule() {
    // `app.handlers` may exist for its registration side effects, so it stays and the report names it.
    let (plan, updated, _) = organize(
        &[("a.py", "import app.handlers, sys\n\nprint(1)\n")],
        "a.py",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["sys"]
    );
    assert_eq!(updated, "import app.handlers\n\nprint(1)\n");
    assert!(
        plan.warnings.iter().any(|w| w.detail.contains("submodule")),
        "the kept clause must be explained: {:?}",
        plan.warnings
    );
}

#[test]
fn an_unused_namespace_import_is_pruned_like_a_named_one() {
    // Extraction records `import * as fs` as a glob for resolution's sake, and it binds exactly
    // one name.
    let (plan, updated, _) = organize(
        &[(
            "a.ts",
            "import * as fs from \"fs\";\n\nexport const x = 1;\n",
        )],
        "a.ts",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["fs"]
    );
    assert_eq!(updated, "\nexport const x = 1;\n");
    assert!(
        !plan
            .warnings
            .iter()
            .any(|w| w.detail.contains("glob import")),
        "an enumerable binding is not the glob case: {:?}",
        plan.warnings
    );
}

#[test]
fn a_used_namespace_import_is_kept() {
    let (plan, updated, _) = organize(
        &[(
            "a.ts",
            "import * as path from \"path\";\n\nexport const x = path.sep;\n",
        )],
        "a.ts",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(updated.contains("import * as path from \"path\";"));
}

#[test]
fn a_glob_import_is_never_removed_and_says_why() {
    let (plan, updated, _) = organize(
        &[("a.rs", "use zed::*;\nuse std::fmt;\n\nfn main() {}\n")],
        "a.rs",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["std::fmt"],
        "only the provably-unused one goes"
    );
    assert!(updated.contains("use zed::*;"), "got:\n{updated}");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.kind == WarningKind::WeaklyResolved && w.detail.contains("glob import")),
        "the glob must be reported: {:?}",
        plan.warnings
    );
}

#[test]
fn a_python_star_import_is_never_removed() {
    let (plan, updated, _) = organize(
        &[("a.py", "from foo import *\nimport sys\n\nprint(1)\n")],
        "a.py",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["sys"]
    );
    assert_eq!(updated, "from foo import *\n\nprint(1)\n");
}

#[test]
fn a_side_effect_import_binds_nothing_and_is_never_removed() {
    let (plan, updated, _) = organize(
        &[(
            "a.ts",
            "import './polyfills';\nimport { unused } from './m';\n\nexport const x = 1;\n",
        )],
        "a.ts",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["./m"]
    );
    assert_eq!(updated, "import './polyfills';\n\nexport const x = 1;\n");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.detail.contains("side effects")),
        "got {:?}",
        plan.warnings
    );
}

#[test]
fn a_go_blank_import_binds_nothing_and_is_never_removed() {
    let (plan, updated, _) = organize(
        &[(
            "a.go",
            "package main\n\nimport (\n\t_ \"embed\"\n\t\"strings\"\n)\n\nfunc main() {}\n",
        )],
        "a.go",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["strings"],
    );
    assert_eq!(
        updated,
        "package main\n\nimport (\n\t_ \"embed\"\n)\n\nfunc main() {}\n"
    );
}

#[test]
fn a_typescript_named_import_used_in_the_file_is_kept_and_the_rest_go() {
    let (plan, updated, _) = organize(
        &[(
            "a.ts",
            "import { b, a } from './m';\nimport def from 'other';\n\nexport function go() { return a; }\n",
        )],
        "a.ts",
    );

    // `./m` binds two names and the body reads one, so the statement stays and `b` goes.
    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["other", "./m"]
    );
    assert_eq!(
        updated,
        "import { a } from './m';\n\nexport function go() { return a; }\n"
    );
}

#[test]
fn a_rust_trait_imported_only_for_its_methods_is_kept() {
    // The call site spells `write_str` and never `Write`, so name-based liveness
    // sees an unused import.
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use std::fmt::Write;\n\nfn f(s: &mut String) {\n    let _ = s.write_str(\"x\");\n}\n",
        )],
        "a.rs",
    );

    assert!(
        plan.removed.is_empty(),
        "a possible trait import has to stay: {:?}",
        plan.removed
    );
    assert!(updated.contains("use std::fmt::Write;"), "got:\n{updated}");
    assert!(
        plan.warnings.iter().any(|w| w.detail.contains("trait")),
        "the decision must be explained: {:?}",
        plan.warnings
    );
}

#[test]
fn a_rust_inner_attribute_stays_before_the_sorted_import_block() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "#![allow(dead_code)]\n\nuse zebra::Thing;\nuse apple::Other;\n\nfn f() {\n    g(Thing);\n    g(Other);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.sorted_blocks, 1, "got {:?}", plan.edits);
    assert!(
        updated.starts_with("#![allow(dead_code)]\n\nuse apple::Other;"),
        "got:\n{updated}"
    );
}

#[test]
fn an_external_rust_trait_stays_despite_an_unrelated_workspace_type() {
    // A local type shares the external trait's spelling.
    let (plan, updated, _) = organize(
        &[
            (
                "a.rs",
                "use anyhow::Context;\n\nfn f(result: Result<(), ()>) {\n    let _ = result.context(\"while working\");\n}\n",
            ),
            ("elsewhere.rs", "struct Context;\n"),
        ],
        "a.rs",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(updated.contains("use anyhow::Context;"), "got:\n{updated}");
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.detail.contains("trait")),
        "the protected import needs an explanation: {:?}.",
        plan.warnings
    );
}

#[test]
fn an_unused_workspace_concrete_rust_import_still_goes() {
    let (plan, updated, _) = organize(
        &[
            ("a.rs", "use crate::model::Unused;\n\nfn f() {}\n"),
            ("model.rs", "pub struct Unused;\n"),
        ],
        "a.rs",
    );

    assert_eq!(plan.removed.len(), 1, "got {:?}", plan.removed);
    assert!(
        !updated.contains("use crate::model::Unused;"),
        "got:\n{updated}"
    );
}

#[test]
fn a_rust_public_reexport_stays_without_a_workspace_reader() {
    let (plan, updated, _) = organize(
        &[
            ("mod.rs", "pub use crate::api::Public;\n"),
            ("api.rs", "pub struct Public;\n"),
        ],
        "mod.rs",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(
        updated.contains("pub use crate::api::Public;"),
        "got:\n{updated}"
    );
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.detail.contains("outside this workspace")),
        "the public surface needs an explanation: {:?}.",
        plan.warnings
    );
}

#[test]
fn a_typescript_reexport_stays_without_a_workspace_reader() {
    let (plan, updated, _) = organize(
        &[
            ("barrel.ts", "export { publicName } from './api';\n"),
            ("api.ts", "export const publicName = 1;\n"),
        ],
        "barrel.ts",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(
        updated.contains("export { publicName } from './api';"),
        "got:\n{updated}"
    );
}

#[test]
fn a_rust_import_a_child_reaches_through_super_stays() {
    let (plan, updated, _) = organize(
        &[
            ("parent/mod.rs", "use crate::dep::shared;\n"),
            ("parent/child.rs", "fn f() { super::shared(); }\n"),
            ("dep.rs", "pub fn shared() {}\n"),
        ],
        "parent/mod.rs",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert!(
        updated.contains("use crate::dep::shared;"),
        "got:\n{updated}"
    );
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.detail.contains("child module")),
        "the child binding needs an explanation: {:?}.",
        plan.warnings
    );
}

#[test]
fn sorts_a_block_by_path() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use zebra::Thing;\nuse apple::Other;\n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.sorted_blocks, 1);
    assert_eq!(
        updated,
        "use apple::Other;\nuse zebra::Thing;\n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n"
    );
}

#[test]
fn sorting_reorders_bytes_and_never_rewrites_them() {
    // Odd spacing, a trailing comment-free tail and a non-canonical `;` position all have to
    // come through untouched: the sort moves whole statements.
    let (_plan, updated, _) = organize(
        &[(
            "a.rs",
            "use   zebra::Thing  ;   \nuse apple::Other;\n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(
        updated,
        "use apple::Other;\nuse   zebra::Thing  ;   \n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n"
    );
}

#[test]
fn sorting_is_stable_for_equal_paths() {
    // Two statements with the same path keep their written order.
    let (plan, updated, _) = organize(
        &[(
            "a.py",
            "from m import zeta\nfrom m import alpha\nimport aaa\n\nprint(zeta, alpha, aaa)\n",
        )],
        "a.py",
    );

    assert!(plan.removed.is_empty(), "got {:?}", plan.removed);
    assert_eq!(
        updated,
        "import aaa\nfrom m import zeta\nfrom m import alpha\n\nprint(zeta, alpha, aaa)\n"
    );
}

#[test]
fn an_already_sorted_block_produces_no_edit() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use apple::A;\nuse zebra::Z;\n\nfn main() {\n    f(A);\n    f(Z);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.sorted_blocks, 0);
    assert!(plan.edits.is_empty());
    assert_eq!(
        updated,
        "use apple::A;\nuse zebra::Z;\n\nfn main() {\n    f(A);\n    f(Z);\n}\n"
    );
}

#[test]
fn a_blank_line_ends_a_block_and_nothing_moves_across_it() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use zzz::A;\nuse bbb::B;\n\nuse yyy::C;\nuse aaa::D;\n\nfn main() {\n    f(A);\n    f(B);\n    f(C);\n    f(D);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.sorted_blocks, 2, "each group sorted within itself");
    assert_eq!(
        updated,
        "use bbb::B;\nuse zzz::A;\n\nuse aaa::D;\nuse yyy::C;\n\nfn main() {\n    f(A);\n    f(B);\n    f(C);\n    f(D);\n}\n",
        "aaa::D must stay in the second group"
    );
}

#[test]
fn a_comment_between_imports_ends_a_block() {
    // Reordering across the comment would silently reassign what it documents.
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use zzz::A;\n// the second group\nuse aaa::B;\n\nfn main() {\n    f(A);\n    f(B);\n}\n",
        )],
        "a.rs",
    );

    assert_eq!(plan.sorted_blocks, 0);
    assert!(plan.edits.is_empty());
    assert_eq!(
        updated,
        "use zzz::A;\n// the second group\nuse aaa::B;\n\nfn main() {\n    f(A);\n    f(B);\n}\n"
    );
}

#[test]
fn two_imports_on_one_line_are_left_alone_and_reported() {
    let (plan, updated, _) = organize(
        &[(
            "a.rs",
            "use zzz::A; use aaa::B;\n\nfn main() {\n    f(A);\n    f(B);\n}\n",
        )],
        "a.rs",
    );

    assert!(plan.edits.is_empty(), "got {:?}", plan.edits);
    assert!(updated.starts_with("use zzz::A; use aaa::B;"));
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.detail.contains("shares its line")),
        "got {:?}",
        plan.warnings
    );
}

#[test]
fn sorts_a_go_import_block_inside_its_parentheses() {
    let (plan, updated, _) = organize(
        &[(
            "a.go",
            "package main\n\nimport (\n\t\"os\"\n\t\"fmt\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
        )],
        "a.go",
    );

    assert_eq!(plan.sorted_blocks, 1);
    assert_eq!(
        updated,
        "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
        "the parentheses and the tabs are outside the reordered region"
    );
}

#[test]
fn sorts_and_prunes_typescript_in_one_pass() {
    let (plan, updated, _) = organize(
        &[(
            "a.ts",
            "import { z } from './zzz';\nimport { dead } from './dead';\nimport { a } from './aaa';\n\nexport const v = [a, z];\n",
        )],
        "a.ts",
    );

    assert_eq!(
        plan.removed
            .iter()
            .map(|r| r.path.as_str())
            .collect::<Vec<_>>(),
        vec!["./dead"]
    );
    assert_eq!(
        updated,
        "import { a } from './aaa';\nimport { z } from './zzz';\n\nexport const v = [a, z];\n"
    );
}

#[test]
fn removing_every_import_in_a_block_removes_its_lines_entirely() {
    let (plan, updated, _) = organize(&[("a.py", "import os\nimport sys\n\nprint(1)\n")], "a.py");

    assert_eq!(plan.removed.len(), 2);
    assert_eq!(updated, "\nprint(1)\n");
}

#[test]
fn zig_import_consts_sort_by_path() {
    let (plan, updated, _) = organize(
        &[(
            "a.zig",
            "const zzz = @import(\"zzz\");\nconst aaa = @import(\"aaa\");\n\npub fn f() void {\n    _ = zzz;\n    _ = aaa;\n}\n",
        )],
        "a.zig",
    );

    assert_eq!(plan.sorted_blocks, 1);
    assert_eq!(
        updated,
        "const aaa = @import(\"aaa\");\nconst zzz = @import(\"zzz\");\n\npub fn f() void {\n    _ = zzz;\n    _ = aaa;\n}\n"
    );
}

#[test]
fn languages_without_import_declarations_refuse() {
    let (tmp, index) = workspace(&[
        (
            "style.css",
            "@import \"a.css\";\n@import \"b.css\";\n\n.btn { color: red; }\n",
        ),
        ("page.html", "<div id=\"x\"></div>\n"),
        ("config.yaml", "key: value\n"),
        ("main.tf", "variable \"x\" {}\n"),
        ("README.md", "# Title\n"),
        ("doc.xml", "<root id=\"a\"/>\n"),
        ("run.sh", "echo hi\n"),
    ]);

    for file in [
        "style.css",
        "page.html",
        "config.yaml",
        "main.tf",
        "README.md",
        "doc.xml",
        "run.sh",
    ] {
        let error = imports::plan(&index, &tmp.path().join(file)).unwrap_err();
        let refusal = error
            .downcast_ref::<Refusal>()
            .unwrap_or_else(|| panic!("{file}: expected a Refusal, got {error}"));
        assert!(
            matches!(refusal, Refusal::Unsupported { .. }),
            "{file}: {refusal}"
        );
    }
}

#[test]
fn a_css_import_is_left_alone_because_its_order_is_semantic() {
    // CSS `@import` is a real import, but sorting it would change the cascade and `@import`
    // must precede every other rule.
    let (tmp, index) = workspace(&[(
        "style.css",
        "@import \"z.css\";\n@import \"a.css\";\n\n.btn { color: red; }\n",
    )]);
    let error = imports::plan(&index, &tmp.path().join("style.css")).unwrap_err();
    assert!(
        error.to_string().contains("not supported for css"),
        "{error}"
    );
}

#[test]
fn a_file_with_syntax_errors_refuses_rather_than_guess_at_uses() {
    let (tmp, index) = workspace(&[("a.rs", "use std::fmt;\n\nfn broken( {\n")]);
    let error = imports::plan(&index, &tmp.path().join("a.rs")).unwrap_err();
    assert!(error.to_string().contains("syntax errors"), "{error}");
}

#[test]
fn a_file_outside_the_index_is_an_error() {
    let (tmp, index) = workspace(&[("a.rs", "fn main() {}\n")]);
    let error = imports::plan(&index, &tmp.path().join("nope.rs")).unwrap_err();
    assert!(error.to_string().contains("not in the index"), "{error}");
}

#[test]
fn a_file_with_no_imports_is_a_clean_no_op() {
    let (plan, updated, _) = organize(&[("a.rs", "fn main() {}\n")], "a.rs");
    assert!(plan.edits.is_empty());
    assert!(plan.removed.is_empty());
    assert_eq!(plan.sorted_blocks, 0);
    assert_eq!(updated, "fn main() {}\n");
}

#[test]
fn the_plan_reports_the_file_it_planned_for() {
    let (plan, _, path) = organize(&[("a.rs", "fn main() {}\n")], "a.rs");
    assert_eq!(plan.file, path);
    assert_eq!(
        plan.file.parent().map(Path::to_path_buf),
        path.parent().map(Path::to_path_buf)
    );
}

#[test]
fn the_edits_survive_the_engines_reparse_check() {
    let (tmp, index) = workspace(&[(
        "a.rs",
        // `dead::gone` is lower-case, so it is not trait-shaped and is removable;
        // the two used imports are reordered around its removal.
        "use zebra::Thing;\nuse dead::gone;\nuse apple::Other;\n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n",
    )]);
    let path = tmp.path().join("a.rs");
    let plan = imports::plan(&index, &path).unwrap();

    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("organized imports must still parse");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].updated,
        "use apple::Other;\nuse zebra::Thing;\n\nfn main() {\n    f(Thing);\n    f(Other);\n}\n"
    );
}

#[test]
fn the_go_edits_survive_the_engines_reparse_check() {
    let (tmp, index) = workspace(&[(
        "a.go",
        "package main\n\nimport (\n\t\"os\"\n\t\"strings\"\n\t\"fmt\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
    )]);
    let path = tmp.path().join("a.go");
    let plan = imports::plan(&index, &path).unwrap();

    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("organized imports must still parse");
    assert_eq!(
        outcomes[0].updated,
        "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n"
    );
}

#[test]
fn consecutive_standalone_go_imports_are_sorted() {
    // Go records `import "os"` as the spec alone, so the `import` keyword sits outside the
    // span.
    let (_plan, updated, _path) = organize(
        &[(
            "m.go",
            "package main\n\nimport \"os\"\nimport \"fmt\"\nimport \"bytes\"\n\nfunc main() { _ = os.Args; _ = fmt.Sprint; _ = bytes.MinRead }\n",
        )],
        "m.go",
    );

    let imports: Vec<&str> = updated
        .lines()
        .filter(|l| l.starts_with("import "))
        .collect();
    assert_eq!(
        imports,
        vec!["import \"bytes\"", "import \"fmt\"", "import \"os\""],
        "got {imports:?}"
    );
}

#[test]
fn css_is_refused_with_the_reason_rather_than_a_bare_no() {
    // Not an unimplemented cell: reordering CSS imports changes which rules win.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("a.css");
    std::fs::write(
        &path,
        "@import \"b.css\";\n@import \"a.css\";\n.x { color: red; }\n",
    )
    .unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();

    let err = imports::plan(&index, &path).unwrap_err().to_string();
    assert!(
        err.contains("cascade"),
        "the refusal must explain itself: {err}"
    );
}
