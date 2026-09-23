from dataclasses import replace
from pathlib import Path
import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep, StepState
from fr_ir.runtime import FrClient, FrRuntimeError, Occurrence

FR = Path(__file__).resolve().parents[3] / "target/debug/fr"


def test_persist_resume_and_dependency_invalidation(tmp_path):
    (tmp_path / "subject.py").write_text("def checkout():\n    return 1\n")
    plan = TaskPlan("fix checkout", ("behavior",), (TaskStep(
        "diagnose", "why?", (Dependency(DependencyKind.SOURCE, "subject.py"),),
        required_checks=("oracle",), satisfies=("behavior",),
    ),))
    store = MemoryObjectStore()
    digest = plan.store(store)
    restored = TaskPlan.restore(store, digest)
    assert restored == plan
    started = restored.resume(FrClient(tmp_path, executable=str(FR)), transition="diagnose:start")
    assert started.plan.steps[0].state == StepState.RUNNING
    receipt = Evidence("oracle", EvidenceKind.CHECK, started.input_digests["diagnose"], True, "oracle-result.json")
    ready = replace(started.plan, steps=(replace(started.plan.steps[0], evidence=(receipt,)),))
    finished = ready.resume(FrClient(tmp_path, executable=str(FR)), transition="diagnose:satisfy")
    assert finished.complete
    saved = finished.plan.store(store)
    (tmp_path / "subject.py").write_text("def checkout():\n    return 2\n")
    resumed = TaskPlan.restore(store, saved).resume(FrClient(tmp_path, executable=str(FR)))
    assert resumed.invalidated == ("diagnose",)
    assert not resumed.complete


def test_occurrence_rejects_stale_revision_and_bad_coordinates():
    value = {"id": "fro1:" + "b" * 64, "revision": "a" * 64, "path": "subject.py",
             "location": {"span": {"start": 0, "end": 2}, "range": {
                 "start": {"line": 1, "col": 1}, "end": {"line": 1, "col": 2}}},
             "role": "call", "enclosing": None}
    occurrence = Occurrence.from_data(value)
    assert occurrence.text("é()", revision="a" * 64) == "é"
    with pytest.raises(FrRuntimeError, match="revision"):
        occurrence.text("é()", revision="c" * 64)
    value["location"]["range"]["end"]["col"] = 3
    with pytest.raises(FrRuntimeError, match="line range"):
        Occurrence.from_data(value).text("é()", revision="a" * 64)


def test_plan_refuses_unknown_fields_and_state():
    plan = TaskPlan("goal", ("done",), ()).to_data()
    plan["execute"] = True
    with pytest.raises(FrRuntimeError, match="unknown"):
        TaskPlan.from_data(plan)
