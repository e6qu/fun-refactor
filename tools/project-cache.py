#!/usr/bin/env python3
"""Compare disabled, empty and populated fr caches on prescribed regex queries."""

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
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
MODES = ("disabled", "empty", "populated")


def cache_inventory(directory):
    files = sorted(path for path in directory.rglob("*") if path.is_file())
    entries = {str(path.relative_to(directory)): harness.digest(path.read_bytes()) for path in files}
    return {"files": len(files), "bytes": sum(path.stat().st_size for path in files),
            "contents_sha256": harness.digest(json.dumps(entries, sort_keys=True).encode())}


def query(binary, root, cache, args, disabled=False, success=True):
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(cache)
    argv = [str(binary), "--json", "-C", str(root)]
    if disabled:
        argv.append("--no-cache")
    argv.extend(args)
    started = time.perf_counter_ns()
    result = subprocess.run(argv, cwd=root, env=env, capture_output=True, timeout=180)
    seconds = (time.perf_counter_ns() - started) / 1_000_000_000
    assert (result.returncode == 0) == success, (argv, result)
    report = json.loads(result.stdout)
    return {"args": args, "cache_disabled": disabled, "seconds": seconds,
            "exit_code": result.returncode, "stdout_bytes": len(result.stdout),
            "stdout_sha256": harness.digest(result.stdout), "stdout": result.stdout.decode(),
            "stderr": result.stderr.decode()}, report


def same_report(expected, actual):
    assert actual["stdout"] == expected["stdout"], "Cache mode changed the complete query report"


def invalidation(binary, root, cache, args, baseline):
    before_cache = cache_inventory(cache)
    original_row = dict(zip(baseline["columns"], baseline["rows"][0]))
    path = root / original_row["path"]
    original = path.read_bytes()
    source = original_row["source"]
    start, end = source["span"]["start"], source["span"]["end"]
    selected = original[start:end]
    assert selected.decode() == source["text"] and source["next_offset"] is None
    marker = b"\n    // fr cache invalidation probe\n"
    at = selected.index(b"{") + 1
    replacement = selected[:at] + marker + selected[at:]
    try:
        path.write_bytes(original[:start] + replacement + original[end:])
        changed, found = query(binary, root, cache, args)
        fresh, uncached = query(binary, root, cache, args, disabled=True)
        same_report(fresh, changed)
        assert found == uncached and found["revision"] != baseline["revision"]
        row = dict(zip(found["columns"], found["rows"][0]))
        assert row["source"]["text"] == replacement.decode()
        assert row["handle"] != original_row["handle"]
        refused, error = query(binary, root, cache,
                               ["project", "show", original_row["handle"], "--source"], success=False)
        assert "stale" in error["error"]["message"].lower(), error
        changed_cache = cache_inventory(cache)
        assert changed_cache["files"] > before_cache["files"], "Changed source must populate a new cache entry"
    finally:
        path.write_bytes(original)
    restored, found = query(binary, root, cache, args)
    assert found == baseline, "Restoring source must restore the original revision and report"
    return {"passed": True, "path": original_row["path"],
            "original_sha256": harness.digest(original), "probe": marker.decode(),
            "before_cache": before_cache, "changed_cache": changed_cache,
            "changed": changed, "uncached": fresh, "stale_handle": refused, "restored": restored,
            "scope": "A temporary comment inside the selected function; this checks source invalidation, not build behavior."}


def measure(binary, repetitions):
    assert 3 <= repetitions <= 20
    started_at = datetime.now(timezone.utc).isoformat()
    binary_sha = harness.digest(binary.read_bytes())
    examples = []
    with tempfile.TemporaryDirectory(prefix="fr-project-cache-") as tmp:
        base = Path(tmp)
        root = base / "project"
        harness.unpack(root, harness.regex_workspace.TASK)
        harness.initialize(root)
        original = harness.snapshot(root)
        original_index = (root / ".git/index").read_bytes()
        for name, path in [("escape", "src/lib.rs"), ("escape_into", "regex-syntax/src/lib.rs")]:
            args = ["project", "find", name, "--in", path, "--signature", "--source", "--bytes", "2048"]
            runs, baseline, warm_cache = [], None, None
            for repetition in range(repetitions):
                order = MODES[repetition % 3:] + MODES[:repetition % 3]
                for mode in order:
                    cache = base / "caches" / name / str(repetition) / mode
                    assert not cache.exists()
                    warmup = None
                    if mode == "populated":
                        warmup, _ = query(binary, root, cache, args)
                        warm_cache = cache
                    before = cache_inventory(cache)
                    sample, found = query(binary, root, cache, args, disabled=mode == "disabled")
                    after = cache_inventory(cache)
                    assert found["page"]["total"] == 1 and found["page"]["next"] is None
                    if baseline is None:
                        baseline = sample
                    same_report(baseline, sample)
                    if warmup:
                        same_report(baseline, warmup)
                        assert before["files"] > 0 and before == after
                    elif mode == "disabled":
                        assert before["files"] == 0 and before == after and not cache.exists()
                    else:
                        assert before["files"] == 0 and after["files"] > 0
                    runs.append({"repetition": repetition + 1, "mode": mode,
                                 "before_cache": before, "after_cache": after,
                                 "warmup_seconds": warmup["seconds"] if warmup else None,
                                 **{key: value for key, value in sample.items() if key != "stdout"}})
            assert harness.snapshot(root) == original and (root / ".git/index").read_bytes() == original_index
            probe = invalidation(binary, root, warm_cache, args, json.loads(baseline["stdout"]))
            assert harness.snapshot(root) == original and (root / ".git/index").read_bytes() == original_index
            summary = {}
            for mode in MODES:
                times = [run["seconds"] for run in runs if run["mode"] == mode]
                summary[mode] = {"min_seconds": min(times), "median_seconds": statistics.median(times),
                                 "max_seconds": max(times), "samples": len(times)}
            examples.append({"name": name, "path": path, "args": args, "baseline": baseline,
                             "runs": runs, "summary": summary, "invalidation": probe})
    assert harness.digest(binary.read_bytes()) == binary_sha, "Binary changed during measurement"
    return {"schema": "fr-project-cache-1", "passed": True, "binary_sha256": binary_sha,
            "binary_path": str(binary), "repetitions": repetitions, "started_at": started_at,
            "finished_at": datetime.now(timezone.utc).isoformat(),
            "runtime": {"platform": platform.platform(), "machine": platform.machine(),
                        "python": platform.python_version(), "logical_cpus": os.cpu_count(),
                        "rayon_num_threads": os.environ.get("RAYON_NUM_THREADS")},
            "archive_sha256": harness.regex_workspace.ARCHIVE_SHA,
            "dependency_lock_sha256": harness.regex_workspace.LOCK_SHA,
            "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in
                                  (Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "tools/agent_eval/regex_workspace.py")},
            "examples": examples, "source_and_index_restored": True,
            "scope": "Single-host prescribed queries; rotating cache-mode order; each populated run has a separately timed priming query excluded from the summaries. Cache inventory hashing also runs outside the timed query and reads populated entries. Empty means an empty fr fact cache, not a cold OS cache. Timings cover whole subprocess calls, including scanning, indexing, source verification and output; no phase profiling or cache-hit counters. No agents, task builds, context savings or production latency claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(3, 21))
    args = parser.parse_args()
    print(json.dumps(measure(args.fr.resolve(strict=True), args.repetitions), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
