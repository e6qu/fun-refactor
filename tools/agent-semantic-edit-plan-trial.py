#!/usr/bin/env python3
"""Prepare, score and retain direct-plan versus explicit-intent agent sessions."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess


ROOT = Path(__file__).resolve().parent.parent
SOURCE = """pub fn summarize(value: i32) -> i32 {
    let stage_one = value + 1;
    let stage_two = stage_one * 2;
    let stage_three = stage_two - 3;
    let stage_four = stage_three + 4;
    let stage_five = stage_four * 5;
    stage_five
}

#[test]
fn changed_behavior() {
    assert_eq!(summarize(2), 95);
}
"""


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def semantic(binary: Path, project: Path) -> dict:
    result = subprocess.run(
        [
            str(binary), "--json", "--no-cache", "-C", str(project), "project", "semantic",
            "app.rs", "--declaration", "summarize", "--body", "--nodes", "128", "--minimal",
        ],
        check=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


def expected_body(binary: Path, project: Path) -> dict:
    report = semantic(binary, project)
    body = {
        "schema": "fr-semantic-body-1",
        "body": report["model"]["items"][0]["value"]["body"],
    }
    body["body"][0]["value"]["value"]["value"]["right"] = {
        "kind": "int", "value": "7",
    }
    return body


def prepare(out: Path, binary: Path) -> None:
    if out.exists():
        raise ValueError(f"{out} already exists")
    names = ["semantic-edit-plan-direct", "semantic-edit-plan-explicit"]
    out.mkdir(parents=True)
    (out / "experiment.json").write_text(
        json.dumps(
            {
                "project": "semantic-edit-plan",
                "repetitions": 1,
                "pair_arms": ["direct", "explicit"],
                "trials": names,
            },
            indent=2,
        )
        + "\n"
    )
    common = (
        "Read the single Markdown file under ../skill first. Change summarize so stage_one adds 7 instead of 1. "
        "Do not read app.rs or source text through any tool. Do not inspect fr implementation source. "
        "Preview the requested operation, then apply it with --write and the preview plan basis. "
        "Stop after the write succeeds."
    )
    for name, arm in zip(names, ("direct", "explicit"), strict=True):
        session = out / name
        project = session / "project"
        skill = session / "skill"
        project.mkdir(parents=True)
        skill.mkdir()
        shutil.copy2(binary, project / "fr")
        shutil.copy2(ROOT / "skills/fr/references/semantic-intent.md", skill / "semantic-intent.md")
        (project / "app.rs").write_text(SOURCE, encoding="utf-8")
        operation = "edit-body-scalar" if arm == "direct" else "edit-body-intent"
        route = (
            "Use edit-body-scalar directly on path . with --declaration summarize, --operation set-int, --from 1 and --to 7. "
            "Do not create an authoring payload and do not run a separate project query."
            if arm == "direct"
            else "Use project semantic with --body --locators --locators-only and exact --locator-op and --locator-from filters. "
            "Write one fr-semantic-intent-1 payload only to ../change.json and apply it with edit-body-intent."
        )
        (session / "session.json").write_text(
            json.dumps({"task": "semantic-edit-plan", "arm": arm, "repetition": 1, "operation": operation}, indent=2) + "\n"
        )
        (session / "prompt.txt").write_text(f"{common} {route}\n", encoding="utf-8")
        (session / "expected.json").write_text(
            json.dumps(expected_body(project / "fr", project), separators=(",", ":")) + "\n",
            encoding="utf-8",
        )


def command_events(path: Path) -> list[str]:
    commands = []
    for line in path.read_text(encoding="utf-8").splitlines():
        event = json.loads(line)
        item = event.get("item", {})
        if event.get("type") == "item.completed" and item.get("type") == "command_execution":
            commands.append(item.get("command", ""))
    return commands


def usage(path: Path) -> dict:
    completed = None
    for line in path.read_text(encoding="utf-8").splitlines():
        event = json.loads(line)
        if event.get("type") == "turn.completed":
            completed = event.get("usage")
    return completed or {}


def direct_source_reads(commands: list[str]) -> int:
    patterns = [
        r"\b(cat|sed|head|tail|less|more|grep|awk|perl)\b[^\n]*app\.rs",
        r"\brg\b(?![^\n;&|]*\s--files(?:\s|$))[^\n;&|]*app\.rs",
        r"\b(read_text|read_bytes|open)\s*\([^\n]*app\.rs",
        r"\brustc\b[^\n]*app\.rs",
    ]
    return sum(any(re.search(pattern, command) for pattern in patterns) for command in commands)


def compile_test(project: Path) -> bool:
    compiled = subprocess.run(
        ["rustc", "--edition=2021", "--test", "app.rs", "-o", "app-test"],
        cwd=project,
        check=False,
        capture_output=True,
    )
    if compiled.returncode != 0:
        return False
    return subprocess.run(
        [str(project / "app-test")], cwd=project, check=False, capture_output=True
    ).returncode == 0


def score(sessions: Path) -> dict:
    results = []
    for name in load(sessions / "experiment.json")["trials"]:
        session = sessions / name
        config = load(session / "session.json")
        project = session / "project"
        run = load(session / "codex-run.json")
        commands = command_events(session / "codex-events.jsonl")
        payload_path = session / "change.json"
        payload = load(payload_path) if payload_path.is_file() else {}
        final = semantic(project / "fr", project)
        body = {
            "schema": "fr-semantic-body-1",
            "body": final["model"]["items"][0]["value"]["body"],
        }
        expected = load(session / "expected.json")
        operation = config["operation"]
        other = "edit-body-intent" if operation == "edit-body-scalar" else "edit-body-scalar"
        source_reads = direct_source_reads(commands)
        route_used = any(operation in command for command in commands) and not any(
            other in command for command in commands
        )
        expected_schema = "fr-semantic-intent-1"
        behavior_passed = compile_test(project)
        filtered_locator_used = config["arm"] == "direct" or any(
            "--locators-only" in command and "--locator-op" in command and "--locator-from" in command
            for command in commands
        )
        payload_valid = (
            not payload_path.exists()
            if config["arm"] == "direct"
            else payload.get("schema") == expected_schema
        )
        direct_shape_used = config["arm"] != "direct" or any(
            "edit-body-scalar ." in command and "--declaration" in command
            and "--operation" in command and "--from" in command and "--to" in command
            for command in commands
        )
        separate_query_avoided = config["arm"] != "direct" or not any(
            "project semantic" in command for command in commands
        )
        passed = (
            run.get("exit_code") == 0
            and payload_valid
            and route_used
            and filtered_locator_used
            and direct_shape_used
            and separate_query_avoided
            and source_reads == 0
            and body == expected
            and behavior_passed
            and (project / ".fr-history").is_dir()
        )
        result = {
            "schema": "fr-agent-semantic-edit-plan-result-1",
            "trial": name,
            "route": "direct-scalar-plan" if config["arm"] == "direct" else "explicit-intent",
            "passed": passed,
            "exact_semantic_body": body == expected,
            "behavior_passed": behavior_passed,
            "route_used": route_used,
            "filtered_locator_used": filtered_locator_used,
            "direct_shape_used": direct_shape_used,
            "separate_query_avoided": separate_query_avoided,
            "direct_source_reads": source_reads,
            "payload_bytes": payload_path.stat().st_size if payload_path.is_file() else 0,
            "commands": len(commands),
            "usage": usage(session / "codex-events.jsonl"),
            "run_sha256": digest(session / "codex-run.json"),
        }
        (session / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        results.append(result)
    summary = {
        "schema": "fr-agent-semantic-edit-plan-pair-1",
        "passed": all(result["passed"] for result in results),
        "results": results,
    }
    print(json.dumps(summary, indent=2))
    return summary


def record(sessions: Path, out: Path, implementation_commit: str) -> None:
    if out.exists():
        raise ValueError(f"{out} already exists")
    out.mkdir(parents=True)
    shutil.copy2(sessions / "experiment.json", out / "experiment.json")
    experiment = load(sessions / "experiment.json")
    for name in experiment["trials"]:
        source = sessions / name
        destination = out / name
        destination.mkdir()
        for filename in (
            "session.json", "prompt.txt", "expected.json", "codex-events.jsonl",
            "codex-stderr.txt", "codex-final.txt", "codex-run.json", "result.json",
            "change.json",
        ):
            candidate = source / filename
            if candidate.is_file():
                shutil.copy2(candidate, destination / filename)
        skill_name = "semantic-intent.md"
        shutil.copy2(source / "skill" / skill_name, destination / skill_name)
    files = {
        str(path.relative_to(out)): digest(path)
        for path in sorted(out.rglob("*"))
        if path.is_file()
    }
    first = load(out / experiment["trials"][0] / "codex-run.json")
    manifest = {
        "schema": "fr-agent-semantic-edit-plan-evidence-1",
        "implementation_commit": implementation_commit,
        "model": first["model"],
        "reasoning_effort": first["reasoning_effort"],
        "service_tier": first["service_tier"],
        "codex_version": first["codex_version"],
        "conditions": "Fresh sequential ephemeral sessions, ignored user configuration and rules, workspace-write sandbox, no human corrections or restarts.",
        "files": files,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    prepare_parser = sub.add_parser("prepare")
    prepare_parser.add_argument("--out", type=Path, required=True)
    prepare_parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    score_parser = sub.add_parser("score")
    score_parser.add_argument("sessions", type=Path)
    record_parser = sub.add_parser("record")
    record_parser.add_argument("sessions", type=Path)
    record_parser.add_argument("--out", type=Path, required=True)
    record_parser.add_argument("--implementation-commit", required=True)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.out.resolve(), args.fr.resolve())
        return 0
    if args.command == "record":
        record(args.sessions, args.out, args.implementation_commit)
        return 0
    return 0 if score(args.sessions)["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
