//! Entry points a framework calls and the source never does.
//!
//! Python's are not spelled the way the other five spell theirs, and Next.js's newest
//! convention is not spelled the way its older ones are. Both are the same problem: a
//! name rule cannot find something that has no name to match.
//!
//! Every other catalog can say `name: main`, because every other language here agrees
//! that a program starts in a function called `main`. Python's starts in a *statement*,
//! and the function it calls can be named anything — so the rule that worked everywhere
//! else reported nothing at all for an ordinary script.

use fun_refactor::analysis::entrypoints::{Catalog, EntryKind};
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn entry_kinds(files: &[(&str, &str)]) -> Vec<(String, EntryKind)> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let catalog = Catalog::builtin().expect("the built-in catalogs");
    catalog
        .detect(&index)
        .into_iter()
        .filter_map(|e| index.symbol(e.symbol).map(|s| (s.name.clone(), e.kind)))
        .collect()
}

#[test]
fn the_main_guard_names_the_entry_point() {
    let found = entry_kinds(&[(
        "run.py",
        "def cli():\n    return 1\n\nif __name__ == \"__main__\":\n    cli()\n",
    )]);
    assert!(
        found.contains(&("cli".to_string(), EntryKind::CliMain)),
        "got {found:?}"
    );
}

#[test]
fn the_main_guard_reaches_through_sys_exit() {
    // `sys.exit(run())` is the other half of the idiom, and the call it wraps is still
    // where the program starts.
    let found = entry_kinds(&[(
        "run.py",
        "import sys\n\ndef run():\n    return 0\n\nif __name__ == \"__main__\":\n    \
         sys.exit(run())\n",
    )]);
    assert!(
        found.contains(&("run".to_string(), EntryKind::CliMain)),
        "got {found:?}"
    );
}

#[test]
fn a_function_the_guard_does_not_call_is_not_an_entry_point() {
    // Only what the guard calls directly. Anything further is reachability, which the
    // call graph answers; folding it in here would tag half a program.
    let found = entry_kinds(&[(
        "run.py",
        "def helper():\n    return 1\n\ndef cli():\n    return helper()\n\n\
         if __name__ == \"__main__\":\n    cli()\n",
    )]);
    assert!(
        !found.iter().any(|(name, _)| name == "helper"),
        "got {found:?}"
    );
}

#[test]
fn a_module_with_no_guard_gains_nothing() {
    let found = entry_kinds(&[("lib.py", "def cli():\n    return 1\n")]);
    assert!(found.is_empty(), "got {found:?}");
}

#[test]
fn a_shared_fixture_is_an_entry_point() {
    // Nothing calls a fixture by name — pytest injects it by matching the parameter.
    // In `conftest.py`, where the shared ones live, neither the file nor the function
    // is named `test_*`, so no rule matched it at all.
    let found = entry_kinds(&[(
        "conftest.py",
        "import pytest\n\n@pytest.fixture\ndef shared():\n    return 3\n",
    )]);
    assert!(
        found.contains(&("shared".to_string(), EntryKind::Test)),
        "got {found:?}"
    );
}

#[test]
fn a_parameterised_fixture_is_an_entry_point() {
    let found = entry_kinds(&[(
        "conftest.py",
        "import pytest\n\n@pytest.fixture(scope=\"module\")\ndef db():\n    return 2\n",
    )]);
    assert!(
        found.contains(&("db".to_string(), EntryKind::Test)),
        "got {found:?}"
    );
}

#[test]
fn unittest_fixtures_are_entry_points() {
    // `unittest` calls these itself, once per test, and no source refers to them.
    let found = entry_kinds(&[(
        "tc.py",
        "import unittest\n\nclass ThingTest(unittest.TestCase):\n    \
         def setUp(self):\n        self.value = 1\n\n    \
         def tearDown(self):\n        self.value = 0\n",
    )]);
    for name in ["setUp", "tearDown"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::Test)),
            "{name} missing from {found:?}"
        );
    }
}

// ------------------------------------------------ and the same case in TypeScript

#[test]
fn a_next_js_server_action_is_an_entry_point() {
    // A `"use server"` file exports functions the framework makes reachable over the
    // network, called by nothing in the source. The catalogue already covered Next.js's
    // *filename* conventions — `page.tsx`, `route.ts` — and this one is not a filename:
    // `components/cart/actions.ts` is an ordinary name. Found in `vercel/commerce`,
    // where five live network endpoints were reported as having no detected use.
    let found = entry_kinds(&[(
        "actions.ts",
        "\"use server\";\n\nexport async function addItem(id: string) {\n  return id;\n}\n\n\
         export async function removeItem(id: string) {\n  return id;\n}\n",
    )]);
    for name in ["addItem", "removeItem"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "{name} missing from {found:?}"
        );
    }
}

#[test]
fn the_directive_inside_one_function_marks_only_that_one() {
    // Both forms are real: at the top of a file it marks every export, at the top of a
    // body it marks that body. Treating the second as the first would call an ordinary
    // helper a network endpoint.
    let found = entry_kinds(&[(
        "mixed.ts",
        "export async function ordinary(id: string) {\n  return id;\n}\n\n\
         export async function action(id: string) {\n  \"use server\";\n  return id;\n}\n",
    )]);
    assert!(
        found.contains(&("action".to_string(), EntryKind::HttpRoute)),
        "got {found:?}"
    );
    assert!(
        !found.iter().any(|(name, _)| name == "ordinary"),
        "an ordinary function beside an action was called an endpoint: {found:?}"
    );
}

#[test]
fn the_words_in_a_comment_are_not_a_directive() {
    // A directive is the first statement and it is quoted. Anything else that mentions
    // it is prose.
    let found = entry_kinds(&[(
        "notes.ts",
        "// use server: this file does not, despite the comment\n\n\
         export async function helper(id: string) {\n  return id;\n}\n",
    )]);
    assert!(
        !found.iter().any(|(name, _)| name == "helper"),
        "a comment was read as a directive: {found:?}"
    );
}

/// A web framework calls its route handlers; the source never does. This project has a
/// whole page about FastAPI, and a FastAPI handler was being reported as dead code.
#[test]
fn a_route_handler_is_an_entry_point() {
    let found = entry_kinds(&[
        (
            "api.py",
            "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n\n\
             @router.get(\"/pets\")\nasync def list_pets():\n    return []\n\n\n\
             @router.post(\"/pets\")\nasync def create_pet(body):\n    return body\n",
        ),
        (
            "web.py",
            "from flask import Flask\n\napp = Flask(__name__)\n\n\n\
             @app.route(\"/status\")\ndef health():\n    return \"ok\"\n",
        ),
    ]);
    for name in ["list_pets", "create_pet", "health"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "{name} is reached by a request, not by a call: {found:?}"
        );
    }
}

/// The broker calls the consumer and the scheduler calls the job. Neither appears in a
/// call graph built from the source alone.
#[test]
fn a_consumer_and_a_scheduled_job_are_entry_points() {
    let found = entry_kinds(&[
        (
            "tasks.py",
            "from celery import Celery\n\napp = Celery()\n\n\n\
             @app.task\ndef send_email(to):\n    return to\n",
        ),
        (
            "Jobs.java",
            "package app;\n\npublic class Jobs {\n    \
             @Scheduled(cron = \"0 0 * * * *\")\n    public void rotateKeys() {\n    }\n\n    \
             @KafkaListener(topics = \"orders\")\n    public void consume(String payload) {\n    }\n}\n",
        ),
    ]);
    assert!(
        found.contains(&("send_email".to_string(), EntryKind::QueueConsumer)),
        "got {found:?}"
    );
    assert!(
        found.contains(&("consume".to_string(), EntryKind::QueueConsumer)),
        "got {found:?}"
    );
    assert!(
        found.contains(&("rotateKeys".to_string(), EntryKind::ScheduledJob)),
        "got {found:?}"
    );
}

/// The annotation's arguments may contain dots of their own. Stripping the qualifier
/// before cutting the arguments off read the last dotted piece of an *argument* as the
/// annotation's name, so a version number in a route path — `/v1.0/status` — quietly
/// turned a live handler into dead code.
#[test]
fn a_dot_in_the_arguments_does_not_hide_the_annotation() {
    let found = entry_kinds(&[
        (
            "w.py",
            "from flask import Flask\n\napp = Flask(__name__)\n\n\n\
             @app.route(\"/v1.0/status\")\ndef ping():\n    return \"ok\"\n",
        ),
        (
            "C.java",
            "package app;\n\npublic class C {\n    \
             @GetMapping(Routes.PETS)\n    public String listPets() {\n        return \"\";\n    }\n\n    \
             @ExceptionHandler(RuntimeException.class)\n    public String onFailure(RuntimeException e) {\n        return \"\";\n    }\n}\n",
        ),
    ]);
    for name in ["ping", "listPets", "onFailure"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "a dot in the arguments hid {name}: {found:?}"
        );
    }
}

/// Qualified and bare spellings of the same annotation mean the same thing, and the
/// bracket forms differ by language.
#[test]
fn the_annotation_is_found_however_it_is_spelled() {
    let found = entry_kinds(&[
        (
            "T.java",
            "package app;\n\npublic class T {\n    \
             @org.junit.jupiter.api.Test\n    public void qualified() {\n    }\n}\n",
        ),
        ("lib.rs", "#[tokio::test]\nfn spawns() {\n}\n"),
    ]);
    for name in ["qualified", "spawns"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::Test)),
            "got {found:?}"
        );
    }
}

/// `export class` is how TypeScript writes almost every class, and the word `export`
/// sits between the decorator and the declaration. Reading it as a preceding line ended
/// the run of annotations before it reached the decorator, so a NestJS controller — the
/// class the whole framework is organised around — matched nothing.
#[test]
fn a_modifier_between_the_annotation_and_the_declaration_is_not_a_line_before_it() {
    let found = entry_kinds(&[
        ("a.ts", "@Controller('pets')\nexport class Pets {\n}\n"),
        ("b.ts", "@Controller\nexport default class Vendors {\n}\n"),
        ("c.ts", "@Controller('x')\nclass Bare {\n}\n"),
        (
            "D.java",
            "package app;\n\n@RestController\npublic class D {\n}\n",
        ),
    ]);
    for name in ["Pets", "Vendors", "Bare", "D"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "got {found:?}"
        );
    }
}

/// The other side of that: what precedes the symbol on its line may be a real statement
/// rather than a modifier, and then an annotation above belongs to the statement.
#[test]
fn a_nested_declaration_does_not_inherit_the_annotation() {
    let found = entry_kinds(&[("lib.rs", "#[test]\nfn outer() { fn inner() {} }\n")]);
    assert!(
        found.contains(&("outer".to_string(), EntryKind::Test)),
        "got {found:?}"
    );
    assert!(
        !found.iter().any(|(name, _)| name == "inner"),
        "`inner` is not the test; `outer` is: {found:?}"
    );
}

/// NestJS puts the method on the handler the way Spring does, and the handlers are what
/// serve the requests.
#[test]
fn a_nest_handler_is_an_entry_point() {
    let found = entry_kinds(&[(
        "pets.controller.ts",
        "import { Controller, Get, Post } from '@nestjs/common';\n\n\
         @Controller('pets')\nexport class PetsController {\n  \
         @Get()\n  findAll(): string[] {\n    return [];\n  }\n\n  \
         @Post()\n  create(body: object): object {\n    return body;\n  }\n}\n",
    )]);
    for name in ["findAll", "create"] {
        assert!(
            found.contains(&(name.to_string(), EntryKind::HttpRoute)),
            "got {found:?}"
        );
    }
}

/// actix-web and Rocket put the method on the handler as an attribute. These were
/// covered only by coincidence: `#[get("/health")]` above `fn health` spells the
/// symbol's own name in a string literal, which `fr unused` skips. The route path is
/// not required to repeat the function name.
#[test]
fn a_rust_route_attribute_is_an_entry_point() {
    let found = entry_kinds(&[(
        "lib.rs",
        "#[actix_web::get(\"/up\")]\nasync fn health() -> &'static str {\n    \"ok\"\n}\n",
    )]);
    assert!(
        found.contains(&("health".to_string(), EntryKind::HttpRoute)),
        "got {found:?}"
    );
}

/// A decorator's name is not unique across libraries. `@patch` is `unittest.mock`'s far
/// more often than it is FastAPI's, and matching the name alone tagged twenty-two of
/// `psf/black`'s test methods as remotely reachable HTTP routes — including, once, a
/// `self` parameter. A route decorator names a URL path; a mock names a module.
#[test]
fn a_route_rule_requires_the_path_and_not_just_the_name() {
    let found = entry_kinds(&[(
        "test_release.py",
        "from unittest.mock import patch\n\n\n\
         @patch(\"black.release.git\")\ndef test_current_version(mocked):\n    pass\n\n\n\
         @app.patch(\"/pets/{id}\")\nasync def edit_pet(id):\n    return id\n",
    )]);
    assert!(
        found.contains(&("edit_pet".to_string(), EntryKind::HttpRoute)),
        "got {found:?}"
    );
    assert!(
        !found.contains(&("test_current_version".to_string(), EntryKind::HttpRoute)),
        "`unittest.mock.patch` is not an HTTP route: {found:?}"
    );
}

/// A rule asking for an annotation's argument without naming the annotation matches
/// nothing, which reads exactly like a framework that is covered and simply absent.
#[test]
fn a_rule_that_cannot_mean_anything_is_rejected_when_it_loads() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("broken.yaml"),
        "- id: nonsense\n  kind: http-route\n  languages: [python]\n  \
         matches:\n    annotation_argument_prefix: \"/\"\n",
    )
    .expect("the catalog");

    let mut catalog = Catalog::builtin().expect("the built-in catalogs");
    let err = catalog
        .load_dir(dir.path())
        .expect_err("a rule with no annotation to read the argument of");
    assert!(
        format!("{err:#}").contains("without naming the annotation"),
        "the error should say what is wrong with the rule: {err:#}"
    );
}

/// A parameter of an annotated method is not itself an entry point.
///
/// It sits inside the declaration's argument list, so the walk up to the annotation
/// passes through the method's own text. `@KafkaListener` marks `consume` as a queue
/// consumer; `payload` is what the queue hands it.
#[test]
fn a_parameter_does_not_carry_its_method_s_annotation() {
    let found = entry_kinds(&[
        (
            "Jobs.java",
            "package app;\n\npublic class Jobs {\n    \
             @KafkaListener(topics = \"orders\")\n    public void consume(String payload) {\n    }\n\n    \
             @ExceptionHandler(RuntimeException.class)\n    public String onFailure(RuntimeException e) {\n        return \"\";\n    }\n}\n",
        ),
        (
            "signals.py",
            "from django.dispatch import receiver\n\n\n\
             @receiver(post_save)\ndef audit(sender, **kwargs):\n    pass\n",
        ),
    ]);
    for name in ["consume", "onFailure", "audit"] {
        assert!(
            found.iter().any(|(found_name, _)| found_name == name),
            "{name} is the entry point: {found:?}"
        );
    }
    for parameter in ["payload", "e", "sender", "kwargs"] {
        assert!(
            !found.iter().any(|(name, _)| name == parameter),
            "`{parameter}` is an argument the framework passes, not an entry point: {found:?}"
        );
    }
}

/// A catalogue's enum-valued fields are enums.
///
/// `deny_unknown_fields` rejects a misspelled key. A misspelled *value* was accepted:
/// `symbol_kind: functoin` and `languages: [pyhton]` both parsed, loaded, and matched
/// nothing — indistinguishable from a rule that is present and simply never true. Both
/// are parsed into the type they denote now, so the mistake is a message at load.
#[test]
fn a_misspelled_value_in_a_catalogue_is_rejected_when_it_loads() {
    let cases = [
        (
            "symbol_kind",
            "- id: typo\n  kind: http-route\n  languages: [python]\n  \
             matches:\n    name_suffix: handler\n    symbol_kind: functoin\n",
            "functoin",
        ),
        (
            "languages",
            "- id: typo\n  kind: http-route\n  languages: [pyhton]\n  \
             matches:\n    name_suffix: handler\n",
            "pyhton",
        ),
        (
            "kind",
            "- id: typo\n  kind: htp-route\n  languages: [python]\n  \
             matches:\n    name_suffix: handler\n",
            "htp-route",
        ),
    ];

    for (field, yaml, typo) in cases {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("rules.yaml"), yaml).expect("the catalog");
        let err = Catalog::builtin()
            .expect("the built-in catalogs")
            .load_dir(dir.path())
            .expect_err(&format!("`{typo}` is not a {field}"));
        let message = format!("{err:#}");
        assert!(
            message.contains(typo),
            "the error should name the typo `{typo}`: {message}"
        );
    }
}

/// `*` is the one value that is not a language, and it still means every language.
#[test]
fn a_rule_can_apply_to_every_language() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("rules.yaml"),
        "- id: anywhere\n  kind: exported-api\n  languages: [\"*\"]\n  \
         matches:\n    name_suffix: _handler\n",
    )
    .expect("the catalog");
    let mut catalog = Catalog::builtin().expect("the built-in catalogs");
    catalog.load_dir(dir.path()).expect("a wildcard rule loads");

    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(tmp.path().join("a.py"), "def my_handler():\n    return 1\n").expect("py");
    std::fs::write(
        tmp.path().join("a.go"),
        "package main\n\nfunc my_handler() int {\n\treturn 1\n}\n",
    )
    .expect("go");
    let index =
        fun_refactor::index::Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let found: Vec<_> = catalog
        .detect(&index)
        .into_iter()
        .filter(|e| e.rule == "anywhere")
        .filter_map(|e| index.symbol(e.symbol).map(|s| s.language.name()))
        .collect();
    assert!(found.contains(&"python"), "got {found:?}");
    assert!(found.contains(&"go"), "got {found:?}");
}

/// A rule that says nothing matched nothing, quietly.
///
/// The comment on the old check said an empty matcher "is never what a catalog author
/// means" and then returned false, which is also not what they meant — silently doing
/// the opposite of the dangerous thing is still silent. And the list of what counted as
/// a condition had drifted from the fields that exist: `symbol_kind`, `exported` and
/// `top_level` were not on it, so a rule whose only condition was one of those counted
/// as saying nothing and matched nothing.
#[test]
fn a_rule_that_names_no_condition_is_rejected_when_it_loads() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("rules.yaml"),
        "- id: says-nothing\n  kind: http-route\n  languages: [python]\n  matches: {}\n",
    )
    .expect("the catalog");

    let err = Catalog::builtin()
        .expect("the built-in catalogs")
        .load_dir(dir.path())
        .expect_err("a matcher with no conditions");
    assert!(format!("{err:#}").contains("names no condition"), "{err:#}");
}

/// The conditions that were missing from that list work on their own.
#[test]
fn a_kind_or_an_export_is_a_condition_by_itself() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("rules.yaml"),
        "- id: any-function\n  kind: exported-api\n  languages: [python]\n  \
         matches:\n    symbol_kind: function\n",
    )
    .expect("the catalog");
    let mut catalog = Catalog::builtin().expect("the built-in catalogs");
    catalog.load_dir(dir.path()).expect("the rule loads");

    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(tmp.path().join("a.py"), "def handler():\n    return 1\n").expect("the file");
    let index =
        fun_refactor::index::Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let found: Vec<_> = catalog
        .detect(&index)
        .into_iter()
        .filter(|e| e.rule == "any-function")
        .filter_map(|e| index.symbol(e.symbol).map(|s| s.name.clone()))
        .collect();
    assert_eq!(found, vec!["handler".to_string()], "a kind is a condition");
}
