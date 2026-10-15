"""Known module values may be imported without inventing scalar reads or calls."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


VALUES = [
    ("marker = lambda: source()\n", "allocation"), ("marker = dict(send=lambda: source())\n", "allocation"),
    ("marker = 0\n", "literal"), ("marker = None\n", "literal"),
    ("marker = 'café'\n", "literal"), ("marker = []\n", "literal"),
    ("marker = {(0,): [None]}\n", "literal"), ("class marker:\n    pass\n", "class"),
    ("class marker:\n    def __init__(self):\n        sink(source())\n", "class"),
    ("class Token:\n    def __repr__(self):\n        return str(source())\nmarker = Token()\n", "instance"),
]


def install(tmp_path, declaration, route="direct", returned="value", body="return sink(relay(source()))"):
    client, rules = workspace(tmp_path)
    helper = declaration + "def relay(value):\n    return " + returned + "\n"
    (tmp_path / "helpers.py").write_text(helper)
    if route == "direct":
        prefix = "from helpers import marker, relay\n"
    elif route == "alias":
        prefix = "from helpers import marker as stored, relay\n"
    elif route == "reexport":
        (tmp_path / "bridge.py").write_text("from helpers import marker, relay\n")
        prefix = "from bridge import marker, relay\n"
    else:
        (tmp_path / "helpers.py").write_text("__all__ = ['marker', 'relay']\n" + helper)
        prefix = "from helpers import *\n"
    (tmp_path / "subject.py").write_text(prefix + "def entry():\n" +
        "".join("    " + line + "\n" for line in body.splitlines()))
    return client, rules


@pytest.mark.parametrize("declaration,kind", VALUES)
@pytest.mark.parametrize("route", ["direct", "alias", "reexport", "star"])
@pytest.mark.parametrize("returned", ["value", "0"])
def test_unused_imported_values_preserve_a_helper_without_becoming_functions(tmp_path, declaration, kind, route, returned):
    client, rules = install(tmp_path, declaration, route, returned)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is (returned == "value") and not result.initialization.sites
    values = result.dependencies.value_bindings
    assert len(values) == 1 and values[0].path == "helpers.py" and values[0].name == "marker"
    assert values[0].kind == kind
    source = (tmp_path / "helpers.py").read_text().encode()
    assert source[values[0].offset:].startswith(b"marker =" if kind != "class" else b"class marker:")
    lookup = next(item for item in result.dependencies.lookups if item.importer == "subject.py"
                  and item.alias == ("stored" if route == "alias" else "marker"))
    assert lookup.resolution == "value" and lookup.terminal_module is None and lookup.submodule is None
    assert lookup.binding_chain[-1].path == "helpers.py" and lookup.binding_chain[-1].name == "marker"
    assert "helpers.py::marker" not in {item.function for item in result.summaries.functions}
    assert observe(tmp_path) == {"result": 17 if returned == "value" else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if returned == "value" else 0]]}


@pytest.mark.parametrize("body", ["return sink(marker)", "return sink(marker(source()))",
                                  "return sink(marker.attribute)", "return sink(marker[0])"])
def test_value_reads_and_calls_remain_unfinished(tmp_path, body):
    client, rules = install(tmp_path, "marker = [17]\n", body=body)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.dependencies.complete and not result.initialization.sites
    assert next(item for item in result.dependencies.lookups if item.alias == "marker").resolution == "value"


@pytest.mark.parametrize("declaration", [
    "marker = source()\n", "marker: object = 0\n", "marker = other = 0\n",
    "marker = (lambda: 0)()\n", "@decorate\nclass marker:\n    pass\n",
    "class marker(list):\n    pass\n", "class marker:\n    value = source()\n",
    "class Token:\n    def __init__(self):\n        sink(source())\nmarker = Token()\n",
    "marker = 0\nmarker = 1\n", "marker = 0\nmarker = source()\n",
])
def test_computed_values_and_conflicting_declarations_are_not_promoted(tmp_path, declaration):
    client, rules = install(tmp_path, declaration)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and not result.dependencies.value_bindings
    assert next(item for item in result.dependencies.lookups if item.alias == "marker").resolution == "unavailable"


def test_a_known_import_defers_a_named_default_without_admitting_its_calls(tmp_path):
    client, rules = install(tmp_path, "marker = 0\n")
    subject = tmp_path / "subject.py"
    subject.write_text(subject.read_text() + "def unused(value=marker):\n    return value\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and result.dependencies.complete and not result.initialization.sites
    assert observe(tmp_path)["result"] == 17
    subject.write_text(subject.read_text().replace("relay(source())", "unused(source())"))
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and not result.initialization.sites
    assert "unsupported-parameter-contract:subject.py::unused" in result.report.at("/cutoffs")


def test_a_value_import_cannot_replace_an_external_rule(tmp_path):
    client, rules = install(tmp_path, "marker = 0\n")
    subject = tmp_path / "subject.py"
    subject.write_text("from helpers import marker as source\ndef entry():\n    return sink(source())\n")
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry", imports=True)


@pytest.mark.parametrize("edit,complete", [("marker = []\n", True), ("marker = source()\n", False),
                                          ("marker = 0\nmarker = 1\n", False)])
def test_value_declaration_edits_invalidate_reuse_and_match_fresh_analysis(tmp_path, edit, complete):
    client, rules = install(tmp_path, "marker = 0\n")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    (tmp_path / "helpers.py").write_text(edit + "def relay(value):\n    return value\n")
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused and result.report.at("/complete") is complete
    assert evidence(result) == evidence(analyze(client, rules, "entry", imports=True))


def rehash(value):
    value["input_digest"] = hashlib.sha256(json.dumps(value["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()


@pytest.mark.parametrize("change", ["missing", "empty", "duplicate", "path", "name", "kind", "offset",
                                    "function", "extra"])
def test_corrupt_or_older_value_inventories_fail_before_sdk_or_native_reuse(tmp_path, change):
    client, rules = install(tmp_path, "marker = 0\n")
    value = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    modules = value["inputs"]["modules"]
    binding = modules["value_bindings"][0]
    if change == "missing":
        del modules["value_bindings"]
    elif change == "empty":
        modules["value_bindings"] = []
    elif change == "duplicate":
        modules["value_bindings"].append(copy.deepcopy(binding))
    elif change == "function":
        next(item for item in modules["lookups"] if item["alias"] == "marker")["resolution"] = "function"
    elif change == "extra":
        binding["extra"] = 0
    else:
        binding[change] = {"path": "missing.py", "name": "other", "kind": "call", "offset": True}[change]
    rehash(value)
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-value-imports-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="imported value declarations"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
