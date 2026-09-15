#!/usr/bin/env python3
"""Compare composed context/change calls with one intent-bound reviewed action."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))

from fr_ir.intent import AgentIntent, IntentAction  # noqa: E402
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget  # noqa: E402
from fr_ir.runtime import FrClient  # noqa: E402


BOUND_FILES = (
    "tools/intent-action-context.py",
    "src/cli.rs",
    "src/project/agent_intent.rs",
    "src/project/task_change.rs",
    "sdk/python/src/fr_ir/intent.py",
    "sdk/python/src/fr_ir/runtime.py",
)
SOURCE = "pub fn render(value: &str) -> String { value.to_owned() }\n"


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def bindings() -> dict[str, str]:
    return {name: sha256((ROOT / name).read_bytes()) for name in BOUND_FILES}


class CountingClient(FrClient):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.process_calls = 0
        self.request_bytes = 0
        self.response_bytes = 0

    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        self.process_calls += 1
        self.request_bytes += len(canonical({"arguments": arguments}))
        self.request_bytes += len(input_bytes or b"")
        self.response_bytes += len(canonical(report.to_data()))
        return report


def prepare(root: Path) -> None:
    (root / "src").mkdir(parents=True)
    (root / ".fr").mkdir()
    (root / "artifacts").mkdir()
    (root / "src/lib.rs").write_text(SOURCE, encoding="utf-8")
    (root / ".fr/checks.json").write_bytes(canonical({
        "schema": 1,
        "checks": [{
            "name": "syntax", "argv": ["true"], "cwd": ".",
            "timeout_seconds": 10, "covers": ["selected source state"],
        }],
    }))


def change(handle: str) -> TaskChange:
    return TaskChange(
        [],
        [TaskTarget("render-body", handle, "replace-body",
                    fragment="{ value.to_uppercase() }\n")],
        {"files-changed": 1, "edits": 1, "changed-operations": 1,
         "paths-changed": ["src/lib.rs"]},
        ["syntax"],
        TaskDelivery(patch="artifacts/change.patch"),
    )


def run_arm(root: Path, executable: str, bound: bool) -> dict[str, object]:
    discovery = FrClient(root, executable=executable)
    handle = discovery.project("find", "render", "--signature").at("/rows/0/0")
    client = CountingClient(root, executable=executable)
    task = change(handle)
    if bound:
        compiled = client.compile(AgentIntent(
            handle, "change", packet_limit=65_536, action=IntentAction(task),
        ))
        result = client.execute_intent(compiled)
        workflow = result.at("/action/workflow")
        basis_kind = compiled.action_basis.split(":", 1)[0]
    else:
        client.compile(AgentIntent(handle, "change", packet_limit=65_536))
        review = client.review(task)
        result = client.execute(review)
        workflow = result.at("/workflow")
        basis_kind = review.task_change_basis.split(":", 1)[0]
    source = (root / "src/lib.rs").read_bytes()
    patch = (root / "artifacts/change.patch").read_bytes()
    stages = [{"stage": row["stage"], "status": row["status"]}
              for row in workflow["stages"]]
    return {
        "process_calls": client.process_calls,
        "request_bytes": client.request_bytes,
        "response_bytes": client.response_bytes,
        "exchange_bytes": client.request_bytes + client.response_bytes,
        "source_sha256": sha256(source),
        "patch_sha256": sha256(patch),
        "transaction_status": workflow["transaction_status"],
        "stages": stages,
        "basis_kind": basis_kind,
    }


def measure(executable: str) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="fr-intent-action-") as directory:
        base = Path(directory)
        composed_root = base / "composed"
        bound_root = base / "bound"
        prepare(composed_root)
        shutil.copytree(composed_root, bound_root)
        composed = run_arm(composed_root, executable, False)
        bound = run_arm(bound_root, executable, True)
        equality = {
            key: composed[key] == bound[key]
            for key in ("source_sha256", "patch_sha256", "transaction_status", "stages")
        }
        return {
            "schema": "fr-intent-action-context-1",
            "fixture": {"source_sha256": sha256(SOURCE.encode()), "purpose": "change"},
            "composed": composed,
            "intent_action": bound,
            "reduction": {
                "process_calls": composed["process_calls"] - bound["process_calls"],
                "response_bytes": composed["response_bytes"] - bound["response_bytes"],
            },
            "cost": {
                "request_bytes": bound["request_bytes"] - composed["request_bytes"],
                "exchange_bytes": bound["exchange_bytes"] - composed["exchange_bytes"],
            },
            "equality": equality,
            "bindings": bindings(),
            "claim": "Deterministic process and serialized-byte comparison; no model, token, quota or population claim.",
        }


def audit(value: object) -> None:
    if not isinstance(value, dict) or value.get("schema") != "fr-intent-action-context-1":
        raise RuntimeError("intent action evidence schema is invalid")
    if value.get("bindings") != bindings():
        raise RuntimeError("intent action evidence source bindings are stale")
    composed = value.get("composed")
    bound = value.get("intent_action")
    reduction = value.get("reduction")
    cost = value.get("cost")
    equality = value.get("equality")
    if not all(isinstance(item, dict) for item in (composed, bound, reduction, cost, equality)):
        raise RuntimeError("intent action evidence rows are malformed")
    if (composed["process_calls"] != 3 or bound["process_calls"] != 2
            or reduction["process_calls"] != 1
            or reduction["response_bytes"] != composed["response_bytes"] - bound["response_bytes"]
            or reduction["response_bytes"] <= 0
            or cost["request_bytes"] != bound["request_bytes"] - composed["request_bytes"]
            or cost["exchange_bytes"] != bound["exchange_bytes"] - composed["exchange_bytes"]
            or cost["exchange_bytes"] < 0
            or not all(equality.values())
            or composed["transaction_status"] != "applied"
            or bound["basis_kind"] != "fraa1"):
        raise RuntimeError("intent action evidence equality or arithmetic is invalid")


def main() -> None:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--fr")
    group.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.fr is not None:
        value = measure(str(Path(args.fr).resolve()))
    else:
        value = json.loads(args.audit.read_text(encoding="utf-8"))
        audit(value)
    print(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
