use fun_refactor::{formal_kernel, spec};
use std::process::Command;

fn plan(source: &str, symbol: &str) -> (tempfile::TempDir, spec::FormalPlan) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.rs"), source).unwrap();
    let plan = spec::formal_plan(root.path(), &format!("subject.rs::{symbol}"),
        &["rust-scalar-model".into()]).unwrap();
    (root, plan)
}

fn check(source: &str, symbol: &str, tactics: &str) {
    let (root, plan) = plan(source, symbol);
    let model = &plan.kernel.model;
    let term = formal_kernel::quote_term(&plan.kernel.evaluation.as_ref().unwrap().term);
    let property = &plan.properties[0];
    let text = format!("{}\n{}\ndef {model}Term : FrPureKernel.Term := {term}\ntheorem {} {} := by\n{}\n",
        formal_kernel::LEAN_SOURCE, plan.kernel.lean_definition, property.name,
        property.proposition, tactics.replace("MODEL", model));
    let file = root.path().join("Connection.lean");
    std::fs::write(&file, &text).unwrap();
    if let Ok(artifacts) = std::env::var("FR_SCALAR_ARTIFACTS") {
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(std::path::Path::new(&artifacts).join(format!("{symbol}.lean")), &text).unwrap();
    }
    let output = Command::new("lean").arg(&file).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn signed_page_count_has_no_arithmetic_failure_for_any_i64_input() {
    check(include_str!("agent-eval/proof-writing/fixtures/src/pages.rs"), "page_count",
        include_str!("fixtures/scalar/page-count.tactics"));
}

#[test]
fn unsigned_window_preserves_the_guard_over_the_entire_u64_domain() {
    check(include_str!("agent-eval/proof-writing/fixtures/src/window.rs"), "fits_window",
        include_str!("fixtures/scalar/window.tactics"));
}

#[test]
fn scalar_admission_does_not_erase_local_integer_widths() {
    let root = tempfile::tempdir().unwrap();
    for source in [
        "pub fn subject(value: i64) -> i64 { let narrow: i8 = 127; let next = narrow + 1; value }",
        "pub fn subject(value: i64) -> i64 { let narrow = 127i8; let next = narrow + 1; value }",
        "pub fn subject(value: u32) -> u32 { value }",
        "pub fn subject(value: i64) -> u64 { 0 }",
    ] {
        std::fs::write(root.path().join("subject.rs"), source).unwrap();
        assert!(spec::formal_plan(root.path(), "subject.rs::subject", &["rust-scalar-model".into()]).is_err(), "{source}");
    }
}

#[test]
fn machine_arithmetic_distinguishes_failure_from_a_mathematical_result() {
    let text = format!("{}\n{}", formal_kernel::LEAN_SOURCE, include_str!("fixtures/scalar/failures.lean"));
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("Failures.lean");
    std::fs::write(&file, text).unwrap();
    let output = Command::new("lean").arg(file).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
}
