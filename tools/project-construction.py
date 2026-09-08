#!/usr/bin/env python3
"""Compare cached CLI reports and construction profiles before and after a change."""

import argparse
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import statistics
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("phase_measurement", ROOT / "tools/project-profile.py")
phases = importlib.util.module_from_spec(spec)
spec.loader.exec_module(phases)
cache = phases.cache_measurement
harness = cache.harness
CONSTRUCTION = {"manifests", "setup", "source_read", "source_digest", "source_lines",
                "hierarchy", "symbol_digest", "reference_digest", "finish"}


def check_construction(profile):
    stages = profile["construction_seconds"]
    assert set(stages) == CONSTRUCTION
    assert all(value >= 0 for value in stages.values())
    assert sum(stages.values()) <= profile["phases_seconds"]["project"]


def measure(before, after, before_profiler, after_profiler, repetitions):
    binaries = {str(p): harness.digest(p.read_bytes()) for p in (before, after, before_profiler, after_profiler)}
    started = datetime.now(timezone.utc).isoformat()
    examples, inspections = [], []
    with tempfile.TemporaryDirectory(prefix="fr-project-construction-") as tmp:
        base = Path(tmp)
        root = base / "project"
        harness.unpack(root, harness.regex_workspace.TASK)
        harness.initialize(root)
        original, original_index = harness.snapshot(root), (root / ".git/index").read_bytes()
        caches = {side: base / side for side in ("before", "after")}
        for name, path in [("escape", "src/lib.rs"), ("escape_into", "regex-syntax/src/lib.rs")]:
            args = ["project", "find", name, "--in", path, "--signature", "--source", "--bytes", "2048"]
            baseline, _ = cache.query(before, root, caches["before"], args)
            warmup, _ = cache.query(after, root, caches["after"], args)
            cache.same_report(baseline, warmup)
            profilers = {"before": before_profiler, "after": after_profiler}
            executables = {"before": before, "after": after}
            profile_args = ["project", "--construction", *args[1:]]
            for side in profilers:
                warmup = phases.profile(profilers[side], root, caches[side], profile_args, False)
                assert warmup["report_stdout"] == baseline["stdout"]
            inventory = {side: cache.cache_inventory(directory) for side, directory in caches.items()}
            samples = []
            for repetition in range(repetitions):
                for side in (("before", "after") if repetition % 2 == 0 else ("after", "before")):
                    sample, _ = cache.query(executables[side], root, caches[side], args)
                    cache.same_report(baseline, sample)
                    profile = phases.profile(profilers[side], root, caches[side], profile_args, False)
                    assert profile["report_stdout"] == baseline["stdout"]
                    assert profile["fact_cache_hits"] == profile["indexed_files"]
                    check_construction(profile)
                    cache_after = cache.cache_inventory(caches[side])
                    assert cache_after == inventory[side]
                    samples.append({"side": side, "repetition": repetition + 1,
                                    "cache_after": cache_after,
                                    "cli": {k: v for k, v in sample.items() if k != "stdout"},
                                    "profile": {"report_sha256": harness.digest(profile["report_stdout"].encode()),
                                                **{k: v for k, v in profile.items() if k != "report_stdout"}}})
            summary = {}
            for side in ("before", "after"):
                selected = [s for s in samples if s["side"] == side]
                summary[side] = {"cli_median_seconds": statistics.median(s["cli"]["seconds"] for s in selected),
                                 "construction_median_seconds": statistics.median(s["profile"]["phases_seconds"]["project"] for s in selected),
                                 "construction_stages_median_seconds": {stage: statistics.median(s["profile"]["construction_seconds"][stage] for s in selected) for stage in sorted(CONSTRUCTION)}}
            examples.append({"name": name, "args": args, "baseline": baseline, "cache_before": inventory,
                             "samples": samples, "summary": summary,
                             "invalidation": cache.invalidation(after, root, caches["after"], args, json.loads(baseline["stdout"]))})
        commands = [["project", name] for name in ("packages", "dependencies", "workspaces", "gaps")]
        commands.append(["project", "map", "--depth", "64", "--limit", "500"])
        for args in commands:
            page_count, byte_count, digests = 0, 0, []
            command = args
            while True:
                first, result = cache.query(before, root, caches["before"], command)
                second, _ = cache.query(after, root, caches["after"], command)
                cache.same_report(first, second)
                page_count += 1
                byte_count += first["stdout_bytes"]
                digests.append(first["stdout_sha256"])
                cursor = result.get("page", {}).get("next")
                if cursor is None:
                    break
                command = [*args, "--cursor", cursor]
            inspections.append({"args": args, "pages": page_count, "stdout_bytes": byte_count, "page_sha256": digests})
        assert harness.snapshot(root) == original and (root / ".git/index").read_bytes() == original_index
    assert all(harness.digest(Path(path).read_bytes()) == digest for path, digest in binaries.items())
    files = ["tools/project-construction.py", "tools/project-profile.py", "tools/project-profile.rs", "tools/project-cache.py",
             "tools/agent-eval.py", "tools/agent_eval/regex_workspace.py", "src/project.rs", "Cargo.toml", "Cargo.lock",
             "tests/agent-eval/project-construction-before.patch"]
    return {"schema": "fr-project-construction-comparison-1", "passed": True, "started_at": started,
            "finished_at": datetime.now(timezone.utc).isoformat(), "binaries_sha256": binaries,
            "runtime": {"platform": phases.platform.platform(), "machine": phases.platform.machine(),
                        "python": phases.platform.python_version(), "logical_cpus": phases.os.cpu_count(),
                        "rayon_num_threads": phases.os.environ.get("RAYON_NUM_THREADS")},
            "measurement_files": {name: harness.digest((ROOT / name).read_bytes()) for name in files},
            "archive_sha256": harness.regex_workspace.ARCHIVE_SHA, "dependency_lock_sha256": harness.regex_workspace.LOCK_SHA,
            "repetitions": repetitions, "examples": examples, "inspections": inspections, "source_and_index_restored": True,
            "scope": "Alternating before/after release CLI and development-profiler samples on one host with populated caches. Construction checkpoints include their own overhead; ordinary CLI calls have no construction timing. Every query and complete map/manifest/diagnostic page matches the baseline bytes, including revisions and continuation cursors. Cached source-invalidation probes target the candidate CLI. No agent, context-saving, cold-start or general production-latency claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--before-profiler", type=Path, required=True)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/release/fr")
    parser.add_argument("--profiler", type=Path, default=ROOT / "target/release/examples/project-profile")
    parser.add_argument("--repetitions", type=int, default=4, choices=range(2, 21))
    args = parser.parse_args()
    print(json.dumps(measure(args.before.resolve(strict=True), args.fr.resolve(strict=True),
                            args.before_profiler.resolve(strict=True), args.profiler.resolve(strict=True), args.repetitions), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
