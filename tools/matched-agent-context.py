#!/usr/bin/env python3
"""Run and retain a matched SDK-versus-files Codex context comparison."""

from __future__ import annotations

import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
MODEL = "gpt-5.6-luna"
EFFORT = "low"
SERVICE_TIER = "default"
ARMS = ("fr", "files")
WORKFLOWS = (
    "understanding",
    "tracing",
    "direct-change",
    "recipe",
    "semantic-edit",
    "framework-migration",
    "proof",
)
ROUTES = (
    "evidence",
    "evidence",
    "direct-capability",
    "recipe",
    "semantic-scalar",
    "framework-migration",
    "proof",
)
RETAINED_FILES = (
    "session.json",
    "prompt.txt",
    "events.jsonl",
    "codex-events.jsonl",
    "codex-stderr.txt",
    "codex-final.txt",
    "codex-run.json",
    "result.json",
)


def completion_module():
    spec = importlib.util.spec_from_file_location(
        "completion_workflows", ROOT / "tools/completion-workflows.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CW = completion_module()


def canonical(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False
    ).encode()


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def source_bindings() -> dict[str, str]:
    paths = (
        "tools/matched-agent-context.py",
        "tools/completion-workflows.py",
        "sdk/python/src/fr_ir/guide.py",
        "sdk/python/src/fr_ir/runtime.py",
        "src/project/agent_guide.rs",
        "kernels/FrKernels/AgentGuide.lean",
    )
    return {name: digest((ROOT / name).read_bytes()) for name in paths}


def expected_answers(proof_path: str, obligation: str) -> dict[str, object]:
    return {
        "schema": "fr-matched-outcomes-1",
        "workflows": [
            {"id": "understanding", "path": "src/lib.rs", "name": "calculate", "kind": "function", "language": "rust"},
            {"id": "tracing", "path": "src/lib.rs", "name": "calculate", "direct_callers": 0},
            {"id": "direct-change", "path": "src/lib.rs", "from": "calculate", "to": "compute", "changed_files": 1},
            {"id": "recipe", "verb": "rename", "from": "calculate", "to": "compute", "matched": 1, "changed_files": 1, "refusals": 0},
            {"id": "semantic-edit", "path": "src/lib.rs", "operation": "set-int", "from": "7", "to": "9", "changed_files": 1},
            {"id": "framework-migration", "source": "app/api/signals/route.ts", "target": "fastapi", "destination": "converted/signals.py", "method": "GET", "path": "/api/signals", "response": {"healthy": True}},
            {"id": "proof", "path": proof_path, "obligation": obligation, "tactics": ["rfl"], "accepted": True},
        ],
    }


def common_prompt(proof_path: str, obligation: str) -> str:
    return f"""Complete the same seven read/preview workflows in order on this unfamiliar project:
1. Understand the declaration named calculate.
2. Trace the declaration named calculate and count its direct callers.
3. Preview renaming calculate to compute with a direct capability.
4. Preview the same rename as a schema-1 recipe matching exactly one declaration, one changed file and zero refusals.
5. Preview changing integer 7 to 9 in calculate with semantic set-int.
6. Preview app/api/signals/route.ts as FastAPI at converted/signals.py; preserve method, route and literal JSON response.
7. Check obligation {obligation} in {proof_path} with agent-authored tactics `rfl`.

Finish with one `fr-matched-outcomes-1` packet containing all seven workflow rows. Use strings for
the scalar values 7 and 9. The instrumented tool validates the packet independently. Every operation
must remain a preview and project source must stay unchanged.

Use these exact row fields: understanding has `id,path,name,kind,language`; tracing has
`id,path,name,direct_callers`; direct-change has `id,path,from,to,changed_files`; recipe has
`id,verb,from,to,matched,changed_files,refusals`; semantic-edit has
`id,path,operation,from,to,changed_files`; framework-migration has
`id,source,target,destination,method,path,response`; proof has
`id,path,obligation,tactics,accepted`. Preserve this workflow order and add no fields.
"""


def sdk_example(proof_path: str, obligation: str) -> str:
    return f'''import json
import sys
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector, GuideFile, GuideInputs
from fr_ir.runtime import FrClient
client = FrClient(sys.argv[1], executable=sys.argv[2])
target = GoalSelector(name="calculate")
client.complete_guide(AgentGoal("understand", selector=target))
client.complete_guide(AgentGoal("trace", selector=target))
client.complete_guide(AgentGoal("change", selector=target, operation=GoalOperation(
    "capability", {{"capability":"rename","parameters":{{"new_name":"compute"}}}},
)))
recipe = "\\n".join([
    "schema 1", "recipe rename-calculate {{",
    "  rename to \\"compute\\" where name=\\"calculate\\"",
    "  expect matched = 1", "  expect changed = 1 files",
    "  expect refusals = 0", "}}", "",
])
client.complete_guide(AgentGoal("change", selector=target,
    operation=GoalOperation("recipe", {{"verb":"rename"}})), {{
    0: GuideInputs({{"recipe-file": GuideFile("rename.recipe", recipe)}}),
}})
client.complete_guide(AgentGoal("change", selector=target,
    operation=GoalOperation("semantic-scalar", {{
        "operation":"set-int", "from":"7", "to":"9",
    }})))
migration_goal = AgentGoal("migrate",
    selector=GoalSelector(path="app/api/signals/route.ts"),
    operation=GoalOperation("framework-migration", {{"to":"fastapi"}}))
migration = client.guide(migration_goal)
feature = migration.at("/route/evidence/compatible_features")[0]
client.follow_guide(migration.actions()[0])
client.follow_guide(migration.actions()[1], GuideInputs({{
    "feature-id": feature, "destination":"converted/signals.py",
}}))
tactics = GuideFile("proof.lean", "rfl\\n")
client.complete_guide(AgentGoal("prove",
    selector=GoalSelector(path={json.dumps(proof_path)}),
    operation=GoalOperation("proof", {{"obligation":{json.dumps(obligation)}}})), {{
    1: GuideInputs({{"tactics-file": tactics}}),
    2: GuideInputs({{"tactics-file": tactics}}),
}})
'''


def prompt(session: Path, arm: str, proof_path: str, obligation: str) -> str:
    common = common_prompt(proof_path, obligation)
    packet = json.dumps(expected_answers(proof_path, obligation), separators=(",", ":"))
    command = f"python3 {ROOT / 'tools/matched-agent-context.py'} step {session} --request-stdin"
    if arm == "fr":
        surface = f"""Use one agent-authored Python SDK program so intermediate guides and reports stay local.
Submit it once as `{{"tool":"sdk","program_lines":[...],"answers":PACKET}}`, then call finish. The program receives
the project root as `sys.argv[1]` and an instrumented fr executable as `sys.argv[2]`. Import only
`json`, `sys`, `FrClient`, and IR-shaped goal classes from `fr_ir.guide`. Use `client.complete_guide`
for routes whose inputs are known. For migration, obtain one guide, take the first compatible feature
ID from `/route/evidence/compatible_features`, then follow both actions with `GuideInputs`. Supply
recipe and tactics text with `GuideFile`; the agent authors both. Print only the final outcome packet
as JSON. Do not call `client.call`, execute a write, read source through Python, or use project/file APIs.

The recipe lines are: `schema 1`, `recipe rename-calculate {{`, `  rename to "compute" where
name="calculate"`, `  expect matched = 1`, `  expect changed = 1 files`,
`  expect refusals = 0`, `}}`. Build goals with `AgentGoal`, `GoalSelector`, `GoalOperation`,
`GuideInputs`, and `GuideFile`. `complete_guide` returns a guide and reports; the migration guide's
actions are followed with `client.follow_guide`. Recipe input belongs to action 0. Migration inputs
belong to action 1. The proof tactics file belongs unchanged to actions 1 and 2.

Use this complete tested program shape. Preserve the constructor names, field types, operation
fields and indexed input maps. You may reformat it, but do not add fields or calls:
```python
{sdk_example(proof_path, obligation)}
```
Do not put the final packet inside the Python source. Put it in the SDK request's separate `answers`
field so JSON transport cannot change the program.
"""
    else:
        surface = """Use only the instrumented `files`, `read`, `search`, `submit`, and `finish` tools.
Read the relevant Rust, route and Lean files directly. Do not invoke fr, Git, a compiler, a shell
search, or Python file APIs. Submit the complete outcome packet once with
`{"tool":"submit","answers":PACKET}`, then finish. The scorer requires reads of src/lib.rs,
app/api/signals/route.ts and the named Lean proof file.
There are no per-workflow tools. `search` accepts `pattern`, not `query`; it is optional.
"""
    return f"""You are an independent matched-context evaluation agent.

{common}
{surface}
Use only the instrumented tool through functions.exec / tools.exec_command. Every call uses this
complete stdin command and one JSON object:
{command} <<'FRJSON'
{{"tool":"finish","summary":"all seven workflows completed"}}
FRJSON
No human correction is available. Do not browse, delegate, inspect the evaluator, or access another
session. Do not repeat a successful request. The harness records complete prompts, requests,
responses and Codex usage; the CLI exposes no billed-quota value.

The exact common final packet is below. Print it from the SDK program or pass it as `answers` in the
direct-file arm. Do not change its envelope, row IDs, arrays, objects or values:
{packet}
"""


WRAPPER = r'''#!/usr/bin/env python3
import hashlib, json, pathlib, subprocess, sys, time
session = pathlib.Path(__file__).resolve().parent
config = json.loads((session / "session.json").read_text())
binary = pathlib.Path(config["binary"])
if hashlib.sha256(binary.read_bytes()).hexdigest() != config["binary_sha256"]:
    raise SystemExit("frozen fr binary changed")
argv = sys.argv[1:]
if len(argv) < 4 or argv[0] != "--json" or argv[1] != "-C" or pathlib.Path(argv[2]).resolve() != (session / "project").resolve():
    raise SystemExit("SDK call left the instrumented project")
arguments = argv[3:]
if not arguments or any(value.split("=", 1)[0] in ("--write", "--save-plan") for value in arguments):
    raise SystemExit("SDK comparison admits previews only")
if arguments[0] not in ("guide", "intent", "rename", "recipe", "project", "author", "migrate", "spec"):
    raise SystemExit("SDK comparison command is outside the guided surface")
input_bytes = sys.stdin.buffer.read(65537)
if len(input_bytes) > 65536:
    raise SystemExit("SDK input exceeds 64 KiB")
started = time.time()
completed = subprocess.run([str(binary), "--no-cache", *argv], input=input_bytes, capture_output=True, timeout=180)
try:
    response = json.loads(completed.stdout)
except Exception:
    response = None
event = {"arguments": arguments, "request_bytes": len(input_bytes), "response_bytes": len(completed.stdout),
         "exit_code": completed.returncode, "elapsed_seconds": time.time() - started,
         "response": response, "stderr": completed.stderr[:4096].decode(errors="replace")}
with (session / "sdk-events.jsonl").open("a") as stream:
    stream.write(json.dumps(event, ensure_ascii=False) + "\n")
sys.stdout.buffer.write(completed.stdout)
sys.stderr.buffer.write(completed.stderr)
raise SystemExit(completed.returncode)
'''


def prepare(out: Path, executable: Path) -> dict[str, object]:
    if out.exists():
        raise ValueError("matched context directory already exists")
    out.mkdir(parents=True)
    frozen = out / "fr-matched-agent-bin"
    shutil.copyfile(executable, frozen)
    frozen.chmod(0o555)
    base = out / "base"
    base.mkdir()
    proof_path, obligation = CW.prepare(frozen, base)
    answers = expected_answers(proof_path, obligation)
    sessions = []
    for arm in ARMS:
        session = out / arm
        session.mkdir()
        project = session / "project"
        shutil.copytree(base, project, ignore=shutil.ignore_patterns(".fr-history"))
        (session / "artifacts").mkdir()
        wrapper = session / "fr-sdk-wrapper.py"
        wrapper.write_text(WRAPPER)
        wrapper.chmod(0o555)
        config = {
            "schema": "fr-matched-agent-session-1",
            "arm": arm,
            "binary": str(frozen),
            "binary_sha256": digest(frozen.read_bytes()),
            "wrapper": str(wrapper),
            "proof_path": proof_path,
            "obligation": obligation,
            "expected_answers": answers,
            "source_bindings": source_bindings(),
            "original_source": CW.source_snapshot(project),
            "manual_corrections": 0,
        }
        save(session / "session.json", config)
        (session / "prompt.txt").write_text(prompt(session, arm, proof_path, obligation))
        sessions.append(str(session))
    shutil.rmtree(base)
    experiment = {
        "schema": "fr-matched-agent-experiment-1",
        "arms": list(ARMS),
        "sessions": sessions,
        "workflow_ids": list(WORKFLOWS),
        "model": MODEL,
        "reasoning_effort": EFFORT,
        "service_tier": SERVICE_TIER,
    }
    save(out / "experiment.json", experiment)
    return experiment


def config(session: Path) -> dict[str, object]:
    value = json.loads((session / "session.json").read_text())
    if value.get("schema") != "fr-matched-agent-session-1":
        raise ValueError("matched agent session schema is invalid")
    if value.get("source_bindings") != source_bindings():
        raise ValueError("matched agent session source bindings are stale")
    binary = Path(value["binary"])
    if digest(binary.read_bytes()) != value["binary_sha256"]:
        raise ValueError("matched agent binary changed after preparation")
    return value


def event_rows(session: Path, name: str = "events.jsonl") -> list[dict[str, object]]:
    path = session / name
    return [json.loads(line) for line in path.read_text().splitlines() if line] if path.exists() else []


def append_event(session: Path, event: dict[str, object]) -> None:
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(event, ensure_ascii=False) + "\n")


def within(root: Path, name: str) -> Path:
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("path leaves the matched project")
    return path


def validate_program(source: str) -> None:
    try:
        tree = ast.parse(source)
    except SyntaxError as error:
        raise ValueError(f"SDK program is invalid Python: {error}") from None
    allowed_imports = {"json", "sys"}
    allowed_from_imports = {
        "fr_ir.guide": {"AgentGoal", "GoalOperation", "GoalSelector", "GuideFile", "GuideInputs"},
        "fr_ir.runtime": {"FrClient"},
    }
    denied_attributes = {
        "call", "execute", "execute_intent", "project", "disclose", "context", "compile",
        "prepare", "read_text", "read_bytes", "write_text", "write_bytes", "open", "system",
        "popen", "run",
    }
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            if any(alias.name not in allowed_imports for alias in node.names):
                raise ValueError("SDK program imports outside the bounded guide surface")
        if isinstance(node, ast.ImportFrom):
            admitted = allowed_from_imports.get(node.module or "")
            if node.level or admitted is None or any(alias.name not in admitted for alias in node.names):
                raise ValueError("SDK program imports outside the bounded guide surface")
        if isinstance(node, ast.Attribute) and (node.attr.startswith("__") or node.attr in denied_attributes):
            raise ValueError("SDK program calls outside the bounded guide surface")
        if isinstance(node, ast.Name) and node.id in {
            "open", "exec", "eval", "compile", "__import__", "getattr", "setattr",
            "delattr", "globals", "locals", "vars", "input", "breakpoint",
        }:
            raise ValueError("SDK program uses a forbidden Python primitive")


def read_file(project: Path, request: dict[str, object]) -> dict[str, object]:
    path = within(project, request.get("path", ""))
    if not path.is_file():
        raise ValueError("read needs one regular project file")
    start, count = request.get("start", 1), request.get("lines", 80)
    if type(start) is not int or type(count) is not int or start < 1 or not 1 <= count <= 200:
        raise ValueError("read requires a positive start and 1 through 200 lines")
    lines = path.read_text().splitlines(keepends=True)
    text = "".join(
        f"{index + 1}: {line}" for index, line in enumerate(lines)
        if start - 1 <= index < start - 1 + count
    )
    return {"path": str(path.relative_to(project)), "text": text[:20_000], "total_lines": len(lines),
            "next_line": start + count if start + count <= len(lines) else None}


def action(session: Path, request: dict[str, object]) -> dict[str, object]:
    selected = config(session)
    project = session / "project"
    arm = selected["arm"]
    tool = request.get("tool")
    if tool == "sdk":
        if arm != "fr":
            raise ValueError("sdk belongs to the fr arm")
        prior = [(row["request"], json.loads(row["visible"])) for row in event_rows(session)]
        if any(request.get("tool") == "sdk" and visible.get("exit_code") == 0
               and visible.get("answers") == selected["expected_answers"]
               for request, visible in prior):
            raise ValueError("a successful SDK program runs exactly once")
        lines = request.get("program_lines")
        if (not isinstance(lines, list) or not 1 <= len(lines) <= 400
                or not all(isinstance(line, str) for line in lines)):
            raise ValueError("sdk needs 1 through 400 program lines")
        source = "\n".join(lines) + "\n"
        if len(source.encode()) > 65_536:
            raise ValueError("SDK program exceeds 64 KiB")
        validate_program(source)
        program = session / "artifacts/agent.py"
        program.write_text(source)
        environment = os.environ.copy()
        environment["PYTHONPATH"] = str(ROOT / "sdk/python/src")
        completed = subprocess.run(
            [sys.executable, str(program), str(project), selected["wrapper"]],
            cwd=project, env=environment, capture_output=True, timeout=600,
        )
        stdout = completed.stdout[:65_536]
        submitted = request.get("answers")
        if submitted is not None:
            packet = submitted
        else:
            try:
                packet = json.loads(stdout)
            except (UnicodeDecodeError, json.JSONDecodeError):
                packet = None
        return {"exit_code": completed.returncode, "answers": packet,
                "stdout_bytes": len(completed.stdout),
                "stdout_omitted_bytes": max(0, len(completed.stdout) - len(stdout)),
                "stderr": completed.stderr[:4096].decode(errors="replace"),
                "stderr_omitted_bytes": max(0, len(completed.stderr) - 4096)}
    if tool in ("files", "read", "search"):
        if arm != "files":
            raise ValueError("ordinary project inspection belongs to the files arm")
        if tool == "files":
            root = within(project, request.get("path", "."))
            paths = sorted(str(path.relative_to(project)) for path in root.rglob("*") if path.is_file())
            return {"paths": paths[:200], "omitted": max(0, len(paths) - 200)}
        if tool == "read":
            return read_file(project, request)
        pattern = request.get("pattern")
        if not isinstance(pattern, str) or not pattern or len(pattern.encode()) > 512:
            raise ValueError("search needs a bounded nonempty literal pattern")
        root = within(project, request.get("path", "."))
        rows = []
        for path in sorted(root.rglob("*") if root.is_dir() else [root]):
            if not path.is_file():
                continue
            for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                if pattern in line:
                    rows.append({"path": str(path.relative_to(project)), "line": number,
                                 "text": line[:1000]})
                    if len(rows) == 100:
                        return {"matches": rows, "truncated": True}
        return {"matches": rows, "truncated": False}
    if tool == "submit":
        if arm != "files":
            raise ValueError("submit belongs to the direct-file arm")
        answers = request.get("answers")
        return {"accepted": answers == selected["expected_answers"], "answers": answers}
    if tool == "finish":
        summary = request.get("summary")
        if not isinstance(summary, str) or not summary or len(summary.encode()) > 4096:
            raise ValueError("finish needs one bounded summary")
        return {"finished": True, "summary": summary}
    raise ValueError("unknown matched-agent tool")


def step(session: Path, request: dict[str, object]) -> dict[str, object]:
    before = CW.source_snapshot(session / "project")
    started = time.time()
    try:
        result = action(session, request)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        result = {"error": str(error), "exit_code": 1}
    after = CW.source_snapshot(session / "project")
    visible = json.dumps(result, ensure_ascii=False)
    append_event(session, {"request": request, "visible": visible,
                           "request_bytes": len(canonical(request)),
                           "response_bytes": len(visible.encode()),
                           "source_before": digest(canonical(before)),
                           "source_after": digest(canonical(after)),
                           "elapsed_seconds": time.time() - started})
    return result


def codex_command(codex: Path, session: Path, model: str, effort: str, tier: str) -> list[str]:
    return [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
            "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
            "--add-dir", str(session), "--model", model, "--config",
            f'model_reasoning_effort="{effort}"', "--config", f'service_tier="{tier}"',
            "--cd", str(session / "project"), "--output-last-message",
            str(session / "codex-final.txt"), "-"]


def run_agent(codex: Path, session: Path, model: str, effort: str, tier: str,
              timeout: int, version: str) -> dict[str, object]:
    for name in ("events.jsonl", "sdk-events.jsonl", "codex-events.jsonl", "codex-run.json"):
        if (session / name).exists():
            raise ValueError(f"matched session is not fresh: {session / name}")
    command = codex_command(codex, session, model, effort, tier)
    started = time.time()
    timed_out = False
    with (session / "codex-events.jsonl").open("wb") as stdout, (session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            timed_out, exit_code = True, 124
    record = {"schema": "fr-matched-agent-run-1", "arm": session.name,
              "codex_version": version, "model": model, "reasoning_effort": effort,
              "service_tier": tier, "ephemeral": True, "ignored_user_config": True,
              "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": digest((session / "prompt.txt").read_bytes()),
              "events_sha256": digest((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": digest((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def codex_observation(session: Path) -> dict[str, object]:
    rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    usage = next((row["usage"] for row in reversed(rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    allowed = str(ROOT / "tools/matched-agent-context.py")
    direct = [item["command"] for item in commands if allowed not in item["command"] or " step " not in item["command"]]
    stderr = (session / "codex-stderr.txt").read_text()
    return {"usage": usage, "command_executions": len(commands),
            "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
            "direct_project_commands": direct,
            "infrastructure_errors": sum(" ERROR " in line for line in stderr.splitlines()),
            "stderr_bytes": len(stderr.encode()),
            "event_bytes": (session / "codex-events.jsonl").stat().st_size,
            "final_bytes": (session / "codex-final.txt").stat().st_size,
            "billed_quota": {"available": False, "value": None}}


def sdk_coverage(session: Path) -> dict[str, object]:
    rows = event_rows(session, "sdk-events.jsonl")
    routes = [row["response"]["route"]["id"] for row in rows
              if row.get("arguments", [])[:1] == ["guide"] and isinstance(row.get("response"), dict)
              and isinstance(row["response"].get("route"), dict)]
    commands = [row.get("arguments", []) for row in rows if row.get("exit_code") == 0]
    required = [
        lambda args: args[:1] == ["intent"],
        lambda args: args[:1] == ["rename"],
        lambda args: args[:1] == ["recipe"],
        lambda args: args[:2] == ["project", "semantic"],
        lambda args: args[:2] == ["author", "edit-body-scalar"],
        lambda args: args[:2] == ["project", "features"],
        lambda args: args[:2] == ["migrate", "feature"],
        lambda args: args[:2] == ["spec", "proof-task"],
        lambda args: args[:2] == ["spec", "proof-check"],
        lambda args: args[:2] == ["spec", "prove"],
    ]
    return {"calls": len(rows), "failed_calls": sum(row.get("exit_code") != 0 for row in rows),
            "observed_routes": routes, "route_coverage": all(route in routes for route in ROUTES),
            "action_coverage": all(any(check(args) for args in commands) for check in required),
            "request_bytes": sum(row.get("request_bytes", 0) for row in rows),
            "response_bytes": sum(row.get("response_bytes", 0) for row in rows)}


def score(session: Path) -> dict[str, object]:
    selected = config(session)
    rows = event_rows(session)
    requests = [row["request"] for row in rows]
    visible = [json.loads(row["visible"]) for row in rows]
    final = visible[-1] if visible else {}
    source = CW.source_snapshot(session / "project")
    observation = codex_observation(session)
    run = json.loads((session / "codex-run.json").read_text())
    answers = None
    if selected["arm"] == "fr":
        sdk_results = [value for request, value in zip(requests, visible) if request.get("tool") == "sdk"]
        answers = sdk_results[0].get("answers") if len(sdk_results) == 1 else None
        coverage = sdk_coverage(session)
        arm_ok = (len(sdk_results) == 1 and sdk_results[0].get("exit_code") == 0
                  and coverage["failed_calls"] == 0 and coverage["route_coverage"]
                  and coverage["action_coverage"])
    else:
        submissions = [value for request, value in zip(requests, visible) if request.get("tool") == "submit"]
        answers = submissions[0].get("answers") if len(submissions) == 1 else None
        reads = {request.get("path") for request in requests if request.get("tool") == "read"}
        needed = {"src/lib.rs", "app/api/signals/route.ts", selected["proof_path"]}
        coverage = {"reads": sorted(value for value in reads if isinstance(value, str)),
                    "required_reads": sorted(needed), "read_coverage": needed <= reads}
        arm_ok = len(submissions) == 1 and submissions[0].get("accepted") is True and needed <= reads
    result = {"schema": "fr-matched-agent-result-1", "arm": selected["arm"],
              "workflow_ids": list(WORKFLOWS), "answers_match": answers == selected["expected_answers"],
              "coverage": coverage, "finished": final.get("finished") is True,
              "source_unchanged": source == selected["original_source"],
              "prompt_bytes": (session / "prompt.txt").stat().st_size,
              "tool_calls": len(rows), "tool_request_bytes": sum(row["request_bytes"] for row in rows),
              "tool_response_bytes": sum(row["response_bytes"] for row in rows),
              "tool_errors": sum("error" in value or value.get("exit_code") not in (None, 0)
                                 for value in visible),
              "codex": observation, "manual_corrections": selected["manual_corrections"],
              "measurement_scope": "Complete prompt, instrumented requests/responses and Codex JSONL usage; billed quota unavailable."}
    result["passed"] = (run["exit_code"] == 0 and not run["timed_out"] and arm_ok
                        and result["answers_match"] and result["finished"] and result["source_unchanged"]
                        and result["tool_errors"] == 0
                        and not observation["direct_project_commands"]
                        and observation["failed_command_executions"] == 0
                        and observation["infrastructure_errors"] == 0
                        and selected["manual_corrections"] == 0)
    save(session / "result.json", result)
    return result


def comparison(results: dict[str, dict[str, object]]) -> dict[str, object]:
    usage = {arm: results[arm]["codex"]["usage"] or {} for arm in ARMS}
    metrics = ("input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens")
    differences = {name: usage["fr"].get(name, 0) - usage["files"].get(name, 0) for name in metrics}
    return {"usage_difference_fr_minus_files": differences,
            "tool_call_difference_fr_minus_files": results["fr"]["tool_calls"] - results["files"]["tool_calls"],
            "context_saving_observed": differences["input_tokens"] < 0}


def score_all(directory: Path) -> dict[str, object]:
    experiment = json.loads((directory / "experiment.json").read_text())
    results = {arm: score(directory / arm) for arm in ARMS}
    report = {"schema": "fr-matched-agent-manifest-1", "model": experiment["model"],
              "reasoning_effort": experiment["reasoning_effort"],
              "service_tier": experiment["service_tier"], "workflow_families": 7,
              "results": [results[arm] for arm in ARMS],
              "passed": all(results[arm]["passed"] for arm in ARMS),
              "claim": "One matched pair over seven fixed preview workflows; no population, hidden-reasoning or billed-quota claim."}
    report.update(comparison(results))
    save(directory / "manifest.json", report)
    return report


def record(directory: Path, destination: Path, diagnostic: bool = False) -> dict[str, object]:
    report = json.loads((directory / "manifest.json").read_text())
    if not report.get("passed") and not diagnostic:
        raise ValueError("only a passing matched cohort can become acceptance evidence")
    if destination.exists():
        raise ValueError("matched cohort destination already exists")
    destination.mkdir(parents=True)
    files = {}
    for name in ("experiment.json", "manifest.json"):
        shutil.copyfile(directory / name, destination / name)
        files[name] = digest((destination / name).read_bytes())
    for arm in ARMS:
        target = destination / arm
        target.mkdir()
        names = list(RETAINED_FILES)
        if arm == "fr":
            names.append("sdk-events.jsonl")
        for name in names:
            if not (directory / arm / name).is_file():
                continue
            shutil.copyfile(directory / arm / name, target / name)
            files[f"{arm}/{name}"] = digest((target / name).read_bytes())
    report["acceptance_evidence"] = bool(report.get("passed")) and not diagnostic
    report["files"] = files
    save(destination / "manifest.json", report)
    return report


def replay(directory: Path) -> dict[str, object]:
    report = json.loads((directory / "manifest.json").read_text())
    for name, expected in report.get("files", {}).items():
        if name != "manifest.json" and digest((directory / name).read_bytes()) != expected:
            raise ValueError(f"retained matched-agent file changed: {name}")
    results = {arm: json.loads((directory / arm / "result.json").read_text()) for arm in ARMS}
    actual = all(value.get("passed") for value in results.values())
    if bool(report.get("passed")) != actual:
        raise ValueError("retained matched-agent outcome is inconsistent")
    calculated = comparison(results)
    for name, value in calculated.items():
        if report.get(name) != value:
            raise ValueError(f"retained matched-agent comparison is inconsistent: {name}")
    return {"verified": True, "passed": actual,
            "acceptance_evidence": bool(report.get("acceptance_evidence")),
            "arms": list(ARMS), "workflow_families": 7,
            "context_saving_observed": bool(report.get("context_saving_observed"))}


def request_from(arguments) -> dict[str, object]:
    if arguments.request_stdin:
        return json.load(sys.stdin)
    if arguments.request is None:
        raise ValueError("step needs a JSON request or --request-stdin")
    return json.loads(arguments.request)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("directory", type=Path)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    step_parser = commands.add_parser("step")
    step_parser.add_argument("session", type=Path)
    step_parser.add_argument("request", nargs="?")
    step_parser.add_argument("--request-stdin", action="store_true")
    run_parser = commands.add_parser("run")
    run_parser.add_argument("directory", type=Path)
    run_parser.add_argument("--codex", type=Path, default=Path("codex"))
    run_parser.add_argument("--model", default=MODEL)
    run_parser.add_argument("--effort", default=EFFORT)
    run_parser.add_argument("--service-tier", default=SERVICE_TIER)
    run_parser.add_argument("--timeout", type=int, default=1800)
    run_parser.add_argument("--confirm-agent-spend", action="store_true")
    score_parser = commands.add_parser("score")
    score_parser.add_argument("directory", type=Path)
    record_parser = commands.add_parser("record")
    record_parser.add_argument("directory", type=Path)
    record_parser.add_argument("destination", type=Path)
    record_parser.add_argument("--diagnostic", action="store_true")
    replay_parser = commands.add_parser("replay")
    replay_parser.add_argument("directory", type=Path)
    arguments = parser.parse_args()
    try:
        if arguments.command == "prepare":
            result = prepare(arguments.directory, arguments.fr.resolve())
        elif arguments.command == "step":
            result = step(arguments.session.resolve(), request_from(arguments))
        elif arguments.command == "run":
            if not arguments.confirm_agent_spend:
                parser.error("--confirm-agent-spend is required for live Codex sessions")
            version = subprocess.check_output([arguments.codex, "--version"], text=True).strip()
            result = {"runs": [run_agent(arguments.codex, arguments.directory / arm,
                                         arguments.model, arguments.effort,
                                         arguments.service_tier, arguments.timeout, version)
                               for arm in ARMS]}
        elif arguments.command == "score":
            result = score_all(arguments.directory)
        elif arguments.command == "record":
            result = record(arguments.directory, arguments.destination, arguments.diagnostic)
        else:
            result = replay(arguments.directory)
    except (OSError, ValueError, KeyError, json.JSONDecodeError, RuntimeError) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
