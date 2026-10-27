#!/usr/bin/env python3
"""Measure current indexing on fixed sources; retain the original comparison separately."""

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
import tomllib

from evidence_basis import file_digest, native_source_bindings
from lean_reference_corrections import check_review, check_snapshot, wildcard_sites

ROOT = Path(__file__).resolve().parents[1]
TASK = ROOT / "tests/agent-eval/index-resolution/task.json"
BASELINE = TASK.parent / "baseline.json"
COMPARISON = 'Fixed source workloads; Rust namespace, attribute and conditional-declaration fixes change repository answers. An exact before/after inventory accounts for the Lean reader changes, plus six previously removed false wildcard reads. The smaller workloads retain exact original answers. Timing includes language changes; no repository-wide identical-answer or optimization-only claim.'


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def manifest_basis(data):
    manifest = tomllib.loads(data.decode())
    version = manifest["package"]["version"]
    manifest["package"]["version"] = "<workspace>"
    for container in [manifest, manifest.get("workspace", {}), *manifest.get("target", {}).values()]:
        for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
            for dependency in container.get(kind, {}).values():
                if (isinstance(dependency, dict) and "path" in dependency
                        and dependency.get("version") == version):
                    dependency["version"] = "<workspace>"
    return json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def bindings():
    paths = sorted(ROOT.joinpath("src").rglob("*.rs"))
    paths += [ROOT / p for p in ["Cargo.toml", "Cargo.lock", "tests/index_resolution.rs",
              "tools/index-resolution-acceptance.py", "tools/evidence_basis.py", str(TASK.relative_to(ROOT))]]
    paths += [ROOT / 'tools/audit-historical-index-resolution.py']
    paths += [ROOT / name for name in ('tools/lean_reference_corrections.py',
              'tests/common/lean_reference_corrections.rs', 'tests/agent-eval/index-consumers/ignored-wildcards.lean',
              'tests/agent-eval/index-consumers/lean-reader-review.json')]
    return {str(p.relative_to(ROOT)): hashlib.sha256(manifest_basis(p.read_bytes())).hexdigest()
            if p.name == "Cargo.toml" else file_digest(p) for p in paths} | native_source_bindings(ROOT)


def validate_samples(report):
    task = json.loads(TASK.read_text())
    expected = [w for w in task["budgets"]["workloads"] for _ in range(task["budgets"]["repetitions"])]
    assert [s["workload"] for s in report["samples"]] == expected
    assert report["schema"] == "fr-index-resolution-result-1"
    assert report["repository_revision"] == task["baseline"]
    for sample in report["samples"]:
        assert re.fullmatch(r"[0-9a-f]{64}", sample["answer_sha256"])
        assert all(sample[k] > 0 for k in ("symbols", "references", "answer_bytes", "peak_rss_bytes"))
        assert 0 < sample["build_seconds"] <= sample["process_seconds"] < task["budgets"]["measurement_timeout_seconds"]


def verify(path):
    report = json.loads(path.read_text())
    baseline = json.loads(BASELINE.read_text())
    validate_samples(report)
    validate_samples(baseline)
    assert report["bindings"] == bindings(), "stale source bindings"
    assert report['comparison'] == COMPARISON
    assert report["baseline_sha256"] == digest(BASELINE)
    sites = wildcard_sites(TASK)
    for before, after in zip(baseline["samples"], report["samples"]):
        fields = ("workload",) if after['workload'] == 'repository' else (
            "workload", "symbols", "references", "answer_bytes", "answer_sha256")
        if after['workload'] == 'repository':
            assert after['removed_wildcard_references'] == sites
            check_review(TASK, before, after)
        else:
            assert after['removed_wildcard_references'] == []
            assert after['lean_reader_review'] is None
        for field in fields:
            assert before[field] == after[field], (field, before, after)
    for workload in json.loads(TASK.read_text())["budgets"]["workloads"]:
        assert len({s["answer_sha256"] for s in baseline["samples"] if s["workload"] == workload}) == 1
        assert len({s['answer_sha256'] for s in report['samples'] if s['workload'] == workload}) == 1
    print("Current indexing: unchanged small workloads match; complete repository answers repeat exactly under the declared language changes")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline", action="store_true")
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    if args.verify:
        verify(args.verify)
        return
    if not args.binary or not args.output:
        parser.error("measurement requires --binary and --output")
    if args.baseline and BASELINE.exists():
        parser.error("the pinned baseline already exists")
    task = json.loads(TASK.read_text())
    before_bindings = bindings()
    binary_digest = digest(args.binary)
    samples = []
    with tempfile.TemporaryDirectory(prefix="fr-resolution-") as temporary:
        work = Path(temporary)
        snapshot = work / "repository"
        snapshot.mkdir()
        archive = work / "repository.tar"
        with archive.open("wb") as stream:
            subprocess.run(["git", "archive", task["baseline"]], cwd=ROOT, stdout=stream, check=True)
        subprocess.run(["tar", "-xf", str(archive), "-C", str(snapshot)], check=True)
        check_snapshot(TASK, snapshot)
        for workload in task["budgets"]["workloads"]:
            for repetition in range(task["budgets"]["repetitions"]):
                output = work / "sample.json"
                output.unlink(missing_ok=True)
                env = dict(os.environ, FR_RESOLUTION_ROOT=str(snapshot), FR_RESOLUTION_WORKLOAD=workload,
                           FR_RESOLUTION_OUTPUT=str(output), RAYON_NUM_THREADS="1", RUST_TEST_THREADS="1",
                           XDG_CACHE_HOME=str(work / "cache"))
                command = [str(args.binary.resolve()), "measure_resolution", "--exact", "--ignored"]
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
                print(f"{workload} run {repetition + 1}: {sample['build_seconds']:.3f}s, {rss / 1048576:.1f} MiB", flush=True)
    report = {"schema": "fr-index-resolution-result-1", "repository_revision": task["baseline"],
              "host": platform.platform(), "binary_sha256": binary_digest,
              "profile": "unoptimized test, debug information and incremental compilation disabled",
              "bindings": before_bindings, "samples": samples,
              "baseline_sha256": None if args.baseline else digest(BASELINE),
              "comparison": COMPARISON,
              "claim": "Current fresh indexing on the fixed workloads. Rust changes alter repository answers; the original equivalence comparison is audited against its original source. No facts cache; OS caches remain active. Process RSS includes output serialization. No full agent-cost or general speed claim."}
    assert before_bindings == bindings(), "sources changed during measurement"
    assert binary_digest == digest(args.binary), "binary changed during measurement"
    validate_samples(report)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    if not args.baseline:
        verify(args.output)


if __name__ == "__main__":
    main()
