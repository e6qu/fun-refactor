#!/usr/bin/env python3
"""Retain Rust structural delivery and independent compiled receiver evidence."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/"sdk/python/src"))
from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_delivery import DeliveryReceipt, run_delivery
from fr_ir.ir import TaskChange, TaskDelivery
from fr_ir.runtime import FrClient, FrRuntimeError
from fr_ir.structural_change import RefactorRequest

FIXTURE = ROOT/"tests/agent-eval/structural-change"
FILES = ("pricing.rs", "api.rs", "main.rs")
BINDINGS = ("tools/structural-change-acceptance.py", "src/project/structural_change.rs",
    "src/project/author.rs", "src/project/task.rs", "src/project/task_change.rs",
    "src/project/change_scope.rs", "src/refactor/rename.rs", "src/refactor/signature.rs",
    "src/project.rs", "src/parse.rs", "src/index.rs", "src/model.rs",
    "sdk/python/src/fr_ir/structural_change.py", "sdk/python/src/fr_ir/change_scope.py",
    "sdk/python/src/fr_ir/ir.py", "sdk/python/src/fr_ir/runtime.py",
    "sdk/python/src/fr_ir/investigation_delivery.py", "kernels/FrKernels/Project.lean")


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+"\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, root):
    return subprocess.run(args, cwd=root, capture_output=True, text=True, check=True, timeout=180)


def populate(root, fixture):
    root.mkdir(parents=True)
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    for name in FILES:
        shutil.copyfile(fixture/name, root/name)
    shutil.copyfile(fixture/"oracle.py", root/".fr/oracle.py")
    save(root/".fr/checks.json", {"schema": 1, "checks": [{"name": "behavior",
        "argv": [sys.executable, "-B", ".fr/oracle.py"], "cwd": ".", "timeout_seconds": 30,
        "covers": ["49 compiled public results"], "identity_files": [".fr/oracle.py"]}]})
    save(root/".fr/check-scopes.json", {"schema": 1, "checks": [{"name": "behavior", "paths": list(FILES)}]})
    run(["git", "init", "-q"], root)
    run(["git", "add", "--", *FILES], root)


def oracle(root):
    return json.loads(run([sys.executable, "-B", ".fr/oracle.py"], root).stdout)


def sources(root):
    return {name: (root/name).read_bytes() for name in FILES}


def deliver(binary, project, output, scope_root):
    client = FrClient(project, executable=binary)
    store = DirectoryObjectStore(output/"objects")
    scope = ChangeScope.restore(store, scope_root)
    handle = client.project("find", "subtotal").definition_target().handle
    targets = [RefactorRequest.rename("sum_values").target("rename", handle),
               RefactorRequest.remove_parameter(2).target("remove", handle)]
    change = scope.bind(TaskChange([], targets, {"files-changed": 2,
        "paths-changed": ["pricing.rs", "api.rs"], "changed-operations": 2},
        ["behavior"], TaskDelivery(patch="artifacts/change.patch"), acceptance_checks=["behavior"]))
    review = client.review(change)
    save(output/"review.json", review.to_data())
    plan = TaskPlan("Migrate the scalar API", ("public results preserved",), (
        TaskStep.checked("outcome", "Are public results preserved?", checks=("behavior",),
                         satisfies=("public results preserved",)),))
    result = run_delivery(plan, client, "outcome", review, store)
    assert result.passed and result.resumed.complete
    save(output/"delivery.json", result.receipt.result.to_data())
    return {"passed": True, "receipt_root": result.receipt_root, "plan_root": result.plan_root,
            "started_plan_root": result.started_plan_root, "scope_root": scope_root}


def measure(binary, output):
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(FIXTURE, output/"fixture")
    for name in BINDINGS:
        destination = output/"binding-sources"/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT/name, destination)
    result: dict[str, Any] = {"schema": "fr-structural-change-acceptance-1"}
    with tempfile.TemporaryDirectory(prefix="fr-structural-acceptance-") as temporary:
        base = Path(temporary)
        for arm in ("ordinary", "native"):
            root = base/arm
            populate(root, FIXTURE)
            baseline = oracle(root)
            started = time.monotonic()
            if arm == "ordinary":
                for name in ("pricing.rs", "api.rs"):
                    source = (root/name).read_text().replace("subtotal", "sum_values")
                    source = source.replace(", _legacy: i64", "").replace(", 0)", ")")
                    (root/name).write_text(source)
                metrics = {"source_files": 2, "source_bytes": sum((FIXTURE/name).stat().st_size for name in ("pricing.rs", "api.rs"))}
            else:
                client = FrClient(root, executable=binary)
                handle = client.project("find", "subtotal").definition_target().handle
                scope = ChangeScope.inspect(client, [handle], max_bytes=262144)
                assert scope.ready, scope.report.to_data()
                references = [r for r in scope.report.at("/references") if r["target"]["name"] == "subtotal"]
                assert len(references) == 49
                for reference in references:
                    item = reference["occurrence"]
                    span = item["location"]["span"]
                    assert (root/item["path"]).read_bytes()[span["start"]:span["end"]] == b"subtotal"
                scope_root = scope.persist(DirectoryObjectStore(output/"objects"))
                args = [sys.executable, str(Path(__file__).resolve()), "--fr", str(binary),
                        "--project", str(root), "--reopen", str(output), "--scope", scope_root]
                metrics = json.loads(run(args, ROOT).stdout)
                plan = TaskPlan.restore(DirectoryObjectStore(output/"objects"), metrics["plan_root"])
                assert plan.resume(FrClient(root, executable=binary)).complete
                old_target = RefactorRequest.rename("another").target("stale", handle)
                stale = scope.bind(TaskChange([], [old_target], {"files-changed": 2}, ["behavior"]))
                try:
                    client.review(stale)
                except FrRuntimeError:
                    metrics["old_scope_refused"] = True
                else:
                    raise AssertionError("stale structural scope admitted")
                metrics["exact_reference_sites"] = len(references)
                metrics["scope_bytes"] = len(json.dumps(scope.report.to_data()).encode())
                metrics["fresh_process_delivery"] = True
            measured = oracle(root)
            assert measured == baseline
            patch = run(["git", "diff", "--binary", "--", *FILES], root).stdout
            (output/f"{arm}.patch").write_text(patch)
            result[arm] = {"baseline": baseline, "after": measured, "metrics": metrics,
                           "seconds": time.monotonic()-started}
    result["passed"] = True
    result["limits"] = "Prescribed deterministic fixture; scalar literal arguments, indexed consumers and tested outputs only. No live-agent, token-saving, external consumer or source equivalence proof. Nested rustc execution is observed by the oracle; its identity is recorded separately."
    result["rustc"] = run(["rustc", "-vV"], ROOT).stdout
    save(output/"result.json", result)
    save(output/"manifest.json", {"schema": "fr-structural-change-artifacts-1",
        "source_commit": run(["git", "rev-parse", "HEAD"], ROOT).stdout.strip(), "binary_sha256": sha(binary),
        "files": {str(p.relative_to(output)): sha(p) for p in sorted(output.rglob("*")) if p.is_file()}})
    return audit(output)


def audit(output):
    manifest = json.loads((output/"manifest.json").read_text())
    assert manifest["schema"] == "fr-structural-change-artifacts-1"
    for name, digest in manifest["files"].items():
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        assert sha(output/name) == digest, name
    result = json.loads((output/"result.json").read_text())
    assert result["passed"] and result["schema"] == "fr-structural-change-acceptance-1"
    metrics = result["native"]["metrics"]
    store = DirectoryObjectStore(output/"objects")
    assert ChangeScope.restore(store, metrics["scope_root"]).ready
    assert DeliveryReceipt.restore(store, metrics["receipt_root"]).passed
    assert metrics["old_scope_refused"] and metrics["fresh_process_delivery"]
    assert metrics["exact_reference_sites"] == 49
    with tempfile.TemporaryDirectory(prefix="fr-structural-receiver-") as temporary:
        for arm in ("ordinary", "native"):
            root = Path(temporary)/arm
            populate(root, output/"fixture")
            before = sources(root)
            baseline = oracle(root)
            patch = str((output/f"{arm}.patch").resolve())
            run(["git", "apply", "--check", patch], root)
            run(["git", "apply", patch], root)
            assert oracle(root) == baseline == result[arm]["after"]
            after = sources(root)
            assert before != after and all(b"subtotal" not in value for value in after.values())
            run(["git", "apply", "--reverse", patch], root)
            assert sources(root) == before
            run(["git", "apply", patch], root)
            assert sources(root) == after
    return {"passed": True, "arms": 2, "public_results_per_receiver": 49}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT/"target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--reopen", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--scope")
    args = parser.parse_args()
    if args.reopen:
        result = deliver(args.fr, args.project, args.reopen, args.scope)
    elif args.audit:
        result = audit(args.audit.resolve())
    elif args.output:
        result = measure(args.fr.resolve(), args.output.resolve())
    else:
        parser.error("provide --output or --audit")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
