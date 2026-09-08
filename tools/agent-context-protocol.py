#!/usr/bin/env python3
"""Project Agent Context Protocol v2 onto the frozen coordinated cohort."""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def imported(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


harness = imported("agent_eval_harness_context", ROOT / "tools/agent-eval.py")
checks_policy = imported("checks_policy_context", ROOT / "tools/checks-policy-context.py")
EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-08-coordinated"
PROJECT_FIELDS = ("coverage", "handle_prefix", "revision")
HISTORY_FIELDS = ("action", "changes", "transaction")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(value):
    data = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
    return hashlib.sha256(data).hexdigest()


def project_basis(report):
    require(all(field in report for field in PROJECT_FIELDS), "Project context is incomplete")
    return "frcb1:" + sha([
        "fr-context-1",
        report["revision"],
        report["handle_prefix"],
        report["coverage"],
    ])


def history_basis(transaction, action, changes):
    return "frhb1:" + sha(["fr-history-context-1", transaction, action, changes])


def render_skill_read(request):
    relative = Path(request["path"])
    require(relative.parts[:1] == ("skill",), "Skill read leaves bundle")
    skill_root = (ROOT / "skills/fr").resolve()
    path = (skill_root / Path(*relative.parts[1:])).resolve()
    require(path.is_relative_to(skill_root) and path.is_file(), "Skill read is missing from the current bundle")
    start, count = request.get("start", 1), request.get("lines", 80)
    lines = path.read_text().splitlines(keepends=True)
    text = "".join(
        f"{index + 1}: {line}"
        for index, line in enumerate(lines)
        if start - 1 <= index < start - 1 + count
    )
    return json.dumps(
        {
            "text": text[:20000],
            "total_lines": len(lines),
            "next_line": start + count if start + count <= len(lines) else None,
        },
        ensure_ascii=False,
    )


def full_history_changes(events):
    changes = {}
    for event in events:
        request = event["request"]
        args = request.get("args", [])
        if request.get("tool") != "fr" or len(args) < 3 or args[0] != "history":
            continue
        payload = json.loads(event["visible"])
        report = payload.get("result")
        if not isinstance(report, dict) or not isinstance(report.get("changes"), list):
            continue
        if all(isinstance(change.get("diff"), str) for change in report["changes"]):
            changes[(report["transaction"], report["action"])] = copy.deepcopy(report["changes"])
    return changes


def forward_changes(changes, transaction, action):
    direct = changes.get((transaction, action))
    if direct is not None:
        return direct
    if action == "apply":
        return changes.get((transaction, "redo"))
    if action == "redo":
        return changes.get((transaction, "apply"))
    return None


def project_events(events, check_outputs):
    outputs = []
    requests = []
    changes = []
    reviewed_projects = set()
    reviewed_history = set()
    history_changes = full_history_changes(events)
    for index, (event, check_visible) in enumerate(zip(events, check_outputs)):
        request = copy.deepcopy(event["request"])
        visible = check_visible
        if harness.category(request) == "skill":
            visible = render_skill_read(request)
            changes.append({"event_index": index, "fields": ["visible.skill"]})
        elif request.get("tool") == "fr":
            args = request.get("args", [])
            payload = json.loads(visible)
            report = payload.get("result")
            require(payload.get("stdout_omitted_bytes") == 0, "Cannot project truncated fr stdout")
            if args[:1] in (["project"], ["author"]):
                require(isinstance(report, dict), "Project or author report is unstructured")
                basis = project_basis(report)
                report["context_basis"] = basis
                fields = ["result.context_basis"]
                if basis in reviewed_projects:
                    for field in PROJECT_FIELDS:
                        del report[field]
                    report["context_omitted"] = list(PROJECT_FIELDS)
                    request["args"] = [*args, "--context-basis", basis]
                    fields.extend([f"result.{field}" for field in (*PROJECT_FIELDS, "context_omitted")])
                else:
                    reviewed_projects.add(basis)
                visible = json.dumps(payload, ensure_ascii=False)
                changes.append({"event_index": index, "fields": fields})
            elif len(args) >= 3 and args[0] == "history" and args[1] in ("apply", "undo", "redo", "recover"):
                require(isinstance(report, dict), "History transition report is unstructured")
                transaction, action = report["transaction"], report["action"]
                complete = forward_changes(history_changes, transaction, action)
                require(complete is not None, "No full transition basis exists in the transcript")
                basis = history_basis(transaction, action, complete)
                fields = ["result.context_basis"]
                if basis in reviewed_history:
                    stripped = copy.deepcopy(complete)
                    for change in stripped:
                        change.pop("diff")
                    require(report["changes"] in (complete, stripped), "History completion differs from preview")
                    for field in HISTORY_FIELDS:
                        del report[field]
                    report.pop("diffs_omitted", None)
                    report["context_omitted"] = list(HISTORY_FIELDS)
                    request["args"] = [*args, "--context-basis", basis]
                    fields.extend([f"result.{field}" for field in (*HISTORY_FIELDS, "context_omitted", "diffs_omitted")])
                else:
                    if report["changes"] == complete:
                        reviewed_history.add(basis)
                report["context_basis"] = basis
                visible = json.dumps(payload, ensure_ascii=False)
                changes.append({"event_index": index, "fields": fields})
        outputs.append(visible)
        requests.append(request)
    return outputs, requests, changes


def category(request):
    if harness.category(request) == "skill":
        return "skill"
    if request.get("tool") == "fr":
        command = request.get("args", [None])[0]
        if command == "project":
            return "inspection"
        if command == "checks":
            return "checks"
        if command == "author":
            return "authoring"
        if command in ("history", "git"):
            return "delivery"
    if request.get("tool") in ("files", "read", "search"):
        return "inspection"
    if request.get("tool") == "write":
        return "authoring"
    return "delivery"


def sizes(texts, encoding):
    return {
        "bytes": sum(len(text.encode()) for text in texts),
        "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts)
        if encoding
        else None,
    }


def totals(prompt, events, outputs, requests, encoding):
    categories = {}
    for name in ("skill", "inspection", "checks", "authoring", "delivery"):
        categories[name] = sizes(
            [text for event, text in zip(events, outputs) if category(event["request"]) == name],
            encoding,
        )
        categories[name]["seconds"] = round(
            sum(event["elapsed_seconds"] for event in events if category(event["request"]) == name),
            3,
        )
    return {
        "context": sizes([prompt, *outputs], encoding),
        "prompt": sizes([prompt], encoding),
        "visible_output": sizes(outputs, encoding),
        "requests": sizes([json.dumps(request, ensure_ascii=False) for request in requests], encoding),
        "categories": categories,
        "tool_calls": len(events),
        "tool_seconds": round(sum(event["elapsed_seconds"] for event in events), 3),
    }


def comparison(trials, unit):
    values = {
        arm: [trial["projected"]["context"][unit] for trial in trials if trial["arm"] == arm]
        for arm in ("fr", "files")
    }
    require(all(len(samples) == 2 for samples in values.values()), "Need two complete pairs")
    means = {arm: sum(samples) / len(samples) for arm, samples in values.items()}
    difference = means["fr"] - means["files"]
    return {
        "fr_mean": means["fr"],
        "files_mean": means["files"],
        "fr_minus_files": difference,
        "fr_percent_difference": 100 * difference / means["files"],
    }


def measure(encoding):
    manifest = checks_policy.verify_evidence(EVIDENCE)
    trials = []
    skill_paths = set()
    for trial in manifest["trials"]:
        directory = EVIDENCE / trial
        config = json.loads((directory / "session.json").read_text())
        recorded = json.loads((directory / "result.json").read_text())
        require(recorded["passed"] is True, "Recorded trial did not pass")
        events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
        skill_paths.update(
            (ROOT / "skills/fr" / Path(*Path(event["request"]["path"]).parts[1:])).resolve()
            for event in events
            if harness.category(event["request"]) == "skill"
        )
        prompt = (directory / "prompt.txt").read_text()
        before_outputs = [event["visible"] for event in events]
        before_requests = [event["request"] for event in events]
        before = totals(prompt, events, before_outputs, before_requests, encoding)
        check_outputs, _ = checks_policy.project_events(events, "quiet_success_no_declarations")
        if config["arm"] == "fr":
            outputs, requests, changes = project_events(events, check_outputs)
        else:
            outputs, requests, changes = check_outputs, before_requests, []
        projected = totals(prompt, events, outputs, requests, encoding)
        require(len(outputs) == len(events) == len(requests), "Projection changed tool-call count")
        trials.append(
            {
                "trial": trial,
                "arm": config["arm"],
                "repetition": config["repetition"],
                "recorded_passed": True,
                "recorded": before,
                "projected": projected,
                "changed_events": changes,
            }
        )
    checks_policy.verify_evidence(EVIDENCE)
    units = ("bytes", "tokens") if encoding else ("bytes",)
    return {
        "schema": "fr-agent-context-protocol-projection-1",
        "passed": True,
        "evidence_manifest_sha256": harness.digest((EVIDENCE / "manifest.json").read_bytes()),
        "tokenizer": recorded["tokenizer"] if encoding else None,
        "trials": trials,
        "summary": {unit: comparison(trials, unit) for unit in units},
        "measurement_files": {
            str(path.relative_to(ROOT)): harness.digest(path.read_bytes())
            for path in [
                Path(__file__).resolve(),
                ROOT / "tools/checks-policy-context.py",
                ROOT / "tools/agent-eval.py",
                *sorted(skill_paths),
            ]
        },
        "scope": "Fixed-transcript projection with unchanged prompts, call count, outcomes, source states and timings. It applies the retained shared check policy, current requested skill files, and declared context-basis response and request fields. It does not predict autonomous adaptation, billed usage or latency changes. Original evidence and scores remain immutable.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(harness.tokenizer() if args.tokens else None), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
