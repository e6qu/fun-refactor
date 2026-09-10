mod common;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FR: &str = env!("CARGO_BIN_EXE_fr");

fn command_output(mut command: Command) -> Output {
    let description = format!("{command:?}");
    command
        .output()
        .unwrap_or_else(|error| panic!("{description}: {error}"))
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn available(program: &Path) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn toolchains() -> Option<PathBuf> {
    let local_tsc = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/typesafety/typescript/node_modules/.bin/tsc");
    let tsc = if local_tsc.exists() {
        local_tsc
    } else {
        PathBuf::from("tsc")
    };
    let missing = [Path::new("python3"), Path::new("node"), tsc.as_path()]
        .into_iter()
        .filter(|program| !available(program))
        .map(|program| program.display().to_string())
        .collect::<Vec<_>>();
    common::require_on_ci("feature migration runtime comparison", &missing);
    missing.is_empty().then_some(tsc)
}

fn fr(root: &Path, args: &[&str]) -> Value {
    let output = command_output({
        let mut command = Command::new(FR);
        command
            .args(["--json", "--no-cache", "-C"])
            .arg(root)
            .args(args);
        command
    });
    let stdout = assert_success(output);
    serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

fn feature(root: &Path) -> String {
    let report = fr(root, &["project", "features", "--limit", "500"]);
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "feature")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn compile_typescript(root: &Path, tsc: &Path, out: &str, files: &[&str]) {
    let output = command_output({
        let mut command = Command::new(tsc);
        command
            .current_dir(root)
            .args([
                "--pretty",
                "false",
                "--strict",
                "--target",
                "ES2022",
                "--module",
                "CommonJS",
                "--lib",
                "ES2022,DOM",
                "--rootDir",
                ".",
                "--outDir",
                out,
            ])
            .args(files);
        command
    });
    assert_success(output);
}

fn run_json(program: &str, root: &Path, file: &str) -> Value {
    let stdout = assert_success(command_output({
        let mut command = Command::new(program);
        command.current_dir(root).arg(file);
        command
    }));
    serde_json::from_str(stdout.trim()).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

#[test]
fn nextjs_and_generated_fastapi_handlers_return_the_same_value() {
    let Some(tsc) = toolchains() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/internal/telemetry/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(route, include_str!("migration-runtime/nextjs-route.ts")).unwrap();
    fs::write(
        dir.path().join("source-runner.ts"),
        include_str!("migration-runtime/nextjs-source-runner.ts"),
    )
    .unwrap();

    let feature = feature(dir.path());
    let report = fr(
        dir.path(),
        &[
            "migrate",
            "feature",
            &feature,
            "--to",
            "fastapi",
            "--out",
            "services/telemetry.py",
            "--write",
        ],
    );
    assert_eq!(report["applied"], true);
    compile_typescript(
        dir.path(),
        &tsc,
        "source-build",
        &["source-runner.ts", "app/internal/telemetry/route.ts"],
    );
    let source = run_json("node", dir.path(), "source-build/source-runner.js");

    fs::write(
        dir.path().join("target-runner.py"),
        include_str!("migration-runtime/fastapi-target-runner.py"),
    )
    .unwrap();
    let target = run_json("python3", dir.path(), "target-runner.py");
    assert_eq!(source, target);
    assert_eq!(source["body"]["service"], "telemetry");
    assert_eq!(source["body"]["samples"], 3);
}

#[test]
fn fastapi_and_generated_nextjs_handlers_return_the_same_value() {
    let Some(tsc) = toolchains() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("metrics.py"),
        include_str!("migration-runtime/fastapi-route.py"),
    )
    .unwrap();
    fs::write(
        dir.path().join("source-runner.py"),
        include_str!("migration-runtime/fastapi-source-runner.py"),
    )
    .unwrap();

    let source = run_json("python3", dir.path(), "source-runner.py");
    let feature = feature(dir.path());
    let report = fr(
        dir.path(),
        &[
            "migrate", "feature", &feature, "--to", "nextjs", "--out", "web/app", "--write",
        ],
    );
    assert_eq!(report["applied"], true);
    fs::write(
        dir.path().join("target-runner.ts"),
        include_str!("migration-runtime/nextjs-target-runner.ts"),
    )
    .unwrap();
    compile_typescript(
        dir.path(),
        &tsc,
        "target-build",
        &["target-runner.ts", "web/app/metrics/[metric_id]/route.ts"],
    );
    let target = run_json("node", dir.path(), "target-build/target-runner.js");
    assert_eq!(source, target);
    assert_eq!(source["body"]["metric_id"], 7);
    assert_eq!(source["body"]["scaled"], 28);
}

#[test]
fn nextjs_payload_keys_and_values_survive_fastapi_generation() {
    let Some(tsc) = toolchains() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/readings/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::write(
        route,
        include_str!("migration-runtime/nextjs-payload-route.ts"),
    )
    .unwrap();
    fs::write(
        dir.path().join("source-runner.ts"),
        include_str!("migration-runtime/nextjs-payload-source-runner.ts"),
    )
    .unwrap();

    let feature = feature(dir.path());
    let report = fr(
        dir.path(),
        &[
            "migrate",
            "feature",
            &feature,
            "--to",
            "fastapi",
            "--out",
            "services/readings.py",
            "--write",
        ],
    );
    assert_eq!(
        report["contract"]["declared_schemas"]["translation_agreement"],
        true
    );
    let generated = fs::read_to_string(dir.path().join("services/readings.py")).unwrap();
    assert!(generated.contains(
        "reading: ReadingEnvelope = ReadingEnvelope.model_validate(await request.json())"
    ));
    compile_typescript(
        dir.path(),
        &tsc,
        "source-build",
        &["source-runner.ts", "app/readings/route.ts"],
    );
    let source = run_json("node", dir.path(), "source-build/source-runner.js");

    fs::write(
        dir.path().join("target-runner.py"),
        include_str!("migration-runtime/fastapi-payload-target-runner.py"),
    )
    .unwrap();
    let target = run_json("python3", dir.path(), "target-runner.py");
    assert_eq!(source, target);
    assert_eq!(source["body"]["sensor_id"], "sensor-4");
    assert_eq!(source["body"]["measuredAt"], "2026-09-10T10:30:00Z");
    assert_eq!(source["body"]["values"], serde_json::json!([3, 5, 8]));
}

#[test]
fn fastapi_payload_keys_and_values_survive_nextjs_generation() {
    let Some(tsc) = toolchains() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("events.py"),
        include_str!("migration-runtime/fastapi-payload-route.py"),
    )
    .unwrap();
    fs::write(
        dir.path().join("source-runner.py"),
        include_str!("migration-runtime/fastapi-payload-source-runner.py"),
    )
    .unwrap();

    let source = run_json("python3", dir.path(), "source-runner.py");
    let feature = feature(dir.path());
    let report = fr(
        dir.path(),
        &[
            "migrate", "feature", &feature, "--to", "nextjs", "--out", "web/app", "--write",
        ],
    );
    assert_eq!(
        report["contract"]["declared_schemas"]["translation_agreement"],
        true
    );
    fs::write(
        dir.path().join("target-runner.ts"),
        include_str!("migration-runtime/nextjs-payload-target-runner.ts"),
    )
    .unwrap();
    compile_typescript(
        dir.path(),
        &tsc,
        "target-build",
        &["target-runner.ts", "web/app/events/route.ts"],
    );
    let target = run_json("node", dir.path(), "target-build/target-runner.js");
    assert_eq!(source, target);
    assert_eq!(source["body"]["event_id"], "evt-9");
    assert_eq!(source["body"]["sentAt"], 1_757_500_200_u64);
    assert_eq!(
        source["body"]["labels"],
        serde_json::json!(["accepted", "priority"])
    );
}
