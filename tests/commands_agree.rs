//! The commands that read answer overlapping questions from one index.
//!
//! `fr refs`, `fr usages`, `fr callers`, `fr graph`, `fr impact` and `fr duplicates` are
//! six views of the same facts. Where two of them disagree, one is wrong, and nothing
//! checked that. These are the agreements, asked of this repository, which is the largest
//! workspace available to the tests.
//!
//! A truncated list is the other half. A report that stops at twenty and says nothing
//! reads as complete, so every list that stops early states how many it left out.

use fun_refactor::analysis::call_graph::{CallGraph, EdgeOrigin};
use fun_refactor::analysis::{duplicates, impact};
use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::navigate;
use fun_refactor::scan::{scan, ScanOptions};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One index for the whole file. Building it is the expensive part, and every test here
/// asks a different question of the same facts.
fn workspace() -> &'static (PathBuf, Index) {
    static WORKSPACE: std::sync::OnceLock<(PathBuf, Index)> = std::sync::OnceLock::new();
    WORKSPACE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
        let scanned = scan(&root, &ScanOptions::default()).expect("scan");
        let index = Index::build_from_scan(&scanned).expect("index");
        (root, index)
    })
}

#[test]
fn every_call_edge_has_the_reference_that_produced_it() {
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);

    let sites: HashSet<(SymbolId, &Path, usize)> = index
        .references
        .iter()
        .filter_map(|r| Some((r.target?, r.file.as_path(), r.span.start)))
        .collect();

    let mut missing = Vec::new();
    for symbol in &index.symbols {
        for (_, edge) in graph.callers(symbol.id) {
            if edge.origin != EdgeOrigin::Resolved {
                // A dispatch candidate is not a reference. It is the point of that
                // layer that no single call site produced it.
                continue;
            }
            if !sites.contains(&(symbol.id, edge.file.as_path(), edge.offset)) {
                missing.push(format!(
                    "{} at {}:{}",
                    symbol.name,
                    edge.file.display(),
                    edge.offset
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "{} edges without a reference: {:?}",
        missing.len(),
        &missing[..missing.len().min(5)]
    );
}

#[test]
fn every_call_site_sits_inside_the_function_it_is_attributed_to() {
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);
    let mut wrong = Vec::new();
    for symbol in &index.symbols {
        for (caller, edge) in graph.callers(symbol.id) {
            let Some(c) = index.symbol(caller) else {
                continue;
            };
            let inside = c.file == edge.file
                && c.full_span.start <= edge.offset
                && edge.offset < c.full_span.end;
            if !inside {
                wrong.push(format!(
                    "{} at {}:{}",
                    c.name,
                    edge.file.display(),
                    edge.offset
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} call sites outside their caller: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(5)]
    );
}

#[test]
fn callers_and_callees_are_two_views_of_one_edge() {
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);
    for symbol in index.symbols.iter().step_by(23) {
        for (caller, _) in graph.callers(symbol.id) {
            assert!(
                graph.callees(caller).iter().any(|(c, _)| *c == symbol.id),
                "{:?} lists a caller that does not list it back",
                symbol.name
            );
        }
    }
}

#[test]
fn usages_reports_the_references_that_resolved_to_the_symbol() {
    let (_root, index) = workspace();
    let mut per_target: HashMap<SymbolId, usize> = HashMap::new();
    for r in &index.references {
        if let Some(t) = r.target {
            *per_target.entry(t).or_default() += 1;
        }
    }
    // `usages_of` walks every reference, so this samples rather than sweeps.
    for symbol in index.symbols.iter().step_by(1493) {
        let reported = navigate::usages_of(index, symbol.id).usages.len();
        assert_eq!(
            reported,
            *per_target.get(&symbol.id).unwrap_or(&0),
            "usages and references disagree about {:?}",
            symbol.name
        );
    }
}

#[test]
fn impact_covers_every_reference_it_could_rewrite() {
    let (_root, index) = workspace();
    let mut sources: HashMap<PathBuf, String> = HashMap::new();
    for symbol in index.symbols.iter().step_by(311) {
        let Ok(report) = impact::analyse(index, symbol.id, 2) else {
            continue;
        };
        let covered: HashSet<(PathBuf, usize)> = report
            .items
            .iter()
            .map(|i| (i.file.clone(), i.line))
            .collect();
        for reference in index.references_to(symbol.id) {
            if !reference.confidence.is_safe_to_rewrite() {
                continue;
            }
            let text = sources
                .entry(reference.file.clone())
                .or_insert_with(|| std::fs::read_to_string(&reference.file).unwrap_or_default());
            let line = fun_refactor::span::LineIndex::new(text)
                .line_col(reference.span.start, text)
                .line;
            assert!(
                covered.contains(&(reference.file.clone(), line)),
                "impact on {:?} omits {}:{line}",
                symbol.name,
                reference.file.display()
            );
        }
    }
}

#[test]
fn every_span_duplicates_reports_is_a_region_of_its_file() {
    let (root, index) = workspace();
    let classes = duplicates::find_in(index, root).expect("duplicates");
    assert!(
        !classes.is_empty(),
        "this repository has duplicates to report"
    );
    for class in classes.iter().take(60) {
        for instance in &class.instances {
            let text = std::fs::read_to_string(&instance.file).expect("the file reads");
            assert!(
                instance.span.start < instance.span.end && instance.span.end <= text.len(),
                "{} reports {:?}, which is not inside it",
                instance.file.display(),
                instance.span
            );
        }
    }
}

#[test]
fn a_report_that_stops_early_says_how_many_it_left_out() {
    // The `certain` list already said so and the `needs review` list beside it did not,
    // in the same report.
    let (_root, index) = workspace();
    let busiest = index
        .symbols
        .iter()
        .max_by_key(|s| index.references_to(s.id).len())
        .expect("a symbol");
    let Ok(report) = impact::analyse(index, busiest.id, 2) else {
        return;
    };
    let text = impact::format_report(index, &report);
    for (heading, total) in [
        ("Certain", report.certain().len()),
        ("Needs review", report.needs_review().len()),
    ] {
        let limit = if heading == "Certain" { 40 } else { 20 };
        if total > limit {
            assert!(
                text.contains(&format!("… and {} more", total - limit)),
                "the {heading} list stops at {limit} of {total} without saying so:\n{text}"
            );
        }
    }
}
