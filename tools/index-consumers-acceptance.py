#!/usr/bin/env python3
"""Isolated, bounded consumer measurements against a pinned repository snapshot."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
TASK = ROOT / "tests/agent-eval/index-consumers/task.json"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="compiled index_consumers test executable")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    task = json.loads(TASK.read_text())
    samples = []
    with tempfile.TemporaryDirectory(prefix="fr-consumers-") as temporary:
        work = Path(temporary)
        snapshot = work / "repository"
        snapshot.mkdir()
        archive = work / "repository.tar"
        with archive.open("wb") as stream:
            subprocess.run(["git", "archive", task["baseline"]], cwd=ROOT, stdout=stream, check=True)
        subprocess.run(["tar", "-xf", str(archive), "-C", str(snapshot)], check=True)
        for workload in ["128", "512", "repository"]:
            output = work / "sample.json"
            env = dict(os.environ, FR_CONSUMER_ROOT=str(snapshot), FR_CONSUMER_WORKLOAD=workload,
                       FR_CONSUMER_OUTPUT=str(output), RAYON_NUM_THREADS="1", RUST_TEST_THREADS="1",
                       XDG_CACHE_HOME=str(work / "cache"))
            command = [str(args.binary.resolve()), "measure_consumer_queries", "--exact", "--ignored"]
            command = ["/usr/bin/time", "-l" if platform.system() == "Darwin" else "-v", *command]
            start = time.monotonic()
            result = subprocess.run(command, env=env, capture_output=True, text=True,
                                    timeout=task["budgets"]["measurement_timeout_seconds"])
            if result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
            elapsed = time.monotonic() - start
            sample = json.loads(output.read_text())
            if platform.system() == "Darwin":
                match = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
                rss = int(match[1]) if match else None
            else:
                match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", result.stderr)
                rss = int(match[1]) * 1024 if match else None
            assert rss is not None, result.stderr
            sample.update(process_seconds=elapsed, peak_rss_bytes=rss)
            samples.append(sample)
            print(f"{workload}: {sample['queries']} queries, {sample['query_seconds']}", flush=True)
    bindings = ["tests/index_consumers.rs", "tests/agent-eval/index-consumers/task.json",
                "tools/index-consumers-acceptance.py", "src/index.rs"]
    if (ROOT / "src/index/references.rs").exists():
        bindings.append("src/index/references.rs")
    report = {"schema": "fr-index-consumers-result-1", "repository_revision": task["baseline"],
              "host": platform.platform(), "binary_sha256": digest(args.binary),
              "profile": "unoptimized test, debug information and incremental compilation disabled",
              "bindings": {path: digest(ROOT / path) for path in bindings}, "samples": samples,
              "claim": "Membership and occurrence order agree with a linear oracle on this finite workload. Process RSS includes indexing and oracle storage."}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
