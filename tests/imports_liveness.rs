//! Import liveness, one guard per language, and the two halves of the unused-symbol
//! report that a resolver alone cannot get right.
//!
//! Name-based liveness answers "does anything in the file spell this name?" That is the
//! whole truth only for a value or type that has to be written where it is used. Every
//! test here is a language construct that uses an import *without* spelling its name,
//! paired with the case that looks the same and really is dead. The asymmetry is the
//! point: removing a live import breaks a build silently, whereas keeping a dead one
//! leaves a line of noise, so every guard errs towards keeping and says why.

use fun_refactor::{
    index::Index,
    refactor::{delete, imports},
    scan::{scan, ScanOptions},
};

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

/// The paths organize-imports would drop from `target`, plus every warning detail.
fn outcome(files: &[(&str, &str)], target: &str) -> (Vec<String>, Vec<String>) {
    let (tmp, index) = workspace(files);
    let plan = imports::plan(&index, &tmp.path().join(target)).unwrap();
    let removed = plan.removed.iter().map(|r| r.path.clone()).collect();
    let warnings = plan.warnings.iter().map(|w| w.detail.clone()).collect();
    (removed, warnings)
}

/// Assert `path` was kept and that a warning says why, naming `because`.
fn kept_because(files: &[(&str, &str)], target: &str, path: &str, because: &str) {
    let (removed, warnings) = outcome(files, target);
    assert!(
        !removed.iter().any(|r| r == path),
        "'{path}' must be kept, got removed: {removed:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains(because)),
        "keeping '{path}' must be explained by a warning naming {because:?}: {warnings:?}"
    );
}

fn removed_paths(files: &[(&str, &str)], target: &str) -> Vec<String> {
    outcome(files, target).0
}

// -------------------------------------------------------------------------- Rust

#[test]
fn rust_an_import_nothing_names_goes() {
    assert_eq!(
        removed_paths(
            &[("a.rs", "use std::fmt;\n\nfn main() {\n    let _ = 1;\n}\n")],
            "a.rs"
        ),
        vec!["std::fmt".to_string()]
    );
}

#[test]
fn rust_a_trait_used_through_its_methods_is_kept() {
    // `Write` is never spelled at the call site — only `write_str` is.
    kept_because(
        &[(
            "a.rs",
            "use std::fmt::Write;\n\nfn f(s: &mut String) {\n    let _ = s.write_str(\"x\");\n}\n",
        )],
        "a.rs",
        "std::fmt::Write",
        "trait",
    );
}

// ------------------------------------------------------------------------ Python

#[test]
fn python_a_plain_unused_import_goes() {
    // A single-segment `import sys` binds `sys` and nothing else; there is no
    // submodule whose import could have had a side effect.
    assert_eq!(
        removed_paths(&[("a.py", "import sys\n\nprint(1)\n")], "a.py"),
        vec!["sys".to_string()]
    );
}

#[test]
fn python_a_submodule_import_is_kept_for_its_registration_side_effect() {
    // `import myapp.handlers` binds only `myapp`. Writing the submodule out is how
    // decorator-registration modules get loaded, and the name is never mentioned
    // again by design.
    kept_because(
        &[
            ("a.py", "import myapp.handlers\n\nprint(1)\n"),
            ("myapp/__init__.py", ""),
            ("myapp/handlers.py", "def h():\n    pass\n"),
        ],
        "a.py",
        "myapp.handlers",
        "side effect",
    );
}

#[test]
fn python_a_future_import_is_never_removed() {
    // `annotations` is not a name anyone spells; the statement changes how the file
    // is compiled. Removing it can change what every annotation in the file means.
    kept_because(
        &[(
            "a.py",
            "from __future__ import annotations\n\ndef f(x: int) -> int:\n    return x\n",
        )],
        "a.py",
        "__future__",
        "__future__",
    );
}

#[test]
fn python_a_name_re_exported_through_dunder_all_is_kept() {
    // Importing it *is* the use: `__all__` republishes it as this module's surface.
    kept_because(
        &[
            ("a.py", "from m import thing\n\n__all__ = [\"thing\"]\n"),
            ("m.py", "def thing():\n    pass\n"),
        ],
        "a.py",
        "m",
        "__all__",
    );
}

#[test]
fn python_dunder_all_naming_something_else_does_not_save_an_import() {
    // The negative half: `__all__` is consulted, not treated as blanket immunity.
    assert_eq!(
        removed_paths(
            &[
                ("a.py", "from m import thing\n\ndef other():\n    pass\n\n__all__ = [\"other\"]\n"),
                ("m.py", "def thing():\n    pass\n"),
            ],
            "a.py"
        ),
        vec!["m".to_string()]
    );
}

// -------------------------------------------------------------------- TypeScript

#[test]
fn typescript_an_import_nothing_names_goes() {
    assert_eq!(
        removed_paths(
            &[(
                "a.ts",
                "import { dead } from './m';\n\nexport const x = 1;\n"
            )],
            "a.ts"
        ),
        vec!["./m".to_string()]
    );
}

#[test]
fn typescript_a_side_effect_import_binds_nothing_and_is_kept() {
    // Already handled by binding nothing rather than by a guard; asserted here so the
    // zero-binding path stays covered next to the guards that surround it.
    kept_because(
        &[(
            "a.ts",
            "import './polyfill';\n\nexport const x = 1;\n",
        )],
        "a.ts",
        "./polyfill",
        "side effects",
    );
}

#[test]
fn typescript_a_type_used_only_in_a_jsdoc_comment_is_kept() {
    kept_because(
        &[(
            "a.ts",
            "import { Foo } from './m';\n\n/** @type {Foo} */\nexport const x = 1;\n",
        )],
        "a.ts",
        "./m",
        "JSDoc",
    );
}

#[test]
fn typescript_a_comment_merely_mentioning_the_name_does_not_keep_it() {
    // The braces are what make a JSDoc tag a type annotation. Prose about `Foo` is
    // not a use of `Foo`, and treating it as one would disable removal outright.
    assert_eq!(
        removed_paths(
            &[(
                "a.ts",
                "import { Foo } from './m';\n\n// Foo used to live here\nexport const x = 1;\n",
            )],
            "a.ts"
        ),
        vec!["./m".to_string()]
    );
}

#[test]
fn typescript_a_type_only_import_is_kept() {
    // Every use of a type-only import is in a type position, and the fact queries do
    // not capture all of them (`typeof Foo` is one they miss), so the whole form is
    // held back rather than removed on incomplete evidence.
    kept_because(
        &[(
            "a.ts",
            "import type { Foo } from './m';\n\nexport const x = 1;\n",
        )],
        "a.ts",
        "./m",
        "type-only",
    );
}

#[test]
fn typescript_an_inline_type_specifier_marks_the_statement_type_only() {
    // `import { type Foo }` carries the modifier on the specifier, where the grammar
    // exposes it only as an anonymous token.
    kept_because(
        &[(
            "a.ts",
            "import { type Foo } from './m';\n\nexport const x = 1;\n",
        )],
        "a.ts",
        "./m",
        "type-only",
    );
}

#[test]
fn typescript_a_value_import_used_only_under_typeof_is_kept() {
    // `typeof Foo` in a type position is a `type_query`, which no `@reference` capture
    // reports, so name-based liveness sees an unused import and would break the build.
    kept_because(
        &[(
            "a.ts",
            "import { Foo } from './m';\n\nexport type B = typeof Foo;\n",
        )],
        "a.ts",
        "./m",
        "typeof Foo",
    );
}

#[test]
fn typescript_a_jsx_pragma_names_the_factory_every_element_compiles_to() {
    kept_because(
        &[(
            "a.tsx",
            "/** @jsx h */\nimport { h } from 'preact';\n\nexport const x = 1;\n",
        )],
        "a.tsx",
        "preact",
        "JSX pragma",
    );
}

#[test]
fn typescript_without_the_pragma_the_same_import_goes() {
    assert_eq!(
        removed_paths(
            &[(
                "a.tsx",
                "import { h } from 'preact';\n\nexport const x = 1;\n"
            )],
            "a.tsx"
        ),
        vec!["preact".to_string()]
    );
}

// ---------------------------------------------------------------------------- Go

#[test]
fn go_an_import_nothing_names_goes() {
    assert_eq!(
        removed_paths(
            &[("a.go", "package main\n\nimport \"strings\"\n\nfunc main() {}\n")],
            "a.go"
        ),
        vec!["strings".to_string()]
    );
}

#[test]
fn go_a_blank_import_binds_nothing_and_is_kept() {
    kept_because(
        &[(
            "a.go",
            "package main\n\nimport (\n\t_ \"embed\"\n\t\"strings\"\n)\n\nfunc main() {}\n",
        )],
        "a.go",
        "embed",
        "side effects",
    );
}

#[test]
fn go_a_package_named_differently_from_its_path_is_not_mistaken_for_unused() {
    // `gopkg.in/yaml.v2` declares `package yaml`. Guessing the binding from the last
    // path segment gives `v2`, which nothing names — and removing the import would
    // break a build that uses `yaml.Marshal` on the next line.
    assert!(
        removed_paths(
            &[(
                "a.go",
                "package main\n\nimport \"gopkg.in/yaml.v2\"\n\nfunc main() {\n\t_, _ = yaml.Marshal(1)\n}\n",
            )],
            "a.go"
        )
        .is_empty(),
        "a used package must survive a path its name cannot be read off"
    );
}

#[test]
fn go_an_unreadable_package_clause_holds_the_import_back() {
    // Nothing here names `yaml` either, and the package is not in the scan, so the
    // honest answer is that the binding is unknown rather than unused.
    kept_because(
        &[(
            "a.go",
            "package main\n\nimport \"gopkg.in/yaml.v2\"\n\nfunc main() {}\n",
        )],
        "a.go",
        "gopkg.in/yaml.v2",
        "package clause",
    );
}

#[test]
fn go_a_package_clause_the_scan_can_see_is_used_instead_of_the_path() {
    // The directory is `helper/`, the package is `helper`, and the import path ends
    // in `helper`: the binding is a fact here, not a guess, so `helper.Do()` keeps it.
    assert!(
        removed_paths(
            &[
                (
                    "main.go",
                    "package main\n\nimport \"example.com/app/helper\"\n\nfunc main() {\n\thelper.Do()\n}\n",
                ),
                ("helper/helper.go", "package helper\n\nfunc Do() {}\n"),
            ],
            "main.go"
        )
        .is_empty(),
        "the package clause in the scan says the binding is `helper`"
    );
}

#[test]
fn go_a_visible_package_clause_that_nothing_names_still_goes() {
    // The counterpart: once the binding is known rather than guessed, an unused
    // import has nothing left to hide behind.
    assert_eq!(
        removed_paths(
            &[
                (
                    "main.go",
                    "package main\n\nimport \"example.com/app/helper\"\n\nfunc main() {}\n",
                ),
                ("helper/helper.go", "package helper\n\nfunc Do() {}\n"),
            ],
            "main.go"
        ),
        vec!["example.com/app/helper".to_string()]
    );
}

// -------------------------------------------------------------------------- Zig

#[test]
fn zig_needs_no_guard_and_removes_what_nothing_names() {
    // `@import` yields an ordinary container-level `const`, and every use of it spells
    // that const's name. Zig has no construct that brings an imported name into scope
    // invisibly, so name-based liveness is exact here.
    let (removed, warnings) = outcome(
        &[(
            "a.zig",
            "const dead = @import(\"dead.zig\");\n\npub fn f() void {}\n",
        )],
        "a.zig",
    );
    assert_eq!(removed, vec!["dead.zig".to_string()]);
    assert!(
        warnings.is_empty(),
        "nothing to hold back means nothing to warn about: {warnings:?}"
    );
}

#[test]
fn zig_keeps_an_import_its_const_name_reaches() {
    let (removed, warnings) = outcome(
        &[(
            "a.zig",
            "const std = @import(\"std\");\n\npub fn f() void {\n    std.debug.assert(true);\n}\n",
        )],
        "a.zig",
    );
    assert!(removed.is_empty(), "got {removed:?}");
    assert!(warnings.is_empty(), "got {warnings:?}");
}

// ------------------------------------------------- unused symbols (B5, find_unused)

fn only_symbol(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
    found[0].id
}

#[test]
fn a_symbol_named_in_a_string_literal_is_not_reported_unused() {
    // The only trace a handler table keyed by name leaves is the name in a string.
    // Reporting `on_event` as dead invites deleting live code, so it is left off.
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "fn on_event() {}\nfn main() {\n    dispatch(\"on_event\");\n}\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(
        !unused.contains(&only_symbol(&index, "on_event")),
        "a name spelled in a string may be reached by reflection: {unused:?}"
    );
}

#[test]
fn a_string_in_another_file_still_counts() {
    // Reflection crosses files: the table is rarely in the same file as the handler.
    let (_tmp, index) = workspace(&[
        ("a.rs", "fn on_event() {}\nfn main() {}\n"),
        ("b.py", "HANDLERS = {\"on_event\": None}\n"),
    ]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(
        !unused.contains(&only_symbol(&index, "on_event")),
        "got {unused:?}"
    );
}

#[test]
fn a_symbol_no_string_mentions_is_still_reported() {
    // The negative half: the string check must not swallow the whole report.
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "fn orphan() {}\nfn main() {\n    dispatch(\"something_else\");\n}\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(
        unused.contains(&only_symbol(&index, "orphan")),
        "got {unused:?}"
    );
}

#[test]
fn a_mutually_recursive_dead_group_is_reported() {
    // `ping` and `pong` each have an incoming reference, so the per-symbol check
    // clears both. Nothing outside the pair references either and no entry point
    // reaches them, so the component is dead as a whole.
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "fn ping() { pong(); }\nfn pong() { ping(); }\nfn main() {}\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(unused.contains(&only_symbol(&index, "ping")), "got {unused:?}");
    assert!(unused.contains(&only_symbol(&index, "pong")), "got {unused:?}");
}

#[test]
fn a_mutually_recursive_group_one_entry_point_reaches_is_not_reported() {
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "fn ping() { pong(); }\nfn pong() { ping(); }\nfn main() { ping(); }\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    assert!(!unused.contains(&only_symbol(&index, "ping")), "got {unused:?}");
    assert!(!unused.contains(&only_symbol(&index, "pong")), "got {unused:?}");
}

#[test]
fn a_longer_dead_cycle_is_reported_as_a_group() {
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "fn a() { b(); }\nfn b() { c(); }\nfn c() { a(); }\nfn main() {}\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    for name in ["a", "b", "c"] {
        assert!(
            unused.contains(&only_symbol(&index, name)),
            "{name} is in a cycle nothing outside reaches: {unused:?}"
        );
    }
}

#[test]
fn dynamic_dispatch_with_no_string_to_go_on_remains_a_false_positive() {
    // The part of B5 that stays open. The `hello` that runs is the impl's, reached
    // through a `&dyn Greet`, so the only resolved edge from `main` goes to the trait
    // method and nothing leads to the implementation. No string names it either.
    // Nothing in the workspace distinguishes this from dead code, and inventing a
    // distinction would be guessing.
    let (_tmp, index) = workspace(&[(
        "a.rs",
        "trait Greet { fn hello(&self); }\nstruct Greeter;\nimpl Greet for Greeter {\n    fn hello(&self) {}\n}\nfn main() {\n    let g: &dyn Greet = &Greeter;\n    g.hello();\n}\n",
    )]);
    let unused = delete::find_unused(&index, &[only_symbol(&index, "main")]);
    let names: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        names.contains(&"hello"),
        "a method only ever reached through a trait object still looks dead: {names:?}"
    );
}
