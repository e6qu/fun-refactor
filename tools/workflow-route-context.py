#!/usr/bin/env python3
"""Compare prescribed checked editing routes; collection runs only on GitHub."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from fr_ir import guide as guide_module  # noqa: E402
from fr_ir.guide import AgentGoal, GoalConstraints, GoalLimits, GoalOperation, GoalSelector  # noqa: E402
from fr_ir.intent import AgentIntent, IntentNeed  # noqa: E402
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation  # noqa: E402
from fr_ir.ir import ScalarRequest, TaskChange, TaskDelivery, TaskTarget  # noqa: E402
from fr_ir.runtime import FrClient, FrRuntimeError  # noqa: E402

BASELINE = "cbc7ad31ba878c58963b84da41d518178bd50abd"
BASELINE_FILE_SHA256 = "efb76e603a7eb2f410395bf68d066f381fc3cbf056319c5af4350a6aacc2b4e5"
BASELINE_FUNCTION_SHA256 = "9d6c5ae88c14ec9cfacb72c47120dc97b5d5cfff6314353fc96ed505e930ac5f"
ARMS = ("author", "task", "intent", "guide", "guide-before")
FAULTS = ("source", "checks", "fragment", "original-check")
STAGES = ("check-original", "apply", "check-applied", "undo", "check-restored",
          "redo", "check-applied", "deliver-patch")
SOURCES = (
    {"id": "rust-scalar", "files": {"src/lib.rs":
        "pub fn adjust(value: i64) -> i64 {\n    return value + 7;\n}\n"},
     "edits": [{"name": "adjust", "path": "src/lib.rs", "scalar": True,
                "old": "value + 7", "new": "value + 9"}]},
    {"id": "python-body", "files": {"text.py":
        "def normalize(value):\n    return value.strip()\n"},
     "edits": [{"name": "normalize", "path": "text.py", "body": "return value.strip().upper()\n",
                "old": "return value.strip()", "new": "return value.strip().upper()"}]},
    {"id": "rust-two-bodies", "files": {"src/lib.rs":
        "pub fn lower(value: i64) -> i64 { value - 1 }\n"
        "pub fn upper(value: i64) -> i64 { value + 1 }\n"},
     "edits": [
         {"name": "lower", "path": "src/lib.rs", "body": "{ value - 2 }\n",
          "old": "value - 1", "new": "value - 2"},
         {"name": "upper", "path": "src/lib.rs", "body": "{ value + 2 }\n",
          "old": "value + 1", "new": "value + 2"}]},
)
BOUND_FILES = (
    "tools/workflow-route-context.py", "sdk/python/src/fr_ir/guide.py",
    "sdk/python/src/fr_ir/intent.py", "sdk/python/src/fr_ir/intent_actions.py",
    "sdk/python/src/fr_ir/runtime.py", "sdk/python/src/fr_ir/ir.py",
    "src/project/agent_guide.rs", "src/project/agent_intent.rs",
    "src/project/agent_actions.rs", "src/project/task_change.rs",
    "src/project/author.rs", "src/workflow.rs", "src/checks.rs", "src/history.rs",
    "Cargo.lock",
)


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def bindings():
    return {name: digest((ROOT / name).read_bytes()) for name in BOUND_FILES}


def baseline_function():
    source = subprocess.check_output(
        ["git", "show", f"{BASELINE}:sdk/python/src/fr_ir/guide.py"], cwd=ROOT).decode()
    function = next(node for node in ast.parse(source).body
                    if isinstance(node, ast.FunctionDef) and node.name == "execute_guide")
    text = ast.get_source_segment(source, function)
    if digest(source.encode()) != BASELINE_FILE_SHA256 or digest(text.encode()) != BASELINE_FUNCTION_SHA256:
        raise ValueError("before execution source changed")
    scope = dict(vars(guide_module))
    exec(compile(text, "<retained-before-execute-guide>", "exec"), scope)
    return scope["execute_guide"], {"commit": BASELINE, "file": "sdk/python/src/fr_ir/guide.py",
                                   "file_sha256": digest(source.encode()), "function": text}


class RecordedClient(FrClient):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.events = []

    def call(self, *arguments, input_bytes=None):
        event = {"request": {"arguments": list(arguments),
                             "stdin": input_bytes.decode() if input_bytes is not None else None}}
        if "--from" in arguments:
            path = arguments[arguments.index("--from") + 1]
            if path != "-":
                event["request"]["file_input"] = (self.root / path).read_text()
        self.events.append(event)
        try:
            result = super().call(*arguments, input_bytes=input_bytes)
        except FrRuntimeError as error:
            event["response"] = {"exit_code": error.exit_code, "error": str(error),
                                 "report": error.report}
            raise
        event["response"] = {"exit_code": 0, "report": result.to_data()}
        return result


def checker(case, *, final=False):
    """A compiler/runtime oracle independent of the edit planner."""
    if case["id"] == "python-body":
        expression = "value.strip().upper()" if final else "normalize(' x ') in ('x', 'X')"
        if final:
            return ("from text import normalize\n"
                    "for value in ('', ' x ', '\\tMixed words\\n', 'Straße'):\n"
                    f"    assert normalize(value) == {expression}\n")
        return "from text import normalize\nassert " + expression + "\n"
    if case["id"] == "rust-scalar":
        expectation = "9" if final else "adjust(0)"
        assertion = f"assert_eq!(adjust(value), value + {expectation});"
    else:
        offset = "2" if final else "upper(0)"
        assertion = (f"assert_eq!(lower(value), value - {offset}); "
                     f"assert_eq!(upper(value), value + {offset});")
    harness = "fn main() { for value in -16..=16 { " + assertion + " } }\n"
    return ("import pathlib, subprocess, tempfile\n"
            "with tempfile.TemporaryDirectory() as directory:\n"
            "    path = pathlib.Path(directory)\n"
            f"    text = pathlib.Path('src/lib.rs').read_text() + {harness!r}\n"
            "    (path / 'main.rs').write_text(text)\n"
            "    subprocess.run(['rustc', '--edition=2021', str(path / 'main.rs'), '-o', str(path / 'check')], check=True)\n"
            "    subprocess.run([str(path / 'check')], check=True)\n")


def prepare(root, case):
    for name, text in case["files"].items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    (root / ".fr").mkdir()
    (root / "artifacts").mkdir()
    control = root.parent / "control"
    control.mkdir()
    check = ("import pathlib, sys\nsys.path.insert(0, str(pathlib.Path.cwd()))\n"
             "assert not pathlib.Path('../control/fail').exists(), 'original check failed'\n"
             + checker(case))
    (root / ".fr/check.py").write_text(check)
    (root / ".fr/checks.json").write_bytes(canonical({"schema": 1, "checks": [{
        "name": "behavior", "argv": ["python3", "-B", ".fr/check.py"], "cwd": ".",
        "timeout_seconds": 30, "covers": ["compiler or Python runtime; finite behavior cases"]}]}))
    for i, edit in enumerate(case["edits"]):
        if "body" in edit:
            (control / f"body-{i}.txt").write_text(edit["body"])


def start(client, case, arm):
    delivery = TaskDelivery(patch="artifacts/change.patch", check_output_bytes=512)
    edits = case["edits"]
    guide = None
    if arm.startswith("guide"):
        operation = (GoalOperation("semantic-scalar", {"operation": "set-int", "from": "7", "to": "9"})
                     if case["id"] == "rust-scalar" else
                     GoalOperation("source-bodies", {"additional": [
                         {"name": e["name"], "scope": e["path"]} for e in edits[1:]]})
                     if len(edits) > 1 else GoalOperation("source-body"))
        guide = client.guide(AgentGoal(
            "change", selector=GoalSelector(name=edits[0]["name"], scope=edits[0]["path"]),
            operation=operation, constraints=GoalConstraints(allow_source=True),
            context=GoalLimits(packet_limit=65536), checks=("behavior",), delivery=delivery))
        data = guide.to_data()
        rows = data.get("targets", [data["target"]])
        handles = {row["name"]: row["handle"] for row in rows}
    else:
        report = client.project("select", *(e["name"] for e in edits), "--signature").to_data()
        rows = [dict(zip(report["columns"], row)) for row in report["rows"]]
        handles = {}
        for edit in edits:
            matches = [r for r in rows if r["name"] == edit["name"] and r["path"] == edit["path"]]
            if len(matches) != 1:
                raise RuntimeError("discovery must resolve exactly one declaration per target")
            handles[edit["name"]] = matches[0]["handle"]
    targets = [TaskTarget(
        f"edit-{i}", handles[e["name"]], "edit-body-scalar" if e.get("scalar") else "replace-body",
        scalar=ScalarRequest("set-int", "7", "9") if e.get("scalar") else None,
        from_path=None if e.get("scalar") else f"../control/body-{i}.txt",
    ) for i, e in enumerate(edits)]
    paths = sorted({e["path"] for e in edits})
    postconditions = {"files-changed": len(paths), "edits": len(edits),
                      "changed-operations": len(edits), "paths-changed": paths}
    change = TaskChange([], targets, postconditions, ["behavior"], delivery)
    if guide is not None:
        return client.review_guide(guide, TaggedIntentAction(TaskChangeOperation(change)))
    if arm == "task":
        return client.review(change)
    if arm == "intent":
        return client.compile(AgentIntent(
            targets[0].handle, "change", needs=(IntentNeed("map", "code_map", "/target"),),
            packet_limit=65536, action=TaggedIntentAction(TaskChangeOperation(change))))
    checks = client.call("checks").to_data()
    author = {"operations": [{k: v for k, v in t.to_data().items() if k != "id"} for t in targets],
              "postconditions": postconditions}
    author_path = "../control/author.json"
    (client.root / author_path).write_bytes(canonical(author))
    preview = client.call("author", "batch", "--from", author_path,
                          "--diff-bytes", "65536").to_data()
    saved = client.call("author", "batch", "--from", author_path, "--save-plan", "--plan-basis",
                        preview["plan_context_basis"]).to_data()
    workflow = {"schema": 1, "transaction": saved["transaction"],
                "transaction-context-basis": saved["transaction_context_basis"],
                "checks": {"basis": checks["basis"], "names": ["behavior"]},
                "check-original": True, "compact-success": True, "exercise-reversal": True,
                "patch": {"output": "artifacts/change.patch"}, "check-output-bytes": 512}
    path = "../control/workflow.json"
    (client.root / path).write_bytes(canonical(workflow))
    preview = client.call("workflow", "--from", path).to_data()
    return path, preview["workflow_basis"]


def finish(client, review, arm, before):
    if arm == "guide-before":
        return before(client, review).to_data()
    if arm == "guide":
        return client.execute_guide(review).to_data()
    if arm == "intent":
        return client.execute_intent(review).to_data()
    if arm == "task":
        return client.execute(review).to_data()
    path, basis = review
    return client.call("workflow", "--from", path, "--write", "--basis", basis).to_data()


def source_state(root, case):
    return {name: (root / name).read_text() for name in case["files"]}


def metrics(events):
    requests = sum(len(canonical(e["request"])) for e in events)
    responses = sum(len(canonical(e["response"])) for e in events)
    return {"process_calls": len(events), "request_bytes": requests,
            "response_bytes": responses, "exchange_bytes": requests + responses}


def run_arm(root, case, arm, executable, before, fault=None):
    if root.parent.exists():
        shutil.rmtree(root.parent)
    prepare(root, case)
    client = RecordedClient(root, executable=executable, timeout=60)
    review = start(client, case, arm)
    if fault == "source":
        path = root / next(iter(case["files"]))
        path.write_text(path.read_text() + "# changed after review\n")
    elif fault == "checks":
        path = root / ".fr/checks.json"
        config = json.loads(path.read_text())
        config["checks"][0]["covers"].append("changed after review")
        path.write_bytes(canonical(config))
    elif fault == "fragment":
        (root.parent / "control/body-0.txt").write_text("return 'different'\n")
    elif fault == "original-check":
        (root.parent / "control/fail").write_text("fail\n")
    elif fault == "goal":
        review.guide.goal.operation.fields["unexpected"] = "changed"
    elif fault == "review":
        review.compiled.packet._value["action"]["review"]["diff"] = "changed"
    before_source = source_state(root, case)
    execute_start = len(client.events)
    error = None
    result = None
    try:
        result = finish(client, review, arm, before)
    except FrRuntimeError as refused:
        error = {"message": str(refused), "exit_code": refused.exit_code, "report": refused.report}
    after = source_state(root, case)
    patch = root / "artifacts/change.patch"
    state = root / ".fr-history/state.json"
    history = json.loads(state.read_text()) if state.exists() else {"records": []}
    workflow = result.get("workflow", result) if result else None
    expected = dict(case["files"])
    for edit in case["edits"]:
        expected[edit["path"]] = expected[edit["path"]].replace(edit["old"], edit["new"])
    if fault:
        if after != before_source or patch.exists() or (error is None and workflow.get("passed") is not False):
            raise RuntimeError(f"{arm}/{fault}: changed source or delivered a patch after refusal")
        oracle = None
    else:
        if error or not workflow or workflow.get("passed") is not True or after != expected or not patch.exists():
            raise RuntimeError(json.dumps({"case": case["id"], "arm": arm,
                "error": error, "result": result, "expected": expected, "source": after,
                "patch_exists": patch.exists(), "events": client.events}))
        stages = [{"stage": row["stage"], "status": row["status"]} for row in workflow["stages"]]
        if stages != [{"stage": stage, "status": "passed"} for stage in STAGES]:
            raise RuntimeError("incomplete checked lifecycle")
        script = checker(case, final=True)
        checked = subprocess.run([sys.executable, "-B", "-c", script], cwd=root,
                                 capture_output=True, timeout=60)
        oracle = {"program": script, "exit_code": checked.returncode,
                  "stdout": checked.stdout.decode(), "stderr": checked.stderr.decode()}
        if checked.returncode:
            raise RuntimeError(f"independent behavior check failed: {oracle}")
    return {"case": case["id"], "arm": arm, "fault": fault, "events": client.events,
            "execution_start": execute_start, "metrics": metrics(client.events),
            "before_execution": before_source, "source": after,
            "patch": patch.read_text() if patch.exists() else None,
            "error": error, "result": result, "oracle": oracle,
            "history_records": len(history["records"]),
            "required_checks_bound": bool(history["records"]) and all(
                "required_checks" in r for r in history["records"]),
            "fragment_files": {f"body-{i}.txt": e["body"] for i, e in enumerate(case["edits"]) if "body" in e}}


def measure(executable):
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Collect on GitHub Actions; local collection is disabled")
    before, baseline = baseline_function()
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-route-context-") as directory:
        base = Path(directory)
        for case in SOURCES:
            for arm in ARMS:
                runs.append(run_arm(base / "cell" / "project", case, arm, executable, before))
        case = SOURCES[1]
        for fault in FAULTS:
            for arm in ARMS:
                # A saved author transaction owns its fragment bytes already. Changes to the
                # original fragment file no longer change that plan; report this distinction.
                if fault == "fragment" and arm == "author":
                    continue
                runs.append(run_arm(base / "cell" / "project", case, arm, executable, before, fault))
        for fault in ("goal", "review"):
            for arm in ("guide", "guide-before"):
                runs.append(run_arm(base / "cell" / "project", case, arm, executable, before, fault))
    return {"schema": "fr-workflow-route-context-1", "bindings": bindings(),
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
            "binary_sha256": digest(Path(executable).read_bytes()), "baseline": baseline,
            "fixtures": list(SOURCES), "runs": runs,
            "limits": "Prescribed edits, canonical SDK request/response bytes and finite behavior checks. "
                      "No model, hidden context, billing, token, time or population comparison. "
                      "Common fixture setup and external oracle calls are excluded from fr traffic. "
                      "Fragment file bytes are retained separately. Saved author plans own their fragments."}


def audit(value):
    if value.get("schema") != "fr-workflow-route-context-1" or value.get("bindings") != bindings():
        raise ValueError("route comparison schema or source bindings changed")
    if value["fixtures"] != json.loads(json.dumps(SOURCES)):
        raise ValueError("route comparison fixtures changed")
    baseline = value["baseline"]
    if (set(baseline) != {"commit", "file", "file_sha256", "function"}
            or baseline["commit"] != BASELINE or baseline["file"] != "sdk/python/src/fr_ir/guide.py"
            or baseline["file_sha256"] != BASELINE_FILE_SHA256
            or digest(baseline["function"].encode()) != BASELINE_FUNCTION_SHA256):
        raise ValueError("before execution function changed")
    expected_keys = {(c["id"], a, None) for c in SOURCES for a in ARMS}
    expected_keys |= {(SOURCES[1]["id"], a, f) for f in FAULTS for a in ARMS
                      if (f, a) != ("fragment", "author")}
    expected_keys |= {(SOURCES[1]["id"], a, f) for f in ("goal", "review") for a in ("guide", "guide-before")}
    rows = {(r["case"], r["arm"], r["fault"]): r for r in value["runs"]}
    if set(rows) != expected_keys or len(rows) != len(value["runs"]):
        raise ValueError("route comparison has missing or duplicate cells")
    for key, row in rows.items():
        if row["metrics"] != metrics(row["events"]):
            raise ValueError("route traffic accounting changed")
        events = row["events"]
        if not 0 < row["execution_start"] <= len(events):
            raise ValueError("route execution boundary is absent")
        if any(e["response"]["exit_code"] != 0 for e in events[:row["execution_start"]]):
            raise ValueError("route preview failed before execution")
        if row["result"] is not None and events[-1]["response"] != {
                "exit_code": 0, "report": row["result"]}:
            raise ValueError("route result differs from its recorded native response")
        if row["error"] and row["error"]["exit_code"] is not None:
            if events[-1]["response"] != {"exit_code": row["error"]["exit_code"],
                    "error": row["error"]["message"], "report": row["error"]["report"]}:
                raise ValueError("route refusal differs from its recorded native response")
        if row["fault"]:
            workflow = (row["result"].get("workflow", row["result"]) if row["result"] else None)
            if (row["before_execution"] != row["source"] or row["patch"] is not None
                    or not (row["error"] or (workflow and workflow.get("passed") is False))):
                raise ValueError("refused route changed source or delivered a patch")
            if row["fault"] in ("goal", "review") and row["execution_start"] != len(row["events"]):
                # The former implementation uses a guide read for a mutated goal.
                if not (row["arm"] == "guide-before" and row["fault"] == "goal"):
                    raise ValueError("mutated review launched native work")
            continue
        case = next(c for c in SOURCES if c["id"] == row["case"])
        expected = dict(case["files"])
        for edit in case["edits"]:
            expected[edit["path"]] = expected[edit["path"]].replace(edit["old"], edit["new"])
        workflow = row["result"].get("workflow", row["result"])
        stages = [{"stage": r["stage"], "status": r["status"]} for r in workflow["stages"]]
        if (row["error"] or row["source"] != expected or not row["patch"]
                or workflow.get("passed") is not True or row["oracle"]["exit_code"] != 0
                or row["oracle"]["program"] != checker(case, final=True)
                or any(e["response"]["exit_code"] != 0 for e in events)
                or stages != [{"stage": s, "status": "passed"} for s in STAGES]
                or row["required_checks_bound"] is not (row["arm"] != "author")):
            raise ValueError("route outcome, behavior, check binding or lifecycle changed")
        receipt = workflow["stages"][-1]["result"]
        if receipt["sha256"] != digest(row["patch"].encode()) or receipt["bytes"] != len(row["patch"].encode()):
            raise ValueError("route patch differs from its delivery receipt")
    differences = {}
    for case in SOURCES:
        peers = [rows[(case["id"], a, None)] for a in ARMS]
        if len({r["patch"] for r in peers}) != 1:
            raise ValueError("route patches differ")
        old, new = peers[-1], peers[-2]
        if old["metrics"]["process_calls"] != new["metrics"]["process_calls"] + 1:
            raise ValueError("guided consolidation did not remove exactly one call")
        differences[case["id"]] = {key: new["metrics"][key] - old["metrics"][key]
                                    for key in new["metrics"]}
    return {"cells": len(rows), "successes": 15, "refusals": len(rows) - 15,
            "guide_difference": differences}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        value = json.loads(args.audit.read_text()) if args.audit else measure(str(args.fr.resolve()))
        if args.output:
            args.output.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
        summary = audit(value)
    except Exception as error:
        if args.output:
            args.output.with_suffix(".failed.txt").write_text(str(error) + "\n")
        raise
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
