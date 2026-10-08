"""Freeze code-change trials and replay terminal submissions without executing code."""
import copy
import hashlib
from pathlib import Path
import re

from . import native_changes as edits, native_mcp as mcp, source_reviews
from . import structured_submission as protocol, terminal_reviews as review
from . import opencode_changes, structured_probe
from .study import digest, encode, require
from .workspace_bundle import validate

SCHEMA = "fr-terminal-code-changes-1"
ARMS = ("files", "fr")
ARTIFACTS = review.ARTIFACTS | {"submission.json"}
# A GC sizing hint, not an RSS cap. The process guard still enforces every limit.
CLIENT_ENVIRONMENTS = ({"BUN_OPTIONS": "--smol"},
                       {"BUN_OPTIONS": "--smol", "BUN_JSC_forceRAMSize": "268435456"})


def prompt(arm):
    return edits.prompt(arm, 2).replace(
        "Call submit_patch once with a short summary when finished, then stop.",
        'Finish with StructuredOutput: {"answer":{"summary":"brief changes and uncertainties"}}.')


def schemas(arm):
    return [tool for tool in edits.schemas(arm, 2) if tool["name"] != "submit_patch"]


def implementation():
    names = ("terminal_changes.py", "terminal_change_runner.py")
    return {**review.implementation(), **opencode_changes.implementation(),
            **{n: source_reviews.identity(Path(__file__).with_name(n)) for n in names},
            "terminal-changes.py": source_reviews.identity(Path(__file__).parents[1] / "terminal-changes.py")}


def design(tasks, models, identities, catalog, provenance, *, client_environment=None):
    require(isinstance(models, list) and 1 <= len(models) <= 4, "invalid model count")
    for model in models:
        review.profile(model)
        require(model.get("configured") is True and "variant" in model, "explicit configured variant required")
        require(model["output"] <= 2048 and model["context"] <= 32768, "model profile exceeds admitted limits")
    require(len({(m["providerID"], m["modelID"]) for m in models}) == len(models), "duplicate model")
    require(isinstance(catalog, dict) and len(encode(catalog)) <= 16384, "invalid bounded provider catalog")
    require(isinstance(tasks, list) and 1 <= len(tasks) <= 4, "invalid task count")
    frozen_tasks, snapshots = [], {}
    for task in tasks:
        require(set(task) == {"id", "requirement", "public_feedback", "files", "grader"}, "invalid change task")
        name = task["id"]
        require(isinstance(name, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", name)
                and name not in snapshots, "invalid task identity")
        validate(task["files"], edits.MAX_WORKSPACE)
        opencode_changes.isolated_grade.validate(task["grader"])
        require(all(isinstance(task[k], str) and task[k].strip() for k in ("requirement", "public_feedback")),
                "task and identical public feedback required")
        snapshots[name] = task["files"]
        frozen_tasks.append({k: v for k, v in task.items() if k != "files"} | {"files_sha256": digest(task["files"])})
        for arm in ARMS:
            require(len((prompt(arm) + task["requirement"] + task["public_feedback"]).encode()) <= 16000,
                    "change prompt exceeds budget")
    require(set(identities) == {"runtime", "binary_sha256", "opencode_sha256"}, "invalid runtime identities")
    require(all(isinstance(v, str) and re.fullmatch(r"[0-9a-f]{64}", v)
                for v in [identities["binary_sha256"], identities["opencode_sha256"], *identities["runtime"].values()]),
            "invalid executable identity")
    plan = {"schema": SCHEMA, "tasks": frozen_tasks, "models": models, "catalog": catalog,
            "prompts": {arm: prompt(arm) for arm in ARMS}, "tools": {arm: schemas(arm) for arm in ARMS},
            "format": protocol.FORMAT, "limits": review.native.LIMITS, "opencode_version": protocol.VERSION,
            "cells": [{"id": t["id"] + "-" + str(i) + "-" + arm, "task": t["id"], "model": i, "arm": arm}
                      for t in tasks for i in range(len(models)) for arm in (ARMS if i % 2 == 0 else ARMS[::-1])],
            "retries": 0, "stop_after_consecutive_failures": 2, "candidate_execution": False,
            "provenance": provenance, **identities}
    if client_environment is not None:
        require(client_environment in CLIENT_ENVIRONMENTS, "unsupported client environment")
        plan["client_environment"] = copy.deepcopy(client_environment)
    require(len(encode(plan)) <= review.MAX_BYTES and len(encode(snapshots)) <= 8 * review.MAX_BYTES,
            "frozen change inputs exceed budget")
    return {"plan": copy.deepcopy(plan), "sha256": digest(plan)}, copy.deepcopy(snapshots)


def freeze(tasks, models, binary, opencode, catalog, provenance):
    return design(tasks, models, {"runtime": implementation(), "binary_sha256": source_reviews.identity(binary),
        "opencode_sha256": source_reviews.identity(opencode)}, catalog, provenance,
        client_environment=CLIENT_ENVIRONMENTS[-1])


def checked(frozen, snapshots, *, execution=False):
    plan = frozen["plan"]
    require(plan["schema"] == SCHEMA and digest(plan) == frozen["sha256"], "change plan differs")
    require(set(snapshots) == {t["id"] for t in plan["tasks"]}, "snapshot tasks differ")
    tasks = [{k: v for k, v in t.items() if k != "files_sha256"} | {"files": snapshots[t["id"]]} for t in plan["tasks"]]
    expected, _ = design(tasks, plan["models"], {k: plan[k] for k in ("runtime", "binary_sha256", "opencode_sha256")},
                         plan["catalog"], plan["provenance"], client_environment=plan.get("client_environment"))
    require(expected == frozen, "change protocol or source differs")
    if execution:
        require(plan["runtime"] == implementation(), "change runtime differs; freeze again")
    return plan


def request(plan, task, cell):
    model = plan["models"][cell["model"]]
    return {"model": {k: model[k] for k in ("providerID", "modelID")}, "variant": model["variant"],
            "agent": "fr-submission", "format": plan["format"], "parts": [{"type": "text",
            "text": plan["prompts"][cell["arm"]] + "\nTask:\n" + task["requirement"]
                    + "\nPublic check and baseline result (not rerun during this attempt):\n" + task["public_feedback"]}]}


def replay_edits(files, arm, rows, answer):
    require(isinstance(answer, dict) and set(answer) == {"summary"}, "invalid patch summary")
    require(isinstance(answer["summary"], str) and answer["summary"].strip()
            and len(answer["summary"].encode()) <= 2048, "patch summary exceeds budget")
    machine = edits.Machine(files, arm, replay=True, version=2)
    del machine.tools["submit_patch"]
    metrics = {"fr_calls": 0, "refused_calls": 0, "tool_result_bytes": 0, "edit_argument_bytes": 0}
    try:
        for row in rows:
            params, result = row["params"], row["result"]
            require(machine.call(params, result) == result, "change tool replay differs")
            metrics["fr_calls"] += params["name"].startswith("fr_")
            metrics["refused_calls"] += "error" in result
            metrics["tool_result_bytes"] += len(encode(result))
            if params["name"] in {"replace_source", "fr_preview_body", "fr_apply_preview"}:
                metrics["edit_argument_bytes"] += len(encode(params.get("arguments", {})))
        return {"files": machine.files, "submission_sha256": digest(machine.files),
                "changed_paths": machine.changed_paths(), "summary": answer["summary"],
                "accepted_edits": machine.edits, "metrics": metrics}
    finally:
        machine.close()


def audit(plan, task, cell, root):
    data = {name: mcp.decode(review.read(root / name)) for name in
            ("request.json", "terminal.json", "export.json", "messages.json", "identity.json")}
    require(data["request.json"] == request(plan, task, cell), "frozen request differs")
    require(data["identity.json"] == {"schema": SCHEMA, "plan_sha256": digest(plan), "cell": cell,
        "opencode_version": protocol.VERSION, "binary_sha256": plan["binary_sha256"],
        "opencode_sha256": plan["opencode_sha256"]}, "capture identity differs")
    messages = data["messages.json"]
    require(data["export.json"]["messages"] == messages and data["export.json"]["info"]["id"]
            == messages[0]["info"]["sessionID"], "independent export differs")
    rows = [mcp.decode(line) for line in review.read(root / "tools.jsonl").splitlines()]
    events = [mcp.decode(line) for line in review.read(root / "events.jsonl").splitlines()]
    result = protocol.audit(data["request.json"], data["terminal.json"], messages, events, rows)
    patch = replay_edits(task["files"], cell["arm"], rows, result["answer"])
    return {**result, **patch, "behavior_verified": False, "actual_usd": None, "complete_context_accounting": False,
            "prompt_bytes": len(data["request.json"]["parts"][0]["text"].encode()),
            "tool_schema_bytes": len(encode(plan["tools"][cell["arm"]]))}


def observed(root):
    from . import native_costs
    events, tail = native_costs.prefix(review.read(root / "events.jsonl", optional=True), review.MAX_BYTES)
    rows, host_tail = native_costs.prefix(review.read(root / "tools.jsonl", optional=True), mcp.MAX_LOG)
    parts, infos = {}, {}
    for event in events:
        props = event.get("properties", {})
        if event["type"] == "message.part.updated":
            parts[props["part"]["id"]] = props["part"]
        elif event["type"] == "message.updated":
            infos[props["info"]["id"]] = props["info"]
    return {"assistant_messages": sum(i.get("role") == "assistant" for i in infos.values()),
            "host_calls": len(rows), "fr_calls": sum(r["params"]["name"].startswith("fr_") for r in rows),
            "tool_result_bytes": sum(len(encode(r["result"])) for r in rows),
            "usage": native_costs.usage([p for p in parts.values() if p["type"] == "step-finish"]),
            "unparsed_stream_tail_bytes": tail, "unparsed_host_tail_bytes": host_tail,
            "scope": "Retained client counters, including failed work; missing evidence is unknown cost.",
            "provider_usage_verified": False, "complete_context_accounting": False}


def report(frozen, snapshots, output):
    plan = checked(frozen, snapshots)
    require(not output.exists() or {p.name for p in output.iterdir()} <= {c["id"] for c in plan["cells"]}, "unplanned attempt")
    attempts, failures, stopped, missing = [], 0, False, False
    for cell in plan["cells"]:
        root = output / cell["id"]
        if not root.exists():
            require(not root.is_symlink(), "linked attempt")
            missing = True
            attempts.append({"cell": cell, "status": "not_started"})
            continue
        require(not missing and not stopped and failures < 2 and root.is_dir() and not root.is_symlink(),
                "attempt after stop or gap")
        manifest = mcp.decode(review.read(root / "manifest.json"))
        require(set(manifest) <= ARTIFACTS | {"record.json"}
                and {p.name for p in root.iterdir()} == set(manifest) | {"manifest.json"}, "artifact inventory differs")
        require(all(hashlib.sha256(review.read(root / n)).hexdigest() == sha for n, sha in manifest.items()), "artifact changed")
        record = mcp.decode(review.read(root / "record.json"))
        process = mcp.decode(review.read(root / "process.json"))
        require(record["cell"] == cell and record["plan_sha256"] == frozen["sha256"], "attempt identity differs")
        require(record["status"] in {"completed", "failed"}, "invalid attempt status")
        review.retained_process(record, process, manifest, root)
        require(process["limits"] == {k: v for k, v in plan["limits"].items() if k not in {"steps", "tool_calls"}},
                "resource limits differ")
        stopped = process["stop_reason"] is not None
        failures = failures + 1 if record["status"] == "failed" else 0
        if record["status"] == "completed":
            structured_probe.checked_process(process)
            task = next(t for t in plan["tasks"] if t["id"] == cell["task"]) | {"files": snapshots[cell["task"]]}
            replayed = audit(plan, task, cell, root)
            files = replayed.pop("files")
            require(record["failure"] is None and record["audit"] == replayed
                    and mcp.decode(review.read(root / "submission.json")) == files, "submission replay differs")
        else:
            require("audit" not in record and isinstance(record["failure"], str) and record["failure"]
                    and "submission.json" not in manifest, "failed attempt cannot claim a submission")
        attempts.append({**record, "process": process, "observed": observed(root)})
    return {"schema": SCHEMA, "plan_sha256": frozen["sha256"], "attempts": attempts,
            "completed": sum(a["status"] == "completed" for a in attempts),
            "failed": sum(a["status"] == "failed" for a in attempts),
            "not_started": sum(a["status"] == "not_started" for a in attempts),
            "behavior_verified": False, "provider_usage_verified": False, "efficiency_advantage": False}
