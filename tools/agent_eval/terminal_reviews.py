"""Freeze and replay source reviews that end through a native structured answer."""
import copy
import hashlib
from pathlib import Path
import re
from urllib.parse import urlsplit

from . import native_mcp as mcp, native_references as refs, opencode_native as native
from . import source_packets as packets, source_reviews, structured_submission as protocol
from . import terminal_review_costs as costs
from .study import digest, encode, number, require
from .workspace_bundle import validate

SCHEMA = "fr-terminal-source-review-1"
PROMPT = source_reviews.PROMPT.replace("Call submit_answer once", "Call StructuredOutput once")
MAX_BYTES = 1024**2
ARTIFACTS = {"request.json", "terminal.json", "messages.json", "export.json", "events.jsonl",
             "tools.jsonl", "identity.json", "process.json", "host.stdout", "host.stderr",
             "server.stdout", "server.stderr", "export.stderr", "startup-error.json"}


def retained_process(record, process, manifest, directory):
    require(type(process["exit_code"]) is int and type(process["timed_out"]) is bool, "invalid process outcome")
    for key in ("elapsed_seconds", "sampled_cpu_seconds", "sampled_aggregate_rss_bytes",
                "sampled_disk_growth_bytes", "transcript_bytes"):
        number(process[key], key)
    truncated = record["truncated_artifacts"]
    require(isinstance(truncated, dict), "invalid artifact truncation record")
    for name, sizes in truncated.items():
        require(name in manifest and set(sizes) == {"observed_bytes", "retained_bytes"}
                and type(sizes["observed_bytes"]) is int and sizes["observed_bytes"] > MAX_BYTES
                and sizes["retained_bytes"] == MAX_BYTES and (directory / name).stat().st_size == MAX_BYTES,
                "artifact truncation differs")
    require(not truncated or record["status"] == "failed", "completed review lost artifacts")


def implementation():
    names = ("structured_submission.py", "structured_probe.py", "terminal_transport.py",
             "terminal_reviews.py", "terminal_review_costs.py", "terminal_review_runner.py")
    return {**source_reviews.implementation(), **{n: source_reviews.identity(Path(__file__).with_name(n)) for n in names},
            "terminal-reviews.py": source_reviews.identity(Path(__file__).parents[1] / "terminal-reviews.py")}


def profile(value):
    require(isinstance(value, dict) and set(value) == {"providerID", "modelID", "baseURL", "context", "output"},
            "invalid provider profile")
    require(all(isinstance(value[k], str) and len(value[k].encode()) <= 256 and re.fullmatch(r"[\w./-]+", value[k]) for k in ("providerID", "modelID")),
            "invalid provider or model identity")
    require(isinstance(value["baseURL"], str) and len(value["baseURL"].encode()) <= 2048 and not any(c in value["baseURL"] for c in "{}\r\n\0"), "invalid endpoint substitution")
    endpoint = urlsplit(value["baseURL"])
    require(endpoint.hostname and not endpoint.username and not endpoint.password and not endpoint.query
            and not endpoint.fragment and (endpoint.scheme == "https" or
                endpoint.scheme == "http" and endpoint.hostname == "127.0.0.1"), "invalid provider endpoint")
    require(endpoint.port is None or 0 < endpoint.port < 65536, "invalid provider port")
    require(all(type(value[k]) is int and 0 < value[k] <= 2**20 for k in ("context", "output")), "invalid model limits")
    require(value["output"] <= value["context"], "output exceeds context")
    return value


def design(questions, models, identities, provenance):
    require(isinstance(models, list) and 1 <= len(models) <= 4, "choose one to four model profiles")
    for model in models:
        profile(model)
    require(len({(m["providerID"], m["modelID"]) for m in models}) == len(models), "duplicate model identity")
    require(isinstance(questions, list) and 1 <= len(questions) <= 8, "choose one to eight questions")
    tasks, snapshots = [], {}
    for question in questions:
        name, files = question["id"], question["files"]
        require(isinstance(name, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", name)
                and name not in snapshots, "invalid or duplicate question identity")
        validate(files, source_reviews.legacy.MAX_WORKSPACE)
        packet = packets.build(files, question["selections"])
        tasks.append({"id": name, "question": question["question"], "packet": packet,
                      "requirement": packets.requirement(question["question"], packet), "files_sha256": digest(files)})
        require(len((PROMPT + "\nTask:\n" + tasks[-1]["requirement"]).encode()) <= 16384, "prompt exceeds budget")
        snapshots[name] = files
    tools = [t for t in refs.recovery_schemas("fr") if t["name"] != "submit_answer"]
    require(isinstance(identities, dict) and set(identities) == {"runtime", "binary_sha256", "opencode_sha256"}, "invalid runtime identities")
    require(all(isinstance(v, str) and re.fullmatch(r"[0-9a-f]{64}", v)
                for v in [identities["binary_sha256"], identities["opencode_sha256"], *identities["runtime"].values()]), "invalid executable identity")
    plan = {"schema": SCHEMA, "tasks": tasks, "models": models, "prompt": PROMPT, "format": protocol.FORMAT,
            "opencode_version": protocol.VERSION, "provider_transport": "openai-compatible",
            "cells": [{"id": t["id"] + "-" + str(i), "task": t["id"], "model": i, "arm": "fr"}
                      for t in tasks for i in range(len(models))], "limits": native.LIMITS,
            "tools_schema_version": 6, "tools": tools, "stop_after_consecutive_failures": 2, "retries": 0,
            "candidate_execution": False, "claims_verified": False, "independent_task_selection": False,
            "efficiency_comparison": False, "provenance": provenance, **identities}
    require(len(encode({"plan": plan, "sha256": digest(plan)})) <= MAX_BYTES, "review plan exceeds retention budget")
    require(len(encode(snapshots)) <= 8 * MAX_BYTES, "review snapshots exceed replay budget")
    return {"plan": copy.deepcopy(plan), "sha256": digest(plan)}, snapshots


def freeze(questions, models, binary, opencode, provenance):
    return design(questions, models, {"runtime": implementation(), "binary_sha256": source_reviews.identity(binary),
                  "opencode_sha256": source_reviews.identity(opencode)}, provenance)


def checked(frozen, snapshots, *, execution=False):
    plan = frozen["plan"]
    require(plan["schema"] == SCHEMA and digest(plan) == frozen["sha256"], "terminal review plan changed")
    require(set(snapshots) == {t["id"] for t in plan["tasks"]}, "snapshot tasks differ")
    questions = [{"id": t["id"], "question": t["question"], "files": snapshots[t["id"]],
                  "selections": [{k: s[k] for k in ("path", "sha256", "start", "end")} for s in t["packet"]["spans"]]}
                 for t in plan["tasks"]]
    expected, _ = design(questions, plan["models"],
        {k: plan[k] for k in ("runtime", "binary_sha256", "opencode_sha256")}, plan["provenance"])
    require(encode(expected) == encode(frozen), "review protocol or source changed")
    if execution:
        require(plan["runtime"] == implementation(), "review runtime changed; freeze a new plan")
    return plan


def request(plan, task, cell):
    model = plan["models"][cell["model"]]
    return {"model": {k: model[k] for k in ("providerID", "modelID")}, "agent": "fr-submission",
            "format": plan["format"], "parts": [{"type": "text", "text": plan["prompt"] + "\nTask:\n" + task["requirement"]}]}


def read(path, *, optional=False):
    if optional and not path.exists() and not path.is_symlink():
        return b""
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_BYTES, "invalid review artifact")
    return path.read_bytes()


def audit(plan, task, cell, directory):
    data = {n: mcp.decode(read(directory / n)) for n in
            ("request.json", "terminal.json", "export.json", "messages.json", "identity.json")}
    require(data["request.json"] == request(plan, task, cell), "frozen request differs")
    require(data["identity.json"] == {"schema": SCHEMA, "plan_sha256": digest(plan), "cell": cell,
            "opencode_version": plan["opencode_version"], "binary_sha256": plan["binary_sha256"],
            "opencode_sha256": plan["opencode_sha256"]}, "capture identity differs")
    messages = data["messages.json"]
    require(data["export.json"]["messages"] == messages and data["export.json"]["info"]["id"]
            == messages[0]["info"]["sessionID"], "independent export differs")
    events = [mcp.decode(line) for line in read(directory / "events.jsonl").splitlines()]
    rows = [mcp.decode(line) for line in read(directory / "tools.jsonl").splitlines()]
    result = protocol.audit(data["request.json"], data["terminal.json"], messages, events, rows)
    disclosed = costs.source(rows, task)
    provided = packets.checked(task["files"], task["packet"])
    return {**result, "review": packets.finding(result["answer"], provided + disclosed["spans"]),
            "source_context": packets.accounting(task["packet"], disclosed["spans"]),
            "fr_calls": disclosed["fr_calls"], "claims_verified": False}


def report(frozen, snapshots, output):
    plan = checked(frozen, snapshots)
    require(not output.exists() or {p.name for p in output.iterdir()} <= {c["id"] for c in plan["cells"]}, "unplanned attempt")
    attempts, failures, missing = [], 0, False
    for cell in plan["cells"]:
        directory = output / cell["id"]
        if not directory.exists():
            require(not directory.is_symlink(), "invalid attempt directory")
            missing = True
            attempts.append({"cell": cell, "status": "not_started"})
            continue
        require(not missing and failures < 2 and directory.is_dir() and not directory.is_symlink(), "attempt after stop or gap")
        manifest = mcp.decode(read(directory / "manifest.json"))
        require(set(manifest) <= ARTIFACTS | {"record.json"}
                and {p.name for p in directory.iterdir()} == set(manifest) | {"manifest.json"}, "artifact inventory differs")
        require(all(hashlib.sha256(read(directory / name)).hexdigest() == sha
                    for name, sha in manifest.items()), "review artifact changed")
        record = mcp.decode(read(directory / "record.json"))
        require(record["cell"] == cell and record["plan_sha256"] == frozen["sha256"], "attempt identity differs")
        require(record["status"] in {"completed", "failed"}, "incomplete attempt record")
        failures = failures + 1 if record["status"] == "failed" else 0
        process = mcp.decode(read(directory / "process.json"))
        retained_process(record, process, manifest, directory)
        require(process["limits"] == {k: v for k, v in plan["limits"].items() if k not in {"steps", "tool_calls"}}, "process limits changed")
        task = {**next(t for t in plan["tasks"] if t["id"] == cell["task"]), "files": snapshots[cell["task"]]}
        observed = costs.observed(read(directory / "events.jsonl", optional=True), read(directory / "tools.jsonl", optional=True), task)
        reviewed = None
        if record["status"] == "completed":
            from .structured_probe import checked_process
            checked_process(process)
            reviewed = audit(plan, task, cell, directory)
            require(record["failure"] is None and reviewed == record["audit"], "completed review replay differs")
        else:
            require("audit" not in record and isinstance(record["failure"], str) and record["failure"], "missing failure record")
            # A failed attempt never becomes accepted merely because an answer survives.
        attempts.append({"cell": cell, "status": record["status"], "failure": record["failure"],
                         "audit": reviewed, "observed": observed, "process": process,
                         "truncated_artifacts": record["truncated_artifacts"]})
    return {"schema": SCHEMA, "plan_sha256": frozen["sha256"], "attempts": attempts,
            "completed": sum(a["status"] == "completed" for a in attempts),
            "failed": sum(a["status"] == "failed" for a in attempts),
            "not_started": sum(a["status"] == "not_started" for a in attempts),
            "claims_verified": False, "efficiency_comparison": False, "provider_usage_verified": False}
