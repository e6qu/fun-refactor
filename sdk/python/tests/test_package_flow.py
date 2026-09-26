from dataclasses import replace
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "tests/agent-eval/package-flow"


def workspace(root):
    for path in FIXTURE.rglob("*.py"):
        target = root / path.relative_to(FIXTURE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
    rules = root / "rules.json"
    rules.write_text('{"version":"package-1","sources":["source"],"sinks":["sink"]}')
    return FrClient(root, executable=str(ROOT / "target/debug/fr"), max_output_bytes=1_048_576), rules


def analyze(client, rules, name="render", cache=None):
    target = client.project("find", name).definition_target().handle
    return (cache or FlowCache(MemoryObjectStore())).analyze(
        client, target, rules=rules, summaries=True, imports=True, steps=4096, max_bytes=1_048_576)


def evidence(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def rehash(value):
    value["input_digest"] = hashlib.sha256(json.dumps(
        value["inputs"], ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return FrReport(value, ())


def test_runtime_and_utf8_oracles_agree_with_package_evidence(tmp_path):
    client, rules = workspace(tmp_path)
    for path in tmp_path.rglob("*.py"):
        path.write_text('# café 🦀\n' + path.read_text())
    oracle = json.loads(subprocess.check_output(["python3", "-B", str(FIXTURE / "oracle.py"), str(tmp_path)]))
    for name in ("render", "safe"):
        result = analyze(client, rules, name)
        assert result.report.at("/complete") and result.dependencies.complete
        assert bool(result.witnesses) == any(oracle["results"][name]["emitted"])
        assert result.dependencies.entry.target == "app.py"
        for witness in result.witnesses:
            for point in witness.occurrences:
                point.text((tmp_path / point.path).read_text(), revision=result.report.at("/revision"))
                if point.role in {"source", "sink", "call-result", "summary-call"}:
                    assert [point.location.span.start, point.location.span.end] in oracle["call_spans"][point.path]
    lookup = next(item for item in analyze(client, rules).dependencies.lookups if item.module == "portal.api")
    assert lookup.target == "portal/api.py" and lookup.packages == ("portal/__init__.py",)
    assert len(lookup.candidates) == 8


@pytest.mark.parametrize("change", ["initializer", "helper", "parent-shadow", "parent-stub", "leaf-stub", "missing-parent", "import-alias", "unrelated"])
def test_package_dependency_changes_agree_with_clean_rebuilds(tmp_path, change):
    client, rules = workspace(tmp_path)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    initial = analyze(client, rules, cache=cache)
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, cache=cache).reused
    if change == "missing-parent":
        (tmp_path / "portal/__init__.py").unlink()
    else:
        path, source = {
            "initializer": ("portal/__init__.py", "# new parent revision\n"),
            "helper": ("portal/transform.py", "def clean_value(value):\n    return 0\n"),
            "parent-shadow": ("portal.py", ""),
            "parent-stub": ("portal/__init__.pyi", ""),
            "leaf-stub": ("portal/api.pyi", ""),
            "import-alias": ("portal/api.py", "from archive.transform import clean_value\ndef send(value):\n    return sink(clean_value(value))\n"),
            "unrelated": ("unrelated.py", "def independent():\n    return 7\n"),
        }[change]
        (tmp_path / path).write_text(source)
    current = analyze(client, rules, cache=cache)
    assert current.reused == (change == "unrelated")
    assert evidence(current) == evidence(analyze(client, rules))
    assert (current.report.at("/input_digest") == initial.report.at("/input_digest")) == (change == "unrelated")
    assert current.report.at("/complete") == (change in {"initializer", "helper", "import-alias", "unrelated"})


@pytest.mark.parametrize("change", ["target", "parent", "candidate", "digest", "alias", "prefix", "entry", "entry-parent", "complete"])
def test_package_dependency_tampering_refuses_even_with_a_new_digest(tmp_path, change):
    client, rules = workspace(tmp_path)
    value = analyze(client, rules).report.to_data()
    modules = value["inputs"]["modules"]
    lookup = next(item for item in modules["lookups"] if item["module"] == "portal.api")
    if change == "target": lookup["target"] = "archive/transform.py"
    elif change == "parent": lookup["packages"] = []
    elif change == "candidate": lookup["candidates"].pop("portal.py")
    elif change == "digest": lookup["candidates"]["portal/__init__.py"]["digest"] = "0" * 64
    elif change == "alias": lookup["alias"] = "renamed"
    elif change == "prefix": lookup["prefix"] = "portal.api"
    elif change == "entry": modules["entry"]["target"] = "portal/api.py"
    elif change == "entry-parent": modules["entry"]["packages"] = ["portal/__init__.py"]
    else: modules["files"].pop("portal/__init__.py")
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(rehash(value))


def test_package_parent_invalidates_dependents_after_plan_reopen(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "note.py").write_text("# independent\n")
    result = analyze(client, rules)
    plan = TaskPlan("explain package flow", ("explained",), (
        TaskStep("flow", "Does input reach the sink?", (result.dependencies.dependency,), satisfies=("explained",)),
        TaskStep("note", "Read independent note", (Dependency(DependencyKind.SOURCE, "note.py"),)),
        TaskStep("conclusion", "Explain the result", (Dependency(DependencyKind.SOURCE, "app.py"),), depends_on=("flow",)),
    ))
    for name in ("flow", "note", "conclusion"):
        started = plan.resume(client, transition=f"{name}:start")
        receipt = Evidence(name, EvidenceKind.OBSERVATION, started.input_digests[name], True, "package fixture")
        plan = replace(started.plan, steps=tuple(replace(step, evidence=(receipt,)) if step.id == name else step
                                                for step in started.plan.steps))
        plan = plan.resume(client, transition=f"{name}:satisfy").plan
    store = MemoryObjectStore()
    plan = TaskPlan.restore(store, plan.store(store))
    assert plan.resume(client).complete
    (tmp_path / "portal/__init__.py").write_text("# parent changed\n")
    resumed = plan.resume(client)
    assert set(resumed.invalidated) == {"flow", "conclusion"}
    assert resumed.plan.steps[1].state == StepState.SATISFIED


def test_legacy_module_dependency_records_remain_readable():
    digest = "1" * 64
    value = {"schema": "fr-dataflow-1", "inputs": {
        "summary_mode": True, "source": {"path": "app.py", "digest": digest},
        "selection": {"name": "entry"}, "rule_file": None, "context": "generic",
        "budget": {"steps": 128, "depth": 8, "bytes": 65536},
        "modules": {"schema": "fr-flow-modules-1", "complete": True, "cutoffs": [],
            "files": {"app.py": digest, "leaf.py": digest}, "lookups": [{
                "importer": "app.py", "alias": "helper", "module": "leaf", "member": "identity",
                "admitted": True, "resolution": "function", "candidates": {
                    "leaf.py": {"status": "source", "digest": digest},
                    "leaf.pyi": {"status": "missing"}, "leaf/__init__.py": {"status": "missing"}}}]}}}
    result = FlowDependencies.from_report(rehash(value))
    assert result.entry is None and result.complete
    assert result.lookups[0].target == "leaf.py" and result.lookups[0].prefix == "helper"
