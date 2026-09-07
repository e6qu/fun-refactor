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

#[test]
fn staged_context_resolves_only_selected_files_and_keeps_file_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before = "from dependency import leaf\nfrom excluded import external\ndef changed():\n    number = 1\n    leaf()\n    external()\n";
    fs::write(root.join("app.py"), before).unwrap();
    fs::write(root.join("dependency.py"), "def leaf():\n    pass\n").unwrap();
    fs::write(root.join("excluded.py"), "def external():\n    pass\n").unwrap();
    fs::write(
        root.join("caller.py"),
        "from app import changed\ndef invoke():\n    changed()\nchanged()\n",
    )
    .unwrap();
    commit(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    git(root, &["add", "app.py"]);
    let index = fs::read(root.join(".git/index")).unwrap();
    let args = [
        "app.py",
        "--calls",
        "--staged",
        "--include",
        "dependency.py",
        "--include",
        "caller.py",
    ];
    let value = report(root, &args);
    assert_eq!(
        value["structure"]["relationships"],
        "selected-file-call-candidates"
    );
    assert_eq!(value["page"]["total"], 8, "{value}");
    for side in ["before", "after"] {
        let rows = value["entries"]
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
        assert!(rows
            .iter()
            .filter(|r| r["scope_relation"] == "incoming")
            .all(|r| r["site"]["path"] == "caller.py" && r["callee"]["path"] == "app.py"));
        let leaf = rows
            .iter()
            .find(|r| r["callee"]["name"]["text"] == "leaf")
            .unwrap();
        assert_eq!(leaf["scope_relation"], "outgoing");
        assert_eq!(leaf["callee"]["path"], "dependency.py");
        assert_eq!(leaf["callee"]["changed_declaration"], false);
        assert_eq!(leaf["callee"]["in_selection"], false);
        assert_eq!(leaf["caller"]["changed_declaration"], true);
        assert!(rows
            .iter()
            .any(|r| r["name"]["text"] == "external" && r["status"] == "unresolved"));
        assert_eq!(
            value["structure"]["coverage"][side]["calls"]["cross_file"],
            "explicit-files-only"
        );
    }
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(!value.to_string().contains("number ="));
    let mut incoming = args.to_vec();
    incoming.extend(["--direction", "incoming"]);
    assert_eq!(report(root, &incoming)["page"]["total"], 4);
}

#[test]
fn staged_context_uses_independent_blob_sides_and_ignores_working_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before = "from dependency import old\ndef changed():\n    old()\n";
    fs::write(root.join("app.py"), before).unwrap();
    fs::write(root.join("dependency.py"), "def old():\n    pass\n").unwrap();
    commit(root);
    fs::write(root.join("app.py"), before.replace("old", "new")).unwrap();
    fs::write(root.join("dependency.py"), "def new():\n    pass\n").unwrap();
    git(root, &["add", "."]);
    fs::write(root.join("app.py"), b"working\0garbage").unwrap();
    fs::write(root.join("dependency.py"), b"working\0garbage").unwrap();
    let value = report(
        root,
        &[
            "app.py",
            "--calls",
            "--staged",
            "--include",
            "dependency.py",
        ],
    );
    assert_eq!(value["page"]["total"], 2, "{value}");
    for (side, name) in [("before", "old"), ("after", "new")] {
        let row = value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["side"] == side)
            .unwrap();
        assert_eq!(row["callee"]["name"]["text"], name);
        assert_eq!(row["callee"]["path"], "dependency.py");
    }
    let coverage = &value["structure"]["coverage"]["context"]["files"][0];
    assert_ne!(coverage["before"]["blob"], coverage["after"]["blob"]);
    assert!(!value.to_string().contains("garbage"));
}

#[test]
fn context_cursors_bind_blobs_even_when_call_rows_stay_equal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before = "from dependency import leaf\ndef changed():\n    number = 1\n    leaf()\n";
    fs::write(root.join("app.py"), before).unwrap();
    fs::write(root.join("dependency.py"), "def leaf():\n    return 1\n").unwrap();
    fs::write(
        root.join("caller.py"),
        "from app import changed\nchanged()\n",
    )
    .unwrap();
    commit(root);
    fs::write(
        root.join("app.py"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    git(root, &["add", "app.py"]);
    let args = [
        "app.py",
        "--calls",
        "--staged",
        "--include",
        "dependency.py",
        "--include",
        "caller.py",
    ];
    let all = report(root, &args);
    let equivalent = report(
        root,
        &[
            "app.py",
            "--calls",
            "--staged",
            "--include",
            "caller.py",
            "--include",
            "dependency.py",
            "--include",
            "caller.py",
        ],
    );
    assert_eq!(all, equivalent);
    let mut page = args.to_vec();
    page.extend(["--limit", "1"]);
    let first = report(root, &page);
    let token = first["page"]["next"].as_str().unwrap();
    let mut continued = args.to_vec();
    continued.extend(["--cursor", token]);
    assert_eq!(
        report(root, &continued)["entries"],
        json!(&all["entries"].as_array().unwrap()[1..])
    );
    error(
        root,
        &["app.py", "--calls", "--staged", "--cursor", token],
        "stale cursor",
    );
    error(
        root,
        &[
            "app.py",
            "--calls",
            "--staged",
            "--include",
            "caller.py",
            "--cursor",
            token,
        ],
        "stale cursor",
    );
    fs::write(root.join("dependency.py"), "def leaf():\n    return 2\n").unwrap();
    git(root, &["add", "dependency.py"]);
    assert_eq!(report(root, &args)["entries"], all["entries"]);
    error(root, &continued, "stale cursor");
}

#[test]
fn context_preserves_additions_deletions_and_unborn_sides() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(
        root.join("app.py"),
        "from dependency import leaf\ndef changed():\n    leaf()\n",
    )
    .unwrap();
    fs::write(root.join("dependency.py"), "def leaf():\n    pass\n").unwrap();
    git(root, &["add", "."]);
    let args = [
        "app.py",
        "--calls",
        "--staged",
        "--include",
        "dependency.py",
    ];
    let added = report(root, &args);
    assert_eq!(added["page"]["total"], 1, "{added}");
    assert_eq!(added["entries"][0]["side"], "after");
    assert_eq!(
        added["structure"]["coverage"]["context"]["files"][0]["before"]["status"],
        "absent"
    );
    commit(root);
    git(root, &["rm", "app.py", "dependency.py"]);
    let removed = report(root, &args);
    assert_eq!(removed["page"]["total"], 1, "{removed}");
    assert_eq!(removed["entries"][0]["side"], "before");
    assert_eq!(
        removed["structure"]["coverage"]["context"]["files"][0]["after"]["status"],
        "absent"
    );
}

#[test]
fn context_refuses_unsupported_selections_and_checks_selected_filters() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("app.py"), "def changed():\n    pass\n").unwrap();
    fs::write(root.join("dir/dep.py"), "def leaf():\n    pass\n").unwrap();
    fs::write(root.join("binary.py"), b"a\0b").unwrap();
    fs::write(root.join("style.css"), "a {}\n").unwrap();
    fs::write(root.join("unknown"), "unknown").unwrap();
    commit(root);
    fs::write(root.join("app.py"), "def changed():\n    missing()\n").unwrap();
    git(root, &["add", "app.py"]);
    for include in [
        "../dir/dep.py",
        "/tmp/dep.py",
        "app.py",
        "dir",
        "missing.py",
        "binary.py",
        "style.css",
        "unknown",
    ] {
        let out = fr(
            root,
            &["app.py", "--calls", "--staged", "--include", include],
        )
        .output()
        .unwrap();
        assert!(!out.status.success(), "{include}");
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(value.get("entries").is_none());
    }
    for flags in [
        vec!["app.py", "--calls", "--include", "dir/dep.py"],
        vec!["app.py", "--staged", "--include", "dir/dep.py"],
    ] {
        assert!(!fr(root, &flags).output().unwrap().status.success());
    }
    fs::write(root.join(".gitattributes"), "style.css filter=blocked\n").unwrap();
    let args = ["app.py", "--calls", "--staged", "--include", "dir/dep.py"];
    assert!(report(root, &args)["page"]["total"].as_u64().unwrap() > 0);
    fs::write(root.join(".gitattributes"), "dir/dep.py filter=blocked\n").unwrap();
    error(root, &args, "content filters");
}

#[test]
fn context_paths_are_literal_and_partial_parses_retain_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(
        root,
        &["init", "-q", "-b", "main", "--object-format=sha256"],
    );
    let context = "caller 名[*].py";
    fs::write(root.join("app.py"), "def changed():\n    pass\n").unwrap();
    fs::write(
        root.join(context),
        "from app import changed\ndef invoke():\n    changed()\n",
    )
    .unwrap();
    commit(root);
    fs::write(root.join("app.py"), "def changed():\n    return 1\n").unwrap();
    fs::write(
        root.join(context),
        "from app import changed\ndef invoke():\n    changed()\ndef broken(:\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    let value = report(
        root,
        &["app.py", "--calls", "--staged", "--include", context],
    );
    assert_eq!(value["page"]["total"], 2, "{value}");
    assert!(value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["site"]["path"] == context));
    assert_eq!(
        value["structure"]["coverage"]["after"]["calls"]["status"],
        "partial"
    );
    let coverage = &value["structure"]["coverage"]["context"]["files"][0];
    assert_eq!(coverage["after"]["status"], "partial");
    assert_eq!(coverage["after"]["blob"].as_str().unwrap().len(), 64);
}

#[cfg(unix)]
#[test]
fn context_refuses_an_index_change_after_blob_capture() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("app.py"), "def changed():\n    pass\n").unwrap();
    fs::write(
        root.join("caller.py"),
        "from app import changed\nchanged()\n",
    )
    .unwrap();
    commit(root);
    fs::write(root.join("app.py"), "def changed():\n    return 1\n").unwrap();
    git(root, &["add", "app.py"]);
    fs::write(
        root.join("caller.py"),
        "from app import changed\nchanged()\nchanged()\n",
    )
    .unwrap();
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(actual.status.success());
    let actual = String::from_utf8(actual.stdout).unwrap();
    let shim = root.join("shim");
    fs::create_dir(&shim).unwrap();
    fs::write(shim.join("git"), "#!/bin/sh\ncase \" $* \" in\n*' cat-file '*)\n  \"$FR_ACTUAL_GIT\" \"$@\" || exit $?\n  \"$FR_ACTUAL_GIT\" add caller.py\n  ;;\n*) exec \"$FR_ACTUAL_GIT\" \"$@\" ;;\nesac\n").unwrap();
    fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o755)).unwrap();
    let output = fr(
        root,
        &["app.py", "--calls", "--staged", "--include", "caller.py"],
    )
    .env("FR_ACTUAL_GIT", actual.trim())
    .env(
        "PATH",
        format!("{}:{}", shim.display(), std::env::var("PATH").unwrap()),
    )
    .output()
    .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("index entries changed"),
        "{value}"
    );
    assert!(value.get("entries").is_none());
}

#[test]
fn staged_context_merges_cross_file_dispatch_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    let before = "use crate::api::Task;\nfn changed(value: &dyn Task) {\n    value.run();\n    let number = 1;\n}\n";
    fs::write(root.join("app.rs"), before).unwrap();
    fs::write(root.join("api.rs"), "pub trait Task { fn run(&self); }\nstruct A;\nimpl Task for A { fn run(&self) {} }\nstruct B;\nimpl Task for B { fn run(&self) {} }\n").unwrap();
    commit(root);
    fs::write(
        root.join("app.rs"),
        before.replace("number = 1", "number = 2"),
    )
    .unwrap();
    git(root, &["add", "app.rs"]);
    let value = report(
        root,
        &["app.rs", "--calls", "--staged", "--include", "api.rs"],
    );
    for side in ["before", "after"] {
        let rows = value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["side"] == side && r["dispatch_candidate"] == true)
            .collect::<Vec<_>>();
        assert!(!rows.is_empty(), "{value}");
        assert!(rows.iter().all(|r| r["callee"]["path"] == "api.rs"
            && r["site"]["path"] == "app.rs"
            && r["scope_relation"] == "outgoing"
            && r["confidence"] == "field-based"));
    }
}

#[cfg(unix)]
#[test]
fn context_refuses_index_symlinks_conflicts_and_excess_paths() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init(root);
    fs::write(root.join("app.py"), "def changed():\n    pass\n").unwrap();
    fs::write(
        root.join("caller.py"),
        "from app import changed\nchanged()\n",
    )
    .unwrap();
    symlink("caller.py", root.join("link.py")).unwrap();
    commit(root);
    fs::write(root.join("app.py"), "def changed():\n    return 1\n").unwrap();
    git(root, &["add", "app.py"]);
    error(
        root,
        &["app.py", "--calls", "--staged", "--include", "link.py"],
        "regular file blobs",
    );
    let mut args = vec!["app.py", "--calls", "--staged"];
    for _ in 0..33 {
        args.extend(["--include", "caller.py"]);
    }
    error(root, &args, "at most 32");
    commit(root);
    git(root, &["checkout", "-qb", "other"]);
    fs::write(root.join("caller.py"), "other branch\n").unwrap();
    commit(root);
    git(root, &["checkout", "-q", "main"]);
    fs::write(root.join("caller.py"), "main branch\n").unwrap();
    commit(root);
    assert!(!git_output(root, &["merge", "other"]).status.success());
    error(
        root,
        &["app.py", "--calls", "--staged", "--include", "caller.py"],
        "unmerged call context",
    );
}
