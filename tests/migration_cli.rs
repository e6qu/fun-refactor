mod common;

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
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: {error}\n{}\n{}",
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

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/api/pets/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(
        route,
        "export async function GET() {\n  return Response.json([{ name: \"Milo\" }]);\n}\n\nexport async function POST(request: Request) {\n  const pet = await request.json();\n  return Response.json(pet, { status: 201 });\n}\n",
    )
    .unwrap();
    dir
}

fn feature(root: &Path) -> String {
    let report = ok(root, &["project", "features", "--limit", "500"]);
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "feature")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn feature_for_framework(root: &Path, framework: &str) -> String {
    let report = ok(root, &["project", "features", "--limit", "500"]);
    let items = report["items"].as_array().unwrap();
    let application = items
        .iter()
        .find(|row| row["kind"] == "application" && row["application"]["framework"] == framework)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    items
        .iter()
        .find(|row| row["kind"] == "feature" && row["parent"] == application)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn migration<'a>(feature: &'a str, intent: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec![
        "migrate",
        "feature",
        feature,
        "--to",
        "fastapi",
        "--out",
        "migrated/pets.py",
    ];
    if let Some(intent) = intent {
        args.push(intent);
    }
    args
}

fn reconstruct_plan(reviewed: &Value, compact: Value) -> Value {
    let mut reconstructed = reviewed.clone();
    let mut compact = compact.as_object().unwrap().clone();
    let omitted = compact.remove("plan_context_omitted").unwrap();
    for key in omitted.as_array().unwrap() {
        assert!(reviewed.get(key.as_str().unwrap()).is_some());
    }
    reconstructed.as_object_mut().unwrap().extend(compact);
    reconstructed
}

#[test]
fn feature_migration_preview_binds_scope_and_separates_decisions() {
    let dir = fixture();
    let feature = feature(dir.path());
    let report = ok(dir.path(), &migration(&feature, None));

    assert_eq!(report["query"], "migration");
    assert_eq!(report["migration"]["feature"], feature);
    assert_eq!(report["migration"]["source_framework"], "nextjs-app");
    assert_eq!(report["migration"]["target_framework"], "fastapi");
    assert_eq!(
        report["migration"]["source_files"],
        serde_json::json!(["app/api/pets/route.ts"])
    );
    assert_eq!(
        report["migration"]["destination_files"],
        serde_json::json!(["migrated/pets.py"])
    );
    assert_eq!(report["migration"]["coexistence"]["source_retained"], true);
    assert_eq!(report["contract"]["semantic_translation_agreement"], true);
    assert_eq!(
        report["migration"]["coexistence"]["destination_registration"],
        "agent-decision"
    );
    assert_eq!(
        report["contract"]["endpoints"],
        serde_json::json!([
            {"method": "GET", "url": "/api/pets"},
            {"method": "POST", "url": "/api/pets"}
        ])
    );
    assert_eq!(report["steps"]["automatic"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["steps"]["agent_decisions"].as_array().unwrap().len(),
        4
    );
    assert_eq!(
        report["migration"]["dependencies"]["status"],
        "agent-decision"
    );
    assert!(!report["steps"]["unsupported"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|fact| { fact["kind"] == "route" && fact["disposition"] == "automatic" }));
    assert!(report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|fact| { fact["kind"] == "package-gap" && fact["disposition"] == "unsupported" }));
    assert_eq!(report["applied"], false);
    assert_eq!(report["saved"], false);
    assert!(!dir.path().join("migrated/pets.py").exists());
}

#[test]
fn migration_uses_the_selected_route_instead_of_fixture_names() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/internal/telemetry/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(
        route,
        "export async function PATCH(request: Request) {\n  return Response.json(await request.json());\n}\n\nexport async function DELETE() {\n  return new Response(null, { status: 204 });\n}\n",
    )
    .unwrap();
    let feature = feature(dir.path());
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "feature",
            &feature,
            "--to",
            "fastapi",
            "--out",
            "services/telemetry.py",
        ],
    );
    assert_eq!(
        report["contract"]["endpoints"],
        serde_json::json!([
            {"method": "DELETE", "url": "/internal/telemetry"},
            {"method": "PATCH", "url": "/internal/telemetry"}
        ])
    );
    assert_eq!(
        report["migration"]["destination_files"],
        serde_json::json!(["services/telemetry.py"])
    );
}

#[test]
fn explicit_fastapi_registration_joins_the_reversible_migration_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend")).unwrap();
    fs::write(
        route,
        "export async function GET() {\n  return Response.json({ state: \"ready\" });\n}\n",
    )
    .unwrap();
    let application = dir.path().join("backend/main.py");
    let original =
        "from fastapi import FastAPI as API\n\napplication = API()\nfr_migrated_router = 'occupied'\n";
    fs::write(&application, original).unwrap();
    let selected = feature_for_framework(dir.path(), "nextjs-app");
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "backend/routes/signals.py",
            "--register-with",
            "backend/main.py::application",
            "--write",
        ],
    );

    assert_eq!(
        report["migration"]["connected_files"],
        serde_json::json!(["backend/main.py"])
    );
    assert_eq!(
        report["migration"]["coexistence"]["destination_registration"],
        "automatic"
    );
    assert_eq!(
        report["migration"]["target_application"]["application"],
        "application"
    );
    assert!(report["steps"]["automatic"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["action"] == "register-fastapi-router-with-application"));
    let registered = fs::read_to_string(&application).unwrap();
    assert!(registered.contains("from backend.routes.signals import router as fr_migrated_router_"));
    assert!(registered.contains("application.include_router(fr_migrated_router_)"));
    assert!(dir.path().join("backend/routes/signals.py").exists());

    ok(dir.path(), &["history", "undo", "1", "--write"]);
    assert_eq!(fs::read_to_string(&application).unwrap(), original);
    assert!(!dir.path().join("backend/routes/signals.py").exists());
    ok(dir.path(), &["history", "redo", "1", "--write"]);
    assert_eq!(fs::read_to_string(&application).unwrap(), registered);
}

#[test]
fn explicit_python_dependencies_join_the_reversible_migration_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/measurements/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend")).unwrap();
    fs::write(
        route,
        "// => generic route fixture\ninterface Measurement {\n  source_id: string;\n  values: number[];\n}\nexport async function POST(request: Request) {\n  const measurement: Measurement = await request.json();\n  return Response.json(measurement);\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("backend/main.py"),
        "from fastapi import FastAPI\napplication = FastAPI()\n",
    )
    .unwrap();
    let manifest = dir.path().join("backend/pyproject.toml");
    let original_manifest =
        "[project]\nname = \"measurement-service\"\ndependencies = [\"uvicorn>=0.30\"]\n";
    fs::write(&manifest, original_manifest).unwrap();
    let selected = feature_for_framework(dir.path(), "nextjs-app");
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "backend/routes/measurements.py",
            "--register-with",
            "backend/main.py::application",
            "--dependency-manifest",
            "backend/pyproject.toml",
            "--dependency-requirement",
            "fastapi>=0.115,<1",
            "--dependency-requirement",
            "pydantic>=2,<3",
            "--write",
        ],
    );

    assert_eq!(
        report["migration"]["connected_files"],
        serde_json::json!(["backend/main.py", "backend/pyproject.toml"])
    );
    assert_eq!(report["migration"]["dependencies"]["status"], "updated");
    assert!(report["migration"]["dependencies"]["manifest_revision"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        report["migration"]["dependencies"]["required"],
        serde_json::json!(["fastapi", "pydantic"])
    );
    assert_eq!(
        report["migration"]["dependencies"]["added"],
        serde_json::json!(["fastapi>=0.115,<1", "pydantic>=2,<3"])
    );
    let updated_manifest = fs::read_to_string(&manifest).unwrap();
    assert!(updated_manifest.contains("\"uvicorn>=0.30\""));
    assert!(updated_manifest.contains("\"fastapi>=0.115,<1\""));
    assert!(updated_manifest.contains("\"pydantic>=2,<3\""));
    assert!(report["steps"]["automatic"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["action"] == "update-python-project-dependencies"));
    let patch = ok(dir.path(), &["history", "patch", "1"])["patch"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(patch.contains("b/backend/pyproject.toml"));
    assert!(patch.contains("fastapi>=0.115,<1"));

    ok(dir.path(), &["history", "undo", "1", "--write"]);
    assert_eq!(fs::read_to_string(&manifest).unwrap(), original_manifest);
    assert!(!dir.path().join("backend/routes/measurements.py").exists());
    ok(dir.path(), &["history", "redo", "1", "--write"]);
    assert_eq!(fs::read_to_string(&manifest).unwrap(), updated_manifest);
}

#[test]
fn python_dependency_edits_refuse_missing_or_unowned_requirements() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/measurements/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend")).unwrap();
    fs::create_dir_all(dir.path().join("unrelated")).unwrap();
    fs::write(
        route,
        "// => generic route fixture\ninterface Measurement { value: number; }\nexport async function POST(request: Request) {\n  const measurement: Measurement = await request.json();\n  return Response.json(measurement);\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("backend/pyproject.toml"),
        "[project]\nname = \"backend\"\ndependencies = []\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("unrelated/pyproject.toml"),
        "[project]\nname = \"unrelated\"\ndependencies = []\n",
    )
    .unwrap();
    let selected = feature(dir.path());
    let base = [
        "migrate",
        "feature",
        &selected,
        "--to",
        "fastapi",
        "--out",
        "backend/routes/measurements.py",
        "--dependency-manifest",
    ];
    let mut missing = base.to_vec();
    missing.extend([
        "backend/pyproject.toml",
        "--dependency-requirement",
        "fastapi>=0.115,<1",
    ]);
    let (success, report) = run(dir.path(), &missing);
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("each missing generated runtime import exactly once"));

    let mut unowned = base.to_vec();
    unowned.extend([
        "unrelated/pyproject.toml",
        "--dependency-requirement",
        "fastapi>=0.115,<1",
        "--dependency-requirement",
        "pydantic>=2,<3",
    ]);
    let (success, report) = run(dir.path(), &unowned);
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("must be an ancestor"));
    assert!(!dir.path().join("backend/routes/measurements.py").exists());
}

#[test]
fn explicit_cutover_removes_and_restores_the_source_in_the_migration_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend")).unwrap();
    let original_route =
        "export async function GET() { return Response.json({ state: \"ready\" }); }\n";
    fs::write(&route, original_route).unwrap();
    let application = dir.path().join("backend/main.py");
    let original_application = "from fastapi import FastAPI\n\napplication = FastAPI()\n";
    fs::write(&application, original_application).unwrap();
    fs::create_dir(dir.path().join(".fr")).unwrap();
    let checks_path = dir.path().join(".fr/checks.json");
    let checks_configuration = serde_json::json!({
        "schema": 1,
        "checks": [{
            "name": "migration",
            "argv": ["python3", "-c", "import pathlib;assert pathlib.Path('backend/routes/signals.py').is_file();assert not pathlib.Path('app/signals/route.ts').exists() #."],
            "cwd": ".",
            "timeout_seconds": 5,
            "covers": ["registered destination and source cutover"]
        }, {
            "name": "syntax",
            "argv": ["python3", "-m", "py_compile", "backend/routes/signals.py"],
            "cwd": ".",
            "timeout_seconds": 5,
            "covers": ["generated destination syntax"]
        }, {
            "name": "unrelated",
            "argv": ["python3", "-c", "import pathlib;pathlib.Path('should-not-run').write_text('ran')"],
            "cwd": ".",
            "timeout_seconds": 5,
            "covers": ["an unrelated command"]
        }]
    });
    let checks_bytes = serde_json::to_vec(&checks_configuration).unwrap();
    fs::write(&checks_path, &checks_bytes).unwrap();
    let selected = feature_for_framework(dir.path(), "nextjs-app");
    let args = [
        "migrate",
        "feature",
        &selected,
        "--to",
        "fastapi",
        "--out",
        "backend/routes/signals.py",
        "--register-with",
        "backend/main.py::application",
        "--check",
        "syntax",
        "--check",
        "migration",
        "--cutover",
    ];
    let preview = ok(dir.path(), &args);
    assert_eq!(preview["migration"]["coexistence"]["cutover_planned"], true);
    assert_eq!(
        preview["migration"]["coexistence"]["cutover_applied"],
        false
    );
    assert!(route.exists());
    let mut write_args = args.to_vec();
    write_args.push("--write");
    let report = ok(dir.path(), &write_args);
    assert_eq!(report["migration"]["coexistence"]["source_retained"], false);
    assert_eq!(report["migration"]["coexistence"]["cutover_applied"], true);
    assert_eq!(report["migration"]["verification"]["status"], "bound");
    assert_eq!(
        report["migration"]["verification"]["checks"],
        serde_json::json!(["migration", "syntax"])
    );
    assert!(!route.exists());
    assert!(dir.path().join("backend/routes/signals.py").exists());
    let registered = fs::read_to_string(&application).unwrap();
    let check_basis = ok(dir.path(), &["checks"])["basis"]
        .as_str()
        .unwrap()
        .to_owned();
    let (success, mismatch) = run(
        dir.path(),
        &[
            "checks",
            "--run",
            "unrelated",
            "--basis",
            &check_basis,
            "--record-for",
            "1",
        ],
    );
    assert!(!success);
    assert!(mismatch["error"]["message"]
        .as_str()
        .unwrap()
        .contains("required selection"));
    assert!(!dir.path().join("should-not-run").exists());
    let mut changed_configuration = checks_configuration;
    changed_configuration["checks"][0]["argv"] = serde_json::json!([
        "python3",
        "-c",
        "import pathlib;pathlib.Path('changed-check-ran').write_text('ran')"
    ]);
    fs::write(
        &checks_path,
        serde_json::to_vec(&changed_configuration).unwrap(),
    )
    .unwrap();
    let changed_basis = ok(dir.path(), &["checks"])["basis"]
        .as_str()
        .unwrap()
        .to_owned();
    let (success, mismatch) = run(
        dir.path(),
        &[
            "checks",
            "--run",
            "syntax,migration",
            "--basis",
            &changed_basis,
            "--record-for",
            "1",
        ],
    );
    assert!(!success);
    assert!(mismatch["error"]["message"]
        .as_str()
        .unwrap()
        .contains("required selection"));
    assert!(!dir.path().join("changed-check-ran").exists());
    fs::write(&checks_path, checks_bytes).unwrap();
    let evidence = ok(
        dir.path(),
        &[
            "checks",
            "--run",
            "syntax,migration",
            "--basis",
            &check_basis,
            "--record-for",
            "1",
            "--quiet-success",
        ],
    );
    assert_eq!(evidence["passed"], true);
    assert_eq!(evidence["recorded_evidence"]["transaction"], 1);
    assert!(evidence["recorded_evidence"]["receipt"]
        .as_str()
        .unwrap()
        .starts_with("frce1:"));
    let history = ok(dir.path(), &["history", "show", "1"]);
    assert_eq!(
        history["records"][0]["required_checks"]["checks"],
        serde_json::json!(["migration", "syntax"])
    );
    assert_eq!(
        history["records"][0]["check_evidence"][0]["checks"],
        serde_json::json!(["migration", "syntax"])
    );

    let patch = ok(dir.path(), &["history", "patch", "1"]);
    assert!(patch["patch"]
        .as_str()
        .unwrap()
        .contains("deleted file mode"));
    ok(dir.path(), &["history", "undo", "1", "--write"]);
    assert_eq!(fs::read_to_string(&route).unwrap(), original_route);
    assert_eq!(
        fs::read_to_string(&application).unwrap(),
        original_application
    );
    assert!(!dir.path().join("backend/routes/signals.py").exists());
    ok(dir.path(), &["history", "redo", "1", "--write"]);
    assert!(!route.exists());
    assert_eq!(fs::read_to_string(&application).unwrap(), registered);
    let replayed = ok(dir.path(), &["history", "show", "1"]);
    assert_eq!(
        replayed["records"][0]["required_checks"]["checks"],
        serde_json::json!(["migration", "syntax"])
    );
    assert_eq!(
        replayed["records"][0]["check_evidence"][0]["checks"],
        serde_json::json!(["migration", "syntax"])
    );
    let state_path = dir.path().join(".fr-history/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    state["records"][0]["required_checks"]["checks"] =
        serde_json::json!(["migration", "migration"]);
    fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
    let (success, corrupt) = run(dir.path(), &["history", "show", "1"]);
    assert!(!success);
    assert!(corrupt["error"]["message"]
        .as_str()
        .unwrap()
        .contains("invalid required check selection"));
}

#[test]
fn fastapi_registration_refuses_unrecognized_apps_and_direct_route_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(
        route,
        "export async function GET() { return Response.json({ state: \"ready\" }); }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("main.py"),
        "from fastapi import FastAPI\napplication = FastAPI()\n@application.get('/signals')\ndef existing(): return {}\n",
    )
    .unwrap();
    let selected = feature_for_framework(dir.path(), "nextjs-app");
    let base = [
        "migrate",
        "feature",
        &selected,
        "--to",
        "fastapi",
        "--out",
        "generated/signals.py",
        "--register-with",
    ];
    let mut conflict = base.to_vec();
    conflict.push("main.py::application");
    let (success, report) = run(dir.path(), &conflict);
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no direct endpoint conflict"));

    let mut unknown = base.to_vec();
    unknown.push("main.py::unknown");
    assert!(!run(dir.path(), &unknown).0);

    let (success, report) = run(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "generated/signals.py",
            "--cutover",
        ],
    );
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("requires automatic destination registration"));

    let fastapi_feature = feature_for_framework(dir.path(), "fastapi");
    let (success, report) = run(
        dir.path(),
        &[
            "migrate",
            "feature",
            &fastapi_feature,
            "--to",
            "nextjs",
            "--out",
            "web/app",
            "--register-with",
            "main.py::application",
        ],
    );
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("applies only to a FastAPI destination"));
    assert!(!dir.path().join("generated/signals.py").exists());
}

#[test]
fn cutover_refuses_a_resolved_external_source_reference() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend")).unwrap();
    fs::write(
        &route,
        "export async function GET() { return Response.json({ state: \"ready\" }); }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("consumer.ts"),
        "import { GET } from './app/signals/route';\nexport async function probe() { return GET(); }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("backend/main.py"),
        "from fastapi import FastAPI\napplication = FastAPI()\n",
    )
    .unwrap();
    let selected = feature_for_framework(dir.path(), "nextjs-app");
    let (success, report) = run(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "backend/routes/signals.py",
            "--register-with",
            "backend/main.py::application",
            "--cutover",
        ],
    );
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no resolved external source references"));
    assert!(route.exists());
    assert!(!dir.path().join("backend/routes/signals.py").exists());
}

#[test]
fn migration_compares_generic_declared_schema_shapes_after_generation() {
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/measurements/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(
        route,
        "interface Measurement {\n  sensor_id: string;\n  values: number[];\n  active: boolean;\n}\n\nexport async function POST(request: Request): Promise<Response> {\n  const measurement: Measurement = await request.json();\n  return Response.json(measurement);\n}\n",
    )
    .unwrap();
    let feature = feature(dir.path());
    let report = ok(
        dir.path(),
        &[
            "migrate",
            "feature",
            &feature,
            "--to",
            "fastapi",
            "--out",
            "services/measurements.py",
        ],
    );
    let schemas = &report["contract"]["declared_schemas"];
    assert_eq!(schemas["status"], "agreed");
    assert_eq!(schemas["translation_agreement"], true);
    assert_eq!(schemas["wire_schema_agreement"], "unverified");
    assert_eq!(schemas["source"], schemas["generated"]);
    assert_eq!(schemas["source"][0]["name"], "Measurement");
    assert_eq!(
        schemas["source"][0]["fields"],
        serde_json::json!([
            {"name": "active", "declared_type": "bool"},
            {"name": "sensor_id", "declared_type": "string"},
            {"name": "values", "declared_type": "list<number>"}
        ])
    );
}

#[test]
fn feature_migration_replays_through_history_and_exports_a_git_patch() {
    let dir = fixture();
    let source = dir.path().join("app/api/pets/route.ts");
    let original = fs::read(&source).unwrap();
    let feature = feature(dir.path());
    let saved = ok(dir.path(), &migration(&feature, Some("--save-plan")));
    assert_eq!(saved["transaction"], 1);
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["applied"], false);

    let checked = ok(dir.path(), &["history", "patch", "1", "--check"]);
    assert_eq!(checked["matches_recorded_snapshots"], true);
    let exported = ok(dir.path(), &["history", "patch", "1"]);
    let patch = exported["patch"].as_str().unwrap();
    assert!(patch.contains("b/migrated/pets.py"), "{patch}");
    assert!(patch.contains("@router.get(\"/api/pets\")"), "{patch}");

    ok(dir.path(), &["history", "apply", "1", "--write"]);
    let destination = dir.path().join("migrated/pets.py");
    let migrated = fs::read(&destination).unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(String::from_utf8_lossy(&migrated).contains("@router.post(\"/api/pets\")"));

    ok(dir.path(), &["history", "undo", "1", "--write"]);
    assert!(!destination.exists());
    assert_eq!(fs::read(&source).unwrap(), original);
    ok(dir.path(), &["history", "redo", "1", "--write"]);
    assert_eq!(fs::read(&destination).unwrap(), migrated);
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn reviewed_plan_basis_compacts_migration_and_reconstructs_exactly() {
    let root = fixture();
    let selected = feature(root.path());
    let reviewed = ok(root.path(), &migration(&selected, None));
    let basis = reviewed["plan_context_basis"].as_str().unwrap();
    assert!(basis.starts_with("frpb1:"));
    let mut args = migration(&selected, Some("--save-plan"));
    args.extend(["--plan-basis", basis]);
    let compact = ok(root.path(), &args);
    assert!(compact.get("diff").is_none(), "{compact}");
    assert!(compact.get("steps").is_none(), "{compact}");
    assert_eq!(compact["saved"], true);
    let mut full = ok(root.path(), &migration(&selected, Some("--save-plan")));
    full["saved"] = serde_json::json!(true);
    full.as_object_mut().unwrap().remove("reused_transaction");
    assert_eq!(reconstruct_plan(&reviewed, compact), full);
}

#[test]
fn changed_migration_options_refuse_a_reviewed_plan_before_history() {
    let dir = fixture();
    let selected = feature(dir.path());
    let reviewed = ok(dir.path(), &migration(&selected, None));
    let basis = reviewed["plan_context_basis"].as_str().unwrap();
    let (success, error) = run(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "other/pets.py",
            "--save-plan",
            "--plan-basis",
            basis,
        ],
    );
    assert!(!success, "{error}");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale or conflicting plan basis"));
    assert!(!dir.path().join(".fr-history").exists());
    assert!(!dir.path().join("migrated/pets.py").exists());
    assert!(!dir.path().join("other/pets.py").exists());
}

#[test]
fn exported_migration_patch_round_trips_in_a_clean_receiver() {
    let missing = (!Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success()))
    .then_some("git".to_owned())
    .into_iter()
    .collect::<Vec<_>>();
    common::require_on_ci("feature migration clean receiver", &missing);
    if !missing.is_empty() {
        return;
    }

    let producer = tempfile::tempdir().unwrap();
    let source_path = "app/operations/[operationId]/route.ts";
    let source = producer.path().join(source_path);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    let original = "export async function GET(_request: Request, context: { params: { operationId: string } }) {\n  return Response.json({ operation_id: context.params.operationId, state: \"ready\" });\n}\n";
    fs::write(&source, original).unwrap();
    let selected = feature(producer.path());
    let saved = ok(
        producer.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "services/operations.py",
            "--save-plan",
        ],
    );
    let transaction = saved["transaction"].as_u64().unwrap().to_string();
    let exported = ok(producer.path(), &["history", "patch", &transaction]);
    let patch_file = tempfile::NamedTempFile::new().unwrap();
    fs::write(patch_file.path(), exported["patch"].as_str().unwrap()).unwrap();

    ok(
        producer.path(),
        &["history", "apply", &transaction, "--write"],
    );
    let expected = fs::read(producer.path().join("services/operations.py")).unwrap();

    let receiver = tempfile::tempdir().unwrap();
    let receiver_source = receiver.path().join(source_path);
    fs::create_dir_all(receiver_source.parent().unwrap()).unwrap();
    fs::write(&receiver_source, original).unwrap();
    git(receiver.path(), &["init", "-q"]);
    let patch = patch_file.path().to_str().unwrap();
    git(receiver.path(), &["apply", "--check", patch]);
    git(receiver.path(), &["apply", patch]);
    let destination = receiver.path().join("services/operations.py");
    assert_eq!(fs::read(&destination).unwrap(), expected);
    assert_eq!(fs::read(&receiver_source).unwrap(), original.as_bytes());

    git(receiver.path(), &["apply", "--reverse", "--check", patch]);
    git(receiver.path(), &["apply", "--reverse", patch]);
    assert!(!destination.exists());
    assert_eq!(fs::read(&receiver_source).unwrap(), original.as_bytes());
    git(receiver.path(), &["apply", "--check", patch]);
    git(receiver.path(), &["apply", patch]);
    assert_eq!(fs::read(&destination).unwrap(), expected);
}

#[test]
fn feature_migration_rejects_stale_scope_same_framework_and_escaping_output() {
    let dir = fixture();
    let stale_feature = feature(dir.path());
    let route = dir.path().join("app/api/pets/route.ts");
    fs::write(
        route,
        "export async function GET() { return Response.json([]); }\n",
    )
    .unwrap();
    assert!(!run(dir.path(), &migration(&stale_feature, None)).0);

    let fresh = feature(dir.path());
    let mut same = migration(&fresh, None);
    same[4] = "nextjs";
    assert!(!run(dir.path(), &same).0);

    let mut escaping = migration(&fresh, None);
    escaping[6] = "../outside.py";
    let (success, report) = run(dir.path(), &escaping);
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("workspace-relative"));
}

#[test]
fn a_dynamic_fastapi_feature_previews_as_a_nextjs_app_route() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("routes.py"),
        "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get('/accounts/{account_id}/audit-log')\nasync def read_audit_log(account_id: str):\n    return []\n\n@router.delete('/accounts/{account_id}/audit-log')\nasync def clear_audit_log(account_id: str):\n    return {'account': account_id}\n",
    )
    .unwrap();
    let feature = feature(dir.path());
    let report = ok(
        dir.path(),
        &[
            "migrate", "feature", &feature, "--to", "nextjs", "--out", "web/app",
        ],
    );
    assert_eq!(report["migration"]["source_framework"], "fastapi");
    assert_eq!(report["migration"]["target_framework"], "nextjs");
    assert_eq!(
        report["migration"]["destination_files"],
        serde_json::json!(["web/app/accounts/[account_id]/audit-log/route.ts"])
    );
    assert_eq!(
        report["contract"]["endpoints"],
        serde_json::json!([
            {"method": "DELETE", "url": "/accounts/{account_id}/audit-log"},
            {"method": "GET", "url": "/accounts/{account_id}/audit-log"}
        ])
    );
    assert_eq!(report["contract"]["semantic_translation_agreement"], true);
    assert!(!dir
        .path()
        .join("web/app/accounts/[account_id]/audit-log/route.ts")
        .exists());
}

#[test]
fn a_captured_nextjs_app_registers_the_migrated_route_by_placement() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("web")).unwrap();
    fs::write(
        dir.path().join("web/package.json"),
        "{\"private\":true,\"dependencies\":{\"next\":\"15.4.0\"}}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("routes.py"),
        "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get('/operations/{operation_id}')\nasync def read_operation(operation_id: str):\n    return {'operation_id': operation_id}\n",
    )
    .unwrap();
    let feature = feature(dir.path());
    let report = ok(
        dir.path(),
        &[
            "migrate", "feature", &feature, "--to", "nextjs", "--out", "web/app",
        ],
    );
    assert_eq!(
        report["migration"]["target_application"]["manifest"],
        "web/package.json"
    );
    assert_eq!(
        report["migration"]["dependencies"],
        serde_json::json!({
            "status": "satisfied",
            "manifest": "web/package.json",
            "required": ["next"],
            "declared": ["next"],
            "added": [],
            "basis": "captured-nextjs-dependency"
        })
    );
    assert_eq!(
        report["migration"]["coexistence"]["destination_registration"],
        "automatic"
    );
    assert!(report["steps"]["automatic"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["action"] == "register-nextjs-route-by-app-router-placement"));
    assert_eq!(
        report["steps"]["agent_decisions"].as_array().unwrap().len(),
        2
    );
    assert!(!dir
        .path()
        .join("web/app/operations/[operation_id]/route.ts")
        .exists());

    let src_report = ok(
        dir.path(),
        &[
            "migrate",
            "feature",
            &feature,
            "--to",
            "nextjs",
            "--out",
            "web/src/app",
        ],
    );
    assert_eq!(
        src_report["migration"]["coexistence"]["destination_registration"],
        "automatic"
    );
}

#[test]
fn fastapi_migration_compares_only_models_reached_by_handler_signatures() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("routes.py"),
        "from fastapi import APIRouter\nfrom pydantic import BaseModel\n\nrouter = APIRouter()\n\nclass Reading(BaseModel):\n    device_id: str\n    samples: list[float]\n    valid: bool\n\nclass InternalCache:\n    value: str\n\n@router.post('/readings', response_model=Reading)\nasync def record_reading(reading: Reading) -> Reading:\n    return reading\n",
    )
    .unwrap();
    let feature = feature(dir.path());
    let report = ok(
        dir.path(),
        &[
            "migrate", "feature", &feature, "--to", "nextjs", "--out", "web/app",
        ],
    );
    let schemas = &report["contract"]["declared_schemas"];
    assert_eq!(schemas["status"], "agreed");
    assert_eq!(schemas["translation_agreement"], true);
    assert_eq!(schemas["source"].as_array().unwrap().len(), 1);
    assert_eq!(schemas["source"][0]["name"], "Reading");
    assert!(schemas["generated"]
        .as_array()
        .unwrap()
        .iter()
        .any(|shape| shape == &schemas["source"][0]));
    assert!(!schemas["source"].to_string().contains("InternalCache"));
}
