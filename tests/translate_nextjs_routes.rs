//! A FastAPI application written as a Next.js App Router tree.
//!
//! The reverse of `fr translate <route.ts> fastapi`, and not its mirror image. A Next.js
//! route's URL is where its file sits, so one Python module becomes a tree. Two handlers
//! on one URL are two exports of one `route.ts`, and a path parameter is a directory.
//!
//! The contract is what has to survive. The last test builds an OpenAPI document from the
//! Python and another from the TypeScript it became, and compares the two.

use fun_refactor::transpile::fastapi;
use std::path::PathBuf;

const APP: &str = "\
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()


class Pet(BaseModel):
    name: str
    age: int


@app.get(\"/pets/{pet_id}\")
async def read_pet(pet_id: int) -> Pet:
    pet = lookup(pet_id)
    if pet is None:
        return {\"error\": \"not found\"}
    return pet


@app.post(\"/pets\")
async def create_pet(pet: Pet) -> Pet:
    saved = save(pet)
    return saved


@app.get(\"/pets\")
async def list_pets(limit: str) -> list:
    return search(limit)


@app.get(\"/files/{path:path}\")
async def read_file(path: str) -> str:
    return open_file(path)
";

/// Translate a module and hand back the temporary directory it wrote into.
fn translate(source: &str) -> (tempfile::TempDir, fastapi::AppPlan) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("main.py");
    std::fs::write(&path, source).expect("write");
    let out = tmp.path().join("app").join("api");
    let plan = fastapi::plan_to(&path, Some(&out), false).expect("a plan");
    (tmp, plan)
}

/// The file serving one URL, by the route this tool reports for it.
fn route<'a>(plan: &'a fastapi::AppPlan, url: &str) -> &'a fastapi::RouteFile {
    plan.routes
        .iter()
        .find(|route| route.route == url)
        .unwrap_or_else(|| {
            let found: Vec<&str> = plan.routes.iter().map(|r| r.route.as_str()).collect();
            panic!("no route {url}, only {found:?}")
        })
}

#[test]
fn one_url_is_one_file_however_many_methods_it_answers() {
    let (_tmp, plan) = translate(APP);
    let pets = route(&plan, "/pets");
    assert_eq!(pets.methods.len(), 2, "{:?}", pets.methods);
    assert!(
        pets.output.contains("export async function GET("),
        "{pets:?}"
    );
    assert!(
        pets.output.contains("export async function POST("),
        "{pets:?}"
    );
}

#[test]
fn a_path_parameter_is_a_directory() {
    let (tmp, plan) = translate(APP);
    let one = route(&plan, "/pets/[petId]");
    assert_eq!(
        one.destination,
        tmp.path().join("app/api/pets/[petId]/route.ts"),
        "the URL is where the file sits"
    );
}

#[test]
fn a_catch_all_keeps_its_reach() {
    // `{path:path}` matches slashes, and Next.js spells that `[...path]`. Written
    // `[path]`, the route would answer `/files/a` and miss `/files/a/b`.
    let (_tmp, plan) = translate(APP);
    let files = route(&plan, "/files/[...path]");
    assert!(
        files
            .destination
            .to_string_lossy()
            .contains("files/[...path]"),
        "{files:?}"
    );
}

#[test]
fn a_path_parameter_arrives_as_text_and_is_converted() {
    // `pet_id: int` in Python. `context.params` is strings, so a handler that indexes
    // with it would look up the string "7" in a store keyed by the number 7.
    let (_tmp, plan) = translate(APP);
    let one = route(&plan, "/pets/[petId]");
    assert!(
        one.output
            .contains("const petId = Number(context.params.petId);"),
        "{}",
        one.output
    );
}

#[test]
fn a_model_parameter_is_the_request_body() {
    let (_tmp, plan) = translate(APP);
    let pets = route(&plan, "/pets");
    assert!(
        pets.output
            .contains("const pet: Pet = await request.json();"),
        "{}",
        pets.output
    );
}

#[test]
fn anything_else_is_a_query_parameter() {
    let (_tmp, plan) = translate(APP);
    let pets = route(&plan, "/pets");
    assert!(
        pets.output
            .contains("const limit = new URL(request.url).searchParams.get(\"limit\");"),
        "{}",
        pets.output
    );
}

#[test]
fn a_model_crosses_without_the_framework_it_was_declared_to() {
    // `class Pet(BaseModel)` says "FastAPI validates this shape". The shape crosses;
    // carried as inheritance it would name a type the route file never declares.
    let (_tmp, plan) = translate(APP);
    let pets = route(&plan, "/pets");
    assert!(pets.output.contains("export interface Pet {"), "{pets:?}");
    assert!(!pets.output.contains("BaseModel"), "{}", pets.output);
}

#[test]
fn every_returned_value_becomes_a_response() {
    // FastAPI serialises what a handler returns. A Next.js handler returns the response,
    // so a body carrying the value alone answers with an object where a `Response` goes.
    let (_tmp, plan) = translate(APP);
    let one = route(&plan, "/pets/[petId]");
    assert!(
        one.output.contains("return Response.json(pet);"),
        "{}",
        one.output
    );
    assert!(
        one.output
            .contains("return Response.json({ error: \"not found\" });"),
        "the return inside the `if` too: {}",
        one.output
    );
}

#[test]
fn a_module_with_no_endpoint_is_refused() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("helpers.py");
    std::fs::write(&path, "def add(a, b):\n    return a + b\n").expect("write");
    let refusal =
        fastapi::plan_to(&path, None, false).expect_err("a module of helpers is not a route tree");
    assert!(
        refusal.to_string().contains("declares no endpoint"),
        "{refusal}"
    );
}

#[test]
fn the_contract_survives_the_crossing() {
    // What the whole translation is for. The document built from the Python and the one
    // built from the TypeScript it became must name the same URLs and the same methods.
    let (tmp, plan) = translate(APP);
    let source = tmp.path().join("main.py");
    let before =
        fun_refactor::openapi::from_fastapi("pets", tmp.path(), &[source]).expect("a baseline");

    let written: Vec<PathBuf> = plan
        .routes
        .iter()
        .map(|route| {
            std::fs::create_dir_all(route.destination.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&route.destination, &route.output).expect("write");
            route.destination.clone()
        })
        .collect();
    let after =
        fun_refactor::openapi::from_routes("pets", tmp.path(), &written).expect("a baseline");

    assert_eq!(
        endpoints(&before.document),
        endpoints(&after.document),
        "the same URLs and methods"
    );
}

/// Every `(path, method)` a document declares, sorted.
fn endpoints(document: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let paths = document
        .get("paths")
        .and_then(|p| p.as_object())
        .expect("a paths object");
    for (path, methods) in paths {
        for method in methods.as_object().expect("an operations object").keys() {
            found.push(format!("{method} {path}"));
        }
    }
    found.sort();
    found
}

/// The route tree is written under the directory the caller named.
#[test]
fn the_tree_is_written_where_it_was_asked_for() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("main.py");
    std::fs::write(&path, APP).expect("write");
    let out = tmp.path().join("web").join("app").join("api");
    let plan = fastapi::plan_to(&path, Some(&out), false).expect("a plan");
    for route in &plan.routes {
        assert!(
            route.destination.starts_with(&out),
            "{}",
            route.destination.display()
        );
    }
}

/// A file that is not Python is refused before anything is read out of it.
#[test]
fn only_python_declares_a_fastapi_application() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("route.ts");
    std::fs::write(&path, "export async function GET() {}\n").expect("write");
    let refusal = fastapi::plan_to(&path, None, false).expect_err("typescript is not FastAPI");
    assert!(refusal.to_string().contains("is typescript"), "{refusal}");
}

/// The reader answers about a file before anything asks it to translate one.
#[test]
fn a_module_says_whether_it_declares_endpoints() {
    assert!(fastapi::is_fastapi_module(APP).expect("a parse"));
    assert!(!fastapi::is_fastapi_module("def add(a, b):\n    return a + b\n").expect("a parse"));
}

/// Two calls with the same input write the same tree.
#[test]
fn the_translation_is_settled() {
    let (_a, first) = translate(APP);
    let (_b, second) = translate(APP);
    let text = |plan: &fastapi::AppPlan| -> Vec<String> {
        plan.routes.iter().map(|r| r.output.clone()).collect()
    };
    assert_eq!(text(&first), text(&second));
}

#[test]
fn a_bare_return_still_answers_with_a_response() {
    // FastAPI serialises a bare `return` as a JSON null with status 200. A Next.js
    // handler that just `return`s answers with nothing at all, which is a different
    // endpoint.
    let (_tmp, plan) = translate(
        "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.post(\"/reset\")\nasync def reset():\n    clear()\n    return\n",
    );
    let reset = route(&plan, "/reset");
    assert!(
        reset.output.contains("return Response.json(null);"),
        "{}",
        reset.output
    );
}
