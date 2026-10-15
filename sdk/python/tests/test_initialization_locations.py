"""Explain unchecked imports with exact source statements and honest omissions."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_initialization import FlowInitialization
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_class_initialization import install, observe
from test_flow_summaries import analyze, evidence, workspace


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("definition,instance,reason,fragment", [
    ("class Token(list):\n    pass", "", "module-statement", "class Token(list):\n    pass"),
    ("class Token:\n    def __init__(self):\n        sink(source())", "marker = Token()",
     "module-statement", "marker = Token()"),
    ("def unused(value=source()):\n    return value", "", "definition-time-code",
     "def unused(value=source()):\n    return value"),
    ("metadata = 0\nmetadata = 1", "", "binding-collision", "metadata = 1"),
    ("def K(value):\n    return value", "", "non-normalized-identifier",
     "def K(value):\n    return value"),
    ("def decorate(fn):\n    return fn\n@decorate\ndef unused():\n    pass", "",
     "module-statement", "@decorate\ndef unused():\n    pass"),
])
def test_blocking_statements_have_exact_locations_and_runtime_remains_separate(tmp_path, imports, definition, instance, reason, fragment):
    client, rules = install(tmp_path, definition, instance, imports, value="0")
    result = analyze(client, rules, "entry", imports=imports)
    disclosure = result.initialization
    assert not result.report.at("/complete") and disclosure.omitted == 0
    assert len(disclosure.sites) == 1
    site = disclosure.sites[0]
    assert site.reasons == (reason,) and site.explanations
    path = "helpers.py" if imports else "subject.py"
    assert site.occurrence.path == path
    assert site.occurrence.text((tmp_path / path).read_text(), revision=result.report.at("/revision")) == fragment
    actual = observe(tmp_path)
    assert actual["result"] == 0 and actual["raised"] is None
    if instance:
        assert actual["events"] == [["source", 17], ["sink", 17], ["sink", 0]]
    assert not result.witnesses


def test_multiple_reasons_share_one_statement_and_unicode_byte_coordinates(tmp_path):
    client, rules = workspace(tmp_path)
    source = '# Unicode π changes byte offsets\nmetadata = 0\ndef metadata(value=source()):\n    return value\ndef entry():\n    return sink(0)\n'
    (tmp_path / "subject.py").write_text(source)
    result = analyze(client, rules, "entry")
    sites = result.initialization.sites
    assert len(sites) == 1
    assert sites[0].reasons == ("definition-time-code", "binding-collision")
    assert sites[0].occurrence.text(source, revision=result.report.at("/revision")) == 'def metadata(value=source()):\n    return value'


@pytest.mark.parametrize("max_bytes", [8192, 1_048_576])
def test_omitted_locations_keep_the_total_and_never_establish_absence(tmp_path, max_bytes):
    client, rules = install(tmp_path, "\n".join("opaque()" for _ in range(24)), value="0")
    result = analyze(client, rules, "entry", max_bytes=max_bytes)
    disclosure = result.initialization
    assert len(disclosure.sites) + disclosure.omitted == 24
    assert len(disclosure.sites) <= 16 and disclosure.omitted >= 8
    assert not result.report.at("/complete")
    assert "module-effects-unchecked" in result.report.at("/cutoffs")
    if max_bytes == 1_048_576:
        assert len(disclosure.sites) == 16 and disclosure.omitted == 8


def test_locations_follow_source_order_across_imported_files_and_ignore_unloaded_files(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text('from zeta import relay\nfrom alpha import spare\ndef entry():\n    return sink(relay(0))\n')
    (tmp_path / "alpha.py").write_text('first = unknown()\nsecond = unknown()\ndef spare(value):\n    return value\n')
    (tmp_path / "zeta.py").write_text('third = unknown()\ndef relay(value):\n    return value\n')
    (tmp_path / "unloaded.py").write_text('unrelated = unknown()\n')
    result = analyze(client, rules, "entry", imports=True)
    sites = result.initialization.sites
    assert [site.occurrence.path for site in sites] == ["alpha.py", "alpha.py", "zeta.py"]
    assert [site.occurrence.text((tmp_path / site.occurrence.path).read_text(), revision=result.report.at("/revision")) for site in sites] == ['first = unknown()', 'second = unknown()', 'third = unknown()']


def test_import_errors_remain_separate_from_initialization_statements(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / "subject.py").write_text('from missing import relay\ndef entry():\n    return sink(relay(0))\n')
    result = analyze(client, rules, "entry", imports=True)
    assert not result.initialization.sites and result.initialization.omitted == 0
    assert not result.report.at("/complete")
    assert "unresolved-local-import:missing" in result.report.at("/cutoffs")


def test_an_edit_removes_a_blocker_and_later_unrelated_edits_reuse_the_complete_answer(tmp_path):
    client, rules = install(tmp_path, "metadata = source()", imports=True, value="0")
    cache = FlowCache(MemoryObjectStore())
    blocked = analyze(client, rules, "entry", cache, imports=True)
    assert blocked.initialization.sites and not blocked.reused
    (tmp_path / "helpers.py").write_text('metadata = 0\ndef relay(value):\n    return value\n')
    fixed = analyze(client, rules, "entry", cache, imports=True)
    assert fixed.report.at("/complete") and not fixed.reused
    assert not fixed.initialization.sites and fixed.initialization.omitted == 0
    (tmp_path / "unrelated.py").write_text('def unrelated():\n    return 0\n')
    reused = analyze(client, rules, "entry", cache, imports=True)
    assert reused.reused
    assert evidence(reused) == evidence(analyze(client, rules, "entry", imports=True))


@pytest.mark.parametrize("mutation", ["missing", "field", "boolean", "negative", "reason", "duplicate-reason", "revision", "path", "role", "repeat", "order", "hide", "complete"])
def test_corrupt_disclosures_are_rejected_even_when_the_report_is_rehashed(tmp_path, mutation):
    client, rules = install(tmp_path, 'metadata = source()\nother = source()', value="0")
    data = analyze(client, rules, "entry").report.to_data()
    value = data["initialization"]
    site = value["sites"][0]
    if mutation == "missing":
        del data["initialization"]
    elif mutation == "field":
        value["version"] = 1
    elif mutation == "boolean":
        value["omitted"] = False
    elif mutation == "negative":
        value["omitted"] = -1
    elif mutation == "reason":
        site["reasons"] = ["trusted"]
    elif mutation == "duplicate-reason":
        site["reasons"] *= 2
    elif mutation == "revision":
        site["occurrence"]["revision"] = "0" * 64
    elif mutation == "path":
        site["occurrence"]["path"] = "unloaded.py"
    elif mutation == "role":
        site["occurrence"]["role"] = "source"
    elif mutation == "repeat":
        value["sites"].append(copy.deepcopy(site))
    elif mutation == "order":
        value["sites"].reverse()
    elif mutation == "hide":
        value["sites"] = []
    else:
        data["complete"] = True
    data["input_digest"] = hashlib.sha256(json.dumps(data["inputs"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    for reader in (FlowInitialization.from_report, FunctionSummaries.from_report):
        with pytest.raises(FrRuntimeError):
            reader(FrReport(data, ()))


def test_native_reuse_rejects_missing_current_initialization_disclosure(tmp_path):
    client, rules = install(tmp_path, "metadata = 0", value="0")
    original = analyze(client, rules, "entry")
    data = original.report.to_data()
    del data["initialization"]
    encoded = json.dumps(data, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-initialization-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="unchecked initialization"):
            client.project('dataflow', handle, '--summaries', '--rules', str(rules), '--context', 'html',
                           '--steps', '4096', '--bytes', '1048576', '--reuse', str(path),
                           '--reuse-digest', hashlib.sha256(encoded.encode()).hexdigest())
