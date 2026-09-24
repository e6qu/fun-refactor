#!/usr/bin/env python3
"""Retain proof invalidation and checked source delivery on a finite Boolean task."""
from __future__ import annotations

import argparse
from dataclasses import replace
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from evidence_basis import file_digest
from fr_ir.context import MemoryObjectStore, store_merkle_value
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, ProofRequirement, StepState, TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, attach_proofs, run_proofs
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/retained-proofs"
BINDINGS = ["src/spec.rs", "src/spec/retained.rs", "src/checks.rs", "src/project/investigation.rs",
            "src/history.rs", "src/project.rs", "src/parse.rs", "src/extract.rs", "src/cli.rs", "Cargo.lock",
            "sdk/python/src/fr_ir/investigation.py", "sdk/python/src/fr_ir/investigation_proofs.py",
            "sdk/python/src/fr_ir/context.py", "sdk/python/src/fr_ir/runtime.py",
            "kernels/FrKernels/Investigation.lean", "kernels/InvestigationMain.lean",
            "tools/proof-evidence-acceptance.py", "tools/evidence_basis.py"] + [
                f"tests/agent-eval/retained-proofs/{name}" for name in ("task.json", "baseline.json", "subject.py", "Model.lean", "oracle.py")]


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True)


def satisfied(plan, client, id):
    started = plan.resume(client, transition=f"{id}:start")
    observation = Evidence("read", EvidenceKind.OBSERVATION, started.input_digests[id], True, "independent fixture observation")
    changed = replace(started.plan, steps=tuple(replace(s, evidence=(observation,)) if s.id == id else s for s in started.plan.steps))
    return changed.resume(client, transition=f"{id}:satisfy").plan


def measure(args):
    source = (FIXTURE / "subject.py").read_text()
    model = (FIXTURE / "Model.lean").read_text()
    with tempfile.TemporaryDirectory(prefix="fr-proof-eval-") as temporary:
        directory = Path(temporary)
        root = directory / "project"
        root.mkdir()
        client = FrClient(root, executable=str(args.fr.resolve()))
        client.call("spec", "init", "--write")
        (root / "subject.py").write_text(source)
        (root / "independent.py").write_text("def independent():\n    return 99\n")
        (root / "specs/FrSpecs").mkdir(exist_ok=True)
        module = root / "specs/FrSpecs/Model.lean"
        module.write_text(model)
        (root / ".fr").mkdir()
        (root / "artifacts").mkdir()
        shutil.copy(FIXTURE / "oracle.py", root / "oracle.py")
        (root / ".fr/checks.json").write_text(json.dumps({"schema":1,"checks":[{"name":"behavior",
            "argv":[sys.executable,"oracle.py","subject.py"],"cwd":".","timeout_seconds":10,
            "covers":["Independent Boolean truth table for the fixture source."]}]}))
        run(["git", "init", "-q"], root)
        run(["git", "add", "."], root)
        run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "pinned proof fixture"], root)
        plan = TaskPlan("Preserve Boolean identity with retained model evidence", ("identity model",), (
            TaskStep.proved("proof", "Does Lean accept the identity theorem?", proofs=(ProofRequirement("specs", "FrSpecs/Model.lean", "identity"),), satisfies=("identity model",)),
            TaskStep("dependent", "Does the next conclusion use this theorem?", (Dependency(DependencyKind.SOURCE, "subject.py"),), depends_on=("proof",)),
            TaskStep("independent", "Does unrelated source remain understood?", (Dependency(DependencyKind.SOURCE, "independent.py"),)),
        ))
        plan = satisfied(plan, client, "independent")
        store = MemoryObjectStore()
        start = time.perf_counter()
        proved = run_proofs(plan, client, "proof", ProofReport.review(client), store)
        elapsed = time.perf_counter() - start
        assert proved.passed and proved.resumed.complete
        retained_plan = satisfied(proved.resumed.plan, client, "dependent")
        plan_root = retained_plan.store(store)
        reopened = TaskPlan.restore(store, plan_root).resume(client)
        assert reopened.complete
        cases = {}
        for name in ("source", "model", "property", "addition", "deletion", "configuration", "toolchain"):
            originals = {p: p.read_bytes() for p in (root / "subject.py", module, root / "specs/lakefile.toml", root / "specs/lean-toolchain")}
            extra = root / "specs/FrSpecs/New.lean"
            if name == "source": (root / "subject.py").write_text(source.replace("return ok", "return not ok"))
            elif name == "model": module.write_text(model.replace(":= ok", ":= !ok"))
            elif name == "property": module.write_text(model.replace("identity", "renamed"))
            elif name == "addition": extra.write_text("theorem additional : True := by trivial\n")
            elif name == "deletion": module.unlink()
            elif name == "configuration": (root / "specs/lakefile.toml").write_text('name = "other"\n')
            else: (root / "specs/lean-toolchain").write_text("unavailable\n")
            current = retained_plan.resume(client)
            assert current.invalidated == ("proof", "dependent")
            assert current.plan.steps[2].state == StepState.SATISFIED
            try:
                attach_proofs(retained_plan, client, "proof", proved.proofs)
            except FrRuntimeError as error:
                refusal = str(error)
            else:
                raise AssertionError("stale proof attached")
            cases[name] = {"resumed":current.report.to_data(),"attachment_refusal":refusal}
            for path, contents in originals.items(): path.write_bytes(contents)
            extra.unlink(missing_ok=True)
        handle = next(i["handle"] for i in client.project("identities", "subject.py").at("/items") if i["name"] == "allowed")
        def change(handle):
            return TaskChange([], [TaskTarget("preserve", handle, "replace-body", fragment="return not not ok")],
                {"files-changed":1,"edits":1,"paths-changed":["subject.py"]}, ["behavior"], TaskDelivery(patch="artifacts/proved.patch"))
        old_review = client.review(change(handle))
        module.write_text(model + "\n-- A new review must bind this model revision.\n")
        try:
            client.execute(old_review)
        except FrRuntimeError as error:
            stale_review = str(error)
        else:
            raise AssertionError("stale mutation review admitted")
        handle = next(i["handle"] for i in client.project("identities", "subject.py").at("/items") if i["name"] == "allowed")
        fresh_review = client.review(change(handle))
        delivered = client.execute(fresh_review)
        assert delivered.passed
        stale = retained_plan.resume(client)
        assert not stale.complete
        reset = stale.plan.resume(client, transition="proof:reset")
        fresh = run_proofs(reset.plan, client, "proof", ProofReport.review(client), store)
        assert fresh.passed and fresh.proofs.basis != proved.proofs.basis
        receiver = directory / "receiver"
        receiver.mkdir()
        (receiver / "subject.py").write_text(source)
        run(["git", "init", "-q"], receiver)
        run(["git", "apply", "--check", str(root / "artifacts/proved.patch")], receiver)
        run(["git", "apply", str(root / "artifacts/proved.patch")], receiver)
        oracle = json.loads(run([sys.executable, str(FIXTURE / "oracle.py"), str(receiver / "subject.py")], receiver).stdout)
        module.write_text("theorem impossible : False := by trivial\n")
        orphan = ProofReport.review(client).execute(client)
        assert not orphan.passed
        return {"schema":"fr-proof-acceptance-1","repository_revision":run(["git","rev-parse","HEAD"],ROOT).stdout.strip(),
            "source_bindings":{path:file_digest(ROOT/path) for path in BINDINGS},"platform":platform.platform(),
            "baseline":json.loads((FIXTURE/"baseline.json").read_text()),"original":proved.proofs.report.to_data(),"original_root":proved.report_root,
            "plan":retained_plan.to_data(),"plan_root":plan_root,"reopened":reopened.report.to_data(),"cases":cases,
            "old_review":old_review.to_data(),"fresh_review":fresh_review.to_data(),"stale_review_refusal":stale_review,
            "delivery":delivered.to_data(),"patch":(root/"artifacts/proved.patch").read_text(),"receiver_behavior":oracle,
            "fresh":fresh.proofs.report.to_data(),"fresh_root":fresh.report_root,"fresh_plan":fresh.resumed.report.to_data(),
            "orphan":orphan.report.to_data(),"measurement":{"initial_proof_seconds":elapsed,"initial_report_bytes":len(json.dumps(proved.proofs.report.to_data()).encode()),"tokens":None},
            "scope":"Finite Boolean source cases and Lean model theorems. Clean package rebuilds deliberately retain no incremental cache claim. Caller-retained execution records remain trusted. Installed Lean libraries remain trusted. No source implementation correspondence proof or live-agent claim."}


def audit(value):
    assert value["schema"] == "fr-proof-acceptance-1"
    assert value["source_bindings"] == {path:file_digest(ROOT/path) for path in BINDINGS}
    assert value["baseline"] == json.loads((FIXTURE/"baseline.json").read_text())
    for key in ("original", "fresh"):
        proof = ProofReport(FrReport(value[key], ()))
        assert proof.passed and proof.persist(MemoryObjectStore()) == value[key + "_root"]
        assert proof.report.at("/source_implementation_proved") is False
        assert any(p["name"] == "identity" for p in proof.report.at("/evidence/properties"))
    assert value["original"]["input_digest"] != value["fresh"]["input_digest"]
    assert store_merkle_value(MemoryObjectStore(), value["plan"]).digest == value["plan_root"]
    assert value["reopened"]["complete"] and value["fresh_plan"]["complete"]
    assert set(value["cases"]) == {"source", "model", "property", "addition", "deletion", "configuration", "toolchain"}
    for case in value["cases"].values():
        assert case["attachment_refusal"] and not case["resumed"]["complete"]
        assert case["resumed"]["invalidated"] == ["proof", "dependent"]
        assert case["resumed"]["plan"]["steps"][2]["state"] == "satisfied"
    assert not ProofReport(FrReport(value["orphan"], ())).passed
    assert value["stale_review_refusal"] and value["delivery"]["passed"]
    stages = value["delivery"]["workflow"]["stages"]
    assert {"apply", "undo", "redo", "deliver-patch"} <= {stage["stage"] for stage in stages}
    assert all(stage["status"] == "passed" for stage in stages)
    with tempfile.TemporaryDirectory(prefix="fr-proof-audit-") as temporary:
        root = Path(temporary)
        (root/"subject.py").write_text((FIXTURE/"subject.py").read_text())
        (root/"change.patch").write_text(value["patch"])
        run(["git", "init", "-q"], root)
        run(["git", "apply", "--check", "change.patch"], root)
        run(["git", "apply", "change.patch"], root)
        assert (root/"subject.py").read_text() == (FIXTURE/"subject.py").read_text().replace("return ok", "return not not ok")
        assert json.loads(run([sys.executable,str(FIXTURE/"oracle.py"),str(root/"subject.py")],root).stdout) == value["receiver_behavior"] == [False, True]
    assert value["measurement"]["initial_proof_seconds"] > 0 and value["measurement"]["initial_report_bytes"] > 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT/"target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
    else:
        print(json.dumps(value, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
