use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::Symbol;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub fn source_identity(symbol: &Symbol, root: &Path) -> Value {
    json!({"file": symbol.file.strip_prefix(root).unwrap(), "name": symbol.name,
        "kind": symbol.kind.as_str(), "name_span": symbol.name_span})
}

pub fn reviewed_lean_facts(index: &Index, root: &Path) -> Value {
    let inventory_bytes = include_bytes!("../agent-eval/index-consumers/lean-reader-review.json");
    let inventory: Value = serde_json::from_slice(inventory_bytes).unwrap();
    let mut sources = BTreeMap::new();
    for (path, _) in index
        .files()
        .filter(|(_, file)| file.language == Language::Lean)
    {
        let source = std::fs::read(path).unwrap();
        sources.insert(
            path.strip_prefix(root).unwrap(),
            hex::encode(Sha256::digest(source)),
        );
    }
    assert_eq!(
        serde_json::to_value(sources).unwrap(),
        inventory["lean_sources"]
    );
    let symbols = index
        .symbols
        .iter()
        .filter(|s| s.language == Language::Lean)
        .map(|s| {
            json!({"identity": source_identity(s, root), "full_span": s.full_span,
            "qualifier": s.qualifier, "exported": s.exported,
            "container": s.container.map(|id| source_identity(index.symbol(id).unwrap(), root))})
        })
        .collect::<Vec<_>>();
    let references = index
        .references
        .iter()
        .filter(|r| r.language == Language::Lean)
        .map(|r| {
            json!({"file": r.file.strip_prefix(root).unwrap(), "name": r.name,
            "span": r.span, "kind": r.kind, "twin": r.twin,
            "confidence": r.confidence, "receiver": r.receiver, "expects": r.expects,
            "receiver_is_path": r.receiver_is_path, "member_in_macro": r.member_in_macro,
            "target": r.target.map(|id| source_identity(index.symbol(id).unwrap(), root))})
        })
        .collect::<Vec<_>>();
    let facts = json!({"symbols": symbols, "references": references});
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&facts).unwrap()));
    assert_eq!(
        digest,
        inventory["lean_facts_sha256"].as_str().unwrap(),
        "Lean facts changed outside the reviewed inventory"
    );
    json!({"facts_sha256": digest, "inventory_sha256": hex::encode(Sha256::digest(inventory_bytes))})
}

pub fn ignored_rcases_wildcards(index: &Index, root: &std::path::Path) -> Vec<serde_json::Value> {
    let pattern = regex::Regex::new(r"(?m)\brcases\b[^\n]*?\bwith[ \t]+(_)[ \t]*\|").unwrap();
    let mut sites = Vec::new();
    for (path, file) in index
        .files()
        .filter(|(_, file)| file.language == Language::Lean)
    {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        let source = std::fs::read_to_string(&absolute).unwrap();
        for capture in pattern.captures_iter(&source) {
            let wildcard = capture.get(1).unwrap();
            assert!(file
                .references
                .iter()
                .all(|index_in_file| !index.references[*index_in_file]
                    .span
                    .contains_offset(wildcard.start())));
            sites.push(
                serde_json::json!({"file": absolute.strip_prefix(root).unwrap(),
                "span": {"start": wildcard.start(), "end": wildcard.end()}}),
            );
        }
    }
    sites
}
