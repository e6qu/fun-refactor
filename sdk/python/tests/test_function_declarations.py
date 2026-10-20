import copy

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_call_binding import observe
from test_flow_summaries import analyze, evidence, workspace


DECLARATIONS = [
    "def unused(*items):\n    return items[0]\n",
    "def unused(**options):\n    return options['value']\n",
    "def unused(value, /, *items, flag=True, **options):\n    return opaque(items, options)\n",
    "def unused(*items):\n    yield items\n",
    "async def unused(*items):\n    await opaque(items)\n",
    "def unused(*items):\n    return lambda: items\n",
]


@pytest.mark.parametrize("declaration", DECLARATIONS)
@pytest.mark.parametrize("imports", [False, True])
def test_uncalled_function_bodies_do_not_run_at_module_initialization(tmp_path, declaration, imports):
    client, rules = workspace(tmp_path)
    helper = "def relay(value):\n    return value\n"
    if imports:
        package = tmp_path / "library"
        package.mkdir()
        (package / "__init__.py").write_text(declaration + "from .core import relay\n")
        (package / "core.py").write_text(helper)
        prelude = "from library import relay\n"
    else:
        prelude = declaration + helper
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and result.summaries.call_binding.definitions
    assert not any(item.function.endswith("unused") for item in result.summaries.functions)
    assert observe(tmp_path) == {"result": 17, "raised": None, "events": [["source", 17], ["sink", 17]]}


def test_referenced_variadic_calls_still_refuse_without_an_initialization_cutoff(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        "def deferred(*items):\n    return sink(items[0])\n"
        "def entry():\n    return deferred(source())\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.witnesses
    assert "unsupported-parameter-contract:deferred" in result.report.at("/cutoffs")
    assert "module-effects-unchecked" not in result.report.at("/cutoffs")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


def test_unsupported_entry_parameters_cannot_establish_absence(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("def entry(*items):\n    items = 0\n    return sink(source())\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "unsupported-parameter-contract:entry" in result.report.at("/cutoffs")
    assert not result.summaries.complete
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


@pytest.mark.parametrize("body", [
    "yield sink(value)",
    "return False and (yield sink(value))",
    "return False and (lambda ignored=(yield sink(value)): 0)",
    "def inner(ignored=(yield sink(value))):\n        return ignored\n    return 0",
])
@pytest.mark.parametrize("summaries", [False, True])
def test_generator_construction_does_not_execute_its_body(tmp_path, body, summaries):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        "def deferred(value):\n    " + body + "\n"
        "def entry():\n    return deferred(source())\n")
    handle = client.project("find", "entry").definition_target().handle
    result = FlowCache(MemoryObjectStore()).analyze(client, handle, standard_library=(), rules=rules,
        summaries=summaries, steps=4096, max_bytes=1_048_576)
    assert not result.report.at("/complete") and not result.witnesses
    assert "generator-function" in result.report.at("/cutoffs")
    assert observe(tmp_path, "entry().close()")["events"] == [["source", 17]]


def test_a_skipped_nested_generator_does_not_turn_its_parent_into_a_generator(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        "def relay(value):\n    return 0 if True else (lambda: (yield value))\n"
        "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") and not result.witnesses
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 0]]


def test_coroutine_construction_does_not_execute_its_body(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(
        "async def deferred(value):\n    return sink(value)\n"
        "def entry():\n    return deferred(source())\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.witnesses
    assert "async-function" in result.report.at("/cutoffs")
    assert observe(tmp_path, "entry().close()")["events"] == [["source", 17]]


def test_definition_time_effects_invalidate_a_cached_inert_declaration(tmp_path):
    client, rules = workspace(tmp_path)
    path = tmp_path / "subject.py"
    path.write_text(DECLARATIONS[0] + "def entry():\n    return sink(source())\n")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache).report.at("/complete")
    assert analyze(client, rules, "entry", cache).reused
    path.write_text(path.read_text().replace("unused(*items)", "unused(marker=source(), *items)"))
    result = analyze(client, rules, "entry", cache)
    assert not result.reused and not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert evidence(result) == evidence(analyze(client, rules, "entry"))
    assert observe(tmp_path)["events"] == [["source", 17], ["source", 17], ["sink", 17]]


@pytest.mark.parametrize("mutation", ["missing", "inputs", "description"])
def test_current_declaration_contract_is_required(tmp_path, mutation):
    client, rules = workspace(tmp_path)
    data = copy.deepcopy(analyze(client, rules).report.to_data())
    if mutation == "missing":
        del data["call_binding"]["definitions"]
    elif mutation == "inputs":
        del data["inputs"]["call_binding"]["definitions"]
    else:
        data["call_binding"]["definitions"] = "execute unsupported bodies during initialization"
        data["inputs"]["call_binding"]["definitions"] = data["call_binding"]["definitions"]
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
