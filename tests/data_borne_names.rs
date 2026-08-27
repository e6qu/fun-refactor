//! Names that reach the code through data instead of through references.

use fun_refactor::index::Index;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

fn dead_names(index: &Index) -> Vec<String> {
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    fun_refactor::refactor::delete::find_unused(index, &entrypoints)
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.clone())
        .collect()
}

#[test]
fn a_variant_a_catalog_spells_in_kebab_case_is_alive() {
    let (_tmp, index) = workspace(&[
        (
            "model.rs",
            "pub enum ThreatModel {\n    Remote,\n    LocalFirst,\n}\n",
        ),
        (
            "catalog.yaml",
            "threat_model: remote\nfallback: \"local-first\"\n",
        ),
    ]);
    let dead = dead_names(&index);
    assert!(!dead.contains(&"Remote".to_string()), "{dead:?}");
    assert!(
        !dead.contains(&"LocalFirst".to_string()),
        "a quoted scalar and a two-word rename both count: {dead:?}"
    );
}

#[test]
fn a_comma_separated_data_attribute_names_each_part() {
    let (_tmp, index) = workspace(&[(
        "page.html",
        "<div data-quiz=\"alpha_case,beta_case\"></div>\n",
    )]);
    let names: Vec<&str> = index
        .symbols
        .iter()
        .filter(|s| s.kind == fun_refactor::model::SymbolKind::DataAttribute)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["alpha_case", "beta_case"],
        "one symbol per hook, not one blob"
    );
}

#[test]
fn a_method_implementing_a_foreign_trait_is_alive() {
    let (_tmp, index) = workspace(&[(
        "lib.rs",
        "pub struct Money(i64);\n\n\
         impl std::fmt::Display for Money {\n    \
         fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        \
         write!(f, \"{}\", self.0)\n    }\n}\n",
    )]);
    let dead = dead_names(&index);
    assert!(
        !dead.contains(&"fmt".to_string()),
        "Display's machinery is the caller: {dead:?}"
    );
}

#[test]
fn a_method_of_a_local_trait_still_answers_to_reachability() {
    let (_tmp, index) = workspace(&[(
        "lib.rs",
        "pub trait Speak {\n    fn hello(&self);\n}\n\n\
         pub struct Quiet;\n\n\
         impl Speak for Quiet {\n    fn hello(&self) {}\n}\n",
    )]);
    let entrypoints = fun_refactor::analysis::entrypoints::Entrypoints::default();
    let report = fun_refactor::refactor::delete::find_unused_report(&index, &entrypoints);
    let foreign = report.spared.iter().any(|(_, reason)| {
        matches!(
            reason,
            fun_refactor::refactor::delete::SparedReason::ImplementsForeignTrait
        )
    });
    assert!(
        !foreign,
        "a trait declared here is not foreign; the hierarchy already rules on it"
    );
}
