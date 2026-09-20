#!/usr/bin/env python3
"""Audit or replay the cross-language agent acceptance registry."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
from typing import Any, TypedDict, cast


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
        if trial.get("id") != "regex-guided-understand-trace":
            raise ValueError("representative acceptance has an unknown live trial")
        audit_upstream_read(trial)

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
