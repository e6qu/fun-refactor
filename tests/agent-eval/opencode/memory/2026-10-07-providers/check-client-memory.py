#!/usr/bin/env python3
"""Compare the pinned OpenCode client with default and lower-memory Bun settings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

from agent_eval import client_memory_profile, source_reviews, terminal_reviews
from agent_eval.study import encode, require

SCHEMA = "fr-client-memory-check-1"
CELLS = [{"id": f"{mode}-{index}", "mode": mode, "bun_options": option}
         for index in range(3) for mode, option in (("default", ""), ("smol", "--smol"))]
CATALOG_CELLS = [{"id": f"{mode}-{index}", "mode": mode, "bun_options": ""}
                 for index in range(3) for mode in ("default", "catalog")]
CATALOG_DATA = b"{}\n"
HEADROOM_RSS = 640 * 1024**2
LIMITS = {"wall_seconds": 120, "cpu_seconds": 20, "rss_bytes": 768 * 1024**2,
          "disk_bytes": 16 * 1024**2, "transcript_bytes": 1024**2}


def identity(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_catalog(path):
    with path.open("rb") as source:
        data = source.read(65537)
    require(0 < len(data) <= 65536 and isinstance(json.loads(data), dict), "invalid catalog")
    return data


def report(root):
    plan = json.loads((root / "plan.json").read_bytes())
    require(plan["script_sha256"] == identity(Path(__file__)), "control implementation changed")
    experiment = plan.get("experiment", "smol")
    require(experiment in ("smol", "catalog"), "unknown memory experiment")
    cells = CATALOG_CELLS if experiment == "catalog" else CELLS
    if experiment == "catalog":
        data = read_catalog(root / "catalog.json")
        require(plan["catalog_sha256"] == identity(root / "catalog.json"), "scripted catalog changed")
        if "catalog_source_sha256" in plan:
            require(plan["catalog_source_sha256"] == plan["catalog_sha256"], "catalog source changed")
        else:
            require(data == CATALOG_DATA, "scripted catalog changed")
    require(plan["schema"] == SCHEMA and plan["cells"] == cells and plan["limits"] == LIMITS,
            "memory control design changed")
    require(plan["admission_rss_bytes"] == HEADROOM_RSS, "memory admission margin changed")
    if "profile_implementation" in plan:
        require(plan["profile_implementation"] == {
            "module": identity(Path(client_memory_profile.__file__)),
            "launcher": identity(Path(__file__).with_name("profile-client-memory.py"))},
            "profile implementation changed")
    rows = []
    for cell in cells:
        folder = root / cell["id"] / "configured"
        frozen = json.loads((folder / "plan.json").read_bytes())
        require(frozen["plan"]["binary_sha256"] == plan["fr_sha256"]
                and frozen["plan"]["opencode_sha256"] == plan["opencode_sha256"], "control binary changed")
        snapshots = source_reviews.read_inputs(folder)
        audited = terminal_reviews.report(frozen, snapshots, folder / "attempts")
        require(len(audited["attempts"]) == 1, "unexpected control attempt count")
        attempt = audited["attempts"][0]
        require(attempt["process"]["limits"] == LIMITS, "control resource limit changed")
        status = json.loads((root / cell["id"] / "command.json").read_bytes())
        require(status["cell"] == cell, "control runtime option changed")
        completed = attempt["status"] == "completed" and status["exit_code"] == 0
        if completed:
            checked = json.loads((root / cell["id"] / "result.json").read_bytes())
            require(len(checked) == 1 and checked[0]["case"] == "configured"
                    and checked[0]["provider_requests"] == 2 and checked[0]["report"] == audited,
                    "scripted source-delivery result differs")
        rows.append({**cell, "completed": completed, "failure": attempt["failure"],
                     "process": attempt["process"]})
        if "profile_implementation" in plan:
            profile = client_memory_profile.audit(folder, attempt["process"])
            require(profile == json.loads((folder / "profile.json").read_bytes()), "profile report differs")
            rows[-1]["profile"] = profile
    candidate = [row for row in rows if row["mode"] == experiment]
    return {"schema": SCHEMA, "experiment": experiment, "plan_sha256": identity(root / "plan.json"), "cases": rows,
            "admitted": all(row["completed"] and row["process"]["sampled_aggregate_rss_bytes"] <= HEADROOM_RSS
                            for row in candidate),
            "scope": "Scripted client controls only; live-provider memory and workstation peaks remain unverified."}


def check(root, binary, opencode, *, profile=False, experiment="smol", catalog=None):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run the complete memory gate on GitHub")
    require(experiment in ("smol", "catalog"), "unknown memory experiment")
    require(catalog is None or experiment == "catalog", "a catalog requires the catalog experiment")
    data = CATALOG_DATA if catalog is None else read_catalog(catalog)
    root.mkdir(parents=True, exist_ok=False)
    cells = CATALOG_CELLS if experiment == "catalog" else CELLS
    plan = {"schema": SCHEMA, "experiment": experiment, "cells": cells, "limits": LIMITS, "admission_rss_bytes": HEADROOM_RSS,
            "platform": sys.platform, "machine": platform.machine(), "os_release": platform.release(),
            "commit": os.environ["GITHUB_SHA"], "script_sha256": identity(Path(__file__)),
            "fr_sha256": identity(binary), "opencode_sha256": identity(opencode)}
    if experiment == "catalog":
        (root / "catalog.json").write_bytes(data)
        plan["catalog_sha256"] = identity(root / "catalog.json")
        if catalog is not None:
            plan["catalog_source_sha256"] = hashlib.sha256(data).hexdigest()
    if profile:
        plan["profile_implementation"] = {
            "module": identity(Path(client_memory_profile.__file__)),
            "launcher": identity(Path(__file__).with_name("profile-client-memory.py"))}
    (root / "plan.json").write_bytes(encode(plan))
    for cell in cells:
        env = {**os.environ, "BUN_OPTIONS": cell["bun_options"], "RAYON_NUM_THREADS": "1"}
        env.pop("OPENCODE_MODELS_PATH", None)
        if cell["mode"] == "catalog":
            env["OPENCODE_MODELS_PATH"] = str(root / "catalog.json")
        destination = root / cell["id"]
        log = root / (cell["id"] + ".log")
        with log.open("xb") as output:
            process = subprocess.run([sys.executable, "-B", str(Path(__file__).with_name("profile-client-memory.py" if profile else "check-terminal-reviews.py")),
                "check", str(destination), "--case", "configured", "--fr", str(binary), "--opencode", str(opencode)],
                env=env, stdout=output, stderr=subprocess.STDOUT, timeout=150)
        (destination / "command.json").write_bytes(encode({"cell": cell, "exit_code": process.returncode}))
        print(cell["id"] + ": exit " + str(process.returncode), flush=True)
    result = report(root)
    (root / "result.json").write_bytes(encode(result))
    print(encode(result).decode())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("measure", "check", "report", "admit"))
    parser.add_argument("output", type=Path)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    parser.add_argument("--profile", action="store_true", help="Retain process samples and inferred capture stages")
    parser.add_argument("--experiment", choices=("smol", "catalog"), default="smol")
    parser.add_argument("--catalog", type=Path, help="Use a bounded, hash-bound model catalog in catalog candidates")
    args = parser.parse_args()
    if args.command in ("report", "admit"):
        result = report(args.output.resolve())
        print(encode(result).decode())
        if args.command == "admit":
            require(result["admitted"], "lower-memory controls did not establish the required headroom")
        return
    require(args.fr is not None and args.opencode is not None, "supply both pinned binaries")
    result = check(args.output.resolve(), args.fr.resolve(), args.opencode.resolve(), profile=args.profile, experiment=args.experiment, catalog=args.catalog)
    if args.command == "check":
        require(result["admitted"], "lower-memory controls did not establish the required headroom")


if __name__ == "__main__":
    main()
