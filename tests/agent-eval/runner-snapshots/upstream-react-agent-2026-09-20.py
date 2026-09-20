#!/usr/bin/env python3
"""Record one guided TSX/Tailwind edit on a pinned MIT React project."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
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
ARCHIVE = ROOT / "tests/agent-eval/react-workspace.tar.gz"
COMMIT = "0633ab1ff90cd0a09b70c849718b9504500a7bd5"
SOURCE = "src/components/layouts/Header.tsx"
BEFORE = "text-lg font-medium text-black dark:text-white"
AFTER = "text-xl font-medium text-black dark:text-white"
MODEL, EFFORT, TIER = "gpt-5.6-luna", "low", "default"
STEPS = ("guide", "surface", "preview", "review", "execute", "finish")
FILES = ("session.json", "prompt.txt", "events.jsonl", "codex-events.jsonl",
         "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json")
CHECKS = [{"name": "typecheck", "argv": ["npm", "run", "typecheck"], "cwd": ".",
           "timeout_seconds": 90, "covers": ["pinned TypeScript source without writing build output"]}]


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


def evaluator_hash() -> str:
    return sha(Path(__file__).read_bytes())


def goal() -> AgentGoal:
    return AgentGoal("change", selector=GoalSelector(name="Header", scope=SOURCE, language="tsx"),
                     operation=GoalOperation("surface-edit", {"surface": "styles"}),
                     checks=("typecheck",), delivery=TaskDelivery(patch="artifacts/header.patch", check_output_bytes=1024),
                     context=GoalLimits(packet_limit=65536))


def unpack(project: Path) -> None:
    project.mkdir(parents=True)
    with tarfile.open(ARCHIVE, "r:gz") as archive:
        for member in archive.getmembers():
            if member.name.startswith("/") or ".." in Path(member.name).parts or not member.isfile():
                if member.isdir() and member.name != "." and ".." not in Path(member.name).parts:
                    continue
                raise ValueError("archive has an unsupported member")
            destination = project / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError("archive member is unreadable")
            destination.write_bytes(stream.read())


def prompt(session: Path) -> str:
    return f"""You are an acceptance agent in a pinned MIT-licensed React/Tailwind project.

Task: increase the header title's Tailwind font size from `text-lg` to `text-xl` in `{SOURCE}`. Keep the rest of the class list and all other source bytes unchanged. Use the guide's exact surface capability. Inspect the preview and complete review before execution. Report success only after all checks and delivery stages pass.

Use only the instrumented tool below through functions.exec / tools.exec_command. Every command calls this exact step with one JSON request on stdin. Do not read project files directly, run project commands, edit source, browse, delegate, or ask a human to correct the result:
python3 {ROOT / 'tools/upstream-react-agent.py'} step {session} --request-stdin <<'FRJSON'
{{"tool":"guide"}}
FRJSON

Allowed requests, in order:
1. {{"tool":"guide"}} selects the TSX surface route.
2. {{"tool":"surface"}} lists the header's exact class capabilities.
3. {{"tool":"preview","edit":"<the text-lg edit ID from surface>"}} previews `text-xl`.
4. {{"tool":"review","edit":"<same edit ID>"}} returns the complete diff and checks.
5. {{"tool":"execute","edit":"<same edit ID>"}} executes the unchanged review.
6. {{"tool":"finish","answer":{{"file":"{SOURCE}","from":"text-lg","to":"text-xl","check":"typecheck"}}}} finishes after execution passes.

The evaluator separately checks exact source, receiver patch replay, TypeScript/Vite build, and generated Tailwind CSS. Do not claim success if any stage fails.
"""


def prepare(session: Path, binary: Path, deps: Path) -> dict[str, Any]:
    if not (deps / "react").is_dir() or not (deps / ".bin/tsc").exists():
        raise ValueError("--deps needs installed dependencies from the pinned lockfile")
    session.mkdir(parents=True, exist_ok=False)
    frozen = session / "fr-bin"
    shutil.copyfile(binary, frozen)
    frozen.chmod(0o555)
    project = session / "project"
    unpack(project)
    (project / "node_modules").symlink_to(deps.resolve(), target_is_directory=True)
    (project / ".fr").mkdir()
    save(project / ".fr/checks.json", {"schema": 1, "checks": CHECKS})
    (project / "artifacts").mkdir()
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "--all"], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Eval", "-c", "user.email=eval@example.invalid",
                    "commit", "-qm", "pinned upstream"], cwd=project, check=True)
    selected = {"schema": "fr-upstream-react-session-1", "upstream_commit": COMMIT,
                "archive_sha256": sha(ARCHIVE.read_bytes()), "lock_sha256": sha((project / "pnpm-lock.yaml").read_bytes()),
                "binary": str(frozen), "binary_sha256": sha(frozen.read_bytes()),
                "deps": str(deps.resolve()), "source_files": tracked(project), "goal": goal().to_data(),
                "evaluator_sha256": evaluator_hash(), "manual_corrections": 0}
    save(session / "session.json", selected)
    (session / "prompt.txt").write_text(prompt(session))
    return selected


def config(session: Path) -> dict[str, Any]:
    value = read(session / "session.json")
    if (value.get("schema") != "fr-upstream-react-session-1" or value.get("upstream_commit") != COMMIT
            or value.get("archive_sha256") != sha(ARCHIVE.read_bytes())
            or value.get("evaluator_sha256") != evaluator_hash() or value.get("goal") != goal().to_data()
            or value.get("manual_corrections") != 0
            or sha(Path(value["binary"]).read_bytes()) != value.get("binary_sha256")
            or sha((session / "project/pnpm-lock.yaml").read_bytes()) != value.get("lock_sha256")):
        raise ValueError("session binding changed")
    return value


def events(session: Path) -> list[dict[str, Any]]:
    path = session / "events.jsonl"
    return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []


def edit_id(prior: list[dict[str, Any]]) -> str:
    if len(prior) < 2:
        raise ValueError("surface report is missing")
    surface = prior[1]["full"]
    matching = [item["edit"]["id"] for item in surface["items"]
                if item.get("class_use", {}).get("name") == "text-lg"
                and item.get("source", {}).get("path") == SOURCE]
    if len(matching) != 1:
        raise ValueError("surface did not expose one exact text-lg capability")
    return matching[0]


def action(edit: str) -> TaggedIntentAction:
    return TaggedIntentAction(SurfaceEditOperation(edit, "text-xl", ("typecheck",), goal().delivery),
                              diff_bytes=65536, report_bytes=65536)


def step(session: Path, request: dict[str, Any]) -> dict[str, Any]:
    selected = config(session)
    prior = events(session)
    if len(prior) >= len(STEPS) or request.get("tool") != STEPS[len(prior)]:
        raise ValueError("tool sequence differs from pinned workflow")
    kind = request["tool"]
    if kind in ("preview", "review", "execute") and request.get("edit") != edit_id(prior):
        raise ValueError("request does not use the returned text-lg edit ID")
    project = session / "project"
    client = FrClient(str(project), executable=selected["binary"])
    guide = client.guide(goal()) if kind != "finish" else None
    full: dict[str, Any] | None = None
    if kind == "guide":
        assert guide is not None
        full = guide.to_data()
        visible = {key: full[key] for key in ("state", "route", "target", "basis")}
        visible["actions"] = [item.to_data() for item in guide.actions()]
    elif kind == "surface":
        assert guide is not None
        full = client.follow_guide(guide.actions()[0]).to_data()
        visible = {"query": full["query"], "items": [
            {"class_use": item.get("class_use"), "edit": item.get("edit"), "source": item.get("source")}
            for item in full["items"]]}
    elif kind == "preview":
        assert guide is not None
        full = client.follow_guide(guide.actions()[1], GuideInputs({"edit-id": request["edit"], "to": "text-xl"})).to_data()
        visible = {key: full.get(key) for key in ("applied", "path", "from", "to", "occurrences", "changes", "warnings")}
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
            full = client.execute_guide(review).to_data()
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
    row = {"request": request, "visible": visible, "full": full, "source_files": tracked(project)}
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
    record = {"schema": "fr-upstream-react-run-1", "codex_version": version, "model": MODEL,
              "reasoning_effort": EFFORT, "service_tier": TIER, "ephemeral": True,
              "ignored_user_config": True, "ignored_rules": True, "sandbox": "workspace-write",
              "prompt_sha256": sha((session / "prompt.txt").read_bytes()),
              "events_sha256": sha((session / "codex-events.jsonl").read_bytes()),
              "stderr_sha256": sha((session / "codex-stderr.txt").read_bytes()),
              "elapsed_seconds": time.time() - started, "exit_code": exit_code,
              "timed_out": timed_out, "command": command}
    save(session / "codex-run.json", record)
    return record


def oracle(session: Path, selected: dict[str, Any]) -> dict[str, Any]:
    project = session / "project"
    current = tracked(project)
    changed = sorted(name for name in selected["source_files"] if current.get(name) != selected["source_files"][name])
    with tempfile.TemporaryDirectory(prefix="fr-react-receiver-") as temporary:
        receiver = Path(temporary) / "project"
        unpack(receiver)
        receiver.joinpath("node_modules").symlink_to(selected["deps"], target_is_directory=True)
        baseline = (receiver / SOURCE).read_bytes()
        exact = (changed == [SOURCE] and baseline.count(BEFORE.encode()) == 1
                 and (project / SOURCE).read_bytes() == baseline.replace(BEFORE.encode(), AFTER.encode()))
        patch = project / "artifacts/header.patch"
        applied = subprocess.run(["git", "apply", str(patch)], cwd=receiver, capture_output=True, timeout=30)
        replay = applied.returncode == 0 and (receiver / SOURCE).read_bytes() == (project / SOURCE).read_bytes()
        build = subprocess.run(["npm", "run", "build"], cwd=receiver, capture_output=True, timeout=90)
        css_files = list((receiver / "dist/assets").glob("*.css"))
        css = "\n".join(path.read_text() for path in css_files)
        generated = build.returncode == 0 and ".text-xl" in css
        return {"changed_files": changed, "exact_source": exact, "receiver_patch_replay": replay,
                "receiver_build": build.returncode == 0, "generated_text_xl_css": generated,
                "build_detail": (build.stdout + build.stderr).decode(errors="replace")[-1024:]}


def score(session: Path) -> dict[str, Any]:
    selected = config(session)
    rows = events(session)
    record = read(session / "codex-run.json")
    codex_rows = [json.loads(line) for line in (session / "codex-events.jsonl").read_text().splitlines() if line]
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    direct = [item["command"] for item in commands if str(ROOT / "tools/upstream-react-agent.py") + " step " not in item.get("command", "")]
    sequence = [row["request"].get("tool") for row in rows]
    guide = rows[0]["full"] if rows else {}
    preview = rows[2]["full"] if len(rows) > 2 else {}
    review = rows[3]["full"] if len(rows) > 3 else {}
    execution = rows[4]["full"] if len(rows) > 4 else {}
    stages = execution.get("workflow", {}).get("stages", []) if execution else []
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    answer = rows[5]["request"].get("answer", {}) if len(rows) > 5 else {}
    independent = oracle(session, selected) if (session / "project/artifacts/header.patch").is_file() else {}
    result = {"schema": "fr-upstream-react-result-1", "passed": False, "sequence": sequence,
              "guide_route": guide.get("route", {}).get("id"), "preview": preview,
              "reviewed_diff_sha256": sha(review["diff"].encode()) if review else None,
              "review_ready": review.get("ready") if review else None,
              "execution_passed": execution.get("passed") if execution else None,
              "stages": [(item.get("stage"), item.get("status")) for item in stages],
              "oracle": independent, "answer": answer,
              "codex": {"usage": usage, "command_executions": len(commands),
                        "failed_command_executions": sum(item.get("exit_code") != 0 for item in commands),
                        "direct_project_commands": direct, "billed_quota": {"available": False, "value": None}},
              "manual_corrections": selected["manual_corrections"]}
    result["passed"] = (record["exit_code"] == 0 and not record["timed_out"]
                        and sequence == list(STEPS) and guide.get("state") == "ready"
                        and result["guide_route"] == "surface-edit"
                        and preview.get("applied") is False and review.get("ready") is True
                        and review.get("checks", {}).get("names") == ["typecheck"]
                        and f"--- a/{SOURCE}" in review.get("diff", "")
                        and execution.get("passed") is True
                        and [item.get("stage") for item in stages] == expected_stages
                        and all(item.get("status") == "passed" for item in stages)
                        and answer == {"file": SOURCE, "from": "text-lg", "to": "text-xl", "check": "typecheck"}
                        and independent.get("exact_source") and independent.get("receiver_patch_replay")
                        and independent.get("receiver_build") and independent.get("generated_text_xl_css")
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
    manifest = {"schema": "fr-upstream-react-manifest-1", "passed": result["passed"],
                "acceptance_evidence": result["passed"] and not diagnostic,
                "diagnostic_reason": reason if diagnostic else None,
                "upstream_commit": COMMIT, "model": MODEL,
                "evaluator_sha256": evaluator_hash(), "files": files}
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare"); p.add_argument("session", type=Path)
    p.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    p.add_argument("--deps", type=Path, required=True)
    p = sub.add_parser("step"); p.add_argument("session", type=Path); p.add_argument("--request-stdin", action="store_true")
    p = sub.add_parser("run"); p.add_argument("session", type=Path)
    p.add_argument("--codex", type=Path, default=Path("codex"))
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
            value = step(args.session.resolve(), read_stdin())
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


def read_stdin() -> dict[str, Any]:
    value = json.load(sys.stdin)
    if not isinstance(value, dict):
        raise ValueError("tool request must be an object")
    return value


if __name__ == "__main__":
    main()
