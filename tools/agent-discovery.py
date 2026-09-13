#!/usr/bin/env python3
"""Exercise bounded discovery and concurrent resolution on a generic workspace."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
OWNER = re.compile(rb"(\d+) resolution owner\(s\)")
WAIT = re.compile(rb"(\d+) wait\(s\)")
TIMEOUT = re.compile(rb"(\d+) timeout\(s\)")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def command(binary, project, *arguments):
    return [str(binary), "--json", "-C", str(project), "project", *arguments]


def run(binary, project, cache, *arguments, timeout=30):
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(cache)
    result = subprocess.run(
        command(binary, project, *arguments),
        env=env,
        capture_output=True,
        timeout=timeout,
    )
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    return json.loads(result.stdout), result.stderr


def cache_count(pattern, stderr):
    matches = pattern.findall(stderr)
    assert matches, stderr.decode(errors="replace")
    return int(matches[-1])


def measure(binary):
    with tempfile.TemporaryDirectory(prefix="fr-agent-discovery-") as temporary:
        base = Path(temporary)
        project = base / "project"
        project.mkdir()
        source = project / "service.py"
        source.write_text(
            "def dependency():\n"
            "    return 1\n\n"
            "def behavior_target():\n"
            "    return dependency()\n\n"
            "def caller():\n"
            "    return behavior_target()\n"
        )
        cache = base / "cache"
        env = os.environ.copy()
        env["FUN_REFACTOR_CACHE"] = str(cache)
        env["RUST_LOG"] = "fun_refactor=debug"
        argv = command(binary, project, "explore", "behavior_target")
        started = time.monotonic()
        processes = [
            subprocess.Popen(argv, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for _ in range(2)
        ]
        completed = [process.communicate(timeout=30) for process in processes]
        elapsed = time.monotonic() - started
        assert all(process.returncode == 0 for process in processes), completed
        reports = [json.loads(stdout) for stdout, _ in completed]
        stderrs = [stderr for _, stderr in completed]
        assert reports[0] == reports[1]
        owners = sum(cache_count(OWNER, stderr) for stderr in stderrs)
        waits = sum(cache_count(WAIT, stderr) for stderr in stderrs)
        timeouts = sum(cache_count(TIMEOUT, stderr) for stderr in stderrs)
        assert owners == 1 and timeouts == 0
        handle = reports[0]["rows"][0]["handle"]

        behavior, _ = run(
            binary,
            project,
            cache,
            "explore",
            "behavior_target",
            "--mode",
            "behavior",
            "--target",
            handle,
        )
        assert behavior["declaration"]["node"]["handle"] == handle
        assert behavior["declaration"]["source"]["returned_bytes"] <= 2_048
        assert len(behavior["relationships"]["items"]) <= 8
        assert len(json.dumps(behavior, separators=(",", ":")).encode()) <= 16_384

        manifest = base / "queries.json"
        manifest.write_text(
            json.dumps(
                {
                    "schema": "fr-project-batch-1",
                    "requests": [
                        {"id": "names", "arguments": ["explore", "behavior_target"]},
                        {
                            "id": "behavior",
                            "arguments": [
                                "explore",
                                "behavior_target",
                                "--mode",
                                "behavior",
                                "--target",
                                {"request": "names", "pointer": "/rows/0/handle"},
                            ],
                        },
                    ],
                }
            )
        )
        batch, _ = run(
            binary,
            project,
            cache,
            "batch",
            "--from",
            str(manifest),
            "--report-bytes",
            "1048576",
            "--profile",
            "compact",
        )
        assert batch["agent_profile"]["project_views"] == 1
        assert batch["report_budget"]["limit_bytes"] == 16_384
        assert [request["status"] for request in batch["requests"]] == [
            "returned",
            "returned",
        ]

        source.write_text(source.read_text().replace("return dependency()", "return 2"))
        stale_env = os.environ.copy()
        stale_env["FUN_REFACTOR_CACHE"] = str(cache)
        stale = subprocess.run(
            command(
                binary,
                project,
                "explore",
                "behavior_target",
                "--mode",
                "behavior",
                "--target",
                handle,
            ),
            env=stale_env,
            capture_output=True,
            timeout=30,
        )
        assert stale.returncode != 0 and b"stale" in stale.stdout.lower() + stale.stderr.lower()

    return {
        "schema": "fr-agent-discovery-eval-1",
        "passed": True,
        "tool_sha256": digest(Path(__file__).read_bytes()),
        "binary_sha256": digest(binary.read_bytes()),
        "parallel": {
            "queries": 2,
            "reports_byte_identical": True,
            "resolution_owners": owners,
            "resolution_waiters": waits,
            "resolution_timeouts": timeouts,
            "elapsed_seconds": round(elapsed, 6),
        },
        "compact": {
            "name_rows": len(reports[0]["rows"]),
            "source_bytes": behavior["declaration"]["source"]["returned_bytes"],
            "relationship_rows": len(behavior["relationships"]["items"]),
            "report_bytes": len(json.dumps(behavior, separators=(",", ":")).encode()),
        },
        "batch": {
            "requests": len(batch["requests"]),
            "project_views": batch["agent_profile"]["project_views"],
            "report_limit_bytes": batch["report_budget"]["limit_bytes"],
            "manifest_basis_schema": batch["manifest_basis"].split(":", 1)[0],
        },
        "stale_handle_refused": True,
        "scope": "Generated Python fixture on one host; no agent, parser proof, scheduler proof, hostile-filesystem proof or production latency claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-agent-discovery-eval-1" and report["passed"] is True
    assert report["tool_sha256"] == digest(Path(__file__).read_bytes())
    assert report["parallel"]["queries"] == 2
    assert report["parallel"]["reports_byte_identical"] is True
    assert report["parallel"]["resolution_owners"] == 1
    assert report["parallel"]["resolution_timeouts"] == 0
    assert report["compact"]["name_rows"] <= 12
    assert report["compact"]["source_bytes"] <= 2_048
    assert report["compact"]["relationship_rows"] <= 8
    assert report["compact"]["report_bytes"] <= 16_384
    assert report["batch"] == {
        "requests": 2,
        "project_views": 1,
        "report_limit_bytes": 16_384,
        "manifest_basis_schema": "frpqb2",
    }
    assert report["stale_handle_refused"] is True
    return {"schema": report["schema"], "passed": True, "report": str(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    report = audit(args.audit.resolve(strict=True)) if args.audit else measure(args.fr.resolve())
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
