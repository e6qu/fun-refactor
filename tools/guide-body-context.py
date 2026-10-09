#!/usr/bin/env python3
"""Measure complete guided body-edit programs and retained failure recovery on GitHub."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("routes", ROOT / "tools/workflow-route-context.py")
routes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(routes)
from fr_ir import runtime  # noqa: E402

CASES = (routes.SOURCES[1], routes.SOURCES[2],
         {"id": "rust-body", "files": routes.SOURCES[0]["files"], "edits": [{
             "name": "adjust", "path": "src/lib.rs", "body": "{\n    return value + 9;\n}\n",
             "old": "value + 7", "new": "value + 9"}]},
         {"id": "python-two-bodies", "files": {
             "left.py": "def same(value):\n    return value.strip()\n",
             "right.py": "def same(value):\n    return value.strip()\n"}, "edits": [
                 {"name": "same", "path": "left.py", "body": "return value.strip().upper()\n",
                  "old": "return value.strip()", "new": "return value.strip().upper()"},
                 {"name": "same", "path": "right.py", "body": "return value.strip().lower()\n",
                  "old": "return value.strip()", "new": "return value.strip().lower()"}]})
ARMS = ("explicit", "builder")
FAULTS = (None, "source", "checks", "applied-check", "undo-conflict")


def program(case, arm):
    """The complete executed caller, including imports, discovery, authored bodies and submission."""
    edits = case["edits"]
    operation = ("GoalOperation('source-body')" if len(edits) == 1 else
                 "GoalOperation('source-bodies', {'additional': " + repr([
                     {"name": e["name"], "scope": e["path"]} for e in edits[1:]]) + "})")
    imports = "from fr_ir.ir import TaskDelivery\n"
    if arm == "explicit":
        imports = ("from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget\n"
                   "from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation\n")
    text = ("from fr_ir.runtime import FrClient\n"
            "from fr_ir.guide import AgentGoal, GoalSelector, GoalOperation, GoalConstraints, GoalLimits\n"
            + imports + "client = FrClient('.')\n"
            "guide = client.guide(AgentGoal('change',\n"
            f"    selector=GoalSelector(name={edits[0]['name']!r}, scope={edits[0]['path']!r}),\n"
            f"    operation={operation}, constraints=GoalConstraints(allow_source=True),\n"
            "    context=GoalLimits(packet_limit=65536), checks=('behavior',),\n"
            "    delivery=TaskDelivery(patch='artifacts/change.patch', check_output_bytes=512)))\n"
            f"authored = {repr({(e['path'], e['name']): e['body'] for e in edits})}\n"
            "data = guide.to_data()\n"
            "rows = data.get('targets', [data['target']])\n"
            "assert len(rows) == len(authored)\n"
            "assert {(r['path'], r['name']) for r in rows} == set(authored)\n"
            "bodies = {r['handle']: authored[(r['path'], r['name'])] for r in rows}\n")
    if arm == "explicit":
        text += ("paths = sorted({r['path'] for r in rows})\n"
                 "change = TaskChange((), tuple(TaskTarget(f'body-{i}', r['handle'],\n"
                 "    'replace-body', fragment=bodies[r['handle']]) for i, r in enumerate(rows)),\n"
                 "    {'files-changed': len(paths), 'edits': len(rows),\n"
                 "     'changed-operations': len(rows), 'paths-changed': paths},\n"
                 "    guide.goal.checks, guide.goal.delivery)\n"
                 "action = TaggedIntentAction(TaskChangeOperation(change),\n"
                 "    diff_bytes=guide.goal.context.token_limit, report_bytes=guide.goal.context.packet_limit,\n"
                 "    proof_expectation=guide.goal.proof)\n")
    else:
        text += "action = guide.source_body_action(bodies)\n"
    return (text + "review = client.review_guide(guide, action)\n"
            "result = client.execute_guide(review).to_data()\n"
            "submission = {'passed': result['passed'], 'transaction': result['transaction']}\n")


def bindings():
    return {**routes.bindings(), "tools/guide-body-context.py": routes.digest(Path(__file__).read_bytes())}


def expected_source(case):
    result = dict(case["files"])
    for edit in case["edits"]:
        result[edit["path"]] = result[edit["path"]].replace(edit["old"], edit["new"])
    return result


def checker(case, *, final=False):
    if case["id"] == "rust-body":
        return routes.checker(routes.SOURCES[0], final=final)
    if case["id"] != "python-two-bodies":
        return routes.checker(case, final=final)
    return ("from left import same as left\nfrom right import same as right\n"
            "for value in ('', ' x ', '\\tMixed words\\n', 'Straße'):\n" +
            ("    assert left(value) == value.strip().upper()\n"
             "    assert right(value) == value.strip().lower()\n" if final else
             "    assert left(value) in (value.strip(), value.strip().upper())\n"
             "    assert right(value) in (value.strip(), value.strip().lower())\n"))


def execute(root, case, arm, fault, executable):
    if root.parent.exists():
        shutil.rmtree(root.parent)
    routes.prepare(root, case)
    # Same declared check for both arms. The external control is set after review.
    check = root / ".fr/check.py"
    check.write_text("import pathlib, sys\nsys.path.insert(0, str(pathlib.Path.cwd()))\n" + checker(case) +
        "\nif pathlib.Path('../control/fail-applied').exists():\n"
        f"    assert {{p: pathlib.Path(p).read_text() for p in {list(case['files'])!r}}} == {case['files']!r}, 'applied check failed'\n")
    client = routes.RecordedClient(root, executable=executable, timeout=60)
    source = program(case, arm)
    prefix, separator, suffix = source.partition("result = client.execute_guide")
    scope = {}
    original_client = runtime.FrClient
    runtime.FrClient = lambda *args, **kwargs: client
    try:
        exec(compile(prefix, f"<{arm}-body-program>", "exec"), scope)
    finally:
        runtime.FrClient = original_client
    if fault == "source":
        path = root / next(iter(case["files"]))
        path.write_text(path.read_text() + "\n# changed after review\n" if path.suffix == ".py"
                        else path.read_text() + "\n// changed after review\n")
    elif fault == "checks":
        path = root / ".fr/checks.json"
        data = json.loads(path.read_text())
        data["checks"][0]["covers"].append("changed after review")
        path.write_bytes(routes.canonical(data))
    elif fault in ("applied-check", "undo-conflict"):
        (root.parent / "control/fail-applied").touch()
    before = routes.source_state(root, case)
    boundary = len(client.events)
    error = None
    try:
        exec(compile(separator + suffix, f"<{arm}-body-program>", "exec"), scope)
    except runtime.FrRuntimeError as refused:
        error = {"message": str(refused), "exit_code": refused.exit_code, "report": refused.report}
    result = scope.get("result")
    execution_report = result if result is not None else error["report"] if error else None
    after = routes.source_state(root, case)
    patch = root / "artifacts/change.patch"
    recovery = None
    if fault in ("applied-check", "undo-conflict"):
        if (not execution_report or execution_report.get("passed") is not False
                or after != expected_source(case) or patch.exists()):
            raise ValueError(f"applied failure not retained: {error!r} {result!r}")
        transaction = str(execution_report["transaction"])
        # Reopen the client: recovery must use the retained transaction, not the in-memory review.
        reopened = routes.RecordedClient(root, executable=executable, timeout=60)
        shown = reopened.call("history", "show", transaction).to_data()
        undo_preview = reopened.call("history", "undo", transaction).to_data()
        conflict = None
        if fault == "undo-conflict":
            path = root / next(iter(case["files"]))
            path.write_text(path.read_text() + ("\n# preserve this later edit\n" if path.suffix == ".py"
                                               else "\n// preserve this later edit\n"))
            conflict = routes.source_state(root, case)
        undo_error = None
        undone = None
        try:
            undone = reopened.call("history", "undo", transaction, "--write", "--no-diff").to_data()
        except runtime.FrRuntimeError as refused:
            undo_error = {"message": str(refused), "exit_code": refused.exit_code, "report": refused.report}
        restored = routes.source_state(root, case)
        checks = None
        if fault == "applied-check":
            catalog = reopened.call("checks").to_data()
            checks = reopened.call("checks", "--run", "behavior", "--basis", catalog["basis"]).to_data()
        recovery = {"events": reopened.events, "shown": shown, "preview": undo_preview,
                    "conflict_source": conflict, "result": undone, "error": undo_error,
                    "source": restored, "checks": checks, "patch_exists": patch.exists()}
    oracle = None
    if fault is None:
        script = checker(case, final=True)
        checked = subprocess.run([sys.executable, "-B", "-c", script], cwd=root,
                                 capture_output=True, timeout=60)
        oracle = {"program": script, "exit_code": checked.returncode,
                  "stdout": checked.stdout.decode(), "stderr": checked.stderr.decode()}
    submission = scope.get("submission")
    return {"case": case["id"], "arm": arm, "fault": fault, "program": source,
            "program_bytes": len(source.encode()), "submission": submission,
            "agent_bytes": len(source.encode()) + len(routes.canonical(submission)),
            "events": client.events, "execution_start": boundary, "metrics": routes.metrics(client.events),
            "before": before, "source": after, "result": result, "error": error,
            "patch": patch.read_text() if patch.exists() else None,
            "oracle": oracle, "recovery": recovery}


def measure(executable):
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Collect on GitHub Actions; local collection is disabled")
    with tempfile.TemporaryDirectory(prefix="fr-guide-body-") as directory:
        rows = [execute(Path(directory) / "cell/project", case, arm, fault, executable)
                for case in CASES for arm in ARMS for fault in FAULTS]
    return {"schema": "fr-guide-body-context-1", "bindings": bindings(), "runs": rows,
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
            "binary_sha256": routes.digest(Path(executable).read_bytes()),
            "limits": "Prescribed complete caller programs, canonical submissions and internal traffic. "
                      "No model, tokenizer, billing or autonomous-efficiency measurement. "
                      "Common fixture setup, instrumentation and independent oracle are excluded. "
                      "Recovery traffic is retained separately from edit traffic."}


def audit(value):
    if value.get("schema") != "fr-guide-body-context-1" or value.get("bindings") != bindings():
        raise ValueError("body comparison schema or source bindings changed")
    rows = {(r["case"], r["arm"], r["fault"]): r for r in value["runs"]}
    expected = {(c["id"], a, f) for c in CASES for a in ARMS for f in FAULTS}
    if set(rows) != expected or len(rows) != len(value["runs"]):
        raise ValueError("body comparison has missing or duplicate cells")
    for (case_id, arm, fault), row in rows.items():
        case = next(c for c in CASES if c["id"] == case_id)
        source = program(case, arm)
        if (row["program"] != source or row["program_bytes"] != len(source.encode())
                or row["agent_bytes"] != len(source.encode()) + len(routes.canonical(row["submission"]))
                or row["metrics"] != routes.metrics(row["events"])):
            raise ValueError("body program or traffic accounting changed")
        events = row["events"]
        if row["execution_start"] != 2 or len(events) != 3:
            raise ValueError("body comparison lost guide, review or execution")
        if row["result"] is not None and events[-1]["response"] != {"exit_code": 0, "report": row["result"]}:
            raise ValueError("body result differs from recorded response")
        if row["error"] and events[-1]["response"] != {
                "exit_code": row["error"]["exit_code"], "error": row["error"]["message"], "report": row["error"]["report"]}:
            raise ValueError("body refusal differs from recorded response")
        if fault in ("source", "checks"):
            if not row["error"] or row["source"] != row["before"] or row["patch"] or row["recovery"]:
                raise ValueError("stale body review changed source or delivered a patch")
            continue
        execution_report = row["result"] if row["result"] is not None else row["error"]["report"]
        workflow = execution_report["workflow"]
        if row["source"] != expected_source(case):
            raise ValueError("body edit produced unexpected source")
        stages = [(r["stage"], r["status"]) for r in workflow["stages"]]
        if fault is None:
            if (row["error"] or not row["result"]["passed"] or stages != [(s, "passed") for s in routes.STAGES]
                    or not row["patch"] or row["oracle"]["exit_code"] != 0
                    or row["oracle"]["program"] != checker(case, final=True)):
                raise ValueError("body edit lost checked delivery or independent behavior")
            receipt = workflow["stages"][-1]["result"]
            if receipt["sha256"] != routes.digest(row["patch"].encode()) or receipt["bytes"] != len(row["patch"].encode()):
                raise ValueError("body patch differs from delivery receipt")
        else:
            expected_stages = [(s, "passed" if i < 2 else "failed" if i == 2 else "pending")
                               for i, s in enumerate(routes.STAGES)]
            if (execution_report["passed"] or row["patch"] or stages != expected_stages
                    or workflow["transaction_status"] != "applied"):
                raise ValueError("applied failure did not retain changed source and withhold delivery")
            recovery = row["recovery"]
            if not recovery or recovery["patch_exists"] or not recovery["events"]:
                raise ValueError("missing retained recovery")
            recovered_events = recovery["events"]
            transaction = str(execution_report["transaction"])
            for index, verb, receipt in ((0, "show", "shown"), (1, "undo", "preview")):
                if (recovered_events[index]["request"]["arguments"] != ["history", verb, transaction]
                        or recovered_events[index]["response"] != {"exit_code": 0, "report": recovery[receipt]}):
                    raise ValueError("recovery receipt differs from recorded transaction")
            if recovered_events[2]["request"]["arguments"] != ["history", "undo", transaction, "--write", "--no-diff"]:
                raise ValueError("recovery did not undo the retained transaction")
            if fault == "applied-check":
                if (recovery["error"] or recovery["source"] != case["files"]
                        or not recovery["result"]["applied"] or not recovery["checks"]["passed"]
                        or len(recovered_events) != 5
                        or recovered_events[2]["response"] != {"exit_code": 0, "report": recovery["result"]}
                        or recovered_events[4]["response"] != {"exit_code": 0, "report": recovery["checks"]}):
                    raise ValueError("reopened transaction did not restore checked source")
            elif (not recovery["error"] or recovery["source"] != recovery["conflict_source"]
                  or recovery["result"] is not None or len(recovered_events) != 3
                  or recovered_events[2]["response"] != {"exit_code": recovery["error"]["exit_code"],
                      "error": recovery["error"]["message"], "report": recovery["error"]["report"]}):
                raise ValueError("undo conflict overwrote a later edit")
    differences = {}
    for case in CASES:
        before, after = (rows[(case["id"], a, None)] for a in ARMS)
        if (before["patch"] != after["patch"]
                or [e["request"] for e in before["events"][:2]] != [e["request"] for e in after["events"][:2]]):
            raise ValueError("body helper changed the native operation or result")
        differences[case["id"]] = {"program_bytes": after["program_bytes"] - before["program_bytes"],
                                  "agent_bytes": after["agent_bytes"] - before["agent_bytes"]}
    return {"cells": len(rows), "difference": differences}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--program-sizes", action="store_true")
    args = parser.parse_args()
    if args.program_sizes:
        print(json.dumps({c["id"]: {a: len(program(c, a).encode()) for a in ARMS} for c in CASES}, indent=2))
        return
    value = json.loads(args.audit.read_text()) if args.audit else measure(str(args.fr.resolve()))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(audit(value), indent=2))


if __name__ == "__main__":
    main()
