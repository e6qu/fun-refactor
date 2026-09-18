#!/usr/bin/env python3
"""Run and retain a matched Codex source-writing comparison."""

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
SOURCE = "pub fn calculate(value: i64) -> i64 {\n    return value + 7;\n}\n"
EXPECTED_SOURCE = SOURCE.replace("+ 7", "+ 9")
EXPECTED = {
    "schema": "fr-matched-source-change-1",
    "path": "src/lib.rs",
    "symbol": "calculate",
    "operation": "set-int",
    "from": "7",
    "to": "9",
}
RETAINED = (
    "session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
    "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json",
)


def load_context_harness():
    spec = importlib.util.spec_from_file_location(
        "matched_agent_context", ROOT / "tools/matched-agent-context.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BASE = load_context_harness()


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode()


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def bindings() -> dict[str, str]:
    paths = (
        "tools/matched-agent-source.py", "tools/matched-agent-context.py",
        "sdk/python/src/fr_ir/guide.py", "sdk/python/src/fr_ir/runtime.py",
        "src/project/agent_guide.rs", "src/project/task_change.rs",
        "kernels/FrKernels/AgentGuide.lean",
    )
    return {name: digest((ROOT / name).read_bytes()) for name in paths}


def prepare_project(root: Path) -> None:
    (root / "src").mkdir(parents=True)
    (root / ".fr").mkdir()
    (root / "artifacts").mkdir()
    (root / "src/lib.rs").write_text(SOURCE)
    (root / ".fr/check.py").write_text(
        "import pathlib, subprocess, tempfile\n"
        "source = pathlib.Path('src/lib.rs').read_text()\n"
        "with tempfile.TemporaryDirectory() as directory:\n"
        "    root = pathlib.Path(directory)\n"
        "    text = source + '\\nfn main() { let offset = calculate(0); for value in -16..=16 { "
        "assert_eq!(calculate(value), value + offset); } }\\n'\n"
        "    (root / 'main.rs').write_text(text)\n"
        "    subprocess.run(['rustc', str(root / 'main.rs'), '-o', str(root / 'check')], check=True)\n"
        "    subprocess.run([str(root / 'check')], check=True)\n"
    )
    save(root / ".fr/checks.json", {"schema": 1, "checks": [{
        "name": "compiler", "argv": ["python3", ".fr/check.py"], "cwd": ".",
        "timeout_seconds": 30, "covers": ["Rust compiler", "33 exact behavior cases"],
    }]})


WRAPPER = r'''#!/usr/bin/env python3
import hashlib, json, pathlib, subprocess, sys, time
session = pathlib.Path(__file__).resolve().parent
config = json.loads((session / "session.json").read_text())
binary = pathlib.Path(config["binary"])
if hashlib.sha256(binary.read_bytes()).hexdigest() != config["binary_sha256"]:
    raise SystemExit("frozen fr binary changed")
argv = sys.argv[1:]
project = (session / "project").resolve()
if len(argv) < 4 or argv[0] != "--json" or argv[1] != "-C" or pathlib.Path(argv[2]).resolve() != project:
    raise SystemExit("SDK call left the instrumented project")
arguments = argv[3:]
if not arguments or arguments[0] not in ("guide", "intent"):
    raise SystemExit("SDK call left the guided source-writing surface")
if any(value.split("=", 1)[0] == "--save-plan" for value in arguments):
    raise SystemExit("SDK call requested unrelated persistence")
if "--write" in arguments and arguments[0] != "intent":
    raise SystemExit("only a reviewed guide-bound intent may write")
input_bytes = sys.stdin.buffer.read(65537)
if len(input_bytes) > 65536:
    raise SystemExit("SDK input exceeds 64 KiB")
started = time.time()
completed = subprocess.run([str(binary), "--no-cache", *argv], input=input_bytes,
                           capture_output=True, timeout=180)
try:
    response = json.loads(completed.stdout)
except Exception:
    response = None
event = {"arguments": arguments, "request_bytes": len(input_bytes),
         "response_bytes": len(completed.stdout), "exit_code": completed.returncode,
         "elapsed_seconds": time.time() - started, "response": response,
         "stderr": completed.stderr[:4096].decode(errors="replace")}
with (session / "sdk-events.jsonl").open("a") as stream:
    stream.write(json.dumps(event, ensure_ascii=False) + "\n")
sys.stdout.buffer.write(completed.stdout)
sys.stderr.buffer.write(completed.stderr)
raise SystemExit(completed.returncode)
'''


def sdk_program() -> str:
    return '''from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient
import json, sys
client = FrClient(sys.argv[1], executable=sys.argv[2])
goal = AgentGoal("change", selector=GoalSelector(name="calculate"),
    operation=GoalOperation("semantic-scalar", {"operation":"set-int","from":"7","to":"9"}),
    checks=("compiler",), delivery=TaskDelivery(patch="artifacts/change.patch", check_output_bytes=512))
guide = client.guide(goal)
review = client.review_guide(guide, guide.semantic_scalar_action())
assert "+ 9" in review.at("/diff")
result = client.execute_guide(review)
assert result.passed
print(json.dumps({"schema":"fr-matched-source-change-1","path":"src/lib.rs",
    "symbol":"calculate","operation":"set-int","from":"7","to":"9"}))
'''


def prompt(session: Path, arm: str) -> str:
    command = f"python3 {ROOT / 'tools/matched-agent-source.py'} step {session} --request-stdin"
    common = """Change the integer offset in the Rust function `calculate` from 7 to 9. Preserve its
name, parameter, return type and all other source. Finish with exactly this JSON object:
{"schema":"fr-matched-source-change-1","path":"src/lib.rs","symbol":"calculate","operation":"set-int","from":"7","to":"9"}
"""
    if arm == "fr":
        surface = f"""Use exactly one agent-authored Python SDK program. Submit it with the `sdk` tool
as `{{"tool":"sdk","program":"COMPLETE PYTHON SOURCE"}}`,
then call `finish`. Use the guide and delivery classes shown below. Call `guide`, obtain the exact
typed scalar action, inspect `GuideReview` at `/diff`, and call `execute_guide` only after
that review. Declare check `compiler` and patch `artifacts/change.patch`. Do not read or write source
through Python and do not call the generic `client.call` or `client.execute` methods.

This tested program shape is complete:
```python
{sdk_program()}```
"""
    else:
        surface = """Use `read` once for `src/lib.rs`, then `replace` its exact `+ 7` substring with
`+ 9`. Submit the final JSON object as
`{"tool":"submit","answers":{"schema":"fr-matched-source-change-1","path":"src/lib.rs","symbol":"calculate","operation":"set-int","from":"7","to":"9"}}`,
then call `finish`. Do not use a shell, Git, Python file APIs, or any other project tool.
"""
    return f"""You are an independent matched source-writing evaluation agent.

{common}
{surface}
Use only the instrumented tool through functions.exec / tools.exec_command. Every request uses this
complete stdin command and one JSON object:
{command} <<'FRJSON'
{{"tool":"finish","summary":"source change complete"}}
FRJSON
Do not browse, delegate, inspect the evaluator, or access another session. No human correction is
available. Do not repeat a successful request.
"""


def prepare(out: Path, executable: Path) -> dict[str, object]:
    if out.exists():
        raise ValueError("matched source directory already exists")
    out.mkdir(parents=True)
    frozen = out / "fr-matched-source-bin"
    shutil.copyfile(executable, frozen)
    frozen.chmod(0o555)
    base = out / "base"
    prepare_project(base)
    sessions = []
    for arm in ARMS:
        session = out / arm
        session.mkdir()
        shutil.copytree(base, session / "project")
        (session / "artifacts").mkdir()
        wrapper = session / "fr-sdk-wrapper.py"
        wrapper.write_text(WRAPPER)
        wrapper.chmod(0o555)
        config = {"schema": "fr-matched-source-session-1", "arm": arm,
                  "binary": str(frozen.resolve()), "binary_sha256": digest(frozen.read_bytes()),
                  "wrapper": str(wrapper.resolve()), "expected": EXPECTED,
                  "before_sha256": digest(SOURCE.encode()), "after_sha256": digest(EXPECTED_SOURCE.encode()),
                  "bindings": bindings(), "manual_corrections": 0}
        save(session / "session.json", config)
        (session / "prompt.txt").write_text(prompt(session.resolve(), arm))
        sessions.append(str(session.resolve()))
    shutil.rmtree(base)
    experiment = {"schema": "fr-matched-source-experiment-1", "arms": list(ARMS),
                  "sessions": sessions, "model": MODEL, "reasoning_effort": EFFORT,
                  "service_tier": SERVICE_TIER}
    save(out / "experiment.json", experiment)
    return experiment


def config(session: Path) -> dict[str, object]:
    value = json.loads((session / "session.json").read_text())
    if value.get("schema") != "fr-matched-source-session-1" or value.get("bindings") != bindings():
        raise ValueError("matched source session is invalid or stale")
    binary = Path(value["binary"])
    if digest(binary.read_bytes()) != value["binary_sha256"]:
        raise ValueError("frozen fr binary changed")
    return value


def rows(session: Path, name: str = "events.jsonl") -> list[dict[str, object]]:
    path = session / name
    return [json.loads(line) for line in path.read_text().splitlines() if line] if path.exists() else []


def append(session: Path, event: dict[str, object]) -> None:
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(event, ensure_ascii=False) + "\n")


def validate_program(source: str) -> None:
    tree = ast.parse(source)
    admitted = {"fr_ir.guide": {"AgentGoal", "GoalOperation", "GoalSelector"},
                "fr_ir.ir": {"TaskDelivery"}, "fr_ir.runtime": {"FrClient"},
                "json": None, "sys": None}
    denied = {"call", "execute", "execute_intent", "compile", "project", "context",
              "disclose", "open", "read_text", "write_text", "read_bytes", "write_bytes"}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            if any(alias.name not in admitted for alias in node.names):
                raise ValueError("SDK program imports outside the admitted surface")
        elif isinstance(node, ast.ImportFrom):
            names = admitted.get(node.module or "", set())
            if node.level or names is None or any(alias.name not in names for alias in node.names):
                raise ValueError("SDK program imports outside the admitted surface")
        elif isinstance(node, ast.Attribute) and (node.attr.startswith("__") or node.attr in denied):
            raise ValueError("SDK program calls outside the admitted surface")
        elif isinstance(node, ast.Name) and node.id in {
            "open", "exec", "eval", "compile", "__import__", "getattr", "setattr",
            "globals", "locals", "vars", "input", "breakpoint",
        }:
            raise ValueError("SDK program uses a forbidden Python primitive")


def act(session: Path, request: dict[str, object]) -> dict[str, object]:
    selected = config(session)
    project = session / "project"
    tool = request.get("tool")
    if tool == "sdk":
        if selected["arm"] != "fr" or any(row["request"].get("tool") == "sdk" for row in rows(session)):
            raise ValueError("sdk runs once in the fr arm")
        program = request.get("program")
        lines = request.get("program_lines")
        if isinstance(program, str) and 1 <= len(program.encode()) <= 65_536:
            source = program if program.endswith("\n") else program + "\n"
        elif (isinstance(lines, list) and 1 <= len(lines) <= 100
              and all(isinstance(line, str) for line in lines)):
            source = "\n".join(lines) + "\n"
        else:
            raise ValueError("sdk needs one bounded program or 1 through 100 program lines")
        validate_program(source)
        program = session / "artifacts/agent.py"
        program.write_text(source)
        environment = os.environ.copy()
        environment["PYTHONPATH"] = str(ROOT / "sdk/python/src")
        completed = subprocess.run([sys.executable, str(program.resolve()), str(project.resolve()), selected["wrapper"]],
                                   cwd=project, env=environment, capture_output=True, timeout=600)
        try:
            answer = json.loads(completed.stdout)
        except (UnicodeDecodeError, json.JSONDecodeError):
            answer = None
        return {"exit_code": completed.returncode, "answers": answer,
                "stderr": completed.stderr[:4096].decode(errors="replace")}
    if tool == "read":
        if selected["arm"] != "files" or request.get("path") != "src/lib.rs":
            raise ValueError("direct arm may read only src/lib.rs")
        return {"path": "src/lib.rs", "text": (project / "src/lib.rs").read_text()}
    if tool == "replace":
        if selected["arm"] != "files" or request.get("path") != "src/lib.rs":
            raise ValueError("direct replacement belongs to src/lib.rs")
        old, new = request.get("old"), request.get("new")
        if not isinstance(old, str) or not isinstance(new, str) or not old or len(old.encode()) > 1024 or len(new.encode()) > 1024:
            raise ValueError("replacement needs bounded text")
        path = project / "src/lib.rs"
        source = path.read_text()
        if source.count(old) != 1:
            raise ValueError("replacement old text must occur exactly once")
        path.write_text(source.replace(old, new))
        return {"changed": True, "path": "src/lib.rs"}
    if tool == "submit":
        if selected["arm"] != "files":
            raise ValueError("submit belongs to direct files")
        return {"accepted": request.get("answers") == EXPECTED, "answers": request.get("answers")}
    if tool == "finish":
        summary = request.get("summary")
        if not isinstance(summary, str) or not summary or len(summary.encode()) > 4096:
            raise ValueError("finish needs a bounded summary")
        return {"finished": True, "summary": summary}
    raise ValueError("unknown matched source tool")


def step(session: Path, request: dict[str, object]) -> dict[str, object]:
    before = digest((session / "project/src/lib.rs").read_bytes())
    started = time.time()
    try:
        result = act(session, request)
    except (OSError, ValueError, KeyError, SyntaxError, subprocess.SubprocessError) as error:
        result = {"error": str(error), "exit_code": 1}
    after = digest((session / "project/src/lib.rs").read_bytes())
    visible = json.dumps(result, ensure_ascii=False)
    append(session, {"request": request, "visible": visible,
                     "request_bytes": len(canonical(request)),
                     "response_bytes": len(visible.encode()), "source_before": before,
                     "source_after": after, "elapsed_seconds": time.time() - started})
    return result


def sdk_lifecycle(session: Path) -> dict[str, object]:
    events = rows(session, "sdk-events.jsonl")
    writes = [row for row in events if "--write" in row.get("arguments", [])]
    response = writes[0].get("response") if len(writes) == 1 else None
    workflow = response.get("workflow") if isinstance(response, dict) else None
    stages = workflow.get("stages") if isinstance(workflow, dict) else None
    patch = session / "project/artifacts/change.patch"
    return {"calls": len(events), "failed_calls": sum(row.get("exit_code") != 0 for row in events),
            "reviewed_writes": len(writes), "passed": response.get("passed") is True if isinstance(response, dict) else False,
            "transaction_status": workflow.get("transaction_status") if isinstance(workflow, dict) else None,
            "stages": [{"stage": row.get("stage"), "status": row.get("status")} for row in stages]
                      if isinstance(stages, list) else [],
            "patch_sha256": digest(patch.read_bytes()) if patch.is_file() else None}


def codex_observation(session: Path) -> dict[str, object]:
    events = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    usage = next((row["usage"] for row in reversed(events) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in events if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    allowed = str(ROOT / "tools/matched-agent-source.py")
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


def behavior_oracle(project: Path) -> bool:
    completed = subprocess.run(["python3", ".fr/check.py"], cwd=project,
                               capture_output=True, timeout=60)
    return completed.returncode == 0


def score(session: Path) -> dict[str, object]:
    selected = config(session)
    events = rows(session)
    requests = [row["request"] for row in events]
    visible = [json.loads(row["visible"]) for row in events]
    observation = codex_observation(session)
    run = json.loads((session / "codex-run.json").read_text())
    if selected["arm"] == "fr":
        actions = [value for request, value in zip(requests, visible) if request.get("tool") == "sdk"]
        answers = actions[0].get("answers") if len(actions) == 1 else None
        coverage = sdk_lifecycle(session)
        arm_ok = (len(actions) == 1 and actions[0].get("exit_code") == 0
                  and coverage["failed_calls"] == 0 and coverage["reviewed_writes"] == 1
                  and coverage["passed"] and coverage["transaction_status"] == "applied"
                  and len(coverage["stages"]) == 8 and coverage["patch_sha256"] is not None)
    else:
        submissions = [value for request, value in zip(requests, visible) if request.get("tool") == "submit"]
        answers = submissions[0].get("answers") if len(submissions) == 1 else None
        reads = sum(request.get("tool") == "read" for request in requests)
        replacements = sum(request.get("tool") == "replace" for request in requests)
        coverage = {"reads": reads, "replacements": replacements}
        arm_ok = len(submissions) == 1 and submissions[0].get("accepted") is True and reads == 1 and replacements == 1
    final = visible[-1] if visible else {}
    result = {"schema": "fr-matched-source-result-1", "arm": selected["arm"],
              "answers_match": answers == EXPECTED, "source_matches": (session / "project/src/lib.rs").read_text() == EXPECTED_SOURCE,
              "behavior_oracle": behavior_oracle(session / "project"),
              "coverage": coverage, "finished": final.get("finished") is True,
              "tool_calls": len(events), "tool_request_bytes": sum(row["request_bytes"] for row in events),
              "tool_response_bytes": sum(row["response_bytes"] for row in events),
              "tool_errors": sum("error" in value or value.get("exit_code") not in (None, 0) for value in visible),
              "codex": observation, "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (run["exit_code"] == 0 and not run["timed_out"] and arm_ok
                        and result["answers_match"] and result["source_matches"] and result["behavior_oracle"]
                        and result["finished"]
                        and result["tool_errors"] == 0 and not observation["direct_project_commands"]
                        and observation["failed_command_executions"] == 0
                        and observation["infrastructure_errors"] == 0)
    save(session / "result.json", result)
    return result


def score_all(directory: Path) -> dict[str, object]:
    experiment = json.loads((directory / "experiment.json").read_text())
    results = {arm: score(directory / arm) for arm in ARMS}
    usage = {arm: results[arm]["codex"]["usage"] or {} for arm in ARMS}
    metrics = ("input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens")
    report = {"schema": "fr-matched-source-manifest-1", "model": experiment["model"],
              "reasoning_effort": experiment["reasoning_effort"], "service_tier": experiment["service_tier"],
              "results": [results[arm] for arm in ARMS], "passed": all(results[arm]["passed"] for arm in ARMS),
              "usage_difference_fr_minus_files": {name: usage["fr"].get(name, 0) - usage["files"].get(name, 0) for name in metrics},
              "tool_call_difference_fr_minus_files": results["fr"]["tool_calls"] - results["files"]["tool_calls"],
              "claim": "One matched pair over one fixed source-writing task; no population, hidden-reasoning or billed-quota claim."}
    save(directory / "manifest.json", report)
    return report


def record(directory: Path, destination: Path, diagnostic: bool) -> dict[str, object]:
    report = json.loads((directory / "manifest.json").read_text())
    if not report.get("passed") and not diagnostic:
        raise ValueError("only a passing cohort can become acceptance evidence")
    if destination.exists():
        raise ValueError("evidence destination already exists")
    destination.mkdir(parents=True)
    files = {}
    for name in ("experiment.json", "manifest.json"):
        shutil.copyfile(directory / name, destination / name)
        files[name] = digest((destination / name).read_bytes())
    for arm in ARMS:
        target = destination / arm
        target.mkdir()
        names = list(RETAINED) + (["sdk-events.jsonl"] if arm == "fr" else [])
        for name in names:
            source = directory / arm / name
            if source.is_file():
                shutil.copyfile(source, target / name)
                files[f"{arm}/{name}"] = digest((target / name).read_bytes())
    report["acceptance_evidence"] = bool(report.get("passed")) and not diagnostic
    report["bindings"] = bindings()
    report["files"] = files
    save(destination / "manifest.json", report)
    return report


def audit(directory: Path) -> dict[str, object]:
    report = json.loads((directory / "manifest.json").read_text())
    if report.get("schema") != "fr-matched-source-manifest-1" or report.get("bindings") != bindings():
        raise ValueError("matched source evidence is stale")
    for relative, expected in report.get("files", {}).items():
        if relative == "manifest.json":
            continue
        path = directory / relative
        if not path.is_file() or digest(path.read_bytes()) != expected:
            raise ValueError(f"matched source artifact changed: {relative}")
    for arm in ARMS:
        result = json.loads((directory / arm / "result.json").read_text())
        if result.get("passed") is not True or result.get("source_matches") is not True:
            raise ValueError("retained matched source result did not pass")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    prepare_parser = sub.add_parser("prepare")
    prepare_parser.add_argument("directory", type=Path)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    run_parser = sub.add_parser("run")
    run_parser.add_argument("directory", type=Path)
    run_parser.add_argument("--codex", type=Path, default=Path("codex"))
    run_parser.add_argument("--timeout", type=int, default=1200)
    sub.add_parser("score").add_argument("directory", type=Path)
    record_parser = sub.add_parser("record")
    record_parser.add_argument("directory", type=Path)
    record_parser.add_argument("destination", type=Path)
    record_parser.add_argument("--diagnostic", action="store_true")
    sub.add_parser("audit").add_argument("directory", type=Path)
    step_parser = sub.add_parser("step")
    step_parser.add_argument("session", type=Path)
    step_parser.add_argument("--request-stdin", action="store_true")
    args = parser.parse_args()
    if args.command == "prepare":
        value = prepare(args.directory, args.fr.resolve())
    elif args.command == "run":
        version = subprocess.run([args.codex, "--version"], capture_output=True, text=True, check=True).stdout.strip()
        directory = args.directory.resolve()
        experiment = json.loads((directory / "experiment.json").read_text())
        value = {arm: BASE.run_agent(args.codex, directory / arm, MODEL, EFFORT,
                                     SERVICE_TIER, args.timeout, version) for arm in ARMS}
        value["experiment"] = experiment["schema"]
    elif args.command == "score":
        value = score_all(args.directory)
    elif args.command == "record":
        value = record(args.directory, args.destination, args.diagnostic)
    elif args.command == "audit":
        value = audit(args.directory)
    else:
        if not args.request_stdin:
            raise ValueError("step requires --request-stdin")
        request = json.load(sys.stdin)
        if not isinstance(request, dict):
            raise ValueError("step request must be an object")
        value = step(args.session, request)
    print(json.dumps(value, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
