"""Function help text changes must preserve calls without hiding executable effects."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


def install(tmp_path, helper, imports, body="return sink(relay(source()))"):
    client, rules = workspace(tmp_path)
    if imports:
        (tmp_path / "helpers.py").write_text(helper)
        helper = "from helpers import relay\n"
    (tmp_path / "subject.py").write_text(helper + "def entry():\n" +
        "".join("    " + line + "\n" for line in body.splitlines()))
    return client, rules


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("returned", ["value", "0"])
@pytest.mark.parametrize("literal,doc", [
    ('"helper"', "helper"), ("None", None), ('r"café"', "café"),
    ('u"π 🦀"', "π 🦀"), ('"""first\nsecond"""', "first\nsecond"), ('""', ""),
])
def test_literal_documentation_changes_help_text_and_preserves_original_calls(tmp_path, imports, returned, literal, doc):
    helper = "def relay(value):\n    return " + returned + "\nrelay.__doc__ = " + literal + "\n"
    client, rules = install(tmp_path, helper, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete"), result.report.at("/cutoffs")
    assert bool(result.witnesses) is (returned == "value") and not result.initialization.sites
    summary = result.summaries.for_function("helpers.py::relay" if imports else "relay")
    path = "helpers.py" if imports else "subject.py"
    assert summary.signature[0].site.path == path
    assert summary.signature[0].site.text((tmp_path / path).read_text(), revision=result.report.at("/revision")) == "value"
    assert observe(tmp_path) == {"result": 17 if returned == "value" else 0, "raised": None,
                                "events": [["source", 17], ["sink", 17 if returned == "value" else 0]]}
    script = "import json; namespace = {}; exec(" + repr(helper) + ", namespace); print(json.dumps(namespace['relay'].__doc__))"
    runtime = subprocess.run([sys.executable, "-I", "-B", "-c", script], check=True,
                             capture_output=True, text=True, timeout=30)
    assert json.loads(runtime.stdout) == doc


@pytest.mark.parametrize("imports", [False, True])
def test_repeated_documentation_writes_and_unsupported_uncalled_bodies_remain_separate(tmp_path, imports):
    helper = ('def unused(value):\n    return value.attribute\nunused.__doc__ = "first"\n'
              'unused.__doc__ = None\nmetadata = 0\n'
              'def relay(value):\n    return value\nrelay.__doc__ = "second"\npublic = relay\n')
    client, rules = install(tmp_path, helper, imports)
    result = analyze(client, rules, "entry", imports=imports)
    assert result.report.at("/complete") and result.witnesses and not result.initialization.sites
    assert observe(tmp_path)["result"] == 17


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("statement", [
    'relay.__doc__ = source()', 'relay.__doc__ = f"helper"', 'relay.__doc__ = b"helper"',
    'relay.__doc__ = t"helper"', 'relay.__doc__ = ("helper")', 'relay.__doc__ = "a" "b"',
    'relay.__doc__ = 0', 'relay.__doc__: str = "helper"', 'relay.__doc__ = other = "helper"',
    'relay.__doc__ += "helper"', 'del relay.__doc__', 'relay.__name__ = "other"',
    'relay.__defaults__ = (0,)', 'relay.__kwdefaults__ = {}', 'relay.__annotations__ = {}',
    'relay.__code__ = other.__code__', 'relay.skip = 0',
])
def test_other_writes_and_evaluated_values_keep_their_original_unchecked_site(tmp_path, imports, statement):
    client, rules = install(tmp_path, "def relay(value):\n    return value\n" + statement + "\n", imports,
                            body="return sink(source())")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete") and "module-effects-unchecked" in result.report.at("/cutoffs")
    path = "helpers.py" if imports else "subject.py"
    text = (tmp_path / path).read_text()
    assert any(site.occurrence.path == path and
               site.occurrence.text(text, revision=result.report.at("/revision")) == statement
               for site in result.initialization.sites)


@pytest.mark.parametrize("imports", [False, True])
@pytest.mark.parametrize("prefix,target", [
    ('', 'relay'), ('def relay(value):\n    return value\npublic = relay\n', 'public'),
    ('class relay:\n    pass\n', 'relay'), ('relay = 0\n', 'relay'),
    ('def relay(value):\n    return value\nrelay = 0\n', 'relay'),
    ('def relay(value):\n    return value\nrelay = relay\n', 'relay'),
    ('def relay(value):\n    return value\ndel relay\n', 'relay'),
    ('@decorate\ndef relay(value):\n    return value\n', 'relay'),
    ('def relay(value=source()):\n    return value\n', 'relay'),
    ('relay = make_helper()\n', 'relay'),
    ('def relay(value):\n    return value\nchange_function()\n', 'relay'),
    ('def relay(value):\n    return value\ndef other(value=change_function()):\n    return value\n', 'relay'),
    ('def relay(value):\n    return value\nimport other\n', 'relay'),
    ('from other import relay\n', 'relay'),
    ('def relay(value):\n    return value\ndef relay(value):\n    return 0\n', 'relay'),
])
def test_only_prior_unchanged_plain_function_objects_accept_documentation(tmp_path, imports, prefix, target):
    statement = target + '.__doc__ = "helper"'
    client, rules = install(tmp_path, prefix + statement + "\n", imports, body="return sink(source())")
    (tmp_path / "other.py").write_text("def relay(value):\n    return value\n")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete")
    path = "helpers.py" if imports else "subject.py"
    text = (tmp_path / path).read_text()
    assert any(site.occurrence.path == path and
               site.occurrence.text(text, revision=result.report.at("/revision")) == statement
               for site in result.initialization.sites)


@pytest.mark.parametrize("imports", [False, True])
def test_reading_documentation_still_requires_an_unmodeled_attribute_read(tmp_path, imports):
    client, rules = install(tmp_path, 'def relay(value):\n    return value\nrelay.__doc__ = "helper"\n',
                            imports, body="return sink(relay.__doc__)")
    result = analyze(client, rules, "entry", imports=imports)
    assert not result.report.at("/complete") and not result.initialization.sites
    assert observe(tmp_path)["result"] == "helper"


@pytest.mark.parametrize("edit,complete", [
    ('relay.__doc__ = "new help"\n', True), ('relay.__doc__ = None\n', True),
    ('relay.__doc__ = source()\n', False), ('relay.__defaults__ = (0,)\n', False),
])
def test_source_edits_invalidate_cached_documentation_decisions(tmp_path, edit, complete):
    prefix = "def relay(value):\n    return value\n"
    client, rules = install(tmp_path, prefix + 'relay.__doc__ = "helper"\n', True)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, "entry", cache, imports=True)
    assert first.report.at("/complete") and analyze(client, rules, "entry", cache, imports=True).reused
    (tmp_path / "helpers.py").write_text(prefix + edit)
    result = analyze(client, rules, "entry", cache, imports=True)
    assert not result.reused and result.report.at("/complete") is complete
    assert evidence(result) == evidence(analyze(client, rules, "entry", imports=True))


def test_previous_initialization_contract_is_rejected_before_native_or_sdk_reuse(tmp_path):
    client, rules = install(tmp_path, 'def relay(value):\n    return value\nrelay.__doc__ = "helper"\n', True)
    value = analyze(client, rules, "entry", imports=True).report.to_data()
    value["call_binding"]["initialization"] = (
        "Literals, aliases, built-in references, exports and plain classes/instances are inert. "
        "Reads, collisions and effects stay unchecked.")
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(value, ()))
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    handle = client.project("find", "entry").definition_target().handle
    with tempfile.TemporaryDirectory(prefix="fr-function-docs-retained-") as directory:
        path = Path(directory) / "retained.json"
        path.write_text(encoded)
        with pytest.raises(FrRuntimeError, match="call binding"):
            client.project("dataflow", handle, "--summaries", "--imports", "--rules", str(rules),
                           "--steps", "4096", "--bytes", "1048576", "--reuse", str(path),
                           "--reuse-digest", hashlib.sha256(encoded.encode()).hexdigest())
