#!/usr/bin/env python3
"""Retain one guided, reviewed cross-crate rename on pinned regex source."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any, TypedDict, cast

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk/python/src"))

from agent_eval import regex_workspace
from fr_ir.guide import AgentGoal, GoalLimits, GoalOperation, GoalSelector
from fr_ir.intent_actions import CapabilityOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient


ROOT = Path(__file__).resolve().parents[1]
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
NAME = "quote_regex"
CHANGED = ("regex-cli/args/patterns.rs", "regex-syntax/src/lib.rs", "src/lib.rs")
CHECK_NAMES = ("upstream", "cli", "minimal")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
CHECKS = [
    {"name": "upstream", "argv": ["cargo", "test", "-p", "regex", "-p", "regex-syntax", "--lib", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 180, "covers": ["Pinned regex and syntax library tests"]},
    {"name": "cli", "argv": ["cargo", "check", "-p", "regex-cli", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 120, "covers": ["Pinned regex CLI builds"]},
    {"name": "minimal", "argv": ["cargo", "check", "-p", "regex", "--no-default-features", "--locked", "--offline"],
     "cwd": ".", "timeout_seconds": 120, "covers": ["Regex builds without default features"]},
]


class Event(TypedDict):
    request: dict[str, Any]
    visible: dict[str, Any]
    full: dict[str, Any] | None
    source_sha256: str


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def object_map(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value):
        raise ValueError(f"{label} must be an object")
    return cast(dict[str, Any], value)


def source_files(project: Path) -> dict[str, str]:
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=project).decode().split("\0")
    return {name: sha((project / name).read_bytes()) for name in paths if name and not name.startswith(".fr/")}


def source_digest(project: Path) -> str:
    return sha(json.dumps(source_files(project), sort_keys=True).encode())


def binding() -> dict[str, str]:
    return {name: sha((ROOT / name).read_bytes()) for name in
            ("tools/upstream-rename-agent.py", "tools/agent_eval/regex_workspace.py")}


def goal() -> AgentGoal:
    return AgentGoal("change", selector=GoalSelector(name="escape", scope="regex-syntax/src/lib.rs", language="rust"),
                     operation=GoalOperation("capability", {"capability": "rename", "parameters": {"new_name": NAME}}),
                     checks=CHECK_NAMES, delivery=TaskDelivery(patch="artifacts/rename.patch", check_output_bytes=1024),
                     context=GoalLimits(packet_limit=65536))


def action() -> TaggedIntentAction:
    return TaggedIntentAction(CapabilityOperation("rename", parameters={"new_name": NAME},
                                                  checks=CHECK_NAMES, delivery=goal().delivery),
                              diff_bytes=65536, report_bytes=65536)


def prompt(session: Path) -> str:
    return f"""You are an independent acceptance agent in pinned rust-lang/regex.

Task: rename the public regex-syntax function `escape` to `{NAME}` and update its resolved in-workspace callers. Keep the public `regex::escape` facade and its behavior. Inspect the guided preview and the complete reviewed diff before execution. Execute the unchanged review, then report the files and checks. This is a rename task, not a request to author a new function body.

Use only this instrumented tool through functions.exec / tools.exec_command. Every command must call this exact step with one JSON request on stdin. No raw source reads, direct project commands, browsing, edits, delegation or human correction:
python3 {ROOT / 'tools/upstream-rename-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
- {{"tool":"guide"}} obtains a ready change route and its target.
- {{"tool":"preview"}} follows the guide's ready rename preview. Inspect the reported source changes and warnings.
- {{"tool":"review"}} binds the authored rename operation, checks and patch delivery to a complete native review. Inspect all changed paths in its diff.
- {{"tool":"execute"}} executes that unchanged review once.
- {{"tool":"finish","answer":{{"files":["regex-cli/args/patterns.rs","regex-syntax/src/lib.rs","src/lib.rs"],"checks":["upstream","cli","minimal"],"public_facade_preserved":true}}}} reports only after execution passes.

The evaluator independently checks the pinned input, exact three-file change, all delivery stages, a separate receiver patch replay and compiled behavior. If a stage fails, do not claim success.
"""


def prepare(session: Path, binary: Path) -> dict[str, Any]:
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    regex_workspace.unpack(project)
    (project / ".fr").mkdir()
    save(project / ".fr/checks.json", {"schema": 1, "checks": CHECKS})
    (project / "artifacts").mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned upstream"], cwd=project, check=True)
    selected = {"schema": "fr-upstream-rename-session-1", "upstream_commit": regex_workspace.COMMIT,
                "archive_sha256": regex_workspace.ARCHIVE_SHA, "lock_sha256": regex_workspace.LOCK_SHA,
                "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
                "source_files": source_files(project), "source_sha256": source_digest(project),
                "checks_sha256": sha((project / ".fr/checks.json").read_bytes()),
                "goal": goal().to_data(), "evaluator": binding(), "manual_corrections": 0}
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = object_map(json.loads((session / "session.json").read_text()), "session")
    if (value.get("schema") != "fr-upstream-rename-session-1" or value.get("evaluator") != binding()
            or value.get("upstream_commit") != regex_workspace.COMMIT
            or value.get("archive_sha256") != regex_workspace.ARCHIVE_SHA
            or value.get("lock_sha256") != regex_workspace.LOCK_SHA
            or value.get("goal") != goal().to_data() or value.get("manual_corrections") != 0
            or sha(Path(value["binary"]).read_bytes()) != value.get("binary_sha256")
            or sha((session / "project/.fr/checks.json").read_bytes()) != value.get("checks_sha256")):
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
    sequence = ("guide", "preview", "review", "execute", "finish")
    if len(prior) >= len(sequence) or request.get("tool") != sequence[len(prior)]:
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
        report = client.follow_guide(guide.actions()[0])
        full = dict(report.to_data())
        visible = {"files_changed": full.get("files_changed"), "changes": full.get("changes"),
                   "definition_edits": full.get("definition_edits"), "reference_edits": full.get("reference_edits"),
                   "warnings_count": len(full.get("warnings", []))}
    elif kind in ("review", "execute"):
        guide = client.guide(goal())
        review = client.review_guide(guide, action())
        operation = review.at()
        if kind == "review":
            full = dict(operation)
            visible = {"ready": operation["ready"], "diff": operation["diff"],
                       "checks": operation["checks"], "stages": operation["stages"],
                       "exact_changes_sha256": operation["exact_changes_sha256"],
                       "review_sha256": review.review_sha256,
                       "warnings_count": len(operation["plan"]["planner"]["warnings"])}
        else:
            if review.review_sha256 != prior[2]["visible"]["review_sha256"]:
                raise ValueError("review changed before execution")
            full = dict(client.execute_guide(review).to_data())
            visible = {"passed": full["passed"], "transaction": full["transaction"],
                       "stages": [{"stage": item["stage"], "status": item["status"]}
                                  for item in full["workflow"]["stages"]],
                       "patch": full["workflow"]["stages"][-1]["result"]}
    else:
        answer = object_map(request.get("answer"), "answer")
        if len(json.dumps(answer).encode()) > 1024:
            raise ValueError("answer is too large")
        visible = {"finished": True, "answer": answer}
    if len(json.dumps(visible).encode()) > 20000:
        raise ValueError("visible report exceeds limit")
    row: Event = {"request": request, "visible": visible, "full": full,
                  "source_sha256": source_digest(project)}
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    return visible


def codex_command(codex: Path, session: Path, model: str, effort: str, tier: str) -> list[str]:
    return [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
            "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
            "--add-dir", str(session), "--model", model, "--config", f'model_reasoning_effort="{effort}"',
            "--config", f'service_tier="{tier}"', "--cd", str(session / "project"),
            "--output-last-message", str(session / "codex-final.txt"), "-"]


def run(session: Path, codex: Path, model: str, effort: str, tier: str, timeout: int) -> dict[str, Any]:
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
    record = {"schema": "fr-upstream-rename-run-1", "codex_version": version, "model": model,
              "reasoning_effort": effort, "service_tier": tier, "ephemeral": True,
              "ignored_user_config": True, "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
              "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def independent_oracle(project: Path, baseline: dict[str, str], patch: Path) -> dict[str, Any]:
    current = source_files(project)
    changed = sorted(name for name in baseline if current.get(name) != baseline[name])
    expected = sorted(CHANGED)
    text = {name: (project / name).read_text() for name in CHANGED}
    exact = (changed == expected
             and "pub fn quote_regex(text: &str) -> String {" in text["regex-syntax/src/lib.rs"]
             and "pub fn escape(text: &str) -> String {" not in text["regex-syntax/src/lib.rs"]
             and "quote_regex(r\"\\.+*?()|[]{}^$#&-~\")" in text["regex-syntax/src/lib.rs"]
             and "regex_syntax::quote_regex(p)" in text["regex-cli/args/patterns.rs"]
             and "pub fn escape(pattern: &str)" in text["src/lib.rs"]
             and "regex_syntax::quote_regex(pattern)" in text["src/lib.rs"])
    with tempfile.TemporaryDirectory(prefix="fr-rename-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        regex_workspace.unpack(receiver)
        (receiver / "Cargo.lock").write_bytes((project / "Cargo.lock").read_bytes())
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver,
                                 capture_output=True, timeout=30)
        replay_files = {name: sha((receiver / name).read_bytes()) for name in CHANGED} if applied.returncode == 0 else {}
        replay = applied.returncode == 0 and all(replay_files[name] == current[name] for name in CHANGED)
        build = subprocess.run(["cargo", "build", "-p", "regex", "-p", "regex-syntax", "--lib",
                                "--locked", "--offline"], cwd=receiver, capture_output=True, timeout=180)
        behavior = False
        detail = build.stderr.decode(errors="replace")[-1024:]
        if build.returncode == 0:
            oracle = Path(temporary) / "oracle.rs"
            oracle.write_text('''extern crate regex; extern crate regex_syntax;
fn reference(s: &str) -> String { s.chars().flat_map(|c| {
    if "\\\\.+*?()|[]{}^$#&-~".contains(c) { vec!['\\\\', c] } else { vec![c] }
}).collect() }
fn main() {
    let alphabet = ["", "abc", ".*", "\\\\", "[a-z]", "é🙂", "π#&-~", "a\\n b"];
    let mut cases = 0;
    for a in alphabet { for b in alphabet {
        let input = format!("{a}{b}"); let expected = reference(&input);
        assert_eq!(regex_syntax::quote_regex(&input), expected);
        assert_eq!(regex::escape(&input), expected); cases += 1;
    }}
    println!("{cases} independent cases passed");
}''')
            compiled = subprocess.run(["rustc", "--edition=2021", str(oracle),
                                       "--extern", f"regex={receiver / 'target/debug/libregex.rlib'}",
                                       "--extern", f"regex_syntax={receiver / 'target/debug/libregex_syntax.rlib'}",
                                       "-L", f"dependency={receiver / 'target/debug/deps'}",
                                       "-o", str(Path(temporary) / "oracle")], capture_output=True, timeout=60)
            detail = compiled.stderr.decode(errors="replace")[-1024:]
            if compiled.returncode == 0:
                executed = subprocess.run([str(Path(temporary) / "oracle")], capture_output=True, timeout=30)
                behavior = executed.returncode == 0 and executed.stdout.strip() == b"64 independent cases passed"
                detail = (executed.stdout + executed.stderr).decode(errors="replace")[-1024:]
    return {"changed_files": changed, "exact_source": exact, "receiver_patch_replay": replay,
            "behavior_64_cases": behavior, "oracle_detail": detail}


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    run_record = object_map(json.loads((session / "codex-run.json").read_text()), "run")
    codex_rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    command_prefix = str(ROOT / "tools/upstream-rename-agent.py") + " step "
    direct = [item["command"] for item in commands if command_prefix not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    review = rows[2]["full"] if len(rows) > 2 else None
    execution = rows[3]["full"] if len(rows) > 3 else None
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[4]["request"].get("answer", {}) if len(rows) > 4 else {}
    patch = session / "project/artifacts/rename.patch"
    oracle = independent_oracle(session / "project", selected["source_files"], patch) if patch.is_file() else {}
    result = {"schema": "fr-upstream-rename-result-1", "passed": False,
              "sequence": sequence, "guide_route": rows[0]["full"].get("route", {}).get("id") if rows else None,
              "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
              "review_ready": review.get("ready") if review else None,
              "execution_passed": execution.get("passed") if execution else None,
              "stages": [(item.get("stage"), item.get("status")) for item in stages],
              "oracle": oracle, "answer": answer,
              "codex": {"usage": usage, "command_executions": len(commands),
                        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
                        "direct_project_commands": direct, "billed_quota": {"available": False, "value": None}},
              "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (run_record["exit_code"] == 0 and not run_record["timed_out"]
                        and sequence == ["guide", "preview", "review", "execute", "finish"]
                        and result["guide_route"] == "direct-capability"
                        and rows[0]["full"].get("state") == "ready"
                        and rows[1]["full"].get("files_changed") == 3
                        and review is not None and review.get("ready") is True
                        and review.get("checks", {}).get("names") == list(CHECK_NAMES)
                        and all(f"--- a/{name}" in review["diff"] for name in CHANGED)
                        and execution is not None and execution.get("passed") is True
                        and [item.get("stage") for item in stages] == expected_stages
                        and all(item.get("status") == "passed" for item in stages)
                        and sorted(answer.get("files", [])) == sorted(CHANGED)
                        and answer.get("checks") == list(CHECK_NAMES)
                        and answer.get("public_facade_preserved") is True
                        and oracle.get("exact_source") and oracle.get("receiver_patch_replay")
                        and oracle.get("behavior_64_cases") and not direct
                        and result["codex"]["failed_command_executions"] == 0
                        and isinstance(usage, dict) and selected["manual_corrections"] == 0)
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool, reason: str | None) -> dict[str, Any]:
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
    manifest = {"schema": "fr-upstream-rename-manifest-1", "passed": result["passed"],
                "acceptance_evidence": result["passed"] and not diagnostic,
                "diagnostic_reason": reason if diagnostic else None,
                "upstream_commit": regex_workspace.COMMIT, "model": run_record["model"],
                "evaluator": config(session)["evaluator"], "files": files}
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, Any]:
    manifest = object_map(json.loads((destination / "manifest.json").read_text()), "manifest")
    for name, expected in object_map(manifest["files"], "files").items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result = json.loads((destination / "result.json").read_text())
    if manifest["passed"] != result["passed"] or manifest["acceptance_evidence"] and not result["passed"]:
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
    p = sub.add_parser("record"); p.add_argument("session", type=Path); p.add_argument("destination", type=Path)
    p.add_argument("--diagnostic", action="store_true"); p.add_argument("--reason")
    p = sub.add_parser("audit"); p.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.action == "prepare": result = prepare(args.session.resolve(), args.fr.resolve())
        elif args.action == "step":
            if not args.request_stdin: raise ValueError("step requires --request-stdin")
            result = step(args.session.resolve(), object_map(json.load(sys.stdin), "tool request"))
        elif args.action == "run":
            if not args.confirm_agent_spend: raise ValueError("--confirm-agent-spend is required")
            result = run(args.session.resolve(), args.codex, args.model, args.effort, args.service_tier, args.timeout)
        elif args.action == "score": result = score(args.session.resolve())
        elif args.action == "record": result = record(args.session.resolve(), args.destination.resolve(), args.diagnostic, args.reason)
        else: result = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, RuntimeError, json.JSONDecodeError,
            subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
