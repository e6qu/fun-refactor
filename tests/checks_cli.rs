use serde_json::{json, Value};
use std::fs;
use std::process::Command;

fn check(name: &str, program: &str) -> Value {
    json!({"name": name, "argv": ["python3", "-c", program], "cwd": ".",
        "timeout_seconds": 5, "covers": ["fixture behavior"]})
}

fn fixture(checks: Vec<Value>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".fr")).unwrap();
    configure(&root, checks);
    root
}

fn configure(root: &tempfile::TempDir, checks: Vec<Value>) {
    fs::write(
        root.path().join(".fr/checks.json"),
        serde_json::to_vec(&json!({"schema": 1, "checks": checks})).unwrap(),
    )
    .unwrap();
}

fn run(root: &tempfile::TempDir, args: &[&str], code: i32) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "-C"])
        .arg(root.path())
        .arg("checks")
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("One JSON report, even on failure.")
}

fn basis(root: &tempfile::TempDir) -> String {
    run(root, &[], 0)["basis"].as_str().unwrap().to_owned()
}

#[test]
fn quiet_success_omits_success_text_but_retains_failed_diagnostics() {
    let root = fixture(vec![
        check("pass", "print('success')"),
        check("fail", "print('diagnostic'); raise SystemExit(3)"),
    ]);
    let report = run(
        &root,
        &[
            "--run",
            "pass,fail",
            "--basis",
            &basis(&root),
            "--quiet-success",
        ],
        1,
    );
    assert_eq!(report["results"][0]["stdout"]["text"], "");
    assert_eq!(report["results"][0]["stdout"]["omitted_bytes"], 8);
    assert_eq!(report["results"][1]["stdout"]["text"], "diagnostic\n");
    assert_eq!(report["results"][1]["exit_code"], 3);
    assert_eq!(report["passed"], false);
}

#[test]
fn omitted_declarations_rejoin_the_reviewed_listing_without_losing_outcomes() {
    let mut missing = check("missing", "");
    missing["argv"] = json!(["fr-fixture-executable-that-does-not-exist"]);
    let root = fixture(vec![
        check("pass", "print('success')"),
        check(
            "fail",
            "import os; os.write(2, b'\\xffbad'); raise SystemExit(7)\n\n",
        ),
        missing,
        check("unselected", "open('marker', 'w').write('ran')"),
    ]);
    let listing = run(&root, &[], 0);
    assert!(listing.get("declarations_omitted").is_none());
    for quiet in [false, true] {
        let mut args = vec![
            "--run",
            "missing,pass,fail",
            "--basis",
            listing["basis"].as_str().unwrap(),
            "--output-bytes",
            "3",
        ];
        if quiet {
            args.push("--quiet-success");
        }
        let mut full = run(&root, &args, 1);
        args.push("--no-declarations");
        let mut compact = run(&root, &args, 1);
        assert_eq!(compact["declarations_omitted"], true);
        assert!(compact.get("checks").is_none());
        assert_eq!(compact["basis"], listing["basis"]);
        assert_eq!(compact["not_run"], json!(["unselected"]));
        assert_eq!(compact["results"][0]["name"], "pass");
        assert_eq!(compact["results"][1]["exit_code"], 7);
        assert_eq!(compact["results"][1]["stderr"]["text"], "\u{fffd}ba");
        assert_eq!(compact["results"][1]["stderr"]["omitted_bytes"], 1);
        assert!(compact["results"][2]["error"].is_string());
        for (before, after) in full["results"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(compact["results"].as_array_mut().unwrap())
        {
            let declaration = listing["checks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|check| check["name"] == after["name"])
                .unwrap();
            for key in ["argv", "cwd", "covers"] {
                assert!(after.get(key).is_none());
                after[key] = declaration[key].clone();
            }
            assert!(after["elapsed_ms"].is_number());
            before.as_object_mut().unwrap().remove("elapsed_ms");
            after.as_object_mut().unwrap().remove("elapsed_ms");
        }
        compact["checks"] = listing["checks"].clone();
        compact
            .as_object_mut()
            .unwrap()
            .remove("declarations_omitted");
        assert_eq!(compact, full);
    }
    assert!(!root.path().join("marker").exists());
}

#[test]
fn declaration_omission_requires_execution_and_a_current_review() {
    let root = fixture(vec![check("unit", "open('marker', 'w').write('ran')")]);
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args([
            "-C",
            root.path().to_str().unwrap(),
            "checks",
            "--no-declarations",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let old = basis(&root);
    for names in ["unit", "unit,absent", "unit,unit"] {
        let mut args = vec!["--run", names, "--no-declarations"];
        if names != "unit" {
            args.extend(["--basis", &old]);
        }
        run(&root, &args, 1);
    }
    configure(
        &root,
        vec![check("unit", "open('marker', 'w').write('changed')")],
    );
    run(
        &root,
        &["--run", "unit", "--basis", &old, "--no-declarations"],
        1,
    );
    assert!(!root.path().join("marker").exists());
}

#[test]
fn listing_never_executes_and_selection_reports_declared_coverage() {
    let root = fixture(vec![
        check("unit", "print('unit passed')"),
        check("side_effect", "open('marker', 'w').write('ran')"),
    ]);
    let preview = run(&root, &[], 0);
    assert_eq!(preview["executed"], false);
    assert_eq!(preview["passed"], Value::Null);
    assert!(!root.path().join("marker").exists());
    let result = run(
        &root,
        &[
            "--run",
            "unit",
            "--basis",
            preview["basis"].as_str().unwrap(),
        ],
        0,
    );
    assert_eq!(result["passed"], true);
    assert_eq!(result["not_run"], json!(["side_effect"]));
    assert_eq!(result["results"][0]["covers"], json!(["fixture behavior"]));
    assert!(!root.path().join("marker").exists());
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn reviewed_basis_accepts_only_cryptographically_strong_prefixes() {
    let root = fixture(vec![check("unit", "print('passed')")]);
    let full = basis(&root);
    assert_eq!(full.len(), 64);
    let prefix = &full[..32];
    assert_eq!(
        run(&root, &["--run", "unit", "--basis", prefix], 0)["passed"],
        true
    );
    run(&root, &["--run", "unit", "--basis", &full[..31]], 1);
    let mut wrong = prefix.to_owned();
    wrong.replace_range(..1, if &prefix[..1] == "0" { "1" } else { "0" });
    run(&root, &["--run", "unit", "--basis", &wrong], 1);
}

#[test]
fn stale_missing_unknown_and_duplicate_selection_refuse_before_any_execution() {
    let command = check("unit", "open('marker', 'w').write('ran')");
    let root = fixture(vec![command.clone()]);
    let old = basis(&root);
    run(&root, &["--run", "unit"], 1);
    run(&root, &["--run", "unit,absent", "--basis", &old], 1);
    run(&root, &["--run", "unit,unit", "--basis", &old], 1);
    let mut changed = command;
    changed["covers"] = json!(["changed declaration"]);
    configure(&root, vec![changed]);
    run(&root, &["--run", "unit", "--basis", &old], 1);
    assert!(!root.path().join("marker").exists());
}

#[test]
fn reports_failure_spawn_error_and_later_success_without_claiming_coverage() {
    let mut missing = check("missing", "");
    missing["argv"] = json!(["fr-fixture-executable-that-does-not-exist"]);
    let root = fixture(vec![
        check("fail", "raise SystemExit(7)"),
        missing,
        check("pass", "print('ok')"),
    ]);
    let report = run(
        &root,
        &["--run", "fail,missing,pass", "--basis", &basis(&root)],
        1,
    );
    assert_eq!(report["passed"], false);
    assert_eq!(report["results"][0]["exit_code"], 7);
    assert!(report["results"][1]["error"].is_string());
    assert_eq!(report["results"][2]["passed"], true);
    assert_eq!(report["source_snapshot_checked"], false);
}

#[test]
fn captures_both_streams_with_exact_omissions_and_no_pipe_deadlock() {
    let root = fixture(vec![check(
        "loud",
        "import os; os.write(1, b'a'*200000); os.write(2, b'b'*200000)",
    )]);
    let report = run(
        &root,
        &[
            "--run",
            "loud",
            "--basis",
            &basis(&root),
            "--output-bytes",
            "17",
        ],
        0,
    );
    for stream in ["stdout", "stderr"] {
        assert_eq!(report["results"][0][stream]["retained_bytes"], 17);
        assert_eq!(report["results"][0][stream]["omitted_bytes"], 199983);
    }
    let report = run(
        &root,
        &[
            "--run",
            "loud",
            "--basis",
            &basis(&root),
            "--output-bytes",
            "0",
        ],
        0,
    );
    assert_eq!(report["results"][0]["stdout"]["omitted_bytes"], 200000);
    run(&root, &["--output-bytes", "65537"], 1);
}

#[test]
fn timeout_reports_failure_and_returns_without_waiting_for_inherited_pipes() {
    let mut slow = check(
        "slow",
        "import time; print('started', flush=True); time.sleep(10)",
    );
    slow["timeout_seconds"] = json!(1);
    let root = fixture(vec![slow]);
    let started = std::time::Instant::now();
    let report = run(&root, &["--run", "slow", "--basis", &basis(&root)], 1);
    assert_eq!(report["results"][0]["timed_out"], true);
    assert!(started.elapsed().as_secs() < 8);
}

#[test]
fn oversized_capture_fails_even_when_the_child_exits_successfully() {
    let root = fixture(vec![check(
        "excess",
        "import os; os.write(1, b'\\xff' * (17 * 1024 * 1024))\n\n",
    )]);
    let report = run(
        &root,
        &[
            "--run",
            "excess",
            "--basis",
            &basis(&root),
            "--output-bytes",
            "1",
        ],
        1,
    );
    assert_eq!(report["results"][0]["passed"], false);
    assert_eq!(report["results"][0]["output_limit_exceeded"], true);
    assert_eq!(report["results"][0]["stdout"]["retained_bytes"], 1);
    assert_eq!(report["results"][0]["stdout"]["text"], "\u{fffd}");
}

#[test]
fn argv_is_literal_and_cwd_is_the_declared_directory() {
    let mut literal = check(
        "literal",
        "import sys; open('marker', 'w').write(sys.argv[1])",
    );
    literal["argv"]
        .as_array_mut()
        .unwrap()
        .push(json!("$(touch injected); `touch injected`"));
    literal["cwd"] = json!("sub");
    let root = fixture(vec![literal]);
    fs::create_dir(root.path().join("sub")).unwrap();
    run(&root, &["--run", "literal", "--basis", &basis(&root)], 0);
    assert_eq!(
        fs::read_to_string(root.path().join("sub/marker")).unwrap(),
        "$(touch injected); `touch injected`"
    );
    assert!(!root.path().join("sub/injected").exists());
}

#[test]
fn refuses_malformed_oversized_and_outside_configurations() {
    let root = fixture(vec![check("unit", "")]);
    let path = root.path().join(".fr/checks.json");
    for data in [b"{".to_vec(), vec![b' '; 65537]] {
        fs::write(&path, data).unwrap();
        run(&root, &[], 1);
    }
    let mut outside = check("unit", "");
    outside["cwd"] = json!("..");
    configure(&root, vec![outside]);
    run(&root, &[], 1);
    configure(&root, vec![check("unit", ""), check("unit", "")]);
    run(&root, &[], 1);
    let mut unknown = check("unit", "");
    unknown["env"] = json!({});
    configure(&root, vec![unknown]);
    run(&root, &[], 1);
}

#[cfg(unix)]
#[test]
fn refuses_symlink_configuration_and_working_directory() {
    use std::os::unix::fs::symlink;
    let root = fixture(vec![check("unit", "")]);
    let path = root.path().join(".fr/checks.json");
    fs::rename(&path, root.path().join("config.json")).unwrap();
    symlink("../config.json", &path).unwrap();
    run(&root, &[], 1);
    fs::remove_file(&path).unwrap();
    symlink(".", root.path().join("linked")).unwrap();
    let mut linked = check("unit", "");
    linked["cwd"] = json!("linked");
    configure(&root, vec![linked]);
    run(&root, &[], 1);
}
