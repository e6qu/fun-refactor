"""Separate bounded built-in type maps from user callbacks and global reads."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


REGISTRY = "{type(object.__dict__): dict, type({}.keys()): list, type({}.values()): list, type({}.items()): list}"
VALUES = [REGISTRY, "{int: str, bool: bytes}", "{type(None): object}",
          "{type(-1): float}", "{type('café'): str}", "{type([0]): tuple}",
          "{type({(0,): [None]}): dict}", "(({((type))(({}).keys()): (list)}))"]


def install(tmp_path, value=REGISTRY, route="local", prefix="", suffix="", forwards=True, body=None):
    client, rules = workspace(tmp_path)
    declaration = prefix + f"registry = {value}\n" + suffix
    helper = declaration + "def relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    prelude = helper
    if route != "local":
        (tmp_path / "helpers.py").write_text(helper)
        prelude = "from helpers import relay\n"
        if route == "value":
            prelude = "from helpers import registry, relay\n"
        elif route == "alias":
            prelude = "from helpers import registry as stored, relay\n"
        elif route == "reexport":
            (tmp_path / "bridge.py").write_text("from helpers import registry, relay\n")
            prelude = "from bridge import registry, relay\n"
        elif route == "star":
            (tmp_path / "helpers.py").write_text("__all__ = ['registry', 'relay']\n" + helper)
            prelude = "from helpers import *\n"
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    " +
                                       (body or "return sink(relay(source()))") + "\n")
    return client, rules


@pytest.mark.parametrize("value", VALUES)
@pytest.mark.parametrize("route", ["local", "helper", "value"])
@pytest.mark.parametrize("forwards", [False, True])
def test_unused_builtin_type_maps_preserve_helper_behavior(tmp_path, value, route, forwards):
    client, rules = install(tmp_path, value, route, forwards=forwards)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}
    if route == "value":
        binding, = result.dependencies.value_bindings
        assert binding.kind == "registry" and binding.path == "helpers.py" and binding.name == "registry"
        assert (tmp_path / binding.path).read_bytes()[binding.offset:].startswith(b"registry =")


@pytest.mark.parametrize("route", ["alias", "reexport", "star"])
def test_registry_imports_keep_the_terminal_declaration(tmp_path, route):
    client, rules = install(tmp_path, route=route)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    binding, = result.dependencies.value_bindings
    assert (binding.path, binding.name, binding.kind) == ("helpers.py", "registry", "registry")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("prefix", [
    "def type(value):\n    sink(source())\n    return int\n",
    "class object:\n    pass\n", "dict = list\n", "list = dict\n",
    "__builtins__ = __builtins__\n", "def unused(__builtins__):\n    pass\n",
])
@pytest.mark.parametrize("route", ["local", "helper"])
def test_shadowed_or_explicit_builtins_keep_initialization_unchecked(tmp_path, prefix, route):
    client, rules = install(tmp_path, route=route, prefix=prefix)
    result = analyze(client, rules, "entry", imports=route != "local")
    assert not result.report.at("/complete") and result.initialization.sites
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    runtime = observe(tmp_path)
    assert runtime["result"] == 17
    if prefix.startswith("def type"):
        assert runtime["events"] == [["source", 17], ["sink", 17]] * 5


@pytest.mark.parametrize("value", [
    "{type(source()): list}", "{type(marker): list}", "{type(User()): list}",
    "{object.__dict__: dict}", "{type(list.__dict__): dict}",
    "{type({}.keys(source())): list}", "{type(dict.keys({})): list}",
    "{type(*[]): list}", "{type(name='X', bases=(), dict={}): list}",
    "{type(1 + 2): int}", "{[]: list}", "{int: source}",
    "{int: source()}", "{int: Missing}", "{int: (lambda: 0)}",
    "{**{int: str}}", "{key: str for key in [int]}", "{type(f'constant'): str}",
])
def test_other_evaluation_is_not_promoted_to_a_builtin_registry(tmp_path, value):
    client, rules = install(tmp_path, value)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and result.initialization.sites


@pytest.mark.parametrize("count,complete", [(42, True), (43, False)])
def test_registry_ast_budget_keeps_the_existing_128_node_limit(tmp_path, count, complete):
    client, rules = install(tmp_path, "{" + ",".join("int: list" for _ in range(count)) + "}")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete") is complete, result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("suffix", ["registry[int] = source()\n", "registry = {}\n", "del registry\n"])
def test_later_registry_mutation_and_rebinding_stay_incomplete(tmp_path, suffix):
    client, rules = install(tmp_path, suffix=suffix)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and result.initialization.sites
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("body", ["return sink(registry)", "return sink(registry[int])", "return registry(source())"])
def test_an_inert_map_does_not_enable_global_reads_or_calls(tmp_path, body):
    client, rules = install(tmp_path, body=body)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.initialization.sites


def test_registry_setup_preserves_prior_markers_for_unused_defaults(tmp_path):
    client, rules = install(tmp_path, prefix="class Token:\n    pass\nmarker = Token()\n",
                            suffix="def unused(key=marker):\n    return opaque(key)\n")
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == 17


def test_an_imported_builtin_name_cannot_be_assumed_standard(tmp_path):
    client, rules = install(tmp_path, prefix="from altered import type\n")
    (tmp_path / "altered.py").write_text("def type(value):\n    sink(source())\n    return int\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]] * 5


def test_changed_registry_dependencies_invalidate_reuse(tmp_path):
    client, rules = install(tmp_path, route="value")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "helpers.py"
    path.write_text("type = source\n" + path.read_text())
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and not changed.report.at("/complete")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))


def test_noncurrent_registry_contracts_and_kinds_fail_before_reuse(tmp_path):
    client, rules = install(tmp_path, route="value")
    original = analyze(client, rules, "entry", imports=True).report.to_data()
    wrong = copy.deepcopy(original)
    wrong["inputs"]["modules"]["value_bindings"][0]["kind"] = "custom-registry"
    wrong["input_digest"] = hashlib.sha256(json.dumps(wrong["inputs"], sort_keys=True, ensure_ascii=False,
                                                     separators=(",", ":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(wrong, ()))
    for binding in (original["call_binding"], original["inputs"]["call_binding"]):
        binding["initialization"] = ("Literals, aliases, built-in references, exports and plain classes/instances are inert. "
                                     "Literal __doc__ writes to prior unchanged plain functions preserve scalar calls. "
                                     "Reads, collisions and other effects stay unchecked.")
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(original, ()))
    original["input_digest"] = hashlib.sha256(json.dumps(original["inputs"], sort_keys=True, ensure_ascii=False,
                                                        separators=(",", ":")).encode()).hexdigest()
    subject = tmp_path / "subject.py"
    subject.write_text(subject.read_text() + "unrelated = 0\n")
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-registry-retained-") as directory:
        path = Path(directory) / "retained.json"
        for data, message in [(wrong, "malformed imported value declarations"), (original, "unsupported call binding")]:
            encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
            path.write_text(encoded)
            with pytest.raises(FrRuntimeError, match=message):
                client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                               "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                               "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
