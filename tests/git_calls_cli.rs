use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn git_output(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-c",
            "user.name=fr fixture",
            "-c",
            "user.email=fr@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap()
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = git_output(root, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn fr(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fr"));
    cmd.args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(["git", "diff"])
        .args(args);
    cmd
}
fn report(root: &Path, args: &[&str]) -> Value {
    let out = fr(root, args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn error(root: &Path, args: &[&str], expected: &str) {
    let out = fr(root, args).output().unwrap();
    assert!(!out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains(expected),
        "{value}"
    );
}
fn init(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture"]);
}

#[test]
fn calls_page_incoming_outgoing_file_scope_and_unresolved_sites() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before="def leaf():\n    pass\ndef changed():\n    value = 1\n    leaf()\n    missing()\ndef caller():\n    changed()\ndef unrelated():\n    leaf()\nchanged()\n";
    fs::write(root.join("code.py"), before).unwrap();
    commit(root);
    fs::write(
        root.join("code.py"),
        before.replace("value = 1", "value = 2"),
    )
    .unwrap();
    let all = report(root, &["code.py", "--calls"]);
    assert_eq!(all["view"], "calls");
    assert_eq!(all["page"]["total"], 8);
    for side in ["before", "after"] {
        let rows = all["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["side"] == side)
            .collect::<Vec<_>>();
        assert_eq!(
            rows.iter()
                .filter(|r| r["scope_relation"] == "incoming")
                .count(),
            2
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["scope_relation"] == "outgoing")
                .count(),
            2
        );
        assert!(rows
            .iter()
            .any(|r| r["caller"].is_null() && r["caller_scope"] == "file"));
        assert!(rows.iter().any(|r| r["status"] == "unresolved"
            && r["name"]["text"] == "missing"
            && r["callee"].is_null()));
        assert!(rows.iter().any(|r| r["caller"]["name"]["text"] == "changed"
            && r["callee"]["name"]["text"] == "leaf"
            && r["confidence"] == "exact"));
        assert_eq!(all["structure"]["coverage"][side]["declarations"], 1);
        assert_eq!(
            all["structure"]["coverage"][side]["calls"]["selected_rows"],
            4
        );
    }
    assert!(!all.to_string().contains("value ="));
    assert!(!all.to_string().contains("unrelated"));
    let first = report(root, &["code.py", "--calls", "--limit", "1"]);
    let token = first["page"]["next"].as_str().unwrap();
    let rest = report(
        root,
        &["code.py", "--calls", "--limit", "500", "--cursor", token],
    );
    assert_eq!(
        rest["entries"],
        json!(&all["entries"].as_array().unwrap()[1..])
    );
    assert_eq!(
        report(root, &["code.py", "--calls", "--direction", "incoming"])["page"]["total"],
        4
    );
    assert_eq!(
        report(root, &["code.py", "--calls", "--direction", "outgoing"])["page"]["total"],
        4
    );
    error(
        root,
        &[
            "code.py",
            "--calls",
            "--direction",
            "incoming",
            "--cursor",
            token,
        ],
        "stale cursor",
    );
    error(
        root,
        &["code.py", "--symbols", "--cursor", token],
        "stale cursor",
    );
    assert!(!fr(root, &["code.py", "--calls", "--symbols"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(!fr(root, &["code.py", "--direction", "incoming"])
        .output()
        .unwrap()
        .status
        .success());
    fs::write(
        root.join("code.py"),
        before.replace("value = 1", "value = 3"),
    )
    .unwrap();
    error(
        root,
        &["code.py", "--calls", "--cursor", token],
        "stale cursor",
    );
}

#[test]
fn calls_use_each_selected_snapshot_and_keep_external_targets_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before="from dependency import external\ndef first():\n    pass\ndef second():\n    pass\ndef third():\n    pass\ndef changed():\n    first()\n    external()\n";
    fs::write(root.join("code.py"), before).unwrap();
    fs::write(root.join("dependency.py"), "def external():\n    pass\n").unwrap();
    commit(root);
    let staged = before.replace("    first()", "    second()");
    fs::write(root.join("code.py"), &staged).unwrap();
    git(root, &["add", "code.py"]);
    fs::write(
        root.join("code.py"),
        before.replace("    first()", "    third()"),
    )
    .unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    for (flags, old, new) in [
        (vec!["--staged"], "first", "second"),
        (vec!["--since", "HEAD"], "first", "third"),
        (vec![], "second", "third"),
    ] {
        let mut args = vec!["code.py", "--calls", "--direction", "outgoing"];
        args.extend(flags);
        let value = report(root, &args);
        for (side, target) in [("before", old), ("after", new)] {
            let rows = value["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["side"] == side)
                .collect::<Vec<_>>();
            assert!(rows.iter().any(|r| r["callee"]["name"]["text"] == target));
            assert!(rows
                .iter()
                .any(|r| r["name"]["text"] == "external" && r["status"] == "unresolved"));
            assert_eq!(
                value["structure"]["coverage"][side]["calls"]["cross_file"],
                "not-collected"
            );
        }
    }
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    git(root, &["rm", "-f", "code.py"]);
    let deleted = report(root, &["code.py", "--calls", "--staged"]);
    assert!(deleted["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["side"] == "before"));
    assert_eq!(
        deleted["structure"]["coverage"]["after"]["status"],
        "no-changed-lines"
    );
}

#[test]
fn typed_receiver_reads_staged_source_instead_of_working_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before="struct A;\nimpl A { fn run(&self) {} }\nstruct B;\nimpl B { fn run(&self) {} }\nfn changed(value: &A) {\n    value.run();\n    let number = 1;\n}\n";
    fs::write(root.join("code.rs"), before).unwrap();
    commit(root);
    fs::write(
        root.join("code.rs"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    git(root, &["add", "code.rs"]);
    fs::write(
        root.join("code.rs"),
        before
            .replace("value: &A", "value: &B")
            .replace("number = 1", "number = 3"),
    )
    .unwrap();
    let value = report(
        root,
        &["code.rs", "--calls", "--staged", "--direction", "outgoing"],
    );
    assert!(!value["entries"].as_array().unwrap().is_empty());
    for row in value["entries"].as_array().unwrap() {
        assert_eq!(row["callee"]["qualifier"]["text"], "A", "{value}");
    }
}

#[test]
fn dispatch_candidates_keep_their_origin_and_confidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before="trait Task { fn run(&self); }\nstruct A;\nimpl Task for A { fn run(&self) {} }\nstruct B;\nimpl Task for B { fn run(&self) {} }\nfn changed(value: &dyn Task) {\n    value.run();\n    let number = 1;\n}\n";
    fs::write(root.join("code.rs"), before).unwrap();
    commit(root);
    fs::write(
        root.join("code.rs"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    let value = report(root, &["code.rs", "--calls", "--direction", "outgoing"]);
    let candidates = value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["dispatch_candidate"] == true)
        .collect::<Vec<_>>();
    assert!(!candidates.is_empty(), "{value}");
    assert!(candidates
        .iter()
        .all(|r| r["status"] == "dispatch-candidate"
            && r["confidence"] == "field-based"
            && r["origin"] != "resolved"));
}

#[test]
fn call_coverage_distinguishes_unsupported_partial_and_metadata_only_sides() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("style.css"), ".before {}\n").unwrap();
    fs::write(root.join("code.py"), "def before():\n    pass\n").unwrap();
    fs::write(root.join("binary.rs"), b"a\0b").unwrap();
    commit(root);
    fs::write(root.join("style.css"), ".after {}\n").unwrap();
    let css = report(root, &["style.css", "--calls"]);
    assert_eq!(
        css["structure"]["coverage"]["after"]["calls"]["status"],
        "unsupported-language"
    );
    fs::write(root.join("code.py"), "def broken(:\n    missing()\n").unwrap();
    let partial = report(root, &["code.py", "--calls"]);
    assert_eq!(
        partial["structure"]["coverage"]["after"]["status"],
        "partial"
    );
    assert_eq!(
        partial["structure"]["coverage"]["after"]["calls"]["status"],
        "partial"
    );
    fs::write(root.join("binary.rs"), b"new\0blob").unwrap();
    let binary = report(root, &["binary.rs", "--calls"]);
    assert_eq!(binary["page"]["total"], 0);
    assert_eq!(binary["structure"]["coverage"]["after"]["status"], "binary");
}

#[test]
fn changed_containers_include_unchanged_children_and_internal_calls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before = "def leaf():\n    pass\nclass Group:\n    def changed(self):\n        number = 1\n        self.sibling()\n    def sibling(self):\n        leaf()\n";
    fs::write(root.join("code.py"), before).unwrap();
    commit(root);
    fs::write(
        root.join("code.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    let all = report(root, &["code.py", "--calls"]);
    assert_eq!(all["page"]["total"], 4, "{all}");
    for side in ["before", "after"] {
        let rows = all["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["side"] == side)
            .collect::<Vec<_>>();
        let outgoing = rows
            .iter()
            .find(|r| r["callee"]["name"]["text"] == "leaf")
            .unwrap();
        assert_eq!(outgoing["scope_relation"], "outgoing");
        assert_eq!(outgoing["caller"]["changed_declaration"], false);
        assert_eq!(outgoing["caller"]["in_selection"], true);
        let internal = rows
            .iter()
            .find(|r| r["callee"]["name"]["text"] == "sibling")
            .unwrap();
        assert_eq!(internal["scope_relation"], "internal");
        assert_eq!(internal["callee"]["changed_declaration"], false);
        assert_eq!(internal["callee"]["in_selection"], true);
    }
    let incoming = report(root, &["code.py", "--calls", "--direction", "incoming"]);
    assert_eq!(incoming["page"]["total"], 2);
    assert!(incoming["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["scope_relation"] == "internal"));
    let explicit = report(root, &["code.py", "--calls", "--direction", "both"]);
    assert_eq!(explicit, all);
}

#[test]
fn unresolved_names_are_utf8_bounded_and_conversion_mismatches_refuse() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let name = "λ".repeat(180);
    let before = format!("def changed():\n    number = 1\n    {name}()\n");
    fs::write(root.join("code.py"), &before).unwrap();
    fs::write(root.join(".gitattributes"), "code.py text eol=lf\n").unwrap();
    commit(root);
    let after = before.replace("number = 1", "number = 2");
    fs::write(root.join("code.py"), &after).unwrap();
    let value = report(root, &["code.py", "--calls"]);
    assert_eq!(value["page"]["total"], 2, "{value}");
    for row in value["entries"].as_array().unwrap() {
        assert_eq!(row["name"]["bytes"], name.len());
        assert_eq!(row["name"]["truncated"], true);
        assert_eq!(row["name"]["text"].as_str().unwrap().len(), 256);
    }
    fs::write(root.join("code.py"), after.replace('\n', "\r\n")).unwrap();
    error(root, &["code.py", "--calls"], "snapshot");
}
