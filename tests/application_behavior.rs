mod common;

use fun_refactor::application_ir::{write_routes, Adapter, HttpRoute};
use serde_json::{json, Value};
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

const TS_NODE: &str = "pinned TypeScript compiler and Node runtime";
const EXPRESS_TS: &str = "pinned Express and TypeScript with a local socket";

fn fastapi_runtime_available() -> bool {
    Command::new("python3")
        .args(["-c", "from importlib.metadata import version; assert (version('fastapi'),version('pydantic'),version('starlette')) == ('0.141.1','2.13.5','1.6.0')"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn behavior_routes() -> (
    Vec<HttpRoute>,
    Vec<fun_refactor::application_ir::HttpMiddleware>,
) {
    let routes = serde_json::from_value(json!([
        {"method":"GET", "path":"/guarded", "status":200,
         "dependencies":[{"binding":"session","provider":"load_session","security":true}],
         "response":{"kind":"literal","value":true}}
    ]))
    .unwrap();
    let middleware = serde_json::from_value(json!([
        {"name":"mark","request_order":1},
        {"name":"audit","request_order":2}
    ]))
    .unwrap();
    (routes, middleware)
}

fn behavior_write(dir: &Path, adapter: Adapter) {
    let (routes, middleware) = behavior_routes();
    for (path, source) in write_routes(&routes, &middleware, adapter).unwrap() {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }
}

#[test]
fn middleware_and_dependencies_run_through_real_adapters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let _behavior = root.join("tests/application-runtime/behavior");

    let fastapi = fastapi_runtime_available();
    common::require_on_ci(
        "application FastAPI dependency runtime",
        &if fastapi {
            vec![]
        } else {
            vec!["pinned FastAPI runtime".into()]
        },
    );
    if fastapi {
        let dir = tempfile::tempdir().unwrap();
        behavior_write(dir.path(), Adapter::Fastapi);
        fs::write(
            dir.path().join("fr_dependencies.py"),
            include_str!("application-runtime/behavior/fr_dependencies.py"),
        )
        .unwrap();
        fs::write(
            dir.path().join("run.py"),
            include_str!("application-runtime/behavior/fastapi.py"),
        )
        .unwrap();
        let mut command = Command::new("python3");
        command.current_dir(dir.path()).arg("run.py");
        let result: Value = serde_json::from_str(&success(command)).unwrap();
        assert_eq!(result[0]["status"], 401);
        assert_eq!(result[1], json!({"status": 200, "body": true}));
    }

    let go_available = Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application Go middleware runtime",
        &if go_available {
            vec![]
        } else {
            vec!["Go HTTP toolchain".into()]
        },
    );
    if go_available {
        let dir = tempfile::tempdir().unwrap();
        behavior_write(dir.path(), Adapter::GoNetHttp);
        fs::write(
            dir.path().join("go.mod"),
            "module frapplicationfixture\n\ngo 1.22\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("hosts.go"),
            include_str!("application-runtime/behavior/hosts.go"),
        )
        .unwrap();
        fs::write(
            dir.path().join("behavior_test.go"),
            include_str!("application-runtime/behavior/behavior_test.go"),
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
        assert_eq!(
            serde_json::from_str::<Value>(result).unwrap(),
            json!([
                {"status":401,"body":{"error":"dependency","provider":"session"},"order":["mark","audit"]},
                {"status":200,"body":true,"order":["mark","audit"]}
            ])
        );
    }

    let packages = root.join("tests/application-runtime/express/node_modules");
    let compiler = root.join("tests/typesafety/typescript/node_modules/.bin/tsc");
    let node_available = compiler.is_file()
        && Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
    let express_available = packages.join("express/package.json").is_file() && node_available;
    common::require_on_ci(
        "application Express middleware runtime",
        &if express_available {
            vec![]
        } else {
            vec![EXPRESS_TS.into()]
        },
    );
    if express_available {
        let dir = tempfile::tempdir().unwrap();
        behavior_write(dir.path(), Adapter::Express);
        fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(packages, dir.path().join("node_modules")).unwrap();
        fs::write(
            dir.path().join("fr-middleware.ts"),
            include_str!("application-runtime/behavior/fr-middleware.ts"),
        )
        .unwrap();
        fs::write(
            dir.path().join("fr-dependencies.ts"),
            include_str!("application-runtime/behavior/fr-dependencies.ts"),
        )
        .unwrap();
        fs::write(
            dir.path().join("run.mjs"),
            include_str!("application-runtime/behavior/express.mjs"),
        )
        .unwrap();
        let mut command = Command::new(&compiler);
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
            json!([
                {"status":401,"body":{"error":"dependency","provider":"session"},"order":"mark, audit"},
                {"status":200,"body":true,"order":"mark, audit"}
            ])
        );
    }

    common::require_on_ci(
        "application Next.js middleware runtime",
        &if node_available {
            vec![]
        } else {
            vec![TS_NODE.into()]
        },
    );
    if node_available {
        let dir = tempfile::tempdir().unwrap();
        behavior_write(dir.path(), Adapter::Nextjs);
        fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
        let shim = dir.path().join("node_modules/next");
        fs::create_dir_all(&shim).unwrap();
        for (name, contents) in [
            (
                "package.json",
                include_str!("application-runtime/behavior/next-shim/package.json"),
            ),
            (
                "server.js",
                include_str!("application-runtime/behavior/next-shim/server.js"),
            ),
            (
                "server.d.ts",
                include_str!("application-runtime/behavior/next-shim/server.d.ts"),
            ),
        ] {
            fs::write(shim.join(name), contents).unwrap();
        }
        fs::write(
            dir.path().join("fr-middleware.ts"),
            include_str!("application-runtime/behavior/fr-middleware-next.ts"),
        )
        .unwrap();
        fs::write(
            dir.path().join("fr-dependencies.ts"),
            include_str!("application-runtime/behavior/fr-dependencies.ts"),
        )
        .unwrap();
        fs::write(
            dir.path().join("run.mjs"),
            include_str!("application-runtime/behavior/next.mjs"),
        )
        .unwrap();
        let mut command = Command::new(&compiler);
        command.current_dir(dir.path()).args([
            "middleware.ts",
            "guarded/route.ts",
            "fr-middleware.ts",
            "fr-dependencies.ts",
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
            json!({
                "order": ["mark", "audit"],
                "middlewareStatus": 200,
                "denied": 401,
                "allowed": 200,
                "allowedBody": true
            })
        );
    }
}

fn service_routes() -> Vec<HttpRoute> {
    serde_json::from_value(json!([
        {"method":"GET", "path":"/records", "status":200,
         "response":{"kind":"object","fields":{"items":{"kind":"array","items":[
            {"kind":"literal","value":"a"},{"kind":"literal","value":"b"}]}}}},
        {"method":"GET", "path":"/feed", "status":200,
         "response":{"kind":"service","method":"GET","path":"/records"}}
    ]))
    .unwrap()
}

#[test]
fn service_calls_forward_through_real_adapters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let expected = json!([
        {"status":200,"body":{"items":["a","b"]}},
        {"status":200,"body":{"items":["a","b"]}}
    ]);

    let go_available = Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application Go service-call runtime",
        &if go_available {
            vec![]
        } else {
            vec!["Go HTTP toolchain".into()]
        },
    );
    if go_available {
        let dir = tempfile::tempdir().unwrap();
        let routes = service_routes();
        for (path, source) in write_routes(&routes, &[], Adapter::GoNetHttp).unwrap() {
            fs::write(dir.path().join(path), source).unwrap();
        }
        fs::write(
            dir.path().join("go.mod"),
            "module frapplicationfixture\n\ngo 1.22\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("service_test.go"),
            include_str!("application-runtime/behavior/service_test.go"),
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

    let packages = root.join("tests/application-runtime/express/node_modules");
    let compiler = root.join("tests/typesafety/typescript/node_modules/.bin/tsc");
    let express_available = packages.join("express/package.json").is_file()
        && compiler.is_file()
        && std::net::TcpListener::bind(("127.0.0.1", 0)).is_ok();
    common::require_on_ci(
        "application Express service-call runtime",
        &if express_available {
            vec![]
        } else {
            vec![EXPRESS_TS.into()]
        },
    );
    if express_available {
        let dir = tempfile::tempdir().unwrap();
        let routes = service_routes();
        for (path, source) in write_routes(&routes, &[], Adapter::Express).unwrap() {
            fs::write(dir.path().join(path), source).unwrap();
        }
        fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&packages, dir.path().join("node_modules")).unwrap();
        fs::write(
            dir.path().join("run.mjs"),
            include_str!("application-runtime/behavior/service.mjs"),
        )
        .unwrap();
        let mut command = Command::new(&compiler);
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

    let next_available = compiler.is_file()
        && Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
    common::require_on_ci(
        "application Next.js service-call runtime",
        &if next_available {
            vec![]
        } else {
            vec![TS_NODE.into()]
        },
    );
    if next_available {
        let dir = tempfile::tempdir().unwrap();
        let routes = service_routes();
        for (path, source) in write_routes(&routes, &[], Adapter::Nextjs).unwrap() {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}\n").unwrap();
        fs::write(
            dir.path().join("run.mjs"),
            include_str!("application-runtime/behavior/service-next.mjs"),
        )
        .unwrap();
        let mut command = Command::new(&compiler);
        command.current_dir(dir.path()).args([
            "records/route.ts",
            "feed/route.ts",
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
}
