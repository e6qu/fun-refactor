#!/usr/bin/env python3
"""Measure the prescribed PR 8 workflow against the frozen fresh PR 7 trace."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from agent_eval import regex_escape_len, regex_workspace  # noqa: E402


def imported(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


harness = imported("agent_workflow_v4_harness", ROOT / "tools/agent-eval.py")

EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-11-context-v3"
TRIAL = EVIDENCE / "regex-escape-len-fr"
EVENTS_SHA256 = "b92385d53e080cf3a9685d5cc9d6d80139f7b351508d92be2b6a1817a6c7f8cf"
REMOVED = {
    3: "show accepts handles and byte slices; the skill names that shape",
    4: "show accepts handles and byte slices; the skill names that shape",
    5: "find uses --contains for literal substrings; the route names that option",
    9: "the handle-aware selection at event 8 returns this exact declaration",
    10: "the handle-aware selection at event 8 returns this exact declaration",
    11: "duplicate of event 10",
    12: "duplicate of event 9",
    16: "the loaded author route names every operation and transition",
    17: "the loaded author route names every operation and transition",
    22: "the write response supplies the exact manifest fr_reference",
    23: "fragment fr_reference values make the first manifest executable",
    24: "the first manifest uses the returned absolute fragment references",
    31: "the loaded Git route names history patch as the fr export path",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify_evidence():
    manifest = json.loads((EVIDENCE / "manifest.json").read_text())
    for name, expected in manifest["files"].items():
        path = harness.within(EVIDENCE, name)
        require(digest(path.read_bytes()) == expected, f"Frozen evidence changed: {name}")
    require(digest((TRIAL / "events.jsonl").read_bytes()) == EVENTS_SHA256,
            "Frozen fr event trace changed")
    recorded = json.loads((TRIAL / "result.json").read_text())
    require(recorded["passed"] and recorded["tool_calls"] == 42
            and recorded["context_tokens"] == 15458
            and recorded["refusals_or_failures"] == 7,
            "Frozen PR 7 acceptance baseline changed")
    return recorded


def skill_visible(relative, lines):
    text = (ROOT / "skills/fr" / relative).read_text().splitlines(keepends=True)
    body = "".join(f"{index + 1}: {line}" for index, line in enumerate(text[:lines]))
    return json.dumps({"text": body[:20000], "total_lines": len(text),
                       "next_line": lines + 1 if lines < len(text) else None})


def live_inspection(binary, find_request, select_request):
    with tempfile.TemporaryDirectory(prefix="fr-workflow-v4-") as tmp:
        project = Path(tmp) / "project"
        regex_workspace.unpack(project)
        (project / ".fr").mkdir()
        harness.save(project / ".fr/checks.json",
                     {"schema": 1, "checks": regex_escape_len.CHECKS})
        harness.initialize(project)
        found = subprocess.run(
            [str(binary), "--no-cache", "--json", "-C", str(project), *find_request["args"]],
            capture_output=True, text=True, timeout=120,
        )
        require(found.returncode == 0, f"Current escape lookup failed: {found.stderr}")
        find_report = json.loads(found.stdout)
        columns = [str(column) for column in find_report["columns"]]
        handle_index, path_index = columns.index("handle"), columns.index("path")
        by_path = {row[path_index]: row[handle_index] for row in find_report["rows"]}
        handles = [by_path[path] for path in ("regex-syntax/src/lib.rs", "src/lib.rs")]
        request = json.loads(json.dumps(select_request))
        request["args"][2:4] = handles
        selected = subprocess.run(
            [str(binary), "--no-cache", "--json", "-C", str(project), *request["args"]],
            capture_output=True, text=True, timeout=120,
        )
        require(selected.returncode == 0,
                f"Handle-aware selection failed: {selected.stderr}")
        report = json.loads(selected.stdout)
        require(report["query"] == "select" and report["page"]["total"] == 2
                and all(item["status"] == "matched" for item in report["selections"]),
                "Handle-aware selection did not return both exact declarations")
        wrap = lambda value: json.dumps({"exit_code": 0, "result": value,
                                         "stdout_omitted_bytes": 0, "stderr": "",
                                         "stderr_omitted_bytes": 0})
        return wrap(find_report), request, wrap(report)


def current_write(event, request):
    old = json.loads(event["visible"])
    path = old["path"]
    if request["path"] == "manifest.json":
        artifact_root = str(Path(path).parent)
        request = json.loads(json.dumps(request))
        request["text"] = request["text"].replace('"artifacts/', f'"{artifact_root}/')
    return request, json.dumps({**old, "fr_reference": path})


def projected_events(binary):
    events = [json.loads(line) for line in (TRIAL / "events.jsonl").read_text().splitlines()]
    live_find, live_select_request, live_select = live_inspection(
        binary, events[6]["request"], events[8]["request"]
    )
    projected = []
    skill_paths = {0: ("SKILL.md", 80), 7: ("references/author.md", 160),
                   14: ("references/checks.md", 120), 29: ("references/history.md", 120),
                   30: ("references/git.md", 100)}
    for index, event in enumerate(events):
        if index in REMOVED:
            continue
        entry = {"original_event": index, "request": event["request"], "visible": event["visible"]}
        if index in skill_paths:
            entry["visible"] = skill_visible(*skill_paths[index])
        elif index == 6:
            entry["visible"] = live_find
        elif index == 8:
            entry["request"], entry["visible"] = live_select_request, live_select
        elif index in (18, 19, 20, 21):
            entry["request"], entry["visible"] = current_write(event, entry["request"])
        projected.append(entry)
    require(len(projected) == 29, "The prescribed projection must contain exactly 29 calls")
    require(all(json.loads(event["visible"]).get("exit_code", 0) == 0
                and not json.loads(event["visible"]).get("error") for event in projected),
            "The prescribed projection retained a refusal or failure")
    return events, projected


def metrics(events, prompt, encoding=None):
    result = {
        "calls": len(events),
        "prompt_bytes": len(prompt.encode()),
        "visible_output_bytes": sum(len(event["visible"].encode()) for event in events),
        "tool_request_bytes": sum(len(json.dumps(event["request"], ensure_ascii=False).encode())
                                  for event in events),
    }
    if encoding is not None:
        count = lambda value: len(encoding.encode(value, disallowed_special=()))
        visible = sum(count(event["visible"]) for event in events)
        result.update({
            "prompt_tokens": count(prompt),
            "visible_output_tokens": visible,
            "context_tokens": count(prompt) + visible,
            "tool_request_tokens": sum(count(json.dumps(event["request"], ensure_ascii=False))
                                       for event in events),
        })
    return result


def measure(binary, encoding=None):
    recorded = verify_evidence()
    binary = binary.resolve()
    require(binary.is_file(), f"Missing fr binary: {binary}")
    original, projected = projected_events(binary)
    old_prompt = (TRIAL / "prompt.txt").read_text()
    match = re.search(r"The task directory is (.+)/project\.\n", old_prompt)
    require(match, "Frozen prompt does not expose its session path")
    current_prompt = harness.prompt(Path(match.group(1)), regex_escape_len.TASK, "fr")
    observed = metrics(original, old_prompt, encoding)
    prescribed = metrics(projected, current_prompt, encoding)
    if encoding is not None:
        require(observed["context_tokens"] == recorded["context_tokens"],
                "Independent observed-token reconstruction disagrees with the retained score")
    return {
        "schema": "fr-agent-workflow-counterfactual-1",
        "passed": True,
        "binary": {"path": str(binary.relative_to(ROOT)), "sha256": digest(binary.read_bytes())},
        "frozen_trace": {"path": str((TRIAL / 'events.jsonl').relative_to(ROOT)),
                         "sha256": EVENTS_SHA256, "accepted": True},
        "tokenizer": ({"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                       "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"}
                      if encoding is not None else None),
        "observed": observed,
        "prescribed": prescribed,
        "difference": {key: prescribed[key] - observed[key] for key in observed},
        "removed_calls": [{"event": index, "reason": reason} for index, reason in REMOVED.items()],
        "replacement": {"event": 8, "query": "handle-aware project select",
                        "returned_declarations": 2},
        "measurement_files": {
            str(Path(__file__).relative_to(ROOT)): digest(Path(__file__).read_bytes()),
            "skills/fr/SKILL.md": digest((ROOT / "skills/fr/SKILL.md").read_bytes()),
            "skills/fr/references/author.md": digest((ROOT / "skills/fr/references/author.md").read_bytes()),
            "tools/agent-eval.py": digest((ROOT / "tools/agent-eval.py").read_bytes()),
        },
        "scope": "Prescribed counterfactual over one accepted frozen agent trace. It replaces one failed handle selection with live output from this binary, uses current skill and harness prompt payloads, corrects the first artifact manifest from returned references, and removes only calls made unnecessary by those delivered contracts. Source-changing and verification calls remain in their original order. This is not an autonomous trial, latency estimate, population result, or billed-token claim; an agent may choose a different path.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(args.fr, harness.tokenizer() if args.tokens else None),
                     indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
