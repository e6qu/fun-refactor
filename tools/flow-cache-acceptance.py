#!/usr/bin/env python3
"""Compare retained report storage with the pinned per-node cache implementation."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/agent-eval/flow-cache"
sys.path[:0] = [str(ROOT / "sdk/python/src"), str(FIXTURE)]
from corpus import CASES, SCENARIOS, install, mutate, oracle, sources
from fr_ir.context import DirectoryObjectStore, restore_stored_value
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrClient, FrReport

BINDINGS = ["tools/flow-cache-acceptance.py", "sdk/python/src/fr_ir/flow.py",
            "sdk/python/src/fr_ir/flow_storage.py", "sdk/python/src/fr_ir/context.py",
            "sdk/python/src/fr_ir/flow_dependencies.py",
            "sdk/python/src/fr_ir/ir.py", "sdk/python/src/fr_ir/runtime.py",
            "src/project/dataflow.rs", "src/project/flow_cache.rs", "src/project/flow_summaries.rs",
            "src/project/flow_modules.rs", "src/project/control_flow.rs", "src/project/occurrence.rs",
            "src/project.rs", "src/project/manifests.rs", "src/project/lockfiles.rs",
            "src/span.rs", "src/parse.rs", "src/scan.rs", "src/index.rs", "src/extract.rs",
            "Cargo.toml", "Cargo.lock"] + [f"tests/agent-eval/flow-cache/{name}" for name in
                ("corpus.py", "task.json", "baseline_flow.py", "storage-probe.py", "storage-probe.json")]
STRATEGIES = ("legacy", "chunks")
FIELDS = ("elapsed_seconds", "native_seconds", "store_seconds", "context_bytes", "native_calls",
          "analysis_steps", "worker_peak_rss_bytes", "native_peak_rss_bytes", "store_gets", "store_puts",
          "stored_files", "stored_bytes", "added_files", "added_bytes")


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def bindings():
    return {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in BINDINGS}


def legacy_class():
    spec = importlib.util.spec_from_file_location("fr_ir._baseline_flow", FIXTURE / "baseline_flow.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module.FlowCache


def inventory(directory):
    files = list(directory.rglob("*.json"))
    return len(files), sum(path.stat().st_size for path in files)


class MeasuredStore:
    def __init__(self, root):
        self.store = DirectoryObjectStore(root)
        self.gets = self.puts = 0
        self.seconds = 0.0

    def get(self, key):
        self.gets += 1
        start = time.perf_counter()
        try:
            return self.store.get(key)
        finally:
            self.seconds += time.perf_counter() - start

    def put(self, key, value):
        self.puts += 1
        start = time.perf_counter()
        try:
            return self.store.put(key, value)
        finally:
            self.seconds += time.perf_counter() - start


class MeasuredClient(FrClient):
    def __init__(self, root, executable):
        super().__init__(root, executable=executable, max_output_bytes=1_048_576)
        self.calls = self.context_bytes = 0
        self.seconds = 0.0

    def project(self, *arguments):
        start = time.perf_counter()
        report = self.call("--no-cache", "project", *arguments)
        self.seconds += time.perf_counter() - start
        self.calls += 1
        self.context_bytes += len(encode(report.to_data()))
        return report


def worker(args):
    store = MeasuredStore(args.work / "objects")
    before = inventory(args.work / "objects")
    client = MeasuredClient(args.project, str(args.fr.resolve()))
    cache_type = legacy_class() if args.strategy == "legacy" else FlowCache
    start = time.perf_counter()
    target = client.project("find", "entry").definition_target().handle
    if args.worker == "clean":
        report = client.project("dataflow", target, "--rules", str(args.project / "rules.json"),
                                "--summaries", "--imports", "--steps", "4096", "--depth", "32",
                                "--bytes", "1048576")
        manifest = None
    else:
        cache = cache_type.restore(store, args.cache_root) if args.cache_root else cache_type(store)
        report = cache.analyze(client, target, rules=args.project / "rules.json", steps=4096,
                               depth=32, max_bytes=1_048_576, summaries=True, imports=True).report
        manifest = cache.persist()
    elapsed = time.perf_counter() - start
    after = inventory(args.work / "objects")
    data = report.to_data()
    execution = data.pop("execution")
    data.pop("context_basis", None)
    scale = 1 if sys.platform == "darwin" else 1024
    metrics = {"elapsed_seconds": elapsed, "native_seconds": client.seconds, "store_seconds": store.seconds,
               "context_bytes": client.context_bytes, "native_calls": client.calls,
               "analysis_steps": execution["analysis_steps"], "store_gets": store.gets, "store_puts": store.puts,
               "worker_peak_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
               "native_peak_rss_bytes": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale,
               "stored_files": after[0], "stored_bytes": after[1],
               "added_files": after[0] - before[0], "added_bytes": after[1] - before[1]}
    cached = restore_stored_value(store, manifest)["entries"] if manifest else {}
    return {"report": data, "evidence_digest": digest(data), "metrics": metrics, "manifest": manifest,
            "reused": execution["kind"] == "retained", "complete": data["complete"],
            "current_input_stored": data["input_digest"] in cached, "tokens": None, "source_reveals": 0}


def invoke(fr, project, work, strategy, mode, cache_root=None):
    args = [sys.executable, str(Path(__file__).resolve()), "--worker", mode, "--project", str(project),
            "--work", str(work), "--strategy", strategy, "--fr", str(fr)]
    if cache_root:
        args += ["--cache-root", cache_root]
    process = subprocess.run(args, check=True, capture_output=True, text=True, timeout=180)
    return json.loads(process.stdout)


def summary(samples):
    result = {}
    for case in CASES:
        result[case] = {}
        for scenario in ("cold", *SCENARIOS):
            result[case][scenario] = {}
            for strategy in STRATEGIES:
                rows = [sample for sample in samples if (sample["case"], sample["scenario"], sample["strategy"])
                        == (case, scenario, strategy)]
                result[case][scenario][strategy] = {field: statistics.median(row["metrics"][field] for row in rows)
                                                   for field in FIELDS}
    return result


def verify_report(report, files, expected_oracle):
    FlowDependencies.from_report(FrReport(report, ()))
    closure = {name: digest(contents) for name, contents in files.items()
               if name.endswith(".py") and not name.startswith("noise")}
    assert report["inputs"]["modules"]["files"] == closure
    assert report["inputs"]["source"] == {"path": "app.py", "digest": digest(files["app.py"])}
    assert report["input_digest"] == digest(report["inputs"])
    if report["complete"]:
        assert expected_oracle["error"] is None
        assert len(report["witnesses"]) == sum(expected_oracle["sinks"]) // 37
    else:
        assert expected_oracle["error"] in ("NameError", "AttributeError") and report["cutoffs"]
    for witness in report["witnesses"]:
        site = witness["site"]
        assert [site["location"]["span"]["start"], site["location"]["span"]["end"]] in expected_oracle["call_spans"][site["path"]]


def measure(args):
    before = bindings()
    binary = hashlib.sha256(args.fr.read_bytes()).hexdigest()
    samples, references, oracles = [], {}, {}
    with tempfile.TemporaryDirectory(prefix="fr-flow-cache-") as temporary:
        base = Path(temporary)
        for case in CASES:
            project = base / case
            original = sources(case)
            oracles[case] = {}
            clean = {}
            for scenario in ("cold", *SCENARIOS):
                files = mutate(original, case, "warm" if scenario == "cold" else scenario)
                install(project, files)
                oracles[case][scenario] = oracle(files)
                result = invoke(args.fr, project, base / "clean", "chunks", "clean")
                verify_report(result["report"], files, oracles[case][scenario])
                assert result["complete"] == (scenario != "missing"), (case, scenario)
                clean[scenario] = result["evidence_digest"]
                references[result["evidence_digest"]] = result["report"]
            for repetition in range(args.repetitions):
                strategies = STRATEGIES if repetition % 2 == 0 else STRATEGIES[::-1]
                for strategy in strategies:
                    work = base / "caches" / case / str(repetition) / strategy
                    seed = None
                    for scenario in ("cold", *SCENARIOS):
                        install(project, mutate(original, case, "warm" if scenario == "cold" else scenario))
                        result = invoke(args.fr, project, work, strategy, "cached", seed)
                        assert result["evidence_digest"] == clean[scenario], (case, strategy, scenario)
                        result.pop("report")
                        if scenario == "cold":
                            seed = result["manifest"]
                        samples.append({"case": case, "scenario": scenario, "strategy": strategy,
                                        "repetition": repetition, **result})
                print(f"measured {case} repetition {repetition + 1}", file=sys.stderr, flush=True)
        boundaries = {}
        for case in ("pipeline-24", "recursive-24"):
            project = base / case
            install(project, sources(case))
            result = invoke(args.fr, project, base / "boundary" / case, "chunks", "cached")
            again = invoke(args.fr, project, base / "boundary" / case, "chunks", "cached", result["manifest"])
            assert not result["complete"] and not again["reused"] and not again["current_input_stored"]
            boundaries[case] = result
    assert before == bindings() and binary == hashlib.sha256(args.fr.read_bytes()).hexdigest()
    return {"schema": "fr-flow-cache-acceptance-1", "source_bindings": before, "binary_sha256": binary,
            "repository_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "runtime": {"platform": platform.platform(), "python": platform.python_version(), "cpus": os.cpu_count()},
            "repetitions": args.repetitions, "samples": samples, "reports": references, "oracles": oracles,
            "boundaries": boundaries, "summary": summary(samples), "task": json.loads((FIXTURE / "task.json").read_text()),
            "scope": "Six finite scalar workloads. Cold means empty flow store; native fact cache disabled. OS caches are uncontrolled. Isolated worker RSS includes startup; child RSS is the largest native child, not aggregate memory. Context bytes count canonical JSON responses, not model tokens. No general latency or partial-summary reuse claim."}


def audit(value):
    assert value["schema"] == "fr-flow-cache-acceptance-1" and value["source_bindings"] == bindings()
    assert value["task"] == json.loads((FIXTURE / "task.json").read_text())
    assert 3 <= value["repetitions"] <= 7
    expected = {(case, scenario, strategy, repeat) for case in CASES for scenario in ("cold", *SCENARIOS)
                for strategy in STRATEGIES for repeat in range(value["repetitions"])}
    observed = [(row["case"], row["scenario"], row["strategy"], row["repetition"]) for row in value["samples"]]
    assert set(observed) == expected and len(observed) == len(expected)
    assert value["summary"] == summary(value["samples"])
    for key, report in value["reports"].items():
        assert digest(report) == key
    for case in CASES:
        cold = next(row for row in value["samples"] if row["case"] == case and row["scenario"] == "cold")
        cold_input = value["reports"][cold["evidence_digest"]]["input_digest"]
        for scenario in ("cold", *SCENARIOS):
            files = mutate(sources(case), case, "warm" if scenario == "cold" else scenario)
            expected_oracle = oracle(files)
            assert value["oracles"][case][scenario] == expected_oracle
            for row in [item for item in value["samples"] if (item["case"], item["scenario"]) == (case, scenario)]:
                report = value["reports"][row["evidence_digest"]]
                verify_report(report, files, expected_oracle)
                assert (report["input_digest"] == cold_input) == (scenario in ("cold", "warm", "unrelated"))
                assert report["complete"] == row["complete"] == (scenario != "missing")
                assert row["reused"] == (scenario in ("warm", "unrelated"))
                assert row["current_input_stored"] == row["complete"]
                assert row["tokens"] is None and row["source_reveals"] == 0
                metrics = row["metrics"]
                assert set(metrics) == set(FIELDS)
                assert all(type(number) in (int, float) and 0 <= number < float("inf") for number in metrics.values())
                assert metrics["native_calls"] == 3 and metrics["context_bytes"] > 0
                assert metrics["native_seconds"] + metrics["store_seconds"] <= metrics["elapsed_seconds"]
                assert (metrics["analysis_steps"] == 0) == row["reused"]
    assert set(value["boundaries"]) == {"pipeline-24", "recursive-24"}
    for result in value["boundaries"].values():
        assert not result["complete"] and not result["reused"] and not result["current_input_stored"]
        assert "response-budget" in result["report"]["cutoffs"]
    return {"passed": True, "samples": len(expected), "workloads": len(CASES), "repetitions": value["repetitions"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--repetitions", type=int, choices=range(3, 8), default=3)
    parser.add_argument("--worker", choices=("cached", "clean"))
    parser.add_argument("--project", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--strategy", choices=STRATEGIES, default="chunks")
    parser.add_argument("--cache-root")
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args)))
    elif args.audit:
        print(json.dumps(audit(json.loads(args.audit.read_text()))))
    else:
        value = measure(args)
        audit(value)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
        else:
            print(json.dumps(value, indent=2))


if __name__ == "__main__":
    main()
