//! What URLs a service serves, whichever framework it was written with.
//!
//! `fr openapi` builds a contract from a tree of route files, and it could read
//! one shape of tree: a Next.js `app/api` directory. Express, Flask, axum, gin
//! and Spring declare the same thing, and the command had nothing to say about
//! any of them.
//!
//! Each test below asserts three things. The methods found, the URLs with the
//! framework's own path-parameter spelling turned into a contract's, and the
//! handler that answers. And each asserts what is *not* found, because a reader
//! that calls `map.get(k)` a route is worse than one that reads nothing.

use fun_refactor::lang::Language;
use fun_refactor::transpile::routes::{self, Framework};

fn found(source: &str, language: Language) -> Vec<(String, String, Option<String>)> {
    let (_, endpoints) = routes::endpoints_of(source, language).expect("routes");
    endpoints
        .into_iter()
        .map(|e| (e.method, e.url, e.handler))
        .collect()
}

fn framework_of(source: &str, language: Language) -> Framework {
    routes::endpoints_of(source, language).expect("routes").0
}

#[test]
fn express_declares_its_routes_on_a_router() {
    let source = "\
import express from \"express\";

const app = express();

app.get(\"/pets\", listPets);
app.post(\"/pets\", createPet);
app.get(\"/pets/:petId\", showPet);
";
    assert_eq!(framework_of(source, Language::TypeScript), Framework::Express);
    assert_eq!(
        found(source, Language::TypeScript),
        vec![
            ("GET".into(), "/pets".into(), Some("listPets".into())),
            ("POST".into(), "/pets".into(), Some("createPet".into())),
            (
                "GET".into(),
                "/pets/{petId}".into(),
                Some("showPet".into())
            ),
        ]
    );
}

#[test]
fn a_method_on_something_that_is_not_a_router_is_not_a_route() {
    // `.get` is the commonest method name there is. A reader that took every
    // one of them would report a contract full of endpoints nobody serves.
    let source = "\
const cache = new Map();

export function lookup(k: string): string {
  return cache.get(k);
}
";
    assert!(routes::endpoints_of(source, Language::TypeScript).is_none());
}

#[test]
fn flask_declares_its_routes_in_a_decorator() {
    let source = "\
from flask import Flask

app = Flask(__name__)


@app.route(\"/pets\", methods=[\"GET\", \"POST\"])
def pets():
    return \"\"


@app.get(\"/pets/<int:pet_id>\")
def show_pet(pet_id):
    return \"\"


@app.route(\"/health\")
def health():
    return \"\"
";
    assert_eq!(framework_of(source, Language::Python), Framework::Flask);
    assert_eq!(
        found(source, Language::Python),
        vec![
            ("GET".into(), "/pets".into(), Some("pets".into())),
            ("POST".into(), "/pets".into(), Some("pets".into())),
            (
                "GET".into(),
                "/pets/{pet_id}".into(),
                Some("show_pet".into())
            ),
            // A `@app.route` naming no methods answers `GET`, which is what
            // Flask itself does with one.
            ("GET".into(), "/health".into(), Some("health".into())),
        ]
    );
}

#[test]
fn axum_builds_a_router_by_chaining() {
    let source = "\
use axum::routing::{get, post};
use axum::Router;

pub fn router() -> Router {
    Router::new()
        .route(\"/pets\", get(list_pets).post(create_pet))
        .route(\"/pets/:pet_id\", get(show_pet))
}
";
    assert_eq!(framework_of(source, Language::Rust), Framework::Axum);
    let endpoints = found(source, Language::Rust);
    assert!(
        endpoints.contains(&("GET".into(), "/pets".into(), Some("list_pets".into()))),
        "{endpoints:?}"
    );
    assert!(
        endpoints.contains(&("POST".into(), "/pets".into(), Some("create_pet".into()))),
        "{endpoints:?}"
    );
    assert!(
        endpoints.contains(&(
            "GET".into(),
            "/pets/{pet_id}".into(),
            Some("show_pet".into())
        )),
        "{endpoints:?}"
    );
}

#[test]
fn gin_names_its_methods_in_capitals() {
    let source = "\
package main

import \"github.com/gin-gonic/gin\"

func routes(r *gin.Engine) {
\tr.GET(\"/pets\", listPets)
\tr.POST(\"/pets\", createPet)
\tr.DELETE(\"/pets/:petId\", deletePet)
}
";
    assert_eq!(framework_of(source, Language::Go), Framework::Gin);
    assert_eq!(
        found(source, Language::Go),
        vec![
            ("GET".into(), "/pets".into(), Some("listPets".into())),
            ("POST".into(), "/pets".into(), Some("createPet".into())),
            (
                "DELETE".into(),
                "/pets/{petId}".into(),
                Some("deletePet".into())
            ),
        ]
    );
}

#[test]
fn a_go_method_in_mixed_case_is_not_a_route() {
    // gin spells the method in capitals, which tells one of its calls from
    // every other method on a receiver.
    let source = "\
package main

import \"strings\"

func shout(s string) string {
\treturn strings.ToUpper(s)
}
";
    assert!(routes::endpoints_of(source, Language::Go).is_none());
}

#[test]
fn spring_annotates_its_handlers() {
    let source = "\
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping(\"/pets\")
public class PetController {
    @GetMapping
    public String list() {
        return \"\";
    }

    @GetMapping(\"/{petId}\")
    public String show(@PathVariable String petId) {
        return \"\";
    }

    @PostMapping
    public String create() {
        return \"\";
    }
}
";
    assert_eq!(framework_of(source, Language::Java), Framework::Spring);
    let endpoints = found(source, Language::Java);
    // The class-level mapping stands in front of every method under it.
    assert!(
        endpoints.contains(&("GET".into(), "/pets".into(), Some("list".into()))),
        "{endpoints:?}"
    );
    assert!(
        endpoints.contains(&("GET".into(), "/pets/{petId}".into(), Some("show".into()))),
        "{endpoints:?}"
    );
    assert!(
        endpoints.contains(&("POST".into(), "/pets".into(), Some("create".into()))),
        "{endpoints:?}"
    );
}

#[test]
fn a_path_parameter_is_spelled_the_way_a_contract_spells_one() {
    // Express, gin and axum write `:id`. Flask writes `<int:id>`, where the
    // part before the colon is a converter and not the name. A contract writes
    // `{id}`, and that is the one every reader produces.
    let express = found(
        "const app = express();\napp.get(\"/a/:one/b/:two\", h);\n",
        Language::TypeScript,
    );
    assert_eq!(express[0].1, "/a/{one}/b/{two}");

    let flask = found(
        "@app.get(\"/a/<int:one>/b/<two>\")\ndef h(one, two):\n    return \"\"\n",
        Language::Python,
    );
    assert_eq!(flask[0].1, "/a/{one}/b/{two}");
}

// ------------------------------------------------------ into the contract

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

#[test]
fn a_contract_is_built_from_a_service_that_is_not_next_js() {
    // The whole point of reading these. `fr openapi` could describe one shape
    // of tree, and a service written with any of the five was invisible.
    let (_tmp, root) = workspace(&[
        (
            "api.py",
            "from flask import Flask\n\napp = Flask(__name__)\n\n\n\
             @app.route(\"/pets\", methods=[\"GET\", \"POST\"])\n\
             def pets():\n    return \"\"\n",
        ),
        (
            "orders.go",
            "package main\n\nimport \"github.com/gin-gonic/gin\"\n\n\
             func routes(r *gin.Engine) {\n\tr.GET(\"/orders/:orderId\", showOrder)\n}\n",
        ),
        ("helper.py", "def helper() -> int:\n    return 1\n"),
    ]);
    let files: Vec<std::path::PathBuf> = vec![
        root.join("api.py"),
        root.join("orders.go"),
        root.join("helper.py"),
    ];
    let baseline = fun_refactor::openapi::from_routes("demo", &root, &files).unwrap();
    let paths = baseline.document["paths"].as_object().unwrap();

    assert!(paths["/pets"]["get"].is_object(), "{paths:?}");
    assert!(paths["/pets"]["post"].is_object(), "{paths:?}");
    assert!(paths["/orders/{orderId}"]["get"].is_object(), "{paths:?}");

    // The path parameter is in the contract, taken from the URL and not guessed.
    let parameters = paths["/orders/{orderId}"]["get"]["parameters"]
        .as_array()
        .unwrap();
    assert_eq!(parameters[0]["name"], "orderId");
    assert_eq!(parameters[0]["in"], "path");

    // A file that declares no routes is not a route file.
    assert_eq!(baseline.routes.len(), 2, "{:?}", baseline.routes);

    // What the document does not settle is said out loud. None of the five
    // declares its request body where the route is declared.
    assert!(
        baseline.notes.iter().any(|n| n.contains("flask")),
        "{:?}",
        baseline.notes
    );
    assert!(
        baseline.notes.iter().any(|n| n.contains("gin")),
        "{:?}",
        baseline.notes
    );

    // And the document says which trees it was read from.
    let described = baseline.document["info"]["description"].as_str().unwrap();
    assert!(described.contains("flask"), "{described}");
    assert!(described.contains("gin"), "{described}");
}
