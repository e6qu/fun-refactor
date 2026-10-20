use sha2::{Digest, Sha256};

#[test]
fn retained_host_recovery_evidence_matches_sources_and_oracles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("tests/agent-eval/roadmap.json")).unwrap())
            .unwrap();
    let entries: Vec<_> = catalog["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["id"] == "host-recovery")
        .collect();
    assert_eq!(entries.len(), 1);
    let path = entries[0]["path"].as_str().unwrap();
    assert!(
        path.starts_with("tests/agent-eval/results/") && !path.split('/').any(|part| part == "..")
    );
    assert_eq!(
        hex::encode(Sha256::digest(std::fs::read(root.join(path)).unwrap())),
        entries[0]["sha256"].as_str().unwrap()
    );
    let output = std::process::Command::new("python3")
        .args(["tools/host-recovery-acceptance.py", "--audit", path])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
