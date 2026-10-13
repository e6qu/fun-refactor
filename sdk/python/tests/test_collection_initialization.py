"""Check literal allocations separately from global reads and mutations."""
import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.runtime import FrRuntimeError
from test_postponed_annotations import observe
from test_flow_summaries import analyze, evidence, workspace


COLLECTIONS = [
    "[]", "{}", "()", "(0,)", "0, 1", "((([])))",
    "(-1, +2.5, 2j, r'raw')", "{0, 1, False}",
    "{(1, 'café'), (None, -2.5)}", "{0: [], 'x': {}, None: (True,)}",
    "({(0, ('x',)): [[None]], b'bytes': {1, 2}})",
    "[(1,), {}, {('x', 2)}]", "[ # comment\n 0,\n # comment\n [None],\n]",
    "(('café', None, -2.5), [True, b'bytes'], {'x': [0]})",
]


def install(tmp_path, initialization, imports, forwards=True):
    client, rules = workspace(tmp_path)
    helper = initialization + "\ndef relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    if imports:
        package = tmp_path / "library"
        package.mkdir(exist_ok=True)
        (package / "__init__.py").write_text(helper)
        helper = "from library import relay\n"
    (tmp_path / "subject.py").write_text(helper + "def entry():\n    return sink(relay(source()))\n")
    return client, rules


@pytest.mark.parametrize("literal", COLLECTIONS)
@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("forwards", [False, True])
def test_unused_literal_collections_preserve_helper_behavior(tmp_path, literal, imports, forwards):
    client, rules = install(tmp_path, "metadata = " + literal, imports, forwards)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards
    assert not result.initialization.sites
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}


@pytest.mark.parametrize("literal", [
    "[source()]", "{'x': source()}", "{source(): 0}", "{source()}",
    "[(0, [source()])]", "[missing]", "[(lambda: source())]", "[1 + 2]",
    "[x for x in []]", "[source() for x in []]", "[*(0, 1)]", "{**{0: 1}}",
    "(*(),)", "{*(0, 1)}", "[f'constant']", "[f'{source()}']",
    "{[]: 0}", "{([0],): 0}", "{{0}: 1}", "{[]}", "{([],)}",
])
@pytest.mark.parametrize("imports", [False, True])
def test_computed_unpacked_and_unhashable_collections_remain_incomplete(tmp_path, literal, imports):
    client, rules = install(tmp_path, "metadata = " + literal, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert result.initialization.sites


@pytest.mark.parametrize("imports", [False, True])
def test_user_hash_code_cannot_be_hidden_as_literal_initialization(tmp_path, imports):
    client, rules = install(tmp_path, "class Key:\n    def __hash__(self):\n        sink(source())\n        return 0\nmetadata = {Key(): 0}", imports, False)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["source", 17], ["sink", 0]]


@pytest.mark.parametrize("count,complete", [(127, True), (128, False)])
@pytest.mark.parametrize("imports", [False, True])
def test_collection_syntax_node_budget_is_enforced(tmp_path, count, complete, imports):
    client, rules = install(tmp_path, "metadata = [" + ",".join("0" for _ in range(count)) + "]", imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") is complete
    assert bool(result.initialization.sites) is (not complete)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("extra", [
    "metadata.append(source())", "metadata[0] = source()", "metadata = {}",
    "metadata: list = []", "other = metadata = []",
    "class Token:\n    metadata = []", "def unused(value=[]):\n    return value",
    "__version__ = []", "__path__ = []", "méta = []",
])
@pytest.mark.parametrize("imports", [False, True])
def test_collection_allocation_does_not_admit_other_module_effects(tmp_path, extra, imports):
    client, rules = install(tmp_path, "metadata = []\n" + extra, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("expression,cutoff,result_value", [
    ("metadata", "unknown-binding:metadata", [17]),
    ("metadata[0]", "unsupported-expression:subscript", 17),
])
def test_global_collection_reads_remain_incomplete(tmp_path, expression, cutoff, result_value):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("metadata = [17]\ndef entry():\n    return sink(" + expression + ")\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.initialization.sites
    assert cutoff in result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == result_value


@pytest.mark.parametrize("prefix", [
    "relay = []\ndef relay(value):\n    return value",
    "def relay(value):\n    return value\nrelay = {}",
])
@pytest.mark.parametrize("imports", [False, True])
def test_collection_function_collisions_cannot_establish_absence(tmp_path, prefix, imports):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(prefix + "\ndef entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


def test_collection_rule_overlap_is_rejected(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text("source = []\ndef entry():\n    return sink(source())\n")
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry")


@pytest.mark.parametrize("edit,complete", [("metadata = (0,)\n", True),
    ("metadata = [source()]\n", False), ("metadata = []\nmetadata.append(source())\n", False)])
def test_collection_edits_invalidate_saved_answers(tmp_path, edit, complete):
    client, rules = install(tmp_path, "metadata = []", True)
    path = tmp_path / "library/__init__.py"
    body = "def relay(value):\n    return value\n"
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    path.write_text(edit + body)
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused and result.report.at("/complete") is complete
    assert evidence(result) == evidence(analyze(client, rules, "entry", imports=True))


def test_collection_package_binding_precedes_a_same_named_file(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    (package / "__init__.py").write_text("core = {}\n")
    (package / "core.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text("from library import core\ndef entry():\n    return sink(core.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and not result.initialization.sites
    lookup = next(item for item in result.dependencies.lookups if item.alias == "core")
    assert lookup.submodule is None
    assert observe(tmp_path)["raised"] == "AttributeError"
