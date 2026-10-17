"""A default can capture a function without executing its deferred body."""
import copy
import hashlib
import json
import tempfile
from pathlib import Path

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


FUNCTION = "def selected(value):\n    return source()\n"
RELAY = "def relay(value):\n    return value\n"
PRIOR_CONTRACT = (
    "Module function bodies defer with literal or known prior defaults, including variadics. "
    "Calls accept literals or captured local scalars. Annotations: checked module-local future, "
    "<=128 nodes, no await/yield/assignment/type unpacking."
)


def install(tmp_path, route="local", parameters="callback=((selected))", before="", after=""):
    client, rules = workspace(tmp_path)
    provider = FUNCTION + RELAY
    if route == "local":
        prelude = provider
    elif route == "local-alias":
        prelude = provider + "stored = selected\n"
        parameters = parameters.replace("selected", "stored")
    elif route == "builtin-alias":
        prelude = "selected = sorted\n" + RELAY
    else:
        (tmp_path / "helpers.py").write_text(provider)
        if route == "reexport":
            (tmp_path / "bridge.py").write_text("from helpers import selected, relay\n")
            prelude = "from bridge import selected, relay\n"
        elif route == "star":
            (tmp_path / "helpers.py").write_text("__all__ = ['selected', 'relay']\n" + provider)
            prelude = "from helpers import *\n"
        elif route == "import-alias":
            prelude = "from helpers import selected as stored, relay\n"
            parameters = parameters.replace("selected", "stored")
        else:
            prelude = "from helpers import selected, relay\n"
    (tmp_path / "subject.py").write_text(
        prelude + before + f"def unused({parameters}):\n    return callback(source())\n" + after +
        "def entry():\n    return sink(relay(source()))\n")
    return client, rules


@pytest.mark.parametrize("route", ["local", "local-alias", "builtin-alias", "direct", "import-alias", "reexport", "star"])
@pytest.mark.parametrize("parameters", ["callback=((selected))", "value, /, *items, callback=selected, **options"])
def test_capturing_a_known_function_does_not_call_it(tmp_path, route, parameters):
    client, rules = install(tmp_path, route, parameters)
    result = analyze(client, rules, "entry", imports=route not in {"local", "local-alias", "builtin-alias"})
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and not result.initialization.sites
    assert not any(item.function.endswith(("selected", "unused")) for item in result.summaries.functions)
    assert observe(tmp_path) == {"result": 17, "raised": None, "events": [["source", 17], ["sink", 17]]}


@pytest.mark.parametrize("route", ["local", "direct", "reexport"])
@pytest.mark.parametrize("call", ["unused()", "unused(source())"])
def test_default_consumption_and_calls_through_parameters_stay_unfinished(tmp_path, route, call):
    client, rules = install(tmp_path, route)
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("sink(relay(source()))", call))
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete") and not result.initialization.sites
    reason = "unsupported-default-value:" if call == "unused()" else "unknown-external-call:callback"
    assert any(item.startswith(reason) for item in result.report.at("/cutoffs"))


@pytest.mark.parametrize("before", ["del selected\n", "selected = 0\n", "selected = source()\n", "setup()\n"])
def test_intervening_writes_and_unknown_calls_keep_definition_unchecked(tmp_path, before):
    client, rules = install(tmp_path, before=before)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in item.reasons for item in result.initialization.sites)


@pytest.mark.parametrize("route", ["local", "direct"])
def test_a_later_function_or_import_cannot_supply_an_earlier_default(tmp_path, route):
    client, rules = install(tmp_path, route)
    path = tmp_path / "subject.py"
    text = path.read_text()
    binding = FUNCTION if route == "local" else "from helpers import selected, relay\n"
    path.write_text(text.replace(binding, "") + binding)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in item.reasons for item in result.initialization.sites)
    assert observe(tmp_path)["raised"] == "NameError"


@pytest.mark.parametrize("default", ["selected(source())", "selected.__call__", "missing", "dict()"])
def test_calls_attribute_reads_and_unbound_names_stay_unchecked(tmp_path, default):
    client, rules = install(tmp_path, parameters="callback=" + default)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in item.reasons for item in result.initialization.sites)


@pytest.mark.parametrize("builtin", ["dict", "sorted", "open", "len", "__import__"])
@pytest.mark.parametrize("route", ["local", "direct"])
def test_builtin_defaults_read_objects_without_invoking_them(tmp_path, builtin, route):
    client, rules = install(tmp_path, route, parameters="callback=" + builtin)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert not result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


@pytest.mark.parametrize("before", ["dict = source()\n", "del dict\n", "__builtins__ = {}\n"])
def test_builtin_shadowing_and_explicit_builtin_namespaces_stay_unchecked(tmp_path, before):
    client, rules = install(tmp_path, parameters="callback=dict", before=before)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in item.reasons for item in result.initialization.sites)


def test_evaluating_a_default_calls_the_function_during_import(tmp_path):
    client, rules = install(tmp_path, parameters="callback=selected(0)")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert observe(tmp_path) == {"result": 17, "raised": None,
                                "events": [["source", 17], ["source", 17], ["sink", 17]]}


def test_later_deletion_does_not_reinterpret_an_existing_capture(tmp_path):
    client, rules = install(tmp_path, after="del selected\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert all("definition-time-code" not in item.reasons for item in result.initialization.sites)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("route", ["local", "direct"])
def test_replacing_a_function_reference_with_a_call_invalidates_reuse(tmp_path, route):
    client, rules = install(tmp_path, route)
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=route != "local").report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=route != "local").reused
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("((selected))", "selected(0)"))
    changed = analyze(client, rules, "entry", cache, imports=route != "local")
    assert not changed.reused and not changed.report.at("/complete")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=route != "local"))


def test_previous_default_rules_are_rejected_before_reuse(tmp_path):
    client, rules = install(tmp_path)
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    for contract in (data["call_binding"], data["inputs"]["call_binding"]):
        contract["definitions"] = PRIOR_CONTRACT
    with pytest.raises(FrRuntimeError, match="unsupported call binding"):
        FunctionSummaries.from_report(FrReport(data, ()))
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-function-defaults-") as directory:
        retained = Path(directory) / "retained.json"
        retained.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="unsupported call binding"):
            client.project("dataflow", handle, "--summaries", "--rules", str(rules), "--steps", "4096",
                           "--bytes", "1048576", "--reuse", str(retained),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())


@pytest.mark.parametrize("edit", ["def selected(value):\n    return value\n", "selected = source()\n", ""])
def test_changed_imported_function_invalidates_its_capture_evidence(tmp_path, edit):
    client, rules = install(tmp_path, route="reexport")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    (tmp_path / "helpers.py").write_text(edit + RELAY)
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused
    assert changed.report.at("/complete") is edit.startswith("def ")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))


def test_an_alias_can_capture_a_prior_function_that_itself_has_a_function_default(tmp_path):
    client, rules = install(tmp_path, route="local-alias")
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("selected(value)", "selected(value, factory=dict)"))
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert not result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


@pytest.mark.parametrize("setup", ["def dict(value):\n    return source()\n", ""])
def test_function_or_builtin_lookup_respects_the_definition_point(tmp_path, setup):
    client, rules = install(tmp_path, parameters="callback=dict", before=setup,
                            after="def dict(value):\n    return source()\n" if not setup else "")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]
