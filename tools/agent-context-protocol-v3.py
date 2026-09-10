#!/usr/bin/env python3
"""Project protocol v3 onto the frozen passing cohort with an exact change allowlist."""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
V2_REPORT = ROOT / "tests/agent-eval/context-protocol.json"
V2_REPORT_SHA256 = "0e0ed89063f4877e461cdad5e82b9fd91e567b3e69e21a275e3d9a7167bfb2b4"


def imported(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


v2 = imported("agent_context_protocol_v2_frozen", ROOT / "tools/agent-context-protocol.py")

ALLOWLIST = {
    "check-execution": ["$.result.checks", "$.result.declarations_omitted",
                        "$.result.results[].argv", "$.result.results[].covers",
                        "$.result.results[].cwd", "$.result.results[].stderr.omitted_bytes",
                        "$.result.results[].stderr.retained_bytes", "$.result.results[].stderr.text",
                        "$.result.results[].stdout.omitted_bytes",
                        "$.result.results[].stdout.retained_bytes", "$.result.results[].stdout.text"],
    "skill-read": ["$.text", "$.total_lines", "$.next_line"],
    "project-basis": ["$.result.context_basis"],
    "project-context": ["$.result.context_basis", "$.result.context_omitted",
                        "$.result.coverage", "$.result.handle_prefix", "$.result.revision"],
    "author-context": ["$.result.context_basis", "$.result.context_omitted",
                       "$.result.coverage", "$.result.handle_prefix", "$.result.revision",
                       "$.result.transaction_context_basis"],
    "patch-artifact": ["$.result.output", "$.result.patch", "$.result.patch_bytes",
                       "$.result.patch_sha256"],
    "redo-preview": ["$.result.changes[].diff", "$.result.changes[].diff_bytes",
                     "$.result.context_basis", "$.result.context_omitted"],
    "redo-completion": ["$.result.changes[].diff_bytes", "$.result.context_basis",
                        "$.result.context_omitted", "$.result.diffs_omitted"],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def projection_transaction_bases(events):
    """Return a v3-shaped opaque identity; frozen reports lack exact after snapshots."""
    bases = {}
    for transaction, basis in ORIGINAL_TRANSACTION_BASES(events).items():
        bases[transaction] = "frtb2:" + basis.split(":", 1)[1]
    return bases


ORIGINAL_TRANSACTION_BASES = v2.transaction_bases


def tree_diff(before, after, path="$"):
    if type(before) is not type(after):
        return [path]
    if isinstance(before, dict):
        changed = []
        for key in sorted(before.keys() | after.keys()):
            child = f"{path}.{key}"
            if key not in before or key not in after:
                changed.append(child)
            else:
                changed.extend(tree_diff(before[key], after[key], child))
        return changed
    if isinstance(before, list):
        if len(before) != len(after):
            return [path + ".length"]
        changed = []
        for index, (left, right) in enumerate(zip(before, after)):
            changed.extend(tree_diff(left, right, f"{path}[{index}]"))
        return changed
    return [] if before == after else [path]


def generalized(paths):
    return [re.sub(r"\[\d+\]", "[]", path) for path in paths]


def appended(original, projected, suffix):
    return projected == [*original, *suffix]


def audit_event(index, event, visible, request):
    output_paths = generalized(tree_diff(json.loads(event["visible"]), json.loads(visible)))
    original_request = event["request"]
    request_paths = tree_diff(original_request, request)
    if not output_paths and not request_paths:
        return None

    projected = json.loads(visible)
    result = projected.get("result", {})
    tool = original_request.get("tool")
    args = original_request.get("args", [])
    suffix = []
    if v2.harness.category(original_request) == "skill":
        rule = "skill-read"
        require(not request_paths, "Skill projection changed its request")
    elif tool == "fr" and args[:1] in (["project"], ["author"]):
        if args[:1] == ["author"]:
            rule = "author-context"
        elif "context_omitted" in result:
            rule = "project-context"
        else:
            rule = "project-basis"
        if "context_omitted" in result:
            suffix = ["--context-basis", result["context_basis"]]
    elif tool == "fr" and args[:2] == ["history", "patch"]:
        rule = "patch-artifact"
        suffix = ["--output", "../artifacts/change.patch"]
    elif tool == "fr" and args[:2] == ["history", "redo"]:
        rule = "redo-completion" if result.get("applied") is True else "redo-preview"
        suffix = ["--context-basis", result["context_basis"]]
    elif tool == "fr" and args[:1] == ["checks"]:
        rule = "check-execution"
    else:
        raise ValueError(f"Event {index} changed outside the allowlist: {args}")

    allowed = set(ALLOWLIST[rule])
    require(set(output_paths) <= allowed and output_paths, f"Event {index} changed {output_paths}")
    require(
        (not suffix and not request_paths)
        or (request_paths == ["$.args.length"]
            and appended(original_request["args"], request["args"], suffix)),
        f"Event {index} changed its request beyond {suffix}",
    )
    return {
        "event_index": index,
        "rule": rule,
        "output_paths": output_paths,
        "request_suffix": suffix,
        "recorded_request_sha256": digest(json.dumps(original_request, ensure_ascii=False).encode()),
        "projected_request_sha256": digest(json.dumps(request, ensure_ascii=False).encode()),
        "recorded_visible_sha256": digest(event["visible"].encode()),
        "projected_visible_sha256": digest(visible.encode()),
    }


def applicability(events):
    exact = []
    for index, event in enumerate(events):
        request = event["request"]
        args = request.get("args", [])
        if request.get("tool") == "fr" and args[:2] == ["project", "find"] and "--contains" not in args:
            exact.append((index, args[2], tuple(args[3:])))
    groups = []
    for options in sorted({item[2] for item in exact}):
        members = [(index, name) for index, name, found_options in exact if found_options == options]
        if len(members) > 1:
            groups.append({"options": list(options), "events": [item[0] for item in members],
                           "names": [item[1] for item in members]})

    previews = []
    saves = []
    for index, event in enumerate(events):
        request = event["request"]
        args = request.get("args", [])
        if request.get("tool") != "fr" or args[:1] != ["author"]:
            continue
        if "--save-plan" in args or "--write" in args:
            saves.append(index)
        else:
            previews.append(index)
    return {
        "multi_select": {"eligible_groups": groups, "projected_savings": 0,
                         "reason": "No two exact lookups share the same scope and options."},
        "plan_basis": {"preview_events": previews, "persistence_events": saves,
                       "eligible_pairs": [], "projected_savings": 0,
                       "reason": "The transcript has no reviewed author preview before persistence."},
    }


def mean(values):
    return sum(values) / len(values)


def measure(encoding):
    require(digest(V2_REPORT.read_bytes()) == V2_REPORT_SHA256, "Frozen v2 projection changed")
    baseline = json.loads(V2_REPORT.read_text())
    require(baseline["passed"] is True, "Frozen v2 projection did not pass")

    current_v2 = v2.measure(encoding)
    v2.transaction_bases = projection_transaction_bases
    try:
        projected = v2.measure(encoding)
    finally:
        v2.transaction_bases = ORIGINAL_TRANSACTION_BASES

    by_name = {trial["trial"]: trial for trial in baseline["trials"]}
    current_by_name = {trial["trial"]: trial for trial in current_v2["trials"]}
    audits = {}
    applicable = {}
    manifest = v2.checks_policy.verify_evidence(v2.EVIDENCE)
    for trial in manifest["trials"]:
        directory = v2.EVIDENCE / trial
        events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
        check_outputs, _ = v2.checks_policy.project_events(events, "quiet_success_no_declarations")
        if "-fr-" in trial:
            v2.transaction_bases = projection_transaction_bases
            try:
                outputs, requests, _ = v2.project_events(events, check_outputs)
            finally:
                v2.transaction_bases = ORIGINAL_TRANSACTION_BASES
        else:
            outputs = check_outputs
            requests = [event["request"] for event in events]
        entries = [audit_event(index, event, visible, request)
                   for index, (event, visible, request) in enumerate(zip(events, outputs, requests))]
        audits[trial] = [entry for entry in entries if entry is not None]
        applicable[trial] = applicability(events) if "-fr-" in trial else None

    units = ("bytes", "tokens") if encoding else ("bytes",)
    for trial in projected["trials"]:
        name = trial["trial"]
        trial["allowlist_audit"] = audits[name]
        trial["feature_applicability"] = applicable[name]
        trial["contribution"] = {}
        for unit in units:
            frozen = by_name[name]["projected"]["context"][unit]
            current = current_by_name[name]["projected"]["context"][unit]
            final = trial["projected"]["context"][unit]
            trial["contribution"][unit] = {
                "skill_and_current_docs": frozen - current,
                "transaction_basis_v2": current - final,
                "multi_select": 0,
                "plan_basis": 0,
                "total": frozen - final,
            }

    fr = [trial for trial in projected["trials"] if trial["arm"] == "fr"]
    contributions = {
        unit: {name: mean([trial["contribution"][unit][name] for trial in fr])
               for name in ("skill_and_current_docs", "transaction_basis_v2", "multi_select",
                            "plan_basis", "total")}
        for unit in units
    }
    projected.update({
        "schema": "fr-agent-context-protocol-projection-2",
        "baseline": {"path": str(V2_REPORT.relative_to(ROOT)), "sha256": V2_REPORT_SHA256,
                     "schema": baseline["schema"], "summary": baseline["summary"]},
        "allowlist": ALLOWLIST,
        "contributions": contributions,
        "feature_applicability": {
            "multi_select_eligible_groups": sum(len(value["multi_select"]["eligible_groups"])
                                                for value in applicable.values() if value),
            "plan_basis_eligible_pairs": sum(len(value["plan_basis"]["eligible_pairs"])
                                             for value in applicable.values() if value),
        },
        "measurement_files": {
            **projected["measurement_files"],
            str(Path(__file__).resolve().relative_to(ROOT)): digest(Path(__file__).read_bytes()),
            str(V2_REPORT.relative_to(ROOT)): V2_REPORT_SHA256,
        },
        "scope": "Checksum-bound fixed-transcript projection over four passing trials. Prompts, calls, outcomes, source states and timings stay fixed. Every changed request and response path is audited against the embedded allowlist. The frtb2 body is projection-only because frozen visible reports omit after-snapshot bytes; only its production prefix and length are measured. Multi-select and reviewed-plan compaction are reported with zero savings because this cohort has no eligible calls. This is serialization evidence, not a new agent outcome, latency estimate or billed-usage claim.",
    })
    return projected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(v2.harness.tokenizer() if args.tokens else None),
                     indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
