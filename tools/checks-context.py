#!/usr/bin/env python3
"""Measure declaration omission on live workspace checks and frozen trial payloads."""

import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-08-regex"


def omit(report):
    result = copy.deepcopy(report)
    del result["checks"]
    for check in result["results"]:
        for key in ("argv", "cwd", "covers"):
            del check[key]
    result["declarations_omitted"] = True
    return dict(sorted(result.items()))


def restore(report, listing):
    result = copy.deepcopy(report)
    assert result.pop("declarations_omitted") is True
    assert result["basis"] == listing["basis"]
    result["checks"] = listing["checks"]
    declarations = {check["name"]: check for check in listing["checks"]}
    for check in result["results"]:
        for key in ("argv", "cwd", "covers"):
            check[key] = declarations[check["name"]][key]
    return result


def run(binary, root, *args):
    env = os.environ.copy()
    env.update(CARGO_HOME=str(ROOT / "target/cargo-home"), CARGO_NET_OFFLINE="true")
    process = subprocess.run([str(binary), "-C", str(root), "--json", "checks", *args],
                             env=env, capture_output=True, timeout=300)
    if process.returncode:
        raise RuntimeError(process.stdout.decode() + process.stderr.decode())
    return process.stdout.decode(), json.loads(process.stdout)


def stable(report):
    result = copy.deepcopy(report)
    for check in result["results"]:
        del check["elapsed_ms"]
        for stream in ("stdout", "stderr"):
            assert check[stream]["retained_bytes"] == 0 and check[stream]["text"] == ""
            assert isinstance(check[stream]["omitted_bytes"], int)
            del check[stream]["omitted_bytes"]
    return result


def sizes(outputs, encoding):
    return {"bytes": sum(len(text.encode()) for text in outputs),
            "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in outputs) if encoding else None}


def measure(binary, encoding):
    binary_sha = harness.digest(binary.read_bytes())
    manifest = json.loads((EVIDENCE / "manifest.json").read_text())
    for name, sha in manifest["files"].items():
        assert harness.digest(harness.within(EVIDENCE, name).read_bytes()) == sha, name
    with tempfile.TemporaryDirectory(prefix="fr-checks-context-") as tmp:
        root = Path(tmp) / "project"
        harness.unpack(root, harness.regex_workspace.TASK)
        (root / ".fr").mkdir(exist_ok=True)
        harness.save(root / ".fr/checks.json", {"schema": 1, "checks": harness.regex_workspace.CHECKS})
        harness.initialize(root)
        original = harness.snapshot(root)
        original_index = (root / ".git/index").read_bytes()
        listing_text, listing = run(binary, root)
        assert listing["executed"] is False and listing["passed"] is None
        args = ("--run", "upstream,minimal", "--basis", listing["basis"], "--quiet-success")
        run(binary, root, *args)
        full_text, full = run(binary, root, *args)
        compact_text, compact = run(binary, root, *args, "--no-declarations")
        assert full["passed"] and compact["passed"]
        assert [check["name"] for check in compact["results"]] == ["upstream", "minimal"]
        assert stable(restore(compact, listing)) == stable(full)
        assert stable(compact) == stable(omit(full))
        assert harness.snapshot(root) == original
        assert (root / ".git/index").read_bytes() == original_index
        live = {"passed": True, "listing_stdout": listing_text,
                "reports": {"quiet_success": full_text, "quiet_success_no_declarations": compact_text},
                "measures": {"quiet_success": sizes([full_text], encoding),
                             "quiet_success_no_declarations": sizes([compact_text], encoding)},
                "comparison": "Rejoined declarations match; elapsed times and successful omitted-stream byte counts may vary between executions.",
                "source_and_index_unchanged": True}
        assert live["measures"]["quiet_success_no_declarations"]["bytes"] < live["measures"]["quiet_success"]["bytes"]
    projected = []
    for trial in manifest["trials"]:
        if "-fr-" not in trial:
            continue
        events = [json.loads(line) for line in (EVIDENCE / trial / "events.jsonl").read_text().splitlines()]
        original_payloads, smaller_payloads = [], []
        listing = None
        for event in events:
            payload = json.loads(event["visible"])
            report = payload.get("result")
            if not isinstance(report, dict) or report.get("schema") != "fr-checks-1":
                continue
            if not report["executed"]:
                listing = report
                continue
            assert listing is not None
            original_payloads.append(event["visible"])
            payload["result"] = omit(report)
            assert restore(payload["result"], listing) == report
            smaller_payloads.append(json.dumps(payload, ensure_ascii=False))
        assert len(original_payloads) == len(smaller_payloads) == 4
        before, after = sizes(original_payloads, encoding), sizes(smaller_payloads, encoding)
        assert after["bytes"] < before["bytes"]
        projected.append({"trial": trial, "executions": 4, "recorded": before, "projected": after})
    assert harness.digest(binary.read_bytes()) == binary_sha, "Binary changed during measurement"
    return {"schema": "fr-checks-context-1", "passed": True,
            "binary_sha256": binary_sha,
            "archive_sha256": harness.regex_workspace.ARCHIVE_SHA,
            "dependency_lock_sha256": harness.regex_workspace.LOCK_SHA,
            "evidence_manifest_sha256": harness.digest((EVIDENCE / "manifest.json").read_bytes()),
            "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in
                                  (Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "tools/agent_eval/regex_workspace.py")},
            "tokenizer": json.loads((EVIDENCE / manifest["trials"][0] / "result.json").read_text())["tokenizer"] if encoding else None,
            "live": live, "projected_execution_payloads": projected,
            "scope": "Controlled check-report measurement and deterministic frozen-payload projection; no autonomous agents or total-task context measurement. Live runs use a pristine workspace and a warm-up execution. Projection preserves original timing and stream counts; listing, prompts and other task outputs are excluded."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    result = measure(args.fr.resolve(strict=True), harness.tokenizer() if args.tokens else None)
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
