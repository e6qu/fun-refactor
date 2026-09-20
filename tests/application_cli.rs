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
        {"method": "POST", "path": "/audit", "status": 201, "response": {"kind": "literal", "value": true}},
        {"method": "POST", "path": "/records/{id}", "inputs": [
            {"name":"limit","source":"query","scalar":"integer"},
            {"name":"title","source":"json-body","scalar":"string"}
        ], "status": 201, "response": {"kind":"object","fields":{
            "id":{"kind":"path","name":"id"},
            "limit":{"kind":"input","name":"limit"},
            "title":{"kind":"input","name":"title"}
        }}}
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
fn application_fastapi_registration_and_dependencies_share_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    input(dir.path());
    let application = "from fastapi import FastAPI\n\napp = FastAPI()\n";
    let manifest = "[project]\nname = \"sample\"\nversion = \"1.0.0\"\ndependencies = []\n";
    fs::write(dir.path().join("app.py"), application).unwrap();
    fs::write(dir.path().join("pyproject.toml"), manifest).unwrap();
    let args = [
        "migrate",
        "application",
        "--ir",
        "application.json",
        "--to",
        "fastapi",
        "--out",
        "generated",
        "--register-with",
        "app.py::app",
        "--dependency-manifest",
        "pyproject.toml",
        "--dependency-requirement",
        "fastapi==0.141.1",
        "--save-plan",
    ];
    let preview = ok(dir.path(), &args);
    assert_eq!(preview["migration"]["integration"]["status"], "connected");
    assert_eq!(
        preview["migration"]["integration"]["dependencies"]["status"],
        "updated"
    );
    let transaction = preview["transaction"].as_u64().unwrap().to_string();
    ok(dir.path(), &["history", "apply", &transaction, "--write"]);
    assert!(fs::read_to_string(dir.path().join("app.py"))
        .unwrap()
        .contains("app.include_router(fr_migrated_router)"));
    assert!(fs::read_to_string(dir.path().join("pyproject.toml"))
        .unwrap()
        .contains("fastapi==0.141.1"));
    assert!(dir.path().join("generated/routes.py").is_file());
    ok(dir.path(), &["history", "undo", &transaction, "--write"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("app.py")).unwrap(),
        application
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("pyproject.toml")).unwrap(),
        manifest
    );
    assert!(!dir.path().join("generated/routes.py").exists());
}

#[test]
fn application_express_registration_and_npm_dependency_share_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    input(dir.path());
    let application = "import express from \"express\";\nconst app = express();\n";
    let manifest = "{\n  \"name\": \"sample\",\n  \"dependencies\": {}\n}\n";
    fs::write(dir.path().join("app.ts"), application).unwrap();
    fs::write(dir.path().join("package.json"), manifest).unwrap();
    let args = [
        "migrate",
        "application",
        "--ir",
        "application.json",
        "--to",
        "express",
        "--out",
        "generated",
        "--register-with",
        "app.ts::app",
        "--dependency-manifest",
        "package.json",
        "--dependency-requirement",
        "express@5.2.1",
        "--save-plan",
    ];
    let preview = ok(dir.path(), &args);
    assert_eq!(preview["migration"]["integration"]["status"], "connected");
    assert_eq!(
        preview["migration"]["integration"]["dependencies"]["status"],
        "updated"
    );
    let transaction = preview["transaction"].as_u64().unwrap().to_string();
    ok(dir.path(), &["history", "apply", &transaction, "--write"]);
    let updated = fs::read_to_string(dir.path().join("app.ts")).unwrap();
    assert!(updated.contains("from \"./generated/routes\""));
    assert!(updated.contains("app.use(fr_migrated_router);"));
    assert_eq!(
        serde_json::from_str::<Value>(
            &fs::read_to_string(dir.path().join("package.json")).unwrap()
        )
        .unwrap()["dependencies"]["express"],
        "5.2.1"
    );
    ok(dir.path(), &["history", "undo", &transaction, "--write"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("app.ts")).unwrap(),
        application
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("package.json")).unwrap(),
        manifest
    );
    assert!(!dir.path().join("generated/routes.ts").exists());
}

#[test]
fn application_go_registration_generates_a_reversible_module_mount() {
    let dir = tempfile::tempdir().unwrap();
    input(dir.path());
    let application = "package main\n\nimport \"net/http\"\n\nvar mux = http.NewServeMux()\n\nfunc main() { _ = mux }\n";
    fs::write(dir.path().join("main.go"), application).unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.com/sample\n\ngo 1.22\n",
    )
    .unwrap();
    let args = [
        "migrate",
        "application",
        "--ir",
        "application.json",
        "--to",
        "go-net-http",
        "--out",
        "generated",
        "--register-with",
        "main.go::mux",
        "--save-plan",
    ];
    let preview = ok(dir.path(), &args);
    assert_eq!(preview["migration"]["integration"]["status"], "connected");
    assert_eq!(
        preview["migration"]["integration"]["registration"]["framework"],
        "go-net-http"
    );
    let transaction = preview["transaction"].as_u64().unwrap().to_string();
    ok(dir.path(), &["history", "apply", &transaction, "--write"]);
    assert!(dir.path().join("generated/routes.go").is_file());
    assert!(
        fs::read_to_string(dir.path().join("fr_application_mount.go"))
            .unwrap()
            .contains("example.com/sample/generated")
    );
    let output = Command::new("go")
        .arg("test")
        .arg("./...")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ok(dir.path(), &["history", "undo", &transaction, "--write"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("main.go")).unwrap(),
        application
    );
    assert!(!dir.path().join("generated/routes.go").exists());
    assert!(!dir.path().join("fr_application_mount.go").exists());
}

#[test]
fn application_cutover_removes_only_one_owned_connected_source() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/api/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    let source = "export async function GET() { return Response.json({ healthy: true }); }\n";
    fs::write(&route, source).unwrap();
    let application = "from fastapi import FastAPI\n\napp = FastAPI()\n";
    fs::write(dir.path().join("api.py"), application).unwrap();
    let args = [
        "migrate",
        "application",
        "--project",
        "app/api/signals/route.ts",
        "--to",
        "fastapi",
        "--out",
        "generated",
        "--register-with",
        "api.py::app",
        "--cutover",
        "--save-plan",
    ];
    let preview = ok(dir.path(), &args);
    assert_eq!(preview["migration"]["coexistence"]["cutover_planned"], true);
    let transaction = preview["transaction"].as_u64().unwrap().to_string();
    ok(dir.path(), &["history", "apply", &transaction, "--write"]);
    assert!(!route.exists());
    assert!(dir.path().join("generated/routes.py").is_file());
    assert!(fs::read_to_string(dir.path().join("api.py"))
        .unwrap()
        .contains("include_router"));
    ok(dir.path(), &["history", "undo", &transaction, "--write"]);
    assert_eq!(fs::read_to_string(route).unwrap(), source);
    assert_eq!(
        fs::read_to_string(dir.path().join("api.py")).unwrap(),
        application
    );
}

#[test]
fn application_cutover_refuses_sources_without_whole_file_ownership() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("api.ts"), "import express from 'express';\nconst app = express();\nfunction signal(_req: Request, res: Response) { return res.status(200).json({ healthy: true }); }\napp.get('/signals', signal);\n").unwrap();
    fs::write(
        dir.path().join("target.py"),
        "from fastapi import FastAPI\napp = FastAPI()\n",
    )
    .unwrap();
    let (passed, report) = run(
        dir.path(),
        &[
            "migrate",
            "application",
            "--project",
            "api.ts",
            "--to",
            "fastapi",
            "--out",
            "generated",
            "--register-with",
            "target.py::app",
            "--cutover",
        ],
    );
    assert!(!passed);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("wholly owned source file"));
    assert!(dir.path().join("api.ts").is_file());
}

#[test]
fn application_nextjs_uses_captured_app_router_placement() {
    let dir = tempfile::tempdir().unwrap();
    input(dir.path());
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"next":"16.3.5"}}"#,
    )
    .unwrap();
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "application",
            "--ir",
            "application.json",
            "--to",
            "nextjs",
            "--out",
            "app",
        ],
    );
    assert_eq!(report["migration"]["integration"]["status"], "connected");
    assert_eq!(
        report["migration"]["integration"]["registration"]["framework"],
        "nextjs-app"
    );
    assert_eq!(
        report["migration"]["integration"]["dependencies"]["status"],
        "satisfied"
    );
}

#[test]
fn application_migration_can_normalize_the_project_snapshot_without_an_ir_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        "from fastapi import FastAPI\nfrom fastapi.responses import JSONResponse\napp = FastAPI()\n@app.get('/records/{id}')\ndef show(id: str):\n    return JSONResponse(content={'id': id}, status_code=200)\n",
    )
    .unwrap();
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "application",
            "--project",
            ".",
            "--to",
            "express",
            "--out",
            "generated",
        ],
    );
    assert_eq!(report["migration"]["source_kind"], "project-snapshot");
    assert_eq!(
        report["migration"]["endpoints"].as_array().unwrap().len(),
        1
    );
    assert_eq!(report["migration"]["manual_boundaries"], 0);
    assert!(!dir.path().join("generated").exists());
}

#[test]
fn application_report_publishes_every_adapter_feature_pair_and_refusal() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("App.tsx"),
        "export default function App() { return <main>Ready</main>; }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"react":"19.3.0"}}"#,
    )
    .unwrap();
    let report = ok(dir.path(), &["project", "application"]);
    let adapters = report["adapters"].as_array().unwrap();
    assert_eq!(adapters.len(), 5);
    assert!(adapters.iter().all(|source| {
        source["targets"].as_array().is_some_and(|targets| {
            targets.len() == 5
                && targets.iter().all(|target| {
                    target["features"]
                        .as_array()
                        .is_some_and(|features| features.len() == 4)
                })
        })
    }));
    let feature = |source: &str, target: &str, name: &str| {
        adapters.iter().find(|row| row["source"] == source).unwrap()["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["target"] == target)
            .unwrap()["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["feature"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        feature("react", "nextjs", "static-component")["status"],
        "supported"
    );
    assert_eq!(
        feature("react", "fastapi", "json-route")["reason"],
        "source-reader-does-not-model-json-route"
    );
    assert_eq!(
        feature("nextjs", "react", "path-json-route")["reason"],
        "target-writer-does-not-model-path-json-route"
    );
    assert_eq!(
        feature("fastapi", "express", "validated-json-route")["status"],
        "supported"
    );
    for source in ["nextjs", "express", "go-net-http"] {
        assert_eq!(
            feature(source, "fastapi", "validated-json-route")["status"],
            "supported",
            "{source}"
        );
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

#[test]
fn application_normalizes_the_shared_literal_http_subset_across_frameworks() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("app/next/[id]")).unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"express":"5.2.1","next":"16.3.5"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("app/next/[id]/route.ts"),
        include_str!("application-fixtures/portable_next.ts"),
    )
    .unwrap();
    fs::write(
        dir.path().join("api.py"),
        include_str!("application-fixtures/portable_fastapi.py"),
    )
    .unwrap();
    fs::write(
        dir.path().join("express.ts"),
        include_str!("application-fixtures/portable_express.ts"),
    )
    .unwrap();
    fs::write(
        dir.path().join("server.go"),
        include_str!("application-fixtures/portable_go.go"),
    )
    .unwrap();

    let report = ok(dir.path(), &["project", "application"]);
    let mut migration_report = report.clone();
    migration_report["model"]["applications"]
        .as_array_mut()
        .unwrap()
        .retain(|application| application["data"]["application"]["framework"] == "express");
    migration_report["object_digest"] =
        json!(fun_refactor::project::object_merkle(&migration_report["model"]).unwrap());
    fs::write(
        dir.path().join("application-model.json"),
        serde_json::to_vec(&migration_report).unwrap(),
    )
    .unwrap();
    fn routes(node: &Value, found: &mut Vec<Value>) {
        if let Some(route) = node.get("route") {
            found.push(route.clone());
            assert_eq!(node["data"]["normalization"]["status"], "portable");
        }
        for child in node["children"].as_array().unwrap() {
            routes(child, found);
        }
    }
    let mut found = Vec::new();
    for application in report["model"]["applications"].as_array().unwrap() {
        routes(application, &mut found);
    }
    found.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    assert_eq!(
        found
            .iter()
            .map(|route| (
                route["path"].as_str().unwrap(),
                route["status"].as_u64().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("/express/{id}", 201),
            ("/fast/{id}", 201),
            ("/go/{id}", 201),
            ("/next/{id}", 201)
        ]
    );
    let response = found[0]["response"].clone();
    for route in &found {
        assert_eq!(route["response"]["fields"]["id"]["kind"], "path");
        assert_eq!(route["response"], response);
    }
    assert!(
        fun_refactor::project::framework_kernel::application_endpoint_agreement(
            found.iter().all(|route| route["method"] == "GET"),
            found
                .iter()
                .all(|route| route["path"].as_str().unwrap().ends_with("/{id}")),
            found.iter().all(|route| route["status"] == 201),
            found.iter().all(|route| route["response"] == response),
        )
    );
    let migration = ok(
        dir.path(),
        &[
            "migrate",
            "application",
            "--ir",
            "application-model.json",
            "--to",
            "fastapi",
            "--out",
            "generated",
        ],
    );
    assert_eq!(
        migration["migration"]["source_kind"],
        "project-application-report"
    );
    assert_eq!(migration["migration"]["manual_boundaries"], 0);
    assert_eq!(
        migration["migration"]["endpoints"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn application_reads_required_fastapi_query_and_embedded_body_inputs() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        include_str!("application-fixtures/validated_fastapi.py"),
    )
    .unwrap();

    let report = ok(dir.path(), &["project", "application"]);
    fn route(node: &Value) -> Option<&Value> {
        if node["kind"] == "route" {
            return Some(node);
        }
        node["children"].as_array()?.iter().find_map(route)
    }
    let route = report["model"]["applications"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(route)
        .unwrap();
    fn has_parameter_gap(node: &Value) -> bool {
        node["data"]["contract"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Parameters lack one supported explicit binding"))
            || node["children"]
                .as_array()
                .is_some_and(|children| children.iter().any(has_parameter_gap))
    }
    assert!(!has_parameter_gap(route));
    assert_eq!(route["data"]["normalization"]["status"], "portable");
    assert_eq!(
        route["data"]["normalization"]["basis"],
        "syntax-derived-validated-http-ir"
    );
    assert!(route["boundary"]
        .as_str()
        .unwrap()
        .contains("validation errors use the portable IR contract"));
    assert_eq!(
        route["route"]["inputs"],
        json!([
            {"name":"limit", "source":"query", "scalar":"integer"},
            {"name":"visible", "source":"json-body", "scalar":"boolean"}
        ])
    );
    assert_eq!(route["route"]["response"]["fields"]["id"]["kind"], "path");
    assert_eq!(
        route["route"]["response"]["fields"]["limit"],
        json!({"kind":"input", "name":"limit"})
    );
    assert_eq!(
        route["route"]["response"]["fields"]["published"],
        json!({"kind":"input", "name":"visible"})
    );

    for target in ["nextjs", "express", "go-net-http"] {
        let out = format!("generated-{target}");
        let migration = ok(
            dir.path(),
            &[
                "migrate",
                "application",
                "--project",
                ".",
                "--to",
                target,
                "--out",
                &out,
            ],
        );
        assert_eq!(migration["migration"]["manual_boundaries"], 0);
        assert_eq!(
            migration["migration"]["endpoints"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn application_reads_explicit_validated_inputs_in_next_express_and_go() {
    fn routes(node: &Value, found: &mut Vec<Value>) {
        if node["kind"] == "route" {
            found.push(node.clone());
        }
        for child in node["children"].as_array().into_iter().flatten() {
            routes(child, found);
        }
    }

    let sources = [
        (
            "nextjs",
            "app/records/[id]/route.ts",
            include_str!("application-fixtures/validated_next.ts"),
            "/records/{id}",
        ),
        (
            "express",
            "server.ts",
            include_str!("application-fixtures/validated_express.ts"),
            "/records/{id}",
        ),
        (
            "go-net-http",
            "server.go",
            include_str!("application-fixtures/validated_go.go"),
            "/records/{id}",
        ),
    ];
    for (adapter, path, source, expected_path) in sources {
        let dir = tempfile::tempdir().unwrap();
        if let Some(parent) = dir.path().join(path).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(dir.path().join(path), source).unwrap();
        if adapter != "go-net-http" {
            fs::write(
                dir.path().join("package.json"),
                format!(
                    r#"{{"dependencies":{{"{}":"1"}}}}"#,
                    if adapter == "nextjs" {
                        "next"
                    } else {
                        "express"
                    }
                ),
            )
            .unwrap();
        }
        let report = ok(dir.path(), &["project", "application"]);
        let mut found = Vec::new();
        for application in report["model"]["applications"].as_array().unwrap() {
            routes(application, &mut found);
        }
        let route = found
            .iter()
            .find(|route| route["data"]["route"]["url"] == expected_path)
            .unwrap_or_else(|| panic!("{adapter}: {found:?}"));
        assert_eq!(
            route["data"]["normalization"]["status"], "portable",
            "{adapter}: {route}"
        );
        let mut inputs = vec![json!({"name":"limit", "source":"query", "scalar":"integer"})];
        if adapter != "go-net-http" {
            inputs.push(json!({"name":"visible", "source":"json-body", "scalar":"boolean"}));
        }
        assert_eq!(route["route"]["inputs"], json!(inputs), "{adapter}");
        assert_eq!(
            route["route"]["response"]["fields"]["limit"],
            json!({"kind":"input", "name":"limit"}),
            "{adapter}"
        );
        if adapter != "go-net-http" {
            assert_eq!(
                route["route"]["response"]["fields"]["visible"],
                json!({"kind":"input", "name":"visible"}),
                "{adapter}"
            );
        }
        let migration = ok(
            dir.path(),
            &[
                "migrate",
                "application",
                "--project",
                ".",
                "--to",
                "fastapi",
                "--out",
                "generated",
            ],
        );
        assert_eq!(migration["migration"]["manual_boundaries"], 0, "{adapter}");
    }
}

#[test]
fn application_keeps_unchecked_or_unsupported_validation_manual() {
    for (path, source, package) in [
        (
            "app/records/route.ts",
            include_str!("application-fixtures/invalid_validated_next.ts"),
            "next",
        ),
        (
            "server.ts",
            include_str!("application-fixtures/invalid_validated_express.ts"),
            "express",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        if let Some(parent) = dir.path().join(path).parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(dir.path().join(path), source).unwrap();
        fs::write(
            dir.path().join("package.json"),
            format!(r#"{{"dependencies":{{"{package}":"1"}}}}"#),
        )
        .unwrap();
        let report = ok(dir.path(), &["project", "application"]);
        fn route(node: &Value) -> Option<&Value> {
            if node["kind"] == "route" {
                return Some(node);
            }
            node["children"].as_array()?.iter().find_map(route)
        }
        let found = report["model"]["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(route)
            .unwrap();
        assert_eq!(found["data"]["normalization"]["status"], "manual", "{path}");
        assert!(found["route"].is_null(), "{path}");
    }
}

#[test]
fn application_refuses_fastapi_input_semantics_outside_the_exact_subset() {
    for parameters in [
        "term: str = Query(default=\"all\")",
        "term: str | None = Query()",
        "term: str = Query(min_length=1)",
        "term: str = Query(alias=NAME)",
        "term: str = Body()",
        "term: str = Body(embed=False)",
        concat!("term: str = Body(embed=True, ", "min_length=1)"),
        concat!("term = Depends(load_term, ", "use_cache=False)"),
        concat!("term = Security(load_term, ", "scopes=[\"read\"])"),
        "term = Depends(lambda: \"x\")",
        "term = Depends()",
        "term = Depends(load_term)",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("api.py"),
            format!(
                "from fastapi import Body, Depends, FastAPI, Query\nfrom fastapi.responses import JSONResponse\napp = FastAPI()\nNAME = 'term'\ndef load_term():\n    return 'x'\n@app.post('/records')\ndef create({parameters}):\n    return JSONResponse(content={{'term': term}}, status_code=200)\n"
            ),
        )
        .unwrap();
        let report = ok(dir.path(), &["project", "application"]);
        fn route(node: &Value) -> Option<&Value> {
            if node["kind"] == "route" {
                return Some(node);
            }
            node["children"].as_array()?.iter().find_map(route)
        }
        let route = report["model"]["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(route)
            .unwrap();
        assert_eq!(
            route["data"]["normalization"]["status"], "manual",
            "unexpected admission for {parameters}"
        );
        assert!(route.get("route").is_none());
    }
}

#[test]
fn application_reads_fastapi_middleware_order_and_route_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        include_str!("application-fixtures/behavior_fastapi.py"),
    )
    .unwrap();

    let report = ok(dir.path(), &["project", "application"]);
    let application = &report["model"]["applications"][0];
    assert_eq!(
        application["middleware"],
        json!([
            {"name": "AuthMiddleware", "request_order": 1},
            {"name": "audit", "request_order": 2}
        ])
    );
    assert_eq!(
        application["data"]["middleware_normalization"]["status"],
        "portable"
    );
    fn route(node: &Value) -> Option<&Value> {
        if node["kind"] == "route" {
            return Some(node);
        }
        node["children"].as_array()?.iter().find_map(route)
    }
    let route = report["model"]["applications"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(route)
        .unwrap();
    assert_eq!(route["data"]["normalization"]["status"], "portable");
    assert_eq!(
        route["route"]["dependencies"],
        json!([
            {"binding": "session", "provider": "load_session", "security": true},
            {"binding": "terms", "provider": "load_terms", "security": false}
        ])
    );

    for (target, file, marker) in [
        (
            "express",
            "routes.ts",
            "router.use(frMiddleware.AuthMiddleware);",
        ),
        (
            "go-net-http",
            "routes.go",
            "handler = AuthMiddleware(handler)",
        ),
        (
            "nextjs",
            "middleware.ts",
            "frMiddleware.AuthMiddleware(request",
        ),
    ] {
        let target_dir = dir.path().join(format!("target-{target}"));
        fs::create_dir(&target_dir).unwrap();
        fs::write(
            target_dir.join("api.py"),
            include_str!("application-fixtures/behavior_fastapi.py"),
        )
        .unwrap();
        let migration = ok(
            &target_dir,
            &[
                "migrate",
                "application",
                "--project",
                ".",
                "--to",
                target,
                "--out",
                "generated",
                "--write",
            ],
        );
        assert_eq!(migration["migration"]["manual_boundaries"], 0);
        let mut source = fs::read_to_string(target_dir.join("generated").join(file)).unwrap();
        if target == "nextjs" {
            source.push_str(
                &fs::read_to_string(
                    target_dir
                        .join("generated")
                        .join("records/[record_id]/route.ts"),
                )
                .unwrap(),
            );
        }
        assert!(source.contains(marker), "{target}: {source}");
        let auth = source.find("AuthMiddleware").unwrap();
        let audit = source.find("audit").unwrap();
        if target == "go-net-http" {
            assert!(audit < auth, "{target}: middleware order");
        } else {
            assert!(auth < audit, "{target}: middleware order");
        }
        assert!(source.contains("401"), "{target}: dependency contract");
    }
}

#[test]
fn application_reads_closed_world_fastapi_service_calls() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        include_str!("application-fixtures/service_fastapi.py"),
    )
    .unwrap();
    let report = ok(dir.path(), &["project", "application"]);
    fn routes(node: &Value, found: &mut Vec<Value>) {
        if node["kind"] == "route" {
            found.push(node.clone());
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                routes(child, found);
            }
        }
    }
    let mut found = Vec::new();
    routes(&report["model"]["applications"][0], &mut found);
    assert_eq!(found.len(), 2);
    let feed = found
        .iter()
        .find(|route| route["route"]["path"] == "/feed")
        .unwrap();
    assert_eq!(feed["data"]["normalization"]["status"], "portable");
    assert_eq!(
        feed["route"]["response"],
        json!({"kind": "service", "method": "GET", "path": "/records"})
    );
    for target in ["express", "go-net-http", "nextjs"] {
        let migration = ok(
            dir.path(),
            &[
                "migrate",
                "application",
                "--project",
                ".",
                "--to",
                target,
                "--out",
                &format!("generated-{target}"),
            ],
        );
        assert_eq!(migration["migration"]["manual_boundaries"], 0, "{target}");
        assert_eq!(
            migration["migration"]["endpoints"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn application_keeps_nonlocal_or_dynamic_service_calls_manual() {
    for call in [
        "requests.get(\"https://example.com/records\").json()",
        "requests.get(BASE + \"/records\").json()",
        "requests.get(\"/records\").text",
        "requests.get(\"/absent\").json()",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("api.py"),
            format!(
                "import requests\nfrom fastapi import FastAPI\nfrom fastapi.responses import JSONResponse\n\napp = FastAPI()\nBASE = 'http://x'\n\n@app.get('/records')\ndef list_records():\n    return JSONResponse(content={{'items': []}}, status_code=200)\n@app.get('/feed')\ndef read_feed():\n    return JSONResponse(content={call}, status_code=200)\n"
            ),
        )
        .unwrap();
        let report = ok(dir.path(), &["project", "application"]);
        fn routes(node: &Value, found: &mut Vec<Value>) {
            if node["kind"] == "route" {
                found.push(node.clone());
            }
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    routes(child, found);
                }
            }
        }
        let mut found = Vec::new();
        routes(&report["model"]["applications"][0], &mut found);
        let feed = found
            .iter()
            .find(|route| route["data"]["route"]["url"] == "/feed")
            .cloned()
            .unwrap();
        assert_eq!(feed["data"]["normalization"]["status"], "manual", "{call}");
        if call == "requests.get(\"/absent\").json()" {
            let migration = ok(
                dir.path(),
                &[
                    "migrate",
                    "application",
                    "--project",
                    ".",
                    "--to",
                    "express",
                    "--out",
                    "generated",
                ],
            );
            assert_eq!(migration["migration"]["manual_boundaries"], 1);
            assert_eq!(
                migration["migration"]["endpoints"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
    }
}

#[test]
fn application_keeps_configured_middleware_and_scoped_security_manual() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        "from fastapi import FastAPI\nfrom fastapi.responses import JSONResponse\napp = FastAPI()\napp.add_middleware(CORSMiddleware, allow_origins=[\"*\"])\n@app.get('/records')\ndef read():\n    return JSONResponse(content={'ok': True}, status_code=200)\n",
    )
    .unwrap();
    let report = ok(dir.path(), &["project", "application"]);
    let application = &report["model"]["applications"][0];
    assert!(application.get("middleware").is_none());
    assert_eq!(
        application["data"]["middleware_normalization"]["status"],
        "manual"
    );
    let migration = run(
        dir.path(),
        &[
            "migrate",
            "application",
            "--project",
            ".",
            "--to",
            "fastapi",
            "--out",
            "generated-fastapi",
            "--write",
        ],
    );
    assert!(!migration.0);
}

#[test]
fn application_keeps_effectful_handlers_as_explicit_manual_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        include_str!("application-fixtures/effectful_fastapi.py"),
    )
    .unwrap();
    let report = ok(dir.path(), &["project", "application"]);
    fn route(node: &Value) -> Option<&Value> {
        if node["kind"] == "route" {
            return Some(node);
        }
        node["children"].as_array()?.iter().find_map(route)
    }
    let route = report["model"]["applications"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(route)
        .unwrap();
    assert!(route.get("route").is_none());
    assert_eq!(route["data"]["normalization"]["status"], "manual");
}

#[test]
fn static_react_and_nextjs_components_share_one_frontend_ir() {
    let mut normalized = Vec::new();
    for (manifest, path, source, target, generated) in [
        (
            r#"{"dependencies":{"react":"19.3.0"}}"#,
            "src/App.tsx",
            "export default function App() { return <main className=\"shell\"><h1>Signals</h1><p role=\"status\">Ready</p></main>; }\n",
            "nextjs",
            "generated/page.tsx",
        ),
        (
            r#"{"dependencies":{"next":"16.3.5","react":"19.3.0"}}"#,
            "app/page.tsx",
            "export default function Page() { return <main className=\"shell\"><h1>Signals</h1><p role=\"status\">Ready</p></main>; }\n",
            "react",
            "generated/App.tsx",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), manifest).unwrap();
        let source_path = dir.path().join(path);
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, source).unwrap();
        let application = ok(dir.path(), &["project", "application"]);
        fn component(node: &Value) -> Option<Value> {
            if let Some(component) = node.get("component") {
                return Some(component.clone());
            }
            node["children"].as_array()?.iter().find_map(component)
        }
        let component = application["model"]["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(component)
            .unwrap();
        normalized.push(component.clone());
        let migration = ok(
            dir.path(),
            &[
                "migrate",
                "application",
                "--project",
                ".",
                "--to",
                target,
                "--out",
                "generated",
            ],
        );
        assert_eq!(migration["migration"]["components"][0], component);
        assert!(migration["migration"]["endpoints"].as_array().unwrap().is_empty());
        assert!(migration["migration"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == generated));
    }
    assert_eq!(normalized[0]["root"], normalized[1]["root"]);
}

#[test]
fn stateful_client_components_normalize_and_write_across_adapters() {
    let source = "\"use client\";\n\nimport { useState } from \"react\";\n\nexport default function Page() {\n  const [open, setOpen] = useState(false);\n  const [count, setCount] = useState(0);\n  return (\n    <main className=\"shell\">\n      <button onClick={() => setOpen(!open)}>Toggle</button>\n      <button onClick={() => setCount(1)}>Reset</button>\n      <p role=\"status\">{count}</p>\n    </main>\n  );\n}\n";
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"next":"16.3.5","react":"19.3.0"}}"#,
    )
    .unwrap();
    let app = dir.path().join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("page.tsx"), source).unwrap();
    let application = ok(dir.path(), &["project", "application"]);
    fn component(node: &Value) -> Option<Value> {
        if let Some(component) = node.get("component") {
            return Some(component.clone());
        }
        node["children"].as_array()?.iter().find_map(component)
    }
    let component = application["model"]["applications"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(component)
        .unwrap();
    assert_eq!(component["client"], true);
    assert_eq!(
        component["state"],
        json!([
            {"name": "open", "setter": "setOpen", "initial": false},
            {"name": "count", "setter": "setCount", "initial": 0}
        ])
    );
    let main = &component["root"];
    assert_eq!(main["events"], Value::Null);
    let toggle = &main["children"][0];
    assert_eq!(
        toggle["events"]["onClick"],
        json!({"kind": "toggle-state", "state": "open"})
    );
    let reset = &main["children"][1];
    assert_eq!(
        reset["events"]["onClick"],
        json!({"kind": "set-state", "state": "count", "value": 1})
    );
    assert_eq!(
        main["children"][2]["children"][0],
        json!({"kind": "state", "name": "count"})
    );

    let migration = ok(
        dir.path(),
        &[
            "migrate",
            "application",
            "--project",
            ".",
            "--to",
            "react",
            "--out",
            "generated",
        ],
    );
    assert_eq!(migration["migration"]["manual_boundaries"], 0);
    let diff = migration["diff"].as_str().unwrap();
    assert!(
        diff.contains("const [open, setOpen] = useState(false);"),
        "{diff}"
    );
    assert!(
        diff.contains("onClick={() => setOpen((value) => !value)}"),
        "{diff}"
    );
    assert!(diff.contains("onClick={() => setCount(1)}"), "{diff}");
    assert!(diff.contains("{count}"), "{diff}");
    assert!(!diff.contains("use client"), "{diff}");
}

#[test]
fn same_line_duplicate_component_events_keep_distinct_identities() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"react":"19.3.0"}}"#,
    )
    .unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("App.tsx"),
        "import { useState } from \"react\";\nexport default function App() { const [a, setA] = useState(false); const [b, setB] = useState(false); return <main><button onClick={() => setA(!a)}>A</button><button onClick={() => setB(!b)}>B</button></main>; }\n",
    )
    .unwrap();
    let report = ok(dir.path(), &["project", "application"]);
    fn kinds(node: &Value, kind: &str, found: &mut usize) {
        if node["kind"] == kind {
            *found += 1;
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                kinds(child, kind, found);
            }
        }
    }
    let mut events = 0;
    kinds(
        &report["model"]["applications"][0],
        "component-event",
        &mut events,
    );
    assert_eq!(events, 2);
    fn component(node: &Value) -> Option<Value> {
        if let Some(component) = node.get("component") {
            return Some(component.clone());
        }
        node["children"].as_array()?.iter().find_map(component)
    }
    let component = report["model"]["applications"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(component)
        .unwrap();
    assert_eq!(component["client"], true);
    assert_eq!(component["state"].as_array().unwrap().len(), 2);
}

#[test]
fn stateful_components_without_a_client_boundary_stay_manual() {
    for source in [
        "import { useState } from \"react\";\n\nexport default function Page() {\n  const [open, setOpen] = useState(false);\n  return <button onClick={() => setOpen(!open)}>Toggle</button>;\n}\n",
        "\"use client\";\n\nimport { useEffect, useState } from \"react\";\n\nexport default function Page() {\n  const [open, setOpen] = useState(false);\n  useEffect(() => { document.title = \"x\"; }, []);\n  return <button onClick={() => setOpen(!open)}>Toggle</button>;\n}\n",
        "\"use client\";\n\nimport { useState } from \"react\";\n\nexport default function Page() {\n  const [count, setCount] = useState(0);\n  return <button onClick={() => setCount(count + 1)}>+</button>;\n}\n",
        "\"use client\";\n\nimport { useState } from \"react\";\n\nexport default function Page() {\n  const [count, setCount] = useState(0.5);\n  return <p>{count}</p>;\n}\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"next":"16.3.5","react":"19.3.0"}}"#,
        )
        .unwrap();
        let app = dir.path().join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("page.tsx"), source).unwrap();
        let application = ok(dir.path(), &["project", "application"]);
        fn manual(node: &Value) -> Option<Value> {
            if node["kind"] == "component" {
                return Some(node.clone());
            }
            node["children"].as_array()?.iter().find_map(manual)
        }
        let component = application["model"]["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(manual)
            .unwrap();
        assert!(
            component.get("component").is_none(),
            "unexpected admission: {source}"
        );
        assert_eq!(component["data"]["normalization"]["status"], "manual");
    }
}

#[test]
fn static_component_entities_remain_an_explicit_manual_boundary() {
    for source in [
        "export default function App() { return <p>Tom &amp; Ada</p>; }\n",
        "export default function App() { return <p title=\"Tom &amp; Ada\">Names</p>; }\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"19.3.0"}}"#,
        )
        .unwrap();
        fs::write(dir.path().join("App.tsx"), source).unwrap();
        let application = ok(dir.path(), &["project", "application"]);
        fn component(node: &Value) -> Option<&Value> {
            if node["kind"] == "component" {
                return Some(node);
            }
            node["children"].as_array()?.iter().find_map(component)
        }
        let component = application["model"]["applications"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(component)
            .unwrap();
        assert!(component.get("component").is_none());
        assert_eq!(component["data"]["normalization"]["status"], "manual");
    }
}
