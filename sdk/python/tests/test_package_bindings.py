"""Compare package member selection with CPython, including cache edits."""
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from test_flow_summaries import analyze, evidence, workspace


BINDINGS = [
    "class core:\n    @staticmethod\n    def relay(value):\n        return 0\n",
    "def replace(function):\n    return object()\n@replace\ndef core(value):\n    return value\n",
    "def replace(cls):\n    return object()\n@replace\nclass core:\n    pass\n",
    "core = object()\n",
    "core: object = object()\n",
    "other = core = object()\n",
    "core, other = object(), 0\n",
    "[core, other] = [object(), 0]\n",
    "core, *other = [object()]\n",
]


def install(tmp_path, initializer):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir(exist_ok=True)
    (package / "__init__.py").write_text(initializer)
    (package / "core.py").write_text("def relay(value):\n    return value\n")
    (tmp_path / "subject.py").write_text(
        "from library import core\ndef entry():\n    return sink(core.relay(source()))\n")
    return client, rules


def runtime(tmp_path):
    script = '''import json, runpy, sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]).parent))
events = []
def source(): events.append(["source", 17]); return 17
def sink(value): events.append(["sink", value]); return value
namespace = runpy.run_path(sys.argv[1], init_globals=dict(source=source, sink=sink))
try:
    result = namespace["entry"]()
    raised = None
except Exception as error:
    result, raised = None, type(error).__name__
print(json.dumps(dict(result=result, raised=raised, events=events,
                     child_loaded="library.core" in sys.modules)))
'''
    output = subprocess.run([sys.executable, "-B", "-c", script,
                             str(tmp_path / "subject.py")], capture_output=True,
                            text=True, check=True, timeout=30)
    return json.loads(output.stdout)


@pytest.mark.parametrize("binding", BINDINGS)
@pytest.mark.parametrize("child_exists", [True, False])
def test_existing_package_members_never_select_a_same_named_file(tmp_path, binding, child_exists):
    client, rules = install(tmp_path, binding)
    if not child_exists:
        (tmp_path / "library/core.py").unlink()
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    lookup = next(item for item in result.dependencies.lookups if item.alias == "core")
    assert lookup.submodule is None and lookup.resolution == "unavailable"
    assert "library/core.py" not in result.report.at("/inputs/modules/files")
    assert "unknown-external-call:core.relay" in result.report.at("/cutoffs")
    observed = runtime(tmp_path)
    assert observed["child_loaded"] is False
    if binding.startswith("class core:"):
        assert observed["result"] == 0 and observed["events"] == [["source", 17], ["sink", 0]]
    else:
        assert observed["raised"] == "AttributeError" and not observed["events"]


def test_an_annotation_without_a_value_does_not_create_a_package_member(tmp_path):
    client, rules = install(tmp_path, "core: object\n")
    result = analyze(client, rules, "entry", imports=True)
    lookup = next(item for item in result.dependencies.lookups if item.alias == "core")
    assert lookup.submodule.target == "library/core.py" and lookup.resolution == "module"
    assert not result.report.at("/complete")  # The annotation still executes at import.
    assert runtime(tmp_path) == {"result": 17, "raised": None,
                                 "events": [["source", 17], ["sink", 17]], "child_loaded": True}


@pytest.mark.parametrize("initializer,complete", [
    ("def unused():\n    core = object()\n", True),
    ("def unused():\n    class core:\n        pass\n", True),
    ("class Owner:\n    core = object()\n", False),
])
def test_inner_scope_bindings_do_not_hide_a_package_child(tmp_path, initializer, complete):
    client, rules = install(tmp_path, initializer)
    result = analyze(client, rules, "entry", imports=True)
    lookup = next(item for item in result.dependencies.lookups if item.alias == "core")
    assert lookup.submodule.target == "library/core.py" and lookup.resolution == "module"
    assert result.report.at("/complete") is complete
    assert runtime(tmp_path)["result"] == 17


@pytest.mark.parametrize("binding", BINDINGS)
def test_adding_and_removing_a_package_binding_invalidates_saved_resolution(tmp_path, binding):
    client, rules = install(tmp_path, "")
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    initializer = tmp_path / "library/__init__.py"
    initializer.write_text(binding)
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and not changed.report.at("/complete")
    assert changed.report.at("/input_digest") != first.report.at("/input_digest")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    initializer.write_text("")
    restored = analyze(client, rules, "entry", cache, imports=True)
    assert restored.report.at("/complete")
    assert evidence(restored) == evidence(analyze(client, rules, "entry", imports=True))


@pytest.mark.parametrize("binding", BINDINGS)
def test_explicit_child_loading_reports_the_conflicting_package_member(tmp_path, binding):
    client, rules = install(tmp_path, binding)
    (tmp_path / "subject.py").write_text(
        "import library.core\ndef entry():\n    return sink(library.core.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "package-child-binding-conflict" in result.report.at("/cutoffs")
    # CPython loads the explicit child and replaces the earlier attribute.
    assert runtime(tmp_path)["result"] == 17
