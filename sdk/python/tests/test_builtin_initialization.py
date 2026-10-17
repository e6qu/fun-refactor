"""Unused standard built-in references must not invent call or value behavior."""
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


# Python's documented functions and types; help is supplied by site, not builtins.
BUILTINS = """abs aiter all anext any ascii bin bool breakpoint bytearray bytes callable
chr classmethod compile complex delattr dict dir divmod enumerate eval exec filter float
format frozenset getattr globals hasattr hash hex id input int isinstance issubclass iter
len list locals map max memoryview min next object oct open ord pow print property range
repr reversed round set setattr slice sorted staticmethod str sum super tuple type vars
zip __import__""".split()


def install(tmp_path, prefix, imports, returned="value", body="return sink(relay(source()))"):
    client, rules = workspace(tmp_path)
    helper = prefix + "def relay(value):\n    return " + returned + "\n"
    if imports:
        (tmp_path / "helpers.py").write_text(helper)
        helper = "from helpers import relay\n"
        if "stored" in body:
            helper += "from helpers import stored\n"
    (tmp_path / "subject.py").write_text(helper + "def entry():\n" +
        "".join("    " + line + "\n" for line in body.splitlines()))
    return client, rules


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("name", BUILTINS)
def test_unused_builtin_bindings_preserve_the_helper_and_do_not_become_summaries(tmp_path, imports, name):
    prefix = "stored = " + name + "\n"
    if not name.startswith("__"):
        prefix += name + " = " + name + "\n"
    client, rules = install(tmp_path, prefix, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and not result.initialization.sites
    assert len(result.summaries.functions) == 2
    if imports:
        assert result.report.at("/inputs/modules/function_aliases") == []
    assert observe(tmp_path) == {"result": 17, "raised": None,
                                "events": [["source", 17], ["sink", 17]]}


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("returned", ["0", "value"])
def test_tool_style_declarations_do_not_change_a_helper_transfer(tmp_path, imports, returned):
    client, rules = install(tmp_path, "sorted = sorted\nmap = map\nfilter = filter\n", imports, returned)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") and bool(result.witnesses) is (returned == "value")
    assert observe(tmp_path)["result"] == (17 if returned == "value" else 0)


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("prefix,kind", [
    ("sorted = 0\nstored = sorted\n", "literal"),
    ("sorted = []\nstored = sorted\n", "literal"),
    ("class sorted:\n    pass\nstored = sorted\n", "class"),
])
def test_unused_aliases_preserve_values_named_like_builtins(tmp_path, imports, prefix, kind):
    client, rules = install(tmp_path, prefix, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and not result.initialization.sites
    assert observe(tmp_path) == {"result": 17, "raised": None,
                                "events": [["source", 17], ["sink", 17]]}
    if imports:
        assert not result.dependencies.function_aliases
        alias, = result.dependencies.value_aliases
        assert (alias.path, alias.name, alias.target) == ("helpers.py", "stored", "sorted")
        original, = result.dependencies.value_bindings
        assert (original.path, original.name, original.kind) == ("helpers.py", "sorted", kind)


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("prefix", [
    "stored = missing\n",
    "stored = help\n", "stored = quit\n", "stored = sorted(())\n",
    "stored = (sorted)\n", "stored: object = sorted\n", "stored = other = sorted\n",
    "__custom__ = sorted\n", "sorted = sorted\nsorted = sorted\n",
    "stored = sorted\nstored = 0\n", "stored = sorted\ndef stored(value):\n    return 0\n",
    "stored = sorted\nstored.attribute = source()\n", "stored = sorted\ndel stored\n",
])
def test_unknown_values_effects_and_rebindings_leave_initialization_unchecked(tmp_path, imports, prefix):
    client, rules = install(tmp_path, prefix, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs") and result.initialization.sites


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("prefix", [
    "__builtins__ = {}\n", "def __builtins__():\n    return 0\n",
    "class __builtins__:\n    pass\n", "def unused(__builtins__):\n    return 0\n",
])
def test_explicit_builtin_namespace_references_prevent_standard_admission(tmp_path, imports, prefix):
    client, rules = install(tmp_path, prefix + "stored = sorted\n", imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete") and result.initialization.sites
    path = "helpers.py" if imports else "subject.py"
    source = (tmp_path / path).read_text()
    assert any(site.occurrence.text(source, revision=result.report.at("/revision")) == "stored = sorted"
               for site in result.initialization.sites if site.occurrence.path == path)


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("body", ["return sink(stored)", "return sink(stored(source()))",
                                  "stored = 0\nreturn sink(stored(source()))"])
def test_using_a_stored_builtin_remains_unchecked(tmp_path, imports, body):
    client, rules = install(tmp_path, "stored = abs\n", imports, body=body)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete") and not result.initialization.sites


@pytest.mark.parametrize("imports", [False, True])
def test_a_prior_local_function_named_like_a_builtin_keeps_its_own_behavior(tmp_path, imports):
    client, rules = install(tmp_path, "def sorted(value):\n    return 0\nstored = sorted\n", imports,
                            body="return sink(stored(source()))")
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") and not result.witnesses
    assert observe(tmp_path)["result"] == 0


def test_imported_value_preserves_its_original_declaration(tmp_path):
    client, rules = install(tmp_path, "from other import sorted\nstored = sorted\n", True)
    (tmp_path / "other.py").write_text("sorted = 0\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert result.witnesses and not result.initialization.sites
    lookup = next(item for item in result.dependencies.lookups
                  if item.importer == "helpers.py" and item.alias == "sorted")
    assert lookup.resolution == "value"
    assert [(hop.path, hop.name) for hop in lookup.binding_chain] == [("other.py", "sorted")]
    assert not result.dependencies.function_aliases
    assert observe(tmp_path)["result"] == 17


def test_unknown_import_named_like_a_builtin_stays_incomplete(tmp_path):
    client, rules = install(tmp_path, "from other import sorted\nstored = sorted\n", True)
    (tmp_path / "other.py").write_text("from itertools import chain as sorted\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.initialization.sites
    assert not result.dependencies.value_aliases
    assert observe(tmp_path)["result"] == 17


def test_an_explicit_builtin_value_precedes_a_same_named_package_child(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("sorted = sorted\n")
    (package / "sorted.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text("from library import sorted\ndef entry():\n    return sink(sorted(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    lookup = next(item for item in result.dependencies.lookups if item.importer == "subject.py")
    assert lookup.resolution == "unavailable" and lookup.submodule is None
    assert "library/sorted.py" not in dict(result.dependencies.files)


@pytest.mark.parametrize("imports", [False, True])
def test_a_stored_builtin_cannot_replace_an_external_rule(tmp_path, imports):
    client, rules = install(tmp_path, "source = abs\n", imports)
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry", imports=imports)


@pytest.mark.parametrize("edit", ["stored = map\n", "stored = source()\n", "stored = abs\nstored = 0\n"])
def test_declaration_edits_invalidate_reuse_and_match_clean_analysis(tmp_path, edit):
    client, rules = install(tmp_path, "stored = abs\n", True)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    helper = tmp_path / "helpers.py"
    helper.write_text(edit + "def relay(value):\n    return value\n")
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused and evidence(result) == evidence(analyze(client, rules, "entry", imports=True))
    assert result.report.at("/complete") is (edit == "stored = map\n")


def test_noncurrent_initialization_contract_is_rejected_before_reuse(tmp_path):
    client, rules = install(tmp_path, "stored = abs\n", True)
    value = analyze(client, rules, "entry", imports=True).report.to_data()
    value["call_binding"]["initialization"] = "outdated contract"
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(value, ()))
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-builtins-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="call binding"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
