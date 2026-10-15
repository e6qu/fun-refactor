"""Check function-name assignments against independent Python execution."""
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_postponed_annotations import observe
from test_flow_summaries import analyze, evidence, workspace


def install(tmp_path, helper, imports, call="public(source())"):
    client, rules = workspace(tmp_path)
    if imports:
        (tmp_path / "helpers.py").write_text(helper)
        helper = "from helpers import public\n"
    (tmp_path / "subject.py").write_text(helper + "def entry():\n    return sink(" + call + ")\n")
    return client, rules


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("forwards", [False, True])
@pytest.mark.parametrize("aliases", ["public = relay\n", "bridge = relay\npublic = bridge\n"])
def test_alias_calls_keep_actual_helper_transfers_and_locations(tmp_path, imports, forwards, aliases):
    client, rules = install(tmp_path, "def relay(value):\n    return " + ("value" if forwards else "0") + "\n" + aliases, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    identity = ("helpers.py::" if imports else "") + "relay"
    summary = result.summaries.for_function(identity)
    assert summary.return_parameters == ((0,) if forwards else ())
    assert summary.signature[0].site.text((tmp_path / ("helpers.py" if imports else "subject.py")).read_text(),
                                          revision=result.report.at("/revision")) == "value"
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("signature,call,expected", [
    ("value=0", "public()", 0),
    ("value=0", "public(value=source())", 17),
    ("value, /, *, other=0", "public(source(), other=0)", 17),
    ("value, *unused, **also_unused", "public(source())", 17),
])
def test_alias_calls_use_the_defining_signature(tmp_path, imports, signature, call, expected):
    client, rules = install(tmp_path, "def relay(" + signature + "):\n    return value\npublic = relay\n", imports, call)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is (expected == 17)
    assert observe(tmp_path)["result"] == expected


@pytest.mark.parametrize("statement,call", [
    ("from bridge import public", "public"),
    ("import bridge as module", "module.public"),
    ("from library import public", "public"),
    ("from library import *", "public"),
])
@pytest.mark.parametrize("forwards", [False, True])
def test_aliases_of_imported_functions_and_reexports(tmp_path, statement, call, forwards):
    client, rules = workspace(tmp_path)
    (tmp_path / "helpers.py").write_text("def relay(value):\n    return " + ("value" if forwards else "0") + "\n")
    (tmp_path / "bridge.py").write_text("from helpers import relay\npublic = relay\n")
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("from bridge import public as original\npublic = original\n__all__ = ['public']\n")
    (tmp_path / "subject.py").write_text(statement + "\ndef entry():\n    return sink(" + call + "(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    result.summaries.for_function("helpers.py::relay")
    assert observe(tmp_path)["result"] == (17 if forwards else 0)
    if statement == "from bridge import public":
        lookup = next(item for item in result.dependencies.lookups if item.importer == "subject.py")
        assert lookup.resolution == "function" and lookup.submodule is None
        assert [(hop.path, hop.name) for hop in lookup.binding_chain] == [
            ("bridge.py", "public"), ("bridge.py", "relay"), ("helpers.py", "relay")]


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("initialization", [
    "public = missing\n", "public = source\n", "public = help\n",
    "public = relay\ndef relay(value):\n    return value\n",
    "value = 0\npublic = value\n", "value = []\npublic = value\n",
    "class Marker:\n    pass\npublic = Marker\n",
    "def relay(value):\n    return value\npublic: Callable = relay\n",
    "def relay(value):\n    return value\npublic = (relay)\n",
    "def relay(value):\n    return value\npublic = relay(0)\n",
    "def relay(value):\n    return value\npublic = other = relay\n",
    "public = (lambda value: value)(0)\n",
])
def test_unchecked_alias_sources_and_assignment_forms_remain_incomplete(tmp_path, imports, initialization):
    client, rules = install(tmp_path, initialization, imports, "source()")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert result.initialization.sites


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("extra", [
    "public = relay\n", "public = 0\n", "relay = 0\n",
    "def relay(value):\n    return 0\n", "def public(value):\n    return 0\n",
    "del relay\n", "public.attribute = source()\n",
])
def test_rebinding_deletion_and_attribute_changes_cannot_establish_absence(tmp_path, imports, extra):
    client, rules = install(tmp_path, "def relay(value):\n    return value\npublic = relay\n" + extra, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("count", [1, 14, 15, 16])
def test_aliases_keep_the_existing_sixteen_hop_limit(tmp_path, imports, count):
    aliases = ""
    target = "relay"
    for index in range(count):
        name = "public" if index == count - 1 else "alias" + str(index)
        aliases += name + " = " + target + "\n"
        target = name
    client, rules = install(tmp_path, "def relay(value):\n    return value\n" + aliases, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") is (count <= (14 if imports else 15))
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("body,cutoff,returned,raised", [
    ("public = 0\nreturn sink(public(source()))", "ambiguous-call:public", None, "TypeError"),
    ("global public\nreturn sink(public(source()))", "unsupported-lexical-binding", 17, None),
])
def test_local_alias_shadowing_and_global_declarations_remain_incomplete(tmp_path, imports, body, cutoff, returned, raised):
    client, rules = install(tmp_path, "def relay(value):\n    return value\npublic = relay\n", imports)
    path = tmp_path / "subject.py"
    prefix = path.read_text().split("def entry():")[0]
    path.write_text(prefix + "def entry():\n" + "".join("    " + line + "\n" for line in body.splitlines()))
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete") and not result.initialization.sites
    assert cutoff in result.report.at("/cutoffs")
    runtime = observe(tmp_path)
    assert runtime["result"] == returned and runtime["raised"] == raised


def test_an_alias_value_read_is_distinct_from_an_alias_call(tmp_path):
    client, rules = install(tmp_path, "def relay(value):\n    return value\npublic = relay\n", False, "public")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.initialization.sites
    assert "unknown-binding:public" in result.report.at("/cutoffs")


def test_aliases_cannot_overlap_external_rule_names(tmp_path):
    client, rules = install(tmp_path, "def relay(value):\n    return value\nsource = relay\npublic = relay\n", False)
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry")


@pytest.mark.parametrize("replacement", ["public = stop\n", "public = missing\n", "public = relay(0)\n"])
def test_alias_edits_invalidate_saved_answers(tmp_path, replacement):
    helper = "def relay(value):\n    return value\ndef stop(value):\n    return 0\n"
    client, rules = install(tmp_path, helper + "public = relay\n", True)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and first.witnesses
    assert analyze(client, rules, "entry", cache, imports=True).reused
    (tmp_path / "helpers.py").write_text(helper + replacement)
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    if replacement == "public = stop\n":
        assert changed.report.at("/complete") and not changed.witnesses
        assert observe(tmp_path)["result"] == 0
    else:
        assert not changed.report.at("/complete")


def test_package_function_alias_precedes_a_same_named_child(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("def relay(value):\n    return value\npublic = relay\n")
    (package / "public.py").write_text("def relay(value):\n    return 0\n")
    (tmp_path / "subject.py").write_text("from library import public\ndef entry():\n    return sink(public(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and result.witnesses
    lookup = next(item for item in result.dependencies.lookups if item.importer == "subject.py")
    assert lookup.resolution == "function" and lookup.submodule is None
    assert "library/public.py" not in dict(result.dependencies.files)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("count", [127, 128, 129])
def test_alias_inventory_is_bounded_without_increasing_import_limits(tmp_path, imports, count):
    aliases = "".join(("public" if i == count - 1 else "alias" + str(i)) + " = relay\n" for i in range(count))
    client, rules = install(tmp_path, "def relay(value):\n    return value\n" + aliases, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") is (count <= 128)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("mutation", ["missing", "omitted", "target", "name", "path", "offset", "order", "duplicate", "hop"])
def test_alias_dependency_records_reject_forgery_even_after_rehashing(tmp_path, mutation):
    client, rules = install(tmp_path, "def relay(value):\n    return value\nbridge = relay\npublic = bridge\n", True)
    value = analyze(client, rules, "entry", imports=True).report.to_data()
    modules = value["inputs"]["modules"]
    aliases = modules["function_aliases"]
    if mutation == "missing":
        del modules["function_aliases"]
    elif mutation == "omitted":
        aliases.pop()
    elif mutation == "target":
        aliases[-1]["target"] = "different"
    elif mutation == "name":
        aliases[-1]["name"] = "café"
    elif mutation == "path":
        aliases[-1]["path"] = "../outside.py"
    elif mutation == "offset":
        aliases[0]["offset"] = True
    elif mutation == "order":
        aliases[-1]["offset"] = 0
    elif mutation == "duplicate":
        aliases.append(dict(aliases[0]))
    else:
        lookup = next(item for item in modules["lookups"] if item["importer"] == "subject.py")
        del lookup["binding_chain"][1]
    value["input_digest"] = hashlib.sha256(json.dumps(value["inputs"], sort_keys=True,
        ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))


def test_native_reuse_rejects_missing_alias_records_before_checking_freshness(tmp_path):
    client, rules = install(tmp_path, "def relay(value):\n    return value\npublic = relay\n", True)
    value = analyze(client, rules, "entry", imports=True).report.to_data()
    del value["inputs"]["modules"]["function_aliases"]
    value["input_digest"] = hashlib.sha256(json.dumps(value["inputs"], sort_keys=True,
        ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-alias-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="malformed function aliases"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
