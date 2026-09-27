//! Consumer queries agree with a separate linear membership oracle.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::{Reference, SymbolId};
use std::path::PathBuf;

fn fixture() -> Index {
    Index::build_from_sources(&[
        (PathBuf::from("app.py"), Language::Python,
         "def café(value):\n    return value\ndef other(value):\n    return value\ndef run():\n    return café(1) + other(2) + café(3)\n".into()),
        (PathBuf::from("overloads.ts"), Language::TypeScript,
         "function pick(x: string): string;\nfunction pick(x: number): number;\nfunction pick(x: any): any { return x; }\nconst result = pick(1);\n".into()),
        (PathBuf::from("style.css"), Language::Css, ".card { color: red; }\n.card { color: blue; }\n".into()),
        (PathBuf::from("page.html"), Language::Html, "<div class=\"card\"></div>\n".into()),
    ]).unwrap()
}

fn linear(index: &Index, id: SymbolId) -> Vec<&Reference> {
    let group = index.definition_group(id);
    index
        .references
        .iter()
        .filter(|reference| {
            reference
                .target
                .is_some_and(|target| group.contains(&target))
        })
        .collect()
}

fn same(index: &Index) {
    for symbol in &index.symbols {
        let expected = linear(index, symbol.id);
        let actual = index.references_to(symbol.id);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(std::ptr::eq(*actual, expected), "occurrence order changed");
        }
        let expected = index
            .references
            .iter()
            .filter(|r| r.name == symbol.name && r.target != Some(symbol.id))
            .collect::<Vec<_>>();
        let actual = index.unresolved_matching(symbol.id);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(std::ptr::eq(*actual, expected));
        }
    }
    assert!(index.references_to(SymbolId(u32::MAX)).is_empty());
    assert!(index.unresolved_matching(SymbolId(u32::MAX)).is_empty());
}

#[test]
fn consumer_occurrences_keep_groups_order_and_confidence() {
    let index = fixture();
    assert!(index
        .symbols
        .iter()
        .any(|s| index.definition_group(s.id).len() > 1));
    assert!(index.references.iter().any(|r| r.target.is_some()));
    same(&index);
    same(&index);
}

#[test]
fn consumer_queries_follow_public_reference_mutations() {
    let mut index = fixture();
    same(&index);
    let id = index.symbols.iter().find(|s| s.name == "other").unwrap().id;
    index.references[0].target = Some(id);
    same(&index);
    index.references[0].name = "other".into();
    same(&index);
    index.references.reverse();
    same(&index);
    let reference = index.references[0].clone();
    index.references.push(reference.clone());
    same(&index);
    index.references.insert(0, reference);
    same(&index);
    index.references.remove(1);
    same(&index);
    for reference in &mut index.references {
        reference.target = None;
    }
    same(&index);
    index.references.clear();
    same(&index);
}

/// Run in a separate process; the evaluator records process RSS and wall time.
#[test]
#[ignore = "retained measurement, run with tools/index-consumers-acceptance.py"]
fn measure_consumer_queries() {
    use serde_json::json;
    use std::time::Instant;
    let workload = std::env::var("FR_CONSUMER_WORKLOAD").unwrap();
    let start = Instant::now();
    let index = if workload == "repository" {
        Index::build(
            &PathBuf::from(std::env::var("FR_CONSUMER_ROOT").unwrap()),
            &Default::default(),
        )
        .unwrap()
    } else {
        let count: usize = workload.parse().unwrap();
        let mut source = String::new();
        for n in 0..count {
            source.push_str(&format!("def f{n}(value):\n    return value\n"));
        }
        source.push_str("def run(value):\n");
        for n in 0..count {
            for _ in 0..8 {
                source.push_str(&format!("    value = f{n}(value)\n"));
            }
        }
        source.push_str("    return value\n");
        Index::build_from_sources(&[(PathBuf::from("generated.py"), Language::Python, source)])
            .unwrap()
    };
    let build_seconds = start.elapsed().as_secs_f64();
    let stride = index.symbols.len().div_ceil(512).max(1);
    let ids = index
        .symbols
        .iter()
        .step_by(stride)
        .map(|s| s.id)
        .collect::<Vec<_>>();
    let oracle = ids
        .iter()
        .map(|id| {
            linear(&index, *id)
                .iter()
                .map(|r| (r.file.clone(), r.span, r.target, r.confidence))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut samples = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let actual = ids
            .iter()
            .map(|id| {
                index
                    .references_to(*id)
                    .iter()
                    .map(|r| (r.file.clone(), r.span, r.target, r.confidence))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let seconds = start.elapsed().as_secs_f64();
        assert_eq!(actual, oracle);
        samples.push(seconds);
    }
    let output = json!({"workload": workload, "symbols": index.symbols.len(),
        "references": index.references.len(), "queries": ids.len(),
        "build_seconds": build_seconds, "query_seconds": samples,
        "answer_bytes": serde_json::to_vec(&oracle).unwrap().len(), "oracle_agrees": true});
    std::fs::write(
        std::env::var("FR_CONSUMER_OUTPUT").unwrap(),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
}
