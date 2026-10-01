#!/usr/bin/env python3
"""Freeze, run or audit small OpenCode comparisons. Run locally through fr-local-guard."""
import argparse
import json
from pathlib import Path

from agent_eval.opencode_rehearsal import freeze, report, run_attempt
from agent_eval.study import load


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("freeze")
    prepare.add_argument("manifest", type=Path)
    prepare.add_argument("--binary", type=Path, required=True)
    execute = commands.add_parser("run")
    execute.add_argument("plan", type=Path)
    execute.add_argument("cell")
    execute.add_argument("output", type=Path)
    execute.add_argument("--base", type=Path, required=True)
    execute.add_argument("--binary", type=Path, required=True)
    execute.add_argument("--opencode", type=Path, required=True)
    execute.add_argument("--confirm-agent-spend", action="store_true")
    audit = commands.add_parser("report")
    audit.add_argument("plan", type=Path)
    audit.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "freeze":
            result = freeze(load(args.manifest), args.manifest.resolve().parent, args.binary.resolve())
        elif args.command == "run":
            if not args.confirm_agent_spend:
                parser.error("run requires --confirm-agent-spend; OpenCode cannot enforce a dollar cap")
            result = run_attempt(load(args.plan), args.cell, args.base.resolve(), args.output.resolve(),
                                 args.binary.resolve(), args.opencode.resolve())
        else:
            result = report(load(args.plan), args.output.resolve())
        print(json.dumps(result, indent=2, allow_nan=False))
        if args.command == "run" and (result["status"] != "completed" or not result["grade"]["passed"]):
            parser.exit(1)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"rehearsal: {error}\n")


if __name__ == "__main__":
    main()
