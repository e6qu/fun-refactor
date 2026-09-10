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
        2
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
    assert!(!route.exists());
    assert!(dir.path().join("backend/routes/signals.py").exists());
    let registered = fs::read_to_string(&application).unwrap();

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
        1
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
