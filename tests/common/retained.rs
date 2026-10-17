//! Select the current saved report from the required collection receipt.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn report(group: &str) -> String {
    checked_report(Path::new(env!("CARGO_MANIFEST_DIR")), group)
}

pub fn checked_report(root: &Path, group: &str) -> String {
    let catalog: Value =
        serde_json::from_slice(&fs::read(root.join("tests/agent-eval/roadmap.json")).unwrap())
            .unwrap();
    assert_eq!(catalog["schema"], "fr-roadmap-catalog-1");
    let profiles: Vec<_> = catalog["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["id"] == "compiler-profile")
        .collect();
    assert_eq!(profiles.len(), 1, "need one current compiler profile");
    let profile = profiles[0];
    let prefix = profile["path"]
        .as_str()
        .unwrap()
        .strip_prefix("tests/agent-eval/results/")
        .unwrap()
        .strip_suffix("-compiler-profile/result.json")
        .unwrap();
    assert!(!prefix.is_empty() && !prefix.contains(['/', '\\']));
    assert!(!group.is_empty() && !group.contains(['/', '\\']));
    let results = root.join("tests/agent-eval/results");
    let receipt: Value = serde_json::from_slice(
        &fs::read(results.join(format!("{prefix}-scalar-collection/manifest.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["schema"], "fr-scalar-collection");
    assert_eq!(
        profile["sha256"].as_str().unwrap(),
        receipt["groups"]["compiler-profile"]["files"]["result.json"]
            .as_str()
            .unwrap(),
        "catalog and collection must identify the same compiler report"
    );
    let path = results.join(format!("{prefix}-{group}/result.json"));
    let expected = receipt["groups"][group]["files"]["result.json"]
        .as_str()
        .expect("need the requested group in the current collection");
    assert_eq!(
        hex::encode(Sha256::digest(fs::read(&path).unwrap())),
        expected,
        "saved report differs from its collection receipt"
    );
    path.to_str().unwrap().to_owned()
}
