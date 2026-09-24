import copy
import json
from pathlib import Path
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow_facts import FlowFacts
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "tests/agent-eval/flow-facts"


def workspace(tmp_path):
    (tmp_path / "subject.py").write_text((FIXTURE / "subject.py").read_text())
    rules = tmp_path / "rules.json"
    rules.write_text(json.dumps({"version": "facts-1", "sources": ["café"], "sinks": ["sink"],
                                 "sanitizers": {"clean": "html"}}))
    client = FrClient(tmp_path, executable=str(ROOT / "target/debug/fr"), max_output_bytes=1_048_576)
    return client, rules


def inspect(client, rules, name="checkout", **options):
    target = client.project("find", name).definition_target().handle
    return FlowFacts.inspect(client, target, rules=rules, context="html", steps=options.pop("steps", 4096), **options)


def witness(page, client):
    return next(fact for fact in page.collect(client).facts if fact.kind == "witness")


def test_paged_facts_and_explanations_match_independent_unicode_oracle(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, limit=2, evidence_limit=2)
    assert not page.complete
    assert page.report.at("/analysis/complete") is True
    short = page.collect(client, max_pages=1)
    assert not short.complete and short.continuation
    facts = page.collect(client)
    assert facts.complete and facts.pages > 1
    witnesses = [fact for fact in facts.facts if fact.kind == "witness"]
    assert len(witnesses) == 2
    oracle = json.loads(subprocess.check_output([sys.executable, str(FIXTURE / "oracle.py")]))
    sources = []
    for fact in witnesses:
        detail = page.explain(client, fact)
        collection = detail.collect(client)
        assert collection.complete
        for point in collection.evidence:
            assert point.rule["id"].endswith(point.occurrence.role)
            span = point.occurrence.location.span
            if point.occurrence.role in {"source", "sink", "call-result", "summary-call"}:
                assert [span.start, span.end] in oracle["call_spans"]
            if point.occurrence.role == "source":
                sources.append(point.occurrence.id)
                assert point.mapping["status"] == "mapped"
                assert point.mapping["items"][0]["body_pointer"].startswith("/body/")
                linked = point.semantic(client)
                assert linked.for_occurrence(point.occurrence)
    assert len(set(sources)) == 2


def test_normalization_and_parameters_keep_missing_mappings_explicit(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, "normalization_boundary", evidence_limit=64)
    detail = page.explain(client, witness(page, client))
    assert detail.complete
    points = detail.evidence
    assert any(point.mapping["status"] == "absent" for point in points)
    assert any(point.mapping["status"] == "mapped" for point in points)
    assert all(point.mapping["items"] == [] for point in points if point.mapping["status"] == "absent")


def test_model_absence_requires_complete_analysis_and_complete_catalogue(tmp_path):
    client, rules = workspace(tmp_path)
    for name in ["overwritten", "sanitized"]:
        facts = inspect(client, rules, name).collect(client)
        assert facts.complete and not any(fact.kind == "witness" for fact in facts.facts)
    unknown = inspect(client, rules, "unknown")
    assert unknown.report.at("/disclosure/complete") and not unknown.complete
    assert any(item.kind == "boundary" for item in unknown.items)
    exhausted = inspect(client, rules, steps=1)
    assert not exhausted.complete
    assert "step-budget" in exhausted.report.at("/analysis/cutoffs")
    tail = inspect(client, rules, limit=1).next(client).collect(client)
    assert not tail.disclosure_complete and not tail.complete


def test_byte_budget_preserves_continuations_without_silent_omission(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, limit=64, max_bytes=8192)
    assert len(json.dumps(page.report.to_data(), ensure_ascii=False, separators=(",", ":")).encode()) <= 8192
    assert page.report.at("/page/remaining") > 0
    assert page.collect(client).complete


def test_rules_context_and_revisions_bind_fact_and_cursor_identity(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, limit=1)
    fact = witness(page, client)
    root = page.persist(MemoryObjectStore())
    assert len(root) == 64 and not root.startswith("frff1:")
    rules.write_text(rules.read_text().replace("facts-1", "facts-2"))
    with pytest.raises(FrRuntimeError):
        page.next(client)
    with pytest.raises(FrRuntimeError):
        page.explain(client, fact)
    fresh = inspect(client, rules)
    assert fresh.report.at("/basis") != page.report.at("/basis")
    with pytest.raises(FrRuntimeError):
        fresh.explain(client, fact)
    (tmp_path / "other.py").write_text("def unrelated():\n    return 0\n")
    with pytest.raises(FrRuntimeError):
        fresh.explain(client, witness(fresh, client))


def test_fact_store_verifies_objects_and_keeps_stale_actions_stale(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, limit=1)
    store = MemoryObjectStore()
    root = page.persist(store)
    restored = FlowFacts.restore(store, root)
    assert restored.report.to_data() == page.report.to_data()
    assert restored.next(client).items

    class Corrupt:
        def get(self, key):
            return {"kind": "scalar", "value": "forged"} if key == root else store.get(key)
        def put(self, key, value):
            store.put(key, value)

    with pytest.raises((FrRuntimeError, ValueError)):
        FlowFacts.restore(Corrupt(), root)
    (tmp_path / "subject.py").rename(tmp_path / "moved.py")
    with pytest.raises(FrRuntimeError):
        restored.next(client)


@pytest.mark.parametrize("change", ["fact", "coverage", "analysis", "input", "occurrence", "index", "mapping", "pointer", "digest"])
def test_tampered_facts_and_explanations_refuse(tmp_path, change):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, evidence_limit=64)
    detail = page.explain(client, witness(page, client))
    value = copy.deepcopy(detail.report.to_data())
    if change == "fact":
        value["fact"]["core"]["confidence"] = "proved"
    elif change == "coverage":
        value["page"]["total"] += 1
    elif change == "analysis":
        value["analysis"]["complete"] = False
    elif change == "input":
        value["analysis"]["inputs"]["context"] = "other"
    elif change == "occurrence":
        value["evidence"][0]["occurrence"]["location"]["span"]["end"] += 1
    elif change == "index":
        value["evidence"][0]["index"] = 99
    elif change == "mapping":
        value["evidence"][0]["mapping"]["status"] = "absent"
    elif change == "pointer":
        value["evidence"][0]["mapping"]["items"][0]["body_pointer"] = "/body/999"
    else:
        value["evidence"].reverse()
        for index, point in enumerate(value["evidence"]):
            point["index"] = index
    with pytest.raises(FrRuntimeError):
        FlowFacts.from_report(FrReport(value, ()))


def test_follow_rejects_mutating_routes_before_execution(tmp_path):
    client, rules = workspace(tmp_path)
    page = inspect(client, rules, limit=1)
    value = page.report.to_data()
    value["continuation"] = {"arguments": ["history", "apply", "bogus", "--write"]}
    altered = FlowFacts.from_report(FrReport(value, ()))
    with pytest.raises(FrRuntimeError, match="read-only"):
        altered.next(client)


def test_discovery_offers_source_free_analysis_for_top_level_python(tmp_path):
    client, _ = workspace(tmp_path)
    page = client.project("explore", "checkout")
    action = page.at("/rows/0/analysis/arguments")
    assert action[:2] == ["project", "flow-facts"]
    report = FlowFacts.from_report(client.call(*action))
    assert not report.complete
    assert report.report.at("/analysis/cutoffs")
