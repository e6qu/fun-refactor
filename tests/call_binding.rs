use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn analyze(root: &Path, parameters: &str, call: &str) -> (String, Value) {
    analyze_body(root, parameters, "return left", call)
}

fn analyze_body(root: &Path, parameters: &str, body: &str, call: &str) -> (String, Value) {
    fs::write(
        root.join("app.py"),
        format!("def choose({parameters}):\n    {body}\ndef entry():\n    return sink({call})\n"),
    )
    .unwrap();
    fs::write(
        root.join("rules.json"),
        r#"{"version":"calls-1","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let found: Value =
        serde_json::from_slice(&run(root, &["project", "find", "entry"]).stdout).unwrap();
    let target = found["rows"][0][0].as_str().unwrap().to_owned();
    let output = run(
        root,
        &[
            "project",
            "dataflow",
            &target,
            "--summaries",
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--steps",
            "4096",
            "--bytes",
            "1048576",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (target, serde_json::from_slice(&output.stdout).unwrap())
}

#[test]
fn uncalled_variadic_bodies_defer_effects_but_calls_keep_their_contract() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, body) in [
        ("*items", "return items[0]"),
        ("**options", "return options['value']"),
        (
            "value, /, *items, flag=True, **options",
            "return opaque(items, options)",
        ),
        ("*items", "yield items"),
    ] {
        let (_, report) = analyze_body(root.path(), parameters, body, "source()");
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert!(!report["witnesses"].as_array().unwrap().is_empty());
        assert!(report["function_summaries"]["functions"]["choose"].is_null());
    }
    let (_, called) = analyze_body(root.path(), "*items", "return items[0]", "choose(source())");
    assert_eq!(called["complete"], false);
    assert!(called["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("unsupported-parameter-contract:choose")));
    assert!(!called["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("module-effects-unchecked")));
}

#[test]
fn generator_calls_never_transfer_their_deferred_body() {
    let root = tempfile::tempdir().unwrap();
    for body in [
        "yield sink(left)",
        "return False and (yield sink(left))",
        "return False and (lambda ignored=(yield sink(left)): 0)",
    ] {
        let (_, report) = analyze_body(root.path(), "left", body, "choose(source())");
        assert_eq!(report["complete"], false);
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("generator-function")));
        assert!(report["witnesses"].as_array().unwrap().is_empty());
    }
    let (_, report) = analyze_body(
        root.path(),
        "left",
        "return 0 if True else (lambda: (yield left))",
        "choose(source())",
    );
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert!(report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn unused_variadic_declarations_accept_only_fixed_slot_calls() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call, complete, witness) in [
        ("left, *extra", "choose(source())", true, true),
        ("left=0, *extra", "choose()", true, false),
        ("left, /, *extra", "choose(source())", true, true),
        (
            "left, *extra, right=0",
            "choose(0, right=source())",
            true,
            false,
        ),
        (
            "left, *extra, right",
            "choose(source(), right=0)",
            true,
            true,
        ),
        ("left, **extra", "choose(left=source())", true, true),
        (
            "left, *extra, **others",
            "choose(left=source())",
            true,
            true,
        ),
        ("left, *extra", "choose(0, source())", false, false),
        ("left, **extra", "choose(0, extra=source())", false, false),
        ("left, *extra", "choose(0, sink(source()))", false, true),
        ("left, *extra, right", "choose(source())", false, false),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(
            report["complete"], complete,
            "{parameters}: {call}: {}",
            report["cutoffs"]
        );
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
        assert_eq!(report["call_binding"]["schema"], "fr-call-binding");
    }
    for (parameters, body) in [
        ("left, *extra", "return extra"),
        ("left, **extra", "return extra"),
        ("left, *extra", "extra = 0; return left"),
        ("left, *extra: int", "return left"),
        ("left, **extra: int", "return left"),
    ] {
        let (_, report) = analyze_body(root.path(), parameters, body, "choose(source())");
        assert_eq!(report["complete"], false, "{parameters}: {body}");
        assert!(report["cutoffs"].as_array().unwrap().iter().any(|c| {
            c.as_str()
                .unwrap()
                .starts_with("unsupported-parameter-contract:")
        }));
    }
}

#[test]
fn required_parameters_bind_by_kind_and_name() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call, complete, witness) in [
        ("left, right", "choose(right=0, left=source())", true, true),
        ("left, right", "choose(left=0, right=source())", true, false),
        ("left, /, *, right", "choose(source(), right=0)", true, true),
        (
            "*, left, right",
            "choose(right=0, left=source())",
            true,
            true,
        ),
        (
            "left, /, right",
            "choose(left=source(), right=0)",
            false,
            false,
        ),
        ("left, *, right", "choose(source(), 0)", false, false),
        (
            "left, right",
            "choose(source(), left=0, right=0)",
            false,
            false,
        ),
        ("left, right", "choose(sink(source()))", false, true),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(
            report["complete"], complete,
            "{parameters}: {call}: {}",
            report["cutoffs"]
        );
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
        assert_eq!(report["semantics"], "python-scalar-summaries");
        assert_eq!(report["call_binding"], report["inputs"]["call_binding"]);
    }
}

#[test]
fn retained_call_contract_cannot_be_forged_with_a_new_digest() {
    let root = tempfile::tempdir().unwrap();
    let (target, original) = analyze(root.path(), "left", "choose(left=source())");
    for field in ["semantics", "call_binding"] {
        let mut report = original.clone();
        report[field] = if field == "semantics" {
            json!("python-scalar-summaries-2")
        } else {
            json!({"schema":"fr-call-binding-1", "implicit_exceptions":true})
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        let digest = hex::encode(Sha256::digest(&bytes));
        let retained = tempfile::NamedTempFile::new().unwrap();
        fs::write(retained.path(), &bytes).unwrap();
        let output = run(
            root.path(),
            &[
                "project",
                "dataflow",
                &target,
                "--summaries",
                "--rules",
                root.path().join("rules.json").to_str().unwrap(),
                "--steps",
                "4096",
                "--bytes",
                "1048576",
                "--reuse",
                retained.path().to_str().unwrap(),
                "--reuse-digest",
                &digest,
            ],
        );
        assert!(!output.status.success(), "forged {field} admitted");
        assert!(String::from_utf8_lossy(&output.stderr).contains("inconsistent"));
    }
}

#[test]
fn call_binding_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/call-binding-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-10-13-builtin-bindings-call-binding/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
