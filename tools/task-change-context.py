#!/usr/bin/env python3
"""Compare composed task delivery with one reviewed task-change command."""

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
ARMS = ("composed", "task_change")
TOKEN_IDENTITIES = {
    32: hashlib.sha256(b"fr-task-change-token-32").hexdigest()[:32],
    64: hashlib.sha256(b"fr-task-change-token-64").hexdigest(),
}
SOURCE = """pub fn render(value: &str) -> String { value.trim().to_owned() }

pub fn display(value: &str) -> String { render(value) }
"""
FRAGMENT = "{ value.trim().to_uppercase() }\n"
CHECKS = {"schema": 1, "checks": [{
    "name": "syntax",
    "argv": ["rustc", "--crate-type", "lib", "src/lib.rs", "--emit", "metadata", "-o", "artifacts/check.rmeta"],
    "cwd": ".", "timeout_seconds": 30, "covers": ["Rust syntax and types"],
}]}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(text, roots):
    for root in roots:
        text = text.replace(str(root), "<ROOT>")
    text = re.sub(
        r"(?<![0-9a-f])(?:[0-9a-f]{64}|[0-9a-f]{32})(?![0-9a-f])",
        lambda match: TOKEN_IDENTITIES[len(match.group(0))], text)
    return re.sub(r'"elapsed_ms":\s*\d+', '"elapsed_ms":0', text)


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
        capture_output=True, timeout=120, env=os.environ.copy())
    elapsed = time.perf_counter() - started
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    output = result.stdout.decode() + visible_stderr(result.stderr)
    return json.loads(result.stdout), output, elapsed


def task_manifest():
    handle = {"request": "target", "pointer": "/rows/0/0"}
    return {
        "schema": "fr-project-task-1",
        "requests": [
            {"id": "target", "arguments": ["find", "render", "--signature", "--source", "--bytes", "2048"]},
            {"id": "callers", "arguments": ["calls", handle, "--direction", "incoming", "--limit", "8"]},
        ],
        "targets": [{"id": "render-body", "handle": handle, "op": "replace-body"}],
        "checks": ["syntax"],
        "delivery": {"exercise-reversal": True, "patch": "artifacts/change.patch"},
    }


def change_manifest(fragment):
    value = task_manifest()
    value["schema"] = "fr-task-change-1"
    value["targets"][0]["from"] = str(fragment)
    value["postconditions"] = {
        "files-changed": 1, "edits": 1, "changed-operations": 1,
        "paths-changed": ["src/lib.rs"],
    }
    value["delivery"]["check-output-bytes"] = 256
    return value


def write_json(path, value):
    path.write_text(json.dumps(value, separators=(",", ":")))
    return path.read_text()


def request(args):
    return json.dumps({"tool": "fr", "args": args}, separators=(",", ":"))


def normalized(value):
    value = json.loads(json.dumps(value))
    def clean(item):
        if isinstance(item, dict):
            for key in ["elapsed_ms", "root", "workflow_basis", "manifest_sha256"]:
                item.pop(key, None)
            for child in item.values():
                clean(child)
        elif isinstance(item, list):
            for child in item:
                clean(child)
    clean(value)
    return value


def composed(binary, root, base, fragment):
    task_path = base / "task.json"
    author_path = base / "author.json"
    workflow_path = base / "workflow.json"
    artifacts = [fragment.read_text(), write_json(task_path, task_manifest())]
    reports, outputs, requests, elapsed = [], [], [], []

    command = ["project", "task", "--from", str(task_path), "--report-bytes", "1048576"]
    task, output, seconds = invoke(binary, root, command)
    reports.append(task); outputs.append(output); requests.append(request(
        ["project", "task", "--from", "<TASK_MANIFEST>", "--report-bytes", "1048576"]))
    elapsed.append(seconds)

    author = task["author_manifest_template"]
    author["operations"][0]["from"] = str(fragment)
    author["postconditions"] = {
        "files-changed": 1, "edits": 1, "changed-operations": 1,
        "paths-changed": ["src/lib.rs"],
    }
    artifacts.append(write_json(author_path, author))
    command = ["author", "batch", "--from", str(author_path), "--diff-bytes", "65536"]
    preview, output, seconds = invoke(binary, root, command)
    reports.append(preview); outputs.append(output); requests.append(request(
        ["author", "batch", "--from", "<AUTHOR_MANIFEST>", "--diff-bytes", "65536"]))
    elapsed.append(seconds)
    command = [*command, "--save-plan", "--plan-basis", preview["plan_context_basis"]]
    saved, output, seconds = invoke(binary, root, command)
    reports.append(saved); outputs.append(output); requests.append(request(
        ["author", "batch", "--from", "<AUTHOR_MANIFEST>", "--diff-bytes", "65536",
         "--save-plan", "--plan-basis", "<PLAN_CONTEXT_BASIS>"]))
    elapsed.append(seconds)

    workflow = task["workflow_manifest_template"]
    workflow["transaction"] = saved["transaction"]
    workflow["transaction-context-basis"] = saved["transaction_context_basis"]
    workflow["check-output-bytes"] = 256
    artifacts.append(write_json(workflow_path, workflow))
    command = ["workflow", "--from", str(workflow_path)]
    reviewed, output, seconds = invoke(binary, root, command)
    reports.append(reviewed); outputs.append(output); requests.append(request(
        ["workflow", "--from", "<WORKFLOW_MANIFEST>"]))
    elapsed.append(seconds)
    command = [*command, "--write", "--basis", reviewed["workflow_basis"]]
    completed, output, seconds = invoke(binary, root, command)
    reports.append(completed); outputs.append(output); requests.append(request(
        ["workflow", "--from", "<WORKFLOW_MANIFEST>", "--write", "--basis", "<WORKFLOW_BASIS>"]))
    elapsed.append(seconds)
    return reports, outputs, requests, artifacts, sum(elapsed), completed


def task_change(binary, root, base, fragment):
    path = base / "task-change.json"
    artifacts = [fragment.read_text(), write_json(path, change_manifest(fragment))]
    preview_command = ["task-change", "--from", str(path)]
    preview, preview_output, preview_seconds = invoke(binary, root, preview_command)
    write_command = [*preview_command, "--write", "--basis", preview["task_change_basis"]]
    completed, completed_output, completed_seconds = invoke(binary, root, write_command)
    requests = [
        request(["task-change", "--from", "<TASK_CHANGE_MANIFEST>"]),
        request(["task-change", "--from", "<TASK_CHANGE_MANIFEST>", "--write", "--basis",
                 "<TASK_CHANGE_BASIS>"]),
    ]
    return ([preview, completed], [preview_output, completed_output], requests, artifacts,
            preview_seconds + completed_seconds, completed["workflow"])


def state_identity(root):
    state = json.loads((root / ".fr-history/state.json").read_text())
    state.pop("root")
    for record in state["records"]:
        record.pop("required_checks", None)
    return digest(json.dumps(state, sort_keys=True, separators=(",", ":")).encode())


def measure(binary, repetitions, encoding):
    binary_sha = digest(binary.read_bytes())
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-task-change-context-") as directory:
        base = Path(directory)
        for repetition in range(repetitions):
            pair = {}
            order = ARMS if repetition % 2 == 0 else tuple(reversed(ARMS))
            roots = []
            for arm in order:
                root = base / f"r{repetition + 1}-{arm}" / "project"
                control = root.parent / "control"
                (root / "src").mkdir(parents=True)
                (root / ".fr").mkdir()
                (root / "artifacts").mkdir()
                control.mkdir()
                (root / "src/lib.rs").write_text(SOURCE)
                (root / ".fr/checks.json").write_text(json.dumps(CHECKS, separators=(",", ":")))
                fragment = control / "render.fragment"
                fragment.write_text(FRAGMENT)
                roots.extend([root, control])
                result = (composed(binary, root, control, fragment) if arm == "composed"
                          else task_change(binary, root, control, fragment))
                reports, outputs, requests, artifacts, seconds, workflow = result
                assert workflow["passed"] and workflow["transaction_status"] == "applied"
                stages = [normalized(stage["result"]) for stage in workflow["stages"]]
                run = {
                    "repetition": repetition + 1, "arm": arm, "calls": len(requests),
                    "stdout": sizes(outputs, encoding, roots),
                    "requests": sizes(requests, encoding, roots),
                    "artifact": sizes(artifacts, encoding, roots),
                    "context": sizes([*requests, *artifacts, *outputs], encoding, roots),
                    "seconds": seconds,
                    "stage_identity": digest(json.dumps(stages, sort_keys=True).encode()),
                    "state_identity": state_identity(root),
                    "source_sha256": digest((root / "src/lib.rs").read_bytes()),
                    "patch_sha256": digest((root / "artifacts/change.patch").read_bytes()),
                    "required_checks_bound": "required_checks" in json.loads(
                        (root / ".fr-history/state.json").read_text())["records"][0],
                }
                pair[arm] = run
                runs.append(run)
            for key in ["stage_identity", "state_identity", "source_sha256", "patch_sha256"]:
                assert pair["composed"][key] == pair["task_change"][key], (repetition, key)
            assert not pair["composed"]["required_checks_bound"]
            assert pair["task_change"]["required_checks_bound"]
    assert digest(binary.read_bytes()) == binary_sha
    summary = {}
    for arm in ARMS:
        selected = [run for run in runs if run["arm"] == arm]
        summary[arm] = {
            "samples": len(selected), "calls": selected[0]["calls"],
            "median_context_bytes": statistics.median(run["context"]["bytes"] for run in selected),
            "median_context_tokens": statistics.median(run["context"]["tokens"] for run in selected),
            "median_seconds": statistics.median(run["seconds"] for run in selected),
        }
    sources = [Path(__file__).resolve(), ROOT / "tools/agent-eval.py",
               ROOT / "src/project/task_change.rs", ROOT / "src/project/task.rs",
               ROOT / "src/project/author.rs", ROOT / "src/workflow.rs",
               ROOT / "src/history.rs", ROOT / "src/checks.rs", ROOT / "Cargo.lock"]
    return {
        "schema": "fr-task-change-context-1", "passed": True,
        "binary_sha256": binary_sha,
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "repetitions": repetitions,
        "runtime": {"platform": platform.platform(), "python": platform.python_version(),
                    "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
        "measurement_files": {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sources},
        "fixture": {"src/lib.rs": SOURCE, ".fr/checks.json": CHECKS,
                    "render.fragment": FRAGMENT},
        "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                      "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"},
        "token_normalization": {
            "pattern": "temporary roots, elapsed milliseconds and isolated lowercase hexadecimal identities",
            "representatives": {str(length): value for length, value in TOKEN_IDENTITIES.items()},
        },
        "summary": summary, "runs": runs,
        "scope": "Prescribed generic Rust change. The composed arm counts task, author and workflow manifests plus five calls. The task-change arm counts one manifest and two calls. Both resolve the same task, author the same fragment, enforce the same postconditions, run the same reversal stages and produce equal source, patch and normalized history. Task change additionally binds required checks to its transaction. Counts exclude system context, hidden reasoning and billed usage. This is deterministic workflow evidence without an agent or population claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-task-change-context-1" and report["passed"]
    assert report["repetitions"] >= 1
    assert report["token_normalization"]["representatives"] == {
        str(length): value for length, value in TOKEN_IDENTITIES.items()}
    for name, expected in report["measurement_files"].items():
        assert digest((ROOT / name).read_bytes()) == expected, name
    for repetition in range(1, report["repetitions"] + 1):
        pair = {run["arm"]: run for run in report["runs"] if run["repetition"] == repetition}
        assert pair["composed"]["calls"] == 5 and pair["task_change"]["calls"] == 2
        assert not pair["composed"]["required_checks_bound"]
        assert pair["task_change"]["required_checks_bound"]
        for key in ["stage_identity", "state_identity", "source_sha256", "patch_sha256"]:
            assert pair["composed"][key] == pair["task_change"][key]
    for arm in ARMS:
        selected = [run for run in report["runs"] if run["arm"] == arm]
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
    parser.add_argument("--fr", type=Path, default=ROOT / "target/release/fr")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(1, 9))
    parser.add_argument("--tokens", action="store_true")
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    if args.audit:
        print(json.dumps({"passed": True, "schema": audit(args.audit)["schema"]}))
    else:
        encoding = HARNESS.tokenizer() if args.tokens else None
        assert encoding is not None, "task-change evidence requires --tokens"
        print(json.dumps(measure(args.fr.resolve(strict=True), args.repetitions, encoding), indent=2))


if __name__ == "__main__":
    main()
