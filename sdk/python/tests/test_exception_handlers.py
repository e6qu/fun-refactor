"""Check explicit error dispatch against Python and repair an original-import application."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_summaries import FunctionSummaries
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError

ROOT = Path(__file__).resolve().parents[3]
CASES = json.loads((ROOT / 'tests/agent-eval/python-handlers/cases.json').read_text())


def workspace(root, case, renamed):
    source = case['helpers'] + 'def entry():\n'
    source += ''.join('    ' + line + '\n' for line in case['body'].splitlines())
    names = ('load_customer', 'publish', 'close_task', 'customer_entry') if renamed else ('read_user', 'send', 'finish', 'entry')
    if renamed:
        for old, new in zip(('read_user', 'send', 'finish', 'entry'), names, strict=True):
            source = source.replace(old, new)
    path = root / ('moved/customer.py' if renamed else 'subject.py')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(source)
    rules = root / 'rules.json'
    rules.write_text(json.dumps({'version': 'handlers', 'sources': [names[0]], 'sinks': [names[1]],
        'propagators': [names[2]], 'sanitizers': {}}))
    return path, rules, FrClient(root, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144), names


def analyze(client, rules, cache, name, standards, steps):
    handle = client.project('find', name).definition_target().handle
    return cache.analyze(client, handle, standard_library=standards, rules=rules, context='html',
        steps=steps, depth=8, max_bytes=262144, summaries=True, imports=True)


@pytest.mark.parametrize('renamed', [False, True], ids=['original', 'renamed-and-moved'])
@pytest.mark.parametrize('case', CASES, ids=[case['id'] for case in CASES])
def test_error_handlers_match_independent_python_outcomes(tmp_path, case, renamed):
    path, rules, client, names = workspace(tmp_path, case, renamed)
    driver = ('import json, runpy\nsent = []\ncleanups = []\n'
        'def read(): return "Customer"\n'
        'def send(value): sent.append(value); return value\n'
        'def finish(): cleanups.append("cleanup"); return "PUBLIC"\n'
        f'namespace = runpy.run_path({str(path)!r}, init_globals={{{names[0]!r}: read, {names[1]!r}: send, {names[2]!r}: finish}})\n'
        'returned = None\nraised = None\ntry:\n'
        f'    returned = namespace[{names[3]!r}]()\n'
        'except BaseException as error:\n    raised = type(error).__name__\n'
        'print(json.dumps({"returned": returned, "sent": sent, "raised": raised, "cleanups": len(cleanups)}))\n')
    run = subprocess.run([sys.executable, '-I', '-B', '-c', driver], capture_output=True, text=True, timeout=10)
    assert run.returncode == 0, run.stderr
    assert json.loads(run.stdout) == {key: case[key] for key in ('returned', 'sent', 'raised', 'cleanups')}
    result = analyze(client, rules, FlowCache(MemoryObjectStore()), names[3], ('builtins',), 4096)
    data = result.report.to_data()
    assert data['complete'] is case['complete'], data['cutoffs']
    assert bool(result.witnesses) is case['witness']
    if case['normal'] is not None:
        assert data['completion'] == {'normal_return': case['normal'], 'may_raise': case['may_raise']}
    assert result.completion_control.exception_classes == ('ValueError', 'TypeError', 'Exception', 'BaseException')
    assert result.completion_control.block_limit == 512
    assert not result.runtime_effects.exception_safety_established
    assert not result.runtime_effects.shared_mutable_values_supported
    assert result.graphs


def test_exception_classes_keep_their_own_value_origins_and_rebind_every_saved_trace(tmp_path):
    case = {'helpers': 'def fail(value):\n    if value:\n        raise ValueError(value)\n    raise TypeError("PUBLIC")\n',
        'body': 'return fail(read_user())'}
    path, rules, client, names = workspace(tmp_path, case, False)
    store = MemoryObjectStore()
    cache = FlowCache(store)
    first = analyze(client, rules, cache, names[3], ('builtins',), 4096)
    assert first.report.at('/complete')
    errors = {error.kind: error.traces for error in first.summaries.for_function('fail').exceptional_returns}
    assert errors['ValueError'] and not errors['TypeError']
    for value, expected in [('Customer', 'ValueError'), ('', 'TypeError')]:
        script = f'import runpy\nf = runpy.run_path({str(path)!r})["fail"]\ntry:\n    f({value!r})\nexcept Exception as e:\n    print(type(e).__name__)'
        run = subprocess.run([sys.executable, '-I', '-B', '-c', script], capture_output=True, text=True, timeout=10)
        assert run.returncode == 0 and run.stdout.strip() == expected
    cache = FlowCache.restore(store, cache.persist())
    (tmp_path / 'unrelated.py').write_text('def other():\n    return 0\n')
    reused = analyze(client, rules, cache, names[3], ('builtins',), 4096)
    fresh = analyze(client, rules, FlowCache(MemoryObjectStore()), names[3], ('builtins',), 4096)
    assert reused.reused
    for field in ('exceptional_returns', 'function_summaries', 'control_flow'):
        assert reused.report.at('/' + field) == fresh.report.at('/' + field)
    forged = copy.deepcopy(first.report.to_data())
    summary = forged['function_summaries']['functions']['fail']['exceptional_returns']
    summary['PretendError'] = summary.pop('ValueError')
    with pytest.raises(FrRuntimeError, match='exception class'):
        FunctionSummaries.from_report(FrReport(forged, ()))


def test_handler_edit_expires_a_saved_answer_and_preserves_step_limits(tmp_path):
    case, = [case for case in CASES if case['id'] == 'nonmatching-type-error']
    path, rules, client, names = workspace(tmp_path, case, False)
    cache = FlowCache(MemoryObjectStore())
    before = analyze(client, rules, cache, names[3], ('builtins',), 4096)
    assert before.report.at('/complete') and not before.witnesses
    assert analyze(client, rules, cache, names[3], ('builtins',), 4096).reused
    path.write_text(path.read_text().replace('except TypeError:', 'except ValueError:'))
    after = analyze(client, rules, cache, names[3], ('builtins',), 4096)
    assert after.report.at('/complete') and after.witnesses and not after.reused
    limited = analyze(client, rules, cache, names[3], ('builtins',), 1)
    assert not limited.report.at('/complete') and 'step-budget' in limited.report.at('/cutoffs')


def test_fr_repairs_the_handler_through_original_boltons_imports_and_replays_its_patch(tmp_path):
    archive = ROOT / 'tests/agent-eval/python-repositories/boltons.tar.gz'
    with tarfile.open(archive) as bundle:
        bundle.extractall(tmp_path / 'upstream', filter='data')
    root = tmp_path / 'upstream/boltons-55dfe5077bcb5fa76611b5f2a557ea6c442ad87c'
    original = ROOT / 'tests/fixtures/python-handlers'
    shutil.copyfile(original / 'app.py', root / 'app.py')
    library = root / 'boltons/mathutils.py'
    library_before = hashlib.sha256(library.read_bytes()).hexdigest()
    (root / '.fr').mkdir()
    (root / 'artifacts').mkdir()
    shutil.copyfile(original / 'oracle.py', root / '.fr/oracle.py')
    oracle = [sys.executable, '-B', '.fr/oracle.py', '.']
    before = subprocess.run(oracle, cwd=root, capture_output=True, text=True, timeout=10)
    assert before.returncode != 0 and 'ValueError' in before.stderr
    for argv in (['git', 'init', '-q'], ['git', 'add', '.']):
        subprocess.run(argv, cwd=root, check=True, capture_output=True, timeout=10)
    (root / '.fr/checks.json').write_text(json.dumps({'schema': 1, 'checks': [
        {'name': 'syntax', 'argv': [sys.executable, '-B', '-c', "import ast; ast.parse(open('app.py').read())"],
            'cwd': '.', 'timeout_seconds': 10, 'covers': ['application syntax']},
        {'name': 'behavior', 'argv': oracle, 'cwd': '.', 'timeout_seconds': 10,
            'identity_files': ['.fr/oracle.py'], 'covers': ['original Boltons; valid bounds; four recovery labels; unexpected TypeError']}
    ]}))
    rules = root / 'rules.json'
    rules.write_text(json.dumps({'version': 'handlers', 'sources': ['read_user'], 'sinks': ['send'], 'sanitizers': {}}))
    client = FrClient(root, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    standards = ('builtins', 'math', 'bisect', 'binascii')
    cache = FlowCache(MemoryObjectStore())
    initial = analyze(client, rules, cache, 'entry', standards, 4096)
    assert initial.report.at('/complete') and not initial.witnesses, initial.report.at('/cutoffs')
    selected = client.project('find', 'prepare', '--in', 'app.py', '--source', '--bytes', '4096')
    body = "try:\n    clamp(0, lower, upper)\nexcept ValueError:\n    return value\nreturn 'ACCEPTED'"
    change = TaskChange([], [TaskTarget('catch-the-raised-error', selected.definition_target().handle,
        'replace-body', fragment=body)], {'files-changed': 1, 'paths-changed': ['app.py']}, ['syntax'],
        TaskDelivery(check_original=True, compact_success=True, exercise_reversal=True,
            patch='artifacts/handler.patch', check_output_bytes=2048), acceptance_checks=['behavior'])
    review = client.review(change)
    assert (root / 'app.py').read_bytes() == (original / 'app.py').read_bytes()
    delivered = client.execute(review)
    assert delivered.at('/workflow/passed') is True
    assert all(stage['status'] == 'passed' for stage in delivered.at('/workflow/stages'))
    assert any(stage['stage'] == 'undo' for stage in delivered.at('/workflow/stages'))
    after = analyze(client, rules, cache, 'entry', standards, 4096)
    assert after.report.at('/complete') and after.witnesses and not after.reused
    assert hashlib.sha256(library.read_bytes()).hexdigest() == library_before
    receiver = tmp_path / 'receiver'
    shutil.copytree(root / 'boltons', receiver / 'boltons')
    shutil.copyfile(original / 'app.py', receiver / 'app.py')
    subprocess.run(['git', 'apply', str(root / 'artifacts/handler.patch')], cwd=receiver,
        check=True, capture_output=True, timeout=10)
    subprocess.run([sys.executable, '-B', str(original / 'oracle.py'), str(receiver)], cwd=receiver,
        check=True, capture_output=True, timeout=10)
