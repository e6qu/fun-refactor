//! A stylesheet's module system: what `@use` and `@import` make visible where.
//!
//! Sass splits a codebase into files and gives each one a namespace. `@use "theme"` makes
//! its variables reachable as `theme.$brand`, `as t` renames that namespace, `as *` drops
//! it, and the older `@import` drops it too. A partial is written `_theme.scss` and named
//! `"theme"`, so no import ever spells the file it names.
//!
//! Before this, a `$variable` declared in one file and used in another resolved to
//! nothing in either syntax. `fr rename` rewrote the declaration and left every use site
//! behind, reporting them as occurrences it could not place. These tests hold both
//! halves: what the module system makes visible resolves, and what it does not stays
//! unresolved.

use fun_refactor::index::Index;
use fun_refactor::model::{Confidence, SymbolId};
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

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

/// Every reference to `name`, as (file name, whether it resolved to `name`'s declaration).
fn uses(index: &Index, name: &str) -> Vec<(String, bool, Confidence)> {
    let declaration = symbol(index, name);
    index
        .references
        .iter()
        .filter(|r| r.name == name)
        .map(|r| {
            (
                r.file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string(),
                r.target == Some(declaration),
                r.confidence,
            )
        })
        .collect()
}

#[test]
fn a_namespaced_use_reaches_the_file_it_names() {
    for (partial, stylesheet, uses_it) in [
        (
            "_theme.sass",
            "style.sass",
            "@use \"theme\" as t\n\n.a\n  color: t.$brand\n",
        ),
        (
            "_theme.scss",
            "style.scss",
            "@use \"theme\" as t;\n\n.a { color: t.$brand; }\n",
        ),
    ] {
        let (_tmp, _root, index) = workspace(&[(partial, "$brand: #fff\n"), (stylesheet, uses_it)]);
        let found = uses(&index, "$brand");
        assert_eq!(
            found.len(),
            1,
            "the use site is a reference in {stylesheet}: {found:?}"
        );
        assert!(
            found[0].1,
            "`t.$brand` reaches the declaration `@use` names in {stylesheet}: {found:?}"
        );
    }
}

#[test]
fn a_partial_is_named_without_its_underscore() {
    // `@use "theme"` names `_theme.scss`, which is the file every Sass codebase writes.
    let (_tmp, root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n"),
        (
            "style.scss",
            "@use \"theme\" as t;\n\n.a { color: t.$brand; }\n",
        ),
    ]);
    let import = index
        .file(&root.join("style.scss"))
        .expect("the file is indexed")
        .imports
        .first()
        .expect("the `@use` is an import")
        .clone();
    assert_eq!(import.path, "theme");
    assert_eq!(
        index.resolve_import_path(&root.join("style.scss"), &import.path),
        Some(root.join("_theme.scss")),
    );
}

#[test]
fn the_default_namespace_is_the_file_it_came_from() {
    // `@use "theme"` with no `as` binds the namespace `theme`.
    let (_tmp, _root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n"),
        (
            "style.scss",
            "@use \"theme\";\n\n.a { color: theme.$brand; }\n",
        ),
    ]);
    let found = uses(&index, "$brand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].1, "`theme.$brand` reaches it: {found:?}");
}

#[test]
fn a_bare_name_reaches_what_import_and_a_star_bring_in() {
    // The older `@import` drops everything into one scope, and `@use ... as *` says the
    // same thing in the newer syntax. A bare `$brand` is the whole name in both.
    for opening in ["@import \"theme\";", "@use \"theme\" as *;"] {
        let (_tmp, _root, index) = workspace(&[
            ("_theme.scss", "$brand: #fff;\n"),
            (
                "style.scss",
                &format!("{opening}\n\n.a {{ color: $brand; }}\n"),
            ),
        ]);
        let found = uses(&index, "$brand");
        assert_eq!(found.len(), 1, "{opening}: {found:?}");
        assert!(found[0].1, "{opening} brings `$brand` in: {found:?}");
    }
}

#[test]
fn a_namespace_nothing_bound_reaches_nothing() {
    // `@use "theme"` binds the namespace `theme` and nothing else. A bare `$brand` is an
    // undefined variable in Sass, and the tool says so instead of guessing the file.
    let (_tmp, _root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n"),
        ("style.scss", "@use \"theme\";\n\n.a { color: $brand; }\n"),
    ]);
    let found = uses(&index, "$brand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        !found[0].1,
        "a bare `$brand` is not what `@use \"theme\"` made visible: {found:?}"
    );
}

#[test]
fn a_mixin_and_a_function_cross_the_same_way() {
    let (_tmp, _root, index) = workspace(&[
        (
            "_theme.sass",
            "@mixin card($fg)\n  color: $fg\n\n@function double($n)\n  @return $n * 2\n",
        ),
        (
            "style.sass",
            "@use \"theme\" as t\n\n.a\n  @include t.card(red)\n  width: t.double(2)\n",
        ),
    ]);
    for name in ["card", "double"] {
        let found = uses(&index, name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        assert!(found[0].1, "`t.{name}` reaches the declaration: {found:?}");
    }
}

#[test]
fn a_rename_crosses_the_file_boundary_it_resolved_across() {
    // The point of resolving it: a rename rewrites the use site, instead of reporting it
    // as an occurrence it could not place.
    let (_tmp, root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n"),
        (
            "style.scss",
            "@use \"theme\" as t;\n\n.a { color: t.$brand; }\n",
        ),
    ]);
    let plan = fun_refactor::refactor::rename::plan(&index, symbol(&index, "$brand"), "$ink")
        .expect("a rename plan");
    let files: Vec<String> = plan
        .edits
        .paths()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert!(files.contains(&"_theme.scss".to_string()), "{files:?}");
    assert!(files.contains(&"style.scss".to_string()), "{files:?}");
}

#[test]
fn a_forward_carries_a_name_one_file_further() {
    // `@forward "theme"` re-exports what that file declares. A third file that `@use`s
    // the forwarder reaches them through the forwarder's namespace, which is one hop
    // more than the declaring file's own.
    let (_tmp, _root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n"),
        ("_index.scss", "@forward \"theme\";\n"),
        (
            "style.scss",
            "@use \"index\" as t;\n\n.a { color: t.$brand; }\n",
        ),
    ]);
    let found = uses(&index, "$brand");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].1,
        "the name reaches the file that declares it, through the one that forwards it: \
         {found:?}"
    );
}

#[test]
fn a_builtin_module_names_no_file_and_says_nothing_false() {
    // `@use "sass:math"` names a module the compiler carries, not a file in this
    // workspace. Nothing resolves to it, and nothing here pretends otherwise.
    let (_tmp, root, index) = workspace(&[(
        "style.scss",
        "@use \"sass:math\";\n\n.a { width: math.div(1, 2); }\n",
    )]);
    let import = index
        .file(&root.join("style.scss"))
        .expect("the file is indexed")
        .imports
        .first()
        .expect("the `@use` is an import")
        .clone();
    assert_eq!(import.path, "sass:math");
    assert_eq!(
        index.resolve_import_path(&root.join("style.scss"), &import.path),
        None,
        "no file in this workspace is `sass:math`"
    );
    let calls: Vec<_> = index
        .references
        .iter()
        .filter(|r| r.name == "div")
        .collect();
    assert_eq!(calls.len(), 1, "the call is still a reference");
    assert_eq!(calls[0].target, None, "and it reaches nothing here");
}

#[test]
fn a_variable_used_only_from_another_file_is_not_dead() {
    // What resolution is for. Every use site sat in another file, so the declaration
    // looked like a name nothing reads, and `fr delete` would have taken it away.
    let (_tmp, _root, index) = workspace(&[
        ("_theme.scss", "$brand: #fff;\n$unread: #000;\n"),
        (
            "style.scss",
            "@use \"theme\" as t;\n\n.a { color: t.$brand; }\n",
        ),
    ]);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::detect(&index).expect("entry points");
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !dead.contains(&"$brand"),
        "it is read through the namespace `@use` bound: {dead:?}"
    );
    assert!(
        dead.contains(&"$unread"),
        "and one nothing reads is still listed: {dead:?}"
    );
}

/// A CSS module's selectors are its own, the way a chart's values are.
///
/// `.primary` in `Button.module.css` compiles to a name nobody writes, and the
/// component reaches it through the object its import binds. So the `.primary` a
/// neighbouring module declares is a different class. Grouped by name across
/// files, renaming one component's class rewrote every other component's
/// stylesheet that happened to use the word.
#[test]
fn a_css_module_keeps_its_selectors_to_itself() {
    let (_dir, root, index) = workspace(&[
        ("Button.module.css", ".primary {\n  color: red;\n}\n"),
        ("Other.module.css", ".primary {\n  color: green;\n}\n"),
        (
            "Button.tsx",
            "import styles from \"./Button.module.css\";\n\n\
             export function Button() {\n  \
             return <button className={styles.primary} />;\n}\n",
        ),
    ]);

    let mine: Vec<SymbolId> = index
        .symbols
        .iter()
        .filter(|s| s.name == "primary" && s.file.ends_with("Button.module.css"))
        .map(|s| s.id)
        .collect();
    let [mine] = mine.as_slice() else {
        panic!("one `.primary` in Button.module.css");
    };
    let group = index.definition_group(*mine);
    assert_eq!(group.len(), 1, "a module's class is its own entity");
    assert!(index
        .symbol(group[0])
        .is_some_and(|s| s.file.ends_with("Button.module.css")));

    // And the component's use reaches the module it imported, not the other one.
    let plan = fun_refactor::refactor::rename::plan(&index, *mine, "lead").expect("a rename");
    let files: Vec<String> = plan
        .edits
        .paths()
        .map(|p| p.strip_prefix(&root).unwrap_or(p).display().to_string())
        .collect();
    assert_eq!(
        files,
        vec!["Button.module.css".to_string(), "Button.tsx".to_string()],
        "the module and its component, and nothing else"
    );
}

/// A plain stylesheet is the opposite: two files declaring `.banner` style the
/// same elements, so the rename has to take both.
#[test]
fn a_plain_stylesheets_classes_stay_global() {
    let (_dir, _root, index) = workspace(&[
        ("a.css", ".banner {\n  color: red;\n}\n"),
        ("b.css", ".banner {\n  font-weight: bold;\n}\n"),
    ]);
    let first = index
        .symbols
        .iter()
        .find(|s| s.name == "banner")
        .expect("a `.banner`");
    assert_eq!(
        index.definition_group(first.id).len(),
        2,
        "both declarations are the one class"
    );
}
