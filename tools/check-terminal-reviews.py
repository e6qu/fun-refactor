#!/usr/bin/env python3
"""Exercise frozen terminal reviews with the real client and scripted responses."""
import argparse
import ast
import base64
from contextlib import nullcontext
import gzip
import hashlib
import io
import os
from pathlib import Path
import sys
import subprocess
import textwrap
import threading
import time
from unittest.mock import patch

from agent_eval import bounded_host, native_costs, native_mcp as mcp, source_packets, source_reviews
from agent_eval import structured_probe as probe, terminal_reviews as review, terminal_review_runner as runner
from agent_eval.study import encode, require

CASES = ("packet", "read", "missing", "duplicate", "interrupt", "configured")


def author_control(binary, root):
    """Have fr restore this task's extracted function in a throwaway source copy."""
    root.mkdir(parents=True, exist_ok=False)
    original = Path(__file__).with_name("agent_eval") / "structured_probe.py"
    source = original.read_text()
    node = next(n for n in ast.parse(source).body if isinstance(n, ast.FunctionDef) and n.name == "capture")
    selected = ast.get_source_segment(source, node)
    signature, body = selected.split(":\n", 1)
    fixture = root / original.name
    fixture.write_text(source.replace(selected, signature + ':\n    raise RuntimeError("author control")', 1))
    fragment = root / "body.txt"
    fragment.write_text(textwrap.dedent(body))
    def fr(*args):
        result = subprocess.run([str(binary), "--json", "-C", str(fixture), *args],
            capture_output=True, check=True, timeout=30, env={**os.environ, "RAYON_NUM_THREADS": "1"})
        require(len(result.stdout) <= 1024**2, "author output exceeded budget")
        return mcp.decode(result.stdout)
    found = fr("project", "find", "capture", "--source", "--bytes", "8192")
    require(len(found["rows"]) == 1, "capture declaration differs")
    handle = dict(zip(found["columns"], found["rows"][0]))["handle"]
    manifest = root / "author.json"
    manifest.write_bytes(encode({"operations": [{"op": "replace-body", "handle": handle, "from": str(fragment)}],
        "postconditions": {"files-changed": 1, "paths-changed": [fixture.name]}}))
    preview = fr("author", "batch", "--from", str(manifest), "--diff-bytes", "16384")
    (root / "preview.json").write_bytes(encode(preview))
    saved = fr("author", "batch", "--from", str(manifest), "--save-plan", "--plan-basis", preview["plan_context_basis"])
    fr("history", "apply", str(saved["transaction"]), "--write", "--no-diff", "--context-basis", saved["transaction_context_basis"])
    require(fixture.read_text() == source, "fr did not restore the reviewed function exactly")
    print("fr restored the extracted capture function exactly")


def questions(case):
    raw = {"module.py": probe.SOURCE, "requirement.py": b"EXPECTED_VALUE = 43\n"}
    files = {p: {"data": base64.b64encode(data).decode(), "executable": False} for p, data in raw.items()}
    selected = "module.py" if case == "packet" else "requirement.py"
    return [{"id": "value-review", "question": "Check whether value() returns the required value 43.",
             "files": files, "selections": [{"path": selected, "sha256": hashlib.sha256(raw[selected]).hexdigest(),
                                             "start": 0, "end": len(raw[selected])}]}]


def response(original, case, turn, answer, folder):
    if case == "interrupt" and turn == 2:
        deadline = time.monotonic() + 2
        while True:
            events, _ = native_costs.prefix(review.read(folder / "events.jsonl", optional=True), 1024**2)
            if any(e["type"] == "message.part.updated" and e["properties"]["part"]["type"] == "step-finish" for e in events):
                break
            require(time.monotonic() < deadline, "first response usage did not reach the retained stream")
            time.sleep(0.01)
        os._exit(17)
    if case == "packet":
        require(turn == 1, "unexpected request after terminal answer")
        reply = original(2, "one-answer")
    else:
        reply = original(turn, {"missing": "missing-answer", "duplicate": "duplicate-answer"}.get(case, "one-answer"))
    for call in reply["choices"][0]["message"].get("tool_calls", []):
        if call["function"]["name"] == "StructuredOutput":
            call["function"]["arguments"] = encode(answer).decode()
    return reply


def capture(root, case, binary, opencode):
    original = probe.response
    with probe.provider(root, "one-answer") as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            models = [{"providerID": "scripted", "modelID": "protocol",
                       "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "context": 8192, "output": 1024}]
            client_context = nullcontext()
            if case == "configured":
                env = probe.environment(root / "attempts" / "value-review-0", models[0]["baseURL"])
                fixture = root / "fixture"
                fixture.mkdir()
                config = fixture / "opencode.json"
                config.write_bytes(encode({"provider": mcp.decode(env["OPENCODE_CONFIG_CONTENT"].encode())["provider"]}))
                env["OPENCODE_CONFIG"] = str(config)
                client_context = patch.dict(os.environ, env)
                models = [{k: v for k, v in models[0].items() if k != "baseURL"} | {"configured": True}]
            frozen, snapshots = review.freeze(questions(case), models, binary, opencode,
                {"kind": "scripted control", "case": case, "script_sha256": source_reviews.identity(Path(__file__))})
            (root / "plan.json").write_bytes(encode(frozen))
            (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
            cell = frozen["plan"]["cells"][0]
            folder = root / "attempts" / cell["id"]
            folder.mkdir(parents=True)
            source = {"path": "module.py", "sha256": hashlib.sha256(probe.SOURCE).hexdigest(),
                      "start": 0, "end": len(probe.SOURCE)}
            packet = source_packets.build(snapshots[cell["task"]], [source])
            answer = {"answer": {"findings": [{"gap": "The implementation returns 42", "wrong_repair": "Leave value unchanged",
                "input": "value()", "expected": "43", "citations": [{"source": packet["source_refs"][0]["source"]}]}],
                "limitations": "Scripted source-citation control, not an independent model judgment."}}
            with client_context, patch.object(probe, "response", lambda turn, _: response(original, case, turn, answer, folder)):
                runner.capture(frozen, snapshots, cell, folder, binary, opencode)
            require(not server.errors and len(server.requests) == (1 if case == "packet" else 2), "unexpected provider work")
        finally:
            server.shutdown()


def report(root, case):
    frozen = mcp.decode(review.read(root / "plan.json"))
    require(frozen["plan"]["provenance"]["case"] == case, "control case differs")
    snapshots = source_reviews.read_inputs(root)
    result = review.report(frozen, snapshots, root / "attempts")
    row = result["attempts"][0]
    requests = mcp.decode(review.read(root / "provider.json"))
    require(len(requests) == (1 if case == "packet" else 2), "provider count differs")
    expected = {"rehearsal_" + t["name"] for t in frozen["plan"]["tools"]} | {"StructuredOutput"}
    for request in requests:
        require(request["model"] == "protocol" and request["tool_choice"] == "required", "provider request differs")
        names = [t["function"]["name"] for t in request["tools"]]
        require(set(names) == expected and len(names) == len(expected), "frozen tools differ at provider")
    if case != "packet":
        folder = root / "attempts" / row["cell"]["id"]
        rows = [mcp.decode(line) for line in review.read(folder / "tools.jsonl").splitlines()]
        delivered = [m for m in requests[1]["messages"] if m["role"] == "tool"]
        require(len(rows) == len(delivered) == 1 and delivered[0]["content"]
                == rows[0]["response"]["content"][0]["text"], "source did not reach the provider")
    if case in {"packet", "read", "configured"}:
        require(row["status"] == "completed", f"valid review failed: {row['failure']}")
        require(row["audit"]["assistant_responses"] == len(requests), "response accounting differs")
        require(row["audit"]["fr_calls"] == 0 and row["audit"]["review"]["findings"][0]["citations"]
                == [{"path": "module.py", "quote": probe.SOURCE.decode()}], "citation replay differs")
    else:
        expected_failure = {"missing": "failed assistant response", "duplicate": "expected exactly one structured submission",
                            "interrupt": "capture process did not complete"}[case]
        require(row["status"] == "failed" and row["failure"] == expected_failure, "unexpected failed-control outcome")
        require(row["observed"]["usage"]["finished_steps"] >= 1 and row["observed"]["host_calls"] == 1,
                "failed work was lost")
        if case == "interrupt":
            require(row["process"]["exit_code"] == row["process"]["process_exit_code"] == 17
                    and row["process"]["stop_reason"] is None, "capture did not stop at the planned interruption")
            require(not (root / "attempts" / row["cell"]["id"] / "export.json").exists(), "interruption unexpectedly exported")
    return {"case": case, "provider_requests": len(requests), "report": result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture", "report", "author"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--case", choices=CASES)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    if args.command == "author":
        require(args.fr is not None, "supply --fr")
        author_control(args.fr.resolve(), root)
        return
    if args.command == "report":
        print(encode([report(root / case, case) for case in ((args.case,) if args.case else CASES)]).decode())
        return
    require(args.fr is not None and args.opencode is not None, "supply both pinned binaries")
    if args.command == "capture":
        capture(root, args.case, args.fr.resolve(), args.opencode.resolve())
        return
    root.mkdir(parents=True, exist_ok=False)
    results = []
    for case in (args.case,) if args.case else CASES:
        folder = root / case
        folder.mkdir()
        out, err = io.BytesIO(), io.BytesIO()
        process = bounded_host.run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", str(folder),
            "--case", case, "--fr", str(args.fr.resolve()), "--opencode", str(args.opencode.resolve())],
            b"", out, err, folder, wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2,
            disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
        (folder / "host.stdout").write_bytes(out.getvalue())
        (folder / "host.stderr").write_bytes(err.getvalue())
        (folder / "process.json").write_bytes(encode(process))
        frozen = mcp.decode(review.read(folder / "plan.json"))
        snapshots = source_reviews.read_inputs(folder)
        cell = frozen["plan"]["cells"][0]
        attempt = folder / "attempts" / cell["id"]
        runner.cleanup(attempt)
        runner.seal(frozen, snapshots, cell, attempt, process)
        results.append(report(folder, case))
        (root / "result.json").write_bytes(encode(results))
        print(case + ": " + results[-1]["report"]["attempts"][0]["status"], flush=True)


if __name__ == "__main__":
    main()
