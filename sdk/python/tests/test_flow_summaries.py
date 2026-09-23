import json
from pathlib import Path

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]


def workspace(tmp_path):
    (tmp_path / "subject.py").write_text((ROOT / "tests/agent-eval/recursive-flow/subject.py").read_text())
    rules = tmp_path / "rules.json"
    rules.write_text(json.dumps({"version": "recursive-1", "sources": ["source"],
                                 "sinks": ["sink"], "sanitizers": {"clean": "html"}}))
    return FrClient(tmp_path, executable=str(ROOT / "target/debug/fr"), max_output_bytes=1_048_576), rules


def analyze(client, rules, name="positive", cache=None, **options):
    handle = client.project("find", name).definition_target().handle
    return (cache or FlowCache(MemoryObjectStore())).analyze(
        client, handle, rules=rules, steps=options.pop("steps", 4096),
        max_bytes=options.pop("max_bytes", 1_048_576), summaries=True,
        context=options.pop("context", "html"), **options)


def evidence(result):
    value = result.report.to_data()
    value.pop("execution")
    value.pop("context_basis", None)
    return value


def test_typed_summaries_expose_parameter_transfer_and_effects(tmp_path):
    client, rules = workspace(tmp_path)
    report = analyze(client, rules)
    summaries = report.summaries
    assert summaries.complete and summaries.converged and summaries.rounds > 1
    relay = summaries.for_function("relay")
    assert relay.return_parameters == (0,)
    assert relay.callees == ("relay",)
    assert relay.normal_return and not relay.may_raise
    assert len(report.witnesses) == 1
    assert analyze(client, rules, "negative").summaries.for_function("erase").return_parameters == ()
    assert not analyze(client, rules, "unreachable").summaries.for_function("forever").normal_return
    raised = analyze(client, rules, "exceptional").summaries.for_function("throw")
    assert raised.may_raise and not raised.normal_return and raised.exceptional_returns
    assert analyze(client, rules, "effects").summaries.for_function("emit").sinks


def test_context_contracts_and_callers_remain_separate(tmp_path):
    client, rules = workspace(tmp_path)
    assert not analyze(client, rules, "sanitized").witnesses
    assert len(analyze(client, rules, "sanitized", context="sql").witnesses) == 1
    assert not analyze(client, rules, "separate").witnesses


def test_summary_cache_rebinds_every_nested_occurrence_and_matches_clean(tmp_path):
    client, rules = workspace(tmp_path)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    first = analyze(client, rules, "effects", cache)
    cache = FlowCache.restore(store, cache.persist())
    assert evidence(analyze(client, rules, "effects", cache)) == evidence(first)
    for number in range(3):
        (tmp_path / "unrelated.py").write_text(f"def other():\n    return {number}\n")
        reused = analyze(client, rules, "effects", cache)
        clean = analyze(client, rules, "effects")
        assert reused.reused and evidence(reused) == evidence(clean)
        assert reused.summaries.complete
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("sink(value)", "sink(0)"))
    fresh = analyze(client, rules, "effects", cache)
    assert not fresh.reused and not fresh.witnesses
    assert evidence(fresh) == evidence(analyze(client, rules, "effects"))


@pytest.mark.parametrize("kind", ["revision", "parameter", "origin", "convergence", "callee", "boolean"])
def test_tampered_summary_contracts_refuse(tmp_path, kind):
    client, rules = workspace(tmp_path)
    value = analyze(client, rules).report.to_data()
    table = value["function_summaries"]
    relay = table["functions"]["relay"]
    if kind == "revision":
        relay["returns"]["argument:relay:0"]["occurrences"][0]["revision"] = "0" * 64
    elif kind == "parameter":
        relay["parameters"][0] = "argument:other:0"
    elif kind == "origin":
        relay["returns"]["argument:relay:0"]["origin"] = "invented"
    elif kind == "convergence":
        table["converged"] = False
    elif kind == "callee":
        relay["callees"] = ["missing"]
    else:
        relay["normal_return"] = "true"
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(value, ()))


def test_exhausted_solver_and_clipped_reports_cannot_establish_absence(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        result = analyze(client, rules, cache=cache, steps=1)
        assert not result.reused and not result.summaries.complete
        assert not result.summaries.converged
    result = analyze(client, rules, max_bytes=4096)
    assert result.report.at("/complete") is False
    with pytest.raises(FrRuntimeError):
        result.summaries


def test_modes_do_not_share_cache_entries(tmp_path):
    client, rules = workspace(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    fresh = analyze(client, rules, cache=cache)
    handle = client.project("find", "positive").definition_target().handle
    old = cache.analyze(client, handle, rules=rules, steps=4096, max_bytes=1_048_576, context="html")
    assert not old.reused and old.report.at("/complete") is False
    assert old.report.at("/input_digest") != fresh.report.at("/input_digest")
