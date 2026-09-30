"""Literal defaults, exact signature origins and independent Python behavior."""
import ast
import copy
import json

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries, _CALL_BINDING
from fr_ir.runtime import FrReport, FrRuntimeError
from test_call_binding import install, observe
from test_flow_summaries import analyze, evidence, workspace


@pytest.mark.parametrize("literal", ["None", "False", "True", "0", "17", "-17", "+17", "0x10", "1.25", "-1.25", "1j", "'café'", "b'bytes'", "r'raw'", "(None)", "((False))"])
@pytest.mark.parametrize("explicit", [False, True])
def test_literal_defaults_match_independent_runtime(tmp_path, literal, explicit):
    call = "choose(source())" if explicit else "choose()"
    client, rules = install(tmp_path, f"left={literal}", f"sink({call})")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == explicit
    # Runtime JSON transport needs values with a JSON representation.
    if literal not in {"1j", "b'bytes'"}:
        runtime = observe(tmp_path)
        assert runtime["raised"] is None
        assert runtime["result"] == (17 if explicit else ast.literal_eval(literal))
        assert any(event == ["source", 17] for event in runtime["events"]) == explicit
    if literal in {"1j", "b'bytes'"}:
        namespace = {"source": lambda: 17, "sink": lambda value: value}
        exec(compile((tmp_path / "subject.py").read_text(), "subject.py", "exec"), namespace)
        assert namespace["entry"]() == (17 if explicit else ast.literal_eval(literal))
    signature = result.summaries.for_function("choose").signature
    assert len(signature) == 1 and signature[0].name == "left"
    assert not signature[0].required and signature[0].kind == "positional-or-keyword"
    assert result.summaries.call_binding.defaults


@pytest.mark.parametrize("parameters,call,tainted", [
    ("left=0, right=0", "choose()", False),
    ("left=0, right=0", "choose(right=source())", False),
    ("left=0, right=0", "choose(left=source())", True),
    ("left=0, /, right=0", "choose(source())", True),
    ("left=0, /, right=0", "choose(right=source())", False),
    ("left=0, *, right", "choose(right=source())", False),
    ("left, /, *, right=None", "choose(source())", True),
    ("*, left=0, right", "choose(right=source())", False),
    ("*, left, right=None", "choose(left=source())", True),
    ("left=0, # comment\n    /, *, right=None", "choose(source())", True),
])
def test_default_slots_respect_parameter_kinds(tmp_path, parameters, call, tainted):
    client, rules = install(tmp_path, parameters, f"sink({call})")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == tainted
    runtime = observe(tmp_path)
    assert runtime["raised"] is None and runtime["result"] == (17 if tainted else 0)
    assert result.summaries.for_function("choose").return_parameters == (0,)


@pytest.mark.parametrize("parameters,call", [
    ("left=0, /", "choose(left=source())"),
    ("*, left=0", "choose(source())"),
    ("left=0", "choose(source(), left=0)"),
    ("left=0, *, right", "choose()"),
    ("left=0", "choose(unknown=source())"),
])
def test_defaults_do_not_hide_invalid_argument_binding(tmp_path, parameters, call):
    client, rules = install(tmp_path, parameters, call)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "invalid-call-binding:choose" in result.report.at("/cutoffs")
    assert observe(tmp_path)["raised"] == "TypeError"


@pytest.mark.parametrize("default", ["source()", "[]", "{}", "set()", "(1, 2)", "[source()]", "1 + 2", "name", "lambda: 0", "f'{source()}'", "f''", "-True"])
def test_evaluated_or_nonadmitted_defaults_refuse_even_when_overridden(tmp_path, default):
    client, rules = install(tmp_path, f"left={default}", "choose(source())")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("parameters", ["left=0, right", "left=0, /, right", "left=0, left=1", "left: int=0", "*left=0", "left=0, *, /, right=0"])
def test_invalid_or_annotated_signatures_refuse(tmp_path, parameters):
    client, rules = install(tmp_path, parameters, "0")
    try:
        result = analyze(client, rules, "entry")
    except FrRuntimeError:
        return
    assert not result.report.at("/complete")


def test_definition_time_effect_is_not_reinterpreted_as_a_call_default(tmp_path):
    client, rules = install(tmp_path, "left=sink(source())", "choose(0)")
    runtime = observe(tmp_path)
    assert runtime["result"] == 0 and runtime["events"] == [["source", 17], ["sink", 17]]
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")


def test_signature_locations_match_independent_ast_and_rebind(tmp_path):
    client, rules = install(tmp_path, "left, /, middle='café', *, right=(None)", "sink(choose(source()))")
    path = tmp_path / "subject.py"
    path.write_text("# é before the signature\n" + path.read_text())
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache)
    assert first.report.at("/complete")
    source = path.read_text(); node = ast.parse(source).body[0]
    offsets = [0]
    for line in source.splitlines(keepends=True): offsets.append(offsets[-1] + len(line.encode()))
    def span(n): return offsets[n.lineno-1]+n.col_offset, offsets[n.end_lineno-1]+n.end_col_offset
    expected = node.args.posonlyargs + node.args.args + node.args.kwonlyargs
    signature = first.summaries.for_function("choose").signature
    assert [p.kind for p in signature] == ["positional-only", "positional-or-keyword", "keyword-only"]
    for parameter, argument in zip(signature, expected):
        assert (parameter.site.location.span.start, parameter.site.location.span.end) == span(argument)
        assert parameter.name == argument.arg
    # Parentheses belong to the retained syntax occurrence; Python's AST strips them.
    for parameter, literal in zip(signature[1:], ["'café'", "(None)"]):
        location = parameter.default.location.span
        assert source.encode()[location.start:location.end].decode() == literal
    (tmp_path / "other.py").write_text("def other(): return 0\n")
    renewed = analyze(client, rules, "entry", cache)
    assert renewed.reused and evidence(renewed) == evidence(analyze(client, rules, "entry"))
    assert all(p.site.revision == renewed.report.at("/revision") for p in renewed.summaries.for_function("choose").signature)


@pytest.mark.parametrize("mutation", ["default", "required", "kind", "body"])
def test_default_changes_invalidate_and_match_clean_analysis(tmp_path, mutation):
    client, rules = install(tmp_path, "left=0", "sink(choose())")
    cache = FlowCache(MemoryObjectStore())
    assert not analyze(client, rules, "entry", cache).witnesses
    assert analyze(client, rules, "entry", cache).reused
    path = tmp_path / "subject.py"; source = path.read_text()
    if mutation == "default": source = source.replace("left=0", "left=None")
    elif mutation == "required": source = source.replace("left=0", "left")
    elif mutation == "kind": source = source.replace("left=0", "*, left=0")
    else: source = source.replace("return left", "return source()")
    path.write_text(source)
    changed = analyze(client, rules, "entry", cache)
    assert not changed.reused and evidence(changed) == evidence(analyze(client, rules, "entry"))
    assert bool(changed.witnesses) == (mutation == "body")
    assert changed.report.at("/complete") == (mutation != "required")


@pytest.mark.parametrize("mutation", ["missing", "name", "kind", "role", "default-role", "revision", "length", "semantics", "legacy"])
def test_signature_and_default_contract_tampering_refuses(tmp_path, mutation):
    client, rules = install(tmp_path, "left=0", "choose()")
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    item = data["function_summaries"]["functions"]["choose"]
    if mutation == "missing": del item["signature"]
    elif mutation == "name": item["signature"][0]["name"] = "bad-name"
    elif mutation == "kind": item["signature"][0]["kind"] = "variadic"
    elif mutation == "role": item["signature"][0]["site"]["role"] = "source"
    elif mutation == "default-role": item["signature"][0]["default"]["role"] = "source"
    elif mutation == "revision": item["signature"][0]["site"]["revision"] = "0" * 64
    elif mutation == "length": item["signature"] = []
    elif mutation == "semantics": data["call_binding"]["defaults"] = "evaluate on each call"
    else: data["semantics"] = "python-scalar-summaries-3"
    with pytest.raises(FrRuntimeError): FunctionSummaries.from_report(FrReport(data, ()))


def test_version_three_reports_remain_readable_without_default_claims(tmp_path):
    client, rules = install(tmp_path, "left", "choose(source())")
    data = analyze(client, rules, "entry").report.to_data()
    data["semantics"] = "python-scalar-summaries-3"
    data["call_binding"] = copy.deepcopy(_CALL_BINDING)
    data["inputs"]["call_binding"] = copy.deepcopy(_CALL_BINDING)
    for item in data["function_summaries"]["functions"].values(): item.pop("signature")
    data.pop("assignment_control"); data["inputs"].pop("assignment_control")
    summaries = FunctionSummaries.from_report(FrReport(data, ()))
    assert summaries.call_binding.defaults is None
    assert all(item.signature is None for item in summaries.functions)


def test_imported_defaults_and_recursive_calls_remain_independent(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "portal").mkdir()
    (tmp_path / "portal/__init__.py").write_text("from .api import choose\n")
    (tmp_path / "portal/api.py").write_text("def choose(left=0, *, recurse=False):\n    if recurse:\n        return choose()\n    return left\n")
    (tmp_path / "subject.py").write_text("from portal import choose as dispatch\ndef entry():\n    dispatch(source())\n    return sink(dispatch())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and not result.witnesses
    signature = result.summaries.for_function("portal/api.py::choose").signature
    assert all(p.site.path == "portal/api.py" and p.default.path == "portal/api.py" for p in signature)
    assert observe(tmp_path)["result"] == 0


def test_entry_defaults_still_represent_all_explicit_inputs(tmp_path):
    client, rules = install(tmp_path, "left=0", "sink(left)", entry_parameters="left=0")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") and result.witnesses
    assert result.summaries.for_function("entry").return_parameters == (0,)
    assert observe(tmp_path)["result"] == 0
    assert observe(tmp_path, "entry(source())")["result"] == 17
