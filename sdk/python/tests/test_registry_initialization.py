"""Separate creation of callbacks from executing startup code or shared writes."""
import copy
import hashlib
import json

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


ALLOCATIONS = [
    "lambda: sink(source())", "lambda value=0, /, *items, key=None, **options: source()",
    "{'send': lambda value: sink(value)}", "[lambda value: value, (lambda: missing(),)]",
    "dict()", "dict(send=lambda value: sink(value), label='café')",
    "dict({'send': [lambda value=0: source()]}, blocked=(lambda: 0))",
    "{'outer': dict(inner=lambda: missing())}",
]
POPULATIONS = [
    "table['send'] = lambda value: sink(value)",
    "table[('café', -1)] = {'inner': [lambda: missing()]}",
    "table[None] = dict(send=lambda: source())",
    "table.update()", "table.update(send=lambda: source())",
    "table.update({'send': lambda: source()}, count=1)",
    "table.update(dict(send=lambda: source()))",
    "table['send'] = lambda: source()\ntable['send'] = 0",
    "table.update(send=lambda: source())\ntable.update(send=None)",
    "(table).update(label='constant')", "(table)['label'] = 'constant'",
]


def install(tmp_path, setup, route='local', forwards=True, body=None):
    client, rules = workspace(tmp_path)
    helper = setup + "\ndef relay(value):\n    return " + ("value" if forwards else "0") + "\n"
    prelude = helper
    if route != 'local':
        (tmp_path / 'helpers.py').write_text(helper)
        prelude = 'from helpers import relay\n'
        if route == 'value':
            prelude = 'from helpers import table as stored, relay\n'
        elif route == 'reexport':
            (tmp_path / 'bridge.py').write_text('from helpers import table, relay\n')
            prelude = 'from bridge import table, relay\n'
        elif route == 'star':
            (tmp_path / 'helpers.py').write_text("__all__ = ['table', 'relay']\n" + helper)
            prelude = 'from helpers import *\n'
    (tmp_path / 'subject.py').write_text(prelude + 'def entry():\n    ' +
                                       (body or 'return sink(relay(source()))') + '\n')
    return client, rules


@pytest.mark.parametrize('value', ALLOCATIONS)
@pytest.mark.parametrize('route', ['local', 'helper', 'value', 'reexport', 'star'])
@pytest.mark.parametrize('forwards', [False, True])
def test_allocating_unused_callbacks_never_executes_their_bodies(tmp_path, value, route, forwards):
    client, rules = install(tmp_path, 'table = ' + value, route, forwards)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is forwards and not result.initialization.sites
    assert observe(tmp_path) == {'result': 17 if forwards else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17 if forwards else 0]]}
    if route in {'value', 'reexport', 'star'}:
        binding, = result.dependencies.value_bindings
        assert (binding.kind, binding.path, binding.name) == ('allocation', 'helpers.py', 'table')


@pytest.mark.parametrize('population', POPULATIONS)
@pytest.mark.parametrize('declaration', ['{}', 'dict()', "{'send': lambda: 0}"])
@pytest.mark.parametrize('route', ['local', 'value'])
def test_only_locally_owned_dictionary_population_is_inert(tmp_path, population, declaration, route):
    client, rules = install(tmp_path, 'table = ' + declaration + '\n' + population, route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.witnesses and not result.initialization.sites
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17]]


@pytest.mark.parametrize('setup', [
    'table = dict(send=source())', 'table = dict(missing)',
    'table = dict(**{})', "table = dict([('send', 0)])",
    'table = lambda value=source(): value', 'table = lambda value=[]: value',
    'table = lambda value=missing: value', 'table = lambda válue: válue',
    "table = {'send': (lambda: source())()}", 'table = {lambda: 0: 1}',
    "table = {}\ntable['send'] = source()", 'table = {}\ntable[source()] = 0',
    'table = {}\ntable[missing] = 0', 'table = {}\ntable[[]] = 0',
    "table = {}\ntable.update(send=source())", 'table = {}\ntable.update(**{})',
    "table = {}\ntable.update([('send', 0)])", 'table = {}\ntable.update(missing)',
    "table = {'inner': {}}\ntable['inner']['send'] = 0",
    'table = {}\ntable[0, 1] = 0', 'table = {}\ntable[0:1] = 0',
    'table = {}\ntable |= {}', 'table = {}\ndel table[0]',
    'table = {}\ntable.setdefault(0, 1)', 'table = {}\ntable = {}\ntable[0] = 0',
    'table = {}\nother = table\nother[0] = 0',
    'table = {}\nunknown(table)\ntable[0] = 0',
    'table = {}\ndef mutate():\n    pass\nmutate()\ntable[0] = 0',
    'table = {}\nimport unknown\ntable[0] = 0', '__path__ = dict()',
])
@pytest.mark.parametrize('route', ['local', 'helper'])
def test_effects_unknown_objects_and_escaping_references_remain_unchecked(tmp_path, setup, route):
    client, rules = install(tmp_path, setup, route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete')
    assert 'module-effects-unchecked' in result.report.at('/cutoffs')
    assert result.initialization.sites


@pytest.mark.parametrize('prefix', [
    'def dict(**items):\n    sink(source())\n    return {}\n',
    'class dict:\n    pass\n', 'dict = 0\n', '__builtins__ = __builtins__\n',
])
@pytest.mark.parametrize('route', ['local', 'helper'])
def test_constructor_spelling_does_not_hide_a_replaced_builtin(tmp_path, prefix, route):
    client, rules = install(tmp_path, prefix + 'table = dict(send=lambda: 0)', route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete') and result.initialization.sites
    if prefix.startswith('def dict'):
        assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17], ['source', 17], ['sink', 17]]


def test_imported_dictionary_is_shared_and_cannot_be_populated_as_local(tmp_path):
    client, rules = install(tmp_path, 'from helpers import table\ntable[0] = 1')
    (tmp_path / 'helpers.py').write_text('table = {}\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('write', ["table['send'] = 0", 'table.update(send=0)'])
def test_user_dictionary_protocols_cannot_be_hidden_as_local_population(tmp_path, write):
    setup = ('class Custom:\n    def __setitem__(self, key, value):\n        sink(source())\n'
             '    def update(self, **items):\n        sink(source())\ntable = Custom()\n' + write)
    client, rules = install(tmp_path, setup)
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17], ['source', 17], ['sink', 17]]


@pytest.mark.parametrize('body', ["return sink(table['send'](source()))", 'return sink(table(source()))'])
def test_allocations_do_not_grant_callback_execution_or_heap_reads(tmp_path, body):
    client, rules = install(tmp_path, "table = {'send': lambda value: value}", body=body)
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete') and not result.initialization.sites


def test_a_default_can_capture_a_registry_but_cannot_run_it(tmp_path):
    client, rules = install(tmp_path, 'table = dict(send=lambda: source())\ndef unused(value=table):\n    return value')
    assert analyze(client, rules, 'entry').report.at('/complete')
    client, rules = install(tmp_path, 'table = dict(send=lambda: source())\ndef unused(value=table()):\n    return value')
    assert not analyze(client, rules, 'entry').report.at('/complete')


@pytest.mark.parametrize('count,complete', [(122, True), (123, False)])
def test_dictionary_constructor_keeps_the_existing_allocation_node_limit(tmp_path, count, complete):
    client, rules = install(tmp_path, 'table = dict(items=[' + ','.join('0' for _ in range(count)) + '])')
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') is complete
    assert bool(result.initialization.sites) is (not complete)
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('count,complete', [(119, True), (120, False)])
def test_population_counts_the_entire_write_statement(tmp_path, count, complete):
    client, rules = install(tmp_path, 'table = {}\ntable.update(items=[' + ','.join('0' for _ in range(count)) + '])')
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') is complete
    assert bool(result.initialization.sites) is (not complete)
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('extra', ['table.update(send=source())', "table['send'] = source()", 'table = missing'])
def test_population_edits_invalidate_saved_answers(tmp_path, extra):
    client, rules = install(tmp_path, 'table = dict(send=lambda: 0)', 'value')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True)
    assert first.report.at('/complete') and analyze(client, rules, 'entry', cache, imports=True).reused
    path = tmp_path / 'helpers.py'
    path.write_text(path.read_text().replace('def relay', extra + '\ndef relay'))
    result = analyze(client, rules, 'entry', cache, imports=True)
    assert not result.reused and not result.report.at('/complete')
    assert evidence(result) == evidence(analyze(client, rules, 'entry', imports=True))


@pytest.mark.parametrize('mode', ['policy', 'kind', 'offset'])
def test_current_allocation_records_reject_changed_contracts(tmp_path, mode):
    client, rules = install(tmp_path, 'table = dict(send=lambda: 0)', 'value')
    value = copy.deepcopy(analyze(client, rules, 'entry', imports=True).report.to_data())
    if mode == 'policy':
        value['call_binding']['initialization'] = 'Literals only'
        reader = FunctionSummaries
    else:
        binding, = value['inputs']['modules']['value_bindings']
        binding['kind' if mode == 'kind' else 'offset'] = 'unknown' if mode == 'kind' else -1
        value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True,
            ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
        reader = FlowDependencies
    with pytest.raises(FrRuntimeError):
        reader.from_report(FrReport(value, ()))
