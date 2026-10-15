"""Compare import order with independent Python execution."""
import json
import subprocess

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_dependencies import FlowDependencies
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_imports import analyze, evidence, workspace


def install(tmp_path, files):
    client, rules = workspace(tmp_path)
    for name, source in files.items():
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source)
    return client, rules


def runtime(tmp_path):
    script = (
        "import json, builtins\n"
        "builtins.source=lambda:17\n"
        "builtins.sink=lambda value:value\n"
        "try:\n"
        " import app\n"
        " app.source=lambda:17\n"
        " app.sink=lambda value:value\n"
        " print(json.dumps({'returned':app.positive()}))\n"
        "except Exception as error:\n"
        " print(json.dumps({'raised':type(error).__name__}))\n"
    )
    return json.loads(subprocess.check_output(['python3', '-B', '-c', script], cwd=tmp_path, text=True))


APP = 'from alpha import relay\ndef positive():\n    return sink(relay(source()))\n'
SAFE = [
    ('early-function', 'def relay(value):\n    return ANSWER\nfrom beta import other\n',
     'from alpha import relay\ndef other():\n    return 0\n'),
    ('module-handles', 'import beta\ndef relay(value):\n    return ANSWER\n',
     'import alpha\ndef other():\n    return 0\n'),
    ('self-handle', 'import alpha\ndef relay(value):\n    return ANSWER\n', ''),
    ('self-member', 'def relay(value):\n    return ANSWER\nfrom alpha import relay as public\n', ''),
    ('reexport', 'def relay(value):\n    return ANSWER\nfrom beta import public\n',
     'from alpha import relay as public\n'),
    ('named-default', 'LIMIT = 0\nfrom beta import other\ndef relay(value):\n    return ANSWER\n',
     'from alpha import LIMIT\ndef other(value=LIMIT):\n    return value\n'),
    ('early-star', "__all__ = ['relay']\ndef relay(value):\n    return ANSWER\nfrom beta import other\n",
     'from alpha import *\ndef other():\n    return 0\n'),
    ('two-available-members', 'LIMIT = 0\ndef relay(value):\n    return ANSWER\nfrom beta import other\n',
     'from alpha import LIMIT, relay\ndef other():\n    return 0\n'),
]


@pytest.mark.parametrize('label,alpha,beta', SAFE)
@pytest.mark.parametrize('answer', ['value', '0'])
def test_supported_cycles_match_runtime_and_exact_locations(tmp_path, label, alpha, beta, answer):
    client, rules = install(tmp_path, {'app.py': APP, 'alpha.py': alpha.replace('ANSWER', answer), 'beta.py': beta})
    result = analyze(client, rules)
    assert result.report.at('/complete'), (label, result.report.at('/cutoffs'))
    assert result.dependencies.complete and bool(result.witnesses) == (answer == 'value')
    assert runtime(tmp_path) == {'returned': 17 if answer == 'value' else 0}
    assert result.summaries.for_function('alpha.py::relay').return_parameters == ((0,) if answer == 'value' else ())
    for witness in result.witnesses:
        assert {point.path for point in witness.occurrences} == {'app.py', 'alpha.py'}


UNAVAILABLE = [
    ('late-function', 'from beta import other\ndef relay(value):\n    return value\n',
     'from alpha import relay\ndef other():\n    return 0\n', 'ImportError'),
    ('late-literal', 'from beta import other\nLIMIT = 0\ndef relay(value):\n    return value\n',
     'from alpha import LIMIT\ndef other(value=LIMIT):\n    return value\n', 'ImportError'),
    ('second-member', 'LIMIT = 0\nfrom beta import other\ndef relay(value):\n    return value\n',
     'from alpha import LIMIT, relay\ndef other():\n    return 0\n', 'ImportError'),
    ('late-star-member', "__all__ = ['relay']\nfrom beta import other\ndef relay(value):\n    return value\n",
     'from alpha import *\ndef other():\n    return 0\n', 'AttributeError'),
    ('reexport-loop', 'from beta import relay\n', 'from alpha import relay\n', 'ImportError'),
]


@pytest.mark.parametrize('label,alpha,beta,error', UNAVAILABLE)
def test_unavailable_cycle_members_never_establish_absence(tmp_path, label, alpha, beta, error):
    client, rules = install(tmp_path, {'app.py': APP, 'alpha.py': alpha, 'beta.py': beta})
    result = analyze(client, rules)
    assert runtime(tmp_path) == {'raised': error}, label
    assert not result.report.at('/complete')
    assert 'cyclic-module-initialization' in result.report.at('/cutoffs')
    assert not result.dependencies.complete


@pytest.mark.parametrize('early', [True, False])
@pytest.mark.parametrize('answer', ['value', '0'])
def test_parent_package_initializes_before_its_child_entry(tmp_path, early, answer):
    definition = f'def relay(value):\n    return {answer}\n'
    initializer = definition + 'from . import child\n' if early else 'from . import child\n' + definition
    client, rules = install(tmp_path, {
        'pkg/__init__.py': initializer,
        'pkg/child.py': 'from pkg import relay\ndef positive():\n    return sink(relay(source()))\n',
        'app.py': 'from pkg.child import positive\n',
    })
    handle = client.project('find', 'positive').definition_target().handle
    result = FlowCache(MemoryObjectStore()).analyze(client, handle, rules=rules, steps=4096,
                                                   max_bytes=1_048_576, summaries=True, imports=True)
    assert result.report.at('/complete') == early
    if early:
        assert bool(result.witnesses) == (answer == 'value')
        assert runtime(tmp_path) == {'returned': 17 if answer == 'value' else 0}
    else:
        assert 'cyclic-module-initialization' in result.report.at('/cutoffs')
        assert runtime(tmp_path) == {'raised': 'ImportError'}


def test_source_order_changes_invalidate_saved_answers(tmp_path):
    definition = 'def relay(value):\n    return value\n'
    imports = 'from beta import other\n'
    client, rules = install(tmp_path, {
        'app.py': APP, 'alpha.py': definition + imports,
        'beta.py': 'from alpha import relay\ndef other():\n    return 0\n',
    })
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache=cache)
    assert first.report.at('/complete') and first.witnesses
    assert analyze(client, rules, cache=cache).reused
    (tmp_path / 'alpha.py').write_text(imports + definition)
    changed = analyze(client, rules, cache=cache)
    assert not changed.reused and not changed.report.at('/complete')
    assert changed.report.at('/input_digest') != first.report.at('/input_digest')
    assert evidence(changed) == evidence(analyze(client, rules))
    assert runtime(tmp_path) == {'raised': 'ImportError'}
    (tmp_path / 'alpha.py').write_text(definition.replace('return value', 'return 0') + imports)
    repaired = analyze(client, rules, cache=cache)
    assert repaired.report.at('/complete') and not repaired.witnesses
    assert evidence(repaired) == evidence(analyze(client, rules))
    assert runtime(tmp_path) == {'returned': 0}


@pytest.mark.parametrize('count', [15, 16])
def test_cycles_keep_the_existing_file_limit(tmp_path, count):
    files = {'app.py': 'from module0 import relay\ndef positive():\n    return sink(relay(source()))\n'}
    for index in range(count):
        files[f'module{index}.py'] = f'import module{(index + 1) % count}\ndef relay(value):\n    return value\n'
    client, rules = install(tmp_path, files)
    result = analyze(client, rules)
    assert runtime(tmp_path) == {'returned': 17}
    assert result.report.at('/complete') == (count == 15)
    if count == 16:
        assert 'import-module-budget' in result.report.at('/cutoffs')


def test_unknown_startup_effects_stay_incomplete_after_cycle_check(tmp_path):
    client, rules = install(tmp_path, {
        'app.py': APP,
        'alpha.py': 'import beta\nstate = 1 + 2\ndef relay(value):\n    return value\n',
        'beta.py': 'import alpha\n',
    })
    result = analyze(client, rules)
    assert runtime(tmp_path) == {'returned': 17}
    assert not result.report.at('/complete')
    assert 'module-effects-unchecked' in result.report.at('/cutoffs')
    assert result.initialization.sites


def test_obsolete_import_policy_cannot_validate_current_answers(tmp_path):
    client, rules = install(tmp_path, {'app.py': APP, 'alpha.py': 'import alpha\ndef relay(value):\n    return value\n'})
    result = analyze(client, rules)
    value = result.report.to_data()
    value['inputs']['modules']['policy'] = 'accept every import cycle'
    with pytest.raises(FrRuntimeError):
        FlowDependencies.from_report(FrReport(value, ()))
