use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{args:?}: {error}: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        }),
    )
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (passed, report) = run(root, args);
    assert!(passed, "{args:?}: {report}");
    report
}

fn input(root: &Path) {
    fs::write(root.join("application.json"), json!({"schema": "fr-http-application-1", "routes": [
        {"method": "GET", "path": "/records/{id}", "status": 200, "response": {"kind": "object", "fields": {"id": {"kind": "path", "name": "id"}}}},
        {"method": "POST", "path": "/audit", "status": 201, "response": {"kind": "literal", "value": true}}
    ]}).to_string()).unwrap();
}

#[test]
fn application_adapters_use_review_history_and_owned_new_files() {
    for adapter in ["nextjs", "fastapi", "express", "go-net-http"] {
        let dir = tempfile::tempdir().unwrap();
        input(dir.path());
        let args = [
            "migrate",
            "application",
            "--ir",
            "application.json",
            "--to",
            adapter,
            "--out",
            "generated",
        ];
        let preview = ok(dir.path(), &args);
        assert_eq!(preview["schema"], "fr-application-migration-1");
        assert!(!dir.path().join("generated").exists());
        let mut save = args.to_vec();
        save.push("--save-plan");
        let saved = ok(dir.path(), &save);
        assert_eq!(saved["transaction"], 1);
        ok(dir.path(), &["history", "apply", "1", "--write"]);
        let files = preview["migration"]["files"].as_array().unwrap();
        for file in files {
            assert!(dir.path().join(file["path"].as_str().unwrap()).is_file());
        }
        let patch = ok(dir.path(), &["history", "patch", "1"]);
        assert!(patch["patch"].as_str().unwrap().contains("generated/"));
        ok(dir.path(), &["history", "undo", "1", "--write"]);
        for file in files {
            assert!(!dir.path().join(file["path"].as_str().unwrap()).exists());
        }
        ok(dir.path(), &["history", "redo", "1", "--write"]);
        for file in files {
            assert!(dir.path().join(file["path"].as_str().unwrap()).is_file());
        }
        assert!(dir.path().join("application.json").is_file());
        assert!(!run(dir.path(), &args).0);
    }
}

#[test]
fn application_hierarchy_preserves_every_fact_and_merkle_address() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("api.py"), "from fastapi import FastAPI\napp = FastAPI()\n@app.get('/records/{id}')\ndef show(id: str):\n    return {'id': id}\n").unwrap();
    let facts = ok(dir.path(), &["project", "features", "--limit", "500"]);
    let report = ok(dir.path(), &["project", "application"]);
    assert_eq!(report["model"]["runtime_proved"], false);
    let mut ids = BTreeSet::new();
    fn walk(node: &Value, ids: &mut BTreeSet<String>) {
        assert!(ids.insert(node["id"].as_str().unwrap().into()));
        for child in node["children"].as_array().unwrap() {
            walk(child, ids);
        }
    }
    for node in report["model"]["applications"].as_array().unwrap() {
        walk(node, &mut ids);
    }
    let expected: BTreeSet<String> = facts["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().into())
        .collect();
    assert_eq!(ids, expected);
    let map = ok(dir.path(), &["project", "map", "--depth", "0"]);
    let handle = map["root"].as_str().unwrap();
    let disclosed = ok(
        dir.path(),
        &["project", "disclose", handle, "--view", "application"],
    );
    assert_eq!(
        disclosed["commitment"]["object_root"],
        report["object_digest"]
    );
    assert!(serde_json::to_vec(&disclosed).unwrap().len() <= 4096);
    assert_eq!(disclosed["frontier"].as_array().unwrap().len(), 1);
    assert!(disclosed["application_shortcuts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["domain"] == "applications"));
}

#[test]
fn analysis_disclosure_uses_the_proven_depth_admission_policy() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        "def greeting():\n    return True\n",
    )
    .unwrap();
    let map = ok(dir.path(), &["project", "map", "--depth", "0"]);
    for view in ["project", "application"] {
        for depth in ["9", "18446744073709551615"] {
            let (passed, report) = run(
                dir.path(),
                &[
                    "project",
                    "disclose",
                    map["root"].as_str().unwrap(),
                    "--view",
                    view,
                    "--depth",
                    depth,
                ],
            );
            assert!(!passed);
            assert!(report["error"]["message"]
                .as_str()
                .unwrap()
                .contains("depth"));
        }
        ok(
            dir.path(),
            &[
                "project",
                "disclose",
                map["root"].as_str().unwrap(),
                "--view",
                view,
                "--depth",
                "8",
            ],
        );
    }
}

#[test]
fn application_hierarchy_drains_feature_pages_without_losing_identities() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = "from fastapi import FastAPI\napp = FastAPI()\n".to_owned();
    for index in 0..180 {
        source.push_str(&format!(
            "@app.get('/records-{index}')\ndef route_{index}():\n    return True\n"
        ));
    }
    fs::write(dir.path().join("api.py"), source).unwrap();
    let mut facts = ok(dir.path(), &["project", "features", "--limit", "500"]);
    assert!(facts["page"]["remaining"].as_u64().unwrap() > 0);
    let mut expected = BTreeSet::new();
    loop {
        for node in facts["items"].as_array().unwrap() {
            expected.insert(node["id"].as_str().unwrap().to_owned());
        }
        let Some(cursor) = facts["page"]["next"].as_str() else {
            break;
        };
        facts = ok(
            dir.path(),
            &["project", "features", "--limit", "500", "--cursor", cursor],
        );
    }
    let model = ok(dir.path(), &["project", "application"]);
    fn walk(node: &Value, ids: &mut BTreeSet<String>) {
        assert!(ids.insert(node["id"].as_str().unwrap().to_owned()));
        for child in node["children"].as_array().unwrap() {
            walk(child, ids);
        }
    }
    let mut actual = BTreeSet::new();
    for root in model["model"]["applications"].as_array().unwrap() {
        walk(root, &mut actual);
    }
    assert_eq!(actual, expected);
}
