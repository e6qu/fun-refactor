#!/usr/bin/env python3
"""Prepare and score a direct-JSON versus Python-SDK Codex pair."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "sdk/python/src"))

from fr_ir import BinaryOp, Expr, SemanticBody, Stmt, Type  # noqa: E402


def expected() -> dict:
    return SemanticBody([
        Stmt.Let("total", Type.Int(), Expr.Int(0), True),
        Stmt.ForEach("item", Expr.Name("values"), [
            Stmt.If(
                Expr.Binary(BinaryOp.GT, Expr.Name("item"), Expr.Int(0)),
                [Stmt.Assign(Expr.Name("total"), Expr.Binary(BinaryOp.ADD, Expr.Name("total"), Expr.Name("item")))],
            )
        ]),
        Stmt.Return(Expr.Name("total")),
    ]).to_data()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(out: Path, binary: Path) -> None:
    if out.exists():
        raise ValueError(f"{out} already exists")
    trials = (("semantic-ir-sdk-fr", "fr"), ("semantic-ir-sdk-files", "files"))
    out.mkdir(parents=True)
    (out / "experiment.json").write_text(json.dumps({"project": "semantic-ir-sdk", "repetitions": 1, "trials": [name for name, _ in trials]}, indent=2) + "\n")
    task = "Construct change.json for this exact source-free body: mutable integer total starts at 0; for each item in values, add positive items to total; return total. Preserve these names and use fr-semantic-body-1. Validate the result with ./fr author validate-semantic --from change.json --canonical. Do not inspect fr or fr_ir implementation source, use the bounded semantic-schema command when details are needed, and stop after validation succeeds."
    for name, arm in trials:
        session = out / name
        project = session / "project"
        project.mkdir(parents=True)
        shutil.copy2(binary, project / "fr")
        (session / "session.json").write_text(json.dumps({"task": "semantic-ir-sdk", "arm": arm, "repetition": 1}, indent=2) + "\n")
        prompt = task
        if arm == "fr":
            shutil.copytree(ROOT / "sdk/python/src/fr_ir", project / "fr_ir", ignore=shutil.ignore_patterns("__pycache__"))
            prompt += " Use the local fr_ir Python SDK. Retain the short Python producer as make_change.py and run it to create change.json."
        else:
            prompt += " Construct change.json directly. Do not use or create a Python SDK producer."
        (session / "prompt.txt").write_text(prompt + "\n")
        (session / "expected.json").write_text(json.dumps(expected(), separators=(",", ":")) + "\n")


def usage(events: Path) -> dict:
    completed = None
    commands = 0
    for line in events.read_text().splitlines():
        event = json.loads(line)
        if event.get("type") == "item.completed" and event.get("item", {}).get("type") == "command_execution":
            commands += 1
        if event.get("type") == "turn.completed":
            completed = event.get("usage")
    return {"commands": commands, **(completed or {})}


def implementation_source_reads(events: Path) -> int:
    reads = 0
    for line in events.read_text().splitlines():
        event = json.loads(line)
        item = event.get("item", {})
        if event.get("type") != "item.completed" or item.get("type") != "command_execution":
            continue
        command = item.get("command", "")
        if "fr_ir/__init__.py" in command or "rg --files fr_ir" in command:
            reads += 1
    return reads


def canonical(binary: Path, project: Path, payload: Path) -> tuple[bool, dict]:
    result = subprocess.run([str(binary), "--json", "--no-cache", "-C", str(project), "author", "validate-semantic", "--from", str(payload), "--canonical"], capture_output=True, text=True)
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        report = {"stdout": result.stdout, "stderr": result.stderr}
    return result.returncode == 0, report


def score(sessions: Path) -> dict:
    reports = []
    for name in json.loads((sessions / "experiment.json").read_text())["trials"]:
        session = sessions / name
        config = json.loads((session / "session.json").read_text())
        project = session / "project"
        payload = project / "change.json"
        valid, report = canonical(project / "fr", project, payload) if payload.is_file() else (False, {})
        expected_value = json.loads((session / "expected.json").read_text())
        exact = valid and report.get("canonical") == expected_value
        producer = project / "make_change.py"
        run = json.loads((session / "codex-run.json").read_text())
        source_reads = implementation_source_reads(session / "codex-events.jsonl")
        result = {
            "schema": "fr-agent-ir-sdk-trial-result-1", "trial": name, "arm": config["arm"],
            "passed": exact and run.get("exit_code") == 0 and source_reads == 0,
            "valid": valid, "exact_canonical": exact, "python_producer": producer.is_file(),
            "implementation_source_reads": source_reads,
            "producer_bytes": producer.stat().st_size if producer.is_file() else payload.stat().st_size if payload.is_file() else 0,
            "usage": usage(session / "codex-events.jsonl"), "run_sha256": digest(session / "codex-run.json"),
        }
        (session / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        reports.append(result)
    summary = {"schema": "fr-agent-ir-sdk-trial-1", "passed": all(report["passed"] for report in reports), "results": reports}
    print(json.dumps(summary, indent=2))
    return summary


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    prepare_parser = sub.add_parser("prepare")
    prepare_parser.add_argument("--out", type=Path, required=True)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    score_parser = sub.add_parser("score")
    score_parser.add_argument("sessions", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.out, args.fr.resolve())
        return 0
    return 0 if score(args.sessions)["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
