use fun_refactor::{analysis::stitch, index::Index, scan::ScanOptions};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn fixture() -> (tempfile::TempDir, Index, BTreeMap<PathBuf, String>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("chart/templates")).unwrap();
    let sources: BTreeMap<_, _> = [
        ("chart/Chart.yaml", "name: demo\nversion: 0.1.0\n"),
        ("chart/values.yaml", "db:\n  url: private\n"),
        (
            "chart/templates/app.yaml",
            "spec:\n  env:\n    - name: DATABASE_URL\n      value: {{ .Values.db.url }}\n",
        ),
        (
            "app.py",
            "import os\nos.getenv('DATABASE_URL')\nos.getenv('UNDECLARED')\n",
        ),
    ]
    .into_iter()
    .map(|(path, source)| (root.join(path), source.to_owned()))
    .collect();
    for (path, source) in &sources {
        fs::write(path, source).unwrap();
    }
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    (dir, index, sources)
}

#[test]
fn captured_analysis_matches_legacy_chains_and_never_falls_back_to_disk() {
    let (_dir, index, sources) = fixture();
    let expected = stitch::chains(&index).unwrap();
    assert_eq!(expected.len(), 1);
    for path in sources.keys() {
        fs::remove_file(path).unwrap();
    }
    let result = stitch::analyze_snapshot(&index, &sources).unwrap();
    assert_eq!(result.chains, expected);
    assert_eq!(result.reads.len(), 2);
    assert!(result.reads.iter().any(|read| read.name == "UNDECLARED"));
    assert!(result.gaps.is_empty());
}

#[test]
fn missing_and_mismatched_inputs_report_gaps_and_cannot_supply_values_or_reads() {
    let (dir, index, mut sources) = fixture();
    let root = dir.path().canonicalize().unwrap();
    sources.remove(&root.join("app.py"));
    sources.insert(root.join("chart/values.yaml"), "wrong: source\n".into());
    let result = stitch::analyze_snapshot(&index, &sources).unwrap();
    assert_eq!(result.gaps.len(), 2);
    assert!(result
        .gaps
        .iter()
        .any(|(_, reason)| reason.contains("absent")));
    assert!(result
        .gaps
        .iter()
        .any(|(_, reason)| reason.contains("content hash")));
    assert_eq!(result.chains.len(), 1);
    assert!(result.chains[0].values_file.is_none());
    assert!(result.chains[0].reads.is_empty());
    assert!(result.reads.is_empty());
}

#[test]
fn syntax_gaps_exclude_partial_facts_and_extra_sources_do_not_expand_the_index() {
    let (dir, _, mut sources) = fixture();
    let root = dir.path().canonicalize().unwrap();
    sources.insert(
        root.join("app.py"),
        "import os\nos.getenv('DATABASE_URL')\nprint(\n".into(),
    );
    sources.insert(root.join("chart/values.yaml"), "db: [\n".into());
    for (path, source) in &sources {
        fs::write(path, source).unwrap();
    }
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    sources.insert(
        root.join("unindexed.py"),
        "import os\nos.getenv('DATABASE_URL')\n".into(),
    );
    let result = stitch::analyze_snapshot(&index, &sources).unwrap();
    assert_eq!(result.gaps.len(), 2);
    assert!(result.reads.is_empty());
    assert_eq!(result.chains.len(), 1);
    assert!(result.chains[0].values_file.is_none());
}
