#!/usr/bin/env python3
"""Compare history completion reports for identical retained edits, without running agents."""

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)
EVIDENCE = ROOT / "tests/agent-eval/results/2026-09-07-context"


def run(binary, root, *args):
    result = subprocess.run([str(binary), "-C", str(root), "--json", "--no-cache", *args],
                            capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stderr.decode() + result.stdout.decode())
    return result.stdout.decode(), json.loads(result.stdout)


def measure(binary, task, no_diff, directory):
    root = directory / "project"
    harness.unpack(root)
    harness.initialize(root)
    original = harness.snapshot(root)
    initial_index = (root / ".git/index").read_bytes()
    events = [json.loads(line) for line in (EVIDENCE / f"{task}-fr/events.jsonl").read_text().splitlines()]
    fragments = [e["request"]["text"] for e in events if e["request"]["tool"] == "write"]
    assert len(fragments) == 1
    fragment = directory / "fragment.rs"
    fragment.write_text(fragments[0])
    if task == "unicode-dice":
        _, query = run(binary, root, "project", "find", "sorensen_dice", "--in", "src/lib.rs")
        operation = "replace-body"
    else:
        _, query = run(binary, root, "project", "map", "src/lib.rs", "--depth", "0", "--limit", "1", "--fields", "handle")
        operation = "insert-declaration"
    assert len(query["rows"]) == 1
    handle = dict(zip(query["columns"], query["rows"][0]))["handle"]
    _, plan = run(binary, root, "author", operation, handle, "--from", str(fragment), "--save-plan")
    tx = str(plan["transaction"])
    assert plan["saved"] and not plan["applied"] and harness.snapshot(root) == original
    reports = []
    final = None
    for action in ("apply", "undo", "redo"):
        if action != "apply":
            text, preview = run(binary, root, "history", action, tx)
            assert not preview["applied"] and all("diff" in change for change in preview["changes"])
            reports.append({"action": action, "write": False, "stdout": text})
        text, report = run(binary, root, "history", action, tx, "--write", *(["--no-diff"] if no_diff else []))
        assert report["applied"] and report["action"] == action and report["transaction"] == int(tx)
        reports.append({"action": action, "write": True, "stdout": text})
        current = harness.snapshot(root)
        if action == "apply":
            final = current
            assert current["src/lib.rs"] == events[-1]["after"]["src/lib.rs"]
            assert [path for path in original if original[path] != final[path]] == ["src/lib.rs"]
            (root / "unrelated.txt").write_text("Preserve this later edit.\n")
        else:
            assert current == (original if action == "undo" else final)
            assert (root / "unrelated.txt").read_text() == "Preserve this later edit.\n"
        assert (root / ".git/index").read_bytes() == initial_index
    _, patch = run(binary, root, "history", "patch", tx)
    assert patch["patch"] == (EVIDENCE / f"{task}-fr/change.patch").read_text()
    return reports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--tokens", action="store_true", help="Use the optional pinned acceptance tokenizer.")
    args = parser.parse_args()
    binary = args.fr.resolve(strict=True)
    enc = harness.tokenizer() if args.tokens else None
    manifest = json.loads((EVIDENCE / "manifest.json").read_text())
    for name, sha in manifest["files"].items():
        assert harness.digest(harness.within(EVIDENCE, name).read_bytes()) == sha, name
    results = []
    with tempfile.TemporaryDirectory(prefix="fr-history-context-") as tmp:
        for task in ("unicode-dice", "normalized-osa"):
            outputs = {}
            for mode in ("default", "no_diff"):
                directory = Path(tmp) / f"{task}-{mode}"
                directory.mkdir()
                outputs[mode] = measure(binary, task, mode == "no_diff", directory)
            for full, smaller in zip(outputs["default"], outputs["no_diff"]):
                expected = json.loads(full["stdout"])
                if full["write"]:
                    for change in expected["changes"]:
                        del change["diff"]
                    expected["diffs_omitted"] = True
                assert json.loads(smaller["stdout"]) == expected
            measures = {}
            for mode, reports in outputs.items():
                completion = [r["stdout"] for r in reports if r["write"]]
                transition = [r["stdout"] for r in reports]
                measures[mode] = {"completion_bytes": sum(len(s.encode()) for s in completion),
                                  "with_previews_bytes": sum(len(s.encode()) for s in transition),
                                  "completion_tokens": sum(len(enc.encode(s, disallowed_special=())) for s in completion) if enc else None,
                                  "with_previews_tokens": sum(len(enc.encode(s, disallowed_special=())) for s in transition) if enc else None}
            assert measures["no_diff"]["completion_bytes"] < measures["default"]["completion_bytes"]
            results.append({"task": task, "passed": True, "measures": measures, "reports": outputs})
    print(json.dumps({"schema": "fr-history-context-1", "binary_sha256": harness.digest(binary.read_bytes()),
                      "evidence_manifest_sha256": harness.digest((EVIDENCE / "manifest.json").read_bytes()),
                      "tokenizer": json.loads((EVIDENCE / "unicode-dice-fr/result.json").read_text())["tokenizer"] if enc else None,
                      "scope": "Three completion reports and two undo/redo previews per task; direct CLI stdout only. Identical retained edits, no agents or total-task context measurement.",
                      "results": results}, indent=2))


if __name__ == "__main__":
    main()
