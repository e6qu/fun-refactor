#!/usr/bin/env python3
"""Retain recursive flow evidence and isolated cold/warm/edit comparisons."""
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
from evidence_basis import file_digest

FIXTURE = ROOT / "tests/agent-eval/recursive-flow"
BINDINGS = ["src/project/flow_modules.rs", "sdk/python/src/fr_ir/flow_dependencies.py", "tools/recursive-flow-acceptance.py", "tools/evidence_basis.py", "src/project/dataflow.rs",
            "src/project/flow_summaries.rs", "src/project/control_flow.rs", "src/project/flow_cache.rs",
            "src/project/occurrence.rs", "src/span.rs", "src/parse.rs", "Cargo.lock",
            "sdk/python/src/fr_ir/flow.py", "sdk/python/src/fr_ir/flow_summaries.py",
            "sdk/python/src/fr_ir/runtime.py", "sdk/python/src/fr_ir/context.py",
            "sdk/python/src/fr_ir/investigation.py", "kernels/FrKernels/Flow.lean",
            "kernels/InvestigationMain.lean", *[f"tests/agent-eval/recursive-flow/{name}"
                for name in ["subject.py", "oracle.py", "task.json", "baseline.json"]]]
CASES = ["positive", "negative", "mutual", "effects", "unreachable", "exceptional", "sanitized", "separate", "unknown", "aliased"]


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def client_for(work, executable):
    return FrClient(work / "project", executable=str(executable.resolve()), max_output_bytes=1_048_576)


def analyze(client, work, name, cache):
    target = client.project("find", name).definition_target().handle
    return cache.analyze(client, target, rules=work / "rules.json", context="html",
                         steps=4096, summaries=True, max_bytes=1_048_576)


def worker(args):
    store = DirectoryObjectStore(args.work / "objects")
    manifest = args.work / "cache.txt"
    cache = FlowCache.restore(store, manifest.read_text()) if args.worker == "warm" else FlowCache(store)
    start = time.perf_counter()
    result = analyze(client_for(args.work, args.fr), args.work, "positive", cache)
    seconds = time.perf_counter() - start
    value = result.report.to_data()
    native_steps = value.pop("execution")["analysis_steps"]
    value.pop("context_basis", None)
    if args.worker == "cold":
        manifest.write_text(cache.persist())
    scale = 1 if sys.platform == "darwin" else 1024
    return {"evidence_digest": digest(encode(value)), "complete": value["complete"],
            "reused": result.reused, "analysis_steps": native_steps, "elapsed_seconds": seconds,
            "evidence_bytes": len(encode(value)), "witnesses": len(result.witnesses),
            "peak_native_child_rss_bytes": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale,
            "peak_worker_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
            "process_calls": 3, "source_reveals": 0, "tokens": None}


def measure(args):
    start = time.perf_counter()
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    oracle_seconds = time.perf_counter() - start
    with tempfile.TemporaryDirectory(prefix="fr-recursive-eval-") as directory:
        work = Path(directory)
        (work / "project").mkdir()
        shutil.copyfile(FIXTURE / "subject.py", work / "project/subject.py")
        (work / "rules.json").write_text(json.dumps({"version": "recursive-1", "sources": ["source"],
                                                   "sinks": ["sink"], "sanitizers": {"clean": "html"}}))
        client = client_for(work, args.fr)
        cases = {}
        for name in CASES:
            start = time.perf_counter()
            report = analyze(client, work, name, FlowCache(DirectoryObjectStore(work / "case-objects"))).report.to_data()
            seconds = time.perf_counter() - start
            selected = {key: value for key, value in report.items() if key not in {
                "events", "summaries", "control_flow", "coverage", "context_basis", "handle_prefix"}}
            cases[name] = {"report": selected, "evidence_digest": digest(encode(selected)),
                           "response_bytes": len(encode(report)), "elapsed_seconds": seconds,
                           "process_calls": 3, "source_reveals": 0, "tokens": None}
        arms = {}

        def arm(name, mode):
            arms[name] = json.loads(subprocess.check_output([sys.executable, str(Path(__file__).resolve()),
                "--worker", mode, "--work", str(work), "--fr", str(args.fr.resolve())]))

        arm("cold", "cold")
        arm("warm", "warm")
        (work / "project/unrelated.py").write_text("def other():\n    return 42\n")
        arm("unrelated_edit", "warm")
        arm("unrelated_clean", "clean")
        source = work / "project/subject.py"
        source.write_text(source.read_text().replace("return value", "return 0"))
        arm("helper_edit", "warm")
        arm("helper_clean", "clean")
    return {"schema": "fr-recursive-flow-acceptance-1", "repository_revision": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "source_bindings": {path: file_digest(ROOT / path) for path in BINDINGS},
            "binary_sha256": digest(args.fr.read_bytes()), "platform": platform.platform(),
            "oracle": oracle, "ordinary_ast_runtime_seconds": oracle_seconds,
            "baseline": json.loads((FIXTURE / "baseline.json").read_text()), "cases": cases, "arms": arms,
            "scope": "Deterministic scalar fixtures, exact AST call coordinates and 44 executed runtime cases. Whole-file cache reuse; no finer summary cache, feasible-path, runtime-termination, security or general speed claim. RSS records isolated worker and largest native child maxima. No live agent or billed tokens.",
            "false_claims": 0}


def audit(value):
    assert value["schema"] == "fr-recursive-flow-acceptance-1"
    assert value["source_bindings"] == {path: file_digest(ROOT / path) for path in BINDINGS}
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    assert value["oracle"] == oracle
    assert value["baseline"] == json.loads((FIXTURE / "baseline.json").read_text())
    assert value["false_claims"] == 0 and set(value["cases"]) == set(CASES)
    for name, case in value["cases"].items():
        report = case["report"]
        assert case["evidence_digest"] == digest(encode(report))
        assert report["complete"] == (name not in {"unknown", "aliased"})
        assert report["function_summaries"]["converged"]
        expected = name in {"positive", "mutual", "effects"}
        assert bool(report["witnesses"]) == expected, name
        for witness in report["witnesses"]:
            assert witness["trace"]["origin"].startswith("recursive-1:source:")
            for point in witness["trace"]["occurrences"]:
                assert point["revision"] == report["revision"]
                if point["role"] in {"source", "sink", "call-result", "summary-call"}:
                    span = point["location"]["span"]
                    assert [span["start"], span["end"]] in oracle["call_spans"]
        if name in {"unreachable", "exceptional"}:
            assert report["completion"]["normal_return"] is False
    arms = value["arms"]
    for a, b in [("cold", "warm"), ("unrelated_edit", "unrelated_clean"), ("helper_edit", "helper_clean")]:
        assert arms[a]["evidence_digest"] == arms[b]["evidence_digest"], (a, b)
    for name, arm in arms.items():
        assert arm["complete"] and arm["reused"] == (name in {"warm", "unrelated_edit"})
        assert (arm["analysis_steps"] == 0) == arm["reused"]
        assert bool(arm["witnesses"]) == (not name.startswith("helper_"))
        assert arm["elapsed_seconds"] >= 0 and arm["peak_native_child_rss_bytes"] > 0


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
