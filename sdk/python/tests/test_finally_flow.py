"""Compare cleanup paths with ordinary Python, including overrides and nested exits."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

import pytest

from fr_ir.context import MemoryObjectStore
from fr_ir.flow import FlowCache
from fr_ir.flow_completion import CompletionControl
from fr_ir.runtime import FrClient, FrReport, FrRuntimeError
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget

ROOT = Path(__file__).resolve().parents[3]
CASES = json.loads((ROOT / 'tests/agent-eval/python-finally/cases.json').read_text())


def program(case, renamed):
    result = case['helpers'] + 'def entry():\n'
    result += ''.join('    ' + line + '\n' for line in case['body'].splitlines())
    if renamed:
        for old, new in [('read_user', 'load_customer'), ('send', 'publish'), ('entry', 'customer_entry'),
                         ('relay', 'forward_label'), ('stop', 'reject_label'), ('finish', 'close_task')]:
            result = result.replace(old, new)
    return result


def workspace(tmp_path, case, renamed):
    relative = 'moved/customer.py' if renamed else 'subject.py'
    source = tmp_path / relative
    source.parent.mkdir(parents=True, exist_ok=True)
    source.write_text(program(case, renamed))
    rules = tmp_path / 'rules.json'
    rules.write_text(json.dumps({'version': 'finally', 'sources': ['load_customer' if renamed else 'read_user'],
        'sinks': ['publish' if renamed else 'send'], 'propagators': ['close_task' if renamed else 'finish'], 'sanitizers': {}}))
    client = FrClient(tmp_path, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    return source, rules, client


def analyze(client, rules, cache, renamed, steps):
    name = 'customer_entry' if renamed else 'entry'
    handle = client.project('find', name).definition_target().handle
    return cache.analyze(client, handle, standard_library=('builtins',), rules=rules, context='html',
        steps=steps, depth=8, max_bytes=262144, summaries=True, imports=True)


@pytest.mark.parametrize('renamed', [False, True], ids=['original', 'renamed-and-moved'])
@pytest.mark.parametrize('case', CASES, ids=[case['id'] for case in CASES])
def test_cleanup_matches_independent_python_and_preserves_explicit_limits(tmp_path, case, renamed):
    source, rules, client = workspace(tmp_path, case, renamed)
    source_name, sink_name, finish_name, entry_name = (('load_customer', 'publish', 'close_task', 'customer_entry')
        if renamed else ('read_user', 'send', 'finish', 'entry'))
    driver = ('import json, runpy\nsent = []\ncleanups = []\n'
        'def read(): return "Customer"\n'
        'def send(value): sent.append(value); return value\n'
        'def finish(): cleanups.append("cleanup"); return "PUBLIC"\n'
        f'namespace = runpy.run_path({str(source)!r}, init_globals={{{source_name!r}: read, {sink_name!r}: send, {finish_name!r}: finish}})\n'
        'returned = None\nraised = None\ntry:\n'
        f'    returned = namespace[{entry_name!r}]()\n'
        'except Exception as error:\n    raised = type(error).__name__\n'
        'print(json.dumps({"returned": returned, "sent": sent, "raised": raised, "cleanups": len(cleanups)}))\n')
    run = subprocess.run([sys.executable, '-I', '-B', '-c', driver], capture_output=True, text=True, timeout=10)
    assert run.returncode == 0, run.stderr
    observed = json.loads(run.stdout)
    assert observed == {key: case[key] for key in ('returned', 'sent', 'raised', 'cleanups')}
    result = analyze(client, rules, FlowCache(MemoryObjectStore()), renamed, 4096)
    data = result.report.to_data()
    assert data['complete'] is case['complete'], data['cutoffs']
    assert bool(result.witnesses) is case['witness']
    if case['normal'] is not None:
        assert data['completion'] == {'normal_return': case['normal'], 'may_raise': case['may_raise']}
        if not case['normal']:
            assert not data['returns']
        if not case['may_raise']:
            assert not data['exceptional_returns']
    assert result.completion_control.block_limit == 512
    assert not result.runtime_effects.exception_safety_established
    assert not result.runtime_effects.shared_mutable_values_supported
    assert result.graphs
    if case['id'] == 'handler-matching-stays-incomplete':
        assert any(site.kind == 'exception-handler' for site in result.runtime_effects.sites)
    if case['id'] == 'implicit-exception-is-not-an-explicit-raise':
        assert any(site.kind == 'implicit-exception' for site in result.runtime_effects.sites)


def test_saved_cleanup_answer_expires_when_cleanup_changes_and_rebinds_when_unrelated_code_changes(tmp_path):
    case, = [case for case in CASES if case['id'] == 'capture-before-reassignment']
    source, rules, client = workspace(tmp_path, case, False)
    cache = FlowCache(MemoryObjectStore())
    first = analyze(client, rules, cache, False, 4096)
    assert first.witnesses and first.report.at('/complete')
    assert analyze(client, rules, cache, False, 4096).reused
    (tmp_path / 'unrelated.py').write_text('def another():\n    return 0\n')
    reused = analyze(client, rules, cache, False, 4096)
    fresh = analyze(client, rules, FlowCache(MemoryObjectStore()), False, 4096)
    assert reused.reused and reused.witnesses == fresh.witnesses
    assert reused.graphs == fresh.graphs
    source.write_text(source.read_text().replace("value = 'PUBLIC'", "return 'PUBLIC'"))
    changed = analyze(client, rules, cache, False, 4096)
    assert not changed.reused and changed.report.at('/complete') and not changed.witnesses
    exhausted = analyze(client, rules, cache, False, 1)
    assert not exhausted.reused and not exhausted.report.at('/complete')
    assert 'step-budget' in exhausted.report.at('/cutoffs')


@pytest.mark.parametrize('location', ['report', 'inputs', 'both'])
def test_forged_cleanup_contract_is_rejected_by_both_clients_even_with_new_digests(tmp_path, location):
    case = CASES[0]
    _, rules, client = workspace(tmp_path, case, False)
    first = analyze(client, rules, FlowCache(MemoryObjectStore()), False, 4096)
    data = copy.deepcopy(first.report.to_data())
    if location in ('report', 'both'):
        data['completion_control']['implicit_exceptions'] = True
    if location in ('inputs', 'both'):
        data['inputs']['completion_control']['implicit_exceptions'] = True
    encoded = lambda value: json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode()
    data['input_digest'] = hashlib.sha256(encoded(data['inputs'])).hexdigest()
    with pytest.raises(FrRuntimeError, match='completion contract'):
        CompletionControl.from_report(FrReport(data, ()))
    saved = tmp_path / 'forged-answer'
    saved.write_bytes(encoded(data))
    handle = client.project('find', 'entry').definition_target().handle
    with pytest.raises(FrRuntimeError, match='completion contract'):
        client.project('dataflow', handle, '--summaries', '--imports', '--standard-library', 'builtins',
            '--rules', str(rules), '--context', 'html', '--steps', '4096', '--depth', '8', '--bytes', '262144',
            '--reuse', str(saved), '--reuse-digest', hashlib.sha256(encoded(data)).hexdigest())


def test_fr_repairs_the_imported_helper_checks_cleanup_and_replays_the_patch(tmp_path):
    original = ROOT / 'tests/fixtures/python-finally'
    root = tmp_path / 'working'
    root.mkdir()
    for name in ('app.py', 'delivery.py'):
        shutil.copyfile(original / name, root / name)
    (root / '.fr').mkdir()
    (root / 'artifacts').mkdir()
    shutil.copyfile(original / 'oracle.py', root / '.fr/oracle.py')
    oracle = [sys.executable, '-B', '.fr/oracle.py', '.']
    before = subprocess.run(oracle, cwd=root, capture_output=True, text=True, timeout=10)
    assert before.returncode != 0 and "('Customer', 'PUBLIC')" in before.stderr
    for argv in (['git', 'init', '-q'], ['git', 'add', '.']):
        subprocess.run(argv, cwd=root, check=True, capture_output=True, timeout=10)
    (root / '.fr/checks.json').write_text(json.dumps({'schema': 1, 'checks': [
        {'name': 'syntax', 'argv': [sys.executable, '-B', '-c',
            "import ast; ast.parse(open('delivery.py').read()); ast.parse(open('app.py').read())"],
            'cwd': '.', 'timeout_seconds': 10, 'covers': ['both Python files']},
        {'name': 'behavior', 'argv': oracle, 'cwd': '.', 'timeout_seconds': 10,
            'identity_files': ['.fr/oracle.py'], 'covers': ['four customer labels; cleanup once; cleanup failure preserved']}
    ]}))
    rules = root / 'rules.json'
    rules.write_text(json.dumps({'version': 'finally', 'sources': ['read_user'], 'sinks': ['send'],
        'propagators': ['finish'], 'sanitizers': {}}))
    client = FrClient(root, executable=str(ROOT / 'target/debug/fr'), max_output_bytes=262144)
    cache = FlowCache(MemoryObjectStore())
    entry = client.project('find', 'entry').definition_target().handle
    result = cache.analyze(client, entry, standard_library=('builtins',), rules=rules, context='html',
        steps=4096, depth=8, max_bytes=262144, summaries=True, imports=True)
    assert result.report.at('/complete') and not result.witnesses
    selected = client.project('find', 'prepare', '--in', 'delivery.py', '--source', '--bytes', '4096')
    handle = selected.definition_target().handle
    body = 'try:\n    return value\nfinally:\n    finish()'
    change = TaskChange([], [TaskTarget('keep-customer', handle, 'replace-body', fragment=body)],
        {'files-changed': 1, 'paths-changed': ['delivery.py']}, ['syntax'],
        TaskDelivery(check_original=True, compact_success=True, exercise_reversal=True,
            patch='artifacts/cleanup.patch', check_output_bytes=2048), acceptance_checks=['behavior'])
    review = client.review(change)
    assert (root / 'delivery.py').read_bytes() == (original / 'delivery.py').read_bytes()
    delivered = client.execute(review)
    assert delivered.at('/workflow/passed') is True
    assert all(stage['status'] == 'passed' for stage in delivered.at('/workflow/stages'))
    assert any(stage['stage'] == 'undo' for stage in delivered.at('/workflow/stages'))
    current = client.project('find', 'entry').definition_target().handle
    refreshed = cache.analyze(client, current, standard_library=('builtins',), rules=rules, context='html',
        steps=4096, depth=8, max_bytes=262144, summaries=True, imports=True)
    assert not refreshed.reused and refreshed.report.at('/complete') and refreshed.witnesses
    receiver = tmp_path / 'receiver'
    receiver.mkdir()
    for name in ('app.py', 'delivery.py'):
        shutil.copyfile(original / name, receiver / name)
    subprocess.run(['git', 'apply', str(root / 'artifacts/cleanup.patch')], cwd=receiver,
        check=True, capture_output=True, timeout=10)
    subprocess.run([sys.executable, '-B', str(original / 'oracle.py'), str(receiver)], cwd=receiver,
        check=True, capture_output=True, timeout=10)
    assert (receiver / 'app.py').read_bytes() == (original / 'app.py').read_bytes()
