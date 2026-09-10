mod common;

use serde_json::Value;
use std::fs::{self, File};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

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

fn fastapi_python() -> Option<PathBuf> {
    let python = std::env::var_os("FR_FASTAPI_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("python3"));
    let has_framework = available(&python)
        && Command::new(&python)
            .args([
                "-c",
                "from importlib.metadata import version; assert (version('fastapi'), version('pydantic'), version('starlette')) == ('0.141.1', '2.13.5', '1.6.0')",
            ])
            .output()
            .is_ok_and(|output| output.status.success());
    let missing = (!has_framework)
        .then(|| format!("{} with the pinned FastAPI stack", python.display()))
        .into_iter()
        .collect::<Vec<_>>();
    common::require_on_ci("feature migration FastAPI runtime comparison.", &missing);
    has_framework.then_some(python)
}

struct NextRuntime {
    root: PathBuf,
    executable: PathBuf,
}

fn nextjs_runtime() -> Option<NextRuntime> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/migration-runtime/nextjs-framework");
    let executable = root.join("node_modules/.bin/next");
    let packages = [
        ("next", "16.3.4"),
        ("react", "19.3.0"),
        ("react-dom", "19.3.0"),
        ("typescript", "5.9.3"),
        ("@types/node", "22.20.2"),
        ("@types/react", "19.3.0"),
        ("@types/react-dom", "19.3.0"),
    ];
    let mut missing = packages
        .into_iter()
        .filter_map(|(name, expected)| {
            let manifest = root.join("node_modules").join(name).join("package.json");
            let actual = fs::read_to_string(&manifest)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|json| json["version"].as_str().map(str::to_owned));
            (actual.as_deref() != Some(expected))
                .then(|| format!("{name} {expected} under {}", root.display()))
        })
        .collect::<Vec<_>>();
    if !available(&executable) {
        missing.push(format!("{}", executable.display()));
    }
    if TcpListener::bind(("127.0.0.1", 0)).is_err() {
        missing.push("a local loopback listener".to_owned());
    }
    #[cfg(not(unix))]
    missing.push("Unix process and directory-link support".to_owned());
    common::require_on_ci("feature migration Next.js runtime comparison.", &missing);
    missing
        .is_empty()
        .then_some(NextRuntime { root, executable })
}

struct RunningNext {
    child: Child,
}

impl Drop for RunningNext {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let group = format!("-{}", self.child.id());
            let _ = Command::new("kill").args(["-TERM", &group]).status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn run_nextjs_route(runtime: &NextRuntime, root: &Path, payloads: &[Value]) -> Vec<Value> {
    let web = root.join("web");
    #[cfg(unix)]
    std::os::unix::fs::symlink(runtime.root.join("node_modules"), web.join("node_modules"))
        .unwrap();
    #[cfg(not(unix))]
    panic!("the installed Next.js runtime fixture currently requires Unix directory links.");

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let log_path = root.join("nextjs-runtime.log");
    let log = File::create(&log_path).unwrap();
    let mut command = Command::new(&runtime.executable);
    command
        .current_dir(&web)
        .args([
            "dev",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .env("NEXT_TELEMETRY_DISABLED", "1")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().unwrap();
    let _running = RunningNext { child };
    let runner = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/migration-runtime/nextjs-framework-runner.mjs");
    let output = command_output({
        let mut command = Command::new("node");
        command
            .current_dir(&web)
            .arg(runner)
            .arg(format!("http://127.0.0.1:{port}/events"));
        for payload in payloads {
            command.arg(payload.to_string());
        }
        command
    });
    assert!(
        output.status.success(),
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        fs::read_to_string(log_path).unwrap_or_default()
    );
    serde_json::from_slice(&output.stdout).unwrap()
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

fn run_json_path(program: &Path, root: &Path, file: &str) -> Value {
    let stdout = assert_success(command_output({
        let mut command = Command::new(program);
        command.current_dir(root).arg(file);
        command
    }));
    serde_json::from_str(stdout.trim()).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

fn run_json_path_args(program: &Path, root: &Path, file: &str, args: &[&str]) -> Value {
    let stdout = assert_success(command_output({
        let mut command = Command::new(program);
        command.current_dir(root).arg(file).args(args);
        command
    }));
    serde_json::from_str(stdout.trim()).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

#[test]
fn generated_fastapi_router_runs_through_the_explicit_application_registration() {
    let Some(python) = fastapi_python() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let route = dir.path().join("app/signals/route.ts");
    fs::create_dir_all(route.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("backend/routes")).unwrap();
    fs::write(
        &route,
        "export async function GET() {\n  return Response.json({ state: \"ready\" });\n}\n",
    )
    .unwrap();
    fs::write(dir.path().join("backend/__init__.py"), "").unwrap();
    fs::write(dir.path().join("backend/routes/__init__.py"), "").unwrap();
    fs::write(
        dir.path().join("backend/main.py"),
        "from fastapi import FastAPI\n\napplication = FastAPI()\n",
    )
    .unwrap();
    let selected = feature(dir.path());
    let report = fr(
        dir.path(),
        &[
            "migrate",
            "feature",
            &selected,
            "--to",
            "fastapi",
            "--out",
            "backend/routes/signals.py",
            "--register-with",
            "backend/main.py::application",
            "--cutover",
            "--write",
        ],
    );
    assert_eq!(
        report["migration"]["coexistence"]["destination_registration"],
        "automatic"
    );
    assert!(!route.exists());
    fs::write(
        dir.path().join("registered-runner.py"),
        include_str!("migration-runtime/fastapi-registered-runner.py"),
    )
    .unwrap();
    let response = run_json_path_args(
        &python,
        dir.path(),
        "registered-runner.py",
        &["backend.main", "application", "GET", "/signals"],
    );
    assert_eq!(response["status"], 200);
    assert_eq!(response["body"], serde_json::json!({"state": "ready"}));
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
    assert!(generated.contains("async def post(request: Request, reading: ReadingEnvelope):"));
    assert!(!generated.contains("await request.json()"));
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
    assert_eq!(
        source["body"]["values"],
        serde_json::json!([3.25, 5.5, 8.75])
    );

    if let Some(python) = fastapi_python() {
        fs::write(
            dir.path().join("framework-target-runner.py"),
            include_str!("migration-runtime/fastapi-framework-target-runner.py"),
        )
        .unwrap();
        let framework = run_json_path(&python, dir.path(), "framework-target-runner.py");
        assert_eq!(framework["valid"], source);
        assert_eq!(framework["invalid"]["status"], 422);
        assert!(framework["invalid"]["body"]["detail"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["loc"] == serde_json::json!(["body", "values", 0])));
    }
}

#[test]
fn fastapi_payload_keys_and_values_survive_nextjs_generation() {
    let Some(tsc) = toolchains() else {
        return;
    };
    let next_runtime = nextjs_runtime();
    let dir = match &next_runtime {
        Some(runtime) => tempfile::Builder::new()
            .prefix(".fr-runtime-")
            .tempdir_in(&runtime.root)
            .unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    fs::write(
        dir.path().join("events.py"),
        include_str!("migration-runtime/fastapi-payload-route.py"),
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("web")).unwrap();
    fs::write(
        dir.path().join("web/package.json"),
        include_str!("migration-runtime/nextjs-framework/package.json"),
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
    assert_eq!(
        report["migration"]["coexistence"]["destination_registration"],
        "automatic"
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

    let invalid = serde_json::json!({
        "event_id": "evt-9",
        "sentAt": 1_757_500_200_u64,
        "labels": [7],
    });
    let fastapi_framework = fastapi_python().map(|python| {
        fs::write(
            dir.path().join("framework-source-runner.py"),
            include_str!("migration-runtime/fastapi-framework-source-runner.py"),
        )
        .unwrap();
        run_json_path(&python, dir.path(), "framework-source-runner.py")
    });
    if let Some(runtime) = next_runtime {
        let framework = run_nextjs_route(&runtime, dir.path(), &[source["body"].clone(), invalid]);
        assert_eq!(framework[0], source);
        assert_eq!(framework[1]["status"], 422);
        assert!(framework[1]["body"]["detail"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["loc"] == serde_json::json!(["body", "labels", 0])));
        if let Some(fastapi) = fastapi_framework {
            assert_eq!(fastapi["valid"], framework[0]);
            assert_eq!(fastapi["invalid"]["status"], framework[1]["status"]);
            assert!(fastapi["invalid"]["body"]["detail"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error["loc"] == serde_json::json!(["body", "labels", 0])));
        }
    }
}
