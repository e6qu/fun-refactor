"""Give checked values another name without changing their kind or declaration."""
import ast
import copy
import hashlib
import json
from pathlib import Path
import tempfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


def install(tmp_path, value='0', route='local', parallel=False, explicit=False):
    client, rules = workspace(tmp_path)
    assignment = 'saved, other = base, None\n' if parallel else 'saved = base\n'
    helper = f'base = {value}\n' + assignment + 'def choose(value=saved):\n    return value\n'
    prefix = helper
    if route != 'local':
        if route == 'shared':
            (tmp_path / 'settings.py').write_text(f'base = {value}\n' + assignment)
            helper = 'from settings import saved as imported\nsaved = imported\n' + 'def choose(value=saved):\n    return value\n'
        if route == 'star':
            helper = "__all__ = ['choose', 'saved']\n" + helper
        (tmp_path / 'helpers.py').write_text(helper)
        prefix = 'from helpers import *\n' if route == 'star' else 'from helpers import choose, saved\n'
    call = 'choose(source())' if explicit else 'choose()'
    (tmp_path / 'subject.py').write_text(prefix + f'def entry():\n    return sink({call})\n')
    return client, rules


@pytest.mark.parametrize('value', ['None', 'False', '17', '-17', "'café'", '((0))'])
@pytest.mark.parametrize('route', ['local', 'direct', 'shared', 'star'])
@pytest.mark.parametrize('parallel', [False, True])
@pytest.mark.parametrize('explicit', [False, True])
def test_scalar_aliases_capture_the_same_default_and_preserve_overrides(tmp_path, value, route, parallel, explicit):
    client, rules = install(tmp_path, value, route, parallel, explicit)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    expected = 17 if explicit else ast.literal_eval(value)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is explicit and not result.initialization.sites
    assert observe(tmp_path) == {'result': expected, 'raised': None,
                                'events': [['source', 17], ['sink', 17]] if explicit else [['sink', expected]]}
    function = next(item for item in result.summaries.functions if item.function.endswith('choose'))
    default = function.signature[0].default
    assert default is not None
    assert (tmp_path / default.path).read_bytes()[default.location.span.start:default.location.span.end] == b'saved'
    if route != 'local':
        dependencies = result.dependencies
        assert dependencies.value_aliases and not dependencies.function_aliases
        for alias in dependencies.value_aliases:
            assert (tmp_path / alias.path).read_bytes()[alias.offset:].startswith(alias.name.encode())
        lookup = next(item for item in dependencies.lookups if item.importer == 'subject.py' and item.alias == 'saved')
        assert lookup.resolution == 'value'
        assert lookup.binding_chain[-1].name == 'base'
        assert lookup.binding_chain[-1].path == ('settings.py' if route == 'shared' else 'helpers.py')
        assert len(dependencies.value_bindings) == 1 and dependencies.value_bindings[0].name == 'base'


@pytest.mark.parametrize('imports', [False, True])
@pytest.mark.parametrize('setup', [
    'base = 0\n', 'base = []\n', 'base = {}\n', 'base = (0,)\n',
    'class base:\n    pass\n', 'base = lambda: opaque()\n',
    'class Token:\n    pass\nbase = Token()\n', 'base = dict(send=lambda value: value)\n',
])
def test_unused_value_aliases_keep_defaults_inert_without_calling_objects(tmp_path, imports, setup):
    client, rules = workspace(tmp_path)
    helper = setup + 'saved = base\ndef unused(value=saved):\n    return opaque()\n'
    if imports:
        (tmp_path / 'helpers.py').write_text(helper)
        helper = 'from helpers import saved, unused\n'
    (tmp_path / 'subject.py').write_text(helper + 'def entry():\n    return sink(source())\n')
    result = analyze(client, rules, 'entry', imports=imports)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.witnesses and not result.initialization.sites
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('route', ['local', 'direct', 'shared'])
@pytest.mark.parametrize('value', ['[]', '{}', '(0,)', 'lambda: 0'])
@pytest.mark.parametrize('explicit', [False, True])
def test_aliasing_a_collection_or_callable_does_not_make_its_default_scalar(tmp_path, route, value, explicit):
    client, rules = install(tmp_path, value, route, explicit=explicit)
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete')
    assert any('unsupported-parameter-contract' in cutoff for cutoff in result.report.at('/cutoffs'))
    if explicit:
        assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('imports', [False, True])
@pytest.mark.parametrize('setup,raised', [
    ('saved = base\nbase = 0\n', 'NameError'),
    ('base = 0\nsaved = missing\n', 'NameError'),
    ('base = 0\nsaved = base\nbase = 1\n', None),
    ('base = 0\nsaved = base\nsaved = 1\n', None),
    ('base = 0\nsaved = base\ndel base\n', None),
    ('base = 0\nsaved = (base)\n', None),
    ('base = 0\nsaved = other = base\n', None),
    ('base = 0\nsaved: int = base\n', None),
    ('base = 0\nsaved, other = base, saved\n', 'NameError'),
    ('base = 0\nsaved, other = base, missing\n', 'NameError'),
])
def test_missing_late_rebound_and_unsupported_aliases_stay_incomplete(tmp_path, imports, setup, raised):
    client, rules = workspace(tmp_path)
    if imports:
        (tmp_path / 'helpers.py').write_text(setup)
        setup = 'from helpers import saved\n'
    (tmp_path / 'subject.py').write_text(setup + 'def entry():\n    return sink(source())\n')
    result = analyze(client, rules, 'entry', imports=imports)
    assert not result.report.at('/complete')
    assert observe(tmp_path)['raised'] == raised


@pytest.mark.parametrize('imports', [False, True])
@pytest.mark.parametrize('count,complete', [(14, True), (15, True), (16, False)])
def test_value_alias_chains_keep_the_existing_sixteen_binding_limit(tmp_path, imports, count, complete):
    client, rules = workspace(tmp_path)
    setup = 'base = 0\n' + ''.join(f'a{index} = ' + ('base' if index == 0 else f'a{index - 1}') + '\n'
                                           for index in range(count))
    if imports:
        (tmp_path / 'helpers.py').write_text(setup + f'def choose(value=a{count - 1}):\n    return value\n')
        setup = 'from helpers import choose\n'
    else:
        setup += f'def choose(value=a{count - 1}):\n    return value\n'
    (tmp_path / 'subject.py').write_text(setup + 'def entry():\n    return sink(choose())\n')
    result = analyze(client, rules, 'entry', imports=imports)
    assert result.report.at('/complete') is complete
    assert observe(tmp_path)['result'] == 0


@pytest.mark.parametrize('imports', [False, True])
@pytest.mark.parametrize('count,complete', [(127, True), (128, True), (129, False)])
def test_value_and_function_aliases_share_the_same_128_slots(tmp_path, imports, count, complete):
    client, rules = workspace(tmp_path)
    setup = 'base = 0\ndef relay(value):\n    return value\npublic = relay\n'
    setup += ''.join(f'copy{index} = base\n' for index in range(count - 1))
    if imports:
        (tmp_path / 'helpers.py').write_text(setup)
        setup = 'from helpers import public\n'
    (tmp_path / 'subject.py').write_text(setup + 'def entry():\n    return sink(public(source()))\n')
    result = analyze(client, rules, 'entry', imports=imports)
    assert result.report.at('/complete') is complete
    assert observe(tmp_path)['result'] == 17


@pytest.mark.parametrize('replacement', ['base = 1', 'base = []'])
def test_changing_the_original_value_invalidates_saved_alias_answers(tmp_path, replacement):
    client, rules = install(tmp_path, route='shared')
    cache = FlowCache(MemoryObjectStore())
    assert analyze(client, rules, 'entry', cache, imports=True).report.at('/complete')
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    path = tmp_path / 'settings.py'
    path.write_text(path.read_text().replace('base = 0', replacement))
    changed = analyze(client, rules, 'entry', cache, imports=True)
    assert not changed.reused
    assert changed.report.at('/complete') is (replacement == 'base = 1')
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True))


@pytest.mark.parametrize('edit', ['missing', 'kind', 'order', 'duplicate', 'offset', 'target', 'chain'])
def test_value_alias_evidence_is_required_and_consistent_even_after_rehashing(tmp_path, edit):
    client, rules = install(tmp_path, route='shared')
    data = copy.deepcopy(analyze(client, rules, 'entry', imports=True).report.to_data())
    modules = data['inputs']['modules']
    if edit == 'missing':
        del modules['value_aliases']
    else:
        aliases = modules['value_aliases']
        if edit == 'kind':
            modules['function_aliases'].append(aliases.pop())
        elif edit == 'order':
            aliases.reverse()
        elif edit == 'duplicate':
            aliases.append(copy.deepcopy(aliases[0]))
        elif edit == 'offset':
            aliases[0]['offset'] = True
        elif edit == 'target':
            aliases[0]['target'] = 'missing'
        else:
            lookup = next(item for item in modules['lookups'] if item['names'] is not None)
            lookup['names'][0]['binding_chain'] = []
    data['input_digest'] = hashlib.sha256(json.dumps(data['inputs'], sort_keys=True, ensure_ascii=False,
                                                    separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(data, ()))
    with tempfile.TemporaryDirectory(prefix='fr-value-alias-retained-') as directory:
        retained = Path(directory) / 'report.json'
        retained.write_text(json.dumps(data))
        handle = client.project('find', 'entry').definition_target().handle
        with pytest.raises(FrRuntimeError):
            client.project('dataflow', handle, '--summaries', '--imports', '--rules', str(rules),
                           '--reuse', str(retained), '--steps', '4096', '--bytes', '1048576')


@pytest.mark.parametrize('imports', [False, True])
@pytest.mark.parametrize('body,raised', [
    ('return sink(saved)', None),
    ('return sink(saved(source()))', 'TypeError'),
    ('return sink(saved.attribute)', 'AttributeError'),
])
def test_aliases_do_not_add_global_reads_calls_or_attributes(tmp_path, imports, body, raised):
    client, rules = workspace(tmp_path)
    setup = 'base = 0\nsaved = base\n'
    if imports:
        (tmp_path / 'helpers.py').write_text(setup)
        setup = 'from helpers import saved\n'
    (tmp_path / 'subject.py').write_text(setup + f'def entry():\n    {body}\n')
    assert not analyze(client, rules, 'entry', imports=imports).report.at('/complete')
    assert observe(tmp_path)['raised'] == raised


def test_aliasing_an_unknown_native_import_does_not_guess_its_behavior(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / 'subject.py').write_text('from itertools import chain\nsaved = chain\n'
                                       'def entry():\n    return sink(source())\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete')
    assert any('unresolved' in cutoff for cutoff in result.report.at('/cutoffs'))
    assert not result.dependencies.value_aliases
    assert result.initialization.sites
    assert observe(tmp_path)['result'] == 17
