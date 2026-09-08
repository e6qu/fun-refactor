use fun_refactor::{
    analysis::entrypoints::{Catalog, EntryKind},
    index::Index,
    scan::ScanOptions,
};
use std::{collections::BTreeMap, fs};

#[test]
fn captured_test_rules_agree_with_legacy_detection_after_source_removal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let sources: BTreeMap<_, _> = [
        (root.join("app.rs"), "#[test]\nfn check() {}\n".to_owned()),
        (
            root.join("app.py"),
            "@pytest.fixture\ndef shared():\n    return 1\n\ndef test_app():\n    pass\n"
                .to_owned(),
        ),
    ]
    .into();
    for (file, source) in &sources {
        fs::write(file, source).unwrap();
    }
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    let catalog = Catalog::builtin().unwrap();
    let expected: Vec<_> = catalog
        .detect(&index)
        .into_iter()
        .filter(|e| e.kind == EntryKind::Test)
        .collect();
    assert_eq!(expected.len(), 3);
    for file in sources.keys() {
        fs::remove_file(file).unwrap();
    }
    let actual = catalog.tests_in_snapshot(&index, &sources);
    assert_eq!(actual.entries, expected);
    assert!(actual.gaps.is_empty());
}

#[test]
fn missing_mismatched_and_broken_test_sources_report_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut sources: BTreeMap<_, _> = [
        (
            root.join("missing.rs"),
            "#[test]\nfn check() {}\n".to_owned(),
        ),
        (
            root.join("changed.py"),
            "def test_app():\n    pass\n".to_owned(),
        ),
        (
            root.join("broken.py"),
            "def test_broken():\n    print(\n".to_owned(),
        ),
    ]
    .into();
    for (file, source) in &sources {
        fs::write(file, source).unwrap();
    }
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    sources.remove(&root.join("missing.rs"));
    sources.insert(root.join("changed.py"), "print('different')\n".into());
    sources.insert(
        root.join("unindexed.py"),
        "def test_extra():\n    pass\n".into(),
    );
    let actual = Catalog::builtin()
        .unwrap()
        .tests_in_snapshot(&index, &sources);
    assert!(actual.entries.is_empty());
    assert_eq!(actual.gaps.len(), 3);
    assert!(actual
        .gaps
        .iter()
        .any(|(_, reason)| reason.contains("absent")));
    assert!(actual
        .gaps
        .iter()
        .any(|(_, reason)| reason.contains("content hash")));
    assert!(actual
        .gaps
        .iter()
        .any(|(_, reason)| reason.contains("parser")));
}
