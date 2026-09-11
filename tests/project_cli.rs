use serde_json::Value;
use std::fs;
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

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/app.py"), "from lib import helper\n\nclass Service:\n    def run(self, name: str) -> str:\n        local = 'λ名'\n        return helper(name) + local\n\ndef check():\n    return Service().run('ok')\n").unwrap();
    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name: str) -> str:\n    return name\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# Example\n\nA service fixture.\n",
    )
    .unwrap();
    dir
}

fn reconstruct_context(mut compact: Value, reviewed: &Value) -> Value {
    assert_eq!(compact["context_basis"], reviewed["context_basis"]);
    assert_eq!(
        compact["context_omitted"],
        serde_json::json!(["coverage", "handle_prefix", "revision"])
    );
    compact.as_object_mut().unwrap().remove("context_omitted");
    for field in ["coverage", "handle_prefix", "revision"] {
        assert!(compact
            .as_object_mut()
            .unwrap()
            .insert(field.into(), reviewed[field].clone())
            .is_none());
    }
    compact
}

#[test]
fn context_basis_compacts_related_queries_and_rejects_stale_projects() {
    let dir = fixture();
    let reviewed = ok(dir.path(), &["project", "map"]);
    let basis = reviewed["context_basis"].as_str().unwrap();
    assert!(basis.starts_with("frcb1:"));

    let full = ok(dir.path(), &["project", "find", "run"]);
    let compact = ok(
        dir.path(),
        &["project", "find", "run", "--context-basis", basis],
    );
    assert!(compact.get("coverage").is_none());
    assert!(compact.get("handle_prefix").is_none());
    assert!(compact.get("revision").is_none());
    assert_eq!(reconstruct_context(compact, &reviewed), full);

    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name: str) -> str:\n    return name.upper()\n",
    )
    .unwrap();
    let (success, error) = run(
        dir.path(),
        &["project", "find", "run", "--context-basis", basis],
    );
    assert!(!success, "{error}");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale or conflicting context basis"));
}

fn project_batch(root: &Path, manifest: Value, report_bytes: usize) -> Value {
    let input = tempfile::NamedTempFile::new().unwrap();
    fs::write(input.path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
    ok(
        root,
        &[
            "project",
            "batch",
            "--from",
            input.path().to_str().unwrap(),
            "--report-bytes",
            &report_bytes.to_string(),
        ],
    )
}

fn project_task(root: &Path, manifest: Value, report_bytes: usize) -> (bool, Value) {
    let input = tempfile::NamedTempFile::new().unwrap();
    fs::write(input.path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
    run(
        root,
        &[
            "project",
            "task",
            "--from",
            input.path().to_str().unwrap(),
            "--report-bytes",
            &report_bytes.to_string(),
        ],
    )
}

fn task_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join(".fr")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "use std::fmt;\n\npub fn render(value: &str) -> String {\n    value.to_owned()\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".fr/checks.json"),
        r#"{"schema":1,"checks":[{"name":"unit","argv":["true"],"cwd":".","timeout_seconds":10,"covers":["render behavior"]}]}"#,
    )
    .unwrap();
    dir
}

#[test]
fn project_task_binds_queries_exact_targets_checks_and_delivery_templates() {
    let dir = task_fixture();
    let manifest = serde_json::json!({
        "schema": "fr-project-task-1",
        "requests": [
            {"id": "file", "arguments": ["map", "src/lib.rs", "--depth", "0", "--fields", "handle,kind,path"]},
            {"id": "target", "arguments": ["find", "render", "--signature", "--source", "--bytes", "2048"]},
            {"id": "calls", "arguments": ["calls", {"request": "target", "pointer": "/rows/0/0"}]}
        ],
        "targets": [
            {
                "id": "new-helper",
                "handle": {"request": "file", "pointer": "/rows/0/0"},
                "op": "insert-declaration"
            },
            {
                "id": "render-body",
                "handle": {"request": "target", "pointer": "/rows/0/0"},
                "op": "replace-body"
            }
        ],
        "checks": ["unit"],
        "delivery": {"exercise-reversal": true, "patch": "artifacts/change.patch"}
    });
    let (success, task) = project_task(dir.path(), manifest.clone(), 1_048_576);
    assert!(success, "{task}");
    assert_eq!(task["query"], "task");
    assert!(task["task_basis"].as_str().unwrap().starts_with("frpt1:"));
    assert!(task["task_resolution_basis"]
        .as_str()
        .unwrap()
        .starts_with("frpt2:"));
    assert_eq!(task["requests"].as_array().unwrap().len(), 3);
    assert_eq!(task["targets"][0]["language"], "rust");
    assert_eq!(task["targets"][0]["kind"], "file");
    assert_eq!(task["targets"][0]["operation"], "insert-declaration");
    assert_eq!(task["targets"][1]["kind"], "function");
    assert_eq!(task["targets"][1]["operation"], "replace-body");
    for target in task["targets"].as_array().unwrap() {
        assert_eq!(target["eligibility"], "target-supported");
        assert_eq!(target["syntax_preflighted"], false);
    }
    let handle = task["targets"][1]["handle"].as_str().unwrap();
    assert_eq!(
        task["author_manifest_template"]["operations"][1]["handle"],
        handle
    );
    assert_eq!(
        task["author_manifest_template"]["operations"][0]["from"],
        "<FRAGMENT:new-helper>"
    );
    assert_eq!(
        task["author_manifest_template"]["operations"][1]["from"],
        "<FRAGMENT:render-body>"
    );
    assert_eq!(task["checks"]["selected"], true);
    assert_eq!(task["checks"]["names"], serde_json::json!(["unit"]));
    assert_eq!(
        task["workflow_manifest_template"]["checks"]["basis"],
        task["checks"]["basis"]
    );
    assert_eq!(
        task["workflow_manifest_template"]["exercise-reversal"],
        true
    );
    assert_eq!(
        task["workflow_manifest_template"]["patch"]["output"],
        "artifacts/change.patch"
    );
    assert_eq!(
        task["task_basis"],
        project_task(dir.path(), manifest, 1_048_576).1["task_basis"]
    );
}

#[test]
fn project_task_refuses_invalid_targets_operations_checks_and_recursion() {
    let dir = task_fixture();
    let cases = [
        serde_json::json!({
            "schema": "old", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"bad id","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"rows/0/0"},"op":"replace-body"}]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"insert-declaration"}]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}],
            "checks": ["missing"]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"nested","arguments":["task","--from","other.json"]}],
            "targets": [{"id":"edit","handle":"frp1:bad:0","op":"replace-body"}]
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}],
            "delivery": {"patch":"../outside.patch"}
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}],
            "delivery": {"patch":"artifacts/change.patch"}
        }),
        serde_json::json!({
            "schema": "fr-project-task-1", "requests": [{"id":"target","arguments":["find","render"]}],
            "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}],
            "checks": ["unit"], "delivery": {"patch":".git/change.patch"}
        }),
    ];
    for manifest in cases {
        let (success, report) = project_task(dir.path(), manifest.clone(), 1_048_576);
        assert!(!success, "unexpected success for {manifest}: {report}");
    }
}

#[test]
fn project_task_refuses_a_target_whose_source_report_was_omitted() {
    let dir = task_fixture();
    let manifest = serde_json::json!({
        "schema": "fr-project-task-1",
        "requests": [{"id":"target","arguments":["find","render","--source","--bytes","65536"]}],
        "targets": [{"id":"edit","handle":{"request":"target","pointer":"/rows/0/0"},"op":"replace-body"}]
    });
    let (success, report) = project_task(dir.path(), manifest, 256);
    assert!(!success, "{report}");
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("omitted by the report budget"));
}

fn reconstruct_batch_report(batch: &Value, request: usize) -> Value {
    let mut report = batch["requests"][request]["report"].clone();
    for field in [
        "schema",
        "revision",
        "handle_prefix",
        "coverage",
        "context_basis",
    ] {
        report[field] = batch[field].clone();
    }
    report
}

#[test]
fn semantic_query_returns_complete_source_free_ir_and_patterns() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("app.py"),
        "def positive_names(names: list[str]) -> list[str]:\n    return [name.upper() for name in names if len(name) > 0]\n",
    )
    .unwrap();

    let summary = ok(dir.path(), &["project", "semantic", "app.py"]);
    assert_eq!(summary["semantic_schema"], "fr-semantic-model-1");
    assert_eq!(summary["source_policy"], "source-free");
    assert_eq!(summary["status"], "returned");
    assert_eq!(summary["selection"]["body_requested"], false);
    assert!(summary["model"]["items"][0]["value"].get("body").is_none());
    assert!(summary["omitted"]["bodies"].as_u64().unwrap() > 0);
    assert!(summary["semantic_basis"]
        .as_str()
        .unwrap()
        .starts_with("frsm1:"));
    assert_eq!(summary["patterns"], serde_json::json!([]));

    let full = ok(
        dir.path(),
        &["project", "semantic", "app.py", "--body", "--nodes", "64"],
    );
    assert_eq!(full["status"], "returned");
    assert!(full["model"]["items"][0]["value"]["body"]
        .as_array()
        .is_some_and(|body| !body.is_empty()));
    assert_eq!(full["patterns"][0]["pattern"], "filter-map");
    let address = full["patterns"][0]["address"].as_str().unwrap();
    assert!(address.starts_with(full["semantic_basis"].as_str().unwrap()));
    assert!(address.contains("#/model/"));
    let encoded = serde_json::to_string(&full["model"]).unwrap();
    assert!(!encoded.contains("positive_names(names"));

    let direct = ok(
        dir.path(),
        &[
            "project",
            "semantic",
            "app.py",
            "--declaration",
            "positive_names",
            "--body",
            "--minimal",
        ],
    );
    assert_eq!(direct["selection"]["kind"], "declaration");
    assert!(direct["selection"]["handle"]
        .as_str()
        .unwrap()
        .starts_with("frp1:"));
    assert!(direct.get("coverage").is_none());
    assert!(direct["report_omitted"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "coverage"));
}

#[test]
fn semantic_query_omits_the_whole_model_when_its_node_budget_is_too_small() {
    let dir = fixture();
    let report = ok(
        dir.path(),
        &[
            "project",
            "semantic",
            "src/app.py",
            "--body",
            "--nodes",
            "1",
        ],
    );
    assert_eq!(report["status"], "omitted-node-budget");
    assert!(report["model"].is_null());
    assert_eq!(report["patterns"], serde_json::json!([]));
    assert!(report["node_budget"]["required"].as_u64().unwrap() > 1);
    assert_eq!(report["node_budget"]["complete_subtrees_only"], true);
}

#[test]
fn semantic_query_is_available_inside_a_project_batch() {
    let dir = fixture();
    let direct = ok(dir.path(), &["project", "semantic", "src/lib.py", "--body"]);
    let batch = project_batch(
        dir.path(),
        serde_json::json!({
            "schema": "fr-project-batch-1",
            "requests": [{"id":"meaning", "arguments":["semantic", "src/lib.py", "--body"]}]
        }),
        65_536,
    );
    assert_eq!(reconstruct_batch_report(&batch, 0), direct);
}

#[test]
fn semantic_query_refuses_stale_handles_and_oversized_sources() {
    let dir = fixture();
    let found = ok(dir.path(), &["project", "find", "helper"]);
    let handle = found["rows"][0][0].as_str().unwrap().to_owned();
    fs::write(
        dir.path().join("src/lib.py"),
        "def helper():\n    return 2\n",
    )
    .unwrap();
    let (success, stale) = run(dir.path(), &["project", "semantic", &handle, "--body"]);
    assert!(!success, "{stale}");
    assert!(stale["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale or invalid project handle"));

    fs::write(dir.path().join("huge.py"), "# x\n".repeat(70_000)).unwrap();
    let (success, huge) = run(dir.path(), &["project", "semantic", "huge.py"]);
    assert!(!success, "{huge}");
    assert!(huge["error"]["message"]
        .as_str()
        .unwrap()
        .contains("262144-byte analysis limit"));
}

#[test]
fn project_batch_reuses_one_verified_context_for_existing_queries() {
    let dir = fixture();
    let manifest = serde_json::json!({
        "schema": "fr-project-batch-1",
        "requests": [
            {"id": "structure", "arguments": ["map", "src", "--depth", "2", "--limit", "8"]},
            {"id": "symbols", "arguments": ["select", "run", "helper", "--signature", "--source", "--bytes", "64"]},
            {"id": "packages", "arguments": ["packages", "--limit", "4"]}
        ]
    });
    let batch = project_batch(dir.path(), manifest.clone(), 1_048_576);
    assert_eq!(batch["query"], "batch");
    assert!(batch["manifest_basis"]
        .as_str()
        .unwrap()
        .starts_with("frpqb1:"));
    assert_eq!(batch["requests"].as_array().unwrap().len(), 3);
    assert_eq!(batch["report_budget"]["omitted_requests"], 0);
    for request in batch["requests"].as_array().unwrap() {
        assert_eq!(request["status"], "returned");
        for common in [
            "schema",
            "revision",
            "handle_prefix",
            "coverage",
            "context_basis",
        ] {
            assert!(request["report"].get(common).is_none());
        }
    }
    assert!(batch["resolution_basis"]
        .as_str()
        .unwrap()
        .starts_with("frpqb2:"));
    assert_eq!(
        reconstruct_batch_report(&batch, 0),
        ok(
            dir.path(),
            &["project", "map", "src", "--depth", "2", "--limit", "8"]
        )
    );
    assert_eq!(
        reconstruct_batch_report(&batch, 1),
        ok(
            dir.path(),
            &[
                "project",
                "select",
                "run",
                "helper",
                "--signature",
                "--source",
                "--bytes",
                "64"
            ]
        )
    );
    assert_eq!(
        reconstruct_batch_report(&batch, 2),
        ok(dir.path(), &["project", "packages", "--limit", "4"])
    );
    assert_eq!(
        batch["manifest_basis"],
        project_batch(dir.path(), manifest, 1_048_576)["manifest_basis"]
    );
}

#[test]
fn project_batch_budget_omits_only_whole_reports_and_keeps_later_small_queries() {
    let dir = fixture();
    let unlimited = project_batch(
        dir.path(),
        serde_json::json!({
            "schema": "fr-project-batch-1",
            "requests": [
                {"id": "large", "arguments": ["map", ".", "--depth", "8", "--limit", "500", "--fields", "handle,parent,kind,name,path,line,children,depth,language,exported,signature,qualifier"]},
                {"id": "small", "arguments": ["find", "absent", "--limit", "1"]}
            ]
        }),
        1_048_576,
    );
    let small_bytes = serde_json::to_vec(&unlimited["requests"][1]["report"])
        .unwrap()
        .len();
    let batch = project_batch(
        dir.path(),
        serde_json::json!({
            "schema": "fr-project-batch-1",
            "requests": [
                {"id": "large", "arguments": ["map", ".", "--depth", "8", "--limit", "500", "--fields", "handle,parent,kind,name,path,line,children,depth,language,exported,signature,qualifier"]},
                {"id": "small", "arguments": ["find", "absent", "--limit", "1"]}
            ]
        }),
        small_bytes,
    );
    assert_eq!(batch["requests"][0]["status"], "omitted-report-budget");
    assert!(batch["requests"][0].get("report").is_none());
    assert_eq!(batch["requests"][1]["status"], "returned");
    assert_eq!(batch["report_budget"]["returned_bytes"], small_bytes);
    assert_eq!(batch["report_budget"]["omitted_requests"], 1);
}

#[test]
fn project_batch_references_prior_string_results_even_when_the_source_report_is_omitted() {
    let dir = fixture();
    let manifest = serde_json::json!({
        "schema": "fr-project-batch-1",
        "requests": [
            {"id": "lookup", "arguments": ["find", "run", "--source", "--bytes", "65536"]},
            {"id": "inspect", "arguments": [
                "show", {"request": "lookup", "pointer": "/rows/0/0"}
            ]}
        ]
    });
    let unlimited = project_batch(dir.path(), manifest.clone(), 1_048_576);
    let handle = unlimited["requests"][0]["report"]["rows"][0][0]
        .as_str()
        .unwrap();
    assert_eq!(
        reconstruct_batch_report(&unlimited, 1),
        ok(dir.path(), &["project", "show", handle])
    );
    let inspect_bytes = serde_json::to_vec(&unlimited["requests"][1]["report"])
        .unwrap()
        .len();
    assert!(
        serde_json::to_vec(&unlimited["requests"][0]["report"])
            .unwrap()
            .len()
            > inspect_bytes
    );

    let bounded = project_batch(dir.path(), manifest, inspect_bytes);
    assert_eq!(bounded["requests"][0]["status"], "omitted-report-budget");
    assert_eq!(bounded["requests"][1]["status"], "returned");
    assert_eq!(bounded["resolution_basis"], unlimited["resolution_basis"]);
    assert_eq!(
        reconstruct_batch_report(&bounded, 1),
        ok(dir.path(), &["project", "show", handle])
    );
}

#[test]
fn project_batch_references_are_backward_string_only_and_bounded() {
    let dir = fixture();
    let cases = [
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "first", "arguments": ["show", {"request": "later", "pointer": "/root"}]},
            {"id": "later", "arguments": ["map"]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "map", "arguments": ["map"]},
            {"id": "bad-pointer", "arguments": ["show", {"request": "map", "pointer": "root"}]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "map", "arguments": ["map"]},
            {"id": "missing", "arguments": ["show", {"request": "map", "pointer": "/not-there"}]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "map", "arguments": ["map"]},
            {"id": "object", "arguments": ["show", {"request": "map", "pointer": "/page"}]}
        ]}),
    ];
    for manifest in cases {
        let input = tempfile::NamedTempFile::new().unwrap();
        fs::write(input.path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            !run(
                dir.path(),
                &["project", "batch", "--from", input.path().to_str().unwrap()]
            )
            .0,
            "{manifest}"
        );
    }
}

#[test]
fn project_batch_refuses_ambiguous_or_recursive_manifests_before_reporting() {
    let dir = fixture();
    let cases = [
        serde_json::json!({"schema": "old", "requests": [{"id": "one", "arguments": ["map"]}]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": []}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "same", "arguments": ["map"]}, {"id": "same", "arguments": ["gaps"]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "bad id", "arguments": ["map"]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "nested", "arguments": ["batch", "--from", "other.json"]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "global", "arguments": ["--context-basis", "x", "map"]}
        ]}),
        serde_json::json!({"schema": "fr-project-batch-1", "requests": [
            {"id": "unknown", "arguments": ["no-such-query"]}
        ]}),
    ];
    for manifest in cases {
        let input = tempfile::NamedTempFile::new().unwrap();
        fs::write(input.path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            !run(
                dir.path(),
                &["project", "batch", "--from", input.path().to_str().unwrap()]
            )
            .0,
            "{manifest}"
        );
    }
    for budget in ["0", "255", "1048577"] {
        let input = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            input.path(),
            r#"{"schema":"fr-project-batch-1","requests":[{"id":"one","arguments":["map"]}]}"#,
        )
        .unwrap();
        assert!(
            !run(
                dir.path(),
                &[
                    "project",
                    "batch",
                    "--from",
                    input.path().to_str().unwrap(),
                    "--report-bytes",
                    budget
                ]
            )
            .0
        );
    }
}

#[test]
fn select_returns_several_exact_symbols_with_one_context_and_source_budget() {
    let dir = fixture();
    let reviewed = ok(dir.path(), &["project", "map"]);
    let basis = reviewed["context_basis"].as_str().unwrap();
    let args = [
        "project",
        "select",
        "run",
        "helper",
        "local",
        "missing",
        "--signature",
        "--source",
        "--bytes",
        "64",
    ];
    let full = ok(dir.path(), &args);
    assert_eq!(full["query"], "select");
    assert_eq!(full["page"]["total"], 2);
    assert_eq!(full["columns"][0], "request");
    assert_eq!(full["selections"][0]["status"], "matched");
    assert_eq!(full["selections"][0]["total"], 1);
    assert_eq!(full["selections"][1]["status"], "matched");
    assert_eq!(full["selections"][2]["status"], "matching-locals-omitted");
    assert_eq!(full["selections"][2]["matching_locals_omitted"], 1);
    assert_eq!(full["selections"][3]["status"], "no-indexed-match");
    let selected = rows(&full);
    assert_eq!(selected[0]["request"], "run");
    assert_eq!(selected[0]["name"], "run");
    assert_eq!(selected[1]["request"], "helper");
    assert_eq!(selected[1]["name"], "helper");
    let returned = selected
        .iter()
        .map(|row| row["source"]["returned_bytes"].as_u64().unwrap())
        .sum::<u64>();
    assert_eq!(full["source_budget"]["returned_bytes"], returned);
    assert!(returned <= 64);

    let mut compact_args = args.to_vec();
    compact_args.extend(["--context-basis", basis]);
    let compact = ok(dir.path(), &compact_args);
    assert_eq!(reconstruct_context(compact, &reviewed), full);
}

#[test]
fn select_accepts_exact_handles_and_reports_handle_boundaries() {
    let dir = fixture();
    let run_handle = ok(dir.path(), &["project", "find", "run"])["rows"][0][0]
        .as_str()
        .unwrap()
        .to_owned();
    let helper = ok(dir.path(), &["project", "find", "helper"])["rows"][0][0]
        .as_str()
        .unwrap()
        .to_owned();
    let local = ok(dir.path(), &["project", "find", "local", "--locals"])["rows"][0][0]
        .as_str()
        .unwrap()
        .to_owned();

    let selected = ok(
        dir.path(),
        &[
            "project",
            "select",
            &run_handle,
            &helper,
            "--source",
            "--bytes",
            "65536",
        ],
    );
    assert_eq!(selected["page"]["total"], 2);
    assert_eq!(selected["match"]["mode"], "exact-name-or-handle");
    assert_eq!(selected["selections"][0]["kind"], "handle");
    assert_eq!(selected["selections"][1]["kind"], "handle");
    assert!(selected["selections"]
        .as_array()
        .unwrap()
        .iter()
        .all(|selection| selection["status"] == "matched"));
    assert_eq!(rows(&selected)[0]["handle"], run_handle);
    assert_eq!(rows(&selected)[1]["handle"], helper);
    assert!(rows(&selected).iter().all(|row| row["source"]["text"]
        .as_str()
        .is_some_and(|text| !text.is_empty())));

    let mixed = ok(dir.path(), &["project", "select", &run_handle, "helper"]);
    assert_eq!(mixed["page"]["total"], 2);
    assert_eq!(mixed["selections"][0]["kind"], "handle");
    assert_eq!(mixed["selections"][1]["kind"], "name");

    let omitted = ok(dir.path(), &["project", "select", &local]);
    assert_eq!(
        omitted["selections"][0]["status"],
        "matching-locals-omitted"
    );
    assert_eq!(omitted["selections"][0]["matching_locals_omitted"], 1);
    let included = ok(dir.path(), &["project", "select", &local, "--locals"]);
    assert_eq!(included["selections"][0]["status"], "matched");

    let app_map = ok(
        dir.path(),
        &[
            "project",
            "map",
            "src/app.py",
            "--depth",
            "1",
            "--fields",
            "handle,name",
        ],
    );
    let file = app_map["root"].as_str().unwrap();
    let non_declaration = ok(dir.path(), &["project", "select", file]);
    assert_eq!(
        non_declaration["selections"][0]["status"],
        "not-a-declaration"
    );
    let outside = ok(
        dir.path(),
        &["project", "select", &helper, "--in", "src/app.py"],
    );
    assert_eq!(outside["selections"][0]["status"], "outside-scope");

    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name: str) -> str:\n    return name.upper()\n",
    )
    .unwrap();
    let (success, stale) = run(dir.path(), &["project", "select", &helper]);
    assert!(!success, "{stale}");
    assert!(stale["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale or invalid project handle"));
}

#[test]
fn select_pages_the_combined_result_and_binds_every_selection_option() {
    let dir = fixture();
    let first = ok(
        dir.path(),
        &["project", "select", "helper", "run", "--limit", "1"],
    );
    assert_eq!(first["page"]["total"], 2);
    assert_eq!(first["selections"][0]["returned"], 1);
    assert_eq!(first["selections"][1]["returned"], 0);
    let cursor = first["page"]["next"].as_str().unwrap();
    let second = ok(
        dir.path(),
        &[
            "project", "select", "helper", "run", "--limit", "1", "--cursor", cursor,
        ],
    );
    assert_eq!(second["selections"][0]["returned"], 0);
    assert_eq!(second["selections"][1]["returned"], 1);
    assert_eq!(rows(&second)[0]["request"], "run");

    for args in [
        vec!["project", "select", "run", "helper", "--cursor", cursor],
        vec![
            "project",
            "select",
            "helper",
            "run",
            "--limit",
            "1",
            "--signature",
            "--cursor",
            cursor,
        ],
        vec![
            "project", "select", "helper", "run", "--limit", "1", "--locals", "--cursor", cursor,
        ],
        vec![
            "project", "select", "helper", "run", "--limit", "1", "--source", "--cursor", cursor,
        ],
    ] {
        assert!(!run(dir.path(), &args).0, "{args:?}");
    }
    assert!(!run(dir.path(), &["project", "select", "run", "run"]).0);
    assert!(!run(dir.path(), &["project", "select", ""]).0);
    assert!(!run(dir.path(), &["project", "select", "run", &"x".repeat(4096)]).0);

    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name: str) -> str:\n    return name.upper()\n",
    )
    .unwrap();
    assert!(
        !run(
            dir.path(),
            &["project", "select", "helper", "run", "--limit", "1", "--cursor", cursor,]
        )
        .0
    );
}

#[test]
fn find_source_matches_show_and_keeps_default_lookup_metadata() {
    let dir = fixture();
    let args = [
        "project",
        "find",
        "run",
        "--in",
        "src/app.py",
        "--signature",
    ];
    let plain = ok(dir.path(), &args);
    let mut selected = args.to_vec();
    selected.extend(["--source", "--bytes", "65536"]);
    let mut found = ok(dir.path(), &selected);
    let row = rows(&found).remove(0);
    let shown = ok(
        dir.path(),
        &[
            "project",
            "show",
            row["handle"].as_str().unwrap(),
            "--source",
            "--bytes",
            "65536",
        ],
    );
    assert_eq!(row["source"], shown["source"]);
    assert!(!row["source"]["text"]
        .as_str()
        .unwrap()
        .contains("def check"));
    assert_eq!(
        found["source_budget"]["returned_bytes"],
        row["source"]["returned_bytes"]
    );
    found.as_object_mut().unwrap().remove("source_budget");
    assert_eq!(
        found["columns"].as_array_mut().unwrap().pop().unwrap(),
        "source"
    );
    found["rows"][0].as_array_mut().unwrap().pop();
    assert_eq!(found, plain);
}

#[test]
fn find_source_shares_one_budget_and_resumes_utf8_slices_in_show() {
    let dir = fixture();
    let file = dir.path().join("src/more.py");
    let original = "def café():\n    return 'λ🙂'\n\nclass Other:\n    def café(self):\n        return '世界'\n";
    fs::write(&file, original).unwrap();
    for budget in [4, 8, 32, 65536] {
        let found = ok(
            dir.path(),
            &[
                "project",
                "find",
                "café",
                "--source",
                "--bytes",
                &budget.to_string(),
            ],
        );
        let selected = rows(&found);
        assert_eq!(selected.len(), 2);
        let total: usize = selected
            .iter()
            .map(|row| row["source"]["text"].as_str().unwrap().len())
            .sum();
        assert!(total <= budget);
        assert_eq!(found["source_budget"]["returned_bytes"], total);
        if budget == 4 {
            assert_eq!(selected[0]["source"]["text"], "def ");
            assert_eq!(selected[1]["source"]["text"], "");
            assert_eq!(selected[1]["source"]["next_offset"], 0);
        }
        if budget == 8 {
            assert_eq!(selected[0]["source"]["text"], "def caf");
            assert_eq!(selected[1]["source"]["text"], "d");
        }
        for row in selected {
            let handle = row["handle"].as_str().unwrap();
            let full = ok(
                dir.path(),
                &["project", "show", handle, "--source", "--bytes", "65536"],
            );
            let slice = &row["source"];
            let mut text = slice["text"].as_str().unwrap().to_string();
            assert_eq!(slice["span"]["start"], full["source"]["span"]["start"]);
            assert_eq!(slice["total_bytes"], full["source"]["total_bytes"]);
            if let Some(offset) = slice["next_offset"].as_u64() {
                let tail = ok(
                    dir.path(),
                    &[
                        "project",
                        "show",
                        handle,
                        "--source",
                        "--bytes",
                        "65536",
                        "--offset",
                        &offset.to_string(),
                    ],
                );
                text.push_str(tail["source"]["text"].as_str().unwrap());
            }
            assert_eq!(text, full["source"]["text"]);
        }
    }
    assert_eq!(fs::read_to_string(file).unwrap(), original);
}

#[test]
fn find_source_pages_bind_mode_budget_and_revision() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/other.py"),
        "def run():\n    return 2\n",
    )
    .unwrap();
    let first = ok(
        dir.path(),
        &[
            "project", "find", "run", "--source", "--bytes", "4", "--limit", "1",
        ],
    );
    let cursor = first["page"]["next"].as_str().unwrap();
    let next = ok(
        dir.path(),
        &[
            "project", "find", "run", "--source", "--bytes", "4", "--limit", "1", "--cursor",
            cursor,
        ],
    );
    assert_ne!(handle(&first, "run"), handle(&next, "run"));
    assert_eq!(next["source_budget"]["returned_bytes"], 4);
    for extra in [
        vec![],
        vec!["--source", "--bytes", "8"],
        vec!["--source", "--bytes", "4", "--signature"],
    ] {
        let mut args = vec!["project", "find", "run", "--cursor", cursor];
        args.extend(extra);
        assert!(!run(dir.path(), &args).0);
    }
    let plain = ok(dir.path(), &["project", "find", "run", "--limit", "1"]);
    assert!(
        !run(
            dir.path(),
            &[
                "project",
                "find",
                "run",
                "--source",
                "--cursor",
                plain["page"]["next"].as_str().unwrap()
            ]
        )
        .0
    );
    fs::write(
        dir.path().join("src/other.py"),
        "def run():\n    return 3\n",
    )
    .unwrap();
    assert!(
        !run(
            dir.path(),
            &["project", "find", "run", "--source", "--bytes", "4", "--cursor", cursor]
        )
        .0
    );
    assert!(
        !run(
            dir.path(),
            &["project", "show", &handle(&first, "run"), "--source"]
        )
        .0
    );
}

#[test]
fn find_source_bounds_large_bodies_and_reports_empty_results() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/huge.py"),
        format!("def huge():\n    return '{}'\n", "x".repeat(70000)),
    )
    .unwrap();
    let found = ok(
        dir.path(),
        &["project", "find", "huge", "--source", "--bytes", "65536"],
    );
    assert_eq!(found["source_budget"]["returned_bytes"], 65536);
    assert_eq!(rows(&found)[0]["source"]["next_offset"], 65536);
    let absent = ok(dir.path(), &["project", "find", "absent", "--source"]);
    assert_eq!(absent["source_budget"]["returned_bytes"], 0);
    assert_eq!(absent["page"]["total"], 0);
    for budget in ["0", "3", "65537"] {
        assert!(
            !run(
                dir.path(),
                &["project", "find", "run", "--source", "--bytes", budget]
            )
            .0
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["project", "find", "run", "--bytes", "4"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn find_returns_followable_scoped_declarations_without_unrelated_bodies() {
    let dir = fixture();
    let found = ok(dir.path(), &["project", "find", "run", "--signature"]);
    assert_eq!(found["page"]["total"], 1);
    let selected = handle(&found, "run");
    let shown = ok(dir.path(), &["project", "show", &selected, "--source"]);
    assert!(shown["source"]["text"]
        .as_str()
        .unwrap()
        .contains("helper(name)"));
    assert!(!found.to_string().contains("helper(name)"));
    assert_eq!(
        ok(
            dir.path(),
            &["project", "find", "run", "--in", "src/lib.py"]
        )["page"]["total"],
        0
    );
    assert_eq!(
        ok(dir.path(), &["project", "find", "local"])["omitted"]["matching_locals"],
        1
    );
    assert_eq!(
        ok(dir.path(), &["project", "find", "local", "--locals"])["page"]["total"],
        1
    );
    assert_eq!(
        ok(dir.path(), &["project", "find", "RUN"])["page"]["total"],
        0
    );
}

#[test]
fn find_pages_duplicate_unicode_names_and_refuses_changed_queries_and_sources() {
    let dir = fixture();
    fs::write(
        dir.path().join("src/more.py"),
        "def café():\n    pass\n\nclass Other:\n    def café(self):\n        pass\n",
    )
    .unwrap();
    let first = ok(dir.path(), &["project", "find", "café", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let second = ok(
        dir.path(),
        &[
            "project", "find", "café", "--limit", "1", "--cursor", cursor,
        ],
    );
    assert_eq!(first["page"]["total"], 2);
    assert_ne!(handle(&first, "café"), handle(&second, "café"));
    for extra in [
        vec!["--contains"],
        vec!["--signature"],
        vec!["--locals"],
        vec!["--in", "src/more.py"],
    ] {
        let mut args = vec!["project", "find", "café", "--cursor", cursor];
        args.extend(extra);
        assert!(!run(dir.path(), &args).0);
    }
    assert_eq!(
        ok(dir.path(), &["project", "find", "afé", "--contains"])["page"]["total"],
        2
    );
    assert!(!run(dir.path(), &["project", "find", ""]).0);
    assert!(!run(dir.path(), &["project", "find", &"x".repeat(513)]).0);
    fs::write(
        dir.path().join("src/more.py"),
        "def replacement():\n    pass\n",
    )
    .unwrap();
    assert!(!run(dir.path(), &["project", "find", "café", "--cursor", cursor]).0);
    assert!(!run(dir.path(), &["project", "show", &handle(&first, "café")]).0);
}

#[test]
fn find_matches_full_names_before_clipping_and_discloses_skipped_source() {
    let dir = fixture();
    let first = format!("{}x", "a".repeat(200));
    let second = format!("{}y", "a".repeat(200));
    fs::write(
        dir.path().join("src/long.py"),
        format!("def {first}():\n    pass\n\ndef {second}():\n    pass\n"),
    )
    .unwrap();
    let found = ok(dir.path(), &["project", "find", &second]);
    assert_eq!(found["page"]["total"], 1);
    assert!(rows(&found)[0]["name"]["omitted_bytes"].as_u64().unwrap() > 0);
    let root = mapped(dir.path());
    let scoped = ok(
        dir.path(),
        &["project", "find", "run", "--in", &handle(&root, "Service")],
    );
    assert_eq!(scoped["page"]["total"], 1);
    let skipped = ok(
        dir.path(),
        &["--max-file-size", "4", "project", "find", "run"],
    );
    assert_eq!(skipped["page"]["total"], 0);
    assert!(skipped["coverage"]["skipped_files"].as_u64().unwrap() > 0);
}

fn rows(report: &Value) -> Vec<Value> {
    let columns = report["columns"].as_array().unwrap();
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut result = serde_json::Map::new();
            for (column, cell) in columns.iter().zip(row.as_array().unwrap()) {
                result.insert(column.as_str().unwrap().to_owned(), cell.clone());
            }
            Value::Object(result)
        })
        .collect()
}

fn mapped(root: &Path) -> Value {
    ok(
        root,
        &[
            "project",
            "map",
            "src/app.py",
            "--depth",
            "8",
            "--fields",
            "handle,parent,kind,name,children,signature",
        ],
    )
}

fn handle(report: &Value, name: &str) -> String {
    rows(report)
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("missing {name}: {report}"))
        .get("handle")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn map_shows_lexical_hierarchy_hides_locals_and_gives_headers_without_bodies() {
    let dir = fixture();
    let report = mapped(dir.path());
    assert_eq!(report["schema"], "fr-project-1");
    let class = handle(&report, "Service");
    let method = handle(&report, "run");
    let records = rows(&report);
    let method_row = records.iter().find(|r| r["handle"] == method).unwrap();
    assert_eq!(method_row["parent"], class.rsplit(':').next().unwrap());
    assert_eq!(method_row["signature"]["basis"], "syntax-header");
    let signature = method_row["signature"]["text"].as_str().unwrap();
    assert!(signature.contains("name: str"), "{signature}");
    assert!(!signature.contains("local"), "{signature}");
    assert!(report["omitted"]["locals"].as_u64().unwrap() > 0);
    assert!(!records.iter().any(|r| r["name"] == "local"));
    let locals = ok(
        dir.path(),
        &["project", "map", "src/app.py", "--depth", "8", "--locals"],
    );
    assert!(rows(&locals).iter().any(|r| r["name"] == "local"));
    let subtree = ok(dir.path(), &["project", "map", &class, "--depth", "1"]);
    assert_eq!(subtree["root"], class);
    assert!(rows(&subtree).iter().any(|r| r["name"] == "run"));
}

#[test]
fn cursor_pages_cover_results_once_and_refuse_a_changed_query_or_revision() {
    let dir = fixture();
    let expected = ok(
        dir.path(),
        &["project", "map", "--depth", "8", "--limit", "500"],
    );
    let mut page = ok(
        dir.path(),
        &["project", "map", "--depth", "8", "--limit", "2"],
    );
    let first_cursor = page["page"]["next"].as_str().unwrap().to_owned();
    let mut combined = Vec::new();
    loop {
        combined.extend(rows(&page));
        let Some(next) = page["page"]["next"].as_str() else {
            break;
        };
        page = ok(
            dir.path(),
            &[
                "project", "map", "--depth", "8", "--limit", "2", "--cursor", next,
            ],
        );
    }
    assert_eq!(combined, rows(&expected));
    assert!(
        !run(
            dir.path(),
            &["project", "map", "--depth", "1", "--cursor", &first_cursor]
        )
        .0
    );
    fs::write(dir.path().join("src/new.py"), "def added(): pass\n").unwrap();
    let (success, error) = run(
        dir.path(),
        &["project", "map", "--depth", "8", "--cursor", &first_cursor],
    );
    assert!(!success);
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stale"));
}

#[test]
fn source_pages_reassemble_unicode_and_show_has_no_body_by_default() {
    let dir = fixture();
    let report = mapped(dir.path());
    let method = handle(&report, "run");
    let plain = ok(dir.path(), &["project", "show", &method]);
    assert!(plain.get("source").is_none());
    assert_eq!(
        plain["node"]["position"],
        serde_json::json!({"line": 4, "col": 9})
    );
    let expected = ok(dir.path(), &["project", "show", &method, "--source"]);
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let page = ok(
            dir.path(),
            &[
                "project",
                "show",
                &method,
                "--source",
                "--bytes",
                "4",
                "--offset",
                &offset.to_string(),
            ],
        );
        let part = page["source"]["text"].as_str().unwrap();
        assert!(part.len() <= 4);
        text.push_str(part);
        let Some(next) = page["source"]["next_offset"].as_u64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(text, expected["source"]["text"]);
    let invalid = text.find('λ').unwrap() + 1;
    assert!(
        !run(
            dir.path(),
            &[
                "project",
                "show",
                &method,
                "--source",
                "--offset",
                &invalid.to_string()
            ]
        )
        .0
    );
    fs::write(
        dir.path().join("src/lib.py"),
        "def helper(name): return name.upper()\n",
    )
    .unwrap();
    assert!(!run(dir.path(), &["project", "show", &method, "--source"]).0);
}

#[test]
fn relations_are_bounded_and_preserve_targets_confidence_and_unknowns() {
    let dir = fixture();
    let report = mapped(dir.path());
    let method = handle(&report, "run");
    let shown = ok(
        dir.path(),
        &["project", "show", &method, "--relations", "--limit", "500"],
    );
    let items = shown["relations"]["items"].as_array().unwrap();
    let call = items
        .iter()
        .find(|r| r["direction"] == "outgoing" && r["name"] == "helper")
        .unwrap();
    assert_eq!(call["confidence"], "import-qualified");
    let destination = ok(
        dir.path(),
        &["project", "show", call["target"].as_str().unwrap()],
    );
    assert_eq!(destination["node"]["name"], "helper");
    assert!(items.iter().any(|r| r["direction"] == "file-import"));
    let limited = ok(
        dir.path(),
        &["project", "show", &method, "--relations", "--limit", "1"],
    );
    assert_eq!(limited["relations"]["items"].as_array().unwrap().len(), 1);
    let cursor = limited["relations"]["page"]["next"].as_str().unwrap();
    let next = ok(
        dir.path(),
        &[
            "project",
            "show",
            &method,
            "--relations",
            "--limit",
            "1",
            "--cursor",
            cursor,
        ],
    );
    assert_eq!(next["relations"]["page"]["before"], 1);
}

#[test]
fn coverage_exposes_syntax_gaps_size_exclusions_symlinks_and_unknown_extensions() {
    let dir = fixture();
    fs::write(dir.path().join("broken.rs"), "fn broken( {\n").unwrap();
    fs::write(dir.path().join("large.rs"), "fn large() {}\n".repeat(40)).unwrap();
    fs::write(dir.path().join("opaque.blob"), "abc").unwrap();
    std::os::unix::fs::symlink(dir.path().join("src/lib.py"), dir.path().join("alias.py")).unwrap();
    let report = ok(dir.path(), &["--max-file-size", "256", "project", "map"]);
    assert_eq!(report["coverage"]["skipped_files"], 1);
    assert_eq!(report["coverage"]["skipped_symlinks"], 1);
    assert_eq!(report["coverage"]["unsupported_files"], 1);
    assert_eq!(
        report["coverage"]["files_by_gap"]["file has syntax errors"],
        1
    );
    let gaps = ok(dir.path(), &["--max-file-size", "256", "project", "gaps"]);
    assert!(gaps["items"].to_string().contains("large.rs"));
    assert!(gaps["items"].to_string().contains("broken.rs"));
    assert!(gaps["items"].to_string().contains("alias.py"));
}

#[test]
fn limits_projection_and_read_only_contract_hold() {
    let dir = fixture();
    let shallow = ok(
        dir.path(),
        &["project", "map", "--depth", "0", "--fields", "kind,name"],
    );
    assert_eq!(shallow["columns"], serde_json::json!(["kind", "name"]));
    assert_eq!(shallow["rows"], serde_json::json!([["directory", "."]]));
    assert!(shallow["omitted"]["depth"].as_u64().unwrap() > 0);
    for args in [
        vec!["project", "map", "--limit", "0"],
        vec!["project", "map", "--depth", "65"],
    ] {
        assert!(!run(dir.path(), &args).0);
    }
    assert!(!dir.path().join(".fr-history").exists());
}

#[test]
fn signature_headers_keep_multiline_types_and_do_not_include_one_line_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let samples = [
        ("main.rs", "pub fn process<T: Clone>(\n    value: T,\n) -> T { let secret = 42; value }\n", "process"),
        ("main.go", "package main\nfunc Process(value string) string { secret := 42; _ = secret; return value }\n", "Process"),
        ("main.ts", "export function process(value: { label: string }): string { const secret = 42; return value.label; }\n", "process"),
        ("Main.java", "class Main { public String process(String value) { int secret = 42; return value; } }\n", "process"),
    ];
    for (name, source, symbol) in samples {
        fs::write(dir.path().join(name), source).unwrap();
        let report = ok(
            dir.path(),
            &[
                "project",
                "map",
                name,
                "--depth",
                "8",
                "--fields",
                "handle,name,signature",
            ],
        );
        let records = rows(&report);
        let row = records.iter().find(|r| r["name"] == symbol).unwrap();
        assert_eq!(
            row["signature"]["basis"], "syntax-header",
            "{name}: {report}"
        );
        let header = row["signature"]["text"].as_str().unwrap();
        assert!(!header.contains("secret"), "{name}: {header}");
        assert!(header.contains("value"), "{name}: {header}");
    }
}

#[test]
fn large_labels_and_signatures_report_their_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let name = "n".repeat(1200);
    fs::write(
        dir.path().join("long.py"),
        format!("def {name}(value: str) -> str:\n    return value\n"),
    )
    .unwrap();
    let report = ok(
        dir.path(),
        &[
            "project",
            "map",
            "long.py",
            "--fields",
            "handle,kind,name,signature",
        ],
    );
    let records = rows(&report);
    let function = records.iter().find(|r| r["kind"] == "function").unwrap();
    assert_eq!(function["name"]["text"].as_str().unwrap().len(), 160);
    assert_eq!(function["name"]["omitted_bytes"], 1040);
    assert!(
        function["signature"]["text"]["omitted_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(serde_json::to_vec(&report).unwrap().len() < 2200);
}

#[test]
fn a_query_refuses_source_and_inventory_changes_before_emitting_its_report() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    project.verify(&root).unwrap();
    fs::write(root.join("added.py"), "def added(): pass\n").unwrap();
    assert!(project.verify(&root).is_err());
    fs::remove_file(root.join("added.py")).unwrap();
    fs::write(root.join("src/lib.py"), "def helper(): pass\n").unwrap();
    assert!(project.verify(&root).is_err());
    assert!(Project::new(&root, &index, &scanned, &options).is_err());
}

#[test]
fn short_ids_require_the_revision_and_cannot_drift_to_another_symbol() {
    let dir = fixture();
    let map = ok(
        dir.path(),
        &["project", "map", "src/app.py", "--depth", "8"],
    );
    let records = rows(&map);
    let method = records.iter().find(|r| r["name"] == "run").unwrap();
    let id = method["id"].as_str().unwrap();
    let revision = map["revision"].as_str().unwrap();
    assert!(!run(dir.path(), &["project", "show", id]).0);
    let shown = ok(dir.path(), &["project", "show", id, "--revision", revision]);
    assert_eq!(shown["node"]["name"], "run");
    let subtree = ok(dir.path(), &["project", "map", id, "--revision", revision]);
    assert_eq!(rows(&subtree)[0]["name"], "run");
    let full = format!("{}{id}", map["handle_prefix"].as_str().unwrap());
    assert_eq!(shown["node"]["handle"], full);
    fs::write(dir.path().join("src/lib.py"), "def different(): pass\n").unwrap();
    assert!(!run(dir.path(), &["project", "show", id, "--revision", revision]).0);
}

#[test]
fn an_inspected_location_drives_a_checked_refactoring_without_source_retrieval() {
    let dir = fixture();
    let map = ok(dir.path(), &["project", "map", "src/lib.py"]);
    let records = rows(&map);
    let symbol = records.iter().find(|r| r["name"] == "helper").unwrap();
    let shown = ok(
        dir.path(),
        &[
            "project",
            "show",
            symbol["id"].as_str().unwrap(),
            "--revision",
            map["revision"].as_str().unwrap(),
        ],
    );
    assert!(shown.get("source").is_none());
    let node = &shown["node"];
    let target = format!(
        "{}:{}:{}",
        node["path"].as_str().unwrap(),
        node["position"]["line"],
        node["position"]["col"]
    );
    let plan = ok(dir.path(), &["rename", &target, "decorate"]);
    assert_eq!(plan["applied"], false);
    assert_eq!(plan["files_changed"], 2);
    assert!(plan["changes"].to_string().contains("decorate"));
    assert!(fs::read_to_string(dir.path().join("src/lib.py"))
        .unwrap()
        .contains("def helper"));
}

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn package_views_preserve_cargo_and_npm_declarations_without_resolving_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        r#"
[workspace]
members = ["crates/*"]
exclude = ["crates/old"]
default-members = ["crates/core"]
[workspace.dependencies]
serde = "1"
[package]
name = "server"
version.workspace = true
# Package dependencies.
[dependencies]
serde = { workspace = true, features = ["derive"] }
util = { package = "actual-util", path = "../external", optional = true, default-features = false }
remote = { git = "https://example.test/repo", branch = "main" }
[dev-dependencies]
test-helper = "2"
[target.'cfg(unix)'.build-dependencies]
cc = { version = "1", features = [] }
"#,
    );
    put(
        root,
        "crates/core/Cargo.toml",
        "[package]\nname = 'core'\nversion = '0.1.0'\n",
    );
    put(
        root,
        "web/package.json",
        r#"{"name":"web","version":"1.0.0","private":true,"workspaces":{"packages":["apps/*"]},"dependencies":{"react":"^19"},"devDependencies":{"typescript":"*"},"peerDependencies":{"host":"workspace:*"},"optionalDependencies":{"local":"file:../local"}}"#,
    );
    let packages = ok(root, &["project", "packages"]);
    assert_eq!(packages["page"]["total"], 3);
    assert_eq!(packages["coverage"]["manifests"]["gaps"], 0);
    assert_eq!(packages["items"][0]["name"], "server");
    assert_eq!(packages["items"][0]["version"], Value::Null);
    assert_eq!(packages["items"][0]["version_inherited"], true);
    assert_eq!(packages["items"][1]["root"], "crates/core");
    assert_eq!(packages["items"][2]["private"], true);
    let dependencies = ok(
        root,
        &["project", "dependencies", "--manifest", "./Cargo.toml"],
    );
    let items = dependencies["items"].as_array().unwrap();
    assert_eq!(items.len(), 9);
    assert!(items.iter().any(|r| r["name"] == "util"
        && r["package"] == "actual-util"
        && r["path"] == "../external"
        && r["optional"] == true
        && r["default-features"] == false));
    assert!(items
        .iter()
        .any(|r| r["name"] == "serde" && r["workspace"] == true && r["feature_count"] == 1));
    assert!(items
        .iter()
        .any(|r| r["name"] == "serde" && r["scope"] == "workspace"));
    assert!(items.iter().any(|r| r["name"] == "cc"
        && r["scope"] == "target"
        && r["target_condition"] == "cfg(unix)"));
    assert!(items.iter().any(|r| r["kind"] == "workspace-member-pattern"
        && r["pattern"] == "crates/*"
        && r["expanded"] == false));
    for item in items {
        assert_eq!(item["basis"], "manifest-declaration");
        if item["kind"] == "dependency" {
            assert_eq!(item["resolution"], "not-attempted");
            assert!(item.get("resolved_target").is_none());
        }
    }
    let npm = ok(
        root,
        &["project", "dependencies", "--manifest", "web/package.json"],
    );
    assert_eq!(npm["page"]["total"], 5);
    assert!(npm["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["requirement"] == "workspace:*"));
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn manifest_pages_are_bounded_complete_and_bound_to_the_filter() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for n in 0..3 {
        put(
            root,
            &format!("p{n}/package.json"),
            r#"{"dependencies":{"a":"1","b":"2","c":"3"}}"#,
        );
    }
    for query in ["packages", "dependencies"] {
        let full = ok(root, &["project", query]);
        let mut current = ok(root, &["project", query, "--limit", "1"]);
        let mut collected = Vec::new();
        loop {
            collected.extend(current["items"].as_array().unwrap().clone());
            let Some(cursor) = current["page"]["next"].as_str() else {
                break;
            };
            current = ok(
                root,
                &["project", query, "--limit", "2", "--cursor", cursor],
            );
            assert!(current["items"].as_array().unwrap().len() <= 2);
        }
        assert_eq!(collected, *full["items"].as_array().unwrap());
        assert!(!run(root, &["project", query, "--limit", "0"]).0);
        assert!(!run(root, &["project", query, "--limit", "501"]).0);
    }
    let filtered = ok(
        root,
        &[
            "project",
            "dependencies",
            "--manifest",
            "p0/package.json",
            "--limit",
            "1",
        ],
    );
    let cursor = filtered["page"]["next"].as_str().unwrap();
    assert!(
        !run(
            root,
            &[
                "project",
                "dependencies",
                "--manifest",
                "p1/package.json",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "dependencies", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "packages", "--cursor", cursor]).0);
    assert!(
        !run(
            root,
            &["project", "dependencies", "--manifest", "missing.json"]
        )
        .0
    );
    put(
        root,
        "long/package.json",
        &serde_json::json!({"name": "λ".repeat(300), "dependencies": {"long": "λ".repeat(1000)}})
            .to_string(),
    );
    let long = ok(
        root,
        &["project", "dependencies", "--manifest", "long/package.json"],
    );
    assert_eq!(
        long["items"][0]["requirement"]["text"]
            .as_str()
            .unwrap()
            .len(),
        512
    );
    assert_eq!(long["items"][0]["requirement"]["omitted_bytes"], 1488);
}

#[test]
fn toml_content_and_manifest_inventory_changes_invalidate_project_handles_and_cursors() {
    let dir = fixture();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[package]\nname = 'old'\n[dependencies]\na = '1'\nb = '2'\n",
    );
    put(root, "other/Cargo.toml", "[workspace]\n");
    let map = mapped(root);
    let handle = handle(&map, "run");
    let old = ok(root, &["project", "packages", "--limit", "1"]);
    let deps = ok(root, &["project", "dependencies", "--limit", "1"]);
    let cursor = old["page"]["next"].as_str().unwrap();
    put(
        root,
        "Cargo.toml",
        "[package]\nname = 'new'\n[dependencies]\na = '1'\nb = '2'\n",
    );
    assert!(!run(root, &["project", "show", &handle]).0);
    assert!(!run(root, &["project", "packages", "--cursor", cursor]).0);
    assert!(
        !run(
            root,
            &[
                "project",
                "dependencies",
                "--cursor",
                deps["page"]["next"].as_str().unwrap()
            ]
        )
        .0
    );
    let before = ok(root, &["project", "packages"]);
    fs::rename(root.join("other/Cargo.toml"), root.join("other/other.toml")).unwrap();
    let after = ok(root, &["project", "packages"]);
    assert_ne!(before["revision"], after["revision"]);
    assert_eq!(after["page"]["total"], 1);
}

#[test]
fn package_gaps_cover_bad_syntax_and_unsupported_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "invalid/Cargo.toml", "[package\n");
    put(root, "invalid/package.json", "[]");
    put(root, "Cargo.toml", "[package]\nname = 42\n[dependencies]\nbad = 1\n# Partial fields.\nfields = { path = false, optional = 'yes', features = [1], future = true }\n[workspace]\nmembers = [1, 'valid/*']\n");
    put(
        root,
        "package.json",
        r#"{"dependencies":[],"workspaces":{},"private":"yes"}"#,
    );
    let packages = ok(root, &["project", "packages"]);
    assert_eq!(packages["coverage"]["manifests"]["discovered"], 4);
    assert_eq!(packages["coverage"]["manifests"]["parsed"], 2);
    assert!(packages["coverage"]["manifests"]["gaps"].as_u64().unwrap() >= 10);
    let gaps = ok(root, &["project", "gaps"]);
    let manifest_gaps: Vec<_> = gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["scope"] == "manifest")
        .collect();
    assert!(manifest_gaps
        .iter()
        .any(|r| r["reason"] == "invalid TOML syntax"));
    assert!(manifest_gaps
        .iter()
        .any(|r| r["reason"] == "invalid JSON object"));
    let declarations = ok(root, &["project", "dependencies"]);
    assert!(declarations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["declaration_status"] == "unsupported"));
    assert!(declarations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["unreported_fields"] == 1));
}

#[test]
fn manifest_discovery_obeys_scan_scope_ignore_size_and_history_exclusion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\n");
    put(root, "src/main.rs", "fn main() {}\n");
    put(root, "ignored/package.json", "{}");
    put(root, ".hidden/package.json", "{}");
    put(root, ".fr-history/package.json", "{}");
    put(root, ".gitignore", "ignored/\n");
    put(root, "big/Cargo.toml", &format!("#{}\n", "x".repeat(200)));
    put(root, "bad/Cargo.toml", "");
    fs::write(root.join("bad/Cargo.toml"), [0xff]).unwrap();
    let ordinary = ok(root, &["--max-file-size", "100", "project", "packages"]);
    assert_eq!(ordinary["page"]["total"], 1);
    assert_eq!(ordinary["coverage"]["manifests"]["discovered"], 3);
    assert_eq!(ordinary["coverage"]["manifests"]["gaps"], 2);
    let all = ok(
        root,
        &[
            "--no-ignore",
            "--max-file-size",
            "100",
            "project",
            "packages",
        ],
    );
    assert_eq!(all["page"]["total"], 3);
    assert_eq!(all["coverage"]["manifests"]["discovered"], 5);
    let file = ok(&root.join("src/main.rs"), &["project", "packages"]);
    assert_eq!(file["page"]["total"], 0);
    let manifest = ok(&root.join("Cargo.toml"), &["project", "packages"]);
    assert_eq!(manifest["page"]["total"], 1);
}

#[cfg(unix)]
#[test]
fn manifest_symlinks_are_reported_without_reading_their_targets() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    put(
        outside.path(),
        "Cargo.toml",
        "[package]\nname = 'outside'\n",
    );
    symlink(
        outside.path().join("Cargo.toml"),
        dir.path().join("Cargo.toml"),
    )
    .unwrap();
    symlink(outside.path(), dir.path().join("linked")).unwrap();
    let packages = ok(dir.path(), &["project", "packages"]);
    assert_eq!(packages["page"]["total"], 0);
    assert_eq!(packages["coverage"]["manifests"]["discovered"], 1);
    let gaps = ok(dir.path(), &["project", "gaps"]);
    assert!(gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["path"] == "Cargo.toml" && r["reason"] == "symlink manifest"));
    assert!(
        !run(
            dir.path(),
            &["project", "dependencies", "--manifest", "Cargo.toml"]
        )
        .0
    );
}

#[test]
fn project_verification_refuses_manifest_content_and_inventory_races() {
    use fun_refactor::index::Index;
    use fun_refactor::project::Project;
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let source = "[package]\nname = 'original'\n";
    put(&root, "Cargo.toml", source);
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    project.verify(&root).unwrap();
    put(&root, "Cargo.toml", "[package]\nname = 'modified'\n");
    assert!(project.verify(&root).is_err());
    put(&root, "Cargo.toml", source);
    project.verify(&root).unwrap();
    fs::rename(root.join("Cargo.toml"), root.join("other.toml")).unwrap();
    assert!(project.verify(&root).is_err());
    fs::rename(root.join("other.toml"), root.join("Cargo.toml")).unwrap();
    put(&root, "nested/Cargo.toml", "[workspace]\n");
    assert!(project.verify(&root).is_err());
}

#[test]
fn local_links_keep_manifest_identity_separate_from_version_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        r#"
[package]
name = 'app'
[dependencies]
# Local alias.
alias = { package = 'core', path = 'crates/core', version = '999' }
wrong = { path = 'crates/core' }
# Source declarations.
remote = '1'
inherited = { workspace = true }
conflict = { path = 'crates/core', git = 'https://example.test/repo' }
# Platform dependencies.
[target.'cfg(unix)'.build-dependencies]
core = { path = 'crates/core' }
[workspace.dependencies]
core = { path = 'crates/core' }
"#,
    );
    put(
        root,
        "crates/core/Cargo.toml",
        "[package]\nname = 'core'\nversion = '1.0.0'\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let items = report["items"].as_array().unwrap();
    assert_eq!(items.len(), 6);
    let alias = items.iter().find(|r| r["name"] == "alias").unwrap();
    assert_eq!(alias["status"], "linked");
    assert_eq!(alias["target_manifest"], "crates/core/Cargo.toml");
    assert_eq!(alias["version_check"], "not-performed");
    assert!(items.iter().any(|r| r["name"] == "wrong"
        && r["reason"] == "package-name-mismatch"
        && r["target_manifest"].is_null()));
    assert!(items
        .iter()
        .any(|r| r["name"] == "inherited" && r["reason"] == "workspace-dependency-not-declared"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "conflict" && r["reason"] == "conflicting-dependency-sources"));
    assert!(items.iter().any(|r| r["scope"] == "target"
        && r["target_condition"] == "cfg(unix)"
        && r["status"] == "linked"));
    assert!(items
        .iter()
        .any(|r| r["scope"] == "workspace" && r["status"] == "linked"));
}

#[test]
fn npm_file_links_allow_aliases_and_workspace_matches_keep_directory_depth() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "package.json",
        r#"{"workspaces":["apps/*","apps/*/nested","apps/other*","missing/*"],"dependencies":{"alias":"file:apps/a","relative":"./apps/b","registry":"1","workspace":"workspace:*","archive":"file:pkg.tgz"}}"#,
    );
    put(root, "apps/a/package.json", r#"{"name":"actual-a"}"#);
    put(root, "apps/b/package.json", r#"{"name":"b"}"#);
    put(root, "apps/a/nested/package.json", "{}");
    put(root, "apps/rust/Cargo.toml", "[package]\nname='rust'\n");
    let report = ok(root, &["project", "links", "--manifest", "package.json"]);
    let items = report["items"].as_array().unwrap();
    assert!(items.iter().any(|r| r["name"] == "alias"
        && r["target_manifest"] == "apps/a/package.json"
        && r["target_name"] == "actual-a"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "relative" && r["target_manifest"] == "apps/b/package.json"));
    assert!(!items
        .iter()
        .any(|r| r["name"] == "registry" || r["name"] == "workspace"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "archive" && r["status"] == "unresolved"));
    let matched: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "workspace-member-match" && r["status"] == "matched")
        .collect();
    assert_eq!(matched.len(), 3);
    assert_eq!(
        matched.iter().filter(|r| r["pattern"] == "apps/*").count(),
        2
    );
    assert!(matched.iter().all(|r| r["membership"] == "candidate"));
    assert!(items
        .iter()
        .any(|r| r["pattern"] == "apps/other*" && r["reason"] == "pattern-syntax-unsupported"));
    assert!(items
        .iter()
        .any(|r| r["pattern"] == "missing/*" && r["reason"] == "no-observed-package-match"));
}

#[test]
fn workspace_exclusions_and_ownership_ambiguity_prevent_confirmed_matches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/old']\n",
    );
    put(root, "crates/ok/Cargo.toml", "[package]\nname='ok'\n");
    put(root, "crates/old/Cargo.toml", "[package]\nname='old'\n");
    put(
        root,
        "crates/nested/Cargo.toml",
        "[package]\nname='nested'\n[workspace]\n",
    );
    put(
        root,
        "crates/elsewhere/Cargo.toml",
        "[package]\nname='elsewhere'\nworkspace='../elsewhere'\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let items = report["items"].as_array().unwrap();
    assert_eq!(items.iter().filter(|r| r["status"] == "matched").count(), 1);
    assert!(items
        .iter()
        .any(|r| r["status"] == "excluded" && r["excluded_by"] == "crates/old"));
    assert_eq!(
        items
            .iter()
            .filter(|r| r["reason"] == "workspace-ownership-unresolved")
            .count(),
        2
    );
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/**']\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved" && r["reason"] == "unsupported-exclusion"));
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=[42,'../elsewhere','crates/?','crates/[ab]']\n",
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert_eq!(report["page"]["total"], 4);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
}

#[test]
fn link_paths_cannot_escape_the_snapshot_or_normalize_through_unobserved_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/Cargo.toml", "[package]\nname='app'\n[dependencies]\ncore={path='../core'}\nescape={path='../../outside'}\nmissing={path='../missing/../core',package='core'}\ninvalid={path=42}\npercent={path='../%63ore'}\n");
    put(root, "core/Cargo.toml", "[package]\nname='core'\n");
    let report = ok(root, &["project", "links"]);
    let items = report["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["name"] == "core" && r["status"] == "linked"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "escape" && r["reason"] == "outside-selected-root"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "missing" && r["reason"] == "directory-not-observed"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "invalid" && r["reason"] == "invalid-path-field"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "percent" && r["reason"] == "path-syntax-unsupported"));
    let scoped = ok(&root.join("app"), &["project", "links"]);
    assert!(scoped["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
    assert!(!root.join(".fr-history").exists());
}

#[cfg(unix)]
#[test]
fn links_do_not_follow_symlinks_or_ignored_manifests() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[package]\nname='app'\n[dependencies]\nlinked={package='core',path='linked'}\nbypass={package='core',path='linked/../core'}\nignored={path='ignored'}\nskipped={path='skipped'}\n");
    put(root, "core/Cargo.toml", "[package]\nname='core'\n");
    put(root, "ignored/Cargo.toml", "[package]\nname='ignored'\n");
    put(root, ".gitignore", "ignored/\n");
    fs::create_dir(root.join("skipped")).unwrap();
    symlink(root.join("core"), root.join("linked")).unwrap();
    symlink(
        root.join("core/Cargo.toml"),
        root.join("skipped/Cargo.toml"),
    )
    .unwrap();
    let report = ok(root, &["project", "links"]);
    assert_eq!(report["page"]["total"], 4);
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "unresolved"));
}

#[test]
fn link_pages_are_complete_query_bound_and_invalidated_by_target_manifest_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[package]\nname='app'\n[dependencies]\na={path='a'}\nb={path='b'}\n[workspace]\nmembers=['*','a']\n");
    put(root, "a/Cargo.toml", "[package]\nname='a'\n");
    put(root, "b/Cargo.toml", "[package]\nname='b'\n");
    let full = ok(root, &["project", "links"]);
    let first = ok(root, &["project", "links", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut page = first.clone();
    let mut items = Vec::new();
    loop {
        items.extend(page["items"].as_array().unwrap().clone());
        let Some(next) = page["page"]["next"].as_str() else {
            break;
        };
        page = ok(
            root,
            &["project", "links", "--limit", "2", "--cursor", next],
        );
    }
    assert_eq!(items, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &[
                "project",
                "links",
                "--manifest",
                "a/Cargo.toml",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "dependencies", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "links", "--limit", "0"]).0);
    assert!(!run(root, &["project", "links", "--limit", "501"]).0);
    put(root, "a/Cargo.toml", "[package]\nname='renamed'\n");
    assert!(!run(root, &["project", "links", "--cursor", cursor]).0);
    let updated = ok(root, &["project", "links"]);
    assert!(updated["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "a" && r["reason"] == "package-name-mismatch"));
}

#[test]
fn link_resolution_uses_full_values_before_clipping_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let directory = (0..5)
        .map(|_| "p".repeat(120))
        .collect::<Vec<_>>()
        .join("/");
    let name = "n".repeat(300);
    put(
        root,
        "Cargo.toml",
        &format!("[package]\nname='app'\n[dependencies]\n{name}={{path='{directory}'}}\n"),
    );
    put(
        root,
        &format!("{directory}/Cargo.toml"),
        &format!("[package]\nname='{name}'\n"),
    );
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    assert_eq!(report["items"][0]["status"], "linked");
    assert_eq!(report["items"][0]["name"]["omitted_bytes"], 140);
    assert_eq!(
        report["items"][0]["target_manifest"]["text"]
            .as_str()
            .unwrap()
            .len(),
        512
    );
}

#[test]
fn simple_workspace_pattern_matches_agree_with_cargo_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/*']\nexclude=['crates/old']\nresolver='2'\n",
    );
    for name in ["a", "b", "old"] {
        put(
            root,
            &format!("crates/{name}/Cargo.toml"),
            &format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
        );
        put(
            root,
            &format!("crates/{name}/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        );
    }
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let canonical = root.canonicalize().unwrap();
    let expected: std::collections::BTreeSet<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            metadata["workspace_members"]
                .as_array()
                .unwrap()
                .contains(&p["id"])
        })
        .map(|p| {
            Path::new(p["manifest_path"].as_str().unwrap())
                .strip_prefix(&canonical)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let report = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
    let matched: std::collections::BTreeSet<_> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "matched")
        .map(|r| r["target_manifest"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(matched, expected);
}

fn cargo_package(root: &Path, path: &str, name: &str, extra: &str) {
    put(
        root,
        &format!("{path}/Cargo.toml"),
        &format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n{extra}"),
    );
    put(
        root,
        &format!("{path}/src/lib.rs"),
        "pub fn value() -> u32 { 1 }\n",
    );
}

#[test]
fn inherited_local_paths_and_transitive_members_agree_with_cargo_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\nmembers=['app']\nresolver='2'\n[workspace.dependencies]\nalias={package='local-lib',path='lib',features=['shared']}\n");
    cargo_package(
        root,
        "app",
        "app",
        "[dependencies]\nalias={workspace=true,features=['local'],optional=true}\n",
    );
    cargo_package(
        root,
        "lib",
        "local-lib",
        "[dependencies]\nleaf={path='../leaf'}\n[features]\nshared=[]\nlocal=[]\n",
    );
    cargo_package(root, "leaf", "leaf", "");
    cargo_package(root, "unused", "unused", "");
    let metadata_output = Command::new("cargo")
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        metadata_output.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata_output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&metadata_output.stdout).unwrap();
    let canonical = root.canonicalize().unwrap();
    let expected: std::collections::BTreeSet<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            metadata["workspace_members"]
                .as_array()
                .unwrap()
                .contains(&p["id"])
        })
        .map(|p| {
            Path::new(p["manifest_path"].as_str().unwrap())
                .strip_prefix(&canonical)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let view = ok(root, &["project", "workspaces"]);
    let actual: std::collections::BTreeSet<_> = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "member")
        .map(|r| r["manifest"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["manifest"] == "leaf/Cargo.toml"
            && r["membership_basis"] == "automatic-path-member"
            && r["via_manifest"] == "lib/Cargo.toml"));
    let links = ok(root, &["project", "links", "--manifest", "app/Cargo.toml"]);
    let inherited = &links["items"][0];
    assert_eq!(inherited["status"], "linked");
    assert_eq!(inherited["target_manifest"], "lib/Cargo.toml");
    assert_eq!(inherited["basis"], "workspace-inheritance");
    assert_eq!(inherited["workspace_manifest"], "Cargo.toml");
    assert_eq!(inherited["features_check"], "not-performed");
    let app = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "app")
        .unwrap();
    assert_eq!(
        Path::new(app["dependencies"][0]["path"].as_str().unwrap()),
        canonical.join("lib")
    );
    assert_eq!(app["dependencies"][0]["rename"], "alias");
}

#[test]
fn nearest_workspace_and_explicit_workspace_pointer_choose_the_inheritance_basis() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['nested/app']\n[workspace.dependencies]\ncore={path='outer'}\n",
    );
    put(
        root,
        "nested/Cargo.toml",
        "[workspace]\nmembers=['app']\n[workspace.dependencies]\ncore={path='inner'}\n",
    );
    cargo_package(root, "outer", "core", "");
    cargo_package(root, "nested/inner", "core", "");
    cargo_package(
        root,
        "nested/app",
        "app",
        "[dependencies]\ncore={workspace=true}\n",
    );
    let first = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(
        first["items"][0]["target_manifest"],
        "nested/inner/Cargo.toml"
    );
    put(
        root,
        "nested/app/Cargo.toml",
        "[package]\nname='app'\nworkspace='../..'\n[dependencies]\ncore={workspace=true}\n",
    );
    let second = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(second["items"][0]["target_manifest"], "outer/Cargo.toml");
    let ownership = ok(
        root,
        &[
            "project",
            "workspaces",
            "--manifest",
            "nested/app/Cargo.toml",
        ],
    );
    assert_eq!(
        ownership["items"][0]["ownership_basis"],
        "package-workspace"
    );
    assert_eq!(ownership["items"][0]["workspace_manifest"], "Cargo.toml");
}

#[test]
fn ignored_and_malformed_ancestor_manifests_block_inheritance_from_farther_roots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['nested/app']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    cargo_package(root, "core", "core", "");
    cargo_package(
        root,
        "nested/app",
        "app",
        "[dependencies]\ncore={workspace=true}\n",
    );
    put(root, "nested/Cargo.toml", "[workspace]\nmembers=['app']\n");
    put(root, ".gitignore", "/nested/Cargo.toml\n");
    let ignored = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(ignored["items"][0]["status"], "unresolved");
    assert_eq!(
        ignored["items"][0]["reason"],
        "workspace-ancestor-unavailable"
    );
    let gaps = ok(root, &["project", "gaps"]);
    assert!(gaps["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["path"] == "nested/Cargo.toml"
            && r["reason"] == "workspace ancestor excluded or unavailable"));
    put(root, ".gitignore", "");
    put(root, "nested/Cargo.toml", "[workspace\n");
    let malformed = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(
        malformed["items"][0]["reason"],
        "workspace-ancestor-unavailable"
    );
    fs::remove_file(root.join("nested/Cargo.toml")).unwrap();
    let unblocked = ok(
        root,
        &["project", "links", "--manifest", "nested/app/Cargo.toml"],
    );
    assert_eq!(unblocked["items"][0]["status"], "linked");
    let scoped = ok(&root.join("nested/app"), &["project", "links"]);
    assert_eq!(scoped["items"][0]["reason"], "workspace-root-not-observed");
}

#[test]
fn inheritance_refuses_unsupported_overrides_and_nonlocal_workspace_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "Cargo.toml", "[workspace]\nmembers=['app']\n[workspace.dependencies]\nremote='1'\ninvalid={path='core',optional=true}\ncore={path='core'}\n");
    cargo_package(root, "core", "core", "");
    cargo_package(root, "app", "app", "[dependencies]\ncore={workspace=true,path='../core'}\nmissing={workspace=true}\nremote={workspace=true}\ninvalid={workspace=true}\nfalseflag={workspace=false}\n");
    let links = ok(root, &["project", "links", "--manifest", "app/Cargo.toml"]);
    let items = links["items"].as_array().unwrap();
    assert!(items.iter().all(|r| r["status"] == "unresolved"));
    for (name, reason) in [
        ("core", "unsupported-inherited-fields"),
        ("missing", "workspace-dependency-not-declared"),
        ("remote", "workspace-dependency-not-local"),
        ("invalid", "invalid-workspace-dependency"),
        ("falseflag", "invalid-inherited-dependency"),
    ] {
        assert!(items
            .iter()
            .any(|r| r["name"] == name && r["reason"] == reason));
    }
    let ownership = ok(
        root,
        &["project", "workspaces", "--manifest", "core/Cargo.toml"],
    );
    assert_eq!(
        ownership["items"][0]["reason"],
        "package-not-observed-member"
    );
}

#[test]
fn automatic_membership_terminates_on_cycles_and_respects_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a']\nexclude=['excluded']\n",
    );
    cargo_package(
        root,
        "a",
        "a",
        "[dependencies]\nb={path='../b'}\nexcluded={path='../excluded'}\n",
    );
    cargo_package(root, "b", "b", "[dependencies]\na={path='../a'}\n");
    cargo_package(root, "excluded", "excluded", "");
    let view = ok(root, &["project", "workspaces"]);
    assert_eq!(
        view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "member")
            .count(),
        2
    );
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["manifest"] == "excluded/Cargo.toml" && r["reason"] == "workspace-excluded"));
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a']\nexclude=['**/excluded']\n",
    );
    let unknown = ok(root, &["project", "workspaces"]);
    assert!(!unknown["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "member"));
}

#[test]
fn workspace_pages_and_inherited_links_are_revision_bound() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a','b']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    cargo_package(root, "a", "a", "[dependencies]\ncore={workspace=true}\n");
    cargo_package(root, "b", "b", "");
    cargo_package(root, "core", "core", "");
    let full = ok(root, &["project", "workspaces"]);
    let first = ok(root, &["project", "workspaces", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut collected = first["items"].as_array().unwrap().clone();
    let mut current = first.clone();
    while let Some(next) = current["page"]["next"].as_str() {
        current = ok(
            root,
            &["project", "workspaces", "--limit", "2", "--cursor", next],
        );
        collected.extend(current["items"].as_array().unwrap().clone());
    }
    assert_eq!(collected, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &[
                "project",
                "workspaces",
                "--manifest",
                "a/Cargo.toml",
                "--cursor",
                cursor
            ]
        )
        .0
    );
    assert!(!run(root, &["project", "workspaces", "--limit", "0"]).0);
    assert!(!run(root, &["project", "workspaces", "--limit", "501"]).0);
    assert!(!run(root, &["project", "links", "--cursor", cursor]).0);
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['b']\n[workspace.dependencies]\ncore={path='core'}\n",
    );
    assert!(!run(root, &["project", "workspaces", "--cursor", cursor]).0);
    let links = ok(root, &["project", "links", "--manifest", "a/Cargo.toml"]);
    assert_eq!(links["items"][0]["reason"], "package-not-observed-member");
}

#[test]
fn root_packages_seed_automatic_membership_without_a_members_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[package]\nname='root'\n[workspace]\n[dependencies]\nchild={path='child'}\n",
    );
    cargo_package(root, "child", "child", "");
    let view = ok(root, &["project", "workspaces"]);
    assert_eq!(view["items"][0]["membership_basis"], "root-package");
    assert_eq!(
        view["items"][1]["membership_basis"],
        "automatic-path-member"
    );
}

#[test]
fn ignored_workspace_ancestor_metadata_participates_in_snapshot_verification() {
    use fun_refactor::index::Index;
    use fun_refactor::project::Project;
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "Cargo.toml", "[workspace]\nmembers=['nested/app']\n");
    cargo_package(&root, "nested/app", "app", "");
    put(&root, ".gitignore", "/nested/Cargo.toml\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let before = Project::new(&root, &index, &scanned, &options).unwrap();
    before.verify(&root).unwrap();
    put(&root, "nested/Cargo.toml", "opaque excluded contents");
    assert!(before.verify(&root).is_err());
    let blocked = Project::new(&root, &index, &scanned, &options).unwrap();
    blocked.verify(&root).unwrap();
    put(&root, "nested/Cargo.toml", "changed excluded contents");
    blocked.verify(&root).unwrap();
    fs::remove_file(root.join("nested/Cargo.toml")).unwrap();
    assert!(blocked.verify(&root).is_err());
}

fn relation_handle(root: &Path, name: &str, qualifier: Option<&str>) -> String {
    let map = ok(
        root,
        &[
            "project",
            "map",
            "--depth",
            "64",
            "--fields",
            "handle,name,qualifier",
        ],
    );
    rows(&map)
        .iter()
        .find(|r| r["name"] == name && r["qualifier"].as_str() == qualifier)
        .unwrap_or_else(|| panic!("missing {qualifier:?}::{name}: {map}"))["handle"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn call_pages_preserve_graph_edges_and_candidate_evidence() {
    use fun_refactor::analysis::call_graph::CallGraph;
    use fun_refactor::index::Index;
    use fun_refactor::scan::ScanOptions;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "trait Shape { fn area(&self); }\nstruct A;\nstruct B;\nimpl Shape for A { fn area(&self) {} }\nimpl Shape for B { fn area(&self) {} }\nfn helper() {}\nfn report(s: &dyn Shape) { s.area(); helper(); unknown(); }\nfn main() { report(&A); }\n");
    let canonical = root.canonicalize().unwrap();
    let index = Index::build(&canonical, &ScanOptions::default()).unwrap();
    let graph = CallGraph::build(&index);
    let view = ok(root, &["project", "calls", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        graph.edge_count() + graph.file_scope.len() + graph.unresolved.len()
    );
    for (caller, callee, edge) in graph.edges() {
        let caller = index.symbol(caller).unwrap();
        let callee = index.symbol(callee).unwrap();
        assert!(items.iter().any(|r| r["caller"]["name"] == caller.name
            && r["callee"]["name"] == callee.name
            && r["callee"]["qualifier"].as_str() == callee.qualifier.as_deref()
            && r["site"]["offset"] == edge.offset
            && r["origin"] == edge.origin.as_str()
            && r["confidence"] == edge.confidence.as_str()));
    }
    let dispatch: Vec<_> = items
        .iter()
        .filter(|r| r["dispatch_candidate"] == true)
        .collect();
    assert!(!dispatch.is_empty());
    assert!(dispatch
        .iter()
        .all(|r| r["status"] == "dispatch-candidate" && r["confidence"] == "field-based"));
    assert!(items
        .iter()
        .any(|r| r["status"] == "unresolved" && r["name"] == "unknown" && r["callee"].is_null()));
    let target = dispatch[0]["callee"]["handle"].as_str().unwrap();
    let detail = ok(root, &["project", "show", target]);
    assert_eq!(detail["node"]["name"], "area");
    assert!(detail.get("source").is_none());
}

#[test]
fn call_scope_handles_incoming_outgoing_internal_and_file_scope_calls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "def leaf():\n    return 'BODY_ONLY_SENTINEL'\n\ndef recur():\n    recur()\n    leaf()\n    unknown()\n\ndef outer():\n    def nested():\n        leaf()\n    nested()\n\nrecur()\n");
    let recur = relation_handle(root, "recur", None);
    let outgoing = ok(
        root,
        &["project", "calls", &recur, "--direction", "outgoing"],
    );
    assert!(outgoing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["callee"]["name"] == "leaf"));
    assert!(outgoing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "unresolved"));
    let incoming = ok(
        root,
        &["project", "calls", &recur, "--direction", "incoming"],
    );
    let items = incoming["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["scope_relation"] == "internal" && r["caller"]["name"] == "recur"));
    assert!(items
        .iter()
        .any(|r| r["caller_scope"] == "file" && r["caller"].is_null()));
    assert!(!items.iter().any(|r| r["status"] == "unresolved"));
    let both = ok(root, &["project", "calls", &recur]);
    assert_eq!(
        both["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["scope_relation"] == "internal")
            .count(),
        1
    );
    assert!(!both.to_string().contains("BODY_ONLY_SENTINEL"));
    let outer = relation_handle(root, "outer", None);
    let nested = ok(
        root,
        &["project", "calls", &outer, "--direction", "outgoing"],
    );
    assert!(nested["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["caller"]["name"] == "nested" && r["callee"]["name"] == "leaf"));
    let file = ok(root, &["project", "calls", "app.py"]);
    assert!(file["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["caller_scope"] == "file"));
}

#[test]
fn implementation_pages_match_the_hierarchy_without_claiming_runtime_certainty() {
    use fun_refactor::analysis::call_graph::Hierarchy;
    use fun_refactor::index::Index;
    use fun_refactor::scan::ScanOptions;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "shape.rs", "trait Shape { fn area(&self); }\nstruct A;\nstruct B;\nimpl Shape for A { fn area(&self) { } }\nimpl Shape for B { fn area(&self) { } }\n");
    let index = Index::build(&root.canonicalize().unwrap(), &ScanOptions::default()).unwrap();
    let hierarchy = Hierarchy::scan(&index);
    for (name, qualifier) in [("Shape", None), ("area", Some("Shape"))] {
        let declaration = index
            .symbols
            .iter()
            .find(|s| s.name == name && s.qualifier.as_deref() == qualifier)
            .unwrap();
        let expected = hierarchy.implementations_of(&index, declaration.id);
        assert_eq!(expected.len(), 2);
        let expected: Vec<_> = index
            .symbols
            .iter()
            .filter(|s| s.file == declaration.file && declaration.full_span.contains(s.full_span))
            .flat_map(|s| {
                let index = &index;
                hierarchy
                    .implementations_of(index, s.id)
                    .into_iter()
                    .map(move |implementation| (s, index.symbol(implementation).unwrap()))
            })
            .collect();
        let handle = relation_handle(root, name, qualifier);
        let view = ok(root, &["project", "implementations", &handle]);
        assert_eq!(view["page"]["total"], expected.len());
        for row in view["items"].as_array().unwrap() {
            assert!(expected.iter().any(|(declaration, implementation)| {
                row["declaration"]["name"] == declaration.name
                    && row["declaration"]["qualifier"].as_str() == declaration.qualifier.as_deref()
                    && row["implementation"]["name"] == implementation.name
                    && row["implementation"]["qualifier"].as_str()
                        == implementation.qualifier.as_deref()
            }));
            assert_eq!(row["status"], "candidate");
            assert_eq!(row["basis"], "hierarchy-analysis");
            assert!(row["confidence"].is_null());
            let target = row["implementation"]["handle"].as_str().unwrap();
            assert!(
                ok(root, &["project", "show", target])["node"]["signature"].is_object()
                    || qualifier.is_none()
            );
        }
    }
    let concrete = relation_handle(root, "area", Some("A"));
    assert_eq!(
        ok(root, &["project", "implementations", &concrete])["page"]["total"],
        0
    );
}

#[test]
fn relationship_pages_are_complete_and_bound_to_scope_direction_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "trait T { fn run(&self); }\nstruct A;\nstruct B;\nimpl T for A { fn run(&self) {} }\nimpl T for B { fn run(&self) {} }\nfn invoke(t: &dyn T) { t.run(); missing(); }\n");
    for query in ["calls", "implementations"] {
        let full = ok(root, &["project", query, "--limit", "500"]);
        let first = ok(root, &["project", query, "--limit", "1"]);
        let cursor = first["page"]["next"].as_str().unwrap();
        let mut current = first.clone();
        let mut collected = Vec::new();
        loop {
            collected.extend(current["items"].as_array().unwrap().clone());
            let Some(next) = current["page"]["next"].as_str() else {
                break;
            };
            current = ok(root, &["project", query, "--limit", "2", "--cursor", next]);
        }
        assert_eq!(collected, *full["items"].as_array().unwrap());
        assert!(!run(root, &["project", query, "app.rs", "--cursor", cursor]).0);
        assert!(!run(root, &["project", query, "--limit", "0"]).0);
        assert!(!run(root, &["project", query, "--limit", "501"]).0);
        if query == "calls" {
            assert!(
                !run(
                    root,
                    &[
                        "project",
                        query,
                        "--direction",
                        "outgoing",
                        "--cursor",
                        cursor
                    ]
                )
                .0
            );
        }
        let target = relation_handle(root, "T", None);
        let id = target.rsplit(':').next().unwrap();
        let short = ok(
            root,
            &[
                "project",
                query,
                id,
                "--revision",
                full["revision"].as_str().unwrap(),
            ],
        );
        let long = ok(root, &["project", query, &target]);
        assert_eq!(short, long);
        put(root, "new.rs", "fn added() {}\n");
        assert!(!run(root, &["project", query, "--cursor", cursor]).0);
        assert!(!run(root, &["project", query, &target]).0);
        fs::remove_file(root.join("new.rs")).unwrap();
    }
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn relationship_coverage_does_not_turn_unsupported_hierarchies_into_empty_certainty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.zig", "pub fn main() void {}\n");
    put(root, "page.html", "<main>hello</main>\n");
    let implementations = ok(root, &["project", "implementations"]);
    assert_eq!(
        implementations["analysis"]["hierarchy_unsupported_files"]["zig"],
        1
    );
    assert!(implementations["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "zig"));
    let calls = ok(root, &["project", "calls"]);
    assert!(calls["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "html"));
}

#[test]
fn relationship_labels_are_bounded_before_detail_retrieval() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "long_".repeat(100);
    put(
        root,
        "app.py",
        &format!("def {name}():\n    return 'HIDDEN_IMPLEMENTATION'\n\ndef run():\n    {name}()\n"),
    );
    let view = ok(root, &["project", "calls"]);
    let callee = &view["items"][0]["callee"];
    assert_eq!(callee["name"]["omitted_bytes"], 340);
    assert!(!view.to_string().contains("HIDDEN_IMPLEMENTATION"));
    assert!(ok(
        root,
        &["project", "show", callee["handle"].as_str().unwrap()]
    )["node"]["name"]
        .is_object());
}

#[test]
fn route_pages_preserve_each_pattern_readers_declarations() {
    use fun_refactor::lang::Language;
    use fun_refactor::transpile::routes;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let fixtures = [
        ("app.ts", Language::TypeScript, "function pets() { return 'BODY_SENTINEL'; }\napp.get('/pets/:id', pets);\n"),
        ("app.py", Language::Python, "@app.route('/pets/<int:id>', methods=['GET', 'POST'])\ndef pets():\n    return 'BODY_SENTINEL'\n"),
        ("app.rs", Language::Rust, "fn pets() {}\nfn router() { Router::new().route(\"/pets/:id\", get(pets).post(pets)); }\n"),
        ("app.go", Language::Go, "package main\nfunc pets() {}\nfunc routes(r *gin.Engine) { r.GET(\"/pets/:id\", pets) }\n"),
        ("App.java", Language::Java, "class App { @GetMapping(\"/pets/{id}\") String pets() { return \"BODY_SENTINEL\"; } }\n"),
    ];
    for (path, _, source) in fixtures {
        put(root, path, source);
    }
    for (path, language, source) in fixtures {
        let (framework, endpoints) = routes::endpoints_of(source, language).unwrap();
        let view = ok(root, &["project", "routes", path, "--limit", "500"]);
        assert_eq!(view["analysis"]["declarations"], endpoints.len());
        assert_eq!(view["analysis"]["analyzed_files"], 1);
        let items = view["items"].as_array().unwrap();
        for endpoint in endpoints {
            let row = items
                .iter()
                .find(|r| r["kind"] == "route" && r["method"] == endpoint.method)
                .unwrap();
            assert_eq!(row["url"], endpoint.url);
            assert_eq!(row["line"], endpoint.line);
            assert_eq!(row["framework_candidate"], framework.to_string());
            assert_eq!(row["handler"]["name"].as_str(), endpoint.handler.as_deref());
            assert_eq!(row["status"], "candidate");
            assert!(row["confidence"].is_null());
            assert_eq!(row["handler"]["candidate_count"], 1);
            let candidate = items
                .iter()
                .find(|r| r["kind"] == "route-handler" && r["route"] == row["id"])
                .unwrap();
            assert_eq!(candidate["confidence"], "name-only");
            let detail = ok(
                root,
                &[
                    "project",
                    "show",
                    candidate["handler"]["handle"].as_str().unwrap(),
                ],
            );
            assert_eq!(detail["node"]["name"], "pets");
            let file = ok(
                root,
                &["project", "show", row["file_handle"].as_str().unwrap()],
            );
            assert_eq!(file["node"]["kind"], "file");
        }
        assert!(!view.to_string().contains("BODY_SENTINEL"));
    }
}

#[test]
fn route_handler_matches_keep_ambiguity_and_do_not_resolve_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.ts", "class A { run() {} }\nclass B { run() {} }\napp.get('/ambiguous', run);\napp.get('/external', external);\napp.get('/inline', () => 'PRIVATE_BODY');\n");
    put(
        root,
        "other.ts",
        "function external() {}\nfunction run() {}\n",
    );
    let view = ok(root, &["project", "routes", "app.ts"]);
    let items = view["items"].as_array().unwrap();
    let by_url = |url| items.iter().find(|r| r["url"] == url).unwrap();
    assert_eq!(by_url("/ambiguous")["handler"]["status"], "ambiguous");
    assert_eq!(by_url("/ambiguous")["handler"]["candidate_count"], 2);
    assert_eq!(by_url("/external")["handler"]["status"], "unresolved");
    assert_eq!(by_url("/inline")["handler"]["status"], "unnamed");
    let candidates: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-handler")
        .collect();
    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|r| r["handler"]["path"] == "app.ts" && r["status"] == "candidate"));
    assert!(!view.to_string().contains("PRIVATE_BODY"));
}

#[test]
fn route_pages_bind_scope_revision_and_query_and_page_handler_candidates_separately() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "src/app.ts",
        "function getPets() {}\napp.get('/pets', getPets);\napp.post('/pets', getPets);\n",
    );
    let full = ok(root, &["project", "routes", "--limit", "500"]);
    let first = ok(root, &["project", "routes", "--limit", "1"]);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["page"]["total"], 4);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut current = first.clone();
    let mut collected = Vec::new();
    loop {
        collected.extend(current["items"].as_array().unwrap().clone());
        let Some(next) = current["page"]["next"].as_str() else {
            break;
        };
        current = ok(
            root,
            &["project", "routes", "--cursor", next, "--limit", "2"],
        );
    }
    assert_eq!(collected, *full["items"].as_array().unwrap());
    assert!(!run(root, &["project", "routes", "src", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "implementations", "--cursor", cursor]).0);
    for limit in ["0", "501"] {
        assert!(!run(root, &["project", "routes", "--limit", limit]).0);
    }
    let file = relation_handle(root, "app.ts", None);
    let id = file.rsplit(':').next().unwrap();
    let file_view = ok(root, &["project", "routes", &file]);
    assert_eq!(
        file_view,
        ok(
            root,
            &[
                "project",
                "routes",
                id,
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    assert_eq!(file_view, ok(root, &["project", "routes", "src/app.ts"]));
    let handler = relation_handle(root, "getPets", None);
    assert!(!run(root, &["project", "routes", &handler]).0);
    put(
        root,
        "src/app.ts",
        "function getPets() {}\napp.get('/changed', getPets);\n",
    );
    assert!(!run(root, &["project", "routes", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "routes", &file]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn route_coverage_reports_broken_and_unsupported_files_and_limits_framework_claims() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "broken.ts",
        "app.get('/partial', handler);\nfunction broken( {\n",
    );
    put(root, "app.zig", "pub fn main() void {}\n");
    put(
        root,
        "app/api/pets/route.ts",
        "export async function GET() { return Response.json([]); }\n",
    );
    put(root, "fast.py", "from fastapi import FastAPI\napp = FastAPI()\n@app.get('/pets')\ndef pets():\n    return []\n");
    let view = ok(root, &["project", "routes"]);
    assert_eq!(view["analysis"]["syntax_gaps"], 1);
    assert_eq!(view["analysis"]["unsupported_files"]["zig"], 1);
    assert_eq!(view["analysis"]["files_without_patterns"], 0);
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["kind"] == "analysis-gap" && r["path"] == "broken.ts"));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "zig"));
    assert!(!items.iter().any(|r| r["url"] == "/partial"));
    let candidate = items
        .iter()
        .find(|r| r["kind"] == "route" && r["path"] == "fast.py")
        .unwrap();
    assert_eq!(candidate["framework_candidate"], "fastapi");
    assert_eq!(candidate["status"], "candidate");
    assert!(view["analysis"]["certainty"]
        .as_str()
        .unwrap()
        .contains("does not verify framework identity"));
    let subset = ok(root, &["project", "routes", "app"]);
    assert_eq!(subset["page"]["total"], 2);
    assert_eq!(subset["analysis"]["analyzed_files"], 1);
    assert_eq!(subset["analysis"]["syntax_gaps"], 0);
    assert_eq!(
        subset["analysis"]["unsupported_files"],
        serde_json::json!({})
    );
}

#[test]
fn route_fields_clip_unicode_without_losing_handler_handles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "long_".repeat(100);
    let url = format!("/{}", "名".repeat(300));
    put(
        root,
        "app.ts",
        &format!("function {name}() {{ return 'HIDDEN_BODY'; }}\napp.get('{url}', {name});\n"),
    );
    let view = ok(root, &["project", "routes"]);
    let items = view["items"].as_array().unwrap();
    let route = items.iter().find(|r| r["kind"] == "route").unwrap();
    assert_eq!(route["url"]["text"].as_str().unwrap().len(), 511);
    assert_eq!(route["url"]["omitted_bytes"], 390);
    assert_eq!(route["handler"]["name"]["omitted_bytes"], 340);
    assert_eq!(route["handler"]["candidate_count"], 1);
    let candidate = items.iter().find(|r| r["kind"] == "route-handler").unwrap();
    assert!(ok(
        root,
        &[
            "project",
            "show",
            candidate["handler"]["handle"].as_str().unwrap()
        ]
    )["node"]["name"]
        .is_object());
    assert!(!view.to_string().contains("HIDDEN_BODY"));
}

#[test]
fn route_analysis_uses_captured_source_and_final_verification_refuses_drift() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(
        &root,
        "app.ts",
        "function pets() {}\napp.get('/before', pets);\n",
    );
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    put(
        &root,
        "app.ts",
        "function pets() {}\napp.get('/after', pets);\n",
    );
    let query = <Query as clap::Parser>::parse_from(["fr", "routes"]);
    let view = project.report(&query.command).unwrap();
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["url"] == "/before"));
    assert!(!view.to_string().contains("/after"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn next_routes_preserve_api_and_parameter_spelling_and_match_export_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/(shop)/api/pets/[petId]/route.ts", "class Cache { GET(): string { return 'PRIVATE_CACHE'; } }\n\nfunction wrapper() { function GET() { return 'PRIVATE_NESTED'; } }\n\nexport async function GET(request: Request): Promise<Response> { return Response.json('PRIVATE_GET'); }\n\nexport function POST(): Response { return Response.json('PRIVATE_POST'); }\n\nfunction DELETE() {}\n\n");
    put(
        root,
        "src/app/health/route.js",
        "export function GET() { return new Response('PRIVATE_HEALTH'); }\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 3);
    assert_eq!(view["analysis"]["handler_candidates"], 3);
    assert_eq!(view["analysis"]["nextjs_gaps"], 0);
    for route in items.iter().filter(|r| r["kind"] == "route") {
        assert_eq!(route["framework_candidate"], "nextjs-app");
        assert_eq!(route["basis"], "nextjs-app-function-export");
        assert_eq!(route["handler"]["basis"], "declaration-span");
        assert_eq!(route["handler"]["candidate_count"], 1);
        assert!(route["confidence"].is_null());
        let handler = items
            .iter()
            .find(|r| r["kind"] == "route-handler" && r["route"] == route["id"])
            .unwrap();
        assert_eq!(handler["basis"], "declaration-span");
        assert!(handler["confidence"].is_null());
        let shown = ok(
            root,
            &[
                "project",
                "show",
                handler["handler"]["handle"].as_str().unwrap(),
            ],
        );
        assert_eq!(shown["node"]["name"], route["method"]);
        if route["path"] == "src/app/health/route.js" {
            assert_eq!(route["url"], "/health");
        } else {
            assert_eq!(route["url"], "/api/pets/{petId}");
            assert_eq!(handler["handler"]["line"], route["line"]);
            assert_eq!(route["line"], if route["method"] == "GET" { 5 } else { 7 });
        }
    }
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn next_contracts_share_route_ids_and_keep_input_bindings_and_status_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/api/[petId]/route.ts", "export async function POST(request: Request, context: Context): Promise<Response> { return Response.json({ secret: 'PRIVATE_BODY' }, { status: 201 }); }\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let summary = items
        .iter()
        .find(|r| r["kind"] == "route-contract")
        .unwrap();
    assert_eq!(summary["request_fields"], 1);
    assert_eq!(summary["response_fields"], 1);
    assert_eq!(summary["gap_count"], 1);
    assert_eq!(summary["completeness"], "partial");
    assert!(items
        .iter()
        .any(|r| r["direction"] == "request" && r["name"] == "petId" && r["location"] == "path"));
    assert!(items
        .iter()
        .any(|r| r["direction"] == "response" && r["declared_type"] == "Promise<Response>"));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "route-contract-gap" && r["parameters"] == 2));
    let routes = ok(root, &["project", "routes", "--limit", "500"]);
    let declarations: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route" || r["kind"] == "route-handler")
        .cloned()
        .collect();
    assert_eq!(declarations, *routes["items"].as_array().unwrap());
    assert!(!view.to_string().contains("PRIVATE_BODY"));
    assert!(!items.iter().any(|r| r["status_code"] == 201));
}

#[test]
fn next_routes_report_unsupported_paths_and_exports_without_guessing_methods() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in [
        "app/files/[...path]/extra/route.ts",
        "app/docs/[[parts]]/route.ts",
        "app/_private/route.ts",
        "app/@slot/route.ts",
        "app/(.)photo/route.ts",
        "app/api/[bad-name]/route.ts",
        "app/%5Fsecret/route.ts",
        "app/query?name/route.ts",
        "app/fragment#name/route.ts",
    ] {
        put(root, path, "export function GET() {}\n");
    }
    put(
        root,
        "app/variable/route.ts",
        "export const GET = wrap(() => new Response('PRIVATE'));\n",
    );
    put(
        root,
        "app/reexport/route.ts",
        "export { handler as POST } from './other';\n",
    );
    put(root, "app/star/route.ts", "export * from './other';\n");
    put(
        root,
        "app/default/route.ts",
        "export default function GET() {}\n",
    );
    put(
        root,
        "app/config/route.ts",
        "export const runtime = 'edge';\n",
    );
    put(
        root,
        "pages/api/legacy.ts",
        "export default function handler(req, res) {}\n",
    );
    put(
        root,
        "packages/client/app/api/route.ts",
        "export function GET() {}\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 0);
    assert_eq!(view["analysis"]["nextjs_gaps"], 14, "{view}");
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .all(|r| r["kind"] == "analysis-gap" && r["basis"] == "nextjs-app-reader"));
    assert!(items.iter().all(|r| r["line"].as_u64().unwrap() >= 1));
    assert!(!view.to_string().contains("PRIVATE"));
}

#[test]
fn next_routes_keep_duplicate_exports_and_ignore_method_names_inside_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "export function GET(): Response { throw 'PRIVATE_ONE'; }\n\nexport function GET(): OtherResponse { throw 'PRIVATE_TWO'; }\n\nexport const config = { GET: 'not a handler' };\n\nexport type POST = string;\n\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 2);
    assert_eq!(view["analysis"]["nextjs_gaps"], 1);
    let items = view["items"].as_array().unwrap();
    let handlers: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-handler")
        .collect();
    assert_eq!(handlers.len(), 2);
    assert_ne!(
        handlers[0]["handler"]["handle"],
        handlers[1]["handler"]["handle"]
    );
    for route in items.iter().filter(|r| r["kind"] == "route") {
        assert_eq!(route["url"], "/");
        assert_eq!(route["handler"]["candidate_count"], 1);
    }
    assert!(items.iter().any(|r| r["declared_type"] == "Response"));
    assert!(items.iter().any(|r| r["declared_type"] == "OtherResponse"));
}

#[test]
fn next_catch_all_contracts_preserve_cardinality_and_parameter_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "app/(shop)/api/[Owner]/[...Parts]/route.ts",
        "export function GET(): Response { throw 'PRIVATE_BODY'; }\n",
    );
    put(
        root,
        "src/app/docs/[[...Path]]/(group)/route.js",
        "export function POST() {}\n",
    );
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 2);
    assert_eq!(view["analysis"]["nextjs_gaps"], 0);
    for (url, name, kind, minimum) in [
        ("/api/{Owner}/{...Parts}", "Parts", "catch-all", 1),
        ("/docs/{...Path?}", "Path", "optional-catch-all", 0),
    ] {
        let route = items.iter().find(|r| r["url"] == url).unwrap();
        let field = items
            .iter()
            .find(|r| r["route"] == route["id"] && r["name"] == name)
            .unwrap();
        assert_eq!(field["segment_kind"], kind);
        assert_eq!(field["min_segments"], minimum);
        assert!(field["max_segments"].is_null());
        assert!(field["required"].is_null());
        assert!(field["declared_type"].is_null());
        assert_eq!(field["basis"], "nextjs-catch-all-path");
    }
    assert!(items
        .iter()
        .any(|r| r["name"] == "Owner" && r["basis"] == "literal-path-segment"));
    assert!(!items
        .iter()
        .any(|r| r["kind"] == "route-contract-gap" && r["basis"] == "literal-path-segment"));
    assert!(!view.to_string().contains("PRIVATE_BODY"));
}

#[test]
fn next_catch_all_paths_reject_malformed_names_repeats_and_later_segments() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in [
        "app/a/[...]/route.ts",
        "app/b/[[...]]/route.ts",
        "app/c/[...bad-name]/route.ts",
        "app/d/[[name]]/route.ts",
        "app/e/[...parts]/tail/route.ts",
        "app/f/[id]/[...id]/route.ts",
        "app/g/[id]/[id]/route.ts",
        "app/h/[[...parts]]/(group)/tail/route.ts",
    ] {
        put(root, path, "export function GET() {}\n");
    }
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 0);
    assert_eq!(view["analysis"]["nextjs_gaps"], 8);
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["kind"] == "analysis-gap"));
}

#[test]
fn next_local_export_aliases_follow_only_top_level_function_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "function handle(request: Request): Promise<Response> { throw 'PRIVATE_BODY'; }\n\nfunction wrapper() { function handle(): Wrong { throw 'PRIVATE_NESTED'; } }\n\nexport { handle as GET, handle as POST, handle as PUT, handle as PATCH, handle as DELETE, handle as HEAD, handle as OPTIONS };\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 7);
    assert_eq!(view["analysis"]["handler_candidates"], 7);
    assert_eq!(view["analysis"]["nextjs_gaps"], 0);
    for route in items.iter().filter(|r| r["kind"] == "route") {
        assert_eq!(route["handler"]["name"], "handle");
        assert_eq!(route["basis"], "nextjs-app-local-function-export");
        assert_eq!(route["line"], 5);
        let candidate = items
            .iter()
            .find(|r| r["kind"] == "route-handler" && r["route"] == route["id"])
            .unwrap();
        assert_eq!(candidate["handler"]["line"], 1);
        assert_eq!(candidate["basis"], "declaration-span");
        let shown = ok(
            root,
            &[
                "project",
                "show",
                candidate["handler"]["handle"].as_str().unwrap(),
            ],
        );
        assert_eq!(shown["node"]["name"], "handle");
        assert!(items
            .iter()
            .any(|r| r["route"] == route["id"] && r["declared_type"] == "Promise<Response>"));
    }
    assert!(!view.to_string().contains("Wrong"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn next_local_exports_preserve_duplicate_definitions_and_export_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "function handle(): One { throw 1; }\n\nfunction handle(): Two { throw 2; }\n\nexport { handle as POST };\n\nexport function GET(): Three { throw 3; }\n\nexport { GET };\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 4);
    assert_eq!(view["analysis"]["nextjs_gaps"], 1);
    assert_eq!(
        items
            .iter()
            .filter(|r| r["kind"] == "route" && r["method"] == "POST")
            .count(),
        2
    );
    for ty in ["One", "Two", "Three"] {
        assert!(items.iter().any(|r| r["declared_type"] == ty));
    }
    let handles: std::collections::BTreeSet<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-handler")
        .map(|r| r["handler"]["handle"].as_str().unwrap())
        .collect();
    assert_eq!(handles.len(), 3);
}

#[test]
fn next_local_export_reader_leaves_imports_variables_and_type_exports_unresolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "import { remote } from './other';\n\nfunction local(): Response { throw 0; }\n\nconst variable = wrap(() => new Response('PRIVATE_BODY'));\n\nexport { local as GET, variable as POST, remote as PATCH, missing as DELETE };\n\nexport { local as PUT } from './other';\n\nexport type { local as HEAD };\n\nexport { type local as OPTIONS };\n");
    put(
        root,
        "app/other.ts",
        "export function remote() {}\n\nexport function local() {}\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 1, "{view}");
    assert_eq!(view["analysis"]["nextjs_gaps"], 2, "{view}");
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["method"] == "GET" && r["handler"]["name"] == "local"));
    assert!(!view.to_string().contains("PRIVATE_BODY"));
}

#[test]
fn next_catch_all_alias_pages_keep_route_ids_and_reject_stale_exports() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = "app/[[...parts]]/route.ts";
    put(
        root,
        path,
        "function handle(): Response { throw 0; }\n\nexport { handle as GET, handle as POST };\n",
    );
    let full = ok(root, &["project", "contracts", "--limit", "500"]);
    let first = ok(root, &["project", "contracts", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap().to_owned();
    let mut combined = first["items"].as_array().unwrap().clone();
    let mut page = first;
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &["project", "contracts", "--limit", "2", "--cursor", next],
        );
        assert!(page["items"].as_array().unwrap().len() <= 2);
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    let routes = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(
        combined
            .iter()
            .filter(|r| r["kind"] == "route" || r["kind"] == "route-handler")
            .cloned()
            .collect::<Vec<_>>(),
        *routes["items"].as_array().unwrap()
    );
    assert!(!run(root, &["project", "routes", "--cursor", &cursor]).0);
    assert!(!run(root, &["project", "contracts", path, "--cursor", &cursor]).0);
    let handle = combined
        .iter()
        .find(|r| r["kind"] == "route-handler")
        .unwrap()["handler"]["handle"]
        .as_str()
        .unwrap();
    put(
        root,
        path,
        "function handle(): Response { throw 0; }\n\nexport { handle as DELETE };\n",
    );
    assert!(!run(root, &["project", "contracts", "--cursor", &cursor]).0);
    assert!(!run(root, &["project", "show", handle]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn next_catch_all_contracts_bound_unicode_names_and_preserve_full_markers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(80);
    put(
        root,
        &format!("app/[...{name}]/route.ts"),
        "function handle(): Response { throw 0; }\n\nexport { handle as GET };\n",
    );
    put(
        root,
        &format!(
            "app/{}/{}/[...{name}]/route.ts",
            "x".repeat(200),
            "y".repeat(200)
        ),
        "export function GET(): Response { throw 0; }\n",
    );
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let field = items
        .iter()
        .find(|r| r["basis"] == "nextjs-catch-all-path")
        .unwrap();
    assert_eq!(field["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(field["name"]["omitted_bytes"], name.len() - 159);
    assert!(items.iter().any(|r| r["url"] == format!("/{{...{name}}}")));
    let clipped = items
        .iter()
        .find(|r| r["url"]["omitted_bytes"].as_u64().is_some_and(|n| n > 0))
        .unwrap();
    assert!(clipped["url"]["text"].as_str().unwrap().len() <= 512);
    assert!(items.iter().any(|r| r["route"] == clipped["id"]
        && r["basis"] == "nextjs-catch-all-path"
        && r["name"]["omitted_bytes"] == name.len() - 159));
    assert_eq!(view["analysis"]["nextjs_gaps"], 0);
}

#[test]
fn next_catch_all_alias_analysis_uses_captured_declarations_and_exports() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let file = "app/[...parts]/route.ts";
    put(
        &root,
        file,
        "function handle(): Before { throw 0; }\n\nexport { handle as GET };\n",
    );
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join(file)).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts"]);
    let view = project.report(&query.command).unwrap();
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["method"] == "GET" && r["handler"]["name"] == "handle"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "parts" && r["segment_kind"] == "catch-all"));
    assert!(items.iter().any(|r| r["declared_type"] == "Before"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn next_variable_handlers_read_initializer_signatures_and_keep_inputs_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "export const GET = async (req: Request = PRIVATE_DEFAULT): Promise<Response> => { throw 'PRIVATE_BODY'; };\n\nexport let POST = function inner(): Created { throw 'PRIVATE_INNER'; };\n\nexport var PUT = (): Updated => make();\n\nexport const PATCH = req => req;\n\nexport const DELETE: Handler = () => make(), HEAD = (): HeadResult => make();\n\nexport const OPTIONS = function(): OptionResult { return make(); };\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 7, "{view}");
    assert_eq!(view["analysis"]["handler_candidates"], 7);
    assert_eq!(view["analysis"]["nextjs_gaps"], 0);
    for (method, ty) in [
        ("GET", "Promise<Response>"),
        ("POST", "Created"),
        ("PUT", "Updated"),
        ("HEAD", "HeadResult"),
        ("OPTIONS", "OptionResult"),
    ] {
        let route = items.iter().find(|r| r["method"] == method).unwrap();
        assert_eq!(route["basis"], "nextjs-app-variable-export");
        assert!(items
            .iter()
            .any(|r| r["route"] == route["id"] && r["declared_type"] == ty));
        let candidate = items
            .iter()
            .find(|r| r["kind"] == "route-handler" && r["route"] == route["id"])
            .unwrap();
        let shown = ok(
            root,
            &[
                "project",
                "show",
                candidate["handler"]["handle"].as_str().unwrap(),
            ],
        );
        assert_eq!(shown["node"]["name"], method);
    }
    for method in ["GET", "PATCH"] {
        let route = items.iter().find(|r| r["method"] == method).unwrap();
        assert!(items.iter().any(|r| r["route"] == route["id"]
            && r["kind"] == "route-contract-gap"
            && r["parameters"] == 1));
    }
    assert!(!items.iter().any(|r| r["declared_type"] == "Handler"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn next_variable_aliases_preserve_binding_positions_and_duplicate_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "const handle = (): One => make();\n\nconst handle = function internal(): Two { return make(); };\n\nfunction wrapper() { const handle = (): Wrong => make(); }\n\nexport { handle as GET, handle as POST };\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 4);
    assert_eq!(view["analysis"]["handler_candidates"], 4);
    assert_eq!(view["analysis"]["nextjs_gaps"], 1);
    for route in items.iter().filter(|r| r["kind"] == "route") {
        assert_eq!(route["basis"], "nextjs-app-local-variable-export");
        assert_eq!(route["line"], 7);
        assert_eq!(route["handler"]["name"], "handle");
    }
    assert!(items.iter().any(|r| r["declared_type"] == "One"));
    assert!(items.iter().any(|r| r["declared_type"] == "Two"));
    assert!(!view.to_string().contains("Wrong"));
    let handles: std::collections::BTreeSet<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-handler")
        .map(|r| r["handler"]["handle"].as_str().unwrap())
        .collect();
    assert_eq!(handles.len(), 2);
}

#[test]
fn next_variable_reader_reports_wrappers_aliases_generators_and_assertions_as_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (name, source) in [
        ("wrapper", "export const GET = wrap(() => make());"),
        (
            "alias",
            "const local = () => make(); export const GET = local;",
        ),
        (
            "generator",
            "export const GET = function*() { yield make(); };",
        ),
        ("assertion", "export const GET = (() => make()) as Handler;"),
        ("member", "export const GET = handlers.get;"),
        ("destructure", "export const { GET } = makeHandlers();"),
    ] {
        put(root, &format!("app/{name}/route.ts"), source);
    }
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 0, "{view}");
    assert_eq!(view["analysis"]["nextjs_gaps"], 6);
}

#[test]
fn next_nested_projects_use_captured_next_dependencies_and_package_relative_urls() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "apps/shop/package.json",
        r#"{"dependencies":{"next":"PRIVATE_VERSION"}}"#,
    );
    put(
        root,
        "apps/shop/app/api/[id]/route.ts",
        "export const GET = (): Response => make();\n",
    );
    put(
        root,
        "apps/shop/src/app/status/route.js",
        "export const POST = () => make();\n",
    );
    put(
        root,
        "apps/docs/package.json",
        r#"{"devDependencies":{"next":"16"}}"#,
    );
    put(
        root,
        "apps/docs/src/app/[[...parts]]/route.ts",
        "const local = (): Response => make(); export { local as GET };\n",
    );
    put(
        root,
        "apps/unknown/app/route.ts",
        "export function GET() {}\n",
    );
    put(root, "app/route.ts", "export function GET() {}\n");
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 4, "{view}");
    for (url, package) in [
        ("/api/{id}", "apps/shop"),
        ("/status", "apps/shop"),
        ("/{...parts?}", "apps/docs"),
    ] {
        let route = items.iter().find(|r| r["url"] == url).unwrap();
        assert_eq!(route["nextjs_project"]["root"], package);
        assert_eq!(
            route["nextjs_project"]["manifest"],
            format!("{package}/package.json")
        );
        assert_eq!(
            route["nextjs_project"]["basis"],
            "observed-npm-next-dependency"
        );
    }
    assert!(items
        .iter()
        .any(|r| r["url"] == "/" && r["nextjs_project"]["basis"] == "project-root-layout"));
    assert!(!view.to_string().contains("PRIVATE_VERSION"));
}

#[test]
fn next_nested_projects_stop_at_nearest_manifest_and_preserve_invalid_boundary_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "apps/shop/package.json",
        r#"{"dependencies":{"next":"16"}}"#,
    );
    for (directory, manifest) in [
        ("plain", "{}"),
        ("bad", "{ broken"),
        ("typed", r#"{"dependencies":{"next":true}}"#),
        ("peer", r#"{"peerDependencies":{"next":"16"}}"#),
    ] {
        put(
            root,
            &format!("apps/shop/app/{directory}/package.json"),
            manifest,
        );
        put(
            root,
            &format!("apps/shop/app/{directory}/app/route.ts"),
            "export const GET = () => make();\n",
        );
    }
    put(
        root,
        "apps/shop/app/child/package.json",
        r#"{"dependencies":{"next":"16"}}"#,
    );
    put(
        root,
        "apps/shop/app/child/app/route.ts",
        "export const GET = () => make();\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(view["analysis"]["declarations"], 1, "{view}");
    assert_eq!(view["analysis"]["nextjs_gaps"], 4);
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["url"] == "/" && r["nextjs_project"]["root"] == "apps/shop/app/child"));
}

#[test]
fn next_nested_variable_pages_bind_manifest_revision_scope_and_handler_handles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "apps/site/package.json",
        r#"{"dependencies":{"next":"16"}}"#,
    );
    let file = "apps/site/app/[...parts]/route.ts";
    put(
        root,
        file,
        "const local = (): Response => make(); export { local as GET, local as POST };\n",
    );
    let full = ok(root, &["project", "contracts", "--limit", "500"]);
    let first = ok(root, &["project", "contracts", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap().to_owned();
    let mut page = first;
    let mut combined = page["items"].as_array().unwrap().clone();
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &["project", "contracts", "--limit", "2", "--cursor", next],
        );
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    let routes = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(
        combined
            .iter()
            .filter(|r| r["kind"] == "route"
                || r["kind"] == "route-handler"
                || r["kind"] == "coverage-gap")
            .cloned()
            .collect::<Vec<_>>(),
        *routes["items"].as_array().unwrap()
    );
    assert!(
        !run(
            root,
            &["project", "contracts", "apps/site", "--cursor", &cursor]
        )
        .0
    );
    let candidate = combined
        .iter()
        .find(|r| r["kind"] == "route-handler")
        .unwrap()["handler"]["handle"]
        .as_str()
        .unwrap();
    ok(root, &["project", "show", candidate]);
    let scoped = ok(root, &["project", "contracts", file]);
    assert_eq!(scoped["analysis"]["declarations"], 2);
    put(root, "apps/site/package.json", "{}");
    assert!(!run(root, &["project", "contracts", "--cursor", &cursor]).0);
    assert!(!run(root, &["project", "show", candidate]).0);
    assert_eq!(
        ok(root, &["project", "routes"])["analysis"]["declarations"],
        0
    );
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn next_nested_variables_read_captured_manifests_and_sources_after_removal() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(
        &root,
        "nested/package.json",
        r#"{"dependencies":{"next":"16"}}"#,
    );
    put(
        &root,
        "nested/app/route.ts",
        "export const GET = (): Before => make();\n",
    );
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join("nested/package.json")).unwrap();
    fs::remove_file(root.join("nested/app/route.ts")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts"]);
    let view = project.report(&query.command).unwrap();
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["declared_type"] == "Before"));
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["nextjs_project"]["manifest"] == "nested/package.json"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn next_variable_aliases_bound_names_and_initializer_return_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(80);
    let ty = "Type".repeat(160);
    put(
        root,
        "app/route.ts",
        &format!("const {name} = (): {ty} => make();\n\nexport {{ {name} as GET }};\n"),
    );
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let route = items.iter().find(|r| r["kind"] == "route").unwrap();
    assert_eq!(route["handler"]["candidate_count"], 1);
    assert_eq!(
        route["handler"]["name"]["text"].as_str().unwrap().len(),
        159
    );
    assert_eq!(route["handler"]["name"]["omitted_bytes"], name.len() - 159);
    let returned = items.iter().find(|r| r["location"] == "return").unwrap();
    assert_eq!(
        returned["declared_type"]["text"].as_str().unwrap().len(),
        512
    );
    assert_eq!(returned["declared_type"]["omitted_bytes"], ty.len() - 512);
}

#[test]
fn next_route_pages_bind_scope_revision_and_contract_queries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
    let source: String = methods
        .iter()
        .map(|method| format!("export function {method}(): Response {{ throw 'PRIVATE'; }}\n"))
        .collect();
    put(root, "app/api/route.ts", &source);
    let full = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(full["page"]["total"], 14);
    let first = ok(root, &["project", "routes", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut combined = first["items"].as_array().unwrap().clone();
    let mut page = first.clone();
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &["project", "routes", "--cursor", next, "--limit", "3"],
        );
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    assert!(!run(root, &["project", "contracts", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "routes", "app", "--cursor", cursor]).0);
    let file = relation_handle(root, "route.ts", None);
    let by_file = ok(root, &["project", "routes", &file, "--limit", "500"]);
    assert_eq!(by_file["items"], full["items"]);
    assert_eq!(
        by_file,
        ok(
            root,
            &[
                "project",
                "routes",
                file.rsplit(':').next().unwrap(),
                "--revision",
                full["revision"].as_str().unwrap(),
                "--limit",
                "500"
            ]
        )
    );
    put(
        root,
        "app/api/route.ts",
        &source.replace("GET", "GET_CHANGED"),
    );
    assert!(!run(root, &["project", "routes", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "routes", &file]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn framework_features_join_routes_handlers_and_schemas_into_selectable_subtrees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "web/package.json",
        r#"{"name":"web","scripts":{"build":"next build"},"dependencies":{"next":"16","@demo/ui":"file:packages/ui"}}"#,
    );
    put(
        root,
        "web/packages/ui/package.json",
        r#"{"name":"@demo/ui","version":"1.0.0"}"#,
    );
    put(
        root,
        "web/app/api/pets/route.ts",
        concat!(
            "interface Pet { id: string; }\n",
            "export function GET(): Pet {\n",
            "  process.env.NEXT_PUBLIC_SITE; process.env.UNDECLARED;\n",
            "  fetch('https://catalog.example/pets?token=PRIVATE_QUERY');\n",
            "  fetch(dynamicEndpoint); throw new Error('PRIVATE_NEXT');\n",
            "}\n",
            "export function POST(): Pet { throw new Error('PRIVATE_NEXT'); }\n",
        ),
    );
    put(
        root,
        "web/proxy.ts",
        "export function proxy() { throw new Error('PRIVATE_PROXY'); }\n",
    );
    put(
        root,
        "web/instrumentation.ts",
        "export function register() {}\nexport const onRequestError = () => {};\n",
    );
    put(
        root,
        "web/app/api/pets/page.tsx",
        concat!(
            "'use client';\n",
            "import { useEffect, useState } from 'react';\n",
            "interface Props { initial: number; }\n",
            "function Badge() { return <span>ok</span>; }\n",
            "export default function Pets({ initial }: Props) {\n",
            "  const [count, setCount] = useState(initial);\n",
            "  useEffect(() => { console.log('PRIVATE_EFFECT'); }, [count]);\n",
            "  return <Panel className=\"PRIVATE_CLASS\" onClick={() => setCount(count + 1)}><Badge /></Panel>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "api/app.py",
        concat!(
            "from typing import Annotated\n",
            "import httpx, requests\n",
            "from fastapi import Depends, FastAPI, Security\n",
            "from fastapi.middleware.cors import CORSMiddleware\n",
            "class Pet:\n    id: int\n",
            "async def app_lifespan(app): yield\n",
            "def load_user(): pass\n",
            "def check_scope(): pass\n",
            "def global_guard(): pass\n",
            "def route_guard(): pass\n",
            "app = FastAPI(lifespan=app_lifespan, ",
            "dependencies=[Security(global_guard)])\n",
            "@app.middleware('http')\n",
            "async def timing(request, call_next):\n",
            "    return await call_next(request)\n",
            "app.add_middleware(CORSMiddleware, allow_origins=[])\n",
            "@app.get('/pets', dependencies=[Depends(route_guard)])\n",
            "def pets(user=Depends(load_user), ",
            "guard: Annotated[str, Security(check_scope)] = None) -> Pet:\n",
            "    os.environ['API_URL']\n",
            "    requests.get('https://PRIVATE_USER:PRIVATE_PASS@billing.example/pets#PRIVATE_FRAGMENT')\n",
            "    httpx.post(dynamic_url)\n",
            "    raise RuntimeError('PRIVATE_FAST')\n",
        ),
    );
    put(
        root,
        "deploy/compose.yaml",
        "services:\n  app:\n    environment:\n      API_URL: PRIVATE_API_VALUE\n      NEXT_PUBLIC_SITE: PRIVATE_SITE_VALUE\n",
    );

    let full = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(full["analysis"]["applications"], 2, "{full}");
    assert_eq!(full["analysis"]["features"], 2);
    assert_eq!(full["analysis"]["routes"], 3);
    assert_eq!(full["analysis"]["handlers"], 3);
    assert_eq!(full["analysis"]["schemas"], 3);
    assert_eq!(full["analysis"]["packages"], 1);
    assert_eq!(full["analysis"]["build_settings"], 1);
    assert_eq!(full["analysis"]["dependencies"], 2);
    assert_eq!(full["analysis"]["package_gaps"], 1);
    assert_eq!(full["analysis"]["middleware"], 3);
    assert_eq!(full["analysis"]["execution_dependencies"], 4);
    assert_eq!(full["analysis"]["authentication_candidates"], 2);
    assert_eq!(full["analysis"]["lifecycle_hooks"], 3);
    assert_eq!(full["analysis"]["lifecycle_gaps"], 0);
    assert_eq!(full["analysis"]["runtime_configurations"], 3);
    assert_eq!(full["analysis"]["configuration_consumers"], 3);
    assert_eq!(full["analysis"]["service_dependencies"], 2);
    assert_eq!(full["analysis"]["service_gaps"], 2);
    assert_eq!(full["analysis"]["components"], 2, "{full}");
    assert_eq!(full["analysis"]["component_properties"], 1, "{full}");
    assert_eq!(full["analysis"]["component_states"], 1, "{full}");
    assert_eq!(full["analysis"]["component_effects"], 1, "{full}");
    assert_eq!(full["analysis"]["component_events"], 1, "{full}");
    assert_eq!(full["analysis"]["component_styles"], 1, "{full}");
    assert_eq!(full["analysis"]["render_edges"], 2, "{full}");
    let items = full["items"].as_array().unwrap();
    for fact in items {
        assert!(fact.get("id").is_some(), "{fact}");
        assert!(fact.get("parent").is_some(), "{fact}");
        assert!(fact.get("source").is_some(), "{fact}");
        assert!(fact.get("status").is_some(), "{fact}");
        assert!(fact.get("confidence").is_some(), "{fact}");
        assert!(fact["evidence"]["basis"] != Value::Null, "{fact}");
        assert!(
            fact["evidence"]["validation"]
                == serde_json::json!(["captured-source", "syntax-tree", "project-revision"])
                || fact["evidence"]["validation"]
                    == serde_json::json!(["captured-manifest", "project-revision"]),
            "{fact}"
        );
        assert!(fact["gaps"].is_array(), "{fact}");
    }
    for kind in [
        "application",
        "feature",
        "route",
        "handler",
        "contract-field",
        "schema-reference",
        "schema-candidate",
        "schema",
        "schema-field",
        "package",
        "build-setting",
        "dependency",
        "package-gap",
        "middleware",
        "execution-dependency",
        "lifecycle-hook",
        "runtime-configuration",
        "configuration-consumer",
        "service-dependency",
        "service-gap",
        "component",
        "component-properties",
        "component-state",
        "component-effect",
        "component-event",
        "component-style",
        "component-render",
    ] {
        assert!(
            items.iter().any(|fact| fact["kind"] == kind),
            "{kind}: {full}"
        );
    }
    assert!(!full.to_string().contains("PRIVATE_"));

    let next_app = items
        .iter()
        .find(|fact| {
            fact["kind"] == "application" && fact["application"]["framework"] == "nextjs-app"
        })
        .unwrap();
    let next_feature = items
        .iter()
        .find(|fact| fact["kind"] == "feature" && fact["parent"] == next_app["id"])
        .unwrap();
    assert_eq!(next_feature["feature"]["route_count"], 2);
    let pets_component = items
        .iter()
        .find(|fact| {
            fact["kind"] == "component"
                && fact["parent"] == next_feature["id"]
                && fact["component"]["name"] == "Pets"
        })
        .unwrap();
    assert_eq!(pets_component["component"]["rendering_boundary"], "client");
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-properties"
            && fact["parent"] == pets_component["id"]
            && fact["properties"]["names"] == serde_json::json!(["initial"])
            && fact["properties"]["declared_type"] == "Props"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-state"
            && fact["parent"] == pets_component["id"]
            && fact["state"]["binding"] == "count"
            && fact["state"]["setter"] == "setCount"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-effect"
            && fact["parent"] == pets_component["id"]
            && fact["effect"]["schedule"] == "dependency-change-candidate"
            && fact["effect"]["dependency_count"] == 1
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-render"
            && fact["parent"] == pets_component["id"]
            && fact["render"]["target"] == "Badge"
    }));
    assert!(
        items.iter().any(|fact| {
            fact["kind"] == "component-event"
                && fact["parent"] == pets_component["id"]
                && fact["event"]["name"] == "onClick"
                && fact["event"]["handler_kind"] == "inline"
        }),
        "{full}"
    );
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-style"
            && fact["parent"] == pets_component["id"]
            && fact["style"]["attribute"] == "className"
            && fact["style"]["value_kind"] == "literal"
    }));
    let package = items
        .iter()
        .find(|fact| fact["kind"] == "package" && fact["parent"] == next_app["id"])
        .unwrap();
    assert!(items.iter().any(|fact| {
        fact["kind"] == "build-setting"
            && fact["parent"] == package["id"]
            && fact["build_setting"]["name"] == "build"
            && fact["build_setting"]["command"] == "next build"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "dependency"
            && fact["parent"] == package["id"]
            && fact["dependency"]["name"] == "@demo/ui"
            && fact["dependency"]["boundary"] == "local-package"
            && fact["dependency"]["target_manifest"] == "web/packages/ui/package.json"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "dependency"
            && fact["parent"] == package["id"]
            && fact["dependency"]["name"] == "next"
            && fact["dependency"]["boundary"] == "external-or-unresolved"
    }));
    let fast_app = items
        .iter()
        .find(|fact| fact["kind"] == "application" && fact["application"]["framework"] == "fastapi")
        .unwrap();
    let mut fast_middleware: Vec<_> = items
        .iter()
        .filter(|fact| fact["kind"] == "middleware" && fact["parent"] == fast_app["id"])
        .collect();
    fast_middleware.sort_by_key(|fact| fact["middleware"]["declaration_order"].as_u64());
    assert_eq!(fast_middleware.len(), 2, "{full}");
    assert_eq!(fast_middleware[0]["middleware"]["name"], "timing");
    assert_eq!(fast_middleware[0]["middleware"]["request_order"], 2);
    assert_eq!(fast_middleware[1]["middleware"]["name"], "CORSMiddleware");
    assert_eq!(fast_middleware[1]["middleware"]["request_order"], 1);
    let route = items
        .iter()
        .find(|fact| {
            fact["kind"] == "route"
                && fact["parent"].as_str().is_some_and(|parent| {
                    items.iter().any(|candidate| {
                        candidate["id"] == parent && candidate["parent"] == fast_app["id"]
                    })
                })
        })
        .unwrap();
    let dependencies: Vec<_> = items
        .iter()
        .filter(|fact| fact["kind"] == "execution-dependency" && fact["parent"] == route["id"])
        .collect();
    assert_eq!(dependencies.len(), 3, "{full}");
    assert!(dependencies.iter().any(|fact| {
        fact["execution_dependency"]["provider"] == "load_user"
            && fact["execution_dependency"]["authentication_candidate"] == false
    }));
    assert!(dependencies.iter().any(|fact| {
        fact["execution_dependency"]["provider"] == "check_scope"
            && fact["execution_dependency"]["authentication_candidate"] == true
    }));
    assert!(dependencies.iter().any(|fact| {
        fact["execution_dependency"]["provider"] == "route_guard"
            && fact["execution_dependency"]["scope"] == "route"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "execution-dependency"
            && fact["parent"] == fast_app["id"]
            && fact["execution_dependency"]["provider"] == "global_guard"
            && fact["execution_dependency"]["scope"] == "application"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "middleware"
            && fact["parent"] == next_app["id"]
            && fact["middleware"]["form"] == "nextjs-proxy"
            && fact["middleware"]["deprecated_convention"] == false
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "lifecycle-hook"
            && fact["parent"] == next_app["id"]
            && fact["lifecycle"]["phase"] == "startup"
            && fact["lifecycle"]["exported_as"] == "register"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "lifecycle-hook"
            && fact["parent"] == fast_app["id"]
            && fact["lifecycle"]["form"] == "fastapi-lifespan"
            && fact["lifecycle"]["name"] == "app_lifespan"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "runtime-configuration"
            && fact["parent"] == next_app["id"]
            && fact["configuration"]["name"] == "NEXT_PUBLIC_SITE"
            && fact["configuration"]["visibility"] == "client-build-time-candidate"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "runtime-configuration"
            && fact["parent"] == next_app["id"]
            && fact["configuration"]["name"] == "UNDECLARED"
            && fact["status"] == "no-observed-declaration"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "runtime-configuration"
            && fact["parent"] == fast_app["id"]
            && fact["configuration"]["name"] == "API_URL"
            && fact["configuration"]["visibility"] == "server-process-candidate"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "service-dependency"
            && fact["service_dependency"]["target"] == "https://catalog.example/pets"
            && fact["service_dependency"]["query_or_fragment_omitted"] == true
            && fact["service_dependency"]["credentials_omitted"] == false
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "service-dependency"
            && fact["service_dependency"]["target"] == "https://billing.example/pets"
            && fact["service_dependency"]["method"] == "GET"
            && fact["service_dependency"]["query_or_fragment_omitted"] == true
            && fact["service_dependency"]["credentials_omitted"] == true
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "package-gap"
            && fact["parent"] == fast_app["id"]
            && fact["gap"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("Python package manifests"))
    }));
    let feature_id = next_feature["id"].as_str().unwrap();
    let selected = ok(
        root,
        &[
            "project",
            "features",
            "--feature",
            feature_id,
            "--limit",
            "500",
        ],
    );
    assert_eq!(selected["analysis"]["applications"], 1);
    assert_eq!(selected["analysis"]["features"], 1);
    assert!(selected["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|fact| fact["source"]["path"] != "api/app.py"));

    let first = ok(root, &["project", "features", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    assert!(
        !run(
            root,
            &[
                "project",
                "features",
                "--feature",
                feature_id,
                "--cursor",
                cursor,
            ],
        )
        .0
    );
    assert!(!run(root, &["project", "features", "--feature", "frff1:absent"]).0);
}

#[test]
fn framework_features_include_frontend_only_next_pages_and_client_boundary_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "package.json",
        r#"{"dependencies":{"next":"16","react":"19"}}"#,
    );
    put(
        root,
        "app/page.tsx",
        concat!(
            "import { useState } from 'react';\n",
            "export default function Home() {\n",
            "  const [value, setValue] = useState(0);\n",
            "  return <button onClick={() => setValue(value + 1)}>Count</button>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "app/[bad-name]/page.tsx",
        "export default function Hidden() { return <div />; }\n",
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["applications"], 1, "{view}");
    assert_eq!(view["analysis"]["features"], 1, "{view}");
    assert_eq!(view["analysis"]["routes"], 0, "{view}");
    assert_eq!(view["analysis"]["components"], 1, "{view}");
    assert_eq!(view["analysis"]["component_gaps"], 2, "{view}");
    let items = view["items"].as_array().unwrap();
    let feature = items.iter().find(|fact| fact["kind"] == "feature").unwrap();
    assert_eq!(feature["feature"]["route_path"], "/");
    let component = items
        .iter()
        .find(|fact| fact["kind"] == "component")
        .unwrap();
    assert_eq!(
        component["component"]["rendering_boundary"],
        "server-default"
    );
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-state"
            && fact["parent"] == component["id"]
            && fact["status"] == "conflict"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["gap"]["reason"] == "The page has a malformed dynamic parameter."
    }));
}

#[test]
fn framework_feature_components_are_bounded_with_an_explicit_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    let components: String = (0..129)
        .map(|number| format!("function Component{number}() {{ return <div />; }}\n"))
        .collect();
    put(root, "app/page.tsx", &components);

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["components"], 128, "{view}");
    assert_eq!(view["analysis"]["components_omitted"], 1, "{view}");
    assert!(view["items"].as_array().unwrap().iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["evidence"]["basis"] == "component-fact-limit"
            && fact["gap"]["components_omitted"] == 1
    }));
}

#[test]
fn framework_features_expand_layouts_relative_components_and_custom_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "package.json",
        r#"{"dependencies":{"next":"16","react":"19"}}"#,
    );
    put(
        root,
        "app/layout.tsx",
        concat!(
            "import Shell from './ui/Shell';\n",
            "export default function RootLayout({ children }: { children: React.ReactNode }) {\n",
            "  return <Shell>{children}</Shell>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "app/page.tsx",
        concat!(
            "import { Card as PetCard } from './ui/Card';\n",
            "import Local from './Local';\n",
            "import type TypeCard from './TypeCard';\n",
            "function usePets() { return 1; }\n",
            "function Badge() { return <span />; }\n",
            "export default function Home() {\n",
            "  usePets();\n",
            "  return <main><Badge /><PetCard /><Local /><TypeCard /></main>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "app/ui/Shell.tsx",
        "export default function Shell() { return <section />; }\n",
    );
    put(
        root,
        "app/ui/Card.tsx",
        concat!(
            "import Leaf from './Leaf';\n",
            "export function Card() { return <article><Leaf /></article>; }\n",
        ),
    );
    put(
        root,
        "app/ui/Leaf.tsx",
        concat!(
            "import { Card } from './Card';\n",
            "export default function Leaf() { return <small />; }\n",
        ),
    );
    put(
        root,
        "app/Local.jsx",
        concat!(
            "'use client';\n",
            "import Inner from './Inner';\n",
            "export default function Local() { return <aside><Inner /></aside>; }\n",
        ),
    );
    put(
        root,
        "app/Inner.tsx",
        concat!(
            "import { useState } from 'react';\n",
            "export default function Inner() {\n",
            "  const [value] = useState(0);\n",
            "  return <strong>{value}</strong>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "app/TypeCard.tsx",
        "export default function TypeCard() { return <strong />; }\n",
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["features"], 1, "{view}");
    assert_eq!(view["analysis"]["components"], 8, "{view}");
    assert_eq!(view["analysis"]["component_hooks"], 1, "{view}");
    let items = view["items"].as_array().unwrap();
    let feature = items.iter().find(|fact| fact["kind"] == "feature").unwrap();
    assert_eq!(feature["feature"]["component_file_count"], 7, "{view}");
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component"
            && fact["component"]["name"] == "RootLayout"
            && fact["component"]["file_role"] == "layout"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-hook"
            && fact["hook"]["name"] == "usePets"
            && fact["hook"]["kind"] == "custom-hook-candidate"
            && fact["status"] == "unresolved"
    }));
    let inner = items
        .iter()
        .find(|fact| fact["kind"] == "component" && fact["component"]["name"] == "Inner")
        .unwrap();
    assert_eq!(
        inner["component"]["rendering_boundary"],
        "client-transitive-candidate"
    );
    assert_eq!(inner["component"]["declared_boundary"], "server-default");
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-state"
            && fact["parent"] == inner["id"]
            && fact["status"] == "candidate"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-render"
            && fact["render"]["target"] == "Badge"
            && fact["render"]["resolution"] == "same-file"
            && fact["status"] == "resolved"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "component-render"
            && fact["render"]["target"] == "TypeCard"
            && fact["render"]["resolution"] == "unresolved-import"
            && fact["status"] == "unresolved"
    }));
    for (target, imported_as, path) in [
        ("Shell", "default", "app/ui/Shell.tsx"),
        ("PetCard", "Card", "app/ui/Card.tsx"),
        ("Local", "default", "app/Local.jsx"),
        ("Leaf", "default", "app/ui/Leaf.tsx"),
    ] {
        assert!(
            items.iter().any(|fact| {
                fact["kind"] == "component-render"
                    && fact["render"]["target"] == target
                    && fact["render"]["imported_as"] == imported_as
                    && fact["render"]["target_source"]["path"] == path
                    && fact["status"] == "resolved"
                    && fact["confidence"] == "import-qualified"
            }),
            "{target}: {view}"
        );
    }
}

#[test]
fn framework_component_imports_preserve_missing_ambiguous_and_package_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "web/package.json",
        r#"{"dependencies":{"next":"16"}}"#,
    );
    put(
        root,
        "web/app/page.tsx",
        concat!(
            "import Missing from './Missing';\n",
            "import Ambiguous from './Ambiguous';\n",
            "import Escaped from '../../outside/Escaped';\n",
            "export default function Home() {\n",
            "  return <main><Missing /><Ambiguous /><Escaped /></main>;\n",
            "}\n",
        ),
    );
    put(
        root,
        "web/app/Ambiguous.tsx",
        "export default function Ambiguous() { return <div />; }\n",
    );
    put(
        root,
        "web/app/Ambiguous.jsx",
        "export default function Ambiguous() { return <div />; }\n",
    );
    put(
        root,
        "outside/Escaped.tsx",
        "export default function Escaped() { return <div />; }\n",
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    for reason in [
        "A relative component import has no captured TSX or JSX target.",
        "A relative component import has multiple captured TSX or JSX targets.",
        "A relative component import crosses the captured package boundary.",
    ] {
        assert!(
            items
                .iter()
                .any(|fact| { fact["kind"] == "framework-gap" && fact["gap"]["reason"] == reason }),
            "{reason}: {view}"
        );
    }
    assert_eq!(view["analysis"]["components"], 1, "{view}");
    assert_eq!(view["analysis"]["component_gaps"], 3, "{view}");
    assert_eq!(
        items
            .iter()
            .filter(|fact| fact["kind"] == "component-render" && fact["status"] == "unresolved")
            .count(),
        3,
        "{view}"
    );
}

#[test]
fn framework_component_file_expansion_is_bounded_with_an_explicit_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    let imports: String = (0..65)
        .map(|number| format!("import Component{number} from './Component{number}';\n"))
        .collect();
    let renders: String = (0..65)
        .map(|number| format!("<Component{number} />"))
        .collect();
    put(
        root,
        "app/page.tsx",
        &format!("{imports}export default function Home() {{ return <main>{renders}</main>; }}\n"),
    );
    for number in 0..65 {
        put(
            root,
            &format!("app/Component{number}.tsx"),
            &format!("export default function Component{number}() {{ return <div />; }}\n"),
        );
    }

    let view = ok(root, &["project", "features", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let feature = items.iter().find(|fact| fact["kind"] == "feature").unwrap();
    assert_eq!(view["analysis"]["component_file_limit"], 64, "{view}");
    assert_eq!(feature["feature"]["component_file_count"], 64, "{view}");
    assert!(
        items.iter().any(|fact| {
            fact["kind"] == "framework-gap"
                && fact["gap"]["reason"]
                    == "The component file limit omitted a relative import target."
        }),
        "{view}"
    );
}

#[test]
fn framework_page_layout_and_import_diagnostics_are_bounded_and_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    for path in ["app/page.tsx", "app/page.jsx"] {
        put(
            root,
            path,
            "export default function Home() { return <main />; }\n",
        );
    }
    for path in ["app/layout.tsx", "app/layout.jsx"] {
        put(
            root,
            path,
            "export default function Layout() { return <section />; }\n",
        );
    }

    let view = ok(root, &["project", "features", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    for reason in [
        "Multiple page convention files map to the same feature path.",
        "Multiple layout convention files apply at one route level.",
    ] {
        assert!(
            items
                .iter()
                .any(|fact| { fact["kind"] == "framework-gap" && fact["gap"]["reason"] == reason }),
            "{reason}: {view}"
        );
    }
    assert_eq!(view["analysis"]["features"], 1, "{view}");
    assert_eq!(view["analysis"]["components"], 4, "{view}");
    assert_eq!(view["analysis"]["component_gaps"], 2, "{view}");

    let imports: String = (0..129)
        .map(|number| format!("import Missing{number} from './Missing{number}';\n"))
        .collect();
    let renders: String = (0..129)
        .map(|number| format!("<Missing{number} />"))
        .collect();
    put(
        root,
        "app/page.jsx",
        &format!("{imports}export default function Home() {{ return <main>{renders}</main>; }}\n"),
    );
    std::fs::remove_file(root.join("app/page.tsx")).unwrap();
    std::fs::remove_file(root.join("app/layout.tsx")).unwrap();
    std::fs::remove_file(root.join("app/layout.jsx")).unwrap();
    let bounded = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(bounded["analysis"]["component_file_gap_limit"], 128);
    assert!(
        bounded["items"].as_array().unwrap().iter().any(|fact| {
            fact["kind"] == "framework-gap"
                && fact["evidence"]["basis"] == "component-file-gap-limit"
                && fact["gap"]["omitted"] == 1
        }),
        "{bounded}"
    );
}

#[test]
fn framework_feature_package_facts_bound_build_settings_and_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let scripts: serde_json::Map<_, _> = (0..65)
        .map(|number| (format!("script-{number:03}"), serde_json::json!("run")))
        .collect();
    let dependencies: serde_json::Map<_, _> = (0..257)
        .map(|number| (format!("dependency-{number:03}"), serde_json::json!("1")))
        .collect();
    put(
        root,
        "package.json",
        &serde_json::json!({
            "dependencies": dependencies,
            "scripts": scripts,
        })
        .to_string(),
    );
    put(
        root,
        "app/api/route.ts",
        "export function GET(): Response { throw new Error('PRIVATE'); }\n",
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["build_settings"], 64);
    assert_eq!(view["analysis"]["build_settings_omitted"], 1);
    assert_eq!(view["analysis"]["dependencies"], 256);
    assert_eq!(view["analysis"]["dependencies_omitted"], 1);
    let items = view["items"].as_array().unwrap();
    assert_eq!(
        items
            .iter()
            .filter(|fact| fact["kind"] == "build-setting")
            .count(),
        64
    );
    assert_eq!(
        items
            .iter()
            .filter(|fact| fact["kind"] == "dependency")
            .count(),
        256
    );
    for omitted in ["build-setting limit", "dependency fact limit"] {
        assert!(items.iter().any(|fact| {
            fact["kind"] == "package-gap"
                && fact["gap"]["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains(omitted))
        }));
    }
    assert!(!view.to_string().contains("PRIVATE"));
}

#[test]
fn framework_feature_middleware_facts_are_bounded_with_an_explicit_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let registrations: String = (0..65)
        .map(|number| format!("app.add_middleware(Middleware{number})\n"))
        .collect();
    put(
        root,
        "app.py",
        &format!(
            "from fastapi import FastAPI\napp = FastAPI()\n{registrations}@app.get('/pets')\ndef pets(): pass\n"
        ),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["middleware"], 64, "{view}");
    assert_eq!(view["analysis"]["middleware_omitted"], 1, "{view}");
    assert_eq!(view["analysis"]["middleware_gaps"], 1, "{view}");
    assert!(view["items"].as_array().unwrap().iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["gap"]["omitted"] == 1
            && fact["evidence"]["basis"] == "middleware-fact-limit"
    }));
}

#[test]
fn framework_feature_configuration_facts_are_bounded_without_values() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    put(root, "app/api/route.ts", "export function GET() {}\n");
    let declarations: String = (0..129)
        .map(|number| format!("      SETTING_{number:03}: PRIVATE_VALUE_{number:03}\n"))
        .collect();
    put(
        root,
        "compose.yaml",
        &format!("services:\n  app:\n    environment:\n{declarations}"),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["runtime_configurations"], 128, "{view}");
    assert_eq!(view["analysis"]["configurations_omitted"], 1, "{view}");
    assert!(view["items"].as_array().unwrap().iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["evidence"]["basis"] == "runtime-configuration-limit"
            && fact["gap"]["configurations_omitted"] == 1
    }));
    assert!(!view.to_string().contains("PRIVATE_VALUE"));
}

#[test]
fn framework_feature_execution_dependencies_are_bounded_with_an_explicit_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let parameters = (0..257)
        .map(|number| format!("p{number}=Depends(d{number})"))
        .collect::<Vec<_>>()
        .join(", ");
    put(
        root,
        "app.py",
        &format!(
            "from fastapi import Depends, FastAPI\napp=FastAPI()\n@app.get('/pets')\ndef pets({parameters}): pass\n"
        ),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["execution_dependencies"], 256, "{view}");
    assert_eq!(
        view["analysis"]["execution_dependencies_omitted"], 1,
        "{view}"
    );
    assert!(view["items"].as_array().unwrap().iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["evidence"]["basis"] == "execution-dependency-fact-limit"
            && fact["gap"]["omitted"] == 1
    }));
}

#[test]
fn framework_feature_lifecycle_facts_are_bounded_with_an_explicit_gap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let hooks: String = (0..65)
        .map(|number| format!("@app.on_event('startup')\ndef startup_{number}(): pass\n"))
        .collect();
    put(
        root,
        "app.py",
        &format!(
            "from fastapi import FastAPI\napp=FastAPI()\n{hooks}@app.get('/pets')\ndef pets(): pass\n"
        ),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["lifecycle_hooks"], 64, "{view}");
    assert_eq!(view["analysis"]["lifecycle_omitted"], 1, "{view}");
    assert!(view["items"].as_array().unwrap().iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["evidence"]["basis"] == "lifecycle-fact-limit"
            && fact["gap"]["omitted"] == 1
    }));
}

#[test]
fn framework_features_preserve_schema_ambiguity_and_unsupported_framework_routes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "app/api/pets/route.ts",
        "interface Pet { id: string; }\ninterface Pet { name: string; }\nexport function GET(): Pet { throw new Error('PRIVATE'); }\n",
    );
    put(
        root,
        "legacy.ts",
        "app.get('/legacy', legacy);\nfunction legacy() { return 1; }\n",
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let reference = items
        .iter()
        .find(|fact| fact["kind"] == "schema-reference" && fact["reference"]["name"] == "Pet")
        .unwrap();
    assert_eq!(reference["status"], "ambiguous", "{view}");
    assert_eq!(reference["reference"]["candidate_count"], 2);
    assert_eq!(
        items
            .iter()
            .filter(|fact| fact["kind"] == "schema-candidate" && fact["parent"] == reference["id"])
            .count(),
        2
    );
    assert_eq!(view["analysis"]["unsupported_framework_routes"], 1);
    assert!(items.iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["gap"]["framework"] == "express"
            && fact["gap"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("does not model express"))
    }));
    assert!(items.iter().any(|fact| fact["kind"] == "schema-gap"));
    assert!(!view.to_string().contains("PRIVATE"));
}

#[test]
fn framework_features_preserve_dependency_and_next_middleware_ambiguity_as_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    put(root, "app/api/route.ts", "export function GET() {}\n");
    put(root, "proxy.js", "export function proxy() {}\n");
    put(root, "middleware.ts", "export function middleware() {}\n");
    put(
        root,
        "api.py",
        concat!(
            "from typing import Annotated\n",
            "from fastapi import Depends, FastAPI, Security\n",
            "app = FastAPI(dependencies=[Depends(global_dep), dynamic])\n",
            "@app.get('/pets', dependencies=ROUTE_DEPENDENCIES)\n",
            "def pets(value: Annotated[str, Depends(first), Security(second)], ",
            "dynamic=Depends(build_provider())):\n",
            "    pass\n",
        ),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["middleware"], 2, "{view}");
    assert_eq!(view["analysis"]["middleware_gaps"], 1, "{view}");
    assert_eq!(view["analysis"]["execution_dependencies"], 4, "{view}");
    assert_eq!(view["analysis"]["authentication_candidates"], 1, "{view}");
    let items = view["items"].as_array().unwrap();
    assert!(items.iter().any(|fact| {
        fact["kind"] == "middleware"
            && fact["middleware"]["form"] == "nextjs-middleware"
            && fact["middleware"]["deprecated_convention"] == true
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "framework-gap"
            && fact["gap"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("Multiple Next.js"))
    }));
    let dependencies: Vec<_> = items
        .iter()
        .filter(|fact| fact["kind"] == "execution-dependency")
        .collect();
    assert!(dependencies
        .iter()
        .filter(|fact| fact["execution_dependency"]["scope"] != "application")
        .all(|fact| fact["status"] == "unresolved"));
    assert!(dependencies.iter().any(|fact| {
        fact["execution_dependency"]["binding"] == "dynamic"
            && fact["execution_dependency"]["provider"].is_null()
    }));
    assert!(dependencies.iter().any(|fact| {
        fact["execution_dependency"]["provider"] == "global_dep"
            && fact["execution_dependency"]["scope"] == "application"
    }));
    for fragment in ["application dependency list", "route dependency list"] {
        assert!(items.iter().any(|fact| {
            fact["gaps"].as_array().is_some_and(|gaps| {
                gaps.iter()
                    .any(|gap| gap.as_str().is_some_and(|gap| gap.contains(fragment)))
            })
        }));
    }

    let contracts = ok(root, &["project", "contracts", "--limit", "500"]);
    assert_eq!(
        contracts["analysis"]["route_dependencies"], 3,
        "{contracts}"
    );
    assert_eq!(
        contracts["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|fact| fact["kind"] == "route-dependency")
            .count(),
        3
    );
    assert!(!view.to_string().contains("build_provider"));
}

#[test]
fn framework_features_keep_lifecycle_conflicts_and_cross_file_hooks_as_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "package.json", r#"{"dependencies":{"next":"16"}}"#);
    put(root, "app/api/route.ts", "export function GET() {}\n");
    put(
        root,
        "instrumentation.ts",
        concat!(
            "const boot = () => {};\n",
            "export { boot as register };\n",
            "export { missing as onRequestError };\n",
            "export { onRequestError } from './errors';\n",
        ),
    );
    put(
        root,
        "api.py",
        concat!(
            "from fastapi import FastAPI\n",
            "def life(app): yield\n",
            "def start(): pass\n",
            "app = FastAPI(lifespan=life, on_startup=[start, build_hook()])\n",
            "@app.on_event(EVENT)\n",
            "def dynamic_event(): pass\n",
            "@app.get('/pets')\n",
            "def pets(): pass\n",
        ),
    );

    let view = ok(root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["lifecycle_hooks"], 4, "{view}");
    assert_eq!(view["analysis"]["lifecycle_gaps"], 4, "{view}");
    let items = view["items"].as_array().unwrap();
    assert!(items.iter().any(|fact| {
        fact["kind"] == "lifecycle-hook"
            && fact["lifecycle"]["name"] == "boot"
            && fact["lifecycle"]["exported_as"] == "register"
    }));
    assert!(items.iter().any(|fact| {
        fact["kind"] == "lifecycle-hook"
            && fact["lifecycle"]["form"] == "fastapi-constructor-event"
            && fact["lifecycle"]["name"].is_null()
            && fact["status"] == "ambiguous"
    }));
    for fragment in [
        "Re-exported",
        "no unique direct callable",
        "event phase",
        "lifespan and deprecated",
    ] {
        assert!(items.iter().any(|fact| {
            fact["kind"] == "framework-gap"
                && fact["gap"]["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains(fragment))
        }));
    }
    assert!(!view.to_string().contains("build_hook"));
}

#[test]
fn next_contract_fields_clip_paths_and_names_without_losing_handles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(75);
    let path = format!(
        "app/{}/[{name}]/route.ts",
        vec!["s".repeat(100); 6].join("/")
    );
    put(
        root,
        &path,
        "export function GET(): Response { throw 'PRIVATE'; }\n",
    );
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let route = items.iter().find(|r| r["kind"] == "route").unwrap();
    assert_eq!(route["url"]["text"].as_str().unwrap().len(), 512);
    assert!(route["url"]["omitted_bytes"].as_u64().unwrap() > 0);
    let field = items.iter().find(|r| r["direction"] == "request").unwrap();
    assert_eq!(field["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(field["name"]["omitted_bytes"], name.len() - 159);
    assert_eq!(
        ok(
            root,
            &["project", "show", route["file_handle"].as_str().unwrap()]
        )["node"]["kind"],
        "file"
    );
    assert!(!view.to_string().contains("PRIVATE"));
}

#[test]
fn next_route_analysis_uses_captured_exports_after_source_removal() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(
        &root,
        "app/api/[petId]/route.ts",
        "export function GET(): Before { throw 'PRIVATE'; }\n",
    );
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_dir_all(root.join("app")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts"]);
    let view = project.report(&query.command).unwrap();
    let items = view["items"].as_array().unwrap();
    assert!(items.iter().any(|r| r["url"] == "/api/{petId}"));
    assert!(items.iter().any(|r| r["declared_type"] == "Before"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn fastapi_routes_use_observed_constructor_aliases_and_declaration_positions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "from fastapi import FastAPI as API, APIRouter as Router\nservice = API()\nroutes = Router(prefix='/v1')\n\n@service.get('/pets')\n@routes.post(path='/pets')\ndef pets():\n    return 'PRIVATE_ONE'\n\n@service.delete('/pets/{petId}')\ndef pets():\n    return 'PRIVATE_TWO'\n\nclass Other:\n    def pets(self):\n        return 'PRIVATE_OTHER'\n\n@bp.get('/legacy')\ndef legacy():\n    pass\n");
    put(
        root,
        "other.py",
        "import fastapi as fa\napi = fa.FastAPI()\n@api.patch('/other')\ndef other():\n    pass\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let fast: Vec<_> = items
        .iter()
        .filter(|r| r["framework_candidate"] == "fastapi")
        .collect();
    assert_eq!(fast.len(), 4, "{view}");
    assert_eq!(view["analysis"]["declarations"], 5);
    for route in fast {
        assert_eq!(route["basis"], "fastapi-import-constructor-decorator");
        assert_eq!(route["handler"]["candidate_count"], 1);
        assert_eq!(route["handler"]["basis"], "declaration-span");
        let handler = items
            .iter()
            .find(|r| r["kind"] == "route-handler" && r["route"] == route["id"])
            .unwrap();
        assert!(handler["confidence"].is_null());
        if route["method"] == "DELETE" {
            assert_eq!(handler["handler"]["line"], 11);
        }
        assert_eq!(
            ok(
                root,
                &[
                    "project",
                    "show",
                    handler["handler"]["handle"].as_str().unwrap()
                ]
            )["node"]["name"],
            route["handler"]["name"]
        );
    }
    assert!(items
        .iter()
        .any(|r| r["framework_candidate"] == "flask" && r["url"] == "/legacy"));
    assert!(items.iter().any(|r| {
        r["framework_candidate"] == "fastapi" && r["method"] == "POST" && r["url"] == "/v1/pets"
    }));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn fastapi_router_prefixes_accept_runtime_shapes_and_reject_unresolved_forms() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "app.py",
        "from fastapi import APIRouter, FastAPI\nplain = APIRouter(prefix='/items')\nempty = APIRouter(prefix='')\ndynamic = APIRouter(prefix=PREFIX)\ntrailing = APIRouter(prefix='/bad/')\napp = FastAPI(prefix='/not-a-router-prefix')\n\n@plain.get('/')\ndef list_items(): pass\n@empty.get('/health')\ndef health(): pass\n@dynamic.get('/hidden')\ndef hidden(): pass\n@trailing.get('/invalid')\ndef invalid(): pass\n@app.get('/app')\ndef app_route(): pass\n",
    );
    let view = ok(root, &["project", "routes", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let routes: Vec<_> = items
        .iter()
        .filter(|row| row["framework_candidate"] == "fastapi")
        .map(|row| (row["method"].clone(), row["url"].clone()))
        .collect();
    assert_eq!(
        routes,
        vec![
            (serde_json::json!("GET"), serde_json::json!("/health")),
            (serde_json::json!("GET"), serde_json::json!("/items/"))
        ]
    );
    assert_eq!(view["analysis"]["fastapi_gaps"], 6);
    assert_eq!(
        items
            .iter()
            .filter(|row| row["basis"] == "fastapi-reader")
            .count(),
        6
    );
}

#[test]
fn pinned_fastapi_project_matches_an_independent_route_contract() {
    use sha2::{Digest, Sha256};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/framework-corpus/fastapi");
    let source = fs::read(root.join("items.py")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source)),
        "7f0fa55d1f7b02188c4abd4f88aa6db92fe03de38258b765ed050ec72032b410"
    );
    let view = ok(&root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["applications"], 1);
    assert_eq!(view["analysis"]["features"], 2);
    assert_eq!(view["analysis"]["routes"], 5);
    assert_eq!(view["analysis"]["handlers"], 5);
    let mut actual: Vec<_> = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "route")
        .map(|row| {
            (
                row["route"]["method"].as_str().unwrap().to_owned(),
                row["route"]["url"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    actual.sort();
    assert_eq!(
        actual,
        [
            ("DELETE", "/items/{id}"),
            ("GET", "/items/"),
            ("GET", "/items/{id}"),
            ("POST", "/items/"),
            ("PUT", "/items/{id}"),
        ]
        .map(|(method, url)| (method.to_owned(), url.to_owned()))
    );
    let application = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "application")
        .unwrap();
    assert_eq!(application["application"]["framework"], "fastapi");
    assert!(application["gaps"].as_array().unwrap().iter().any(|gap| gap
        .as_str()
        .unwrap()
        .contains("Include-router and mounted prefixes")));
}

#[test]
fn pinned_nextjs_project_matches_an_independent_route_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/nextjs");
    let view = ok(&root, &["project", "features", "--limit", "500"]);
    assert_eq!(view["analysis"]["applications"], 1);
    assert_eq!(view["analysis"]["features"], 3);
    assert_eq!(view["analysis"]["routes"], 5);
    assert_eq!(view["analysis"]["handlers"], 5);
    let mut actual: Vec<_> = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "route")
        .map(|row| {
            (
                row["route"]["method"].as_str().unwrap().to_owned(),
                row["route"]["url"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    actual.sort();
    assert_eq!(
        actual,
        [
            ("DELETE", "/api/posts/{postId}"),
            ("GET", "/api/posts"),
            ("PATCH", "/api/posts/{postId}"),
            ("POST", "/api/posts"),
            ("POST", "/api/webhooks/stripe"),
        ]
        .map(|(method, url)| (method.to_owned(), url.to_owned()))
    );
    let application = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "application")
        .unwrap();
    assert_eq!(application["application"]["framework"], "nextjs-app");
    assert!(application["gaps"].as_array().unwrap().iter().any(|gap| gap
        .as_str()
        .unwrap()
        .contains("Runtime Next.js configuration")));
}

#[test]
fn fastapi_contracts_read_default_markers_without_leaking_defaults_or_constraints() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "from fastapi import FastAPI, Path, Query, Body, Header, Cookie, Form, File, Depends\napp=FastAPI()\n\n@app.post('/pets/{id}')\ndef create(id: int=Path(...), q: str=Query('PRIVATE_DEFAULT',alias='search',description='PRIVATE_DESCRIPTION'), body: Create=Body(...), x_trace: str=Header(...), token: str=Cookie(alias='session'), form: FormData=Form(...), upload=File(...), expanded: str=Query(**options), implicit: str='', user=Depends(load_user)) -> Pet:\n    return 'PRIVATE_BODY'\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let inputs: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "fastapi-explicit-binding" && r["kind"] == "route-contract-field")
        .collect();
    assert_eq!(inputs.len(), 8, "{view}");
    for (binding, location, name, ty) in [
        ("id", "path", Some("id"), Some("int")),
        ("q", "query", Some("search"), Some("str")),
        ("body", "body", None, Some("Create")),
        ("x_trace", "header", None, Some("str")),
        ("token", "cookie", Some("session"), Some("str")),
        ("form", "body", None, Some("FormData")),
        ("upload", "body", None, None),
        ("expanded", "query", None, Some("str")),
    ] {
        let field = inputs.iter().find(|r| r["binding"] == binding).unwrap();
        assert_eq!(field["location"], location);
        assert_eq!(field["name"].as_str(), name);
        assert_eq!(field["declared_type"].as_str(), ty);
        assert!(field["required"].is_null());
        assert_eq!(field["confidence"], "name-only");
    }
    assert!(items
        .iter()
        .any(|r| r["parameters"] == 1 && r["basis"] == "fastapi-explicit-binding"));
    assert!(items.iter().any(|r| {
        r["kind"] == "route-dependency"
            && r["binding"] == "user"
            && r["provider"] == "load_user"
            && r["authentication_candidate"] == false
    }));
    assert!(items
        .iter()
        .any(|r| r["declared_type"] == "Pet" && r["location"] == "return"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn fastapi_annotated_bindings_strip_metadata_and_keep_unknown_types_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "import fastapi\nfrom typing import Annotated\napi = fastapi.FastAPI()\n\n@api.get('/pets/{id}')\ndef get(id: Annotated[int, fastapi.Path(ge=1)], q: Annotated[str | None, fastapi.Query(alias='search', description='PRIVATE_METADATA')] = 'PRIVATE_DEFAULT', tags: Annotated[list[str], fastapi.Query()] = [], conflict: Annotated[str, fastapi.Query(), fastapi.Header()] = '', dep: Annotated[User, Depends(load_user)] = None, dynamic: Strange['PRIVATE_TYPE'] = fastapi.Body(...)):\n    return 'PRIVATE_BODY'\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let inputs: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "fastapi-explicit-binding" && r["kind"] == "route-contract-field")
        .collect();
    assert_eq!(inputs.len(), 4, "{view}");
    assert!(inputs
        .iter()
        .any(|r| r["binding"] == "id" && r["declared_type"] == "int"));
    assert!(inputs.iter().any(|r| r["binding"] == "q"
        && r["declared_type"] == "str | None"
        && r["name"] == "search"));
    assert!(inputs
        .iter()
        .any(|r| r["binding"] == "tags" && r["declared_type"] == "list[str]"));
    assert!(inputs
        .iter()
        .any(|r| r["binding"] == "dynamic" && r["declared_type"].is_null()));
    assert!(items.iter().any(|r| r["parameters"] == 1));
    assert!(items.iter().any(|r| {
        r["kind"] == "route-dependency" && r["binding"] == "dep" && r["provider"] == "load_user"
    }));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn fastapi_response_models_remain_separate_from_return_annotations_and_disabled_models() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get('/pets', response_model=list[Pet], status_code=201)\ndef pets() -> Any:\n    return 'PRIVATE_BODY'\n\n@app.get('/raw', response_model=None)\ndef raw() -> Response:\n    return 'PRIVATE_RAW'\n\n@app.get('/declared', response_model=Pet | None)\n@app.post('/declared', response_model=Create)\ndef declared():\n    pass\n\n@app.get('/dynamic', response_model=build_model('PRIVATE_MODEL'))\ndef dynamic():\n    pass\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let models: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-contract-field" && r["basis"] == "fastapi-response-model")
        .collect();
    assert_eq!(models.len(), 4, "{view}");
    assert!(models
        .iter()
        .any(|r| r["declared_type"] == "list[Pet]" && r["model_state"] == "declared"));
    assert!(models
        .iter()
        .any(|r| r["declared_type"] == "None" && r["model_state"] == "disabled"));
    assert!(models.iter().any(|r| r["declared_type"] == "Pet | None"));
    let created = items
        .iter()
        .find(|r| r["url"] == "/declared" && r["method"] == "POST")
        .unwrap();
    assert!(models
        .iter()
        .any(|r| r["route"] == created["id"] && r["declared_type"] == "Create"));
    assert!(!models
        .iter()
        .any(|r| r["route"] == created["id"] && r["declared_type"] == "Pet | None"));
    for (url, ty) in [("/pets", "Any"), ("/raw", "Response")] {
        let route = items.iter().find(|r| r["url"] == url).unwrap();
        assert!(items.iter().any(|r| r["route"] == route["id"]
            && r["location"] == "return"
            && r["declared_type"] == ty));
    }
    let declared = items.iter().find(|r| r["url"] == "/declared").unwrap();
    assert!(!items
        .iter()
        .any(|r| r["route"] == declared["id"] && r["kind"] == "route-contract-gap"));
    assert!(items
        .iter()
        .any(|r| r["basis"] == "fastapi-response-model" && r["kind"] == "route-contract-gap"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn fastapi_gaps_replace_legacy_guesses_for_unsupported_decorators() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.py", "from fastapi import FastAPI\napp = FastAPI()\n\n@app.get(PATH)\ndef dynamic():\n    pass\n\n@app.get('/prefix' + suffix)\ndef joined():\n    pass\n\n@app.api_route('/many', methods=['GET', 'POST'])\ndef many():\n    pass\n\n@app.get('/literal', path='/other')\ndef conflicting():\n    pass\n\n@app.get('/ok', **options)\ndef ok():\n    pass\n");
    put(root, "unknown.py", "from fastapi import FastAPI\nrouter = FastAPI()\nrouter = Other()\n@router.get('/rebound')\ndef rebound():\n    pass\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    assert_eq!(view["analysis"]["fastapi_gaps"], 4);
    let items = view["items"].as_array().unwrap();
    let routes: Vec<_> = items.iter().filter(|r| r["kind"] == "route").collect();
    assert_eq!(routes.len(), 1, "{view}");
    assert_eq!(routes[0]["url"], "/ok");
    assert!(items.iter().any(|r| r["reason"]
        .as_str()
        .is_some_and(|s| s.starts_with("Expanded decorator options"))));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "analysis-gap" && r["basis"] == "fastapi-reader"));
}

#[test]
fn fastapi_contract_pages_preserve_all_fields_and_bind_scope_query_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "src/app.py", "from fastapi import FastAPI, Query\napp = FastAPI()\n@app.get('/pets', response_model=Pet)\ndef pets(q: str = Query(alias='search')) -> Any:\n    pass\n");
    let full = ok(root, &["project", "contracts", "--limit", "500"]);
    let first = ok(root, &["project", "contracts", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut combined = first["items"].as_array().unwrap().clone();
    let mut page = first.clone();
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &["project", "contracts", "--cursor", next, "--limit", "2"],
        );
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    assert_eq!(combined.len(), 6);
    assert!(!run(root, &["project", "routes", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "contracts", "src", "--cursor", cursor]).0);
    let file = relation_handle(root, "app.py", None);
    let by_file = ok(root, &["project", "contracts", &file]);
    assert_eq!(
        by_file,
        ok(
            root,
            &[
                "project",
                "contracts",
                file.rsplit(':').next().unwrap(),
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    let routes = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(
        combined
            .into_iter()
            .filter(|r| r["kind"] == "route" || r["kind"] == "route-handler")
            .collect::<Vec<_>>(),
        *routes["items"].as_array().unwrap()
    );
    put(root, "src/new.py", "x = 1\n");
    assert!(!run(root, &["project", "contracts", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "contracts", &file]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn fastapi_contract_fields_bound_names_and_types_and_keep_dynamic_aliases_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(80);
    let ty = "Type".repeat(160);
    put(root, "app.py", &format!("from fastapi import FastAPI, Query\napp = FastAPI()\n@app.get('/pets', response_model={ty})\ndef pets(q: {ty} = Query(alias='{name}'), dynamic: str = Query(alias=NAME), escaped: str = Query(alias='a\\nb')):\n    pass\n"));
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let q = items.iter().find(|r| r["binding"] == "q").unwrap();
    assert_eq!(q["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(q["name"]["omitted_bytes"], name.len() - 159);
    assert_eq!(q["declared_type"]["text"].as_str().unwrap().len(), 512);
    assert_eq!(q["declared_type"]["omitted_bytes"], ty.len() - 512);
    assert!(items
        .iter()
        .any(|r| r["binding"] == "dynamic" && r["name"].is_null()));
    assert!(items
        .iter()
        .any(|r| r["binding"] == "escaped" && r["name"].is_null()));
}

#[test]
fn fastapi_contracts_use_captured_imports_decorators_and_markers_after_source_removal() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "app.py", "from fastapi import FastAPI, Query\napp = FastAPI()\n@app.get('/before', response_model=Before)\ndef get(q: str = Query(alias='before')):\n    pass\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join("app.py")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts"]);
    let view = project.report(&query.command).unwrap();
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["url"] == "/before" && r["framework_candidate"] == "fastapi"));
    assert!(items
        .iter()
        .any(|r| r["declared_type"] == "Before" && r["basis"] == "fastapi-response-model"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "before" && r["binding"] == "q"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn schemas_python_fields_omit_defaults_and_metadata_and_preserve_type_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.py", "from typing import Annotated\n\nclass Other:\n    code: str\n\nclass Pet(Base):\n    id: int = 42\n    other: list[Other | None] = Field(default_factory=PRIVATE_FACTORY)\n    label: Annotated[str, Field(description='PRIVATE_METADATA')]\n    model_config = {'PRIVATE_CONFIG': True}\n    def validate(self):\n        return 'PRIVATE_BODY'\n");
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let pet = items
        .iter()
        .find(|r| r["kind"] == "schema" && r["declaration"]["name"] == "Pet")
        .unwrap();
    assert_eq!(pet["field_count"], 3, "{view}");
    assert_eq!(pet["gap_count"], 4);
    assert_eq!(pet["completeness"], "partial");
    assert!(items
        .iter()
        .any(|r| r["name"] == "other" && r["declared_type"] == "list[Other | None]"));
    assert!(items
        .iter()
        .any(|r| r["name"] == "label" && r["declared_type"] == "str"));
    assert!(items.iter().any(|r| r["kind"] == "schema-type-reference"
        && r["name"] == "Other"
        && r["candidate_count"] == 1));
    for field in items.iter().filter(|r| r["kind"] == "schema-field") {
        assert!(field["required"].is_null());
        assert!(field["optional_marker"].is_null());
        assert!(field["readonly_marker"].is_null());
    }
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn schemas_typescript_interfaces_object_aliases_and_declared_markers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.ts", "export interface Pet<T> extends Base {\n readonly id?: string;\n tags: Array<Other | null>;\n item: ns.Item;\n tuple: [string, Other];\n [key: string]: Other;\n method(): void;\n}\n\ntype Other = { value: Pet[] };\n\ntype ID = string;\n");
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 2, "{view}");
    assert_eq!(view["analysis"]["fields"], 5);
    let id = items
        .iter()
        .find(|r| r["kind"] == "schema-field" && r["name"] == "id")
        .unwrap();
    assert_eq!(id["optional_marker"], true);
    assert_eq!(id["readonly_marker"], true);
    assert!(id["required"].is_null());
    for (name, spelling) in [
        ("tags", "Array<Other | null>"),
        ("item", "ns.Item"),
        ("tuple", "[string, Other]"),
        ("value", "Pet[]"),
    ] {
        let field = items
            .iter()
            .find(|r| r["kind"] == "schema-field" && r["name"] == name)
            .unwrap();
        assert_eq!(field["declared_type"], spelling);
        assert_eq!(field["optional_marker"], false);
    }
    assert!(items.iter().any(|r| r["kind"] == "schema-type-reference"
        && r["name"] == "ns.Item"
        && r["status"] == "unresolved"));
    assert!(items.iter().any(|r| r["reason"]
        .as_str()
        .is_some_and(|s| s.contains("object type aliases"))));
}

#[test]
fn schemas_preserve_ambiguous_definitions_and_cycles_without_cross_file_guesses() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.ts", "interface Duplicate { a: string; }\n\ninterface Duplicate { b: number; }\n\ninterface Cycle { self: Cycle; value: Duplicate; external: External; }\n");
    put(root, "other.ts", "interface External { hidden: string; }\n");
    let full = ok(root, &["project", "schemas", "models.ts", "--limit", "500"]);
    let items = full["items"].as_array().unwrap();
    assert_eq!(full["analysis"]["declarations"], 3);
    let reference = items
        .iter()
        .find(|r| r["kind"] == "schema-type-reference" && r["name"] == "Duplicate")
        .unwrap();
    assert_eq!(reference["status"], "ambiguous");
    assert_eq!(reference["candidate_count"], 2);
    let candidates: Vec<_> = items
        .iter()
        .filter(|r| r["reference"] == reference["id"])
        .collect();
    assert_eq!(candidates.len(), 2);
    assert_ne!(
        candidates[0]["target"]["handle"],
        candidates[1]["target"]["handle"]
    );
    assert!(items.iter().any(|r| r["kind"] == "schema-type-reference"
        && r["name"] == "External"
        && r["candidate_count"] == 0));
    let cycle = items
        .iter()
        .find(|r| r["kind"] == "schema" && r["declaration"]["name"] == "Cycle")
        .unwrap();
    let scoped = ok(
        root,
        &[
            "project",
            "schemas",
            cycle["declaration"]["handle"].as_str().unwrap(),
            "--limit",
            "500",
        ],
    );
    assert_eq!(scoped["analysis"]["declarations"], 1);
    assert_eq!(scoped["analysis"]["fields"], 3);
    assert!(scoped["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "schema-type-candidate" && r["target"]["name"] == "Duplicate"));
}

#[test]
fn schemas_report_unsupported_fields_without_partial_type_or_value_leaks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.py", "@decorate(PRIVATE_DECORATOR)\nclass Pet:\n    forward: 'PRIVATE_FORWARD'\n    computed: make_type('PRIVATE_COMPUTED')\n    nested: list[Annotated[str, PRIVATE_NESTED]]\n    same: int\n    same: str\n    if True:\n        hidden: str = 'PRIVATE_CONDITIONAL'\n");
    put(root, "models.ts", "interface Shape {\n literal: 'PRIVATE_LITERAL';\n object: { hidden: string };\n callback: () => Private;\n comment: Array</* PRIVATE_COMMENT */ string>;\n ['PRIVATE_KEY']: string;\n}\n");
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    assert!(!view.to_string().contains("PRIVATE_"), "{view}");
    assert!(!view.to_string().contains("Private"));
    let items = view["items"].as_array().unwrap();
    for name in [
        "forward", "computed", "nested", "literal", "object", "callback", "comment",
    ] {
        let field = items
            .iter()
            .find(|r| r["kind"] == "schema-field" && r["name"] == name)
            .unwrap();
        assert!(field["declared_type"].is_null());
        assert!(!items
            .iter()
            .any(|r| r["kind"] == "schema-type-reference" && r["field"] == field["id"]));
    }
    assert_eq!(
        items
            .iter()
            .filter(|r| r["kind"] == "schema-field" && r["name"] == "same")
            .count(),
        2
    );
    assert!(items.iter().any(|r| r["reason"]
        .as_str()
        .is_some_and(|s| s.contains("Duplicate field"))));
}

#[test]
fn schemas_pages_bind_query_scope_revision_and_support_followable_handles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "models.py",
        "class Pet:\n    value: Pet\n    id: int\n",
    );
    let full = ok(root, &["project", "schemas", "--limit", "500"]);
    let first = ok(root, &["project", "schemas", "--limit", "2"]);
    let first_cursor = first["page"]["next"].as_str().unwrap().to_owned();
    let mut page = first;
    let mut combined = Vec::new();
    loop {
        assert!(page["items"].as_array().unwrap().len() <= 2);
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
        let Some(cursor) = page["page"]["next"].as_str() else {
            break;
        };
        page = ok(
            root,
            &["project", "schemas", "--limit", "2", "--cursor", cursor],
        );
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    assert!(!run(root, &["project", "contracts", "--cursor", &first_cursor]).0);
    assert!(
        !run(
            root,
            &["project", "schemas", "models.py", "--cursor", &first_cursor]
        )
        .0
    );
    for limit in ["0", "501"] {
        assert!(!run(root, &["project", "schemas", "--limit", limit]).0);
    }
    let schema = combined.iter().find(|r| r["kind"] == "schema").unwrap();
    let handle = schema["declaration"]["handle"].as_str().unwrap();
    ok(root, &["project", "show", handle]);
    let scoped = ok(root, &["project", "schemas", handle]);
    assert_eq!(
        scoped,
        ok(
            root,
            &[
                "project",
                "schemas",
                handle.rsplit(':').next().unwrap(),
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    assert!(
        !run(
            root,
            &["project", "schemas", handle.rsplit(':').next().unwrap()]
        )
        .0
    );
    put(root, "models.py", "class Pet:\n    changed: str\n");
    assert!(!run(root, &["project", "schemas", "--cursor", &first_cursor]).0);
    assert!(!run(root, &["project", "schemas", handle]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn schemas_bound_utf8_fields_and_resolve_full_type_names_before_clipping() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(80);
    let ty = "Type".repeat(160);
    put(
        root,
        "models.py",
        &format!("class {ty}:\n    pass\n\nclass Pet:\n    {name}: {ty}\n"),
    );
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let field = items.iter().find(|r| r["kind"] == "schema-field").unwrap();
    assert_eq!(field["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(field["name"]["omitted_bytes"], name.len() - 159);
    assert_eq!(field["declared_type"]["text"].as_str().unwrap().len(), 512);
    assert_eq!(field["declared_type"]["omitted_bytes"], ty.len() - 512);
    let reference = items
        .iter()
        .find(|r| r["kind"] == "schema-type-reference")
        .unwrap();
    assert_eq!(reference["candidate_count"], 1);
    assert_eq!(reference["name"]["omitted_bytes"], ty.len() - 160);
}

#[test]
fn schemas_bound_type_depth_and_deduplicate_names_without_partial_references() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let deep = format!("{}A{}", "Array<".repeat(20), ">".repeat(20));
    put(root, "models.tsx", &format!("interface A {{ id: string; }}\n\ntype Holder = {{ deep: {deep}; repeated: A | A; }};\n"));
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let deep = items
        .iter()
        .find(|r| r["kind"] == "schema-field" && r["name"] == "deep")
        .unwrap();
    assert!(deep["declared_type"].is_null());
    assert!(!items
        .iter()
        .any(|r| r["kind"] == "schema-type-reference" && r["field"] == deep["id"]));
    let references: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "schema-type-reference")
        .collect();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0]["name"], "A");
    assert_eq!(references[0]["candidate_count"], 1);
}

#[test]
fn schemas_report_syntax_and_language_gaps_without_claiming_empty_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "bad.py", "class Pet:\n    field: [\n");
    put(
        root,
        "other.go",
        "package models\ntype Other struct { ID uint64 }\n",
    );
    put(root, "empty.ts", "const value = 1;\n");
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["kind"] == "analysis-gap" && r["path"] == "bad.py"));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "go"));
    assert_eq!(view["analysis"]["fields"], 0);
    let empty = ok(root, &["project", "schemas", "empty.ts"]);
    assert!(empty["items"].as_array().unwrap().is_empty());
    assert!(!empty["analysis"]["limitations"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn schemas_read_captured_declarations_after_source_removal_and_verify_drift() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "models.py", "class Pet:\n    before: Pet\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join("models.py")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "schemas"]);
    let view = project.report(&query.command).unwrap();
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "schema-field" && r["name"] == "before"));
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "schema-type-reference" && r["candidate_count"] == 1));
    assert!(project.verify(&root).is_err());
}

#[test]
fn schemas_rust_structs_keep_declared_fields_without_attribute_values_or_wire_guesses() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.rs", "struct Child { id: u64 }\n\n#[derive(Serialize)]\n#[serde(rename = \"PRIVATE_MODEL\")]\nstruct Pet<'a, T> {\n #[serde(rename = \"PRIVATE_FIELD\")]\n pub child: Option<Box<Child>>,\n borrowed: &'a mut Child,\n slice: &'a [Child],\n qualified: crate::Child,\n raw: [u8; PRIVATE_COUNT],\n}\n\nstruct Unit;\n\nstruct Tuple(Child);\n\ntype Alias = Child;\n\nimpl Child { fn private() { panic!(\"PRIVATE_BODY\"); } }\n");
    let view = ok(root, &["project", "schemas", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["declarations"], 3, "{view}");
    assert_eq!(view["analysis"]["fields"], 6);
    for (name, ty) in [
        ("child", "Option<Box<Child>>"),
        ("borrowed", "&'a mut Child"),
        ("slice", "&'a [Child]"),
        ("qualified", "crate::Child"),
    ] {
        let field = items
            .iter()
            .find(|r| r["kind"] == "schema-field" && r["name"] == name)
            .unwrap();
        assert_eq!(field["declared_type"], ty, "{view}");
        assert!(field["required"].is_null());
        assert!(field["optional_marker"].is_null());
        assert!(field["readonly_marker"].is_null());
    }
    assert!(items
        .iter()
        .any(|r| r["name"] == "raw" && r["declared_type"].is_null()));
    assert!(items
        .iter()
        .any(|r| r["name"] == "crate::Child" && r["status"] == "unresolved"));
    assert!(items.iter().any(|r| r["kind"] == "schema"
        && r["declaration"]["name"] == "Unit"
        && r["field_count"] == 0));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn schemas_rust_references_preserve_duplicates_cycles_and_file_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "models.rs", "mod a { pub struct Pet { id: u64 } }\n\nmod b { pub struct Pet { name: String } }\n\nstruct Body { item: Pet, next: Box<Body>, external: External }\n");
    put(root, "other.rs", "struct External;\n");
    let view = ok(root, &["project", "schemas", "models.rs", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let reference = items
        .iter()
        .find(|r| r["kind"] == "schema-type-reference" && r["name"] == "Pet")
        .unwrap();
    assert_eq!(reference["candidate_count"], 2);
    assert_eq!(reference["status"], "ambiguous");
    assert_eq!(
        items
            .iter()
            .filter(|r| r["reference"] == reference["id"])
            .count(),
        2
    );
    assert!(items.iter().any(|r| r["kind"] == "schema-type-reference"
        && r["name"] == "External"
        && r["candidate_count"] == 0));
    let body = items
        .iter()
        .find(|r| r["kind"] == "schema" && r["declaration"]["name"] == "Body")
        .unwrap();
    let scoped = ok(
        root,
        &[
            "project",
            "schemas",
            body["declaration"]["handle"].as_str().unwrap(),
            "--limit",
            "500",
        ],
    );
    assert_eq!(scoped["analysis"]["declarations"], 1);
    assert_eq!(scoped["analysis"]["fields"], 3);
}

#[test]
fn contract_types_link_axum_requests_and_returns_to_followable_rust_structs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "struct Create { name: String }\n\nstruct Pet { id: u64 }\n\nasync fn create(Json(body): Json<Create>) -> Json<Pet> { todo!(\"PRIVATE_BODY\") }\n\nfn router() { Router::new().route(\"/pets\", post(create)); }\n");
    let plain = ok(root, &["project", "contracts", "--limit", "500"]);
    assert!(!plain["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "route-contract-type-reference"));
    let view = ok(root, &["project", "contracts", "--types", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    assert_eq!(view["analysis"]["type_references"], 4, "{view}");
    assert_eq!(view["analysis"]["type_candidates"], 2);
    for (name, direction) in [("Create", "request"), ("Pet", "response")] {
        let reference = items
            .iter()
            .find(|r| r["kind"] == "route-contract-type-reference" && r["name"] == name)
            .unwrap();
        let field = items
            .iter()
            .find(|r| r["kind"] == "route-contract-field" && r["id"] == reference["field"])
            .unwrap();
        assert_eq!(field["direction"], direction);
        assert_eq!(field["type_reference_count"], 2);
        let target = items
            .iter()
            .find(|r| r["reference"] == reference["id"])
            .unwrap();
        assert_eq!(target["confidence"], "name-only");
        let handle = target["target"]["handle"].as_str().unwrap();
        let schema = ok(root, &["project", "schemas", handle]);
        assert_eq!(schema["analysis"]["declarations"], 1);
        assert_eq!(schema["analysis"]["fields"], 1);
        ok(root, &["project", "show", handle]);
    }
    assert!(!view.to_string().contains("PRIVATE_BODY"));
}

#[test]
fn contract_types_preserve_ambiguous_types_and_skip_fastapi_model_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app/route.ts", "interface Pet { id: string; }\n\ninterface Pet { name: string; }\n\nexport const GET = (): Promise<Pet | Pet> => make();\n");
    put(root, "api.py", "from fastapi import FastAPI\n\nclass Pet:\n    id: int\n\napp = FastAPI()\n\n@app.get('/pets', response_model=Other)\ndef pets() -> list[Pet]:\n    pass\n");
    let view = ok(root, &["project", "contracts", "--types", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let pets: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route-contract-type-reference" && r["name"] == "Pet")
        .collect();
    assert_eq!(pets.len(), 2);
    assert!(pets
        .iter()
        .any(|r| r["candidate_count"] == 2 && r["status"] == "ambiguous"));
    assert!(pets.iter().any(|r| r["candidate_count"] == 1));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "route-contract-type-reference"
            && r["name"] == "Promise"
            && r["candidate_count"] == 0));
    assert!(!items
        .iter()
        .any(|r| r["kind"] == "route-contract-type-reference" && r["name"] == "Other"));
    assert!(items
        .iter()
        .any(|r| r["basis"] == "fastapi-response-model" && r["declared_type"] == "Other"));
}

#[test]
fn contract_types_gap_on_complex_or_deep_types_without_partial_references() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let deep = format!("{}Pet{}", "Array<".repeat(20), ">".repeat(20));
    put(root, "app/route.ts", &format!("interface Pet {{ id: string; }}\n\nexport function GET(): {deep} {{ throw 0; }}\n\nexport function POST(): Wrapper<Pet, 'literal'> {{ throw 0; }}\n"));
    put(root, "app.rs", "struct Pet;\n\nfn get() -> Wrapper<Pet, 4> { todo!() }\n\nfn router() { Router::new().route(\"/pets\", get(get)); }\n");
    let view = ok(root, &["project", "contracts", "--types", "--limit", "500"]);
    assert_eq!(view["analysis"]["type_references"], 0, "{view}");
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "route-contract-field")
        .all(|r| r["type_reference_count"].is_null()));
    assert_eq!(
        view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(
                |r| r["kind"] == "route-contract-gap" && r["basis"] == "declared-type-references"
            )
            .count(),
        3
    );
}

#[test]
fn contract_types_pages_bind_mode_scope_revision_and_preserve_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "app/route.ts",
        "interface Pet { id: string; }\n\nexport function GET(): Promise<Pet> { throw 0; }\n",
    );
    let full = ok(root, &["project", "contracts", "--types", "--limit", "500"]);
    let first = ok(root, &["project", "contracts", "--types", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap().to_owned();
    let mut page = first;
    let mut combined = page["items"].as_array().unwrap().clone();
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &[
                "project",
                "contracts",
                "--types",
                "--limit",
                "2",
                "--cursor",
                next,
            ],
        );
        assert!(page["items"].as_array().unwrap().len() <= 2);
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    let routes = ok(root, &["project", "routes", "--limit", "500"]);
    assert_eq!(
        combined
            .iter()
            .filter(|r| r["kind"] == "route" || r["kind"] == "route-handler")
            .cloned()
            .collect::<Vec<_>>(),
        *routes["items"].as_array().unwrap()
    );
    assert!(!run(root, &["project", "contracts", "--cursor", &cursor]).0);
    assert!(
        !run(
            root,
            &[
                "project",
                "contracts",
                "app",
                "--types",
                "--cursor",
                &cursor
            ]
        )
        .0
    );
    let plain = ok(root, &["project", "contracts", "--limit", "1"]);
    assert!(
        !run(
            root,
            &[
                "project",
                "contracts",
                "--types",
                "--cursor",
                plain["page"]["next"].as_str().unwrap()
            ]
        )
        .0
    );
    put(
        root,
        "app/route.ts",
        "interface Other { id: string; }\n\nexport function GET(): Other { throw 0; }\n",
    );
    assert!(
        !run(
            root,
            &["project", "contracts", "--types", "--cursor", &cursor]
        )
        .0
    );
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn contract_types_match_full_names_before_clipping_and_preserve_reference_ids() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "名".repeat(180);
    put(
        root,
        "app/route.ts",
        &format!(
            "interface {name} {{ id: string; }}\n\nexport function GET(): {name} {{ throw 0; }}\n"
        ),
    );
    let view = ok(root, &["project", "contracts", "--types", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let field = items.iter().find(|r| r["location"] == "return").unwrap();
    assert_eq!(field["declared_type"]["text"].as_str().unwrap().len(), 510);
    assert_eq!(field["declared_type"]["omitted_bytes"], name.len() - 510);
    let reference = items
        .iter()
        .find(|r| r["kind"] == "route-contract-type-reference")
        .unwrap();
    assert_eq!(reference["field"], field["id"]);
    assert_eq!(reference["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(reference["candidate_count"], 1);
    assert!(items.iter().any(|r| r["reference"] == reference["id"]
        && r["target"]["name"]["omitted_bytes"] == name.len() - 159));
}

#[test]
fn contract_types_and_rust_schemas_use_captured_source_after_removal() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "app.rs", "struct Pet { id: u64 }\n\nfn get() -> Pet { todo!() }\n\nfn router() { Router::new().route(\"/pets\", get(get)); }\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join("app.rs")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts", "--types"]);
    let contracts = project.report(&query.command).unwrap();
    let target = contracts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "route-contract-type-candidate")
        .unwrap();
    let query = <Query as clap::Parser>::parse_from([
        "fr",
        "schemas",
        target["target"]["handle"].as_str().unwrap(),
    ]);
    let schemas = project.report(&query.command).unwrap();
    assert_eq!(schemas["analysis"]["fields"], 1);
    assert!(project.verify(&root).is_err());
}

#[test]
fn contracts_preserve_axum_extractor_types_and_declared_responses() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "async fn create(Path(id): Path<u64>, Query(filter): axum::extract::Query<Filter>, Json(body): Json<Create>, state: State<App>, optional: Option<Json<Other>>) -> Result<Json<Pet>, StatusCode> { todo!(\"PRIVATE_BODY\") }\nfn router() { Router::new().route(\"/pets/{id}\", post(create)); }\n");
    let view = ok(root, &["project", "contracts", "app.rs", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let summary = items
        .iter()
        .find(|r| r["kind"] == "route-contract")
        .unwrap();
    assert_eq!(summary["request_fields"], 4, "{view}");
    assert_eq!(summary["response_fields"], 1);
    assert_eq!(summary["gap_count"], 1);
    assert_eq!(summary["completeness"], "partial");
    let inputs: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "axum-extractor-type")
        .collect();
    for (location, ty, inner) in [
        ("path", "Path<u64>", "u64"),
        ("query", "axum::extract::Query<Filter>", "Filter"),
        ("body", "Json<Create>", "Create"),
    ] {
        let input = inputs.iter().find(|r| r["location"] == location).unwrap();
        assert_eq!(input["declared_type"], ty);
        assert_eq!(input["payload_type"], inner);
        assert_eq!(input["confidence"], "name-only");
        assert!(input["required"].is_null());
        assert!(input["name"].is_null());
        assert_eq!(
            ok(
                root,
                &[
                    "project",
                    "show",
                    input["handler"]["handle"].as_str().unwrap()
                ]
            )["node"]["name"],
            "create"
        );
    }
    let response = items.iter().find(|r| r["direction"] == "response").unwrap();
    assert_eq!(response["declared_type"], "Result<Json<Pet>, StatusCode>");
    assert!(response["confidence"].is_null());
    assert_eq!(
        items
            .iter()
            .find(|r| r["kind"] == "route-contract-gap")
            .unwrap()["parameters"],
        2
    );
    assert!(!view.to_string().contains("PRIVATE_BODY"));
    let routes = ok(root, &["project", "routes", "app.rs", "--limit", "500"]);
    let old_rows: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "route" || r["kind"] == "route-handler")
        .cloned()
        .collect();
    assert_eq!(old_rows, *routes["items"].as_array().unwrap());
}

#[test]
fn contracts_read_spring_binding_annotations_without_defaults_or_body_values() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root,"App.java","class App {\n@PostMapping(\"/pets/{id}\")\nPet create(@PathVariable(\"id\") long key,@RequestParam(name=\"q\",required=false,defaultValue=\"PRIVATE_DEFAULT\") String query,@RequestBody Create body,@RequestHeader(\"X-Trace\") String trace,@CookieValue(value=\"session\") String token,@Other String ignored) { throw new RuntimeException(\"PRIVATE_BODY\"); }\n}\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let inputs: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "spring-parameter-annotation")
        .collect();
    assert_eq!(inputs.len(), 5, "{view}");
    for (location, name, binding, ty) in [
        ("path", Some("id"), "key", "long"),
        ("query", Some("q"), "query", "String"),
        ("body", None, "body", "Create"),
        ("header", Some("X-Trace"), "trace", "String"),
        ("cookie", Some("session"), "token", "String"),
    ] {
        let input = inputs.iter().find(|r| r["location"] == location).unwrap();
        assert_eq!(input["name"].as_str(), name);
        assert_eq!(input["binding"], binding);
        assert_eq!(input["declared_type"], ty);
        assert!(input["required"].is_null());
    }
    assert_eq!(
        items.iter().find(|r| r["direction"] == "response").unwrap()["declared_type"],
        "Pet"
    );
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn contracts_keep_handler_ambiguity_and_report_unreadable_or_missing_signatures() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "App.java", "class A { @GetMapping(\"/a\") String run(@RequestParam String q) { return \"PRIVATE_A\"; } }\nclass B { Integer run(@RequestBody Input body) { return 1; } }\n");
    put(root, "app.ts", "function typed(req: Request): Promise<Response> { throw 'PRIVATE_TS'; }\nconst arrow = (req: Request): Response => { throw 'PRIVATE_ARROW'; };\napp.get('/typed', typed);\napp.get('/arrow', arrow);\napp.get('/inline', () => 'PRIVATE_INLINE');\napp.get('/external', external);\n");
    put(
        root,
        "other.ts",
        "function external(): Secret { throw 'PRIVATE_EXTERNAL'; }\n",
    );
    put(root, "app.py", "@app.get('/typed/{id}')\ndef typed(id: int) -> Pet:\n    return 'PRIVATE_PY'\n\n@app.get('/unknown')\ndef unknown():\n    return 'PRIVATE_UNKNOWN'\n");
    let java = ok(
        root,
        &["project", "contracts", "App.java", "--limit", "500"],
    );
    let items = java["items"].as_array().unwrap();
    assert_eq!(
        items.iter().find(|r| r["kind"] == "route").unwrap()["handler"]["status"],
        "ambiguous"
    );
    let responses: Vec<_> = items
        .iter()
        .filter(|r| r["direction"] == "response")
        .collect();
    assert_eq!(responses.len(), 2);
    assert_ne!(
        responses[0]["handler"]["handle"],
        responses[1]["handler"]["handle"]
    );
    let ts = ok(root, &["project", "contracts", "app.ts", "--limit", "500"]);
    let types: Vec<_> = ts["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["direction"] == "response")
        .map(|r| &r["declared_type"])
        .collect();
    assert_eq!(
        types,
        vec![
            &Value::String("Promise<Response>".into()),
            &Value::String("Response".into())
        ]
    );
    let items = ts["items"].as_array().unwrap();
    let arrow = items.iter().find(|r| r["url"] == "/arrow").unwrap();
    assert!(items.iter().any(|r| r["route"] == arrow["id"]
        && r["declared_type"] == "Response"
        && r["handler"]["name"] == "arrow"));
    for path in ["/inline", "/external"] {
        let route = items.iter().find(|r| r["url"] == path).unwrap();
        assert!(!items
            .iter()
            .any(|r| r["route"] == route["id"] && r["direction"] == "response"));
    }
    assert!(ts["analysis"]["contract_gaps"].as_u64().unwrap() >= 3);
    let py = ok(root, &["project", "contracts", "app.py", "--limit", "500"]);
    assert!(py["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["declared_type"] == "Pet"));
    assert!(py["items"].as_array().unwrap().iter().any(|r| r["reason"]
        .as_str()
        .is_some_and(|s| s.starts_with("No explicit return type"))));
    for view in [java, ts, py] {
        assert!(!view.to_string().contains("PRIVATE_"));
    }
}

#[test]
fn contract_pages_bind_queries_scope_and_revision_and_page_fields_separately() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "src/app.rs", "fn get(Path(id): Path<u64>) -> Json<Pet> { todo!() }\nfn router() { Router::new().route(\"/pets/{id}\", get(get)); }\n");
    let full = ok(root, &["project", "contracts", "--limit", "500"]);
    let first = ok(root, &["project", "contracts", "--limit", "1"]);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(full["page"]["total"], 6);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut combined = first["items"].as_array().unwrap().clone();
    let mut page = first.clone();
    while let Some(next) = page["page"]["next"].as_str() {
        page = ok(
            root,
            &["project", "contracts", "--cursor", next, "--limit", "2"],
        );
        combined.extend(page["items"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(combined, *full["items"].as_array().unwrap());
    assert!(!run(root, &["project", "routes", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "contracts", "src", "--cursor", cursor]).0);
    let route = full["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "route")
        .unwrap();
    let file = route["file_handle"].as_str().unwrap();
    let by_file = ok(root, &["project", "contracts", file]);
    assert_eq!(by_file, ok(root, &["project", "contracts", "src/app.rs"]));
    assert_eq!(
        by_file,
        ok(
            root,
            &[
                "project",
                "contracts",
                file.rsplit(':').next().unwrap(),
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    let handler = full["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "route-handler")
        .unwrap()["handler"]["handle"]
        .as_str()
        .unwrap();
    assert!(!run(root, &["project", "contracts", handler]).0);
    for limit in ["0", "501"] {
        assert!(!run(root, &["project", "contracts", "--limit", limit]).0);
    }
    put(root, "src/new.rs", "fn new() {}\n");
    assert!(!run(root, &["project", "contracts", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "contracts", file]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn contracts_bound_unicode_types_names_and_report_syntax_and_language_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let long = "名".repeat(220);
    let ty = "Type".repeat(170);
    put(root, "App.java", &format!("class App {{ @GetMapping(\"/pets/{{{long}}}\") {ty} get(@RequestParam(\"{long}\") String q) {{ throw new RuntimeException(\"PRIVATE_BODY\"); }} }}\n"));
    put(root, "broken.py", "@app.get('/broken')\ndef broken(:\n");
    put(root, "script.sh", "echo PRIVATE_SHELL\n");
    let view = ok(root, &["project", "contracts", "--limit", "500"]);
    assert_eq!(view["analysis"]["syntax_gaps"], 1);
    assert_eq!(view["analysis"]["unsupported_files"]["bash"], 1);
    let items = view["items"].as_array().unwrap();
    let response = items.iter().find(|r| r["direction"] == "response").unwrap();
    assert_eq!(
        response["declared_type"]["text"].as_str().unwrap().len(),
        512
    );
    assert_eq!(response["declared_type"]["omitted_bytes"], ty.len() - 512);
    let named = items
        .iter()
        .find(|r| r["basis"] == "literal-path-segment")
        .unwrap();
    assert_eq!(named["name"]["text"].as_str().unwrap().len(), 159);
    assert_eq!(named["name"]["omitted_bytes"], long.len() - 159);
    assert!(items.iter().any(|r| r["kind"] == "analysis-gap"));
    assert!(items.iter().any(|r| r["kind"] == "coverage-gap"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn contract_analysis_uses_captured_types_and_final_verification_refuses_drift() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(&root, "app.rs", "fn get(Json(body): Json<Before>) -> Json<Before> { todo!() }\nfn router() { Router::new().route(\"/pets\", get(get)); }\n");
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    fs::remove_file(root.join("app.rs")).unwrap();
    let query = <Query as clap::Parser>::parse_from(["fr", "contracts"]);
    let view = project.report(&query.command).unwrap();
    let items = view["items"].as_array().unwrap();
    assert!(items.iter().any(|r| r["payload_type"] == "Before"));
    assert!(items
        .iter()
        .any(|r| r["declared_type"] == "Json<Before>" && r["direction"] == "response"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn contracts_leave_dynamic_names_and_complex_paths_unknown_and_preserve_type_spellings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "App.java", "class App {\n@GetMapping(\"/{id}/{id}/{id:[0-9]+}/{bad-name}\")\nvoid get(@org.springframework.web.bind.annotation.RequestParam(name=KEY) String q,@RequestHeader(name=\"one\",value=\"two\") String conflict,@RequestParam(\"\") String empty,@RequestBody @RequestParam String multiple) {}\n}\n");
    put(root, "app.rs", "fn create(body: axum::Json<Vec<Pet>>, form: Form<Input>, custom: MyJson<Secret>) -> impl IntoResponse { todo!() }\nfn router() { Router::new().route(\"/pets\", post(create)); }\n");
    put(root, "app.go", "package main\nfunc get(c *gin.Context) {}\nfunc routes(r *gin.Engine) { r.GET(\"/pets/:id\", get) }\n");
    let java = ok(
        root,
        &["project", "contracts", "App.java", "--limit", "500"],
    );
    let items = java["items"].as_array().unwrap();
    let paths: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "literal-path-segment" && r["kind"] == "route-contract-field")
        .collect();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["name"], "id");
    assert!(items
        .iter()
        .any(|r| r["basis"] == "literal-path-segment" && r["segments"] == 2));
    let inputs: Vec<_> = items
        .iter()
        .filter(|r| r["basis"] == "spring-parameter-annotation")
        .collect();
    assert_eq!(inputs.len(), 3, "{java}");
    assert!(inputs.iter().all(|r| r["name"].is_null()));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "route-contract-gap" && r["parameters"] == 1));
    let rust = ok(root, &["project", "contracts", "app.rs", "--limit", "500"]);
    let items = rust["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["payload_type"] == "Vec<Pet>" && r["declared_type"] == "axum::Json<Vec<Pet>>"));
    assert!(items
        .iter()
        .any(|r| r["payload_type"] == "Input" && r["declared_type"] == "Form<Input>"));
    assert!(items
        .iter()
        .any(|r| r["declared_type"] == "impl IntoResponse" && r["direction"] == "response"));
    assert!(!rust.to_string().contains("Secret"));
    let go = ok(root, &["project", "contracts", "app.go", "--limit", "500"]);
    let summary = go["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "route-contract")
        .unwrap();
    assert_eq!(summary["request_fields"], 1);
    assert_eq!(summary["response_fields"], 0);
    assert_eq!(summary["gap_count"], 2);
}

#[test]
fn contracts_preserve_absolute_rust_types_and_java_array_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "fn get() -> ::std::string::String { todo!() }\nfn router() { Router::new().route(\"/pets\", get(get)); }\n");
    put(root, "App.java", "class App {\n@GetMapping(\"/pets\")\nPet[] get(@RequestParam String names[][])[] { return null; }\n}\n");
    let rust = ok(root, &["project", "contracts", "app.rs"]);
    assert!(rust["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["direction"] == "response" && r["declared_type"] == "::std::string::String"));
    let java = ok(root, &["project", "contracts", "App.java"]);
    let items = java["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["direction"] == "response" && r["declared_type"] == "Pet[][]"));
    assert!(items
        .iter()
        .any(|r| r["direction"] == "request" && r["declared_type"] == "String[][]"));
}

fn configuration_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "deploy/compose.yaml", "services:\n  app:\n    environment:\n      APP_MODE: PRIVATE_LITERAL_VALUE\n      ORPHAN: PRIVATE_ORPHAN_VALUE\n");
    put(
        dir.path(),
        "deploy/k8s.yaml",
        "spec:\n  env:\n    - name: APP_MODE\n      value: PRIVATE_K8S_VALUE\n",
    );
    put(dir.path(), "app/a.py", "import os\ndef load():\n    return os.getenv('APP_MODE', 'PRIVATE_DEFAULT')\nx = os.getenv('UNDECLARED')\n");
    put(
        dir.path(),
        "app/b.ts",
        "const mode = process.env.APP_MODE;\n",
    );
    dir
}

#[test]
fn configuration_pages_preserve_competing_declarations_and_name_only_consumers() {
    use fun_refactor::{analysis::stitch, index::Index, scan::ScanOptions};
    let dir = configuration_fixture();
    let root = dir.path();
    let index = Index::build(&root.canonicalize().unwrap(), &ScanOptions::default()).unwrap();
    let expected = stitch::chains(&index).unwrap();
    let view = ok(root, &["project", "configuration", "--limit", "500"]);
    let items = view["items"].as_array().unwrap();
    let declarations: Vec<_> = items
        .iter()
        .filter(|r| r["kind"] == "config-declaration")
        .collect();
    assert_eq!(declarations.len(), expected.len());
    assert_eq!(declarations.len(), 3);
    for chain in expected {
        let row = declarations
            .iter()
            .find(|r| {
                r["name"] == chain.env_var
                    && chain
                        .declared_in
                        .ends_with(r["site"]["path"].as_str().unwrap())
            })
            .unwrap();
        let consumers: Vec<_> = items
            .iter()
            .filter(|r| r["declaration"] == row["id"])
            .collect();
        assert_eq!(row["consumer_count"], chain.reads.len());
        assert_eq!(consumers.len(), chain.reads.len());
        for read in chain.reads {
            assert!(consumers.iter().any(|r| read
                .file
                .ends_with(r["site"]["path"].as_str().unwrap())
                && r["site"]["line"] == read.line
                && r["confidence"] == "name-only"));
        }
        let detail = ok(
            root,
            &[
                "project",
                "show",
                row["site"]["file_handle"].as_str().unwrap(),
            ],
        );
        assert_eq!(detail["node"]["kind"], "file");
    }
    assert!(items
        .iter()
        .any(|r| r["name"] == "ORPHAN" && r["consumer_status"] == "no-observed-consumer"));
    assert!(items.iter().any(|r| r["name"] == "UNDECLARED"
        && r["declaration"].is_null()
        && r["status"] == "no-observed-declaration"));
    assert!(!view.to_string().contains("PRIVATE_"));
    assert!(items
        .iter()
        .all(|r| r.get("source").is_none() && r.get("text").is_none()));
}

#[test]
fn configuration_scope_follows_selected_declarations_and_reads() {
    let dir = configuration_fixture();
    let root = dir.path();
    let code = ok(root, &["project", "configuration", "app/a.py"]);
    assert_eq!(code["analysis"]["selected_declarations"], 2);
    assert_eq!(code["analysis"]["selected_consumers"], 2);
    assert_eq!(code["analysis"]["selected_reads_without_declarations"], 1);
    assert!(code["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "config-consumer")
        .all(|r| r["site"]["path"] == "app/a.py"));
    let manifest = ok(root, &["project", "configuration", "deploy/compose.yaml"]);
    assert_eq!(manifest["analysis"]["selected_declarations"], 2);
    assert_eq!(manifest["analysis"]["selected_consumers"], 2);
    assert_eq!(
        manifest["analysis"]["selected_reads_without_declarations"],
        0
    );
    let function = relation_handle(root, "load", None);
    assert!(!run(root, &["project", "configuration", &function]).0);
}

#[test]
fn configuration_values_links_keep_heuristic_basis_and_conditions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "chart/Chart.yaml", "name: demo\nversion: 0.1.0\n");
    put(
        root,
        "chart/values.yaml",
        "db:\n  url: PRIVATE_VALUE\nenabled: true\n",
    );
    put(root, "chart/templates/app.yaml", "spec:\n  env:\n    {{ if .Values.enabled }}\n    - name: DATABASE_URL\n      value: {{ .Values.db.url }}\n    {{ end }}\n");
    put(root, "app.py", "import os\nos.getenv('DATABASE_URL')\n");
    let view = ok(root, &["project", "configuration", "chart/values.yaml"]);
    let declaration = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "config-declaration")
        .unwrap();
    assert_eq!(declaration["values_path"], "db.url");
    assert_eq!(declaration["values_path_components"], 2);
    assert_eq!(
        declaration["values_file_candidate"]["path"],
        "chart/values.yaml"
    );
    assert!(declaration["values_file_candidate"]["line"].is_null());
    assert_eq!(
        declaration["values_file_basis"],
        "nearest-ancestor-leaf-name"
    );
    assert!(declaration["conditional_on"]
        .as_str()
        .unwrap()
        .contains("enabled"));
    assert_eq!(declaration["selected_consumers"], 1);
    assert!(!view.to_string().contains("PRIVATE_VALUE"));
}

#[test]
fn configuration_pages_are_complete_bounded_and_revision_bound() {
    let dir = configuration_fixture();
    let root = dir.path();
    let full = ok(root, &["project", "configuration", "--limit", "500"]);
    let first = ok(root, &["project", "configuration", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut current = first.clone();
    let mut collected = Vec::new();
    loop {
        collected.extend(current["items"].as_array().unwrap().clone());
        let Some(next) = current["page"]["next"].as_str() else {
            break;
        };
        current = ok(
            root,
            &["project", "configuration", "--cursor", next, "--limit", "2"],
        );
    }
    assert_eq!(collected, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &["project", "configuration", "app", "--cursor", cursor]
        )
        .0
    );
    assert!(!run(root, &["project", "routes", "--cursor", cursor]).0);
    for limit in ["0", "501"] {
        assert!(!run(root, &["project", "configuration", "--limit", limit]).0);
    }
    let file = relation_handle(root, "a.py", None);
    let id = file.rsplit(':').next().unwrap();
    assert_eq!(
        ok(root, &["project", "configuration", &file]),
        ok(
            root,
            &[
                "project",
                "configuration",
                id,
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    put(root, "new.py", "import os\nos.getenv('APP_MODE')\n");
    assert!(!run(root, &["project", "configuration", "--cursor", cursor]).0);
    assert!(!run(root, &["project", "configuration", &file]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn configuration_gaps_and_long_names_do_not_become_false_absence_or_unbounded_rows() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = "A".repeat(500);
    put(
        root,
        "compose.yaml",
        &format!("services:\n  app:\n    environment:\n      {name}: PRIVATE_VALUE\n"),
    );
    put(
        root,
        "app.ts",
        &format!("const value = process.env.{name};\n"),
    );
    put(
        root,
        "broken.py",
        "import os\nos.getenv('APP_MODE')\nprint(\n",
    );
    put(root, "page.html", "<main>hello</main>\n");
    let view = ok(root, &["project", "configuration"]);
    assert_eq!(view["analysis"]["gaps"], 1);
    assert_eq!(view["analysis"]["unsupported_files"]["html"], 1);
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["kind"] == "analysis-gap" && r["path"] == "broken.py"));
    assert!(items
        .iter()
        .any(|r| r["kind"] == "coverage-gap" && r["language"] == "html"));
    let declaration = items
        .iter()
        .find(|r| r["kind"] == "config-declaration")
        .unwrap();
    assert_eq!(declaration["name"]["omitted_bytes"], 340);
    assert_eq!(declaration["consumer_count"], 1);
    assert!(items
        .iter()
        .any(|r| r["kind"] == "config-consumer" && r["name"]["omitted_bytes"] == 340));
    assert!(!view.to_string().contains("PRIVATE_VALUE"));
    let narrow = ok(root, &["project", "configuration", "app.ts"]);
    assert_eq!(narrow["analysis"]["gaps"], 1);
}

#[test]
fn configuration_project_query_reads_its_snapshot_and_refuses_final_source_drift() {
    use fun_refactor::{
        index::Index,
        project::Project,
        scan::{scan, ScanOptions},
    };
    #[derive(clap::Parser)]
    struct Query {
        #[command(subcommand)]
        command: fun_refactor::project::Command,
    }
    let dir = configuration_fixture();
    let root = dir.path().canonicalize().unwrap();
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    put(&root, "app/a.py", "print('changed')\n");
    let query = <Query as clap::Parser>::parse_from(["fr", "configuration"]);
    let view = project.report(&query.command).unwrap();
    assert!(view["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "UNDECLARED"));
    assert!(project.verify(&root).is_err());
}

#[test]
fn test_pages_preserve_catalog_rules_and_include_helpers_without_claiming_execution() {
    use fun_refactor::{
        analysis::entrypoints::{Catalog, EntryKind},
        index::Index,
        scan::ScanOptions,
    };
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "test_app.py",
        "def helper():\n    return 'PRIVATE_BODY'\n\ndef test_app():\n    helper()\n",
    );
    put(
        root,
        "other.py",
        "@pytest.fixture\ndef shared():\n    return 'PRIVATE_FIXTURE'\n",
    );
    put(root, "app.rs", "#[test]\nfn arbitrary_name() {}\n");
    put(root, "app_test.go", "package main\nfunc TestApp() {}\n");
    let index = Index::build(&root.canonicalize().unwrap(), &ScanOptions::default()).unwrap();
    let expected: Vec<_> = Catalog::builtin()
        .unwrap()
        .detect(&index)
        .into_iter()
        .filter(|e| e.kind == EntryKind::Test)
        .collect();
    let view = ok(root, &["project", "tests", "--limit", "500"]);
    let actual: Vec<_> = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "test-candidate")
        .collect();
    assert_eq!(actual.len(), expected.len());
    for entry in expected {
        let name = &index.symbol(entry.symbol).unwrap().name;
        let row = actual.iter().find(|r| r["test"]["name"] == *name).unwrap();
        assert_eq!(row["rule"], entry.rule);
        assert_eq!(row["basis"], "catalog-in-scope");
        assert_eq!(row["hops"], 0);
        assert!(row["confidence"].is_null() && row["path_confidence"].is_null());
        assert_eq!(
            ok(
                root,
                &["project", "show", row["test"]["handle"].as_str().unwrap()]
            )["node"]["name"],
            *name
        );
    }
    assert!(actual.iter().any(|r| r["test"]["name"] == "helper"));
    assert!(actual.iter().any(|r| r["test"]["name"] == "shared"));
    assert!(!view.to_string().contains("PRIVATE_"));
}

fn test_paths_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "app.py", "def leaf():\n    return 1\n\ndef bridge():\n    return leaf()\n\ndef cycle():\n    cycle()\n    bridge()\n\ndef test_direct():\n    leaf()\n\ndef test_indirect():\n    bridge()\n\ndef test_cycle():\n    cycle()\n\ndef test_unrelated():\n    missing()\n");
    dir
}

#[test]
fn test_call_paths_respect_depth_terminate_cycles_and_retain_real_edges() {
    let dir = test_paths_fixture();
    let root = dir.path();
    let target = relation_handle(root, "leaf", None);
    for (depth, expected) in [("0", 0), ("1", 1), ("2", 2), ("3", 3), ("16", 3)] {
        let view = ok(
            root,
            &[
                "project", "tests", &target, "--depth", depth, "--limit", "500",
            ],
        );
        assert_eq!(view["analysis"]["selected_candidates"], expected);
        assert!(view["analysis"]["unresolved_calls"].as_u64().unwrap() > 0);
        if depth == "0" {
            assert!(view["analysis"]["depth_frontier_nodes"].as_u64().unwrap() > 0);
        }
        let items = view["items"].as_array().unwrap();
        for row in items.iter().filter(|r| r["kind"] == "test-candidate") {
            let mut path: Vec<_> = items
                .iter()
                .filter(|r| r["test_candidate"] == row["id"])
                .collect();
            path.sort_by_key(|r| r["step"].as_u64().unwrap());
            assert_eq!(path.len() as u64, row["hops"].as_u64().unwrap());
            assert_eq!(
                path.first().unwrap()["caller"]["handle"],
                row["test"]["handle"]
            );
            assert_eq!(path.last().unwrap()["callee"]["handle"], target);
            for pair in path.windows(2) {
                assert_eq!(pair[0]["callee"]["handle"], pair[1]["caller"]["handle"]);
            }
            for edge in path {
                let calls = ok(
                    root,
                    &[
                        "project",
                        "calls",
                        edge["caller"]["handle"].as_str().unwrap(),
                        "--direction",
                        "outgoing",
                    ],
                );
                assert!(calls["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|call| call["callee"]["handle"] == edge["callee"]["handle"]
                        && call["site"] == edge["site"]
                        && call["confidence"] == edge["confidence"]
                        && call["origin"] == edge["origin"]));
            }
            assert_eq!(row["path_confidence"], "exact");
        }
    }
}

#[test]
fn test_paths_keep_dispatch_evidence_and_the_weakest_confidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "app.rs", "trait Shape { fn area(&self); }\nstruct A;\nimpl Shape for A { fn area(&self) {} }\nfn report(s: &dyn Shape) { s.area(); }\n#[test]\nfn check_shape() { report(&A); }\n");
    let target = relation_handle(root, "area", Some("A"));
    let view = ok(root, &["project", "tests", &target]);
    let items = view["items"].as_array().unwrap();
    let candidate = items
        .iter()
        .find(|r| r["kind"] == "test-candidate")
        .unwrap();
    assert_eq!(candidate["test"]["name"], "check_shape");
    assert_eq!(candidate["hops"], 2);
    assert_eq!(candidate["path_confidence"], "field-based");
    assert!(candidate["confidence"].is_null());
    assert!(items.iter().any(|r| r["kind"] == "test-path-edge"
        && r["dispatch_candidate"] == true
        && r["confidence"] == "field-based"));
}

#[test]
fn test_pages_bind_depth_and_page_witnesses_separately() {
    let dir = test_paths_fixture();
    let root = dir.path();
    let target = relation_handle(root, "leaf", None);
    let full = ok(
        root,
        &[
            "project", "tests", &target, "--depth", "16", "--limit", "500",
        ],
    );
    let first = ok(
        root,
        &["project", "tests", &target, "--depth", "16", "--limit", "1"],
    );
    assert_eq!(first["page"]["returned"], 1);
    let cursor = first["page"]["next"].as_str().unwrap();
    let mut current = first.clone();
    let mut collected = Vec::new();
    loop {
        collected.extend(current["items"].as_array().unwrap().clone());
        let Some(next) = current["page"]["next"].as_str() else {
            break;
        };
        current = ok(
            root,
            &[
                "project", "tests", &target, "--depth", "16", "--limit", "2", "--cursor", next,
            ],
        );
    }
    assert_eq!(collected, *full["items"].as_array().unwrap());
    assert!(
        !run(
            root,
            &["project", "tests", &target, "--depth", "15", "--cursor", cursor]
        )
        .0
    );
    assert!(
        !run(
            root,
            &["project", "tests", "--depth", "16", "--cursor", cursor]
        )
        .0
    );
    assert!(!run(root, &["project", "calls", &target, "--cursor", cursor]).0);
    assert!(!run(root, &["project", "tests", "--depth", "17"]).0);
    assert!(!run(root, &["project", "tests", "--limit", "0"]).0);
    assert!(!run(root, &["project", "tests", "--limit", "501"]).0);
    let id = target.rsplit(':').next().unwrap();
    assert_eq!(
        ok(root, &["project", "tests", &target]),
        ok(
            root,
            &[
                "project",
                "tests",
                id,
                "--revision",
                full["revision"].as_str().unwrap()
            ]
        )
    );
    put(root, "new.py", "def test_added():\n    pass\n");
    assert!(!run(root, &["project", "tests", &target]).0);
    assert!(
        !run(
            root,
            &["project", "tests", "--depth", "16", "--cursor", cursor]
        )
        .0
    );
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn test_catalogs_report_broken_inputs_and_missing_language_rules() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(root, "bad.py", "def test_partial():\n    print(\n");
    put(root, "main.sh", "echo hello\n");
    put(root, "page.html", "<main>hello</main>\n");
    let view = ok(root, &["project", "tests"]);
    assert_eq!(view["analysis"]["catalog_gaps"], 1);
    assert_eq!(view["analysis"]["files_without_test_rules"]["bash"], 1);
    let items = view["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|r| r["kind"] == "analysis-gap" && r["basis"] == "test-catalog"));
    assert!(items.iter().any(|r| r["kind"] == "coverage-gap"
        && r["basis"] == "test-catalog"
        && r["language"] == "bash"));
    assert!(!items.iter().any(|r| r["kind"] == "test-candidate"));
}

#[test]
fn test_candidate_names_clip_before_source_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let name = format!("test_{}", "long_".repeat(100));
    put(
        root,
        "app.py",
        &format!("def {name}():\n    return 'PRIVATE_BODY'\n"),
    );
    let view = ok(root, &["project", "tests"]);
    let row = view["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "test-candidate")
        .unwrap();
    assert_eq!(row["test"]["name"]["omitted_bytes"], 345);
    assert!(!view.to_string().contains("PRIVATE_BODY"));
    assert!(ok(
        root,
        &["project", "show", row["test"]["handle"].as_str().unwrap()]
    )["node"]["name"]
        .is_object());
}

#[test]
fn workspace_closure_keeps_first_round_witnesses_and_declared_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['a','z','declared']\n",
    );
    cargo_package(
        root,
        "a",
        "a",
        "[dependencies]\nz={path='../z'}\nshared={path='../shared'}\n",
    );
    cargo_package(
        root,
        "z",
        "z",
        "[build-dependencies]\nshared={path='../shared'}\nleaf={path='../leaf'}\n",
    );
    cargo_package(
        root,
        "shared",
        "shared",
        "[dev-dependencies]\nleaf={path='../leaf'}\ndeclared={path='../declared'}\n",
    );
    cargo_package(
        root,
        "leaf",
        "leaf",
        "[target.'cfg(unix)'.dependencies]\ntail={path='../tail'}\n",
    );
    cargo_package(root, "tail", "tail", "[dependencies]\na={path='../a'}\n");
    cargo_package(root, "declared", "declared", "");
    cargo_package(
        root,
        "unused",
        "unused",
        "[dependencies]\ndisconnected={path='../disconnected'}\n",
    );
    cargo_package(
        root,
        "disconnected",
        "disconnected",
        "[dependencies]\nunused={path='../unused'}\n",
    );
    let view = ok(root, &["project", "workspaces"]);
    let rows = view["items"].as_array().unwrap();
    let row = |manifest: &str| rows.iter().find(|r| r["manifest"] == manifest).unwrap();
    for (manifest, source) in [
        ("shared/Cargo.toml", "a/Cargo.toml"),
        ("leaf/Cargo.toml", "z/Cargo.toml"),
        ("tail/Cargo.toml", "leaf/Cargo.toml"),
    ] {
        assert_eq!(row(manifest)["status"], "member");
        assert_eq!(row(manifest)["membership_basis"], "automatic-path-member");
        assert_eq!(row(manifest)["via_manifest"], source);
    }
    assert_eq!(
        row("declared/Cargo.toml")["membership_basis"],
        "declared-member"
    );
    assert!(row("declared/Cargo.toml")["via_manifest"].is_null());
    for manifest in ["unused/Cargo.toml", "disconnected/Cargo.toml"] {
        assert_eq!(row(manifest)["status"], "unresolved");
        assert_eq!(row(manifest)["reason"], "package-not-observed-member");
    }
}

#[test]
fn workspace_closure_cannot_cross_excluded_or_different_owner_packages() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['app']\nexclude=['blocked']\n",
    );
    cargo_package(
        root,
        "app",
        "app",
        "[dependencies]\nblocked={path='../blocked'}\nforeign={path='../nested/foreign'}\n",
    );
    cargo_package(
        root,
        "blocked",
        "blocked",
        "[dependencies]\nleak={path='../leak'}\n",
    );
    cargo_package(root, "leak", "leak", "");
    put(root, "nested/Cargo.toml", "[workspace]\nmembers=['seed']\n");
    cargo_package(
        root,
        "nested/seed",
        "seed",
        "[dependencies]\nchild={path='../child'}\n",
    );
    cargo_package(root, "nested/child", "child", "");
    cargo_package(
        root,
        "nested/foreign",
        "foreign",
        "[dependencies]\nleak={path='../../leak'}\n",
    );
    let view = ok(root, &["project", "workspaces"]);
    let rows = view["items"].as_array().unwrap();
    let row = |manifest: &str| rows.iter().find(|r| r["manifest"] == manifest).unwrap();
    assert_eq!(row("blocked/Cargo.toml")["reason"], "workspace-excluded");
    assert_eq!(row("leak/Cargo.toml")["status"], "unresolved");
    assert_eq!(row("nested/foreign/Cargo.toml")["status"], "unresolved");
    assert_eq!(row("nested/child/Cargo.toml")["status"], "member");
    assert_eq!(
        row("nested/child/Cargo.toml")["workspace_manifest"],
        "nested/Cargo.toml"
    );
    assert_eq!(
        row("nested/child/Cargo.toml")["via_manifest"],
        "nested/seed/Cargo.toml"
    );
}

#[test]
fn cargo_subtree_exclusions_and_literal_member_overrides_match_metadata() {
    use std::collections::BTreeSet;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (path, name) in [
        ("ws/zone/a", "a"),
        ("ws/zone/b", "b"),
        ("ws/zone/a/child", "child"),
        ("ws/zone/a-sibling", "a-sibling"),
        ("ws/λ名/a", "unicode"),
        ("ws/λ名extra/a", "unicode-neighbor"),
    ] {
        cargo_package(root, path, name, "");
    }
    cargo_package(
        root,
        "ws/zone/a",
        "a",
        "[dependencies]\nchild={path='child'}\n",
    );
    for (members, exclude) in [
        ("['zone/*']", "['zone']"),
        ("['zone/*','zone/a']", "['zone']"),
        ("['zone/a']", "['zone/a']"),
        ("['zone/*']", "['zone/a']"),
        ("['zone/*']", "['zone/a/child']"),
        ("['zone/a']", "['zone/a/child']"),
        ("['zone/*']", "['zone/a/Cargo.toml']"),
        ("['zone/a']", "['zone/a/Cargo.toml']"),
        ("['zone/*','../ws/zone/a']", "['zone']"),
        ("['zone/*']", "['./zone/']"),
        ("['λ名/*','λ名extra/*']", "['λ名']"),
        ("['λ名/*','λ名extra/*','λ名/a']", "['λ名']"),
    ] {
        put(
            root,
            "ws/Cargo.toml",
            &format!("[workspace]\nresolver='2'\nmembers={members}\nexclude={exclude}\n"),
        );
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ])
            .current_dir(root.join("ws"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{members} {exclude}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        let expected: BTreeSet<_> = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                metadata["workspace_members"]
                    .as_array()
                    .unwrap()
                    .contains(&p["id"])
            })
            .map(|p| {
                Path::new(p["manifest_path"].as_str().unwrap())
                    .strip_prefix(root.canonicalize().unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let view = ok(root, &["project", "workspaces"]);
        let actual: BTreeSet<_> = view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "member")
            .map(|r| r["manifest"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual, expected, "{members} {exclude}");
        let links = ok(root, &["project", "links", "--manifest", "ws/Cargo.toml"]);
        for row in links["items"].as_array().unwrap() {
            let target = row["candidate_manifest"].as_str().unwrap();
            assert_eq!(
                row["status"],
                if expected.contains(target) {
                    "matched"
                } else {
                    "excluded"
                },
                "{members} {exclude}: {row}"
            );
            if row["status"] == "excluded" {
                assert!(row["target_manifest"].is_null());
                assert!(row["excluded_by"].is_string());
            }
        }
    }
}

#[test]
fn cargo_exclusion_overrides_control_inherited_dependencies_and_transitive_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    cargo_package(
        root,
        "zone/app",
        "app",
        "[dependencies]\ncore={workspace=true}\n",
    );
    cargo_package(root, "core", "core", "");
    for (members, linked) in [
        ("['zone/*']", false),
        ("['zone/*','zone/app']", true),
        ("['./zone/app']", true),
    ] {
        put(root, "Cargo.toml", &format!("[workspace]\nmembers={members}\nexclude=['zone']\n[workspace.dependencies]\ncore={{path='core'}}\n"));
        let links = ok(
            root,
            &["project", "links", "--manifest", "zone/app/Cargo.toml"],
        );
        let inherited = &links["items"][0];
        if linked {
            assert_eq!(inherited["status"], "linked");
            assert_eq!(inherited["target_manifest"], "core/Cargo.toml");
        } else {
            assert_eq!(inherited["status"], "unresolved");
            assert_eq!(inherited["reason"], "workspace-excluded");
        }
        let view = ok(
            root,
            &["project", "workspaces", "--manifest", "core/Cargo.toml"],
        );
        assert_eq!(
            view["items"][0]["status"],
            if linked { "member" } else { "unresolved" }
        );
        if linked {
            assert_eq!(view["items"][0]["via_manifest"], "zone/app/Cargo.toml");
        }
    }
}

#[test]
fn cargo_glob_shaped_exclusions_are_unresolved_even_with_explicit_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    cargo_package(root, "zone/a", "a", "");
    cargo_package(root, "zone/b", "b", "");
    for exclude in ["zone/*", "zone/**", "zone/?", "zone/[ab]"] {
        put(
            root,
            "Cargo.toml",
            &format!("[workspace]\nmembers=['zone/*','zone/a']\nexclude=['{exclude}']\n"),
        );
        let view = ok(root, &["project", "workspaces"]);
        assert!(view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "workspace-membership")
            .all(|r| r["status"] == "unresolved" && r["reason"] == "unsupported-exclusion"));
        let links = ok(root, &["project", "links", "--manifest", "Cargo.toml"]);
        assert!(links["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "unresolved" && r["reason"] == "unsupported-exclusion"));
    }
}

#[test]
fn cargo_subtree_exclusions_use_captured_manifests_and_reject_stale_cursors() {
    use fun_refactor::index::Index;
    use fun_refactor::project::{Command as ProjectCommand, Project};
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=['zone/*']\nexclude=['zone']\n",
    );
    cargo_package(&root, "zone/a", "a", "");
    cargo_package(&root, "zone/b", "b", "");
    let commands = [
        ProjectCommand::Workspaces {
            manifest: None,
            limit: 500,
            cursor: None,
        },
        ProjectCommand::Links {
            manifest: None,
            limit: 500,
            cursor: None,
        },
    ];
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    let captured: Vec<_> = commands
        .iter()
        .map(|command| project.report(command).unwrap())
        .collect();
    let cursors: Vec<_> = ["workspaces", "links"]
        .iter()
        .map(|command| {
            ok(&root, &["project", command, "--limit", "1"])["page"]["next"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    put(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=['zone/*','zone/a']\nexclude=['zone']\n",
    );
    for (command, expected) in commands.iter().zip(captured) {
        assert_eq!(project.report(command).unwrap(), expected);
    }
    assert!(project.verify(&root).is_err());
    for (command, cursor) in ["workspaces", "links"].iter().zip(cursors) {
        assert!(!run(&root, &["project", command, "--cursor", &cursor]).0);
    }
    let view = ok(
        &root,
        &["project", "workspaces", "--manifest", "zone/a/Cargo.toml"],
    );
    assert_eq!(view["items"][0]["status"], "member");
}

#[test]
fn cargo_parent_relative_members_and_inheritance_agree_with_metadata() {
    use std::collections::BTreeSet;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    cargo_package(
        root,
        "workspaces/ws/app",
        "app",
        "[dependencies]\ndetached={path='../../../detached'}\n",
    );
    cargo_package(root, "detached", "detached", "");
    cargo_package(
        root,
        "crates/shared",
        "shared",
        "workspace='../../workspaces/ws'\n[dependencies]\nleaf={workspace=true}\n",
    );
    cargo_package(
        root,
        "crates/leaf",
        "leaf",
        "workspace='../../workspaces/ws'\n",
    );
    cargo_package(
        root,
        "crates/unused",
        "unused",
        "workspace='../../workspaces/ws'\n",
    );
    for members in [
        "['app','../../crates/shared']",
        "['app','./../../crates/shared']",
        "['app','../../crates/*']",
    ] {
        put(root, "workspaces/ws/Cargo.toml", &format!("[workspace]\nresolver='2'\nmembers={members}\n[workspace.dependencies]\nleaf={{path='../../crates/leaf'}}\n"));
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--no-deps",
                "--format-version",
                "1",
            ])
            .current_dir(root.join("workspaces/ws"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{members}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        let canonical = root.canonicalize().unwrap();
        let expected: BTreeSet<_> = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                metadata["workspace_members"]
                    .as_array()
                    .unwrap()
                    .contains(&p["id"])
            })
            .map(|p| {
                Path::new(p["manifest_path"].as_str().unwrap())
                    .strip_prefix(&canonical)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let view = ok(root, &["project", "workspaces"]);
        let actual: BTreeSet<_> = view["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["status"] == "member")
            .map(|r| r["manifest"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual, expected, "{members}");
        assert!(actual.contains("crates/shared/Cargo.toml"));
        assert!(actual.contains("crates/leaf/Cargo.toml"));
        assert!(!actual.contains("detached/Cargo.toml"));
        let inherited = ok(
            root,
            &["project", "links", "--manifest", "crates/shared/Cargo.toml"],
        );
        assert_eq!(inherited["items"][0]["status"], "linked");
        assert_eq!(
            inherited["items"][0]["workspace_manifest"],
            "workspaces/ws/Cargo.toml"
        );
        assert_eq!(
            inherited["items"][0]["target_manifest"],
            "crates/leaf/Cargo.toml"
        );
        let links = ok(
            root,
            &["project", "links", "--manifest", "workspaces/ws/Cargo.toml"],
        );
        let sibling = links["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["candidate_manifest"] == "crates/shared/Cargo.toml")
            .unwrap();
        assert_eq!(sibling["status"], "unresolved");
        assert_eq!(sibling["reason"], "workspace-ownership-unresolved");
        assert_eq!(sibling["membership"], "candidate");
    }
}

#[test]
fn cargo_parent_patterns_preserve_distinct_and_unavailable_owners() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    put(
        root,
        "ws/Cargo.toml",
        "[workspace]\nmembers=['../crates/*']\n",
    );
    put(root, "other/Cargo.toml", "[workspace]\nmembers=[]\n");
    cargo_package(root, "crates/yes", "yes", "workspace='../../ws'\n");
    cargo_package(root, "crates/no-pointer", "no-pointer", "");
    cargo_package(root, "crates/other", "other", "workspace='../../other'\n");
    cargo_package(root, "crates/own", "own", "[workspace]\n");
    cargo_package(
        root,
        "crates/missing",
        "missing",
        "workspace='../../missing'\n",
    );
    cargo_package(root, "crates/ignored", "ignored", "workspace='../../ws'\n");
    put(root, ".gitignore", "/crates/ignored/\n");
    let view = ok(root, &["project", "workspaces"]);
    let rows = view["items"].as_array().unwrap();
    let row = |path: &str| rows.iter().find(|r| r["manifest"] == path).unwrap();
    assert_eq!(row("crates/yes/Cargo.toml")["status"], "member");
    assert_eq!(
        row("crates/yes/Cargo.toml")["workspace_manifest"],
        "ws/Cargo.toml"
    );
    assert_eq!(
        row("crates/no-pointer/Cargo.toml")["reason"],
        "workspace-root-not-observed"
    );
    assert_eq!(row("crates/other/Cargo.toml")["status"], "unresolved");
    assert_eq!(
        row("crates/other/Cargo.toml")["workspace_manifest"],
        "other/Cargo.toml"
    );
    assert_eq!(
        row("crates/own/Cargo.toml")["workspace_manifest"],
        "crates/own/Cargo.toml"
    );
    assert_eq!(row("crates/missing/Cargo.toml")["status"], "unresolved");
    assert!(!rows
        .iter()
        .any(|r| r["manifest"] == "crates/ignored/Cargo.toml"));
    let links = ok(root, &["project", "links", "--manifest", "ws/Cargo.toml"]);
    assert!(!links["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["candidate_manifest"] == "crates/ignored/Cargo.toml"));
}

#[test]
fn parent_member_patterns_refuse_scope_escape_internal_parents_and_npm_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    cargo_package(root, "crates/a", "a", "workspace='../../ws'\n");
    for (pattern, reason) in [
        ("../../crates/a", "pattern-outside-selected-root"),
        ("../missing/../crates/a", "pattern-syntax-unsupported"),
        ("../*/../crates/a", "pattern-syntax-unsupported"),
    ] {
        put(
            root,
            "ws/Cargo.toml",
            &format!("[workspace]\nmembers=['{pattern}']\n"),
        );
        let links = ok(root, &["project", "links", "--manifest", "ws/Cargo.toml"]);
        assert_eq!(links["items"][0]["status"], "unresolved");
        assert_eq!(links["items"][0]["reason"], reason);
        let view = ok(
            root,
            &["project", "workspaces", "--manifest", "crates/a/Cargo.toml"],
        );
        assert_eq!(view["items"][0]["reason"], "unsupported-members");
    }
    put(
        root,
        "web/package.json",
        r#"{"workspaces":["../crates/*"]}"#,
    );
    let npm = ok(
        root,
        &["project", "links", "--manifest", "web/package.json"],
    );
    assert_eq!(npm["items"][0]["reason"], "pattern-syntax-unsupported");
    put(
        root,
        "ws/Cargo.toml",
        "[workspace]\nmembers=['../crates/*']\nexclude=['../crates/a']\n",
    );
    let view = ok(
        root,
        &["project", "workspaces", "--manifest", "crates/a/Cargo.toml"],
    );
    assert_eq!(view["items"][0]["reason"], "unsupported-exclusion");
}

#[test]
fn parent_workspace_members_stay_snapshot_bound_and_page_without_widening_scope() {
    use fun_refactor::index::Index;
    use fun_refactor::project::{Command as ProjectCommand, Project};
    use fun_refactor::scan::{scan, ScanOptions};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    put(
        &root,
        "ws/Cargo.toml",
        "[workspace]\nmembers=['app','../crates/*']\n",
    );
    cargo_package(&root, "ws/app", "app", "");
    cargo_package(&root, "crates/a", "a", "workspace='../../ws'\n");
    cargo_package(&root, "crates/b", "b", "workspace='../../ws'\n");
    for command in ["workspaces", "links"] {
        let full = ok(&root, &["project", command]);
        let mut page = ok(&root, &["project", command, "--limit", "1"]);
        let mut rows = page["items"].as_array().unwrap().clone();
        while let Some(next) = page["page"]["next"].as_str() {
            page = ok(
                &root,
                &["project", command, "--limit", "1", "--cursor", next],
            );
            rows.extend(page["items"].as_array().unwrap().clone());
        }
        assert_eq!(rows, *full["items"].as_array().unwrap());
    }
    let scoped = ok(&root.join("ws"), &["project", "workspaces"]);
    assert!(!scoped["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "member"));
    let options = ScanOptions::default();
    let scanned = scan(&root, &options).unwrap();
    let index = Index::build_with_cache(&scanned, None).unwrap();
    let project = Project::new(&root, &index, &scanned, &options).unwrap();
    let command = ProjectCommand::Workspaces {
        manifest: None,
        limit: 500,
        cursor: None,
    };
    let captured = project.report(&command).unwrap();
    let first = ok(&root, &["project", "workspaces", "--limit", "1"]);
    let cursor = first["page"]["next"].as_str().unwrap();
    fs::remove_file(root.join("crates/a/Cargo.toml")).unwrap();
    assert_eq!(project.report(&command).unwrap(), captured);
    assert!(project.verify(&root).is_err());
    assert!(!run(&root, &["project", "workspaces", "--cursor", cursor]).0);
}
