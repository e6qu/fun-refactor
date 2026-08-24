//! React server functions, translated and put under contract.
//!
//! `"use server"` at the top of a file makes every exported async function callable from
//! a browser, over a request the framework generates. There is no URL on disk to read,
//! so each function is reached by its own name: `createPet` answers `/create-pet`.
//!
//! That gives the function the two facts a route file keeps elsewhere. The ordinary
//! machinery applies from there: `fr translate <module> fastapi` writes one handler per
//! export, and `fr openapi` puts each on its own path.

use fun_refactor::transpile::nextjs;
use std::path::PathBuf;

const ACTIONS: &str = "\
\"use server\";

interface Pet {
  name: string;
  age: number;
}

export async function createPet(pet: Pet): Promise<Pet> {
  const saved = await db.insert(pet);
  return saved;
}

export async function deletePet(petId: number): Promise<boolean> {
  await db.remove(petId);
  return true;
}

function helper(x: number): number {
  return x * 2;
}
";

fn module(source: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join("actions.ts");
    std::fs::write(&path, source).expect("write");
    (tmp, path)
}

#[test]
fn each_exported_function_is_an_endpoint_reached_by_its_name() {
    let (_tmp, path) = module(ACTIONS);
    let plan = nextjs::plan(&path).expect("a plan");
    assert_eq!(
        plan.endpoints,
        vec![
            ("POST".to_string(), "/create-pet".to_string()),
            ("POST".to_string(), "/delete-pet".to_string()),
        ],
        "one endpoint per export, kebab-cased. The helper is not among them"
    );
}

#[test]
fn a_server_function_becomes_a_fastapi_handler() {
    let (_tmp, path) = module(ACTIONS);
    let plan = nextjs::plan(&path).expect("a plan");
    assert!(
        plan.output
            .contains("@router.post(\"/create-pet\")\nasync def create_pet(pet: Pet):"),
        "{}",
        plan.output
    );
    assert!(
        plan.output
            .contains("@router.post(\"/delete-pet\")\nasync def delete_pet(pet_id: float):"),
        "the arguments are the function's own parameters: {}",
        plan.output
    );
}

#[test]
fn the_directive_does_not_survive_as_a_statement() {
    // `"use server"` is what made the file a server module: a fact about the source's
    // framework, already spent. Carried, it landed in the output as a string that does
    // nothing, wrapped in the writer's `__main__` block.
    let (_tmp, path) = module(ACTIONS);
    let plan = nextjs::plan(&path).expect("a plan");
    assert!(!plan.output.contains("use server"), "{}", plan.output);
}

#[test]
fn a_helper_is_not_an_endpoint_but_still_crosses() {
    let (_tmp, path) = module(ACTIONS);
    let plan = nextjs::plan(&path).expect("a plan");
    assert!(
        plan.output.contains("def _helper(x: float) -> float:"),
        "unexported, so it keeps Python's leading underscore: {}",
        plan.output
    );
    assert!(
        !plan.output.contains("@router.post(\"/helper\")"),
        "{}",
        plan.output
    );
}

#[test]
fn a_module_with_the_directive_and_no_exports_is_refused() {
    let (_tmp, path) =
        module("\"use server\";\n\nfunction helper(x: number): number {\n  return x;\n}\n");
    let refusal = nextjs::plan(&path).expect_err("nothing to reach");
    assert!(
        refusal.to_string().contains("exports no async function"),
        "{refusal}"
    );
}

#[test]
fn the_words_in_a_comment_or_a_string_do_not_count() {
    for source in [
        "// \"use server\" one day\nexport function f(): number { return 1; }\n",
        "const s = \"use server\";\nexport function f(): number { return 1; }\n",
    ] {
        let (_tmp, path) = module(source);
        let refusal = nextjs::plan(&path).expect_err("not a server module, not a route");
        assert!(
            refusal.to_string().contains("neither"),
            "mentioning the directive is not declaring it: {refusal}"
        );
    }
}

#[test]
fn the_contract_puts_each_function_on_its_own_path() {
    let (tmp, path) = module(ACTIONS);
    let baseline =
        fun_refactor::openapi::from_routes("actions", tmp.path(), &[path]).expect("a baseline");
    let paths = baseline
        .document
        .get("paths")
        .and_then(|p| p.as_object())
        .expect("a paths object");
    assert!(paths.contains_key("/create-pet"), "{paths:?}");
    assert!(paths.contains_key("/delete-pet"), "{paths:?}");
    assert!(
        paths["/create-pet"].get("post").is_some(),
        "a server function answers POST: {paths:?}"
    );
}

#[test]
fn the_contract_names_the_payloads_it_cannot_describe() {
    // A server function's arguments travel in the framework's own wire encoding. A JSON
    // body written here would be a guess presented as a contract.
    let (tmp, path) = module(ACTIONS);
    let baseline =
        fun_refactor::openapi::from_routes("actions", tmp.path(), &[path]).expect("a baseline");
    assert!(
        baseline
            .notes
            .iter()
            .any(|note| note.contains("framework's own")),
        "{:?}",
        baseline.notes
    );
}

#[test]
fn a_route_file_still_translates_exactly_as_before() {
    // The endpoint machinery was generalised under the route path. Its output for a
    // route file is pinned elsewhere; this holds the seam: a route's endpoints all
    // share the file's URL.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let dir = tmp.path().join("app/api/pets");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("route.ts");
    std::fs::write(
        &path,
        "export async function GET(): Promise<Response> {\n  return Response.json([]);\n}\n\n\
         export async function POST(): Promise<Response> {\n  return Response.json({});\n}\n",
    )
    .expect("write");
    let plan = nextjs::plan(&path).expect("a plan");
    assert_eq!(
        plan.endpoints,
        vec![
            ("GET".to_string(), "/pets".to_string()),
            ("POST".to_string(), "/pets".to_string()),
        ]
    );
}
