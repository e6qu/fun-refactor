#!/usr/bin/env python3
"""Freeze, run and replay bounded native OpenCode explanations; use fr-local-guard locally."""
import argparse
import json
from pathlib import Path
import sys

from agent_eval import native_mcp, opencode_native, source_coverage
from agent_eval.study import load


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "serve":
        config, destination = sys.argv[2:]
        with Path(destination).open("xb") as log:
            native_mcp.serve(load(Path(config)), log, sys.stdin.buffer, sys.stdout.buffer)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("freeze")
    freeze.add_argument("manifest", type=Path)
    freeze.add_argument("--binary", type=Path, required=True)
    freeze.add_argument("--guidance", type=Path, help="Freeze compact exploration and three arms with this public guide excerpt")
    freeze.add_argument("--source-references", action="store_true", help="Freeze direct source reads and citations by disclosed source ID")
    freeze.add_argument("--focused-pages", action="store_true",
                        help="Expose source-only and relationship-only fr pages; requires --source-references")
    freeze.add_argument("--recovery-hints", action="store_true",
                        help="Expose actionable handle refusals, source references and focused pages")
    run = commands.add_parser("run")
    run.add_argument("plan", type=Path)
    run.add_argument("cell")
    run.add_argument("output", type=Path)
    run.add_argument("--base", type=Path, required=True)
    run.add_argument("--binary", type=Path, required=True)
    run.add_argument("--opencode", type=Path, required=True)
    run.add_argument("--confirm-agent-spend", action="store_true")
    report = commands.add_parser("report")
    report.add_argument("plan", type=Path)
    report.add_argument("output", type=Path)
    review = commands.add_parser("review")
    review.add_argument("plan", type=Path)
    review.add_argument("output", type=Path)
    review.add_argument("--contiguous-source", action="store_true", help="Separately regrade exact citations across contiguous retrieved pages")
    args = parser.parse_args()
    try:
        if args.command == "freeze":
            if args.recovery_hints:
                result = opencode_native.freeze_recovery(load(args.manifest), args.manifest.resolve().parent, args.binary.resolve(), args.guidance)
            else:
                result = opencode_native.freeze(load(args.manifest), args.manifest.resolve().parent, args.binary.resolve(), args.guidance, source_references=args.source_references, focused_pages=args.focused_pages)
        elif args.command in {"report", "review"}:
            if args.command == "review":
                policy = source_coverage.POLICY if args.contiguous_source else None
                result = opencode_native.review(load(args.plan), args.output.resolve(), policy)
            else:
                result = opencode_native.report(load(args.plan), args.output.resolve())
        else:
            if not args.confirm_agent_spend:
                parser.error("run requires --confirm-agent-spend; no dollar cap is enforced")
            result = opencode_native.run_attempt(load(args.plan), args.cell, args.base.resolve(), args.output.resolve(), args.binary.resolve(), args.opencode.resolve())
        print(json.dumps(result, indent=2, allow_nan=False))
        if args.command == "run" and not result["passed"]:
            parser.exit(1)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, f"native rehearsal: {error}\n")


if __name__ == "__main__":
    main()
