"""Capture immutable local defaults at definition and bind calls independently."""
import ast
import copy
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


def install(tmp_path, value="0", route="local", parameters="value=((marker))", call="choose()", before="", after=""):
    client, rules = workspace(tmp_path)
    helper = f"marker = {value}\n{before}def choose({parameters}):\n    return value\n{after}"
    prelude = helper
    if route != "local":
        (tmp_path / "helpers.py").write_text(helper)
        prelude = "from helpers import choose\n"
        if route == "reexport":
            (tmp_path / "bridge.py").write_text(prelude)
            prelude = "from bridge import choose\n"
    (tmp_path / "subject.py").write_text(prelude + f"def entry():\n    return sink({call})\n")
    return client, rules


@pytest.mark.parametrize("value", ["None", "False", "True", "17", "-17", "1.25", "'café'", "r'raw'"])
@pytest.mark.parametrize("route", ["local", "direct", "reexport"])
@pytest.mark.parametrize("explicit", [False, True])
def test_captured_scalar_and_explicit_override_match_python(tmp_path, value, route, explicit):
    client, rules = install(tmp_path, value, route, call="choose(source())" if explicit else "choose()")
    result = analyze(client, rules, "entry", imports=route != "local")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == explicit
    runtime = observe(tmp_path)
    assert runtime == {"result": 17 if explicit else ast.literal_eval(value), "raised": None,
                       "events": [["source", 17], ["sink", 17]] if explicit else [["sink", ast.literal_eval(value)]]}
    function = next(item for item in result.summaries.functions if item.function.endswith("choose"))
    parameter = function.signature[0]
    assert not parameter.required and parameter.default is not None
    path = tmp_path / parameter.default.path
    span = parameter.default.location.span
    assert path.read_bytes()[span.start:span.end] == b"((marker))"


@pytest.mark.parametrize("parameters,call,tainted", [
    ("value=marker, /", "choose()", False),
    ("value=marker, /", "choose(source())", True),
    ("*, value=marker", "choose()", False),
    ("*, value=marker", "choose(value=source())", True),
    ("marker, /, value=marker", "choose(source())", False),
    ("value=marker, *unused, **options", "choose()", False),
])
def test_kinds_and_parameter_shadowing_preserve_binding(tmp_path, parameters, call, tainted):
    client, rules = install(tmp_path, parameters=parameters, call=call)
    result = analyze(client, rules, "entry")
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) == tainted
    assert observe(tmp_path)["result"] == (17 if tainted else 0)


@pytest.mark.parametrize("before,after,expected", [
    ("marker = 1\n", "", 1),
    ("del marker\n", "", None),
    ("", "marker = 1\n", 0),
    ("", "del marker\n", 0),
    ("", "marker = source()\n", 0),
    ("def setup():\n    pass\nsetup()\n", "", 0),
])
def test_unsupported_setup_keeps_capture_order_and_result_incomplete(tmp_path, before, after, expected):
    client, rules = install(tmp_path, before=before, after=after)
    result = analyze(client, rules, "entry")
    assert not result.report.at("/complete")
    if after:
        assert "unsupported-parameter-contract:choose" not in result.report.at("/cutoffs")
    else:
        assert "unsupported-parameter-contract:choose" in result.report.at("/cutoffs")
    runtime = observe(tmp_path)
    assert runtime["raised"] == ("NameError" if before == "del marker\n" else None)
    assert runtime["result"] == expected


@pytest.mark.parametrize("value", ["[]", "{}", "(0,)", "[0]", "source()"])
@pytest.mark.parametrize("call", ["choose()", "choose(source())"])
def test_checked_non_scalar_defaults_can_be_replaced_but_not_consumed(tmp_path, value, call):
    client, rules = install(tmp_path, value, call=call)
    result = analyze(client, rules, "entry")
    complete = value != 'source()' and call != 'choose()'
    assert result.report.at('/complete') is complete
    assert bool(result.witnesses) is complete
    if not complete:
        reason = 'unsupported-parameter-contract' if value == 'source()' else 'unsupported-default-value'
        assert reason + ':choose' in result.report.at('/cutoffs')
    else:
        assert observe(tmp_path)['result'] == 17


def test_an_imported_scalar_can_supply_a_default(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / "constants.py").write_text("marker = 0\n")
    path = tmp_path / "subject.py"
    path.write_text(path.read_text().replace("marker = 0", "from constants import marker"))
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete") and not result.initialization.sites
    assert not result.witnesses
    assert observe(tmp_path)["result"] == 0


def test_same_named_defaults_in_different_files_keep_their_own_capture(tmp_path):
    client, rules = install(tmp_path, route="direct")
    (tmp_path / "other.py").write_text("marker = []\ndef choose(value=marker):\n    return value\n")
    (tmp_path / "subject.py").write_text("from helpers import choose\nfrom other import choose as shared\ndef entry():\n    sink(choose())\n    return shared(source())\n")
    result = analyze(client, rules, "entry", imports=True)
    assert result.report.at("/complete"), result.report.at('/cutoffs')
    assert result.witnesses
    assert "unsupported-parameter-contract:helpers.py::choose" not in result.report.at("/cutoffs")


def test_changed_scalar_capture_invalidates_reuse(tmp_path):
    client, rules = install(tmp_path, route="direct")
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, "entry", cache, imports=True).report.at("/complete")
    assert analyze(client, rules, "entry", cache, imports=True).reused
    path = tmp_path / "helpers.py"
    path.write_text(path.read_text().replace("marker = 0", "marker = []"))
    changed = analyze(client, rules, "entry", cache, imports=True)
    assert not changed.reused and not changed.report.at("/complete")
    assert evidence(changed) == evidence(analyze(client, rules, "entry", imports=True))


def test_previous_call_rules_are_rejected_before_reuse(tmp_path):
    client, rules = install(tmp_path)
    data = copy.deepcopy(analyze(client, rules, "entry").report.to_data())
    for contract in (data["call_binding"], data["inputs"]["call_binding"]):
        contract["parameters"] = "ASCII positional-only, positional-or-keyword and keyword-only slots; required or immutable literal defaults."
        contract["definitions"] = ("ASCII module functions defer bodies with literal or prior known value defaults, including variadics. "
                                   "Calls require literal defaults. Annotations need a checked module-local future directive and at most 128 nodes; "
                                   "no await, yield, assignment or type unpacking.")
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(data, ()))
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False,
                                                    separators=(",", ":")).encode()).hexdigest()
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    subject = tmp_path / "subject.py"
    subject.write_text(subject.read_text().replace("marker = 0", "marker = 1"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-scalar-default-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="unsupported call binding"):
            client.project("dataflow", handle, "--summaries", "--rules", str(rules), "--steps", "4096",
                           "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
