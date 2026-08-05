//! Python's entry points, which are not spelled the way the other five spell theirs.
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
