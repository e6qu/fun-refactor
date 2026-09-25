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
fn orphan_module_is_checked_by_ordinary_and_retained_evidence() {
    let root = tempfile::tempdir().unwrap();
    report(root.path(), &["spec", "init", "--write"]);
    fs::create_dir_all(root.path().join("specs/FrSpecs")).unwrap();
    fs::write(
        root.path().join("specs/FrSpecs/Orphan.lean"),
        "theorem impossible : False := by trivial\n",
    )
    .unwrap();
    let ordinary = run(
        root.path(),
        &["spec", "evidence", "specs/FrSpecs/Orphan.lean"],
    );
    assert!(!ordinary.status.success());
    let ordinary: Value = serde_json::from_slice(&ordinary.stdout).unwrap();
    assert_eq!(ordinary["properties"][0]["status"], "unchecked");
    let review = report(root.path(), &["spec", "retain"]);
    assert_eq!(review["executed"], false);
    let checked = run(
        root.path(),
        &[
            "spec",
            "retain",
            "--run",
            "--basis",
            review["input_digest"].as_str().unwrap(),
        ],
    );
    assert!(!checked.status.success());
    let checked: Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(checked["passed"], false);
    assert_eq!(checked["stable"], true);
    assert_eq!(checked["evidence"]["properties"][0]["status"], "unchecked");
}

#[test]
fn proof_arguments_and_native_requirement_coverage_refuse() {
    let root = tempfile::tempdir().unwrap();
    report(root.path(), &["spec", "init", "--write"]);
    assert!(!run(root.path(), &["spec", "retain", "--run"])
        .status
        .success());
    assert!(
        !run(root.path(), &["spec", "retain", "--run", "--basis", "old"])
            .status
            .success()
    );
    let plan = json!({"schema":"fr-investigation-plan-1","goal":"prove","acceptance":["model"],"steps":[
        {"id":"proof","question":"identity?","inputs":[{"kind":"workspace","key":"."}],
         "required_proofs":[{"package":"specs","spec":"FrSpecs.lean","theorem":"identity"}]}]});
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), plan.to_string()).unwrap();
    let result = run(
        root.path(),
        &[
            "project",
            "investigate",
            "--from",
            file.path().to_str().unwrap(),
        ],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("proof-inputs dependency"));
}

#[test]
fn proof_package_budgets_refuse_before_execution() {
    let root = tempfile::tempdir().unwrap();
    report(root.path(), &["spec", "init", "--write"]);
    fs::write(
        root.path().join("specs/oversized.txt"),
        vec![b'x'; 4_194_305],
    )
    .unwrap();
    let output = run(root.path(), &["spec", "retain"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("4 MiB"));
    fs::remove_file(root.path().join("specs/oversized.txt")).unwrap();
    for i in 0..129 {
        fs::write(root.path().join(format!("specs/file{i}.txt")), "x").unwrap();
    }
    let output = run(root.path(), &["spec", "retain"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("128 files"));
}

#[test]
fn proof_acceptance_agrees_with_lean_for_every_boolean_input() {
    let built = Command::new("lake")
        .args(["build", "fr-investigation-kernel", "--wfail"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new("lake")
        .args(["exe", "fr-investigation-kernel", "proof-evidence"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for executed in [false, true] {
        for stable in [false, true] {
            for checked in [false, true] {
                for debt_free in [false, true] {
                    expected.push(
                        fun_refactor::spec::retained::proof_acceptable(
                            executed, stable, checked, debt_free,
                        )
                        .to_string(),
                    );
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
fn retained_proof_acceptance_matches_its_inputs_and_replays_delivery() {
    let output = Command::new("python3")
        .args([
            "tools/proof-evidence-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-24-retained-proofs/result.json",
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
fn proof_review_bounds_escaped_path_disclosure() {
    let root = tempfile::tempdir().unwrap();
    report(root.path(), &["spec", "init", "--write"]);
    let mut directory = root.path().join("specs");
    for _ in 0..4 {
        directory.push("\u{1}".repeat(200));
    }
    fs::create_dir_all(&directory).unwrap();
    for index in 0..125 {
        fs::write(directory.join(format!("{index}.lean")), "x").unwrap();
    }
    let output = run(root.path(), &["spec", "retain"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("proof review exceeds 1 MiB"));
}
