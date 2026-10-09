#!/usr/bin/env python3
"""Measure scripted OpenCode streaming CPU on hosted runners; never call a model."""
import argparse
import base64
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch

from agent_eval import client_memory_profile, scripted_stream, source_reviews
from agent_eval.study import digest, encode, require

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("change_controls", ROOT / "tools/check-terminal-changes.py")
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
SOURCE = '''"""Import and total invoice records without changing their original order."""
from decimal import Decimal


def parse_amount(text):
    """Accept decimal amounts and reject non-finite values."""
    amount = Decimal(text.strip())
    if not amount.is_finite():
        raise ValueError("amount must be finite")
    return amount


def parse_record(line):
    """Read reference, amount and an optional void flag from a tab-separated row."""
    fields = line.rstrip("\\n").split("\\t")
    if len(fields) not in (2, 3):
        raise ValueError("expected two or three fields")
    reference, amount = fields[:2]
    if not reference.strip():
        raise ValueError("missing reference")
    flag = fields[2].strip().lower() if len(fields) == 3 else ""
    if flag not in ("", "void"):
        raise ValueError("unknown record flag")
    return {"reference": reference.strip(), "amount": parse_amount(amount),
            "voided": flag == "void"}


def total(records):
    """Return the amount due for the supplied records."""
    return sum(record["amount"] for record in records)


def load_records(lines):
    """Ignore blank lines and retain duplicate references."""
    return [parse_record(line) for line in lines if line.strip()]


def format_report(records):
    """Render records before the total; consume a one-shot input only once."""
    rows = list(records)
    labels = [row["reference"] + (" (void)" if row["voided"] else "") for row in rows]
    return "\\n".join(labels + [str(total(rows))])
'''
OLD = 'return sum(record["amount"] for record in records)'
NEW = 'return sum(record["amount"] for record in records if not record.get("voided", False))'
TEXT = ("Inspect the amount calculation and preserve record order, duplicate references, and one-shot "
        "iteration. Change only the treatment of voided records. The source is available, but no "
        "behavior tests have run yet. Keep that limitation in the final summary.\n" * 12)
ORIGINAL_TASK, ORIGINAL_RESPONSE, ORIGINAL_PROVIDER = control.task, control.probe.response, control.probe.provider
ORDER = ("whole", "chunked", "chunked", "whole")


def task():
    value = ORIGINAL_TASK()
    value.update(id="invoice", requirement="Exclude voided records from total; preserve other behavior.",
                 public_feedback="A voided amount is incorrectly included in the total.",
                 files={"module.py": {"data": base64.b64encode(SOURCE.encode()).decode(), "executable": False}})
    # This scripted transport fixture is checked by exact source identity, never a model-quality grader.
    return value


def reply(turn, case, attempt):
    rows = [json.loads(line) for line in (attempt / "tools.jsonl").read_bytes().splitlines()] if (attempt / "tools.jsonl").exists() else []
    if turn == 1:
        call = ("read_source", {"path": "module.py", "offset": 0, "bytes": 4096, "sha256": ""})
    elif case["arm"] == "files" and turn == 2:
        call = ("replace_source", {"path": "module.py", "sha256": rows[0]["result"]["sha256"], "old": OLD, "new": NEW})
    elif case["arm"] == "fr" and turn == 2:
        call = ("fr_explore", {"term": "total", "path": "module.py"})
    elif case["arm"] == "fr" and turn in (3, 4):
        handle = rows[1]["result"]["rows"][0]["handle"]
        call = (("fr_explore", {"term": "total", "path": "module.py", "mode": "behavior", "target": handle})
                if turn == 3 else ("fr_preview_body", {"path": "module.py", "handle": handle,
                    "body": '"""Return the amount due for the supplied records."""\n' + NEW + "\n"}))
    elif case["arm"] == "fr" and turn == 5:
        call = ("fr_apply_preview", {"preview_id": rows[-1]["result"]["preview_id"]})
    else:
        require(turn == (3 if case["arm"] == "files" else 6), "extra scripted response")
        call = ("StructuredOutput", {"answer": {"summary": "Excluded voided records; tests not run."}})
    value = ORIGINAL_RESPONSE(1, "one-answer")
    value["id"] = f"stream_{turn}"
    value["choices"][0]["message"] = {"role": "assistant", "content": TEXT,
        "tool_calls": [{"id": f"call_{turn}", "type": "function", "function": {
            "name": call[0] if call[0] == "StructuredOutput" else "rehearsal_" + call[0],
            "arguments": encode(call[1]).decode()}}]}
    return value


def reply_identity(value):
    normalized = copy.deepcopy(value)
    for call in normalized["choices"][0]["message"].get("tool_calls", []):
        args = json.loads(call["function"]["arguments"])
        # Handles identify the disposable workspace; source, edit and text bytes stay exact.
        for key in ("handle", "target", "preview_id"):
            if key in args:
                args[key] = "<workspace-reference>"
        call["function"]["arguments"] = encode(args).decode()
    return digest(normalized)


def capture(folder, case, mode, binary, client):
    case = {**case, "streaming": binding(mode)}
    def provider(root, name, **kwargs):
        server = ORIGINAL_PROVIDER(root, name, **kwargs)
        replies = []
        def writer(handler, value):
            replies.append(value)
            (folder / "replies.json").write_bytes(encode(replies))
            scripted_stream.write(handler, value, mode)
        server.stream_writer = writer
        return server
    with patch.object(control, "task", task), patch.object(control, "response", lambda original, turn, c, attempt: reply(turn, c, attempt)), patch.object(control.probe, "provider", provider):
        control.capture(folder, case, binary, client)


def binding(mode):
    return {"mode": mode, "script_sha256": source_reviews.identity(Path(__file__)),
            "codec_sha256": source_reviews.identity(Path(scripted_stream.__file__))}


def profile(folder, process):
    audited = client_memory_profile.audit(folder, process)
    samples = [json.loads(line) for line in (folder / "profile.jsonl").read_bytes().splitlines()]
    pids = {}
    for sample in samples:
        for row in sample["processes"]:
            entry = pids.setdefault(row["pid"], {"executables": set(), "cpu_seconds": 0})
            entry["executables"].add(row["executable"])
            entry["cpu_seconds"] = max(entry["cpu_seconds"], row["cpu_seconds"])
    totals = {}
    for entry in pids.values():
        name = "+".join(sorted(entry["executables"]))
        totals[name] = totals.get(name, 0) + entry["cpu_seconds"]
    require(abs(sum(totals.values()) - process["sampled_cpu_seconds"]) < 1e-6, "CPU attribution differs")
    return {"audit": audited, "cpu_seconds_by_executable": totals}


def report(folder):
    config = json.loads((folder / "case.json").read_bytes())
    process = json.loads((folder / "process.json").read_bytes())
    result = {**config, "status": "failed", "process": process, "profile": profile(folder, process)}
    attempt = folder / "attempt"
    result["record"] = json.loads((attempt / "record.json").read_bytes()) if (attempt / "record.json").exists() else None
    replies = json.loads((folder / "replies.json").read_bytes()) if (folder / "replies.json").exists() else []
    result["reply_identities"] = [reply_identity(value) for value in replies]
    result["wire_frames"] = sum(len(scripted_stream.frames(value, config["mode"])) for value in replies)
    events = (attempt / "events.jsonl").read_bytes().splitlines() if (attempt / "events.jsonl").exists() else []
    result["text_delta_events"] = 0
    for line in events:
        try:
            event = json.loads(line)
        except ValueError:
            continue  # A stopped capture may retain an incomplete final line.
        result["text_delta_events"] += event.get("type") == "message.part.delta"
    if result["record"] and result["record"]["status"] == "completed":
        control.probe.checked_process(process)
        frozen = json.loads((folder / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(folder)
        plan = control.changes.checked(frozen, snapshots)
        require(plan["provenance"]["case"] == {**config["case"], "streaming": binding(config["mode"])}, "diagnostic binding differs")
        cell = next(c for c in plan["cells"] if c["arm"] == config["case"]["arm"])
        require(result["record"]["cell"] == cell and result["record"]["plan_sha256"] == frozen["sha256"], "record identity differs")
        manifest = json.loads((attempt / "manifest.json").read_bytes())
        require(set(manifest) == {p.name for p in attempt.iterdir()} - {"manifest.json"}, "capture inventory differs")
        require(all(source_reviews.identity(attempt / name) == sha for name, sha in manifest.items()), "capture changed")
        audited = control.changes.audit(plan, plan["tasks"][0] | {"files": snapshots[cell["task"]]}, cell, attempt)
        files = audited.pop("files")
        require(audited == result["record"]["audit"], "replayed edit accounting differs")
        require(files == json.loads((attempt / "submission.json").read_bytes()), "submission differs")
        require(base64.b64decode(files["module.py"]["data"]) == SOURCE.replace(OLD, NEW).encode(), "exact repair differs")
        require(len(replies) == (3 if cell["arm"] == "files" else 6), "response count differs")
        requests = json.loads((folder / "provider.json").read_bytes())
        require(len(requests) == len(replies) and all(r.get("stream") for r in requests), "streaming was not requested")
        require(all(r.get("reasoning_effort") == "low" and r.get("max_tokens") == 2048 for r in requests), "provider settings differ")
        require(all(r["choices"][0]["message"]["content"] == TEXT for r in replies), "scripted text differs")
        result.update(status="completed", submission_sha256=digest(files),
                      audit_sha256=digest(audited), runtime=plan["runtime"],
                      binary_sha256=plan["binary_sha256"], opencode_sha256=plan["opencode_sha256"])
    result["admitted"] = result["status"] == "completed" and process["sampled_aggregate_rss_bytes"] <= 640 * 1024**2
    return result


def single(folder, case, mode, binary, client):
    folder.mkdir(parents=True, exist_ok=False)
    (folder / "case.json").write_bytes(encode({"case": control.CASES[case], "mode": mode}))
    out, err = io.BytesIO(), io.BytesIO()
    process = control.profiled_run([sys.executable, "-B", str(Path(__file__).resolve()), "capture", str(folder),
        "--case", str(case), "--mode", mode, "--fr", str(binary), "--opencode", str(client)], out, err, folder)
    (folder / "process.json").write_bytes(encode(process))
    (folder / "host.stdout").write_bytes(out.getvalue())
    (folder / "host.stderr").write_bytes(err.getvalue())
    if (folder / "plan.json").exists():
        frozen = json.loads((folder / "plan.json").read_bytes())
        snapshots = source_reviews.read_inputs(folder)
        cell = next(c for c in frozen["plan"]["cells"] if c["arm"] == control.CASES[case]["arm"])
        control.runner.seal(frozen, snapshots, cell, folder / "attempt", process)
    value = report(folder)
    (folder / "result.json").write_bytes(encode(value))
    return value


def compare(results):
    completed = [r for r in results if r["status"] == "completed"]
    if completed:
        for key in ("submission_sha256", "reply_identities", "runtime", "binary_sha256", "opencode_sha256"):
            require(all(r[key] == completed[0][key] for r in completed), f"paired {key} differs")
    paired = len(completed) == len(ORDER)
    delta = None
    if paired:
        means = {mode: sum(r["process"]["sampled_cpu_seconds"] for r in completed if r["mode"] == mode) / 2
                 for mode in scripted_stream.MODES}
        delta = means["chunked"] - means["whole"]
        require(min(r["text_delta_events"] for r in completed if r["mode"] == "chunked") >
                max(r["text_delta_events"] for r in completed if r["mode"] == "whole"), "client did not observe more deltas")
    return {"schema": "fr-streaming-cpu-1", "attempts": results, "complete_comparison": paired,
            "chunked_minus_whole_mean_cpu_seconds": delta,
            "admitted": paired and all(r["admitted"] for r in completed),
            "scope": "Scripted client transport, not agent efficiency or the live pilot's CPU cause. "
                     "Two runs per mode; pacing adds wall time. CPU and RSS are sampled process-group totals."}


def check(root, case, binary, client):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run streaming controls on GitHub")
    root.mkdir(parents=True, exist_ok=False)
    results, stopped = [], False
    order = ORDER
    # Alternate the leading mode between tool arms while reversing each second pair.
    if case % 2:
        order = tuple("chunked" if mode == "whole" else "whole" for mode in ORDER)
    for index, mode in enumerate(order):
        if stopped:
            results.append({"status": "not_started", "mode": mode, "reason": "earlier capture failed"})
        else:
            value = single(root / str(index), case, mode, binary, client)
            results.append(value)
            stopped = value["status"] != "completed" or not value["admitted"]
        (root / "result.json").write_bytes(encode(compare(results)))
        print(f"{index}/{mode}: {results[-1]['status']}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "capture", "report"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--case", type=int, choices=range(4), default=0)
    parser.add_argument("--mode", choices=scripted_stream.MODES)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    if args.command == "capture":
        require(os.environ.get("GITHUB_ACTIONS") == "true", "run streaming captures on GitHub")
        capture(args.output.resolve(), control.CASES[args.case], args.mode, args.fr.resolve(), args.opencode.resolve())
    elif args.command == "report":
        retained = json.loads((args.output / "result.json").read_bytes())
        actual = [report(args.output / str(i)) if r["status"] != "not_started" else r
                  for i, r in enumerate(retained["attempts"])]
        require(compare(actual) == retained, "retained streaming report differs")
        print(encode(retained).decode())
    else:
        check(args.output.resolve(), args.case, args.fr.resolve(), args.opencode.resolve())
