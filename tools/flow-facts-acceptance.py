#!/usr/bin/env python3
"""Retain bounded flow explanations against independent runtime and coordinate oracles."""
from __future__ import annotations

import argparse
import copy
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
from fr_ir.flow_facts import FlowFacts
from fr_ir.origins import SemanticOrigins
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

FIXTURE = ROOT / "tests/agent-eval/flow-facts"
BINDINGS = ["src/project/flow_modules.rs", "sdk/python/src/fr_ir/flow_dependencies.py", "tools/flow-facts-acceptance.py", "tools/evidence_basis.py", "Cargo.lock",
    *[f"src/project/{name}.rs" for name in ["flow_facts", "flow_fact_origins", "dataflow",
        "flow_summaries", "control_flow", "flow_cache", "occurrence", "semantic_origins", "semantic", "explore"]],
    "src/project.rs", "src/span.rs", "src/transpile/read.rs", "src/transpile/normalize.rs",
    *[f"sdk/python/src/fr_ir/{name}.py" for name in ["flow_facts", "origins", "runtime", "context"]],
    "kernels/FrKernels/Flow.lean", "kernels/InvestigationMain.lean",
    *[f"tests/agent-eval/flow-facts/{name}" for name in ["subject.py", "oracle.py", "task.json", "baseline.json"]]]


def encode(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(encode(value)).hexdigest()


def oracle():
    return json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))


def refused(action):
    try:
        action()
    except FrRuntimeError:
        return True
    return False


def measure(args):
    start = time.perf_counter()
    independent = oracle()
    ordinary_seconds = time.perf_counter() - start
    with tempfile.TemporaryDirectory(prefix="fr-fact-eval-") as temporary:
        work = Path(temporary)
        project = work / "project"
        project.mkdir()
        shutil.copyfile(FIXTURE / "subject.py", project / "subject.py")
        rules = work / "rules.json"
        rules.write_text(json.dumps({"version": "facts-1", "sources": ["café"], "sinks": ["sink"],
                                     "sanitizers": {"clean": "html"}}))
        client = FrClient(project, executable=str(args.fr.resolve()), max_output_bytes=1_048_576)
        target = client.project("find", "checkout").definition_target().handle
        start = time.perf_counter()
        raw = client.project("dataflow", target, "--summaries", "--rules", str(rules), "--context", "html",
                             "--steps", "4096", "--bytes", "1048576").to_data()
        raw_seconds = time.perf_counter() - start
        start = time.perf_counter()
        first = FlowFacts.inspect(client, target, rules=rules, context="html", steps=4096, limit=2, evidence_limit=2)
        first_seconds = time.perf_counter() - start
        pages, current = [], first
        while True:
            pages.append(current.report.to_data())
            if current.report.at("/continuation") is None:
                break
            current = current.next(client)
        witnesses = [row for page in pages for row in page["items"] if row["core"]["kind"] == "witness"]
        explanations = []
        semantic_links = []
        start = time.perf_counter()
        for row in witnesses:
            fact = next(item for page in pages for item in FlowFacts.from_report(FrReport(page, ())).items if item.id == row["id"])
            detail, retained = first.explain(client, fact), []
            while True:
                retained.append(detail.report.to_data())
                for point in detail.evidence:
                    for index in range(len(point.mapping["items"])):
                        linked = point.semantic(client, index)
                        semantic_links.append({"occurrence": point.raw_occurrence, "link": point.mapping["items"][index],
                                               "report": linked.report.to_data()})
                if detail.report.at("/continuation") is None:
                    break
                detail = detail.next(client)
            explanations.append(retained)
        explanation_seconds = time.perf_counter() - start
        cases = {}
        for name in ["overwritten", "sanitized", "unknown", "normalization_boundary"]:
            handle = client.project("find", name).definition_target().handle
            page = FlowFacts.inspect(client, handle, rules=rules, context="html", steps=4096, evidence_limit=64)
            cases[name] = {"report": page.report.to_data(), "details": [page.explain(client, item).report.to_data()
                for item in page.items if item.kind == "witness"]}
        exhausted = FlowFacts.inspect(client, target, rules=rules, context="html", steps=1).report.to_data()
        altered = copy.deepcopy(pages[0])
        altered["items"][0]["core"]["confidence"] = "proved"
        refusals = {"tampered_fact": refused(lambda: FlowFacts.from_report(FrReport(altered, ())))}
        rules.write_text(rules.read_text().replace("facts-1", "facts-2"))
        refusals["changed_rules"] = refused(lambda: first.next(client))
        (project / "subject.py").rename(project / "moved.py")
        refusals["moved_source"] = refused(lambda: first.next(client))
    return {"schema": "fr-flow-facts-acceptance-1", "repository_revision": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_bindings": {path: file_digest(ROOT / path) for path in BINDINGS},
        "binary_sha256": hashlib.sha256(args.fr.read_bytes()).hexdigest(), "platform": platform.platform(),
        "oracle": independent, "ordinary_ast_runtime_seconds": ordinary_seconds,
        "baseline": json.loads((FIXTURE / "baseline.json").read_text()),
        "raw": {"response_bytes": len(encode(raw)), "elapsed_seconds": raw_seconds,
                "witnesses": len(raw["witnesses"]), "complete": raw["complete"], "process_calls": 1},
        "facts": {"pages": pages, "first_page_bytes": len(encode(pages[0])), "first_page_seconds": first_seconds,
                  "catalogue_bytes": sum(len(encode(page)) for page in pages), "process_calls": len(pages)},
        "explanations": explanations, "explanation_seconds": explanation_seconds,
        "explanation_bytes": sum(len(encode(page)) for group in explanations for page in group),
        "explanation_process_calls": sum(map(len, explanations)), "validated_semantic_links": len(semantic_links), "semantic_links": semantic_links,
        "useful_discoveries": {"witnesses": len(witnesses), "distinct_source_sites": 2},
        "diagnostics": ["Initial smoke omitted project context headers; corrected before acceptance.",
                        "SDK validation exposed native structured-span ordering and trace canonicalization; corrected before acceptance.",
                        "The initial large-origin fixture had only 206 rows; expanded it beyond the 256-row limit."],
        "cases": cases, "exhausted": exhausted, "refusals": refusals,
        "evidence_digest": digest([pages, explanations, cases, exhausted]),
        "source_reveals": 0, "tokens": None, "false_claims": 0,
        "scope": "Deterministic nine-case runtime fixture and independent UTF-8 AST coordinates. Fact pages reduce initial disclosure; full explanations add provenance and can cost more than raw analysis. Each semantic link was separately followed and checked. No feasible-path, security, source-correspondence proof, live-agent or general speed claim."}


def audit(value):
    assert value["schema"] == "fr-flow-facts-acceptance-1"
    assert value["source_bindings"] == {path: file_digest(ROOT / path) for path in BINDINGS}
    assert value["oracle"] == oracle()
    assert value["baseline"] == json.loads((FIXTURE / "baseline.json").read_text())
    pages, explanations, cases = value["facts"]["pages"], value["explanations"], value["cases"]
    assert value["evidence_digest"] == digest([pages, explanations, cases, value["exhausted"]])
    assert value["raw"]["complete"] and value["raw"]["witnesses"] == len(explanations) == 2
    assert value["facts"]["first_page_bytes"] == len(encode(pages[0])) < value["raw"]["response_bytes"]
    offset, ids = 0, set()
    for page in pages:
        parsed = FlowFacts.from_report(FrReport(page, ()))
        assert page["page"]["before"] == offset and page["analysis"]["complete"]
        offset += len(parsed.items)
        for item in parsed.items:
            assert item.id not in ids
            ids.add(item.id)
    assert pages[-1]["page"]["remaining"] == 0
    sources = set()
    for group in explanations:
        points, offset = [], 0
        for page in group:
            detail = FlowFacts.from_report(FrReport(page, ()))
            assert detail.fact.id in ids and page["page"]["before"] == offset
            offset += len(detail.evidence)
            points.extend(point.raw_occurrence for point in detail.evidence)
            for point in detail.evidence:
                span = [point.occurrence.location.span.start, point.occurrence.location.span.end]
                if point.occurrence.role in {"source", "sink", "call-result", "summary-call"}:
                    assert span in value["oracle"]["call_spans"]
                if point.occurrence.role == "source":
                    sources.add(tuple(span))
        assert group[-1]["page"]["remaining"] == 0 and digest(points) == detail.fact.core["evidence_digest"]
    assert len(sources) == 2 and value["validated_semantic_links"] == len(value["semantic_links"]) > 0
    from fr_ir.runtime import Occurrence
    for entry in value["semantic_links"]:
        semantic = SemanticOrigins.from_report(FrReport(entry["report"], ()))
        point = Occurrence.from_data(entry["occurrence"])
        assert semantic.basis == entry["link"]["semantic_basis"]
        assert any(item.id == entry["link"]["id"] for item in semantic.for_occurrence(point))
    for name in ["overwritten", "sanitized"]:
        page = FlowFacts.from_report(FrReport(cases[name]["report"], ()))
        assert page.complete and not any(item.kind == "witness" for item in page.items)
    unknown = FlowFacts.from_report(FrReport(cases["unknown"]["report"], ()))
    assert not unknown.complete and any(item.kind == "boundary" for item in unknown.items)
    normalized = cases["normalization_boundary"]["details"][0]
    detail = FlowFacts.from_report(FrReport(normalized, ()))
    assert any(point.mapping["status"] == "absent" for point in detail.evidence)
    assert not FlowFacts.from_report(FrReport(value["exhausted"], ())).complete
    assert all(value["refusals"].values()) and value["false_claims"] == value["source_reveals"] == 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--fr", type=Path, default=ROOT / "target/debug/fr")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--audit", type=Path)
    args = parser.parse_args()
    value = json.loads(args.audit.read_text()) if args.audit else measure(args)
    audit(value)
    text = json.dumps(value, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
