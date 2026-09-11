#!/usr/bin/env python3
"""Compare equivalent separate and batched project queries on a generic fixture."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)
POLICIES = ("cold", "warm")
ARMS = ("separate", "batch")
FILES = {
    "Cargo.toml": """[package]\nname = \"context-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n""",
    "src/lib.rs": """pub struct Request { pub value: String }\n\npub fn normalize(value: &str) -> String { value.trim().to_lowercase() }\n\npub fn process_request(request: &Request) -> String { normalize(&request.value) }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn request_is_normalized() {\n        assert_eq!(process_request(&Request { value: \" A \".into() }), \"a\");\n    }\n}\n""",
    "web/client.ts": "export function present(value: string): string { return value.toUpperCase(); }\n",
    "scripts/check.py": "def validate(value: str) -> bool:\n    return bool(value.strip())\n",
}
COMMON = ("schema", "revision", "handle_prefix", "coverage", "context_basis", "context_omitted")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def compact(report):
    return {key: value for key, value in report.items() if key not in COMMON}


def sizes(texts, encoding):
    return {
        "bytes": sum(len(text.encode()) for text in texts),
        "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts)
        if encoding else None,
    }


def invoke(binary, root, cache, policy, args):
    command = [str(binary), "--json", "-C", str(root)]
    if policy == "cold":
        command.append("--no-cache")
    command.extend(args)
    environment = os.environ.copy()
    environment["FUN_REFACTOR_CACHE"] = str(cache)
    started = time.perf_counter()
    result = subprocess.run(command, capture_output=True, env=environment, timeout=120)
    elapsed = time.perf_counter() - started
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    assert not result.stderr
    return json.loads(result.stdout), result.stdout.decode(), elapsed


def manifest():
    handle = {"request": "lookup", "pointer": "/rows/0/0"}
    return {
        "schema": "fr-project-batch-1",
        "requests": [
            {"id": "structure", "arguments": ["map", ".", "--depth", "2", "--limit", "24",
                                                        "--fields", "handle,parent,kind,name,path,line,children,language,signature"]},
            {"id": "lookup", "arguments": ["find", "process_request", "--signature"]},
            {"id": "source", "arguments": ["show", handle, "--source", "--bytes", "512"]},
            {"id": "callers", "arguments": ["calls", handle, "--direction", "incoming", "--limit", "8"]},
            {"id": "tests", "arguments": ["tests", handle, "--depth", "3", "--limit", "8"]},
            {"id": "packages", "arguments": ["packages", "--limit", "8"]},
            {"id": "dependencies", "arguments": ["dependencies", "--manifest", "Cargo.toml", "--limit", "12"]},
            {"id": "gaps", "arguments": ["gaps", "--limit", "8"]},
        ],
    }


def separate(binary, root, cache, policy, encoding):
    requests = [
        ["project", "map", ".", "--depth", "2", "--limit", "24", "--fields",
         "handle,parent,kind,name,path,line,children,language,signature"],
        ["project", "find", "process_request", "--signature"],
    ]
    reports, outputs, elapsed = [], [], []
    report, output, seconds = invoke(binary, root, cache, policy, requests[0])
    basis = report["context_basis"]
    reports.append(compact(report)); outputs.append(output); elapsed.append(seconds)
    report, output, seconds = invoke(binary, root, cache, policy, [*requests[1], "--context-basis", basis])
    handle = report["rows"][0][0]
    reports.append(compact(report)); outputs.append(output); elapsed.append(seconds)
    for args in [
        ["project", "show", handle, "--source", "--bytes", "512"],
        ["project", "calls", handle, "--direction", "incoming", "--limit", "8"],
        ["project", "tests", handle, "--depth", "3", "--limit", "8"],
        ["project", "packages", "--limit", "8"],
        ["project", "dependencies", "--manifest", "Cargo.toml", "--limit", "12"],
        ["project", "gaps", "--limit", "8"],
    ]:
        requests.append(args)
        report, output, seconds = invoke(binary, root, cache, policy, [*args, "--context-basis", basis])
        reports.append(compact(report)); outputs.append(output); elapsed.append(seconds)
    request_texts = [json.dumps({"tool": "fr", "args": args}, separators=(",", ":")) for args in requests]
    context = sizes([*request_texts, *outputs], encoding)
    return {"reports": reports, "report_sha256": [digest(json.dumps(report, sort_keys=True).encode()) for report in reports],
            "calls": len(requests), "stdout": sizes(outputs, encoding), "requests": sizes(request_texts, encoding),
            "artifact": sizes([], encoding), "context": context, "seconds": sum(elapsed)}


def batch(binary, root, cache, policy, input_path, encoding):
    args = ["project", "batch", "--from", "<MANIFEST>", "--report-bytes", "1048576"]
    actual = ["project", "batch", "--from", str(input_path), "--report-bytes", "1048576"]
    report, output, seconds = invoke(binary, root, cache, policy, actual)
    assert report["report_budget"]["omitted_requests"] == 0
    assert all(request["status"] == "returned" for request in report["requests"])
    reports = [request["report"] for request in report["requests"]]
    request_text = json.dumps({"tool": "fr", "args": args}, separators=(",", ":"))
    artifact_text = input_path.read_text()
    context = sizes([request_text, artifact_text, output], encoding)
    return {"reports": reports, "report_sha256": [digest(json.dumps(item, sort_keys=True).encode()) for item in reports],
            "calls": 1, "stdout": sizes([output], encoding), "requests": sizes([request_text], encoding),
            "artifact": sizes([artifact_text], encoding), "context": context, "seconds": seconds}


def measure(binary, repetitions, encoding):
    assert 1 <= repetitions <= 8
    binary_sha = digest(binary.read_bytes())
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-project-batch-context-") as directory:
        base = Path(directory)
        root = base / "project"
        for path, text in FILES.items():
            destination = root / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(text)
        input_path = base / "queries.json"
        input_path.write_text(json.dumps(manifest(), separators=(",", ":")))
        source_before = {path: (root / path).read_bytes() for path in FILES}
        for repetition in range(repetitions):
            for policy in POLICIES:
                order = ARMS if repetition % 2 == 0 else tuple(reversed(ARMS))
                pair = {}
                for arm in order:
                    cache = base / "cache" / str(repetition) / policy / arm
                    if policy == "warm":
                        invoke(binary, root, cache, policy, ["project", "map", "--depth", "0", "--limit", "1"])
                    result = (separate(binary, root, cache, policy, encoding) if arm == "separate"
                              else batch(binary, root, cache, policy, input_path, encoding))
                    result.pop("reports")
                    pair[arm] = result
                    runs.append({"repetition": repetition + 1, "policy": policy, "arm": arm, **result})
                assert pair["separate"]["report_sha256"] == pair["batch"]["report_sha256"]
        assert source_before == {path: (root / path).read_bytes() for path in FILES}
    assert digest(binary.read_bytes()) == binary_sha
    summary = {}
    for policy in POLICIES:
        summary[policy] = {}
        for arm in ARMS:
            selected = [run for run in runs if run["policy"] == policy and run["arm"] == arm]
            summary[policy][arm] = {
                "samples": len(selected), "calls": selected[0]["calls"],
                "median_context_bytes": statistics.median(run["context"]["bytes"] for run in selected),
                "median_context_tokens": statistics.median(run["context"]["tokens"] for run in selected) if encoding else None,
                "median_seconds": statistics.median(run["seconds"] for run in selected),
            }
    sources = [Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "src/project.rs",
               ROOT / "src/project/batch.rs", ROOT / "Cargo.lock"]
    return {
        "schema": "fr-project-batch-context-1", "passed": True, "binary_sha256": binary_sha,
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "repetitions": repetitions, "runtime": {"platform": platform.platform(), "python": platform.python_version(),
                                                    "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
        "measurement_files": {str(path.relative_to(ROOT)): digest(path.read_bytes()) for path in sources},
        "fixture": FILES, "manifest": manifest(),
        "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                      "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"} if encoding else None,
        "summary": summary, "runs": runs, "source_unchanged": True,
        "scope": "Prescribed generic Rust, TypeScript and Python fixture. Eight standalone project calls reuse context_basis; one batch uses a backward handle reference. Batch context includes its request manifest. Reports match after removing the standalone compact-context markers. Cold runs disable the fact cache; warm runs prefill separate per-arm caches. Rotating order does not control OS filesystem cache. Timings are local subprocess wall time. No agent, skill read, task success or population claim.",
    }


def audit(path):
    report = json.loads(path.read_text())
    assert report["schema"] == "fr-project-batch-context-1" and report["passed"]
    assert report["source_unchanged"] and report["repetitions"] >= 1
    for name, expected in report["measurement_files"].items():
        assert digest((ROOT / name).read_bytes()) == expected, name
    for policy in POLICIES:
        separate_runs = [run for run in report["runs"] if run["policy"] == policy and run["arm"] == "separate"]
        batch_runs = [run for run in report["runs"] if run["policy"] == policy and run["arm"] == "batch"]
        assert len(separate_runs) == len(batch_runs) == report["repetitions"]
        for left, right in zip(sorted(separate_runs, key=lambda run: run["repetition"]),
                               sorted(batch_runs, key=lambda run: run["repetition"])):
            assert left["report_sha256"] == right["report_sha256"] and left["calls"] == 8 and right["calls"] == 1
        for arm, selected in (("separate", separate_runs), ("batch", batch_runs)):
            summary = report["summary"][policy][arm]
            assert summary["samples"] == len(selected) and summary["calls"] == selected[0]["calls"]
            assert summary["median_context_bytes"] == statistics.median(run["context"]["bytes"] for run in selected)
            assert summary["median_context_tokens"] == statistics.median(run["context"]["tokens"] for run in selected)
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
