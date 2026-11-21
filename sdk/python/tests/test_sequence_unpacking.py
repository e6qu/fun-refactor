"""Compare sequence binding, argument expansion and errors with Python execution."""
import copy

import pytest

from fr_ir.runtime import FrReport, FrRuntimeError
from fr_ir.flow_summaries import FunctionSummaries
from test_graph_analysis import analyze, execute
from test_list_sequences import program


@pytest.mark.parametrize('expression', ['[read(), 0]', '(read(), 0)', 'read(), 0'])
@pytest.mark.parametrize('target', ['first, second', '[first, second]', '(first, second)'])
@pytest.mark.parametrize('renamed', [False, True])
def test_literal_and_stored_sequences_unpack(tmp_path, expression, target, renamed):
    source = program(f'values = {expression}\n{target} = values\nsend(first)\nsend(second)', renamed=renamed)
    assert execute(source, 'Customer') == ['Customer', '0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert len(report['witnesses']) == 1


@pytest.mark.parametrize('body, expected, witness', [
    ('inner = [read()]\nvalues = [inner, 0]\nalias, other = values\nalias[0] = 0\nsend(inner[0])', ['0'], False),
    ('inner = [read()]\nvalues = (inner, 0)\nalias, other = values\nalias[0] = 0\nsend(inner[0])', ['0'], False),
    ('values = [read(), 0]\nfirst, second = values\nvalues[0] = 0\nsend(first)', ['Customer'], True),
    ('values = [read(), 0]\nvalues, second = values\nsend(values)', ['Customer'], True),
    ('inner = [read()]\nvalues = [0, inner, 0]\nfirst, *rest, last = values\nrest[0][0] = 0\nsend(inner[0])', ['0'], False),
    ('values = [read(), 0]\n*rest, last = values\nrest[0] = 0\nsend(values[0])', ['Customer'], True),
    ('values = (read(), (0, [0]))\nfirst, (second, alias) = values\nalias[0] = first\nsend(alias[0])', ['Customer'], True),
    ('values = [read(), 0]\nfirst, first = values\nsend(first)', ['0'], False),
    ('values = [read(), 0]\nalias = first, second = values\nalias[0] = 0\nsend(first)', ['Customer'], True),
    ('values = [read(), 0]\nfirst, second = alias = values\nalias[0] = 0\nsend(first)', ['Customer'], True),
    ('values = (read(), 0)\nalias = (*values, 0)\nfirst, *rest = alias\nsend(first)', ['Customer'], True),
    ('values = [read(), 0]\nalias = [0, *values, *()]\nsend(alias[1])', ['Customer'], True),
    ('values = [read(), 0]\nfirst, second, third = [*values, 0]\nsend(first)', ['Customer'], True),
    ('values = [read(), 0]\nfirst, *rest, last = values\nsend(rest)', ['[]'], False),
    ('values = [read()]\n*rest, = values\nsend(rest[0])', ['Customer'], True),
    ('values = [read()]\nfirst, (second,) = (0, values)\nsend(second)', ['Customer'], True),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_identity_and_nested_starred_binding(tmp_path, body, expected, witness, renamed):
    source = program(body, renamed=renamed)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('rhs, kind', [('[]', 'ValueError'), ('[0, 0, 0]', 'ValueError'),
    ('(0,)', 'ValueError'), ('0', 'TypeError'), ('None', 'TypeError'), ('False', 'TypeError')])
def test_unpack_failure_preserves_previous_names_and_reaches_handler(tmp_path, rhs, kind):
    source = program(f'first = read()\ntry:\n    first, second = {rhs}\n    send("unreached")\nexcept {kind}:\n    send(first)')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses'] and not report['completion']['may_raise']
    assert any(event['kind'] == 'sequence-error' and event['exception'] == kind for event in report['events'])


@pytest.mark.parametrize('body, expected, witness', [
    ('first = read()\ntry:\n    first, (second, third) = [0, []]\nexcept ValueError:\n    send(first)', ['0'], False),
    ('first = 0\ntry:\n    first, (second,) = [read(), 0]\nexcept TypeError:\n    send(first)', ['Customer'], True),
    ('values = [read()]\ntry:\n    first, second = [values.pop()]\nexcept ValueError:\n    send(values)', ['[]'], False),
    ('values = [read()]\ntry:\n    alias = first, second = values\nexcept ValueError:\n    alias[0] = 0\nsend(values[0])', ['0'], False),
    ('values = [read()]\ntry:\n    first, second = values\nfinally:\n    values[0] = 0', [], False),
])
def test_nested_and_chained_failures_keep_completed_work(tmp_path, body, expected, witness):
    if 'finally:' in body:
        source = program('values = [read()]\ntry:\n    relay(values)\nexcept ValueError:\n    send(values[0])',
            'def relay(items):\n    try:\n        first, second = items\n    finally:\n        items[0] = 0\n')
        expected = ['0']
    else:
        source = program(body)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('call', ['relay(*values)', 'relay(*(read(), 0))', 'relay(0, *[read()])',
    'relay(*[read()], second=0)', 'relay(second=0, *[read()])', 'relay(*[], *values, *())'])
def test_expanded_arguments_bind_fixed_slots(tmp_path, call):
    source = program(f'values = [read(), 0]\nsend({call})', 'def relay(first, second):\n    return first\n')
    expected = execute(source, 'Customer')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in str(expected))


@pytest.mark.parametrize('call', ['relay(*[])', 'relay(*[0, 0, 0])', 'relay(*[0], first=0)', 'relay(*[0], wrong=0)'])
def test_expanded_binding_errors_skip_helper_body(tmp_path, call):
    source = program(f'try:\n    {call}\nexcept TypeError:\n    send("handled")',
        'def relay(first, second=0):\n    send(read())\n')
    assert execute(source, 'Customer') == ['handled']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses'] and not report['completion']['may_raise']


@pytest.mark.parametrize('call', ['relay(*[values])', 'relay(*(values,))', 'relay(*[], *[values])'])
def test_expanded_lists_keep_helper_mutations_and_error_paths(tmp_path, call):
    source = program(f'values = [read()]\ntry:\n    {call}\nexcept IndexError:\n    send(values[0])',
        'def relay(items):\n    alias, = (items,)\n    alias[0] = 0\n    return alias[9]\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


@pytest.mark.parametrize('call', ['relay(*values, marker=change(values))',
    'relay(marker=change(values), *values)', 'relay(*values, *(), marker=change(values))',
    'relay(0, *values, marker=change(values))'])
def test_expansion_observes_python_snapshot_timing(tmp_path, call):
    source = program(f'values = [read()]\nsend({call})',
        'def change(items):\n    items[0] = 0\n    return 0\n'
        'def relay(first=0, second=0, *, marker=0):\n    return first\n')
    expected = execute(source, 'Customer')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in str(expected))


@pytest.mark.parametrize('body', ['values = [read()]\nvalues, = "text"',
    'values = [read()]\nfirst, second = unknown()',
    'values = (read(),)\nvalues[0] = 0\nsend(values)',
    'values = (read(),)\nvalues.append(0)\nsend(values)'])
def test_unsupported_iterables_and_tuple_writes_remain_incomplete(tmp_path, body):
    report = analyze(tmp_path, program(body))
    assert not report['complete'] and report['cutoffs']


def test_sdk_rejects_invented_unpacking_contract(tmp_path):
    report = analyze(tmp_path, program('values = [read(), 0]\nfirst, second = values\nsend(first)'))
    assert FunctionSummaries.from_report(FrReport(report, ())).complete
    report = copy.deepcopy(report)
    report['assignment_control']['binding'] = 'arbitrary iterators'
    report['inputs']['assignment_control'] = copy.deepcopy(report['assignment_control'])
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(report, ()))


@pytest.mark.parametrize('call,expected', [
    ('relay(*0, marker=change(values))', ['0']),
    ('relay(*0, *(), marker=change(values))', ['Customer']),
])
def test_noniterable_expansion_keeps_python_error_order(tmp_path, call, expected):
    source = program(f'values = [read()]\ntry:\n    {call}\nexcept TypeError:\n    send(values[0])',
        'def change(items):\n    items[0] = 0\n    return 0\n'
        'def relay(first=0, *, marker=0):\n    return first\n')
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in expected)


@pytest.mark.parametrize('count,complete', [(64, True), (65, False)])
def test_expanded_item_and_argument_limits_are_unchanged(tmp_path, count, complete):
    values = ', '.join('0' for _ in range(count))
    parameters = ', '.join(f'arg{index}' for index in range(count))
    source = program(f'values = [{values}]\nsend(relay(*values))',
                     f'def relay({parameters}):\n    return arg0\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'] is complete, report['cutoffs']


@pytest.mark.parametrize('module', ['helper', 'renamed_helper'])
def test_imported_unpacking_edits_invalidate_cached_answers(tmp_path, module):
    import json
    import subprocess
    import sys
    from fr_ir.runtime import FrClient
    from fr_ir.context import MemoryObjectStore
    from fr_ir.flow import FlowCache
    from test_graph_analysis import ROOT

    helper = tmp_path / (module + '.py')
    helper.write_text('def relay(items):\n    first, second = items\n    return first\n')
    source = program('values = [read(), 0]\nsend(relay(values))', f'from {module} import relay\n')
    analyze(tmp_path, source)
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=1048576)
    cache = FlowCache(MemoryObjectStore())

    def query(selected):
        handle = client.project('find', 'entry').definition_target().handle
        return selected.analyze(client, handle, standard_library=('builtins',),
                                rules=tmp_path / 'rules.json', context='html', steps=4096,
                                depth=8, max_bytes=1048576, summaries=True, imports=True)

    def runtime():
        driver = ('import sys,json\n' + f'sys.path.insert(0, {str(tmp_path)!r})\n'
                  'sent=[]\ndef read(): return "Customer"\ndef send(value): sent.append(value)\n'
                  + source + '\nentry()\nprint(json.dumps(sent))\n')
        return json.loads(subprocess.check_output([sys.executable, '-I', '-B', '-c', driver], text=True, timeout=10))

    assert runtime() == ['Customer']
    cold = query(cache)
    assert cold.report.at('/complete') and cold.witnesses
    assert query(cache).reused
    helper.write_text('def relay(items):\n    first, *rest = items\n    return rest[0]\n')
    assert runtime() == [0]
    changed = query(cache)
    assert not changed.reused and changed.report.at('/complete') and not changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    for pointer in ('/input_digest', '/completion', '/cutoffs', '/events'):
        assert changed.report.at(pointer) == fresh.report.at(pointer)


def test_lists_inside_scalar_call_assignments_keep_returned_identity(tmp_path):
    source = program('values = make(read())\nfirst, = values\nvalues[0] = 0\nsend(first)',
        'def relay(items):\n    return items\ndef make(value):\n    result = relay([value])\n    return result\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']
