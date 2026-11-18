use fun_refactor::{formal_kernel, spec};
use std::process::Command;

fn plan(source: &str, symbol: &str) -> (tempfile::TempDir, spec::FormalPlan) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.rs"), source).unwrap();
    let plan = spec::formal_plan(
        root.path(),
        &format!("subject.rs::{symbol}"),
        &["rust-scalar-model".into()],
    )
    .unwrap();
    (root, plan)
}

fn check(source: &str, symbol: &str, tactics: &str) {
    let (root, plan) = plan(source, symbol);
    let model = &plan.kernel.model;
    let term = formal_kernel::quote_term(&plan.kernel.evaluation.as_ref().unwrap().term);
    let property = &plan.properties[0];
    let text = format!(
        "{}\n{}\ndef {model}Term : FrPureKernel.Term := {term}\ntheorem {} {} := by\n{}\n",
        formal_kernel::LEAN_SOURCE,
        plan.kernel.lean_definition,
        property.name,
        property.proposition,
        tactics.replace("MODEL", model)
    );
    let file = root.path().join("Connection.lean");
    std::fs::write(&file, &text).unwrap();
    if let Ok(artifacts) = std::env::var("FR_SCALAR_ARTIFACTS") {
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(
            std::path::Path::new(&artifacts).join(format!("{symbol}.lean")),
            &text,
        )
        .unwrap();
    }
    let output = Command::new("lean").arg(&file).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn signed_page_count_has_no_arithmetic_failure_for_any_i64_input() {
    check(
        include_str!("agent-eval/proof-writing/fixtures/src/pages.rs"),
        "page_count",
        include_str!("fixtures/scalar/page-count.tactics"),
    );
}

#[test]
fn unsigned_window_preserves_the_guard_over_the_entire_u64_domain() {
    check(
        include_str!("agent-eval/proof-writing/fixtures/src/window.rs"),
        "fits_window",
        include_str!("fixtures/scalar/window.tactics"),
    );
}

#[test]
fn scalar_admission_does_not_erase_local_integer_widths() {
    let root = tempfile::tempdir().unwrap();
    for source in [
        "pub fn subject(value: i64) -> i64 { let narrow: i8 = 127; let next = narrow + 1; value }",
        "pub fn subject(value: i64) -> i64 { let narrow = 127i8; let next = narrow + 1; value }",
        "pub fn subject(value: u32) -> u32 { value }",
        "pub fn subject(value: i64) -> u64 { 0 }",
        "pub fn subject(value: i64) -> i64 { let narrow = 2147483647 + 1; value }",
        "pub fn subject(value: i64) -> bool { value > 0 && 2147483647 + 1 > 0 }",
        "pub fn subject(value: u64) -> u64 { -value }",
    ] {
        std::fs::write(root.path().join("subject.rs"), source).unwrap();
        assert!(
            spec::formal_plan(
                root.path(),
                "subject.rs::subject",
                &["rust-scalar-model".into()]
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn machine_arithmetic_distinguishes_failure_from_a_mathematical_result() {
    let text = format!(
        "{}\n{}",
        formal_kernel::LEAN_SOURCE,
        include_str!("fixtures/scalar/failures.lean")
    );
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("Failures.lean");
    std::fs::write(&file, text).unwrap();
    let output = Command::new("lean").arg(file).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn fr(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("--json")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: std::process::Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn checked_scalar_proofs_use_normal_authoring_and_expire_with_source_or_library_changes() {
    let source = include_str!("agent-eval/proof-writing/fixtures/src/window.rs");
    let (root, plan) = plan(source, "fits_window");
    success(fr(root.path(), &["spec", "init", "specs", "--write"]));
    std::fs::write(
        root.path().join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    success(fr(
        root.path(),
        &["spec", "scaffold", "--from", "plan.json", "--write"],
    ));
    let model = format!("specs/FrSpecs/{}.lean", plan.kernel.module);
    let target = format!("{model}::{}", plan.properties[0].name);
    std::fs::write(
        root.path().join("tactics"),
        include_str!("fixtures/scalar/window.tactics").replace("MODEL", &plan.kernel.model),
    )
    .unwrap();
    let saved = success(fr(
        root.path(),
        &[
            "proof",
            "save",
            &target,
            "--draft",
            "tactics",
            "--bytes",
            "8192",
            "--timeout-seconds",
            "30",
        ],
    ));
    assert_eq!(saved["proof_checked"], true);
    let evidence = success(fr(root.path(), &["spec", "evidence", "specs"]));
    assert_eq!(
        evidence["rust_scalar_correspondence"][0]["status"],
        "checked_by_lean"
    );
    assert_eq!(
        evidence["rust_scalar_correspondence"][0]["source_implementation_proved"],
        false
    );
    assert!(evidence["kernel_correspondence"]
        .as_array()
        .unwrap()
        .is_empty());
    let before = std::fs::read(root.path().join(&model)).unwrap();
    std::fs::write(
        root.path().join("subject.rs"),
        source.replace("offset <= total", "offset < total"),
    )
    .unwrap();
    assert!(!fr(
        root.path(),
        &[
            "proof",
            "check",
            &target,
            "--draft",
            "tactics",
            "--bytes",
            "8192",
            "--timeout-seconds",
            "30"
        ]
    )
    .status
    .success());
    assert_eq!(std::fs::read(root.path().join(&model)).unwrap(), before);
    std::fs::write(root.path().join("subject.rs"), source).unwrap();
    let library = root.path().join("specs/FrSpecs/PureKernel.lean");
    let original_library = std::fs::read_to_string(&library).unwrap();
    std::fs::write(
        &library,
        format!("{original_library}\n-- changed dependency\n"),
    )
    .unwrap();
    assert!(!fr(root.path(), &["spec", "evidence", "specs"])
        .status
        .success());
    std::fs::write(&library, original_library).unwrap();
    let changed_model = String::from_utf8(before.clone())
        .unwrap()
        .replace("total - offset", "total + offset");
    assert_ne!(changed_model.as_bytes(), before);
    std::fs::write(root.path().join(&model), changed_model).unwrap();
    assert!(!fr(root.path(), &["spec", "evidence", "specs"])
        .status
        .success());
}

#[test]
fn compiled_rust_and_lean_agree_at_signed_and_unsigned_boundaries() {
    let pages = [
        -9223372036854775808_i128,
        -1,
        0,
        1,
        2,
        11,
        9223372036854775807,
    ];
    let windows = [0_i128, 1, 2, 9223372036854775808, 18446744073709551615];
    let page_cases = pages
        .iter()
        .flat_map(|a| pages.iter().map(move |b| vec![*a, *b]))
        .collect::<Vec<_>>();
    let mut window_cases = Vec::new();
    for a in windows {
        for b in windows {
            for c in windows {
                window_cases.push(vec![a, b, c]);
            }
        }
    }
    for (source, name, cases) in [
        (
            include_str!("agent-eval/proof-writing/fixtures/src/pages.rs"),
            "page_count",
            page_cases,
        ),
        (
            include_str!("agent-eval/proof-writing/fixtures/src/window.rs"),
            "fits_window",
            window_cases,
        ),
    ] {
        let (root, plan) = plan(source, name);
        let mut rust = format!("{source}\nfn main() {{\n");
        let mut lean = format!("import Std\n{}\n", plan.kernel.lean_definition);
        for arguments in cases {
            rust.push_str(&format!(
                "println!(\"{{}}\", {name}({}));\n",
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            lean.push_str(&format!(
                "#eval IO.println ({} {})\n",
                plan.kernel.model,
                arguments
                    .iter()
                    .map(|value| format!("({value})"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        rust.push_str("}\n");
        std::fs::write(root.path().join("main.rs"), rust).unwrap();
        std::fs::write(root.path().join("Main.lean"), lean).unwrap();
        let expected = Command::new("lean")
            .arg(root.path().join("Main.lean"))
            .output()
            .unwrap();
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stdout)
        );
        for optimization in ["0", "3"] {
            let executable = root.path().join("runtime");
            let compiled = Command::new("rustc")
                .arg(root.path().join("main.rs"))
                .args([
                    "-C",
                    &format!("opt-level={optimization}"),
                    "-C",
                    "overflow-checks=yes",
                    "-o",
                ])
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let actual = Command::new(&executable).output().unwrap();
            assert!(actual.status.success());
            assert_eq!(
                actual.stdout, expected.stdout,
                "{name}: optimization {optimization}"
            );
        }
    }
}
