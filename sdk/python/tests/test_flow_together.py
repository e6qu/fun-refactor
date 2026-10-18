"""Compare shared setup with independent answers and ordinary Python behavior."""
import json
from pathlib import Path
import re
import subprocess
import sys

import pytest

from fr_ir.flow_together import FlowTogether, analyze_together
from fr_ir.runtime import FrReport, FrRuntimeError
from test_flow_summaries import analyze, evidence, workspace


SETUP = "table = {}\ndef register():\n    table['label'] = 'ready'\nregister()\n"
HELPERS = 'def relay(value):\n    return value\ndef discard(value):\n    return 0\n'
ENTRIES = ('def forwards():\n    return sink(relay(source()))\n'
           'def stopped():\n    return sink(discard(source()))\n')


def install(tmp_path, route='local', setup=SETUP):
    client, rules = workspace(tmp_path)
    if route == 'local':
        source = setup + HELPERS
    else:
        (tmp_path / 'helpers.py').write_text(setup + HELPERS)
        source = 'from helpers import relay, discard\n'
        if route == 'reexport':
            (tmp_path / 'bridge.py').write_text('from helpers import relay, discard\n')
            source = 'from bridge import relay, discard\n'
        elif route == 'star':
            (tmp_path / 'helpers.py').write_text("__all__ = ['relay', 'discard']\n" + setup + HELPERS)
            source = 'from helpers import *\n'
    (tmp_path / 'subject.py').write_text(source + ENTRIES)
    return client, rules


def together(client, rules, names=('forwards', 'stopped'), **options):
    handles = [client.project('find', name).definition_target().handle for name in names]
    return analyze_together(client, handles, rules=rules, steps=options.pop('steps', 4096),
                            context='html', max_bytes=options.pop('max_bytes', 1_048_576), **options)


def observe(tmp_path):
    script = ("import sys,json,builtins; sys.path.insert(0,sys.argv[1]); "
              "builtins.source=lambda:17; builtins.sink=lambda value:value; import subject; "
              "print(json.dumps([subject.forwards(),subject.stopped()]))")
    result = subprocess.run([sys.executable, '-I', '-B', '-c', script, str(tmp_path)],
                            capture_output=True, text=True, check=True, timeout=20)
    return json.loads(result.stdout)


@pytest.mark.parametrize('route', ['local', 'helper', 'reexport', 'star'])
@pytest.mark.parametrize('names', [('forwards', 'stopped'), ('stopped', 'forwards')])
def test_shared_startup_matches_independent_answers_and_python(tmp_path, route, names):
    client, rules = install(tmp_path, route)
    result = together(client, rules, names, imports=route != 'local')
    assert result.report.at('/startup/checks') == 1
    assert len(result.dictionary_statements) == 1
    for name, answer in zip(names, result.answers):
        cold = analyze(client, rules, name, imports=route != 'local')
        assert answer.report.at('/complete') and evidence(answer) == evidence(cold)
        assert bool(answer.witnesses) is (name == 'forwards')
    assert observe(tmp_path) == [17, 0]


@pytest.mark.parametrize('setup', [SETUP + 'unknown()\n', SETUP.replace("'ready'", 'source()'),
                                    SETUP.replace('\nregister()\n', '\nregister(0)\n')])
def test_unknown_effect_keeps_every_question_unfinished(tmp_path, setup):
    client, rules = install(tmp_path, 'helper', setup)
    result = together(client, rules, imports=True)
    for answer in result.answers:
        assert not answer.report.at('/complete')
        assert 'module-effects-unchecked' in answer.report.at('/cutoffs')


def test_helper_replacement_changes_python_and_never_looks_checked(tmp_path):
    replacement = (SETUP + HELPERS + 'def replace():\n    global relay\n'
                   '    relay = discard\nreplace()\n')
    client, rules = install(tmp_path)
    (tmp_path / 'subject.py').write_text(replacement + ENTRIES)
    assert observe(tmp_path) == [0, 0]
    assert all(not answer.report.at('/complete') for answer in together(client, rules).answers)


@pytest.mark.parametrize('change', ['write', 'name', 'import', 'new-candidate'])
def test_new_requests_recheck_changed_dependencies_for_both_questions(tmp_path, change):
    client, rules = install(tmp_path, 'helper')
    first = together(client, rules, imports=True)
    helper = tmp_path / 'helpers.py'
    changes = {
        'write': SETUP.replace("'ready'", 'source()') + HELPERS,
        'name': SETUP + HELPERS.replace('return value', 'return 0'),
        'import': 'from missing import relay\n' + SETUP + HELPERS,
    }
    if change == 'new-candidate':
        (tmp_path / 'helpers.pyi').write_text('def relay(value): ...\n')
    else:
        helper.write_text(changes[change])
    result = together(client, rules, imports=True)
    assert first.report.at('/startup/input_digest') != result.report.at('/startup/input_digest')
    for name, answer in zip(('forwards', 'stopped'), result.answers):
        assert evidence(answer) == evidence(analyze(client, rules, name, imports=True))
        if change != 'name':
            assert not answer.report.at('/complete')
    if change == 'name':
        assert observe(tmp_path) == [0, 0] and not result.answers[0].witnesses


def test_all_questions_share_one_step_budget(tmp_path):
    client, rules = install(tmp_path)
    one = analyze(client, rules, 'forwards')
    used = one.report.at('/execution/analysis_steps')
    assert used > 0
    result = together(client, rules, steps=used)
    assert result.report.at('/budget/used') == used
    assert result.answers[1].report.at('/execution/analysis_steps') == 0
    assert not result.answers[1].report.at('/complete')
    assert 'step-budget' in result.answers[1].report.at('/cutoffs')
    assert sum(answer.report.at('/budget/used') for answer in result.answers) <= used


def test_combined_output_has_one_byte_budget(tmp_path):
    client, rules = install(tmp_path)
    sizes = [len(json.dumps(analyze(client, rules, name).report.to_data(),
                           ensure_ascii=False, separators=(',', ':')).encode())
             for name in ('forwards', 'stopped')]
    with pytest.raises(FrRuntimeError, match='combined response exceeds byte budget'):
        together(client, rules, max_bytes=max(sizes) + 1024)


@pytest.mark.parametrize('kind', ['duplicate', 'other-file', 'too-many', 'singleton'])
def test_questions_need_distinct_functions_in_one_file(tmp_path, kind):
    client, rules = install(tmp_path)
    names = {'duplicate': ('forwards', 'forwards'), 'other-file': ('forwards', 'other'),
             'too-many': tuple('f' + str(i) for i in range(17)), 'singleton': ('forwards',)}[kind]
    (tmp_path / 'other.py').write_text('def other():\n    return 0\n')
    if kind == 'too-many':
        (tmp_path / 'subject.py').write_text(''.join(f'def {name}():\n    return 0\n' for name in names))
    with pytest.raises(FrRuntimeError):
        together(client, rules, names)


@pytest.mark.parametrize('kind', ['count', 'work', 'budget', 'inputs', 'digest', 'write', 'names', 'complete'])
def test_altered_group_reports_are_rejected(tmp_path, kind):
    client, rules = install(tmp_path)
    value = together(client, rules).report.to_data()
    if kind == 'count':
        value['startup']['checks'] = True
    elif kind == 'work':
        value['answers'][1]['execution']['analysis_steps'] += 1
    elif kind == 'budget':
        value['budget']['used'] = value['budget']['steps'] + 1
    elif kind == 'inputs':
        value['answers'][1]['inputs']['source']['digest'] = '0' * 64
    elif kind == 'digest':
        value['startup']['input_digest'] = '0' * 64
    elif kind == 'write':
        value['startup']['dictionary_statements'][0]['revision'] = '0' * 64
    elif kind == 'names':
        value['startup']['names'].append(value['startup']['names'][0])
    else:
        value['answers'][1]['complete'] = 'true'
    with pytest.raises(FrRuntimeError):
        FlowTogether.from_report(FrReport(value, ()))


def test_complete_lesson_runs_both_checker_and_python(tmp_path):
    import os
    root = Path(__file__).resolve().parents[3]
    guide = (root / 'docs/check-python-together.md').read_text()
    programs = re.findall(r'```python\n(.*?)```', guide, re.S)
    assert len(programs) == 4
    for name, source in zip(('helpers.py', 'app.py', 'check_example.py', 'run_example.py'),
                            programs):
        (tmp_path / name).write_text(source)
    (tmp_path / 'rules.json').write_text(re.findall(r'```json\n(.*?)```', guide, re.S)[0])
    environment = dict(os.environ, PATH=str(root / 'target/debug') + os.pathsep + os.environ['PATH'],
                       PYTHONPATH=str(root / 'sdk/python/src'))
    def run(name):
        result = subprocess.run([sys.executable, '-B', name], cwd=tmp_path, env=environment,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        return result.stdout
    assert run('check_example.py') == ('Startup checks: 1\n'
        'forwards finished: True possible input connection: True\n'
        'stopped finished: True possible input connection: False\n')
    assert run('run_example.py') == 'ready\n17\n0\n'
    with (tmp_path / 'helpers.py').open('a') as output:
        output.write('\nmissing()\n')
    assert run('check_example.py') == ('Startup checks: 1\n'
        'forwards finished: False possible input connection: True\n'
        'stopped finished: False possible input connection: False\n')
