#!/usr/bin/env python3
"""Record one guided Mermaid node edit on pinned MIT Micromaid source."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "sdk/python/src"))

from fr_ir.guide import AgentGoal, GoalLimits, GoalOperation, GoalSelector, GuideInputs
from fr_ir.intent_actions import SurfaceEditOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient


ROOT = Path(__file__).resolve().parents[1]
ARCHIVE = ROOT / "tests/agent-eval/micromaid-workspace.tar.gz"
ORACLE_LOCK = ROOT / "tests/agent-eval/mermaid-oracle/package-lock.json"
COMMIT = "e6e49600ad1e86f1a5bb1e375049534e5a6e2961"
SOURCE = "README.md"
HEADING = "Viewing mermaid diagrams embedded in markdown documents on Pharo"
OLD, NEW = "E", "EvalStep"
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
DELIVERY = TaskDelivery(patch="artifacts/flowchart.patch", check_output_bytes=1024)
STEPS = ("guide", "surface", "preview", "review", "execute", "finish")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
CHECKS = [{"name": "mermaid", "argv": ["node", ".fr/check.mjs"], "cwd": ".",
           "timeout_seconds": 30, "covers": ["both README Mermaid flowcharts parse with pinned Mermaid 11.12.2"]}]
CHECK_SCRIPT = '''import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';

const dom = new JSDOM('<!doctype html><html><body></body></html>');
globalThis.window = dom.window;
globalThis.document = dom.window.document;
const { default: mermaid } = await import('mermaid');
const source = readFileSync('README.md', 'utf8');
const diagrams = [...source.matchAll(/^```mermaid\\n([\\s\\S]*?)^```/gm)];
if (diagrams.length !== 2) throw new Error('expected two Mermaid examples');
for (const diagram of diagrams) {
  const parsed = await mermaid.parse(diagram[1]);
  if (parsed.diagramType !== 'flowchart-v2') throw new Error('not a flowchart');
}
console.log('two flowcharts parsed');
'''


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def read(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not an object")
    return value


def tracked(project: Path) -> dict[str, str]:
    names = subprocess.check_output(["git", "ls-files", "-z"], cwd=project).decode().split("\0")
    return {name: sha((project / name).read_bytes()) for name in names if name and not name.startswith(".fr/")}


def goal() -> AgentGoal:
    return AgentGoal("change", selector=GoalSelector(name=HEADING, scope=SOURCE, language="markdown"),
                     operation=GoalOperation("surface-edit", {"surface": "diagrams"}),
                     checks=("mermaid",), delivery=DELIVERY, context=GoalLimits(packet_limit=65536))


def unpack(project: Path) -> None:
    project.mkdir(parents=True)
    with tarfile.open(ARCHIVE, "r:gz") as archive:
        for member in archive.getmembers():
            if member.isdir() and not member.name.startswith("/") and ".." not in Path(member.name).parts:
                continue
            if not member.isfile() or member.name.startswith("/") or ".." in Path(member.name).parts:
                raise ValueError("archive has an unsupported member")
            destination = project / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError("archive member is unreadable")
            destination.write_bytes(stream.read())


def prompt(session: Path) -> str:
    return f"""You are an acceptance agent in pinned MIT-licensed Micromaid source.

Task: rename node `{OLD}` to `{NEW}` in the rendered REPL Mermaid flowchart under the heading `{HEADING}` in `README.md`. Keep its visible `Eval` label and all five edges. The later quoted Microdown code sample is outside the selected diagram; leave it and every other source byte unchanged. Inspect the preview diff and complete review before execution. Report success only after checks and delivery stages pass.

Use only this instrumented tool through functions.exec / tools.exec_command. Every command calls the exact step with one JSON request on stdin. No direct source reads, project commands, edits, browsing, delegation or human correction:
python3 {ROOT / 'tools/upstream-mermaid-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
1. {{"tool":"guide"}} selects the Markdown diagram surface route.
2. {{"tool":"surface"}} lists exact Mermaid node edit capabilities.
3. {{"tool":"preview","edit":"<E edit ID from surface>"}} previews `{NEW}`.
4. {{"tool":"review","edit":"<same edit ID>"}} returns the complete diff and checks.
5. {{"tool":"execute","edit":"<same edit ID>"}} executes the unchanged review.
6. {{"tool":"finish","answer":{{"file":"README.md","node":"E","new_node":"EvalStep","edges":5,"check":"mermaid"}}}} finishes after execution passes.

The evaluator checks exact source bytes, all delivery stages, receiver patch replay, parser admission and the independent graph oracle. Do not claim success if a stage fails.
"""


def prepare(session: Path, binary: Path, deps: Path) -> dict[str, Any]:
    if not (deps / "mermaid").is_dir() or not (deps / "jsdom").is_dir():
        raise ValueError("--deps needs npm ci from the pinned parser lockfile")
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    unpack(project)
    (project / "node_modules").symlink_to(deps.resolve(), target_is_directory=True)
    (project / ".fr").mkdir()
    (project / ".fr/check.mjs").write_text(CHECK_SCRIPT)
    save(project / ".fr/checks.json", {"schema": 1, "checks": CHECKS})
    (project / "artifacts").mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    with (project / ".git/info/exclude").open("a") as stream:
        stream.write("\nnode_modules\n")
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned upstream"], cwd=project, check=True)
    selected = {"schema": "fr-upstream-mermaid-session-1", "upstream_commit": COMMIT,
                "archive_sha256": sha(ARCHIVE.read_bytes()), "oracle_lock_sha256": sha(ORACLE_LOCK.read_bytes()),
                "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
                "deps": str(deps.resolve()), "check_script_sha256": sha(CHECK_SCRIPT.encode()),
                "checks_sha256": sha((project / ".fr/checks.json").read_bytes()),
                "source_files": tracked(project), "goal": goal().to_data(),
                "evaluator_sha256": sha(Path(__file__).read_bytes()), "manual_corrections": 0}
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = read(session / "session.json")
    if (value.get("schema") != "fr-upstream-mermaid-session-1" or value.get("upstream_commit") != COMMIT
            or value.get("archive_sha256") != sha(ARCHIVE.read_bytes())
            or value.get("oracle_lock_sha256") != sha(ORACLE_LOCK.read_bytes())
            or value.get("evaluator_sha256") != sha(Path(__file__).read_bytes())
            or value.get("goal") != goal().to_data() or value.get("manual_corrections") != 0
            or value.get("check_script_sha256") != sha((session / "project/.fr/check.mjs").read_bytes())
            or value.get("checks_sha256") != sha((session / "project/.fr/checks.json").read_bytes())
            or value.get("binary_sha256") != sha(Path(value["binary"]).read_bytes())):
        raise ValueError("session binding changed")
    return value


def events(session: Path) -> list[dict[str, Any]]:
    path = session / "events.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def edit_id(prior: list[dict[str, Any]]) -> str:
    if len(prior) < 2:
        raise ValueError("surface report is missing")
    matches = [item["edit"]["id"] for item in prior[1]["full"]["items"]
               if item.get("kind") == "mermaid-node" and item.get("node", {}).get("name") == OLD
               and item.get("edit", {}).get("occurrences") == 4]
    if len(matches) != 1:
        raise ValueError("surface did not expose one exact E node capability")
    return matches[0]


def action(edit: str) -> TaggedIntentAction:
    return TaggedIntentAction(SurfaceEditOperation(edit, NEW, ("mermaid",), DELIVERY),
                              diff_bytes=65536, report_bytes=65536)


def step(session: Path, request: dict[str, Any]) -> dict[str, Any]:
    selected = config(session)
    prior = events(session)
    if len(prior) >= len(STEPS) or request.get("tool") != STEPS[len(prior)]:
        raise ValueError("tool sequence differs from pinned workflow")
    kind = request["tool"]
    if kind in ("preview", "review", "execute") and request.get("edit") != edit_id(prior):
        raise ValueError("request does not use the returned E edit ID")
    client = FrClient(str(session / "project"), executable=selected["binary"])
    guide = client.guide(goal()) if kind != "finish" else None
    full: dict[str, Any] | None = None
    if kind == "guide":
        assert guide is not None
        full = guide.to_data()
        visible = {key: full[key] for key in ("state", "route", "target", "basis")}
        visible["actions"] = [item.to_data() for item in guide.actions()]
    elif kind == "surface":
        assert guide is not None
        guided_page = client.follow_guide(guide.actions()[0]).to_data()
        expanded = client.call("project", "diagrams", guide.at("/target/handle"), "--limit", "32").to_data()
        if expanded.get("revision") != guided_page.get("revision") or expanded.get("page", {}).get("next") is not None:
            raise ValueError("bounded surface expansion changed the guide's project view")
        full = {**expanded, "guided_page": guided_page}
        visible = {"query": full["query"], "guided_page_returned": guided_page["page"]["returned"],
                   "expanded_page_total": full["page"]["total"], "items": [
            {"kind": item.get("kind"), "node": item.get("node"), "edit": item.get("edit"),
             "source": item.get("source")} for item in full["items"]]}
    elif kind == "preview":
        assert guide is not None
        full = client.follow_guide(guide.actions()[1], GuideInputs({"edit-id": request["edit"], "to": NEW})).to_data()
        visible = {key: full.get(key) for key in ("applied", "changed", "path", "edit", "diff",
                                                  "occurrences", "preservation", "validation")}
    elif kind in ("review", "execute"):
        assert guide is not None
        review = client.review_guide(guide, action(request["edit"]))
        if kind == "review":
            full = dict(review.at())
            visible = {"ready": full["ready"], "diff": full["diff"], "checks": full["checks"],
                       "stages": full["stages"], "review_sha256": review.review_sha256}
        else:
            if review.review_sha256 != prior[3]["visible"]["review_sha256"]:
                raise ValueError("review changed before execution")
            full = dict(client.execute_guide(review).to_data())
            visible = {"passed": full["passed"], "transaction": full["transaction"],
                       "stages": [{"stage": item["stage"], "status": item["status"]}
                                  for item in full["workflow"]["stages"]]}
    else:
        answer = request.get("answer")
        if not isinstance(answer, dict) or len(json.dumps(answer)) > 1024:
            raise ValueError("finish answer is invalid")
        visible = {"finished": True, "answer": answer}
    if len(json.dumps(visible).encode()) > 20000:
        raise ValueError("visible report exceeds limit")
    row = {"request": request, "visible": visible, "full": full,
           "source_files": tracked(session / "project")}
    with (session / "events.jsonl").open("a") as stream:
        stream.write(json.dumps(row, ensure_ascii=False) + "\n")
    return visible


def run(session: Path, codex: Path, timeout: int) -> dict[str, Any]:
    config(session)
    if (session / "events.jsonl").exists() or (session / "codex-run.json").exists():
        raise ValueError("live session must be fresh")
    version = subprocess.check_output([codex, "--version"], text=True).strip()
    command = [str(codex), "exec", "--ephemeral", "--ignore-user-config", "--ignore-rules",
               "--skip-git-repo-check", "--json", "--color", "never", "--sandbox", "workspace-write",
               "--add-dir", str(session), "--model", MODEL,
               "--config", f'model_reasoning_effort="{EFFORT}"',
               "--config", f'service_tier="{TIER}"', "--cd", str(session / "project"),
               "--output-last-message", str(session / "codex-final.txt"), "-"]
    started = time.time()
    with (session / "codex-events.jsonl").open("wb") as stdout, (session / "codex-stderr.txt").open("wb") as stderr:
        try:
            completed = subprocess.run(command, input=(session / "prompt.txt").read_bytes(),
                                       stdout=stdout, stderr=stderr, timeout=timeout)
            exit_code, timed_out = completed.returncode, False
        except subprocess.TimeoutExpired:
            exit_code, timed_out = 124, True
    record = {"schema": "fr-upstream-mermaid-run-1", "codex_version": version, "model": MODEL,
              "reasoning_effort": EFFORT, "service_tier": TIER, "ephemeral": True,
              "ignored_user_config": True, "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
              "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def independent_oracle(session: Path, selected: dict[str, Any]) -> dict[str, Any]:
    project = session / "project"
    current = tracked(project)
    changed = sorted(name for name in selected["source_files"] if current.get(name) != selected["source_files"][name])
    with tempfile.TemporaryDirectory(prefix="fr-mermaid-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        unpack(receiver)
        before = (receiver / SOURCE).read_text()
        blocks = list(re.finditer(r"(?m)^```mermaid\n([\s\S]*?)^```", before))
        first = blocks[0].group(1) if len(blocks) == 2 else ""
        renamed = re.sub(r"\bE\b", NEW, first)
        expected = before[:blocks[0].start(1)] + renamed + before[blocks[0].end(1):] if len(blocks) == 2 else ""
        exact = (changed == [SOURCE] and first.count("E") >= 4
                 and len(re.findall(r"\bE\b", first)) == 4
                 and (project / SOURCE).read_text() == expected)
        patch = project / "artifacts/flowchart.patch"
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver, capture_output=True, timeout=30)
        replay = applied.returncode == 0 and (receiver / SOURCE).read_bytes() == (project / SOURCE).read_bytes()
        graph = ("EvalStep[Eval]" in renamed and "R -->  EvalStep" in renamed
                 and "EvalStep -->|quit| Q([quit])" in renamed and "EvalStep --> P" in renamed
                 and "A([Init]) --> R" in renamed and "P --> R" in renamed
                 and renamed.count("EvalStep") == 4)
        (receiver / "node_modules").symlink_to(selected["deps"], target_is_directory=True)
        (receiver / ".fr").mkdir()
        (receiver / ".fr/check.mjs").write_text(CHECK_SCRIPT)
        parsed = subprocess.run(["node", ".fr/check.mjs"], cwd=receiver, capture_output=True, timeout=30)
        return {"changed_files": changed, "exact_source": exact, "receiver_patch_replay": replay,
                "graph_preserved": graph, "mermaid_parser_passed": parsed.returncode == 0,
                "parser_detail": (parsed.stdout + parsed.stderr).decode(errors="replace")[-1024:]}


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    record = read(session / "codex-run.json")
    codex_rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    direct = [item["command"] for item in commands if str(ROOT / "tools/upstream-mermaid-agent.py") + " step " not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    guide = rows[0]["full"] if rows else {}
    preview = rows[2]["full"] if len(rows) > 2 else {}
    review = rows[3]["full"] if len(rows) > 3 else {}
    execution = rows[4]["full"] if len(rows) > 4 else {}
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[5]["request"].get("answer", {}) if len(rows) > 5 else {}
    oracle = independent_oracle(session, selected) if (session / "project/artifacts/flowchart.patch").is_file() else {}
    result = {"schema": "fr-upstream-mermaid-result-1", "passed": False, "sequence": sequence,
              "guide_route": guide.get("route", {}).get("id"),
              "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
              "review_ready": review.get("ready") if review else None,
              "execution_passed": execution.get("passed") if execution else None,
              "stages": [(item.get("stage"), item.get("status")) for item in stages],
              "oracle": oracle, "answer": answer,
              "codex": {"usage": usage, "command_executions": len(commands),
                        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
                        "direct_project_commands": direct, "billed_quota": {"available": False, "value": None}},
              "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (record["exit_code"] == 0 and not record["timed_out"]
                        and sequence == list(STEPS) and guide.get("state") == "ready"
                        and result["guide_route"] == "surface-edit"
                        and preview.get("applied") is False and preview.get("changed") is True
                        and preview.get("diff") == review.get("diff") and review.get("ready") is True
                        and review.get("checks", {}).get("names") == ["mermaid"]
                        and f"--- a/{SOURCE}" in review.get("diff", "")
                        and execution.get("passed") is True
                        and [item.get("stage") for item in stages] == expected_stages
                        and all(item.get("status") == "passed" for item in stages)
                        and answer == {"file": SOURCE, "node": OLD, "new_node": NEW, "edges": 5, "check": "mermaid"}
                        and all(oracle.get(name) is True for name in
                                ("exact_source", "receiver_patch_replay", "graph_preserved", "mermaid_parser_passed"))
                        and not direct and result["codex"]["failed_command_executions"] == 0
                        and isinstance(usage, dict) and selected["manual_corrections"] == 0)
    save(session / "result.json", result)
    return result


def record(session: Path, destination: Path, diagnostic: bool, reason: str | None) -> dict[str, Any]:
    result = read(session / "result.json")
    if not result["passed"] and not diagnostic:
        raise ValueError("failed trial can only be recorded as diagnostic")
    if diagnostic and not reason:
        raise ValueError("diagnostic retention requires --reason")
    destination.mkdir(parents=True, exist_ok=False)
    files = {}
    for name in FILES:
        if (session / name).exists():
            shutil.copyfile(session / name, destination / name)
            files[name] = sha((destination / name).read_bytes())
    manifest = {"schema": "fr-upstream-mermaid-manifest-1", "passed": result["passed"],
                "acceptance_evidence": result["passed"] and not diagnostic,
                "diagnostic_reason": reason if diagnostic else None,
                "upstream_commit": COMMIT, "model": MODEL,
                "evaluator_sha256": sha(Path(__file__).read_bytes()), "files": files}
    save(destination / "manifest.json", manifest)
    return manifest


def audit(destination: Path) -> dict[str, Any]:
    manifest = read(destination / "manifest.json")
    for name, expected in manifest["files"].items():
        if sha((destination / name).read_bytes()) != expected:
            raise ValueError(f"retained artifact changed: {name}")
    result = read(destination / "result.json")
    if manifest["passed"] != result["passed"] or manifest["acceptance_evidence"] and not result["passed"]:
        raise ValueError("retained outcome classification disagrees with score")
    return {"verified": True, "passed": manifest["passed"], "acceptance_evidence": manifest["acceptance_evidence"]}


def stdin_request() -> dict[str, Any]:
    value = json.load(sys.stdin)
    if not isinstance(value, dict):
        raise ValueError("tool request must be an object")
    return value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare"); p.add_argument("session", type=Path)
    p.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr"); p.add_argument("--deps", type=Path, required=True)
    p = sub.add_parser("step"); p.add_argument("session", type=Path); p.add_argument("--request-stdin", action="store_true")
    p = sub.add_parser("run"); p.add_argument("session", type=Path); p.add_argument("--codex", type=Path, default=Path("codex"))
    p.add_argument("--timeout", type=int, default=1800); p.add_argument("--confirm-agent-spend", action="store_true")
    p = sub.add_parser("score"); p.add_argument("session", type=Path)
    p = sub.add_parser("record"); p.add_argument("session", type=Path); p.add_argument("destination", type=Path)
    p.add_argument("--diagnostic", action="store_true"); p.add_argument("--reason")
    p = sub.add_parser("audit"); p.add_argument("destination", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "prepare": value = prepare(args.session.resolve(), args.fr.resolve(), args.deps.resolve())
        elif args.command == "step":
            if not args.request_stdin:
                raise ValueError("step requires --request-stdin")
            value = step(args.session.resolve(), stdin_request())
        elif args.command == "run":
            if not args.confirm_agent_spend:
                raise ValueError("--confirm-agent-spend is required")
            value = run(args.session.resolve(), args.codex, args.timeout)
        elif args.command == "score": value = score(args.session.resolve())
        elif args.command == "record": value = record(args.session.resolve(), args.destination.resolve(), args.diagnostic, args.reason)
        else: value = audit(args.destination.resolve())
    except (OSError, ValueError, KeyError, IndexError, RuntimeError, json.JSONDecodeError,
            subprocess.TimeoutExpired) as error:
        parser.error(str(error))
    print(json.dumps(value, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
