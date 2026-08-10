//! Which language boundaries does resolution actually cross today?
//!
//! Scratch measurement: tabulate every resolved reference as
//! (language it is written in) -> (language it resolves into), so the cross-language
//! edges that exist can be counted and not assumed.
use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};
use std::collections::BTreeMap;

/// How many references cross a given pair, and what kinds of thing they reach.
type EdgeCounts<'a> = BTreeMap<(&'a str, &'a str), (usize, BTreeMap<&'a str, usize>)>;

fn main() -> anyhow::Result<()> {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let scanned = scan(std::path::Path::new(&root), &ScanOptions::default())?;
    let index = Index::build_from_scan(&scanned)?;

    let mut edges: EdgeCounts = BTreeMap::new();
    let mut total = 0usize;
    for reference in &index.references {
        let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
            continue;
        };
        total += 1;
        let from = reference.language.name();
        let to = target.language.name();
        let entry = edges.entry((from, to)).or_default();
        entry.0 += 1;
        *entry.1.entry(target.kind.as_str()).or_default() += 1;
    }

    // Config-to-template edges live inside one "language" — a values.yaml beside a
    // Chart.yaml is Helm, and so is the template that reads it — so language alone
    // undercounts. Count by file role too.
    let role = |p: &std::path::Path| -> &'static str {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with("values") {
            "values"
        } else if p.components().any(|c| c.as_os_str() == "templates") {
            "template"
        } else if name.starts_with("Chart") {
            "chart"
        } else {
            "other"
        }
    };
    let mut by_role: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for reference in &index.references {
        let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
            continue;
        };
        *by_role
            .entry((role(&reference.file), role(&target.file)))
            .or_default() += 1;
    }
    println!("  by file role:");
    for ((from, to), n) in &by_role {
        if from != to {
            println!("    {from:>9} -> {to:<9} {n}");
        }
    }

    let cross: usize = edges
        .iter()
        .filter(|((a, b), _)| a != b)
        .map(|(_, (n, _))| n)
        .sum();
    println!("{root}");
    println!("  {total} resolved references, {cross} of them cross a language boundary");
    for ((from, to), (n, kinds)) in &edges {
        if from == to {
            continue;
        }
        let mut by_kind: Vec<_> = kinds.iter().collect();
        by_kind.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let detail: Vec<String> = by_kind
            .iter()
            .take(3)
            .map(|(k, n)| format!("{k} {n}"))
            .collect();
        println!("    {from:>10} -> {to:<10} {n:>6}   {}", detail.join(", "));
    }
    // Name the individual crossings for the pairs the caller asks about, so a
    // surprising edge can be looked at and not believed.
    if let Some(want) = std::env::args().nth(2) {
        let (from_want, to_want) = want.split_once("->").unwrap_or((want.as_str(), ""));
        println!("\n  crossings {from_want} -> {to_want}:");
        for reference in &index.references {
            let Some(target) = reference.target.and_then(|t| index.symbol(t)) else {
                continue;
            };
            if reference.language.name() != from_want || target.language.name() != to_want {
                continue;
            }
            let source = std::fs::read_to_string(&reference.file).unwrap_or_default();
            let line = source[..reference.span.start.min(source.len())]
                .matches('\n')
                .count()
                + 1;
            println!(
                "    {}:{} `{}` [{}] -> {} {} in {}",
                reference
                    .file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                line,
                reference.name,
                reference.confidence.as_str(),
                target.kind.as_str(),
                target.qualified_name(),
                target
                    .file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            );
        }
    }
    Ok(())
}
