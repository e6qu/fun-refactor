//! Next.js API routes as FastAPI modules.
//!
//! The part worth testing hardest is the part no content-only translation could do:
//! the URL comes from where the file sits on disk, not from anything inside it.

use fun_refactor::transpile::nextjs;
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

const ROUTE: &str = r#"import { NextResponse } from "next/server";

export interface User {
  id: string;
  name: string;
  email?: string;
}

/** Fetch one user. */
export async function GET(request: Request, context: { params: { id: string } }) {
  const id = context.params.id;
  const user = await db.users.find(id);
  return NextResponse.json(user);
}

/** Replace a user. */
export async function PUT(request: Request) {
  const body = await request.json();
  return NextResponse.json(body);
}
"#;

#[test]
fn the_url_comes_from_the_path_not_the_file() {
    // Nothing inside a Next.js route says what it serves. Four spellings, four rules.
    assert_eq!(
        nextjs::route_for(Path::new("app/api/users/route.ts")),
        "/users"
    );
    assert_eq!(
        nextjs::route_for(Path::new("app/api/users/[id]/route.ts")),
        "/users/{id}"
    );
    assert_eq!(
        nextjs::route_for(Path::new("app/api/files/[...path]/route.ts")),
        "/files/{path:path}"
    );
    assert_eq!(
        nextjs::route_for(Path::new("pages/api/health.ts")),
        "/health"
    );
}

#[test]
fn a_route_becomes_a_router_with_its_methods() {
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).expect("a route translates");

    assert_eq!(plan.route, "/users/{id}");
    assert_eq!(plan.methods, vec!["GET", "PUT"]);
    assert!(plan.output.contains("@router.get(\"/users/{id}\")"));
    assert!(plan.output.contains("@router.put(\"/users/{id}\")"));
    // The path parameter is a typed argument, which is the whole point of FastAPI.
    assert!(plan
        .output
        .contains("async def get(id: str, request: Request):"));
    // An exported interface is a validated model. It is not a bare class.
    assert!(plan.output.contains("class User(BaseModel):"));
    assert!(plan.output.contains("from fastapi import APIRouter"));
    // Docs survive the crossing.
    assert!(plan.output.contains("\"\"\"Fetch one user.\"\"\""));
}

#[test]
fn pulling_a_path_parameter_off_the_context_is_dropped_not_carried() {
    // `const id = context.params.id` is the commonest line in a Next.js route and is
    // exactly the work FastAPI already did. Carrying it would open every translated
    // handler with a line naming an object Python does not have.
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();

    assert!(
        !plan.output.contains("context"),
        "the context object should not survive into the Python:\n{}",
        plan.output
    );
    assert!(
        plan.fidelity
            .notes
            .iter()
            .any(|n| n.contains("FastAPI supplies it")),
        "the drop has to be reported, not silent: {:?}",
        plan.fidelity.notes
    );
}

#[test]
fn the_request_object_is_kept_because_fastapi_has_one() {
    // `NextRequest` and Starlette's `Request` are the same thing: same headers, same
    // `await .json()`. Dropping it and commenting out every line that read it was the
    // wrong call, the correspondence is exact, so the parameter is kept and typed.
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();

    assert!(
        plan.output
            .contains("async def put(id: str, request: Request):"),
        "{}",
        plan.output
    );
    assert!(plan
        .output
        .contains("from fastapi import APIRouter, Request"));
    assert!(
        plan.output.contains("body = await request.json()"),
        "`await` means the same thing in both languages:\n{}",
        plan.output
    );
}

#[test]
fn a_handler_that_never_takes_a_request_does_not_import_one() {
    // An unused import in generated code is one more thing the reader has to decide
    // about.
    let (_tmp, root) = workspace(&[(
        "app/api/health/route.ts",
        "export async function GET() {\n  return null;\n}\n",
    )]);
    let plan = nextjs::plan(&root.join("app/api/health/route.ts")).unwrap();
    assert!(
        plan.output.contains("from fastapi import APIRouter\n"),
        "{}",
        plan.output
    );
    assert!(!plan.output.contains("Request"), "{}", plan.output);
}

#[test]
fn the_next_response_helpers_become_their_fastapi_equivalents() {
    // Not approximations: returning a value from a FastAPI handler *is* what
    // `NextResponse.json` does, and `JSONResponse` is how a status is spelled. The
    // nested case matters most, an error return inside an `if` is the commonest
    // branch in a route, and rewriting only the top level missed exactly those.
    let (_tmp, root) = workspace(&[(
        "app/api/posts/[id]/route.ts",
        r#"import { NextResponse, NextRequest } from "next/server";

export async function GET(request: NextRequest, context: { params: { id: string } }) {
  const post = await db.posts.find(context.params.id);
  if (post == null) {
    return NextResponse.json({ error: "not found" }, { status: 404 });
  }
  return NextResponse.json(post);
}
"#,
    )]);
    let plan = nextjs::plan(&root.join("app/api/posts/[id]/route.ts")).unwrap();

    assert!(plan.output.contains("return post"), "{}", plan.output);
    assert!(
        plan.output
            .contains(r#"return JSONResponse({"error": "not found"}, 404)"#),
        "a nested error return has to be rewritten too:\n{}",
        plan.output
    );
    assert!(plan
        .output
        .contains("from fastapi.responses import JSONResponse"));
}

#[test]
fn the_frameworks_own_imports_are_not_listed_as_work_to_do() {
    // `import { NextResponse } from "next/server"` is the one import whose uses this
    // translated away. Listing it under "yours to add" points at a job already done.
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();
    assert!(!plan.output.contains("next/server"), "{}", plan.output);
}

#[test]
fn the_banner_says_draft_only_when_it_is_one() {
    // A file that carried nothing is not a draft. A banner that says SKELETON over a complete
    // translation is how a banner stops being read.
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let clean = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();
    assert_eq!(clean.fidelity.carried_verbatim, 0);
    assert!(!clean.output.contains("DRAFT"), "{}", clean.output);

    let (_tmp2, root2) = workspace(&[(
        "app/api/odd/route.ts",
        "export async function GET() {\n  for (let i = 0; i < 3; i++) { work(i); }\n  return null;\n}\n",
    )]);
    let draft = nextjs::plan(&root2.join("app/api/odd/route.ts")).unwrap();
    assert!(draft.fidelity.carried_verbatim > 0);
    assert!(
        draft.output.contains("THIS FILE IS A DRAFT"),
        "{}",
        draft.output
    );
    assert!(
        draft.output.contains("for (let i = 0; i < 3; i++)"),
        "what could not be translated has to be in the output verbatim:\n{}",
        draft.output
    );
}

#[test]
fn the_output_parses_as_python() {
    // The same self-check the general translator runs: an unparseable result is a
    // defect here, and `plan` is what has to notice.
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();

    let parsers = fun_refactor::parse::Parsers::new();
    let parsed = parsers
        .parse(fun_refactor::lang::Language::Python, &plan.output)
        .unwrap();
    assert!(!parsed.has_errors(), "not Python:\n{}", plan.output);
}

#[test]
fn a_react_component_is_refused_with_the_reason() {
    // A component renders UI and an endpoint answers HTTP. A file that pretended
    // otherwise would be worse than no file.
    let (_tmp, root) = workspace(&[(
        "app/api/widget/route.tsx",
        "export default function Widget() {\n  return <div>hi</div>;\n}\n",
    )]);
    let error =
        nextjs::plan(&root.join("app/api/widget/route.tsx")).expect_err("JSX is not a route");
    let message = error.to_string();
    assert!(
        message.contains("JSX") && message.contains("component"),
        "the refusal must name the reason: {message}"
    );
}

#[test]
fn a_file_outside_the_api_directories_is_refused() {
    // The URL is the path. A file that is not at a routable path has no URL, and
    // inventing one would be a guess.
    let (_tmp, root) = workspace(&[("lib/users.ts", ROUTE)]);
    let error = nextjs::plan(&root.join("lib/users.ts")).expect_err("not a route");
    assert!(
        error.to_string().contains("not a Next.js API route"),
        "{error}"
    );
    assert!(!nextjs::is_api_route(Path::new("lib/users.ts")));
    assert!(nextjs::is_api_route(Path::new("app/api/users/route.ts")));
    assert!(nextjs::is_api_route(Path::new("pages/api/users.ts")));
}

#[test]
fn a_route_that_exports_no_method_is_refused() {
    let (_tmp, root) = workspace(&[(
        "app/api/users/route.ts",
        "export function helper(x: string): string {\n  return x;\n}\n",
    )]);
    let error = nextjs::plan(&root.join("app/api/users/route.ts")).expect_err("no methods");
    assert!(
        error.to_string().contains("exports no HTTP method"),
        "{error}"
    );
}

#[test]
fn a_catch_all_segment_becomes_a_path_converter() {
    // `{path}` matches one segment and `{path:path}` matches the rest. A catch-all
    // route written as the former would quietly serve the wrong URLs.
    let (_tmp, root) = workspace(&[(
        "app/api/files/[...path]/route.ts",
        "export async function GET(request: Request) {\n  return null;\n}\n",
    )]);
    let plan = nextjs::plan(&root.join("app/api/files/[...path]/route.ts")).unwrap();
    assert_eq!(plan.route, "/files/{path:path}");
    assert!(
        plan.output
            .contains("async def get(path: str, request: Request):"),
        "{}",
        plan.output
    );
}

#[test]
fn the_destination_is_named_for_the_route() {
    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/users/[id]/route.ts")).unwrap();
    assert_eq!(plan.destination.file_name().unwrap(), "users_id.py");
    assert_eq!(plan.edits.file_count(), 1);
}

// ------------------------------------------------------------ zod and OpenAPI

const ZOD_ROUTE: &str = r#"import { NextResponse } from "next/server";
import * as z from "zod";

const postCreateSchema = z.object({
  title: z.string().min(3).max(128),
  content: z.string().optional(),
  views: z.number().int(),
  tags: z.array(z.string()),
  draft: z.boolean(),
  publishedAt: z.date().nullable(),
});

export async function POST(request: Request) {
  const body = await request.json();
  return NextResponse.json(body, { status: 201 });
}
"#;

#[test]
fn a_zod_schema_becomes_a_pydantic_model() {
    // Most Next.js applications declare their shapes with zod, and a zod schema is a runtime
    // value instead of a type declaration. So nothing that reads declarations finds it. Left
    // alone the translated service publishes a contract with no request body in it. The
    // endpoint works and the contract is smaller than the one it replaced.
    let (_tmp, root) = workspace(&[("app/api/posts/route.ts", ZOD_ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/posts/route.ts")).unwrap();

    assert!(
        plan.output.contains("class PostCreate(BaseModel):"),
        "{}",
        plan.output
    );
    for (field, ty) in [
        ("title", "str"),
        ("content", "str | None"),
        ("views", "int"),
        ("tags", "list[str]"),
        ("draft", "bool"),
        // The name takes Python's convention, like every other name the file declares.
        ("published_at", "datetime | None"),
    ] {
        assert!(
            plan.output.contains(&format!("{field}: {ty}")),
            "expected `{field}: {ty}`:\n{}",
            plan.output
        );
    }
    // A name the output uses is a name the output has to import.
    assert!(
        plan.output.contains("from datetime import datetime"),
        "{}",
        plan.output
    );
}

#[test]
fn a_zod_constraint_is_not_invented_into_a_type() {
    // `.min(3).max(128)` is validation, and Pydantic spells it with `Field(...)`. Guessing one
    // from a zod call is a guess about the part of a contract it is least safe to guess at, so
    // the constraint is dropped and not mistranslated.
    let (_tmp, root) = workspace(&[("app/api/posts/route.ts", ZOD_ROUTE)]);
    let plan = nextjs::plan(&root.join("app/api/posts/route.ts")).unwrap();
    assert!(!plan.output.contains("min_length"), "{}", plan.output);
    assert!(!plan.output.contains("128"), "{}", plan.output);
}

#[test]
fn the_openapi_baseline_states_what_the_tree_declares() {
    let (_tmp, root) = workspace(&[
        ("app/api/posts/route.ts", ZOD_ROUTE),
        (
            "app/api/posts/[postId]/route.ts",
            "export async function DELETE(request: Request) {\n  return null;\n}\n",
        ),
        (
            "lib/helper.ts",
            "export function helper(): number {\n  return 1;\n}\n",
        ),
    ]);
    let files: Vec<std::path::PathBuf> = vec![
        root.join("app/api/posts/route.ts"),
        root.join("app/api/posts/[postId]/route.ts"),
        root.join("lib/helper.ts"),
    ];
    let baseline = fun_refactor::openapi::from_routes("demo", &root, &files).unwrap();
    let document = &baseline.document;

    // Only the routes. A library file is not an endpoint.
    assert_eq!(baseline.routes.len(), 2, "{:?}", baseline.routes);

    let paths = document["paths"].as_object().unwrap();
    assert!(paths.contains_key("/posts"));
    assert!(paths["/posts"]["post"].is_object());

    // The path parameter comes from the tree, which is the whole trick.
    let parameters = paths["/posts/{post_id}"]["delete"]["parameters"]
        .as_array()
        .unwrap();
    assert_eq!(parameters[0]["name"], "post_id");
    assert_eq!(parameters[0]["in"], "path");
    assert_eq!(parameters[0]["required"], true);

    // The zod schema is a component.
    let schema = &document["components"]["schemas"]["PostCreate"];
    assert_eq!(schema["properties"]["views"]["type"], "integer");
    assert_eq!(schema["properties"]["tags"]["type"], "array");
    assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
    // Optional says "may be absent", which OpenAPI spells by leaving it out of
    // `required` and not by changing the type.
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(required.contains(&"title"));
    assert!(!required.contains(&"content"));
}

#[test]
fn the_baseline_never_invents_a_response() {
    // Which status an endpoint returns is a fact about its code. It is not its declaration.
    // Writing `200` for everything would be putting fiction into the file you are about
    // to diff against, which is worse than an empty one.
    let (_tmp, root) = workspace(&[("app/api/posts/route.ts", ZOD_ROUTE)]);
    let files = vec![root.join("app/api/posts/route.ts")];
    let baseline = fun_refactor::openapi::from_routes("demo", &root, &files).unwrap();

    let responses = &baseline.document["paths"]["/posts"]["post"]["responses"];
    assert!(responses["default"].is_object());
    assert!(responses["200"].is_null(), "{responses}");
    assert!(responses["201"].is_null(), "{responses}");

    // And what it could not settle is said. It is not hidden.
    assert!(
        baseline.notes.iter().any(|n| n.contains("returns status")),
        "{:?}",
        baseline.notes
    );
}

/// The tool's own output, read back by the tool.
///
/// A translated route is a FastAPI module whose handlers the framework calls and the
/// source never does. Before the catalogue knew what `@router.get` meant, every handler
/// this command emitted came back from `fr unused` as having no detected use, output
/// that was not valid input, with nothing checking that the two ends agreed.
#[test]
fn every_handler_this_command_emits_is_an_entry_point() {
    use fun_refactor::analysis::entrypoints::{Catalog, EntryKind};
    use fun_refactor::index::Index;
    use fun_refactor::scan::ScanOptions;

    let (_tmp, root) = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let source = root.join("app/api/users/[id]/route.ts");
    let plan = nextjs::plan(&source).expect("a route translates");

    // Only the emitted module: the TypeScript it came from would answer for itself.
    let out = workspace(&[]);
    std::fs::write(out.1.join("users_id.py"), &plan.output).expect("the translation");

    let index = Index::build(&out.1, &ScanOptions::default()).expect("an index");
    let routes: Vec<_> = Catalog::builtin()
        .expect("the built-in catalogs")
        .detect(&index)
        .into_iter()
        .filter(|e| e.kind == EntryKind::HttpRoute)
        .filter_map(|e| index.symbol(e.symbol).map(|s| s.name.clone()))
        .collect();

    for method in &plan.methods {
        let name = method.to_lowercase();
        assert!(
            routes.contains(&name),
            "this command emitted `{name}` under a route decorator and the catalogue \
             does not see it as an entry point, so `fr unused` calls it dead:\n{}",
            plan.output
        );
    }
}
