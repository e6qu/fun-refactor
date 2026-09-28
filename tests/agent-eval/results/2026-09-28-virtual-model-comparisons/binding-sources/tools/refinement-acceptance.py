#!/usr/bin/env python3
"""Retain Boolean model comparisons, compiled outcomes and receiver replay."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/"sdk/python/src"))
from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, run_proofs
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.refinement import ModelComparison, ModelSnapshot
from fr_ir.runtime import FrClient, FrRuntimeError
from fr_ir.structural_change import RefactorRequest

FIXTURE = ROOT/"tests/agent-eval/refinement"
FILES = ("subject.rs", "main.rs", "translation.py")
BINDINGS = ("grammars/python/src/scanner.c", "src/vfs.rs", "src/spec.rs", "src/spec/refinement.rs", "src/spec/retained.rs", "src/cli.rs",
    "src/formal_kernel.rs", "src/project/investigation.rs", "src/project/task_change.rs",
    "sdk/python/src/fr_ir/refinement.py", "sdk/python/src/fr_ir/investigation_proofs.py",
    "sdk/python/src/fr_ir/investigation.py", "tools/refinement-acceptance.py")


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+"\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, check=True, timeout=180)


def populate(root, fixture):
    root.mkdir(parents=True)
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    for name in FILES:
        shutil.copyfile(fixture/name, root/name)
    shutil.copyfile(fixture/"oracle.py", root/".fr/oracle.py")
    save(root/".fr/checks.json", {"schema": 1, "checks": [{"name": "truth-table",
        "argv": [sys.executable, "-B", ".fr/oracle.py"], "cwd": ".", "timeout_seconds": 30,
        "identity_files": [".fr/oracle.py"], "covers": ["All eight Boolean assignments in Rust and Python"]}]})
    save(root/".fr/check-scopes.json", {"schema": 1, "checks": [{"name": "truth-table", "paths": list(FILES)}]})
    run(["git", "init", "-q"], root)
    run(["git", "add", "--", *FILES], root)


def oracle(root):
    return run([sys.executable, "-B", ".fr/oracle.py"], root).stdout.strip()


def finish(binary, root, output, snapshot_root):
    client = FrClient(root, executable=binary)
    store = DirectoryObjectStore(output/"objects")
    before = ModelSnapshot.restore(store, snapshot_root)
    client.call("spec", "init", "--write")
    comparisons = [ModelComparison("RustPreservation", before, "subject.rs::permits", (0, 1)),
        ModelComparison("Translation", before, "translation.py::permits", (1, 0)),
        ModelComparison("Restriction", before, "translation.py::restricted", (0, 1, 2), "refines")]
    tactics = output/"tactics.txt"
    tactics.write_text("decide\n")
    for comparison in comparisons:
        review = comparison.review(client)
        save(output/f"{comparison.name}-review.json", review.to_data())
        comparison.execute(client, review)
        target = f"specs/FrSpecs/{comparison.name}.lean::preserves"
        checked = client.call("spec", "proof-check", target, "--from", str(tactics))
        save(output/f"{comparison.name}-proof.json", checked.to_data())
        client.call("spec", "prove", target, "--from", str(tactics), "--write")
    plan = TaskPlan("Check retained Boolean relations", ("three model relations",), (
        TaskStep.proved("models", "Do the retained models satisfy the three requested relations?",
            proofs=tuple(c.requirement() for c in comparisons), satisfies=("three model relations",)),))
    result = run_proofs(plan, client, "models", ProofReport.review(client), store)
    assert result.passed and result.resumed.complete
    evidence = result.proofs.report.at("/evidence/model_comparisons")
    assert len(evidence) == 3 and all(row["status"] == "checked_by_lean" for row in evidence)
    assert all(row["source_implementation_proved"] is False for row in evidence)
    plan_root = result.resumed.plan.store(store)
    save(output/"proof-report.json", result.proofs.report.to_data())
    shutil.copytree(root/"specs", output/"specs", ignore=shutil.ignore_patterns(".lake"))
    return {"passed": True, "plan_root": plan_root, "report_root": result.report_root,
        "snapshot_root": snapshot_root, "relations": evidence, "fresh_process": True}


def measure(binary, output):
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(FIXTURE, output/"fixture")
    for name in BINDINGS:
        destination = output/"binding-sources"/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT/name, destination)
    result: dict[str, Any] = {"schema": "fr-refinement-acceptance-1"}
    with tempfile.TemporaryDirectory(prefix="fr-refinement-eval-") as temporary:
        base = Path(temporary)
        for arm in ("ordinary", "native"):
            root = base/arm
            populate(root, FIXTURE)
            baseline = oracle(root)
            client = FrClient(root, executable=binary)
            before = ModelSnapshot.capture(client, "subject.rs::allowed")
            if arm == "ordinary":
                for name in ("subject.rs", "main.rs"):
                    text = (root/name).read_text().replace("allowed", "permits")
                    text = re.sub(r", (?:false|true)\)", ")", text.replace(", unused: bool", ""))
                    (root/name).write_text(text)
                path = root/"translation.py"
                path.write_text(path.read_text().replace("    return a and not b\n", "    return not b and a\n"))
            else:
                rust = client.project("find", "allowed").definition_target().handle
                python = client.project("find", "permits").definition_target().handle
                scope = ChangeScope.inspect(client, [rust, python])
                assert scope.ready, scope.report.to_data()
                change = scope.bind(TaskChange([], [RefactorRequest.rename("permits").target("rename", rust),
                    RefactorRequest.remove_parameter(2).target("remove", rust),
                    TaskTarget("translation", python, "replace-body", fragment="return not b and a")],
                    {"files-changed": 3, "paths-changed": list(FILES)}, ["truth-table"],
                    TaskDelivery(patch="artifacts/migration.patch"), acceptance_checks=["truth-table"]))
                review = client.review(change)
                save(output/"delivery-review.json", review.to_data())
                delivered = client.execute(review)
                assert delivered.passed
                save(output/"delivery.json", delivered.to_data())
            assert oracle(root) == baseline
            patch = ((root/"artifacts/migration.patch").read_text() if arm == "native"
                else run(["git", "diff", "--binary", "--", *FILES], root).stdout)
            (output/f"{arm}.patch").write_text(patch)
            result[arm] = {"passed": True, "oracle": baseline,
                "source_sha256": {name: sha(root/name) for name in FILES}}
            if arm == "native":
                snapshot_root = before.persist(DirectoryObjectStore(output/"objects"))
                args = [sys.executable, str(Path(__file__).resolve()), "--fr", str(binary),
                    "--finish", str(output), "--project", str(root), "--snapshot", snapshot_root]
                result["models"] = json.loads(run(args, ROOT).stdout)
                plan = TaskPlan.restore(DirectoryObjectStore(output/"objects"), result["models"]["plan_root"])
                assert plan.resume(FrClient(root, executable=binary)).complete
                path = root/"translation.py"
                source = path.read_text()
                path.write_text(source.replace("return not b and a", "return b or a"))
                assert not plan.resume(client).complete
                result["changed_source_invalidated"] = True
                path.write_text(source)
        root = base/"false-claim"
        populate(root, FIXTURE)
        client = FrClient(root, executable=binary)
        client.call("spec", "init", "--write")
        before = ModelSnapshot.capture(client, "subject.rs::allowed")
        wrong = ModelComparison("WrongMap", before, "translation.py::permits", (0, 1))
        wrong.execute(client, wrong.review(client))
        failed = client.call("spec", "proof-check", "specs/FrSpecs/WrongMap.lean::preserves", "--from", str(output/"tactics.txt"))
        assert failed.at("/passed") is False
        save(output/"false-claim.json", {"refused": True, "report": failed.to_data()})
    assert result["ordinary"]["source_sha256"] == result["native"]["source_sha256"]
    result["passed"] = True
    result["limits"] = "Prescribed deterministic edits with a shared independent truth-table oracle. Lean proves the requested generated Boolean model relations. Parsing and source correspondence remain trusted or unproved. No live-agent or token-saving claim. Nested rustc identity is recorded separately."
    result["rustc"] = run(["rustc", "-vV"], ROOT).stdout
    save(output/"result.json", result)
    save(output/"manifest.json", {"schema": "fr-refinement-artifacts-1",
        "source_commit": run(["git", "rev-parse", "HEAD"], ROOT).stdout.strip(), "binary_sha256": sha(binary),
        "files": {str(p.relative_to(output)): sha(p) for p in sorted(output.rglob("*")) if p.is_file()}})
    return audit(binary, output)


def audit(binary, output):
    manifest = json.loads((output/"manifest.json").read_text())
    assert manifest["schema"] == "fr-refinement-artifacts-1"
    for name, digest in manifest["files"].items():
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
        assert sha(output/name) == digest, name
    result = json.loads((output/"result.json").read_text())
    assert result["passed"] and result["changed_source_invalidated"] and result["models"]["fresh_process"]
    store = DirectoryObjectStore(output/"objects")
    assert ProofReport.restore(store, result["models"]["report_root"]).passed
    before_snapshot = ModelSnapshot.restore(store, result["models"]["snapshot_root"])
    assert before_snapshot.report.at("/source") == (output/"fixture/subject.rs").read_text()
    for path in (output/"specs/FrSpecs").glob("*.refinement.json"):
        assert json.loads(path.read_text())["request"]["before"] == before_snapshot.report.to_data()
    failed = json.loads((output/"false-claim.json").read_text())
    assert failed["refused"] is True and failed["report"]["passed"] is False
    with tempfile.TemporaryDirectory(prefix="fr-refinement-receiver-") as temporary:
        for arm in ("ordinary", "native"):
            root = Path(temporary)/arm
            populate(root, output/"fixture")
            before = {name: (root/name).read_bytes() for name in FILES}
            patch = str((output/f"{arm}.patch").resolve())
            run(["git", "apply", "--check", patch], root)
            run(["git", "apply", patch], root)
            assert oracle(root) == result[arm]["oracle"]
            assert {name: sha(root/name) for name in FILES} == result[arm]["source_sha256"]
            shutil.copytree(output/"specs", root/"specs")
            client = FrClient(root, executable=binary)
            assert ProofReport.review(client).execute(client).passed
            run(["git", "apply", "--reverse", patch], root)
            assert {name: (root/name).read_bytes() for name in FILES} == before
            try:
                client.call("spec", "check", "specs", "--strict")
            except FrRuntimeError:
                pass
            else:
                raise AssertionError("post-change relation survived source reversal")
            run(["git", "apply", patch], root)
            assert oracle(root) == result[arm]["oracle"]
    return {"passed": True, "arms": 2, "assignments": 8, "model_relations": 3}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr", type=Path, default=ROOT/"target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--finish", type=Path)
    parser.add_argument("--project", type=Path)
    parser.add_argument("--snapshot")
    args = parser.parse_args()
    if args.finish:
        result = finish(args.fr, args.project, args.finish, args.snapshot)
    elif args.audit:
        result = audit(args.fr.resolve(), args.audit.resolve())
    elif args.output:
        result = measure(args.fr.resolve(), args.output.resolve())
    else:
        parser.error("provide --output or --audit")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
