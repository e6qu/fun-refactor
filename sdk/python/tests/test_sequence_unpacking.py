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
