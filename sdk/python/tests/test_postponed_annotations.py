"""Compare explicit postponed annotations with Python and reject stale imports."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace


FUTURE = "from __future__ import annotations\n"


def install(tmp_path, helper, value="source()", entry_prefix=""):
    client, rules = workspace(tmp_path)
    (tmp_path / "helpers.py").write_text(helper)
    (tmp_path / "subject.py").write_text(entry_prefix + "from helpers import relay\ndef entry():\n    return sink(relay(" + value + "))\n")
    return client, rules


def observe(tmp_path):
    script = '''import builtins, json, sys
from pathlib import Path
root = Path(sys.argv[1])
sys.path.insert(0, str(root))
sys.modules.pop('__future__', None)
events = []
def source():
    events.append(['source', 17]); return 17
def sink(value):
    events.append(['sink', value]); return value
builtins.source, builtins.sink = source, sink
try:
    namespace = {}
    exec(compile((root / 'subject.py').read_text(), 'subject.py', 'exec', dont_inherit=True), namespace)
    result, raised = namespace['entry'](), None
except Exception as error:
    result, raised = None, type(error).__name__
print(json.dumps(dict(result=result, raised=raised, events=events)))
'''
    process = subprocess.run([sys.executable, "-I", "-B", "-c", script, str(tmp_path)],
                             check=True, capture_output=True, text=True, timeout=30)
    return json.loads(process.stdout)


@pytest.mark.parametrize("signature", [
    "value: Missing",
    "value: source()",
    "value: Missing, /, *, flag: Missing = 0",
    "value: Missing = 0",
    "value: Missing, *unused: Missing, **also_unused: Missing",
])
@pytest.mark.parametrize("value,reaches", [("source()", True), ("0", False)])
def test_annotations_are_not_executed_and_fixed_slots_keep_their_flow(tmp_path, signature, value, reaches):
    client, rules = install(tmp_path, FUTURE + "def relay(" + signature + ") -> source():\n    return value\n", value)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is reaches
    assert not result.initialization.sites
    summary = result.summaries.for_function("helpers.py::relay")
    assert summary.signature[0].name == "value"
    lookup = next(item for item in result.dependencies.lookups if item.future)
    assert lookup.importer == "helpers.py" and lookup.resolution == "future"
    assert {item.path for item in lookup.candidates} == {
        "__future__.py", "__future__/__init__.py", "__future__.pyi", "__future__/__init__.pyi", "__future__"}
    assert all(item.status == "missing" for item in lookup.candidates)
    assert observe(tmp_path) == {"result": 17 if reaches else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17]] if reaches else [["sink", 0]]}


@pytest.mark.parametrize("prefix", [FUTURE, '# comment\n"module docs"\n' + FUTURE,
                                     "from __future__ import ( # comment\n    annotations,\n)\n",
                                     "from __future__ import " + "\\" + "\n    annotations\n"])
def test_valid_directive_positions_and_spelling(tmp_path, prefix):
    client, rules = install(tmp_path, prefix + "def relay(value: Missing):\n    return value\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("prefix", [
    "metadata = 0\n" + FUTURE,
    '"first"\n"second"\n' + FUTURE,
    'b"bytes are not a docstring"\n' + FUTURE,
    "from __future__ import annotations as delayed\n",
    "from __future__ import annotations, division\n",
    "from __future__ import unknown_feature\n" + FUTURE,
    "import __future__\n",
])
def test_unsupported_or_misplaced_directives_do_not_authorize_annotations(tmp_path, prefix):
    client, rules = install(tmp_path, prefix + "def relay(value: Missing):\n    return value\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    assert not any(item.future for item in result.dependencies.lookups)


def test_directives_belong_to_the_defining_module(tmp_path):
    client, rules = install(tmp_path, "def relay(value: Missing):\n    return value\n", entry_prefix=FUTURE)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert any(site.occurrence.path == "helpers.py" for site in result.initialization.sites)
    assert observe(tmp_path)["raised"] == "NameError"


def test_single_file_mode_cannot_skip_import_candidate_checks(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text(FUTURE + "def entry(value: Missing = 0):\n    return sink(value)\n")
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


@pytest.mark.parametrize("extra", [
    "def unused(value: Missing = source()):\n    return value\n",
    "def decorate(fn):\n    return fn\n@decorate\ndef unused(value: Missing):\n    return value\n",
    "class Token:\n    value: Missing = 0\n",
    "class Token:\n    def __init__(self, value: Missing = 0):\n        sink(source())\nmarker = Token()\n",
    "def generic[T](value: T):\n    return value\n",
])
def test_postponement_does_not_admit_other_definition_effects(tmp_path, extra):
    client, rules = install(tmp_path, FUTURE + extra + "def relay(value: Missing):\n    return value\n", value="0")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")


def test_postponed_methods_are_deferred_in_plain_classes(tmp_path):
    client, rules = install(tmp_path, FUTURE + "class Token:\n    def unused(self, value: Missing) -> source():\n        return value\nmarker = Token()\ndef relay(value: Missing):\n    return value\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


@pytest.mark.parametrize("path", ["__future__.py", "__future__/__init__.py", "__future__.pyi",
                                   "__future__/__init__.pyi", "__future__"])
def test_local_candidates_prevent_standard_library_admission(tmp_path, path):
    client, rules = install(tmp_path, FUTURE + "def relay(value: Missing):\n    return value\n", value="0")
    shadow = tmp_path / path
    shadow.parent.mkdir(parents=True, exist_ok=True)
    if path == "__future__":
        shadow.mkdir()
    else:
        shadow.write_text("annotations = 0\nsink(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert not any(item.future for item in result.dependencies.lookups)
    if path.endswith(".py"):
        assert path in dict(result.dependencies.files)
        assert any(site.occurrence.path == path for site in result.initialization.sites)
        assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["sink", 0]]


def test_adding_and_removing_a_local_shadow_invalidates_a_saved_answer(tmp_path):
    client, rules = install(tmp_path, FUTURE + "def relay(value: Missing):\n    return value\n", value="0")
    cache = FlowCache(MemoryObjectStore())
    original = analyze(client, rules, "entry", cache, imports=True)
    assert original.report.at("/complete")
    shadow = tmp_path / "__future__.py"
    shadow.write_text("annotations = 0\nsink(source())\n")
    blocked = analyze(client, rules, "entry", cache, imports=True)
    assert not blocked.reused and not blocked.report.at("/complete")
    assert evidence(blocked) == evidence(analyze(client, rules, "entry", imports=True))
    shadow.unlink()
    restored = analyze(client, rules, "entry", cache, imports=True)
    assert restored.report.at("/complete")
    assert evidence(restored) == evidence(analyze(client, rules, "entry", imports=True))
    (tmp_path / "helpers.py").write_text("def relay(value: Missing):\n    return value\n")
    edited = analyze(client, rules, "entry", cache, imports=True)
    assert not edited.reused and not edited.report.at("/complete")


@pytest.mark.parametrize("body", ["return annotations", "return annotations()", "return relay.__annotations__"])
def test_annotation_inspection_and_feature_values_remain_unchecked(tmp_path, body):
    client, rules = install(tmp_path, FUTURE + "def relay(value: Missing):\n    " + body + "\n", value="0")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")


@pytest.mark.parametrize("mutation", ["missing", "boolean", "ordinary", "module", "alias", "member", "prefix", "candidate", "chain", "submodule", "terminal", "star", "exports"])
def test_rehashed_future_lookup_corruption_is_rejected(tmp_path, mutation):
    client, rules = install(tmp_path, FUTURE + "def relay(value: Missing):\n    return value\n")
    data = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    lookup = next(item for item in data["inputs"]["modules"]["lookups"] if item["future"])
    if mutation == "missing":
        del lookup["future"]
    elif mutation == "boolean":
        lookup["future"] = 1
    elif mutation == "ordinary":
        lookup["future"] = False
    elif mutation == "module":
        lookup["module"] = "missing"
    elif mutation in {"alias", "member", "prefix"}:
        lookup[mutation] = "other"
    elif mutation == "candidate":
        lookup["candidates"]["__future__.py"]["status"] = "symlink"
    elif mutation == "chain":
        lookup["binding_chain"] = [{"path": "helpers.py", "name": "relay"}]
    elif mutation == "submodule":
        lookup["submodule"] = copy.deepcopy(data["inputs"]["modules"]["entry"])
    elif mutation == "terminal":
        lookup["terminal_module"] = "helpers.py"
    elif mutation == "star":
        lookup["star"] = 0
    else:
        lookup["exports"] = []
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    for reader in (FlowDependencies.from_report, FunctionSummaries.from_report):
        with pytest.raises(FrRuntimeError):
            reader(FrReport(data, ()))


def test_native_reuse_rejects_the_previous_call_contract(tmp_path):
    client, rules = install(tmp_path, FUTURE + "def relay(value: Missing):\n    return value\n")
    data = analyze(client, rules, "entry", imports=True).report.to_data()
    for binding in (data["call_binding"], data["inputs"]["call_binding"]):
        binding["definitions"] = "Unannotated ASCII signatures with immutable literal defaults defer bodies, including referenced variadics. Calls require scalar fixed slots."
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-future-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
