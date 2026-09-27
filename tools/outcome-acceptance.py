#!/usr/bin/env python3
"""Compare compiler-only delivery with post-change acceptance and retained completion."""
from __future__ import annotations

import argparse
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
sys.path.insert(0,str(ROOT/"sdk/python/src"))
from fr_ir.context import DirectoryObjectStore
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
from fr_ir.investigation_delivery import DeliveryReceipt, attach_delivery, run_delivery
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient

FIXTURE = ROOT/"tests/agent-eval/checked-outcomes"


def save(path, value):
    path.write_text(json.dumps(value,indent=2,ensure_ascii=False)+"\n")


def run(args, root, *, check=True):
    return subprocess.run(args,cwd=root,check=check,capture_output=True,text=True,timeout=120)


def populate(root, fixture):
    (root/"src").mkdir(parents=True)
    (root/".fr").mkdir()
    (root/"artifacts").mkdir()
    for name in ("lib.rs","pricing.rs"):
        shutil.copyfile(fixture/name,root/"src"/name)
    shutil.copyfile(fixture/"oracle.py",root/".fr/oracle.py")
    (root/"README.md").write_text("Pinned independent documentation.\n")
    save(root/".fr/checks.json",{"schema":1,"checks":[
        {"name":"compile","argv":["rustc","--crate-type","lib","src/lib.rs","--emit","metadata","-o","artifacts/check.rmeta"],
         "cwd":".","timeout_seconds":30,"covers":["original, changed and restored Rust compilation"]},
        {"name":"behavior","argv":[sys.executable,"-B",".fr/oracle.py"],"cwd":".","timeout_seconds":30,
         "covers":["49 bounded public API outputs"],"identity_files":[".fr/oracle.py"]}]})
    run(["git","init","-q"],root)


def oracle(root):
    result = run([sys.executable,"-B",".fr/oracle.py"],root,check=False)
    return {"passed":result.returncode == 0,"exit_code":result.returncode,
            "stdout":result.stdout,"stderr":result.stderr[-4096:]}


def proposal(client, correct, acceptance):
    targets = []
    for name,fragment in (("subtotal","{ a + b }" if correct else "{ a * b }"),
                          ("quote","{ pricing::subtotal(a, b) + 3 }")):
        handle = client.project("find",name).definition_target().handle
        targets.append(TaskTarget(name,handle,"replace-body",fragment=fragment))
    return client.review(TaskChange([],targets,{"files-changed":2,"paths-changed":["src/lib.rs","src/pricing.rs"]},
        ["compile"],TaskDelivery(patch="artifacts/change.patch"),acceptance_checks=["behavior"] if acceptance else []))


def plan(client):
    value = TaskPlan("Repair the subtotal and add the requested fee",("behavior delivered",),(
        TaskStep.checked("outcome","Does the checked delivery meet the public requirement?",checks=("behavior",),satisfies=("behavior delivered",)),
        TaskStep("independent","Keep the documentation observation",(Dependency(DependencyKind.SOURCE,"README.md"),)),
    ))
    started = value.resume(client,transition="independent:start")
    observation = Evidence("read",EvidenceKind.OBSERVATION,started.input_digests["independent"],True,"README.md")
    value = replace(started.plan,steps=(started.plan.steps[0],replace(started.plan.steps[1],evidence=(observation,))))
    return value.resume(client,transition="independent:satisfy").plan


def reopen(args):
    store = DirectoryObjectStore(args.objects)
    client = FrClient(args.project,executable=args.fr)
    receipt = DeliveryReceipt.restore(store,args.receipt)
    current = TaskPlan.restore(store,args.plan).resume(client,transition="outcome:reset").plan
    current = current.resume(client,transition="outcome:start").plan
    attached = attach_delivery(current,client,"outcome",receipt)
    assert attached.complete
    return {"complete":attached.complete,"independent":attached.plan.steps[1].state.value}


def measure(binary, output):
    output.mkdir(parents=True,exist_ok=False)
    records = {}
    with tempfile.TemporaryDirectory(prefix="fr-outcome-acceptance-") as temporary:
        base = Path(temporary)
        for name,correct,acceptance in (("compiler-only",False,False),("rejected",False,True),("accepted",True,True)):
            root=base/name
            populate(root,FIXTURE)
            client=FrClient(root,executable=binary,timeout=120)
            baseline=oracle(root)
            assert not baseline["passed"]
            reviewed=proposal(client,correct,acceptance)
            started=time.monotonic()
            record: dict[str, Any]
            if acceptance:
                store=DirectoryObjectStore(output/name/"objects")
                delivered=run_delivery(plan(client),client,"outcome",reviewed,store)
                result=delivered.receipt.result
                record={"receipt_root":delivered.receipt_root,"plan_root":delivered.plan_root,
                        "started_plan_root":delivered.started_plan_root,"complete":delivered.resumed.complete,
                        "attachment_error":delivered.attachment_error}
                assert delivered.passed == correct and delivered.resumed.complete == correct
                if correct:
                    command=[sys.executable,str(Path(__file__).resolve()),"--reopen","--fr",str(binary),"--project",str(root),
                             "--objects",str(output/name/"objects"),"--receipt",delivered.receipt_root,"--plan",delivered.started_plan_root]
                    record["fresh_process"]=json.loads(run(command,ROOT).stdout)
                    (root/"src/pricing.rs").write_text((root/"src/pricing.rs").read_text()+"\n")
                    stale=TaskPlan.restore(store,delivered.plan_root).resume(client)
                    assert not stale.complete and stale.invalidated == ("outcome",)
                    record["invalidated"]=list(stale.invalidated)
                    source=root/"src/pricing.rs"
                    source.write_text(source.read_text()[:-1])
            else:
                result=client.execute(reviewed)
                assert result.passed
                record={"complete":False}
            observed=oracle(root)
            assert observed["passed"] == correct
            patch=root/"artifacts/change.patch"
            assert patch.exists() == (not acceptance or correct)
            if patch.exists():
                shutil.copyfile(patch,output/f"{name}.patch")
            record.update(review=reviewed.to_data(),delivery=result.to_data(),oracle=observed,baseline=baseline,
                          seconds=time.monotonic()-started,patch_delivered=patch.exists())
            records[name]=record
        save(output/"result.json",{"schema":"fr-checked-outcomes-result-1","cases":records,
             "live_agent":False,"source_equivalence_proven":False,
             "scope":"Deterministic prescribed changes on a two-file Rust fixture; bounded runtime oracle, local trusted evidence, no population or implementation-proof claim."})
    shutil.copytree(FIXTURE,output/"fixture")
    sources=["tools/outcome-acceptance.py","src/workflow.rs","src/project/task_change.rs","src/cli.rs","src/checks.rs",
        "src/checks/evidence.rs","src/project/investigation.rs","kernels/FrKernels/Workflow.lean",
        *[f"sdk/python/src/fr_ir/{name}.py" for name in ("ir","runtime","context","investigation","investigation_checks","investigation_delivery")]]
    for name in sources:
        target=output/"binding-sources"/name
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(ROOT/name,target)
    save(output/"manifest.json",{"schema":"fr-checked-outcomes-manifest-1",
        "revision":run(["git","rev-parse","HEAD"],ROOT).stdout.strip(),
        "binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
        "files":{str(p.relative_to(output)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(output.rglob('*')) if p.is_file()}})


def audit(output):
    manifest=json.loads((output/"manifest.json").read_text())
    assert manifest["schema"] == "fr-checked-outcomes-manifest-1"
    for name,digest in manifest["files"].items():
        path=output/name
        assert path.resolve().is_relative_to(output.resolve()) and not path.is_symlink()
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest,name
    value=json.loads((output/"result.json").read_text())
    assert value["live_agent"] is False and value["source_equivalence_proven"] is False
    cases=value["cases"]
    assert set(cases) == {"compiler-only","rejected","accepted"}
    for name in cases:
        row=cases[name]
        assert row["baseline"]["passed"] is False
        assert row["oracle"]["passed"] == (name == "accepted")
        assert row["delivery"]["passed"] == (name != "rejected")
        assert row["complete"] == (name == "accepted")
        assert row["patch_delivered"] == (name != "rejected")
        if name != "compiler-only":
            receipt=DeliveryReceipt.restore(DirectoryObjectStore(output/name/"objects"),row["receipt_root"])
            assert receipt.result.to_data() == row["delivery"]
            TaskPlan.restore(DirectoryObjectStore(output/name/"objects"),row["plan_root"])
    assert cases["accepted"]["fresh_process"] == {"complete":True,"independent":"satisfied"}
    assert cases["accepted"]["invalidated"] == ["outcome"]
    with tempfile.TemporaryDirectory(prefix="fr-outcome-receiver-") as temporary:
        for name in ("compiler-only","accepted"):
            root=Path(temporary)/name
            populate(root,output/"fixture")
            before={p.name:p.read_bytes() for p in (root/"src").iterdir()}
            patch=str((output/f"{name}.patch").resolve())
            run(["git","apply",patch],root)
            assert oracle(root)["passed"] == (name == "accepted")
            after={p.name:p.read_bytes() for p in (root/"src").iterdir()}
            run(["git","apply","--reverse",patch],root)
            assert before == {p.name:p.read_bytes() for p in (root/"src").iterdir()}
            run(["git","apply",patch],root)
            assert after == {p.name:p.read_bytes() for p in (root/"src").iterdir()}
    return {"passed":True,"cases":3,"receiver_cases":49,"fresh_process":True}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fr",type=Path,default=ROOT/"target/debug/fr")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--audit",type=Path)
    parser.add_argument("--reopen",action="store_true")
    for name in ("project","objects"):
        parser.add_argument(f"--{name}",type=Path)
    for name in ("receipt","plan"):
        parser.add_argument(f"--{name}")
    args=parser.parse_args()
    if args.reopen:
        value=reopen(args)
    else:
        if args.output:
            measure(args.fr.resolve(),args.output.resolve())
        value=audit(args.audit or args.output)
    print(json.dumps(value))


if __name__ == "__main__":
    main()
