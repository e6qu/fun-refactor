#!/usr/bin/env python3
"""Exercise serial review collection on GitHub using only a scripted provider."""
import argparse
import base64
import gzip
import hashlib
from pathlib import Path
import threading
from unittest.mock import patch

from agent_eval import native_mcp as mcp, source_reviews, structured_probe as probe
from agent_eval import terminal_review_collection as collection, terminal_reviews as review
from agent_eval.study import encode, require

CASES = ("two-providers", "stop-after-two")


def check(root, case, binary, opencode):
    root.mkdir(parents=True, exist_ok=False)
    original = probe.response
    with probe.provider(root, "one-answer") as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            models = [{"providerID": name, "modelID": "protocol",
                       "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "context": 8192, "output": 1024}
                      for name in (("alpha", "beta") if case == "two-providers" else ("alpha", "beta", "gamma"))]
            questions = [{"id": "value-review", "question": "Check whether value returns 43.",
                "files": {"module.py": {"data": base64.b64encode(probe.SOURCE).decode(), "executable": False}},
                "selections": [{"path": "module.py", "sha256": hashlib.sha256(probe.SOURCE).hexdigest(),
                                "start": 0, "end": len(probe.SOURCE)}]}]
            frozen, snapshots = review.freeze(questions, models, binary, opencode,
                {"kind": "scripted collection control", "case": case,
                 "script_sha256": source_reviews.identity(Path(__file__))})
            (root / "plan.json").write_bytes(encode(frozen))
            (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
            answer = {"answer": {"findings": [{"gap": "Returns 42", "wrong_repair": "Leave value unchanged",
                "input": "value()", "expected": "43", "citations": [{"path": "module.py", "quote": probe.SOURCE.decode()}]}],
                "limitations": "Scripted collection control, not a model judgment."}}
            def response(turn, _):
                require(turn <= 4, "collection continued after its terminal state")
                reply = original((turn - 1) % 2 + 1, "one-answer" if case == "two-providers" else "missing-answer")
                for call in reply["choices"][0]["message"].get("tool_calls", []):
                    if call["function"]["name"] == "StructuredOutput":
                        call["function"]["arguments"] = encode(answer).decode()
                return reply
            with patch.object(probe, "response", response):
                result = collection.collect_all(frozen, snapshots, root / "attempts", binary, opencode, root, frozen["sha256"])
                before = len(server.requests)
                repeated = collection.collect_all(frozen, snapshots, root / "attempts", binary, opencode, root, frozen["sha256"])
            require(result == repeated and len(server.requests) == before == 4 and not server.errors,
                    "collection retried or provider work differs")
            (root / "result.json").write_bytes(encode(result))
        finally:
            server.shutdown()
    return report(root, case)


def report(root, case):
    frozen = mcp.decode(review.read(root / "plan.json"))
    require(frozen["plan"]["provenance"]["case"] == case, "collection control identity differs")
    snapshots = source_reviews.read_inputs(root)
    result = review.report(frozen, snapshots, root / "attempts")
    require(mcp.decode(review.read(root / "result.json")) == {"status": collection.state(result), "report": result},
            "saved collection report differs")
    requests = mcp.decode(review.read(root / "provider.json"))
    require(len(requests) == 4, "collection provider count differs")
    names = {"rehearsal_" + tool["name"] for tool in frozen["plan"]["tools"]} | {"StructuredOutput"}
    for request in requests:
        require(request["model"] == "protocol" and request["tool_choice"] == "required"
                and {tool["function"]["name"] for tool in request["tools"]} == names, "provider request differs")
    counts = (result["completed"], result["failed"], result["not_started"])
    require(counts == ((2, 0, 0) if case == "two-providers" else (0, 2, 1)), "collection outcome differs")
    for i, row in enumerate(result["attempts"][:2]):
        require(frozen["plan"]["models"][row["cell"]["model"]]["providerID"] == ("alpha", "beta")[i],
                "provider order differs")
        folder = root / "attempts" / row["cell"]["id"]
        tools = [mcp.decode(line) for line in review.read(folder / "tools.jsonl").splitlines()]
        delivered = [m for m in requests[i * 2 + 1]["messages"] if m["role"] == "tool"]
        require(len(tools) == len(delivered) == 1 and delivered[0]["content"]
                == tools[0]["response"]["content"][0]["text"], "source delivery differs")
        require(row["observed"]["usage"]["finished_steps"] == 2 and row["observed"]["host_calls"] == 1,
                "observed collection work was lost")
        if case == "stop-after-two":
            require(row["failure"] == "failed assistant response", "unexpected failure cause")
    return {"case": case, "status": collection.state(result), "completed": result["completed"],
            "failed": result["failed"], "not_started": result["not_started"], "provider_requests": len(requests),
            "live_compatibility_verified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "report"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    if args.command == "check":
        require(args.fr is not None and args.opencode is not None, "supply the pinned binaries")
        results = [check(root / case, case, args.fr.resolve(), args.opencode.resolve()) for case in CASES]
    else:
        results = [report(root / case, case) for case in CASES]
    print(encode(results).decode())


if __name__ == "__main__":
    main()
