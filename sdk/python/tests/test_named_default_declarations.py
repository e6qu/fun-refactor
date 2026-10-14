"""Reading an earlier known default defers an uncalled module function body."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_call_binding import observe
from test_flow_summaries import analyze, evidence, workspace


VALUES = [
    "marker = 0\n", "marker = []\n", "marker = (0,)\n", "marker = 'café'\n",
    "class marker:\n    pass\n",
    "class Token:\n    def __repr__(self):\n        return 'opaque'\nmarker = Token()\n",
]


def install(tmp_path, declaration=VALUES[0], route="local", parameters="key=marker", prefix="", suffix=""):
    client, rules = workspace(tmp_path)
    helper = "def relay(value):\n    return value\n"
    if route == "local":
        prelude = declaration + helper
    else:
        (tmp_path / "helpers.py").write_text(declaration + helper)
        if route == "direct":
            prelude = "from helpers import marker, relay\n"
        elif route == "alias":
            prelude = "from helpers import marker as stored, relay\n"
            parameters = parameters.replace("marker", "stored")
        elif route == "reexport":
            (tmp_path / "bridge.py").write_text("from helpers import marker, relay\n")
            prelude = "from bridge import marker, relay\n"
        else:
            (tmp_path / "helpers.py").write_text("__all__ = ['marker', 'relay']\n" + declaration + helper)
            prelude = "from helpers import *\n"
    (tmp_path / "subject.py").write_text(
        prelude + prefix + "def unused(" + parameters + "):\n    sink(source())\n    return opaque(key)\n" +
        suffix + "def entry():\n    return sink(relay(source()))\n")
    return client, rules


@pytest.mark.parametrize("declaration", VALUES)
@pytest.mark.parametrize("route", ["local", "direct", "alias", "reexport", "star"])
@pytest.mark.parametrize("parameters", ["key=marker", "value, /, *items, key=((marker)), **options"])
def test_prior_values_defer_unused_module_functions(tmp_path, declaration, route, parameters):
    client, rules = install(tmp_path, declaration, route, parameters)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and not result.initialization.sites
    assert not any(item.function.endswith("unused") for item in result.summaries.functions)
    assert observe(tmp_path) == {"result": 17, "raised": None, "events": [["source", 17], ["sink", 17]]}


@pytest.mark.parametrize("prefix", [
    "del marker\n", "marker = 1\n", "marker = source()\n",
    "def marker():\n    return 0\n", "def setup():\n    pass\nsetup()\n",
    "exec('del marker')\n",
])
def test_shadowing_and_unknown_setup_do_not_leave_a_known_default(tmp_path, prefix):
    client, rules = install(tmp_path, prefix=prefix)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in site.reasons for site in result.initialization.sites)


@pytest.mark.parametrize("default", ["missing", "source()", "marker[0]", "marker.real", "marker + 1", "Token()"])
def test_other_default_evaluation_stays_unchecked(tmp_path, default):
    client, rules = install(tmp_path, declaration="class Token:\n    pass\nmarker = [0]\n", parameters="key=" + default)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in site.reasons for site in result.initialization.sites)


@pytest.mark.parametrize("route", ["local", "direct"])
def test_a_later_binding_cannot_supply_an_earlier_default(tmp_path, route):
    client, rules = install(tmp_path, route=route)
    path = tmp_path / "subject.py"
    text = path.read_text()
    line = "marker = 0\n" if route == "local" else "from helpers import marker, relay\n"
    path.write_text(text.replace(line, "") + line)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete")
    assert any("definition-time-code" in site.reasons for site in result.initialization.sites)
    assert observe(tmp_path)["raised"] == "NameError"


@pytest.mark.parametrize("call", ["unused()", "unused(source())"])
@pytest.mark.parametrize("declaration", [VALUES[0], VALUES[1], VALUES[-1]])
def test_calling_a_named_default_stays_unfinished_even_when_overridden(tmp_path, call, declaration):
    client, rules = install(tmp_path, declaration)
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("sink(relay(source()))", call))
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.initialization.sites
    assert "unsupported-parameter-contract:unused" in result.report.at("/cutoffs")


def test_parameter_names_do_not_shadow_globals_during_definition(tmp_path):
    client, rules = install(tmp_path, parameters="marker, key=marker")
    assert analyze(client, rules, "entry").report.at("/complete")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("route", ["local", "direct"])
def test_a_class_default_does_not_execute_its_custom_constructor(tmp_path, route):
    client, rules = install(tmp_path, declaration="class marker:\n    def __init__(self):\n        sink(source())\n", route=route)
    assert analyze(client, rules, "entry", imports=route != "local").report.at("/complete")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


def test_later_deletion_does_not_reinterpret_an_already_created_default(tmp_path):
    client, rules = install(tmp_path, suffix="del marker\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert result.initialization.sites and all("definition-time-code" not in site.reasons
                                               for site in result.initialization.sites)
    assert observe(tmp_path)["result"] == 17


def test_class_method_defaults_keep_their_separate_namespace_unchecked(tmp_path):
    client, rules = install(tmp_path)
    path = tmp_path / "subject.py"
    path.write_text(path.read_text() + "class Container:\n    marker = 0\n    def method(self, key=marker):\n        return key\n")
    assert not analyze(client, rules, "entry").report.at("/complete")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("edit", ["del marker\n", "marker = source()\n", "marker = 1\n"])
def test_changed_availability_invalidates_reuse_and_matches_fresh_analysis(tmp_path, edit):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache).report.at("/complete")
    assert analyze(client, rules, "entry", cache).reused
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("def unused", edit + "def unused"))
    result = analyze(client, rules, "entry", cache)
    assert not result.reused and not result.report.at("/complete")
    assert evidence(result) == evidence(analyze(client, rules, "entry"))


def test_old_declaration_rules_fail_before_sdk_or_native_reuse(tmp_path):
    client, rules = install(tmp_path)
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    for contract in (data["call_binding"], data["inputs"]["call_binding"]):
        contract["definitions"] = ("ASCII signatures and literal defaults defer bodies, including variadics. "
                                   "Annotations need a checked module-local future directive and at most 128 nodes. "
                                   "No annotation await, yield, assignment or type unpacking.")
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    subject = tmp_path / "subject.py"
    subject.write_text(subject.read_text().replace("marker = 0", "marker = 1"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-named-defaults-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="unsupported call binding"):
            client.project("dataflow", handle, "--summaries", "--rules", str(rules), "--steps", "4096",
                           "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
