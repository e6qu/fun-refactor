#!/usr/bin/env python3
"""Measure progressive reports at the boundary versus one selected context packet."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "tests/agent-eval/agent-context-workspace.json"
sys.path.insert(0, str(ROOT / "sdk/python/src"))

from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient  # noqa: E402


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def fixture(root: Path) -> None:
    (root / "src").mkdir()
    (root / "src/lib.rs").write_text(
        "pub struct Item { pub name: String }\n"
        "pub fn render(item: &Item) -> String { item.name.clone() }\n"
        "pub fn caller() -> String { render(&Item { name: \"ok\".into() }) }\n",
        encoding="utf-8",
    )


def request(arguments) -> bytes:
    return canonical({"tool": "fr", "arguments": list(arguments)})


def without_opaque_identities(value):
    if isinstance(value, dict):
        return {
            key: without_opaque_identities(child)
            for key, child in value.items()
            if key not in {"handle", "parent"}
        }
    if isinstance(value, list):
        return [without_opaque_identities(child) for child in value]
    return value


def summarize_packet(packet: dict) -> dict:
    selected = without_opaque_identities(packet["selected"])
    return {
        "schema": packet["schema"],
        "view": packet["view"],
        "calls": packet["calls"],
        "serialized_bytes": packet["serialized_bytes"],
        "cached_objects": len(packet["cached_objects"]),
        "target": without_opaque_identities(packet["target"]),
        "selected": selected,
        "selected_sha256": digest(canonical(selected)),
    }


def direct_arm(binary: Path, root: Path, objects: Path) -> dict:
    client = FrClient(root, executable=binary)
    found = client.project("find", "render", "--signature")
    handle = found.at("/rows/0/0")
    session = client.context(
        handle, view="evidence", token_limit=4096,
        store=DirectoryObjectStore(objects),
    )
    session.materialize_section("code_map")
    packet = session.packet(
        {"code_map": "/model/code_map"}, include_actions=False, max_bytes=4096,
    ).to_data()
    visible = [
        request(found.arguments), canonical(found.to_data()),
    ]
    for report in session.reports:
        visible.extend((request(report.arguments), canonical(report.to_data())))
    return {
        "agent_visible_calls": 1 + session.calls,
        "internal_fr_calls": 1 + session.calls,
        "agent_visible_bytes": sum(map(len, visible)),
        "request_response_bytes": list(map(len, visible)),
        "packet": summarize_packet(packet),
    }


RUNTIME_PROGRAM = r'''# => selected content-addressed context packet
import json, sys
from fr_ir.context import DirectoryObjectStore
from fr_ir.runtime import FrClient
client = FrClient(sys.argv[1], executable=sys.argv[2])
found = client.project("find", "render", "--signature")
handle = found.at("/rows/0/0")
session = client.context(
    handle, view="evidence", token_limit=4096,
    store=DirectoryObjectStore(sys.argv[3]))
session.materialize_section("code_map")
packet = session.packet(
    {"code_map": "/model/code_map"}, include_actions=False, max_bytes=4096)
print(json.dumps(packet.to_data(), sort_keys=True, separators=(",", ":")))
'''


def runtime_arm(binary: Path, root: Path, objects: Path) -> dict:
    environment = os.environ.copy()
    environment["PYTHONPATH"] = str(ROOT / "sdk/python/src")
    completed = subprocess.run(
        ["python3", "-c", RUNTIME_PROGRAM, str(root), str(binary),
         str(objects)],
        capture_output=True, check=False, env=environment,
    )
    if completed.returncode:
        raise RuntimeError(completed.stderr.decode("utf-8", "replace"))
    packet = json.loads(completed.stdout)
    visible = [canonical({"tool": "python", "program": RUNTIME_PROGRAM}), canonical(packet)]
    return {
        "agent_visible_calls": 1,
        "internal_fr_calls": 1 + packet["calls"],
        "agent_visible_bytes": sum(map(len, visible)),
        "request_response_bytes": list(map(len, visible)),
        "packet": summarize_packet(packet),
    }


def generate(binary: Path) -> dict:
    binary = binary.resolve()
    with tempfile.TemporaryDirectory(prefix="fr-agent-context-workspace-") as directory:
        base = Path(directory)
        direct_root, runtime_root = base / "direct", base / "runtime"
        direct_root.mkdir()
        runtime_root.mkdir()
        fixture(direct_root)
        fixture(runtime_root)
        direct = direct_arm(binary, direct_root, base / "direct-objects")
        runtime = runtime_arm(binary, runtime_root, base / "runtime-objects")
    if direct["packet"] != runtime["packet"]:
        raise AssertionError(json.dumps(
            {"progressive_reports": direct["packet"], "selected_packet": runtime["packet"]},
            indent=2, sort_keys=True,
        ))
    return {
        "schema": "fr-agent-context-workspace-eval-1",
        "passed": True,
        "inputs": {
            "tool_sha256": digest(Path(__file__).read_bytes()),
            "runtime_sha256": digest((ROOT / "sdk/python/src/fr_ir/runtime.py").read_bytes()),
            "context_sha256": digest((ROOT / "sdk/python/src/fr_ir/context.py").read_bytes()),
        },
        "arms": {"progressive_reports": direct, "selected_packet": runtime},
        "reduction": {
            "agent_visible_calls": direct["agent_visible_calls"] - runtime["agent_visible_calls"],
            "agent_visible_bytes": direct["agent_visible_bytes"] - runtime["agent_visible_bytes"],
            "agent_visible_percent": round(
                100 * (direct["agent_visible_bytes"] - runtime["agent_visible_bytes"])
                / direct["agent_visible_bytes"], 1),
        },
        "scope": (
            "One deterministic generic Rust project. Both arms perform identical handle discovery, "
            "bounded exact disclosure traversal, recursive code-map materialization, digest checks "
            "and object-store writes. The report arm counts every fr request and response at the "
            "agent boundary. The packet arm counts its complete Python program and final selected "
            "packet while intermediate reports remain local. This does not run a model or measure "
            "tokens, hidden reasoning, billed quota or population behavior."
        ),
    }


def audit(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    assert value["schema"] == "fr-agent-context-workspace-eval-1" and value["passed"]
    reports = value["arms"]["progressive_reports"]
    packet = value["arms"]["selected_packet"]
    assert reports["packet"] == packet["packet"]
    assert reports["internal_fr_calls"] == packet["internal_fr_calls"]
    assert reports["agent_visible_calls"] > packet["agent_visible_calls"] == 1
    assert reports["agent_visible_bytes"] > packet["agent_visible_bytes"]
    assert value["reduction"]["agent_visible_calls"] > 0
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
