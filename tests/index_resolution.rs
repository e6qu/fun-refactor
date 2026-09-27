use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolId;

fn original_count(index: &Index, ids: &[SymbolId]) -> usize {
    let mut remaining = ids.to_vec();
    let mut count = 0;
    while let Some(first) = remaining.first().copied() {
        let group = index.definition_group(first);
        remaining.retain(|id| !group.contains(id) && *id != first);
        count += 1;
    }
    count
}

#[test]
fn entity_counts_preserve_order_duplicates_missing_ids_and_mutations() {
    let mut index = Index::build_from_sources(&[
        ("a.py".into(), Language::Python, "def run():\n    value = 1\n    value = 2\n    return value\nclass A:\n    def pick(self): return 1\nclass B:\n    def pick(self): return 2\n".into()),
        ("a.ts".into(), Language::TypeScript, "function pick(x: string): string;\nfunction pick(x: number): number;\nfunction pick(x: any): any { return x; }\n".into()),
        ("a.css".into(), Language::Css, ".card { color: red; }\n.card { color: blue; }".into()),
        ("a.module.css".into(), Language::Css, ".card { color: green; }".into()),
        ("chart/values.yaml".into(), Language::Yaml, "image:\n  tag: stable\n".into()),
        ("chart/values-prod.yaml".into(), Language::Yaml, "image:\n  tag: latest\n".into()),
    ]).unwrap();
    let mut ids = index.symbols.iter().map(|s| s.id).collect::<Vec<_>>();
    ids.extend([SymbolId(u32::MAX), SymbolId(u32::MAX - 1)]);
    let mut seed = 713u64;
    for mutation in 0..4 {
        for size in 0..80 {
            let sample = (0..size)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    ids[seed as usize % ids.len()]
                })
                .collect::<Vec<_>>();
            assert_eq!(
                index.count_entities(&sample),
                original_count(&index, &sample)
            );
        }
        for symbol in &mut index.symbols {
            match mutation {
                0 => symbol.container = None,
                1 => symbol.qualifier = Some("owner".into()),
                2 => symbol.language = Language::Python,
                _ => symbol.scope = fun_refactor::model::ScopeId(0),
            }
        }
    }
}

#[test]
#[ignore = "isolated baseline comparison via tools/index-resolution-acceptance.py"]
fn measure_resolution() {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::time::Instant;

    let workload = std::env::var("FR_RESOLUTION_WORKLOAD").unwrap();
    let root = PathBuf::from(std::env::var("FR_RESOLUTION_ROOT").unwrap());
    let sources = if let Some(count) = workload.strip_prefix("members-") {
        let count: usize = count.parse().unwrap();
        let mut source = String::new();
        for n in 0..count {
            source.push_str(&format!(
                "struct T{n};\nimpl T{n} {{ fn pick(&self) -> i32 {{ {n} }} }}\n"
            ));
        }
        for n in 0..count {
            source.push_str(&format!(
                "fn run{n}(x: T{n}) -> i32 {{ x.pick() + x.pick() }}\n"
            ));
        }
        vec![("members.rs".into(), Language::Rust, source)]
    } else if let Some(count) = workload.strip_prefix("assignments-") {
        let count: usize = count.parse().unwrap();
        let mut source = "def run(value):\n".to_string();
        for n in 0..count {
            source.push_str(&format!("    value = value + {n}\n"));
        }
        source.push_str("    return value\n");
        vec![("assignments.py".into(), Language::Python, source)]
    } else {
        assert_eq!(workload, "repository");
        vec![]
    };
    let start = Instant::now();
    let mut index = if workload == "repository" {
        let scan = fun_refactor::scan::scan(&root, &Default::default()).unwrap();
        Index::build_with_cache(&scan, None).unwrap()
    } else {
        Index::build_from_sources(&sources).unwrap()
    };
    let build_seconds = start.elapsed().as_secs_f64();
    for symbol in &mut index.symbols {
        if let Ok(path) = symbol.file.strip_prefix(&root) {
            symbol.file = path.to_path_buf();
        }
    }
    for reference in &mut index.references {
        if let Ok(path) = reference.file.strip_prefix(&root) {
            reference.file = path.to_path_buf();
        }
    }
    let answers = serde_json::to_vec(&(&index.symbols, &index.references)).unwrap();
    let report = json!({"workload": workload, "symbols": index.symbols.len(),
        "references": index.references.len(), "answer_bytes": answers.len(),
        "answer_sha256": hex::encode(Sha256::digest(&answers)), "build_seconds": build_seconds});
    std::fs::write(
        std::env::var("FR_RESOLUTION_OUTPUT").unwrap(),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
