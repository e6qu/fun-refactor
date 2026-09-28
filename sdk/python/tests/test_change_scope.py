import copy
from dataclasses import replace
import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore, MemoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, TaskPlan, TaskStep, StepState
from fr_ir.investigation_delivery import run_delivery
from fr_ir.ir import TaskChange, TaskTarget, TaskDelivery
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FR = Path(__file__).resolve().parents[3]/"target/debug/fr"


def fixture(tmp_path):
    root = tmp_path/"project"
    root.mkdir()
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    sources = {
        "pricing.py": "def subtotal(a, b):\n    return a - b\n",
        "api.py": "from pricing import subtotal\ndef quote(a, b):\n    return subtotal(a, b)\n",
        "batch.py": "from api import quote\ndef total(a, b):\n    return quote(a, b)\n",
        "test_api.py": "from batch import total\ndef test_total():\n    assert total(3, 2) == 5\n",
    }
    for name, source in sources.items():
        (root/name).write_text(source)
    checks = {"schema": 1, "checks": [
        {"name": "syntax", "argv": [sys.executable, "-B", "-c",
            "import ast,pathlib; [ast.parse(p.read_text()) for p in pathlib.Path('.').glob('*.py')]"],
         "cwd": ".", "timeout_seconds": 10, "covers": ["Python syntax"]},
        {"name": "behavior", "argv": [sys.executable, "-B", "-c",
            "from batch import total; from test_api import test_total; test_total(); "
            "assert all(total(a,b)==sum([a,b]) for a in range(-3,4) for b in range(-3,4))"],
         "cwd": ".", "timeout_seconds": 10, "covers": ["public API and test consumer"]},
    ]}
    (root/".fr/checks.json").write_text(json.dumps(checks))
    (root/".fr/check-scopes.json").write_text(json.dumps({"schema": 1, "checks": [
        {"name": "behavior", "paths": list(sources)},
    ]}))
    return root, FrClient(root, executable=FR)


def scope(client, **kwargs):
    handle = client.project("find", "subtotal").definition_target().handle
    return ChangeScope.inspect(client, [handle], **kwargs)


def proposal(client):
    handle = client.project("find", "subtotal").definition_target().handle
    return TaskChange([], [TaskTarget("fix", handle, "replace-body", fragment="return a + b")],
        {"files-changed": 1, "paths-changed": ["pricing.py"]}, ["syntax"],
        TaskDelivery(patch="artifacts/change.patch"), acceptance_checks=["behavior"])


def test_discovers_transitive_consumers_test_candidates_and_exact_references(tmp_path):
    root, client = fixture(tmp_path)
    result = scope(client)
    assert result.ready, result.report.to_data()
    assert result.checks == ("behavior",)
    assert {row["declaration"]["name"] for row in result.report.at("/consumers")} == {"subtotal", "quote", "total", "test_total"}
    assert [row["test"]["name"] for row in result.report.at("/test_candidates")] == ["test_total"]
    for row in result.report.at("/references"):
        item = row["occurrence"]
        span = item["location"]["span"]
        text = (root/item["path"]).read_bytes()[span["start"]:span["end"]].decode()
        assert text == row["target"]["name"]
    assert result.report.at("/runtime_coverage") is False


@pytest.mark.parametrize("budget", [{"depth": 0}, {"nodes": 1}, {"references": 1}, {"max_bytes": 5000}])
def test_budget_exhaustion_never_admits_scoped_delivery(tmp_path, budget):
    _, client = fixture(tmp_path)
    result = scope(client, **budget)
    assert not result.ready and not result.report.at("/indexed_complete")
    assert result.report.at("/cutoffs")
    with pytest.raises(FrRuntimeError, match="gaps"):
        result.bind(proposal(client))


@pytest.mark.parametrize("drift", ["consumer", "map", "check", "deletion", "ambiguous"])
def test_persisted_scope_and_plan_invalidate_after_input_drift(tmp_path, drift):
    root, client = fixture(tmp_path)
    result = scope(client)
    plan = TaskPlan("Inspect repair scope", ("consumers reviewed",), (
        TaskStep("scope", "Which consumers are affected?", (result.dependency,), satisfies=("consumers reviewed",)),
        TaskStep("independent", "Keep unrelated input", (Dependency(DependencyKind.SOURCE, "notes.py"),)),
    ))
    started = plan.resume(client, transition="scope:start")
    complete = result.observe(started.plan, client, "scope")
    assert complete.complete
    store = DirectoryObjectStore(tmp_path/"objects")
    scope_root = result.persist(store)
    plan_root = complete.plan.store(store)
    if drift == "consumer":
        (root/"new.py").write_text("from api import quote\ndef new_client():\n    return quote(1, 2)\n")
    elif drift == "map":
        path = root/".fr/check-scopes.json"
        path.write_text(path.read_text()+"\n")
    elif drift == "check":
        path = root/".fr/checks.json"
        path.write_text(path.read_text()+"\n")
    elif drift == "deletion":
        (root/"pricing.py").unlink()
    else:
        path = root/"pricing.py"
        path.write_text(path.read_text()+"\ndef subtotal(a, b):\n    return 0\n")
    resumed = TaskPlan.restore(store, plan_root).resume(client)
    assert not resumed.complete and resumed.plan.steps[0].state == StepState.STALE
    assert "independent" not in resumed.invalidated
    with pytest.raises(FrRuntimeError, match="current inputs"):
        ChangeScope.restore(store, scope_root).observe(resumed.plan, client, "scope")


def test_scoped_delivery_rechecks_map_and_rejects_omitted_checks_or_foreign_targets(tmp_path):
    root, client = fixture(tmp_path)
    result = scope(client)
    with pytest.raises(FrRuntimeError, match="candidate check"):
        client.review(result.bind(replace(proposal(client), acceptance_checks=())))
    (root/"other.py").write_text("def other():\n    return 0\n")
    result = scope(client)
    handle = client.project("find", "other").definition_target().handle
    wrong = replace(proposal(client), targets=[TaskTarget("other", handle, "replace-body", fragment="return 1")],
                    postconditions={"files-changed": 1, "paths-changed": ["other.py"]})
    with pytest.raises(FrRuntimeError, match="outside"):
        client.review(result.bind(wrong))
    reviewed = client.review(result.bind(proposal(client)))
    path = root/".fr/check-scopes.json"
    path.write_text(path.read_text()+"\n")
    with pytest.raises(FrRuntimeError, match="scope inputs changed"):
        client.execute(reviewed)
    assert "a - b" in (root/"pricing.py").read_text()


def test_scoped_repair_delivers_checked_behavior_and_restores_discovery_in_fresh_process(tmp_path):
    root, client = fixture(tmp_path)
    result = scope(client)
    store = DirectoryObjectStore(tmp_path/"objects")
    digest = result.persist(store)
    code = """import sys
from fr_ir.change_scope import ChangeScope
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan,TaskStep
from fr_ir.runtime import FrClient
s=ChangeScope.restore(DirectoryObjectStore(sys.argv[1]),sys.argv[2]); c=FrClient(sys.argv[3],executable=sys.argv[4])
p=TaskPlan('inspect',('reviewed',),(TaskStep('scope','consumers',(s.dependency,),satisfies=('reviewed',)),))
assert s.observe(p.resume(c,transition='scope:start').plan,c,'scope').complete
"""
    subprocess.run([sys.executable, "-c", code, str(tmp_path/"objects"), digest, str(root), str(FR)],
                   check=True, capture_output=True, text=True)
    plan = TaskPlan("Repair API", ("behavior",), (
        TaskStep.checked("outcome", "Correct public output?", checks=result.checks, satisfies=("behavior",)),
    ))
    run = run_delivery(plan, client, "outcome", client.review(result.bind(proposal(client))), store)
    assert run.passed and run.resumed.complete and (root/"artifacts/change.patch").is_file()


@pytest.mark.parametrize("mapping", [None, {"schema": 1, "checks": []},
    {"schema": 1, "checks": [{"name": "behavior", "paths": ["pricing.py", "missing.py"]}]}])
def test_unmapped_or_missing_paths_stay_visible(tmp_path, mapping):
    root, client = fixture(tmp_path)
    path = root/".fr/check-scopes.json"
    if mapping is None:
        path.unlink()
    else:
        path.write_text(json.dumps(mapping))
    result = scope(client)
    assert not result.ready
    assert result.report.at("/unmapped_paths") or result.report.at("/missing_mapped_paths")


def test_tampered_receipt_cannot_claim_readiness(tmp_path):
    _, client = fixture(tmp_path)
    result = scope(client, depth=0)
    data = copy.deepcopy(result.report.to_data())
    data["review_ready"] = True
    with pytest.raises(FrRuntimeError, match="incomplete"):
        ChangeScope(FrReport(data, ()))
    store = MemoryObjectStore()
    assert not ChangeScope.restore(store, result.persist(store)).ready


def test_unicode_occurrences_and_cycles_preserve_distinct_sites(tmp_path):
    root, client = fixture(tmp_path)
    for name in ("pricing.py", "api.py"):
        path = root/name
        path.write_text(path.read_text().replace("subtotal", "café"))
    path = root/"pricing.py"
    path.write_text("def café(a, b):\n    return cycle(a, b)\n"
                    "def cycle(a, b):\n    return café(a, b)\n"
                    "def twice(a, b):\n    return café(a, b) + café(a, b)\n")
    handle = client.project("find", "café").definition_target().handle
    result = ChangeScope.inspect(client, [handle], depth=8)
    assert result.ready
    sites = [row["occurrence"] for row in result.report.at("/references")
             if row["consumer"] and row["consumer"]["name"] == "twice"]
    assert len(sites) == 2 and sites[0]["id"] != sites[1]["id"]
    raw = path.read_bytes()
    for item in sites:
        start = item["location"]["span"]["start"]
        end = item["location"]["span"]["end"]
        assert raw[start:end].decode() == "café"
        position = item["location"]["range"]["start"]
        assert position["col"] == len(raw[:start].decode().split("\n")[-1])+1
    assert len(result.report.at("/consumers")) == 6


def test_uncertain_receiver_references_cannot_claim_review_readiness(tmp_path):
    root, client = fixture(tmp_path)
    path = root/"pricing.py"
    path.write_text(path.read_text()+"\ndef indirect(obj):\n    return obj.subtotal(1, 2)\n")
    result = scope(client)
    assert not result.ready
    assert result.report.at("/unresolved") or any(row["confidence"] not in {"exact", "import-qualified"}
                                                  for row in result.report.at("/references"))


@pytest.mark.parametrize("mapping", [
    {"schema": 1, "checks": [{"name": "missing", "paths": ["pricing.py"]}]},
    {"schema": 1, "checks": [{"name": "behavior", "paths": ["../pricing.py"]}]},
    {"schema": 1, "checks": [{"name": "behavior", "paths": ["pricing.py", "pricing.py"]}]},
    {"schema": 1, "checks": [{"name": "behavior", "paths": ["pricing.py"]}]*2},
])
def test_invalid_associations_refuse_before_discovery(tmp_path, mapping):
    root, client = fixture(tmp_path)
    (root/".fr/check-scopes.json").write_text(json.dumps(mapping))
    with pytest.raises(FrRuntimeError):
        scope(client)


def test_mapping_symlink_refuses_and_real_output_respects_bytes(tmp_path):
    root, client = fixture(tmp_path)
    result = scope(client, max_bytes=5000)
    assert len(json.dumps(result.report.to_data(), ensure_ascii=False, separators=(",", ":")).encode()) <= 5000
    path = root/".fr/check-scopes.json"
    path.rename(root/".fr/original.json")
    path.symlink_to("original.json")
    with pytest.raises(FrRuntimeError, match="symlink"):
        scope(client)
