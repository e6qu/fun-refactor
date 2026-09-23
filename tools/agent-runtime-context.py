#!/usr/bin/env python3
"""Compare direct JSON orchestration with the structured Python agent runtime."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "tests/agent-eval/agent-runtime-context.json"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode("utf-8")


def fixture(root: Path) -> None:
    (root / "src").mkdir()
    (root / ".fr").mkdir()
    (root / "artifacts").mkdir()
    (root / "src/lib.rs").write_text(
        "pub fn render(value: &str) -> String { value.to_owned() }\n"
        "pub fn caller() -> String { render(\"ok\") }\n", encoding="utf-8")
    (root / ".fr/checks.json").write_text(json.dumps({
        "schema": 1,
        "checks": [{"name": "syntax", "argv": ["true"], "cwd": ".",
                    "timeout_seconds": 10, "covers": ["selected source state"]}],
    }), encoding="utf-8")


def change_manifest(handle: str) -> dict:
    return {
        "schema": "fr-task-change-1", "requests": [],
        "targets": [{"id": "render-body", "handle": handle, "op": "replace-body",
                     "fragment": "{ value.to_uppercase() }"}],
        "postconditions": {"files-changed": 1, "edits": 1, "changed-operations": 1,
                           "paths-changed": ["src/lib.rs"]},
        "checks": ["syntax"],
        "delivery": {"check-original": True, "compact-success": True,
                     "exercise-reversal": True, "patch": "artifacts/change.patch",
                     "check-output-bytes": 2048},
    }


def fr(binary: Path, root: Path, arguments: list[str], visible: list[bytes], data=None):
    request = {"tool": "fr", "arguments": arguments}
    if data is not None:
        request["stdin"] = data.decode("utf-8")
    completed = subprocess.run(
        [str(binary), "--json", "-C", str(root), *arguments], input=data,
        capture_output=True, check=False,
    )
    if completed.returncode:
        raise RuntimeError(completed.stderr.decode("utf-8", "replace"))
    response = json.loads(completed.stdout)
    visible.extend((canonical(request), canonical(response)))
    return response


def outcome(root: Path, report: dict) -> dict:
    stages = report["workflow"]["stages"]
    return {
        "source_sha256": digest((root / "src/lib.rs").read_bytes()),
        "patch_sha256": digest((root / "artifacts/change.patch").read_bytes()),
        "transaction_status": report["workflow"]["transaction_status"],
        "stage_names": [stage["stage"] for stage in stages],
        "stage_statuses": [stage["status"] for stage in stages],
        "passed": report["passed"],
    }


def direct_arm(binary: Path, root: Path) -> dict:
    visible: list[bytes] = []
    found = fr(binary, root, ["project", "find", "render", "--signature"], visible)
    handle = found["rows"][0][0]
    initial = fr(binary, root, ["project", "disclose", handle, "--view", "evidence",
                                "--depth", "3", "--token-limit", "4096"], visible)
    action = next(row["reveal"]["arguments"] for row in initial["frontier"]
                  if row["domain"] == "project-evidence")
    fr(binary, root, action, visible)
    manifest = canonical(change_manifest(handle))
    preview = fr(binary, root, ["task-change", "--from", "-"], visible, manifest)
    completed = fr(binary, root, ["task-change", "--from", "-", "--write", "--basis",
                                  preview["task_change_basis"]], visible, manifest)
    return {
        "agent_visible_calls": 5,
        "internal_fr_calls": 5,
        "agent_visible_bytes": sum(len(value) for value in visible),
        "request_response_bytes": [len(value) for value in visible],
        "outcome": outcome(root, completed),
    }


RUNTIME_PROGRAM = r'''# => structured agent runtime comparison
import hashlib, json, sys
from pathlib import Path
from fr_ir.runtime import FrClient
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
root, binary = Path(sys.argv[1]), sys.argv[2]
client = FrClient(root, executable=binary)
found = client.project("find", "render", "--signature")
handle = found.definition_target().handle
initial = client.disclose(handle, view="evidence", depth=3, token_limit=4096)
client.follow(initial.actions(domain="project-evidence")[0])
change = TaskChange(
    [], [TaskTarget("render-body", handle, "replace-body",
                    fragment="{ value.to_uppercase() }")],
    {"files-changed": 1, "edits": 1, "changed-operations": 1,
     "paths-changed": ["src/lib.rs"]}, ["syntax"],
    TaskDelivery(patch="artifacts/change.patch"))
review = client.review(change)
result = client.execute(review)
stages = result.at("/workflow/stages")
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
print(json.dumps({
    "source_sha256": sha(root / "src/lib.rs"),
    "patch_sha256": sha(root / "artifacts/change.patch"),
    "transaction_status": result.at("/workflow/transaction_status"),
    "stage_names": [stage["stage"] for stage in stages],
    "stage_statuses": [stage["status"] for stage in stages],
    "passed": result.passed,
}, sort_keys=True, separators=(",", ":")))
'''


def runtime_arm(binary: Path, root: Path) -> dict:
    environment = os.environ.copy()
    environment["PYTHONPATH"] = str(ROOT / "sdk/python/src")
    completed = subprocess.run(
        ["python3", "-c", RUNTIME_PROGRAM, str(root), str(binary)],
        capture_output=True, check=False, env=environment,
    )
    if completed.returncode:
        raise RuntimeError(completed.stderr.decode("utf-8", "replace"))
    result = json.loads(completed.stdout)
    request = canonical({"tool": "python", "program": RUNTIME_PROGRAM})
    response = canonical(result)
    return {
        "agent_visible_calls": 1,
        "internal_fr_calls": 5,
        "agent_visible_bytes": len(request) + len(response),
        "request_response_bytes": [len(request), len(response)],
        "outcome": result,
    }


def generate(binary: Path) -> dict:
    binary = binary.resolve()
    with tempfile.TemporaryDirectory(prefix="fr-agent-runtime-context-") as directory:
        base = Path(directory)
        direct_root, runtime_root = base / "direct", base / "runtime"
        direct_root.mkdir()
        runtime_root.mkdir()
        fixture(direct_root)
        fixture(runtime_root)
        direct = direct_arm(binary, direct_root)
        runtime = runtime_arm(binary, runtime_root)
    assert direct["outcome"] == runtime["outcome"]
    return {
        "schema": "fr-agent-runtime-context-1",
        "passed": True,
        "inputs": {
            "tool_sha256": digest(Path(__file__).read_bytes()),
            "runtime_sha256": digest((ROOT / "sdk/python/src/fr_ir/runtime.py").read_bytes()),
            "sdk_sha256": digest((ROOT / "sdk/python/src/fr_ir/ir.py").read_bytes()),
        },
        "arms": {"direct_json": direct, "python_runtime": runtime},
        "reduction": {
            "agent_visible_calls": direct["agent_visible_calls"] - runtime["agent_visible_calls"],
            "agent_visible_bytes": direct["agent_visible_bytes"] - runtime["agent_visible_bytes"],
            "agent_visible_percent": round(
                100 * (direct["agent_visible_bytes"] - runtime["agent_visible_bytes"])
                / direct["agent_visible_bytes"], 1),
        },
        "scope": "One deterministic generic Rust task. Both arms execute the same five fr operations and produce equal source, patch and lifecycle outcomes. Direct JSON counts every request and response at the agent boundary. The runtime arm counts its complete Python program and compact result; intermediate reports stay inside the local process. This does not measure a model, hidden reasoning, tokens, billed quota or population behavior.",
    }


def audit(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    assert value["schema"] == "fr-agent-runtime-context-1" and value["passed"]
    direct, runtime = value["arms"]["direct_json"], value["arms"]["python_runtime"]
    assert direct["outcome"] == runtime["outcome"]
    assert direct["agent_visible_calls"] == 5 and runtime["agent_visible_calls"] == 1
    assert direct["internal_fr_calls"] == runtime["internal_fr_calls"] == 5
    assert direct["agent_visible_bytes"] > runtime["agent_visible_bytes"]
    assert value["reduction"]["agent_visible_calls"] == 4
    return value


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--output", type=Path, default=REPORT)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.audit:
        report = audit(args.audit)
    else:
        if args.fr is None:
            parser.error("--fr is required when generating")
        report = generate(args.fr)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        audit(args.output)
    print(json.dumps(report["reduction"], sort_keys=True))


if __name__ == "__main__":
    main()
