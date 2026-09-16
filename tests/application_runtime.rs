mod common;

use fun_refactor::application_ir::{write_routes, Adapter, HttpRoute};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

fn success(mut command: Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn routes() -> Vec<HttpRoute> {
    serde_json::from_value(json!([
        {"method":"GET", "path":"/", "status":200, "response":{"kind":"literal","value":true}},
        {"method":"POST", "path":"/records/{JSONResponse}", "status":201,
         "response":{"kind":"object","fields":{
             "__proto__":{"kind":"object","fields":{"safe":{"kind":"literal","value":true}}},
             "id":{"kind":"path","name":"JSONResponse"},
             "nested":{"kind":"array","items":[{"kind":"literal","value":null},{"kind":"literal","value":"é\n\"\\"},{"kind":"literal","value":-9007199254740991_i64}]}
         }}}
    ])).unwrap()
}

fn fixture(adapter: Adapter) -> (tempfile::TempDir, Value) {
    let dir = tempfile::tempdir().unwrap();
    let routes = routes();
    for (path, source) in write_routes(&routes, adapter).unwrap() {
        fs::write(dir.path().join(path), source).unwrap();
    }
    let cases = json!([{"method":"GET", "url":"/"}, {"method":"POST", "url":"/records/chosen"}]);
    fs::write(dir.path().join("cases.json"), cases.to_string()).unwrap();
    let expected = json!([
        {"status":200,"body":routes[0].response.evaluate(&BTreeMap::new()).unwrap()},
        {"status":201,"body":routes[1].response.evaluate(&BTreeMap::from([("JSONResponse".into(),"chosen".into())])).unwrap()}
    ]);
    (dir, expected)
}

#[test]
fn common_ir_runs_through_real_fastapi() {
    let check = Command::new("python3").args(["-c", "from importlib.metadata import version; assert (version('fastapi'),version('pydantic'),version('starlette')) == ('0.141.1','2.13.5','1.6.0')"]).output();
    let available = check.is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application FastAPI runtime",
        &if available {
            vec![]
        } else {
            vec!["pinned FastAPI runtime".into()]
        },
    );
    if !available {
        return;
    }
    let (dir, expected) = fixture(Adapter::Fastapi);
    fs::write(
        dir.path().join("run.py"),
        include_str!("application-runtime/fastapi.py"),
    )
    .unwrap();
    let mut command = Command::new("python3");
    command.current_dir(dir.path()).arg("run.py");
    assert_eq!(
        serde_json::from_str::<Value>(&success(command)).unwrap(),
        expected
    );
}

#[test]
fn common_ir_runs_through_real_go_http() {
    let available = Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application Go HTTP runtime",
        &if available {
            vec![]
        } else {
            vec!["Go HTTP toolchain".into()]
        },
    );
    if !available {
        return;
    }
    let (dir, expected) = fixture(Adapter::GoNetHttp);
    fs::write(
        dir.path().join("go.mod"),
        "module frapplicationfixture\n\ngo 1.22\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("routes_test.go"),
        include_str!("application-runtime/routes_test.go"),
    )
    .unwrap();
    let mut command = Command::new("go");
    command
        .current_dir(dir.path())
        .args(["test", "-v", "."])
        .env(
            "GOCACHE",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/go-cache"),
        );
    let output = success(command);
    let result = output
        .lines()
        .find_map(|line| line.strip_prefix("FR_APPLICATION_RESULT="))
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(result).unwrap(), expected);
}

#[test]
fn common_ir_compiles_and_runs_through_real_express() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let packages = root.join("tests/application-runtime/express/node_modules");
    let compiler = root.join("tests/typesafety/typescript/node_modules/.bin/tsc");
    let available = packages.join("express/package.json").is_file()
        && compiler.is_file()
        && std::net::TcpListener::bind(("127.0.0.1", 0)).is_ok();
    common::require_on_ci(
        "application Express runtime",
        &if available {
            vec![]
        } else {
            vec!["pinned Express and TypeScript with a local socket".into()]
        },
    );
    if !available {
        return;
    }
    let (dir, expected) = fixture(Adapter::Express);
    fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(packages, dir.path().join("node_modules")).unwrap();
    fs::write(
        dir.path().join("run.mjs"),
        include_str!("application-runtime/express.mjs"),
    )
    .unwrap();
    let mut command = Command::new(compiler);
    command.current_dir(dir.path()).args([
        "routes.ts",
        "--target",
        "ES2022",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        "--strict",
        "--skipLibCheck",
        "--outDir",
        "build",
    ]);
    success(command);
    let mut command = Command::new("node");
    command.current_dir(dir.path()).arg("run.mjs");
    assert_eq!(
        serde_json::from_str::<Value>(&success(command)).unwrap(),
        expected
    );
}
