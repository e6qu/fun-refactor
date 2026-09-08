#!/usr/bin/env python3
"""Compare prescribed batch and individual authoring workflows on a Rust fixture."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("agent_eval_harness", ROOT / "tools/agent-eval.py")
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)

ORIGINAL = {
    "main.rs": 'mod calc;\nfn main() { println!("{}", calc::evaluate(3)); }\n',
    "calc.rs": '// π\r\npub fn evaluate(n: i32) -> i32 { n + 1 }\r\n',
}
FRAGMENTS = {
    "helper.txt": "fn twice(n: i64) -> i64 { n * 2 }",
    "callee.txt": "pub fn evaluate(n: i64, extra: i64) -> i64 { twice(n) + extra }",
    "caller.txt": '{ println!("{}", calc::evaluate(3, 1)); }',
}
EXPECTED = {
    "main.rs": 'mod calc;\nfn main() { println!("{}", calc::evaluate(3, 1)); }\n',
    "calc.rs": '// π\r\npub fn evaluate(n: i64, extra: i64) -> i64 { twice(n) + extra }\r\nfn twice(n: i64) -> i64 { n * 2 }\r\n',
}
CHECKS = {"schema": 1, "checks": [{
    "name": "compile", "argv": ["rustc", "--edition=2021", "-D", "warnings", "main.rs", "-o", "../program"],
    "cwd": ".", "timeout_seconds": 60, "covers": ["Both source files compile with warnings denied"],
}]}
OPERATIONS = (("insert-declaration", "helper.txt"), ("replace-declaration", "callee.txt"), ("replace-body", "caller.txt"))
ARMS = ("individual", "batch")


def sizes(texts, encoding):
    return {"bytes": sum(len(text.encode()) for text in texts),
            "tokens": sum(len(encoding.encode(text, disallowed_special=())) for text in texts) if encoding else None}


def checked_payload(result):
    assert result.returncode == 0, (result.stdout, result.stderr)
    assert len(result.stdout) <= 20000 and len(result.stderr) <= 4096, "Harness output would be clipped"
    report = json.loads(result.stdout)
    assert isinstance(report, dict)
    visible = json.dumps({"exit_code": result.returncode, "result": report, "stdout_omitted_bytes": 0,
                          "stderr": result.stderr.decode(), "stderr_omitted_bytes": 0}, ensure_ascii=False)
    return report, visible


def require_sources(root, expected):
    actual = {path.relative_to(root).as_posix() for path in root.rglob("*.rs")
              if not any(part in (".git", ".fr-history") for part in path.relative_to(root).parts)}
    assert actual == set(expected), "Unexpected Rust source inventory"
    for name, text in expected.items():
        assert (root / name).read_bytes() == text.encode(), f"Unexpected source bytes: {name}"


def complete_diff(report):
    assert isinstance(report["diff"], str) and report["diff"], "Review diff must be complete and nonempty"
    assert report["changed"] is True and report["saved"] is True and report["applied"] is False
    assert isinstance(report["transaction"], int) and report["transaction"] > 0


def selection(report, path, name):
    assert report["page"]["total"] == 1 and report["page"]["next"] is None
    row = dict(zip(report["columns"], report["rows"][0]))
    assert row["path"] == path and row["name"] == name
    assert row["source"]["next_offset"] is None, "Selection source must be complete"
    return row["handle"], report["root"]


def metrics(events, encoding):
    groups = {}
    for phase in ("selection", "authoring", "history", "checks", "all"):
        chosen = [event for event in events if phase == "all" or event["phase"] == phase]
        groups[phase] = {"calls": len(chosen), "stdout": sizes([event["stdout"] for event in chosen], encoding),
                         "visible": sizes([event["visible"] for event in chosen], encoding),
                         "requests": sizes([json.dumps({"tool": "fr", "args": event["args"]}, ensure_ascii=False)
                                            for event in chosen], encoding)}
    projects = sum(event["args"][0] in ("project", "author") for event in events)
    return {"groups": groups, "project_commands": projects, "derived_scan_passes": projects * 2}


def workflow(binary, base, arm, encoding):
    root = base / "project"
    root.mkdir(parents=True)
    for name, text in ORIGINAL.items():
        (root / name).write_bytes(text.encode())
    (root / ".fr").mkdir()
    harness.save(root / ".fr/checks.json", CHECKS)
    harness.initialize(root)
    original = harness.snapshot(root)
    index = (root / ".git/index").read_bytes()
    artifacts = base / "fragments"
    artifacts.mkdir()
    for name, text in FRAGMENTS.items():
        (artifacts / name).write_bytes(text.encode())
    events, states, patches, transactions = [], [], [], []
    env = os.environ.copy()
    env["FUN_REFACTOR_CACHE"] = str(base / "unused-cache")

    def invoke(phase, args):
        argv = [str(binary), "--no-cache", "--json", "-C", str(root), *args]
        result = subprocess.run(argv, cwd=root, env=env, capture_output=True, timeout=180)
        report, visible = checked_payload(result)
        events.append({"phase": phase, "args": args, "argv": argv, "stdout": result.stdout.decode(), "visible": visible})
        assert (root / ".git/index").read_bytes() == index, "Git index changed"
        return report

    listing = invoke("checks", ["checks"])
    assert listing["executed"] is False and listing["checks"][0]["name"] == "compile"

    def validate(stage, expected, output):
        require_sources(root, expected)
        before = harness.snapshot(root)
        report = invoke("checks", ["checks", "--run", "compile", "--basis", listing["basis"], "--quiet-success", "--no-declarations"])
        assert report["passed"] is True and len(report["results"]) == 1
        run = subprocess.run([str(base / "program")], capture_output=True, timeout=10)
        assert run.returncode == 0 and run.stdout == output.encode() and run.stderr == b"", run
        assert harness.snapshot(root) == before, "Checks changed tracked files"
        states.append({"stage": stage, "snapshot": before, "behavior_stdout": run.stdout.decode(), "passed": True})

    def find(path, name):
        before = harness.snapshot(root)
        report = invoke("selection", ["project", "find", name, "--in", path, "--source", "--signature", "--bytes", "2048"])
        assert harness.snapshot(root) == before
        return selection(report, path, name)

    def save_apply(args):
        before = harness.snapshot(root)
        report = invoke("authoring", ["author", *args, "--diff-bytes", "65536", "--save-plan"])
        complete_diff(report)
        assert harness.snapshot(root) == before, "Saving changed tracked files"
        transaction = str(report["transaction"])
        assert transaction not in transactions
        transactions.append(transaction)
        invoke("history", ["history", "apply", transaction, "--write"])
        exported = invoke("history", ["history", "patch", transaction])
        assert isinstance(exported["patch"], str) and exported["patch"]
        patches.append(exported["patch"])

    validate("original", ORIGINAL, "4\n")
    if arm == "batch":
        callee, file_handle = find("calc.rs", "evaluate")
        caller, _ = find("main.rs", "main")
        manifest = {"operations": [{"op": op, "handle": handle, "from": f"../fragments/{name}"}
                                   for (op, name), handle in zip(OPERATIONS, (file_handle, callee, caller))]}
        harness.save(artifacts / "batch.json", manifest)
        save_apply(["batch", "--from", "../fragments/batch.json"])
    else:
        assert arm == "individual"
        for op, name in OPERATIONS:
            handle, file_handle = find("main.rs", "main") if name == "caller.txt" else find("calc.rs", "evaluate")
            save_apply([op, file_handle if op == "insert-declaration" else handle, "--from", f"../fragments/{name}"])
    validate("applied", EXPECTED, "7\n")
    changed = harness.snapshot(root)
    (root / "unrelated.txt").write_bytes(b"Preserve this independent later edit.\n")
    for transaction in reversed(transactions):
        invoke("history", ["history", "undo", transaction, "--write"])
    validate("undone", ORIGINAL, "4\n")
    assert harness.snapshot(root) == original
    for transaction in transactions:
        invoke("history", ["history", "redo", transaction, "--write"])
    validate("redone", EXPECTED, "7\n")
    assert harness.snapshot(root) == changed
    assert (root / "unrelated.txt").read_bytes() == b"Preserve this independent later edit.\n"
    receiver = base / "receiver"
    receiver.mkdir()
    for name, text in ORIGINAL.items():
        (receiver / name).write_bytes(text.encode())
    for patch in patches:
        harness.git(receiver, "apply", "--check", data=patch.encode())
        harness.git(receiver, "apply", data=patch.encode())
    require_sources(receiver, EXPECTED)
    compiled = subprocess.run(["rustc", "--edition=2021", "-D", "warnings", "main.rs", "-o", "../receiver-program"],
                              cwd=receiver, capture_output=True, timeout=60)
    assert compiled.returncode == 0, compiled.stderr
    run = subprocess.run([str(base / "receiver-program")], capture_output=True, timeout=10)
    assert run.returncode == 0 and run.stdout == b"7\n" and run.stderr == b""
    assert not (base / "unused-cache").exists()
    artifact_texts = {path.name: path.read_bytes().decode() for path in sorted(artifacts.iterdir())}
    return {"arm": arm, "passed": True, "events": events, "metrics": metrics(events, encoding),
            "artifacts": artifact_texts, "artifact_sizes": sizes(list(artifact_texts.values()), encoding),
            "transactions": transactions, "states": states, "patches": patches,
            "receiver_behavior_stdout": run.stdout.decode(), "receiver_matches": True,
            "index_unchanged": True, "sentinel_preserved": True, "cache_absent": True}


def measure(binary, repetitions, encoding):
    assert 1 <= repetitions <= 8
    binary_sha = harness.digest(binary.read_bytes())
    runs = []
    with tempfile.TemporaryDirectory(prefix="fr-author-batch-context-") as tmp:
        for repetition in range(repetitions):
            order = ARMS if repetition % 2 == 0 else tuple(reversed(ARMS))
            pair = []
            for arm in order:
                report = workflow(binary, Path(tmp) / str(repetition + 1) / f"arm-{ARMS.index(arm)}", arm, encoding)
                runs.append({"repetition": repetition + 1, **report})
                pair.append(report)
            assert pair[0]["states"] == pair[1]["states"], "Arms must have identical validated source and behavior"
    assert harness.digest(binary.read_bytes()) == binary_sha, "Binary changed during measurement"
    summary = {}
    for arm in ARMS:
        selected = [run for run in runs if run["arm"] == arm]
        summary[arm] = {"samples": len(selected), "calls": selected[0]["metrics"]["groups"]["all"]["calls"],
                        "project_commands": selected[0]["metrics"]["project_commands"],
                        "derived_scan_passes": selected[0]["metrics"]["derived_scan_passes"],
                        "transactions": len(selected[0]["transactions"]),
                        "median_visible_bytes": statistics.median(run["metrics"]["groups"]["all"]["visible"]["bytes"] for run in selected),
                        "median_visible_tokens": statistics.median(run["metrics"]["groups"]["all"]["visible"]["tokens"] for run in selected) if encoding else None}
    sources = [Path(__file__).resolve(), ROOT / "tools/agent-eval.py", ROOT / "src/cli.rs", ROOT / "src/project.rs",
               ROOT / "src/project/author.rs", ROOT / "src/edit.rs", ROOT / "src/history.rs", ROOT / "Cargo.lock"]
    return {"schema": "fr-author-batch-context-1", "passed": True, "binary_sha256": binary_sha,
            "source_commit": harness.git(ROOT, "rev-parse", "HEAD").stdout.decode().strip(),
            "binary_path": str(binary), "repetitions": repetitions, "runtime": {"platform": platform.platform(),
            "python": platform.python_version(), "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip()},
            "measurement_files": {str(path.relative_to(ROOT)): harness.digest(path.read_bytes()) for path in sources},
            "fixture": {"original": ORIGINAL, "expected": EXPECTED, "fragments": FRAGMENTS, "checks": CHECKS},
            "tokenizer": {"package": "tiktoken", "version": "0.12.0", "encoding": "o200k_base",
                          "vocabulary_sha256": "446a9538cb6c348e3516120d7c08b09f57c36495e2acfffe59a5bf8b0cfb1a2d"} if encoding else None,
            "summary": summary, "runs": runs,
            "scan_accounting": "Derived, not instrumented: each successful project/author command calls with_project once (scan, index, Project::new), then Project::verify scans again. Counts exclude manifest discovery, source reads, fragment/destination parsing, history and check internals. Source hashes identify the inspected implementation.",
            "scope": "Prescribed synthetic two-file Rust task, rotating arm order, disabled fact cache. Both arms select complete source and save/apply complete diffs, compile and check behavior at four stages, export patches and check a separate receiver. Raw stdout and harness-style visible payloads are counted separately. Requests and prepared artifact sizes are reported separately; no artifact-write responses, skill reads, discovery, agent reasoning, total-task context, latency or cold-disk claim. Individual steps are only syntax-checked until the complete change is applied; intermediate states need not compile."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--repetitions", type=int, default=3, choices=range(1, 9))
    parser.add_argument("--tokens", action="store_true")
    args = parser.parse_args()
    print(json.dumps(measure(args.fr.resolve(strict=True), args.repetitions, harness.tokenizer() if args.tokens else None), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
