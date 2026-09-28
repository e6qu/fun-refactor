import copy
from dataclasses import replace
import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.context import DirectoryObjectStore, MemoryObjectStore, store_merkle_value
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.investigation_delivery import DeliveryReceipt, attach_delivery, run_delivery
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrRuntimeError, TaskResult

FR = Path(__file__).resolve().parents[3]/"target/debug/fr"


def fixture(tmp_path, oracle=None):
    root = tmp_path/"project"
    root.mkdir()
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    (root/"README.md").write_text("independent documentation\n")
    (root/"subject.py").write_text("def total(a, b):\n    return a - b\n")
    (root/".fr/oracle.py").write_text(oracle or (
        "import sys\nsys.path.insert(0, '.')\nfrom subject import total\n"
        "for a in range(-3, 4):\n for b in range(-3, 4):\n  assert total(a, b) == sum([a, b])\n"))
    declarations = {"schema":1,"checks":[
        {"name":"syntax","argv":[sys.executable,"-B","-c","import ast; ast.parse(open('subject.py').read())"],
         "cwd":".","timeout_seconds":10,"covers":["syntax"]},
        {"name":"behavior","argv":[sys.executable,"-B",".fr/oracle.py"],"cwd":".",
         "timeout_seconds":10,"covers":["49 independent sums"],"identity_files":[".fr/oracle.py"],
         "environment":["FR_OUTCOME_TEST_INPUT"]}]}
    (root/".fr/checks.json").write_text(json.dumps(declarations))
    client = FrClient(root, executable=FR)
    plan = TaskPlan("repair addition", ("correct result",), (
        TaskStep.checked("deliver", "Does the delivered behavior pass?", checks=("behavior",), satisfies=("correct result",)),
        TaskStep("independent", "Keep documentation", (Dependency(DependencyKind.SOURCE,"README.md"),)),
    ))
    started = plan.resume(client, transition="independent:start")
    observation = Evidence("read",EvidenceKind.OBSERVATION,started.input_digests["independent"],True,"README.md")
    plan = replace(started.plan,steps=(started.plan.steps[0],replace(started.plan.steps[1],evidence=(observation,))))
    plan = plan.resume(client,transition="independent:satisfy").plan
    return root,client,plan


def review(client, body="return a + b"):
    handle = client.project("find","total").at("/rows/0/0")
    return client.review(TaskChange([], [TaskTarget("total",handle,"replace-body",fragment=body)],
        {"files-changed":1,"paths-changed":["subject.py"]}, ["syntax"],
        TaskDelivery(patch="artifacts/change.patch"), acceptance_checks=["behavior"]))


def test_post_change_acceptance_survives_reversal_and_completes_a_persisted_plan(tmp_path):
    root,client,plan = fixture(tmp_path)
    store = DirectoryObjectStore(tmp_path/"objects")
    result = run_delivery(plan,client,"deliver",review(client),store)
    assert result.passed and result.resumed.complete
    stages = result.receipt.result.at("/workflow/stages")
    assert [s["stage"] for s in stages].count("check-acceptance") == 2
    assert all(s["status"] == "passed" for s in stages)
    assert (root/"artifacts/change.patch").exists()
    assert result.resumed.plan.steps[1].state == StepState.SATISFIED
    assert DeliveryReceipt.restore(store,result.receipt_root).passed
    code = '''from pathlib import Path
import sys
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import TaskPlan
from fr_ir.investigation_delivery import DeliveryReceipt,attach_delivery
from fr_ir.runtime import FrClient
store=DirectoryObjectStore(Path(sys.argv[1])); client=FrClient(sys.argv[2],executable=sys.argv[3])
receipt=DeliveryReceipt.restore(store,sys.argv[4])
plan=TaskPlan.restore(store,sys.argv[5]).resume(client,transition='deliver:reset').plan
plan=plan.resume(client,transition='deliver:start').plan
assert attach_delivery(plan,client,'deliver',receipt).complete
'''
    subprocess.run([sys.executable,"-c",code,str(tmp_path/"objects"),str(root),str(FR),
                    result.receipt_root,result.started_plan_root],check=True,capture_output=True,text=True)


def test_compile_valid_wrong_repair_retains_failure_and_withholds_patch(tmp_path):
    root,client,plan = fixture(tmp_path)
    store = MemoryObjectStore()
    result = run_delivery(plan,client,"deliver",review(client,"return a * b"),store)
    assert not result.passed and not result.resumed.complete
    assert result.receipt.checks.at("/passed") is False
    assert result.receipt.result.at("/workflow/transaction_status") == "applied"
    assert not (root/"artifacts/change.patch").exists()
    assert "a * b" in (root/"subject.py").read_text()
    with pytest.raises(FrRuntimeError,match="failed delivery"):
        attach_delivery(result.resumed.plan,client,"deliver",DeliveryReceipt.restore(store,result.receipt_root))


@pytest.mark.parametrize("change", ["source","oracle","configuration","environment"])
def test_retained_outcomes_refuse_drift_and_preserve_independent_evidence(tmp_path, monkeypatch, change):
    root,client,plan = fixture(tmp_path)
    store = MemoryObjectStore()
    result = run_delivery(plan,client,"deliver",review(client),store)
    assert result.passed
    if change == "environment":
        monkeypatch.setenv("FR_OUTCOME_TEST_INPUT","changed")
    else:
        path = root/{"source":"subject.py","oracle":".fr/oracle.py","configuration":".fr/checks.json"}[change]
        path.write_text(path.read_text()+"\n")
    resumed = TaskPlan.restore(store,result.plan_root).resume(client)
    assert not resumed.complete and "deliver" in resumed.invalidated
    assert resumed.plan.steps[1].state == StepState.SATISFIED
    fresh = resumed.plan.resume(client,transition="deliver:reset").plan.resume(client,transition="deliver:start")
    with pytest.raises(FrRuntimeError,match="changed|differs"):
        attach_delivery(fresh.plan,client,"deliver",DeliveryReceipt.restore(store,result.receipt_root))


def test_changed_checker_refuses_the_mutation_review_before_writing(tmp_path):
    root,client,plan = fixture(tmp_path)
    reviewed = review(client)
    before = (root/"subject.py").read_bytes()
    (root/".fr/oracle.py").write_text("raise SystemExit(0)\n")
    with pytest.raises(FrRuntimeError,match="basis"):
        client.execute(reviewed)
    with pytest.raises(FrRuntimeError,match="transition"):
        run_delivery(plan,client,"deliver",reviewed,MemoryObjectStore())
    assert (root/"subject.py").read_bytes() == before


def test_acceptance_check_cannot_change_source_and_deliver(tmp_path):
    root,client,plan = fixture(tmp_path,"from pathlib import Path\nPath('subject.py').write_text('def total(a,b): return 0\\n')\n")
    result = run_delivery(plan,client,"deliver",review(client),MemoryObjectStore())
    assert not result.passed and not result.resumed.complete
    assert result.receipt.checks.at("/source_snapshot_stable") is False
    assert not (root/"artifacts/change.patch").exists()


def test_second_acceptance_failure_cannot_export_a_patch(tmp_path):
    oracle = """from pathlib import Path
p=Path('artifacts/attempt')
if p.exists(): raise SystemExit(1)
p.write_text('first passed')
"""
    root,client,plan = fixture(tmp_path,oracle)
    result = run_delivery(plan,client,"deliver",review(client),MemoryObjectStore())
    checks = [s for s in result.receipt.result.at("/workflow/stages") if s["stage"] == "check-acceptance"]
    assert [s["status"] for s in checks] == ["passed","failed"]
    assert not result.passed and not (root/"artifacts/change.patch").exists()


def test_receipt_rejects_laundered_outcomes_and_changed_requirements(tmp_path):
    _,client,plan = fixture(tmp_path)
    result = run_delivery(plan,client,"deliver",review(client),MemoryObjectStore())
    bad = copy.deepcopy(result.receipt.result.to_data())
    bad["workflow"]["stages"][2]["status"] = "pending"
    with pytest.raises(FrRuntimeError,match="incomplete stage"):
        DeliveryReceipt(result.receipt.manifest_json,TaskResult(bad,(),result.receipt.result.task_change_basis))
    changed = json.loads(result.receipt.manifest_json)
    changed["acceptance_checks"] = ["syntax"]
    with pytest.raises(FrRuntimeError,match="manifest identity"):
        DeliveryReceipt(json.dumps(changed),result.receipt.result)
    store = MemoryObjectStore()
    digest = store_merkle_value(store,{"schema":"invented"}).digest
    with pytest.raises(FrRuntimeError,match="not a delivery receipt"):
        DeliveryReceipt.restore(store,digest)


def test_plan_requirements_must_match_before_mutation(tmp_path):
    root,client,plan = fixture(tmp_path)
    plan = replace(plan,steps=(replace(plan.steps[0],required_checks=("syntax",)),plan.steps[1]))
    before = (root/"subject.py").read_bytes()
    with pytest.raises(FrRuntimeError,match="exactly the acceptance checks"):
        run_delivery(plan,client,"deliver",review(client),MemoryObjectStore())
    assert (root/"subject.py").read_bytes() == before


def test_passing_delivery_cannot_renew_a_changed_prerequisite(tmp_path):
    root,client,plan = fixture(tmp_path)
    diagnosis = TaskStep("diagnosis","Retain the pre-change explanation",
        (Dependency(DependencyKind.SOURCE,"subject.py"),))
    plan = replace(plan,steps=(*plan.steps,diagnosis))
    started = plan.resume(client,transition="diagnosis:start")
    evidence = Evidence("observation",EvidenceKind.OBSERVATION,started.input_digests["diagnosis"],True,"pre-change source")
    plan = replace(started.plan,steps=(*started.plan.steps[:2],replace(started.plan.steps[2],evidence=(evidence,))))
    plan = plan.resume(client,transition="diagnosis:satisfy").plan
    plan = replace(plan,steps=(replace(plan.steps[0],depends_on=("diagnosis",)),*plan.steps[1:]))
    result = run_delivery(plan,client,"deliver",review(client),MemoryObjectStore())
    assert result.receipt.passed and (root/"artifacts/change.patch").exists()
    assert not result.passed and not result.resumed.complete
    assert result.attachment_error is not None
    assert result.resumed.plan.steps[2].state == StepState.STALE
    assert result.resumed.plan.steps[1].state == StepState.SATISFIED
