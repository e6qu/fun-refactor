#!/usr/bin/env python3
"""Compare separate agent task discovery with one project task bundle."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)
ARMS = ("separate", "task")
COMMON = ("schema", "revision", "handle_prefix", "coverage", "context_basis", "context_omitted")
TOKEN_IDENTITIES = {
    32: hashlib.sha256(b"fr-task-bundle-token-32").hexdigest()[:32],
    64: hashlib.sha256(b"fr-task-bundle-token-64").hexdigest(),
}
SOURCE = """use std::fmt;

pub fn render(value: &str) -> String { value.trim().to_owned() }

pub fn display(value: &str) -> String { render(value) }
"""
CHECKS = {"schema": 1, "checks": [{
    "name": "unit", "argv": ["cargo", "test", "--lib"], "cwd": ".",
    "timeout_seconds": 60, "covers": ["render behavior"],
}]}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(text, roots):
    for root in roots:
        text = text.replace(str(root), "<ROOT>")
    return re.sub(
        r"(?<![0-9a-f])(?:[0-9a-f]{64}|[0-9a-f]{32})(?![0-9a-f])",
        lambda match: TOKEN_IDENTITIES[len(match.group(0))], text)


def sizes(texts, encoding, roots):
    return {
        "bytes": sum(len(text.encode()) for text in texts),
        "tokens": sum(len(encoding.encode(canonical(text, roots), disallowed_special=()))
                      for text in texts) if encoding else None,
    }


def visible_stderr(data):
    text = data.decode()
    for line in text.splitlines():
        value = json.loads(line)
        progress = value.get("indexing") if isinstance(value, dict) else None
        assert isinstance(progress, dict) and set(progress) == {"done", "total"}
    return text


def invoke(binary, root, args):
    started = time.perf_counter()
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), *args],
        capture_output=True, env=os.environ.copy(), timeout=120)
    elapsed = time.perf_counter() - started
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    output = result.stdout.decode() + visible_stderr(result.stderr)
    return json.loads(result.stdout), output, elapsed


def compact(report):
    return {key: value for key, value in report.items() if key not in COMMON}


def manifest():
    handle = {"request": "target", "pointer": "/rows/0/0"}
    return {
        "schema": "fr-project-task-1",
        "requests": [
            {"id": "target", "arguments": ["find", "render", "--signature", "--source", "--bytes", "2048"]},
            {"id": "callers", "arguments": ["calls", handle, "--direction", "incoming", "--limit", "8"]},
        ],
        "targets": [{"id": "render-body", "handle": handle, "op": "replace-body"}],
        "checks": ["unit"],
        "delivery": {"exercise-reversal": True, "patch": "artifacts/change.patch"},
    }


def semantic_identity(query_reports, handle, operation, check_basis, check_names, coverage):
    return digest(json.dumps({
        "queries": [compact(report) for report in query_reports],
        "handle": handle, "operation": operation, "check_basis": check_basis,
        "check_names": check_names, "coverage": coverage,
    }, sort_keys=True, separators=(",", ":")).encode())


def separate(binary, root, encoding, roots):
    commands = [["project", "find", "render", "--signature", "--source", "--bytes", "2048"]]
    reports, outputs, seconds = [], [], []
    found, output, elapsed = invoke(binary, root, commands[0])
    reports.append(found); outputs.append(output); seconds.append(elapsed)
    handle = found["rows"][0][0]
    basis = found["context_basis"]
    commands.append(["project", "calls", handle, "--direction", "incoming", "--limit", "8",
                     "--context-basis", basis])
    callers, output, elapsed = invoke(binary, root, commands[-1])
    reports.append(callers); outputs.append(output); seconds.append(elapsed)
    commands.append(["author", "guide"])
    guide, output, elapsed = invoke(binary, root, commands[-1])
    outputs.append(output); seconds.append(elapsed)
    commands.append(["checks"])
    checks, output, elapsed = invoke(binary, root, commands[-1])
    outputs.append(output); seconds.append(elapsed)
    assert any(operation["op"] == "replace-body" for operation in guide["operations"])
    selected = next(check for check in checks["checks"] if check["name"] == "unit")
    shown = [["<CONTEXT_BASIS>" if argument == basis else argument for argument in command]
             for command in commands]
    requests = [json.dumps({"tool": "fr", "args": command}, separators=(",", ":"))
                for command in shown]
    identity = semantic_identity(reports, handle, "replace-body", checks["basis"], ["unit"],
                                 [{"name": "unit", "covers": selected["covers"]}])
    return {"calls": 4, "semantic_sha256": identity, "stdout": sizes(outputs, encoding, roots),
            "requests": sizes(requests, encoding, roots), "artifact": sizes([], encoding, roots),
            "context": sizes([*requests, *outputs], encoding, roots), "seconds": sum(seconds)}


def task(binary, root, input_path, encoding, roots):
    command = ["project", "task", "--from", "<TASK_MANIFEST>", "--report-bytes", "1048576"]
    report, output, seconds = invoke(binary, root, ["project", "task", "--from", str(input_path),
                                                     "--report-bytes", "1048576"])
    assert report["report_budget"]["omitted_requests"] == 0
    query_reports = [request["report"] for request in report["requests"]]
    identity = semantic_identity(query_reports, report["targets"][0]["handle"],
                                 report["targets"][0]["operation"], report["checks"]["basis"],
                                 report["checks"]["names"], report["checks"]["coverage"])
    request = json.dumps({"tool": "fr", "args": command}, separators=(",", ":"))
    artifact = input_path.read_text()
    return {"calls": 1, "semantic_sha256": identity, "stdout": sizes([output], encoding, roots),
            "requests": sizes([request], encoding, roots), "artifact": sizes([artifact], encoding, roots),
            "context": sizes([request, artifact, output], encoding, roots), "seconds": seconds}


def measure(binary, repetitions, encoding):
    assert 1 <= repetitions <= 8
    binary_sha = digest(binary.read_bytes())
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-task-bundle-context-") as directory:
        base = Path(directory)
        root = base / "project"
        (root / "src").mkdir(parents=True)
        (root / ".fr").mkdir()
        (root / "src/lib.rs").write_text(SOURCE)
        (root / ".fr/checks.json").write_text(json.dumps(CHECKS, separators=(",", ":")))
        input_path = base / "task.json"
        input_path.write_text(json.dumps(manifest(), separators=(",", ":")))
        original = {path: path.read_bytes() for path in root.rglob("*") if path.is_file()}
        roots = [base, root, input_path]
        for repetition in range(repetitions):
            order = ARMS if repetition % 2 == 0 else tuple(reversed(ARMS))
            pair = {}
            for arm in order:
                result = separate(binary, root, encoding, roots) if arm == "separate" else task(
                    binary, root, input_path, encoding, roots)
                pair[arm] = result
                runs.append({"repetition": repetition + 1, "arm": arm, **result})
            assert pair["separate"]["semantic_sha256"] == pair["task"]["semantic_sha256"]
        assert original == {path: path.read_bytes() for path in root.rglob("*") if path.is_file()}
    assert digest(binary.read_bytes()) == binary_sha
    summary = {}
    for arm in ARMS:
        selected = [run for run in runs if run["arm"] == arm]
        summary[arm] = {
            "samples": len(selected), "calls": selected[0]["calls"],
            "median_context_bytes": statistics.median(run["context"]["bytes"] for run in selected),
            "median_context_tokens": statistics.median(run["context"]["tokens"] for run in selected)
            if encoding else None,
            "median_seconds": statistics.median(run["seconds"] for run in selected),
        }
    sources = [Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "src/project/task.rs",
               ROOT / "src/project/batch.rs", ROOT / "src/checks.rs", ROOT / "Cargo.lock"]
    return {
        "schema": "fr-task-bundle-context-1", "passed": True, "binary_sha256": binary_sha,
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "repetitions": repetitions,
        "runtime": {"platform": platform.platform(), "python": platform.python_version(),
                    "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
        "measurement_files": {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sources},
        "fixture": {"src/lib.rs": SOURCE, ".fr/checks.json": CHECKS}, "manifest": manifest(),
        "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                      "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"}
        if encoding else None,
        "token_normalization": {"pattern": "isolated lowercase hexadecimal identities of length 32 or 64",
                                "representatives": {str(key): value for key, value in TOKEN_IDENTITIES.items()}},
        "summary": summary, "runs": runs, "source_unchanged": True,
        "scope": "Prescribed generic Rust fixture. Four separate calls discover a target and callers, read the author guide and list declared checks. One task call returns equivalent query reports, target operation and selected check evidence; its manifest bytes are counted. Both arms stop before fragment creation or mutation. Token counts exclude system context, hidden reasoning, cache effects and billed usage. This is serialization and call-count evidence, not an agent or population claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-task-bundle-context-1" and report["passed"]
    assert report["source_unchanged"] and report["repetitions"] >= 1
    assert report["token_normalization"]["representatives"] == {
        str(key): value for key, value in TOKEN_IDENTITIES.items()}
    for name, expected in report["measurement_files"].items():
        assert digest((ROOT / name).read_bytes()) == expected, name
    separate_runs = sorted((run for run in report["runs"] if run["arm"] == "separate"),
                           key=lambda run: run["repetition"])
    task_runs = sorted((run for run in report["runs"] if run["arm"] == "task"),
                       key=lambda run: run["repetition"])
    assert len(separate_runs) == len(task_runs) == report["repetitions"]
    for left, right in zip(separate_runs, task_runs):
        assert left["semantic_sha256"] == right["semantic_sha256"]
        assert left["calls"] == 4 and right["calls"] == 1
    for arm, selected in (("separate", separate_runs), ("task", task_runs)):
        summary = report["summary"][arm]
        assert summary["samples"] == len(selected) and summary["calls"] == selected[0]["calls"]
        assert summary["median_context_bytes"] == statistics.median(
            run["context"]["bytes"] for run in selected)
        assert summary["median_context_tokens"] == statistics.median(
            run["context"]["tokens"] for run in selected)
        assert summary["median_seconds"] == statistics.median(run["seconds"] for run in selected)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(1, 9))
    parser.add_argument("--tokens", action="store_true")
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.audit:
        print(json.dumps({"passed": True, "schema": audit(args.audit)["schema"]}))
    else:
        encoding = HARNESS.tokenizer() if args.tokens else None
        print(json.dumps(measure(args.fr.resolve(strict=True), args.repetitions, encoding), indent=2))


if __name__ == "__main__":
    main()
