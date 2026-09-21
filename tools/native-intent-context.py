#!/usr/bin/env python3
"""Compare native and progressive compilation of one declarative agent intent."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))

from fr_ir.intent import AgentIntent  # noqa: E402
from fr_ir.runtime import FrClient  # noqa: E402


BOUND_FILES = (
    "tools/native-intent-context.py",
    "src/project/agent_intent.rs",
    "src/project/disclose.rs",
    "sdk/python/src/fr_ir/intent.py",
    "sdk/python/src/fr_ir/runtime.py",
    "sdk/python/src/fr_ir/context.py",
    "src/project/agent_actions.rs", "src/project/capability_action.rs",
    "sdk/python/src/fr_ir/intent_actions.py",
)


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def digest(value: object) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def stable_handles(value: object) -> object:
    if isinstance(value, str):
        return re.sub(r"frp1:[0-9a-f]{32}:", "frp1:<revision>:", value)
    if isinstance(value, list):
        return [stable_handles(item) for item in value]
    if isinstance(value, dict):
        return {key: stable_handles(item) for key, item in value.items()}
    return value


def bindings() -> dict[str, str]:
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
            for name in BOUND_FILES}


class CountingClient(FrClient):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.process_calls = 0
        self.response_bytes = 0

    def call(self, *arguments, input_bytes=None):
        report = super().call(*arguments, input_bytes=input_bytes)
        self.process_calls += 1
        self.response_bytes += len(canonical(report.to_data()))
        return report


def measure(executable: str) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="fr-native-intent-") as directory:
        root = Path(directory)
        (root / "src").mkdir()
        (root / "src/lib.rs").write_text(
            "pub fn render(value: &str) -> String { value.to_owned() }\n"
            "pub fn caller() -> String { render(\"ok\") }\n",
            encoding="utf-8",
        )
        discovery = FrClient(root, executable=executable)
        handle = discovery.project("find", "render", "--signature").at("/rows/0/0")
        intent = AgentIntent(handle, "trace", call_limit=192, packet_limit=65_536)

        progressive_client = CountingClient(root, executable=executable)
        progressive = progressive_client.prepare(intent)
        native_client = CountingClient(root, executable=executable)
        native = native_client.compile(intent)
        progressive_selected = progressive.at("/selected")
        native_selected = native.at("/selected")
        if progressive_selected != native_selected:
            raise RuntimeError("native and progressive intent selections differ")

        return {
            "schema": "fr-native-intent-context-1",
            "fixture": {
                "source_sha256": hashlib.sha256((root / "src/lib.rs").read_bytes()).hexdigest(),
                "purpose": "trace",
                "selected_sha256": digest(stable_handles(native_selected)),
            },
            "progressive": {
                "process_calls": progressive_client.process_calls,
                "response_bytes": progressive_client.response_bytes,
                "packet_bytes": progressive.at("/serialized_bytes"),
            },
            "native": {
                "process_calls": native_client.process_calls,
                "response_bytes": native_client.response_bytes,
                "packet_bytes": native.at("/serialized_bytes"),
                "progressive_disclosure_calls": native.at("/calls"),
            },
            "reduction": {
                "process_calls": progressive_client.process_calls - native_client.process_calls,
                "response_bytes": progressive_client.response_bytes - native_client.response_bytes,
            },
            "bindings": bindings(),
            "limits": {"packet_bytes": 65_536, "progressive_calls": 192},
            "claim": "Deterministic subprocess and serialized-byte measurement; no model, token, quota or population claim.",
        }


def audit(value: object) -> None:
    if not isinstance(value, dict) or value.get("schema") != "fr-native-intent-context-1":
        raise RuntimeError("native intent evidence schema is invalid")
    if value.get("bindings") != bindings():
        raise RuntimeError("native intent evidence source bindings are stale")
    progressive = value.get("progressive")
    native = value.get("native")
    reduction = value.get("reduction")
    if (
        not isinstance(progressive, dict)
        or not isinstance(native, dict)
        or not isinstance(reduction, dict)
    ):
        raise RuntimeError("native intent evidence rows are malformed")
    if (native["process_calls"] != 1 or native["progressive_disclosure_calls"] != 0
            or progressive["process_calls"] <= native["process_calls"]
            or reduction["process_calls"] != progressive["process_calls"] - 1
            or reduction["response_bytes"] != progressive["response_bytes"] - native["response_bytes"]
            or not 0 < progressive["packet_bytes"] <= value["limits"]["packet_bytes"]
            or not 0 < native["packet_bytes"] <= value["limits"]["packet_bytes"]):
        raise RuntimeError("native intent evidence arithmetic or bounds are invalid")


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
