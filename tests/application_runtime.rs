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
         }}},
        {"method":"POST", "path":"/validated/{id}", "inputs":[
          {"name":"limit","source":"query","scalar":"integer"},
          {"name":"active","source":"query","scalar":"boolean"},
          {"name":"label","source":"query","scalar":"string"},
          {"name":"title","source":"json-body","scalar":"string"},
          {"name":"count","source":"json-body","scalar":"integer"},
          {"name":"approved","source":"json-body","scalar":"boolean"}
        ], "status":201, "response":{"kind":"object","fields":{
          "id":{"kind":"path","name":"id"},
          "limit":{"kind":"input","name":"limit"},
          "active":{"kind":"input","name":"active"},
          "label":{"kind":"input","name":"label"},
          "title":{"kind":"input","name":"title"},
          "count":{"kind":"input","name":"count"},
          "approved":{"kind":"input","name":"approved"}
        }}}
    ])).unwrap()
}

fn fixture(adapter: Adapter) -> (tempfile::TempDir, Value) {
    let dir = tempfile::tempdir().unwrap();
    let routes = routes();
    for (path, source) in write_routes(&routes, adapter).unwrap() {
        let path = dir.path().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }
    let cases = json!([
        {"method":"GET", "url":"/"},
        {"method":"POST", "url":"/records/chosen"},
        {"method":"POST", "url":"/validated/chosen?limit=12&active=false&label=tag", "body":{"title":"hello","count":3.0,"approved":true}},
        {"method":"POST", "url":"/validated/chosen?limit=01&active=yes", "body":{"title":1,"count":1.5,"approved":"yes"}},
        {"method":"POST", "url":"/validated/chosen?limit=1&limit=2&active=true&label=tag", "body":{"title":"hello","count":3,"approved":true}}
    ]);
    fs::write(dir.path().join("cases.json"), cases.to_string()).unwrap();
    let expected = json!([
        {"status":200,"body":routes[0].response.evaluate(&BTreeMap::new()).unwrap()},
        {"status":201,"body":routes[1].response.evaluate(&BTreeMap::from([("JSONResponse".into(),"chosen".into())])).unwrap()},
        {"status":201,"body":routes[2].response.evaluate_with_inputs(
            &BTreeMap::from([("id".into(),"chosen".into())]),
            &routes[2].validate_request(
                &BTreeMap::from([("limit".into(),"12".into()),("active".into(),"false".into()),("label".into(),"tag".into())]),
                Some(&json!({"title":"hello","count":3.0,"approved":true}))).unwrap()).unwrap()},
        {"status":422,"body":{"error":"validation","issues":[
            {"source":"query","name":"limit","expected":"integer"},
            {"source":"query","name":"active","expected":"boolean"},
            {"source":"query","name":"label","expected":"string"},
            {"source":"json-body","name":"title","expected":"string"},
            {"source":"json-body","name":"count","expected":"integer"},
            {"source":"json-body","name":"approved","expected":"boolean"}
        ]}},
        {"status":422,"body":{"error":"validation","issues":[
            {"source":"query","name":"limit","expected":"integer"}
        ]}}
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

#[test]
fn common_ir_compiles_and_runs_through_nextjs_route_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.join("tests/typesafety/typescript/node_modules/.bin/tsc");
    let available = compiler.is_file()
        && Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application Next.js route runtime",
        &if available {
            vec![]
        } else {
            vec!["pinned TypeScript compiler and Node runtime".into()]
        },
    );
    if !available {
        return;
    }
    let (dir, expected) = fixture(Adapter::Nextjs);
    fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    fs::create_dir_all(dir.path().join("validated/[id]")).unwrap();
    fs::write(
        dir.path().join("next.mjs"),
        include_str!("application-runtime/next.mjs"),
    )
    .unwrap();
    let mut command = Command::new(compiler);
    command.current_dir(dir.path()).args([
        "validated/[id]/route.ts",
        "--target",
        "ES2022",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        "--strict",
        "--outDir",
        "build",
    ]);
    success(command);
    let mut command = Command::new("node");
    command.current_dir(dir.path()).arg("next.mjs");
    assert_eq!(
        serde_json::from_str::<Value>(&success(command)).unwrap(),
        json!([
            expected[2].clone(),
            expected[3].clone(),
            expected[4].clone()
        ])
    );
}
