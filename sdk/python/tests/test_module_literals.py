"""Initialization admission must preserve runtime bindings and cache boundaries."""
import copy

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_call_binding import observe
from test_flow_summaries import analyze, evidence, workspace


LITERALS = ["None", "True", "False", "17", "-17", "+2.5", "2j", "'café'", "b'bytes'", "r'raw'", "((0))"]
METADATA = ["__version__", "__author__", "__license__", "__copyright__"]


@pytest.mark.parametrize("name", METADATA)
@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("forwards", [False, True])
def test_literal_package_metadata_preserves_helper_flow(tmp_path, name, imports, forwards):
    client, rules = workspace(tmp_path)
    helper = name + " = 'café'\ndef relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    if imports:
        package = tmp_path / "library"
        package.mkdir()
        (package / "__init__.py").write_text(helper)
        prefix = "from library import relay\n"
    else:
        prefix = helper
    (tmp_path / "subject.py").write_text(prefix + "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards
    assert not result.initialization.sites
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}


@pytest.mark.parametrize("binding", [
    "__version__ = source()", "__version__ = []", "__author__ = f'{source()}'",
    "__license__: str = 'MIT'", "__copyright__ = other = 'owner'",
    "__version__ = '1'\n__version__ = '2'", "__custom__ = 0",
    "__path__ = ''", "__name__ = ''", "__package__ = ''", "__spec__ = None",
    "__loader__ = None", "__file__ = ''", "__cached__ = ''", "__builtins__ = 0",
    "__annotations__ = None", "__annotate__ = None", "__getattr__ = None",
])
@pytest.mark.parametrize("imports", [False, True])
def test_metadata_exception_does_not_hide_evaluation_or_import_controls(tmp_path, binding, imports):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(binding + "\ndef entry():\n    return sink(source())\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert result.initialization.sites


def test_metadata_reads_and_class_metadata_stay_unchecked(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("__version__ = '8.1.0'\ndef entry():\n    return sink(__version__)\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.initialization.sites
    assert "unknown-binding:__version__" in result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == "8.1.0"
    (tmp_path / "subject.py").write_text("class Token:\n    __version__ = '8.1.0'\ndef entry():\n    return sink(source())\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and result.initialization.sites


@pytest.mark.parametrize("edit", ["__version__ = '9'\n", "__version__ = source()\n",
                                  "__version__ = '8'\n__version__ = '9'\n"])
def test_metadata_edits_invalidate_reuse(tmp_path, edit):
    client, rules = workspace(tmp_path)
    helper = tmp_path / "helper.py"
    body = "def relay(value):\n    return value\n"
    helper.write_text("__version__ = '8'\n" + body)
    (tmp_path / "subject.py").write_text("from helper import relay\ndef entry():\n    return sink(relay(source()))\n")
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    helper.write_text(edit + body)
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused
    assert evidence(result) == evidence(analyze(client, rules, "entry", imports=True))
    assert result.report.at("/complete") is (edit == "__version__ = '9'\n")


def test_metadata_binding_cannot_be_an_external_rule(tmp_path):
    client, rules = workspace(tmp_path)
    rules.write_text('{"version":"metadata-test","sources":["__version__"],"sinks":["sink"]}')
    (tmp_path / "subject.py").write_text("__version__ = '8'\ndef entry():\n    return sink(__version__())\n")
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry")


@pytest.mark.parametrize("prefix", [
    "__version__ = '8'\ndef __version__(value):\n    return value\n",
    "def __version__(value):\n    return value\n__version__ = '8'\n",
])
@pytest.mark.parametrize("imports", [False, True])
def test_metadata_function_collisions_remain_incomplete(tmp_path, prefix, imports):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(prefix + "def entry():\n    return sink(__version__(source()))\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("literal", LITERALS)
@pytest.mark.parametrize("imports", [False, True])
def test_unused_distinct_scalar_bindings_do_not_poison_initialization(tmp_path, literal, imports):
    client, rules = workspace(tmp_path)
    prefix = "metadata = " + literal + "\n"
    helper = "def relay(value):\n    return value\n"
    if imports:
        package = tmp_path / "library"
        package.mkdir()
        (package / "__init__.py").write_text(prefix + "from .core import relay\n")
        (package / "core.py").write_text("other = 0\n" + helper)
        prefix = "from library import relay\n"
    else:
        prefix += helper
    for selected, reaches in [("source()", True), ("0", False)]:
        (tmp_path / "subject.py").write_text(prefix + "def entry():\n    return sink(relay(" + selected + "))\n")
        result = analyze(client, rules, "entry", imports=imports)
        assert result.report.at("/complete"), result.report.at("/cutoffs")
        assert bool(result.witnesses) is reaches
        assert result.summaries.call_binding.initialization
        assert observe(tmp_path) == {"result": 17 if reaches else 0, "raised": None,
                                    "events": [["source", 17], ["sink", 17]] if reaches else [["sink", 0]]}


@pytest.mark.parametrize("binding", [
    "metadata = 1 + 2",
    "metadata = f'constant'", "metadata = f'{source()}'", "metadata = source()",
    "metadata: int = 0", "metadata = other = 0", "metadata, other = 0, 1",
    "metadata = 0\nmetadata = 1", "__path__ = 'elsewhere'", "méta = 0",
])
def test_evaluated_special_or_repeated_bindings_stay_incomplete(tmp_path, binding):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(binding + "\ndef entry():\n    return sink(source())\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("prefix", [
    "def relay(value):\n    return value\nrelay = 0\n",
    "relay = 0\ndef relay(value):\n    return value\n",
    "import helper as relay\nrelay = 0\n",
    "relay = 0\nimport helper as relay\n",
])
@pytest.mark.parametrize("imports", [False, True])
def test_literal_function_and_import_collisions_cannot_establish_absence(tmp_path, prefix, imports):
    client, rules = workspace(tmp_path)
    (tmp_path / "helper.py").write_text("def identity(value):\n    return value\n")
    (tmp_path / "subject.py").write_text(prefix + "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


def test_a_literal_package_member_cannot_be_replaced_by_a_child_module(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("core = 0\n")
    (package / "core.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text("from library import core\ndef entry():\n    return sink(core.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    lookup = next(item for item in result.dependencies.lookups if item.alias == "core")
    assert lookup.submodule is None and lookup.resolution == "unavailable"
    assert observe(tmp_path)["raised"] == "AttributeError"


def test_loading_a_child_module_cannot_hide_a_literal_package_binding(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("core = 0\n")
    (package / "core.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text("import library.core\ndef entry():\n    return sink(library.core.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "package-child-binding-conflict" in result.report.at("/cutoffs")


def test_global_literal_reads_remain_an_explicit_boundary(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("metadata = 5\ndef entry():\n    return sink(metadata)\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.witnesses
    assert "unknown-binding:metadata" in result.report.at("/cutoffs")
    assert "module-effects-unchecked" not in result.report.at("/cutoffs")
    assert observe(tmp_path)["events"] == [["sink", 5]]


def test_a_literal_binding_cannot_be_declared_as_an_external_rule(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("source = 0\ndef entry():\n    return sink(source())\n")
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry")
    assert observe(tmp_path)["raised"] == "TypeError"


@pytest.mark.parametrize("edit", ["metadata = 1\n", "metadata = source()\n", "metadata = 0\nmetadata = 1\n"])
def test_initialization_changes_invalidate_reuse_and_match_clean_analysis(tmp_path, edit):
    client, rules = workspace(tmp_path)
    path = tmp_path / "subject.py"
    body = "def entry():\n    return sink(source())\n"
    path.write_text("metadata = 0\n" + body)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache).reused
    path.write_text(edit + body)
    result = analyze(client, rules, "entry", cache)
    assert not result.reused and result.report.at("/input_digest") != first.report.at("/input_digest")
    assert evidence(result) == evidence(analyze(client, rules, "entry"))
    assert result.report.at("/complete") is (edit == "metadata = 1\n")


@pytest.mark.parametrize("mutation", ["missing", "inputs", "description"])
def test_current_initialization_contract_is_required(tmp_path, mutation):
    client, rules = workspace(tmp_path)
    data = copy.deepcopy(analyze(client, rules).report.to_data())
    if mutation == "missing":
        del data["call_binding"]["initialization"]
    elif mutation == "inputs":
        del data["inputs"]["call_binding"]["initialization"]
    else:
        data["call_binding"]["initialization"] = "replace imported functions with literals"
        data["inputs"]["call_binding"]["initialization"] = data["call_binding"]["initialization"]
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
