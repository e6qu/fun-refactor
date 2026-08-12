//! The cache must be invisible: an index built from cached facts has to be
//! indistinguishable from one built by parsing every file. These tests compare the
//! two directly, because a cache that is merely *fast* is worthless if it is wrong.

use fun_refactor::cache::Cache;
use fun_refactor::index::Index;
use fun_refactor::model::FactGap;
use fun_refactor::scan::{scan, ScanOptions, ScanResult};
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, ScanResult) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, scanned)
}

/// A comparable summary of everything an index knows.
fn fingerprint(index: &Index) -> Vec<String> {
    let mut out: Vec<String> = index
        .symbols
        .iter()
        .map(|s| {
            format!(
                "sym {} {} {} {:?} {} {}",
                s.name,
                s.kind.as_str(),
                s.exported,
                s.qualifier,
                s.name_span,
                s.file.file_name().unwrap().to_str().unwrap()
            )
        })
        .collect();
    out.extend(index.references.iter().map(|r| {
        format!(
            "ref {} {} {:?} {} {}",
            r.name,
            r.confidence.as_str(),
            r.kind,
            r.span,
            r.file.file_name().unwrap().to_str().unwrap()
        )
    }));
    out.sort();
    out
}

const FILES: &[(&str, &str)] = &[
    ("a.rs", "pub fn alpha() {}\nfn caller() { alpha(); }\n"),
    (
        "b.py",
        "def beta():\n    return 1\n\ndef main():\n    return beta()\n",
    ),
    ("styles.css", ".card { color: red; }\n"),
    ("index.html", "<div class=\"card\" id=\"root\"></div>\n"),
    ("main.tf", "variable \"region\" {\n  default = \"a\"\n}\n"),
    ("broken.rs", "fn oops( {\n"),
];

#[test]
fn a_cached_index_matches_an_uncached_one() {
    let (_tmp, scanned) = workspace(FILES);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    let uncached = Index::build_with_cache(&scanned, None).unwrap();
    // First pass populates, second reads back.
    let _populate = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let cached = Index::build_with_cache(&scanned, Some(&cache)).unwrap();

    assert_eq!(fingerprint(&uncached), fingerprint(&cached));
    assert_eq!(uncached.file_count(), cached.file_count());
}

#[test]
fn the_second_pass_is_all_hits() {
    let (_tmp, scanned) = workspace(FILES);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let hits_after_first = cache.stats().hits;

    Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let hits_after_second = cache.stats().hits;

    assert_eq!(hits_after_first, 0, "the first pass cannot hit anything");
    assert_eq!(
        hits_after_second - hits_after_first,
        scanned.files.len(),
        "every file should be served from cache on the second pass"
    );
}

#[test]
fn editing_a_file_invalidates_only_that_entry() {
    let (tmp, _) = workspace(FILES);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    Index::build_with_cache(&scanned, Some(&cache)).unwrap();

    // Change one file's content.
    std::fs::write(tmp.path().join("a.rs"), "pub fn renamed() {}\n").unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();

    let before_hits = cache.stats().hits;
    let index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let after_hits = cache.stats().hits;

    // Every file but the edited one is still a hit.
    assert_eq!(after_hits - before_hits, scanned.files.len() - 1);
    // And the index reflects the new content, not the cached old facts.
    assert!(
        index.find_symbols("alpha", None).is_empty(),
        "the edited-away symbol must not come back from cache"
    );
    assert_eq!(index.find_symbols("renamed", None).len(), 1);
}

#[test]
fn a_files_parse_error_state_survives_a_round_trip() {
    // The flag is carried with the facts so a hit need not reparse to learn it.
    let (tmp, scanned) = workspace(FILES);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let cached = Index::build_with_cache(&scanned, Some(&cache)).unwrap();

    let broken = cached
        .file(&tmp.path().join("broken.rs"))
        .expect("the broken file is indexed");
    assert_eq!(
        broken.gaps,
        [FactGap::SyntaxErrors],
        "a file with syntax errors must still be reported as such from cache"
    );
    let fine = cached.file(&tmp.path().join("a.rs")).unwrap();
    assert!(fine.gaps.is_empty());
}

#[test]
fn two_files_with_identical_content_share_one_entry() {
    let (_tmp, scanned) = workspace(&[
        ("one.rs", "pub fn same() {}\n"),
        ("two.rs", "pub fn same() {}\n"),
    ]);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    // Files are extracted in parallel, so within one run both may finish before
    // either writes; the shared entry pays off from the next run onwards.
    Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let before = cache.stats().hits;
    let index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let after = cache.stats().hits;
    assert_eq!(after - before, 2, "both files served from one entry");

    // Each file must still own its own symbol, pointing at its own path.
    let symbols = index.find_symbols("same", None);
    assert_eq!(symbols.len(), 2);
    let files: Vec<&PathBuf> = symbols.iter().map(|s| &s.file).collect();
    assert_ne!(
        files[0], files[1],
        "facts must be rewritten to their own file"
    );
}

#[test]
fn indexing_is_deterministic_despite_running_in_parallel() {
    // Symbol ids are assigned by position, so results are collected in scan order
    // and merged serially; otherwise ids would depend on thread timing.
    let (_tmp, scanned) = workspace(FILES);
    let first = Index::build_with_cache(&scanned, None).unwrap();
    for _ in 0..4 {
        let again = Index::build_with_cache(&scanned, None).unwrap();
        assert_eq!(fingerprint(&first), fingerprint(&again));
        assert_eq!(
            first.symbols.iter().map(|s| s.id).collect::<Vec<_>>(),
            again.symbols.iter().map(|s| s.id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn cross_language_resolution_survives_caching() {
    // Resolution happens after facts are loaded, so a cached CSS class must still
    // be found by an HTML attribute.
    let (_tmp, scanned) = workspace(FILES);
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let cached = Index::build_with_cache(&scanned, Some(&cache)).unwrap();

    let card = cached.find_symbols("card", None);
    assert_eq!(card.len(), 1);
    let refs = cached.references_to(card[0].id);
    assert_eq!(
        refs.len(),
        1,
        "the HTML class should still resolve: {refs:?}"
    );
}

#[test]
fn entries_do_not_store_a_path_per_item() {
    // Every symbol and reference in a file shares one path. Storing it per item made
    // entries several times larger than the source they describe, so it is dropped on
    // write and restored on read. This test guards the size, not just the behaviour.
    let big = (0..200u32)
        .map(|i| format!("pub fn f{i}() {{ f{}(); }}\n", i.saturating_sub(1)))
        .collect::<String>();
    let (_tmp, scanned) = workspace(&[("big.rs", &big)]);

    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();
    Index::build_with_cache(&scanned, Some(&cache)).unwrap();

    let stored = cache.size_bytes() as usize;
    assert!(
        stored < big.len() * 6,
        "entry is {stored} bytes for {} bytes of source, which suggests paths are \
         being stored per item again",
        big.len()
    );

    // And the paths still come back correctly.
    let index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    assert!(index
        .symbols
        .iter()
        .all(|s| s.file.file_name().unwrap() == "big.rs"));
    assert!(index
        .references
        .iter()
        .all(|r| r.file.file_name().unwrap() == "big.rs"));
}

#[test]
fn a_missing_cache_directory_is_not_an_error() {
    // Caching is an optimisation; losing it must only cost time.
    let (_tmp, scanned) = workspace(FILES);
    let index = Index::build_with_cache(&scanned, None).unwrap();
    assert!(index.file_count() > 0);
}

#[test]
fn the_cache_namespace_includes_the_extractor_that_produced_the_facts() {
    // The cache is keyed by file content and by the query set. That is only correct
    // while "the extractor" is a constant, and it is not. Adding a field to
    // `Reference` changes what a cached fact means, while `#[serde(default)]` lets
    // yesterday's entry deserialize cleanly into today's struct. The result is a
    // cache that looks healthy and answers wrongly; it cost an afternoon of bisecting
    // a test failure that was not in the code being bisected.
    //
    // build.rs hashes the sources that define extraction into the namespace, so an
    // edit to any of them makes every stale entry unreachable and not wrong.
    let fingerprint = env!("FUN_REFACTOR_EXTRACTOR_FINGERPRINT");
    assert_eq!(
        fingerprint.len(),
        16,
        "expected a 64-bit hex fingerprint, got {fingerprint:?}"
    );
    assert!(
        fingerprint.chars().all(|c| c.is_ascii_hexdigit()),
        "got {fingerprint:?}"
    );

    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(dir.path()).expect("a cache under a writable directory");
    let namespace = cache.location().display().to_string();
    assert!(
        namespace.contains(fingerprint),
        "the extractor fingerprint must be part of the namespace: {namespace}"
    );
}
