//! Read explicit current report paths and verify their recorded bytes.

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
    assert!(!group.is_empty() && !group.contains(['/', '\\']));
    let settings: Value = serde_json::from_slice(
        &fs::read(root.join("tests/agent-eval/current-scalar-reports.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["schema"], "fr-current-scalar-reports");
    let entry = &settings["reports"][group];
    let relative = entry["path"]
        .as_str()
        .expect("need an explicit current report");
    assert!(relative.starts_with("tests/agent-eval/results/") && relative.ends_with("/result.json"));
    assert!(
        Path::new(relative)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    );
    assert!(!relative.contains('\\'));
    let expected = entry["sha256"]
        .as_str()
        .expect("need the current report fingerprint");
    let catalog_id = entry
        .get("catalog_id")
        .expect("declare the catalog association");
    if let Some(identifier) = catalog_id.as_str() {
        let matches: Vec<_> = catalog["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["id"] == identifier)
            .collect();
        assert_eq!(matches.len(), 1, "need one matching catalog entry");
        assert_eq!(matches[0]["path"].as_str().unwrap(), relative);
        assert_eq!(matches[0]["sha256"].as_str().unwrap(), expected);
    } else {
        assert!(
            catalog_id.is_null(),
            "catalog association must name an entry or explicitly use null"
        );
    }
    let path = root.join(relative);
    assert!(path.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()));
    assert_eq!(
        hex::encode(Sha256::digest(fs::read(&path).unwrap())),
        expected,
        "saved report differs from its explicit current fingerprint"
    );
    path.to_str().unwrap().to_owned()
}
