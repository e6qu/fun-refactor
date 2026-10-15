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
fn named_defaults_preserve_definition_order_and_keep_shared_values_unchecked() {
    let root = tempfile::tempdir().unwrap();
    for (before, after, call) in [
        ("", "marker = 0\n", "sink(source())"),
        ("marker = 0\ndel marker\n", "", "sink(source())"),
        ("marker = source()\n", "", "sink(source())"),
        ("marker = []\n", "", "unused()"),
        ("marker = []\n", "", "unused(source())"),
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
fn local_scalar_defaults_bind_calls_and_preserve_definition_time_capture() {
    let root = tempfile::tempdir().unwrap();
    for (literal, call, witness) in [
        ("0", "choose()", false),
        ("None", "choose(source())", true),
        ("b'bytes'", "choose()", false),
        ("1j", "choose()", false),
        ("'café'", "choose(value=source())", true),
    ] {
        let report = analyze_program(root.path(), &format!("marker = {literal}\ndef choose(value=((marker))):\n    return value\ndef entry():\n    return sink({call})\n"), false);
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), witness);
    }
    let report = analyze_program(root.path(), "marker = 0\ndef choose(value=marker):\n    return value\nmarker = []\ndef entry():\n    return sink(choose())\n", false);
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|value| value != "unsupported-parameter-contract:choose"));
}

#[test]
fn imported_function_uses_its_own_local_scalar_default() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("helpers.py"),
        "marker = 0\ndef choose(value=marker):\n    return value\n",
    )
    .unwrap();
    let report = analyze_program(
        root.path(),
        "from helpers import choose\ndef entry():\n    return sink(choose())\n",
        true,
    );
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(
        report["function_summaries"]["functions"]["helpers.py::choose"]["signature"][0]["default"]
            ["path"],
        "helpers.py"
    );
}

#[test]
fn builtin_type_registries_preserve_unused_marker_defaults() {
    let root = tempfile::tempdir().unwrap();
    for value in [
        "{type(object.__dict__): dict, type({}.keys()): list, type({}.values()): list, type({}.items()): list}",
        "{int: str, bool: bytes}",
        "{type(None): object}",
        "{type({(0,): [None]}): dict}",
    ] {
        for imports in [false, true] {
            let report = analyze_program(root.path(), &format!("class Token:\n    pass\nmarker = Token()\nregistry = {value}\ndef unused(key=marker):\n    return opaque(key)\ndef entry():\n    return sink(source())\n"), imports);
            assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
            assert!(report["initialization"]["sites"].as_array().unwrap().is_empty());
        }
    }
}

#[test]
fn builtin_type_registries_keep_shadowing_and_callbacks_unchecked() {
    let root = tempfile::tempdir().unwrap();
    for source in [
        "type = source\nregistry = {type({}): list}\n",
        "list = source\nregistry = {type({}): list}\n",
        "class object:\n    pass\nregistry = {type(object.__dict__): dict}\n",
        "registry = {type(source()): list}\n",
        "registry = {type({}.keys(source())): list}\n",
        "registry = {type(1 + 2): int}\n",
        "registry = {type({}): source}\n",
        "def unused(__builtins__):\n    pass\nregistry = {type({}): list}\n",
    ] {
        let report = analyze_program(
            root.path(),
            &(source.to_owned() + "def entry():\n    return sink(source())\n"),
            false,
        );
        assert_eq!(report["complete"], false, "{source}");
        assert!(!report["initialization"]["sites"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn imported_registry_retains_its_distinct_value_kind() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("helpers.py"),
        "registry = {type({}.keys()): list}\n",
    )
    .unwrap();
    let report = analyze_program(
        root.path(),
        "from helpers import registry\ndef entry():\n    return sink(source())\n",
        true,
    );
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    let binding = &report["inputs"]["modules"]["value_bindings"][0];
    assert_eq!(binding["path"], "helpers.py");
    assert_eq!(binding["name"], "registry");
    assert_eq!(binding["kind"], "registry");
    assert_eq!(binding["offset"], 0);
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
fn builtin_bases_defer_unused_constructor_bodies() {
    let root = tempfile::tempdir().unwrap();
    for base in ["object", "dict", "Exception", "((dict))"] {
        let declaration =
            format!("class Token({base}):\n    def __init__(self):\n        sink(source())\n");
        for imports in [false, true] {
            let prelude = if imports {
                fs::write(root.path().join("helpers.py"), &declaration).unwrap();
                "from helpers import Token\n".to_owned()
            } else {
                declaration.clone()
            };
            let report = analyze_program(
                root.path(),
                &format!("{prelude}def entry():\n    return sink(source())\n"),
                imports,
            );
            assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
            assert!(report["initialization"]["sites"]
                .as_array()
                .unwrap()
                .is_empty());
            if imports {
                assert_eq!(
                    report["inputs"]["modules"]["value_bindings"][0]["kind"],
                    "class"
                );
            }
        }
    }
}

#[test]
fn builtin_base_names_cannot_hide_custom_class_creation() {
    let root = tempfile::tempdir().unwrap();
    for base in ["object", "dict", "Exception"] {
        let source = format!("class {base}:\n    def __init_subclass__(cls):\n        sink(source())\nclass Token({base}):\n    pass\ndef entry():\n    return sink(0)\n");
        let report = analyze_program(root.path(), &source, false);
        assert_eq!(report["complete"], false);
        assert!(report["initialization"]["sites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|site| { site["occurrence"]["location"]["range"]["start"]["line"] == 4 }));
    }
}

#[test]
fn inherited_dict_and_exception_construction_stays_unchecked() {
    let root = tempfile::tempdir().unwrap();
    for (base, complete) in [("object", true), ("dict", false), ("Exception", false)] {
        let source = format!("class Token({base}):\n    pass\nmarker = Token()\ndef entry():\n    return sink(source())\n");
        let report = analyze_program(root.path(), &source, false);
        assert_eq!(report["complete"], complete, "{}", report["cutoffs"]);
    }
}

#[test]
fn literal_defaults_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/literal-defaults-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-10-15-type-registries-literal-defaults/result.json",
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
