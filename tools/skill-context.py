#!/usr/bin/env python3
"""Compare recorded skill reads with an explicit targeted reading route; no agents."""

import argparse
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-08-regex"
SKILL = ROOT / "skills/fr"
TARGETED = ["SKILL.md", "references/author.md", "references/checks.md", "references/history.md", "references/git.md"]


def payload(path):
    lines = path.read_text().splitlines(keepends=True)
    text = "".join(f"{i + 1}: {line}" for i, line in enumerate(lines))
    assert len(lines) <= 200 and len(text) <= 20000
    return json.dumps({"text": text, "total_lines": len(lines), "next_line": None}, ensure_ascii=False)


def sizes(texts, encoding):
    return {"bytes": sum(len(text.encode()) for text in texts),
            "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts) if encoding else None}


def redundant_maps(events, encoding):
    results = []
    for index, event in enumerate(events):
        request = event["request"]
        args = request.get("args", [])
        if request["tool"] != "fr" or args[:2] != ["project", "map"]:
            continue
        if args[3:] != ["--depth", "0", "--fields", "handle,kind,name", "--limit", "1"]:
            continue
        report = json.loads(event["visible"])["result"]
        rows = [dict(zip(report["columns"], row)) for row in report["rows"]]
        assert len(rows) == 1 and rows[0]["kind"] == "file"
        candidates = []
        for earlier, previous in enumerate(events[:index]):
            previous_args = previous["request"].get("args", [])
            if previous_args[:2] != ["project", "find"] or "--in" not in previous_args:
                continue
            if previous_args[previous_args.index("--in") + 1] != args[2]:
                continue
            found = json.loads(previous["visible"])["result"]
            if (found["revision"] == report["revision"] and found["root"] == rows[0]["handle"]
                    and all(item["before"] == item["after"] == previous["after"] for item in events[earlier:index + 1])):
                candidates.append(earlier)
        results.append({"map_event": index, "file": args[2], "eligible": bool(candidates),
                        "prior_file_scoped_lookup_events": candidates,
                        "removable_payload": sizes([event["visible"]], encoding) if candidates else None})
    return results


def measure(encoding):
    manifest = json.loads((EVIDENCE / "manifest.json").read_text())
    for name, sha in manifest["files"].items():
        assert harness.digest(harness.within(EVIDENCE, name).read_bytes()) == sha, name
    current = {str(path.relative_to(SKILL)): payload(path) for path in sorted(SKILL.rglob("*.md"))}
    results = []
    for trial in manifest["trials"]:
        if "-fr-" not in trial:
            continue
        events = [json.loads(line) for line in (EVIDENCE / trial / "events.jsonl").read_text().splitlines()]
        reads = [event for event in events if event["request"]["tool"] == "read"
                 and event["request"]["path"].startswith("skill/")]
        paths = [event["request"]["path"].removeprefix("skill/") for event in reads]
        assert len(paths) == len(set(paths)) and set(TARGETED).issubset(paths)
        for path, event in zip(paths, reads):
            assert payload(EVIDENCE / "skill" / path) == event["visible"]
        recorded = sizes([event["visible"] for event in reads], encoding)
        if encoding:
            score = json.loads((EVIDENCE / trial / "result.json").read_text())
            assert recorded["tokens"] == score["output_tokens_by_category"]["skill"]
        results.append({"trial": trial, "recorded_paths": paths, "recorded_reads": recorded,
                        "current_same_paths": sizes([current[path] for path in paths], encoding),
                        "current_targeted_paths": TARGETED,
                        "current_targeted_reads": sizes([current[path] for path in TARGETED], encoding),
                        "handle_lookup_analysis": redundant_maps(events, encoding)})
    return {"schema": "fr-skill-context-1", "passed": True,
            "evidence_manifest_sha256": harness.digest((EVIDENCE / "manifest.json").read_bytes()),
            "measurement_script_sha256": harness.digest(Path(__file__).read_bytes()),
            "skill_files": {str(path.relative_to(SKILL)): harness.digest(path.read_bytes())
                            for path in sorted(SKILL.rglob("*")) if path.is_file()},
            "tokenizer": json.loads((EVIDENCE / manifest["trials"][0] / "result.json").read_text())["tokenizer"] if encoding else None,
            "current_read_payloads": current,
            "conditional_recovery_read": sizes([current["references/recovery.md"]], encoding),
            "trials": results,
            "scope": "Controlled reading-route counts and redundant handle-lookup payload analysis; no agents, total-task savings or latency estimate. Current skill includes the M4m check guidance and the new executable authoring example. The targeted route assumes a localized edit without pagination, relationship queries or interrupted writes. Broader tasks load the required references."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(harness.tokenizer() if args.tokens else None), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
