#!/usr/bin/env python3
"""Plan a matched study or audit its retained evidence without invoking agents."""
import argparse
import json
from pathlib import Path

from agent_eval.study import load, plan
from agent_eval.study_report import report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("plan", help="Freeze tasks, settings, budgets and randomized arm order")
    freeze.add_argument("manifest", type=Path)
    audit = commands.add_parser("report", help="Include every planned cell, including failures and missing evidence")
    audit.add_argument("plan", type=Path)
    audit.add_argument("attempts", type=Path)
    audit.add_argument("--require-complete", action="store_true", help="Exit 1 for unfinished accounting or budget violations")
    args = parser.parse_args()
    try:
        result = plan(load(args.manifest)) if args.command == "plan" else report(load(args.plan), args.attempts)
        print(json.dumps(result, indent=2, allow_nan=False))
        if args.command == "report" and args.require_complete and not result["audit_complete"]:
            parser.exit(1, "study: evidence is incomplete or a budget was exceeded\n")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"study: {error}\n")


if __name__ == "__main__":
    main()
