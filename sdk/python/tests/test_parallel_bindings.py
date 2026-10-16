"""Read flat module assignment values before binding their destination names."""
import copy
import hashlib
import json

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


FUNCTIONS = 'def relay(value):\n    return value\ndef stop(value):\n    return 0\n'
PRIOR_INITIALIZATION = (
    'Inert: literals/aliases, builtin refs/maps, exports, plain instances, prior function __doc__. '
    'Bases: none/unshadowed object/dict/Exception. Lambda/dict setup/item/update <=128 nodes; own writes only.'
)


def install(tmp_path, statement='public, discarded = relay, stop', route='local', call='public(source())',
            before='', after=''):
    client, rules = workspace(tmp_path)
    helper = '# café 🦀\n' + FUNCTIONS + before + statement + '\n' + after
    prelude = helper
    if route != 'local':
        (tmp_path / 'helpers.py').write_text(helper)
        if route == 'direct':
            prelude = 'from helpers import public\n'
        elif route == 'module':
            prelude = 'import helpers\n'
            call = call.replace('public', 'helpers.public')
        elif route == 'reexport':
            (tmp_path / 'bridge.py').write_text('from helpers import public\n')
            prelude = 'from bridge import public\n'
        elif route == 'star':
            (tmp_path / 'helpers.py').write_text("__all__ = ['public']\n" + helper)
            prelude = 'from helpers import *\n'
        else:
            prelude = 'from helpers import relay\n'
            call = 'relay(source())'
    (tmp_path / 'subject.py').write_text(prelude + 'def entry():\n    return sink(' + call + ')\n')
    return client, rules


@pytest.mark.parametrize('route', ['local', 'direct', 'module', 'reexport', 'star'])
@pytest.mark.parametrize('statement', [
    'public, discarded = relay, stop', '(public, discarded) = (relay, stop)',
    '[public, discarded] = [relay, stop]', 'public, discarded = [relay, stop]',
    '(public,) = (relay,)', 'public, discarded = stop, relay',
])
def test_parallel_function_aliases_preserve_the_selected_function(tmp_path, route, statement):
    client, rules = install(tmp_path, statement, route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    expected = 0 if '= stop,' in statement else 17
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is (expected == 17) and not result.initialization.sites
    assert observe(tmp_path) == {'result': expected, 'raised': None,
                                'events': [['source', 17], ['sink', expected]]}
    if route != 'local':
        aliases = result.dependencies.function_aliases
        assert len(aliases) == (1 if 'public,' in statement and 'discarded' not in statement else 2)
        source = (tmp_path / 'helpers.py').read_bytes()
        for alias in aliases:
            assert source[alias.offset:].startswith(alias.name.encode())
        assert len({item.offset for item in aliases}) == len(aliases)


@pytest.mark.parametrize('route', ['local', 'helper'])
@pytest.mark.parametrize('statement', [
    '_map, _filter = map, filter', 'saved, factory = relay, dict',
    'metadata, factory = [0, None], dict', 'factory, stored = dict, sorted',
])
def test_unused_function_and_builtin_references_allow_later_defaults(tmp_path, route, statement):
    names = statement.split('=')[0].replace(' ', '').split(',')
    defaults = ', '.join(f'argument{index}={name}' for index, name in enumerate(names))
    client, rules = install(tmp_path, statement, route, call='relay(source())',
                            after=f'def unused({defaults}):\n    return opaque()\n')
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert not result.initialization.sites
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17]]


def test_a_prior_import_can_supply_both_function_aliases(tmp_path):
    client, rules = install(tmp_path, route='direct')
    (tmp_path / 'helpers.py').write_text('from provider import relay, stop\npublic, discarded = relay, stop\n')
    (tmp_path / 'provider.py').write_text(FUNCTIONS)
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete') and result.witnesses
    result.summaries.for_function('provider.py::relay')
    assert observe(tmp_path)['result'] == 17


def test_builtin_values_are_read_before_new_names_shadow_them(tmp_path):
    client, rules = install(tmp_path, 'map, saved = relay, map', call='map(source())',
                            after='def unused(callback=saved):\n    return callback\n')
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') and result.witnesses
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('route', ['local', 'helper'])
@pytest.mark.parametrize('statement,raised', [
    ('public, other = relay, public', 'NameError'),
    ('public, other = relay, missing', 'NameError'),
    ('public, other = (relay,)', 'ValueError'),
    ('public, = (relay, stop)', 'ValueError'),
    ('public, public = relay, stop', None),
    ('public, (other,) = relay, (stop,)', None),
    ('public, *other = relay, stop', None),
    ('public, other = (item for item in [relay, stop])', None),
    ('public, other = relay(0), stop', None),
    ('public, other = relay, source()', None),
])
def test_unknown_evaluation_or_unpacking_cannot_establish_a_complete_answer(tmp_path, route, statement, raised):
    client, rules = install(tmp_path, statement, route, call='relay(source())')
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete') and result.initialization.sites
    assert observe(tmp_path)['raised'] == raised


@pytest.mark.parametrize('before,after', [
    ('public = 0\n', ''), ('', 'public = 0\n'), ('', 'del public\n'),
    ('', 'relay = 0\n'), ('', 'def public(value):\n    return 0\n'),
])
@pytest.mark.parametrize('route', ['local', 'direct'])
def test_collisions_and_later_writes_stay_incomplete(tmp_path, route, before, after):
    client, rules = install(tmp_path, route=route, call='source()', before=before, after=after)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete')


@pytest.mark.parametrize('route', ['local', 'helper'])
@pytest.mark.parametrize('count,complete', [(62, True), (63, False)])
def test_flat_assignments_keep_the_128_node_budget(tmp_path, route, count, complete):
    statement = ','.join(f'item{index}' for index in range(count)) + ' = ' + ','.join('0' for _ in range(count))
    client, rules = install(tmp_path, statement, route, call='relay(source())')
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete') is complete, result.report.at('/cutoffs')
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('route', ['local', 'direct'])
@pytest.mark.parametrize('count,complete', [(128, True), (130, False)])
def test_each_function_alias_uses_one_existing_budget_slot(tmp_path, route, count, complete):
    statements = []
    for index in range(0, count, 2):
        left = 'public' if index == count - 2 else f'alias{index}'
        statements.append(f'{left}, alias{index + 1} = relay, relay')
    client, rules = install(tmp_path, '\n'.join(statements), route)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete') is complete, result.report.at('/cutoffs')
    assert observe(tmp_path)['result'] == 17


def test_parallel_literal_exports_keep_distinct_source_positions(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / 'helpers.py').write_text('left, right = 0, 1\n' + FUNCTIONS)
    (tmp_path / 'subject.py').write_text('from helpers import left, right, relay\n'
                                       'def entry():\n    return sink(relay(source()))\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete') and result.witnesses
    values = result.dependencies.value_bindings
    assert [(item.name, item.offset, item.kind) for item in values] == [('left', 0, 'literal'), ('right', 6, 'literal')]
    assert observe(tmp_path)['result'] == 17


def test_changed_parallel_targets_and_values_invalidate_saved_answers(tmp_path):
    client, rules = install(tmp_path, route='direct')
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, 'entry', cache, imports=True).witnesses
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    path = tmp_path / 'helpers.py'
    path.write_text(path.read_text().replace('relay, stop', 'stop, relay'))
    changed = analyze(client, rules, 'entry', cache, imports=True)
    assert not changed.reused and changed.report.at('/complete') and not changed.witnesses
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True))
    assert observe(tmp_path)['result'] == 0


def test_alias_offsets_and_noncurrent_contracts_are_rejected(tmp_path):
    client, rules = install(tmp_path, route='direct')
    original = analyze(client, rules, 'entry', imports=True).report.to_data()
    for mutation in ('offset', 'target', 'contract'):
        wrong = copy.deepcopy(original)
        if mutation == 'contract':
            wrong['call_binding']['initialization'] = PRIOR_INITIALIZATION
            wrong['inputs']['call_binding']['initialization'] = PRIOR_INITIALIZATION
        else:
            aliases = wrong['inputs']['modules']['function_aliases']
            if mutation == 'offset':
                aliases[1]['offset'] = aliases[0]['offset']
            else:
                aliases[0]['target'] = aliases[1]['name']
        encoded = json.dumps(wrong['inputs'], sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()
        wrong['input_digest'] = hashlib.sha256(encoded).hexdigest()
        with pytest.raises(FrRuntimeError):
            FunctionSummaries.from_report(FrReport(wrong, ()))


@pytest.mark.parametrize('route', ['local', 'direct'])
def test_parallel_aliases_cannot_replace_caller_rule_names(tmp_path, route):
    client, rules = install(tmp_path, 'source, public = relay, stop', route, call='public(0)')
    with pytest.raises(FrRuntimeError, match='rule overlaps'):
        analyze(client, rules, 'entry', imports=route != 'local')
