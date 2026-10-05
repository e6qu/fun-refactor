#!/usr/bin/env python3
"""Check terminal submissions with scripted OpenCode responses; never call a live model."""
import argparse
import io
from pathlib import Path
import sys

from agent_eval import structured_probe as probe, structured_submission as protocol
from agent_eval import native_mcp as mcp
from agent_eval.bounded_host import run
from agent_eval.study import encode, load, require


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture", "serve"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--opencode", type=Path)
    parser.add_argument("--case", choices=probe.CASES)
    args = parser.parse_args()
    if args.command == "serve":
        probe.serve(args.output)
        return
    require(args.opencode is not None, "supply --opencode")
    if args.command == "capture":
        probe.capture(args.opencode.resolve(), args.output.resolve(), args.case)
        return
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for case in (args.case,) if args.case else probe.CASES:
        folder = args.output.resolve() / case
        folder.mkdir()
        out, err = io.BytesIO(), io.BytesIO()
        process = run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", str(folder),
                       "--opencode", str(args.opencode.resolve()), "--case", case], b"", out, err, folder,
                      wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2,
                      disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
        (folder / "host.stdout").write_bytes(out.getvalue())
        (folder / "host.stderr").write_bytes(err.getvalue())
        (folder / "process.json").write_bytes(encode(process))
        require(process["exit_code"] == 0 and process["stop_reason"] is None,
                f"{case} capture failed: {process['stop_reason'] or process['exit_code']}; see {folder}")
        verdict, refusal = None, None
        try:
            verdict = protocol.audit(load(folder / "request.json"), load(folder / "terminal.json"),
                load(folder / "messages.json"), [mcp.decode(line) for line in (folder / "events.jsonl").read_bytes().splitlines()],
                [mcp.decode(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()])
        except ValueError as error:
            refusal = str(error)
        require((verdict is not None) == (case == "one-answer"), f"unexpected {case} verdict: {refusal}")
        if verdict:
            require(verdict["answer"] == probe.ANSWER["answer"] and verdict["assistant_responses"] == 2,
                    "scripted answer or model call count differs")
        results.append({"case": case, "accepted": verdict is not None, "refusal": refusal, "audit": verdict})
        (args.output / "result.json").write_bytes(encode({"schema": protocol.SCHEMA, "cases": results}))
        print(case + ": " + ("accepted" if verdict else refusal), flush=True)


if __name__ == "__main__":
    main()
