import json
import copy
import hashlib
from pathlib import Path

import pytest
from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]


def analyze(client, rules, cache):
    handle = client.project('find', 'entry').definition_target().handle
    return cache.analyze(client, handle, standard_library=('builtins', 're'), rules=rules,
        context='html', steps=4096, depth=8, max_bytes=262144, summaries=True, imports=True)


def test_text_answers_expire_after_receiver_replacement_and_competing_imports(tmp_path):
    source = tmp_path / 'subject.py'
    source.write_text('def entry():\n    label = str(read_user())\n    return send(label.lower())\n')
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'text-flow', 'sources': ['read_user'], 'sinks': ['send'],
        'sanitizers': {}, 'propagators': []}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache)
    assert first.report.at('/complete') and first.witnesses and not first.reused
    assert analyze(client, rules, cache).reused
    source.write_text('class Other:\n    def lower(self):\n        return 0\n'
        'def entry():\n    label = str(read_user())\n    label = Other()\n    return send(label.lower())\n')
    changed = analyze(client, rules, cache)
    assert not changed.reused and not changed.report.at('/complete')
    assert any(site.kind == 'dynamic-call' for site in changed.runtime_effects.sites)
    source.write_text('def entry():\n    label = str(read_user())\n    return send(label.lower())\n')
    restored = analyze(client, rules, cache)
    assert restored.report.at('/complete') and restored.witnesses
    (tmp_path / 're.py').write_text('raise RuntimeError("local replacement")\n')
    shadowed = analyze(client, rules, cache)
    assert not shadowed.reused and not shadowed.report.at('/complete')


def test_a_saved_string_answer_rebinds_to_the_current_source_revision(tmp_path):
    (tmp_path / 'subject.py').write_text('def clean(value):\n    return str(value).strip()\n'
        'def entry():\n    return send(clean(read_user()).casefold())\n')
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'text-flow', 'sources': ['read_user'], 'sinks': ['send'],
        'sanitizers': {}, 'propagators': []}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache)
    assert first.report.at('/complete') and first.witnesses
    (tmp_path / 'unrelated.py').write_text('def another():\n    return 0\n')
    reused = analyze(client, rules, cache)
    fresh = analyze(client, rules, FlowCache(MemoryObjectStore()))
    assert reused.reused and reused.report.at('/revision') != first.report.at('/revision')
    assert reused.witnesses == fresh.witnesses
    assert reused.runtime_effects == fresh.runtime_effects


def test_an_imported_function_named_lower_uses_its_own_return_behavior(tmp_path):
    (tmp_path / 'helpers.py').write_text('def lower(value):\n    return 7\n')
    (tmp_path / 'subject.py').write_text('import helpers\ndef entry():\n'
        '    return send(helpers.lower(read_user()))\n')
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'text-flow', 'sources': ['read_user'], 'sinks': ['send'],
        'sanitizers': {}, 'propagators': []}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    result = analyze(client, rules, FlowCache(MemoryObjectStore()))
    assert result.report.at('/complete') and not result.witnesses
    (tmp_path / 'helpers.py').write_text('def lower(value):\n    return str(value)\n')
    result = analyze(client, rules, FlowCache(MemoryObjectStore()))
    assert result.report.at('/complete') and result.witnesses


@pytest.mark.parametrize('field,value', [('receiver_nodes', 0), ('methods', []), ('boundary', 'trust all receivers')])
def test_a_saved_answer_cannot_replace_the_checked_string_contract(tmp_path, field, value):
    (tmp_path / 'subject.py').write_text('def entry():\n    return send(str(read_user()).lower())\n')
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'text-flow', 'sources': ['read_user'], 'sinks': ['send'],
        'sanitizers': {}, 'propagators': []}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    result = analyze(client, rules, FlowCache(MemoryObjectStore()))
    forged = copy.deepcopy(result.report.to_data())
    forged['inputs']['modules']['standard_library']['string_methods'][field] = value
    forged['input_digest'] = hashlib.sha256(json.dumps(forged['inputs'], sort_keys=True, ensure_ascii=False,
        separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(forged, ()))
