use serde_json::Value;
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
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (success, report) = run(root, args);
    assert!(success, "{args:?}: {report}");
    report
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/app.py"), "from lib import helper\n\nclass Service:\n    def run(self, name: str) -> str:\n        local = 'λ名'\n        return helper(name) + local\n\ndef check():\n    return Service().run('ok')\n").unwrap();
    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name: str) -> str:\n    return name\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# Example\n\nA service fixture.\n",
    )
    .unwrap();
    dir
}

fn rows(report: &Value) -> Vec<Value> {
    let columns = report["columns"].as_array().unwrap();
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut result = serde_json::Map::new();
            for (column, cell) in columns.iter().zip(row.as_array().unwrap()) {
                result.insert(column.as_str().unwrap().to_owned(), cell.clone());
            }
            Value::Object(result)
        })
        .collect()
}

fn mapped(root: &Path) -> Value {
    ok(
        root,
        &[
            "project",
            "map",
            "src/app.py",
            "--depth",
            "8",
            "--fields",
            "handle,parent,kind,name,children,signature",
        ],
    )
}

fn handle(report: &Value, name: &str) -> String {
    rows(report)
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("missing {name}: {report}"))
        .get("handle")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn map_shows_lexical_hierarchy_hides_locals_and_gives_headers_without_bodies() {
    let dir = fixture();
    let report = mapped(dir.path());
    assert_eq!(report["schema"], "fr-project-1");
    let class = handle(&report, "Service");
    let method = handle(&report, "run");
    let records = rows(&report);
    let method_row = records.iter().find(|r| r["handle"] == method).unwrap();
    assert_eq!(method_row["parent"], class.rsplit(':').next().unwrap());
    assert_eq!(method_row["signature"]["basis"], "syntax-header");
    let signature = method_row["signature"]["text"].as_str().unwrap();
    assert!(signature.contains("name: str"), "{signature}");
    assert!(!signature.contains("local"), "{signature}");
    assert!(report["omitted"]["locals"].as_u64().unwrap() > 0);
    assert!(!records.iter().any(|r| r["name"] == "local"));
    let locals = ok(
        dir.path(),
        &["project", "map", "src/app.py", "--depth", "8", "--locals"],
    );
    assert!(rows(&locals).iter().any(|r| r["name"] == "local"));
    let subtree = ok(dir.path(), &["project", "map", &class, "--depth", "1"]);
    assert_eq!(subtree["root"], class);
    assert!(rows(&subtree).iter().any(|r| r["name"] == "run"));
}

#[test]
fn cursor_pages_cover_results_once_and_refuse_a_changed_query_or_revision() {
    let dir = fixture();
    let expected = ok(
        dir.path(),
        &["project", "map", "--depth", "8", "--limit", "500"],
    );
    let mut page = ok(
        dir.path(),
        &["project", "map", "--depth", "8", "--limit", "2"],
    );
    let first_cursor = page["page"]["next"].as_str().unwrap().to_owned();
    let mut combined = Vec::new();
    loop {
        combined.extend(rows(&page));
        let Some(next) = page["page"]["next"].as_str() else {
            break;
        };
        page = ok(
            dir.path(),
            &[
                "project", "map", "--depth", "8", "--limit", "2", "--cursor", next,
            ],
        );
    }
    assert_eq!(combined, rows(&expected));
    assert!(
        !run(
            dir.path(),
            &["project", "map", "--depth", "1", "--cursor", &first_cursor]
        )
        .0
    );
    fs::write(dir.path().join("src/new.py"), "def added(): pass\n").unwrap();
    let (success, error) = run(
        dir.path(),
        &["project", "map", "--depth", "8", "--cursor", &first_cursor],
    );
    assert!(!success);
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale"));
}

#[test]
fn source_pages_reassemble_unicode_and_show_has_no_body_by_default() {
    let dir = fixture();
    let report = mapped(dir.path());
    let method = handle(&report, "run");
    let plain = ok(dir.path(), &["project", "show", &method]);
    assert!(plain.get("source").is_none());
    assert_eq!(
        plain["node"]["position"],
        serde_json::json!({"line": 4, "col": 9})
    );
    let expected = ok(dir.path(), &["project", "show", &method, "--source"]);
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let page = ok(
            dir.path(),
            &[
                "project",
                "show",
                &method,
                "--source",
                "--bytes",
                "4",
                "--offset",
                &offset.to_string(),
            ],
        );
        let part = page["source"]["text"].as_str().unwrap();
        assert!(part.len() <= 4);
        text.push_str(part);
        let Some(next) = page["source"]["next_offset"].as_u64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(text, expected["source"]["text"]);
    let invalid = text.find('λ').unwrap() + 1;
    assert!(
        !run(
            dir.path(),
            &[
                "project",
                "show",
                &method,
                "--source",
                "--offset",
                &invalid.to_string()
            ]
        )
        .0
    );
    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name): return name.upper()\n",
    )
    .unwrap();
    assert!(!run(dir.path(), &["project", "show", &method, "--source"]).0);
}

#[test]
fn relations_are_bounded_and_preserve_targets_confidence_and_unknowns() {
    let dir = fixture();
    let report = mapped(dir.path());
    let method = handle(&report, "run");
    let shown = ok(
        dir.path(),
        &["project", "show", &method, "--relations", "--limit", "500"],
    );
    let items = shown["relations"]["items"].as_array().unwrap();
    let call = items
        .iter()
        .find(|r| r["direction"] == "outgoing" && r["name"] == "helper")
        .unwrap();
    assert_eq!(call["confidence"], "import-qualified");
    let destination = ok(
        dir.path(),
        &["project", "show", call["target"].as_str().unwrap()],
    );
    assert_eq!(destination["node"]["name"], "helper");
    assert!(items.iter().any(|r| r["direction"] == "file-import"));
    let limited = ok(
        dir.path(),
        &["project", "show", &method, "--relations", "--limit", "1"],
    );
    assert_eq!(limited["relations"]["items"].as_array().unwrap().len(), 1);
    let cursor = limited["relations"]["page"]["next"].as_str().unwrap();
    let next = ok(
        dir.path(),
        &[
            "project",
            "show",
            &method,
            "--relations",
            "--limit",
            "1",
            "--cursor",
            cursor,
        ],
    );
    assert_eq!(next["relations"]["page"]["before"], 1);
}

#[test]
fn coverage_exposes_syntax_gaps_size_exclusions_symlinks_and_unknown_extensions() {
    let dir = fixture();
    fs::write(dir.path().join("broken.rs"), "fn broken( {\n").unwrap();
    fs::write(dir.path().join("large.rs"), "fn large() {}\n".repeat(40)).unwrap();
    fs::write(dir.path().join("opaque.blob"), "abc").unwrap();
    std::os::unix::fs::symlink(dir.path().join("src/lib.py"), dir.path().join("alias.py")).unwrap();
    let report = ok(dir.path(), &["--max-file-size", "256", "project", "map"]);
    assert_eq!(report["coverage"]["skipped_files"], 1);
    assert_eq!(report["coverage"]["skipped_symlinks"], 1);
    assert_eq!(report["coverage"]["unsupported_files"], 1);
    assert_eq!(
        report["coverage"]["files_by_gap"]["file has syntax errors"],
        1
    );
    let gaps = ok(dir.path(), &["--max-file-size", "256", "project", "gaps"]);
    assert!(gaps["items"].to_string().contains("large.rs"));
    assert!(gaps["items"].to_string().contains("broken.rs"));
    assert!(gaps["items"].to_string().contains("alias.py"));
}

#[test]
fn limits_projection_and_read_only_contract_hold() {
    let dir = fixture();
    let shallow = ok(
        dir.path(),
        &["project", "map", "--depth", "0", "--fields", "kind,name"],
    );
    assert_eq!(shallow["columns"], serde_json::json!(["kind", "name"]));
    assert_eq!(shallow["rows"], serde_json::json!([["directory", "."]]));
    assert!(shallow["omitted"]["depth"].as_u64().unwrap() > 0);
    for args in [
        vec!["project", "map", "--limit", "0"],
        vec!["project", "map", "--depth", "65"],
    ] {
        assert!(!run(dir.path(), &args).0);
    }
    assert!(!dir.path().join(".fr-history").exists());
}

#[test]
fn signature_headers_keep_multiline_types_and_do_not_include_one_line_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let samples = [
        ("main.rs", "pub fn process<T: Clone>(\n    value: T,\n) -> T { let secret = 42; value }\n", "process"),
        ("main.go", "package main\nfunc Process(value string) string { secret := 42; _ = secret; return value }\n", "Process"),
        ("main.ts", "export function process(value: { label: string }): string { const secret = 42; return value.label; }\n", "process"),
        ("Main.java", "class Main { public String process(String value) { int secret = 42; return value; } }\n", "process"),
    ];
    for (name, source, symbol) in samples {
        fs::write(dir.path().join(name), source).unwrap();
        let report = ok(
            dir.path(),
            &[
                "project",
                "map",
                name,
                "--depth",
                "8",
                "--fields",
                "handle,name,signature",
            ],
        );
        let records = rows(&report);
        let row = records.iter().find(|r| r["name"] == symbol).unwrap();
        assert_eq!(
            row["signature"]["basis"], "syntax-header",
            "{name}: {report}"
        );
        let header = row["signature"]["text"].as_str().unwrap();
        assert!(!header.contains("secret"), "{name}: {header}");
        assert!(header.contains("value"), "{name}: {header}");
    }
}

#[test]
fn large_labels_and_signatures_report_their_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let name = "n".repeat(1200);
    fs::write(
        dir.path().join("long.py"),
        format!("def {name}(value: str) -> str:\n    return value\n"),
    )
    .unwrap();
    let report = ok(
        dir.path(),
        &[
            "project",
            "map",
            "long.py",
            "--fields",
            "handle,kind,name,signature",
        ],
    );
    let records = rows(&report);
    let function = records.iter().find(|r| r["kind"] == "function").unwrap();
    assert_eq!(function["name"]["text"].as_str().unwrap().len(), 160);
    assert_eq!(function["name"]["omitted_bytes"], 1040);
    assert!(
        function["signature"]["text"]["omitted_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(serde_json::to_vec(&report).unwrap().len() < 2200);
}

#[test]
fn a_query_refuses_source_and_inventory_changes_before_emitting_its_report() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    project.verify(&root).unwrap();
    fs::write(root.join("added.py"), "def added(): pass\n").unwrap();
    assert!(project.verify(&root).is_err());
    fs::remove_file(root.join("added.py")).unwrap();
    fs::write(root.join("src/lib.py"), "def helper(): pass\n").unwrap();
    assert!(project.verify(&root).is_err());
    assert!(Project::new(&root, &index, &scanned, &options).is_err());
}

#[test]
fn short_ids_require_the_revision_and_cannot_drift_to_another_symbol() {
    let dir = fixture();
    let map = ok(
        dir.path(),
        &["project", "map", "src/app.py", "--depth", "8"],
    );
    let records = rows(&map);
    let method = records.iter().find(|r| r["name"] == "run").unwrap();
    let id = method["id"].as_str().unwrap();
    let revision = map["revision"].as_str().unwrap();
    assert!(!run(dir.path(), &["project", "show", id]).0);
    let shown = ok(dir.path(), &["project", "show", id, "--revision", revision]);
    assert_eq!(shown["node"]["name"], "run");
    let subtree = ok(dir.path(), &["project", "map", id, "--revision", revision]);
    assert_eq!(rows(&subtree)[0]["name"], "run");
    let full = format!("{}{id}", map["handle_prefix"].as_str().unwrap());
    assert_eq!(shown["node"]["handle"], full);
    fs::write(dir.path().join("src/lib.py"), "def different(): pass\n").unwrap();
    assert!(!run(dir.path(), &["project", "show", id, "--revision", revision]).0);
}

#[test]
fn an_inspected_location_drives_a_checked_refactoring_without_source_retrieval() {
    let dir = fixture();
    let map = ok(dir.path(), &["project", "map", "src/lib.py"]);
    let records = rows(&map);
    let symbol = records.iter().find(|r| r["name"] == "helper").unwrap();
    let shown = ok(
        dir.path(),
        &[
            "project",
            "show",
            symbol["id"].as_str().unwrap(),
            "--revision",
            map["revision"].as_str().unwrap(),
        ],
    );
    assert!(shown.get("source").is_none());
    let node = &shown["node"];
    let target = format!(
        "{}:{}:{}",
        node["path"].as_str().unwrap(),
        node["position"]["line"],
        node["position"]["col"]
    );
    let plan = ok(dir.path(), &["rename", &target, "decorate"]);
    assert_eq!(plan["applied"], false);
    assert_eq!(plan["files_changed"], 2);
    assert!(plan["changes"].to_string().contains("decorate"));
    assert!(fs::read_to_string(dir.path().join("src/lib.py"))
        .unwrap()
        .contains("def helper"));
}

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn package_views_preserve_cargo_and_npm_declarations_without_resolving_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        r#"
[workspace]
members = ["crates/*"]
exclude = ["crates/old"]
default-members = ["crates/core"]
[workspace.dependencies]
serde = "1"
[package]
name = "server"
version.workspace = true
# Package dependencies.
[dependencies]
serde = { workspace = true, features = ["derive"] }
util = { package = "actual-util", path = "../external", optional = true, default-features = false }
remote = { git = "https://example.test/repo", branch = "main" }
[dev-dependencies]
test-helper = "2"
[target.'cfg(unix)'.build-dependencies]
cc = { version = "1", features = [] }
"#,
    );
    put(
        root,
        "crates/core/Cargo.toml",
        "[package]\nname = 'core'\nversion = '0.1.0'\n",
    );
    put(
        root,
        "web/package.json",
        r#"{"name":"web","version":"1.0.0","private":true,"workspaces":{"packages":["apps/*"]},"dependencies":{"react":"^19"},"devDependencies":{"typescript":"*"},"peerDependencies":{"host":"workspace:*"},"optionalDependencies":{"local":"file:../local"}}"#,
    );
    let packages = ok(root, &["project", "packages"]);
    assert_eq!(packages["page"]["total"], 3);
    assert_eq!(packages["coverage"]["manifests"]["gaps"], 0);
    assert_eq!(packages["items"][0]["name"], "server");
    assert_eq!(packages["items"][0]["version"], Value::Null);
    assert_eq!(packages["items"][0]["version_inherited"], true);
    assert_eq!(packages["items"][1]["root"], "crates/core");
    assert_eq!(packages["items"][2]["private"], true);
    let dependencies = ok(
        root,
        &["project", "dependencies", "--manifest", "./Cargo.toml"],
    );
    let items = dependencies["items"].as_array().unwrap();
    assert_eq!(items.len(), 9);
    assert!(items.iter().any(|r| r["name"] == "util"
        && r["package"] == "actual-util"
        && r["path"] == "../external"
        && r["optional"] == true
        && r["default-features"] == false));
    assert!(items
        .iter()
        .any(|r| r["name"] == "serde" && r["workspace"] == true && r["feature_count"] == 1));
    assert!(items
        .iter()
        .any(|r| r["name"] == "serde" && r["scope"] == "workspace"));
    assert!(items.iter().any(|r| r["name"] == "cc"
        && r["scope"] == "target"
        && r["target_condition"] == "cfg(unix)"));
    assert!(items.iter().any(|r| r["kind"] == "workspace-member-pattern"
        && r["pattern"] == "crates/*"
        && r["expanded"] == false));
    for item in items {
        assert_eq!(item["basis"], "manifest-declaration");
        if item["kind"] == "dependency" {
            assert_eq!(item["resolution"], "not-attempted");
            assert!(item.get("resolved_target").is_none());
        }
    }
    let npm = ok(
        root,
        &["project", "dependencies", "--manifest", "web/package.json"],
    );
    assert_eq!(npm["page"]["total"], 5);
    assert!(npm["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["requirement"] == "workspace:*"));
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn manifest_pages_are_bounded_complete_and_bound_to_the_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for n in 0..3 {
        put(
            root,
            &format!("p{n}/package.json"),
            r#"{"dependencies":{"a":"1","b":"2","c":"3"}}"#,
        );
    }
    for query in ["packages", "dependencies"] {
        let full = ok(root, &["project", query]);
        let mut current = ok(root, &["project", query, "--limit", "1"]);
        let mut collected = Vec::new();
        loop {
            collected.extend(current["items"].as_array().unwrap().clone());
            let Some(cursor) = current["page"]["next"].as_str() else {
                break;
            };
            current = ok(
                root,
                &["project", query, "--limit", "2", "--cursor", cursor],
            );
            assert!(current["items"].as_array().unwrap().len() <= 2);
        }
        assert_eq!(collected, *full["items"].as_array().unwrap());
        assert!(!run(root, &["project", query, "--limit", "0"]).0);
        assert!(!run(root, &["project", query, "--limit", "501"]).0);
    }
    let filtered = ok(
        root,
        &[
            "project",
            "dependencies",
            "--manifest",
            "p0/package.json",
            "--limit",
            "1",
        ],
    );
    let cursor = filtered["page"]["next"].as_str().unwrap();
    assert!(
        !run(
            root,
            &[
                "project",
                "dependencies",
                "--manifest",
                "p1/package.json",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "dependencies", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "packages", "--cursor", cursor]).0);
    assert!(
        !run(
            root,
            &["project", "dependencies", "--manifest", "missing.json"]
        )
        .0
    );
    put(
        root,
        "long/package.json",
        &serde_json::json!({"name": "λ".repeat(300), "dependencies": {"long": "λ".repeat(1000)}})
            .to_string(),
    );
    let long = ok(
        root,
        &["project", "dependencies", "--manifest", "long/package.json"],
    );
    assert_eq!(
        long["items"][0]["requirement"]["text"]
            .as_str()
            .unwrap()
            .len(),
        512
    );
    assert_eq!(long["items"][0]["requirement"]["omitted_bytes"], 1488);
}

#[test]
fn toml_content_and_manifest_inventory_changes_invalidate_project_handles_and_cursors() {
    let dir = fixture();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[package]\nname = 'old'\n[dependencies]\na = '1'\nb = '2'\n",
    );
    put(root, "other/Cargo.toml", "[workspace]\n");
    let map = mapped(root);
    let handle = handle(&map, "run");
    let old = ok(root, &["project", "packages", "--limit", "1"]);
    let deps = ok(root, &["project", "dependencies", "--limit", "1"]);
    let cursor = old["page"]["next"].as_str().unwrap();
    put(
        root,
        "Cargo.toml",
        "[package]\nname = 'new'\n[dependencies]\na = '1'\nb = '2'\n",
    );
    assert!(!run(root, &["project", "show", &handle]).0);
    assert!(!run(root, &["project", "packages", "--cursor", cursor]).0);
    assert!(
        !run(
            root,
            &[
                "project",
                "dependencies",
                "--cursor",
                deps["page"]["next"].as_str().unwrap()
            ]
        )
        .0
    );
    let before = ok(root, &["project", "packages"]);
    fs::rename(root.join("other/Cargo.toml"), root.join("other/other.toml")).unwrap();
    let after = ok(root, &["project", "packages"]);
    assert_ne!(before["revision"], after["revision"]);
    assert_eq!(after["page"]["total"], 1);
}

#[test]
fn package_gaps_cover_bad_syntax_and_unsupported_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "invalid/Cargo.toml", "[package\n");
    put(root, "invalid/package.json", "[]");
    put(root, "Cargo.toml", "[package]\nname = 42\n[dependencies]\nbad = 1\n# Partial fields.\nfields = { path = false, optional = 'yes', features = [1], future = true }\n[workspace]\nmembers = [1, 'valid/*']\n");
    put(
        root,
        "package.json",
        r#"{"dependencies":[],"workspaces":{},"private":"yes"}"#,
    );
    let packages = ok(root, &["project", "packages"]);
    assert_eq!(packages["coverage"]["manifests"]["discovered"], 4);
    assert_eq!(packages["coverage"]["manifests"]["parsed"], 2);
    assert!(packages["coverage"]["manifests"]["gaps"].as_u64().unwrap() >= 10);
    let gaps = ok(root, &["project", "gaps"]);
    let manifest_gaps: Vec<_> = gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["scope"] == "manifest")
        .collect();
    assert!(manifest_gaps
        .iter()
        .any(|r| r["reason"] == "invalid TOML syntax"));
    assert!(manifest_gaps
        .iter()
        .any(|r| r["reason"] == "invalid JSON object"));
    let declarations = ok(root, &["project", "dependencies"]);
    assert!(declarations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["declaration_status"] == "unsupported"));
    assert!(declarations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["unreported_fields"] == 1));
}

#[test]
fn manifest_discovery_obeys_scan_scope_ignore_size_and_history_exclusion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\n");
    put(root, "src/main.rs", "fn main() {}\n");
    put(root, "ignored/package.json", "{}");
    put(root, ".hidden/package.json", "{}");
    put(root, ".fr-history/package.json", "{}");
    put(root, ".gitignore", "ignored/\n");
    put(root, "big/Cargo.toml", &format!("#{}\n", "x".repeat(200)));
    put(root, "bad/Cargo.toml", "");
    fs::write(root.join("bad/Cargo.toml"), [0xff]).unwrap();
    let ordinary = ok(root, &["--max-file-size", "100", "project", "packages"]);
    assert_eq!(ordinary["page"]["total"], 1);
    assert_eq!(ordinary["coverage"]["manifests"]["discovered"], 3);
    assert_eq!(ordinary["coverage"]["manifests"]["gaps"], 2);
    let all = ok(
        root,
        &[
            "--no-ignore",
            "--max-file-size",
            "100",
            "project",
            "packages",
        ],
    );
    assert_eq!(all["page"]["total"], 3);
    assert_eq!(all["coverage"]["manifests"]["discovered"], 5);
    let file = ok(&root.join("src/main.rs"), &["project", "packages"]);
    assert_eq!(file["page"]["total"], 0);
    let manifest = ok(&root.join("Cargo.toml"), &["project", "packages"]);
    assert_eq!(manifest["page"]["total"], 1);
}

#[cfg(unix)]
#[test]
fn manifest_symlinks_are_reported_without_reading_their_targets() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    put(
        outside.path(),
        "Cargo.toml",
        "[package]\nname = 'outside'\n",
    );
    symlink(
        outside.path().join("Cargo.toml"),
        dir.path().join("Cargo.toml"),
    )
    .unwrap();
    symlink(outside.path(), dir.path().join("linked")).unwrap();
    let packages = ok(dir.path(), &["project", "packages"]);
    assert_eq!(packages["page"]["total"], 0);
    assert_eq!(packages["coverage"]["manifests"]["discovered"], 1);
    let gaps = ok(dir.path(), &["project", "gaps"]);
    assert!(gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["path"] == "Cargo.toml" && r["reason"] == "symlink manifest"));
    assert!(
        !run(
            dir.path(),
            &["project", "dependencies", "--manifest", "Cargo.toml"]
        )
        .0
    );
}

#[test]
fn project_verification_refuses_manifest_content_and_inventory_races() {
    use fun_refactor::index::Index;
    use fun_refactor::project::Project;
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let source = "[package]\nname = 'original'\n";
    put(&root, "Cargo.toml", source);
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    project.verify(&root).unwrap();
    put(&root, "Cargo.toml", "[package]\nname = 'modified'\n");
    assert!(project.verify(&root).is_err());
    put(&root, "Cargo.toml", source);
    project.verify(&root).unwrap();
    fs::rename(root.join("Cargo.toml"), root.join("other.toml")).unwrap();
    assert!(project.verify(&root).is_err());
    fs::rename(root.join("other.toml"), root.join("Cargo.toml")).unwrap();
    put(&root, "nested/Cargo.toml", "[workspace]\n");
    assert!(project.verify(&root).is_err());
}

#[test]
fn local_links_keep_manifest_identity_separate_from_version_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        r#"
[package]
name = 'app'
[dependencies]
# Local alias.
alias = { package = 'core', path = 'crates/core', version = '999' }
wrong = { path = 'crates/core' }
# Source declarations.
remote = '1'
inherited = { workspace = true }
conflict = { path = 'crates/core', git = 'https://example.test/repo' }
# Platform dependencies.
[target.'cfg(unix)'.build-dependencies]
core = { path = 'crates/core' }
[workspace.dependencies]
core = { path = 'crates/core' }
"#,
    );
    put(
        root,
        "crates/core/Cargo.toml",
        "[package]\nname = 'core'\nversion = '1.0.0'\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let items = report["items"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    let alias = items.iter().find(|r| r["name"] == "alias").unwrap();
    assert_eq!(alias["status"], "linked");
    assert_eq!(alias["target_manifest"], "crates/core/Cargo.toml");
    assert_eq!(alias["version_check"], "not-performed");
    assert!(items.iter().any(|r| r["name"] == "wrong"
        && r["reason"] == "package-name-mismatch"
        && r["target_manifest"].is_null()));
    assert!(items
        .iter()
        .any(|r| r["name"] == "inherited" && r["reason"] == "workspace-dependency-not-declared"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "conflict" && r["reason"] == "conflicting-dependency-sources"));
    assert!(items.iter().any(|r| r["scope"] == "target"
        && r["target_condition"] == "cfg(unix)"
        && r["status"] == "linked"));
    assert!(items
        .iter()
        .any(|r| r["scope"] == "workspace" && r["status"] == "linked"));
}

#[test]
fn npm_file_links_allow_aliases_and_workspace_matches_keep_directory_depth() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "package.json",
        r#"{"workspaces":["apps/*","apps/*/nested","apps/other*","missing/*"],"dependencies":{"alias":"file:apps/a","relative":"./apps/b","registry":"1","workspace":"workspace:*","archive":"file:pkg.tgz"}}"#,
    );
    put(root, "apps/a/package.json", r#"{"name":"actual-a"}"#);
    put(root, "apps/b/package.json", r#"{"name":"b"}"#);
    put(root, "apps/a/nested/package.json", "{}");
    put(root, "apps/rust/Cargo.toml", "[package]\nname='rust'\n");
    let report = ok(root, &["project", "links", "--manifest", "package.json"]);
    let items = report["items"].as_array().unwrap();
    assert!(items.iter().any(|r| r["name"] == "alias"
        && r["target_manifest"] == "apps/a/package.json"
        && r["target_name"] == "actual-a"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "relative" && r["target_manifest"] == "apps/b/package.json"));
    assert!(!items
        .iter()
        .any(|r| r["name"] == "registry" || r["name"] == "workspace"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "archive" && r["status"] == "unresolved"));
    let matched: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "workspace-member-match" && r["status"] == "matched")
        .collect();
    assert_eq!(matched.len(), 3);
    assert_eq!(
        matched.iter().filter(|r| r["pattern"] == "apps/*").count(),
        2
    );
    assert!(matched.iter().all(|r| r["membership"] == "candidate"));
    assert!(items
        .iter()
        .any(|r| r["pattern"] == "apps/other*" && r["reason"] == "pattern-syntax-unsupported"));
    assert!(items
        .iter()
        .any(|r| r["pattern"] == "missing/*" && r["reason"] == "no-observed-package-match"));
}

#[test]
fn workspace_exclusions_and_ownership_ambiguity_prevent_confirmed_matches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/old']\n",
    );
    put(root, "crates/ok/Cargo.toml", "[package]\nname='ok'\n");
    put(root, "crates/old/Cargo.toml", "[package]\nname='old'\n");
    put(
        root,
        "crates/nested/Cargo.toml",
        "[package]\nname='nested'\n[workspace]\n",
    );
    put(
        root,
        "crates/elsewhere/Cargo.toml",
        "[package]\nname='elsewhere'\nworkspace='../elsewhere'\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let items = report["items"].as_array().unwrap();
    assert_eq!(items.iter().filter(|r| r["status"] == "matched").count(), 1);
    assert!(items
        .iter()
        .any(|r| r["status"] == "excluded" && r["excluded_by"] == "crates/old"));
    assert_eq!(
        items
            .iter()
            .filter(|r| r["reason"] == "workspace-ownership-unresolved")
            .count(),
        2
    );
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/**']\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved" && r["reason"] == "unsupported-exclusion"));
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=[42,'../elsewhere','crates/?','crates/[ab]']\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert_eq!(report["page"]["total"], 4);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
}

#[test]
fn link_paths_cannot_escape_the_snapshot_or_normalize_through_unobserved_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/Cargo.toml", "[package]\nname='app'\n[dependencies]\ncore={path='../core'}\nescape={path='../../outside'}\nmissing={path='../missing/../core',package='core'}\ninvalid={path=42}\npercent={path='../%63ore'}\n");
    put(root, "core/Cargo.toml", "[package]\nname='core'\n");
    let report = ok(root, &["project", "links"]);
    let items = report["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["name"] == "core" && r["status"] == "linked"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "escape" && r["reason"] == "outside-selected-root"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "missing" && r["reason"] == "directory-not-observed"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "invalid" && r["reason"] == "invalid-path-field"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "percent" && r["reason"] == "path-syntax-unsupported"));
    let scoped = ok(&root.join("app"), &["project", "links"]);
    assert!(scoped["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
    assert!(!root.join(".fr-history").exists());
}

#[cfg(unix)]
#[test]
fn links_do_not_follow_symlinks_or_ignored_manifests() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[package]\nname='app'\n[dependencies]\nlinked={package='core',path='linked'}\nbypass={package='core',path='linked/../core'}\nignored={path='ignored'}\nskipped={path='skipped'}\n");
    put(root, "core/Cargo.toml", "[package]\nname='core'\n");
    put(root, "ignored/Cargo.toml", "[package]\nname='ignored'\n");
    put(root, ".gitignore", "ignored/\n");
    fs::create_dir(root.join("skipped")).unwrap();
    symlink(root.join("core"), root.join("linked")).unwrap();
    symlink(
        root.join("core/Cargo.toml"),
        root.join("skipped/Cargo.toml"),
    )
    .unwrap();
    let report = ok(root, &["project", "links"]);
    assert_eq!(report["page"]["total"], 4);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
}

#[test]
fn link_pages_are_complete_query_bound_and_invalidated_by_target_manifest_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[package]\nname='app'\n[dependencies]\na={path='a'}\nb={path='b'}\n[workspace]\nmembers=['*','a']\n");
    put(root, "a/Cargo.toml", "[package]\nname='a'\n");
    put(root, "b/Cargo.toml", "[package]\nname='b'\n");
    let full = ok(root, &["project", "links"]);
    let first = ok(root, &["project", "links", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut page = first.clone();
    let mut items = Vec::new();
    loop {
        items.extend(page["items"].as_array().unwrap().clone());
        let Some(next) = page["page"]["next"].as_str() else {
            break;
        };
        page = ok(
            root,
            &["project", "links", "--limit", "2", "--cursor", next],
        );
    }
    assert_eq!(items, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &[
                "project",
                "links",
                "--manifest",
                "a/Cargo.toml",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "dependencies", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "links", "--limit", "0"]).0);
    assert!(!run(root, &["project", "links", "--limit", "501"]).0);
    put(root, "a/Cargo.toml", "[package]\nname='renamed'\n");
    assert!(!run(root, &["project", "links", "--cursor", cursor]).0);
    let updated = ok(root, &["project", "links"]);
    assert!(updated["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "a" && r["reason"] == "package-name-mismatch"));
}

#[test]
fn link_resolution_uses_full_values_before_clipping_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let directory = (0..5)
        .map(|_| "p".repeat(120))
        .collect::<Vec<_>>()
        .join("/");
    let name = "n".repeat(300);
    put(
        root,
        "Cargo.toml",
        &format!("[package]\nname='app'\n[dependencies]\n{name}={{path='{directory}'}}\n"),
    );
    put(
        root,
        &format!("{directory}/Cargo.toml"),
        &format!("[package]\nname='{name}'\n"),
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert_eq!(report["items"][0]["status"], "linked");
    assert_eq!(report["items"][0]["name"]["omitted_bytes"], 140);
    assert_eq!(
        report["items"][0]["target_manifest"]["text"]
            .as_str()
            .unwrap()
            .len(),
        512
    );
}

#[test]
fn simple_workspace_pattern_matches_agree_with_cargo_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/old']\nresolver='2'\n",
    );
    for name in ["a", "b", "old"] {
        put(
            root,
            &format!("crates/{name}/Cargo.toml"),
            &format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
        );
        put(
            root,
            &format!("crates/{name}/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        );
    }
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let canonical = root.canonicalize().unwrap();
    let expected: std::collections::BTreeSet<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            metadata["workspace_members"]
                .as_array()
                .unwrap()
                .contains(&p["id"])
        })
        .map(|p| {
            Path::new(p["manifest_path"].as_str().unwrap())
                .strip_prefix(&canonical)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let matched: std::collections::BTreeSet<_> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "matched")
        .map(|r| r["target_manifest"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(matched, expected);
}

fn cargo_package(root: &Path, path: &str, name: &str, extra: &str) {
    put(
        root,
        &format!("{path}/Cargo.toml"),
        &format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n{extra}"),
    );
    put(
        root,
        &format!("{path}/src/lib.rs"),
        "pub fn value() -> u32 { 1 }\n",
    );
}

#[test]
fn inherited_local_paths_and_transitive_members_agree_with_cargo_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\nmembers=['app']\nresolver='2'\n[workspace.dependencies]\nalias={package='local-lib',path='lib',features=['shared']}\n");
    cargo_package(
        root,
        "app",
        "app",
        "[dependencies]\nalias={workspace=true,features=['local'],optional=true}\n",
    );
    cargo_package(
        root,
        "lib",
        "local-lib",
        "[dependencies]\nleaf={path='../leaf'}\n[features]\nshared=[]\nlocal=[]\n",
    );
    cargo_package(root, "leaf", "leaf", "");
    cargo_package(root, "unused", "unused", "");
    let metadata_output = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        metadata_output.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata_output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&metadata_output.stdout).unwrap();
    let canonical = root.canonicalize().unwrap();
    let expected: std::collections::BTreeSet<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            metadata["workspace_members"]
                .as_array()
                .unwrap()
                .contains(&p["id"])
        })
        .map(|p| {
            Path::new(p["manifest_path"].as_str().unwrap())
                .strip_prefix(&canonical)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let view = ok(root, &["project", "workspaces"]);
    let actual: std::collections::BTreeSet<_> = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "member")
        .map(|r| r["manifest"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["manifest"] == "leaf/Cargo.toml"
            && r["membership_basis"] == "automatic-path-member"
            && r["via_manifest"] == "lib/Cargo.toml"));
    let links = ok(root, &["project", "links", "--manifest", "app/Cargo.toml"]);
    let inherited = &links["items"][0];
    assert_eq!(inherited["status"], "linked");
    assert_eq!(inherited["target_manifest"], "lib/Cargo.toml");
    assert_eq!(inherited["basis"], "workspace-inheritance");
    assert_eq!(inherited["workspace_manifest"], "Cargo.toml");
    assert_eq!(inherited["features_check"], "not-performed");
    let app = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "app")
        .unwrap();
    assert_eq!(
        Path::new(app["dependencies"][0]["path"].as_str().unwrap()),
        canonical.join("lib")
    );
    assert_eq!(app["dependencies"][0]["rename"], "alias");
}

#[test]
fn nearest_workspace_and_explicit_workspace_pointer_choose_the_inheritance_basis() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['nested/app']\n[workspace.dependencies]\ncore={path='outer'}\n",
    );
    put(
        root,
        "nested/Cargo.toml",
        "[workspace]\nmembers=['app']\n[workspace.dependencies]\ncore={path='inner'}\n",
    );
    cargo_package(root, "outer", "core", "");
    cargo_package(root, "nested/inner", "core", "");
    cargo_package(
        root,
        "nested/app",
        "app",
        "[dependencies]\ncore={workspace=true}\n",
    );
    let first = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(
        first["items"][0]["target_manifest"],
        "nested/inner/Cargo.toml"
    );
    put(
        root,
        "nested/app/Cargo.toml",
        "[package]\nname='app'\nworkspace='../..'\n[dependencies]\ncore={workspace=true}\n",
    );
    let second = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(second["items"][0]["target_manifest"], "outer/Cargo.toml");
    let ownership = ok(
        root,
        &[
            "project",
            "workspaces",
            "--manifest",
            "nested/app/Cargo.toml",
        ],
    );
    assert_eq!(
        ownership["items"][0]["ownership_basis"],
        "package-workspace"
    );
    assert_eq!(ownership["items"][0]["workspace_manifest"], "Cargo.toml");
}

#[test]
fn ignored_and_malformed_ancestor_manifests_block_inheritance_from_farther_roots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['nested/app']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    cargo_package(root, "core", "core", "");
    cargo_package(
        root,
        "nested/app",
        "app",
        "[dependencies]\ncore={workspace=true}\n",
    );
    put(root, "nested/Cargo.toml", "[workspace]\nmembers=['app']\n");
    put(root, ".gitignore", "/nested/Cargo.toml\n");
    let ignored = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(ignored["items"][0]["status"], "unresolved");
    assert_eq!(
        ignored["items"][0]["reason"],
        "workspace-ancestor-unavailable"
    );
    let gaps = ok(root, &["project", "gaps"]);
    assert!(gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["path"] == "nested/Cargo.toml"
            && r["reason"] == "workspace ancestor excluded or unavailable"));
    put(root, ".gitignore", "");
    put(root, "nested/Cargo.toml", "[workspace\n");
    let malformed = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(
        malformed["items"][0]["reason"],
        "workspace-ancestor-unavailable"
    );
    fs::remove_file(root.join("nested/Cargo.toml")).unwrap();
    let unblocked = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(unblocked["items"][0]["status"], "linked");
    let scoped = ok(&root.join("nested/app"), &["project", "links"]);
    assert_eq!(scoped["items"][0]["reason"], "workspace-root-not-observed");
}

#[test]
fn inheritance_refuses_unsupported_overrides_and_nonlocal_workspace_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\nmembers=['app']\n[workspace.dependencies]\nremote='1'\ninvalid={path='core',optional=true}\ncore={path='core'}\n");
    cargo_package(root, "core", "core", "");
    cargo_package(root, "app", "app", "[dependencies]\ncore={workspace=true,path='../core'}\nmissing={workspace=true}\nremote={workspace=true}\ninvalid={workspace=true}\nfalseflag={workspace=false}\n");
    let links = ok(root, &["project", "links", "--manifest", "app/Cargo.toml"]);
    let items = links["items"].as_array().unwrap();
    assert!(items.iter().all(|r| r["status"] == "unresolved"));
    for (name, reason) in [
        ("core", "unsupported-inherited-fields"),
        ("missing", "workspace-dependency-not-declared"),
        ("remote", "workspace-dependency-not-local"),
        ("invalid", "invalid-workspace-dependency"),
        ("falseflag", "invalid-inherited-dependency"),
    ] {
        assert!(items
            .iter()
            .any(|r| r["name"] == name && r["reason"] == reason));
    }
    let ownership = ok(
        root,
        &["project", "workspaces", "--manifest", "core/Cargo.toml"],
    );
    assert_eq!(
        ownership["items"][0]["reason"],
        "package-not-observed-member"
    );
}

#[test]
fn automatic_membership_terminates_on_cycles_and_respects_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a']\nexclude=['excluded']\n",
    );
    cargo_package(
        root,
        "a",
        "a",
        "[dependencies]\nb={path='../b'}\nexcluded={path='../excluded'}\n",
    );
    cargo_package(root, "b", "b", "[dependencies]\na={path='../a'}\n");
    cargo_package(root, "excluded", "excluded", "");
    let view = ok(root, &["project", "workspaces"]);
    assert_eq!(
        view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "member")
            .count(),
        2
    );
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["manifest"] == "excluded/Cargo.toml" && r["reason"] == "workspace-excluded"));
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a']\nexclude=['**/excluded']\n",
    );
    let unknown = ok(root, &["project", "workspaces"]);
    assert!(!unknown["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "member"));
}

#[test]
fn workspace_pages_and_inherited_links_are_revision_bound() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a','b']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    cargo_package(root, "a", "a", "[dependencies]\ncore={workspace=true}\n");
    cargo_package(root, "b", "b", "");
    cargo_package(root, "core", "core", "");
    let full = ok(root, &["project", "workspaces"]);
    let first = ok(root, &["project", "workspaces", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut collected = first["items"].as_array().unwrap().clone();
    let mut current = first.clone();
    while let Some(next) = current["page"]["next"].as_str() {
        current = ok(
            root,
            &["project", "workspaces", "--limit", "2", "--cursor", next],
        );
        collected.extend(current["items"].as_array().unwrap().clone());
    }
    assert_eq!(collected, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &[
                "project",
                "workspaces",
                "--manifest",
                "a/Cargo.toml",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "workspaces", "--limit", "0"]).0);
    assert!(!run(root, &["project", "workspaces", "--limit", "501"]).0);
    assert!(!run(root, &["project", "links", "--cursor", cursor]).0);
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['b']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    assert!(!run(root, &["project", "workspaces", "--cursor", cursor]).0);
    let links = ok(root, &["project", "links", "--manifest", "a/Cargo.toml"]);
    assert_eq!(links["items"][0]["reason"], "package-not-observed-member");
}

#[test]
fn root_packages_seed_automatic_membership_without_a_members_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[package]\nname='root'\n[workspace]\n[dependencies]\nchild={path='child'}\n",
    );
    cargo_package(root, "child", "child", "");
    let view = ok(root, &["project", "workspaces"]);
    assert_eq!(view["items"][0]["membership_basis"], "root-package");
    assert_eq!(
        view["items"][1]["membership_basis"],
        "automatic-path-member"
    );
}

#[test]
fn ignored_workspace_ancestor_metadata_participates_in_snapshot_verification() {
    use fun_refactor::index::Index;
    use fun_refactor::project::Project;
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "Cargo.toml", "[workspace]\nmembers=['nested/app']\n");
    cargo_package(&root, "nested/app", "app", "");
    put(&root, ".gitignore", "/nested/Cargo.toml\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let before = Project::new(&root, &index, &scanned, &options).unwrap();
    before.verify(&root).unwrap();
    put(&root, "nested/Cargo.toml", "opaque excluded contents");
    assert!(before.verify(&root).is_err());
    let blocked = Project::new(&root, &index, &scanned, &options).unwrap();
    blocked.verify(&root).unwrap();
    put(&root, "nested/Cargo.toml", "changed excluded contents");
    blocked.verify(&root).unwrap();
    fs::remove_file(root.join("nested/Cargo.toml")).unwrap();
    assert!(blocked.verify(&root).is_err());
}

fn relation_handle(root: &Path, name: &str, qualifier: Option<&str>) -> String {
    let map = ok(
        root,
        &[
            "project",
            "map",
            "--depth",
            "64",
            "--fields",
            "handle,name,qualifier",
        ],
    );
    rows(&map)
        .iter()
        .find(|r| r["name"] == name && r["qualifier"].as_str() == qualifier)
        .unwrap_or_else(|| panic!("missing {qualifier:?}::{name}: {map}"))["handle"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn call_pages_preserve_graph_edges_and_candidate_evidence() {
    use fun_refactor::analysis::call_graph::CallGraph;
    use fun_refactor::index::Index;
    use fun_refactor::scan::ScanOptions;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "trait Shape { fn area(&self); }\nstruct A;\nstruct B;\nimpl Shape for A { fn area(&self) {} }\nimpl Shape for B { fn area(&self) {} }\nfn helper() {}\nfn report(s: &dyn Shape) { s.area(); helper(); unknown(); }\nfn main() { report(&A); }\n");
    let canonical = root.canonicalize().unwrap();
    let index = Index::build(&canonical, &ScanOptions::default()).unwrap();
    let graph = CallGraph::build(&index);
    let view = ok(root, &["project", "calls", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        graph.edge_count() + graph.file_scope.len() + graph.unresolved.len()
    );
    for (caller, callee, edge) in graph.edges() {
        let caller = index.symbol(caller).unwrap();
        let callee = index.symbol(callee).unwrap();
        assert!(items.iter().any(|r| r["caller"]["name"] == caller.name
            && r["callee"]["name"] == callee.name
            && r["callee"]["qualifier"].as_str() == callee.qualifier.as_deref()
            && r["site"]["offset"] == edge.offset
            && r["origin"] == edge.origin.as_str()
            && r["confidence"] == edge.confidence.as_str()));
    }
    let dispatch: Vec<_> = items
        .iter()
        .filter(|r| r["dispatch_candidate"] == true)
        .collect();
    assert!(!dispatch.is_empty());
    assert!(dispatch
        .iter()
        .all(|r| r["status"] == "dispatch-candidate" && r["confidence"] == "field-based"));
    assert!(items
        .iter()
        .any(|r| r["status"] == "unresolved" && r["name"] == "unknown" && r["callee"].is_null()));
    let target = dispatch[0]["callee"]["handle"].as_str().unwrap();
    let detail = ok(root, &["project", "show", target]);
    assert_eq!(detail["node"]["name"], "area");
    assert!(detail.get("source").is_none());
}

#[test]
fn call_scope_handles_incoming_outgoing_internal_and_file_scope_calls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "def leaf():\n    return 'BODY_ONLY_SENTINEL'\n\ndef recur():\n    recur()\n    leaf()\n    unknown()\n\ndef outer():\n    def nested():\n        leaf()\n    nested()\n\nrecur()\n");
    let recur = relation_handle(root, "recur", None);
    let outgoing = ok(
        root,
        &["project", "calls", &recur, "--direction", "outgoing"],
    );
    assert!(outgoing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["callee"]["name"] == "leaf"));
    assert!(outgoing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "unresolved"));
    let incoming = ok(
        root,
        &["project", "calls", &recur, "--direction", "incoming"],
    );
    let items = incoming["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["scope_relation"] == "internal" && r["caller"]["name"] == "recur"));
    assert!(items
        .iter()
        .any(|r| r["caller_scope"] == "file" && r["caller"].is_null()));
    assert!(!items.iter().any(|r| r["status"] == "unresolved"));
    let both = ok(root, &["project", "calls", &recur]);
    assert_eq!(
        both["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["scope_relation"] == "internal")
            .count(),
        1
    );
    assert!(!both.to_string().contains("BODY_ONLY_SENTINEL"));
    let outer = relation_handle(root, "outer", None);
    let nested = ok(
        root,
        &["project", "calls", &outer, "--direction", "outgoing"],
    );
    assert!(nested["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["caller"]["name"] == "nested" && r["callee"]["name"] == "leaf"));
    let file = ok(root, &["project", "calls", "app.py"]);
    assert!(file["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["caller_scope"] == "file"));
}

#[test]
fn implementation_pages_match_the_hierarchy_without_claiming_runtime_certainty() {
    use fun_refactor::analysis::call_graph::Hierarchy;
    use fun_refactor::index::Index;
    use fun_refactor::scan::ScanOptions;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "shape.rs", "trait Shape { fn area(&self); }\nstruct A;\nstruct B;\nimpl Shape for A { fn area(&self) { } }\nimpl Shape for B { fn area(&self) { } }\n");
    let index = Index::build(&root.canonicalize().unwrap(), &ScanOptions::default()).unwrap();
    let hierarchy = Hierarchy::scan(&index);
    for (name, qualifier) in [("Shape", None), ("area", Some("Shape"))] {
        let declaration = index
            .symbols
            .iter()
            .find(|s| s.name == name && s.qualifier.as_deref() == qualifier)
            .unwrap();
        let expected = hierarchy.implementations_of(&index, declaration.id);
        assert_eq!(expected.len(), 2);
        let expected: Vec<_> = index
            .symbols
            .iter()
            .filter(|s| s.file == declaration.file && declaration.full_span.contains(s.full_span))
            .flat_map(|s| {
                let index = &index;
                hierarchy
                    .implementations_of(index, s.id)
                    .into_iter()
                    .map(move |implementation| (s, index.symbol(implementation).unwrap()))
            })
            .collect();
        let handle = relation_handle(root, name, qualifier);
        let view = ok(root, &["project", "implementations", &handle]);
        assert_eq!(view["page"]["total"], expected.len());
        for row in view["items"].as_array().unwrap() {
            assert!(expected.iter().any(|(declaration, implementation)| {
                row["declaration"]["name"] == declaration.name
                    && row["declaration"]["qualifier"].as_str() == declaration.qualifier.as_deref()
                    && row["implementation"]["name"] == implementation.name
                    && row["implementation"]["qualifier"].as_str()
                        == implementation.qualifier.as_deref()
            }));
            assert_eq!(row["status"], "candidate");
            assert_eq!(row["basis"], "hierarchy-analysis");
            assert!(row["confidence"].is_null());
            let target = row["implementation"]["handle"].as_str().unwrap();
            assert!(
                ok(root, &["project", "show", target])["node"]["signature"].is_object()
                    || qualifier.is_none()
            );
        }
    }
    let concrete = relation_handle(root, "area", Some("A"));
    assert_eq!(
        ok(root, &["project", "implementations", &concrete])["page"]["total"],
        0
    );
}

#[test]
fn relationship_pages_are_complete_and_bound_to_scope_direction_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "trait T { fn run(&self); }\nstruct A;\nstruct B;\nimpl T for A { fn run(&self) {} }\nimpl T for B { fn run(&self) {} }\nfn invoke(t: &dyn T) { t.run(); missing(); }\n");
    for query in ["calls", "implementations"] {
        let full = ok(root, &["project", query, "--limit", "500"]);
        let first = ok(root, &["project", query, "--limit", "1"]);
        let cursor = first["page"]["next"].as_str().unwrap();
        let mut current = first.clone();
        let mut collected = Vec::new();
        loop {
            collected.extend(current["items"].as_array().unwrap().clone());
            let Some(next) = current["page"]["next"].as_str() else {
                break;
            };
            current = ok(root, &["project", query, "--limit", "2", "--cursor", next]);
        }
        assert_eq!(collected, *full["items"].as_array().unwrap());
        assert!(!run(root, &["project", query, "app.rs", "--cursor", cursor]).0);
        assert!(!run(root, &["project", query, "--limit", "0"]).0);
        assert!(!run(root, &["project", query, "--limit", "501"]).0);
        if query == "calls" {
            assert!(
                !run(
                    root,
                    &[
                        "project",
                        query,
                        "--direction",
                        "outgoing",
                        "--cursor",
                        cursor
                    ]
                )
                .0
            );
        }
        let target = relation_handle(root, "T", None);
        let id = target.rsplit(':').next().unwrap();
        let short = ok(
            root,
            &[
                "project",
                query,
                id,
                "--revision",
                full["revision"].as_str().unwrap(),
            ],
        );
        let long = ok(root, &["project", query, &target]);
        assert_eq!(short, long);
        put(root, "new.rs", "fn added() {}\n");
        assert!(!run(root, &["project", query, "--cursor", cursor]).0);
        assert!(!run(root, &["project", query, &target]).0);
        fs::remove_file(root.join("new.rs")).unwrap();
    }
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn relationship_coverage_does_not_turn_unsupported_hierarchies_into_empty_certainty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.zig", "pub fn main() void {}\n");
    put(root, "page.html", "<main>hello</main>\n");
    let implementations = ok(root, &["project", "implementations"]);
    assert_eq!(
        implementations["analysis"]["hierarchy_unsupported_files"]["zig"],
        1
    );
    assert!(implementations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "zig"));
    let calls = ok(root, &["project", "calls"]);
    assert!(calls["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "html"));
}

#[test]
fn relationship_labels_are_bounded_before_detail_retrieval() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "long_".repeat(100);
    put(
        root,
        "app.py",
        &format!("def {name}():\n    return 'HIDDEN_IMPLEMENTATION'\n\ndef run():\n    {name}()\n"),
    );
    let view = ok(root, &["project", "calls"]);
    let callee = &view["items"][0]["callee"];
    assert_eq!(callee["name"]["omitted_bytes"], 340);
    assert!(!view.to_string().contains("HIDDEN_IMPLEMENTATION"));
    assert!(ok(
        root,
        &["project", "show", callee["handle"].as_str().unwrap()]
    )["node"]["name"]
        .is_object());
}
