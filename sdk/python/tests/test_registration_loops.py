"""Compare known registration loops and unsafe edits with ordinary Python."""
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from test_flow_summaries import analyze, evidence
from test_postponed_annotations import observe
from test_registry_initialization import install
from test_startup_registration import setup


PREFIX = "table = {}\nlabels = {'state': 'ready', 'count': 2}\n"
BODIES = [
    "for key in ('state', 'label'):\n    table[key] = 'ready'",
    "for key in ['state', 'label']:\n    table.update({key: 'ready'})",
    "for key, item in labels.items():\n    table[key] = item",
    "for (key, item) in labels.items():\n    table.update({key: [item]})",
    "for key in ('state',):\n    for item in (1, 2):\n        table[key] = item",
]


@pytest.mark.parametrize('body', BODIES)
@pytest.mark.parametrize('route', ['local', 'value', 'reexport', 'star'])
@pytest.mark.parametrize('forwards', [True, False])
def test_known_iterations_preserve_both_answers(tmp_path, body, route, forwards):
    client, rules = install(tmp_path, setup(body, 'register()\nregister()', prefix=PREFIX), route, forwards)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {'result': 17 if forwards else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17 if forwards else 0]]}


@pytest.mark.parametrize('body', [
    "for key, item in labels.items():\n    labels[key] = item",
    "for key, item in labels.items():\n    labels['extra'] = 0",
    "for key in ('state',):\n    table[key] = source()",
    "for key in unknown():\n    table[key] = 0",
    "for key in range(2):\n    table[key] = 0",
    "for key in ():\n    table[key] = 0",
    "for key in ('state',):\n    break\n    table[key] = 0",
    "for key in ('state',):\n    continue\n    table[key] = 0",
    "for key in ('state',):\n    table[key] = 0\nelse:\n    table['else'] = source()",
    "for key in ('state',):\n    return\n    table[key] = 0",
    "for table in ('state',):\n    table['state'] = 0",
    "for key in ('state',):\n    for key in ('label',):\n        table[key] = 0",
    "for key in ('state',):\n    table[key] = 0\nfor key in ('label',):\n    table[key] = 0",
    "for dict in ('state',):\n    table['state'] = dict()",
    "table['state'] = dict()\nfor dict in ('state',):\n    pass",
    "for key, item in labels.items():\n    global relay\n    relay = lambda value: 0",
    "for key in ([1],):\n    table[key] = 0",
    "for key in ('state',):\n    table[key] = 0\n    key = source()",
    "for key, item in labels.items(0):\n    table[key] = item",
    "for key, key in labels.items():\n    table[key] = 0",
])
def test_unknown_effects_scope_changes_and_dictionary_mutation_refuse(tmp_path, body):
    client, rules = install(tmp_path, setup(body, prefix=PREFIX), 'value')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete')
    assert 'module-effects-unchecked' in result.report.at('/cutoffs')
    assert result.initialization.sites


@pytest.mark.parametrize('intervening', [
    "labels['state'] = 'changed'", "labels.update(state='changed')",
    'alias = labels', 'import unused', "labels = {'state': 'changed'}",
])
def test_changed_or_escaped_input_contents_are_not_reused(tmp_path, intervening):
    client, rules = install(tmp_path, setup(BODIES[2], intervening + '\nregister()', prefix=PREFIX))
    assert not analyze(client, rules, 'entry').report.at('/complete')


def test_writes_in_one_call_discard_contents_before_the_next_call(tmp_path):
    body = "for key, item in labels.items():\n    table[key] = item\nlabels['state'] = 'changed'"
    client, rules = install(tmp_path, setup(body, 'register()\nregister()', prefix=PREFIX))
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete')
    assert observe(tmp_path)['result'] == 17


def test_python_copies_entries_and_leaves_callbacks_unexecuted(tmp_path):
    prefix = "table = {}\nlabels = {'state': 'ready', 'send': lambda: missing()}\n"
    client, rules = install(tmp_path, setup(BODIES[2], prefix=prefix), 'value')
    assert analyze(client, rules, 'entry', imports=True).report.at('/complete')
    script = ("import sys,json; sys.path.insert(0,sys.argv[1]); import helpers; "
              "print(json.dumps([helpers.table['state'], callable(helpers.table['send']), sorted(helpers.table)]))")
    result = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            check=True, capture_output=True, text=True, timeout=30)
    assert json.loads(result.stdout) == ['ready', True, ['send', 'state']]


@pytest.mark.parametrize('edit', ['iterable', 'body', 'input'])
def test_edits_invalidate_persisted_answers(tmp_path, edit):
    client, rules = install(tmp_path, setup(BODIES[2], prefix=PREFIX), 'value')
    store = MemoryObjectStore()
    cache = FlowCache(store)
    assert analyze(client, rules, 'entry', cache, imports=True).report.at('/complete')
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    path = tmp_path / 'helpers.py'
    old, new = {'iterable': ('labels.items()', 'missing()'),
                'body': ('table[key] = item', 'table[key] = source()'),
                'input': ("'state': 'ready'", "'state': source()")} [edit]
    path.write_text(path.read_text().replace(old, new))
    result = analyze(client, rules, 'entry', cache, imports=True)
    assert not result.reused and not result.report.at('/complete')
    assert evidence(result) == evidence(analyze(client, rules, 'entry', imports=True))


@pytest.mark.parametrize('entries,complete', [(2, True), (20, False)])
def test_expanded_work_keeps_the_existing_128_node_limit(tmp_path, entries, complete):
    body = 'for key in (' + ','.join(str(n) for n in range(entries)) + "):\n    table[key] = 'ready'"
    client, rules = install(tmp_path, setup(body))
    assert analyze(client, rules, 'entry').report.at('/complete') is complete
    assert observe(tmp_path)['result'] == 17


def test_saved_startup_rechecks_a_changed_iteration_source(tmp_path):
    from fr_ir.flow_startup import StartupCache

    client, rules = install(tmp_path, setup(BODIES[2], prefix=PREFIX), 'value')
    store = MemoryObjectStore()
    cache = StartupCache(store)
    handle = client.project('find', 'entry').definition_target().handle
    first = cache.analyze(client, handle, standard_library=(), rules=rules, context='html', steps=4096,
                          depth=8, max_bytes=1_048_576, imports=True)
    assert first.checks == 1 and first.answer.report.at('/complete')
    cache = StartupCache.restore(store, cache.persist())
    second = cache.analyze(client, handle, standard_library=(), rules=rules, context='html', steps=4096,
                           depth=8, max_bytes=1_048_576, imports=True)
    assert second.reused and second.checks == 0
    path = tmp_path / 'helpers.py'
    path.write_text(path.read_text().replace("'state': 'ready'", "'state': source()"))
    handle = client.project('find', 'entry').definition_target().handle
    changed = cache.analyze(client, handle, standard_library=(), rules=rules, context='html', steps=4096,
                            depth=8, max_bytes=1_048_576, imports=True)
    assert not changed.reused and changed.checks == 1
    assert not changed.answer.report.at('/complete') and changed.report.at('/startup') is None
    assert evidence(changed.answer) == evidence(analyze(client, rules, 'entry', imports=True))


def test_replacing_a_helper_inside_a_loop_changes_python_but_stays_unfinished(tmp_path):
    client, rules = install(tmp_path, '')
    (tmp_path / 'subject.py').write_text(
        "table = {}\ndef relay(value):\n    return value\n"
        "def register():\n    for key in ('state',):\n        global relay\n"
        "        relay = lambda value: 0\nregister()\n"
        "def entry():\n    return sink(relay(source()))\n")
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path) == {'result': 0, 'raised': None,
                                'events': [['source', 17], ['sink', 0]]}


@pytest.mark.parametrize('change,expected', [('existing', ['ready', None]), ('size', [None, 'RuntimeError'])])
def test_python_dictionary_mutation_does_not_become_checked_iteration(tmp_path, change, expected):
    write = "labels[key] = 'ready'" if change == 'existing' else "labels['extra'] = 'ready'"
    client, rules = install(tmp_path, setup('for key, item in labels.items():\n    ' + write, prefix=PREFIX), 'value')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')
    script = ("import sys,json; sys.path.insert(0,sys.argv[1])\n"
              "try:\n import helpers\n print(json.dumps([helpers.labels['state'], None]))\n"
              "except Exception as error:\n print(json.dumps([None, type(error).__name__]))")
    result = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            check=True, capture_output=True, text=True, timeout=30)
    assert json.loads(result.stdout) == expected
