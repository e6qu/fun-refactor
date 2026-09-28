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
fn package_helpers_with_identical_names_keep_distinct_flow_and_origins() {
    let root = fixture();
    let positive = analyze(root.path(), "render");
    let negative = analyze(root.path(), "safe");
    assert_eq!(positive["complete"], true, "{positive}");
    assert_eq!(negative["complete"], true, "{negative}");
    assert!(!positive["witnesses"].as_array().unwrap().is_empty());
    assert!(negative["witnesses"].as_array().unwrap().is_empty());
    assert_eq!(positive["inputs"]["modules"]["schema"], "fr-flow-modules-2");
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
fn ambiguous_candidates_stubs_namespace_packages_and_effects_stay_incomplete() {
    for (path, source) in [
        ("portal.py", "def other():\n    return 0\n"),
        ("portal.pyi", ""),
        ("portal/__init__.pyi", ""),
        ("portal/api.pyi", ""),
        ("portal/api/__init__.py", ""),
        ("portal/__init__.py", "state = 1\n"),
        ("portal/__init__.py", "f\"{sink(source())}\"\n"),
        ("portal/__init__.py", "from .transform import clean_value\n"),
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
    assert_eq!(missing["complete"], false);
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
        "from portal import api\ndef entry():\n    return sink(api.send(source()))\n",
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
            "tests/agent-eval/results/2026-09-28-package-flow/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
