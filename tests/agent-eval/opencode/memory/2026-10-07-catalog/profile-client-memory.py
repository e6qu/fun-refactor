#!/usr/bin/env python3
"""Profile one scripted configured-client control on a GitHub runner."""
import argparse
import io
import json
import os
from pathlib import Path
import runpy
import sys
from unittest.mock import patch

from agent_eval import bounded_host, client_memory_profile, source_reviews, terminal_review_runner
from agent_eval.study import encode, require


def check(output, binary, opencode):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "profile the client only on GitHub")
    output.mkdir(parents=True, exist_ok=False)
    root = output / "configured"
    root.mkdir()
    command = Path(__file__).with_name("check-terminal-reviews.py")
    sampler = client_memory_profile.Sampler(root)
    out, err = io.BytesIO(), io.BytesIO()
    with patch.object(bounded_host, "sample", sampler):
        process = bounded_host.run([sys.executable, "-B", str(command), "capture", str(root),
            "--case", "configured", "--fr", str(binary), "--opencode", str(opencode)],
            b"", out, err, root, wall_seconds=120, cpu_limit_seconds=20, rss_bytes=768 * 1024**2,
            disk_bytes=16 * 1024**2, transcript_bytes=1024**2)
    (root / "host.stdout").write_bytes(out.getvalue())
    (root / "host.stderr").write_bytes(err.getvalue())
    (root / "process.json").write_bytes(encode(process))
    profile = client_memory_profile.audit(root, process)
    (root / "profile.json").write_bytes(encode(profile))
    frozen = json.loads((root / "plan.json").read_bytes())
    snapshots = source_reviews.read_inputs(root)
    cell = frozen["plan"]["cells"][0]
    attempt = root / "attempts" / cell["id"]
    terminal_review_runner.cleanup(attempt)
    terminal_review_runner.seal(frozen, snapshots, cell, attempt, process)
    result = runpy.run_path(str(command))["report"](root, "configured")
    (output / "result.json").write_bytes(encode([result]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check",))
    parser.add_argument("output", type=Path)
    parser.add_argument("--case", choices=("configured",), required=True)
    parser.add_argument("--fr", type=Path, required=True)
    parser.add_argument("--opencode", type=Path, required=True)
    args = parser.parse_args()
    check(args.output.resolve(), args.fr.resolve(), args.opencode.resolve())


if __name__ == "__main__":
    main()
