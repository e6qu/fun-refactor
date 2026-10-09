#!/usr/bin/env python3
"""Verify archived streaming measurements using their exact retained Python sources."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile

from agent_eval.study import require

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "tests/agent-eval/opencode/changes/streaming-2026-10-09/runs"
DELIVERY = ROOT / "tests/agent-eval/opencode/changes/delivery-2026-10-09/runs"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract(path, destination, limit):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        require(len(names) == len(set(names)) and sum(i.file_size for i in archive.infolist()) <= limit,
                "archive inventory exceeds budget")
        require(all((destination / n).resolve().is_relative_to(destination) for n in names), "unsafe archive path")
        archive.extractall(destination)


def verify(folder):
    provenance = json.loads((folder / "provenance.json").read_bytes())
    require(str(provenance["run_id"]) == folder.name, "run identity differs")
    expected = {f"streaming-{platform}-{case}" for platform in ("macos-15", "ubuntu-latest") for case in range(4)}
    require(len(provenance["artifacts"]) == 8 and {a["name"] for a in provenance["artifacts"]} == expected,
            "measurement matrix differs")
    require(sha(folder / "runner.zip") == provenance["runner_sha256"], "runner archive changed")
    rows = []
    with tempfile.TemporaryDirectory(prefix="fr-streaming-replay-") as temporary:
        root = Path(temporary).resolve()
        runner = root / "runner"
        runner.mkdir()
        extract(folder / "runner.zip", runner, 8 * 1024**2)
        sources = {p.relative_to(runner).as_posix(): sha(p) for p in runner.rglob("*") if p.is_file()}
        require(sources == provenance["sources"], "retained source inventory changed")
        for artifact in provenance["artifacts"]:
            path = folder / (artifact["name"] + ".zip")
            require(path.stat().st_size == artifact["bytes"] <= 16 * 1024**2 and sha(path) == artifact["sha256"],
                    "hosted artifact changed")
            with tempfile.TemporaryDirectory(dir=root) as case_directory:
                case = Path(case_directory)
                extract(path, case, 64 * 1024**2)
                subprocess.run([sys.executable, "-B", str(runner / "tools/check-change-streaming.py"), "report", str(case)],
                               cwd=runner, stdout=subprocess.DEVNULL, check=True, timeout=30)
                rows.append({"artifact": artifact["name"], "result": json.loads((case / "result.json").read_bytes())})
        require(rows == json.loads((folder / "results.json").read_bytes()), "retained measurement summary differs")
    counts = {status: sum(a["status"] == status for row in rows for a in row["result"]["attempts"])
              for status in ("completed", "failed", "not_started")}
    print(json.dumps({"run": provenance["run_id"], "verified": True, "counts": counts,
                      "admitted_cases": sum(row["result"]["admitted"] for row in rows)}))


def verify_delivery(folder):
    provenance = json.loads((folder / "provenance.json").read_bytes())
    require(provenance["schema"] == "fr-delivery-evidence-1" and str(provenance["run_id"]) == folder.name,
            "delivery run identity differs")
    modes = ("whole-burst", "whole-paced", "chunked-burst", "chunked-paced")
    expected = {f"delivery-{platform}-{case}-{mode}" for platform in ("macos-15", "ubuntu-latest")
                for case in (0, 2) for mode in modes} | {"delivery-summary"}
    require(len(provenance["artifacts"]) == 17 and {a["name"] for a in provenance["artifacts"]} == expected,
            "delivery measurement matrix differs")
    require(sha(folder / "runner.zip") == provenance["runner_sha256"], "runner archive changed")
    with tempfile.TemporaryDirectory(prefix="fr-delivery-replay-") as temporary:
        root = Path(temporary).resolve()
        runner = root / "runner"
        runner.mkdir()
        extract(folder / "runner.zip", runner, 8 * 1024**2)
        sources = {p.relative_to(runner).as_posix(): sha(p) for p in runner.rglob("*") if p.is_file()}
        require(sources == provenance["sources"], "retained source inventory changed")
        expanded = 0
        for artifact in provenance["artifacts"]:
            path = folder / (artifact["name"] + ".zip")
            require(path.stat().st_size == artifact["bytes"] <= 16 * 1024**2 and sha(path) == artifact["sha256"],
                    "hosted artifact changed")
            with zipfile.ZipFile(path) as archive:
                expanded += sum(entry.file_size for entry in archive.infolist())
            require(expanded <= 128 * 1024**2, "delivery archive set exceeds budget")
            destination = root / artifact["name"]
            destination.mkdir()
            extract(path, destination, 32 * 1024**2)
        with (root / "replayed.json").open("wb") as output:
            subprocess.run([sys.executable, "-B", str(runner / "tools/check-streaming-delivery.py"), "matrix", str(root)],
                           cwd=runner, stdout=output, check=True, timeout=30)
        result = json.loads((root / "replayed.json").read_bytes())
        require(result == json.loads((root / "delivery-summary/delivery-result.json").read_bytes())
                == json.loads((folder / "results.json").read_bytes()), "retained delivery summary differs")
    counts = {status: sum(a["status"] == status for group in result["groups"] for cell in group["cells"].values()
                         for a in cell["attempts"]) for status in ("completed", "failed", "not_started")}
    print(json.dumps({"run": provenance["run_id"], "verified": True, "counts": counts, "admitted": result["admitted"]}))


if __name__ == "__main__":
    require(EVIDENCE.is_dir(), "no retained streaming measurements")
    folders = sorted(EVIDENCE.iterdir())
    require(folders, "empty streaming evidence collection")
    for folder in folders:
        require(folder.is_dir() and folder.name.isdigit(), "unexpected evidence entry")
        verify(folder)
    if DELIVERY.exists():
        folders = sorted(DELIVERY.iterdir())
        require(folders, "empty delivery evidence collection")
        for folder in folders:
            require(folder.is_dir() and folder.name.isdigit(), "unexpected delivery evidence entry")
            verify_delivery(folder)
