#!/usr/bin/env python3
"""Retain one guided, reviewed Express-to-Go application migration."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any, TypedDict, cast

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk/python/src"))

from fr_ir.guide import AgentGoal, GoalLimits, GoalOperation, GoalSelector, GuideInputs
from fr_ir.intent_actions import ApplicationMigrationOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient


ROOT = Path(__file__).resolve().parents[1]
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
SOURCE = "api.ts"
DESTINATION = "generated/routes.go"
PATCH = "artifacts/application-migration.patch"
CHECK_NAMES = ("go",)
DELIVERY = TaskDelivery(patch=PATCH, check_output_bytes=4096)
STEPS = ("guide", "preview", "review", "execute", "finish")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
SOURCE_TEXT = '''import { z } from "zod";
const Query = z.object({limit: z.coerce.number().int()});
const Body = z.object({visible: z.boolean()});
function show(req: Request, res: Response) {
  const parsed = Query.safeParse(req.query);
  if (!parsed.success) return res.status(422).json({error: "validation"});
  const body = Body.safeParse(req.body);
  if (!body.success) return res.status(422).json({error: "validation"});
  return res.status(201).json({id: req.params.id, limit: parsed.data.limit, visible: body.data.visible});
}
app.post("/records/:id", show);
'''
GO_MOD = "module example.invalid/fr-application-migration\n\ngo 1.23\n"
BASELINE_GO = "package fixture\n\nfunc Baseline() bool { return true }\n"
GITIGNORE = ".cache/\n"
ORACLE_TEST = r'''package frgenerated

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestMigratedContract(t *testing.T) {
	tests := []struct {
		name, target, body string
		status int
		expected map[string]any
	}{
		{"accepted", "/records/chosen?limit=12", `{"visible":true}`, 201,
			map[string]any{"id":"chosen", "limit":float64(12), "visible":true}},
		{"negative", "/records/chosen?limit=-7", `{"visible":false}`, 201,
			map[string]any{"id":"chosen", "limit":float64(-7), "visible":false}},
		{"noncanonical integer", "/records/chosen?limit=01", `{"visible":true}`, 422, nil},
		{"duplicate query", "/records/chosen?limit=1&limit=2", `{"visible":true}`, 422, nil},
		{"missing query", "/records/chosen", `{"visible":true}`, 422, nil},
		{"wrong body type", "/records/chosen?limit=1", `{"visible":"true"}`, 422, nil},
		{"malformed body", "/records/chosen?limit=1", `{`, 422, nil},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			req := httptest.NewRequest("POST", test.target, strings.NewReader(test.body))
			res := httptest.NewRecorder()
			Handler().ServeHTTP(res, req)
			if res.Code != test.status { t.Fatalf("status: got %d want %d: %s", res.Code, test.status, res.Body.String()) }
			var body map[string]any
			if err := json.Unmarshal(res.Body.Bytes(), &body); err != nil { t.Fatal(err) }
			if test.expected != nil {
				for key, value := range test.expected {
					if body[key] != value { t.Fatalf("%s: got %#v want %#v", key, body[key], value) }
				}
			} else if body["error"] != "validation" {
				t.Fatalf("validation body: %#v", body)
			}
		})
	}
}
'''


class Event(TypedDict):
    request: dict[str, Any]
    visible: dict[str, Any]
    full: dict[str, Any] | None
    source_files: dict[str, str]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def object_map(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value):
        raise ValueError(f"{label} must be an object")
    return cast(dict[str, Any], value)


def source_files(project: Path) -> dict[str, str]:
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=project
    ).decode().split("\0")
    return {name: sha((project / name).read_bytes()) for name in names
            if name and not name.startswith(".fr/") and not name.startswith("artifacts/")}


def fixture_revision() -> str:
    return sha(json.dumps({SOURCE: SOURCE_TEXT, "go.mod": GO_MOD, "baseline.go": BASELINE_GO,
                           ".gitignore": GITIGNORE},
                          sort_keys=True, separators=(",", ":")).encode())


def evaluator_hash() -> str:
    return sha(Path(__file__).read_bytes())


def sdk_hash() -> str:
    return sha((ROOT / "sdk/python/src/fr_ir/intent.py").read_bytes())


def checks(project: Path) -> list[dict[str, object]]:
    return [{
        "name": "go",
        "argv": ["env", f"GOCACHE={project / '.cache/go-build'}", "go", "test", "./..."],
        "cwd": ".",
        "timeout_seconds": 120,
        "covers": ["Baseline and generated Go packages compile"],
    }]


def goal() -> AgentGoal:
    return AgentGoal(
        "migrate",
        selector=GoalSelector(path=SOURCE),
        operation=GoalOperation("framework-migration", {"to": "go-net-http"}),
        checks=CHECK_NAMES,
        delivery=DELIVERY,
        context=GoalLimits(packet_limit=65536),
    )


def action() -> TaggedIntentAction:
    operation = ApplicationMigrationOperation(
        to="go-net-http", out="generated", checks=CHECK_NAMES, delivery=DELIVERY,
    )
    return TaggedIntentAction(operation, diff_bytes=65536, report_bytes=65536)


def populate(project: Path) -> None:
    project.mkdir(parents=True)
    (project / SOURCE).write_text(SOURCE_TEXT)
    (project / "go.mod").write_text(GO_MOD)
    (project / "baseline.go").write_text(BASELINE_GO)
    (project / ".gitignore").write_text(GITIGNORE)


def prompt(session: Path) -> str:
    return f"""You are an acceptance agent in an unfamiliar pinned Express project.

Task: migrate the validated POST `/records/:id` route in `{SOURCE}` to the Go standard-library HTTP adapter under `generated`. Preserve the path parameter, required canonical integer `limit` query input, required Boolean `visible` JSON body input, 201 success status and JSON response. Keep the Express source for coexistence. Inspect the application migration preview and complete review before executing the unchanged review. Report success only after every check and reversible delivery stage passes.

Use only the instrumented tool below through functions.exec / tools.exec_command. Every command calls this exact step with one JSON request on stdin. Do not read project files directly, run project commands, edit source, browse, delegate, or ask a human to correct the result:
python3 {ROOT / 'tools/application-migration-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
1. {{"tool":"guide"}} selects the source-free framework migration route.
2. {{"tool":"preview"}} supplies the destination `generated` and returns the normalized route and generated diff.
3. {{"tool":"review"}} binds that destination, the declared Go check and patch delivery to one complete immutable review.
4. {{"tool":"execute"}} executes the unchanged review once.
5. {{"tool":"finish","answer":{{"source":"{SOURCE}","destination":"{DESTINATION}","method":"POST","path":"/records/{{id}}","status":201,"inputs":["path:id","query:limit:integer","json-body:visible:boolean"],"source_preserved":true,"checks":["go"]}}}} finishes after execution passes.

The evaluator independently replays the patch into a fresh receiver, confirms that only `{DESTINATION}` is added and runs seven real Go HTTP request cases covering accepted values and validation refusals. Do not claim success if any stage fails.
"""


def prepare(session: Path, binary: Path) -> dict[str, Any]:
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    populate(project)
    (project / ".fr").mkdir()
    save(project / ".fr/checks.json", {"schema": 1, "checks": checks(project)})
    (project / "artifacts").mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned migration fixture"], cwd=project, check=True)
    selected = {
        "schema": "fr-application-migration-agent-session-1",
        "fixture_revision": fixture_revision(),
        "binary": str(frozen),
        "binary_sha256": sha(frozen.read_bytes()),
        "source_files": source_files(project),
        "goal": goal().to_data(),
        "evaluator_sha256": evaluator_hash(),
        "sdk_intent_sha256": sdk_hash(),
        "manual_corrections": 0,
    }
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = object_map(json.loads((session / "session.json").read_text()), "session")
    if (value.get("schema") != "fr-application-migration-agent-session-1"
            or value.get("fixture_revision") != fixture_revision()
            or value.get("goal") != goal().to_data()
            or value.get("evaluator_sha256") != evaluator_hash()
            or value.get("sdk_intent_sha256") != sdk_hash()
            or value.get("manual_corrections") != 0
            or sha(Path(value["binary"]).read_bytes()) != value.get("binary_sha256")):
        raise ValueError("session binding changed")
    return value


def events(session: Path) -> list[Event]:
    path = session / "events.jsonl"
    if not path.exists():
        return []
    rows: list[Event] = []
    for line in path.read_text().splitlines():
        row = object_map(json.loads(line), "event")
        object_map(row.get("request"), "event request")
        object_map(row.get("visible"), "event response")
        rows.append(cast(Event, row))
    return rows


def step(session: Path, request: dict[str, Any]) -> dict[str, Any]:
    selected = config(session)
    prior = events(session)
    if len(prior) >= len(STEPS) or request.get("tool") != STEPS[len(prior)]:
        raise ValueError("tool sequence differs from the pinned workflow")
    project = session / "project"
    client = FrClient(str(project), executable=selected["binary"])
    kind = request["tool"]
    full: dict[str, Any] | None = None
    if kind == "guide":
        guide = client.guide(goal())
        full = dict(guide.to_data())
        visible = {key: full[key] for key in ("state", "route", "target", "basis", "checks")}
        visible["preview_action"] = guide.actions()[0].to_data()
    elif kind == "preview":
        guide = client.guide(goal())
        report = client.follow_guide(guide.actions()[0], GuideInputs({"destination": "generated"}))
        full = dict(report.to_data())
        visible = {
            "applied": full.get("applied"),
            "migration": full.get("migration"),
            "diff": full.get("diff"),
        }
    elif kind in ("review", "execute"):
        guide = client.guide(goal())
        review = client.review_guide(guide, action())
        operation = dict(review.at())
        if kind == "review":
            full = operation
            visible = {
                "ready": operation["ready"],
                "diff": operation["diff"],
                "checks": operation["checks"],
                "stages": operation["stages"],
                "review_sha256": review.review_sha256,
                "migration": operation["plan"]["migration"],
            }
        else:
            if review.review_sha256 != prior[2]["visible"]["review_sha256"]:
                raise ValueError("review changed before execution")
            full = dict(client.execute_guide(review).to_data())
            visible = {
                "passed": full["passed"],
                "transaction": full["transaction"],
                "stages": [{"stage": item["stage"], "status": item["status"]}
                           for item in full["workflow"]["stages"]],
                "patch": full["workflow"]["stages"][-1]["result"],
            }
    else:
        answer = object_map(request.get("answer"), "answer")
        if len(json.dumps(answer).encode()) > 2048:
            raise ValueError("answer is too large")
        visible = {"finished": True, "answer": answer}
    if len(json.dumps(visible).encode()) > 30000:
        raise ValueError("visible report exceeds limit")
    row: Event = {"request": request, "visible": visible, "full": full,
                  "source_files": source_files(project)}
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    return visible


def codex_command(codex: Path, session: Path, model: str, effort: str, tier: str) -> list[str]:
    return [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
            "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
            "--add-dir", str(session), "--model", model,
            "--config", f'model_reasoning_effort="{effort}"',
            "--config", f'service_tier="{tier}"', "--cd", str(session / "project"),
            "--output-last-message", str(session / "codex-final.txt"), "-"]


def run(session: Path, codex: Path, model: str, effort: str, tier: str,
        timeout: int) -> dict[str, Any]:
    config(session)
    if any((session / name).exists() for name in
           ("events.jsonl", "codex-events.jsonl", "codex-run.json")):
        raise ValueError("live session must be fresh")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    command = codex_command(codex, session, model, effort, tier)
    started, timed_out = time.time(), False
    with (session / "codex-events.jsonl").open("wb") as stdout, (
            session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            exit_code, timed_out = 124, True
    record = {
        "schema": "fr-application-migration-agent-run-1",
        "codex_version": version,
        "model": model,
        "reasoning_effort": effort,
        "service_tier": tier,
        "ephemeral": True,
        "ignored_user_config": True,
        "ignored_rules": True,
        "sandbox": "workspace-write",
        "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
        "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
        "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
        "elapsed_seconds": time.time() - started,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "command": command,
    }
    save(session / "codex-run.json", record)
    return record


def independent_oracle(project: Path, original: dict[str, str], patch: Path) -> dict[str, Any]:
    current = source_files(project)
    changed = sorted(name for name in set(original) | set(current)
                     if original.get(name) != current.get(name))
    with tempfile.TemporaryDirectory(prefix="fr-application-migration-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        populate(receiver)
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver,
                                 capture_output=True, timeout=30)
        generated = receiver / DESTINATION
        replay = applied.returncode == 0 and generated.is_file()
        source_preserved = (receiver / SOURCE).read_text() == SOURCE_TEXT
        behavior = False
        detail = (applied.stdout + applied.stderr).decode(errors="replace")[-2048:]
        if replay:
            (receiver / "generated/routes_test.go").write_text(ORACLE_TEST)
            environment = os.environ.copy()
            environment["GOCACHE"] = str(Path(temporary) / "go-cache")
            tested = subprocess.run(["go", "test", "-v", "./..."], cwd=receiver,
                                    env=environment, capture_output=True, timeout=120)
            detail = (tested.stdout + tested.stderr).decode(errors="replace")[-4096:]
            behavior = tested.returncode == 0 and "--- PASS: TestMigratedContract" in detail
        return {
            "changed_files": changed,
            "source_preserved": source_preserved,
            "receiver_patch_replay": replay,
            "go_http_7_cases": behavior,
            "oracle_detail": detail,
        }


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    run_record = object_map(json.loads((session / "codex-run.json").read_text()), "run")
    codex_rows = [json.loads(line) for line in
                  (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows)
                  if row.get("type") == "turn.completed"), None)
    command_prefix = str(ROOT / "tools/application-migration-agent.py") + " step "
    direct = [item["command"] for item in commands
              if command_prefix not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    guide = object_map(rows[0]["full"] if rows else {}, "guide")
    preview = object_map(rows[1]["full"] if len(rows) > 1 else {}, "preview")
    review = object_map(rows[2]["full"] if len(rows) > 2 else {}, "review")
    execution = object_map(rows[3]["full"] if len(rows) > 3 else {}, "execution")
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[4]["request"].get("answer", {}) if len(rows) > 4 else {}
    patch = session / "project" / PATCH
    oracle = independent_oracle(session / "project", selected["source_files"], patch) \
        if patch.is_file() else {}
    expected_answer = {
        "source": SOURCE,
        "destination": DESTINATION,
        "method": "POST",
        "path": "/records/{id}",
        "status": 201,
        "inputs": ["path:id", "query:limit:integer", "json-body:visible:boolean"],
        "source_preserved": True,
        "checks": ["go"],
    }
    migration = preview.get("migration", {}) if preview else {}
    endpoints = migration.get("endpoints", [])
    result = {
        "schema": "fr-application-migration-agent-result-1",
        "passed": False,
        "sequence": sequence,
        "guide_route": guide.get("route", {}).get("id") if guide else None,
        "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
        "review_ready": review.get("ready") if review else None,
        "execution_passed": execution.get("passed") if execution else None,
        "stages": [(item.get("stage"), item.get("status")) for item in stages],
        "oracle": oracle,
        "answer": answer,
        "codex": {
            "usage": usage,
            "command_executions": len(commands),
            "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
            "direct_project_commands": direct,
            "billed_quota": {"available": False, "value": None},
        },
        "manual_corrections": selected["manual_corrections"],
    }
    result["passed"] = (
        run_record["exit_code"] == 0 and not run_record["timed_out"]
        and sequence == list(STEPS)
        and result["guide_route"] == "framework-migration"
        and guide.get("state") == "needs-authoring"
        and guide.get("route", {}).get("source_required") is False
        and guide.get("route", {}).get("evidence", {}).get("planner") == "application-ir"
        and preview.get("applied") is False
        and migration.get("source_kind") == "project-snapshot"
        and migration.get("target") == "go-net-http"
        and migration.get("manual_boundaries") == 0
        and migration.get("coexistence", {}).get("source_preserved") is True
        and len(endpoints) == 1
        and preview.get("diff") == review.get("diff")
        and review.get("ready") is True
        and review.get("checks", {}).get("names") == list(CHECK_NAMES)
        and "+++ b/generated/routes.go" in review.get("diff", "")
        and execution.get("passed") is True
        and [item.get("stage") for item in stages] == expected_stages
        and all(item.get("status") == "passed" for item in stages)
        and answer == expected_answer
        and oracle.get("changed_files") == [DESTINATION]
        and oracle.get("source_preserved") is True
        and oracle.get("receiver_patch_replay") is True
        and oracle.get("go_http_7_cases") is True
        and len(commands) == len(STEPS) and not direct
        and result["codex"]["failed_command_executions"] == 0
        and isinstance(usage, dict)
        and selected["manual_corrections"] == 0
    )
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool,
           reason: str | None) -> dict[str, Any]:
    result = object_map(json.loads((session / "result.json").read_text()), "result")
    if not result["passed"] and not diagnostic:
        raise ValueError("failed trial can only be recorded as diagnostic")
    if diagnostic and not reason:
        raise ValueError("diagnostic retention requires --reason")
    destination.mkdir(parents=True, exist_ok=False)
    files: dict[str, str] = {}
    for name in FILES:
        if (session / name).exists():
            shutil.copyfile(session / name, destination / name)
            files[name] = sha((destination / name).read_bytes())
    run_record = json.loads((session / "codex-run.json").read_text())
    manifest = {
        "schema": "fr-application-migration-agent-manifest-1",
        "passed": result["passed"],
        "acceptance_evidence": result["passed"] and not diagnostic,
        "diagnostic_reason": reason if diagnostic else None,
        "fixture_revision": fixture_revision(),
        "model": run_record["model"],
        "evaluator_sha256": evaluator_hash(),
        "files": files,
    }
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, Any]:
    manifest = object_map(json.loads((destination / "manifest.json").read_text()), "manifest")
    for name, expected in object_map(manifest["files"], "files").items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result = json.loads((destination / "result.json").read_text())
    if (manifest["passed"] != result["passed"]
            or manifest["acceptance_evidence"] and not result["passed"]):
        raise ValueError("retained outcome classification disagrees with score")
    return {"verified": True, "passed": manifest["passed"],
            "acceptance_evidence": manifest["acceptance_evidence"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    prepared = sub.add_parser("prepare")
    prepared.add_argument("session", type=Path)
    prepared.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    stepped = sub.add_parser("step")
    stepped.add_argument("session", type=Path)
    stepped.add_argument("--request-stdin", action="store_true")
    running = sub.add_parser("run")
    running.add_argument("session", type=Path)
    running.add_argument("--codex", type=Path, default=Path("codex"))
    running.add_argument("--model", default=MODEL)
    running.add_argument("--effort", default=EFFORT)
    running.add_argument("--service-tier", default=TIER)
    running.add_argument("--timeout", type=int, default=1800)
    running.add_argument("--confirm-agent-spend", action="store_true")
    scored = sub.add_parser("score")
    scored.add_argument("session", type=Path)
    recorded = sub.add_parser("record")
    recorded.add_argument("session", type=Path)
    recorded.add_argument("destination", type=Path)
    recorded.add_argument("--diagnostic", action="store_true")
    recorded.add_argument("--reason")
    audited = sub.add_parser("audit")
    audited.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.action == "prepare":
            result = prepare(args.session.resolve(), args.fr.resolve())
        elif args.action == "step":
            if not args.request_stdin:
                raise ValueError("step requires --request-stdin")
            result = step(args.session.resolve(), object_map(json.load(sys.stdin), "tool request"))
        elif args.action == "run":
            if not args.confirm_agent_spend:
                raise ValueError("--confirm-agent-spend is required")
            result = run(args.session.resolve(), args.codex, args.model, args.effort,
                         args.service_tier, args.timeout)
        elif args.action == "score":
            result = score(args.session.resolve())
        elif args.action == "record":
            result = record(args.session.resolve(), args.destination.resolve(),
                            args.diagnostic, args.reason)
        else:
            result = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, RuntimeError, json.JSONDecodeError,
            subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
