//! The call graph leaves the tool as data, not only as a summary.

use assert_cmd::Command;
use predicates::str::contains;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(path, content).expect("the file");
    }
    tmp
}

fn graph_json(tmp: &tempfile::TempDir) -> serde_json::Value {
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["graph", "--json", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("graph emits json")
}

const CALLS_RS: &str = "fn leaf() {}\nfn top() {\n    leaf();\n}\n";

#[test]
fn the_json_export_carries_nodes_and_edges() {
    let tmp = workspace(&[("a.rs", CALLS_RS)]);
    let printed = graph_json(&tmp);

    let nodes = printed["nodes"].as_array().expect("a nodes array");
    let leaf = nodes
        .iter()
        .find(|n| n["name"] == "leaf")
        .expect("a node for leaf");
    let top = nodes
        .iter()
        .find(|n| n["name"] == "top")
        .expect("a node for top");
    assert_eq!(leaf["file"], "a.rs", "the file is workspace-relative");
    assert_eq!(leaf["line"], 1);
    assert_eq!(top["line"], 2);
    assert_eq!(leaf["kind"], "function");

    let edges = printed["edges"].as_array().expect("an edges array");
    assert_eq!(edges.len(), 1, "got {edges:?}");
    assert_eq!(edges[0]["from"], top["id"]);
    assert_eq!(edges[0]["to"], leaf["id"]);
    assert_eq!(edges[0]["confidence"], "exact");
    assert_eq!(edges[0]["origin"], "resolved");
}

#[test]
fn the_counts_stay_beside_the_graph() {
    // Whatever read the old shape keeps working against the new one.
    let tmp = workspace(&[("a.rs", CALLS_RS)]);
    let printed = graph_json(&tmp);
    assert_eq!(printed["functions"], 2);
    assert_eq!(printed["calls"], 1);
    assert_eq!(printed["unresolved_calls"], 0);
    assert!(printed["by_confidence"].is_object());
}

#[test]
fn two_functions_sharing_a_name_are_told_apart_by_file() {
    let tmp = workspace(&[
        ("one/util.rs", "pub fn process() {}\n"),
        ("two/util.rs", "pub fn process() {}\n"),
    ]);
    let printed = graph_json(&tmp);
    let mut files: Vec<&str> = printed["nodes"]
        .as_array()
        .expect("a nodes array")
        .iter()
        .filter(|n| n["name"] == "process")
        .map(|n| n["file"].as_str().expect("a file"))
        .collect();
    files.sort();
    assert_eq!(files, ["one/util.rs", "two/util.rs"]);
}

#[test]
fn dot_labels_carry_the_file_under_the_name() {
    let tmp = workspace(&[("a.rs", CALLS_RS)]);
    let out = Command::cargo_bin("fr")
        .expect("the binary")
        .args(["graph", "--dot", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    assert!(out.status.success());
    let dot = String::from_utf8_lossy(&out.stdout);

    // The syntax shape Graphviz parses: a digraph, one bracketed statement per
    // line, and the file on the label's second line behind an escaped newline.
    assert!(dot.starts_with("digraph calls {"), "got:\n{dot}");
    assert!(dot.trim_end().ends_with('}'), "got:\n{dot}");
    assert!(dot.contains("[label=\"top\\na.rs\"]"), "got:\n{dot}");
    assert!(dot.contains("[label=\"leaf\\na.rs\"]"), "got:\n{dot}");
}

#[test]
fn a_call_at_file_scope_is_counted_apart_from_an_unresolved_one() {
    // A shell script calls its own function at the top level.
    let tmp = workspace(&[(
        "lib.sh",
        "#!/usr/bin/env bash\ndeploy() {\n  target=prod\n}\n\ndeploy\n",
    )]);
    let printed = graph_json(&tmp);

    assert_eq!(printed["file_scope_calls"], 1, "got:\n{printed}");
    assert_eq!(printed["unresolved_calls"], 0, "got:\n{printed}");
}

#[test]
fn a_language_with_no_call_graph_refuses_instead_of_answering_nothing() {
    // The matrix says `n/a` for SCSS, and the caller pointed at one symbol.
    let tmp = workspace(&[(
        "m.scss",
        "@mixin flex { display: flex; }\n.a { @include flex; }\n.b { @include flex; }\n",
    )]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["callers", "flex", "-C"])
        .arg(tmp.path())
        .assert()
        .code(5)
        .stderr(contains("`fr usages` lists every `@include`"));
}

#[test]
fn asking_for_dot_and_json_together_is_refused() {
    let tmp = workspace(&[("a.rs", CALLS_RS)]);
    Command::cargo_bin("fr")
        .expect("the binary")
        .args(["graph", "--dot", "--json", "-C"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(contains("one format at a time"));
}
