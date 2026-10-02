#!/usr/bin/env python3
"""Freeze, collect and replay native OpenCode patches; grade only on a GitHub runner."""
import argparse
import json
from pathlib import Path
import os
import sys

from agent_eval import native_changes, opencode_changes
from agent_eval.study import load, require


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "serve":
        config, destination = sys.argv[2:]
        with Path(destination).open("xb") as log:
            native_changes.serve(load(Path(config)), log, sys.stdin.buffer, sys.stdout.buffer)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("freeze")
    freeze.add_argument("manifest", type=Path)
    freeze.add_argument("--binary", type=Path, required=True)
    freeze.add_argument("--save-runner", type=Path, help="Retain an independently executable copy in a fresh directory")
    freeze.add_argument("--public-edits", action="store_true", help="Offer public fr author previews and history apply in the fr arm")
    run = commands.add_parser("run")
    run.add_argument("plan", type=Path)
    run.add_argument("cell")
    run.add_argument("output", type=Path)
    run.add_argument("--binary", type=Path, required=True)
    run.add_argument("--opencode", type=Path, required=True)
    run.add_argument("--confirm-agent-spend", action="store_true")
    for name in ("replay", "grade"):
        sub = commands.add_parser(name)
        sub.add_argument("plan", type=Path)
        sub.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "freeze":
            result = opencode_changes.freeze(load(args.manifest), args.manifest.resolve().parent, args.binary.resolve(), public_edits=args.public_edits)
            if args.save_runner:
                opencode_changes.retain_runner(result, args.save_runner.resolve())
        elif args.command == "run":
            require(args.confirm_agent_spend, "run requires --confirm-agent-spend; no dollar cap is enforced")
            result = opencode_changes.run_attempt(load(args.plan), args.cell, args.output.resolve(), args.binary.resolve(), args.opencode.resolve())
        elif args.command == "grade":
            require(os.environ.get("GITHUB_ACTIONS") == "true", "candidate grading belongs on a GitHub runner")
            result = opencode_changes.grade_attempts(load(args.plan), args.output.resolve())
        else:
            result = opencode_changes.replay(load(args.plan), args.output.resolve())
        print(json.dumps(result, indent=2, allow_nan=False))
        if args.command == "run" and result["status"] != "submitted":
            parser.exit(1)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"native changes: {error}\n")


if __name__ == "__main__":
    main()
