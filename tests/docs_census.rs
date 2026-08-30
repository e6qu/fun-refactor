//! The census `CROSS_LANGUAGE.md` publishes, measured rather than remembered.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::scan::{scan, ScanOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn sample() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/web/sample"))
}

fn doc() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/CROSS_LANGUAGE.md"))
        .expect("CROSS_LANGUAGE.md is readable")
}

struct Census {
    files: usize,
    languages: usize,
    resolved: usize,
    /// From-language to to-language, for the references that cross one.
    crossings: BTreeMap<(Language, Language), usize>,
}

fn measure() -> Census {
    let scanned = scan(&sample(), &ScanOptions::default()).expect("the sample scans");
    let index = Index::build_from_scan(&scanned).expect("the sample indexes");

    // Counted from the scan and not from the symbols, because `fr scan` is what a reader runs
    // to check this line.
    let mut languages = BTreeSet::new();
    let mut files = BTreeSet::new();
    for file in &scanned.files {
        languages.insert(file.language);
        files.insert(file.path.clone());
    }

    let mut resolved = 0usize;
    let mut crossings: BTreeMap<(Language, Language), usize> = BTreeMap::new();
    for reference in &index.references {
        let Some(target) = reference.target else {
            continue;
        };
        resolved += 1;
        let Some(symbol) = index.symbol(target) else {
            continue;
        };
        // TSX is TypeScript with JSX in it, and the index keeps them apart.
        let same = symbol.language == reference.language;
        if !same {
            *crossings
                .entry((reference.language, symbol.language))
                .or_default() += 1;
        }
    }

    Census {
        files: files.len(),
        languages: languages.len(),
        resolved,
        crossings,
    }
}

#[test]
fn the_sample_census_is_what_the_document_says() {
    let census = measure();
    eprintln!(
        "sample census: {} files, {} languages, {} resolved, crossings {:?}",
        census.files, census.languages, census.resolved, census.crossings
    );
    let claim = format!(
        "web/sample ({} files, {} languages, {} resolved references)",
        census.files, census.languages, census.resolved
    );
    assert!(
        doc().contains(&claim),
        "CROSS_LANGUAGE.md does not say `{claim}`. This measured the sample just \
         now, so the document is what moved."
    );
}

#[test]
fn every_crossing_the_sample_has_is_in_the_table() {
    // The table under the census names each crossing and its count.
    let census = measure();
    let doc = doc();
    // The table is laid out in columns, so match on the pair and the count
    // separately rather than on the spacing between them.
    let mut absent = Vec::new();
    for ((from, to), n) in &census.crossings {
        let pair = format!("{from} -> {to}");
        let Some(at) = doc.find(&pair) else {
            absent.push(format!("{pair} ({n}), no row at all"));
            continue;
        };
        let line_end = doc[at..].find('\n').map_or(doc.len(), |i| at + i);
        let row = &doc[at..line_end];
        if !row.split_whitespace().any(|word| word == n.to_string()) {
            absent.push(format!("{pair} is {n} now. The row says `{row}`."));
        }
    }
    assert!(
        absent.is_empty(),
        "the crossings table in CROSS_LANGUAGE.md disagrees with the sample: {absent:?}."
    );
}

#[test]
fn the_sample_still_exercises_many_languages() {
    // Both checks above would pass over an empty sample by comparing nothing.
    let census = measure();
    assert!(
        census.languages > 10 && census.resolved > 100,
        "the sample now measures {} language(s) and {} resolved reference(s), \
         which is too little for the census to mean anything",
        census.languages,
        census.resolved
    );
}

/// A figure a document states about this repository, and how to count it.
struct Stated {
    doc: &'static str,
    claim: &'static str,
    counted: usize,
}

#[test]
fn every_figure_a_document_states_about_this_repository_is_countable() {
    // The documents also quote measurements on helm, ripgrep and requests.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let entries = |dir: &str| -> Vec<std::path::PathBuf> {
        std::fs::read_dir(root.join(dir))
            .unwrap_or_else(|e| panic!("{dir}/ is readable: {e}"))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect()
    };

    let groups = entries("tests/conformance")
        .iter()
        .filter(|p| p.is_dir())
        .count();

    // Every language any entry-point rule names, across the catalogs.
    let mut catalog_languages = BTreeSet::new();
    for path in entries("catalogs") {
        let text = std::fs::read_to_string(&path).expect("a catalog is readable");
        for line in text.lines() {
            let Some(list) = line.trim().strip_prefix("languages: [") else {
                continue;
            };
            let Some(list) = list.strip_suffix(']') else {
                continue;
            };
            for name in list.split(',') {
                catalog_languages.insert(name.trim().to_string());
            }
        }
    }

    let route_files = walk(&root.join("tests/petstore"))
        .iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("route.ts"))
        .count();

    let word = |n: usize| -> &'static str {
        match n {
            8 => "eight",
            13 => "thirteen",
            14 => "fourteen",
            15 => "fifteen",
            other => panic!("no word here for {other}; add it and update the prose"),
        }
    };

    for stated in [
        Stated {
            doc: "IR.md",
            claim: "`tests/conformance/` holds fourteen\ngroups",
            counted: groups,
        },
        Stated {
            doc: "RECIPES.md",
            claim: "which already carries rules for thirteen languages",
            counted: catalog_languages.len(),
        },
        Stated {
            doc: "API_CONTRACTS.md",
            claim: "a Next.js App Router API with eight route files",
            counted: route_files,
        },
    ] {
        let text = std::fs::read_to_string(root.join(stated.doc)).expect("the doc is readable");
        assert!(
            stated.claim.contains(word(stated.counted)),
            "{} states {:?} and the count is {} now.",
            stated.doc,
            stated.claim,
            stated.counted
        );
        assert!(
            text.contains(stated.claim),
            "{} no longer says {:?}, so the figure went unchecked. The count is {}.",
            stated.doc,
            stated.claim,
            stated.counted
        );
    }
}

/// Every file under `dir`, recursively.
fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            match path.is_dir() {
                true => stack.push(path),
                false => out.push(path),
            }
        }
    }
    out
}
