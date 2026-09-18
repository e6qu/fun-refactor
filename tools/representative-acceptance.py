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


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "tests/agent-eval/representative-acceptance.json"
REQUIRED = {
    "unfamiliar-upstream", "rust-multi-file", "typescript-react",
    "css-tailwind-mermaid", "backend-migration", "agent-authored-lean-proof",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load() -> dict[str, object]:
    value = json.loads(REGISTRY.read_text())
    if value.get("schema") != "fr-representative-acceptance-1":
        raise ValueError("representative acceptance registry has the wrong schema")
    return value


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
