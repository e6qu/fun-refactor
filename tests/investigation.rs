use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn report(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn exact_occurrences_distinguish_same_line_calls_and_unicode() {
    let dir = tempfile::tempdir().unwrap();
    let source = "def café(x):\n    return x\ndef main():\n    return café(1) + café(2)\n";
    fs::write(dir.path().join("subject.py"), source).unwrap();
    let result = report(dir.path(), &["project", "calls", ".", "--limit", "16"]);
    let sites: Vec<_> = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "call" && row["callee"]["name"] == "café")
        .map(|row| &row["site"]["origins"]["occurrence"])
        .collect();
    assert_eq!(sites.len(), 2, "{result}");
    assert_ne!(sites[0]["id"], sites[1]["id"]);
    for site in sites {
        let start = site["location"]["span"]["start"].as_u64().unwrap() as usize;
        let end = site["location"]["span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], "café");
        assert_eq!(site["location"]["range"]["start"]["line"], 4);
        assert_eq!(site["revision"], result["revision"]);
    }
    let bounded = report(dir.path(), &["project", "calls", ".", "--limit", "1"]);
    assert_eq!(bounded["items"].as_array().unwrap().len(), 1);
    assert!(!bounded["page"]["next"].is_null(), "{bounded}");
}

fn function_handle(root: &Path, name: &str) -> String {
    let found = report(root, &["project", "find", name]);
    found["rows"][0][0].as_str().unwrap().to_owned()
}

#[test]
fn scalar_flow_tracks_helpers_overwrites_branches_and_sanitizer_contexts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("subject.py"), "def helper(x):\n    return x\ndef run(flag):\n    value = source()\n    if flag:\n        value = 0\n    sink(helper(clean(value)))\ndef overwritten():\n    value = source()\n    value = 0\n    sink(helper(value))\n").unwrap();
    let rules = tempfile::NamedTempFile::new().unwrap();
    fs::write(rules.path(), r#"{"version":"test-1","sources":["source"],"sinks":["sink"],"sanitizers":{"clean":"html"}}"#).unwrap();
    let handle = function_handle(dir.path(), "run");
    let query = |context| {
        report(
            dir.path(),
            &[
                "project",
                "dataflow",
                &handle,
                "--rules",
                rules.path().to_str().unwrap(),
                "--context",
                context,
            ],
        )
    };
    let html = query("html");
    assert_eq!(html["complete"], true, "{html}");
    assert!(html["witnesses"].as_array().unwrap().is_empty());
    let sql = query("sql");
    assert_eq!(sql["complete"], true, "{sql}");
    assert_eq!(sql["witnesses"].as_array().unwrap().len(), 1, "{sql}");
    assert!(sql["witnesses"][0]["trace"]["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["role"] == "parameter"));
    let overwritten = function_handle(dir.path(), "overwritten");
    let result = report(
        dir.path(),
        &[
            "project",
            "dataflow",
            &overwritten,
            "--rules",
            rules.path().to_str().unwrap(),
        ],
    );
    assert!(
        result["witnesses"].as_array().unwrap().is_empty(),
        "{result}"
    );
}

#[test]
fn exhausted_or_unsupported_flow_never_reports_complete() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("subject.py"), "def recur(x):\n    return recur(x)\ndef run(x):\n    while x:\n        unknown(x)\n    x.field = 1\n    return recur(x)\n").unwrap();
    let handle = function_handle(dir.path(), "run");
    let result = report(dir.path(), &["project", "dataflow", &handle]);
    assert_eq!(result["complete"], false);
    let cutoffs = result["cutoffs"].to_string();
    for expected in [
        "loop-fixed-point",
        "unknown-external",
        "alias-or",
        "recursion",
    ] {
        assert!(cutoffs.contains(expected), "{result}");
    }
    let bounded = report(
        dir.path(),
        &["project", "dataflow", &handle, "--steps", "1"],
    );
    assert_eq!(bounded["complete"], false, "{bounded}");
}

fn plan() -> Value {
    serde_json::json!({"schema":"fr-investigation-plan-1","goal":"diagnose checkout", "acceptance":["oracle passes"], "steps":[
        {"id":"diagnose","question":"why negative?","inputs":[{"kind":"source","key":"subject.py","digest":null}], "required_checks":["oracle"], "satisfies":["oracle passes"]},
        {"id":"consumer","question":"which consumers?","depends_on":["diagnose"],"inputs":[{"kind":"lookup","key":"checkout","digest":null}]},
        {"id":"independent","question":"other file","inputs":[{"kind":"source","key":"other.py","digest":null}]}
    ]})
}

#[test]
fn resume_preserves_independent_inputs_and_propagates_staleness() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def checkout():\n    return 1\n",
    )
    .unwrap();
    fs::write(dir.path().join("other.py"), "def other():\n    return 2\n").unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), plan().to_string()).unwrap();
    let resumed = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    assert_eq!(resumed["plan"]["steps"][0]["state"], "ready");
    assert_eq!(resumed["plan"]["steps"][1]["state"], "pending");
    fs::write(file.path(), resumed["plan"].to_string()).unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def checkout():\n    return 3\n",
    )
    .unwrap();
    let changed = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    assert_eq!(
        changed["invalidated"],
        serde_json::json!(["diagnose", "consumer"])
    );
    assert_eq!(changed["plan"]["steps"][2]["state"], "ready");
    assert_eq!(changed["complete"], false);
}

#[test]
fn task_completion_requires_current_check_receipts_and_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def checkout():\n    return 1\n",
    )
    .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), plan().to_string()).unwrap();
    let started = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
            "--transition",
            "diagnose:start",
        ],
    );
    let mut document = started["plan"].clone();
    document["steps"][0]["evidence"] = serde_json::json!([{"id":"oracle","kind":"check", "passed":true, "input_digest":started["input_digests"]["diagnose"],"reference":"retained/oracle.json"}]);
    fs::write(file.path(), document.to_string()).unwrap();
    let satisfied = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
            "--transition",
            "diagnose:satisfy",
        ],
    );
    assert_eq!(satisfied["complete"], true, "{satisfied}");
    assert_eq!(satisfied["mutation_authority"], false);
    fs::write(file.path(), satisfied["plan"].to_string()).unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def checkout():\n    return 2\n",
    )
    .unwrap();
    let stale = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    assert_eq!(stale["complete"], false);
}

#[test]
fn task_admission_agrees_exhaustively_with_lean_kernel() {
    use fun_refactor::project::investigation::{transition_allowed, State};
    let built = Command::new("lake")
        .args(["build", "fr-investigation-kernel", "--wfail"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new("lake")
        .args(["exe", "fr-investigation-kernel"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(output.status.success());
    let states = [
        State::Pending,
        State::Ready,
        State::Running,
        State::Satisfied,
        State::Blocked,
        State::Stale,
    ];
    let mut expected = Vec::new();
    for from in states {
        for to in states {
            for prerequisites in [false, true] {
                for evidence in [false, true] {
                    expected
                        .push(transition_allowed(from, to, prerequisites, evidence).to_string());
                }
            }
        }
    }
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn correspondence_reports_moves_duplicates_and_deletion_without_rebinding() {
    let dir = tempfile::tempdir().unwrap();
    let source = "def target():\n    return 1\n";
    fs::write(dir.path().join("old.py"), source).unwrap();
    let identities = report(dir.path(), &["project", "identities"]);
    let retained = tempfile::NamedTempFile::new().unwrap();
    fs::write(retained.path(), identities.to_string()).unwrap();
    fs::rename(dir.path().join("old.py"), dir.path().join("new.py")).unwrap();
    let args = [
        "project",
        "identities",
        "--from",
        retained.path().to_str().unwrap(),
    ];
    let moved = report(dir.path(), &args);
    assert_eq!(moved["items"][0]["status"], "matched");
    assert_eq!(moved["items"][0]["action_rebound"], false);
    fs::write(dir.path().join("duplicate.py"), source).unwrap();
    assert_eq!(report(dir.path(), &args)["items"][0]["status"], "ambiguous");
    fs::remove_file(dir.path().join("new.py")).unwrap();
    fs::remove_file(dir.path().join("duplicate.py")).unwrap();
    assert_eq!(report(dir.path(), &args)["items"][0]["status"], "missing");
}
