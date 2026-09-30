#!/usr/bin/env python3
"""Normalize retained usage or operate a host-owned study budget; never call a model."""
import argparse
import json
from pathlib import Path
import sqlite3

from agent_eval.provider_usage import FORMATS, normalize
from agent_eval.study import load
from agent_eval.study_budget import Budget


def budget_action(ledger, request):
    action = request["action"]
    if action in {"begin", "finish"}:
        getattr(ledger, action)(request["cell"])
    elif action == "reserve":
        return ledger.reserve(request["cell"], request["request"], request["agent"],
                              request["input_limit"], request["output_limit"])
    elif action in {"dispatch", "cancel", "unknown"}:
        getattr(ledger, action)(request["request"])
    elif action == "settle":
        return {"state": ledger.settle_response(request["request"], request["format"],
                                                 load(Path(request["response"])))}
    elif action != "snapshot":
        raise ValueError("unknown budget action")
    return ledger.snapshot()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    usage = commands.add_parser("normalize", help="Convert one final provider response to disjoint counters")
    usage.add_argument("format", choices=FORMATS)
    usage.add_argument("model", help="Exact frozen model identity")
    usage.add_argument("response", type=Path)
    budget = commands.add_parser("budget", help="Apply one host request atomically to a shared ledger")
    budget.add_argument("plan", type=Path)
    budget.add_argument("ledger", type=Path)
    budget.add_argument("request", type=Path, help="JSON action; see docs/agent-study.md")
    args = parser.parse_args()
    try:
        if args.command == "normalize":
            result = normalize(args.format, load(args.response), args.model)
        else:
            result = budget_action(Budget(args.ledger, load(args.plan)), load(args.request))
        print(json.dumps(result, indent=2, allow_nan=False))
    except (OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        parser.exit(2, f"host: {error}\n")


if __name__ == "__main__":
    main()
