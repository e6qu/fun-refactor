//! The commands that read answer overlapping questions from one index.

use fun_refactor::analysis::call_graph::{CallGraph, EdgeOrigin};
use fun_refactor::analysis::{duplicates, impact};
use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::navigate;
use fun_refactor::scan::{scan, ScanOptions};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One index for the whole file.
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
                // A dispatch candidate is not a reference.
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
    let mut edges = 0;
    for symbol in index.symbols.iter().step_by(23) {
        for (caller, _) in graph.callers(symbol.id) {
            edges += 1;
            assert!(
                graph.callees(caller).iter().any(|(c, _)| *c == symbol.id),
                "{:?} lists a caller that does not list it back.",
                symbol.name
            );
        }
    }
    // A graph that resolved no call at all agrees with itself trivially.
    assert!(edges > 0, "no call edge was checked in either direction.");
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
    // Every reference in the workspace, by where it is written and what it spells.
    let spelled: HashMap<(&Path, usize), &str> = index
        .references
        .iter()
        .map(|r| ((r.file.as_path(), r.span.start), r.name.as_str()))
        .collect();

    // Two agreements, and not a count.
    for symbol in index.symbols.iter().step_by(1493) {
        let report = navigate::usages_of(index, symbol.id);
        assert!(
            report.usages.len() >= per_target.get(&symbol.id).copied().unwrap_or(0),
            "usages left out references that resolved to {:?}: {} reported, {} resolved.",
            symbol.name,
            report.usages.len(),
            per_target.get(&symbol.id).copied().unwrap_or(0)
        );
        // A polymorphic declaration is used through its implementations, so a reported use may
        // resolve to one of those by name.
        let mut answers: Vec<&str> = vec![symbol.name.as_str()];
        for target in index
            .definition_group(symbol.id)
            .into_iter()
            .chain(navigate::implementations_of(index, symbol.id))
        {
            if let Some(implementation) = index.symbol(target) {
                answers.push(implementation.name.as_str());
            }
        }
        for usage in &report.usages {
            let at = (usage.location.file.as_path(), usage.location.span.start);
            let found = spelled.get(&at);
            assert!(
                found.is_none_or(|name| answers.contains(name)),
                "usages reports {:?} at {}:{} where the index has {:?}. That is \
                 neither the symbol nor an implementation of it.",
                symbol.name,
                usage.location.file.display(),
                usage.location.line,
                found
            );
        }
    }
}

#[test]
fn impact_covers_every_reference_it_could_rewrite() {
    let (_root, index) = workspace();
    let mut sources: HashMap<PathBuf, String> = HashMap::new();
    let mut checked = 0;
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
            checked += 1;
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
    // Every `impact::analyse` erroring, or no reference being safe to rewrite, would
    // leave nothing compared and the test passing.
    assert!(
        checked > 0,
        "no rewritable reference was checked against any impact report."
    );
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

#[test]
fn usages_reports_the_name_where_it_appears_in_prose() {
    // `fr usages` answers "where does this name appear".
    let (_root, index) = workspace();
    let symbol = index
        .symbols
        .iter()
        .find(|s| !index.references_to(s.id).is_empty())
        .expect("a symbol something uses");
    let name = symbol.name.clone();

    let found = fun_refactor::navigate::usages_of(index, symbol.id);
    for mention in &found.in_text {
        assert_eq!(
            mention.kind,
            fun_refactor::model::ReferenceKind::Textual,
            "a mention is not a resolved reference"
        );
        let text = std::fs::read_to_string(&mention.location.file).expect("the file");
        assert!(
            text.contains(&name),
            "{} was reported as holding '{name}' and does not",
            mention.location.file.display()
        );
    }

    // The two lists never overlap: a mention is not counted as a use.
    for mention in &found.in_text {
        assert!(
            !found.usages.iter().any(|u| u.location == mention.location),
            "{}:{} is reported as both a use and a mention",
            mention.location.file.display(),
            mention.location.line
        );
    }
}

#[test]
fn a_comment_naming_a_symbol_is_reported_by_usages() {
    // The case the sweep of this repository found.
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("a.rs"),
        "pub fn width() -> usize {\n    4\n}\n\n// width() is the one to call here.\npub fn area() -> usize {\n    width() * 2\n}\n",
    )
    .expect("write");
    let scanned = fun_refactor::scan::scan(dir.path(), &fun_refactor::scan::ScanOptions::default())
        .expect("scan");
    let index = fun_refactor::index::Index::build_from_scan(&scanned).expect("index");
    let width = index
        .symbols
        .iter()
        .find(|s| s.name == "width" && s.kind == fun_refactor::model::SymbolKind::Function)
        .expect("width")
        .id;

    let found = fun_refactor::navigate::usages_of(&index, width);
    assert_eq!(found.usages.len(), 1, "one call resolves");
    assert_eq!(
        found.in_text.len(),
        1,
        "and the comment that names it is reported: {:?}",
        found.in_text
    );
    assert_eq!(found.in_text[0].location.line, 5);
}

#[test]
fn the_neighbourhood_is_bounded_and_ranked_around_the_symbol() {
    // What the playground draws.
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);

    let start = index
        .symbols
        .iter()
        .find(|s| !graph.callers(s.id).is_empty() && !graph.callees(s.id).is_empty())
        .expect("a function with a caller and a callee")
        .id;

    let near = graph.neighbourhood(start, 2);
    assert_eq!(near.start, start);
    assert_eq!(
        near.nodes.iter().filter(|(id, _)| *id == start).count(),
        1,
        "the symbol asked about appears once"
    );
    assert_eq!(
        near.nodes.iter().find(|(id, _)| *id == start).unwrap().1,
        0,
        "and sits at distance zero"
    );

    for (_, rank) in &near.nodes {
        assert!(
            rank.abs() <= 2,
            "the walk went past the depth it was given: {rank}"
        );
    }
    for (from, to, _) in &near.edges {
        assert!(
            near.nodes.iter().any(|(id, _)| id == from)
                && near.nodes.iter().any(|(id, _)| id == to),
            "an edge joins a node the walk never reached"
        );
    }

    // Depth one is a subset of depth two: a bound cannot add nodes.
    let one = graph.neighbourhood(start, 1);
    for (id, _) in &one.nodes {
        assert!(
            near.nodes.iter().any(|(other, _)| other == id),
            "a node at depth 1 vanished at depth 2"
        );
    }
    assert!(one.nodes.len() <= near.nodes.len());
}

#[test]
fn every_symbol_in_a_neighbourhood_can_be_pointed_at() {
    // The browser opens a file at the node's position when a reader clicks it.
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);
    let start = index
        .symbols
        .iter()
        .find(|s| !graph.callers(s.id).is_empty())
        .expect("a called function")
        .id;

    let near = graph.neighbourhood(start, 2);
    assert!(!near.nodes.is_empty());
    for (id, _) in &near.nodes {
        let symbol = index.symbol(*id).expect("a node the index holds");
        let source = std::fs::read_to_string(&symbol.file).expect("the file");
        let at =
            fun_refactor::span::LineIndex::new(&source).line_col(symbol.name_span.start, &source);
        assert!(at.col > 0, "{} has no column", symbol.name);
        // And the position names the symbol, so the editor lands on it.
        let found = index.definition_at(&symbol.file, symbol.name_span.start);
        assert!(
            found.is_some(),
            "{} at {}:{} names nothing the index knows",
            symbol.name,
            at.line,
            at.col
        );
    }
}

#[test]
fn a_symbol_nothing_calls_draws_only_itself() {
    let (_root, index) = workspace();
    let graph = CallGraph::build(index);
    let lonely = index
        .symbols
        .iter()
        .find(|s| graph.callers(s.id).is_empty() && graph.callees(s.id).is_empty())
        .expect("a symbol with no edges");

    let near = graph.neighbourhood(lonely.id, 3);
    assert_eq!(near.nodes.len(), 1, "itself and nothing else");
    assert!(near.edges.is_empty());
    assert!(!near.more, "nothing lies beyond the depth");
}
