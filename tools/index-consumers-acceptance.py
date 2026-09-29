#!/usr/bin/env python3
"""Isolated, bounded consumer measurements against a pinned repository snapshot."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import tempfile
import time

from evidence_basis import file_digest

ROOT = Path(__file__).resolve().parents[1]
TASK = ROOT / "tests/agent-eval/index-consumers/task.json"
BINDINGS = ["tests/index_consumers.rs", "tests/reference_storage.rs",
            "tests/agent-eval/index-consumers/task.json", "tools/index-consumers-acceptance.py",
            "tools/evidence_basis.py", "src/index.rs", "src/index/references.rs",
            "src/model.rs", "src/extract.rs", "src/parse.rs", "src/scan.rs", "Cargo.lock"]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verify(path):
    report = json.loads(path.read_text())
    baseline = json.loads((TASK.parent / "baseline.json").read_text())
    assert report["schema"] == "fr-index-consumers-result-1"
    assert report["repository_revision"] == json.loads(TASK.read_text())["baseline"]
    assert report["bindings"] == {p: file_digest(ROOT / p) for p in BINDINGS}
    assert report["baseline_sha256"] == digest(TASK.parent / "baseline.json")
    assert len(report["samples"]) == len(baseline["samples"]) == 3
    for before, after in zip(baseline["samples"], report["samples"]):
        portable_fields = ["workload", "symbols", "references", "queries"]
        if after["workload"] != "repository":
            portable_fields.append("answer_bytes")
        for field in portable_fields:
            assert before[field] == after[field], (field, before[field], after[field])
        assert type(after["answer_bytes"]) is int and after["answer_bytes"] > 0
        assert after["oracle_agrees"] is True
        assert len(after["query_seconds"]) == 3
        assert all(isinstance(value, (float, int)) and 0 <= value < 240 for value in after["query_seconds"])
        assert after["peak_rss_bytes"] > 0 and after["process_seconds"] > 0
    print("consumer evidence bindings and finite workload agreement verified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="compiled index_consumers test executable")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    if args.verify:
        verify(args.verify)
        return
    if not args.binary or not args.output:
        parser.error("measurement requires --binary and --output")
    task = json.loads(TASK.read_text())
    bindings = {path: file_digest(ROOT / path) for path in BINDINGS}
    binary_digest = digest(args.binary)
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
            process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       text=True, start_new_session=True)
            try:
                stdout, stderr = process.communicate(timeout=task["budgets"]["measurement_timeout_seconds"])
            except BaseException:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.communicate()
                raise
            if process.returncode:
                raise RuntimeError(stdout + stderr)
            elapsed = time.monotonic() - start
            sample = json.loads(output.read_text())
            if platform.system() == "Darwin":
                match = re.search(r"(\d+)\s+maximum resident set size", stderr)
                rss = int(match[1]) if match else None
            else:
                match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
                rss = int(match[1]) * 1024 if match else None
            assert rss is not None, stderr
            sample.update(process_seconds=elapsed, peak_rss_bytes=rss)
            samples.append(sample)
            print(f"{workload}: {sample['queries']} queries, {sample['query_seconds']}", flush=True)
    report = {"schema": "fr-index-consumers-result-1", "repository_revision": task["baseline"],
              "host": platform.platform(), "binary_sha256": binary_digest,
              "profile": "unoptimized test, debug information and incremental compilation disabled",
              "bindings": bindings, "samples": samples,
              "baseline_sha256": digest(TASK.parent / "baseline.json"),
              "claim": "Membership and occurrence order agree with a linear oracle on this finite workload. Process RSS includes indexing and oracle storage."}
    assert bindings == {path: file_digest(ROOT / path) for path in BINDINGS}, "sources changed during measurement"
    assert binary_digest == digest(args.binary), "binary changed during measurement"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    verify(args.output)


if __name__ == "__main__":
    main()
