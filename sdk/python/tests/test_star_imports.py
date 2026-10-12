"""Check declared star exports against CPython and strict dependency records."""
import copy
import hashlib
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_call_binding import observe
from test_flow_summaries import analyze, evidence, workspace


def install(tmp_path, exports="['relay']", body="return value", prelude=""):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir(exist_ok=True)
    declaration = "" if exports is None else "__all__ = " + exports + "\n"
    (package / "leaf.py").write_text(declaration + prelude + "def relay(value):\n    " + body + "\n")
    (package / "__init__.py").write_text("__all__ = ['relay']\nfrom .leaf import *\n")
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(relay(source()))\n")
    return client, rules


@pytest.mark.parametrize("exports", ["['relay']", "('relay',)", "['relay', 'relay']"])
@pytest.mark.parametrize("body,selected", [("return value", True), ("return 0", False)])
def test_declared_names_follow_star_reexports_and_exact_runtime_values(tmp_path, exports, body, selected):
    client, rules = install(tmp_path, exports, body)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is selected
    assert result.summaries.for_function("library/leaf.py::relay").return_parameters == ((0,) if selected else ())
    stars = [lookup for lookup in result.dependencies.lookups if lookup.alias == "*"]
    assert len(stars) == 2 and all(lookup.exports == ("relay",) for lookup in stars)
    assert observe(tmp_path) == {"result": 17 if selected else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if selected else 0]]}


@pytest.mark.parametrize("exports", ["[]", "()"])
def test_empty_exports_still_track_the_imported_module(tmp_path, exports):
    client, rules = install(tmp_path, exports)
    (tmp_path / "library/__init__.py").write_text("__all__ = []\nfrom .leaf import *\n")
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete")
    assert all(lookup.exports == () for lookup in result.dependencies.lookups)
    assert "library/leaf.py" in dict(result.dependencies.files)
    assert observe(tmp_path)["result"] == 17


def test_explicit_exports_include_private_names_and_exclude_unlisted_names(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / "library/leaf.py").write_text("__all__ = ['_relay']\ndef _relay(value):\n    return value\ndef hidden(value):\n    return 0\n")
    (tmp_path / "library/__init__.py").write_text("__all__ = ['_relay']\nfrom .leaf import *\n")
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(_relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and observe(tmp_path)["result"] == 17
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(hidden(source()))\n")
    changed = analyze(client, rules, "entry", imports=True)
    assert not changed.report.at("/complete") and observe(tmp_path)["raised"] == "NameError"


def test_a_listed_missing_name_keeps_the_answer_incomplete(tmp_path):
    client, rules = install(tmp_path, "['missing']")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "missing-or-ambiguous-import-member:missing" in result.report.at("/cutoffs")
    script = "import sys\nsys.path.insert(0, sys.argv[1])\ntry:\n import subject\nexcept Exception as error:\n print(type(error).__name__)\n"
    actual = subprocess.run([sys.executable, "-B", "-c", script, str(tmp_path)], capture_output=True,
                            text=True, check=True, timeout=30)
    assert actual.stdout.strip() == "AttributeError"


@pytest.mark.parametrize("exports,prelude", [
    (None, ""), ("build()", "def build():\n    return ['relay']\n"),
    ("['relay']", "__all__.append('hidden')\ndef hidden(value):\n    return value\n"),
    ("['relay']", "copy = __all__\n"), ("['relay']", "__all__ = []\n"),
    ("[b'relay']", ""), ("[f'relay']", ""), ("['re\\u006cay']", ""),
    ("['relay'] + []", ""), ("[name for name in ['relay']]", ""),
])
def test_unchecked_export_sets_never_establish_absence(tmp_path, exports, prelude):
    client, rules = install(tmp_path, exports, prelude=prelude)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "star-exports-unchecked" in result.report.at("/cutoffs")
    assert any(lookup.alias == "*" and lookup.exports is None for lookup in result.dependencies.lookups)
    if exports is None:
        assert observe(tmp_path)["result"] == 17


def test_large_export_sets_stop_at_the_existing_import_budget(tmp_path):
    names = [f"helper{index}" for index in range(128)]
    client, rules = install(tmp_path)
    (tmp_path / "library/leaf.py").write_text("__all__ = " + repr(names) + "\n" +
        "".join(f"def {name}(value):\n    return value\n" for name in names))
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "star-exports-unchecked" in result.report.at("/cutoffs")
    assert result.report.at("/inputs/modules/limits/imports") == 128
    assert len(result.dependencies.lookups) <= 128


@pytest.mark.parametrize("change", ["exports", "body", "mutate"])
def test_export_and_helper_edits_invalidate_saved_answers(tmp_path, change):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    leaf = tmp_path / "library/leaf.py"
    text = leaf.read_text()
    if change == "exports":
        text = text.replace("['relay']", "[]")
    elif change == "body":
        text = text.replace("return value", "return 0")
    else:
        text += "__all__.clear()\n"
    leaf.write_text(text)
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and changed.report.at("/input_digest") != first.report.at("/input_digest")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    if change == "body":
        assert changed.report.at("/complete") and not changed.witnesses
    else:
        assert not changed.report.at("/complete")


def test_package_exported_children_preserve_existing_objects(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / "library/__init__.py").write_text("__all__ = ['leaf']\n")
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(leaf.relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and observe(tmp_path)["result"] == 17
    (tmp_path / "library/__init__.py").write_text("__all__ = ['leaf']\nclass leaf:\n    pass\n")
    changed = analyze(client, rules, "entry", imports=True)
    assert not changed.report.at("/complete") and observe(tmp_path)["raised"] == "AttributeError"
    lookup = next(item for item in changed.dependencies.lookups if item.alias == "leaf")
    assert lookup.submodule is None and lookup.resolution == "unavailable"


@pytest.mark.parametrize("prefix", ["relay = 0\n", "from other import relay\n", "def relay(value):\n    return 0\n"])
@pytest.mark.parametrize("star_last", [True, False])
def test_star_and_local_binding_collisions_remain_explicit(tmp_path, prefix, star_last):
    client, rules = install(tmp_path)
    (tmp_path / "other.py").write_text("def relay(value):\n    return 0\n")
    star = "from library import *\n"
    source = prefix + star if star_last else star + prefix
    (tmp_path / "subject.py").write_text(source + "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "ambiguous-module-binding" in result.report.at("/cutoffs")


@pytest.mark.parametrize("mutation", ["missing-star", "missing-exports", "omit", "extra", "link", "repeat"])
def test_rehashed_star_records_cannot_drop_or_invent_expansion_members(tmp_path, mutation):
    client, rules = install(tmp_path)
    data = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    lookups = data["inputs"]["modules"]["lookups"]
    anchor = next(item for item in lookups if item["alias"] == "*")
    member = next(item for item in lookups if item["star"] == 0)
    if mutation == "missing-star":
        del member["star"]
    elif mutation == "missing-exports":
        del anchor["exports"]
    elif mutation == "omit":
        lookups.remove(member)
    elif mutation == "extra":
        anchor["exports"].append("invented")
    elif mutation == "link":
        member["star"] = True
    else:
        anchor["exports"].append("relay")
    encoded = json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()
    data["input_digest"] = hashlib.sha256(encoded).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(data, ()))
