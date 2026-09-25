#!/usr/bin/env python3
"""Retain and audit deterministic native recovery fault evidence."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import time

from evidence_basis import file_digest

ROOT = Path(__file__).resolve().parents[1]
BINDINGS = [
    "tools/host-recovery-acceptance.py", "tools/evidence_basis.py", "tests/host_recovery.rs", "Cargo.lock",
    "src/history.rs", "src/history/host.rs", "src/history/host/tests.rs",
    "src/history/failure.rs", "src/cli.rs", "src/transaction_kernel.rs",
    "tests/lean_kernels.rs", "tools/lean-guard.py", "tools/lean-resources.sh",
    "kernels/FrKernels/HostRecovery.lean", "kernels/FrKernels/EditPlan.lean", "kernels/HistoryMain.lean",
    "tests/agent-eval/host-recovery/task.json", "tests/agent-eval/host-recovery/baseline.json",
]


def bindings():
    return {name: file_digest(ROOT / name) for name in BINDINGS}


def run(arguments):
    env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
               LEAN_NUM_THREADS="1", FR_LEAN_JOBS="1")
    start = time.monotonic()
    result = subprocess.run(arguments, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=1800)
    assert result.returncode == 0, result.stdout
    return {"arguments": arguments, "exit_code": result.returncode,
            "seconds": time.monotonic() - start, "output": result.stdout}


def audit(value):
    assert value["schema"] == "fr-host-recovery-acceptance-1"
    assert value["source_bindings"] == bindings(), "stale host recovery evidence"
    assert value["baseline"]["passed"] == 22
    fault, model = value["runs"]
    assert fault["exit_code"] == model["exit_code"] == 0
    assert "7 passed; 0 failed" in fault["output"]
    assert "1 passed; 0 failed" in model["output"]
    for label in ["handled", "process-exit"]:
        count = int(re.search(rf"host-recovery {label} boundaries: (\d+)", fault["output"])[1])
        assert count == value["boundaries"][label] and count >= 400
    for action in ["Apply", "Undo", "Redo", "Recover"]:
        assert len(re.findall(rf"host-recovery {action} boundaries: \d+", fault["output"])) == 2
    assert value["model_cases"] == 224
    assert value["claims"] == {"process_interruption": True, "power_loss": False,
                                "source_correspondence_proved": False, "live_agent_trial": False}


def measure():
    before = bindings()
    fault = run(["cargo", "test", "--lib", "history::host::tests::", "--", "--test-threads", "2", "--nocapture"])
    model = run(["cargo", "test", "--test", "lean_kernels", "host_recovery_decisions", "--", "--test-threads", "2"])
    assert before == bindings(), "sources changed during measurement"
    return {"schema": "fr-host-recovery-acceptance-1", "source_bindings": before,
            "platform": platform.platform(), "baseline": json.loads((ROOT / BINDINGS[-1]).read_text()),
            "boundaries": {label: int(re.search(rf"host-recovery {label} boundaries: (\d+)", fault["output"])[1])
                           for label in ["handled", "process-exit"]},
            "model_cases": 224, "runs": [fault, model],
            "claims": {"process_interruption": True, "power_loss": False,
                       "source_correspondence_proved": False, "live_agent_trial": False}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--audit", type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure()
    audit(value)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(value, indent=2) + "\n")
    print(json.dumps({"boundaries": value["boundaries"], "model_cases": value["model_cases"], "audit": "passed"}))
