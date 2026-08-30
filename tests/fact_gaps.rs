//! What the index does not know about a file, and how it says so.

use fun_refactor::extract::Extractor;
use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::{FactGap, SymbolId};
use fun_refactor::parse::Parsers;
use fun_refactor::refactor::{rename, WarningKind};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::Path;

fn gaps(source: &str) -> Vec<FactGap> {
    Parsers::new()
        .parse(Language::Helm, source)
        .expect("the grammar loads")
        .gaps
}

fn key_names(source: &str) -> Vec<String> {
    let parsers = Parsers::new();
    let parsed = parsers.parse(Language::Helm, source).expect("loads");
    Extractor::new()
        .extract(&parsed, Path::new("t.yaml"), source)
        .expect("extraction")
        .symbols
        .into_iter()
        .map(|s| s.name)
        .collect()
}

#[test]
fn a_templated_key_is_reported_whether_or_not_the_parse_survives_it() {
    // The same construct, differing only in what surrounds it.
    let parses = "matchLabels:\n  {{ $key | quote }}: {{ $value | quote }}\n";
    let does_not = "a: 1\n{{ .Values.k }}: v\nb: 2\n";

    assert_eq!(gaps(parses), [FactGap::TemplatedKeys]);
    assert_eq!(
        gaps(does_not),
        [FactGap::SyntaxErrors, FactGap::TemplatedKeys]
    );
}

#[test]
fn a_templated_key_opening_a_sequence_entry_is_a_key_too() {
    // `- ` is structure.
    assert_eq!(
        gaps("items:\n  - {{ $key }}: v\n"),
        [FactGap::TemplatedKeys]
    );
}

#[test]
fn a_templated_key_never_becomes_a_symbol() {
    // Byte offsets index the original source, so a key masked to a scalar reports under the
    // action's own text.
    for source in [
        "items:\n  - {{ $key }}: v\n",
        "matchLabels:\n  {{ $key | quote }}: {{ $value | quote }}\n",
    ] {
        assert!(
            key_names(source).iter().all(|name| !name.contains("{{")),
            "a template action reached the index as a key name: {:?}",
            key_names(source)
        );
    }
}

#[test]
fn structural_actions_are_not_key_positions() {
    // `{{- if }}`/`{{- end }}` own their lines and carry no `:`.
    let source = "spec:\n  {{- if .Values.enabled }}\n  replicas: 1\n  {{- end }}\n";
    assert!(gaps(source).is_empty(), "{:?}", gaps(source));
    assert!(key_names(source).contains(&"replicas".to_string()));
}

#[test]
fn a_value_action_is_not_a_key_position() {
    let source = "image:\n  tag: {{ .Values.image.tag }}\n";
    assert!(gaps(source).is_empty(), "{:?}", gaps(source));
}

#[test]
fn rename_reports_the_file_whose_key_it_could_not_read() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("Chart.yaml"), "name: c\nversion: 1.0.0\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("templates")).unwrap();
    std::fs::write(
        tmp.path().join("templates/np.yaml"),
        "matchLabels:\n  {{ $key | quote }}: {{ $value | quote }}\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("values.yaml"), "replicaCount: 1\n").unwrap();

    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();

    let target: SymbolId = index
        .find_symbols("replicaCount", None)
        .first()
        .expect("values.yaml defines replicaCount")
        .id;
    let plan = rename::plan(&index, target, "replicas").unwrap();

    let reported: Vec<&str> = plan
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::IncompleteFacts)
        .map(|w| w.detail.as_str())
        .collect();
    assert!(
        reported
            .iter()
            .any(|d| d.contains("template action where a key belongs")),
        "the templated key needs its own cause in the report, got {reported:?}"
    );
}
