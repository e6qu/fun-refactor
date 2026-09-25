#!/usr/bin/env python3
"""Retain imported flow, negative dependencies, resumed plans and checked delivery."""
from __future__ import annotations

import argparse
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdk/python/src"))
from evidence_basis import file_digest
from fr_ir.context import DirectoryObjectStore, MemoryObjectStore, store_merkle_value
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_facts import FlowFacts
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/imported-flow"
NAMES = ("app", "relay", "leaf", "erase", "effects")
BINDINGS = ["tools/imported-flow-acceptance.py", "tools/evidence_basis.py", "src/project/dataflow.rs",
            "src/project/flow_modules.rs", "src/project/flow_summaries.rs", "src/project/control_flow.rs",
            "src/project/flow_cache.rs", "src/project/flow_facts.rs", "src/project/flow_fact_origins.rs",
            "src/project/investigation.rs", "src/project/occurrence.rs", "src/project/manifests.rs",
            "src/project/lockfiles.rs", "src/project.rs", "src/span.rs", "src/parse.rs", "src/scan.rs",
            "src/index.rs", "src/extract.rs", "Cargo.lock", "sdk/python/src/fr_ir/flow.py",
            "sdk/python/src/fr_ir/flow_dependencies.py", "sdk/python/src/fr_ir/flow_summaries.py",
            "sdk/python/src/fr_ir/flow_facts.py", "sdk/python/src/fr_ir/investigation.py",
            "sdk/python/src/fr_ir/context.py", "sdk/python/src/fr_ir/runtime.py",
            "kernels/FrKernels/Investigation.lean", "kernels/InvestigationMain.lean"] + [
                f"tests/agent-eval/imported-flow/{name}" for name in
                ("app.py", "relay.py", "leaf.py", "erase.py", "effects.py", "oracle.py", "task.json", "baseline.json", "diagnostics.json")]


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True)


def client_for(work, executable):
    return FrClient(work / "project", executable=str(executable.resolve()), max_output_bytes=1_048_576)


def analyze(client, name="positive", cache=None, context="html"):
    target = client.project("find", name).definition_target().handle
    return (cache or FlowCache(MemoryObjectStore())).analyze(client, target,
        rules=client.root / "rules.json", context=context, steps=4096,
        summaries=True, imports=True, max_bytes=1_048_576)


def equivalent(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def satisfied(plan, client, name):
    started = plan.resume(client, transition=f"{name}:start")
    evidence = Evidence("observation", EvidenceKind.OBSERVATION, started.input_digests[name], True, "retained flow fixture")
    plan = replace(started.plan, steps=tuple(replace(step, evidence=(evidence,)) if step.id == name else step for step in started.plan.steps))
    return plan.resume(client, transition=f"{name}:satisfy").plan


def worker(args):
    store = DirectoryObjectStore(args.work / "objects")
    manifest = args.work / "cache.txt"
    cache = FlowCache.restore(store, manifest.read_text()) if args.worker == "warm" else FlowCache(store)
    start = time.perf_counter()
    result = analyze(client_for(args.work, args.fr), cache=cache)
    seconds = time.perf_counter() - start
    if args.worker == "cold":
        manifest.write_text(cache.persist())
    scale = 1 if sys.platform == "darwin" else 1024
    return {"evidence_digest":digest(equivalent(result)), "complete":result.report.at("/complete"),
            "input_digest":result.report.at("/input_digest"), "reused":result.reused,
            "analysis_steps":result.report.at("/execution/analysis_steps"), "elapsed_seconds":seconds,
            "context_bytes":len(encode(result.report.to_data())), "witnesses":len(result.witnesses),
            "peak_native_child_rss_bytes":resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale,
            "peak_worker_rss_bytes":resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
            "process_calls":3, "source_reveals":0, "tokens":None}


def measure(args):
    start = time.perf_counter()
    oracle = json.loads(run([sys.executable, str(FIXTURE / "oracle.py"), str(FIXTURE), "--check"], ROOT).stdout)
    ordinary_seconds = time.perf_counter() - start
    with tempfile.TemporaryDirectory(prefix="fr-import-eval-") as temporary:
        work = Path(temporary)
        root = work / "project"
        root.mkdir()
        for name in NAMES:
            shutil.copyfile(FIXTURE / f"{name}.py", root / f"{name}.py")
        shutil.copyfile(FIXTURE / "oracle.py", root / "oracle.py")
        (root / "independent.py").write_text("def independent():\n    return 99\n")
        rules = root / "rules.json"
        rules.write_text(json.dumps({"version":"imported-1","sources":["source"],"sinks":["sink"],"sanitizers":{"clean":"html"}}))
        (root / ".fr").mkdir()
        (root / "artifacts").mkdir()
        (root / ".fr/checks.json").write_text(json.dumps({"schema":1,"checks":[{"name":"behavior",
            "argv":[sys.executable,"-B","oracle.py",".","--check"],"cwd":".","timeout_seconds":10,
            "covers":["Independent Python runtime oracle for ten imported flow cases."]}]}))
        run(["git", "init", "-q"], root)
        run(["git", "add", "."], root)
        run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "pinned imported flow"], root)
        client = client_for(work, args.fr)
        cases = {context: {name: analyze(client, name, context=context).report.to_data()
                           for name in expected["outcomes"]} for context, expected in oracle.items()}
        observed = analyze(client)
        plan = TaskPlan("Retain imported value-flow evidence", ("explained",), (
            TaskStep("flow", "Does the source reach the sink through imported helpers?", (observed.dependencies.dependency,), satisfies=("explained",)),
            TaskStep("dependent", "What follows from this flow?", (Dependency(DependencyKind.SOURCE,"app.py"),), depends_on=("flow",)),
            TaskStep("independent", "Read independent source", (Dependency(DependencyKind.SOURCE,"independent.py"),)),
        ))
        for name in ("flow", "dependent", "independent"):
            plan = satisfied(plan, client, name)
        store = MemoryObjectStore()
        plan_root = plan.store(store)
        reopened = TaskPlan.restore(store, plan_root).resume(client)
        assert reopened.complete
        mutations = {}
        original_leaf = (root / "leaf.py").read_text()
        original_rules = rules.read_text()
        for name in ("helper", "delete", "missing-member", "package", "stub", "configuration", "rules"):
            if name == "helper": (root / "leaf.py").write_text("def identity(value):\n    return 0\n")
            elif name == "delete": (root / "leaf.py").unlink()
            elif name == "missing-member": (root / "leaf.py").write_text("def other(value):\n    return value\n")
            elif name == "package":
                (root / "leaf").mkdir()
                (root / "leaf/__init__.py").write_text("def identity(value):\n    return 0\n")
            elif name == "stub": (root / "leaf.pyi").write_text("def identity(value): ...\n")
            elif name == "configuration": (root / "pyproject.toml").write_text('[project]\nname="changed"\n')
            else: rules.write_text(original_rules.replace("imported-1", "imported-2"))
            resumed = plan.resume(client)
            assert resumed.invalidated == ("flow", "dependent")
            assert resumed.plan.steps[2].state == StepState.SATISFIED
            mutations[name] = {"resume":resumed.report.to_data(), "analysis":analyze(client).report.to_data()}
            (root / "leaf.py").write_text(original_leaf)
            rules.write_text(original_rules)
            for extra in ("leaf/__init__.py", "leaf.pyi", "pyproject.toml"):
                (root / extra).unlink(missing_ok=True)
            if (root / "leaf").exists(): (root / "leaf").rmdir()
        arms = {}
        def arm(name, mode):
            arms[name] = json.loads(run([sys.executable, str(Path(__file__).resolve()), "--worker", mode,
                "--work", str(work), "--fr", str(args.fr.resolve())], ROOT).stdout)
        arm("cold", "cold")
        arm("warm", "warm")
        (root / "unrelated.py").write_text("def other():\n    return 42\n")
        independent_resume = plan.resume(client).report.to_data()
        arm("unrelated_edit", "warm")
        arm("unrelated_clean", "clean")
        (root / "leaf.py").write_text("def identity(value):\n    return 0\n")
        arm("helper_edit", "warm")
        arm("helper_clean", "clean")
        (root / "leaf.py").write_text(original_leaf)
        (root / "unrelated.py").unlink()
        target = client.project("find", "positive").definition_target().handle
        catalogue = FlowFacts.inspect(client, target, rules=rules, imports=True, steps=4096, limit=64, max_bytes=1_048_576)
        witness = next(item for item in catalogue.items if item.kind == "witness")
        page = catalogue.explain(client, witness)
        fact_pages = [page.report.to_data()]
        while page.report.at("/continuation") is not None:
            page = page.next(client)
            fact_pages.append(page.report.to_data())
        target = client.project("find", "identity").definition_target().handle
        def change(handle):
            return TaskChange([], [TaskTarget("preserve",handle,"replace-body",fragment="temporary = value\nreturn temporary")],
                {"files-changed":1,"edits":1,"paths-changed":["leaf.py"]}, ["behavior"], TaskDelivery(patch="artifacts/imported.patch"))
        old_review = client.review(change(target))
        (root / "relay.py").write_text((root / "relay.py").read_text() + "\n# A changed helper needs a fresh review.\n")
        try:
            client.execute(old_review)
        except FrRuntimeError as error:
            refusal = str(error)
        else:
            raise AssertionError("stale imported-helper review admitted")
        target = client.project("find", "identity").definition_target().handle
        reviewed = client.review(change(target))
        delivery = client.execute(reviewed)
        assert delivery.passed
        fresh = analyze(client)
        stale = plan.resume(client)
        assert stale.invalidated == ("flow", "dependent") and fresh.report.at("/complete")
        receiver = work / "receiver"
        receiver.mkdir()
        for name in NAMES:
            shutil.copyfile(FIXTURE / f"{name}.py", receiver / f"{name}.py")
        run(["git", "init", "-q"], receiver)
        patch = root / "artifacts/imported.patch"
        run(["git", "apply", "--check", str(patch)], receiver)
        run(["git", "apply", str(patch)], receiver)
        behavior = json.loads(run([sys.executable, "-B", str(FIXTURE / "oracle.py"), str(receiver), "--check"], receiver).stdout)
        return {"schema":"fr-imported-flow-acceptance-1", "repository_revision":run(["git","rev-parse","HEAD"],ROOT).stdout.strip(),
            "source_bindings":{path:file_digest(ROOT/path) for path in BINDINGS}, "platform":platform.platform(),
            "binary_sha256":hashlib.sha256(args.fr.read_bytes()).hexdigest(), "baseline":json.loads((FIXTURE/"baseline.json").read_text()),
            "oracle":oracle, "ordinary_runtime_ast_seconds":ordinary_seconds, "cases":cases,"mutations":mutations,
            "arms":arms,"plan":plan.to_data(),"plan_root":plan_root,"reopened":reopened.report.to_data(),"independent_resume":independent_resume,
            "fact_pages":fact_pages,"stale_review_refusal":refusal,"review":reviewed.to_data(),"delivery":delivery.to_data(),
            "patch":patch.read_text(),"receiver_behavior":behavior,"fresh":fresh.report.to_data(),"stale_after_delivery":stale.report.to_data(),
            "false_claims":0, "scope":"Static root-local Python module closure and caller-authored external rule contracts. Whole-module reuse; no package loader, cyclic initialization, feasible-path, source-proof, live-agent or general speed claim. Measurements describe this finite fixture."}


def audit(value):
    assert value["schema"] == "fr-imported-flow-acceptance-1"
    assert value["source_bindings"] == {path:file_digest(ROOT/path) for path in BINDINGS}
    assert value["baseline"] == json.loads((FIXTURE/"baseline.json").read_text())
    oracle = json.loads(run([sys.executable, "-B", str(FIXTURE/"oracle.py"), str(FIXTURE), "--check"], ROOT).stdout)
    assert value["oracle"] == oracle and value["false_claims"] == 0
    for context, expected in oracle.items():
        assert set(value["cases"][context]) == set(expected["outcomes"])
        for name, report in value["cases"][context].items():
            dependencies = FlowDependencies.from_report(FrReport(report, ()))
            assert dependencies.complete and report["complete"]
            assert bool(report["witnesses"]) == expected["outcomes"][name]
            for witness in report["witnesses"]:
                for point in witness["trace"]["occurrences"]:
                    assert point["revision"] == report["revision"]
                    if point["role"] in {"source","sink","call-result","summary-call"}:
                        span = point["location"]["span"]
                        assert [span["start"],span["end"]] in expected["call_spans"][point["path"]]
    assert set(value["mutations"]) == {"helper", "delete", "missing-member", "package", "stub", "configuration", "rules"}
    for name, mutation in value["mutations"].items():
        assert mutation["resume"]["invalidated"] == ["flow","dependent"]
        assert mutation["resume"]["plan"]["steps"][2]["state"] == "satisfied"
        assert mutation["analysis"]["complete"] == (name in {"helper","configuration","rules"})
        FlowDependencies.from_report(FrReport(mutation["analysis"], ()))
    assert value["reopened"]["complete"] and value["independent_resume"]["complete"]
    assert not value["independent_resume"]["invalidated"]
    assert store_merkle_value(MemoryObjectStore(),value["plan"]).digest == value["plan_root"]
    assert value["stale_after_delivery"]["invalidated"] == ["flow","dependent"]
    for first, second in [("cold","warm"),("unrelated_edit","unrelated_clean"),("helper_edit","helper_clean")]:
        assert value["arms"][first]["evidence_digest"] == value["arms"][second]["evidence_digest"]
    for name, arm in value["arms"].items():
        assert arm["complete"] and arm["reused"] == (name in {"warm","unrelated_edit"})
        assert (arm["analysis_steps"] == 0) == arm["reused"]
        assert bool(arm["witnesses"]) == (not name.startswith("helper_"))
        assert arm["elapsed_seconds"] >= 0 and arm["peak_native_child_rss_bytes"] > 0
    paths = set()
    for raw in value["fact_pages"]:
        page = FlowFacts.from_report(FrReport(raw, ()))
        paths.update(point.occurrence.path for point in page.evidence)
    assert paths == {"app.py","relay.py","leaf.py"}
    assert value["stale_review_refusal"] and value["delivery"]["passed"]
    stages = value["delivery"]["workflow"]["stages"]
    assert {"apply", "undo", "redo", "deliver-patch"} <= {stage["stage"] for stage in stages}
    assert all(stage["status"] == "passed" for stage in stages)
    assert value["fresh"]["complete"] and value["fresh"]["witnesses"]
    with tempfile.TemporaryDirectory(prefix="fr-import-replay-") as directory:
        root = Path(directory)
        for name in NAMES:
            shutil.copyfile(FIXTURE/f"{name}.py",root/f"{name}.py")
        patch = root/"change.patch"
        patch.write_text(value["patch"])
        run(["git","init","-q"],root)
        run(["git","apply","--check",str(patch)],root)
        run(["git","apply",str(patch)],root)
        oracle = json.loads(run([sys.executable,"-B",str(FIXTURE/"oracle.py"),str(root),"--check"],root).stdout)
        assert oracle == value["receiver_behavior"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr",type=Path,default=ROOT/"target/debug/fr")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--audit",type=Path)
    parser.add_argument("--worker",choices=["cold","warm","clean"])
    parser.add_argument("--work",type=Path)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args)))
        return
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    text = json.dumps(value,indent=2)+"\n"
    if args.output: args.output.write_text(text)
    else: print(text,end="")


if __name__ == "__main__":
    main()
