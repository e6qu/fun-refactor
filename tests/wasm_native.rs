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

fn rename_review(ws: &Workspace, name: &str) -> String {
    ws.preview(
        &serde_json::json!({
            "operation": "rename", "path": "a.py", "line": 1, "col": 5, "name": name
        })
        .to_string(),
    )
}

#[test]
fn browser_reviews_survive_restart_without_applying_early() {
    let source = "def leaf():\n    return 7\ndef main():\n    return leaf()\n";
    let ws = workspace(&[("a.py", source)]);
    let checkpoint = ws.session();
    let review = rename_review(&ws, "renamed");
    let value = json(&review);
    assert_eq!(value["review"]["schema"], "fr-browser-review-1", "{value}");
    assert_eq!(value["review"]["native_checks"], serde_json::json!([]));
    assert_eq!(ws.session(), checkpoint);
    assert_eq!(ws.read("a.py"), source);
    let mut restarted = Workspace::restore_session(&checkpoint).unwrap();
    let applied = json(&restarted.apply_review(&review));
    assert_eq!(applied, value["review"]["result"]);
    assert_eq!(restarted.read("a.py"), source.replace("leaf", "renamed"));
    assert_eq!(
        json(&restarted.transaction_patch(1, false)),
        value["review"]["patch"]
    );
    let fresh = workspace(&[("a.py", &source.replace("leaf", "renamed"))]);
    assert_eq!(json(&restarted.graph()), json(&fresh.graph()));
    assert!(json(&restarted.undo(1)).get("error").is_none());
    assert_eq!(restarted.read("a.py"), source);
    assert!(json(&restarted.redo(1)).get("error").is_none());
    assert_eq!(restarted.read("a.py"), source.replace("leaf", "renamed"));
}

#[test]
fn browser_reviews_reject_changed_source_history_and_tampered_results() {
    let source = "def leaf():\n    return 7\n";
    let mut ws = workspace(&[("a.py", source)]);
    let review = rename_review(&ws, "renamed");
    let mut tampered = json(&review);
    tampered["review"]["result"]["files"][0]["after"] = serde_json::json!("wrong");
    let before = ws.session();
    assert!(json(&ws.apply_review(&tampered.to_string()))
        .get("error")
        .is_some());
    assert_eq!(ws.session(), before);
    assert!(json(&ws.rename("a.py", 1, 5, "other"))
        .get("error")
        .is_none());
    let changed = ws.session();
    assert!(json(&ws.apply_review(&review)).get("error").is_some());
    assert_eq!(ws.session(), changed);
    assert!(json(&ws.undo(1)).get("error").is_none());
    assert_eq!(ws.read("a.py"), source);
    let undone = ws.session();
    assert!(json(&ws.apply_review(&review)).get("error").is_some());
    assert_eq!(
        ws.session(),
        undone,
        "identical source with different history is stale"
    );
}

#[test]
fn browser_review_refusals_leave_other_workspaces_and_history_untouched() {
    let ws = workspace(&[("a.py", "def leaf():\n    return 7\n")]);
    let other = workspace(&[("a.py", "def other():\n    return 9\n")]);
    let before = ws.session();
    for request in [
        r#"{"operation":"rename","path":"a.py","line":1,"col":5,"name":"not valid"}"#,
        r#"{"operation":"rename","path":"a.py","line":1,"col":5,"name":"valid","extra":true}"#,
        r#"{"operation":"unknown"}"#,
    ] {
        assert!(json(&ws.preview(request)).get("error").is_some());
        assert_eq!(ws.session(), before);
        assert!(other.read("a.py").contains("def other"));
    }
}

#[test]
fn browser_reviews_bind_unrelated_files_and_compaction() {
    let source = "def leaf():\n    return 7\n";
    let ws = workspace(&[("a.py", source), ("notes.txt", "original")]);
    let review = rename_review(&ws, "renamed");
    let mut different = workspace(&[("a.py", source), ("notes.txt", "changed")]);
    let before = different.session();
    assert!(json(&different.apply_review(&review))
        .get("error")
        .is_some());
    assert_eq!(different.session(), before);
    let mut ws = Workspace::restore_session(&ws.session()).unwrap();
    assert!(json(&ws.apply_review(&review)).get("error").is_none());
    let next = rename_review(&ws, "next");
    assert!(json(&ws.compact_history(0)).get("error").is_none());
    let compacted = ws.session();
    assert!(json(&ws.apply_review(&next)).get("error").is_some());
    assert_eq!(ws.session(), compacted);
}

#[test]
fn browser_review_limits_and_repeated_application_do_not_write() {
    let mut ws = workspace(&[("a.py", "def leaf():\n    return 7\n")]);
    let before = ws.session();
    assert!(json(&ws.preview(&" ".repeat(65537))).get("error").is_some());
    assert_eq!(ws.session(), before);
    let review = rename_review(&ws, "renamed");
    assert!(json(&ws.apply_review(&review)).get("error").is_none());
    let applied = ws.session();
    assert!(json(&ws.apply_review(&review)).get("error").is_some());
    assert_eq!(ws.session(), applied);
}

#[test]
fn browser_neighbourhood_keeps_unresolved_calls_and_coverage() {
    let ws = workspace(&[("a.py", "def entry():\n    unknown()\n")]);
    let drawing = json(&ws.graph_around("a.py", 1, 5, 2));
    assert_eq!(drawing["unresolved"][0]["name"], "unknown");
    assert_eq!(drawing["coverage"]["all_visible_calls_resolved"], false);
    assert_eq!(drawing["coverage"]["external_callers_known"], false);
    assert_eq!(drawing["coverage"]["runtime_dispatch_established"], false);
}

#[test]
fn browser_review_refuses_an_edit_that_would_exceed_checkpoint_capacity() {
    let source = "def leaf():\n    return 7\n# ".to_string() + &"x".repeat(1_500_000) + "\n";
    let ws = workspace(&[("a.py", &source)]);
    let before = ws.session();
    assert!(json(&before).get("error").is_none());
    let review = json(&rename_review(&ws, "renamed"));
    assert!(review["error"].as_str().unwrap().contains("4 MiB"));
    assert_eq!(ws.session(), before);
}

#[test]
#[cfg(feature = "cli")]
fn reviewed_browser_patch_has_the_same_graph_through_the_python_sdk() {
    let source = "def leaf():\n    return 7\ndef main():\n    return leaf()\n";
    let mut ws = workspace(&[("a.py", source)]);
    let review = rename_review(&ws, "renamed");
    assert!(json(&ws.apply_review(&review)).get("error").is_none());
    let receiver = tempfile::tempdir().unwrap();
    std::fs::write(receiver.path().join("a.py"), source).unwrap();
    let patch = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        patch.path(),
        json(&review)["review"]["patch"]["patch"].as_str().unwrap(),
    )
    .unwrap();
    for args in [vec!["init", "-q"], vec!["apply", "--check"], vec!["apply"]] {
        let mut command = std::process::Command::new("git");
        command.current_dir(receiver.path()).args(&args);
        if args[0] == "apply" {
            command.arg(patch.path());
        }
        assert!(command.output().unwrap().status.success());
    }
    assert_eq!(
        std::fs::read_to_string(receiver.path().join("a.py")).unwrap(),
        ws.read("a.py")
    );
    let sdk = std::process::Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONPATH", concat!(env!("CARGO_MANIFEST_DIR"), "/sdk/python/src"))
        .args(["-c", "import json,sys; from pathlib import Path; from fr_ir.runtime import FrClient; print(json.dumps(FrClient(Path(sys.argv[1]), executable=sys.argv[2]).call('graph').to_data()))"])
        .arg(receiver.path())
        .arg(env!("CARGO_BIN_EXE_fr"))
        .output().unwrap();
    assert!(
        sdk.status.success(),
        "{}",
        String::from_utf8_lossy(&sdk.stderr)
    );
    let native: serde_json::Value = serde_json::from_slice(&sdk.stdout).unwrap();
    let browser = json(&ws.graph())["details"].clone();
    for key in [
        "functions",
        "calls",
        "source_digests",
        "coverage",
        "by_confidence",
        "by_origin",
    ] {
        assert_eq!(native[key], browser[key], "{key}");
    }
    let names = |report: &serde_json::Value| {
        report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["name"].as_str().unwrap().to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(names(&native), names(&browser));
    let named_edges = |report: &serde_json::Value| {
        let names = report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| (node["id"].as_u64().unwrap(), node["name"].clone()))
            .collect::<BTreeMap<_, _>>();
        let mut edges = report["edges"].as_array().unwrap().clone();
        for edge in &mut edges {
            let from = edge["from"].as_u64().unwrap();
            let to = edge["to"].as_u64().unwrap();
            edge["from"] = names[&from].clone();
            edge["to"] = names[&to].clone();
        }
        edges.sort_by_cached_key(serde_json::Value::to_string);
        edges
    };
    assert_eq!(named_edges(&native), named_edges(&browser));
}

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
            "an option either turns up or says why not: {option}"
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
        "the menu has to show the route before a reader picks it: {fastapi}"
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

    let files = json(&ws.files());
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == written),
        "a file a refactoring created has to join the workspace:\n{files}"
    );

    assert_eq!(applied["schema"], "fr-memory-apply-1");
    assert_eq!(applied["transaction"], 1);
    assert!(applied["transaction_basis"]
        .as_str()
        .unwrap()
        .starts_with("frmb1:"));
    let patch = json(&ws.transaction_patch(1, false));
    assert_eq!(patch["schema"], "fr-memory-patch-1");
    assert!(patch["patch"].as_str().unwrap().contains("new file mode"));

    let undone = json(&ws.undo(1));
    assert_eq!(undone["status"], "undone");
    assert!(json(&ws.files())
        .as_array()
        .unwrap()
        .iter()
        .all(|file| file["path"] != written));
    let redone = json(&ws.redo(1));
    assert_eq!(redone["status"], "applied");
    assert!(json(&ws.files())
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == written));
}

#[test]
fn browser_edits_have_checked_stack_history_and_a_cumulative_patch() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    let first = json(&ws.rename("a.py", 1, 5, "sum_one"));
    assert_eq!(first["transaction"], 1, "{first}");
    let second = json(&ws.rename("a.py", 1, 5, "sum_two"));
    assert_eq!(second["transaction"], 2, "{second}");

    let refused = json(&ws.undo(1));
    assert!(refused["error"].as_str().unwrap().contains("stack order"));
    assert!(ws.read("a.py").contains("sum_two"));

    assert_eq!(json(&ws.undo(2))["status"], "undone");
    assert!(ws.read("a.py").contains("sum_one"));
    let history = json(&ws.history());
    assert_eq!(history["applied"], serde_json::json!([1]));
    assert_eq!(history["redo"], serde_json::json!([2]));

    assert_eq!(json(&ws.redo(2))["status"], "applied");
    let patch = json(&ws.patch());
    let text = patch["patch"].as_str().unwrap();
    assert!(text.contains("-def add"), "{text}");
    assert!(text.contains("+def sum_two"), "{text}");
    assert!(!text.contains("sum_one"), "{text}");
    assert_eq!(patch["patch_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn browser_session_restores_files_history_and_the_next_checked_transition() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    assert_eq!(json(&ws.rename("a.py", 1, 5, "sum_one"))["transaction"], 1);
    assert_eq!(json(&ws.rename("a.py", 1, 5, "sum_two"))["transaction"], 2);
    assert_eq!(json(&ws.undo(2))["status"], "undone");

    let encoded = ws.session();
    let mut restored = Workspace::restore_session(&encoded).expect("the checkpoint restores");
    assert_eq!(restored.read("a.py"), ws.read("a.py"));
    assert_eq!(json(&restored.history()), json(&ws.history()));
    assert_eq!(json(&restored.patch()), json(&ws.patch()));
    assert_eq!(json(&restored.redo(2))["status"], "applied");
    assert!(restored.read("a.py").contains("sum_two"));
    assert_eq!(json(&restored.undo(2))["status"], "undone");
}

#[test]
fn browser_compaction_preserves_the_patch_and_retained_reversal_cycle() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    for name in ["sum_one", "sum_two", "sum_three"] {
        let applied = json(&ws.rename("a.py", 1, 5, name));
        assert!(applied["error"].is_null(), "{applied}");
    }
    let before = json(&ws.patch());
    let compacted = json(&ws.compact_history(1));
    assert_eq!(compacted["records_before"], 3);
    assert_eq!(compacted["records_after"], 1);
    assert_eq!(compacted["frozen_transactions"], 2);
    assert_eq!(compacted["undo"], 3);
    assert_eq!(json(&ws.patch()), before);
    assert!(json(&ws.transaction_patch(1, false))["error"].is_string());

    assert_eq!(json(&ws.undo(3))["status"], "undone");
    let encoded = ws.session();
    let mut restored = Workspace::restore_session(&encoded).expect("compacted state restores");
    assert_eq!(json(&restored.redo(3))["status"], "applied");
    assert_eq!(json(&restored.patch()), before);
    assert_eq!(
        json(&restored.rename("a.py", 1, 5, "sum_four"))["transaction"],
        4
    );
}

#[test]
fn browser_session_refuses_tampering_unknown_fields_and_unsafe_paths() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    ws.rename("a.py", 1, 5, "sum_one");
    let encoded = ws.session();

    let changed = encoded.replace("sum_one", "sum_many");
    let error = Workspace::restore_session(&changed)
        .err()
        .expect("tampering refuses");
    assert!(error.contains("digest"), "{error}");

    let mut value = json(&encoded);
    value["extra"] = serde_json::json!(true);
    let error = Workspace::restore_session(&value.to_string())
        .err()
        .expect("unknown fields refuse");
    assert!(error.contains("unknown field"), "{error}");

    let unsafe_path = encoded.replace("a.py", "../a.py");
    let error = Workspace::restore_session(&unsafe_path)
        .err()
        .expect("unsafe paths refuse");
    assert!(error.contains("invalid workspace path"), "{error}");
}

#[test]
fn browser_session_refuses_a_workspace_beyond_its_file_cap() {
    let files = (0..4097)
        .map(|number| (format!("{number}.txt"), String::new()))
        .collect::<BTreeMap<_, _>>();
    let ws = Workspace::load(files).expect("the in-memory workspace loads");
    let refused = json(&ws.session());
    assert!(refused["error"].as_str().unwrap().contains("cannot become"));
}

#[test]
fn browser_session_file_cap_includes_an_absent_redo_target() {
    let mut files = (0..4095)
        .map(|number| (format!("{number}.txt"), String::new()))
        .collect::<BTreeMap<_, _>>();
    files.insert("app/api/users/[id]/route.ts".to_string(), ROUTE.to_string());
    let mut ws = Workspace::load(files).expect("the capped workspace loads");
    let applied = json(&ws.translate("app/api/users/[id]/route.ts", "fastapi"));
    assert_eq!(applied["transaction"], 1, "{applied}");
    assert_eq!(json(&ws.undo(1))["status"], "undone");
    let refused = json(&ws.session());
    assert!(refused["error"].as_str().unwrap().contains("cannot become"));
}

#[test]
fn a_noop_refactoring_does_not_create_a_browser_transaction() {
    let mut ws = workspace(&[("a.py", "def add(x: int) -> int:\n    return x\n")]);
    let applied = json(&ws.organize_imports("a.py"));
    assert_eq!(applied["schema"], "fr-memory-apply-1", "{applied}");
    assert!(applied["transaction"].is_null(), "{applied}");
    assert_eq!(applied["files"], serde_json::json!([]));
    assert_eq!(json(&ws.history())["records"], serde_json::json!([]));
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
