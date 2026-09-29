"""Parallel and chained scalar assignments compared with independent Python execution."""
import ast
import copy
import textwrap

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace


def install(tmp_path, body, helpers=""):
    client, rules = workspace(tmp_path)
    source = helpers + "\ndef entry():\n" + textwrap.indent(body + "\n", "    ")
    (tmp_path / "subject.py").write_text(source)
    return client, rules, source


def observe(source):
    events = []
    def source_value():
        events.append(["source", 17])
        return 17
    def sink(value):
        events.append(["sink", value])
        return value
    namespace = {"source": source_value, "sink": sink}
    exec(compile(source, "subject.py", "exec"), namespace)
    raised = None
    try:
        namespace["entry"]()
    except Exception as error:
        raised = type(error).__name__
    return events, raised


@pytest.mark.parametrize("body,tainted", [
    ("a, b = source(), 0\nsink(a)", True),
    ("a, b = source(), 0\nsink(b)", False),
    ("a, b = source(), 0\na, b = b, a\nsink(b)", True),
    ("a, b = source(), 0\na, b = b, a\nsink(a)", False),
    ("a = source()\na, b = 0, a\nsink(b)", True),
    ("a, a = source(), 0\nsink(a)", False),
    ("a, a = 0, source()\nsink(a)", True),
    ("(a, (b, c)) = [0, [source(), 0]]\nsink(b)", True),
    ("[a, [b, c]] = (source(), (0, 0))\nsink(c)", False),
    ("(a) = source()\nsink(a)", True),
    ("(a,) = (source(),)\nsink(a)", True),
    ("a, = [source()]\nsink(a)", True),
    ("a = b = source()\na = 0\nsink(b)", True),
    ("a = a = source()\nsink(a)", True),
    ("a, b = c, d = source(), 0\nsink(c)", True),
    ("a, b = b, a = source(), 0\nsink(a)", False),
    ("a, b = b, a = source(), 0\nsink(b)", True),
    ("[] = ()\nsink(0)", False),
    ("a, b = (source(), # ordered\n        0)\nsink(a)", True),
    ("café, spare = source(), 0\nsink(café)", True),
])
def test_literal_shapes_and_chains_match_python(tmp_path, body, tainted):
    client, rules, source = install(tmp_path, body)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == tainted
    events, raised = observe(source)
    assert raised is None
    assert any(event == ["sink", 17] for event in events) == tainted
    assert result.summaries.assignment_control is not None
    assert result.report.at("/assignment_control") == result.report.at("/inputs/assignment_control")


def test_rhs_effects_run_once_before_all_writes(tmp_path):
    body = "left = 0\nleft, right = other, again = source(), sink(left)\nsink(left)\nsink(right)"
    client, rules, source = install(tmp_path, body)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert observe(source) == ([["source", 17], ["sink", 0], ["sink", 17], ["sink", 0]], None)
    assert len(result.witnesses) == 1


@pytest.mark.parametrize("position", ["first", "middle", "last"])
def test_explicit_raise_stops_rhs_effects_and_target_writes(tmp_path, position):
    leaves = {"first": "stop(), sink(source()), 0", "middle": "sink(source()), stop(), 0",
              "last": "sink(source()), 0, stop()"}[position]
    client, rules, source = install(tmp_path, f"a, b, c = {leaves}\nsink(source())", "def stop():\n    raise 0\n")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert not result.summaries.for_function("entry").normal_return
    assert len(result.witnesses) == (0 if position == "first" else 1)
    events, raised = observe(source)
    assert raised == "TypeError"
    assert events == ([] if position == "first" else [["source", 17], ["sink", 17]])


@pytest.mark.parametrize("body", [
    "a, b = source()", "a, b = [source()]", "a, = [source(), 0]",
    "a, *rest = source(), 0", "a, b = (*source(), 0)",
    "a = b = [source(), 0]", "a, b = source(), [0]",
    "holder.item, b = source(), 0", "holder[0], b = source(), 0",
    "a: int = source()", "a, b = (x for x in source())",
])
def test_non_scalar_or_implicit_unpacking_contracts_refuse(tmp_path, body):
    client, rules, _ = install(tmp_path, body + "\nsink(0)")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert result.report.at("/cutoffs")


@pytest.mark.parametrize("body", [
    "sink(source())\nsource, spare = 0, 0",
    "source, spare = 0, 0\nsink(source())",
    "a, b = b, source()\nsink(a)",
])
def test_unpacking_names_remain_lexical_bindings(tmp_path, body):
    client, rules, source = install(tmp_path, body)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert observe(source)[1] in {"UnboundLocalError", "TypeError"}


def test_definition_occurrences_match_ast_and_rebind_after_unrelated_edits(tmp_path):
    body = "café, spare = source(), 0\ncafé, spare = spare, café\nresult = final = spare\nsink(final)"
    client, rules, source = install(tmp_path, body)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache)
    assert first.report.at("/complete")
    offsets = [0]
    for line in source.splitlines(keepends=True): offsets.append(offsets[-1] + len(line.encode()))
    stores = {(offsets[n.lineno-1] + n.col_offset, offsets[n.end_lineno-1] + n.end_col_offset)
              for n in ast.walk(ast.parse(source)) if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store)}
    definitions = [point for witness in first.witnesses for point in witness.occurrences if point.role == "definition"]
    assert len(definitions) >= 3
    for point in definitions:
        assert (point.location.span.start, point.location.span.end) in stores
    (tmp_path / "unrelated.py").write_text("def unrelated(): return 0\n")
    renewed = analyze(client, rules, "entry", cache)
    assert renewed.reused and evidence(renewed) == evidence(analyze(client, rules, "entry"))
    assert all(point.revision == renewed.report.at("/revision") for witness in renewed.witnesses for point in witness.occurrences)
    (tmp_path / "subject.py").write_text(source.replace("spare, café", "spare, 0"))
    edited = analyze(client, rules, "entry", cache)
    assert not edited.reused and not edited.witnesses
    assert evidence(edited) == evidence(analyze(client, rules, "entry"))


@pytest.mark.parametrize("mutation", ["missing", "inputs", "binding", "integer", "input-integer", "legacy"])
def test_assignment_contract_tampering_refuses(tmp_path, mutation):
    client, rules, _ = install(tmp_path, "a, b = source(), 0\nsink(a)")
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    if mutation == "missing": del data["assignment_control"]
    elif mutation == "inputs": data["inputs"]["assignment_control"] = None
    elif mutation == "binding": data["assignment_control"]["binding"] = "write before evaluation"
    elif mutation == "integer": data["assignment_control"]["implicit_exceptions"] = 0
    elif mutation == "input-integer": data["inputs"]["assignment_control"]["implicit_exceptions"] = 0
    else: data["semantics"] = "python-scalar-summaries-4"
    with pytest.raises(FrRuntimeError): FunctionSummaries.from_report(FrReport(data, ()))


def test_version_four_reports_remain_readable_without_assignment_claims(tmp_path):
    client, rules, _ = install(tmp_path, "a = source()\nsink(a)")
    data = analyze(client, rules, "entry").report.to_data()
    data["semantics"] = "python-scalar-summaries-4"
    data.pop("assignment_control"); data["inputs"].pop("assignment_control")
    summaries = FunctionSummaries.from_report(FrReport(data, ()))
    assert summaries.assignment_control is None and summaries.call_binding.defaults is not None


@pytest.mark.parametrize("body", [
    " = ".join(f"v{i}" for i in range(65)) + " = source()\nsink(v0)",
    ", ".join(f"v{i}" for i in range(65)) + " = " + ", ".join("source()" for _ in range(65)),
    "[" * 18 + "value" + "]" * 18 + " = " + "[" * 18 + "source()" + "]" * 18,
])
def test_assignment_budgets_never_establish_absence(tmp_path, body):
    client, rules, _ = install(tmp_path, body)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("assignment" in reason or "binding" in reason for reason in result.report.at("/cutoffs"))
