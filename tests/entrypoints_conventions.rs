//! Entry points a framework calls and the source never does.
//!
//! Python's are not spelled the way the other five spell theirs, and Next.js's newest
//! convention is not spelled the way its older ones are. Both are the same problem: a
//! name rule cannot find something that has no name to match.
//!
//! Every other catalog can say `name: main`, because every other language here agrees
//! that a program starts in a function called `main`. Python's starts in a *statement*,
//! and the function it calls can be named anything — so the rule that worked everywhere
//! else reported nothing at all for an ordinary script.

use fun_refactor::analysis::entrypoints::{Catalog, EntryKind};
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn entry_kinds(files: &[(&str, &str)]) -> Vec<(String, EntryKind)> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let catalog = Catalog::builtin().expect("the built-in catalogs");
    catalog
        .detect(&index)
        .into_iter()
        .filter_map(|e| index.symbol(e.symbol).map(|s| (s.name.clone(), e.kind)))
        .collect()
}

#[test]
fn the_main_guard_names_the_entry_point() {
    let found = entry_kinds(&[(
        "run.py",
        "def cli():\n    return 1\n\nif __name__ == \"__main__\":\n    cli()\n",
    )]);
    assert!(
        found.contains(&("cli".to_string(), EntryKind::CliMain)),
        "got {found:?}"
    );
}

#[test]
fn the_main_guard_reaches_through_sys_exit() {
    // `sys.exit(run())` is the other half of the idiom, and the call it wraps is still
    // where the program starts.
    let found = entry_kinds(&[(
        "run.py",
        "import sys\n\ndef run():\n    return 0\n\nif __name__ == \"__main__\":\n    \
         sys.exit(run())\n",
    )]);
    assert!(
        found.contains(&("run".to_string(), EntryKind::CliMain)),
        "got {found:?}"
    );
}

#[test]
fn a_function_the_guard_does_not_call_is_not_an_entry_point() {
    // Only what the guard calls directly. Anything further is reachability, which the
    // call graph answers; folding it in here would tag half a program.
    let found = entry_kinds(&[(
        "run.py",
        "def helper():\n    return 1\n\ndef cli():\n    return helper()\n\n\
         if __name__ == \"__main__\":\n    cli()\n",
    )]);
    assert!(
        !found.iter().any(|(name, _)| name == "helper"),
        "got {found:?}"
    );
}

#[test]
fn a_module_with_no_guard_gains_nothing() {
    let found = entry_kinds(&[("lib.py", "def cli():\n    return 1\n")]);
    assert!(found.is_empty(), "got {found:?}");
}

#[test]
fn a_shared_fixture_is_an_entry_point() {
    // Nothing calls a fixture by name — pytest injects it by matching the parameter.
    // In `conftest.py`, where the shared ones live, neither the file nor the function
    // is named `test_*`, so no rule matched it at all.
    let found = entry_kinds(&[(
        "conftest.py",
        "import pytest\n\n@pytest.fixture\ndef shared():\n    return 3\n",
    )]);
    assert!(
        found.contains(&("shared".to_string(), EntryKind::Test)),
        "got {found:?}"
    );
}

#[test]
fn a_parameterised_fixture_is_an_entry_point() {
    let found = entry_kinds(&[(
        "conftest.py",
        "import pytest\n\n@pytest.fixture(scope=\"module\")\ndef db():\n    return 2\n",
    )]);
    assert!(
        found.contains(&("db".to_string(), EntryKind::Test)),
        "got {found:?}"
    );
}

#[test]
fn unittest_fixtures_are_entry_points() {
    // `unittest` calls these itself, once per test, and no source refers to them.
    let found = entry_kinds(&[(
        "tc.py",
        "import unittest\n\nclass ThingTest(unittest.TestCase):\n    \
         def setUp(self):\n        self.value = 1\n\n    \
         def tearDown(self):\n        self.value = 0\n",
    )]);
    for name in ["setUp", "tearDown"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::Test)),
            "{name} missing from {found:?}"
        );
    }
}

// ------------------------------------------------ and the same case in TypeScript

#[test]
fn a_next_js_server_action_is_an_entry_point() {
    // A `"use server"` file exports functions the framework makes reachable over the
    // network, called by nothing in the source. The catalogue already covered Next.js's
    // *filename* conventions — `page.tsx`, `route.ts` — and this one is not a filename:
    // `components/cart/actions.ts` is an ordinary name. Found in `vercel/commerce`,
    // where five live network endpoints were reported as having no detected use.
    let found = entry_kinds(&[(
        "actions.ts",
        "\"use server\";\n\nexport async function addItem(id: string) {\n  return id;\n}\n\n\
         export async function removeItem(id: string) {\n  return id;\n}\n",
    )]);
    for name in ["addItem", "removeItem"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "{name} missing from {found:?}"
        );
    }
}

#[test]
fn the_directive_inside_one_function_marks_only_that_one() {
    // Both forms are real: at the top of a file it marks every export, at the top of a
    // body it marks that body. Treating the second as the first would call an ordinary
    // helper a network endpoint.
    let found = entry_kinds(&[(
        "mixed.ts",
        "export async function ordinary(id: string) {\n  return id;\n}\n\n\
         export async function action(id: string) {\n  \"use server\";\n  return id;\n}\n",
    )]);
    assert!(
        found.contains(&("action".to_string(), EntryKind::HttpRoute)),
        "got {found:?}"
    );
    assert!(
        !found.iter().any(|(name, _)| name == "ordinary"),
        "an ordinary function beside an action was called an endpoint: {found:?}"
    );
}

#[test]
fn the_words_in_a_comment_are_not_a_directive() {
    // A directive is the first statement and it is quoted. Anything else that mentions
    // it is prose.
    let found = entry_kinds(&[(
        "notes.ts",
        "// use server: this file does not, despite the comment\n\n\
         export async function helper(id: string) {\n  return id;\n}\n",
    )]);
    assert!(
        !found.iter().any(|(name, _)| name == "helper"),
        "a comment was read as a directive: {found:?}"
    );
}
