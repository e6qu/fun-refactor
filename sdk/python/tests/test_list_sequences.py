"""Compare bounded list sequences with independent Python execution."""
import pytest

from test_graph_analysis import analyze, execute


def program(body, prelude='', renamed=False):
    source = prelude + 'def entry():\n' + ''.join('    ' + line + '\n' for line in body.splitlines())
    if renamed:
        source = source.replace('values', 'customers').replace('alias', 'shared').replace('relay', 'transfer')
    return source


@pytest.mark.parametrize('selection', [':', '1:', ':2', '1:3', '3:1', '-3:-1', '-99:99',
    '99:', ':-99', '::2', '1::2', '::-1', '3:0:-2', ':-1:-1', '-99::-1',
    ':None:None', 'None:None:-1', '::9223372036854775807', '::-9223372036854775808'])
@pytest.mark.parametrize('length', [0, 1, 4])
def test_slice_selection_matches_python(tmp_path, selection, length):
    items = ', '.join('read()' if index == 0 else '0' for index in range(length))
    source = program(f'values = [{items}]\nsend(values[{selection}])')
    expected = execute(source, 'Customer')
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is ('Customer' in str(expected))


@pytest.mark.parametrize('copy', ['values[:]', 'values.copy()', 'values + []', '[] + values'])
@pytest.mark.parametrize('nested', [False, True])
@pytest.mark.parametrize('renamed', [False, True])
def test_shallow_copies_keep_only_inner_aliases(tmp_path, copy, nested, renamed):
    body = ('inner = [read()]\nvalues = [inner]\n' if nested else 'values = [read()]\n')
    body += f'alias = {copy}\n'
    body += 'alias[0][0] = 0\nsend(values[0][0])' if nested else 'alias[0] = 0\nsend(values[0])'
    source = program(body, renamed=renamed)
    assert execute(source, 'Customer') == (['0'] if nested else ['Customer'])
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is (not nested)


@pytest.mark.parametrize('body, expected, witness', [
    ('values = [read(), 0, 0]\nalias = values\nvalues[:] = [0]\nsend(alias)', ['[0]'], False),
    ('values = [read(), 0, 0]\nvalues[1:2] = values\nsend(values[1])', ['Customer'], True),
    ('values = [read(), 0, 0]\nvalues[:] = values\nsend(values[0])', ['Customer'], True),
    ('values = [read(), 0, 0]\nvalues[2:1] = [read()]\nsend(values[2])', ['Customer'], True),
    ('values = [read(), 0, 0]\nvalues[-99:99] = []\nsend(values)', ['[]'], False),
    ('values = [read(), 0, read()]\nvalues[::2] = [0, 0]\nsend(values)', ['[0, 0, 0]'], False),
    ('values = [0, 0, 0]\nvalues[2:0:-1] = [read(), 0]\nsend(values[2])', ['Customer'], True),
    ('values = [read(), 0]\nvalues[::-1] = values\nsend(values[1])', ['Customer'], True),
    ('values = [read()]\ndel values[:]\nsend(values)', ['[]'], False),
    ('values = [read(), 0, 0]\nalias = values\ndel values[0]\nsend(alias)', ['[0, 0]'], False),
    ('values = [0, read()]\ndel values[-1]\nsend(values)', ['[0]'], False),
    ('values = [read(), 0, read()]\ndel values[::2]\nsend(values)', ['[0]'], False),
    ('values = [read(), 0, 0]\ndel values[2:0:-1]\nsend(values[0])', ['Customer'], True),
    ('values = [read(), 0]\ndel values[99:]\nsend(values[0])', ['Customer'], True),
    ('values = [0, read(), 0]\ndel values[0], values[1]\nsend(values[0])', ['Customer'], True),
    ('inner = [read()]\nvalues = [inner]\nalias = values[:]\ndel values[:]\nsend(alias[0][0])', ['Customer'], True),
    ('inner = [read()]\nvalues = [inner]\nvalues[:] = [[0]]\nsend(inner[0])', ['Customer'], True),
    ('inner = [0]\nvalues = []\nvalues[:] = [inner]\ninner[0] = read()\nsend(values[0][0])', ['Customer'], True),
    ('values = [read()]\nvalues.append(values)\nalias = values.copy()\nvalues[0] = 0\nsend(alias[1][0])', ['0'], False),
    ('values = [read()]\nvalues = values[1:]\nsend(values)', ['[]'], False),
])
@pytest.mark.parametrize('renamed', [False, True])
def test_slice_mutations_and_deletions_match_python(tmp_path, body, expected, witness, renamed):
    source = program(body, renamed=renamed)
    assert execute(source, 'Customer') == expected
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert bool(report['witnesses']) is witness


@pytest.mark.parametrize('expression', ['values + relay(values)', 'relay(values)[:]'])
def test_sequence_reads_happen_after_operand_side_effects(tmp_path, expression):
    source = program(f'values = [read()]\nalias = {expression}\nsend(alias[0])',
                     'def relay(items):\n    items[0] = 0\n    return items\n')
    assert execute(source, 'Customer') == ['0']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert not report['witnesses']


def test_slice_assignment_evaluates_replacement_before_target(tmp_path):
    source = program('values = [read()]\nrelay(values)[:] = [values.pop()]\nsend(values[0])',
                     'def relay(items):\n    items.append(0)\n    return items\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('copy', ['items[:]', 'items.copy()', 'items + []'])
def test_returned_copy_keeps_scalar_contents_before_cleanup(tmp_path, copy):
    source = program('values = [read()]\nsend(relay(values)[0])',
        f'def relay(items):\n    try:\n        return {copy}\n    finally:\n        items[0] = 0\n')
    assert execute(source, 'Customer') == ['Customer']
    report = analyze(tmp_path, source)
    assert report['complete'], report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('body, cutoff', [
    ('values = [read()]\nsend(values[::0])', 'list-slice-step-zero'),
    ('values = [read()]\nvalues[::0] = []', 'list-slice-step-zero'),
    ('values = [read()]\ndel values[::0]', 'list-slice-step-zero'),
    ('values = [read()]\nsend(values[read():])', 'list-slice-bounds-unchecked'),
    ('values = [read()]\nvalues[::2] = []', 'list-slice-length-mismatch'),
    ('values = [read()]\nvalues[:] = read()', 'list-sequence-target-unchecked'),
    ('values = [read()]\nif read():\n    values.append(0)\nsend(values[:])', 'list-sequence-length-unchecked'),
    ('values = [read()]\nalias = values if read() else [0]\nsend(alias.copy())', 'list-sequence-target-unchecked'),
    ('values = [read()]\nsend(values + 0)', 'list-sequence-target-unchecked'),
    ('values = [read()]\nsend(values * 2)', 'list-operator-unchecked'),
    ('values = [read()]\nvalues.copy(0)', 'list-method-arguments'),
    ('values = [read()]\ndel values[99]', 'list-index-may-raise'),
    ('values = [read()]\nwhile read():\n    alias = values[:]', 'repeated-list-allocation'),
])
def test_unchecked_sequences_keep_their_boundaries(tmp_path, body, cutoff):
    report = analyze(tmp_path, program(body))
    assert not report['complete']
    assert cutoff in report['cutoffs']


@pytest.mark.parametrize('operation', ['values + values', 'values[:0] = values'])
def test_sequence_growth_obeys_the_item_budget(tmp_path, operation):
    body = 'values = [' + ', '.join(['0'] * 40) + ']\n' + operation
    report = analyze(tmp_path, program(body))
    assert not report['complete']
    assert 'list-item-budget' in report['cutoffs']


def test_slice_bounds_are_evaluated_once_even_when_unchecked(tmp_path):
    source = program('values = [0]\ncopy = values[relay():]\nsend(values[0])',
                     'def relay():\n    send(read())\n    return 0\n')
    assert execute(source, 'Customer') == ['Customer', '0']
    report = analyze(tmp_path, source)
    assert not report['complete']
    assert 'list-slice-bounds-unchecked' in report['cutoffs']
    assert report['witnesses']


@pytest.mark.parametrize('module', ['helpers', 'moved_helpers'])
@pytest.mark.parametrize('copy', ['items[:]', 'items.copy()', 'items + []'])
def test_imported_copy_answers_invalidate_after_source_changes(tmp_path, module, copy):
    from fr_ir.runtime import FrClient
    from fr_ir.context import MemoryObjectStore
    from fr_ir.flow import FlowCache
    from test_graph_analysis import ROOT

    helper = tmp_path / (module + '.py')
    helper.write_text(f'def relay(items):\n    result = {copy}\n    result[0] = 0\n    return result\n')
    source = program('values = [read()]\nsend(relay(values)[0])', f'from {module} import relay\n')
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
    helper.write_text(f'def relay(items):\n    return {copy}\n')
    changed = query(cache)
    assert not changed.reused
    assert changed.report.at('/complete') and changed.witnesses
    fresh = query(FlowCache(MemoryObjectStore()))
    assert changed.witnesses == fresh.witnesses
    assert any(point['path'] == module + '.py'
               for witness in changed.report.at('/witnesses')
               for point in witness['trace']['occurrences'])
