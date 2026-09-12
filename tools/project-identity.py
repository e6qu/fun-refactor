#!/usr/bin/env python3
"""Exercise project identity and incremental resolution on a generic workspace."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
QUERY = ["find", "alpha", "--in", "src/a.rs", "--signature", "--source", "--bytes", "1024"]
ORIGINAL = "pub const VALUE: i32 = 1;\npub fn alpha() -> i32 { VALUE }\n"
SCALAR = "pub const VALUE: i32 = 2;\npub fn alpha() -> i32 { VALUE }\n"
STRUCTURAL = "pub const VALUE: i32 = 2;\npub fn omega() -> i32 { VALUE }\n"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def profile(binary, root, cache, disabled=False):
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(cache)
    command = [str(binary), "-C", str(root)]
    if disabled:
        command.append("--no-cache")
    command += QUERY
    result = subprocess.run(command, env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, (command, result)
    report = json.loads(result.stdout)
    assert report["schema"] == "fr-project-profile-1"
    return report


def revision(profile_report):
    return json.loads(profile_report["report_stdout"])["revision"]


def selected_handle(profile_report):
    report = json.loads(profile_report["report_stdout"])
    return report["rows"][0][report["columns"].index("handle")]


def stale_handle_refused(binary, root, cache, handle):
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(cache)
    result = subprocess.run(
        [str(binary), "--json", "-C", str(root), "project", "show", handle],
        env=env,
        capture_output=True,
        timeout=30,
    )
    combined = result.stdout + result.stderr
    assert result.returncode != 0 and b"stale" in combined.lower(), result
    return {"exit_code": result.returncode, "output_sha256": digest(combined)}


def compact(report):
    return {
        "report_sha256": digest(report["report_stdout"].encode()),
        "revision": revision(report),
        "fact_cache_hits": report["fact_cache_hits"],
        "resolution_cache_hits": report["resolution_cache_hits"],
        "resolution_cache_misses": report["resolution_cache_misses"],
        "indexed_files": report["indexed_files"],
        "reference_count": report["reference_count"],
        "phases_seconds": report["phases_seconds"],
        "measured_seconds": report["measured_seconds"],
    }


def measure(binary, profiler):
    started = datetime.now(timezone.utc).isoformat()
    binary_hashes = {str(path): digest(path.read_bytes()) for path in (binary, profiler)}
    with tempfile.TemporaryDirectory(prefix="fr-project-identity-") as temporary:
        base = Path(temporary)
        project = base / "project"
        source = project / "src/a.rs"
        source.parent.mkdir(parents=True)
        source.write_text(ORIGINAL)
        (project / "src/b.rs").write_text("pub fn caller() -> i32 { alpha() }\n")
        (project / "Cargo.toml").write_text('[package]\nname = "identity-fixture"\nversion = "0.1.0"\n')
        cache = base / "cache"

        uncached = profile(profiler, project, cache, disabled=True)
        cold = profile(profiler, project, cache)
        warm = profile(profiler, project, cache)
        assert cold["report_stdout"] == uncached["report_stdout"] == warm["report_stdout"]
        assert cold["resolution_cache_misses"] == 1 and warm["resolution_cache_hits"] == 1
        original_revision = revision(warm)
        handle = selected_handle(warm)

        source.write_text(SCALAR)
        scalar = profile(profiler, project, cache)
        scalar_uncached = profile(profiler, project, cache, disabled=True)
        assert scalar["report_stdout"] == scalar_uncached["report_stdout"]
        assert scalar["resolution_cache_hits"] == 1
        assert revision(scalar) != original_revision
        refused = stale_handle_refused(binary, project, cache, handle)

        source.write_text(STRUCTURAL)
        structural = profile(profiler, project, cache)
        assert structural["resolution_cache_misses"] == 1
        assert json.loads(structural["report_stdout"])["page"]["total"] == 0

        source.write_text(ORIGINAL)
        restored = profile(profiler, project, cache)
        assert restored["resolution_cache_hits"] == 1
        assert restored["report_stdout"] == warm["report_stdout"]
        assert source.read_text() == ORIGINAL

    assert all(digest(Path(path).read_bytes()) == value for path, value in binary_hashes.items())
    return {
        "schema": "fr-project-identity-eval-1",
        "passed": True,
        "started_at": started,
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "runtime": {"platform": platform.platform(), "python": platform.python_version()},
        "binaries_sha256": binary_hashes,
        "measurement_sha256": digest(Path(__file__).read_bytes()),
        "query": QUERY,
        "uncached": compact(uncached),
        "cold": compact(cold),
        "warm": compact(warm),
        "scalar_edit": compact(scalar),
        "structural_edit": compact(structural),
        "restored": compact(restored),
        "stale_handle": refused,
        "scope": "Generated Rust fixture; one host; warm OS caches; no agent, build, parser proof, latency threshold or production-performance claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-project-identity-eval-1" and report["passed"] is True
    assert report["measurement_sha256"] == digest(Path(__file__).read_bytes())
    for binary, expected in report["binaries_sha256"].items():
        candidate = Path(binary)
        if candidate.exists():
            assert digest(candidate.read_bytes()) == expected, candidate
    unchanged = [report[name] for name in ("uncached", "cold", "warm", "restored")]
    assert len({item["report_sha256"] for item in unchanged}) == 1
    assert len({item["revision"] for item in unchanged}) == 1
    assert report["warm"]["resolution_cache_hits"] == 1
    assert report["scalar_edit"]["resolution_cache_hits"] == 1
    assert report["scalar_edit"]["revision"] != report["warm"]["revision"]
    assert report["structural_edit"]["resolution_cache_misses"] == 1
    assert report["structural_edit"]["report_sha256"] != report["warm"]["report_sha256"]
    return {"schema": report["schema"], "passed": True, "report": str(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--profiler", type=Path, default=ROOT / "target/debug/examples/project-profile")
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.audit:
        report = audit(args.audit.resolve(strict=True))
    else:
        report = measure(args.fr.resolve(strict=True), args.profiler.resolve(strict=True))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
