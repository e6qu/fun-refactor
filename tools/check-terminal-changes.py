#!/usr/bin/env python3
"""Exercise configured edits with a real client and scripted loopback replies."""
import argparse
import base64
import copy
import gzip
import io
import json
import os
from pathlib import Path
import sys
import threading
from unittest.mock import patch

from agent_eval import bounded_host, source_reviews, structured_probe as probe
from agent_eval import terminal_changes as changes, terminal_change_runner as runner
from agent_eval.study import encode, require
from agent_eval.test_terminal_changes import task

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "tests/agent-eval/opencode/catalog/2026-10-07/catalog.json"
CASES = [{"provider": p, "model": m, "arm": arm, "missing": False}
         for p, m in (("kimi-code-plan-global", "k3"), ("zai-coding-plan", "glm-5.3-flash"))
         for arm in changes.ARMS]
CASES.append({**CASES[-2], "missing": True})


def response(original, turn, case, attempt):
    rows = [json.loads(line) for line in (attempt / "tools.jsonl").read_bytes().splitlines()] if (attempt / "tools.jsonl").exists() else []
    if turn == 1:
        call = ("read_source", {"path": "module.py", "offset": 0, "bytes": 128, "sha256": ""})
    elif case["arm"] == "files" and turn == 2:
        call = ("replace_source", {"path": "module.py", "sha256": rows[0]["result"]["sha256"],
                                  "old": "return 42", "new": "return 43"})
    elif case["arm"] == "fr" and turn == 2:
        call = ("fr_explore", {"term": "value", "path": "module.py"})
    elif case["arm"] == "fr" and turn in (3, 4):
        result = rows[1]["result"]
        handle = result["rows"][0]["handle"]
        call = (("fr_explore", {"term": "value", "path": "module.py", "mode": "behavior", "target": handle})
                if turn == 3 else ("fr_preview_body", {"path": "module.py", "handle": handle, "body": "return 43\n"}))
    elif case["arm"] == "fr" and turn == 5:
        call = ("fr_apply_preview", {"preview_id": rows[-1]["result"]["preview_id"]})
    else:
        require(turn == (3 if case["arm"] == "files" else 6), "unexpected extra model response")
        call = ("StructuredOutput", {"answer": {"summary": "Changed value; tests not run."}})
    result = original(1, "one-answer")
    result["id"] = f"change_{turn}"
    message = result["choices"][0]["message"]
    if call[0] == "StructuredOutput" and case["missing"]:
        message.pop("tool_calls")
        message["content"] = "The edit is ready."
        result["choices"][0]["finish_reason"] = "stop"
    else:
        message["tool_calls"] = [{"id": f"call_{turn}", "type": "function", "function": {
            "name": call[0] if call[0] == "StructuredOutput" else "rehearsal_" + call[0],
            "arguments": encode(call[1]).decode()}}]
    return result


def capture(root, case, binary, client):
    original = probe.response
    with probe.provider(root, "one-answer", model=case["model"]) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            endpoint = f"http://127.0.0.1:{server.server_port}/v1"
            catalog = json.loads(CATALOG.read_bytes())
            catalog[case["provider"]]["api"] = endpoint
            attempt = root / "attempt"
            attempt.mkdir()
            env = probe.environment(attempt, endpoint)
            fixture = root / "fixture.json"
            fixture.write_bytes(encode({"provider": {case["provider"]: {"options": {"apiKey": "scripted-no-credential"}}}}))
            env["OPENCODE_CONFIG"] = str(fixture)
            model = {"providerID": case["provider"], "modelID": case["model"], "configured": True,
                     "context": 32768, "output": 2048, "variant": "low"}
            frozen, snapshots = changes.freeze([task()], [model], binary, client, catalog,
                {"kind": "scripted change control", "case": case, "script_sha256": source_reviews.identity(Path(__file__))})
            (root / "plan.json").write_bytes(encode(frozen))
            (root / "inputs.json.gz").write_bytes(gzip.compress(encode(snapshots), mtime=0))
            cell = next(c for c in frozen["plan"]["cells"] if c["arm"] == case["arm"])
            with patch.dict(os.environ, env), patch.object(probe, "response", lambda turn, _: response(original, turn, case, attempt)):
                runner.capture(frozen, snapshots, cell, attempt, binary, client)
            require(not server.errors, "loopback provider rejected a request")
        finally:
            server.shutdown()
            worker.join(timeout=2)


def report(root):
    frozen = json.loads((root / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(root)
    plan = changes.checked(frozen, snapshots)
    case = plan["provenance"]["case"]
    require(case in CASES, "unknown control")
    cell = next(c for c in plan["cells"] if c["arm"] == case["arm"])
    attempt = root / "attempt"
    process = json.loads((attempt / "process.json").read_bytes())
    probe.checked_process(process)
    record = json.loads((attempt / "record.json").read_bytes())
    require(record["cell"] == cell and record["plan_sha256"] == frozen["sha256"], "record identity differs")
    manifest = json.loads((attempt / "manifest.json").read_bytes())
    require(set(manifest) == {p.name for p in attempt.iterdir()} - {"manifest.json"}, "control inventory differs")
    require(all(source_reviews.identity(attempt / n) == sha for n, sha in manifest.items()), "control evidence changed")
    requests = json.loads((root / "provider.json").read_bytes())
    expected = 3 if case["arm"] == "files" else 6
    require(len(requests) == expected, "provider call count differs")
    names = {"rehearsal_" + t["name"] for t in changes.schemas(case["arm"])} | {"StructuredOutput"}
    for request in requests:
        require(request["model"] == case["model"] and request.get("reasoning_effort") == "low"
                and request.get("max_tokens") == 2048 and request["tool_choice"] == "required", "provider settings differ")
        require({t["function"]["name"] for t in request["tools"]} == names, "provider tools differ")
    if case["missing"]:
        require(record["status"] == "failed" and record["failure"] == "failed assistant response"
                and not (attempt / "submission.json").exists(), "missing terminal answer became a submission")
    else:
        audited = changes.audit(plan, plan["tasks"][0] | {"files": snapshots[cell["task"]]}, cell, attempt)
        files = audited.pop("files")
        require(record["status"] == "completed" and record["audit"] == audited, "completed control differs")
        require(files == json.loads((attempt / "submission.json").read_bytes()), "retained submission differs")
        require(base64.b64decode(files["module.py"]["data"]) == b"def value():\n    return 43\n", "exact edit differs")
        require(audited["accepted_edits"] == 1 and audited["metrics"]["fr_calls"] == (4 if case["arm"] == "fr" else 0),
                "edit accounting differs")
    return {"case": case, "record": record, "process": process, "observed": changes.observed(attempt),
            "headroom_admitted": process["sampled_aggregate_rss_bytes"] <= 640 * 1024**2}


def profiled_run(command, out, err, folder):
    from agent_eval import client_memory_profile
    sampler = client_memory_profile.Sampler(folder)
    sampler.attempt = folder / "attempt"
    with patch.object(bounded_host, "sample", sampler):
        process = bounded_host.run(command, b"", out, err, folder,
            wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2,
            disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
    profile = client_memory_profile.audit(folder, process)
    (folder / "profile.json").write_bytes(encode(profile))
    return process


def single(folder, index, binary, client):
    folder.mkdir(parents=True, exist_ok=False)
    case = CASES[index]
    out, err = io.BytesIO(), io.BytesIO()
    process = profiled_run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", str(folder),
        "--case", str(index), "--fr", str(binary), "--opencode", str(client)], out, err, folder)
    (folder / "host.stdout").write_bytes(out.getvalue())
    (folder / "host.stderr").write_bytes(err.getvalue())
    (folder / "process.json").write_bytes(encode(process))
    frozen = json.loads((folder / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(folder)
    cell = next(c for c in frozen["plan"]["cells"] if c["arm"] == case["arm"])
    runner.seal(frozen, snapshots, cell, folder / "attempt", process)
    return report(folder)


def check(root, binary, client):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run change transport controls on GitHub")
    root.mkdir(parents=True, exist_ok=False)
    results = []
    for index, case in enumerate(CASES):
        results.append(single(root / str(index), index, binary, client))
        (root / "result.json").write_bytes(encode(results))
        require(results[-1]["headroom_admitted"], "change adapter lacks admitted memory headroom")
        print(f"{case['provider']}/{case['arm']}/missing={case['missing']}: verified", flush=True)


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
    elif args.command == "capture":
        capture(args.output.resolve(), CASES[args.case], args.fr.resolve(), args.opencode.resolve())
    else:
        check(args.output.resolve(), args.fr.resolve(), args.opencode.resolve())
