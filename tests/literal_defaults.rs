use serde_json::Value;
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
    fs::write(
        root.join("app.py"),
        format!(
            "def choose({parameters}):\n    return left\ndef entry():\n    return sink({call})\n"
        ),
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
fn omitted_literals_and_explicit_values_have_distinct_origins() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call, witness) in [
        ("left=0", "choose()", false),
        ("left=0", "choose(source())", true),
        ("left=0, /, *, right=None", "choose(right=source())", false),
        ("left=-17", "choose()", false),
        ("left='café'", "choose()", false),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert_eq!(report["semantics"], "python-scalar-summaries");
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
        let first = &report["function_summaries"]["functions"]["choose"]["signature"][0];
        assert_eq!(first["name"], "left");
        assert_eq!(first["site"]["role"], "summary-parameter");
        assert_eq!(first["default"]["role"], "parameter-default");
        assert_eq!(first["default"]["revision"], report["revision"]);
    }
}

fn analyze_program(root: &Path, source: &str, imports: bool) -> Value {
    fs::write(root.join("app.py"), source).unwrap();
    fs::write(
        root.join("rules.json"),
        r#"{"version":"named-defaults","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    let found: Value =
        serde_json::from_slice(&run(root, &["project", "find", "entry"]).stdout).unwrap();
    let target = found["rows"][0][0].as_str().unwrap();
    let mut args = vec!["project", "dataflow", target, "--summaries", "--rules"];
    let rules = root.join("rules.json");
    args.extend([
        rules.to_str().unwrap(),
        "--steps",
        "4096",
        "--bytes",
        "1048576",
    ]);
    if imports {
        args.push("--imports");
    }
    let output = run(root, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn known_prior_defaults_defer_uncalled_module_bodies() {
    let root = tempfile::tempdir().unwrap();
    for declaration in [
        "marker = 0\n",
        "marker = []\n",
        "class marker:\n    pass\n",
        "class Token:\n    def __repr__(self):\n        return 'opaque'\nmarker = Token()\n",
    ] {
        let report = analyze_program(root.path(), &format!("{declaration}def unused(*items, key=((marker)), **options):\n    return opaque(items)\ndef entry():\n    return sink(source())\n"), false);
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert!(report["initialization"]["sites"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!report["witnesses"].as_array().unwrap().is_empty());
        assert!(report["function_summaries"]["functions"]
            .get("unused")
            .is_none());
    }
}

#[test]
fn named_defaults_preserve_definition_order_and_keep_calls_unchecked() {
    let root = tempfile::tempdir().unwrap();
    for (before, after, call) in [
        ("", "marker = 0\n", "sink(source())"),
        ("marker = 0\ndel marker\n", "", "sink(source())"),
        ("marker = source()\n", "", "sink(source())"),
        ("marker = 0\n", "", "unused()"),
        ("marker = 0\n", "", "unused(source())"),
    ] {
        let report = analyze_program(root.path(), &format!("{before}def unused(key=marker):\n    return key\n{after}def entry():\n    return {call}\n"), false);
        assert_eq!(report["complete"], false);
        if call.starts_with("unused") {
            assert!(report["initialization"]["sites"]
                .as_array()
                .unwrap()
                .is_empty());
            assert!(report["cutoffs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|cutoff| cutoff == "unsupported-parameter-contract:unused"));
        }
    }
}

#[test]
fn imported_default_values_are_available_only_after_the_import() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("helpers.py"),
        "__all__ = ['marker']\nmarker = 0\n",
    )
    .unwrap();
    for import in ["from helpers import marker\n", "from helpers import *\n"] {
        for before in [true, false] {
            let function = "def unused(key=marker):\n    return opaque(key)\n";
            let source = if before {
                format!("{import}{function}")
            } else {
                format!("{function}{import}")
            };
            let report = analyze_program(
                root.path(),
                &(source + "def entry():\n    return sink(source())\n"),
                true,
            );
            assert_eq!(report["complete"], before, "{}", report["cutoffs"]);
        }
    }
}

#[test]
fn definition_effects_and_required_slots_are_not_hidden_by_defaults() {
    let root = tempfile::tempdir().unwrap();
    for (parameters, call) in [
        ("left=source()", "choose(0)"),
        ("left=[]", "choose(0)"),
        ("left=0, *, right", "choose()"),
        ("left=0, /", "choose(left=source())"),
    ] {
        let (_, report) = analyze(root.path(), parameters, call);
        assert_eq!(report["complete"], false);
    }
}

#[test]
fn scalar_module_literals_do_not_hide_binding_or_initialization_effects() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("rules.json"),
        r#"{"version":"literals","sources":["source"],"sinks":["sink"]}"#,
    )
    .unwrap();
    for (binding, complete) in [
        ("metadata = -17", true),
        ("metadata = f'constant'", false),
        ("metadata: int = 0", false),
        ("metadata = 0\nmetadata = 1", false),
        ("metadata = other = 0", false),
        ("metadata = []", true),
        ("metadata = {}", true),
        ("metadata = (0,)", true),
        ("metadata = []\nmetadata.append(source())", false),
        ("metadata = source()", false),
    ] {
        fs::write(
            root.path().join("app.py"),
            format!("{binding}\ndef entry():\n    return sink(source())\n"),
        )
        .unwrap();
        let found: Value =
            serde_json::from_slice(&run(root.path(), &["project", "find", "entry"]).stdout)
                .unwrap();
        let target = found["rows"][0][0].as_str().unwrap();
        let output = run(
            root.path(),
            &[
                "project",
                "dataflow",
                target,
                "--summaries",
                "--rules",
                root.path().join("rules.json").to_str().unwrap(),
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
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            report["complete"], complete,
            "{binding}: {}",
            report["cutoffs"]
        );
        assert!(report["call_binding"]["initialization"].is_string());
    }
}

#[test]
fn literal_defaults_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/literal-defaults-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-10-14-value-imports-literal-defaults/result.json",
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
