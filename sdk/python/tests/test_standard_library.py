"""Check explicit models against Python and preserve local import dependencies."""
import copy
import hashlib
import json
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.flow_standard_library import NAMESPACE_DIGEST, contract
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import ROOT, analyze, evidence, workspace
from test_postponed_annotations import observe


def install(tmp_path, prelude, body):
    client, rules = workspace(tmp_path)
    (tmp_path / 'subject.py').write_text(prelude + '\ndef entry():\n' +
                                        ''.join('    ' + line + '\n' for line in body.splitlines()))
    return client, rules


@pytest.mark.parametrize('expression,expected', [
    ('abs(source())', 17), ('int(source())', 17), ('float(source())', 17.0),
    ('round(source(), 0)', 17), ('min(source(), 50)', 17), ('max(source(), 0)', 17),
    ('min(max(source(), 0), 50)', 17),
])
@pytest.mark.parametrize('blocked', [False, True])
def test_numeric_models_keep_reaching_and_overwritten_answers(tmp_path, expression, expected, blocked):
    body = 'value = ' + expression + '\n' + ('value = 0\n' if blocked else '') + 'return sink(value)'
    client, rules = install(tmp_path, '', body)
    result = analyze(client, rules, 'entry', imports=True, standard_library=('builtins',))
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is not blocked
    assert observe(tmp_path) == {'result': 0 if blocked else expected, 'raised': None,
                                'events': [['source', 17], ['sink', 0 if blocked else expected]]}
    assert result.dependencies.standard_library == ('builtins',)


@pytest.mark.parametrize('declaration,expression', [
    ('import math\n', 'math.ceil(source())'),
    ('import math as numeric\n', 'numeric.floor(source())'),
    ('from math import ceil as ceiling\n', 'ceiling(source())'),
    ('from math import ceil, floor\n', 'ceil(floor(source()))'),
])
def test_standard_imports_keep_aliases_and_absent_candidates(tmp_path, declaration, expression):
    client, rules = install(tmp_path, declaration, 'return sink(' + expression + ')')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('math',))
    assert result.report.at('/complete') and result.witnesses
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17]]
    dependencies = result.dependencies
    assert dependencies.lookup_count == len(dependencies.standard_imports)
    for imported in dependencies.standard_imports:
        assert imported.module == 'math' and imported.importer == 'subject.py'
        assert {candidate.path for candidate in imported.candidates} == {
            'math.py', 'math/__init__.py', 'math.pyi', 'math/__init__.pyi', 'math'}
        assert all(candidate.status == 'missing' for candidate in imported.candidates)
    unfinished = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert not unfinished.report.at('/complete')


CLAMP = '''def relay(value, lower=float('-inf'), upper=float('inf')):
    if upper < lower:
        raise ValueError('expected upper (%r) >= lower (%r)' % (upper, lower))
    return min(max(value, lower), upper)
'''


@pytest.mark.parametrize('arguments', ['source()', 'source(), 0, 50'])
def test_infinity_defaults_and_error_construction_are_checked(tmp_path, arguments):
    client, rules = install(tmp_path, CLAMP, 'return sink(relay(' + arguments + '))')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('builtins',))
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.witnesses and result.summaries.for_function('subject.py::relay').may_raise
    assert observe(tmp_path) == {'result': 17, 'raised': None, 'events': [['source', 17], ['sink', 17]]}


@pytest.mark.parametrize('prefix', [
    'def float(value):\n    return source()\n',
    'float = int\n',
    'import unknown\n',
])
def test_defaults_do_not_assume_a_replaced_or_unchecked_float(tmp_path, prefix):
    client, rules = install(tmp_path, prefix + CLAMP, 'return sink(relay(source(), 0, 50))')
    assert not analyze(client, rules, 'entry', imports=True, standard_library=('builtins',)).report.at('/complete')


@pytest.mark.parametrize('declaration,expression', [
    ('min = 0\n', 'min(source(), 50)'),
    ('', 'min(source())'),
    ('', 'min(source(), 50, key=unknown)'),
    ('', 'int(source(), 10)'),
    ('', 'ValueError(source())'),
    ('import math\n', 'math.missing(source())'),
    ('import math\n', 'math.ceil(value=source())'),
])
def test_shadowing_unsupported_members_and_argument_forms_remain_unknown(tmp_path, declaration, expression):
    client, rules = install(tmp_path, declaration, 'return sink(' + expression + ')')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('builtins', 'math'))
    assert not result.report.at('/complete')


def test_a_new_local_module_invalidates_the_standard_model(tmp_path):
    client, rules = install(tmp_path, 'import math\n', 'return sink(math.ceil(source()))')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True, standard_library=('math',))
    assert first.report.at('/complete') and first.witnesses
    assert analyze(client, rules, 'entry', cache, imports=True, standard_library=('math',)).reused
    (tmp_path / 'math.py').write_text('def ceil(value):\n    return 0\n')
    changed = analyze(client, rules, 'entry', cache, imports=True, standard_library=('math',))
    assert not changed.reused and not changed.report.at('/complete')
    assert changed.dependencies.standard_shadowing == ('math.py',)
    assert not changed.dependencies.standard_imports
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True, standard_library=('math',)))
    local = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert not local.report.at('/complete')
    assert 'standard-library-import-collision' in local.report.at('/cutoffs')


@pytest.mark.parametrize('candidate', ['math.pyi', 'math/__init__.pyi', 'math'])
def test_stubs_and_namespace_directories_cannot_inherit_a_standard_model(tmp_path, candidate):
    client, rules = install(tmp_path, 'import math\n', 'return sink(math.ceil(source()))')
    path = tmp_path / candidate
    if candidate == 'math':
        path.mkdir()
    else:
        path.parent.mkdir(exist_ok=True)
        path.write_text('')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('math',))
    assert not result.report.at('/complete') and not result.dependencies.standard_imports


@pytest.mark.parametrize('candidate', ['math.pyc', 'math.abi3.so', 'math.pyd', 'math.dll', 'operator.py', 'datetime'])
def test_transitive_and_compiled_candidates_invalidate_the_model_premise(tmp_path, candidate):
    client, rules = install(tmp_path, 'import math\n', 'return sink(math.ceil(source()))')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True, standard_library=('math',))
    assert first.report.at('/complete') and first.witnesses
    path = tmp_path / candidate
    if candidate == 'datetime':
        path.mkdir()
    else:
        path.write_bytes(b'not inspected or executed')
    changed = analyze(client, rules, 'entry', cache, imports=True, standard_library=('math',))
    assert not changed.reused and not changed.report.at('/complete')
    assert changed.dependencies.standard_shadowing == (candidate,)


@pytest.mark.parametrize('candidate', ['helpers.pyc', 'helpers.abi3.so', 'helpers/__init__.pyc', 'helpers/__init__.abi3.so'])
def test_opaque_imports_prevent_a_false_source_only_answer(tmp_path, candidate):
    package = candidate.startswith('helpers/')
    helper = tmp_path / ('helpers/__init__.py' if package else 'helpers.py')
    helper.parent.mkdir(exist_ok=True)
    helper.write_text('def relay(value):\n    return value\n')
    client, rules = install(tmp_path, 'from helpers import relay\n', 'return sink(relay(source()))')
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert first.report.at('/complete') and first.witnesses
    (tmp_path / candidate).write_bytes(b'not inspected or executed')
    changed = analyze(client, rules, 'entry', cache, imports=True, standard_library=())
    assert not changed.reused and not changed.report.at('/complete')
    lookup = next(item for item in changed.dependencies.lookups if item.module == 'helpers')
    assert lookup.opaque_candidates == (candidate,)


@pytest.mark.parametrize('module', ['sys', 'builtins'])
def test_intrinsic_modules_are_not_replaced_by_a_same_named_source_file(tmp_path, module):
    (tmp_path / (module + '.py')).write_text('def relay(value):\n    return value\n')
    client, rules = install(tmp_path, 'from ' + module + ' import relay\n', 'return sink(relay(source()))')
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert not result.report.at('/complete')
    assert 'intrinsic-standard-import-unchecked' in result.report.at('/cutoffs')
    assert observe(tmp_path)['raised'] == 'ImportError'


def test_namespace_contract_names_come_from_the_pinned_inventory():
    assert hashlib.sha256((ROOT / 'src/project/python_standard_modules.txt').read_bytes()).hexdigest() == NAMESPACE_DIGEST


@pytest.mark.parametrize('decorator', ['classmethod', 'staticmethod', 'property'])
def test_builtin_descriptors_defer_method_bodies(tmp_path, decorator):
    declaration = ("class Token:\n    __slots__ = ('value',)\n"
                   '    @' + decorator + '\n    def unused(self):\n        return source()\n')
    client, rules = install(tmp_path, declaration, 'return sink(source())')
    result = analyze(client, rules, 'entry', imports=True, standard_library=())
    assert result.report.at('/complete') and result.witnesses
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 17]]


@pytest.mark.parametrize('declaration', [
    "class Token:\n    __slots__ = ('value', 'value')\n",
    "class Token:\n    __slots__ = ('value',)\n    value = 0\n",
    "class Token:\n    __slots__ = ('__dict__',)\n",
    "class Token:\n    classmethod = 0\n    @classmethod\n    def unused(self):\n        pass\n",
    "def classmethod(value):\n    source()\n    return value\nclass Token:\n    @classmethod\n    def unused(self):\n        pass\n",
])
def test_slot_conflicts_and_custom_decorators_stay_unfinished(tmp_path, declaration):
    client, rules = install(tmp_path, declaration, 'return sink(source())')
    assert not analyze(client, rules, 'entry', imports=True, standard_library=()).report.at('/complete')


@pytest.mark.parametrize('change', ['contract', 'candidate', 'member', 'prefix', 'missing-field'])
def test_corrupt_standard_dependencies_are_rejected(tmp_path, change):
    client, rules = install(tmp_path, 'from math import ceil\n', 'return sink(ceil(source()))')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('math',))
    value = copy.deepcopy(result.report.to_data())
    modules = value['inputs']['modules']
    imported = modules['standard_imports'][0]
    if change == 'contract':
        modules['standard_library']['calls'] = 'trust every call'
    elif change == 'candidate':
        imported['candidates']['math.py'] = {'status': 'source', 'digest': '0' * 64}
    elif change == 'member':
        imported['member'] = 'missing'
    elif change == 'prefix':
        imported['prefix'] = 'other'
    else:
        del imported['target']
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True, ensure_ascii=False,
                                                      separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))


@pytest.mark.parametrize('selection', [('unknown',), ('math', 'math'), ['math'], 'math'])
def test_model_selection_must_be_explicit_and_supported(selection):
    with pytest.raises(FrRuntimeError):
        contract(selection)


def test_immutable_date_constructor_keeps_the_year_origin(tmp_path):
    client, rules = install(tmp_path, 'import datetime\n', 'return sink(datetime.date(2000 + source(), 3, 1))')
    result = analyze(client, rules, 'entry', imports=True, standard_library=('datetime',))
    assert result.report.at('/complete') and result.witnesses
    script = '''import builtins, datetime, json, sys
sys.path.insert(0, sys.argv[1])
builtins.source = lambda: 17
builtins.sink = lambda value: value
import subject
value = subject.entry()
assert type(value) is datetime.date
print(json.dumps(value.isoformat()))
'''
    process = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                             check=True, capture_output=True, text=True, timeout=30)
    assert json.loads(process.stdout) == '2017-03-01'
