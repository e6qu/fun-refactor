"""Compare conversion and handler paths with independent Python execution."""
import json
import subprocess
import sys
from pathlib import Path

import pytest

from fr_ir.runtime import FrClient
from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache

ROOT = Path(__file__).resolve().parents[3]


def analyze(tmp_path, source):
    (tmp_path / 'subject.py').write_text(source)
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'graph-analysis', 'sources': ['read'],
                                'sinks': ['send'], 'sanitizers': {}}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    handle = client.project('find', 'entry').definition_target().handle
    return client.project('dataflow', handle, '--summaries', '--imports',
                          '--standard-library', 'builtins', '--rules', str(rules),
                          '--context', 'html', '--steps', '4096', '--depth', '8',
                          '--bytes', '1048576').to_data()


def execute(source, value):
    driver = ('import json\nsent = []\n'
              f'def read(): return {value!r}\n'
              'def send(value): sent.append(str(value))\n' + source +
              '\nentry()\nprint(json.dumps(sent))\n')
    run = subprocess.run([sys.executable, '-I', '-c', driver], capture_output=True,
                         text=True, check=True, timeout=10)
    return json.loads(run.stdout)


@pytest.mark.parametrize('comparison', ['0 > 1 > clear(values)', '0 < 1 < 0 < clear(values)'])
def test_short_circuited_comparisons_preserve_list_contents(tmp_path, comparison):
    source = ('def clear(items):\n    items[0] = 0\n    return 0\n'
              'def entry():\n    values = [read()]\n'
              f'    result = {comparison}\n    send(values[0])\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('base', ['', ', 10'])
@pytest.mark.parametrize('moved', [False, True])
def test_invalid_number_input_reaches_its_bound_error_handler(tmp_path, base, moved):
    source = (f'def convert(value):\n    return int(str(value){base})\n'
              'def entry():\n    try:\n        convert(read())\n'
              '    except ValueError as problem:\n        send(str(problem))\n')
    if moved:
        source = source.replace('convert', 'parse_customer').replace('problem', 'failure')
    assert execute(source, '123') == []
    assert execute(source, 'bad-input')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']
    assert not report['completion']['may_raise']
    assert any(point['role'] == 'exception-binding'
               for witness in report['witnesses']
               for point in witness['trace']['occurrences'])


def test_nonmatching_handler_does_not_receive_the_conversion_error(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    try:\n        int(str(read()))\n'
                              '    except TypeError as error:\n        send(str(error))\n')
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']
    assert report['completion']['may_raise']


def test_error_binding_is_deleted_after_its_handler(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    error = "old"\n    try:\n'
                              '        raise ValueError(read())\n'
                              '    except ValueError as error:\n        pass\n'
                              '    send(error)\n')
    assert not report['complete']
    assert any('binding:error' in reason or 'unbound:error' in reason for reason in report['cutoffs'])
    assert not report['witnesses']


def test_explicit_base_with_unknown_input_stays_incomplete(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    return int(read(), 10)\n')
    assert not report['complete']
    assert 'integer-conversion-type-unchecked' in report['cutoffs']


def test_scalar_numeric_conversion_keeps_its_exception_boundary(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    return send(int(read()))\n')
    assert report['complete'], report['cutoffs']
    assert report['witnesses']
    assert not report['runtime_effects']['exception_safety_established']
    assert any(site['kind'] == 'implicit-exception' for site in report['effect_limits'])
    assert execute('def entry():\n    send(int(read()))\n', 17) == ['17']


def test_valid_integer_literal_needs_no_error_path(tmp_path):
    report = analyze(tmp_path, 'def entry():\n    try:\n        int(42)\n'
                              '    except ValueError:\n        send(read())\n')
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']
    assert not report['completion']['may_raise']


@pytest.mark.parametrize('body, expected, witness', [
    ('values = [0]\nalias = values\nalias[0] = read()\nsend(values[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('values = []\nalias = values\nalias.append(read())\nsend(values[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias.clear()\nvalues.append(0)\nsend(values[0])', ['0'], False),
    ('values = [read()]\nother = []\nother.extend(values)\nvalues.clear()\nsend(other[0])', ['Customer'], True),
    ('values = [read()]\nalias = values\nalias = [0]\nsend(values[0])', ['Customer'], True),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_list_aliases_match_python(tmp_path, body, expected, witness, renamed):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    if renamed:
        source = source.replace('values', 'customers').replace('alias', 'shared')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('clear, witness', [(False, True), (True, False)])
def test_helper_mutation_reaches_every_alias(tmp_path, clear, witness):
    source = ('def update(items, value):\n    items[0] = value\n'
              'def entry():\n    values = [read()]\n    alias = values\n'
              f'    update(alias, {"0" if clear else "read()"})\n    send(values[0])\n')
    assert execute(source, 'Customer') == (['Customer'] if witness else ['0'])
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('body, cutoff', [
    ('values = []\nsend(values[0])', 'list-index-may-raise'),
    ('values = [read()]\nsend(values[read()])', 'list-index-unchecked'),
    ('values = [read()]\nif read():\n    values = 0\nvalues.clear()', 'list-binding-type-unchecked'),
])
def test_unsupported_list_behavior_stays_incomplete(tmp_path, body, cutoff):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert cutoff in report['cutoffs'], report['cutoffs']


@pytest.mark.parametrize('call', ['replace(values, read())', 'replace(value=read(), items=values)'])
def test_imported_helper_mutation_survives_move_and_alias(tmp_path, call):
    (tmp_path / 'helpers.py').write_text('def update(items, value):\n    items[0] = value\n')
    source = ('from helpers import update as replace\n'
              f'def entry():\n    values = [0]\n    {call}\n    send(values[0])\n')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']
    assert any(point['path'] == 'helpers.py' for witness in report['witnesses']
               for point in witness['trace']['occurrences'])


def test_shared_aliases_remain_shared_inside_the_helper(tmp_path):
    source = ('def update(first, second, value):\n    first.clear()\n    second.append(value)\n'
              'def entry():\n    values = [0]\n    update(values, values, read())\n    send(values[0])\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('call', ['update(values)', 'update(items=values)'])
def test_saved_list_answers_invalidate_after_an_imported_helper_changes(tmp_path, call):
    helper = tmp_path / 'helpers.py'
    helper.write_text('def update(items):\n    items[0] = 0\n')
    source = ('from helpers import update\ndef entry():\n'
              f'    values = [read()]\n    {call}\n    send(values[0])\n')
    analyze(tmp_path, source)
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())

    def query(selected):
        handle = client.project('find', 'entry').definition_target().handle
        return selected.analyze(client, handle, standard_library=('builtins',),
                                rules=tmp_path / 'rules.json', context='html', steps=4096,
                                depth=8, max_bytes=1048576, summaries=True, imports=True)

    cold = query(cache)
    assert cold.report.at('/complete') and not cold.witnesses
    assert query(cache).reused
    helper.write_text('def update(items):\n    pass\n')
    fresh = query(cache)
    assert not fresh.reused
    assert fresh.report.at('/complete') and fresh.witnesses
    clean = query(FlowCache(MemoryObjectStore()))
    assert fresh.witnesses == clean.witnesses


@pytest.mark.parametrize('module', ['helpers', 'relocated_helpers'])
@pytest.mark.parametrize('nested', [False, True])
def test_returned_imported_lists_keep_identity_and_invalidate_saved_answers(tmp_path, module, nested):
    helper = tmp_path / (module + '.py')
    wrapped = '[items]' if nested else 'items'
    helper.write_text(f'def select(*, items):\n    items[0] = 0\n    return {wrapped}\n')
    item = 'selected[0][0]' if nested else 'selected[0]'
    source = (f'from {module} import select as choose\ndef entry():\n'
              '    values = [read()]\n    selected = choose(items=values)\n'
              f'    send({item})\n')
    analyze(tmp_path, source)
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())

    def query(selected):
        handle = client.project('find', 'entry').definition_target().handle
        return selected.analyze(client, handle, standard_library=('builtins',),
                                rules=tmp_path / 'rules.json', context='html', steps=4096,
                                depth=8, max_bytes=1048576, summaries=True, imports=True)

    cold = query(cache)
    assert cold.report.at('/complete') and not cold.witnesses
    assert query(cache).reused
    helper.write_text(f'def select(*, items):\n    return {wrapped}\n')
    changed = query(cache)
    assert not changed.reused
    assert changed.report.at('/complete') and changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    assert any(point['path'] == module + '.py'
               for witness in changed.report.at('/witnesses')
               for point in witness['trace']['occurrences'])


@pytest.mark.parametrize('index', ['0,', '0, 0'])
def test_tuple_indices_are_not_mistaken_for_integer_indices(tmp_path, index):
    report = analyze(tmp_path, f'def entry():\n    values = [read()]\n    send(values[{index}])\n')
    assert not report['complete']
    assert 'list-index-unchecked' in report['cutoffs']


def test_list_calls_keep_keyword_only_parameter_rules(tmp_path):
    source = ('def update(*, items):\n    items.clear()\n'
              'def entry():\n    values = [read()]\n    update(values)\n    send(values[0])\n')
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert 'list-call-signature-unchecked' in report['cutoffs']


@pytest.mark.parametrize('body, expected, witness', [
    ('values = [0, read()]\nsend(values[-1])', ['Customer'], True),
    ('values = [read(), 0]\nvalues[-2] = 0\nsend(values[0])', ['0'], False),
    ('values = [read()]\nsend(values[-0])', ['Customer'], True),
    ('values = [read()]\nsend(values[(+0)])', ['Customer'], True),
    ('values = [0, read()]\nsend(values[0x1])', ['Customer'], True),
    ('values = [0, read()]\nsend(values[-0b1])', ['Customer'], True),
    ('values = [0, read()]\nsend(values[0x_1])', ['Customer'], True),
    ('values = [0, read()]\nalias = values\nsend(alias.pop())\nsend(values[0])', ['Customer', '0'], True),
    ('values = [read(), 0]\nalias = values\nalias.pop(0)\nsend(values[0])', ['0'], False),
    ('values = [read(), 0]\nsend(values.pop(-2))\nsend(values[0])', ['Customer', '0'], True),
    ('values = [0]\nalias = values\nalias.insert(0, read())\nsend(values[0])', ['Customer'], True),
    ('values = [read()]\nvalues.insert(-100, 0)\nsend(values[0])', ['0'], False),
    ('values = [0]\nvalues.insert(100, read())\nsend(values[-1])', ['Customer'], True),
    ('values = [read(), 0]\nvalues.insert(-1, 0)\nsend(values[-2])', ['0'], False),
    ('values = [read(), 0]\nalias = values\nalias.reverse()\nsend(values[0])', ['0'], False),
    ('values = [read(), 0]\nvalues.reverse()\nsend(values[-1])', ['Customer'], True),
    ('values = []\nvalues.reverse()\nvalues.insert(-1, read())\nsend(values.pop())', ['Customer'], True),
    ('values = [read()]\nif read():\n    values.append(0)\nvalues[-1] = 0\nsend(values[0])', ['Customer'], True),
    ('values = [0]\nif read():\n    values.append(read())\nsend(values[-1])', ['Customer'], True),
    ('first = [0]\nsecond = [0, read()]\nvalues = first\nif read():\n    values = second\nsend(values[-1])', ['Customer'], True),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_signed_indices_and_mutations_match_python(tmp_path, body, expected, witness, renamed):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    if renamed:
        source = source.replace('values', 'customers').replace('alias', 'shared')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('parameters, call', [
    ('items, value', 'update(items=values, value=read())'),
    ('items, value', 'update(value=read(), items=values)'),
    ('items, value', 'update(values, value=read())'),
    ('*, items, value', 'update(value=read(), items=values)'),
    ('items, /, *, value', 'update(values, value=read())'),
    ('items, value=0', 'update(items=values)'),
    ('*, items, value=0', 'update(items=values)'),
    ('items, value=0', 'update(value=read(), items=values)'),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_list_helper_binding_matches_python(tmp_path, parameters, call, renamed):
    source = (f'def update({parameters}):\n    items[-1] = value\n'
              f'def entry():\n    values = [read()]\n    {call}\n    send(values[0])\n')
    if renamed:
        source = source.replace('update', 'replace_last').replace('items', 'shared').replace('value', 'customer')
    expected = ['Customer'] if 'read()' in call else ['0']
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is (expected == ['Customer'])


def test_helper_arguments_evaluate_once_in_keyword_order(tmp_path):
    source = ('def update(*, items, value):\n    items[0] = value\n'
              'def entry():\n    values = [0, read()]\n'
              '    update(value=values.pop(), items=values)\n    send(values[0])\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('parameters, call', [
    ('items, /', 'update(items=values)'),
    ('*, items', 'update(values)'),
    ('items', 'update(values, items=values)'),
    ('items', 'update(values, unknown=0)'),
    ('items, value', 'update(items=values)'),
])
def test_invalid_list_helper_bindings_remain_incomplete(tmp_path, parameters, call):
    source = (f'def update({parameters}):\n    items.clear()\n'
              f'def entry():\n    values = [read()]\n    {call}\n    send(values[0])\n')
    with pytest.raises(subprocess.CalledProcessError) as failure:
        execute(source, 'Customer')
    assert 'TypeError' in failure.value.stderr
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert any('binding' in reason or 'signature' in reason for reason in report['cutoffs'])


@pytest.mark.parametrize('body, cutoff', [
    ('values = []\nvalues.pop()', 'list-index-may-raise'),
    ('values = [read()]\nsend(values[-2])', 'list-index-may-raise'),
    ('values = [read()]\nvalues.pop(-2)', 'list-index-may-raise'),
    ('values = [read()]\nvalues.pop(read())', 'list-index-unchecked'),
    ('values = [read()]\nvalues.insert(read(), 0)', 'list-index-unchecked'),
    ('values = [read()]\nvalues.reverse(0)', 'list-method-arguments'),
    ('values = [read()]\nvalues.pop(index=0)', 'list-method-arguments'),
    ('values = [read()]\nif read():\n    values.append(0)\nvalues.pop()', 'list-mutation-target-unchecked'),
    ('values = [read()]\nif read():\n    values.append(0)\nvalues.reverse()', 'list-mutation-target-unchecked'),
    ('values = [read()]\nif read():\n    values.append(0)\nsend(values[-2])', 'list-index-may-raise'),
])
def test_list_extensions_preserve_uncertainty(tmp_path, body, cutoff):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert cutoff in report['cutoffs'], report['cutoffs']


@pytest.mark.parametrize('prelude, body, expected, witness', [
    ('def relay(items):\n    return items\n',
     'values = [read()]\nalias = relay(values)\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('def relay(items):\n    return items\n',
     'values = [0]\nrelay(values)[0] = read()\nsend(values[0])', ['Customer'], True),
    ('def make(value):\n    return [value]\n',
     'values = make(read())\nsend(values[0])', ['Customer'], True),
    ('def make(value):\n    return [value]\n',
     'first = make(read())\nsecond = make(0)\nsecond[0] = 0\nsend(first[0])', ['Customer'], True),
    ('def make(value):\n    return [value]\n',
     'first = make(read())\nsecond = make(0)\nfirst[0] = read()\nsend(second[0])', ['0'], False),
    ('def make(value):\n    return [value]\ndef relay(value):\n    return make(value)\n',
     'send(relay(read())[0])', ['Customer'], True),
    ('def make(value):\n    return [[value]]\n',
     'values = make(read())\nvalues[0][0] = 0\nsend(values[0][0])', ['0'], False),
    ('def relay(*, items):\n    return items[0]\n',
     'values = [[read()]]\nalias = relay(items=values)\nalias.clear()\nalias.append(0)\nsend(values[0][0])', ['0'], False),
    ('', 'values = [read()]\nouter = [values]\nalias = outer[0]\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [0]\nouter = [values]\nouter[0][0] = read()\nsend(values[0])', ['Customer'], True),
    ('', 'outer = [[read()]]\nouter[0] = [0]\nsend(outer[0][0])', ['0'], False),
    ('', 'values = [read()]\nouter = []\nouter.append(values)\nvalues[0] = 0\nsend(outer[0][0])', ['0'], False),
    ('', 'values = [0]\nouter = []\nouter.insert(0, values)\nouter[0][0] = read()\nsend(values[0])', ['Customer'], True),
    ('', 'values = [read()]\nouter = [values]\ncopy = []\ncopy.extend(outer)\ncopy[0][0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [read()]\nouter = [values]\nalias = outer.pop()\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [read()]\nouter = [values, [0]]\nouter.reverse()\nouter[-1][0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [read()]\nvalues.append(values)\nvalues[1][0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [read()]\nvalues.insert(0, values)\nvalues[0][-1] = 0\nsend(values[-1])', ['0'], False),
    ('', 'values = [[read()]]\nalias = values[0]\nvalues[0] = [0]\nsend(alias[0])', ['Customer'], True),
    ('', 'values = [[read()]]\nalias = values[0]\nvalues.clear()\nsend(alias[0])', ['Customer'], True),
    ('', 'values = [[read()]]\nalias = values[0]\nalias = [0]\nsend(values[0][0])', ['Customer'], True),
    ('', 'values = [read()]\nouter = [values]\nvalues = [0]\nsend(outer[0][0])', ['Customer'], True),
    ('def replace(items):\n    items[0] = 0\n    return 0\n',
     'values = [read()]\nouter = [values, replace(values)]\nsend(outer[0][0])', ['0'], False),
    ('', 'values = [read()]\nouter = [values, [0]]\nouter.pop()[0] = outer.pop()[0]\nsend(values[0])', ['0'], False),
    ('', 'values = [0]\nouter = [values, [read()]]\nouter.pop(0).append(outer.pop()[0])\nsend(values[1])', ['Customer'], True),
    ('def relay(items):\n    try:\n        return items\n    finally:\n        items[0] = 0\n',
     'values = [read()]\nsend(relay(values)[0])', ['0'], False),
    ('def relay(items):\n    try:\n        return items\n    finally:\n        items = [0]\n',
     'values = [read()]\nsend(relay(values)[0])', ['Customer'], True),
    ('def relay(items):\n    try:\n        return items\n    finally:\n        return [0]\n',
     'values = [read()]\nsend(relay(values)[0])', ['0'], False),
    ('def relay(items):\n    try:\n        return [items]\n    finally:\n        items[0] = 0\n',
     'values = [read()]\nsend(relay(values)[0][0])', ['0'], False),
    ('def relay(items):\n    try:\n        return items[0]\n    finally:\n        items[0] = 0\n',
     'values = [read()]\nsend(relay(values))', ['Customer'], True),
    ('def relay(items):\n    try:\n        return items\n    finally:\n        try:\n            pass\n        finally:\n            items[0] = 0\n',
     'values = [read()]\nsend(relay(values)[0])', ['0'], False),
    ('def replace(items):\n    items[0] = 0\n    return items\n',
     'values = [read()]\nalias = replace(values) if True else values\nsend(alias[0])', ['0'], False),
    ('def replace(items):\n    items[0] = 0\n    return items\n',
     'values = [read()]\nalias = values if False else replace(values)\nsend(alias[0])', ['0'], False),
    ('', 'values = [read()]\nalias = False or values\nalias[0] = 0\nsend(values[0])', ['0'], False),
    ('', 'values = [read()]\nalias = True and values\nalias[0] = 0\nsend(values[0])', ['0'], False),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_returned_and_nested_list_identities_match_python(tmp_path, prelude, body, expected, witness, renamed):
    source = prelude + 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    if renamed:
        source = source.replace('values', 'customers').replace('items', 'shared').replace('relay', 'forward')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('selector', [
    'replace(values) if read() else values',
    'values if read() else replace(values)',
    'read() and replace(values)',
    'read() or replace(values)',
])
def test_conditional_mutation_preserves_the_unexecuted_alternative(tmp_path, selector):
    source = ('def replace(items):\n    items[0] = 0\n    return items\n'
              f'def entry():\n    values = [read()]\n    {selector}\n    send(values[0])\n')
    execute(source, 'Customer')
    execute(source, '')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('body, cutoff', [
    ('values = [[read()]]\nsend(values[0][2])', 'list-index-may-raise'),
    ('values = [[read()]]\nvalues[0] = 0\nsend(values[0][0])', 'unsupported-expression:subscript'),
    ('values = [[read()]]\nif read():\n    values[0] = 0\nvalues[0].clear()', 'list-binding-type-unchecked'),
    ('values = [[read()]]\nvalues[0][read()] = 0', 'list-index-unchecked'),
    ('values = [*[read()]]\nsend(values[0])', 'list-unpacking-unchecked'),
    ('values = []\nwhile read():\n    values.append([read()])', 'repeated-list-allocation'),
])
def test_nested_list_boundaries_remain_visible(tmp_path, body, cutoff):
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert cutoff in report['cutoffs'], report['cutoffs']


def test_recursive_list_helpers_do_not_reuse_scalar_summaries(tmp_path):
    source = ('def relay(items):\n    if read():\n        return relay(items)\n    return items\n'
              'def entry():\n    values = [read()]\n    send(relay(values)[0])\n')
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert 'list-call-depth-budget' in report['cutoffs']


def test_nested_allocation_keeps_the_existing_object_budget(tmp_path):
    body = '\n'.join(f'value{index} = []' for index in range(64)) + '\nouter = [value0]\n'
    source = 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert 'list-object-budget' in report['cutoffs']
