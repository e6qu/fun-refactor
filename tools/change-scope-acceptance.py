#!/usr/bin/env python3
"""Compare explicit source inspection with consumer scopes and replay checked changes."""
from __future__ import annotations

import argparse
import ast
from dataclasses import replace
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
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.investigation_delivery import DeliveryReceipt, attach_delivery, run_delivery
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError

FIXTURE = ROOT/"tests/agent-eval/change-scope"
FILES = ("pricing.py", "api.py", "batch.py", "test_api.py")
BINDINGS = ("tools/change-scope-acceptance.py", "src/project/change_scope.rs", "src/project.rs",
            "src/project/task_change.rs", "src/project/investigation.rs", "src/workflow.rs",
            "src/index.rs", "src/index/references.rs", "src/project/occurrence.rs",
            "src/parse.rs", "src/cache.rs", "src/model.rs", "build.rs",
            "src/analysis/entrypoints.rs", "sdk/python/src/fr_ir/change_scope.py",
            "sdk/python/src/fr_ir/investigation.py", "sdk/python/src/fr_ir/investigation_delivery.py",
            "sdk/python/src/fr_ir/ir.py", "sdk/python/src/fr_ir/runtime.py")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+"\n")


def run(args, root, check=True):
    return subprocess.run(args, cwd=root, capture_output=True, text=True, check=check, timeout=120)


def populate(root, fixture):
    root.mkdir(parents=True)
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    for name in FILES:
        shutil.copyfile(fixture/name, root/name)
    shutil.copyfile(fixture/"oracle.py", root/".fr/oracle.py")
    (root/"README.md").write_text("Independent documentation observation.\n")
    save(root/".fr/checks.json", {"schema": 1, "checks": [
        {"name": "syntax", "argv": [sys.executable, "-B", "-c",
            "import ast,pathlib; [ast.parse(p.read_text()) for p in pathlib.Path('.').glob('*.py')]"],
         "cwd": ".", "timeout_seconds": 10, "covers": ["Python syntax"]},
        {"name": "behavior", "argv": [sys.executable, "-B", ".fr/oracle.py"], "cwd": ".",
         "timeout_seconds": 10, "covers": ["147 bounded public results and the discovered test"],
         "identity_files": [".fr/oracle.py"]}]})
    save(root/".fr/check-scopes.json", {"schema": 1, "checks": [{"name": "behavior", "paths": list(FILES)}]})
    run(["git", "init", "-q"], root)
    run(["git", "add", "--", *FILES, "README.md"], root)


def oracle(root):
    result = run([sys.executable, "-B", ".fr/oracle.py"], root, False)
    return {"passed": result.returncode == 0, "exit_code": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr[-4096:]}


def ordinary_discovery(root):
    started = time.monotonic()
    edges = []
    source_bytes = 0
    for name in FILES:
        source = (root/name).read_bytes()
        source_bytes += len(source)
        tree = ast.parse(source)
        for function in (node for node in tree.body if isinstance(node, ast.FunctionDef)):
            for call in (node for node in ast.walk(function) if isinstance(node, ast.Call)):
                assert isinstance(call.func, ast.Name)
                if call.func.id not in {"subtotal", "quote", "total"}:
                    continue
                assert call.func.end_lineno is not None and call.func.end_col_offset is not None
                offsets = [0]
                for line in source.splitlines(keepends=True):
                    offsets.append(offsets[-1]+len(line))
                edges.append({"consumer": function.name, "target": call.func.id, "path": name,
                    "start": offsets[call.func.lineno-1]+call.func.col_offset,
                    "end": offsets[call.func.end_lineno-1]+call.func.end_col_offset})
    return {"edges": sorted(edges, key=lambda row: row["path"]), "source_bytes": source_bytes,
            "source_files": len(FILES), "seconds": time.monotonic()-started,
            "scope": "Independent AST inspection of this pinned direct-call fixture; not a general resolver."}


def native_edges(scope):
    return sorted([{"consumer": row["consumer"]["name"], "target": row["target"]["name"],
                    "path": row["occurrence"]["path"], **row["occurrence"]["location"]["span"]}
                   for row in scope.report.at("/references") if row["reference_kind"] == "call" and row["consumer"]],
                  key=lambda row: row["path"])


def inspect(client):
    handle = client.project("find", "subtotal").definition_target().handle
    return ChangeScope.inspect(client, [handle])


def proposal(client):
    targets = []
    for name, body in (("subtotal", "return a + b"), ("quote", "return subtotal(a, b) + 3")):
        handle = client.project("find", name).definition_target().handle
        targets.append(TaskTarget(name, handle, "replace-body", fragment=body))
    return TaskChange([], targets, {"files-changed": 2, "paths-changed": ["api.py", "pricing.py"]},
                      ["syntax"], TaskDelivery(patch="artifacts/change.patch"), acceptance_checks=["behavior"])


def measure(binary, output):
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(FIXTURE, output/"fixture")
    for name in BINDINGS:
        path = output/"binding-sources"/name
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT/name, path)
    result: dict[str, Any] = {"schema": "fr-change-scope-acceptance-1"}
    with tempfile.TemporaryDirectory(prefix="fr-scope-acceptance-") as temporary:
        base = Path(temporary)
        ordinary = base/"ordinary"
        populate(ordinary, FIXTURE)
        baseline = oracle(ordinary)
        assert not baseline["passed"]
        observed = ordinary_discovery(ordinary)
        before = {name: (ordinary/name).read_bytes() for name in FILES}
        (ordinary/"pricing.py").write_text(before["pricing.py"].decode().replace("a - b", "a + b"))
        (ordinary/"api.py").write_text(before["api.py"].decode().replace("return subtotal(a, b)", "return subtotal(a, b) + 3"))
        ordinary_oracle = oracle(ordinary)
        assert ordinary_oracle["passed"]
        patch = run(["git", "diff", "--binary", "--", *FILES], ordinary).stdout
        (output/"ordinary.patch").write_text(patch)
        after = {name: (ordinary/name).read_bytes() for name in FILES}
        for name, source in before.items():
            (ordinary/name).write_bytes(source)
        assert all((ordinary/name).read_bytes() == source for name, source in before.items())
        for name, source in after.items():
            (ordinary/name).write_bytes(source)
        result["ordinary"] = {"discovery": observed, "baseline": baseline, "oracle": ordinary_oracle,
                              "undo_exact": True, "redo_exact": True}

        root = base/"native"
        populate(root, FIXTURE)
        client = FrClient(root, executable=binary, timeout=120)
        started_at = time.monotonic()
        scope = inspect(client)
        discovery_seconds = time.monotonic()-started_at
        assert scope.ready and native_edges(scope) == observed["edges"]
        assert {row["test"]["name"] for row in scope.report.at("/test_candidates")} == {"test_total"}
        limited = ChangeScope.inspect(client, [scope.report.at("/targets/0/handle")], depth=0)
        try:
            limited.bind(proposal(client))
        except FrRuntimeError:
            pass
        else:
            raise AssertionError("incomplete discovery admitted a scoped change")
        store = DirectoryObjectStore(output/"objects")
        scope_root = scope.persist(store)
        plan = TaskPlan("Repair the subtotal and apply a fee at the public API", ("consumer scope", "behavior"), (
            TaskStep("scope", "Which consumers and checks need review?", (scope.dependency,), satisfies=("consumer scope",)),
            TaskStep.checked("outcome", "Do public results pass?", checks=scope.checks, depends_on=("scope",), satisfies=("behavior",)),
            TaskStep("notes", "Retain independent documentation", (Dependency(DependencyKind.SOURCE, "README.md"),)),
        ))
        started = plan.resume(client, transition="notes:start")
        evidence = Evidence("read", EvidenceKind.OBSERVATION, started.input_digests["notes"], True, "README.md")
        plan = replace(started.plan, steps=tuple(replace(s, evidence=(evidence,)) if s.id == "notes" else s for s in started.plan.steps))
        plan = plan.resume(client, transition="notes:satisfy").plan
        plan = scope.observe(plan.resume(client, transition="scope:start").plan, client, "scope").plan
        review = client.review(scope.bind(proposal(client)))
        run_result = run_delivery(plan, client, "outcome", review, store)
        assert run_result.receipt.passed and not run_result.passed and not run_result.resumed.complete
        assert run_result.resumed.plan.steps[0].state == StepState.STALE
        refreshed = inspect(client)
        steps = tuple(replace(s, inputs=(refreshed.dependency,), state=StepState.PENDING, evidence=())
                      if s.id == "scope" else s for s in run_result.resumed.plan.steps)
        plan = replace(run_result.resumed.plan, steps=steps).resume(client, transition="scope:start").plan
        plan = refreshed.observe(plan, client, "scope").plan
        plan = plan.resume(client, transition="outcome:reset").plan.resume(client, transition="outcome:start").plan
        completed = attach_delivery(plan, client, "outcome", run_result.receipt)
        assert completed.complete
        plan_root = completed.plan.store(store)
        fresh = run([sys.executable, str(Path(__file__).resolve()), "--reopen", str(output),
                     "--project", str(root), "--fr", str(binary), "--plan", plan_root], ROOT)
        assert json.loads(fresh.stdout)["complete"]
        (root/"new.py").write_text("from api import quote\ndef client():\n    return quote(1, 2)\n")
        drifted = TaskPlan.restore(store, plan_root).resume(client)
        assert not drifted.complete and drifted.plan.steps[2].state == StepState.SATISFIED
        drift_scope = inspect(client)
        assert not drift_scope.ready and "new.py" in drift_scope.report.at("/unmapped_paths")
        (root/"new.py").unlink()
        final_oracle = oracle(root)
        assert final_oracle["passed"] and all((root/name).read_bytes() == after[name] for name in FILES)
        shutil.copyfile(root/"artifacts/change.patch", output/"native.patch")
        result["native"] = {"scope": scope.report.to_data(), "scope_root": scope_root,
            "discovery_seconds": discovery_seconds, "discovery_calls": 2, "discovery_source_bytes": 0,
            "response_bytes": len(json.dumps(scope.report.to_data(), separators=(",", ":")).encode()),
            "limited": limited.report.to_data(), "review": review.to_data(),
            "delivery": run_result.receipt.result.to_data(), "receipt_root": run_result.receipt_root,
            "initial_attachment_error": run_result.attachment_error, "post_change_scope": refreshed.report.to_data(),
            "completed_plan": plan_root, "fresh_process": json.loads(fresh.stdout),
            "invalidated": list(drifted.invalidated), "independent_state": drifted.plan.steps[2].state.value,
            "added_consumer_scope": drift_scope.report.to_data(), "oracle": final_oracle}
    result["passed"] = True
    result["scope"] = "Prescribed deterministic changes on a direct-call Python fixture. No live agent, token saving, runtime dispatch proof or population claim."
    save(output/"result.json", result)
    save(output/"manifest.json", {"schema": "fr-change-scope-artifacts-1", "binary_sha256": sha(binary),
        "source_commit": run(["git", "rev-parse", "HEAD"], ROOT).stdout.strip(),
        "files": {str(p.relative_to(output)): sha(p) for p in sorted(output.rglob('*')) if p.is_file()}})
    audit(output)


def audit(output):
    manifest = json.loads((output/"manifest.json").read_text())
    assert manifest["schema"] == "fr-change-scope-artifacts-1"
    for name, digest in manifest["files"].items():
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        assert sha(output/name) == digest, name
    result = json.loads((output/"result.json").read_text())
    assert result["passed"] and result["schema"] == "fr-change-scope-acceptance-1"
    native = result["native"]
    store = DirectoryObjectStore(output/"objects")
    scope = ChangeScope.restore(store, native["scope_root"])
    assert scope.ready and native_edges(scope) == result["ordinary"]["discovery"]["edges"]
    assert DeliveryReceipt.restore(store, native["receipt_root"]).passed
    assert native["fresh_process"]["complete"] and native["independent_state"] == "satisfied"
    assert set(native["invalidated"]) == {"scope", "outcome"}
    with tempfile.TemporaryDirectory(prefix="fr-scope-receiver-") as temporary:
        for arm in ("ordinary", "native"):
            root = Path(temporary)/arm
            populate(root, output/"fixture")
            before = {name: (root/name).read_bytes() for name in FILES}
            patch = str((output/f"{arm}.patch").resolve())
            run(["git", "apply", "--check", patch], root)
            run(["git", "apply", patch], root)
            assert oracle(root)["passed"]
            after = {name: (root/name).read_bytes() for name in FILES}
            run(["git", "apply", "--reverse", patch], root)
            assert all((root/name).read_bytes() == source for name, source in before.items())
            run(["git", "apply", patch], root)
            assert all((root/name).read_bytes() == source for name, source in after.items())
    return {"passed": True, "arms": 2, "public_results_per_receiver": 147}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT/"target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--reopen", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--plan")
    args = parser.parse_args()
    if args.reopen:
        plan = TaskPlan.restore(DirectoryObjectStore(args.reopen/"objects"), args.plan)
        print(json.dumps({"complete": plan.resume(FrClient(args.project, executable=args.fr)).complete}))
    elif args.audit:
        print(json.dumps(audit(args.audit.resolve())))
    elif args.output:
        measure(args.fr.resolve(), args.output.resolve())
        print(json.dumps({"passed": True, "output": str(args.output)}))
    else:
        parser.error("provide --output or --audit")


if __name__ == "__main__":
    main()
