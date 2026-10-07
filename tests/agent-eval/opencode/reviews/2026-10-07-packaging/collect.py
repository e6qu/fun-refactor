#!/usr/bin/env python3
"""Freeze and retain two whole-task reviews with an admitted catalog override."""
import argparse
import gzip
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(REPO / "tools"))
from agent_eval import source_reviews, terminal_reviews as review, terminal_review_runner as runner
from agent_eval.study import encode, require

EVIDENCE = REPO / "tests/agent-eval/opencode/memory/2026-10-07-catalog"
DESIGN = REPO / "tests/agent-eval/opencode/reviews/2026-10-06-packaging-inputs/design.json"


def bindings():
    paths = [Path(__file__), HERE / "catalog.json", DESIGN,
             EVIDENCE / "manifest.json", EVIDENCE / "report.json", EVIDENCE / "workstation.json.gz"]
    return {str(path.relative_to(REPO)): source_reviews.identity(path) for path in paths}


def freeze(binary, client):
    require(not (HERE / "plan.json").exists(), "collection is already frozen")
    require((HERE / "catalog.json").read_bytes() == b"{}\n", "catalog differs")
    measured = json.loads((EVIDENCE / "report.json").read_bytes())
    require(all(measured[name]["admitted"] for name in ("macos-14", "ubuntu-latest")), "hosted admission failed")
    local = measured["workstation"]
    attempt = local["control"][0]["report"]["attempts"][0]
    require(attempt["status"] == "completed" and attempt["process"]["sampled_aggregate_rss_bytes"] <= 640 * 1024**2,
            "workstation control failed")
    require(all(local["model_list"]["models"].values()), "configured models unavailable")
    spec = importlib.util.spec_from_file_location("prepare_review", REPO / "tools/prepare-terminal-reviews.py")
    prepare = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prepare)
    design = json.loads(DESIGN.read_bytes())
    questions, provenance = prepare.prepare(design, REPO)
    provenance.update(bindings=bindings(), environment={"BUN_OPTIONS": "", "OPENCODE_MODELS_PATH": "catalog.json"},
                      stop_rule="Stop this collection at its first resource-limit failure; no retries.")
    frozen, snapshots = review.freeze(questions, design["models"], binary, client, provenance)
    (HERE / "plan.json").write_bytes(encode(frozen))
    (HERE / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
    print(frozen["sha256"])


def collect(cell, binary, client):
    require(not (HERE / "stop.json").exists(), "collection permanently stopped")
    frozen = json.loads((HERE / "plan.json").read_bytes())
    require(frozen["plan"]["provenance"]["bindings"] == bindings(), "collection inputs changed")
    for path in [*bindings(), str((HERE / "plan.json").relative_to(REPO)), str((HERE / "inputs.json.gz").relative_to(REPO))]:
        committed = subprocess.run(["git", "show", "HEAD:" + path], cwd=REPO, capture_output=True, check=True).stdout
        require(committed == (REPO / path).read_bytes(), "commit the frozen collection before calls")
    snapshots = source_reviews.read_inputs(HERE)
    with patch.dict(os.environ, {"OPENCODE_MODELS_PATH": str(HERE / "catalog.json"), "BUN_OPTIONS": "", "RAYON_NUM_THREADS": "1"}):
        record = runner.collect(frozen, snapshots, cell, HERE / "attempts", binary, client, HERE)
    result = review.report(frozen, snapshots, HERE / "attempts")
    (HERE / "report.json").write_bytes(encode(result))
    process = json.loads((HERE / "attempts" / cell / "process.json").read_bytes())
    if process["stop_reason"] is not None:
        (HERE / "stop.json").write_bytes(encode({"cell": cell, "reason": process["stop_reason"], "resume_allowed": False}))
    print(encode({"cell": cell, "status": record["status"], "failure": record["failure"], "process": process}).decode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("freeze", "collect", "report"))
    parser.add_argument("--cell")
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    if args.command == "report":
        result = review.report(json.loads((HERE / "plan.json").read_bytes()), source_reviews.read_inputs(HERE), HERE / "attempts")
        print(encode(result).decode())
    else:
        require(args.fr is not None and args.opencode is not None, "supply both binaries")
        if args.command == "freeze":
            freeze(args.fr.resolve(), args.opencode.resolve())
        else:
            require(args.cell is not None, "select one frozen cell")
            collect(args.cell, args.fr.resolve(), args.opencode.resolve())
