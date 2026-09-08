#!/usr/bin/env python3
"""Project matching check-output policies onto the frozen coordinated cohort."""

import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-08-coordinated"
POLICIES = ("quiet_success", "quiet_success_no_declarations")
DECLARATION_FIELDS = ("argv", "cwd", "covers")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def project(report, listing, policy):
    require(policy in POLICIES, "Unknown check-output policy")
    require(report.get("schema") == "fr-checks-1" and report.get("executed") is True,
            "Projection requires an executed check report")
    require(listing is not None and listing.get("executed") is False
            and listing.get("schema") == "fr-checks-1", "A prior check listing is required")
    require(all(report[key] == listing[key] for key in ("basis", "root", "configuration")),
            "Execution and reviewed listing have different bases or locations")
    declarations = {check["name"]: check for check in listing["checks"]}
    require(len(declarations) == len(listing["checks"]), "Duplicate check declarations")
    result = copy.deepcopy(report)
    omitted = result.pop("declarations_omitted", False)
    require(type(omitted) is bool, "Invalid declaration omission marker")
    if omitted:
        require("checks" not in result, "Omitted report contains declarations")
        result["checks"] = copy.deepcopy(listing["checks"])
    else:
        require(result["checks"] == listing["checks"], "Execution declarations differ from listing")
    names = [check["name"] for check in result["results"]]
    require(names and len(set(names)) == len(names) and set(names) <= declarations.keys(),
            "Execution has missing, repeated or undeclared check names")
    require(result["not_run"] == [name for name in declarations if name not in names],
            "Unexecuted check declarations differ from listing")
    for check in result["results"]:
        declaration = declarations[check["name"]]
        for key in DECLARATION_FIELDS:
            if omitted:
                require(key not in check, "Omitted result contains a declaration")
                check[key] = copy.deepcopy(declaration[key])
            else:
                require(check[key] == declaration[key], "Result declaration differs from listing")
        require(type(check["passed"]) is bool, "Invalid check outcome")
        if check["passed"]:
            require(check["exit_code"] == 0 and check["error"] is None
                    and check["timed_out"] is False and check["output_limit_exceeded"] is False,
                    "Successful check has contradictory failure evidence")
            for name in ("stdout", "stderr"):
                stream = check[name]
                require(all(type(stream[key]) is int and stream[key] >= 0
                            for key in ("retained_bytes", "omitted_bytes")), "Invalid stream byte counts")
                stream["omitted_bytes"] += stream["retained_bytes"]
                stream["retained_bytes"] = 0
                stream["text"] = ""
    require(result["passed"] is all(check["passed"] for check in result["results"]),
            "Aggregate outcome differs from check outcomes")
    if policy == "quiet_success_no_declarations":
        del result["checks"]
        for check in result["results"]:
            for key in DECLARATION_FIELDS:
                del check[key]
        result["declarations_omitted"] = True
    return json.loads(json.dumps(result, sort_keys=True, ensure_ascii=False))


def project_events(events, policy):
    outputs, changes = [], []
    listing = None
    for index, event in enumerate(events):
        visible = event["visible"]
        request = event["request"]
        if request.get("tool") == "fr" and request.get("args", [])[:1] == ["checks"]:
            payload = json.loads(visible)
            report = payload.get("result")
            require(isinstance(report, dict) and report.get("schema") == "fr-checks-1"
                    and payload.get("stdout_omitted_bytes") == 0,
                    "Check payload is missing, truncated or unstructured")
            if report["executed"]:
                projected = project(report, listing, policy)
                if projected != report:
                    payload["result"] = projected
                    visible = json.dumps(payload, ensure_ascii=False)
                changes.append({"event_index": index,
                                "recorded_visible_sha256": harness.digest(event["visible"].encode()),
                                "projected_visible_sha256": harness.digest(visible.encode()),
                                "projected_visible": visible})
            else:
                listing = report
        outputs.append(visible)
    require(changes, "Transcript contains no check executions")
    return outputs, changes


def sizes(texts, encoding):
    return {"bytes": sum(len(text.encode()) for text in texts),
            "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts) if encoding else None}


def totals(prompt, events, outputs, encoding):
    check_outputs = [text for event, text in zip(events, outputs) if harness.category(event["request"]) == "checks"]
    listings = [text for text in check_outputs if json.loads(text)["result"]["executed"] is False]
    executions = [text for text in check_outputs if json.loads(text)["result"]["executed"] is True]
    return {"context": sizes([prompt, *outputs], encoding), "visible_output": sizes(outputs, encoding),
            "checks": sizes(check_outputs, encoding),
            "check_listings": sizes(listings, encoding), "check_executions": sizes(executions, encoding),
            "tool_calls": len(events),
            "requests": sizes([json.dumps(event["request"], ensure_ascii=False) for event in events], encoding)}


def comparison(trials, policy, unit):
    values = {arm: [trial["policies"][policy]["context"][unit] for trial in trials if trial["arm"] == arm]
              for arm in ("fr", "files")}
    require(all(len(arm) == 2 and all(value is not None for value in arm) for arm in values.values()),
            "Comparison requires two complete repetitions per arm")
    means = {arm: sum(samples) / len(samples) for arm, samples in values.items()}
    difference = means["fr"] - means["files"]
    return {"fr_mean": means["fr"], "files_mean": means["files"],
            "fr_minus_files": difference,
            "fr_percent_difference": 100 * difference / means["files"]}


def verify_evidence(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    expected = [name for name, _, _, _ in harness.trial_names("regex-coordinated", 2)]
    require(manifest["trials"] == expected, "Projection requires the complete coordinated cohort")
    for name, sha in manifest["files"].items():
        require(harness.digest(harness.within(directory, name).read_bytes()) == sha,
                f"Recorded evidence checksum mismatch: {name}")
    for trial in expected:
        for name in ("prompt.txt", "events.jsonl", "result.json", "session.json"):
            require(f"{trial}/{name}" in manifest["files"], "Projection input is absent from manifest")
    return manifest


def live_check(binary):
    with tempfile.TemporaryDirectory(prefix="fr-checks-policy-") as tmp:
        root = Path(tmp)
        (root / ".fr").mkdir()
        checks = [{"name": name, "argv": ["python3", "-c", program], "cwd": ".",
                   "timeout_seconds": 5, "covers": ["fixture stream and exit behavior"]}
                  for name, program in (
                      ("pass", "import os; os.write(1, b'\\xffok'); os.write(2, b'ok')"),
                      ("fail", "import os; os.write(1, b'diagnostic'); os.write(2, b'\\xffbad'); raise SystemExit(7)"))]
        harness.save(root / ".fr/checks.json", {"schema": 1, "checks": checks})
        original = (root / ".fr/checks.json").read_bytes()

        def run(*args):
            process = subprocess.run([str(binary), "--json", "-C", str(root), "checks", *args],
                                     capture_output=True, timeout=30)
            require(process.returncode == (1 if args else 0) and not process.stderr,
                    "Live fixture did not return its expected mixed outcome")
            return json.loads(process.stdout)

        def stable(report):
            result = copy.deepcopy(report)
            for check in result["results"]:
                del check["elapsed_ms"]
            return result

        listing = run()
        args = ("--run", "pass,fail", "--basis", listing["basis"], "--output-bytes", "3")
        full = run(*args)
        reports = {}
        for policy in POLICIES:
            flags = ("--quiet-success",) + (("--no-declarations",) if policy.endswith("no_declarations") else ())
            actual = run(*args, *flags)
            projected = project(full, listing, policy)
            require(stable(projected) == stable(actual), "Projection differs from live CLI policy")
            require(project(actual, listing, policy) == actual, "Projection is not idempotent")
            reports[policy] = actual
        require((root / ".fr/checks.json").read_bytes() == original, "Fixture configuration changed")
        return {"passed": True, "listing": listing, "recorded_full": full, "actual_policies": reports,
                "comparison": "Exact report equality except elapsed_ms; failed streams retain their byte budgets and replacement decoding."}


def measure(binary, encoding):
    manifest = verify_evidence(EVIDENCE)
    binary_sha = harness.digest(binary.read_bytes())
    trials = []
    for trial in manifest["trials"]:
        directory = EVIDENCE / trial
        recorded = json.loads((directory / "result.json").read_text())
        config = json.loads((directory / "session.json").read_text())
        require(recorded["passed"] is True, "Recorded trial did not pass acceptance")
        events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
        prompt = (directory / "prompt.txt").read_text()
        before = totals(prompt, events, [event["visible"] for event in events], encoding)
        if encoding:
            require(before["context"]["tokens"] == recorded["context_tokens"]
                    and before["visible_output"]["tokens"] == recorded["visible_output_tokens"]
                    and before["requests"]["tokens"] == recorded["tool_request_tokens"],
                    "Retokenization differs from the original score")
        policies = {}
        for policy in POLICIES:
            outputs, changes = project_events(events, policy)
            require(len(changes) == 4, "Coordinated trial must retain four check executions")
            projected = totals(prompt, events, outputs, encoding)
            for unit in ("bytes", "tokens"):
                if projected["context"][unit] is not None:
                    require(projected["context"][unit] - before["context"][unit]
                            == projected["checks"][unit] - before["checks"][unit],
                            "Projection changed context outside check output")
                    require(projected["visible_output"][unit] - before["visible_output"][unit]
                            == projected["checks"][unit] - before["checks"][unit],
                            "Projection changed visible output outside checks")
            policies[policy] = {**projected, "executions": changes}
        trials.append({"trial": trial, "arm": config["arm"], "repetition": config["repetition"],
                       "recorded": before, "policies": policies})
    live = live_check(binary)
    require(harness.digest(binary.read_bytes()) == binary_sha, "Binary changed during measurement")
    require(verify_evidence(EVIDENCE) == manifest, "Evidence manifest changed during measurement")
    return {"schema": "fr-checks-policy-context-1", "passed": True,
            "evidence_manifest_sha256": harness.digest((EVIDENCE / "manifest.json").read_bytes()),
            "binary_sha256": binary_sha,
            "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in
                                  (Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "src/checks.rs")},
            "tokenizer": recorded["tokenizer"] if encoding else None,
            "trials": trials,
            "summary": {unit: {policy: comparison(trials, policy, unit) for policy in POLICIES}
                        for unit in (("bytes", "tokens") if encoding else ("bytes",))},
            "live": live,
            "scope": "Fixed-transcript projection, not new agent outcomes. Prompts, requests, listings, non-check payloads and call counts stay unchanged. Only executed check payloads change. Both policies suppress successful streams; one also omits declarations after a matching prior listing. Existing failure output budgets, outcomes, timings and stream totals stay intact. The cohort has no failed checks. No output can be reconstructed after harness truncation. Original evidence and scores remain immutable. No latency or billed-token claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(args.fr.resolve(strict=True), harness.tokenizer() if args.tokens else None),
                     indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
