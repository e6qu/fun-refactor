"""Compare list failures, cleanup and shared mutations with ordinary Python."""
import json
import subprocess
import sys

import pytest

from test_graph_analysis import ROOT, analyze, execute
from test_list_sequences import program


FAILURES = [
    ('values[9]', 'IndexError'),
    ('values[-2]', 'IndexError'),
    ('values[9] = read()', 'IndexError'),
    ('values[-2] = 0', 'IndexError'),
    ('del values[9]', 'IndexError'),
    ('values.pop(9)', 'IndexError'),
    ('values.pop(-2)', 'IndexError'),
    ('values.clear(); values.pop()', 'IndexError'),
    ('values[::0]', 'ValueError'),
    ('values[::0] = []', 'ValueError'),
    ('del values[::0]', 'ValueError'),
    ('values[::2] = []', 'ValueError'),
    ('values[::-1] = [0, 0]', 'ValueError'),
]


@pytest.mark.parametrize('operation, kind', FAILURES)
@pytest.mark.parametrize('renamed', [False, True])
def test_known_failures_skip_later_work_and_reach_the_handler(tmp_path, operation, kind, renamed):
    source = program('values = [read()]\ntry:\n    ' + operation +
                     f'\n    send(read())\nexcept {kind}:\n    send("handled")', renamed=renamed)
    assert execute(source, 'Customer') == ['handled']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']
    assert report['completion'] == {'normal_return': True, 'may_raise': False}
    errors = [event for event in report['events'] if event['kind'] == 'list-error']
    assert errors and all(event['exception'] == kind and event['certain'] for event in errors)
    for event in errors:
        location = event['occurrence']['location']['span']
        fragment = source.encode()[location['start']:location['end']].decode()
        assert fragment and ('values' in fragment or 'customers' in fragment)


@pytest.mark.parametrize('handler', ['IndexError', 'LookupError', 'Exception', 'BaseException',
                                      '(ValueError, LookupError)'])
def test_builtin_hierarchy_and_handler_order(tmp_path, handler):
    source = program('values = []\ntry:\n    values.pop()\nexcept TypeError:\n    send("wrong")\n'
                     f'except {handler}:\n    send(read())')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses'] and not report['completion']['may_raise']


@pytest.mark.parametrize('operation, kind', FAILURES)
def test_uncaught_failures_are_complete_exceptional_exits(tmp_path, operation, kind):
    source = program('values = [read()]\n' + operation + '\nsend(read())')
    with pytest.raises(subprocess.CalledProcessError) as failure:
        execute(source, 'Customer')
    assert kind in failure.value.stderr
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['completion'] == {'normal_return': False, 'may_raise': True}
    assert set(report['exceptional_returns']) == {kind}
    assert not report['witnesses']


@pytest.mark.parametrize('operation', ['values[9]', 'values.pop(9)', 'values[::0]', 'values[::2] = []'])
def test_builtin_error_messages_do_not_contain_the_list_element(tmp_path, operation):
    source = program('values = [read()]\ntry:\n    ' + operation +
                     '\nexcept Exception as error:\n    send(str(error))')
    assert 'Customer' not in str(execute(source, 'Customer'))
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


@pytest.mark.parametrize('change, expected, witness', [
    ('items[0] = 0', ['0'], False),
    ('items[0] = value', ['Customer'], True),
])
@pytest.mark.parametrize('failure', ['items[9]', 'items.pop(9)', 'items[::0]', 'items[::2] = []',
                                    'raise IndexError("failed")', 'raise ValueError("failed")'])
def test_helper_mutations_survive_failure_without_restoring_old_values(tmp_path, change, expected, witness, failure):
    prelude = f'def relay(items, value):\n    {change}\n    {failure}\n    items[0] = "unreached"\n'
    source = program('values = [read() if False else 0]\nalias = values\ntry:\n'
                     '    relay(values, read())\nexcept Exception:\n    send(alias[0])', prelude)
    if not witness:
        source = source.replace('read() if False else 0', 'read()')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('operation, expected', [
    ('values[9] = values.pop()', ['[]']),
    ('values[::0] = [values.pop()]', ['[]']),
    ('del values[0], values[9]', ['[0]']),
    ('del values[:], values[0]', ['[]']),
    ('values[::2] = [values.pop(), 0]', ['[0]']),
])
def test_side_effects_before_a_failed_store_or_delete_are_retained(tmp_path, operation, expected):
    initial = '[0, read()]' if operation.endswith('[values.pop(), 0]') else '[read(), 0]' if operation.startswith('del values[0],') else '[read()]'
    source = program(f'values = {initial}\nalias = values\ntry:\n    {operation}\n'
                     'except Exception:\n    send(alias)')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


@pytest.mark.parametrize('cleanup, expected, witness', [
    ('items[0] = 0', ['0'], False),
    ('items[0] = value', ['Customer'], True),
])
def test_cleanup_runs_before_the_caller_catches_a_list_failure(tmp_path, cleanup, expected, witness):
    prelude = ('def relay(items, value):\n    try:\n        return items[9]\n'
               f'    finally:\n        {cleanup}\n')
    source = program('values = [read()]\nalias = values\ntry:\n    relay(values, read())\n'
                     'except IndexError:\n    send(alias[0])', prelude)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


def test_cleanup_return_replaces_the_failed_list_read(tmp_path):
    prelude = ('def relay(items):\n    try:\n        return items[9]\n'
               '    finally:\n        items[0] = 0\n        return items\n')
    source = program('values = [read()]\nsend(relay(values)[0])', prelude)
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses'] and not report['completion']['may_raise']


def test_cleanup_error_replaces_a_pending_return_and_reraise_keeps_the_error(tmp_path):
    prelude = ('def relay(items):\n    try:\n        return items\n'
               '    finally:\n        try:\n            items.pop(9)\n'
               '        except LookupError:\n            items[0] = 0\n            raise\n')
    source = program('values = [read()]\ntry:\n    send(relay(values))\n'
                     'except IndexError:\n    send(values[0])', prelude)
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses'] and not report['completion']['may_raise']


@pytest.mark.parametrize('expression', ['relay(values) if read() else 0',
                                       'read() and relay(values)', 'read() or relay(values)',
                                       '0 < 1 < relay(values)'])
def test_failed_expression_branch_does_not_pollute_successful_branch(tmp_path, expression):
    prelude = 'def relay(items):\n    items[0] = 0\n    items.pop(9)\n'
    source = program(f'values = [read()]\ntry:\n    result = {expression}\n'
                     'except IndexError:\n    send(values[0])', prelude)
    assert 'Customer' not in str(execute(source, 'Customer'))
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


def test_possible_index_failure_retains_both_completion_paths(tmp_path):
    source = program('values = [read()]\nif read():\n    values.append(0)\ntry:\n'
                     '    send(values[-2])\nexcept IndexError:\n    send("empty")')
    assert execute(source, 'Customer') == ['Customer']
    assert execute(source, '') == ['empty']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses'] and not report['completion']['may_raise']
    assert any(event['kind'] == 'list-error' and not event['certain'] for event in report['events'])


def test_normal_helper_exit_does_not_pollute_its_exceptional_exit(tmp_path):
    prelude = ('def relay(items, value, flag):\n    if flag:\n'
               '        items[0] = value\n        return items\n'
               '    items[0] = 0\n    items.pop(9)\n')
    source = program('values = [0]\ntry:\n    relay(values, read(), read())\n'
                     'except IndexError:\n    send(values[0])', prelude)
    assert execute(source, 'Customer') == []
    assert execute(source, '') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


def test_nested_shared_list_failure_preserves_inner_alias_mutation(tmp_path):
    prelude = 'def relay(items):\n    items[0][0] = 0\n    return items[0][2]\n'
    source = program('inner = [read()]\nvalues = [inner]\ntry:\n    relay(values)\n'
                     'except IndexError:\n    send(inner[0])', prelude)
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


@pytest.mark.parametrize('operation', ['values[relay(values)]', 'values[relay(values)] = 0',
                                      'del values[relay(values)]', 'values.pop(relay(values))',
                                      'values[relay(values):]', 'values[::relay(values)] = []'])
def test_failure_while_evaluating_the_index_stops_the_outer_operation(tmp_path, operation):
    prelude = 'def relay(items):\n    items[0] = 0\n    items.pop(9)\n'
    source = program(f'values = [read()]\ntry:\n    {operation}\n'
                     'except IndexError:\n    send(values[0])', prelude)
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


def test_failed_replacement_does_not_evaluate_the_assignment_target(tmp_path):
    prelude = ('def relay(items):\n    items[0] = 0\n    items.pop(9)\n'
               'def target(items, value):\n    items[0] = value\n    return items\n')
    source = program('values = [read()]\ntry:\n    target(values, read())[0] = relay(values)\n'
                     'except IndexError:\n    send(values[0])', prelude)
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


def test_nonmatching_handler_leaves_index_error_uncaught(tmp_path):
    source = program('values = []\ntry:\n    values.pop()\nexcept ValueError:\n    send(read())')
    with pytest.raises(subprocess.CalledProcessError) as failure:
        execute(source, 'Customer')
    assert 'IndexError' in failure.value.stderr
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses'] and report['completion']['may_raise']


def test_shadowed_index_error_is_not_treated_as_a_builtin_handler(tmp_path):
    source = program('IndexError = read()\nvalues = []\ntry:\n    values.pop()\n'
                     'except IndexError:\n    send(read())')
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert 'exception-handler-type-unchecked' in report['cutoffs']


@pytest.mark.parametrize('module', ['helpers', 'moved_helpers'])
def test_imported_error_mutations_invalidate_saved_answers(tmp_path, module):
    from fr_ir.runtime import FrClient
    from fr_ir.context import MemoryObjectStore
    from fr_ir.flow import FlowCache

    helper = tmp_path / (module + '.py')
    helper.write_text('def relay(items):\n    items[0] = 0\n    items.pop(9)\n')
    source = program('values = [read()]\ntry:\n    relay(values)\n'
                     'except IndexError:\n    send(values[0])', f'from {module} import relay\n')
    analyze(tmp_path, source)
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())

    def runtime():
        driver = ('import sys,json\n' + f'sys.path.insert(0, {str(tmp_path)!r})\n'
                  'sent=[]\ndef read(): return "Customer"\ndef send(value): sent.append(value)\n'
                  + source + '\nentry()\nprint(json.dumps(sent))\n')
        return json.loads(subprocess.check_output([sys.executable, '-I', '-B', '-c', driver], text=True, timeout=10))

    def query(selected):
        handle = client.project('find', 'entry').definition_target().handle
        return selected.analyze(client, handle, standard_library=('builtins',),
                                rules=tmp_path / 'rules.json', context='html', steps=4096,
                                depth=8, max_bytes=1048576, summaries=True, imports=True)

    assert runtime() == [0]
    cold = query(cache)
    assert cold.report.at('/complete') and not cold.witnesses
    assert query(cache).reused
    helper.write_text('def relay(items):\n    items.pop(9)\n')
    assert runtime() == ['Customer']
    changed = query(cache)
    assert not changed.reused
    assert changed.report.at('/complete') and changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    assert any(event['kind'] == 'list-error' and event['occurrence']['path'] == module + '.py'
               for event in changed.report.at('/events'))
