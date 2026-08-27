//! A warm command answers from the resolution snapshot, and answers the same.

use fun_refactor::cache::Cache;
use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};

fn targets(index: &Index) -> Vec<(Option<u32>, &'static str)> {
    index
        .references
        .iter()
        .map(|r| (r.target.map(|t| t.0), r.confidence.as_str()))
        .collect()
}

#[test]
fn a_snapshot_answers_identically_and_an_edit_invalidates_it() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("a.rs"),
        "pub fn helper() -> i64 {\n    7\n}\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.rs"),
        "pub fn go() -> i64 {\n    crate::a::helper()\n}\n",
    )
    .unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).expect("a cache");

    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let cold = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let warm = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    assert_eq!(
        targets(&cold),
        targets(&warm),
        "the snapshot answers what resolution answered"
    );

    // An edit anywhere changes the workspace key, so the stale snapshot
    // cannot be applied to the new shape.
    std::fs::write(
        tmp.path().join("a.rs"),
        "pub fn helper() -> i64 {\n    8\n}\n\npub fn extra() -> i64 {\n    helper()\n}\n",
    )
    .unwrap();
    let rescanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let edited = Index::build_with_cache(&rescanned, Some(&cache)).unwrap();
    assert!(
        edited.references.len() > warm.references.len(),
        "the new reference resolved, so the snapshot was not replayed stale"
    );
    let helper = edited
        .symbols
        .iter()
        .find(|s| s.name == "helper")
        .expect("helper");
    assert_eq!(
        edited
            .references
            .iter()
            .filter(|r| r.target == Some(helper.id))
            .count(),
        2,
        "both calls resolve after the edit"
    );
}
