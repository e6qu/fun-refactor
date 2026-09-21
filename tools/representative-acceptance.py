#!/usr/bin/env python3
"""Audit or replay the cross-language agent acceptance registry."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import time
from typing import Any, TypedDict, cast

from agent_eval import regex_workspace


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "tests/agent-eval/representative-acceptance.json"
REQUIRED = {
    "unfamiliar-upstream", "rust-multi-file", "typescript-react",
    "css-tailwind-mermaid", "backend-migration", "agent-authored-lean-proof",
}


class LiveTrial(TypedDict):
    id: str
    manifest: str
    runner: str
    runner_snapshot: str
    runner_sha256: str
    fixture_revision: str
    model: str
    reasoning_effort: str
    tool_version: str
    diagnostics: list[str]


class Registry(TypedDict):
    schema: str
    cases: list[dict[str, Any]]
    live_trials: list[LiveTrial]
    live_cohorts: list[dict[str, Any]]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load() -> Registry:
    value = json.loads(REGISTRY.read_text())
    if not isinstance(value, dict) or value.get("schema") != "fr-representative-acceptance-1":
        raise ValueError("representative acceptance registry has the wrong schema")
    if any(not isinstance(value.get(name), list) for name in ("cases", "live_trials", "live_cohorts")):
        raise ValueError("representative acceptance registry has malformed sections")
    return cast(Registry, value)


def audit_upstream_read(trial: LiveTrial) -> None:
    if (any(not isinstance(trial.get(name), str) for name in
            ("manifest", "runner", "runner_snapshot", "runner_sha256", "fixture_revision", "model",
             "reasoning_effort", "tool_version"))
            or not isinstance(trial.get("diagnostics"), list)
            or any(not isinstance(name, str) for name in trial["diagnostics"])):
        raise ValueError("live upstream-read trial has malformed fields")
    runner = ROOT / trial["runner_snapshot"]
    if not runner.is_file() or digest(runner) != trial["runner_sha256"]:
        raise ValueError("live upstream-read runner changed")
    directory = (ROOT / trial["manifest"]).parent
    manifest = json.loads((directory / "manifest.json").read_text())
    if (manifest.get("schema") != "fr-upstream-read-manifest-1"
            or manifest.get("passed") is not True
            or manifest.get("acceptance_evidence") is not True
            or manifest.get("model") != trial["model"]):
        raise ValueError("live upstream-read trial is not accepted as declared")
    for name, expected in manifest["files"].items():
        path = directory / name
        if not path.is_file() or digest(path) != expected:
            raise ValueError(f"live upstream-read artifact changed: {path}")
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("evaluator", {}).get(trial["runner"]) != trial["runner_sha256"]
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live upstream-read source or Codex run binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    guide_rows = [row for row in rows if row["request"]["tool"] == "guide"]
    follow_rows = [row for row in rows if row["request"]["tool"] == "follow"]
    show_rows = [row for row in rows if row["request"]["tool"] == "show"]
    if ([row["request"]["goal"]["purpose"] for row in guide_rows] != ["understand", "trace"]
            or [row["full"]["route"]["id"] for row in guide_rows] != ["evidence", "evidence"]
            or [row["request"]["guide"] for row in follow_rows] != [0, 1]
            or any(row["source_sha256"] != session["source_sha256"] for row in rows)
            or not 4 <= len(show_rows) <= 6
            or rows[-1]["request"]["tool"] != "finish"):
        raise ValueError("live upstream-read guidance, reveal or source evidence changed")
    source = {(row["full"]["node"]["path"], row["full"]["node"]["name"]): row["full"]["source"]["text"]
              for row in show_rows}
    expected_snippets = {
        ("src/lib.rs", "escape"): "regex_syntax::escape(pattern)",
        ("regex-syntax/src/lib.rs", "escape"): "escape_into(text, &mut quoted)",
        ("regex-syntax/src/lib.rs", "escape_into"): "is_meta_character(c)",
        ("regex-syntax/src/lib.rs", "is_meta_character"): "'.'",
    }
    if (any(row["full"]["source"]["returned_bytes"] > 512 for row in show_rows)
            or any(snippet not in source.get(key, "") for key, snippet in expected_snippets.items())):
        raise ValueError("live upstream-read source oracle changed")
    trace = follow_rows[1]["full"]["selected"]["call_traces"]["callees"]["nodes"]
    if not any(node["depth"] == 1 and node["symbol"]["path"] == "regex-syntax/src/lib.rs"
               and node["symbol"]["name"] == "escape" and node["via"]["confidence"] == "import-qualified"
               for node in trace):
        raise ValueError("live upstream-read cross-crate edge changed")
    answer = rows[-1]["request"]["answer"]
    if (answer.get("delegate_path") not in ("regex-syntax/src/lib.rs", "regex-syntax/src/lib.rs::escape")
            or answer.get("append_helper") != "escape_into"
            or answer.get("escape_predicate") != "is_meta_character"
            or answer.get("trace_confidence") != "import-qualified"
            or answer.get("runtime_proven") is not False
            or answer.get("example_metacharacter") not in ".\\+*?()|[]{}^$#&-~"):
        raise ValueError("live upstream-read answer oracle changed")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    prompt = (directory / "prompt.txt").read_text()
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-read-agent\.py step \S+ --request-stdin) <<'FRJSON'$", prompt)
    if recorded_step is None:
        raise ValueError("live upstream-read prompt omits its instrumented step")
    if (not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or any(item.get("exit_code") != 0 or recorded_step.group(1) not in item.get("command", "")
                   for item in commands)
            or result.get("passed") is not True or result.get("answer_oracle") is not True
            or result.get("source_unchanged") is not True or result.get("codex", {}).get("usage") != usage
            or result.get("manual_corrections") != 0):
        raise ValueError("live upstream-read score or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("passed") is not False or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")):
            raise ValueError(f"live upstream-read diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live upstream-read diagnostic artifact changed: {artifact}")


def audit_upstream_rename(trial: LiveTrial) -> None:
    for name in ("manifest", "runner", "runner_snapshot", "runner_sha256", "fixture_revision",
                 "model", "reasoning_effort", "tool_version"):
        if not isinstance(trial.get(name), str):
            raise ValueError(f"live rename trial omits {name}")
    if (not isinstance(trial.get("diagnostics"), list)
            or any(not isinstance(name, str) for name in trial["diagnostics"])):
        raise ValueError("live rename diagnostics are malformed")
    snapshot = ROOT / trial["runner_snapshot"]
    if not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]:
        raise ValueError("live rename runner snapshot changed")
    directory = (ROOT / trial["manifest"]).parent
    manifest = json.loads((directory / "manifest.json").read_text())
    if (manifest.get("schema") != "fr-upstream-rename-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("model") != trial["model"]
            or manifest.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]):
        raise ValueError("live rename trial is not accepted as declared")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live rename artifact changed: {artifact}")
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (session.get("upstream_commit") != manifest["upstream_commit"]
            or session.get("evaluator", {}).get(trial["runner"]) != trial["runner_sha256"]
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live rename source or Codex run binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    sequence = [row["request"].get("tool") for row in rows]
    if sequence != ["guide", "preview", "review", "execute", "finish"]:
        raise ValueError("live rename tool sequence changed")
    guide, preview, review, execute, finish = rows
    expected_files = ["regex-cli/args/patterns.rs", "regex-syntax/src/lib.rs", "src/lib.rs"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    stages = execute["full"]["workflow"]["stages"]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "direct-capability"
            or preview["full"].get("files_changed") != 3
            or sorted(Path(change["path"]).name for change in preview["full"]["changes"])
               != sorted(Path(name).name for name in expected_files)
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["upstream", "cli", "minimal"]
            or any(f"--- a/{name}" not in diff or f"+++ b/{name}" not in diff for name in expected_files)
            or "regex_syntax::quote_regex(pattern)" not in diff
            or "pub fn escape(pattern: &str)" not in diff
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "files": expected_files, "checks": ["upstream", "cli", "minimal"],
                "public_facade_preserved": True,
            }):
        raise ValueError("live rename guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-rename-audit-") as temporary:
        receiver = Path(temporary) / "project"
        regex_workspace.unpack(receiver)
        before = {str(path.relative_to(receiver)): digest(path) for path in receiver.rglob("*") if path.is_file()}
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        after = {str(path.relative_to(receiver)): digest(path) for path in receiver.rglob("*") if path.is_file()}
        if (replay.returncode != 0
                or sorted(name for name in before if before[name] != after.get(name)) != expected_files
                or "pub fn quote_regex(text: &str) -> String {" not in
                   (receiver / "regex-syntax/src/lib.rs").read_text()
                or "regex_syntax::quote_regex(p)" not in
                   (receiver / "regex-cli/args/patterns.rs").read_text()
                or "regex_syntax::quote_regex(pattern)" not in (receiver / "src/lib.rs").read_text()):
            raise ValueError("live rename reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-rename-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f'/bin/zsh -lc "python3 {recorded_step.group(1)} <<\'FRJSON\'\n'
        suffix = '\nFRJSON"'
        commands_match = all(
            item.get("exit_code") == 0
            and item.get("command", "").startswith(prefix)
            and item.get("command", "").endswith(suffix)
            and json.loads(item["command"][len(prefix):-len(suffix)].replace('\\"', '"')) == row["request"]
            for item, row in zip(commands, rows)
        )
    oracle = result.get("oracle", {})
    if (recorded_step is None or not isinstance(usage, dict)
            or not isinstance(usage.get("input_tokens"), int)
            or not commands_match
            or result.get("passed") is not True or result.get("guide_route") != "direct-capability"
            or result.get("reviewed_diff_sha256") != hashlib.sha256(diff.encode()).hexdigest()
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != expected_files
            or any(oracle.get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "behavior_64_cases"))
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("manual_corrections") != 0):
        raise ValueError("live rename score, oracle or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("passed") is not False or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")):
            raise ValueError(f"live rename diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live rename diagnostic artifact changed: {artifact}")


def audit_upstream_react(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    archive = ROOT / "tests/agent-eval/react-workspace.tar.gz"
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-react-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != digest(archive)
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live React trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live React artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live React Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    if [row["request"].get("tool") for row in rows] != [
            "guide", "surface", "preview", "review", "execute", "finish"]:
        raise ValueError("live React tool sequence changed")
    guide, surface, preview, review, execute, finish = rows
    source = "src/components/layouts/Header.tsx"
    edit = [item["edit"]["id"] for item in surface["full"]["items"]
            if item.get("class_use", {}).get("name") == "text-lg"
            and item.get("source", {}).get("path") == source]
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "surface-edit"
            or len(edit) != 1
            or any(row["request"].get("edit") != edit[0] for row in (preview, review, execute))
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("diff") != diff
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["typecheck"]
            or f"--- a/{source}" not in diff or f"+++ b/{source}" not in diff
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "file": source, "from": "text-lg", "to": "text-xl", "check": "typecheck"}):
        raise ValueError("live React guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-react-audit-") as temporary:
        receiver = Path(temporary)
        with tarfile.open(archive, "r:gz") as contents:
            original = contents.extractfile(source)
            lock = contents.extractfile("pnpm-lock.yaml")
            if original is None or lock is None or sha256_bytes(lock.read()) != session.get("lock_sha256"):
                raise ValueError("pinned React source or lock is missing")
            before = original.read()
        path = receiver / source
        path.parent.mkdir(parents=True)
        path.write_bytes(before)
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        expected = before.replace(b"text-lg font-medium text-black dark:text-white",
                                  b"text-xl font-medium text-black dark:text-white")
        if (before.count(b"text-lg font-medium text-black dark:text-white") != 1
                or replay.returncode != 0 or path.read_bytes() != expected):
            raise ValueError("live React reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-react-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f'/bin/zsh -lc "python3 {recorded_step.group(1)} <<\'FRJSON\'\n'
        suffix = '\nFRJSON"'
        commands_match = all(
            item.get("exit_code") == 0
            and item.get("command", "").startswith(prefix)
            and item.get("command", "").endswith(suffix)
            and json.loads(item["command"][len(prefix):-len(suffix)].replace('\\"', '"')) == row["request"]
            for item, row in zip(commands, rows)
        )
    if (recorded_step is None or len(commands) != len(rows)
            or not commands_match
            or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or result.get("oracle", {}).get("changed_files") != [source]
            or any(result.get("oracle", {}).get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "receiver_build", "generated_text_xl_css"))):
        raise ValueError("live React score, oracle or Codex provenance changed")


def audit_upstream_css(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    archive = ROOT / "tests/agent-eval/react-workspace.tar.gz"
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-css-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != digest(archive)
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live CSS trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live CSS artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live CSS Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    if [row["request"].get("tool") for row in rows] != [
            "guide", "surface", "preview", "review", "execute", "finish"]:
        raise ValueError("live CSS tool sequence changed")
    guide, surface, preview, review, execute, finish = rows
    source = "src/App.css"
    edit = [item["edit"]["id"] for item in surface["full"]["items"]
            if item.get("kind") == "css-class-definition"
            and item.get("class", {}).get("name") == "read-the-docs"
            and item.get("source", {}).get("path") == source
            and item.get("edit", {}).get("occurrences") == 1]
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "surface-edit"
            or guide["full"].get("target", {}).get("path") != source
            or surface["full"].get("query") != "styles"
            or len(edit) != 1
            or any(row["request"].get("edit") != edit[0] for row in (preview, review, execute))
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("diff") != diff
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["css-parse"]
            or f"--- a/{source}" not in diff or f"+++ b/{source}" not in diff
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "file": source, "from": "read-the-docs", "to": "resource-links", "check": "css-parse"}):
        raise ValueError("live CSS guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-css-audit-") as temporary:
        receiver = Path(temporary)
        with tarfile.open(archive, "r:gz") as contents:
            original = contents.extractfile(source)
            lock = contents.extractfile("pnpm-lock.yaml")
            if original is None or lock is None or sha256_bytes(lock.read()) != session.get("lock_sha256"):
                raise ValueError("pinned CSS source or lock is missing")
            before = original.read()
        path = receiver / source
        path.parent.mkdir(parents=True)
        path.write_bytes(before)
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        expected = before.replace(b".read-the-docs {", b".resource-links {")
        if (before.count(b".read-the-docs {") != 1
                or replay.returncode != 0 or path.read_bytes() != expected):
            raise ValueError("live CSS reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-css-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f'/bin/zsh -lc "python3 {recorded_step.group(1)} <<\'FRJSON\'\n'
        suffix = '\nFRJSON"'
        commands_match = all(
            item.get("exit_code") == 0
            and item.get("command", "").startswith(prefix)
            and item.get("command", "").endswith(suffix)
            and json.loads(item["command"][len(prefix):-len(suffix)].replace('\\"', '"')) == row["request"]
            for item, row in zip(commands, rows)
        )
    oracle = result.get("oracle", {})
    if (not commands_match or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("codex", {}).get("failed_command_executions") != 0
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != [source]
            or any(oracle.get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "receiver_build", "generated_css_selector"))):
        raise ValueError("live CSS score, oracle or Codex provenance changed")


def audit_upstream_tsx_body(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    archive = ROOT / "tests/agent-eval/react-workspace.tar.gz"
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-tsx-body-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != digest(archive)
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live TSX body trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live TSX body artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live TSX body Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    if [row["request"].get("tool") for row in rows] != [
            "guide", "reveal", "preview", "review", "execute", "finish"]:
        raise ValueError("live TSX body tool sequence changed")
    guide, reveal, preview, review, execute, finish = rows
    source = "src/components/layouts/Layout.tsx"
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    body = preview["request"].get("body")
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "source-body"
            or guide["full"].get("route", {}).get("source_required") is not True
            or session.get("goal", {}).get("constraints", {}).get("allow_source") is not True
            or guide["full"].get("target", {}).get("path") != source
            or reveal["full"].get("source", {}).get("returned_bytes", 4097) > 4096
            or "<main className=" not in reveal["full"].get("source", {}).get("text", "")
            or not isinstance(body, str) or not 1 <= len(body.encode()) <= 4096
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("path") != source
            or preview["full"].get("diff") != diff
            or review["request"] != {"tool": "review"}
            or execute["request"] != {"tool": "execute"}
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["typecheck"]
            or f"--- a/{source}" not in diff or f"+++ b/{source}" not in diff
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "file": source, "mobile": "Mobile content", "responsive": "Responsive content",
                "check": "typecheck"}):
        raise ValueError("live TSX body guidance, review or delivery changed")
    before_main = b"<main className={`layout-container ${layoutType}`}>{children}</main>"
    variants = (
        b"<main aria-label={layoutType === 'mobile' ? 'Mobile content' : 'Responsive content'} className={`layout-container ${layoutType}`}>{children}</main>",
        b"<main className={`layout-container ${layoutType}`} aria-label={layoutType === 'mobile' ? 'Mobile content' : 'Responsive content'}>{children}</main>",
    )
    with tempfile.TemporaryDirectory(prefix="fr-tsx-body-audit-") as temporary:
        receiver = Path(temporary)
        with tarfile.open(archive, "r:gz") as contents:
            original = contents.extractfile(source)
            lock = contents.extractfile("pnpm-lock.yaml")
            if original is None or lock is None or sha256_bytes(lock.read()) != session.get("lock_sha256"):
                raise ValueError("pinned TSX body source or lock is missing")
            before = original.read()
        path = receiver / source
        path.parent.mkdir(parents=True)
        path.write_bytes(before)
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        expected = {before.replace(before_main, variant) for variant in variants}
        if before.count(before_main) != 1 or replay.returncode != 0 or path.read_bytes() not in expected:
            raise ValueError("live TSX body reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-tsx-body-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f"python3 {recorded_step.group(1)} <<'FRJSON'\n"
        suffix = "\nFRJSON"
        arguments = [shlex.split(item.get("command", "")) for item in commands]
        commands_match = all(
            item.get("exit_code") == 0 and argv[:2] == ["/bin/zsh", "-lc"]
            and len(argv) == 3 and argv[2].startswith(prefix) and argv[2].endswith(suffix)
            and json.loads(argv[2][len(prefix):-len(suffix)]) == row["request"]
            for item, argv, row in zip(commands, arguments, rows)
        )
    oracle = result.get("oracle", {})
    rendered = json.loads(oracle.get("render_detail", "null"))
    if (not commands_match or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("codex", {}).get("failed_command_executions") != 0
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != [source]
            or any(oracle.get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "receiver_build", "rendered_layout_cases"))
            or not isinstance(rendered, list) or len(rendered) != 3
            or [(row.get("type"), row.get("main")) for row in rendered] != [
                ("mobile", '<main class="layout-container mobile" aria-label="Mobile content">'),
                ("responsive", '<main class="layout-container responsive" aria-label="Responsive content">'),
                (None, '<main class="layout-container responsive" aria-label="Responsive content">')]):
        raise ValueError("live TSX body score, oracle or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("schema") != "fr-upstream-tsx-body-manifest-1"
                or diagnostic.get("passed") is not False
                or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")
                or diagnostic.get("evaluator_sha256") != diagnostic.get("files", {}).get("runner.py")):
            raise ValueError(f"live TSX body diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live TSX body diagnostic artifact changed: {artifact}")


def audit_upstream_multibody(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    archive = ROOT / "tests/agent-eval/react-workspace.tar.gz"
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-multibody-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != digest(archive)
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live multi-body trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live multi-body artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live multi-body Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    if [row["request"].get("tool") for row in rows] != [
            "guide", "reveal-layout", "reveal-header", "preview", "review", "execute", "finish"]:
        raise ValueError("live multi-body tool sequence changed")
    guide, layout_reveal, header_reveal, preview, review, execute, finish = rows
    layout = "src/components/layouts/Layout.tsx"
    header = "src/components/layouts/Header.tsx"
    sources = [header, layout]
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    bodies = [preview["request"].get("layout_body"), preview["request"].get("header_body")]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "source-bodies"
            or guide["full"].get("route", {}).get("source_required") is not True
            or session.get("goal", {}).get("constraints", {}).get("allow_source") is not True
            or [item.get("path") for item in guide["full"].get("targets", [])] != [layout, header]
            or any(row["full"].get("source", {}).get("returned_bytes", 4097) > 4096
                   for row in (layout_reveal, header_reveal))
            or "<main className=" not in layout_reveal["full"].get("source", {}).get("text", "")
            or "<header" not in header_reveal["full"].get("source", {}).get("text", "")
            or any(not isinstance(body, str) or not 1 <= len(body.encode()) <= 4096 for body in bodies)
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("files_changed") != 2
            or preview["full"].get("diff") != diff
            or review["request"] != {"tool": "review"}
            or execute["request"] != {"tool": "execute"}
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["typecheck"]
            or any(f"--- a/{path}" not in diff or f"+++ b/{path}" not in diff for path in sources)
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "files": [layout, header],
                "mobile": ["Mobile content", "Mobile navigation"],
                "responsive": ["Responsive content", "Responsive navigation"],
                "check": "typecheck"}):
        raise ValueError("live multi-body guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-multibody-audit-") as temporary:
        receiver = Path(temporary)
        with tarfile.open(archive, "r:gz") as contents:
            originals = {}
            for path in sources:
                member = contents.extractfile(path)
                if member is None:
                    raise ValueError("pinned multi-body source is missing")
                originals[path] = member.read()
                destination = receiver / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(originals[path])
            lock = contents.extractfile("pnpm-lock.yaml")
            if lock is None or sha256_bytes(lock.read()) != session.get("lock_sha256"):
                raise ValueError("pinned multi-body lock is missing")
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        main_before = b"<main className={`layout-container ${layoutType}`}>{children}</main>"
        main_label = b"aria-label={layoutType === 'mobile' ? 'Mobile content' : 'Responsive content'}"
        header_before = b"<header\n      className="
        header_label = b"aria-label={type === 'mobile' ? 'Mobile navigation' : 'Responsive navigation'}"
        layout_variants = {
            originals[layout].replace(main_before, b"<main " + main_label +
                                      b" className={`layout-container ${layoutType}`}>{children}</main>"),
            originals[layout].replace(main_before,
                                      b"<main className={`layout-container ${layoutType}`} " + main_label +
                                      b">{children}</main>"),
        }
        header_variants = {
            originals[header].replace(header_before, b"<header\n      " + header_label + b"\n      className="),
            originals[header].replace(b"<header\n", b"<header " + header_label + b"\n", 1),
            originals[header].replace(b"      )}\n    >", b"      )}\n      " + header_label + b"\n    >", 1),
        }
        if (replay.returncode != 0 or originals[layout].count(main_before) != 1
                or originals[header].count(header_before) != 1
                or (receiver / layout).read_bytes() not in layout_variants
                or (receiver / header).read_bytes() not in header_variants):
            raise ValueError("live multi-body reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-multibody-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f"python3 {recorded_step.group(1)} <<'FRJSON'\n"
        suffix = "\nFRJSON"
        arguments = [shlex.split(item.get("command", "")) for item in commands]
        commands_match = all(
            item.get("exit_code") == 0 and argv[:2] == ["/bin/zsh", "-lc"]
            and len(argv) == 3 and argv[2].startswith(prefix) and argv[2].endswith(suffix)
            and json.loads(argv[2][len(prefix):-len(suffix)]) == row["request"]
            for item, argv, row in zip(commands, arguments, rows)
        )
    oracle = result.get("oracle", {})
    rendered = json.loads(oracle.get("render_detail", "null"))
    expected_render = [("mobile", "Mobile content", "Mobile navigation", "mobile"),
                       ("responsive", "Responsive content", "Responsive navigation", "responsive"),
                       (None, "Responsive content", "Responsive navigation", "responsive")]
    render_matches = (isinstance(rendered, list) and len(rendered) == 3 and all(
        item.get("type") == kind and f'aria-label="{main_label}"' in item.get("main", "")
        and f'class="layout-container {layout_type}"' in item.get("main", "")
        and f'aria-label="{header_label}"' in item.get("header", "")
        for item, (kind, main_label, header_label, layout_type) in zip(rendered, expected_render)))
    if (not commands_match or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("codex", {}).get("failed_command_executions") != 0
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != sources
            or any(oracle.get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "receiver_build", "rendered_layout_cases"))
            or not render_matches):
        raise ValueError("live multi-body score, oracle or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("schema") != "fr-upstream-multibody-manifest-1"
                or diagnostic.get("passed") is not False
                or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")
                or diagnostic.get("evaluator_sha256") != diagnostic.get("files", {}).get("runner.py")):
            raise ValueError(f"live multi-body diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live multi-body diagnostic artifact changed: {artifact}")


def audit_upstream_cross_crate_bodies(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-cross-crate-bodies-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != regex_workspace.ARCHIVE_SHA
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live cross-crate body trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live cross-crate body artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live cross-crate body Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    expected_sequence = ["guide", "reveal-syntax", "reveal-facade", "preview", "review", "execute", "finish"]
    if [row["request"].get("tool") for row in rows] != expected_sequence:
        raise ValueError("live cross-crate body tool sequence changed")
    guide, syntax_reveal, facade_reveal, preview, review, execute, finish = rows
    syntax_path, facade_path = "regex-syntax/src/lib.rs", "src/lib.rs"
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    bodies = [preview["request"].get("syntax_body"), preview["request"].get("facade_body")]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "source-bodies"
            or guide["full"].get("route", {}).get("source_required") is not True
            or session.get("goal", {}).get("constraints", {}).get("allow_source") is not True
            or [item.get("path") for item in guide["full"].get("targets", [])] != [syntax_path, facade_path]
            or any(row["full"].get("source", {}).get("returned_bytes", 4097) > 4096
                   for row in (syntax_reveal, facade_reveal))
            or "pub fn escape_into" not in syntax_reveal["full"].get("source", {}).get("text", "")
            or "pub fn escape(pattern" not in facade_reveal["full"].get("source", {}).get("text", "")
            or any(not isinstance(body, str) or not 1 <= len(body.encode()) <= 4096 for body in bodies)
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("files_changed") != 2
            or preview["full"].get("diff") != diff
            or review["request"] != {"tool": "review"}
            or execute["request"] != {"tool": "execute"}
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["upstream", "minimal"]
            or any(f"--- a/{path}" not in diff or f"+++ b/{path}" not in diff
                   for path in (syntax_path, facade_path))
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "files": [syntax_path, facade_path], "checks": ["upstream", "minimal"],
                "allocation_calls": 1, "append_preserved": True}):
        raise ValueError("live cross-crate body guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-cross-crate-body-audit-") as temporary:
        receiver = Path(temporary) / "project"
        regex_workspace.unpack(receiver)
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        syntax = (receiver / syntax_path).read_text()
        facade = (receiver / facade_path).read_text()
        if (replay.returncode != 0
                or ".filter(|&c| is_meta_character(c)).count()" not in syntax
                or "buf.reserve(" not in syntax
                or "alloc::string::String::new()" not in facade
                or "regex_syntax::escape_into(pattern, &mut " not in facade
                or "regex_syntax::escape(pattern)" in facade):
            raise ValueError("live cross-crate body reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(
        r"(?m)^python3 (\S+/tools/upstream-cross-crate-bodies-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
        (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f"python3 {recorded_step.group(1)} <<'FRJSON'\n"
        suffix = "\nFRJSON"
        arguments = [shlex.split(item.get("command", "")) for item in commands]
        commands_match = all(
            item.get("exit_code") == 0 and argv[:2] == ["/bin/zsh", "-lc"]
            and len(argv) == 3 and argv[2].startswith(prefix) and argv[2].endswith(suffix)
            and json.loads(argv[2][len(prefix):-len(suffix)]) == row["request"]
            for item, argv, row in zip(commands, arguments, rows)
        )
    oracle = result.get("oracle", {})
    if (not commands_match or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("codex", {}).get("failed_command_executions") != 0
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != [syntax_path, facade_path]
            or any(oracle.get(name) is not True for name in (
                "source_contract", "receiver_patch_replay", "offline_compiled", "behavior_64_cases",
                "one_allocation_nonempty", "append_preserved"))):
        raise ValueError("live cross-crate body score, oracle or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("schema") != "fr-upstream-cross-crate-bodies-manifest-1"
                or diagnostic.get("passed") is not False
                or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")
                or diagnostic.get("evaluator_sha256") != diagnostic.get("files", {}).get("runner.py")):
            raise ValueError(f"live cross-crate body diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live cross-crate body diagnostic artifact changed: {artifact}")


def audit_upstream_mermaid(trial: LiveTrial) -> None:
    snapshot = ROOT / trial["runner_snapshot"]
    directory = (ROOT / trial["manifest"]).parent
    archive = ROOT / "tests/agent-eval/micromaid-workspace.tar.gz"
    lock = ROOT / "tests/agent-eval/mermaid-oracle/package-lock.json"
    manifest = json.loads((directory / "manifest.json").read_text())
    session = json.loads((directory / "session.json").read_text())
    run = json.loads((directory / "codex-run.json").read_text())
    result = json.loads((directory / "result.json").read_text())
    if (not snapshot.is_file() or digest(snapshot) != trial["runner_sha256"]
            or manifest.get("schema") != "fr-upstream-mermaid-manifest-1"
            or manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
            or manifest.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("evaluator_sha256") != trial["runner_sha256"]
            or session.get("upstream_commit") != trial["fixture_revision"].split("@")[-1]
            or session.get("archive_sha256") != digest(archive)
            or session.get("oracle_lock_sha256") != digest(lock)
            or run.get("model") != trial["model"]
            or run.get("reasoning_effort") != trial["reasoning_effort"]
            or run.get("codex_version") != trial["tool_version"]
            or run.get("exit_code") != 0 or run.get("timed_out")
            or result.get("passed") is not True):
        raise ValueError("live Mermaid trial binding changed")
    for name, expected in manifest["files"].items():
        artifact = directory / name
        if not artifact.is_file() or digest(artifact) != expected:
            raise ValueError(f"live Mermaid artifact changed: {artifact}")
    if (run.get("prompt_sha256") != digest(directory / "prompt.txt")
            or run.get("events_sha256") != digest(directory / "codex-events.jsonl")
            or run.get("stderr_sha256") != digest(directory / "codex-stderr.txt")):
        raise ValueError("live Mermaid Codex transcript binding changed")
    rows = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
    if [row["request"].get("tool") for row in rows] != [
            "guide", "surface", "preview", "review", "execute", "finish"]:
        raise ValueError("live Mermaid tool sequence changed")
    guide, surface, preview, review, execute, finish = rows
    expanded = surface["full"]
    guided_page = expanded["guided_page"]
    edit = [item["edit"]["id"] for item in expanded["items"]
            if item.get("kind") == "mermaid-node" and item.get("node", {}).get("name") == "E"
            and item.get("edit", {}).get("occurrences") == 4]
    stages = execute["full"]["workflow"]["stages"]
    expected_stages = ["check-original", "apply", "check-applied", "undo", "check-restored",
                       "redo", "check-applied", "deliver-patch"]
    diff = review["full"]["diff"]
    if (guide["full"].get("state") != "ready"
            or guide["full"].get("route", {}).get("id") != "surface-edit"
            or guide["full"].get("target", {}).get("path") != "README.md"
            or guided_page.get("query") != "diagrams"
            or guided_page.get("revision") != expanded.get("revision")
            or guided_page.get("page", {}).get("returned") != 8
            or expanded.get("page", {}).get("total") != 20
            or len(edit) != 1
            or any(row["request"].get("edit") != edit[0] for row in (preview, review, execute))
            or preview["full"].get("applied") is not False
            or preview["full"].get("changed") is not True
            or preview["full"].get("diff") != diff
            or review["full"].get("ready") is not True
            or review["full"].get("checks", {}).get("names") != ["mermaid"]
            or "--- a/README.md" not in diff or "+++ b/README.md" not in diff
            or execute["full"].get("passed") is not True
            or [item.get("stage") for item in stages] != expected_stages
            or any(item.get("status") != "passed" for item in stages)
            or finish["request"].get("answer") != {
                "file": "README.md", "node": "E", "new_node": "EvalStep", "edges": 5, "check": "mermaid"}):
        raise ValueError("live Mermaid guidance, review or delivery changed")
    with tempfile.TemporaryDirectory(prefix="fr-mermaid-audit-") as temporary:
        receiver = Path(temporary)
        with tarfile.open(archive, "r:gz") as contents:
            original = contents.extractfile("README.md")
            if original is None:
                raise ValueError("pinned Mermaid README is missing")
            before = original.read().decode()
        blocks = list(re.finditer(r"(?m)^```mermaid\n([\s\S]*?)^```", before))
        if len(blocks) != 2 or len(re.findall(r"\bE\b", blocks[0].group(1))) != 4:
            raise ValueError("pinned Mermaid diagram shape changed")
        expected = (before[:blocks[0].start(1)] + re.sub(r"\bE\b", "EvalStep", blocks[0].group(1))
                    + before[blocks[0].end(1):])
        (receiver / "README.md").write_text(before)
        replay = subprocess.run(["git", "apply", "-"], cwd=receiver, input=diff.encode(),
                                capture_output=True, timeout=30)
        if replay.returncode != 0 or (receiver / "README.md").read_text() != expected:
            raise ValueError("live Mermaid reviewed diff fails pinned receiver replay")
    codex_rows = [json.loads(line) for line in (directory / "codex-events.jsonl").read_text().splitlines()]
    usage = next((row["usage"] for row in reversed(codex_rows) if row.get("type") == "turn.completed"), None)
    commands = [row["item"] for row in codex_rows if row.get("type") == "item.completed"
                and row.get("item", {}).get("type") == "command_execution"]
    recorded_step = re.search(r"(?m)^python3 (\S+/tools/upstream-mermaid-agent\.py step \S+ --request-stdin) <<'FRJSON'$",
                              (directory / "prompt.txt").read_text())
    commands_match = False
    if recorded_step is not None and len(commands) == len(rows):
        prefix = f'/bin/zsh -lc "python3 {recorded_step.group(1)} <<\'FRJSON\'\n'
        suffix = '\nFRJSON"'
        commands_match = all(
            item.get("exit_code") == 0
            and item.get("command", "").startswith(prefix)
            and item.get("command", "").endswith(suffix)
            and json.loads(item["command"][len(prefix):-len(suffix)].replace('\\"', '"')) == row["request"]
            for item, row in zip(commands, rows)
        )
    oracle = result.get("oracle", {})
    if (not commands_match or not isinstance(usage, dict) or not isinstance(usage.get("input_tokens"), int)
            or result.get("codex", {}).get("usage") != usage
            or result.get("codex", {}).get("direct_project_commands") != []
            or result.get("manual_corrections") != 0
            or result.get("reviewed_diff_sha256") != sha256_text(diff)
            or result.get("stages") != [[name, "passed"] for name in expected_stages]
            or oracle.get("changed_files") != ["README.md"]
            or any(oracle.get(name) is not True for name in
                   ("exact_source", "receiver_patch_replay", "graph_preserved", "mermaid_parser_passed"))):
        raise ValueError("live Mermaid score, oracle or Codex provenance changed")
    for path_name in trial["diagnostics"]:
        path = ROOT / path_name
        diagnostic = json.loads(path.read_text())
        if (diagnostic.get("schema") != "fr-upstream-mermaid-manifest-1"
                or diagnostic.get("acceptance_evidence") is not False
                or not diagnostic.get("diagnostic_reason")):
            raise ValueError(f"live Mermaid diagnostic is mislabeled: {path}")
        for name, expected in diagnostic["files"].items():
            artifact = path.parent / name
            if not artifact.is_file() or digest(artifact) != expected:
                raise ValueError(f"live Mermaid diagnostic artifact changed: {artifact}")


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def audit() -> dict[str, object]:
    registry = load()
    cases = registry.get("cases")
    if not isinstance(cases, list) or len(cases) < 6:
        raise ValueError("representative acceptance registry is incomplete")
    covered: set[str] = set()
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("representative acceptance case is not an object")
        categories = case.get("covers")
        if not isinstance(categories, list) or not all(isinstance(item, str) for item in categories):
            raise ValueError("representative acceptance case has invalid coverage")
        covered.update(categories)
        runner = ROOT / str(case.get("runner"))
        if not runner.is_file() or digest(runner) != case.get("runner_sha256"):
            raise ValueError(f"representative acceptance runner changed: {runner}")
        test = case.get("test")
        if isinstance(test, str) and not re.search(rf"\bfn\s+{re.escape(test)}\s*\(", runner.read_text()):
            raise ValueError(f"representative acceptance test is absent: {test}")
        for field in ("fixture_revision", "oracle", "postconditions"):
            if not case.get(field):
                raise ValueError(f"representative acceptance case omits {field}")
    if not REQUIRED <= covered:
        raise ValueError(f"representative acceptance omits {sorted(REQUIRED - covered)}")

    trials = registry.get("live_trials")
    if not isinstance(trials, list) or len(trials) < 1:
        raise ValueError("representative acceptance lacks live upstream guidance")
    for trial in trials:
        if trial.get("id") == "regex-guided-understand-trace":
            audit_upstream_read(trial)
        elif trial.get("id") == "regex-guided-multi-file-rename":
            audit_upstream_rename(trial)
        elif trial.get("id") == "react-guided-tailwind-surface":
            audit_upstream_react(trial)
        elif trial.get("id") == "react-guided-standalone-css":
            audit_upstream_css(trial)
        elif trial.get("id") == "react-guided-tsx-body":
            audit_upstream_tsx_body(trial)
        elif trial.get("id") == "react-guided-multi-body":
            audit_upstream_multibody(trial)
        elif trial.get("id") == "regex-guided-cross-crate-bodies":
            audit_upstream_cross_crate_bodies(trial)
        elif trial.get("id") == "micromaid-guided-mermaid-node":
            audit_upstream_mermaid(trial)
        else:
            raise ValueError("representative acceptance has an unknown live trial")

    cohorts = registry.get("live_cohorts")
    if not isinstance(cohorts, list) or len(cohorts) < 2:
        raise ValueError("representative acceptance needs repeated live matched cohorts")
    for cohort in cohorts:
        if not isinstance(cohort, dict):
            raise ValueError("live cohort entry is malformed")
        manifest_path = ROOT / str(cohort.get("manifest"))
        manifest = json.loads(manifest_path.read_text())
        for field in ("revision", "model", "reasoning_effort", "tool_version", "inputs", "oracle"):
            if not cohort.get(field):
                raise ValueError(f"live cohort entry omits {field}: {manifest_path}")
        for field in ("model", "reasoning_effort", "service_tier", "results", "files"):
            if field not in manifest:
                raise ValueError(f"live cohort omits {field}: {manifest_path}")
        if (manifest.get("passed") is not True or manifest.get("acceptance_evidence") is not True
                or manifest.get("model") != cohort.get("model")
                or manifest.get("reasoning_effort") != cohort.get("reasoning_effort")):
            raise ValueError(f"live cohort is not accepted as declared: {manifest_path}")
        files = manifest.get("files")
        if not isinstance(files, dict):
            raise ValueError("live cohort file bindings are malformed")
        for relative, expected in files.items():
            if relative == "manifest.json":
                continue
            path = manifest_path.parent / relative
            if not path.is_file() or digest(path) != expected:
                raise ValueError(f"live cohort artifact changed: {path}")
        prompt_digests = [files.get("fr/prompt.txt"), files.get("files/prompt.txt")]
        if cohort["inputs"] != prompt_digests:
            raise ValueError(f"live cohort inputs do not name its prompts: {manifest_path}")
        for result in manifest["results"]:
            usage = result.get("codex", {}).get("usage")
            if (not isinstance(usage, dict) or "input_tokens" not in usage
                    or "billed_quota" not in result.get("codex", {})):
                raise ValueError("live cohort omits usage or quota availability")
        for arm in ("fr", "files"):
            run = json.loads((manifest_path.parent / arm / "codex-run.json").read_text())
            if (run.get("codex_version") != cohort["tool_version"]
                    or run.get("model") != cohort["model"]
                    or run.get("reasoning_effort") != cohort["reasoning_effort"]):
                raise ValueError(f"live cohort tool identity changed: {manifest_path}")
    return {
        "schema": registry["schema"],
        "passed": True,
        "cases": len(cases),
        "coverage": sorted(covered),
        "live_cohorts": len(cohorts),
        "live_trials": len(trials),
    }


def replay(include_external: bool) -> dict[str, object]:
    registry = load()
    results = []
    environment = os.environ.copy()
    environment.update({"CARGO_BUILD_JOBS": "1", "FR_LEAN_JOBS": "1", "LEAN_NUM_THREADS": "1"})
    for case in registry["cases"]:
        if case.get("external") and not include_external:
            results.append({"id": case["id"], "status": "not-requested", "failure_kind": None})
            continue
        missing = [tool for tool in case.get("required_tools", []) if shutil.which(tool) is None]
        if missing:
            results.append({
                "id": case["id"], "status": "failed", "failure_kind": "infrastructure",
                "detail": f"missing tools: {', '.join(missing)}",
            })
            continue
        command = [part.replace("<FR>", str(ROOT / "target/debug/fr")) for part in case["command"]]
        started = time.time()
        completed = subprocess.run(
            command, cwd=ROOT, env=environment, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        results.append({
            "id": case["id"],
            "status": "passed" if completed.returncode == 0 else "failed",
            "failure_kind": None if completed.returncode == 0 else "product",
            "elapsed_seconds": round(time.time() - started, 3),
            "output_sha256": hashlib.sha256(completed.stdout.encode()).hexdigest(),
            "output_tail": completed.stdout[-2_048:] if completed.returncode else "",
        })
    return {
        "schema": "fr-representative-acceptance-replay-1",
        "passed": all(row["status"] in ("passed", "not-requested") for row in results),
        "external_requested": include_external,
        "results": results,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("audit")
    replay_parser = sub.add_parser("replay")
    replay_parser.add_argument("--external", action="store_true")
    args = parser.parse_args()
    value = audit() if args.command == "audit" else replay(args.external)
    print(json.dumps(value, indent=2, sort_keys=True))
    if value.get("passed") is not True:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
