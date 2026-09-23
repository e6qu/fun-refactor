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
    for expected in ["unknown-external", "alias-or", "recursion"] {
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

#[test]
fn negative_lookups_configuration_and_analyzer_versions_invalidate() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def caller():\n    return missing()\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = 'subject'\nversion = '1'\n",
    )
    .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    let document = serde_json::json!({"schema":"fr-investigation-plan-1", "goal":"resolve", "acceptance":["checked"], "steps":[
        {"id":"lookup", "question":"missing?", "inputs":[{"kind":"lookup","key":"missing","digest":null}]},
        {"id":"config", "question":"config?", "inputs":[{"kind":"configuration","key":"pyproject.toml","digest":null}]},
        {"id":"version", "question":"version?", "inputs":[{"kind":"analyzer","key":"native","digest":null}]}
    ]});
    fs::write(file.path(), document.to_string()).unwrap();
    let initial = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    let mut retained = initial["plan"].clone();
    retained["steps"][2]["inputs"][0]["digest"] = serde_json::json!("old-analyzer");
    fs::write(file.path(), retained.to_string()).unwrap();
    fs::write(
        dir.path().join("added.py"),
        "def missing():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = 'subject'\nversion = '2'\n",
    )
    .unwrap();
    let resumed = report(
        dir.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    assert_eq!(
        resumed["invalidated"],
        serde_json::json!(["lookup", "config", "version"])
    );
}

#[test]
fn unknown_target_bug_and_feature_pass_review_reversal_and_patch_replay() {
    let result = tempfile::tempdir().unwrap();
    let output = Command::new("python3")
        .args([
            "tools/investigation-acceptance.py",
            "--fr",
            env!("CARGO_BIN_EXE_fr"),
            "--output",
        ])
        .arg(result.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value =
        serde_json::from_slice(&fs::read(result.path().join("result.json")).unwrap()).unwrap();
    assert_eq!(value["receiver_oracle_passed"], true);
}

#[test]
fn python_insertion_refuses_collisions_and_non_function_fragments() {
    for (source, fragment) in [
        ("quote = 1\n", "def quote():\n    return 2\n"),
        (
            "from other import thing as quote\n",
            "def quote():\n    return 2\n",
        ),
        ("from other import *\n", "def quote():\n    return 2\n"),
        ("def existing():\n    pass\n", "value = 2\n"),
        (
            "def existing():\n    pass\n",
            "def one():\n    pass\ndef two():\n    pass\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("subject.py"), source).unwrap();
        let found = report(
            dir.path(),
            &[
                "project",
                "map",
                "subject.py",
                "--depth",
                "0",
                "--fields",
                "handle",
            ],
        );
        let fragment_file = tempfile::NamedTempFile::new().unwrap();
        fs::write(fragment_file.path(), fragment).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "-C"])
            .arg(dir.path())
            .args([
                "author",
                "insert-declaration",
                found["rows"][0][0].as_str().unwrap(),
                "--from",
            ])
            .arg(fragment_file.path())
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "refusal expected: {source} {fragment}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("subject.py")).unwrap(),
            source
        );
    }
}

#[test]
fn cycles_missing_evidence_and_changed_acceptance_cannot_complete() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def checkout():\n    return 1\n",
    )
    .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut document = plan();
    document["steps"][0]["depends_on"] = serde_json::json!(["consumer"]);
    fs::write(file.path(), document.to_string()).unwrap();
    let command = |transition: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fr"));
        command
            .args(["--json", "-C"])
            .arg(dir.path())
            .args(["project", "investigate", "--from"])
            .arg(file.path());
        if let Some(transition) = transition {
            command.args(["--transition", transition]);
        }
        command.output().unwrap()
    };
    assert!(!command(None).status.success());
    fs::write(file.path(), plan().to_string()).unwrap();
    assert!(!command(Some("diagnose:satisfy")).status.success());
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
    let mut changed = started["plan"].clone();
    changed["steps"][0]["evidence"] = serde_json::json!([{"id":"oracle","kind":"check", "passed":true, "input_digest":started["input_digests"]["diagnose"],"reference":"oracle.json"}]);
    changed["steps"][0]["question"] = serde_json::json!("a different property");
    fs::write(file.path(), changed.to_string()).unwrap();
    assert!(!command(Some("diagnose:satisfy")).status.success());
}

#[test]
fn flow_response_budget_reports_omission_and_shadowed_calls_stay_unknown() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("subject.py"),
        "def helper(x):\n    return x\ndef run(helper, x):\n    return helper(x)\n",
    )
    .unwrap();
    let handle = function_handle(dir.path(), "run");
    let result = report(dir.path(), &["project", "dataflow", &handle]);
    assert_eq!(result["complete"], false);
    assert!(result["cutoffs"].to_string().contains("ambiguous-call"));
    let mut source = String::from("def long(x):\n");
    for _ in 0..60 {
        source.push_str("    x = x + 1\n");
    }
    source.push_str("    return x\n");
    fs::write(dir.path().join("long.py"), source).unwrap();
    let handle = function_handle(dir.path(), "long");
    let result = report(
        dir.path(),
        &[
            "project", "dataflow", &handle, "--bytes", "4096", "--steps", "512",
        ],
    );
    assert_eq!(result["complete"], false);
    assert!(result["cutoffs"].to_string().contains("response-budget"));
    assert!(serde_json::to_vec(&result).unwrap().len() < 4096);
}
