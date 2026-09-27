#!/usr/bin/env python3
"""Prepare, run, score and retain unfamiliar upstream investigation pairs."""
import argparse
import json
import os
from pathlib import Path
import sys

from agent_eval import investigation as trial
from agent_eval import investigation_run as live
from agent_eval import investigation_rehearsal as rehearsal


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "rehearse"):
        prepare = commands.add_parser(name)
        prepare.add_argument("session", type=Path)
        prepare.add_argument("--fr", type=Path, required=True)
        prepare.add_argument("--task", choices=("unicode-dice", "regex-escape-len"), required=True)
        prepare.add_argument("--arm", choices=("fr", "files"), required=True)
    step = commands.add_parser("step")
    step.add_argument("session", type=Path)
    transport = step.add_mutually_exclusive_group(required=True)
    transport.add_argument("--request-stdin", action="store_true")
    transport.add_argument("--request")
    for name in ("resume", "score"):
        commands.add_parser(name).add_argument("session", type=Path)
    run = commands.add_parser("run")
    run.add_argument("session", type=Path)
    run.add_argument("--confirm-agent-spend", action="store_true", required=True)
    run.add_argument("--codex", default="codex")
    run.add_argument("--model", default="gpt-5.6-luna")
    run.add_argument("--effort", default="low")
    run.add_argument("--service-tier", default="default")
    run.add_argument("--timeout", type=int, default=1200)
    record = commands.add_parser("record")
    record.add_argument("session", type=Path)
    record.add_argument("destination", type=Path)
    record.add_argument("--diagnostic", action="store_true")
    for name in ("audit", "replay"):
        commands.add_parser(name).add_argument("destination", type=Path)
    args = parser.parse_args()
    os.environ.update(trial.environment())
    if args.command == "prepare":
        value = trial.prepare(args.session.resolve(), args.fr.resolve(), args.task, args.arm)
    elif args.command == "rehearse":
        value = rehearsal.run(args.session.resolve(), args.fr.resolve(), args.task, args.arm)
    elif args.command == "step":
        data = args.request.encode() if args.request is not None else sys.stdin.buffer.read(65537)
        trial.require(len(data) <= 65536, "request exceeds 64 KiB")
        value = trial.step(args.session.resolve(), json.loads(data))
    elif args.command == "resume":
        value = trial.resume(args.session.resolve())
    elif args.command == "run":
        value = live.run(args.session.resolve(), args.codex, args.model, args.effort, args.service_tier, args.timeout)
    elif args.command == "score":
        value = live.score(args.session.resolve())
    elif args.command == "record":
        value = live.record(args.session.resolve(), args.destination.resolve(), args.diagnostic)
    else:
        value = live.audit(args.destination.resolve(), replay=args.command == "replay")
    print(json.dumps(value, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
