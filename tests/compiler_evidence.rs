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

fn fixture(program: &str, extra: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".fr")).unwrap();
    fs::write(
        dir.path().join("source.rs"),
        "pub fn value() -> i32 { 7 }\n",
    )
    .unwrap();
    let mut check = json!({"name":"compiler","argv":["python3","-c",program],"cwd":".","timeout_seconds":10,"covers":["declared wire fixture"]});
    for (key, value) in extra.as_object().unwrap() {
        check[key] = value.clone();
    }
    fs::write(
        dir.path().join(".fr/checks.json"),
        json!({"schema":1,"checks":[check]}).to_string(),
    )
    .unwrap();
    dir
}

fn execute(root: &Path, bytes: &str) -> Value {
    let listed = run(root, &["checks", "--toolchain"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listing: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let checked = run(
        root,
        &[
            "checks",
            "--toolchain",
            "--run",
            "compiler",
            "--basis",
            listing["basis"].as_str().unwrap(),
            "--output-bytes",
            bytes,
        ],
    );
    serde_json::from_slice(&checked.stdout).unwrap()
}

#[test]
fn declared_identity_drift_during_execution_fails_stability() {
    let dir = fixture(
        "from pathlib import Path; Path('identity.txt').write_text('changed')",
        json!({"identity_files":["identity.txt"]}),
    );
    fs::write(dir.path().join("identity.txt"), "original").unwrap();
    let report = execute(dir.path(), "1024");
    assert_eq!(report["toolchain_stable"], false);
    assert_eq!(report["passed"], false);
}

#[test]
fn identity_declarations_refuse_duplicate_names_and_escaping_relative_files() {
    for extra in [
        json!({"environment":["PATH","PATH"]}),
        json!({"environment":["BAD=NAME"]}),
        json!({"identity_files":["../outside"]}),
        json!({"identity_files":["same","same"]}),
    ] {
        let dir = fixture("pass", extra);
        assert!(!run(dir.path(), &["checks", "--toolchain"]).status.success());
    }
}

#[test]
fn diagnostics_with_successful_exit_preserve_the_protocol_disagreement() {
    let dir = fixture("import json,sys; print(json.dumps({'$message_type':'diagnostic','message':'wire error','level':'error','code':None,'spans':[],'children':[]}),file=sys.stderr)", json!({}));
    let report = execute(dir.path(), "65536");
    assert_eq!(report["passed"], true);
    let retained = tempfile::NamedTempFile::new().unwrap();
    fs::write(retained.path(), serde_json::to_vec(&report).unwrap()).unwrap();
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&report).unwrap()));
    let output = run(
        dir.path(),
        &[
            "project",
            "compiler-evidence",
            "--from",
            retained.path().to_str().unwrap(),
            "--digest",
            &digest,
            "--check",
            "compiler",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let facts: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(facts["complete"], false);
    assert_eq!(facts["outcome"]["passed"], true);
    assert!(facts["capture"]["cutoffs"]
        .as_array()
        .unwrap()
        .contains(&json!("errors-with-passing-check")));
}

#[test]
fn compiler_coverage_matches_all_sixteen_lean_cases() {
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
        .args(["exe", "fr-investigation-kernel", "compiler-coverage"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut expected = Vec::new();
    for execution in [false, true] {
        for protocol in [false, true] {
            for from_start in [false, true] {
                for no_remaining in [false, true] {
                    expected.push(
                        fun_refactor::project::compiler_evidence_complete(
                            execution,
                            protocol,
                            from_start,
                            no_remaining,
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
fn retained_compiler_acceptance_matches_its_inputs_and_oracles() {
    let output = Command::new("python3")
        .args([
            "tools/compiler-evidence-acceptance.py",
            "--audit",
            "tests/agent-eval/results/2026-09-24-compiler-evidence/result.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
