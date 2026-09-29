"""Module aliases checked against CPython, fresh analysis and dependency forgery."""
import hashlib
import json
import subprocess

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_imports import analyze, evidence, workspace


def install(tmp_path):
    client, rules = workspace(tmp_path)
    (tmp_path / 'pkg').mkdir()
    (tmp_path / 'pkg/__init__.py').write_text('from . import leaf as public\n')
    (tmp_path / 'pkg/leaf.py').write_text('# café 🦀\ndef identity(value):\n    return value\n')
    (tmp_path / 'bridge.py').write_text('from pkg import public as relay\n')
    (tmp_path / 'app.py').write_text('from bridge import relay as forward\ndef positive():\n    return sink(forward.identity(source()))\n')
    return client, rules


def runtime(root):
    return subprocess.check_output(['python3', '-B', '-c',
        'import app; app.source=lambda:17; app.sink=lambda x:x; print(app.positive())'], cwd=root, text=True).strip()


@pytest.mark.parametrize('source', [
    'from bridge import relay as forward\ndef positive():\n    return sink(forward.identity(source()))\n',
    'from pkg import leaf as helper\ndef positive():\n    return sink(helper.identity(source()))\n',
    'import pkg\ndef positive():\n    return sink(pkg.public.identity(source()))\n',
    'import pkg.leaf\ndef positive():\n    return sink(pkg.leaf.identity(source()))\n',
    'import bridge as b\ndef positive():\n    return sink(b.relay.identity(source()))\n',
])
def test_module_aliases_match_runtime_and_exact_origins(tmp_path, source):
    client, rules = install(tmp_path)
    (tmp_path / 'app.py').write_text(source)
    result = analyze(client, rules)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.dependencies.complete and result.witnesses
    assert runtime(tmp_path) == '17'
    assert result.summaries.for_function('pkg/leaf.py::identity').return_parameters == (0,)
    assert {point.path for point in result.witnesses[0].occurrences} == {'app.py', 'pkg/leaf.py'}
    (tmp_path / 'pkg/leaf.py').write_text('def identity(value):\n    return 0\n')
    changed = analyze(client, rules)
    assert changed.report.at('/complete') and not changed.witnesses
    assert runtime(tmp_path) == '0'


def test_dependency_records_keep_module_chain_and_fallback_candidates(tmp_path):
    client, rules = install(tmp_path)
    deps = analyze(client, rules).dependencies
    outer = next(item for item in deps.lookups if item.importer == 'app.py')
    assert outer.resolution == 'module' and outer.terminal_module == 'pkg/leaf.py'
    assert [(hop.path, hop.name) for hop in outer.binding_chain] == [('bridge.py', 'relay'), ('pkg/__init__.py', 'public')]
    fallback = next(item for item in deps.lookups if item.importer == 'pkg/__init__.py')
    assert fallback.target == 'pkg/__init__.py' and fallback.member == 'leaf'
    assert fallback.submodule.module == 'pkg.leaf' and fallback.submodule.target == 'pkg/leaf.py'
    assert not fallback.binding_chain
    assert {item.path for item in fallback.submodule.candidates} == {
        'pkg', 'pkg/leaf', 'pkg.py', 'pkg.pyi', 'pkg/__init__.py', 'pkg/__init__.pyi',
        'pkg/leaf.py', 'pkg/leaf.pyi', 'pkg/leaf/__init__.py', 'pkg/leaf/__init__.pyi'}


@pytest.mark.parametrize('initializer', ['from . import leaf\n', 'import pkg.leaf as leaf\n', 'import pkg.leaf as public\nfrom pkg import public as leaf\n'])
def test_matching_child_binding_and_plain_initializer_imports(tmp_path, initializer):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text(initializer)
    (tmp_path / 'app.py').write_text('from pkg import leaf\ndef positive():\n    return sink(leaf.identity(source()))\n')
    result = analyze(client, rules)
    # Reading another member back from the initializing package is deliberately cyclic.
    expected = 'from pkg' not in initializer
    assert result.report.at('/complete') == expected
    if expected:
        assert result.witnesses and runtime(tmp_path) == '17'
    else:
        assert 'cyclic-module-initialization' in result.report.at('/cutoffs')


def test_bare_relative_child_and_nested_package_alias(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/nested').mkdir()
    (tmp_path / 'pkg/nested/__init__.py').write_text('from .. import leaf as helper\n')
    (tmp_path / 'pkg/use.py').write_text('from . import nested as inner\ndef use(value):\n    return inner.helper.identity(value)\n')
    (tmp_path / 'app.py').write_text('from pkg.use import use\ndef positive():\n    return sink(use(source()))\n')
    result = analyze(client, rules)
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert result.dependencies.complete and result.witnesses and runtime(tmp_path) == '17'


def test_function_member_wins_over_unloaded_child_candidate(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text('def leaf(value):\n    return value\n')
    (tmp_path / 'app.py').write_text('from pkg import leaf\ndef positive():\n    return sink(leaf(source()))\n')
    result = analyze(client, rules)
    assert result.report.at('/complete') and result.witnesses and runtime(tmp_path) == '17'
    assert 'pkg/leaf.py' not in dict(result.dependencies.files)
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert lookup.resolution == 'function' and lookup.submodule is None


@pytest.mark.parametrize('path,source,cutoff', [
    ('pkg/leaf.py', 'from bridge import relay\ndef identity(value):\n    return value\n', 'cyclic-module-initialization'),
    ('pkg/__init__.py', 'from . import leaf as public\ndef leaf(value):\n    return value\n', 'unknown-external-call:forward.identity'),
    ('pkg/__init__.py', 'from . import leaf as public\nstate = 1\n', 'module-effects-unchecked'),
    ('pkg/__init__.py', 'from . import leaf as public\nfrom erase import forward as public\n', 'ambiguous-module-binding'),
    ('pkg/__init__.py', 'import pkg.leaf\n', 'missing-or-ambiguous-import-member:relay'),
    ('pkg/__init__.py', 'from . import *\n', 'unsupported-import-form'),
    ('pkg/leaf.pyi', '', 'unresolved-local-import:pkg.leaf'),
    ('pkg/leaf/__init__.py', '', 'unresolved-local-import:pkg.leaf'),
    ('pkg/__init__.pyi', '', 'unresolved-local-import:pkg'),
    ('pkg/leaf.py', 'def other(value):\n    return value\n', 'unknown-external-call:forward.identity'),
])
def test_incomplete_boundaries(tmp_path, path, source, cutoff):
    client, rules = install(tmp_path)
    target = tmp_path / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(source)
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert cutoff in result.report.at('/cutoffs'), result.report.at('/cutoffs')
    result.dependencies


@pytest.mark.parametrize('mutation', ['leaf', 'alias', 'missing', 'stub', 'package', 'initializer', 'unrelated'])
def test_cache_and_clean_analysis_agree_after_module_changes(tmp_path, mutation):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    assert analyze(client, rules, cache=cache).reused
    if mutation == 'leaf':
        (tmp_path / 'pkg/leaf.py').write_text('def identity(value):\n    return 0\n')
    elif mutation == 'alias':
        (tmp_path / 'bridge.py').write_text('import erase as relay\n')
    elif mutation == 'missing':
        (tmp_path / 'pkg/leaf.py').unlink()
    elif mutation == 'stub':
        (tmp_path / 'pkg/leaf.pyi').write_text('')
    elif mutation == 'package':
        (tmp_path / 'pkg/leaf').mkdir()
        (tmp_path / 'pkg/leaf/__init__.py').write_text('')
    elif mutation == 'initializer':
        (tmp_path / 'pkg/__init__.py').write_text('from . import leaf as public\n# changed\n')
    else:
        (tmp_path / 'unrelated.py').write_text('def other():\n    return 42\n')
    changed = analyze(client, rules, cache=cache)
    assert changed.reused == (mutation == 'unrelated')
    assert evidence(changed) == evidence(analyze(client, rules))
    assert (changed.report.at('/input_digest') == first.report.at('/input_digest')) == (mutation == 'unrelated')
    assert changed.report.at('/complete') == (mutation in {'leaf', 'initializer', 'unrelated'})
    changed.dependencies


def test_negative_child_lookup_is_invalidated_when_module_appears(tmp_path):
    client, rules = install(tmp_path)
    leaf = tmp_path / 'pkg/leaf.py'
    source = leaf.read_text()
    leaf.unlink()
    first = analyze(client, rules)
    assert not first.report.at('/complete')
    child = next(item.submodule for item in first.dependencies.lookups if item.submodule is not None)
    assert not child.admitted
    leaf.write_text(source)
    second = analyze(client, rules)
    assert second.report.at('/complete') and second.witnesses
    assert first.report.at('/input_digest') != second.report.at('/input_digest')


@pytest.mark.parametrize('mutation', ['omit-chain', 'skip-hop', 'terminal', 'nonmodule-terminal', 'fallback',
    'omit-candidate', 'candidate-digest', 'child-name', 'child-target', 'child-chain', 'escape', 'omit-terminal', 'special-child'])
def test_typed_module_dependencies_reject_forgery(tmp_path, mutation):
    client, rules = install(tmp_path)
    value = analyze(client, rules).report.to_data()
    lookups = value['inputs']['modules']['lookups']
    outer = next(item for item in lookups if item['importer'] == 'app.py')
    fallback = next(item for item in lookups if item['importer'] == 'pkg/__init__.py')
    if mutation == 'omit-chain':
        outer['binding_chain'].clear()
    elif mutation == 'skip-hop':
        outer['binding_chain'].pop()
    elif mutation == 'terminal':
        outer['terminal_module'] = 'bridge.py'
    elif mutation == 'nonmodule-terminal':
        outer['resolution'] = 'function'
    elif mutation == 'fallback':
        fallback['submodule'] = None
    elif mutation == 'omit-candidate':
        del fallback['submodule']['candidates']['pkg/leaf.pyi']
    elif mutation == 'candidate-digest':
        fallback['submodule']['candidates']['pkg/leaf.py']['digest'] = '0' * 64
    elif mutation == 'child-name':
        fallback['submodule']['module'] = 'different'
    elif mutation == 'child-target':
        fallback['submodule']['target'] = 'bridge.py'
    elif mutation == 'child-chain':
        fallback['binding_chain'] = [{'path': 'pkg/__init__.py', 'name': 'leaf'}]
    elif mutation == 'escape':
        outer['terminal_module'] = '../escape.py'
    elif mutation == 'special-child':
        fallback['member'] = '__name__'
        child = fallback['submodule']
        child['module'] = 'pkg.__name__'
        child['target'] = 'pkg/__name__.py'
        child['candidates'] = {path.replace('pkg/leaf', 'pkg/__name__'): item
                               for path, item in child['candidates'].items()}
        files = value['inputs']['modules']['files']
        files['pkg/__name__.py'] = files['pkg/leaf.py']
        for item in lookups:
            if item['terminal_module'] == 'pkg/leaf.py':
                item['terminal_module'] = 'pkg/__name__.py'
    else:
        del outer['terminal_module']
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True,
        ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))


def test_package_child_fallback_keeps_explicit_initializer_cycle(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text('from app import positive\n')
    (tmp_path / 'app.py').write_text('from pkg import leaf\ndef positive():\n    return sink(leaf.identity(source()))\n')
    observed = subprocess.check_output(['python3', '-B', '-c',
        'try:\n import app\nexcept ImportError as error:\n print(type(error).__name__, error.name)\n'
        'else:\n raise AssertionError("expected an import cycle failure")'], cwd=tmp_path, text=True)
    assert observed.strip() == 'ImportError app'
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'cyclic-module-initialization' in result.report.at('/cutoffs')
    result.dependencies


def test_unaliased_dotted_import_cannot_be_reexported_as_its_leaf(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'bridge.py').write_text('import pkg.leaf\n')
    (tmp_path / 'app.py').write_text('from bridge import pkg as forward\ndef positive():\n    return sink(forward.identity(source()))\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'missing-or-ambiguous-import-member:forward' in result.report.at('/cutoffs')
    failed = subprocess.run(['python3', '-B', '-c', 'import app; app.source=lambda:17; app.sink=lambda x:x; app.positive()'],
                            cwd=tmp_path, capture_output=True, text=True)
    assert failed.returncode != 0 and 'AttributeError' in failed.stderr


def test_module_alias_chain_budget_retains_incomplete_dependency_record(tmp_path):
    client, rules = install(tmp_path)
    even, odd = [], []
    for number in range(20):
        line = f'from {"right" if number % 2 == 0 else "left"} import n{number + 1} as n{number}\n'
        (even if number % 2 == 0 else odd).append(line)
    even.append('import pkg.leaf as n20\n')
    (tmp_path / 'left.py').write_text(''.join(even))
    (tmp_path / 'right.py').write_text(''.join(odd))
    (tmp_path / 'app.py').write_text('from left import n0\ndef positive():\n    return sink(n0.identity(source()))\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert lookup.resolution == 'budget' and len(lookup.binding_chain) == 16


@pytest.mark.parametrize('module,path', [('__init__', '__init__.py'), ('pkg.__init__', 'pkg/__init__.py')])
def test_plain_init_module_does_not_acquire_package_fallback(tmp_path, module, path):
    client, rules = install(tmp_path)
    (tmp_path / path).write_text('pass\n')
    (tmp_path / 'app.py').write_text(f'from {module} import leaf\ndef positive():\n    return sink(leaf.identity(source()))\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert lookup.resolution == 'missing' and lookup.submodule is None


@pytest.mark.parametrize('hook', [
    'def __getattr__(name):\n    return 0\n',
    'from erase import forward as __getattr__\n',
])
def test_module_attribute_hooks_cannot_establish_static_child_resolution(tmp_path, hook):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text(hook + 'from . import leaf as public\n')
    observed = subprocess.run(['python3', '-B', '-c',
        'import app; app.source=lambda:17; app.sink=lambda x:x; app.positive()'],
        cwd=tmp_path, capture_output=True, text=True)
    assert observed.returncode != 0 and 'AttributeError' in observed.stderr
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'special-module-attributes' in result.report.at('/cutoffs')
    result.dependencies


@pytest.mark.parametrize('attribute', ['__name__', '__dict__', '__class__', '__path__', '__doc__'])
def test_implicit_package_attributes_cannot_fall_back_to_same_named_children(tmp_path, attribute):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text('pass\n')
    (tmp_path / f'pkg/{attribute}.py').write_text('def identity(value):\n    return value\n')
    (tmp_path / 'app.py').write_text(f'from pkg import {attribute} as forward\ndef positive():\n    return sink(forward.identity(source()))\n')
    observed = subprocess.run(['python3', '-B', '-c',
        'import app; app.source=lambda:17; app.sink=lambda x:x; app.positive()'],
        cwd=tmp_path, capture_output=True, text=True)
    assert observed.returncode != 0 and 'AttributeError' in observed.stderr
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'special-module-attributes' in result.report.at('/cutoffs')
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert lookup.submodule is None and lookup.resolution == 'missing'
    assert f'pkg/{attribute}.py' not in dict(result.dependencies.files)


def test_adding_module_attribute_hook_invalidates_cached_complete_analysis(tmp_path):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    assert first.report.at('/complete') and analyze(client, rules, cache=cache).reused
    path = tmp_path / 'pkg/__init__.py'
    path.write_text('def __getattr__(name):\n    return 0\n' + path.read_text())
    changed = analyze(client, rules, cache=cache)
    assert not changed.reused and not changed.report.at('/complete')
    assert evidence(changed) == evidence(analyze(client, rules))
    assert first.report.at('/input_digest') != changed.report.at('/input_digest')
    changed.dependencies


@pytest.mark.parametrize('attribute', ['__getattr__', '__path__'])
def test_loading_special_child_modules_cannot_install_parent_metadata(tmp_path, attribute):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/__init__.py').write_text('pass\n')
    (tmp_path / f'pkg/{attribute}.py').write_text('def identity(value):\n    return value\n')
    (tmp_path / 'app.py').write_text(f'import pkg.{attribute} as metadata\nfrom pkg import leaf\ndef positive():\n    return sink(leaf.identity(source()))\n')
    observed = subprocess.run(['python3', '-B', '-c', 'import app'], cwd=tmp_path, capture_output=True, text=True)
    assert observed.returncode != 0 and 'TypeError' in observed.stderr
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'special-module-attributes' in result.report.at('/cutoffs')
    result.dependencies


def test_special_child_entry_cannot_bypass_module_metadata_boundary(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'app.py').write_text('pass\n')
    (tmp_path / 'pkg/__path__.py').write_text('def positive():\n    return sink(source())\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert 'special-module-attributes' in result.report.at('/cutoffs')
    assert result.dependencies.entry.module == 'pkg.__path__'


def test_typed_complete_record_cannot_select_special_child_module(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'app.py').write_text('import pkg.leaf as forward\ndef positive():\n    return sink(forward.identity(source()))\n')
    value = analyze(client, rules).report.to_data()
    modules = value['inputs']['modules']
    lookup = next(item for item in modules['lookups'] if item['importer'] == 'app.py')
    lookup['module'] = 'pkg.__path__'
    lookup['target'] = lookup['terminal_module'] = 'pkg/__path__.py'
    lookup['candidates'] = {path.replace('pkg/leaf', 'pkg/__path__'): item
                            for path, item in lookup['candidates'].items()}
    modules['files']['pkg/__path__.py'] = modules['files']['pkg/leaf.py']
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True,
        ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))
