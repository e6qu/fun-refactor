#!/usr/bin/env python3
"""Retain finite provenance and checked-plan comparisons with independent coordinates."""
from __future__ import annotations

import argparse
import ast
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
from fr_ir.investigation import TaskPlan, TaskStep
from fr_ir.investigation_checks import run_checks, restore_checks
from fr_ir.origins import SemanticOrigins
from fr_ir.runtime import FrClient, FrReport

FIXTURE = ROOT / "tests/agent-eval/semantic-evidence"
BINDINGS = ["tools/semantic-evidence-acceptance.py", "tools/evidence_basis.py", "src/project/semantic_origins.rs",
            "src/project/semantic.rs", "src/project/occurrence.rs", "src/project/investigation.rs", "src/project.rs",
            "src/checks.rs", "src/checks/evidence.rs", "src/history.rs", "src/transpile/read.rs", "src/transpile/normalize.rs",
            "src/transpile/mod.rs", "src/span.rs", "Cargo.lock", "sdk/python/src/fr_ir/origins.py",
            "sdk/python/src/fr_ir/investigation.py", "sdk/python/src/fr_ir/investigation_checks.py",
            "sdk/python/src/fr_ir/runtime.py", "sdk/python/src/fr_ir/context.py", "kernels/FrKernels/Investigation.lean",
            "kernels/InvestigationMain.lean"] + [f"tests/agent-eval/semantic-evidence/{name}" for name in
                                                 ("task.json", "subject.py", "oracle.py", "diagnostics.json")]


def encoded(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def measure(binary):
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    source = (FIXTURE / "subject.py").read_text()
    started = time.perf_counter()
    tree = ast.parse(source)
    ast_calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)]
    ordinary = {"seconds": time.perf_counter() - started, "context_bytes": len(source.encode()) + len(encoded(oracle)),
                "source_reveals": 1, "useful_discoveries": len(ast_calls), "tokens": None}
    with tempfile.TemporaryDirectory(prefix="fr-semantic-evidence-") as temporary:
        project = Path(temporary) / "project"
        project.mkdir()
        for name in ("subject.py", "oracle.py"):
            shutil.copyfile(FIXTURE / name, project / name)
        (project / ".fr").mkdir()
        config = {"schema": 1, "checks": [{"name": "oracle", "argv": [sys.executable, "oracle.py"], "cwd": ".",
                  "timeout_seconds": 10, "covers": ["finite Python execution and AST coordinates"]}]}
        (project / ".fr/checks.json").write_text(json.dumps(config))
        client = FrClient(project, executable=str(binary.resolve()))
        target = client.project("find", "total").definition_target().handle
        started = time.perf_counter()
        previous = client.project("semantic", target, "--body", "--pointers")
        reveal = client.project("show", target, "--source", "--bytes", "2048")
        old_route = {"seconds": time.perf_counter() - started,
                     "context_bytes": len(encoded(previous.to_data())) + len(encoded(reveal.to_data())),
                     "source_reveals": 1, "process_calls": 2, "tokens": None}
        started = time.perf_counter()
        origins = SemanticOrigins.inspect(client, target, limit=64)
        new_route = {"seconds": time.perf_counter() - started, "context_bytes": len(encoded(origins.report.to_data())),
                     "source_reveals": 0, "process_calls": 1, "tokens": None}
        calls = [item for item in origins.items if item.kind == "call"]
        coordinates = [[item.origins.occurrences[0].location.span.start,
                        item.origins.occurrences[0].location.span.end] for item in calls]
        assert coordinates == oracle["calls"]
        store = MemoryObjectStore()
        plan = TaskPlan("retain exact checked evidence", ("oracle passes",), (
            TaskStep.checked("verify", "does the independent oracle pass?", checks=("oracle",), satisfies=("oracle passes",)),))
        checked = run_checks(plan, client, "verify", client.call("checks", "--toolchain"), store)
        assert checked.passed and checked.resumed.complete
        assert restore_checks(store, checked.report_root).to_data() == checked.checks.to_data()
        saved_plan = checked.resumed.plan.store(store)
        assert TaskPlan.restore(store, saved_plan).resume(client).complete
        (project / "subject.py").write_text(source + "\n# changed source\n")
        stale = TaskPlan.restore(store, saved_plan).resume(client)
        assert stale.invalidated == ("verify",) and not stale.complete
        config["checks"][0]["argv"] = [sys.executable, "-c", "raise RuntimeError('independent failure')"]
        (project / ".fr/checks.json").write_text(json.dumps(config))
        failed = run_checks(plan, client, "verify", client.call("checks", "--toolchain"), store)
        assert not failed.passed and not failed.resumed.complete and failed.attachment_error is None
        return {"schema": "fr-semantic-evidence-acceptance-1", "repository_revision": subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "source_bindings": {name: file_digest(ROOT / name) for name in BINDINGS},
                "binary_sha256": file_digest(binary), "platform": platform.platform(), "python": platform.python_version(),
                "ordinary_ast": ordinary, "prior_source_reveal_route": old_route, "origin_route": new_route,
                "oracle": oracle, "coordinates": coordinates, "false_coordinate_claims": 0,
                "origin_report": origins.report.to_data(), "checks": checked.checks.to_data(), "check_root": checked.report_root,
                "completed_plan": checked.resumed.plan.to_data(), "plan_root": saved_plan,
                "stale": stale.report.to_data(), "failed_checks": failed.checks.to_data(), "failed_root": failed.report_root,
                "failed_plan": failed.resumed.plan.to_data(),
                "scope": "One deterministic fixture. Ordinary AST timing excludes process startup. The two fr routes share discovery and use the current binary; the prior route is a workflow baseline, not a historical binary comparison. Context counts include raw reports, not live model token use. No latency, compiler semantics, security or population claim."}


def audit(value):
    assert value["schema"] == "fr-semantic-evidence-acceptance-1"
    assert value["source_bindings"] == {name: file_digest(ROOT / name) for name in BINDINGS}
    origins = SemanticOrigins.from_report(FrReport(value["origin_report"], ()))
    coordinates = [[item.origins.occurrences[0].location.span.start, item.origins.occurrences[0].location.span.end]
                   for item in origins.items if item.kind == "call"]
    assert coordinates == value["coordinates"] == value["oracle"]["calls"]
    assert value["oracle"]["outcomes"] == [1, 5, -1] and value["false_coordinate_claims"] == 0
    for report, root in [("checks", "check_root"), ("failed_checks", "failed_root"), ("completed_plan", "plan_root")]:
        assert store_merkle_value(MemoryObjectStore(), value[report]).digest == value[root]
    assert value["checks"]["passed"] and not value["failed_checks"]["passed"]
    assert value["completed_plan"]["steps"][0]["state"] == "satisfied"
    assert value["stale"]["invalidated"] == ["verify"] and not value["stale"]["complete"]
    assert value["failed_plan"]["steps"][0]["evidence"][0]["passed"] is False
    for name in ("ordinary_ast", "prior_source_reveal_route", "origin_route"):
        assert value[name]["seconds"] >= 0 and value[name]["context_bytes"] > 0 and value[name]["tokens"] is None
    assert value["origin_route"]["source_reveals"] == 0 and value["prior_source_reveal_route"]["source_reveals"] == 1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args.fr)
    audit(value)
    output = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
