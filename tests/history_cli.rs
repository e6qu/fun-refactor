use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (success, report) = run(root, args);
    assert!(success, "{args:?}: {report}");
    report
}

#[test]
fn agent_can_save_inspect_apply_undo_redo_across_cli_processes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.rs");
    let original = "fn helper() {}\nfn main() { helper(); }\n";
    fs::write(&path, original).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
    ok(dir.path(), &["rename", "helper", "renamed"]);
    assert!(!dir.path().join(".fr-history").exists());
    let plan = ok(dir.path(), &["rename", "helper", "renamed", "--save-plan"]);
    assert_eq!(plan["applied"], false);
    let id = plan["transaction"].as_u64().unwrap().to_string();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let shown = ok(dir.path(), &["history", "show", &id]);
    assert_eq!(shown["records"][0]["status"], "planned");
    assert_eq!(shown["records"][0]["validation"], "reparse-strict");
    assert!(shown["records"][0]["changes"][0]["diff"]
        .as_str()
        .unwrap()
        .contains("renamed"));
    ok(dir.path(), &["history", "apply", &id]);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    ok(dir.path(), &["history", "apply", &id, "--write"]);
    let updated = fs::read_to_string(&path).unwrap();
    assert!(updated.contains("renamed"));
    fs::write(dir.path().join("unrelated.rs"), "fn dirty() {}\n").unwrap();
    ok(dir.path(), &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o751
    );
    ok(dir.path(), &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), updated);
    let scan = ok(dir.path(), &["scan", "--no-ignore"]);
    assert!(!scan.to_string().contains(".fr-history"));
    assert_eq!(
        fs::read_to_string(dir.path().join("unrelated.rs")).unwrap(),
        "fn dirty() {}\n"
    );
}

#[test]
fn ordinary_writes_and_recipe_formatting_share_history() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("app.rs"), "fn helper() {}\n").unwrap();
    fs::write(
        dir.path().join("tidy.recipe"),
        "schema 1\nrecipe tidy { rename to \"renamed\" where name=\"helper\" kind=function }\n",
    )
    .unwrap();
    let first = ok(dir.path(), &["recipe", "tidy.recipe", "--write"]);
    assert_eq!(first["transaction"], 1);
    let before = fs::read_to_string(dir.path().join("tidy.recipe")).unwrap();
    let second = ok(dir.path(), &["recipe", "fmt", "tidy.recipe", "--write"]);
    assert_eq!(second["transaction"], 2);
    assert_eq!(
        ok(dir.path(), &["history", "show", "2"])["records"][0]["validation"],
        "recipe-parser"
    );
    assert!(!run(dir.path(), &["history", "undo", "1", "--write"]).0);
    ok(dir.path(), &["history", "undo", "2", "--write"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("tidy.recipe")).unwrap(),
        before
    );
    ok(dir.path(), &["history", "undo", "1", "--write"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("app.rs")).unwrap(),
        "fn helper() {}\n"
    );
}

#[test]
fn generated_file_undo_removes_and_redo_recreates_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("style.css"), ".panel { color: red; }\n").unwrap();
    let generated = dir.path().join("style.scss");
    let report = ok(dir.path(), &["translate", "style.css", "scss", "--write"]);
    let id = report["transaction"].as_u64().unwrap().to_string();
    let text = fs::read_to_string(&generated).unwrap();
    ok(dir.path(), &["history", "undo", &id, "--write"]);
    assert!(!generated.exists());
    ok(dir.path(), &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(generated).unwrap(), text);
}

#[test]
fn save_plan_conflicts_and_failed_recipes_emit_one_error_without_history() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("app.rs"), "fn helper() {}\n").unwrap();
    for args in [
        vec!["scan", "--save-plan"],
        vec!["rename", "helper", "renamed", "--save-plan", "--write"],
    ] {
        let (success, report) = run(dir.path(), &args);
        assert!(!success);
        assert!(report["error"].is_object());
        assert!(!dir.path().join(".fr-history").exists());
    }
    fs::write(dir.path().join("bad.recipe"), "schema 1\nrecipe bad { rename to \"renamed\" where name=\"helper\" kind=function\nexpect changed exactly 10 files\n}\n").unwrap();
    assert!(!run(dir.path(), &["recipe", "bad.recipe", "--save-plan"]).0);
    assert!(!dir.path().join(".fr-history").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("app.rs")).unwrap(),
        "fn helper() {}\n"
    );
}

#[test]
fn openapi_output_uses_the_same_history() {
    let dir = tempfile::tempdir().unwrap();
    let routes = dir.path().join("app/api/ping");
    fs::create_dir_all(&routes).unwrap();
    fs::write(
        routes.join("route.ts"),
        "export async function GET() { return Response.json({ ok: true }); }\n",
    )
    .unwrap();
    let output = dir.path().join("openapi.json");
    let report = ok(dir.path(), &["openapi", "--out", output.to_str().unwrap()]);
    let id = report["transaction"].as_u64().unwrap().to_string();
    let document = fs::read_to_string(&output).unwrap();
    let parsed: Value = serde_json::from_str(&document).unwrap();
    assert!(!parsed["paths"].as_object().unwrap().is_empty());
    ok(dir.path(), &["history", "undo", &id, "--write"]);
    assert!(!output.exists());
    ok(dir.path(), &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(output).unwrap(), document);
}
