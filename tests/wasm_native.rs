//! The browser API, driven by `cargo test`.

#![cfg(feature = "wasm")]

use fun_refactor::wasm::Workspace;
use std::collections::BTreeMap;

fn workspace(files: &[(&str, &str)]) -> Workspace {
    let map: BTreeMap<String, String> = files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    Workspace::load(map).expect("the workspace loads")
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not JSON: {e}\n{text}"))
}

const ROUTE: &str = r#"import { NextResponse } from "next/server";

export async function GET(request: Request, context: { params: { id: string } }) {
  const user = await db.users.find(context.params.id);
  return NextResponse.json(user);
}
"#;

#[test]
fn a_workspace_loads_and_reports_what_it_read() {
    let ws = workspace(&[
        (
            "a.py",
            "def add(x: int, y: int) -> int:\n    return x + y\n",
        ),
        (
            "b.ts",
            "export function twice(n: number): number {\n  return n * 2;\n}\n",
        ),
    ]);
    let files = json(&ws.files());
    assert_eq!(files.as_array().unwrap().len(), 2);
    assert!(files[0]["indexed"].as_bool().unwrap());
}

#[test]
fn two_workspaces_do_not_read_each_others_bytes() {
    // The reason `files` is a handle and not one global map.
    let first = workspace(&[("a.py", "def only_in_first() -> int:\n    return 1\n")]);
    let second = workspace(&[("a.py", "def only_in_second() -> int:\n    return 2\n")]);

    assert!(
        first.symbols("a.py").contains("only_in_first"),
        "{}",
        first.symbols("a.py")
    );
    assert!(
        !first.symbols("a.py").contains("only_in_second"),
        "the second workspace overwrote the first:\n{}",
        first.symbols("a.py")
    );
    assert!(second.symbols("a.py").contains("only_in_second"));
}

#[test]
fn the_translation_menu_answers_for_every_language() {
    // This is the call whose result shape broke: somebody added a field to the option struct and
    // missed at one of six literals.
    let ws = workspace(&[(
        "a.py",
        "def add(x: int, y: int) -> int:\n    return x + y\n",
    )]);
    let options = json(&ws.translations("a.py"));
    let options = options.as_array().expect("an array of options");

    assert!(
        options.len() > 3,
        "expected every language, got {}",
        options.len()
    );
    for option in options {
        // Every option says what it is, and the three kinds are distinguishable,
        // which is the whole point of the field that went missing.
        assert!(option["language"].is_string(), "{option}");
        assert!(option["framework"].is_boolean(), "{option}");
        assert!(
            option["unavailable"].is_string() || option["destination"].is_string(),
            "an option must either be offered or say why not: {option}"
        );
    }
    assert!(
        options
            .iter()
            .any(|o| o["language"] == "typescript" && o["draft"].is_string()),
        "python should be translatable into typescript, as a draft"
    );
}

#[test]
fn a_next_js_route_is_offered_as_a_framework_port() {
    // `fastapi` is not a language, and the menu has to say so.
    let ws = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let options = json(&ws.translations("app/api/users/[id]/route.ts"));
    let fastapi = options
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["language"] == "fastapi")
        .expect("a route offers fastapi");

    assert_eq!(fastapi["framework"], true);
    assert!(fastapi["destination"].is_string(), "{fastapi}");
    assert!(
        fastapi["draft"].as_str().unwrap().contains("/users/{id}"),
        "the menu has to show the route before it is chosen: {fastapi}"
    );
}

#[test]
fn a_file_that_is_not_a_route_says_why_fastapi_is_not_on_offer() {
    let ws = workspace(&[("lib/users.ts", ROUTE)]);
    let options = json(&ws.translations("lib/users.ts"));
    let fastapi = options
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["language"] == "fastapi");
    // Not a route at all, so it is not in the list; the refusal belongs to files that
    // are routes and still cross into nothing.
    assert!(fastapi.is_none(), "{options}");
}

#[test]
fn translating_writes_the_new_file_into_the_workspace() {
    let mut ws = workspace(&[("app/api/users/[id]/route.ts", ROUTE)]);
    let applied = json(&ws.translate("app/api/users/[id]/route.ts", "fastapi"));

    assert!(applied["error"].is_null(), "{applied}");
    let written = applied["files"][0]["path"]
        .as_str()
        .expect("a file was written");
    assert!(written.ends_with("users_id.py"), "{applied}");
    assert!(
        applied["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("/users/{id}")),
        "the route has to travel with the result: {applied}"
    );

    // The new file is in the workspace and indexed.
    let files = json(&ws.files());
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == written),
        "a file a refactoring created has to join the workspace:\n{files}"
    );
}

#[test]
fn translating_into_something_impossible_fails_without_writing() {
    let mut ws = workspace(&[(
        "a.py",
        "def add(x: int, y: int) -> int:\n    return x + y\n",
    )]);
    let before = json(&ws.files()).as_array().unwrap().len();

    let applied = json(&ws.translate("a.py", "yaml"));
    assert!(applied["error"].is_string(), "{applied}");

    assert_eq!(
        json(&ws.files()).as_array().unwrap().len(),
        before,
        "a refused translation must not leave a file behind"
    );
}

#[test]
fn an_unknown_target_is_refused_by_name() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    let applied = json(&ws.translate("a.py", "cobol"));
    assert!(
        applied["error"].as_str().unwrap().contains("cobol"),
        "{applied}"
    );
}

#[test]
fn the_capability_matrix_is_answerable() {
    let ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    let matrix = json(&ws.capabilities());
    assert!(!matrix.as_array().unwrap().is_empty());
}
