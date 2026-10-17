#[path = "common/retained.rs"]
mod retained;

use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn fixture(root: &Path) {
    let results = root.join("tests/agent-eval/results");
    fs::create_dir_all(results.join("current-scalar-collection")).unwrap();
    fs::create_dir_all(results.join("current-flow-facts")).unwrap();
    fs::create_dir_all(results.join("old-flow-facts")).unwrap();
    fs::write(results.join("current-flow-facts/result.json"), b"{}").unwrap();
    fs::write(results.join("old-flow-facts/result.json"), b"{}").unwrap();
    let digest = hex::encode(Sha256::digest(b"{}"));
    fs::write(
        root.join("tests/agent-eval/roadmap.json"),
        json!({"schema": "fr-roadmap-catalog-1", "evidence": [{
            "id": "compiler-profile", "sha256": digest,
            "path": "tests/agent-eval/results/current-compiler-profile/result.json"
        }]})
        .to_string(),
    )
    .unwrap();
    fs::write(
        results.join("current-scalar-collection/manifest.json"),
        json!({"schema": "fr-scalar-collection", "groups": {
            "compiler-profile": {"files": {"result.json": digest}},
            "flow-facts": {"files": {"result.json": digest}}
        }})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn tests_use_the_current_collection_and_verify_its_report_bytes() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let path = retained::checked_report(root.path(), "flow-facts");
    assert!(path.ends_with("current-flow-facts/result.json"));
    fs::write(&path, b"changed").unwrap();
    assert!(
        std::panic::catch_unwind(|| retained::checked_report(root.path(), "flow-facts")).is_err()
    );
}

#[test]
fn a_missing_current_receipt_cannot_select_an_older_report() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let manifest = root
        .path()
        .join("tests/agent-eval/results/current-scalar-collection/manifest.json");
    fs::remove_file(manifest).unwrap();
    assert!(
        std::panic::catch_unwind(|| retained::checked_report(root.path(), "flow-facts")).is_err()
    );
}

#[test]
fn a_collection_from_another_catalog_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let path = root.path().join("tests/agent-eval/roadmap.json");
    let mut catalog: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    catalog["evidence"][0]["sha256"] = json!("different collection");
    fs::write(path, catalog.to_string()).unwrap();
    assert!(
        std::panic::catch_unwind(|| retained::checked_report(root.path(), "flow-facts")).is_err()
    );
}

#[test]
fn every_active_scalar_group_has_an_authenticated_current_report() {
    for group in [
        "flow-fixed-point",
        "recursive-flow",
        "flow-facts",
        "imported-flow",
        "package-flow",
        "package-reexports",
        "module-aliases",
        "expression-control",
        "call-binding",
        "literal-defaults",
        "namespace-packages",
        "scalar-assignments",
        "flow-cache",
        "compiler-profile",
        "index-resolution",
        "python-repositories",
        "fixed-arity-probes",
    ] {
        assert!(Path::new(&retained::report(group)).is_file());
    }
}
