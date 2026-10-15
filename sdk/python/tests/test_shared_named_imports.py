"""Compare grouped named imports, startup effects and saved answers with Python."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


def install(tmp_path, count=128, route="direct", forwards=True, startup=""):
    client, rules = workspace(tmp_path)
    names = ["relay", *[f"helper{index}" for index in range(count - 1)]]
    helper = startup + "def relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    helper += "".join(f"def {name}(value):\n    return value\n" for name in names[1:])
    (tmp_path / "helpers.py").write_text(helper)
    requested = ", ".join(names)
    prelude, call = f"from helpers import ({requested},)\n", "relay(source())"
    if route == "alias":
        requested = ", ".join(f"{name} as picked{index}" for index, name in enumerate(names))
        prelude, call = f"from helpers import ({requested},)\n", "picked0(source())"
    elif route == "reexport":
        (tmp_path / "bridge.py").write_text(prelude)
        prelude = f"from bridge import ({requested},)\n"
    elif route == "package":
        (tmp_path / "library").mkdir()
        (tmp_path / "library/helpers.py").write_text(helper)
        (tmp_path / "library/__init__.py").write_text(f"from .helpers import ({requested},)\n")
        prelude, call = "import library\n", "library.relay(source())"
    (tmp_path / "subject.py").write_text(prelude + f"def entry():\n    return sink({call})\n")
    return client, rules


@pytest.mark.parametrize("count", [1, 127, 128])
@pytest.mark.parametrize("route", ["direct", "alias", "reexport", "package"])
@pytest.mark.parametrize("forwards", [False, True])
def test_named_lists_share_provider_searches_and_keep_each_binding(tmp_path, count, route, forwards):
    client, rules = install(tmp_path, count, route, forwards)
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is forwards
    assert result.dependencies.lookup_count == (1 if route in {"direct", "alias"} else 2)
    assert observe(tmp_path) == {"result": 17 if forwards else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if forwards else 0]]}
    records = result.report.at("/inputs/modules/lookups")
    for record in records:
        if record["names"] is None:
            assert not record["members"]
            continue
        assert len(record["names"]) == count and len(record["members"]) == count - 1
        assert record["names"][0] == [record["alias"], record["member"]]
        assert record["names"][1:] == [[member["alias"], member["member"]] for member in record["members"]]
        assert all(len(member) == 7 and "candidates" not in member for member in record["members"])
    assert all(lookup.star is None for lookup in result.dependencies.lookups)


def test_wide_named_import_fits_the_existing_response_byte_limit(tmp_path):
    client, rules = install(tmp_path)
    client = FrClient(tmp_path, executable=client.executable, max_output_bytes=65536)
    result = analyze(client, rules, "entry", imports=True, max_bytes=65536)
    assert result.report.at("/complete") and result.dependencies.lookup_count == 1
    assert len(json.dumps(result.report.to_data(), ensure_ascii=False, separators=(",", ":")).encode()) < 65536


@pytest.mark.parametrize("route", ["direct", "alias", "reexport", "package"])
def test_provider_startup_effects_stay_visible(tmp_path, route):
    client, rules = install(tmp_path, route=route, startup="sink(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.witnesses
    assert result.initialization.sites
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17], ["source", 17], ["sink", 17]]


def test_named_import_list_stops_at_128_bindings(tmp_path):
    client, rules = install(tmp_path, count=129)
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    assert "import-binding-budget" in result.report.at("/cutoffs")
    assert result.report.at("/inputs/modules/limits/names_per_import") == 128
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("prior,complete", [(127, True), (128, False)])
def test_separate_import_statements_still_consume_separate_searches(tmp_path, prior, complete):
    client, rules = install(tmp_path)
    (tmp_path / "values.py").write_text("".join(f"v{index} = 0\n" for index in range(prior)))
    path = tmp_path / "subject.py"
    path.write_text("".join(f"from values import v{index}\n" for index in range(prior)) + path.read_text())
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") is complete
    assert result.dependencies.lookup_count == 128
    assert ("import-lookup-budget" in result.report.at("/cutoffs")) is (not complete)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("prior,complete", [(126, True), (127, False)])
def test_named_package_children_charge_the_same_search_budget(tmp_path, prior, complete):
    client, rules = install(tmp_path, count=1)
    (tmp_path / "library").mkdir()
    (tmp_path / "library/__init__.py").write_text("def selected(value):\n    return value\n")
    (tmp_path / "library/child.py").write_text("pass\n")
    (tmp_path / "subject.py").write_text(
        "".join(f"from helpers import relay as prior{index}\n" for index in range(prior)) +
        "from library import selected, child\ndef entry():\n    return sink(selected(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") is complete
    assert result.dependencies.lookup_count == 128
    member = result.report.at("/inputs/modules/lookups")[-1]["members"][0]
    assert member["resolution"] == ("module" if complete else "budget")
    assert (member["submodule"] is not None) is complete
    assert observe(tmp_path)["result"] == 17


def test_128_named_children_keep_provider_and_child_costs_separate(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "library").mkdir()
    (tmp_path / "library/__init__.py").write_text("pass\n")
    names = [f"child{index:03}" for index in range(128)]
    for name in names:
        (tmp_path / "library" / (name + ".py")).write_text("pass\n")
    (tmp_path / "subject.py").write_text("from library import " + ", ".join(names) +
                                       "\ndef entry():\n    return sink(0)\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete") and result.dependencies.lookup_count == 128
    record = result.report.at("/inputs/modules/lookups/0")
    assert len(record["names"]) == 128 and len(record["members"]) == 127
    assert record["members"][-1]["resolution"] == "budget"
    assert sum(lookup.submodule is not None for lookup in result.dependencies.lookups) == 127
    assert len(result.dependencies.files) == 16
    assert observe(tmp_path)["result"] == 0


def test_existing_package_objects_take_precedence_for_each_named_member(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "library").mkdir()
    (tmp_path / "library/__init__.py").write_text("class child:\n    pass\ndef relay(value):\n    return value\n")
    (tmp_path / "library/child.py").write_text("sink(source())\n")
    (tmp_path / "subject.py").write_text("from library import child as marker, relay\ndef entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and result.dependencies.lookup_count == 1
    assert "library/child.py" not in dict(result.dependencies.files)
    assert result.dependencies.lookups[0].resolution == "value"
    assert observe(tmp_path)["events"] == [["source", 17], ["sink", 17]]


@pytest.mark.parametrize("prelude,raised,value", [
    ("from helpers import relay as selected, helper0 as selected\n", None, 0),
    ("from helpers import relay as selected, relay as selected\n", None, 17),
    ("from helpers import relay as selected, helper0\nselected = 0\n", "TypeError", None),
    ("from helpers import relay as selected, helper0\ndel selected\n", "NameError", None),
    ("from helpers import relay as selected, helper0\nexec('selected = lambda value: 0')\n", None, 0),
    ("from helpers import relay as selected, missing\n", "ImportError", None),
])
def test_collisions_replacement_deletion_dynamic_setup_and_missing_members_stay_incomplete(tmp_path, prelude, raised, value):
    client, rules = install(tmp_path, count=2)
    helper = tmp_path / "helpers.py"
    helper.write_text(helper.read_text().replace("def helper0(value):\n    return value", "def helper0(value):\n    return 0"))
    (tmp_path / "subject.py").write_text(prelude + "def entry():\n    return sink(selected(source()))\n")
    result = analyze(client, rules, "entry", imports=True)
    assert not result.report.at("/complete")
    runtime = observe(tmp_path)
    assert runtime["raised"] == raised and runtime["result"] == value


def test_named_groups_keep_the_128_distinct_value_limit(tmp_path):
    client, rules = workspace(tmp_path)
    imports = ""
    for module in ("left", "right"):
        names = [f"{module}{index}" for index in range(65)]
        (tmp_path / (module + ".py")).write_text("".join(name + " = 0\n" for name in names))
        imports += "from " + module + " import " + ", ".join(names) + "\n"
    (tmp_path / "subject.py").write_text(imports + "def entry():\n    return sink(source())\n")
    with pytest.raises(FrRuntimeError, match="import value declaration budget exceeds 128"):
        analyze(client, rules, "entry", imports=True)
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("edit", ["body", "missing", "startup", "stub"])
@pytest.mark.parametrize("route", ["direct", "reexport"])
def test_provider_edits_invalidate_every_shared_named_binding(tmp_path, edit, route):
    client, rules = install(tmp_path, route=route)
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "helpers.py"
    text = path.read_text()
    if edit == "body":
        text = text.replace("def relay(value):\n    return value", "def relay(value):\n    return 0")
    elif edit == "missing":
        text = text.replace("def relay(value)", "def other(value)")
    elif edit == "startup":
        text += "sink(source())\n"
    else:
        (tmp_path / "helpers.pyi").write_text("pass\n")
    path.write_text(text)
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused and result.report.at("/complete") is (edit == "body")
    assert evidence(result) == evidence(analyze(client, rules, "entry", imports=True))
    if edit == "body":
        assert not result.witnesses and observe(tmp_path)["result"] == 0


@pytest.mark.parametrize("mutation", ["policy", "limits", "old-layout", "names", "pair", "first", "omit",
                                      "repeat", "alias", "prefix", "parent-prefix", "copied-search", "old-link"])
def test_rehashed_named_groups_fail_before_sdk_and_native_freshness_checks(tmp_path, mutation):
    client, rules = install(tmp_path, count=3, route="alias")
    data = copy.deepcopy(analyze(client, rules, "entry", imports=True).report.to_data())
    modules = data["inputs"]["modules"]
    record = modules["lookups"][0]
    if mutation == "policy":
        modules["policy"] = "Single-root static modules/packages; wildcard names share module searches and retain per-name bindings."
    elif mutation == "limits":
        del modules["limits"]["names_per_import"]
    elif mutation == "old-layout":
        del record["names"]
    elif mutation == "names":
        record["names"] = None
    elif mutation == "pair":
        record["names"][1].append("extra")
    elif mutation == "first":
        record["names"][0][0] = "invented"
    elif mutation == "omit":
        record["members"].pop()
    elif mutation == "repeat":
        record["members"].append(copy.deepcopy(record["members"][0]))
    elif mutation == "alias":
        record["members"][0]["alias"] = "invented"
    elif mutation == "prefix":
        record["members"][0]["prefix"] = "invented"
    elif mutation == "parent-prefix":
        record["prefix"] = "invented"
    elif mutation == "copied-search":
        record["members"][0]["candidates"] = record["candidates"]
    else:
        record["members"][0]["star"] = 0
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(data, ()))
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    path = tmp_path / "helpers.py"
    path.write_text(path.read_text().replace("return value", "return 0"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-shared-named-retained-") as directory:
        report = Path(directory) / "report.json"
        report.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="malformed imported value declarations"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(report),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
