//! The JSON surface an agent scripts against, checked end to end.

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
    let parsed = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout is not JSON: {:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    (parsed, stderr, out.status.success())
}

const TWO_PROCESSES: [(&str, &str); 2] = [
    (
        "go1/a.go",
        "package a\n\nfunc Process() int {\n\treturn 1\n}\n",
    ),
    (
        "go2/b.go",
        "package b\n\nfunc Process() int {\n\treturn 2\n}\n",
    ),
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
    let tmp = workspace(&[("py/app.py", "def process():\n    return 1\n")]);
    let (printed, stderr, ok) = run_json(&tmp, &["def", "py/app.py:abc:1", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "invalid-input");
    let message = printed["error"]["message"].as_str().expect("a message");
    assert!(message.contains("that looks like a position"), "{message}");
    assert!(
        message.contains("'abc'"),
        "the wrong part is named: {message}"
    );
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
    let d = nodes
        .iter()
        .find(|n| n["name"] == "d")
        .expect("a node for d");
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
    assert_eq!(
        printed["openapi"], "3.1.0",
        "the document is still the document"
    );
    let notes = printed["notes"].as_array().expect("a notes array");
    assert!(
        notes
            .iter()
            .any(|n| n.as_str().is_some_and(|n| n.contains("returns status 201"))),
        "got {notes:?}"
    );
}

#[test]
fn the_openapi_status_note_speaks_the_language_of_the_input() {
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

const HELPER_AND_CALLER: [(&str, &str); 1] = [(
    "a.go",
    "package p\n\nfunc Helper() int {\n\treturn 1\n}\n\nfunc Caller() int {\n\treturn Helper()\n}\n",
)];

#[test]
fn usages_reports_the_definition_sites_apart_from_the_uses() {
    // `fr usages` counts uses only, while `fr rename` also edits definitions.
    let tmp = workspace(&HELPER_AND_CALLER);
    let (printed, _, ok) = run_json(&tmp, &["usages", "Helper", "--json"]);
    assert!(ok);
    assert_eq!(printed["usages"].as_array().expect("a list").len(), 1);
    let definitions = printed["definitions"].as_array().expect("a list");
    assert_eq!(definitions.len(), 1, "got {definitions:?}");
    assert_eq!(definitions[0]["line"], 3);
    assert_eq!(definitions[0]["col"], 6);
    assert!(
        definitions[0]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("a.go")),
        "got {definitions:?}"
    );
}

#[test]
fn a_rename_summary_counts_definition_edits_beside_reference_edits() {
    let tmp = workspace(&HELPER_AND_CALLER);
    let (printed, _, ok) = run_json(&tmp, &["rename", "Helper", "Fetched", "--json"]);
    assert!(ok);
    assert_eq!(printed["definition_edits"], 1);
    assert_eq!(printed["reference_edits"], 1);
    assert_eq!(printed["files_changed"], 1);
}

#[test]
fn a_change_diff_header_is_workspace_relative_while_the_path_stays_absolute() {
    // `git apply -p1` refuses `a//absolute/path`; a joining agent still needs the absolute
    // `path` field.
    let tmp = workspace(&HELPER_AND_CALLER);
    let (printed, _, ok) = run_json(&tmp, &["rename", "Helper", "Fetched", "--json"]);
    assert!(ok);
    let change = &printed["changes"].as_array().expect("changes")[0];
    let diff = change["diff"].as_str().expect("a diff");
    assert!(diff.starts_with("--- a/a.go\n+++ b/a.go\n"), "{diff}");
    let path = change["path"].as_str().expect("a path");
    assert!(
        std::path::Path::new(path).is_absolute(),
        "the path field stays absolute: {path}"
    );
}

#[cfg(unix)]
#[test]
fn scan_names_each_file_absolutely_and_lists_skipped_symlinks() {
    // `scan` said `"path": "./x"` while every other command says `"file"`
    // absolutely, and a symlinked source file vanished from the listing entirely.
    let tmp = workspace(&HELPER_AND_CALLER);
    std::os::unix::fs::symlink(tmp.path().join("a.go"), tmp.path().join("link.go"))
        .expect("a symlink");
    let (printed, _, ok) = run_json(&tmp, &["scan", "--json"]);
    assert!(ok);
    let files = printed["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "the link is not a second copy: {files:?}");
    let file = files[0]["file"].as_str().expect("an absolute file");
    assert!(std::path::Path::new(file).is_absolute(), "{file}");
    assert!(files[0]["path"].is_string(), "the old key survives");
    let skipped = printed["skipped"].as_array().expect("a skipped list");
    assert_eq!(skipped.len(), 1, "got {skipped:?}");
    assert!(
        skipped[0]["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("symlink to")),
        "got {skipped:?}"
    );
    assert!(
        skipped[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("link.go")),
        "got {skipped:?}"
    );
}

/// A Python module whose one symbol a second, oversized module reads.
fn skipping_workspace() -> tempfile::TempDir {
    let mut big = String::from("from a import helper\n");
    for i in 0..40 {
        big.push_str(&format!("def pad_{i}():\n    return helper()\n"));
    }
    workspace(&[
        (
            "a.py",
            "def helper():\n    return 1\n\ndef caller():\n    return helper()\n",
        ),
        ("big.py", &big),
    ])
}

#[test]
fn a_file_skipped_for_size_rides_in_every_answer_it_could_falsify() {
    // A skipped file can hold the reference that makes the answer wrong.
    let tmp = skipping_workspace();
    let (printed, _, ok) = run_json(
        &tmp,
        &[
            "--max-file-size",
            "200",
            "rename",
            "helper",
            "assist",
            "--json",
        ],
    );
    assert!(ok, "{printed}");
    let skipped = printed["skipped_files"].as_array().expect("a skipped list");
    assert_eq!(skipped.len(), 1, "got {printed}");
    assert!(
        skipped[0]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("big.py")),
        "got {skipped:?}"
    );
    assert!(
        skipped[0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("size limit")),
        "got {skipped:?}"
    );

    // The read-only answers carry the same fact.
    let (printed, _, ok) = run_json(
        &tmp,
        &["--max-file-size", "200", "usages", "helper", "--json"],
    );
    assert!(ok, "{printed}");
    assert_eq!(
        printed["skipped_files"].as_array().map(Vec::len),
        Some(1),
        "got {printed}"
    );

    // And a mutation through the shared presenter, delete, warns loudly on
    // stderr for a human run.
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["--max-file-size", "200", "delete", "caller", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("were not read") && stderr.contains("big.py"),
        "the human run warns on stderr: {stderr}"
    );
    assert!(
        stderr.contains("--max-file-size"),
        "the warning says how to widen the scan: {stderr}"
    );
}

#[test]
fn a_file_that_did_not_parse_rides_in_every_answer_it_could_falsify() {
    // `fr parse` reported the broken file and the commands people act on did not.
    let tmp = workspace(&[
        ("a.py", "def helper():\n    return 1\n"),
        ("broken.py", "def caller(:\n    return helper()\n"),
    ]);

    let (printed, _, ok) = run_json(&tmp, &["unused", "--json"]);
    assert!(ok, "{printed}");
    let unparsed = printed["unparsed_files"]
        .as_array()
        .expect("an unparsed list");
    assert_eq!(unparsed.len(), 1, "got {printed}");
    assert!(
        unparsed[0]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("broken.py")),
        "got {unparsed:?}"
    );
    assert!(
        unparsed[0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("syntax errors")),
        "got {unparsed:?}"
    );

    // The read-only answers built on the same index carry the same fact.
    let (printed, _, ok) = run_json(&tmp, &["usages", "helper", "--json"]);
    assert!(ok, "{printed}");
    assert_eq!(
        printed["unparsed_files"].as_array().map(Vec::len),
        Some(1),
        "got {printed}"
    );

    // And every human run says it on stderr, from the one choke point, so a
    // command that never mentioned the file before mentions it now.
    for command in [
        ["unused"].as_slice(),
        ["symbols"].as_slice(),
        ["graph"].as_slice(),
        ["usages", "helper"].as_slice(),
    ] {
        let out = Command::cargo_bin("fr")
            .expect("the binary")
            .args(command)
            .arg("-C")
            .arg(tmp.path())
            .output()
            .expect("running fr");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("did not parse in full") && stderr.contains("broken.py"),
            "`fr {}` must say what it could not read. {stderr}",
            command.join(" ")
        );
        assert!(
            stderr.contains("fr parse"),
            "the warning must point at the positions. {stderr}"
        );
    }
}

const RENDER_PY: &str = "def render():\n    return 1\n\ndef user():\n    return render()\n";

#[test]
fn a_failed_expectation_under_write_rolls_back_and_says_so() {
    // RECIPES.md promises one transaction.
    let tmp = workspace(&[
        ("m.py", RENDER_PY),
        (
            "expectfail.recipe",
            "schema 1\n\nrecipe expectfail {\n  rename to \"render_label\" \
             where name=\"render\" kind=function\n  expect changed >= 5 files\n}\n",
        ),
    ]);
    let (printed, _, ok) = run_json(&tmp, &["recipe", "expectfail.recipe", "--write", "--json"]);
    assert!(!ok, "a failed expectation is a failed run");
    assert_eq!(printed["ok"], false, "{printed}");
    assert_eq!(printed["applied"], false, "{printed}");
    assert_eq!(printed["rolled_back"], true, "{printed}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("m.py")).expect("the file"),
        RENDER_PY,
        "the workspace holds the text it started from"
    );

    // The dry run says it neither applied nor rolled anything back.
    let (printed, _, _) = run_json(&tmp, &["recipe", "expectfail.recipe", "--json"]);
    assert_eq!(printed["applied"], false, "{printed}");
    assert_eq!(printed["rolled_back"], false, "{printed}");
}

#[test]
fn a_recipe_stopped_by_a_refusal_exits_5_with_the_blocking_positions() {
    // The standalone command's refusal exits 5.
    let tmp = workspace(&[
        ("m.py", RENDER_PY),
        (
            "break.recipe",
            "schema 1\n\nrecipe breakme {\n  delete where name=\"render\" kind=function\n}\n",
        ),
    ]);
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["recipe", "break.recipe", "--json", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    assert_eq!(out.status.code(), Some(5), "a refusal-stop is a refusal");
    let printed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a report");
    assert!(
        printed["stopped_by_refusal"].as_bool().unwrap_or(false),
        "{printed}"
    );
    let refusal = &printed["recipes"][0]["steps"][0]["refusals"][0];
    let references = refusal["references"].as_array().expect("references");
    assert_eq!(references.len(), 1, "got {refusal}");
    assert!(
        references[0]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("m.py")),
        "got {references:?}"
    );
    assert_eq!(references[0]["line"], 5, "got {references:?}");
    assert!(references[0]["col"].is_u64(), "got {references:?}");
}

#[test]
fn a_recipe_warning_has_the_same_shape_as_a_standalone_one() {
    // `fr rename --json` emits warnings as {file, line, col, kind, detail}.
    let tmp = workspace(&[
        (
            "m.py",
            "def render():\n    return 1\n\n# render is drawn here\ndef user():\n    return render()\n",
        ),
        (
            "tidy.recipe",
            "schema 1\n\nrecipe tidy {\n  rename to \"draw\" where name=\"render\" \
             kind=function\n}\n",
        ),
    ]);
    let (printed, _, ok) = run_json(&tmp, &["recipe", "tidy.recipe", "--json"]);
    assert!(ok, "{printed}");
    let warnings = printed["recipes"][0]["steps"][0]["warnings"]
        .as_array()
        .expect("warnings");
    let textual = warnings
        .iter()
        .find(|w| w["kind"] == "textual-occurrence")
        .unwrap_or_else(|| panic!("a comment mention warns: {warnings:?}"));
    assert!(
        textual["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("m.py")),
        "got {textual}"
    );
    assert!(
        textual["line"].is_u64() && textual["col"].is_u64(),
        "{textual}"
    );
    assert!(textual["detail"].is_string(), "{textual}");
}

#[test]
fn one_recipe_file_rolls_back_every_earlier_recipe() {
    let tmp = workspace(&[
        ("m.py", RENDER_PY),
        (
            "whole.recipe",
            "schema 1\n\nrecipe rename-render {\n  rename to \"draw\" where name=\"render\" kind=function\n}\n\nrecipe rename-user {\n  rename to \"paint\" where name=\"user\" kind=function\n  expect changed >= 5 files\n}\n",
        ),
    ]);
    let (printed, _, ok) = run_json(&tmp, &["recipe", "whole.recipe", "--write", "--json"]);
    assert!(
        !ok,
        "a failed later recipe must abort the file transaction."
    );
    assert_eq!(printed["failed_recipe"], "rename-user", "{printed}");
    assert_eq!(
        printed["recipes"].as_array().map(Vec::len),
        Some(2),
        "{printed}"
    );
    assert_eq!(printed["applied"], false, "{printed}");
    assert_eq!(printed["rolled_back"], true, "{printed}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("m.py")).expect("the file"),
        RENDER_PY,
        "the first recipe must not reach disk before the second has passed"
    );
}

#[test]
fn one_recipe_file_commits_the_complete_virtual_workspace() {
    let tmp = workspace(&[
        ("m.py", RENDER_PY),
        (
            "whole.recipe",
            "schema 1\n\nrecipe rename-render {\n  rename to \"draw\" where name=\"render\" kind=function\n}\n\nrecipe rename-again {\n  rename to \"paint\" where name=\"draw\" kind=function\n}\n",
        ),
    ]);
    let (printed, _, ok) = run_json(&tmp, &["recipe", "whole.recipe", "--write", "--json"]);
    assert!(ok, "{printed}");
    assert_eq!(printed["applied"], true, "{printed}");
    assert_eq!(printed["files_changed"], 1, "{printed}");
    assert_eq!(
        printed["recipes"].as_array().map(Vec::len),
        Some(2),
        "{printed}"
    );
    let text = std::fs::read_to_string(tmp.path().join("m.py")).expect("the file");
    assert!(text.contains("paint()"), "{text}");
    assert!(!text.contains("render") && !text.contains("draw"), "{text}");
}

#[test]
fn a_delete_refusal_carries_its_blocking_positions_as_data() {
    let tmp = workspace(&[("m.py", RENDER_PY)]);
    let (printed, _, ok) = run_json(&tmp, &["delete", "render", "--json"]);
    assert!(!ok);
    assert_eq!(printed["error"]["kind"], "refused");
    let references = printed["error"]["references"]
        .as_array()
        .expect("references beside the prose");
    assert_eq!(references.len(), 1, "got {printed}");
    assert!(
        references[0]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("m.py")),
        "got {references:?}"
    );
    assert_eq!(references[0]["line"], 5, "{references:?}");
    assert_eq!(references[0]["col"], 12, "{references:?}");
}

#[test]
fn the_translate_listing_obeys_json() {
    // List mode ignored --json and printed prose into a parser.
    let tmp = workspace(&[("m.py", RENDER_PY)]);
    let (printed, _, ok) = run_json(&tmp, &["translate", "m.py", "--json"]);
    assert!(ok, "{printed}");
    assert_eq!(printed["language"], "python", "{printed}");
    let options = printed["options"].as_array().expect("options");
    assert!(!options.is_empty(), "{printed}");
    for option in options {
        assert!(option["target"].is_string(), "{option}");
        assert!(option["destination"].is_string(), "{option}");
    }
}

#[test]
fn a_single_file_translation_reports_the_sweeps_fidelity_block() {
    // The directory sweep said how much of the draft carried.
    let tmp = workspace(&[("m.py", RENDER_PY)]);
    let (printed, _, ok) = run_json(&tmp, &["translate", "m.py", "typescript", "--json"]);
    assert!(ok, "{printed}");
    assert_eq!(printed["functions"], 2, "{printed}");
    assert!(printed["signatures_complete"].is_u64(), "{printed}");
    assert!(printed["carried_verbatim"].is_u64(), "{printed}");
    assert_eq!(printed["signatures_with_foreign_types"], 0, "{printed}");

    // The count still fires for a signature that really carries a foreign name.
    let tmp = workspace(&[("w.py", "def show(w: Widget) -> Widget:\n    return w\n")]);
    let (printed, _, ok) = run_json(&tmp, &["translate", "w.py", "typescript", "--json"]);
    assert!(ok, "{printed}");
    assert_eq!(printed["signatures_with_foreign_types"], 1, "{printed}");
}

#[test]
fn a_symbols_span_round_trips_into_extract() {
    // The listing mixed 0-based byte spans with 1-based line and column, and emitted no end
    // position.
    let tmp = workspace(&[(
        "m.py",
        "def log_line():\n    print(\"hello\")\n\n\ndef caller():\n    log_line()\n",
    )]);
    let (printed, _, ok) = run_json(&tmp, &["symbols", "--json"]);
    assert!(ok);
    let symbol = printed
        .as_array()
        .expect("an array")
        .iter()
        .find(|s| s["name"] == "log_line")
        .expect("the function is listed")
        .clone();
    assert_eq!(symbol["start"]["line"], 1, "{symbol}");
    assert_eq!(symbol["start"]["col"], 1, "{symbol}");
    assert!(symbol["end"]["line"].is_u64(), "{symbol}");
    // The byte spans say they are bytes; the old spellings survive one release.
    assert_eq!(symbol["name_span_bytes"], symbol["name_span"], "{symbol}");
    assert_eq!(symbol["full_span_bytes"], symbol["full_span"], "{symbol}");

    let range = format!(
        "{}:{}:{}-{}:{}",
        symbol["file"].as_str().expect("a file"),
        symbol["start"]["line"],
        symbol["start"]["col"],
        symbol["end"]["line"],
        symbol["end"]["col"],
    );
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["extract", &range, "wrapped", "--function", "-C"])
        .arg(tmp.path())
        .assert()
        .success();
}

#[test]
fn explain_emits_selectors_and_expectations_as_structures() {
    // --explain --json re-serialized selectors and expectations as surface
    // strings, which an agent had to re-parse with the recipe grammar in its head.
    let tmp = workspace(&[
        ("m.py", RENDER_PY),
        (
            "plan.recipe",
            "schema 1\n\nrecipe plan {\n  rename to \"draw\" where name=\"render\" \
             kind=function\n  expect changed >= 1 files\n}\n",
        ),
    ]);
    let (printed, _, ok) = run_json(&tmp, &["recipe", "plan.recipe", "--explain", "--json"]);
    assert!(ok, "{printed}");
    let step = &printed[0]["steps"][0];
    let parts = step["selector_parts"].as_array().expect("selector parts");
    assert!(
        parts
            .iter()
            .any(|p| p["field"] == "name" && p["op"] == "=" && p["value"] == "render"),
        "got {parts:?}"
    );
    let expects = printed[0]["expectations_parts"]
        .as_array()
        .expect("expectation parts");
    assert_eq!(expects[0]["predicate"], "changed", "{expects:?}");
    assert_eq!(expects[0]["op"], ">=", "{expects:?}");
    assert_eq!(expects[0]["value"], 1, "{expects:?}");
}
