"""Check dictionary arguments, stored references and aliasing against Python."""
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
COPY = 'for key, item in inputs.items():\n    output[key] = item'


@pytest.mark.parametrize('signature,call', [
    ('inputs, output', 'register(labels, table)'),
    ('inputs, output', 'register(output=table, inputs=labels)'),
    ('inputs, output', 'register(labels, output=table)'),
    ('inputs=labels, output=table', 'register()'),
    ('inputs, output=table', 'register(labels)'),
    ('inputs=table, output=labels', 'register(output=table, inputs=labels)'),
])
@pytest.mark.parametrize('route', ['local', 'value', 'reexport', 'star'])
@pytest.mark.parametrize('forwards', [True, False])
def test_dictionary_arguments_and_defaults_preserve_answers(tmp_path, signature, call, route, forwards):
    client, rules = install(tmp_path, setup(COPY, call + '\n' + call, signature, PREFIX), route, forwards)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {'result': 17 if forwards else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17 if forwards else 0]]}


@pytest.mark.parametrize('body,signature,call', [
    ("table['state'] = 'ready'", 'table', 'register(table)'),
    ("table.update(state='ready')", 'table=table', 'register()'),
    ("first['state'] = 'ready'\nsecond.update(count=2)", 'first, second', 'register(table, table)'),
    ("for key in ('state',):\n    output[key] = dict()", 'output', 'register(table)'),
    ('for key, item in table.items():\n    labels[key] = item',
     'table=labels, labels=table', 'register()'),
])
def test_parameter_shadowing_and_safe_alias_writes(tmp_path, body, signature, call):
    client, rules = install(tmp_path, setup(body, call, signature, PREFIX), 'value')
    assert analyze(client, rules, 'entry', imports=True).report.at('/complete')
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('signature,call', [
    ('inputs, output', 'register(labels)'),
    ('inputs, output', 'register(labels, table, table)'),
    ('inputs, output', 'register(labels, inputs=table)'),
    ('inputs, output', 'register(labels, missing=table)'),
    ('inputs, output', 'register(*[labels, table])'),
    ('inputs, output', "register(**{'inputs': labels, 'output': table})"),
    ('inputs, output', 'register(labels, {})'),
    ('inputs, output', 'register(labels, dict())'),
    ('inputs, output', 'register(labels, source())'),
    ('inputs, output', 'register(labels, 0)'),
    ('inputs, output', 'register(labels, missing)'),
    ('inputs, output={}', 'register(labels)'),
    ('inputs=labels, output=None', 'register()'),
    ('inputs=labels, output=source()', 'register()'),
    ('inputs, /, output', 'register(labels, table)'),
    ('inputs, *, output', 'register(labels, output=table)'),
    ('inputs, **output', 'register(labels, output=table)'),
    ('inputs, *output', 'register(labels, table)'),
    ('inputs: dict, output', 'register(labels, table)'),
])
def test_unchecked_binding_and_argument_evaluation_stay_unfinished(tmp_path, signature, call):
    client, rules = install(tmp_path, setup(COPY, call, signature, PREFIX), 'value')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')


@pytest.mark.parametrize('body,signature,call', [
    (COPY, 'inputs, output', 'register(labels, labels)'),
    (COPY, 'inputs=labels, output=labels', 'register()'),
    (COPY + "\nlabels['state'] = 'changed'", 'inputs, output', 'register(labels, table)\nregister(labels, table)'),
    (COPY + "\ninputs['state'] = 'changed'", 'inputs, output', 'register(labels, table)\nregister(labels, table)'),
    ("for key, item in inputs.items():\n    labels[key] = item", 'inputs', 'register(labels)'),
    ("first['state'] = 'changed'\nfor key, item in second.items():\n    table[key] = item",
     'first, second', 'register(labels, labels)'),
    ('for key, item in first.items():\n    for other, value in second.items():\n        table[other] = value',
     'first, second', 'register(labels, labels)'),
    ("output['state'] = source()", 'output', 'register(table)'),
    ("output['state'] = inputs", 'inputs, output', 'register(labels, table)'),
    ("output = labels\noutput['state'] = 0", 'output', 'register(table)'),
    ("for inputs in ('state',):\n    output[inputs] = 0", 'inputs, output', 'register(labels, table)'),
    ("output['state'] = dict()", 'dict, output', 'register(labels, table)'),
    ("for key in ('state',):\n    output[key] = 0\n    global relay\n    relay = lambda value: 0",
     'output', 'register(table)'),
])
def test_aliases_local_scope_and_unknown_effects_do_not_hide_startup(tmp_path, body, signature, call):
    client, rules = install(tmp_path, setup(body, call, signature, PREFIX), 'value')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete') and result.initialization.sites


def dictionary_observation(tmp_path):
    script = ("import sys,json; sys.path.insert(0,sys.argv[1]); import helpers; "
              "print(json.dumps([helpers.table, helpers.labels]))")
    result = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            check=True, capture_output=True, text=True, timeout=30)
    return json.loads(result.stdout)


def test_an_explicit_argument_replaces_the_stored_reference(tmp_path):
    client, rules = install(tmp_path, setup("output['state'] = 'ready'", 'register(table)',
                                           'output=labels', PREFIX), 'value')
    assert analyze(client, rules, 'entry', imports=True).report.at('/complete')
    assert dictionary_observation(tmp_path) == [{'state': 'ready'}, {'state': 'ready', 'count': 2}]


def test_passing_an_imported_dictionary_does_not_make_it_owned(tmp_path):
    source = setup("output['state'] = 'changed'", 'register(table)', 'output',
                   'from shared import table\n')
    client, rules = install(tmp_path, source, 'value')
    (tmp_path / 'shared.py').write_text('table = {}\n')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')
    assert observe(tmp_path)['result'] == 17


def test_evaluated_argument_stops_python_before_the_reported_application_source(tmp_path):
    client, rules = install(tmp_path, setup("output['state'] = 'ready'", 'register(source())', 'output'))
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path) == {'result': None, 'raised': 'TypeError', 'events': [['source', 17]]}
    occurrence = result.report.at('/witnesses')[0]['trace']['occurrences'][0]
    span = occurrence['location']['span']
    source = (tmp_path / 'subject.py').read_text()
    assert source[span['start']:span['end']] == 'source()'
    assert span['start'] > source.index('def entry():')


def test_rebinding_does_not_move_a_python_default_to_the_new_object(tmp_path):
    source = setup("output['state'] = 'stored'", "table = {}\nregister()", 'output=table', PREFIX)
    client, rules = install(tmp_path, source, 'value')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')
    script = ("import sys,json; sys.path.insert(0,sys.argv[1]); import helpers; "
              "print(json.dumps([helpers.table, helpers.register.__defaults__[0]]))")
    result = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            check=True, capture_output=True, text=True, timeout=30)
    assert json.loads(result.stdout) == [{}, {'state': 'stored'}]


def test_a_default_reads_later_mutations_not_a_copied_literal(tmp_path):
    source = setup(COPY, "labels['state'] = 'changed'\nregister()", 'inputs=labels, output=table', PREFIX)
    client, rules = install(tmp_path, source, 'value')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')
    assert dictionary_observation(tmp_path) == [{'state': 'changed', 'count': 2}, {'state': 'changed', 'count': 2}]


@pytest.mark.parametrize('entries,complete', [(2, True), (20, False)])
def test_parameter_loops_keep_the_expanded_work_limit(tmp_path, entries, complete):
    body = 'for key in (' + ','.join(str(n) for n in range(entries)) + "):\n    output[key] = 'ready'"
    client, rules = install(tmp_path, setup(body, 'register(table)', 'output', PREFIX))
    assert analyze(client, rules, 'entry').report.at('/complete') is complete
    assert observe(tmp_path)['result'] == 17


def test_saved_startup_rechecks_an_aliased_dictionary_argument(tmp_path):
    from fr_ir.flow_startup import StartupCache

    client, rules = install(tmp_path, setup(COPY, 'register(labels)', 'inputs, output=table', PREFIX), 'value')
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
    path.write_text(path.read_text().replace('register(labels)', 'register(labels, labels)'))
    handle = client.project('find', 'entry').definition_target().handle
    changed = cache.analyze(client, handle, standard_library=(), rules=rules, context='html', steps=4096,
                            depth=8, max_bytes=1_048_576, imports=True)
    assert not changed.reused and changed.checks == 1
    assert not changed.answer.report.at('/complete') and changed.report.at('/startup') is None
    assert evidence(changed.answer) == evidence(analyze(client, rules, 'entry', imports=True))


@pytest.mark.parametrize('edit', ['argument', 'default', 'alias'])
def test_saved_answers_recheck_dictionary_binding_edits(tmp_path, edit):
    client, rules = install(tmp_path, setup(COPY, 'register(labels)', 'inputs, output=table', PREFIX), 'value')
    store = MemoryObjectStore()
    cache = FlowCache(store)
    assert analyze(client, rules, 'entry', cache, imports=True).report.at('/complete')
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    old, new = {'argument': ('register(labels)', 'register(source())'),
                'default': ('output=table', 'output=source()'),
                'alias': ('register(labels)', 'register(labels, labels)')}[edit]
    path = tmp_path / 'helpers.py'
    path.write_text(path.read_text().replace(old, new))
    result = analyze(client, rules, 'entry', cache, imports=True)
    assert not result.reused and not result.report.at('/complete')
    assert evidence(result) == evidence(analyze(client, rules, 'entry', imports=True))
