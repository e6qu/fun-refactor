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
        r#"export async function GET(_request: Request, context: {params: Promise<{id: string}>}) {
  const params = await context.params;
  return Response.json({id: params["id"], framework: "portable"}, {status: 201});
}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("api.py"),
        r#"from fastapi import FastAPI
from fastapi.responses import JSONResponse
app = FastAPI()
@app.get('/fast/{id}')
def show_fast(id: str):
    return JSONResponse(content={"id": id, "framework": "portable"}, status_code=201)
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("express.ts"),
        r#"function showExpress(req: Request, res: Response) {
  return res.status(201).json({id: req.params["id"], framework: "portable"});
}
app.get('/express/:id', showExpress);
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("server.go"),
        r#"package sample
import (
    "encoding/json"
    "net/http"
)
func showGo(w http.ResponseWriter, r *http.Request) {
    w.WriteHeader(201)
    json.NewEncoder(w).Encode(map[string]any{"id": r.PathValue("id"), "framework": "portable"})
}
func routes() http.Handler {
    mux := http.NewServeMux()
    mux.HandleFunc("GET /go/{id}", showGo)
    return mux
}
"#,
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
fn application_keeps_effectful_handlers_as_explicit_manual_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("api.py"),
        "from fastapi import FastAPI\napp = FastAPI()\n@app.get('/records/{id}')\ndef show(id: str):\n    return load_record(id)\n",
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
