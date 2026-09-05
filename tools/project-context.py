#!/usr/bin/env python3
"""Measure complete compact maps against the legacy symbol response on a local fixture."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--fr", type=Path, default=Path("target/debug/fr"))
    args = parser.parse_args()
    root = args.root.resolve()
    binary = args.fr.resolve()

    def run(*command):
        started = time.perf_counter()
        result = subprocess.run(
            [str(binary), "--no-cache", "--json", "-C", str(root), *command],
            capture_output=True, check=True,
        )
        return json.loads(result.stdout), len(result.stdout), time.perf_counter() - started

    legacy, legacy_bytes, legacy_seconds = run("symbols")
    expected = sorted(
        (str(Path(row["file"]).relative_to(root if root.is_dir() else root.parent)),
         row["kind"], row["name"], row["line"])
        for row in legacy if row["kind"] not in ("variable", "parameter")
    )
    cursor = None
    compact_bytes = 0
    compact_seconds = 0
    pages = 0
    actual = []
    revision = None
    while True:
        command = ["project", "map", "--depth", "64", "--limit", "500",
                   "--fields", "id,parent,kind,name,path,line"]
        if cursor:
            command += ["--cursor", cursor]
        report, size, seconds = run(*command)
        if revision is not None and revision != report["revision"]:
            raise RuntimeError("fixture changed during measurement")
        revision = report["revision"]
        compact_bytes += size
        compact_seconds += seconds
        pages += 1
        for cells in report["rows"]:
            row = dict(zip(report["columns"], cells))
            if row["kind"] not in ("directory", "file"):
                actual.append((row["path"], row["kind"], row["name"], row["line"]))
        cursor = report["page"]["next"]
        if cursor is None:
            break
    if sorted(actual) != expected:
        raise RuntimeError("compact map did not preserve every nonlocal symbol's identity and position")
    files = {Path(row["file"]) for row in legacy}
    source_bytes = sum(path.stat().st_size for path in files)
    print(json.dumps({
        "root": str(root), "project_revision": revision,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "symbols": len(legacy), "nonlocal_symbols_matched": len(expected),
        "source_bytes_in_symbol_files": source_bytes,
        "legacy": {"stdout_bytes": legacy_bytes, "calls": 1, "seconds": round(legacy_seconds, 3)},
        "compact": {"stdout_bytes": compact_bytes, "calls": pages, "seconds": round(compact_seconds, 3)},
        "stdout_reduction_percent": round(100 * (1 - compact_bytes / legacy_bytes), 2),
        "model_tokens": None,
        "scope": "all nonlocal symbols; locals intentionally omitted; no model task-success measurement",
    }, indent=2))


if __name__ == "__main__":
    main()
