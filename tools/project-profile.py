#!/usr/bin/env python3
"""Profile public project-query stages and compare every report with the release CLI."""

import argparse
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("cache_measurement", ROOT / "tools/project-cache.py")
cache_measurement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache_measurement)
harness = cache_measurement.harness
PHASES = {"root", "scan", "cache_open", "index", "project", "query", "verify", "serialize", "drop"}


def profile(binary, root, cache, args, disabled):
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(cache)
    argv = [str(binary), "-C", str(root)] + (["--no-cache"] if disabled else []) + args[1:]
    started = time.perf_counter_ns()
    process = subprocess.run(argv, cwd=root, env=env, capture_output=True, timeout=180)
    seconds = (time.perf_counter_ns() - started) / 1_000_000_000
    assert process.returncode == 0, process
    result = json.loads(process.stdout)
    assert result["schema"] == "fr-project-profile-1"
    assert set(result["phases_seconds"]) == PHASES
    assert all(0 <= value <= result["measured_seconds"] for value in result["phases_seconds"].values())
    assert sum(result["phases_seconds"].values()) <= result["measured_seconds"] <= seconds
    return {**result, "subprocess_seconds": seconds, "stderr": process.stderr.decode()}


def measure(binary, profiler, repetitions):
    started_at = datetime.now(timezone.utc).isoformat()
    binaries = {str(path): harness.digest(path.read_bytes()) for path in (binary, profiler)}
    examples = []
    with tempfile.TemporaryDirectory(prefix="fr-project-profile-") as tmp:
        base = Path(tmp)
        root = base / "project"
        harness.unpack(root, harness.regex_workspace.TASK)
        harness.initialize(root)
        original = harness.snapshot(root)
        original_index = (root / ".git/index").read_bytes()
        for name, path in [("escape", "src/lib.rs"), ("escape_into", "regex-syntax/src/lib.rs")]:
            args = ["project", "find", name, "--in", path, "--signature", "--source", "--bytes", "2048"]
            baseline, _ = cache_measurement.query(binary, root, base / "disabled", args, disabled=True)
            runs = []
            for repetition in range(repetitions):
                modes = cache_measurement.MODES
                for mode in modes[repetition % 3:] + modes[:repetition % 3]:
                    cache = base / "caches" / name / str(repetition) / mode
                    assert not cache.exists()
                    warmup = None
                    if mode == "populated":
                        warmup = profile(profiler, root, cache, args, False)
                        assert warmup["report_stdout"] == baseline["stdout"]
                    before = cache_measurement.cache_inventory(cache)
                    result = profile(profiler, root, cache, args, mode == "disabled")
                    after = cache_measurement.cache_inventory(cache)
                    assert result["report_stdout"] == baseline["stdout"], "Profile and CLI reports differ"
                    if mode == "populated":
                        assert before["files"] > 0 and before == after
                        assert result["fact_cache_hits"] == result["indexed_files"]
                    elif mode == "empty":
                        assert before["files"] == 0 and after["files"] > 0
                    else:
                        assert before == after and not cache.exists() and result["fact_cache_hits"] is None
                    runs.append({"mode": mode, "repetition": repetition + 1, "before_cache": before,
                                 "after_cache": after, "warmup_subprocess_seconds": warmup["subprocess_seconds"] if warmup else None,
                                 "report_sha256": harness.digest(result["report_stdout"].encode()),
                                 **{key: value for key, value in result.items() if key != "report_stdout"}})
            summaries = {}
            for mode in cache_measurement.MODES:
                selected = [run for run in runs if run["mode"] == mode]
                summaries[mode] = {"samples": len(selected),
                                   "median_subprocess_seconds": statistics.median(run["subprocess_seconds"] for run in selected),
                                   "median_phases_seconds": {phase: statistics.median(run["phases_seconds"][phase] for run in selected) for phase in sorted(PHASES)}}
            examples.append({"name": name, "path": path, "args": args, "baseline": baseline,
                             "runs": runs, "summary": summaries})
        assert harness.snapshot(root) == original and (root / ".git/index").read_bytes() == original_index
    assert all(harness.digest(Path(path).read_bytes()) == digest for path, digest in binaries.items())
    files = ["tools/project-profile.py", "tools/project-profile.rs", "tools/project-cache.py", "tools/agent-eval.py",
             "tools/agent_eval/regex_workspace.py", "Cargo.toml", "Cargo.lock", "src/cli.rs", "src/index.rs", "src/project.rs", "src/project/digest.rs", "src/cache.rs"]
    return {"schema": "fr-project-phase-comparison-1", "passed": True, "binaries_sha256": binaries,
            "started_at": started_at, "finished_at": datetime.now(timezone.utc).isoformat(),
            "runtime": {"platform": platform.platform(), "machine": platform.machine(), "python": platform.python_version(),
                        "logical_cpus": os.cpu_count(), "rayon_num_threads": os.environ.get("RAYON_NUM_THREADS")},
            "measurement_files": {name: harness.digest((ROOT / name).read_bytes()) for name in files},
            "archive_sha256": harness.regex_workspace.ARCHIVE_SHA, "dependency_lock_sha256": harness.regex_workspace.LOCK_SHA,
            "repetitions": repetitions, "examples": examples, "source_and_index_unchanged": True,
            "scope": "A separate executable times the public library pipeline with default scan options. Every report matches the ordinary CLI byte for byte. Stage times exclude argument parsing, startup and profiling-report output; subprocess time includes them. Index time includes fact loading/extraction, merging and workspace resolution. Populated runs require a fact-cache hit for every indexed file; resolution-cache hits are not counted. Priming and cache-inventory hashing occur outside the measured sample. Single host, rotating modes, no cold OS cache, agent, context-saving or general production-latency claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/release/fr")
    parser.add_argument("--profiler", type=Path, default=ROOT / "target/release/examples/project-profile")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(3, 21))
    args = parser.parse_args()
    print(json.dumps(measure(args.fr.resolve(strict=True), args.profiler.resolve(strict=True), args.repetitions), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
