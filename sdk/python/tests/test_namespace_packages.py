"""Single-root namespace imports checked against CPython and typed dependencies."""
import copy
import hashlib
import json
import subprocess

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_imports import analyze, evidence, workspace


def install(tmp_path, statement='from space.pkg.leaf import identity', expression='identity(source())', regular=()):
    client, rules = workspace(tmp_path)
    (tmp_path / 'space/pkg').mkdir(parents=True)
    for directory in regular:
        (tmp_path / directory / '__init__.py').write_text('')
    (tmp_path / 'space/pkg/leaf.py').write_text('# café 🦀\ndef identity(value):\n    return value\n')
    (tmp_path / 'app.py').write_text(f'{statement}\ndef positive():\n    return sink({expression})\n')
    return client, rules


def runtime(root):
    return subprocess.check_output(['python3', '-B', '-c',
        'import app; app.source=lambda:17; app.sink=lambda x:x; print(app.positive())'], cwd=root, text=True).strip()


@pytest.mark.parametrize('regular', [(), ('space',), ('space/pkg',), ('space', 'space/pkg')])
@pytest.mark.parametrize('statement,expression', [
    ('from space.pkg.leaf import identity', 'identity(source())'),
    ('import space.pkg.leaf', 'space.pkg.leaf.identity(source())'),
    ('import space.pkg.leaf as helper', 'helper.identity(source())'),
    ('from space.pkg import leaf as helper', 'helper.identity(source())'),
    ('from space import pkg\nfrom space.pkg import leaf as helper', 'helper.identity(source())'),
])
def test_namespace_and_regular_combinations_match_python(tmp_path, regular, statement, expression):
    client, rules = install(tmp_path, statement, expression, regular)
    result = analyze(client, rules)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.dependencies.complete and result.witnesses and runtime(tmp_path) == '17'
    expected = tuple(path for path in ('space', 'space/pkg') if path not in regular)
    assert result.dependencies.namespaces == expected
    assert not set(expected) & dict(result.dependencies.files).keys()
    signature = result.summaries.for_function('space/pkg/leaf.py::identity').signature
    assert signature[0].site.path == 'space/pkg/leaf.py'
    (tmp_path / 'space/pkg/leaf.py').write_text('def identity(value):\n    return 0\n')
    repaired = analyze(client, rules)
    assert repaired.report.at('/complete') and not repaired.witnesses and runtime(tmp_path) == '0'


def test_relative_imports_and_namespace_entry(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'app.py').write_text('')
    (tmp_path / 'space/main.py').write_text('from .pkg.leaf import identity\ndef positive():\n    return sink(identity(source()))\n')
    result = analyze(client, rules)
    assert result.report.at('/complete') and result.witnesses
    assert result.dependencies.entry.target == 'space/main.py'
    assert result.dependencies.entry.namespaces == ('space',)
    observed = subprocess.check_output(['python3','-B','-c',
        'import space.main as m; m.source=lambda:17; m.sink=lambda x:x; print(m.positive())'], cwd=tmp_path,text=True)
    assert observed.strip() == '17'


def test_namespace_alias_reexports_have_real_binding_origins(tmp_path):
    client, rules = install(tmp_path, 'from bridge import public\nfrom space.pkg import leaf', 'leaf.identity(source())')
    (tmp_path / 'bridge.py').write_text('import space.pkg as public\n')
    result = analyze(client, rules)
    assert result.report.at('/complete') and runtime(tmp_path) == '17'
    lookup = next(item for item in result.dependencies.lookups if item.alias == 'public' and item.importer == 'app.py')
    assert lookup.terminal_module == 'space/pkg' and lookup.resolution == 'module'
    assert [(hop.path, hop.name) for hop in lookup.binding_chain] == [('bridge.py', 'public')]


def test_empty_namespace_is_an_explicit_directory_dependency(tmp_path):
    client, rules = install(tmp_path, 'import empty\nfrom space.pkg.leaf import identity')
    (tmp_path / 'empty').mkdir()
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    assert first.report.at('/complete') and 'empty' in first.dependencies.namespaces
    assert runtime(tmp_path) == '17'
    (tmp_path / 'empty').rmdir()
    missing = analyze(client, rules, cache=cache)
    assert not missing.reused and not missing.report.at('/complete')
    assert evidence(missing) == evidence(analyze(client, rules))


@pytest.mark.parametrize('mutation,complete', [
    ('initializer', True), ('nested-initializer', True), ('effect', False),
    ('parent-module', False), ('parent-stub', False), ('package-stub', False),
    ('helper', True), ('missing', False), ('unrelated', True), ('sibling', True),
])
def test_namespace_dependency_changes_match_clean_rebuilds(tmp_path, mutation, complete):
    client, rules = install(tmp_path)
    store = MemoryObjectStore(); cache = FlowCache(store)
    first = analyze(client, rules, cache=cache)
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, cache=cache).reused
    changes = {
        'initializer': ('space/__init__.py', '# regular now\n'),
        'nested-initializer': ('space/pkg/__init__.py', ''),
        'effect': ('space/__init__.py', 'state = 1\n'),
        'parent-module': ('space.py', ''), 'parent-stub': ('space.pyi', ''),
        'package-stub': ('space/pkg/__init__.pyi', ''),
        'helper': ('space/pkg/leaf.py', 'def identity(value):\n    return 0\n'),
        'unrelated': ('other.py', 'def other():\n    return 0\n'),
        'sibling': ('space/pkg/sibling.py', 'def sibling():\n    return 0\n'),
    }
    if mutation == 'missing': (tmp_path / 'space/pkg/leaf.py').unlink()
    else:
        path, source = changes[mutation]; (tmp_path / path).write_text(source)
    changed = analyze(client, rules, cache=cache)
    assert changed.reused == (mutation in {'unrelated', 'sibling'})
    assert changed.report.at('/complete') == complete
    assert evidence(changed) == evidence(analyze(client, rules))
    if complete:
        assert runtime(tmp_path) == ('0' if mutation == 'helper' else '17')
        assert all(point.revision == changed.report.at('/revision') for witness in changed.witnesses for point in witness.occurrences)
    assert (changed.report.at('/input_digest') == first.report.at('/input_digest')) == changed.reused


def test_regular_to_namespace_transition_keeps_relative_bindings(tmp_path):
    client, rules = install(tmp_path, regular=('space', 'space/pkg'))
    cache = FlowCache(MemoryObjectStore()); analyze(client, rules, cache=cache)
    (tmp_path / 'space/__init__.py').unlink(); (tmp_path / 'space/pkg/__init__.py').unlink()
    changed = analyze(client, rules, cache=cache)
    assert changed.report.at('/complete') and not changed.reused
    assert changed.dependencies.namespaces == ('space', 'space/pkg')
    assert evidence(changed) == evidence(analyze(client, rules)) and runtime(tmp_path) == '17'


@pytest.mark.parametrize('source', [
    'import space\ndef positive():\n    return sink(space.pkg.leaf.identity(source()))\n',
    'from space import missing\ndef positive():\n    return sink(source())\n',
    'from space import __path__\ndef positive():\n    return sink(source())\n',
    'from space.pkg import *\ndef positive():\n    return sink(source())\n',
])
def test_unloaded_children_and_dynamic_attributes_remain_incomplete(tmp_path, source):
    client, rules = install(tmp_path); (tmp_path / 'app.py').write_text(source)
    assert not analyze(client, rules).report.at('/complete')


@pytest.mark.parametrize('kind', ['symlink', 'ignored-source', 'nondirectory'])
def test_filesystem_boundaries_refuse(tmp_path, kind):
    client, rules = install(tmp_path)
    if kind == 'symlink':
        (tmp_path / 'space').rename(tmp_path / 'actual')
        (tmp_path / 'space').symlink_to('actual', target_is_directory=True)
    elif kind == 'ignored-source':
        subprocess.run(['git', 'init', '-q'], cwd=tmp_path, check=True)
        (tmp_path / '.gitignore').write_text('space/pkg/leaf.py\n')
    else:
        (tmp_path / 'space/pkg/leaf.py').unlink(); (tmp_path / 'space/pkg').rmdir(); (tmp_path / 'space/pkg').write_text('blocked')
    assert not analyze(client, rules).report.at('/complete')


def test_namespace_directories_share_the_module_budget(tmp_path):
    client, rules = workspace(tmp_path)
    lines = []
    for index in range(9):
        (tmp_path / f'ns{index}').mkdir()
        (tmp_path / f'ns{index}/leaf.py').write_text('def identity(value):\n    return value\n')
        lines.append(f'from ns{index}.leaf import identity as item{index}')
    (tmp_path / 'app.py').write_text('\n'.join(lines)+'\ndef positive():\n    return sink(item0(source()))\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete') and 'import-module-budget' in result.report.at('/cutoffs')
    assert len(result.dependencies.files) + len(result.dependencies.namespaces) <= 16


@pytest.mark.parametrize('mutation', ['missing', 'duplicate', 'source', 'directory', 'candidate', 'lookup', 'extra', 'legacy', 'terminal'])
def test_forged_namespace_dependencies_refuse_with_rehashed_inputs(tmp_path, mutation):
    client, rules = install(tmp_path, 'from space.pkg import leaf as helper', 'helper.identity(source())')
    data = copy.deepcopy(analyze(client, rules).report.to_data()); modules = data['inputs']['modules']
    lookup = next(item for item in modules['lookups'] if item['importer'] == 'app.py')
    if mutation == 'missing': modules['namespaces'].remove('space')
    elif mutation == 'duplicate': modules['namespaces'].append('space')
    elif mutation == 'source': modules['namespaces'].append('app.py')
    elif mutation == 'directory': lookup['candidates']['space']['status'] = 'missing'
    elif mutation == 'candidate': del lookup['candidates']['space']
    elif mutation == 'lookup': lookup['namespaces'] = []
    elif mutation == 'extra': modules['namespaces'].append('zombie')
    elif mutation == 'legacy': modules['schema'] = 'fr-flow-modules-4'
    else: lookup['terminal_module'] = 'space'
    raw = json.dumps(data['inputs'], sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()
    data['input_digest'] = hashlib.sha256(raw).hexdigest()
    with pytest.raises(FrRuntimeError): FlowDependencies.from_report(FrReport(data, ()))
