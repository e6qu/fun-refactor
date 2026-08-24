//! A service skeleton written from an OpenAPI document.
//!
//! `fr openapi` derives a document from a service; this is the other direction. The rule
//! that governs the derivation governs the writing: invent nothing. Paths, methods,
//! parameters and schemas come from the document. A handler body is in no document, so
//! every generated handler answers 501 out loud rather than returning a plausible empty
//! value. A service that answers `[]` looks finished.
//!
//! The last two tests close the loop: the contract read back out of the scaffold names
//! the same endpoints the document declared.

use fun_refactor::transpile::scaffold::{self, Target};
use std::path::PathBuf;

// The document under test, in its own file so the fixture reads as YAML.
const DOCUMENT: &str = include_str!("scaffold_petstore.yaml");

fn scaffolded(target: Target) -> (tempfile::TempDir, scaffold::ScaffoldPlan) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("openapi.yaml");
    std::fs::write(&path, DOCUMENT).expect("write");
    // The api root, the same place the FastAPI-to-Next.js translation writes: a
    // Next.js route is only a route under `app/api`.
    let out = tmp.path().join("app").join("api");
    let plan = scaffold::plan_to(&path, target, Some(&out), false).expect("a plan");
    (tmp, plan)
}

/// Every `(method, URL)` a plan's files answer, sorted.
fn served(plan: &scaffold::ScaffoldPlan) -> Vec<String> {
    let mut all: Vec<String> = plan
        .files
        .iter()
        .flat_map(|f| f.endpoints.iter())
        .map(|(method, route)| format!("{method} {route}"))
        .collect();
    all.sort();
    all
}

#[test]
fn a_fastapi_scaffold_is_one_module_answering_every_operation() {
    let (_tmp, plan) = scaffolded(Target::FastApi);
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        served(&plan),
        ["GET /pets", "GET /pets/{petId}", "POST /pets"]
    );
}

#[test]
fn a_nextjs_scaffold_is_one_file_per_url() {
    let (_tmp, plan) = scaffolded(Target::NextJs);
    let mut destinations: Vec<String> = plan
        .files
        .iter()
        .map(|f| {
            f.destination
                .to_string_lossy()
                .rsplit("/app/api/")
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    destinations.sort();
    assert_eq!(destinations, ["pets/[petId]/route.ts", "pets/route.ts"]);
}

#[test]
fn a_schema_becomes_a_model_with_its_required_fields() {
    let (_tmp, plan) = scaffolded(Target::FastApi);
    let output = &plan.files[0].output;
    assert!(output.contains("class PetCreate(BaseModel):"), "{output}");
    assert!(output.contains("    name: str\n"), "{output}");
    assert!(
        output.contains("    age: int | None = None"),
        "optional stays optional: {output}"
    );
    assert!(
        output.contains("    tags: list[str] | None = None"),
        "{output}"
    );
}

#[test]
fn a_body_reference_types_the_handler_parameter() {
    let (_tmp, plan) = scaffolded(Target::FastApi);
    let output = &plan.files[0].output;
    assert!(
        output.contains("async def post_pets(body: PetCreate):"),
        "{output}"
    );
}

#[test]
fn a_path_parameter_is_spelled_the_way_the_signature_spells_it() {
    // FastAPI binds a path parameter by name: `{petId}` over `pet_id: int` never binds,
    // and the route answers 422 for every request.
    let (_tmp, plan) = scaffolded(Target::FastApi);
    let output = &plan.files[0].output;
    assert!(
        output.contains("@router.get(\"/pets/{pet_id}\")\nasync def get_pets_pet_id(pet_id: int):"),
        "{output}"
    );
}

#[test]
fn every_handler_says_not_implemented_out_loud() {
    let (_tmp, fastapi) = scaffolded(Target::FastApi);
    assert_eq!(
        fastapi.files[0].output.matches("status_code=501").count(),
        3,
        "one refusal per handler, none returning a plausible empty value"
    );
    let (_tmp2, nextjs) = scaffolded(Target::NextJs);
    let refusals: usize = nextjs
        .files
        .iter()
        .map(|f| f.output.matches("{ status: 501 }").count())
        .sum();
    assert_eq!(refusals, 3);
}

#[test]
fn a_file_that_is_not_a_document_is_not_scaffolded() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("values.yaml");
    std::fs::write(&path, "replicas: 3\n").expect("write");
    assert!(!scaffold::is_openapi_document(&path));
    let refusal =
        scaffold::plan_to(&path, Target::FastApi, None, false).expect_err("not a document");
    assert!(
        refusal.to_string().contains("no `openapi` key"),
        "{refusal}"
    );
}

#[test]
fn the_nextjs_scaffold_round_trips_through_the_contract() {
    // The closing of the loop: the contract read back out of the tree names the same
    // endpoints the document declared.
    let (_tmp, plan) = scaffolded(Target::NextJs);
    let written: Vec<PathBuf> = plan
        .files
        .iter()
        .map(|f| {
            std::fs::create_dir_all(f.destination.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&f.destination, &f.output).expect("write");
            f.destination.clone()
        })
        .collect();
    let root = written[0]
        .ancestors()
        .find(|a| a.ends_with("app/api"))
        .expect("the api root")
        .to_path_buf();
    let baseline = fun_refactor::openapi::from_routes("pets", &root, &written).expect("a baseline");
    let paths = baseline.document["paths"].as_object().expect("paths");
    let mut urls: Vec<&String> = paths.keys().collect();
    urls.sort();
    // A placeholder's name is internal to the framework, and the derivation spells it
    // snake_case. `/pets/{pet_id}` and `/pets/{petId}` serve the same URLs.
    assert_eq!(urls, ["/pets", "/pets/{pet_id}"]);
    assert!(paths["/pets"].get("get").is_some());
    assert!(paths["/pets"].get("post").is_some());
    assert!(paths["/pets/{pet_id}"].get("get").is_some());
}

#[test]
fn the_fastapi_scaffold_round_trips_through_the_contract() {
    let (_tmp, plan) = scaffolded(Target::FastApi);
    let file = &plan.files[0];
    std::fs::create_dir_all(file.destination.parent().expect("a parent")).expect("mkdir");
    std::fs::write(&file.destination, &file.output).expect("write");
    let root = file.destination.parent().expect("a parent").to_path_buf();
    let baseline =
        fun_refactor::openapi::from_fastapi("pets", &root, std::slice::from_ref(&file.destination))
            .expect("a baseline");
    let paths = baseline.document["paths"].as_object().expect("paths");
    let mut urls: Vec<&String> = paths.keys().collect();
    urls.sort();
    assert_eq!(urls, ["/pets", "/pets/{pet_id}"]);
}

/// A document whose names are not this tool's convention, held to the wire.
const CASED: &str = r##"
openapi: "3.1.0"
info:
  title: cased
  version: "1.0.0"
paths:
  /widgets:
    get:
      parameters:
        - name: pageSize
          in: query
          schema:
            type: integer
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Widget"
components:
  schemas:
    Widget:
      type: object
      required: [displayName]
      properties:
        displayName:
          type: string
        x-vendor-tag:
          type: string
"##;

fn scaffolded_cased(target: Target) -> (tempfile::TempDir, scaffold::ScaffoldPlan) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("openapi.yaml");
    std::fs::write(&path, CASED).expect("write");
    let out = tmp.path().join("app").join("api");
    let plan = scaffold::plan_to(&path, target, Some(&out), false).expect("a plan");
    (tmp, plan)
}

#[test]
fn a_property_name_is_the_wire_contract_and_is_not_re_cased() {
    // `displayName` in the document is the key every request carries. Re-cased to
    // `display_name`, the model would serialise a different JSON than the contract.
    let (_tmp, python) = scaffolded_cased(Target::FastApi);
    assert!(
        python.files[0].output.contains("    displayName: str\n"),
        "{}",
        python.files[0].output
    );
    let (_tmp2, ts) = scaffolded_cased(Target::NextJs);
    let route = ts
        .files
        .iter()
        .find(|f| f.output.contains("interface Widget"))
        .expect("the schema is in the file that uses it");
    assert!(
        route.output.contains("    displayName: string;"),
        "{}",
        route.output
    );
}

#[test]
fn a_query_key_is_the_wire_contract_and_is_not_re_cased() {
    // FastAPI reads the query key from the parameter's own name, so `page_size` would
    // answer a different URL than the document declares.
    let (_tmp, plan) = scaffolded_cased(Target::FastApi);
    assert!(
        plan.files[0].output.contains("pageSize: int | None = None"),
        "{}",
        plan.files[0].output
    );
}

#[test]
fn a_name_python_cannot_declare_is_left_out_loudly() {
    // `x-vendor-tag` is not a Python name. Re-spelled it would change the wire key, so
    // the field is left out and the plan says so. TypeScript can quote any key, so
    // nothing is left out there.
    let (_tmp, python) = scaffolded_cased(Target::FastApi);
    assert!(
        !python.files[0].output.contains("x-vendor-tag"),
        "{}",
        python.files[0].output
    );
    assert!(
        python.notes.iter().any(|n| n.contains("x-vendor-tag")),
        "{:?}",
        python.notes
    );
    let (_tmp2, ts) = scaffolded_cased(Target::NextJs);
    let route = ts
        .files
        .iter()
        .find(|f| f.output.contains("interface Widget"))
        .expect("the schema is in the file that uses it");
    assert!(
        route.output.contains("    \"x-vendor-tag\"?: string;"),
        "{}",
        route.output
    );
}
