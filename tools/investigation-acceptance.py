#!/usr/bin/env python3
"""Deterministic unknown-target diagnosis, reviewed delivery and independent replay."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient


def run(arguments, cwd, *, check=True):
    return subprocess.run(arguments, cwd=cwd, check=check, capture_output=True, text=True)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = ROOT / "tests/agent-eval/investigation"
    records = []
    with tempfile.TemporaryDirectory(prefix="fr-investigation-") as temporary:
        project = Path(temporary) / "project"
        project.mkdir()
        shutil.copy(fixture / "catalog.py", project / "catalog.py")
        shutil.copy(fixture / "oracle.py", project / "oracle.py")
        (project / ".fr").mkdir()
        (project / "artifacts").mkdir()
        checks = {"schema": 1, "checks": [{"name": "syntax", "argv": [sys.executable, "-c",
            "import ast; ast.parse(open('catalog.py').read())"], "cwd": ".", "timeout_seconds": 20,
            "covers": ["Python parse; independent behavior checked separately"]}]}
        (project / ".fr/checks.json").write_text(json.dumps(checks))
        run(["git", "init", "-q"], project)
        run(["git", "add", "."], project)
        run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "pinned baseline"], project)
        client = FrClient(project, executable=str(args.fr.resolve()), max_output_bytes=1048576)
        baseline = run([sys.executable, "oracle.py", "catalog.py"], project, check=False)
        assert baseline.returncode != 0, "bug reproducer unexpectedly passes"
        # The task supplies the public symptom. Discover callees and inspect semantic expressions.
        start = time.perf_counter()
        calls = client.project("calls", ".", "--limit", "16")
        semantic = client.project("semantic", "catalog.py", "--body", "--nodes", "128")
        def has_subtraction(value):
            if isinstance(value, dict):
                return value.get("op") == "sub" or any(has_subtraction(v) for v in value.values())
            return isinstance(value, list) and any(has_subtraction(v) for v in value)
        candidates = [item["value"]["name"] for item in semantic.at("/model/items") if has_subtraction(item)]
        assert len(candidates) == 1
        target = client.project("find", candidates[0]).definition_target()
        body = "if member:\n    return max(0, amount - 10)\nreturn amount"
        change = TaskChange([], [TaskTarget("repair", target.handle, "replace-body", fragment=body)],
                            {"files-changed": 1, "edits": 1, "paths-changed": ["catalog.py"]}, ["syntax"],
                            TaskDelivery(patch="artifacts/bug.patch"))
        review = client.review(change)
        assert review.at("/ready")
        result = client.execute(review)
        assert result.passed
        run([sys.executable, "oracle.py", "catalog.py"], project)
        records.append({"task": "bug", "discovered_target": candidates[0], "calls": calls.to_data(),
                        "semantic": semantic.to_data(), "review": review.to_data(), "delivery": result.to_data(),
                        "context_bytes": len(json.dumps(calls.to_data()).encode()) + len(json.dumps(semantic.to_data()).encode()),
                        "seconds": time.perf_counter() - start, "independent_oracle": "passed"})
        # Discover the insertion anchor from the public checkout entry after the first revision.
        target_handle = client.project("map", "catalog.py", "--depth", "0", "--fields", "handle,kind").at("/rows/0/0")
        fragment = 'def quote(price, count, member):\n    return {"total": checkout(price, count, member), "currency": "USD"}\n'
        change = TaskChange([], [TaskTarget("feature", target_handle, "insert-declaration", fragment=fragment)],
                            {"files-changed": 1, "edits": 1, "paths-changed": ["catalog.py"]}, ["syntax"],
                            TaskDelivery(patch="artifacts/feature.patch"))
        review = client.review(change)
        assert review.at("/ready")
        result = client.execute(review)
        assert result.passed
        run([sys.executable, "oracle.py", "catalog.py", "--feature"], project)
        records.append({"task": "feature", "review": review.to_data(), "delivery": result.to_data(),
                        "independent_oracle": "passed"})
        receiver = Path(temporary) / "receiver"
        receiver.mkdir()
        shutil.copy(fixture / "catalog.py", receiver / "catalog.py")
        shutil.copy(fixture / "oracle.py", receiver / "oracle.py")
        run(["git", "init", "-q"], receiver)
        for name in ["bug", "feature"]:
            patch = project / f"artifacts/{name}.patch"
            run(["git", "apply", "--check", str(patch)], receiver)
            run(["git", "apply", str(patch)], receiver)
        run([sys.executable, "oracle.py", "catalog.py", "--feature"], receiver)
        measurements = []
        answers = []
        for label, no_cache in [("cold", True), ("warm", False), ("single-edit", False), ("clean-rebuild", True)]:
            if label == "single-edit":
                with (receiver / "catalog.py").open("a") as stream:
                    stream.write("\n# unrelated trailing source comment\n")
            command = [str(args.fr.resolve()), "--json", "-C", str(receiver)]
            if no_cache:
                command.append("--no-cache")
            command.extend(["project", "calls", ".", "--limit", "16"])
            began = time.perf_counter()
            completed = run(command, receiver)
            measurements.append({"run": label, "seconds": time.perf_counter() - began,
                                 "context_bytes": len(completed.stdout.encode())})
            answers.append(json.loads(completed.stdout))
        assert answers[0] == answers[1], "warm result differs from clean analysis"
        assert answers[2] == answers[3], "single-edit result differs from clean rebuild"
        args.output.mkdir(parents=True, exist_ok=True)
        for name in ["bug", "feature"]:
            shutil.copy(project / f"artifacts/{name}.patch", args.output / f"{name}.patch")
        (args.output / "result.json").write_text(json.dumps({"schema": "fr-investigation-acceptance-1",
            "cohort": "deterministic replay; no live-agent or population claim", "baseline_bug_failed": True,
            "receiver_oracle_passed": True, "records": records,
            "cache_comparison": {"measurements": measurements, "cold_warm_equal": True,
                                 "edit_clean_equal": True, "memory": None, "tokens": None,
                                 "scope": "tiny pinned fixture; existing index cache, no new incremental flow cache"}}, indent=2) + "\n")
        manifest = {"schema": "fr-investigation-manifest-1", "repository_revision": run(["git", "rev-parse", "HEAD"], ROOT).stdout.strip(),
                    "evaluator_sha256": sha(Path(__file__).read_bytes()),
                    "fixture_sha256": {p.name: sha(p.read_bytes()) for p in fixture.iterdir() if p.is_file()},
                    "artifacts": {p.name: sha(p.read_bytes()) for p in args.output.iterdir() if p.is_file() and p.name != "manifest.json"}}
        (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        print(json.dumps({"passed": True, "output": str(args.output)}))


if __name__ == "__main__":
    main()
