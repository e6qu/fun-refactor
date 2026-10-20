#[path = "common/retained.rs"]
mod retained;

use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn project(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .arg("project")
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

fn analyze(root: &Path, name: &str) -> Value {
    write(
        root,
        "rules.json",
        &json!({"version":"package-tests-1","sources":["source"],"sinks":["sink"]}).to_string(),
    );
    let found = project(root, &["find", name]);
    project(
        root,
        &[
            "dataflow",
            found["rows"][0][0].as_str().unwrap(),
            "--summaries",
            "--imports",
            "--rules",
            root.join("rules.json").to_str().unwrap(),
            "--steps",
            "4096",
            "--bytes",
            "1048576",
        ],
    )
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agent-eval/package-flow");
    for path in [
        "app.py",
        "portal/__init__.py",
        "portal/api.py",
        "portal/transform.py",
        "archive/__init__.py",
        "archive/transform.py",
    ] {
        write(
            root.path(),
            path,
            &fs::read_to_string(base.join(path)).unwrap(),
        );
    }
    root
}

#[test]
fn function_reference_defaults_preserve_imported_helpers_without_calling_callbacks() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "callbacks.py",
        "def callback(value):\n    return unknown(value)\n",
    );
    write(
        root.path(),
        "bridge.py",
        "from callbacks import callback as selected\n",
    );
    write(
        root.path(),
        "helpers.py",
        "from bridge import selected\ndef unused(value, factory=dict, follow=selected):\n    return follow(value)\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(report["initialization"]["by_file"], json!({}));
    assert!(report["inputs"]["modules"]["files"]
        .get("callbacks.py")
        .is_some());
    write(root.path(), "helpers.py", "from bridge import selected\ndef unused(follow=selected(0)):\n    return follow\ndef relay(value):\n    return value\n");
    let called = analyze(root.path(), "entry");
    assert_eq!(called["complete"], false);
    assert!(called["initialization"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .any(|site| site["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("definition-time-code"))));
}

#[test]
fn builtin_defaults_do_not_guess_missing_local_imports_or_earlier_definitions() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers.py",
        "def selected(value):\n    return value\n",
    );
    for setup in [
        "def unused(follow=selected):\n    pass\nfrom helpers import selected\n",
        "from helpers import absent as selected\ndef unused(follow=selected):\n    pass\n",
        "del dict\ndef unused(factory=dict):\n    pass\n",
    ] {
        write(
            root.path(),
            "app.py",
            &format!("{setup}def entry():\n    return sink(source())\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false, "{setup}");
        assert!(
            report["initialization"]["sites"]
                .as_array()
                .unwrap()
                .iter()
                .any(|site| site["reasons"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("definition-time-code"))),
            "{setup}"
        );
    }
}

#[test]
fn startup_registration_checks_writes_and_rejects_helper_replacement() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    for (body, complete) in [
        ("table.update(send=lambda: missing())", true),
        ("table['label'] = 0\n    return None", true),
        ("table['label'] = source()", false),
        ("global relay\n    relay = stopped", false),
        ("register()", false),
    ] {
        write(root.path(), "helpers.py", &format!("table = {{}}\ndef relay(value):\n    return value\ndef stopped(value):\n    return 0\ndef register():\n    {body}\nregister()\n"));
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], complete, "{body}: {report}");
        assert_eq!(
            report["initialization"]["by_file"]
                .as_object()
                .unwrap()
                .is_empty(),
            complete
        );
    }
}

#[test]
fn registry_allocations_defer_callbacks_but_not_computed_population_values() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.py",
        "from helpers import table, relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    for (setup, complete) in [
        (
            "table = dict(send=lambda value: unknown(value))\ntable.update(label='constant')",
            true,
        ),
        (
            "table = {'send': lambda value: unknown(value)}\ntable['label'] = 0",
            true,
        ),
        ("table = dict(send=source())", false),
        ("table = {}\ntable.update(send=source())", false),
        ("table = {}\ntable['send'] = source()", false),
        ("table = {}\nexpose(table)\ntable['label'] = 0", false),
    ] {
        write(
            root.path(),
            "helpers.py",
            &format!("{setup}\ndef relay(value):\n    return value\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(
            report["complete"], complete,
            "{setup}: {}",
            report["cutoffs"]
        );
        assert!(!report["witnesses"].as_array().unwrap().is_empty());
        assert_eq!(
            report["initialization"]["sites"]
                .as_array()
                .unwrap()
                .is_empty(),
            complete
        );
    }
}

#[test]
fn allocated_package_value_keeps_precedence_over_same_named_child_file() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers/__init__.py",
        "table = dict(send=lambda value: value)\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "helpers/table.py",
        "def relay(value):\n    return 0\n",
    );
    write(
        root.path(),
        "app.py",
        "from helpers import table, relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(
        report["inputs"]["modules"]["value_bindings"][0]["kind"],
        "allocation"
    );
    assert!(report["inputs"]["modules"]["files"]
        .get("helpers/table.py")
        .is_none());
}

#[test]
fn unused_value_imports_preserve_helpers_and_keep_value_calls_unchecked() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers.py",
        "class Token:\n    pass\nmarker = Token()\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "bridge.py",
        "from helpers import marker, relay\n",
    );
    write(
        root.path(),
        "app.py",
        "from bridge import marker, relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["inputs"]["modules"]["value_bindings"],
        json!([{"path":"helpers.py","name":"marker","kind":"instance","offset":22}])
    );
    write(
        root.path(),
        "app.py",
        "from bridge import marker, relay\ndef entry():\n    return sink(marker(source()))\n",
    );
    let called = analyze(root.path(), "entry");
    assert_eq!(called["complete"], false);
    assert_eq!(called["inputs"]["modules"]["complete"], true);
}

#[test]
fn function_documentation_preserves_calls_without_hiding_other_writes() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    for (returned, witnesses) in [("value", 1), ("0", 0)] {
        write(root.path(), "helpers.py", &format!("def relay(value):\n    return {returned}\nrelay.__doc__ = \"helper\"\nrelay.__doc__ = None\n"));
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
        assert_eq!(report["witnesses"].as_array().unwrap().len(), witnesses);
        assert_eq!(report["initialization"]["sites"], json!([]));
    }
    for statement in [
        "relay.__doc__ = source()",
        "relay.__defaults__ = (0,)",
        "relay = 0\nrelay.__doc__ = \"helper\"",
        "public = relay\npublic.__doc__ = \"helper\"",
    ] {
        write(
            root.path(),
            "helpers.py",
            &format!("def relay(value):\n    return value\n{statement}\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false);
        assert!(!report["initialization"]["sites"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn unused_builtin_references_preserve_helpers_without_admitting_calls_or_shadows() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers.py",
        "sorted = sorted\nmap = map\nfilter = filter\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(report["initialization"]["sites"], json!([]));
    assert_eq!(report["inputs"]["modules"]["function_aliases"], json!([]));
    write(
        root.path(),
        "helpers.py",
        "sorted = 0\npublic = sorted\ndef relay(value):\n    return 0\n",
    );
    let shadowed = analyze(root.path(), "entry");
    assert_eq!(shadowed["complete"], true, "{}", shadowed["cutoffs"]);
    assert_eq!(shadowed["witnesses"], json!([]));
    assert_eq!(shadowed["initialization"]["sites"], json!([]));
    assert_eq!(shadowed["inputs"]["modules"]["function_aliases"], json!([]));
    let aliases = shadowed["inputs"]["modules"]["value_aliases"]
        .as_array()
        .unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0]["name"], "public");
    assert_eq!(aliases[0]["target"], "sorted");
    write(
        root.path(),
        "helpers.py",
        "public = abs\ndef relay(value):\n    return public(value)\n",
    );
    let called = analyze(root.path(), "entry");
    assert_eq!(called["complete"], false);
    assert_eq!(called["initialization"]["sites"], json!([]));
}

#[test]
fn function_aliases_follow_original_helpers_and_keep_rebinding_unchecked() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers.py",
        "def relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "library/__init__.py",
        "from helpers import relay\npublic = relay\n",
    );
    write(
        root.path(),
        "app.py",
        "from library import public\ndef entry():\n    return sink(public(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(report["initialization"]["sites"], json!([]));
    assert!(report["function_summaries"]["functions"]
        .get("helpers.py::relay")
        .is_some());
    write(
        root.path(),
        "helpers.py",
        "def relay(value):\n    return 0\n",
    );
    let blocked = analyze(root.path(), "entry");
    assert_eq!(blocked["complete"], true, "{}", blocked["cutoffs"]);
    assert_eq!(blocked["witnesses"], json!([]));
    write(
        root.path(),
        "library/__init__.py",
        "from helpers import relay\npublic = relay\npublic = 0\n",
    );
    let changed = analyze(root.path(), "entry");
    assert_eq!(changed["complete"], false);
    assert!(!changed["initialization"]["sites"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn literal_collection_initialization_keeps_values_opaque_and_mutations_unchecked() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "library/__init__.py",
        "metadata = ({}, [0], ('module',), {1})\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from library import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    write(
        root.path(),
        "library/__init__.py",
        "metadata = []\nmetadata.append(source())\ndef relay(value):\n    return value\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["initialization"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .any(|site| site["occurrence"]["path"] == "library/__init__.py"));
}

#[test]
fn literal_package_metadata_does_not_hide_initialization_effects() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "library/__init__.py",
        "__version__ = '8.1.0'\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from library import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    write(
        root.path(),
        "library/__init__.py",
        "__version__ = source()\ndef relay(value):\n    return value\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["initialization"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .any(|site| site["occurrence"]["path"] == "library/__init__.py"));
}

#[test]
fn postponed_annotations_retain_negative_import_candidates_and_check_local_shadows() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "helpers.py", "from __future__ import annotations\ndef relay(value: Missing, /, *, flag: Missing = 0) -> source():\n    return value\n");
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{}", report["cutoffs"]);
    assert_eq!(report["witnesses"].as_array().unwrap().len(), 1);
    let lookup = report["inputs"]["modules"]["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["future"] == true)
        .unwrap();
    assert_eq!(lookup["resolution"], "future");
    assert_eq!(lookup["importer"], "helpers.py");
    assert_eq!(lookup["candidates"].as_object().unwrap().len(), 5);
    assert!(lookup["candidates"]
        .as_object()
        .unwrap()
        .values()
        .all(|value| value["status"] == "missing"));
    write(
        root.path(),
        "__future__.py",
        "annotations = 0\nsink(source())\n",
    );
    let shadowed = analyze(root.path(), "entry");
    assert_eq!(shadowed["complete"], false);
    assert!(shadowed["inputs"]["modules"]["files"]
        .get("__future__.py")
        .is_some());
    assert!(shadowed["initialization"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .any(|site| site["occurrence"]["path"] == "__future__.py"));
}

#[test]
fn unchecked_initialization_points_to_the_original_statement_and_clears_after_repair() {
    let root = tempfile::tempdir().unwrap();
    let helper = "metadata = source()\ndef relay(value):\n    return value\n";
    write(root.path(), "helpers.py", helper);
    write(
        root.path(),
        "app.py",
        "from helpers import relay\ndef entry():\n    return sink(relay(0))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["witnesses"].as_array().unwrap().is_empty());
    assert_eq!(report["initialization"]["omitted"], 0);
    let sites = report["initialization"]["sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0]["reasons"], json!(["startup-call"]));
    assert_eq!(
        report["initialization"]["by_file"],
        json!({"helpers.py":{"statements":1,"reasons":{"startup-call":1}}})
    );
    let occurrence = &sites[0]["occurrence"];
    assert_eq!(occurrence["path"], "helpers.py");
    assert_eq!(occurrence["revision"], report["revision"]);
    assert_eq!(occurrence["role"], "module-initialization");
    let span = &occurrence["location"]["span"];
    let start = span["start"].as_u64().unwrap() as usize;
    let end = span["end"].as_u64().unwrap() as usize;
    assert_eq!(&helper[start..end], "metadata = source()");
    write(root.path(), "helpers.py", &helper.replace("source()", "0"));
    let repaired = analyze(root.path(), "entry");
    assert_eq!(repaired["complete"], true);
    assert_eq!(
        repaired["initialization"],
        json!({"sites":[],"omitted":0,"by_file":{}})
    );
}

#[test]
fn startup_counts_cover_unshown_files_and_count_each_statement_once() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "alpha.py",
        &format!(
            "{}def spare(value):\n    return value\n",
            "opaque()\n".repeat(20)
        ),
    );
    write(root.path(), "zeta.py", "marker = 0\ndef marker(value=source()):\n    return value\ndef relay(value):\n    return value\n");
    write(root.path(), "app.py", "from alpha import spare\nfrom zeta import relay\ndef entry():\n    return sink(relay(0))\n");
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["initialization"]["sites"].as_array().unwrap().len(),
        16
    );
    assert_eq!(report["initialization"]["omitted"], 5);
    assert_eq!(
        report["initialization"]["by_file"],
        json!({
            "alpha.py":{"statements":20,"reasons":{"startup-call":20}},
            "zeta.py":{"statements":1,"reasons":{"definition-time-code":1,"binding-collision":1}}
        })
    );
}

#[test]
fn package_helpers_with_identical_names_keep_distinct_flow_and_origins() {
    let root = fixture();
    let positive = analyze(root.path(), "render");
    let negative = analyze(root.path(), "safe");
    assert_eq!(positive["complete"], true, "{positive}");
    assert_eq!(negative["complete"], true, "{negative}");
    assert!(!positive["witnesses"].as_array().unwrap().is_empty());
    assert!(negative["witnesses"].as_array().unwrap().is_empty());
    assert_eq!(positive["inputs"]["modules"]["schema"], "fr-flow-modules");
    let files = positive["inputs"]["modules"]["files"].as_object().unwrap();
    assert_eq!(files.len(), 6);
    for path in [
        "portal/__init__.py",
        "archive/__init__.py",
        "portal/transform.py",
        "archive/transform.py",
    ] {
        assert!(files.contains_key(path));
    }
    let mut paths = std::collections::BTreeSet::new();
    for witness in positive["witnesses"].as_array().unwrap() {
        for point in witness["trace"]["occurrences"].as_array().unwrap() {
            let path = point["path"].as_str().unwrap();
            let source = fs::read_to_string(root.path().join(path)).unwrap();
            let start = point["location"]["span"]["start"].as_u64().unwrap() as usize;
            let end = point["location"]["span"]["end"].as_u64().unwrap() as usize;
            assert!(source.get(start..end).is_some());
            paths.insert(path);
        }
    }
    assert!(paths.contains("portal/transform.py"));
    assert!(!paths.contains("archive/transform.py"));
}

#[test]
fn plain_classes_and_default_instances_preserve_imported_scalar_summaries() {
    for body in ["return value", "return 0"] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "app.py",
            "from pkg import *\ndef entry():\n    return sink(relay(source()))\n",
        );
        write(
            root.path(),
            "pkg/__init__.py",
            "__all__ = ['relay']\nfrom .core import relay\n",
        );
        write(root.path(), "pkg/core.py", &format!("class Token:\n    def __repr__(self):\n        return 'token'\n    def unused(self, *values):\n        return values\nmarker = Token()\ndef relay(value):\n    {body}\n"));
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(
            !report["witnesses"].as_array().unwrap().is_empty(),
            body == "return value"
        );
        assert_eq!(
            report["function_summaries"]["functions"]["pkg/core.py::relay"]["normal_return"],
            true
        );
    }
}

#[test]
fn custom_construction_cannot_turn_an_empty_trace_into_an_absence_answer() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.py",
        "from core import relay\ndef entry():\n    return sink(relay(0))\n",
    );
    write(root.path(), "core.py", "class Token:\n    def __init__(self):\n        sink(source())\nmarker = Token()\ndef relay(value):\n    return value\n");
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false, "{report}");
    assert!(report["witnesses"].as_array().unwrap().is_empty());
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("module-effects-unchecked")));
}

#[test]
fn dotted_calls_and_parent_relative_imports_resolve_across_nested_packages() {
    let root = tempfile::tempdir().unwrap();
    for (path, source) in [
        ("app.py", "import pkg.nested.relay\ndef entry():\n    return sink(pkg.nested.relay.forward(source()))\n"),
        ("pkg/__init__.py", "pass\n"),
        ("pkg/nested/__init__.py", "\"nested\"\n"),
        ("pkg/nested/relay.py", "from ..leaf import identity\ndef forward(value):\n    return identity(value)\n"),
        ("pkg/leaf.py", "def identity(value):\n    return value\n"),
    ] { write(root.path(), path, source); }
    let value = analyze(root.path(), "entry");
    assert_eq!(value["complete"], true, "{value}");
    assert!(!value["witnesses"].as_array().unwrap().is_empty());
    let lookups = value["inputs"]["modules"]["lookups"].as_array().unwrap();
    assert!(lookups
        .iter()
        .any(|item| item["module"] == "pkg.leaf" && item["target"] == "pkg/leaf.py"));
}

#[test]
fn package_entry_and_initializer_functions_have_exact_identities() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "pkg/__init__.py",
        "def identity(value):\n    return value\n",
    );
    write(
        root.path(),
        "pkg/app.py",
        "from pkg import identity\ndef entry():\n    return sink(identity(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{report}");
    assert_eq!(
        report["inputs"]["modules"]["entry"]["packages"],
        json!(["pkg/__init__.py"])
    );
    assert!(!report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn ambiguous_candidates_stubs_and_effects_stay_incomplete() {
    for (path, source) in [
        ("portal.py", "def other():\n    return 0\n"),
        ("portal.pyi", ""),
        ("portal/__init__.pyi", ""),
        ("portal/api.pyi", ""),
        ("portal/api/__init__.py", ""),
        ("portal/__init__.py", "state = 1 + 2\n"),
        ("portal/__init__.py", "f\"{sink(source())}\"\n"),
        ("portal/__init__.py", "import unavailable_external\n"),
    ] {
        let root = fixture();
        write(root.path(), path, source);
        let report = analyze(root.path(), "render");
        assert_eq!(report["complete"], false, "{path}: {report}");
        assert!(!report["cutoffs"].as_array().unwrap().is_empty());
    }
    let root = fixture();
    fs::remove_file(root.path().join("portal/__init__.py")).unwrap();
    let missing = analyze(root.path(), "render");
    assert_eq!(missing["complete"], true);
    write(root.path(), "portal/__init__.py", "");
    let added = analyze(root.path(), "render");
    assert_eq!(added["complete"], true);
    assert_ne!(missing["input_digest"], added["input_digest"]);
}

#[test]
fn initializer_changes_invalidate_but_unrelated_sources_do_not() {
    let root = fixture();
    let before = analyze(root.path(), "render");
    write(root.path(), "unrelated.py", "def other():\n    return 42\n");
    let unrelated = analyze(root.path(), "render");
    assert_eq!(before["input_digest"], unrelated["input_digest"]);
    assert_ne!(before["revision"], unrelated["revision"]);
    write(root.path(), "portal/__init__.py", "# changed parent\n");
    let changed = analyze(root.path(), "render");
    assert_eq!(changed["complete"], true);
    assert_ne!(before["input_digest"], changed["input_digest"]);
}

#[test]
fn unsupported_import_forms_cycles_and_shadowed_module_calls_stay_incomplete() {
    for source in [
        "from app import entry\ndef entry():\n    return sink(source())\n",
        "from .leaf import identity\ndef entry():\n    return sink(source())\n",
        "from portal import missing\ndef entry():\n    return sink(missing.send(source()))\n",
        "import portal.api\ndef entry(portal):\n    return portal.api.send(source())\n",
        "from portal.api import *\ndef entry():\n    return sink(source())\n",
    ] {
        let root = fixture();
        write(root.path(), "app.py", source);
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false, "{source}: {report}");
    }
}

#[test]
fn import_cycles_use_names_already_created_in_source_order() {
    for returned in ["value", "0"] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "app.py",
            "from alpha import relay\ndef entry():\n    return sink(relay(source()))\n",
        );
        write(
            root.path(),
            "alpha.py",
            &format!("def relay(value):\n    return {returned}\nfrom beta import other\n"),
        );
        write(
            root.path(),
            "beta.py",
            "from alpha import relay\ndef other():\n    return 0\n",
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(
            report["witnesses"].as_array().unwrap().is_empty(),
            returned == "0"
        );
        write(
            root.path(),
            "alpha.py",
            &format!("from beta import other\ndef relay(value):\n    return {returned}\n"),
        );
        let changed = analyze(root.path(), "entry");
        assert_eq!(changed["complete"], false, "{changed}");
        assert!(changed["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("cyclic-module-initialization")));
        assert_ne!(report["input_digest"], changed["input_digest"]);
    }
}

#[test]
fn import_cycle_checks_initialize_parent_packages_before_child_entries() {
    for early in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let definition = "def relay(value):\n    return value\n";
        let import = "from . import child\n";
        write(
            root.path(),
            "pkg/__init__.py",
            &if early {
                format!("{definition}{import}")
            } else {
                format!("{import}{definition}")
            },
        );
        write(
            root.path(),
            "pkg/child.py",
            "from pkg import relay\ndef entry():\n    return sink(relay(source()))\n",
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], early, "{report}");
        if !early {
            assert!(report["cutoffs"]
                .as_array()
                .unwrap()
                .contains(&json!("cyclic-module-initialization")));
        }
    }
}

#[test]
fn from_imports_preload_children_while_ordinary_imports_bind_in_sequence() {
    for (statement, complete) in [
        ("from pkg import marker, child\n", false),
        ("from pkg import child, marker\n", false),
        ("import shared as marker, pkg.child as child\n", true),
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "shared.py", "flag = 0\n");
        write(root.path(), "pkg/__init__.py", "marker = 0\n");
        write(
            root.path(),
            "pkg/child.py",
            "from app import marker\ndef relay(value):\n    return value\n",
        );
        write(
            root.path(),
            "app.py",
            &format!("{statement}def entry():\n    return sink(child.relay(source()))\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], complete, "{statement}: {report}");
        if complete {
            assert!(!report["witnesses"].as_array().unwrap().is_empty());
        } else {
            assert!(report["cutoffs"]
                .as_array()
                .unwrap()
                .contains(&json!("cyclic-module-initialization")));
        }
    }
}

#[test]
fn unsupported_package_bindings_cannot_select_same_named_child_files() {
    for initializer in [
        "class core:\n    pass\n",
        "@decorator\ndef core(value):\n    return value\n",
        "@decorator\nclass core:\n    pass\n",
        "core = object()\n",
        "core: object = object()\n",
        "other = core = object()\n",
        "core, other = object(), 0\n",
        "[core, other] = [object(), 0]\n",
        "core, *other = [object()]\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "pkg/__init__.py", initializer);
        write(
            root.path(),
            "pkg/core.py",
            "def relay(value):\n    return value\n",
        );
        write(
            root.path(),
            "app.py",
            "from pkg import core\ndef entry():\n    return sink(core.relay(source()))\n",
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false, "{initializer}: {report}");
        let lookup = &report["inputs"]["modules"]["lookups"][0];
        assert!(lookup["submodule"].is_null(), "{initializer}: {lookup}");
        assert_eq!(
            lookup["resolution"],
            if initializer.starts_with("class core:") {
                "value"
            } else {
                "unavailable"
            }
        );
        assert!(report["inputs"]["modules"]["files"]["pkg/core.py"].is_null());
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("unknown-external-call:core.relay")));
    }
}

#[test]
fn annotations_without_values_do_not_hide_child_modules() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "pkg/__init__.py", "core: object\n");
    write(
        root.path(),
        "pkg/core.py",
        "def relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from pkg import core\ndef entry():\n    return sink(core.relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    let lookup = &report["inputs"]["modules"]["lookups"][0];
    assert_eq!(lookup["submodule"]["target"], "pkg/core.py");
    assert_eq!(lookup["resolution"], "module");
    assert_eq!(report["complete"], false);
    assert!(!report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn unsupported_bindings_block_function_and_import_resolution() {
    for prefix in [
        "def relay(value):\n    return value\nrelay = object()\n",
        "from helper import relay\nrelay = object()\n",
        "relay = object()\nfrom helper import relay\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "helper.py",
            "def relay(value):\n    return value\n",
        );
        write(
            root.path(),
            "app.py",
            &format!("{prefix}def entry():\n    return sink(relay(source()))\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false);
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("ambiguous-module-binding")));
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("unknown-external-call:relay")));
    }
}

#[test]
fn final_function_definitions_replace_opaque_bindings_without_checking_effects() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "pkg/__init__.py", "@decorate\ndef relay(value):\n    pass\n@decorate\ndef relay(value):\n    pass\ndef relay(value):\n    return value\n");
    write(
        root.path(),
        "app.py",
        "from pkg import relay\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    let lookup = &report["inputs"]["modules"]["lookups"][0];
    assert!(lookup["submodule"].is_null());
    assert_eq!(lookup["resolution"], "function");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("module-effects-unchecked")));
    assert_eq!(
        report["function_summaries"]["functions"]["pkg/__init__.py::relay"]["normal_return"],
        true
    );
    assert!(!report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn literal_export_lists_expand_star_imports_with_exact_definition_chains() {
    for exports in ["['relay']", "('relay',)", "['relay', 'relay']"] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "pkg/leaf.py",
            &format!("__all__ = {exports}\ndef relay(value):\n    return value\n"),
        );
        write(
            root.path(),
            "pkg/__init__.py",
            "__all__ = ['relay']\nfrom .leaf import *\n",
        );
        write(
            root.path(),
            "app.py",
            "from pkg import *\ndef entry():\n    return sink(relay(source()))\n",
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{report}");
        assert!(!report["witnesses"].as_array().unwrap().is_empty());
        let lookups = report["inputs"]["modules"]["lookups"].as_array().unwrap();
        assert_eq!(
            lookups.iter().filter(|item| item["alias"] == "*").count(),
            2
        );
        for item in lookups {
            if item["alias"] == "*" {
                assert_eq!(item["exports"], json!(["relay"]));
                assert_eq!(item["members"][0]["name"], "relay");
            }
        }
    }
}

#[test]
fn wide_star_exports_share_candidates_without_dropping_binding_chains() {
    let root = tempfile::tempdir().unwrap();
    let names: Vec<_> = std::iter::once("relay".to_owned())
        .chain((0..127).map(|index| format!("helper{index}")))
        .collect();
    let mut helper = format!("__all__ = {}\n", serde_json::to_string(&names).unwrap());
    for name in &names {
        helper.push_str(&format!("def {name}(value):\n    return value\n"));
    }
    write(root.path(), "helpers.py", &helper);
    write(
        root.path(),
        "app.py",
        "from helpers import *\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{report}");
    let lookups = report["inputs"]["modules"]["lookups"].as_array().unwrap();
    assert_eq!(lookups.len(), 1);
    let members = lookups[0]["members"].as_array().unwrap();
    assert_eq!(members.len(), 128);
    for member in members {
        assert_eq!(member.as_object().unwrap().len(), 5);
        assert_eq!(member["resolution"], "function");
        assert_eq!(
            member["binding_chain"],
            json!([{"path":"helpers.py","name":member["name"]}])
        );
    }
    assert_eq!(report["inputs"]["modules"]["limits"]["imports"], 128);
}

#[test]
fn wildcard_child_searches_cannot_escape_the_import_limit() {
    let root = tempfile::tempdir().unwrap();
    let names: Vec<_> = (0..128).map(|index| format!("child{index:03}")).collect();
    write(
        root.path(),
        "pkg/__init__.py",
        &format!("__all__ = {}\n", serde_json::to_string(&names).unwrap()),
    );
    for name in &names {
        write(root.path(), &format!("pkg/{name}.py"), "pass\n");
    }
    write(
        root.path(),
        "app.py",
        "from pkg import *\ndef entry():\n    return sink(0)\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("import-lookup-budget")));
    let members = report["inputs"]["modules"]["lookups"][0]["members"]
        .as_array()
        .unwrap();
    assert_eq!(members.len(), 128);
    assert_eq!(
        members
            .iter()
            .filter(|member| !member["submodule"].is_null())
            .count(),
        127
    );
    assert_eq!(members.last().unwrap()["resolution"], "budget");
}

#[test]
fn named_imports_share_one_provider_and_keep_alias_order() {
    let root = tempfile::tempdir().unwrap();
    let mut helper = "def relay(value):\n    return value\n".to_owned();
    let mut names = vec!["relay as selected".to_owned()];
    for index in 0..127 {
        names.push(format!("helper{index} as picked{index}"));
        helper.push_str(&format!("def helper{index}(value):\n    return value\n"));
    }
    write(root.path(), "helpers.py", &helper);
    write(
        root.path(),
        "app.py",
        &format!(
            "from helpers import ({})\ndef entry():\n    return sink(selected(source()))\n",
            names.join(", ")
        ),
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{report}");
    let lookups = report["inputs"]["modules"]["lookups"].as_array().unwrap();
    assert_eq!(lookups.len(), 1);
    assert_eq!(lookups[0]["names"].as_array().unwrap().len(), 128);
    assert_eq!(lookups[0]["names"][0], json!(["selected", "relay"]));
    let members = lookups[0]["members"].as_array().unwrap();
    assert_eq!(members.len(), 127);
    for (index, member) in members.iter().enumerate() {
        assert_eq!(member.as_object().unwrap().len(), 7);
        assert_eq!(member["alias"], format!("picked{index}"));
        assert_eq!(member["member"], format!("helper{index}"));
        assert_eq!(
            member["binding_chain"],
            json!([{"path":"helpers.py", "name":format!("helper{index}")}])
        );
    }
}

#[test]
fn named_package_children_share_the_fixed_search_budget() {
    let root = tempfile::tempdir().unwrap();
    let names: Vec<_> = (0..128).map(|index| format!("child{index:03}")).collect();
    write(root.path(), "pkg/__init__.py", "pass\n");
    for name in &names {
        write(root.path(), &format!("pkg/{name}.py"), "pass\n");
    }
    write(
        root.path(),
        "app.py",
        &format!(
            "from pkg import {}\ndef entry():\n    return sink(0)\n",
            names.join(", ")
        ),
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    let lookup = &report["inputs"]["modules"]["lookups"][0];
    assert!(!lookup["submodule"].is_null());
    let members = lookup["members"].as_array().unwrap();
    assert_eq!(members.len(), 127);
    assert_eq!(
        members
            .iter()
            .filter(|member| !member["submodule"].is_null())
            .count(),
        126
    );
    assert_eq!(members.last().unwrap()["resolution"], "budget");
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("import-lookup-budget")));
}

#[test]
fn named_import_lists_have_a_bounded_binding_inventory() {
    let root = tempfile::tempdir().unwrap();
    let names: Vec<_> = (0..129).map(|index| format!("helper{index}")).collect();
    write(
        root.path(),
        "helpers.py",
        &names
            .iter()
            .map(|name| format!("def {name}(value):\n    return value\n"))
            .collect::<String>(),
    );
    write(
        root.path(),
        "app.py",
        &format!(
            "from helpers import {}\ndef entry():\n    return sink(source())\n",
            names.join(", ")
        ),
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("import-binding-budget")));
    assert_eq!(
        report["inputs"]["modules"]["limits"]["names_per_import"],
        128
    );
}

#[test]
fn sharing_star_searches_keeps_provider_startup_effects_unchecked() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helpers.py",
        "__all__ = ['relay']\nsink(source())\ndef relay(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from helpers import *\ndef entry():\n    return sink(relay(source()))\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["initialization"]["sites"]
        .as_array()
        .unwrap()
        .iter()
        .any(|site| site["occurrence"]["path"] == "helpers.py"));
    assert!(!report["witnesses"].as_array().unwrap().is_empty());
}

#[test]
fn empty_exports_retain_import_dependencies_without_binding_names() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "helper.py",
        "__all__ = []\ndef unused(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from helper import *\ndef entry():\n    return sink(source())\n",
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{report}");
    let lookup = &report["inputs"]["modules"]["lookups"][0];
    assert_eq!(lookup["exports"], json!([]));
    assert_eq!(lookup["terminal_module"], "helper.py");
    assert!(report["inputs"]["modules"]["files"]["helper.py"].is_string());
}

#[test]
fn computed_missing_and_mutated_exports_cannot_claim_a_complete_answer() {
    for declaration in [
        "",
        "__all__ = build()\n",
        "__all__ = ['relay']\n__all__.append('other')\n",
        "__all__ = ['relay']\ncopy = __all__\n",
        "__all__ = ['relay']\n__all__ = []\n",
        "__all__ = [b'relay']\n",
        "__all__ = [f'relay']\n",
        "__all__ = ['re\\u006cay']\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "helper.py",
            &format!("{declaration}def relay(value):\n    return value\n"),
        );
        write(
            root.path(),
            "app.py",
            "from helper import *\ndef entry():\n    return sink(relay(source()))\n",
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false);
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("star-exports-unchecked")));
    }
}

#[test]
fn package_parents_count_toward_the_module_budget() {
    let root = tempfile::tempdir().unwrap();
    let mut source = String::new();
    for n in 0..9 {
        source.push_str(&format!("import p{n}.leaf as m{n}\n"));
        write(root.path(), &format!("p{n}/__init__.py"), "");
        write(
            root.path(),
            &format!("p{n}/leaf.py"),
            "def identity(value):\n    return value\n",
        );
    }
    source.push_str("def entry():\n    return sink(m0.identity(source()))\n");
    write(root.path(), "app.py", &source);
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("import-module-budget")));
    assert_eq!(
        report["inputs"]["modules"]["files"]
            .as_object()
            .unwrap()
            .len(),
        16
    );
}

#[cfg(unix)]
#[test]
fn symlinked_package_ancestors_cannot_supply_module_sources() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "__init__.py", "");
    write(
        outside.path(),
        "leaf.py",
        "def identity(value):\n    return value\n",
    );
    write(
        root.path(),
        "app.py",
        "from pkg.leaf import identity\ndef entry():\n    return sink(identity(source()))\n",
    );
    std::os::unix::fs::symlink(outside.path(), root.path().join("pkg")).unwrap();
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert_eq!(
        report["inputs"]["modules"]["files"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        report["inputs"]["modules"]["lookups"][0]["candidates"]["pkg/leaf.py"]["status"],
        "symlink"
    );
}

#[test]
fn overdeep_import_names_refuse_before_candidate_expansion() {
    let root = tempfile::tempdir().unwrap();
    let module = vec!["nested"; 17].join(".");
    write(
        root.path(),
        "app.py",
        &format!("import {module} as helper\ndef entry():\n    return sink(source())\n"),
    );
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], false);
    assert!(report["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("unsupported-import-form")));
    assert!(report["inputs"]["modules"]["lookups"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn package_flow_acceptance_matches_inputs_and_replays_both_deliveries() {
    let output = Command::new("python3")
        .args([
            "tools/package-flow-acceptance.py",
            "--audit",
            &retained::report("package-flow"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn explicit_reexports_follow_aliases_without_parent_initialization_cycles() {
    let root = tempfile::tempdir().unwrap();
    for (path, source) in [
        (
            "app.py",
            "from pkg import public as relay\ndef entry():\n    return sink(relay(source()))\n",
        ),
        (
            "pkg/__init__.py",
            "from .bridge import exported as public\n",
        ),
        ("pkg/bridge.py", "from .leaf import identity as exported\n"),
        ("pkg/leaf.py", "def identity(value):\n    return value\n"),
    ] {
        write(root.path(), path, source);
    }
    let report = analyze(root.path(), "entry");
    assert_eq!(report["complete"], true, "{report}");
    assert!(!report["witnesses"].as_array().unwrap().is_empty());
    let lookup = report["inputs"]["modules"]["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["importer"] == "app.py")
        .unwrap();
    assert_eq!(
        lookup["binding_chain"],
        json!([
            {"path":"pkg/__init__.py","name":"public"},
            {"path":"pkg/bridge.py","name":"exported"},
            {"path":"pkg/leaf.py","name":"identity"}
        ])
    );
    write(
        root.path(),
        "pkg/leaf.py",
        "def identity(value):\n    return 0\n",
    );
    let changed = analyze(root.path(), "entry");
    assert_eq!(changed["complete"], true, "{changed}");
    assert!(changed["witnesses"].as_array().unwrap().is_empty());
    assert_ne!(report["input_digest"], changed["input_digest"]);
}

#[test]
fn package_reexports_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/package-reexports-acceptance.py",
            "--audit",
            &retained::report("package-reexports"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn module_aliases_retain_child_candidates_and_exact_terminal_functions() {
    let root = tempfile::tempdir().unwrap();
    for (path, source) in [
        (
            "app.py",
            "from bridge import relay\ndef entry():\n    return sink(relay.identity(source()))\n",
        ),
        ("bridge.py", "from pkg import public as relay\n"),
        ("pkg/__init__.py", "from . import leaf as public\n"),
        ("pkg/leaf.py", "def identity(value):\n    return value\n"),
    ] {
        write(root.path(), path, source);
    }
    let value = analyze(root.path(), "entry");
    assert_eq!(value["complete"], true, "{value}");
    assert!(!value["witnesses"].as_array().unwrap().is_empty());
    let lookups = value["inputs"]["modules"]["lookups"].as_array().unwrap();
    let outer = lookups
        .iter()
        .find(|item| item["importer"] == "app.py")
        .unwrap();
    assert_eq!(outer["terminal_module"], "pkg/leaf.py");
    assert_eq!(
        outer["binding_chain"],
        json!([
            {"path":"bridge.py","name":"relay"}, {"path":"pkg/__init__.py","name":"public"}
        ])
    );
    let child = lookups
        .iter()
        .find(|item| item["importer"] == "pkg/__init__.py")
        .unwrap();
    assert_eq!(child["submodule"]["module"], "pkg.leaf");
    assert_eq!(
        child["submodule"]["candidates"]["pkg/leaf.pyi"]["status"],
        "missing"
    );
    write(root.path(), "pkg/leaf.pyi", "");
    let changed = analyze(root.path(), "entry");
    assert_eq!(changed["complete"], false);
    assert_ne!(value["input_digest"], changed["input_digest"]);
}

#[test]
fn module_aliases_acceptance_replays_checked_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/module-aliases-acceptance.py",
            "--audit",
            &retained::report("module-aliases"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn module_attribute_hooks_defer_only_when_plain_and_unused() {
    for (source, complete) in [
        ("def __getattr__(name):\n    return 0\n", true),
        ("def __dir__():\n    return []\n", true),
        (
            "from archive.transform import clean_value as __getattr__\n",
            false,
        ),
        ("def __path__(value):\n    return value\n", false),
    ] {
        let root = fixture();
        write(root.path(), "portal/__init__.py", source);
        let report = analyze(root.path(), "render");
        assert_eq!(report["complete"], complete, "{report}");
        if !complete {
            assert!(report["cutoffs"]
                .as_array()
                .unwrap()
                .contains(&json!("special-module-attributes")));
        }
    }
}

#[test]
fn parallel_aliases_keep_distinct_functions_and_source_bindings() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "app.py",
        "from helpers import public\ndef entry():\n    return sink(public(source()))\n",
    );
    for (values, connected) in [("relay, stop", true), ("stop, relay", false)] {
        write(root.path(), "helpers.py", &format!("# café 🦀\ndef relay(value):\n    return value\ndef stop(value):\n    return 0\npublic, discarded = {values}\n"));
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(
            !report["witnesses"].as_array().unwrap().is_empty(),
            connected
        );
        let aliases = report["inputs"]["modules"]["function_aliases"]
            .as_array()
            .unwrap();
        assert_eq!(aliases.len(), 2);
        assert_ne!(aliases[0]["offset"], aliases[1]["offset"]);
    }
}

#[test]
fn imported_scalar_defaults_bind_omitted_and_explicit_arguments() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "settings.py", "marker, other = 0, []\n");
    write(root.path(), "bridge.py", "from settings import marker\n");
    for (argument, reaches) in [("", false), ("source()", true)] {
        write(root.path(), "app.py", &format!("from bridge import marker as saved\ndef choose(value=saved):\n    return value\ndef entry():\n    return sink(choose({argument}))\n"));
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], true, "{report}");
        assert_eq!(!report["witnesses"].as_array().unwrap().is_empty(), reaches);
        assert_eq!(
            report["inputs"]["modules"]["value_bindings"][0]["path"],
            "settings.py"
        );
    }
}

#[test]
fn incomplete_parallel_initializers_cannot_establish_an_absence() {
    for assignment in [
        "public, other = relay, public",
        "public, other = (relay,)",
        "public, other = relay, setup()",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "app.py",
            "from helpers import relay\ndef entry():\n    return sink(relay(source()))\n",
        );
        write(
            root.path(),
            "helpers.py",
            &format!("def relay(value):\n    return value\n{assignment}\n"),
        );
        let report = analyze(root.path(), "entry");
        assert_eq!(report["complete"], false, "{report}");
        assert!(!report["initialization"]["sites"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn two_questions_share_startup_and_one_step_budget() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "helpers.py", "table = {}\ndef register():\n    table['label'] = 'ready'\nregister()\ndef relay(value):\n    return value\n");
    write(root.path(), "app.py", "from helpers import relay\ndef forwards():\n    return sink(relay(source()))\ndef stopped():\n    return sink(0)\n");
    write(
        root.path(),
        "rules.json",
        "{\"version\":\"together\",\"sources\":[\"source\"],\"sinks\":[\"sink\"]}",
    );
    let first = project(root.path(), &["find", "forwards"]);
    let second = project(root.path(), &["find", "stopped"]);
    let run = |steps: &str| {
        project(
            root.path(),
            &[
                "dataflow",
                first["rows"][0][0].as_str().unwrap(),
                "--also",
                second["rows"][0][0].as_str().unwrap(),
                "--summaries",
                "--imports",
                "--rules",
                root.path().join("rules.json").to_str().unwrap(),
                "--steps",
                steps,
                "--bytes",
                "1048576",
            ],
        )
    };
    let checked = run("4096");
    assert_eq!(checked["schema"], "fr-dataflow-together");
    assert_eq!(checked["startup"]["checks"], 1);
    assert_eq!(
        checked["startup"]["dictionary_statements"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(checked["answers"][0]["complete"], true);
    assert_eq!(checked["answers"][1]["complete"], true);
    assert_eq!(
        checked["answers"][0]["witnesses"].as_array().unwrap().len(),
        1
    );
    assert_eq!(checked["answers"][1]["witnesses"], json!([]));
    let limited = run("1");
    assert_eq!(limited["budget"]["used"], 1);
    assert_eq!(limited["answers"][1]["execution"]["analysis_steps"], 0);
    assert_eq!(limited["answers"][1]["complete"], false);
    write(
        root.path(),
        "helpers.py",
        "def relay(value):\n    return value\nunknown()\n",
    );
    let unknown = project(root.path(), &["find", "forwards"]);
    let other = project(root.path(), &["find", "stopped"]);
    let report = project(
        root.path(),
        &[
            "dataflow",
            unknown["rows"][0][0].as_str().unwrap(),
            "--also",
            other["rows"][0][0].as_str().unwrap(),
            "--summaries",
            "--imports",
            "--bytes",
            "1048576",
        ],
    );
    assert_eq!(report["answers"][0]["complete"], false);
    assert_eq!(report["answers"][1]["complete"], false);
}
