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
    assert_eq!(shadowed["complete"], false);
    assert!(!shadowed["initialization"]["sites"]
        .as_array()
        .unwrap()
        .is_empty());
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
    assert_eq!(sites[0]["reasons"], json!(["module-statement"]));
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
    assert_eq!(repaired["initialization"], json!({"sites":[],"omitted":0}));
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
        "import app\ndef entry():\n    return sink(source())\n",
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
        for (index, item) in lookups.iter().enumerate() {
            if item["alias"] == "*" {
                assert_eq!(item["exports"], json!(["relay"]));
                assert!(lookups
                    .iter()
                    .any(|member| member["star"] == index && member["member"] == "relay"));
            }
        }
    }
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
            "tests/agent-eval/results/2026-10-15-builtin-bases-package-flow/result.json",
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
            "tests/agent-eval/results/2026-10-15-builtin-bases-package-reexports/result.json",
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
            "tests/agent-eval/results/2026-10-15-builtin-bases-module-aliases/result.json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn special_module_bindings_refuse_even_when_the_function_body_is_inert() {
    for source in [
        "def __getattr__(name):\n    return 0\n",
        "from archive.transform import clean_value as __getattr__\n",
        "def __path__(value):\n    return value\n",
    ] {
        let root = fixture();
        write(root.path(), "portal/__init__.py", source);
        let report = analyze(root.path(), "render");
        assert_eq!(report["complete"], false, "{report}");
        assert!(report["cutoffs"]
            .as_array()
            .unwrap()
            .contains(&json!("special-module-attributes")));
    }
}
