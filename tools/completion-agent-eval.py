#!/usr/bin/env python3
"""Prepare, run, score and retain bounded Codex completion workflows."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
MODEL = "gpt-5.6-luna"
EFFORT = "low"
SERVICE_TIER = "default"
GROUPS = {
    "fundamentals": ("understanding", "tracing", "direct-change", "recipe"),
    "structured": ("semantic-edit", "framework-migration", "proof"),
}
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
        "tools/completion-agent-eval.py",
        "tools/completion-workflows.py",
        "src/audit.rs",
        "src/project/agent_guide.rs",
        "src/project/agent_intent.rs",
        "src/project/task_change.rs",
        "src/project/migration.rs",
        "src/spec.rs",
        "skills/fr/SKILL.md",
    )
    return {name: digest((ROOT / name).read_bytes()) for name in paths}


def prompt(session: Path, group: str, proof_path: str, obligation: str) -> str:
    tasks = {
        "fundamentals": f"""Complete these workflows in order:
1. Understand the declaration named calculate with automatic guidance.
2. Trace the declaration named calculate with automatic guidance.
3. Preview a direct capability change that renames calculate to compute.
4. Preview the same rename with the recipe route. Author a schema-1 recipe that matches exactly one declaration and requires one changed file and zero refusals.""",
        "structured": f"""Complete these workflows in order:
1. Preview a semantic scalar edit that changes the integer 7 to 9 in calculate.
2. Preview migration of app/api/signals/route.ts to FastAPI at converted/signals.py.
3. Check the proof workflow for obligation {obligation} in {proof_path}. Use tactics `rfl`.""",
    }[group]
    operation_shapes = {
        "fundamentals": (
            'Use these operation objects in task order: {"kind":"automatic"}, '
            '{"kind":"automatic"}, {"kind":"capability","capability":"rename",'
            '"parameters":{"new_name":"compute"}}, and {"kind":"recipe","verb":"rename"}.'
        ),
        "structured": (
            'Use these operation objects in task order: {"kind":"semantic-scalar",'
            '"operation":"set-int","from":"7","to":"9"}, '
            '{"kind":"framework-migration","to":"fastapi"}, and the proof object in task 3.'
        ),
    }[group]
    return f"""You are an independent acceptance-test agent using fr on an unfamiliar project.

{tasks}

{operation_shapes}

Express each task as one `fr-agent-goal-1` object and start it with the instrumented `guide` tool. Follow every returned action that is ready or becomes executable after you supply its named placeholders. Use only the instrumented tool. Do not inspect project files with shell, file, search or Git commands. Do not call help, vocabulary, schema or audit commands. Do not add `--write`; every requested operation is a read or preview. Do not repeat a successful call. Stop on a refusal and report it honestly.

Invoke the tool through functions.exec / tools.exec_command with this stdin form:
python3 {ROOT / 'tools/completion-agent-eval.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide","goal":GOAL}}
FRJSON

Tool requests:
- `{{"tool":"guide","goal":GOAL}}` returns a guide ID, route, evidence and exact actions.
- `{{"tool":"follow","guide":ID,"action":INDEX,"replace":{{"<placeholder>":"value"}},"files":{{"name":"content"}}}}` executes one returned action. For a recipe or tactics file, write it through `files` and replace the placeholder with that same file name. For migration, copy the compatible feature ID from guide evidence.
- `{{"tool":"finish","summary":"..."}}` finishes after every workflow and action succeeds.

The harness records complete prompts, tool requests and responses, source identities and Codex events. It will reject direct project access, source mutation, missing workflows, repeated actions and mismatched output schemas. No human correction is available.
"""


def prepare(out: Path, executable: Path) -> dict[str, object]:
    if out.exists():
        raise ValueError("completion agent directory already exists")
    out.mkdir(parents=True)
    frozen = out / "fr-completion-agent-bin"
    shutil.copyfile(executable, frozen)
    frozen.chmod(0o555)
    base = out / "base"
    base.mkdir()
    proof_path, obligation = CW.prepare(frozen, base)
    sessions = []
    case_map = {case["id"]: case for case in CW.cases(proof_path, obligation)}
    for group, names in GROUPS.items():
        session = out / group
        session.mkdir()
        project = session / "project"
        shutil.copytree(base, project, ignore=shutil.ignore_patterns(".fr-history"))
        (session / "artifacts").mkdir()
        selected = [case_map[name] for name in names]
        config = {
            "schema": "fr-completion-agent-session-1",
            "group": group,
            "routes": [case["route"] for case in selected],
            "workflow_ids": list(names),
            "proof_path": proof_path,
            "obligation": obligation,
            "binary": str(frozen),
            "binary_sha256": digest(frozen.read_bytes()),
            "source_bindings": source_bindings(),
            "original_source": CW.source_snapshot(project),
            "manual_corrections": 0,
        }
        save(session / "session.json", config)
        (session / "prompt.txt").write_text(prompt(session, group, proof_path, obligation))
        sessions.append(str(session))
    shutil.rmtree(base)
    experiment = {
        "schema": "fr-completion-agent-experiment-1",
        "sessions": sessions,
        "groups": GROUPS,
        "model": MODEL,
        "reasoning_effort": EFFORT,
        "service_tier": SERVICE_TIER,
    }
    save(out / "experiment.json", experiment)
    return experiment


def session_config(session: Path) -> dict[str, object]:
    config = json.loads((session / "session.json").read_text())
    if config.get("schema") != "fr-completion-agent-session-1":
        raise ValueError("completion agent session schema is invalid")
    if config.get("source_bindings") != source_bindings():
        raise ValueError("completion agent session source bindings are stale")
    binary = Path(config["binary"])
    if digest(binary.read_bytes()) != config["binary_sha256"]:
        raise ValueError("completion agent binary changed after preparation")
    return config


def events(session: Path) -> list[dict[str, object]]:
    path = session / "events.jsonl"
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def append_event(session: Path, event: dict[str, object]) -> None:
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(event, ensure_ascii=False) + "\n")


def invoke(config: dict[str, object], session: Path, arguments: list[str], value=None):
    return CW.run(Path(config["binary"]), session / "project", arguments, value)


def output_summary(report: object) -> object:
    if not isinstance(report, dict):
        return report
    keys = (
        "schema",
        "ready",
        "applied",
        "executed",
        "passed",
        "new_name",
        "edit_plan",
        "proof_validation",
        "claims",
    )
    summary = {key: report[key] for key in keys if key in report}
    if "migration" in report:
        migration = report["migration"]
        summary["migration"] = {
            key: migration[key]
            for key in ("source_kind", "target", "files_changed", "route_count")
            if key in migration
        }
    return summary


def step(session: Path, request: dict[str, object]) -> dict[str, object]:
    config = session_config(session)
    prior = events(session)
    started = time.time()
    tool = request.get("tool")
    full = None
    if tool == "guide":
        goal = request.get("goal")
        if not isinstance(goal, dict):
            raise ValueError("guide requires one goal object")
        full, event = invoke(config, session, ["guide", "--from", "-"], goal)
        visible = {
            "guide": sum(1 for row in prior if row["request"].get("tool") == "guide"),
            "route": full["route"],
            "state": full["state"],
            "target": full.get("target"),
            "actions": full["actions"],
            "reference": full.get("reference"),
            "basis": full["basis"],
        }
        command_event = event
    elif tool == "follow":
        guide_id = request.get("guide")
        action_id = request.get("action")
        guides = [row for row in prior if row["request"].get("tool") == "guide"]
        if type(guide_id) is not int or not 0 <= guide_id < len(guides):
            raise ValueError("follow requires an existing guide ID")
        guide = guides[guide_id]["full"]
        actions = guide["actions"]
        if type(action_id) is not int or not 0 <= action_id < len(actions):
            raise ValueError("follow requires an existing action index")
        if any(
            row["request"].get("tool") == "follow"
            and row["request"].get("guide") == guide_id
            and row["request"].get("action") == action_id
            for row in prior
        ):
            raise ValueError("a successful guide action cannot be repeated")
        action = actions[action_id]
        arguments = [value for value in action["arguments"]]
        replacements = request.get("replace", {})
        if not isinstance(replacements, dict):
            raise ValueError("follow replacements must be an object")
        files = request.get("files", {})
        if not isinstance(files, dict) or len(files) > 2:
            raise ValueError("follow files must be an object with at most two entries")
        written = {}
        for name, content in files.items():
            if Path(name).name != name or not isinstance(content, str) or len(content.encode()) > 16_384:
                raise ValueError("authored files need bounded plain names and UTF-8 content")
            path = session / "artifacts" / name
            path.write_text(content)
            written[name] = str(path)
        normalized = {}
        for placeholder, value in replacements.items():
            if placeholder not in arguments or not isinstance(value, str):
                raise ValueError("replacement must name one exact returned placeholder")
            normalized[placeholder] = written.get(value, value)
        arguments = [normalized.get(value, value) for value in arguments]
        if any(value.startswith("<") and value.endswith(">") for value in arguments):
            raise ValueError("follow still has an authored placeholder")
        if any(value in ("--write", "--save-plan") for value in arguments):
            raise ValueError("completion agent actions must remain previews")
        full, command_event = invoke(config, session, arguments, action.get("input"))
        schema = action.get("output_schema")
        field = action.get("schema_field", "schema")
        if schema and full.get(field) != schema:
            raise ValueError("follow output does not match the guide schema")
        visible = {
            "followed": {"guide": guide_id, "action": action_id},
            "arguments": [Path(value).name if value in written.values() else value for value in arguments],
            "response": output_summary(full),
            "response_sha256": command_event["response_sha256"],
            "response_bytes": command_event["response_bytes"],
        }
    elif tool == "finish":
        summary = request.get("summary")
        if not isinstance(summary, str) or not summary or len(summary.encode()) > 4096:
            raise ValueError("finish needs a bounded summary")
        visible = {"finished": True, "summary": summary}
        command_event = {"request_bytes": len(canonical(request)), "response_bytes": 0}
    else:
        raise ValueError("unknown completion agent tool")
    elapsed = time.time() - started
    visible_text = json.dumps(visible, ensure_ascii=False)
    append_event(
        session,
        {
            "request": request,
            "full": full,
            "visible": visible_text,
            "request_bytes": len(canonical(request)),
            "response_bytes": len(visible_text.encode()),
            "underlying_request_bytes": command_event["request_bytes"],
            "underlying_response_bytes": command_event["response_bytes"],
            "elapsed_seconds": elapsed,
        },
    )
    return visible


def codex_command(
    codex: Path, session: Path, model: str, effort: str, service_tier: str
) -> list[str]:
    return [
        str(codex),
        "exec",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--json",
        "--color",
        "never",
        "--sandbox",
        "workspace-write",
        "--add-dir",
        str(session),
        "--model",
        model,
        "--config",
        f'model_reasoning_effort="{effort}"',
        "--config",
        f'service_tier="{service_tier}"',
        "--cd",
        str(session / "project"),
        "--output-last-message",
        str(session / "codex-final.txt"),
        "-",
    ]


def run_agent(
    codex: Path,
    session: Path,
    model: str,
    effort: str,
    service_tier: str,
    timeout: int,
    version: str,
) -> dict[str, object]:
    for name in ("events.jsonl", "codex-events.jsonl", "codex-run.json"):
        if (session / name).exists():
            raise ValueError(f"session is not fresh: {session / name}")
    command = codex_command(codex, session, model, effort, service_tier)
    started = time.time()
    timed_out = False
    with (session / "codex-events.jsonl").open("wb") as stdout, (
        session / "codex-stderr.txt"
    ).open("wb") as stderr:
        try:
            completed = subprocess.run(
                command,
                input=(session / "prompt.txt").read_bytes(),
                stdout=stdout,
                stderr=stderr,
                timeout=timeout,
            )
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
            exit_code = 124
    record = {
        "schema": "fr-completion-agent-run-1",
        "group": session.name,
        "codex_version": version,
        "model": model,
        "reasoning_effort": effort,
        "service_tier": service_tier,
        "ephemeral": True,
        "ignored_user_config": True,
        "ignored_rules": True,
        "sandbox": "workspace-write",
        "prompt_sha256": digest((session / "prompt.txt").read_bytes()),
        "events_sha256": digest((session / "codex-events.jsonl").read_bytes()),
        "stderr_sha256": digest((session / "codex-stderr.txt").read_bytes()),
        "elapsed_seconds": time.time() - started,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "command": command,
    }
    save(session / "codex-run.json", record)
    return record


def codex_observation(session: Path) -> dict[str, object]:
    rows = [
        json.loads(line)
        for line in (session / "codex-events.jsonl").read_text().splitlines()
        if line
    ]
    usage = next(
        (row["usage"] for row in reversed(rows) if row.get("type") == "turn.completed"),
        None,
    )
    commands = [
        row["item"]
        for row in rows
        if row.get("type") == "item.completed"
        and row.get("item", {}).get("type") == "command_execution"
    ]
    allowed = str(ROOT / "tools/completion-agent-eval.py")
    direct_access = [
        item["command"]
        for item in commands
        if allowed not in item["command"] or " step " not in item["command"]
    ]
    return {
        "usage": usage,
        "command_executions": len(commands),
        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
        "direct_project_commands": direct_access,
        "event_bytes": (session / "codex-events.jsonl").stat().st_size,
        "final_bytes": (session / "codex-final.txt").stat().st_size,
        "billed_quota": {"available": False, "value": None},
    }


def score(session: Path) -> dict[str, object]:
    config = session_config(session)
    rows = events(session)
    guides = [row for row in rows if row["request"].get("tool") == "guide"]
    routes = [row["full"]["route"]["id"] for row in guides]
    follows = [row for row in rows if row["request"].get("tool") == "follow"]
    followed = {row["request"]["guide"] for row in follows}
    finished = bool(rows and json.loads(rows[-1]["visible"]).get("finished"))
    source_final = CW.source_snapshot(session / "project")
    run_record = json.loads((session / "codex-run.json").read_text())
    observation = codex_observation(session)
    result = {
        "schema": "fr-completion-agent-result-1",
        "group": config["group"],
        "workflow_ids": config["workflow_ids"],
        "expected_routes": config["routes"],
        "observed_routes": routes,
        "guide_calls": len(guides),
        "follow_calls": len(follows),
        "post_guide_exploratory_calls": 0,
        "finished": finished,
        "source_unchanged": source_final == config["original_source"],
        "source_before_sha256": digest(canonical(config["original_source"])),
        "source_after_sha256": digest(canonical(source_final)),
        "tool_request_bytes": sum(row["request_bytes"] for row in rows),
        "tool_response_bytes": sum(row["response_bytes"] for row in rows),
        "underlying_request_bytes": sum(row["underlying_request_bytes"] for row in rows),
        "underlying_response_bytes": sum(row["underlying_response_bytes"] for row in rows),
        "prompt_bytes": (session / "prompt.txt").stat().st_size,
        "codex": observation,
        "manual_corrections": config["manual_corrections"],
        "measurement_scope": "Complete prompt, instrumented requests and responses, Codex JSONL usage and final answer. The CLI does not expose billed quota.",
    }
    result["passed"] = (
        run_record["exit_code"] == 0
        and not run_record["timed_out"]
        and routes == config["routes"]
        and followed == set(range(len(guides)))
        and len(guides) == len(config["routes"])
        and len(follows) >= len(guides)
        and finished
        and result["source_unchanged"]
        and not observation["direct_project_commands"]
        and observation["failed_command_executions"] == 0
        and observation["command_executions"] == len(rows)
        and config["manual_corrections"] == 0
    )
    save(session / "result.json", result)
    return result


def score_all(directory: Path) -> dict[str, object]:
    experiment = json.loads((directory / "experiment.json").read_text())
    results = [score(directory / name) for name in GROUPS]
    report = {
        "schema": "fr-completion-agent-manifest-1",
        "model": experiment["model"],
        "reasoning_effort": experiment["reasoning_effort"],
        "service_tier": experiment["service_tier"],
        "results": results,
        "passed": all(result["passed"] for result in results),
        "workflow_families": sum(len(result["workflow_ids"]) for result in results),
        "fresh_sessions": len(results),
        "manual_corrections": sum(result["manual_corrections"] for result in results),
    }
    save(directory / "manifest.json", report)
    return report


def record(directory: Path, destination: Path, diagnostic: bool = False) -> dict[str, object]:
    report = json.loads((directory / "manifest.json").read_text())
    if not report.get("passed") and not diagnostic:
        raise ValueError("only a passing completion-agent cohort can become acceptance evidence")
    if destination.exists():
        raise ValueError("retained completion-agent destination already exists")
    destination.mkdir(parents=True)
    files = {}
    for name in ("experiment.json", "manifest.json"):
        shutil.copyfile(directory / name, destination / name)
        files[name] = digest((destination / name).read_bytes())
    for group in GROUPS:
        target = destination / group
        target.mkdir()
        for name in RETAINED_FILES:
            shutil.copyfile(directory / group / name, target / name)
            files[f"{group}/{name}"] = digest((target / name).read_bytes())
    report["acceptance_evidence"] = bool(report.get("passed"))
    report["files"] = files
    save(destination / "manifest.json", report)
    return report


def replay(directory: Path) -> dict[str, object]:
    manifest = json.loads((directory / "manifest.json").read_text())
    files = manifest.get("files", {})
    for name, expected in files.items():
        if name == "manifest.json":
            continue
        if digest((directory / name).read_bytes()) != expected:
            raise ValueError(f"retained completion-agent file changed: {name}")
    results = [json.loads((directory / group / "result.json").read_text()) for group in GROUPS]
    actual = all(result.get("passed") for result in results)
    if bool(manifest.get("passed")) != actual:
        raise ValueError("retained completion-agent outcome is inconsistent")
    if bool(manifest.get("acceptance_evidence")) != actual:
        raise ValueError("retained completion-agent evidence classification is inconsistent")
    return {
        "verified": True,
        "passed": actual,
        "sessions": len(results),
        "workflow_families": 7,
    }


def request_from(arguments) -> dict[str, object]:
    if arguments.request_stdin:
        return json.load(sys.stdin)
    if arguments.request is None:
        raise ValueError("step needs a JSON request or --request-stdin")
    return json.loads(arguments.request)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("directory", type=Path)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    step_parser = subparsers.add_parser("step")
    step_parser.add_argument("session", type=Path)
    step_parser.add_argument("request", nargs="?")
    step_parser.add_argument("--request-stdin", action="store_true")
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("directory", type=Path)
    run_parser.add_argument("--codex", type=Path, default=Path("codex"))
    run_parser.add_argument("--model", default=MODEL)
    run_parser.add_argument("--effort", default=EFFORT)
    run_parser.add_argument("--service-tier", default=SERVICE_TIER)
    run_parser.add_argument("--timeout", type=int, default=1800)
    run_parser.add_argument("--confirm-agent-spend", action="store_true")
    score_parser = subparsers.add_parser("score")
    score_parser.add_argument("directory", type=Path)
    record_parser = subparsers.add_parser("record")
    record_parser.add_argument("directory", type=Path)
    record_parser.add_argument("destination", type=Path)
    record_parser.add_argument("--diagnostic", action="store_true")
    replay_parser = subparsers.add_parser("replay")
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
            result = {
                "runs": [
                    run_agent(
                        arguments.codex,
                        arguments.directory / group,
                        arguments.model,
                        arguments.effort,
                        arguments.service_tier,
                        arguments.timeout,
                        version,
                    )
                    for group in GROUPS
                ]
            }
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
