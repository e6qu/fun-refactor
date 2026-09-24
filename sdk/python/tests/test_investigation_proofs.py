from dataclasses import replace
import copy
import json
from pathlib import Path

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, ProofRequirement, StepState, TaskPlan, TaskStep
from fr_ir.investigation_proofs import ProofReport, attach_proofs, run_proofs
from fr_ir.ir import IrError
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FR = Path(__file__).resolve().parents[3] / "target/debug/fr"
LEAN = "namespace FrSpecs\ndef allowed (ok : Bool) : Bool := ok\ntheorem identity (ok : Bool) : allowed ok = ok := by rfl\nend FrSpecs\n"


@pytest.fixture
def project(tmp_path):
    client = FrClient(tmp_path, executable=str(FR))
    client.call("spec", "init", "--write")
    (tmp_path / "subject.py").write_text("def allowed(ok: bool) -> bool:\n    return ok\n")
    (tmp_path / "independent.py").write_text("def independent():\n    return 99\n")
    (tmp_path / "specs/FrSpecs").mkdir(exist_ok=True)
    (tmp_path / "specs/FrSpecs/Model.lean").write_text(LEAN)
    return client, tmp_path


def plan():
    return TaskPlan("retain the identity theorem", ("model identity",), (
        TaskStep.proved("proof", "is the model identity proved?", proofs=(ProofRequirement("specs", "FrSpecs/Model.lean", "identity"),), satisfies=("model identity",)),
        TaskStep("dependent", "does the model support this observation?", (Dependency(DependencyKind.SOURCE, "subject.py"),), depends_on=("proof",)),
        TaskStep("independent", "is unrelated source understood?", (Dependency(DependencyKind.SOURCE, "independent.py"),)),
    ))


def test_real_proof_checks_unimported_module_and_restores_verified_objects(project):
    client, root = project
    review = ProofReport.review(client)
    assert not review.passed and review.report.at("/executed") is False
    store = MemoryObjectStore()
    run = run_proofs(plan(), client, "proof", review, store)
    assert run.passed and run.resumed.complete
    assert run.resumed.plan.steps[0].state == StepState.SATISFIED
    assert run.resumed.plan.steps[0].evidence[0].kind == EvidenceKind.MODEL_PROOF
    restored = ProofReport.restore(store, run.report_root)
    assert restored.report.to_data() == run.proofs.report.to_data()
    assert run.proofs.report.at("/source_implementation_proved") is False
    assert [r["module"] for r in run.proofs.report.at("/results")] == [None, *run.proofs.report.at("/modules")]
    assert not (root / "specs/.lake").exists()
    assert TaskPlan.restore(store, run.resumed.plan.store(store)) == run.resumed.plan


@pytest.mark.parametrize("mutation", ["source", "model", "property", "addition", "deletion", "configuration", "toolchain"])
def test_retained_proof_invalidates_and_preserves_independent_observation(project, mutation):
    client, root = project
    started = plan().resume(client, transition="independent:start")
    evidence = Evidence("observation", EvidenceKind.OBSERVATION, started.input_digests["independent"], True, "read independent source")
    current = replace(started.plan, steps=tuple(replace(s, evidence=(evidence,)) if s.id == "independent" else s for s in started.plan.steps))
    current = current.resume(client, transition="independent:satisfy").plan
    run = run_proofs(current, client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert run.passed
    if mutation == "source":
        (root / "subject.py").write_text("def allowed(ok: bool) -> bool:\n    return not ok\n")
    elif mutation == "model":
        (root / "specs/FrSpecs/Model.lean").write_text(LEAN.replace(":= ok", ":= !ok"))
    elif mutation == "property":
        (root / "specs/FrSpecs/Model.lean").write_text(LEAN.replace("identity", "another"))
    elif mutation == "addition":
        (root / "specs/FrSpecs/Added.lean").write_text("theorem another : True := by trivial\n")
    elif mutation == "deletion":
        (root / "specs/FrSpecs/Model.lean").unlink()
    elif mutation == "configuration":
        (root / "specs/lakefile.toml").write_text('name = "different"\n')
    else:
        (root / "specs/lean-toolchain").write_text("unavailable\n")
    resumed = run.resumed.plan.resume(client)
    assert {"proof", "dependent"} <= set(resumed.invalidated)
    assert resumed.plan.steps[2].state == StepState.SATISFIED
    assert not resumed.complete
    with pytest.raises(FrRuntimeError):
        attach_proofs(run.resumed.plan, client, "proof", run.proofs)
    with pytest.raises(FrRuntimeError):
        run.resumed.plan.resume(client, transition="proof:satisfy")


@pytest.mark.parametrize("source", ["theorem impossible : False := by trivial\n", "theorem debt : True := by sorry\n"])
def test_failed_or_debt_bearing_module_never_completes(project, source):
    client, root = project
    (root / "specs/FrSpecs/Orphan.lean").write_text(source)
    run = run_proofs(plan(), client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert not run.passed and not run.resumed.complete
    assert run.attachment_error and not run.resumed.plan.steps[0].evidence
    store = MemoryObjectStore()
    assert not ProofReport.restore(store, run.proofs.persist(store)).passed


def test_unknown_theorem_and_manually_reported_proof_cannot_meet_named_requirement(project):
    client, _ = project
    current = plan().resume(client, transition="proof:start")
    manual = Evidence("identity", EvidenceKind.MODEL_PROOF, current.input_digests["proof"], True, "some proof")
    authored = replace(current.plan, steps=(replace(current.plan.steps[0], evidence=(manual,)), *current.plan.steps[1:]))
    with pytest.raises(FrRuntimeError, match="transition refused"):
        authored.resume(client, transition="proof:satisfy")
    wrong = replace(plan(), steps=(TaskStep.proved("proof", "unknown", proofs=(ProofRequirement("specs", "FrSpecs/Model.lean", "unknown"),), satisfies=("model identity",)),))
    run = run_proofs(wrong, client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert run.proofs.passed and not run.passed and "missing" in run.attachment_error


def test_stale_review_is_rejected_before_execution(project):
    client, root = project
    review = ProofReport.review(client)
    (root / "subject.py").write_text("changed = True\n")
    with pytest.raises(FrRuntimeError, match="inputs changed"):
        review.execute(client)
    assert not (root / "specs/.lake").exists()


@pytest.mark.parametrize("mutation", ["authority", "implementation", "digest", "coverage", "outcome", "debt", "theorem", "assumptions"])
def test_tampered_proof_transport_or_native_evidence_refuses(project, mutation):
    client, _ = project
    report = ProofReport.review(client).execute(client)
    value = copy.deepcopy(report.report.to_data())
    if mutation == "authority": value["mutation_authority"] = True
    elif mutation == "implementation": value["source_implementation_proved"] = True
    elif mutation == "digest": value["input_digest"] = "0" * 64
    elif mutation == "coverage": value["results"].pop()
    elif mutation == "outcome": value["results"][0]["result"]["exit_code"] = 9
    elif mutation == "debt": value["evidence"]["verification"]["report"]["debts"] = [{}]
    elif mutation == "theorem": value["evidence"]["properties"][0]["name"] = "invented"
    else: value["evidence"]["declared_assumptions"] = [{}]
    with pytest.raises(FrRuntimeError):
        changed = ProofReport(FrReport(value, ()))
        attach_proofs(plan().resume(client).plan, client, "proof", changed)


def test_external_packages_and_symlinks_refuse(project, tmp_path):
    client, root = project
    manifest = root / "specs/lake-manifest.json"
    manifest.write_text(json.dumps({"packages": [{"name": "external"}]}))
    with pytest.raises(FrRuntimeError, match="external dependencies"):
        ProofReport.review(client)
    manifest.unlink()
    (root / "specs/FrSpecs/Link.lean").symlink_to(root / "specs/FrSpecs/Model.lean")
    with pytest.raises(FrRuntimeError, match="symlinks"):
        ProofReport.review(client)


@pytest.mark.parametrize("package,spec,theorem", [("../specs", "Model.lean", "p"), ("specs", "/Model.lean", "p"), ("specs", "Model.lean", ""), ("specs", "Model.rs", "p")])
def test_invalid_proof_requirements_refuse(package, spec, theorem):
    with pytest.raises(FrRuntimeError):
        ProofRequirement(package, spec, theorem)


def fake_installation(root, tools, mutate=False):
    import shlex
    import subprocess

    real = Path(subprocess.check_output(["lean", "--print-prefix"], cwd=root / "specs", text=True).strip())
    tools = tools / "bin"
    tools.mkdir()
    lean = tools / "lean"
    lean.write_text(f'#!/bin/sh\nif [ "$1" = "--print-prefix" ]; then echo {shlex.quote(str(tools.parent))}; else exec {shlex.quote(str(real / "bin/lean"))} "$@"; fi\n')
    lean.chmod(0o755)
    lake = tools / "lake"
    mutation = f'printf "changed = True\\n" > {shlex.quote(str(root / "subject.py"))}\n' if mutate else ""
    lake.write_text(f'#!/bin/sh\n{mutation}exec {shlex.quote(str(real / "bin/lake"))} "$@"\n')
    lake.chmod(0o755)
    return tools, lake


def test_checker_executable_and_environment_drift_invalidates(project, monkeypatch, tmp_path_factory):
    import os

    client, root = project
    tools, wrapper = fake_installation(root, tmp_path_factory.mktemp("proof-tools"))
    monkeypatch.setenv("PATH", str(tools) + os.pathsep + os.environ["PATH"])
    run = run_proofs(plan(), client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert run.passed
    wrapper.write_text(wrapper.read_text() + "# executable changed\n")
    assert "proof" in run.resumed.plan.resume(client).invalidated
    wrapper.write_text(wrapper.read_text().replace("# executable changed\n", ""))
    assert run.resumed.plan.resume(client).complete
    monkeypatch.setenv("LEAN_NUM_THREADS", "1" if os.environ.get("LEAN_NUM_THREADS") != "1" else "2")
    assert "proof" in run.resumed.plan.resume(client).invalidated


def test_source_change_during_checker_execution_refuses_attachment(project, monkeypatch, tmp_path_factory):
    import os

    client, root = project
    tools, _ = fake_installation(root, tmp_path_factory.mktemp("mutating-checker"), mutate=True)
    monkeypatch.setenv("PATH", str(tools) + os.pathsep + os.environ["PATH"])
    run = run_proofs(plan(), client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert not run.passed and run.proofs.report.at("/stable") is False
    assert run.resumed.plan.steps[0].state == StepState.STALE


def test_direct_lean_installation_does_not_require_elan(project, monkeypatch, tmp_path_factory):
    import os
    import shutil

    client, root = project
    tools, _ = fake_installation(root, tmp_path_factory.mktemp("direct-lean"))
    monkeypatch.setenv("PATH", str(tools) + os.pathsep + "/usr/bin:/bin")
    assert shutil.which("elan") is None
    assert run_proofs(plan(), client, "proof", ProofReport.review(client), MemoryObjectStore()).passed


def test_merkle_corruption_refuses(project):
    client, _ = project
    report = ProofReport.review(client).execute(client)
    store = MemoryObjectStore()
    digest = report.persist(store)
    store._objects[digest] = {}
    with pytest.raises(IrError):
        ProofReport.restore(store, digest)


def test_unimported_modules_build_local_dependencies_and_reject_ambiguous_names(project):
    client, root = project
    (root / "specs/FrSpecs/Helper.lean").write_text("namespace FrSpecs\ndef helper (ok : Bool) := ok\nend FrSpecs\n")
    (root / "specs/FrSpecs/Model.lean").write_text("import FrSpecs.Helper\nnamespace FrSpecs\ntheorem identity (ok : Bool) : helper ok = ok := by rfl\nend FrSpecs\n")
    run = run_proofs(plan(), client, "proof", ProofReport.review(client), MemoryObjectStore())
    assert run.passed
    (root / "specs/FrSpecs/Model.lean").write_text("namespace First\ntheorem identity : True := by trivial\nend First\nnamespace Second\ntheorem identity : True := by trivial\nend Second\n")
    report = ProofReport.review(client).execute(client)
    assert report.passed
    with pytest.raises(FrRuntimeError, match="ambiguous|missing"):
        attach_proofs(plan().resume(client).plan, client, "proof", report)


def test_direct_installation_rejects_an_unpinned_effective_version(project, monkeypatch, tmp_path_factory):
    import os

    client, root = project
    tools, _ = fake_installation(root, tmp_path_factory.mktemp("wrong-lean"))
    lean = tools / "lean"
    lean.write_text(lean.read_text().replace('else exec', 'elif [ "$1" = "--version" ]; then echo "Lean (version 4.27.0, test)"; else exec'))
    monkeypatch.setenv("PATH", str(tools) + os.pathsep + os.environ["PATH"])
    with pytest.raises(FrRuntimeError, match="pinned version"):
        ProofReport.review(client)
