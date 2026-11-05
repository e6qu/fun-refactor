use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const FR: &str = env!("CARGO_BIN_EXE_fr");
const TARGET: &str = "specs/FrSpecs/SrcLibRsKeep.lean::keepModel_identity";

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(FR)
        .args(["--json", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn keep(value: bool) -> bool { value }\n",
    )
    .unwrap();
    let initialized = run(root.path(), &["spec", "init", "specs", "--write"]);
    assert!(initialized.status.success(), "{}", report(&initialized));
    let plan =
        fun_refactor::spec::formal_plan(root.path(), "src/lib.rs::keep", &["identity".into()])
            .unwrap();
    std::fs::write(
        root.path().join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let scaffolded = run(
        root.path(),
        &[
            "spec",
            "scaffold",
            "--from",
            "plan.json",
            "--package",
            "specs",
            "--write",
        ],
    );
    assert!(scaffolded.status.success(), "{}", report(&scaffolded));
    root
}

#[test]
fn proof_start_creates_a_draft_and_returns_the_actual_goal_without_overwriting() {
    let root = project();
    let args = [
        "proof",
        "start",
        TARGET,
        "--draft",
        ".fr/tactics",
        "--bytes",
        "8192",
        "--timeout-seconds",
        "30",
    ];
    let started = run(root.path(), &args);
    assert!(started.status.success(), "{}", report(&started));
    let state = report(&started);
    assert_eq!(state["operation"], "proof-start");
    assert_eq!(state["target"], TARGET);
    assert_eq!(state["acceptance"], "not-checked");
    assert!(state["goals"][0]["context"]
        .as_str()
        .unwrap()
        .contains("Bool"));
    assert!(state["goals"][0]["goal"]
        .as_str()
        .unwrap()
        .contains("keepModel"));
    std::fs::write(root.path().join(".fr/tactics"), "rfl\n").unwrap();
    let refused = run(root.path(), &args);
    assert!(!refused.status.success());
    assert!(report(&refused)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("existing drafts stay unchanged"));
    assert_eq!(
        std::fs::read_to_string(root.path().join(".fr/tactics")).unwrap(),
        "rfl\n"
    );
}

#[test]
fn a_rejected_check_has_one_json_report_a_failing_exit_and_no_write() {
    let root = project();
    let before = std::fs::read(root.path().join("specs/FrSpecs/SrcLibRsKeep.lean")).unwrap();
    std::fs::write(root.path().join("bad.tactics"), "skip\n").unwrap();
    let rejected = run(
        root.path(),
        &[
            "proof",
            "check",
            TARGET,
            "--draft",
            "bad.tactics",
            "--bytes",
            "8192",
            "--timeout-seconds",
            "30",
        ],
    );
    assert_eq!(rejected.status.code(), Some(1));
    assert_eq!(report(&rejected)["passed"], false);
    assert!(report(&rejected)["receipt"].is_null());
    assert!(rejected.stderr.is_empty());
    assert_eq!(
        std::fs::read(root.path().join("specs/FrSpecs/SrcLibRsKeep.lean")).unwrap(),
        before
    );
}

#[test]
fn one_save_checks_and_writes_then_package_check_and_history_deliver_the_patch() {
    let root = project();
    std::fs::write(root.path().join("ready.tactics"), "rfl\n").unwrap();
    let saved = run(
        root.path(),
        &[
            "proof",
            "save",
            TARGET,
            "--draft",
            "ready.tactics",
            "--bytes",
            "8192",
            "--timeout-seconds",
            "30",
        ],
    );
    assert!(saved.status.success(), "{}", report(&saved));
    let saved = report(&saved);
    assert_eq!(saved["status"], "written");
    assert_eq!(saved["proof_checked"], true);
    let transaction = saved["transaction"].as_u64().unwrap().to_string();
    let checked = Command::new("lake")
        .args(["-d", "specs", "build", "--wfail"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    let delivered = run(
        root.path(),
        &["history", "patch", &transaction, "--output", "change.patch"],
    );
    assert!(delivered.status.success(), "{}", report(&delivered));
    let patch = std::fs::read_to_string(root.path().join("change.patch")).unwrap();
    assert!(patch.contains("SrcLibRsKeep.lean"));
    assert!(patch.contains("+  rfl"));
}

#[test]
fn argument_errors_honor_json_and_do_not_create_a_draft() {
    let root = tempfile::tempdir().unwrap();
    let invalid = run(
        root.path(),
        &[
            "proof",
            "start",
            "wrong-target",
            "--draft",
            "draft.tactics",
            "--bytes",
            "4096",
        ],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(report(&invalid)["error"]["kind"], "invalid-input");
    assert!(invalid.stderr.is_empty());
    assert!(!root.path().join("draft.tactics").exists());
}

#[test]
fn project_queries_have_text_for_people_and_json_for_programs() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("example.py"),
        "def greet(name):\n    return name\n",
    )
    .unwrap();
    let text = Command::new(FR)
        .arg("-C")
        .arg(root.path())
        .args(["project", "find", "greet", "--limit", "1"])
        .output()
        .unwrap();
    assert!(
        text.status.success(),
        "{}",
        String::from_utf8_lossy(&text.stderr)
    );
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("Scan root:"));
    assert!(text.contains("Result 1:"));
    assert!(text.contains("greet"));
    assert!(serde_json::from_str::<Value>(&text).is_err());
    let structured = run(root.path(), &["project", "find", "greet", "--limit", "1"]);
    assert!(structured.status.success());
    assert_eq!(report(&structured)["page"]["returned"], 1);
}
