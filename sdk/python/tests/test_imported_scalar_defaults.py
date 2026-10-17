"""Capture imported scalar defaults at definition, with independent Python outcomes."""
import ast
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


PRIOR_DEFINITIONS = (
    'Module functions defer bodies with literals, prior values/functions or unshadowed builtin defaults. '
    'Calls accept literals or captured local scalars. Annotations: checked module-local future, '
    '<=128 nodes, no await/yield/assignment/type unpacking.'
)


def install(tmp_path, value='0', route='direct', call='choose()', parameters='value=((marker))'):
    client, rules = workspace(tmp_path)
    (tmp_path / 'settings.py').write_text(f'marker = {value}\n')
    imports = 'from settings import marker\n'
    if route == 'alias':
        imports = 'from settings import marker as saved\n'
        parameters = parameters.replace('marker', 'saved')
    elif route == 'reexport':
        (tmp_path / 'bridge.py').write_text('from settings import marker\n')
        imports = 'from bridge import marker\n'
    elif route == 'star':
        (tmp_path / 'settings.py').write_text(f"__all__ = ['marker']\nmarker = {value}\n")
        imports = 'from settings import *\n'
    elif route in ('relative', 'namespace'):
        package = tmp_path / 'library'
        package.mkdir()
        if route == 'relative':
            (package / '__init__.py').write_text('')
        (package / 'settings.py').write_text(f'marker = {value}\n')
        (package / 'helpers.py').write_text('from .settings import marker\n'
                                         f'def choose({parameters}):\n    return value\n')
        imports = 'from library.helpers import choose\n'
    if route not in ('relative', 'namespace'):
        imports += f'def choose({parameters}):\n    return value\n'
    (tmp_path / 'subject.py').write_text(imports + f'def entry():\n    return sink({call})\n')
    return client, rules


@pytest.mark.parametrize('value', ['None', 'False', 'True', '17', '-17', '1.25', "'café'", '((0))'])
@pytest.mark.parametrize('route', ['direct', 'alias', 'reexport', 'star', 'relative', 'namespace'])
@pytest.mark.parametrize('explicit', [False, True])
def test_imported_scalars_and_overrides_match_python(tmp_path, value, route, explicit):
    client, rules = install(tmp_path, value, route, call='choose(source())' if explicit else 'choose()')
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is explicit
    expected = 17 if explicit else ast.literal_eval(value)
    assert observe(tmp_path) == {'result': expected, 'raised': None,
                                'events': [['source', 17], ['sink', 17]] if explicit else [['sink', expected]]}
    assert result.dependencies.value_bindings
    function = next(item for item in result.summaries.functions if item.function.endswith('::choose'))
    default = function.signature[0].default
    assert default is not None
    assert (tmp_path / default.path).read_bytes()[default.location.span.start:default.location.span.end] in (
        b'((marker))', b'((saved))')


@pytest.mark.parametrize('parameters,call,reaches', [
    ('value=marker, /', 'choose()', False), ('value=marker, /', 'choose(source())', True),
    ('*, value=marker', 'choose()', False), ('*, value=marker', 'choose(value=source())', True),
    ('marker, /, value=marker', 'choose(source())', False),
    ('value=marker, *unused, **options', 'choose()', False),
])
def test_imported_defaults_preserve_parameter_kinds_and_shadowing(tmp_path, parameters, call, reaches):
    client, rules = install(tmp_path, parameters=parameters, call=call)
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is reaches
    assert observe(tmp_path)['result'] == (17 if reaches else 0)


@pytest.mark.parametrize('value', ['[]', '{}', '(0,)', '[0]', 'lambda: 0'])
@pytest.mark.parametrize('explicit', [False, True])
def test_non_scalar_imports_require_explicit_replacements(tmp_path, value, explicit):
    client, rules = install(tmp_path, value, call='choose(source())' if explicit else 'choose()')
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete') is explicit
    assert bool(result.witnesses) is explicit
    if not explicit:
        assert 'unsupported-default-value:subject.py::choose' in result.report.at('/cutoffs')
    else:
        assert observe(tmp_path)['result'] == 17


def test_parallel_scalar_exports_keep_each_literal_kind(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'settings.py').write_text('marker, other = 0, []\n')
    assert analyze(client, rules, 'entry', imports=True).report.at('/complete')
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('import marker', 'import other as marker'))
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete')
    assert 'unsupported-default-value:subject.py::choose' in result.report.at('/cutoffs')


@pytest.mark.parametrize('edit', ['marker = 0\nmarker = 1\n', 'marker = opaque()\n',
                                  'marker: int = 0\n', 'marker = 0\ndel marker\n'])
def test_provider_rebinding_evaluation_and_missing_values_stay_incomplete(tmp_path, edit):
    client, rules = install(tmp_path)
    (tmp_path / 'settings.py').write_text(edit)
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')


def test_an_import_after_the_definition_cannot_supply_its_default(tmp_path):
    client, rules = install(tmp_path)
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('from settings import marker\n', '')
                       + 'from settings import marker\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete')
    assert observe(tmp_path)['raised'] == 'NameError'


@pytest.mark.parametrize('available', [False, True])
def test_a_cyclic_import_requires_the_scalar_to_exist_before_reentry(tmp_path, available):
    client, rules = workspace(tmp_path)
    declaration, revisit = 'marker = 0\n', 'from helpers import choose\n'
    (tmp_path / 'settings.py').write_text((declaration + revisit if available else revisit + declaration)
                                        + 'ready = 0\n')
    (tmp_path / 'helpers.py').write_text('from settings import marker\n'
                                       'def choose(value=marker):\n    return value\n')
    (tmp_path / 'subject.py').write_text('from settings import ready\nfrom helpers import choose\n'
                                       'def entry():\n    return sink(choose())\n')
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete') is available, result.report.at('/cutoffs')
    assert observe(tmp_path)['raised'] == (None if available else 'ImportError')


def test_global_reads_and_module_attribute_defaults_remain_unchecked(tmp_path):
    client, rules = install(tmp_path)
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('return value', 'return marker'))
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')
    subject.write_text('import settings\ndef choose(value=settings.marker):\n    return value\n'
                       'def entry():\n    return sink(choose())\n')
    assert not analyze(client, rules, 'entry', imports=True).report.at('/complete')


@pytest.mark.parametrize('value,complete', [('1', True), ('[]', False)])
def test_provider_changes_invalidate_cached_defaults(tmp_path, value, complete):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True)
    assert first.report.at('/complete') and analyze(client, rules, 'entry', cache, imports=True).reused
    (tmp_path / 'settings.py').write_text(f'marker = {value}\n')
    changed = analyze(client, rules, 'entry', cache, imports=True)
    assert not changed.reused and changed.report.at('/complete') is complete
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True))
    assert observe(tmp_path)['result'] == (1 if complete else [])


def test_previous_default_policy_is_rejected_even_after_rehashing(tmp_path):
    client, rules = install(tmp_path)
    value = copy.deepcopy(analyze(client, rules, 'entry', imports=True).report.to_data())
    for contract in (value['call_binding'], value['inputs']['call_binding']):
        contract['definitions'] = PRIOR_DEFINITIONS
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True, ensure_ascii=False,
                                                    separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FunctionSummaries.from_report(FrReport(value, ()))


@pytest.mark.parametrize('bridges,complete', [(14, True), (15, False)])
def test_imported_capture_keeps_the_sixteen_file_and_binding_limits(tmp_path, bridges, complete):
    client, rules = install(tmp_path)
    previous = 'settings'
    for index in range(bridges):
        name = f'bridge{index}'
        (tmp_path / f'{name}.py').write_text(f'from {previous} import marker\n')
        previous = name
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('from settings import marker', f'from {previous} import marker'))
    result = analyze(client, rules, 'entry', imports=True)
    assert result.report.at('/complete') is complete, result.report.at('/cutoffs')
    assert observe(tmp_path)['result'] == 0


@pytest.mark.parametrize('provider', ['class marker:\n    pass\n',
                                     'def marker():\n    return 0\n',
                                     'class Token:\n    pass\nmarker = Token()\n'])
def test_an_explicit_override_does_not_make_object_defaults_scalar(tmp_path, provider):
    client, rules = install(tmp_path, call='choose(source())')
    (tmp_path / 'settings.py').write_text(provider)
    result = analyze(client, rules, 'entry', imports=True)
    assert not result.report.at('/complete')
    assert 'unsupported-parameter-contract:subject.py::choose' in result.report.at('/cutoffs')
    assert observe(tmp_path)['result'] == 17
