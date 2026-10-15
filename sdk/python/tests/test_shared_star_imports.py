"""Compare shared wildcard searches, startup effects and strict records with Python."""
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


def install(tmp_path, count=128, route="direct", forwards=True, startup=""):
    client, rules = workspace(tmp_path)
    names = ["relay", *[f"helper{index}" for index in range(count - 1)]]
    helpers = "__all__ = " + repr(names) + "\n" + startup
    helpers += "def relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    helpers += "".join(f"def {name}(value):\n    return value\n" for name in names[1:])
    (tmp_path / "helpers.py").write_text(helpers)
    prelude, call = "from helpers import *\n", "relay(source())"
    if route == "reexport":
        (tmp_path / "bridge.py").write_text("__all__ = " + repr(names) + "\nfrom helpers import *\n")
        prelude = "from bridge import *\n"
    elif route == "package":
        (tmp_path / "library").mkdir()
        (tmp_path / "library/__init__.py").write_text("from helpers import *\n")
        prelude, call = "import library\n", "library.relay(source())"
    elif route == "alias":
        prelude += "from helpers import relay as selected\n"
        call = "selected(source())"
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(" + call + ")\n")
    return client, rules


@pytest.mark.parametrize("count", [1, 127, 128])
@pytest.mark.parametrize("route", ["direct", "reexport", "package", "alias"])
@pytest.mark.parametrize("forwards", [False, True])
def test_wide_exports_share_searches_and_preserve_selected_behavior(tmp_path, count, route, forwards):
    client, rules = install(tmp_path, count, route, forwards)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards
    assert result.dependencies.lookup_count == (1 if route == "direct" else 2)
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}
    records = result.report.at("/inputs/modules/lookups")
    assert all("star" not in record for record in records)
    assert all(len(record["members"]) == count for record in records if record["alias"] == "*")
    assert all("candidates" not in member for record in records for member in record["members"])


def test_shared_exports_fit_the_existing_response_byte_limit(tmp_path):
    client, rules = install(tmp_path)
    result = analyze(client, rules, "entry", imports=True, max_bytes=65536)
    assert result.report.at("/complete") and result.dependencies.lookup_count == 1
    assert len(json.dumps(result.report.to_data()).encode()) < 65536


@pytest.mark.parametrize("route", ["direct", "reexport", "package"])
def test_startup_callbacks_remain_visible_with_wide_exports(tmp_path, route):
    client, rules = install(tmp_path, route=route, startup="sink(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.witnesses
    assert any(site.occurrence.path == "helpers.py" for site in result.initialization.sites)
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["source", 17], ["sink", 17]]


def test_export_limit_is_still_128_names(tmp_path):
    client, rules = install(tmp_path, count=129)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "star-exports-unchecked" in result.report.at("/cutoffs")
    assert result.report.at("/inputs/modules/lookups/0/members") == []
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("prior_imports,complete", [(127, True), (128, False)])
def test_shared_members_cannot_bypass_the_module_search_budget(tmp_path, prior_imports, complete):
    client, rules = install(tmp_path)
    (tmp_path / "values.py").write_text("".join(f"v{index} = 0\n" for index in range(prior_imports)))
    path = tmp_path / "subject.py"
    path.write_text("".join(f"from values import v{index}\n" for index in range(prior_imports)) + path.read_text())
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") is complete
    assert result.dependencies.lookup_count == 128
    assert ("import-lookup-budget" in result.report.at("/cutoffs")) is (not complete)
    assert observe(tmp_path)["result"] == 17


def test_wildcard_child_searches_consume_the_same_fixed_budget(tmp_path):
    client, rules = workspace(tmp_path)
    package = tmp_path / "library"
    package.mkdir()
    names = [f"child{index:03}" for index in range(128)]
    (package / "__init__.py").write_text("__all__ = " + repr(names) + "\n")
    for name in names:
        (package / (name + ".py")).write_text("pass\n")
    (tmp_path / "subject.py").write_text("from library import *\ndef entry():\n    return sink(0)\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.dependencies.lookup_count == 128
    assert "import-lookup-budget" in result.report.at("/cutoffs")
    assert len(result.dependencies.files) == 16
    members = result.report.at("/inputs/modules/lookups/0/members")
    assert len(members) == 128 and members[-1]["resolution"] == "budget"
    assert sum(member["submodule"] is not None for member in members) == 127
    assert observe(tmp_path) == {"result": 0, "raised": None, "events": [["sink", 0]]}


def test_distinct_value_declarations_keep_their_existing_cap(tmp_path):
    client, rules = workspace(tmp_path)
    prelude = ""
    for module in ("left", "right"):
        names = [f"{module}{index}" for index in range(65)]
        (tmp_path / (module + ".py")).write_text("__all__ = " + repr(names) + "\n" +
                                               "".join(name + " = 0\n" for name in names))
        prelude += "from " + module + " import *\n"
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(source())\n")
    with pytest.raises(FrRuntimeError, match="import value declaration budget exceeds 128"):
        analyze(client, rules, "entry", imports=True)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("edit", ["body", "exports", "startup", "stub"])
@pytest.mark.parametrize("route", ["direct", "reexport"])
def test_provider_changes_invalidate_saved_answers(tmp_path, edit, route):
    client, rules = install(tmp_path, route=route)
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "helpers.py"
    text = path.read_text()
    if edit == "body":
        text = text.replace("def relay(value):\n    return value", "def relay(value):\n    return 0")
    elif edit == "exports":
        text = text.replace("'relay', ", "")
    elif edit == "startup":
        text += "sink(source())\n"
    else:
        (tmp_path / "helpers.pyi").write_text("pass\n")
    path.write_text(text)
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and changed.report.at("/complete") is (edit == "body")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))
    if edit == "body":
        assert not changed.witnesses and observe(tmp_path)["result"] == 0


@pytest.mark.parametrize("mutation", ["policy", "limits", "old-layout", "old-link", "omit", "name", "repeat", "copied-search"])
def test_rehashed_compact_records_fail_before_sdk_or_native_freshness_checks(tmp_path, mutation):
    client, rules = install(tmp_path, count=2)
    data = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    modules = data["inputs"]["modules"]
    anchor = modules["lookups"][0]
    if mutation == "policy":
        modules["policy"] = "static single-root modules"
    elif mutation == "limits":
        modules["limits"]["imports"] = 129
    elif mutation == "old-layout":
        del anchor["members"]
        anchor["star"] = None
    elif mutation == "old-link":
        anchor["members"][0]["star"] = 0
    elif mutation == "omit":
        anchor["members"].pop()
    elif mutation == "name":
        anchor["members"][0]["name"] = "invented"
    elif mutation == "repeat":
        anchor["members"].append(copy.deepcopy(anchor["members"][0]))
    else:
        anchor["members"][0]["candidates"] = anchor["candidates"]
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(data, ()))
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    path = tmp_path / "helpers.py"
    path.write_text(path.read_text().replace("return value", "return 0"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-shared-star-retained-") as directory:
        report = Path(directory) / "report.json"
        report.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="malformed imported value declarations"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(report),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
