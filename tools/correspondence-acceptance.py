#!/usr/bin/env python3
"""Retain syntax correspondence, explicit plan refresh and checked delivery evidence."""
from __future__ import annotations

import argparse
from dataclasses import replace
import importlib.util
import json
import os
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
from fr_ir.correspondence import DeclarationSnapshot, IdentityPage
from fr_ir.investigation import Dependency, DependencyKind, Evidence, EvidenceKind, StepState, TaskPlan, TaskStep
from fr_ir.investigation_session import InvestigationSession, target_inputs
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/resumable-correspondence"
BINDINGS = ["tools/correspondence-acceptance.py", "tools/evidence_basis.py", "src/project/correspondence.rs",
            "src/project/investigation.rs", "src/project/occurrence.rs", "src/project.rs", "src/parse.rs", "src/span.rs", "Cargo.lock",
            "sdk/python/src/fr_ir/correspondence.py", "sdk/python/src/fr_ir/investigation_session.py",
            "sdk/python/src/fr_ir/investigation.py", "sdk/python/src/fr_ir/context.py", "sdk/python/src/fr_ir/runtime.py",
            "kernels/FrKernels/Correspondence.lean", "kernels/CorrespondenceMain.lean"] + [
                f"tests/agent-eval/resumable-correspondence/{name}" for name in ("task.json", "subject.py", "oracle.py", "baseline.json", "diagnostics.json")]


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def oracle_module():
    spec = importlib.util.spec_from_file_location("correspondence_oracle", FIXTURE / "oracle.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, check=True)


def satisfied(plan, client, id):
    started = plan.resume(client, transition=f"{id}:start")
    evidence = Evidence("coordinates", EvidenceKind.OBSERVATION, started.input_digests[id], True, "independent-ast-oracle")
    updated = replace(started.plan, steps=tuple(replace(s, evidence=(evidence,)) if s.id == id else s for s in started.plan.steps))
    return updated.resume(client, transition=f"{id}:satisfy").plan


def worker(args):
    command = [str(args.fr.resolve()), "--json", "-C", str(args.work)]
    if args.worker == "clean":
        command.append("--no-cache")
    start = time.perf_counter()
    retained = args.work.parent / "timing-snapshot.json"
    environment = dict(os.environ, FUN_REFACTOR_CACHE=str(args.work.parent / "native-cache"))
    output = subprocess.run(command + ["project", "identities", "--from", str(retained), "--limit", "16"],
                            cwd=args.work, env=environment, capture_output=True, text=True, check=True)
    elapsed = time.perf_counter() - start
    scale = 1 if sys.platform == "darwin" else 1024
    report = json.loads(output.stdout)
    IdentityPage.from_report(FrReport(report, ()))
    return {"report": report, "elapsed_seconds": elapsed, "context_bytes": len(output.stdout.encode()),
            "peak_native_child_rss_bytes": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * scale,
            "peak_worker_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * scale,
            "process_calls": 1, "source_reveals": 0, "tokens": None,
            "correspondence_recomputed": True, "index_cache": args.worker != "clean"}


def measure(args):
    source = (FIXTURE / "subject.py").read_text()
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    started = time.perf_counter()
    ordinary = oracle_module().inspect(source)
    ordinary_seconds = time.perf_counter() - started
    with tempfile.TemporaryDirectory(prefix="fr-correspondence-eval-") as directory:
        work = Path(directory)
        project = work / "project"
        project.mkdir()
        (project / "subject.py").write_text(source)
        (project / "independent.py").write_text("def independent():\n    return 99\n")
        (project / ".fr").mkdir()
        (project / "artifacts").mkdir()
        (project / ".fr/checks.json").write_text(json.dumps({"schema": 1, "checks": [{"name": "syntax",
            "argv": [sys.executable, "-c", "import ast,pathlib; [ast.parse(p.read_text()) for p in pathlib.Path('.').glob('*.py')]"],
            "cwd": ".", "timeout_seconds": 10, "covers": ["independent Python parser; behavior checked separately"]}]}))
        run(["git", "init", "-q"], project)
        run(["git", "add", "."], project)
        run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "pinned fixture"], project)
        client = FrClient(project, executable=str(args.fr.resolve()))
        snapshot = DeclarationSnapshot.capture(client, "subject.py", limit=2)
        coordinates = [{"name": i.name, "span": {"start": i.occurrence.location.span.start,
                       "end": i.occurrence.location.span.end}} for i in snapshot.items if i.kind == "function"]
        assert coordinates == ordinary == oracle["declarations"]
        snapshot = snapshot.subset(tuple(i.handle for i in snapshot.items if i.name == "café"))
        old = snapshot.items[0]
        store = DirectoryObjectStore(work / "objects")
        snapshot_root = snapshot.persist(store)
        assert DeclarationSnapshot.restore(store, snapshot_root) == snapshot
        plan = TaskPlan("resume the selected diagnosis", ("diagnosed",), (
            TaskStep("target", "which declaration implements the result?", target_inputs(),
                     action=("project", "show", old.handle), satisfies=("diagnosed",)),
            TaskStep("dependent", "what depends on this finding?", (Dependency(DependencyKind.SOURCE, "subject.py"),), depends_on=("target",)),
            TaskStep("independent", "does the unrelated observation remain valid?", (Dependency(DependencyKind.SOURCE, "independent.py"),)),
        ))
        for id in ("independent", "target", "dependent"):
            plan = satisfied(plan, client, id)
        session = InvestigationSession.bind(plan, {"target": snapshot})
        session_root = session.persist(store)
        interrupted = plan.resume(client, transition="target:reset").plan.resume(client, transition="target:start").plan
        reopened = InvestigationSession.bind(interrupted, {"target": snapshot}).resume(client)
        assert reopened.resumed.plan.steps[0].state == StepState.READY and not reopened.resumed.complete
        old_change = TaskChange([], [TaskTarget("repair", old.handle, "replace-body", fragment="return value + 2")],
                                {"files-changed": 1, "edits": 1, "paths-changed": ["subject.py"]}, ["syntax"])
        old_review = client.review(old_change)
        assert old_review.at("/ready")
        cases = {}
        for name in ("move", "rename", "changed", "copy-and-change", "duplicates", "deleted"):
            for path in (project / "subject.py", project / "moved.py", project / "copy.py"):
                path.unlink(missing_ok=True)
            if name == "move":
                (project / "moved.py").write_text(source)
            elif name == "rename":
                (project / "moved.py").write_text(source.replace("café", "renamed"))
            elif name != "deleted":
                (project / "subject.py").write_text(source.replace("+ 1", "+ 2") if name in {"changed", "copy-and-change"} else source)
            if name in {"copy-and-change", "duplicates"}:
                (project / "copy.py").write_text(source)
            start = time.perf_counter()
            comparison = snapshot.compare(client)
            cases[name] = {"pages": [p.report.to_data() for p in comparison.pages], "complete": comparison.complete,
                           "seconds": time.perf_counter() - start, "context_bytes": sum(len(encoded(p.report.to_data())) for p in comparison.pages)}
        (project / "moved.py").write_text(source)
        resumed = InvestigationSession.restore(store, session_root).resume(client)
        assert resumed.resumed.invalidated == ("target", "dependent")
        match = resumed.correspondence["target"].matches[0]
        selected = match.candidates[0]
        refreshed = resumed.refresh(client, "target", {old.handle: selected.handle}, inputs=target_inputs("moved.py"))
        assert refreshed.plan.steps[0].state == StepState.READY
        assert not refreshed.plan.steps[0].action and not refreshed.plan.steps[0].evidence
        assert refreshed.plan.steps[1].state == StepState.STALE
        assert refreshed.plan.steps[2] == resumed.resumed.plan.steps[2]
        try:
            client.execute(old_review)
        except FrRuntimeError as error:
            stale_review = str(error)
        else:
            raise AssertionError("stale mutation review executed")
        assert (project / "moved.py").read_text() == source
        current = refreshed.targets["target"].items[0]
        change = TaskChange([], [TaskTarget("repair", current.handle, "replace-body", fragment="return value + 2")],
                            {"files-changed": 1, "edits": 1, "paths-changed": ["moved.py"]}, ["syntax"],
                            TaskDelivery(patch="artifacts/resumed.patch"))
        review = client.review(change)
        assert review.at("/ready")
        delivered = client.execute(review)
        assert delivered.passed
        receiver = work / "receiver"
        receiver.mkdir()
        (receiver / "moved.py").write_text(source)
        run(["git", "init", "-q"], receiver)
        run(["git", "apply", "--check", str(project / "artifacts/resumed.patch")], receiver)
        run(["git", "apply", str(project / "artifacts/resumed.patch")], receiver)
        assert (receiver / "moved.py").read_bytes() == (project / "moved.py").read_bytes()
        namespace = {}
        exec(compile((receiver / "moved.py").read_text(), "moved.py", "exec"), namespace)
        behavior = [namespace["café"](n) for n in (-2, 0, 7)]
        assert behavior == [0, 2, 9]
        timing_snapshot = DeclarationSnapshot.capture(FrClient(receiver, executable=str(args.fr.resolve())))
        (work / "timing-snapshot.json").write_text(json.dumps(timing_snapshot.to_data()))
        arms = {}
        for name, mode in (("cold", "warm"), ("warm", "warm"), ("single-edit", "warm"), ("clean-rebuild", "clean")):
            if name == "single-edit":
                (receiver / "moved.py").write_text((receiver / "moved.py").read_text() + "\n# unrelated comment\n")
            arms[name] = json.loads(subprocess.check_output([sys.executable, str(Path(__file__).resolve()), "--fr", str(args.fr.resolve()),
                                                            "--worker", mode, "--work", str(receiver)]))
        return {"schema": "fr-correspondence-acceptance-1", "repository_revision": subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "source_bindings": {path: file_digest(ROOT / path) for path in BINDINGS},
                "binary_sha256": file_digest(args.fr), "platform": platform.platform(), "oracle": oracle, "coordinates": coordinates,
                "ordinary_ast": {"seconds": ordinary_seconds, "source_reveals": 1,
                                 "context_bytes": len(source.encode()) + len(encoded(ordinary)), "tokens": None},
                "baseline": json.loads((FIXTURE / "baseline.json").read_text()), "snapshot": snapshot.to_data(), "snapshot_root": snapshot_root,
                "session": session.to_data(), "session_root": session_root, "reopened": reopened.resumed.report.to_data(),
                "resumed": resumed.resumed.report.to_data(), "refreshed": refreshed.to_data(), "cases": cases,
                "stale_review_refusal": stale_review, "old_review": old_review.to_data(), "fresh_review": review.to_data(),
                "delivery": delivered.to_data(), "patch": (project / "artifacts/resumed.patch").read_text(),
                "receiver_behavior": behavior, "arms": arms,
                "scope": "Finite deterministic Python syntax fixture. Rename means only the declaration name changed. Old plan observations remain agent records. Native mutation review, reversal and independent patch replay cover the fresh edit. Each timing arm recomputes correspondence with the existing index cache; RSS records isolated worker and largest child maxima. No new cache granularity, semantic equivalence, live-agent, token or general performance claim."}


def audit(value):
    assert value["schema"] == "fr-correspondence-acceptance-1"
    assert value["source_bindings"] == {path: file_digest(ROOT / path) for path in BINDINGS}
    assert value["baseline"] == json.loads((FIXTURE / "baseline.json").read_text())
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    assert value["oracle"] == oracle and value["coordinates"] == oracle["declarations"]
    assert value["receiver_behavior"] == [0, 2, 9] and value["stale_review_refusal"]
    assert value["delivery"]["executed"] and value["delivery"]["passed"]
    stages = value["delivery"]["workflow"]["stages"]
    assert {"apply", "undo", "redo", "deliver-patch"} <= {stage["stage"] for stage in stages}
    assert all(stage["status"] == "passed" for stage in stages)
    assert value["old_review"]["task_change_basis"] != value["fresh_review"]["task_change_basis"]
    with tempfile.TemporaryDirectory(prefix="fr-correspondence-audit-") as directory:
        root = Path(directory)
        source = (FIXTURE / "subject.py").read_text()
        (root / "moved.py").write_text(source)
        (root / "change.patch").write_text(value["patch"])
        run(["git", "init", "-q"], root)
        run(["git", "apply", "--check", "change.patch"], root)
        run(["git", "apply", "change.patch"], root)
        assert (root / "moved.py").read_text() == source.replace("value + 1", "value + 2")
    for name in ("snapshot", "session"):
        assert store_merkle_value(MemoryObjectStore(), value[name]).digest == value[name + "_root"]
    snapshot = DeclarationSnapshot.from_data(value["snapshot"])
    expected = {"move": "matched", "rename": "matched", "changed": "matched", "copy-and-change": "ambiguous",
                "duplicates": "ambiguous", "deleted": "missing"}
    assert set(value["cases"]) == set(expected)
    for name, status in expected.items():
        case = value["cases"][name]
        pages = [IdentityPage.from_report(FrReport(p, ())) for p in case["pages"]]
        assert case["complete"] and len(pages) == 1 and pages[0].complete
        match = pages[0].matches[0]
        assert match.previous == snapshot.items[0] and match.status == status
        assert match.candidate_count == (0 if status == "missing" else 2 if status == "ambiguous" else 1)
        if name == "move":
            assert match.candidates[0].object_digest == match.previous.object_digest
        if name == "rename":
            assert match.candidates[0].object_digest != match.previous.object_digest
            assert match.candidates[0].name_erased_digest == match.previous.name_erased_digest
    assert value["reopened"]["plan"]["steps"][0]["state"] == "ready"
    assert value["resumed"]["invalidated"] == ["target", "dependent"] and not value["resumed"]["complete"]
    refreshed = value["refreshed"]["plan"]["steps"]
    assert refreshed[0]["state"] == "ready" and not refreshed[0]["evidence"] and not refreshed[0]["action"]
    assert refreshed[1]["state"] == "stale" and refreshed[2]["state"] == "satisfied"
    for a, b in (("cold", "warm"), ("single-edit", "clean-rebuild")):
        assert value["arms"][a]["report"] == value["arms"][b]["report"]
    for arm in value["arms"].values():
        assert arm["elapsed_seconds"] >= 0 and arm["context_bytes"] > 0 and arm["peak_native_child_rss_bytes"] > 0
        assert arm["correspondence_recomputed"] and arm["tokens"] is None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    parser.add_argument("--worker", choices=["clean", "warm"])
    parser.add_argument("--work", type=Path)
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args)))
        return
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    output = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
