#!/usr/bin/env python3
"""Compare separate lookup/source reads with bounded source lookup on pinned regex."""

import argparse
import copy
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


def run(binary, root, args):
    payload = harness.process([str(binary), "--no-cache", "--json", "-C", str(root), *args], root)
    assert payload["exit_code"] == 0, payload
    assert isinstance(payload["result"], dict) and payload["stdout_omitted_bytes"] == 0
    return json.dumps(payload, ensure_ascii=False), payload["result"]


def sizes(texts, encoding):
    return {"bytes": sum(len(text.encode()) for text in texts),
            "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts) if encoding else None}


def measure(binary, encoding):
    binary_sha = harness.digest(binary.read_bytes())
    examples = []
    with tempfile.TemporaryDirectory(prefix="fr-find-source-context-") as tmp:
        root = Path(tmp) / "project"
        harness.unpack(root, harness.regex_workspace.TASK)
        harness.initialize(root)
        original = harness.snapshot(root)
        original_index = (root / ".git/index").read_bytes()
        for name, path in [("escape", "src/lib.rs"), ("escape_into", "regex-syntax/src/lib.rs")]:
            find_args = ["project", "find", name, "--in", path, "--signature"]
            find_text, found = run(binary, root, find_args)
            assert found["page"]["total"] == 1 and found["page"]["next"] is None
            row = dict(zip(found["columns"], found["rows"][0]))
            show_args = ["project", "show", row["handle"], "--source", "--bytes", "2048"]
            show_text, shown = run(binary, root, show_args)
            combined_args = [*find_args, "--source", "--bytes", "2048"]
            combined_text, combined = run(binary, root, combined_args)
            combined_row = dict(zip(combined["columns"], combined["rows"][0]))
            assert combined_row["source"] == shown["source"]
            assert combined_row["source"]["next_offset"] is None
            assert combined["source_budget"]["returned_bytes"] == shown["source"]["returned_bytes"]
            restored = copy.deepcopy(combined)
            del restored["source_budget"]
            assert restored["columns"].pop() == "source"
            restored["rows"][0].pop()
            assert restored == found
            before, after = sizes([find_text, show_text], encoding), sizes([combined_text], encoding)
            assert after["bytes"] < before["bytes"]
            examples.append({"name": name, "path": path, "passed": True,
                             "separate": before, "combined": after,
                             "requests": {"separate": [find_args, show_args], "combined": [combined_args]},
                             "payloads": {"separate": [find_text, show_text], "combined": [combined_text]},
                             "preserved": "Lookup rows, coverage, scope, revision and complete selected source slice match."})
        assert harness.snapshot(root) == original
        assert (root / ".git/index").read_bytes() == original_index
    assert harness.digest(binary.read_bytes()) == binary_sha, "Binary changed during measurement"
    return {"schema": "fr-find-source-context-1", "passed": True, "binary_sha256": binary_sha,
            "archive_sha256": harness.regex_workspace.ARCHIVE_SHA,
            "dependency_lock_sha256": harness.regex_workspace.LOCK_SHA,
            "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in
                                  (Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "tools/agent_eval/regex_workspace.py")},
            "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                          "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"} if encoding else None,
            "examples": examples, "source_and_index_unchanged": True,
            "scope": "Controlled prescribed queries on a pinned workspace, using the agent harness JSON wrapper. No agents, discovery, total-task context or latency measurement. Separate show also includes node metadata not requested by the combined lookup; use show when those extra facts are needed."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(args.fr.resolve(strict=True), harness.tokenizer() if args.tokens else None), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
