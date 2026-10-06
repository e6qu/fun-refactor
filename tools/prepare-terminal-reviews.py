#!/usr/bin/env python3
"""Check or freeze new review questions using an unchanged retained source archive."""
import argparse
import base64
import gzip
from pathlib import Path

from agent_eval import native_mcp as mcp, source_packets, source_reviews, terminal_reviews as review
from agent_eval.study import digest, encode, require


def prepare(design, repository):
    require(set(design) == {"schema", "source_archive", "models", "questions"}
            and design["schema"] == "fr-terminal-review-design-1", "invalid review design")
    archive = design["source_archive"]
    require(set(archive) == {"path", "sha256"}, "invalid source archive identity")
    relative = Path(archive["path"])
    require(not relative.is_absolute() and ".." not in relative.parts and relative.name == "inputs.json.gz",
            "invalid source archive path")
    source = repository / relative
    require(source.resolve().is_relative_to(repository.resolve()) and not source.is_symlink(), "source archive escapes repository")
    require(source.stat().st_size <= review.MAX_BYTES and source_reviews.identity(source) == archive["sha256"],
            "source archive changed or exceeds budget")
    snapshots = source_reviews.read_inputs(source.parent)
    require(isinstance(design["questions"], list) and 1 <= len(design["questions"]) <= 3,
            "choose one to three new review questions")
    questions = []
    for question in design["questions"]:
        require(set(question) == {"id", "source_task", "question", "selections", "comparison"}, "invalid question design")
        files = snapshots[question["source_task"]]
        comparison = mcp.decode(base64.b64decode(files[question["comparison"]]["data"], validate=True))
        source_packets.check_comparison(files, comparison)
        questions.append({k: question[k] for k in ("id", "question", "selections")} | {"files": files})
    require(len(questions) * len(design["models"]) <= 6, "design exceeds six-cell collection limit")
    provenance = {"design_sha256": digest(design), "source_archive_sha256": archive["sha256"],
                  "scope": "New questions and terminal protocol; no earlier attempt is resumed."}
    return questions, provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "freeze"))
    parser.add_argument("design", type=Path)
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fr", type=Path)
    parser.add_argument("--opencode", type=Path)
    args = parser.parse_args()
    design = mcp.decode(review.read(args.design))
    questions, provenance = prepare(design, args.repository.resolve())
    if args.command == "freeze":
        require(args.fr is not None and args.opencode is not None and args.output is not None,
                "freeze requires --fr, --opencode and --output")
        provenance["preparation_sha256"] = source_reviews.identity(Path(__file__))
        frozen, snapshots = review.freeze(questions, design["models"], args.fr.resolve(), args.opencode.resolve(), provenance)
        archive = gzip.compress(encode(snapshots), mtime=0)
        require(len(archive) <= review.MAX_BYTES, "review input archive exceeds replay budget")
        args.output.mkdir(parents=True, exist_ok=False)
        (args.output / "plan.json").write_bytes(encode(frozen))
        (args.output / "inputs.json.gz").write_bytes(archive)
        print(frozen["sha256"])
    else:
        frozen, _ = review.design(questions, design["models"],
            {"runtime": {}, "binary_sha256": "0" * 64, "opencode_sha256": "0" * 64}, provenance)
        print(encode({"design_sha256": digest(design), "cells": len(frozen["plan"]["cells"]),
            "questions": [{"id": task["id"], "prompt_bytes": len((review.PROMPT + "\nTask:\n" + task["requirement"]).encode()),
                           "source_bytes": sum(s["end"] - s["start"] for s in task["packet"]["spans"])}
                          for task in frozen["plan"]["tasks"]],
            "frozen_for_execution": False, "claims_verified": False}).decode())


if __name__ == "__main__":
    main()
