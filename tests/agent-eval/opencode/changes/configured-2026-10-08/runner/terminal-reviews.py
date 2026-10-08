#!/usr/bin/env python3
"""Freeze, collect and replay bounded source reviews with terminal native answers."""
import argparse
import gzip
from pathlib import Path

from agent_eval import native_mcp as mcp, source_reviews, terminal_reviews as review
from agent_eval import terminal_review_runner as runner
from agent_eval.study import encode, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze = commands.add_parser("freeze")
    freeze.add_argument("questions", type=Path)
    freeze.add_argument("models", type=Path)
    freeze.add_argument("output", type=Path)
    freeze.add_argument("--fr", type=Path, required=True)
    freeze.add_argument("--opencode", type=Path, required=True)
    for name in ("collect", "capture", "preflight", "collect-all"):
        command = commands.add_parser(name)
        command.add_argument("inputs", type=Path)
        if name in ("collect", "capture"):
            command.add_argument("cell")
        command.add_argument("output", type=Path)
        command.add_argument("--fr", type=Path, required=True)
        command.add_argument("--opencode", type=Path, required=True)
        if name in ("capture", "collect-all"):
            command.add_argument("--plan-sha", required=True)
    report = commands.add_parser("report")
    report.add_argument("inputs", type=Path)
    report.add_argument("output", type=Path)
    serve = commands.add_parser("serve")
    serve.add_argument("config", type=Path)
    serve.add_argument("log", type=Path)
    args = parser.parse_args()
    if args.command == "serve":
        runner.serve(args.config, args.log)
        return
    if args.command == "freeze":
        frozen, snapshots = review.freeze(mcp.decode(review.read(args.questions)),
            mcp.decode(review.read(args.models)), args.fr.resolve(), args.opencode.resolve(),
            {"scope": "New terminal source review; no previous collection is resumed."})
        archived = gzip.compress(encode(snapshots), mtime=0)
        require(len(archived) <= review.MAX_BYTES, "review input archive exceeds replay budget")
        args.output.mkdir(parents=True, exist_ok=False)
        (args.output / "plan.json").write_bytes(encode(frozen))
        (args.output / "inputs.json.gz").write_bytes(archived)
        print(frozen["sha256"])
        return
    inputs = args.inputs.resolve()
    frozen = mcp.decode(review.read(inputs / "plan.json"))
    snapshots = source_reviews.read_inputs(inputs)
    if args.command == "report":
        result = review.report(frozen, snapshots, args.output.resolve())
    elif args.command in ("preflight", "collect-all"):
        from agent_eval import terminal_review_collection as collection
        params = (frozen, snapshots, args.output.resolve(), args.fr.resolve(), args.opencode.resolve())
        result = (collection.preflight(*params) if args.command == "preflight" else
                  collection.collect_all(*params, inputs, args.plan_sha))
    elif args.command == "collect":
        result = runner.collect(frozen, snapshots, args.cell, args.output.resolve(),
                                args.fr.resolve(), args.opencode.resolve(), inputs)
    else:
        require(frozen["sha256"] == args.plan_sha, "capture plan differs from parent")
        plan = review.checked(frozen, snapshots, execution=True)
        cell = next(c for c in plan["cells"] if c["id"] == args.cell)
        runner.capture(frozen, snapshots, cell, args.output.resolve(), args.fr.resolve(), args.opencode.resolve())
        return
    print(encode(result).decode())


if __name__ == "__main__":
    main()
