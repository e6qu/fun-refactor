#!/usr/bin/env python3
"""Freeze, collect and replay bounded configured-client code changes."""
import argparse
import gzip
import json
from pathlib import Path
import sys

from agent_eval import source_reviews, terminal_changes as changes, terminal_change_runner as runner
from agent_eval.study import encode, require
from agent_eval.workspace_bundle import pack


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "serve":
        runner.serve(Path(sys.argv[2]), Path(sys.argv[3]))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("freeze", "collect", "capture", "report"))
    parser.add_argument("inputs", type=Path)
    parser.add_argument("positions", nargs="*")
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    parser.add_argument("--plan-sha")
    parser.add_argument("--confirm-agent-spend", action="store_true")
    args = parser.parse_args()
    if args.command == "freeze":
        require(len(args.positions) == 1 and args.fr and args.opencode, "supply destination and executable paths")
        design = json.loads(args.inputs.read_bytes())
        base = args.inputs.resolve().parent
        tasks = [{**{k: t[k] for k in ("id", "requirement", "public_feedback")},
                  "files": pack(base / t["source"], changes.edits.MAX_WORKSPACE),
                  "grader": json.loads((base / t["grader"]).read_bytes())} for t in design["tasks"]]
        frozen, snapshots = changes.freeze(tasks, design["models"], args.fr.resolve(), args.opencode.resolve(),
            json.loads((base / design["catalog"]).read_bytes()), design["provenance"])
        output = Path(args.positions[0])
        output.mkdir(parents=True, exist_ok=False)
        (output / "plan.json").write_bytes(encode(frozen))
        (output / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
        print(frozen["sha256"])
        return
    inputs = args.inputs.resolve()
    frozen = json.loads((inputs / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(inputs)
    changes.checked(frozen, snapshots)
    if args.command == "report":
        require(len(args.positions) == 1, "supply attempts directory")
        result = changes.report(frozen, snapshots, Path(args.positions[0]))
    else:
        require(len(args.positions) == 2 and args.fr and args.opencode, "supply cell, output and executable paths")
        cell_id, destination = args.positions
        if args.command == "capture":
            require(args.plan_sha == frozen["sha256"], "capture plan differs")
            cell = next(c for c in frozen["plan"]["cells"] if c["id"] == cell_id)
            runner.capture(frozen, snapshots, cell, Path(destination).resolve(), args.fr.resolve(), args.opencode.resolve())
            return
        require(args.confirm_agent_spend, "live collection requires explicit spend confirmation")
        result = runner.collect(frozen, snapshots, cell_id, inputs, Path(destination).resolve(), args.fr.resolve(), args.opencode.resolve())
    print(encode(result).decode())


if __name__ == "__main__":
    main()
