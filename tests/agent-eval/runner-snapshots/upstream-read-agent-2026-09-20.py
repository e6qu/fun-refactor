#!/usr/bin/env python3
"""Run and retain a bounded live understand/trace task on pinned regex source."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time

from agent_eval import regex_workspace


ROOT = Path(__file__).resolve().parents[1]
MODEL = "gpt-5.6-luna"
EFFORT = "low"
TIER = "default"
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def source_digest(project: Path) -> str:
    source = []
    for path in sorted(project.rglob("*")):
        if path.is_file():
            source.append((str(path.relative_to(project)), sha(path.read_bytes())))
    return sha(json.dumps(source, separators=(",", ":")).encode())


def binding() -> dict[str, str]:
    return {name: sha((ROOT / name).read_bytes()) for name in
            ("tools/upstream-read-agent.py", "tools/agent_eval/regex_workspace.py")}


def prompt(session: Path) -> str:
    return f"""You are an independent acceptance agent in the pinned rust-lang/regex workspace.

Explain where the public regex::escape function sends its input, trace the implementation through the append helper and the metacharacter predicate, and distinguish source evidence from runtime proof. Use bounded fr evidence. The requested declaration is escape in src/lib.rs. Do not use raw source reading, search, Git, browsing, other tools, delegation, or edits. No human correction is available.

Use only this instrumented tool, through functions.exec / tools.exec_command. Every shell command must invoke exactly this step command, with a JSON request on stdin:
python3 {ROOT / 'tools/upstream-read-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide","goal":{{...}}}}
FRJSON

Requests:
- guide: call once with each of these two exact goal objects, in order. The `schema` key is required:
  {{"tool":"guide","goal":{{"schema":"fr-agent-goal-1","purpose":"understand","selector":{{"name":"escape","scope":"src/lib.rs","language":"rust"}},"operation":{{"kind":"automatic"}}}}}}
  {{"tool":"guide","goal":{{"schema":"fr-agent-goal-1","purpose":"trace","selector":{{"name":"escape","scope":"src/lib.rs","language":"rust"}},"operation":{{"kind":"automatic"}}}}}}
- follow: {{"tool":"follow","guide":0}} follows the first ready action of that guide. Follow both guides before source reveal.
- find: {{"tool":"find","name":"...","scope":"..."}} returns at most five exact matches in src/lib.rs or regex-syntax/src/lib.rs. Use it to locate implementation helpers after the trace.
- show: {{"tool":"show","handle":"..."}} reveals at most 512 source bytes from a handle returned by guide, follow or find. Reveal the facade, lower-crate escape, append helper and predicate as needed. Do not assume a call edge is behavioral proof.
- finish: {{"tool":"finish","answer":{{"delegate_path":"...","append_helper":"...","escape_predicate":"...","trace_confidence":"...","runtime_proven":false,"example_metacharacter":"..."}}}}. Use the target path of the resolved call, exact helper names, the call edge's confidence and a character visible in the predicate. Finish only after evidence supports all fields.

The evaluator independently checks the pinned source, exact answer, tool sequence, bounded reveals and unchanged workspace. If any evidence is unavailable, report it honestly in the answer.
"""


def prepare(session: Path, binary: Path) -> dict[str, object]:
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    regex_workspace.unpack(project)
    config = {"schema": "fr-upstream-read-session-1", "upstream_commit": regex_workspace.COMMIT,
              "archive_sha256": regex_workspace.ARCHIVE_SHA, "lock_sha256": regex_workspace.LOCK_SHA,
              "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
              "source_sha256": source_digest(project), "evaluator": binding(),
              "manual_corrections": 0}
    save(session / "session.json", config)
    (session / "prompt.txt").write_text(prompt(session))
    return config


def config(session: Path) -> dict[str, object]:
    value = json.loads((session / "session.json").read_text())
    if value["schema"] != "fr-upstream-read-session-1" or value["evaluator"] != binding():
        raise ValueError("session evaluator changed")
    if sha(Path(value["binary"]).read_bytes()) != value["binary_sha256"]:
        raise ValueError("session binary changed")
    return value


def events(session: Path) -> list[dict[str, object]]:
    path = session / "events.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def fr(session: Path, arguments: list[str], input_value: object = None) -> dict[str, object]:
    selected = config(session)
    command = [selected["binary"], "--json", "--no-cache", "-C", str(session / "project"), *arguments]
    completed = subprocess.run(command, input=(json.dumps(input_value).encode() if input_value is not None else None),
                               capture_output=True, timeout=90)
    if completed.returncode:
        raise ValueError(f"fr returned {completed.returncode}: {completed.stderr.decode(errors='replace')[-512:]}")
    if len(completed.stdout) > 65_536:
        raise ValueError("fr response exceeded the instrumented limit")
    return json.loads(completed.stdout)


def step(session: Path, request: dict[str, object]) -> object:
    config(session)
    prior = events(session)
    kind = request.get("tool")
    full = None
    if kind == "guide":
        goal = request.get("goal")
        if not isinstance(goal, dict) or goal.get("schema") != "fr-agent-goal-1" or goal.get("purpose") not in ("understand", "trace"):
            raise ValueError("guide needs a valid understand or trace goal")
        if goal.get("selector") != {"name": "escape", "scope": "src/lib.rs", "language": "rust"} or goal.get("operation") != {"kind": "automatic"}:
            raise ValueError("guide selector and operation must match the pinned task")
        if any(row["request"].get("goal", {}).get("purpose") == goal["purpose"] for row in prior):
            raise ValueError("the guide purpose was already requested")
        full = fr(session, ["guide", "--from", "-"], goal)
        visible = {key: full[key] for key in ("route", "state", "target", "actions", "basis")}
        visible["guide"] = sum(row["request"]["tool"] == "guide" for row in prior)
    elif kind == "follow":
        number = request.get("guide")
        guides = [row for row in prior if row["request"]["tool"] == "guide"]
        if type(number) is not int or not 0 <= number < len(guides):
            raise ValueError("follow requires a returned guide")
        if any(row["request"]["tool"] == "follow" and row["request"]["guide"] == number for row in prior):
            raise ValueError("guide already followed")
        action = guides[number]["full"]["actions"][0]
        if not action["ready"] or action["writes"]:
            raise ValueError("guide action is not a ready read")
        full = fr(session, action["arguments"], action.get("input"))
        if full.get("schema") != action["output_schema"]:
            raise ValueError("follow output schema differs from guide")
        visible = {"followed": number, "target": full.get("target"), "selected": full.get("selected"),
                   "coverage": full.get("coverage")}
    elif kind == "find":
        name, scope = request.get("name"), request.get("scope")
        if (not isinstance(name, str) or not name.isidentifier() or len(name) > 64
                or scope not in ("src/lib.rs", "regex-syntax/src/lib.rs")):
            raise ValueError("find needs a bounded literal name in a task file")
        full = fr(session, ["project", "find", name, "--in", scope, "--limit", "5"])
        visible = {"rows": full["rows"], "columns": full["columns"], "revision": full["revision"]}
    elif kind == "show":
        handle = request.get("handle")
        offered = {row["full"]["target"]["handle"] for row in prior if row["request"]["tool"] == "guide"}
        offered.update(row["full"]["rows"][i][0] for row in prior if row["request"]["tool"] == "find"
                       for i in range(len(row["full"]["rows"])))
        def disclosed_handles(value: object) -> set[str]:
            if isinstance(value, dict):
                return ({value["handle"]} if isinstance(value.get("handle"), str) else set()).union(
                    *(disclosed_handles(part) for part in value.values()))
            if isinstance(value, list):
                return set().union(*(disclosed_handles(part) for part in value))
            return set()
        for row in prior:
            if row["request"]["tool"] == "follow":
                offered.update(disclosed_handles(row["visible"]))
        if handle not in offered:
            raise ValueError("show requires a disclosed guide, follow or find handle")
        full = fr(session, ["project", "show", handle, "--source", "--bytes", "512"])
        visible = full
    elif kind == "finish":
        answer = request.get("answer")
        if not isinstance(answer, dict) or len(json.dumps(answer).encode()) > 2048:
            raise ValueError("finish needs a bounded structured answer")
        visible = {"finished": True, "answer": answer}
    else:
        raise ValueError("unknown upstream-read tool")
    if len(json.dumps(visible).encode()) > 20_000:
        raise ValueError("visible response exceeds limit")
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps({"request": request, "full": full, "visible": visible,
                                 "source_sha256": source_digest(session / "project")}, ensure_ascii=False) + "\n")
    return visible


def codex_command(codex: Path, session: Path, model: str, effort: str, tier: str) -> list[str]:
    return [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
            "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
            "--add-dir", str(session), "--model", model, "--config", f'model_reasoning_effort="{effort}"',
            "--config", f'service_tier="{tier}"', "--cd", str(session / "project"),
            "--output-last-message", str(session / "codex-final.txt"), "-"]


def run(session: Path, codex: Path, model: str, effort: str, tier: str, timeout: int) -> dict[str, object]:
    config(session)
    if any((session / name).exists() for name in ("events.jsonl", "codex-events.jsonl", "codex-run.json")):
        raise ValueError("live session must be fresh")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    command = codex_command(codex, session, model, effort, tier)
    started, timed_out = time.time(), False
    with (session / "codex-events.jsonl").open("wb") as stdout, (session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code = completed.returncode
        except subprocess.TimeoutExpired:
            exit_code, timed_out = 124, True
    record = {"schema": "fr-upstream-read-run-1", "codex_version": version, "model": model,
              "reasoning_effort": effort, "service_tier": tier, "ephemeral": True,
              "ignored_user_config": True, "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
              "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def score(session: Path) -> dict[str, object]:
    selected = config(session)
    rows = events(session)
    run_record = json.loads((session / "codex-run.json").read_text())
    codex_rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    direct = [item["command"] for item in commands if str(ROOT / "tools/upstream-read-agent.py") not in item["command"]
              or " step " not in item["command"]]
    guides = [row for row in rows if row["request"]["tool"] == "guide"]
    follows = [row for row in rows if row["request"]["tool"] == "follow"]
    shows = [row for row in rows if row["request"]["tool"] == "show"]
    answers = [row["visible"]["answer"] for row in rows if row["request"]["tool"] == "finish"]
    # These expectations are independent of guide output. Their source is the pinned upstream
    # archive, and the archive digest is checked before every preparation.
    expected = {"append_helper": "escape_into",
                "escape_predicate": "is_meta_character", "trace_confidence": "import-qualified",
                "runtime_proven": False}
    answer = answers[-1] if answers else {}
    example = answer.get("example_metacharacter")
    answer_ok = (answer.get("delegate_path") in ("regex-syntax/src/lib.rs", "regex-syntax/src/lib.rs::escape")
                 and all(answer.get(key) == value for key, value in expected.items())
                 and isinstance(example, str) and len(example) == 1
                 and example in "\\.+*?()|[]{}^$#&-~")
    shown_paths = {row["full"].get("node", {}).get("path") for row in shows}
    shown_symbols = {row["full"].get("node", {}).get("name") for row in shows}
    result = {"schema": "fr-upstream-read-result-1", "passed": False,
              "guide_purposes": [row["request"]["goal"]["purpose"] for row in guides],
              "guide_routes": [row["full"]["route"]["id"] for row in guides],
              "follows": [row["request"]["guide"] for row in follows],
              "source_reveal_calls": len(shows), "shown_paths": sorted(path for path in shown_paths if path),
              "shown_symbols": sorted(name for name in shown_symbols if name),
              "answer": answer, "answer_oracle": answer_ok,
              "tool_calls": len(rows), "source_unchanged": source_digest(session / "project") == selected["source_sha256"]
              and all(row["source_sha256"] == selected["source_sha256"] for row in rows),
              "codex": {"usage": usage, "command_executions": len(commands),
                        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
                        "direct_project_commands": direct,
                        "billed_quota": {"available": False, "value": None}},
              "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (run_record["exit_code"] == 0 and not run_record["timed_out"]
                        and result["guide_purposes"] == ["understand", "trace"]
                        and result["guide_routes"] == ["evidence", "evidence"]
                        and result["follows"] == [0, 1]
                        and {"src/lib.rs", "regex-syntax/src/lib.rs"} <= shown_paths
                        and {"escape", "escape_into", "is_meta_character"} <= shown_symbols
                        and len(shows) <= 6 and len(answers) == 1 and rows[-1]["request"]["tool"] == "finish"
                        and answer_ok and result["source_unchanged"] and not direct
                        and result["codex"]["failed_command_executions"] == 0
                        and isinstance(usage, dict) and selected["manual_corrections"] == 0)
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool, reason: str | None,
           omit_tool_events: bool) -> dict[str, object]:
    result_path = session / "result.json"
    result = json.loads(result_path.read_text()) if result_path.exists() else None
    if (result is None or not result["passed"]) and not diagnostic:
        raise ValueError("failed trial can only be recorded as diagnostic")
    if diagnostic and not reason:
        raise ValueError("diagnostic retention requires --reason")
    destination.mkdir(parents=True, exist_ok=False)
    files = {}
    for name in FILES:
        if name == "events.jsonl" and omit_tool_events:
            continue
        if not (session / name).exists():
            continue
        shutil.copyfile(session / name, destination / name)
        files[name] = sha((destination / name).read_bytes())
    run_path = session / "codex-run.json"
    run_record = json.loads(run_path.read_text()) if run_path.exists() else None
    manifest = {"schema": "fr-upstream-read-manifest-1", "passed": bool(result and result["passed"]),
                "acceptance_evidence": bool(result and result["passed"] and not diagnostic),
                "diagnostic_reason": reason if diagnostic else None,
                "upstream_commit": regex_workspace.COMMIT, "model": run_record.get("model") if run_record else MODEL,
                "evaluator": json.loads((session / "session.json").read_text())["evaluator"],
                "files": files}
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, object]:
    manifest = json.loads((destination / "manifest.json").read_text())
    for name, expected in manifest["files"].items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result_path = destination / "result.json"
    result = json.loads(result_path.read_text()) if result_path.exists() else None
    if manifest["passed"] != bool(result and result["passed"]) or (manifest["acceptance_evidence"] and not manifest["passed"]):
        raise ValueError("retained outcome classification disagrees with score")
    return {"verified": True, "passed": manifest["passed"], "acceptance_evidence": manifest["acceptance_evidence"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare"); p.add_argument("session", type=Path); p.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    p = sub.add_parser("step"); p.add_argument("session", type=Path); p.add_argument("--request-stdin", action="store_true")
    p = sub.add_parser("run"); p.add_argument("session", type=Path); p.add_argument("--codex", type=Path, default=Path("codex"))
    p.add_argument("--model", default=MODEL); p.add_argument("--effort", default=EFFORT); p.add_argument("--service-tier", default=TIER)
    p.add_argument("--timeout", type=int, default=1800); p.add_argument("--confirm-agent-spend", action="store_true")
    p = sub.add_parser("score"); p.add_argument("session", type=Path)
    p = sub.add_parser("record"); p.add_argument("session", type=Path); p.add_argument("destination", type=Path); p.add_argument("--diagnostic", action="store_true")
    p.add_argument("--reason"); p.add_argument("--omit-tool-events", action="store_true")
    p = sub.add_parser("audit"); p.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.action == "prepare": result = prepare(args.session.resolve(), args.fr.resolve())
        elif args.action == "step":
            if not args.request_stdin: raise ValueError("step requires --request-stdin")
            result = step(args.session.resolve(), json.load(sys.stdin))
        elif args.action == "run":
            if not args.confirm_agent_spend: raise ValueError("--confirm-agent-spend is required")
            result = run(args.session.resolve(), args.codex, args.model, args.effort, args.service_tier, args.timeout)
        elif args.action == "score": result = score(args.session.resolve())
        elif args.action == "record": result = record(args.session.resolve(), args.destination.resolve(), args.diagnostic, args.reason, args.omit_tool_events)
        else: result = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, json.JSONDecodeError, subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
