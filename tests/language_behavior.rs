//! Execute the documented bounded translations and Python effect counterexamples.
use std::process::Command;

fn collect(script: &str) {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new("python3")
        .args([
            "-B",
            script,
            "collect",
            "--binary",
            env!("CARGO_BIN_EXE_fr"),
            "--output",
        ])
        .arg(directory.path().join("result"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bounded_translations_match_independent_integer_arithmetic() {
    collect("tools/checked-translation.py");
}

#[test]
fn effect_locations_and_model_limits_match_independent_python_executions() {
    collect("tools/python-effects.py");
}
