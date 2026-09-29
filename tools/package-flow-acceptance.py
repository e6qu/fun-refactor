#!/usr/bin/env python3
"""Retain package diagnosis, resumed dependencies and checked two-patch delivery."""
from __future__ import annotations

import argparse
import ast
from dataclasses import replace
import hashlib
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
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, TaskPlan, TaskStep
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/package-flow"
SOURCES = ("app.py", "portal/__init__.py", "portal/api.py", "portal/transform.py",
           "archive/__init__.py", "archive/transform.py")
BINDINGS = ["src/project/flow_expressions.rs", "src/index/references.rs", "tools/package-flow-acceptance.py", "tools/evidence_basis.py", "Cargo.lock",
    *[f"src/project/{name}.rs" for name in ("dataflow", "flow_modules", "flow_summaries", "control_flow",
        "flow_cache", "investigation", "occurrence", "manifests", "lockfiles")],
    *[f"src/{name}.rs" for name in ("project", "span", "parse", "scan", "index", "extract")],
    *[f"sdk/python/src/fr_ir/{name}.py" for name in ("flow", "flow_storage", "flow_dependencies",
        "flow_summaries", "investigation", "context", "runtime", "ir")],
    *[f"tests/agent-eval/package-flow/{name}" for name in (*SOURCES, "oracle.py", "task.json", "baseline.json")]]


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True)


def copy_fixture(root):
    root.mkdir()
    for name in (*SOURCES, "oracle.py"):
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(FIXTURE / name, target)


def oracle(root, expected):
    return json.loads(run([sys.executable, "-B", str(FIXTURE / "oracle.py"), str(root),
                           "--expect", expected], root).stdout)


def analyze(client, name, cache=None):
    target = client.project("find", name).definition_target().handle
    return (cache or FlowCache(MemoryObjectStore())).analyze(client, target,
        rules=client.root / "rules.json", summaries=True, imports=True, steps=4096, max_bytes=1_048_576)


def equivalent(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def satisfy(plan, client, name):
    started = plan.resume(client, transition=f"{name}:start")
    evidence = Evidence("observation", EvidenceKind.OBSERVATION, started.input_digests[name], True, "package fixture")
    plan = replace(started.plan, steps=tuple(replace(step, evidence=(evidence,))
                   if step.id == name else step for step in started.plan.steps))
    return plan.resume(client, transition=f"{name}:satisfy").plan


class MeasuredClient(FrClient):
    calls = 0
    response_bytes = 0

    def call(self, *arguments, input_bytes=None):
        result = super().call(*arguments, input_bytes=input_bytes)
        self.calls += 1
        self.response_bytes += len(encode(result.to_data()))
        return result


def ordinary(root):
    start = time.perf_counter()
    sources = {name: (root / name).read_text() for name in SOURCES}
    trees = {name: ast.parse(source) for name, source in sources.items()}
    candidates = [(name, node) for name, tree in trees.items() for node in tree.body
        if isinstance(node, ast.FunctionDef) and len(node.args.args) == 1 and len(node.body) == 1
        and isinstance(node.body[0], ast.Return) and isinstance(node.body[0].value, ast.Name)
        and node.body[0].value.id == node.args.args[0].arg]
    assert len(candidates) == 1
    name, node = candidates[0]
    lines = sources[name].splitlines(keepends=True)
    statement = node.body[0]
    lines[statement.lineno-1:statement.end_lineno] = ["    return 0\n"]
    (root / name).write_text("".join(lines))
    repaired = oracle(root, "repair")
    public = next(name for name, tree in trees.items() if any(
        isinstance(node, ast.FunctionDef) and node.name == "render" for node in tree.body))
    safe = next(node for node in trees[public].body if isinstance(node, ast.FunctionDef) and node.name == "safe")
    expression = safe.body[0].value.args[0]
    with (root / public).open("a") as stream:
        stream.write(f"\ndef preview():\n    return {ast.unparse(expression)}\n")
    featured = oracle(root, "feature")
    return {"target": f"{name}::{node.name}", "public_path": public, "repair": repaired, "feature": featured,
            "seconds": time.perf_counter()-start, "source_reveals": len(sources),
            "context_bytes": sum(len(source.encode()) for source in sources.values()), "tokens": None,
            "scope": "Deterministic AST discovery, source editing and two runtime oracle calls; no live agent."}


def measure(args):
    bindings = {path: file_digest(ROOT / path) for path in BINDINGS}
    binary = hashlib.sha256(args.fr.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="fr-package-eval-") as temporary:
        work = Path(temporary)
        plain = work / "ordinary"
        copy_fixture(plain)
        ordinary_result = ordinary(plain)
        root = work / "project"
        copy_fixture(root)
        (root / "rules.json").write_text(json.dumps({"version":"package-1", "sources":["source"], "sinks":["sink"]}))
        (root / ".fr").mkdir()
        (root / "artifacts").mkdir()
        (root / ".fr/checks.json").write_text(json.dumps({"schema":1, "checks":[
            {"name":name, "argv":[sys.executable,"-c",
                "import subprocess,sys; modes=sys.argv[1:]; "
                "codes=[subprocess.run([sys.executable,'-B','oracle.py','.','--expect',m],capture_output=True).returncode for m in modes]; "
                "sys.exit(0 if 0 in codes else 1)", "baseline" if name == "repair" else "repair", name],
             "cwd":".","timeout_seconds":10,"covers":[f"Valid before/after states during {name} reversal; strict postconditions run independently."]}
            for name in ("repair","feature")]}))
        run(["git","init","-q"],root)
        run(["git","add","."],root)
        run(["git","-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","pinned package task"],root)
        client = MeasuredClient(root, executable=str(args.fr.resolve()), max_output_bytes=1_048_576)
        start = time.perf_counter()
        positive, negative = analyze(client,"render"), analyze(client,"safe")
        candidates = [item.function for item in positive.summaries.functions
            if item.return_parameters == (0,) and not item.callees and not item.sinks]
        assert len(candidates) == 1
        path, member = candidates[0].split("::")
        selected = client.project("find",member,"--in",path).definition_target()
        public = client.project("find","render").definition_target().path
        safe_summary = negative.summaries.for_function(f"{public}::safe")
        assert len(safe_summary.callees) == 1
        safe_path, safe_member = safe_summary.callees[0].split("::")
        lookup = next(item for item in negative.dependencies.lookups if item.importer == public and item.target == safe_path)
        expression = lookup.alias if lookup.member else f"{lookup.prefix}.{safe_member}"
        discovery = {"seconds":time.perf_counter()-start,"process_calls":client.calls,
                     "context_bytes":client.response_bytes,"source_reveals":0,"tokens":None,
                     "target":candidates[0],"public_path":public,"preview_callee":expression}
        plan = TaskPlan("Repair package input flow", ("explained",), (
            TaskStep("flow","Which helper carries the input?",(positive.dependencies.dependency,),satisfies=("explained",)),
            TaskStep("conclusion","What follows?",(Dependency(DependencyKind.SOURCE,public),),depends_on=("flow",)),
            TaskStep("independent","Keep the pinned note",(Dependency(DependencyKind.SOURCE,"oracle.py"),)),
        ))
        for name in ("flow","conclusion","independent"):
            plan = satisfy(plan,client,name)
        store = MemoryObjectStore()
        plan_root = plan.store(store)
        reopened = TaskPlan.restore(store,plan_root).resume(client)
        cache = FlowCache(store)
        analyze(client,"render",cache)
        warm = analyze(client,"render",cache)
        mutations = {}
        for name, mutation_path, contents in (
            ("initializer","portal/__init__.py",'"changed initializer"\n'),
            ("helper",path,f"def {member}(value):\n    return 0\n"),
            ("parent-shadow","portal.py",""),
            ("leaf-stub","portal/api.pyi",""),
            ("unrelated","unrelated.py","def other():\n    return 3\n"),
        ):
            target = root / mutation_path
            original = target.read_bytes() if target.exists() else None
            target.write_text(contents)
            resumed = TaskPlan.restore(store,plan_root).resume(client)
            edited, clean = analyze(client,"render",cache), analyze(client,"render")
            assert equivalent(edited) == equivalent(clean)
            mutations[name] = {"resume":resumed.report.to_data(),"reused":edited.reused,
                               "analysis":edited.report.to_data(),"clean":clean.report.to_data()}
            if original is None: target.unlink()
            else: target.write_bytes(original)
            analyze(client,"render",cache)
        def change(handle, kind):
            fragment = "return 0" if kind == "repair" else f"def preview():\n    return {expression}(source())\n"
            return TaskChange([], [TaskTarget(kind,handle,"replace-body" if kind == "repair" else "insert-declaration",fragment=fragment)],
                {"files-changed":1,"edits":1,"paths-changed":[path if kind == "repair" else public]},
                [kind],TaskDelivery(patch=f"artifacts/{kind}.patch"))
        stale_review = client.review(change(selected.handle,"repair"))
        parent = root / "portal/__init__.py"
        original = parent.read_bytes()
        parent.write_bytes(original + b"\n# revised parent\n")
        try: client.execute(stale_review)
        except FrRuntimeError as error: refusal = str(error)
        else: raise AssertionError("stale package review admitted")
        parent.write_bytes(original)
        records = {}
        for kind in ("repair","feature"):
            handle = (client.project("find",member,"--in",path).definition_target().handle if kind == "repair" else
                      client.project("map",public,"--depth","0","--fields","handle,kind").at("/rows/0/0"))
            review = client.review(change(handle,kind))
            assert review.at("/ready")
            delivery = client.execute(review)
            assert delivery.passed
            records[kind] = {"review":review.to_data(),"delivery":delivery.to_data(),
                "patch":(root/f"artifacts/{kind}.patch").read_text(),"oracle":oracle(root,kind)}
        receiver = work / "receiver"
        copy_fixture(receiver)
        run(["git","init","-q"],receiver)
        for kind in ("repair","feature"):
            patch = root / f"artifacts/{kind}.patch"
            run(["git","apply","--check",str(patch)],receiver)
            run(["git","apply",str(patch)],receiver)
        result = {"schema":"fr-package-flow-acceptance-1", "source_bindings":bindings,
            "repository_revision":run(["git","rev-parse","HEAD"],ROOT).stdout.strip(),"platform":platform.platform(),
            "binary_sha256":binary,"baseline":json.loads((FIXTURE/"baseline.json").read_text()),
            "oracle":oracle(FIXTURE,"baseline"),"ordinary":ordinary_result,"discovery":discovery,
            "positive":positive.report.to_data(),"negative":negative.report.to_data(),"warm_reused":warm.reused,
            "plan":plan.to_data(),"plan_root":plan_root,"reopened":reopened.report.to_data(),"mutations":mutations,
            "stale_review_refusal":refusal,"records":records,"receiver":oracle(receiver,"feature"),
            "after":analyze(client,"render").report.to_data(),"resumed_after":plan.resume(client).report.to_data(),
            "false_claims":0,"live_agent":False,"scope":json.loads((FIXTURE/"task.json").read_text())["claims"]}
        assert bindings == {path:file_digest(ROOT/path) for path in BINDINGS}
        assert binary == hashlib.sha256(args.fr.read_bytes()).hexdigest()
        return result


def audit(value):
    assert value["schema"] == "fr-package-flow-acceptance-1"
    assert value["source_bindings"] == {path:file_digest(ROOT/path) for path in BINDINGS}
    assert value["baseline"] == json.loads((FIXTURE/"baseline.json").read_text())
    assert value["oracle"] == oracle(FIXTURE,"baseline")
    assert value["false_claims"] == 0 and value["live_agent"] is False
    assert value["scope"] == json.loads((FIXTURE/"task.json").read_text())["claims"]
    for name, expected in (("positive",True),("negative",False),("after",False)):
        report = value[name]
        assert report["complete"] and bool(report["witnesses"]) == expected
        assert FlowDependencies.from_report(FrReport(report,())).complete
        for witness in report["witnesses"]:
            for point in witness["trace"]["occurrences"]:
                assert point["revision"] == report["revision"]
                if point["role"] in {"source","sink","call-result","summary-call"}:
                    span = point["location"]["span"]
                    assert [span["start"],span["end"]] in value["oracle"]["call_spans"][point["path"]]
    summaries = FunctionSummaries.from_report(FrReport(value["positive"],()))
    candidates = [item.function for item in summaries.functions
                  if item.return_parameters == (0,) and not item.callees and not item.sinks]
    assert candidates == [value["discovery"]["target"]] == [value["ordinary"]["target"]]
    assert value["discovery"]["public_path"] == value["ordinary"]["public_path"] == "app.py"
    assert value["warm_reused"] and value["reopened"]["complete"]
    assert store_merkle_value(MemoryObjectStore(),value["plan"]).digest == value["plan_root"]
    assert set(value["mutations"]) == {"initializer","helper","parent-shadow","leaf-stub","unrelated"}
    for name, mutation in value["mutations"].items():
        assert mutation["resume"]["invalidated"] == ([] if name == "unrelated" else ["flow","conclusion"])
        assert mutation["resume"]["plan"]["steps"][2]["state"] == "satisfied"
        assert mutation["reused"] == (name == "unrelated")
        assert mutation["analysis"]["complete"] == (name in {"helper", "unrelated"} or name == "initializer"
                and json.loads((FIXTURE / "task.json").read_text()).get("initializer_mutation_complete", True))
        FlowDependencies.from_report(FrReport(mutation["analysis"],()))
        left, right = mutation["analysis"].copy(), mutation["clean"].copy()
        for item in (left,right):
            item.pop("execution")
            item.pop("context_basis",None)
        assert left == right
    assert value["stale_review_refusal"] and value["resumed_after"]["invalidated"] == ["flow","conclusion"]
    assert value["discovery"]["source_reveals"] == 0 and value["ordinary"]["source_reveals"] == len(SOURCES)
    for name in ("ordinary","discovery"):
        assert value[name]["seconds"] >= 0 and value[name]["context_bytes"] > 0 and value[name]["tokens"] is None
    with tempfile.TemporaryDirectory(prefix="fr-package-replay-") as temporary:
        plain = Path(temporary)/"ordinary"
        copy_fixture(plain)
        ordinary_result = ordinary(plain)
        for key in ("target", "public_path", "repair", "feature", "source_reveals", "context_bytes", "tokens", "scope"):
            assert value["ordinary"][key] == ordinary_result[key]
        root = Path(temporary)/"receiver"
        copy_fixture(root)
        run(["git","init","-q"],root)
        for kind in ("repair","feature"):
            record = value["records"][kind]
            assert record["review"]["ready"] and record["delivery"]["passed"]
            stages = record["delivery"]["workflow"]["stages"]
            assert {"apply","undo","redo","deliver-patch"} <= {s["stage"] for s in stages}
            assert all(stage["status"] == "passed" for stage in stages)
            patch = root / "change.patch"
            patch.write_text(record["patch"])
            run(["git","apply","--check",str(patch)],root)
            run(["git","apply",str(patch)],root)
            observed = oracle(root,kind)
            assert observed == record["oracle"]
            assert observed["results"] == value["ordinary"][kind]["results"]
        assert observed == value["receiver"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr",type=Path,default=ROOT/"target/debug/fr")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--audit",type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    text = json.dumps(value,indent=2)+"\n"
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(text)
    else: print(text,end="")


if __name__ == "__main__":
    main()
