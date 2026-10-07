#!/usr/bin/env python3
"""Verify frozen variants at a loopback provider using the actual OpenCode client."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import sys
import threading
from unittest.mock import patch

from agent_eval import bounded_host, source_packets, source_reviews, structured_probe as probe
from agent_eval import terminal_reviews as review, terminal_review_runner as runner
from agent_eval.study import encode, require

CATALOG = Path(__file__).resolve().parents[1] / "tests/agent-eval/opencode/catalog/2026-10-07/catalog.json"
CASES = [{"provider": provider, "model": model, "variant": variant}
         for provider, model in (("kimi-code-plan-global", "k3"), ("zai-coding-plan", "glm-5.3-flash"))
         for variant in ("low", "max")]


def capture(root, case, binary, client):
    control = runpy.run_path(str(Path(__file__).with_name("check-terminal-reviews.py")))
    original = probe.response
    with probe.provider(root, "one-answer", model=case["model"]) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            endpoint = f"http://127.0.0.1:{server.server_port}/v1"
            catalog = json.loads(CATALOG.read_bytes())
            catalog[case["provider"]]["api"] = endpoint
            (root / "catalog.json").write_bytes(encode(catalog))
            attempt = root / "attempts/value-review-0"
            env = probe.environment(attempt, endpoint)
            settings = json.loads(env["OPENCODE_CONFIG_CONTENT"])
            settings["provider"] = {case["provider"]: {"options": {"apiKey": "scripted-no-credential"}}}
            fixture = root / "fixture.json"
            fixture.write_bytes(encode({"provider": settings["provider"]}))
            env.update(OPENCODE_CONFIG=str(fixture), OPENCODE_MODELS_PATH=str(root / "catalog.json"), BUN_OPTIONS="")
            model = {"providerID": case["provider"], "modelID": case["model"], "configured": True,
                     "context": 32768, "output": 2048, "variant": case["variant"]}
            frozen, snapshots = review.freeze(control["questions"]("configured"), [model], binary, client,
                {"kind": "scripted reasoning control", "case": case,
                 "catalog_sha256": source_reviews.identity(CATALOG), "script_sha256": source_reviews.identity(Path(__file__))})
            (root / "plan.json").write_bytes(encode(frozen))
            (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
            attempt.mkdir(parents=True)
            cell = frozen["plan"]["cells"][0]
            selected = {"path": "module.py", "sha256": hashlib.sha256(probe.SOURCE).hexdigest(), "start": 0, "end": len(probe.SOURCE)}
            packet = source_packets.build(snapshots[cell["task"]], [selected])
            answer = {"answer": {"findings": [{"gap": "The implementation returns 42", "wrong_repair": "Keep 42",
                "input": "value()", "expected": "43", "citations": [{"source": packet["source_refs"][0]["source"]}]}],
                "limitations": "Scripted transport control; not an independent model judgment."}}
            with patch.dict(os.environ, env), patch.object(probe, "response",
                    lambda turn, _: control["response"](original, "configured", turn, answer, attempt)):
                runner.capture(frozen, snapshots, cell, attempt, binary, client)
            require(not server.errors and len(server.requests) == 2, "unexpected loopback provider work")
        finally:
            server.shutdown()
            worker.join(timeout=2)


def report(root):
    frozen = json.loads((root / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(root)
    result = review.report(frozen, snapshots, root / "attempts")
    attempt = result["attempts"][0]
    require(attempt["status"] == "completed", "reasoning control did not complete")
    case = frozen["plan"]["provenance"]["case"]
    require(case in CASES, "unknown reasoning control")
    model = frozen["plan"]["models"][0]
    require(model["variant"] == case["variant"] and model["context"] == 32768 and model["output"] == 2048,
            "frozen reasoning settings differ")
    requests = json.loads((root / "provider.json").read_bytes())
    require(len(requests) == 2, "provider request count differs")
    for request in requests:
        require(request["model"] == case["model"] and request.get("reasoning_effort") == case["variant"],
                "reasoning effort did not reach the provider")
        require(request.get("max_tokens") == 2048 and request["tool_choice"] == "required", "provider output budget or tools differ")
    folder = root / "attempts" / attempt["cell"]["id"]
    rows = [json.loads(line) for line in (folder / "tools.jsonl").read_bytes().splitlines()]
    delivered = [m for m in requests[1]["messages"] if m["role"] == "tool"]
    require(len(rows) == len(delivered) == 1 and delivered[0]["content"] == rows[0]["response"]["content"][0]["text"],
            "source did not reach the provider")
    require(attempt["audit"]["fr_calls"] == 0 and attempt["audit"]["assistant_responses"] == 2, "control accounting differs")
    return {"case": case, "report": result, "provider_requests": len(requests),
            "provider_sha256": source_reviews.identity(root / "provider.json")}


def check(root, binary, client):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run reasoning transport controls on GitHub")
    root.mkdir(parents=True, exist_ok=False)
    results = []
    for index, case in enumerate(CASES):
        folder = root / str(index)
        folder.mkdir()
        out, err = io.BytesIO(), io.BytesIO()
        process = bounded_host.run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", str(folder),
            "--case", str(index), "--fr", str(binary), "--opencode", str(client)], b"", out, err, folder,
            wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2, disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
        (folder / "host.stdout").write_bytes(out.getvalue())
        (folder / "host.stderr").write_bytes(err.getvalue())
        (folder / "process.json").write_bytes(encode(process))
        frozen = json.loads((folder / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(folder)
        cell = frozen["plan"]["cells"][0]
        attempt = folder / "attempts" / cell["id"]
        runner.cleanup(attempt)
        runner.seal(frozen, snapshots, cell, attempt, process)
        results.append(report(folder))
        (root / "result.json").write_bytes(encode(results))
        print(f"{case['provider']}/{case['variant']}: verified", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture", "report"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--case", type=int, choices=range(len(CASES)))
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    if args.command == "report":
        print(encode([report(args.output / str(i)) for i in range(len(CASES))]).decode())
    else:
        require(args.fr is not None and args.opencode is not None, "supply pinned binaries")
        if args.command == "capture":
            require(args.case is not None, "select a frozen case")
            capture(args.output.resolve(), CASES[args.case], args.fr.resolve(), args.opencode.resolve())
        else:
            check(args.output.resolve(), args.fr.resolve(), args.opencode.resolve())
