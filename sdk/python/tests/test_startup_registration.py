"""Compare bounded registration calls and unsafe startup changes with Python."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from test_flow_summaries import analyze, evidence
from test_postponed_annotations import observe
from test_registry_initialization import install


def setup(body, tail='register()', signature='', prefix='table = {}\n'):
    return (prefix + 'def register(' + signature + '):\n' +
            ''.join('    ' + line + '\n' for line in body.splitlines()) + tail)


@pytest.mark.parametrize('body', [
    "table['label'] = 'ready'",
    "table.update(send=lambda value: unknown(value), label='ready')",
    "table[('send', None)] = dict(inner=lambda: missing())",
    "pass\ntable.update(label='ready')\nreturn",
    "table.update(label='ready')\nreturn None",
    "table['label'] = 'first'\ntable['label'] = 'ready'",
])
@pytest.mark.parametrize('route', ['local', 'value', 'reexport', 'star'])
@pytest.mark.parametrize('forwards', [True, False])
def test_checked_call_preserves_both_flow_outcomes(tmp_path, body, route, forwards):
    client, rules = install(tmp_path, setup(body, 'register()\nregister()'), route, forwards)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {'result': 17 if forwards else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17 if forwards else 0]]}


@pytest.mark.parametrize('body', [
    "table['label'] = source()", "table.update(label=source())",
    "table[source()] = 0", "table[[]] = 0", "table[0, 1] = 0",
    "table['inner']['label'] = 0", "table.setdefault('label', 0)",
    'register()', 'unknown()', 'raise RuntimeError()',
    'yield 0', "if True:\n    table['label'] = 0",
    "for key in ():\n    table[key] = 0", "table['label'] = 0\nreturn 17",
    "return\ntable['label'] = 0", "table = {}\ntable['label'] = 0",
    "global table\ntable = {}", "del table['label']",
    "table.update(**{})", "table['label'] = (lambda: source())()",
    "table.update(send=lambda value=source(): value)",
    "table['label'] = 0\nregister = lambda: None",
])
@pytest.mark.parametrize('route', ['local', 'helper'])
def test_unknown_calls_control_flow_and_writes_stay_unfinished(tmp_path, body, route):
    client, rules = install(tmp_path, setup(body), route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete')
    assert 'module-effects-unchecked' in result.report.at('/cutoffs')
    assert any('startup-call' in site.reasons for site in result.initialization.sites)


@pytest.mark.parametrize('signature,call', [
    ('value', 'register()'), ('value=0', 'register()'),
    ('*unused', 'register()'), ('**unused', 'register()'),
    ('', 'register(0)'), ('', 'register(value=0)'),
    ('', 'register(*())'), ('', 'register(**{})'),
])
def test_missing_non_dictionary_and_variadic_arguments_stay_unfinished(tmp_path, signature, call):
    client, rules = install(tmp_path, setup("table['label'] = 0", call, signature))
    assert not analyze(client, rules, 'entry').report.at('/complete')


@pytest.mark.parametrize('intervening', [
    'alias = table', 'expose(table)', 'import unused',
    'register = lambda: source()', 'alias = register',
    'def register():\n    sink(source())',
    'table = {}', 'table = 0', 'del table',
    'dict = lambda **items: sink(source())',
])
def test_late_bindings_and_escape_discard_the_checked_connection(tmp_path, intervening):
    source = setup('table.update(label=dict())', intervening + '\nregister()')
    client, rules = install(tmp_path, source, 'helper')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')


def test_imported_table_is_shared_even_inside_a_local_function(tmp_path):
    client, rules = install(tmp_path, setup("table['label'] = 0", prefix='from shared import table\n'))
    (tmp_path / 'shared.py').write_text('table = {}\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete') and observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('edit', ['body', 'call', 'allocation'])
def test_source_edits_invalidate_a_saved_startup_answer(tmp_path, edit):
    client, rules = install(tmp_path, setup("table['label'] = 0"), 'value')
    store = MemoryObjectStore()
    cache = FlowCache(store)
    first = analyze(client, rules, 'entry', cache, imports=True)
    assert first.report.at('/complete')
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    path = tmp_path / 'helpers.py'
    old, new = {'body': ("table['label'] = 0", "table['label'] = source()"),
                'call': ('\nregister()\n', '\nregister(0)\n'),
                'allocation': ('table = {}', 'table = missing()')}[edit]
    source = path.read_text()
    assert source.count(old) == 1
    path.write_text(source.replace(old, new, 1))
    result = analyze(client, rules, 'entry', cache, imports=True)
    assert not result.reused and not result.report.at('/complete')
    assert 'import-syntax-errors' not in result.report.at('/cutoffs')
    assert evidence(result) == evidence(analyze(client, rules, 'entry', imports=True))
    expected = {'body': {'result': 17, 'raised': None,
                         'events': [['source', 17], ['source', 17], ['sink', 17]]},
                'call': {'result': None, 'raised': 'TypeError', 'events': []},
                'allocation': {'result': None, 'raised': 'NameError', 'events': []}}
    assert observe(tmp_path) == expected[edit]


def test_startup_helper_replacement_cannot_be_hidden_by_a_checked_call(tmp_path):
    source = ("table = {}\ndef relay(value):\n    return value\n"
              "def stopped(value):\n    return 0\n"
              "def register():\n    global relay\n    relay = stopped\n"
              "register()\ndef entry():\n    return sink(relay(source()))\n")
    client, rules = install(tmp_path, '')
    (tmp_path / 'subject.py').write_text(source)
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path) == {'result': 0, 'raised': None,
                                'events': [['source', 17], ['sink', 0]]}


def test_python_observes_the_registration_and_unused_callback_stays_deferred(tmp_path):
    client, rules = install(tmp_path, setup("table.update(label='ready', send=lambda: missing())"), 'value')
    assert analyze(client, rules, 'entry', imports=True).report.at('/complete')
    script = ("import sys,json; sys.path.insert(0,sys.argv[1]); import helpers; "
              "print(json.dumps([helpers.table['label'], sorted(helpers.table), callable(helpers.table['send'])]))")
    actual = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            check=True, capture_output=True, text=True, timeout=30)
    assert json.loads(actual.stdout) == ['ready', ['label', 'send'], True]


@pytest.mark.parametrize('count,complete', [(1, True), (128, False)])
def test_whole_function_uses_the_existing_initializer_node_bound(tmp_path, count, complete):
    client, rules = install(tmp_path, setup("table['label'] = 0\n" + 'pass\n' * count))
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') is complete
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('change', ['async', 'decorated', 'before-definition'])
def test_definition_and_execution_order_are_required(tmp_path, change):
    text = setup("table['label'] = 0")
    if change == 'async':
        text = text.replace('def register', 'async def register')
    elif change == 'decorated':
        text = text.replace('def register', '@missing\ndef register')
    else:
        text = 'register()\n' + text
    client, rules = install(tmp_path, text)
    assert not analyze(client, rules, 'entry').report.at('/complete')


def test_complete_tutorial_checks_startup_and_then_rejects_helper_replacement(tmp_path):
    root = Path(__file__).resolve().parents[3]
    guide = (root / 'docs/check-python-startup.md').read_text()
    programs = re.findall(r'```python\n(.*?)```', guide, re.S)
    assert len(programs) == 5
    for name, source in zip(('helpers.py', 'app.py', 'check_example.py', 'run_example.py'), programs):
        (tmp_path / name).write_text(source)
    (tmp_path / 'rules.json').write_text(re.findall(r'```json\n(.*?)```', guide, re.S)[0])
    environment = dict(os.environ, PATH=str(root / 'target/debug') + os.pathsep + os.environ['PATH'],
                       PYTHONPATH=str(root / 'sdk/python/src'))
    def run(name):
        result = subprocess.run([sys.executable, '-B', name], cwd=tmp_path, env=environment,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        return result.stdout
    assert run('check_example.py') == 'Finished: True\nPossible input connection: True\n'
    assert run('run_example.py') == 'ready\nTrue\n17\n'
    helper = tmp_path / 'helpers.py'
    changed = programs[0].replace('    output.update(send=lambda: missing())\n',
                                  '    output.update(send=lambda: missing())\n' + programs[4])
    changed = changed.replace('\nregister(labels)\n', '\n') + '\nregister(labels)\n'
    helper.write_text(changed)
    assert run('check_example.py').startswith('Finished: False\nPossible input connection:')
    assert run('run_example.py') == 'ready\nTrue\n0\n'
