"""Compare plain class creation, constructor boundaries and saved answers with Python."""
import copy
import hashlib
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace


def install(tmp_path, definition, instance="", imports=False, value="source()"):
    client, rules = workspace(tmp_path)
    prelude = definition + "\n" + instance + "\n"
    helper = "def relay(value):\n    return value\n"
    if imports:
        (tmp_path / "helpers.py").write_text(prelude + helper)
        prelude = "from helpers import relay\n"
    else:
        prelude += helper
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(relay(" + value + "))\n")
    return client, rules


def observe(tmp_path):
    script = '''import builtins, json, runpy, sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]).parent))
events = []
def source():
    events.append(['source', 17]); return 17
def sink(value):
    events.append(['sink', value]); return value
builtins.source, builtins.sink = source, sink
try:
    namespace = runpy.run_path(sys.argv[1], init_globals=dict(source=source, sink=sink))
    result, raised = namespace['entry'](), None
except Exception as error:
    result, raised = None, type(error).__name__
print(json.dumps(dict(result=result, raised=raised, events=events)))
'''
    process = subprocess.run([sys.executable, "-B", "-c", script, str(tmp_path / "subject.py")],
                             check=True, capture_output=True, text=True, timeout=30)
    return json.loads(process.stdout)


PLAIN = [
    ("class Token:\n    pass", ""),
    ("class Token:\n    pass", "marker = Token()"),
    ("class Token():\n    code = ((0))\n    def unused(self, *values):\n        return values", "marker = Token( # no arguments\n)"),
    ("class Token:\n    def __repr__(self):\n        return str(source())", "marker = Token()"),
    ("class Token:\n    def __init__(self):\n        sink(source())", ""),
    ("class Token:\n    def __new__(cls):\n        return source()", ""),
    ("class Token:\n    def __del__(self):\n        sink(source())", ""),
]


@pytest.mark.parametrize("definition,instance", PLAIN)
@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("value,reaches", [("source()", True), ("0", False)])
def test_plain_declarations_and_default_instances_preserve_scalar_answers(tmp_path, definition, instance, imports, value, reaches):
    client, rules = install(tmp_path, definition, instance, imports, value)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is reaches
    assert observe(tmp_path) == {"result": 17 if reaches else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17]] if reaches else [["sink", 0]]}


UNCHECKED = [
    "class Token(object):\n    pass",
    "class Token(metaclass=type):\n    pass",
    "def decorate(cls):\n    return cls\n@decorate\nclass Token:\n    pass",
    "class Token:\n    @staticmethod\n    def unused():\n        pass",
    "class Token:\n    def unused(self, value: int):\n        return value",
    "class Token:\n    def unused(self, value=source()):\n        return value",
    "class Token:\n    values = []",
    "class Token:\n    value: int = 0",
    "class Token:\n    sink(source())",
    "class Token:\n    class Nested:\n        pass",
    "class Token:\n    def __qualname__(self):\n        pass",
    "class Token:\n    def __slots__(self):\n        pass",
    "class Token:\n    def méta(self):\n        pass",
    "class Token[T]:\n    pass",
]


@pytest.mark.parametrize("definition", UNCHECKED)
@pytest.mark.parametrize("imports", [False, True])
def test_class_headers_and_executed_bodies_remain_unchecked(tmp_path, definition, imports):
    client, rules = install(tmp_path, definition, imports=imports, value="0")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("method,body", [("__init__", "self.value = 0"),
                                         ("__new__", "return object.__new__(self)"),
                                         ("__del__", "pass")])
@pytest.mark.parametrize("imports", [False, True])
def test_custom_construction_and_finalization_prevent_an_inert_instance_claim(tmp_path, method, body, imports):
    client, rules = install(tmp_path, "class Token:\n    def " + method + "(self):\n        " + body,
                            "marker = Token()", imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("arguments", ["0", "flag=1", "*[]", "**{}"])
def test_only_direct_calls_without_arguments_are_admitted(tmp_path, arguments):
    client, rules = install(tmp_path, "class Token:\n    pass", "marker = Token(" + arguments + ")")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    actual = observe(tmp_path)
    assert actual["raised"] == (None if arguments in {"*[]", "**{}"} else "TypeError")


@pytest.mark.parametrize("prefix", ["Token = 0", "class Token:\n    pass", "def Token():\n    return 0"])
def test_rebinding_cannot_preserve_default_construction(tmp_path, prefix):
    client, rules = install(tmp_path, "class Token:\n    pass\n" + prefix, "marker = Token()")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


def test_descriptor_installation_keeps_import_time_calls_visible(tmp_path):
    definition = "class Descriptor:\n    def __set_name__(self, owner, name):\n        sink(source())\ndescriptor = Descriptor()\nclass Owner:\n    field = descriptor"
    client, rules = install(tmp_path, definition, value="0")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete") and not result.witnesses
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert observe(tmp_path) == {"result": 0, "raised": None, "events": [["source", 17], ["sink", 17], ["sink", 0]]}


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("edit,complete", [("class Token:\n    def __repr__(self):\n        return 'changed'", True),
                                           ("class Token:\n    def __init__(self):\n        pass", False)])
def test_method_and_constructor_edits_invalidate_cached_answers(tmp_path, imports, edit, complete):
    client, rules = install(tmp_path, "class Token:\n    pass", "marker = Token()", imports)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=imports)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=imports).reused
    path = tmp_path / ("helpers.py" if imports else "subject.py")
    path.write_text(path.read_text().replace("class Token:\n    pass", edit))
    changed = analyze(client, rules, "entry", cache, imports=imports)
    assert not changed.reused and changed.report.at("/input_digest") != first.report.at("/input_digest")
    assert changed.report.at("/complete") is complete
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=imports))


def test_an_instance_value_is_still_an_unknown_global_read(tmp_path):
    client, rules = install(tmp_path, "class Token:\n    pass", "marker = Token()", value="marker is None")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "unknown-binding:marker" in result.report.at("/cutoffs")
    assert "module-effects-unchecked" not in result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] is False


@pytest.mark.parametrize("definition,instance", [("class source:\n    pass", ""),
                                                ("class Token:\n    pass", "source = Token()")])
def test_class_and_instance_bindings_cannot_be_claimed_as_external_rules(tmp_path, definition, instance):
    client, rules = install(tmp_path, definition, instance, value="0")
    with pytest.raises(FrRuntimeError, match="rule overlaps"):
        analyze(client, rules, "entry")
    assert observe(tmp_path)["result"] == 0


@pytest.mark.parametrize("kind", ["class", "instance"])
def test_package_objects_keep_precedence_over_same_named_files(tmp_path, kind):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    declaration = "class channel:\n    pass\n" if kind == "class" else "class Token:\n    pass\nchannel = Token()\n"
    (package / "__init__.py").write_text(declaration)
    (package / "channel.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text("from library import channel\ndef entry():\n    return sink(channel.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" not in result.report.at("/cutoffs")
    lookup = next(item for item in result.dependencies.lookups if item.alias == "channel")
    assert lookup.submodule is None and lookup.resolution == "unavailable"
    assert observe(tmp_path)["raised"] == "AttributeError"


def test_previous_initialization_description_is_not_accepted(tmp_path):
    client, rules = install(tmp_path, "class Token:\n    pass")
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    old = "Unique ASCII scalar bindings and literal __all__ declarations are inert; collisions and effects remain unchecked. No global reads or imported literals."
    data["call_binding"]["initialization"] = old
    data["inputs"]["call_binding"]["initialization"] = old
    encoded = json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()
    data["input_digest"] = hashlib.sha256(encoded).hexdigest()
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
