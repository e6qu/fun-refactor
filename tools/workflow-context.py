#!/usr/bin/env python3
"""Compare manual and manifest-driven delivery for one reviewed transaction."""

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
ARMS = ("manual", "workflow")
TOKEN_IDENTITIES = {
    32: hashlib.sha256(b"fr-workflow-token-32").hexdigest()[:32],
    64: hashlib.sha256(b"fr-workflow-token-64").hexdigest(),
}
SOURCE = "def before(value: int) -> int:\n    return value + 1\n"
CHECKS = {
    "schema": 1,
    "checks": [{
        "name": "syntax",
        "argv": ["python3", "-c", "compile(open('app.py').read(), 'app.py', 'exec')"],
        "cwd": ".",
        "timeout_seconds": 10,
        "covers": ["Python syntax"],
    }],
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical_token_text(text, roots):
    for root in roots:
        text = text.replace(str(root), "<ROOT>")
    text = re.sub(
        r"(?<![0-9a-f])(?:[0-9a-f]{64}|[0-9a-f]{32})(?![0-9a-f])",
        lambda match: TOKEN_IDENTITIES[len(match.group(0))],
        text,
    )
    text = re.sub(r'"elapsed_ms":\s*\d+', '"elapsed_ms":0', text)
    return text


def sizes(texts, encoding, roots):
    return {
        "bytes": sum(len(text.encode()) for text in texts),
        "tokens": sum(len(encoding.encode(canonical_token_text(text, roots),
                                          disallowed_special=())) for text in texts)
        if encoding else None,
    }


def invoke(binary, root, args):
    started = time.perf_counter()
    result = subprocess.run(
        [str(binary), "--json", "--no-cache", "-C", str(root), *args],
        capture_output=True,
        timeout=120,
        env=os.environ.copy(),
    )
    elapsed = time.perf_counter() - started
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    assert not result.stderr
    return json.loads(result.stdout), result.stdout.decode(), elapsed


def prepare(binary, root):
    root.mkdir()
    (root / "app.py").write_text(SOURCE)
    (root / ".fr").mkdir()
    (root / ".fr/checks.json").write_text(json.dumps(CHECKS, separators=(",", ":")))
    listing, _, _ = invoke(binary, root, ["checks"])
    plan, _, _ = invoke(binary, root, ["rename", "before", "after", "--save-plan"])
    transaction = str(plan["transaction"])
    shown, _, _ = invoke(binary, root, ["history", "show", transaction])
    context = shown["records"][0]["context_basis"]
    manifest = {
        "schema": 1,
        "transaction": plan["transaction"],
        "transaction-context-basis": context,
        "checks": {"basis": listing["basis"], "names": ["syntax"]},
        "exercise-reversal": True,
        "patch": {"output": "change.patch"},
        "check-output-bytes": 256,
    }
    return transaction, context, listing["basis"], manifest


def normalized(value):
    value = json.loads(json.dumps(value))
    def clean(item):
        if isinstance(item, dict):
            item.pop("elapsed_ms", None)
            item.pop("root", None)
            for child in item.values():
                clean(child)
        elif isinstance(item, list):
            for child in item:
                clean(child)
    clean(value)
    return value


def compact_check(report):
    return normalized({
        "basis": report["basis"],
        "source_revision": report["source_revision"],
        "source_snapshot_stable": report["source_snapshot_stable"],
        "passed": report["passed"],
        "results": report["results"],
        "recorded_evidence": report.get("recorded_evidence"),
    })


def manual(binary, root, transaction, context, check_basis):
    commands = [
        ["history", "apply", transaction, "--write", "--no-diff", "--context-basis", context],
        ["checks", "--run", "syntax", "--basis", check_basis, "--record-for", transaction,
         "--quiet-success", "--no-declarations", "--output-bytes", "256"],
        ["history", "undo", transaction, "--write", "--no-diff"],
        ["checks", "--run", "syntax", "--basis", check_basis, "--quiet-success",
         "--no-declarations", "--output-bytes", "256"],
        ["history", "redo", transaction, "--write", "--no-diff", "--context-basis", context],
        ["checks", "--run", "syntax", "--basis", check_basis, "--record-for", transaction,
         "--quiet-success", "--no-declarations", "--output-bytes", "256"],
        ["history", "patch", transaction, "--output", "change.patch"],
    ]
    reports, outputs, elapsed = [], [], []
    for command in commands:
        value, output, seconds = invoke(binary, root, command)
        reports.append(value)
        outputs.append(output)
        elapsed.append(seconds)
    stage_results = [
        normalized(reports[0]), compact_check(reports[1]), normalized(reports[2]),
        compact_check(reports[3]), normalized(reports[4]), compact_check(reports[5]),
        {"output": reports[6]["output"], "bytes": reports[6]["patch_bytes"],
         "sha256": reports[6]["patch_sha256"]},
    ]
    requests = [json.dumps({"tool": "fr", "args": command}, separators=(",", ":"))
                for command in commands]
    return reports, stage_results, outputs, requests, [], sum(elapsed)


def workflow(binary, root, manifest):
    control = root / ".fr-workflow"
    control.write_text(json.dumps(manifest, separators=(",", ":")))
    commands = [
        ["workflow", "--from", ".fr-workflow"],
        ["workflow", "--from", ".fr-workflow", "--write"],
    ]
    reports, outputs, elapsed = [], [], []
    for command in commands:
        value, output, seconds = invoke(binary, root, command)
        reports.append(value)
        outputs.append(output)
        elapsed.append(seconds)
    assert reports[0]["ready"] and not reports[0]["executed"]
    assert reports[1]["passed"] and reports[1]["transaction_status"] == "applied"
    stage_results = [normalized(stage["result"]) for stage in reports[1]["stages"]]
    requests = [json.dumps({"tool": "fr", "args": command}, separators=(",", ":"))
                for command in commands]
    return reports, stage_results, outputs, requests, [control.read_text()], sum(elapsed)


def state_identity(root):
    state = json.loads((root / ".fr-history/state.json").read_text())
    state.pop("root")
    return digest(json.dumps(state, sort_keys=True, separators=(",", ":")).encode())


def measure(binary, repetitions, encoding):
    assert 1 <= repetitions <= 8
    binary_sha = digest(binary.read_bytes())
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-workflow-context-") as directory:
        base = Path(directory)
        for repetition in range(repetitions):
            order = ARMS if repetition % 2 == 0 else tuple(reversed(ARMS))
            pair = {}
            roots = []
            prepared = {}
            for arm in order:
                root = base / f"r{repetition + 1}-{arm}"
                prepared[arm] = (root, *prepare(binary, root))
                roots.append(root)
            for arm in order:
                root, transaction, context, check_basis, manifest = prepared[arm]
                result = (manual(binary, root, transaction, context, check_basis)
                          if arm == "manual" else workflow(binary, root, manifest))
                reports, stage_results, outputs, requests, artifacts, seconds = result
                assert (root / "app.py").read_text().startswith("def after(")
                run = {
                    "repetition": repetition + 1,
                    "arm": arm,
                    "calls": len(requests),
                    "stdout": sizes(outputs, encoding, roots),
                    "requests": sizes(requests, encoding, roots),
                    "artifact": sizes(artifacts, encoding, roots),
                    "context": sizes([*requests, *artifacts, *outputs], encoding, roots),
                    "seconds": seconds,
                    "stage_identity": digest(json.dumps(stage_results, sort_keys=True).encode()),
                    "state_identity": state_identity(root),
                    "source_sha256": digest((root / "app.py").read_bytes()),
                    "patch_sha256": digest((root / "change.patch").read_bytes()),
                }
                pair[arm] = run
                runs.append(run)
            for key in ("stage_identity", "state_identity", "source_sha256", "patch_sha256"):
                assert pair["manual"][key] == pair["workflow"][key], (repetition, key)
    assert digest(binary.read_bytes()) == binary_sha
    summary = {}
    for arm in ARMS:
        selected = [run for run in runs if run["arm"] == arm]
        summary[arm] = {
            "samples": len(selected),
            "calls": selected[0]["calls"],
            "median_context_bytes": statistics.median(run["context"]["bytes"] for run in selected),
            "median_context_tokens": statistics.median(run["context"]["tokens"] for run in selected)
            if encoding else None,
            "median_seconds": statistics.median(run["seconds"] for run in selected),
        }
    sources = [Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "src/workflow.rs",
               ROOT / "src/history.rs", ROOT / "src/checks.rs", ROOT / "Cargo.lock"]
    return {
        "schema": "fr-workflow-context-1",
        "passed": True,
        "binary_sha256": binary_sha,
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "repetitions": repetitions,
        "runtime": {
            "platform": platform.platform(),
            "python": platform.python_version(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        },
        "measurement_files": {
            str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sources
        },
        "fixture": {"app.py": SOURCE, ".fr/checks.json": CHECKS},
        "tokenizer": {
            "package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
            "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d",
        } if encoding else None,
        "token_normalization": {
            "pattern": "temporary roots, elapsed milliseconds and isolated lowercase hexadecimal identities",
            "representatives": {str(length): value for length, value in TOKEN_IDENTITIES.items()},
            "scope": "token counts only; byte counts and semantic identities use original output",
        },
        "summary": summary,
        "runs": runs,
        "scope": "Prescribed post-review delivery on a generic Python fixture. Preparation, plan review and check listing are common setup and excluded. The manual arm makes seven calls. The workflow arm counts its manifest, preview and write calls. Both apply, check, undo, check, redo, check and export the same transaction. Normalized stage reports, final history, source and patch must match. Timings are local subprocess wall time. No agent, skill read, independent behavior oracle or population claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-workflow-context-1" and report["passed"]
    assert report["repetitions"] >= 1
    assert report["token_normalization"]["representatives"] == {
        str(length): value for length, value in TOKEN_IDENTITIES.items()
    }
    for name, expected in report["measurement_files"].items():
        assert digest((ROOT / name).read_bytes()) == expected, name
    for repetition in range(1, report["repetitions"] + 1):
        pair = {run["arm"]: run for run in report["runs"] if run["repetition"] == repetition}
        assert pair["manual"]["calls"] == 7 and pair["workflow"]["calls"] == 2
        for key in ("stage_identity", "state_identity", "source_sha256", "patch_sha256"):
            assert pair["manual"][key] == pair["workflow"][key]
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
        print(json.dumps(measure(args.fr.resolve(strict=True), args.repetitions, encoding), indent=2))


if __name__ == "__main__":
    main()
