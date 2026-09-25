use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(root: &Path, args: &[&str]) -> Value {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn module_admission_matches_every_lean_case() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels");
    let built = Command::new("lake")
        .args(["build", "fr-investigation-kernel", "--wfail"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new("lake")
        .args(["exe", "fr-investigation-kernel", "local-module"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut expected = Vec::new();
    for source in [false, true] {
        for package_missing in [false, true] {
            for stub_missing in [false, true] {
                expected.push(
                    fun_refactor::project::dataflow::local_module_admitted(
                        source,
                        package_missing,
                        stub_missing,
                    )
                    .to_string(),
                );
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
fn imports_require_summaries_and_root_local_entries() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(
        root.path().join("nested/app.py"),
        "def entry():\n    return 0\n",
    )
    .unwrap();
    let found = report(root.path(), &["project", "find", "entry"]);
    let target = found["rows"][0][0].as_str().unwrap();
    assert!(
        !run(root.path(), &["project", "dataflow", target, "--imports"])
            .status
            .success()
    );
    let outside = run(
        root.path(),
        &["project", "dataflow", target, "--imports", "--summaries"],
    );
    assert!(!outside.status.success());
    assert!(String::from_utf8_lossy(&outside.stderr).contains("root-local"));
}

#[test]
fn flow_dependency_rejects_nonlocal_queries_before_reading_rules() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("app.py"), "def entry():\n    return 0\n").unwrap();
    for (path, rules) in [
        ("../outside.py", None),
        ("app.py", Some("../rules.json")),
        ("app.py", Some("/tmp/rules.json")),
    ] {
        let query = json!({"path":path,"function":"entry","rules":rules,"context":"generic","steps":256,"depth":8,"bytes":65536});
        let plan = json!({"schema":"fr-investigation-plan-1","goal":"read","acceptance":["done"],"steps":[
            {"id":"flow","question":"flow?","inputs":[{"kind":"flow-inputs","key":query.to_string()}]}]});
        let file = root.path().join("plan.json");
        fs::write(&file, plan.to_string()).unwrap();
        assert!(!run(
            root.path(),
            &["project", "investigate", "--from", file.to_str().unwrap()]
        )
        .status
        .success());
    }
}

#[test]
fn imported_flow_acceptance_matches_its_inputs_and_replays_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/imported-flow-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-25-imported-flow/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
