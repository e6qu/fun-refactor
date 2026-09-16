#!/usr/bin/env python3
"""Measure complete agent programs and internal traffic for a checked scalar change."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from fr_ir import runtime  # noqa: E402

SOURCE = "pub fn calculate(value: i64) -> i64 {\n    return value + 7;\n}\n"
MANUAL = '''from fr_ir.ir import ScalarRequest, TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient
client = FrClient(WORKSPACE, executable=FR)
handle = client.project("find", "calculate", "--signature").at("/rows/0/0")
review = client.review(TaskChange([], [TaskTarget(
    "scalar", handle, "edit-body-scalar", scalar=ScalarRequest("set-int", "7", "9"),
)], {"files-changed": 1, "edits": 1, "changed-operations": 1,
    "paths-changed": ["src/lib.rs"]}, ["compiler"],
    TaskDelivery(patch="artifacts/change.patch", check_output_bytes=256)))
result = client.execute(review)
packet = {"passed": result.passed, "workflow": result.at("/workflow")}
'''
GUIDED = '''from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient
client = FrClient(WORKSPACE, executable=FR)
guide = client.guide(AgentGoal("change", selector=GoalSelector(name="calculate"),
    operation=GoalOperation("semantic-scalar", {"operation": "set-int", "from": "7", "to": "9"}),
    checks=("compiler",), delivery=TaskDelivery(patch="artifacts/change.patch", check_output_bytes=256)))
review = client.follow_guide(guide.actions()[0])
result = client.execute(review)
packet = {"passed": result.passed, "workflow": result.at("/workflow")}
'''
BOUND_FILES = (
    "tools/agent-guide-context.py", "src/cli.rs", "src/capabilities.rs",
    "src/project/agent_guide.rs", "src/project/task_change.rs",
    "sdk/python/src/fr_ir/guide.py", "sdk/python/src/fr_ir/ir.py",
    "sdk/python/src/fr_ir/runtime.py", "kernels/FrKernels/AgentGuide.lean",
)
STAGES = ["check-original", "apply", "check-applied", "undo", "check-restored", "redo", "check-applied", "deliver-patch"]
def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode()


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def bindings() -> dict[str, str]:
    return {name: digest((ROOT / name).read_bytes()) for name in BOUND_FILES}


class CountingClient(runtime.FrClient):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.calls = []

    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        self.calls.append({"arguments": list(arguments),
                           "request_bytes": len(canonical({"arguments": arguments})) + len(input_bytes or b""),
                           "response_bytes": len(canonical(report.to_data()))})
        return report


def prepare(root: Path) -> None:
    (root / "src").mkdir(parents=True)
    (root / ".fr").mkdir()
    (root / "artifacts").mkdir()
    (root / "src/lib.rs").write_text(SOURCE)
    (root / ".fr/check.py").write_text('''import pathlib, subprocess, tempfile
source = pathlib.Path("src/lib.rs").read_text()
with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory)
    harness = source + "\\nfn main() { let offset = calculate(0); for value in -16..=16 { assert_eq!(calculate(value), value + offset); } }\\n"
    (root / "main.rs").write_text(harness)
    subprocess.run(["rustc", str(root / "main.rs"), "-o", str(root / "check")], check=True)
    subprocess.run([str(root / "check")], check=True)
''')
    (root / ".fr/checks.json").write_bytes(canonical({"schema": 1, "checks": [{
        "name": "compiler", "argv": ["python3", ".fr/check.py"], "cwd": ".",
        "timeout_seconds": 30, "covers": ["Rust compiler", "33 finite affine cases"],
    }]}))


def run_arm(root: Path, executable: str, program: str) -> dict:
    scope = {"WORKSPACE": str(root), "FR": executable}
    original = runtime.FrClient
    runtime.FrClient = CountingClient
    try:
        exec(compile(program, "<retained-agent-program>", "exec"), scope)
    finally:
        runtime.FrClient = original
    client = scope["client"]
    result = scope["packet"]
    workflow = result["workflow"]
    stages = [{"stage": row["stage"], "status": row["status"]} for row in workflow["stages"]]
    packet = {"passed": result["passed"], "transaction_status": workflow["transaction_status"], "stages": stages}
    source = (root / "src/lib.rs").read_bytes()
    expected = SOURCE.replace("+ 7", "+ 9").encode()
    if source != expected or not result["passed"]:
        raise RuntimeError(f"guided/manual source or lifecycle oracle failed: {source!r}; passed={result['passed']}")
    with tempfile.TemporaryDirectory() as directory:
        oracle = Path(directory)
        (oracle / "main.rs").write_bytes(source + b"fn main() { for value in -16..=16 { assert_eq!(calculate(value), value + 9); } }\n")
        subprocess.run(["rustc", str(oracle / "main.rs"), "-o", str(oracle / "check")], check=True)
        subprocess.run([str(oracle / "check")], check=True)
    request = {"program": program, "bindings": {"WORKSPACE": "<workspace>", "FR": "<fr>"}}
    request_bytes, response_bytes = len(canonical(request)), len(canonical(packet))
    internal_requests = sum(row["request_bytes"] for row in client.calls)
    internal_responses = sum(row["response_bytes"] for row in client.calls)
    return {"program": program, "program_sha256": digest(program.encode()), "agent_request": request,
            "agent_packet": packet, "agent_request_bytes": request_bytes, "agent_response_bytes": response_bytes,
            "agent_exchange_bytes": request_bytes + response_bytes, "agent_exchanges": 1,
            "process_calls": len(client.calls), "calls": client.calls,
            "internal_request_bytes": internal_requests, "internal_response_bytes": internal_responses,
            "internal_exchange_bytes": internal_requests + internal_responses,
            "source_sha256": digest(source), "patch_sha256": digest((root / "artifacts/change.patch").read_bytes()),
            "behavior_oracle_cases": 33}


def measure(executable: str) -> dict:
    with tempfile.TemporaryDirectory(prefix="fr-agent-guide-") as directory:
        root = Path(directory)
        prepare(root / "manual")
        shutil.copytree(root / "manual", root / "guided")
        manual = run_arm(root / "manual", executable, MANUAL)
        guided = run_arm(root / "guided", executable, GUIDED)
        return {"schema": "fr-agent-guide-context-1", "bindings": bindings(),
                "manual": manual, "guided": guided,
                "equality": {key: manual[key] == guided[key] for key in ("source_sha256", "patch_sha256", "agent_packet")},
                "difference": {key: guided[key] - manual[key] for key in ("agent_exchange_bytes", "process_calls", "internal_exchange_bytes")},
                "claim": "Fixed complete agent programs and serialized protocol bytes; no model, token, quota or population claim."}


def audit(value: dict) -> None:
    if value.get("schema") != "fr-agent-guide-context-1" or value.get("bindings") != bindings():
        raise RuntimeError("guide comparison schema or source bindings are stale")
    for name, program, count in (("manual", MANUAL, 3), ("guided", GUIDED, 4)):
        row = value[name]
        if row["program"] != program or row["program_sha256"] != digest(program.encode()):
            raise RuntimeError("retained agent program is changed")
        if row["agent_request"] != {"program": program, "bindings": {"WORKSPACE": "<workspace>", "FR": "<fr>"}}:
            raise RuntimeError("agent request does not retain its complete program")
        if row["agent_exchanges"] != 1 or row["process_calls"] != count or len(row["calls"]) != count:
            raise RuntimeError("guide process/exchange count is inconsistent")
        for prefix in ("agent", "internal"):
            requests = len(canonical(row["agent_request"])) if prefix == "agent" else sum(call["request_bytes"] for call in row["calls"])
            responses = len(canonical(row["agent_packet"])) if prefix == "agent" else sum(call["response_bytes"] for call in row["calls"])
            if (row[prefix + "_request_bytes"], row[prefix + "_response_bytes"], row[prefix + "_exchange_bytes"]) != (requests, responses, requests + responses):
                raise RuntimeError("guide byte accounting is inconsistent")
        packet = row["agent_packet"]
        if not packet["passed"] or packet["transaction_status"] != "applied" or len(packet["stages"]) != 8:
            raise RuntimeError("guide lifecycle evidence is incomplete")
        if packet["stages"] != [{"stage": stage, "status": "passed"} for stage in STAGES] or row["behavior_oracle_cases"] != 33:
            raise RuntimeError("guide checks or behavior oracle failed")
        if row["source_sha256"] != digest(SOURCE.replace("+ 7", "+ 9").encode()):
            raise RuntimeError("guide final source oracle differs")
    for key in ("source_sha256", "patch_sha256", "agent_packet"):
        if not value["equality"].get(key) or value["manual"][key] != value["guided"][key]:
            raise RuntimeError("manual and guided outputs differ")
    if value["difference"] != {key: value["guided"][key] - value["manual"][key] for key in ("agent_exchange_bytes", "process_calls", "internal_exchange_bytes")}:
        raise RuntimeError("guide comparison differences are inconsistent")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", default=str(ROOT / "target/debug/fr"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(str(Path(args.fr).resolve()))
    audit(value)
    output = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
