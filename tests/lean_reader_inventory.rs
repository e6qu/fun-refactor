use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::Symbol;
use fun_refactor::parse::Parsers;
use fun_refactor::scan::{scan, ScanOptions};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

fn identity(symbol: &Symbol, root: &Path) -> Value {
    json!({"file": symbol.file.strip_prefix(root).unwrap(), "name": symbol.name,
        "kind": symbol.kind.as_str(), "name_span": symbol.name_span})
}

#[test]
#[ignore = "explicit before/after source review on GitHub"]
fn record_lean_reader_facts() {
    let root = std::path::PathBuf::from(std::env::var("FR_READER_ROOT").unwrap());
    let output = std::env::var("FR_READER_OUTPUT").unwrap();
    let scanned = scan(
        &root,
        &ScanOptions {
            respect_ignore: true,
            languages: vec![], // Match the complete repository benchmark's language scope.
            max_file_bytes: 4 * 1024 * 1024,
        },
    )
    .unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let mut sources = BTreeMap::new();
    let mut syntax = BTreeMap::new();
    let parsers = Parsers::new();
    for file in &scanned.files {
        let bytes = std::fs::read(&file.path).unwrap();
        let name = file.path.strip_prefix(&root).unwrap().to_path_buf();
        sources.insert(name.clone(), hex::encode(Sha256::digest(&bytes)));
        if file.language != Language::Lean {
            continue;
        }
        let source = std::str::from_utf8(&bytes).unwrap();
        let tree = parsers.parse(Language::Lean, source).unwrap();
        syntax.insert(
            name,
            json!({"errors": tree.error_spans(),
            "tree_with_errors": tree.has_errors().then(|| tree.root().to_sexp())}),
        );
    }
    let symbols = index
        .symbols
        .iter()
        .filter(|symbol| symbol.language == Language::Lean)
        .map(|symbol| {
            json!({
                "identity": identity(symbol, &root), "full_span": symbol.full_span,
                "qualifier": symbol.qualifier, "exported": symbol.exported,
                "container": symbol.container.map(|id| identity(index.symbol(id).unwrap(), &root)),
            })
        })
        .collect::<Vec<_>>();
    let references = index
        .references
        .iter()
        .filter(|reference| reference.language == Language::Lean)
        .map(|reference| {
            json!({
                "file": reference.file.strip_prefix(&root).unwrap(), "name": reference.name,
                "span": reference.span, "kind": reference.kind, "twin": reference.twin,
                "confidence": reference.confidence, "receiver": reference.receiver,
                "expects": reference.expects, "receiver_is_path": reference.receiver_is_path,
                "member_in_macro": reference.member_in_macro,
                "target": reference.target.map(|id| identity(index.symbol(id).unwrap(), &root)),
            })
        })
        .collect::<Vec<_>>();
    let stats = index.stats();
    let consumer_queries = index
        .symbols
        .iter()
        .step_by(index.symbols.len().div_ceil(512).max(1))
        .map(|symbol| identity(symbol, &root))
        .collect::<Vec<_>>();
    let kernel_symbols = index
        .symbols
        .iter()
        .filter(|symbol| {
            symbol.language == Language::Lean
                && symbol
                    .file
                    .strip_prefix(&root)
                    .unwrap()
                    .starts_with("kernels")
        })
        .count();
    let kernel_references = index
        .references
        .iter()
        .filter(|reference| {
            reference.language == Language::Lean
                && reference
                    .file
                    .strip_prefix(&root)
                    .unwrap()
                    .starts_with("kernels")
        })
        .count();
    let skipped_large = scanned
        .skipped_too_large
        .iter()
        .map(|(path, bytes)| json!({"file": path.strip_prefix(&root).unwrap(), "bytes": bytes}))
        .collect::<Vec<_>>();
    let skipped_links = scanned
        .skipped_symlinks
        .iter()
        .map(|(path, reason)| json!({"file": path.strip_prefix(&root).unwrap(), "reason": reason}))
        .collect::<Vec<_>>();
    let facts = json!({"sources": sources, "syntax": syntax, "symbols": symbols,
        "references": references, "stats": {"files": stats.files, "symbols": stats.symbols,
        "references": stats.references, "resolved": stats.resolved},
        "kernel_stats": {"symbols": kernel_symbols, "references": kernel_references},
        "consumer_queries": consumer_queries, "skipped_large": skipped_large, "skipped_links": skipped_links});
    std::fs::write(output, serde_json::to_vec_pretty(&facts).unwrap()).unwrap();
}
