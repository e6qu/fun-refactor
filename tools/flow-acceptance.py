#!/usr/bin/env python3
"""Retain independent loop oracles and isolated cold/warm/edit measurements."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from fr_ir.context import DirectoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.runtime import FrClient

FIXTURE = ROOT / "tests/agent-eval/flow-fixed-point"
BINDINGS = ["tools/flow-acceptance.py", "src/project/dataflow.rs", "src/project/control_flow.rs",
            "src/project/flow_cache.rs", "sdk/python/src/fr_ir/flow.py", "kernels/FrKernels/Flow.lean",
            "tests/agent-eval/flow-fixed-point/subject.py", "tests/agent-eval/flow-fixed-point/oracle.py",
            "tests/agent-eval/flow-fixed-point/task.json", "tests/agent-eval/flow-fixed-point/diagnostics.json"]


def digest(value):
    return hashlib.sha256(value).hexdigest()


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


class MeasuredClient(FrClient):
    calls = 0
    response_bytes = 0

    def call(self, *arguments, input_bytes=None):
        result = super().call(*arguments, input_bytes=input_bytes)
        self.calls += 1
        self.response_bytes += len(encode(result.to_data()))
        return result


def worker(args):
    store = DirectoryObjectStore(args.work / "objects")
    manifest = args.work / "cache-root.txt"
    cache = FlowCache.restore(store, manifest.read_text()) if args.worker != "clean" and manifest.exists() else FlowCache(store)
    client = MeasuredClient(args.work / "project", executable=str(args.fr.resolve()), max_output_bytes=1_048_576)
    start = time.perf_counter()
    target = client.project("find", "delayed").definition_target().handle
    analysis = cache.analyze(client, target, rules=args.work / "rules.json", steps=1024, max_bytes=1_048_576)
    elapsed = time.perf_counter() - start
    report = analysis.report.to_data()
    native_steps = report["execution"]["analysis_steps"]
    del report["execution"]
    report.pop("context_basis", None)
    if args.worker == "cold":
        manifest.write_text(cache.persist())
    scale = 1 if sys.platform == "darwin" else 1024
    return {"reused": analysis.reused, "complete": report["complete"],
            "input_digest": report["input_digest"], "evidence_digest": digest(encode(report)),
            "witnesses": len(report["witnesses"]), "analysis_steps": native_steps,
            "elapsed_seconds": elapsed, "native_calls": client.calls,
            "native_response_bytes": client.response_bytes, "evidence_bytes": len(encode(report)),
            "peak_native_child_rss_bytes": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale,
            "peak_worker_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
            "tokens": None}


def measure(args):
    bindings = {path: digest((ROOT / path).read_bytes()) for path in BINDINGS}
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    oracle = subprocess.run([sys.executable, str(FIXTURE / "oracle.py")], capture_output=True, text=True, check=True)
    with tempfile.TemporaryDirectory(prefix="fr-flow-eval-") as temporary:
        work = Path(temporary)
        project = work / "project"
        project.mkdir()
        shutil.copyfile(FIXTURE / "subject.py", project / "subject.py")
        (work / "rules.json").write_text('{"version":"oracle-1","sources":["source"],"sinks":["sink"]}')
        arms = {}

        def arm(name, mode):
            output = subprocess.check_output([sys.executable, str(Path(__file__).resolve()), "--worker", mode,
                                              "--work", str(work), "--fr", str(args.fr.resolve())], text=True)
            arms[name] = json.loads(output)

        arm("cold", "cold")
        arm("warm", "warm")
        (project / "aaa.py").write_text("def unrelated():\n    return 42\n")
        arm("unrelated_edit", "warm")
        arm("unrelated_clean", "clean")
        source = project / "subject.py"
        source.write_text(source.read_text().replace("first = source()", "first = 0"))
        arm("relevant_edit", "warm")
        arm("relevant_clean", "clean")
    result = {"schema": "fr-flow-acceptance-1", "repository_revision": revision,
              "source_bindings": bindings, "binary_sha256": digest(args.fr.read_bytes()),
              "platform": platform.platform(), "python": platform.python_version(),
              "oracle": {"passed": True, "stdout": oracle.stdout.strip()}, "arms": arms,
              "scope": "One deterministic scalar Python fixture. Each arm uses an isolated SDK worker and native subprocesses. RSS is the worker maximum and largest native child, not allocation accounting. Times include discovery and validation. No live model, billed token or population claim.",
              "cache_policy": "Opt-in whole-file reuse. Parse/index and dependency validation still run; no claim that this small fixture runs faster."}
    audit(result)
    return result


def audit(value):
    assert value["schema"] == "fr-flow-acceptance-1"
    assert value["source_bindings"] == {path: digest((ROOT / path).read_bytes()) for path in BINDINGS}
    assert value["oracle"]["passed"]
    arms = value["arms"]
    for a, b in [("cold", "warm"), ("unrelated_edit", "unrelated_clean"), ("relevant_edit", "relevant_clean")]:
        assert arms[a]["evidence_digest"] == arms[b]["evidence_digest"], (a, b)
    for name, arm in arms.items():
        assert arm["complete"] and arm["native_calls"] == 3
        assert arm["reused"] == (name in {"warm", "unrelated_edit"})
        assert (arm["analysis_steps"] == 0) == arm["reused"]
        assert arm["elapsed_seconds"] >= 0 and arm["peak_native_child_rss_bytes"] > 0
        assert (arm["witnesses"] == 0) == name.startswith("relevant_")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--worker", choices=["cold", "warm", "clean"])
    parser.add_argument("--work", type=Path)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args)))
        return
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    text = json.dumps(value, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
