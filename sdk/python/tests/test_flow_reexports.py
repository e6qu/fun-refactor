"""Independent runtime, dependency and refusal checks for function re-exports."""
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
    (tmp_path / 'pkg/__init__.py').write_text('from .bridge import outward as exported\n')
    (tmp_path / 'pkg/bridge.py').write_text('from .leaf import identity as outward\n')
    (tmp_path / 'pkg/leaf.py').write_text('# café 🦀\ndef identity(value):\n    return value\n')
    (tmp_path / 'app.py').write_text('from pkg import exported as forward\ndef positive():\n    return sink(forward(source()))\n')
    return client, rules


def test_alias_chain_matches_cpython_and_exact_definition(tmp_path):
    client, rules = install(tmp_path)
    result = analyze(client, rules)
    assert result.report.at('/complete') and result.witnesses
    observed = subprocess.check_output(['python3', '-B', '-c',
        'import app; app.source=lambda:17; app.sink=lambda x:x; print(app.positive())'], cwd=tmp_path, text=True)
    assert observed.strip() == '17'
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert [(hop.path, hop.name) for hop in lookup.binding_chain] == [
        ('pkg/__init__.py', 'exported'), ('pkg/bridge.py', 'outward'), ('pkg/leaf.py', 'identity')]
    assert result.summaries.for_function('pkg/leaf.py::identity').return_parameters == (0,)
    assert {point.path for point in result.witnesses[0].occurrences} == {'app.py', 'pkg/leaf.py'}
    (tmp_path / 'pkg/leaf.py').write_text('def identity(value):\n    return 0\n')
    assert not analyze(client, rules).witnesses


@pytest.mark.parametrize('source', [
    'import pkg\ndef positive():\n    return sink(pkg.exported(source()))\n',
    'from pkg.bridge import outward\ndef positive():\n    return sink(outward(source()))\n',
])
def test_module_and_direct_imports_share_terminal_identity(tmp_path, source):
    client, rules = install(tmp_path)
    (tmp_path / 'app.py').write_text(source)
    result = analyze(client, rules)
    assert result.report.at('/complete') and result.witnesses
    assert result.summaries.for_function('pkg/leaf.py::identity').return_parameters == (0,)


def test_bare_relative_function_import_resolves_parent_binding(tmp_path):
    client, rules = install(tmp_path)
    (tmp_path / 'pkg/use.py').write_text('from . import exported\ndef use(value):\n    return exported(value)\n')
    (tmp_path / 'app.py').write_text('from pkg.use import use\ndef positive():\n    return sink(use(source()))\n')
    result = analyze(client, rules)
    assert result.report.at('/complete') and result.witnesses


@pytest.mark.parametrize('path,source,cutoff', [
    ('pkg/bridge.py', 'from pkg import exported as outward\n', 'cyclic-module-initialization'),
    ('pkg/__init__.py', 'from .bridge import missing as exported\n', 'missing-or-ambiguous-import-member:exported'),
    ('pkg/__init__.py', 'from .bridge import outward as exported\nfrom .leaf import identity as exported\n', 'ambiguous-module-binding'),
    ('pkg/__init__.py', 'from .bridge import outward as exported\ndef exported(value):\n    return 0\n', 'ambiguous-module-binding'),
    ('pkg/__init__.py', 'from .bridge import outward as exported\nstate = 1\n', 'module-effects-unchecked'),
    ('pkg/__init__.py', 'import pkg.leaf as exported\n', 'unknown-external-call:forward'),
    ('pkg/bridge.py', 'import pkg.leaf as outward\n', 'unknown-external-call:forward'),
    ('pkg/__init__.py', 'from .bridge import *\n', 'unsupported-import-form'),
    ('pkg/__init__.py', 'from .bridge import outward as exported\ndef leaf(value):\n    return 0\n', 'package-child-binding-conflict'),
    ('pkg/leaf.py', 'def identity(value=source()):\n    return value\n', 'module-effects-unchecked'),
])
def test_reexport_boundaries_never_establish_complete_analysis(tmp_path, path, source, cutoff):
    client, rules = install(tmp_path)
    (tmp_path / path).write_text(source)
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    assert cutoff in result.report.at('/cutoffs')
    result.dependencies


@pytest.mark.parametrize('mutation', ['alias', 'missing', 'stub', 'initializer', 'unrelated'])
def test_reexport_cache_matches_clean_analysis_after_changes(tmp_path, mutation):
    client, rules = install(tmp_path)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    assert analyze(client, rules, cache=cache).reused
    if mutation == 'alias':
        (tmp_path / 'pkg/bridge.py').write_text('from erase import forward as outward\n')
    elif mutation == 'missing':
        (tmp_path / 'pkg/leaf.py').write_text('def renamed(value):\n    return value\n')
    elif mutation == 'stub':
        (tmp_path / 'pkg/bridge.pyi').write_text('')
    elif mutation == 'initializer':
        with (tmp_path / 'pkg/__init__.py').open('a') as stream:
            stream.write('# changed initializer\n')
    else:
        (tmp_path / 'unrelated.py').write_text('def other():\n    return 42\n')
    changed = analyze(client, rules, cache=cache)
    assert changed.reused == (mutation == 'unrelated')
    assert evidence(changed) == evidence(analyze(client, rules))
    assert (changed.report.at('/input_digest') == first.report.at('/input_digest')) == (mutation == 'unrelated')
    assert changed.report.at('/complete') == (mutation not in {'missing', 'stub'})
    if mutation == 'alias':
        assert not changed.witnesses


@pytest.mark.parametrize('mutation', ['omit', 'skip', 'terminal', 'repeat', 'outside', 'rename', 'status'])
def test_typed_reexport_chain_rejects_forgery_with_recomputed_digest(tmp_path, mutation):
    client, rules = install(tmp_path)
    value = analyze(client, rules).report.to_data()
    lookups = value['inputs']['modules']['lookups']
    lookup = next(item for item in lookups if item['importer'] == 'app.py')
    chain = lookup['binding_chain']
    if mutation == 'omit':
        chain.clear()
    elif mutation == 'skip':
        del chain[1]
    elif mutation == 'terminal':
        chain.pop()
    elif mutation == 'repeat':
        chain.append(chain[0])
    elif mutation == 'outside':
        chain[-1]['path'] = '../outside.py'
    elif mutation == 'rename':
        chain[-1]['name'] = 'renamed'
    else:
        lookup['resolution'] = 'module'
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True,
        ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))


def test_binding_chain_budget_stops_before_unbounded_alias_chasing(tmp_path):
    client, rules = install(tmp_path)
    even, odd = [], []
    for number in range(20):
        line = f'from {"right" if number % 2 == 0 else "left"} import n{number + 1} as n{number}\n'
        (even if number % 2 == 0 else odd).append(line)
    even.append('def n20(value):\n    return value\n')
    (tmp_path / 'left.py').write_text(''.join(even))
    (tmp_path / 'right.py').write_text(''.join(odd))
    (tmp_path / 'app.py').write_text('from left import n0\ndef positive():\n    return sink(n0(source()))\n')
    result = analyze(client, rules)
    assert not result.report.at('/complete')
    lookup = next(item for item in result.dependencies.lookups if item.importer == 'app.py')
    assert lookup.resolution == 'budget' and len(lookup.binding_chain) == 16
