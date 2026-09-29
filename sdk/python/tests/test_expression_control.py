"""Ordered scalar expressions checked against CPython and clean cache rebuilds."""
import ast
import copy
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace


PRELUDE = "def stop(value):\n    raise value\ndef forever():\n    return forever()\n"


def install(tmp_path, expression, tail=""):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(PRELUDE + "# café 🦀\ndef entry(flag):\n    "
        + ("result = " + expression + "\n    " + tail if tail else "return " + expression) + "\n")
    return client, rules


def observe(root, flag):
    script = """
import json, subject, sys
seen = []
def source():
    seen.append(["source", 17]); return 17
def sink(value):
    seen.append(["sink", value]); return value
subject.source, subject.sink, subject.clean = source, sink, lambda value: 0
try:
    result = subject.entry(sys.argv[1] == 'true')
    outcome = {"returned": result, "raised": None}
except Exception as error:
    outcome = {"returned": None, "raised": type(error).__name__}
print(json.dumps({**outcome, "events": seen}))
"""
    return json.loads(subprocess.check_output([sys.executable, "-B", "-c", script,
        "true" if flag else "false"], cwd=root, text=True))


@pytest.mark.parametrize("expression,reachable", [
    ("False and sink(source())", False),
    ("True or sink(source())", False),
    ("None and sink(source())", False),
    ("(not True) and sink(source())", False),
    ("not False or sink(source())", False),
    ("(False and source()) and sink(source())", False),
    ("(True or source()) or sink(source())", False),
    ("sink(source()) if True else stop(source())", True),
    ("stop(source()) if False else sink(source())", True),
    ("0 if True else sink(source())", False),
    ("sink(source()) if False else 0", False),
    ("(True if flag else True) or sink(source())", False),
    ("(False if flag else False) and sink(source())", False),
    ("(not (False or True)) and sink(source())", False),
    ("True and sink(source())", True),
    ("False or sink(source())", True),
    ("None or sink(source())", True),
    ("flag and sink(source())", True),
    ("flag or sink(source())", True),
    ("sink(source()) if flag else 0", True),
    ("0 if flag else sink(source())", True),
    ("sink(clean(source())) if flag else 0", False),
    ("sink(source()) < 20", True),
    ("0 < sink(source()) < 20", True),
    ("0 < 1 < sink(source())", True),
    ("sink(source()) if (True and not False) else 0", True),
    ("(False and stop(source())) or sink(source())", True),
])
def test_runtime_effects_and_exact_origins(tmp_path, expression, reachable):
    client, rules = install(tmp_path, expression)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == reachable
    runtime = [observe(tmp_path, flag) for flag in (False, True)]
    assert any(["sink", 17] in item["events"] for item in runtime) == reachable
    assert all(item["raised"] is None for item in runtime)
    assert result.summaries.expression_control.path_feasibility is False
    source = (tmp_path / "subject.py").read_text()
    lines = source.splitlines(keepends=True)
    offsets = [0]
    for line in lines: offsets.append(offsets[-1] + len(line.encode()))
    calls = {(offsets[n.lineno-1] + n.col_offset, offsets[n.end_lineno-1] + n.end_col_offset)
             for n in ast.walk(ast.parse(source)) if isinstance(n, ast.Call)}
    for witness in result.witnesses:
        for point in witness.occurrences:
            if point.role in {"source", "sink", "summary-call", "call-result"}:
                assert (point.location.span.start, point.location.span.end) in calls


@pytest.mark.parametrize("expression,normal,raises,reaches", [
    ("flag and stop(source())", True, True, True),
    ("flag or stop(source())", True, True, True),
    ("True and stop(source())", False, True, False),
    ("False or stop(source())", False, True, False),
    ("stop(source()) and sink(source())", False, True, False),
    ("stop(source()) or sink(source())", False, True, False),
    ("source() if stop(source()) else 0", False, True, False),
    ("stop(source()) if flag else 0", True, True, True),
    ("0 if flag else stop(source())", True, True, True),
    ("stop(source()) if flag else stop(source())", False, True, False),
    ("False and stop(source())", True, False, True),
    ("True or stop(source())", True, False, True),
    ("stop(source()) < source()", False, True, False),
    ("source() < stop(source()) < sink(source())", False, True, False),
    ("source() < 0 < stop(source()) < sink(source())", True, True, True),
])
def test_normal_and_exceptional_paths_join_without_losing_continuations(tmp_path, expression, normal, raises, reaches):
    client, rules = install(tmp_path, expression, "return sink(source())")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    summary = result.summaries.for_function("entry")
    assert summary.normal_return == normal and summary.may_raise == raises
    assert bool(result.witnesses) == reaches
    runtime = [observe(tmp_path, flag) for flag in (False, True)]
    # Numeric comparisons remain unknown in the model, so an exceptional branch can be conservative.
    assert not any(item["raised"] for item in runtime) or raises
    assert not any(item["raised"] is None for item in runtime) or normal
    assert not any(["sink", 17] in item["events"] for item in runtime) or reaches


@pytest.mark.parametrize("expression", ["flag or forever()", "flag and forever()",
    "0 if flag else forever()", "forever() if flag else 0"])
def test_recursive_no_return_alternative_does_not_erase_normal_path(tmp_path, expression):
    client, rules = install(tmp_path, expression, "return sink(source())")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") and result.witnesses
    assert result.summaries.for_function("entry").normal_return
    assert not result.summaries.for_function("forever").normal_return


@pytest.mark.parametrize("expression", ["False and missing()", "True or missing()",
    "0 if True else missing()", "missing() if False else 0", "False and (lambda: 1)"])
def test_skipped_unsupported_operands_need_no_contract(tmp_path, expression):
    client, rules = install(tmp_path, expression)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert not result.witnesses and not result.summaries.for_function("entry").callees


@pytest.mark.parametrize("expression", ["flag and missing()", "False or missing()",
    "missing() if flag else 0", "True and (lambda: 1)"])
def test_evaluated_unknown_operands_remain_incomplete(tmp_path, expression):
    client, rules = install(tmp_path, expression)
    assert not analyze(client, rules, "entry").report.at("/complete")


def test_middle_comparison_operand_is_transferred_once_per_evaluation(tmp_path):
    client, rules = install(tmp_path, "source() < sink(source()) < source()")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete")
    raw = (tmp_path / "subject.py").read_bytes()
    calls = [event["occurrence"]["location"]["span"]["start"]
             for event in result.report.at("/events") if event["kind"] == "use"
             and raw[event["occurrence"]["location"]["span"]["start"]:
                     event["occurrence"]["location"]["span"]["end"]] in {b"source()", b"sink(source())"}]
    from collections import Counter
    assert len(Counter(calls)) == 4
    assert set(Counter(calls).values()) == {result.summaries.for_function("entry").evaluations}
    assert observe(tmp_path, False)["events"] == [["source", 17], ["source", 17], ["sink", 17]]


@pytest.mark.parametrize("mutation", ["selector", "branch", "helper", "unrelated", "rules"])
def test_cached_and_clean_results_agree_after_expression_changes(tmp_path, mutation):
    client, rules = install(tmp_path, "True and sink(source())")
    store = MemoryObjectStore(); cache = FlowCache(store)
    first = analyze(client, rules, "entry", cache)
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, "entry", cache).reused
    path = tmp_path / "subject.py"
    if mutation == "selector": path.write_text(path.read_text().replace("True and", "False and"))
    elif mutation == "branch": path.write_text(path.read_text().replace("sink(source())", "sink(0)"))
    elif mutation == "helper": path.write_text(path.read_text().replace("raise value", "return value"))
    elif mutation == "rules": rules.write_text(rules.read_text().replace("recursive-1", "changed-2"))
    else: (tmp_path / "unrelated.py").write_text("def other():\n    return 4\n")
    fresh = analyze(client, rules, "entry", cache)
    assert fresh.reused == (mutation == "unrelated")
    assert evidence(fresh) == evidence(analyze(client, rules, "entry"))
    assert (fresh.report.at("/input_digest") == first.report.at("/input_digest")) == (mutation == "unrelated")


@pytest.mark.parametrize("mutation", ["missing", "selectors", "comparison", "boolean", "integer", "inputs", "input-integer", "semantics"])
def test_typed_expression_contract_refuses_tampering(tmp_path, mutation):
    client, rules = install(tmp_path, "flag and sink(source())")
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    if mutation == "missing": del data["expression_control"]
    elif mutation == "semantics": data["semantics"] = "python-scalar-summaries-1"
    elif mutation == "input-integer": data["inputs"]["expression_control"]["path_feasibility"] = 0
    elif mutation == "inputs": data["inputs"]["expression_control"] = None
    elif mutation == "boolean": data["expression_control"]["path_feasibility"] = True
    elif mutation == "integer": data["expression_control"]["path_feasibility"] = 0
    else: data["expression_control"]["comparisons" if mutation == "comparison" else "selectors"] = "invented"
    with pytest.raises(FrRuntimeError): FunctionSummaries.from_report(FrReport(data, ()))


def test_legacy_summaries_stay_readable_and_budgets_stay_incomplete(tmp_path):
    client, rules = install(tmp_path, "flag and sink(source())")
    result = analyze(client, rules, "entry")
    legacy = result.report.to_data(); legacy["semantics"] = "python-scalar-summaries-1"
    legacy.pop("expression_control"); legacy["inputs"].pop("expression_control")
    legacy.pop("call_binding"); legacy["inputs"].pop("call_binding")
    assert FunctionSummaries.from_report(FrReport(legacy, ())).expression_control is None
    cache = FlowCache(MemoryObjectStore())
    for _ in range(2):
        low = analyze(client, rules, "entry", cache, steps=1)
        assert not low.reused and not low.report.at("/complete")
