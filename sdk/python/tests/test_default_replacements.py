"""Explicit arguments replace checked defaults without consuming stored objects."""
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace
from test_postponed_annotations import observe


SETUPS = [
    'marker = []\n', 'marker = (0,)\n', 'marker = dict(send=lambda: source())\n',
    'def marker(value):\n    return source()\n',
    'class Token:\n    pass\nmarker = Token()\n', 'marker = sorted\n',
]


def install(tmp_path, setup=SETUPS[0], route='local', parameters='value=((marker))',
            call='choose(source())', body='return value'):
    client, rules = workspace(tmp_path)
    helper = setup + f'def choose({parameters}):\n    {body}\n'
    if route == 'local':
        prelude = helper
    else:
        (tmp_path / 'helpers.py').write_text(helper)
        prelude = 'from helpers import choose\n'
        if route == 'alias':
            prelude = 'from helpers import choose as selected\n'
            call = call.replace('choose', 'selected')
        elif route == 'reexport':
            (tmp_path / 'bridge.py').write_text('from helpers import choose\n')
            prelude = 'from bridge import choose\n'
        elif route == 'star':
            (tmp_path / 'helpers.py').write_text("__all__ = ['choose']\n" + helper)
            prelude = 'from helpers import *\n'
    (tmp_path / 'subject.py').write_text(prelude + f'def entry():\n    return sink({call})\n')
    return client, rules


@pytest.mark.parametrize('setup', SETUPS)
@pytest.mark.parametrize('route', ['local', 'direct', 'alias', 'reexport', 'star'])
@pytest.mark.parametrize('tainted', [False, True])
def test_replacements_use_the_supplied_value_and_match_python(tmp_path, setup, route, tainted):
    client, rules = install(tmp_path, setup, route, call='choose(source())' if tainted else 'choose(0)')
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert result.report.at('/complete'), result.report.at('/cutoffs')
    assert bool(result.witnesses) is tainted and not result.initialization.sites
    assert observe(tmp_path) == {'result': 17 if tainted else 0, 'raised': None,
                                'events': [['source', 17], ['sink', 17]] if tainted else [['sink', 0]]}
    choose = next(item for item in result.summaries.functions if item.function.endswith('choose'))
    assert choose.return_parameters == (0,) and choose.signature[0].default is not None


@pytest.mark.parametrize('setup', SETUPS)
@pytest.mark.parametrize('route', ['local', 'direct'])
def test_omitted_objects_remain_unchecked_even_when_the_parameter_is_unused(tmp_path, setup, route):
    client, rules = install(tmp_path, setup, route, call='choose()', body='return 0')
    result = analyze(client, rules, 'entry', imports=route != 'local')
    assert not result.report.at('/complete') and not result.initialization.sites
    assert any(item.startswith('unsupported-default-value:') for item in result.report.at('/cutoffs'))
    assert observe(tmp_path) == {'result': 0, 'raised': None, 'events': [['sink', 0]]}


@pytest.mark.parametrize('parameters,call,reaches,complete', [
    ('value=marker, /', 'choose(source())', True, True),
    ('*, value=marker', 'choose(value=source())', True, True),
    ('value=marker, *items, **options', 'choose(source())', True, True),
    ('value=LIMIT, /, *, other=marker', 'choose(other=source())', False, True),
    ('value=LIMIT, /, *, other=marker', 'choose(source(), other=0)', True, True),
    ('value=LIMIT, /, *, other=marker', 'choose(source())', False, False),
    ('value=marker, /', 'choose(value=source())', False, False),
    ('*, value=marker', 'choose(source())', False, False),
    ('value=marker', 'choose(source(), value=0)', False, False),
    ('value=marker', 'choose(extra=source())', False, False),
])
def test_scalar_defaults_and_parameter_kinds_bind_each_slot_independently(tmp_path, parameters, call, reaches, complete):
    client, rules = install(tmp_path, 'LIMIT = 0\nmarker = []\n', parameters=parameters, call=call)
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') is complete
    assert bool(result.witnesses) is reaches
    if complete:
        assert observe(tmp_path)['result'] == (17 if reaches else 0)
    else:
        assert result.report.at('/cutoffs')


@pytest.mark.parametrize('default', ['missing', 'marker[0]', 'marker.__class__', 'source()', 'dict()'])
def test_an_explicit_argument_does_not_hide_unchecked_definition_work(tmp_path, default):
    client, rules = install(tmp_path, parameters='value=' + default)
    result = analyze(client, rules, 'entry')
    assert not result.report.at('/complete')
    assert any('definition-time-code' in site.reasons for site in result.initialization.sites)


def test_each_call_replaces_its_own_slots_without_leaking_origins(tmp_path):
    client, rules = install(tmp_path, body='return 0')
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('return 0', 'return value')
                       .replace('return sink(choose(source()))', 'choose(source())\n    return sink(choose(0))'))
    result = analyze(client, rules, 'entry')
    assert result.report.at('/complete') and not result.witnesses
    assert observe(tmp_path)['events'] == [['source', 17], ['sink', 0]]


def test_saved_answers_invalidate_when_a_replacement_is_removed(tmp_path):
    client, rules = install(tmp_path, route='direct')
    store = MemoryObjectStore()
    cache = FlowCache(store)
    first = analyze(client, rules, 'entry', cache, imports=True)
    assert first.report.at('/complete') and first.witnesses
    cache = FlowCache.restore(store, cache.persist())
    assert analyze(client, rules, 'entry', cache, imports=True).reused
    subject = tmp_path / 'subject.py'
    subject.write_text(subject.read_text().replace('choose(source())', 'choose()'))
    changed = analyze(client, rules, 'entry', cache, imports=True)
    assert not changed.reused and not changed.report.at('/complete')
    assert evidence(changed) == evidence(analyze(client, rules, 'entry', imports=True))


def test_saved_reports_require_the_current_replacement_rules(tmp_path):
    client, rules = install(tmp_path)
    value = copy.deepcopy(analyze(client, rules, 'entry').report.to_data())
    old = 'ASCII positional-only, positional-or-keyword and keyword-only slots; required or immutable scalar defaults.'
    for contract in (value['call_binding'], value['inputs']['call_binding']):
        contract['parameters'] = old
    value['input_digest'] = hashlib.sha256(json.dumps(value['inputs'], sort_keys=True,
        ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
    with pytest.raises(FrRuntimeError, match='unsupported call binding'):
        FunctionSummaries.from_report(FrReport(value, ()))


def test_the_complete_tutorial_runs_before_and_after_removing_the_argument(tmp_path):
    root = Path(__file__).resolve().parents[3]
    guide = (root / 'docs/replace-python-defaults.md').read_text()
    programs = re.findall(r'```python\n(.*?)```', guide, re.S)
    assert len(programs) == 4
    for name, source in zip(('helpers.py', 'app.py', 'check_example.py', 'run_example.py'), programs):
        (tmp_path / name).write_text(source)
    (tmp_path / 'rules.json').write_text(re.findall(r'```json\n(.*?)```', guide, re.S)[0])
    environment = dict(os.environ, PATH=str(root / 'target/debug') + os.pathsep + os.environ['PATH'],
                       PYTHONPATH=str(root / 'sdk/python/src'))
    def run(name):
        result = subprocess.run([sys.executable, name], cwd=tmp_path, env=environment,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        return result.stdout
    assert run('check_example.py') == 'Finished: True\nPossible input connection: True\nUnfinished reasons: []\n'
    assert run('run_example.py') == '17\n'
    app = tmp_path / 'app.py'
    app.write_text(app.read_text().replace('choose(read_input())', 'choose()'))
    after = run('check_example.py')
    assert 'Finished: False' in after and 'unsupported-default-value:helpers.py::choose' in after
    assert run('run_example.py') == '[]\n'
