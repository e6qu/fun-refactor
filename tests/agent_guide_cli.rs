use clap::ValueEnum;
use fun_refactor::project::object_merkle;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(root: &Path, arguments: &[String], input: Option<&Value>) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(input).unwrap())
            .unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{arguments:?}: {error}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

fn goal(purpose: &str, selector: Value, operation: Value) -> Value {
    json!({"schema":"fr-agent-goal-1","purpose":purpose,"selector":selector,"operation":operation})
}

fn guide(root: &Path, goal: &Value) -> Value {
    let (passed, report) = run(
        root,
        &["guide".into(), "--from".into(), "-".into()],
        Some(goal),
    );
    assert!(passed, "{report}");
    report
}

fn follow(root: &Path, action: &Value) -> Value {
    assert_eq!(action["ready"], true, "{action}");
    assert_eq!(action["writes"], false);
    let arguments = action["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(!arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--write" | "--save-plan")));
    let (passed, report) = run(
        root,
        &arguments,
        action.get("input").filter(|value| !value.is_null()),
    );
    assert!(passed, "{arguments:?}: {report}");
    let field = action["schema_field"].as_str().unwrap();
    assert_eq!(
        report.get(field).unwrap_or(&Value::Null),
        &action["output_schema"]
    );
    report
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.rs"),"pub fn calculate(value: i64) -> i64 { value + 7 }\npub fn allowed(ok: bool) -> bool { ok }\n").unwrap();
    root
}

#[test]
fn migration_guide_uses_generic_feature_compatibility_in_both_directions() {
    for (source_path, source, destination, output_path) in [
        ("app/api/signals/route.ts", "export async function GET() { return Response.json({ healthy: true }); }\n", "fastapi", "converted/signals.py"),
        ("api.py", "from fastapi import FastAPI\napp = FastAPI()\n@app.get('/signals')\ndef signals():\n    return {'healthy': True}\n", "nextjs", "converted/app"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(source_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        let request = goal("migrate", json!({"path":source_path}), json!({"kind":"framework-migration","to":destination}));
        let report = guide(root.path(), &request);
        assert_eq!(report["state"], "ready", "{report}");
        assert!(report["route"]["evidence"]["compatible_feature_count"].as_u64().unwrap() > 0);
        follow(root.path(), &report["actions"][0]);
        let feature = report["route"]["evidence"]["compatible_features"][0].as_str().unwrap();
        let (passed, preview) = run(root.path(), &args_for(&report["actions"][1], &[("<feature-id>",feature),("<destination>",output_path)]), None);
        assert!(passed, "{preview}");
        assert_eq!(preview["schema"], report["actions"][1]["output_schema"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        assert!(!root.path().join(output_path).exists());
        let same = if destination == "fastapi" { "nextjs" } else { "fastapi" };
        let refused = guide(root.path(), &goal("migrate", json!({"path":source_path}), json!({"kind":"framework-migration","to":same})));
        assert_eq!(refused["state"], "unsupported");
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("api.ts"), "import express from 'express';\nconst app = express();\napp.get('/signals', (_req, res) => res.json({healthy: true}));\n").unwrap();
    let refused = guide(
        root.path(),
        &goal(
            "migrate",
            json!({"path":"api.ts"}),
            json!({"kind":"framework-migration","to":"fastapi"}),
        ),
    );
    assert_eq!(refused["state"], "unsupported");
    assert_eq!(refused["actions"], json!([]));
}

#[test]
fn migration_guide_uses_the_application_ir_for_portable_backend_adapters() {
    let root = tempfile::tempdir().unwrap();
    let source = "function signal(req: Request, res: Response) {\n  return res.status(200).json({id: req.params['id']});\n}\napp.get('/signals/:id', signal);\n";
    std::fs::write(root.path().join("api.ts"), source).unwrap();
    let report = guide(
        root.path(),
        &goal(
            "migrate",
            json!({"path":"api.ts"}),
            json!({"kind":"framework-migration","to":"go-net-http"}),
        ),
    );
    assert_eq!(report["state"], "needs-authoring", "{report}");
    assert_eq!(report["route"]["evidence"]["planner"], "application-ir");
    assert_eq!(report["route"]["evidence"]["portable_feature_count"], 1);
    assert_eq!(report["route"]["evidence"]["portable_route_count"], 1);
    assert_eq!(report["actions"].as_array().unwrap().len(), 1);
    let arguments = args_for(&report["actions"][0], &[("<destination>", "converted")]);
    let (passed, preview) = run(root.path(), &arguments, None);
    assert!(passed, "{preview}");
    assert_eq!(preview["schema"], "fr-application-migration-1");
    assert_eq!(preview["migration"]["source_kind"], "project-snapshot");
    assert_eq!(
        std::fs::read_to_string(root.path().join("api.ts")).unwrap(),
        source
    );
    assert!(!root.path().join("converted").exists());
}

#[test]
fn migration_guide_routes_static_react_components_through_application_ir() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"dependencies":{"react":"19.3.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("src/App.tsx"),
        "export default function App() { return <main><h1>Ready</h1></main>; }\n",
    )
    .unwrap();
    let report = guide(
        root.path(),
        &goal(
            "migrate",
            json!({"path":"src/App.tsx"}),
            json!({"kind":"framework-migration","to":"nextjs"}),
        ),
    );
    assert_eq!(report["state"], "needs-authoring", "{report}");
    assert_eq!(report["route"]["evidence"]["planner"], "application-ir");
    assert_eq!(report["route"]["evidence"]["portable_feature_count"], 1);
    assert_eq!(report["route"]["evidence"]["portable_route_count"], 0);
    let arguments = args_for(&report["actions"][0], &[("<destination>", "converted")]);
    let (passed, preview) = run(root.path(), &arguments, None);
    assert!(passed, "{preview}");
    assert_eq!(
        preview["migration"]["components"].as_array().unwrap().len(),
        1
    );
}

fn args_for(action: &Value, replacements: &[(&str, &str)]) -> Vec<String> {
    action["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|argument| {
            let value = argument.as_str().unwrap();
            replacements
                .iter()
                .find(|(placeholder, _)| *placeholder == value)
                .map_or(value, |(_, replacement)| *replacement)
                .to_owned()
        })
        .collect()
}

#[test]
fn recipe_guidance_exposes_only_live_targeted_forms_and_runs_the_authored_preview() {
    let root = fixture();
    let request = goal(
        "change",
        json!({"name":"allowed"}),
        json!({"kind":"recipe","verb":"rename"}),
    );
    let report = guide(root.path(), &request);
    assert_eq!(report["state"], "needs-authoring", "{report}");
    let contract = &report["route"]["evidence"]["author_contract"];
    assert_eq!(contract["file"]["schema_line"], "schema 1");
    assert_eq!(contract["file"]["open"], "recipe <lower-kebab-name> {");
    assert_eq!(contract["required_expectations"][0], "expect matched = 1");
    assert!(contract["template"]
        .as_str()
        .unwrap()
        .contains("where name=\"allowed\""));
    assert_eq!(contract["template_lines"][0], "schema 1");
    assert_eq!(contract["template_lines"][1], "recipe <lower-kebab-name> {");
    assert!(contract["template_lines"][2]
        .as_str()
        .unwrap()
        .contains("where name=\"allowed\""));
    assert_eq!(contract["target_values"]["name"], "allowed");
    assert_eq!(report["route"]["evidence"]["verb"]["name"], "rename");
    assert_eq!(
        contract["selector_fields"],
        json!(["name", "kind", "lang", "file"])
    );
    assert!(report["route"]["evidence"].get("verbs").is_none());
    std::fs::write(root.path().join("change.recipe"), "schema 1\nrecipe change {\nrename to \"permitted\" where kind=function name=\"allowed\" file=\"app.rs\" lang=rust\nexpect matched = 1\nexpect refusals = 0\n}\n").unwrap();
    let arguments = args_for(&report["actions"][0], &[("<recipe-file>", "change.recipe")]);
    let (passed, preview) = run(root.path(), &arguments, None);
    assert!(passed, "{preview}");
    assert_eq!(preview["schema"], report["actions"][0]["output_schema"]);
    assert!(!std::fs::read_to_string(root.path().join("app.rs"))
        .unwrap()
        .contains("permitted"));
    let refused = guide(
        root.path(),
        &goal(
            "change",
            json!({"path":"app.rs"}),
            json!({"kind":"recipe","verb":"rename"}),
        ),
    );
    assert_eq!(refused["state"], "unsupported");
}

#[test]
fn workspace_recipe_reveals_structure_before_authoring_a_real_source_target() {
    let root = fixture();
    let request = json!({"schema":"fr-agent-goal-1","purpose":"change","operation":{"kind":"recipe","verb":"restructure"},"constraints":{"allow_source":true}});
    let report = guide(root.path(), &request);
    assert_eq!(report["state"], "ready", "{report}");
    assert_eq!(report["actions"][0]["id"], "structure");
    follow(root.path(), &report["actions"][0]);
    let reveal = &report["actions"][1];
    assert_eq!(reveal["ready"], false);
    assert_eq!(reveal["author_fields"][0]["name"], "source-handle");
    let file = guide(
        root.path(),
        &goal(
            "understand",
            json!({"path":"app.rs"}),
            json!({"kind":"automatic"}),
        ),
    );
    let arguments = args_for(
        reveal,
        &[(
            "<source-handle>",
            file["target"]["handle"].as_str().unwrap(),
        )],
    );
    let (passed, source) = run(root.path(), &arguments, None);
    assert!(passed, "{source}");
    assert_eq!(source["schema"], reveal["output_schema"]);
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn nul_string_values_require_json_stdin_delivery_and_never_enter_argv() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.rs"),
        "pub fn label() -> &'static str { \"safe\" }\n",
    )
    .unwrap();
    let mut request = goal(
        "change",
        json!({"name":"label"}),
        json!({"kind":"semantic-scalar","operation":"set-string","from":"safe","to":"a\0b"}),
    );
    let refused = guide(root.path(), &request);
    assert_eq!(refused["state"], "unsupported", "{refused}");
    assert!(refused.to_string().contains("JSON stdin"));
    assert_eq!(refused["actions"], json!([]));
    std::fs::create_dir(root.path().join(".fr")).unwrap();
    std::fs::write(root.path().join(".fr/checks.json"), serde_json::to_vec(&json!({"schema":1,"checks":[{"name":"compiler","argv":["rustc","--version"],"cwd":".","timeout_seconds":30,"covers":["compiler identity"]}]})).unwrap()).unwrap();
    request["checks"] = json!(["compiler"]);
    let report = guide(root.path(), &request);
    assert_eq!(report["state"], "ready", "{report}");
    assert_eq!(report["alternatives"], json!([]));
    assert!(report["actions"][0]["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|argument| !argument.as_str().unwrap().contains('\0')));
    follow(root.path(), &report["actions"][0]);
    assert!(std::fs::read_to_string(root.path().join("app.rs"))
        .unwrap()
        .contains("safe"));
}

#[test]
fn capability_parameters_cannot_supply_execution_or_plan_persistence_options() {
    let root = fixture();
    for value in ["--write", "--save-plan", "--write=true", "--save-plan=true"] {
        let request = goal(
            "change",
            json!({"name":"allowed"}),
            json!({"kind":"capability","capability":"rename","parameters":{"new_name":value}}),
        );
        let (passed, report) = run(
            root.path(),
            &["guide".into(), "--from".into(), "-".into()],
            Some(&request),
        );
        assert!(!passed, "{report}");
        assert!(report.to_string().contains("persistence"), "{report}");
        assert!(!root.path().join(".fr-history").exists());
    }
}

#[test]
fn scalar_action_arrays_preserve_option_like_data_and_refuse_signed_scalar_categories() {
    for (source, operation, from, to) in [
        (
            "pub fn calculate(value: i64) -> i64 { value + 7 }\n",
            "set-int",
            "7",
            "-9",
        ),
        (
            "pub fn calculate() -> &'static str { \"--write\" }\n",
            "set-string",
            "--write",
            "--save-plan",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("app.rs"), source).unwrap();
        let request = goal(
            "change",
            json!({"name":"calculate"}),
            json!({"kind":"semantic-scalar","operation":operation,"from":from,"to":to}),
        );
        let report = guide(root.path(), &request);
        if operation == "set-int" {
            assert_eq!(report["state"], "unsupported", "{report}");
            assert!(
                report.to_string().contains("portable decimal integer"),
                "{report}"
            );
            assert_eq!(report["actions"], json!([]));
        } else {
            assert_eq!(
                report["route"]["evidence"]["scalar_contract"]["scalar"],
                "string"
            );
            assert_eq!(report["state"], "ready", "{report}");
            follow(root.path(), &report["actions"][0]);
            follow(root.path(), &report["actions"][1]);
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join("app.rs")).unwrap(),
            source
        );
        assert!(!root.path().join(".fr-history").exists());
    }
}

#[test]
fn guide_refuses_ambiguous_scalar_and_enforces_the_complete_packet_ceiling() {
    let root = fixture();
    std::fs::write(
        root.path().join("app.rs"),
        "pub fn calculate(value: i64) -> i64 { value + 7 + 7 }\n",
    )
    .unwrap();
    let request = goal(
        "change",
        json!({"name":"calculate"}),
        json!({"kind":"semantic-scalar","operation":"set-int","from":"7","to":"9"}),
    );
    let report = guide(root.path(), &request);
    assert_eq!(report["state"], "unsupported");
    assert_eq!(report["actions"], json!([]));
    let mut request = goal(
        "understand",
        json!({"name":"calculate"}),
        json!({"kind":"automatic"}),
    );
    request["context"] = json!({"token_limit":1024,"packet_limit":2048});
    let (passed, report) = run(
        root.path(),
        &["guide".into(), "--from".into(), "-".into()],
        Some(&request),
    );
    assert!(!passed);
    assert!(report.to_string().contains("packet"), "{report}");
}

#[test]
fn guide_resolves_names_and_compiles_source_free_evidence_deterministically() {
    let root = fixture();
    let request = goal(
        "understand",
        json!({"name":"calculate"}),
        json!({"kind":"automatic"}),
    );
    let report = guide(root.path(), &request);
    assert_eq!(report, guide(root.path(), &request));
    assert_eq!(report["state"], "ready");
    assert_eq!(report["route"]["id"], "evidence");
    assert!(report["target"]["handle"]
        .as_str()
        .unwrap()
        .starts_with("frp1:"));
    assert_eq!(report["target"]["language"], "rust");
    assert_eq!(
        report["serialized_bytes"].as_u64().unwrap() as usize,
        serde_json::to_vec(&report).unwrap().len()
    );
    let mut identity = report.clone();
    for field in ["basis", "object_root", "serialized_bytes"] {
        identity.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(report["object_root"], object_merkle(&identity).unwrap());
    let mut contracts = vec![
        run(
            root.path(),
            &["author".into(), "semantic-schema".into()],
            None,
        )
        .1,
    ];
    for section in fun_refactor::project::semantic_ir::Section::value_variants() {
        let name = section.to_possible_value().unwrap().get_name().to_owned();
        let (passed, contract) = run(
            root.path(),
            &["author".into(), "semantic-schema".into(), name],
            None,
        );
        assert!(passed, "{contract}");
        contracts.push(contract);
    }
    assert_eq!(
        report["catalog_digest"],
        object_merkle(&json!(contracts)).unwrap()
    );
    assert!(!report.to_string().contains("value + 7"));
    let evidence = follow(root.path(), &report["actions"][0]);
    assert_eq!(evidence["target"]["handle"], report["target"]["handle"]);
    assert!(!root.path().join(".fr-history").exists());
    std::fs::write(
        root.path().join("app.rs"),
        "pub fn calculate(value: i64) -> i64 { value + 8 }\n",
    )
    .unwrap();
    assert_ne!(report["basis"], guide(root.path(), &request)["basis"]);
}

#[test]
fn scalar_goal_returns_exact_plan_and_preview_without_mutation() {
    let root = fixture();
    let report = guide(
        root.path(),
        &goal(
            "change",
            json!({"name":"calculate"}),
            json!({"kind":"semantic-scalar","operation":"set-int","from":"7","to":"9"}),
        ),
    );
    assert_eq!(report["route"]["id"], "semantic-scalar");
    assert_eq!(report["route"]["admitted"], true);
    assert_eq!(report["intent_action"]["writable"], true);
    assert_eq!(report["intent_action"]["review"], "FrClient.review_guide");
    assert_eq!(report["intent_action"]["execute"], "FrClient.execute_guide");
    assert_eq!(report["delivery"]["schema"], "fr-guide-delivery-1");
    assert_eq!(report["delivery"]["review"], "FrClient.review_guide");
    assert_eq!(report["delivery"]["execute"], "FrClient.execute_guide");
    let plan = follow(root.path(), &report["actions"][0]);
    assert_eq!(plan["edit_plan"]["source_free"], true);
    let preview = follow(root.path(), &report["actions"][1]);
    assert!(preview.to_string().contains("+ 9"));
    assert!(!root.path().join(".fr-history").exists());
}

#[test]
fn direct_goal_derives_a_position_and_only_recommends_preview() {
    let root = fixture();
    let report = guide(
        root.path(),
        &goal(
            "change",
            json!({"name":"calculate"}),
            json!({"kind":"capability","capability":"rename","parameters":{"new_name":"compute"}}),
        ),
    );
    assert_eq!(report["route"]["admitted"], true);
    let preview = follow(root.path(), &report["actions"][0]);
    assert_eq!(preview["new_name"], "compute");
    assert_eq!(preview["applied"], false);
    assert_eq!(report["execution"]["admitted"], false);
    let file = guide(
        root.path(),
        &goal(
            "change",
            json!({"path":"app.rs"}),
            json!({"kind":"capability","capability":"rename","parameters":{"new_name":"compute"}}),
        ),
    );
    assert_eq!(file["intent_action"]["writable"], false);
    assert!(file.get("delivery").is_none());
    assert_eq!(file["state"], "unsupported");
    assert_eq!(file["actions"], json!([]));
}

#[test]
fn language_file_goals_have_executable_bounded_structure_actions() {
    let cases = [
        ("app.rs", "fn run() {}", "rust"),
        ("app.go", "package main\nfunc run() {}", "go"),
        ("app.zig", "pub fn run() void {}", "zig"),
        ("App.java", "class App { void run() {} }", "java"),
        ("app.ts", "function run(): void {}", "typescript"),
        ("app.js", "function run() {}", "typescript"),
        (
            "app.tsx",
            "export function App() { return <div />; }",
            "tsx",
        ),
        ("app.jsx", "function App() { return <div />; }", "tsx"),
        ("app.py", "def run():\n    pass\n", "python"),
        ("app.sh", "run() { :; }\n", "bash"),
        ("app.html", "<div id=\"app\"></div>", "html"),
        ("app.css", ".app { color: red; }", "css"),
        ("app.scss", "$color: red; .app { color: $color; }", "scss"),
        ("app.sass", "$color: red\n.app\n  color: $color\n", "sass"),
        ("app.tf", "variable \"app\" { default = \"ok\" }", "hcl"),
        ("app.json", "{\"app\":1}", "json"),
        ("app.yaml", "app: ok\n", "yaml"),
        ("app.tpl", "{{ .Values.app }}", "helm"),
        ("app.xml", "<app />", "xml"),
        (
            "app.md",
            "# App\n\n```mermaid\ngraph TD\nA-->B\n```\n",
            "markdown",
        ),
        ("App.lean", "def app : Bool := true\n", "lean"),
    ];
    for (path, source, language) in cases {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(path), source).unwrap();
        let report = guide(
            root.path(),
            &goal(
                "understand",
                json!({"path":path}),
                json!({"kind":"automatic"}),
            ),
        );
        assert_eq!(report["target"]["language"], language, "{path}: {report}");
        assert_eq!(report["state"], "ready", "{path}: {report}");
        follow(root.path(), &report["actions"][0]);
    }
}

#[test]
fn guide_refuses_ambiguous_cross_purpose_and_stronger_proof_claims() {
    let root = fixture();
    std::fs::write(root.path().join("other.rs"), "fn calculate() {}\n").unwrap();
    let ambiguous = guide(
        root.path(),
        &goal(
            "understand",
            json!({"name":"calculate"}),
            json!({"kind":"automatic"}),
        ),
    );
    assert_eq!(ambiguous["state"], "needs-selection");
    assert_eq!(ambiguous["candidates"].as_array().unwrap().len(), 2);
    let absent = guide(
        root.path(),
        &goal(
            "understand",
            json!({"name":"absent"}),
            json!({"kind":"automatic"}),
        ),
    );
    assert_eq!(absent["state"], "not-found");
    let wrong = guide(
        root.path(),
        &goal(
            "trace",
            json!({"name":"allowed"}),
            json!({"kind":"semantic-change"}),
        ),
    );
    assert_eq!(wrong["state"], "unsupported");
    assert_eq!(wrong["actions"], json!([]));
    let mut request = goal(
        "prove",
        json!({"name":"allowed"}),
        json!({"kind":"formalize"}),
    );
    request["proof"] = json!("implementation");
    let stronger = guide(root.path(), &request);
    assert_eq!(stronger["state"], "unsupported");
    assert!(stronger
        .to_string()
        .contains("general implementation correspondence"));
}

#[test]
fn formalization_and_surface_goals_load_only_relevant_workbenches() {
    let root = fixture();
    let formal = guide(
        root.path(),
        &goal(
            "prove",
            json!({"name":"allowed"}),
            json!({"kind":"formalize"}),
        ),
    );
    assert_eq!(formal["state"], "ready");
    assert_eq!(formal["reference"], "lean");
    follow(root.path(), &formal["actions"][0]);
    std::fs::write(root.path().join("app.css"), ".app { color: red; }\n").unwrap();
    let structural = guide(
        root.path(),
        &goal(
            "prove",
            json!({"path":"app.css"}),
            json!({"kind":"formalize"}),
        ),
    );
    assert_eq!(structural["state"], "ready", "{structural}");
    let property = follow(root.path(), &structural["actions"][0]);
    assert_eq!(property["target"]["symbol"], "__fr_structure__");
    let surface = guide(
        root.path(),
        &goal(
            "change",
            json!({"path":"app.css"}),
            json!({"kind":"surface-edit","surface":"styles"}),
        ),
    );
    assert_eq!(surface["state"], "ready");
    follow(root.path(), &surface["actions"][0]);
}

#[test]
fn source_required_routes_need_explicit_bounded_reveal_permission() {
    let root = fixture();
    let mut request = goal(
        "change",
        json!({"name":"calculate"}),
        json!({"kind":"capability","capability":"extract-variable","parameters":{"range":"app.rs:1:41-1:50","name":"result"}}),
    );
    assert_eq!(guide(root.path(), &request)["state"], "unsupported");
    request["constraints"] = json!({"allow_source":true});
    let report = guide(root.path(), &request);
    assert_eq!(report["state"], "ready");
    let source = follow(root.path(), &report["actions"][0]);
    assert!(source.to_string().contains("value + 7"));
}

#[test]
fn malformed_goals_refuse_before_project_history_creation() {
    let root = fixture();
    let base = goal(
        "understand",
        json!({"name":"calculate"}),
        json!({"kind":"automatic"}),
    );
    for request in [
        json!({"schema":"wrong","purpose":"understand"}),
        json!({"schema":"fr-agent-goal-1","purpose":"understand","extra":true}),
        json!({"schema":"fr-agent-goal-1","purpose":"change","operation":{"kind":"capability","capability":"invent"}}),
        json!({"schema":"fr-agent-goal-1","purpose":"understand","selector":{"name":"x","path":"app.rs"}}),
        {
            let mut request = base.clone();
            request["context"] = json!({"token_limit":4097});
            request
        },
    ] {
        let (passed, _) = run(
            root.path(),
            &["guide".into(), "--from".into(), "-".into()],
            Some(&request),
        );
        assert!(!passed, "{request}");
    }
    assert!(!root.path().join(".fr-history").exists());
}
