"""Required-parameter binding checked against CPython and retained summaries."""
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


def install(tmp_path, parameters, call, body="return left", entry_parameters="", extra=""):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        f"def choose({parameters}):\n    {body}\n\n{extra}"
        f"def entry({entry_parameters}):\n    return {call}\n")
    return client, rules


def observe(tmp_path, invocation="entry()"):
    script = '''import json, runpy, sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]).parent))
observed = []
def source():
    observed.append(["source", 17]); return 17
def sink(value):
    observed.append(["sink", value]); return value
def clean(value): return 0
namespace = runpy.run_path(sys.argv[1], init_globals=dict(source=source, sink=sink, clean=clean))
try:
    result = eval(sys.argv[2], namespace)
    raised = None
except Exception as error:
    result = None
    raised = type(error).__name__
print(json.dumps(dict(result=result, raised=raised, events=observed)))
'''
    output = subprocess.run([sys.executable, "-B", "-c", script,
                             str(tmp_path / "subject.py"), invocation], check=True,
                            capture_output=True, text=True)
    return json.loads(output.stdout)


CASES = [
    ("left, right", "choose(source(), 0)", True),
    ("left, right", "choose(right=0, left=source())", True),
    ("left, right", "choose(left=0, right=source())", False),
    ("left, right", "choose(source(), right=0)", True),
    ("left, /, right", "choose(source(), right=0)", True),
    ("left, /, right", "choose(0, source())", False),
    ("left, *, right", "choose(source(), right=0)", True),
    ("left, *, right", "choose(right=source(), left=0)", False),
    ("*, left, right", "choose(right=0, left=source())", True),
    ("*, left, right", "choose(left=0, right=source())", False),
    ("left, /, *, right", "choose(source(), right=0)", True),
    ("left, /, *, right", "choose(0, right=source())", False),
    ("left, right, /", "choose(source(), 0)", True),
    ("left, right, /", "choose(0, source())", False),
    ("left, # parameter comment\n    *, right", "choose(left=source(), # argument comment\n        right=0)", True),
    ("left, right", "choose(left=clean(source()), right=source())", False),
]


@pytest.mark.parametrize("parameters,call,tainted", CASES)
@pytest.mark.parametrize("body", ["return left", "sink(left)\n    return left"])
def test_binding_matches_cpython_and_exact_parameter_origins(tmp_path, parameters, call, tainted, body):
    client, rules = install(tmp_path, parameters, f"sink({call})", body)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == tainted
    assert result.summaries.for_function("choose").return_parameters == (0,)
    assert result.summaries.call_binding.implicit_exceptions is False
    runtime = observe(tmp_path)
    assert runtime["raised"] is None and runtime["result"] == (17 if tainted else 0)
    assert any(event == ["sink", 17] for event in runtime["events"]) == tainted
    source = (tmp_path / "subject.py").read_text()
    lines = source.splitlines(keepends=True); offsets = [0]
    for line in lines: offsets.append(offsets[-1] + len(line.encode()))
    calls = {(offsets[n.lineno-1]+n.col_offset, offsets[n.end_lineno-1]+n.end_col_offset)
             for n in ast.walk(ast.parse(source)) if isinstance(n, ast.Call)}
    for witness in result.witnesses:
        for point in witness.occurrences:
            if point.role in {"source", "sink", "summary-call", "call-result"}:
                assert (point.location.span.start, point.location.span.end) in calls


@pytest.mark.parametrize("parameters,call", [
    ("left, right", "choose(source())"),
    ("left, right", "choose(source(), 0, 1)"),
    ("left, right", "choose(source(), left=0, right=0)"),
    ("left, right", "choose(left=source(), unknown=0)"),
    ("left, /, right", "choose(left=source(), right=0)"),
    ("left, *, right", "choose(source(), 0)"),
    ("*, left, right", "choose(source(), right=0)"),
    ("left, /, *, right", "choose(source())"),
])
def test_invalid_binding_is_incomplete_after_argument_effects(tmp_path, parameters, call):
    call = call.replace("source()", "sink(source())")
    client, rules = install(tmp_path, parameters, call)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "invalid-call-binding:choose" in result.report.at("/cutoffs")
    assert result.witnesses
    runtime = observe(tmp_path)
    assert runtime["raised"] == "TypeError" and runtime["events"] == [["source", 17], ["sink", 17]]
    assert not result.summaries.for_function("entry").callees


@pytest.mark.parametrize("parameters,call", [
    ("left=0", "choose(left=source())"),
    ("left: int", "choose(left=source())"),
    ("*left", "choose(source())"),
    ("**left", "choose(left=source())"),
    ("left", "choose(*source())"),
    ("left", "choose(**source())"),
    ("left", "choose(left=source(), left=0)"),
    ("left, left", "choose(source(), 0)"),
    ("left, *, /", "choose(source())"),
])
def test_unsupported_or_invalid_python_never_establishes_absence(tmp_path, parameters, call):
    client, rules = install(tmp_path, parameters, call)
    try:
        result = analyze(client, rules, "entry")
    except FrRuntimeError as error:
        assert "syntax" in str(error).lower()
        return
    assert not result.report.at("/complete")


@pytest.mark.parametrize("call", ["source(value=0)", "sink(value=source())", "clean(value=source())"])
def test_external_rules_need_a_keyword_signature_contract(tmp_path, call):
    client, rules = install(tmp_path, "left", call)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any(c.startswith("keyword-rule-contract-unchecked:") for c in result.report.at("/cutoffs"))


def test_keyword_values_run_in_source_order_and_stop_on_raise(tmp_path):
    extra = "def stop(value):\n    raise value\n\n"
    client, rules = install(tmp_path, "left, right", "choose(right=stop(sink(source())), left=sink(0))",
                            extra=extra)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    entry = result.summaries.for_function("entry")
    assert entry.may_raise and not entry.normal_return and len(entry.sinks) == 1
    assert "choose" not in entry.callees
    runtime = observe(tmp_path)
    assert runtime["raised"] == "TypeError" and runtime["events"] == [["source", 17], ["sink", 17]]
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("right=stop(sink(source())), left=sink(0)",
                                           "right=sink(source()), left=sink(0)"))
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") and not result.report.at("/returns")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["sink", 0]]


def test_entry_parameters_and_recursive_keyword_substitution(tmp_path):
    client, rules = install(tmp_path, "left, /, *, right",
        "choose(value, right=flag)",
        "if right:\n        return left\n    return choose(left, right=True)", "value, /, *, flag")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert set(result.report.at("/returns")) == {"parameter:value"}
    assert set(result.report.at("/origins")) == {"parameter:value", "parameter:flag"}
    assert result.summaries.for_function("choose").callees == ("choose",)
    for flag in (False, True):
        assert observe(tmp_path, f"entry(17, flag={flag})")["result"] == 17


@pytest.mark.parametrize("mutation", ["name", "order", "separator", "helper", "unrelated"])
def test_binding_dependencies_invalidate_and_match_clean_analysis(tmp_path, mutation):
    client, rules = install(tmp_path, "left, right", "sink(choose(right=0, left=source()))")
    store = MemoryObjectStore(); cache = FlowCache(store)
    first = analyze(client, rules, "entry", cache)
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, "entry", cache).reused
    path = tmp_path / "subject.py"; source = path.read_text()
    if mutation == "name": source = source.replace("right=0, left=source()", "left=0, right=source()")
    elif mutation == "order": source = source.replace("left, right", "right, left")
    elif mutation == "separator": source = source.replace("left, right", "left, /, right")
    elif mutation == "helper": source = source.replace("return left", "return right")
    else: (tmp_path / "unrelated.py").write_text("def other():\n    return 0\n")
    path.write_text(source)
    fresh = analyze(client, rules, "entry", cache)
    assert fresh.reused == (mutation == "unrelated")
    assert evidence(fresh) == evidence(analyze(client, rules, "entry"))
    assert (fresh.report.at("/input_digest") == first.report.at("/input_digest")) == (mutation == "unrelated")


@pytest.mark.parametrize("mutation", ["missing", "inputs", "binding", "integer", "input-integer", "semantics"])
def test_typed_call_contract_refuses_forged_or_mixed_versions(tmp_path, mutation):
    client, rules = install(tmp_path, "left", "choose(left=source())")
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    if mutation == "missing": del data["call_binding"]
    elif mutation == "inputs": data["inputs"]["call_binding"] = None
    elif mutation == "binding": data["call_binding"]["binding"] = "invented"
    elif mutation == "integer": data["call_binding"]["implicit_exceptions"] = 0
    elif mutation == "input-integer": data["inputs"]["call_binding"]["implicit_exceptions"] = 0
    else: data["semantics"] = "python-scalar-summaries-2"
    with pytest.raises(FrRuntimeError): FunctionSummaries.from_report(FrReport(data, ()))


@pytest.mark.parametrize("version", [1, 2])
def test_old_contracts_remain_readable_without_new_capabilities(tmp_path, version):
    client, rules = install(tmp_path, "left", "choose(source())")
    data = analyze(client, rules, "entry").report.to_data()
    data["semantics"] = f"python-scalar-summaries-{version}"
    data.pop("call_binding"); data["inputs"].pop("call_binding")
    if version == 1:
        data.pop("expression_control"); data["inputs"].pop("expression_control")
    result = FunctionSummaries.from_report(FrReport(data, ()))
    assert result.call_binding is None
    assert (result.expression_control is None) == (version == 1)


def test_keyword_calls_remain_independent_and_transfer_each_value_once(tmp_path):
    client, rules = install(tmp_path, "left, right", "0",
        "return left")
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("return 0", "unused = choose(right=0, left=source())\n    return sink(choose(right=source(), left=0))"))
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") and not result.witnesses
    source = path.read_bytes()
    from collections import Counter
    calls = Counter(event["occurrence"]["location"]["span"]["start"]
        for event in result.report.at("/events") if event["kind"] == "use"
        and source[event["occurrence"]["location"]["span"]["start"]:
                   event["occurrence"]["location"]["span"]["end"]] == b"source()")
    assert len(calls) == 2
    assert set(calls.values()) == {result.summaries.for_function("entry").evaluations}
    assert observe(tmp_path)["events"] == [["source", 17], ["source", 17], ["sink", 0]]


@pytest.mark.parametrize("import_line,call", [
    ("import portal.api as helpers", "helpers.choose(right=0, left=source())"),
    ("from portal import choose as dispatch", "dispatch(right=0, left=source())"),
])
def test_imported_keyword_bindings_use_the_callee_source_and_dependencies(tmp_path, import_line, call):
    client, rules = workspace(tmp_path)
    (tmp_path / "portal").mkdir()
    (tmp_path / "portal/__init__.py").write_text("from .api import choose\n")
    (tmp_path / "portal/api.py").write_text("def choose(*, left, right):\n    return left\n")
    (tmp_path / "subject.py").write_text(f"{import_line}\n\ndef entry():\n    return sink({call})\n")
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete"), first.report.at("/cutoffs")
    assert first.witnesses and observe(tmp_path)["result"] == 17
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "portal/api.py"
    path.write_text(path.read_text().replace("left, right", "right, left"))
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and changed.witnesses
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    path.write_text(path.read_text().replace("return left", "return right"))
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and not changed.witnesses and observe(tmp_path)["result"] == 0


@pytest.mark.parametrize("parameters,call", [
    ("K, K", "choose(0, source())"),
    ("left, é", "choose(left=0, é=source())"),
    ("left, K", "choose(left=source(), K=0)"),
])
def test_unicode_normalization_requires_an_explicit_contract(tmp_path, parameters, call):
    client, rules = install(tmp_path, parameters, call)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert ("module-effects-unchecked" in result.report.at("/cutoffs")
            or "non-ascii-keyword-binding-unchecked" in result.report.at("/cutoffs"))
