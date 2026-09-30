#!/usr/bin/env python3
"""Run study requests through budget controls or grade code in an isolated container."""
import argparse
import json
from pathlib import Path
import sqlite3

from agent_eval.provider_usage import FORMATS, normalize
from agent_eval.study import checked_plan, load
from agent_eval.study_budget import Budget
from agent_eval.request_gateway import send
from agent_eval.isolated_grade import grade


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
    request = commands.add_parser("send", help="Count, reserve, send once and settle a provider request")
    request.add_argument("plan", type=Path)
    request.add_argument("ledger", type=Path)
    request.add_argument("cell")
    request.add_argument("agent")
    request.add_argument("identity")
    request.add_argument("input", type=Path)
    request.add_argument("evidence", type=Path)
    request.add_argument("--parent")
    request.add_argument("--timeout", type=int, default=60)
    request.add_argument("--confirm-agent-spend", action="store_true")
    check = commands.add_parser("grade", help="Compare isolated candidate output with private pinned cases")
    check.add_argument("plan", type=Path)
    check.add_argument("cell")
    check.add_argument("candidate", type=Path)
    check.add_argument("grader", type=Path)
    args = parser.parse_args()
    if args.command == "send" and not args.confirm_agent_spend:
        parser.error("--confirm-agent-spend is required; this command can incur provider charges")
    try:
        if args.command == "normalize":
            result = normalize(args.format, load(args.response), args.model)
        elif args.command == "send":
            result = send(Budget(args.ledger, load(args.plan)), args.cell, args.agent, args.identity,
                          load(args.input), args.evidence, parent=args.parent, timeout=args.timeout)
        elif args.command == "grade":
            frozen = checked_plan(load(args.plan))
            cell = next((cell for cell in frozen["cells"] if cell["id"] == args.cell), None)
            if cell is None:
                raise ValueError("unplanned cell")
            task = next(task for task in frozen["manifest"]["tasks"] if task["id"] == cell["task"])
            result = grade(args.candidate, args.grader, task["grader_sha256"])
        else:
            result = budget_action(Budget(args.ledger, load(args.plan)), load(args.request))
        print(json.dumps(result, indent=2, allow_nan=False))
        if args.command == "send" and result["state"] != "settled":
            parser.exit(1, "host: request charge is unresolved or exceeded its reservation\n")
        if args.command == "grade" and result["outcome"] != "passed":
            parser.exit(1, "host: candidate did not pass the pinned grader\n")
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, sqlite3.Error) as error:
        parser.exit(2, f"host: {error}\n")


if __name__ == "__main__":
    main()
