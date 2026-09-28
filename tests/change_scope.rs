use std::process::Command;

#[test]
fn scope_readiness_matches_lean_and_requires_every_condition() {
    let output = Command::new("lake")
        .args(["exe", "fr-investigation-kernel", "change-scope"])
        .current_dir("kernels")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for complete in [false, true] {
        for certain in [false, true] {
            for mapped in [false, true] {
                for checks in [false, true] {
                    let ready = fun_refactor::project::change_scope::review_ready(
                        complete, certain, mapped, checks,
                    );
                    assert_eq!(
                        ready,
                        [complete, certain, mapped, checks]
                            .iter()
                            .all(|value| *value)
                    );
                    expected.push(ready.to_string());
                }
            }
        }
    }
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}
