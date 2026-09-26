use std::process::Command;

#[test]
fn proof_guard_bounds_children_and_fails_closed() {
    let output = Command::new("python3")
        .arg("tools/test_lean_guard.py")
        .output()
        .expect("Python is installed for the Lean gate");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
