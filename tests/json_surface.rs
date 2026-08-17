//! The JSON surface an agent scripts against, checked end to end.
//!
//! Every error path used to print prose to stderr and nothing to stdout, so a
//! caller that passed `--json` had nothing to parse. And several reports left
//! out fields a program needs. A call tree flattened to name and depth. A flow
//! step with no line. An unused report without its own caveat. These tests pin
//! the machine-readable half of each contract.

use assert_cmd::Command;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(path, content).expect("the file");
    }
    tmp
}

/// Run `fr` and hand back stdout as JSON, stderr as text, and the success flag.
fn run_json(tmp: &tempfile::TempDir, args: &[&str]) -> (serde_json::Value, String, bool) {
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(args)
        .arg("-C")
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let parsed = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("stdout is not JSON: {:?}", String::from_utf8_lossy(&out.stdout)));
    (parsed, stderr, out.status.success())
}

const TWO_PROCESSES: [(&str, &str); 2] = [
    ("go1/a.go", "package a\n\nfunc Process() int {\n\treturn 1\n}\n"),
    ("go2/b.go", "package b\n\nfunc Process() int {\n\treturn 2\n}\n"),
];

#[test]
fn a_missing_symbol_becomes_a_json_error_object() {
    let tmp = workspace(&TWO_PROCESSES);
    let (printed, stderr, ok) = run_json(&tmp, &["def", "nosuch", "--json"]);
    assert!(!ok, "a missing symbol is still a failure");
    assert_eq!(printed["error"]["kind"], "not-found");
    let message = printed["error"]["message"].as_str().expect("a message");
    assert!(message.contains("no symbol named 'nosuch'"), "{message}");
    // The prose stays on stderr for a human reading the same run.
    assert!(stderr.contains("no symbol named 'nosuch'"), "{stderr}");
}

#[test]
fn an_ambiguous_name_lists_its_candidates_as_data() {
    let tmp = workspace(&TWO_PROCESSES);
    let (printed, _, ok) = run_json(&tmp, &["def", "Process", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "ambiguous");
    let candidates = printed["error"]["candidates"]
        .as_array()
        .expect("a candidates array");
    assert_eq!(candidates.len(), 2, "got {candidates:?}");
    for candidate in candidates {
        assert_eq!(candidate["name"], "Process");
        assert_eq!(candidate["kind"], "function");
        assert_eq!(candidate["line"], 3, "lines are 1-based");
        assert_eq!(candidate["col"], 6, "columns are 1-based");
    }
    let paths: Vec<&str> = candidates
        .iter()
        .map(|c| c["path"].as_str().expect("a path"))
        .collect();
    assert_ne!(paths[0], paths[1], "each rival keeps its own file");
}

#[test]
fn a_target_shaped_like_a_position_is_refused_as_one() {
    // `py/app.py:abc:1` used to fall through to name lookup and answer "no symbol
    // named 'py/app.py:abc:1'", pointing the reader at the wrong mistake.
    let tmp = workspace(&[("py/app.py", "def process():\n    return 1\n")]);
    let (printed, stderr, ok) = run_json(&tmp, &["def", "py/app.py:abc:1", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "invalid-input");
    let message = printed["error"]["message"].as_str().expect("a message");
    assert!(message.contains("that looks like a position"), "{message}");
    assert!(message.contains("'abc'"), "the wrong part is named: {message}");
    assert!(stderr.contains("that looks like a position"), "{stderr}");

    // A name that never resembled a position still resolves as a name.
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["def", "process", "-C"])
        .arg(tmp.path())
        .assert()
        .success();
}

#[test]
fn a_missing_column_is_named_as_the_missing_part() {
    let tmp = workspace(&[("py/app.py", "def process():\n    return 1\n")]);
    let (printed, _, ok) = run_json(&tmp, &["def", "py/app.py:2", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "invalid-input");
    let message = printed["error"]["message"].as_str().expect("a message");
    assert!(message.contains("no column"), "{message}");
}

#[test]
fn a_refusal_keeps_its_own_kind_in_json() {
    let tmp = workspace(&TWO_PROCESSES);
    let (printed, _, ok) = run_json(&tmp, &["rename", "go1/a.go:3:6", "9bad", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "refused");
    let message = printed["error"]["message"].as_str().expect("a message");
    assert!(message.contains("not a valid name"), "{message}");
}

const CALLS_RS: &str = "fn d() {}\nfn b() { d(); }\nfn c() {}\nfn a() { b(); c(); }\n";

#[test]
fn the_symbols_listing_says_where_each_name_sits() {
    let tmp = workspace(&[("calls.rs", CALLS_RS)]);
    let (printed, _, ok) = run_json(&tmp, &["symbols", "--json"]);
    assert!(ok);
    let symbols = printed.as_array().expect("an array");
    let b = symbols
        .iter()
        .find(|s| s["name"] == "b")
        .expect("a symbol for b");
    assert_eq!(b["line"], 2, "the line of the name span start");
    assert_eq!(b["col"], 4, "the column of the name span start");
    // The byte spans stay for whatever already reads them.
    assert!(b["name_span"]["start"].is_u64());
}

#[test]
fn a_call_tree_can_be_rebuilt_from_the_json() {
    // Name and depth alone flatten the walk: `b` and `c` both sit at depth 1, and
    // nothing said which of them `d` hangs off.
    let tmp = workspace(&[("calls.rs", CALLS_RS)]);
    let (printed, _, ok) = run_json(&tmp, &["callees", "a", "--depth", "2", "--json"]);
    assert!(ok);
    let nodes = printed["nodes"].as_array().expect("a nodes array");
    let root = nodes.iter().find(|n| n["name"] == "a").expect("the root");
    assert!(root["parent"].is_null(), "the root hangs off nothing");
    assert_eq!(root["line"], 4);
    let d = nodes.iter().find(|n| n["name"] == "d").expect("a node for d");
    assert_eq!(d["parent"]["name"], "b", "d hangs off b, not off c");
    assert!(d["parent"]["file"].as_str().is_some());
    assert_eq!(d["line"], 1);
    assert!(d["file"].as_str().is_some());
}

#[test]
fn a_value_flow_step_says_where_it_is_and_which_model_answered() {
    let tmp = workspace(&[(
        "flow.py",
        "def load(raw):\n    cleaned = raw.strip()\n    doubled = cleaned * 2\n    return doubled\n",
    )]);
    let (printed, _, ok) = run_json(&tmp, &["flow", "fwd", "flow.py:2:5", "--json"]);
    assert!(ok);
    // Provenance answers already name their model; this side names its own, so an
    // agent can dispatch on the field instead of sniffing the shape.
    assert_eq!(printed["model"], "value-flow");
    let steps = printed["steps"].as_array().expect("a steps array");
    assert!(!steps.is_empty());
    assert_eq!(steps[0]["line"], 2, "lines are 1-based");
    assert_eq!(steps[0]["col"], 5, "columns are 1-based");
}

#[test]
fn the_unused_report_carries_its_caveat_in_json() {
    let tmp = workspace(&[("calls.rs", CALLS_RS)]);
    let (printed, _, ok) = run_json(&tmp, &["unused", "--json"]);
    assert!(ok);
    let caveat = printed["caveat"].as_str().expect("a caveat string");
    assert!(caveat.contains("Reachability follows"), "{caveat}");
    let unused = printed["unused"].as_array().expect("an unused array");
    assert!(unused.iter().any(|s| s["name"] == "a"), "got {unused:?}");
}

const STATUS_ROUTE: &str = "import { NextResponse } from \"next/server\";\n\n\
export async function POST(req: Request) {\n  const body = await req.json();\n  \
return NextResponse.json(body, { status: 201 });\n}\n";

#[test]
fn the_openapi_json_carries_the_notes_in_the_payload() {
    let tmp = workspace(&[("app/api/pets/route.ts", STATUS_ROUTE)]);
    let (printed, _, ok) = run_json(&tmp, &["openapi", "--json"]);
    assert!(ok);
    assert_eq!(printed["openapi"], "3.1.0", "the document is still the document");
    let notes = printed["notes"].as_array().expect("a notes array");
    assert!(
        notes.iter().any(|n| n
            .as_str()
            .is_some_and(|n| n.contains("returns status 201"))),
        "got {notes:?}"
    );
}

#[test]
fn the_openapi_status_note_speaks_the_language_of_the_input() {
    // The tree is Next.js, and the note used to advise adding `status_code=` to a
    // `@router` decorator. That decorator exists only in the FastAPI file a
    // translation would write.
    let tmp = workspace(&[("app/api/pets/route.ts", STATUS_ROUTE)]);
    let (printed, _, ok) = run_json(&tmp, &["openapi", "--json"]);
    assert!(ok);
    let notes = printed["notes"].to_string();
    assert!(notes.contains("NextResponse.json"), "{notes}");
    assert!(!notes.contains("status_code="), "{notes}");
    assert!(!notes.contains("@router"), "{notes}");
    // The human run reads the same words on stderr.
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["openapi", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let human = String::from_utf8_lossy(&out.stderr);
    assert!(human.contains("NextResponse.json"), "{human}");
    assert!(!human.contains("status_code="), "{human}");
}
