#!/usr/bin/env python3
"""Verify or restore registered historical evidence without changing its bytes."""
import argparse
import json
from pathlib import Path

import evidence_archive as archive

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=ROOT / "tests/agent-eval/evidence-archives.json")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check")
    restore = commands.add_parser("restore")
    restore.add_argument("original", help="Original repository-relative JSON path from the catalog")
    restore.add_argument("destination", type=Path, help="New file; existing files are never overwritten")
    args = parser.parse_args()
    entries = archive.catalog(ROOT, args.catalog)["entries"]
    if args.command == "check":
        for entry in entries:
            archive.require(not archive.safe(ROOT, entry["path"]).exists(), "both archived and original evidence exist")
            archive.transfer(ROOT, entry)
        print(json.dumps({"verified": len(entries), "original_bytes": sum(e["bytes"] for e in entries),
                          "archive_bytes": sum(e["archive_bytes"] for e in entries)}))
    else:
        found = [entry for entry in entries if entry["path"] == args.original]
        archive.require(len(found) == 1, "original path is not registered")
        archive.restore(ROOT, found[0], args.destination)
        print(json.dumps({"restored": str(args.destination), "bytes": found[0]["bytes"], "sha256": found[0]["sha256"]}))


if __name__ == "__main__":
    main()
