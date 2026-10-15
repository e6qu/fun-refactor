"""Compare inert built-in class bases and callback boundaries with Python."""
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
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


BASES = ["object", "dict", "Exception"]


def install(tmp_path, base="dict", route="local", prefix="", suffix="", forwards=True):
    client, rules = workspace(tmp_path)
    declaration = (prefix + "class Token(" + base + "):\n    code = 0\n"
                   "    def __init__(self):\n        sink(source())\n" + suffix)
    helper = declaration + "def relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    prelude = helper
    if route != "local":
        (tmp_path / "helpers.py").write_text(helper)
        prelude = "from helpers import relay\n"
        if route == "value":
            prelude = "from helpers import Token, relay\n"
        elif route == "alias":
            prelude = "from helpers import Token as saved, relay\n"
        elif route == "reexport":
            (tmp_path / "bridge.py").write_text("from helpers import Token, relay\n")
            prelude = "from bridge import Token, relay\n"
        elif route == "star":
            (tmp_path / "helpers.py").write_text("__all__ = ['Token', 'relay']\n" + helper)
            prelude = "from helpers import *\n"
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(relay(source()))\n")
    return client, rules


@pytest.mark.parametrize("base", BASES)
@pytest.mark.parametrize("spelling", ["{}", "(({}))", "# base\n    {}, # one base\n"])
@pytest.mark.parametrize("route", ["local", "helper", "value"])
@pytest.mark.parametrize("forwards", [False, True])
def test_builtin_bases_defer_method_bodies(tmp_path, base, spelling, route, forwards):
    client, rules = install(tmp_path, spelling.format(base), route, forwards=forwards)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}
    if route == "value":
        binding, = result.dependencies.value_bindings
        assert (binding.path, binding.name, binding.kind) == ("helpers.py", "Token", "class")
        assert (tmp_path / binding.path).read_bytes()[binding.offset:].startswith(b"class Token(")


@pytest.mark.parametrize("route", ["alias", "reexport", "star"])
def test_class_value_imports_keep_the_defining_declaration(tmp_path, route):
    client, rules = install(tmp_path, route=route)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    binding, = result.dependencies.value_bindings
    assert (binding.path, binding.name, binding.kind) == ("helpers.py", "Token", "class")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("base", BASES)
@pytest.mark.parametrize("route", ["local", "helper"])
def test_only_object_based_default_construction_is_admitted(tmp_path, base, route):
    client, rules = install(tmp_path, base, route, suffix="marker = Token()\n")
    path = tmp_path / ("subject.py" if route == "local" else "helpers.py")
    path.write_text(path.read_text().replace("    def __init__(self):\n        sink(source())\n", ""))
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete") is (base == "object")
    assert observe(tmp_path) == {"result": 17, "raised": None, "events": [["source", 17], ["sink", 17]]}


@pytest.mark.parametrize("base", BASES)
@pytest.mark.parametrize("route", ["local", "helper"])
@pytest.mark.parametrize("replacement", ["{base} = 0\n", "class {base}:\n    def __init_subclass__(cls):\n        sink(source())\n"])
def test_shadowed_builtin_names_do_not_hide_callbacks(tmp_path, base, route, replacement):
    client, rules = install(tmp_path, base, route, prefix=replacement.format(base=base))
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete") and result.initialization.sites
    actual = observe(tmp_path)
    if replacement.startswith("class"):
        assert actual["events"][:2] == [["source", 17], ["sink", 17]]
    else:
        assert actual["raised"] == "TypeError"


@pytest.mark.parametrize("base", ["object, object", "metaclass=type", "object, metaclass=type", "*[]", "**{}",
                                "dict[str, int]", "object.__class__", "bool", "Exception()"])
@pytest.mark.parametrize("route", ["local", "helper"])
def test_other_headers_stay_unchecked(tmp_path, base, route):
    client, rules = install(tmp_path, base, route)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete") and result.initialization.sites
    assert observe(tmp_path)["raised"] in {None, "TypeError"}


@pytest.mark.parametrize("base", BASES)
def test_imported_base_names_are_checked_in_the_defining_module(tmp_path, base):
    client, rules = install(tmp_path, base, route="helper", prefix="from aliases import " + base + "\n")
    (tmp_path / "aliases.py").write_text("class " + base + ":\n    def __init_subclass__(cls):\n        sink(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["source", 17], ["sink", 17]]


@pytest.mark.parametrize("base", BASES)
@pytest.mark.parametrize("method", ["__init__", "__new__"])
def test_executed_constructors_remain_unchecked(tmp_path, base, method):
    client, rules = install(tmp_path, base, suffix="marker = Token()\n")
    path = tmp_path / "subject.py"
    if method == "__new__":
        path.write_text(path.read_text().replace("def __init__(self):\n        sink(source())", "def __new__(cls):\n        sink(source())\n        return 0"))
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["source", 17], ["sink", 17]]


@pytest.mark.parametrize("base", BASES)
def test_explicit_builtins_prevents_a_builtin_base_claim(tmp_path, base):
    client, rules = install(tmp_path, base, prefix="saved = __builtins__\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert any(site.occurrence.location.range.start.line == 2 for site in result.initialization.sites)
    assert observe(tmp_path)["result"] == 17


def test_changed_base_invalidates_reuse_and_matches_fresh_analysis(tmp_path):
    client, rules = install(tmp_path, route="value")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "helpers.py"
    path.write_text("class dict:\n    def __init_subclass__(cls):\n        sink(source())\n" + path.read_text())
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and not changed.report.at("/complete")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    assert observe(tmp_path)["events"][:2] == [["source", 17], ["sink", 17]]


def test_previous_rules_fail_before_sdk_or_native_freshness_checks(tmp_path):
    client, rules = install(tmp_path, route="value")
    data = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    old = ("Literals, aliases, built-in references/type maps, exports and plain classes/instances are inert. "
           "Prior plain-function __doc__ writes preserve calls. Reads, collisions and other effects stay unchecked.")
    for contract in (data["call_binding"], data["inputs"]["call_binding"]):
        contract["initialization"] = old
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    path = tmp_path / "helpers.py"
    path.write_text(path.read_text().replace("code = 0", "code = 1"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-builtin-bases-retained-") as directory:
        report = Path(directory) / "retained.json"
        report.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="unsupported call binding"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(report),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
