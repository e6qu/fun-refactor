use fun_refactor::analysis::call_graph::CallGraph;
use fun_refactor::analysis::entrypoints::Entrypoints;
use fun_refactor::analysis::impact;
use fun_refactor::cache::Cache;
use fun_refactor::index::Index;
use fun_refactor::refactor::delete;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        "pub fn leaf() {}\npub fn middle() { leaf(); }\npub fn top() { middle(); }\n",
    )
    .unwrap();
    tmp
}

fn signature(graph: &CallGraph) -> Vec<(u32, u32, usize, String, String)> {
    graph
        .edges()
        .into_iter()
        .map(|(from, to, edge)| {
            (
                from.0,
                to.0,
                edge.offset,
                edge.confidence.as_str().to_string(),
                edge.origin.as_str().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_cached_graph_answers_like_a_fresh_graph_and_an_edit_replaces_it() {
    let tmp = workspace();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();

    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let cold_index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let cold = CallGraph::build_cached(&cold_index, &cache);

    let warm_index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let warm = CallGraph::build_cached(&warm_index, &cache);
    assert_eq!(signature(&cold), signature(&warm));
    assert_eq!(cold.unresolved, warm.unresolved);
    assert_eq!(cold.file_scope, warm.file_scope);

    std::fs::write(
        tmp.path().join("lib.rs"),
        "pub fn leaf() {}\npub fn middle() { leaf(); }\npub fn top() { middle(); }\npub fn extra() { top(); }\n",
    )
    .unwrap();
    let edited_scan = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let edited_index = Index::build_with_cache(&edited_scan, Some(&cache)).unwrap();
    let edited = CallGraph::build_cached(&edited_index, &cache);
    assert_eq!(edited.node_count(), 4);
    assert_eq!(edited.edge_count(), 3);
}

#[test]
fn callers_that_already_have_the_graph_keep_the_existing_answers() {
    let tmp = workspace();
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(cache_dir.path()).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_with_cache(&scanned, Some(&cache)).unwrap();
    let graph = CallGraph::build_cached(&index, &cache);
    let entrypoints = Entrypoints::none();
    assert_eq!(
        delete::find_unused(&index, &entrypoints),
        delete::find_unused_with_graph(&index, &entrypoints, &graph)
    );

    let leaf = index.find_symbols("leaf", None)[0].id;
    let direct = impact::analyse(&index, leaf, 2).unwrap();
    let reused = impact::analyse_with_graph(&index, leaf, 2, &graph).unwrap();
    assert_eq!(direct.by_kind(), reused.by_kind());
    assert_eq!(direct.by_confidence(), reused.by_confidence());
    assert_eq!(
        direct.callers_beyond_the_depth_limit,
        reused.callers_beyond_the_depth_limit
    );
}
