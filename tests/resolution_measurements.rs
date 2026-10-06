#[test]
fn resolution_measurements_match_complete_baseline_outputs() {
    let output = std::process::Command::new("python3")
        .args([
            "tools/index-resolution-acceptance.py",
            "--verify",
            "tests/agent-eval/results/2026-10-06-explore-ci-index-resolution/result.json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("resolution measurement auditor");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn resolution_evidence_retains_dependency_and_feature_changes() {
    let script = r#"
import importlib.util, sys
sys.path.insert(0, 'tools')
spec = importlib.util.spec_from_file_location('resolution', 'tools/index-resolution-acceptance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def manifest(version='0.1.0', local='0.1.0', remote='1.0.0', feature='std'):
    return f'''[package]
name="root"
version="{version}"
[dependencies]
child={{path="child", version="{local}"}}
remote={{version="{remote}", features=["{feature}"]}}
'''.encode()

original = module.manifest_basis(manifest())
assert original == module.manifest_basis(manifest(version='0.2.0', local='0.2.0'))
assert original != module.manifest_basis(manifest(local='0.1.1'))
assert original != module.manifest_basis(manifest(remote='1.0.1'))
assert original != module.manifest_basis(manifest(feature='alloc'))
"#;
    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
