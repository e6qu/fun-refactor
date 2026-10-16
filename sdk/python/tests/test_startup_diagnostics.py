"""Explain real startup blockers without changing the analysis answer."""
import copy

import pytest

from fr_ir.flow_initialization import FlowInitialization
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, workspace
from test_postponed_annotations import observe


CASES = [
    ('opaque()', 'startup-call'),
    ('metadata = opaque()', 'startup-call'),
    ('metadata = (opaque())', 'startup-call'),
    ('metadata = unknown', 'startup-assignment'),
    ('metadata: int = 0', 'startup-assignment'),
    ('table[0] = 0', 'startup-write'),
    ('table.value = 0', 'startup-write'),
    ('table[0] += 1', 'startup-write'),
    ('del table[0]', 'startup-write'),
    ('class Token(list):\n    pass', 'class-definition'),
    ('@decorate\ndef unused():\n    pass', 'decorated-definition'),
    ('if False:\n    opaque()', 'startup-control-flow'),
    ('for item in ():\n    opaque()', 'startup-control-flow'),
    ('while False:\n    opaque()', 'startup-control-flow'),
    ('try:\n    pass\nexcept Exception:\n    opaque()', 'startup-control-flow'),
    ('with resource():\n    pass', 'startup-control-flow'),
    ('match 0:\n    case 0:\n        opaque()', 'startup-control-flow'),
    ('metadata = [' + ','.join('0' for _ in range(128)) + ']', 'initializer-node-budget'),
    ('table.update(items=[' + ','.join('0' for _ in range(128)) + '])', 'initializer-node-budget'),
    ('unknown', 'module-statement'),
]


def install(tmp_path, setup, imports=False):
    client, rules = workspace(tmp_path)
    code = setup + '\ndef relay(value):\n    return value\n'
    if imports:
        (tmp_path / 'helpers.py').write_text(code)
        code = 'from helpers import relay\n'
    (tmp_path / 'subject.py').write_text(code + 'def entry():\n    return sink(relay(0))\n')
    return client, rules


@pytest.mark.parametrize('setup,reason', CASES)
@pytest.mark.parametrize('imports', [False, True])
def test_reason_describes_the_top_level_operation(tmp_path, setup, reason, imports):
    client, rules = install(tmp_path, setup, imports)
    result = analyze(client, rules, 'entry', imports=imports)
    site, = result.initialization.sites
    summary, = result.initialization.by_file
    assert not result.report.at('/complete') and not result.witnesses
    assert site.reasons == (reason,)
    assert summary.path == ('helpers.py' if imports else 'subject.py')
    assert summary.statements == 1 and dict(summary.reasons) == {reason: 1}
    assert summary.explanations == site.explanations
    assert site.occurrence.text((tmp_path / summary.path).read_text(),
                                revision=result.report.at('/revision')) == setup


def test_non_import_mode_explains_imports_without_claiming_they_failed(tmp_path):
    client, rules = install(tmp_path, 'import math')
    result = analyze(client, rules, 'entry')
    assert result.initialization.sites[0].reasons == ('startup-import',)
    assert observe(tmp_path)['result'] == 0


@pytest.mark.parametrize('max_bytes', [8192, 1_048_576])
def test_totals_cover_files_beyond_the_sixteen_shown_locations(tmp_path, max_bytes):
    client, rules = install(tmp_path, '\n'.join('opaque()' for _ in range(20)), True)
    (tmp_path / 'subject.py').write_text('from helpers import relay\n' +
        '\n'.join('metadata' for _ in range(7)) + '\ndef entry():\n    return sink(relay(0))\n')
    result = analyze(client, rules, 'entry', imports=True, max_bytes=max_bytes)
    disclosure = result.initialization
    assert len(disclosure.sites) + disclosure.omitted == 27
    assert [(item.path, item.statements, dict(item.reasons)) for item in disclosure.by_file] == [
        ('helpers.py', 20, {'startup-call': 20}), ('subject.py', 7, {'module-statement': 7})]
    assert not result.report.at('/complete')


def test_multiple_reasons_count_a_statement_once_and_deferred_bodies_stay_deferred(tmp_path):
    client, rules = install(tmp_path, 'marker = 0\ndef marker(value=source()):\n    opaque()')
    result = analyze(client, rules, 'entry')
    summary, = result.initialization.by_file
    assert summary.statements == 1
    assert dict(summary.reasons) == {'binding-collision': 1, 'definition-time-code': 1}
    client, rules = install(tmp_path, 'def unused():\n    opaque()')
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') and not result.initialization.by_file
    assert observe(tmp_path)['result'] == 0


MUTATIONS = ['missing', 'extra', 'list', 'unknown-file', 'unknown-reason', 'boolean',
             'zero', 'negative', 'total', 'reason-total', 'hidden-reason', 'hidden-file',
             'empty-reasons', 'extra-count-field', 'boolean-reason', 'too-many-files']


@pytest.mark.parametrize('mutation', MUTATIONS)
def test_per_file_counts_are_required_and_checked_against_locations(tmp_path, mutation):
    client, rules = install(tmp_path, 'metadata = source()\nother = source()')
    data = copy.deepcopy(analyze(client, rules, 'entry').report.to_data())
    disclosure = data['initialization']
    counts = disclosure['by_file']
    item = counts['subject.py']
    if mutation == 'missing':
        del disclosure['by_file']
    elif mutation == 'extra':
        disclosure['version'] = 1
    elif mutation == 'list':
        disclosure['by_file'] = []
    elif mutation == 'unknown-file':
        counts['unloaded.py'] = counts.pop('subject.py')
    elif mutation == 'unknown-reason':
        item['reasons'] = {'trusted': 2}
    elif mutation == 'boolean':
        item['statements'] = True
    elif mutation == 'zero':
        item['statements'] = 0
    elif mutation == 'negative':
        item['statements'] = -1
    elif mutation == 'total':
        item['statements'] = 3
        item['reasons']['startup-call'] = 3
    elif mutation == 'reason-total':
        item['reasons']['startup-call'] = 1
    elif mutation == 'hidden-reason':
        item['reasons'] = {'startup-write': 2}
    elif mutation == 'hidden-file':
        counts.clear()
    elif mutation == 'empty-reasons':
        item['reasons'] = {}
    elif mutation == 'extra-count-field':
        item['version'] = 1
    elif mutation == 'boolean-reason':
        item['reasons']['startup-call'] = True
    else:
        disclosure['by_file'] = {f'file{index}.py': item for index in range(17)}
    for reader in (FlowInitialization.from_report, FunctionSummaries.from_report):
        with pytest.raises(FrRuntimeError):
            reader(FrReport(data, ()))


def test_totals_must_account_for_unshown_reasons_too(tmp_path):
    client, rules = install(tmp_path, '\n'.join('opaque()' for _ in range(20)))
    data = analyze(client, rules, 'entry').report.to_data()
    data['initialization']['by_file']['subject.py']['reasons'] = {'startup-call': 16}
    with pytest.raises(FrRuntimeError):
        FlowInitialization.from_report(FrReport(data, ()))
