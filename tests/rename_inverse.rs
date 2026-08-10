//! Renaming a symbol and renaming it back leaves the workspace where it started.
//!
//! A rename touches every file that references the symbol, and the edits are byte
//! splices instead of a reformat, so the inverse ought to restore the tree exactly —
//! including the files it decided not to touch. Anything else means the first rename
//! wrote something the second could not find, or found something the first did not.
//!
//! Verified over `helm/helm` before writing this: 14 uniquely-named Go callables, all 14
//! byte-identical after `A -> ZzTmpName -> A`. This pins the same property on a workspace
//! that spans languages, where a cross-language reference gives it more to get wrong.

use fun_refactor::edit::{commit, plan as plan_edits, Validation};
use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::ScanOptions;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every file's bytes, so the comparison is the whole tree and not one file.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("a readable directory")
            .flatten()
        {
            let path = entry.path();
            match path.is_dir() {
                true => stack.push(path),
                false => {
                    let bytes = std::fs::read(&path).expect("a readable file");
                    out.insert(path, bytes);
                }
            }
        }
    }
    out
}

fn rename_in_place(root: &Path, from: &str, to: &str) {
    let index = Index::build(root, &ScanOptions::default()).expect("an index");
    let found = index.find_symbols(from, None);
    assert_eq!(found.len(), 1, "expected one `{from}`, got {}", found.len());
    let plan = rename::plan(&index, found[0].id, to).unwrap_or_else(|e| panic!("{from}: {e}"));
    let outcomes = plan_edits(&plan.edits, Validation::ReparseStrict).expect("the edits validate");
    commit(&outcomes).expect("the edits are written");
}

#[test]
fn renaming_a_symbol_and_back_restores_every_file() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let root = tmp.path();
    for (name, content) in [
        (
            "src/core.ts",
            "export function computeTotal(n: number): number {\n  return n * 2;\n}\n",
        ),
        (
            "src/app.ts",
            "import { computeTotal } from \"./core\";\n\n\
             export function run(): number {\n  return computeTotal(21);\n}\n",
        ),
        (
            "src/unrelated.ts",
            "export function computeTotalElsewhere(): number {\n  return 1;\n}\n",
        ),
        ("notes.md", "`computeTotal` is described here.\n"),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, content).expect("the file");
    }

    let before = snapshot(root);
    rename_in_place(root, "computeTotal", "ZzTmpName");

    let midpoint = snapshot(root);
    assert_ne!(before, midpoint, "the first rename should change something");

    rename_in_place(root, "ZzTmpName", "computeTotal");
    let after = snapshot(root);

    let differing: Vec<_> = before
        .keys()
        .filter(|p| before.get(*p) != after.get(*p))
        .map(|p| p.strip_prefix(root).unwrap_or(p).to_path_buf())
        .collect();
    assert!(
        differing.is_empty(),
        "renaming back should restore the tree, but these differ: {differing:?}"
    );
}

/// The same, where the reference crosses a language boundary.
#[test]
fn a_cross_language_rename_and_back_restores_every_file() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let root = tmp.path();
    for (name, content) in [
        (
            "styles.css",
            ".btn-primary { color: red; }\n.other { color: blue; }\n",
        ),
        ("index.html", "<button class=\"btn-primary\">Go</button>\n"),
        (
            "src/App.tsx",
            "export const App = () => <button className=\"btn-primary\">Go</button>;\n",
        ),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, content).expect("the file");
    }

    let before = snapshot(root);
    rename_in_place(root, "btn-primary", "btn-secondary");
    assert_ne!(
        before,
        snapshot(root),
        "the first rename should change something"
    );
    rename_in_place(root, "btn-secondary", "btn-primary");

    let after = snapshot(root);
    let differing: Vec<_> = before
        .keys()
        .filter(|p| before.get(*p) != after.get(*p))
        .map(|p| p.strip_prefix(root).unwrap_or(p).to_path_buf())
        .collect();
    assert!(differing.is_empty(), "these differ: {differing:?}");
}
